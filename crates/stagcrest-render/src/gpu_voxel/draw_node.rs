use bevy::camera::{MainPassResolutionOverride, Viewport};
use bevy::prelude::*;
use bevy::render::{
    camera::ExtractedCamera,
    render_asset::RenderAssets,
    render_resource::{
        BindGroup, BindGroupEntry, BindingResource, Buffer, BufferInitDescriptor, BufferSize,
        BufferUsages, PipelineCache, RenderPassDescriptor, SpecializedRenderPipelines, StoreOp,
    },
    renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
    texture::GpuImage,
    view::{ExtractedView, Msaa, ViewDepthTexture, ViewTarget},
};
use std::collections::HashMap;

use crate::gpu_voxel::extract::camera_to_gpu_uniform;
use crate::gpu_voxel::gpu_resources::{GpuVoxelTables, InstanceBufferPool};
use crate::gpu_voxel::pipelines::{material_uniforms_from, VoxelDrawPipeline, VoxelDrawPipelineKey};
use crate::gpu_voxel::prepare::GpuVoxelRenderState;
use crate::gpu_voxel::types::{instance_slot_byte_size, GpuRenderLayer};

use crate::plugin::{VoxelAtlasImage, VoxelCamera, VoxelMaterialSource};

/// Cached specialized voxel draw pipelines keyed by view format, MSAA, and layer.
#[derive(Resource, Default)]
pub struct VoxelSpecializedPipelineCache {
    pub pipelines: HashMap<VoxelDrawPipelineKey, bevy::render::render_resource::CachedRenderPipelineId>,
}

/// Ensures voxel draw pipelines exist for each active view format.
pub fn prepare_voxel_draw_pipelines(
    views: Query<(&ExtractedView, &Msaa)>,
    draw_pipeline: Option<Res<VoxelDrawPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    mut specialized: ResMut<SpecializedRenderPipelines<VoxelDrawPipeline>>,
    mut cache: ResMut<VoxelSpecializedPipelineCache>,
) {
    let Some(draw_pipeline) = draw_pipeline else {
        return;
    };
    for (view, msaa) in views {
        for layer in 0..3u32 {
            let key = VoxelDrawPipelineKey {
                target_format: view.target_format,
                samples: msaa.samples(),
                layer,
            };
            cache.pipelines.entry(key).or_insert_with(|| {
                specialized.specialize(&pipeline_cache, &draw_pipeline, key)
            });
        }
    }
}

#[derive(Clone)]
struct LayerDraw {
    cache_key: (usize, u32),
    layer_idx: usize,
    indirect_byte_offset: u64,
}

/// Cached draw bind groups and uniform buffers; recreated when the instance buffer changes.
#[derive(Resource, Default)]
pub struct VoxelDrawBindCache {
    camera_buffer: Option<Buffer>,
    material_buffers: [Option<Buffer>; 2],
    bucket_binds: HashMap<(usize, u32), BindGroup>,
    instances_generation: u32,
    atlas_id: Option<AssetId<bevy::prelude::Image>>,
    prepared_draws: Vec<LayerDraw>,
}

impl VoxelDrawBindCache {
    pub fn invalidate(&mut self) {
        self.bucket_binds.clear();
    }

    pub fn reset_for_growth(&mut self, generation: u32) {
        self.instances_generation = generation;
        self.bucket_binds.clear();
    }
}

/// Updates cached bind groups and records draw list for the render graph node.
pub fn prepare_voxel_draw_binds(
    state: Option<Res<GpuVoxelRenderState>>,
    tables: Option<Res<GpuVoxelTables>>,
    camera: Option<Res<VoxelCamera>>,
    atlas_image: Option<Res<VoxelAtlasImage>>,
    material_source: Option<Res<VoxelMaterialSource>>,
    draw_pipeline: Option<Res<VoxelDrawPipeline>>,
    pool: Option<Res<InstanceBufferPool>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut bind_cache: ResMut<VoxelDrawBindCache>,
) {
    let (Some(state), Some(tables), Some(camera), Some(atlas_image), Some(draw_pipeline)) =
        (state, tables, camera, atlas_image, draw_pipeline)
    else {
        tracing::debug!("voxel draw prep skipped: missing required resources");
        bind_cache.prepared_draws.clear();
        return;
    };
    let Some(gpu_image) = gpu_images.get(&atlas_image.0) else {
        bind_cache.prepared_draws.clear();
        return;
    };

    let pool_generation = pool.map(|p| p.generation).unwrap_or(0);
    if bind_cache.instances_generation != pool_generation
        || bind_cache.atlas_id != Some(atlas_image.0.id())
    {
        bind_cache.invalidate();
        bind_cache.instances_generation = pool_generation;
        bind_cache.atlas_id = Some(atlas_image.0.id());
    }

    if bind_cache.camera_buffer.is_none() {
        let cam_uniform = camera_to_gpu_uniform(&camera);
        bind_cache.camera_buffer = Some(render_device.create_buffer_with_data(
            &BufferInitDescriptor {
                label: Some("voxel_draw_camera"),
                contents: bytemuck::bytes_of(&cam_uniform),
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            },
        ));
    }
    if let Some(buf) = &bind_cache.camera_buffer {
        let cam_uniform = camera_to_gpu_uniform(&camera);
        render_queue.write_buffer(buf, 0, bytemuck::bytes_of(&cam_uniform));
    }

    for (idx, cutout) in [(0, false), (1, true)] {
        if bind_cache.material_buffers[idx].is_none() {
            bind_cache.material_buffers[idx] = Some(material_buffer_for_layer(
                &render_device,
                material_source.as_deref(),
                cutout,
            ));
        }
        if let Some(buf) = &bind_cache.material_buffers[idx] {
            let material_uniform =
                material_uniforms_from(material_source.as_deref(), cutout);
            render_queue.write_buffer(buf, 0, bytemuck::bytes_of(&material_uniform));
        }
    }

    let camera_buffer = bind_cache.camera_buffer.clone().expect("camera buffer");
    let material_opaque = bind_cache.material_buffers[0].clone().expect("material buffer");
    let material_cutout = bind_cache.material_buffers[1].clone().expect("material buffer");
    let indirect_stride =
        std::mem::size_of::<crate::gpu_voxel::types::DrawIndexedIndirectArgs>() as u64;

    bind_cache.prepared_draws.clear();
    for (layer_idx, layer) in [
        GpuRenderLayer::Opaque,
        GpuRenderLayer::Cutout,
        GpuRenderLayer::Blend,
    ]
    .into_iter()
    .enumerate()
    {
        let cutout = layer == GpuRenderLayer::Cutout;
        let material_buffer = if cutout {
            &material_cutout
        } else {
            &material_opaque
        };

        let layer_draws = collect_layer_draws(
            &render_device,
            &draw_pipeline,
            &state.buffers,
            &mut bind_cache,
            &camera_buffer,
            material_buffer,
            &gpu_image.texture_view,
            &tables,
            layer_idx,
            indirect_stride,
        );
        bind_cache.prepared_draws.extend(layer_draws);
    }
}

