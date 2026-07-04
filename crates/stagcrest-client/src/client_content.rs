use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use bevy::text::{EditableText, TextCursorStyle, TextLayout};
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use bevy::ui_widgets::{ControlOrientation, Scrollbar, ScrollbarDragState, ScrollbarThumb};
use stagcrest_content::ContentSettings;
use stagcrest_storage::DATA_DIR;
use std::path::PathBuf;

#[derive(Component)]
pub struct ScreenScrollArea;

#[derive(Resource)]
pub struct ClientContentSettings(pub ContentSettings);

impl ClientContentSettings {
    pub fn reload() -> Self {
        let data_dir = content_data_dir();
        let settings = match ContentSettings::load(&data_dir) {
            Ok(settings) => settings,
            Err(e) => {
                tracing::warn!("failed to load content settings, using defaults: {e}");
                ContentSettings::empty(&data_dir)
            }
        };
        Self(settings)
    }

    pub fn save(&mut self) -> Result<(), stagcrest_content::SettingsError> {
        self.0.save()
    }
}

pub fn content_data_dir() -> PathBuf {
    PathBuf::from(DATA_DIR)
}

/// Run blocking network/disk work off the main thread.
pub fn spawn_blocking_task<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Task<T> {
    AsyncComputeTaskPool::get().spawn(async move { f() })
}

#[derive(Component)]
pub struct DeleteConfirmOverlay;

pub fn spawn_screen_root(commands: &mut Commands, theme: &crate::ui::UiTheme) -> Entity {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme.screen_bg),
        ))
        .id()
}

pub fn with_screen_panel(
    parent: &mut ChildSpawnerCommands,
    theme: &crate::ui::UiTheme,
    min_width: f32,
    max_width: f32,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0),
            padding: UiRect::all(Val::Px(24.0)),
            min_width: Val::Px(min_width),
            max_width: Val::Px(max_width),
            width: Val::Px(max_width),
            height: Val::Percent(85.0),
            max_height: Val::Percent(90.0),
            border_radius: BorderRadius::all(Val::Px(8.0)),
            overflow: Overflow::clip(),
            ..default()
        })
        .insert(BackgroundColor(theme.panel_bg))
        .with_children(build);
}

/// Scrollable region with a visible scrollbar (inventory-style).
pub fn with_scroll_area(
    parent: &mut ChildSpawnerCommands,
    theme: &crate::ui::UiTheme,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn(Node {
            display: Display::Grid,
            grid_template_columns: vec![
                RepeatedGridTrack::flex(1, 1.0),
                RepeatedGridTrack::px(1, 8.0),
            ],
            column_gap: Val::Px(4.0),
            width: Val::Percent(100.0),
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_height: Val::Px(0.0),
            ..default()
        })
        .with_children(|scroll_container| {
            let scroll_area_id = scroll_container
                .spawn((
                    ScreenScrollArea,
                    RelativeCursorPosition::default(),
                    FocusPolicy::Pass,
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        overflow: Overflow::scroll_y(),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    ScrollPosition::default(),
                ))
                .with_children(build)
                .id();

            scroll_container
                .spawn((
                    Node {
                        width: Val::Px(8.0),
                        grid_column: GridPlacement::start(2),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    Scrollbar {
                        orientation: ControlOrientation::Vertical,
                        target: scroll_area_id,
                        min_thumb_length: 16.0,
                    },
                ))
                .with_children(|thumb_parent| {
                    thumb_parent.spawn((
                        Hovered::default(),
                        ScrollbarThumb {
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            border: UiRect::default(),
                        },
                        BackgroundColor(theme.slot_border),
                    ));
                });
        });
}

pub fn update_scrollbar_thumb(
    mut q_thumb: Query<
        (&mut BackgroundColor, &Hovered, &ScrollbarDragState),
        (
            With<ScrollbarThumb>,
            Or<(Changed<Hovered>, Changed<ScrollbarDragState>)>,
        ),
    >,
    theme: Res<crate::ui::UiTheme>,
) {
    for (mut thumb_bg, Hovered(is_hovering), drag) in q_thumb.iter_mut() {
        let color = if *is_hovering || drag.dragging {
            theme.text_muted
        } else {
            theme.slot_border
        };
        if thumb_bg.0 != color {
            thumb_bg.0 = color;
        }
    }
}

pub fn spawn_screen_title(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    theme: &crate::ui::UiTheme,
) {
    parent.spawn((
        Text::new(title),
        theme.text_font(FontSize::Px(32.0)),
        TextColor(theme.text_primary),
    ));
}

pub fn spawn_screen_subtitle(
    parent: &mut ChildSpawnerCommands,
    text: &str,
    theme: &crate::ui::UiTheme,
) {
    parent.spawn((
        Text::new(text),
        theme.text_font(theme.body_size),
        TextColor(theme.text_muted),
    ));
}

pub fn spawn_divider(parent: &mut ChildSpawnerCommands, theme: &crate::ui::UiTheme) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Px(2.0),
            margin: UiRect::vertical(Val::Px(4.0)),
            border_radius: BorderRadius::all(Val::Px(1.0)),
            ..default()
        })
        .insert(BackgroundColor(theme.divider));
}

