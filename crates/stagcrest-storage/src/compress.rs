use lz4_flex::{compress_prepend_size, decompress_size_prepended};

use crate::StorageError;

pub fn compress_stored(data: &[u8]) -> Vec<u8> {
    compress_prepend_size(data)
}

pub fn decompress_stored(data: &[u8]) -> Result<Vec<u8>, StorageError> {
    decompress_size_prepended(data).map_err(|e| StorageError::Decompress(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let raw = b"hello chunk wire bytes";
        let compressed = compress_stored(raw);
        let back = decompress_stored(&compressed).unwrap();
        assert_eq!(back, raw);
    }
}
