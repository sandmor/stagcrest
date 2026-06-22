use crate::registry::BlockRegistry;
use stagcrest_protocol::{BlockId, BlockPos, BlockState};

#[derive(Clone, Copy)]
pub struct ColumnBlocks {
    pub bedrock: BlockId,
    pub stone: BlockId,
    pub dirt: BlockId,
    pub grass: BlockId,
    pub water: BlockId,
    pub air: BlockId,
}

impl ColumnBlocks {
    pub fn resolve(registry: &BlockRegistry, air: BlockId) -> Self {
        Self {
            bedrock: registry
                .block_by_name("stagcrest:bedrock")
                .unwrap_or(BlockId(0)),
            stone: registry
                .block_by_name("stagcrest:stone")
                .unwrap_or(BlockId(0)),
            dirt: registry
                .block_by_name("stagcrest:dirt")
                .unwrap_or(BlockId(0)),
            grass: registry
                .block_by_name("stagcrest:grass_block")
                .unwrap_or(BlockId(0)),
            water: registry
                .block_by_name("stagcrest:water")
                .unwrap_or(BlockId(0)),
            air,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColumnData {
    pub wx: i32,
    pub wz: i32,
    pub entries: Vec<(i32, BlockId, BlockState)>,
}

impl ColumnData {
    pub fn block_positions(&self) -> impl Iterator<Item = (BlockPos, BlockId, BlockState)> + '_ {
        self.entries
            .iter()
            .map(|&(y, id, state)| (BlockPos::new(self.wx, y, self.wz), id, state))
    }
}
