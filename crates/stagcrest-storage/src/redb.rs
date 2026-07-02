use std::path::{Path, PathBuf};

use redb::{Database, ReadableTable, TableDefinition};
use stagcrest_protocol::ChunkPos;

use crate::compress::{compress_stored, decompress_stored};
use crate::format::InactiveChunk;
use crate::port::{encode_chunk_key, encode_map_chunk_key, ChunkStorage};
use crate::StorageError;

const CHUNKS_TABLE: TableDefinition<&[u8; 12], &[u8]> = TableDefinition::new("chunks");
const CHUNK_REV_TABLE: TableDefinition<&[u8; 12], [u8; 8]> = TableDefinition::new("chunk_rev");
const META_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const MAP_CHUNKS_TABLE: TableDefinition<&[u8; 8], &[u8]> = TableDefinition::new("map_chunks");

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
                let _ = txn.open_table(CHUNK_REV_TABLE);
                let _ = txn.open_table(META_TABLE);
                let _ = txn.open_table(MAP_CHUNKS_TABLE);
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

    pub fn get_chunk_revision(&self, pos: ChunkPos) -> Result<u64, StorageError> {
        let key = encode_chunk_key(pos);
        let txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let table = txn
            .open_table(CHUNK_REV_TABLE)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        match table.get(&key) {
            Ok(Some(value)) => Ok(u64::from_be_bytes(value.value())),
            Ok(None) => Ok(0),
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    pub fn increment_chunk_revision(&self, pos: ChunkPos) -> Result<u64, StorageError> {
        let key = encode_chunk_key(pos);
        let txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let next = {
            let mut table = txn
                .open_table(CHUNK_REV_TABLE)
                .map_err(|e| StorageError::Database(e.to_string()))?;
            let current = match table.get(&key) {
                Ok(Some(value)) => u64::from_be_bytes(value.value()),
                Ok(None) => 0,
                Err(e) => return Err(StorageError::Database(e.to_string())),
            };
            let next = current.saturating_add(1);
            table
                .insert(&key, next.to_be_bytes())
                .map_err(|e| StorageError::Database(e.to_string()))?;
            next
        };
        txn.commit()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(next)
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

    /// Iterate all stored chunk positions.
    pub fn iter_chunk_positions(&self) -> Result<Vec<ChunkPos>, StorageError> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let table = txn
            .open_table(CHUNKS_TABLE)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let mut out = Vec::new();
        for entry in table
            .iter()
            .map_err(|e| StorageError::Database(e.to_string()))?
        {
            let (key, _) = entry.map_err(|e| StorageError::Database(e.to_string()))?;
            if let Some(pos) = crate::port::decode_chunk_key(key.value()) {
                out.push(pos);
            }
        }
        Ok(out)
    }

    pub fn put_map_chunk(&self, mx: i32, mz: i32, data: &[u8]) -> Result<(), StorageError> {
        let key = encode_map_chunk_key(mx, mz);
        let txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        {
            let mut table = txn
                .open_table(MAP_CHUNKS_TABLE)
                .map_err(|e| StorageError::Database(e.to_string()))?;
            table
                .insert(&key, data)
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    pub fn get_map_chunk(&self, mx: i32, mz: i32) -> Result<Option<Vec<u8>>, StorageError> {
        let key = encode_map_chunk_key(mx, mz);
        let txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let table = txn
            .open_table(MAP_CHUNKS_TABLE)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        match table.get(&key) {
            Ok(Some(value)) => Ok(Some(value.value().to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Database(e.to_string())),
        }
    }

    pub fn delete_map_chunk(&self, mx: i32, mz: i32) -> Result<(), StorageError> {
        let key = encode_map_chunk_key(mx, mz);
        let txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        {
            let mut table = txn
                .open_table(MAP_CHUNKS_TABLE)
                .map_err(|e| StorageError::Database(e.to_string()))?;
            let _ = table
                .remove(&key)
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    /// Iterate all stored map chunk coordinates.
    pub fn iter_map_chunk_coords(&self) -> Result<Vec<(i32, i32)>, StorageError> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let table = txn
            .open_table(MAP_CHUNKS_TABLE)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let mut out = Vec::new();
        for entry in table
            .iter()
            .map_err(|e| StorageError::Database(e.to_string()))?
        {
            let (key, _) = entry.map_err(|e| StorageError::Database(e.to_string()))?;
            if let Some(coords) = crate::port::decode_map_chunk_key(key.value()) {
                out.push(coords);
            }
        }
        Ok(out)
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
            let mut chunks = txn
                .open_table(CHUNKS_TABLE)
                .map_err(|e| StorageError::Database(e.to_string()))?;
            chunks
                .insert(&key, compressed)
                .map_err(|e| StorageError::Database(e.to_string()))?;

            let mut revs = txn
                .open_table(CHUNK_REV_TABLE)
                .map_err(|e| StorageError::Database(e.to_string()))?;
            let current = match revs.get(&key) {
                Ok(Some(value)) => u64::from_be_bytes(value.value()),
                Ok(None) => 0,
                Err(e) => return Err(StorageError::Database(e.to_string())),
            };
            revs
                .insert(&key, current.saturating_add(1).to_be_bytes())
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

    #[test]
    fn iter_chunk_positions_roundtrip() {
        let dir = std::env::temp_dir().join(format!("stagcrest_redb_iter_{}", std::process::id()));
        let _ = std::fs::remove_file(dir.join("world.redb"));
        let storage = RedbChunkStorage::open(dir.join("world.redb")).unwrap();
        let chunk = InactiveChunk::from_indices(
            vec![BlockId(0), BlockId(1)],
            vec![BlockState(0), BlockState(0)],
            &vec![1u16; CHUNK_VOLUME],
        )
        .unwrap();
        storage
            .put_chunk(ChunkPos { x: -2, y: 0, z: 5 }, &chunk)
            .unwrap();
        let positions = storage.iter_chunk_positions().unwrap();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], ChunkPos { x: -2, y: 0, z: 5 });
    }

    #[test]
    fn chunk_revision_increments_on_write() {
        let dir = std::env::temp_dir().join(format!("stagcrest_redb_rev_{}", std::process::id()));
        let _ = std::fs::remove_file(dir.join("world.redb"));
        let storage = RedbChunkStorage::open(dir.join("world.redb")).unwrap();
        let pos = ChunkPos { x: 0, y: 0, z: 0 };
        assert_eq!(storage.get_chunk_revision(pos).unwrap(), 0);
        let chunk = InactiveChunk::from_indices(
            vec![BlockId(0), BlockId(1)],
            vec![BlockState(0), BlockState(0)],
            &vec![1u16; CHUNK_VOLUME],
        )
        .unwrap();
        storage.put_chunk(pos, &chunk).unwrap();
        assert_eq!(storage.get_chunk_revision(pos).unwrap(), 1);
        storage.put_chunk(pos, &chunk).unwrap();
        assert_eq!(storage.get_chunk_revision(pos).unwrap(), 2);
    }

    #[test]
    fn map_chunk_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("stagcrest_redb_map_{}", std::process::id()));
        let _ = std::fs::remove_file(dir.join("world.redb"));
        let storage = RedbChunkStorage::open(dir.join("world.redb")).unwrap();
        let data = vec![1u8, 2, 3, 4];
        storage.put_map_chunk(-1, 2, &data).unwrap();
        let loaded = storage.get_map_chunk(-1, 2).unwrap().unwrap();
        assert_eq!(loaded, data);
        let coords = storage.iter_map_chunk_coords().unwrap();
        assert_eq!(coords, vec![(-1, 2)]);
        storage.delete_map_chunk(-1, 2).unwrap();
        assert!(storage.get_map_chunk(-1, 2).unwrap().is_none());
    }
}
