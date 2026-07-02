use std::collections::HashSet;

use stagcrest_protocol::{ChunkPos, CHUNK_SIZE};
use stagcrest_storage::RedbChunkStorage;

use crate::map_tile::WORLD_CHUNKS_PER_MAP;

/// Source of per-chunk revision counters used for map layer freshness.
pub trait ChunkRevisionSource {
    fn get_chunk_revision(&self, pos: ChunkPos) -> u64;
}

impl ChunkRevisionSource for RedbChunkStorage {
    fn get_chunk_revision(&self, pos: ChunkPos) -> u64 {
        RedbChunkStorage::get_chunk_revision(self, pos).unwrap_or(0)
    }
}

/// Max revision across world chunks contributing to map layer `(mx, mz)` over `[min_y, max_y]`.
pub fn compute_layer_source_revision(
    storage: &impl ChunkRevisionSource,
    mx: i32,
    mz: i32,
    min_y: i32,
    max_y: i32,
    y_chunks: &[i32],
    modified_live: &HashSet<ChunkPos>,
) -> u64 {
    let base_cx = mx * WORLD_CHUNKS_PER_MAP;
    let base_cz = mz * WORLD_CHUNKS_PER_MAP;
    let mut max_rev = 0u64;
    for dz in 0..WORLD_CHUNKS_PER_MAP {
        for dx in 0..WORLD_CHUNKS_PER_MAP {
            let cx = base_cx + dx;
            let cz = base_cz + dz;
            for &cy in y_chunks {
                let chunk_y0 = cy * CHUNK_SIZE;
                let chunk_y1 = chunk_y0 + CHUNK_SIZE - 1;
                if chunk_y1 < min_y || chunk_y0 > max_y {
                    continue;
                }
                let pos = ChunkPos { x: cx, y: cy, z: cz };
                let rev = storage.get_chunk_revision(pos);
                max_rev = max_rev.max(if modified_live.contains(&pos) {
                    rev.saturating_add(1)
                } else {
                    rev
                });
            }
        }
    }
    max_rev
}

/// True when any populated live chunk in the map tile has unsaved edits.
pub fn map_tile_has_live_edits(
    world: &stagcrest_world::World,
    mx: i32,
    mz: i32,
    y_chunks: &[i32],
) -> bool {
    let base_cx = mx * WORLD_CHUNKS_PER_MAP;
    let base_cz = mz * WORLD_CHUNKS_PER_MAP;
    for dz in 0..WORLD_CHUNKS_PER_MAP {
        for dx in 0..WORLD_CHUNKS_PER_MAP {
            let cx = base_cx + dx;
            let cz = base_cz + dz;
            for &cy in y_chunks {
                let pos = ChunkPos { x: cx, y: cy, z: cz };
                if world.has_chunk(pos) && world.is_populated(pos) && world.is_modified(pos) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeRev(HashMap<ChunkPos, u64>);

    impl ChunkRevisionSource for FakeRev {
        fn get_chunk_revision(&self, pos: ChunkPos) -> u64 {
            self.0.get(&pos).copied().unwrap_or(0)
        }
    }

    #[test]
    fn max_revision_across_columns() {
        let mut revs = HashMap::new();
        revs.insert(ChunkPos { x: 0, y: 0, z: 0 }, 3);
        revs.insert(ChunkPos { x: 1, y: 0, z: 0 }, 7);
        let storage = FakeRev(revs);
        let y_chunks = vec![0];
        let rev = compute_layer_source_revision(
            &storage,
            0,
            0,
            0,
            256,
            &y_chunks,
            &HashSet::new(),
        );
        assert_eq!(rev, 7);
    }

    #[test]
    fn live_modified_chunk_bumps_revision() {
        let mut revs = HashMap::new();
        revs.insert(ChunkPos { x: 0, y: 0, z: 0 }, 3);
        let storage = FakeRev(revs);
        let y_chunks = vec![0];
        let mut modified = HashSet::new();
        modified.insert(ChunkPos { x: 0, y: 0, z: 0 });
        let rev = compute_layer_source_revision(
            &storage,
            0,
            0,
            0,
            256,
            &y_chunks,
            &modified,
        );
        assert_eq!(rev, 4);
    }
}
