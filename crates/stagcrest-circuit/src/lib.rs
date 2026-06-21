mod event;
mod init;
mod node;
mod world;

pub use init::init_circuit_blocks;
pub use node::is_player_toggleable;
pub use world::{CircuitWorld, MAX_EVALS_PER_TICK};

use stagcrest_protocol::BlockPos;

pub(crate) fn neighbors(pos: BlockPos) -> [BlockPos; 6] {
    [
        BlockPos::new(pos.x + 1, pos.y, pos.z),
        BlockPos::new(pos.x - 1, pos.y, pos.z),
        BlockPos::new(pos.x, pos.y + 1, pos.z),
        BlockPos::new(pos.x, pos.y - 1, pos.z),
        BlockPos::new(pos.x, pos.y, pos.z + 1),
        BlockPos::new(pos.x, pos.y, pos.z - 1),
    ]
}
