//! Server-side registry of Bedrock entity types registered by mods.

use stagcrest_protocol::{
    EntityAssetTransfer, EntityAssetsChunk, EntityManifest, EntityTypeId, EntityWireDef,
};

/// Full server-side entity type definition, including raw Bedrock asset bytes
/// and spawn tuning.
#[derive(Debug, Clone)]
pub struct EntityServerDef {
    pub type_id: EntityTypeId,
    pub namespaced_id: String,
    pub archetype: String,
    pub texture_width: u32,
    pub texture_height: u32,
    pub scale: f32,
    pub idle_animation: String,
    pub walk_animation: Option<String>,
    pub geometry_json: Vec<u8>,
    pub texture_png: Vec<u8>,
    pub animation_json: Option<Vec<u8>>,
    /// Per-surface-chunk probability that this entity spawns at all.
    pub spawn_per_chunk_chance: f32,
    /// Max instances placed in a single chunk when it does spawn.
    pub spawn_max_per_chunk: u8,
}

/// Number of entity asset transfers per handshake chunk.
const ENTITIES_PER_CHUNK: usize = 4;

#[derive(Debug, Default)]
pub struct EntityRegistry {
    entities: Vec<EntityServerDef>,
    next_id: u32,
}

impl EntityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allocate_type_id(&mut self) -> EntityTypeId {
        let id = EntityTypeId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn register(&mut self, def: EntityServerDef) {
        self.entities.push(def);
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn all(&self) -> impl Iterator<Item = &EntityServerDef> {
        self.entities.iter()
    }

    pub fn get(&self, type_id: EntityTypeId) -> Option<&EntityServerDef> {
        self.entities.iter().find(|e| e.type_id == type_id)
    }

    /// Client-facing metadata (no asset bytes).
    pub fn to_manifest(&self) -> EntityManifest {
        EntityManifest {
            entities: self
                .entities
                .iter()
                .map(|e| EntityWireDef {
                    type_id: e.type_id,
                    namespaced_id: e.namespaced_id.clone(),
                    archetype: e.archetype.clone(),
                    texture_width: e.texture_width,
                    texture_height: e.texture_height,
                    scale: e.scale,
                    idle_animation: e.idle_animation.clone(),
                    walk_animation: e.walk_animation.clone(),
                })
                .collect(),
        }
    }

    /// Bulk asset transfers, chunked for the handshake bulk channel.
    pub fn build_asset_chunks(&self) -> Vec<EntityAssetsChunk> {
        let transfers: Vec<EntityAssetTransfer> = self
            .entities
            .iter()
            .map(|e| EntityAssetTransfer {
                type_id: e.type_id,
                geometry_json: e.geometry_json.clone(),
                texture_png: e.texture_png.clone(),
                animation_json: e.animation_json.clone(),
            })
            .collect();
        if transfers.is_empty() {
            return Vec::new();
        }
        let chunks: Vec<_> = transfers
            .chunks(ENTITIES_PER_CHUNK)
            .map(|c| c.to_vec())
            .collect();
        let total = chunks.len() as u32;
        chunks
            .into_iter()
            .enumerate()
            .map(|(index, entities)| EntityAssetsChunk {
                index: index as u32,
                total,
                entities,
            })
            .collect()
    }
}
