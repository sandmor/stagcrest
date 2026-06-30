use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;

/// GPU memory budget preset (code-configurable until a settings UI exists).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VoxelGpuMemoryTier {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

impl VoxelGpuMemoryTier {
    fn scratch_headroom_num_den(self) -> (u64, u64) {
        match self {
            Self::Low => (1, 1),
            Self::Medium => (5, 4),
            Self::High | Self::Ultra => (2, 1),
        }
    }

    fn budget_cap_bytes(self) -> u64 {
        match self {
            Self::Low => 512 * 1024 * 1024,
            Self::Medium => 768 * 1024 * 1024,
            Self::High | Self::Ultra => 1536 * 1024 * 1024,
        }
    }

    fn draw_instance_cap(self) -> u32 {
        match self {
            Self::Low => 1_000_000,
            Self::Medium => 1_500_000,
            Self::High | Self::Ultra => 2_000_000,
        }
    }

    pub fn emit_batch_size(self) -> u32 {
        match self {
            Self::Low => 64,
            Self::Medium => 96,
            Self::High => 128,
            Self::Ultra => 192,
        }
    }

    pub fn preallocate_scratch_buffer(self) -> bool {
        !matches!(self, Self::Low | Self::Medium)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }
}

/// GPU memory budgets derived from render distance (mirrors world streaming LRU footprint).
///
/// Scratch arena VRAM is sparse: only active slots consume `slot_capacity × instance_bytes`.
/// Total is capped by [`scratch_arena_budget_bytes`].
#[derive(Resource, Clone, ExtractResource)]
pub struct VoxelMemoryConfig {
    pub gpu_memory_tier: VoxelGpuMemoryTier,
    pub max_resident_chunks: u32,
    pub scratch_base_per_chunk: u32,
    pub scratch_max_per_chunk: u32,
    pub scratch_arena_budget_bytes: u64,
    pub instance_budget_per_chunk: u32,
    pub max_draw_instances: u32,
    pub emit_batch_size: u32,
    pub overflow_poll_interval_idle: u32,
    pub overflow_poll_interval_active: u32,
    pub preallocate_scratch_buffer: bool,
}

impl Default for VoxelMemoryConfig {
    fn default() -> Self {
        Self::from_render_distance_and_tier(8, 4, VoxelGpuMemoryTier::High)
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
        Self::from_render_distance_and_tier(
            render_distance,
            vertical_render_distance,
            VoxelGpuMemoryTier::High,
        )
    }

    pub fn from_render_distance_and_tier(
        render_distance: i32,
        vertical_render_distance: i32,
        tier: VoxelGpuMemoryTier,
    ) -> Self {
        let max_resident_chunks =
            Self::streaming_lru_capacity(render_distance, vertical_render_distance) as u32;
        let scratch_base_per_chunk = 2048;
        let instance_budget_per_chunk = 2048;
        Self {
            gpu_memory_tier: tier,
            max_resident_chunks,
            scratch_base_per_chunk,
            scratch_max_per_chunk: 16_384,
            scratch_arena_budget_bytes: Self::scratch_arena_budget_bytes_for(
                max_resident_chunks,
                scratch_base_per_chunk,
                tier,
            ),
            max_draw_instances: (max_resident_chunks * instance_budget_per_chunk)
                .min(tier.draw_instance_cap()),
            instance_budget_per_chunk,
            emit_batch_size: tier.emit_batch_size(),
            overflow_poll_interval_idle: 60,
            overflow_poll_interval_active: 4,
            preallocate_scratch_buffer: tier.preallocate_scratch_buffer(),
        }
    }

    pub fn scratch_arena_budget_bytes_for(
        max_resident_chunks: u32,
        scratch_base_per_chunk: u32,
        tier: VoxelGpuMemoryTier,
    ) -> u64 {
        let (headroom_num, headroom_den) = tier.scratch_headroom_num_den();
        let instance_bytes = crate::gpu_voxel::types::instance_slot_byte_size();
        let base_bytes =
            max_resident_chunks as u64 * scratch_base_per_chunk as u64 * instance_bytes;
        (base_bytes * headroom_num / headroom_den).min(tier.budget_cap_bytes())
    }

    pub fn scratch_byte_size(&self) -> u64 {
        self.scratch_base_per_chunk as u64 * crate::gpu_voxel::types::instance_slot_byte_size()
    }

    /// Upper bound if every slot used base capacity (actual arena is sparse).
    pub fn scratch_arena_instances_bytes_upper_bound(&self) -> u64 {
        self.max_resident_chunks as u64 * self.scratch_byte_size()
    }

    pub fn scratch_arena_meta_bytes(&self) -> u64 {
        self.max_resident_chunks as u64 * std::mem::size_of::<u32>() as u64
    }

    /// Layout SSBOs (offsets + capacities) plus counter/overflow meta at max slot count.
    pub fn scratch_arena_layout_bytes(&self) -> u64 {
        self.scratch_arena_meta_bytes() * 2
    }
}

#[cfg(test)]
mod tests {
    use super::{VoxelGpuMemoryTier, VoxelMemoryConfig};
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
    fn scratch_budget_scales_with_render_distance_and_tier() {
        let cfg = VoxelMemoryConfig::from_render_distance_and_tier(8, 4, VoxelGpuMemoryTier::High);
        assert_eq!(cfg.scratch_base_per_chunk, 2048);
        assert_eq!(cfg.scratch_max_per_chunk, 16_384);
        assert_eq!(cfg.emit_batch_size, 128);
        assert!(cfg.preallocate_scratch_buffer);

        let upper = cfg.scratch_arena_instances_bytes_upper_bound();
        assert!(cfg.scratch_arena_budget_bytes >= upper);
        assert_eq!(
            cfg.scratch_arena_budget_bytes,
            upper.saturating_mul(2).min(1536 * 1024 * 1024)
        );

        let low = VoxelMemoryConfig::from_render_distance_and_tier(8, 4, VoxelGpuMemoryTier::Low);
        assert!(low.scratch_arena_budget_bytes < cfg.scratch_arena_budget_bytes);
        assert_eq!(
            low.scratch_arena_budget_bytes,
            upper.min(512 * 1024 * 1024)
        );
        assert!(!low.preallocate_scratch_buffer);
        assert_eq!(low.max_draw_instances, 1_000_000);

        let small = VoxelMemoryConfig::from_render_distance_and_tier(4, 2, VoxelGpuMemoryTier::High);
        assert!(small.scratch_arena_budget_bytes < cfg.scratch_arena_budget_bytes);

        let huge =
            VoxelMemoryConfig::from_render_distance_and_tier(32, 16, VoxelGpuMemoryTier::High);
        assert!(huge.scratch_arena_budget_bytes <= 1536 * 1024 * 1024);

        let bucket_slots = 8u32;
        let capacities = scaled_bucket_capacities(bucket_slots, &cfg);
        let (_, total) = build_bucket_regions(bucket_slots, &capacities);
        let draw_bytes = total * std::mem::size_of::<VoxelInstance>() as u64;
        assert!(draw_bytes
            <= cfg.max_draw_instances as u64 * std::mem::size_of::<VoxelInstance>() as u64);
        assert!(draw_bytes > 1024 * 1024);
        assert!(capacities[0] > capacities[4]);
    }
}
