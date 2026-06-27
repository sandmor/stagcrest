use std::time::{Duration, Instant};

use bevy::prelude::*;
use stagcrest_net::{
    ClientHello, ClientMessage, GameMessage, GameTransport, NetConfig, ServerMessage,
    PROTOCOL_VERSION,
};
use stagcrest_protocol::manifest::ContentManifest;
use stagcrest_server::{spawn_local, ServerConfig};

#[derive(Resource, Clone)]
pub struct LaunchConfig {
    pub connect: Option<String>,
    pub net_sim_latency_ms: u64,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            connect: None,
            net_sim_latency_ms: 0,
        }
    }
}

#[derive(Resource)]
pub struct GameNetClient {
    pub transport: Option<Box<dyn GameTransport>>,
    pub net_config: NetConfig,
    pub handshake_done: bool,
    pub manifest: Option<ContentManifest>,
    pub initial_received: bool,
    pub action_seq: u32,
    pub pose_seq: u32,
    pub connect_addr: Option<String>,
    pub embedded: bool,
    pub last_rtt_ms: Option<f32>,
    pub ping_nonce: u32,
    ping_sent_at: Option<Instant>,
    ping_timer: Duration,
    pub _server_handle: Option<std::thread::JoinHandle<()>>,
}

impl Default for GameNetClient {
    fn default() -> Self {
        Self::from_launch(&LaunchConfig::default())
    }
}

impl GameNetClient {
    pub fn from_launch(launch: &LaunchConfig) -> Self {
        let mut net_config = NetConfig::default();
        net_config.sim_latency_ms = launch.net_sim_latency_ms;
        Self {
            transport: None,
            net_config,
            handshake_done: false,
            manifest: None,
            initial_received: false,
            action_seq: 0,
            pose_seq: 0,
            connect_addr: launch.connect.clone(),
            embedded: launch.connect.is_none(),
            last_rtt_ms: None,
            ping_nonce: 0,
            ping_sent_at: None,
            ping_timer: Duration::ZERO,
            _server_handle: None,
        }
    }

    pub fn transport_label(&self) -> &'static str {
        if self.embedded {
            "in-process"
        } else {
            "tcp"
        }
    }

    pub fn maybe_send_ping(&mut self, dt: Duration) {
        if self.transport.is_none() {
            return;
        }
        self.ping_timer += dt;
        if self.ping_timer < Duration::from_secs(1) {
            return;
        }
        self.ping_timer = Duration::ZERO;
        self.ping_nonce = self.ping_nonce.wrapping_add(1);
        self.ping_sent_at = Some(Instant::now());
        if let Some(t) = self.transport.as_mut() {
            let _ = t.send(GameMessage::Client(ClientMessage::Ping {
                nonce: self.ping_nonce,
            }));
        }
    }

    pub fn handle_pong(&mut self, nonce: u32) {
        if self.ping_nonce != nonce {
            return;
        }
        if let Some(sent) = self.ping_sent_at.take() {
            self.last_rtt_ms = Some(sent.elapsed().as_secs_f32() * 1000.0);
        }
    }
    pub fn start_embedded(&mut self, config: ServerConfig) -> Result<(), String> {
        let (handle, client_transport) = spawn_local(config).map_err(|e| e.to_string())?;
        self.transport = Some(Box::new(client_transport));
        self.embedded = true;
        self._server_handle = Some(handle);
        self.send_hello();
        Ok(())
    }

    pub fn start_tcp(&mut self, transport: Box<dyn GameTransport>) {
        self.transport = Some(transport);
        self.embedded = false;
        self.send_hello();
    }

    pub fn send_hello(&mut self) {
        if let Some(t) = self.transport.as_mut() {
            let _ = t.send(GameMessage::Client(ClientMessage::Hello(ClientHello {
                protocol_version: PROTOCOL_VERSION,
                client_id: 1,
            })));
        }
    }

    pub fn poll(&mut self) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        let mut pongs = Vec::new();
        let Some(transport) = self.transport.as_mut() else {
            return out;
        };
        loop {
            match transport.try_recv() {
                Ok(Some(GameMessage::Server(server_msg))) => {
                    if let ServerMessage::Pong { nonce } = &server_msg {
                        pongs.push(*nonce);
                    }
                    out.push(server_msg);
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => break,
            }
        }
        for nonce in pongs {
            self.handle_pong(nonce);
        }
        out
    }

    pub fn send_pose(&mut self, x: f32, y: f32, z: f32, yaw: f32, pitch: f32) {
        self.pose_seq = self.pose_seq.wrapping_add(1);
        if let Some(t) = self.transport.as_mut() {
            let _ = t.send(GameMessage::Client(ClientMessage::Pose(
                stagcrest_net::PlayerPose {
                    seq: self.pose_seq,
                    x,
                    y,
                    z,
                    yaw,
                    pitch,
                },
            )));
        }
    }

    pub fn send_action(&mut self, action: stagcrest_net::PlayerAction) {
        if let Some(t) = self.transport.as_mut() {
            let _ = t.send(GameMessage::Client(ClientMessage::Action(action)));
        }
    }
}

pub async fn connect_tcp(addr: &str, config: NetConfig) -> Result<Box<dyn GameTransport>, String> {
    let transport = stagcrest_net::TcpTransport::connect(addr, config)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Box::new(transport))
}
