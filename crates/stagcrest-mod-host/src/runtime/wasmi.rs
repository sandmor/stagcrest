use crate::host::register_block_host;
use crate::registry::BlockRegistry;
use crate::resourcepack::ResourcePackLoader;
use crate::runtime::memory::{read_utf8, write_bytes};
use stagcrest_mod_sdk::{RegisterBlockRequest, RegisterTextureRequest};
use std::ptr;
use wasmi::*;

pub struct ModLoadContext<'a> {
    pub registry: &'a mut BlockRegistry,
    pub packs: Option<&'a ResourcePackLoader>,
}

struct HostState {
    registry: *mut BlockRegistry,
    packs: *const ResourcePackLoader,
}

fn guest_memory(caller: &Caller<'_, HostState>) -> Option<Memory> {
    caller.get_export("memory").and_then(|export| export.into_memory())
}

fn link_host_functions(linker: &mut Linker<HostState>) -> Result<(), Error> {
    linker.func_wrap(
        "stagcrest_host",
        "register_block",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller)
                .ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_block payload"))?;
            let req: RegisterBlockRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_block json: {e}")))?;
            let registry = unsafe { &mut *caller.data().registry };
            register_block_host(registry, req);
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "register_texture",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, Error> {
            let memory = guest_memory(&caller)
                .ok_or_else(|| Error::new("missing guest memory"))?;
            let json = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid register_texture payload"))?;
            let req: RegisterTextureRequest = serde_json::from_str(&json)
                .map_err(|e| Error::new(format!("register_texture json: {e}")))?;
            let registry = unsafe { &mut *caller.data().registry };
            registry.register_texture(
                req.namespaced_id,
                req.width,
                req.height,
                req.rgba,
            );
            Ok(0)
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "log_message",
        |caller: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<(), Error> {
            let memory = guest_memory(&caller)
                .ok_or_else(|| Error::new("missing guest memory"))?;
            let msg = read_utf8(&memory, &caller, ptr, len)
                .ok_or_else(|| Error::new("invalid log_message payload"))?;
            tracing::info!(target: "mod", "{msg}");
            Ok(())
        },
    )?;

    linker.func_wrap(
        "stagcrest_host",
        "load_texture_from_pack",
        |mut caller: Caller<'_, HostState>,
         name_ptr: i32,
         name_len: i32,
         out_ptr: i32,
         out_max: i32|
         -> Result<i32, Error> {
            let memory = guest_memory(&caller)
                .ok_or_else(|| Error::new("missing guest memory"))?;
            let name = read_utf8(&memory, &caller, name_ptr, name_len)
                .ok_or_else(|| Error::new("invalid texture name"))?;
            let packs = unsafe {
                if caller.data().packs.is_null() {
                    None
                } else {
                    Some(&*caller.data().packs)
                }
            };
            let Some(packs) = packs else {
                return Ok(-1);
            };
            let Some((width, height, rgba)) = packs.load_mc_block_texture(&name) else {
                return Ok(-1);
            };
            let payload = serde_json::json!({
                "width": width,
                "height": height,
                "rgba": rgba,
            });
            let bytes = payload.to_string();
            let written = write_bytes(
                &memory,
                &mut caller,
                out_ptr,
                out_max,
                bytes.as_bytes(),
            )
            .ok_or_else(|| Error::new("texture output buffer too small"))?;
            Ok(written)
        },
    )?;

    Ok(())
}

pub fn load_mod(ctx: &mut ModLoadContext<'_>, wasm_bytes: &[u8]) -> Result<(), String> {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes).map_err(|e| e.to_string())?;

    let state = HostState {
        registry: ctx.registry as *mut BlockRegistry,
        packs: ctx
            .packs
            .map(|p| p as *const _)
            .unwrap_or(ptr::null()),
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
    register.call(&mut store, ()).map_err(|e| e.to_string())?;
    Ok(())
}
