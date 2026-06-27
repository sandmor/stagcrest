mod atlas;
mod biome;
mod block_tints;
mod colormap;
mod dust_visual;
mod models;
mod registry;

pub use atlas::TextureAtlas;
pub use biome::{BiomeRegistryClient, InterpolatedEnvironment};
pub use block_tints::{apply_block_face_tints, face_texture_for};
pub use colormap::{infer_vertical_strip_animation, sample_colormap_rgb, ColormapSet};
pub use dust_visual::{
    dust_connections_from_neighbors, dust_vertex_tint, is_dust_connectable,
    is_dust_connectable_neighbor, resolve_dust_face, DustConnections, PowerLookup,
};
pub use models::{model_variant_for_block, resolve_block_model, ModelRegistry};
pub use registry::BlockRegistry;

pub mod content;

pub const SEA_LEVEL: i32 = 62;
