use bevy::camera::visibility::RenderLayers;
use bevy::math::Vec3;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};
use stagcrest_net::{PlayerAction, PlayerActionKind};

use crate::game_session::GameCamera;
use crate::net_client::GameNetClient;
use crate::world_replica::WorldReplica;
use stagcrest_render::{main_camera_layers, main_camera_third_person_layers, MainViewCamera};

/// Eye height above the player's feet (Minecraft default).
pub const PLAYER_EYE_HEIGHT: f32 = 1.62;
/// Default third-person orbit distance behind the player.
pub const THIRD_PERSON_DISTANCE: f32 = 8.0;
/// First-person default; F5 toggles to [`THIRD_PERSON_DISTANCE`].
pub const FIRST_PERSON_DISTANCE: f32 = 0.0;

/// Marks the client-local player body entity (feet position).
#[derive(Component)]
pub struct LocalPlayer;

/// Walk vs idle animation hint for the local player body.
#[derive(Component, Default)]
pub struct PlayerBodyState {
    pub walking: bool,
}

/// Look + movement settings for the third-person controller.
#[derive(Component)]
pub struct PlayerController {
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
    pub sensitivity: f32,
    pub distance: f32,
    pub captured: bool,
}

impl Default for PlayerController {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            speed: 4.3,
            sensitivity: 0.002,
            distance: FIRST_PERSON_DISTANCE,
            captured: false,
        }
    }
}

#[derive(Resource, Default)]
pub struct SelectedBlock(pub stagcrest_protocol::BlockId);

pub fn player_eye_position(feet: Vec3) -> Vec3 {
    feet + Vec3::Y * PLAYER_EYE_HEIGHT
}

pub fn player_block_pos(feet: Vec3) -> stagcrest_protocol::BlockPos {
    stagcrest_protocol::BlockPos::new(
        feet.x.floor() as i32,
        feet.y.floor() as i32,
        feet.z.floor() as i32,
    )
}

fn look_rotation(yaw: f32, pitch: f32) -> Quat {
    Quat::from_euler(bevy::math::EulerRot::YXZ, yaw, pitch, 0.0)
}

/// World transform for the game camera orbiting the player feet.
///
/// `distance == 0` places the lens at eye height (first person). A positive
/// distance pulls the lens back opposite the look direction (third person).
pub fn player_camera_transform(feet: Vec3, yaw: f32, pitch: f32, distance: f32) -> Transform {
    let eye = player_eye_position(feet);
    let rotation = look_rotation(yaw, pitch);
    // Bevy cameras look down local -Z; +Z is behind the view axis.
    let offset = rotation * Vec3::Z * distance;
    Transform::from_translation(eye + offset).with_rotation(rotation)
}

fn flat_forward(yaw: f32) -> Vec3 {
    let rot = Quat::from_euler(bevy::math::EulerRot::YXZ, yaw, 0.0, 0.0);
    (rot * Vec3::NEG_Z).normalize_or_zero()
}

fn flat_right(yaw: f32) -> Vec3 {
    let rot = Quat::from_euler(bevy::math::EulerRot::YXZ, yaw, 0.0, 0.0);
    (rot * Vec3::X).normalize_or_zero()
}

