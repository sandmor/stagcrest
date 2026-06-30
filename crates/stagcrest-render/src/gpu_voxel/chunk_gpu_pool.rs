use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferInitDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use stagcrest_protocol::{ChunkPos, CHUNK_SIZE};
use std::collections::{HashMap, HashSet};

use crate::gpu_voxel::chunk_cache::GpuChunkExtractBatch;
use crate::gpu_voxel::chunk_feedback::GpuChunkRenderFeedback;
use crate::gpu_voxel::memory_config::VoxelMemoryConfig;
use crate::gpu_voxel::types::{
    chunk_gpu_asset_bytes, GpuChunkTableEntry, GpuChunkUpload, CHUNK_TABLE_VALID,
};

#[derive(Clone, Copy, Debug)]
pub struct ChunkSlot {
    pub pos: ChunkPos,
    pub table_index: u32,
}

/// Per-chunk GPU block/power/tint buffers; allocated on load, dropped on eviction.
pub struct ChunkGpuAssets {
    pub blocks: Buffer,
    pub power: Buffer,
    pub tint_cells: Buffer,
}

impl ChunkGpuAssets {
    pub fn byte_size() -> u64 {
        chunk_gpu_asset_bytes()
    }

    pub fn upload(
        &self,
        render_queue: &RenderQueue,
        upload: &GpuChunkUpload,
    ) {
        render_queue.write_buffer(
            &self.blocks,
            0,
            bytemuck::cast_slice(&upload.blocks),
        );
        render_queue.write_buffer(
            &self.power,
            0,
            bytemuck::cast_slice(&upload.power),
        );
        render_queue.write_buffer(
            &self.tint_cells,
            0,
            bytemuck::cast_slice(&upload.tint_cells),
        );
    }
}

fn alloc_chunk_assets(render_device: &RenderDevice, upload: &GpuChunkUpload) -> ChunkGpuAssets {
    let blocks = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("voxel_chunk_blocks"),
        contents: bytemuck::cast_slice(&upload.blocks),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });
    let power = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("voxel_chunk_power"),
        contents: bytemuck::cast_slice(&upload.power),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });
    let tint_cells = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("voxel_chunk_tint"),
        contents: bytemuck::cast_slice(&upload.tint_cells),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
    });
    ChunkGpuAssets {
        blocks,
        power,
        tint_cells,
    }
}

/// Pending slots for a staggered full scratch rebuild (visible chunks first).
#[derive(Debug, Default)]
pub struct StaggeredFullRebuild {
    pub pending_slots: Vec<u32>,
}

/// Render-world chunk pool with lifecycle-scoped GPU memory.
#[derive(Resource)]
pub struct ChunkGpuPool {
    pub assets: HashMap<ChunkPos, ChunkGpuAssets>,
    pub pos_to_slot: HashMap<ChunkPos, u32>,
    pub slots: Vec<Option<ChunkSlot>>,
    pub free_slots: Vec<u32>,
    pub chunk_table: Vec<GpuChunkTableEntry>,
    pub max_resident_chunks: u32,
    pub bucket_slot_count: u32,
    pub data_dirty: HashSet<ChunkPos>,
    pub visible_slots: Vec<u32>,
    pub staggered_rebuild: Option<StaggeredFullRebuild>,
    pub pending_gpu_uploads: Vec<GpuChunkUpload>,
    pub dirty_table_slots: HashSet<u32>,
    pub slots_needing_scratch_clear: HashSet<u32>,
    pub scratch_init_pending: HashSet<u32>,
    frame_evicted: Vec<ChunkPos>,
}

impl ChunkGpuPool {
    pub fn new(config: &VoxelMemoryConfig, bucket_slot_count: u32) -> Self {
        let chunk_capacity = config.max_resident_chunks;
        Self {
            assets: HashMap::new(),
            pos_to_slot: HashMap::new(),
            slots: vec![None; chunk_capacity as usize],
            free_slots: (0..chunk_capacity).collect(),
            chunk_table: vec![GpuChunkTableEntry::default(); chunk_capacity as usize],
            max_resident_chunks: chunk_capacity,
            bucket_slot_count,
            data_dirty: HashSet::new(),
            visible_slots: Vec::new(),
            staggered_rebuild: None,
            pending_gpu_uploads: Vec::new(),
            dirty_table_slots: HashSet::new(),
            slots_needing_scratch_clear: HashSet::new(),
            scratch_init_pending: HashSet::new(),
            frame_evicted: Vec::new(),
        }
    }

