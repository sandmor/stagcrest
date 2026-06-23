use crate::mesh_scheduler::{chunk_distance_sq_from_block, poll_future_now, MeshScheduler, RemeshUrgency};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
#[cfg(not(target_arch = "wasm32"))]
use bevy::tasks::AsyncComputeTaskPool;
use stagcrest_mod_host::{ChunkGenData, ColumnBlocks, DecorateSnapshot, WorldGenState};
use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos, CHUNK_SIZE};
use stagcrest_storage::{compress_stored, load_inactive_chunk, InactiveChunk, StorageError};
use std::collections::{HashMap, HashSet, VecDeque};

const MAX_GEN_IN_FLIGHT: usize = 80;
const MAX_IO_IN_FLIGHT: usize = 8;
const QUEUE_REFILL: usize = 256;

/// Vertical chunk span for streaming. Extends above terrain `y_bounds` when the
/// player flies higher so air chunks still generate instead of leaving a void.
fn stream_vertical_y_range(
    center_y: i32,
    vertical_radius: i32,
    y_bounds: &std::ops::RangeInclusive<i32>,
) -> Option<(i32, i32)> {
    let y_min = (center_y - vertical_radius).max(*y_bounds.start());
    let y_cap = (*y_bounds.end()).max(center_y);
    let y_max = (center_y + vertical_radius).min(y_cap);
    (y_min <= y_max).then_some((y_min, y_max))
}

#[derive(Resource, Clone, Copy)]
pub struct TerrainBlocks(pub ColumnBlocks);

#[derive(Resource, Clone)]
pub struct TerrainBiomes(pub stagcrest_mod_host::BiomeRegistry);

#[derive(Resource, Default)]
pub struct TerrainStreamState {
    pub center_x: i32,
    pub center_y: i32,
    pub center_z: i32,
    pub valid: bool,
}

struct InFlight {
    pos: ChunkPos,
    task: ErasedTask,
}

enum ErasedTask {
    Density(Task<ChunkGenData>),
    Decorate(Task<Vec<(BlockPos, BlockId, BlockState)>>),
    StorageLoad(Task<Result<Option<InactiveChunk>, StorageError>>),
    StoragePersist(Task<Result<(), StorageError>>),
}

#[derive(Resource, Default)]
pub struct StreamingPipeline {
    pending_generate: VecDeque<ChunkPos>,
    pending_load: VecDeque<ChunkPos>,
    pending_persist: VecDeque<(ChunkPos, Vec<u8>)>,
    deferred_decorate: VecDeque<ChunkGenData>,
    /// Pass-1 density retained until Pass-2 populate completes (survives queue pruning).
    pass2_pending: HashMap<ChunkPos, ChunkGenData>,
    in_progress: HashSet<ChunkPos>,
    gen_in_flight: usize,
    io_in_flight: usize,
    tasks: Vec<InFlight>,
    discarded: HashSet<ChunkPos>,
    integrate_terrain: Vec<ChunkGenData>,
    integrate_decorate: Vec<(ChunkPos, Vec<(BlockPos, BlockId, BlockState)>)>,
    integrate_load: Vec<(ChunkPos, Result<Option<InactiveChunk>, StorageError>)>,
    integrate_persist: Vec<(ChunkPos, Result<(), StorageError>)>,
}

impl StreamingPipeline {
    pub fn enqueue_generate(
        &mut self,
        pos: ChunkPos,
        terrain: &mut WorldGenState,
        world: &stagcrest_world::World,
    ) -> bool {
        if world.has_chunk(pos) {
            if terrain.is_chunk_generated(pos)
                || terrain.is_chunk_terrain_ready(pos)
            {
                return false;
            }
        } else {
            terrain.clear_chunk(pos);
        }
        if self.in_progress.contains(&pos)
            || self.pending_generate.iter().any(|&p| p == pos)
            || self.deferred_decorate.iter().any(|d| d.pos == pos)
        {
            return false;
        }
        self.pending_load.retain(|&p| p != pos);
        self.pending_generate.push_back(pos);
        true
    }

    pub fn enqueue_load(&mut self, pos: ChunkPos) -> bool {
        if self.in_progress.contains(&pos) || self.pending_load.iter().any(|&p| p == pos) {
            return false;
        }
        self.clear_generation_work(pos);
        self.pending_load.push_back(pos);
        true
    }

