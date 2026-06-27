use bevy::prelude::*;
use bevy::tasks::Task;
use stagcrest_mesh::{build_chunk_mesh_snapshot, capture_power_grid, MeshSnapshot};
use stagcrest_protocol::{BlockPos, ChunkPos, CHUNK_SIZE};

use crate::world_replica::WorldReplica;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

const MAX_MESH_IN_FLIGHT: usize = 16;
const MAX_BACKGROUND_DISPATCH_PER_FRAME: usize = 8;
pub const MAX_DIRTY_DRAIN_PER_FRAME: usize = 32;

/// Poll a future once without blocking the browser event loop (required on wasm).
pub(crate) fn poll_future_now<T>(future: &mut (impl Future<Output = T> + Unpin)) -> Option<T> {
    struct NoopWaker;
    impl Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(NoopWaker));
    let mut cx = Context::from_waker(&waker);
    match Pin::new(future).poll(&mut cx) {
        Poll::Ready(val) => Some(val),
        Poll::Pending => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RemeshUrgency {
    Background = 0,
    Visible = 1,
    Circuit = 2,
    Interactive = 3,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct MeshRequest {
    urgency: RemeshUrgency,
    distance_sq: i32,
    version: u64,
    pos: ChunkPos,
}

impl Ord for MeshRequest {
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

impl PartialOrd for MeshRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct MeshInFlight {
    pos: ChunkPos,
    expected_version: u64,
    task: Task<stagcrest_mesh::ChunkMesh>,
}

#[derive(Resource, Default)]
pub struct MeshScheduler {
    pending: HashMap<ChunkPos, MeshRequest>,
    heap: BinaryHeap<MeshRequest>,
    mesh_version: HashMap<ChunkPos, u64>,
    in_progress: HashSet<ChunkPos>,
    mesh_in_flight: usize,
    tasks: Vec<MeshInFlight>,
    ready_meshes: VecDeque<(ChunkPos, stagcrest_mesh::ChunkMesh)>,
    background_dispatched_this_frame: usize,
}

impl MeshScheduler {
    pub fn request(&mut self, pos: ChunkPos, urgency: RemeshUrgency, distance_sq: i32) {
        if let Some(existing) = self.pending.get(&pos) {
            if existing.urgency > urgency {
                return;
            }
        }

        let version = self.mesh_version.entry(pos).or_insert(0);
        *version += 1;
        let req = MeshRequest {
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
        self.mesh_version.remove(&pos);
        self.in_progress.remove(&pos);
        let tasks_before = self.tasks.len();
        self.tasks.retain(|t| t.pos != pos);
        self.mesh_in_flight = self
            .mesh_in_flight
            .saturating_sub(tasks_before - self.tasks.len());
    }

    pub fn prune_out_of_stream(
        &mut self,
        center_x: i32,
        center_y: i32,
        center_z: i32,
        valid: bool,
        horizontal_radius: i32,
        vertical_radius: i32,
    ) {
        if !valid {
            return;
        }
        self.pending.retain(|&pos, _| {
            chunk_in_stream(
                pos,
                center_x,
                center_y,
                center_z,
                horizontal_radius,
                vertical_radius,
            )
        });
        self.mesh_version.retain(|pos, _| {
            chunk_in_stream(
                *pos,
                center_x,
                center_y,
                center_z,
                horizontal_radius,
                vertical_radius,
            )
        });
        let out_of_stream: Vec<_> = self
            .in_progress
            .iter()
            .copied()
            .filter(|pos| {
                !chunk_in_stream(
                    *pos,
                    center_x,
                    center_y,
                    center_z,
                    horizontal_radius,
                    vertical_radius,
                )
            })
            .collect();
        for pos in out_of_stream {
            self.cancel(pos);
        }
        self.rebuild_heap_from_pending();
    }

    fn rebuild_heap_from_pending(&mut self) {
        self.heap.clear();
        for req in self.pending.values() {
            self.heap.push(*req);
        }
    }

    fn pop_next(&mut self) -> Option<MeshRequest> {
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

    fn counts_toward_dispatch_cap(urgency: RemeshUrgency) -> bool {
        matches!(urgency, RemeshUrgency::Background | RemeshUrgency::Visible)
    }

    pub fn dispatch_one(
        &mut self,
        world: &stagcrest_world::World,
        ctx: &crate::game::ModContext,
        power: Option<&crate::world_replica::CircuitPowerOverlay>,
        replica: Option<&crate::world_replica::WorldReplica>,
    ) -> bool {
        if self.mesh_in_flight >= MAX_MESH_IN_FLIGHT {
            return false;
        }

        let mut deferred = Vec::new();
        let mut rebuilt_heap = false;

        for _ in 0..64 {
            let Some(req) = self.pop_next() else {
                if !self.pending.is_empty() && !rebuilt_heap {
                    self.rebuild_heap_from_pending();
                    rebuilt_heap = true;
                    continue;
                }
                break;
            };

            if self.in_progress.contains(&req.pos) {
                deferred.push(req);
                continue;
            }

            if Self::counts_toward_dispatch_cap(req.urgency)
                && self.background_dispatched_this_frame >= MAX_BACKGROUND_DISPATCH_PER_FRAME
            {
                deferred.push(req);
                continue;
            }

            if !world.is_generated(req.pos) {
                deferred.push(req);
                continue;
            }

            if !world.has_chunk(req.pos) {
                deferred.push(req);
                continue;
            }

            let Some(snapshot) = capture_mesh_snapshot(req.pos, world, ctx, power, replica) else {
                deferred.push(req);
                continue;
            };

            if Self::counts_toward_dispatch_cap(req.urgency) {
                self.background_dispatched_this_frame += 1;
            }

            self.pending.remove(&req.pos);
            self.in_progress.insert(req.pos);
            self.mesh_in_flight += 1;
            self.tasks.push(MeshInFlight {
                pos: req.pos,
                expected_version: req.version,
                task: cpu_pool().spawn(async move { build_chunk_mesh_snapshot(&snapshot) }),
            });
            for req in deferred {
                self.heap.push(req);
            }
            return true;
        }

        for req in deferred {
            self.heap.push(req);
        }
        false
    }

    pub fn poll(&mut self) {
        let mut finished = Vec::new();
        self.tasks.retain_mut(|in_flight| {
            if let Some(mesh) = poll_future_now(&mut in_flight.task) {
                finished.push((in_flight.pos, in_flight.expected_version, mesh));
                false
            } else {
                true
            }
        });
        self.mesh_in_flight = self.mesh_in_flight.saturating_sub(finished.len());

        for (pos, expected_version, mesh) in finished {
            self.in_progress.remove(&pos);
            let current = self.mesh_version.get(&pos).copied().unwrap_or(0);
            if expected_version == current {
                self.ready_meshes.push_back((pos, mesh));
            }
        }
    }
}

fn chunk_in_stream(
    pos: ChunkPos,
    center_x: i32,
    center_y: i32,
    center_z: i32,
    horizontal_radius: i32,
    vertical_radius: i32,
) -> bool {
    (pos.x - center_x).abs() <= horizontal_radius
        && (pos.z - center_z).abs() <= horizontal_radius
        && (pos.y - center_y).abs() <= vertical_radius
}

fn cpu_pool() -> &'static bevy::tasks::TaskPool {
    bevy::tasks::AsyncComputeTaskPool::get()
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

fn capture_mesh_snapshot(
    pos: ChunkPos,
    world: &stagcrest_world::World,
    ctx: &crate::game::ModContext,
    power: Option<&crate::world_replica::CircuitPowerOverlay>,
    replica: Option<&crate::world_replica::WorldReplica>,
) -> Option<MeshSnapshot> {
    let power_lookup = power.map(|p| p as &dyn stagcrest_mod_client::PowerLookup);
    let power_grid = capture_power_grid(pos, power_lookup);

    let climate = replica.and_then(|r| {
        let grid = r.biome_grid(pos)?;
        let wx = pos.x * CHUNK_SIZE + CHUNK_SIZE / 2;
        let wz = pos.z * CHUNK_SIZE + CHUNK_SIZE / 2;
        let biome_idx = r.biome_at(BlockPos::new(wx, pos.y * CHUNK_SIZE, wz))?;
        let biome = ctx.biomes.biome_by_index(biome_idx)?;
        Some(stagcrest_mesh::MeshClimateSnapshot {
            colormaps: Arc::new(ctx.colormaps.clone()),
            temperature: biome.temperature,
            downfall: biome.downfall,
        })
    });

    MeshSnapshot::capture(
        pos,
        world,
        Arc::new(ctx.registry.clone()),
        Arc::new(ctx.models.clone()),
        power_grid,
        climate,
    )
}

pub fn mesh_dispatch(
    mut scheduler: ResMut<MeshScheduler>,
    mod_ctx: Option<Res<crate::game::ModContext>>,
    world: Res<crate::world_replica::WorldReplica>,
    power: Option<Res<crate::world_replica::CircuitPowerOverlay>>,
) {
    let Some(ctx) = mod_ctx else { return };
    scheduler.background_dispatched_this_frame = 0;
    scheduler.rebuild_heap_from_pending();
    let mut spins = 0;
    while spins < 64 && scheduler.dispatch_one(&*world, &ctx, power.as_deref(), Some(&world)) {
        spins += 1;
    }
}

pub fn mesh_poll(mut scheduler: ResMut<MeshScheduler>) {
    scheduler.poll();
}

pub fn mesh_commit_meshes(
    mut scheduler: ResMut<MeshScheduler>,
    mut cache: ResMut<stagcrest_render::MeshCacheResource>,
) {
    while let Some((pos, mesh)) = scheduler.ready_meshes.pop_front() {
        cache.0.commit_mesh(pos, mesh);
    }
}

pub fn mesh_recover_unmeshed(
    world: Res<crate::world_replica::WorldReplica>,
    cache: Res<stagcrest_render::MeshCacheResource>,
    mut scheduler: ResMut<MeshScheduler>,
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
    let center = cam.translation;
    let center_chunk = stagcrest_protocol::BlockPos::new(
        center.x.floor() as i32,
        center.y.floor() as i32,
        center.z.floor() as i32,
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
        if cache.0.get(pos).is_some() {
            continue;
        }
        if scheduler.pending.contains_key(&pos) || scheduler.in_progress.contains(&pos) {
            continue;
        }
        scheduler.request(
            pos,
            RemeshUrgency::Visible,
            chunk_distance_sq_from_camera(cam, pos),
        );
    }
}

pub fn mesh_drain_dirty(
    mut world: ResMut<WorldReplica>,
    mut scheduler: ResMut<MeshScheduler>,
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
        if !world.is_generated(pos) {
            world.dirty_chunks.insert(pos);
            continue;
        }
        if !world.has_chunk(pos) {
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

pub fn circuit_flush_mesh(
    mut world: ResMut<WorldReplica>,
    mut scheduler: ResMut<MeshScheduler>,
    camera: Query<&Transform, With<crate::player::FlyCamera>>,
) {
    let Ok(cam) = camera.single() else { return };
    let dirty = world.take_dirty_chunks();
    for pos in dirty {
        if !world.is_generated(pos) {
            world.dirty_chunks.insert(pos);
            continue;
        }
        if world.has_chunk(pos) {
            scheduler.request(
                pos,
                RemeshUrgency::Circuit,
                chunk_distance_sq_from_camera(cam, pos),
            );
        } else {
            world.dirty_chunks.insert(pos);
        }
    }
}

pub fn request_face_neighbors(
    scheduler: &mut MeshScheduler,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::IoTaskPool;

    #[test]
    fn scheduling_respects_urgency() {
        let mut sched = MeshScheduler::default();
        let pos = ChunkPos { x: 0, y: 0, z: 0 };

        sched.request(pos, RemeshUrgency::Interactive, 0);
        sched.request(pos, RemeshUrgency::Background, 0);
        assert_eq!(
            sched.pop_next().unwrap().urgency,
            RemeshUrgency::Interactive
        );

        sched.request(ChunkPos { x: 1, y: 0, z: 0 }, RemeshUrgency::Circuit, 1);
        sched.request(pos, RemeshUrgency::Interactive, 100);
        assert_eq!(
            sched.pop_next().unwrap().urgency,
            RemeshUrgency::Interactive
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn stale_in_flight_mesh_discarded_after_rerequest() {
        IoTaskPool::get_or_init(|| bevy::tasks::TaskPoolBuilder::new().num_threads(1).build());

        let mut sched = MeshScheduler::default();
        let pos = ChunkPos { x: 0, y: 0, z: 0 };
        sched.mesh_version.insert(pos, 1);
        sched.in_progress.insert(pos);
        sched.mesh_in_flight = 1;
        sched.tasks.push(MeshInFlight {
            pos,
            expected_version: 1,
            task: IoTaskPool::get().spawn(async move { stagcrest_mesh::ChunkMesh::default() }),
        });

        sched.request(pos, RemeshUrgency::Circuit, 0);
        sched.poll();

        assert!(sched.ready_meshes.is_empty());
        assert_eq!(sched.mesh_version.get(&pos), Some(&2));
    }

    #[test]
    fn dispatch_defers_unfinished_chunk() {
        use crate::game::ModContext;
        use stagcrest_mod_client::{BlockRegistry, ModelRegistry, TextureAtlas};
        use stagcrest_protocol::{BlockId, BlockPos, BlockState};

        let mut sched = MeshScheduler::default();
        let pos = ChunkPos { x: 0, y: 0, z: 0 };
        sched.request(pos, RemeshUrgency::Visible, 0);

        let air = BlockId(0);
        let stone = BlockId(1);
        let mut world = stagcrest_world::World::new(air);
        world.set_blocks([(BlockPos::new(0, 0, 0), stone, BlockState(0))]);
        world.mark_chunk_terrain_ready(pos);
        assert!(world.has_chunk(pos));
        assert!(!world.is_generated(pos));

        let ctx = ModContext {
            registry: BlockRegistry::new(),
            atlas: TextureAtlas::build(std::iter::empty()),
            models: ModelRegistry::new(),
            colormaps: stagcrest_mod_client::ColormapSet {
                grass: vec![],
                grass_w: 1,
                grass_h: 1,
                foliage: vec![],
                foliage_w: 1,
                foliage_h: 1,
                water: vec![],
                water_w: 1,
                water_h: 1,
            },
            biomes: stagcrest_mod_client::BiomeRegistryClient::default(),
        };

        assert!(!sched.dispatch_one(&world, &ctx, None, None));
        assert!(sched.pending.contains_key(&pos));
    }
}
