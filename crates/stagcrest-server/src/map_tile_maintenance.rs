use std::collections::{HashMap, HashSet};

use rayon::prelude::*;
use stagcrest_minimap::{
    build_map_chunk_blob, world_chunk_to_map_chunk, MapChunkLoadInput, MapResolveContext,
};
use stagcrest_protocol::{BlockId, ChunkPos};
use stagcrest_storage::{BlockIdRemap, RedbChunkStorage};
use thiserror::Error;

/// Report from a full-world map tile rebuild.
pub struct MapTileRebuildReport {
    pub world_chunks: usize,
    pub map_tiles: usize,
    pub rebuilt: usize,
}

#[derive(Debug, Error)]
pub enum MapTileError {
    #[error("storage: {0}")]
    Storage(#[from] stagcrest_storage::StorageError),
    #[error("map: {0}")]
    Map(#[from] stagcrest_minimap::MapBuildError),
    #[error("{0}")]
    Message(String),
}

/// Tracks map tiles needing rebuild after world-chunk writes.
#[derive(Debug, Default)]
pub struct MapTileDirtySet {
    tiles: HashSet<(i32, i32)>,
}

impl MapTileDirtySet {
    pub fn mark_chunk(&mut self, pos: ChunkPos) {
        let (mx, mz) = world_chunk_to_map_chunk(pos.x, pos.z);
        self.tiles.insert((mx, mz));
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Rebuild all dirty tiles once (parallel rasterize, sequential writes), then clear.
    pub fn rebuild_all(
        &mut self,
        storage: &RedbChunkStorage,
        map_ctx: &MapResolveContext,
        y_chunks: &[i32],
        air: BlockId,
        block_remap: Option<&BlockIdRemap>,
    ) -> Result<usize, MapTileError> {
        let tiles: Vec<(i32, i32)> = self.tiles.drain().collect();
        rebuild_map_tiles(&tiles, storage, map_ctx, y_chunks, air, block_remap)
    }
}

/// Run `f` on a rayon pool with optional thread count.
pub fn with_rayon_pool<R: Send>(jobs: Option<usize>, f: impl FnOnce() -> R + Send) -> R {
    if let Some(n) = jobs {
        match rayon::ThreadPoolBuilder::new().num_threads(n).build() {
            Ok(pool) => return pool.install(f),
            Err(err) => {
                tracing::warn!("invalid --jobs {n}, using default thread pool: {err}");
            }
        }
    }
    f()
}

/// Rebuild a single map tile from storage (no live world).
pub fn rebuild_map_tile(
    mx: i32,
    mz: i32,
    storage: &RedbChunkStorage,
    map_ctx: &MapResolveContext,
    y_chunks: &[i32],
    air: BlockId,
    block_remap: Option<&BlockIdRemap>,
) -> Result<(), MapTileError> {
    let empty_overrides = HashMap::new();
    let empty_modified = HashSet::new();
    let input = MapChunkLoadInput {
        storage,
        world: None,
        y_chunks,
        mx,
        mz,
        air,
        block_remap,
        overrides: &empty_overrides,
        modified_live: &empty_modified,
    };
    let blob = build_map_chunk_blob(&input, map_ctx)?;
    storage.put_map_chunk(mx, mz, &blob)?;
    Ok(())
}

/// Rebuild explicit map tiles (parallel rasterize, sequential DB writes).
pub fn rebuild_map_tiles(
    tiles: &[(i32, i32)],
    storage: &RedbChunkStorage,
    map_ctx: &MapResolveContext,
    y_chunks: &[i32],
    air: BlockId,
    block_remap: Option<&BlockIdRemap>,
) -> Result<usize, MapTileError> {
    if tiles.is_empty() {
        return Ok(0);
    }
    if tiles.len() == 1 {
        let (mx, mz) = tiles[0];
        rebuild_map_tile(mx, mz, storage, map_ctx, y_chunks, air, block_remap)?;
        return Ok(1);
    }

    let empty_overrides = HashMap::new();
    let empty_modified = HashSet::new();

    let built: Vec<(i32, i32, Vec<u8>)> = tiles
        .par_iter()
        .map(|&(mx, mz)| {
            let input = MapChunkLoadInput {
                storage,
                world: None,
                y_chunks,
                mx,
                mz,
                air,
                block_remap,
                overrides: &empty_overrides,
                modified_live: &empty_modified,
            };
            let blob = build_map_chunk_blob(&input, map_ctx)?;
            Ok((mx, mz, blob))
        })
        .collect::<Result<Vec<_>, MapTileError>>()?;

    for (mx, mz, blob) in built {
        storage.put_map_chunk(mx, mz, &blob)?;
    }

    Ok(tiles.len())
}

/// Derive unique map tile coords covering a set of world chunk columns.
pub fn map_tiles_from_chunk_positions(positions: &[ChunkPos]) -> Vec<(i32, i32)> {
    let mut map_coords: HashSet<(i32, i32)> = HashSet::new();
    for pos in positions {
        map_coords.insert(world_chunk_to_map_chunk(pos.x, pos.z));
    }
    map_coords.into_iter().collect()
}

/// Full-world scan: derive map tiles from all saved world chunk positions (single scan).
pub fn rebuild_all_map_tiles(
    storage: &RedbChunkStorage,
    map_ctx: &MapResolveContext,
    y_chunks: &[i32],
    air: BlockId,
    block_remap: Option<&BlockIdRemap>,
) -> Result<MapTileRebuildReport, MapTileError> {
    let positions = storage.iter_chunk_positions()?;
    if positions.is_empty() {
        return Err(MapTileError::Message(
            "no saved chunks in world — explore the world first".into(),
        ));
    }

    let tiles = map_tiles_from_chunk_positions(&positions);
    let rebuilt = rebuild_map_tiles(&tiles, storage, map_ctx, y_chunks, air, block_remap)?;
    Ok(MapTileRebuildReport {
        world_chunks: positions.len(),
        map_tiles: tiles.len(),
        rebuilt,
    })
}