    pub fn clear_generation_work(&mut self, pos: ChunkPos) {
        self.pending_generate.retain(|&p| p != pos);
        self.deferred_decorate.retain(|d| d.pos != pos);
        self.integrate_terrain.retain(|d| d.pos != pos);
        self.pass2_pending.remove(&pos);
    }

    fn pass2_in_flight(&self, pos: ChunkPos) -> bool {
        self.tasks.iter().any(|t| {
            t.pos == pos && matches!(t.task, ErasedTask::Decorate(_))
        })
    }

    /// Re-queue terrain-ready chunks that lost their Pass-2 slot (e.g. after stream pruning).
    pub fn requeue_pass2_in_stream(
        &mut self,
        world: &stagcrest_world::World,
        stream: &TerrainStreamState,
        horizontal_radius: i32,
        vertical_radius: i32,
        y_bounds: std::ops::RangeInclusive<i32>,
        air: BlockId,
    ) -> usize {
        let candidates: Vec<ChunkPos> = self
            .pass2_pending
            .keys()
            .copied()
            .filter(|pos| {
                chunk_in_stream(*pos, stream, horizontal_radius, vertical_radius)
                    && world.is_terrain_ready(*pos)
                    && !world.is_generated(*pos)
                    && !self.deferred_decorate.iter().any(|d| d.pos == *pos)
                    && !self.pass2_in_flight(*pos)
            })
            .collect();
        let mut requeued = 0usize;
        for pos in candidates {
            let Some(data) = self.pass2_pending.get(&pos).cloned() else {
                continue;
            };
            self.deferred_decorate.push_back(data);
            requeued += 1;
        }
        // Recover chunks that have terrain in world but no cached density (e.g. after reload).
        if requeued == 0 {
            let Some((y_min, y_max)) =
                stream_vertical_y_range(stream.center_y, vertical_radius, &y_bounds)
            else {
                return requeued;
            };
            for cy in y_min..=y_max {
                for cx in (stream.center_x - horizontal_radius)..=(stream.center_x + horizontal_radius)
                {
                    for cz in (stream.center_z - horizontal_radius)
                        ..=(stream.center_z + horizontal_radius)
                    {
                        let pos = ChunkPos { x: cx, y: cy, z: cz };
                        if !world.is_terrain_ready(pos)
                            || world.is_generated(pos)
                            || self.pass2_pending.contains_key(&pos)
                            || self.deferred_decorate.iter().any(|d| d.pos == pos)
                            || self.pass2_in_flight(pos)
                        {
                            continue;
                        }
                        let captured = capture_density_from_world(world, pos, air);
                        self.pass2_pending.insert(pos, captured.clone());
                        self.deferred_decorate.push_back(captured);
                        requeued += 1;
                    }
                }
            }
        }
        requeued
    }

    pub fn needs_pass2_requeue(&self, world: &stagcrest_world::World) -> bool {
        self.pass2_pending.keys().any(|pos| {
            world.is_terrain_ready(*pos)
                && !world.is_generated(*pos)
                && !self.deferred_decorate.iter().any(|d| d.pos == *pos)
                && !self.pass2_in_flight(*pos)
        })
    }

    pub fn generation_backlog(&self) -> usize {
        self.pending_generate.len()
            + self.deferred_decorate.len()
            + self.pending_load.len()
            + self.pass2_pending.len()
    }

    pub fn enqueue_area(
        &mut self,
        terrain: &mut WorldGenState,
        stored_chunks: &mut HashSet<ChunkPos>,
        storage: &dyn stagcrest_storage::ChunkStorage,
        world: &stagcrest_world::World,
        center: ChunkPos,
        horizontal_radius: i32,
        vertical_radius: i32,
        y_bounds: std::ops::RangeInclusive<i32>,
        player_block: BlockPos,
    ) {
        let Some((y_min, y_max)) = stream_vertical_y_range(center.y, vertical_radius, &y_bounds)
        else {
            return;
        };
        let mut gen_candidates = Vec::new();
        let mut load_candidates = Vec::new();
        for cx in (center.x - horizontal_radius)..=(center.x + horizontal_radius) {
            for cz in (center.z - horizontal_radius)..=(center.z + horizontal_radius) {
                for cy in y_min..=y_max {
                    let pos = ChunkPos { x: cx, y: cy, z: cz };
                    let chunk_center_x = cx * stagcrest_protocol::CHUNK_SIZE + 8;
                    let chunk_center_y = cy * stagcrest_protocol::CHUNK_SIZE + 8;
                    let chunk_center_z = cz * stagcrest_protocol::CHUNK_SIZE + 8;
                    let dx = chunk_center_x - player_block.x;
                    let dy = chunk_center_y - player_block.y;
                    let dz = chunk_center_z - player_block.z;
                    let key = (dx * dx + dy * dy + dz * dz, -cy);
                    if self.should_generate(pos, terrain, stored_chunks, storage, world) {
                        gen_candidates.push((key, pos));
                    } else if self.should_load(pos, stored_chunks, storage, world) {
                        load_candidates.push((key, pos));
                    }
                }
            }
        }
        gen_candidates.sort_by_key(|(key, _)| *key);
        load_candidates.sort_by_key(|(key, _)| *key);
        for (_, pos) in gen_candidates {
            self.enqueue_generate(pos, terrain, world);
        }
        for (_, pos) in load_candidates {
            self.enqueue_load(pos);
        }
    }

