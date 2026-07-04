use stagcrest_mod_server::{
    BiomeRegistry, BlockRegistry, ChunkGenData, ColumnBlocks, DecorateSnapshot, TerrainGenerator,
    WorldGenState,
};
use stagcrest_protocol::{BlockId, ChunkPos};
use stagcrest_world::World;

/// Apply precomputed density results to the in-memory world (sequential).
pub fn apply_density_batch(
    world: &mut World,
    terrain: &mut WorldGenState,
    results: &[ChunkGenData],
) {
    for data in results {
        world.ensure_chunk(data.pos);
        if !world.is_generated(data.pos) {
            world.set_blocks(data.entries.clone());
            world.mark_chunk_terrain_ready(data.pos);
        }
        terrain.mark_chunk_terrain_ready(data.pos);
    }
}

/// Pass 1: compute terrain density and apply to the in-memory world.
pub fn apply_pass1_density(
    world: &mut World,
    terrain: &mut WorldGenState,
    generator: &TerrainGenerator,
    column_blocks: ColumnBlocks,
    pos: ChunkPos,
) -> ChunkGenData {
    world.ensure_chunk(pos);
    let data = generator.compute_chunk_density(column_blocks, pos);
    if !world.is_generated(pos) {
        world.set_blocks(data.entries.clone());
        world.mark_chunk_terrain_ready(pos);
    }
    terrain.mark_chunk_terrain_ready(pos);
    data
}

/// Pass 2: decorate a terrain-ready chunk and finalize it in the world.
pub fn apply_pass2_decorate(
    world: &mut World,
    terrain: &mut WorldGenState,
    generator: &TerrainGenerator,
    column_blocks: ColumnBlocks,
    biomes: &BiomeRegistry,
    registry: &BlockRegistry,
    air: BlockId,
    data: &ChunkGenData,
) {
    let pos = data.pos;
    let snapshot = DecorateSnapshot::capture(world, pos, air);
    let decorated =
        generator.decorate_chunk_offline(column_blocks, biomes, registry, data, &snapshot);
    terrain.mark_chunk_generated(pos);
    terrain.store_biome_grid(pos, decorated.biome_grid);
    if !world.is_generated(pos) {
        world.set_blocks(decorated.blocks);
        world.finalize_generated_chunk(pos);
    }
}
