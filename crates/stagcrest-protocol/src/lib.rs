pub mod block_model;

use serde::{Deserialize, Serialize};

pub use block_model::{
    BlockGeometry, BlockModel, BoxFace, ModelAxis, ModelElement, ModelFace, ModelId,
    ModelRenderLayer, ModelRotation, ModelVariant,
};

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
pub enum TorchAttachment {
    #[default]
    Floor = 0,
    WallNorth = 1,
    WallSouth = 2,
    WallEast = 3,
    WallWest = 4,
}

impl TorchAttachment {
    pub fn from_bits(bits: u16) -> Self {
        match bits {
            1 => Self::WallNorth,
            2 => Self::WallSouth,
            3 => Self::WallEast,
            4 => Self::WallWest,
            _ => Self::Floor,
        }
    }

    pub fn to_bits(self) -> u16 {
        self as u16
    }

    /// Block-space offset from the torch cell to the supporting solid block.
    pub fn support_offset(self) -> (i32, i32, i32) {
        match self {
            Self::Floor => (0, -1, 0),
            Self::WallNorth => (0, 0, 1),
            Self::WallSouth => (0, 0, -1),
            Self::WallEast => (-1, 0, 0),
            Self::WallWest => (1, 0, 0),
        }
    }

    /// Derive attachment from the hit face normal pointing into the torch cell.
    pub fn from_place_normal(nx: i32, ny: i32, nz: i32) -> Option<Self> {
        match (nx, ny, nz) {
            (0, 1, 0) => Some(Self::Floor),
            (0, -1, 0) => None,
            (0, 0, -1) => Some(Self::WallNorth),
            (0, 0, 1) => Some(Self::WallSouth),
            (1, 0, 0) => Some(Self::WallEast),
            (-1, 0, 0) => Some(Self::WallWest),
            _ => None,
        }
    }
}

pub const TORCH_LIT_BIT: u16 = 1;
pub const TORCH_ATTACHMENT_SHIFT: u16 = 1;
pub const TORCH_ATTACHMENT_MASK: u16 = 0b1110;

pub fn torch_state(lit: bool, attachment: TorchAttachment) -> BlockState {
    let mut bits = attachment.to_bits() << TORCH_ATTACHMENT_SHIFT;
    if lit {
        bits |= TORCH_LIT_BIT;
    }
    BlockState(bits)
}

pub fn torch_lit(state: BlockState) -> bool {
    state.0 & TORCH_LIT_BIT != 0
}

pub fn torch_attachment(state: BlockState) -> TorchAttachment {
    TorchAttachment::from_bits((state.0 & TORCH_ATTACHMENT_MASK) >> TORCH_ATTACHMENT_SHIFT)
}

pub fn set_torch_lit(state: BlockState, lit: bool) -> BlockState {
    if lit {
        BlockState(state.0 | TORCH_LIT_BIT)
    } else {
        BlockState(state.0 & !TORCH_LIT_BIT)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TintKind {
    #[default]
    None = 0,
    Grass = 1,
    Foliage = 2,
    /// Vertex tint is `encode_redstone_power_tint(power)`; shader lerps dark→bright by level.
    RedstonePower = 3,
}

/// Base value for redstone power encoded in the vertex `tint` attribute.
pub const TINT_REDSTONE_POWER_BASE: f32 = 3.0;

/// Encode redstone strength 0–15 into the vertex tint channel.
pub fn encode_redstone_power_tint(power: u8) -> f32 {
    TINT_REDSTONE_POWER_BASE + (power as f32 / 15.0)
}

/// Normalized power 0.0–1.0 from an encoded vertex tint.
pub fn decode_redstone_power_tint(tint: f32) -> f32 {
    (tint - TINT_REDSTONE_POWER_BASE).clamp(0.0, 1.0)
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
    pub geometry: BlockGeometry,
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
