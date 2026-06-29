use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;

use crate::gpu_voxel::chunk_cache::GpuChunkCache;
use crate::gpu_voxel::compute_node::ChunkGpuPool;
use crate::gpu_voxel::gpu_resources::{GpuVoxelStats, GpuVoxelTables};
use crate::gpu_voxel::gpu_resources::{init_render_gpu_buffers, RenderGpuVoxelBuffers};

#[derive(Resource)]
pub struct GpuVoxelRenderState {
    pub buffers: RenderGpuVoxelBuffers,
}

#[derive(Resource, Default)]
pub struct ChunkGpuPoolResource(pub ChunkGpuPool);

pub fn prepare_gpu_voxel_buffers(
    tables: Option<Res<GpuVoxelTables>>,
    render_device: Res<RenderDevice>,
    existing: Option<Res<GpuVoxelRenderState>>,
    mut commands: Commands,
) {
    if existing.is_some() {
        return;
    }
    let Some(tables) = tables else {
        return;
    };
    let buffers = init_render_gpu_buffers(&render_device, tables.as_ref());
    commands.insert_resource(GpuVoxelRenderState { buffers });
}

pub fn sync_gpu_voxel_stats(
    chunk_cache: Option<Res<GpuChunkCache>>,
    mut stats: ResMut<GpuVoxelStats>,
) {
    let Some(chunk_cache) = chunk_cache else {
        return;
    };
    stats.chunk_count = chunk_cache.chunk_count();
}
