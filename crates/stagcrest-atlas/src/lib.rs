mod downscale;
mod pack;

use stagcrest_protocol::{TextureDef, TextureId};
use thiserror::Error;

pub use pack::{AtlasLimits, AtlasPage, AtlasSet, Placement};

#[derive(Debug, Error)]
pub enum AtlasError {
    #[error("texture {name} rgba length {actual} != expected {expected}")]
    RgbaLength {
        name: String,
        actual: usize,
        expected: usize,
    },
    #[error("texture {name} has zero dimensions")]
    ZeroDimensions { name: String },
    #[error("failed to pack texture {name} ({w}x{h}): {reason}")]
    PackFailed {
        name: String,
        w: u32,
        h: u32,
        reason: String,
    },
    #[error("exceeded max atlas pages ({max_atlases})")]
    TooManyAtlases { max_atlases: u32 },
}

/// Decode PNG bytes into a `TextureDef` (validates dimensions vs rgba after decode).
pub fn texture_def_from_png(
    id: TextureId,
    namespaced_id: String,
    png: &[u8],
    animation: Option<stagcrest_protocol::TextureAnimation>,
) -> Result<TextureDef, AtlasError> {
    let img = image::load_from_memory(png).map_err(|e| AtlasError::PackFailed {
        name: namespaced_id.clone(),
        w: 0,
        h: 0,
        reason: format!("PNG decode: {e}"),
    })?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return Err(AtlasError::ZeroDimensions {
            name: namespaced_id,
        });
    }
    let rgba = rgba.into_raw();
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|p| p.checked_mul(4))
        .unwrap_or(0);
    if rgba.len() != expected {
        return Err(AtlasError::RgbaLength {
            name: namespaced_id,
            actual: rgba.len(),
            expected,
        });
    }
    Ok(TextureDef {
        id,
        namespaced_id,
        width,
        height,
        rgba,
        animation,
    })
}

/// Build atlas pages from decoded texture definitions.
pub fn build_atlas_set(
    textures: impl IntoIterator<Item = TextureDef>,
    limits: AtlasLimits,
) -> Result<AtlasSet, AtlasError> {
    AtlasSet::build(textures, limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn solid_png(w: u32, h: u32, r: u8) -> Vec<u8> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(w, h, Rgba([r, r, r, 255]));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        png
    }

    fn mock_texture(id: u32, name: &str, w: u32, h: u32, fill: u8) -> TextureDef {
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for px in rgba.chunks_mut(4) {
            px.copy_from_slice(&[fill, fill, fill, 255]);
        }
        TextureDef {
            id: TextureId(id),
            namespaced_id: name.to_string(),
            width: w,
            height: h,
            rgba,
            animation: None,
        }
    }

    #[test]
    fn decodes_png_to_texture_def() {
        let png = solid_png(64, 64, 40);
        let tex = texture_def_from_png(TextureId(1), "stagcrest:stone".into(), &png, None).unwrap();
        assert_eq!(tex.width, 64);
        assert_eq!(tex.height, 64);
        assert_eq!(tex.rgba.len(), 64 * 64 * 4);
    }

    #[test]
    fn packs_many_64x64_textures() {
        let textures: Vec<_> = (0..150)
            .map(|i| mock_texture(i, &format!("t{i}"), 64, 64, (i % 255) as u8))
            .collect();
        let limits = AtlasLimits {
            max_dimension: 8192,
            max_atlases: 8,
        };
        let set = AtlasSet::build(textures, limits).unwrap();
        assert!(!set.pages.is_empty());
        let total_placements: usize = set.pages.iter().map(|p| p.placements.len()).sum();
        assert_eq!(total_placements, 150);
    }

    #[test]
    fn packs_tall_water_strip() {
        let mut textures: Vec<_> = (0..20)
            .map(|i| mock_texture(i, &format!("block{i}"), 64, 64, 10))
            .collect();
        textures.push(mock_texture(20, "water_still", 64, 2048, 80));
        let limits = AtlasLimits {
            max_dimension: 8192,
            max_atlases: 8,
        };
        let set = AtlasSet::build(textures, limits).unwrap();
        let water = set
            .placements
            .iter()
            .find(|p| p.namespaced_id == "water_still")
            .expect("water placement");
        assert_eq!(water.rect.w, 64);
        assert_eq!(water.rect.h, 2048);
    }

    #[test]
    fn rejects_bad_rgba_length() {
        let tex = TextureDef {
            id: TextureId(0),
            namespaced_id: "bad".into(),
            width: 16,
            height: 16,
            rgba: vec![0; 100],
            animation: None,
        };
        let err = AtlasSet::build([tex], AtlasLimits::default()).unwrap_err();
        assert!(matches!(err, AtlasError::RgbaLength { .. }));
    }

    #[test]
    fn fails_when_max_atlases_exceeded() {
        // Each 64x64 texture fills one 64x64 page (minimum page size).
        // With max_atlases=2 and max_dimension=64, the third texture
        // cannot fit and must trigger TooManyAtlases.
        let textures: Vec<_> = (0..3)
            .map(|i| mock_texture(i, &format!("t{i}"), 64, 64, 1))
            .collect();
        let limits = AtlasLimits {
            max_dimension: 64,
            max_atlases: 2,
        };
        let err = AtlasSet::build(textures, limits).unwrap_err();
        assert!(matches!(err, AtlasError::TooManyAtlases { .. }));
    }
}
