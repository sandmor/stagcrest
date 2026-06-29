use crate::eval::is_torch_geometry;
use crate::registry::BlockRegistry;
use stagcrest_protocol::{
    facing_delta, mount_face, mount_facing, mount_on, mount_support_offset, observer_facing,
    repeater_facing, BlockDef, BlockPos, CircuitKind,
};
use stagcrest_world::World;

use crate::world::CircuitWorld;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlockPower {
    pub strong: u8,
    pub weak: u8,
}

impl BlockPower {
    pub fn effective(self) -> u8 {
        self.strong.max(self.weak)
    }

    pub fn is_powered(self) -> bool {
        self.effective() > 0
    }
}

pub fn is_opaque_block(def: &BlockDef) -> bool {
    def.solid && def.opaque && !def.fluid
}

/// Strong and weak power levels contributed to an opaque block cell.
///
/// `strong` comes from mounted switches, lit inverters above, repeater output faces, and
/// adjacent source blocks. `weak` comes from powered wire neighbors and wire sitting on top.
pub fn block_power_at(
    circuit: &CircuitWorld,
    block_pos: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> BlockPower {
    let mut power = BlockPower::default();

    if !world_blocks.is_chunk_interactive(block_pos.chunk_pos()) {
        return power;
    }

    let (block_id, _) = world_blocks.get_block(block_pos);
    let Some(block_def) = registry.block(block_id) else {
        return power;
    };
    if !is_opaque_block(block_def) {
        return power;
    }

    for npos in crate::neighbors(block_pos) {
        if !world_blocks.is_chunk_interactive(npos.chunk_pos()) {
            continue;
        }
        let (id, state) = world_blocks.get_block(npos);
        let Some(def) = registry.block(id) else {
            continue;
        };
        let Some(node) = def.circuit else {
            continue;
        };

        match node.kind {
            CircuitKind::Switch { output } => {
                if mount_on(state) {
                    let (dx, dy, dz) =
                        mount_support_offset(mount_face(state), mount_facing(state));
                    let support = BlockPos::new(npos.x + dx, npos.y + dy, npos.z + dz);
                    if support == block_pos {
                        power.strong = power.strong.max(output);
                    }
                }
            }
            CircuitKind::Source { level } => {
                power.strong = power.strong.max(level);
            }
            CircuitKind::Wire { .. } => {
                power.weak = power.weak.max(circuit.power_at(npos));
            }
            CircuitKind::Inverter { .. } => {
                if is_torch_geometry(def) && circuit.power_at(npos) > 0 {
                    let above = BlockPos::new(npos.x, npos.y + 1, npos.z);
                    if block_pos == above {
                        power.strong = power.strong.max(15);
                    }
                }
            }
            CircuitKind::Repeater { .. } => {
                let facing = repeater_facing(state);
                let (fx, _, fz) = facing_delta(facing);
                let output_pos = BlockPos::new(npos.x + fx, npos.y, npos.z + fz);
                if block_pos == output_pos {
                    power.strong = power.strong.max(circuit.power_at(npos));
                }
            }
            CircuitKind::Observer { .. } => {
                let facing = observer_facing(state);
                let (fx, _, fz) = facing_delta(facing);
                let output_pos = BlockPos::new(npos.x + fx, npos.y, npos.z + fz);
                if block_pos == output_pos {
                    power.strong = power.strong.max(circuit.power_at(npos));
                }
            }
            CircuitKind::Piston { .. } => {}
        }
    }

    let wire_above = BlockPos::new(block_pos.x, block_pos.y + 1, block_pos.z);
    if world_blocks.is_chunk_interactive(wire_above.chunk_pos()) {
        let (id, _) = world_blocks.get_block(wire_above);
        if let Some(def) = registry.block(id) {
            if def
                .circuit
                .is_some_and(|n| matches!(n.kind, CircuitKind::Wire { .. }))
            {
                power.weak = power.weak.max(circuit.power_at(wire_above));
            }
        }
    }

    power
}
