use stagcrest_protocol::{BlockId, BlockState, LocalBlockPos, CHUNK_SIZE, CHUNK_VOLUME};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkBlock {
    pub id: BlockId,
    pub state: BlockState,
}

impl Default for ChunkBlock {
    fn default() -> Self {
        Self {
            id: BlockId(0),
            state: BlockState(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    palette: Vec<BlockId>,
    states: Vec<BlockState>,
    indices: Vec<u16>,
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            palette: vec![BlockId(0)],
            states: vec![BlockState(0)],
            indices: vec![0; CHUNK_VOLUME],
        }
    }

    pub fn get(&self, local: LocalBlockPos) -> ChunkBlock {
        let palette_idx = self.indices[local.index()] as usize;
        ChunkBlock {
            id: self.palette[palette_idx],
            state: self.states[palette_idx],
        }
    }

    pub fn set(&mut self, local: LocalBlockPos, id: BlockId, state: BlockState) {
        let idx = local.index();
        let palette_idx = self.find_or_insert_palette(id, state);
        self.indices[idx] = palette_idx as u16;
    }

    fn find_or_insert_palette(&mut self, id: BlockId, state: BlockState) -> usize {
        for (i, (&pid, &pstate)) in self.palette.iter().zip(self.states.iter()).enumerate() {
            if pid == id && pstate == state {
                return i;
            }
        }
        self.palette.push(id);
        self.states.push(state);
        self.palette.len() - 1
    }

    pub fn fill(&mut self, id: BlockId, state: BlockState) {
        let palette_idx = self.find_or_insert_palette(id, state) as u16;
        self.indices.fill(palette_idx);
    }

    pub fn palette(&self) -> &[BlockId] {
        &self.palette
    }

    pub fn is_empty(&self, air: BlockId) -> bool {
        self.palette.len() == 1 && self.palette[0] == air
    }
}

/// Resolve block id at local position using neighbor chunk data when on boundary.
pub struct ChunkNeighborhood<'a> {
    pub center: &'a Chunk,
    pub neighbors: HashMap<(i32, i32, i32), &'a Chunk>,
}

impl ChunkNeighborhood<'_> {
    pub fn get(&self, x: i32, y: i32, z: i32) -> ChunkBlock {
        let (cx, lx) = div_rem(x, CHUNK_SIZE);
        let (cy, ly) = div_rem(y, CHUNK_SIZE);
        let (cz, lz) = div_rem(z, CHUNK_SIZE);

        let chunk = if cx == 0 && cy == 0 && cz == 0 {
            self.center
        } else {
            match self.neighbors.get(&(cx, cy, cz)) {
                Some(c) => c,
                None => return ChunkBlock::default(),
            }
        };

        chunk.get(LocalBlockPos {
            x: lx as u8,
            y: ly as u8,
            z: lz as u8,
        })
    }
}

fn div_rem(v: i32, d: i32) -> (i32, i32) {
    if v >= 0 {
        (v / d, v % d)
    } else {
        let q = (v - (d - 1)) / d;
        (q, v - q * d)
    }
}
