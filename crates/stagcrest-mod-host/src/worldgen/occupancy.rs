use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use stagcrest_protocol::{BlockId, BlockPos, BlockState, CHUNK_SIZE};
use std::collections::HashMap;

/// Tracks block occupancy during feature placement for a decorating chunk.
pub struct OccupancyMap {
    blocks: HashMap<BlockPos, BlockId>,
    air: BlockId,
    chunk_min: BlockPos,
    chunk_max: BlockPos,
}

impl OccupancyMap {
    pub fn from_surface_entries(
        surface_entries: &[(BlockPos, BlockId, BlockState)],
        chunk_pos: stagcrest_protocol::ChunkPos,
        air: BlockId,
    ) -> Self {
        let base_x = chunk_pos.x * CHUNK_SIZE;
        let base_y = chunk_pos.y * CHUNK_SIZE;
        let base_z = chunk_pos.z * CHUNK_SIZE;
        let chunk_min = BlockPos::new(base_x, base_y, base_z);
        let chunk_max = BlockPos::new(
            base_x + CHUNK_SIZE - 1,
            base_y + CHUNK_SIZE - 1,
            base_z + CHUNK_SIZE - 1,
        );

        let mut blocks = HashMap::new();
        for &(pos, id, _) in surface_entries {
            blocks.insert(pos, id);
        }

        Self {
            blocks,
            air,
            chunk_min,
            chunk_max,
        }
    }

    pub fn block_at(&self, snapshot: &DecorateSnapshot, pos: BlockPos) -> BlockId {
        if let Some(&id) = self.blocks.get(&pos) {
            return id;
        }
        if self.in_decorating_chunk(pos) {
            return self.air;
        }
        snapshot.block_at(pos)
    }

    pub fn can_place(&self, snapshot: &DecorateSnapshot, pos: BlockPos) -> bool {
        self.block_at(snapshot, pos) == self.air
    }

    pub fn place(&mut self, pos: BlockPos, id: BlockId) {
        self.blocks.insert(pos, id);
    }

    fn in_decorating_chunk(&self, pos: BlockPos) -> bool {
        pos.x >= self.chunk_min.x
            && pos.x <= self.chunk_max.x
            && pos.y >= self.chunk_min.y
            && pos.y <= self.chunk_max.y
            && pos.z >= self.chunk_min.z
            && pos.z <= self.chunk_max.z
    }
}
