use std::collections::{HashMap, HashSet, VecDeque};

use stagcrest_net::{GameMessage, MapChunkSnapshot, MapViewSubscribe, ServerMessage};
use stagcrest_storage::RedbChunkStorage;
use stagcrest_world::World;

use crate::map_generation::{is_map_chunk_fresh, MapChunkPipeline};

pub const MAX_MAP_SNAPSHOT_SEND_PER_TICK: usize = 8;

enum BlobStatus {
    Fresh(Vec<u8>),
    Missing,
    Stale,
}

fn blob_status(
    mx: i32,
    mz: i32,
    world: &World,
    y_chunks: &[i32],
    storage: &RedbChunkStorage,
    blob_cache: &mut ServerBlobCache,
) -> BlobStatus {
    if !is_map_chunk_fresh(mx, mz, world, y_chunks, storage) {
        return BlobStatus::Stale;
    }
    if let Some(blob) = blob_cache.resolve_blob(mx, mz, storage) {
        return BlobStatus::Fresh(blob);
    }
    BlobStatus::Missing
}

/// Last encoded `MapChunkBlob` per map tile — avoids redb reads on regen fan-out.
#[derive(Default)]
pub struct ServerBlobCache {
    pub blobs: HashMap<(i32, i32), Vec<u8>>,
}

impl ServerBlobCache {
    pub fn insert(&mut self, mx: i32, mz: i32, blob: Vec<u8>) {
        self.blobs.insert((mx, mz), blob);
    }

    pub fn resolve_blob(
        &mut self,
        mx: i32,
        mz: i32,
        storage: &RedbChunkStorage,
    ) -> Option<Vec<u8>> {
        if let Some(blob) = self.blobs.get(&(mx, mz)).cloned() {
            return Some(blob);
        }
        let blob = storage.get_map_chunk(mx, mz).ok().flatten()?;
        self.blobs.insert((mx, mz), blob.clone());
        Some(blob)
    }
}

#[derive(Default)]
pub struct ClientMapState {
    pub active: bool,
    pub subscribed: HashSet<(i32, i32)>,
    pending_send: VecDeque<(i32, i32)>,
    pending_send_set: HashSet<(i32, i32)>,
}

impl ClientMapState {
    pub fn reset(&mut self) {
        self.active = false;
        self.subscribed.clear();
        self.pending_send.clear();
        self.pending_send_set.clear();
    }

    fn enqueue_send(&mut self, mx: i32, mz: i32) {
        if self.pending_send_set.insert((mx, mz)) {
            self.pending_send.push_back((mx, mz));
        }
    }

    pub fn handle_subscribe(&mut self, sub: MapViewSubscribe) {
        if !sub.active {
            self.reset();
            return;
        }
        self.active = true;
        let new_set: HashSet<(i32, i32)> = sub.tiles.into_iter().collect();
        let to_enqueue: Vec<_> = new_set.difference(&self.subscribed).copied().collect();
        for (mx, mz) in to_enqueue {
            self.enqueue_send(mx, mz);
        }
        self.subscribed = new_set;
    }

    pub fn is_subscribed(&self, mx: i32, mz: i32) -> bool {
        self.active && self.subscribed.contains(&(mx, mz))
    }

    pub fn drain_pending(
        &mut self,
        limit: usize,
        blob_cache: &mut ServerBlobCache,
        storage: &RedbChunkStorage,
        world: &World,
        y_chunks: &[i32],
        pipeline: &mut MapChunkPipeline,
    ) -> Vec<MapChunkSnapshot> {
        let mut out = Vec::new();
        let mut attempts = self.pending_send.len();
        while out.len() < limit && attempts > 0 {
            attempts -= 1;
            let Some((mx, mz)) = self.pending_send.pop_front() else {
                break;
            };
            self.pending_send_set.remove(&(mx, mz));
            if !self.subscribed.contains(&(mx, mz)) {
                continue;
            }
            match blob_status(mx, mz, world, y_chunks, storage, blob_cache) {
                BlobStatus::Fresh(blob) => {
                    out.push(MapChunkSnapshot {
                        mx,
                        mz,
                        compressed: blob,
                    });
                }
                BlobStatus::Missing | BlobStatus::Stale => {
                    self.enqueue_send(mx, mz);
                    pipeline.ensure_fresh(mx, mz, world, y_chunks, storage);
                }
            }
        }
        out
    }

    pub fn fan_out_regen(
        &self,
        mx: i32,
        mz: i32,
        blob: Vec<u8>,
    ) -> Option<GameMessage> {
        if !self.is_subscribed(mx, mz) {
            return None;
        }
        Some(GameMessage::Server(ServerMessage::MapChunkSnapshot(
            MapChunkSnapshot {
                mx,
                mz,
                compressed: blob,
            },
        )))
    }
}
