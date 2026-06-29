use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use stagcrest_protocol::ChunkPos;

use crate::gpu_voxel::types::GpuChunkUpload;

/// Delta batch extracted to the render world each frame (uploads + removals + rebuild flags).
#[derive(Clone, Default)]
pub struct GpuChunkExtractBatch {
    pub uploads: Vec<GpuChunkUpload>,
    pub removals: Vec<ChunkPos>,
    pub rebuild_dirty: HashSet<ChunkPos>,
}

/// Main-world chunk index. Bulk block data is queued for extract, not cloned every frame.
#[derive(Resource, Default)]
pub struct GpuChunkCache {
    loaded: HashSet<ChunkPos>,
    versions: HashMap<ChunkPos, u64>,
    extract: GpuChunkExtractBatch,
}

impl GpuChunkCache {
    pub fn commit(&mut self, upload: GpuChunkUpload) {
        let pos = upload.pos;
        self.loaded.insert(pos);
        let version = self.versions.entry(pos).or_insert(0);
        *version += 1;
        self.extract.rebuild_dirty.insert(pos);
        self.extract.uploads.push(upload);
    }

    pub fn remove(&mut self, pos: ChunkPos) {
        self.loaded.remove(&pos);
        self.versions.remove(&pos);
        self.extract.removals.push(pos);
        self.extract.rebuild_dirty.remove(&pos);
    }

    pub fn contains(&self, pos: ChunkPos) -> bool {
        self.loaded.contains(&pos)
    }

    pub fn get_version(&self, pos: ChunkPos) -> Option<u64> {
        self.versions.get(&pos).copied()
    }

    pub fn mark_dirty(&mut self, pos: ChunkPos) {
        if self.loaded.contains(&pos) {
            self.extract.rebuild_dirty.insert(pos);
        }
    }

    pub fn mark_all_dirty(&mut self) {
        self.extract
            .rebuild_dirty
            .extend(self.loaded.iter().copied());
    }

    pub fn chunk_count(&self) -> usize {
        self.loaded.len()
    }

    pub fn take_extract_batch(&mut self) -> GpuChunkExtractBatch {
        GpuChunkExtractBatch {
            uploads: std::mem::take(&mut self.extract.uploads),
            removals: std::mem::take(&mut self.extract.removals),
            rebuild_dirty: std::mem::take(&mut self.extract.rebuild_dirty),
        }
    }
}
