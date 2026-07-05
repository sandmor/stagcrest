use crate::assets::FsAssetReader;
use crate::commands::{CommandHost, CommandRegistry};
use crate::host::{register_block_host, register_texture_from_pack};
use crate::registry::BlockRegistry;
use crate::resourcepack::ResourcePackLoader;
use crate::runtime::memory::{read_utf8, write_bytes};
use crate::worldgen::{
    register_biome_feature_host, register_biome_host, register_cave_config_host,
    register_feature_host, register_river_config_host, register_river_feature_host, BiomeRegistry,
};
use stagcrest_mod_sdk::{
    RegisterBiomeFeatureRequest, RegisterBiomeRequest, RegisterBlockRequest,
    RegisterCaveConfigRequest, RegisterCommandRequest, RegisterFeatureRequest,
    RegisterRiverConfigRequest, RegisterRiverFeatureRequest, RegisterTextureRequest,
};
use wasmi::*;

/// Per-command fuel budget. Prevents a runaway mod callback from hanging the
/// server tick; an out-of-fuel trap is reported as a command failure.
const COMMAND_FUEL_BUDGET: u64 = 200_000;

/// Fuel budget for the `_stagcrest_register` call. Registration touches a lot
/// of host imports (blocks, textures, biomes, features), so this is generous.
const REGISTRATION_FUEL_BUDGET: u64 = 100_000_000;

/// Send-able raw pointer wrapper. The host only dereferences these on the
/// single server thread during synchronous registration / command dispatch,
/// but `Store<HostState>` must be `Send + Sync` for `GameServer` to stay `Send`
/// (the server is held across `tokio::select!` awaits in `run_standalone`).
#[derive(Debug)]
struct SendPtr<T: ?Sized>(*mut T);

impl<T: ?Sized> Clone for SendPtr<T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}
impl<T: ?Sized> Copy for SendPtr<T> {}

unsafe impl<T: ?Sized> Send for SendPtr<T> {}
unsafe impl<T: ?Sized> Sync for SendPtr<T> {}

impl<T: ?Sized> SendPtr<T> {
    /// # Safety
    /// Caller must ensure the pointee is valid and that no aliasing rules are
    /// violated. The returned reference has an unbound lifetime so it can
    /// outlive the `SendPtr` handle (the pointer itself is not owned data).
    unsafe fn as_mut<'a>(&self) -> Option<&'a mut T> {
        if self.0.is_null() {
            None
        } else {
            Some(&mut *self.0)
        }
    }

    unsafe fn as_ref<'a>(&self) -> Option<&'a T> {
        if self.0.is_null() {
            None
        } else {
            Some(&*self.0)
        }
    }
}

/// Live command-dispatch context, installed on a mod's `HostState` only while
/// its `_stagcrest_command` export is executing.
struct CommandCtx {
    /// `CommandHost` trait object with its lifetime erased to `'static` so it
    /// can live inside `Store<HostState>`. It is only dereferenced
    /// synchronously during `invoke_command`, within the real borrow's lifetime.
    host: SendPtr<dyn CommandHost>,
    client_id: u64,
    name: String,
    args: String,
}

/// # Safety
/// The returned `'static` trait-object pointer is only valid for as long as the
/// original borrow lives; callers must not dereference it after that.
unsafe fn erase_host_lifetime<'a>(
    host: *mut (dyn CommandHost + 'a),
) -> *mut (dyn CommandHost + 'static) {
    std::mem::transmute(host)
}

pub struct ModLoadContext<'a> {
    pub registry: &'a mut BlockRegistry,
    pub biome_registry: &'a mut BiomeRegistry,
    pub command_registry: &'a mut CommandRegistry,
    pub mod_index: usize,
    pub packs: Option<&'a ResourcePackLoader>,
}

