use std::collections::HashMap;

use stagcrest_protocol::{BlockDef, BlockId, CHUNK_SIZE};

use crate::color::{
    is_map_visible, is_surface_plant, resolve_column_color, MinimapBiomeClimate, MinimapColorCtx,
    UNLOADED_COLOR,
};
use crate::column::ColumnBlockSource;
use crate::column_cache::ColumnCache;
use crate::column_slice::ColumnSlice;
use crate::raster::strip_biome_at;
use crate::strip::StorageStripSource;

#[derive(Clone)]
pub struct BlockDefTable {
    defs: Vec<BlockDef>,
    index: HashMap<BlockId, usize>,
}

impl BlockDefTable {
    pub fn new(defs: Vec<BlockDef>) -> Self {
        let mut index = HashMap::new();
        for (i, d) in defs.iter().enumerate() {
            index.insert(d.id, i);
        }
        Self { defs, index }
    }

    pub fn get(&self, id: BlockId) -> Option<&BlockDef> {
        self.index.get(&id).map(|&i| &self.defs[i])
    }

    pub fn defs(&self) -> &[BlockDef] {
        &self.defs
    }
}

pub struct ColumnResolver<'a> {
    pub table: &'a BlockDefTable,
    pub air: BlockId,
    pub y_min: i32,
    pub y_max: i32,
    pub color_ctx: MinimapColorCtx<'a>,
}

impl<'a> ColumnResolver<'a> {
    pub fn resolve_slice(&self, slice: &ColumnSlice) -> ([u8; 3], bool) {
        if !slice.loaded {
            return (UNLOADED_COLOR, false);
        }
        let Some((id, _, _)) = self.surface_block_y(slice, slice.wx, slice.wz) else {
            return (UNLOADED_COLOR, false);
        };
        let Some(def) = self.table.get(id) else {
            return (UNLOADED_COLOR, true);
        };
        (
            resolve_column_color(def, slice.biome, &self.color_ctx),
            true,
        )
    }

    pub fn resolve_batch(&self, slices: &[ColumnSlice]) -> Vec<(i32, i32, [u8; 3], bool)> {
        slices
            .iter()
            .map(|s| {
                let (rgb, loaded) = self.resolve_slice(s);
                (s.wx, s.wz, rgb, loaded)
            })
            .collect()
    }

    /// Fill a transient cache for one horizontal strip (server export).
    pub fn resolve_strip(
        &self,
        strip: &StorageStripSource,
        cx: i32,
        cz: i32,
        biome_at: &impl Fn(i32, i32, i32) -> MinimapBiomeClimate,
    ) -> ColumnCache {
        let mut cache = ColumnCache::new(256);
        if strip.is_empty() {
            return cache;
        }
        let base_x = cx * CHUNK_SIZE;
        let base_z = cz * CHUNK_SIZE;
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let wx = base_x + lx;
                let wz = base_z + lz;
                let biome = biome_at(wx, self.y_max, wz);
                let slice =
                    ColumnSlice::capture(strip, wx, wz, self.y_min, self.y_max, biome, true);
                let (rgb, loaded) = self.resolve_slice(&slice);
                cache.insert(wx, wz, rgb, loaded);
            }
        }
        cache
    }

    fn surface_block_y(
        &self,
        source: &impl ColumnBlockSource,
        wx: i32,
        wz: i32,
    ) -> Option<(BlockId, stagcrest_protocol::BlockState, i32)> {
        for y in (self.y_min..=self.y_max).rev() {
            let (id, state) = source.block_at(wx, y, wz);
            if let Some(def) = self.table.get(id) {
                if is_surface_plant(def) {
                    continue;
                }
                if is_map_visible(def, self.air, id) {
                    return Some((id, state, y));
                }
            } else if id != self.air {
                return Some((id, state, y));
            }
        }
        None
    }
}

pub fn strip_biome_fn<'a>(
    strip: &'a StorageStripSource,
    climate_for_index: &'a impl Fn(u16) -> MinimapBiomeClimate,
) -> impl Fn(i32, i32, i32) -> MinimapBiomeClimate + 'a {
    move |wx, wy, wz| strip_biome_at(strip, wx, wy, wz, climate_for_index)
}
