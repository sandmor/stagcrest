use crate::client_content::{
    cleanup_screen, handle_button_hover, spawn_confirm_delete_overlay, spawn_delete_icon_button,
    spawn_divider, spawn_screen_button, spawn_text_input, DeleteConfirmOverlay,
};
use crate::game::AppState;
use crate::net_client::GameNetClient;
use crate::ui::UiTheme;
use bevy::prelude::*;
use bevy::text::EditableText;
use stagcrest_storage::{RedbChunkStorage, WorldMeta, DATA_DIR};

fn worlds_dir() -> std::path::PathBuf {
    std::path::Path::new(DATA_DIR).join("worlds")
}

#[derive(Resource, Default)]
pub struct SelectedWorld {
    pub world_name: String,
    pub world_seed: u64,
}

#[derive(Clone)]
struct WorldEntry {
    name: String,
    seed: u64,
    circuit_tick: u64,
}

#[derive(Resource, Default)]
struct WorldList(Vec<WorldEntry>);

#[derive(Resource, Default)]
struct CreateWorldForm;

#[derive(Resource, Default)]
struct PendingDelete(pub Option<String>);

#[derive(Resource, Default)]
struct SelectedEntryName(pub Option<String>);

pub struct WorldSelectPlugin;

impl Plugin for WorldSelectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedWorld>()
            .init_resource::<WorldList>()
            .init_resource::<CreateWorldForm>()
            .init_resource::<PendingDelete>()
            .init_resource::<SelectedEntryName>()
            .add_systems(OnEnter(AppState::WorldSelect), scan_worlds_system)
            .add_systems(
                OnEnter(AppState::WorldSelect),
                spawn_world_select_ui_on_enter.after(scan_worlds_system),
            )
            .add_systems(
                OnExit(AppState::WorldSelect),
                (
                    cleanup_screen::<WorldSelectRoot>,
                    cleanup_screen::<DeleteConfirmOverlay>,
                    cleanup_form_resources,
                ),
            )
            .add_systems(
                Update,
                world_select_button_system.run_if(in_state(AppState::WorldSelect)),
            );
    }
}

fn is_valid_world_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.contains(|c: char| c.is_ascii_control())
        && name.len() <= 128
}

#[derive(Component)]
struct WorldSelectRoot;

#[derive(Component)]
struct WorldEntryButton;

#[derive(Component)]
struct WorldEntryName(String);

#[derive(Component)]
enum WorldSelectAction {
    Play,
    Back,
    CreateNew,
    ConfirmCreate,
    CancelCreate,
    Delete(String),
    ConfirmDelete,
    CancelDelete,
}

#[derive(Component)]
struct NewWorldNameField;

#[derive(Component)]
struct NewWorldSeedField;

fn scan_worlds(list: &mut Vec<WorldEntry>) {
    let dir = worlds_dir();
    list.clear();
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let db_path = path.join("world.redb");
            if !db_path.exists() {
                continue;
            }
            let name = entry.file_name().to_str().unwrap_or("?").to_string();
            match RedbChunkStorage::open(&db_path) {
                Ok(storage) => match WorldMeta::load(&storage) {
                    Ok(meta) => list.push(WorldEntry {
                        name,
                        seed: meta.world_seed,
                        circuit_tick: meta.circuit_tick,
                    }),
                    Err(e) => {
                        tracing::warn!("world {name}: failed to read meta: {e}");
                        list.push(WorldEntry {
                            name,
                            seed: 0,
                            circuit_tick: 0,
                        });
                    }
                },
                Err(e) => {
                    tracing::warn!("world {name}: failed to open storage: {e}");
                }
            }
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
}

fn scan_worlds_system(mut world_list: ResMut<WorldList>) {
    scan_worlds(&mut world_list.0);
}

fn spawn_world_select_ui(
    mut commands: Commands,
    theme: &UiTheme,
    world_list: &[WorldEntry],
    selected: Option<&str>,
) {
    commands
        .spawn((
            WorldSelectRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme.screen_bg),
        ))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                padding: UiRect::all(Val::Px(24.0)),
                min_width: Val::Px(380.0),
                max_width: Val::Px(480.0),
                max_height: Val::Percent(90.0),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            })
            .insert(BackgroundColor(theme.panel_bg))
            .with_children(|menu| {
                menu.spawn((
                    Text::new("Select World"),
                    theme.text_font(FontSize::Px(32.0)),
                    TextColor(theme.text_primary),
                ));

                spawn_divider(menu, theme);

                if world_list.is_empty() {
                    menu.spawn((
                        Text::new("No saved worlds found"),
                        theme.text_font(theme.body_size),
                        TextColor(theme.text_muted),
                    ));
                } else {
                    menu.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        overflow: Overflow::scroll_y(),
                        max_height: Val::Px(320.0),
                        width: Val::Percent(100.0),
                        ..default()
                    })
                    .with_children(|list_area| {
                        for entry in world_list {
                            spawn_world_entry_inline(list_area, entry, theme, selected);
                        }
                    });
                }

                spawn_divider(menu, theme);

                spawn_screen_button(
                    menu,
                    "Create New World",
                    WorldSelectAction::CreateNew,
                    &theme,
                );

                menu.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    justify_content: JustifyContent::Center,
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_screen_button(row, "Play", WorldSelectAction::Play, &theme);
                    spawn_screen_button(row, "Back", WorldSelectAction::Back, &theme);
                });
            });
        });
}

