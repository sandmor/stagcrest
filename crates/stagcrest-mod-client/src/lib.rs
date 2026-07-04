mod atlas;
mod biome;
mod block_tints;
mod chunk_tint;
mod colormap;
mod models;
mod registry;
mod wire_visual;

pub use atlas::TextureAtlas;
pub use biome::{BiomeRegistryClient, InterpolatedEnvironment};
pub use block_tints::apply_block_face_tints;
pub use chunk_tint::{build_chunk_tint_cells, ChunkTintCell};
pub use colormap::{infer_vertical_strip_animation, sample_colormap_rgb, ColormapSet};
pub use models::{
    model_variant_for_block, representative_state, resolve_block_model, ModelRegistry,
};
pub use registry::BlockRegistry;
pub use stagcrest_protocol::tints::face_texture_for;
pub use wire_visual::{
    compute_wire_connections, is_wire_line_block, is_wire_line_neighbor,
    resolve_wire_line_textures, wire_power_vertex_tint, wire_shows_center_junction, PowerLookup,
    WireConnections, WireLineTextures, WireLink,
};

pub mod content;

pub const SEA_LEVEL: i32 = 62;
