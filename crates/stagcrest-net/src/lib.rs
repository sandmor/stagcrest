pub mod config;
pub mod frame;
pub mod message;
pub mod transport;

pub use config::NetConfig;
pub use message::{
    BlockUpdate, ChatKind, ChatLine, ChunkSnapshot, CircuitPowerBatch, ClientHello, ClientMessage,
    GameMessage, HelloReject, InitialState, MapChunkSnapshot, MapViewSubscribe, PlayerAck,
    PlayerAction, PlayerActionKind, PlayerPose, ServerHello, ServerMessage,
};
pub use transport::{
    send_message, spawn_tcp_session, AsyncTcpSession, GameTransport, InProcessTransport,
    TcpTransport, TransportError,
};

pub const PROTOCOL_VERSION: u32 = 1;

pub use stagcrest_protocol::validate_username;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_postcard_round_trip() {
        let msg = GameMessage::Client(ClientMessage::Chat {
            text: "hello".into(),
        });
        let bytes = postcard::to_allocvec(&msg).unwrap();
        let decoded: GameMessage = postcard::from_bytes(&bytes).unwrap();
        match decoded {
            GameMessage::Client(ClientMessage::Chat { text }) => assert_eq!(text, "hello"),
            _ => panic!("expected chat client message"),
        }

        let line = GameMessage::Server(ServerMessage::Chat(ChatLine {
            kind: ChatKind::Player {
                sender: "Steve".into(),
            },
            text: "hello".into(),
        }));
        let bytes = postcard::to_allocvec(&line).unwrap();
        let decoded: GameMessage = postcard::from_bytes(&bytes).unwrap();
        match decoded {
            GameMessage::Server(ServerMessage::Chat(chat)) => {
                assert_eq!(chat.text, "hello");
                assert_eq!(
                    chat.kind,
                    ChatKind::Player {
                        sender: "Steve".into()
                    }
                );
            }
            _ => panic!("expected chat server message"),
        }
    }

    #[test]
    fn client_hello_includes_username() {
        let hello = ClientHello {
            protocol_version: PROTOCOL_VERSION,
            username: "Player".into(),
        };
        let bytes = postcard::to_allocvec(&hello).unwrap();
        let decoded: ClientHello = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.username, "Player");
    }
}
