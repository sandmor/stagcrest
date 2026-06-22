use stagcrest_mod_host::BlockRegistry;
use stagcrest_protocol::{
    repeater_facing, facing_delta, BlockGeometry, BlockPos, BlockState, CircuitKind, ModelId,
    set_torch_lit, torch_lit,
};
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
        CircuitKind::Delay { .. } => {
            if matches!(def.geometry, BlockGeometry::Model(ModelId::Repeater)) {
                let facing = repeater_facing(state);
                let (fx, _, fz) = facing_delta(facing);
                let output_pos = BlockPos::new(source.x + fx, source.y, source.z + fz);
                if consumer == output_pos {
                    circuit.power_at(source)
                } else {
                    0
                }
            } else {
                circuit.power_at(source)
            }
        }
    }
}

pub fn max_neighbor_input(
    circuit: &CircuitWorld,
    consumer: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> u8 {
    let mut max_power = 0u8;
    for npos in crate::neighbors(consumer) {
        max_power = max_power.max(neighbor_output_into(
            circuit, npos, consumer, world_blocks, registry,
        ));
    }
    max_power
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

pub fn compute_power(
    circuit: &CircuitWorld,
    pos: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
    kind: CircuitKind,
    state: BlockState,
) -> u8 {
    match kind {
        CircuitKind::Source { level } => level,
        CircuitKind::Switch { output } => {
            if state.0 & 1 != 0 {
                output
            } else {
                0
            }
        }
        CircuitKind::Inverter { output } => {
            let input = max_neighbor_input(circuit, pos, world_blocks, registry);
            if input > 0 {
                0
            } else {
                output
            }
        }
        CircuitKind::Wire { falloff } => {
            max_neighbor_input(circuit, pos, world_blocks, registry).saturating_sub(falloff)
        }
        CircuitKind::Delay { .. } => circuit.power_at(pos),
    }
}

pub fn sync_block_state(
    world_blocks: &mut World,
    pos: BlockPos,
    id: stagcrest_protocol::BlockId,
    def: &stagcrest_protocol::BlockDef,
    kind: CircuitKind,
    state: BlockState,
    new_power: u8,
) {
    match kind {
        CircuitKind::Wire { .. } | CircuitKind::Switch { .. } | CircuitKind::Delay { .. } => {
            let powered = u16::from(new_power > 0);
            let new_bits = (state.0 & !1) | powered;
            if state.0 != new_bits {
                world_blocks.set_block(pos, id, BlockState(new_bits));
            }
        }
        CircuitKind::Inverter { .. } => {
            if matches!(def.geometry, BlockGeometry::Model(ModelId::RedstoneTorch)) {
                let lit = new_power > 0;
                if torch_lit(state) != lit {
                    world_blocks.set_block(pos, id, set_torch_lit(state, lit));
                }
            }
        }
        CircuitKind::Source { .. } => {}
    }
}

pub fn is_torch_geometry(def: &stagcrest_protocol::BlockDef) -> bool {
    matches!(def.geometry, BlockGeometry::Model(ModelId::RedstoneTorch))
}

pub fn is_player_toggleable(def: &stagcrest_protocol::BlockDef) -> bool {
    let Some(node) = def.circuit else {
        return false;
    };
    match node.kind {
        CircuitKind::Switch { .. } => true,
        CircuitKind::Inverter { .. } => is_torch_geometry(def),
        _ => false,
    }
}

pub fn is_repeater(def: &stagcrest_protocol::BlockDef) -> bool {
    matches!(def.geometry, BlockGeometry::Model(ModelId::Repeater))
}
