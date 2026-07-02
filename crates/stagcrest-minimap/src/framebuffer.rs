use stagcrest_protocol::CHUNK_SIZE;

use crate::color::UNLOADED_COLOR;
use crate::column_cache::ColumnCache;

#[derive(Debug, Default, Clone)]
pub struct DirtyRegion {
    pub full: bool,
    /// Inclusive pixel bounds (px0, pz0, px1, pz1).
    pub rects: Vec<(u32, u32, u32, u32)>,
}

impl DirtyRegion {
    pub fn full() -> Self {
        Self {
            full: true,
            rects: Vec::new(),
        }
    }

    pub fn strip(cx: i32, cz: i32, world_x0: i32, world_z0: i32, bpp: u32, size: u32) -> Self {
        let base_x = cx * CHUNK_SIZE;
        let base_z = cz * CHUNK_SIZE;
        let wx0 = base_x.max(world_x0);
        let wz0 = base_z.max(world_z0);
        let wx1 = (base_x + CHUNK_SIZE - 1).min(world_x0 + size as i32 * bpp as i32 - 1);
        let wz1 = (base_z + CHUNK_SIZE - 1).min(world_z0 + size as i32 * bpp as i32 - 1);
        if wx0 > wx1 || wz0 > wz1 {
            return Self::default();
        }
        let px0 = ((wx0 - world_x0) / bpp as i32) as u32;
        let pz0 = ((wz0 - world_z0) / bpp as i32) as u32;
        let px1 = ((wx1 - world_x0) / bpp as i32) as u32;
        let pz1 = ((wz1 - world_z0) / bpp as i32) as u32;
        Self {
            full: false,
            rects: vec![(px0, pz0, px1, pz1)],
        }
    }

    pub fn edge_x_positive(size: u32, bpp: u32, dx: i32) -> Self {
        let pixels = (dx.abs() as u32).div_ceil(bpp).min(size);
        Self {
            full: false,
            rects: vec![(size - pixels, 0, size - 1, size - 1)],
        }
    }

    pub fn edge_x_negative(size: u32, bpp: u32, dx: i32) -> Self {
        let pixels = (dx.abs() as u32).div_ceil(bpp).min(size);
        Self {
            full: false,
            rects: vec![(0, 0, pixels - 1, size - 1)],
        }
    }

    pub fn edge_z_positive(size: u32, bpp: u32, dz: i32) -> Self {
        let pixels = (dz.abs() as u32).div_ceil(bpp).min(size);
        Self {
            full: false,
            rects: vec![(0, size - pixels, size - 1, size - 1)],
        }
    }

    pub fn edge_z_negative(size: u32, bpp: u32, dz: i32) -> Self {
        let pixels = (dz.abs() as u32).div_ceil(bpp).min(size);
        Self {
            full: false,
            rects: vec![(0, 0, size - 1, pixels - 1)],
        }
    }

    pub fn from_pan(dx: i32, dz: i32, size: u32, bpp: u32) -> Self {
        let mut rects = Vec::new();
        if dx > 0 {
            rects.extend(Self::edge_x_positive(size, bpp, dx).rects);
        } else if dx < 0 {
            rects.extend(Self::edge_x_negative(size, bpp, dx).rects);
        }
        if dz > 0 {
            rects.extend(Self::edge_z_positive(size, bpp, dz).rects);
        } else if dz < 0 {
            rects.extend(Self::edge_z_negative(size, bpp, dz).rects);
        }
        if rects.is_empty() {
            Self::default()
        } else {
            Self { full: false, rects }
        }
    }

    pub fn from_rect(px0: u32, pz0: u32, px1: u32, pz1: u32) -> Self {
        Self {
            full: false,
            rects: vec![(px0, pz0, px1, pz1)],
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.full && self.rects.is_empty()
    }

    pub fn merge(&mut self, other: DirtyRegion) {
        if other.full {
            *self = DirtyRegion::full();
            return;
        }
        if self.full {
            return;
        }
        self.rects.extend(other.rects);
    }
}

pub struct MinimapFramebuffer {
    pub data: Vec<u8>,
    pub size: u32,
    pub center_x: i32,
    pub center_z: i32,
    pub bpp: u32,
}

impl MinimapFramebuffer {
    pub fn new(size: u32, center_x: i32, center_z: i32, bpp: u32) -> Self {
        let mut data = vec![0u8; (size * size * 4) as usize];
        for px in data.chunks_mut(4) {
            px[0] = UNLOADED_COLOR[0];
            px[1] = UNLOADED_COLOR[1];
            px[2] = UNLOADED_COLOR[2];
            px[3] = 255;
        }
        Self {
            data,
            size,
            center_x,
            center_z,
            bpp,
        }
    }

