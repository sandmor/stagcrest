use bevy::prelude::*;
use stagcrest_protocol::{
    mount_face, mount_facing, mount_on, observer_facing, observer_powered, repeater_delay_ticks,
    repeater_facing, repeater_powered, torch_attachment, torch_lit, AttachFace, BlockDef,
    BlockGeometry, BlockPos, BlockState, ChunkPos, CircuitKind, Facing, ModelId, TorchAttachment,
};
use stagcrest_world::RaycastHit;

use crate::chunk_streaming::BiomeGridCache;
use crate::environment::PlayerEnvironment;
use crate::game::{AppState, GameConfig, ModContext};
use crate::net_client::GameNetClient;
use crate::player::{FlyCamera, SelectedBlock};
use crate::targeting::BlockTarget;
use crate::ui::UiTheme;
use crate::world_replica::WorldReplica;
use stagcrest_render::{GpuChunkCache, GpuVoxelStats};

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
                    .run_if(in_state(AppState::InGame).or_else(in_state(AppState::Paused))),
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

pub fn spawn_debug_overlay(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((
            DebugOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                padding: UiRect::all(Val::Px(10.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
        ))
        .insert(BackgroundColor(theme.panel_bg))
        .insert(Visibility::Hidden)
        .with_child((
            DebugOverlayText,
            Text::new(""),
            theme.text_font(theme.caption_size),
            TextColor(theme.text_primary),
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
    world: Option<Res<WorldReplica>>,
    biome_cache: Option<Res<BiomeGridCache>>,
    config: Res<GameConfig>,
    net: Option<Res<GameNetClient>>,
    env: Res<PlayerEnvironment>,
    target: Res<BlockTarget>,
    selected: Res<SelectedBlock>,
    camera: Query<(&Transform, &FlyCamera), With<FlyCamera>>,
    gpu_cache: Option<Res<GpuChunkCache>>,
    gpu_stats: Option<Res<GpuVoxelStats>>,
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
        biome_cache.as_deref(),
        &config,
        net.as_deref(),
        &env,
        &target,
        selected.0,
        *fps_smooth,
        gpu_cache.as_deref(),
        gpu_stats.as_deref(),
    );
}

fn format_debug_text(
    transform: &Transform,
    fly: &FlyCamera,
    mod_ctx: Option<&ModContext>,
    world: Option<&WorldReplica>,
    biome_cache: Option<&BiomeGridCache>,
    config: &GameConfig,
    net: Option<&GameNetClient>,
    env: &PlayerEnvironment,
    target: &BlockTarget,
    selected_id: stagcrest_protocol::BlockId,
    fps: f32,
    gpu_cache: Option<&GpuChunkCache>,
    gpu_stats: Option<&GpuVoxelStats>,
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

    lines.extend(format_environment_section(env, config));
    lines.extend(format_biome_section(
        mod_ctx,
        biome_cache,
        block_pos,
        chunk,
    ));
    lines.push(String::new());
    lines.extend(format_net_section(net));
    lines.push(String::new());

    lines.extend(format_target_section(
        target.hit,
        mod_ctx,
        world,
        biome_cache,
    ));
    lines.push(String::new());

    let selected_name = mod_ctx
        .and_then(|ctx| ctx.registry.block(selected_id))
        .map(|def| def.namespaced_id.as_str())
        .unwrap_or("-");
    let gpu_chunks = gpu_cache.map(|c| c.chunk_count()).unwrap_or(0);
    let total_instances = gpu_stats.map(|s| s.total_instances).unwrap_or(0);
    let chunks_emitted = gpu_stats
        .map(|s| s.chunks_emitted)
        .unwrap_or_else(|| gpu_stats.map(|s| s.chunks_rebuilt).unwrap_or(0));
    let chunks_compacted = gpu_stats.map(|s| s.chunks_compacted).unwrap_or(0);
    let buffer_mb = gpu_stats
        .map(|s| s.draw_arena_bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);
    let chunk_gpu_mb = gpu_stats
        .map(|s| s.chunk_gpu_bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);
    let scratch_mb = gpu_stats
        .map(|s| s.scratch_arena_bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or_else(|| gpu_stats.map(|s| s.scratch_bytes as f64 / (1024.0 * 1024.0)).unwrap_or(0.0));
    let total_vram_mb = gpu_stats
        .map(|s| s.total_voxel_bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);
    let overflow_warn = gpu_stats
        .map(|s| {
            if s.overflow_pending
                || s.global_overflow_buckets > 0
                || s.scratch_overflow_chunks > 0
                || s.upload_failures > 0
                || s.evictions > 0
            {
                "  OVERFLOW"
            } else {
                ""
            }
        })
        .unwrap_or("");
    let upload_failures = gpu_stats.map(|s| s.upload_failures).unwrap_or(0);
    let evictions = gpu_stats.map(|s| s.evictions).unwrap_or(0);
    let scratch_overflow = gpu_stats.map(|s| s.scratch_overflow_instances).unwrap_or(0);

    lines.push(format!("{} {selected_name}", pad_label("Selected")));
    lines.push(format!(
        "{} render {}  gpu {}  inst {}  fps {:.0}",
        pad_label("World"),
        config.render_distance,
        gpu_chunks,
        total_instances,
        fps
    ));
    lines.push(format!(
        "{} emit {}  compact {}  draw {:.1} MB  stagger {}{overflow_warn}",
        pad_label("GPU"),
        chunks_emitted,
        chunks_compacted,
        buffer_mb,
        gpu_stats.map(|s| s.staggered_rebuild_pending).unwrap_or(0),
    ));
    lines.push(format!(
        "{} tier {}  chunk {:.1} MB  scratch {:.1} MB ({} inst, max {})  total {:.1} MB",
        pad_label("VRAM"),
        config.gpu_memory_tier.label(),
        chunk_gpu_mb,
        scratch_mb,
        gpu_stats.map(|s| s.scratch_arena_total_instances).unwrap_or(0),
        gpu_stats.map(|s| s.scratch_arena_max_slot_capacity).unwrap_or(0),
        total_vram_mb,
    ));
    if upload_failures > 0 || evictions > 0 || scratch_overflow > 0 {
        lines.push(format!(
            "{} fail {}  evict {}  scratch_ov {}",
            pad_label("GPU err"),
            upload_failures,
            evictions,
            scratch_overflow,
        ));
    }

    lines.join("\n")
}

fn format_net_section(net: Option<&GameNetClient>) -> Vec<String> {
    let Some(net) = net else {
        return vec![format!("{} -", pad_label("Net"))];
    };

    let addr = net.connect_addr.as_deref().unwrap_or("embedded");
    let mut lines = vec![format!(
        "{} {} ({})",
        pad_label("Net"),
        net.transport_label(),
        addr
    )];

    let rtt = net
        .last_rtt_ms
        .map(|ms| format!("{ms:.1} ms"))
        .unwrap_or_else(|| "-".to_string());
    lines.push(format!("{} {rtt}", pad_label("RTT")));

    if net.net_config.sim_latency_ms > 0 {
        lines.push(format!(
            "{} {} ms",
            pad_label("Sim lag"),
            net.net_config.sim_latency_ms
        ));
    }

    lines
}

fn format_environment_section(env: &PlayerEnvironment, config: &GameConfig) -> Vec<String> {
    let submerged = if env.submerged { "yes" } else { "no" };

    vec![
        format!(
            "{} temp {:.2}  rain {:.2}  fog {:.3}",
            pad_label("Climate"),
            env.temperature,
            env.downfall,
            env.fog_density
        ),
        format!(
            "{} {submerged}  {:.0}%",
            pad_label("Water"),
            env.submersion * 100.0
        ),
        format!("{} {}", pad_label("Seed"), config.world_seed),
    ]
}

fn format_biome_at(
    mod_ctx: &ModContext,
    biome_cache: &BiomeGridCache,
    block_pos: BlockPos,
) -> String {
    let Some(idx) = biome_cache.biome_at(block_pos) else {
        return "-".to_string();
    };
    mod_ctx
        .biomes
        .biome_by_index(idx)
        .map(|b| format!("{} (#{})", b.namespaced_id, idx))
        .unwrap_or_else(|| format!("#{} (unknown)", idx))
}

fn format_biome_section(
    mod_ctx: Option<&ModContext>,
    biome_cache: Option<&BiomeGridCache>,
    block_pos: BlockPos,
    chunk: ChunkPos,
) -> Vec<String> {
    let (Some(ctx), Some(biome_cache)) = (mod_ctx, biome_cache) else {
        return vec![format!("{} -", pad_label("Biome"))];
    };

    let player_biome = format_biome_at(ctx, biome_cache, block_pos);
    let grid = if biome_cache.biome_grid(chunk).is_some() {
        "yes"
    } else {
        "no"
    };
    let count = ctx.biomes.biomes.len();

    vec![
        format!("{} {}  grid {grid}", pad_label("Biome"), player_biome),
        format!("{} {count}", pad_label("Biomes")),
    ]
}

fn format_target_section(
    hit: Option<RaycastHit>,
    mod_ctx: Option<&ModContext>,
    world: Option<&WorldReplica>,
    biome_cache: Option<&BiomeGridCache>,
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
    let Some(biome_cache) = biome_cache else {
        return vec![format!("{} -", pad_label("Target"))];
    };

    let (id, state) = world.get_block(hit.block);
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

    let target_biome = format_biome_at(ctx, biome_cache, hit.block);
    lines.push(format!("        biome {target_biome}"));

    if let Some(decoded) = format_block_state(def, state) {
        let mut detail = format!("        {decoded}");
        if let Some(node) = def.circuit {
            detail.push_str(&format!("  {}", format_circuit_kind(node.kind)));
        }
        lines.push(detail);
    } else if let Some(node) = def.circuit {
        lines.push(format!("        {}", format_circuit_kind(node.kind)));
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
        BlockGeometry::Model(ModelId::Observer) => Some(format!(
            "{}, {}",
            if observer_powered(state) { "on" } else { "off" },
            fmt_facing(observer_facing(state))
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
        CircuitKind::Repeater { output } => format!("repeater out {output}"),
        CircuitKind::Observer { output } => format!("observer out {output}"),
        CircuitKind::Piston { sticky } => format!("piston sticky={sticky}"),
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
