use bevy::prelude::*;
use bevy::render::render_resource::{
    BindGroup, BindGroupEntry, Buffer, BufferDescriptor, BufferInitDescriptor, BufferUsages,
    ComputePassDescriptor, PipelineCache,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use stagcrest_protocol::ChunkPos;
use std::collections::{HashMap, HashSet};

use crate::gpu_voxel::chunk_gpu_pool::ChunkGpuPool;
use crate::gpu_voxel::chunk_scratch_arena::{ChunkScratchArena, ScratchFlushResult};
use crate::gpu_voxel::gpu_resources::{static_buffer_bytes, GpuVoxelStats, InstanceBufferPool};
use crate::gpu_voxel::memory_config::VoxelMemoryConfig;
use crate::gpu_voxel::pipelines::{VoxelCompactPipeline, VoxelComputePipeline, VoxelFinalizePipeline};
use crate::gpu_voxel::prepare::GpuVoxelRenderState;
use crate::gpu_voxel::types::{CompactUniform, EmitUniform, FinalizeUniform};

/// Must match `EMIT_TILES_PER_AXIS` in `voxel_compute.wgsl`.
const EMIT_TILES_PER_AXIS: u32 = 4;

/// Dirty-chunk emit job (compact is batched separately).
pub struct ChunkEmitJob {
    pub emit_bind_group: BindGroup,
}

/// Per-frame GPU rebuild state prepared on the CPU before the render graph node runs.
#[derive(Resource, Default)]
pub struct VoxelRebuildFrame {
    pub skip: bool,
    pub emit_jobs: Vec<ChunkEmitJob>,
    pub visible: Vec<u32>,
    pub compact_bind_group: Option<BindGroup>,
    pub finalize_bind_group: Option<BindGroup>,
    pub emit_uniform_buffer: Option<Buffer>,
    pub compact_uniform_buffer: Option<Buffer>,
    pub finalize_uniform_buffer: Option<Buffer>,
    pub chunks_emitted: usize,
    pub chunks_compacted: usize,
    pub chunk_count: usize,
    pub scratch_max_capacity: u32,
}

/// Reused GPU buffers for rebuild passes.
#[derive(Resource, Default)]
pub struct VoxelRebuildPersistent {
    visible_buffer: Option<Buffer>,
    visible_capacity: usize,
    emit_uniform_buffer: Option<Buffer>,
    compact_uniform_buffer: Option<Buffer>,
    finalize_uniform_buffer: Option<Buffer>,
}

struct CachedEmitBind {
    dispatch_buffer: Buffer,
    bind_group: BindGroup,
}

/// Cached per-chunk emit bind groups (invalidated on chunk removal / buffer realloc).
#[derive(Resource, Default)]
pub struct ChunkComputeBindCache {
    emit_binds: HashMap<ChunkPos, CachedEmitBind>,
}

impl ChunkComputeBindCache {
    pub fn invalidate(&mut self, pos: ChunkPos) {
        self.emit_binds.remove(&pos);
    }

    pub fn clear(&mut self) {
        self.emit_binds.clear();
    }

    fn emit_bind_group(
        &mut self,
        render_device: &RenderDevice,
        render_queue: &RenderQueue,
        pos: ChunkPos,
        slot: u32,
        assets: &crate::gpu_voxel::chunk_gpu_pool::ChunkGpuAssets,
        scratch: &ChunkScratchArena,
        gpu_buffers: &crate::gpu_voxel::gpu_resources::RenderGpuVoxelBuffers,
        compute_layout: &bevy::render::render_resource::BindGroupLayout,
        emit_uniform_buffer: &Buffer,
    ) -> BindGroup {
        if let Some(cached) = self.emit_binds.get(&pos) {
            render_queue.write_buffer(&cached.dispatch_buffer, 0, bytemuck::bytes_of(&slot));
            return cached.bind_group.clone();
        }

        let dispatch_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_chunk_dispatch"),
            size: std::mem::size_of::<u32>() as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        render_queue.write_buffer(&dispatch_buffer, 0, bytemuck::bytes_of(&slot));

        let bind_group = render_device.create_bind_group(
            "voxel_emit_bind_group",
            compute_layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: gpu_buffers.block_meta_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: assets.blocks.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: assets.power.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: gpu_buffers.chunk_table_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: dispatch_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: scratch.scratch_instances.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: scratch.scratch_counters.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: scratch.scratch_overflow.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: assets.tint_cells.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 9,
                    resource: emit_uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 10,
                    resource: scratch.scratch_slot_offsets.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 11,
                    resource: scratch.scratch_slot_capacities.as_entire_binding(),
                },
            ],
        );
        self.emit_binds.insert(
            pos,
            CachedEmitBind {
                dispatch_buffer,
                bind_group: bind_group.clone(),
            },
        );
        bind_group
    }
}

