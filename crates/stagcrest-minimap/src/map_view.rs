use std::collections::HashMap;

use stagcrest_protocol::floor_div;

use std::collections::HashSet;

use crate::color::UNLOADED_COLOR;
use crate::framebuffer::{DirtyRegion, MinimapFramebuffer};
use crate::map_tile::{map_chunk_block_bounds, MAP_CHUNK_SIZE};
use crate::map_tile_rgb::MapTileRgb;

pub const MINIMAP_TEX_SIZE: u32 = 128;
pub const MINIMAP_PREFETCH_MAP_CHUNKS: i32 = 1;
pub const MINIMAP_ZOOM_LEVELS: [u32; 5] = [1, 2, 4, 8, 16];

/// Expected map tile subscription set for a minimap view at the given center and zoom.
pub fn expected_subscribe_tiles(center_x: i32, center_z: i32, bpp: u32) -> HashSet<(i32, i32)> {
    visible_map_tiles(
        center_x,
        center_z,
        bpp,
        MINIMAP_TEX_SIZE,
        MINIMAP_PREFETCH_MAP_CHUNKS,
    )
    .into_iter()
    .collect()
}

/// Map tiles overlapping a minimap view, with optional prefetch margin in map-chunk units.
pub fn visible_map_tiles(
    center_x: i32,
    center_z: i32,
    bpp: u32,
    tex_size: u32,
    prefetch_chunks: i32,
) -> Vec<(i32, i32)> {
    let span = tex_size as i32 * bpp as i32;
    let half = span / 2;
    let margin = prefetch_chunks * MAP_CHUNK_SIZE;
    let wx0 = center_x - half - margin;
    let wz0 = center_z - half - margin;
    let wx1 = center_x + half - 1 + margin;
    let wz1 = center_z + half - 1 + margin;

    let mx0 = floor_div(wx0, MAP_CHUNK_SIZE);
    let mz0 = floor_div(wz0, MAP_CHUNK_SIZE);
    let mx1 = floor_div(wx1, MAP_CHUNK_SIZE);
    let mz1 = floor_div(wz1, MAP_CHUNK_SIZE);

    let mut out = Vec::new();
    for mz in mz0..=mz1 {
        for mx in mx0..=mx1 {
            out.push((mx, mz));
        }
    }
    out
}

fn div_ceil_i32(a: i32, b: i32) -> i32 {
    if a <= 0 {
        a / b
    } else {
        (a + b - 1) / b
    }
}

/// Inclusive HUD pixel rect `(px0, pz0, px1, pz1)` for map tile `(mx, mz)`, or `None` if off-screen.
/// Each pixel is owned exclusively by the tile whose sample block falls inside the tile bounds.
pub fn map_tile_framebuffer_rect(
    mx: i32,
    mz: i32,
    world_x0: i32,
    world_z0: i32,
    bpp: u32,
    tex_size: u32,
) -> Option<(u32, u32, u32, u32)> {
    let (wx0, wz0, wx1, wz1) = map_chunk_block_bounds(mx, mz);
    let bpp_i = bpp as i32;
    let px0 = div_ceil_i32(wx0 - world_x0, bpp_i).max(0) as u32;
    let px1 = ((wx1 - world_x0) / bpp_i).max(0) as u32;
    let pz0 = div_ceil_i32(wz0 - world_z0, bpp_i).max(0) as u32;
    let pz1 = ((wz1 - world_z0) / bpp_i).max(0) as u32;
    if px0 >= tex_size || pz0 >= tex_size {
        return None;
    }
    let px1 = px1.min(tex_size - 1);
    let pz1 = pz1.min(tex_size - 1);
    if px0 > px1 || pz0 > pz1 {
        return None;
    }
    Some((px0, pz0, px1, pz1))
}

pub fn dirty_region_for_map_tile(
    mx: i32,
    mz: i32,
    world_x0: i32,
    world_z0: i32,
    bpp: u32,
    tex_size: u32,
) -> DirtyRegion {
    match map_tile_framebuffer_rect(mx, mz, world_x0, world_z0, bpp, tex_size) {
        Some((px0, pz0, px1, pz1)) => DirtyRegion::from_rect(px0, pz0, px1, pz1),
        None => DirtyRegion::default(),
    }
}

fn intersect_rect(
    a: (u32, u32, u32, u32),
    b: (u32, u32, u32, u32),
) -> Option<(u32, u32, u32, u32)> {
    let px0 = a.0.max(b.0);
    let pz0 = a.1.max(b.1);
    let px1 = a.2.min(b.2);
    let pz1 = a.3.min(b.3);
    if px0 > px1 || pz0 > pz1 {
        None
    } else {
        Some((px0, pz0, px1, pz1))
    }
}

