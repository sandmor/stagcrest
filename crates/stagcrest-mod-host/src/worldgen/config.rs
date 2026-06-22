use stagcrest_protocol::CHUNK_SIZE;
use std::ops::RangeInclusive;

pub const SEA_LEVEL: i32 = 64;

#[derive(Debug, Clone, Copy)]
pub struct Octave {
    pub frequency: f64,
    pub amplitude: f64,
}

#[derive(Debug, Clone)]
pub struct TerrainConfig {
    pub sea_level: i32,
    pub world_min_y: i32,
    pub world_max_y: i32,
    pub base_elevation: f64,
    pub elevation_octaves: [Octave; 3],
    pub roughness_frequency: f64,
    pub roughness_range: (f64, f64),
    pub sky_islands_enabled: bool,
    pub island_placement_frequency: f64,
    pub island_placement_threshold: f64,
    pub island_min_y: i32,
    pub island_max_y: i32,
    pub island_blob_frequency: f64,
    pub island_blob_threshold: f64,
    pub island_vertical_radius: i32,
    pub temperature_frequency: f64,
    pub moisture_frequency: f64,
    pub temperature_scale: f32,
    pub cheese_frequency: f64,
    pub cheese_threshold: f64,
    pub spaghetti_frequency: f64,
    pub spaghetti_vertical_frequency: f64,
    pub spaghetti_thickness: f64,
    pub spaghetti_threshold: f64,
    pub noodle_frequency: f64,
    pub noodle_vertical_frequency: f64,
    pub noodle_thickness: f64,
    pub noodle_threshold: f64,
    pub ore_frequency: f64,
    pub ore_threshold: f64,
    pub ore_max_y: i32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            sea_level: SEA_LEVEL,
            world_min_y: 0,
            world_max_y: 160,
            base_elevation: 72.0,
            elevation_octaves: [
                Octave {
                    frequency: 0.001,
                    amplitude: 48.0,
                },
                Octave {
                    frequency: 0.005,
                    amplitude: 24.0,
                },
                Octave {
                    frequency: 0.025,
                    amplitude: 8.0,
                },
            ],
            roughness_frequency: 0.002,
            roughness_range: (0.15, 1.0),
            sky_islands_enabled: false,
            island_placement_frequency: 0.003,
            // Lower than 0.65 so more sky islands spawn.
            island_placement_threshold: 0.55,
            island_min_y: 100,
            island_max_y: 140,
            island_blob_frequency: 0.08,
            island_blob_threshold: 0.35,
            island_vertical_radius: 12,
            temperature_frequency: 0.0015,
            moisture_frequency: 0.0015,
            temperature_scale: 2.5,
            cheese_frequency: 0.015,
            cheese_threshold: 0.55,
            spaghetti_frequency: 0.02,
            spaghetti_vertical_frequency: 0.04,
            spaghetti_thickness: 0.08,
            spaghetti_threshold: 0.02,
            noodle_frequency: 0.04,
            noodle_vertical_frequency: 0.06,
            noodle_thickness: 0.04,
            noodle_threshold: 0.015,
            ore_frequency: 0.08,
            ore_threshold: 0.15,
            ore_max_y: 48,
        }
    }
}

pub fn terrain_chunk_y_range(config: &TerrainConfig) -> RangeInclusive<i32> {
    world_chunk_y_bounds(config)
}

pub fn world_chunk_y_bounds(config: &TerrainConfig) -> RangeInclusive<i32> {
    0..=(config.world_max_y / CHUNK_SIZE)
}
