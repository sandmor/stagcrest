use std::collections::{HashMap, HashSet, VecDeque};

use stagcrest_circuit::CircuitWorld;
use stagcrest_mod_server::{
    world_chunk_y_bounds, BiomeRegistry, BlockRegistry, ChunkGenData, ColumnBlocks,
    TerrainGenerator, WorldGenState,
};
use stagcrest_net::ChunkSnapshot;
use stagcrest_protocol::{BlockId, BlockPos, ChunkPos, CHUNK_SIZE};
use stagcrest_storage::{load_inactive_chunk, BlockIdRemap};
use stagcrest_world::{EvictedChunk, World};

use crate::chunk_gen::{apply_pass1_density, apply_pass2_decorate};
use crate::client_session::{ClientId, ConnectedClient};
use crate::interest::chunk_in_client_radius;
use crate::persistence::{compressed_chunk_wire, pack_network_chunk, persist_evicted_chunk, remap_loaded_chunk};
use crate::session::WorldSession;

const MAX_GEN_PER_TICK: usize = 2;
const MAX_IO_PER_TICK: usize = 2;
pub const MAX_CHUNK_SNAPSHOTS_PER_CLIENT_PER_TICK: usize = 4;

#[derive(Clone, Copy, Default, Debug)]
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
    deferred_decorate: VecDeque<ChunkGenData>,
    pass2_pending: std::collections::HashMap<ChunkPos, ChunkGenData>,
}

