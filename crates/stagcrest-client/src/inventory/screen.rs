use super::hotbar::{InventoryScreenSlot, SlotIcon};
use super::input::blur_search_focus_on_outside_pointer_press;
use super::state::{filtered_placeable, InventoryUiState, HOTBAR_SLOTS, MAIN_SLOTS};
use crate::block_icons::BlockIconCache;
use crate::game::ModContext;
use crate::ui::UiTheme;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle, TextEditChange, TextLayout};
use bevy::ui::widget::NodeImageMode;
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use bevy::ui_widgets::{ControlOrientation, Scrollbar, ScrollbarDragState, ScrollbarThumb};
use stagcrest_protocol::BlockId;

#[derive(Component)]
pub struct InventoryScreenRoot;

#[derive(Component)]
pub struct CatalogGrid;

#[derive(Component)]
pub struct CatalogScrollArea;

#[derive(Component)]
pub struct CatalogCell {
    pub block: BlockId,
}

#[derive(Component)]
pub struct InventorySearchField;

pub fn spawn_inventory_screen(
    commands: &mut Commands,
    ctx: &ModContext,
    icons: &BlockIconCache,
    theme: &UiTheme,
) {
    let blocks = filtered_placeable(&ctx.registry, "");

    commands
        .spawn((
            InventoryScreenRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .observe(blur_search_focus_on_outside_pointer_press)
        .insert(BackgroundColor(theme.overlay_bg))
        .with_children(|overlay| {
            overlay
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    max_height: Val::Percent(90.0),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                })
                .insert(BackgroundColor(theme.panel_bg))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Creative Inventory"),
                        theme.text_font(theme.body_size),
                        TextColor(theme.text_primary),
                    ));

                    panel
                        .spawn((
                            InventorySearchField,
                            EditableText {
                                visible_width: Some(28.0),
                                allow_newlines: false,
                                ..default()
                            },
                            TextLayout::no_wrap(),
                            theme.text_font(theme.caption_size),
                            TextCursorStyle::default(),
                            Node {
                                padding: UiRect::all(Val::Px(6.0)),
                                min_width: Val::Px(220.0),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                        ))
                        .insert(BackgroundColor(theme.search_bg))
                        .insert(BorderColor::all(theme.slot_border));

                    panel
                        .spawn(Node {
                            display: Display::Grid,
                            grid_template_columns: vec![RepeatedGridTrack::px(1, 572.0), RepeatedGridTrack::px(1, 8.0)],
                            column_gap: Val::Px(8.0),
                            align_self: AlignSelf::Center,
                            height: Val::Px(280.0),
                            ..default()
                        })
                        .with_children(|scroll_container| {
                            let scroll_area_id = scroll_container
                                .spawn((
                                    CatalogScrollArea,
                                    RelativeCursorPosition::default(),
                                    FocusPolicy::Pass,
                                    Node {
                                        display: Display::Flex,
                                        flex_direction: FlexDirection::Column,
                                        overflow: Overflow::scroll_y(),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    ScrollPosition::default(),
                                ))
                                .with_children(|scroll_area| {
                                    scroll_area.spawn((
                                        CatalogGrid,
                                        Node {
                                            display: Display::Grid,
                                            grid_template_columns: vec![RepeatedGridTrack::px(9, 60.0)],
                                            column_gap: Val::Px(4.0),
                                            row_gap: Val::Px(4.0),
                                            ..default()
                                        },
                                    ))
                                    .with_children(|grid| {
                                        for &block_id in &blocks {
                                            spawn_catalog_cell(grid, ctx, icons, block_id, theme);
                                        }
                                    });
                                })
                                .id();

                            scroll_container
                                .spawn((
                                    Node {
                                        width: Val::Px(8.0),
                                        grid_column: GridPlacement::start(2),
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

                    panel.spawn(Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(2.0),
                        margin: UiRect::vertical(Val::Px(4.0)),
                        border_radius: BorderRadius::all(Val::Px(1.0)),
                        ..default()
                    })
                    .insert(BackgroundColor(theme.divider));

                    panel
                        .spawn(Node {
                            display: Display::Grid,
                            grid_template_columns: vec![RepeatedGridTrack::px(9, 60.0)],
                            column_gap: Val::Px(4.0),
                            row_gap: Val::Px(4.0),
                            align_self: AlignSelf::Center,
                            margin: UiRect::right(Val::Px(16.0)),
                            ..default()
                        })
                        .with_children(|main_area| {
                            for row in 0..3 {
                                for col in 0..9 {
                                    let index = row * 9 + col;
                                    spawn_main_slot(main_area, index, theme);
                                }
                            }
                        });

                    panel
                        .spawn(Node {
                            display: Display::Grid,
                            grid_template_columns: vec![RepeatedGridTrack::px(9, 60.0)],
                            column_gap: Val::Px(4.0),
                            align_self: AlignSelf::Center,
                            margin: UiRect::right(Val::Px(16.0)),
                            ..default()
                        })
                        .with_children(|hotbar_row| {
                            for i in 0..HOTBAR_SLOTS {
                                spawn_screen_hotbar_slot(hotbar_row, i, theme);
                            }
                        });

                    panel.spawn((
                        Text::new(
                            "LMB pick / place / swap  |  Shift+LMB quick-move  |  Click empty space to discard  |  Type to search  |  E to close",
                        ),
                        theme.text_font(theme.hint_size),
                        TextColor(theme.text_hint),
                    ));
                });
        });
}

fn spawn_catalog_cell(
    parent: &mut ChildSpawnerCommands,
    ctx: &ModContext,
    icons: &BlockIconCache,
    block_id: BlockId,
    theme: &UiTheme,
) {
    let name = ctx
        .registry
        .block(block_id)
        .map(|d| d.display_name.clone())
        .unwrap_or_else(|| "?".into());

    parent
        .spawn((
            CatalogCell { block: block_id },
            Button,
            Node {
                width: Val::Px(60.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
        ))
        .insert(BackgroundColor(theme.catalog_cell_bg))
        .with_children(|cell| {
            cell.spawn((Node {
                width: Val::Px(40.0),
                height: Val::Px(40.0),
                overflow: Overflow::clip(),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },))
                .with_children(|icon_box| {
                    icon_box.spawn((
                        ImageNode::new(icons.get(block_id)).with_mode(NodeImageMode::Auto),
                        Node {
                            width: Val::Px(40.0),
                            height: Val::Px(40.0),
                            ..default()
                        },
                    ));
                });
            cell.spawn((
                Text::new(name),
                theme.text_font(theme.hint_size),
                TextColor(theme.text_muted),
            ));
        });
}

fn spawn_main_slot(parent: &mut ChildSpawnerCommands, index: usize, theme: &UiTheme) {
    let kind = super::state::SlotKind::Main(index);
    parent
        .spawn((
            InventoryScreenSlot { kind, index },
            Button,
            Node {
                width: Val::Px(60.0),
                height: Val::Px(60.0),
                padding: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
        ))
        .insert(BackgroundColor(theme.slot_bg))
        .insert(BorderColor::all(theme.slot_border))
        .with_children(|slot| {
            slot.spawn((Node {
                width: Val::Px(40.0),
                height: Val::Px(40.0),
                overflow: Overflow::clip(),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },))
                .with_children(|icon_box| {
                    icon_box.spawn((
                        ImageNode::default().with_mode(NodeImageMode::Auto),
                        Node {
                            width: Val::Px(40.0),
                            height: Val::Px(40.0),
                            ..default()
                        },
                        SlotIcon {
                            kind,
                            icon_index: index,
                        },
                        Visibility::Hidden,
                    ));
                });
        });
}

fn spawn_screen_hotbar_slot(parent: &mut ChildSpawnerCommands, index: usize, theme: &UiTheme) {
    let kind = super::state::SlotKind::Hotbar(index);
    parent
        .spawn((
            InventoryScreenSlot { kind, index },
            Button,
            Node {
                width: Val::Px(60.0),
                height: Val::Px(60.0),
                padding: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
        ))
        .insert(BackgroundColor(theme.slot_bg))
        .insert(BorderColor::all(theme.slot_border))
        .with_children(|slot| {
            slot.spawn((Node {
                width: Val::Px(40.0),
                height: Val::Px(40.0),
                overflow: Overflow::clip(),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },))
                .with_children(|icon_box| {
                    icon_box.spawn((
                        ImageNode::default().with_mode(NodeImageMode::Auto),
                        Node {
                            width: Val::Px(40.0),
                            height: Val::Px(40.0),
                            ..default()
                        },
                        SlotIcon {
                            kind,
                            icon_index: index + MAIN_SLOTS,
                        },
                        Visibility::Hidden,
                    ));
                });
        });
}

pub fn on_search_text_changed(
    trigger: On<TextEditChange>,
    ui: Res<InventoryUiState>,
    theme: Res<UiTheme>,
    mod_ctx: Option<Res<ModContext>>,
    icons: Option<Res<BlockIconCache>>,
    search_fields: Query<Entity, With<InventorySearchField>>,
    search_text: Query<&EditableText>,
    mut commands: Commands,
    grids: Query<Entity, With<CatalogGrid>>,
    children_q: Query<&Children>,
) {
    if !ui.open {
        return;
    }
    let entity = trigger.event_target();
    if !search_fields.contains(entity) {
        return;
    }
    let Ok(editable) = search_text.get(entity) else {
        return;
    };
    let search = editable.value().to_string();

    let (Some(ctx), Some(icons)) = (mod_ctx, icons) else {
        return;
    };

    rebuild_catalog(
        &search,
        &ctx,
        &icons,
        &theme,
        &mut commands,
        &grids,
        &children_q,
    );
}

fn rebuild_catalog(
    search: &str,
    ctx: &ModContext,
    icons: &BlockIconCache,
    theme: &UiTheme,
    commands: &mut Commands,
    grids: &Query<Entity, With<CatalogGrid>>,
    children_q: &Query<&Children>,
) {
    let blocks = filtered_placeable(&ctx.registry, search);
    for grid_entity in grids {
        if let Ok(children) = children_q.get(grid_entity) {
            for &child in children {
                commands.entity(child).despawn();
            }
        }
        commands.entity(grid_entity).with_children(|grid| {
            for &block_id in &blocks {
                spawn_catalog_cell(grid, ctx, icons, block_id, theme);
            }
        });
    }
}

pub fn cleanup_inventory_screen(
    mut commands: Commands,
    query: Query<Entity, With<InventoryScreenRoot>>,
) {
    for e in &query {
        commands.entity(e).despawn();
    }
}

pub fn update_scrollbar_thumb(
    mut q_thumb: Query<
        (&mut BackgroundColor, &Hovered, &ScrollbarDragState),
        (
            With<ScrollbarThumb>,
            Or<(Changed<Hovered>, Changed<ScrollbarDragState>)>,
        ),
    >,
    theme: Res<UiTheme>,
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