    pub fn world_x0(&self) -> i32 {
        let span = self.size as i32 * self.bpp as i32;
        self.center_x - span / 2
    }

    pub fn world_z0(&self) -> i32 {
        let span = self.size as i32 * self.bpp as i32;
        self.center_z - span / 2
    }

    pub fn set_bpp(&mut self, bpp: u32) {
        self.bpp = bpp;
    }

    /// Shift pixel data when the view center moves; returns dirty edge region.
    /// If movement is smaller than one pixel at the current bpp, returns a full dirty
    /// region so the caller can recomposite from cache while preserving the framebuffer.
    pub fn scroll(&mut self, dx: i32, dz: i32) -> DirtyRegion {
        self.center_x += dx;
        self.center_z += dz;
        let pdx = pixel_shift(dx, self.bpp);
        let pdz = pixel_shift(dz, self.bpp);
        if pdx == 0 && pdz == 0 && (dx != 0 || dz != 0) {
            return DirtyRegion::full();
        }
        if pdx != 0 {
            scroll_rows(&mut self.data, self.size, pdx);
        }
        if pdz != 0 {
            scroll_cols(&mut self.data, self.size, pdz);
        }
        DirtyRegion::from_pan(dx, dz, self.size, self.bpp)
    }

    pub fn composite_full(&mut self, cache: &ColumnCache) {
        self.composite_full_mode(cache, false);
    }

    /// Re-sample the full view; when `preserve_base` is true, pixels with no loaded
    /// cache data keep their current framebuffer color (prior minimap displaced by scroll).
    pub fn composite_full_mode(&mut self, cache: &ColumnCache, preserve_base: bool) {
        let world_x0 = self.world_x0();
        let world_z0 = self.world_z0();
        for pz in 0..self.size {
            for px in 0..self.size {
                composite_pixel(
                    &mut self.data,
                    cache,
                    world_x0,
                    world_z0,
                    px,
                    pz,
                    self.size,
                    self.bpp,
                    preserve_base,
                );
            }
        }
    }

    pub fn composite_region(&mut self, cache: &ColumnCache, region: &DirtyRegion) {
        self.composite_region_mode(cache, region, false);
    }

    pub fn composite_region_mode(
        &mut self,
        cache: &ColumnCache,
        region: &DirtyRegion,
        preserve_base: bool,
    ) {
        if region.full {
            self.composite_full_mode(cache, preserve_base);
            return;
        }
        let world_x0 = self.world_x0();
        let world_z0 = self.world_z0();
        for &(px0, pz0, px1, pz1) in &region.rects {
            for pz in pz0..=pz1.min(self.size - 1) {
                for px in px0..=px1.min(self.size - 1) {
                    composite_pixel(
                        &mut self.data,
                        cache,
                        world_x0,
                        world_z0,
                        px,
                        pz,
                        self.size,
                        self.bpp,
                        preserve_base,
                    );
                }
            }
        }
    }