    pub fn chunk_gpu_bytes(&self) -> u64 {
        self.assets.len() as u64 * ChunkGpuAssets::byte_size()
    }

    pub fn resident_count(&self) -> usize {
        self.pos_to_slot.len()
    }

    pub fn assets_for(&self, pos: ChunkPos) -> Option<&ChunkGpuAssets> {
        self.assets.get(&pos)
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
                    "chunk GPU pool full; upload dropped"
                );
                feedback.failed_uploads.push(upload.pos);
            }
        }
        if batch.full_pool_rebuild {
            self.mark_all_dirty();
        } else {
            for pos in &batch.rebuild_dirty {
                let Some(&slot) = self.pos_to_slot.get(pos) else {
                    continue;
                };
                self.data_dirty.insert(*pos);
                self.dirty_table_slots.insert(slot);
            }
        }
        feedback.evicted.extend(self.frame_evicted.drain(..));
        feedback.rendered = self.pos_to_slot.keys().copied().collect();
    }

    /// Extends slot/table vectors when render distance grows at runtime.
    pub fn grow_capacity(&mut self, new_capacity: u32) {
        if new_capacity <= self.max_resident_chunks {
            return;
        }
        let old_len = self.max_resident_chunks as usize;
        let new_len = new_capacity as usize;
        self.slots.resize(new_len, None);
        self.chunk_table.resize(new_len, GpuChunkTableEntry::default());
        for slot in old_len..new_len {
            self.free_slots.push(slot as u32);
        }
        self.max_resident_chunks = new_capacity;
    }

    pub fn mark_all_dirty(&mut self) {
        self.data_dirty.extend(self.pos_to_slot.keys().copied());
        self.dirty_table_slots.extend(self.pos_to_slot.values().copied());
        self.schedule_staggered_full_rebuild();
    }

    fn schedule_staggered_full_rebuild(&mut self) {
        let residents = self.resident_slots_with_assets();
        if residents.is_empty() {
            return;
        }
        let visible: HashSet<u32> = self.visible_slots.iter().copied().collect();
        self.staggered_rebuild = Some(StaggeredFullRebuild {
            pending_slots: stagger_queue_visible_first(&residents, &visible),
        });
    }

    pub fn staggered_rebuild_pending(&self) -> u32 {
        self.staggered_rebuild
            .as_ref()
            .map(|s| s.pending_slots.len() as u32)
            .unwrap_or(0)
    }

    pub fn staggered_rebuild_active(&self) -> bool {
        self.staggered_rebuild
            .as_ref()
            .is_some_and(|s| !s.pending_slots.is_empty())
    }

    /// Re-sort remaining stagger queue so visible slots are emitted first.
    pub fn refresh_staggered_queue_visibility(&mut self) {
        let Some(stagger) = self.staggered_rebuild.as_mut() else {
            return;
        };
        if stagger.pending_slots.is_empty() {
            return;
        }
        let visible: HashSet<u32> = self.visible_slots.iter().copied().collect();
        let pending = std::mem::take(&mut stagger.pending_slots);
        stagger.pending_slots = stagger_queue_visible_first(&pending, &visible);
    }

    pub fn take_staggered_emit_batch(&mut self, batch_size: u32) -> Vec<u32> {
        let Some(stagger) = self.staggered_rebuild.as_mut() else {
            return Vec::new();
        };
        if stagger.pending_slots.is_empty() {
            self.staggered_rebuild = None;
            return Vec::new();
        }
        let take = (batch_size as usize).min(stagger.pending_slots.len());
        let batch: Vec<u32> = stagger.pending_slots.drain(..take).collect();
        if stagger.pending_slots.is_empty() {
            self.staggered_rebuild = None;
        }
        batch
    }

    pub fn upsert_chunk(&mut self, upload: &GpuChunkUpload) -> bool {
        let Some(slot) = self.alloc_or_get_slot(upload.pos) else {
            return false;
        };
        let entry = chunk_table_entry(upload);
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
        self.assets.remove(&pos);
        self.chunk_table[slot as usize] = GpuChunkTableEntry::default();
        self.slots[slot as usize] = None;
        self.free_slots.push(slot);
        self.data_dirty.remove(&pos);
        self.visible_slots.retain(|&s| s != slot);
        self.dirty_table_slots.insert(slot);
        self.slots_needing_scratch_clear.insert(slot);
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
            self.slots_needing_scratch_clear.insert(slot);
            self.scratch_init_pending.insert(slot);
            return Some(slot);
        }
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
        self.assets.remove(&evict_pos);
        self.slots[slot as usize] = None;
        self.chunk_table[slot as usize] = GpuChunkTableEntry::default();
        self.data_dirty.remove(&evict_pos);
        self.dirty_table_slots.insert(slot);
        self.slots_needing_scratch_clear.insert(slot);
        self.frame_evicted.push(evict_pos);
        self.pos_to_slot.insert(pos, slot);
        self.slots[slot as usize] = Some(ChunkSlot {
            pos,
            table_index: slot,
        });
        self.scratch_init_pending.insert(slot);
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

    /// Slots that currently have GPU assets (for full scratch rebuilds).
    pub fn resident_slots_with_assets(&self) -> Vec<u32> {
        let mut slots: Vec<u32> = self
            .pos_to_slot
            .iter()
            .filter(|(pos, _)| self.assets.contains_key(pos))
            .map(|(_, &slot)| slot)
            .collect();
        slots.sort_unstable();
        slots.dedup();
        slots
    }

    pub fn data_dirty_slots(&self) -> Vec<u32> {
        let mut slots: Vec<u32> = self
            .data_dirty
            .iter()
            .filter_map(|pos| self.pos_to_slot.get(pos).copied())
            .collect();
        slots.sort_unstable();
        slots.dedup();
        slots
    }

    pub fn slot_has_assets(&self, slot: u32) -> bool {
        self.pos_for_slot(slot)
            .is_some_and(|pos| self.assets.contains_key(&pos))
    }

    pub fn clear_data_dirty_for_slots(&mut self, slots: &[u32]) {
        for &slot in slots {
            if let Some(pos) = self.pos_for_slot(slot) {
                self.data_dirty.remove(&pos);
            }
        }
    }

    pub fn take_scratch_clear_slots(&mut self) -> HashSet<u32> {
        std::mem::take(&mut self.slots_needing_scratch_clear)
    }

    pub fn take_scratch_init_pending(&mut self) -> HashSet<u32> {
        std::mem::take(&mut self.scratch_init_pending)
    }

    pub fn upload_pending(
        &mut self,
        render_device: &RenderDevice,
        render_queue: &RenderQueue,
    ) -> HashSet<u32> {
        let mut uploaded_slots = HashSet::new();
        for upload in self.pending_gpu_uploads.drain(..) {
            let Some(&slot) = self.pos_to_slot.get(&upload.pos) else {
                continue;
            };
            uploaded_slots.insert(slot);
            let assets = self
                .assets
                .entry(upload.pos)
                .or_insert_with(|| alloc_chunk_assets(render_device, &upload));
            assets.upload(render_queue, &upload);
        }
        uploaded_slots
    }

    pub fn upload_dirty_chunk_table(
        &self,
        render_queue: &RenderQueue,
        chunk_table_buffer: &Buffer,
        full: bool,
    ) {
        let entry_size = std::mem::size_of::<GpuChunkTableEntry>() as u64;
        if full {
            let bytes = self.chunk_table.len() as u64 * entry_size;
            if chunk_table_buffer.size() < bytes {
                tracing::error!(
                    buffer_bytes = chunk_table_buffer.size(),
                    needed_bytes = bytes,
                    "chunk table GPU buffer too small for full upload"
                );
                return;
            }
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
            render_queue.write_buffer(chunk_table_buffer, offset, bytemuck::bytes_of(&entry));
        }
    }

    pub fn clear_dirty_table_slots(&mut self) {
        self.dirty_table_slots.clear();
    }
}

