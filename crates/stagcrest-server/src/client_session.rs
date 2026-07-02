use std::collections::HashSet;

use stagcrest_net::{AsyncTcpSession, GameMessage, PlayerPose};
use stagcrest_protocol::ChunkPos;

use crate::map_streaming::ClientMapState;
use crate::streaming::TerrainStreamState;

const MAX_PENDING_BULK_PER_CLIENT: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub u64);

pub struct ConnectedClient {
    pub id: ClientId,
    pub pose: Option<PlayerPose>,
    pub stream: TerrainStreamState,
    pub last_center: Option<ChunkPos>,
    pub sent_chunks: HashSet<ChunkPos>,
    pub map: ClientMapState,
    pub handshake_complete: bool,
    pub handshake_pending: bool,
    pending_priority: Vec<GameMessage>,
    pending_bulk: Vec<GameMessage>,
    pub tcp: Option<AsyncTcpSession>,
}

impl ConnectedClient {
    pub fn new(id: ClientId) -> Self {
        Self {
            id,
            pose: None,
            stream: TerrainStreamState::default(),
            last_center: None,
            sent_chunks: HashSet::new(),
            map: ClientMapState::default(),
            handshake_complete: false,
            handshake_pending: false,
            pending_priority: Vec::new(),
            pending_bulk: Vec::new(),
            tcp: None,
        }
    }

    pub fn with_tcp(id: ClientId, session: AsyncTcpSession) -> Self {
        let mut client = Self::new(id);
        client.tcp = Some(session);
        client
    }

    pub fn queue_priority(&mut self, msg: GameMessage) {
        self.pending_priority.push(msg);
    }

    pub fn queue_bulk(&mut self, msg: GameMessage) {
        if self.pending_bulk.len() >= MAX_PENDING_BULK_PER_CLIENT {
            self.pending_bulk.remove(0);
        }
        self.pending_bulk.push(msg);
    }

    pub fn take_priority(&mut self) -> Vec<GameMessage> {
        std::mem::take(&mut self.pending_priority)
    }

    pub fn take_bulk(&mut self) -> Vec<GameMessage> {
        std::mem::take(&mut self.pending_bulk)
    }

    pub fn finish_handshake_if_wire_ready(&mut self, wire_ready: bool) {
        if self.handshake_pending && wire_ready {
            self.handshake_pending = false;
            self.handshake_complete = true;
        }
    }

    pub fn reset_streaming_state(&mut self) {
        self.sent_chunks.clear();
        self.map.reset();
        self.handshake_complete = false;
        self.handshake_pending = false;
    }
}

pub struct ClientRegistry {
    next_id: u64,
    clients: Vec<ConnectedClient>,
    pub max_clients: usize,
    enqueue_rotate: usize,
    fair_rotate: usize,
}

impl ClientRegistry {
    pub fn new(max_clients: usize) -> Self {
        Self {
            next_id: 1,
            clients: Vec::new(),
            max_clients,
            enqueue_rotate: 0,
            fair_rotate: 0,
        }
    }

    pub fn has_capacity(&self) -> bool {
        self.clients.len() < self.max_clients
    }

    pub fn len(&self) -> usize {
        self.clients.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    pub fn register_tcp(&mut self, session: AsyncTcpSession) -> ClientId {
        let id = ClientId(self.next_id);
        self.next_id += 1;
        self.clients.push(ConnectedClient::with_tcp(id, session));
        id
    }

    pub fn register_inprocess(&mut self) -> ClientId {
        let id = ClientId(self.next_id);
        self.next_id += 1;
        self.clients.push(ConnectedClient::new(id));
        id
    }

    pub fn remove(&mut self, id: ClientId) -> Option<ConnectedClient> {
        let idx = self.clients.iter().position(|c| c.id == id)?;
        Some(self.clients.remove(idx))
    }

    pub fn get(&self, id: ClientId) -> Option<&ConnectedClient> {
        self.clients.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: ClientId) -> Option<&mut ConnectedClient> {
        self.clients.iter_mut().find(|c| c.id == id)
    }

    pub fn clients_mut(&mut self) -> &mut [ConnectedClient] {
        &mut self.clients
    }

    pub fn clients(&self) -> &[ConnectedClient] {
        &self.clients
    }

    pub fn client_ids(&self) -> Vec<ClientId> {
        self.clients.iter().map(|c| c.id).collect()
    }

    pub fn streaming_clients(&self) -> Vec<ClientId> {
        self.clients
            .iter()
            .filter(|c| c.handshake_complete && c.stream.valid)
            .map(|c| c.id)
            .collect()
    }

    pub fn any_handshake_complete(&self) -> bool {
        self.clients.iter().any(|c| c.handshake_complete)
    }

    pub fn next_enqueue_client_index(&mut self, count: usize) -> usize {
        if count == 0 {
            return 0;
        }
        let idx = self.enqueue_rotate % count;
        self.enqueue_rotate = (self.enqueue_rotate + 1) % count;
        idx
    }

    pub fn next_fair_client_index(&mut self, count: usize) -> usize {
        if count == 0 {
            return 0;
        }
        let idx = self.fair_rotate % count;
        self.fair_rotate = (self.fair_rotate + 1) % count;
        idx
    }
}
