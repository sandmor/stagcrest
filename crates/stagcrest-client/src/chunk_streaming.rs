//! Client-side chunk streaming caches and unload lifecycle.

use bevy::prelude::*;
use stagcrest_protocol::manifest::{BIOME_GRID_SIZE, BIOME_GRID_VOLUME};
use stagcrest_protocol::{BlockPos, ChunkPos};
use stagcrest_render::GpuChunkCache;
use std::collections::HashMap;

use crate::gpu_chunk_scheduler::GpuChunkScheduler;
use crate::world_replica::{CircuitPowerOverlay, WorldReplica};

#[derive(Event)]
pub struct ChunkUnloaded(pub ChunkPos);

#[derive(Resource, Default)]
pub struct BiomeGridCache {
    grids: HashMap<ChunkPos, [u8; BIOME_GRID_VOLUME]>,
}

impl BiomeGridCache {
    pub fn insert(&mut self, pos: ChunkPos, grid: [u8; BIOME_GRID_VOLUME]) {
        self.grids.insert(pos, grid);
    }

    pub fn remove(&mut self, pos: ChunkPos) {
        self.grids.remove(&pos);
    }

    pub fn biome_grid(&self, pos: ChunkPos) -> Option<&[u8; BIOME_GRID_VOLUME]> {
        self.grids.get(&pos)
    }

    pub fn biome_at(&self, block_pos: BlockPos) -> Option<u16> {
        let chunk = block_pos.chunk_pos();
        let grid = self.grids.get(&chunk)?;
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
}

pub fn on_chunk_unloaded_remove_biome(
    trigger: Trigger<ChunkUnloaded>,
    mut biome_cache: ResMut<BiomeGridCache>,
) {
    biome_cache.remove(trigger.event().0);
}

pub fn drop_chunk_assets(
    pos: ChunkPos,
    gpu_scheduler: &mut GpuChunkScheduler,
    gpu_cache: &mut GpuChunkCache,
    power: &mut CircuitPowerOverlay,
    commands: &mut Commands,
) {
    gpu_scheduler.cancel(pos);
    gpu_cache.remove(pos);
    power.0.retain(|block_pos, _| block_pos.chunk_pos() != pos);
    commands.trigger(ChunkUnloaded(pos));
}

pub fn unload_chunk(
    pos: ChunkPos,
    world: &mut WorldReplica,
    gpu_scheduler: &mut GpuChunkScheduler,
    gpu_cache: &mut GpuChunkCache,
    power: &mut CircuitPowerOverlay,
    commands: &mut Commands,
) {
    world.world.remove_chunk(pos);
    drop_chunk_assets(pos, gpu_scheduler, gpu_cache, power, commands);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_chunk_scheduler::RemeshUrgency;

    #[test]
    fn unload_drops_gpu_cache_and_triggers_event() {
        use bevy::ecs::world::CommandQueue;

        let mut world = WorldReplica::new(stagcrest_world::World::new(stagcrest_protocol::BlockId(
            0,
        )));
        let mut scheduler = GpuChunkScheduler::default();
        let mut cache = GpuChunkCache::default();
        let mut power = CircuitPowerOverlay::default();
        let mut bevy_world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &bevy_world);
        let pos = ChunkPos { x: 0, y: 0, z: 0 };
        cache.commit(stagcrest_render::gpu_voxel::types::GpuChunkUpload {
            pos,
            air: stagcrest_protocol::BlockId(0),
            blocks: [Default::default(); stagcrest_render::gpu_voxel::types::HALO_VOLUME],
            power: [0; stagcrest_render::gpu_voxel::types::CHUNK_BLOCKS],
            biome: [0; stagcrest_render::gpu_voxel::types::BIOME_GRID_CELLS],
            climate: None,
        });
        scheduler.request(pos, RemeshUrgency::Visible, 0);
        drop_chunk_assets(pos, &mut scheduler, &mut cache, &mut power, &mut commands);
        queue.apply(&mut bevy_world);
        assert!(!cache.contains(pos));
        assert!(!scheduler.is_pending(pos));
    }
}
