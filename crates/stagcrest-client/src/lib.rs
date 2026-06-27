use bevy::prelude::*;
use stagcrest_render::{VoxelMaterial, VoxelMaterialPlugin};

pub mod block_icons;
pub mod block_outline;
pub mod chunk_streaming;
pub mod debug_overlay;
pub mod environment;
pub mod game;
pub mod inventory;
pub mod loading;
pub mod logging;
pub mod menu;
pub mod mesh_scheduler;
pub mod net_client;
pub mod pause;
pub mod player;
pub mod targeting;
pub mod ui;
pub mod world_replica;

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

pub use net_client::{GameNetClient, LaunchConfig};

pub fn run_app(launch: LaunchConfig) {
    crate::logging::init();

    App::new()
        .add_plugins(DefaultPlugins.set(window_plugin()))
        .insert_resource(launch.clone())
        .insert_resource(GameNetClient::from_launch(&launch))
        .init_state::<AppState>()
        .add_plugins(VoxelMaterialPlugin)
        .add_plugins(stagcrest_render::OutlineMaterialPlugin)
        .add_plugins(stagcrest_render::UnderwaterPlugin)
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
