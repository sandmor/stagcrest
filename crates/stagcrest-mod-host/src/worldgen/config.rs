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
    pub cave_frequency: f64,
    pub density_threshold: f64,
    pub gradient_strength: f64,
    pub sky_islands_enabled: bool,
    pub island_placement_frequency: f64,
    pub island_placement_threshold: f64,
    pub island_min_y: i32,
    pub island_max_y: i32,
    pub island_blob_frequency: f64,
    pub island_blob_threshold: f64,
    pub island_vertical_radius: i32,
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
            cave_frequency: 0.03,
            density_threshold: 0.0,
            gradient_strength: 0.06,
            sky_islands_enabled: false,
            island_placement_frequency: 0.003,
            island_placement_threshold: 0.65,
            island_min_y: 100,
            island_max_y: 140,
            island_blob_frequency: 0.08,
            island_blob_threshold: 0.35,
            island_vertical_radius: 12,
        }
    }
}

pub fn terrain_chunk_y_range(config: &TerrainConfig) -> RangeInclusive<i32> {
    world_chunk_y_bounds(config)
}

pub fn world_chunk_y_bounds(config: &TerrainConfig) -> RangeInclusive<i32> {
    0..=(config.world_max_y / CHUNK_SIZE)
}
