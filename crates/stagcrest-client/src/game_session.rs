use crate::debug_overlay;
use crate::game::{AppState, GameplayState, ModContext};
use crate::inventory::{self, InventoryUiState};
use crate::minimap;
use crate::player::FlyCamera;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use stagcrest_mod_client::SEA_LEVEL;
use stagcrest_render::{spawn_block_outline, OutlineMaterial, UnderwaterEffect};

/// Marker for entities owned by the active in-world session (camera, HUD roots, lights, etc.).
#[derive(Component)]
pub struct GameSessionEntity;

/// Marks the player world camera entity (distinct from the UI overlay camera).
#[derive(Component)]
pub struct GameCamera;

pub struct GameSessionPlugin;

impl Plugin for GameSessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_game_session);
    }
}

/// Returns true when the player is in-world and not in the pause menu.
///
/// `State<GameplayState>` only exists while `AppState::InGame`; use `Option` so
/// run conditions are safe on menus and loading screens.
pub fn in_active_gameplay(
    app: Res<State<AppState>>,
    gameplay: Option<Res<State<GameplayState>>>,
) -> bool {
    *app.get() == AppState::InGame
        && gameplay.is_some_and(|state| *state.get() == GameplayState::Active)
}

/// Returns true when the player is in-world with the pause menu open.
pub fn in_paused_gameplay(
    app: Res<State<AppState>>,
    gameplay: Option<Res<State<GameplayState>>>,
) -> bool {
    *app.get() == AppState::InGame
        && gameplay.is_some_and(|state| *state.get() == GameplayState::Paused)
}

pub fn spawn_game_session(
    mut commands: Commands,
    existing: Query<Entity, With<GameSessionEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<OutlineMaterial>>,
    theme: Res<crate::ui::UiTheme>,
    mod_ctx: Option<Res<ModContext>>,
    atlas: Option<Res<stagcrest_render::BlockAtlasResource>>,
    mut images: ResMut<Assets<Image>>,
    icons: Option<Res<crate::block_icons::BlockIconCache>>,
) {
    if !existing.is_empty() {
        tracing::warn!(
            "spawn_game_session called while {} session entities already exist; skipping duplicate spawn",
            existing.iter().count()
        );
        return;
    }

    commands.insert_resource(GlobalAmbientLight {
        brightness: 800.0,
        ..default()
    });

    commands.spawn((
        GameSessionEntity,
        DirectionalLight {
            illuminance: 10_000.0,
            ..default()
        },
    ));

    let look_y = (SEA_LEVEL + 8) as f32;
    let spawn_y = (SEA_LEVEL + 16) as f32;
    let transform =
        Transform::from_xyz(8.0, spawn_y, 8.0).looking_at(Vec3::new(0.0, look_y, 0.0), Vec3::Y);
    commands.spawn((
        GameSessionEntity,
        GameCamera,
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        transform,
        FlyCamera::default(),
        UnderwaterEffect::default(),
        DistanceFog {
            color: Color::srgba(0.1, 0.3, 0.5, 0.0),
            falloff: FogFalloff::Linear {
                start: 4.0,
                end: 24.0,
            },
            ..default()
        },
    ));

    let outline = spawn_block_outline(&mut commands, &mut meshes, &mut materials);
    commands.entity(outline).insert(GameSessionEntity);

    debug_overlay::spawn_debug_overlay(&mut commands, &theme);
    minimap::spawn_minimap(&mut commands, &theme, &mut images);
    inventory::setup_inventory(
        &mut commands,
        &theme,
        mod_ctx.as_deref(),
        atlas.as_deref(),
        &mut images,
        icons.as_deref(),
    );
}

pub(crate) fn teardown_game_session_entities(
    commands: &mut Commands,
    session_entities: &Query<Entity, With<GameSessionEntity>>,
    debug_roots: &Query<Entity, With<debug_overlay::DebugOverlayRoot>>,
    minimap_roots: &Query<Entity, With<minimap::MinimapRoot>>,
    chunk_entities: &Query<Entity, With<stagcrest_render::ChunkEntityMarker>>,
    ui: &mut InventoryUiState,
    inventory_screens: &Query<Entity, With<crate::inventory::screen::InventoryScreenRoot>>,
) {
    for entity in session_entities {
        commands.entity(entity).despawn();
    }
    debug_overlay::cleanup_debug_overlay(commands, debug_roots);
    minimap::cleanup_minimap(commands, minimap_roots);
    inventory::teardown_inventory(commands, ui, inventory_screens);
    stagcrest_render::despawn_chunk_entities(commands, chunk_entities);
}
