mod blocks_extra;
mod commands;
mod content;
mod worldgen;

#[cfg(target_arch = "wasm32")]
mod bindings;

#[cfg(target_arch = "wasm32")]
mod guest;

pub use content::register_content;
