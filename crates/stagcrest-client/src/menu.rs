use crate::game::AppState;
use crate::ui::UiTheme;
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

fn spawn_main_menu(mut commands: Commands, theme: Res<UiTheme>) {
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
        ))
        .insert(BackgroundColor(theme.screen_bg))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Stagcrest"),
                theme.text_font(theme.title_size),
                TextColor(theme.accent),
            ));
            parent.spawn((
                Text::new("Mod-first voxel engine"),
                theme.text_font(theme.subtitle_size),
                TextColor(theme.text_muted),
            ));
            spawn_button(parent, "Play", MenuButton::Play, &theme);
            spawn_button(parent, "Quit", MenuButton::Quit, &theme);
        });
}

fn spawn_button(parent: &mut ChildSpawnerCommands, label: &str, action: MenuButton, theme: &UiTheme) {
    parent
        .spawn((
            action,
            Button,
            Node {
                width: Val::Px(220.0),
                height: Val::Px(48.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
        ))
        .insert(BackgroundColor(theme.button_bg))
        .with_child((
            Text::new(label),
            theme.text_font(theme.body_size),
            TextColor(theme.text_primary),
        ));
}

fn menu_button_system(
    theme: Res<UiTheme>,
    mut interaction_query: Query<
        (&Interaction, &MenuButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut next_state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
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
                *bg = BackgroundColor(theme.button_hover);
            }
            Interaction::None => {
                *bg = BackgroundColor(theme.button_bg);
            }
        }
    }
}

fn cleanup_menu(mut commands: Commands, query: Query<Entity, With<MainMenuRoot>>) {
    for e in &query {
        commands.entity(e).despawn();
    }
}
