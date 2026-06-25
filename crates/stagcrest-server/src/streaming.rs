use std::collections::{HashSet, VecDeque};

use stagcrest_mod_server::{
    world_chunk_y_bounds, BiomeRegistry, ChunkGenData, ColumnBlocks, DecorateSnapshot,
    TerrainGenerator, WorldGenState,
};
use stagcrest_net::ChunkSnapshot;
use stagcrest_protocol::{BlockId, BlockPos, ChunkPos, CHUNK_SIZE};
use stagcrest_storage::{compress_stored, load_inactive_chunk};
use stagcrest_world::World;

use crate::session::WorldSession;

const MAX_GEN_PER_TICK: usize = 2;
const MAX_IO_PER_TICK: usize = 2;

#[derive(Clone, Copy, Default)]
pub struct TerrainStreamState {
    pub center_x: i32,
    pub center_y: i32,
    pub center_z: i32,
    pub valid: bool,
}

#[derive(Default)]
pub struct StreamingPipeline {
    pending_generate: VecDeque<ChunkPos>,
    pending_load: VecDeque<ChunkPos>,
    pending_persist: VecDeque<(ChunkPos, Vec<u8>)>,
    deferred_decorate: VecDeque<ChunkGenData>,
    pass2_pending: std::collections::HashMap<ChunkPos, ChunkGenData>,
    pub(crate) sent_to_client: HashSet<ChunkPos>,
}

impl StreamingPipeline {
    pub fn tick(
        &mut self,
        world: &mut World,
        terrain: &mut WorldGenState,
        session: &mut WorldSession,
        column_blocks: ColumnBlocks,
        biomes: &BiomeRegistry,
        generator: &TerrainGenerator,
        stream: &TerrainStreamState,
        last_center: &mut Option<ChunkPos>,
        air: BlockId,
        h_radius: i32,
        v_radius: i32,
    ) -> StreamingTickResult {
        let mut result = StreamingTickResult::default();
        if !stream.valid {
            return result;
        }

        let center = ChunkPos {
            x: stream.center_x,
            y: stream.center_y,
            z: stream.center_z,
        };
        let player_block = BlockPos::new(
            stream.center_x * CHUNK_SIZE,
            stream.center_y * CHUNK_SIZE,
            stream.center_z * CHUNK_SIZE,
        );
        let y_bounds = world_chunk_y_bounds(terrain.config());
        let unload_h = h_radius + 2;
        let unload_v = v_radius + 2;

        let (evicted, _, _) = world.unload_far_chunks_3d(center, unload_h, unload_v);
        for evicted_chunk in evicted {
            let pos = evicted_chunk.pos;
            if evicted_chunk.meta.is_populated() {
                if let Some(inactive) = evicted_chunk.inactive {
                    let wire = inactive.encode_wire();
                    let compressed = compress_stored(&wire);
                    self.pending_persist.push_back((pos, compressed));
                }
            }
            self.clear_work(pos);
            if self.sent_to_client.remove(&pos) {
                result.unloads.push(pos);
            }
        }

        let center_changed = *last_center != Some(center);
        if center_changed {
            self.prune_out_of_stream(stream, h_radius, v_radius);
            *last_center = Some(center);
        }

        if center_changed || self.generation_backlog() < 64 {
            self.enqueue_area(
                terrain,
                &mut session.stored_chunks,
                session.storage.as_ref(),
                world,
                center,
                h_radius,
                v_radius,
                y_bounds.clone(),
                player_block,
            );
        }

        for pos in world.loaded_chunk_positions() {
            if !chunk_in_stream(pos, stream, h_radius, v_radius) {
                continue;
            }
            if !world.is_generated(pos) || self.sent_to_client.contains(&pos) {
                continue;
            }
            if let Some(snapshot) = chunk_snapshot(world, pos) {
                self.sent_to_client.insert(pos);
                result.snapshots.push(snapshot);
            }
        }

        for _ in 0..MAX_IO_PER_TICK {
            if let Some((pos, compressed)) = self.pending_persist.pop_front() {
                if let Err(err) = session.storage.put(pos, &compressed) {
                    tracing::error!("failed to persist chunk {pos:?}: {err}");
                } else {
                    session.stored_chunks.insert(pos);
                }
            }
        }

        for _ in 0..MAX_IO_PER_TICK {
            if let Some(pos) = self.pending_load.pop_front() {
                match load_inactive_chunk(session.storage.as_ref(), pos) {
                    Ok(Some(chunk)) => {
                        world.insert_inactive_chunk(pos, chunk);
                        terrain.mark_chunk_generated(pos);
                        session.stored_chunks.insert(pos);
                        self.clear_work(pos);
                        if self.sent_to_client.insert(pos) {
                            if let Some(snapshot) = chunk_snapshot(world, pos) {
                                result.snapshots.push(snapshot);
                            }
                        }
                    }
                    Ok(None) => {
                        session.stored_chunks.remove(&pos);
                        self.enqueue_generate(pos, terrain, world);
                    }
                    Err(err) => {
                        tracing::error!("failed to load chunk {pos:?}: {err}");
                        self.pending_load.push_back(pos);
                    }
                }
            }
        }

        for _ in 0..MAX_GEN_PER_TICK {
            if let Some(data) = self.deferred_decorate.pop_front() {
                let pos = data.pos;
                let snapshot = DecorateSnapshot::capture(world, pos, air);
                let entries =
                    generator.decorate_chunk_offline(column_blocks, biomes, &data, &snapshot);
                terrain.mark_chunk_generated(pos);
                if !world.is_generated(pos) {
                    world.set_blocks(entries);
                    world.finalize_generated_chunk(pos);
                }
                self.pass2_pending.remove(&pos);
                if self.sent_to_client.insert(pos) {
                    if let Some(snapshot) = chunk_snapshot(world, pos) {
                        result.snapshots.push(snapshot);
                    }
                }
                continue;
            }

            if let Some(pos) = self.pending_generate.pop_front() {
                if world.has_chunk(pos)
                    && (terrain.is_chunk_generated(pos)
                        || world.is_generated(pos)
                        || terrain.is_chunk_terrain_ready(pos)
                        || world.is_terrain_ready(pos))
                {
                    continue;
                }
                let data = generator.compute_chunk_density(column_blocks, pos);
                self.pass2_pending.insert(pos, data.clone());
                let _ = terrain.mark_chunk_terrain_ready(pos);
                if !world.is_generated(pos) {
                    world.set_blocks(data.entries.clone());
                    world.mark_chunk_terrain_ready(pos);
                }
                if !terrain.is_chunk_generated(pos) {
                    self.deferred_decorate.push_back(data);
                }
            }
        }

        result
    }