pub fn voxel_draw_pass(
    world: &World,
    view: ViewQuery<(
        &ExtractedCamera,
        &ViewTarget,
        &ViewDepthTexture,
        &Msaa,
        &ExtractedView,
        Option<&MainPassResolutionOverride>,
    )>,
    mut ctx: RenderContext,
) {
    let (camera, target, depth, msaa, extracted_view, resolution_override) = view.into_inner();

    let Some(state) = world.get_resource::<GpuVoxelRenderState>() else {
        return;
    };
    if world.get_resource::<VoxelDrawPipeline>().is_none() {
        return;
    };
    let bind_cache = world.resource::<VoxelDrawBindCache>();
    if bind_cache.prepared_draws.is_empty() {
        return;
    }

    let pipeline_cache = world.resource::<PipelineCache>();
    let pipeline_cache_resource = world.resource::<VoxelSpecializedPipelineCache>();
    let target_format = extracted_view.target_format;

    let color_attachment = target.get_color_attachment();
    let depth_attachment = depth.get_attachment(StoreOp::Store);
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("voxel_draw_pass"),
        color_attachments: &[Some(color_attachment)],
        depth_stencil_attachment: Some(depth_attachment),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });

    if let Some(viewport) =
        Viewport::from_viewport_and_override(camera.viewport.as_ref(), resolution_override)
    {
        pass.set_camera_viewport(&viewport);
    }

    pass.set_vertex_buffer(0, state.buffers.mesh_vertex_buffer.slice(..));
    pass.set_index_buffer(
        state.buffers.mesh_index_buffer.slice(..),
        bevy::render::render_resource::IndexFormat::Uint32,
    );

    for draw in &bind_cache.prepared_draws {
        let key = VoxelDrawPipelineKey {
            target_format,
            samples: msaa.samples(),
            layer: draw.layer_idx as u32,
        };
        let Some(pipeline_id) = pipeline_cache_resource.pipelines.get(&key) else {
            continue;
        };
        let Some(pipeline) = pipeline_cache.get_render_pipeline(*pipeline_id) else {
            continue;
        };
        let Some(bind_group) = bind_cache.bucket_binds.get(&draw.cache_key) else {
            continue;
        };
        pass.set_render_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw_indexed_indirect(
            &state.buffers.indirect_buffers[draw.layer_idx],
            draw.indirect_byte_offset,
        );
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
    bind_cache: &mut VoxelDrawBindCache,
    camera_buffer: &Buffer,
    material_buffer: &Buffer,
    texture_view: &bevy::render::render_resource::TextureView,
    tables: &GpuVoxelTables,
    layer_idx: usize,
    indirect_stride: u64,
) -> Vec<LayerDraw> {
    let mut draws = Vec::new();

    for (indirect_idx, bucket_id) in buffers.layer_bucket_ids[layer_idx]
        .iter()
        .enumerate()
    {
        let Some(bucket_idx) = tables.buckets.bucket_index_by_id.get(*bucket_id as usize).copied().flatten() else {
            continue;
        };
        let bucket = &tables.buckets.buckets[bucket_idx];
        if bucket.index_count == 0 {
            continue;
        }

        let region = buffers
            .bucket_regions
            .get(*bucket_id as usize)
            .copied()
            .unwrap_or_default();
        let slot_bytes = instance_slot_byte_size();
        let region_byte_offset = region.offset as u64 * slot_bytes;
        let bucket_binding_size =
            BufferSize::new(region.capacity as u64 * slot_bytes).expect("bucket region size is non-zero");

        let cache_key = (layer_idx, *bucket_id);
        bind_cache.bucket_binds.entry(cache_key).or_insert_with(|| {
            render_device.create_bind_group(
                "voxel_draw_bind_group",
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
                            offset: region_byte_offset,
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
                    BindGroupEntry {
                        binding: 6,
                        resource: buffers.block_meta_buffer.as_entire_binding(),
                    },
                ],
            )
        });

        draws.push(LayerDraw {
            cache_key,
            layer_idx,
            indirect_byte_offset: indirect_idx as u64 * indirect_stride,
        });
    }
    draws
}
