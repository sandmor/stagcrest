use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos};
use std::collections::{HashMap, HashSet};

use crate::chunk::Chunk;

#[derive(Debug, Default)]
pub struct World {
    chunks: HashMap<ChunkPos, Chunk>,
    pub dirty_chunks: HashSet<ChunkPos>,
    air: BlockId,
}

impl World {
    pub fn new(air: BlockId) -> Self {
        Self {
            chunks: HashMap::new(),
            dirty_chunks: HashSet::new(),
            air,
        }
    }

    pub fn air(&self) -> BlockId {
        self.air
    }

    pub fn ensure_chunk(&mut self, pos: ChunkPos) -> &mut Chunk {
        self.chunks.entry(pos).or_default()
    }

    pub fn get_block(&self, pos: BlockPos) -> (BlockId, BlockState) {
        let chunk_pos = pos.chunk_pos();
        let local = pos.local();
        match self.chunks.get(&chunk_pos) {
            Some(chunk) => {
                let b = chunk.get(local);
                (b.id, b.state)
            }
            None => (self.air, BlockState(0)),
        }
    }

    pub fn set_block(&mut self, pos: BlockPos, id: BlockId, state: BlockState) {
        let chunk_pos = pos.chunk_pos();
        let local = pos.local();
        self.chunks
            .entry(chunk_pos)
            .or_default()
            .set(local, id, state);
        self.mark_dirty_and_neighbors(pos);
    }

    pub fn set_blocks(
        &mut self,
        blocks: impl IntoIterator<Item = (BlockPos, BlockId, BlockState)>,
    ) {
        let mut touched_chunks = HashSet::new();
        for (pos, id, state) in blocks {
            let chunk_pos = pos.chunk_pos();
            self.chunks
                .entry(chunk_pos)
                .or_default()
                .set(pos.local(), id, state);
            touched_chunks.insert(chunk_pos);
        }
        for chunk_pos in touched_chunks {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        self.dirty_chunks.insert(ChunkPos {
                            x: chunk_pos.x + dx,
                            y: chunk_pos.y + dy,
                            z: chunk_pos.z + dz,
                        });
                    }
                }
            }
        }
    }

    pub fn mark_dirty_and_neighbors(&mut self, pos: BlockPos) {
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let p = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
                    self.dirty_chunks.insert(p.chunk_pos());
                }
            }
        }
    }

    pub fn take_dirty_chunks(&mut self) -> HashSet<ChunkPos> {
        std::mem::take(&mut self.dirty_chunks)
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.chunks.get(&pos)
    }

    pub fn chunk_mut(&mut self, pos: ChunkPos) -> Option<&mut Chunk> {
        self.chunks.get_mut(&pos)
    }

    pub fn chunks(&self) -> impl Iterator<Item = (&ChunkPos, &Chunk)> {
        self.chunks.iter()
    }

    pub fn loaded_chunk_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.chunks.keys().copied()
    }

    pub fn unload_far_chunks_3d(
        &mut self,
        center: ChunkPos,
        horizontal_radius: i32,
        vertical_radius: i32,
    ) -> Vec<ChunkPos> {
        let mut removed = Vec::new();
        self.chunks.retain(|pos, _| {
            let keep = (pos.x - center.x).abs() <= horizontal_radius
                && (pos.z - center.z).abs() <= horizontal_radius
                && (pos.y - center.y).abs() <= vertical_radius;
            if !keep {
                removed.push(*pos);
            }
            keep
        });
        removed
    }

    pub fn load_area_3d(
        &mut self,
        center: ChunkPos,
        horizontal_radius: i32,
        vertical_radius: i32,
        y_bounds: std::ops::RangeInclusive<i32>,
    ) {
        let y_min = (center.y - vertical_radius).max(*y_bounds.start());
        let y_max = (center.y + vertical_radius).min(*y_bounds.end());
        for x in (center.x - horizontal_radius)..=(center.x + horizontal_radius) {
            for z in (center.z - horizontal_radius)..=(center.z + horizontal_radius) {
                for y in y_min..=y_max {
                    self.ensure_chunk(ChunkPos { x, y, z });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{BlockPos, CHUNK_SIZE};

    #[test]
    fn set_blocks_marks_neighbor_chunks() {
        let air = BlockId(0);
        let stone = BlockId(1);
        let mut world = World::new(air);
        let chunk = ChunkPos { x: 0, y: 0, z: 0 };
        world.ensure_chunk(chunk);

        world.set_blocks([(
            BlockPos::new(0, 0, 0),
            stone,
            BlockState(0),
        )]);
        assert!(world.dirty_chunks.contains(&chunk));
        assert!(world.dirty_chunks.len() <= 27);
    }

    #[test]
    fn set_blocks_writes_many_blocks_in_one_chunk() {
        let air = BlockId(0);
        let stone = BlockId(1);
        let mut world = World::new(air);
        world.ensure_chunk(ChunkPos { x: 0, y: 0, z: 0 });

        let blocks: Vec<_> = (0..CHUNK_SIZE)
            .flat_map(|x| {
                (0..CHUNK_SIZE).flat_map(move |z| {
                    (0..CHUNK_SIZE).map(move |y| (BlockPos::new(x, y, z), stone, BlockState(0)))
                })
            })
            .collect();
        world.set_blocks(blocks);

        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    assert_eq!(world.get_block(BlockPos::new(x, y, z)).0, stone);
                }
            }
        }
    }
}