impl StreamingPipeline {
    pub fn tick(
        &mut self,
        world: &mut World,
        terrain: &mut WorldGenState,
        circuit: &mut CircuitWorld,
        session: &mut WorldSession,
        column_blocks: ColumnBlocks,
        biomes: &BiomeRegistry,
        registry: &BlockRegistry,
        generator: &TerrainGenerator,
        clients: &mut [ConnectedClient],
        air: BlockId,
        h_radius: i32,
        v_radius: i32,
        enqueue_rotate: usize,
        fair_rotate: usize,
        block_remap: Option<&BlockIdRemap>,
    ) -> StreamingTickResult {
        let mut result = StreamingTickResult::default();
        if !clients
            .iter()
            .any(|c| c.handshake_complete && c.stream.valid)
        {
            return result;
        }

        session
            .persistence
            .absorb_dirty_chunks(circuit.drain_dirty_chunks());

        let y_bounds = world_chunk_y_bounds(terrain.config());
        let unload_h = h_radius + 2;
        let unload_v = v_radius + 2;

        let evicted = unload_outside_all_clients(world, clients, unload_h, unload_v);
        for evicted_chunk in evicted {
            let pos = evicted_chunk.pos;
            if evicted_chunk.meta.is_populated() && evicted_chunk.meta.modified {
                match persist_evicted_chunk(
                    &session.persistence,
                    evicted_chunk,
                    terrain,
                    circuit,
                    &mut session.stored_chunks,
                ) {
                    Ok(Some(persisted_pos)) => result.persisted.push(persisted_pos),
                    Ok(None) => {}
                    Err(err) => {
                        tracing::error!("failed to persist evicted chunk {pos:?}: {err}");
                        session.persistence.mark_dirty(pos);
                    }
                }
            }
            self.clear_work(pos);
            for client in clients.iter_mut() {
                if client.sent_chunks.remove(&pos) {
                    result
                        .per_client
                        .entry(client.id)
                        .or_default()
                        .unloads
                        .push(pos);
                }
            }
        }

        let mut any_center_changed = false;
        for client in clients.iter_mut() {
            if !client.handshake_complete || !client.stream.valid {
                continue;
            }
            let center = ChunkPos {
                x: client.stream.center_x,
                y: client.stream.center_y,
                z: client.stream.center_z,
            };
            if client.last_center != Some(center) {
                any_center_changed = true;
                client.last_center = Some(center);
            }
        }

        if any_center_changed {
            self.prune_out_of_union_stream(clients, h_radius, v_radius);
        }

        if any_center_changed || self.generation_backlog() < 64 {
            self.enqueue_for_clients(
                terrain,
                &mut session.stored_chunks,
                session.storage.as_ref(),
                world,
                clients,
                h_radius,
                v_radius,
                y_bounds.clone(),
                enqueue_rotate,
            );
        }

        let mut snapshot_budget: HashMap<ClientId, usize> = HashMap::new();
        for client in clients.iter() {
            if client.handshake_complete {
                snapshot_budget.insert(client.id, MAX_CHUNK_SNAPSHOTS_PER_CLIENT_PER_TICK);
            }
        }

        for pos in world.loaded_chunk_positions().collect::<Vec<_>>() {
            if !world.is_generated(pos) {
                continue;
            }
            let needs_any = clients.iter().any(|c| {
                c.handshake_complete
                    && chunk_in_client_radius(pos, c, h_radius, v_radius)
                    && !c.sent_chunks.contains(&pos)
            });
            if !needs_any {
                continue;
            }
            let Some(snapshot) = chunk_snapshot(world, pos, terrain, circuit) else {
                continue;
            };
            offer_snapshot_to_clients(
                pos,
                snapshot,
                clients,
                &mut result,
                h_radius,
                v_radius,
                &mut snapshot_budget,
            );
        }

        let persisted = session.persistence.drain(
            MAX_IO_PER_TICK,
            world,
            terrain,
            circuit,
            &mut session.stored_chunks,
        );
        result.persisted.extend(persisted);

        for _ in 0..MAX_IO_PER_TICK {
            let Some(pos) = self.pop_fair_load(clients, h_radius, v_radius, fair_rotate) else {
                break;
            };
            match load_inactive_chunk(session.storage.as_ref(), pos) {
                Ok(Some(mut chunk)) => {
                    if let Some(remap) = block_remap {
                        remap_loaded_chunk(&mut chunk, remap);
                    }
                    let biome = chunk.biome_grid;
                    let circuit_snap = chunk.circuit.clone();
                    world.insert_inactive_chunk(pos, chunk);
                    terrain.store_biome_grid(pos, biome);
                    circuit.import_chunk_snapshot(pos, circuit_snap, session.meta.circuit_tick);
                    terrain.mark_chunk_generated(pos);
                    session.stored_chunks.insert(pos);
                    self.clear_work(pos);
                    if let Some(snapshot) = chunk_snapshot(world, pos, terrain, circuit) {
                        offer_snapshot_to_clients(
                            pos,
                            snapshot,
                            clients,
                            &mut result,
                            h_radius,
                            v_radius,
                            &mut snapshot_budget,
                        );
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

        for _ in 0..MAX_GEN_PER_TICK {
            if let Some(data) = self.deferred_decorate.pop_front() {
                let pos = data.pos;
                apply_pass2_decorate(
                    world,
                    terrain,
                    generator,
                    column_blocks,
                    biomes,
                    registry,
                    air,
                    &data,
                );
                self.pass2_pending.remove(&pos);
                if let Some(snapshot) = chunk_snapshot(world, pos, terrain, circuit) {
                    offer_snapshot_to_clients(
                        pos,
                        snapshot,
                        clients,
                        &mut result,
                        h_radius,
                        v_radius,
                        &mut snapshot_budget,
                    );
                }
                continue;
            }

            if let Some(pos) = self.pop_fair_generate(clients, h_radius, v_radius, fair_rotate) {
                if world.has_chunk(pos)
                    && (terrain.is_chunk_generated(pos)
                        || world.is_generated(pos)
                        || terrain.is_chunk_terrain_ready(pos)
                        || world.is_terrain_ready(pos))
                {
                    continue;
                }
                let data = apply_pass1_density(world, terrain, generator, column_blocks, pos);
                self.pass2_pending.insert(pos, data.clone());
                if !terrain.is_chunk_generated(pos) {
                    self.deferred_decorate.push_back(data);
                }
            }
        }

        result
    }

    fn enqueue_for_clients(
        &mut self,
        terrain: &mut WorldGenState,
        stored_chunks: &mut HashSet<ChunkPos>,
        storage: &dyn stagcrest_storage::ChunkStorage,
        world: &World,
        clients: &[ConnectedClient],
        h_radius: i32,
        v_radius: i32,
        y_bounds: std::ops::RangeInclusive<i32>,
        rotate_start: usize,
    ) {
        let eligible: Vec<_> = clients
            .iter()
            .filter(|c| c.handshake_complete && c.stream.valid)
            .collect();
        if eligible.is_empty() {
            return;
        }
        let n = eligible.len();
        for offset in 0..n {
            let client = eligible[(rotate_start + offset) % n];
            let center = ChunkPos {
                x: client.stream.center_x,
                y: client.stream.center_y,
                z: client.stream.center_z,
            };
            let player_block = BlockPos::new(
                client.stream.center_x * CHUNK_SIZE,
                client.stream.center_y * CHUNK_SIZE,
                client.stream.center_z * CHUNK_SIZE,
            );
            self.enqueue_area(
                terrain,
                stored_chunks,
                storage,
                world,
                center,
                h_radius,
                v_radius,
                y_bounds.clone(),
                player_block,
            );
        }
    }

    fn pop_fair_load(
        &mut self,
        clients: &[ConnectedClient],
        h_radius: i32,
        v_radius: i32,
        fair_rotate: usize,
    ) -> Option<ChunkPos> {
        let eligible: Vec<_> = clients
            .iter()
            .filter(|c| c.handshake_complete && c.stream.valid)
            .collect();
        if eligible.is_empty() {
            return self.pending_load.pop_front();
        }
        let n = eligible.len();
        for offset in 0..n {
            let client = eligible[(fair_rotate + offset) % n];
            if let Some(idx) = self
                .pending_load
                .iter()
                .position(|pos| chunk_in_client_radius(*pos, client, h_radius, v_radius))
            {
                return self.pending_load.remove(idx);
            }
        }
        self.pending_load.pop_front()
    }

    fn pop_fair_generate(
        &mut self,
        clients: &[ConnectedClient],
        h_radius: i32,
        v_radius: i32,
        fair_rotate: usize,
    ) -> Option<ChunkPos> {
        let eligible: Vec<_> = clients
            .iter()
            .filter(|c| c.handshake_complete && c.stream.valid)
            .collect();
        if eligible.is_empty() {
            return self.pending_generate.pop_front();
        }
        let n = eligible.len();
        for offset in 0..n {
            let client = eligible[(fair_rotate + offset) % n];
            if let Some(idx) = self
                .pending_generate
                .iter()
                .position(|pos| chunk_in_client_radius(*pos, client, h_radius, v_radius))
            {
                return self.pending_generate.remove(idx);
            }
        }
        self.pending_generate.pop_front()
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
                    let pos = ChunkPos {
                        x: cx,
                        y: cy,
                        z: cz,
                    };
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

    fn prune_out_of_union_stream(
        &mut self,
        clients: &[ConnectedClient],
        horizontal_radius: i32,
        vertical_radius: i32,
    ) {
        self.pending_generate.retain(|&pos| {
            chunk_needed_by_any_client(pos, clients, horizontal_radius, vertical_radius)
        });
        self.pending_load.retain(|&pos| {
            chunk_needed_by_any_client(pos, clients, horizontal_radius, vertical_radius)
        });
        self.deferred_decorate.retain(|data| {
            chunk_needed_by_any_client(data.pos, clients, horizontal_radius, vertical_radius)
        });
        self.pass2_pending.retain(|pos, _| {
            chunk_needed_by_any_client(*pos, clients, horizontal_radius, vertical_radius)
        });
    }
}

#[derive(Default)]
pub struct ClientStreamDelta {
    pub snapshots: Vec<ChunkSnapshot>,
    pub unloads: Vec<ChunkPos>,
}

#[derive(Default)]
pub struct StreamingTickResult {
    pub per_client: HashMap<ClientId, ClientStreamDelta>,
    pub persisted: Vec<ChunkPos>,
}

fn chunk_needed_by_any_client(
    pos: ChunkPos,
    clients: &[ConnectedClient],
    h_radius: i32,
    v_radius: i32,
) -> bool {
    clients
        .iter()
        .any(|c| c.handshake_complete && chunk_in_client_radius(pos, c, h_radius, v_radius))
}

fn unload_outside_all_clients(
    world: &mut World,
    clients: &[ConnectedClient],
    unload_h: i32,
    unload_v: i32,
) -> Vec<EvictedChunk> {
    let active: Vec<_> = clients
        .iter()
        .filter(|c| c.handshake_complete && c.stream.valid)
        .collect();
    if active.is_empty() {
        return Vec::new();
    }

    let positions: Vec<_> = world
        .loaded_chunk_positions()
        .filter(|pos| {
            !active
                .iter()
                .any(|c| chunk_in_client_radius(*pos, c, unload_h, unload_v))
        })
        .collect();

    let mut evicted = Vec::with_capacity(positions.len());
    for pos in positions {
        if let Some((inactive, meta)) = world.remove_chunk(pos) {
            evicted.push(EvictedChunk {
                pos,
                inactive,
                meta,
            });
        }
    }
    evicted
}

fn offer_snapshot_to_clients(
    pos: ChunkPos,
    snapshot: ChunkSnapshot,
    clients: &mut [ConnectedClient],
    result: &mut StreamingTickResult,
    h_radius: i32,
    v_radius: i32,
    budget: &mut HashMap<ClientId, usize>,
) {
    for client in clients.iter_mut() {
        if !client.handshake_complete {
            continue;
        }
        if !chunk_in_client_radius(pos, client, h_radius, v_radius) {
            continue;
        }
        if client.sent_chunks.contains(&pos) {
            continue;
        }
        let Some(remaining) = budget.get_mut(&client.id) else {
            continue;
        };
        if *remaining == 0 {
            continue;
        }
        *remaining -= 1;
        client.sent_chunks.insert(pos);
        result
            .per_client
            .entry(client.id)
            .or_default()
            .snapshots
            .push(snapshot.clone());
    }
}

fn chunk_snapshot(
    world: &World,
    pos: ChunkPos,
    terrain: &WorldGenState,
    circuit: &CircuitWorld,
) -> Option<ChunkSnapshot> {
    let chunk = pack_network_chunk(world, terrain, circuit, pos)?;
    Some(ChunkSnapshot {
        pos,
        compressed: compressed_chunk_wire(&chunk),
        biome_grid: Some(chunk.biome_grid.to_vec()),
    })
}
