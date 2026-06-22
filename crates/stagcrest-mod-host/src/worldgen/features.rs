use crate::worldgen::biome::{BiomeRegistry, FeatureKind};
use crate::worldgen::climate::ClimateSampler;
use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::occupancy::OccupancyMap;
use crate::worldgen::seed::WorldSeed;
use crate::worldgen::terrain::{ColumnBlocks, SkyIslandSampler};
use crate::worldgen::trees::place_oak_tree;
use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos, CHUNK_SIZE};
use stagcrest_world::World;

const SKY_ISLAND_TREE_CHANCE: f32 = 0.02;

pub struct FeaturePlacer<'a> {
    config: &'a TerrainConfig,
    sky_islands: SkyIslandSampler<'a>,
    climate: ClimateSampler<'a>,
    blocks: ColumnBlocks,
    biomes: &'a BiomeRegistry,
    seed: WorldSeed,
}

impl<'a> FeaturePlacer<'a> {
    pub fn new(
        config: &'a TerrainConfig,
        noise: &'a NoiseBank,
        blocks: ColumnBlocks,
        biomes: &'a BiomeRegistry,
        seed: WorldSeed,
    ) -> Self {
        Self {
            config,
            sky_islands: SkyIslandSampler::new(config, noise, seed),
            climate: ClimateSampler::new(config, noise),
            blocks,
            biomes,
            seed,
        }
    }

    pub fn place(
        &self,
        world: &World,
        pos: ChunkPos,
        surface_entries: &[(BlockPos, BlockId, BlockState)],
    ) -> Vec<(BlockPos, BlockId, BlockState)> {
        let base_x = pos.x * CHUNK_SIZE;
        let base_z = pos.z * CHUNK_SIZE;

        let chunk_area = CHUNK_SIZE as usize;
        let mut top_surface: [Option<(i32, BlockId)>; 256] = [None; 256];

        for &(block_pos, id, _) in surface_entries {
            let local = block_pos.local();
            let lx = local.x as usize;
            let lz = local.z as usize;
            let idx = lz * chunk_area + lx;
            let y = block_pos.y;
            match top_surface[idx] {
                None => top_surface[idx] = Some((y, id)),
                Some((prev_y, _)) if y > prev_y => top_surface[idx] = Some((y, id)),
                _ => {}
            }
        }

        let mut occupancy = OccupancyMap::from_surface_entries(surface_entries, pos, self.blocks.air);
        let mut features = Vec::new();

        for lz in 0..CHUNK_SIZE as usize {
            for lx in 0..CHUNK_SIZE as usize {
                let idx = lz * chunk_area + lx;
                let Some((surface_y, surface_id)) = top_surface[idx] else {
                    continue;
                };

                let wx = base_x + lx as i32;
                let wz = base_z + lz as i32;
                let (temp, downfall) = self.climate.at(wx, wz);
                let biome = self.biomes.biome_at(temp, downfall);
                let on_island = self.sky_islands.is_solid(wx, surface_y, wz);

                let above_y = surface_y + 1;
                let above_pos = BlockPos::new(wx, above_y, wz);
                if !occupancy.can_place(world, above_pos) {
                    continue;
                }

                let is_grass_surface = surface_id == self.blocks.grass
                    || surface_id == biome.surface_top
                    || (on_island && surface_id == self.blocks.grass);
                let is_sand_surface = surface_id == self.blocks.sand;

                for feature in &biome.features {
                    self.place_feature(
                        feature.kind,
                        feature.chance,
                        wx,
                        wz,
                        above_y,
                        is_grass_surface,
                        is_sand_surface,
                        on_island,
                        world,
                        &mut occupancy,
                        &mut features,
                    );
                }

                if on_island
                    && hash_chance(self.seed, wx, wz, FeatureKind::OakTree) < SKY_ISLAND_TREE_CHANCE
                {
                    let height = oak_tree_height(self.seed, wx, wz);
                    place_oak_tree(
                        wx,
                        above_y,
                        wz,
                        height,
                        &self.blocks,
                        world,
                        &mut occupancy,
                        &mut features,
                        self.config,
                    );
                }
            }
        }

        features
    }

