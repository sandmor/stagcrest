use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

use stagcrest_minimap::{
    build_map_chunk_blob, collect_modified_positions, collect_world_overrides,
    compute_layer_source_revision, map_tile_has_live_edits, MapChunkBlob, MapChunkLoadInput,
    MapResolveContext,
};
use stagcrest_mod_server::{BiomeRegistry, BlockRegistry, ColormapSet, TerrainConfig};
use stagcrest_protocol::{BlockDef, BlockId, ChunkPos};
use stagcrest_storage::{InactiveChunk, RedbChunkStorage};
use stagcrest_world::World;

const MAX_MAP_GEN_PER_TICK: usize = 1;

pub struct MapRegenComplete {
    pub mx: i32,
    pub mz: i32,
    pub blob: Vec<u8>,
}

struct MapChunkJob {
    mx: i32,
    mz: i32,
    overrides: HashMap<ChunkPos, InactiveChunk>,
    modified_live: HashSet<ChunkPos>,
    y_chunks: Vec<i32>,
    ctx: Arc<MapResolveContext>,
    storage: Arc<RedbChunkStorage>,
}

pub struct MapChunkPipeline {
    pending: HashSet<(i32, i32)>,
    tx: mpsc::Sender<MapChunkJob>,
    completions: mpsc::Receiver<MapRegenComplete>,
    _worker: JoinHandle<()>,
}

impl MapChunkPipeline {
    pub fn new(
        _storage: Arc<RedbChunkStorage>,
        _ctx: Arc<MapResolveContext>,
        _y_chunks: Vec<i32>,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<MapChunkJob>();
        let (complete_tx, completions) = mpsc::channel::<MapRegenComplete>();
        let worker = thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                let input = MapChunkLoadInput {
                    storage: &job.storage,
                    world: None,
                    y_chunks: &job.y_chunks,
                    mx: job.mx,
                    mz: job.mz,
                    overrides: &job.overrides,
                    modified_live: &job.modified_live,
                };
                match build_map_chunk_blob(&input, &job.ctx) {
                    Ok(blob) => {
                        if let Err(err) = job.storage.put_map_chunk(job.mx, job.mz, &blob) {
                            tracing::error!(
                                "failed to write map chunk ({}, {}): {err}",
                                job.mx,
                                job.mz
                            );
                        } else if complete_tx
                            .send(MapRegenComplete {
                                mx: job.mx,
                                mz: job.mz,
                                blob,
                            })
                            .is_err()
                        {
                            break;
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
            pending: HashSet::new(),
            tx,
            completions,
            _worker: worker,
        }
    }

    pub fn ensure_fresh(
        &mut self,
        mx: i32,
        mz: i32,
        world: &World,
        y_chunks: &[i32],
        storage: &RedbChunkStorage,
    ) {
        if is_map_chunk_fresh(mx, mz, world, y_chunks, storage) {
            return;
        }
        self.pending.insert((mx, mz));
    }

    pub fn tick(
        &mut self,
        world: &World,
        y_chunks: &[i32],
        ctx: &Arc<MapResolveContext>,
        storage: &Arc<RedbChunkStorage>,
    ) {
        for _ in 0..MAX_MAP_GEN_PER_TICK {
            let Some((mx, mz)) = self.pending.iter().next().copied() else {
                break;
            };
            self.pending.remove(&(mx, mz));
            if is_map_chunk_fresh(mx, mz, world, y_chunks, storage) {
                continue;
            }
            let overrides = collect_world_overrides(world, mx, mz, y_chunks);
            let modified_live = collect_modified_positions(Some(world), mx, mz, y_chunks);
            let job = MapChunkJob {
                mx,
                mz,
                overrides,
                modified_live,
                y_chunks: y_chunks.to_vec(),
                ctx: Arc::clone(ctx),
                storage: Arc::clone(storage),
            };
            if self.tx.send(job).is_err() {
                tracing::error!("map chunk worker disconnected");
                self.pending.insert((mx, mz));
                break;
            }
        }
    }

    pub fn drain_completions(&mut self) -> Vec<MapRegenComplete> {
        let mut out = Vec::new();
        while let Ok(done) = self.completions.try_recv() {
            out.push(done);
        }
        out
    }
}

pub fn is_map_chunk_fresh(
    mx: i32,
    mz: i32,
    world: &World,
    y_chunks: &[i32],
    storage: &RedbChunkStorage,
) -> bool {
    if map_tile_has_live_edits(world, mx, mz, y_chunks) {
        return false;
    }
    let Some(blob) = storage.get_map_chunk(mx, mz).ok().flatten() else {
        return false;
    };
    let layers = match MapChunkBlob::peek_layers(&blob) {
        Ok(layers) => layers,
        Err(_) => return false,
    };
    let Some(layer) = layers.first() else {
        return false;
    };
    let modified_live = collect_modified_positions(Some(world), mx, mz, y_chunks);
    let current = compute_layer_source_revision(
        storage,
        mx,
        mz,
        layer.min_y,
        layer.max_y,
        y_chunks,
        &modified_live,
    );
    layer.source_revision == current
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
