use crate::client_content::{cleanup_screen, spawn_screen_button};
use crate::game::{AppState, GameConfig, ModContext};
use crate::net_client::{connect_tcp, GameNetClient};
use crate::ui::UiTheme;
use crate::world_replica::WorldReplica;
use crate::world_select::SelectedWorld;
use bevy::prelude::*;
use bevy::tasks::{block_on, IoTaskPool, Task};
use futures_lite::future;
use stagcrest_atlas::AtlasLimits;
use stagcrest_content::ContentSettings;
use stagcrest_mod_client::content::ContentRuntime;
use crate::entity_render::build_entity_catalog;
use stagcrest_protocol::manifest::{ContentManifest, TextureAssetTransfer};
use stagcrest_protocol::{BlockId, EntityAssetTransfer, EntityManifest};
use stagcrest_render::{next_atlas_revision, BlockAtlasResource, MeshCacheResource};
use stagcrest_storage::DATA_DIR;

pub struct LoadingPlugin;

#[derive(Resource, Default)]
struct LoadingState {
    started: bool,
    done: bool,
    error: Option<String>,
    tcp_connect: Option<Task<Result<Box<dyn stagcrest_net::GameTransport>, String>>>,
    pending_username: Option<String>,
    pending_manifest: Option<ContentManifest>,
    pending_textures: Vec<TextureAssetTransfer>,
    pending_world_time: Option<f64>,
    all_textures_received: bool,
    pending_entity_manifest: Option<EntityManifest>,
    pending_entity_assets: Vec<EntityAssetTransfer>,
    all_entity_assets_received: bool,
    content_ready: bool,
    entities_built: bool,
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
            .add_systems(OnExit(AppState::Loading), cleanup_screen::<LoadingRoot>)
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
    net.shutdown_session();
}

fn start_connection_system(
    mut state: ResMut<LoadingState>,
    mut net: ResMut<GameNetClient>,
    config: Res<GameConfig>,
    selected: Res<SelectedWorld>,
    profile: Res<crate::player_profile::PlayerProfile>,
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

    let username = ContentSettings::load(DATA_DIR)
        .ok()
        .map(|s| s.username().to_string())
        .unwrap_or_else(|| profile.username.clone());

    if let Some(addr) = net.connect_addr.clone() {
        let net_config = net.net_config.clone();
        state.tcp_connect =
            Some(IoTaskPool::get().spawn(async move { connect_tcp(&addr, net_config).await }));
        state.pending_username = Some(username);
        return;
    }

    match net.start_embedded(
        server_config,
        crate::player_profile::LOCAL_PLAYER_NAME,
    ) {
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
    let username = state.pending_username.take().unwrap_or_else(|| {
        crate::player_profile::LOCAL_PLAYER_NAME.to_string()
    });
    match block_on(future::poll_once(&mut finished)) {
        Some(Ok(transport)) => net.start_tcp(transport, &username),
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
    mut chat: ResMut<crate::chat::ChatUiState>,
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
            ServerMessage::TextureAssets(chunk) => {
                let is_last = chunk.index + 1 >= chunk.total;
                state.pending_textures.extend(chunk.textures);
                if is_last {
                    state.all_textures_received = true;
                }
            }
            ServerMessage::EntityManifest(manifest) => {
                state.pending_entity_manifest = Some(manifest);
            }
            ServerMessage::EntityAssets(chunk) => {
                let is_last = chunk.index + 1 >= chunk.total;
                state.pending_entity_assets.extend(chunk.entities);
                if is_last {
                    state.all_entity_assets_received = true;
                }
            }
            ServerMessage::Initial(initial) => {
                net.initial_received = true;
                state.pending_world_time = Some(initial.world_time);
            }
            ServerMessage::Chat(line) => {
                crate::chat::push_chat_line(&mut chat, line);
            }
            _ => {}
        }
    }

    if state.pending_manifest.is_some() && state.all_textures_received {
        let Some(manifest) = state.pending_manifest.take() else {
            return;
        };
        let textures = std::mem::take(&mut state.pending_textures);
        let limits = AtlasLimits::default();
        match ContentRuntime::from_parts(manifest, &textures, limits) {
            Ok(runtime) => {
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
                let world =
                    WorldReplica::new(stagcrest_world::World::with_lru_capacity(lru_cap, air));

                let fluid_anim = fluid_anim_uniform(&runtime.registry);
                let revision = next_atlas_revision();

                commands.insert_resource(ModContext {
                    registry: runtime.registry,
                    atlases: runtime.atlases.clone(),
                    models: runtime.models,
                    colormaps: runtime.colormaps,
                    biomes: runtime.biomes,
                });
                commands.insert_resource(world);
                commands.insert_resource(crate::chunk_streaming::BiomeGridCache::default());
                commands.insert_resource(BlockAtlasResource {
                    revision,
                    atlases: runtime.atlases,
                    grass_tint: Color::srgb(grass_rgb[0], grass_rgb[1], grass_rgb[2]),
                    foliage_tint: Color::srgb(foliage_rgb[0], foliage_rgb[1], foliage_rgb[2]),
                    water_tint: Color::srgb(water_rgb[0], water_rgb[1], water_rgb[2]),
                    fluid_anim,
                });
                commands.insert_resource(MeshCacheResource::default());
                let mut wt = crate::world_time::WorldTime::default();
                if let Some(t) = state.pending_world_time.take() {
                    wt.set_from_server(t);
                }
                commands.insert_resource(wt);
                state.content_ready = true;
            }
            Err(err) => {
                state.error = Some(format!("Failed to build texture atlases: {err}"));
                state.done = true;
                return;
            }
        }
    }

    // Build the entity catalog once the manifest and all asset chunks arrived.
    let entities_complete = state.pending_entity_manifest.as_ref().is_some_and(|m| {
        m.entities.is_empty() || state.all_entity_assets_received
    });
    if state.content_ready && entities_complete && !state.entities_built {
        if let Some(manifest) = state.pending_entity_manifest.take() {
            let assets = std::mem::take(&mut state.pending_entity_assets);
            let catalog = build_entity_catalog(&manifest, &assets);
            commands.insert_resource(catalog);
        }
        state.entities_built = true;
        net.handshake_done = true;
    }

    if !state.done && net.handshake_done && net.initial_received {
        state.done = true;
    }
}

fn fluid_anim_uniform(registry: &stagcrest_mod_client::BlockRegistry) -> Vec4 {
    let Some(tex_id) = registry.texture_by_name("stagcrest:water_still") else {
        return Vec4::ONE;
    };
    let uv = registry.atlas_uv(tex_id);
    let (_, page_h) = registry.atlas_dimensions(uv.atlas_index);
    let anim = registry.texture_animation(tex_id).cloned().or_else(|| {
        registry
            .textures()
            .find(|t| t.id == tex_id)
            .and_then(|t| stagcrest_mod_client::infer_vertical_strip_animation(t.width, t.height))
    });
    let Some(anim) = anim else {
        return Vec4::ONE;
    };
    let frame_uv_step = anim.frame_height as f32 / page_h.max(1) as f32;
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
    } else if state.pending_manifest.is_some() {
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
                spawn_screen_button(parent, "Main Menu", LoadingAction::BackToMenu, &theme);
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
