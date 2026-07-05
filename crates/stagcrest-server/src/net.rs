use stagcrest_minimap::{expected_subscribe_tiles, MINIMAP_ZOOM_LEVELS};
use stagcrest_mod_server::split_command;
use stagcrest_net::{
    validate_username, ChatKind, ChatLine, ClientHello, ClientMessage, GameMessage, InitialState,
    MapViewSubscribe, PlayerPose, ServerHello, ServerMessage, PROTOCOL_VERSION,
};
use stagcrest_protocol::BlockPos;

use crate::client_session::{ClientId, ClientRegistry, ConnectedClient};
use crate::commands::CommandHostImpl;
use crate::GameServer;

pub fn handle_client_hello(
    clients: &ClientRegistry,
    client_id: ClientId,
    hello: ClientHello,
) -> Result<(), String> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "protocol mismatch: client {} server {}",
            hello.protocol_version, PROTOCOL_VERSION
        ));
    }
    validate_username(&hello.username)?;
    if clients.clients().iter().any(|c| {
        c.id != client_id && c.username.as_deref() == Some(hello.username.as_str())
    }) {
        return Err("username already in use".into());
    }
    Ok(())
}

pub fn send_handshake(server: &GameServer, client: &mut ConnectedClient) {
    client.sent_chunks.clear();
    client.map.reset();
    let world_name = server.config.world_name.clone();
    let world_seed = server.config.world_seed;
    client.queue_priority(GameMessage::Server(ServerMessage::Hello(ServerHello {
        protocol_version: PROTOCOL_VERSION,
        server_id: server.server_id,
        world_name,
        world_seed,
    })));

    let spawn = server.spawn_position();
    client.queue_priority(GameMessage::Server(ServerMessage::Initial(InitialState {
        spawn_x: spawn.x as f32 + 0.5,
        spawn_y: spawn.y as f32 + 1.6,
        spawn_z: spawn.z as f32 + 0.5,
        render_distance: server.config.render_distance,
        world_time: server.session.meta.world_time,
    })));

    let manifest = server.cached_manifest.clone();
    client.queue_priority(GameMessage::Server(ServerMessage::Manifest(manifest)));
    for chunk in &server.cached_texture_chunks {
        client.queue_priority(GameMessage::Server(ServerMessage::TextureAssets(
            chunk.clone(),
        )));
    }
    client.handshake_pending = true;
}

pub fn handle_pose(client: &mut ConnectedClient, pose: PlayerPose) {
    client.pose = Some(pose);
    let block = BlockPos::new(
        pose.x.floor() as i32,
        pose.y.floor() as i32,
        pose.z.floor() as i32,
    );
    let center = block.chunk_pos();
    client.stream.center_x = center.x;
    client.stream.center_y = center.y;
    client.stream.center_z = center.z;
    client.stream.valid = true;
}

