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
    // Multi-noise sampling frequencies
    pub continentalness_frequency: f64,
    pub erosion_frequency: f64,
    pub peaks_valleys_frequency: f64,
    pub ridges_frequency: f64,
    pub river_frequency: f64,
    pub river_b_frequency: f64,
    pub cave_biome_humidity_frequency: f64,
    // Sky islands
    pub sky_islands_enabled: bool,
    pub island_placement_frequency: f64,
    pub island_placement_threshold: f64,
    pub island_min_y: i32,
    pub island_max_y: i32,
    pub island_blob_frequency: f64,
    pub island_blob_threshold: f64,
    pub island_vertical_radius: i32,
    pub island_dome_strength: f64,
    // Climate
    pub temperature_frequency: f64,
    pub moisture_frequency: f64,
    pub temperature_scale: f32,
    // Terrain shaping spline amplitudes
    pub continental_ocean_depth: f64,
    pub continental_land_boost: f64,
    pub erosion_flatten: f64,
    pub peaks_boost: f64,
    // Caves
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
    // Ore
    pub ore_frequency: f64,
    pub ore_threshold: f64,
    pub ore_max_y: i32,
    // Rivers
    pub river_width: f64,
    pub river_valley_depth: f64,
    pub river_warp_strength: f64,
    /// Offset applied to erosion-gate edges for river suppression in mountains.
    pub river_erosion_bias: f64,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            sea_level: SEA_LEVEL,
            world_min_y: 0,
            world_max_y: 256,
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
            continentalness_frequency: 0.0021,
            erosion_frequency: 0.003,
            peaks_valleys_frequency: 0.0038,
            ridges_frequency: 0.002,
            river_frequency: 0.004,
            river_b_frequency: 0.004,
            cave_biome_humidity_frequency: 0.02,
            sky_islands_enabled: true,
            island_placement_frequency: 0.002,
            island_placement_threshold: 0.45,
            island_min_y: 200,
            island_max_y: 250,
            island_blob_frequency: 0.06,
            island_blob_threshold: 0.25,
            island_vertical_radius: 24,
            island_dome_strength: 1.2,
            temperature_frequency: 0.0042,
            moisture_frequency: 0.0042,
            temperature_scale: 2.5,
            continental_ocean_depth: 28.0,
            continental_land_boost: 32.0,
            erosion_flatten: 18.0,
            peaks_boost: 64.0,
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
            river_width: 0.06,
            river_valley_depth: 6.0,
            river_warp_strength: 0.4,
            river_erosion_bias: 0.0,
        }
    }
}

pub fn terrain_chunk_y_range(config: &TerrainConfig) -> RangeInclusive<i32> {
    world_chunk_y_bounds(config)
}

pub fn world_chunk_y_bounds(config: &TerrainConfig) -> RangeInclusive<i32> {
    floor_div(config.world_min_y, CHUNK_SIZE)..=floor_div(config.world_max_y, CHUNK_SIZE)
}

fn floor_div(a: i32, b: i32) -> i32 {
    if a >= 0 {
        a / b
    } else {
        (a - (b - 1)) / b
    }
}
