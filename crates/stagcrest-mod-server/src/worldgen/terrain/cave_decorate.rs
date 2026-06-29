use crate::registry::BlockRegistry;
use crate::worldgen::biome::{BiomeDimension, BiomeRegistry};
use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use crate::worldgen::terrain::column::ColumnBlocks;
use crate::worldgen::terrain::density::DensitySampler;
use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos};

/// Cave biome decoration pass.
pub struct CaveDecorator<'a> {
    blocks: ColumnBlocks,
    biomes: &'a BiomeRegistry,
    registry: &'a BlockRegistry,
}

impl<'a> CaveDecorator<'a> {
    pub fn new(
        blocks: ColumnBlocks,
        biomes: &'a BiomeRegistry,
        registry: &'a BlockRegistry,
    ) -> Self {
        Self {
            blocks,
            biomes,
            registry,
        }
    }

    pub fn decorate_caves(
        &self,
        snapshot: &DecorateSnapshot,
        _pos: ChunkPos,
        density_entries: &[(BlockPos, BlockId, BlockState)],
        climate_sampler: &crate::worldgen::climate::ClimateSampler<'_>,
        density: &DensitySampler<'_>,
    ) -> Vec<(BlockPos, BlockId, BlockState)> {
        let mut decor = Vec::new();

        for &(block_pos, id, _) in density_entries {
            if id != self.blocks.stone && id != self.blocks.air {
                continue;
            }
            let wx = block_pos.x;
            let y = block_pos.y;
            let wz = block_pos.z;
            let surface_y = density.surface_y(wx, wz);
            if y as f64 >= surface_y - 4.0 {
                continue;
            }

            let params = climate_sampler.at_3d(wx, y, wz, surface_y);
            let biome = self.biomes.biome_at(params, BiomeDimension::Underground);
            let above = snapshot.block_at(BlockPos::new(wx, y + 1, wz));
            let below = snapshot.block_at(BlockPos::new(wx, y - 1, wz));
            let is_cave_air = above == self.blocks.air || above == self.blocks.water;
            if !is_cave_air {
                continue;
            }

            if density.touches_river_channel(wx, y, wz) {
                continue;
            }

            match biome.namespaced_id.as_str() {
                "stagcrest:lush_caves" => {
                    if below == self.blocks.stone || below == id {
                        if let Some(moss) = self.resolve("stagcrest:moss_block") {
                            decor.push((block_pos, moss, BlockState(0)));
                        }
                    }
                    if y % 7 == 0 {
                        let glow_pos = BlockPos::new(wx, y + 1, wz);
                        if density.touches_river_channel(wx, y + 1, wz) {
                            continue;
                        }
                        if let Some(glow) = self.resolve("stagcrest:glow_lichen") {
                            decor.push((glow_pos, glow, BlockState(0)));
                        }
                    }
                }
                "stagcrest:dripstone_caves" => {
                    let above_solid = above != self.blocks.air && above != self.blocks.water;
                    if y % 5 == 0 && above_solid {
                        let stal_pos = BlockPos::new(wx, y + 1, wz);
                        if density.touches_river_channel(wx, y + 1, wz) {
                            continue;
                        }
                        if let Some(stal) = self.resolve("stagcrest:pointed_dripstone") {
                            decor.push((stal_pos, stal, BlockState(0)));
                        }
                    }
                    if below == self.blocks.stone {
                        if let Some(stal) = self.resolve("stagcrest:pointed_dripstone") {
                            decor.push((block_pos, stal, BlockState(0)));
                        }
                    }
                }
                "stagcrest:deep_dark" => {
                    if let Some(deepslate) = self.resolve("stagcrest:deepslate") {
                        decor.push((block_pos, deepslate, BlockState(0)));
                    }
                    if y % 11 == 0 {
                        if let Some(sculk) = self.resolve("stagcrest:sculk") {
                            decor.push((block_pos, sculk, BlockState(0)));
                        }
                    }
                }
                _ => {}
            }
        }

        decor
    }

    fn resolve(&self, name: &str) -> Option<BlockId> {
        self.registry.block_by_name(name)
    }
}
