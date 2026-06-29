use bevy::prelude::*;
use stagcrest_protocol::ChunkPos;
use std::collections::HashSet;

/// Render-world feedback published to the main world each frame.
#[derive(Resource, Default, Clone)]
pub struct GpuChunkRenderFeedback {
    pub failed_uploads: Vec<ChunkPos>,
    pub evicted: Vec<ChunkPos>,
    pub rendered: HashSet<ChunkPos>,
    /// Chunks that exceeded scratch capacity and need a fresh upload/emit.
    pub scratch_overflow: Vec<ChunkPos>,
}

impl GpuChunkRenderFeedback {
    pub fn clear_frame(&mut self) {
        self.failed_uploads.clear();
        self.evicted.clear();
        self.rendered.clear();
        self.scratch_overflow.clear();
    }
}

/// Main-world GPU sync state updated from render feedback.
#[derive(Resource, Default)]
pub struct GpuChunkSyncState {
    pub render_confirmed: HashSet<ChunkPos>,
}