    fn should_generate(
        &self,
        pos: ChunkPos,
        terrain: &WorldGenState,
        stored_chunks: &mut HashSet<ChunkPos>,
        storage: &dyn stagcrest_storage::ChunkStorage,
        world: &stagcrest_world::World,
    ) -> bool {
        if self.chunk_in_pipeline(pos) {
            return false;
        }

        if !world.has_chunk(pos) {
            if stored_chunks.contains(&pos) {
                return false;
            }
            if storage.contains(pos) {
                stored_chunks.insert(pos);
                return false;
            }
            return true;
        }

        if world.is_generated(pos)
            || world.is_terrain_ready(pos)
            || terrain.is_chunk_generated(pos)
            || terrain.is_chunk_terrain_ready(pos)
        {
            return false;
        }
        if stored_chunks.contains(&pos) {
            return false;
        }
        if storage.contains(pos) {
            stored_chunks.insert(pos);
            return false;
        }
        true
    }

    fn should_load(
        &self,
        pos: ChunkPos,
        stored_chunks: &mut HashSet<ChunkPos>,
        storage: &dyn stagcrest_storage::ChunkStorage,
        world: &stagcrest_world::World,
    ) -> bool {
        if world.has_chunk(pos) || self.chunk_in_pipeline(pos) {
            return false;
        }
        if stored_chunks.contains(&pos) {
            return true;
        }
        if storage.contains(pos) {
            stored_chunks.insert(pos);
            return true;
        }
        false
    }

    pub fn chunk_in_pipeline(&self, pos: ChunkPos) -> bool {
        self.in_progress.contains(&pos)
            || self.pending_generate.iter().any(|&p| p == pos)
            || self.pending_load.iter().any(|&p| p == pos)
            || self.pending_persist.iter().any(|&(p, _)| p == pos)
            || self.deferred_decorate.iter().any(|d| d.pos == pos)
            || self.integrate_terrain.iter().any(|d| d.pos == pos)
            || self.pass2_pending.contains_key(&pos)
            || self.pass2_in_flight(pos)
    }

    pub fn cancel_chunk(&mut self, pos: ChunkPos) {
        self.pending_generate.retain(|&p| p != pos);
        self.pending_load.retain(|&p| p != pos);
        self.pending_persist.retain(|&(p, _)| p != pos);
        self.deferred_decorate.retain(|d| d.pos != pos);
        self.pass2_pending.remove(&pos);
        self.in_progress.remove(&pos);
        let mut dropped_io = 0usize;
        let mut dropped_gen = 0usize;
        self.tasks.retain(|t| {
            if t.pos == pos {
                if task_is_io(&t.task) {
                    dropped_io += 1;
                } else {
                    dropped_gen += 1;
                }
                false
            } else {
                true
            }
        });
        self.io_in_flight = self.io_in_flight.saturating_sub(dropped_io);
        self.gen_in_flight = self.gen_in_flight.saturating_sub(dropped_gen);
        self.discarded.insert(pos);
    }

    pub fn prune_out_of_stream(
        &mut self,
        stream: &TerrainStreamState,
        horizontal_radius: i32,
        vertical_radius: i32,
    ) {
        if !stream.valid {
            return;
        }
        self.pending_generate
            .retain(|&pos| chunk_in_stream(pos, stream, horizontal_radius, vertical_radius));
        self.pending_load
            .retain(|&pos| chunk_in_stream(pos, stream, horizontal_radius, vertical_radius));
        self.pending_persist
            .retain(|&(p, _)| chunk_in_stream(p, stream, horizontal_radius, vertical_radius));
        self.deferred_decorate.retain(|data| {
            chunk_in_stream(data.pos, stream, horizontal_radius, vertical_radius)
        });
        self.pass2_pending.retain(|pos, _| {
            chunk_in_stream(*pos, stream, horizontal_radius, vertical_radius)
        });
    }
}

