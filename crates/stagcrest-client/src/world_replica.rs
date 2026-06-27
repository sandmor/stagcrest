use bevy::prelude::*;
use stagcrest_net::{BlockUpdate, ChunkSnapshot, ServerMessage};
use stagcrest_protocol::manifest::{BIOME_GRID_SIZE, BIOME_GRID_VOLUME};
use stagcrest_protocol::{BlockPos, ChunkPos};
use stagcrest_storage::{decompress_stored, InactiveChunk};
use stagcrest_world::World;
use std::collections::HashMap;

use crate::mesh_scheduler::{MeshScheduler, RemeshUrgency};

#[derive(Resource)]
pub struct WorldReplica {
    pub world: World,
    pub biome_grids: HashMap<ChunkPos, [u8; BIOME_GRID_VOLUME]>,
}

impl WorldReplica {
    pub fn new(world: World) -> Self {
        Self {
            world,
            biome_grids: HashMap::new(),
        }
    }

    pub fn biome_grid(&self, pos: ChunkPos) -> Option<&[u8; BIOME_GRID_VOLUME]> {
        self.biome_grids.get(&pos)
    }

    pub fn biome_at(&self, block_pos: BlockPos) -> Option<u16> {
        let chunk = block_pos.chunk_pos();
        let grid = self.biome_grids.get(&chunk)?;
        let lx = block_pos.x.rem_euclid(stagcrest_protocol::CHUNK_SIZE);
        let ly = block_pos.y.rem_euclid(stagcrest_protocol::CHUNK_SIZE);
        let lz = block_pos.z.rem_euclid(stagcrest_protocol::CHUNK_SIZE);
        let cell = stagcrest_protocol::CHUNK_SIZE / BIOME_GRID_SIZE as i32;
        let gx = (lx / cell).clamp(0, 3) as usize;
        let gy = (ly / cell).clamp(0, 3) as usize;
        let gz = (lz / cell).clamp(0, 3) as usize;
        let idx = gx + gy * 4 + gz * 16;
        Some(grid[idx] as u16)
    }

    pub fn apply_server_message(&mut self, msg: ServerMessage, mesh_scheduler: &mut MeshScheduler) {
        match msg {
            ServerMessage::ChunkSnapshot(snapshot) => {
                self.apply_chunk_snapshot(snapshot, mesh_scheduler);
            }
            ServerMessage::ChunkUnload(pos) => {
                self.world.remove_chunk(pos);
                self.biome_grids.remove(&pos);
                mesh_scheduler.cancel(pos);
            }
            ServerMessage::BlockUpdate(update) => {
                self.apply_block_update(update, mesh_scheduler);
            }
            _ => {}
        }
    }

    fn apply_chunk_snapshot(
        &mut self,
        snapshot: ChunkSnapshot,
        mesh_scheduler: &mut MeshScheduler,
    ) {
        let Ok(wire) = decompress_stored(&snapshot.compressed) else {
            tracing::warn!("failed to decompress chunk {:?}", snapshot.pos);
            return;
        };
        let Ok(inactive) = InactiveChunk::decode_wire(&wire) else {
            tracing::warn!("failed to decode chunk {:?}", snapshot.pos);
            return;
        };
        self.world.insert_inactive_chunk(snapshot.pos, inactive);
        self.world.finalize_generated_chunk(snapshot.pos);
        if let Some(grid) = snapshot.biome_grid {
            if grid.len() == BIOME_GRID_VOLUME {
                let mut arr = [0u8; BIOME_GRID_VOLUME];
                arr.copy_from_slice(&grid);
                self.biome_grids.insert(snapshot.pos, arr);
            }
        }
        if self.world.has_chunk(snapshot.pos) {
            mesh_scheduler.request(snapshot.pos, RemeshUrgency::Visible, 0);
        }
    }

    fn apply_block_update(&mut self, update: BlockUpdate, mesh_scheduler: &mut MeshScheduler) {
        self.world.set_block(update.pos, update.id, update.state);
        mesh_scheduler.request(update.pos.chunk_pos(), RemeshUrgency::Interactive, 0);
    }
}

impl std::ops::Deref for WorldReplica {
    type Target = World;
    fn deref(&self) -> &Self::Target {
        &self.world
    }
}

impl std::ops::DerefMut for WorldReplica {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.world
    }
}

use stagcrest_mod_client::PowerLookup;

/// Power levels from server for redstone dust tinting.
#[derive(Resource, Default)]
pub struct CircuitPowerOverlay(pub std::collections::HashMap<BlockPos, u8>);

impl PowerLookup for CircuitPowerOverlay {
    fn power_at(&self, pos: BlockPos) -> u8 {
        self.0.get(&pos).copied().unwrap_or(0)
    }
}

pub fn apply_power_batch(
    overlay: &mut CircuitPowerOverlay,
    updates: Vec<(BlockPos, u8)>,
    mesh_scheduler: &mut MeshScheduler,
) {
    for (pos, power) in updates {
        overlay.0.insert(pos, power);
        mesh_scheduler.request(pos.chunk_pos(), RemeshUrgency::Circuit, 0);
    }
}
