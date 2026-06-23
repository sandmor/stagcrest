use stagcrest_protocol::ChunkPos;

use crate::port::ChunkStorage;
use crate::StorageError;

#[derive(Debug, Default)]
pub struct NullChunkStorage;

impl ChunkStorage for NullChunkStorage {
    fn get(&self, _pos: ChunkPos) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(None)
    }

    fn put(&self, _pos: ChunkPos, _compressed: &[u8]) -> Result<(), StorageError> {
        Ok(())
    }

    fn delete(&self, _pos: ChunkPos) -> Result<(), StorageError> {
        Ok(())
    }

    fn contains(&self, _pos: ChunkPos) -> bool {
        false
    }
}
