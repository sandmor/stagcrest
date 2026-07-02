use glam::Vec3;
use serde::{Deserialize, Serialize};
use stagcrest_protocol::{
    manifest::{AtlasTransfer, ContentManifest},
    BlockId, BlockPos, BlockState, ChunkPos,
};

/// Handshake: client → server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientHello {
    pub protocol_version: u32,
    pub client_id: u64,
}

/// Handshake: server → client (success).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHello {
    pub protocol_version: u32,
    pub server_id: u64,
    pub world_name: String,
    pub world_seed: u64,
}

/// Handshake: server → client (failure).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloReject {
    pub reason: String,
}

/// Spawn and streaming hints after manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialState {
    pub spawn_x: f32,
    pub spawn_y: f32,
    pub spawn_z: f32,
    pub render_distance: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerPose {
    pub seq: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl PlayerPose {
    pub fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlayerActionKind {
    Break,
    Place,
    Toggle,
    Pick,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerAction {
    pub action_seq: u32,
    pub kind: PlayerActionKind,
    pub target: BlockPos,
    pub hotbar_slot: u8,
    pub block_id: BlockId,
    /// Face normal pointing into `target` for Place actions (from the clicked block face).
    pub face_normal: [i32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerAck {
    pub action_seq: u32,
    pub ok: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSnapshot {
    pub pos: ChunkPos,
    /// LZ4-compressed inactive chunk wire bytes.
    pub compressed: Vec<u8>,
    /// 4×4×4 biome index grid (64 bytes).
    #[serde(default)]
    pub biome_grid: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockUpdate {
    pub pos: BlockPos,
    pub id: BlockId,
    pub state: BlockState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitPowerBatch {
    pub updates: Vec<(BlockPos, u8)>,
}

/// Client reports which map tiles overlap the minimap HUD (sent on view change only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapViewSubscribe {
    pub active: bool,
    pub center_x: i32,
    pub center_z: i32,
    pub bpp: u32,
    pub tiles: Vec<(i32, i32)>,
}

/// Server pushes a stored map tile (`MapChunkBlob` bytes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapChunkSnapshot {
    pub mx: i32,
    pub mz: i32,
    pub compressed: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello(ClientHello),
    Pose(PlayerPose),
    Action(PlayerAction),
    ChunkUnsubscribe(ChunkPos),
    MapViewSubscribe(MapViewSubscribe),
    Ping { nonce: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    Hello(ServerHello),
    Reject(HelloReject),
    Manifest(ContentManifest),
    AtlasTransfer(AtlasTransfer),
    Initial(InitialState),
    ChunkSnapshot(ChunkSnapshot),
    ChunkUnload(ChunkPos),
    BlockUpdate(BlockUpdate),
    CircuitPowerBatch(CircuitPowerBatch),
    MapChunkSnapshot(MapChunkSnapshot),
    PlayerAck(PlayerAck),
    Pong { nonce: u32 },
}

/// Unified game message for transports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameMessage {
    Client(ClientMessage),
    Server(ServerMessage),
}

impl GameMessage {
    pub fn is_bulk(&self) -> bool {
        matches!(
            self,
            GameMessage::Server(ServerMessage::ChunkSnapshot(_))
                | GameMessage::Server(ServerMessage::MapChunkSnapshot(_))
        )
    }
}
