use super::state::{CreativeInventory, InventoryUiState, SlotKind, HOTBAR_SLOTS};
use crate::block_icons::BlockIconCache;
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;

#[derive(Component)]
pub struct HotbarRoot;

#[derive(Component)]
pub struct HotbarSlot {
    pub index: usize,
}

pub fn spawn_hotbar(commands: &mut Commands) {
    commands
        .spawn((
            HotbarRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(24.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-244.0)),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            for i in 0..HOTBAR_SLOTS {
                spawn_world_hotbar_slot(parent, i);
            }
        });
}

fn spawn_world_hotbar_slot(parent: &mut ChildSpawnerCommands, index: usize) {
    let kind = SlotKind::Hotbar(index);
    parent
        .spawn((
            InventorySlot { kind },
            HotbarSlot { index },
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
                    empty_slot_image(),
                    Node {
                        width: Val::Px(40.0),
                        height: Val::Px(40.0),
                        ..default()
                    },
                    SlotIcon { kind, icon_index: index },
                ));
            });
        });
}

#[derive(Component, Clone, Copy)]
pub struct InventorySlot {
    pub kind: SlotKind,
}

#[derive(Component, Clone, Copy)]
pub struct SlotIcon {
    pub kind: SlotKind,
    pub icon_index: usize,
}

/// A slot icon image that starts fully transparent so empty slots don't show
/// the default white placeholder texture before `update_hotbar_highlight` runs.
pub fn empty_slot_image() -> ImageNode {
    let mut node = ImageNode::default().with_mode(NodeImageMode::Auto);
    node.color = Color::NONE;
    node
}

pub fn hotbar_keyboard(keys: Res<ButtonInput<KeyCode>>, mut inventory: ResMut<CreativeInventory>) {
    for i in 0..HOTBAR_SLOTS {
        let key = match i {
            0 => KeyCode::Digit1,
            1 => KeyCode::Digit2,
            2 => KeyCode::Digit3,
            3 => KeyCode::Digit4,
            4 => KeyCode::Digit5,
            5 => KeyCode::Digit6,
            6 => KeyCode::Digit7,
            7 => KeyCode::Digit8,
            _ => KeyCode::Digit9,
        };
        if keys.just_pressed(key) {
            inventory.selected_index = i;
        }
    }
}

pub fn hotbar_scroll(
    mut wheel: EventReader<bevy::input::mouse::MouseWheel>,
    mut inventory: ResMut<CreativeInventory>,
) {
    let delta: f32 = wheel.read().map(|e| e.y).sum();
    if delta == 0.0 {
        return;
    }

    let len = HOTBAR_SLOTS;
    if delta > 0.0 {
        inventory.selected_index = (inventory.selected_index + len - 1) % len;
    } else {
        inventory.selected_index = (inventory.selected_index + 1) % len;
    }
}

pub fn hotbar_click(
    mut interaction: Query<(&Interaction, &HotbarSlot), (Changed<Interaction>, With<Button>)>,
    mut inventory: ResMut<CreativeInventory>,
    ui: Res<InventoryUiState>,
) {
    if ui.open {
        return;
    }
    for (interaction, slot) in &mut interaction {
        if *interaction == Interaction::Pressed {
            inventory.selected_index = slot.index;
        }
    }
}

pub fn update_hotbar_visibility(
    ui: Res<InventoryUiState>,
    mut hotbar: Query<&mut Visibility, With<HotbarRoot>>,
) {
    for mut vis in &mut hotbar {
        *vis = if ui.open {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

pub fn update_hotbar_highlight(
    inventory: Res<CreativeInventory>,
    ui: Res<InventoryUiState>,
    mut hotbar_slots: Query<(&HotbarSlot, &mut BorderColor), Without<InventoryScreenSlot>>,
    mut screen_slots: Query<(&InventoryScreenSlot, &mut BorderColor)>,
    icons: Option<Res<BlockIconCache>>,
    mut images: Query<(&SlotIcon, &mut ImageNode)>,
) {
    let Some(icons) = icons else { return };

    for (slot, mut border) in &mut hotbar_slots {
        let selected = !ui.open && slot.index == inventory.selected_index;
        *border = slot_border(selected);
    }

    for (slot, mut border) in &mut screen_slots {
        let selected = match slot.kind {
            SlotKind::Hotbar(i) => i == inventory.selected_index,
            SlotKind::Main(_) => false,
        };
        *border = slot_border(selected);
    }

    for (icon, mut node) in &mut images {
        let block = inventory.get_slot(icon.kind);
        match block {
            Some(id) => {
                node.image = icons.get(id);
                node.image_mode = NodeImageMode::Auto;
                node.color = Color::WHITE;
            }
            None => {
                node.image = Handle::default();
                node.color = Color::NONE;
            }
        }
    }
}

#[derive(Component, Clone, Copy)]
pub struct InventoryScreenSlot {
    pub kind: SlotKind,
    pub index: usize,
}

fn slot_border(selected: bool) -> BorderColor {
    if selected {
        BorderColor(Color::WHITE)
    } else {
        BorderColor(Color::srgba(0.35, 0.35, 0.4, 0.9))
    }
}

pub fn cleanup_hotbar(mut commands: Commands, query: Query<Entity, With<HotbarRoot>>) {
    for e in &query {
        commands.entity(e).despawn();
    }
}
