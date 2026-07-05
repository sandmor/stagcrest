//! Shared entity wire types (Bedrock-compatible entity content + replication).

use serde::{Deserialize, Serialize};

/// Numeric entity *type* id assigned at mod registration time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct EntityTypeId(pub u32);

/// Unique id of a live entity *instance* in the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct EntityId(pub u64);

/// Client-facing description of an entity type (no asset bytes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityWireDef {
    pub type_id: EntityTypeId,
    pub namespaced_id: String,
    /// Archetype hint (e.g. `"humanoid"`); reserved for archetype-specific logic.
    pub archetype: String,
    pub texture_width: u32,
    pub texture_height: u32,
    /// Uniform model scale multiplier.
    pub scale: f32,
    /// Animation identifier played when idle (e.g. `animation.humanoid.idle`).
    pub idle_animation: String,
    /// Optional walk animation identifier.
    #[serde(default)]
    pub walk_animation: Option<String>,
}

/// Raw Bedrock asset bytes for one entity type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityAssetTransfer {
    pub type_id: EntityTypeId,
    /// `*.geo.json` geometry bytes.
    pub geometry_json: Vec<u8>,
    /// Entity texture PNG bytes.
    pub texture_png: Vec<u8>,
    /// Optional `*.animation.json` bytes.
    #[serde(default)]
    pub animation_json: Option<Vec<u8>>,
}

/// Chunk of entity asset transfers (bulk channel) during handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityAssetsChunk {
    pub index: u32,
    pub total: u32,
    pub entities: Vec<EntityAssetTransfer>,
}

/// Entity type metadata sent once after the block manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityManifest {
    pub entities: Vec<EntityWireDef>,
}
