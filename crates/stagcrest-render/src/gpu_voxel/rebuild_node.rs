use bevy::prelude::*;
use bevy::render::{
    render_graph::{NodeRunError, RenderGraphContext, RenderLabel},
    render_resource::{
        BindGroup, BindGroupEntry, Buffer, BufferInitDescriptor, BufferUsages,
        ComputePassDescriptor, PipelineCache,
    },
    renderer::{RenderContext, RenderDevice, RenderQueue},
};
use bevy::render::render_graph::Node;

use crate::gpu_voxel::gpu_resources::GpuVoxelStats;
use crate::gpu_voxel::pipelines::{VoxelCompactPipeline, VoxelComputePipeline, VoxelFinalizePipeline};
use crate::gpu_voxel::prepare::GpuVoxelRenderState;
use crate::gpu_voxel::render_chunk_store::RenderChunkStore;
use crate::gpu_voxel::types::{CompactUniform, EmitUniform, FinalizeUniform, CHUNK_SCRATCH_CAPACITY};
/// Must match `EMIT_TILE_SIZE` / `EMIT_TILES_PER_AXIS` in `voxel_compute.wgsl` (16 / 4).
const EMIT_TILES_PER_AXIS: u32 = 4;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct VoxelRebuildLabel;

#[derive(Default)]
pub struct VoxelRebuildNode;

/// Per-frame GPU rebuild state prepared on the CPU before the render graph node runs.
#[derive(Resource, Default)]
pub struct VoxelRebuildFrame {
    pub skip: bool,
    pub dispatch_list: Vec<u32>,
    pub visible: Vec<u32>,
    pub dispatch_buffer: Option<Buffer>,
    pub emit_uniform_buffer: Option<Buffer>,
    pub visible_buffer: Option<Buffer>,
    pub compact_uniform_buffer: Option<Buffer>,
    pub finalize_uniform_buffer: Option<Buffer>,
    pub emit_bind_group: Option<BindGroup>,
    pub compact_bind_group: Option<BindGroup>,
    pub finalize_bind_group: Option<BindGroup>,
    pub chunks_rebuilt: usize,
    pub chunks_compacted: usize,
    pub chunk_count: usize,
    pub instance_buffer_bytes: u64,
}

