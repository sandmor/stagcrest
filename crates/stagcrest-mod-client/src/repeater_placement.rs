use stagcrest_protocol::{repeater_state, BlockPos, BlockState, Facing};

/// Place a repeater on the top face of a solid block, oriented by player look.
pub fn validate_repeater_placement(
    is_solid_at: impl Fn(i32, i32, i32) -> bool,
    place_pos: BlockPos,
    nx: i32,
    ny: i32,
    nz: i32,
    look_x: f32,
    look_z: f32,
) -> Option<BlockState> {
    if (nx, ny, nz) != (0, 1, 0) {
        return None;
    }
    if !is_solid_at(place_pos.x, place_pos.y - 1, place_pos.z) {
        return None;
    }
    let facing = Facing::from_horizontal(look_x, look_z);
    Some(repeater_state(false, facing, 2))
}
