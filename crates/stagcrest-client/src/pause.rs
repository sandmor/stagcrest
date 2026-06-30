use crate::block_outline;
use crate::game::AppState;
use crate::player::{self, FlyCamera};
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
    MainMenu,
}

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                toggle_pause.run_if(in_state(AppState::InGame).or_else(in_state(AppState::Paused))),
                pause_button_system.run_if(in_state(AppState::Paused)),
            ),
        )
        .add_systems(
            OnEnter(AppState::Paused),
            (spawn_pause_menu, hide_block_outline_on_pause),
        )
        .add_systems(OnExit(AppState::Paused), cleanup_pause);
    }
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut fly: Query<&mut FlyCamera>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }

    match state.get() {
        AppState::InGame => {
            if let Ok(mut fly) = fly.single_mut() {
                if fly.captured {
                    if let Ok(mut c) = cursor.single_mut() {
                        player::release_cursor(&mut fly, &mut c);
                    }
                    return;
                }
            }
            next.set(AppState::Paused);
        }
        AppState::Paused => next.set(AppState::InGame),
        _ => {}
    }
}

fn spawn_pause_menu(
    mut commands: Commands,
    theme: Res<UiTheme>,
    mut fly: Query<&mut FlyCamera>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if let Ok(mut fly) = fly.single_mut() {
        if let Ok(mut c) = cursor.single_mut() {
            player::release_cursor(&mut fly, &mut c);
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
                    spawn_pause_btn(menu, "Resume", PauseAction::Resume, &theme);
                    spawn_pause_btn(menu, "Main Menu", PauseAction::MainMenu, &theme);
                });
        });
}

fn spawn_pause_btn(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: PauseAction,
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
        ))
        .insert(BackgroundColor(theme.button_bg))
        .with_child((
            Text::new(label),
            theme.text_font(theme.subtitle_size),
            TextColor(theme.text_primary),
        ));
}

fn pause_button_system(
    mut interaction: Query<(&Interaction, &PauseAction), (Changed<Interaction>, With<Button>)>,
    mut next: ResMut<NextState<AppState>>,
) {
    for (interaction, action) in &mut interaction {
        if *interaction == Interaction::Pressed {
            match action {
                PauseAction::Resume => next.set(AppState::InGame),
                PauseAction::MainMenu => next.set(AppState::MainMenu),
            }
        }
    }
}

fn cleanup_pause(mut commands: Commands, query: Query<Entity, With<PauseRoot>>) {
    for e in &query {
        commands.entity(e).despawn();
    }
}

fn hide_block_outline_on_pause(
    mut target: ResMut<BlockTarget>,
    mut outline: Query<&mut Visibility, With<BlockOutlineMarker>>,
) {
    block_outline::hide_block_outline(&mut target, &mut outline);
}
