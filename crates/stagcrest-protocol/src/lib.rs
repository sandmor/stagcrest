pub mod block_model;

use serde::{Deserialize, Serialize};

pub use block_model::{
    BlockGeometry, BlockModel, BoxFace, ModelAxis, ModelElement, ModelFace, ModelId,
    ModelRenderLayer, ModelRotation, ModelTexture, ModelVariant,
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
        (self.x as usize)
            + (self.z as usize) * CHUNK_SIZE as usize
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

/// Surface a face-mounted block (lever, button) is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AttachFace {
    #[default]
    Floor = 0,
    Ceiling = 1,
    Wall = 2,
}

/// Cardinal facing of a face-mounted block. For wall mounts this is the
/// direction the block points away from its supporting wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Facing {
    #[default]
    North = 0,
    South = 1,
    East = 2,
    West = 3,
}

impl AttachFace {
    pub fn from_bits(bits: u16) -> Self {
        match bits {
            1 => Self::Ceiling,
            2 => Self::Wall,
            _ => Self::Floor,
        }
    }
    pub fn to_bits(self) -> u16 {
        self as u16
    }
}

impl Facing {
    pub fn from_bits(bits: u16) -> Self {
        match bits {
            1 => Self::South,
            2 => Self::East,
            3 => Self::West,
            _ => Self::North,
        }
    }
    pub fn to_bits(self) -> u16 {
        self as u16
    }

    /// Cardinal facing from a horizontal look direction (e.g. camera forward).
    pub fn from_horizontal(fx: f32, fz: f32) -> Self {
        if fx.abs() >= fz.abs() {
            if fx >= 0.0 {
                Self::East
            } else {
                Self::West
            }
        } else if fz >= 0.0 {
            Self::South
        } else {
            Self::North
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::South => Self::North,
            Self::East => Self::West,
            Self::West => Self::East,
        }
    }
}

// State bit layout for mountable blocks (lever, button):
//   bit 0      -> on/powered (shared with the circuit `Switch` node)
//   bits 1-2   -> AttachFace
//   bits 3-4   -> Facing
pub const MOUNT_ON_BIT: u16 = 1;
pub const MOUNT_FACE_SHIFT: u16 = 1;
pub const MOUNT_FACE_MASK: u16 = 0b110;
pub const MOUNT_FACING_SHIFT: u16 = 3;
pub const MOUNT_FACING_MASK: u16 = 0b11000;

pub fn mount_state(on: bool, face: AttachFace, facing: Facing) -> BlockState {
    let mut bits = face.to_bits() << MOUNT_FACE_SHIFT;
    bits |= facing.to_bits() << MOUNT_FACING_SHIFT;
    if on {
        bits |= MOUNT_ON_BIT;
    }
    BlockState(bits)
}

pub fn mount_on(state: BlockState) -> bool {
    state.0 & MOUNT_ON_BIT != 0
}

pub fn mount_face(state: BlockState) -> AttachFace {
    AttachFace::from_bits((state.0 & MOUNT_FACE_MASK) >> MOUNT_FACE_SHIFT)
}

pub fn mount_facing(state: BlockState) -> Facing {
    Facing::from_bits((state.0 & MOUNT_FACING_MASK) >> MOUNT_FACING_SHIFT)
}

/// Model variant index encoding (on, face, facing) for model lookup.
pub fn mount_variant(state: BlockState) -> ModelVariant {
    let on = (state.0 & MOUNT_ON_BIT) as u8;
    let face = ((state.0 & MOUNT_FACE_MASK) >> MOUNT_FACE_SHIFT) as u8;
    let facing = ((state.0 & MOUNT_FACING_MASK) >> MOUNT_FACING_SHIFT) as u8;
    on | (face << 1) | (facing << 3)
}

