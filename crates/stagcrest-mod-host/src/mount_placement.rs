use stagcrest_protocol::{
    mount_from_placement, mount_state, mount_support_offset, AttachFace, BlockPos, BlockState,
    Facing,
};

/// Whether a mounted block at `place_pos` has solid support for the given
/// attachment.
pub fn mount_can_attach(
    is_solid_at: impl Fn(i32, i32, i32) -> bool,
    place_pos: BlockPos,
    face: AttachFace,
    facing: Facing,
) -> bool {
    let (dx, dy, dz) = mount_support_offset(face, facing);
    is_solid_at(place_pos.x + dx, place_pos.y + dy, place_pos.z + dz)
}

/// Validate placement of a lever/button from the clicked face normal and the
/// player's horizontal look direction, returning the initial (unpowered) state.
pub fn validate_mount_placement(
    is_solid_at: impl Fn(i32, i32, i32) -> bool,
    place_pos: BlockPos,
    nx: i32,
    ny: i32,
    nz: i32,
    look_x: f32,
    look_z: f32,
) -> Option<BlockState> {
    let (face, facing) = mount_from_placement(nx, ny, nz, look_x, look_z)?;
    if !mount_can_attach(&is_solid_at, place_pos, face, facing) {
        return None;
    }
    Some(mount_state(false, face, facing))
}