fn capture_density_from_world(
    world: &stagcrest_world::World,
    pos: ChunkPos,
    air: BlockId,
) -> ChunkGenData {
    let base_x = pos.x * CHUNK_SIZE;
    let base_y = pos.y * CHUNK_SIZE;
    let base_z = pos.z * CHUNK_SIZE;
    let mut entries = Vec::new();
    for ly in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let block_pos = BlockPos::new(base_x + lx, base_y + ly, base_z + lz);
                let (id, state) = world.get_block(block_pos);
                if id != air {
                    entries.push((block_pos, id, state));
                }
            }
        }
    }
    ChunkGenData { pos, entries }
}

fn chunk_in_stream(
    pos: ChunkPos,
    stream: &TerrainStreamState,
    horizontal_radius: i32,
    vertical_radius: i32,
) -> bool {
    (pos.x - stream.center_x).abs() <= horizontal_radius
        && (pos.z - stream.center_z).abs() <= horizontal_radius
        && (pos.y - stream.center_y).abs() <= vertical_radius
}

fn cpu_pool() -> &'static bevy::tasks::TaskPool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        AsyncComputeTaskPool::get()
    }
    #[cfg(target_arch = "wasm32")]
    {
        IoTaskPool::get()
    }
}

pub(crate) fn pipeline_streaming(
    mod_ctx: Option<Res<crate::game::ModContext>>,
    config: Res<crate::game::GameConfig>,
    session: Option<ResMut<crate::session::WorldSession>>,
    mut world: ResMut<crate::game::StagcrestWorldResource>,
    mut terrain: ResMut<crate::game::TerrainGen>,
    mut last_center: ResMut<crate::game::LastStreamCenter>,
    mut pipeline: ResMut<StreamingPipeline>,
    mut mesh_scheduler: ResMut<MeshScheduler>,
    mut stream: ResMut<TerrainStreamState>,
    mut cache: ResMut<stagcrest_render::MeshCacheResource>,
    camera: Query<&Transform, With<crate::player::FlyCamera>>,
) {
    let Some(_ctx) = mod_ctx else { return };
    let Some(mut session) = session else { return };
    let storage = session.storage.clone();
    let Ok(cam) = camera.single() else { return };
    let pos = cam.translation;
    let player_block = BlockPos::new(
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    );
    let center = player_block.chunk_pos();
    let y_bounds = stagcrest_mod_host::world_chunk_y_bounds(terrain.0.config());
    let h_radius = config.render_distance;
    let v_radius = config.vertical_render_distance;
    let unload_h = h_radius + 2;
    let unload_v = v_radius + 2;

    stream.center_x = center.x;
    stream.center_y = center.y;
    stream.center_z = center.z;
    stream.valid = true;

    let (evicted, _, _) = world
        .0
        .unload_far_chunks_3d(center, unload_h, unload_v);

    for evicted_chunk in evicted {
        let pos = evicted_chunk.pos;
        if evicted_chunk.meta.is_populated() {
            if let Some(inactive) = evicted_chunk.inactive {
                let wire = inactive.encode_wire();
                let compressed = compress_stored(&wire);
                pipeline.pending_persist.push_back((pos, compressed));
            }
            cache.0.remove(pos);
            pipeline.cancel_chunk(pos);
            mesh_scheduler.cancel(pos);
            terrain.0.clear_chunk(pos);
            continue;
        }
        cache.0.remove(pos);
        terrain.0.clear_chunk(pos);
        pipeline.cancel_chunk(pos);
        mesh_scheduler.cancel(pos);
    }

    let center_changed = last_center.0 != Some(center);
    if center_changed {
        pipeline.prune_out_of_stream(&stream, h_radius, v_radius);
        mesh_scheduler.prune_out_of_stream(
            stream.center_x,
            stream.center_y,
            stream.center_z,
            stream.valid,
            h_radius,
            v_radius,
        );
    }
    if center_changed || pipeline.generation_backlog() < QUEUE_REFILL {
        pipeline.enqueue_area(
            &mut terrain.0,
            &mut session.stored_chunks,
            storage.as_ref(),
            &world.0,
            center,
            h_radius,
            v_radius,
            y_bounds.clone(),
            player_block,
        );
        if center_changed {
            last_center.0 = Some(center);
        }
    }

    if center_changed || pipeline.needs_pass2_requeue(&world.0) {
        pipeline.requeue_pass2_in_stream(
            &world.0,
            &stream,
            h_radius,
            v_radius,
            y_bounds.clone(),
            world.0.air(),
        );
    }
}

