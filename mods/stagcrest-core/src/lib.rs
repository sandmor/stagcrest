mod blocks_extra;
mod content;
mod worldgen;

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

    fn register_biome(&mut self, req: stagcrest_mod_sdk::RegisterBiomeRequest) -> i32 {
        stagcrest_mod_sdk::register_biome(req)
    }

    fn register_feature(&mut self, req: stagcrest_mod_sdk::RegisterFeatureRequest) -> i32 {
        stagcrest_mod_sdk::register_feature(req)
    }

    fn register_river_config(&mut self, req: stagcrest_mod_sdk::RegisterRiverConfigRequest) -> i32 {
        stagcrest_mod_sdk::register_river_config(req)
    }

    fn register_river_feature(
        &mut self,
        req: stagcrest_mod_sdk::RegisterRiverFeatureRequest,
    ) -> i32 {
        stagcrest_mod_sdk::register_river_feature(req)
    }

    fn register_cave_config(&mut self, req: stagcrest_mod_sdk::RegisterCaveConfigRequest) -> i32 {
        stagcrest_mod_sdk::register_cave_config(req)
    }

    fn register_biome_feature(
        &mut self,
        req: stagcrest_mod_sdk::RegisterBiomeFeatureRequest,
    ) -> i32 {
        stagcrest_mod_sdk::register_biome_feature(req)
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
