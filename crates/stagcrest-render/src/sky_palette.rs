//! Artist-driven dawn / noon / dusk / night color curves blended with biome tints.

use bevy::prelude::*;
use stagcrest_protocol::TimeOfDay;

/// CPU-side sky and lighting palette for one frame.
#[derive(Debug, Clone, Copy)]
pub struct SkyPalette {
    pub sun_color: Vec3,
    pub moon_color: Vec3,
    pub ambient_color: Vec3,
    pub ambient_intensity: f32,
    pub horizon_color: Vec3,
    pub zenith_color: Vec3,
    pub fog_color: Vec3,
    pub sunset_strength: f32,
    pub star_strength: f32,
}

impl Default for SkyPalette {
    fn default() -> Self {
        Self::from_time_and_biome(&TimeOfDay::default(), [0.47, 0.65, 1.0], [0.72, 0.82, 0.95])
    }
}

impl SkyPalette {
    /// Build a palette from world time and biome-interpolated sky/fog colors.
    pub fn from_time_and_biome(
        time: &TimeOfDay,
        biome_sky: [f32; 3],
        biome_fog: [f32; 3],
    ) -> Self {
        let sun_dir = time.sun_dir();
        let day = time.day_factor();
        let night = 1.0 - day;
        let elevation = sun_dir.y;

        // Twilight band: strongest near sunrise/sunset (sun near horizon).
        let twilight = (1.0 - (elevation.abs() / 0.35).clamp(0.0, 1.0)).powf(1.4);
        let sunset_strength = twilight * (0.35 + day * 0.65);

        // Period light colors (MCExample2-style morning / noon / night multipliers).
        let morning_light = Vec3::new(1.0, 0.82, 0.62);
        let noon_light = Vec3::new(0.95, 0.97, 1.0);
        let sunset_light = Vec3::new(1.0, 0.55, 0.22);
        let night_light = Vec3::new(0.35, 0.42, 0.72);

        let morning_ambient = Vec3::new(0.72, 0.62, 0.55);
        let noon_ambient = Vec3::new(0.55, 0.62, 0.78);
        let sunset_ambient = Vec3::new(0.85, 0.45, 0.28);
        let night_ambient = Vec3::new(0.04, 0.05, 0.14);

        let morning_factor = (1.0 - elevation.max(0.0) / 0.25).clamp(0.0, 1.0) * day;
        let noon_factor = (elevation / 0.85).clamp(0.0, 1.0) * day * (1.0 - sunset_strength * 0.65);

        let mut day_light = noon_light * noon_factor + morning_light * morning_factor;
        day_light = day_light.lerp(sunset_light, sunset_strength);
        if day_light.length_squared() < 0.001 {
            day_light = sunset_light;
        } else {
            day_light = day_light.normalize() * noon_light.length().max(0.85);
        }

        let mut day_ambient = noon_ambient * noon_factor + morning_ambient * morning_factor;
        day_ambient = day_ambient.lerp(sunset_ambient, sunset_strength * 0.85);

        let sun_color = day_light.lerp(night_light, night);
        let moon_color = Vec3::new(0.72, 0.78, 0.98) * (0.25 + night * 0.75);
        let ambient_base = day_ambient.lerp(night_ambient, night);

        let biome_sky = Vec3::from_array(biome_sky);
        let biome_fog = Vec3::from_array(biome_fog);

        let noon_zenith_tint = Vec3::new(0.92, 0.96, 1.05);
        let noon_horizon_tint = Vec3::new(1.02, 1.0, 0.94);
        let day_sky_strength = (day * (1.0 - sunset_strength * 0.35)).clamp(0.0, 1.0);

        let night_zenith = Vec3::new(0.02, 0.02, 0.09);
        let night_horizon = Vec3::new(0.08, 0.06, 0.18);
        let sunset_horizon = Vec3::new(0.95, 0.38, 0.18);
        let sunset_zenith = Vec3::new(0.35, 0.22, 0.55);

        let zenith = biome_sky
            .lerp(night_zenith, night)
            .lerp(sunset_zenith, sunset_strength * 0.55)
            .lerp(biome_sky * noon_zenith_tint, day_sky_strength * 0.35);
        let horizon = biome_fog
            .lerp(night_horizon, night)
            .lerp(sunset_horizon, sunset_strength * 0.9)
            .lerp(biome_fog * noon_horizon_tint, day_sky_strength * 0.25);
        let fog_color = horizon.lerp(ambient_base, 0.25);

        let ambient_color = ambient_base.lerp(biome_fog.into(), 0.35);
        let ambient_intensity = 0.12 + day * 0.38 + sunset_strength * 0.08;

        let star_strength = (night * (1.0 - sunset_strength * 0.5)).clamp(0.0, 1.0);

        Self {
            sun_color,
            moon_color,
            ambient_color,
            ambient_intensity,
            horizon_color: horizon,
            zenith_color: zenith,
            fog_color,
            sunset_strength,
            star_strength,
        }
    }
}
