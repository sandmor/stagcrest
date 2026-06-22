mod dust_visual;
mod atlas;
mod assets;
mod block_tints;
mod colormap;
mod host;
mod models;
mod mount_placement;
mod repeater_placement;
mod registry;
mod resourcepack;
mod runtime;
mod torch_placement;
mod worldgen;

pub use assets::{AssetError, AssetReader, FsAssetReader};
#[cfg(target_arch = "wasm32")]
pub use assets::HttpAssetReader;
pub use atlas::TextureAtlas;
pub use block_tints::{apply_block_face_tints, face_texture_for};
pub use dust_visual::{
    dust_connections_from_neighbors, dust_vertex_tint, is_dust_connectable,
    is_dust_connectable_neighbor, resolve_dust_face, DustConnections, PowerLookup,
};
pub use colormap::{sample_colormap_rgb, ColormapSet};
pub use host::{ModError, ModHost};
pub use models::{ModelRegistry, model_variant_for_block, resolve_block_model};
pub use mount_placement::{mount_can_attach, validate_mount_placement};
pub use repeater_placement::validate_repeater_placement;
#[cfg(not(target_arch = "wasm32"))]
pub use host::load_mods;
#[cfg(target_arch = "wasm32")]
pub use host::load_mods_async;
pub use registry::BlockRegistry;
pub use resourcepack::{ResourcePackLoader, infer_vertical_strip_animation};
pub use torch_placement::{torch_can_attach, torch_state_from_normal, validate_torch_placement};
pub use worldgen::{
    generate_chunks, terrain_chunk_y_range, world_chunk_y_bounds, BiomeRegistry, ChunkGenData,
    ClimateSampler, ColumnBlocks, ColumnData, FeatureKind, RegisterBiomeFeatureRequest,
    RegisterBiomeRequest, ResolvedBiome, TerrainConfig, TerrainGenerator, WorldGenState,
    WorldSeed, SEA_LEVEL,
};
pub use worldgen::noise::NoiseBank;
