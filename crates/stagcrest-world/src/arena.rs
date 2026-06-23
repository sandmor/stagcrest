use lru::LruCache;
use slab::Slab;
use stagcrest_protocol::ChunkPos;
use stagcrest_storage::InactiveChunk;
use std::collections::HashMap;
use std::num::NonZeroUsize;

use crate::chunk::Chunk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkMeta {
    pub generated: bool,
    pub modified: bool,
}

impl Default for ChunkMeta {
    fn default() -> Self {
        Self {
            generated: false,
            modified: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkSlot {
    Inactive,
    Active(usize),
}

#[derive(Debug)]
pub struct ChunkArena {
    lru: LruCache<ChunkPos, InactiveChunk>,
    active: Slab<Chunk>,
    slots: HashMap<ChunkPos, ChunkSlot>,
    meta: HashMap<ChunkPos, ChunkMeta>,
}

impl ChunkArena {
    pub fn with_capacity(lru_capacity: usize) -> Self {
        let cap = NonZeroUsize::new(lru_capacity.max(1)).unwrap();
        Self {
            lru: LruCache::new(cap),
            active: Slab::new(),
            slots: HashMap::new(),
            meta: HashMap::new(),
        }
    }

    pub fn contains(&self, pos: ChunkPos) -> bool {
        self.slots.contains_key(&pos)
    }

    pub fn is_generated(&self, pos: ChunkPos) -> bool {
        self.meta.get(&pos).is_some_and(|m| m.generated)
    }

    pub fn mark_generated(&mut self, pos: ChunkPos) {
        self.meta.entry(pos).or_default().generated = true;
    }

    pub fn mark_modified(&mut self, pos: ChunkPos) {
        let entry = self.meta.entry(pos).or_default();
        entry.modified = true;
    }

    pub fn insert_inactive(&mut self, pos: ChunkPos, chunk: InactiveChunk) -> bool {
        if let Some(ChunkSlot::Active(token)) = self.slots.remove(&pos) {
            self.active.remove(token);
        }
        let is_new = !self.lru.contains(&pos);
        if is_new && self.lru.len() == self.lru.cap().get() {
            if let Some((evicted_pos, _)) = self.lru.pop_lru() {
                self.slots.remove(&evicted_pos);
                self.meta.remove(&evicted_pos);
            }
        }
        let evicted = self.lru.put(pos, chunk).is_some();
        self.slots.insert(pos, ChunkSlot::Inactive);
        evicted
    }

    pub fn inactive(&self, pos: ChunkPos) -> Option<&InactiveChunk> {
        match self.slots.get(&pos) {
            Some(ChunkSlot::Inactive) => self.lru.peek(&pos),
            _ => None,
        }
    }

    pub fn active(&self, pos: ChunkPos) -> Option<&Chunk> {
        match self.slots.get(&pos) {
            Some(&ChunkSlot::Active(token)) => self.active.get(token),
            _ => None,
        }
    }

    pub fn active_mut(&mut self, pos: ChunkPos) -> Option<&mut Chunk> {
        match self.slots.get(&pos) {
            Some(&ChunkSlot::Active(token)) => self.active.get_mut(token),
            _ => None,
        }
    }

    pub fn promote_to_active(&mut self, pos: ChunkPos) -> Option<&mut Chunk> {
        let inactive = match self.slots.get(&pos) {
            Some(ChunkSlot::Inactive) => self.lru.pop(&pos)?,
            Some(ChunkSlot::Active(token)) => return self.active.get_mut(*token),
            None => return None,
        };
        let chunk = Chunk::from_inactive(&inactive).ok()?;
        let token = self.active.insert(chunk);
        self.slots.insert(pos, ChunkSlot::Active(token));
        self.active.get_mut(token)
    }

    pub fn ensure_inactive(&mut self, pos: ChunkPos, air: stagcrest_protocol::BlockId) {
        match self.slots.get(&pos) {
            Some(ChunkSlot::Active(_)) => return,
            Some(ChunkSlot::Inactive) if self.lru.peek(&pos).is_some() => return,
            Some(ChunkSlot::Inactive) => {
                let _ = self.remove(pos);
            }
            None => {}
        }
        let mut chunk = Chunk::new();
        chunk.fill(air, stagcrest_protocol::BlockState(0));
        if let Ok(inactive) = chunk.to_inactive() {
            self.insert_inactive(pos, inactive);
        }
    }

    pub fn pack_for_storage(&self, pos: ChunkPos) -> Option<InactiveChunk> {
        match self.slots.get(&pos) {
            Some(&ChunkSlot::Active(token)) => self.active.get(token)?.to_inactive().ok(),
            Some(ChunkSlot::Inactive) => self.lru.peek(&pos).cloned(),
            None => None,
        }
    }

    pub fn remove(&mut self, pos: ChunkPos) -> Option<(Option<InactiveChunk>, ChunkMeta)> {
        let meta = self.meta.remove(&pos).unwrap_or_default();
        let packed = match self.slots.remove(&pos)? {
            ChunkSlot::Inactive => self.lru.pop(&pos),
            ChunkSlot::Active(token) => {
                let chunk = self.active.remove(token);
                chunk.to_inactive().ok()
            }
        };
        Some((packed, meta))
    }

    pub fn loaded_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.slots.keys().copied()
    }
}
