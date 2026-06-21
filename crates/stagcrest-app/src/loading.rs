use bevy::prelude::*;
use stagcrest_mesh::MeshCache;
use stagcrest_mod_host::{ColormapSet, ModHost, WorldGenState};
use stagcrest_protocol::ChunkPos;
use stagcrest_render::BlockAtlasResource;

use crate::game::{AppState, GameConfig, ModContext, StagcrestWorldResource, TerrainGen};

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

fn apply_loaded_content(
    commands: &mut Commands,
    config: &GameConfig,
    mut host: ModHost,
    colormaps: ColormapSet,
) {
    let atlas = host.finalize_atlas();
    let grass_rgb = colormaps.default_grass_tint();
    let foliage_rgb = colormaps.default_foliage_tint();
    let registry = std::mem::take(&mut host.registry);
    let air = registry
        .block_by_name("stagcrest:air")
        .unwrap_or(stagcrest_protocol::BlockId(0));

    let mut world = StagcrestWorldResource(stagcrest_world::World::new(air));
    let mut terrain = WorldGenState::default();
    terrain.generate_area(
        &mut world.0,
        &registry,
        ChunkPos { x: 0, y: 0, z: 0 },
        config.render_distance.min(4),
    );

    let mut cache = MeshCache::default();
    let all_chunks: Vec<_> = world.0.loaded_chunk_positions().collect();
    world.0.dirty_chunks.extend(all_chunks);
    let dirty = world.0.take_dirty_chunks();
    cache.rebuild_dirty(&world.0, &registry, dirty);

    commands.insert_resource(ModContext {
        host,
        atlas: atlas.clone(),
        registry,
    });
    commands.insert_resource(world);
    commands.insert_resource(TerrainGen(terrain));
    commands.insert_resource(BlockAtlasResource {
        atlas,
        grass_tint: Color::srgb(grass_rgb[0], grass_rgb[1], grass_rgb[2]),
        foliage_tint: Color::srgb(foliage_rgb[0], foliage_rgb[1], foliage_rgb[2]),
    });
    commands.insert_resource(stagcrest_render::MeshCacheResource(cache));
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
