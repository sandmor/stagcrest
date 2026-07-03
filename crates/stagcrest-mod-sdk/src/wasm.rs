use crate::{
    RegisterBiomeFeatureRequest, RegisterBiomeRequest, RegisterBlockRequest,
    RegisterCaveConfigRequest, RegisterFeatureRequest, RegisterRiverConfigRequest,
    RegisterRiverFeatureRequest, RegisterTextureRequest,
};
use serde::Deserialize;

const PACK_TEXTURE_BUF: usize = 2 * 1024 * 1024;

#[link(wasm_import_module = "stagcrest_host")]
extern "C" {
    #[link_name = "register_block"]
    fn host_register_block(ptr: i32, len: i32) -> i32;
    #[link_name = "register_texture"]
    fn host_register_texture(ptr: i32, len: i32) -> i32;
    #[link_name = "log_message"]
    fn host_log_message(ptr: i32, len: i32);
    #[link_name = "load_texture_from_pack"]
    fn host_load_texture_from_pack(name_ptr: i32, name_len: i32, out_ptr: i32, out_max: i32)
        -> i32;
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

pub fn log(msg: &str) {
    unsafe {
        with_utf8(msg, |ptr, len| {
            host_log_message(ptr, len);
            0
        });
    }
}

pub fn load_texture_from_pack(mc_name: &str) -> Option<(u32, u32, Vec<u8>)> {
    let mut out = vec![0u8; PACK_TEXTURE_BUF];
    let written = unsafe {
        with_utf8(mc_name, |name_ptr, name_len| {
            host_load_texture_from_pack(
                name_ptr,
                name_len,
                out.as_mut_ptr() as i32,
                out.len() as i32,
            )
        })
    };
    if written <= 0 {
        return None;
    }
    let slice = &out[..written as usize];
    #[derive(Deserialize)]
    struct PackTexture {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    }
    let tex: PackTexture = serde_json::from_slice(slice).ok()?;
    Some((tex.width, tex.height, tex.rgba))
}
