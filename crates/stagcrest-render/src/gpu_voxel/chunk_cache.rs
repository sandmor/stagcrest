use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;
use std::collections::{HashMap, HashSet};
use stagcrest_protocol::ChunkPos;

use crate::gpu_voxel::types::{GpuChunkUpload};

#[derive(Resource, Clone, Default, ExtractResource)]
pub struct GpuChunkCache {
    pub chunks: HashMap<ChunkPos, GpuChunkUpload>,
    pub dirty: HashSet<ChunkPos>,
    pub slots: Vec<crate::gpu_voxel::types::GpuChunkSlot>,
}

impl GpuChunkCache {
    pub fn commit(&mut self, upload: GpuChunkUpload) {
        let pos = upload.pos;
        self.chunks.insert(pos, upload);
        self.dirty.insert(pos);
    }

    pub fn remove(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
        self.dirty.insert(pos);
        self.slots.retain(|s| s.pos != pos);
    }

    pub fn get(&self, pos: ChunkPos) -> Option<&GpuChunkUpload> {
        self.chunks.get(&pos)
    }

    pub fn mark_dirty(&mut self, pos: ChunkPos) {
        if self.chunks.contains_key(&pos) {
            self.dirty.insert(pos);
        }
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty.extend(self.chunks.keys().copied());
    }

    pub fn take_dirty(&mut self) -> HashSet<ChunkPos> {
        std::mem::take(&mut self.dirty)
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn dispatch_list(&self) -> Vec<&GpuChunkUpload> {
        self.chunks.values().collect()
    }
}
