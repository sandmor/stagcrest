mod build_world;
mod chunk_gen;
mod client_session;
mod export_minimap;
mod interest;
mod map_generation;
mod map_streaming;
mod map_tile_maintenance;
mod net;
mod offline_bootstrap;
mod persistence;
mod player;
mod session;
mod streaming;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use stagcrest_circuit::{init_circuit_blocks, CircuitWorld};
use stagcrest_mod_server::{
    load_mods, world_chunk_y_bounds, BiomeRegistry, BlockRegistry, ColormapSet, ColumnBlocks,
    ModHost, TerrainGenerator, WorldGenState, WorldSeed, SEA_LEVEL,
};
use stagcrest_net::{
    send_message, spawn_tcp_session, BlockUpdate, CircuitPowerBatch, ClientMessage, GameMessage,
    GameTransport, InProcessTransport, NetConfig, ServerMessage,
};
use stagcrest_minimap::{world_chunk_to_map_chunk, MapResolveContext};
use stagcrest_protocol::{manifest::AtlasTransfer, BlockId, BlockPos, ChunkPos};
use stagcrest_world::World;

use crate::map_generation::MapChunkPipeline;
use crate::map_streaming::{ServerBlobCache, MAX_MAP_SNAPSHOT_SEND_PER_TICK};

use tokio::net::TcpListener;

