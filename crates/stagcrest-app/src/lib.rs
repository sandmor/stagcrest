use bevy::prelude::*;
use stagcrest_render::{VoxelMaterial, VoxelMaterialPlugin};

pub mod block_icons;
pub mod block_outline;
pub mod debug_overlay;
pub mod game;
pub mod inventory;
pub mod loading;
pub mod menu;
pub mod pause;
pub mod player;
pub mod targeting;
pub mod ui;

pub use game::AppState;

pub fn run_app() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        tracing_subscriber::fmt::init();
    }

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Stagcrest".into(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
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

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_start() {
    console_error_panic_hook::set_once();
    run_app();
}
