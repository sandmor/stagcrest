use bevy::prelude::*;
use stagcrest_protocol::{
    AttachFace, BlockDef, BlockGeometry, BlockPos, BlockState, CircuitKind, Facing, ModelId,
    mount_face, mount_facing, mount_on, repeater_delay_ticks, repeater_facing, repeater_powered,
    torch_attachment, torch_lit, TorchAttachment,
};
use stagcrest_world::RaycastHit;

use crate::game::{
    AppState, CircuitResource, GameConfig, ModContext, StagcrestWorldResource,
};
use crate::player::{FlyCamera, SelectedBlock};
use crate::targeting::BlockTarget;

const LABEL_WIDTH: usize = 7;

pub struct DebugPlugin;

#[derive(Resource, Default)]
pub struct DebugOverlayVisible(pub bool);

#[derive(Component)]
pub struct DebugOverlayRoot;

#[derive(Component)]
struct DebugOverlayText;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugOverlayVisible>()
            .add_systems(OnEnter(AppState::InGame), spawn_debug_overlay)
            .add_systems(
                Update,
                (toggle_debug_overlay, update_debug_overlay)
                    .run_if(in_state(AppState::InGame).or(in_state(AppState::Paused))),
            );
    }
}

pub fn cleanup_debug_overlay(
    commands: &mut Commands,
    roots: &Query<Entity, With<DebugOverlayRoot>>,
) {
    for entity in roots {
        commands.entity(entity).despawn();
    }
    commands.insert_resource(DebugOverlayVisible(false));
}

pub fn spawn_debug_overlay(mut commands: Commands) {
    commands
        .spawn((
            DebugOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.12, 0.12, 0.15, 0.85)),
            BorderRadius::all(Val::Px(4.0)),
            Visibility::Hidden,
        ))
        .with_child((
            DebugOverlayText,
            Text::new(""),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgba(0.95, 0.95, 0.95, 1.0)),
        ));
}

