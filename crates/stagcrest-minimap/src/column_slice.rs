use stagcrest_protocol::BlockId;

use crate::color::MinimapBiomeClimate;
use crate::column::ColumnBlockSource;

/// Compact vertical column captured from a live block source for async resolve.
#[derive(Clone)]
pub struct ColumnSlice {
    pub wx: i32,
    pub wz: i32,
    pub y_min: i32,
    pub y_max: i32,
    pub blocks: Vec<BlockId>,
    pub biome: MinimapBiomeClimate,
    pub loaded: bool,
}

impl ColumnSlice {
    pub fn capture(
        source: &impl ColumnBlockSource,
        wx: i32,
        wz: i32,
        y_min: i32,
        y_max: i32,
        biome: MinimapBiomeClimate,
        loaded: bool,
    ) -> Self {
        let len = (y_max - y_min + 1) as usize;
        let mut blocks = Vec::with_capacity(len);
        for y in y_min..=y_max {
            let (id, _) = source.block_at(wx, y, wz);
            blocks.push(id);
        }
        Self {
            wx,
            wz,
            y_min,
            y_max,
            blocks,
            biome,
            loaded,
        }
    }
}

impl ColumnBlockSource for ColumnSlice {
    fn block_at(&self, wx: i32, wy: i32, wz: i32) -> (BlockId, stagcrest_protocol::BlockState) {
        if wx != self.wx || wz != self.wz {
            return (BlockId(0), stagcrest_protocol::BlockState(0));
        }
        if wy < self.y_min || wy > self.y_max {
            return (BlockId(0), stagcrest_protocol::BlockState(0));
        }
        let idx = (wy - self.y_min) as usize;
        (self.blocks[idx], stagcrest_protocol::BlockState(0))
    }
}