/// Host state stored inside each mod's wasmi `Store`.
///
/// Load-phase pointers (`registry`, `biome_registry`, `command_registry`,
/// `packs`) are `Some` only during `_stagcrest_register`; they are cleared to
/// `None` afterwards so a later import call can't reach into host state that
/// has been moved out (the server does `mem::take(&mut mod_host.registry)`).
struct HostState {
    registry: Option<SendPtr<BlockRegistry>>,
    biome_registry: Option<SendPtr<BiomeRegistry>>,
    command_registry: Option<SendPtr<CommandRegistry>>,
    current_mod_index: usize,
    packs: Option<SendPtr<ResourcePackLoader>>,
    command: Option<CommandCtx>,
}

/// A loaded mod kept alive so its `_stagcrest_command` export can be invoked
/// at runtime. Owned by [`crate::host::ModHost`].
pub struct ModInstance {
    #[allow(dead_code)]
    pub mod_index: usize,
    store: Store<HostState>,
    command_func: Option<TypedFunc<(), i32>>,
}

impl ModInstance {
    /// Invoke the mod's `_stagcrest_command` export with the given dispatch
    /// context. Returns the mod's exit code, or an error string if the export
    /// is missing, the mod trapped, or it ran out of fuel.
    pub fn invoke_command(
        &mut self,
        host: &mut dyn CommandHost,
        client_id: u64,
        name: String,
        args: String,
    ) -> Result<i32, String> {
        let command_func = self
            .command_func
            .ok_or_else(|| "mod has no _stagcrest_command export".to_string())?;

        let host_ptr = unsafe { erase_host_lifetime(host as *mut dyn CommandHost) };
        self.store.data_mut().command = Some(CommandCtx {
            host: SendPtr(host_ptr),
            client_id,
            name,
            args,
        });

        // Reset the fuel budget for this invocation. A trapping call (e.g.
        // out-of-fuel) still reaches the cleanup below via this function's
        // early return path.
        let _ = self.store.set_fuel(COMMAND_FUEL_BUDGET);

        let result = command_func.call(&mut self.store, ());

        // Always clear the command context so a stray import after dispatch
        // fails cleanly.
        self.store.data_mut().command = None;

        result.map_err(|e| format!("command callback failed: {e}"))
    }

    pub fn has_command_export(&self) -> bool {
        self.command_func.is_some()
    }
}

fn guest_memory(caller: &Caller<'_, HostState>) -> Option<Memory> {
    caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
}

