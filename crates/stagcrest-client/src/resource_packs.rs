use crate::client_content::{
    cleanup_screen, content_data_dir, handle_button_hover, spawn_blocking_task,
    spawn_compact_button, spawn_confirm_delete_overlay, spawn_delete_icon_button, spawn_divider,
    spawn_list_row, spawn_row_actions, spawn_screen_button, spawn_screen_root,
    spawn_screen_subtitle, spawn_screen_title, spawn_section_label, with_screen_panel,
    with_scroll_area, ClientContentSettings, DeleteConfirmOverlay,
};
use crate::game::AppState;
use crate::ui::UiTheme;
use bevy::prelude::*;
use bevy::tasks::{block_on, Task};
use bevy::text::{EditableText, TextCursorStyle, TextLayout};
use futures_lite::future;
use stagcrest_content::{ContentInstaller, MoveDirection};
use stagcrest_modrinth::{ModrinthClient, SearchHit, RECOMMENDED};

pub struct ResourcePacksPlugin;

impl Plugin for ResourcePacksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PackManagerState>()
            .add_systems(OnEnter(AppState::ResourcePacks), spawn_manager_ui)
            .add_systems(
                OnExit(AppState::ResourcePacks),
                (
                    cleanup_screen::<ManagerRoot>,
                    cleanup_screen::<DeleteConfirmOverlay>,
                ),
            )
            .add_systems(
                Update,
                (
                    manager_button_system,
                    poll_manager_tasks,
                    refresh_manager_ui,
                )
                    .run_if(in_state(AppState::ResourcePacks)),
            );
    }
}

#[derive(Resource, Default)]
struct PackManagerState {
    download_task: Option<Task<Result<(), String>>>,
    search_task: Option<Task<Result<Vec<SearchHit>, String>>>,
    search_results: Vec<SearchHit>,
    search_query: String,
    status: String,
    busy: bool,
    needs_refresh: bool,
    pending_delete: Option<String>,
}

#[derive(Component)]
struct ManagerRoot;

#[derive(Component)]
struct SearchField;

#[derive(Component)]
enum ManagerAction {
    Back,
    Search,
    InstallSlug(String),
    ToggleEnabled(String),
    MoveUp(String),
    MoveDown(String),
    Delete(String),
    ConfirmDelete,
    CancelDelete,
}

fn spawn_manager_ui(
    mut commands: Commands,
    theme: Res<UiTheme>,
    mut state: ResMut<PackManagerState>,
) {
    *state = PackManagerState::default();
    let settings = ClientContentSettings::reload();
    build_manager_ui(&mut commands, &theme, &settings, &state);
    commands.insert_resource(settings);
}

