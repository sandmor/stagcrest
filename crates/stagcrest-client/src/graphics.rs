//! Applies [`GraphicsSettings`] to the world camera and celestial lights.

use std::collections::HashMap;

use bevy::anti_alias::{
    contrast_adaptive_sharpening::ContrastAdaptiveSharpening, taa::TemporalAntiAliasing,
};
use bevy::camera::Hdr;
use bevy::core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass, NormalPrepass};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel};
use bevy::post_process::bloom::Bloom;
use bevy::post_process::dof::{DepthOfField, DepthOfFieldMode};
use bevy::prelude::*;
use bevy::render::camera::{MipBias, TemporalJitter};

use crate::game_session::GameCamera;
use crate::player::LocalPlayer;
use stagcrest_content::{GraphicsQualityTier, GraphicsReflectionTier, GraphicsSection};
use stagcrest_render::{
    GraphicsSettings, MainViewCamera, QualityTier, ReflectionPlaneHeight, ReflectionTier,
    VolumetricLightSettings, world_light_layers,
};

/// Marks the sun directional light entity (GPU index resolved each frame).
#[derive(Component)]
pub struct SunLight;

/// Marks the moon directional light entity (GPU index resolved each frame).
#[derive(Component)]
pub struct MoonLight;

pub struct GraphicsApplyPlugin;

impl Plugin for GraphicsApplyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                apply_graphics_to_camera,
                apply_graphics_to_new_cameras,
                apply_graphics_to_celestial_lights,
                sync_reflection_plane,
            ),
        );
    }
}

/// Replicates Bevy's directional-light sort: `(volumetric, shadow_maps_enabled, entity)`.
/// Shadow-disabled lights sort before shadow-enabled ones; celestial lights never use volumetric.
pub fn directional_light_gpu_indices(lights: &[(Entity, bool)]) -> HashMap<Entity, u32> {
    let mut sorted = lights.to_vec();
    sorted.sort_by_cached_key(|(entity, shadow_maps_enabled)| {
        (false, *shadow_maps_enabled, *entity)
    });
    sorted
        .iter()
        .enumerate()
        .map(|(index, (entity, _))| (*entity, index as u32))
        .collect()
}

pub fn shadow_config(settings: &GraphicsSettings) -> CascadeShadowConfig {
    CascadeShadowConfigBuilder {
        num_cascades: settings.shadow_cascades.clamp(1, 4) as usize,
        maximum_distance: settings.shadow_distance,
        ..default()
    }
    .build()
}

/// Bundle for a celestial directional light; shadow casting is toggled per-frame in
/// [`crate::game::sync_scene_lighting`].
pub fn celestial_light_bundle(settings: &GraphicsSettings) -> impl Bundle {
    (
        DirectionalLight {
            illuminance: 0.0,
            shadow_maps_enabled: false,
            ..default()
        },
        shadow_config(settings),
        world_light_layers(),
    )
}

/// Configure the sun directional light shadow cascades from graphics settings.
pub fn sun_light_bundle(settings: &GraphicsSettings) -> impl Bundle {
    (SunLight, celestial_light_bundle(settings))
}

/// Configure the moon directional light shadow cascades from graphics settings.
pub fn moon_light_bundle(settings: &GraphicsSettings) -> impl Bundle {
    (MoonLight, celestial_light_bundle(settings))
}

fn apply_graphics_to_new_cameras(
    mut commands: Commands,
    settings: Res<GraphicsSettings>,
    new_cameras: Query<Entity, (With<GameCamera>, Added<GameCamera>)>,
) {
    for entity in &new_cameras {
        strip_optional_components(&mut commands, entity, &settings);
        attach_optional_components(&mut commands, entity, &settings);
    }
}

pub fn apply_graphics_to_camera(
    settings: Res<GraphicsSettings>,
    mut commands: Commands,
    existing: Query<Entity, With<GameCamera>>,
) {
    if !settings.is_changed() {
        return;
    }

    for entity in &existing {
        strip_optional_components(&mut commands, entity, &settings);
        attach_optional_components(&mut commands, entity, &settings);
    }
}

fn apply_graphics_to_celestial_lights(
    settings: Res<GraphicsSettings>,
    mut configs: Query<&mut CascadeShadowConfig, Or<(With<SunLight>, With<MoonLight>)>>,
    new_lights: Query<Entity, Or<(Added<SunLight>, Added<MoonLight>)>>,
) {
    if !settings.is_changed() && new_lights.is_empty() {
        return;
    }
    let config = shadow_config(&settings);
    for mut existing in &mut configs {
        *existing = config.clone();
    }
}

/// Anchor planar water reflections to the player feet, not the orbiting camera.
pub fn sync_reflection_plane(
    player: Query<&Transform, With<LocalPlayer>>,
    mut camera: Query<&mut ReflectionPlaneHeight, With<MainViewCamera>>,
) {
    let Ok(feet) = player.single() else {
        return;
    };
    let Ok(mut plane) = camera.single_mut() else {
        return;
    };
    plane.0 = feet.translation.y.floor() + 0.9;
}

fn strip_optional_components(commands: &mut Commands, entity: Entity, settings: &GraphicsSettings) {
    if !settings.ssao {
        commands
            .entity(entity)
            .remove::<ScreenSpaceAmbientOcclusion>()
            .remove::<NormalPrepass>();
    }
    if !settings.bloom {
        commands.entity(entity).remove::<Bloom>();
    }
    if !settings.taa {
        commands
            .entity(entity)
            .remove::<(TemporalAntiAliasing, TemporalJitter, MipBias)>()
            .remove::<MotionVectorPrepass>();
    }
    if !settings.dof {
        commands.entity(entity).remove::<DepthOfField>();
    }
    if !settings.sharpen {
        commands
            .entity(entity)
            .remove::<ContrastAdaptiveSharpening>();
    }
    if !settings.volumetric_light {
        commands.entity(entity).remove::<VolumetricLightSettings>();
    }
}

