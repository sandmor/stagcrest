use std::sync::Arc;

use crate::assets::FsAssetReader;
use crate::commands::{CommandHost, CommandRegistry};
use crate::entity_registry::EntityRegistry;
use crate::host::{register_block_host, register_entity_host};
use crate::registry::BlockRegistry;
use crate::resourcepack::ResourcePackLoader;
use crate::worldgen::{
    register_biome_feature_host, register_biome_host, register_cave_config_host,
    register_feature_host, register_river_config_host, register_river_feature_host, BiomeRegistry,
};
use stagcrest_mod_sdk::{
    RegisterBiomeFeatureRequest, RegisterBiomeRequest, RegisterBlockRequest,
    RegisterCaveConfigRequest, RegisterCommandRequest, RegisterEntityRequest, RegisterFeatureRequest,
    RegisterRiverConfigRequest, RegisterRiverFeatureRequest, RegisterTextureRequest,
};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, ResourceLimiter, Store};

use crate::runtime::bindings::stagcrest::plugin::host::Host;
use crate::runtime::bindings::stagcrest::plugin::types as wit_types;
use crate::runtime::bindings::Plugin;

const REGISTRATION_EPOCH_TICKS: u64 = 10_000;
const COMMAND_EPOCH_TICKS: u64 = 50;
const BEHAVIOR_EPOCH_TICKS: u64 = 20;
const MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;

pub struct ModLoadContext<'a> {
    pub registry: &'a mut BlockRegistry,
    pub entity_registry: &'a mut EntityRegistry,
    pub biome_registry: &'a mut BiomeRegistry,
    pub command_registry: &'a mut CommandRegistry,
    pub mod_index: usize,
    pub packs: Option<&'a ResourcePackLoader>,
    pub engine: Arc<Engine>,
    pub repo_root: std::path::PathBuf,
    pub mod_assets_prefix: String,
}

struct CommandCtx {
    host: *mut (dyn CommandHost + 'static),
    client_id: u64,
    name: String,
    args: String,
}

struct BehaviorCtx {
    world: *mut stagcrest_world::World,
    registry: *const BlockRegistry,
}

struct HostState {
    registry: Option<*mut BlockRegistry>,
    entity_registry: Option<*mut EntityRegistry>,
    biome_registry: Option<*mut BiomeRegistry>,
    command_registry: Option<*mut CommandRegistry>,
    current_mod_index: usize,
    packs: Option<*const ResourcePackLoader>,
    repo_root: std::path::PathBuf,
    mod_assets_prefix: String,
    command: Option<CommandCtx>,
    behavior: Option<BehaviorCtx>,
    limiter: StoreLimiter,
}

struct StoreLimiter {
    memory_bytes: usize,
}

impl ResourceLimiter for StoreLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(desired <= MAX_MEMORY_BYTES)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(true)
    }
}

unsafe impl Send for HostState {}
unsafe impl Sync for HostState {}

impl Host for HostState {
    fn register_block(
        &mut self,
        req: wit_types::RegisterBlockRequest,
    ) -> Result<wit_types::BlockId, String> {
        let registry = self
            .registry
            .ok_or_else(|| "register_block called outside load phase".to_string())?;
        let sdk = wit_register_block_to_sdk(req, self.current_mod_index);
        let namespaced = sdk.namespaced_id.clone();
        unsafe {
            register_block_host(&mut *registry, sdk, self.current_mod_index);
            let id = (*registry)
                .block_by_name(&namespaced)
                .ok_or_else(|| "block not registered".to_string())?;
            Ok(wit_types::BlockId { value: id.0 })
        }
    }

