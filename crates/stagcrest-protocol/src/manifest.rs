use serde::{Deserialize, Serialize};

use crate::{AtlasRect, BlockDef, BlockId, TextureAnimation, TextureDef, TextureId};

/// Full registry snapshot including texture pixels (in-process / tests only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySnapshot {
    pub blocks: Vec<BlockDef>,
    pub textures: Vec<TextureDef>,
    pub placeable: Vec<BlockId>,
    pub atlas_uvs: Vec<(TextureId, AtlasRect)>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub next_block_id: u32,
    pub next_texture_id: u32,
}

/// Texture metadata sent on the wire without pixel bytes (pixels live in `AtlasTransfer`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureWireDef {
    pub id: TextureId,
    pub namespaced_id: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub animation: Option<TextureAnimation>,
}

/// Serializable block registry state sent from server to client (no texture pixels).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryWireSnapshot {
    pub blocks: Vec<BlockDef>,
    pub textures: Vec<TextureWireDef>,
    pub placeable: Vec<BlockId>,
    pub atlas_uvs: Vec<(TextureId, AtlasRect)>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub next_block_id: u32,
    pub next_texture_id: u32,
}

/// Lossless PNG atlas payload sent after the content manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasTransfer {
    pub width: u32,
    pub height: u32,
    pub placements: Vec<(TextureId, AtlasRect)>,
    pub png: Vec<u8>,
}

/// Colormap PNG bytes for grass/foliage/water tinting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColormapSnapshot {
    pub grass: Vec<u8>,
    pub grass_w: u32,
    pub grass_h: u32,
    pub foliage: Vec<u8>,
    pub foliage_w: u32,
    pub foliage_h: u32,
    pub water: Vec<u8>,
    pub water_w: u32,
    pub water_h: u32,
}

/// Per-biome environmental data sent to the client for interpolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeClientDef {
    pub index: u16,
    pub namespaced_id: String,
    pub fog_color: [u8; 3],
    pub fog_density: f32,
    pub water_color: [u8; 3],
    pub water_fog_color: [u8; 3],
    pub sky_color: [u8; 3],
    pub grass_color: Option<[u8; 3]>,
    pub foliage_color: Option<[u8; 3]>,
    /// Climate anchor for colormap fallback sampling.
    pub temperature: f32,
    pub downfall: f32,
}

/// All registered biomes for client-side environment interpolation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BiomesSnapshot {
    pub biomes: Vec<BiomeClientDef>,
}

/// Content metadata sent once after handshake (atlas follows separately).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentManifest {
    pub loaded_mods: Vec<String>,
    pub registry: RegistryWireSnapshot,
    pub colormaps: ColormapSnapshot,
    #[serde(default)]
    pub biomes: BiomesSnapshot,
}

/// 4×4×4 biome index grid per chunk (64 cells).
pub const BIOME_GRID_SIZE: usize = 4;
pub const BIOME_GRID_VOLUME: usize = BIOME_GRID_SIZE * BIOME_GRID_SIZE * BIOME_GRID_SIZE;