pub fn spawn_section_label(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    theme: &crate::ui::UiTheme,
) {
    parent.spawn((
        Text::new(label),
        theme.text_font(theme.caption_size),
        TextColor(theme.text_muted),
    ));
}

pub fn spawn_screen_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: impl Component,
    theme: &crate::ui::UiTheme,
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

pub fn spawn_primary_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: impl Component,
    theme: &crate::ui::UiTheme,
) {
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
            BackgroundColor(theme.button_bg),
        ))
        .with_child((
            Text::new(label),
            theme.text_font(theme.body_size),
            TextColor(theme.text_primary),
        ));
}

pub fn spawn_list_row<F>(parent: &mut ChildSpawnerCommands, theme: &crate::ui::UiTheme, build: F)
where
    F: FnOnce(&mut ChildSpawnerCommands),
{
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::all(Val::Px(10.0)),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            column_gap: Val::Px(8.0),
            ..default()
        })
        .insert(BackgroundColor(theme.catalog_cell_bg))
        .with_children(build);
}

pub fn spawn_row_actions(
    parent: &mut ChildSpawnerCommands,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(build);
}

pub fn spawn_compact_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: impl Component,
    theme: &crate::ui::UiTheme,
    width: f32,
) {
    parent
        .spawn((
            action,
            Button,
            Node {
                width: Val::Px(width),
                height: Val::Px(32.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme.button_bg),
        ))
        .with_child((
            Text::new(label),
            theme.text_font(FontSize::Px(14.0)),
            TextColor(theme.text_primary),
        ));
}

pub fn spawn_delete_icon_button(
    parent: &mut ChildSpawnerCommands,
    action: impl Component,
    theme: &crate::ui::UiTheme,
) {
    parent
        .spawn((
            action,
            Button,
            Node {
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme.button_bg),
        ))
        .with_child((
            Text::new("X"),
            theme.text_font(FontSize::Px(14.0)),
            TextColor(theme.error_text),
        ));
}

pub fn spawn_confirm_delete_overlay<C, X>(
    commands: &mut Commands,
    theme: &crate::ui::UiTheme,
    item_name: &str,
    subtitle: &str,
    confirm: C,
    cancel: X,
) where
    C: Component,
    X: Component,
{
    commands
        .spawn((
            DeleteConfirmOverlay,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(theme.overlay_bg),
        ))
        .with_children(|overlay| {
            overlay
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(12.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    min_width: Val::Px(320.0),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                })
                .insert(BackgroundColor(theme.panel_bg))
                .with_children(|dialog| {
                    dialog.spawn((
                        Text::new(format!("Delete \"{item_name}\"?")),
                        theme.text_font(FontSize::Px(22.0)),
                        TextColor(theme.text_primary),
                    ));
                    dialog.spawn((
                        Text::new(subtitle),
                        theme.text_font(theme.caption_size),
                        TextColor(theme.text_muted),
                    ));
                    spawn_divider(dialog, theme);
                    dialog
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(12.0),
                            justify_content: JustifyContent::Center,
                            width: Val::Percent(100.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                confirm,
                                Button,
                                Node {
                                    width: Val::Px(140.0),
                                    height: Val::Px(40.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    border_radius: BorderRadius::all(Val::Px(6.0)),
                                    ..default()
                                },
                                BackgroundColor(theme.button_bg),
                            ))
                            .with_child((
                                Text::new("Delete"),
                                theme.text_font(theme.subtitle_size),
                                TextColor(theme.error_text),
                            ));
                            row.spawn((
                                cancel,
                                Button,
                                Node {
                                    width: Val::Px(140.0),
                                    height: Val::Px(40.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    border_radius: BorderRadius::all(Val::Px(6.0)),
                                    ..default()
                                },
                                BackgroundColor(theme.button_bg),
                            ))
                            .with_child((
                                Text::new("Cancel"),
                                theme.text_font(theme.subtitle_size),
                                TextColor(theme.text_primary),
                            ));
                        });
                });
        });
}

pub fn handle_button_hover(
    interaction: Interaction,
    theme: &crate::ui::UiTheme,
    bg: &mut BackgroundColor,
    normal: Color,
) {
    match interaction {
        Interaction::Hovered => *bg = BackgroundColor(theme.button_hover),
        Interaction::None => *bg = BackgroundColor(normal),
        _ => {}
    }
}

pub fn spawn_text_input(
    parent: &mut ChildSpawnerCommands,
    marker: impl Component,
    theme: &crate::ui::UiTheme,
    initial_text: &str,
    visible_width: f32,
    min_width: f32,
) {
    let mut edit = EditableText::new(initial_text);
    edit.visible_width = Some(visible_width);
    edit.allow_newlines = false;
    parent.spawn((
        marker,
        edit,
        TextLayout::no_wrap(),
        theme.text_font(theme.body_size),
        TextCursorStyle::default(),
        Node {
            padding: UiRect::all(Val::Px(8.0)),
            min_width: Val::Px(min_width),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(theme.search_bg),
        BorderColor::all(theme.slot_border),
    ));
}

pub fn cleanup_screen<T: Component>(mut commands: Commands, query: Query<Entity, With<T>>) {
    for e in &query {
        commands.entity(e).despawn();
    }
}
