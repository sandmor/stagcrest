use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::{
    render_asset::RenderAssets,
    render_graph::{NodeRunError, RenderGraphContext, RenderLabel, ViewNode},
    render_resource::{
        BindGroup, BindGroupEntry, BindingResource, Buffer, BufferInitDescriptor, BufferSize,
        BufferUsages, PipelineCache, RenderPassDescriptor, StoreOp,
    },
    renderer::{RenderContext, RenderDevice},
    texture::GpuImage,
    view::{Msaa, ViewDepthTexture, ViewTarget},
};

use crate::gpu_voxel::extract::camera_to_gpu_uniform;
use crate::gpu_voxel::gpu_resources::GpuVoxelTables;
use crate::gpu_voxel::pipelines::{material_uniforms_from, msaa_pipeline_index, VoxelDrawPipeline};
use crate::gpu_voxel::prepare::GpuVoxelRenderState;
use crate::gpu_voxel::types::{
    instance_slot_byte_size, GpuRenderLayer,
};
use crate::plugin::{VoxelAtlasImage, VoxelCamera, VoxelMaterialSource};

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct VoxelDrawLabel;

#[derive(Default)]
pub struct VoxelDrawNode;

struct LayerDraw {
    bind_group: BindGroup,
    dynamic_offset: u32,
    indirect_byte_offset: u64,
}

impl ViewNode for VoxelDrawNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static ViewDepthTexture,
        &'static Msaa,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (target, depth, msaa): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let Some(state) = world.get_resource::<GpuVoxelRenderState>() else {
            return Ok(());
        };
        let Some(tables) = world.get_resource::<GpuVoxelTables>() else {
            return Ok(());
        };
        let Some(camera) = world.get_resource::<VoxelCamera>() else {
            return Ok(());
        };
        let Some(atlas_image) = world.get_resource::<VoxelAtlasImage>() else {
            return Ok(());
        };
        let gpu_images = world.resource::<RenderAssets<GpuImage>>();
        let Some(gpu_image) = gpu_images.get(&atlas_image.0) else {
            return Ok(());
        };

        let pipeline_cache = world.resource::<PipelineCache>();
        let render_device = world.resource::<RenderDevice>();

        let Some(draw_pipeline) = world.get_resource::<VoxelDrawPipeline>() else {
            return Ok(());
        };

        let cam_uniform = camera_to_gpu_uniform(camera);
        let camera_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("voxel_draw_camera"),
            contents: bytemuck::bytes_of(&cam_uniform),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let material_source = world.get_resource::<VoxelMaterialSource>();

        let indirect_stride = std::mem::size_of::<crate::gpu_voxel::types::DrawIndexedIndirectArgs>()
            as u64;

        let msaa_pipelines = &draw_pipeline.pipelines[msaa_pipeline_index(msaa.samples())];

        // Phase 1: build all draws across every layer up front (bind groups must
        // outlive the render pass). wgpu keeps referenced buffers alive internally,
        // so the temporary material buffers can drop after bind group creation.
        let mut all_draws: Vec<(usize, bevy::render::render_resource::CachedRenderPipelineId, LayerDraw)> =
            Vec::new();
        for (layer_idx, layer) in [
            GpuRenderLayer::Opaque,
            GpuRenderLayer::Cutout,
            GpuRenderLayer::Blend,
        ]
        .into_iter()
        .enumerate()
        {
            let pipeline_id = msaa_pipelines[layer_idx];
            if pipeline_cache.get_render_pipeline(pipeline_id).is_none() {
                continue;
            }

            let cutout = layer == GpuRenderLayer::Cutout;
            let material_buffer =
                material_buffer_for_layer(render_device, material_source, cutout);

            let layer_draws = collect_layer_draws(
                render_device,
                draw_pipeline,
                &state.buffers,
                &camera_buffer,
                &material_buffer,
                &gpu_image.texture_view,
                tables,
                layer,
                indirect_stride,
            );
            for draw in layer_draws {
                all_draws.push((layer_idx, pipeline_id, draw));
            }
        }

        if all_draws.is_empty() {
            return Ok(());
        }

        // Phase 2: a single render pass per frame. Using Bevy's attachment helpers
        // gives the correct per-frame clear (first access) / load semantics, so the
        // color and depth buffers are cleared once per frame instead of never.
        let color_attachment = target.get_color_attachment();
        let depth_attachment = depth.get_attachment(StoreOp::Store);
        let mut pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("voxel_draw_pass"),
            color_attachments: &[Some(color_attachment)],
            depth_stencil_attachment: Some(depth_attachment),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_vertex_buffer(0, state.buffers.mesh_vertex_buffer.slice(..));
        pass.set_index_buffer(
            state.buffers.mesh_index_buffer.slice(..),
            0,
            bevy::render::render_resource::IndexFormat::Uint32,
        );

        for (layer_idx, pipeline_id, draw) in &all_draws {
            let Some(pipeline) = pipeline_cache.get_render_pipeline(*pipeline_id) else {
                continue;
            };
            pass.set_render_pipeline(pipeline);
            pass.set_bind_group(0, &draw.bind_group, &[draw.dynamic_offset]);
            pass.draw_indexed_indirect(
                &state.buffers.indirect_buffers[*layer_idx],
                draw.indirect_byte_offset,
            );
        }

        Ok(())
    }
}

fn material_buffer_for_layer(
    render_device: &RenderDevice,
    material_source: Option<&VoxelMaterialSource>,
    cutout: bool,
) -> Buffer {
    let material_uniform = material_uniforms_from(material_source, cutout);
    render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("voxel_draw_material"),
        contents: bytemuck::bytes_of(&material_uniform),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    })
}

fn collect_layer_draws(
    render_device: &RenderDevice,
    draw_pipeline: &VoxelDrawPipeline,
    buffers: &crate::gpu_voxel::gpu_resources::RenderGpuVoxelBuffers,
    camera_buffer: &Buffer,
    material_buffer: &Buffer,
    texture_view: &bevy::render::render_resource::TextureView,
    tables: &GpuVoxelTables,
    layer: GpuRenderLayer,
    indirect_stride: u64,
) -> Vec<LayerDraw> {
    let layer_idx = layer as usize;
    let mut draws = Vec::new();

    for (indirect_idx, bucket_id) in buffers.layer_bucket_ids[layer_idx]
        .iter()
        .enumerate()
    {
        let Some(bucket) = tables.buckets.buckets.iter().find(|b| b.id == *bucket_id) else {
            continue;
        };
        if bucket.index_count == 0 {
            continue;
        }

        let region = buffers
            .bucket_regions
            .get(*bucket_id as usize)
            .copied()
            .unwrap_or_default();
        let slot_bytes = instance_slot_byte_size();
        let dynamic_offset = (region.offset as u64 * slot_bytes) as u32;
        let bucket_binding_size =
            BufferSize::new(region.capacity as u64 * slot_bytes).expect("bucket region size is non-zero");
        let bind_group = render_device.create_bind_group(
            Some("voxel_draw_bind_group"),
            &draw_pipeline.layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Buffer(bevy::render::render_resource::BufferBinding {
                        buffer: &buffers.instances_buffer,
                        offset: 0,
                        size: Some(bucket_binding_size),
                    }),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: buffers.atlas_rects_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::TextureView(texture_view),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::Sampler(&draw_pipeline.sampler),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: material_buffer.as_entire_binding(),
                },
            ],
        );
        draws.push(LayerDraw {
            bind_group,
            dynamic_offset,
            indirect_byte_offset: indirect_idx as u64 * indirect_stride,
        });
    }
    draws
}
