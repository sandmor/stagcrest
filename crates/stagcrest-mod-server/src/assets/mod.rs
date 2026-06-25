mod fs;

pub use fs::FsAssetReader;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AssetError {
    #[error("asset not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(String),
}

pub trait AssetReader {
    fn read_bytes(&self, path: &str) -> Result<Vec<u8>, AssetError>;
    fn exists(&self, path: &str) -> bool;
}
