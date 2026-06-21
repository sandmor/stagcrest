use crate::block_icons::{bake_block_icons, BlockIconCache};
use crate::game::{AppState, ModContext};
use crate::player::{capture_cursor, release_cursor, FlyCamera, SelectedBlock};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use stagcrest_render::BlockAtlasResource;

pub struct UiPlugin;

#[derive(Resource)]
pub struct HotbarState {
    pub slots: [stagcrest_protocol::BlockId; 9],
    pub selected_index: usize,
}

#[derive(Resource, Default)]
pub struct CreativePickerOpen(pub bool);

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedBlock>()
            .init_resource::<CreativePickerOpen>()
            .add_systems(OnEnter(AppState::InGame), setup_inventory)
            .add_systems(
                OnExit(AppState::InGame),
                (cleanup_hotbar, cleanup_picker, cleanup_inventory_resources),
            )
            .add_systems(
                Update,
                (
                    hotbar_keyboard,
                    hotbar_scroll,
                    hotbar_click,
                    sync_selected_block,
                    update_hotbar_highlight,
                    toggle_creative_picker,
                    creative_picker_click,
                    capture_cursor.run_if(in_state(AppState::InGame)),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Component)]
struct HotbarRoot;

#[derive(Component)]
struct HotbarSlot {
    index: usize,
}

#[derive(Component)]
struct PickerRoot;

#[derive(Component)]
struct PickerCell {
    block: stagcrest_protocol::BlockId,
}

fn setup_inventory(
    mut commands: Commands,
    mod_ctx: Option<Res<ModContext>>,
    atlas: Option<Res<BlockAtlasResource>>,
    mut images: ResMut<Assets<Image>>,
    icons: Option<Res<BlockIconCache>>,
) {
    let (Some(ctx), Some(atlas)) = (mod_ctx, atlas) else {
        return;
    };

    if icons.is_none() {
        let cache = bake_block_icons(&ctx, &atlas, &mut images);
        commands.insert_resource(cache);
    }

    let placeable: Vec<_> = ctx.registry.placeable_blocks().to_vec();
    if placeable.is_empty() {
        return;
    }

    let default = placeable[0];
    let mut slots = [default; 9];
    for (i, slot) in slots.iter_mut().enumerate() {
        if let Some(&id) = placeable.get(i) {
            *slot = id;
        }
    }

    commands.insert_resource(HotbarState {
        slots,
        selected_index: 0,
    });
    commands.insert_resource(SelectedBlock(default));

    spawn_hotbar(&mut commands, &ctx);
}

fn spawn_hotbar(commands: &mut Commands, _ctx: &ModContext) {
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
            for i in 0..9 {
                parent
                    .spawn((
                        HotbarSlot { index: i },
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
                                ImageNode::default().with_mode(NodeImageMode::Auto),
                                Node {
                                    width: Val::Px(40.0),
                                    height: Val::Px(40.0),
                                    ..default()
                                },
                                HotbarSlotIcon(i),
                            ));
                        });
                    });
            }
        });
}

#[derive(Component)]
struct HotbarSlotIcon(usize);

fn hotbar_keyboard(keys: Res<ButtonInput<KeyCode>>, mut hotbar: ResMut<HotbarState>) {
    for i in 0..9 {
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
            hotbar.selected_index = i;
        }
    }
}

fn hotbar_scroll(
    mut wheel: EventReader<MouseWheel>,
    mut hotbar: ResMut<HotbarState>,
    picker: Res<CreativePickerOpen>,
    fly: Query<&FlyCamera>,
) {
    if picker.0 {
        return;
    }
    let Ok(fly) = fly.single() else { return };
    if !fly.captured {
        return;
    }

    let delta: f32 = wheel.read().map(|e| e.y).sum();
    if delta == 0.0 {
        return;
    }

    let len = 9;
    if delta > 0.0 {
        hotbar.selected_index = (hotbar.selected_index + len - 1) % len;
    } else {
        hotbar.selected_index = (hotbar.selected_index + 1) % len;
    }
}

fn hotbar_click(
    mut interaction: Query<(&Interaction, &HotbarSlot), (Changed<Interaction>, With<Button>)>,
    mut hotbar: ResMut<HotbarState>,
    picker: Res<CreativePickerOpen>,
) {
    if picker.0 {
        return;
    }
    for (interaction, slot) in &mut interaction {
        if *interaction == Interaction::Pressed {
            hotbar.selected_index = slot.index;
        }
    }
}

