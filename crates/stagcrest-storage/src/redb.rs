use std::path::{Path, PathBuf};

use redb::{Database, TableDefinition};
use stagcrest_protocol::ChunkPos;

use crate::compress::{compress_stored, decompress_stored};
use crate::format::InactiveChunk;
use crate::port::{encode_chunk_key, ChunkStorage};
use crate::StorageError;

const CHUNKS_TABLE: TableDefinition<&[u8; 12], &[u8]> = TableDefinition::new("chunks");
const META_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");

pub struct RedbChunkStorage {
    db: Database,
    path: PathBuf,
}

impl RedbChunkStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StorageError::Io(format!("create world dir: {e}")))?;
        }
        let db = Database::create(&path).map_err(|e| StorageError::Database(e.to_string()))?;
        {
            let txn = db
                .begin_write()
                .map_err(|e| StorageError::Database(e.to_string()))?;
            {
                let _ = txn.open_table(CHUNKS_TABLE);
                let _ = txn.open_table(META_TABLE);
            }
            txn.commit()
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        Ok(Self { db, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn put_meta(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        let txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        {
            let mut table = txn
                .open_table(META_TABLE)
                .map_err(|e| StorageError::Database(e.to_string()))?;
            table
                .insert(key, value)
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let table = txn
            .open_table(META_TABLE)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        match table.get(key) {
            Ok(Some(value)) => Ok(Some(value.value().to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    pub fn put_chunk(&self, pos: ChunkPos, chunk: &InactiveChunk) -> Result<(), StorageError> {
        let wire = chunk.encode_wire();
        let compressed = compress_stored(&wire);
        self.put(pos, &compressed)
    }

    pub fn load_chunk(&self, pos: ChunkPos) -> Result<Option<InactiveChunk>, StorageError> {
        let Some(compressed) = self.get(pos)? else {
            return Ok(None);
        };
        let wire = decompress_stored(&compressed)?;
        Ok(Some(InactiveChunk::decode_wire(&wire)?))
    }
}

impl ChunkStorage for RedbChunkStorage {
    fn get(&self, pos: ChunkPos) -> Result<Option<Vec<u8>>, StorageError> {
        let key = encode_chunk_key(pos);
        let txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let table = txn
            .open_table(CHUNKS_TABLE)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        match table.get(&key) {
            Ok(Some(value)) => Ok(Some(value.value().to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    fn put(&self, pos: ChunkPos, compressed: &[u8]) -> Result<(), StorageError> {
        let key = encode_chunk_key(pos);
        let txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        {
            let mut table = txn
                .open_table(CHUNKS_TABLE)
                .map_err(|e| StorageError::Database(e.to_string()))?;
            table
                .insert(&key, compressed)
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    fn delete(&self, pos: ChunkPos) -> Result<(), StorageError> {
        let key = encode_chunk_key(pos);
        let txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        {
            let mut table = txn
                .open_table(CHUNKS_TABLE)
                .map_err(|e| StorageError::Database(e.to_string()))?;
            let _ = table
                .remove(&key)
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    fn contains(&self, pos: ChunkPos) -> bool {
        self.get(pos).ok().flatten().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{BlockId, BlockState, CHUNK_VOLUME};

    #[test]
    fn redb_roundtrip() {
        let dir = std::env::temp_dir().join(format!("stagcrest_redb_test_{}", std::process::id()));
        let _ = std::fs::remove_file(dir.join("world.redb"));
        let storage = RedbChunkStorage::open(dir.join("world.redb")).unwrap();
        let chunk = InactiveChunk::from_indices(
            vec![BlockId(0), BlockId(1)],
            vec![BlockState(0), BlockState(0)],
            &vec![1u16; CHUNK_VOLUME],
        )
        .unwrap();
        storage
            .put_chunk(ChunkPos { x: 1, y: 2, z: 3 }, &chunk)
            .unwrap();
        let loaded = storage
            .load_chunk(ChunkPos { x: 1, y: 2, z: 3 })
            .unwrap()
            .unwrap();
        assert_eq!(loaded, chunk);
        assert!(storage.contains(ChunkPos { x: 1, y: 2, z: 3 }));
    }
}
