use bevy::prelude::*;
use stagcrest_net::{BlockUpdate, ChunkSnapshot, ServerMessage};
use stagcrest_protocol::manifest::BIOME_GRID_VOLUME;
use stagcrest_storage::{decompress_stored, InactiveChunk};
use stagcrest_world::World;

use crate::chunk_streaming::{
    drop_chunk_assets, unload_chunk, BiomeGridCache,
};
use crate::mesh_scheduler::{MeshScheduler, RemeshUrgency};

#[derive(Resource)]
pub struct WorldReplica {
    pub world: World,
}

impl WorldReplica {
    pub fn new(world: World) -> Self {
        Self { world }
    }

    pub fn apply_server_message(
        &mut self,
        msg: ServerMessage,
        mesh_scheduler: &mut MeshScheduler,
        mesh_cache: &mut stagcrest_mesh::MeshCache,
        biome_cache: &mut BiomeGridCache,
        power: &mut CircuitPowerOverlay,
        commands: &mut Commands,
    ) {
        match msg {
            ServerMessage::ChunkSnapshot(snapshot) => {
                self.apply_chunk_snapshot(
                    snapshot,
                    mesh_scheduler,
                    mesh_cache,
                    biome_cache,
                    power,
                    commands,
                );
            }
            ServerMessage::ChunkUnload(pos) => {
                unload_chunk(
                    pos,
                    self,
                    mesh_scheduler,
                    mesh_cache,
                    power,
                    commands,
                );
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
        mesh_cache: &mut stagcrest_mesh::MeshCache,
        biome_cache: &mut BiomeGridCache,
        power: &mut CircuitPowerOverlay,
        commands: &mut Commands,
    ) {
        let Ok(wire) = decompress_stored(&snapshot.compressed) else {
            tracing::warn!("failed to decompress chunk {:?}", snapshot.pos);
            return;
        };
        let Ok(inactive) = InactiveChunk::decode_wire(&wire) else {
            tracing::warn!("failed to decode chunk {:?}", snapshot.pos);
            return;
        };
        let insert_result = self.world.insert_inactive_chunk(snapshot.pos, inactive);
        if let Some(evicted) = insert_result.lru_evicted {
            drop_chunk_assets(
                evicted,
                mesh_scheduler,
                mesh_cache,
                power,
                commands,
            );
        }
        self.world.finalize_generated_chunk(snapshot.pos);
        if let Some(grid) = snapshot.biome_grid {
            if grid.len() == BIOME_GRID_VOLUME {
                let mut arr = [0u8; BIOME_GRID_VOLUME];
                arr.copy_from_slice(&grid);
                biome_cache.insert(snapshot.pos, arr);
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
use stagcrest_protocol::BlockPos;

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
