use crate::registry::BlockRegistry;
use noise::{NoiseFn, SuperSimplex};
use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos, CHUNK_SIZE};
use stagcrest_world::World;
use std::collections::HashSet;

const SEED: u32 = 42;
const BASE_HEIGHT: i32 = 8;
const AMPLITUDE: i32 = 8;
const MIN_HEIGHT: i32 = 4;
const MAX_HEIGHT: i32 = 32;
const NOISE_SCALE: f64 = 0.02;

pub struct WorldGenState {
    pub generated_columns: HashSet<(i32, i32)>,
}

impl Default for WorldGenState {
    fn default() -> Self {
        Self {
            generated_columns: HashSet::new(),
        }
    }
}

impl WorldGenState {
    pub fn generate_area(
        &mut self,
        world: &mut World,
        registry: &BlockRegistry,
        center: ChunkPos,
        radius: i32,
    ) {
        for cx in (center.x - radius)..=(center.x + radius) {
            for cz in (center.z - radius)..=(center.z + radius) {
                for lx in 0..CHUNK_SIZE {
                    for lz in 0..CHUNK_SIZE {
                        let wx = cx * CHUNK_SIZE + lx;
                        let wz = cz * CHUNK_SIZE + lz;
                        self.generate_column(world, registry, wx, wz);
                    }
                }
            }
        }
    }

    pub fn generate_column(
        &mut self,
        world: &mut World,
        registry: &BlockRegistry,
        wx: i32,
        wz: i32,
    ) {
        if !self.generated_columns.insert((wx, wz)) {
            return;
        }

        let bedrock = registry
            .block_by_name("stagcrest:bedrock")
            .unwrap_or(BlockId(0));
        let stone = registry
            .block_by_name("stagcrest:stone")
            .unwrap_or(BlockId(0));
        let dirt = registry
            .block_by_name("stagcrest:dirt")
            .unwrap_or(BlockId(0));
        let grass = registry
            .block_by_name("stagcrest:grass_block")
            .unwrap_or(BlockId(0));
        let air = world.air();

        let perlin = SuperSimplex::new(SEED);
        let noise = perlin.get([wx as f64 * NOISE_SCALE, wz as f64 * NOISE_SCALE]);
        let height = (BASE_HEIGHT as f64 + noise * AMPLITUDE as f64).round() as i32;
        let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);

        world.set_block(BlockPos::new(wx, 0, wz), bedrock, BlockState(0));

        for y in 1..height {
            let block = if y <= height - 3 {
                stone
            } else if y == height - 1 {
                grass
            } else {
                dirt
            };
            world.set_block(BlockPos::new(wx, y, wz), block, BlockState(0));
        }

        for y in height..MAX_HEIGHT {
            let pos = BlockPos::new(wx, y, wz);
            let (existing, _) = world.get_block(pos);
            if existing != air {
                world.set_block(pos, air, BlockState(0));
            }
        }
    }
}

pub fn generate_columns_for_chunks(
    state: &mut WorldGenState,
    world: &mut World,
    registry: &BlockRegistry,
    chunks: impl IntoIterator<Item = ChunkPos>,
) {
    for chunk_pos in chunks {
        for lx in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let wx = chunk_pos.x * CHUNK_SIZE + lx;
                let wz = chunk_pos.z * CHUNK_SIZE + lz;
                state.generate_column(world, registry, wx, wz);
            }
        }
    }
}
