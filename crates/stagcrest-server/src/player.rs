use glam::Vec3;
use stagcrest_circuit::{is_player_toggleable, is_repeater};
use stagcrest_mod_server::{
    validate_mount_placement, validate_observer_placement, validate_piston_placement,
    validate_repeater_placement, validate_torch_placement,
};
use stagcrest_net::{BlockUpdate, PlayerAck, PlayerAction, PlayerActionKind};
use stagcrest_protocol::{
    piston_extended, piston_facing, piston_front_pos, piston_head_facing, BlockPos, BlockState,
};

use crate::GameServer;

fn is_axis_face_normal(nx: i32, ny: i32, nz: i32) -> bool {
    matches!(
        (nx.abs(), ny.abs(), nz.abs()),
        (1, 0, 0) | (0, 1, 0) | (0, 0, 1)
    )
}

pub fn apply_player_action(server: &mut GameServer, action: PlayerAction) -> PlayerAck {
    let ack = |ok: bool, reason: &str| PlayerAck {
        action_seq: action.action_seq,
        ok,
        reason: reason.to_string(),
    };

    if !server.handshake_complete {
        return ack(false, "handshake incomplete");
    }

    let air = server.air;
    let registry = &server.registry;

    match action.kind {
        PlayerActionKind::Break => {
            if !server.world.is_chunk_interactive(action.target.chunk_pos()) {
                return ack(false, "chunk not interactive");
            }
            let (id, state) = server.world.get_block(action.target);
            let break_positions = {
                let registry = &server.registry;
                if registry
                    .block(id)
                    .is_some_and(|d| d.namespaced_id == "stagcrest:bedrock")
                {
                    return ack(false, "bedrock");
                }

                let mut positions = vec![action.target];
                if let Some(def) = registry.block(id) {
                    if def.namespaced_id == "stagcrest:piston_head" {
                        let facing = piston_head_facing(state);
                        let body_pos = piston_front_pos(action.target, facing.opposite());
                        positions.push(body_pos);
                    } else if def.namespaced_id == "stagcrest:piston"
                        || def.namespaced_id == "stagcrest:sticky_piston"
                    {
                        if piston_extended(state) {
                            let head_pos = piston_front_pos(action.target, piston_facing(state));
                            positions.push(head_pos);
                        }
                    }
                }
                positions
            };

            for pos in break_positions {
                server.world.set_block(pos, air, BlockState(0));
                server.circuit.notify_block_changed(
                    pos,
                    &mut server.world,
                    &server.registry,
                );
                server.mark_chunk_dirty(pos.chunk_pos());
                server.queue_priority(stagcrest_net::GameMessage::Server(
                    stagcrest_net::ServerMessage::BlockUpdate(BlockUpdate {
                        pos,
                        id: air,
                        state: BlockState(0),
                    }),
                ));
            }
            server.circuit.tick(&mut server.world, &server.registry);
            server.broadcast_circuit_replication();
            ack(true, "")
        }
        PlayerActionKind::Place => {
            let place_pos = action.target;
            if !server.world.is_chunk_interactive(place_pos.chunk_pos()) {
                return ack(false, "chunk not interactive");
            }
            let (existing, _) = server.world.get_block(place_pos);
            if existing != air {
                return ack(false, "not air");
            }

            if let Some(pose) = server.latest_pose {
                let min = Vec3::new(place_pos.x as f32, place_pos.y as f32, place_pos.z as f32);
                let max = min + Vec3::ONE;
                let pos = pose.position();
                if pos.x >= min.x
                    && pos.x < max.x
                    && pos.y >= min.y
                    && pos.y < max.y
                    && pos.z >= min.z
                    && pos.z < max.z
                {
                    return ack(false, "inside player");
                }
            }

            let block_id = action.block_id;
            let is_solid_at = |x: i32, y: i32, z: i32| {
                let (id, _) = server.world.get_block(BlockPos::new(x, y, z));
                registry.block(id).map(|d| d.solid).unwrap_or(false) && id != air
            };

            let (dir_x, dir_y, dir_z) = server
                .latest_pose
                .map(|p| {
                    let yaw = p.yaw;
                    let pitch = p.pitch;
                    (
                        yaw.sin() * pitch.cos(),
                        -pitch.sin(),
                        yaw.cos() * pitch.cos(),
                    )
                })
                .unwrap_or((0.0, 0.0, 1.0));

            let selected_name = registry.block(block_id).map(|d| d.namespaced_id.as_str());
            let (nx, ny, nz) = (
                action.face_normal[0],
                action.face_normal[1],
                action.face_normal[2],
            );
            if !is_axis_face_normal(nx, ny, nz) {
                return ack(false, "invalid face normal");
            }
            let block_state = match selected_name {
                Some("stagcrest:redstone_torch") => {
                    let Some(state) = validate_torch_placement(is_solid_at, place_pos, nx, ny, nz)
                    else {
                        return ack(false, "invalid torch placement");
                    };
                    state
                }
                Some("stagcrest:lever") | Some("stagcrest:stone_button") => {
                    let Some(state) =
                        validate_mount_placement(is_solid_at, place_pos, nx, ny, nz, dir_x, dir_z)
                    else {
                        return ack(false, "invalid mount placement");
                    };
                    state
                }
                Some("stagcrest:repeater") => {
                    let Some(state) =
                        validate_repeater_placement(is_solid_at, place_pos, nx, ny, nz, dir_x, dir_z)
                    else {
                        return ack(false, "invalid repeater placement");
                    };
                    state
                }
                Some("stagcrest:observer") => {
                    let Some(state) =
                        validate_observer_placement(is_solid_at, place_pos, nx, ny, nz, dir_x, dir_z)
                    else {
                        return ack(false, "invalid observer placement");
                    };
                    state
                }
                Some("stagcrest:piston") | Some("stagcrest:sticky_piston") => {
                    let Some(state) = validate_piston_placement(place_pos, dir_x, dir_y, dir_z) else {
                        return ack(false, "invalid piston placement");
                    };
                    state
                }
                _ => BlockState(0),
            };

            server.world.set_block(place_pos, block_id, block_state);
            server
                .circuit
                .notify_block_changed(place_pos, &mut server.world, registry);
            server.circuit.tick(&mut server.world, registry);
            server.broadcast_circuit_replication();
            server.mark_chunk_dirty(place_pos.chunk_pos());
            let (_, block_state) = server.world.get_block(place_pos);

            server.queue_priority(stagcrest_net::GameMessage::Server(
                stagcrest_net::ServerMessage::BlockUpdate(BlockUpdate {
                    pos: place_pos,
                    id: block_id,
                    state: block_state,
                }),
            ));
            ack(true, "")
        }
        PlayerActionKind::Toggle => {
            if !server.world.is_chunk_interactive(action.target.chunk_pos()) {
                return ack(false, "chunk not interactive");
            }
            let (hit_id, _) = server.world.get_block(action.target);
            let Some(def) = registry.block(hit_id) else {
                return ack(false, "unknown block");
            };
            if is_repeater(def) {
                server
                    .circuit
                    .cycle_repeater_delay(action.target, &mut server.world, registry);
            } else if is_player_toggleable(def) {
                server
                    .circuit
                    .toggle_block(action.target, &mut server.world, registry);
            } else {
                return ack(false, "not toggleable");
            }
            server.circuit.tick(&mut server.world, registry);
            server.broadcast_circuit_replication();
            let (id, state) = server.world.get_block(action.target);
            server.queue_priority(stagcrest_net::GameMessage::Server(
                stagcrest_net::ServerMessage::BlockUpdate(BlockUpdate {
                    pos: action.target,
                    id,
                    state,
                }),
            ));
            ack(true, "")
        }
        PlayerActionKind::Pick => {
            let (id, _) = server.world.get_block(action.target);
            let Some(def) = registry.block(id) else {
                return ack(false, "unknown block");
            };
            if !def.placeable {
                return ack(false, "not placeable");
            }
            ack(true, "")
        }
    }
}
