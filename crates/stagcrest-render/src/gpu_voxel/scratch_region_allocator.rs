use bevy::prelude::*;

/// Minimum instance slots reserved in the GPU buffer to avoid realloc on every chunk load.
pub const MIN_ALLOCATED_INSTANCE_SLOTS: u32 = 32_768;

/// Result of attempting to grow a slot's scratch capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScratchGrowResult {
    Grown,
    Unchanged,
    AtCapacity,
    BudgetExceeded,
}

/// CPU-side sparse region allocator for per-chunk scratch instance buffers.
pub struct ScratchRegionAllocator {
    pub slot_capacities: Vec<u32>,
    pub slot_offsets: Vec<u32>,
    free_regions: Vec<(u32, u32)>,
    scratch_base_per_chunk: u32,
    pub scratch_max_per_chunk: u32,
    scratch_arena_budget_bytes: u64,
    instance_byte_size: u64,
    pub layout_dirty: bool,
    /// Set when [`Self::compact_layout`] repacks offsets; rebuild should mark all chunks dirty once.
    pub compaction_occurred: bool,
    allocated_instance_slots: u32,
}

impl ScratchRegionAllocator {
    pub fn new(
        slot_count: u32,
        scratch_base_per_chunk: u32,
        scratch_max_per_chunk: u32,
        scratch_arena_budget_bytes: u64,
        instance_byte_size: u64,
        preallocate: bool,
    ) -> Self {
        let max_slots = (scratch_arena_budget_bytes / instance_byte_size) as u32;
        let allocated_instance_slots = if preallocate {
            max_slots.max(MIN_ALLOCATED_INSTANCE_SLOTS)
        } else {
            MIN_ALLOCATED_INSTANCE_SLOTS.min(max_slots)
        };
        Self {
            slot_capacities: vec![0u32; slot_count as usize],
            slot_offsets: vec![0u32; slot_count as usize],
            free_regions: Vec::new(),
            scratch_base_per_chunk,
            scratch_max_per_chunk,
            scratch_arena_budget_bytes,
            instance_byte_size,
            layout_dirty: false,
            compaction_occurred: false,
            allocated_instance_slots,
        }
    }

    pub fn take_compaction_occurred(&mut self) -> bool {
        std::mem::take(&mut self.compaction_occurred)
    }

    pub fn allocated_instance_slots(&self) -> u32 {
        self.allocated_instance_slots
    }

    pub fn set_allocated_instance_slots(&mut self, slots: u32) {
        self.allocated_instance_slots = slots;
    }

    pub fn max_instance_slots(&self) -> u32 {
        (self.scratch_arena_budget_bytes / self.instance_byte_size) as u32
    }

    pub fn arena_max_capacity(&self) -> u32 {
        max_slot_capacity(&self.slot_capacities)
    }

    pub fn arena_high_water(&self) -> u32 {
        arena_high_water(&self.slot_capacities, &self.slot_offsets)
    }

    /// Repack active slots into a dense prefix-sum layout to reclaim fragmented holes.
    pub fn compact_layout(&mut self) -> bool {
        let mut cursor = 0u32;
        let mut offsets_repacked = false;
        for i in 0..self.slot_capacities.len() {
            let cap = self.slot_capacities[i];
            if cap > 0 {
                if self.slot_offsets[i] != cursor {
                    offsets_repacked = true;
                }
                self.slot_offsets[i] = cursor;
                cursor = cursor.saturating_add(cap);
            } else if self.slot_offsets[i] != 0 {
                self.slot_offsets[i] = 0;
                offsets_repacked = true;
            }
        }
        self.free_regions.clear();
        if offsets_repacked {
            self.layout_dirty = true;
            self.compaction_occurred = true;
            tracing::debug!(
                high_water = cursor,
                active_slots = self.slot_capacities.iter().filter(|&&c| c > 0).count(),
                "scratch layout compacted"
            );
        }
        offsets_repacked
    }

    /// Returns instance budget when a pool slot is freed (removal or eviction).
    pub fn release_slot(&mut self, slot: u32) -> bool {
        let slot = slot as usize;
        if slot >= self.slot_capacities.len() || self.slot_capacities[slot] == 0 {
            return false;
        }
        let offset = self.slot_offsets[slot];
        let freed = self.slot_capacities[slot];
        if offset > 0 && freed > 0 {
            self.insert_free_region(offset, freed);
        }
        self.slot_capacities[slot] = 0;
        self.slot_offsets[slot] = 0;
        self.layout_dirty = true;
        true
    }

