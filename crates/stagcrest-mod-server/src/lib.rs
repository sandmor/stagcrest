mod assets;
mod atlas;
mod block_tints;
mod colormap;
mod host;
mod mount_placement;
mod registry;
mod repeater_placement;
mod resourcepack;
mod runtime;
mod torch_placement;
mod worldgen;

pub use assets::{AssetError, AssetReader, FsAssetReader};
pub use atlas::TextureAtlas;
pub use block_tints::{apply_block_face_tints, face_texture_for};
pub use colormap::{sample_colormap_rgb, ColormapSet};
pub use host::{load_mods, ModError, ModHost};
pub use mount_placement::{mount_can_attach, validate_mount_placement};
pub use registry::BlockRegistry;
pub use repeater_placement::validate_repeater_placement;
pub use resourcepack::{
    infer_vertical_strip_animation, ResourcePackLoader, DEFAULT_MC_BLOCK_TEXTURES,
};
pub use worldgen::{
    terrain_chunk_y_range, world_chunk_y_bounds, BiomeRegistry, ChunkGenData, ClimateSampler,
    ColumnBlocks, ColumnData, DecorateSnapshot, FeatureKind, RegisterBiomeFeatureRequest,
    RegisterBiomeRequest, ResolvedBiome, TerrainConfig, TerrainGenerator, WorldGenState, WorldSeed,
    SEA_LEVEL,
};
pub mod manifest;

pub use torch_placement::{torch_can_attach, torch_state_from_normal, validate_torch_placement};
