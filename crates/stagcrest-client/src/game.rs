use crate::chunk_streaming::{on_chunk_unloaded_remove_biome, BiomeGridCache};
use crate::chat::{self, chat_input_open, push_chat_line, ChatUiState};
use crate::environment::{self, PlayerEnvironment};
use crate::game_session::{self, in_active_gameplay, GameCamera};
use crate::world_time::{self, WorldTime};
use crate::inventory::inventory_open;
use crate::mesh_scheduler::{
    mesh_commit_meshes, mesh_dispatch, mesh_drain_dirty, mesh_poll,
    mesh_rebuild_after_atlas_change, mesh_recover_unmeshed, MeshScheduler,
};
use crate::minimap::MinimapDecodeQueue;
use crate::net_client::GameNetClient;
use crate::world_replica::{apply_power_batch, CircuitPowerOverlay, WorldReplica};
use crate::world_select::SelectedWorld;
use crate::{block_outline, player, targeting};
use bevy::prelude::*;
use stagcrest_mod_client::{
    BiomeRegistryClient, BlockRegistry, ColormapSet, ModelRegistry, TextureAtlas,
};
use stagcrest_net::ServerMessage;
use stagcrest_render::{
    BlockAtlasResource, Medium, MeshCacheResource, SceneLighting, SceneLightingUniform,
    SkyMaterial, UnderwaterEffect, VoxelMaterial, VoxelRenderPlugin,
};

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    ResourcePackSetup,
    ResourcePacks,
    WorldSelect,
    Connect,
    Loading,
    InGame,
}

#[derive(SubStates, Clone, Copy, Default, Eq, PartialEq, Hash, Debug)]
#[source(AppState = AppState::InGame)]
pub enum GameplayState {
    #[default]
    Active,
    Paused,
}

#[derive(Resource)]
pub struct ModContext {
    pub registry: BlockRegistry,
    pub atlases: Vec<TextureAtlas>,
    pub models: ModelRegistry,
    pub colormaps: ColormapSet,
    pub biomes: BiomeRegistryClient,
}

#[derive(Resource)]
pub struct GameConfig {
    pub render_distance: i32,
    pub vertical_render_distance: i32,
    pub world_seed: u64,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            render_distance: 8,
            vertical_render_distance: 4,
            world_seed: 42,
        }
    }
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameConfig>()
            .init_resource::<PlayerEnvironment>()
            .init_resource::<WorldTime>()
            .init_resource::<MeshScheduler>()
            .init_resource::<targeting::BlockTarget>()
            .init_resource::<CircuitPowerOverlay>()
            .init_resource::<BiomeGridCache>()
            .add_observer(on_chunk_unloaded_remove_biome)
            .add_plugins(VoxelRenderPlugin)
            .add_systems(
                Update,
                (
                    net_poll_system.run_if(in_active_gameplay),
                    net_ping_system
                        .after(net_poll_system)
                        .run_if(in_state(AppState::InGame)),
                    send_pose_system
                        .after(net_poll_system)
                        .run_if(in_active_gameplay),
                    player::capture_cursor
                        .run_if(in_active_gameplay)
                        .run_if(not(inventory_open))
                        .run_if(not(chat_input_open)),
                    player::camera_system.run_if(in_active_gameplay),
                    targeting::update_block_target
                        .after(player::capture_cursor)
                        .run_if(in_active_gameplay),
                    player::block_interaction
                        .after(targeting::update_block_target)
                        .run_if(in_active_gameplay),
                    block_outline::sync_block_outline.run_if(in_active_gameplay),
                    mesh_drain_dirty
                        .after(net_poll_system)
                        .run_if(in_active_gameplay),
                    mesh_rebuild_after_atlas_change
                        .after(mesh_drain_dirty)
                        .run_if(in_active_gameplay),
                    mesh_dispatch
                        .after(net_poll_system)
                        .after(mesh_rebuild_after_atlas_change)
                        .run_if(in_active_gameplay),
                    mesh_poll.after(mesh_dispatch).run_if(in_active_gameplay),
                    mesh_commit_meshes
                        .after(mesh_poll)
                        .run_if(in_active_gameplay),
                    mesh_recover_unmeshed
                        .after(net_poll_system)
                        .before(mesh_dispatch)
                        .run_if(in_active_gameplay),
                    update_fluid_anim.run_if(in_active_gameplay),
                    world_time::update_world_time.run_if(in_active_gameplay),
                    environment::update_player_environment.run_if(in_active_gameplay),
                    sync_scene_lighting
                        .after(world_time::update_world_time)
                        .after(environment::update_player_environment)
                        .run_if(in_active_gameplay),
                    sync_underwater_vision
                        .after(sync_scene_lighting)
                        .run_if(in_active_gameplay),
                ),
            )
            .add_systems(OnEnter(AppState::MainMenu), cleanup_game_session)
            .add_systems(OnEnter(AppState::WorldSelect), cleanup_game_session)
            .add_systems(OnEnter(AppState::Connect), cleanup_game_session);
    }
}