    fn enqueue_generate(
        &mut self,
        pos: ChunkPos,
        terrain: &mut WorldGenState,
        world: &World,
    ) -> bool {
        if world.has_chunk(pos) {
            if terrain.is_chunk_generated(pos) || terrain.is_chunk_terrain_ready(pos) {
                return false;
            }
        } else {
            terrain.clear_chunk(pos);
        }
        if self.pending_generate.iter().any(|&p| p == pos)
            || self.deferred_decorate.iter().any(|d| d.pos == pos)
        {
            return false;
        }
        self.pending_load.retain(|&p| p != pos);
        self.pending_generate.push_back(pos);
        true
    }

    fn enqueue_load(&mut self, pos: ChunkPos) -> bool {
        if self.pending_load.iter().any(|&p| p == pos) {
            return false;
        }
        self.clear_work(pos);
        self.pending_load.push_back(pos);
        true
    }

    fn clear_work(&mut self, pos: ChunkPos) {
        self.pending_generate.retain(|&p| p != pos);
        self.deferred_decorate.retain(|d| d.pos != pos);
        self.pass2_pending.remove(&pos);
    }

    fn generation_backlog(&self) -> usize {
        self.pending_generate.len() + self.deferred_decorate.len() + self.pending_load.len()
    }

    pub fn enqueue_area(
        &mut self,
        terrain: &mut WorldGenState,
        stored_chunks: &mut HashSet<ChunkPos>,
        storage: &dyn stagcrest_storage::ChunkStorage,
        world: &World,
        center: ChunkPos,
        horizontal_radius: i32,
        vertical_radius: i32,
        y_bounds: std::ops::RangeInclusive<i32>,
        player_block: BlockPos,
    ) {
        let y_min = (center.y - vertical_radius).max(*y_bounds.start());
        let y_cap = (*y_bounds.end()).max(center.y);
        let y_max = (center.y + vertical_radius).min(y_cap);
        if y_min > y_max {
            return;
        }

        let mut gen_candidates = Vec::new();
        let mut load_candidates = Vec::new();
        for cx in (center.x - horizontal_radius)..=(center.x + horizontal_radius) {
            for cz in (center.z - horizontal_radius)..=(center.z + horizontal_radius) {
                for cy in y_min..=y_max {
                    let pos = ChunkPos { x: cx, y: cy, z: cz };
                    let chunk_center_x = cx * CHUNK_SIZE + 8;
                    let chunk_center_y = cy * CHUNK_SIZE + 8;
                    let chunk_center_z = cz * CHUNK_SIZE + 8;
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
        world: &World,
    ) -> bool {
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
        world: &World,
    ) -> bool {
        if world.has_chunk(pos) {
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

    fn prune_out_of_stream(
        &mut self,
        stream: &TerrainStreamState,
        horizontal_radius: i32,
        vertical_radius: i32,
    ) {
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

#[derive(Default)]
pub struct StreamingTickResult {
    pub snapshots: Vec<ChunkSnapshot>,
    pub unloads: Vec<ChunkPos>,
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

fn chunk_snapshot(world: &World, pos: ChunkPos) -> Option<ChunkSnapshot> {
    let inactive = world.pack_chunk_for_storage(pos)?;
    let wire = inactive.encode_wire();
    Some(ChunkSnapshot {
        pos,
        compressed: compress_stored(&wire),
    })
}