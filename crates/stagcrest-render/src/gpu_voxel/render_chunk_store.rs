use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::RenderDevice;
use stagcrest_protocol::{ChunkPos, CHUNK_SIZE};
use std::collections::{HashMap, HashSet};

use crate::gpu_voxel::chunk_cache::GpuChunkExtractBatch;
use crate::gpu_voxel::chunk_feedback::GpuChunkRenderFeedback;
use crate::gpu_voxel::types::{
    GpuChunkTableEntry, GpuChunkUpload, CHUNK_BLOCKS, CHUNK_POWER_BYTES,
    CHUNK_SCRATCH_CAPACITY, CHUNK_TABLE_VALID, CHUNK_TINT_CELLS_BYTES, HALO_VOLUME,
    max_chunk_store_slots,
};

#[derive(Clone, Copy, Debug)]
pub struct ChunkSlot {
    pub pos: ChunkPos,
    pub table_index: u32,
}

/// Render-world authoritative store for chunk GPU data and rebuild scheduling.
#[derive(Resource)]
pub struct RenderChunkStore {
    pub blocks_buffer: Buffer,
    pub power_buffer: Buffer,
    pub tint_cells_buffer: Buffer,
    pub scratch_instances_buffer: Buffer,
    pub scratch_counters_buffer: Buffer,
    pub scratch_overflow_buffer: Buffer,
    pub scratch_overflow_readback_buffer: Buffer,
    pub chunk_table: Vec<GpuChunkTableEntry>,
    pub pos_to_slot: HashMap<ChunkPos, u32>,
    pub slots: Vec<Option<ChunkSlot>>,
    pub free_slots: Vec<u32>,
    pub chunk_capacity: u32,
    pub bucket_slot_count: u32,
    /// Chunks with new block data that need emit after SSBO patch.
    pub data_dirty: HashSet<ChunkPos>,
    pub visible_slots: Vec<u32>,
    pub needs_full_rebuild: bool,
    /// Uploads waiting for GPU patch this frame.
    pub pending_gpu_uploads: Vec<GpuChunkUpload>,
    /// Scratch counters to zero before emit (evicted/removed slots).
    pub slots_needing_scratch_clear: Vec<u32>,
    /// Chunk table entries that changed and need partial GPU upload.
    pub dirty_table_slots: HashSet<u32>,
    /// Evictions performed during the current extract batch.
    frame_evicted: Vec<ChunkPos>,
}

impl RenderChunkStore {
    pub fn new(render_device: &RenderDevice, bucket_slot_count: u32) -> Self {
        let chunk_capacity = max_chunk_store_slots();
        let blocks_size = crate::gpu_voxel::types::CHUNK_BLOCKS_BYTES * chunk_capacity as u64;
        let power_size = CHUNK_POWER_BYTES * chunk_capacity as u64;
        let tint_cells_size = CHUNK_TINT_CELLS_BYTES * chunk_capacity as u64;
        let scratch_instances_size =
            crate::gpu_voxel::types::chunk_scratch_instance_bytes() * chunk_capacity as u64;
        let scratch_meta_size = chunk_capacity as u64 * std::mem::size_of::<u32>() as u64;

        let blocks_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_concat_blocks"),
            size: blocks_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let power_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_concat_power"),
            size: power_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tint_cells_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_concat_tint_cells"),
            size: tint_cells_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scratch_instances_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_chunk_scratch_instances"),
            size: scratch_instances_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scratch_counters_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_chunk_scratch_counters"),
            size: scratch_meta_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scratch_overflow_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_chunk_scratch_overflow"),
            size: scratch_meta_size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let scratch_overflow_readback_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("voxel_scratch_overflow_readback"),
            size: scratch_meta_size,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let free_slots: Vec<u32> = (0..chunk_capacity).collect();
        let chunk_table = vec![GpuChunkTableEntry::default(); chunk_capacity as usize];

        Self {
            blocks_buffer,
            power_buffer,
            tint_cells_buffer,
            scratch_instances_buffer,
            scratch_counters_buffer,
            scratch_overflow_buffer,
            scratch_overflow_readback_buffer,
            chunk_table,
            pos_to_slot: HashMap::new(),
            slots: vec![None; chunk_capacity as usize],
            free_slots,
            chunk_capacity,
            bucket_slot_count,
            data_dirty: HashSet::new(),
            visible_slots: Vec::new(),
            needs_full_rebuild: false,
            pending_gpu_uploads: Vec::new(),
            slots_needing_scratch_clear: Vec::new(),
            dirty_table_slots: HashSet::new(),
            frame_evicted: Vec::new(),
        }
    }

    pub fn apply_extract_batch(
        &mut self,
        batch: &GpuChunkExtractBatch,
        feedback: &mut GpuChunkRenderFeedback,
    ) {
        self.frame_evicted.clear();
        for pos in &batch.removals {
            self.remove_chunk(*pos);
        }
        for upload in &batch.uploads {
            if self.upsert_chunk(upload) {
                self.pending_gpu_uploads.push(upload.clone());
            } else {
                tracing::warn!(
                    chunk = ?upload.pos,
                    "render chunk store full; upload dropped"
                );
                feedback.failed_uploads.push(upload.pos);
            }
        }
        feedback.evicted.extend(self.frame_evicted.drain(..));
        feedback.rendered = self.pos_to_slot.keys().copied().collect();
    }

