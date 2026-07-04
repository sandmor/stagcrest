use std::collections::HashMap;

use thiserror::Error;

use crate::map_tile::MAP_CHUNK_PIXELS;

const MODE_PALETTED: u8 = 0;
const MODE_RAW: u8 = 1;

#[derive(Debug, Error)]
pub enum MapEncodeError {
    #[error("empty compressed data")]
    Empty,
    #[error("zstd decompress: {0}")]
    ZstdDecompress(String),
    #[error("invalid mode flag: {0}")]
    InvalidMode(u8),
    #[error("truncated payload")]
    Truncated,
    #[error("palette index out of range")]
    PaletteIndexOutOfRange,
}

/// Compress a 64×64 RGB layer (12288 bytes) with optional palettization + Zstd.
pub fn encode_layer_rgb(rgb: &[u8; crate::map_tile::MAP_CHUNK_RGB_BYTES]) -> Vec<u8> {
    let pre = build_pre_zstd_payload(rgb);
    zstd::bulk::compress(&pre, 3).expect("zstd compress")
}

/// Decompress a layer payload back to 12288 RGB bytes.
pub fn decode_layer_rgb(
    compressed: &[u8],
) -> Result<[u8; crate::map_tile::MAP_CHUNK_RGB_BYTES], MapEncodeError> {
    if compressed.is_empty() {
        return Err(MapEncodeError::Empty);
    }
    let pre = zstd::bulk::decompress(compressed, 64 * 1024)
        .map_err(|e| MapEncodeError::ZstdDecompress(e.to_string()))?;
    decode_pre_zstd_payload(&pre)
}

fn build_pre_zstd_payload(rgb: &[u8; crate::map_tile::MAP_CHUNK_RGB_BYTES]) -> Vec<u8> {
    let mut unique: HashMap<[u8; 3], u8> = HashMap::new();
    for i in 0..MAP_CHUNK_PIXELS {
        let off = i * 3;
        let color = [rgb[off], rgb[off + 1], rgb[off + 2]];
        if !unique.contains_key(&color) {
            if unique.len() >= 256 {
                let mut raw = Vec::with_capacity(1 + rgb.len());
                raw.push(MODE_RAW);
                raw.extend_from_slice(rgb);
                return raw;
            }
            unique.insert(color, unique.len() as u8);
        }
    }

    let palette_len = unique.len();
    let mut palette: Vec<[u8; 3]> = vec![[0, 0, 0]; palette_len];
    for (color, idx) in &unique {
        palette[*idx as usize] = *color;
    }

    let mut out = Vec::with_capacity(2 + palette_len * 3 + MAP_CHUNK_PIXELS);
    out.push(MODE_PALETTED);
    out.push((palette_len - 1) as u8);
    for c in &palette {
        out.extend_from_slice(c);
    }
    for i in 0..MAP_CHUNK_PIXELS {
        let off = i * 3;
        let color = [rgb[off], rgb[off + 1], rgb[off + 2]];
        out.push(*unique.get(&color).expect("color in palette"));
    }
    out
}

fn decode_pre_zstd_payload(
    pre: &[u8],
) -> Result<[u8; crate::map_tile::MAP_CHUNK_RGB_BYTES], MapEncodeError> {
    let mode = *pre.first().ok_or(MapEncodeError::Truncated)?;
    match mode {
        MODE_RAW => {
            let expected = 1 + crate::map_tile::MAP_CHUNK_RGB_BYTES;
            if pre.len() != expected {
                return Err(MapEncodeError::Truncated);
            }
            let mut rgb = [0u8; crate::map_tile::MAP_CHUNK_RGB_BYTES];
            rgb.copy_from_slice(&pre[1..]);
            Ok(rgb)
        }
        MODE_PALETTED => {
            if pre.len() < 2 {
                return Err(MapEncodeError::Truncated);
            }
            let palette_count = pre[1] as usize + 1;
            let palette_bytes = palette_count * 3;
            let indices_off = 2 + palette_bytes;
            if pre.len() != indices_off + MAP_CHUNK_PIXELS {
                return Err(MapEncodeError::Truncated);
            }
            let mut rgb = [0u8; crate::map_tile::MAP_CHUNK_RGB_BYTES];
            for (i, &idx) in pre[indices_off..].iter().enumerate() {
                let pi = idx as usize;
                if pi >= palette_count {
                    return Err(MapEncodeError::PaletteIndexOutOfRange);
                }
                let po = 2 + pi * 3;
                let ro = i * 3;
                rgb[ro] = pre[po];
                rgb[ro + 1] = pre[po + 1];
                rgb[ro + 2] = pre[po + 2];
            }
            Ok(rgb)
        }
        other => Err(MapEncodeError::InvalidMode(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_tile::MAP_CHUNK_RGB_BYTES;

    #[test]
    fn paletted_roundtrip_few_colors() {
        let mut rgb = [0u8; MAP_CHUNK_RGB_BYTES];
        for i in 0..MAP_CHUNK_PIXELS {
            let c = match i % 3 {
                0 => [255, 0, 0],
                1 => [0, 255, 0],
                _ => [0, 0, 255],
            };
            rgb[i * 3..i * 3 + 3].copy_from_slice(&c);
        }
        let compressed = encode_layer_rgb(&rgb);
        let decoded = decode_layer_rgb(&compressed).unwrap();
        assert_eq!(decoded, rgb);
    }

    #[test]
    fn raw_roundtrip_many_colors() {
        let mut rgb = [0u8; MAP_CHUNK_RGB_BYTES];
        for i in 0..MAP_CHUNK_PIXELS {
            let v = (i % 300) as u8;
            rgb[i * 3] = v;
            rgb[i * 3 + 1] = v.wrapping_mul(3);
            rgb[i * 3 + 2] = v.wrapping_mul(7);
        }
        let compressed = encode_layer_rgb(&rgb);
        let decoded = decode_layer_rgb(&compressed).unwrap();
        assert_eq!(decoded, rgb);
    }

    #[test]
    fn corrupt_input_errors() {
        assert!(matches!(decode_layer_rgb(&[]), Err(MapEncodeError::Empty)));
        let bad = zstd::bulk::compress(&[99u8], 3).unwrap();
        assert!(decode_layer_rgb(&bad).is_err());
    }
}
