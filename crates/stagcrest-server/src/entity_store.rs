//! Server-authoritative store of live entity instances plus spawn placement
//! and interest-filtered replication.

use std::collections::{HashMap, HashSet};

use stagcrest_net::{EntitySpawn, GameMessage, ServerMessage};
use stagcrest_protocol::{BlockId, ChunkPos, EntityId, EntityTypeId, CHUNK_SIZE};
use stagcrest_world::World;

use crate::client_session::{ClientRegistry, ConnectedClient};
use crate::interest::chunk_in_client_radius;

/// A live entity in the world. Movement/physics are out of scope for the MVP,
/// so instances are static after spawn; animation is driven client-side.
#[derive(Debug, Clone, Copy)]
pub struct EntityInstance {
    pub id: EntityId,
    pub type_id: EntityTypeId,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub anim: u8,
    pub chunk: ChunkPos,
}

/// Spawn tuning distilled from an entity type's registration.
#[derive(Debug, Clone, Copy)]
pub struct SpawnRule {
    pub type_id: EntityTypeId,
    pub chance: f32,
    pub max_per_chunk: u8,
}

#[derive(Default)]
pub struct EntityStore {
    next_id: u64,
    by_id: HashMap<EntityId, EntityInstance>,
    by_chunk: HashMap<ChunkPos, Vec<EntityId>>,
    populated: HashSet<ChunkPos>,
}

impl EntityStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Default::default()
        }
    }

    pub fn is_populated(&self, chunk: ChunkPos) -> bool {
        self.populated.contains(&chunk)
    }

    fn alloc_id(&mut self) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn spawn(
        &mut self,
        type_id: EntityTypeId,
        x: f32,
        y: f32,
        z: f32,
        yaw: f32,
        chunk: ChunkPos,
    ) -> EntityId {
        let id = self.alloc_id();
        let inst = EntityInstance {
            id,
            type_id,
            x,
            y,
            z,
            yaw,
            anim: 0,
            chunk,
        };
        self.by_id.insert(id, inst);
        self.by_chunk.entry(chunk).or_default().push(id);
        id
    }

    pub fn get(&self, id: EntityId) -> Option<&EntityInstance> {
        self.by_id.get(&id)
    }

    pub fn instances_in_chunk(&self, chunk: ChunkPos) -> impl Iterator<Item = &EntityInstance> {
        self.by_chunk
            .get(&chunk)
            .into_iter()
            .flatten()
            .filter_map(move |id| self.by_id.get(id))
    }

    /// Remove all entities in a chunk (returns their ids for despawn fanout).
    pub fn remove_chunk(&mut self, chunk: ChunkPos) -> Vec<EntityId> {
        self.populated.remove(&chunk);
        let ids = self.by_chunk.remove(&chunk).unwrap_or_default();
        for id in &ids {
            self.by_id.remove(id);
        }
        ids
    }

    /// Roll spawns for a freshly generated/loaded surface chunk. Idempotent per
    /// chunk (guarded by `populated`).
    pub fn populate_chunk(
        &mut self,
        world: &World,
        chunk: ChunkPos,
        rules: &[SpawnRule],
        air: BlockId,
        world_seed: u64,
    ) {
        if self.populated.contains(&chunk) || rules.is_empty() {
            return;
        }
        self.populated.insert(chunk);

        let base_x = chunk.x * CHUNK_SIZE;
        let base_y = chunk.y * CHUNK_SIZE;
        let base_z = chunk.z * CHUNK_SIZE;

        for rule in rules {
            let mut rng = SplitMix::from_chunk(world_seed, chunk, rule.type_id.0);
            if rng.next_f32() >= rule.chance {
                continue;
            }
            let count = 1 + (rng.next_u32() % rule.max_per_chunk.max(1) as u32);
            for _ in 0..count {
                let lx = (rng.next_u32() % CHUNK_SIZE as u32) as i32;
                let lz = (rng.next_u32() % CHUNK_SIZE as u32) as i32;
                let wx = base_x + lx;
                let wz = base_z + lz;
                let Some(surface_y) = surface_in_chunk(world, wx, base_y, wz, air) else {
                    continue;
                };
                let yaw = rng.next_f32() * std::f32::consts::TAU;
                self.spawn(
                    rule.type_id,
                    wx as f32 + 0.5,
                    (surface_y + 1) as f32,
                    wz as f32 + 0.5,
                    yaw,
                    chunk,
                );
            }
        }
    }

    /// Send `EntitySpawn`/`EntityDespawn` so each client's known set matches the
    /// entities inside its interest radius. Mirrors `fanout_block_update`.
    pub fn sync_clients(&self, clients: &mut ClientRegistry, h_radius: i32, v_radius: i32) {
        for client in clients.clients_mut() {
            if !client.handshake_complete || !client.stream.valid {
                continue;
            }
            self.sync_client(client, h_radius, v_radius);
        }
    }

    fn sync_client(&self, client: &mut ConnectedClient, h_radius: i32, v_radius: i32) {
        // Despawn entities no longer in range or gone.
        let stale: Vec<EntityId> = client
            .sent_entities
            .iter()
            .copied()
            .filter(|id| match self.by_id.get(id) {
                Some(inst) => !chunk_in_client_radius(inst.chunk, client, h_radius, v_radius),
                None => true,
            })
            .collect();
        for id in stale {
            client.sent_entities.remove(&id);
            client.queue_priority(GameMessage::Server(ServerMessage::EntityDespawn(id)));
        }

        // Spawn newly in-range entities.
        for inst in self.by_id.values() {
            if client.sent_entities.contains(&inst.id) {
                continue;
            }
            if chunk_in_client_radius(inst.chunk, client, h_radius, v_radius) {
                client.sent_entities.insert(inst.id);
                client.queue_priority(GameMessage::Server(ServerMessage::EntitySpawn(
                    EntitySpawn {
                        id: inst.id,
                        type_id: inst.type_id,
                        x: inst.x,
                        y: inst.y,
                        z: inst.z,
                        yaw: inst.yaw,
                    },
                )));
            }
        }
    }
}

/// Find the topmost solid block y in `chunk`'s column at (wx, wz) that has air
/// above it and open sky at the chunk top, i.e. a genuine surface chunk. Returns
/// `None` for fully-solid or fully-air columns (caves / above ground).
fn surface_in_chunk(world: &World, wx: i32, base_y: i32, wz: i32, air: BlockId) -> Option<i32> {
    use stagcrest_protocol::BlockPos;
    let top = base_y + CHUNK_SIZE - 1;
    // Require the chunk-top block to be air so we only spawn under open sky.
    if world.get_block(BlockPos::new(wx, top, wz)).0 != air {
        return None;
    }
    let mut y = top;
    while y >= base_y {
        let solid = world.get_block(BlockPos::new(wx, y, wz)).0 != air;
        if solid {
            // Need at least 2 air blocks of head room above.
            let a1 = world.get_block(BlockPos::new(wx, y + 1, wz)).0 == air;
            let a2 = world.get_block(BlockPos::new(wx, y + 2, wz)).0 == air;
            if a1 && a2 {
                return Some(y);
            }
            return None;
        }
        y -= 1;
    }
    None
}

/// Small deterministic RNG (SplitMix64) seeded per chunk + type.
struct SplitMix {
    state: u64,
}

impl SplitMix {
    fn from_chunk(seed: u64, chunk: ChunkPos, salt: u32) -> Self {
        let mut s = seed;
        s ^= (chunk.x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        s = s.rotate_left(17) ^ (chunk.y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        s = s.rotate_left(31) ^ (chunk.z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        s ^= (salt as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
        Self { state: s | 1 }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }
}