    pub fn mark_all_dirty(&mut self) {
        self.data_dirty.extend(self.pos_to_slot.keys().copied());
        self.dirty_table_slots.extend(self.pos_to_slot.values().copied());
        self.needs_full_rebuild = true;
    }

    pub fn upsert_chunk(&mut self, upload: &GpuChunkUpload) -> bool {
        let Some(slot) = self.alloc_or_get_slot(upload.pos) else {
            return false;
        };
        let entry = chunk_table_entry(upload, slot);
        self.chunk_table[slot as usize] = entry;
        if let Some(slot_meta) = self.slots[slot as usize].as_mut() {
            slot_meta.pos = upload.pos;
        }
        self.data_dirty.insert(upload.pos);
        self.dirty_table_slots.insert(slot);
        true
    }

    pub fn remove_chunk(&mut self, pos: ChunkPos) {
        let Some(slot) = self.pos_to_slot.remove(&pos) else {
            return;
        };
        self.chunk_table[slot as usize] = GpuChunkTableEntry::default();
        self.slots[slot as usize] = None;
        self.free_slots.push(slot);
        self.data_dirty.remove(&pos);
        self.visible_slots.retain(|&s| s != slot);
        self.slots_needing_scratch_clear.push(slot);
        self.dirty_table_slots.insert(slot);
    }

    fn alloc_or_get_slot(&mut self, pos: ChunkPos) -> Option<u32> {
        if let Some(&idx) = self.pos_to_slot.get(&pos) {
            return Some(idx);
        }
        if let Some(slot) = self.free_slots.pop() {
            self.pos_to_slot.insert(pos, slot);
            self.slots[slot as usize] = Some(ChunkSlot {
                pos,
                table_index: slot,
            });
            return Some(slot);
        }
        // Evict a non-visible chunk to make room.
        let evict_pos = self
            .pos_to_slot
            .keys()
            .copied()
            .find(|p| {
                self.pos_to_slot
                    .get(p)
                    .map(|&s| !self.visible_slots.contains(&s))
                    .unwrap_or(false)
            })?;
        let slot = self.pos_to_slot.remove(&evict_pos)?;
        self.slots[slot as usize] = None;
        self.chunk_table[slot as usize] = GpuChunkTableEntry::default();
        self.data_dirty.remove(&evict_pos);
        self.slots_needing_scratch_clear.push(slot);
        self.dirty_table_slots.insert(slot);
        self.frame_evicted.push(evict_pos);
        self.pos_to_slot.insert(pos, slot);
        self.slots[slot as usize] = Some(ChunkSlot {
            pos,
            table_index: slot,
        });
        Some(slot)
    }

    pub fn table_entry(&self, pos: ChunkPos) -> Option<&GpuChunkTableEntry> {
        self.pos_to_slot
            .get(&pos)
            .map(|&slot| &self.chunk_table[slot as usize])
    }

    pub fn pos_for_slot(&self, slot: u32) -> Option<ChunkPos> {
        self.slots
            .get(slot as usize)
            .and_then(|entry| entry.as_ref().map(|meta| meta.pos))
    }

    pub fn update_visibility(&mut self, camera_chunk: ChunkPos, h: i32, v: i32) {
        self.visible_slots.clear();
        for (&pos, &slot) in &self.pos_to_slot {
            if (pos.x - camera_chunk.x).abs() <= h
                && (pos.z - camera_chunk.z).abs() <= h
                && (pos.y - camera_chunk.y).abs() <= v
            {
                self.visible_slots.push(slot);
            }
        }
    }

    pub fn take_data_dirty_slots(&mut self) -> Vec<u32> {
        let mut slots: Vec<u32> = self
            .data_dirty
            .iter()
            .filter_map(|pos| self.pos_to_slot.get(pos).copied())
            .collect();
        self.data_dirty.clear();
        slots.sort_unstable();
        slots.dedup();
        slots
    }

    pub fn upload_dirty_chunk_table(
        &self,
        render_queue: &bevy::render::renderer::RenderQueue,
        chunk_table_buffer: &Buffer,
        full: bool,
    ) {
        let entry_size = std::mem::size_of::<GpuChunkTableEntry>() as u64;
        if full {
            render_queue.write_buffer(
                chunk_table_buffer,
                0,
                bytemuck::cast_slice(&self.chunk_table),
            );
            return;
        }
        for &slot in &self.dirty_table_slots {
            let offset = slot as u64 * entry_size;
            let entry = self.chunk_table[slot as usize];
            render_queue.write_buffer(
                chunk_table_buffer,
                offset,
                bytemuck::bytes_of(&entry),
            );
        }
    }

    pub fn clear_dirty_table_slots(&mut self) {
        self.dirty_table_slots.clear();
    }
}

fn chunk_table_entry(upload: &GpuChunkUpload, slot: u32) -> GpuChunkTableEntry {
    GpuChunkTableEntry {
        blocks_offset: slot * HALO_VOLUME as u32,
        power_offset: slot * CHUNK_BLOCKS as u32,
        scratch_offset: slot * CHUNK_SCRATCH_CAPACITY,
        tint_cells_offset: slot * crate::gpu_voxel::types::BIOME_GRID_CELLS as u32,
        origin_x: upload.pos.x * CHUNK_SIZE,
        origin_y: upload.pos.y * CHUNK_SIZE,
        origin_z: upload.pos.z * CHUNK_SIZE,
        air_id: upload.air.0,
        flags: CHUNK_TABLE_VALID,
    }
}
