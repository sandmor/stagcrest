use serde::{Deserialize, Serialize};

/// Built-in block model identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelId {
    RedstoneTorch,
    Lever,
    Button,
    Repeater,
}

/// Variant index for model lookup (e.g. torch attachment as `TorchAttachment::to_bits()`).
pub type ModelVariant = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BlockGeometry {
    #[default]
    Cube,
    Flat,
    Cross,
    Model(ModelId),
}

impl BlockGeometry {
    pub fn from_str(s: &str) -> Self {
        match s {
            "flat" => Self::Flat,
            "cross" => Self::Cross,
            "torch" | "model:redstone_torch" => Self::Model(ModelId::RedstoneTorch),
            "model:lever" => Self::Model(ModelId::Lever),
            "model:button" | "model:stone_button" => Self::Model(ModelId::Button),
            "model:repeater" => Self::Model(ModelId::Repeater),
            _ => Self::Cube,
        }
    }
}

/// Which of a block's three texture slots a model element samples from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ModelTexture {
    Top,
    Bottom,
    #[default]
    Sides,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ModelRenderLayer {
    #[default]
    Opaque,
    Blend,
    Cutout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelRotation {
    pub origin: [f32; 3],
    pub axis: ModelAxis,
    /// Degrees.
    pub angle: f32,
    pub rescale: bool,
}

/// Face UV rectangle in 0–16 Minecraft pixel space.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelFace {
    pub uv: [f32; 4],
}

impl ModelFace {
    pub const FULL: Self = Self {
        uv: [0.0, 0.0, 16.0, 16.0],
    };

    pub const fn new(u0: f32, v0: f32, u1: f32, v1: f32) -> Self {
        Self {
            uv: [u0, v0, u1, v1],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BoxFace {
    Down = 0,
    Up = 1,
    North = 2,
    South = 3,
    West = 4,
    East = 5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelElement {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub rotation: Option<ModelRotation>,
    pub faces: [Option<ModelFace>; 6],
    /// Which block texture slot this element's faces sample from.
    pub texture: ModelTexture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockModel {
    pub layer: ModelRenderLayer,
    pub elements: Vec<ModelElement>,
    /// Whole-model orientation in degrees, applied about the block center
    /// after each element's own rotation, in X then Y then Z order. Mirrors
    /// how Minecraft blockstates rotate a shared model to face/attach in
    /// different directions (`[0, yaw, 0]` is a plain facing rotation).
    pub rotation: [f32; 3],
}

impl ModelElement {
    pub fn all_faces(face: ModelFace) -> [Option<ModelFace>; 6] {
        [
            Some(face),
            Some(face),
            Some(face),
            Some(face),
            Some(face),
            Some(face),
        ]
    }
}