fn link_host_functions(linker: &mut Linker<HostState>) -> Result<(), Error> {
    linker.func_wrap(
        "stagcrest_host",
        "register_block",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_block payload"))?;
            let req: RegisterBlockRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_block json: {e}")))?;
            let registry = caller
                .data()
                .registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| Error::new("register_block called outside load phase"))?;
            register_block_host(registry, req);
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_texture",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_texture payload"))?;
            let req: RegisterTextureRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_texture json: {e}")))?;
            let registry = caller
                .data()
                .registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| Error::new("register_texture called outside load phase"))?;
            let packs = caller.data().packs.and_then(|p| unsafe { p.as_ref() });
            let animation =
                packs.and_then(|p| p.animation_for_stagcrest_texture(&req.namespaced_id));
            registry.register_texture_with_animation(
                req.namespaced_id,
                req.width,
                req.height,
                req.rgba,
                animation,
            );
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_biome",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_biome payload"))?;
            let req: RegisterBiomeRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_biome json: {e}")))?;
            let biome_registry = caller
                .data()
                .biome_registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| Error::new("register_biome called outside load phase"))?;
            register_biome_host(biome_registry, req);
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_feature",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_feature payload"))?;
            let req: RegisterFeatureRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_feature json: {e}")))?;
            let biome_registry = caller
                .data()
                .biome_registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| Error::new("register_feature called outside load phase"))?;
            register_feature_host(biome_registry, req);
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_river_config",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_river_config payload"))?;
            let req: RegisterRiverConfigRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_river_config json: {e}")))?;
            let biome_registry = caller
                .data()
                .biome_registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| Error::new("register_river_config called outside load phase"))?;
            register_river_config_host(biome_registry, req);
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_river_feature",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_river_feature payload"))?;
            let req: RegisterRiverFeatureRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_river_feature json: {e}")))?;
            let biome_registry = caller
                .data()
                .biome_registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| Error::new("register_river_feature called outside load phase"))?;
            register_river_feature_host(biome_registry, req);
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_cave_config",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_cave_config payload"))?;
            let req: RegisterCaveConfigRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_cave_config json: {e}")))?;
            let biome_registry = caller
                .data()
                .biome_registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| Error::new("register_cave_config called outside load phase"))?;
            register_cave_config_host(biome_registry, req);
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_biome_feature",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_biome_feature payload"))?;
            let req: RegisterBiomeFeatureRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_biome_feature json: {e}")))?;
            let biome_registry = caller
                .data()
                .biome_registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| Error::new("register_biome_feature called outside load phase"))?;
            register_biome_feature_host(biome_registry, req);
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_command",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_command payload"))?;
            let req: RegisterCommandRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_command json: {e}")))?;
            let command_registry = caller
                .data()
                .command_registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| Error::new("register_command called outside load phase"))?;
            let mod_index = caller.data().current_mod_index;
            match command_registry.register(mod_index, req) {
                Ok(()) => Ok(0),
                Err(reason) => {
                    tracing::warn!("mod {mod_index} register_command rejected: {reason}");
                    Ok(1)
                }
            }
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "log_message",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<(), Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let msg = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid log_message payload"))?;
            tracing::info!(target: "mod", "{msg}");
            Ok(())
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_texture_from_pack",
        |caller: Caller<'_, HostState>,
         id_ptr: i32,
         id_len: i32,
         mc_ptr: i32,
         mc_len: i32|
         -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let namespaced_id = read_utf8(&memory, &caller, id_ptr, id_len)
                .ok_or_else(|| Error::new("invalid namespaced_id"))?;
            let mc_name = read_utf8(&memory, &caller, mc_ptr, mc_len)
                .ok_or_else(|| Error::new("invalid mc_name"))?;
            let registry = caller
                .data()
                .registry
                .and_then(|p| unsafe { p.as_mut() })
                .ok_or_else(|| {
                    Error::new("register_texture_from_pack called outside load phase")
                })?;
            let packs = caller.data().packs.and_then(|p| unsafe { p.as_ref() });
            let Some(packs) = packs else {
                return Ok(-1);
            };
            let reader = FsAssetReader::new(packs.repo_root());
            let loaded = register_texture_from_pack(
                registry,
                packs,
                &reader,
                &namespaced_id,
                &mc_name,
                packs.animation_for_stagcrest_texture(&namespaced_id),
            );
            Ok(i32::from(loaded))
        },
    )?;

    // Command-phase imports (only valid while a command is being dispatched)

    linker.func_wrap(
        "stagcrest_host",
        "command_name",
        |caller: Caller<'_, HostState>, out_ptr: i32, out_max: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            // Clone the bytes first so we don't hold an immutable borrow of
            // `caller` while `write_bytes` needs it mutably.
            let bytes = caller
                .data()
                .command
                .as_ref()
                .map(|ctx| ctx.name.as_bytes().to_vec());
            let Some(bytes) = bytes else {
                return Ok(-1);
            };
            Ok(write_bytes(&memory, caller, out_ptr, out_max, &bytes).unwrap_or(-1))
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "command_args",
        |caller: Caller<'_, HostState>, out_ptr: i32, out_max: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let bytes = caller
                .data()
                .command
                .as_ref()
                .map(|ctx| ctx.args.as_bytes().to_vec());
            let Some(bytes) = bytes else {
                return Ok(-1);
            };
            Ok(write_bytes(&memory, caller, out_ptr, out_max, &bytes).unwrap_or(-1))
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "command_reply",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<(), Error> {
            let memory = guest_memory(&caller).ok_or_else(|| Error::new("missing guest memory"))?;
            let text = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid command_reply payload"))?;
            let data = caller.data();
            let Some(ctx) = data.command.as_ref() else {
                return Ok(());
            };
            let host = unsafe { ctx.host.as_mut() };
            if let Some(host) = host {
                host.send_chat_to(ctx.client_id, text);
            }
            Ok(())
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "set_world_time",
        |caller: Caller<'_, HostState>, time: f64| -> Result<i32, Error> {
            let data = caller.data();
            let Some(ctx) = data.command.as_ref() else {
                return Ok(-1);
            };
            let host = unsafe { ctx.host.as_mut() };
            if let Some(host) = host {
                host.set_world_time(time);
                Ok(0)
            } else {
                Ok(-1)
            }
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "get_world_time",
        |caller: Caller<'_, HostState>| -> Result<f64, Error> {
            let data = caller.data();
            let Some(ctx) = data.command.as_ref() else {
                return Ok(0.0);
            };
            let host = unsafe { ctx.host.as_ref() };
            if let Some(host) = host {
                Ok(host.world_time())
            } else {
                Ok(0.0)
            }
        },
    )?;

    Ok(())
}