    /// Assign base scratch capacity to a newly allocated pool slot (CPU only).
    pub fn init_slot(&mut self, slot: u32) -> bool {
        let slot = slot as usize;
        if slot >= self.slot_capacities.len() || self.slot_capacities[slot] > 0 {
            return false;
        }
        let Some(offset) = self.assign_or_compact(slot, self.scratch_base_per_chunk) else {
            return false;
        };
        let old_offset = self.slot_offsets[slot];
        if old_offset > 0 && old_offset != offset {
            tracing::debug!(
                slot,
                old_offset = old_offset,
                new_offset = offset,
                cap = self.scratch_base_per_chunk,
                "relocated scratch slot on init"
            );
        }
        self.slot_offsets[slot] = offset;
        self.slot_capacities[slot] = self.scratch_base_per_chunk;
        self.layout_dirty = true;
        true
    }

    pub fn plan_grow_slot(&mut self, slot: u32, needed: u32) -> ScratchGrowResult {
        let slot = slot as usize;
        if slot >= self.slot_capacities.len() {
            return ScratchGrowResult::Unchanged;
        }
        let mut current = self.slot_capacities[slot];
        if current == 0 {
            self.slot_capacities[slot] = self.scratch_base_per_chunk;
            current = self.scratch_base_per_chunk;
        }
        let new_cap = next_pow2_at_least(needed).min(self.scratch_max_per_chunk);
        if new_cap <= current {
            return if current >= self.scratch_max_per_chunk && needed > current {
                ScratchGrowResult::AtCapacity
            } else {
                ScratchGrowResult::Unchanged
            };
        }
        let Some(offset) = self.assign_or_compact(slot, new_cap) else {
            let projected_high = self.projected_high_water_after_slot_cap(slot, new_cap);
            return if projected_high > self.max_instance_slots() {
                ScratchGrowResult::AtCapacity
            } else {
                ScratchGrowResult::BudgetExceeded
            };
        };
        let old_offset = self.slot_offsets[slot];
        if old_offset > 0 && old_offset != offset && current > 0 {
            self.insert_free_region(old_offset, current);
        }
        if old_offset > 0 && old_offset != offset {
            tracing::debug!(
                slot,
                old_offset = old_offset,
                new_offset = offset,
                old_cap = current,
                new_cap,
                "relocated scratch slot on grow"
            );
        }
        self.slot_offsets[slot] = offset;
        self.slot_capacities[slot] = new_cap;
        self.layout_dirty = true;
        ScratchGrowResult::Grown
    }

    /// Returns `None` when the layout is overcommitted.
    pub fn validate_layout_flush(&self) -> Option<u32> {
        let high_water = self.arena_high_water();
        let max_slots = self.max_instance_slots();
        if high_water > max_slots {
            tracing::debug!(
                high_water,
                max_slots,
                active_slots = self.slot_capacities.iter().filter(|&&c| c > 0).count(),
                "scratch layout overcommitted; deferring GPU update"
            );
            return None;
        }
        if high_water > self.allocated_instance_slots {
            let new_alloc =
                required_allocation(high_water, self.allocated_instance_slots, max_slots);
            let new_bytes = instance_buffer_bytes(new_alloc, self.instance_byte_size);
            if new_bytes > self.scratch_arena_budget_bytes || high_water > new_alloc {
                tracing::debug!(
                    high_water,
                    needed_alloc = new_alloc,
                    budget_bytes = self.scratch_arena_budget_bytes,
                    "scratch arena budget exceeded at flush"
                );
                return None;
            }
            Some(new_alloc)
        } else {
            Some(self.allocated_instance_slots)
        }
    }

    pub fn grow_slot_vectors(&mut self, new_slot_count: u32) {
        if new_slot_count as usize > self.slot_capacities.len() {
            self.slot_capacities.resize(new_slot_count as usize, 0);
            self.slot_offsets.resize(new_slot_count as usize, 0);
        }
    }

    fn can_accommodate_high_water(&self, needed_high_water: u32) -> bool {
        if needed_high_water <= self.allocated_instance_slots {
            return true;
        }
        let new_alloc = required_allocation(
            needed_high_water,
            self.allocated_instance_slots,
            self.max_instance_slots(),
        );
        needed_high_water <= new_alloc
            && instance_buffer_bytes(new_alloc, self.instance_byte_size)
                <= self.scratch_arena_budget_bytes
    }