pub use build_world::{build_world_region, iter_circle_chunk_positions, BuildMapConfig, BuildMapError, BuildMapReport};
pub use client_session::{ClientId, ClientRegistry, ConnectedClient};
pub use export_minimap::{export_minimap, rebuild_all_map_chunks, ExportError, ExportMinimapConfig};
pub use map_generation::make_map_resolve_context;
pub use map_tile_maintenance::{rebuild_all_map_tiles, MapTileDirtySet, MapTileRebuildReport};
pub use offline_bootstrap::{
    bootstrap_offline, load_worldgen_context, open_offline_world, BootstrapError, OfflineWorld,
    WorldSeedPolicy, WorldgenContext,
};
pub use player::apply_player_action;
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
    pub max_clients: usize,
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
            max_clients: 16,
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
    pub air: BlockId,
    pub registry: BlockRegistry,
    circuit_accumulator: f32,
    pub server_id: u64,
    cached_manifest: stagcrest_protocol::manifest::ContentManifest,
    cached_atlas: AtlasTransfer,
    map_pipeline: MapChunkPipeline,
    map_ctx: Arc<MapResolveContext>,
    map_y_chunks: Vec<i32>,
    map_blob_cache: ServerBlobCache,
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

        let mut session = WorldSession::open(&config.world_name, config.world_seed)?;
        let lru_cap = streaming_lru_capacity(
            config.render_distance,
            config.vertical_render_distance,
            config.max_clients,
        );
        let world = World::with_lru_capacity(lru_cap, air);
        let mut terrain = WorldGenState::new(WorldSeed(config.world_seed));
        terrain.apply_river_config(biomes.river_config());
        let generator = terrain.generator().clone();

        let spawn = BlockPos::new(8, SEA_LEVEL + 16, 8);
        let spawn_chunk = spawn.chunk_pos();
        let y_bounds = world_chunk_y_bounds(terrain.config());
        let mut pipeline = StreamingPipeline::default();
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
            y_bounds.clone(),
            spawn,
        );

        let mut circuit = CircuitWorld::new();
        circuit.set_tick(session.meta.circuit_tick);
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

        let map_ctx = Arc::new(map_generation::make_map_resolve_context(
            &registry,
            &colormaps,
            &biomes,
            air,
            terrain.config(),
        ));
        let map_y_chunks: Vec<i32> = y_bounds.collect();
        let map_pipeline =
            MapChunkPipeline::new(Arc::clone(&session.storage), Arc::clone(&map_ctx), map_y_chunks.clone());

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
            air,
            registry,
            circuit_accumulator: 0.0,
            server_id: 1,
            cached_manifest,
            cached_atlas,
            map_pipeline,
            map_ctx,
            map_y_chunks,
            map_blob_cache: ServerBlobCache::default(),
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

    pub fn handle_client_message(
        &mut self,
        clients: &mut ClientRegistry,
        client_id: ClientId,
        msg: ClientMessage,
    ) {
        net::handle_client_message(self, clients, client_id, msg);
    }

    pub fn fanout_block_update(
        &self,
        clients: &mut ClientRegistry,
        update: BlockUpdate,
        h_radius: i32,
        v_radius: i32,
    ) {
        clients.fanout_block_update(update, h_radius, v_radius);
    }

    pub(crate) fn broadcast_circuit_replication(&mut self, clients: &mut ClientRegistry) {
        let h = self.config.render_distance;
        let v = self.config.vertical_render_distance;
        for (pos, id, state) in self.circuit.drain_visual_updates() {
            clients.fanout_block_update(BlockUpdate { pos, id, state }, h, v);
        }
        let updates = self.circuit.drain_power_updates();
        if !updates.is_empty() {
            clients.fanout_circuit_batch(CircuitPowerBatch { updates }, h, v);
        }
    }

    pub fn tick(&mut self, clients: &mut ClientRegistry, dt_secs: f32) {
        const CIRCUIT_TICK_INTERVAL: f32 = 0.1;

        self.circuit_accumulator += dt_secs;
        while self.circuit_accumulator >= CIRCUIT_TICK_INTERVAL {
            self.circuit_accumulator -= CIRCUIT_TICK_INTERVAL;
            self.circuit.tick(&mut self.world, &self.registry);
            self.session
                .persistence
                .absorb_dirty_chunks(self.circuit.drain_dirty_chunks());
            self.broadcast_circuit_replication(clients);
        }

        if clients.any_handshake_complete() {
            let streaming_count = clients
                .clients()
                .iter()
                .filter(|c| c.handshake_complete && c.stream.valid)
                .count();
            let enqueue_rotate = if streaming_count > 0 {
                clients.next_enqueue_client_index(streaming_count)
            } else {
                0
            };
            let fair_rotate = if streaming_count > 0 {
                clients.next_fair_client_index(streaming_count)
            } else {
                0
            };

            let stream_result = self.pipeline.tick(
                &mut self.world,
                &mut self.terrain,
                &mut self.circuit,
                &mut self.session,
                self.column_blocks,
                &self.biomes,
                &self.registry,
                &self.generator,
                clients.clients_mut(),
                self.air,
                self.config.render_distance,
                self.config.vertical_render_distance,
                enqueue_rotate,
                fair_rotate,
            );

            self.queue_map_regen_for_persisted(&stream_result.persisted);

            for (client_id, delta) in stream_result.per_client {
                let Some(client) = clients.get_mut(client_id) else {
                    continue;
                };
                for snapshot in delta.snapshots {
                    let chunk = snapshot.pos;
                    client.queue_bulk(GameMessage::Server(ServerMessage::ChunkSnapshot(
                        snapshot,
                    )));
                    let seed = self.circuit.power_in_chunk(chunk);
                    if !seed.is_empty() {
                        client.queue_bulk(GameMessage::Server(ServerMessage::CircuitPowerBatch(
                            CircuitPowerBatch { updates: seed },
                        )));
                    }
                }
                for pos in delta.unloads {
                    client.queue_priority(GameMessage::Server(ServerMessage::ChunkUnload(pos)));
                }
            }

            self.map_pipeline.tick(
                &self.world,
                &self.map_y_chunks,
                &self.map_ctx,
                &self.session.storage,
            );
            self.tick_map_streaming(clients);
        }
    }

    fn tick_map_streaming(&mut self, clients: &mut ClientRegistry) {
        for done in self.map_pipeline.drain_completions() {
            self.map_blob_cache
                .insert(done.mx, done.mz, done.blob.clone());
            for client in clients.clients_mut() {
                if let Some(msg) = client
                    .map
                    .fan_out_regen(done.mx, done.mz, done.blob.clone())
                {
                    client.queue_bulk(msg);
                }
            }
        }

        let storage = self.session.storage.as_ref();
        for client in clients.clients_mut() {
            let snapshots = client.map.drain_pending(
                MAX_MAP_SNAPSHOT_SEND_PER_TICK,
                &mut self.map_blob_cache,
                storage,
                &self.world,
                &self.map_y_chunks,
                &mut self.map_pipeline,
            );
            for snap in snapshots {
                client.queue_bulk(GameMessage::Server(ServerMessage::MapChunkSnapshot(snap)));
            }
        }
    }

    fn queue_map_regen_for_persisted(&mut self, positions: &[ChunkPos]) {
        if positions.is_empty() {
            return;
        }
        let storage = self.session.storage.as_ref();
        let mut seen = std::collections::HashSet::new();
        for &pos in positions {
            let (mx, mz) = world_chunk_to_map_chunk(pos.x, pos.z);
            if seen.insert((mx, mz)) {
                self.map_pipeline.ensure_fresh(
                    mx,
                    mz,
                    &self.world,
                    &self.map_y_chunks,
                    storage,
                );
            }
        }
    }

    pub fn flush_persistence(&mut self) {
        let persisted = self.session.persistence.flush_all(
            &mut self.world,
            &self.terrain,
            &self.circuit,
            &mut self.session.stored_chunks,
        );
        self.queue_map_regen_for_persisted(&persisted);
        if let Err(err) = self.session.save_meta(self.circuit.current_tick()) {
            tracing::error!("failed to save world meta: {err}");
        }
    }

    pub(crate) fn mark_chunk_dirty(&mut self, pos: ChunkPos) {
        self.session.persistence.mark_dirty(pos);
    }

    pub fn run_loop<T: GameTransport>(&mut self, transport: &mut T, tick_ms: u64) {
        let mut clients = ClientRegistry::new(1);
        let embedded_id = clients.register_inprocess();
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
                            if let Some(c) = clients.get_mut(embedded_id) {
                                net::handle_pose(c, pose);
                            }
                        }
                        self.handle_client_message(
                            &mut clients,
                            embedded_id,
                            ClientMessage::Ping { nonce },
                        );
                    }
                    other => {
                        if let Some(pose) = latest_pose.take() {
                            if let Some(c) = clients.get_mut(embedded_id) {
                                net::handle_pose(c, pose);
                            }
                        }
                        self.handle_client_message(&mut clients, embedded_id, other);
                    }
                }
            }
            if let Some(pose) = latest_pose.take() {
                if let Some(c) = clients.get_mut(embedded_id) {
                    net::handle_pose(c, pose);
                }
            }

            self.tick(&mut clients, dt);

            if let Some(c) = clients.get_mut(embedded_id) {
                c.finish_handshake_if_wire_ready(true);
                let priority = c.take_priority();
                let bulk = c.take_bulk();
                for msg in priority.into_iter().chain(bulk) {
                    if transport.send(msg).is_err() {
                        self.flush_persistence();
                        return;
                    }
                }
            }

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