/// Mouse look, WASD movement on the player body, and third-person camera placement.
pub fn camera_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
    mut controller: Query<&mut PlayerController, With<GameCamera>>,
    mut camera: Query<&mut Transform, (With<GameCamera>, Without<LocalPlayer>)>,
    mut player: Query<
        (&mut Transform, &mut PlayerBodyState),
        (With<LocalPlayer>, Without<GameCamera>),
    >,
    time: Res<Time>,
) {
    let mouse_deltas: Vec<_> = motion.read().map(|ev| ev.delta).collect();

    let Ok(mut ctrl) = controller.single_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::F5) {
        if ctrl.distance > 0.0 {
            ctrl.distance = FIRST_PERSON_DISTANCE;
        } else {
            ctrl.distance = THIRD_PERSON_DISTANCE;
        }
    }

    let Ok((mut player_tf, mut body)) = player.single_mut() else {
        return;
    };

    if ctrl.captured {
        for delta in &mouse_deltas {
            ctrl.yaw -= delta.x * ctrl.sensitivity;
            ctrl.pitch -= delta.y * ctrl.sensitivity;
            ctrl.pitch = ctrl.pitch.clamp(-1.54, 1.54);
        }

        let mut velocity = Vec3::ZERO;
        let forward = flat_forward(ctrl.yaw);
        let right = flat_right(ctrl.yaw);
        if keys.pressed(KeyCode::KeyW) {
            velocity += forward;
        }
        if keys.pressed(KeyCode::KeyS) {
            velocity -= forward;
        }
        if keys.pressed(KeyCode::KeyA) {
            velocity -= right;
        }
        if keys.pressed(KeyCode::KeyD) {
            velocity += right;
        }
        if keys.pressed(KeyCode::Space) {
            velocity += Vec3::Y;
        }
        if keys.pressed(KeyCode::ShiftLeft) {
            velocity -= Vec3::Y;
        }

        let moving = velocity.length_squared() > 0.0;
        body.walking = moving;
        if moving {
            player_tf.translation += velocity.normalize() * ctrl.speed * time.delta_secs();
        }

        let flat_vel = Vec3::new(velocity.x, 0.0, velocity.z);
        let body_yaw = if flat_vel.length_squared() > 0.001 {
            flat_vel.x.atan2(flat_vel.z)
        } else {
            ctrl.yaw + std::f32::consts::PI
        };
        player_tf.rotation = Quat::from_rotation_y(body_yaw);
    } else {
        body.walking = false;
    }

    let Ok(mut cam_tf) = camera.single_mut() else {
        return;
    };
    *cam_tf = player_camera_transform(
        player_tf.translation,
        ctrl.yaw,
        ctrl.pitch,
        ctrl.distance,
    );
}

/// Show the local player model on the main camera only in third person.
pub fn sync_local_player_view_layers(
    controller: Query<&PlayerController, With<GameCamera>>,
    mut camera_layers: Query<&mut RenderLayers, With<MainViewCamera>>,
) {
    let Ok(ctrl) = controller.single() else {
        return;
    };
    let Ok(mut layers) = camera_layers.single_mut() else {
        return;
    };
    *layers = if ctrl.distance > 0.0 {
        main_camera_third_person_layers()
    } else {
        main_camera_layers()
    };
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
    controller: Query<&PlayerController, With<GameCamera>>,
) {
    let Some(ctx) = mod_ctx else { return };
    let Ok(ctrl) = controller.single() else { return };

    if !ctrl.captured {
        return;
    }

    if inventory_ui.is_some_and(|ui| ui.open) {
        return;
    }

    let hit = target.hit;

    if mouse.just_pressed(MouseButton::Middle) {
        let Some(hit) = hit else { return };
        let (id, _) = world.get_block(hit.block);
        let Some(def) = ctx.registry.block(id) else {
            return;
        };
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
        let (hit_id, _) = world.get_block(hit.block);
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
        face_normal: [
            hit.face_normal.x as i32,
            hit.face_normal.y as i32,
            hit.face_normal.z as i32,
        ],
    });
}

pub fn capture_cursor(
    mut controller: Query<&mut PlayerController, With<GameCamera>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        if let Ok(mut c) = cursor.single_mut() {
            c.grab_mode = bevy::window::CursorGrabMode::Locked;
            c.visible = false;
        }
        if let Ok(mut ctrl) = controller.single_mut() {
            ctrl.captured = true;
        }
    }
}

pub fn release_cursor(controller: &mut PlayerController, cursor: &mut CursorOptions) {
    cursor.grab_mode = bevy::window::CursorGrabMode::None;
    cursor.visible = true;
    controller.captured = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_person_camera_sits_behind_eye_along_look_direction() {
        let feet = Vec3::new(8.0, 64.0, 8.0);
        let cam = player_camera_transform(feet, 0.0, 0.0, THIRD_PERSON_DISTANCE);
        let eye = player_eye_position(feet);

        assert!((cam.translation.z - (eye.z + THIRD_PERSON_DISTANCE)).abs() < 1e-4);
        assert!((cam.translation.x - eye.x).abs() < 1e-4);
        assert!((cam.translation.y - eye.y).abs() < 1e-4);
    }

    #[test]
    fn first_person_camera_is_at_eye_height() {
        let feet = Vec3::new(1.0, 2.0, 3.0);
        let cam = player_camera_transform(feet, 0.5, -0.25, 0.0);
        let eye = player_eye_position(feet);

        assert!((cam.translation - eye).length() < 1e-4);
    }
}
