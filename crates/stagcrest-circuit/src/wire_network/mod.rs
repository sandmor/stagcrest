mod connections;
mod eval;

pub use connections::{
    apply_single_connection_mirror, compute_wire_connections, wire_link_for_direction,
    WireConnections, WireLink,
};
pub use eval::{max_wire_input, max_wire_input_at};

use crate::registry::BlockRegistry;
use stagcrest_protocol::{
    observer_facing, repeater_connects_toward, repeater_facing, BlockGeometry, BlockId, BlockState,
    CircuitKind, ModelId,
};
use stagcrest_world::World;

/// Whether this block participates in the wire network (extends or receives wire signals).
pub fn is_wire_network_node(registry: &BlockRegistry, id: BlockId) -> bool {
    let Some(def) = registry.block(id) else {
        return false;
    };
    if def.namespaced_id == "stagcrest:redstone_dust" {
        return true;
    }
    let Some(node) = def.circuit else {
        return false;
    };
    matches!(
        node.kind,
        CircuitKind::Wire { .. }
            | CircuitKind::Switch { .. }
            | CircuitKind::Source { .. }
            | CircuitKind::Inverter { .. }
            | CircuitKind::Repeater { .. }
            | CircuitKind::Observer { .. }
    ) || matches!(def.geometry, BlockGeometry::Model(ModelId::RedstoneTorch))
}

/// Whether a neighbor may link to a wire cell from direction `(toward_wire_dx, toward_wire_dz)`.
pub fn is_wire_network_neighbor(
    registry: &BlockRegistry,
    id: BlockId,
    state: BlockState,
    toward_wire_dx: i32,
    toward_wire_dz: i32,
) -> bool {
    let Some(def) = registry.block(id) else {
        return false;
    };
    if matches!(def.geometry, BlockGeometry::Model(ModelId::Repeater)) {
        return repeater_connects_toward(repeater_facing(state), toward_wire_dx, toward_wire_dz);
    }
    if matches!(def.geometry, BlockGeometry::Model(ModelId::Observer)) {
        return repeater_connects_toward(observer_facing(state), toward_wire_dx, toward_wire_dz);
    }
    is_wire_network_node(registry, id)
}

pub fn connects_toward(state: BlockState, toward_dx: i32, toward_dz: i32) -> bool {
    repeater_connects_toward(repeater_facing(state), toward_dx, toward_dz)
}

pub fn wire_connections_at(
    registry: &BlockRegistry,
    world: &World,
    wire_pos: stagcrest_protocol::BlockPos,
) -> WireConnections {
    compute_wire_connections(
        |dx, dy, dz| {
            let npos = stagcrest_protocol::BlockPos::new(wire_pos.x + dx, wire_pos.y + dy, wire_pos.z + dz);
            if !world.is_chunk_interactive(npos.chunk_pos()) {
                return false;
            }
            let (id, state) = world.get_block(npos);
            is_wire_network_neighbor(registry, id, state, -dx, -dz)
        },
        |dx, dy, dz| {
            let npos = stagcrest_protocol::BlockPos::new(wire_pos.x + dx, wire_pos.y + dy, wire_pos.z + dz);
            if !world.is_chunk_interactive(npos.chunk_pos()) {
                return false;
            }
            let (id, _) = world.get_block(npos);
            registry
                .block(id)
                .is_some_and(|d| d.solid && d.opaque && !d.fluid)
        },
    )
}

/// Circuit nodes linked to `wire_pos` via wire network topology (includes diagonal climb links).
pub fn wire_network_neighbors(
    registry: &BlockRegistry,
    world: &World,
    wire_pos: stagcrest_protocol::BlockPos,
) -> Vec<stagcrest_protocol::BlockPos> {
    let connections = wire_connections_at(registry, world, wire_pos);
    let mut out = Vec::new();

    for (i, &(dx, dz)) in WireConnections::DIRECTIONS.iter().enumerate() {
        match connections.side(i) {
            WireLink::None => {}
            WireLink::Side => {
                out.push(stagcrest_protocol::BlockPos::new(
                    wire_pos.x + dx,
                    wire_pos.y,
                    wire_pos.z + dz,
                ));
                let below = stagcrest_protocol::BlockPos::new(
                    wire_pos.x + dx,
                    wire_pos.y - 1,
                    wire_pos.z + dz,
                );
                if wire_connects_toward_neighbor(registry, world, below, wire_pos) {
                    out.push(below);
                }
            }
            WireLink::Up => {
                out.push(stagcrest_protocol::BlockPos::new(
                    wire_pos.x + dx,
                    wire_pos.y + 1,
                    wire_pos.z + dz,
                ));
            }
        }
    }

    let below = stagcrest_protocol::BlockPos::new(wire_pos.x, wire_pos.y - 1, wire_pos.z);
    if wire_connects_toward_neighbor(registry, world, below, wire_pos) {
        out.push(below);
    }

    out
}

/// Whether `wire_pos` links toward `neighbor` per wire network topology.
pub fn wire_connects_toward_neighbor(
    registry: &BlockRegistry,
    world: &World,
    wire_pos: stagcrest_protocol::BlockPos,
    neighbor: stagcrest_protocol::BlockPos,
) -> bool {
    let connections = wire_connections_at(registry, world, wire_pos);
    let dx = neighbor.x - wire_pos.x;
    let dy = neighbor.y - wire_pos.y;
    let dz = neighbor.z - wire_pos.z;

    if dy == 0 {
        for (i, &(hdx, hdz)) in WireConnections::DIRECTIONS.iter().enumerate() {
            if hdx == dx && hdz == dz && connections.side(i) != WireLink::None {
                return true;
            }
        }
    } else if dy == 1 {
        for (i, &(hdx, hdz)) in WireConnections::DIRECTIONS.iter().enumerate() {
            if hdx == dx && hdz == dz && connections.side(i) == WireLink::Up {
                return true;
            }
        }
    } else if dy == -1 {
        for (i, &(hdx, hdz)) in WireConnections::DIRECTIONS.iter().enumerate() {
            if hdx == dx && hdz == dz && connections.side(i) == WireLink::Side {
                return true;
            }
        }
    }
    false
}
