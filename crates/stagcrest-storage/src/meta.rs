use crate::redb::RedbChunkStorage;
use crate::StorageError;

pub const WORLD_META_KEY: &str = "world_meta";
pub const WORLD_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldMeta {
    pub format_version: u32,
    pub world_seed: u64,
    pub circuit_tick: u64,
    /// Seconds within the day/night cycle ([`stagcrest_protocol::DAY_LENGTH_SECS`]).
    pub world_time: f64,
}

impl Default for WorldMeta {
    fn default() -> Self {
        Self {
            format_version: WORLD_FORMAT_VERSION,
            world_seed: 0,
            circuit_tick: 0,
            world_time: 0.0,
        }
    }
}

impl WorldMeta {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(28);
        out.extend_from_slice(&self.format_version.to_le_bytes());
        out.extend_from_slice(&self.world_seed.to_le_bytes());
        out.extend_from_slice(&self.circuit_tick.to_le_bytes());
        out.extend_from_slice(&self.world_time.to_le_bytes());
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
        Ok(Self {
            format_version: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            world_seed: u64::from_le_bytes(bytes[4..12].try_into().unwrap()),
            circuit_tick: u64::from_le_bytes(bytes[12..20].try_into().unwrap()),
            world_time,
        })
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