pub fn pipeline_dispatch(
    mut pipeline: ResMut<StreamingPipeline>,
    mut terrain: ResMut<crate::game::TerrainGen>,
    blocks: Option<Res<TerrainBlocks>>,
    biomes: Option<Res<TerrainBiomes>>,
    mod_ctx: Option<Res<crate::game::ModContext>>,
    world: Res<crate::game::StagcrestWorldResource>,
    session: Option<Res<crate::session::WorldSession>>,
) {
    let Some(blocks) = blocks else { return };
    let Some(biomes) = biomes else { return };
    let Some(_ctx) = mod_ctx else { return };
    let Some(session) = session else { return };
    let generator = terrain.0.generator().clone();

    let mut spins = 0;
    while spins < 128
        && (pipeline.gen_in_flight < MAX_GEN_IN_FLIGHT || pipeline.io_in_flight < MAX_IO_IN_FLIGHT)
    {
        if !dispatch_one(
            &mut pipeline,
            &mut terrain,
            &blocks,
            &biomes,
            &world,
            &session,
            &generator,
        ) {
            break;
        }
        spins += 1;
    }
}

fn dispatch_one(
    pipeline: &mut StreamingPipeline,
    terrain: &mut crate::game::TerrainGen,
    blocks: &TerrainBlocks,
    biomes: &TerrainBiomes,
    world: &crate::game::StagcrestWorldResource,
    session: &crate::session::WorldSession,
    generator: &stagcrest_mod_host::TerrainGenerator,
) -> bool {
    if pipeline.io_in_flight < MAX_IO_IN_FLIGHT {
        if let Some((pos, compressed)) = pipeline.pending_persist.pop_front() {
            pipeline.in_progress.insert(pos);
            pipeline.io_in_flight += 1;
            let storage = session.storage.clone();
            pipeline.tasks.push(InFlight {
                pos,
                task: ErasedTask::StoragePersist(IoTaskPool::get().spawn(async move {
                    storage.put(pos, &compressed)
                })),
            });
            return true;
        }

        if let Some(pos) = pipeline.pending_load.pop_front() {
            pipeline.in_progress.insert(pos);
            pipeline.io_in_flight += 1;
            let storage = session.storage.clone();
            pipeline.tasks.push(InFlight {
                pos,
                task: ErasedTask::StorageLoad(IoTaskPool::get().spawn(async move {
                    load_inactive_chunk(storage.as_ref(), pos)
                })),
            });
            return true;
        }
    }

    if pipeline.gen_in_flight < MAX_GEN_IN_FLIGHT {
        let density_backlog = pipeline.pending_generate.len();
        let decorate_backlog = pipeline.deferred_decorate.len();
        let prefer_density =
            density_backlog > 128 && density_backlog > decorate_backlog.saturating_mul(2);

        if !prefer_density {
            if let Some(data) = take_decorate_candidate(pipeline, terrain, &world.0) {
                let pos = data.pos;
                let snapshot = DecorateSnapshot::capture(&world.0, pos, world.0.air());
                pipeline.in_progress.insert(pos);
                pipeline.gen_in_flight += 1;
                let gen = generator.clone();
                let column_blocks = blocks.0;
                let biome_registry = biomes.0.clone();
                pipeline.tasks.push(InFlight {
                    pos,
                    task: ErasedTask::Decorate(cpu_pool().spawn(async move {
                        gen.decorate_chunk_offline(column_blocks, &biome_registry, &data, &snapshot)
                    })),
                });
                return true;
            }
        }

        if let Some(pos) = pipeline.pending_generate.pop_front() {
            if world.0.has_chunk(pos) {
                if terrain.0.is_chunk_generated(pos)
                    || world.0.is_generated(pos)
                    || terrain.0.is_chunk_terrain_ready(pos)
                    || world.0.is_terrain_ready(pos)
                {
                    return true;
                }
            } else {
                terrain.0.clear_chunk(pos);
            }
            pipeline.in_progress.insert(pos);
            pipeline.gen_in_flight += 1;
            let gen = generator.clone();
            let column_blocks = blocks.0;
            pipeline.tasks.push(InFlight {
                pos,
                task: ErasedTask::Density(cpu_pool().spawn(async move {
                    gen.compute_chunk_density(column_blocks, pos)
                })),
            });
            return true;
        }

        if prefer_density {
            if let Some(data) = take_decorate_candidate(pipeline, terrain, &world.0) {
                let pos = data.pos;
                let snapshot = DecorateSnapshot::capture(&world.0, pos, world.0.air());
                pipeline.in_progress.insert(pos);
                pipeline.gen_in_flight += 1;
                let gen = generator.clone();
                let column_blocks = blocks.0;
                let biome_registry = biomes.0.clone();
                pipeline.tasks.push(InFlight {
                    pos,
                    task: ErasedTask::Decorate(cpu_pool().spawn(async move {
                        gen.decorate_chunk_offline(column_blocks, &biome_registry, &data, &snapshot)
                    })),
                });
                return true;
            }
        }

        return false;
    }

    false
}

