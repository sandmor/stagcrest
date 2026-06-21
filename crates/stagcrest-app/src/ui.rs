use crate::game::{AppState, ModContext};
use crate::player::{capture_cursor, SelectedBlock};
use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedBlock>()
            .add_systems(OnEnter(AppState::InGame), spawn_hotbar)
            .add_systems(OnExit(AppState::InGame), cleanup_hotbar)
            .add_systems(
                Update,
                (
                    hotbar_selection,
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
    block: stagcrest_protocol::BlockId,
}

fn spawn_hotbar(mut commands: Commands, mod_ctx: Option<Res<ModContext>>) {
    let Some(ctx) = mod_ctx else { return };
    let blocks: Vec<_> = ctx.registry.placeable_blocks().to_vec();
    if blocks.is_empty() {
        return;
    }

    commands.insert_resource(SelectedBlock(blocks[0]));

    commands
        .spawn((
            HotbarRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(24.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-220.0)),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            for (i, &block_id) in blocks.iter().take(9).enumerate() {
                let name = ctx
                    .registry
                    .block(block_id)
                    .map(|d| d.display_name.as_str())
                    .unwrap_or("?");
                let color = slot_color(i);
                parent
                    .spawn((
                        HotbarSlot { block: block_id },
                        Button,
                        Node {
                            width: Val::Px(48.0),
                            height: Val::Px(48.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(color),
                        BorderRadius::all(Val::Px(4.0)),
                    ))
                    .with_child((
                        Text::new(name.chars().take(1).collect::<String>()),
                        TextFont {
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
            }
        });
}

fn slot_color(i: usize) -> Color {
    const COLORS: [Color; 9] = [
        Color::srgb(0.45, 0.45, 0.5),
        Color::srgb(0.5, 0.4, 0.35),
        Color::srgb(0.35, 0.5, 0.35),
        Color::srgb(0.35, 0.4, 0.55),
        Color::srgb(0.55, 0.35, 0.35),
        Color::srgb(0.4, 0.45, 0.5),
        Color::srgb(0.5, 0.5, 0.35),
        Color::srgb(0.45, 0.35, 0.5),
        Color::srgb(0.35, 0.5, 0.5),
    ];
    COLORS[i % COLORS.len()]
}

fn hotbar_selection(
    keys: Res<ButtonInput<KeyCode>>,
    mod_ctx: Option<Res<ModContext>>,
    mut selected: ResMut<SelectedBlock>,
    slots: Query<&HotbarSlot>,
) {
    let Some(ctx) = mod_ctx else { return };
    let blocks: Vec<_> = ctx.registry.placeable_blocks().to_vec();

    for i in 0..9.min(blocks.len()) {
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
            selected.0 = blocks[i];
        }
    }

    for slot in &slots {
        if slot.block == selected.0 {
            // visual selection handled by border in future
        }
    }
}

fn cleanup_hotbar(mut commands: Commands, query: Query<Entity, With<HotbarRoot>>) {
    for e in &query {
        commands.entity(e).despawn();
    }
}
