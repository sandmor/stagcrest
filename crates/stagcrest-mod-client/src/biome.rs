use stagcrest_protocol::manifest::{
    BiomeClientDef, BiomesSnapshot, BIOME_GRID_SIZE, BIOME_GRID_VOLUME,
};
use stagcrest_protocol::{BlockPos, ChunkPos, CHUNK_SIZE};

/// Client-side biome data for environment interpolation.
#[derive(Debug, Clone, Default)]
pub struct BiomeRegistryClient {
    pub biomes: Vec<BiomeClientDef>,
}

impl BiomeRegistryClient {
    pub fn from_snapshot(snap: BiomesSnapshot) -> Self {
        Self {
            biomes: snap.biomes,
        }
    }

    pub fn biome_by_index(&self, index: u16) -> Option<&BiomeClientDef> {
        self.biomes.iter().find(|b| b.index == index)
    }

    /// Sample biome environment at a world block position using chunk biome grid.
    pub fn sample_at(
        &self,
        pos: BlockPos,
        grid: Option<&[u8; BIOME_GRID_VOLUME]>,
    ) -> Option<&BiomeClientDef> {
        let grid = grid?;
        let lx = pos.x.rem_euclid(CHUNK_SIZE);
        let ly = pos.y.rem_euclid(CHUNK_SIZE);
        let lz = pos.z.rem_euclid(CHUNK_SIZE);
        let gx = (lx / (CHUNK_SIZE / BIOME_GRID_SIZE as i32)).clamp(0, 3) as usize;
        let gy = (ly / (CHUNK_SIZE / BIOME_GRID_SIZE as i32)).clamp(0, 3) as usize;
        let gz = (lz / (CHUNK_SIZE / BIOME_GRID_SIZE as i32)).clamp(0, 3) as usize;
        let idx = gx + gy * 4 + gz * 16;
        let biome_idx = grid[idx] as u16;
        self.biome_by_index(biome_idx)
    }

    /// Interpolate environment from nearby sample points.
    pub fn interpolate_at<F>(&self, pos: BlockPos, grids: F) -> InterpolatedEnvironment
    where
        F: Fn(ChunkPos) -> Option<[u8; BIOME_GRID_VOLUME]>,
    {
        let chunk_pos = pos.chunk_pos();
        let grid = grids(chunk_pos);
        let biome = grid.and_then(|g| self.sample_at(pos, Some(&g)));
        let default = InterpolatedEnvironment::default();
        let Some(b) = biome else {
            return default;
        };
        InterpolatedEnvironment {
            fog_color: rgb_f32(b.fog_color),
            fog_density: b.fog_density,
            water_color: rgb_f32(b.water_color),
            water_fog_color: rgb_f32(b.water_fog_color),
            sky_color: rgb_f32(b.sky_color),
            grass_color: b.grass_color.map(rgb_f32),
            foliage_color: b.foliage_color.map(rgb_f32),
            temperature: b.temperature,
            downfall: b.downfall,
        }
    }
}

fn rgb_f32(c: [u8; 3]) -> [f32; 3] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    ]
}

#[derive(Debug, Clone)]
pub struct InterpolatedEnvironment {
    pub fog_color: [f32; 3],
    pub fog_density: f32,
    pub water_color: [f32; 3],
    pub water_fog_color: [f32; 3],
    pub sky_color: [f32; 3],
    pub grass_color: Option<[f32; 3]>,
    pub foliage_color: Option<[f32; 3]>,
    pub temperature: f32,
    pub downfall: f32,
}

impl Default for InterpolatedEnvironment {
    fn default() -> Self {
        Self {
            fog_color: [0.75, 0.85, 1.0],
            fog_density: 0.01,
            water_color: [0.25, 0.45, 0.9],
            water_fog_color: [0.02, 0.17, 0.32],
            sky_color: [0.47, 0.65, 1.0],
            grass_color: None,
            foliage_color: None,
            temperature: 0.8,
            downfall: 0.4,
        }
    }
}
