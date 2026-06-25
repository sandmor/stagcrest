//! Thin wrapper around [`noise::SuperSimplex`] from the `noise` crate.

use crate::worldgen::seed::{TerrainLayer, WorldSeed};
use noise::{NoiseFn, SuperSimplex};

fn layer_index(layer: TerrainLayer) -> usize {
    match layer {
        TerrainLayer::ElevationLow => 0,
        TerrainLayer::ElevationMid => 1,
        TerrainLayer::ElevationHigh => 2,
        TerrainLayer::Roughness => 3,
        TerrainLayer::SkyIslandPlacement => 4,
        TerrainLayer::SkyIslandShape => 5,
        TerrainLayer::Temperature => 6,
        TerrainLayer::Moisture => 7,
        TerrainLayer::CaveCheese => 8,
        TerrainLayer::CaveSpaghetti => 9,
        TerrainLayer::CaveNoodle => 10,
        TerrainLayer::OreIron => 11,
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
}
