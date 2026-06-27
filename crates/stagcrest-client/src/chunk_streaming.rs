//! Client-side chunk streaming caches and unload lifecycle.
//!
//! Per-chunk client state (meshes, biome grids, etc.) must be released through
//! [`drop_chunk_assets`] or [`unload_chunk`], or by observing [`ChunkUnloaded`].
//! Do not add ad-hoc per-chunk `HashMap`s to [`crate::world_replica::WorldReplica`].

use bevy::prelude::*;
use stagcrest_mesh::MeshCache;
use stagcrest_protocol::manifest::{BIOME_GRID_SIZE, BIOME_GRID_VOLUME};
use stagcrest_protocol::{BlockPos, ChunkPos};
use std::collections::HashMap;

use crate::mesh_scheduler::MeshScheduler;
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
    mesh_scheduler: &mut MeshScheduler,
    mesh_cache: &mut MeshCache,
    power: &mut CircuitPowerOverlay,
    commands: &mut Commands,
) {
    mesh_scheduler.cancel(pos);
    mesh_cache.remove(pos);
    power.0.retain(|block_pos, _| block_pos.chunk_pos() != pos);
    commands.trigger(ChunkUnloaded(pos));
}

pub fn unload_chunk(
    pos: ChunkPos,
    world: &mut WorldReplica,
    mesh_scheduler: &mut MeshScheduler,
    mesh_cache: &mut MeshCache,
    power: &mut CircuitPowerOverlay,
    commands: &mut Commands,
) {
    world.world.remove_chunk(pos);
    drop_chunk_assets(pos, mesh_scheduler, mesh_cache, power, commands);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_scheduler::RemeshUrgency;
    use stagcrest_protocol::BlockPos;

    #[test]
    fn drop_chunk_assets_releases_mesh_and_biome_caches() {
        let pos = ChunkPos { x: 0, y: 0, z: 0 };

        let mut app = App::new();
        app.add_event::<ChunkUnloaded>();
        app.init_resource::<BiomeGridCache>();
        app.add_observer(on_chunk_unloaded_remove_biome);
        app.world_mut()
            .resource_mut::<BiomeGridCache>()
            .insert(pos, [1u8; BIOME_GRID_VOLUME]);
        app.insert_resource(DropChunkAssetsOnce {
            pos,
            scheduler: {
                let mut scheduler = MeshScheduler::default();
                scheduler.request(pos, RemeshUrgency::Visible, 0);
                scheduler
            },
            mesh_cache: {
                let mut cache = MeshCache::default();
                cache.commit_mesh(pos, stagcrest_mesh::ChunkMesh::default());
                cache
            },
            power: {
                let mut overlay = CircuitPowerOverlay::default();
                overlay.0.insert(BlockPos::new(0, 0, 0), 7);
                overlay
            },
            ran: false,
        });
        app.add_systems(Update, run_drop_chunk_assets_once);
        app.update();

        let mut state = app.world_mut().resource_mut::<DropChunkAssetsOnce>();
        assert!(state.mesh_cache.get(pos).is_none());
        assert!(state.mesh_cache.take_dirty().contains(&pos));
        assert!(!state.power.0.contains_key(&BlockPos::new(0, 0, 0)));

        let biome_cache = app.world().resource::<BiomeGridCache>();
        assert!(biome_cache.biome_grid(pos).is_none());
    }

    #[derive(Resource)]
    struct DropChunkAssetsOnce {
        pos: ChunkPos,
        scheduler: MeshScheduler,
        mesh_cache: MeshCache,
        power: CircuitPowerOverlay,
        ran: bool,
    }

    fn run_drop_chunk_assets_once(
        mut state: ResMut<DropChunkAssetsOnce>,
        mut commands: Commands,
    ) {
        if state.ran {
            return;
        }
        state.ran = true;
        let pos = state.pos;
        let DropChunkAssetsOnce {
            ref mut scheduler,
            ref mut mesh_cache,
            ref mut power,
            ..
        } = &mut *state;
        drop_chunk_assets(pos, scheduler, mesh_cache, power, &mut commands);
    }
}
