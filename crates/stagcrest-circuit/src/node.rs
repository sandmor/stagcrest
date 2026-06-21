use stagcrest_mod_host::BlockRegistry;
use stagcrest_protocol::{
    BlockGeometry, BlockPos, BlockState, CircuitKind, ModelId, set_torch_lit, torch_lit,
};
use stagcrest_world::World;

use crate::world::CircuitWorld;

pub fn neighbor_output(
    world: &CircuitWorld,
    pos: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> u8 {
    let (id, state) = world_blocks.get_block(pos);
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
        CircuitKind::Inverter { .. } | CircuitKind::Wire { .. } | CircuitKind::Delay { .. } => {
            world.power_at(pos)
        }
    }
}

pub fn max_neighbor_input(
    world: &CircuitWorld,
    pos: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> u8 {
    let mut max_power = 0u8;
    for npos in crate::neighbors(pos) {
        max_power = max_power.max(neighbor_output(world, npos, world_blocks, registry));
    }
    max_power
}

pub fn compute_power(
    world: &CircuitWorld,
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
            let input = max_neighbor_input(world, pos, world_blocks, registry);
            if input > 0 {
                0
            } else {
                output
            }
        }
        CircuitKind::Wire { falloff } => max_neighbor_input(world, pos, world_blocks, registry)
            .saturating_sub(falloff),
        CircuitKind::Delay { .. } => world.power_at(pos),
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
