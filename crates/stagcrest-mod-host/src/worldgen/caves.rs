use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::TerrainLayer;

pub struct CaveSampler<'a> {
    config: &'a TerrainConfig,
    noise: &'a NoiseBank,
}

impl<'a> CaveSampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank) -> Self {
        Self { config, noise }
    }

    pub fn is_cave(&self, wx: i32, y: i32, wz: i32, surface_y: f64) -> bool {
        if y <= self.config.world_min_y + 1 {
            return false;
        }
        if y as f64 >= surface_y - 3.0 {
            return false;
        }

        self.cheese(wx, y, wz) || self.spaghetti(wx, y, wz) || self.noodle(wx, y, wz)
    }

    fn cheese(&self, wx: i32, y: i32, wz: i32) -> bool {
        let n = self.noise.get(TerrainLayer::CaveCheese).sample3d(
            wx as f64 * self.config.cheese_frequency,
            y as f64 * self.config.cheese_frequency,
            wz as f64 * self.config.cheese_frequency,
        );
        n > self.config.cheese_threshold
    }

    fn spaghetti(&self, wx: i32, y: i32, wz: i32) -> bool {
        let wxf = wx as f64 * self.config.spaghetti_frequency;
        let wzf = wz as f64 * self.config.spaghetti_frequency;
        let yf = y as f64 * self.config.spaghetti_vertical_frequency;
        let worm = self.noise.get(TerrainLayer::CaveSpaghetti).sample3d(wxf, yf, wzf);
        let ridge = (worm.abs() - self.config.spaghetti_thickness).max(0.0);
        ridge < self.config.spaghetti_threshold
    }

    fn noodle(&self, wx: i32, y: i32, wz: i32) -> bool {
        let wxf = wx as f64 * self.config.noodle_frequency;
        let wzf = wz as f64 * self.config.noodle_frequency;
        let yf = y as f64 * self.config.noodle_vertical_frequency;
        let worm = self.noise.get(TerrainLayer::CaveNoodle).sample3d(wxf, yf, wzf);
        (worm.abs() - self.config.noodle_thickness).max(0.0) < self.config.noodle_threshold
    }
}
