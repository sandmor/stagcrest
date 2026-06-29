use bevy::prelude::*;
use bevy::render::{
    extract_resource::ExtractResourcePlugin,
    render_graph::RenderGraphApp,
    ExtractSchedule, Render, RenderApp,
};
use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};

pub mod block_meta;
pub mod bucket;
pub mod chunk_cache;
pub mod chunk_data;
pub mod chunk_feedback;
pub mod draw_node;
pub mod extract;
pub mod extract_chunk;
pub mod gpu_resources;
pub mod model_library;
pub mod pipelines;
pub mod prepare;
pub mod rebuild_node;
pub mod render_chunk_store;
pub mod types;

pub use chunk_cache::GpuChunkCache;
pub use chunk_data::{capture_gpu_chunk_upload, pack_gpu_chunk_upload, pack_halo_from_world};
pub use chunk_feedback::{GpuChunkRenderFeedback, GpuChunkSyncState};
pub use gpu_resources::{GpuVoxelStats, GpuVoxelTables};
pub use pipelines::load_voxel_shaders;

use crate::gpu_voxel::draw_node::{prepare_voxel_draw_binds, VoxelDrawBindCache, VoxelDrawLabel, VoxelDrawNode};
use crate::gpu_voxel::extract_chunk::{apply_extracted_chunks, stage_gpu_chunk_extract, GpuChunkExtractStaging, PendingChunkExtract};
use crate::gpu_voxel::pipelines::prepare_render_pipelines;
use crate::gpu_voxel::prepare::{
    poll_gpu_voxel_overflow, prepare_gpu_voxel_buffers, publish_gpu_chunk_feedback,
    sync_gpu_voxel_stats,
};
use crate::gpu_voxel::rebuild_node::{
    prepare_voxel_rebuild, VoxelRebuildFrame, VoxelRebuildLabel, VoxelRebuildNode,
    VoxelRebuildPersistent,
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
            .add_systems(PostUpdate, stage_gpu_chunk_extract)
            .add_plugins((
                ExtractResourcePlugin::<GpuChunkExtractStaging>::default(),
                ExtractResourcePlugin::<GpuVoxelTables>::default(),
                ExtractResourcePlugin::<GpuVoxelStats>::default(),
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
                    poll_gpu_voxel_overflow.after(prepare_gpu_voxel_buffers),
                    prepare_voxel_rebuild.after(poll_gpu_voxel_overflow),
                    prepare_voxel_draw_binds.after(prepare_voxel_rebuild),
                    sync_gpu_voxel_stats,
                )
                    .chain(),
            )
            .add_render_graph_node::<VoxelRebuildNode>(Core3d, VoxelRebuildLabel)
            .add_render_graph_node::<bevy::render::render_graph::ViewNodeRunner<VoxelDrawNode>>(
                Core3d,
                VoxelDrawLabel,
            )
            .add_render_graph_edges(
                Core3d,
                (
                    Node3d::StartMainPass,
                    VoxelRebuildLabel,
                    VoxelDrawLabel,
                    Node3d::MainOpaquePass,
                ),
            );
    }
}
