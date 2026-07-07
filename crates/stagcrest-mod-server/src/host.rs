use crate::assets::{AssetReader, FsAssetReader};
use crate::block_tints::apply_block_face_tints;
use crate::commands::{CommandHost, CommandRegistry};
use crate::entity_registry::{EntityRegistry, EntityServerDef};
use crate::registry::BlockRegistry;
use crate::resourcepack::{ResourcePackLoader, DEFAULT_MC_BLOCK_TEXTURES};
use crate::runtime::{create_engine, load_mod, ModInstance, ModLoadContext};
use crate::worldgen::BiomeRegistry;
use crate::behavior::{BehaviorRegistry, BehaviorResult};
use stagcrest_mod_sdk::{
    BehaviorKindRequest, RegisterBlockRequest, RegisterEntityRequest,
};
use stagcrest_protocol::{
    BehaviorRef, BlockDef, BlockFaceTextures, BlockGeometry, BlockId, CallbackFlags,
    ModManifest, ModsManifest, NativeBehaviorId, UNKNOWN_BLOCK_ID,
};
use stagcrest_storage::{BlockIdRemap, BlockRegistryEntry};
use std::sync::Arc;
use wasmtime::Engine;
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
    pub entity_registry: EntityRegistry,
    pub biome_registry: BiomeRegistry,
    pub command_registry: CommandRegistry,
    pub behavior_registry: BehaviorRegistry,
    pub loaded_mods: Vec<String>,
    engine: Arc<Engine>,
    instances: Vec<ModInstance>,
}