/// Block-space offset from a mounted block to its supporting solid block.
pub fn mount_support_offset(face: AttachFace, facing: Facing) -> (i32, i32, i32) {
    match face {
        AttachFace::Floor => (0, -1, 0),
        AttachFace::Ceiling => (0, 1, 0),
        AttachFace::Wall => match facing {
            // Wall mounts point away from their support, so the support sits
            // on the opposite side of the facing direction.
            Facing::North => (0, 0, 1),
            Facing::South => (0, 0, -1),
            Facing::East => (-1, 0, 0),
            Facing::West => (1, 0, 0),
        },
    }
}

/// Derive (face, facing) for a mountable block from the clicked face normal
/// (pointing out of the support into the placed cell) and a horizontal look
/// direction used to orient floor/ceiling mounts.
pub fn mount_from_placement(
    nx: i32,
    ny: i32,
    nz: i32,
    look_x: f32,
    look_z: f32,
) -> Option<(AttachFace, Facing)> {
    match (nx, ny, nz) {
        (0, 1, 0) => Some((AttachFace::Floor, Facing::from_horizontal(look_x, look_z))),
        (0, -1, 0) => Some((AttachFace::Ceiling, Facing::from_horizontal(look_x, look_z))),
        (1, 0, 0) => Some((AttachFace::Wall, Facing::East)),
        (-1, 0, 0) => Some((AttachFace::Wall, Facing::West)),
        (0, 0, 1) => Some((AttachFace::Wall, Facing::South)),
        (0, 0, -1) => Some((AttachFace::Wall, Facing::North)),
        _ => None,
    }
}

// Repeater state (floor-only redstone component):
//   bit 0      -> powered (lit torches; top keeps `repeater`, not `repeater_on`)
//   bits 1-2   -> Facing (output direction)
//   bits 3-4   -> delay index 0..3 (= 1..4 ticks)
pub const REPEATER_POWERED_BIT: u16 = 1;
pub const REPEATER_FACING_SHIFT: u16 = 1;
pub const REPEATER_FACING_MASK: u16 = 0b110;
pub const REPEATER_DELAY_SHIFT: u16 = 3;
pub const REPEATER_DELAY_MASK: u16 = 0b11000;

pub fn repeater_state(powered: bool, facing: Facing, delay_ticks: u8) -> BlockState {
    let delay = (delay_ticks.clamp(1, 4) - 1) as u16;
    let mut bits = facing.to_bits() << REPEATER_FACING_SHIFT;
    bits |= delay << REPEATER_DELAY_SHIFT;
    if powered {
        bits |= REPEATER_POWERED_BIT;
    }
    BlockState(bits)
}

pub fn repeater_powered(state: BlockState) -> bool {
    state.0 & REPEATER_POWERED_BIT != 0
}

pub fn repeater_facing(state: BlockState) -> Facing {
    Facing::from_bits((state.0 & REPEATER_FACING_MASK) >> REPEATER_FACING_SHIFT)
}

pub fn repeater_delay_ticks(state: BlockState) -> u8 {
    let idx = (state.0 & REPEATER_DELAY_MASK) >> REPEATER_DELAY_SHIFT;
    (idx + 1) as u8
}

/// Model variant index: `(delay << 3) | (powered << 2) | facing`.
pub fn repeater_variant(state: BlockState) -> ModelVariant {
    let powered = ((state.0 & REPEATER_POWERED_BIT) != 0) as u8;
    let facing = ((state.0 & REPEATER_FACING_MASK) >> REPEATER_FACING_SHIFT) as u8;
    let delay = ((state.0 & REPEATER_DELAY_MASK) >> REPEATER_DELAY_SHIFT) as u8;
    (delay << 3) | (powered << 2) | facing
}

pub fn repeater_facing_yaw(facing: Facing) -> f32 {
    // Vanilla `repeater_*tick` model arrow points toward -Z at yaw 0; these
    // rotations map stored output `facing` onto world space (MC blockstates).
    match facing {
        Facing::South => 180.0,
        Facing::East => 270.0,
        Facing::North => 0.0,
        Facing::West => 90.0,
    }
}

