use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::{TerrainLayer, WorldSeed};
use crate::worldgen::terrain::elevation::ElevationSampler;
use crate::worldgen::terrain::sky_islands::SkyIslandSampler;

pub struct DensitySampler<'a> {
    config: &'a TerrainConfig,
    noise: &'a NoiseBank,
    elevation: ElevationSampler<'a>,
    sky_islands: SkyIslandSampler<'a>,
}

impl<'a> DensitySampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank, seed: WorldSeed) -> Self {
        Self {
            config,
            noise,
            elevation: ElevationSampler::new(config, noise),
            sky_islands: SkyIslandSampler::new(config, noise, seed),
        }
    }

    pub fn surface_y(&self, wx: i32, wz: i32) -> f64 {
        self.elevation.surface_y(wx, wz)
    }

    pub fn is_solid(&self, wx: i32, y: i32, wz: i32) -> bool {
        let surface_y = self.elevation.surface_y(wx, wz);
        self.is_solid_at_y(wx, y, wz, surface_y)
    }

    pub fn is_solid_at_y(&self, wx: i32, y: i32, wz: i32, surface_y: f64) -> bool {
        if y == self.config.world_min_y {
            return true;
        }

        let raw = self.noise.get(TerrainLayer::Density).sample3d(
            wx as f64 * self.config.cave_frequency,
            y as f64 * self.config.cave_frequency,
            wz as f64 * self.config.cave_frequency,
        );
        let bias = (surface_y - y as f64) * self.config.gradient_strength;
        let base = raw + bias > self.config.density_threshold;

        base || self.sky_islands.is_solid(wx, y, wz)
    }
}
