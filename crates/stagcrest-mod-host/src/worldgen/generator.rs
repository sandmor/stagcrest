use crate::worldgen::biome::BiomeRegistry;
use crate::worldgen::climate::ClimateSampler;
use crate::worldgen::config::TerrainConfig;
use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use crate::worldgen::features::FeaturePlacer;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::WorldSeed;
use crate::worldgen::terrain::{ChunkFiller, ColumnBlocks, DensitySampler};
use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos};
use std::collections::HashSet;

pub struct TerrainGenerator {
    pub config: TerrainConfig,
    pub seed: WorldSeed,
    noise: NoiseBank,
}

impl Clone for TerrainGenerator {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            seed: self.seed,
            noise: self.noise.clone(),
        }
    }
}

impl TerrainGenerator {
    pub fn new(seed: WorldSeed) -> Self {
        Self::with_config(seed, TerrainConfig::default())
    }

    pub fn with_config(seed: WorldSeed, config: TerrainConfig) -> Self {
        let noise = NoiseBank::new(seed);
        Self {
            config,
            seed,
            noise,
        }
    }

    pub fn climate_at(&self, wx: i32, wz: i32) -> (f32, f32) {
        ClimateSampler::new(&self.config, &self.noise).at(wx, wz)
    }

    pub fn noise_bank(&self) -> &NoiseBank {
        &self.noise
    }

    pub fn elevation_at(&self, wx: i32, wz: i32) -> f64 {
        DensitySampler::new(&self.config, &self.noise, self.seed).surface_y(wx, wz)
    }

    pub fn is_solid(&self, wx: i32, y: i32, wz: i32) -> bool {
        DensitySampler::new(&self.config, &self.noise, self.seed).is_solid(wx, y, wz)
    }

    /// Density-only stage (async-safe; decoration runs on main thread with world reads).
    pub fn compute_chunk_density(
        &self,
        blocks: ColumnBlocks,
        pos: ChunkPos,
    ) -> ChunkGenData {
        let density = DensitySampler::new(&self.config, &self.noise, self.seed);
        let climate = ClimateSampler::new(&self.config, &self.noise);
        let biomes = BiomeRegistry::default();
        let filler = ChunkFiller::new(&self.config, density, climate, &biomes, blocks);
        let entries = filler.fill_density(pos);
        ChunkGenData { pos, entries }
    }

    /// Decoration stage (async-safe; uses a pre-captured neighbor snapshot).
    pub fn decorate_chunk_offline(
        &self,
        blocks: ColumnBlocks,
        biomes: &BiomeRegistry,
        data: &ChunkGenData,
        snapshot: &DecorateSnapshot,
    ) -> Vec<(BlockPos, BlockId, BlockState)> {
        let density = DensitySampler::new(&self.config, &self.noise, self.seed);
        let climate = ClimateSampler::new(&self.config, &self.noise);
        let filler = ChunkFiller::new(&self.config, density, climate, biomes, blocks);
        let surface = filler.decorate(snapshot, data.pos, &data.entries);

        let placer = FeaturePlacer::new(&self.config, &self.noise, blocks, biomes, self.seed);
        let features = placer.place(snapshot, data.pos, &surface);
        let mut merged = surface;
        merged.extend(features);
        merged
    }
}

#[derive(Debug, Clone)]
pub struct ChunkGenData {
    pub pos: ChunkPos,
    pub entries: Vec<(BlockPos, BlockId, BlockState)>,
}

pub struct WorldGenState {
    generator: TerrainGenerator,
    pub generated_chunks: HashSet<ChunkPos>,
}

impl WorldGenState {
    pub fn new(seed: WorldSeed) -> Self {
        Self {
            generator: TerrainGenerator::new(seed),
            generated_chunks: HashSet::new(),
        }
    }

    pub fn with_config(seed: WorldSeed, config: TerrainConfig) -> Self {
        Self {
            generator: TerrainGenerator::with_config(seed, config),
            generated_chunks: HashSet::new(),
        }
    }

    pub fn seed(&self) -> WorldSeed {
        self.generator.seed
    }

    pub fn config(&self) -> &TerrainConfig {
        &self.generator.config
    }

    pub fn generator(&self) -> &TerrainGenerator {
        &self.generator
    }

    pub fn climate_at(&self, wx: i32, wz: i32) -> (f32, f32) {
        self.generator.climate_at(wx, wz)
    }

    pub fn is_chunk_generated(&self, pos: ChunkPos) -> bool {
        self.generated_chunks.contains(&pos)
    }

    pub fn mark_chunk_generated(&mut self, pos: ChunkPos) -> bool {
        self.generated_chunks.insert(pos)
    }

    pub fn clear_chunk(&mut self, pos: ChunkPos) {
        self.generated_chunks.remove(&pos);
    }
}

impl Default for WorldGenState {
    fn default() -> Self {
        Self::new(WorldSeed(42))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::config::TerrainConfig;
    use crate::worldgen::test_fixtures::{test_biomes, test_registry};
    use crate::worldgen::world_chunk_y_bounds;
    use stagcrest_world::World;

    #[test]
    fn pipeline_generate_vertical_slice_produces_blocks() {
        let config = TerrainConfig::default();
        let y_bounds = world_chunk_y_bounds(&config);
        let mut state = WorldGenState::with_config(WorldSeed(42), config);
        let mut registry = test_registry();
        let biomes = test_biomes(&mut registry);
        let air = registry.block_by_name("stagcrest:air").unwrap();
        let blocks = ColumnBlocks::resolve(&registry, air);
        let mut world = World::new(air);

        let center_y = 5i32;
        let v_radius = 4i32;
        let y_min = (center_y - v_radius).max(*y_bounds.start());
        let y_max = (center_y + v_radius).min(*y_bounds.end());

        let mut positions = Vec::new();
        for cy in (y_min..=y_max).rev() {
            for cx in -2..=2 {
                for cz in -2..=2 {
                    positions.push(ChunkPos { x: cx, y: cy, z: cz });
                }
            }
        }

        let generator = state.generator().clone();
        for pos in positions {
            world.ensure_chunk(pos);
            let data = generator.compute_chunk_density(blocks, pos);
            let snapshot = DecorateSnapshot::capture(&world, pos, air);
            let entries = generator.decorate_chunk_offline(blocks, &biomes, &data, &snapshot);
            world.set_blocks(entries);
            world.finalize_generated_chunk(pos);
            state.mark_chunk_generated(pos);
        }

        let stone = registry.block_by_name("stagcrest:stone").unwrap();
        let mut solid_count = 0;
        for cpos in world.loaded_chunk_positions() {
            for y in 0..stagcrest_protocol::CHUNK_SIZE {
                for z in 0..stagcrest_protocol::CHUNK_SIZE {
                    for x in 0..stagcrest_protocol::CHUNK_SIZE {
                        let (bid, _) = world.get_block(stagcrest_protocol::BlockPos::new(
                            cpos.x * stagcrest_protocol::CHUNK_SIZE + x,
                            cpos.y * stagcrest_protocol::CHUNK_SIZE + y,
                            cpos.z * stagcrest_protocol::CHUNK_SIZE + z,
                        ));
                        if bid == stone {
                            solid_count += 1;
                        }
                    }
                }
            }
        }
        assert!(
            solid_count > 0,
            "expected stone in generated vertical slice (y={y_min}..={y_max})"
        );
    }
}
