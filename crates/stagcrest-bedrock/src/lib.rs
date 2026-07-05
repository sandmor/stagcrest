//! Minecraft Bedrock model + animation loading for Stagcrest entities.
//!
//! This crate parses Bedrock `*.geo.json` geometry and `*.animation.json`
//! clips, bakes geometry into per-bone rigid meshes, and evaluates a small
//! subset of Molang. It is a pure-data crate (no engine dependencies) so it can
//! be shared by the client renderer and tests.

pub mod animation;
pub mod bake;
pub mod geometry;
pub mod molang;

pub use animation::{AnimationClip, AnimationSet, BonePose};
pub use bake::{BakedBone, BakedModel, EntityVertex, MODEL_PIXELS_PER_BLOCK};
pub use geometry::Geometry;
pub use molang::Expr;

use glam::{EulerRot, Quat};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BedrockError {
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
}

/// Convert a Bedrock bone rotation (degrees, X/Y/Z) into a quaternion in the
/// engine's right-handed Y-up space.
///
/// Bedrock stores bone rotations in a left-handed convention; negating X and Y
/// while keeping Z maps them into glam's right-handed space applied in Z, Y, X
/// intrinsic order.
pub fn bedrock_rotation_to_quat(deg: [f32; 3]) -> Quat {
    let x = deg[0].to_radians();
    let y = deg[1].to_radians();
    let z = deg[2].to_radians();
    Quat::from_euler(EulerRot::ZYX, z, -y, -x)
}
