use crate::registry::BlockRegistry;
use crate::worldgen::biome::BiomeRegistry;
use crate::worldgen::config::TerrainConfig;
use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use crate::worldgen::hybrid_tree::HybridTreeGenerator;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::occupancy::OccupancyMap;
use crate::worldgen::seed::WorldSeed;
use crate::worldgen::terrain::{ColumnBlocks, SkyIslandSampler};
use stagcrest_mod_sdk::{FeaturePlacement, TreeShape};
use stagcrest_protocol::manifest::BIOME_GRID_VOLUME;
use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos, CHUNK_SIZE};

pub struct FeaturePlacer<'a> {
    config: &'a TerrainConfig,
    sky_islands: SkyIslandSampler<'a>,
    blocks: ColumnBlocks,
    biomes: &'a BiomeRegistry,
    registry: &'a BlockRegistry,
    seed: WorldSeed,
}

impl<'a> FeaturePlacer<'a> {
    pub fn new(
        config: &'a TerrainConfig,
        noise: &'a NoiseBank,
        blocks: ColumnBlocks,
        biomes: &'a BiomeRegistry,
        registry: &'a BlockRegistry,
        seed: WorldSeed,
    ) -> Self {
        Self {
            config,
            sky_islands: SkyIslandSampler::new(config, noise, seed),
            blocks,
            biomes,
            registry,
            seed,
        }
    }

    pub fn place(
        &self,
        snapshot: &DecorateSnapshot,
        pos: ChunkPos,
        surface_entries: &[(BlockPos, BlockId, BlockState)],
        biome_grid: &[u8; BIOME_GRID_VOLUME],
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

        let mut occupancy =
            OccupancyMap::from_surface_entries(surface_entries, pos, self.blocks.air);
        let mut features = Vec::new();

        for lz in 0..CHUNK_SIZE as usize {
            for lx in 0..CHUNK_SIZE as usize {
                let idx = lz * chunk_area + lx;
                let Some((surface_y, surface_id)) = top_surface[idx] else {
                    continue;
                };

                let wx = base_x + lx as i32;
                let wz = base_z + lz as i32;
                let gx = (lx / 4).min(3);
                let gy = ((surface_y - pos.y * CHUNK_SIZE) / 4).clamp(0, 3) as usize;
                let gz = (lz / 4).min(3);
                let biome_idx = biome_grid[gx + gy * 4 + gz * 16] as u16;
                let biome = self
                    .biomes
                    .biome_by_index(biome_idx)
                    .unwrap_or_else(|| self.biomes.default_plains());

                let on_island = self.sky_islands.is_solid(wx, surface_y, wz);
                let above_y = surface_y + 1;
                let above_pos = BlockPos::new(wx, above_y, wz);
                if !occupancy.can_place(snapshot, above_pos) {
                    continue;
                }

                let is_grass_surface = surface_id == self.blocks.grass
                    || surface_id == biome.surface_top
                    || (on_island && surface_id == self.blocks.grass);
                let is_sand_surface = surface_id == self.blocks.sand;

                for feature in &biome.features {
                    self.place_feature(
                        &feature.placement,
                        feature.chance,
                        wx,
                        wz,
                        above_y,
                        is_grass_surface,
                        is_sand_surface,
                        on_island,
                        snapshot,
                        &mut occupancy,
                        &mut features,
                    );
                }
            }
        }

        features
    }

