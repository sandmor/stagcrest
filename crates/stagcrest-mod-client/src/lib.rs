mod atlas;
mod biome;
mod block_tints;
mod colormap;
mod wire_visual;
mod models;
mod registry;

pub use atlas::TextureAtlas;
pub use biome::{BiomeRegistryClient, InterpolatedEnvironment};
pub use block_tints::{apply_block_face_tints, face_texture_for};
pub use colormap::{infer_vertical_strip_animation, sample_colormap_rgb, ColormapSet};
pub use wire_visual::{
    compute_wire_connections, is_wire_line_block, is_wire_line_neighbor,
    resolve_wire_line_textures, wire_power_vertex_tint, wire_shows_center_junction,
    WireConnections, WireLineTextures, WireLink, PowerLookup,
};
pub use models::{model_variant_for_block, representative_state, resolve_block_model, ModelRegistry};
pub use registry::BlockRegistry;

pub mod content;

pub const SEA_LEVEL: i32 = 62;
