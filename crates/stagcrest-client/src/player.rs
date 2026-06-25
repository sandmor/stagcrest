use bevy::prelude::*;
use glam::Vec3;
use stagcrest_net::{PlayerAction, PlayerActionKind};

use crate::net_client::GameNetClient;
use crate::world_replica::WorldReplica;

#[derive(Component)]
pub struct FlyCamera {
    pub speed: f32,
    pub sensitivity: f32,
    pub captured: bool,
}

impl Default for FlyCamera {
    fn default() -> Self {
        Self {
            speed: 12.0,
            sensitivity: 0.002,
            captured: false,
        }
    }
}

#[derive(Resource, Default)]
pub struct SelectedBlock(pub stagcrest_protocol::BlockId);

pub fn camera_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: EventReader<bevy::input::mouse::MouseMotion>,
    mut camera: Query<(&mut Transform, &FlyCamera)>,
    time: Res<Time>,
) {
    let mouse_deltas: Vec<_> = motion.read().map(|ev| ev.delta).collect();

    for (mut transform, fly) in &mut camera {
        if !fly.captured {
            continue;
        }

        let mut velocity = Vec3::ZERO;
        let forward = transform.forward();
        let right = transform.right();
        if keys.pressed(KeyCode::KeyW) {
            velocity += *forward;
        }
        if keys.pressed(KeyCode::KeyS) {
            velocity -= *forward;
        }
        if keys.pressed(KeyCode::KeyA) {
            velocity -= *right;
        }
        if keys.pressed(KeyCode::KeyD) {
            velocity += *right;
        }
        if keys.pressed(KeyCode::Space) {
            velocity += Vec3::Y;
        }
        if keys.pressed(KeyCode::ShiftLeft) {
            velocity -= Vec3::Y;
        }

        if velocity.length_squared() > 0.0 {
            transform.translation += velocity.normalize() * fly.speed * time.delta_secs();
        }

        for delta in &mouse_deltas {
            let (mut yaw, mut pitch, _) = transform.rotation.to_euler(bevy::math::EulerRot::YXZ);
            yaw -= delta.x * fly.sensitivity;
            pitch -= delta.y * fly.sensitivity;
            pitch = pitch.clamp(-1.54, 1.54);
            transform.rotation = Quat::from_euler(bevy::math::EulerRot::YXZ, yaw, pitch, 0.0);
        }
    }
}

pub fn block_interaction(
    mouse: Res<ButtonInput<MouseButton>>,
    mod_ctx: Option<Res<crate::game::ModContext>>,
    world: Res<WorldReplica>,
    mut net: ResMut<GameNetClient>,
    mut selected: ResMut<SelectedBlock>,
    target: Res<crate::targeting::BlockTarget>,
    inventory_ui: Option<Res<crate::inventory::InventoryUiState>>,
    inventory: Option<ResMut<crate::inventory::CreativeInventory>>,
    camera: Query<&Transform, With<FlyCamera>>,
    fly: Query<&FlyCamera, With<FlyCamera>>,
) {
    let Some(ctx) = mod_ctx else { return };
    let Ok(fly_cam) = fly.single() else { return };

    if !fly_cam.captured {
        return;
    }

    if inventory_ui.is_some_and(|ui| ui.open) {
        return;
    }

    let hit = target.hit;

    if mouse.just_pressed(MouseButton::Middle) {
        let Some(hit) = hit else { return };
        let (id, _) = world.0.get_block(hit.block);
        let Some(def) = ctx.registry.block(id) else { return };
        if !def.placeable {
            return;
        }
        if let Some(mut inv) = inventory {
            let idx = inv.selected_index;
            inv.hotbar[idx] = Some(id);
            selected.0 = id;
        }
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) && !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    let Some(hit) = hit else { return };

    net.action_seq = net.action_seq.wrapping_add(1);
    let kind = if mouse.just_pressed(MouseButton::Left) {
        PlayerActionKind::Break
    } else {
        let (hit_id, _) = world.0.get_block(hit.block);
        if let Some(def) = ctx.registry.block(hit_id) {
            if stagcrest_circuit::is_repeater(def) || stagcrest_circuit::is_player_toggleable(def) {
                PlayerActionKind::Toggle
            } else {
                PlayerActionKind::Place
            }
        } else {
            PlayerActionKind::Place
        }
    };

    let target_pos = if kind == PlayerActionKind::Place {
        stagcrest_protocol::BlockPos::new(
            hit.block.x + hit.face_normal.x as i32,
            hit.block.y + hit.face_normal.y as i32,
            hit.block.z + hit.face_normal.z as i32,
        )
    } else {
        hit.block
    };

    let action_seq = net.action_seq;
    net.send_action(PlayerAction {
        action_seq,
        kind,
        target: target_pos,
        hotbar_slot: inventory
            .as_ref()
            .map(|i| i.selected_index as u8)
            .unwrap_or(0),
        block_id: selected.0,
    });
    let _ = camera;
}

pub fn capture_cursor(
    mut fly: Query<&mut FlyCamera>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut window: Query<&mut Window>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        if let Ok(mut w) = window.single_mut() {
            w.cursor_options.grab_mode = bevy::window::CursorGrabMode::Locked;
            w.cursor_options.visible = false;
        }
        for mut f in &mut fly {
            f.captured = true;
        }
    }
}

pub fn release_cursor(fly: &mut FlyCamera, window: &mut Window) {
    window.cursor_options.grab_mode = bevy::window::CursorGrabMode::None;
    window.cursor_options.visible = true;
    fly.captured = false;
}
