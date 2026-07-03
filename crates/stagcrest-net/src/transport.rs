use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use socket2::SockRef;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc as async_mpsc;

use crate::config::NetConfig;
use crate::frame::{decode_payload, encode_payload, read_frame_header, wrap_frame, FrameError};
use crate::message::GameMessage;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("frame error: {0}")]
    Frame(#[from] FrameError),
    #[error("io error: {0}")]
    Io(String),
    #[error("channel closed")]
    Closed,
}

pub trait GameTransport: Send + Sync {
    fn try_recv(&mut self) -> Result<Option<GameMessage>, TransportError>;
    fn send(&mut self, msg: GameMessage) -> Result<(), TransportError>;
    /// Block until the next tick or until a peer sends (in-process wake).
    fn idle_wait(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

fn apply_tcp_tuning(stream: &TcpStream, config: &NetConfig) -> Result<(), TransportError> {
    stream
        .set_nodelay(config.tcp_nodelay)
        .map_err(|e| TransportError::Io(e.to_string()))?;
    let sock = SockRef::from(stream);
    sock.set_send_buffer_size(config.send_buffer_bytes)
        .map_err(|e| TransportError::Io(e.to_string()))?;
    sock.set_recv_buffer_size(config.recv_buffer_bytes)
        .map_err(|e| TransportError::Io(e.to_string()))?;
    Ok(())
}

/// In-process transport using sync channels (same encode path as TCP).
pub struct InProcessTransport {
    rx: Mutex<Receiver<GameMessage>>,
    tx: Sender<GameMessage>,
    idle: Arc<(Mutex<()>, Condvar)>,
}

impl InProcessTransport {
    pub fn pair() -> (Self, Self) {
        let (tx_a, rx_a) = mpsc::channel();
        let (tx_b, rx_b) = mpsc::channel();
        let idle = Arc::new((Mutex::new(()), Condvar::new()));
        (
            Self {
                rx: Mutex::new(rx_a),
                tx: tx_b,
                idle: idle.clone(),
            },
            Self {
                rx: Mutex::new(rx_b),
                tx: tx_a,
                idle,
            },
        )
    }
}

impl GameTransport for InProcessTransport {
    fn try_recv(&mut self) -> Result<Option<GameMessage>, TransportError> {
        let rx = self.rx.lock().map_err(|_| TransportError::Closed)?;
        match rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(TransportError::Closed),
        }
    }

    fn send(&mut self, msg: GameMessage) -> Result<(), TransportError> {
        self.tx.send(msg).map_err(|_| TransportError::Closed)?;
        self.idle.1.notify_one();
        Ok(())
    }

    fn idle_wait(&mut self, duration: Duration) {
        let guard = self.idle.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self.idle.1.wait_timeout(guard, duration);
    }
}

/// Blocking TCP transport for use from Bevy poll loop via try_recv.
pub struct TcpTransport {
    stream: std::net::TcpStream,
    read_buf: Vec<u8>,
    pending_payload: Option<Vec<u8>>,
    config: NetConfig,
}

fn apply_tcp_tuning_std(
    stream: &std::net::TcpStream,
    config: &NetConfig,
) -> Result<(), TransportError> {
    stream
        .set_nodelay(config.tcp_nodelay)
        .map_err(|e| TransportError::Io(e.to_string()))?;
    let sock = SockRef::from(stream);
    sock.set_send_buffer_size(config.send_buffer_bytes)
        .map_err(|e| TransportError::Io(e.to_string()))?;
    sock.set_recv_buffer_size(config.recv_buffer_bytes)
        .map_err(|e| TransportError::Io(e.to_string()))?;
    Ok(())
}

impl TcpTransport {
    pub fn connect_blocking(addr: &str, config: NetConfig) -> Result<Self, TransportError> {
        let stream =
            std::net::TcpStream::connect(addr).map_err(|e| TransportError::Io(e.to_string()))?;
        apply_tcp_tuning_std(&stream, &config)?;
        stream
            .set_nonblocking(true)
            .map_err(|e| TransportError::Io(e.to_string()))?;
        Ok(Self {
            stream,
            read_buf: Vec::new(),
            pending_payload: None,
            config,
        })
    }

