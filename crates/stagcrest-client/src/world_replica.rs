use bevy::prelude::*;
use stagcrest_net::{BlockUpdate, ChunkSnapshot, ServerMessage};
use stagcrest_protocol::BlockPos;
use stagcrest_storage::{decompress_stored, InactiveChunk};
use stagcrest_world::World;

use crate::mesh_scheduler::{MeshScheduler, RemeshUrgency};

#[derive(Resource)]
pub struct WorldReplica(pub World);

impl WorldReplica {
    pub fn apply_server_message(
        &mut self,
        msg: ServerMessage,
        mesh_scheduler: &mut MeshScheduler,
    ) {
        match msg {
            ServerMessage::ChunkSnapshot(snapshot) => {
                self.apply_chunk_snapshot(snapshot, mesh_scheduler);
            }
            ServerMessage::ChunkUnload(pos) => {
                self.0.remove_chunk(pos);
                mesh_scheduler.cancel(pos);
            }
            ServerMessage::BlockUpdate(update) => {
                self.apply_block_update(update, mesh_scheduler);
            }
            _ => {}
        }
    }

    fn apply_chunk_snapshot(&mut self, snapshot: ChunkSnapshot, mesh_scheduler: &mut MeshScheduler) {
        let Ok(wire) = decompress_stored(&snapshot.compressed) else {
            tracing::warn!("failed to decompress chunk {:?}", snapshot.pos);
            return;
        };
        let Ok(inactive) = InactiveChunk::decode_wire(&wire) else {
            tracing::warn!("failed to decode chunk {:?}", snapshot.pos);
            return;
        };
        self.0.insert_inactive_chunk(snapshot.pos, inactive);
        self.0.finalize_generated_chunk(snapshot.pos);
        if self.0.has_chunk(snapshot.pos) {
            mesh_scheduler.request(snapshot.pos, RemeshUrgency::Visible, 0);
        }
    }

    fn apply_block_update(&mut self, update: BlockUpdate, mesh_scheduler: &mut MeshScheduler) {
        self.0
            .set_block(update.pos, update.id, update.state);
        mesh_scheduler.request(
            update.pos.chunk_pos(),
            RemeshUrgency::Interactive,
            0,
        );
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