    fn place_feature(
        &self,
        kind: FeatureKind,
        chance: f32,
        wx: i32,
        wz: i32,
        above_y: i32,
        is_grass_surface: bool,
        is_sand_surface: bool,
        on_island: bool,
        world: &World,
        occupancy: &mut OccupancyMap,
        features: &mut Vec<(BlockPos, BlockId, BlockState)>,
    ) {
        let roll = hash_chance(self.seed, wx, wz, kind);
        if roll >= chance {
            return;
        }

        match kind {
            FeatureKind::ShortGrass | FeatureKind::TallGrass | FeatureKind::Dandelion
            | FeatureKind::Poppy if is_grass_surface =>
            {
                let block = match kind {
                    FeatureKind::ShortGrass => self.blocks.short_grass,
                    FeatureKind::TallGrass => self.blocks.tall_grass,
                    FeatureKind::Dandelion => self.blocks.dandelion,
                    FeatureKind::Poppy => self.blocks.poppy,
                    _ => return,
                };
                if block == self.blocks.air {
                    return;
                }
                let pos = BlockPos::new(wx, above_y, wz);
                if !occupancy.can_place(world, pos) {
                    return;
                }
                features.push((pos, block, BlockState(0)));
                occupancy.place(pos, block);
            }
            FeatureKind::Cactus if is_sand_surface => {
                let height = 1 + (hash_chance(self.seed, wx, wz, FeatureKind::Cactus) * 3.0) as i32;
                for dy in 0..height {
                    let pos = BlockPos::new(wx, above_y + dy, wz);
                    if !occupancy.can_place(world, pos) {
                        break;
                    }
                    features.push((pos, self.blocks.cactus, BlockState(0)));
                    occupancy.place(pos, self.blocks.cactus);
                }
            }
            FeatureKind::DeadBush if is_sand_surface => {
                if self.blocks.dead_bush == self.blocks.air {
                    return;
                }
                let pos = BlockPos::new(wx, above_y, wz);
                if !occupancy.can_place(world, pos) {
                    return;
                }
                features.push((pos, self.blocks.dead_bush, BlockState(0)));
                occupancy.place(pos, self.blocks.dead_bush);
            }
            FeatureKind::OakTree if is_grass_surface || on_island => {
                let height = oak_tree_height(self.seed, wx, wz);
                place_oak_tree(
                    wx,
                    above_y,
                    wz,
                    height,
                    &self.blocks,
                    world,
                    occupancy,
                    features,
                    self.config,
                );
            }
            _ => {}
        }
    }
}

fn oak_tree_height(seed: WorldSeed, wx: i32, wz: i32) -> i32 {
    4 + (hash_chance(seed, wx, wz, FeatureKind::OakTree) * 3.0) as i32
}

fn hash_chance(seed: WorldSeed, wx: i32, wz: i32, kind: FeatureKind) -> f32 {
    let tag = kind as u64;
    let h = seed
        .0
        .wrapping_add(wx as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(wz as u64)
        .wrapping_add(tag.wrapping_mul(0x517C_C1B7_2722_0A95));
    let mixed = (h ^ (h >> 33)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (mixed as f32 / u64::MAX as f32).clamp(0.0, 0.9999)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::biome::{BiomeRegistry, FeatureKind, RegisterBiomeFeatureRequest, RegisterBiomeRequest};
    use crate::worldgen::test_fixtures::{test_biomes, test_blocks, test_registry};

    #[test]
    fn feature_skips_occupied_decorate_cell() {
        let config = TerrainConfig::default();
        let noise = NoiseBank::new(WorldSeed(7));
        let blocks = test_blocks();
        let mut reg = test_registry();
        let biomes = test_biomes(&mut reg);
        let placer = FeaturePlacer::new(&config, &noise, blocks, &biomes, WorldSeed(7));
        let world = stagcrest_world::World::new(blocks.air);

        let surface_y = 64;
        let surface_entries = vec![
            (
                BlockPos::new(0, surface_y, 0),
                blocks.grass,
                BlockState(0),
            ),
            (
                BlockPos::new(0, surface_y + 1, 0),
                blocks.stone,
                BlockState(0),
            ),
        ];

        let features = placer.place(
            &world,
            ChunkPos {
                x: 0,
                y: surface_y / CHUNK_SIZE,
                z: 0,
            },
            &surface_entries,
        );

        assert!(
            features.is_empty(),
            "should not place features when space above surface is occupied in decorate output"
        );
    }

    #[test]
    fn oak_tree_places_logs_and_leaves() {
        let config = TerrainConfig::default();
        let noise = NoiseBank::new(WorldSeed(99));
        let blocks = test_blocks();
        let reg = test_registry();
        let mut biomes = BiomeRegistry::default();
        biomes.register_biome(RegisterBiomeRequest {
            namespaced_id: "stagcrest:plains".into(),
            temperature: 0.8,
            downfall: 0.4,
            surface_top: "stagcrest:grass_block".into(),
            surface_under: "stagcrest:dirt".into(),
            surface_depth: 3,
            underwater_top: Some("stagcrest:sand".into()),
        });
        biomes.register_feature(RegisterBiomeFeatureRequest {
            biome_id: "stagcrest:plains".into(),
            feature_kind: FeatureKind::OakTree,
            chance: 1.0,
        });
        biomes.finalize(&reg).unwrap();
        let world = stagcrest_world::World::new(blocks.air);

        let surface_y = 70;
        let surface_entries = vec![(
            BlockPos::new(5, surface_y, 5),
            blocks.grass,
            BlockState(0),
        )];

        let placer = FeaturePlacer::new(&config, &noise, blocks, &biomes, WorldSeed(1));
        let features = placer.place(
            &world,
            ChunkPos {
                x: 0,
                y: surface_y / CHUNK_SIZE,
                z: 0,
            },
            &surface_entries,
        );
        let has_log = features.iter().any(|(_, id, _)| *id == blocks.oak_log);
        let has_leaves = features.iter().any(|(_, id, _)| *id == blocks.oak_leaves);
        assert!(has_log && has_leaves, "oak tree should place logs and leaves");
    }
}