/// Load a mod wasm module, run its `_stagcrest_register` export, and keep the
/// instance alive so `_stagcrest_command` can be invoked later. Returns the
/// live [`ModInstance`].
pub fn load_mod(ctx: &mut ModLoadContext<'_>, wasm_bytes: &[u8]) -> Result<ModInstance, String> {
    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let module = Module::new(&engine, wasm_bytes).map_err(|e| e.to_string())?;

    let state = HostState {
        registry: Some(SendPtr(ctx.registry as *mut BlockRegistry)),
        biome_registry: Some(SendPtr(ctx.biome_registry as *mut BiomeRegistry)),
        command_registry: Some(SendPtr(ctx.command_registry as *mut CommandRegistry)),
        current_mod_index: ctx.mod_index,
        packs: ctx
            .packs
            .map(|p| SendPtr(p as *const ResourcePackLoader as *mut ResourcePackLoader)),
        command: None,
    };
    let mut store = Store::new(&engine, state);
    let mut linker = Linker::new(&engine);
    link_host_functions(&mut linker).map_err(|e| e.to_string())?;

    let instance_pre = linker
        .instantiate(&mut store, &module)
        .map_err(|e| e.to_string())?;
    let instance = instance_pre
        .ensure_no_start(&mut store)
        .map_err(|e| e.to_string())?;

    let register = instance
        .get_typed_func::<(), i32>(&store, "_stagcrest_register")
        .map_err(|e| e.to_string())?;
    // Registration does substantial work (many register_block/texture/biome
    // calls); give it a generous fuel budget. Command dispatch later uses a
    // much smaller per-invocation budget.
    let _ = store.set_fuel(REGISTRATION_FUEL_BUDGET);
    register.call(&mut store, ()).map_err(|e| e.to_string())?;

    // Load phase complete: drop access to host registries so a later import
    // call can't reach into state the server has since moved out.
    let store_data = store.data_mut();
    store_data.registry = None;
    store_data.biome_registry = None;
    store_data.command_registry = None;
    store_data.packs = None;

    // Resolve the optional command export. Missing exports are fine for mods
    // that don't register commands; a mod that registered commands but lacks
    // the export is validated by the caller via the command registry.
    let command_func = instance
        .get_typed_func::<(), i32>(&store, "_stagcrest_command")
        .ok();

    // The `Instance` handle is `Copy` and only needed to resolve exports above;
    // the typed function handles keep working through the store after it's gone.

    Ok(ModInstance {
        mod_index: ctx.mod_index,
        store,
        command_func,
    })
}