impl ModHost {
    pub fn new() -> Self {
        let engine = create_engine().expect("wasmtime engine");
        let registry = BlockRegistry::new();
        Self {
            registry,
            entity_registry: EntityRegistry::new(),
            biome_registry: BiomeRegistry::default(),
            command_registry: CommandRegistry::new(),
            behavior_registry: BehaviorRegistry::new(),
            loaded_mods: Vec::new(),
            engine: Arc::new(engine),
            instances: Vec::new(),
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
        repo_root: &std::path::Path,
    ) -> Result<(), ModError> {
        let content = reader.read_bytes("mods/mods.toml")?;
        let manifest: ModsManifest = toml::from_str(
            std::str::from_utf8(&content)
                .map_err(|e| ModError::Message(format!("mods.toml is not valid UTF-8: {e}")))?,
        )?;

        for mod_entry in manifest.mods {
            self.load_mod(reader, &mod_entry, packs, repo_root)?;
        }

        self.finalize_biomes()?;

        Ok(())
    }

    fn load_mod(
        &mut self,
        reader: &dyn AssetReader,
        entry: &ModManifest,
        packs: Option<&ResourcePackLoader>,
        repo_root: &std::path::Path,
    ) -> Result<(), ModError> {
        let wasm_path = format!("mods/{}/{}", entry.id, entry.wasm);
        if !reader.exists(&wasm_path) {
            return Err(ModError::Message(format!(
                "wasm not found for mod {} at {wasm_path}",
                entry.id
            )));
        }

        let wasm_bytes = reader.read_bytes(&wasm_path)?;
        let mod_index = self.instances.len();
        let mod_assets_prefix = format!("mods/{}/{}", entry.id, entry.assets);
        let mut ctx = ModLoadContext {
            registry: &mut self.registry,
            entity_registry: &mut self.entity_registry,
            biome_registry: &mut self.biome_registry,
            command_registry: &mut self.command_registry,
            mod_index,
            packs,
            engine: self.engine.clone(),
            repo_root: repo_root.to_path_buf(),
            mod_assets_prefix,
        };
        let instance = load_mod(&mut ctx, &wasm_bytes).map_err(ModError::Runtime)?;

        self.behavior_registry.rebuild(&self.registry);

        // If this mod registered commands, it must export handle-command.
        let registered_commands = self
            .command_registry
            .names()
            .any(|name| self.command_registry.get(name).map(|e| e.mod_index) == Some(mod_index));
        if registered_commands && !instance.has_command_export() {
            return Err(ModError::Message(format!(
                "mod {} registered slash commands but does not export handle-command",
                entry.id
            )));
        }

        self.instances.push(instance);
        self.loaded_mods.push(entry.id.clone());
        tracing::info!("loaded wasm mod: {} v{}", entry.name, entry.version);
        Ok(())
    }

    /// Invoke a registered slash command's mod callback. Looks up `name` in the
    /// command registry, dispatches to the owning mod's `_stagcrest_command`
    /// export with the given argument string, and routes host calls
    /// (`set_world_time`, `command_reply`, …) through `host`. Returns the mod's
    /// exit code, or an error if the command is unknown or the callback trapped.
    pub fn invoke_command(
        &mut self,
        host: &mut dyn CommandHost,
        client_id: u64,
        name: &str,
        args: &str,
    ) -> Result<i32, String> {
        let Some(entry) = self.command_registry.get(name) else {
            return Err(format!("unknown command: /{}", name));
        };
        let mod_index = entry.mod_index;
        let name = entry.name.clone();
        let args = args.to_string();
        let Some(instance) = self.instances.get_mut(mod_index) else {
            return Err(format!("command owner mod {mod_index} not loaded"));
        };
        instance.invoke_command(host, client_id, name, args)
    }

    pub fn has_commands(&self) -> bool {
        !self.command_registry.is_empty()
    }

    pub fn air_block(&self) -> BlockId {
        self.registry
            .block_by_name("stagcrest:air")
            .unwrap_or(BlockId(0))
    }

    pub fn block_registry_snapshot(&self) -> Vec<BlockRegistryEntry> {
        BlockIdRemap::from_entries(
            self.registry
                .all_blocks()
                .map(|def| (def.id.0, def.namespaced_id.clone())),
        )
    }

    pub fn build_id_remap(&self, saved: &[BlockRegistryEntry]) -> BlockIdRemap {
        let mut by_name = std::collections::HashMap::new();
        for def in self.registry.all_blocks() {
            by_name.insert(def.namespaced_id.clone(), def.id);
        }
        let unknown = self
            .registry
            .block_by_name(UNKNOWN_BLOCK_ID)
            .unwrap_or(BlockId(0));
        BlockIdRemap::from_saved_registry(saved, &by_name, unknown)
    }

    pub fn rebuild_behaviors(&mut self) {
        self.behavior_registry.rebuild(&self.registry);
    }

    pub fn invoke_behavior(
        &mut self,
        mod_index: u32,
        hook: crate::runtime::BehaviorHook,
        pos: stagcrest_protocol::BlockPos,
        block_id: BlockId,
        state: stagcrest_protocol::BlockState,
        neighbor: Option<stagcrest_protocol::BlockPos>,
        world: &mut stagcrest_world::World,
    ) -> Result<BehaviorResult, String> {
        let Some(instance) = self.instances.get_mut(mod_index as usize) else {
            return Err(format!("mod {mod_index} not loaded"));
        };
        instance.invoke_behavior(
            hook,
            pos,
            block_id,
            state,
            neighbor,
            world,
            &self.registry,
        )
    }
}

impl Default for ModHost {
    fn default() -> Self {
        Self::new()
    }
}

pub fn register_block_host(reg: &mut BlockRegistry, json: RegisterBlockRequest, mod_index: usize) {
    let mut face_textures = reg
        .resolve_face_textures(&json.top_texture, &json.bottom_texture, &json.sides_texture)
        .unwrap_or(BlockFaceTextures::uniform(stagcrest_protocol::TextureId(0)));

    apply_block_face_tints(&json.namespaced_id, json.fluid, &mut face_textures, reg);

    let id = reg.allocate_block_id();
    let behavior = json.behavior.map(|b| match b.kind {
        BehaviorKindRequest::Native(native) => BehaviorRef::Native {
            id: native.to_protocol(),
        },
        BehaviorKindRequest::Wasm => BehaviorRef::Wasm {
            mod_index: mod_index as u32,
        },
    });

    let mut callbacks = json.callbacks;
    if let Some(BehaviorRef::Native { id: native_id }) = behavior {
        if matches!(
            native_id,
            NativeBehaviorId::RedstoneInverter { .. }
                | NativeBehaviorId::RedstoneSwitch { .. }
                | NativeBehaviorId::RedstoneRepeater { .. }
                | NativeBehaviorId::RedstoneObserver { .. }
                | NativeBehaviorId::RedstonePiston { .. }
        ) {
            callbacks = callbacks.union(CallbackFlags::STATE_FOR_PLACE);
        }
        if matches!(
            native_id,
            NativeBehaviorId::RedstoneLamp | NativeBehaviorId::RedstoneInverter { .. }
        ) {
            callbacks = callbacks.union(CallbackFlags::DYNAMIC_LIGHT);
        }
        if native_id == NativeBehaviorId::Bedrock {
            callbacks = callbacks.union(CallbackFlags::ON_BREAK);
        }
    }

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
        placeable: json.placeable,
        fluid: json.fluid,
        geometry: json
            .geometry
            .as_deref()
            .map(BlockGeometry::from_str)
            .unwrap_or_default(),
        render_layer,
        map_color: json.map_color,
        light_emission: json.light_emission,
        light_attenuation: json.light_attenuation,
        behavior,
        callbacks,
    });
}