fn stagger_queue_visible_first(residents: &[u32], visible: &HashSet<u32>) -> Vec<u32> {
    let mut pending: Vec<u32> = residents
        .iter()
        .filter(|slot| visible.contains(slot))
        .copied()
        .collect();
    pending.extend(
        residents
            .iter()
            .filter(|slot| !visible.contains(slot))
            .copied(),
    );
    pending
}

fn chunk_table_entry(upload: &GpuChunkUpload) -> GpuChunkTableEntry {
    GpuChunkTableEntry {
        origin_x: upload.pos.x * CHUNK_SIZE,
        origin_y: upload.pos.y * CHUNK_SIZE,
        origin_z: upload.pos.z * CHUNK_SIZE,
        air_id: upload.air.0,
        flags: CHUNK_TABLE_VALID,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_voxel::memory_config::VoxelMemoryConfig;

    #[test]
    fn pool_lifecycle_dirty_dispatch_and_removal() {
        let cfg = VoxelMemoryConfig::from_render_distance(4, 2);
        let mut pool = ChunkGpuPool::new(&cfg, 8);
        let pos = ChunkPos { x: 0, y: 0, z: 0 };

        assert!(pool.upsert_chunk(&test_upload(pos)));
        assert_eq!(pool.resident_count(), 1);

        let dirty_slots = pool.data_dirty_slots();
        assert_eq!(dirty_slots.len(), 1);
        let slot = dirty_slots[0];

        // GPU assets appear after upload_pending; until then emit must not consume dirty.
        assert!(!pool.slot_has_assets(slot));
        let dispatch: Vec<u32> = dirty_slots
            .iter()
            .copied()
            .filter(|s| pool.slot_has_assets(*s))
            .collect();
        assert!(dispatch.is_empty());
        pool.clear_data_dirty_for_slots(&dispatch);
        assert!(pool.data_dirty.contains(&pos));

        pool.remove_chunk(pos);
        assert_eq!(pool.resident_count(), 0);
        assert!(pool.assets.is_empty());
        assert!(!pool.free_slots.is_empty());
    }

    #[test]
    fn pool_grow_capacity_extends_free_slots() {
        let cfg = VoxelMemoryConfig::from_render_distance(4, 2);
        let mut pool = ChunkGpuPool::new(&cfg, 8);
        let initial = pool.max_resident_chunks;
        pool.grow_capacity(initial + 16);
        assert_eq!(pool.max_resident_chunks, initial + 16);
        assert_eq!(pool.slots.len(), (initial + 16) as usize);
        assert!(pool.free_slots.len() >= 16);
    }

    #[test]
    fn stagger_queue_orders_visible_first() {
        let residents = vec![0, 1, 2, 3, 4];
        let visible: HashSet<u32> = [3, 1].into_iter().collect();
        assert_eq!(
            stagger_queue_visible_first(&residents, &visible),
            vec![1, 3, 0, 2, 4]
        );
    }

    #[test]
    fn staggered_emit_batch_drains_queue() {
        let cfg = VoxelMemoryConfig::from_render_distance(4, 2);
        let mut pool = ChunkGpuPool::new(&cfg, 8);
        pool.staggered_rebuild = Some(StaggeredFullRebuild {
            pending_slots: vec![1, 2, 3, 4, 5],
        });
        assert_eq!(pool.take_staggered_emit_batch(2), vec![1, 2]);
        assert_eq!(pool.staggered_rebuild_pending(), 3);
        assert_eq!(pool.take_staggered_emit_batch(10), vec![3, 4, 5]);
        assert_eq!(pool.staggered_rebuild_pending(), 0);
        assert!(!pool.staggered_rebuild_active());
    }

    fn test_upload(pos: ChunkPos) -> GpuChunkUpload {
        use crate::gpu_voxel::types::{GpuBlockCell, GpuChunkTintCell, HALO_VOLUME, CHUNK_BLOCKS, BIOME_GRID_CELLS};
        use stagcrest_protocol::BlockId;
        GpuChunkUpload {
            pos,
            air: BlockId(0),
            blocks: [GpuBlockCell::default(); HALO_VOLUME],
            power: [0u8; CHUNK_BLOCKS],
            biome: [0u8; BIOME_GRID_CELLS],
            climate: None,
            tint_cells: [GpuChunkTintCell::default(); BIOME_GRID_CELLS],
        }
    }
}
