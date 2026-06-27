use crate::worldgen::biome::{BiomeDimension, BiomeRegistry};
use crate::worldgen::climate::ClimateSampler;
use crate::worldgen::config::TerrainConfig;
use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use crate::worldgen::features::FeaturePlacer;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::WorldSeed;
use crate::worldgen::terrain::{CaveDecorator, ChunkFiller, ColumnBlocks, DensitySampler};
use stagcrest_protocol::manifest::BIOME_GRID_VOLUME;
use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos, CHUNK_SIZE};
use std::collections::HashMap;

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

    pub fn compute_chunk_density(&self, blocks: ColumnBlocks, pos: ChunkPos) -> ChunkGenData {
        let density = DensitySampler::new(&self.config, &self.noise, self.seed);
        let climate = ClimateSampler::new(&self.config, &self.noise);
        let biomes = BiomeRegistry::default();
        let filler = ChunkFiller::new(&self.config, density, climate, &biomes, blocks);
        let entries = filler.fill_density(pos);
        ChunkGenData {
            pos,
            entries,
            biome_grid: [0u8; BIOME_GRID_VOLUME],
        }
    }

    pub fn build_biome_grid(
        &self,
        biomes: &BiomeRegistry,
        pos: ChunkPos,
        density: &DensitySampler<'_>,
        climate: &ClimateSampler<'_>,
    ) -> [u8; BIOME_GRID_VOLUME] {
        let base_x = pos.x * CHUNK_SIZE;
        let base_y = pos.y * CHUNK_SIZE;
        let base_z = pos.z * CHUNK_SIZE;
        let cell = CHUNK_SIZE / 4;
        let mut grid = [0u8; BIOME_GRID_VOLUME];

        for gz in 0..4 {
            for gy in 0..4 {
                for gx in 0..4 {
                    let wx = base_x + gx * cell + cell / 2;
                    let y = base_y + gy * cell + cell / 2;
                    let wz = base_z + gz * cell + cell / 2;
                    let surface_y = density.surface_y(wx, wz);
                    let params = climate.at_3d(wx, y, wz, surface_y);

                    let dimension = if density.sky_islands().is_solid(wx, y, wz) {
                        BiomeDimension::SkyIsland
                    } else if (y as f64) < surface_y - 8.0 {
                        BiomeDimension::Underground
                    } else if density.is_river(wx, wz) {
                        BiomeDimension::Surface
                    } else {
                        BiomeDimension::Surface
                    };

                    let mut biome = biomes.biome_at(params, dimension);

                    // River overrides
                    if density.is_river(wx, wz) && y <= self.config.sea_level {
                        if params.temperature < 0.15 {
                            if let Some(frozen) = biomes.index_of("stagcrest:frozen_river") {
                                biome = biomes.biome_by_index(frozen).unwrap_or(biome);
                            }
                        } else if let Some(river) = biomes.index_of("stagcrest:river") {
                            biome = biomes.biome_by_index(river).unwrap_or(biome);
                        }
                    } else if density.is_riverbank(wx, wz) {
                        if let Some(bank) = biomes.index_of("stagcrest:riverbank") {
                            biome = biomes.biome_by_index(bank).unwrap_or(biome);
                        }
                    }

                    let idx = (gx + gy * 4 + gz * 16) as usize;
                    grid[idx] = biome.index as u8;
                }
            }
        }
        grid
    }

    pub fn decorate_chunk_offline(
        &self,
        blocks: ColumnBlocks,
        biomes: &BiomeRegistry,
        registry: &crate::registry::BlockRegistry,
        data: &ChunkGenData,
        snapshot: &DecorateSnapshot,
    ) -> DecorateResult {
        let density = DensitySampler::new(&self.config, &self.noise, self.seed);
        let climate = ClimateSampler::new(&self.config, &self.noise);
        let biome_grid = self.build_biome_grid(biomes, data.pos, &density, &climate);

        let filler = ChunkFiller::new(&self.config, density, climate, biomes, blocks);
        let surface = filler.decorate(snapshot, data.pos, &data.entries, &biome_grid);

        let placer = FeaturePlacer::new(
            &self.config,
            &self.noise,
            blocks,
            biomes,
            registry,
            self.seed,
        );
        let features = placer.place(snapshot, data.pos, &surface, &biome_grid);

        let cave_decor = CaveDecorator::new(blocks, biomes, registry);
        let cave_features = cave_decor.decorate_caves(
            snapshot,
            data.pos,
            &data.entries,
            &ClimateSampler::new(&self.config, &self.noise),
            &DensitySampler::new(&self.config, &self.noise, self.seed),
        );

        let mut populate = surface;
        populate.extend(features);
        populate.extend(cave_features);

        DecorateResult {
            blocks: populate,
            biome_grid,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkGenData {
    pub pos: ChunkPos,
    pub entries: Vec<(BlockPos, BlockId, BlockState)>,
    pub biome_grid: [u8; BIOME_GRID_VOLUME],
}

#[derive(Debug, Clone)]
pub struct DecorateResult {
    pub blocks: Vec<(BlockPos, BlockId, BlockState)>,
    pub biome_grid: [u8; BIOME_GRID_VOLUME],
}

pub struct WorldGenState {
    generator: TerrainGenerator,
    pub terrain_ready_chunks: std::collections::HashSet<ChunkPos>,
    pub generated_chunks: std::collections::HashSet<ChunkPos>,
    pub biome_grids: HashMap<ChunkPos, [u8; BIOME_GRID_VOLUME]>,
}

impl WorldGenState {
    pub fn new(seed: WorldSeed) -> Self {
        Self {
            generator: TerrainGenerator::new(seed),
            terrain_ready_chunks: std::collections::HashSet::new(),
            generated_chunks: std::collections::HashSet::new(),
            biome_grids: HashMap::new(),
        }
    }

    pub fn with_config(seed: WorldSeed, config: TerrainConfig) -> Self {
        Self {
            generator: TerrainGenerator::with_config(seed, config),
            terrain_ready_chunks: std::collections::HashSet::new(),
            generated_chunks: std::collections::HashSet::new(),
            biome_grids: HashMap::new(),
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

    pub fn biome_grid(&self, pos: ChunkPos) -> Option<&[u8; BIOME_GRID_VOLUME]> {
        self.biome_grids.get(&pos)
    }

    pub fn store_biome_grid(&mut self, pos: ChunkPos, grid: [u8; BIOME_GRID_VOLUME]) {
        self.biome_grids.insert(pos, grid);
    }

    pub fn is_chunk_terrain_ready(&self, pos: ChunkPos) -> bool {
        self.terrain_ready_chunks.contains(&pos) || self.generated_chunks.contains(&pos)
    }

    pub fn is_chunk_generated(&self, pos: ChunkPos) -> bool {
        self.generated_chunks.contains(&pos)
    }

    pub fn mark_chunk_terrain_ready(&mut self, pos: ChunkPos) -> bool {
        self.terrain_ready_chunks.insert(pos)
    }

    pub fn mark_chunk_generated(&mut self, pos: ChunkPos) -> bool {
        self.terrain_ready_chunks.insert(pos);
        self.generated_chunks.insert(pos)
    }

    pub fn clear_chunk(&mut self, pos: ChunkPos) {
        self.terrain_ready_chunks.remove(&pos);
        self.generated_chunks.remove(&pos);
        self.biome_grids.remove(&pos);
    }
}

impl Default for WorldGenState {
    fn default() -> Self {
        Self::new(WorldSeed(42))
    }
}
