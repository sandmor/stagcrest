use stagcrest_protocol::CHUNK_SIZE;

use crate::color::UNLOADED_COLOR;
use crate::column_cache::ColumnCache;

/// Write one horizontal chunk strip into a world-map RGBA buffer (bpp = 1).
pub fn write_strip_to_rgba(
    cache: &ColumnCache,
    cx: i32,
    cz: i32,
    output: &mut [u8],
    img_width: u32,
    img_height: u32,
    world_x0: i32,
    world_z0: i32,
) {
    let base_x = cx * CHUNK_SIZE;
    let base_z = cz * CHUNK_SIZE;
    for lz in 0..CHUNK_SIZE {
        for lx in 0..CHUNK_SIZE {
            let wx = base_x + lx;
            let wz = base_z + lz;
            if wx < world_x0
                || wz < world_z0
                || wx >= world_x0 + img_width as i32
                || wz >= world_z0 + img_height as i32
            {
                continue;
            }
            let px = (wx - world_x0) as u32;
            let pz = (wz - world_z0) as u32;
            let rgb = match cache.get(wx, wz) {
                Some(e) if e.loaded => e.rgb,
                _ => UNLOADED_COLOR,
            };
            write_pixel(output, px, pz, img_width, rgb);
        }
    }
}

fn write_pixel(buf: &mut [u8], x: u32, z: u32, width: u32, rgb: [u8; 3]) {
    let i = ((z * width + x) * 4) as usize;
    if i + 3 < buf.len() {
        buf[i] = rgb[0];
        buf[i + 1] = rgb[1];
        buf[i + 2] = rgb[2];
        buf[i + 3] = 255;
    }
}
