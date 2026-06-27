use crate::worldgen::biome::ClimateParams;
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

    /// Sample all six climate axes at a world position.
    pub fn at_3d(&self, wx: i32, y: i32, wz: i32, surface_y: f64) -> ClimateParams {
        let x = wx as f64;
        let z = wz as f64;
        let temp_raw = self.noise.get(TerrainLayer::Temperature).sample2d(
            x * self.config.temperature_frequency,
            z * self.config.temperature_frequency,
        );
        let moist_raw = self.noise.get(TerrainLayer::Moisture).sample2d(
            x * self.config.moisture_frequency,
            z * self.config.moisture_frequency,
        );
        let cont_raw = self.noise.get(TerrainLayer::Continentalness).sample2d(
            x * self.config.continentalness_frequency,
            z * self.config.continentalness_frequency,
        );
        let erosion_raw = self.noise.get(TerrainLayer::Erosion).sample2d(
            x * self.config.erosion_frequency,
            z * self.config.erosion_frequency,
        );
        let pv_raw = self.noise.get(TerrainLayer::PeaksValleys).sample2d(
            x * self.config.peaks_valleys_frequency,
            z * self.config.peaks_valleys_frequency,
        );
        let cave_hum_raw = self.noise.get(TerrainLayer::CaveBiomeHumidity).sample3d(
            x * self.config.cave_biome_humidity_frequency,
            y as f64 * self.config.cave_biome_humidity_frequency,
            z * self.config.cave_biome_humidity_frequency,
        );

        let temperature = ((temp_raw + 1.0) * 0.5 * self.config.temperature_scale as f64) as f32;
        let humidity = ((moist_raw + 1.0) * 0.5) as f32;
        let continentalness = cont_raw as f32;
        let erosion = erosion_raw as f32;
        let weirdness = pv_raw as f32;
        // Depth: normalized depth below surface (0 = surface, 1 = deep underground)
        let depth_below = (surface_y - y as f64).max(0.0);
        let depth = (depth_below / 64.0).clamp(0.0, 1.0) as f32;
        // Cave humidity modulates underground biome selection
        let cave_humidity = ((cave_hum_raw + 1.0) * 0.5) as f32;

        ClimateParams {
            temperature: temperature.clamp(0.0, self.config.temperature_scale),
            humidity: humidity.clamp(0.0, 1.0),
            continentalness,
            erosion,
            depth: if (y as f64) < surface_y - 4.0 {
                depth.max(cave_humidity * 0.5)
            } else {
                depth
            },
            weirdness,
        }
    }

    /// Returns `(temperature, downfall)` for legacy 2D lookups.
    pub fn at(&self, wx: i32, wz: i32) -> (f32, f32) {
        let p = self.at_3d(wx, self.config.sea_level, wz, self.config.sea_level as f64);
        (p.temperature, p.humidity)
    }
}