fn spawn_world_select_ui_on_enter(
    commands: Commands,
    theme: Res<UiTheme>,
    world_list: Res<WorldList>,
    selected: Res<SelectedEntryName>,
) {
    spawn_world_select_ui(commands, &theme, &world_list.0, selected.0.as_deref());
}

fn spawn_world_entry_inline(
    parent: &mut ChildSpawnerCommands,
    entry: &WorldEntry,
    theme: &UiTheme,
    selected: Option<&str>,
) {
    let is_selected = selected.is_some_and(|s| s == entry.name);
    let bg = if is_selected {
        theme.button_hover
    } else {
        theme.catalog_cell_bg
    };
    parent
        .spawn((
            WorldEntryButton,
            WorldEntryName(entry.name.clone()),
            Button,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(10.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                column_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(bg),
        ))
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            })
            .with_children(|info| {
                info.spawn((
                    Text::new(&entry.name),
                    theme.text_font(theme.body_size),
                    TextColor(theme.text_primary),
                ));
                info.spawn((
                    Text::new(format!(
                        "Seed: {}  ·  Ticks: {}",
                        entry.seed, entry.circuit_tick
                    )),
                    theme.text_font(FontSize::Px(13.0)),
                    TextColor(theme.text_muted),
                ));
            });

            spawn_delete_icon_button(row, WorldSelectAction::Delete(entry.name.clone()), theme);
        });
}

fn spawn_create_world_ui(mut commands: Commands, theme: &UiTheme) {
    commands
        .spawn((
            WorldSelectRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme.screen_bg),
        ))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                padding: UiRect::all(Val::Px(24.0)),
                min_width: Val::Px(380.0),
                max_width: Val::Px(480.0),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            })
            .insert(BackgroundColor(theme.panel_bg))
            .with_children(|menu| {
                menu.spawn((
                    Text::new("Create New World"),
                    theme.text_font(FontSize::Px(32.0)),
                    TextColor(theme.text_primary),
                ));

                spawn_divider(menu, theme);

                menu.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|field_row| {
                    field_row.spawn((
                        Text::new("World Name"),
                        theme.text_font(theme.caption_size),
                        TextColor(theme.text_muted),
                    ));
                    spawn_text_input(field_row, NewWorldNameField, theme, "", 28.0, 280.0);
                });

                menu.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|field_row| {
                    field_row.spawn((
                        Text::new("Seed (number, 0 = random)"),
                        theme.text_font(theme.caption_size),
                        TextColor(theme.text_muted),
                    ));
                    spawn_text_input(field_row, NewWorldSeedField, theme, "", 28.0, 280.0);
                });

                spawn_divider(menu, theme);

                menu.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    justify_content: JustifyContent::Center,
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_screen_button(row, "Create", WorldSelectAction::ConfirmCreate, &theme);
                    spawn_screen_button(row, "Cancel", WorldSelectAction::CancelCreate, &theme);
                });
            });
        });
}

