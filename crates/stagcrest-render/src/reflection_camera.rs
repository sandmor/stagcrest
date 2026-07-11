//! Planar mirror reflection camera for water (off-screen player/entity reflections).

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Hdr, RenderTarget};
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use stagcrest_mod_client::SEA_LEVEL;

use crate::graphics_settings::{GraphicsSettings, ReflectionTier};
use crate::scene_reflection::SceneReflectionImages;
use crate::skybox::SkyMaterial;

/// Marks the primary world view camera (attached by the client).
///
/// Synced to the render world so main-view-only passes (scene copy, god rays) can
/// target the game camera and skip UI / reflection cameras.
#[derive(Component, Clone, Copy, Default, ExtractComponent)]
pub struct MainViewCamera;

/// World geometry visible to both the main and reflection cameras.
pub const WORLD_LAYER: usize = 0;
/// Water surfaces — main camera only (excluded from the reflection camera).
pub const WATER_LAYER: usize = 1;

/// Local player body — excluded from the main camera in first person.
pub const PLAYER_LAYER: usize = 2;

/// Render layers for the main game camera (world + water).
pub fn main_camera_layers() -> RenderLayers {
    RenderLayers::layer(WORLD_LAYER).with(WATER_LAYER)
}

/// Main camera layers when the local player body should be visible (third person).
pub fn main_camera_third_person_layers() -> RenderLayers {
    main_camera_layers().with(PLAYER_LAYER)
}

/// Render layers for the local player Bedrock model.
pub fn player_visual_layers() -> RenderLayers {
    RenderLayers::layer(PLAYER_LAYER)
}

/// Directional lights must include player geometry so it stays lit and casts shadows.
pub fn world_light_layers() -> RenderLayers {
    main_camera_third_person_layers()
}

/// Render layers for the planar reflection camera (no water recursion).
pub fn reflection_camera_layers() -> RenderLayers {
    RenderLayers::layer(WORLD_LAYER)
}

/// Water chunk entities use this layer so the reflection camera skips them.
pub fn water_chunk_layers() -> RenderLayers {
    RenderLayers::layer(WATER_LAYER)
}

#[derive(Component, Clone, Copy, Default, ExtractComponent)]
pub struct PlanarReflectionCamera;

/// Water mirror plane height in world units; set from local player feet on the client.
#[derive(Component, Clone, Copy, Default, ExtractComponent)]
pub struct ReflectionPlaneHeight(pub f32);

pub struct ReflectionCameraPlugin;

impl Plugin for ReflectionCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<MainViewCamera>::default(),
            ExtractComponentPlugin::<PlanarReflectionCamera>::default(),
            ExtractComponentPlugin::<ReflectionPlaneHeight>::default(),
        ))
        .add_systems(Update, sync_planar_reflection_camera);
    }
}

fn sync_planar_reflection_camera(
    mut commands: Commands,
    settings: Res<GraphicsSettings>,
    reflection: Res<SceneReflectionImages>,
    main_cam: Query<
        (&Transform, &Projection, Option<&ReflectionPlaneHeight>),
        (With<MainViewCamera>, Without<PlanarReflectionCamera>),
    >,
    mut reflection_cam: Query<
        (
            Entity,
            &mut Transform,
            &mut Camera,
            &mut Projection,
            &mut RenderTarget,
        ),
        (With<PlanarReflectionCamera>, Without<MainViewCamera>),
    >,
) {
    let wants_planar = matches!(
        settings.reflection_tier,
        ReflectionTier::Planar
    );

    let Ok((main_transform, main_projection, plane_height)) = main_cam.single() else {
        return;
    };

    if !wants_planar || reflection.planar_color == Handle::default() {
        for (entity, ..) in &reflection_cam {
            commands.entity(entity).despawn();
        }
        return;
    }

    let plane_y = plane_height
        .map(|plane| plane.0)
        .unwrap_or(SEA_LEVEL as f32 + 0.9);
    let mirrored = mirror_transform(main_transform, plane_y);
    let target = RenderTarget::Image(reflection.planar_color.clone().into());

    if let Ok((entity, mut tf, mut cam, mut proj, mut render_target)) = reflection_cam.single_mut() {
        *tf = mirrored;
        *proj = main_projection.clone();
        cam.order = -1;
        cam.invert_culling = true;
        *render_target = target;
        commands.entity(entity).try_insert(SkyMaterial::default());
        return;
    }

    if !reflection_cam.is_empty() {
        return;
    }

    commands.spawn((
        PlanarReflectionCamera,
        Camera3d::default(),
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::srgba(0.02, 0.04, 0.08, 1.0)),
            invert_culling: true,
            ..default()
        },
        target,
        mirrored,
        main_projection.clone(),
        Hdr,
        Msaa::Off,
        reflection_camera_layers(),
        SkyMaterial::default(),
        bevy::core_pipeline::prepass::DepthPrepass,
    ));
}

fn mirror_transform(main: &Transform, plane_y: f32) -> Transform {
    let mut pos = main.translation;
    pos.y = 2.0 * plane_y - pos.y;

    let forward = main.forward().as_vec3();
    let up = main.up().as_vec3();
    let mirrored_forward = Vec3::new(forward.x, -forward.y, forward.z).normalize_or_zero();
    let mirrored_up = Vec3::new(up.x, -up.y, up.z).normalize_or_zero();

    if mirrored_forward.length_squared() < 0.001 {
        return Transform::from_translation(pos);
    }

    Transform::from_translation(pos).looking_to(mirrored_forward, mirrored_up)
}
