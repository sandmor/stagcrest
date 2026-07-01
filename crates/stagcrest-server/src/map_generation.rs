use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

use stagcrest_minimap::{
    build_map_chunk_blob, collect_world_overrides, world_chunk_to_map_chunk, MapChunkLoadInput,
    MapResolveContext,
};
use stagcrest_mod_server::{BiomeRegistry, BlockRegistry, ColormapSet, TerrainConfig};
use stagcrest_protocol::{BlockDef, BlockId, ChunkPos};
use stagcrest_storage::{InactiveChunk, RedbChunkStorage};
use stagcrest_world::World;

const MAX_MAP_GEN_PER_TICK: usize = 1;

struct MapChunkJob {
    mx: i32,
    mz: i32,
    overrides: HashMap<ChunkPos, InactiveChunk>,
    y_chunks: Vec<i32>,
    ctx: Arc<MapResolveContext>,
    storage: Arc<RedbChunkStorage>,
}

pub struct MapChunkPipeline {
    dirty: HashSet<(i32, i32)>,
    tx: mpsc::Sender<MapChunkJob>,
    _worker: JoinHandle<()>,
}

impl MapChunkPipeline {
    pub fn new(_storage: Arc<RedbChunkStorage>, _ctx: Arc<MapResolveContext>, _y_chunks: Vec<i32>) -> Self {
        let (tx, rx) = mpsc::channel::<MapChunkJob>();
        let worker = thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                let input = MapChunkLoadInput {
                    storage: &job.storage,
                    world: None,
                    y_chunks: &job.y_chunks,
                    mx: job.mx,
                    mz: job.mz,
                    overrides: &job.overrides,
                };
                match build_map_chunk_blob(&input, &job.ctx) {
                    Ok(blob) => {
                        if let Err(err) = job.storage.put_map_chunk(job.mx, job.mz, &blob) {
                            tracing::error!(
                                "failed to write map chunk ({}, {}): {err}",
                                job.mx,
                                job.mz
                            );
                        }
                    }
                    Err(err) => {
                        tracing::error!(
                            "failed to build map chunk ({}, {}): {err}",
                            job.mx,
                            job.mz
                        );
                    }
                }
            }
        });
        Self {
            dirty: HashSet::new(),
            tx,
            _worker: worker,
        }
    }

    pub fn mark_dirty_from_chunk(&mut self, pos: ChunkPos) {
        let (mx, mz) = world_chunk_to_map_chunk(pos.x, pos.z);
        self.dirty.insert((mx, mz));
    }

    pub fn tick(
        &mut self,
        world: &World,
        y_chunks: &[i32],
        ctx: &Arc<MapResolveContext>,
        storage: &Arc<RedbChunkStorage>,
    ) {
        for _ in 0..MAX_MAP_GEN_PER_TICK {
            let Some((mx, mz)) = self.dirty.iter().next().copied() else {
                break;
            };
            self.dirty.remove(&(mx, mz));
            let overrides = collect_world_overrides(world, mx, mz, y_chunks);
            let job = MapChunkJob {
                mx,
                mz,
                overrides,
                y_chunks: y_chunks.to_vec(),
                ctx: Arc::clone(ctx),
                storage: Arc::clone(storage),
            };
            if self.tx.send(job).is_err() {
                tracing::error!("map chunk worker disconnected");
                self.dirty.insert((mx, mz));
                break;
            }
        }
    }
}

pub fn make_map_resolve_context(
    registry: &BlockRegistry,
    colormaps: &ColormapSet,
    biomes: &BiomeRegistry,
    air: BlockId,
    terrain: &TerrainConfig,
) -> MapResolveContext {
    let block_defs = collect_block_defs(registry);
    let color_ctx = stagcrest_minimap::MinimapColorCtxOwned::new(
        colormaps.grass.clone(),
        colormaps.grass_w,
        colormaps.grass_h,
        colormaps.foliage.clone(),
        colormaps.foliage_w,
        colormaps.foliage_h,
        colormaps.water.clone(),
        colormaps.water_w,
        colormaps.water_h,
    );
    MapResolveContext::new(
        block_defs,
        air,
        terrain.world_min_y,
        terrain.world_max_y,
        color_ctx,
        build_biome_climates(biomes),
    )
}

fn collect_block_defs(registry: &BlockRegistry) -> Vec<BlockDef> {
    registry
        .block_ids()
        .into_iter()
        .filter_map(|id| registry.block(id).cloned())
        .collect()
}

fn build_biome_climates(biomes: &BiomeRegistry) -> Vec<stagcrest_minimap::MinimapBiomeClimate> {
    biomes
        .biomes()
        .iter()
        .map(|b| stagcrest_minimap::MinimapBiomeClimate {
            temperature: b.climate_temp,
            downfall: b.climate_downfall,
            grass_color: b.environment.grass_color,
            foliage_color: b.environment.foliage_color,
            water_color: Some(b.environment.water_color),
        })
        .collect()
}