fn read_framebuffer_pixel(buf: &[u8], x: u32, z: u32, width: u32) -> [u8; 3] {
    let i = ((z * width + x) * 4) as usize;
    if i + 2 < buf.len() {
        [buf[i], buf[i + 1], buf[i + 2]]
    } else {
        UNLOADED_COLOR
    }
}

fn write_framebuffer_pixel(buf: &mut [u8], x: u32, z: u32, width: u32, rgb: [u8; 3]) {
    let i = ((z * width + x) * 4) as usize;
    if i + 3 < buf.len() {
        buf[i] = rgb[0];
        buf[i + 1] = rgb[1];
        buf[i + 2] = rgb[2];
        buf[i + 3] = 255;
    }
}

fn sample_pixel_from_tile(
    tile: &MapTileRgb,
    mx: i32,
    mz: i32,
    world_x0: i32,
    world_z0: i32,
    px: u32,
    pz: u32,
    bpp: u32,
) -> Option<[u8; 3]> {
    let wx = world_x0 + px as i32 * bpp as i32;
    let wz = world_z0 + pz as i32 * bpp as i32;
    let lx = wx - mx * MAP_CHUNK_SIZE;
    let lz = wz - mz * MAP_CHUNK_SIZE;
    tile.sample_at_bpp(lx, lz, bpp)
}

/// Blit one map tile into the framebuffer, intersecting `clip` when not full.
pub fn blit_tile_into_framebuffer(
    tile: &MapTileRgb,
    mx: i32,
    mz: i32,
    framebuffer: &mut MinimapFramebuffer,
    clip: &DirtyRegion,
    preserve_base: bool,
) {
    let world_x0 = framebuffer.world_x0();
    let world_z0 = framebuffer.world_z0();
    let size = framebuffer.size;
    let bpp = framebuffer.bpp;
    let Some(footprint) = map_tile_framebuffer_rect(mx, mz, world_x0, world_z0, bpp, size) else {
        return;
    };

    let rects: Vec<(u32, u32, u32, u32)> = if clip.full {
        vec![footprint]
    } else {
        clip.rects
            .iter()
            .filter_map(|&r| intersect_rect(footprint, r))
            .collect()
    };

    for (px0, pz0, px1, pz1) in rects {
        for pz in pz0..=pz1.min(size - 1) {
            for px in px0..=px1.min(size - 1) {
                let out =
                    match sample_pixel_from_tile(tile, mx, mz, world_x0, world_z0, px, pz, bpp) {
                        Some(rgb) => rgb,
                        None if preserve_base => {
                            read_framebuffer_pixel(&framebuffer.data, px, pz, size)
                        }
                        None => continue,
                    };
                write_framebuffer_pixel(&mut framebuffer.data, px, pz, size, out);
            }
        }
    }
}

