use crate::{
    RegisterBiomeFeatureRequest, RegisterBiomeRequest, RegisterBlockRequest,
    RegisterCaveConfigRequest, RegisterCommandRequest, RegisterFeatureRequest,
    RegisterRiverConfigRequest, RegisterRiverFeatureRequest, RegisterTextureRequest,
};

#[link(wasm_import_module = "stagcrest_host")]
extern "C" {
    #[link_name = "register_block"]
    fn host_register_block(ptr: i32, len: i32) -> i32;
    #[link_name = "register_texture"]
    fn host_register_texture(ptr: i32, len: i32) -> i32;
    #[link_name = "register_texture_from_pack"]
    fn host_register_texture_from_pack(id_ptr: i32, id_len: i32, mc_ptr: i32, mc_len: i32) -> i32;
    #[link_name = "log_message"]
    fn host_log_message(ptr: i32, len: i32);
    #[link_name = "register_biome"]
    fn host_register_biome(ptr: i32, len: i32) -> i32;
    #[link_name = "register_feature"]
    fn host_register_feature(ptr: i32, len: i32) -> i32;
    #[link_name = "register_river_config"]
    fn host_register_river_config(ptr: i32, len: i32) -> i32;
    #[link_name = "register_river_feature"]
    fn host_register_river_feature(ptr: i32, len: i32) -> i32;
    #[link_name = "register_cave_config"]
    fn host_register_cave_config(ptr: i32, len: i32) -> i32;
    #[link_name = "register_biome_feature"]
    fn host_register_biome_feature(ptr: i32, len: i32) -> i32;
    #[link_name = "register_command"]
    fn host_register_command(ptr: i32, len: i32) -> i32;
    /// Pull the dispatched command name into the mod's output buffer.
    /// Returns bytes written (excluding NUL), or -1 on error / buffer too small.
    #[link_name = "command_name"]
    fn host_command_name(out_ptr: i32, out_max: i32) -> i32;
    /// Pull the dispatched command argument string into the mod's output buffer.
    #[link_name = "command_args"]
    fn host_command_args(out_ptr: i32, out_max: i32) -> i32;
    /// Send a system chat reply to the invoking client only.
    #[link_name = "command_reply"]
    fn host_command_reply(ptr: i32, len: i32);
    /// Set the server world day/night time (seconds within the day cycle).
    #[link_name = "set_world_time"]
    fn host_set_world_time(time: f64) -> i32;
    /// Read the server world day/night time.
    #[link_name = "get_world_time"]
    fn host_get_world_time() -> f64;
}

fn with_utf8<F>(text: &str, f: F) -> i32
where
    F: FnOnce(i32, i32) -> i32,
{
    let mut bytes = text.as_bytes().to_vec();
    bytes.shrink_to_fit();
    let ptr = bytes.as_ptr() as i32;
    let len = bytes.len() as i32;
    std::mem::forget(bytes);
    f(ptr, len)
}

pub fn register_block(req: RegisterBlockRequest) -> i32 {
    let json = serde_json::to_string(&req).expect("serialize RegisterBlockRequest");
    unsafe { with_utf8(&json, |ptr, len| host_register_block(ptr, len)) }
}

pub fn register_texture(req: RegisterTextureRequest) -> i32 {
    let json = serde_json::to_string(&req).expect("serialize RegisterTextureRequest");
    unsafe { with_utf8(&json, |ptr, len| host_register_texture(ptr, len)) }
}

pub fn register_texture_from_pack(namespaced_id: &str, mc_name: &str) -> i32 {
    unsafe {
        with_utf8(namespaced_id, |id_ptr, id_len| {
            with_utf8(mc_name, |mc_ptr, mc_len| {
                host_register_texture_from_pack(id_ptr, id_len, mc_ptr, mc_len)
            })
        })
    }
}

pub fn register_biome(req: RegisterBiomeRequest) -> i32 {
    let json = serde_json::to_string(&req).expect("serialize RegisterBiomeRequest");
    unsafe { with_utf8(&json, |ptr, len| host_register_biome(ptr, len)) }
}

pub fn register_feature(req: RegisterFeatureRequest) -> i32 {
    let json = serde_json::to_string(&req).expect("serialize RegisterFeatureRequest");
    unsafe { with_utf8(&json, |ptr, len| host_register_feature(ptr, len)) }
}

pub fn register_river_config(req: RegisterRiverConfigRequest) -> i32 {
    let json = serde_json::to_string(&req).expect("serialize RegisterRiverConfigRequest");
    unsafe { with_utf8(&json, |ptr, len| host_register_river_config(ptr, len)) }
}

pub fn register_river_feature(req: RegisterRiverFeatureRequest) -> i32 {
    let json = serde_json::to_string(&req).expect("serialize RegisterRiverFeatureRequest");
    unsafe { with_utf8(&json, |ptr, len| host_register_river_feature(ptr, len)) }
}

pub fn register_cave_config(req: RegisterCaveConfigRequest) -> i32 {
    let json = serde_json::to_string(&req).expect("serialize RegisterCaveConfigRequest");
    unsafe { with_utf8(&json, |ptr, len| host_register_cave_config(ptr, len)) }
}

pub fn register_biome_feature(req: RegisterBiomeFeatureRequest) -> i32 {
    let json = serde_json::to_string(&req).expect("serialize RegisterBiomeFeatureRequest");
    unsafe { with_utf8(&json, |ptr, len| host_register_biome_feature(ptr, len)) }
}

pub fn register_command(req: RegisterCommandRequest) -> i32 {
    let json = serde_json::to_string(&req).expect("serialize RegisterCommandRequest");
    unsafe { with_utf8(&json, |ptr, len| host_register_command(ptr, len)) }
}

/// Read the dispatched command name into `out`. Returns the number of bytes
/// written, or `None` if the buffer was too small or no command is being
/// dispatched.
pub fn command_name(out: &mut [u8]) -> Option<usize> {
    let n = unsafe { host_command_name(out.as_mut_ptr() as i32, out.len() as i32) };
    if n < 0 {
        None
    } else {
        Some(n as usize)
    }
}

/// Read the dispatched command argument string into `out`.
pub fn command_args(out: &mut [u8]) -> Option<usize> {
    let n = unsafe { host_command_args(out.as_mut_ptr() as i32, out.len() as i32) };
    if n < 0 {
        None
    } else {
        Some(n as usize)
    }
}

pub fn command_reply(text: &str) {
    unsafe {
        with_utf8(text, |ptr, len| {
            host_command_reply(ptr, len);
            0
        });
    }
}

/// Set the server world time (seconds within the day/night cycle).
/// Returns 0 on success, nonzero on error.
pub fn set_world_time(time: f64) -> i32 {
    unsafe { host_set_world_time(time) }
}

pub fn get_world_time() -> f64 {
    unsafe { host_get_world_time() }
}

pub fn log(msg: &str) {
    unsafe {
        with_utf8(msg, |ptr, len| {
            host_log_message(ptr, len);
            0
        });
    }
}
