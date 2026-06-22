use bevy::prelude::*;
use bevy::tasks::Task;
use stagcrest_mod_host::{ChunkGenData, ColumnBlocks, WorldGenState};
use stagcrest_protocol::{BlockPos, ChunkPos};
use std::collections::{HashSet, VecDeque};

const MAX_IN_FLIGHT: usize = 64;
const DISPATCH_PER_FRAME: usize = 32;
const APPLY_CHUNKS_PER_FRAME: usize = 12;

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

#[derive(Resource, Default)]
pub struct TerrainGenQueue {
    pending: VecDeque<ChunkPos>,
    in_progress: HashSet<ChunkPos>,
    in_flight: usize,
    tasks: Vec<Task<ChunkGenData>>,
    completed: VecDeque<ChunkGenData>,
    discarded: HashSet<ChunkPos>,
}

impl TerrainGenQueue {
    pub fn enqueue_chunk(&mut self, pos: ChunkPos, terrain: &WorldGenState) -> bool {
        if terrain.is_chunk_generated(pos)
            || self.in_progress.contains(&pos)
            || self.pending.iter().any(|&p| p == pos)
        {
            return false;
        }
        self.pending.push_back(pos);
        true
    }

    pub fn enqueue_area(
        &mut self,
        terrain: &WorldGenState,
        center: ChunkPos,
        horizontal_radius: i32,
        vertical_radius: i32,
        y_bounds: std::ops::RangeInclusive<i32>,
        player_block: BlockPos,
    ) {
        let y_min = (center.y - vertical_radius).max(*y_bounds.start());
        let y_max = (center.y + vertical_radius).min(*y_bounds.end());
        let mut candidates = Vec::new();
        for cx in (center.x - horizontal_radius)..=(center.x + horizontal_radius) {
            for cz in (center.z - horizontal_radius)..=(center.z + horizontal_radius) {
                for cy in y_min..=y_max {
                    let pos = ChunkPos { x: cx, y: cy, z: cz };
                    if self.enqueue_chunk_prepare(pos, terrain) {
                        let chunk_center_x = cx * stagcrest_protocol::CHUNK_SIZE + 8;
                        let chunk_center_y = cy * stagcrest_protocol::CHUNK_SIZE + 8;
                        let chunk_center_z = cz * stagcrest_protocol::CHUNK_SIZE + 8;
                        let dx = chunk_center_x - player_block.x;
                        let dy = chunk_center_y - player_block.y;
                        let dz = chunk_center_z - player_block.z;
                        candidates.push((dx * dx + dy * dy + dz * dz, -cy, pos));
                    }
                }
            }
        }
        candidates.sort_by_key(|(dist, neg_cy, _)| (*neg_cy, *dist));
        for (_, _, pos) in candidates {
            self.pending.push_back(pos);
        }
    }

    fn enqueue_chunk_prepare(&self, pos: ChunkPos, terrain: &WorldGenState) -> bool {
        !terrain.is_chunk_generated(pos)
            && !self.in_progress.contains(&pos)
            && !self.pending.iter().any(|&p| p == pos)
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len() + self.in_flight + self.completed.len()
    }

    pub fn cancel_chunk(&mut self, pos: ChunkPos) {
        self.pending.retain(|&p| p != pos);
        self.in_progress.remove(&pos);
        self.completed.retain(|d| d.pos != pos);
        self.discarded.insert(pos);
    }
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

pub fn terrain_dispatch(
    mut queue: ResMut<TerrainGenQueue>,
    terrain: Res<crate::game::TerrainGen>,
    blocks: Option<Res<TerrainBlocks>>,
) {
    let Some(blocks) = blocks else { return };
    let generator = terrain.0.generator();

    for _ in 0..DISPATCH_PER_FRAME {
        if queue.in_flight >= MAX_IN_FLIGHT {
            break;
        }
        let Some(pos) = queue.pending.pop_front() else {
            break;
        };
        if terrain.0.is_chunk_generated(pos) {
            continue;
        }
        queue.in_progress.insert(pos);

        let gen = generator.clone();
        let column_blocks = blocks.0;
        queue.in_flight += 1;
        queue.tasks.push(bevy::tasks::IoTaskPool::get().spawn(async move {
            gen.compute_chunk_density(column_blocks, pos)
        }));
    }
}

pub fn terrain_poll_tasks(mut queue: ResMut<TerrainGenQueue>) {
    let mut dropped = 0usize;
    let mut results = Vec::new();
    queue.tasks.retain_mut(|task| {
        if !task.is_finished() {
            return true;
        }
        dropped += 1;
        if let Some(data) = futures_lite::future::block_on(futures_lite::future::poll_once(task)) {
            results.push(data);
        }
        false
    });
    queue.in_flight = queue.in_flight.saturating_sub(dropped);

    let mut finished = Vec::new();
    for data in results {
        let pos = data.pos;
        queue.in_progress.remove(&pos);
        if queue.discarded.remove(&pos) {
            continue;
        }
        finished.push(data);
    }
    finished.sort_by_key(|d| std::cmp::Reverse(d.pos.y));
    queue.completed.extend(finished);
}

pub fn terrain_apply(
    mut queue: ResMut<TerrainGenQueue>,
    mut terrain: ResMut<crate::game::TerrainGen>,
    mut world: ResMut<crate::game::StagcrestWorldResource>,
    stream: Option<Res<TerrainStreamState>>,
    config: Option<Res<crate::game::GameConfig>>,
    blocks: Res<TerrainBlocks>,
    biomes: Res<TerrainBiomes>,
) {
    let h_radius = config.as_ref().map(|c| c.render_distance).unwrap_or(i32::MAX);
    let v_radius = config
        .as_ref()
        .map(|c| c.vertical_render_distance)
        .unwrap_or(i32::MAX);

    let mut batch: Vec<ChunkGenData> = Vec::new();
    while batch.len() < APPLY_CHUNKS_PER_FRAME {
        let Some(data) = queue.completed.pop_front() else {
            break;
        };
        batch.push(data);
    }
    batch.sort_by_key(|d| std::cmp::Reverse(d.pos.y));

    let generator = terrain.0.generator().clone();
    let y_bounds = stagcrest_mod_host::world_chunk_y_bounds(terrain.0.config());
    let mut deferred = Vec::new();

    for data in batch {
        let pos = data.pos;
        queue.in_progress.remove(&pos);

        if let Some(stream) = stream.as_ref() {
            if stream.valid && !chunk_in_stream(pos, stream, h_radius, v_radius) {
                deferred.push(data);
                continue;
            }
        }

        if pos.y < *y_bounds.end() {
            let above = ChunkPos {
                x: pos.x,
                y: pos.y + 1,
                z: pos.z,
            };
            // Only wait for the chunk above when it is loaded in the world (inside vertical
            // stream). Otherwise we'd deadlock: cy=9 needs cy=10, but cy=10 is outside v_radius.
            if world.0.chunk(above).is_some() && !terrain.0.is_chunk_generated(above) {
                deferred.push(data);
                continue;
            }
        }

        if !terrain.0.mark_chunk_generated(pos) {
            continue;
        }

        let entries = generator.decorate_chunk(&world.0, blocks.0, &biomes.0, &data);
        world.0.set_blocks(entries);
    }

    deferred.sort_by_key(|d| std::cmp::Reverse(d.pos.y));
    for data in deferred.into_iter().rev() {
        queue.completed.push_front(data);
    }
}
