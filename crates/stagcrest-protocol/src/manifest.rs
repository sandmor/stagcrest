use serde::{Deserialize, Serialize};

use crate::{AtlasRect, BlockDef, BlockId, TextureDef, TextureId};

/// Serializable block registry state sent from server to client.
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

/// Packed texture atlas pixels and placement rects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasSnapshot {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub placements: Vec<(TextureId, AtlasRect)>,
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

/// Full content package sent once after handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentManifest {
    pub loaded_mods: Vec<String>,
    pub registry: RegistrySnapshot,
    pub atlas: AtlasSnapshot,
    pub colormaps: ColormapSnapshot,
}
