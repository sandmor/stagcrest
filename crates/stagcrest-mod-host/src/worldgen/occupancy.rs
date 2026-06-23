use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use stagcrest_protocol::{BlockId, BlockPos, BlockState};
use std::collections::HashMap;

/// Tracks block occupancy during feature placement for a decorating chunk.
pub struct OccupancyMap {
    blocks: HashMap<BlockPos, BlockId>,
    air: BlockId,
}

impl OccupancyMap {
    pub fn from_surface_entries(
        surface_entries: &[(BlockPos, BlockId, BlockState)],
        chunk_pos: stagcrest_protocol::ChunkPos,
        air: BlockId,
    ) -> Self {
        let _ = chunk_pos;
        let mut blocks = HashMap::new();
        for &(pos, id, _) in surface_entries {
            blocks.insert(pos, id);
        }

        Self { blocks, air }
    }

    pub fn block_at(&self, snapshot: &DecorateSnapshot, pos: BlockPos) -> BlockId {
        if let Some(&id) = self.blocks.get(&pos) {
            return id;
        }
        snapshot.block_at(pos)
    }

    pub fn can_place(&self, snapshot: &DecorateSnapshot, pos: BlockPos) -> bool {
        self.block_at(snapshot, pos) == self.air
    }

    pub fn place(&mut self, pos: BlockPos, id: BlockId) {
        self.blocks.insert(pos, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::decorate_snapshot::DecorateSnapshot;
    use crate::worldgen::test_fixtures::test_blocks;

    #[test]
    fn sees_neighbor_cross_chunk_blocks_via_snapshot() {
        let blocks = test_blocks();
        let mut snapshot = DecorateSnapshot::empty(blocks.air);
        let cross_chunk_leaf = BlockPos::new(8, 70, 8);
        snapshot
            .local_chunk_blocks
            .insert(cross_chunk_leaf, blocks.oak_leaves);

        let occ = OccupancyMap::from_surface_entries(
            &[],
            stagcrest_protocol::ChunkPos { x: 0, y: 4, z: 0 },
            blocks.air,
        );
        assert!(!occ.can_place(&snapshot, cross_chunk_leaf));
    }
}
