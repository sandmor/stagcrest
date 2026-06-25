use glam::Vec3;
use stagcrest_circuit::{is_player_toggleable, is_repeater};
use stagcrest_mod_server::{
    validate_mount_placement, validate_repeater_placement, validate_torch_placement,
};
use stagcrest_net::{BlockUpdate, PlayerAck, PlayerAction, PlayerActionKind};
use stagcrest_protocol::{BlockPos, BlockState};

use crate::GameServer;

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
            if !server
                .world
                .is_chunk_interactive(action.target.chunk_pos())
            {
                return ack(false, "chunk not interactive");
            }
            let (id, _) = server.world.get_block(action.target);
            if registry
                .block(id)
                .is_some_and(|d| d.namespaced_id == "stagcrest:bedrock")
            {
                return ack(false, "bedrock");
            }
            server
                .world
                .set_block(action.target, air, BlockState(0));
            server
                .circuit
                .notify_block_changed(action.target, &server.world, registry);
            server.queue_priority(stagcrest_net::GameMessage::Server(
                stagcrest_net::ServerMessage::BlockUpdate(BlockUpdate {
                    pos: action.target,
                    id: air,
                    state: BlockState(0),
                }),
            ));
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
                let min = Vec3::new(
                    place_pos.x as f32,
                    place_pos.y as f32,
                    place_pos.z as f32,
                );
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

            let (dir_x, dir_z) = server
                .latest_pose
                .map(|p| (p.yaw.sin(), p.yaw.cos()))
                .unwrap_or((0.0, 1.0));

            let selected_name = registry.block(block_id).map(|d| d.namespaced_id.as_str());
            let block_state = match selected_name {
                Some("stagcrest:redstone_torch") => {
                    let Some(state) = validate_torch_placement(is_solid_at, place_pos, 0, -1, 0)
                    else {
                        return ack(false, "invalid torch placement");
                    };
                    state
                }
                Some("stagcrest:lever") | Some("stagcrest:stone_button") => {
                    let Some(state) = validate_mount_placement(
                        is_solid_at, place_pos, 0, -1, 0, dir_x, dir_z,
                    ) else {
                        return ack(false, "invalid mount placement");
                    };
                    state
                }
                Some("stagcrest:repeater") => {
                    let Some(state) = validate_repeater_placement(
                        is_solid_at, place_pos, 0, -1, 0, dir_x, dir_z,
                    ) else {
                        return ack(false, "invalid repeater placement");
                    };
                    state
                }
                _ => BlockState(0),
            };

            server.world.set_block(place_pos, block_id, block_state);
            server
                .circuit
                .notify_block_changed(place_pos, &server.world, registry);
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
            if !server
                .world
                .is_chunk_interactive(action.target.chunk_pos())
            {
                return ack(false, "chunk not interactive");
            }
            let (hit_id, _) = server.world.get_block(action.target);
            let Some(def) = registry.block(hit_id) else {
                return ack(false, "unknown block");
            };
            if is_repeater(def) {
                server.circuit.cycle_repeater_delay(
                    action.target,
                    &mut server.world,
                    registry,
                );
            } else if is_player_toggleable(def) {
                server
                    .circuit
                    .toggle_block(action.target, &mut server.world, registry);
            } else {
                return ack(false, "not toggleable");
            }
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