const HORIZONTAL_NEIGHBORS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

fn chunk_terrain_ready(
    pos: ChunkPos,
    terrain: &crate::game::TerrainGen,
    world: &stagcrest_world::World,
) -> bool {
    world.is_terrain_ready(pos) || terrain.0.is_chunk_terrain_ready(pos)
}

fn neighbor_generation_expected(
    pos: ChunkPos,
    pipeline: &StreamingPipeline,
    world: &stagcrest_world::World,
) -> bool {
    world.has_chunk(pos) || pipeline.chunk_in_pipeline(pos)
}

fn neighbor_satisfied_for_decorate(
    neighbor: ChunkPos,
    pipeline: &StreamingPipeline,
    terrain: &crate::game::TerrainGen,
    world: &stagcrest_world::World,
) -> bool {
    if chunk_terrain_ready(neighbor, terrain, world) {
        return true;
    }
    // Outside the active gen bubble: treat as void (no terrain required).
    !neighbor_generation_expected(neighbor, pipeline, world)
}

fn take_decorate_candidate(
    pipeline: &mut StreamingPipeline,
    terrain: &crate::game::TerrainGen,
    world: &stagcrest_world::World,
) -> Option<ChunkGenData> {
    let y_bounds = stagcrest_mod_host::world_chunk_y_bounds(terrain.0.config());
    let mut deferred = Vec::new();
    while let Some(data) = pipeline.deferred_decorate.pop_front() {
        let pos = data.pos;

        if pos.y < *y_bounds.end() {
            let above = ChunkPos {
                x: pos.x,
                y: pos.y + 1,
                z: pos.z,
            };
            if neighbor_generation_expected(above, pipeline, world)
                && !chunk_terrain_ready(above, terrain, world)
            {
                deferred.push(data);
                continue;
            }
        }

        let mut neighbors_ready = true;
        for (dx, dz) in HORIZONTAL_NEIGHBORS {
            let neighbor = ChunkPos {
                x: pos.x + dx,
                y: pos.y,
                z: pos.z + dz,
            };
            if !neighbor_satisfied_for_decorate(neighbor, pipeline, terrain, world) {
                neighbors_ready = false;
                break;
            }
        }
        if !neighbors_ready {
            deferred.push(data);
            continue;
        }

        return Some(data);
    }
    pipeline.deferred_decorate.extend(deferred);
    None
}

enum FinishedWork {
    Density(ChunkGenData),
    Decorate {
        pos: ChunkPos,
        entries: Vec<(BlockPos, BlockId, BlockState)>,
    },
    StorageLoad {
        pos: ChunkPos,
        result: Result<Option<InactiveChunk>, StorageError>,
    },
    StoragePersist {
        pos: ChunkPos,
        result: Result<(), StorageError>,
    },
}

fn task_is_io(task: &ErasedTask) -> bool {
    matches!(
        task,
        ErasedTask::StorageLoad(_) | ErasedTask::StoragePersist(_)
    )
}

