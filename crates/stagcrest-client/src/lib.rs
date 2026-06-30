use bevy::prelude::*;
use bevy::window::WindowResolution;
use stagcrest_render::UnderwaterPlugin;

use camera::UiCameraPlugin;

pub mod block_icons;
pub mod block_outline;
pub mod camera;
pub mod chunk_streaming;
pub mod debug_overlay;
pub mod environment;
pub mod game;
pub mod gpu_chunk_scheduler;
pub mod inventory;
pub mod loading;
pub mod logging;
pub mod menu;
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
            resolution: WindowResolution::new(1280, 720),
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
        .add_plugins(stagcrest_render::OutlineMaterialPlugin)
        .add_plugins(UnderwaterPlugin)
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
