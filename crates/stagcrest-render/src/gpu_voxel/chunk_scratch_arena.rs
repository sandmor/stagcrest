use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};

use crate::gpu_voxel::memory_config::VoxelMemoryConfig;
use crate::gpu_voxel::scratch_region_allocator::{
    instance_buffer_bytes, ScratchRegionAllocator,
};
pub use crate::gpu_voxel::scratch_region_allocator::ScratchGrowResult;
use crate::gpu_voxel::types::instance_slot_byte_size;

/// Result of applying a pending layout flush.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScratchFlushResult {
    Unchanged,
    LayoutOnly,
    BufferRecreated,
    /// CPU layout changed but GPU buffers were not updated (over budget / overcommitted).
    Deferred,
}

/// Sparse per-slot scratch arena (emit output between frames).
#[derive(Resource)]
pub struct ChunkScratchArena {
    pub scratch_instances: Buffer,
    pub scratch_slot_offsets: Buffer,
    pub scratch_slot_capacities: Buffer,
    pub scratch_counters: Buffer,
    pub scratch_overflow: Buffer,
    pub scratch_overflow_readback: Buffer,
    allocator: ScratchRegionAllocator,
    pub arena_max_capacity: u32,
    pub total_instance_slots: u32,
    pub slot_count: u32,
    pub meta_byte_size: u64,
    pub scratch_max_per_chunk: u32,
    pub scratch_arena_budget_bytes: u64,
    layout_byte_size: u64,
    instances_byte_size: u64,
    pub layout_generation: u32,
}

impl ChunkScratchArena {
    pub fn slot_capacities(&self) -> &[u32] {
        &self.allocator.slot_capacities
    }

    pub fn new(render_device: &RenderDevice, config: &VoxelMemoryConfig) -> Self {
        let slot_count = config.max_resident_chunks;
        let meta_byte_size = config.scratch_arena_meta_bytes();
        let layout_byte_size = slot_count as u64 * std::mem::size_of::<u32>() as u64;
        let instance_byte_size = instance_slot_byte_size();

        let allocator = ScratchRegionAllocator::new(
            slot_count,
            config.scratch_base_per_chunk,
            config.scratch_max_per_chunk,
            config.scratch_arena_budget_bytes,
            instance_byte_size,
            config.preallocate_scratch_buffer,
        );

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

        let allocated_instance_slots = allocator.allocated_instance_slots();
        let max_slots = allocator.max_instance_slots();
        let instances_byte_size =
            instance_buffer_bytes(allocated_instance_slots, instance_byte_size);
        let scratch_instances = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_instances_arena"),
            size: instances_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scratch_slot_offsets = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_slot_offsets"),
            size: layout_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scratch_slot_capacities = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_slot_capacities"),
            size: layout_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        tracing::info!(
            tier = config.gpu_memory_tier.label(),
            budget_mb = config.scratch_arena_budget_bytes / (1024 * 1024),
            resident_slots = slot_count,
            allocated_instance_slots,
            max_instance_slots = max_slots,
            preallocate = config.preallocate_scratch_buffer,
            "voxel scratch arena initialized"
        );

        Self {
            scratch_instances,
            scratch_slot_offsets,
            scratch_slot_capacities,
            scratch_counters,
            scratch_overflow,
            scratch_overflow_readback,
            arena_max_capacity: allocator.arena_max_capacity(),
            total_instance_slots: 0,
            slot_count,
            meta_byte_size,
            scratch_max_per_chunk: config.scratch_max_per_chunk,
            scratch_arena_budget_bytes: config.scratch_arena_budget_bytes,
            layout_byte_size,
            instances_byte_size,
            layout_generation: 0,
            allocator,
        }
    }

    pub fn take_compaction_occurred(&mut self) -> bool {
        self.allocator.take_compaction_occurred()
    }

    pub fn compact_layout(&mut self) -> bool {
        self.allocator.compact_layout()
    }

    pub fn allocated_instance_slots(&self) -> u32 {
        self.allocator.allocated_instance_slots()
    }

    pub fn arena_max_instance_slots(&self) -> u32 {
        self.allocator.max_instance_slots()
    }

    pub fn instances_byte_size(&self) -> u64 {
        self.instances_byte_size
    }

    pub fn total_bytes(&self) -> u64 {
        self.instances_byte_size
            + self.meta_byte_size * 2
            + self.meta_byte_size
            + self.layout_byte_size * 2
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

    pub fn release_slot(&mut self, slot: u32) -> bool {
        self.allocator.release_slot(slot)
    }

    pub fn init_slot(&mut self, slot: u32) -> bool {
        self.allocator.init_slot(slot)
    }

    pub fn plan_grow_slot(&mut self, slot: u32, needed: u32) -> ScratchGrowResult {
        self.allocator.plan_grow_slot(slot, needed)
    }

    pub fn flush_layout(
        &mut self,
        render_device: &RenderDevice,
        render_queue: &RenderQueue,
    ) -> ScratchFlushResult {
        if !self.allocator.layout_dirty {
            return ScratchFlushResult::Unchanged;
        }
        let old_alloc = self.allocator.allocated_instance_slots();
        let Some(new_alloc) = self.allocator.validate_layout_flush() else {
            return ScratchFlushResult::Deferred;
        };

        let high_water = self.allocator.arena_high_water();
        self.total_instance_slots = high_water;
        self.arena_max_capacity = self.allocator.arena_max_capacity();

        let buffer_recreated = new_alloc > old_alloc;
        if buffer_recreated {
            let new_bytes = instance_buffer_bytes(new_alloc, instance_slot_byte_size());
            self.scratch_instances = render_device.create_buffer(&BufferDescriptor {
                label: Some("voxel_scratch_instances_arena"),
                size: new_bytes,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocator.set_allocated_instance_slots(new_alloc);
            self.instances_byte_size = new_bytes;
            self.layout_generation = self.layout_generation.wrapping_add(1);
        }

        render_queue.write_buffer(
            &self.scratch_slot_offsets,
            0,
            bytemuck::cast_slice(&self.allocator.slot_offsets),
        );
        render_queue.write_buffer(
            &self.scratch_slot_capacities,
            0,
            bytemuck::cast_slice(&self.allocator.slot_capacities),
        );

        self.allocator.layout_dirty = false;
        if buffer_recreated {
            ScratchFlushResult::BufferRecreated
        } else {
            ScratchFlushResult::LayoutOnly
        }
    }

    pub fn grow_capacity(
        &mut self,
        render_device: &RenderDevice,
        render_queue: &RenderQueue,
        new_slot_count: u32,
    ) {
        if new_slot_count <= self.slot_count {
            return;
        }
        self.allocator.grow_slot_vectors(new_slot_count);
        self.slot_count = new_slot_count;
        self.meta_byte_size = new_slot_count as u64 * std::mem::size_of::<u32>() as u64;
        self.layout_byte_size = self.meta_byte_size;

        self.scratch_counters = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_counters_arena_grown"),
            size: self.meta_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.scratch_overflow = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_overflow_arena_grown"),
            size: self.meta_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.scratch_overflow_readback = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_overflow_readback_grown"),
            size: self.meta_byte_size,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.scratch_slot_offsets = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_slot_offsets_grown"),
            size: self.layout_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.scratch_slot_capacities = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_slot_capacities_grown"),
            size: self.layout_byte_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.allocator.layout_dirty = true;
        self.flush_layout(render_device, render_queue);
    }
}
