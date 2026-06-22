use bevy::prelude::*;
use stagcrest_render::{VoxelMaterial, VoxelMaterialPlugin};

#[cfg(target_arch = "wasm32")]
mod wasm_gpu;

pub mod block_icons;
pub mod block_outline;
pub mod debug_overlay;
pub mod game;
pub mod inventory;
pub mod loading;
pub mod menu;
pub mod pause;
pub mod player;
pub mod terrain_queue;
pub mod targeting;
pub mod ui;

pub use game::AppState;

fn window_plugin() -> WindowPlugin {
    WindowPlugin {
        primary_window: Some(Window {
            title: "Stagcrest".into(),
            resolution: (1280.0, 720.0).into(),
            ..default()
        }),
        ..default()
    }
}

#[cfg(target_arch = "wasm32")]
fn default_plugins() -> bevy::app::PluginGroupBuilder {
    use bevy::render::settings::{Backends, RenderCreation, WgpuSettings, WgpuSettingsPriority};
    use bevy::render::RenderPlugin;

    let backends = crate::wasm_gpu::web_backends();
    let priority = if backends.contains(Backends::BROWSER_WEBGPU) {
        WgpuSettingsPriority::Functionality
    } else {
        WgpuSettingsPriority::WebGL2
    };
    let webgl2_limits = matches!(priority, WgpuSettingsPriority::WebGL2);
    DefaultPlugins
        .set(window_plugin())
        .set(RenderPlugin {
            render_creation: RenderCreation::Automatic(WgpuSettings {
                backends: Some(backends),
                priority,
                limits: if webgl2_limits {
                    bevy::render::settings::WgpuLimits::downlevel_webgl2_defaults()
                } else {
                    bevy::render::settings::WgpuLimits::default()
                },
                ..default()
            }),
            ..default()
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn default_plugins() -> bevy::app::PluginGroupBuilder {
    DefaultPlugins.set(window_plugin())
}

pub fn run_app() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    #[cfg(not(target_arch = "wasm32"))]
    {
        tracing_subscriber::fmt::init();
    }

    App::new()
        .add_plugins(default_plugins())
        .init_state::<AppState>()
        .add_plugins(VoxelMaterialPlugin)
        .add_plugins(stagcrest_render::OutlineMaterialPlugin)
        .add_plugins(MaterialPlugin::<VoxelMaterial>::default())
        .add_plugins(MaterialPlugin::<stagcrest_render::OutlineMaterial>::default())
        .add_plugins((
            menu::MenuPlugin,
            loading::LoadingPlugin,
            game::GamePlugin,
            debug_overlay::DebugPlugin,
            pause::PausePlugin,
            ui::UiPlugin,
            UiCameraPlugin,
        ))
        .run();
}

struct UiCameraPlugin;

impl Plugin for UiCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_ui_camera);
    }
}

#[derive(Component)]
struct UiCamera;

fn setup_ui_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d::default(),
        UiCamera,
        Camera {
            order: 1,
            ..default()
        },
    ));
}
