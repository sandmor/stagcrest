use stagcrest_mesh::{capture_power_grid, MeshClimateSnapshot, MeshSnapshot};
use stagcrest_mod_client::{
    build_chunk_tint_cells, model_variant_for_block, BiomeRegistryClient, BlockRegistry,
    ColormapSet, ModelRegistry, PowerLookup,
};
use stagcrest_protocol::{
    BlockId, BlockPos, ChunkPos, LocalBlockPos, CHUNK_SIZE, CHUNK_VOLUME,
};
use stagcrest_storage::InactiveChunkReader;
use stagcrest_world::{ChunkBlock, World};
use std::collections::HashMap;
use std::sync::Arc;

use crate::gpu_voxel::types::{GpuBlockCell, GpuChunkTintCell, GpuChunkUpload, GpuClimateData, HALO_VOLUME};

fn halo_filled_with_air(air: BlockId) -> [GpuBlockCell; HALO_VOLUME] {
    let cell = GpuBlockCell {
        id: air.0,
        state: 0,
        variant: 0,
        _pad: 0,
    };
    [cell; HALO_VOLUME]
}

pub fn capture_gpu_chunk_upload(
    pos: ChunkPos,
    world: &World,
    registry: &BlockRegistry,
    power: Option<&dyn PowerLookup>,
    climate: Option<MeshClimateSnapshot>,
    biomes: &BiomeRegistryClient,
    colormaps: &ColormapSet,
) -> Option<GpuChunkUpload> {
    let snapshot = MeshSnapshot::capture(
        pos,
        world,
        Arc::new(registry.clone()),
        Arc::new(ModelRegistry::new()),
        capture_power_grid(pos, power),
        climate,
    )?;
    Some(pack_gpu_chunk_upload(
        &snapshot,
        registry,
        biomes,
        colormaps,
    ))
}

pub fn pack_gpu_chunk_upload(
    snapshot: &MeshSnapshot,
    registry: &BlockRegistry,
    biomes: &BiomeRegistryClient,
    colormaps: &ColormapSet,
) -> GpuChunkUpload {
    let mut blocks = halo_filled_with_air(snapshot.air);

    for ly in -1..=CHUNK_SIZE {
        for lz in -1..=CHUNK_SIZE {
            for lx in -1..=CHUNK_SIZE {
                let Some(block) = snapshot.block_at(lx, ly, lz) else {
                    continue;
                };
                let idx = super::types::halo_index(lx, ly, lz);
                let variant = if let Some(def) = registry.block(block.id) {
                    model_variant_for_block(&def.namespaced_id, block.state) as u16
                } else {
                    0
                };
                blocks[idx] = GpuBlockCell {
                    id: block.id.0,
                    state: block.state.0 as u32,
                    variant: variant as u32,
                    _pad: 0,
                };
            }
        }
    }

    let climate = snapshot.climate.as_ref().map(|c| GpuClimateData {
        temperature: c.temperature,
        downfall: c.downfall,
    });

    let tint_cells = build_chunk_tint_cells(&snapshot.center.biome_grid, biomes, colormaps)
        .map(GpuChunkTintCell::from);

    GpuChunkUpload {
        pos: snapshot.pos,
        air: snapshot.air,
        blocks,
        power: snapshot.power_grid,
        biome: snapshot.center.biome_grid,
        climate,
        tint_cells,
    }
}

pub fn pack_halo_from_world(
    pos: ChunkPos,
    world: &World,
    air: BlockId,
    registry: &BlockRegistry,
    center: &stagcrest_storage::InactiveChunk,
    power_grid: [u8; CHUNK_VOLUME],
    biomes: &BiomeRegistryClient,
    colormaps: &ColormapSet,
) -> GpuChunkUpload {
    let halo = capture_halo(world, pos, air);
    let mut blocks = halo_filled_with_air(air);
    let reader = InactiveChunkReader::new(center);

    for ly in -1..=CHUNK_SIZE {
        for lz in -1..=CHUNK_SIZE {
            for lx in -1..=CHUNK_SIZE {
                let block = if (0..CHUNK_SIZE).contains(&lx)
                    && (0..CHUNK_SIZE).contains(&ly)
                    && (0..CHUNK_SIZE).contains(&lz)
                {
                    let local = LocalBlockPos {
                        x: lx as u8,
                        y: ly as u8,
                        z: lz as u8,
                    };
                    let (id, state) = reader.block_at(local);
                    ChunkBlock { id, state }
                } else {
                    let wx = pos.x * CHUNK_SIZE + lx;
                    let wy = pos.y * CHUNK_SIZE + ly;
                    let wz = pos.z * CHUNK_SIZE + lz;
                    halo.get(&BlockPos::new(wx, wy, wz)).copied().unwrap_or(ChunkBlock {
                        id: air,
                        state: stagcrest_protocol::BlockState(0),
                    })
                };
                if block.id == air {
                    continue;
                }
                let idx = super::types::halo_index(lx, ly, lz);
                let variant = registry
                    .block(block.id)
                    .map(|def| model_variant_for_block(&def.namespaced_id, block.state) as u16)
                    .unwrap_or(0);
                blocks[idx] = GpuBlockCell {
                    id: block.id.0,
                    state: block.state.0 as u32,
                    variant: variant as u32,
                    _pad: 0,
                };
            }
        }
    }

    let tint_cells = build_chunk_tint_cells(&center.biome_grid, biomes, colormaps)
        .map(GpuChunkTintCell::from);

    GpuChunkUpload {
        pos,
        air,
        blocks,
        power: power_grid,
        biome: center.biome_grid,
        climate: None,
        tint_cells,
    }
}

fn capture_halo(
    world: &World,
    pos: ChunkPos,
    air: BlockId,
) -> HashMap<BlockPos, ChunkBlock> {
    let base_x = pos.x * CHUNK_SIZE;
    let base_y = pos.y * CHUNK_SIZE;
    let base_z = pos.z * CHUNK_SIZE;
    let mut halo = HashMap::new();
    for lx in -1..=CHUNK_SIZE {
        for ly in -1..=CHUNK_SIZE {
            for lz in -1..=CHUNK_SIZE {
                if lx >= 0
                    && lx < CHUNK_SIZE
                    && ly >= 0
                    && ly < CHUNK_SIZE
                    && lz >= 0
                    && lz < CHUNK_SIZE
                {
                    continue;
                }
                let wx = base_x + lx;
                let wy = base_y + ly;
                let wz = base_z + lz;
                if let Some(block) = world.get_block_if_loaded(BlockPos::new(wx, wy, wz)) {
                    if block.id != air {
                        halo.insert(BlockPos::new(wx, wy, wz), block);
                    }
                }
            }
        }
    }
    halo
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::BlockId;

    #[test]
    fn halo_buffer_prefills_air_not_zero() {
        let air = BlockId(42);
        let blocks = halo_filled_with_air(air);
        assert!(blocks.iter().all(|c| c.id == 42));
    }
}
