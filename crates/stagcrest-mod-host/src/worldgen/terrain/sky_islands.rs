use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::WorldSeed;

pub struct SkyIslandSampler<'a> {
    config: &'a TerrainConfig,
    noise: &'a NoiseBank,
    seed: WorldSeed,
}

impl<'a> SkyIslandSampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank, seed: WorldSeed) -> Self {
        Self {
            config,
            noise,
            seed,
        }
    }

    pub fn is_solid(&self, wx: i32, y: i32, wz: i32) -> bool {
        if !self.config.sky_islands_enabled {
            return false;
        }

        let placement = self
            .noise
            .get(crate::worldgen::seed::TerrainLayer::SkyIslandPlacement)
            .sample2d(
                wx as f64 * self.config.island_placement_frequency,
                wz as f64 * self.config.island_placement_frequency,
            );
        if placement <= self.config.island_placement_threshold {
            return false;
        }

        let center_y = self.island_center_y(wx, wz);
        let radius = self.island_radius(wx, wz);
        let dy = (y - center_y).abs();
        if dy > radius {
            return false;
        }

        let blob = self
            .noise
            .get(crate::worldgen::seed::TerrainLayer::SkyIslandShape)
            .sample3d(
                (wx - center_y / 4) as f64 * self.config.island_blob_frequency,
                (y - center_y) as f64 * self.config.island_blob_frequency,
                (wz + center_y / 3) as f64 * self.config.island_blob_frequency,
            );

        blob > self.config.island_blob_threshold
    }

    fn island_center_y(&self, wx: i32, wz: i32) -> i32 {
        let h = self
            .seed
            .0
            .wrapping_add(wx as u64)
            .wrapping_mul(0x517C_C1B7_2722_0A95)
            .wrapping_add(wz as u64)
            .wrapping_mul(0x6C07_8965_E59B_D9AD);
        let span = (self.config.island_max_y - self.config.island_min_y).max(1) as u64;
        self.config.island_min_y + (h % span) as i32
    }

    fn island_radius(&self, wx: i32, wz: i32) -> i32 {
        let h = self
            .seed
            .0
            .wrapping_add((wx as u64).wrapping_mul(3))
            .wrapping_add((wz as u64).wrapping_mul(7));
        let span = 9i32;
        8 + (h % span as u64) as i32
    }
}
