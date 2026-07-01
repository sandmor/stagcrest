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
        let world_name = world_name.into();
        let path = Path::new(DATA_DIR).join("worlds").join(&world_name).join("world.redb");
        let storage = Arc::new(RedbChunkStorage::open(path)?);
        let mut meta = WorldMeta::load(storage.as_ref())?;
        meta.world_seed = world_seed;
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
        };
        meta.save(self.storage.as_ref())
    }
}

pub fn streaming_lru_capacity(render_distance: i32, vertical_render_distance: i32) -> usize {
    let footprint_h = 2 * (render_distance + 2) + 1;
    let footprint_v = 2 * (vertical_render_distance + 2) + 1;
    (footprint_h * footprint_h * footprint_v) as usize + 64
}
