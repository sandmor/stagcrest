use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;

use crate::gpu_voxel::chunk_cache::{GpuChunkCache, GpuChunkExtractBatch};
use crate::gpu_voxel::render_chunk_store::RenderChunkStore;

/// Staging resource filled on the main world each frame, then extracted to render.
#[derive(Resource, Default, Clone, ExtractResource)]
pub struct GpuChunkExtractStaging(pub GpuChunkExtractBatch);

pub fn stage_gpu_chunk_extract(
    mut cache: ResMut<GpuChunkCache>,
    mut staging: ResMut<GpuChunkExtractStaging>,
) {
    staging.0 = cache.take_extract_batch();
}

pub fn apply_extracted_chunks(
    staging: Option<Res<GpuChunkExtractStaging>>,
    store: Option<ResMut<RenderChunkStore>>,
    mut pending: Option<ResMut<PendingChunkExtract>>,
) {
    let Some(staging) = staging else {
        return;
    };
    if staging.0.uploads.is_empty()
        && staging.0.removals.is_empty()
        && staging.0.rebuild_dirty.is_empty()
    {
        return;
    }
    if let Some(mut store) = store {
        store.apply_extract_batch(&staging.0);
    } else if let Some(mut pending) = pending {
        pending.batches.push(staging.0.clone());
    }
}

/// Batches extracted before render GPU buffers are initialized.
#[derive(Resource, Default)]
pub struct PendingChunkExtract {
    pub batches: Vec<GpuChunkExtractBatch>,
}
