use stagcrest_protocol::{floor_div, CHUNK_SIZE};

pub const MAP_CHUNK_SIZE: i32 = 64;
pub const MAP_CHUNK_PIXELS: usize = (MAP_CHUNK_SIZE * MAP_CHUNK_SIZE) as usize;
pub const MAP_CHUNK_RGB_BYTES: usize = MAP_CHUNK_PIXELS * 3;
pub const WORLD_CHUNKS_PER_MAP: i32 = MAP_CHUNK_SIZE / CHUNK_SIZE;

/// Map chunk coordinate for a world chunk column `(cx, cz)`.
pub fn world_chunk_to_map_chunk(cx: i32, cz: i32) -> (i32, i32) {
    (
        floor_div(cx, WORLD_CHUNKS_PER_MAP),
        floor_div(cz, WORLD_CHUNKS_PER_MAP),
    )
}

/// World block XZ bounds covered by map chunk `(mx, mz)` as `(x0, z0, x1, z1)` inclusive.
pub fn map_chunk_block_bounds(mx: i32, mz: i32) -> (i32, i32, i32, i32) {
    let x0 = mx * MAP_CHUNK_SIZE;
    let z0 = mz * MAP_CHUNK_SIZE;
    (
        x0,
        z0,
        x0 + MAP_CHUNK_SIZE - 1,
        z0 + MAP_CHUNK_SIZE - 1,
    )
}

/// Base world chunk coordinates for the 4×4 strip grid inside map chunk `(mx, mz)`.
pub fn map_chunk_world_chunk_base(mx: i32, mz: i32) -> (i32, i32) {
    (mx * WORLD_CHUNKS_PER_MAP, mz * WORLD_CHUNKS_PER_MAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_chunk_to_map_chunk_floor_div() {
        assert_eq!(world_chunk_to_map_chunk(0, 0), (0, 0));
        assert_eq!(world_chunk_to_map_chunk(3, 3), (0, 0));
        assert_eq!(world_chunk_to_map_chunk(4, 8), (1, 2));
        assert_eq!(world_chunk_to_map_chunk(-1, -1), (-1, -1));
    }
}
