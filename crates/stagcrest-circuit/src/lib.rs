//! Event-driven spatial graph interpreter for redstone circuits.
//!
//! The world grid is the graph: each block with a [`stagcrest_protocol::CircuitNodeDef`] is a
//! node, and edges are the six face-adjacent neighbors. [`CircuitWorld`] runs at a fixed tick
//! rate and propagates power through the graph in two ways:
//!
//! - **Combinatorial nodes** (source, wire, switch, inverter) publish power immediately when
//!   evaluated.
//! - **Sequential nodes** (delay, repeater) arm a timer on input edges and publish later.
//!
//! Runtime power lives in [`CircuitWorld`] separately from block state; block state carries
//! orientation, delay setting, and visual powered bits.

mod eval;
mod event;
mod init;
mod world;

pub use eval::{is_player_toggleable, is_repeater};
pub use init::init_circuit_blocks;
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
