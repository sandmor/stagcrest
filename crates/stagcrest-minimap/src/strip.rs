use std::collections::HashMap;

use stagcrest_protocol::{manifest::BIOME_GRID_VOLUME, BlockState, LocalBlockPos, CHUNK_SIZE};
use stagcrest_storage::{InactiveChunk, InactiveChunkReader};

use crate::column::ColumnBlockSource;

/// Vertical stack of saved chunks for one horizontal column `(cx, cz)`.
pub struct StorageStripSource {
    chunks: HashMap<i32, InactiveChunk>,
    cx: i32,
    cz: i32,
}

impl StorageStripSource {
    pub fn new(cx: i32, cz: i32) -> Self {
        Self {
            chunks: HashMap::new(),
            cx,
            cz,
        }
    }

    pub fn insert(&mut self, cy: i32, chunk: InactiveChunk) {
        self.chunks.insert(cy, chunk);
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn biome_grid_at(&self, _wx: i32, wy: i32, _wz: i32) -> Option<&[u8; BIOME_GRID_VOLUME]> {
        let cy = floor_div(wy, CHUNK_SIZE);
        let chunk = self.chunks.get(&cy)?;
        Some(&chunk.biome_grid)
    }

    fn reader_at(&self, cy: i32) -> Option<InactiveChunkReader<'_>> {
        self.chunks.get(&cy).map(InactiveChunkReader::new)
    }
}

impl ColumnBlockSource for StorageStripSource {
    fn block_at(&self, wx: i32, wy: i32, wz: i32) -> (stagcrest_protocol::BlockId, BlockState) {
        let cy = floor_div(wy, CHUNK_SIZE);
        let Some(reader) = self.reader_at(cy) else {
            return (stagcrest_protocol::BlockId(0), BlockState(0));
        };
        let lx = wx - self.cx * CHUNK_SIZE;
        let ly = wy - cy * CHUNK_SIZE;
        let lz = wz - self.cz * CHUNK_SIZE;
        if !(0..CHUNK_SIZE).contains(&lx)
            || !(0..CHUNK_SIZE).contains(&ly)
            || !(0..CHUNK_SIZE).contains(&lz)
        {
            return (stagcrest_protocol::BlockId(0), BlockState(0));
        }
        reader.block_at(LocalBlockPos {
            x: lx as u8,
            y: ly as u8,
            z: lz as u8,
        })
    }
}

fn floor_div(a: i32, b: i32) -> i32 {
    stagcrest_protocol::floor_div(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{BlockId, BlockState, CHUNK_VOLUME};

    #[test]
    fn reads_across_vertical_chunk_boundary() {
        let mut strip = StorageStripSource::new(0, 0);
        let air = BlockId(0);
        let stone = BlockId(1);
        let indices_low = vec![1u16; CHUNK_VOLUME];
        let chunk_y0 = InactiveChunk::from_indices(
            vec![air, stone],
            vec![BlockState(0), BlockState(0)],
            &indices_low,
        )
        .unwrap();
        let indices_high = vec![0u16; CHUNK_VOLUME];
        let chunk_y1 =
            InactiveChunk::from_indices(vec![air], vec![BlockState(0)], &indices_high).unwrap();
        strip.insert(0, chunk_y0);
        strip.insert(1, chunk_y1);
        let (id, _) = strip.block_at(0, 15, 0);
        assert_eq!(id, stone);
        let (id, _) = strip.block_at(0, 16, 0);
        assert_eq!(id, air);
    }
}