    fn register_entity(&mut self, req: wit_types::RegisterEntityRequest) -> Result<i32, String> {
        let registry = self
            .entity_registry
            .ok_or_else(|| "register_entity called outside load phase".to_string())?;
        let sdk = RegisterEntityRequest {
            namespaced_id: req.namespaced_id,
            archetype: req.archetype,
            geometry_path: req.geometry_path,
            texture_path: req.texture_path,
            animation_path: req.animation_path,
            texture_width: req.texture_width,
            texture_height: req.texture_height,
            scale: req.scale,
            idle_animation: req.idle_animation,
            walk_animation: req.walk_animation,
            spawn_per_chunk_chance: req.spawn_per_chunk_chance,
            spawn_max_per_chunk: req.spawn_max_per_chunk,
        };
        unsafe {
            let type_id = register_entity_host(
                &mut *registry,
                sdk,
                &self.repo_root,
                &self.mod_assets_prefix,
            )?;
            Ok(type_id.0 as i32)
        }
    }

    fn register_texture(&mut self, req: wit_types::RegisterTextureRequest) -> Result<i32, String> {
        let registry = self
            .registry
            .ok_or_else(|| "register_texture called outside load phase".to_string())?;
        let sdk = RegisterTextureRequest {
            namespaced_id: req.namespaced_id,
            width: req.width,
            height: req.height,
            rgba: req.rgba,
        };
        unsafe {
            let packs = self.packs.and_then(|p| p.as_ref());
            let animation =
                packs.and_then(|p| p.animation_for_stagcrest_texture(&sdk.namespaced_id));
            (*registry).register_texture_with_animation(
                sdk.namespaced_id,
                sdk.width,
                sdk.height,
                sdk.rgba,
                animation,
            );
        }
        Ok(0)
    }

    fn register_texture_from_pack(&mut self, namespaced_id: String, mc_name: String) -> i32 {
        let Some(registry) = self.registry else {
            return -1;
        };
        let Some(packs) = self.packs.and_then(|p| unsafe { p.as_ref() }) else {
            return -1;
        };
        unsafe {
            let reader = FsAssetReader::new(packs.repo_root());
            let loaded = crate::host::register_texture_from_pack(
                &mut *registry,
                packs,
                &reader,
                &namespaced_id,
                &mc_name,
                packs.animation_for_stagcrest_texture(&namespaced_id),
            );
            i32::from(loaded)
        }
    }

    fn register_biome(&mut self, req: wit_types::RegisterBiomeRequest) -> Result<i32, String> {
        let biome_registry = self
            .biome_registry
            .ok_or_else(|| "register_biome called outside load phase".to_string())?;
        let sdk = wit_biome_to_sdk(req);
        unsafe { register_biome_host(&mut *biome_registry, sdk) };
        Ok(0)
    }

    fn register_feature(&mut self, req: wit_types::RegisterFeatureRequest) -> Result<i32, String> {
        let biome_registry = self
            .biome_registry
            .ok_or_else(|| "register_feature called outside load phase".to_string())?;
        let sdk = wit_feature_to_sdk(req);
        unsafe { register_feature_host(&mut *biome_registry, sdk) };
        Ok(0)
    }

    fn register_river_config(
        &mut self,
        req: wit_types::RegisterRiverConfigRequest,
    ) -> Result<i32, String> {
        let biome_registry = self
            .biome_registry
            .ok_or_else(|| "register_river_config called outside load phase".to_string())?;
        let sdk = wit_river_config_to_sdk(req);
        unsafe { register_river_config_host(&mut *biome_registry, sdk) };
        Ok(0)
    }

    fn register_river_feature(
        &mut self,
        req: wit_types::RegisterRiverFeatureRequest,
    ) -> Result<i32, String> {
        let biome_registry = self
            .biome_registry
            .ok_or_else(|| "register_river_feature called outside load phase".to_string())?;
        let sdk = wit_river_feature_to_sdk(req);
        unsafe { register_river_feature_host(&mut *biome_registry, sdk) };
        Ok(0)
    }

