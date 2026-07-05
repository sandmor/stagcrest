use crate::assets::AssetReader;
use crate::block_tints::apply_block_face_tints;
use crate::commands::{CommandHost, CommandRegistry};
use crate::registry::BlockRegistry;
use crate::resourcepack::{ResourcePackLoader, DEFAULT_MC_BLOCK_TEXTURES};
use crate::runtime::{load_mod, ModInstance, ModLoadContext};
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
    pub command_registry: CommandRegistry,
    pub loaded_mods: Vec<String>,
    instances: Vec<ModInstance>,
}

impl ModHost {
    pub fn new() -> Self {
        Self {
            registry: BlockRegistry::new(),
            biome_registry: BiomeRegistry::default(),
            command_registry: CommandRegistry::new(),
            loaded_mods: Vec::new(),
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
        let mod_index = self.instances.len();
        let mut ctx = ModLoadContext {
            registry: &mut self.registry,
            biome_registry: &mut self.biome_registry,
            command_registry: &mut self.command_registry,
            mod_index,
            packs,
        };
        let instance = load_mod(&mut ctx, &wasm_bytes).map_err(ModError::Runtime)?;

        // If this mod registered commands, it must export `_stagcrest_command`.
        let registered_commands = self
            .command_registry
            .names()
            .any(|name| self.command_registry.get(name).map(|e| e.mod_index) == Some(mod_index));
        if registered_commands && !instance.has_command_export() {
            return Err(ModError::Message(format!(
                "mod {} registered slash commands but does not export _stagcrest_command",
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
            CircuitKindRequest::Lamp => CircuitKind::Lamp,
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

    let redstone_powerable = json.redstone_powerable.unwrap_or(
        stagcrest_protocol::default_redstone_powerable(
            json.solid,
            json.opaque,
            json.fluid,
            json.transparent,
        ),
    );

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
        redstone_powerable,
        light_emission: json.light_emission,
        light_emission_when_lit: json.light_emission_when_lit.unwrap_or(false),
        light_attenuation: json.light_attenuation,
        blocks_sky_light: json.blocks_sky_light,
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
}
