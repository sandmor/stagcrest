use crate::assets::AssetReader;
use crate::block_tints::apply_block_face_tints;
use crate::registry::BlockRegistry;
use crate::resourcepack::{ResourcePackLoader, DEFAULT_MC_BLOCK_TEXTURES};
use crate::runtime::{load_mod, ModLoadContext};
use crate::worldgen::BiomeRegistry;
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
    pub biome_registry: BiomeRegistry,
    pub loaded_mods: Vec<String>,
}

impl ModHost {
    pub fn new() -> Self {
        Self {
            registry: BlockRegistry::new(),
            biome_registry: BiomeRegistry::default(),
            loaded_mods: Vec::new(),
        }
    }

    pub fn finalize_biomes(&mut self) -> Result<(), ModError> {
        self.biome_registry
            .finalize(&self.registry)
            .map_err(ModError::Message)
    }

    pub fn load_all(
        &mut self,
        reader: &dyn AssetReader,
        packs: Option<&ResourcePackLoader>,
    ) -> Result<(), ModError> {
        let content = reader.read_bytes("mods/mods.toml")?;
        let manifest: ModsManifest = toml::from_str(
            std::str::from_utf8(&content)
                .map_err(|e| ModError::Message(format!("mods.toml is not valid UTF-8: {e}")))?,
        )?;

        for mod_entry in manifest.mods {
            self.load_mod(reader, &mod_entry, packs)?;
        }

        self.finalize_biomes()?;

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
            biome_registry: &mut self.biome_registry,
            packs,
        };
        load_mod(&mut ctx, &wasm_bytes).map_err(ModError::Runtime)?;

        self.loaded_mods.push(entry.id.clone());
        tracing::info!("loaded wasm mod: {} v{}", entry.name, entry.version);
        Ok(())
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
        .resolve_face_textures(&json.top_texture, &json.bottom_texture, &json.sides_texture)
        .unwrap_or(BlockFaceTextures::uniform(stagcrest_protocol::TextureId(0)));

    apply_block_face_tints(&json.namespaced_id, json.fluid, &mut face_textures, reg);

    let id = reg.allocate_block_id();
    let circuit = json.circuit.map(|r| CircuitNodeDef {
        kind: match r.kind {
            CircuitKindRequest::Source { level } => CircuitKind::Source { level },
            CircuitKindRequest::Inverter { output } => CircuitKind::Inverter { output },
            CircuitKindRequest::Wire { falloff } => CircuitKind::Wire { falloff },
            CircuitKindRequest::Switch { output } => CircuitKind::Switch { output },
            CircuitKindRequest::Repeater { output } => CircuitKind::Repeater { output },
            CircuitKindRequest::Observer { output } => CircuitKind::Observer { output },
            CircuitKindRequest::Piston { sticky } => CircuitKind::Piston { sticky },
        },
    });

    let push_reaction = json
        .push_reaction
        .map(push_reaction_from_sdk)
        .unwrap_or(stagcrest_protocol::PushReaction::Normal);

    let render_layer = json
        .render_layer
        .map(render_layer_from_sdk)
        .unwrap_or_else(|| resolve_render_layer(json.transparent));

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
        fluid: json.fluid,
        geometry: json
            .geometry
            .as_deref()
            .map(BlockGeometry::from_str)
            .unwrap_or_default(),
        render_layer,
        push_reaction,
        map_color: json.map_color,
    });
}

fn push_reaction_from_sdk(
    reaction: stagcrest_mod_sdk::PushReaction,
) -> stagcrest_protocol::PushReaction {
    match reaction {
        stagcrest_mod_sdk::PushReaction::Normal => stagcrest_protocol::PushReaction::Normal,
        stagcrest_mod_sdk::PushReaction::Block => stagcrest_protocol::PushReaction::Block,
        stagcrest_mod_sdk::PushReaction::Destroy => stagcrest_protocol::PushReaction::Destroy,
    }
}

fn resolve_render_layer(transparent: bool) -> stagcrest_protocol::ModelRenderLayer {
    if transparent {
        stagcrest_protocol::ModelRenderLayer::Cutout
    } else {
        stagcrest_protocol::ModelRenderLayer::Opaque
    }
}

fn render_layer_from_sdk(
    layer: stagcrest_mod_sdk::RenderLayer,
) -> stagcrest_protocol::ModelRenderLayer {
    match layer {
        stagcrest_mod_sdk::RenderLayer::Opaque => stagcrest_protocol::ModelRenderLayer::Opaque,
        stagcrest_mod_sdk::RenderLayer::Blend => stagcrest_protocol::ModelRenderLayer::Blend,
        stagcrest_mod_sdk::RenderLayer::Cutout => stagcrest_protocol::ModelRenderLayer::Cutout,
    }
}

pub fn load_mods(repo_root: &std::path::Path) -> Result<ModHost, ModError> {
    let reader = crate::assets::FsAssetReader::new(repo_root);
    let packs = ResourcePackLoader::load(repo_root, &reader).ok();
    if let Some(packs) = packs.as_ref() {
        packs.validate(&reader);
        packs.warm_block_textures(&reader, DEFAULT_MC_BLOCK_TEXTURES);
    }
    let mut host = ModHost::new();
    if let Some(packs) = packs.as_ref() {
        register_pack_fluid_textures(&mut host.registry, packs, &reader);
        register_pack_plant_textures(&mut host.registry, packs, &reader);
    }
    host.load_all(&reader, packs.as_ref())?;
    Ok(host)
}

fn register_pack_plant_textures(
    registry: &mut BlockRegistry,
    packs: &ResourcePackLoader,
    reader: &dyn crate::assets::AssetReader,
) {
    for (namespaced_id, mc_name) in [
        ("stagcrest:short_grass", "short_grass"),
        ("stagcrest:tall_grass_bottom", "tall_grass_bottom"),
        ("stagcrest:tall_grass_top", "tall_grass_top"),
        ("stagcrest:dandelion", "dandelion"),
        ("stagcrest:poppy", "poppy"),
        ("stagcrest:dead_bush", "dead_bush"),
        ("stagcrest:oak_leaves", "oak_leaves"),
    ] {
        register_texture_from_pack(registry, packs, reader, namespaced_id, mc_name, None);
    }
}

fn register_pack_fluid_textures(
    registry: &mut BlockRegistry,
    packs: &ResourcePackLoader,
    reader: &dyn crate::assets::AssetReader,
) {
    for (namespaced_id, mc_name) in [
        ("stagcrest:water_still", "water_still"),
        ("stagcrest:water_flow", "water_flow"),
    ] {
        let animation = packs.animation_for_mc_texture(mc_name);
        register_texture_from_pack(registry, packs, reader, namespaced_id, mc_name, animation);
    }
}

pub(crate) fn register_texture_from_pack(
    registry: &mut BlockRegistry,
    packs: &ResourcePackLoader,
    reader: &dyn crate::assets::AssetReader,
    namespaced_id: &str,
    mc_name: &str,
    animation: Option<stagcrest_protocol::TextureAnimation>,
) -> bool {
    packs.ensure_block_texture(reader, mc_name);
    let Some((width, height, png, anim)) = packs.load_mc_block_texture_png(reader, mc_name) else {
        return false;
    };
    registry.register_texture_from_png(
        namespaced_id.to_string(),
        png,
        width,
        height,
        animation.or(anim),
    );
    true
}
