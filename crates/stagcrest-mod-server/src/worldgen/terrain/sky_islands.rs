use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::TerrainLayer;
use crate::worldgen::seed::WorldSeed;

/// Cohesive floating sky islands using 3D dome-density fields.
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

        let placement = self.noise.get(TerrainLayer::SkyIslandPlacement).sample2d(
            wx as f64 * self.config.island_placement_frequency,
            wz as f64 * self.config.island_placement_frequency,
        );
        if placement <= self.config.island_placement_threshold {
            return false;
        }

        let center_y = self.island_center_y(wx, wz);
        let radius = self.island_radius(wx, wz) as f64;
        let dy = (y - center_y) as f64;

        // Only generate in the band around center
        if dy.abs() > radius * 1.5 {
            return false;
        }

        // Dome density: dense on top, tapering underside
        let horizontal_dist = {
            let hx = (wx % 32) as f64 / 32.0;
            let hz = (wz % 32) as f64 / 32.0;
            ((hx - 0.5).powi(2) + (hz - 0.5).powi(2)).sqrt()
        };

        let dome = if dy >= 0.0 {
            // Upper dome: solid top
            1.0 - (dy / radius).clamp(0.0, 1.0)
        } else {
            // Underside taper
            let t = (-dy / (radius * 0.8)).clamp(0.0, 1.0);
            (1.0 - t).powf(self.config.island_dome_strength)
        };

        let blob = self.noise.get(TerrainLayer::SkyIslandShape).sample3d(
            wx as f64 * self.config.island_blob_frequency,
            (y - center_y) as f64 * self.config.island_blob_frequency,
            wz as f64 * self.config.island_blob_frequency,
        );

        let density = dome * (blob * 0.5 + 0.5) - horizontal_dist * 0.3;
        density > self.config.island_blob_threshold
    }

    pub fn island_surface_y(&self, wx: i32, wz: i32) -> Option<i32> {
        if !self.config.sky_islands_enabled {
            return None;
        }
        let center = self.island_center_y(wx, wz);
        let radius = self.island_radius(wx, wz);
        Some(center + radius / 2)
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
        16 + (h % 16) as i32
    }
}
