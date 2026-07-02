use crate::chunk_streaming::{on_chunk_unloaded_remove_biome, BiomeGridCache};
use crate::environment::{self, PlayerEnvironment};
use crate::mesh_scheduler::{
    mesh_commit_meshes, mesh_dispatch, mesh_drain_dirty, mesh_poll,
    mesh_rebuild_after_atlas_change, mesh_recover_unmeshed, MeshScheduler,
};
use crate::minimap::MinimapDecodeQueue;
use crate::net_client::GameNetClient;
use crate::world_replica::{apply_power_batch, CircuitPowerOverlay, WorldReplica};
use crate::world_select::SelectedWorld;
use crate::{block_outline, debug_overlay, minimap, player, targeting};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use stagcrest_mod_client::{
    BiomeRegistryClient, BlockRegistry, ColormapSet, ModelRegistry, TextureAtlas, SEA_LEVEL,
};
use stagcrest_net::ServerMessage;
use stagcrest_render::{
    spawn_block_outline, BlockAtlasResource, MeshCacheResource, OutlineMaterial, UnderwaterEffect,
    VoxelRenderPlugin,
};

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    WorldSelect,
    Connect,
    Loading,
    InGame,
    Paused,
}

#[derive(Resource)]
pub struct ModContext {
    pub registry: BlockRegistry,
    pub atlas: TextureAtlas,
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
            .init_resource::<MeshScheduler>()
            .init_resource::<targeting::BlockTarget>()
            .init_resource::<CircuitPowerOverlay>()
            .init_resource::<BiomeGridCache>()
            .add_observer(on_chunk_unloaded_remove_biome)
            .add_plugins(VoxelRenderPlugin)
            .add_systems(
                Update,
                (
                    net_poll_system.run_if(in_state(AppState::InGame)),
                    net_ping_system
                        .after(net_poll_system)
                        .run_if(in_state(AppState::InGame)),
                    send_pose_system
                        .after(net_poll_system)
                        .run_if(in_state(AppState::InGame)),
                    player::camera_system.run_if(in_state(AppState::InGame)),
                    targeting::update_block_target.run_if(in_state(AppState::InGame)),
                    player::block_interaction.run_if(in_state(AppState::InGame)),
                    block_outline::sync_block_outline.run_if(in_state(AppState::InGame)),
                    mesh_drain_dirty
                        .after(net_poll_system)
                        .run_if(in_state(AppState::InGame)),
                    mesh_rebuild_after_atlas_change
                        .after(mesh_drain_dirty)
                        .run_if(in_state(AppState::InGame)),
                    mesh_dispatch
                        .after(net_poll_system)
                        .after(mesh_rebuild_after_atlas_change)
                        .run_if(in_state(AppState::InGame)),
                    mesh_poll
                        .after(mesh_dispatch)
                        .run_if(in_state(AppState::InGame)),
                    mesh_commit_meshes
                        .after(mesh_poll)
                        .run_if(in_state(AppState::InGame)),
                    mesh_recover_unmeshed
                        .after(net_poll_system)
                        .before(mesh_dispatch)
                        .run_if(in_state(AppState::InGame)),
                    update_fluid_anim.run_if(in_state(AppState::InGame)),
                    environment::update_player_environment.run_if(in_state(AppState::InGame)),
                    sync_underwater_vision
                        .after(environment::update_player_environment)
                        .run_if(in_state(AppState::InGame)),
                ),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                (setup_game_camera, setup_block_outline),
            )
            .add_systems(OnEnter(AppState::MainMenu), cleanup_game_session)
            .add_systems(OnEnter(AppState::WorldSelect), cleanup_game_session)
            .add_systems(OnEnter(AppState::Connect), cleanup_game_session);
    }
}

