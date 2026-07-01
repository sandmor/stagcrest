use std::collections::HashMap;

use stagcrest_protocol::{BlockDef, BlockId, ChunkPos};
use thiserror::Error;

use crate::color::{MinimapBiomeClimate, MinimapColorCtxOwned};
use crate::map_format::MapChunkBlob;
use crate::map_raster::rasterize_map_chunk;
use crate::map_source::{load_strips_for_map_chunk, MapChunkLoadInput};
use crate::raster::strip_biome_at;
use crate::resolve::{BlockDefTable, ColumnResolver};
use crate::strip::StorageStripSource;

/// Owned color/block context for map chunk builds (safe to share across threads).
#[derive(Clone)]
pub struct MapResolveContext {
    pub table: BlockDefTable,
    pub air: BlockId,
    pub y_min: i32,
    pub y_max: i32,
    pub color_ctx: MinimapColorCtxOwned,
    pub biome_climates: Vec<MinimapBiomeClimate>,
}

impl MapResolveContext {
    pub fn new(
        block_defs: Vec<BlockDef>,
        air: BlockId,
        y_min: i32,
        y_max: i32,
        color_ctx: MinimapColorCtxOwned,
        biome_climates: Vec<MinimapBiomeClimate>,
    ) -> Self {
        Self {
            table: BlockDefTable::new(block_defs),
            air,
            y_min,
            y_max,
            color_ctx,
            biome_climates,
        }
    }

    pub fn climate_for_index(&self, idx: u16) -> MinimapBiomeClimate {
        self.biome_climates
            .get(idx as usize)
            .copied()
            .unwrap_or_default()
    }

    pub fn resolver(&self) -> ColumnResolver<'_> {
        ColumnResolver {
            table: &self.table,
            air: self.air,
            y_min: self.y_min,
            y_max: self.y_max,
            color_ctx: self.color_ctx.as_ctx(),
        }
    }
}

#[derive(Debug, Error)]
pub enum MapBuildError {
    #[error("storage: {0}")]
    Storage(#[from] stagcrest_storage::StorageError),
}

/// Build encoded map chunk bytes for `(mx, mz)`.
pub fn build_map_chunk_blob(
    input: &MapChunkLoadInput<'_>,
    ctx: &MapResolveContext,
) -> Result<Vec<u8>, MapBuildError> {
    let strips = load_strips_for_map_chunk(input)?;
    let rgb = rasterize_map_chunk_with_ctx(&strips, input.mx, input.mz, ctx);
    Ok(MapChunkBlob::encode_single_layer(ctx.y_min, ctx.y_max, &rgb))
}

fn rasterize_map_chunk_with_ctx(
    strips: &[[StorageStripSource; 4]; 4],
    mx: i32,
    mz: i32,
    ctx: &MapResolveContext,
) -> [u8; crate::map_tile::MAP_CHUNK_RGB_BYTES] {
    let resolver = ctx.resolver();
    let climate = &ctx.biome_climates;
    rasterize_map_chunk(strips, mx, mz, &resolver, &|wx, wy, wz| {
        let base_cx = mx * crate::map_tile::WORLD_CHUNKS_PER_MAP;
        let base_cz = mz * crate::map_tile::WORLD_CHUNKS_PER_MAP;
        let cx = stagcrest_protocol::floor_div(wx, stagcrest_protocol::CHUNK_SIZE);
        let cz = stagcrest_protocol::floor_div(wz, stagcrest_protocol::CHUNK_SIZE);
        let dx = cx - base_cx;
        let dz = cz - base_cz;
        if (0..4).contains(&dx) && (0..4).contains(&dz) {
            let strip = &strips[dz as usize][dx as usize];
            return strip_biome_at(strip, wx, wy, wz, &|idx| {
                climate
                    .get(idx as usize)
                    .copied()
                    .unwrap_or_default()
            });
        }
        MinimapBiomeClimate::default()
    })
}

/// Collect in-memory chunk overrides for a map tile region from a live world.
pub fn collect_world_overrides(
    world: &stagcrest_world::World,
    mx: i32,
    mz: i32,
    y_chunks: &[i32],
) -> HashMap<ChunkPos, stagcrest_storage::InactiveChunk> {
    let base_cx = mx * crate::map_tile::WORLD_CHUNKS_PER_MAP;
    let base_cz = mz * crate::map_tile::WORLD_CHUNKS_PER_MAP;
    let mut out = HashMap::new();
    for dz in 0..crate::map_tile::WORLD_CHUNKS_PER_MAP {
        for dx in 0..crate::map_tile::WORLD_CHUNKS_PER_MAP {
            let cx = base_cx + dx;
            let cz = base_cz + dz;
            for &cy in y_chunks {
                let pos = ChunkPos { x: cx, y: cy, z: cz };
                if world.has_chunk(pos) && world.is_populated(pos) {
                    if let Some(chunk) = world.pack_chunk_for_storage(pos) {
                        out.insert(pos, chunk);
                    }
                }
            }
        }
    }
    out
}