pub fn prepare_voxel_rebuild(
    state: Option<Res<GpuVoxelRenderState>>,
    mut store: Option<ResMut<RenderChunkStore>>,
    camera: Option<Res<crate::plugin::VoxelCamera>>,
    compute_pipeline: Option<Res<VoxelComputePipeline>>,
    compact_pipeline: Option<Res<VoxelCompactPipeline>>,
    finalize_pipeline: Option<Res<VoxelFinalizePipeline>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut frame: ResMut<VoxelRebuildFrame>,
    mut stats: ResMut<GpuVoxelStats>,
) {
    *frame = VoxelRebuildFrame::default();

    let (Some(state), Some(mut store), Some(camera)) = (state, store.as_mut(), camera) else {
        frame.skip = true;
        return;
    };
    let (Some(compute_pipeline), Some(compact_pipeline), Some(finalize_pipeline)) =
        (compute_pipeline, compact_pipeline, finalize_pipeline)
    else {
        frame.skip = true;
        return;
    };

    if store.pos_to_slot.is_empty() {
        frame.skip = true;
        return;
    }

    let gpu_buffers = &state.buffers;

    let pending_uploads: Vec<_> = store.pending_gpu_uploads.drain(..).collect();
    for upload in pending_uploads {
        if let Some(&slot) = store.pos_to_slot.get(&upload.pos) {
            let entry = &store.chunk_table[slot as usize];
            let blocks_off = entry.blocks_offset as u64
                * std::mem::size_of::<crate::gpu_voxel::types::GpuBlockCell>() as u64;
            let power_off =
                entry.power_offset as u64 * std::mem::size_of::<u32>() as u64;
            render_queue.write_buffer(
                &store.blocks_buffer,
                blocks_off,
                bytemuck::cast_slice(&upload.blocks),
            );
            let power_u32: Vec<u32> = upload.power.iter().map(|&p| p as u32).collect();
            render_queue.write_buffer(
                &store.power_buffer,
                power_off,
                bytemuck::cast_slice(&power_u32),
            );
        }
    }

    let mut scratch_clear_slots: std::collections::HashSet<u32> =
        store.slots_needing_scratch_clear.drain(..).collect();

    let cam_chunk = stagcrest_protocol::BlockPos::new(
        camera.position.x.floor() as i32,
        camera.position.y.floor() as i32,
        camera.position.z.floor() as i32,
    )
    .chunk_pos();
    store.update_visibility(cam_chunk, 8, 4);

    let rebuild_slots = store.take_rebuild_slots();
    frame.dispatch_list = if store.needs_full_rebuild {
        store.needs_full_rebuild = false;
        store.visible_slots.clone()
    } else if rebuild_slots.is_empty() {
        Vec::new()
    } else {
        rebuild_slots
    };
    frame.visible = store.visible_slots.clone();

    render_queue.write_buffer(
        &gpu_buffers.chunk_table_buffer,
        0,
        bytemuck::cast_slice(&store.chunk_table),
    );

    let zero_len = gpu_buffers.bucket_slot_count as usize;
    let zeros = vec![0u32; zero_len];
    render_queue.write_buffer(&gpu_buffers.counters_buffer, 0, bytemuck::cast_slice(&zeros));
    if let Some(overflow) = &gpu_buffers.overflow_counters_buffer {
        render_queue.write_buffer(overflow, 0, bytemuck::cast_slice(&zeros));
    }

    for &slot in &frame.dispatch_list {
        scratch_clear_slots.insert(slot);
    }
    for &slot in &scratch_clear_slots {
        let z = 0u32;
        render_queue.write_buffer(
            &store.scratch_counters_buffer,
            slot as u64 * 4,
            bytemuck::bytes_of(&z),
        );
        render_queue.write_buffer(
            &store.scratch_overflow_buffer,
            slot as u64 * 4,
            bytemuck::bytes_of(&z),
        );
    }

    if !frame.dispatch_list.is_empty() {
        let dispatch_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("voxel_dispatch_list"),
            contents: bytemuck::cast_slice(&frame.dispatch_list),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });
        let emit_uniform = EmitUniform {
            dispatch_count: frame.dispatch_list.len() as u32,
            bucket_slot_count: gpu_buffers.bucket_slot_count,
            chunk_scratch_capacity: CHUNK_SCRATCH_CAPACITY,
            _pad: 0,
        };
        let emit_uniform_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("voxel_emit_uniform"),
            contents: bytemuck::bytes_of(&emit_uniform),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        let emit_bind_group = render_device.create_bind_group(
            Some("voxel_emit_bind_group"),
            &compute_pipeline.layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: gpu_buffers.block_meta_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: store.blocks_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: store.power_buffer.as_entire_binding(),
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
                    resource: store.scratch_instances_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: store.scratch_counters_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: store.scratch_overflow_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: emit_uniform_buffer.as_entire_binding(),
                },
            ],
        );
        frame.dispatch_buffer = Some(dispatch_buffer);
        frame.emit_uniform_buffer = Some(emit_uniform_buffer);
        frame.emit_bind_group = Some(emit_bind_group);
    }

    if !frame.visible.is_empty() {
        let visible_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("voxel_visible_list"),
            contents: bytemuck::cast_slice(&frame.visible),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });
        let compact_uniform = CompactUniform {
            visible_count: frame.visible.len() as u32,
            bucket_slot_count: gpu_buffers.bucket_slot_count,
            chunk_scratch_capacity: CHUNK_SCRATCH_CAPACITY,
            _pad: 0,
        };
        let compact_uniform_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("voxel_compact_uniform"),
            contents: bytemuck::bytes_of(&compact_uniform),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        let overflow_binding = gpu_buffers
            .overflow_counters_buffer
            .as_ref()
            .unwrap_or(&gpu_buffers.counters_buffer);
        let compact_bind_group = render_device.create_bind_group(
            Some("voxel_compact_bind_group"),
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
                    resource: store.scratch_instances_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: store.scratch_counters_buffer.as_entire_binding(),
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
            ],
        );
        frame.visible_buffer = Some(visible_buffer);
        frame.compact_uniform_buffer = Some(compact_uniform_buffer);
        frame.compact_bind_group = Some(compact_bind_group);
    }

    let finalize_uniform = FinalizeUniform {
        bucket_count: gpu_buffers.bucket_slot_count,
        _pad: [0; 3],
    };
    let finalize_uniform_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("voxel_finalize_uniform"),
        contents: bytemuck::bytes_of(&finalize_uniform),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });
    let overflow_binding = gpu_buffers
        .overflow_counters_buffer
        .as_ref()
        .unwrap_or(&gpu_buffers.counters_buffer);
    let finalize_bind_group = render_device.create_bind_group(
        Some("voxel_finalize_bind_group"),
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
    );
    frame.finalize_uniform_buffer = Some(finalize_uniform_buffer);
    frame.finalize_bind_group = Some(finalize_bind_group);

    frame.chunks_rebuilt = frame.dispatch_list.len();
    frame.chunks_compacted = frame.visible.len();
    frame.chunk_count = store.pos_to_slot.len();
    frame.instance_buffer_bytes = gpu_buffers.instances_buffer.size();

    stats.chunk_count = frame.chunk_count;
    stats.chunks_rebuilt = frame.chunks_rebuilt;
    stats.chunks_compacted = frame.chunks_compacted;
    stats.instance_buffer_bytes = frame.instance_buffer_bytes;
}

impl Node for VoxelRebuildNode {
    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let Some(state) = world.get_resource::<GpuVoxelRenderState>() else {
            return Ok(());
        };
        let Some(frame) = world.get_resource::<VoxelRebuildFrame>() else {
            return Ok(());
        };
        if frame.skip {
            return Ok(());
        }

        let Some(compute_pipeline) = world.get_resource::<VoxelComputePipeline>() else {
            return Ok(());
        };
        let Some(compact_pipeline) = world.get_resource::<VoxelCompactPipeline>() else {
            return Ok(());
        };
        let Some(finalize_pipeline) = world.get_resource::<VoxelFinalizePipeline>() else {
            return Ok(());
        };

        let pipeline_cache = world.resource::<PipelineCache>();
        let gpu_buffers = &state.buffers;
        let encoder = render_context.command_encoder();

        if let (Some(emit_bind), Some(emit_pipeline)) = (
            frame.emit_bind_group.as_ref(),
            pipeline_cache.get_compute_pipeline(compute_pipeline.pipeline_id),
        ) {
            if !frame.dispatch_list.is_empty() {
                let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("voxel_emit_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(emit_pipeline);
                pass.set_bind_group(0, emit_bind, &[]);
                let dispatch_chunks = frame.dispatch_list.len() as u32;
                pass.dispatch_workgroups(
                    dispatch_chunks * EMIT_TILES_PER_AXIS,
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
                let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("voxel_compact_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(compact);
                pass.set_bind_group(0, compact_bind, &[]);
                let wg_x = frame.visible.len() as u32;
                let wg_y = (CHUNK_SCRATCH_CAPACITY + 63) / 64;
                pass.dispatch_workgroups(wg_x, wg_y, 1);
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

        Ok(())
    }
}
