mod fs;
#[cfg(target_arch = "wasm32")]
mod http;

pub use fs::FsAssetReader;
#[cfg(target_arch = "wasm32")]
pub use http::HttpAssetReader;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AssetError {
    #[error("asset not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(String),
    #[error("HTTP error: {0}")]
    Http(String),
}

pub trait AssetReader {
    fn read_bytes(&self, path: &str) -> Result<Vec<u8>, AssetError>;
    fn exists(&self, path: &str) -> bool;
}
