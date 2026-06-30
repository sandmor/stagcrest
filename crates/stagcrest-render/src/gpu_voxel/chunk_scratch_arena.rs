use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};

use crate::gpu_voxel::memory_config::VoxelMemoryConfig;

/// Per-slot concat scratch arena (persists emit output between frames).
#[derive(Resource)]
pub struct ChunkScratchArena {
    pub scratch_instances: Buffer,
    pub scratch_counters: Buffer,
    pub scratch_overflow: Buffer,
    pub scratch_overflow_readback: Buffer,
    pub scratch_capacity: u32,
    pub slot_count: u32,
    pub instances_byte_size: u64,
    pub meta_byte_size: u64,
}

impl ChunkScratchArena {
    pub fn new(render_device: &RenderDevice, config: &VoxelMemoryConfig) -> Self {
        let slot_count = config.max_resident_chunks;
        let scratch_capacity = config.scratch_instances_per_chunk;
        let instances_byte_size = config.scratch_arena_instances_bytes();
        let meta_byte_size = config.scratch_arena_meta_bytes();

        let scratch_instances = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_instances_arena"),
            size: instances_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scratch_counters = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_counters_arena"),
            size: meta_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let scratch_overflow = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_overflow_arena"),
            size: meta_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let scratch_overflow_readback = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_overflow_readback"),
            size: meta_byte_size,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            scratch_instances,
            scratch_counters,
            scratch_overflow,
            scratch_overflow_readback,
            scratch_capacity,
            slot_count,
            instances_byte_size,
            meta_byte_size,
        }
    }

    pub fn total_bytes(&self) -> u64 {
        self.instances_byte_size + self.meta_byte_size * 2 + self.meta_byte_size
    }

    pub fn clear_slot(&self, render_queue: &RenderQueue, slot: u32) {
        let zero = 0u32;
        render_queue.write_buffer(
            &self.scratch_counters,
            slot as u64 * 4,
            bytemuck::bytes_of(&zero),
        );
        render_queue.write_buffer(
            &self.scratch_overflow,
            slot as u64 * 4,
            bytemuck::bytes_of(&zero),
        );
    }
}
