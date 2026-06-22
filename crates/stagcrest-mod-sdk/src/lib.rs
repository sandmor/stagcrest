use serde::{Deserialize, Serialize};

/// How a cube block's faces are drawn (opaque, alpha blend, or alpha cutout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RenderLayer {
    #[default]
    Opaque,
    Blend,
    Cutout,
}

#[derive(Serialize, Deserialize)]
pub struct RegisterBlockRequest {
    pub namespaced_id: String,
    pub display_name: String,
    pub opaque: bool,
    pub transparent: bool,
    pub solid: bool,
    pub hardness: f32,
    pub top_texture: String,
    pub bottom_texture: String,
    pub sides_texture: String,
    pub placeable: bool,
    #[serde(default)]
    pub fluid: bool,
    /// When omitted, the host uses cutout for transparent blocks and opaque otherwise.
    #[serde(default)]
    pub render_layer: Option<RenderLayer>,
    #[serde(default)]
    pub geometry: Option<String>,
    pub circuit: Option<RegisterCircuitRequest>,
}

#[derive(Serialize, Deserialize)]
pub struct RegisterCircuitRequest {
    pub kind: CircuitKindRequest,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CircuitKindRequest {
    Source { level: u8 },
    Inverter { output: u8 },
    Wire { falloff: u8 },
    Switch { output: u8 },
    Delay { output: u8, delay: u8 },
    Repeater { output: u8 },
}

#[derive(Serialize, Deserialize)]
pub struct RegisterTextureRequest {
    pub namespaced_id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureKind {
    ShortGrass,
    TallGrass,
    Dandelion,
    Poppy,
    Cactus,
    DeadBush,
    OakTree,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterBiomeRequest {
    pub namespaced_id: String,
    pub temperature: f32,
    pub downfall: f32,
    pub surface_top: String,
    pub surface_under: String,
    pub surface_depth: u8,
    #[serde(default)]
    pub underwater_top: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterBiomeFeatureRequest {
    pub biome_id: String,
    pub feature_kind: FeatureKind,
    pub chance: f32,
}

/// Implemented by the engine host (native) or host imports (wasm mod).
pub trait ContentRegistrar {
    fn register_texture(&mut self, req: RegisterTextureRequest) -> i32;
    fn register_block(&mut self, req: RegisterBlockRequest) -> i32;
    fn register_biome(&mut self, req: RegisterBiomeRequest) -> i32 {
        let _ = req;
        0
    }
    fn register_biome_feature(&mut self, req: RegisterBiomeFeatureRequest) -> i32 {
        let _ = req;
        0
    }
    fn log(&self, msg: &str);
}

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
pub use wasm::{
    load_texture_from_pack, log, register_biome, register_biome_feature, register_block,
    register_texture,
};