pub fn prepare_voxel_rebuild(
    state: Option<Res<GpuVoxelRenderState>>,
    mut pool: Option<ResMut<ChunkGpuPool>>,
    mut scratch: Option<ResMut<ChunkScratchArena>>,
    memory_config: Option<Res<VoxelMemoryConfig>>,
    camera: Option<Res<crate::plugin::VoxelCamera>>,
    render_config: Option<Res<crate::plugin::VoxelRenderConfig>>,
    compute_pipeline: Option<Res<VoxelComputePipeline>>,
    compact_pipeline: Option<Res<VoxelCompactPipeline>>,
    finalize_pipeline: Option<Res<VoxelFinalizePipeline>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut frame: ResMut<VoxelRebuildFrame>,
    mut persistent: ResMut<VoxelRebuildPersistent>,
    mut bind_cache: Option<ResMut<ChunkComputeBindCache>>,
    mut stats: ResMut<GpuVoxelStats>,
    instance_pool: Option<Res<InstanceBufferPool>>,
) {
    *frame = VoxelRebuildFrame::default();

    let (
        Some(state),
        Some(pool),
        Some(scratch),
        Some(memory_config),
        Some(camera),
        Some(compute_pipeline),
        Some(compact_pipeline),
        Some(finalize_pipeline),
        Some(bind_cache),
        Some(instance_pool),
    ) = (
        state,
        pool.as_mut(),
        scratch.as_mut(),
        memory_config,
        camera,
        compute_pipeline,
        compact_pipeline,
        finalize_pipeline,
        bind_cache.as_mut(),
        instance_pool,
    )
    else {
        frame.skip = true;
        return;
    };

    if pool.pos_to_slot.is_empty() {
        frame.skip = true;
        return;
    }

    let gpu_buffers = &state.buffers;
    let default_render_config = crate::plugin::VoxelRenderConfig::default();
    let render_config = render_config
        .as_deref()
        .unwrap_or(&default_render_config);

    let _uploaded_slots = pool.upload_pending(&render_device, &render_queue);

    let init_pending: Vec<u32> = pool.take_scratch_init_pending().into_iter().collect();
    let release_slots: Vec<u32> = pool.take_scratch_clear_slots().into_iter().collect();
    for &slot in &release_slots {
        scratch.release_slot(slot);
        scratch.clear_slot(&render_queue, slot);
    }
    for slot in &init_pending {
        if !scratch.init_slot(*slot) {
            pool.scratch_init_pending.insert(*slot);
            if let Some(pos) = pool.pos_for_slot(*slot) {
                pool.data_dirty.insert(pos);
            }
        }
    }
    if scratch.take_compaction_occurred() {
        pool.mark_all_dirty();
    }
    let flush_result = scratch.flush_layout(&render_device, &render_queue);
    match flush_result {
        ScratchFlushResult::BufferRecreated => {
            bind_cache.clear();
            pool.mark_all_dirty();
        }
        ScratchFlushResult::Deferred => {
            pool.mark_all_dirty();
        }
        ScratchFlushResult::LayoutOnly | ScratchFlushResult::Unchanged => {}
    }

    let cam_chunk = stagcrest_protocol::BlockPos::new(
        camera.position.x.floor() as i32,
        camera.position.y.floor() as i32,
        camera.position.z.floor() as i32,
    )
    .chunk_pos();
    pool.update_visibility(
        cam_chunk,
        render_config.horizontal,
        render_config.vertical,
    );
    pool.refresh_staggered_queue_visibility();

    let batch_size = memory_config.emit_batch_size;
    let dispatch_list = if pool.staggered_rebuild_active() {
        pool.take_staggered_emit_batch(batch_size)
    } else {
        pool.data_dirty_slots()
            .into_iter()
            .filter(|slot| pool.slot_has_assets(*slot))
            .collect()
    };
    pool.clear_data_dirty_for_slots(&dispatch_list);

    let defer_compact = pool.staggered_rebuild_active();

    frame.visible = pool.visible_slots.clone();

    pool.upload_dirty_chunk_table(
        &render_queue,
        &gpu_buffers.chunk_table_buffer,
        false,
    );
    pool.clear_dirty_table_slots();

    let zero_len = gpu_buffers.bucket_slot_count as usize;
    let zeros = vec![0u32; zero_len];
    render_queue.write_buffer(&gpu_buffers.counters_buffer, 0, bytemuck::cast_slice(&zeros));
    if let Some(overflow) = &gpu_buffers.overflow_counters_buffer {
        render_queue.write_buffer(overflow, 0, bytemuck::cast_slice(&zeros));
    }

    let mut scratch_clear_slots: HashSet<u32> = pool.take_scratch_clear_slots();
    for &slot in &dispatch_list {
        scratch_clear_slots.insert(slot);
    }
    for &slot in &scratch_clear_slots {
        scratch.clear_slot(&render_queue, slot);
    }

    let emit_uniform = EmitUniform {
        dispatch_count: 1,
        bucket_slot_count: gpu_buffers.bucket_slot_count,
        _pad0: 0,
        _pad1: 0,
    };
    let emit_uniform_buffer = ensure_uniform_buffer(
        &render_device,
        &render_queue,
        &mut persistent.emit_uniform_buffer,
        "voxel_emit_uniform",
        bytemuck::bytes_of(&emit_uniform),
    );

    for &slot in &dispatch_list {
        let Some(pos) = pool.pos_for_slot(slot) else {
            continue;
        };
        let Some(assets) = pool.assets_for(pos) else {
            continue;
        };
        let emit_bind_group = bind_cache.emit_bind_group(
            &render_device,
            &render_queue,
            pos,
            slot,
            assets,
            &*scratch,
            gpu_buffers,
            &compute_pipeline.layout,
            &emit_uniform_buffer,
        );
        frame.emit_jobs.push(ChunkEmitJob { emit_bind_group });
    }

    let overflow_binding = gpu_buffers
        .overflow_counters_buffer
        .as_ref()
        .unwrap_or(&gpu_buffers.counters_buffer);

    if !frame.visible.is_empty() && !defer_compact {
        let visible_len = frame.visible.len();
        if persistent.visible_capacity < visible_len {
            persistent.visible_capacity = visible_len.next_power_of_two().max(64);
            persistent.visible_buffer = Some(render_device.create_buffer(&BufferDescriptor {
                label: Some("voxel_visible_list"),
                size: (persistent.visible_capacity * std::mem::size_of::<u32>()) as u64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let visible_buffer = persistent.visible_buffer.as_ref().expect("visible buffer").clone();
        render_queue.write_buffer(&visible_buffer, 0, bytemuck::cast_slice(&frame.visible));

        let compact_uniform = CompactUniform {
            visible_count: visible_len as u32,
            bucket_slot_count: gpu_buffers.bucket_slot_count,
            max_scratch_slot_capacity: scratch.arena_max_capacity,
            _pad: 0,
        };
        let compact_uniform_buffer = ensure_uniform_buffer(
            &render_device,
            &render_queue,
            &mut persistent.compact_uniform_buffer,
            "voxel_compact_uniform",
            bytemuck::bytes_of(&compact_uniform),
        );
        frame.compact_bind_group = Some(render_device.create_bind_group(
            "voxel_compact_bind_group",
            &compact_pipeline.layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: gpu_buffers.chunk_table_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: visible_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: scratch.scratch_instances.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: scratch.scratch_counters.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: gpu_buffers.instances_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: gpu_buffers.counters_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: gpu_buffers.bucket_regions_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: compact_uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: overflow_binding.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 9,
                    resource: scratch.scratch_slot_offsets.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 10,
                    resource: scratch.scratch_slot_capacities.as_entire_binding(),
                },
            ],
        ));
        frame.compact_uniform_buffer = Some(compact_uniform_buffer);
        frame.emit_uniform_buffer = Some(emit_uniform_buffer);
    }

    if !defer_compact {
    let finalize_uniform = FinalizeUniform {
        bucket_count: gpu_buffers.bucket_slot_count,
        _pad: [0; 3],
    };
    let finalize_uniform_buffer = ensure_uniform_buffer(
        &render_device,
        &render_queue,
        &mut persistent.finalize_uniform_buffer,
        "voxel_finalize_uniform",
        bytemuck::bytes_of(&finalize_uniform),
    );
        frame.finalize_bind_group = Some(render_device.create_bind_group(
            "voxel_finalize_bind_group",
        &finalize_pipeline.layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: gpu_buffers.counters_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: gpu_buffers.indirect_buffers[0].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: gpu_buffers.indirect_buffers[1].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: gpu_buffers.indirect_buffers[2].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: gpu_buffers.bucket_finalize_meta_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 5,
                resource: finalize_uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 6,
                resource: overflow_binding.as_entire_binding(),
            },
        ],
    ));
    frame.finalize_uniform_buffer = Some(finalize_uniform_buffer);
    }

    frame.chunks_emitted = frame.emit_jobs.len();
    frame.chunks_compacted = frame.visible.len();
    frame.chunk_count = pool.resident_count();
    frame.scratch_max_capacity = scratch.arena_max_capacity;

    stats.chunks_emitted = frame.chunks_emitted;
    stats.chunks_compacted = frame.chunks_compacted;
    stats.chunks_rebuilt = frame.chunks_emitted;
    stats.chunk_count = frame.chunk_count;
    stats.chunk_gpu_bytes = pool.chunk_gpu_bytes();
    stats.scratch_arena_bytes = scratch.total_bytes();
    stats.scratch_bytes = stats.scratch_arena_bytes;
    stats.scratch_arena_max_slot_capacity = scratch.arena_max_capacity;
    stats.scratch_arena_total_instances = scratch.total_instance_slots;
    stats.draw_arena_bytes = gpu_buffers.instances_buffer.size();
    stats.instance_buffer_bytes = stats.draw_arena_bytes;
    stats.total_voxel_bytes = stats.chunk_gpu_bytes
        + stats.scratch_arena_bytes
        + stats.draw_arena_bytes
        + static_buffer_bytes(gpu_buffers);
    stats.peak_instances.clone_from(&instance_pool.peak);
    stats.staggered_rebuild_pending = pool.staggered_rebuild_pending();

    if pool.resident_count() as u32 >= memory_config.max_resident_chunks.saturating_sub(8) {
        tracing::warn!(
            resident = pool.resident_count(),
            max = memory_config.max_resident_chunks,
            "chunk GPU pool nearing capacity"
        );
    }
}

