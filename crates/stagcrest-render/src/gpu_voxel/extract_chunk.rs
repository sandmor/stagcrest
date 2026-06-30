use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;

use crate::gpu_voxel::chunk_cache::GpuChunkExtractBatch;
use crate::gpu_voxel::chunk_feedback::GpuChunkRenderFeedback;
use crate::gpu_voxel::chunk_gpu_pool::ChunkGpuPool;
use crate::gpu_voxel::rebuild_node::ChunkComputeBindCache;

/// Staging resource filled on the main world each frame, then extracted to render.
#[derive(Resource, Default, Clone, ExtractResource)]
pub struct GpuChunkExtractStaging(pub GpuChunkExtractBatch);

pub fn stage_gpu_chunk_extract(
    mut cache: ResMut<crate::gpu_voxel::chunk_cache::GpuChunkCache>,
    mut staging: ResMut<GpuChunkExtractStaging>,
) {
    staging.0 = cache.take_extract_batch();
}

pub fn apply_extracted_chunks(
    staging: Option<Res<GpuChunkExtractStaging>>,
    pool: Option<ResMut<ChunkGpuPool>>,
    pending: Option<ResMut<PendingChunkExtract>>,
    feedback: Option<ResMut<GpuChunkRenderFeedback>>,
    mut bind_cache: Option<ResMut<ChunkComputeBindCache>>,
) {
    let Some(staging) = staging else {
        return;
    };
    if staging.0.uploads.is_empty()
        && staging.0.removals.is_empty()
        && staging.0.rebuild_dirty.is_empty()
        && !staging.0.full_pool_rebuild
    {
        return;
    };
    let Some(mut feedback) = feedback else {
        return;
    };
    feedback.clear_frame();
    if let Some(mut pool) = pool {
        for pos in &staging.0.removals {
            if let Some(cache) = bind_cache.as_mut() {
                cache.invalidate(*pos);
            }
        }
        pool.apply_extract_batch(&staging.0, &mut feedback);
        if let Some(cache) = bind_cache.as_mut() {
            for pos in &feedback.evicted {
                cache.invalidate(*pos);
            }
        }
    } else if let Some(mut pending) = pending {
        pending.batches.push(staging.0.clone());
    }
}

/// Batches extracted before render GPU buffers are initialized.
#[derive(Resource, Default)]
pub struct PendingChunkExtract {
    pub batches: Vec<GpuChunkExtractBatch>,
}
