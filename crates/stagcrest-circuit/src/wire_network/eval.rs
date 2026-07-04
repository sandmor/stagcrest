use crate::power::{block_power_at, is_redstone_powerable_block, signal_into};
use crate::registry::BlockRegistry;
use crate::wire_network::{
    is_wire_block, wire_connections_at, wire_connects_toward_neighbor, WireConnections, WireLink,
};
use stagcrest_protocol::BlockPos;
use stagcrest_world::World;

use crate::eval::EvalContext;
use crate::world::CircuitWorld;

pub fn max_wire_input(ctx: &EvalContext<'_>, falloff: u8) -> u8 {
    max_wire_input_at(ctx.pos, ctx.circuit, ctx.world, ctx.registry, falloff)
}

pub fn max_wire_input_at(
    wire_pos: BlockPos,
    circuit: &CircuitWorld,
    world: &World,
    registry: &BlockRegistry,
    falloff: u8,
) -> u8 {
    let connections = wire_connections_at(registry, world, wire_pos);
    let mut direct_power = 0u8;
    let mut dust_power = 0u8;

    for (i, &(dx, dz)) in WireConnections::DIRECTIONS.iter().enumerate() {
        match connections.side(i) {
            WireLink::None => {}
            WireLink::Side => {
                let neighbor = BlockPos::new(wire_pos.x + dx, wire_pos.y, wire_pos.z + dz);
                collect_wire_input(
                    neighbor,
                    wire_pos,
                    circuit,
                    world,
                    registry,
                    &mut direct_power,
                    &mut dust_power,
                );
                let below = BlockPos::new(wire_pos.x + dx, wire_pos.y - 1, wire_pos.z + dz);
                if wire_connects_toward_neighbor(registry, world, below, wire_pos) {
                    collect_wire_input(
                        below,
                        wire_pos,
                        circuit,
                        world,
                        registry,
                        &mut direct_power,
                        &mut dust_power,
                    );
                }
            }
            WireLink::Up => {
                let neighbor = BlockPos::new(wire_pos.x + dx, wire_pos.y + 1, wire_pos.z + dz);
                collect_wire_input(
                    neighbor,
                    wire_pos,
                    circuit,
                    world,
                    registry,
                    &mut direct_power,
                    &mut dust_power,
                );
            }
        }
    }

    // Signal from a wire cell directly below into the bottom of this cell.
    let below = BlockPos::new(wire_pos.x, wire_pos.y - 1, wire_pos.z);
    if wire_connects_toward_neighbor(registry, world, below, wire_pos) {
        collect_wire_input(
            below,
            wire_pos,
            circuit,
            world,
            registry,
            &mut direct_power,
            &mut dust_power,
        );
    }

    // Face-adjacent non-wire power components power dust without dust falloff.
    for npos in crate::neighbors(wire_pos) {
        if !world.is_chunk_interactive(npos.chunk_pos()) {
            continue;
        }
        let (nid, _) = world.get_block(npos);
        if is_wire_block(registry, nid) {
            continue;
        }
        direct_power = direct_power.max(signal_into(circuit, npos, wire_pos, world, registry));
    }

    // Dust receives from hard-powered adjacent redstone-powerable blocks.
    for npos in crate::neighbors(wire_pos) {
        if !world.is_chunk_interactive(npos.chunk_pos()) {
            continue;
        }
        let (nid, _) = world.get_block(npos);
        let Some(ndef) = registry.block(nid) else {
            continue;
        };
        if !is_redstone_powerable_block(ndef) || ndef.circuit.is_some() {
            continue;
        }
        let bp = block_power_at(circuit, npos, world, registry);
        if bp.strong > 0 {
            direct_power = direct_power.max(bp.strong);
        }
    }

    direct_power.max(dust_power.saturating_sub(falloff))
}

fn collect_wire_input(
    source: BlockPos,
    consumer: BlockPos,
    circuit: &CircuitWorld,
    world: &World,
    registry: &BlockRegistry,
    direct_power: &mut u8,
    dust_power: &mut u8,
) {
    if !world.is_chunk_interactive(source.chunk_pos()) {
        return;
    }
    let (id, _) = world.get_block(source);
    let sig = signal_into(circuit, source, consumer, world, registry);
    if is_wire_block(registry, id) {
        *dust_power = (*dust_power).max(sig);
    } else {
        *direct_power = (*direct_power).max(sig);
    }
}
