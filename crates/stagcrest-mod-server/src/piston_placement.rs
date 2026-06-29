use stagcrest_protocol::{piston_state, BlockPos, BlockState, Facing6};

/// Place a piston oriented by player look; front points away from the player.
pub fn validate_piston_placement(
    _place_pos: BlockPos,
    look_x: f32,
    look_y: f32,
    look_z: f32,
) -> Option<BlockState> {
    let facing = Facing6::from_look(look_x, look_y, look_z);
    Some(piston_state(false, facing))
}
