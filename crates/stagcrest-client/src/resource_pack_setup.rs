use crate::client_content::{
    cleanup_screen, content_data_dir, handle_button_hover, spawn_blocking_task, spawn_divider,
    spawn_primary_button, spawn_screen_button, spawn_screen_root, spawn_screen_subtitle,
    spawn_screen_title, with_screen_panel, with_scroll_area, ClientContentSettings,
};
use crate::game::AppState;
use crate::ui::UiTheme;
use bevy::prelude::*;
use bevy::tasks::{block_on, Task};
use futures_lite::future;
use stagcrest_content::ContentInstaller;
use stagcrest_modrinth::RECOMMENDED;

pub struct ResourcePackSetupPlugin;

impl Plugin for ResourcePackSetupPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SetupState>()
            .add_systems(OnEnter(AppState::ResourcePackSetup), spawn_setup_ui)
            .add_systems(
                OnExit(AppState::ResourcePackSetup),
                cleanup_screen::<SetupRoot>,
            )
            .add_systems(
                Update,
                (setup_button_system, poll_setup_download, refresh_setup_ui)
                    .run_if(in_state(AppState::ResourcePackSetup)),
            );
    }
}

#[derive(Resource, Default)]
struct SetupState {
    task: Option<Task<Result<(), String>>>,
    status: String,
    busy: bool,
    needs_refresh: bool,
}

#[derive(Component)]
struct SetupRoot;

#[derive(Component)]
enum SetupAction {
    DownloadRecommended(&'static str),
    BrowseModrinth,
    Skip,
}

fn spawn_setup_ui(mut commands: Commands, theme: Res<UiTheme>, mut state: ResMut<SetupState>) {
    *state = SetupState::default();
    build_setup_ui(&mut commands, &theme, &state);
}

fn build_setup_ui(commands: &mut Commands, theme: &UiTheme, state: &SetupState) {
    let root = spawn_screen_root(commands, theme);
    commands.entity(root).insert(SetupRoot).with_children(|root| {
        with_screen_panel(root, theme, 420.0, 520.0, |panel| {
            spawn_screen_title(panel, "Texture Packs", theme);
            spawn_screen_subtitle(
                panel,
                "Blocks use flat colors without a pack. Download one from Modrinth to get vanilla-style textures.",
                theme,
            );
            spawn_divider(panel, theme);

            if state.busy {
                panel.spawn((
                    Text::new(state.status.as_str()),
                    theme.text_font(theme.body_size),
                    TextColor(theme.accent),
                ));
                panel.spawn((
                    Text::new("This may take a minute for large packs."),
                    theme.text_font(theme.caption_size),
                    TextColor(theme.text_muted),
                ));
            } else {
                if !state.status.is_empty() {
                    panel.spawn((
                        Text::new(state.status.as_str()),
                        theme.text_font(theme.body_size),
                        TextColor(theme.error_text),
                    ));
                    spawn_divider(panel, theme);
                }

                with_scroll_area(panel, theme, |body| {
                    body.spawn((
                        Text::new("Recommended"),
                        theme.text_font(theme.caption_size),
                        TextColor(theme.text_muted),
                    ));

                    body.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        width: Val::Percent(100.0),
                        ..default()
                    })
                    .with_children(|list| {
                        for pack in RECOMMENDED {
                            list.spawn(Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(6.0),
                                width: Val::Percent(100.0),
                                padding: UiRect::all(Val::Px(10.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            })
                            .insert(BackgroundColor(theme.catalog_cell_bg))
                            .with_children(|card| {
                                card.spawn((
                                    Text::new(pack.title),
                                    theme.text_font(theme.body_size),
                                    TextColor(theme.text_primary),
                                ));
                                card.spawn((
                                    Text::new(pack.blurb),
                                    theme.text_font(FontSize::Px(13.0)),
                                    TextColor(theme.text_muted),
                                ));
                                spawn_primary_button(
                                    card,
                                    &format!("Download {}", pack.title),
                                    SetupAction::DownloadRecommended(pack.slug),
                                    theme,
                                );
                            });
                        }
                    });
                });

                spawn_divider(panel, theme);
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        align_items: AlignItems::Center,
                        width: Val::Percent(100.0),
                        ..default()
                    })
                    .with_children(|actions| {
                        spawn_screen_button(actions, "Browse Modrinth", SetupAction::BrowseModrinth, theme);
                        spawn_screen_button(
                            actions,
                            "Continue without textures",
                            SetupAction::Skip,
                            theme,
                        );
                    });
            }
        });
    });
}

fn setup_button_system(
    mut state: ResMut<SetupState>,
    mut settings: ResMut<ClientContentSettings>,
    mut next_state: ResMut<NextState<AppState>>,
    theme: Res<UiTheme>,
    mut interaction: Query<
        (&Interaction, &SetupAction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
) {
    if state.busy {
        return;
    }

    for (interaction, action, mut bg) in &mut interaction {
        match *interaction {
            Interaction::Pressed => match action {
                SetupAction::DownloadRecommended(slug) => {
                    let slug = (*slug).to_string();
                    state.busy = true;
                    state.status = format!("Downloading {slug}...");
                    state.needs_refresh = true;
                    let data_dir = content_data_dir();
                    state.task = Some(spawn_blocking_task(move || {
                        let installer = ContentInstaller::new(&data_dir);
                        installer
                            .install_modrinth_pack(&slug)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }));
                }
                SetupAction::BrowseModrinth => {
                    next_state.set(AppState::ResourcePacks);
                }
                SetupAction::Skip => {
                    settings.0.set_setup_dismissed(true);
                    let _ = settings.save();
                    next_state.set(AppState::MainMenu);
                }
            },
            Interaction::Hovered | Interaction::None => {
                handle_button_hover(*interaction, &theme, &mut bg, theme.button_bg);
            }
        }
    }
}

fn poll_setup_download(
    mut state: ResMut<SetupState>,
    mut settings: ResMut<ClientContentSettings>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let Some(task) = state.task.as_mut() else {
        return;
    };
    if !task.is_finished() {
        return;
    }
    let mut finished = state.task.take().unwrap();
    state.busy = false;
    match block_on(future::poll_once(&mut finished)) {
        Some(Ok(())) => {
            *settings = ClientContentSettings::reload();
            next_state.set(AppState::MainMenu);
        }
        Some(Err(err)) => {
            state.status = format!("Download failed: {err}");
            state.needs_refresh = true;
        }
        None => {
            state.status = "Download failed: task ended without result".to_string();
            state.needs_refresh = true;
        }
    }
}

fn refresh_setup_ui(
    mut commands: Commands,
    mut state: ResMut<SetupState>,
    theme: Res<UiTheme>,
    roots: Query<Entity, With<SetupRoot>>,
) {
    if !state.needs_refresh {
        return;
    }
    state.needs_refresh = false;
    let snapshot = SetupState {
        task: None,
        status: state.status.clone(),
        busy: state.busy,
        needs_refresh: false,
    };
    for e in &roots {
        commands.entity(e).despawn();
    }
    build_setup_ui(&mut commands, &theme, &snapshot);
}
