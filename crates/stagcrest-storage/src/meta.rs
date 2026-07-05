use crate::redb::RedbChunkStorage;
use crate::StorageError;

pub const WORLD_META_KEY: &str = "world_meta";
pub const WORLD_FORMAT_VERSION: u32 = 1;

/// Persisted mapping from numeric block id at save time to namespaced string id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRegistryEntry {
    pub id: u32,
    pub namespaced_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldMeta {
    pub format_version: u32,
    pub world_seed: u64,
    pub circuit_tick: u64,
    /// Seconds within the day/night cycle ([`stagcrest_protocol::DAY_LENGTH_SECS`]).
    pub world_time: f64,
    /// Snapshot of block id assignments at last save (id -> namespaced_id).
    pub block_registry: Vec<BlockRegistryEntry>,
}

impl Default for WorldMeta {
    fn default() -> Self {
        Self {
            format_version: WORLD_FORMAT_VERSION,
            world_seed: 0,
            circuit_tick: 0,
            world_time: 0.0,
            block_registry: Vec::new(),
        }
    }
}

impl WorldMeta {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + self.block_registry.len() * 48);
        out.extend_from_slice(&self.format_version.to_le_bytes());
        out.extend_from_slice(&self.world_seed.to_le_bytes());
        out.extend_from_slice(&self.circuit_tick.to_le_bytes());
        out.extend_from_slice(&self.world_time.to_le_bytes());
        out.extend_from_slice(&(self.block_registry.len() as u32).to_le_bytes());
        for entry in &self.block_registry {
            out.extend_from_slice(&entry.id.to_le_bytes());
            let name = entry.namespaced_id.as_bytes();
            out.extend_from_slice(&(name.len() as u32).to_le_bytes());
            out.extend_from_slice(name);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, StorageError> {
        if bytes.len() < 20 {
            return Err(StorageError::Format(
                crate::format::StorageFormatError::Truncated,
            ));
        }
        let world_time = if bytes.len() >= 28 {
            f64::from_le_bytes(bytes[20..28].try_into().unwrap())
        } else {
            0.0
        };
        let mut meta = Self {
            format_version: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            world_seed: u64::from_le_bytes(bytes[4..12].try_into().unwrap()),
            circuit_tick: u64::from_le_bytes(bytes[12..20].try_into().unwrap()),
            world_time,
            block_registry: Vec::new(),
        };
        if bytes.len() >= 32 {
            let count = u32::from_le_bytes(bytes[28..32].try_into().unwrap()) as usize;
            let mut cursor = 32usize;
            for _ in 0..count {
                if cursor + 8 > bytes.len() {
                    return Err(StorageError::Format(
                        crate::format::StorageFormatError::Truncated,
                    ));
                }
                let id = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
                cursor += 4;
                let name_len =
                    u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
                cursor += 4;
                if cursor + name_len > bytes.len() {
                    return Err(StorageError::Format(
                        crate::format::StorageFormatError::Truncated,
                    ));
                }
                let name = std::str::from_utf8(&bytes[cursor..cursor + name_len])
                    .map_err(|_| {
                        StorageError::Format(crate::format::StorageFormatError::Truncated)
                    })?
                    .to_string();
                cursor += name_len;
                meta.block_registry.push(BlockRegistryEntry {
                    id,
                    namespaced_id: name,
                });
            }
        }
        Ok(meta)
    }

    pub fn load(storage: &RedbChunkStorage) -> Result<Self, StorageError> {
        match storage.get_meta(WORLD_META_KEY)? {
            Some(bytes) => Self::decode(&bytes),
            None => Ok(Self::default()),
        }
    }

    pub fn save(&self, storage: &RedbChunkStorage) -> Result<(), StorageError> {
        storage.put_meta(WORLD_META_KEY, &self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_roundtrip_with_registry() {
        let meta = WorldMeta {
            format_version: WORLD_FORMAT_VERSION,
            world_seed: 42,
            circuit_tick: 100,
            world_time: 6000.0,
            block_registry: vec![
                BlockRegistryEntry {
                    id: 0,
                    namespaced_id: "stagcrest:air".into(),
                },
                BlockRegistryEntry {
                    id: 1,
                    namespaced_id: "stagcrest:stone".into(),
                },
            ],
        };
        let bytes = meta.encode();
        let decoded = WorldMeta::decode(&bytes).unwrap();
        assert_eq!(decoded, meta);
    }
}