fn world_select_button_system(
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    mut selected_world: ResMut<SelectedWorld>,
    mut world_list: ResMut<WorldList>,
    mut form: ResMut<CreateWorldForm>,
    mut pending_delete: ResMut<PendingDelete>,
    mut selected_entry: ResMut<SelectedEntryName>,
    mut net: ResMut<GameNetClient>,
    theme: Res<UiTheme>,
    mut queries: ParamSet<(
        Query<
            (&Interaction, &WorldEntryName, &mut BackgroundColor),
            (Changed<Interaction>, With<Button>),
        >,
        Query<
            (&Interaction, &WorldSelectAction, &mut BackgroundColor),
            (Changed<Interaction>, With<Button>),
        >,
    )>,
    name_fields: Query<&EditableText, With<NewWorldNameField>>,
    seed_fields: Query<&EditableText, With<NewWorldSeedField>>,
    roots: Query<Entity, With<WorldSelectRoot>>,
    overlays: Query<Entity, With<DeleteConfirmOverlay>>,
) {
    // Handle world entry rows (click to select + hover)
    for (interaction, entry_name, mut bg) in &mut queries.p0() {
        let is_selected = selected_entry.0.as_deref() == Some(&entry_name.0);
        match *interaction {
            Interaction::Pressed => {
                let clicked = &entry_name.0;
                let changed = selected_entry.0.as_deref() != Some(clicked);
                selected_entry.0 = Some(clicked.clone());
                if changed {
                    for e in &roots {
                        commands.entity(e).despawn();
                    }
                    scan_worlds(&mut world_list.0);
                    spawn_world_select_ui(
                        commands.reborrow(),
                        &theme,
                        &world_list.0,
                        selected_entry.0.as_deref(),
                    );
                    return;
                }
            }
            Interaction::Hovered => {
                if !is_selected {
                    *bg = BackgroundColor(theme.button_hover);
                }
            }
            Interaction::None => {
                *bg = BackgroundColor(if is_selected {
                    theme.button_hover
                } else {
                    theme.catalog_cell_bg
                });
            }
        }
    }

    // Handle action buttons (press + hover)
    for (interaction, action, mut bg) in &mut queries.p1() {
        match *interaction {
            Interaction::Pressed => match action {
                WorldSelectAction::Play => {
                    let name = selected_entry
                        .0
                        .as_deref()
                        .or_else(|| world_list.0.first().map(|e| e.name.as_str()));
                    if let Some(name) = name {
                        if let Some(entry) = world_list.0.iter().find(|e| e.name == name) {
                            selected_world.world_name = entry.name.clone();
                            selected_world.world_seed = entry.seed;
                            net.connect_addr = None;
                            net.embedded = true;
                            next_state.set(AppState::Loading);
                        }
                    }
                }
                WorldSelectAction::Back => {
                    next_state.set(AppState::MainMenu);
                }
                WorldSelectAction::CreateNew => {
                    for e in &roots {
                        commands.entity(e).despawn();
                    }
                    spawn_create_world_ui(commands.reborrow(), &theme);
                }
                WorldSelectAction::CancelCreate => {
                    *form = CreateWorldForm;
                    for e in &roots {
                        commands.entity(e).despawn();
                    }
                    scan_worlds(&mut world_list.0);
                    spawn_world_select_ui(
                        commands.reborrow(),
                        &theme,
                        &world_list.0,
                        selected_entry.0.as_deref(),
                    );
                }
                WorldSelectAction::ConfirmCreate => {
                    let name = name_fields
                        .iter()
                        .next()
                        .map(|t| t.value().to_string())
                        .unwrap_or_default();
                    let name = name.trim().to_string();
                    let seed_text = seed_fields
                        .iter()
                        .next()
                        .map(|t| t.value().to_string())
                        .unwrap_or_default();
                    let seed_text = seed_text.trim().to_string();
                    if !is_valid_world_name(&name) {
                        tracing::warn!("invalid world name: {name}");
                        continue;
                    }
                    let seed: u64 = seed_text.parse().unwrap_or(0);
                    let final_seed = if seed == 0 { rand_seed() } else { seed };

                    let world_path = worlds_dir().join(&name);
                    if world_path.exists() {
                        tracing::warn!("world {name} already exists");
                        continue;
                    }
                    let db_path = world_path.join("world.redb");
                    match RedbChunkStorage::open(&db_path) {
                        Ok(storage) => {
                            let meta = WorldMeta {
                                format_version: stagcrest_storage::WORLD_FORMAT_VERSION,
                                world_seed: final_seed,
                                circuit_tick: 0,
                            };
                            if let Err(e) = meta.save(&storage) {
                                tracing::error!("failed to write world meta: {e}");
                                continue;
                            }
                        }
                        Err(e) => {
                            tracing::error!("failed to create world storage: {e}");
                            continue;
                        }
                    }

                    *form = CreateWorldForm;
                    selected_world.world_name = name;
                    selected_world.world_seed = final_seed;
                    net.connect_addr = None;
                    net.embedded = true;
                    next_state.set(AppState::Loading);
                }
                WorldSelectAction::Delete(name) => {
                    pending_delete.0 = Some(name.clone());
                    if overlays.is_empty() {
                        spawn_confirm_delete_overlay(
                            &mut commands,
                            &theme,
                            name,
                            "This cannot be undone.",
                            WorldSelectAction::ConfirmDelete,
                            WorldSelectAction::CancelDelete,
                        );
                    }
                }
                WorldSelectAction::ConfirmDelete => {
                    let Some(ref name) = pending_delete.0 else {
                        return;
                    };
                    let path = worlds_dir().join(name);
                    if let Err(e) = std::fs::remove_dir_all(&path) {
                        tracing::error!("failed to delete world {name}: {e}");
                    }
                    pending_delete.0 = None;
                    selected_entry.0 = None;
                    for e in &overlays {
                        commands.entity(e).despawn();
                    }
                    for e in &roots {
                        commands.entity(e).despawn();
                    }
                    scan_worlds(&mut world_list.0);
                    spawn_world_select_ui(
                        commands.reborrow(),
                        &theme,
                        &world_list.0,
                        selected_entry.0.as_deref(),
                    );
                }
                WorldSelectAction::CancelDelete => {
                    pending_delete.0 = None;
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

fn rand_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    (nanos ^ (nanos >> 32)) as u64
}

fn cleanup_form_resources(mut form: ResMut<CreateWorldForm>) {
    *form = CreateWorldForm;
}
