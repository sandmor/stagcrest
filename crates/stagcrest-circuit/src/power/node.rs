use crate::eval::is_torch_geometry;
use crate::registry::BlockRegistry;
use crate::wire_network::{
    connects_toward, is_wire_network_neighbor, wire_connects_toward_neighbor,
};
use stagcrest_protocol::{
    facing_delta, mount_on, observer_facing, repeater_facing, torch_attachment, BlockPos,
    BlockState, CircuitKind,
};
use stagcrest_world::World;

use crate::world::CircuitWorld;

/// Opaque block cell an attachment-based inverter is mounted on.
pub fn inverter_support_block(inverter_pos: BlockPos, state: BlockState) -> BlockPos {
    let (dx, dy, dz) = torch_attachment(state).support_offset();
    BlockPos::new(
        inverter_pos.x + dx,
        inverter_pos.y + dy,
        inverter_pos.z + dz,
    )
}

/// Directed signal level from `source` cell into `consumer` cell.
pub fn signal_into(
    circuit: &CircuitWorld,
    source: BlockPos,
    consumer: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> u8 {
    if !world_blocks.is_chunk_interactive(source.chunk_pos()) {
        return 0;
    }

    let (id, state) = world_blocks.get_block(source);
    let Some(def) = registry.block(id) else {
        return 0;
    };
    let Some(node) = def.circuit else {
        return 0;
    };

    if !are_face_adjacent(source, consumer)
        && !(matches!(node.kind, CircuitKind::Wire { .. })
            && wire_connects_toward_neighbor(registry, world_blocks, source, consumer))
    {
        return 0;
    }

    match node.kind {
        CircuitKind::Source { level } => level,
        CircuitKind::Switch { output } => {
            if !mount_on(state) {
                return 0;
            }
            switch_signal_into(registry, id, state, source, consumer, output, world_blocks)
        }
        CircuitKind::Inverter { .. } => {
            if is_torch_geometry(def) && consumer == inverter_support_block(source, state) {
                return 0;
            }
            if inverter_outputs_to(registry, source, state, consumer, world_blocks) {
                circuit.power_at(source)
            } else {
                0
            }
        }
        CircuitKind::Wire { .. } => {
            if wire_connects_toward_neighbor(registry, world_blocks, source, consumer) {
                circuit.power_at(source)
            } else {
                0
            }
        }
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
        CircuitKind::Observer { .. } => {
            let facing = observer_facing(state);
            let (fx, _, fz) = facing_delta(facing);
            let output_pos = BlockPos::new(source.x + fx, source.y, source.z + fz);
            if consumer == output_pos {
                circuit.power_at(source)
            } else {
                0
            }
        }
        CircuitKind::Piston { .. } => 0,
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

    let sig = signal_into(circuit, input_pos, pos, world_blocks, registry);
    if sig > 0 {
        return sig;
    }

    if !world_blocks.is_chunk_interactive(input_pos.chunk_pos()) {
        return 0;
    }
    let (input_id, _) = world_blocks.get_block(input_pos);
    if let Some(input_def) = registry.block(input_id) {
        if input_def.circuit.is_none() && super::is_redstone_powerable_block(input_def) {
            return super::block_power_at(circuit, input_pos, world_blocks, registry).effective();
        }
    }
    0
}

fn are_face_adjacent(a: BlockPos, b: BlockPos) -> bool {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs() == 1
}

fn switch_signal_into(
    registry: &BlockRegistry,
    switch_id: stagcrest_protocol::BlockId,
    switch_state: BlockState,
    source: BlockPos,
    consumer: BlockPos,
    output: u8,
    world_blocks: &World,
) -> u8 {
    let (dx, dy, dz) = (
        consumer.x - source.x,
        consumer.y - source.y,
        consumer.z - source.z,
    );

    let (consumer_id, consumer_state) = world_blocks.get_block(consumer);
    if let Some(consumer_def) = registry.block(consumer_id) {
        if let Some(node) = consumer_def.circuit {
            match node.kind {
                CircuitKind::Wire { .. } => {
                    if wire_connects_toward_neighbor(registry, world_blocks, consumer, source) {
                        return output;
                    }
                }
                CircuitKind::Repeater { .. } => {
                    if connects_toward(consumer_state, -dx, -dz) {
                        return output;
                    }
                }
                CircuitKind::Observer { .. } => {
                    if connects_toward(consumer_state, -dx, -dz) {
                        return output;
                    }
                }
                CircuitKind::Inverter { .. } if is_torch_geometry(consumer_def) => {
                    return 0;
                }
                _ => {}
            }
        }
    }

    // Floor-mounted switch powers the wire cell directly below.
    if dy == -1 && dx == 0 && dz == 0 {
        if registry.block(consumer_id).is_some_and(|d| {
            d.circuit
                .is_some_and(|n| matches!(n.kind, CircuitKind::Wire { .. }))
        }) {
            return output;
        }
    }

    if is_wire_network_neighbor(registry, switch_id, switch_state, -dx, -dy, -dz)
        && wire_connects_toward_neighbor(registry, world_blocks, consumer, source)
    {
        return output;
    }

    0
}

fn inverter_outputs_to(
    registry: &BlockRegistry,
    source: BlockPos,
    state: BlockState,
    consumer: BlockPos,
    world_blocks: &World,
) -> bool {
    if consumer == inverter_support_block(source, state) {
        return false;
    }
    if !are_face_adjacent(source, consumer) {
        return false;
    }
    let (cid, _) = world_blocks.get_block(consumer);
    let Some(cdef) = registry.block(cid) else {
        return false;
    };
    if cdef
        .circuit
        .is_some_and(|n| matches!(n.kind, CircuitKind::Wire { .. }))
    {
        return wire_connects_toward_neighbor(registry, world_blocks, consumer, source);
    }
    cdef.circuit.is_some()
}
