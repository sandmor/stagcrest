use bevy::prelude::*;
use bevy::tasks::Task;
#[cfg(target_arch = "wasm32")]
use bevy::tasks::IoTaskPool;
#[cfg(not(target_arch = "wasm32"))]
use bevy::tasks::AsyncComputeTaskPool;
use stagcrest_mesh::{
    build_chunk_mesh_snapshot, capture_power_grid, MeshClimateSnapshot, MeshSnapshot,
};
use stagcrest_protocol::{BlockPos, ChunkPos, CHUNK_SIZE};
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
            chunk_in_stream(pos, center_x, center_y, center_z, horizontal_radius, vertical_radius)
        });
        self.mesh_version.retain(|pos, _| {
            chunk_in_stream(*pos, center_x, center_y, center_z, horizontal_radius, vertical_radius)
        });
        let out_of_stream: Vec<_> = self
            .in_progress
            .iter()
            .copied()
            .filter(|pos| {
                !chunk_in_stream(*pos, center_x, center_y, center_z, horizontal_radius, vertical_radius)
            })
            .collect();
        for pos in out_of_stream {
            self.cancel(pos);
        }
    }

    fn pop_next(&mut self) -> Option<MeshRequest> {
        while let Some(req) = self.heap.pop() {
            let Some(current) = self.pending.get(&req.pos) else {
                continue;
            };
            if current.version != req.version
                || current.urgency != req.urgency
                || current.distance_sq != req.distance_sq
            {
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
        circuit: Option<&crate::game::CircuitResource>,
        colormaps: Option<&crate::game::WorldColormaps>,
        terrain: Option<&crate::game::TerrainGen>,
    ) -> bool {
        if self.mesh_in_flight >= MAX_MESH_IN_FLIGHT {
            return false;
        }

        let Some(req) = self.pop_next() else {
            return false;
        };

        if self.in_progress.contains(&req.pos) {
            self.heap.push(req);
            return false;
        }

        if Self::counts_toward_dispatch_cap(req.urgency)
            && self.background_dispatched_this_frame >= MAX_BACKGROUND_DISPATCH_PER_FRAME
        {
            self.heap.push(req);
            return false;
        }

        if !world.has_chunk(req.pos) {
            self.pending.remove(&req.pos);
            return true;
        }

        let Some(snapshot) = capture_mesh_snapshot(
            req.pos,
            world,
            ctx,
            circuit,
            colormaps,
            terrain,
        ) else {
            self.heap.push(req);
            return false;
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
        true
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
    #[cfg(not(target_arch = "wasm32"))]
    {
        AsyncComputeTaskPool::get()
    }
    #[cfg(target_arch = "wasm32")]
    {
        IoTaskPool::get()
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

fn capture_mesh_snapshot(
    pos: ChunkPos,
    world: &stagcrest_world::World,
    ctx: &crate::game::ModContext,
    circuit: Option<&crate::game::CircuitResource>,
    colormaps: Option<&crate::game::WorldColormaps>,
    terrain: Option<&crate::game::TerrainGen>,
) -> Option<MeshSnapshot> {
    let power = circuit.map(|c| &c.0 as &dyn stagcrest_mod_host::PowerLookup);
    let power_grid = capture_power_grid(pos, power);
    let climate = colormaps.zip(terrain).map(|(maps, gen)| {
        let generator = gen.0.generator();
        MeshClimateSnapshot {
            colormaps: Arc::new(maps.0.clone()),
            config: generator.config.clone(),
            seed: generator.seed,
            noise: generator.noise_bank().clone(),
        }
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
    world: Res<crate::game::StagcrestWorldResource>,
    circuit: Option<Res<crate::game::CircuitResource>>,
    colormaps: Option<Res<crate::game::WorldColormaps>>,
    terrain: Option<Res<crate::game::TerrainGen>>,
) {
    let Some(ctx) = mod_ctx else { return };
    scheduler.background_dispatched_this_frame = 0;
    let mut spins = 0;
    while spins < 64 && scheduler.dispatch_one(
        &world.0,
        &ctx,
        circuit.as_deref(),
        colormaps.as_deref(),
        terrain.as_deref(),
    ) {
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

pub fn mesh_drain_dirty(
    mut world: ResMut<crate::game::StagcrestWorldResource>,
    mut scheduler: ResMut<MeshScheduler>,
    camera: Query<&Transform, With<crate::player::FlyCamera>>,
) {
    let Ok(cam) = camera.single() else { return };
    let dirty = world.0.take_dirty_chunks();
    let mut scheduled = 0usize;
    for pos in dirty {
        if scheduled >= MAX_DIRTY_DRAIN_PER_FRAME {
            world.0.dirty_chunks.insert(pos);
            continue;
        }
        if !world.0.has_chunk(pos) {
            world.0.dirty_chunks.insert(pos);
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
    mut world: ResMut<crate::game::StagcrestWorldResource>,
    mut scheduler: ResMut<MeshScheduler>,
    camera: Query<&Transform, With<crate::player::FlyCamera>>,
) {
    let Ok(cam) = camera.single() else { return };
    let dirty = world.0.take_dirty_chunks();
    for pos in dirty {
        if world.0.has_chunk(pos) {
            scheduler.request(
                pos,
                RemeshUrgency::Circuit,
                chunk_distance_sq_from_camera(cam, pos),
            );
        } else {
            world.0.dirty_chunks.insert(pos);
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
}
