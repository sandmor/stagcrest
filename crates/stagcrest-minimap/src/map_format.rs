use thiserror::Error;

use crate::map_encode::{decode_layer_rgb, encode_layer_rgb};
use crate::map_tile::MAP_CHUNK_RGB_BYTES;

pub const MAP_BLOB_FORMAT_VERSION: u8 = 1;
const LAYER_DEF_SIZE: usize = 24;
const HEADER_SIZE: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapLayerMeta {
    pub min_y: i32,
    pub max_y: i32,
    pub source_revision: u64,
    pub offset: usize,
    pub length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapLayer {
    pub min_y: i32,
    pub max_y: i32,
    pub source_revision: u64,
    pub rgb: [u8; MAP_CHUNK_RGB_BYTES],
}

#[derive(Debug, Error)]
pub enum MapFormatError {
    #[error("empty blob")]
    Empty,
    #[error("unsupported map blob format version {0}")]
    UnsupportedVersion(u8),
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
        source_revision: u64,
        rgb: &[u8; MAP_CHUNK_RGB_BYTES],
    ) -> Vec<u8> {
        let payload = encode_layer_rgb(rgb);
        let header_size = HEADER_SIZE + LAYER_DEF_SIZE;
        let offset = header_size as u32;
        let length = payload.len() as u32;

        let mut out = Vec::with_capacity(header_size + payload.len());
        out.push(MAP_BLOB_FORMAT_VERSION);
        out.push(1);
        out.extend_from_slice(&min_y.to_be_bytes());
        out.extend_from_slice(&max_y.to_be_bytes());
        out.extend_from_slice(&source_revision.to_be_bytes());
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(&payload);
        out
    }

    /// Read layer metadata from a v2 blob without decompressing payloads.
    pub fn peek_layers(data: &[u8]) -> Result<Vec<MapLayerMeta>, MapFormatError> {
        parse_layer_metas(data)
    }

    pub fn decode(data: &[u8]) -> Result<Vec<MapLayer>, MapFormatError> {
        let metas = parse_layer_metas(data)?;
        let mut layers = Vec::with_capacity(metas.len());
        for meta in metas {
            let rgb = decode_layer_rgb(&data[meta.offset..meta.offset + meta.length])?;
            layers.push(MapLayer {
                min_y: meta.min_y,
                max_y: meta.max_y,
                source_revision: meta.source_revision,
                rgb,
            });
        }
        Ok(layers)
    }
}

fn parse_layer_metas(data: &[u8]) -> Result<Vec<MapLayerMeta>, MapFormatError> {
    if data.is_empty() {
        return Err(MapFormatError::Empty);
    }
    if data[0] != MAP_BLOB_FORMAT_VERSION {
        return Err(MapFormatError::UnsupportedVersion(data[0]));
    }
    if data.len() < HEADER_SIZE {
        return Err(MapFormatError::TruncatedHeader);
    }
    let layer_count = data[1] as usize;
    if layer_count == 0 {
        return Err(MapFormatError::ZeroLayers);
    }
    let header_end = HEADER_SIZE + layer_count * LAYER_DEF_SIZE;
    if data.len() < header_end {
        return Err(MapFormatError::TruncatedHeader);
    }

    let mut layers = Vec::with_capacity(layer_count);
    for i in 0..layer_count {
        let base = HEADER_SIZE + i * LAYER_DEF_SIZE;
        let min_y = i32::from_be_bytes(data[base..base + 4].try_into().unwrap());
        let max_y = i32::from_be_bytes(data[base + 4..base + 8].try_into().unwrap());
        let source_revision = u64::from_be_bytes(data[base + 8..base + 16].try_into().unwrap());
        let offset = u32::from_be_bytes(data[base + 16..base + 20].try_into().unwrap()) as usize;
        let length = u32::from_be_bytes(data[base + 20..base + 24].try_into().unwrap()) as usize;

        if min_y > max_y {
            return Err(MapFormatError::InvalidBounds(i));
        }
        if offset < header_end || length == 0 || offset + length > data.len() {
            return Err(MapFormatError::InvalidOffset(i));
        }

        layers.push(MapLayerMeta {
            min_y,
            max_y,
            source_revision,
            offset,
            length,
        });
    }
    Ok(layers)
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
        let blob = MapChunkBlob::encode_single_layer(0, 256, 42, &rgb);
        let layers = MapChunkBlob::decode(&blob).unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].min_y, 0);
        assert_eq!(layers[0].max_y, 256);
        assert_eq!(layers[0].source_revision, 42);
        assert_eq!(layers[0].rgb, rgb);
    }

    #[test]
    fn invalid_bounds_rejected() {
        let rgb = [0u8; MAP_CHUNK_RGB_BYTES];
        let blob = MapChunkBlob::encode_single_layer(10, 5, 0, &rgb);
        assert!(MapChunkBlob::decode(&blob).is_err());
    }
}
