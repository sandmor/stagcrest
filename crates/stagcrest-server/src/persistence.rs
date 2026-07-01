use std::collections::HashSet;
use std::sync::Arc;

use stagcrest_circuit::CircuitWorld;
use stagcrest_mod_server::WorldGenState;
use stagcrest_protocol::ChunkPos;
use stagcrest_storage::{
    compress_stored, store_inactive_chunk, ChunkStorage, InactiveChunk, RedbChunkStorage,
    StorageError,
};
use stagcrest_world::World;

pub struct ChunkPersistence {
    dirty: HashSet<ChunkPos>,
    storage: Arc<RedbChunkStorage>,
}

impl ChunkPersistence {
    pub fn new(storage: Arc<RedbChunkStorage>) -> Self {
        Self {
            dirty: HashSet::new(),
            storage,
        }
    }

    pub fn mark_dirty(&mut self, pos: ChunkPos) {
        self.dirty.insert(pos);
    }

    pub fn absorb_dirty_chunks(&mut self, chunks: HashSet<ChunkPos>) {
        self.dirty.extend(chunks);
    }

    pub fn persist_inactive(
        &self,
        pos: ChunkPos,
        chunk: &InactiveChunk,
        stored_chunks: &mut HashSet<ChunkPos>,
    ) -> Result<(), StorageError> {
        store_inactive_chunk(self.storage.as_ref(), pos, chunk)?;
        stored_chunks.insert(pos);
        Ok(())
    }

    pub fn drain(
        &mut self,
        budget: usize,
        world: &mut World,
        terrain: &WorldGenState,
        circuit: &CircuitWorld,
        stored_chunks: &mut HashSet<ChunkPos>,
    ) -> Vec<ChunkPos> {
        let mut written = 0usize;
        let mut persisted = Vec::new();
        let dirty: Vec<_> = self.dirty.iter().copied().collect();
        self.dirty.clear();

        for pos in dirty {
            if written >= budget {
                self.dirty.insert(pos);
                continue;
            }
            if !world.has_chunk(pos) || !world.is_populated(pos) {
                continue;
            }
            if !world.is_modified(pos) && self.storage.contains(pos) {
                continue;
            }
            match persist_chunk(world, terrain, circuit, pos, self.storage.as_ref()) {
                Ok(()) => {
                    world.clear_modified(pos);
                    stored_chunks.insert(pos);
                    written += 1;
                    persisted.push(pos);
                }
                Err(err) => {
                    tracing::error!("failed to persist chunk {pos:?}: {err}");
                    self.dirty.insert(pos);
                }
            }
        }
        persisted
    }

    pub fn flush_all(
        &mut self,
        world: &mut World,
        terrain: &WorldGenState,
        circuit: &CircuitWorld,
        stored_chunks: &mut HashSet<ChunkPos>,
    ) {
        for pos in world.loaded_chunk_positions().collect::<Vec<_>>() {
            if world.is_populated(pos) {
                self.dirty.insert(pos);
            }
        }
        while self.drain(usize::MAX, world, terrain, circuit, stored_chunks)
            .into_iter()
            .next()
            .is_some()
        {}
    }
}

pub fn persist_chunk(
    world: &World,
    terrain: &WorldGenState,
    circuit: &CircuitWorld,
    pos: ChunkPos,
    storage: &RedbChunkStorage,
) -> Result<(), StorageError> {
    let Some(mut chunk) = world.pack_chunk_for_storage(pos) else {
        return Ok(());
    };
    let Some(biome) = terrain.biome_grid(pos) else {
        tracing::warn!("missing biome grid for populated chunk {pos:?}");
        return Ok(());
    };
    chunk.biome_grid = *biome;
    chunk.circuit = circuit.export_chunk_snapshot(pos);
    store_inactive_chunk(storage, pos, &chunk)
}

pub fn pack_evicted_chunk(
    mut inactive: InactiveChunk,
    terrain: &WorldGenState,
    circuit: &CircuitWorld,
    pos: ChunkPos,
) -> Option<InactiveChunk> {
    inactive.biome_grid = *terrain.biome_grid(pos)?;
    inactive.circuit = circuit.export_chunk_snapshot(pos);
    Some(inactive)
}

pub fn pack_network_chunk(
    world: &World,
    terrain: &WorldGenState,
    circuit: &CircuitWorld,
    pos: ChunkPos,
) -> Option<InactiveChunk> {
    let mut chunk = world.pack_chunk_for_storage(pos)?;
    chunk.biome_grid = terrain.biome_grid(pos).copied()?;
    chunk.circuit = circuit.export_chunk_snapshot(pos);
    Some(chunk)
}

pub fn compressed_chunk_wire(chunk: &InactiveChunk) -> Vec<u8> {
    compress_stored(&chunk.encode_wire())
}
