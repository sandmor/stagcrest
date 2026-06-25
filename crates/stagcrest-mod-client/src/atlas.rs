use image::{ImageBuffer, RgbaImage};
use stagcrest_protocol::{AtlasRect, TextureDef};

const INITIAL_ATLAS: u32 = 512;
const MAX_ATLAS: u32 = 4096;

#[derive(Debug, Clone)]
pub struct TextureAtlas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub placements: Vec<(stagcrest_protocol::TextureId, AtlasRect)>,
}

impl TextureAtlas {
    pub fn build(textures: impl Iterator<Item = TextureDef>) -> Self {
        let mut textures: Vec<_> = textures.collect();
        textures.sort_by_key(|t| t.id.0);

        let mut atlas_size = INITIAL_ATLAS;
        loop {
            if let Some(result) = try_pack(&textures, atlas_size) {
                return result;
            }
            if atlas_size >= MAX_ATLAS {
                tracing::error!("texture atlas exceeded {MAX_ATLAS}px; some textures may be missing");
                return try_pack(&textures, atlas_size).unwrap_or_else(empty_atlas);
            }
            atlas_size *= 2;
        }
    }
}

fn empty_atlas() -> TextureAtlas {
    TextureAtlas {
        width: INITIAL_ATLAS,
        height: INITIAL_ATLAS,
        pixels: vec![0; (INITIAL_ATLAS * INITIAL_ATLAS * 4) as usize],
        placements: Vec::new(),
    }
}

fn try_pack(textures: &[TextureDef], atlas_size: u32) -> Option<TextureAtlas> {
    let mut img: RgbaImage = ImageBuffer::from_pixel(
        atlas_size,
        atlas_size,
        image::Rgba([0, 0, 0, 0]),
    );
    let mut placements = Vec::new();
    let mut x = 0u32;
    let mut y = 0u32;
    let mut row_h = 0u32;

    for tex in textures {
        let tw = tex.width;
        let th = tex.height;
        if tw == 0 || th == 0 {
            continue;
        }

        if x + tw > atlas_size {
            x = 0;
            y += row_h;
            row_h = 0;
        }
        if y + th > atlas_size {
            return None;
        }

        for py in 0..th {
            for px in 0..tw {
                let i = ((py * tex.width + px) * 4) as usize;
                if i + 3 < tex.rgba.len() {
                    img.put_pixel(
                        x + px,
                        y + py,
                        image::Rgba([
                            tex.rgba[i],
                            tex.rgba[i + 1],
                            tex.rgba[i + 2],
                            tex.rgba[i + 3],
                        ]),
                    );
                }
            }
        }

        let rect = AtlasRect { x, y, w: tw, h: th };
        placements.push((tex.id, rect));
        x += tw;
        row_h = row_h.max(th);
    }

    Some(TextureAtlas {
        width: atlas_size,
        height: atlas_size,
        pixels: img.into_raw(),
        placements,
    })
}
