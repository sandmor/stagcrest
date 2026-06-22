use bevy::render::settings::Backends;
use wasm_bindgen::JsValue;

/// Backends chosen by `public/web-init.js` after a surface-compatible WebGPU probe.
pub fn web_backends() -> Backends {
    if webgpu_preflight_ok() {
        Backends::BROWSER_WEBGPU | Backends::GL
    } else {
        Backends::GL
    }
}

fn webgpu_preflight_ok() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    js_sys::Reflect::get(&window, &JsValue::from_str("__STAGCREST_WEBGPU_OK"))
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}
