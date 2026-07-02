use stagcrest_net::{
    BlockUpdate, CircuitPowerBatch, GameMessage, ServerMessage,
};
use stagcrest_protocol::{BlockPos, ChunkPos};

use crate::client_session::{ClientRegistry, ConnectedClient};

pub fn chunk_in_client_radius(
    chunk: ChunkPos,
    client: &ConnectedClient,
    h_radius: i32,
    v_radius: i32,
) -> bool {
    if !client.stream.valid {
        return false;
    }
    (chunk.x - client.stream.center_x).abs() <= h_radius
        && (chunk.z - client.stream.center_z).abs() <= h_radius
        && (chunk.y - client.stream.center_y).abs() <= v_radius
}

pub fn client_interested_in_block(
    client: &ConnectedClient,
    pos: BlockPos,
    h_radius: i32,
    v_radius: i32,
) -> bool {
    client.handshake_complete
        && chunk_in_client_radius(pos.chunk_pos(), client, h_radius, v_radius)
}

impl ClientRegistry {
    pub fn fanout_block_update(
        &mut self,
        update: BlockUpdate,
        h_radius: i32,
        v_radius: i32,
    ) {
        let pos = update.pos;
        let msg = GameMessage::Server(ServerMessage::BlockUpdate(update));
        for client in self.clients_mut() {
            if client_interested_in_block(client, pos, h_radius, v_radius) {
                client.queue_priority(msg.clone());
            }
        }
    }

    /// Interest-filtered circuit power replication.
    ///
    /// Redstone wires spanning beyond a client's interest radius won't replicate until
    /// they stream the connecting chunks.
    pub fn fanout_circuit_batch(
        &mut self,
        batch: CircuitPowerBatch,
        h_radius: i32,
        v_radius: i32,
    ) {
        for client in self.clients_mut() {
            let updates: Vec<_> = batch
                .updates
                .iter()
                .filter(|(pos, _)| client_interested_in_block(client, *pos, h_radius, v_radius))
                .copied()
                .collect();
            if updates.is_empty() {
                continue;
            }
            client.queue_priority(GameMessage::Server(ServerMessage::CircuitPowerBatch(
                CircuitPowerBatch { updates },
            )));
        }
    }
}
