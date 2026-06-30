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
use crate::gpu_voxel::memory_config::VoxelMemoryConfig;
use crate::gpu_voxel::model_library::GpuMeshLibrary;
use crate::gpu_voxel::types::{
    build_bucket_regions, initial_bucket_capacity, BucketFinalizeMeta, BucketRegion,
    GpuBucketRegion, GpuChunkTableEntry, BUCKET_FINALIZE_INVALID, DrawIndexedIndirectArgs,
    GpuRenderLayer, VoxelInstance,
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
    pub chunks_rebuilt: usize,
    pub chunks_emitted: usize,
    pub chunks_compacted: usize,
    pub scratch_overflow_chunks: u32,
    pub scratch_overflow_instances: u32,
    pub scratch_arena_max_slot_capacity: u32,
    pub scratch_arena_total_instances: u32,
    pub global_overflow_buckets: u32,
    pub instance_buffer_bytes: u64,
    pub chunk_gpu_bytes: u64,
    pub scratch_arena_bytes: u64,
    pub scratch_bytes: u64,
    pub draw_arena_bytes: u64,
    pub total_voxel_bytes: u64,
    pub overflow_pending: bool,
    pub upload_failures: usize,
    pub evictions: usize,
    pub peak_instances: Vec<u32>,
    pub staggered_rebuild_pending: u32,
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
    pub overflow_counters_buffer: Option<Buffer>,
    pub overflow_readback_buffer: Option<Buffer>,
    pub counters_readback_buffer: Option<Buffer>,
    pub bucket_regions_buffer: Buffer,
    pub bucket_finalize_meta_buffer: Buffer,
    pub chunk_table_buffer: Buffer,
    pub indirect_buffers: [Buffer; 3],
    pub layer_bucket_ids: [Vec<u32>; 3],
    pub bucket_regions: Vec<BucketRegion>,
    pub max_bucket_id: u32,
    pub bucket_slot_count: u32,
}

/// Tracks per-bucket instance capacity and growth state.
#[derive(Resource)]
pub struct InstanceBufferPool {
    pub capacities: Vec<u32>,
    pub peak: Vec<u32>,
    pub generation: u32,
    pub max_draw_instances: u32,
}

impl InstanceBufferPool {
    pub fn new(bucket_slot_count: u32, config: &VoxelMemoryConfig) -> Self {
        let n = bucket_slot_count as usize;
        let capacities = scaled_bucket_capacities(bucket_slot_count, config);
        Self {
            capacities,
            peak: vec![0; n],
            generation: 0,
            max_draw_instances: config.max_draw_instances,
        }
    }

    pub fn total_instance_slots(&self) -> u64 {
        self.capacities.iter().map(|&c| c as u64).sum()
    }
}

/// Per-bucket draw arena capacities scaled to [`VoxelMemoryConfig::max_draw_instances`].
pub fn scaled_bucket_capacities(bucket_slot_count: u32, config: &VoxelMemoryConfig) -> Vec<u32> {
    let mut capacities: Vec<u32> = (0..bucket_slot_count)
        .map(|id| {
            initial_bucket_capacity(id, config.max_resident_chunks, config.instance_budget_per_chunk)
        })
        .collect();
    let raw_total: u64 = capacities.iter().map(|&c| c as u64).sum();
    let max_total = config.max_draw_instances as u64;
    if raw_total > max_total && raw_total > 0 {
        for cap in &mut capacities {
            let scaled = (*cap as u64 * max_total / raw_total) as u32;
            *cap = scaled.max(4) & !3;
        }
    }
    capacities
}

pub fn chunk_table_buffer_size(max_resident_chunks: u32) -> u64 {
    std::mem::size_of::<GpuChunkTableEntry>() as u64 * max_resident_chunks as u64
}

