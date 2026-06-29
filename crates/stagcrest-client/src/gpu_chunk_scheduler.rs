use bevy::prelude::*;
use stagcrest_mesh::capture_power_grid;
use stagcrest_protocol::{BlockPos, ChunkPos, CHUNK_SIZE};

use crate::chunk_streaming::BiomeGridCache;
use crate::game::ModContext;
use crate::world_replica::{CircuitPowerOverlay, WorldReplica};
use stagcrest_render::{pack_halo_from_world, GpuChunkCache, GpuVoxelStats, GpuVoxelTables};
use std::collections::{BinaryHeap, HashMap};

const MAX_UPLOAD_PER_FRAME: usize = 32;
const MAX_DIRTY_DRAIN_PER_FRAME: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RemeshUrgency {
    Background = 0,
    Visible = 1,
    Circuit = 2,
    Interactive = 3,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct UploadRequest {
    urgency: RemeshUrgency,
    distance_sq: i32,
    version: u64,
    pos: ChunkPos,
}

impl Ord for UploadRequest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.urgency.cmp(&other.urgency) {
            std::cmp::Ordering::Equal => other
                .distance_sq
                .cmp(&self.distance_sq)
                .then_with(|| self.version.cmp(&other.version)),
            ord => ord,
        }
    }
}

impl PartialOrd for UploadRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Resource, Default)]
pub struct GpuChunkScheduler {
    pending: HashMap<ChunkPos, UploadRequest>,
    heap: BinaryHeap<UploadRequest>,
    version: HashMap<ChunkPos, u64>,
    uploads_this_frame: usize,
}

impl GpuChunkScheduler {
    pub fn request(&mut self, pos: ChunkPos, urgency: RemeshUrgency, distance_sq: i32) {
        if let Some(existing) = self.pending.get(&pos) {
            if existing.urgency > urgency {
                return;
            }
        }
        let version = self.version.entry(pos).or_insert(0);
        *version += 1;
        let req = UploadRequest {
            urgency,
            distance_sq,
            version: *version,
            pos,
        };
        self.pending.insert(pos, req);
        self.heap.push(req);
    }

    pub fn cancel(&mut self, pos: ChunkPos) {
        self.pending.remove(&pos);
        self.version.remove(&pos);
    }

    fn pop_next(&mut self) -> Option<UploadRequest> {
        while let Some(req) = self.heap.pop() {
            let Some(current) = self.pending.get(&req.pos) else {
                continue;
            };
            if current.version != req.version {
                continue;
            }
            return Some(*current);
        }
        None
    }

    pub fn rebuild_heap(&mut self) {
        self.heap.clear();
        for req in self.pending.values() {
            self.heap.push(*req);
        }
    }

    #[cfg(test)]
    pub fn is_pending(&self, pos: ChunkPos) -> bool {
        self.pending.contains_key(&pos)
    }
}

pub fn chunk_distance_sq_from_camera(cam: &Transform, chunk: ChunkPos) -> i32 {
    let player_block = BlockPos::new(
        cam.translation.x.floor() as i32,
        cam.translation.y.floor() as i32,
        cam.translation.z.floor() as i32,
    );
    chunk_distance_sq_from_block(player_block, chunk)
}

pub fn chunk_distance_sq_from_block(player_block: BlockPos, chunk: ChunkPos) -> i32 {
    let cx = chunk.x * CHUNK_SIZE + CHUNK_SIZE / 2;
    let cy = chunk.y * CHUNK_SIZE + CHUNK_SIZE / 2;
    let cz = chunk.z * CHUNK_SIZE + CHUNK_SIZE / 2;
    let dx = cx - player_block.x;
    let dy = cy - player_block.y;
    let dz = cz - player_block.z;
    dx * dx + dy * dy + dz * dz
}

fn capture_upload(
    pos: ChunkPos,
    world: &stagcrest_world::World,
    ctx: &ModContext,
    power: Option<&CircuitPowerOverlay>,
) -> Option<stagcrest_render::gpu_voxel::types::GpuChunkUpload> {
    let center = world.pack_chunk_for_storage(pos)?;
    let air = world.air();
    let power_lookup = power.map(|p| p as &dyn stagcrest_mod_client::PowerLookup);
    let power_grid = capture_power_grid(pos, power_lookup);
    Some(pack_halo_from_world(
        pos,
        world,
        air,
        &ctx.registry,
        &center,
        power_grid,
        &ctx.biomes,
        &ctx.colormaps,
    ))
}