async fn drain_client_io(clients: &mut ClientRegistry, id: ClientId) -> bool {
    let wire_ready = clients
        .get(id)
        .and_then(|c| c.tcp.as_ref())
        .is_some_and(|conn| {
            conn.handshake_wire_ready
                .load(std::sync::atomic::Ordering::Acquire)
        });

    let priority = clients
        .get_mut(id)
        .map(|c| c.take_priority())
        .unwrap_or_default();
    let bulk = if wire_ready {
        clients
            .get_mut(id)
            .map(|c| c.take_bulk())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let Some(conn) = clients.get_mut(id).and_then(|c| c.tcp.as_mut()) else {
        return false;
    };

    for msg in priority {
        if send_message(conn, msg).await.is_err() {
            return true;
        }
    }

    if wire_ready {
        for msg in bulk {
            if send_message(conn, msg).await.is_err() {
                return true;
            }
        }
        if let Some(c) = clients.get_mut(id) {
            c.finish_handshake_if_wire_ready(true);
        }
    }

    false
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
    let mut clients = ClientRegistry::new(config.max_clients);

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                tracing::info!("shutdown requested, flushing world");
                server.flush_persistence();
                return Ok(());
            }
            accept = listener.accept(), if clients.has_capacity() => {
                let (stream, addr) = accept?;
                tracing::info!("client connected from {addr}");
                let session = spawn_tcp_session(stream, net_config.clone()).await?;
                session.handshake_wire_ready
                    .store(false, std::sync::atomic::Ordering::Release);
                clients.register_tcp(session);
            }
            _ = interval.tick() => {
                let client_ids = clients.client_ids();
                let mut disconnected = Vec::new();
                let mut inbound: Vec<(ClientId, ClientMessage)> = Vec::new();

                for id in &client_ids {
                    let Some(client) = clients.get_mut(*id) else {
                        continue;
                    };
                    let Some(conn) = client.tcp.as_mut() else {
                        continue;
                    };
                    loop {
                        match conn.incoming.try_recv() {
                            Ok(GameMessage::Client(client_msg)) => {
                                inbound.push((*id, client_msg));
                            }
                            Ok(_) => {}
                            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                                disconnected.push(*id);
                                break;
                            }
                        }
                    }
                }

                for (id, msg) in inbound {
                    server.handle_client_message(&mut clients, id, msg);
                }

                server.tick(&mut clients, 0.016);

                for id in &client_ids {
                    if disconnected.contains(id) {
                        continue;
                    }
                    if drain_client_io(&mut clients, *id).await {
                        disconnected.push(*id);
                    }
                }

                for id in disconnected {
                    tracing::info!("client {:?} disconnected", id);
                    clients.remove(id);
                }
            }
        }
    }
}
