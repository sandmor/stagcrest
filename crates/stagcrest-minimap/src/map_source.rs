use std::collections::{HashMap, HashSet};

use stagcrest_protocol::ChunkPos;
use stagcrest_storage::{BlockIdRemap, InactiveChunk, RedbChunkStorage, StorageError};
use stagcrest_world::World;

use crate::map_tile::WORLD_CHUNKS_PER_MAP;
use crate::strip::StorageStripSource;

/// Inputs for loading vertical strips covering one map chunk.
pub struct MapChunkLoadInput<'a> {
    pub storage: &'a RedbChunkStorage,
    pub world: Option<&'a World>,
    pub y_chunks: &'a [i32],
    pub mx: i32,
    pub mz: i32,
    pub air: stagcrest_protocol::BlockId,
    pub block_remap: Option<&'a BlockIdRemap>,
    pub overrides: &'a HashMap<ChunkPos, InactiveChunk>,
    pub modified_live: &'a HashSet<ChunkPos>,
}

fn prepare_chunk(mut chunk: InactiveChunk, remap: Option<&BlockIdRemap>) -> InactiveChunk {
    if let Some(remap) = remap {
        remap.remap_palette(&mut chunk.palette_ids);
    }
    chunk
}

/// Load the 4×4 world-chunk strip grid for map chunk `(mx, mz)`.
pub fn load_strips_for_map_chunk(
    input: &MapChunkLoadInput<'_>,
) -> Result<[[StorageStripSource; 4]; 4], StorageError> {
    let base_cx = input.mx * WORLD_CHUNKS_PER_MAP;
    let base_cz = input.mz * WORLD_CHUNKS_PER_MAP;
    let mut strips =
        std::array::from_fn(|_| std::array::from_fn(|_| StorageStripSource::new(0, 0, input.air)));

    for dz in 0..WORLD_CHUNKS_PER_MAP {
        for dx in 0..WORLD_CHUNKS_PER_MAP {
            let cx = base_cx + dx;
            let cz = base_cz + dz;
            let mut strip = StorageStripSource::new(cx, cz, input.air);
            for &cy in input.y_chunks {
                let pos = ChunkPos {
                    x: cx,
                    y: cy,
                    z: cz,
                };
                if let Some(chunk) = input.overrides.get(&pos) {
                    strip.insert(cy, prepare_chunk(chunk.clone(), input.block_remap));
                    continue;
                }
                if let Some(world) = input.world {
                    if world.has_chunk(pos) && world.is_populated(pos) {
                        if let Some(chunk) = world.pack_chunk_for_storage(pos) {
                            strip.insert(cy, prepare_chunk(chunk, input.block_remap));
                            continue;
                        }
                    }
                }
                if let Some(chunk) = input.storage.load_chunk(pos)? {
                    strip.insert(cy, prepare_chunk(chunk, input.block_remap));
                }
            }
            strips[dz as usize][dx as usize] = strip;
        }
    }
    Ok(strips)
}
