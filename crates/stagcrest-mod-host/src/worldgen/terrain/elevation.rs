use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::TerrainLayer;

pub struct ElevationSampler<'a> {
    config: &'a TerrainConfig,
    noise: &'a NoiseBank,
}

impl<'a> ElevationSampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank) -> Self {
        Self { config, noise }
    }

    pub fn surface_y(&self, wx: i32, wz: i32) -> f64 {
        let x = wx as f64;
        let z = wz as f64;
        let roughness = self.roughness(x, z);
        let mut elevation = self.config.base_elevation;

        let layers = [
            TerrainLayer::ElevationLow,
            TerrainLayer::ElevationMid,
            TerrainLayer::ElevationHigh,
        ];

        for (layer, octave) in layers.iter().zip(self.config.elevation_octaves) {
            let n = self
                .noise
                .get(*layer)
                .sample2d(x * octave.frequency, z * octave.frequency);
            let amp = if *layer == TerrainLayer::ElevationHigh {
                octave.amplitude * roughness
            } else {
                octave.amplitude
            };
            elevation += n * amp;
        }

        elevation
    }

    fn roughness(&self, x: f64, z: f64) -> f64 {
        let raw = self.noise.get(TerrainLayer::Roughness).sample2d(
            x * self.config.roughness_frequency,
            z * self.config.roughness_frequency,
        );
        let (min, max) = self.config.roughness_range;
        let t = ((raw + 1.0) * 0.5).clamp(0.0, 1.0);
        min + t * (max - min)
    }
}
