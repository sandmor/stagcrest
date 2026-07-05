use std::ops::RangeInclusive;
use std::path::PathBuf;

use indicatif::ProgressBar;
use rayon::prelude::*;
use stagcrest_circuit::{init_circuit_blocks, CircuitWorld};
use stagcrest_mod_server::{world_chunk_y_bounds, WorldGenState, WorldSeed};
use stagcrest_protocol::{ChunkPos, CHUNK_SIZE};
use stagcrest_storage::ChunkStorage;
use stagcrest_world::World;
use thiserror::Error;

use crate::chunk_gen::{apply_density_batch, apply_pass2_decorate};
use crate::map_tile_maintenance::{with_rayon_pool, MapTileDirtySet};
use crate::offline_bootstrap::{bootstrap_offline, BootstrapError, WorldSeedPolicy};
use crate::persistence::{persist_chunk, persist_evicted_chunk, PersistError};

#[derive(Debug, Error)]
pub enum BuildMapError {
    #[error("bootstrap: {0}")]
    Bootstrap(#[from] BootstrapError),
    #[error("persist: {0}")]
    Persist(#[from] PersistError),
    #[error("storage: {0}")]
    Storage(#[from] stagcrest_storage::StorageError),
    #[error("map tile: {0}")]
    MapTile(#[from] crate::map_tile_maintenance::MapTileError),
    #[error("{0}")]
    Message(String),
}

pub struct BuildMapConfig {
    pub world_name: String,
    pub mods_root: PathBuf,
    pub center_x: i32,
    pub center_z: i32,
    pub radius_chunks: i32,
    pub seed: Option<u64>,
    pub force: bool,
    pub jobs: Option<usize>,
}

pub struct BuildMapReport {
    pub skipped: usize,
    pub generated: usize,
    pub map_tiles_rebuilt: usize,
}

fn floor_div(a: i32, b: i32) -> i32 {
    if a >= 0 {
        a / b
    } else {
        (a - (b - 1)) / b
    }
}

/// Enumerate all chunk positions in a horizontal circle with full vertical span.
pub fn iter_circle_chunk_positions(
    center_block_x: i32,
    center_block_z: i32,
    radius_chunks: i32,
    y_bounds: RangeInclusive<i32>,
) -> Vec<ChunkPos> {
    let center_cx = floor_div(center_block_x, CHUNK_SIZE);
    let center_cz = floor_div(center_block_z, CHUNK_SIZE);
    let r2 = i64::from(radius_chunks) * i64::from(radius_chunks);
    let mut out = Vec::new();

    for cx in (center_cx - radius_chunks)..=(center_cx + radius_chunks) {
        for cz in (center_cz - radius_chunks)..=(center_cz + radius_chunks) {
            let dx = i64::from(cx - center_cx);
            let dz = i64::from(cz - center_cz);
            if dx * dx + dz * dz > r2 {
                continue;
            }
            for cy in y_bounds.clone() {
                out.push(ChunkPos {
                    x: cx,
                    y: cy,
                    z: cz,
                });
            }
        }
    }

    out.sort_by_key(|pos| {
        let dx = pos.x - center_cx;
        let dz = pos.z - center_cz;
        (pos.y, dx * dx + dz * dz)
    });

    out
}

fn progress_bar(len: u64, msg: &str) -> ProgressBar {
    let bar = ProgressBar::new(len);
    bar.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{bar:40} {pos}/{len} {msg} {elapsed}")
            .unwrap(),
    );
    bar.set_message(msg.to_string());
    bar
}

pub fn build_world_region(config: BuildMapConfig) -> Result<BuildMapReport, BuildMapError> {
    if config.radius_chunks < 0 {
        return Err(BuildMapError::Message(
            "radius must be >= 0 (chunk count)".into(),
        ));
    }

    let mut result = BuildMapReport {
        skipped: 0,
        generated: 0,
        map_tiles_rebuilt: 0,
    };

    with_rayon_pool(config.jobs, || {
        build_world_region_inner(&config, &mut result)
    })?;

    Ok(result)
}

fn build_world_region_inner(
    config: &BuildMapConfig,
    result: &mut BuildMapReport,
) -> Result<(), BuildMapError> {
    let seed_policy = match config.seed {
        Some(seed) => WorldSeedPolicy::Override(seed),
        None => WorldSeedPolicy::UseStored,
    };
    let mut offline = bootstrap_offline(&config.world_name, &config.mods_root, seed_policy)?;

    let seed = offline.session.meta.world_seed;
    let air = offline.worldgen.air;
    let column_blocks = offline.worldgen.column_blocks;
    let biomes = offline.worldgen.biomes.clone();
    let registry = offline.worldgen.registry.clone();
    let map_ctx = offline.worldgen.map_ctx();
    let y_chunks = offline.worldgen.map_y_chunks();

    let mut terrain = WorldGenState::new(WorldSeed(seed));
    terrain.apply_river_config(biomes.river_config());
    let generator = terrain.generator().clone();

    let y_bounds = world_chunk_y_bounds(terrain.config());
    let y_count = y_bounds.clone().count();
    let cap = ((2 * config.radius_chunks + 3).pow(2) as usize) * y_count + 256;
    let mut world = World::with_lru_capacity(cap, air);

    let mut circuit = CircuitWorld::new();
    circuit.set_tick(offline.session.meta.circuit_tick);
    init_circuit_blocks(&mut circuit, &world, &registry);

    let all_positions = iter_circle_chunk_positions(
        config.center_x,
        config.center_z,
        config.radius_chunks,
        y_bounds,
    );

    let mut to_generate: Vec<ChunkPos> = Vec::new();
    for pos in all_positions {
        if !config.force && offline.session.storage.as_ref().contains(pos) {
            result.skipped += 1;
            continue;
        }
        to_generate.push(pos);
    }

    if to_generate.is_empty() {
        println!(
            "build-map: nothing to generate ({} chunks already present)",
            result.skipped
        );
        return Ok(());
    }

    println!(
        "build-map: generating {} chunks ({} skipped)",
        to_generate.len(),
        result.skipped
    );

    let pass1_bar = progress_bar(to_generate.len() as u64, "pass1");
    let density_results: Vec<_> = to_generate
        .par_iter()
        .map(|&pos| {
            let data = generator.compute_chunk_density(column_blocks, pos);
            pass1_bar.inc(1);
            data
        })
        .collect();
    pass1_bar.finish_with_message("pass1 done");

    apply_density_batch(&mut world, &mut terrain, &density_results);

    let pass2_bar = progress_bar(to_generate.len() as u64, "pass2");
    let mut map_dirty = MapTileDirtySet::default();
    let center_chunk = ChunkPos {
        x: floor_div(config.center_x, CHUNK_SIZE),
        y: 0,
        z: floor_div(config.center_z, CHUNK_SIZE),
    };

    for data in &density_results {
        apply_pass2_decorate(
            &mut world,
            &mut terrain,
            &generator,
            column_blocks,
            &biomes,
            &registry,
            air,
            data,
        );

        persist_chunk(
            &world,
            &terrain,
            &circuit,
            data.pos,
            offline.session.storage.as_ref(),
        )?;
        map_dirty.mark_chunk(data.pos);
        offline.session.stored_chunks.insert(data.pos);
        result.generated += 1;
        pass2_bar.inc(1);

        let evicted =
            world.unload_far_chunks_3d(center_chunk, config.radius_chunks + 2, y_count as i32);
        for evicted_chunk in evicted.0 {
            match persist_evicted_chunk(
                &offline.session.persistence,
                evicted_chunk,
                &terrain,
                &circuit,
                &mut offline.session.stored_chunks,
            ) {
                Ok(Some(pos)) => map_dirty.mark_chunk(pos),
                Ok(None) => {}
                Err(err) => {
                    return Err(BuildMapError::Persist(err));
                }
            }
        }
    }
    pass2_bar.finish_with_message("pass2 done");

    if !map_dirty.is_empty() {
        let tile_count = map_dirty.len();
        let map_bar = progress_bar(tile_count as u64, "map tiles");
        let rebuilt =
            map_dirty.rebuild_all(
                offline.session.storage.as_ref(),
                &map_ctx,
                &y_chunks,
                air,
                offline.block_remap.as_ref(),
            )?;
        map_bar.inc(tile_count as u64);
        map_bar.finish_with_message("map tiles done");
        result.map_tiles_rebuilt = rebuilt;
    }

    if let Err(err) = offline.session.save_meta(
        circuit.current_tick(),
        stagcrest_storage::BlockIdRemap::from_entries(
            offline
                .worldgen
                .registry
                .all_blocks()
                .map(|def| (def.id.0, def.namespaced_id.clone())),
        ),
    ) {
        tracing::warn!("failed to save world meta: {err}");
    }

    Ok(())
}
