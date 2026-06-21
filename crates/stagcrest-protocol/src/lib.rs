use serde::{Deserialize, Serialize};

pub const CHUNK_SIZE: i32 = 16;
pub const CHUNK_VOLUME: usize = (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Numeric block identifier assigned at mod registration time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct BlockId(pub u32);

/// Compact per-block instance data (facing, powered, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct BlockState(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn chunk_pos(&self) -> ChunkPos {
        ChunkPos {
            x: floor_div(self.x, CHUNK_SIZE),
            y: floor_div(self.y, CHUNK_SIZE),
            z: floor_div(self.z, CHUNK_SIZE),
        }
    }

    pub fn local(&self) -> LocalBlockPos {
        LocalBlockPos {
            x: rem_euclid(self.x, CHUNK_SIZE) as u8,
            y: rem_euclid(self.y, CHUNK_SIZE) as u8,
            z: rem_euclid(self.z, CHUNK_SIZE) as u8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalBlockPos {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl LocalBlockPos {
    pub fn index(self) -> usize {
        (self.x as usize) + (self.z as usize) * CHUNK_SIZE as usize
            + (self.y as usize) * CHUNK_SIZE as usize * CHUNK_SIZE as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextureId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TintKind {
    #[default]
    None = 0,
    Grass = 1,
    Foliage = 2,
}

impl TintKind {
    pub fn as_f32(self) -> f32 {
        self as u8 as f32
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FaceTexture {
    pub texture: TextureId,
    pub overlay: Option<TextureId>,
    pub tint: TintKind,
    pub overlay_tint: TintKind,
}

impl FaceTexture {
    pub fn uniform(texture: TextureId) -> Self {
        Self {
            texture,
            overlay: None,
            tint: TintKind::None,
            overlay_tint: TintKind::None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BlockFaceTextures {
    pub top: FaceTexture,
    pub bottom: FaceTexture,
    pub sides: FaceTexture,
}

impl BlockFaceTextures {
    pub fn uniform(texture: TextureId) -> Self {
        let face = FaceTexture::uniform(texture);
        Self {
            top: face,
            bottom: face,
            sides: face,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDef {
    pub id: BlockId,
    pub namespaced_id: String,
    pub display_name: String,
    pub opaque: bool,
    pub transparent: bool,
    pub solid: bool,
    pub hardness: f32,
    pub face_textures: BlockFaceTextures,
    pub redstone: Option<RedstoneDef>,
    pub placeable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BlockTextures {
    pub top: TextureId,
    pub bottom: TextureId,
    pub sides: TextureId,
}

impl BlockTextures {
    pub fn uniform(id: TextureId) -> Self {
        Self {
            top: id,
            bottom: id,
            sides: id,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RedstoneDef {
    pub emits: u8,
    pub receives: bool,
    pub conducts: bool,
    pub always_on: bool,
    pub invertible: bool,
    pub delay_ticks: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureDef {
    pub id: TextureId,
    pub namespaced_id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub wasm: String,
    pub assets: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModsManifest {
    pub mods: Vec<ModManifest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

fn floor_div(a: i32, b: i32) -> i32 {
    if a >= 0 {
        a / b
    } else {
        (a - (b - 1)) / b
    }
}

fn rem_euclid(a: i32, b: i32) -> i32 {
    ((a % b) + b) % b
}
