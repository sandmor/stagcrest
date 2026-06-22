use stagcrest_protocol::{
    BlockDef, BlockGeometry, BlockId, BlockPos, BlockState, CircuitKind, CircuitNodeDef,
    ModelId, set_torch_lit, torch_lit,
};
use stagcrest_world::World;

pub fn sync_block_state(
    world_blocks: &mut World,
    pos: BlockPos,
    id: BlockId,
    def: &BlockDef,
    kind: CircuitKind,
    state: BlockState,
    new_power: u8,
) {
    match kind {
        CircuitKind::Wire { .. }
        | CircuitKind::Switch { .. }
        | CircuitKind::Delay { .. }
        | CircuitKind::Repeater { .. } => {
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

pub fn is_torch_geometry(def: &BlockDef) -> bool {
    matches!(def.geometry, BlockGeometry::Model(ModelId::RedstoneTorch))
}

pub fn is_player_toggleable(def: &BlockDef) -> bool {
    let Some(node) = def.circuit else {
        return false;
    };
    match node.kind {
        CircuitKind::Switch { .. } => true,
        CircuitKind::Inverter { .. } => is_torch_geometry(def),
        _ => false,
    }
}

pub fn is_repeater(def: &BlockDef) -> bool {
    matches!(
        def.circuit,
        Some(CircuitNodeDef {
            kind: CircuitKind::Repeater { .. },
        })
    )
}
