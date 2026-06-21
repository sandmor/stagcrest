use stagcrest_protocol::{BlockPos, BlockState, TorchAttachment, torch_state};

/// Whether a torch at `place_pos` has solid support for the given attachment.
pub fn torch_can_attach(
    is_solid_at: impl Fn(i32, i32, i32) -> bool,
    place_pos: BlockPos,
    attachment: TorchAttachment,
) -> bool {
    let (dx, dy, dz) = attachment.support_offset();
    is_solid_at(place_pos.x + dx, place_pos.y + dy, place_pos.z + dz)
}

/// Build initial torch block state from the hit face normal (into the torch cell).
pub fn torch_state_from_normal(nx: i32, ny: i32, nz: i32) -> Option<BlockState> {
    let attachment = TorchAttachment::from_place_normal(nx, ny, nz)?;
    Some(torch_state(false, attachment))
}

pub fn validate_torch_placement(
    is_solid_at: impl Fn(i32, i32, i32) -> bool,
    place_pos: BlockPos,
    nx: i32,
    ny: i32,
    nz: i32,
) -> Option<BlockState> {
    let attachment = TorchAttachment::from_place_normal(nx, ny, nz)?;
    if !torch_can_attach(is_solid_at, place_pos, attachment) {
        return None;
    }
    Some(torch_state(false, attachment))
}
