use crate::game::{AppState, GameConfig, ModContext};
use crate::ui::UiTheme;
use crate::world_select::SelectedWorld;
use bevy::prelude::*;
use bevy::tasks::{block_on, IoTaskPool, Task};
use futures_lite::future;
use stagcrest_mod_client::content::ContentRuntime;
use crate::net_client::{connect_tcp, GameNetClient};
use crate::world_replica::WorldReplica;
use stagcrest_protocol::manifest::{AtlasTransfer, ContentManifest};
use stagcrest_protocol::BlockId;
use stagcrest_render::{BlockAtlasResource, MeshCacheResource};

pub struct LoadingPlugin;

#[derive(Resource, Default)]
struct LoadingState {
    started: bool,
    done: bool,
    error: Option<String>,
    tcp_connect: Option<Task<Result<Box<dyn stagcrest_net::GameTransport>, String>>>,
    pending_manifest: Option<ContentManifest>,
    pending_atlas: Option<AtlasTransfer>,
}

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
            .add_systems(OnExit(AppState::Loading), cleanup_loading)
            .add_systems(
                Update,
                (
                    start_connection_system,
                    poll_tcp_connect_system,
                    poll_connection_system,
                    loading_ui,
                    loading_button_system,
                )
                    .run_if(in_state(AppState::Loading)),
            );
    }
}

fn on_enter_loading(mut state: ResMut<LoadingState>, mut net: ResMut<GameNetClient>) {
    *state = LoadingState::default();
    net.handshake_done = false;
    net.initial_received = false;
}

fn start_connection_system(
    mut state: ResMut<LoadingState>,
    mut net: ResMut<GameNetClient>,
    config: Res<GameConfig>,
    selected: Res<SelectedWorld>,
) {
    if state.started {
        return;
    }
    state.started = true;

    let world_name = if selected.world_name.is_empty() {
        "default".to_string()
    } else {
        selected.world_name.clone()
    };
    let world_seed = if selected.world_seed != 0 {
        selected.world_seed
    } else {
        config.world_seed
    };

    let server_config = stagcrest_server::ServerConfig {
        bind: None,
        world_name,
        world_seed,
        mods_root: std::path::PathBuf::from("."),
        render_distance: config.render_distance,
        vertical_render_distance: config.vertical_render_distance,
        net_sim_latency_ms: net.net_config.sim_latency_ms,
        max_clients: 1,
    };

    if let Some(addr) = net.connect_addr.clone() {
        let net_config = net.net_config.clone();
        state.tcp_connect =
            Some(IoTaskPool::get().spawn(async move { connect_tcp(&addr, net_config).await }));
        return;
    }

    match net.start_embedded(server_config) {
        Ok(()) => {}
        Err(e) => {
            state.error = Some(e);
            state.done = true;
        }
    }
}

fn poll_tcp_connect_system(mut state: ResMut<LoadingState>, mut net: ResMut<GameNetClient>) {
    let Some(task) = state.tcp_connect.as_mut() else {
        return;
    };
    if !task.is_finished() {
        return;
    }
    let mut finished = state.tcp_connect.take().unwrap();
    match block_on(future::poll_once(&mut finished)) {
        Some(Ok(transport)) => net.start_tcp(transport),
        Some(Err(err)) => {
            state.error = Some(err);
            state.done = true;
        }
        None => {
            state.error = Some("TCP connect task ended without result".to_string());
            state.done = true;
        }
    }
}