/// Load an entity's Bedrock asset files (relative to the mod assets dir) and
/// register the type. Returns the assigned type id, or an error string.
pub fn register_entity_host(
    reg: &mut EntityRegistry,
    req: RegisterEntityRequest,
    repo_root: &std::path::Path,
    mod_assets_prefix: &str,
) -> Result<stagcrest_protocol::EntityTypeId, String> {
    let reader = FsAssetReader::new(repo_root);
    let read = |rel: &str| -> Result<Vec<u8>, String> {
        let full = format!("{mod_assets_prefix}/{rel}");
        reader
            .read_bytes(&full)
            .map_err(|e| format!("entity asset {full}: {e}"))
    };

    let geometry_json = read(&req.geometry_path)?;
    let texture_png = read(&req.texture_path)?;
    let animation_json = match &req.animation_path {
        Some(p) => Some(read(p)?),
        None => None,
    };

    let type_id = reg.allocate_type_id();
    reg.register(EntityServerDef {
        type_id,
        namespaced_id: req.namespaced_id,
        archetype: req.archetype,
        texture_width: req.texture_width,
        texture_height: req.texture_height,
        scale: if req.scale > 0.0 { req.scale } else { 1.0 },
        idle_animation: req.idle_animation,
        walk_animation: req.walk_animation,
        geometry_json,
        texture_png,
        animation_json,
        spawn_per_chunk_chance: req.spawn_per_chunk_chance,
        spawn_max_per_chunk: req.spawn_max_per_chunk,
    });
    Ok(type_id)
}

