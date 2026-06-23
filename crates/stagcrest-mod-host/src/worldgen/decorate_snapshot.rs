use stagcrest_protocol::{BlockId, BlockPos, ChunkPos, CHUNK_SIZE};
use stagcrest_world::World;
use std::collections::HashMap;

/// Read-only context for offline chunk decoration (no live `World` on worker threads).
#[derive(Debug, Clone)]
pub struct DecorateSnapshot {
    pub air: BlockId,
    pub above_bottom_face: [BlockId; 256],
    pub below_top_face: [BlockId; 256],
    pub has_above: bool,
    pub has_below: bool,
    pub external_blocks: HashMap<BlockPos, BlockId>,
}

impl DecorateSnapshot {
    pub fn empty(air: BlockId) -> Self {
        Self {
            air,
            above_bottom_face: [air; 256],
            below_top_face: [air; 256],
            has_above: false,
            has_below: false,
            external_blocks: HashMap::new(),
        }
    }

    pub fn capture(world: &World, pos: ChunkPos, air: BlockId) -> Self {
        let base_x = pos.x * CHUNK_SIZE;
        let base_y = pos.y * CHUNK_SIZE;
        let base_z = pos.z * CHUNK_SIZE;

        let mut snapshot = Self::empty(air);

        let below = ChunkPos {
            x: pos.x,
            y: pos.y - 1,
            z: pos.z,
        };
        if world.has_chunk(below) {
            snapshot.has_below = true;
            let y = base_y - 1;
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let id = world
                        .get_block(BlockPos::new(base_x + lx, y, base_z + lz))
                        .0;
                    snapshot.below_top_face[(lz * CHUNK_SIZE + lx) as usize] = id;
                }
            }
        }

        let above = ChunkPos {
            x: pos.x,
            y: pos.y + 1,
            z: pos.z,
        };
        if world.has_chunk(above) {
            snapshot.has_above = true;
            let y = base_y + CHUNK_SIZE;
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let id = world
                        .get_block(BlockPos::new(base_x + lx, y, base_z + lz))
                        .0;
                    snapshot.above_bottom_face[(lz * CHUNK_SIZE + lx) as usize] = id;
                }
            }
        }

        for dy in 1..=2i32 {
            let neighbor = ChunkPos {
                x: pos.x,
                y: pos.y + dy,
                z: pos.z,
            };
            if !world.has_chunk(neighbor) {
                continue;
            }
            let ny_base = neighbor.y * CHUNK_SIZE;
            for ly in 0..CHUNK_SIZE {
                for lz in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let block_pos = BlockPos::new(
                            neighbor.x * CHUNK_SIZE + lx,
                            ny_base + ly,
                            neighbor.z * CHUNK_SIZE + lz,
                        );
                        let id = world.get_block(block_pos).0;
                        if id != air {
                            snapshot.external_blocks.insert(block_pos, id);
                        }
                    }
                }
            }
        }

        snapshot
    }

    pub fn block_at(&self, pos: BlockPos) -> BlockId {
        self.external_blocks.get(&pos).copied().unwrap_or(self.air)
    }

    pub fn face_above(&self, lx: i32, lz: i32) -> BlockId {
        if self.has_above {
            self.above_bottom_face[(lz * CHUNK_SIZE + lx) as usize]
        } else {
            self.air
        }
    }

    pub fn face_below(&self, lx: i32, lz: i32) -> BlockId {
        if self.has_below {
            self.below_top_face[(lz * CHUNK_SIZE + lx) as usize]
        } else {
            self.air
        }
    }
}
