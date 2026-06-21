use super::{AssetError, AssetReader};
use std::path::{Path, PathBuf};

pub struct FsAssetReader {
    root: PathBuf,
}

impl FsAssetReader {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

impl AssetReader for FsAssetReader {
    fn read_bytes(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        let full = self.resolve(path);
        std::fs::read(&full).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AssetError::NotFound(path.to_string())
            } else {
                AssetError::Io(e.to_string())
            }
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).exists()
    }
}
