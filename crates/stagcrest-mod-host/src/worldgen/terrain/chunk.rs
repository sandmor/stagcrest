use crate::worldgen::biome::BiomeRegistry;
use crate::worldgen::climate::ClimateSampler;
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
    density: DensitySampler<'a>,
    climate: ClimateSampler<'a>,
    biomes: &'a BiomeRegistry,
    blocks: ColumnBlocks,
}

impl<'a> ChunkFiller<'a> {
    pub fn new(
        config: &'a TerrainConfig,
        density: DensitySampler<'a>,
        climate: ClimateSampler<'a>,
        biomes: &'a BiomeRegistry,
        blocks: ColumnBlocks,
    ) -> Self {
        Self {
            config,
            density,
            climate,
            biomes,
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

                    let mut block = if y == self.config.world_min_y {
                        self.blocks.bedrock
                    } else if self.density.is_solid_at_y(wx, y, wz, surface_y) {
                        self.blocks.stone
                    } else if y < self.config.sea_level {
                        self.blocks.water
                    } else {
                        continue;
                    };

                    if block == self.blocks.stone && self.density.ore_block(wx, y, wz) {
                        block = self.blocks.iron_ore;
                    }

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

    /// Stage B: biome surface decoration (requires chunk above in `world` when at local y=15).
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
        let chunk_side = CHUNK_SIZE as usize;
        let mut surface_y_map = [[sea as f64; 16]; 16];

        for lz in 0..chunk_side {
            for lx in 0..chunk_side {
                let wx = base_x + lx as i32;
                let wz = base_z + lz as i32;
                surface_y_map[lz][lx] = self.density.surface_y(wx, wz);
            }
        }

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

                    let id = buffer[idx];
                    if id != self.blocks.stone && id != self.blocks.iron_ore {
                        continue;
                    }

                    if y == self.config.world_min_y {
                        buffer[idx] = self.blocks.bedrock;
                        states[idx] = BlockState(0);
                        continue;
                    }

                    let surface_y = surface_y_map[lz as usize][lx as usize];
                    let (temp, downfall) = self.climate.at(wx, wz);
                    let biome = self.biomes.biome_at(temp, downfall);
                    let on_island = self.density.sky_islands().is_solid(wx, y, wz);

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

                    let below_id = if ly > 0 {
                        let below_idx = LocalBlockPos {
                            x: lx as u8,
                            y: (ly - 1) as u8,
                            z: lz as u8,
                        }
                        .index();
                        buffer[below_idx]
                    } else {
                        world.get_block(BlockPos::new(wx, y - 1, wz)).0
                    };

                    let above_open =
                        above_id == self.blocks.air || above_id == self.blocks.water;
                    let shoreline = y == sea && below_id == self.blocks.water;

                    if above_open || shoreline {
                        let top = if on_island && y as f64 >= surface_y - 2.0 {
                            self.blocks.grass
                        } else if y >= sea {
                            if is_beach(surface_y, sea, downfall) {
                                self.blocks.sand
                            } else {
                                biome.surface_top
                            }
                        } else {
                            biome.underwater_top.unwrap_or(self.blocks.sand)
                        };
                        buffer[idx] = top;
                        states[idx] = BlockState(0);
                    } else if y >= sea {
                        let under = if on_island {
                            self.blocks.dirt
                        } else if above_id == biome.surface_top
                            || above_id == self.blocks.grass
                            || above_id == self.blocks.sand
                            || above_id == self.blocks.dirt
                        {
                            biome.surface_under
                        } else {
                            continue;
                        };

                        let depth = count_surface_depth(
                            world,
                            &buffer,
                            base_y,
                            wx,
                            wz,
                            lx as usize,
                            ly as usize,
                            lz as usize,
                            &self.blocks,
                        );
                        if depth < biome.surface_depth as i32 {
                            buffer[idx] = under;
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

fn is_beach(surface_y: f64, sea: i32, downfall: f32) -> bool {
    let near_sea = (surface_y - sea as f64).abs() <= 3.0;
    near_sea && downfall > 0.3
}

fn count_surface_depth(
    world: &World,
    buffer: &[BlockId],
    base_y: i32,
    wx: i32,
    wz: i32,
    lx: usize,
    ly: usize,
    lz: usize,
    blocks: &ColumnBlocks,
) -> i32 {
    let mut depth = 0i32;
    for dy in 1..=4 {
        let above = if ly + dy < CHUNK_SIZE as usize {
            let idx = LocalBlockPos {
                x: lx as u8,
                y: (ly + dy) as u8,
                z: lz as u8,
            }
            .index();
            buffer[idx]
        } else {
            world
                .get_block(BlockPos::new(wx, base_y + ly as i32 + dy as i32, wz))
                .0
        };
        if above == blocks.grass
            || above == blocks.sand
            || above == blocks.dirt
            || above == blocks.short_grass
            || above == blocks.tall_grass
        {
            depth += 1;
        } else {
            break;
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::BlockRegistry;
    use crate::worldgen::noise::NoiseBank;
    use crate::worldgen::seed::WorldSeed;
    use crate::worldgen::test_fixtures::{test_biomes, test_blocks};
    #[test]
    fn deep_seafloor_under_water_becomes_sand() {
        let config = TerrainConfig::default();
        let noise = NoiseBank::new(WorldSeed(99));
        let density = DensitySampler::new(&config, &noise, WorldSeed(99));
        let climate = ClimateSampler::new(&config, &noise);
        let mut reg = BlockRegistry::new();
        reg.register_texture("t".into(), 16, 16, vec![0; 16 * 16 * 4]);
        let tex = stagcrest_protocol::TextureId(0);
        let face = stagcrest_protocol::BlockFaceTextures::uniform(tex);
        for (name, id) in [
            ("stagcrest:air", 0u32),
            ("stagcrest:stone", 2),
            ("stagcrest:sand", 6),
            ("stagcrest:water", 5),
            ("stagcrest:grass_block", 4),
            ("stagcrest:dirt", 3),
        ] {
            reg.register_block(stagcrest_protocol::BlockDef {
                id: BlockId(id),
                namespaced_id: name.into(),
                display_name: name.into(),
                opaque: id != 0 && id != 5,
                transparent: id == 5,
                solid: id != 0 && id != 5,
                hardness: 1.0,
                face_textures: face,
                circuit: None,
                placeable: false,
                geometry: stagcrest_protocol::BlockGeometry::Cube,
                fluid: id == 5,
                render_layer: if id == 5 {
                    stagcrest_protocol::ModelRenderLayer::Blend
                } else {
                    stagcrest_protocol::ModelRenderLayer::Opaque
                },
            });
        }
        let biomes = test_biomes(&mut reg);
        let blocks = ColumnBlocks::resolve(&reg, BlockId(0));
        let filler = ChunkFiller::new(&config, density, climate, &biomes, blocks);
        let air = BlockId(0);
        let world = stagcrest_world::World::new(air);
        let water_state = still_water_state();

        let seafloor_y = 20i32;
        let chunk_pos = ChunkPos {
            x: 0,
            y: seafloor_y / CHUNK_SIZE,
            z: 0,
        };
        let chunk_base_y = chunk_pos.y * CHUNK_SIZE;
        let chunk_top_y = chunk_base_y + CHUNK_SIZE;
        let mut density_entries =
            vec![(BlockPos::new(0, seafloor_y, 0), blocks.stone, BlockState(0))];
        for y in (seafloor_y + 1)..config.sea_level.min(chunk_top_y) {
            density_entries.push((BlockPos::new(0, y, 0), blocks.water, water_state));
        }

        let decorated = filler.decorate(&world, chunk_pos, &density_entries);
        let floor = decorated
            .iter()
            .find(|(pos, _, _)| pos.y == seafloor_y)
            .map(|(_, id, _)| *id);
        assert_eq!(
            floor,
            Some(blocks.sand),
            "deep seafloor under water should be sand"
        );
    }

    #[test]
    fn block_at_sea_level_with_air_above_gets_surface() {
        let config = TerrainConfig::default();
        let sea = config.sea_level;
        let noise = NoiseBank::new(WorldSeed(42));
        let density = DensitySampler::new(&config, &noise, WorldSeed(42));
        let climate = ClimateSampler::new(&config, &noise);
        let mut reg = BlockRegistry::new();
        reg.register_texture("t".into(), 16, 16, vec![0; 16 * 16 * 4]);
        let tex = stagcrest_protocol::TextureId(0);
        let face = stagcrest_protocol::BlockFaceTextures::uniform(tex);
        for (name, id) in [
            ("stagcrest:air", 0u32),
            ("stagcrest:stone", 2),
            ("stagcrest:water", 5),
            ("stagcrest:grass_block", 4),
            ("stagcrest:dirt", 3),
            ("stagcrest:sand", 6),
        ] {
            reg.register_block(stagcrest_protocol::BlockDef {
                id: BlockId(id),
                namespaced_id: name.into(),
                display_name: name.into(),
                opaque: id != 0 && id != 5,
                transparent: id == 5,
                solid: id != 0 && id != 5,
                hardness: 1.0,
                face_textures: face,
                circuit: None,
                placeable: false,
                geometry: stagcrest_protocol::BlockGeometry::Cube,
                fluid: id == 5,
                render_layer: if id == 5 {
                    stagcrest_protocol::ModelRenderLayer::Blend
                } else {
                    stagcrest_protocol::ModelRenderLayer::Opaque
                },
            });
        }
        let biomes = test_biomes(&mut reg);
        let blocks = ColumnBlocks::resolve(&reg, BlockId(0));
        let filler = ChunkFiller::new(&config, density, climate, &biomes, blocks);
        let mut world = stagcrest_world::World::new(BlockId(0));
        let water_state = still_water_state();
        world.set_block(
            BlockPos::new(0, sea - 1, 0),
            blocks.water,
            water_state,
        );

        let chunk_pos = ChunkPos {
            x: 0,
            y: sea / CHUNK_SIZE,
            z: 0,
        };
        let density_entries = vec![
            (BlockPos::new(0, sea, 0), blocks.stone, BlockState(0)),
            (BlockPos::new(0, sea + 1, 0), blocks.stone, BlockState(0)),
        ];

        let decorated = filler.decorate(&world, chunk_pos, &density_entries);
        let shore = decorated
            .iter()
            .find(|(pos, _, _)| pos.y == sea)
            .map(|(_, id, _)| *id);
        assert_eq!(
            shore,
            Some(blocks.grass),
            "topmost solid at sea level should be biome surface, not stone"
        );
    }

    #[test]
    fn chunk_density_is_deterministic() {
        let config = TerrainConfig::default();
        let noise = NoiseBank::new(WorldSeed(5));
        let density = DensitySampler::new(&config, &noise, WorldSeed(5));
        let climate = ClimateSampler::new(&config, &noise);
        let mut reg = BlockRegistry::new();
        reg.register_texture("t".into(), 16, 16, vec![0; 16 * 16 * 4]);
        let tex = stagcrest_protocol::TextureId(0);
        let face = stagcrest_protocol::BlockFaceTextures::uniform(tex);
        for (name, id) in [
            ("stagcrest:grass_block", 4u32),
            ("stagcrest:dirt", 3u32),
            ("stagcrest:sand", 6u32),
        ] {
            reg.register_block(stagcrest_protocol::BlockDef {
                id: BlockId(id),
                namespaced_id: name.into(),
                display_name: name.into(),
                opaque: true,
                transparent: false,
                solid: true,
                hardness: 1.0,
                face_textures: face,
                circuit: None,
                placeable: false,
                geometry: stagcrest_protocol::BlockGeometry::Cube,
                fluid: false,
                render_layer: stagcrest_protocol::ModelRenderLayer::Opaque,
            });
        }
        let biomes = test_biomes(&mut reg);
        let blocks = test_blocks();
        let filler = ChunkFiller::new(&config, density, climate, &biomes, blocks);
        let pos = ChunkPos { x: 0, y: 4, z: 0 };
        let a = filler.fill_density(pos);
        let b = filler.fill_density(pos);
        assert_eq!(a, b);
    }

    #[test]
    fn count_surface_depth_reads_world_above_chunk() {
        let blocks = test_blocks();
        let air = BlockId(0);
        let mut buffer = vec![air; CHUNK_VOLUME];
        let lx = 0usize;
        let ly = 14usize;
        let lz = 0usize;
        buffer[LocalBlockPos {
            x: lx as u8,
            y: ly as u8,
            z: lz as u8,
        }
        .index()] = blocks.stone;
        buffer[LocalBlockPos {
            x: lx as u8,
            y: (ly + 1) as u8,
            z: lz as u8,
        }
        .index()] = blocks.dirt;

        let base_y = 64;
        let mut world = stagcrest_world::World::new(air);
        world.set_block(
            BlockPos::new(0, base_y + CHUNK_SIZE, 0),
            blocks.grass,
            BlockState(0),
        );
        world.set_block(
            BlockPos::new(0, base_y + CHUNK_SIZE + 1, 0),
            blocks.dirt,
            BlockState(0),
        );

        let depth = count_surface_depth(
            &world,
            &buffer,
            base_y,
            0,
            0,
            lx,
            ly,
            lz,
            &blocks,
        );
        assert!(
            depth >= 2,
            "surface depth should include grass/dirt from the chunk above"
        );
    }
}
