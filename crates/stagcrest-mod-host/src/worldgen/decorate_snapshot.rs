use stagcrest_protocol::{BlockId, BlockPos, ChunkPos, CHUNK_SIZE};
use stagcrest_world::World;
use std::collections::HashMap;

/// Max horizontal reach for cross-chunk tree canopies (trunk + branch + leaves).
const TREE_CROSS_CHUNK_REACH: i32 = 8;

/// Read-only context for offline chunk decoration (no live `World` on worker threads).
#[derive(Debug, Clone)]
pub struct DecorateSnapshot {
    pub air: BlockId,
    pub chunk_pos: Option<ChunkPos>,
    /// Non-air blocks already in the decorating chunk (terrain + cross-chunk feature writes).
    pub local_chunk_blocks: HashMap<BlockPos, BlockId>,
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
            chunk_pos: None,
            local_chunk_blocks: HashMap::new(),
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
        snapshot.chunk_pos = Some(pos);

        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let block_pos = BlockPos::new(base_x + lx, base_y + ly, base_z + lz);
                    let id = world.get_block(block_pos).0;
                    if id != air {
                        snapshot.local_chunk_blocks.insert(block_pos, id);
                    }
                }
            }
        }

        let below = ChunkPos {
            x: pos.x,
            y: pos.y - 1,
            z: pos.z,
        };
        if world.is_terrain_ready(below) {
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
        if world.is_terrain_ready(above) {
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
            if !world.is_terrain_ready(neighbor) {
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

        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            capture_horizontal_neighbor_shell(world, pos, dx, dz, air, &mut snapshot);
        }

        snapshot
    }

    pub fn block_at(&self, pos: BlockPos) -> BlockId {
        if let Some(&id) = self.local_chunk_blocks.get(&pos) {
            return id;
        }
        if let Some(&id) = self.external_blocks.get(&pos) {
            return id;
        }
        if let Some(chunk_pos) = self.chunk_pos {
            let base_x = chunk_pos.x * CHUNK_SIZE;
            let base_y = chunk_pos.y * CHUNK_SIZE;
            let base_z = chunk_pos.z * CHUNK_SIZE;
            if self.has_above
                && pos.y == base_y + CHUNK_SIZE
                && pos.x >= base_x
                && pos.x < base_x + CHUNK_SIZE
                && pos.z >= base_z
                && pos.z < base_z + CHUNK_SIZE
            {
                return self.face_above(pos.x - base_x, pos.z - base_z);
            }
            if self.has_below
                && pos.y == base_y - 1
                && pos.x >= base_x
                && pos.x < base_x + CHUNK_SIZE
                && pos.z >= base_z
                && pos.z < base_z + CHUNK_SIZE
            {
                return self.face_below(pos.x - base_x, pos.z - base_z);
            }
        }
        self.air
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

fn capture_horizontal_neighbor_shell(
    world: &World,
    pos: ChunkPos,
    dx: i32,
    dz: i32,
    air: BlockId,
    snapshot: &mut DecorateSnapshot,
) {
    let neighbor = ChunkPos {
        x: pos.x + dx,
        y: pos.y,
        z: pos.z + dz,
    };
    if !world.is_terrain_ready(neighbor) {
        return;
    }

    let nx_base = neighbor.x * CHUNK_SIZE;
    let ny_base = neighbor.y * CHUNK_SIZE;
    let nz_base = neighbor.z * CHUNK_SIZE;
    let reach = TREE_CROSS_CHUNK_REACH;

    let lx_range = if dx > 0 {
        0..reach.min(CHUNK_SIZE)
    } else if dx < 0 {
        (CHUNK_SIZE - reach).max(0)..CHUNK_SIZE
    } else {
        0..CHUNK_SIZE
    };
    let lz_range = if dz > 0 {
        0..reach.min(CHUNK_SIZE)
    } else if dz < 0 {
        (CHUNK_SIZE - reach).max(0)..CHUNK_SIZE
    } else {
        0..CHUNK_SIZE
    };

    for ly in 0..CHUNK_SIZE {
        for lz in lz_range.clone() {
            for lx in lx_range.clone() {
                let block_pos = BlockPos::new(nx_base + lx, ny_base + ly, nz_base + lz);
                let id = world.get_block(block_pos).0;
                if id != air {
                    snapshot.external_blocks.insert(block_pos, id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::test_fixtures::test_registry;
    use stagcrest_world::World;

    #[test]
    fn block_at_uses_local_chunk_and_face_rows() {
        let reg = test_registry();
        let air = reg.block_by_name("stagcrest:air").unwrap();
        let stone = reg.block_by_name("stagcrest:stone").unwrap();
        let leaves = reg.block_by_name("stagcrest:oak_leaves").unwrap();
        let mut world = World::new(air);
        let pos = ChunkPos { x: 0, y: 4, z: 0 };
        world.ensure_chunk(pos);
        let base_y = pos.y * CHUNK_SIZE;
        let neighbor_leaf = BlockPos::new(3, base_y + 10, 3);
        world.set_block(neighbor_leaf, leaves, stagcrest_protocol::BlockState(0));
        world.mark_chunk_terrain_ready(pos);

        let snapshot = DecorateSnapshot::capture(&world, pos, air);
        assert_eq!(snapshot.block_at(neighbor_leaf), leaves);

        let above = ChunkPos {
            x: pos.x,
            y: pos.y + 1,
            z: pos.z,
        };
        world.ensure_chunk(above);
        let face_pos = BlockPos::new(1, base_y + CHUNK_SIZE, 1);
        world.set_block(face_pos, stone, stagcrest_protocol::BlockState(0));
        world.mark_chunk_terrain_ready(above);

        let snapshot = DecorateSnapshot::capture(&world, pos, air);
        assert_eq!(snapshot.block_at(face_pos), stone);
        assert_eq!(snapshot.face_above(1, 1), stone);
    }
}
