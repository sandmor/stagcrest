use bevy::prelude::*;
use bevy::core_pipeline::{
    core_3d::main_opaque_pass_3d,
    schedule::{Core3d, Core3dSystems},
};
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::render_resource::SpecializedRenderPipelines;
use bevy::render::{ExtractSchedule, Render, RenderApp, RenderSystems};

pub mod block_meta;
pub mod bucket;
pub mod chunk_cache;
pub mod chunk_data;
pub mod chunk_feedback;
pub mod chunk_gpu_pool;
pub mod draw_node;
pub mod extract;
pub mod extract_chunk;
pub mod gpu_resources;
pub mod memory_config;
pub mod model_library;
pub mod pipelines;
pub mod prepare;
pub mod chunk_scratch_arena;
pub mod rebuild_node;
pub mod types;

pub use chunk_cache::GpuChunkCache;
pub use chunk_data::{capture_gpu_chunk_upload, pack_gpu_chunk_upload, pack_halo_from_world};
pub use chunk_feedback::{GpuChunkRenderFeedback, GpuChunkSyncState};
pub use gpu_resources::{GpuVoxelStats, GpuVoxelTables};
pub use memory_config::VoxelMemoryConfig;
pub use pipelines::load_voxel_shaders;

use crate::gpu_voxel::draw_node::{prepare_voxel_draw_binds, prepare_voxel_draw_pipelines, voxel_draw_pass, VoxelDrawBindCache, VoxelSpecializedPipelineCache};
use crate::gpu_voxel::extract_chunk::{apply_extracted_chunks, stage_gpu_chunk_extract, GpuChunkExtractStaging, PendingChunkExtract};
use crate::gpu_voxel::pipelines::prepare_render_pipelines;
use crate::gpu_voxel::pipelines::VoxelDrawPipeline;
use crate::gpu_voxel::prepare::{
    poll_gpu_voxel_overflow, prepare_gpu_voxel_buffers, publish_gpu_chunk_feedback,
    resize_voxel_memory_if_needed, sync_gpu_voxel_stats,
};
use crate::gpu_voxel::rebuild_node::{
    prepare_voxel_rebuild, voxel_rebuild_pass, VoxelRebuildFrame, VoxelRebuildPersistent,
};
use crate::plugin::{VoxelAtlasImage, VoxelCamera, VoxelMaterialSource, VoxelRenderConfig as VoxelRenderConfigResource};

pub struct GpuVoxelPlugin;

impl Plugin for GpuVoxelPlugin {
    fn build(&self, app: &mut App) {
        load_voxel_shaders(app);

        app.init_resource::<GpuChunkCache>()
            .init_resource::<GpuChunkSyncState>()
            .init_resource::<GpuChunkExtractStaging>()
            .init_resource::<GpuVoxelStats>()
            .init_resource::<VoxelMemoryConfig>()
            .add_systems(PostUpdate, stage_gpu_chunk_extract)
            .add_plugins((
                ExtractResourcePlugin::<GpuChunkExtractStaging>::default(),
                ExtractResourcePlugin::<GpuVoxelTables>::default(),
                ExtractResourcePlugin::<GpuVoxelStats>::default(),
                ExtractResourcePlugin::<VoxelMemoryConfig>::default(),
                ExtractResourcePlugin::<VoxelCamera>::default(),
                ExtractResourcePlugin::<VoxelRenderConfigResource>::default(),
                ExtractResourcePlugin::<VoxelAtlasImage>::default(),
                ExtractResourcePlugin::<VoxelMaterialSource>::default(),
            ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<VoxelDrawBindCache>()
            .init_resource::<VoxelRebuildFrame>()
            .init_resource::<VoxelRebuildPersistent>()
            .init_resource::<PendingChunkExtract>()
            .init_resource::<GpuChunkRenderFeedback>()
            .init_resource::<VoxelSpecializedPipelineCache>()
            .init_resource::<SpecializedRenderPipelines<VoxelDrawPipeline>>()
            .add_systems(
                ExtractSchedule,
                (
                    apply_extracted_chunks,
                    publish_gpu_chunk_feedback.after(apply_extracted_chunks),
                )
                    .chain(),
            )
            .add_systems(
                Render,
                (
                    prepare_render_pipelines,
                    prepare_gpu_voxel_buffers,
                    resize_voxel_memory_if_needed.after(prepare_gpu_voxel_buffers),
                    poll_gpu_voxel_overflow.after(resize_voxel_memory_if_needed),
                    prepare_voxel_rebuild.after(poll_gpu_voxel_overflow),
                    prepare_voxel_draw_binds.after(prepare_voxel_rebuild),
                    prepare_voxel_draw_pipelines.after(prepare_voxel_draw_binds),
                    sync_gpu_voxel_stats,
                )
                    .chain()
                    .in_set(RenderSystems::Prepare),
            )
            .add_systems(
                Core3d,
                (
                    voxel_rebuild_pass,
                    voxel_draw_pass.after(voxel_rebuild_pass).before(main_opaque_pass_3d),
                )
                    .chain()
                    .in_set(Core3dSystems::MainPass),
            );
    }
}
