//! Thin wrapper around [`noise::SuperSimplex`] from the `noise` crate.

use crate::worldgen::seed::{TerrainLayer, WorldSeed};
use noise::{NoiseFn, SuperSimplex};

fn layer_index(layer: TerrainLayer) -> usize {
    match layer {
        TerrainLayer::ElevationLow => 0,
        TerrainLayer::ElevationMid => 1,
        TerrainLayer::ElevationHigh => 2,
        TerrainLayer::Roughness => 3,
        TerrainLayer::Continentalness => 4,
        TerrainLayer::Erosion => 5,
        TerrainLayer::PeaksValleys => 6,
        TerrainLayer::Ridges => 7,
        TerrainLayer::River => 8,
        TerrainLayer::RiverB => 9,
        TerrainLayer::CaveBiomeHumidity => 10,
        TerrainLayer::SkyIslandPlacement => 11,
        TerrainLayer::SkyIslandShape => 12,
        TerrainLayer::Temperature => 13,
        TerrainLayer::Moisture => 14,
        TerrainLayer::CaveCheese => 15,
        TerrainLayer::CaveSpaghetti => 16,
        TerrainLayer::CaveNoodle => 17,
        TerrainLayer::OreIron => 18,
    }
}

#[derive(Clone, Copy)]
pub struct NoiseSource {
    generator: SuperSimplex,
}

impl NoiseSource {
    pub fn from_seed(seed: u32) -> Self {
        Self {
            generator: SuperSimplex::new(seed),
        }
    }

    pub fn sample2d(&self, x: f64, z: f64) -> f64 {
        self.generator.get([x, z])
    }

    pub fn sample3d(&self, x: f64, y: f64, z: f64) -> f64 {
        self.generator.get([x, y, z])
    }
}

#[derive(Clone)]
pub struct NoiseBank {
    sources: [NoiseSource; TerrainLayer::ALL.len()],
}

impl NoiseBank {
    pub fn new(seed: WorldSeed) -> Self {
        let mut sources = [NoiseSource::from_seed(0); TerrainLayer::ALL.len()];
        for layer in TerrainLayer::ALL {
            sources[layer_index(layer)] = NoiseSource::from_seed(seed.layer_seed(layer));
        }
        Self { sources }
    }

    pub fn get(&self, layer: TerrainLayer) -> &NoiseSource {
        &self.sources[layer_index(layer)]
    }

    /// Fractal Brownian motion over 2D noise.
    pub fn fbm2d(
        &self,
        layer: TerrainLayer,
        x: f64,
        z: f64,
        octaves: u32,
        lacunarity: f64,
        gain: f64,
    ) -> f64 {
        let mut amp = 1.0;
        let mut freq = 1.0;
        let mut sum = 0.0;
        let mut norm = 0.0;
        for _ in 0..octaves {
            sum += self.get(layer).sample2d(x * freq, z * freq) * amp;
            norm += amp;
            amp *= gain;
            freq *= lacunarity;
        }
        if norm > 0.0 {
            sum / norm
        } else {
            0.0
        }
    }

    /// Ridged multifractal noise (abs inverted).
    pub fn ridged2d(&self, layer: TerrainLayer, x: f64, z: f64) -> f64 {
        let n = self.get(layer).sample2d(x, z);
        1.0 - n.abs()
    }
}

/// Piecewise-linear spline through control points (x sorted ascending).
pub fn spline_sample(points: &[(f64, f64)], x: f64) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    if x <= points[0].0 {
        return points[0].1;
    }
    if x >= points[points.len() - 1].0 {
        return points[points.len() - 1].1;
    }
    for window in points.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];
        if x >= x0 && x <= x1 {
            let t = if (x1 - x0).abs() < f64::EPSILON {
                0.0
            } else {
                (x - x0) / (x1 - x0)
            };
            return y0 + t * (y1 - y0);
        }
    }
    points[points.len() - 1].1
}