pub fn gpu_chunk_drain_dirty(
    mut world: ResMut<WorldReplica>,
    mut scheduler: ResMut<GpuChunkScheduler>,
    camera: Query<&Transform, With<crate::player::FlyCamera>>,
) {
    let Ok(cam) = camera.single() else { return };
    let dirty = world.take_dirty_chunks();
    let mut scheduled = 0usize;
    for pos in dirty {
        if scheduled >= MAX_DIRTY_DRAIN_PER_FRAME {
            world.dirty_chunks.insert(pos);
            continue;
        }
        if !world.is_generated(pos) || !world.has_chunk(pos) {
            world.dirty_chunks.insert(pos);
            continue;
        }
        scheduler.request(
            pos,
            RemeshUrgency::Background,
            chunk_distance_sq_from_camera(cam, pos),
        );
        scheduled += 1;
    }
}

pub fn gpu_chunk_upload(
    mut scheduler: ResMut<GpuChunkScheduler>,
    mod_ctx: Option<Res<ModContext>>,
    world: Res<WorldReplica>,
    power: Option<Res<CircuitPowerOverlay>>,
    _biome_cache: Option<Res<BiomeGridCache>>,
    mut cache: ResMut<GpuChunkCache>,
    mut stats: ResMut<GpuVoxelStats>,
) {
    let Some(ctx) = mod_ctx else { return };
    scheduler.uploads_this_frame = 0;
    scheduler.rebuild_heap();

    while scheduler.uploads_this_frame < MAX_UPLOAD_PER_FRAME {
        let Some(req) = scheduler.pop_next() else {
            break;
        };
        if !world.is_generated(req.pos) || !world.has_chunk(req.pos) {
            continue;
        }
        let Some(upload) = capture_upload(req.pos, &*world, &ctx, power.as_deref())
        else {
            continue;
        };
        scheduler.pending.remove(&req.pos);
        cache.commit(upload);
        scheduler.uploads_this_frame += 1;
    }

    stats.chunk_count = cache.chunk_count();
}

pub fn gpu_chunk_recover(
    world: Res<WorldReplica>,
    cache: Res<GpuChunkCache>,
    mut scheduler: ResMut<GpuChunkScheduler>,
    camera: Query<&Transform, With<crate::player::FlyCamera>>,
    config: Res<crate::game::GameConfig>,
    mut tick: Local<u32>,
) {
    *tick = tick.wrapping_add(1);
    if *tick % 60 != 0 {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    let h = config.render_distance;
    let v = config.vertical_render_distance;
    let center_chunk = BlockPos::new(
        cam.translation.x.floor() as i32,
        cam.translation.y.floor() as i32,
        cam.translation.z.floor() as i32,
    )
    .chunk_pos();

    for pos in world.loaded_chunk_positions() {
        if (pos.x - center_chunk.x).abs() > h
            || (pos.z - center_chunk.z).abs() > h
            || (pos.y - center_chunk.y).abs() > v
        {
            continue;
        }
        if !world.is_generated(pos) || !world.has_chunk(pos) {
            continue;
        }
        if cache.contains(pos) || scheduler.pending.contains_key(&pos) {
            continue;
        }
        scheduler.request(
            pos,
            RemeshUrgency::Visible,
            chunk_distance_sq_from_camera(cam, pos),
        );
    }
}

pub fn init_gpu_voxel_tables(
    mod_ctx: Option<Res<ModContext>>,
    mut commands: Commands,
    existing: Option<Res<GpuVoxelTables>>,
) {
    if existing.is_some() {
        return;
    }
    let Some(ctx) = mod_ctx else { return };
    let tables = GpuVoxelTables::build(&ctx.registry, &ctx.models);
    commands.insert_resource(tables);
}

pub fn request_face_neighbors(
    scheduler: &mut GpuChunkScheduler,
    world: &mut stagcrest_world::World,
    center: ChunkPos,
    urgency: RemeshUrgency,
    distance_sq: impl Fn(ChunkPos) -> i32,
    clear_dirty: bool,
) {
    for pos in stagcrest_world::World::face_neighbor_chunks(center) {
        if world.has_chunk(pos) {
            scheduler.request(pos, urgency, distance_sq(pos));
            if clear_dirty {
                world.clear_dirty(pos);
            }
        }
    }
}
