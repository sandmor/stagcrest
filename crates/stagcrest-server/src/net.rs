use stagcrest_minimap::{expected_subscribe_tiles, MINIMAP_ZOOM_LEVELS};
use stagcrest_net::{
    ClientHello, ClientMessage, GameMessage, InitialState, MapViewSubscribe, PlayerPose, ServerHello,
    ServerMessage, PROTOCOL_VERSION,
};
use stagcrest_protocol::BlockPos;

use crate::client_session::{ClientId, ClientRegistry, ConnectedClient};
use crate::GameServer;

pub fn handle_client_hello(_server: &GameServer, hello: ClientHello) -> Result<(), String> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "protocol mismatch: client {} server {}",
            hello.protocol_version, PROTOCOL_VERSION
        ));
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
    })));

    let manifest = server.cached_manifest.clone();
    client.queue_priority(GameMessage::Server(ServerMessage::Manifest(manifest)));
    let atlas = server.cached_atlas.clone();
    client.queue_priority(GameMessage::Server(ServerMessage::AtlasTransfer(atlas)));
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
            let result = handle_client_hello(server, hello);
            let Some(client) = clients.get_mut(client_id) else {
                return;
            };
            match result {
                Ok(()) => send_handshake(server, client),
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
    sub.tiles.len() == expected.len()
        && sub.tiles.iter().all(|tile| expected.contains(tile))
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
