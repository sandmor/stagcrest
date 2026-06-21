use crate::game::AppState;
use bevy::prelude::*;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
            .add_systems(OnExit(AppState::MainMenu), cleanup_menu)
            .add_systems(
                Update,
                menu_button_system.run_if(in_state(AppState::MainMenu)),
            );
    }
}

#[derive(Component)]
struct MainMenuRoot;

#[derive(Component)]
enum MenuButton {
    Play,
    Quit,
}

fn spawn_main_menu(mut commands: Commands) {
    commands
        .spawn((
            MainMenuRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.09, 0.12)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Stagcrest"),
                TextFont {
                    font_size: 64.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.85, 0.7)),
            ));
            parent.spawn((
                Text::new("Mod-first voxel engine"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.6, 0.65)),
            ));
            spawn_button(parent, "Play", MenuButton::Play);
            spawn_button(parent, "Quit", MenuButton::Quit);
        });
}

fn spawn_button(parent: &mut ChildSpawnerCommands, label: &str, action: MenuButton) {
    parent
        .spawn((
            action,
            Button,
            Node {
                width: Val::Px(220.0),
                height: Val::Px(48.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.2, 0.22, 0.28)),
            BorderRadius::all(Val::Px(6.0)),
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: 22.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ));
}

fn menu_button_system(
    mut interaction_query: Query<
        (&Interaction, &MenuButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut next_state: ResMut<NextState<AppState>>,
    mut exit: EventWriter<AppExit>,
) {
    for (interaction, action, mut bg) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => match action {
                MenuButton::Play => {
                    next_state.set(AppState::Loading);
                }
                MenuButton::Quit => {
                    exit.write(AppExit::Success);
                }
            },
            Interaction::Hovered => {
                *bg = BackgroundColor(Color::srgb(0.28, 0.32, 0.4));
            }
            Interaction::None => {
                *bg = BackgroundColor(Color::srgb(0.2, 0.22, 0.28));
            }
        }
    }
}

fn cleanup_menu(mut commands: Commands, query: Query<Entity, With<MainMenuRoot>>) {
    for e in &query {
        commands.entity(e).despawn();
    }
}
