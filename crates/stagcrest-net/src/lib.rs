pub mod config;
pub mod frame;
pub mod message;
pub mod transport;

pub use config::NetConfig;
pub use message::{
    BlockUpdate, ChunkSnapshot, ClientHello, ClientMessage, GameMessage, HelloReject, InitialState,
    PlayerAction, PlayerActionKind, PlayerAck, PlayerPose, ServerHello, ServerMessage,
};
pub use transport::{
    send_message, spawn_tcp_session, AsyncTcpSession, GameTransport, InProcessTransport,
    TcpTransport, TransportError,
};

pub const PROTOCOL_VERSION: u32 = 1;
