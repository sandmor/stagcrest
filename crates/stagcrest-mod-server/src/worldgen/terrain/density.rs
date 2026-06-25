use crate::worldgen::caves::CaveSampler;
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
    caves: CaveSampler<'a>,
}

impl<'a> DensitySampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank, seed: WorldSeed) -> Self {
        Self {
            config,
            noise,
            elevation: ElevationSampler::new(config, noise),
            sky_islands: SkyIslandSampler::new(config, noise, seed),
            caves: CaveSampler::new(config, noise),
        }
    }

    pub fn surface_y(&self, wx: i32, wz: i32) -> f64 {
        self.elevation.surface_y(wx, wz)
    }

    pub fn is_solid(&self, wx: i32, y: i32, wz: i32) -> bool {
        let surface_y = self.elevation.surface_y(wx, wz);
        self.is_solid_at_y(wx, y, wz, surface_y)
    }

    pub fn sky_islands(&self) -> &SkyIslandSampler<'a> {
        &self.sky_islands
    }

    /// Column-based underground solidity, then sky islands, then cave carving.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cave_carving_produces_void_below_surface() {
        let config = TerrainConfig::default();
        let noise = NoiseBank::new(WorldSeed(0xDEAD_BEEF));
        let sampler = DensitySampler::new(&config, &noise, WorldSeed(0xDEAD_BEEF));

        let mut found_cave = false;
        for wx in -64..64 {
            for wz in -64..64 {
                let surface_y = sampler.surface_y(wx, wz);
                if surface_y <= config.world_min_y as f64 + 4.0 {
                    continue;
                }
                for y in (config.world_min_y + 2)..(surface_y as i32 - 4) {
                    if !sampler.is_solid_at_y(wx, y, wz, surface_y) {
                        found_cave = true;
                        break;
                    }
                }
                if found_cave {
                    break;
                }
            }
            if found_cave {
                break;
            }
        }
        assert!(found_cave, "expected CaveSampler to carve at least one void");
    }
}
