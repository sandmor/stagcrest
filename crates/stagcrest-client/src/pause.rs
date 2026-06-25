use bevy::prelude::*;
use crate::block_outline;
use crate::game::AppState;
use crate::player::{self, FlyCamera};
use crate::targeting::BlockTarget;
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
                toggle_pause.run_if(in_state(AppState::InGame).or(in_state(AppState::Paused))),
                pause_button_system.run_if(in_state(AppState::Paused)),
            ),
        )
        .add_systems(OnEnter(AppState::Paused), (spawn_pause_menu, hide_block_outline_on_pause))
        .add_systems(OnExit(AppState::Paused), cleanup_pause);
    }
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut fly: Query<&mut FlyCamera>,
    mut window: Query<&mut Window>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }

    match state.get() {
        AppState::InGame => {
            if let Ok(mut fly) = fly.single_mut() {
                if fly.captured {
                    if let Ok(mut w) = window.single_mut() {
                        player::release_cursor(&mut fly, &mut w);
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
    mut fly: Query<&mut FlyCamera>,
    mut window: Query<&mut Window>,
) {
    if let Ok(mut fly) = fly.single_mut() {
        if let Ok(mut w) = window.single_mut() {
            player::release_cursor(&mut fly, &mut w);
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(12.0),
                        padding: UiRect::all(Val::Px(24.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.16, 0.2)),
                    BorderRadius::all(Val::Px(8.0)),
                ))
                .with_children(|menu| {
                    menu.spawn((
                        Text::new("Paused"),
                        TextFont {
                            font_size: 32.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    spawn_pause_btn(menu, "Resume", PauseAction::Resume);
                    spawn_pause_btn(menu, "Main Menu", PauseAction::MainMenu);
                });
        });
}

fn spawn_pause_btn(parent: &mut ChildSpawnerCommands, label: &str, action: PauseAction) {
    parent
        .spawn((
            action,
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
            Text::new(label),
            TextFont {
                font_size: 18.0,
                ..default()
            },
            TextColor(Color::WHITE),
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
