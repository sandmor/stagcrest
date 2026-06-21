use serde::{Deserialize, Serialize};

/// Built-in block model identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelId {
    RedstoneTorch,
}

/// Variant index for model lookup (e.g. torch attachment as `TorchAttachment::to_bits()`).
pub type ModelVariant = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BlockGeometry {
    #[default]
    Cube,
    Flat,
    Model(ModelId),
}

impl BlockGeometry {
    pub fn from_str(s: &str) -> Self {
        match s {
            "flat" => Self::Flat,
            "torch" | "model:redstone_torch" => Self::Model(ModelId::RedstoneTorch),
            _ => Self::Cube,
        }
    }
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockModel {
    pub layer: ModelRenderLayer,
    pub elements: Vec<ModelElement>,
    /// Whole-model rotation about the vertical block-center axis, in degrees
    /// (applied after each element's own rotation). Mirrors how Minecraft
    /// blockstates rotate a shared model to face different directions.
    pub y_rotation: f32,
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
