//! Shared scene lighting state for voxels, sky, water, and entities.
//!
//! Conventions: `docs/RENDERING.md` §5.5.

use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_resource::ShaderType;
use bevy::render::RenderApp;

use crate::graphics_settings::ReflectionTier;
use crate::sky_palette::SkyPalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Medium {
    #[default]
    Air,
    Water,
}

/// GPU uniform consumed by custom shaders (voxel, skybox, water, underwater).
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
    /// `.x` day_factor, `.y` cycle 0..1, `.z` medium (0 air / 1 water), `.w` submersion
    pub params: Vec4,
    pub water_absorption: Vec4,
    pub horizon_color: Vec4,
    pub zenith_color: Vec4,
    /// `.x` sunset_strength, `.y` star_strength, `.z` reflection_tier, `.w` elapsed seconds (star twinkle)
    pub sky_params: Vec4,
    /// `.x` sun GPU light index, `.y` moon GPU light index, `.z` sun shadow strength 0..1, `.w` moon shadow strength 0..1
    pub shadow_params: Vec4,
}

impl Default for SceneLightingUniform {
    fn default() -> Self {
        let palette = SkyPalette::default();
        Self {
            sun_position_dir: Vec4::new(0.0, 1.0, 0.0, 0.0),
            moon_position_dir: Vec4::new(0.0, -0.35, 0.0, 0.0),
            sun_color: palette.sun_color.extend(1.0),
            moon_color: palette.moon_color.extend(1.0),
            ambient_color: palette
                .ambient_color
                .extend(palette.ambient_intensity),
            params: Vec4::new(1.0, 0.0, 0.0, 0.0),
            water_absorption: Vec4::new(0.6, 0.04, 0.01, 0.0),
            horizon_color: palette.horizon_color.extend(1.0),
            zenith_color: palette.zenith_color.extend(1.0),
            sky_params: Vec4::new(0.0, 0.0, 1.0, 0.0),
            shadow_params: Vec4::new(0.0, 1.0, 0.0, 0.0),
        }
    }
}

impl SceneLightingUniform {
    /// `.x` = day_factor
    pub fn day_factor(&self) -> f32 {
        self.params.x
    }

    pub fn from_scene(
        sun_position_dir: Vec3,
        moon_position_dir: Vec3,
        day_factor: f32,
        cycle: f32,
        palette: &SkyPalette,
        submersion: f32,
        medium: Medium,
        reflection_tier: ReflectionTier,
    ) -> Self {
        Self {
            sun_position_dir: sun_position_dir.extend(0.0),
            moon_position_dir: moon_position_dir.extend(0.0),
            sun_color: palette.sun_color.extend(1.0),
            moon_color: palette.moon_color.extend(1.0),
            ambient_color: palette
                .ambient_color
                .extend(palette.ambient_intensity),
            params: Vec4::new(
                day_factor,
                cycle,
                if medium == Medium::Water { 1.0 } else { 0.0 },
                submersion,
            ),
            water_absorption: Vec3::new(0.55, 0.035, 0.008).extend(1.0),
            horizon_color: palette.horizon_color.extend(1.0),
            zenith_color: palette.zenith_color.extend(1.0),
            sky_params: Vec4::new(
                palette.sunset_strength,
                palette.star_strength,
                reflection_tier_index(reflection_tier),
                0.0,
            ),
            shadow_params: Vec4::ZERO,
        }
    }
}

fn reflection_tier_index(tier: ReflectionTier) -> f32 {
    match tier {
        ReflectionTier::SkyOnly => 0.0,
        ReflectionTier::Ssr => 1.0,
        ReflectionTier::Planar => 2.0,
    }
}

#[derive(Resource, Clone, ExtractResource, Default)]
pub struct SceneLighting {
    pub uniform: SceneLightingUniform,
    pub palette: SkyPalette,
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
