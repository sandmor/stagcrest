use crate::assets::AssetReader;
use crate::block_tints::apply_block_face_tints;
use crate::registry::BlockRegistry;
use crate::resourcepack::{ResourcePackLoader, DEFAULT_MC_BLOCK_TEXTURES};
use crate::runtime::{load_mod, ModLoadContext};
use stagcrest_mod_sdk::{CircuitKindRequest, RegisterBlockRequest};
use stagcrest_protocol::{
    BlockDef, BlockFaceTextures, BlockGeometry, BlockId, CircuitKind, CircuitNodeDef, ModManifest,
    ModsManifest,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ModError {
    #[error("asset error: {0}")]
    Asset(#[from] crate::assets::AssetError),
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("{0}")]
    Message(String),
}

pub struct ModHost {
    pub registry: BlockRegistry,
    pub loaded_mods: Vec<String>,
}

impl ModHost {
    pub fn new() -> Self {
        Self {
            registry: BlockRegistry::new(),
            loaded_mods: Vec::new(),
        }
    }

    pub fn load_all(
        &mut self,
        reader: &dyn AssetReader,
        packs: Option<&ResourcePackLoader>,
    ) -> Result<(), ModError> {
        let content = reader.read_bytes("mods/mods.toml")?;
        let manifest: ModsManifest =
            toml::from_str(std::str::from_utf8(&content).map_err(|e| {
                ModError::Message(format!("mods.toml is not valid UTF-8: {e}"))
            })?)?;

        for mod_entry in manifest.mods {
            self.load_mod(reader, &mod_entry, packs)?;
        }

        Ok(())
    }

    fn load_mod(
        &mut self,
        reader: &dyn AssetReader,
        entry: &ModManifest,
        packs: Option<&ResourcePackLoader>,
    ) -> Result<(), ModError> {
        let wasm_path = format!("mods/{}/{}", entry.id, entry.wasm);
        if !reader.exists(&wasm_path) {
            return Err(ModError::Message(format!(
                "wasm not found for mod {} at {wasm_path}",
                entry.id
            )));
        }

        let wasm_bytes = reader.read_bytes(&wasm_path)?;
        let mut ctx = ModLoadContext {
            registry: &mut self.registry,
            packs,
        };
        load_mod(&mut ctx, &wasm_bytes).map_err(ModError::Runtime)?;

        self.loaded_mods.push(entry.id.clone());
        tracing::info!("loaded wasm mod: {} v{}", entry.name, entry.version);
        Ok(())
    }

    pub fn finalize_atlas(&mut self) -> crate::TextureAtlas {
        let textures: Vec<_> = self.registry.textures().cloned().collect();
        let atlas = crate::TextureAtlas::build(textures.into_iter());
        self.registry
            .set_atlas_dimensions(atlas.width, atlas.height);
        for (id, rect) in &atlas.placements {
            self.registry.set_atlas_uv(*id, *rect);
        }
        atlas
    }

    pub fn air_block(&self) -> BlockId {
        self.registry
            .block_by_name("stagcrest:air")
            .unwrap_or(BlockId(0))
    }
}

impl Default for ModHost {
    fn default() -> Self {
        Self::new()
    }
}

pub fn register_block_host(reg: &mut BlockRegistry, json: RegisterBlockRequest) {
    let mut face_textures = reg
        .resolve_face_textures(
            &json.top_texture,
            &json.bottom_texture,
            &json.sides_texture,
        )
        .unwrap_or(BlockFaceTextures::uniform(stagcrest_protocol::TextureId(0)));

    apply_block_face_tints(&json.namespaced_id, &mut face_textures, reg);

    let id = reg.allocate_block_id();
    let circuit = json.circuit.map(|r| CircuitNodeDef {
        kind: match r.kind {
            CircuitKindRequest::Source { level } => CircuitKind::Source { level },
            CircuitKindRequest::Inverter { output } => CircuitKind::Inverter { output },
            CircuitKindRequest::Wire { falloff } => CircuitKind::Wire { falloff },
            CircuitKindRequest::Switch { output } => CircuitKind::Switch { output },
            CircuitKindRequest::Delay { output, delay } => CircuitKind::Delay { output, delay },
        },
    });

    reg.register_block(BlockDef {
        id,
        namespaced_id: json.namespaced_id,
        display_name: json.display_name,
        opaque: json.opaque,
        transparent: json.transparent,
        solid: json.solid,
        hardness: json.hardness,
        face_textures,
        circuit,
        placeable: json.placeable,
        geometry: json
            .geometry
            .as_deref()
            .or(json.shape.as_deref())
            .map(BlockGeometry::from_str)
            .unwrap_or_default(),
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_mods(repo_root: &std::path::Path) -> Result<ModHost, ModError> {
    let reader = crate::assets::FsAssetReader::new(repo_root);
    let mut packs = ResourcePackLoader::load(&reader).ok();
    if let Some(packs) = packs.as_mut() {
        packs.validate(&reader);
        packs.warm_block_textures(&reader, DEFAULT_MC_BLOCK_TEXTURES);
    }
    let mut host = ModHost::new();
    host.load_all(&reader, packs.as_ref())?;
    Ok(host)
}

#[cfg(target_arch = "wasm32")]
pub async fn load_mods_async() -> Result<ModHost, ModError> {
    let reader = crate::assets::HttpAssetReader::new();
    let mut packs = ResourcePackLoader::load_async(&reader).await.ok();
    if let Some(packs) = packs.as_mut() {
        packs.validate_async(&reader).await;
        packs
            .warm_block_textures_async(&reader, DEFAULT_MC_BLOCK_TEXTURES)
            .await;
    }
    let mut host = ModHost::new();
    host.load_all_async(&reader, packs.as_ref()).await?;
    Ok(host)
}

#[cfg(target_arch = "wasm32")]
impl ModHost {
    pub async fn load_all_async(
        &mut self,
        reader: &crate::assets::HttpAssetReader,
        packs: Option<&ResourcePackLoader>,
    ) -> Result<(), ModError> {
        let content = reader.read_bytes_async("mods/mods.toml").await?;
        let manifest: ModsManifest =
            toml::from_str(std::str::from_utf8(&content).map_err(|e| {
                ModError::Message(format!("mods.toml is not valid UTF-8: {e}"))
            })?)?;

        for mod_entry in manifest.mods {
            self.load_mod_async(reader, &mod_entry, packs).await?;
        }

        Ok(())
    }

    async fn load_mod_async(
        &mut self,
        reader: &crate::assets::HttpAssetReader,
        entry: &ModManifest,
        packs: Option<&ResourcePackLoader>,
    ) -> Result<(), ModError> {
        let wasm_path = format!("mods/{}/{}", entry.id, entry.wasm);
        if !reader.exists_async(&wasm_path).await {
            return Err(ModError::Message(format!(
                "wasm not found for mod {} at {wasm_path}",
                entry.id
            )));
        }

        let wasm_bytes = reader.read_bytes_async(&wasm_path).await?;
        let mut ctx = ModLoadContext {
            registry: &mut self.registry,
            packs,
        };
        load_mod(&mut ctx, &wasm_bytes).map_err(ModError::Runtime)?;

        self.loaded_mods.push(entry.id.clone());
        tracing::info!("loaded wasm mod: {} v{}", entry.name, entry.version);
        Ok(())
    }
}

// Kept for API compatibility with loading code that references repo paths on native.
