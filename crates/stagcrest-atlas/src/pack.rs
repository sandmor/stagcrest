use crate::downscale::downscale_texture;
use crate::AtlasError;
use etagere::{BucketedAtlasAllocator, Size};
use image::{ImageBuffer, RgbaImage};
use stagcrest_protocol::{AtlasRect, TextureDef, TextureId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct AtlasLimits {
    pub max_dimension: u32,
    pub max_atlases: u32,
}

impl Default for AtlasLimits {
    fn default() -> Self {
        Self {
            max_dimension: 8192,
            max_atlases: 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Placement {
    pub id: TextureId,
    pub namespaced_id: String,
    pub atlas_index: u8,
    pub rect: AtlasRect,
}

#[derive(Debug, Clone)]
pub struct AtlasPage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub placements: Vec<(TextureId, AtlasRect)>,
}

#[derive(Debug, Clone)]
pub struct AtlasSet {
    pub pages: Vec<AtlasPage>,
    pub placements: Vec<Placement>,
}

struct PageState {
    width: u32,
    height: u32,
    placements: Vec<(TextureId, AtlasRect)>,
}

impl AtlasSet {
    pub fn build(
        textures: impl IntoIterator<Item = TextureDef>,
        limits: AtlasLimits,
    ) -> Result<Self, AtlasError> {
        let max_dim = limits.max_dimension.max(1);
        let max_atlases = limits.max_atlases.max(1);

        let mut by_id: HashMap<TextureId, TextureDef> = HashMap::new();
        for tex in textures {
            validate_texture(&tex)?;
            let def = downscale_texture(&tex, max_dim);
            validate_texture(&def)?;
            by_id.insert(def.id, def);
        }

        let mut ordered: Vec<TextureDef> = by_id.into_values().collect();
        ordered.sort_by_key(|t| t.id.0);

        let texture_map: HashMap<TextureId, TextureDef> =
            ordered.iter().map(|t| (t.id, t.clone())).collect();

        let mut page_states: Vec<PageState> = Vec::new();
        let mut all_placements: Vec<Placement> = Vec::new();

        for tex in &ordered {
            let (page_idx, rect) = place_texture(
                &mut page_states,
                tex.id,
                tex.width,
                tex.height,
                max_dim,
                max_atlases,
                &tex.namespaced_id,
            )?;
            page_states[page_idx as usize]
                .placements
                .push((tex.id, rect));
            all_placements.push(Placement {
                id: tex.id,
                namespaced_id: tex.namespaced_id.clone(),
                atlas_index: page_idx,
                rect,
            });
        }

        let pages: Vec<AtlasPage> = page_states
            .into_iter()
            .enumerate()
            .map(|(idx, state)| build_page_pixels(idx as u8, state, &texture_map))
            .collect();

        if pages.is_empty() {
            return Err(AtlasError::PackFailed {
                name: "(none)".into(),
                w: 0,
                h: 0,
                reason: "no textures to pack".into(),
            });
        }

        Ok(AtlasSet {
            pages,
            placements: all_placements,
        })
    }
}

fn validate_texture(tex: &TextureDef) -> Result<(), AtlasError> {
    if tex.width == 0 || tex.height == 0 {
        return Err(AtlasError::ZeroDimensions {
            name: tex.namespaced_id.clone(),
        });
    }
    let expected = (tex.width as usize)
        .checked_mul(tex.height as usize)
        .and_then(|p| p.checked_mul(4))
        .unwrap_or(0);
    if tex.rgba.len() != expected {
        return Err(AtlasError::RgbaLength {
            name: tex.namespaced_id.clone(),
            actual: tex.rgba.len(),
            expected,
        });
    }
    Ok(())
}

fn place_texture(
    pages: &mut Vec<PageState>,
    _id: TextureId,
    tw: u32,
    th: u32,
    max_dim: u32,
    max_atlases: u32,
    name: &str,
) -> Result<(u8, AtlasRect), AtlasError> {
    if tw > max_dim || th > max_dim {
        return Err(AtlasError::PackFailed {
            name: name.to_string(),
            w: tw,
            h: th,
            reason: format!("texture exceeds max_dimension {max_dim} after downscale"),
        });
    }

    for (idx, page) in pages.iter_mut().enumerate() {
        if let Some(rect) = try_fit(page, tw, th, max_dim, idx as u8) {
            return Ok((idx as u8, rect));
        }
    }

    if pages.len() as u32 >= max_atlases {
        return Err(AtlasError::TooManyAtlases { max_atlases });
    }

    let (pw, ph) = initial_page_size(tw, th, max_dim);
    let mut page = PageState {
        width: pw,
        height: ph,
        placements: Vec::new(),
    };
    let rect = try_fit(&mut page, tw, th, max_dim, pages.len() as u8).ok_or_else(|| {
        AtlasError::PackFailed {
            name: name.to_string(),
            w: tw,
            h: th,
            reason: "does not fit on a fresh atlas page".into(),
        }
    })?;
    pages.push(page);
    Ok(((pages.len() - 1) as u8, rect))
}

fn try_fit(
    page: &mut PageState,
    tw: u32,
    th: u32,
    max_dim: u32,
    atlas_index: u8,
) -> Option<AtlasRect> {
    let mut pw = page.width;
    let mut ph = page.height;
    loop {
        if let Some(rect) = alloc_rect(pw, ph, &page.placements, tw, th, atlas_index) {
            page.width = pw;
            page.height = ph;
            return Some(rect);
        }
        if pw >= max_dim && ph >= max_dim {
            return None;
        }
        if pw <= ph && pw < max_dim {
            pw = (pw * 2).min(max_dim);
        } else if ph < max_dim {
            ph = (ph * 2).min(max_dim);
        } else if pw < max_dim {
            pw = (pw * 2).min(max_dim);
        } else {
            return None;
        }
    }
}

fn alloc_rect(
    pw: u32,
    ph: u32,
    existing: &[(TextureId, AtlasRect)],
    tw: u32,
    th: u32,
    atlas_index: u8,
) -> Option<AtlasRect> {
    let pw = i32::try_from(pw).unwrap_or(i32::MAX);
    let ph = i32::try_from(ph).unwrap_or(i32::MAX);
    let mut alloc = BucketedAtlasAllocator::new(Size::new(pw, ph));
    for (_, r) in existing {
        let rw = i32::try_from(r.w).unwrap_or(i32::MAX);
        let rh = i32::try_from(r.h).unwrap_or(i32::MAX);
        alloc.allocate(Size::new(rw, rh))?;
    }
    let tw_i = i32::try_from(tw).unwrap_or(i32::MAX);
    let th_i = i32::try_from(th).unwrap_or(i32::MAX);
    let a = alloc.allocate(Size::new(tw_i, th_i))?;
    Some(AtlasRect {
        atlas_index,
        x: a.rectangle.min.x as u32,
        y: a.rectangle.min.y as u32,
        w: tw,
        h: th,
    })
}

fn initial_page_size(tw: u32, th: u32, max_dim: u32) -> (u32, u32) {
    let pw = next_pow2(tw).clamp(64, max_dim);
    let ph = next_pow2(th).clamp(64, max_dim);
    (pw, ph)
}

fn next_pow2(v: u32) -> u32 {
    if v <= 1 {
        return 1;
    }
    1u32 << (32 - (v - 1).leading_zeros())
}

fn build_page_pixels(
    atlas_index: u8,
    state: PageState,
    textures: &HashMap<TextureId, TextureDef>,
) -> AtlasPage {
    let pixels = vec![0u8; (state.width * state.height * 4) as usize];
    let mut img: RgbaImage =
        ImageBuffer::from_raw(state.width, state.height, pixels).expect("page buffer");
    let mut placements = Vec::new();
    for (id, mut rect) in state.placements {
        rect.atlas_index = atlas_index;
        if let Some(tex) = textures.get(&id) {
            blit(&mut img, tex, &rect);
        }
        placements.push((id, rect));
    }
    AtlasPage {
        width: state.width,
        height: state.height,
        pixels: img.into_raw(),
        placements,
    }
}

fn blit(img: &mut RgbaImage, tex: &TextureDef, rect: &AtlasRect) {
    for py in 0..tex.height {
        for px in 0..tex.width {
            let i = ((py * tex.width + px) * 4) as usize;
            if i + 3 < tex.rgba.len() {
                img.put_pixel(
                    rect.x + px,
                    rect.y + py,
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
}
