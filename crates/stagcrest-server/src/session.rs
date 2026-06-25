use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use stagcrest_protocol::ChunkPos;
use stagcrest_storage::{ChunkStorage, RedbChunkStorage};

pub struct WorldSession {
    pub world_name: String,
    pub storage: Arc<dyn ChunkStorage>,
    pub stored_chunks: HashSet<ChunkPos>,
}

impl WorldSession {
    pub fn open(world_name: impl Into<String>) -> Result<Self, stagcrest_storage::StorageError> {
        let world_name = world_name.into();
        let path = Path::new("worlds").join(&world_name).join("world.redb");
        let storage = RedbChunkStorage::open(path)?;
        Ok(Self {
            world_name,
            storage: Arc::new(storage),
            stored_chunks: HashSet::new(),
        })
    }
}

pub fn streaming_lru_capacity(render_distance: i32, vertical_render_distance: i32) -> usize {
    let footprint_h = 2 * (render_distance + 2) + 1;
    let footprint_v = 2 * (vertical_render_distance + 2) + 1;
    (footprint_h * footprint_h * footprint_v) as usize + 64
}
