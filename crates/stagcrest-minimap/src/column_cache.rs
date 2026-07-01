use std::collections::HashMap;

use stagcrest_protocol::{ChunkPos, CHUNK_SIZE};

pub const DEFAULT_MAX_CACHE_COLUMNS: usize = 64_000;

#[derive(Debug, Clone, Copy)]
pub struct ColumnEntry {
    pub rgb: [u8; 3],
    pub loaded: bool,
}

#[derive(Debug, Default)]
pub struct ColumnCache {
    columns: HashMap<(i32, i32), ColumnEntry>,
    max_columns: usize,
}

impl ColumnCache {
    pub fn new(max_columns: usize) -> Self {
        Self {
            columns: HashMap::new(),
            max_columns,
        }
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn get(&self, wx: i32, wz: i32) -> Option<&ColumnEntry> {
        self.columns.get(&(wx, wz))
    }

    pub fn insert(&mut self, wx: i32, wz: i32, rgb: [u8; 3], loaded: bool) {
        if self.columns.len() >= self.max_columns && !self.columns.contains_key(&(wx, wz)) {
            self.evict_one();
        }
        self.columns.insert((wx, wz), ColumnEntry { rgb, loaded });
    }

    pub fn remove_column(&mut self, wx: i32, wz: i32) {
        self.columns.remove(&(wx, wz));
    }

    /// Remove cached columns in a chunk's horizontal footprint.
    pub fn invalidate_chunk(&mut self, pos: ChunkPos) -> Vec<(i32, i32)> {
        let base_x = pos.x * CHUNK_SIZE;
        let base_z = pos.z * CHUNK_SIZE;
        let mut keys = Vec::new();
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let wx = base_x + lx;
                let wz = base_z + lz;
                if self.columns.remove(&(wx, wz)).is_some() {
                    keys.push((wx, wz));
                }
            }
        }
        keys
    }

    pub fn evict_far_from(&mut self, center_x: i32, center_z: i32, radius: i32) {
        let r2 = (radius as i64) * (radius as i64);
        self.columns.retain(|(wx, wz), _| {
            let dx = *wx as i64 - center_x as i64;
            let dz = *wz as i64 - center_z as i64;
            dx * dx + dz * dz <= r2
        });
    }

    fn evict_one(&mut self) {
        if let Some(key) = self.columns.keys().next().copied() {
            self.columns.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidate_chunk_removes_cached_columns_only() {
        let mut cache = ColumnCache::new(1000);
        cache.insert(0, 0, [1, 2, 3], true);
        let keys = cache.invalidate_chunk(ChunkPos { x: 0, y: 0, z: 0 });
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], (0, 0));
        assert!(cache.get(0, 0).is_none());
    }

    #[test]
    fn insert_and_lookup() {
        let mut cache = ColumnCache::new(1000);
        cache.insert(5, 10, [100, 120, 80], true);
        let entry = cache.get(5, 10).unwrap();
        assert_eq!(entry.rgb, [100, 120, 80]);
        assert!(entry.loaded);
    }
}
