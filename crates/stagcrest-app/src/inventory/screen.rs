use super::hotbar::{InventoryScreenSlot, SlotIcon};
use super::state::{filtered_placeable, InventoryUiState, MAIN_SLOTS, HOTBAR_SLOTS};
use crate::block_icons::BlockIconCache;
use crate::game::ModContext;
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use stagcrest_protocol::BlockId;

#[derive(Component)]
pub struct InventoryScreenRoot;

#[derive(Component)]
pub struct CatalogGrid;

#[derive(Component)]
pub struct CatalogCell {
    pub block: BlockId,
}

#[derive(Component)]
pub struct SearchLabel;

pub fn spawn_inventory_screen(
    commands: &mut Commands,
    ctx: &ModContext,
    icons: &BlockIconCache,
    search: &str,
) {
    let blocks = filtered_placeable(&ctx.registry, search);

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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        padding: UiRect::all(Val::Px(16.0)),
                        max_height: Val::Percent(90.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.14, 0.15, 0.19)),
                    BorderRadius::all(Val::Px(8.0)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Creative Inventory"),
                        TextFont {
                            font_size: 22.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));

                    panel.spawn((
                        SearchLabel,
                        Text::new(format_search_label(search)),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.75, 0.75, 0.8)),
                    ));

                    panel
                        .spawn((
                            CatalogGrid,
                            Node {
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                column_gap: Val::Px(6.0),
                                row_gap: Val::Px(6.0),
                                max_width: Val::Px(540.0),
                                max_height: Val::Px(280.0),
                                overflow: Overflow::scroll_y(),
                                ..default()
                            },
                        ))
                        .with_children(|grid| {
                            for &block_id in &blocks {
                                spawn_catalog_cell(grid, ctx, icons, block_id);
                            }
                        });

                    panel.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(2.0),
                            margin: UiRect::vertical(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.4, 0.4, 0.46, 0.6)),
                        BorderRadius::all(Val::Px(1.0)),
                    ));

                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|main_area| {
                            for row in 0..3 {
                                main_area
                                    .spawn(Node {
                                        flex_direction: FlexDirection::Row,
                                        column_gap: Val::Px(4.0),
                                        ..default()
                                    })
                                    .with_children(|row_node| {
                                        for col in 0..9 {
                                            let index = row * 9 + col;
                                            spawn_main_slot(row_node, index);
                                        }
                                    });
                            }
                        });

                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|hotbar_row| {
                            for i in 0..HOTBAR_SLOTS {
                                spawn_screen_hotbar_slot(hotbar_row, i);
                            }
                        });

                    panel.spawn((
                        Text::new(
                            "LMB pick / place / swap  |  Shift+LMB quick-move  |  Click empty space to discard  |  Type to search  |  E to close",
                        ),
                        TextFont {
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.65, 0.65, 0.7)),
                    ));
                });
        });
}

fn format_search_label(search: &str) -> String {
    if search.is_empty() {
        "Search: (type to filter blocks)".to_string()
    } else {
        format!("Search: {search}")
    }
}

fn spawn_catalog_cell(
    parent: &mut ChildSpawnerCommands,
    ctx: &ModContext,
    icons: &BlockIconCache,
    block_id: BlockId,
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
                width: Val::Px(64.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.22, 0.23, 0.28)),
            BorderRadius::all(Val::Px(4.0)),
        ))
        .with_children(|cell| {
            cell.spawn((
                Node {
                    width: Val::Px(40.0),
                    height: Val::Px(40.0),
                    overflow: Overflow::clip(),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
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
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.85, 0.9)),
            ));
        });
}

fn spawn_main_slot(parent: &mut ChildSpawnerCommands, index: usize) {
    let kind = super::state::SlotKind::Main(index);
    parent
        .spawn((
            InventoryScreenSlot {
                kind,
                index,
            },
            Button,
            Node {
                width: Val::Px(52.0),
                height: Val::Px(52.0),
                padding: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.12, 0.12, 0.15, 0.85)),
            BorderColor(Color::srgba(0.35, 0.35, 0.4, 0.9)),
            BorderRadius::all(Val::Px(4.0)),
        ))
        .with_children(|slot| {
            slot.spawn((
                Node {
                    width: Val::Px(40.0),
                    height: Val::Px(40.0),
                    overflow: Overflow::clip(),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .with_children(|icon_box| {
                icon_box.spawn((
                    super::hotbar::empty_slot_image(),
                    Node {
                        width: Val::Px(40.0),
                        height: Val::Px(40.0),
                        ..default()
                    },
                    SlotIcon {
                        kind,
                        icon_index: index,
                    },
                ));
            });
        });
}

fn spawn_screen_hotbar_slot(parent: &mut ChildSpawnerCommands, index: usize) {
    let kind = super::state::SlotKind::Hotbar(index);
    parent
        .spawn((
            InventoryScreenSlot {
                kind,
                index,
            },
            Button,
            Node {
                width: Val::Px(52.0),
                height: Val::Px(52.0),
                padding: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.12, 0.12, 0.15, 0.85)),
            BorderColor(Color::srgba(0.35, 0.35, 0.4, 0.9)),
            BorderRadius::all(Val::Px(4.0)),
        ))
        .with_children(|slot| {
            slot.spawn((
                Node {
                    width: Val::Px(40.0),
                    height: Val::Px(40.0),
                    overflow: Overflow::clip(),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .with_children(|icon_box| {
                icon_box.spawn((
                    super::hotbar::empty_slot_image(),
                    Node {
                        width: Val::Px(40.0),
                        height: Val::Px(40.0),
                        ..default()
                    },
                    SlotIcon {
                        kind,
                        icon_index: index + MAIN_SLOTS,
                    },
                ));
            });
        });
}

pub fn update_search_label(ui: Res<InventoryUiState>, mut labels: Query<&mut Text, With<SearchLabel>>) {
    if !ui.is_changed() {
        return;
    }
    for mut text in &mut labels {
        **text = format_search_label(&ui.search);
    }
}

pub fn rebuild_catalog_if_needed(
    ui: Res<InventoryUiState>,
    mod_ctx: Option<Res<ModContext>>,
    icons: Option<Res<BlockIconCache>>,
    mut commands: Commands,
    grids: Query<Entity, With<CatalogGrid>>,
    children_q: Query<&Children>,
    mut last_search: Local<String>,
) {
    if !ui.open {
        last_search.clear();
        return;
    }
    if ui.search == *last_search {
        return;
    }
    *last_search = ui.search.clone();

    let (Some(ctx), Some(icons)) = (mod_ctx, icons) else {
        return;
    };

    let blocks = filtered_placeable(&ctx.registry, &ui.search);
    for grid_entity in &grids {
        if let Ok(children) = children_q.get(grid_entity) {
            for &child in children {
                commands.entity(child).despawn();
            }
        }
        commands.entity(grid_entity).with_children(|grid| {
            for &block_id in &blocks {
                spawn_catalog_cell(grid, &ctx, &icons, block_id);
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
