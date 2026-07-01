use stagcrest_protocol::CHUNK_SIZE;

use crate::color::{biome_index_at, MinimapBiomeClimate};
use crate::strip::StorageStripSource;

pub fn strip_biome_at(
    strip: &StorageStripSource,
    wx: i32,
    wy: i32,
    wz: i32,
    climate_for_index: &impl Fn(u16) -> MinimapBiomeClimate,
) -> MinimapBiomeClimate {
    let cy = stagcrest_protocol::floor_div(wy, CHUNK_SIZE);
    let cx = stagcrest_protocol::floor_div(wx, CHUNK_SIZE);
    let cz = stagcrest_protocol::floor_div(wz, CHUNK_SIZE);
    let origin_x = cx * CHUNK_SIZE;
    let origin_y = cy * CHUNK_SIZE;
    let origin_z = cz * CHUNK_SIZE;
    strip
        .biome_grid_at(wx, wy, wz)
        .map(|grid| {
            let idx = biome_index_at(grid, wx, wy, wz, origin_x, origin_y, origin_z);
            climate_for_index(idx)
        })
        .unwrap_or_default()
}
