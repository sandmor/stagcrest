use glam::Vec3;
use stagcrest_circuit::{is_player_toggleable, is_repeater};
use stagcrest_mod_server::{
    extra_break_positions, placement_state, BehaviorCtx, BehaviorResult,
};
use stagcrest_mod_server::runtime::BehaviorHook;
use stagcrest_net::{BlockUpdate, PlayerAck, PlayerAction, PlayerActionKind};
use stagcrest_protocol::{BlockState, CallbackFlags};

use crate::client_session::{ClientId, ClientRegistry};
use crate::GameServer;

fn is_axis_face_normal(nx: i32, ny: i32, nz: i32) -> bool {
    matches!(
        (nx.abs(), ny.abs(), nz.abs()),
        (1, 0, 0) | (0, 1, 0) | (0, 0, 1)
    )
}

pub fn apply_player_action(
    server: &mut GameServer,
    clients: &mut ClientRegistry,
    client_id: ClientId,
    action: PlayerAction,
) -> PlayerAck {
    let ack = |ok: bool, reason: &str| PlayerAck {
        action_seq: action.action_seq,
        ok,
        reason: reason.to_string(),
    };

    let Some(client) = clients.get(client_id) else {
        return ack(false, "client disconnected");
    };

    if !client.handshake_complete {
        return ack(false, "handshake incomplete");
    }

    let pose = client.pose;

    let air = server.air;
    let h = server.config.render_distance;
    let v = server.config.vertical_render_distance;

    match action.kind {
        PlayerActionKind::Break => {
            if !server.world.is_chunk_interactive(action.target.chunk_pos()) {
                return ack(false, "chunk not interactive");
            }
            let (id, state) = server.world.get_block(action.target);
            let Some(def) = server.registry.block(id) else {
                return ack(false, "unknown block");
            };
            let mut break_ctx = BehaviorCtx {
                pos: action.target,
                block_id: id,
                state,
                neighbor: None,
                world: &mut server.world,
                registry: &server.registry,
                air,
                face_normal: None,
                player_yaw_pitch: None,
            };
            let break_cancelled = if let Some(mod_idx) =
                server.mod_host.behavior_registry.wasm_mod_index(id)
            {
                if def.callbacks.contains(CallbackFlags::ON_BREAK) {
                    server
                        .mod_host
                        .invoke_behavior(
                            mod_idx,
                            BehaviorHook::OnBreak,
                            action.target,
                            id,
                            state,
                            None,
                            &mut server.world,
                        )
                        .map(|r| r == BehaviorResult::Cancel)
                        .unwrap_or(false)
                } else {
                    false
                }
            } else {
                matches!(
                    server.mod_host.behavior_registry.on_break(&mut break_ctx),
                    BehaviorResult::Cancel
                )
            };
            if break_cancelled {
                return ack(false, "bedrock");
            }

            let break_positions = extra_break_positions(
                &server.mod_host.behavior_registry,
                def,
                action.target,
                state,
            );

            for pos in break_positions {
                server.world.set_block(pos, air, BlockState(0));
                server
                    .circuit
                    .notify_block_changed(pos, &mut server.world, &server.registry);
                server.mark_chunk_dirty(pos.chunk_pos());
                server.fanout_block_update(
                    clients,
                    BlockUpdate {
                        pos,
                        id: air,
                        state: BlockState(0),
                    },
                    h,
                    v,
                );
            }
            server.circuit.tick(&mut server.world, &server.registry);
            server.broadcast_circuit_replication(clients);
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

            if let Some(pose) = pose {
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
            let Some(def) = server.registry.block(block_id) else {
                return ack(false, "unknown block");
            };

            let (nx, ny, nz) = (
                action.face_normal[0],
                action.face_normal[1],
                action.face_normal[2],
            );
            if !is_axis_face_normal(nx, ny, nz) {
                return ack(false, "invalid face normal");
            }

            let place_ctx = BehaviorCtx {
                pos: place_pos,
                block_id,
                state: BlockState(0),
                neighbor: None,
                world: &mut server.world,
                registry: &server.registry,
                air,
                face_normal: Some([nx, ny, nz]),
                player_yaw_pitch: pose.map(|p| (p.yaw, p.pitch)),
            };
            let block_state = placement_state(
                &server.mod_host.behavior_registry,
                def,
                &place_ctx,
            )
            .unwrap_or(BlockState(0));

            server.world.set_block(place_pos, block_id, block_state);
            server
                .circuit
                .notify_block_changed(place_pos, &mut server.world, &server.registry);
            server.circuit.tick(&mut server.world, &server.registry);
            server.broadcast_circuit_replication(clients);
            server.mark_chunk_dirty(place_pos.chunk_pos());
            let (_, block_state) = server.world.get_block(place_pos);

            server.fanout_block_update(
                clients,
                BlockUpdate {
                    pos: place_pos,
                    id: block_id,
                    state: block_state,
                },
                h,
                v,
            );
            ack(true, "")
        }
        PlayerActionKind::Toggle => {
            if !server.world.is_chunk_interactive(action.target.chunk_pos()) {
                return ack(false, "chunk not interactive");
            }
            let (hit_id, _) = server.world.get_block(action.target);
            let Some(def) = server.registry.block(hit_id) else {
                return ack(false, "unknown block");
            };
            if is_repeater(def) {
                server
                    .circuit
                    .cycle_repeater_delay(action.target, &mut server.world, &server.registry);
            } else if is_player_toggleable(def) {
                server
                    .circuit
                    .toggle_block(action.target, &mut server.world, &server.registry);
            } else {
                return ack(false, "not toggleable");
            }
            server.circuit.tick(&mut server.world, &server.registry);
            server.broadcast_circuit_replication(clients);
            let (id, state) = server.world.get_block(action.target);
            server.fanout_block_update(
                clients,
                BlockUpdate {
                    pos: action.target,
                    id,
                    state,
                },
                h,
                v,
            );
            ack(true, "")
        }
        PlayerActionKind::Pick => {
            let (id, _) = server.world.get_block(action.target);
            let Some(def) = server.registry.block(id) else {
                return ack(false, "unknown block");
            };
            if !def.placeable {
                return ack(false, "not placeable");
            }
            ack(true, "")
        }
    }
}
