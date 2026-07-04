use bevy::prelude::*;
use bevy::window::WindowResolution;
use stagcrest_render::UnderwaterPlugin;

use camera::UiCameraPlugin;

pub mod block_icons;
pub mod block_outline;
pub mod camera;
pub mod chat;
pub mod chunk_streaming;
pub mod client_content;
pub mod connect_screen;
pub mod debug_overlay;
pub mod environment;
pub mod game;
pub mod game_session;
pub mod inventory;
pub mod loading;
pub mod logging;
pub mod menu;
pub mod mesh_scheduler;
pub mod minimap;
pub mod net_client;
pub mod pause;
pub mod player;
pub mod player_profile;
pub mod resource_pack_setup;
pub mod resource_packs;
pub mod targeting;
pub mod ui;
pub mod world_replica;
pub mod world_select;

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
        .insert_resource(player_profile::PlayerProfile::default())
        .init_state::<AppState>()
        .add_sub_state::<game::GameplayState>()
        .add_plugins(stagcrest_render::OutlineMaterialPlugin)
        .add_plugins(UnderwaterPlugin)
        .add_plugins(MaterialPlugin::<stagcrest_render::OutlineMaterial>::default())
        .add_plugins(MaterialPlugin::<stagcrest_render::VoxelMaterial>::default())
        .add_plugins((
            menu::MenuPlugin,
            resource_pack_setup::ResourcePackSetupPlugin,
            resource_packs::ResourcePacksPlugin,
            connect_screen::ConnectScreenPlugin,
            world_select::WorldSelectPlugin,
            loading::LoadingPlugin,
            game::GamePlugin,
            game_session::GameSessionPlugin,
            chat::ChatPlugin,
            debug_overlay::DebugPlugin,
            minimap::MinimapPlugin,
            pause::PausePlugin,
            ui::UiPlugin,
            UiCameraPlugin,
        ))
        .run();
}
