use bevy::prelude::*;
use bevy::render::{
    extract_resource::ExtractResourcePlugin,
    render_graph::RenderGraphApp,
    Render, RenderApp,
};
use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};

pub mod block_meta;
pub mod bucket;
pub mod chunk_cache;
pub mod chunk_data;
pub mod compute_node;
pub mod draw_node;
pub mod extract;
pub mod gpu_resources;
pub mod model_library;
pub mod pipelines;
pub mod prepare;
pub mod types;

pub use chunk_cache::GpuChunkCache;
pub use chunk_data::{capture_gpu_chunk_upload, pack_gpu_chunk_upload};
pub use gpu_resources::{GpuVoxelStats, GpuVoxelTables};
pub use pipelines::load_voxel_shaders;

use crate::gpu_voxel::compute_node::run_voxel_compute;
use crate::gpu_voxel::draw_node::{VoxelDrawLabel, VoxelDrawNode};
use crate::gpu_voxel::pipelines::prepare_render_pipelines;
use crate::gpu_voxel::prepare::{prepare_gpu_voxel_buffers, sync_gpu_voxel_stats};
use crate::plugin::{VoxelAtlasImage, VoxelCamera, VoxelMaterialSource};

pub struct GpuVoxelPlugin;

impl Plugin for GpuVoxelPlugin {
    fn build(&self, app: &mut App) {
        load_voxel_shaders(app);

        app.init_resource::<GpuChunkCache>()
            .init_resource::<GpuVoxelStats>()
            .add_plugins((
                ExtractResourcePlugin::<GpuChunkCache>::default(),
                ExtractResourcePlugin::<GpuVoxelTables>::default(),
                ExtractResourcePlugin::<GpuVoxelStats>::default(),
                ExtractResourcePlugin::<VoxelCamera>::default(),
                ExtractResourcePlugin::<VoxelAtlasImage>::default(),
                ExtractResourcePlugin::<VoxelMaterialSource>::default(),
            ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<crate::gpu_voxel::prepare::ChunkGpuPoolResource>()
            .add_systems(
                Render,
                (
                    prepare_render_pipelines,
                    prepare_gpu_voxel_buffers,
                    run_voxel_compute.after(prepare_gpu_voxel_buffers),
                    sync_gpu_voxel_stats,
                )
                    .chain(),
            )
            .add_render_graph_node::<bevy::render::render_graph::ViewNodeRunner<VoxelDrawNode>>(
                Core3d,
                VoxelDrawLabel,
            )
            .add_render_graph_edges(
                Core3d,
                (
                    Node3d::StartMainPass,
                    VoxelDrawLabel,
                    Node3d::MainOpaquePass,
                ),
            );
    }
}
