use crate::game::{AppState, GameConfig, ModContext, StagcrestWorldResource, TerrainGen};
use crate::terrain_queue::{TerrainBlocks, TerrainGenQueue, TerrainStreamState};
#[cfg(target_arch = "wasm32")]
use crate::terrain_queue::{terrain_apply, terrain_dispatch, terrain_poll_tasks};
use bevy::prelude::*;
use stagcrest_mesh::MeshCache;
use stagcrest_mod_host::{
    world_chunk_y_bounds, ColormapSet, ColumnBlocks, ModHost, ModelRegistry, WorldGenState,
    WorldSeed, SEA_LEVEL,
};
use stagcrest_protocol::BlockPos;
use stagcrest_render::BlockAtlasResource;

pub struct LoadingPlugin;

#[derive(Resource, Default)]
struct LoadingState {
    started: bool,
    done: bool,
    error: Option<String>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource, Default)]
struct WebLoadingTask(Option<bevy::tasks::Task<Result<(ModHost, ColormapSet), String>>>);

#[derive(Component)]
struct LoadingRoot;

#[derive(Component)]
enum LoadingAction {
    BackToMenu,
}

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingState>()
            .add_systems(OnEnter(AppState::Loading), on_enter_loading)
            .add_systems(OnExit(AppState::Loading), cleanup_loading);

        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(
            Update,
            (
                load_mods_system,
                loading_ui,
                loading_button_system,
            )
                .run_if(in_state(AppState::Loading)),
        );

        #[cfg(target_arch = "wasm32")]
        app.add_systems(
            Update,
            (
                terrain_dispatch,
                terrain_poll_tasks,
                terrain_apply,
            )
                .run_if(in_state(AppState::Loading)),
        );

        #[cfg(target_arch = "wasm32")]
        app.init_resource::<WebLoadingTask>().add_systems(
            Update,
            (
                start_web_load_system,
                poll_web_load_system,
                loading_ui,
                loading_button_system,
            )
                .run_if(in_state(AppState::Loading)),
        );
    }
}

fn on_enter_loading(
    mut state: ResMut<LoadingState>,
    #[cfg(target_arch = "wasm32")] mut task: ResMut<WebLoadingTask>,
) {
    *state = LoadingState::default();
    #[cfg(target_arch = "wasm32")]
    {
        *task = WebLoadingTask::default();
    }
}

fn fluid_anim_uniform(
    registry: &stagcrest_mod_host::BlockRegistry,
    atlas_height: u32,
) -> Vec4 {
    let Some(tex_id) = registry.texture_by_name("stagcrest:water_still") else {
        return Vec4::ONE;
    };
    let anim = registry
        .texture_animation(tex_id)
        .cloned()
        .or_else(|| {
            registry.textures().find(|t| t.id == tex_id).and_then(|t| {
                stagcrest_mod_host::infer_vertical_strip_animation(t.width, t.height)
            })
        });
    let Some(anim) = anim else {
        return Vec4::ONE;
    };
    let frame_uv_step = anim.frame_height as f32 / atlas_height.max(1) as f32;
    let frametime_secs = (anim.frametime_ticks as f32 / 20.0).max(0.05);
    Vec4::new(
        anim.frame_count as f32,
        frame_uv_step,
        frametime_secs,
        0.0,
    )
}

fn apply_loaded_content(
    commands: &mut Commands,
    config: &GameConfig,
    mut host: ModHost,
    colormaps: ColormapSet,
) {
    let atlas = host.finalize_atlas();
    let grass_rgb = colormaps.default_grass_tint();
    let foliage_rgb = colormaps.default_foliage_tint();
    let water_rgb = colormaps.default_water_tint();
    let registry = std::mem::take(&mut host.registry);
    let fluid_anim = fluid_anim_uniform(&registry, atlas.height);
    let air = registry
        .block_by_name("stagcrest:air")
        .unwrap_or(stagcrest_protocol::BlockId(0));
    let column_blocks = ColumnBlocks::resolve(&registry, air);

    let world = StagcrestWorldResource(stagcrest_world::World::new(air));
    let terrain = WorldGenState::new(WorldSeed(config.world_seed));
    let spawn = BlockPos::new(8, SEA_LEVEL + 16, 8);
    let spawn_chunk = spawn.chunk_pos();
    let initial_h = config.render_distance.min(4);
    let initial_v = config.vertical_render_distance.min(4);
    let y_bounds = world_chunk_y_bounds(terrain.config());
    let mut queue = TerrainGenQueue::default();
    queue.enqueue_area(
        &terrain,
        spawn_chunk,
        initial_h,
        initial_v,
        y_bounds,
        spawn,
    );

    commands.insert_resource(ModContext {
        host,
        atlas: atlas.clone(),
        registry,
        models: ModelRegistry::new(),
    });
    commands.insert_resource(world);
    commands.insert_resource(TerrainGen(terrain));
    commands.insert_resource(TerrainBlocks(column_blocks));
    commands.insert_resource(queue);
    commands.insert_resource(TerrainStreamState {
        center_x: spawn_chunk.x,
        center_y: spawn_chunk.y,
        center_z: spawn_chunk.z,
        valid: true,
    });
    commands.insert_resource(BlockAtlasResource {
        atlas,
        grass_tint: Color::srgb(grass_rgb[0], grass_rgb[1], grass_rgb[2]),
        foliage_tint: Color::srgb(foliage_rgb[0], foliage_rgb[1], foliage_rgb[2]),
        water_tint: Color::srgb(water_rgb[0], water_rgb[1], water_rgb[2]),
        fluid_anim,
    });
    commands.insert_resource(stagcrest_render::MeshCacheResource(MeshCache::default()));
}