pub(crate) fn net_poll_system(
    mut net: ResMut<GameNetClient>,
    mut chat: ResMut<ChatUiState>,
    mut world: ResMut<WorldReplica>,
    mut mesh_scheduler: ResMut<MeshScheduler>,
    mut mesh_cache: ResMut<MeshCacheResource>,
    mut biome_cache: ResMut<BiomeGridCache>,
    mut power_overlay: ResMut<CircuitPowerOverlay>,
    mut world_time: ResMut<WorldTime>,
    mut decode_queue: Option<ResMut<MinimapDecodeQueue>>,
    mut commands: Commands,
) {
    for msg in net.poll() {
        match msg {
            ServerMessage::CircuitPowerBatch(batch) => {
                apply_power_batch(&mut power_overlay, &batch, &mut mesh_scheduler);
            }
            ServerMessage::Chat(line) => {
                push_chat_line(&mut chat, line);
            }
            ServerMessage::MapChunkSnapshot(snap) => {
                if let Some(q) = decode_queue.as_mut() {
                    q.enqueue(snap.mx, snap.mz, snap.compressed);
                }
            }
            ServerMessage::WorldTime { time } => {
                world_time::apply_world_time_message(&mut world_time, time);
            }
            other => {
                world.apply_server_message(
                    other,
                    &mut mesh_scheduler,
                    &mut mesh_cache,
                    &mut biome_cache,
                    &mut power_overlay,
                    &mut commands,
                );
            }
        }
    }
}

fn net_ping_system(time: Res<Time>, mut net: ResMut<GameNetClient>) {
    net.maybe_send_ping(time.delta());
}

fn send_pose_system(mut net: ResMut<GameNetClient>, camera: Query<&Transform, With<GameCamera>>) {
    let Ok(transform) = camera.single() else {
        return;
    };
    let (yaw, pitch, _) = transform.rotation.to_euler(bevy::math::EulerRot::YXZ);
    net.send_pose(
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
        yaw,
        pitch,
    );
}

fn cleanup_game_session(
    mut commands: Commands,
    mut selected_world: ResMut<SelectedWorld>,
    mod_ctx: Option<Res<ModContext>>,
    session_entities: Query<Entity, With<game_session::GameSessionEntity>>,
    debug_roots: Query<Entity, With<crate::debug_overlay::DebugOverlayRoot>>,
    minimap_roots: Query<Entity, With<crate::minimap::MinimapRoot>>,
    chat_roots: Query<Entity, With<chat::ChatRoot>>,
    chunk_entities: Query<Entity, With<stagcrest_render::ChunkEntityMarker>>,
    inventory_screens: Query<Entity, With<crate::inventory::screen::InventoryScreenRoot>>,
    mut inventory_ui: ResMut<crate::inventory::InventoryUiState>,
) {
    *selected_world = SelectedWorld::default();
    if mod_ctx.is_none() {
        return;
    }

    game_session::teardown_game_session_entities(
        &mut commands,
        &session_entities,
        &debug_roots,
        &minimap_roots,
        &chunk_entities,
        &mut inventory_ui,
        &inventory_screens,
    );
    chat::cleanup_chat(&mut commands, &chat_roots);
    commands.remove_resource::<ModContext>();
    commands.remove_resource::<WorldReplica>();
    commands.remove_resource::<BlockAtlasResource>();
    commands.remove_resource::<MeshScheduler>();
    commands.remove_resource::<MeshCacheResource>();
    commands.remove_resource::<BiomeGridCache>();
    commands.remove_resource::<targeting::BlockTarget>();
    commands.remove_resource::<PlayerEnvironment>();
    commands.remove_resource::<WorldTime>();
    commands.remove_resource::<CircuitPowerOverlay>();
}