fn attach_optional_components(
    commands: &mut Commands,
    entity: Entity,
    settings: &GraphicsSettings,
) {
    commands.entity(entity).insert((
        Hdr,
        Msaa::Off,
        DepthPrepass,
        Tonemapping::TonyMcMapface,
    ));

    if settings.ssao {
        commands.entity(entity).insert((
            ScreenSpaceAmbientOcclusion {
                quality_level: match settings.tier {
                    QualityTier::Low | QualityTier::Medium => {
                        ScreenSpaceAmbientOcclusionQualityLevel::Low
                    }
                    QualityTier::High => ScreenSpaceAmbientOcclusionQualityLevel::Medium,
                    QualityTier::Ultra => ScreenSpaceAmbientOcclusionQualityLevel::High,
                },
                ..default()
            },
            NormalPrepass,
        ));
    }

    if settings.bloom {
        commands.entity(entity).insert(Bloom::NATURAL);
    }

    if settings.taa {
        commands.entity(entity).insert((
            TemporalAntiAliasing::default(),
            TemporalJitter::default(),
            MipBias(-1.0),
            MotionVectorPrepass,
        ));
    }

    if settings.dof {
        commands.entity(entity).insert(DepthOfField {
            mode: DepthOfFieldMode::Bokeh,
            ..default()
        });
    }

    if settings.sharpen {
        commands
            .entity(entity)
            .insert(ContrastAdaptiveSharpening::default());
    }

    if settings.volumetric_light {
        commands
            .entity(entity)
            .insert(VolumetricLightSettings::default());
    } else {
        commands
            .entity(entity)
            .insert(VolumetricLightSettings::disabled());
    }
}

fn clamp_shadow_cascades(cascades: u32) -> u32 {
    cascades.clamp(1, 4)
}

/// Load graphics settings from client content settings into the render resource.
pub fn graphics_settings_from_section(section: &GraphicsSection) -> GraphicsSettings {
    GraphicsSettings {
        tier: match section.tier {
            GraphicsQualityTier::Low => QualityTier::Low,
            GraphicsQualityTier::Medium => QualityTier::Medium,
            GraphicsQualityTier::High => QualityTier::High,
            GraphicsQualityTier::Ultra => QualityTier::Ultra,
        },
        reflection_tier: match section.reflection_tier {
            GraphicsReflectionTier::SkyOnly => ReflectionTier::SkyOnly,
            GraphicsReflectionTier::Ssr => ReflectionTier::Ssr,
            GraphicsReflectionTier::Planar => ReflectionTier::Planar,
        },
        shadow_cascades: clamp_shadow_cascades(section.shadow_cascades),
        shadow_distance: section.shadow_distance,
        ssao: section.ssao,
        bloom: section.bloom,
        taa: section.taa,
        volumetric_light: section.volumetric_light,
        fog: section.fog,
        dof: section.dof,
        sharpen: section.sharpen,
    }
}

pub fn graphics_section_from_settings(settings: &GraphicsSettings) -> GraphicsSection {
    GraphicsSection {
        tier: match settings.tier {
            QualityTier::Low => GraphicsQualityTier::Low,
            QualityTier::Medium => GraphicsQualityTier::Medium,
            QualityTier::High => GraphicsQualityTier::High,
            QualityTier::Ultra => GraphicsQualityTier::Ultra,
        },
        reflection_tier: match settings.reflection_tier {
            ReflectionTier::SkyOnly => GraphicsReflectionTier::SkyOnly,
            ReflectionTier::Ssr => GraphicsReflectionTier::Ssr,
            ReflectionTier::Planar => GraphicsReflectionTier::Planar,
        },
        shadow_cascades: settings.shadow_cascades,
        shadow_distance: settings.shadow_distance,
        ssao: settings.ssao,
        bloom: settings.bloom,
        taa: settings.taa,
        volumetric_light: settings.volumetric_light,
        fog: settings.fog,
        dof: settings.dof,
        sharpen: settings.sharpen,
    }
}

/// Load graphics settings from client content settings into the render resource.
pub fn sync_graphics_settings_resource(
    client_settings: &crate::client_content::ClientContentSettings,
    mut graphics: ResMut<GraphicsSettings>,
) {
    *graphics = graphics_settings_from_section(client_settings.0.graphics());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directional_light_indices_follow_bevy_sort_when_only_moon_shadows() {
        let sun = Entity::from_bits(10);
        let moon = Entity::from_bits(20);
        let sun_light = DirectionalLight {
            shadow_maps_enabled: false,
            ..default()
        };
        let moon_light = DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        };
        let indices = directional_light_gpu_indices(&[
            (sun, sun_light.shadow_maps_enabled),
            (moon, moon_light.shadow_maps_enabled),
        ]);
        // Non-shadow lights sort before shadow-casting lights (Bevy `(volumetric, shadow, entity)`).
        assert_eq!(indices.get(&sun), Some(&0));
        assert_eq!(indices.get(&moon), Some(&1));
    }

    #[test]
    fn directional_light_indices_stable_when_both_cast_shadows() {
        let sun = Entity::from_bits(10);
        let moon = Entity::from_bits(20);
        let enabled = DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        };
        let indices = directional_light_gpu_indices(&[
            (sun, enabled.shadow_maps_enabled),
            (moon, enabled.shadow_maps_enabled),
        ]);
        assert_eq!(indices.get(&sun), Some(&0));
        assert_eq!(indices.get(&moon), Some(&1));
    }
}
