use crate::{block_outline, debug_overlay, player, targeting};
use crate::terrain_queue::{
    terrain_apply, terrain_dispatch, terrain_poll_tasks, TerrainBlocks, TerrainGenQueue,
    TerrainStreamState,
};
use bevy::prelude::*;
use stagcrest_mod_host::{
    world_chunk_y_bounds, BlockRegistry, ModHost, ModelRegistry, TextureAtlas, WorldGenState,
    SEA_LEVEL,
};
use stagcrest_protocol::ChunkPos;
use stagcrest_circuit::CircuitWorld;
use stagcrest_render::{
    spawn_block_outline, BlockAtlasResource, MeshCacheResource, OutlineMaterial, VoxelCamera,
    VoxelRenderPlugin,
};
use stagcrest_world::World as StagcrestWorld;

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
    pub host: ModHost,
    pub atlas: TextureAtlas,
    pub registry: BlockRegistry,
    pub models: ModelRegistry,
}

#[derive(Resource)]
pub struct StagcrestWorldResource(pub StagcrestWorld);

#[derive(Resource, Default)]
pub struct CircuitResource(pub CircuitWorld);

#[derive(Resource, Default)]
pub struct TerrainGen(pub WorldGenState);

#[derive(Resource, Default)]
struct LastStreamCenter(Option<ChunkPos>);

const MESH_REBUILD_BUDGET: usize = 32;

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
            .init_resource::<CircuitResource>()
            .init_resource::<TerrainGen>()
            .init_resource::<LastStreamCenter>()
            .init_resource::<TerrainGenQueue>()
            .init_resource::<TerrainStreamState>()
            .init_resource::<MeshCacheResource>()
            .init_resource::<VoxelCamera>()
            .init_resource::<targeting::BlockTarget>()
            .add_plugins(VoxelRenderPlugin)
            .add_systems(
                Update,
                (
                    player::camera_system.run_if(in_state(AppState::InGame)),
                    targeting::update_block_target.run_if(in_state(AppState::InGame)),
                    player::block_interaction.run_if(in_state(AppState::InGame)),
                    block_outline::sync_block_outline.run_if(in_state(AppState::InGame)),
                    chunk_streaming.run_if(in_state(AppState::InGame)),
                    terrain_dispatch.run_if(in_state(AppState::InGame)),
                    terrain_poll_tasks.run_if(in_state(AppState::InGame)),
                    terrain_apply.run_if(in_state(AppState::InGame)),
                    rebuild_meshes.run_if(in_state(AppState::InGame)),
                    update_voxel_camera.run_if(in_state(AppState::InGame)),
                    circuit_tick.run_if(in_state(AppState::InGame)),
                ),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                (setup_game_camera, init_circuit_on_enter, setup_block_outline),
            )
            .add_systems(OnEnter(AppState::MainMenu), cleanup_game_session);
    }
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
    commands.remove_resource::<StagcrestWorldResource>();
    commands.remove_resource::<TerrainGen>();
    commands.remove_resource::<CircuitResource>();
    commands.remove_resource::<BlockAtlasResource>();
    commands.remove_resource::<crate::block_icons::BlockIconCache>();
    commands.remove_resource::<LastStreamCenter>();
    commands.remove_resource::<TerrainGenQueue>();
    commands.remove_resource::<TerrainStreamState>();
    commands.remove_resource::<TerrainBlocks>();
    commands.remove_resource::<targeting::BlockTarget>();
    // MeshCacheResource is re-inited by VoxelRenderPlugin; reset it for the next session.
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
    ));
}

fn init_circuit_on_enter(
    world: Res<StagcrestWorldResource>,
    mod_ctx: Option<Res<ModContext>>,
    mut circuit: ResMut<CircuitResource>,
) {
    if let Some(ctx) = mod_ctx {
        stagcrest_circuit::init_circuit_blocks(&mut circuit.0, &world.0, &ctx.registry);
    }
}

fn chunk_streaming(
    mod_ctx: Option<Res<ModContext>>,
    config: Res<GameConfig>,
    mut world: ResMut<StagcrestWorldResource>,
    mut terrain: ResMut<TerrainGen>,
    mut last_center: ResMut<LastStreamCenter>,
    mut queue: ResMut<TerrainGenQueue>,
    mut stream: ResMut<TerrainStreamState>,
    mut cache: ResMut<MeshCacheResource>,
    camera: Query<&Transform, With<player::FlyCamera>>,
) {
    let Some(_ctx) = mod_ctx else { return };
    let Ok(cam) = camera.single() else { return };
    let pos = cam.translation;
    let player_block = stagcrest_protocol::BlockPos::new(
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    );
    let center = player_block.chunk_pos();
    let y_bounds = world_chunk_y_bounds(terrain.0.config());
    let h_radius = config.render_distance;
    let v_radius = config.vertical_render_distance;
    let unload_h = h_radius + 2;
    let unload_v = v_radius + 2;

    stream.center_x = center.x;
    stream.center_y = center.y;
    stream.center_z = center.z;
    stream.valid = true;

    world
        .0
        .load_area_3d(center, h_radius, v_radius, y_bounds.clone());
    let removed = world
        .0
        .unload_far_chunks_3d(center, unload_h, unload_v);

    for chunk_pos in removed {
        cache.0.remove(chunk_pos);
        terrain.0.clear_chunk(chunk_pos);
        queue.cancel_chunk(chunk_pos);
    }

    if last_center.0 != Some(center) {
        queue.enqueue_area(
            &terrain.0,
            center,
            h_radius,
            v_radius,
            y_bounds,
            player_block,
        );
        last_center.0 = Some(center);
    }
}

fn rebuild_meshes(
    mod_ctx: Option<Res<ModContext>>,
    circuit: Option<Res<CircuitResource>>,
    mut world: ResMut<StagcrestWorldResource>,
    mut cache: ResMut<MeshCacheResource>,
) {
    let Some(ctx) = mod_ctx else { return };
    let dirty = world.0.take_dirty_chunks();
    if dirty.is_empty() {
        return;
    }
    let mut iter = dirty.into_iter();
    let to_rebuild: std::collections::HashSet<_> = iter.by_ref().take(MESH_REBUILD_BUDGET).collect();
    world.0.dirty_chunks.extend(iter);
    if to_rebuild.is_empty() {
        return;
    }
    let power = circuit
        .as_ref()
        .map(|r| &r.0 as &dyn stagcrest_mod_host::PowerLookup);
    cache.0.rebuild_dirty(&world.0, &ctx.registry, &ctx.models, power, to_rebuild);
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

fn circuit_tick(
    time: Res<Time>,
    mut accumulator: Local<f32>,
    mod_ctx: Option<Res<ModContext>>,
    mut world: ResMut<StagcrestWorldResource>,
    mut circuit: ResMut<CircuitResource>,
) {
    *accumulator += time.delta_secs();
    while *accumulator >= 0.1 {
        *accumulator -= 0.1;
        if let Some(ctx) = mod_ctx.as_ref() {
            circuit.0.tick(&mut world.0, &ctx.registry);
        }
    }
}
