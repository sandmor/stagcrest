use stagcrest_mod_host::BlockRegistry;
use stagcrest_protocol::{facing_delta, repeater_facing, BlockPos, BlockState, CircuitKind};
use stagcrest_world::World;

use crate::world::CircuitWorld;

/// Power a `consumer` block receives from an adjacent `source` circuit node.
pub fn neighbor_output_into(
    circuit: &CircuitWorld,
    source: BlockPos,
    consumer: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> u8 {
    let (id, state) = world_blocks.get_block(source);
    let Some(def) = registry.block(id) else {
        return 0;
    };
    let Some(node) = def.circuit else {
        return 0;
    };

    match node.kind {
        CircuitKind::Source { level } => level,
        CircuitKind::Switch { output } => {
            if state.0 & 1 != 0 {
                output
            } else {
                0
            }
        }
        CircuitKind::Inverter { .. } | CircuitKind::Wire { .. } => circuit.power_at(source),
        CircuitKind::Delay { .. } => circuit.power_at(source),
        CircuitKind::Repeater { .. } => {
            let facing = repeater_facing(state);
            let (fx, _, fz) = facing_delta(facing);
            let output_pos = BlockPos::new(source.x + fx, source.y, source.z + fz);
            if consumer == output_pos {
                circuit.power_at(source)
            } else {
                0
            }
        }
    }
}

pub fn repeater_input_power(
    circuit: &CircuitWorld,
    pos: BlockPos,
    state: BlockState,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> u8 {
    let facing = repeater_facing(state);
    let (fx, _, fz) = facing_delta(facing);
    let input_pos = BlockPos::new(pos.x - fx, pos.y, pos.z - fz);
    neighbor_output_into(circuit, input_pos, pos, world_blocks, registry)
}
