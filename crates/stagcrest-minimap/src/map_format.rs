use thiserror::Error;

use crate::map_encode::{decode_layer_rgb, encode_layer_rgb};
use crate::map_tile::MAP_CHUNK_RGB_BYTES;

const LAYER_DEF_SIZE: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapLayer {
    pub min_y: i32,
    pub max_y: i32,
    pub rgb: [u8; MAP_CHUNK_RGB_BYTES],
}

#[derive(Debug, Error)]
pub enum MapFormatError {
    #[error("empty blob")]
    Empty,
    #[error("truncated header")]
    TruncatedHeader,
    #[error("layer count is zero")]
    ZeroLayers,
    #[error("invalid layer bounds at index {0}")]
    InvalidBounds(usize),
    #[error("invalid layer offset/length at index {0}")]
    InvalidOffset(usize),
    #[error("encode: {0}")]
    Encode(#[from] crate::map_encode::MapEncodeError),
}

pub struct MapChunkBlob;

impl MapChunkBlob {
    /// Encode a single-layer map chunk blob.
    pub fn encode_single_layer(
        min_y: i32,
        max_y: i32,
        rgb: &[u8; MAP_CHUNK_RGB_BYTES],
    ) -> Vec<u8> {
        let payload = encode_layer_rgb(rgb);
        let header_size = 1 + LAYER_DEF_SIZE;
        let offset = header_size as u32;
        let length = payload.len() as u32;

        let mut out = Vec::with_capacity(header_size + payload.len());
        out.push(1);
        out.extend_from_slice(&min_y.to_be_bytes());
        out.extend_from_slice(&max_y.to_be_bytes());
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(&payload);
        out
    }

    pub fn decode(data: &[u8]) -> Result<Vec<MapLayer>, MapFormatError> {
        if data.is_empty() {
            return Err(MapFormatError::Empty);
        }
        let layer_count = data[0] as usize;
        if layer_count == 0 {
            return Err(MapFormatError::ZeroLayers);
        }
        let header_end = 1 + layer_count * LAYER_DEF_SIZE;
        if data.len() < header_end {
            return Err(MapFormatError::TruncatedHeader);
        }

        let mut layers = Vec::with_capacity(layer_count);
        for i in 0..layer_count {
            let base = 1 + i * LAYER_DEF_SIZE;
            let min_y = i32::from_be_bytes(data[base..base + 4].try_into().unwrap());
            let max_y = i32::from_be_bytes(data[base + 4..base + 8].try_into().unwrap());
            let offset = u32::from_be_bytes(data[base + 8..base + 12].try_into().unwrap()) as usize;
            let length = u32::from_be_bytes(data[base + 12..base + 16].try_into().unwrap()) as usize;

            if min_y > max_y {
                return Err(MapFormatError::InvalidBounds(i));
            }
            if offset < header_end || length == 0 || offset + length > data.len() {
                return Err(MapFormatError::InvalidOffset(i));
            }

            let rgb = decode_layer_rgb(&data[offset..offset + length])?;
            layers.push(MapLayer { min_y, max_y, rgb });
        }
        Ok(layers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_layer_roundtrip() {
        let mut rgb = [0u8; MAP_CHUNK_RGB_BYTES];
        rgb[0] = 10;
        rgb[1] = 20;
        rgb[2] = 30;
        let blob = MapChunkBlob::encode_single_layer(0, 256, &rgb);
        let layers = MapChunkBlob::decode(&blob).unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].min_y, 0);
        assert_eq!(layers[0].max_y, 256);
        assert_eq!(layers[0].rgb, rgb);
    }

    #[test]
    fn invalid_bounds_rejected() {
        let rgb = [0u8; MAP_CHUNK_RGB_BYTES];
        let blob = MapChunkBlob::encode_single_layer(10, 5, &rgb);
        assert!(MapChunkBlob::decode(&blob).is_err());
    }
}