fn sync_selected_block(hotbar: Res<HotbarState>, mut selected: ResMut<SelectedBlock>) {
    if !hotbar.is_changed() {
        return;
    }
    selected.0 = hotbar.slots[hotbar.selected_index];
}

fn update_hotbar_highlight(
    hotbar: Res<HotbarState>,
    mut slots: Query<(&HotbarSlot, &mut BorderColor)>,
    icons: Option<Res<BlockIconCache>>,
    mut images: Query<(&HotbarSlotIcon, &mut ImageNode)>,
) {
    let Some(icons) = icons else { return };

    for (slot, mut border) in &mut slots {
        let selected = slot.index == hotbar.selected_index;
        *border = if selected {
            BorderColor(Color::WHITE)
        } else {
            BorderColor(Color::srgba(0.35, 0.35, 0.4, 0.9))
        };
    }

    for (icon, mut node) in &mut images {
        let block = hotbar.slots[icon.0];
        node.image = icons.get(block);
        node.image_mode = NodeImageMode::Auto;
    }
}

fn toggle_creative_picker(
    keys: Res<ButtonInput<KeyCode>>,
    mut picker_open: ResMut<CreativePickerOpen>,
    mut commands: Commands,
    mod_ctx: Option<Res<ModContext>>,
    icons: Option<Res<BlockIconCache>>,
    mut fly: Query<&mut FlyCamera>,
    mut window: Query<&mut Window>,
    existing: Query<Entity, With<PickerRoot>>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }

    picker_open.0 = !picker_open.0;

    if picker_open.0 {
        if let Ok(mut fly) = fly.single_mut() {
            if let Ok(mut w) = window.single_mut() {
                release_cursor(&mut fly, &mut w);
            }
        }
        let (Some(ctx), Some(icons)) = (mod_ctx, icons) else {
            picker_open.0 = false;
            return;
        };
        spawn_picker(&mut commands, &ctx, &icons);
    } else {
        for e in &existing {
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

fn spawn_picker(commands: &mut Commands, ctx: &ModContext, icons: &BlockIconCache) {
    let blocks: Vec<_> = ctx.registry.placeable_blocks().to_vec();

    commands
        .spawn((
            PickerRoot,
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
                        max_height: Val::Percent(80.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.14, 0.15, 0.19)),
                    BorderRadius::all(Val::Px(8.0)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Blocks"),
                        TextFont {
                            font_size: 22.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: Val::Px(6.0),
                            row_gap: Val::Px(6.0),
                            max_width: Val::Px(420.0),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        })
                        .with_children(|grid| {
                            for &block_id in &blocks {
                                let name = ctx
                                    .registry
                                    .block(block_id)
                                    .map(|d| d.display_name.clone())
                                    .unwrap_or_else(|| "?".into());
                                grid.spawn((
                                    PickerCell { block: block_id },
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
                                            ImageNode::new(icons.get(block_id))
                                                .with_mode(NodeImageMode::Auto),
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
                        });
                    panel.spawn((
                        Text::new("Click a block to select · E to close"),
                        TextFont {
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.65, 0.65, 0.7)),
                    ));
                });
        });
}

fn creative_picker_click(
    mut interaction: Query<
        (&Interaction, &PickerCell),
        (Changed<Interaction>, With<Button>),
    >,
    mut picker_open: ResMut<CreativePickerOpen>,
    mut selected: ResMut<SelectedBlock>,
    mut hotbar: ResMut<HotbarState>,
    mut commands: Commands,
    mut fly: Query<&mut FlyCamera>,
    mut window: Query<&mut Window>,
    picker: Query<Entity, With<PickerRoot>>,
) {
    if !picker_open.0 {
        return;
    }

    for (interaction, cell) in &mut interaction {
        if *interaction != Interaction::Pressed {
            continue;
        }
        selected.0 = cell.block;
        let idx = hotbar.selected_index;
        hotbar.slots[idx] = cell.block;
        picker_open.0 = false;
        for e in &picker {
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

fn cleanup_hotbar(mut commands: Commands, query: Query<Entity, With<HotbarRoot>>) {
    for e in &query {
        commands.entity(e).despawn();
    }
}

fn cleanup_picker(mut commands: Commands, query: Query<Entity, With<PickerRoot>>) {
    for e in &query {
        commands.entity(e).despawn();
    }
}

fn cleanup_inventory_resources(
    mut commands: Commands,
    mut picker: ResMut<CreativePickerOpen>,
) {
    picker.0 = false;
    commands.remove_resource::<HotbarState>();
    commands.remove_resource::<BlockIconCache>();
}