    fn place_feature(
        &self,
        placement: &FeaturePlacement,
        chance: f32,
        wx: i32,
        wz: i32,
        above_y: i32,
        is_grass_surface: bool,
        is_sand_surface: bool,
        on_island: bool,
        snapshot: &DecorateSnapshot,
        occupancy: &mut OccupancyMap,
        features: &mut Vec<(BlockPos, BlockId, BlockState)>,
    ) {
        let roll = hash_chance(self.seed, wx, wz, placement);
        if roll >= chance {
            return;
        }

        match placement {
            FeaturePlacement::Plant { block, tall } => {
                if !is_grass_surface && !on_island {
                    return;
                }
                let id = self.resolve(block);
                if id == self.blocks.air {
                    return;
                }
                let pos = BlockPos::new(wx, above_y, wz);
                if !occupancy.can_place(snapshot, pos) {
                    return;
                }
                features.push((pos, id, BlockState(0)));
                occupancy.place(pos, id);
                if *tall {
                    let pos2 = BlockPos::new(wx, above_y + 1, wz);
                    if occupancy.can_place(snapshot, pos2) {
                        features.push((pos2, id, BlockState(0)));
                        occupancy.place(pos2, id);
                    }
                }
            }
            FeaturePlacement::Column { block, height } => {
                if !is_sand_surface && !is_grass_surface {
                    return;
                }
                let id = self.resolve(block);
                if id == self.blocks.air {
                    return;
                }
                let h = *height as i32;
                for dy in 0..h {
                    let pos = BlockPos::new(wx, above_y + dy, wz);
                    if !occupancy.can_place(snapshot, pos) {
                        break;
                    }
                    features.push((pos, id, BlockState(0)));
                    occupancy.place(pos, id);
                }
            }
            FeaturePlacement::Tree {
                trunk,
                leaves,
                shape,
                height,
            } if is_grass_surface || on_island => {
                let trunk_id = self.resolve(trunk);
                let leaves_id = self.resolve(leaves);
                if trunk_id == self.blocks.air || leaves_id == self.blocks.air {
                    return;
                }
                match shape {
                    TreeShape::Oak | TreeShape::Birch | TreeShape::Cherry => {
                        HybridTreeGenerator::place_oak(
                            wx,
                            above_y,
                            wz,
                            &self.blocks_with(trunk_id, leaves_id),
                            snapshot,
                            occupancy,
                            features,
                            self.config,
                            self.seed,
                        );
                    }
                    TreeShape::Spruce | TreeShape::Pine => {
                        self.place_spruce(
                            wx, above_y, wz, trunk_id, leaves_id, *height, snapshot, occupancy,
                            features,
                        );
                    }
                    TreeShape::Jungle => {
                        HybridTreeGenerator::place_oak(
                            wx,
                            above_y,
                            wz,
                            &self.blocks_with(trunk_id, leaves_id),
                            snapshot,
                            occupancy,
                            features,
                            self.config,
                            self.seed,
                        );
                    }
                    _ => {
                        HybridTreeGenerator::place_oak(
                            wx,
                            above_y,
                            wz,
                            &self.blocks_with(trunk_id, leaves_id),
                            snapshot,
                            occupancy,
                            features,
                            self.config,
                            self.seed,
                        );
                    }
                }
            }
            FeaturePlacement::SurfacePatch { block } if is_grass_surface => {
                let id = self.resolve(block);
                if id == self.blocks.air {
                    return;
                }
                let pos = BlockPos::new(wx, above_y, wz);
                if occupancy.can_place(snapshot, pos) {
                    features.push((pos, id, BlockState(0)));
                    occupancy.place(pos, id);
                }
            }
            FeaturePlacement::GlowFlora { block } => {
                let id = self.resolve(block);
                if id == self.blocks.air {
                    return;
                }
                let pos = BlockPos::new(wx, above_y, wz);
                if occupancy.can_place(snapshot, pos) {
                    features.push((pos, id, BlockState(0)));
                    occupancy.place(pos, id);
                }
            }
            FeaturePlacement::IceSpike if is_grass_surface => {
                if let Some(ice) = self.registry.block_by_name("stagcrest:packed_ice") {
                    for dy in 0..4 {
                        let pos = BlockPos::new(wx, above_y + dy, wz);
                        if occupancy.can_place(snapshot, pos) {
                            features.push((pos, ice, BlockState(0)));
                            occupancy.place(pos, ice);
                        }
                    }
                }
            }
            FeaturePlacement::Stalagmite { block } => {
                let id = self.resolve(block);
                if id == self.blocks.air {
                    return;
                }
                let pos = BlockPos::new(wx, above_y, wz);
                if occupancy.can_place(snapshot, pos) {
                    features.push((pos, id, BlockState(0)));
                    occupancy.place(pos, id);
                }
            }
            FeaturePlacement::Stalactite { block } => {
                let id = self.resolve(block);
                if id == self.blocks.air {
                    return;
                }
                let hang_y = above_y;
                let above = snapshot.block_at(BlockPos::new(wx, hang_y + 1, wz));
                if above == self.blocks.air || above == self.blocks.water {
                    return;
                }
                let pos = BlockPos::new(wx, hang_y, wz);
                if occupancy.can_place(snapshot, pos) {
                    features.push((pos, id, BlockState(0)));
                    occupancy.place(pos, id);
                }
            }
            _ => {}
        }
    }

    fn place_spruce(
        &self,
        wx: i32,
        above_y: i32,
        wz: i32,
        trunk: BlockId,
        leaves: BlockId,
        height: u8,
        snapshot: &DecorateSnapshot,
        occupancy: &mut OccupancyMap,
        features: &mut Vec<(BlockPos, BlockId, BlockState)>,
    ) {
        let h = height.max(5) as i32;
        for dy in 0..h {
            let pos = BlockPos::new(wx, above_y + dy, wz);
            if occupancy.can_place(snapshot, pos) {
                features.push((pos, trunk, BlockState(0)));
                occupancy.place(pos, trunk);
            }
        }
        let top = above_y + h;
        for dx in -2i32..=2 {
            for dz in -2i32..=2 {
                for dy in 0i32..3 {
                    if dx.abs() + dz.abs() > 2 && dy > 0 {
                        continue;
                    }
                    let pos = BlockPos::new(wx + dx, top + dy, wz + dz);
                    if occupancy.can_place(snapshot, pos) {
                        features.push((pos, leaves, BlockState(0)));
                        occupancy.place(pos, leaves);
                    }
                }
            }
        }
    }

    fn blocks_with(&self, trunk: BlockId, leaves: BlockId) -> ColumnBlocks {
        let mut b = self.blocks;
        b.oak_log = trunk;
        b.oak_leaves = leaves;
        b
    }

    fn resolve(&self, name: &str) -> BlockId {
        self.registry.block_by_name(name).unwrap_or(self.blocks.air)
    }
}

fn hash_chance(seed: WorldSeed, wx: i32, wz: i32, placement: &FeaturePlacement) -> f32 {
    let tag = placement_discriminant(placement);
    let h = seed
        .0
        .wrapping_add(wx as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(wz as u64)
        .wrapping_add(tag.wrapping_mul(0x517C_C1B7_2722_0A95));
    let mixed = (h ^ (h >> 33)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (mixed as f32 / u64::MAX as f32).clamp(0.0, 0.9999)
}

fn placement_discriminant(placement: &FeaturePlacement) -> u64 {
    use FeaturePlacement::*;
    match placement {
        Plant { .. } => 1,
        Patch { .. } => 2,
        Tree { .. } => 3,
        Boulder { .. } => 4,
        Column { .. } => 5,
        IceSpike => 6,
        Stalagmite { .. } => 7,
        Stalactite { .. } => 8,
        SurfacePatch { .. } => 9,
        GlowFlora { .. } => 10,
    }
}
