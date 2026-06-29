use crate::power::signal_into;
use crate::registry::BlockRegistry;
use crate::wire_network::{
    wire_connections_at, wire_connects_toward_neighbor, WireConnections, WireLink,
};
use stagcrest_protocol::BlockPos;
use stagcrest_world::World;

use crate::eval::EvalContext;
use crate::world::CircuitWorld;

pub fn max_wire_input(ctx: &EvalContext<'_>, falloff: u8) -> u8 {
    max_wire_input_at(
        ctx.pos,
        ctx.circuit,
        ctx.world,
        ctx.registry,
        falloff,
    )
}

pub fn max_wire_input_at(
    wire_pos: BlockPos,
    circuit: &CircuitWorld,
    world: &World,
    registry: &BlockRegistry,
    falloff: u8,
) -> u8 {
    let connections = wire_connections_at(registry, world, wire_pos);
    let mut max_power = 0u8;

    for (i, &(dx, dz)) in WireConnections::DIRECTIONS.iter().enumerate() {
        match connections.side(i) {
            WireLink::None => {}
            WireLink::Side => {
                let neighbor = BlockPos::new(wire_pos.x + dx, wire_pos.y, wire_pos.z + dz);
                max_power = max_power.max(signal_into(circuit, neighbor, wire_pos, world, registry));
                let below = BlockPos::new(wire_pos.x + dx, wire_pos.y - 1, wire_pos.z + dz);
                if wire_connects_toward_neighbor(registry, world, below, wire_pos) {
                    max_power = max_power.max(signal_into(circuit, below, wire_pos, world, registry));
                }
            }
            WireLink::Up => {
                let neighbor = BlockPos::new(wire_pos.x + dx, wire_pos.y + 1, wire_pos.z + dz);
                max_power = max_power.max(signal_into(circuit, neighbor, wire_pos, world, registry));
            }
        }
    }

    // Signal from a wire cell directly below into the bottom of this cell.
    let below = BlockPos::new(wire_pos.x, wire_pos.y - 1, wire_pos.z);
    if wire_connects_toward_neighbor(registry, world, below, wire_pos) {
        max_power = max_power.max(signal_into(circuit, below, wire_pos, world, registry));
    }

    max_power.saturating_sub(falloff)
}