fn build_manager_ui(
    commands: &mut Commands,
    theme: &UiTheme,
    settings: &ClientContentSettings,
    state: &PackManagerState,
) {
    let root = spawn_screen_root(commands, theme);
    commands
        .entity(root)
        .insert(ManagerRoot)
        .with_children(|root| {
            with_screen_panel(root, theme, 480.0, 560.0, |panel| {
                spawn_screen_title(panel, "Resource Packs", theme);
                spawn_screen_subtitle(
                    panel,
                    "Higher entries override textures below. Re-enter a world after changes.",
                    theme,
                );
                spawn_divider(panel, theme);

                if state.busy {
                    panel.spawn((
                        Text::new(state.status.as_str()),
                        theme.text_font(theme.body_size),
                        TextColor(theme.accent),
                    ));
                } else if !state.status.is_empty() {
                    panel.spawn((
                        Text::new(state.status.as_str()),
                        theme.text_font(theme.caption_size),
                        TextColor(if state.status.contains("failed") {
                            theme.error_text
                        } else {
                            theme.text_muted
                        }),
                    ));
                }

                with_scroll_area(panel, theme, |body| {
                    let installed_ids: std::collections::HashSet<_> = settings
                        .0
                        .content()
                        .resource_packs
                        .iter()
                        .map(|p| p.id.as_str())
                        .collect();
                    let missing_rec: Vec<_> = RECOMMENDED
                        .iter()
                        .filter(|p| !installed_ids.contains(p.slug))
                        .collect();

                    if !missing_rec.is_empty() && !state.busy {
                        spawn_section_label(body, "Recommended", theme);
                        body.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(6.0),
                            width: Val::Percent(100.0),
                            ..default()
                        })
                        .with_children(|list| {
                            for pack in missing_rec {
                                spawn_list_row(list, theme, |row| {
                                    row.spawn((
                                        Text::new(pack.title),
                                        theme.text_font(theme.body_size),
                                        TextColor(theme.text_primary),
                                    ));
                                    spawn_compact_button(
                                        row,
                                        "Install",
                                        ManagerAction::InstallSlug(pack.slug.to_string()),
                                        theme,
                                        72.0,
                                    );
                                });
                            }
                        });
                        spawn_divider(body, theme);
                    }

                    spawn_section_label(body, "Installed", theme);
                    body.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        width: Val::Percent(100.0),
                        ..default()
                    })
                    .with_children(|list| {
                        let order = &settings.0.content().resource_pack_order;
                        if order.is_empty() {
                            list.spawn((
                                Text::new("No packs installed yet."),
                                theme.text_font(theme.body_size),
                                TextColor(theme.text_hint),
                            ));
                        }
                        for id in order {
                            let Some(entry) = settings.0.pack_by_id(id) else {
                                continue;
                            };
                            let source_label = if entry.source.is_modrinth() {
                                "Modrinth"
                            } else {
                                "Local"
                            };
                            let valid = settings.0.is_pack_valid(entry);
                            spawn_list_row(list, theme, |row| {
                                row.spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px(2.0),
                                    flex_grow: 1.0,
                                    ..default()
                                })
                                .with_children(|info| {
                                    info.spawn((
                                        Text::new(&entry.id),
                                        theme.text_font(theme.body_size),
                                        TextColor(theme.text_primary),
                                    ));
                                    let detail = if valid {
                                        format!("{source_label} · {}", entry.path)
                                    } else {
                                        format!(
                                            "{source_label} · {} · invalid — missing pack.mcmeta",
                                            entry.path
                                        )
                                    };
                                    info.spawn((
                                        Text::new(detail),
                                        theme.text_font(FontSize::Px(13.0)),
                                        TextColor(theme.text_muted),
                                    ));
                                });
                                if !state.busy {
                                    spawn_row_actions(row, |actions| {
                                        let enabled_label =
                                            if entry.enabled { "On" } else { "Off" };
                                        spawn_compact_button(
                                            actions,
                                            enabled_label,
                                            ManagerAction::ToggleEnabled(id.clone()),
                                            theme,
                                            40.0,
                                        );
                                        spawn_compact_button(
                                            actions,
                                            "Up",
                                            ManagerAction::MoveUp(id.clone()),
                                            theme,
                                            36.0,
                                        );
                                        spawn_compact_button(
                                            actions,
                                            "Dn",
                                            ManagerAction::MoveDown(id.clone()),
                                            theme,
                                            36.0,
                                        );
                                        spawn_delete_icon_button(
                                            actions,
                                            ManagerAction::Delete(id.clone()),
                                            theme,
                                        );
                                    });
                                }
                            });
                        }
                    });

                    if !state.busy {
                        spawn_divider(body, theme);
                        spawn_section_label(body, "Search Modrinth", theme);
                        body.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(8.0),
                            align_items: AlignItems::Center,
                            width: Val::Percent(100.0),
                            ..default()
                        })
                        .with_children(|search_row| {
                            let mut search_field = EditableText::new(state.search_query.as_str());
                            search_field.visible_width = Some(28.0);
                            search_field.allow_newlines = false;
                            search_row.spawn((
                                SearchField,
                                search_field,
                                TextLayout::no_wrap(),
                                theme.text_font(theme.body_size),
                                TextCursorStyle::default(),
                                Node {
                                    padding: UiRect::all(Val::Px(8.0)),
                                    flex_grow: 1.0,
                                    border_radius: BorderRadius::all(Val::Px(4.0)),
                                    ..default()
                                },
                                BackgroundColor(theme.search_bg),
                                BorderColor::all(theme.slot_border),
                            ));
                            spawn_screen_button(search_row, "Search", ManagerAction::Search, theme);
                        });

                        if !state.search_results.is_empty() {
                            body.spawn(Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(6.0),
                                width: Val::Percent(100.0),
                                margin: UiRect::top(Val::Px(8.0)),
                                ..default()
                            })
                            .with_children(|results| {
                                for hit in &state.search_results {
                                    spawn_list_row(results, theme, |row| {
                                        row.spawn((
                                            Text::new(hit.title.as_str()),
                                            theme.text_font(theme.body_size),
                                            TextColor(theme.text_primary),
                                        ));
                                        spawn_compact_button(
                                            row,
                                            "Install",
                                            ManagerAction::InstallSlug(hit.slug.clone()),
                                            theme,
                                            72.0,
                                        );
                                    });
                                }
                            });
                        }
                    }
                });

                if !state.busy {
                    spawn_divider(panel, theme);
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::Center,
                            width: Val::Percent(100.0),
                            ..default()
                        })
                        .with_children(|row| {
                            spawn_screen_button(row, "Back", ManagerAction::Back, theme);
                        });
                }
            });
        });
}

