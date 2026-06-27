use crate::chunk_streaming::{
    on_chunk_unloaded_remove_biome, BiomeGridCache, ChunkUnloaded,
};
use crate::environment::{self, PlayerEnvironment};
use crate::mesh_scheduler::{
    mesh_commit_meshes, mesh_dispatch, mesh_drain_dirty, mesh_poll, mesh_recover_unmeshed,
    MeshScheduler,
};
use crate::net_client::GameNetClient;
use crate::world_replica::{apply_power_batch, CircuitPowerOverlay, WorldReplica};
use crate::{block_outline, debug_overlay, player, targeting};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use stagcrest_mod_client::{
    BiomeRegistryClient, BlockRegistry, ColormapSet, ModelRegistry, TextureAtlas, SEA_LEVEL,
};
use stagcrest_net::ServerMessage;
use stagcrest_render::{
    spawn_block_outline, BlockAtlasResource, MeshCacheResource, OutlineMaterial, UnderwaterEffect,
    VoxelCamera, VoxelRenderPlugin,
};

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
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
            .init_resource::<MeshCacheResource>()
            .init_resource::<VoxelCamera>()
            .init_resource::<targeting::BlockTarget>()
            .init_resource::<CircuitPowerOverlay>()
            .init_resource::<BiomeGridCache>()
            .add_event::<ChunkUnloaded>()
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
                    mesh_drain_dirty.run_if(in_state(AppState::InGame)),
                    mesh_dispatch.run_if(in_state(AppState::InGame)),
                    mesh_poll.run_if(in_state(AppState::InGame)),
                    mesh_commit_meshes
                        .before(stagcrest_render::sync_chunk_meshes)
                        .run_if(in_state(AppState::InGame)),
                    mesh_recover_unmeshed
                        .before(mesh_dispatch)
                        .run_if(in_state(AppState::InGame)),
                    update_voxel_camera.run_if(in_state(AppState::InGame)),
                    environment::update_player_environment
                        .after(update_voxel_camera)
                        .run_if(in_state(AppState::InGame)),
                    sync_underwater_vision
                        .after(environment::update_player_environment)
                        .run_if(in_state(AppState::InGame)),
                ),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                (setup_game_camera, setup_block_outline),
            )
            .add_systems(OnEnter(AppState::MainMenu), cleanup_game_session);
    }
}

fn net_poll_system(
    mut net: ResMut<GameNetClient>,
    mut world: ResMut<WorldReplica>,
    mut mesh_scheduler: ResMut<MeshScheduler>,
    mut mesh_cache: ResMut<MeshCacheResource>,
    mut biome_cache: ResMut<BiomeGridCache>,
    mut power_overlay: ResMut<CircuitPowerOverlay>,
    mut commands: Commands,
) {
    for msg in net.poll() {
        match msg {
            ServerMessage::CircuitPowerBatch(batch) => {
                apply_power_batch(&mut power_overlay, batch.updates, &mut mesh_scheduler);
            }
            other => {
                world.apply_server_message(
                    other,
                    &mut mesh_scheduler,
                    &mut mesh_cache.0,
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
    mod_ctx: Option<Res<ModContext>>,
    chunk_entities: Query<Entity, With<stagcrest_render::ChunkEntityMarker>>,
    outline_entities: Query<Entity, With<stagcrest_render::BlockOutlineMarker>>,
    debug_roots: Query<Entity, With<debug_overlay::DebugOverlayRoot>>,
    cameras: Query<Entity, With<player::FlyCamera>>,
) {
    if mod_ctx.is_none() {
        return;
    }

    stagcrest_render::despawn_chunk_entities(&mut commands, &chunk_entities);
    block_outline::despawn_block_outline(&mut commands, &outline_entities);
    debug_overlay::cleanup_debug_overlay(&mut commands, &debug_roots);
    for cam in &cameras {
        commands.entity(cam).despawn();
    }
    commands.remove_resource::<ModContext>();
    commands.remove_resource::<WorldReplica>();
    commands.remove_resource::<BlockAtlasResource>();
    commands.remove_resource::<crate::block_icons::BlockIconCache>();
    commands.remove_resource::<MeshScheduler>();
    commands.remove_resource::<BiomeGridCache>();
    commands.remove_resource::<targeting::BlockTarget>();
    commands.remove_resource::<PlayerEnvironment>();
    commands.remove_resource::<CircuitPowerOverlay>();
    commands.insert_resource(MeshCacheResource::default());
}

fn setup_block_outline(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<OutlineMaterial>>,
) {
    spawn_block_outline(&mut commands, &mut meshes, &mut materials);
}

fn setup_game_camera(mut commands: Commands) {
    commands.insert_resource(AmbientLight {
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

fn update_voxel_camera(
    mut voxel_cam: ResMut<VoxelCamera>,
    camera: Query<(&Transform, &Projection), With<player::FlyCamera>>,
) {
    let Ok((transform, projection)) = camera.single() else {
        return;
    };
    let proj = match projection {
        Projection::Perspective(p) => {
            glam::Mat4::perspective_rh(p.fov, p.aspect_ratio, p.near, p.far)
        }
        _ => glam::Mat4::IDENTITY,
    };
    let view = glam::Mat4::look_at_rh(
        glam::Vec3::new(
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
        ),
        glam::Vec3::new(
            transform.translation.x + transform.forward().x * 10.0,
            transform.translation.y + transform.forward().y * 10.0,
            transform.translation.z + transform.forward().z * 10.0,
        ),
        glam::Vec3::Y,
    );
    voxel_cam.view_proj = proj * view;
    voxel_cam.position = glam::Vec3::new(
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
    );
}

fn sync_underwater_vision(
    env: Res<PlayerEnvironment>,
    mut ambient: ResMut<AmbientLight>,
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