pub fn grow_chunk_table_buffer(
    render_device: &RenderDevice,
    existing: &Buffer,
    new_max_resident_chunks: u32,
) -> Buffer {
    let new_size = chunk_table_buffer_size(new_max_resident_chunks);
    if existing.size() >= new_size {
        return existing.clone();
    }
    render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_chunk_table_grown"),
        size: new_size,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub fn grow_instance_buffers(
    render_device: &RenderDevice,
    tables: &GpuVoxelTables,
    pool: &mut InstanceBufferPool,
    overflows: &[u32],
    existing: &RenderGpuVoxelBuffers,
) -> RenderGpuVoxelBuffers {
    for (id, &overflow) in overflows.iter().enumerate() {
        if overflow > 0 && id < pool.capacities.len() {
            let old = pool.capacities[id];
            let doubled = old.saturating_mul(2).max(old + 4);
            let cap = pool.max_draw_instances.min(doubled);
            pool.capacities[id] = if cap & 3 != 0 { (cap + 3) & !3 } else { cap };
        }
    }

    let bucket_slot_count = pool.capacities.len() as u32;
    let (bucket_regions, offset) = build_bucket_regions(bucket_slot_count, &pool.capacities);

    let instances_buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_instances_grown"),
        size: offset * std::mem::size_of::<VoxelInstance>() as u64,
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
        label: Some("gpu_bucket_regions_grown"),
        contents: bytemuck::cast_slice(&gpu_bucket_regions),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    let bucket_finalize_meta = build_bucket_finalize_meta(tables, &pool.capacities);
    let bucket_finalize_meta_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("gpu_bucket_finalize_meta_grown"),
        contents: bytemuck::cast_slice(&bucket_finalize_meta),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });

    RenderGpuVoxelBuffers {
        block_meta_buffer: existing.block_meta_buffer.clone(),
        atlas_rects_buffer: existing.atlas_rects_buffer.clone(),
        mesh_vertex_buffer: existing.mesh_vertex_buffer.clone(),
        mesh_index_buffer: existing.mesh_index_buffer.clone(),
        instances_buffer,
        counters_buffer: existing.counters_buffer.clone(),
        overflow_counters_buffer: existing.overflow_counters_buffer.clone(),
        overflow_readback_buffer: existing.overflow_readback_buffer.clone(),
        counters_readback_buffer: existing.counters_readback_buffer.clone(),
        bucket_regions_buffer,
        bucket_finalize_meta_buffer,
        chunk_table_buffer: existing.chunk_table_buffer.clone(),
        indirect_buffers: existing.indirect_buffers.clone(),
        layer_bucket_ids: existing.layer_bucket_ids.clone(),
        bucket_regions,
        max_bucket_id: existing.max_bucket_id,
        bucket_slot_count,
    }
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

fn build_bucket_finalize_meta(
    tables: &GpuVoxelTables,
    capacities: &[u32],
) -> Vec<BucketFinalizeMeta> {
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
            capacity: capacities.get(bucket.id as usize).copied().unwrap_or(4096),
            _pad: 0,
        };
    }
    meta
}

pub fn init_render_gpu_buffers(
    render_device: &RenderDevice,
    tables: &GpuVoxelTables,
    pool: &InstanceBufferPool,
    bucket_regions: &[BucketRegion],
    total_slots: u64,
    chunk_table_size: u64,
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
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let overflow_counters_buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_overflow_counters"),
        size: counters_buffer_size(bucket_slot_count),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let overflow_readback_buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_overflow_readback"),
        size: counters_buffer_size(bucket_slot_count),
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let counters_readback_buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_counters_readback"),
        size: counters_buffer_size(bucket_slot_count),
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let chunk_table_buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("gpu_chunk_table"),
        size: chunk_table_size,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bucket_finalize_meta = build_bucket_finalize_meta(tables, &pool.capacities);
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
        overflow_counters_buffer: Some(overflow_counters_buffer),
        overflow_readback_buffer: Some(overflow_readback_buffer),
        counters_readback_buffer: Some(counters_readback_buffer),
        bucket_regions_buffer,
        bucket_finalize_meta_buffer,
        chunk_table_buffer,
        indirect_buffers: [
            make_indirect(&opaque_args),
            make_indirect(&cutout_args),
            make_indirect(&blend_args),
        ],
        layer_bucket_ids: [opaque_ids, cutout_ids, blend_ids],
        bucket_regions: bucket_regions.to_vec(),
        max_bucket_id: tables.max_bucket_id,
        bucket_slot_count,
    }
}

pub fn init_render_gpu_buffers_with_config(
    render_device: &RenderDevice,
    tables: &GpuVoxelTables,
    config: &VoxelMemoryConfig,
) -> (RenderGpuVoxelBuffers, InstanceBufferPool) {
    let pool = InstanceBufferPool::new(tables.max_bucket_id + 1, config);
    let (bucket_regions, total_slots) =
        build_bucket_regions(tables.max_bucket_id + 1, &pool.capacities);
    let table_size = chunk_table_buffer_size(config.max_resident_chunks);
    let buffers = init_render_gpu_buffers(
        render_device,
        tables,
        &pool,
        &bucket_regions,
        total_slots,
        table_size,
    );
    (buffers, pool)
}

pub fn static_buffer_bytes(buffers: &RenderGpuVoxelBuffers) -> u64 {
    buffers.block_meta_buffer.size()
        + buffers.atlas_rects_buffer.size()
        + buffers.mesh_vertex_buffer.size()
        + buffers.mesh_index_buffer.size()
        + buffers.bucket_regions_buffer.size()
        + buffers.bucket_finalize_meta_buffer.size()
        + buffers.chunk_table_buffer.size()
        + buffers.counters_buffer.size()
        + buffers
            .overflow_counters_buffer
            .as_ref()
            .map(|b| b.size())
            .unwrap_or(0)
        + buffers.indirect_buffers.iter().map(|b| b.size()).sum::<u64>()
}