    pub fn composite_strip(&mut self, cache: &ColumnCache, cx: i32, cz: i32) {
        let region = DirtyRegion::strip(
            cx,
            cz,
            self.world_x0(),
            self.world_z0(),
            self.bpp,
            self.size,
        );
        self.composite_region(cache, &region);
    }
}

fn pixel_shift(block_delta: i32, bpp: u32) -> i32 {
    if block_delta == 0 {
        return 0;
    }
    let sign = block_delta.signum();
    let abs = block_delta.abs() as u32;
    (abs / bpp) as i32 * sign
}

fn scroll_rows(data: &mut [u8], size: u32, pdx: i32) {
    let row_bytes = (size * 4) as usize;
    let shift = (pdx.abs() as u32 * 4) as usize;
    if pdx > 0 {
        for z in 0..size as usize {
            let start = z * row_bytes;
            let row = &mut data[start..start + row_bytes];
            row.copy_within(shift.., 0);
            let clear_from = (size as i32 - pdx.abs()) as u32;
            for x in clear_from..size {
                let i = (x as usize) * 4;
                row[i] = UNLOADED_COLOR[0];
                row[i + 1] = UNLOADED_COLOR[1];
                row[i + 2] = UNLOADED_COLOR[2];
                row[i + 3] = 255;
            }
        }
    } else {
        for z in 0..size as usize {
            let start = z * row_bytes;
            let row = &mut data[start..start + row_bytes];
            row.copy_within(0..row_bytes - shift, shift);
            let clear_start = row_bytes - shift;
            for i in (clear_start..row_bytes).step_by(4) {
                row[i] = UNLOADED_COLOR[0];
                row[i + 1] = UNLOADED_COLOR[1];
                row[i + 2] = UNLOADED_COLOR[2];
                row[i + 3] = 255;
            }
        }
    }
}

fn scroll_cols(data: &mut [u8], size: u32, pdz: i32) {
    let row_bytes = (size * 4) as usize;
    let n = pdz.unsigned_abs() as usize;
    if pdz > 0 {
        data.copy_within(row_bytes * n.., 0);
        for z in 0..n {
            let start = (size as usize - n + z) * row_bytes;
            for i in (start..start + row_bytes).step_by(4) {
                data[i] = UNLOADED_COLOR[0];
                data[i + 1] = UNLOADED_COLOR[1];
                data[i + 2] = UNLOADED_COLOR[2];
                data[i + 3] = 255;
            }
        }
    } else {
        data.copy_within(0..data.len() - row_bytes * n, row_bytes * n);
        for z in 0..n {
            let start = z * row_bytes;
            for i in (start..start + row_bytes).step_by(4) {
                data[i] = UNLOADED_COLOR[0];
                data[i + 1] = UNLOADED_COLOR[1];
                data[i + 2] = UNLOADED_COLOR[2];
                data[i + 3] = 255;
            }
        }
    }
}

fn composite_pixel(
    buf: &mut [u8],
    cache: &ColumnCache,
    world_x0: i32,
    world_z0: i32,
    px: u32,
    pz: u32,
    size: u32,
    bpp: u32,
    preserve_base: bool,
) {
    let out = match sample_pixel(cache, world_x0, world_z0, px, pz, bpp) {
        Some(rgb) => rgb,
        None if preserve_base => read_pixel(buf, px, pz, size),
        None => UNLOADED_COLOR,
    };
    write_pixel(buf, px, pz, size, out);
}

fn read_pixel(buf: &[u8], x: u32, z: u32, width: u32) -> [u8; 3] {
    let i = ((z * width + x) * 4) as usize;
    if i + 2 < buf.len() {
        [buf[i], buf[i + 1], buf[i + 2]]
    } else {
        UNLOADED_COLOR
    }
}

fn sample_pixel(
    cache: &ColumnCache,
    world_x0: i32,
    world_z0: i32,
    px: u32,
    pz: u32,
    bpp: u32,
) -> Option<[u8; 3]> {
    let wx0 = world_x0 + px as i32 * bpp as i32;
    let wz0 = world_z0 + pz as i32 * bpp as i32;
    let mut r_sum = 0u32;
    let mut g_sum = 0u32;
    let mut b_sum = 0u32;
    let mut count = 0u32;
    for dz in 0..bpp {
        for dx in 0..bpp {
            let wx = wx0 + dx as i32;
            let wz = wz0 + dz as i32;
            let Some(entry) = cache.get(wx, wz) else {
                continue;
            };
            if !entry.loaded {
                continue;
            }
            r_sum += entry.rgb[0] as u32;
            g_sum += entry.rgb[1] as u32;
            b_sum += entry.rgb[2] as u32;
            count += 1;
        }
    }
    if count == 0 {
        None
    } else {
        Some([
            (r_sum / count) as u8,
            (g_sum / count) as u8,
            (b_sum / count) as u8,
        ])
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
