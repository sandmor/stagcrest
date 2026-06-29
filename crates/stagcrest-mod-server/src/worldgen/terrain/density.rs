use crate::worldgen::caves::CaveSampler;
use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::{TerrainLayer, WorldSeed};
use crate::worldgen::terrain::hydrology::{HydrologySampler, RiverColumn};
use crate::worldgen::terrain::shaping::TerrainShaper;
use crate::worldgen::terrain::sky_islands::SkyIslandSampler;

pub struct DensitySampler<'a> {
    config: &'a TerrainConfig,
    noise: &'a NoiseBank,
    hydrology: HydrologySampler<'a>,
    sky_islands: SkyIslandSampler<'a>,
    caves: CaveSampler<'a>,
}

impl<'a> DensitySampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank, seed: WorldSeed) -> Self {
        Self {
            config,
            noise,
            hydrology: HydrologySampler::new(config, noise),
            sky_islands: SkyIslandSampler::new(config, noise, seed),
            caves: CaveSampler::new(config, noise),
        }
    }

    pub fn surface_y(&self, wx: i32, wz: i32) -> f64 {
        self.hydrology.shaper().surface_y(wx, wz)
    }

    pub fn is_solid(&self, wx: i32, y: i32, wz: i32) -> bool {
        let surface_y = self.surface_y(wx, wz);
        self.is_solid_at_y(wx, y, wz, surface_y)
    }

    pub fn shaper(&self) -> &TerrainShaper<'a> {
        self.hydrology.shaper()
    }

    pub fn hydrology(&self) -> &HydrologySampler<'a> {
        &self.hydrology
    }

    pub fn sky_islands(&self) -> &SkyIslandSampler<'a> {
        &self.sky_islands
    }

    pub fn is_river(&self, wx: i32, wz: i32) -> bool {
        self.hydrology.shaper().is_river(wx, wz)
    }

    pub fn is_riverbank(&self, wx: i32, wz: i32) -> bool {
        self.hydrology.shaper().is_riverbank(wx, wz)
    }

    pub fn river_column(&self, wx: i32, wz: i32) -> Option<RiverColumn> {
        self.hydrology.river_column(wx, wz)
    }

    pub fn river_strength(&self, wx: i32, wz: i32) -> f32 {
        self.hydrology.shaper().river_strength(wx, wz)
    }

    pub fn is_cave_at(&self, wx: i32, y: i32, wz: i32, surface_y: f64) -> bool {
        self.caves.is_cave(wx, y, wz, surface_y)
    }

    pub fn is_solid_at_y(&self, wx: i32, y: i32, wz: i32, surface_y: f64) -> bool {
        if y == self.config.world_min_y {
            return true;
        }

        if self.sky_islands.is_solid(wx, y, wz) {
            return true;
        }

        let terrain_solid = (y as f64) < surface_y;
        if !terrain_solid {
            return false;
        }

        // River channel: open above bed up to terrain surface; water band filled in fill_density.
        if let Some(col) = self.river_column(wx, wz) {
            if y <= col.bed_y {
                return true;
            }
            if y > col.bed_y && (y as f64) < surface_y {
                return false;
            }
        }

        // Caves must not breach into adjacent river water at the same Y (8-neighbor walls).
        if self.adjacent_river_water_y(wx, y, wz) {
            return true;
        }

        !self.caves.is_cave(wx, y, wz, surface_y)
    }

    /// True when a neighbor river column has water at this Y (includes diagonals).
    fn adjacent_river_water_y(&self, wx: i32, y: i32, wz: i32) -> bool {
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            if let Some(col) = self.river_column(wx + dx, wz + dz) {
                if y > col.bed_y && y <= col.water_surface_y {
                    return true;
                }
            }
        }
        false
    }

    /// True when this position is inside a river water column or one block above it.
    pub fn touches_river_channel(&self, wx: i32, y: i32, wz: i32) -> bool {
        if let Some(col) = self.river_column(wx, wz) {
            if y >= col.bed_y && y <= col.water_surface_y + 1 {
                return true;
            }
        }
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            if let Some(col) = self.river_column(wx + dx, wz + dz) {
                if y > col.bed_y && y <= col.water_surface_y + 1 {
                    return true;
                }
            }
        }
        false
    }

    pub fn ore_block(&self, wx: i32, y: i32, wz: i32) -> bool {
        if y <= self.config.world_min_y || y > self.config.ore_max_y {
            return false;
        }
        let n = self.noise.get(TerrainLayer::OreIron).sample3d(
            wx as f64 * self.config.ore_frequency,
            y as f64 * self.config.ore_frequency,
            wz as f64 * self.config.ore_frequency,
        );
        n > self.config.ore_threshold
    }
}
