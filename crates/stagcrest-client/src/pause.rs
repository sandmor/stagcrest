use crate::block_outline;
use crate::chat::ChatUiState;
use crate::client_content::{cleanup_screen, spawn_screen_button};
use crate::game::{AppState, GameplayState};
use crate::graphics_settings_screen::GraphicsReturn;
use crate::game_session::in_paused_gameplay;
use crate::game_session::GameCamera;
use crate::player;
use crate::targeting::BlockTarget;
use crate::ui::UiTheme;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};
use stagcrest_render::BlockOutlineMarker;

pub struct PausePlugin;

#[derive(Component)]
struct PauseRoot;

#[derive(Component)]
enum PauseAction {
    Resume,
    Graphics,
    MainMenu,
}

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                toggle_pause.run_if(in_state(AppState::InGame)),
                pause_button_system.run_if(in_paused_gameplay),
            ),
        )
        .add_systems(
            OnEnter(GameplayState::Paused),
            (spawn_pause_menu, hide_block_outline_on_pause),
        )
        .add_systems(OnExit(GameplayState::Paused), cleanup_screen::<PauseRoot>);
    }
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    gameplay: Res<State<GameplayState>>,
    chat: Res<ChatUiState>,
    mut next: ResMut<NextState<GameplayState>>,
    mut ctrl: Query<&mut player::PlayerController, With<GameCamera>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }

    if chat.input_open {
        return;
    }

    match gameplay.get() {
        GameplayState::Active => {
            if let Ok(mut ctrl) = ctrl.single_mut() {
                if ctrl.captured {
                    if let Ok(mut c) = cursor.single_mut() {
                        player::release_cursor(&mut ctrl, &mut c);
                    }
                    return;
                }
            }
            next.set(GameplayState::Paused);
        }
        GameplayState::Paused => next.set(GameplayState::Active),
    }
}

fn spawn_pause_menu(
    mut commands: Commands,
    theme: Res<UiTheme>,
    mut ctrl: Query<&mut player::PlayerController, With<GameCamera>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if let Ok(mut ctrl) = ctrl.single_mut() {
        if let Ok(mut c) = cursor.single_mut() {
            player::release_cursor(&mut ctrl, &mut c);
        }
    }

    commands
        .spawn((
            PauseRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme.overlay_bg),
        ))
        .with_children(|parent| {
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(12.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                })
                .insert(BackgroundColor(theme.panel_bg))
                .with_children(|menu| {
                    menu.spawn((
                        Text::new("Paused"),
                        theme.text_font(FontSize::Px(32.0)),
                        TextColor(theme.text_primary),
                    ));
                    spawn_screen_button(menu, "Resume", PauseAction::Resume, &theme);
                    spawn_screen_button(menu, "Graphics", PauseAction::Graphics, &theme);
                    spawn_screen_button(menu, "Main Menu", PauseAction::MainMenu, &theme);
                });
        });
}

fn pause_button_system(
    mut interaction: Query<(&Interaction, &PauseAction), (Changed<Interaction>, With<Button>)>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_gameplay: ResMut<NextState<GameplayState>>,
    mut return_to: ResMut<GraphicsReturn>,
) {
    for (interaction, action) in &mut interaction {
        if *interaction == Interaction::Pressed {
            match action {
                PauseAction::Resume => next_gameplay.set(GameplayState::Active),
                PauseAction::Graphics => {
                    return_to.0 = AppState::InGame;
                    next_app.set(AppState::GraphicsSettings);
                }
                PauseAction::MainMenu => next_app.set(AppState::MainMenu),
            }
        }
    }
}

fn hide_block_outline_on_pause(
    mut target: ResMut<BlockTarget>,
    mut outline: Query<&mut Visibility, With<BlockOutlineMarker>>,
) {
    block_outline::hide_block_outline(&mut target, &mut outline);
}
