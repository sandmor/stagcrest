use crate::map_tile::{MAP_CHUNK_RGB_BYTES, MAP_CHUNK_SIZE};

pub const MIP1_PIXELS: usize = 32 * 32;
pub const MIP2_PIXELS: usize = 16 * 16;
pub const MIP3_PIXELS: usize = 8 * 8;
pub const MIP4_PIXELS: usize = 4 * 4;

pub const MIP1_RGB_BYTES: usize = MIP1_PIXELS * 3;
pub const MIP2_RGB_BYTES: usize = MIP2_PIXELS * 3;
pub const MIP3_RGB_BYTES: usize = MIP3_PIXELS * 3;
pub const MIP4_RGB_BYTES: usize = MIP4_PIXELS * 3;

/// Decoded map tile with pre-built mip pyramid for fast zoomed compositing.
#[derive(Clone)]
pub struct MapTileRgb {
    pub base: [u8; MAP_CHUNK_RGB_BYTES],
    pub mip1: [u8; MIP1_RGB_BYTES],
    pub mip2: [u8; MIP2_RGB_BYTES],
    pub mip3: [u8; MIP3_RGB_BYTES],
    pub mip4: [u8; MIP4_RGB_BYTES],
}

/// Mip level 0..=4 for bpp 1, 2, 4, 8, 16.
pub fn mip_level_from_bpp(bpp: u32) -> u32 {
    match bpp {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        _ => 4,
    }
}

pub fn build_tile_mips(base: [u8; MAP_CHUNK_RGB_BYTES]) -> MapTileRgb {
    let mip1 = downsample_level(&base, MAP_CHUNK_SIZE, MAP_CHUNK_SIZE);
    let mip2 = downsample_level(&mip1, 32, 32);
    let mip3 = downsample_level(&mip2, 16, 16);
    let mip4 = downsample_level(&mip3, 8, 8);
    MapTileRgb {
        base,
        mip1,
        mip2,
        mip3,
        mip4,
    }
}

fn downsample_level<const OUT: usize>(src: &[u8], src_w: i32, src_h: i32) -> [u8; OUT] {
    let dst_w = src_w / 2;
    let dst_h = src_h / 2;
    let mut out = [0u8; OUT];
    for dz in 0..dst_h {
        for dx in 0..dst_w {
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            for oz in 0..2 {
                for ox in 0..2 {
                    let sx = dx * 2 + ox;
                    let sz = dz * 2 + oz;
                    let idx = ((sz * src_w + sx) * 3) as usize;
                    r += src[idx] as u32;
                    g += src[idx + 1] as u32;
                    b += src[idx + 2] as u32;
                }
            }
            let di = ((dz * dst_w + dx) * 3) as usize;
            out[di] = (r / 4) as u8;
            out[di + 1] = (g / 4) as u8;
            out[di + 2] = (b / 4) as u8;
        }
    }
    out
}

impl MapTileRgb {
    /// Sample at local block coords `(lx, lz)` using mip level derived from `bpp`.
    pub fn sample_at_bpp(&self, lx: i32, lz: i32, bpp: u32) -> Option<[u8; 3]> {
        if !(0..MAP_CHUNK_SIZE).contains(&lx) || !(0..MAP_CHUNK_SIZE).contains(&lz) {
            return None;
        }
        let level = mip_level_from_bpp(bpp);
        let stride = 1 << level;
        let mlx = lx / stride;
        let mlz = lz / stride;
        let (w, data): (i32, &[u8]) = match level {
            0 => (MAP_CHUNK_SIZE, &self.base),
            1 => (32, &self.mip1),
            2 => (16, &self.mip2),
            3 => (8, &self.mip3),
            _ => (4, &self.mip4),
        };
        let idx = ((mlz * w + mlx) * 3) as usize;
        if idx + 2 >= data.len() {
            return None;
        }
        Some([data[idx], data[idx + 1], data[idx + 2]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAP_CHUNK_PIXELS;

    #[test]
    fn uniform_tile_mips_match() {
        let mut base = [0u8; MAP_CHUNK_RGB_BYTES];
        for i in 0..MAP_CHUNK_PIXELS {
            let o = i * 3;
            base[o] = 40;
            base[o + 1] = 80;
            base[o + 2] = 120;
        }
        let tile = build_tile_mips(base);
        assert_eq!(tile.sample_at_bpp(0, 0, 1), Some([40, 80, 120]));
        assert_eq!(tile.sample_at_bpp(0, 0, 16), Some([40, 80, 120]));
        assert_eq!(tile.sample_at_bpp(63, 63, 16), Some([40, 80, 120]));
    }

    #[test]
    fn mip_level_from_bpp_values() {
        assert_eq!(mip_level_from_bpp(1), 0);
        assert_eq!(mip_level_from_bpp(16), 4);
    }
}