    pub async fn connect(addr: &str, config: NetConfig) -> Result<Self, TransportError> {
        Self::connect_blocking(addr, config)
    }

    fn read_more(&mut self) -> Result<bool, TransportError> {
        let mut tmp = [0u8; 65536];
        match self.stream.read(&mut tmp) {
            Ok(0) => Err(TransportError::Closed),
            Ok(n) => {
                self.read_buf.extend_from_slice(&tmp[..n]);
                Ok(true)
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(false),
            Err(e) => Err(TransportError::Io(e.to_string())),
        }
    }

    fn try_decode_frame(&mut self) -> Result<Option<GameMessage>, TransportError> {
        loop {
            if self.pending_payload.is_none() {
                if self.read_buf.len() < 4 {
                    return Ok(None);
                }
                let mut hdr = [0u8; 4];
                hdr.copy_from_slice(&self.read_buf[..4]);
                let len = read_frame_header(&hdr);
                if len > crate::frame::MAX_FRAME_BYTES {
                    return Err(FrameError::TooLarge.into());
                }
                if self.read_buf.len() < 4 + len {
                    return Ok(None);
                }
                let payload = self.read_buf[4..4 + len].to_vec();
                match decode_payload::<GameMessage>(&payload) {
                    Ok(msg) => {
                        self.read_buf.drain(..4 + len);
                        return Ok(Some(msg));
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            if let Some(payload) = self.pending_payload.take() {
                let msg: GameMessage = decode_payload(&payload)?;
                return Ok(Some(msg));
            }
        }
    }
}

impl GameTransport for TcpTransport {
    fn try_recv(&mut self) -> Result<Option<GameMessage>, TransportError> {
        if let Some(msg) = self.try_decode_frame()? {
            return Ok(Some(msg));
        }
        while self.read_more()? {
            if let Some(msg) = self.try_decode_frame()? {
                return Ok(Some(msg));
            }
        }
        Ok(None)
    }

    fn send(&mut self, msg: GameMessage) -> Result<(), TransportError> {
        if self.config.sim_latency_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.config.sim_latency_ms));
        }
        let payload = encode_payload(&msg)?;
        let framed = wrap_frame(&payload)?;
        let mut offset = 0;
        while offset < framed.len() {
            match self.stream.write(&framed[offset..]) {
                Ok(0) => return Err(TransportError::Closed),
                Ok(n) => offset += n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(TransportError::Io(e.to_string())),
            }
        }
        Ok(())
    }
}

/// Async TCP writer/reader pair for server tasks.
pub struct AsyncTcpSession {
    pub incoming: async_mpsc::Receiver<GameMessage>,
    pub outgoing_priority: async_mpsc::Sender<GameMessage>,
    pub outgoing_bulk: async_mpsc::Sender<GameMessage>,
    /// Set by the writer after the Manifest frame is fully flushed to TCP.
    pub handshake_wire_ready: Arc<AtomicBool>,
}

pub async fn spawn_tcp_session(
    stream: TcpStream,
    config: NetConfig,
) -> Result<AsyncTcpSession, TransportError> {
    apply_tcp_tuning(&stream, &config)?;
    let (incoming_tx, incoming_rx) = async_mpsc::channel(config.max_priority_queue);
    let (priority_tx, priority_rx) = async_mpsc::channel(config.max_priority_queue);
    let (bulk_tx, bulk_rx) = async_mpsc::channel(config.max_bulk_queue);
    let handshake_wire_ready = Arc::new(AtomicBool::new(false));

    let (mut read_half, mut write_half) = stream.into_split();

    // Reader task
    let reader_cfg = config.clone();
    tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut scratch = [0u8; 4096];
        loop {
            match read_half.read(&mut scratch).await {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&scratch[..n]),
                Err(_) => break,
            }
            while buf.len() >= 4 {
                let len = read_frame_header(buf[..4].try_into().unwrap()) as usize;
                if len > crate::frame::MAX_FRAME_BYTES {
                    break;
                }
                if buf.len() < 4 + len {
                    break;
                }
                let payload = buf[4..4 + len].to_vec();
                buf.drain(..4 + len);
                match decode_payload::<GameMessage>(&payload) {
                    Ok(msg) => {
                        if incoming_tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
        let _ = reader_cfg;
    });

    // Writer task
    let writer_cfg = config.clone();
    let writer_handshake_ready = handshake_wire_ready.clone();
    tokio::spawn(async move {
        use crate::message::ServerMessage;
        let mut priority_rx = priority_rx;
        let mut bulk_rx = bulk_rx;
        let mut write_buf: Vec<u8> = Vec::new();
        'writer: loop {
            let mut wrote = false;
            while let Ok(msg) = priority_rx.try_recv() {
                let wire_done = matches!(
                    msg,
                    GameMessage::Server(ServerMessage::TextureAssets(ref chunk))
                        if chunk.index + 1 >= chunk.total
                );
                match encode_payload(&msg) {
                    Ok(payload) => match wrap_frame(&payload) {
                        Ok(framed) => {
                            write_buf.extend_from_slice(&framed);
                            if !write_buf.is_empty() {
                                if writer_cfg.sim_latency_ms > 0 {
                                    tokio::time::sleep(Duration::from_millis(
                                        writer_cfg.sim_latency_ms,
                                    ))
                                    .await;
                                }
                                if write_half.write_all(&write_buf).await.is_err() {
                                    break 'writer;
                                }
                                if write_half.flush().await.is_err() {
                                    break 'writer;
                                }
                                write_buf.clear();
                                if wire_done {
                                    writer_handshake_ready.store(true, Ordering::Release);
                                }
                            }
                            wrote = true;
                        }
                        Err(_) => {}
                    },
                    Err(_) => {}
                }
            }
            if writer_handshake_ready.load(Ordering::Acquire)
                && (write_buf.len() >= 8192 || (!wrote && write_buf.is_empty()))
            {
                if let Ok(msg) = bulk_rx.try_recv() {
                    match encode_payload(&msg) {
                        Ok(payload) => {
                            if let Ok(framed) = wrap_frame(&payload) {
                                write_buf.extend_from_slice(&framed);
                                wrote = true;
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
            if !write_buf.is_empty() {
                if writer_cfg.sim_latency_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(writer_cfg.sim_latency_ms)).await;
                }
                if write_half.write_all(&write_buf).await.is_err() {
                    break;
                }
                write_buf.clear();
            }
            if !wrote {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    });

    Ok(AsyncTcpSession {
        incoming: incoming_rx,
        outgoing_priority: priority_tx,
        outgoing_bulk: bulk_tx,
        handshake_wire_ready,
    })
}

pub async fn send_message(
    session: &AsyncTcpSession,
    msg: GameMessage,
) -> Result<(), TransportError> {
    let tx = if msg.is_bulk() {
        &session.outgoing_bulk
    } else {
        &session.outgoing_priority
    };
    tx.send(msg).await.map_err(|_| TransportError::Closed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ChunkSnapshot, ClientMessage, ServerMessage};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn in_process_round_trip() {
        let (mut a, mut b) = InProcessTransport::pair();
        let msg = GameMessage::Client(ClientMessage::Ping { nonce: 7 });
        a.send(msg).unwrap();
        let received = b.try_recv().unwrap().unwrap();
        assert!(matches!(
            received,
            GameMessage::Client(ClientMessage::Ping { nonce: 7 })
        ));
    }

    #[test]
    fn tcp_connect_sets_nodelay() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_nonblocking(true).ok();
            let mut buf = [0u8; 64];
            while stream.read(&mut buf).is_ok() {}
        });
        let transport =
            TcpTransport::connect_blocking(&addr.to_string(), NetConfig::default()).unwrap();
        assert!(transport.stream.nodelay().unwrap());
        server.join().ok();
    }

    #[test]
    fn bulk_messages_classified() {
        let bulk = GameMessage::Server(ServerMessage::ChunkSnapshot(ChunkSnapshot {
            pos: stagcrest_protocol::ChunkPos { x: 0, y: 0, z: 0 },
            compressed: vec![],
            biome_grid: None,
        }));
        assert!(bulk.is_bulk());

        let priority = GameMessage::Server(ServerMessage::Pong { nonce: 1 });
        assert!(!priority.is_bulk());
    }
}
