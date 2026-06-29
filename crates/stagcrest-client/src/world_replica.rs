use bevy::prelude::*;
use stagcrest_net::{BlockUpdate, ChunkSnapshot, ServerMessage};
use stagcrest_storage::{decompress_stored, InactiveChunk};
use stagcrest_world::World;

use crate::chunk_streaming::{drop_chunk_assets, unload_chunk, BiomeGridCache};
use crate::gpu_chunk_scheduler::{request_face_neighbors, GpuChunkScheduler, RemeshUrgency};
use stagcrest_render::GpuChunkCache;

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
        gpu_scheduler: &mut GpuChunkScheduler,
        gpu_cache: &mut GpuChunkCache,
        biome_cache: &mut BiomeGridCache,
        power: &mut CircuitPowerOverlay,
        commands: &mut Commands,
    ) {
        match msg {
            ServerMessage::ChunkSnapshot(snapshot) => {
                self.apply_chunk_snapshot(
                    snapshot,
                    gpu_scheduler,
                    gpu_cache,
                    biome_cache,
                    power,
                    commands,
                );
            }
            ServerMessage::ChunkUnload(pos) => {
                unload_chunk(pos, self, gpu_scheduler, gpu_cache, power, commands);
            }
            ServerMessage::BlockUpdate(update) => {
                self.apply_block_update(update, gpu_scheduler, gpu_cache);
            }
            _ => {}
        }
    }

    fn apply_chunk_snapshot(
        &mut self,
        snapshot: ChunkSnapshot,
        gpu_scheduler: &mut GpuChunkScheduler,
        gpu_cache: &mut GpuChunkCache,
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
        let biome_from_chunk = inactive.biome_grid;
        let insert_result = self.world.insert_inactive_chunk(snapshot.pos, inactive);
        if let Some(evicted) = insert_result.lru_evicted {
            drop_chunk_assets(evicted, gpu_scheduler, gpu_cache, power, commands);
        }
        self.world.finalize_generated_chunk(snapshot.pos);
        biome_cache.insert(snapshot.pos, biome_from_chunk);
        if self.world.has_chunk(snapshot.pos) {
            gpu_scheduler.request(snapshot.pos, RemeshUrgency::Visible, 0);
        }
    }

    fn apply_block_update(
        &mut self,
        update: BlockUpdate,
        gpu_scheduler: &mut GpuChunkScheduler,
        gpu_cache: &mut GpuChunkCache,
    ) {
        self.world.set_block(update.pos, update.id, update.state);
        let chunk = update.pos.chunk_pos();
        gpu_scheduler.request(chunk, RemeshUrgency::Interactive, 0);
        gpu_cache.mark_dirty(chunk);
        request_face_neighbors(
            gpu_scheduler,
            &mut self.world,
            chunk,
            RemeshUrgency::Interactive,
            |_| 0,
            false,
        );
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

/// Power levels from server for wire line tinting.
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
    gpu_scheduler: &mut GpuChunkScheduler,
) {
    for (pos, power) in updates {
        if power == 0 {
            overlay.0.remove(&pos);
        } else {
            overlay.0.insert(pos, power);
        }
        gpu_scheduler.request(pos.chunk_pos(), RemeshUrgency::Circuit, 0);
    }
}