pub fn pipeline_poll(mut pipeline: ResMut<StreamingPipeline>) {
    let mut finished = Vec::new();
    pipeline.tasks.retain_mut(|in_flight| {
        let is_io = task_is_io(&in_flight.task);
        let done = match &mut in_flight.task {
            ErasedTask::Density(task) => poll_future_now(task).map(FinishedWork::Density),
            ErasedTask::Decorate(task) => poll_future_now(task).map(|entries| FinishedWork::Decorate {
                pos: in_flight.pos,
                entries,
            }),
            ErasedTask::StorageLoad(task) => {
                poll_future_now(task).map(|result| FinishedWork::StorageLoad {
                    pos: in_flight.pos,
                    result,
                })
            }
            ErasedTask::StoragePersist(task) => poll_future_now(task).map(|result| {
                FinishedWork::StoragePersist {
                    pos: in_flight.pos,
                    result,
                }
            }),
        };
        if let Some(work) = done {
            finished.push((work, is_io));
            false
        } else {
            true
        }
    });

    for (work, is_io) in finished {
        if is_io {
            pipeline.io_in_flight = pipeline.io_in_flight.saturating_sub(1);
        } else {
            pipeline.gen_in_flight = pipeline.gen_in_flight.saturating_sub(1);
        }
        match work {
            FinishedWork::Density(data) => {
                let pos = data.pos;
                pipeline.in_progress.remove(&pos);
                if pipeline.discarded.remove(&pos) {
                    continue;
                }
                pipeline.integrate_terrain.push(data);
            }
            FinishedWork::Decorate { pos, entries } => {
                pipeline.in_progress.remove(&pos);
                if pipeline.discarded.remove(&pos) {
                    continue;
                }
                pipeline.integrate_decorate.push((pos, entries));
            }
            FinishedWork::StorageLoad { pos, result } => {
                pipeline.in_progress.remove(&pos);
                if pipeline.discarded.remove(&pos) {
                    continue;
                }
                pipeline.integrate_load.push((pos, result));
            }
            FinishedWork::StoragePersist { pos, result } => {
                pipeline.in_progress.remove(&pos);
                pipeline.integrate_persist.push((pos, result));
            }
        }
    }
}

