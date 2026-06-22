use crate::registry::BlockRegistry;
use stagcrest_protocol::{BlockId, BlockPos, BlockState};

#[derive(Clone, Copy)]
pub struct ColumnBlocks {
    pub bedrock: BlockId,
    pub stone: BlockId,
    pub dirt: BlockId,
    pub grass: BlockId,
    pub sand: BlockId,
    pub iron_ore: BlockId,
    pub oak_log: BlockId,
    pub oak_leaves: BlockId,
    pub short_grass: BlockId,
    pub tall_grass: BlockId,
    pub dandelion: BlockId,
    pub poppy: BlockId,
    pub cactus: BlockId,
    pub dead_bush: BlockId,
    pub water: BlockId,
    pub air: BlockId,
}

impl ColumnBlocks {
    pub fn resolve(registry: &BlockRegistry, air: BlockId) -> Self {
        fn block(registry: &BlockRegistry, name: &str) -> BlockId {
            registry.block_by_name(name).unwrap_or(BlockId(0))
        }
        Self {
            bedrock: block(registry, "stagcrest:bedrock"),
            stone: block(registry, "stagcrest:stone"),
            dirt: block(registry, "stagcrest:dirt"),
            grass: block(registry, "stagcrest:grass_block"),
            sand: block(registry, "stagcrest:sand"),
            iron_ore: block(registry, "stagcrest:iron_ore"),
            oak_log: block(registry, "stagcrest:oak_log"),
            oak_leaves: block(registry, "stagcrest:oak_leaves"),
            short_grass: block(registry, "stagcrest:short_grass"),
            tall_grass: block(registry, "stagcrest:tall_grass"),
            dandelion: block(registry, "stagcrest:dandelion"),
            poppy: block(registry, "stagcrest:poppy"),
            cactus: block(registry, "stagcrest:cactus"),
            dead_bush: block(registry, "stagcrest:dead_bush"),
            water: block(registry, "stagcrest:water"),
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