    fn projected_high_water_after_slot_cap(&self, slot: usize, new_cap: u32) -> u32 {
        let mut high = 0u32;
        for (i, &cap) in self.slot_capacities.iter().enumerate() {
            let cap = if i == slot { new_cap } else { cap };
            if cap > 0 {
                high = high.max(self.slot_offsets[i].saturating_add(cap));
            }
        }
        high
    }

    fn region_overlaps_active(&self, exclude: usize, start: u32, size: u32) -> bool {
        region_overlaps_active(
            &self.slot_capacities,
            &self.slot_offsets,
            exclude,
            start,
            size,
        )
    }

    fn projected_high_with_offset(&self, slot: usize, offset: u32, cap: u32) -> u32 {
        let mut high = 0u32;
        for (i, &existing_cap) in self.slot_capacities.iter().enumerate() {
            if existing_cap == 0 && i != slot {
                continue;
            }
            let (o, c) = if i == slot {
                (offset, cap)
            } else {
                (self.slot_offsets[i], existing_cap)
            };
            if c > 0 {
                high = high.max(o.saturating_add(c));
            }
        }
        high
    }

    fn assign_non_overlapping_offset(&mut self, slot: usize, cap: u32) -> Option<u32> {
        if self.slot_offsets[slot] > 0 {
            let preferred = self.slot_offsets[slot];
            if !self.region_overlaps_active(slot, preferred, cap) {
                let projected = self.projected_high_with_offset(slot, preferred, cap);
                if self.can_accommodate_high_water(projected) {
                    return Some(preferred);
                }
            }
        }
        if let Some(offset) = self.take_free_region(slot, cap) {
            let projected = self.projected_high_with_offset(slot, offset, cap);
            if self.can_accommodate_high_water(projected) {
                return Some(offset);
            }
            self.insert_free_region(offset, cap);
        }
        let offset = self.arena_high_water();
        if self.region_overlaps_active(slot, offset, cap) {
            return None;
        }
        let projected = self.projected_high_with_offset(slot, offset, cap);
        if self.can_accommodate_high_water(projected) {
            Some(offset)
        } else {
            None
        }
    }

    fn insert_free_region(&mut self, offset: u32, size: u32) {
        if size == 0 {
            return;
        }
        self.free_regions.push((offset, size));
        coalesce_free_regions(&mut self.free_regions);
    }

    fn take_free_region(&mut self, exclude: usize, cap: u32) -> Option<u32> {
        let mut i = 0;
        while i < self.free_regions.len() {
            let (off, size) = self.free_regions[i];
            if size >= cap
                && !region_overlaps_active(
                    &self.slot_capacities,
                    &self.slot_offsets,
                    exclude,
                    off,
                    cap,
                )
            {
                self.free_regions.remove(i);
                if size > cap {
                    self.insert_free_region(off.saturating_add(cap), size - cap);
                }
                return Some(off);
            }
            i += 1;
        }
        None
    }

    fn compaction_may_help(&self) -> bool {
        if !self.free_regions.is_empty() {
            return true;
        }
        let active_sum: u32 = self.slot_capacities.iter().sum();
        self.arena_high_water() > active_sum
    }

    fn assign_or_compact(&mut self, slot: usize, cap: u32) -> Option<u32> {
        if let Some(offset) = self.assign_non_overlapping_offset(slot, cap) {
            return Some(offset);
        }
        if !self.compaction_may_help() {
            return None;
        }
        if self.compact_layout() {
            self.assign_non_overlapping_offset(slot, cap)
        } else {
            None
        }
    }
}

pub fn coalesce_free_regions(regions: &mut Vec<(u32, u32)>) {
    if regions.is_empty() {
        return;
    }
    regions.sort_by_key(|&(o, _)| o);
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(regions.len());
    for (offset, size) in regions.drain(..) {
        if let Some((last_offset, last_size)) = merged.last_mut() {
            let last_end = last_offset.saturating_add(*last_size);
            if offset <= last_end {
                let new_end = offset.saturating_add(size).max(last_end);
                *last_size = new_end - *last_offset;
                continue;
            }
        }
        merged.push((offset, size));
    }
    *regions = merged;
}

pub fn required_allocation(needed_total: u32, current_alloc: u32, max_slots: u32) -> u32 {
    if needed_total <= current_alloc {
        return current_alloc;
    }
    next_pow2_at_least(needed_total.max(MIN_ALLOCATED_INSTANCE_SLOTS)).min(max_slots)
}

