use crate::game::AppState;
use crate::net_client::GameNetClient;
use crate::ui::UiTheme;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle, TextLayout};
use serde::{Deserialize, Serialize};
use stagcrest_storage::DATA_DIR;

const MAX_HISTORY: usize = 10;
const DEFAULT_PORT: &str = "4242";

fn normalize_addr(addr: &str) -> String {
    let trimmed = addr.trim();
    if trimmed.contains(':') {
        trimmed.to_string()
    } else {
        format!("{trimmed}:{DEFAULT_PORT}")
    }
}

fn history_path() -> std::path::PathBuf {
    std::path::Path::new(DATA_DIR).join("connection_history.json")
}

#[derive(Resource, Default, Serialize, Deserialize)]
pub struct ConnectionHistory(pub Vec<String>);

impl ConnectionHistory {
    fn load() -> Self {
        let path = history_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let path = history_path();
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!("failed to create data dir: {e}");
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::error!("failed to write connection history: {e}");
                }
            }
            Err(e) => tracing::error!("failed to serialize connection history: {e}"),
        }
    }

    fn push_front(&mut self, addr: String) {
        self.0.retain(|a| a != &addr);
        self.0.insert(0, addr);
        self.0.truncate(MAX_HISTORY);
    }
}

pub struct ConnectScreenPlugin;

impl Plugin for ConnectScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Connect), spawn_connect_ui)
            .add_systems(OnExit(AppState::Connect), cleanup_connect)
            .add_systems(
                Update,
                connect_button_system.run_if(in_state(AppState::Connect)),
            );
    }
}

#[derive(Component)]
struct ConnectRoot;

#[derive(Component)]
enum ConnectAction {
    Connect,
    Back,
}

#[derive(Component)]
struct ServerAddressField;

#[derive(Component)]
struct HistoryEntryButton;

#[derive(Component)]
struct HistoryEntryValue(String);

fn spawn_connect_ui(mut commands: Commands, theme: Res<UiTheme>) {
    let history = ConnectionHistory::load();
    let entries: Vec<String> = history.0.clone();
    commands.insert_resource(history);

    commands
        .spawn((
            ConnectRoot,
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
                min_width: Val::Px(420.0),
                max_width: Val::Px(520.0),
                max_height: Val::Percent(90.0),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            })
            .insert(BackgroundColor(theme.panel_bg))
            .with_children(|menu| {
                menu.spawn((
                    Text::new("Connect to Server"),
                    theme.text_font(FontSize::Px(32.0)),
                    TextColor(theme.text_primary),
                ));

                menu.spawn(Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(2.0),
                    margin: UiRect::vertical(Val::Px(4.0)),
                    border_radius: BorderRadius::all(Val::Px(1.0)),
                    ..default()
                })
                .insert(BackgroundColor(theme.divider));

                menu.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|field_row| {
                    field_row.spawn((
                        Text::new("Server Address"),
                        theme.text_font(theme.caption_size),
                        TextColor(theme.text_muted),
                    ));
                    field_row.spawn((
                        ServerAddressField,
                        EditableText {
                            visible_width: Some(32.0),
                            allow_newlines: false,
                            ..default()
                        },
                        TextLayout::no_wrap(),
                        theme.text_font(theme.body_size),
                        TextCursorStyle::default(),
                        Node {
                            padding: UiRect::all(Val::Px(8.0)),
                            min_width: Val::Px(320.0),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(theme.search_bg),
                        BorderColor::all(theme.slot_border),
                    ));
                });

                if !entries.is_empty() {
                    menu.spawn(Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(2.0),
                        margin: UiRect::vertical(Val::Px(4.0)),
                        border_radius: BorderRadius::all(Val::Px(1.0)),
                        ..default()
                    })
                    .insert(BackgroundColor(theme.divider));

                    menu.spawn((
                        Text::new("Past Connections"),
                        theme.text_font(theme.caption_size),
                        TextColor(theme.text_muted),
                    ));

                    menu.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        overflow: Overflow::scroll_y(),
                        max_height: Val::Px(200.0),
                        width: Val::Percent(100.0),
                        ..default()
                    })
                    .with_children(|list_area| {
                        for addr in &entries {
                            list_area
                                .spawn((
                                    HistoryEntryButton,
                                    HistoryEntryValue(addr.clone()),
                                    Button,
                                    Node {
                                        width: Val::Percent(100.0),
                                        flex_direction: FlexDirection::Row,
                                        align_items: AlignItems::Center,
                                        padding: UiRect::all(Val::Px(8.0)),
                                        border_radius: BorderRadius::all(Val::Px(4.0)),
                                        ..default()
                                    },
                                    BackgroundColor(theme.catalog_cell_bg),
                                ))
                                .with_child((
                                    Text::new(addr.as_str()),
                                    theme.text_font(theme.body_size),
                                    TextColor(theme.text_primary),
                                ));
                        }
                    });
                }

                menu.spawn(Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(2.0),
                    margin: UiRect::vertical(Val::Px(4.0)),
                    border_radius: BorderRadius::all(Val::Px(1.0)),
                    ..default()
                })
                .insert(BackgroundColor(theme.divider));

                menu.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    justify_content: JustifyContent::Center,
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_cs_button(row, "Connect", ConnectAction::Connect, &theme);
                    spawn_cs_button(row, "Back", ConnectAction::Back, &theme);
                });
            });
        });
}

