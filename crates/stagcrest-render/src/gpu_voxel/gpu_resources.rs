use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_resource::{
    Buffer, BufferDescriptor, BufferInitDescriptor, BufferUsages,
};
use bevy::render::renderer::RenderDevice;
use stagcrest_mod_client::{BlockRegistry, ModelRegistry};
use std::sync::Arc;

use crate::gpu_voxel::block_meta::GpuBlockTables;
use crate::gpu_voxel::bucket::DrawBucketRegistry;
use crate::gpu_voxel::model_library::GpuMeshLibrary;
use crate::gpu_voxel::types::{
    bucket_capacity_slots, build_bucket_regions, BucketFinalizeMeta, BucketRegion,
    GpuBucketRegion, BUCKET_FINALIZE_INVALID, DrawIndexedIndirectArgs, GpuRenderLayer,
    VoxelInstance,
};

/// CPU-side voxel GPU tables built at content load.
#[derive(Resource, Clone, ExtractResource)]
pub struct GpuVoxelTables {
    pub block_tables: Arc<GpuBlockTables>,
    pub buckets: Arc<DrawBucketRegistry>,
    pub mesh_library: Arc<GpuMeshLibrary>,
    pub bucket_count: usize,
    pub max_bucket_id: u32,
}

impl GpuVoxelTables {
    pub fn build(registry: &BlockRegistry, models: &ModelRegistry) -> Self {
        let mut buckets = DrawBucketRegistry::build(models);
        let mesh_library = GpuMeshLibrary::build(registry, models, &mut buckets);
        let block_tables = GpuBlockTables::build(registry, models, &buckets);
        let bucket_count = buckets.buckets.len();
        let max_bucket_id = buckets
            .buckets
            .iter()
            .map(|b| b.id)
            .max()
            .unwrap_or(0);
        Self {
            block_tables: Arc::new(block_tables),
            buckets: Arc::new(buckets),
            mesh_library: Arc::new(mesh_library),
            bucket_count,
            max_bucket_id,
        }
    }
}

/// Per-frame GPU stats for debug overlay.
#[derive(Resource, Default, Clone, ExtractResource)]
pub struct GpuVoxelStats {
    pub chunk_count: usize,
    pub instance_counts: Vec<u32>,
    pub total_instances: u32,
}

/// Render-world GPU buffers for voxel pipeline.
#[derive(Resource)]
pub struct RenderGpuVoxelBuffers {
    pub block_meta_buffer: Buffer,
    pub atlas_rects_buffer: Buffer,
    pub mesh_vertex_buffer: Buffer,
    pub mesh_index_buffer: Buffer,
    pub instances_buffer: Buffer,
    pub counters_buffer: Buffer,
    pub bucket_regions_buffer: Buffer,
    pub bucket_finalize_meta_buffer: Buffer,
    pub indirect_buffers: [Buffer; 3],
    pub layer_bucket_ids: [Vec<u32>; 3],
    pub bucket_regions: Vec<BucketRegion>,
    pub max_bucket_id: u32,
    pub bucket_slot_count: u32,
}

pub fn counters_buffer_size(bucket_slots: u32) -> u64 {
    bucket_slots as u64 * std::mem::size_of::<u32>() as u64
}

fn build_layer_indirect(
    tables: &GpuVoxelTables,
    layer: GpuRenderLayer,
) -> (Vec<DrawIndexedIndirectArgs>, Vec<u32>) {
    let mut args = Vec::new();
    let mut bucket_ids = Vec::new();
    for bucket in tables.buckets.buckets.iter().filter(|b| b.layer == layer) {
        bucket_ids.push(bucket.id);
        args.push(DrawIndexedIndirectArgs {
            index_count: bucket.index_count,
            instance_count: 0,
            first_index: bucket.first_index,
            base_vertex: bucket.base_vertex as i32,
            first_instance: 0,
        });
    }
    (args, bucket_ids)
}

fn build_bucket_finalize_meta(tables: &GpuVoxelTables) -> Vec<BucketFinalizeMeta> {
    let slot_count = tables.max_bucket_id + 1;
    let mut meta = vec![
        BucketFinalizeMeta {
            layer: 0,
            indirect_index: BUCKET_FINALIZE_INVALID,
            capacity: 0,
            _pad: 0,
        };
        slot_count as usize
    ];

    let mut layer_indices = [0u32; 3];
    for bucket in &tables.buckets.buckets {
        let layer_idx = bucket.layer as usize;
        let idx = layer_indices[layer_idx];
        layer_indices[layer_idx] += 1;
        meta[bucket.id as usize] = BucketFinalizeMeta {
            layer: bucket.layer as u32,
            indirect_index: idx,
            capacity: bucket_capacity_slots(bucket.id),
            _pad: 0,
        };
    }
    meta
}

pub fn init_render_gpu_buffers(
    render_device: &RenderDevice,
    tables: &GpuVoxelTables,
) -> RenderGpuVoxelBuffers {
    let block_meta_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gpu_block_meta"),
        contents: bytemuck::cast_slice(&tables.block_tables.block_meta),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    let atlas_rects_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gpu_atlas_rects"),
        contents: bytemuck::cast_slice(&tables.block_tables.atlas_rects),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    let mesh_vertex_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gpu_mesh_vertices"),
        contents: bytemuck::cast_slice(&tables.mesh_library.vertices),
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
    });

    let mesh_index_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gpu_mesh_indices"),
        contents: bytemuck::cast_slice(&tables.mesh_library.indices),
        usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
    });

    let bucket_slot_count = tables.max_bucket_id + 1;

    let (bucket_regions, total_slots) = build_bucket_regions(bucket_slot_count);
    let instances_buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_instances"),
        size: total_slots * std::mem::size_of::<VoxelInstance>() as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let gpu_bucket_regions: Vec<GpuBucketRegion> = bucket_regions
        .iter()
        .map(|r| GpuBucketRegion {
            offset: r.offset,
            capacity: r.capacity,
        })
        .collect();
    let bucket_regions_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gpu_bucket_regions"),
        contents: bytemuck::cast_slice(&gpu_bucket_regions),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    let counters_buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_counters"),
        size: counters_buffer_size(bucket_slot_count),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bucket_finalize_meta = build_bucket_finalize_meta(tables);
    let bucket_finalize_meta_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gpu_bucket_finalize_meta"),
        contents: bytemuck::cast_slice(&bucket_finalize_meta),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    let (opaque_args, opaque_ids) = build_layer_indirect(tables, GpuRenderLayer::Opaque);
    let (cutout_args, cutout_ids) = build_layer_indirect(tables, GpuRenderLayer::Cutout);
    let (blend_args, blend_ids) = build_layer_indirect(tables, GpuRenderLayer::Blend);

    let make_indirect = |args: &[DrawIndexedIndirectArgs]| {
        render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("gpu_indirect_args"),
            contents: bytemuck::cast_slice(args),
            usage: BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
        })
    };

    RenderGpuVoxelBuffers {
        block_meta_buffer,
        atlas_rects_buffer,
        mesh_vertex_buffer,
        mesh_index_buffer,
        instances_buffer,
        counters_buffer,
        bucket_regions_buffer,
        bucket_finalize_meta_buffer,
        indirect_buffers: [
            make_indirect(&opaque_args),
            make_indirect(&cutout_args),
            make_indirect(&blend_args),
        ],
        layer_bucket_ids: [opaque_ids, cutout_ids, blend_ids],
        bucket_regions,
        max_bucket_id: tables.max_bucket_id,
        bucket_slot_count,
    }
}