pub fn region_overlaps_active(
    capacities: &[u32],
    offsets: &[u32],
    exclude: usize,
    start: u32,
    size: u32,
) -> bool {
    let end = start.saturating_add(size);
    for (i, &cap) in capacities.iter().enumerate() {
        if i == exclude || cap == 0 {
            continue;
        }
        let other_start = offsets[i];
        let other_end = other_start.saturating_add(cap);
        if start < other_end && end > other_start {
            return true;
        }
    }
    false
}

pub fn arena_high_water(capacities: &[u32], offsets: &[u32]) -> u32 {
    let mut high = 0u32;
    for (i, &cap) in capacities.iter().enumerate() {
        if cap > 0 {
            high = high.max(offsets[i].saturating_add(cap));
        }
    }
    high
}

pub fn max_slot_capacity(capacities: &[u32]) -> u32 {
    capacities.iter().copied().max().unwrap_or(0)
}

pub fn instance_buffer_bytes(total_slots: u32, instance_byte_size: u64) -> u64 {
    total_slots.max(1) as u64 * instance_byte_size
}

pub fn next_pow2_at_least(value: u32) -> u32 {
    if value <= 1 {
        return 1;
    }
    let mut cap = 1u32;
    while cap < value {
        cap = cap.saturating_mul(2);
    }
    cap
}

#[cfg(test)]
mod tests {
    use super::{
        coalesce_free_regions, next_pow2_at_least, region_overlaps_active,
        required_allocation, ScratchGrowResult, ScratchRegionAllocator,
    };

    #[test]
    fn free_regions_coalesce_and_split() {
        let mut regions = vec![(0, 2048), (2048, 2048)];
        coalesce_free_regions(&mut regions);
        assert_eq!(regions, vec![(0, 4096)]);
        regions = vec![(0, 4096)];
        let (off, size) = regions.remove(0);
        assert_eq!(off, 0);
        regions.push((off + 2048, size - 2048));
        coalesce_free_regions(&mut regions);
        assert_eq!(regions, vec![(2048, 2048)]);
    }

    #[test]
    fn compact_layout_packs_active_slots() {
        let mut alloc = ScratchRegionAllocator::new(4, 2048, 16_384, 1024 * 1024, 64, false);
        alloc.slot_capacities = vec![2048, 0, 4096, 2048];
        alloc.slot_offsets = vec![100000, 0, 500000, 900000];
        alloc.compact_layout();
        assert_eq!(alloc.slot_offsets, vec![0, 0, 2048, 6144]);
        assert_eq!(alloc.arena_high_water(), 8192);
    }

    #[test]
    fn grow_relocates_when_region_would_overlap() {
        let caps = vec![2048u32, 2048, 0];
        let offsets = vec![0u32, 2048, 0];
        assert!(region_overlaps_active(&caps, &offsets, 0, 0, 4096));
        assert!(!region_overlaps_active(&caps, &offsets, 0, 0, 2048));
    }

    #[test]
    fn stable_offsets_survive_release() {
        let mut alloc = ScratchRegionAllocator::new(3, 2048, 16_384, 1024 * 1024, 64, false);
        alloc.slot_capacities = vec![2048, 4096, 2048];
        alloc.slot_offsets = vec![0, 2048, 6144];
        assert_eq!(alloc.arena_high_water(), 8192);
        alloc.release_slot(1);
        alloc.init_slot(1);
        assert_eq!(alloc.slot_offsets[1], 2048);
    }

    #[test]
    fn next_pow2_at_least_values() {
        assert_eq!(next_pow2_at_least(1), 1);
        assert_eq!(next_pow2_at_least(2049), 4096);
        assert_eq!(next_pow2_at_least(4096), 4096);
        assert_eq!(next_pow2_at_least(5000), 8192);
    }

    #[test]
    fn required_allocation_amortizes_growth() {
        let max = 4_194_304u32;
        assert_eq!(required_allocation(100, 32_768, max), 32_768);
        assert_eq!(required_allocation(40_000, 32_768, max), 65_536);
        assert_eq!(required_allocation(100_000, 65_536, max), 131_072);
        assert_eq!(required_allocation(4_200_448, 4_194_304, max), 4_194_304);
    }

    #[test]
    fn grow_result_distinct() {
        assert_ne!(ScratchGrowResult::Grown, ScratchGrowResult::AtCapacity);
    }
}
