mod content;

pub use content::register_content;

#[cfg(target_arch = "wasm32")]
struct WasmRegistrar;

#[cfg(target_arch = "wasm32")]
impl stagcrest_mod_sdk::ContentRegistrar for WasmRegistrar {
    fn register_texture(&mut self, req: stagcrest_mod_sdk::RegisterTextureRequest) -> i32 {
        stagcrest_mod_sdk::register_texture(req)
    }

    fn register_block(&mut self, req: stagcrest_mod_sdk::RegisterBlockRequest) -> i32 {
        stagcrest_mod_sdk::register_block(req)
    }

    fn log(&self, msg: &str) {
        stagcrest_mod_sdk::log(msg);
    }
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn _stagcrest_register() -> i32 {
    let mut reg = WasmRegistrar;
    register_content(&mut reg);
    0
}
