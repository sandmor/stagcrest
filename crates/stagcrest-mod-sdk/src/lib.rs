use serde::{Deserialize, Serialize};

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
    pub geometry: Option<String>,
    /// Deprecated: use `geometry` instead.
    #[serde(default)]
    pub shape: Option<String>,
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

/// Implemented by the engine host (native) or host imports (wasm mod).
pub trait ContentRegistrar {
    fn register_texture(&mut self, req: RegisterTextureRequest) -> i32;
    fn register_block(&mut self, req: RegisterBlockRequest) -> i32;
    fn log(&self, msg: &str);
}

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
pub use wasm::{load_texture_from_pack, log, register_block, register_texture};