fn update_fluid_anim(time: Res<Time>, atlas: Option<ResMut<BlockAtlasResource>>) {
    if let Some(mut atlas) = atlas {
        atlas.fluid_anim.w = time.elapsed_secs();
    }
}

fn sync_underwater_vision(
    env: Res<PlayerEnvironment>,
    lighting: Res<SceneLighting>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut camera: Query<(&mut UnderwaterEffect, &mut DistanceFog), With<GameCamera>>,
) {
    let Ok((mut effect, mut fog)) = camera.single_mut() else {
        return;
    };

    let s = env.submersion;
    effect.set(env.water_tint, s);

    clear.0 = Color::srgba(env.sky_color[0], env.sky_color[1], env.sky_color[2], 1.0);

    let scene_ambient = lighting.uniform.ambient_color;
    let ambient_rgb = scene_ambient.truncate();
    let ambient_strength = scene_ambient.w;

    if s > 0.01 {
        fog.color = Color::srgba(
            env.water_tint[0],
            env.water_tint[1],
            env.water_tint[2],
            s * 0.85,
        );
        let air_brightness = 200.0 * ambient_strength * 2.5;
        let water_brightness = 80.0 * ambient_strength * 2.0;
        ambient.brightness = air_brightness * (1.0 - s) + water_brightness * s;
        ambient.color = Color::linear_rgb(ambient_rgb.x, ambient_rgb.y, ambient_rgb.z);
    } else {
        fog.color = Color::srgba(
            env.fog_color[0],
            env.fog_color[1],
            env.fog_color[2],
            env.fog_density,
        );
        ambient.brightness = 800.0 * ambient_strength;
        ambient.color = Color::linear_rgb(ambient_rgb.x, ambient_rgb.y, ambient_rgb.z);
    }
}

/// Syncs GPU scene lighting (position directions) and orients the Bevy sun for optional shadows.
///
/// Voxels/sky use `sun_dir` → `sun_position_dir` in shaders. Bevy uses `sun_light_dir()`
/// (incoming light). See `stagcrest-render/docs/scene_lighting_conventions.md`.
fn sync_scene_lighting(
    world_time: Res<WorldTime>,
    env: Res<PlayerEnvironment>,
    mut lighting: ResMut<SceneLighting>,
    mut atlas: Option<ResMut<BlockAtlasResource>>,
    mut materials: ResMut<Assets<VoxelMaterial>>,
    mut sky: Query<&mut SkyMaterial, With<GameCamera>>,
    mut sun: Query<
        (&mut DirectionalLight, &mut Transform),
        With<game_session::GameSessionEntity>,
    >,
) {
    if !world_time.initialized {
        return;
    }

    let medium = if env.submersion > 0.5 {
        Medium::Water
    } else {
        Medium::Air
    };

    lighting.uniform = SceneLightingUniform::from_scene(
        world_time.sun_dir(),
        world_time.moon_dir(),
        world_time.day_factor(),
        world_time.cycle(),
        env.sky_color,
        env.fog_color,
        env.water_tint,
        env.submersion,
        medium,
    );

    if let Ok(mut sky_mat) = sky.single_mut() {
        sky_mat.scene_lighting = lighting.uniform;
    }

    let water = Color::linear_rgb(env.water_tint[0], env.water_tint[1], env.water_tint[2]);
    if let Some(atlas) = atlas.as_mut() {
        atlas.water_tint = water;
    }
    let water_linear = water.to_linear();
    for (_, mat) in materials.iter_mut() {
        mat.scene_lighting = lighting.uniform;
        mat.water_tint = water_linear;
    }

    // Bevy directional light: shadow maps for non-voxel meshes only (see scene_lighting_conventions.md).
    let light_dir = world_time.sun_light_dir();
    for (mut light, mut transform) in &mut sun {
        light.illuminance = 4_000.0 + world_time.day_factor() * 12_000.0;
        light.shadow_maps_enabled = world_time.day_factor() > 0.25;
        if light_dir.length_squared() > 0.001 {
            *transform = Transform::from_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, light_dir));
        }
    }
}
