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

    pub fn unload_far_chunks(&mut self, center: ChunkPos, radius: i32) {
        self.chunks.retain(|pos, _| {
            (pos.x - center.x).abs() <= radius
                && (pos.y - center.y).abs() <= radius
                && (pos.z - center.z).abs() <= radius
        });
    }

    pub fn load_area(&mut self, center: ChunkPos, radius: i32) {
        for x in (center.x - radius)..=(center.x + radius) {
            for z in (center.z - radius)..=(center.z + radius) {
                for y in 0..=1 {
                    self.ensure_chunk(ChunkPos { x, y, z });
                }
            }
        }
    }
}
