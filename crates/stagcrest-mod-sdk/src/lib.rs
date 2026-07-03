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
    #[serde(default)]
    pub render_layer: Option<RenderLayer>,
    #[serde(default)]
    pub geometry: Option<String>,
    pub circuit: Option<RegisterCircuitRequest>,
    #[serde(default)]
    pub push_reaction: Option<PushReaction>,
    pub map_color: [u8; 3],
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
    Repeater { output: u8 },
    Observer { output: u8 },
    Piston { sticky: bool },
}

/// How a block responds when pushed by a piston.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum PushReaction {
    #[default]
    Normal,
    Block,
    Destroy,
}

#[derive(Serialize, Deserialize)]
pub struct RegisterTextureRequest {
    pub namespaced_id: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Inclusive placement range on a noise axis (vanilla-style multi-noise).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NoiseRange {
    pub min: f32,
    pub max: f32,
}

impl NoiseRange {
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    pub const fn point(v: f32) -> Self {
        Self { min: v, max: v }
    }
}

/// Which world layer a biome applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BiomeDimension {
    #[default]
    Surface,
    Underground,
    SkyIsland,
}

/// Per-biome atmospheric and tint parameters interpolated across borders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BiomeEnvironment {
    pub fog_color: [u8; 3],
    pub fog_density: f32,
    pub water_color: [u8; 3],
    pub water_fog_color: [u8; 3],
    pub sky_color: [u8; 3],
    #[serde(default)]
    pub grass_color: Option<[u8; 3]>,
    #[serde(default)]
    pub foliage_color: Option<[u8; 3]>,
}

impl BiomeEnvironment {
    pub fn plains_default() -> Self {
        Self {
            fog_color: [192, 216, 255],
            fog_density: 0.01,
            water_color: [63, 118, 228],
            water_fog_color: [5, 44, 82],
            sky_color: [120, 167, 255],
            grass_color: None,
            foliage_color: None,
        }
    }
}

/// Declarative biome definition registered from WASM mods.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterBiomeRequest {
    pub namespaced_id: String,
    #[serde(default)]
    pub dimension: BiomeDimension,
    pub temperature: NoiseRange,
    pub humidity: NoiseRange,
    pub continentalness: NoiseRange,
    pub erosion: NoiseRange,
    pub depth: NoiseRange,
    pub weirdness: NoiseRange,
    #[serde(default)]
    pub offset: f32,
    pub surface_top: String,
    pub surface_under: String,
    pub surface_depth: u8,
    #[serde(default)]
    pub underwater_top: Option<String>,
    #[serde(default)]
    pub underwater_under: Option<String>,
    pub environment: BiomeEnvironment,
}

/// Tree canopy shapes implemented by the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeShape {
    Oak,
    Birch,
    Spruce,
    Pine,
    Jungle,
    Acacia,
    DarkOak,
    Mangrove,
    Cherry,
    Azalea,
    Bamboo,
}

/// Parametric feature placement — engine implements primitives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FeaturePlacement {
    Plant {
        block: String,
        tall: bool,
    },
    Patch {
        block: String,
        radius: u8,
        density: f32,
    },
    Tree {
        trunk: String,
        leaves: String,
        shape: TreeShape,
        height: u8,
    },
    Boulder {
        block: String,
    },
    Column {
        block: String,
        height: u8,
    },
    IceSpike,
    Stalagmite {
        block: String,
    },
    Stalactite {
        block: String,
    },
    SurfacePatch {
        block: String,
    },
    GlowFlora {
        block: String,
    },
    /// Optional lip block at waterfall top (river features only).
    WaterfallSheet {
        lip_block: Option<String>,
    },
    /// Pool decor at waterfall base (river features only).
    WaterfallPool {
        block: String,
        radius: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HydrologyMode {
    Terrace,
    DrainageGrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiverFeatureSlot {
    WaterfallLip,
    WaterfallBase,
    PoolSurface,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterRiverFeatureRequest {
    pub slot: RiverFeatureSlot,
    pub placement: FeaturePlacement,
    pub chance: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterFeatureRequest {
    pub biome_id: String,
    pub placement: FeaturePlacement,
    pub chance: f32,
}

/// Global river hydrology configuration (one per mod pack).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterRiverConfigRequest {
    /// Full river channel width in blocks (centerline to bank edge, both sides).
    pub width: f32,
    pub bank_blocks: Vec<String>,
    pub river_biome_id: String,
    pub frozen_river_biome_id: String,
    pub riverbank_biome_id: String,
    #[serde(default = "default_hydrology_mode")]
    pub hydrology_mode: HydrologyMode,
    #[serde(default = "default_terrace_step")]
    pub terrace_step: i32,
    #[serde(default)]
    pub terrace_offset: i32,
    #[serde(default = "default_drainage_cell_size")]
    pub drainage_cell_size: i32,
    #[serde(default = "default_drainage_relax_passes")]
    pub drainage_relax_passes: u32,
    #[serde(default = "default_waterfall_min_drop")]
    pub waterfall_min_drop: i32,
    #[serde(default = "default_channel_depth")]
    pub channel_depth: i32,
    #[serde(default = "default_max_channel_carve")]
    pub max_channel_carve: i32,
    #[serde(default = "default_mouth_sea_margin")]
    pub mouth_sea_margin: i32,
}

fn default_hydrology_mode() -> HydrologyMode {
    HydrologyMode::DrainageGrid
}
fn default_terrace_step() -> i32 {
    6
}
fn default_drainage_cell_size() -> i32 {
    64
}
fn default_drainage_relax_passes() -> u32 {
    12
}
fn default_waterfall_min_drop() -> i32 {
    4
}
fn default_channel_depth() -> i32 {
    10
}
fn default_max_channel_carve() -> i32 {
    12
}
fn default_mouth_sea_margin() -> i32 {
    2
}

/// Global cave carver tuning.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterCaveConfigRequest {
    pub cheese_threshold: f64,
    pub spaghetti_threshold: f64,
    pub noodle_threshold: f64,
    pub lush_cave_biome_id: String,
    pub dripstone_cave_biome_id: String,
    pub deep_dark_biome_id: String,
}

// --- Legacy compat (deprecated) ---

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
pub struct RegisterBiomeFeatureRequest {
    pub biome_id: String,
    pub feature_kind: FeatureKind,
    pub chance: f32,
}

/// Implemented by the engine host (native) or host imports (wasm mod).
pub trait ContentRegistrar {
    fn register_texture(&mut self, req: RegisterTextureRequest) -> i32;
    fn register_texture_from_pack(&mut self, namespaced_id: &str, mc_name: &str) -> i32 {
        let _ = (namespaced_id, mc_name);
        0
    }
    fn register_block(&mut self, req: RegisterBlockRequest) -> i32;
    fn register_biome(&mut self, req: RegisterBiomeRequest) -> i32 {
        let _ = req;
        0
    }
    fn register_feature(&mut self, req: RegisterFeatureRequest) -> i32 {
        let _ = req;
        0
    }
    fn register_river_config(&mut self, req: RegisterRiverConfigRequest) -> i32 {
        let _ = req;
        0
    }
    fn register_river_feature(&mut self, req: RegisterRiverFeatureRequest) -> i32 {
        let _ = req;
        0
    }
    fn register_cave_config(&mut self, req: RegisterCaveConfigRequest) -> i32 {
        let _ = req;
        0
    }
    /// Deprecated — use `register_feature`.
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
    log, register_biome, register_biome_feature, register_block, register_cave_config,
    register_feature, register_river_config, register_river_feature, register_texture,
    register_texture_from_pack,
};