/// Horizontal unit step toward the repeater model's output face (arrow tip).
pub fn facing_delta(facing: Facing) -> (i32, i32, i32) {
    match facing {
        Facing::North => (0, 0, -1),
        Facing::South => (0, 0, 1),
        Facing::East => (1, 0, 0),
        Facing::West => (-1, 0, 0),
    }
}

/// Whether dust at `(toward_dx, toward_dz)` relative to a repeater may connect.
pub fn repeater_connects_toward(facing: Facing, toward_dx: i32, toward_dz: i32) -> bool {
    let (fx, _, fz) = facing_delta(facing);
    (toward_dx == fx && toward_dz == fz) || (toward_dx == -fx && toward_dz == -fz)
}

/// Cycle delay 1 → 2 → 3 → 4 → 1; preserves powered and facing bits.
pub fn cycle_repeater_delay(state: BlockState) -> BlockState {
    let next = repeater_delay_ticks(state) % 4 + 1;
    let delay_bits = (next - 1) as u16;
    BlockState((state.0 & !REPEATER_DELAY_MASK) | (delay_bits << REPEATER_DELAY_SHIFT))
}

/// Fluid block state bits (MC-style; flow simulation not yet implemented).
pub const FLUID_FLOWING_BIT: u16 = 1 << 8;
pub const FLUID_LEVEL_MASK: u16 = 0xF << 9;
pub const FLUID_LEVEL_SHIFT: u16 = 9;

pub fn fluid_flowing(state: BlockState) -> bool {
    state.0 & FLUID_FLOWING_BIT != 0
}

pub fn fluid_level(state: BlockState) -> u8 {
    ((state.0 & FLUID_LEVEL_MASK) >> FLUID_LEVEL_SHIFT) as u8
}

pub fn still_water_state() -> BlockState {
    BlockState(0)
}

pub fn flowing_water_state(level: u8) -> BlockState {
    let level = level.min(15);
    BlockState(FLUID_FLOWING_BIT | ((level as u16) << FLUID_LEVEL_SHIFT))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TintKind {
    #[default]
    None = 0,
    Grass = 1,
    Foliage = 2,
    /// Vertex tint is `encode_power_tint(power)`; shader lerps dark→bright by level.
    PowerLevel = 3,
    /// Biome water colormap tint (shader multiplies greyscale fluid texture).
    Water = 4,
}

/// Vertex tint for [`TintKind::Water`]. Distinct from max power (`TINT_POWER_BASE + 1.0` = 4.0).
pub const TINT_WATER: f32 = 4.5;

/// Base value for signal power encoded in the vertex `tint` attribute.
pub const TINT_POWER_BASE: f32 = 3.0;

/// Encode signal strength 0–15 into the vertex tint channel.
pub fn encode_power_tint(power: u8) -> f32 {
    TINT_POWER_BASE + (power as f32 / 15.0)
}

/// Normalized power 0.0–1.0 from an encoded vertex tint.
pub fn decode_power_tint(tint: f32) -> f32 {
    (tint - TINT_POWER_BASE).clamp(0.0, 1.0)
}

impl TintKind {
    pub fn as_f32(self) -> f32 {
        match self {
            TintKind::Water => TINT_WATER,
            _ => self as u8 as f32,
        }
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
    pub circuit: Option<CircuitNodeDef>,
    pub placeable: bool,
    pub geometry: BlockGeometry,
    pub fluid: bool,
    pub render_layer: ModelRenderLayer,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CircuitKind {
    Source { level: u8 },
    Inverter { output: u8 },
    Wire { falloff: u8 },
    Switch { output: u8 },
    Delay { output: u8, delay: u8 },
    /// Directional delay; tick count lives in block state (`repeater_delay_ticks`).
    Repeater { output: u8 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CircuitNodeDef {
    pub kind: CircuitKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureAnimation {
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_count: u32,
    pub frametime_ticks: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureDef {
    pub id: TextureId,
    pub namespaced_id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    #[serde(default)]
    pub animation: Option<TextureAnimation>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_tint_above_max_power_tint() {
        assert!(
            TINT_WATER > encode_power_tint(15),
            "water tint must not collide with max redstone power tint"
        );
    }
}
