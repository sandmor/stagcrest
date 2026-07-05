use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use stagcrest_protocol::ChunkPos;
use stagcrest_storage::{RedbChunkStorage, StorageError, WorldMeta, DATA_DIR};

use crate::persistence::ChunkPersistence;

pub struct WorldSession {
    pub world_name: String,
    pub storage: Arc<RedbChunkStorage>,
    pub stored_chunks: HashSet<ChunkPos>,
    pub meta: WorldMeta,
    pub persistence: ChunkPersistence,
}

impl WorldSession {
    pub fn open(world_name: impl Into<String>, world_seed: u64) -> Result<Self, StorageError> {
        Self::open_with_resolved_seed(world_name, Some(world_seed))
    }

    /// Open world storage. When `world_seed` is `None`, keeps the seed stored in world meta (or 42 if unset).
    pub fn open_with_resolved_seed(
        world_name: impl Into<String>,
        world_seed: Option<u64>,
    ) -> Result<Self, StorageError> {
        let world_name = world_name.into();
        let path = Path::new(DATA_DIR)
            .join("worlds")
            .join(&world_name)
            .join("world.redb");
        let storage = Arc::new(RedbChunkStorage::open(path)?);
        let mut meta = WorldMeta::load(storage.as_ref())?;
        meta.world_seed = match world_seed {
            Some(seed) => seed,
            None => {
                if meta.world_seed == 0 {
                    42
                } else {
                    meta.world_seed
                }
            }
        };
        let persistence = ChunkPersistence::new(storage.clone());
        Ok(Self {
            world_name,
            storage,
            stored_chunks: HashSet::new(),
            meta,
            persistence,
        })
    }

    pub fn save_meta(&self, circuit_tick: u64) -> Result<(), StorageError> {
        let meta = WorldMeta {
            format_version: stagcrest_storage::WORLD_FORMAT_VERSION,
            world_seed: self.meta.world_seed,
            circuit_tick,
            world_time: self.meta.world_time,
        };
        meta.save(self.storage.as_ref())
    }
}

pub const MAX_WORLD_LRU_CHUNKS: usize = 65_536;

pub fn streaming_lru_capacity(
    render_distance: i32,
    vertical_render_distance: i32,
    active_clients: usize,
) -> usize {
    let footprint_h = 2 * (render_distance + 2) + 1;
    let footprint_v = 2 * (vertical_render_distance + 2) + 1;
    let single = (footprint_h * footprint_h * footprint_v) as usize + 64;
    let clients = active_clients.max(1);
    (single * clients).min(MAX_WORLD_LRU_CHUNKS)
}
