use super::hotbar::InventoryScreenSlot;
use super::screen::CatalogCell;
use super::state::{CreativeInventory, InventoryUiState, SlotKind};
use crate::block_icons::BlockIconCache;
use crate::player::{release_cursor, FlyCamera, SelectedBlock};
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use bevy::window::PrimaryWindow;

#[derive(Component)]
pub struct CursorGhost;

pub fn toggle_inventory_screen(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<InventoryUiState>,
    mut commands: Commands,
    mod_ctx: Option<Res<crate::game::ModContext>>,
    icons: Option<Res<BlockIconCache>>,
    mut fly: Query<&mut FlyCamera>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    existing: Query<Entity, With<super::screen::InventoryScreenRoot>>,
    ghost: Query<Entity, With<CursorGhost>>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }

    ui.open = !ui.open;

    if ui.open {
        if let Ok(mut fly) = fly.single_mut() {
            if let Ok(mut w) = window.single_mut() {
                release_cursor(&mut fly, &mut w);
            }
        }
        let (Some(ctx), Some(icons)) = (mod_ctx, icons) else {
            ui.open = false;
            return;
        };
        super::screen::spawn_inventory_screen(&mut commands, &ctx, &icons, &ui.search);
        spawn_cursor_ghost(&mut commands, &icons);
    } else {
        ui.cursor = None;
        for e in &existing {
            commands.entity(e).despawn();
        }
        for e in &ghost {
            commands.entity(e).despawn();
        }
        if let Ok(mut fly) = fly.single_mut() {
            if let Ok(mut w) = window.single_mut() {
                w.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
                w.cursor_options.visible = false;
                fly.captured = true;
            }
        }
    }
}

fn spawn_cursor_ghost(commands: &mut Commands, _icons: &BlockIconCache) {
    commands.spawn((
        CursorGhost,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(40.0),
            height: Val::Px(40.0),
            ..default()
        },
        Visibility::Hidden,
        ImageNode::default().with_mode(NodeImageMode::Auto),
        ZIndex(1000),
    ));
}

pub fn inventory_search_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<InventoryUiState>,
) {
    if !ui.open {
        return;
    }

    if keys.just_pressed(KeyCode::Escape) {
        ui.search.clear();
        return;
    }

    if keys.just_pressed(KeyCode::Backspace) {
        ui.search.pop();
        return;
    }

    for key in keys.get_just_pressed() {
        if let Some(ch) = keycode_to_char(*key) {
            ui.search.push(ch);
        }
    }
}

fn keycode_to_char(key: KeyCode) -> Option<char> {
    match key {
        KeyCode::KeyA => Some('a'),
        KeyCode::KeyB => Some('b'),
        KeyCode::KeyC => Some('c'),
        KeyCode::KeyD => Some('d'),
        KeyCode::KeyE => None, // reserved for toggle
        KeyCode::KeyF => Some('f'),
        KeyCode::KeyG => Some('g'),
        KeyCode::KeyH => Some('h'),
        KeyCode::KeyI => Some('i'),
        KeyCode::KeyJ => Some('j'),
        KeyCode::KeyK => Some('k'),
        KeyCode::KeyL => Some('l'),
        KeyCode::KeyM => Some('m'),
        KeyCode::KeyN => Some('n'),
        KeyCode::KeyO => Some('o'),
        KeyCode::KeyP => Some('p'),
        KeyCode::KeyQ => Some('q'),
        KeyCode::KeyR => Some('r'),
        KeyCode::KeyS => Some('s'),
        KeyCode::KeyT => Some('t'),
        KeyCode::KeyU => Some('u'),
        KeyCode::KeyV => Some('v'),
        KeyCode::KeyW => Some('w'),
        KeyCode::KeyX => Some('x'),
        KeyCode::KeyY => Some('y'),
        KeyCode::KeyZ => Some('z'),
        KeyCode::Space => Some(' '),
        _ => None,
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

    // 1. Clicked a catalog cell: infinite supply, picks onto the cursor.
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

    // 2. Clicked an inventory slot: pick up / place / swap (or shift quick-move).
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

    // 3. Clicked empty space while holding a block: discard it.
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