    fn register_cave_config(
        &mut self,
        req: wit_types::RegisterCaveConfigRequest,
    ) -> Result<i32, String> {
        let biome_registry = self
            .biome_registry
            .ok_or_else(|| "register_cave_config called outside load phase".to_string())?;
        let sdk = wit_cave_config_to_sdk(req);
        unsafe { register_cave_config_host(&mut *biome_registry, sdk) };
        Ok(0)
    }

    fn register_biome_feature(
        &mut self,
        req: wit_types::RegisterBiomeFeatureRequest,
    ) -> Result<i32, String> {
        let biome_registry = self
            .biome_registry
            .ok_or_else(|| "register_biome_feature called outside load phase".to_string())?;
        let sdk = wit_biome_feature_to_sdk(req);
        unsafe { register_biome_feature_host(&mut *biome_registry, sdk) };
        Ok(0)
    }

    fn register_command(&mut self, req: wit_types::RegisterCommandRequest) -> i32 {
        let Some(command_registry) = self.command_registry else {
            return 1;
        };
        let sdk = RegisterCommandRequest {
            name: req.name,
            description: req.description,
            usage: req.usage,
        };
        unsafe {
            match (*command_registry).register(self.current_mod_index, sdk) {
                Ok(()) => 0,
                Err(reason) => {
                    tracing::warn!(
                        "mod {} register_command rejected: {reason}",
                        self.current_mod_index
                    );
                    1
                }
            }
        }
    }

    fn log(&mut self, msg: String) {
        tracing::info!(target: "mod", "{msg}");
    }

    fn command_name(&mut self) -> Option<String> {
        self.command.as_ref().map(|c| c.name.clone())
    }

    fn command_args(&mut self) -> Option<String> {
        self.command.as_ref().map(|c| c.args.clone())
    }

    fn command_reply(&mut self, text: String) {
        if let Some(ctx) = self.command.as_ref() {
            unsafe {
                if let Some(host) = ctx.host.as_mut() {
                    host.send_chat_to(ctx.client_id, text);
                }
            }
        }
    }

    fn set_world_time(&mut self, time: f64) -> i32 {
        if let Some(ctx) = self.command.as_ref() {
            unsafe {
                if let Some(host) = ctx.host.as_mut() {
                    host.set_world_time(time);
                    return 0;
                }
            }
        }
        -1
    }

    fn get_world_time(&mut self) -> f64 {
        if let Some(ctx) = self.command.as_ref() {
            unsafe {
                if let Some(host) = ctx.host.as_ref() {
                    return host.world_time();
                }
            }
        }
        0.0
    }

    fn get_block(
        &mut self,
        pos: wit_types::BlockPos,
    ) -> Option<(wit_types::BlockId, wit_types::BlockState)> {
        let behavior = self.behavior.as_ref()?;
        unsafe {
            let (id, state) = (*behavior.world).get_block(wit_to_block_pos(pos));
            Some((
                wit_types::BlockId { value: id.0 },
                wit_types::BlockState { value: state.0 },
            ))
        }
    }

    fn set_block_at(
        &mut self,
        pos: wit_types::BlockPos,
        id: wit_types::BlockId,
        state: wit_types::BlockState,
    ) -> Result<(), String> {
        let behavior = self
            .behavior
            .as_ref()
            .ok_or_else(|| "set_block_at outside behavior context".to_string())?;
        unsafe {
            (*behavior.world).set_block(
                wit_to_block_pos(pos),
                stagcrest_protocol::BlockId(id.value),
                stagcrest_protocol::BlockState(state.value),
            );
        }
        Ok(())
    }

    fn schedule_tick(&mut self, _pos: wit_types::BlockPos, _delay_ticks: u32) {}

    fn world_time(&mut self) -> f64 {
        self.get_world_time()
    }

    fn send_chat(&mut self, client_id: u64, text: String) {
        if let Some(ctx) = self.command.as_ref() {
            unsafe {
                if let Some(host) = ctx.host.as_mut() {
                    host.send_chat_to(client_id, text);
                }
            }
        }
    }
}

