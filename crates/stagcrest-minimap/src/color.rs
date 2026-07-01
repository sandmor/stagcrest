use stagcrest_protocol::{
    manifest::{BIOME_GRID_SIZE, BIOME_GRID_VOLUME},
    BlockDef, BlockGeometry, BlockId, TintKind, CHUNK_SIZE,
};

/// RGBA for columns with no loaded chunk data.
pub const UNLOADED_COLOR: [u8; 3] = [32, 32, 36];

/// Plains-like climate when biome data is unavailable.
const DEFAULT_TEMP: f32 = 0.8;
const DEFAULT_DOWNFALL: f32 = 0.4;

#[derive(Debug, Clone, Copy)]
pub struct MinimapBiomeClimate {
    pub temperature: f32,
    pub downfall: f32,
    pub grass_color: Option<[u8; 3]>,
    pub foliage_color: Option<[u8; 3]>,
    pub water_color: Option<[u8; 3]>,
}

impl Default for MinimapBiomeClimate {
    fn default() -> Self {
        Self {
            temperature: DEFAULT_TEMP,
            downfall: DEFAULT_DOWNFALL,
            grass_color: None,
            foliage_color: None,
            water_color: None,
        }
    }
}

pub struct MinimapColorCtx<'a> {
    pub grass: &'a [u8],
    pub grass_w: u32,
    pub grass_h: u32,
    pub foliage: &'a [u8],
    pub foliage_w: u32,
    pub foliage_h: u32,
    pub water: &'a [u8],
    pub water_w: u32,
    pub water_h: u32,
}

/// Owned colormap payload for background minimap builds.
#[derive(Clone)]
pub struct MinimapColorCtxOwned {
    pub grass: Vec<u8>,
    pub grass_w: u32,
    pub grass_h: u32,
    pub foliage: Vec<u8>,
    pub foliage_w: u32,
    pub foliage_h: u32,
    pub water: Vec<u8>,
    pub water_w: u32,
    pub water_h: u32,
}

impl MinimapColorCtxOwned {
    pub fn new(
        grass: Vec<u8>,
        grass_w: u32,
        grass_h: u32,
        foliage: Vec<u8>,
        foliage_w: u32,
        foliage_h: u32,
        water: Vec<u8>,
        water_w: u32,
        water_h: u32,
    ) -> Self {
        Self {
            grass,
            grass_w,
            grass_h,
            foliage,
            foliage_w,
            foliage_h,
            water,
            water_w,
            water_h,
        }
    }

    pub fn as_ctx(&self) -> MinimapColorCtx<'_> {
        MinimapColorCtx {
            grass: &self.grass,
            grass_w: self.grass_w,
            grass_h: self.grass_h,
            foliage: &self.foliage,
            foliage_w: self.foliage_w,
            foliage_h: self.foliage_h,
            water: &self.water,
            water_w: self.water_w,
            water_h: self.water_h,
        }
    }
}

fn mul_rgb(base: [u8; 3], mul: [f32; 3]) -> [u8; 3] {
    [
        (base[0] as f32 * mul[0]).clamp(0.0, 255.0) as u8,
        (base[1] as f32 * mul[1]).clamp(0.0, 255.0) as u8,
        (base[2] as f32 * mul[2]).clamp(0.0, 255.0) as u8,
    ]
}

fn rgb_f32(c: [u8; 3]) -> [f32; 3] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    ]
}

pub fn resolve_column_color(
    def: &BlockDef,
    biome: MinimapBiomeClimate,
    ctx: &MinimapColorCtx<'_>,
) -> [u8; 3] {
    let base = def.map_color;
    let tint = def.face_textures.top.tint;
    let mul = match tint {
        TintKind::Grass => biome.grass_color.map(rgb_f32).unwrap_or_else(|| {
            stagcrest_protocol::sample_colormap_rgb(
                ctx.grass,
                ctx.grass_w,
                ctx.grass_h,
                biome.temperature,
                biome.downfall,
            )
        }),
        TintKind::Foliage => biome.foliage_color.map(rgb_f32).unwrap_or_else(|| {
            stagcrest_protocol::sample_colormap_rgb(
                ctx.foliage,
                ctx.foliage_w,
                ctx.foliage_h,
                biome.temperature,
                biome.downfall,
            )
        }),
        TintKind::Water => biome.water_color.map(rgb_f32).unwrap_or_else(|| {
            stagcrest_protocol::sample_colormap_rgb(
                ctx.water,
                ctx.water_w,
                ctx.water_h,
                biome.temperature,
                biome.downfall,
            )
        }),
        _ => return base,
    };
    mul_rgb(base, mul)
}

/// Biome index at a world block from a chunk's 4×4×4 biome grid.
pub fn biome_index_at(
    grid: &[u8; BIOME_GRID_VOLUME],
    block_pos_x: i32,
    block_pos_y: i32,
    block_pos_z: i32,
    chunk_origin_x: i32,
    chunk_origin_y: i32,
    chunk_origin_z: i32,
) -> u16 {
    let lx = block_pos_x - chunk_origin_x;
    let ly = block_pos_y - chunk_origin_y;
    let lz = block_pos_z - chunk_origin_z;
    let cell = CHUNK_SIZE / BIOME_GRID_SIZE as i32;
    let gx = (lx / cell).clamp(0, 3) as usize;
    let gy = (ly / cell).clamp(0, 3) as usize;
    let gz = (lz / cell).clamp(0, 3) as usize;
    let idx = gx + gy * 4 + gz * 16;
    grid[idx] as u16
}

pub fn is_map_visible(def: &BlockDef, air: BlockId, id: BlockId) -> bool {
    if id == air {
        return false;
    }
    if def.fluid {
        return true;
    }
    if !def.solid {
        return false;
    }
    match def.geometry {
        BlockGeometry::Cross | BlockGeometry::Flat => false,
        _ => true,
    }
}

pub fn is_surface_plant(def: &BlockDef) -> bool {
    matches!(def.geometry, BlockGeometry::Cross | BlockGeometry::Flat) && !def.fluid
}
