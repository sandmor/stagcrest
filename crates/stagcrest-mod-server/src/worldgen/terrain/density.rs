use crate::worldgen::caves::CaveSampler;
use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::{TerrainLayer, WorldSeed};
use crate::worldgen::terrain::shaping::TerrainShaper;
use crate::worldgen::terrain::sky_islands::SkyIslandSampler;

pub struct DensitySampler<'a> {
    config: &'a TerrainConfig,
    noise: &'a NoiseBank,
    shaper: TerrainShaper<'a>,
    sky_islands: SkyIslandSampler<'a>,
    caves: CaveSampler<'a>,
}

impl<'a> DensitySampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank, seed: WorldSeed) -> Self {
        Self {
            config,
            noise,
            shaper: TerrainShaper::new(config, noise),
            sky_islands: SkyIslandSampler::new(config, noise, seed),
            caves: CaveSampler::new(config, noise),
        }
    }

    pub fn surface_y(&self, wx: i32, wz: i32) -> f64 {
        self.shaper.surface_y(wx, wz)
    }

    pub fn is_solid(&self, wx: i32, y: i32, wz: i32) -> bool {
        let surface_y = self.shaper.surface_y(wx, wz);
        self.is_solid_at_y(wx, y, wz, surface_y)
    }

    pub fn shaper(&self) -> &TerrainShaper<'a> {
        &self.shaper
    }

    pub fn sky_islands(&self) -> &SkyIslandSampler<'a> {
        &self.sky_islands
    }

    pub fn is_river(&self, wx: i32, wz: i32) -> bool {
        self.shaper.is_river(wx, wz)
    }

    pub fn is_riverbank(&self, wx: i32, wz: i32) -> bool {
        self.shaper.is_riverbank(wx, wz)
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

        !self.caves.is_cave(wx, y, wz, surface_y)
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
