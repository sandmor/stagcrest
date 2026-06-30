//! Multi-camera rendering for the client.
//!
//! Stagcrest draws to one window with two cameras:
//!
//! | Camera | Type | Order | Purpose |
//! |--------|------|-------|---------|
//! | World  | `Camera3d` | 0 | Voxels, block outline, underwater/fog post-process |
//! | UI     | `Camera2d` | 1 | HUD, menus, inventory (via [`IsDefaultUiCamera`]) |
//!
//! ## Why two cameras
//!
//! Post-processing (underwater tint, fog) runs on the 3D view. A separate UI camera
//! composites widgets *after* upscaling the world, so the HUD stays crisp and untinted.
//!
//! ## Footguns — read before adding or changing cameras
//!
//! 1. **Overlay intermediate clear must be transparent.** Never use
//!    `ClearColorConfig::Default` on a UI overlay camera: it clears to the global
//!    [`ClearColor`] (sky color), and upscaling that full-screen texture hides the
//!    entire 3D world outside widget pixels.
//! 2. **Overlay swapchain clear must be `None`.** `output_mode.clear_color` must be
//!    `ClearColorConfig::None` so upscaling loads the framebuffer the world camera
//!    already wrote (the swapchain attachment is shared per window).
//! 3. **Overlay upscaling must alpha-blend.** Use [`overlay_ui_camera_bundle`] or set
//!    `blend_state: Some(BlendState::ALPHA_BLENDING)` explicitly; do not rely on
//!    camera order if you add more views to the same target.
//! 4. **Do not mark the world camera `IsDefaultUiCamera`.** UI would render into the
//!    3D render target and receive post-processing unless you rework the graph.
//! 5. **New overlay cameras** (debug HUD, cinematics) need the same transparent clear
//!    and alpha composite settings as the UI camera.

use bevy::camera::{CameraOutputMode, ClearColorConfig};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::ui::IsDefaultUiCamera;

/// Default render order for [`spawn_overlay_ui_camera`]: above the world `Camera3d`.
pub const OVERLAY_UI_CAMERA_ORDER: isize = 1;

/// Marker for the dedicated HUD / menu overlay camera.
#[derive(Component)]
pub struct OverlayUiCamera;

/// Spawns the UI overlay camera that composites Bevy UI above the 3D world.
pub fn spawn_overlay_ui_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        OverlayUiCamera,
        IsDefaultUiCamera,
        overlay_ui_camera_bundle(OVERLAY_UI_CAMERA_ORDER),
    ));
}

/// [`Camera`] bundle for any camera that composites UI (or other 2D content) over the world.
pub fn overlay_ui_camera_bundle(order: isize) -> Camera {
    Camera {
        order,
        clear_color: ClearColorConfig::Custom(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        output_mode: CameraOutputMode::Write {
            blend_state: Some(BlendState::ALPHA_BLENDING),
            clear_color: ClearColorConfig::None,
        },
        ..default()
    }
}

pub struct UiCameraPlugin;

impl Plugin for UiCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_overlay_ui_camera);
    }
}