fn poll_connection_system(
    mut state: ResMut<LoadingState>,
    mut net: ResMut<GameNetClient>,
    mut commands: Commands,
    config: Res<GameConfig>,
    mut next_state: ResMut<NextState<AppState>>,
    _images: ResMut<Assets<Image>>,
) {
    if state.done {
        if state.error.is_none() {
            next_state.set(AppState::InGame);
        }
        return;
    }

    for msg in net.poll() {
        use stagcrest_net::ServerMessage;
        match msg {
            ServerMessage::Reject(r) => {
                state.error = Some(r.reason);
                state.done = true;
                return;
            }
            ServerMessage::Hello(_) => {}
            ServerMessage::Manifest(manifest) => {
                state.pending_manifest = Some(manifest);
            }
            ServerMessage::AtlasTransfer(atlas) => {
                state.pending_atlas = Some(atlas);
            }
            ServerMessage::Initial(_) => {
                net.initial_received = true;
            }
            _ => {}
        }
    }

    if let (Some(manifest), Some(atlas)) = (
        state.pending_manifest.take(),
        state.pending_atlas.take(),
    ) {
        let runtime = ContentRuntime::from_parts(manifest, atlas);
        let grass_rgb = runtime.colormaps.default_grass_tint();
        let foliage_rgb = runtime.colormaps.default_foliage_tint();
        let water_rgb = runtime.colormaps.default_water_tint();
        let air = runtime
            .registry
            .block_by_name("stagcrest:air")
            .unwrap_or(BlockId(0));
        let lru_cap = stagcrest_server::streaming_lru_capacity(
            config.render_distance,
            config.vertical_render_distance,
            1,
        );
        let world = WorldReplica::new(stagcrest_world::World::with_lru_capacity(lru_cap, air));

        let fluid_anim = fluid_anim_uniform(&runtime.registry, runtime.atlas.height);

        commands.insert_resource(ModContext {
            registry: runtime.registry,
            atlas: runtime.atlas.clone(),
            models: runtime.models,
            colormaps: runtime.colormaps,
            biomes: runtime.biomes,
        });
        commands.insert_resource(world);
        commands.insert_resource(crate::chunk_streaming::BiomeGridCache::default());
        commands.insert_resource(BlockAtlasResource {
            atlas: runtime.atlas,
            grass_tint: Color::srgb(grass_rgb[0], grass_rgb[1], grass_rgb[2]),
            foliage_tint: Color::srgb(foliage_rgb[0], foliage_rgb[1], foliage_rgb[2]),
            water_tint: Color::srgb(water_rgb[0], water_rgb[1], water_rgb[2]),
            fluid_anim,
        });
        commands.insert_resource(MeshCacheResource::default());
        net.handshake_done = true;
    }

    if !state.done && net.handshake_done && net.initial_received {
        state.done = true;
    }
}

fn fluid_anim_uniform(registry: &stagcrest_mod_client::BlockRegistry, atlas_height: u32) -> Vec4 {
    let Some(tex_id) = registry.texture_by_name("stagcrest:water_still") else {
        return Vec4::ONE;
    };
    let anim = registry.texture_animation(tex_id).cloned().or_else(|| {
        registry
            .textures()
            .find(|t| t.id == tex_id)
            .and_then(|t| stagcrest_mod_client::infer_vertical_strip_animation(t.width, t.height))
    });
    let Some(anim) = anim else {
        return Vec4::ONE;
    };
    let frame_uv_step = anim.frame_height as f32 / atlas_height.max(1) as f32;
    let frametime_secs = (anim.frametime_ticks as f32 / 20.0).max(0.05);
    Vec4::new(anim.frame_count as f32, frame_uv_step, frametime_secs, 0.0)
}

fn loading_ui(
    mut commands: Commands,
    state: Res<LoadingState>,
    theme: Res<UiTheme>,
    query: Query<Entity, With<LoadingRoot>>,
) {
    if !state.is_changed() && !query.is_empty() {
        return;
    }

    for e in &query {
        commands.entity(e).despawn();
    }

    let message = if let Some(err) = &state.error {
        format!("Failed to connect:\n{err}")
    } else if state.done {
        "Ready!".to_string()
    } else if state.pending_manifest.is_some() && state.pending_atlas.is_none() {
        "Receiving textures...".to_string()
    } else {
        "Connecting to server...".to_string()
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
            BackgroundColor(theme.screen_bg),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(message),
                theme.text_font(theme.body_size),
                TextColor(if state.error.is_some() {
                    theme.error_text
                } else {
                    theme.text_primary
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
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                    ))
                    .insert(BackgroundColor(theme.button_bg))
                    .with_child((
                        Text::new("Main Menu"),
                        theme.text_font(theme.subtitle_size),
                        TextColor(theme.text_primary),
                    ));
            }
        });
}

fn loading_button_system(
    mut interaction: Query<(&Interaction, &LoadingAction), (Changed<Interaction>, With<Button>)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut state: ResMut<LoadingState>,
    net: Res<crate::net_client::GameNetClient>,
) {
    for (interaction, action) in &mut interaction {
        if *interaction == Interaction::Pressed {
            match action {
                LoadingAction::BackToMenu => {
                    *state = LoadingState::default();
                    if net.embedded {
                        next_state.set(AppState::WorldSelect);
                    } else {
                        next_state.set(AppState::MainMenu);
                    }
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