pub struct ModInstance {
    pub mod_index: usize,
    store: Store<HostState>,
    bindings: Plugin,
    has_command: bool,
}

impl ModInstance {
    pub fn invoke_command(
        &mut self,
        host: &mut dyn CommandHost,
        client_id: u64,
        name: String,
        args: String,
    ) -> Result<i32, String> {
        if !self.has_command {
            return Err("mod has no handle-command export".to_string());
        }
        let host_ptr = host as *mut dyn CommandHost;
        self.store.data_mut().command = Some(CommandCtx {
            host: unsafe { std::mem::transmute(host_ptr) },
            client_id,
            name,
            args,
        });
        self.store.set_epoch_deadline(COMMAND_EPOCH_TICKS);
        let result = self.bindings.call_handle_command(&mut self.store);
        self.store.data_mut().command = None;
        result.map_err(|e| format!("command callback failed: {e}"))
    }

    pub fn has_command_export(&self) -> bool {
        self.has_command
    }

    pub fn invoke_behavior(
        &mut self,
        hook: BehaviorHook,
        pos: stagcrest_protocol::BlockPos,
        block_id: stagcrest_protocol::BlockId,
        state: stagcrest_protocol::BlockState,
        neighbor: Option<stagcrest_protocol::BlockPos>,
        world: &mut stagcrest_world::World,
        registry: &BlockRegistry,
    ) -> Result<crate::behavior::BehaviorResult, String> {
        self.store.data_mut().behavior = Some(BehaviorCtx {
            world: world as *mut _,
            registry: registry as *const _,
        });
        self.store.set_epoch_deadline(BEHAVIOR_EPOCH_TICKS);
        let wit_pos = block_pos_to_wit(pos);
        let wit_id = wit_types::BlockId {
            value: block_id.0,
        };
        let wit_state = wit_types::BlockState { value: state.0 };
        let result = match hook {
            BehaviorHook::OnPlace => self
                .bindings
                .call_on_place(&mut self.store, wit_pos, wit_id, wit_state),
            BehaviorHook::OnBreak => self
                .bindings
                .call_on_break(&mut self.store, wit_pos, wit_id, wit_state),
            BehaviorHook::OnUse => self
                .bindings
                .call_on_use(&mut self.store, wit_pos, wit_id, wit_state),
            BehaviorHook::OnNeighborChanged => {
                let n = neighbor.map(block_pos_to_wit).unwrap_or(wit_pos);
                self.bindings.call_on_neighbor_changed(
                    &mut self.store,
                    wit_pos,
                    wit_id,
                    wit_state,
                    n,
                )
            }
            BehaviorHook::OnScheduledTick => self.bindings.call_on_scheduled_tick(
                &mut self.store,
                wit_pos,
                wit_id,
                wit_state,
            ),
            BehaviorHook::OnRandomTick => self
                .bindings
                .call_on_random_tick(&mut self.store, wit_pos, wit_id, wit_state),
        };
        self.store.data_mut().behavior = None;
        result
            .map(wit_to_behavior_result)
            .map_err(|e| format!("behavior callback failed: {e}"))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BehaviorHook {
    OnPlace,
    OnBreak,
    OnUse,
    OnNeighborChanged,
    OnScheduledTick,
    OnRandomTick,
}

pub fn create_engine() -> Result<Engine, String> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.epoch_interruption(true);
    config.cranelift_opt_level(wasmtime::OptLevel::Speed);
    Engine::new(&config).map_err(|e| e.to_string())
}

pub fn load_mod(ctx: &mut ModLoadContext<'_>, wasm_bytes: &[u8]) -> Result<ModInstance, String> {
    let component = Component::from_binary(&ctx.engine, wasm_bytes).map_err(|e| e.to_string())?;
    let state = HostState {
        registry: Some(ctx.registry as *mut BlockRegistry),
        entity_registry: Some(ctx.entity_registry as *mut EntityRegistry),
        biome_registry: Some(ctx.biome_registry as *mut BiomeRegistry),
        command_registry: Some(ctx.command_registry as *mut CommandRegistry),
        current_mod_index: ctx.mod_index,
        packs: ctx.packs.map(|p| p as *const ResourcePackLoader),
        repo_root: ctx.repo_root.clone(),
        mod_assets_prefix: ctx.mod_assets_prefix.clone(),
        command: None,
        behavior: None,
        limiter: StoreLimiter { memory_bytes: 0 },
    };
    let mut store = Store::new(&ctx.engine, state);
    store.limiter(|state| &mut state.limiter);
    let mut linker = Linker::new(&ctx.engine);
    crate::runtime::bindings::stagcrest::plugin::host::add_to_linker(&mut linker, |s| s)
        .map_err(|e| e.to_string())?;
    let bindings =
        Plugin::instantiate(&mut store, &component, &linker).map_err(|e| e.to_string())?;
    store.set_epoch_deadline(REGISTRATION_EPOCH_TICKS);
    bindings
        .call_register(&mut store)
        .map_err(|e| e.to_string())?;
    let store_data = store.data_mut();
    store_data.registry = None;
    store_data.entity_registry = None;
    store_data.biome_registry = None;
    store_data.command_registry = None;
    store_data.packs = None;
    let has_command = true;
    Ok(ModInstance {
        mod_index: ctx.mod_index,
        store,
        bindings,
        has_command,
    })
}

fn wit_to_block_pos(pos: wit_types::BlockPos) -> stagcrest_protocol::BlockPos {
    stagcrest_protocol::BlockPos::new(pos.x, pos.y, pos.z)
}

fn block_pos_to_wit(pos: stagcrest_protocol::BlockPos) -> wit_types::BlockPos {
    wit_types::BlockPos {
        x: pos.x,
        y: pos.y,
        z: pos.z,
    }
}

fn wit_to_behavior_result(r: wit_types::BehaviorResult) -> crate::behavior::BehaviorResult {
    use crate::behavior::BehaviorResult as Br;
    match r {
        wit_types::BehaviorResult::Ok => Br::Ok,
        wit_types::BehaviorResult::Cancel => Br::Cancel,
        wit_types::BehaviorResult::SetState(s) => Br::SetState(stagcrest_protocol::BlockState(s.value)),
        wit_types::BehaviorResult::SetBlock((id, s)) => Br::SetBlock(
            stagcrest_protocol::BlockId(id.value),
            stagcrest_protocol::BlockState(s.value),
        ),
    }
}

fn wit_register_block_to_sdk(
    req: wit_types::RegisterBlockRequest,
    mod_index: usize,
) -> RegisterBlockRequest {
    use stagcrest_mod_sdk::{BehaviorKindRequest, NativeBehaviorRequest, RegisterBehaviorRequest};
    use stagcrest_protocol::{BehaviorRef, CallbackFlags};

    let behavior = req.behavior.map(|b| match b {
        wit_types::BehaviorRef::Native(native) => RegisterBehaviorRequest {
            kind: BehaviorKindRequest::Native(wit_native_to_sdk(native)),
        },
        wit_types::BehaviorRef::Wasm => RegisterBehaviorRequest {
            kind: BehaviorKindRequest::Wasm,
        },
    });
    let callbacks = wit_callbacks_to_flags(req.callbacks);
    RegisterBlockRequest {
        namespaced_id: req.namespaced_id,
        display_name: req.display_name,
        opaque: req.opaque,
        transparent: req.transparent,
        solid: req.solid,
        hardness: req.hardness,
        top_texture: req.top_texture,
        bottom_texture: req.bottom_texture,
        sides_texture: req.sides_texture,
        placeable: req.placeable,
        fluid: req.fluid,
        render_layer: req.render_layer.map(|l| match l {
            wit_types::RenderLayer::Opaque => stagcrest_mod_sdk::RenderLayer::Opaque,
            wit_types::RenderLayer::Blend => stagcrest_mod_sdk::RenderLayer::Blend,
            wit_types::RenderLayer::Cutout => stagcrest_mod_sdk::RenderLayer::Cutout,
        }),
        geometry: req.geometry,
        behavior,
        callbacks,
        map_color: [req.map_color.0, req.map_color.1, req.map_color.2],
        light_emission: req.light_emission,
        light_attenuation: req.light_attenuation,
    }
}

fn wit_callbacks_to_flags(flags: wit_types::CallbackFlags) -> stagcrest_protocol::CallbackFlags {
    use stagcrest_protocol::CallbackFlags;
    let mut out = CallbackFlags::default();
    if flags.contains(wit_types::CallbackFlags::ON_PLACE) {
        out = out.union(CallbackFlags::ON_PLACE);
    }
    if flags.contains(wit_types::CallbackFlags::ON_BREAK) {
        out = out.union(CallbackFlags::ON_BREAK);
    }
    if flags.contains(wit_types::CallbackFlags::ON_USE) {
        out = out.union(CallbackFlags::ON_USE);
    }
    if flags.contains(wit_types::CallbackFlags::ON_NEIGHBOR_CHANGED) {
        out = out.union(CallbackFlags::ON_NEIGHBOR_CHANGED);
    }
    if flags.contains(wit_types::CallbackFlags::ON_SCHEDULED_TICK) {
        out = out.union(CallbackFlags::ON_SCHEDULED_TICK);
    }
    if flags.contains(wit_types::CallbackFlags::ON_RANDOM_TICK) {
        out = out.union(CallbackFlags::ON_RANDOM_TICK);
    }
    if flags.contains(wit_types::CallbackFlags::STATE_FOR_PLACE) {
        out = out.union(CallbackFlags::STATE_FOR_PLACE);
    }
    if flags.contains(wit_types::CallbackFlags::DYNAMIC_LIGHT) {
        out = out.union(CallbackFlags::DYNAMIC_LIGHT);
    }
    out
}

fn wit_native_to_sdk(native: wit_types::NativeBehavior) -> stagcrest_mod_sdk::NativeBehaviorRequest {
    use stagcrest_mod_sdk::NativeBehaviorRequest as N;
    match native {
        wit_types::NativeBehavior::RedstoneSource(level) => N::RedstoneSource { level },
        wit_types::NativeBehavior::RedstoneWire(falloff) => N::RedstoneWire { falloff },
        wit_types::NativeBehavior::RedstoneInverter(output) => N::RedstoneInverter { output },
        wit_types::NativeBehavior::RedstoneSwitch(output) => N::RedstoneSwitch { output },
        wit_types::NativeBehavior::RedstoneRepeater(output) => N::RedstoneRepeater { output },
        wit_types::NativeBehavior::RedstoneObserver(output) => N::RedstoneObserver { output },
        wit_types::NativeBehavior::RedstonePiston(sticky) => N::RedstonePiston { sticky },
        wit_types::NativeBehavior::RedstoneLamp => N::RedstoneLamp,
        wit_types::NativeBehavior::Bedrock => N::Bedrock,
        wit_types::NativeBehavior::PistonBody => N::PistonBody,
        wit_types::NativeBehavior::PistonHead => N::PistonHead,
    }
}

// --- WIT -> SDK converters for worldgen (abbreviated field mapping) ---

fn wit_biome_to_sdk(req: wit_types::RegisterBiomeRequest) -> RegisterBiomeRequest {
    RegisterBiomeRequest {
        namespaced_id: req.namespaced_id,
        dimension: match req.dimension {
            wit_types::BiomeDimension::Surface => stagcrest_mod_sdk::BiomeDimension::Surface,
            wit_types::BiomeDimension::Underground => stagcrest_mod_sdk::BiomeDimension::Underground,
            wit_types::BiomeDimension::SkyIsland => stagcrest_mod_sdk::BiomeDimension::SkyIsland,
        },
        temperature: noise_range(req.temperature),
        humidity: noise_range(req.humidity),
        continentalness: noise_range(req.continentalness),
        erosion: noise_range(req.erosion),
        depth: noise_range(req.depth),
        weirdness: noise_range(req.weirdness),
        offset: req.offset,
        surface_top: req.surface_top,
        surface_under: req.surface_under,
        surface_depth: req.surface_depth,
        underwater_top: req.underwater_top,
        underwater_under: req.underwater_under,
        environment: stagcrest_mod_sdk::BiomeEnvironment {
            fog_color: tuple3(req.environment.fog_color),
            fog_density: req.environment.fog_density,
            water_color: tuple3(req.environment.water_color),
            water_fog_color: tuple3(req.environment.water_fog_color),
            sky_color: tuple3(req.environment.sky_color),
            grass_color: req.environment.grass_color.map(tuple3),
            foliage_color: req.environment.foliage_color.map(tuple3),
        },
    }
}

fn wit_feature_to_sdk(req: wit_types::RegisterFeatureRequest) -> RegisterFeatureRequest {
    RegisterFeatureRequest {
        biome_id: req.biome_id,
        placement: wit_placement(req.placement),
        chance: req.chance,
    }
}

fn wit_river_config_to_sdk(req: wit_types::RegisterRiverConfigRequest) -> RegisterRiverConfigRequest {
    RegisterRiverConfigRequest {
        width: req.width,
        bank_blocks: req.bank_blocks,
        river_biome_id: req.river_biome_id,
        frozen_river_biome_id: req.frozen_river_biome_id,
        riverbank_biome_id: req.riverbank_biome_id,
        hydrology_mode: match req.hydrology_mode {
            wit_types::HydrologyMode::Terrace => stagcrest_mod_sdk::HydrologyMode::Terrace,
            wit_types::HydrologyMode::DrainageGrid => stagcrest_mod_sdk::HydrologyMode::DrainageGrid,
        },
        terrace_step: req.terrace_step,
        terrace_offset: req.terrace_offset,
        drainage_cell_size: req.drainage_cell_size,
        drainage_relax_passes: req.drainage_relax_passes,
        waterfall_min_drop: req.waterfall_min_drop,
        channel_depth: req.channel_depth,
        max_channel_carve: req.max_channel_carve,
        mouth_sea_margin: req.mouth_sea_margin,
    }
}

fn wit_river_feature_to_sdk(
    req: wit_types::RegisterRiverFeatureRequest,
) -> RegisterRiverFeatureRequest {
    RegisterRiverFeatureRequest {
        slot: match req.slot {
            wit_types::RiverFeatureSlot::WaterfallLip => stagcrest_mod_sdk::RiverFeatureSlot::WaterfallLip,
            wit_types::RiverFeatureSlot::WaterfallBase => {
                stagcrest_mod_sdk::RiverFeatureSlot::WaterfallBase
            }
            wit_types::RiverFeatureSlot::PoolSurface => stagcrest_mod_sdk::RiverFeatureSlot::PoolSurface,
        },
        placement: wit_placement(req.placement),
        chance: req.chance,
    }
}

fn wit_cave_config_to_sdk(req: wit_types::RegisterCaveConfigRequest) -> RegisterCaveConfigRequest {
    RegisterCaveConfigRequest {
        cheese_threshold: req.cheese_threshold,
        spaghetti_threshold: req.spaghetti_threshold,
        noodle_threshold: req.noodle_threshold,
        lush_cave_biome_id: req.lush_cave_biome_id,
        dripstone_cave_biome_id: req.dripstone_cave_biome_id,
        deep_dark_biome_id: req.deep_dark_biome_id,
    }
}

fn wit_biome_feature_to_sdk(
    req: wit_types::RegisterBiomeFeatureRequest,
) -> RegisterBiomeFeatureRequest {
    RegisterBiomeFeatureRequest {
        biome_id: req.biome_id,
        feature_kind: match req.feature_kind {
            wit_types::FeatureKind::ShortGrass => stagcrest_mod_sdk::FeatureKind::ShortGrass,
            wit_types::FeatureKind::TallGrass => stagcrest_mod_sdk::FeatureKind::TallGrass,
            wit_types::FeatureKind::Dandelion => stagcrest_mod_sdk::FeatureKind::Dandelion,
            wit_types::FeatureKind::Poppy => stagcrest_mod_sdk::FeatureKind::Poppy,
            wit_types::FeatureKind::Cactus => stagcrest_mod_sdk::FeatureKind::Cactus,
            wit_types::FeatureKind::DeadBush => stagcrest_mod_sdk::FeatureKind::DeadBush,
            wit_types::FeatureKind::OakTree => stagcrest_mod_sdk::FeatureKind::OakTree,
        },
        chance: req.chance,
    }
}

fn noise_range(r: wit_types::NoiseRange) -> stagcrest_mod_sdk::NoiseRange {
    stagcrest_mod_sdk::NoiseRange { min: r.min, max: r.max }
}

fn tuple3(t: (u8, u8, u8)) -> [u8; 3] {
    [t.0, t.1, t.2]
}

fn wit_placement(p: wit_types::FeaturePlacement) -> stagcrest_mod_sdk::FeaturePlacement {
    use stagcrest_mod_sdk::FeaturePlacement as F;
    match p {
        wit_types::FeaturePlacement::Plant((block, tall)) => F::Plant { block, tall },
        wit_types::FeaturePlacement::Patch((block, radius, density)) => {
            F::Patch { block, radius, density }
        }
        wit_types::FeaturePlacement::Tree((trunk, leaves, shape, height)) => F::Tree {
            trunk,
            leaves,
            shape: match shape {
                wit_types::TreeShape::Oak => stagcrest_mod_sdk::TreeShape::Oak,
                wit_types::TreeShape::Birch => stagcrest_mod_sdk::TreeShape::Birch,
                wit_types::TreeShape::Spruce => stagcrest_mod_sdk::TreeShape::Spruce,
                wit_types::TreeShape::Pine => stagcrest_mod_sdk::TreeShape::Pine,
                wit_types::TreeShape::Jungle => stagcrest_mod_sdk::TreeShape::Jungle,
                wit_types::TreeShape::Acacia => stagcrest_mod_sdk::TreeShape::Acacia,
                wit_types::TreeShape::DarkOak => stagcrest_mod_sdk::TreeShape::DarkOak,
                wit_types::TreeShape::Mangrove => stagcrest_mod_sdk::TreeShape::Mangrove,
                wit_types::TreeShape::Cherry => stagcrest_mod_sdk::TreeShape::Cherry,
                wit_types::TreeShape::Azalea => stagcrest_mod_sdk::TreeShape::Azalea,
                wit_types::TreeShape::Bamboo => stagcrest_mod_sdk::TreeShape::Bamboo,
            },
            height,
        },
        wit_types::FeaturePlacement::Boulder(block) => F::Boulder { block },
        wit_types::FeaturePlacement::Column((block, height)) => F::Column { block, height },
        wit_types::FeaturePlacement::IceSpike => F::IceSpike,
        wit_types::FeaturePlacement::Stalagmite(block) => F::Stalagmite { block },
        wit_types::FeaturePlacement::Stalactite(block) => F::Stalactite { block },
        wit_types::FeaturePlacement::SurfacePatch(block) => F::SurfacePatch { block },
        wit_types::FeaturePlacement::GlowFlora(block) => F::GlowFlora { block },
        wit_types::FeaturePlacement::WaterfallSheet(lip_block) => F::WaterfallSheet { lip_block },
        wit_types::FeaturePlacement::WaterfallPool((block, radius)) => F::WaterfallPool { block, radius },
    }
}
