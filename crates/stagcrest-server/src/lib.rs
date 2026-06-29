mod net;
mod player;
mod session;
mod streaming;

use std::path::PathBuf;
use std::time::Duration;

use stagcrest_circuit::{init_circuit_blocks, CircuitWorld};
use stagcrest_mod_server::{
    load_mods, world_chunk_y_bounds, BiomeRegistry, BlockRegistry, ColormapSet, ColumnBlocks,
    ModHost, TerrainGenerator, WorldGenState, WorldSeed, SEA_LEVEL,
};
use stagcrest_net::{
    send_message, spawn_tcp_session, ClientMessage, GameMessage, GameTransport, InProcessTransport,
    NetConfig, ServerMessage,
};
use stagcrest_protocol::{manifest::AtlasTransfer, BlockId, BlockPos, ChunkPos};
use stagcrest_world::World;
use tokio::net::TcpListener;

pub use session::{streaming_lru_capacity, WorldSession};
pub use streaming::{StreamingPipeline, TerrainStreamState};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind: Option<String>,
    pub world_name: String,
    pub world_seed: u64,
    pub mods_root: PathBuf,
    pub render_distance: i32,
    pub vertical_render_distance: i32,
    pub net_sim_latency_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: Some("0.0.0.0:4242".to_string()),
            world_name: "default".to_string(),
            world_seed: 42,
            mods_root: PathBuf::from("."),
            render_distance: 8,
            vertical_render_distance: 4,
            net_sim_latency_ms: 0,
        }
    }
}

pub struct GameServer {
    pub config: ServerConfig,
    pub mod_host: ModHost,
    pub colormaps: ColormapSet,
    pub world: World,
    pub circuit: CircuitWorld,
    pub terrain: WorldGenState,
    pub column_blocks: ColumnBlocks,
    pub biomes: BiomeRegistry,
    pub session: WorldSession,
    pub generator: TerrainGenerator,
    pub pipeline: StreamingPipeline,
    pub stream_state: TerrainStreamState,
    pub last_center: Option<ChunkPos>,
    pub air: BlockId,
    pub registry: BlockRegistry,
    pub latest_pose: Option<stagcrest_net::PlayerPose>,
    pending_priority: Vec<GameMessage>,
    pending_bulk: Vec<GameMessage>,
    circuit_accumulator: f32,
    pub server_id: u64,
    pub client_id: Option<u64>,
    pub(crate) handshake_complete: bool,
    pub(crate) handshake_pending: bool,
    cached_manifest: stagcrest_protocol::manifest::ContentManifest,
    cached_atlas: AtlasTransfer,
}

impl GameServer {
    pub fn bootstrap(
        config: ServerConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut mod_host = load_mods(&config.mods_root)?;
        let reader = stagcrest_mod_server::FsAssetReader::new(&config.mods_root);
        let packs = stagcrest_mod_server::ResourcePackLoader::load(&config.mods_root, &reader).ok();
        let colormaps = ColormapSet::load(&reader, packs.as_ref());

        let registry = std::mem::take(&mut mod_host.registry);
        let biomes = mod_host.biome_registry.clone();
        let air = registry
            .block_by_name("stagcrest:air")
            .unwrap_or(BlockId(0));
        let column_blocks = ColumnBlocks::resolve(&registry, air);

        let mut session = WorldSession::open(&config.world_name)?;
        let lru_cap =
            streaming_lru_capacity(config.render_distance, config.vertical_render_distance);
        let world = World::with_lru_capacity(lru_cap, air);
        let mut terrain = WorldGenState::new(WorldSeed(config.world_seed));
        terrain.apply_river_config(biomes.river_config());
        let generator = terrain.generator().clone();

        let spawn = BlockPos::new(8, SEA_LEVEL + 16, 8);
        let spawn_chunk = spawn.chunk_pos();
        let y_bounds = world_chunk_y_bounds(terrain.config());
        let mut pipeline = StreamingPipeline::default();
        let stream_state = TerrainStreamState {
            center_x: spawn_chunk.x,
            center_y: spawn_chunk.y,
            center_z: spawn_chunk.z,
            valid: true,
        };
        let initial_h = config.render_distance.min(4);
        let initial_v = config.vertical_render_distance.min(4);
        pipeline.enqueue_area(
            &mut terrain,
            &mut session.stored_chunks,
            session.storage.as_ref(),
            &world,
            spawn_chunk,
            initial_h,
            initial_v,
            y_bounds,
            spawn,
        );

        let mut circuit = CircuitWorld::new();
        init_circuit_blocks(&mut circuit, &world, &registry);

        let cached = {
            let mut host = mod_host;
            let mut reg = registry;
            std::mem::swap(&mut host.registry, &mut reg);
            let (cached_manifest, cached_atlas) = host.build_handshake_content(&colormaps);
            std::mem::swap(&mut host.registry, &mut reg);
            (host, reg, cached_manifest, cached_atlas)
        };
        let (mod_host, registry, cached_manifest, cached_atlas) = cached;

        Ok(Self {
            config,
            mod_host,
            colormaps,
            world,
            circuit,
            terrain,
            column_blocks,
            biomes,
            session,
            generator,
            pipeline,
            stream_state,
            last_center: Some(spawn_chunk),
            air,
            registry,
            latest_pose: None,
            pending_priority: Vec::new(),
            pending_bulk: Vec::new(),
            circuit_accumulator: 0.0,
            server_id: 1,
            client_id: None,
            handshake_complete: false,
            handshake_pending: false,
            cached_manifest,
            cached_atlas,
        })
    }