fn manager_button_system(
    mut commands: Commands,
    mut state: ResMut<PackManagerState>,
    mut settings: ResMut<ClientContentSettings>,
    mut next_state: ResMut<NextState<AppState>>,
    theme: Res<UiTheme>,
    search_fields: Query<&EditableText, With<SearchField>>,
    overlays: Query<Entity, With<DeleteConfirmOverlay>>,
    mut interaction: Query<
        (&Interaction, &ManagerAction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
) {
    if state.busy {
        return;
    }

    for (interaction, action, mut bg) in &mut interaction {
        match *interaction {
            Interaction::Pressed => match action {
                ManagerAction::Back => {
                    next_state.set(AppState::MainMenu);
                }
                ManagerAction::Search => {
                    let query = search_fields
                        .iter()
                        .next()
                        .map(|t| t.value().to_string().trim().to_string())
                        .unwrap_or_default();
                    state.search_query = query.clone();
                    if query.is_empty() {
                        state.status = "Enter a search query.".to_string();
                        state.needs_refresh = true;
                        continue;
                    }
                    state.busy = true;
                    state.status = format!("Searching for \"{query}\"...");
                    state.needs_refresh = true;
                    state.search_task = Some(spawn_blocking_task(move || {
                        let client = ModrinthClient::new();
                        client
                            .search_resource_packs(&query, 20, 0)
                            .map(|r| r.hits)
                            .map_err(|e| e.to_string())
                    }));
                }
                ManagerAction::InstallSlug(slug) => {
                    let slug = slug.clone();
                    state.busy = true;
                    state.status = format!("Installing {slug}...");
                    state.needs_refresh = true;
                    let data_dir = content_data_dir();
                    state.download_task = Some(spawn_blocking_task(move || {
                        let installer = ContentInstaller::new(&data_dir);
                        installer
                            .install_modrinth_pack(&slug)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }));
                }
                ManagerAction::ToggleEnabled(id) => {
                    if let Some(entry) = settings.0.pack_by_id(id) {
                        let new = !entry.enabled;
                        let _ = settings.0.set_enabled(id, new);
                        let _ = settings.save();
                        state.needs_refresh = true;
                    }
                }
                ManagerAction::MoveUp(id) => {
                    let _ = settings.0.move_pack(id, MoveDirection::Up);
                    let _ = settings.save();
                    state.needs_refresh = true;
                }
                ManagerAction::MoveDown(id) => {
                    let _ = settings.0.move_pack(id, MoveDirection::Down);
                    let _ = settings.save();
                    state.needs_refresh = true;
                }
                ManagerAction::Delete(id) => {
                    state.pending_delete = Some(id.clone());
                    if overlays.is_empty() {
                        spawn_confirm_delete_overlay(
                            &mut commands,
                            &theme,
                            id,
                            "The pack files will be removed from disk.",
                            ManagerAction::ConfirmDelete,
                            ManagerAction::CancelDelete,
                        );
                    }
                }
                ManagerAction::ConfirmDelete => {
                    let Some(id) = state.pending_delete.take() else {
                        return;
                    };
                    for e in &overlays {
                        commands.entity(e).despawn();
                    }
                    let installer = ContentInstaller::new(content_data_dir());
                    match installer.remove_pack(&id) {
                        Ok(updated) => {
                            settings.0 = updated;
                            state.status = format!("Removed pack {id}.");
                        }
                        Err(e) => state.status = format!("Delete failed: {e}"),
                    }
                    state.needs_refresh = true;
                }
                ManagerAction::CancelDelete => {
                    state.pending_delete = None;
                    for e in &overlays {
                        commands.entity(e).despawn();
                    }
                }
            },
            Interaction::Hovered | Interaction::None => {
                handle_button_hover(*interaction, &theme, &mut bg, theme.button_bg);
            }
        }
    }
}

fn poll_manager_tasks(
    mut state: ResMut<PackManagerState>,
    mut settings: ResMut<ClientContentSettings>,
) {
    if let Some(task) = state.download_task.as_mut() {
        if task.is_finished() {
            let mut finished = state.download_task.take().unwrap();
            state.busy = false;
            match block_on(future::poll_once(&mut finished)) {
                Some(Ok(())) => {
                    *settings = ClientContentSettings::reload();
                    state.status = "Pack installed.".to_string();
                }
                Some(Err(err)) => state.status = format!("Install failed: {err}"),
                None => state.status = "Install failed: task ended without result".to_string(),
            }
            state.needs_refresh = true;
        }
    }

    if let Some(task) = state.search_task.as_mut() {
        if task.is_finished() {
            let mut finished = state.search_task.take().unwrap();
            state.busy = false;
            match block_on(future::poll_once(&mut finished)) {
                Some(Ok(hits)) => {
                    state.search_results = hits;
                    state.status = format!("{} results.", state.search_results.len());
                }
                Some(Err(err)) => {
                    state.search_results.clear();
                    state.status = format!("Search failed: {err}");
                }
                None => state.status = "Search failed: task ended without result".to_string(),
            }
            state.needs_refresh = true;
        }
    }
}

fn refresh_manager_ui(
    mut commands: Commands,
    mut state: ResMut<PackManagerState>,
    theme: Res<UiTheme>,
    settings: Res<ClientContentSettings>,
    roots: Query<Entity, With<ManagerRoot>>,
) {
    if !state.needs_refresh {
        return;
    }
    state.needs_refresh = false;
    for e in &roots {
        commands.entity(e).despawn();
    }
    build_manager_ui(&mut commands, &theme, &settings, &state);
}
