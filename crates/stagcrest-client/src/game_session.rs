use crate::chat;
use crate::debug_overlay;
use crate::game::{AppState, GameplayState, ModContext};
use crate::graphics::{moon_light_bundle, sun_light_bundle};
use crate::inventory::{self, InventoryUiState};
use crate::minimap;
use crate::player::{
    player_camera_transform, player_eye_position, LocalPlayer, PlayerBodyState, PlayerController,
    FIRST_PERSON_DISTANCE, PLAYER_EYE_HEIGHT,
};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use stagcrest_mod_client::SEA_LEVEL;
use stagcrest_render::{
    main_camera_layers, spawn_block_outline, GraphicsSettings, MainViewCamera, OutlineMaterial,
    ReflectionPlaneHeight, SkyMaterial, UnderwaterEffect, VolumetricLightSettings,
};

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
    graphics: Res<GraphicsSettings>,
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

    // Sun first, moon second — stable entity ids for Bevy's directional-light sort tie-break.
    commands.spawn((GameSessionEntity, sun_light_bundle(&graphics)));
    commands.spawn((GameSessionEntity, moon_light_bundle(&graphics)));

    let look_y = (SEA_LEVEL + 8) as f32;
    let feet_y = (SEA_LEVEL + 16) as f32 - PLAYER_EYE_HEIGHT;
    let feet = Vec3::new(8.0, feet_y, 8.0);
    let look_target = Vec3::new(0.0, look_y, 0.0);
    let (spawn_yaw, spawn_pitch, _) = Transform::from_translation(player_eye_position(feet))
        .looking_at(look_target, Vec3::Y)
        .rotation
        .to_euler(EulerRot::YXZ);
    let camera_transform =
        player_camera_transform(feet, spawn_yaw, spawn_pitch, FIRST_PERSON_DISTANCE);

    commands.spawn((
        GameSessionEntity,
        LocalPlayer,
        PlayerBodyState::default(),
        Transform::from_translation(feet),
        Visibility::default(),
    ));

    commands.spawn((
        GameSessionEntity,
        GameCamera,
        MainViewCamera,
        ReflectionPlaneHeight(feet_y.floor() + 0.9),
        main_camera_layers(),
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        camera_transform,
        PlayerController {
            yaw: spawn_yaw,
            pitch: spawn_pitch,
            ..default()
        },
        SkyMaterial::default(),
        UnderwaterEffect::default(),
        VolumetricLightSettings::default(),
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
    chat::spawn_chat_overlay(&mut commands, &theme);
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
