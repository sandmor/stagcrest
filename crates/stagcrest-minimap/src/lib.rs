mod color;
mod column;
mod column_cache;
mod column_slice;
mod framebuffer;
mod map_build;
mod map_encode;
mod map_format;
mod map_raster;
mod map_source;
mod map_tile;
mod raster;
mod resolve;
mod strip;

pub use color::{
    biome_index_at, resolve_column_color, MinimapBiomeClimate, MinimapColorCtx,
    MinimapColorCtxOwned, UNLOADED_COLOR,
};
pub use column::{surface_block, surface_block_y, ColumnBlockSource};
pub use column_cache::{ColumnCache, ColumnEntry, DEFAULT_MAX_CACHE_COLUMNS};
pub use column_slice::ColumnSlice;
pub use framebuffer::{DirtyRegion, MinimapFramebuffer};
pub use map_build::{build_map_chunk_blob, collect_world_overrides, MapBuildError, MapResolveContext};
pub use map_encode::{decode_layer_rgb, encode_layer_rgb, MapEncodeError};
pub use map_format::{MapChunkBlob, MapFormatError, MapLayer};
pub use map_raster::rasterize_map_chunk;
pub use map_source::{load_strips_for_map_chunk, MapChunkLoadInput};
pub use map_tile::{
    map_chunk_block_bounds, map_chunk_world_chunk_base, world_chunk_to_map_chunk,
    MAP_CHUNK_PIXELS, MAP_CHUNK_RGB_BYTES, MAP_CHUNK_SIZE, WORLD_CHUNKS_PER_MAP,
};
pub use raster::strip_biome_at;
pub use resolve::{strip_biome_fn, BlockDefTable, ColumnResolver};
pub use strip::StorageStripSource;
