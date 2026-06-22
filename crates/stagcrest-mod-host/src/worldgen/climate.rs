use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::TerrainLayer;

pub struct ClimateSampler<'a> {
    config: &'a TerrainConfig,
    noise: &'a NoiseBank,
}

impl<'a> ClimateSampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank) -> Self {
        Self { config, noise }
    }

    /// Returns `(temperature, downfall)` in roughly vanilla ranges (temp may exceed 1.0).
    pub fn at(&self, wx: i32, wz: i32) -> (f32, f32) {
        let temp_raw = self
            .noise
            .get(TerrainLayer::Temperature)
            .sample2d(
                wx as f64 * self.config.temperature_frequency,
                wz as f64 * self.config.temperature_frequency,
            );
        let moist_raw = self
            .noise
            .get(TerrainLayer::Moisture)
            .sample2d(
                wx as f64 * self.config.moisture_frequency,
                wz as f64 * self.config.moisture_frequency,
            );
        let temperature =
            ((temp_raw + 1.0) * 0.5 * self.config.temperature_scale as f64) as f32;
        let downfall = ((moist_raw + 1.0) * 0.5) as f32;
        (temperature.clamp(0.0, self.config.temperature_scale), downfall.clamp(0.0, 1.0))
    }
}