fn toggle_debug_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut visible: ResMut<DebugOverlayVisible>,
    mut roots: Query<&mut Visibility, With<DebugOverlayRoot>>,
) {
    if !keys.just_pressed(KeyCode::F3) {
        return;
    }
    visible.0 = !visible.0;
    for mut visibility in &mut roots {
        *visibility = if visible.0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn update_debug_overlay(
    visible: Res<DebugOverlayVisible>,
    time: Res<Time>,
    mod_ctx: Option<Res<ModContext>>,
    world: Option<Res<StagcrestWorldResource>>,
    circuit: Option<Res<CircuitResource>>,
    config: Res<GameConfig>,
    target: Res<BlockTarget>,
    selected: Res<SelectedBlock>,
    camera: Query<(&Transform, &FlyCamera), With<FlyCamera>>,
    mut text: Query<&mut Text, With<DebugOverlayText>>,
    mut fps_smooth: Local<f32>,
) {
    if !visible.0 {
        return;
    }

    let Ok((transform, fly)) = camera.single() else {
        return;
    };
    let Ok(mut label) = text.single_mut() else {
        return;
    };

    let delta = time.delta_secs();
    if delta > 0.0 {
        let instant_fps = 1.0 / delta;
        if *fps_smooth <= 0.0 {
            *fps_smooth = instant_fps;
        } else {
            *fps_smooth = *fps_smooth * 0.9 + instant_fps * 0.1;
        }
    }

    **label = format_debug_text(
        transform,
        fly,
        mod_ctx.as_deref(),
        world.as_deref(),
        circuit.as_deref(),
        &config,
        &target,
        selected.0,
        *fps_smooth,
    );
}

fn format_debug_text(
    transform: &Transform,
    fly: &FlyCamera,
    mod_ctx: Option<&ModContext>,
    world: Option<&StagcrestWorldResource>,
    circuit: Option<&CircuitResource>,
    config: &GameConfig,
    target: &BlockTarget,
    selected_id: stagcrest_protocol::BlockId,
    fps: f32,
) -> String {
    let pos = transform.translation;
    let block_pos = BlockPos::new(
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    );
    let chunk = block_pos.chunk_pos();
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    let facing = facing_from_forward(transform.forward());
    let cursor = if fly.captured { "captured" } else { "released" };

    let mut lines = vec![
        "Stagcrest Debug  [F3]".to_string(),
        String::new(),
        format!(
            "{} {:.2}, {:.2}, {:.2}",
            pad_label("XYZ"),
            pos.x,
            pos.y,
            pos.z
        ),
        format!(
            "{} ({}, {}, {})   Chunk ({}, {}, {})",
            pad_label("Block"),
            block_pos.x,
            block_pos.y,
            block_pos.z,
            chunk.x,
            chunk.y,
            chunk.z
        ),
        format!(
            "{} yaw {:.1} deg  pitch {:.1} deg  [{}]",
            pad_label("Rot"),
            yaw.to_degrees(),
            pitch.to_degrees(),
            facing
        ),
        format!("{} {cursor}", pad_label("Cursor")),
        String::new(),
    ];

    lines.extend(format_target_section(
        target.hit,
        mod_ctx,
        world,
        circuit,
    ));
    lines.push(String::new());

    let selected_name = mod_ctx
        .and_then(|ctx| ctx.registry.block(selected_id))
        .map(|def| def.namespaced_id.as_str())
        .unwrap_or("-");
    let chunk_count = world
        .map(|w| w.0.loaded_chunk_positions().count())
        .unwrap_or(0);

    lines.push(format!("{} {selected_name}", pad_label("Selected")));
    lines.push(format!(
        "{} render {}  chunks {}  fps {:.0}",
        pad_label("World"),
        config.render_distance,
        chunk_count,
        fps
    ));

    lines.join("\n")
}

fn format_target_section(
    hit: Option<RaycastHit>,
    mod_ctx: Option<&ModContext>,
    world: Option<&StagcrestWorldResource>,
    circuit: Option<&CircuitResource>,
) -> Vec<String> {
    let Some(hit) = hit else {
        return vec![format!("{} -", pad_label("Target"))];
    };

    let Some(ctx) = mod_ctx else {
        return vec![format!("{} -", pad_label("Target"))];
    };
    let Some(world) = world else {
        return vec![format!("{} -", pad_label("Target"))];
    };

    let (id, state) = world.0.get_block(hit.block);
    let Some(def) = ctx.registry.block(id) else {
        return vec![
            format!("{} unknown id {}", pad_label("Target"), id.0),
            format!(
                "        ({}, {}, {})  {} face  {:.2}m",
                hit.block.x,
                hit.block.y,
                hit.block.z,
                face_name(hit.face_normal),
                hit.distance
            ),
        ];
    };

    let mut lines = vec![
        format!("{} {}", pad_label("Target"), def.namespaced_id),
        format!(
            "        {}  id {}  state 0x{:04X}",
            def.display_name, id.0, state.0
        ),
        format!(
            "        ({}, {}, {})  {} face  {:.2}m",
            hit.block.x,
            hit.block.y,
            hit.block.z,
            face_name(hit.face_normal),
            hit.distance
        ),
    ];

    if let Some(decoded) = format_block_state(def, state) {
        let mut detail = format!("        {decoded}");
        if let Some(node) = def.circuit {
            detail.push_str(&format!("  {}", format_circuit_kind(node.kind)));
            if let Some(circuit) = circuit {
                detail.push_str(&format!("  power {}", circuit.0.power_at(hit.block)));
            }
        }
        lines.push(detail);
    } else if let Some(node) = def.circuit {
        let power = circuit
            .map(|c| c.0.power_at(hit.block))
            .unwrap_or(0);
        lines.push(format!(
            "        {}  power {power}",
            format_circuit_kind(node.kind)
        ));
    }

    let flags = format_flags(def);
    if flags != "-" {
        lines.push(format!("        {flags}"));
    }

    lines
}

pub fn format_block_state(def: &BlockDef, state: BlockState) -> Option<String> {
    match def.geometry {
        BlockGeometry::Model(ModelId::RedstoneTorch) => Some(format!(
            "{}, {}",
            if torch_lit(state) { "lit" } else { "off" },
            fmt_torch_attachment(torch_attachment(state))
        )),
        BlockGeometry::Model(ModelId::Lever | ModelId::Button) => Some(format!(
            "{}, {}, {}",
            if mount_on(state) { "on" } else { "off" },
            fmt_attach_face(mount_face(state)),
            fmt_facing(mount_facing(state))
        )),
        BlockGeometry::Model(ModelId::Repeater) => Some(format!(
            "{}, {}, {}t",
            if repeater_powered(state) { "on" } else { "off" },
            fmt_facing(repeater_facing(state)),
            repeater_delay_ticks(state)
        )),
        _ if def.circuit.is_some() => Some(format!("powered {}", state.0 & 1 == 1)),
        _ => None,
    }
}

fn format_circuit_kind(kind: CircuitKind) -> String {
    match kind {
        CircuitKind::Source { level } => format!("source {level}"),
        CircuitKind::Inverter { output } => format!("inverter out {output}"),
        CircuitKind::Wire { falloff } => format!("wire falloff {falloff}"),
        CircuitKind::Switch { output } => format!("switch out {output}"),
        CircuitKind::Delay { output, delay } => format!("delay out {output} delay {delay}"),
        CircuitKind::Repeater { output } => format!("repeater out {output}"),
    }
}

fn format_flags(def: &BlockDef) -> String {
    let mut flags = Vec::new();
    if def.solid {
        flags.push("solid");
    }
    if def.opaque {
        flags.push("opaque");
    }
    if def.placeable {
        flags.push("placeable");
    }
    if flags.is_empty() {
        "-".to_string()
    } else {
        flags.join(" ")
    }
}

fn pad_label(label: &str) -> String {
    format!("{label:<LABEL_WIDTH$}")
}

fn facing_from_forward(forward: Dir3) -> &'static str {
    let fx = forward.x;
    let fz = forward.z;
    if fx.abs() >= fz.abs() {
        if fx >= 0.0 {
            "east"
        } else {
            "west"
        }
    } else if fz >= 0.0 {
        "south"
    } else {
        "north"
    }
}

fn face_name(normal: glam::Vec3) -> &'static str {
    if normal.x > 0.5 {
        "east"
    } else if normal.x < -0.5 {
        "west"
    } else if normal.y > 0.5 {
        "up"
    } else if normal.y < -0.5 {
        "down"
    } else if normal.z > 0.5 {
        "south"
    } else {
        "north"
    }
}

fn fmt_torch_attachment(attachment: TorchAttachment) -> &'static str {
    match attachment {
        TorchAttachment::Floor => "floor",
        TorchAttachment::WallNorth => "wall_north",
        TorchAttachment::WallSouth => "wall_south",
        TorchAttachment::WallEast => "wall_east",
        TorchAttachment::WallWest => "wall_west",
    }
}

fn fmt_attach_face(face: AttachFace) -> &'static str {
    match face {
        AttachFace::Floor => "floor",
        AttachFace::Ceiling => "ceiling",
        AttachFace::Wall => "wall",
    }
}

fn fmt_facing(facing: Facing) -> &'static str {
    match facing {
        Facing::North => "north",
        Facing::South => "south",
        Facing::East => "east",
        Facing::West => "west",
    }
}
