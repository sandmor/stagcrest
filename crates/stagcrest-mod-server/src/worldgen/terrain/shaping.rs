use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::{spline_sample, NoiseBank};
use crate::worldgen::seed::TerrainLayer;
use crate::worldgen::terrain::rivers::RiverSampler;

/// Terrain height from continentalness, erosion, and peaks/valleys splines.
pub struct TerrainShaper<'a> {
    config: &'a TerrainConfig,
    noise: &'a NoiseBank,
    rivers: RiverSampler<'a>,
}

impl<'a> TerrainShaper<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank) -> Self {
        Self {
            config,
            noise,
            rivers: RiverSampler::new(config, noise),
        }
    }

    pub fn rivers(&self) -> &RiverSampler<'a> {
        &self.rivers
    }

    pub fn surface_y(&self, wx: i32, wz: i32) -> f64 {
        let x = wx as f64;
        let z = wz as f64;

        let cont = self.noise.get(TerrainLayer::Continentalness).sample2d(
            x * self.config.continentalness_frequency,
            z * self.config.continentalness_frequency,
        );
        let erosion = self.noise.get(TerrainLayer::Erosion).sample2d(
            x * self.config.erosion_frequency,
            z * self.config.erosion_frequency,
        );
        let pv = self.noise.get(TerrainLayer::PeaksValleys).sample2d(
            x * self.config.peaks_valleys_frequency,
            z * self.config.peaks_valleys_frequency,
        );

        // Continentalness spline: ocean basins vs land
        let cont_height = spline_sample(
            &[
                (-1.0, -self.config.continental_ocean_depth),
                (-0.35, -8.0),
                (-0.1, 4.0),
                (0.2, self.config.continental_land_boost * 0.5),
                (0.55, self.config.continental_land_boost),
                (1.0, self.config.continental_land_boost * 0.8),
            ],
            cont,
        );

        // Erosion: high erosion = flatter terrain
        let erosion_mod = spline_sample(
            &[
                (-1.0, self.config.peaks_boost),
                (-0.4, 24.0),
                (0.0, 8.0),
                (0.5, -self.config.erosion_flatten * 0.5),
                (1.0, -self.config.erosion_flatten),
            ],
            erosion,
        );

        // Peaks/valleys weirdness
        let pv_mod = spline_sample(
            &[
                (-1.0, -12.0),
                (0.0, 0.0),
                (0.4, 20.0),
                (1.0, self.config.peaks_boost),
            ],
            pv,
        );

        let roughness = self.roughness(x, z);
        let mut elevation = self.config.base_elevation + cont_height + erosion_mod + pv_mod;

        // Detail octaves
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

        // River valley carving: continuous channel with bed below sea level
        let river_strength = self.rivers.river_strength(wx, wz, erosion as f32) as f64;
        if river_strength > 0.0 {
            let bank = elevation;
            let bed = (self.config.sea_level as f64)
                - 1.0
                - self.config.river_valley_depth * river_strength;
            let carve = (bank - bed).max(0.0);
            elevation = bank - carve * river_strength;
            // Full river columns must sit below the water fill line.
            if river_strength > 0.5 {
                elevation = elevation.min((self.config.sea_level - 1) as f64);
            }
        }

        elevation
    }

    pub fn is_river(&self, wx: i32, wz: i32) -> bool {
        let erosion = self.noise.get(TerrainLayer::Erosion).sample2d(
            wx as f64 * self.config.erosion_frequency,
            wz as f64 * self.config.erosion_frequency,
        ) as f32;
        self.rivers.is_river(wx, wz, erosion)
    }

    pub fn is_riverbank(&self, wx: i32, wz: i32) -> bool {
        let erosion = self.noise.get(TerrainLayer::Erosion).sample2d(
            wx as f64 * self.config.erosion_frequency,
            wz as f64 * self.config.erosion_frequency,
        ) as f32;
        self.rivers.is_riverbank(wx, wz, erosion)
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