pub(crate) fn net_poll_system(
    mut net: ResMut<GameNetClient>,
    mut world: ResMut<WorldReplica>,
    mut mesh_scheduler: ResMut<MeshScheduler>,
    mut mesh_cache: ResMut<MeshCacheResource>,
    mut biome_cache: ResMut<BiomeGridCache>,
    mut power_overlay: ResMut<CircuitPowerOverlay>,
    mut decode_queue: Option<ResMut<MinimapDecodeQueue>>,
    mut commands: Commands,
) {
    for msg in net.poll() {
        match msg {
            ServerMessage::CircuitPowerBatch(batch) => {
                apply_power_batch(&mut power_overlay, batch.updates, &mut mesh_scheduler);
            }
            ServerMessage::MapChunkSnapshot(snap) => {
                if let Some(q) = decode_queue.as_mut() {
                    q.enqueue(snap.mx, snap.mz, snap.compressed);
                }
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

fn send_pose_system(
    mut net: ResMut<GameNetClient>,
    camera: Query<&Transform, With<player::FlyCamera>>,
) {
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
    outline_entities: Query<Entity, With<stagcrest_render::BlockOutlineMarker>>,
    debug_roots: Query<Entity, With<debug_overlay::DebugOverlayRoot>>,
    minimap_roots: Query<Entity, With<minimap::MinimapRoot>>,
    cameras: Query<Entity, With<player::FlyCamera>>,
    chunk_entities: Query<Entity, With<stagcrest_render::ChunkEntityMarker>>,
) {
    *selected_world = SelectedWorld::default();
    if mod_ctx.is_none() {
        return;
    }

    block_outline::despawn_block_outline(&mut commands, &outline_entities);
    stagcrest_render::despawn_chunk_entities(&mut commands, &chunk_entities);
    debug_overlay::cleanup_debug_overlay(&mut commands, &debug_roots);
    minimap::cleanup_minimap(&mut commands, &minimap_roots);
    for cam in &cameras {
        commands.entity(cam).despawn();
    }
    commands.remove_resource::<ModContext>();
    commands.remove_resource::<WorldReplica>();
    commands.remove_resource::<BlockAtlasResource>();
    commands.remove_resource::<crate::block_icons::BlockIconCache>();
    commands.remove_resource::<MeshScheduler>();
    commands.remove_resource::<MeshCacheResource>();
    commands.remove_resource::<BiomeGridCache>();
    commands.remove_resource::<targeting::BlockTarget>();
    commands.remove_resource::<PlayerEnvironment>();
    commands.remove_resource::<CircuitPowerOverlay>();
}

fn setup_block_outline(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<OutlineMaterial>>,
) {
    spawn_block_outline(&mut commands, &mut meshes, &mut materials);
}

fn setup_game_camera(mut commands: Commands) {
    commands.insert_resource(GlobalAmbientLight {
        brightness: 800.0,
        ..default()
    });
    commands.spawn(DirectionalLight {
        illuminance: 10_000.0,
        ..default()
    });

    let look_y = (SEA_LEVEL + 8) as f32;
    let spawn_y = (SEA_LEVEL + 16) as f32;
    let transform =
        Transform::from_xyz(8.0, spawn_y, 8.0).looking_at(Vec3::new(0.0, look_y, 0.0), Vec3::Y);
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        transform,
        player::FlyCamera::default(),
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
}

fn update_fluid_anim(time: Res<Time>, atlas: Option<ResMut<BlockAtlasResource>>) {
    if let Some(mut atlas) = atlas {
        atlas.fluid_anim.w = time.elapsed_secs();
    }
}

fn sync_underwater_vision(
    env: Res<PlayerEnvironment>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut camera: Query<(&mut UnderwaterEffect, &mut DistanceFog), With<player::FlyCamera>>,
) {
    let Ok((mut effect, mut fog)) = camera.single_mut() else {
        return;
    };

    let s = env.submersion;
    effect.set(env.water_tint, s);

    clear.0 = Color::srgba(env.sky_color[0], env.sky_color[1], env.sky_color[2], 1.0);

    if s > 0.01 {
        fog.color = Color::srgba(
            env.water_tint[0],
            env.water_tint[1],
            env.water_tint[2],
            s * 0.85,
        );
        ambient.brightness = 200.0 * (1.0 - s) + 80.0 * s;
    } else {
        fog.color = Color::srgba(
            env.fog_color[0],
            env.fog_color[1],
            env.fog_color[2],
            env.fog_density,
        );
        ambient.brightness = 800.0;
    }
}