pub fn handle_client_message(
    server: &mut GameServer,
    clients: &mut ClientRegistry,
    client_id: ClientId,
    msg: ClientMessage,
) {
    match msg {
        ClientMessage::Hello(hello) => {
            let username = hello.username.clone();
            let result = handle_client_hello(clients, client_id, hello);
            let Some(client) = clients.get_mut(client_id) else {
                return;
            };
            match result {
                Ok(()) => {
                    client.username = Some(username);
                    client.join_announced = false;
                    send_handshake(server, client);
                }
                Err(reason) => client.queue_priority(GameMessage::Server(ServerMessage::Reject(
                    stagcrest_net::HelloReject { reason },
                ))),
            }
        }
        ClientMessage::Pose(pose) => {
            if let Some(client) = clients.get_mut(client_id) {
                handle_pose(client, pose);
            }
        }
        ClientMessage::Action(action) => {
            let ack = crate::player::apply_player_action(server, clients, client_id, action);
            if let Some(client) = clients.get_mut(client_id) {
                client.queue_priority(GameMessage::Server(ServerMessage::PlayerAck(ack)));
            }
        }
        ClientMessage::ChunkUnsubscribe(pos) => {
            if let Some(client) = clients.get_mut(client_id) {
                client.sent_chunks.remove(&pos);
            }
        }
        ClientMessage::MapViewSubscribe(sub) => {
            if let Some(client) = clients.get_mut(client_id) {
                handle_map_view_subscribe(client, sub);
            }
        }
        ClientMessage::Ping { nonce } => {
            if let Some(client) = clients.get_mut(client_id) {
                client.queue_priority(GameMessage::Server(ServerMessage::Pong { nonce }));
            }
        }
        ClientMessage::Chat { text } => {
            let Some(client) = clients.get(client_id) else {
                return;
            };
            if !client.handshake_complete {
                return;
            }
            let Some(sender) = client.username.clone() else {
                return;
            };
            let text = text.trim().to_string();
            if text.is_empty() || text.len() > 256 {
                return;
            }

            // Slash commands are dispatched to mod callbacks instead of being
            // broadcast as player chat. A leading `/` (optionally followed by
            // a command name and args) routes to the mod-defined handler.
            if let Some(body) = text.strip_prefix('/') {
                if let Some((name, args)) = split_command(body) {
                    let mut host = CommandHostImpl {
                        session: &mut server.session,
                        clients,
                    };
                    match server.mod_host.invoke_command(&mut host, client_id.0, name, args) {
                        Ok(_) => {}
                        Err(reason) => {
                            host.clients.send_chat_to(
                                client_id,
                                ChatLine {
                                    kind: ChatKind::System,
                                    text: format!("Command failed: {reason}"),
                                },
                            );
                        }
                    }
                }
                // An empty `/` or whitespace-only body is silently ignored.
                return;
            }

            clients.broadcast_chat(ChatLine {
                kind: ChatKind::Player { sender },
                text,
            });
        }
    }
}

fn validate_map_view_subscribe(sub: &MapViewSubscribe) -> bool {
    if !sub.active {
        return sub.tiles.is_empty();
    }
    if !MINIMAP_ZOOM_LEVELS.contains(&sub.bpp) {
        return false;
    }
    let expected = expected_subscribe_tiles(sub.center_x, sub.center_z, sub.bpp);
    sub.tiles.len() == expected.len() && sub.tiles.iter().all(|tile| expected.contains(tile))
}

fn handle_map_view_subscribe(client: &mut ConnectedClient, sub: MapViewSubscribe) {
    if !validate_map_view_subscribe(&sub) {
        tracing::warn!(
            "rejecting invalid MapViewSubscribe: active={} bpp={} tiles={}",
            sub.active,
            sub.bpp,
            sub.tiles.len()
        );
        return;
    }
    client.map.handle_subscribe(sub);
}

impl GameServer {
    pub(crate) fn spawn_position(&self) -> BlockPos {
        use stagcrest_mod_server::SEA_LEVEL;
        BlockPos::new(8, SEA_LEVEL + 16, 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_session::ClientRegistry;
    use stagcrest_net::{ClientHello, PROTOCOL_VERSION};

    // Compile-time guard: GameServer must stay `Send` because `run_standalone`
    // holds it across `tokio::select!` awaits on a multi-thread runtime. The
    // wasmtime `Store<HostState>` inside `ModHost` is `Send` when host state is
    // `Send`; this catches regressions.
    #[test]
    fn game_server_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<GameServer>();
    }

    #[test]
    fn hello_rejects_invalid_username() {
        let mut registry = ClientRegistry::new(16);
        let id = registry.register_inprocess();
        let err = handle_client_hello(
            &registry,
            id,
            ClientHello {
                protocol_version: PROTOCOL_VERSION,
                username: "ab".into(),
            },
        )
        .unwrap_err();
        assert!(err.contains("3–16"));
    }

    #[test]
    fn hello_rejects_duplicate_username() {
        let mut registry = ClientRegistry::new(16);
        let first = registry.register_inprocess();
        registry.get_mut(first).unwrap().username = Some("Steve".into());
        let second = registry.register_inprocess();
        let err = handle_client_hello(
            &registry,
            second,
            ClientHello {
                protocol_version: PROTOCOL_VERSION,
                username: "Steve".into(),
            },
        )
        .unwrap_err();
        assert!(err.contains("already in use"));
    }
}
