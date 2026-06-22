use crate::worldgen::config::TerrainConfig;
use crate::worldgen::terrain::column::ColumnBlocks;
use crate::worldgen::terrain::density::DensitySampler;
use stagcrest_protocol::{
    still_water_state, BlockId, BlockPos, BlockState, ChunkPos, LocalBlockPos, CHUNK_SIZE,
    CHUNK_VOLUME,
};
use stagcrest_world::World;

pub struct ChunkFiller<'a> {
    config: &'a TerrainConfig,
    density: &'a DensitySampler<'a>,
    blocks: ColumnBlocks,
}

impl<'a> ChunkFiller<'a> {
    pub fn new(
        config: &'a TerrainConfig,
        density: &'a DensitySampler<'a>,
        blocks: ColumnBlocks,
    ) -> Self {
        Self {
            config,
            density,
            blocks,
        }
    }

    /// Stage A: density fill for one 16³ chunk (no decoration, no world reads).
    pub fn fill_density(&self, pos: ChunkPos) -> Vec<(BlockPos, BlockId, BlockState)> {
        let base_x = pos.x * CHUNK_SIZE;
        let base_y = pos.y * CHUNK_SIZE;
        let base_z = pos.z * CHUNK_SIZE;
        let water_state = still_water_state();
        let mut entries = Vec::new();

        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let wx = base_x + lx;
                    let y = base_y + ly;
                    let wz = base_z + lz;
                    let surface_y = self.density.surface_y(wx, wz);

                    let block = if y == self.config.world_min_y {
                        self.blocks.bedrock
                    } else if self.density.is_solid_at_y(wx, y, wz, surface_y) {
                        self.blocks.stone
                    } else if y < self.config.sea_level {
                        self.blocks.water
                    } else {
                        continue;
                    };

                    let state = if block == self.blocks.water {
                        water_state
                    } else {
                        BlockState(0)
                    };
                    entries.push((BlockPos::new(wx, y, wz), block, state));
                }
            }
        }

        entries
    }

    /// Stage B: grass/dirt/bedrock decoration (requires chunk above in `world` when at local y=15).
    pub fn decorate(
        &self,
        world: &World,
        pos: ChunkPos,
        density_entries: &[(BlockPos, BlockId, BlockState)],
    ) -> Vec<(BlockPos, BlockId, BlockState)> {
        let base_x = pos.x * CHUNK_SIZE;
        let base_y = pos.y * CHUNK_SIZE;
        let base_z = pos.z * CHUNK_SIZE;
        let sea = self.config.sea_level;
        let water_state = still_water_state();

        let mut buffer = vec![self.blocks.air; CHUNK_VOLUME];
        let mut states = vec![BlockState(0); CHUNK_VOLUME];

        for &(block_pos, id, state) in density_entries {
            let local = block_pos.local();
            let idx = local.index();
            buffer[idx] = id;
            states[idx] = state;
        }

        for ly in (0..CHUNK_SIZE).rev() {
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let wx = base_x + lx;
                    let y = base_y + ly;
                    let wz = base_z + lz;
                    let idx = LocalBlockPos {
                        x: lx as u8,
                        y: ly as u8,
                        z: lz as u8,
                    }
                    .index();

                    if buffer[idx] != self.blocks.stone {
                        continue;
                    }
                    if y == self.config.world_min_y {
                        buffer[idx] = self.blocks.bedrock;
                        states[idx] = BlockState(0);
                        continue;
                    }

                    let above_id = if ly < CHUNK_SIZE - 1 {
                        let above_idx = LocalBlockPos {
                            x: lx as u8,
                            y: (ly + 1) as u8,
                            z: lz as u8,
                        }
                        .index();
                        buffer[above_idx]
                    } else {
                        world.get_block(BlockPos::new(wx, y + 1, wz)).0
                    };

                    let above_open = above_id == self.blocks.air || above_id == self.blocks.water;

                    if above_open {
                        buffer[idx] = if y > sea {
                            self.blocks.grass
                        } else {
                            self.blocks.stone
                        };
                        states[idx] = BlockState(0);
                    } else if y > sea && above_id == self.blocks.grass {
                        buffer[idx] = self.blocks.dirt;
                        states[idx] = BlockState(0);
                    } else if y > sea && above_id == self.blocks.dirt {
                        let above2_id = if ly < CHUNK_SIZE - 2 {
                            let above2_idx = LocalBlockPos {
                                x: lx as u8,
                                y: (ly + 2) as u8,
                                z: lz as u8,
                            }
                            .index();
                            buffer[above2_idx]
                        } else if ly == CHUNK_SIZE - 2 {
                            world.get_block(BlockPos::new(wx, y + 2, wz)).0
                        } else {
                            self.blocks.air
                        };
                        if above2_id == self.blocks.grass || above2_id == self.blocks.dirt {
                            buffer[idx] = self.blocks.dirt;
                            states[idx] = BlockState(0);
                        }
                    }
                }
            }
        }

        let mut entries = Vec::new();
        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let idx = LocalBlockPos {
                        x: lx as u8,
                        y: ly as u8,
                        z: lz as u8,
                    }
                    .index();
                    let id = buffer[idx];
                    if id == self.blocks.air {
                        continue;
                    }
                    let wx = base_x + lx;
                    let y = base_y + ly;
                    let wz = base_z + lz;
                    let state = if id == self.blocks.water {
                        water_state
                    } else {
                        states[idx]
                    };
                    entries.push((BlockPos::new(wx, y, wz), id, state));
                }
            }
        }

        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::noise::NoiseBank;
    use crate::worldgen::seed::WorldSeed;

    #[test]
    fn chunk_density_is_deterministic() {
        let config = TerrainConfig::default();
        let noise = NoiseBank::new(WorldSeed(5));
        let density = DensitySampler::new(&config, &noise, WorldSeed(5));
        let blocks = ColumnBlocks {
            bedrock: BlockId(1),
            stone: BlockId(2),
            dirt: BlockId(3),
            grass: BlockId(4),
            water: BlockId(5),
            air: BlockId(0),
        };
        let filler = ChunkFiller::new(&config, &density, blocks);
        let pos = ChunkPos { x: 0, y: 4, z: 0 };
        let a = filler.fill_density(pos);
        let b = filler.fill_density(pos);
        assert_eq!(a, b);
    }
}
