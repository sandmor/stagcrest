use bevy::prelude::*;
use glam::Vec3;

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

fn block_contains_point(pos: stagcrest_protocol::BlockPos, point: Vec3) -> bool {
    let min = Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32);
    let max = min + Vec3::ONE;
    point.x >= min.x
        && point.x < max.x
        && point.y >= min.y
        && point.y < max.y
        && point.z >= min.z
        && point.z < max.z
}

pub fn block_interaction(
    mouse: Res<ButtonInput<MouseButton>>,
    mod_ctx: Option<Res<crate::game::ModContext>>,
    mut world: ResMut<crate::game::StagcrestWorldResource>,
    mut circuit: ResMut<crate::game::CircuitResource>,
    mut selected: ResMut<SelectedBlock>,
    inventory_ui: Option<Res<crate::inventory::InventoryUiState>>,
    inventory: Option<ResMut<crate::inventory::CreativeInventory>>,
    camera: Query<(&Transform, &FlyCamera), With<FlyCamera>>,
) {
    let Some(ctx) = mod_ctx else { return };
    let Ok((cam, fly)) = camera.single() else { return };

    if !fly.captured {
        return;
    }

    if inventory_ui.is_some_and(|ui| ui.open) {
        return;
    }

    let origin = glam::Vec3::new(cam.translation.x, cam.translation.y, cam.translation.z);
    let dir = glam::Vec3::new(cam.forward().x, cam.forward().y, cam.forward().z);
    let air = world.0.air();

    let hit = stagcrest_world::raycast_blocks(origin, dir, 8.0, |pos| {
        let (id, _) = world.0.get_block(pos);
        ctx.registry.block(id).map(|d| d.solid).unwrap_or(false) && id != air
    });

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

    if mouse.just_pressed(MouseButton::Left) {
        let (id, _) = world.0.get_block(hit.block);
        if ctx
            .registry
            .block(id)
            .is_some_and(|d| d.namespaced_id == "stagcrest:bedrock")
        {
            return;
        }
        let break_pos = hit.block;
        world
            .0
            .set_block(break_pos, air, stagcrest_protocol::BlockState(0));
        circuit
            .0
            .notify_block_changed(break_pos, &world.0, &ctx.registry);
    } else if mouse.just_pressed(MouseButton::Right) {
        let (hit_id, _) = world.0.get_block(hit.block);
        if let Some(def) = ctx.registry.block(hit_id) {
            if stagcrest_circuit::is_repeater(def) {
                circuit
                    .0
                    .cycle_repeater_delay(hit.block, &mut world.0, &ctx.registry);
                return;
            }
            if stagcrest_circuit::is_player_toggleable(def) {
                circuit
                    .0
                    .toggle_block(hit.block, &mut world.0, &ctx.registry);
                return;
            }
        }

        let place_pos = stagcrest_protocol::BlockPos::new(
            hit.block.x + hit.face_normal.x as i32,
            hit.block.y + hit.face_normal.y as i32,
            hit.block.z + hit.face_normal.z as i32,
        );
        if block_contains_point(place_pos, cam.translation) {
            return;
        }
        let (existing, _) = world.0.get_block(place_pos);
        if existing == air {
            let nx = hit.face_normal.x as i32;
            let ny = hit.face_normal.y as i32;
            let nz = hit.face_normal.z as i32;

            let is_solid_at = |x: i32, y: i32, z: i32| {
                let (id, _) = world.0.get_block(stagcrest_protocol::BlockPos::new(x, y, z));
                ctx.registry.block(id).map(|d| d.solid).unwrap_or(false) && id != air
            };
            let selected_name = ctx
                .registry
                .block(selected.0)
                .map(|d| d.namespaced_id.as_str());

            let block_state = match selected_name {
                Some("stagcrest:redstone_torch") => {
                    let Some(state) = stagcrest_mod_host::validate_torch_placement(
                        is_solid_at, place_pos, nx, ny, nz,
                    ) else {
                        return;
                    };
                    state
                }
                Some("stagcrest:lever") | Some("stagcrest:stone_button") => {
                    let Some(state) = stagcrest_mod_host::validate_mount_placement(
                        is_solid_at,
                        place_pos,
                        nx,
                        ny,
                        nz,
                        dir.x,
                        dir.z,
                    ) else {
                        return;
                    };
                    state
                }
                Some("stagcrest:repeater") => {
                    let Some(state) = stagcrest_mod_host::validate_repeater_placement(
                        is_solid_at,
                        place_pos,
                        nx,
                        ny,
                        nz,
                        dir.x,
                        dir.z,
                    ) else {
                        return;
                    };
                    state
                }
                _ => stagcrest_protocol::BlockState(0),
            };

            world.0.set_block(place_pos, selected.0, block_state);
            circuit
                .0
                .notify_block_changed(place_pos, &world.0, &ctx.registry);
        }
    }
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
