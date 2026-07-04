//! Event-driven circuit simulator on the world grid.
//!
//! Each block with a [`stagcrest_protocol::CircuitNodeDef`] is a node. Power propagates via:
//! - **Node signals** — directed links between circuit cells (wire lines, repeater faces).
//! - **Block power** — strong/weak levels on opaque block cells (feeds attachment-based inverters).
//!
//! Combinatorial nodes (source, wire, switch) publish immediately; sequential nodes
//! (inverter, repeater) arm tick delays on input changes.

mod eval;
mod event;
mod init;
mod piston;
mod power;
mod registry;
pub mod wire_network;
mod world;

pub use eval::{is_button_geometry, is_observer, is_piston, is_player_toggleable, is_repeater};
pub use init::init_circuit_blocks;
pub use power::{block_power_at, signal_into, BlockPower};
pub use wire_network::{compute_wire_connections, WireConnections, WireLink};
pub use world::{CircuitWorld, MAX_EVALS_PER_TICK};

pub use stagcrest_storage::ChunkCircuitSnapshot;

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
