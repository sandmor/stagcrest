use crate::registry::BlockRegistry;
use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::WorldSeed;
use crate::worldgen::terrain::{ChunkFiller, ColumnBlocks, DensitySampler};
use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos};
use stagcrest_world::World;
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
        let filler = ChunkFiller::new(&self.config, &density, blocks);
        let entries = filler.fill_density(pos);
        ChunkGenData { pos, entries }
    }

    pub fn decorate_chunk(
        &self,
        world: &World,
        blocks: ColumnBlocks,
        data: &ChunkGenData,
    ) -> Vec<(BlockPos, BlockId, BlockState)> {
        let density = DensitySampler::new(&self.config, &self.noise, self.seed);
        let filler = ChunkFiller::new(&self.config, &density, blocks);
        filler.decorate(world, data.pos, &data.entries)
    }

    fn fill_chunk(
        &self,
        world: &mut World,
        registry: &BlockRegistry,
        pos: ChunkPos,
    ) {
        let blocks = ColumnBlocks::resolve(registry, world.air());
        let data = self.compute_chunk_density(blocks, pos);
        let entries = self.decorate_chunk(world, blocks, &data);
        world.set_blocks(entries);
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

    pub fn is_chunk_generated(&self, pos: ChunkPos) -> bool {
        self.generated_chunks.contains(&pos)
    }

    pub fn mark_chunk_generated(&mut self, pos: ChunkPos) -> bool {
        self.generated_chunks.insert(pos)
    }

    pub fn clear_chunk(&mut self, pos: ChunkPos) {
        self.generated_chunks.remove(&pos);
    }

    pub fn generate_area(
        &mut self,
        world: &mut World,
        registry: &BlockRegistry,
        center: ChunkPos,
        horizontal_radius: i32,
        vertical_radius: i32,
        y_bounds: std::ops::RangeInclusive<i32>,
    ) {
        let y_min = (center.y - vertical_radius).max(*y_bounds.start());
        let y_max = (center.y + vertical_radius).min(*y_bounds.end());
        for cx in (center.x - horizontal_radius)..=(center.x + horizontal_radius) {
            for cz in (center.z - horizontal_radius)..=(center.z + horizontal_radius) {
                for cy in y_min..=y_max {
                    self.generate_chunk(
                        world,
                        registry,
                        ChunkPos {
                            x: cx,
                            y: cy,
                            z: cz,
                        },
                    );
                }
            }
        }
    }

    pub fn generate_chunk(
        &mut self,
        world: &mut World,
        registry: &BlockRegistry,
        pos: ChunkPos,
    ) {
        if !self.generated_chunks.insert(pos) {
            return;
        }
        self.generator.fill_chunk(world, registry, pos);
    }
}

impl Default for WorldGenState {
    fn default() -> Self {
        Self::new(WorldSeed(42))
    }
}

pub fn generate_chunks(
    state: &mut WorldGenState,
    world: &mut World,
    registry: &BlockRegistry,
    chunks: impl IntoIterator<Item = ChunkPos>,
) {
    for pos in chunks {
        state.generate_chunk(world, registry, pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::BlockRegistry;
    use crate::worldgen::config::TerrainConfig;
    use crate::worldgen::world_chunk_y_bounds;

    fn test_registry() -> BlockRegistry {
        let mut reg = BlockRegistry::new();
        let tex = stagcrest_protocol::TextureId(0);
        reg.register_texture("stagcrest:stone".into(), 16, 16, vec![0; 16 * 16 * 4]);
        let face = stagcrest_protocol::BlockFaceTextures::uniform(tex);
        for (name, id) in [
            ("stagcrest:air", 0u32),
            ("stagcrest:bedrock", 1),
            ("stagcrest:stone", 2),
            ("stagcrest:dirt", 3),
            ("stagcrest:grass_block", 4),
            ("stagcrest:water", 5),
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
            });
        }
        reg
    }

    #[test]
    fn sync_generate_vertical_slice_produces_blocks() {
        let config = TerrainConfig::default();
        let y_bounds = world_chunk_y_bounds(&config);
        let mut state = WorldGenState::with_config(WorldSeed(42), config);
        let registry = test_registry();
        let air = registry.block_by_name("stagcrest:air").unwrap();
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

        for pos in positions {
            world.ensure_chunk(pos);
            state.generate_chunk(&mut world, &registry, pos);
        }

        let stone = registry.block_by_name("stagcrest:stone").unwrap();
        let mut solid_count = 0;
        for (_, chunk) in world.chunks() {
            for &bid in chunk.palette() {
                if bid == stone {
                    solid_count += 1;
                }
            }
        }
        assert!(
            solid_count > 0,
            "expected stone in generated vertical slice (y={y_min}..={y_max})"
        );
    }
}
