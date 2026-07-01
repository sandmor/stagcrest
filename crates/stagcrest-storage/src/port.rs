use stagcrest_protocol::ChunkPos;
use thiserror::Error;

use crate::compress::{compress_stored, decompress_stored};
use crate::format::InactiveChunk;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage IO: {0}")]
    Io(String),
    #[error("database: {0}")]
    Database(String),
    #[error("decompress: {0}")]
    Decompress(String),
    #[error("format: {0}")]
    Format(#[from] crate::format::StorageFormatError),
}

pub trait ChunkStorage: Send + Sync {
    fn get(&self, pos: ChunkPos) -> Result<Option<Vec<u8>>, StorageError>;
    fn put(&self, pos: ChunkPos, compressed: &[u8]) -> Result<(), StorageError>;
    fn delete(&self, pos: ChunkPos) -> Result<(), StorageError>;
    fn contains(&self, pos: ChunkPos) -> bool;
}

pub fn load_inactive_chunk(
    storage: &dyn ChunkStorage,
    pos: ChunkPos,
) -> Result<Option<InactiveChunk>, StorageError> {
    let Some(compressed) = storage.get(pos)? else {
        return Ok(None);
    };
    let wire = decompress_stored(&compressed)?;
    Ok(Some(InactiveChunk::decode_wire(&wire)?))
}

pub fn store_inactive_chunk(
    storage: &dyn ChunkStorage,
    pos: ChunkPos,
    chunk: &InactiveChunk,
) -> Result<(), StorageError> {
    let wire = chunk.encode_wire();
    let compressed = compress_stored(&wire);
    storage.put(pos, &compressed)
}

impl<T: ChunkStorage + ?Sized> ChunkStorage for std::sync::Arc<T> {
    fn get(&self, pos: ChunkPos) -> Result<Option<Vec<u8>>, StorageError> {
        (**self).get(pos)
    }

    fn put(&self, pos: ChunkPos, compressed: &[u8]) -> Result<(), StorageError> {
        (**self).put(pos, compressed)
    }

    fn delete(&self, pos: ChunkPos) -> Result<(), StorageError> {
        (**self).delete(pos)
    }

    fn contains(&self, pos: ChunkPos) -> bool {
        (**self).contains(pos)
    }
}

pub fn encode_chunk_key(pos: ChunkPos) -> [u8; 12] {
    let mut key = [0u8; 12];
    key[0..4].copy_from_slice(&pos.x.to_be_bytes());
    key[4..8].copy_from_slice(&pos.y.to_be_bytes());
    key[8..12].copy_from_slice(&pos.z.to_be_bytes());
    key
}

pub fn decode_map_chunk_key(key: &[u8]) -> Option<(i32, i32)> {
    if key.len() != 8 {
        return None;
    }
    let mx = i32::from_be_bytes(key[0..4].try_into().ok()?);
    let mz = i32::from_be_bytes(key[4..8].try_into().ok()?);
    Some((mx, mz))
}

pub fn encode_map_chunk_key(mx: i32, mz: i32) -> [u8; 8] {
    let mut key = [0u8; 8];
    key[0..4].copy_from_slice(&mx.to_be_bytes());
    key[4..8].copy_from_slice(&mz.to_be_bytes());
    key
}

pub fn decode_chunk_key(key: &[u8]) -> Option<ChunkPos> {
    if key.len() != 12 {
        return None;
    }
    let x = i32::from_be_bytes(key[0..4].try_into().ok()?);
    let y = i32::from_be_bytes(key[4..8].try_into().ok()?);
    let z = i32::from_be_bytes(key[8..12].try_into().ok()?);
    Some(ChunkPos { x, y, z })
}
