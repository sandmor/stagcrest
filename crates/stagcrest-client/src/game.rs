use crate::celestial_light::CelestialWeights;
use crate::chat::{self, chat_input_open, push_chat_line, ChatUiState};
use crate::chunk_streaming::{on_chunk_unloaded_remove_biome, BiomeGridCache};
use crate::environment::{self, PlayerEnvironment};
use crate::game_session::{self, in_active_gameplay, GameCamera};
use crate::graphics::{directional_light_gpu_indices, MoonLight, SunLight};
use crate::inventory::inventory_open;
use crate::mesh_scheduler::{
    mesh_commit_meshes, mesh_dispatch, mesh_drain_dirty, mesh_poll,
    mesh_rebuild_after_atlas_change, mesh_recover_unmeshed, MeshScheduler,
};
use crate::minimap::MinimapDecodeQueue;
use crate::net_client::GameNetClient;
use crate::world_replica::{apply_power_batch, CircuitPowerOverlay, WorldReplica};
use crate::world_select::SelectedWorld;
use crate::world_time::{self, WorldTime};
use crate::{block_outline, player, targeting};
use bevy::prelude::*;
use stagcrest_mod_client::{
    BiomeRegistryClient, BlockRegistry, ColormapSet, ModelRegistry, TextureAtlas,
};
use stagcrest_net::ServerMessage;
use stagcrest_protocol::TimeOfDay;
use stagcrest_render::{
    BlockAtlasResource, GraphicsSettings, Medium, MeshCacheResource, SceneLighting,
    SceneLightingUniform, SkyMaterial, SkyPalette, UnderwaterEffect, VoxelMaterial,
    VoxelRenderPlugin,
};

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    ResourcePackSetup,
    ResourcePacks,
    GraphicsSettings,
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
            .add_systems(Startup, init_graphics_from_client_settings)
            .add_systems(
                Update,
                (
                    stagcrest_render::ensure_scene_reflection_images,
                    stagcrest_render::sync_water_reflection_bindings,
                )
                    .chain()
                    .before(stagcrest_render::SyncChunkMeshesSet),
            )
            .add_systems(
                Update,
                (
                    net_poll_system.run_if(in_active_gameplay),
                    net_ping_system
                        .after(net_poll_system)
                        .run_if(in_state(AppState::InGame)),
                    send_pose_system
                        .after(player::camera_system)
                        .after(net_poll_system)
                        .run_if(in_active_gameplay),
                    player::capture_cursor
                        .run_if(in_active_gameplay)
                        .run_if(not(inventory_open))
                        .run_if(not(chat_input_open)),
                    player::camera_system.run_if(in_active_gameplay),
                    player::sync_local_player_view_layers
                        .after(player::camera_system)
                        .run_if(in_active_gameplay),
                    targeting::update_block_target
                        .after(player::capture_cursor)
                        .run_if(in_active_gameplay),
                    player::block_interaction
                        .after(targeting::update_block_target)
                        .run_if(in_active_gameplay),
                    block_outline::sync_block_outline
                        .after(targeting::update_block_target)
                        .run_if(in_active_gameplay),
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
                    world_time::update_world_time.run_if(in_active_gameplay),
                    environment::update_player_environment.run_if(in_active_gameplay),
                    sync_scene_lighting
                        .after(world_time::update_world_time)
                        .after(environment::update_player_environment)
                        .run_if(in_active_gameplay),
                    sync_reflection_sky_lighting
                        .after(sync_scene_lighting)
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

fn init_graphics_from_client_settings(
    client: Option<Res<crate::client_content::ClientContentSettings>>,
    mut graphics: ResMut<GraphicsSettings>,
) {
    if let Some(client) = client {
        *graphics = crate::graphics::graphics_settings_from_section(client.0.graphics());
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
    mut entity_events: MessageWriter<crate::entity_render::EntityNetEvent>,
    mut commands: Commands,
) {
    use crate::entity_render::EntityNetEvent;
    for msg in net.poll() {
        match msg {
            ServerMessage::EntitySpawn(s) => {
                entity_events.write(EntityNetEvent::Spawn(s));
            }
            ServerMessage::EntityUpdate(u) => {
                entity_events.write(EntityNetEvent::Update(u));
            }
            ServerMessage::EntityDespawn(id) => {
                entity_events.write(EntityNetEvent::Despawn(id));
            }
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

fn send_pose_system(
    mut net: ResMut<GameNetClient>,
    player: Query<&Transform, With<crate::player::LocalPlayer>>,
    controller: Query<&crate::player::PlayerController, With<GameCamera>>,
) {
    let Ok(transform) = player.single() else {
        return;
    };
    let Ok(ctrl) = controller.single() else {
        return;
    };
    net.send_pose(
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
        ctrl.yaw,
        ctrl.pitch,
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
    reflection_cameras: Query<Entity, With<stagcrest_render::PlanarReflectionCamera>>,
    inventory_screens: Query<Entity, With<crate::inventory::screen::InventoryScreenRoot>>,
    mut inventory_ui: ResMut<crate::inventory::InventoryUiState>,
) {
    *selected_world = SelectedWorld::default();
    if mod_ctx.is_none() {
        return;
    }

    for entity in &reflection_cameras {
        commands.entity(entity).despawn();
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

fn sync_underwater_vision(
    env: Res<PlayerEnvironment>,
    lighting: Res<SceneLighting>,
    graphics: Res<GraphicsSettings>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut camera: Query<(&mut UnderwaterEffect, &mut DistanceFog), With<GameCamera>>,
) {
    let Ok((mut effect, mut fog)) = camera.single_mut() else {
        return;
    };

    let s = env.submersion;
    effect.set(env.water_tint, s);

    let palette = lighting.palette;
    let day = lighting.uniform.day_factor();
    let night = 1.0 - day;
    let clear_rgb = palette
        .zenith_color
        .lerp(Vec3::new(0.02, 0.02, 0.08), night);
    clear.0 = Color::srgba(clear_rgb.x, clear_rgb.y, clear_rgb.z, 1.0);

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
    } else if graphics.fog {
        let fog_rgb = palette.fog_color;
        fog.color = Color::srgba(fog_rgb.x, fog_rgb.y, fog_rgb.z, env.fog_density);
        ambient.brightness = 800.0 * ambient_strength;
        ambient.color = Color::linear_rgb(ambient_rgb.x, ambient_rgb.y, ambient_rgb.z);
    } else {
        fog.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
        ambient.brightness = 800.0 * ambient_strength;
        ambient.color = Color::linear_rgb(ambient_rgb.x, ambient_rgb.y, ambient_rgb.z);
    }
}

/// Syncs GPU scene lighting and orients sun/moon Bevy directional lights for cascaded shadows.
///
/// Voxels/sky use position directions in [`SceneLightingUniform`]. Bevy lights use incoming
/// light directions (`sun_light_dir` / `moon_light_dir`). See `stagcrest-render/docs/RENDERING.md` §5.5.
fn sync_scene_lighting(
    world_time: Res<WorldTime>,
    env: Res<PlayerEnvironment>,
    graphics: Res<GraphicsSettings>,
    mut lighting: ResMut<SceneLighting>,
    mut atlas: Option<ResMut<BlockAtlasResource>>,
    mut materials: ResMut<Assets<VoxelMaterial>>,
    mut water_materials: ResMut<Assets<stagcrest_render::WaterMaterial>>,
    mut sky: Query<&mut SkyMaterial, With<GameCamera>>,
    mut volumetric: Query<&mut stagcrest_render::VolumetricLightSettings, With<GameCamera>>,
    game_camera: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    mut sun: Query<
        (Entity, &mut DirectionalLight, &mut Transform),
        (
            With<SunLight>,
            Without<MoonLight>,
            With<game_session::GameSessionEntity>,
        ),
    >,
    mut moon: Query<
        (Entity, &mut DirectionalLight, &mut Transform),
        (
            With<MoonLight>,
            Without<SunLight>,
            With<game_session::GameSessionEntity>,
        ),
    >,
) {
    let (sun_dir, moon_dir, day_factor, cycle, sun_light_dir, moon_light_dir, sun_disc, moon_disc) =
        if world_time.initialized {
            (
                world_time.sun_dir(),
                world_time.moon_dir(),
                world_time.day_factor(),
                world_time.cycle(),
                world_time.sun_light_dir(),
                world_time.moon_light_dir(),
                world_time.sun_disc_factor(),
                world_time.moon_disc_factor(),
            )
        } else {
            let t = TimeOfDay::default();
            let s = t.sun_dir();
            let m = t.moon_dir();
            let l = t.sun_light_dir();
            let ml = t.moon_light_dir();
            (
                Vec3::new(s.x, s.y, s.z),
                Vec3::new(m.x, m.y, m.z),
                t.day_factor(),
                t.cycle(),
                Vec3::new(l.x, l.y, l.z),
                Vec3::new(ml.x, ml.y, ml.z),
                t.sun_disc_factor(),
                t.moon_disc_factor(),
            )
        };

    let medium = if env.submersion > 0.5 {
        Medium::Water
    } else {
        Medium::Air
    };

    let palette =
        SkyPalette::from_time_and_biome(&world_time.display, env.sky_color, env.fog_color);
    lighting.palette = palette;

    lighting.uniform = SceneLightingUniform::from_scene(
        sun_dir,
        moon_dir,
        day_factor,
        cycle,
        &palette,
        env.submersion,
        medium,
        graphics.reflection_tier,
    );
    lighting.uniform.sun_position_dir.w = sun_disc;
    lighting.uniform.moon_position_dir.w = moon_disc;
    lighting.uniform.sky_params.w = world_time.display.seconds() as f32;

    let water = Color::linear_rgb(env.water_tint[0], env.water_tint[1], env.water_tint[2]);
    if let Some(atlas) = atlas.as_mut() {
        atlas.water_tint = water;
    }
    let water_linear = water.to_linear();

    if graphics.volumetric_light {
        if let Ok(mut vol) = volumetric.single_mut() {
            let mut screen = Vec2::new(0.5, 0.5);
            if let Ok((camera, transform)) = game_camera.single() {
                let sun_world = transform.translation() + sun_dir.normalize_or_zero() * 10_000.0;
                if let Some(ndc) = camera.world_to_ndc(transform, sun_world) {
                    if ndc.z >= 0.0 {
                        screen = Vec2::new(ndc.x * 0.5 + 0.5, 1.0 - (ndc.y * 0.5 + 0.5));
                    }
                }
            }
            let twilight = day_factor * (1.0 - day_factor) * 4.0;
            let strength = twilight.mul_add(0.55, day_factor * 0.12);
            vol.set(screen, strength);
        }
    } else if let Ok(mut vol) = volumetric.single_mut() {
        vol.set(Vec2::ZERO, 0.0);
    }

    let weights = CelestialWeights::compute(graphics.tier, day_factor, sun_disc, moon_disc);

    let mut sun_entity = None;
    let mut moon_entity = None;
    let mut gpu_lights = Vec::with_capacity(2);

    if let Ok((entity, mut light, mut transform)) = sun.single_mut() {
        orient_directional_light(&mut transform, sun_light_dir);
        light.shadow_maps_enabled = weights.sun_shadow_gpu;
        light.color = Color::linear_rgb(
            palette.sun_color.x,
            palette.sun_color.y,
            palette.sun_color.z,
        );
        let base_lux = palette.sun_color.length().mul_add(8_000.0, 2_000.0);
        light.illuminance = base_lux * weights.sun_disc;
        sun_entity = Some(entity);
        gpu_lights.push((entity, weights.sun_shadow_gpu));
    }

    if let Ok((entity, mut light, mut transform)) = moon.single_mut() {
        orient_directional_light(&mut transform, moon_light_dir);
        light.shadow_maps_enabled = weights.moon_shadow_gpu;
        light.color = Color::linear_rgb(
            palette.moon_color.x,
            palette.moon_color.y,
            palette.moon_color.z,
        );
        let base_lux = palette.moon_color.length().mul_add(4_000.0, 500.0);
        light.illuminance = base_lux * weights.moon_disc;
        moon_entity = Some(entity);
        gpu_lights.push((entity, weights.moon_shadow_gpu));
    }

    let indices = directional_light_gpu_indices(&gpu_lights);
    let sun_index = sun_entity
        .and_then(|entity| indices.get(&entity).copied())
        .unwrap_or(0);
    let moon_index = moon_entity
        .and_then(|entity| indices.get(&entity).copied())
        .unwrap_or(1);
    lighting.uniform.shadow_params = Vec4::new(
        sun_index as f32,
        moon_index as f32,
        weights.sun_shadow,
        weights.moon_shadow,
    );

    if let Ok(mut sky_mat) = sky.single_mut() {
        sky_mat.scene_lighting = lighting.uniform;
    }
    for (_, mat) in materials.iter_mut() {
        mat.scene_lighting = lighting.uniform;
        mat.water_tint = water_linear;
    }
    for (_, mat) in water_materials.iter_mut() {
        mat.scene_lighting = lighting.uniform;
        mat.water_tint = water_linear;
    }
}

fn orient_directional_light(transform: &mut Transform, light_dir: Vec3) {
    if light_dir.length_squared() > 0.001 {
        *transform = Transform::from_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, light_dir));
    }
}

fn sync_reflection_sky_lighting(
    lighting: Res<SceneLighting>,
    mut reflection_sky: Query<
        &mut SkyMaterial,
        (
            With<stagcrest_render::PlanarReflectionCamera>,
            Without<GameCamera>,
        ),
    >,
) {
    let uniform = lighting.uniform;
    for mut sky in &mut reflection_sky {
        sky.scene_lighting = uniform;
    }
}
