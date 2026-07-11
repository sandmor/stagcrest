use crate::client_content::{
    cleanup_screen, handle_button_hover, spawn_primary_button, ClientContentSettings,
};
use crate::game::AppState;
use crate::graphics;
use crate::graphics_settings_screen::GraphicsReturn;
use crate::ui::UiTheme;
use crate::LaunchConfig;
use bevy::prelude::*;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), on_enter_main_menu)
            .add_systems(OnExit(AppState::MainMenu), cleanup_screen::<MainMenuRoot>)
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
    ResourcePacks,
    Graphics,
    Connect,
    Quit,
}

fn on_enter_main_menu(
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    theme: Res<UiTheme>,
) {
    let settings = ClientContentSettings::reload();
    let needs_setup = !settings.0.has_enabled_pack() && !settings.0.resource_pack_setup_dismissed();
    let graphics = graphics::graphics_settings_from_section(settings.0.graphics());
    commands.insert_resource(settings);
    commands.insert_resource(graphics);

    if needs_setup {
        next_state.set(AppState::ResourcePackSetup);
        return;
    }

    spawn_main_menu_ui(&mut commands, &theme);
}

fn spawn_main_menu_ui(commands: &mut Commands, theme: &UiTheme) {
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
            spawn_primary_button(parent, "Play", MenuButton::Play, theme);
            spawn_primary_button(parent, "Resource Packs", MenuButton::ResourcePacks, theme);
            spawn_primary_button(parent, "Graphics", MenuButton::Graphics, theme);
            spawn_primary_button(parent, "Connect", MenuButton::Connect, theme);
            spawn_primary_button(parent, "Quit", MenuButton::Quit, theme);
        });
}

fn menu_button_system(
    theme: Res<UiTheme>,
    mut interaction_query: Query<
        (&Interaction, &MenuButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    launch: Res<LaunchConfig>,
    mut next_state: ResMut<NextState<AppState>>,
    mut return_to: ResMut<GraphicsReturn>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, action, mut bg) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => match action {
                MenuButton::Play => {
                    if launch.connect.is_some() {
                        next_state.set(AppState::Loading);
                    } else {
                        next_state.set(AppState::WorldSelect);
                    }
                }
                MenuButton::ResourcePacks => {
                    next_state.set(AppState::ResourcePacks);
                }
                MenuButton::Graphics => {
                    return_to.0 = AppState::MainMenu;
                    next_state.set(AppState::GraphicsSettings);
                }
                MenuButton::Connect => {
                    next_state.set(AppState::Connect);
                }
                MenuButton::Quit => {
                    exit.write(AppExit::Success);
                }
            },
            Interaction::Hovered | Interaction::None => {
                handle_button_hover(*interaction, &theme, &mut bg, theme.button_bg);
            }
        }
    }
}
