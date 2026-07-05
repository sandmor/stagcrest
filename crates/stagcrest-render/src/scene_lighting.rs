//! Shared scene lighting state for voxels, sky, water, and future entities.
//!
//! Conventions: `docs/scene_lighting_conventions.md`.

use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_resource::ShaderType;
use bevy::render::RenderApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Medium {
    #[default]
    Air,
    Water,
}

/// GPU uniform consumed by custom shaders (voxel, skybox, underwater).
///
/// Celestial fields are **position** directions (scene → body), not incoming light.
/// Bevy `DirectionalLight` uses [`stagcrest_protocol::TimeOfDay::sun_light_dir`] instead.
#[derive(Debug, Clone, Copy, ShaderType)]
pub struct SceneLightingUniform {
    /// Unit vector from the scene toward the sun.
    pub sun_position_dir: Vec4,
    /// Unit vector from the scene toward the moon.
    pub moon_position_dir: Vec4,
    pub sun_color: Vec4,
    pub moon_color: Vec4,
    pub ambient_color: Vec4,
    pub params: Vec4,
    pub water_absorption: Vec4,
    pub horizon_color: Vec4,
}

impl Default for SceneLightingUniform {
    fn default() -> Self {
        Self {
            sun_position_dir: Vec4::new(0.0, 1.0, 0.0, 0.0),
            moon_position_dir: Vec4::new(0.0, -0.35, 0.0, 0.0),
            sun_color: Vec4::new(1.0, 0.98, 0.92, 1.0),
            moon_color: Vec4::new(0.65, 0.72, 0.95, 1.0),
            ambient_color: Vec4::new(0.47, 0.65, 1.0, 0.35),
            params: Vec4::new(1.0, 0.0, 0.0, 0.0),
            water_absorption: Vec4::new(0.6, 0.04, 0.01, 0.0),
            horizon_color: Vec4::new(0.47, 0.65, 1.0, 1.0),
        }
    }
}

impl SceneLightingUniform {
    /// `.x` = day_factor, `.y` = time_of_day cycle 0..1, `.z` = medium (0 air, 1 water), `.w` = submersion
    pub fn day_factor(&self) -> f32 {
        self.params.x
    }

    pub fn from_scene(
        sun_position_dir: Vec3,
        moon_position_dir: Vec3,
        day_factor: f32,
        cycle: f32,
        sky_color: [f32; 3],
        fog_color: [f32; 3],
        _water_tint: [f32; 3],
        submersion: f32,
        medium: Medium,
    ) -> Self {
        let dusk = (1.0 - (day_factor * 2.0 - 1.0).abs()).max(0.0);
        let warm = Vec3::new(1.0, 0.85, 0.65);
        let sun_color = Vec3::new(1.0, 0.98, 0.92).lerp(warm, dusk * 0.6);
        let moon_color = Vec3::new(0.65, 0.72, 0.95);
        let ambient_base = Vec3::from_array(fog_color).lerp(Vec3::from_array(sky_color), 0.35);
        let ambient_intensity = 0.15 + day_factor * 0.35;
        let zenith = Vec3::from_array(sky_color);
        let horizon = ambient_base.lerp(zenith, 0.5);

        Self {
            sun_position_dir: sun_position_dir.extend(0.0),
            moon_position_dir: moon_position_dir.extend(0.0),
            sun_color: sun_color.extend(1.0),
            moon_color: moon_color.extend(1.0),
            ambient_color: ambient_base.extend(ambient_intensity),
            params: Vec4::new(
                day_factor,
                cycle,
                if medium == Medium::Water { 1.0 } else { 0.0 },
                submersion,
            ),
            water_absorption: Vec3::new(0.6, 0.04, 0.01).extend(1.0),
            horizon_color: horizon.extend(1.0),
        }
    }
}

#[derive(Resource, Clone, ExtractResource, Default)]
pub struct SceneLighting {
    pub uniform: SceneLightingUniform,
}

pub struct SceneLightingPlugin;

impl Plugin for SceneLightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneLighting>()
            .add_plugins(ExtractResourcePlugin::<SceneLighting>::default());

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.init_resource::<SceneLighting>();
    }
}
