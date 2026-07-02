use stagcrest_minimap::{expected_subscribe_tiles, MINIMAP_ZOOM_LEVELS};
use stagcrest_net::{
    ClientHello, ClientMessage, GameMessage, InitialState, MapViewSubscribe, PlayerPose, ServerHello,
    ServerMessage, PROTOCOL_VERSION,
};
use stagcrest_protocol::BlockPos;

use crate::GameServer;

pub fn handle_client_hello(server: &mut GameServer, hello: ClientHello) -> Result<(), String> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "protocol mismatch: client {} server {}",
            hello.protocol_version, PROTOCOL_VERSION
        ));
    }
    server.client_id = Some(hello.client_id);
    Ok(())
}

pub fn send_handshake(server: &mut GameServer) {
    server.pipeline.reset_client_delivery();
    server.client_map.reset();
    let world_name = server.config.world_name.clone();
    let world_seed = server.config.world_seed;
    server.queue_priority(GameMessage::Server(ServerMessage::Hello(ServerHello {
        protocol_version: PROTOCOL_VERSION,
        server_id: server.server_id,
        world_name,
        world_seed,
    })));

    let spawn = server.spawn_position();
    server.queue_priority(GameMessage::Server(ServerMessage::Initial(InitialState {
        spawn_x: spawn.x as f32 + 0.5,
        spawn_y: spawn.y as f32 + 1.6,
        spawn_z: spawn.z as f32 + 0.5,
        render_distance: server.config.render_distance,
    })));

    let manifest = server.cached_manifest.clone();
    server.queue_priority(GameMessage::Server(ServerMessage::Manifest(manifest)));
    let atlas = server.cached_atlas.clone();
    server.queue_priority(GameMessage::Server(ServerMessage::AtlasTransfer(atlas)));
    server.handshake_pending = true;
}

pub fn handle_pose(server: &mut GameServer, pose: PlayerPose) {
    server.latest_pose = Some(pose);
    let block = BlockPos::new(
        pose.x.floor() as i32,
        pose.y.floor() as i32,
        pose.z.floor() as i32,
    );
    let center = block.chunk_pos();
    server.stream_state.center_x = center.x;
    server.stream_state.center_y = center.y;
    server.stream_state.center_z = center.z;
    server.stream_state.valid = true;
}

pub fn handle_client_message(server: &mut GameServer, msg: ClientMessage) {
    match msg {
        ClientMessage::Hello(hello) => match handle_client_hello(server, hello) {
            Ok(()) => send_handshake(server),
            Err(reason) => server.queue_priority(GameMessage::Server(ServerMessage::Reject(
                stagcrest_net::HelloReject { reason },
            ))),
        },
        ClientMessage::Pose(pose) => handle_pose(server, pose),
        ClientMessage::Action(action) => {
            let ack = crate::player::apply_player_action(server, action);
            server.queue_priority(GameMessage::Server(ServerMessage::PlayerAck(ack)));
        }
        ClientMessage::ChunkUnsubscribe(pos) => {
            server.pipeline.sent_to_client.remove(&pos);
        }
        ClientMessage::MapViewSubscribe(sub) => handle_map_view_subscribe(server, sub),
        ClientMessage::Ping { nonce } => {
            server.queue_priority(GameMessage::Server(ServerMessage::Pong { nonce }));
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

fn handle_map_view_subscribe(server: &mut GameServer, sub: MapViewSubscribe) {
    if !validate_map_view_subscribe(&sub) {
        tracing::warn!(
            "rejecting invalid MapViewSubscribe: active={} bpp={} tiles={}",
            sub.active,
            sub.bpp,
            sub.tiles.len()
        );
        return;
    }
    server.client_map.handle_subscribe(sub);
}

impl GameServer {
    pub(crate) fn spawn_position(&self) -> BlockPos {
        use stagcrest_mod_server::SEA_LEVEL;
        BlockPos::new(8, SEA_LEVEL + 16, 8)
    }
}