/// Register the reserved unknown placeholder block.
pub fn register_unknown_block(reg: &mut BlockRegistry) {
    if reg.block_by_name(UNKNOWN_BLOCK_ID).is_some() {
        return;
    }
    let id = reg.allocate_block_id();
    reg.register_texture("stagcrest:unknown".into(), 16, 16, vec![255; 16 * 16 * 4]);
    let tex = reg.texture_by_name("stagcrest:unknown").unwrap();
    reg.register_block(BlockDef {
        id,
        namespaced_id: UNKNOWN_BLOCK_ID.into(),
        display_name: "Unknown Block".into(),
        opaque: true,
        transparent: false,
        solid: true,
        hardness: 0.0,
        face_textures: BlockFaceTextures::uniform(tex),
        placeable: false,
        geometry: BlockGeometry::Cube,
        fluid: false,
        render_layer: stagcrest_protocol::ModelRenderLayer::Opaque,
        map_color: [255, 0, 255],
        light_emission: 0,
        light_attenuation: 0,
        behavior: None,
        callbacks: CallbackFlags::default(),
    });
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
    host.load_all(&reader, packs.as_ref(), repo_root)?;
    register_unknown_block(&mut host.registry);
    host.rebuild_behaviors();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CommandHost;
    use std::path::Path;

    /// A minimal `CommandHost` recording calls for assertion.
    struct MockHost {
        world_time: f64,
        replies: Vec<(u64, String)>,
    }

    impl CommandHost for MockHost {
        fn set_world_time(&mut self, time: f64) {
            self.world_time = time;
        }
        fn world_time(&self) -> f64 {
            self.world_time
        }
        fn send_chat_to(&mut self, client_id: u64, text: String) {
            self.replies.push((client_id, text));
        }
    }

    /// End-to-end: load the real `stagcrest-core` mod wasm, confirm it
    /// registered `/time`, and dispatch `/time 6000` — verifying the mod's
    /// `_stagcrest_command` callback runs and calls `set_world_time`. Skipped
    /// when the mod wasm has not been built (e.g. bare `cargo test` in CI).
    #[test]
    fn core_mod_time_command_sets_world_time() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let wasm = root.join("mods/stagcrest-core/stagcrest-core.wasm");
        if !wasm.exists() {
            eprintln!("skipping: {wasm:?} not built (run scripts/build-core-mod.sh)");
            return;
        }

        let mut host = load_mods(&root).expect("load core mod");
        assert!(
            host.command_registry.get("time").is_some(),
            "stagcrest-core should register the /time command"
        );

        let mut mock = MockHost {
            world_time: 0.0,
            replies: Vec::new(),
        };
        let rc = host
            .invoke_command(&mut mock, 1, "time", "6000")
            .expect("invoke /time");
        assert_eq!(rc, 0, "mod should report success");
        assert_eq!(mock.world_time, 6000.0, "set_world_time should be called");
        assert!(
            mock.replies.iter().any(|(_, t)| t.contains("Time set to 6000")),
            "mod should reply with confirmation: {replies:?}",
            replies = mock.replies
        );

        // Query path: empty args reports the current time.
        mock.replies.clear();
        let rc = host
            .invoke_command(&mut mock, 1, "time", "")
            .expect("invoke /time query");
        assert_eq!(rc, 0);
        assert!(mock
            .replies
            .iter()
            .any(|(_, t)| t.contains("World time")), "query reply: {replies:?}", replies = mock.replies);

        // Named preset path.
        let rc = host
            .invoke_command(&mut mock, 1, "time", "night")
            .expect("invoke /time night");
        assert_eq!(rc, 0);
        assert_eq!(mock.world_time, 100.0, "night preset uses DAY_LENGTH cycle");

        // Unknown command surfaces an error.
        let err = host
            .invoke_command(&mut mock, 1, "bogus", "")
            .expect_err("unknown command should error");
        assert!(err.contains("unknown command"));
    }

    /// The core mod registers the Bedrock player entity with loaded asset bytes.
    #[test]
    fn core_mod_registers_player_entity() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let wasm = root.join("mods/stagcrest-core/stagcrest-core.wasm");
        if !wasm.exists() {
            eprintln!("skipping: {wasm:?} not built (run scripts/build-core-mod.sh)");
            return;
        }
        let host = load_mods(&root).expect("load core mod");
        let player = host
            .entity_registry
            .all()
            .find(|e| e.namespaced_id == "stagcrest:player")
            .expect("player entity registered");
        assert_eq!(player.archetype, "humanoid");
        assert!(!player.geometry_json.is_empty(), "geometry bytes loaded");
        assert!(!player.texture_png.is_empty(), "texture bytes loaded");
        assert!(player.animation_json.is_some(), "animation bytes loaded");
        assert!(player.spawn_per_chunk_chance > 0.0);
    }
}