/// Composite map tiles into the minimap framebuffer using mip blit.
pub fn composite_tiles_into_framebuffer(
    framebuffer: &mut MinimapFramebuffer,
    tiles: &HashMap<(i32, i32), MapTileRgb>,
    region: &DirtyRegion,
    preserve_base: bool,
) {
    if region.full {
        if !preserve_base {
            for px in framebuffer.data.chunks_mut(4) {
                px[0] = UNLOADED_COLOR[0];
                px[1] = UNLOADED_COLOR[1];
                px[2] = UNLOADED_COLOR[2];
                px[3] = 255;
            }
        }
        for (&(mx, mz), tile) in tiles {
            blit_tile_into_framebuffer(
                tile,
                mx,
                mz,
                framebuffer,
                &DirtyRegion::full(),
                preserve_base,
            );
        }
        return;
    }

    for &(px0, pz0, px1, pz1) in &region.rects {
        let clip = DirtyRegion::from_rect(px0, pz0, px1, pz1);
        let world_x0 = framebuffer.world_x0();
        let world_z0 = framebuffer.world_z0();
        let bpp = framebuffer.bpp;
        let wx0 = world_x0 + px0 as i32 * bpp as i32;
        let wz0 = world_z0 + pz0 as i32 * bpp as i32;
        let wx1 = world_x0 + px1 as i32 * bpp as i32;
        let wz1 = world_z0 + pz1 as i32 * bpp as i32;
        let mx0 = floor_div(wx0, MAP_CHUNK_SIZE);
        let mz0 = floor_div(wz0, MAP_CHUNK_SIZE);
        let mx1 = floor_div(wx1, MAP_CHUNK_SIZE);
        let mz1 = floor_div(wz1, MAP_CHUNK_SIZE);
        for mz in mz0..=mz1 {
            for mx in mx0..=mx1 {
                if let Some(tile) = tiles.get(&(mx, mz)) {
                    blit_tile_into_framebuffer(tile, mx, mz, framebuffer, &clip, preserve_base);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_tile::MAP_CHUNK_RGB_BYTES;
    use crate::map_tile_rgb::build_tile_mips;

    #[test]
    fn visible_tiles_at_origin_zoom_1() {
        let tiles = visible_map_tiles(0, 0, 1, 128, 0);
        assert_eq!(tiles.len(), 4);
        assert!(tiles.contains(&(-1, -1)));
        assert!(tiles.contains(&(0, 0)));
    }

    #[test]
    fn exclusive_footprints_at_bpp16() {
        let center = 32;
        let bpp = 16u32;
        let tex = 128u32;
        let half = tex as i32 * bpp as i32 / 2;
        let world_x0 = center - half;
        let world_z0 = center - half;
        let r0 = map_tile_framebuffer_rect(0, 0, world_x0, world_z0, bpp, tex).unwrap();
        let r1 = map_tile_framebuffer_rect(1, 0, world_x0, world_z0, bpp, tex).unwrap();
        assert_eq!(r0.2 + 1, r1.0, "adjacent tiles must not share HUD pixels");
    }

    #[test]
    fn map_tile_framebuffer_rect_bpp16() {
        let center = 32;
        let bpp = 16u32;
        let tex = 128u32;
        let half = tex as i32 * bpp as i32 / 2;
        let world_x0 = center - half;
        let world_z0 = center - half;
        let rect = map_tile_framebuffer_rect(0, 0, world_x0, world_z0, bpp, tex).unwrap();
        let w = rect.2 - rect.0 + 1;
        let h = rect.3 - rect.1 + 1;
        assert_eq!(w, 4);
        assert_eq!(h, 4);
    }

    #[test]
    fn dirty_region_merge_unions() {
        let mut a = DirtyRegion::from_rect(0, 0, 1, 1);
        a.merge(DirtyRegion::from_rect(2, 2, 3, 3));
        assert_eq!(a.rects.len(), 2);
    }

    #[test]
    fn full_composite_uncached_pixels_stay_stale() {
        let mut fb = MinimapFramebuffer::new(128, 0, 0, 16);
        for px in fb.data.chunks_mut(4) {
            px[0] = 99;
            px[1] = 88;
            px[2] = 77;
            px[3] = 255;
        }
        let mut tiles = HashMap::new();
        let mut base = [0u8; MAP_CHUNK_RGB_BYTES];
        base[0] = 255;
        tiles.insert((0, 0), build_tile_mips(base));
        composite_tiles_into_framebuffer(&mut fb, &tiles, &DirtyRegion::full(), false);
        assert_eq!(
            [fb.data[0], fb.data[1], fb.data[2]],
            UNLOADED_COLOR,
            "corner pixel should be cleared when no tile covers it"
        );
    }

    #[test]
    fn chunk_boundary_neighbor_blit_does_not_erase_pixel() {
        // world_x0=world_z0=8 at bpp=16 → tile mx=0 and mx=1 share HUD pixel px=3
        let mut fb = MinimapFramebuffer::new(128, 1032, 1032, 16);
        assert_eq!(fb.world_x0(), 8);
        assert_eq!(fb.world_z0(), 8);
        let mut red = [0u8; MAP_CHUNK_RGB_BYTES];
        for o in (0..MAP_CHUNK_RGB_BYTES).step_by(3) {
            red[o] = 200;
            red[o + 1] = 10;
            red[o + 2] = 10;
        }
        let mut blue = [0u8; MAP_CHUNK_RGB_BYTES];
        for o in (0..MAP_CHUNK_RGB_BYTES).step_by(3) {
            blue[o] = 10;
            blue[o + 1] = 10;
            blue[o + 2] = 200;
        }
        let red_tile = build_tile_mips(red);
        let blue_tile = build_tile_mips(blue);
        blit_tile_into_framebuffer(&red_tile, 0, 0, &mut fb, &DirtyRegion::full(), false);
        blit_tile_into_framebuffer(&blue_tile, 1, 0, &mut fb, &DirtyRegion::full(), false);
        let i = 3 * 4;
        assert_eq!(
            [fb.data[i], fb.data[i + 1], fb.data[i + 2]],
            [200, 10, 10],
            "neighbor tile must not overwrite boundary pixel with unloaded"
        );
    }

    #[test]
    fn blit_single_tile_fills_rect() {
        let mut base = [0u8; MAP_CHUNK_RGB_BYTES];
        base[0] = 255;
        base[1] = 0;
        base[2] = 0;
        let tile = build_tile_mips(base);
        let mut fb = MinimapFramebuffer::new(128, 32, 32, 1);
        blit_tile_into_framebuffer(&tile, 0, 0, &mut fb, &DirtyRegion::full(), false);
        let px = ((0 - fb.world_x0()) / 1) as u32;
        let i = ((px * fb.size + px) * 4) as usize;
        assert_eq!(fb.data[i], 255);
    }
}