#[cfg(not(target_arch = "wasm32"))]
fn load_mods_system(
    mut state: ResMut<LoadingState>,
    mut commands: Commands,
    config: Res<GameConfig>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if state.started {
        if state.done && state.error.is_none() {
            next_state.set(AppState::InGame);
        }
        return;
    }
    state.started = true;

    let repo_root = std::path::Path::new(".");
    match stagcrest_mod_host::load_mods(repo_root) {
        Ok(host) => {
            let reader = stagcrest_mod_host::FsAssetReader::new(repo_root);
            let packs = stagcrest_mod_host::ResourcePackLoader::load(&reader).ok();
            let colormaps = ColormapSet::load(&reader, packs.as_ref());
            apply_loaded_content(&mut commands, &config, host, colormaps);
            state.done = true;
        }
        Err(e) => {
            state.error = Some(e.to_string());
            state.done = true;
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn start_web_load_system(mut state: ResMut<LoadingState>, mut task: ResMut<WebLoadingTask>) {
    if state.started {
        return;
    }
    state.started = true;

    task.0 = Some(bevy::tasks::IoTaskPool::get().spawn(async {
        let host = stagcrest_mod_host::load_mods_async()
            .await
            .map_err(|e| e.to_string())?;
        let reader = stagcrest_mod_host::HttpAssetReader::new();
        let packs = stagcrest_mod_host::ResourcePackLoader::load_async(&reader)
            .await
            .ok();
        let colormaps = ColormapSet::load_async(&reader, packs.as_ref()).await;
        Ok((host, colormaps))
    }));
}

#[cfg(target_arch = "wasm32")]
fn poll_web_load_system(
    mut state: ResMut<LoadingState>,
    mut task: ResMut<WebLoadingTask>,
    mut commands: Commands,
    config: Res<GameConfig>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if state.done {
        if state.error.is_none() {
            next_state.set(AppState::InGame);
        }
        return;
    }

    let Some(mut running) = task.0.take() else {
        return;
    };

    match futures_lite::future::block_on(futures_lite::future::poll_once(&mut running)) {
        Some(Ok((host, colormaps))) => {
            apply_loaded_content(&mut commands, &config, host, colormaps);
            state.done = true;
        }
        Some(Err(message)) => {
            state.error = Some(message);
            state.done = true;
        }
        None => {
            task.0 = Some(running);
        }
    }
}

fn loading_ui(
    mut commands: Commands,
    state: Res<LoadingState>,
    query: Query<Entity, With<LoadingRoot>>,
) {
    if !state.is_changed() && !query.is_empty() {
        return;
    }

    for e in &query {
        commands.entity(e).despawn();
    }

    let message = if let Some(err) = &state.error {
        format!("Failed to load mods:\n{err}")
    } else if state.done {
        "Ready!".to_string()
    } else {
        "Loading mods...".to_string()
    };

    commands
        .spawn((
            LoadingRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.05, 0.08)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(message),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(if state.error.is_some() {
                    Color::srgb(1.0, 0.4, 0.4)
                } else {
                    Color::WHITE
                }),
            ));
            if state.error.is_some() {
                parent
                    .spawn((
                        LoadingAction::BackToMenu,
                        Button,
                        Node {
                            width: Val::Px(180.0),
                            height: Val::Px(40.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.25, 0.27, 0.32)),
                    ))
                    .with_child((
                        Text::new("Main Menu"),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
            }
        });
}

fn loading_button_system(
    mut interaction: Query<
        (&Interaction, &LoadingAction),
        (Changed<Interaction>, With<Button>),
    >,
    mut next_state: ResMut<NextState<AppState>>,
    mut state: ResMut<LoadingState>,
    #[cfg(target_arch = "wasm32")] mut task: ResMut<WebLoadingTask>,
) {
    for (interaction, action) in &mut interaction {
        if *interaction == Interaction::Pressed {
            match action {
                LoadingAction::BackToMenu => {
                    *state = LoadingState::default();
                    #[cfg(target_arch = "wasm32")]
                    {
                        *task = WebLoadingTask::default();
                    }
                    next_state.set(AppState::MainMenu);
                }
            }
        }
    }
}

fn cleanup_loading(mut commands: Commands, query: Query<Entity, With<LoadingRoot>>) {
    for e in &query {
        commands.entity(e).despawn();
    }
}