pub fn pipeline_integrate(
    mut pipeline: ResMut<StreamingPipeline>,
    mut terrain: ResMut<crate::game::TerrainGen>,
    mut world: ResMut<crate::game::StagcrestWorldResource>,
    mut mesh_scheduler: ResMut<MeshScheduler>,
    stream: Res<TerrainStreamState>,
    mut session: Option<ResMut<crate::session::WorldSession>>,
) {
    let player_block = BlockPos::new(
        stream.center_x * stagcrest_protocol::CHUNK_SIZE,
        stream.center_y * stagcrest_protocol::CHUNK_SIZE,
        stream.center_z * stagcrest_protocol::CHUNK_SIZE,
    );

    let terrain_batch = pipeline.integrate_terrain.drain(..).collect::<Vec<_>>();
    for data in terrain_batch {
        let pos = data.pos;
        pipeline.pass2_pending.insert(pos, data.clone());
        let _ = terrain.0.mark_chunk_terrain_ready(pos);
        if !world.0.is_generated(pos) {
            world.0.set_blocks(data.entries.clone());
            world.0.mark_chunk_terrain_ready(pos);
        }
        if !terrain.0.is_chunk_generated(pos)
            && !pipeline.deferred_decorate.iter().any(|d| d.pos == pos)
            && !pipeline.pass2_in_flight(pos)
        {
            pipeline.deferred_decorate.push_back(data);
        }
    }

    let decorate = pipeline.integrate_decorate.drain(..).collect::<Vec<_>>();
    for (pos, entries) in decorate {
        terrain.0.mark_chunk_generated(pos);
        if !world.0.is_generated(pos) {
            world.0.set_blocks(entries);
            world.0.finalize_generated_chunk(pos);
            if world.0.has_chunk(pos) {
                mesh_scheduler.request(
                    pos,
                    RemeshUrgency::Visible,
                    chunk_distance_sq_from_block(player_block, pos),
                );
            }
        }
        pipeline.pass2_pending.remove(&pos);
    }

    let loads = pipeline.integrate_load.drain(..).collect::<Vec<_>>();
    for (pos, result) in loads {
        match result {
            Ok(Some(chunk)) => {
                world.0.insert_inactive_chunk(pos, chunk);
                terrain.0.mark_chunk_generated(pos);
                pipeline.clear_generation_work(pos);
                if let Some(session) = session.as_mut() {
                    session.stored_chunks.insert(pos);
                }
                if world.0.has_chunk(pos) {
                    mesh_scheduler.request(
                        pos,
                        RemeshUrgency::Visible,
                        chunk_distance_sq_from_block(player_block, pos),
                    );
                }
            }
            Ok(None) => {
                if let Some(session) = session.as_mut() {
                    session.stored_chunks.remove(&pos);
                }
                pipeline.enqueue_generate(pos, &mut terrain.0, &world.0);
            }
            Err(err) => {
                tracing::error!("failed to load chunk {pos:?}: {err}");
                pipeline.enqueue_load(pos);
            }
        }
    }

    let persists = pipeline.integrate_persist.drain(..).collect::<Vec<_>>();
    for (pos, result) in persists {
        match result {
            Ok(()) => {
                if let Some(session) = session.as_mut() {
                    session.stored_chunks.insert(pos);
                }
            }
            Err(err) => {
                tracing::error!("failed to persist chunk {pos:?}: {err}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::AsyncComputeTaskPool;
    use stagcrest_mod_host::WorldSeed;

    #[test]
    fn stream_vertical_y_range_extends_above_terrain_cap() {
        let bounds = 0..=10;
        let (y_min, y_max) = stream_vertical_y_range(15, 4, &bounds).unwrap();
        assert_eq!(y_min, 11);
        assert_eq!(y_max, 15);
        let (y_min, y_max) = stream_vertical_y_range(10, 4, &bounds).unwrap();
        assert_eq!(y_min, 6);
        assert_eq!(y_max, 10);
    }

    #[test]
    fn should_generate_when_terrain_stale_but_chunk_evicted() {
        let mut terrain = WorldGenState::new(WorldSeed(1));
        let pos = ChunkPos { x: 2, y: 1, z: -3 };
        terrain.mark_chunk_terrain_ready(pos);
        let air = stagcrest_protocol::BlockId(0);
        let world = stagcrest_world::World::new(air);
        let pipeline = StreamingPipeline::default();
        let mut stored = HashSet::new();
        let storage = stagcrest_storage::NullChunkStorage;
        assert!(pipeline.should_generate(
            pos,
            &terrain,
            &mut stored,
            &storage,
            &world,
        ));
    }

    #[test]
    fn generate_and_load_queues_are_mutually_exclusive() {
        let terrain = WorldGenState::new(WorldSeed(1));
        let air = stagcrest_protocol::BlockId(0);
        let world = stagcrest_world::World::new(air);
        let load_wins = ChunkPos { x: 3, y: 0, z: -2 };
        let mut pipeline = StreamingPipeline::default();
        let mut terrain = terrain;
        assert!(pipeline.enqueue_generate(load_wins, &mut terrain, &world));
        assert!(pipeline.enqueue_load(load_wins));
        assert!(pipeline.pending_load.iter().any(|&p| p == load_wins));
        assert!(!pipeline.pending_generate.iter().any(|&p| p == load_wins));

        let gen_wins = ChunkPos { x: 4, y: 0, z: -2 };
        assert!(pipeline.enqueue_load(gen_wins));
        assert!(pipeline.enqueue_generate(gen_wins, &mut terrain, &world));
        assert!(pipeline.pending_generate.iter().any(|&p| p == gen_wins));
        assert!(!pipeline.pending_load.iter().any(|&p| p == gen_wins));
    }

    #[test]
    fn pass2_pending_counts_as_in_pipeline() {
        let pos = ChunkPos { x: 1, y: 0, z: 0 };
        let mut pipeline = StreamingPipeline::default();
        pipeline.pass2_pending.insert(
            pos,
            ChunkGenData {
                pos,
                entries: Vec::new(),
            },
        );
        assert!(pipeline.chunk_in_pipeline(pos));
        assert_eq!(pipeline.generation_backlog(), 1);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn cancel_drops_in_flight_tasks() {
        use bevy::tasks::TaskPoolBuilder;
        AsyncComputeTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(1).build());

        let mut pipeline = StreamingPipeline::default();
        let pos = ChunkPos { x: 0, y: 0, z: 0 };
        pipeline.gen_in_flight = 1;
        pipeline.tasks.push(InFlight {
            pos,
            task: ErasedTask::Density(AsyncComputeTaskPool::get().spawn(async {
                std::future::pending::<ChunkGenData>().await
            })),
        });

        pipeline.cancel_chunk(pos);

        assert!(pipeline.tasks.is_empty());
        assert_eq!(pipeline.gen_in_flight, 0);
    }
}