    pub fn build_manifest(&mut self) -> stagcrest_protocol::manifest::ContentManifest {
        std::mem::swap(&mut self.mod_host.registry, &mut self.registry);
        let (manifest, atlas) = self.mod_host.build_handshake_content(&self.colormaps);
        self.cached_atlas = atlas;
        std::mem::swap(&mut self.mod_host.registry, &mut self.registry);
        manifest
    }

    pub fn build_atlas_transfer(&mut self) -> AtlasTransfer {
        self.build_manifest();
        self.cached_atlas.clone()
    }

    pub fn handle_client_message(&mut self, msg: ClientMessage) {
        net::handle_client_message(self, msg);
    }

    pub fn tick(&mut self, dt_secs: f32) {
        const CIRCUIT_TICK_INTERVAL: f32 = 0.1;

        self.circuit_accumulator += dt_secs;
        while self.circuit_accumulator >= CIRCUIT_TICK_INTERVAL {
            self.circuit_accumulator -= CIRCUIT_TICK_INTERVAL;
            self.circuit.tick(&mut self.world, &self.registry);
        }

        if self.handshake_complete {
            let stream_result = self.pipeline.tick(
                &mut self.world,
                &mut self.terrain,
                &mut self.session,
                self.column_blocks,
                &self.biomes,
                &self.registry,
                &self.generator,
                &self.stream_state,
                &mut self.last_center,
                self.air,
                self.config.render_distance,
                self.config.vertical_render_distance,
            );
            for snapshot in stream_result.snapshots {
                self.queue_bulk(GameMessage::Server(ServerMessage::ChunkSnapshot(snapshot)));
            }
            for pos in stream_result.unloads {
                self.queue_priority(GameMessage::Server(ServerMessage::ChunkUnload(pos)));
            }
        }
    }

    pub fn queue_priority(&mut self, msg: GameMessage) {
        self.pending_priority.push(msg);
    }

    pub fn queue_bulk(&mut self, msg: GameMessage) {
        self.pending_bulk.push(msg);
    }

