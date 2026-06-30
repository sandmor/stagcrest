use super::hotbar::InventoryScreenSlot;
use super::screen::CatalogCell;
use super::state::{CreativeInventory, InventoryUiState, SlotKind};
use crate::block_icons::BlockIconCache;
use crate::player::{release_cursor, FlyCamera, SelectedBlock};
use crate::ui::UiTheme;
use bevy::input_focus::InputFocus;
use bevy::picking::events::{Pointer, Press};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::widget::NodeImageMode;
use bevy::window::{CursorOptions, PrimaryWindow};

#[derive(Component)]
pub struct CursorGhost;

/// True when keyboard input should go to a focused [`EditableText`] widget.
pub(crate) fn editable_text_has_focus(
    input_focus: &InputFocus,
    editable: &Query<Entity, With<EditableText>>,
) -> bool {
    input_focus
        .get()
        .is_some_and(|entity| editable.contains(entity))
}

pub fn toggle_inventory_screen(
    keys: Res<ButtonInput<KeyCode>>,
    editable: Query<Entity, With<EditableText>>,
    mut ui: ResMut<InventoryUiState>,
    mut commands: Commands,
    theme: Res<UiTheme>,
    mod_ctx: Option<Res<crate::game::ModContext>>,
    icons: Option<Res<BlockIconCache>>,
    mut fly: Query<&mut FlyCamera>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    existing: Query<Entity, With<super::screen::InventoryScreenRoot>>,
    ghost: Query<Entity, With<CursorGhost>>,
    mut input_focus: ResMut<InputFocus>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    // Let the focused search field receive printable keys (including E).
    if editable_text_has_focus(&input_focus, &editable) {
        return;
    }

    ui.open = !ui.open;

    if ui.open {
        if let Ok(mut fly) = fly.single_mut() {
            if let Ok(mut c) = cursor.single_mut() {
                release_cursor(&mut fly, &mut c);
            }
        }
        let (Some(ctx), Some(icons)) = (mod_ctx, icons) else {
            ui.open = false;
            return;
        };
        super::screen::spawn_inventory_screen(&mut commands, &ctx, &icons, &theme);
        spawn_cursor_ghost(&mut commands);
    } else {
        ui.cursor = None;
        input_focus.clear();
        for e in &existing {
            commands.entity(e).despawn();
        }
        for e in &ghost {
            commands.entity(e).despawn();
        }
        if let Ok(mut fly) = fly.single_mut() {
            if let Ok(mut c) = cursor.single_mut() {
                c.grab_mode = bevy::window::CursorGrabMode::Locked;
                c.visible = false;
                fly.captured = true;
            }
        }
    }
}

fn spawn_cursor_ghost(commands: &mut Commands) {
    commands
        .spawn((
            CursorGhost,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(40.0),
                height: Val::Px(40.0),
                ..default()
            },
            ImageNode::default().with_mode(NodeImageMode::Auto),
            ZIndex(1000),
        ))
        .insert(Visibility::Hidden);
}

pub fn clear_search_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<InventoryUiState>,
    mut search: Query<&mut EditableText, With<super::screen::InventorySearchField>>,
    mut input_focus: ResMut<InputFocus>,
) {
    if !ui.open || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    for mut field in &mut search {
        field.clear();
    }
    input_focus.clear();
}

/// Clears search [`InputFocus`] when the user presses outside the search field.
///
/// Attached to [`super::screen::InventoryScreenRoot`] so bubbling pointer events
/// reach it for any in-inventory click except the search box itself (which stops
/// propagation so it can keep focus for typing).
pub fn blur_search_focus_on_outside_pointer_press(
    press: On<Pointer<Press>>,
    search_fields: Query<Entity, With<super::screen::InventorySearchField>>,
    mut input_focus: ResMut<InputFocus>,
) {
    if press.button != PointerButton::Primary {
        return;
    }
    let clicked = press.original_event_target();
    if search_fields.iter().any(|search| search == clicked) {
        return;
    }
    let Some(focused) = input_focus.get() else {
        return;
    };
    if search_fields.iter().any(|search| search == focused) {
        input_focus.clear();
    }
}

/// Unified click-to-carry handler for the open inventory.
///
/// Priority order on a left click: catalog cell, then inventory slot, then
/// "empty space" (which discards whatever is held on the cursor).
pub fn inventory_click(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut inventory: ResMut<CreativeInventory>,
    mut ui: ResMut<InventoryUiState>,
    screen_slots: Query<(&Interaction, &InventoryScreenSlot)>,
    cells: Query<(&Interaction, &CatalogCell)>,
) {
    if !ui.open || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    for (interaction, cell) in &cells {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if shift {
            let kind = inventory.quick_assign_any(cell.block);
            if let SlotKind::Hotbar(i) = kind {
                inventory.selected_index = i;
            }
        } else {
            ui.cursor = Some(cell.block);
        }
        return;
    }

    for (interaction, slot) in &screen_slots {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if shift {
            inventory.shift_move_slot(slot.kind);
        } else if let Some(held) = ui.cursor {
            let previous = inventory.get_slot(slot.kind);
            inventory.set_slot(slot.kind, Some(held));
            ui.cursor = previous;
        } else {
            ui.cursor = inventory.pick_up(slot.kind);
        }
        if let SlotKind::Hotbar(i) = slot.kind {
            inventory.selected_index = i;
        }
        return;
    }

    if ui.cursor.is_some() {
        ui.cursor = None;
    }
}

pub fn inventory_cursor_ghost(
    ui: Res<InventoryUiState>,
    icons: Option<Res<BlockIconCache>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut ghost: Query<(&mut Node, &mut Visibility, &mut ImageNode), With<CursorGhost>>,
) {
    let Ok(window) = window.single() else { return };
    let Some(icons) = icons else { return };

    let display = ui.cursor;

    for (mut node, mut vis, mut image) in &mut ghost {
        match display {
            Some(block) => {
                *vis = Visibility::Inherited;
                image.image = icons.get(block);
                image.image_mode = NodeImageMode::Auto;
                if let Some(cursor) = window.cursor_position() {
                    node.left = Val::Px(cursor.x - 20.0);
                    node.top = Val::Px(cursor.y - 20.0);
                }
            }
            None => {
                *vis = Visibility::Hidden;
            }
        }
    }
}

pub fn sync_selected_block(
    inventory: Res<CreativeInventory>,
    mod_ctx: Option<Res<crate::game::ModContext>>,
    mut selected: ResMut<SelectedBlock>,
) {
    if !inventory.is_changed() {
        return;
    }
    let fallback = mod_ctx
        .as_ref()
        .and_then(|ctx| ctx.registry.placeable_blocks().first().copied());
    if let Some(block) = inventory.selected_block().or(fallback) {
        selected.0 = block;
    }
}
