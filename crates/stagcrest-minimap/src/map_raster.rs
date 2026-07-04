use stagcrest_protocol::CHUNK_SIZE;

use crate::color::UNLOADED_COLOR;
use crate::column_cache::ColumnCache;
use crate::map_tile::{MAP_CHUNK_RGB_BYTES, MAP_CHUNK_SIZE, WORLD_CHUNKS_PER_MAP};
use crate::resolve::ColumnResolver;
use crate::strip::StorageStripSource;

/// Rasterize a 64×64 map chunk from a 4×4 grid of vertical chunk strips.
pub fn rasterize_map_chunk(
    strips: &[[StorageStripSource; 4]; 4],
    mx: i32,
    mz: i32,
    resolver: &ColumnResolver<'_>,
    biome_at: &impl Fn(i32, i32, i32) -> crate::color::MinimapBiomeClimate,
) -> [u8; MAP_CHUNK_RGB_BYTES] {
    let mut rgb = [0u8; MAP_CHUNK_RGB_BYTES];
    for i in 0..MAP_CHUNK_RGB_BYTES / 3 {
        let o = i * 3;
        rgb[o] = UNLOADED_COLOR[0];
        rgb[o + 1] = UNLOADED_COLOR[1];
        rgb[o + 2] = UNLOADED_COLOR[2];
    }

    let base_cx = mx * WORLD_CHUNKS_PER_MAP;
    let base_cz = mz * WORLD_CHUNKS_PER_MAP;
    let map_x0 = mx * MAP_CHUNK_SIZE;
    let map_z0 = mz * MAP_CHUNK_SIZE;

    for dz in 0..WORLD_CHUNKS_PER_MAP {
        for dx in 0..WORLD_CHUNKS_PER_MAP {
            let cx = base_cx + dx;
            let cz = base_cz + dz;
            let strip = &strips[dz as usize][dx as usize];
            let strip_biome = |wx: i32, wy: i32, wz: i32| biome_at(wx, wy, wz);
            let cache = resolver.resolve_strip(strip, cx, cz, &strip_biome);
            blit_strip_into_map(&cache, cx, cz, map_x0, map_z0, &mut rgb);
        }
    }
    rgb
}

fn blit_strip_into_map(
    cache: &ColumnCache,
    cx: i32,
    cz: i32,
    map_x0: i32,
    map_z0: i32,
    rgb: &mut [u8; MAP_CHUNK_RGB_BYTES],
) {
    let strip_x0 = cx * CHUNK_SIZE;
    let strip_z0 = cz * CHUNK_SIZE;
    for lz in 0..CHUNK_SIZE {
        for lx in 0..CHUNK_SIZE {
            let wx = strip_x0 + lx;
            let wz = strip_z0 + lz;
            let lx_map = wx - map_x0;
            let lz_map = wz - map_z0;
            if !(0..MAP_CHUNK_SIZE).contains(&lx_map) || !(0..MAP_CHUNK_SIZE).contains(&lz_map) {
                continue;
            }
            let pixel = (lz_map * MAP_CHUNK_SIZE + lx_map) as usize;
            let color = match cache.get(wx, wz) {
                Some(e) if e.loaded => e.rgb,
                _ => UNLOADED_COLOR,
            };
            let o = pixel * 3;
            rgb[o] = color[0];
            rgb[o + 1] = color[1];
            rgb[o + 2] = color[2];
        }
    }
}