    pub fn drain_outgoing(&mut self) -> impl Iterator<Item = GameMessage> + '_ {
        let priority = std::mem::take(&mut self.pending_priority);
        let bulk = std::mem::take(&mut self.pending_bulk);
        priority.into_iter().chain(bulk)
    }

    pub fn drain_priority(&mut self) -> impl Iterator<Item = GameMessage> + '_ {
        std::mem::take(&mut self.pending_priority).into_iter()
    }

    pub fn drain_bulk(&mut self) -> impl Iterator<Item = GameMessage> + '_ {
        std::mem::take(&mut self.pending_bulk).into_iter()
    }

    pub(crate) fn finish_handshake_if_wire_ready(&mut self, wire_ready: bool) {
        if self.handshake_pending && wire_ready {
            self.handshake_pending = false;
            self.handshake_complete = true;
        }
    }

    pub fn run_loop<T: GameTransport>(&mut self, transport: &mut T, tick_ms: u64) {
        let dt = tick_ms as f32 / 1000.0;
        loop {
            let mut latest_pose = None;
            while let Ok(Some(GameMessage::Client(msg))) = transport.try_recv() {
                match msg {
                    ClientMessage::Pose(pose) => {
                        latest_pose = Some(pose);
                    }
                    ClientMessage::Ping { nonce } => {
                        if let Some(pose) = latest_pose.take() {
                            net::handle_pose(self, pose);
                        }
                        self.handle_client_message(ClientMessage::Ping { nonce });
                    }
                    other => {
                        if let Some(pose) = latest_pose.take() {
                            net::handle_pose(self, pose);
                        }
                        self.handle_client_message(other);
                    }
                }
            }
            if let Some(pose) = latest_pose.take() {
                net::handle_pose(self, pose);
            }
            self.tick(dt);
            for msg in self.drain_outgoing() {
                if transport.send(msg).is_err() {
                    return;
                }
            }
            self.finish_handshake_if_wire_ready(true);
            transport.idle_wait(Duration::from_millis(tick_ms));
        }
    }
}

pub fn spawn_local(
    config: ServerConfig,
) -> Result<
    (std::thread::JoinHandle<()>, InProcessTransport),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let mut server = GameServer::bootstrap(config)?;
    let (server_transport, client_transport) = InProcessTransport::pair();
    let handle = std::thread::spawn(move || {
        let mut transport = server_transport;
        server.run_loop(&mut transport, 16);
    });
    Ok((handle, client_transport))
}

pub async fn run_standalone(
    config: ServerConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind = config
        .bind
        .clone()
        .unwrap_or_else(|| "0.0.0.0:4242".to_string());
    let mut server = GameServer::bootstrap(config.clone())?;
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!("stagcrest-server listening on {bind}");

    let mut net_config = NetConfig::default();
    net_config.sim_latency_ms = config.net_sim_latency_ms;
    let mut interval = tokio::time::interval(Duration::from_millis(16));
    let mut session: Option<stagcrest_net::AsyncTcpSession> = None;

    loop {
        tokio::select! {
            accept = listener.accept(), if session.is_none() => {
                let (stream, addr) = accept?;
                tracing::info!("client connected from {addr}");
                session = Some(spawn_tcp_session(stream, net_config.clone()).await?);
                server.handshake_complete = false;
                server.handshake_pending = false;
                server.client_id = None;
                if let Some(conn) = session.as_ref() {
                    conn.handshake_wire_ready
                        .store(false, std::sync::atomic::Ordering::Release);
                }
            }
            _ = interval.tick() => {
                let mut disconnected = false;
                if let Some(conn) = session.as_mut() {
                    loop {
                        match conn.incoming.try_recv() {
                            Ok(msg) => {
                                if let GameMessage::Client(client_msg) = msg {
                                    server.handle_client_message(client_msg);
                                }
                            }
                            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                                disconnected = true;
                                break;
                            }
                        }
                    }
                    if !disconnected {
                        server.tick(0.016);
                        for msg in server.drain_priority() {
                            if send_message(conn, msg).await.is_err() {
                                disconnected = true;
                                break;
                            }
                        }
                        if !disconnected
                            && conn.handshake_wire_ready.load(std::sync::atomic::Ordering::Acquire)
                        {
                            for msg in server.drain_bulk() {
                                if send_message(conn, msg).await.is_err() {
                                    disconnected = true;
                                    break;
                                }
                            }
                        }
                        server.finish_handshake_if_wire_ready(
                            conn.handshake_wire_ready
                                .load(std::sync::atomic::Ordering::Acquire),
                        );
                    }
                } else {
                    server.tick(0.016);
                }
                if disconnected {
                    session = None;
                    server.handshake_complete = false;
                    server.handshake_pending = false;
                    server.client_id = None;
                }
            }
        }
    }
}
