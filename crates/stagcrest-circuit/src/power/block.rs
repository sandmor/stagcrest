use crate::eval::is_torch_geometry;
use crate::registry::BlockRegistry;
use stagcrest_mod_server::block_redstone_powerable;
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

pub fn is_redstone_powerable_block(def: &BlockDef) -> bool {
    block_redstone_powerable(def)
}

/// Strong power on a redstone-powerable opaque block from adjacent components.
/// Sources: mounted switches (attachment), inverter/torch (above only),
/// repeater (output face), observer (output face). Blocks of redstone do not
/// power adjacent opaque blocks. No wire/dust contribution.
fn block_strong_power(
    circuit: &CircuitWorld,
    block_pos: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> u8 {
    let mut strong = 0u8;

    for npos in crate::neighbors(block_pos) {
        if !world_blocks.is_chunk_interactive(npos.chunk_pos()) {
            continue;
        }
        let (id, state) = world_blocks.get_block(npos);
        let Some(def) = registry.block(id) else {
            continue;
        };
        let Some(kind) = def.circuit_kind() else {
            continue;
        };

        match kind {
            CircuitKind::Switch { output } => {
                if mount_on(state) {
                    let (dx, dy, dz) = mount_support_offset(mount_face(state), mount_facing(state));
                    let support = BlockPos::new(npos.x + dx, npos.y + dy, npos.z + dz);
                    if support == block_pos {
                        strong = strong.max(output);
                    }
                }
            }
            CircuitKind::Source { .. } => {}
            CircuitKind::Inverter { .. } => {
                if is_torch_geometry(def) && circuit.power_at(npos) > 0 {
                    let above = BlockPos::new(npos.x, npos.y + 1, npos.z);
                    if block_pos == above {
                        strong = strong.max(15);
                    }
                }
            }
            CircuitKind::Repeater { .. } => {
                let facing = repeater_facing(state);
                let (fx, _, fz) = facing_delta(facing);
                let output_pos = BlockPos::new(npos.x + fx, npos.y, npos.z + fz);
                if block_pos == output_pos {
                    strong = strong.max(circuit.power_at(npos));
                }
            }
            CircuitKind::Observer { .. } => {
                let facing = observer_facing(state);
                let (fx, _, fz) = facing_delta(facing);
                let output_pos = BlockPos::new(npos.x + fx, npos.y, npos.z + fz);
                if block_pos == output_pos {
                    strong = strong.max(circuit.power_at(npos));
                }
            }
            CircuitKind::Wire { .. } | CircuitKind::Piston { .. } | CircuitKind::Lamp => {}
        }
    }

    strong
}

/// Full power (strong + weak) on a redstone-powerable opaque block.
///
/// Strong power comes from adjacent circuit components (see `block_strong_power`).
/// Weak power comes only from redstone dust. Powered blocks do not charge other
/// opaque blocks directly; strong blocks can power adjacent dust, while weak
/// blocks can still activate adjacent components.
pub fn block_power_at(
    circuit: &CircuitWorld,
    block_pos: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> BlockPower {
    if !world_blocks.is_chunk_interactive(block_pos.chunk_pos()) {
        return BlockPower::default();
    }

    let (block_id, _) = world_blocks.get_block(block_pos);
    let Some(block_def) = registry.block(block_id) else {
        return BlockPower::default();
    };
    if !is_redstone_powerable_block(block_def) {
        return BlockPower::default();
    }

    let strong = block_strong_power(circuit, block_pos, world_blocks, registry);
    let mut weak = 0u8;

    for npos in crate::neighbors(block_pos) {
        if !world_blocks.is_chunk_interactive(npos.chunk_pos()) {
            continue;
        }
        if dust_power_to_block(circuit, npos, block_pos, world_blocks, registry).is_some() {
            weak = weak.max(circuit.power_at(npos));
        }
    }

    BlockPower { strong, weak }
}

fn dust_power_to_block(
    circuit: &CircuitWorld,
    dust_pos: BlockPos,
    block_pos: BlockPos,
    world_blocks: &World,
    registry: &BlockRegistry,
) -> Option<u8> {
    if circuit.power_at(dust_pos) == 0 || !world_blocks.is_chunk_interactive(dust_pos.chunk_pos()) {
        return None;
    }
    let (id, _) = world_blocks.get_block(dust_pos);
    let def = registry.block(id)?;
    if !def
        .circuit_kind()
        .is_some_and(|kind| matches!(kind, CircuitKind::Wire { .. }))
    {
        return None;
    }

    if dust_pos.x == block_pos.x && dust_pos.z == block_pos.z && dust_pos.y == block_pos.y + 1 {
        return Some(circuit.power_at(dust_pos));
    }

    if dust_pos.y == block_pos.y
        && crate::wire_network::wire_connects_toward_neighbor(
            registry,
            world_blocks,
            dust_pos,
            block_pos,
        )
    {
        return Some(circuit.power_at(dust_pos));
    }

    None
}