fn ensure_uniform_buffer(
    render_device: &RenderDevice,
    render_queue: &RenderQueue,
    buffer: &mut Option<Buffer>,
    label: &'static str,
    bytes: &[u8],
) -> Buffer {
    if buffer.is_none() {
        *buffer = Some(render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some(label),
            contents: bytes,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        }));
    } else if let Some(buf) = buffer.as_ref() {
        render_queue.write_buffer(buf, 0, bytes);
    }
    buffer.as_ref().expect("uniform buffer").clone()
}

pub fn voxel_rebuild_pass(world: &World, mut ctx: RenderContext) {
    let Some(state) = world.get_resource::<GpuVoxelRenderState>() else {
        return;
    };
    let Some(frame) = world.get_resource::<VoxelRebuildFrame>() else {
        return;
    };
    if frame.skip {
        return;
    }

    let Some(compute_pipeline) = world.get_resource::<VoxelComputePipeline>() else {
        return;
    };
    let Some(compact_pipeline) = world.get_resource::<VoxelCompactPipeline>() else {
        return;
    };
    let Some(finalize_pipeline) = world.get_resource::<VoxelFinalizePipeline>() else {
        return;
    };

    let pipeline_cache = world.resource::<PipelineCache>();
    let gpu_buffers = &state.buffers;
    let encoder = ctx.command_encoder();

    if let Some(emit_pipeline) = pipeline_cache.get_compute_pipeline(compute_pipeline.pipeline_id) {
        for job in &frame.emit_jobs {
            let mut emit_pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("voxel_emit_pass"),
                timestamp_writes: None,
            });
            emit_pass.set_pipeline(emit_pipeline);
            emit_pass.set_bind_group(0, &job.emit_bind_group, &[]);
            emit_pass.dispatch_workgroups(
                EMIT_TILES_PER_AXIS,
                EMIT_TILES_PER_AXIS,
                EMIT_TILES_PER_AXIS,
            );
        }
    }

    if let (Some(compact_bind), Some(compact)) = (
        frame.compact_bind_group.as_ref(),
        pipeline_cache.get_compute_pipeline(compact_pipeline.pipeline_id),
    ) {
        if !frame.visible.is_empty() {
            let compact_wg_y = (frame.scratch_max_capacity + 63) / 64;
            let mut compact_pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("voxel_compact_pass"),
                timestamp_writes: None,
            });
            compact_pass.set_pipeline(compact);
            compact_pass.set_bind_group(0, compact_bind, &[]);
            compact_pass.dispatch_workgroups(frame.visible.len() as u32, compact_wg_y, 1);
        }
    }

    if let (Some(finalize_bind), Some(finalize)) = (
        frame.finalize_bind_group.as_ref(),
        pipeline_cache.get_compute_pipeline(finalize_pipeline.pipeline_id),
    ) {
        let workgroups = (gpu_buffers.bucket_slot_count + 63) / 64;
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("voxel_finalize_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(finalize);
        pass.set_bind_group(0, finalize_bind, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
}
