use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos, LocalBlockPos};
use stagcrest_storage::InactiveChunkReader;
use std::collections::HashSet;

use crate::arena::{ChunkArena, ChunkMeta};
use crate::chunk::{Chunk, ChunkBlock, ChunkView};
use stagcrest_storage::{load_inactive_chunk, store_inactive_chunk, ChunkStorage, InactiveChunk};

#[derive(Debug)]
pub struct World {
    arena: ChunkArena,
    pub dirty_chunks: HashSet<ChunkPos>,
    air: BlockId,
}

pub enum ChunkViewRef<'a> {
    Active(&'a Chunk),
    Inactive(InactiveChunkReader<'a>),
}

impl ChunkView for ChunkViewRef<'_> {
    fn get(&self, local: LocalBlockPos) -> ChunkBlock {
        match self {
            ChunkViewRef::Active(chunk) => chunk.get(local),
            ChunkViewRef::Inactive(reader) => ChunkView::get(reader, local),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EvictedChunk {
    pub pos: ChunkPos,
    pub inactive: Option<InactiveChunk>,
    pub meta: ChunkMeta,
}

impl World {
    pub fn new(air: BlockId) -> Self {
        Self::with_lru_capacity(4096, air)
    }

    pub fn with_lru_capacity(lru_capacity: usize, air: BlockId) -> Self {
        Self {
            arena: ChunkArena::with_capacity(lru_capacity),
            dirty_chunks: HashSet::new(),
            air,
        }
    }

    pub fn air(&self) -> BlockId {
        self.air
    }

    pub fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.arena.contains(pos)
    }

    pub fn is_generated(&self, pos: ChunkPos) -> bool {
        self.arena.is_generated(pos)
    }

    pub fn is_terrain_ready(&self, pos: ChunkPos) -> bool {
        self.arena.is_terrain_ready(pos)
    }

    pub fn mark_chunk_terrain_ready(&mut self, pos: ChunkPos) {
        self.arena.mark_terrain_ready(pos);
    }

    pub fn mark_chunk_generated(&mut self, pos: ChunkPos) {
        self.arena.mark_generated(pos);
    }

    pub fn is_populated(&self, pos: ChunkPos) -> bool {
        self.is_generated(pos)
    }

    pub fn is_modified(&self, pos: ChunkPos) -> bool {
        self.arena.is_modified(pos)
    }

    pub fn clear_modified(&mut self, pos: ChunkPos) {
        self.arena.clear_modified(pos);
    }

    pub fn modified_populated_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.arena.modified_populated_positions()
    }

    /// Chunks visible to player interaction, circuits, and similar gameplay systems.
    /// Requires the chunk to be loaded and fully populated (pass-2 complete).
    pub fn is_chunk_interactive(&self, pos: ChunkPos) -> bool {
        self.has_chunk(pos) && self.is_generated(pos)
    }

    pub fn ensure_chunk(&mut self, pos: ChunkPos) {
        self.arena.ensure_inactive(pos, self.air);
    }

    pub fn insert_inactive_chunk(
        &mut self,
        pos: ChunkPos,
        chunk: stagcrest_storage::InactiveChunk,
    ) -> crate::arena::InsertInactiveResult {
        let result = self.arena.insert_inactive(pos, chunk);
        self.arena.mark_generated(pos);
        self.mark_dirty_face_neighbors(pos);
        result
    }

    pub fn pack_chunk_for_storage(
        &self,
        pos: ChunkPos,
    ) -> Option<stagcrest_storage::InactiveChunk> {
        self.arena.pack_for_storage(pos)
    }

    pub fn remove_chunk(
        &mut self,
        pos: ChunkPos,
    ) -> Option<(
        Option<stagcrest_storage::InactiveChunk>,
        crate::arena::ChunkMeta,
    )> {
        self.arena.remove(pos)
    }

    pub fn get_block(&self, pos: BlockPos) -> (BlockId, BlockState) {
        let chunk_pos = pos.chunk_pos();
        let local = pos.local();
        match self.chunk_view(chunk_pos) {
            Some(view) => {
                let b = view.get(local);
                (b.id, b.state)
            }
            None => (self.air, BlockState(0)),
        }
    }

    pub fn chunk_view(&self, pos: ChunkPos) -> Option<ChunkViewRef<'_>> {
        if let Some(chunk) = self.arena.active(pos) {
            return Some(ChunkViewRef::Active(chunk));
        }
        self.arena
            .inactive(pos)
            .map(|inactive| ChunkViewRef::Inactive(InactiveChunkReader::new(inactive)))
    }

    pub fn get_block_if_loaded(&self, pos: BlockPos) -> Option<ChunkBlock> {
        if !self.has_chunk(pos.chunk_pos()) {
            return None;
        }
        let (id, state) = self.get_block(pos);
        Some(ChunkBlock { id, state })
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.arena.active(pos)
    }

    pub fn chunk_mut(&mut self, pos: ChunkPos) -> Option<&mut Chunk> {
        self.arena.active_mut(pos)
    }

    pub fn set_block(&mut self, pos: BlockPos, id: BlockId, state: BlockState) {
        let chunk_pos = pos.chunk_pos();
        let local = pos.local();
        if self.arena.active(chunk_pos).is_none() {
            if self.arena.contains(chunk_pos) && self.arena.inactive(chunk_pos).is_none() {
                let _ = self.arena.remove(chunk_pos);
            }
            self.arena.promote_to_active(chunk_pos);
        }
        if self.arena.active(chunk_pos).is_none() {
            self.arena.ensure_inactive(chunk_pos, self.air);
            self.arena.promote_to_active(chunk_pos);
        }
        if let Some(chunk) = self.arena.active_mut(chunk_pos) {
            chunk.set(local, id, state);
            self.arena.mark_modified(chunk_pos);
            self.mark_dirty_face_neighbors(chunk_pos);
        }
    }

    pub fn set_blocks(
        &mut self,
        blocks: impl IntoIterator<Item = (BlockPos, BlockId, BlockState)>,
    ) {
        let mut touched_chunks = HashSet::new();
        for (pos, id, state) in blocks {
            let chunk_pos = pos.chunk_pos();
            if self.arena.active(chunk_pos).is_none() {
                if self.arena.contains(chunk_pos) && self.arena.inactive(chunk_pos).is_none() {
                    let _ = self.arena.remove(chunk_pos);
                }
                self.arena.promote_to_active(chunk_pos);
            }
            if self.arena.active(chunk_pos).is_none() {
                self.arena.ensure_inactive(chunk_pos, self.air);
                self.arena.promote_to_active(chunk_pos);
            }
            if let Some(chunk) = self.arena.active_mut(chunk_pos) {
                chunk.set(pos.local(), id, state);
                touched_chunks.insert(chunk_pos);
            }
        }
        for chunk_pos in touched_chunks {
            self.mark_dirty_face_neighbors(chunk_pos);
        }
    }

    pub fn finalize_generated_chunk(&mut self, pos: ChunkPos) {
        self.arena.mark_generated(pos);
        if let Some(chunk) = self.arena.active_mut(pos) {
            if let Ok(inactive) = chunk.to_inactive() {
                let _ = self.arena.insert_inactive(pos, inactive);
            }
        } else if !self.has_chunk(pos) {
            self.ensure_chunk(pos);
        }
        self.mark_dirty_face_neighbors(pos);
    }

    pub fn mark_dirty_face_neighbors(&mut self, chunk_pos: ChunkPos) {
        for pos in Self::face_neighbor_chunks(chunk_pos) {
            self.dirty_chunks.insert(pos);
        }
    }

    pub fn face_neighbor_chunks(chunk_pos: ChunkPos) -> [ChunkPos; 7] {
        [
            chunk_pos,
            ChunkPos {
                x: chunk_pos.x + 1,
                y: chunk_pos.y,
                z: chunk_pos.z,
            },
            ChunkPos {
                x: chunk_pos.x - 1,
                y: chunk_pos.y,
                z: chunk_pos.z,
            },
            ChunkPos {
                x: chunk_pos.x,
                y: chunk_pos.y + 1,
                z: chunk_pos.z,
            },
            ChunkPos {
                x: chunk_pos.x,
                y: chunk_pos.y - 1,
                z: chunk_pos.z,
            },
            ChunkPos {
                x: chunk_pos.x,
                y: chunk_pos.y,
                z: chunk_pos.z + 1,
            },
            ChunkPos {
                x: chunk_pos.x,
                y: chunk_pos.y,
                z: chunk_pos.z - 1,
            },
        ]
    }

    pub fn clear_dirty(&mut self, chunk_pos: ChunkPos) {
        self.dirty_chunks.remove(&chunk_pos);
    }

    pub fn mark_dirty_and_neighbors(&mut self, pos: BlockPos) {
        self.mark_dirty_neighbors_of_chunk(pos.chunk_pos());
    }

    fn mark_dirty_neighbors_of_chunk(&mut self, chunk_pos: ChunkPos) {
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    self.dirty_chunks.insert(ChunkPos {
                        x: chunk_pos.x + dx,
                        y: chunk_pos.y + dy,
                        z: chunk_pos.z + dz,
                    });
                }
            }
        }
    }

    pub fn take_dirty_chunks(&mut self) -> HashSet<ChunkPos> {
        std::mem::take(&mut self.dirty_chunks)
    }

    pub fn loaded_chunk_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.arena.loaded_positions()
    }

    pub fn unload_far_chunks_3d(
        &mut self,
        center: ChunkPos,
        horizontal_radius: i32,
        vertical_radius: i32,
    ) -> (Vec<EvictedChunk>, usize, usize) {
        let positions: Vec<_> = self
            .arena
            .loaded_positions()
            .filter(|pos| {
                (pos.x - center.x).abs() > horizontal_radius
                    || (pos.z - center.z).abs() > horizontal_radius
                    || (pos.y - center.y).abs() > vertical_radius
            })
            .collect();
        let candidates = positions.len();
        let mut evicted = Vec::with_capacity(candidates);
        for pos in positions {
            if let Some((inactive, meta)) = self.arena.remove(pos) {
                evicted.push(EvictedChunk {
                    pos,
                    inactive,
                    meta,
                });
            }
        }
        let failed = candidates.saturating_sub(evicted.len());
        (evicted, candidates, failed)
    }

    pub fn persist_and_evict(
        &mut self,
        storage: &dyn ChunkStorage,
        evicted: EvictedChunk,
    ) -> Result<(), stagcrest_storage::StorageError> {
        if evicted.meta.is_populated() {
            if let Some(inactive) = evicted.inactive {
                store_inactive_chunk(storage, evicted.pos, &inactive)?;
            }
        }
        Ok(())
    }

    pub fn load_area_from_storage(
        &mut self,
        storage: &dyn ChunkStorage,
        center: ChunkPos,
        horizontal_radius: i32,
        vertical_radius: i32,
        y_bounds: std::ops::RangeInclusive<i32>,
    ) -> Result<Vec<ChunkPos>, stagcrest_storage::StorageError> {
        let y_min = (center.y - vertical_radius).max(*y_bounds.start());
        let y_max = (center.y + vertical_radius).min(*y_bounds.end());
        let mut loaded = Vec::new();
        for x in (center.x - horizontal_radius)..=(center.x + horizontal_radius) {
            for z in (center.z - horizontal_radius)..=(center.z + horizontal_radius) {
                for y in y_min..=y_max {
                    let pos = ChunkPos { x, y, z };
                    if self.has_chunk(pos) || !storage.contains(pos) {
                        continue;
                    }
                    if let Some(inactive) = load_inactive_chunk(storage, pos)? {
                        self.insert_inactive_chunk(pos, inactive);
                        loaded.push(pos);
                    }
                }
            }
        }
        Ok(loaded)
    }

    pub fn load_area_3d(
        &mut self,
        center: ChunkPos,
        horizontal_radius: i32,
        vertical_radius: i32,
        y_bounds: std::ops::RangeInclusive<i32>,
    ) {
        let y_min = (center.y - vertical_radius).max(*y_bounds.start());
        let y_max = (center.y + vertical_radius).min(*y_bounds.end());
        for x in (center.x - horizontal_radius)..=(center.x + horizontal_radius) {
            for z in (center.z - horizontal_radius)..=(center.z + horizontal_radius) {
                for y in y_min..=y_max {
                    self.ensure_chunk(ChunkPos { x, y, z });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::BlockPos;

    #[test]
    fn set_block_marks_face_neighbors_only() {
        let air = BlockId(0);
        let stone = BlockId(1);
        let mut world = World::new(air);
        let chunk = ChunkPos { x: 0, y: 0, z: 0 };
        world.ensure_chunk(chunk);

        world.set_blocks([(BlockPos::new(0, 0, 0), stone, BlockState(0))]);
        assert_eq!(world.dirty_chunks.len(), 7);
        assert!(world.dirty_chunks.contains(&chunk));
    }

    #[test]
    fn terrain_ready_chunk_is_not_interactive() {
        let air = BlockId(0);
        let stone = BlockId(1);
        let mut world = World::new(air);
        let pos = ChunkPos { x: 0, y: 0, z: 0 };
        world.set_blocks([(BlockPos::new(0, 0, 0), stone, BlockState(0))]);
        world.mark_chunk_terrain_ready(pos);
        assert!(world.has_chunk(pos));
        assert!(world.is_terrain_ready(pos));
        assert!(!world.is_generated(pos));
        assert!(!world.is_chunk_interactive(pos));
    }

    #[test]
    fn populated_chunk_is_interactive() {
        let air = BlockId(0);
        let stone = BlockId(1);
        let mut world = World::new(air);
        let pos = ChunkPos { x: 0, y: 0, z: 0 };
        world.set_blocks([(BlockPos::new(0, 0, 0), stone, BlockState(0))]);
        world.finalize_generated_chunk(pos);
        assert!(world.is_chunk_interactive(pos));
    }

    #[test]
    fn inactive_chunk_readable_before_edit() {
        let air = BlockId(0);
        let stone = BlockId(1);
        let mut world = World::new(air);
        let pos = ChunkPos { x: 0, y: 0, z: 0 };
        world.set_blocks([(BlockPos::new(0, 0, 0), stone, BlockState(0))]);
        world.finalize_generated_chunk(pos);
        assert!(world.chunk(pos).is_none());
        assert_eq!(world.get_block(BlockPos::new(0, 0, 0)).0, stone);
    }
}
