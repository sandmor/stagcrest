use bevy::prelude::*;
use bevy::render::render_resource::{
    BindGroupEntry, Buffer, BufferDescriptor, BufferInitDescriptor, BufferUsages,
    CommandEncoderDescriptor, ComputePassDescriptor, PipelineCache,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use stagcrest_protocol::ChunkPos;
use std::collections::HashMap;

use crate::gpu_voxel::chunk_cache::GpuChunkCache;
use crate::gpu_voxel::extract::camera_to_gpu_uniform;
use crate::gpu_voxel::gpu_resources::RenderGpuVoxelBuffers;
use crate::gpu_voxel::pipelines::{VoxelComputePipeline, VoxelFinalizePipeline};
use crate::gpu_voxel::prepare::ChunkGpuPoolResource;
use crate::gpu_voxel::types::{FinalizeUniform, GpuChunkUniform, GpuChunkUpload};
use crate::plugin::VoxelCamera;

pub struct ChunkGpuSlot {
    pub blocks_buffer: Buffer,
    pub power_buffer: Buffer,
    pub uniform_buffer: Buffer,
    pub chunk_uniform: GpuChunkUniform,
}

pub struct ChunkGpuPool {
    pub slots: HashMap<ChunkPos, ChunkGpuSlot>,
}

impl ChunkGpuPool {
    pub fn ensure_slot(
        &mut self,
        render_device: &RenderDevice,
        upload: &GpuChunkUpload,
    ) -> &ChunkGpuSlot {
        if !self.slots.contains_key(&upload.pos) {
            let blocks_size = (upload.blocks.len()
                * std::mem::size_of::<crate::gpu_voxel::types::GpuBlockCell>())
                as u64;
            let power_size = (upload.power.len() * std::mem::size_of::<u32>()) as u64;

            let blocks_buffer = render_device.create_buffer(&BufferDescriptor {
                label: Some("chunk_blocks"),
                size: blocks_size,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let power_buffer = render_device.create_buffer(&BufferDescriptor {
                label: Some("chunk_power"),
                size: power_size,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let uniform_buffer = render_device.create_buffer(&BufferDescriptor {
                label: Some("chunk_uniform"),
                size: std::mem::size_of::<GpuChunkUniform>() as u64,
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let chunk_uniform = chunk_uniform_for_upload(upload);
            self.slots.insert(
                upload.pos,
                ChunkGpuSlot {
                    blocks_buffer,
                    power_buffer,
                    uniform_buffer,
                    chunk_uniform,
                },
            );
        }
        self.slots.get(&upload.pos).expect("chunk slot")
    }

    pub fn sync_removed(&mut self, active: &HashMap<ChunkPos, GpuChunkUpload>) {
        self.slots.retain(|pos, _| active.contains_key(pos));
    }
}

fn chunk_uniform_for_upload(upload: &GpuChunkUpload) -> GpuChunkUniform {
    GpuChunkUniform {
        chunk_origin: [
            upload.pos.x * stagcrest_protocol::CHUNK_SIZE,
            upload.pos.y * stagcrest_protocol::CHUNK_SIZE,
            upload.pos.z * stagcrest_protocol::CHUNK_SIZE,
            0,
        ],
        air_id: upload.air.0,
        _pad: [0; 3],
    }
}

impl Default for ChunkGpuPool {
    fn default() -> Self {
        Self {
            slots: Default::default(),
        }
    }
}

pub fn run_voxel_compute(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    pipeline_cache: Res<PipelineCache>,
    state: Option<Res<crate::gpu_voxel::prepare::GpuVoxelRenderState>>,
    chunk_pool: Option<ResMut<ChunkGpuPoolResource>>,
    chunk_cache: Option<Res<GpuChunkCache>>,
    camera: Option<Res<VoxelCamera>>,
    compute_pipeline: Option<Res<VoxelComputePipeline>>,
    finalize_pipeline: Option<Res<VoxelFinalizePipeline>>,
) {
    let (Some(state), Some(chunk_cache), Some(camera), Some(mut chunk_pool)) =
        (state, chunk_cache, camera, chunk_pool)
    else {
        return;
    };
    VoxelComputeNode::run_frame(
        &render_device,
        &render_queue,
        &pipeline_cache,
        &state.buffers,
        &chunk_cache,
        &mut chunk_pool.0,
        &camera,
        compute_pipeline.as_deref(),
        finalize_pipeline.as_deref(),
    );
}

pub struct VoxelComputeNode;

impl VoxelComputeNode {
    pub fn run_frame(
        render_device: &RenderDevice,
        render_queue: &RenderQueue,
        pipeline_cache: &PipelineCache,
        gpu_buffers: &RenderGpuVoxelBuffers,
        chunk_cache: &GpuChunkCache,
        chunk_pool: &mut ChunkGpuPool,
        camera: &VoxelCamera,
        compute_pipeline: Option<&VoxelComputePipeline>,
        finalize_pipeline: Option<&VoxelFinalizePipeline>,
    ) {
        let Some(compute_pipeline) = compute_pipeline else {
            return;
        };
        let Some(compute) = pipeline_cache.get_compute_pipeline(compute_pipeline.pipeline_id) else {
            return;
        };

        if chunk_cache.chunks.is_empty() {
            return;
        }

        chunk_pool.sync_removed(&chunk_cache.chunks);

        let camera_uniform = camera_to_gpu_uniform(camera);
        let camera_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("voxel_camera_uniform"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let zero_counters = vec![0u32; gpu_buffers.bucket_slot_count as usize];
        render_queue.write_buffer(
            &gpu_buffers.counters_buffer,
            0,
            bytemuck::cast_slice(&zero_counters),
        );

        let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("voxel_compute_encoder"),
        });

        for upload in chunk_cache.chunks.values() {
            let slot = chunk_pool.ensure_slot(render_device, upload);
            render_queue.write_buffer(
                &slot.blocks_buffer,
                0,
                bytemuck::cast_slice(&upload.blocks),
            );
            let power_u32: Vec<u32> = upload.power.iter().map(|&p| p as u32).collect();
            render_queue.write_buffer(
                &slot.power_buffer,
                0,
                bytemuck::cast_slice(&power_u32),
            );
            let chunk_uniform = chunk_uniform_for_upload(upload);
            render_queue.write_buffer(
                &slot.uniform_buffer,
                0,
                bytemuck::bytes_of(&chunk_uniform),
            );

            let bind_group = render_device.create_bind_group(
                Some("voxel_compute_bind_group"),
                &compute_pipeline.layout,
                &[
                    BindGroupEntry {
                        binding: 0,
                        resource: camera_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: gpu_buffers.block_meta_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: slot.blocks_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: slot.power_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: slot.uniform_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: gpu_buffers.instances_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 6,
                        resource: gpu_buffers.counters_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 7,
                        resource: gpu_buffers.bucket_regions_buffer.as_entire_binding(),
                    },
                ],
            );

            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("voxel_compute_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(compute);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(4, 4, 4);
        }

        if let Some(finalize_pipeline) = finalize_pipeline {
            if let Some(finalize) =
                pipeline_cache.get_compute_pipeline(finalize_pipeline.pipeline_id)
            {
                let finalize_uniform = FinalizeUniform {
                    bucket_count: gpu_buffers.bucket_slot_count,
                    _pad: [0; 3],
                };
                let finalize_uniform_buffer =
                    render_device.create_buffer_with_data(&BufferInitDescriptor {
                        label: Some("voxel_finalize_uniform"),
                        contents: bytemuck::bytes_of(&finalize_uniform),
                        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                    });

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
                            resource: gpu_buffers
                                .bucket_finalize_meta_buffer
                                .as_entire_binding(),
                        },
                        BindGroupEntry {
                            binding: 5,
                            resource: finalize_uniform_buffer.as_entire_binding(),
                        },
                    ],
                );

                let workgroups = (gpu_buffers.bucket_slot_count + 63) / 64;
                let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("voxel_finalize_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(finalize);
                pass.set_bind_group(0, &finalize_bind_group, &[]);
                pass.dispatch_workgroups(workgroups, 1, 1);
            }
        }

        render_queue.submit(Some(encoder.finish()));
    }
}