fn spawn_cs_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: ConnectAction,
    theme: &UiTheme,
) {
    parent
        .spawn((
            action,
            Button,
            Node {
                width: Val::Px(180.0),
                height: Val::Px(40.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(theme.button_bg),
        ))
        .with_child((
            Text::new(label),
            theme.text_font(theme.subtitle_size),
            TextColor(theme.text_primary),
        ));
}

fn connect_button_system(
    mut next_state: ResMut<NextState<AppState>>,
    mut net: ResMut<GameNetClient>,
    mut history: ResMut<ConnectionHistory>,
    theme: Res<UiTheme>,
    mut queries: ParamSet<(
        Query<
            (&Interaction, &HistoryEntryValue, &mut BackgroundColor),
            (Changed<Interaction>, With<Button>),
        >,
        Query<
            (&Interaction, &ConnectAction, &mut BackgroundColor),
            (Changed<Interaction>, With<Button>),
        >,
    )>,
    address_fields: Query<&EditableText, With<ServerAddressField>>,
) {
    // Handle history entry clicks (connect directly)
    for (interaction, addr, mut bg) in &mut queries.p0() {
        match *interaction {
            Interaction::Pressed => {
                let addr = normalize_addr(&addr.0);
                history.push_front(addr.clone());
                history.save();

                net.connect_addr = Some(addr);
                net.embedded = false;
                next_state.set(AppState::Loading);
                continue;
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(theme.button_hover);
            }
            Interaction::None => {
                *bg = BackgroundColor(theme.catalog_cell_bg);
            }
        }
    }

    // Handle action buttons
    for (interaction, action, mut bg) in &mut queries.p1() {
        match *interaction {
            Interaction::Pressed => match action {
                ConnectAction::Connect => {
                    let addr = address_fields
                        .iter()
                        .next()
                        .map(|t| t.value().to_string())
                        .unwrap_or_default();
                    let addr = normalize_addr(&addr);
                    if addr.is_empty() || addr.starts_with(':') {
                        continue;
                    }

                    history.push_front(addr.clone());
                    history.save();

                    net.connect_addr = Some(addr);
                    net.embedded = false;
                    next_state.set(AppState::Loading);
                }
                ConnectAction::Back => {
                    next_state.set(AppState::MainMenu);
                }
            },
            Interaction::Hovered => {
                *bg = BackgroundColor(theme.button_hover);
            }
            Interaction::None => {
                *bg = BackgroundColor(theme.button_bg);
            }
        }
    }
}

fn cleanup_connect(
    mut commands: Commands,
    roots: Query<Entity, With<ConnectRoot>>,
) {
    for e in &roots {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<ConnectionHistory>();
}
