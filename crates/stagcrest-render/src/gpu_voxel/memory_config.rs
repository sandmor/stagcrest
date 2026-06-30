use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;

/// GPU memory budgets derived from render distance (mirrors world streaming LRU footprint).
///
/// Scratch arena VRAM (reserved at init):
/// `max_resident_chunks × scratch_instances_per_chunk × instance_bytes` plus per-slot counters.
#[derive(Resource, Clone, ExtractResource)]
pub struct VoxelMemoryConfig {
    pub max_resident_chunks: u32,
    pub scratch_ring_len: u32,
    pub scratch_instances_per_chunk: u32,
    pub instance_budget_per_chunk: u32,
    pub max_draw_instances: u32,
}

impl Default for VoxelMemoryConfig {
    fn default() -> Self {
        Self::from_render_distance(8, 4)
    }
}

impl VoxelMemoryConfig {
    /// Same formula as `stagcrest_server::streaming_lru_capacity`.
    pub fn streaming_lru_capacity(render_distance: i32, vertical_render_distance: i32) -> usize {
        let footprint_h = 2 * (render_distance + 2) + 1;
        let footprint_v = 2 * (vertical_render_distance + 2) + 1;
        (footprint_h * footprint_h * footprint_v) as usize + 64
    }

    pub fn from_render_distance(render_distance: i32, vertical_render_distance: i32) -> Self {
        let max_resident_chunks = Self::streaming_lru_capacity(render_distance, vertical_render_distance)
            as u32;
        let instance_budget_per_chunk = 2048;
        // Casual target: cap total draw instances ~2M (~128 MB at 64 B/instance).
        let max_draw_instances = (max_resident_chunks * instance_budget_per_chunk).min(2_000_000);
        Self {
            max_resident_chunks,
            scratch_ring_len: 4,
            scratch_instances_per_chunk: 4096,
            instance_budget_per_chunk,
            max_draw_instances,
        }
    }

    pub fn scratch_byte_size(&self) -> u64 {
        self.scratch_instances_per_chunk as u64 * crate::gpu_voxel::types::instance_slot_byte_size()
    }

    pub fn scratch_arena_instances_bytes(&self) -> u64 {
        self.max_resident_chunks as u64 * self.scratch_byte_size()
    }

    pub fn scratch_arena_meta_bytes(&self) -> u64 {
        self.max_resident_chunks as u64 * std::mem::size_of::<u32>() as u64
    }

    pub fn scratch_arena_bytes(&self) -> u64 {
        self.scratch_arena_instances_bytes() + self.scratch_arena_meta_bytes() * 2
    }
}

#[cfg(test)]
mod tests {
    use super::VoxelMemoryConfig;
    use crate::gpu_voxel::gpu_resources::scaled_bucket_capacities;
    use crate::gpu_voxel::types::{build_bucket_regions, VoxelInstance};

    #[test]
    fn streaming_lru_and_draw_instance_cap() {
        let cfg = VoxelMemoryConfig::from_render_distance(8, 4);
        assert_eq!(
            cfg.max_resident_chunks as usize,
            VoxelMemoryConfig::streaming_lru_capacity(8, 4),
        );

        let huge = VoxelMemoryConfig::from_render_distance(32, 16);
        assert!(huge.max_draw_instances <= 2_000_000);
    }

    #[test]
    fn scratch_and_draw_arena_budgets() {
        let cfg = VoxelMemoryConfig::from_render_distance(8, 4);
        let instance_bytes = cfg.scratch_arena_instances_bytes();
        let meta_bytes = cfg.scratch_arena_meta_bytes();
        assert_eq!(cfg.scratch_arena_bytes(), instance_bytes + meta_bytes * 2);
        assert_eq!(
            instance_bytes,
            cfg.max_resident_chunks as u64 * cfg.scratch_byte_size(),
        );

        let bucket_slots = 8u32;
        let capacities = scaled_bucket_capacities(bucket_slots, &cfg);
        let (_, total) = build_bucket_regions(bucket_slots, &capacities);
        let draw_bytes = total * std::mem::size_of::<VoxelInstance>() as u64;
        assert!(draw_bytes <= cfg.max_draw_instances as u64 * std::mem::size_of::<VoxelInstance>() as u64);
        assert!(draw_bytes > 1024 * 1024);
        assert!(capacities[0] > capacities[4]);
    }
}
