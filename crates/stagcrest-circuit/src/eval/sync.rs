use stagcrest_protocol::{
    set_torch_lit, torch_burnt_out, torch_lit, BlockDef, BlockGeometry, BlockId, BlockPos,
    BlockState, CircuitKind, CircuitNodeDef, ModelId,
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
) -> Option<BlockState> {
    match kind {
        CircuitKind::Wire { .. }
        | CircuitKind::Switch { .. }
        | CircuitKind::Repeater { .. }
        | CircuitKind::Observer { .. } => {
            let powered = u16::from(new_power > 0);
            let new_bits = (state.0 & !1) | powered;
            if state.0 != new_bits {
                let new_state = BlockState(new_bits);
                world_blocks.set_block(pos, id, new_state);
                return Some(new_state);
            }
        }
        CircuitKind::Inverter { .. } => {
            if matches!(def.geometry, BlockGeometry::Model(ModelId::RedstoneTorch)) {
                let lit = new_power > 0 && !torch_burnt_out(state);
                if torch_lit(state) != lit {
                    let new_state = set_torch_lit(state, lit);
                    world_blocks.set_block(pos, id, new_state);
                    return Some(new_state);
                }
            }
        }
        CircuitKind::Source { .. } => {}
        CircuitKind::Piston { .. } => {}
    }
    None
}

pub fn is_torch_geometry(def: &BlockDef) -> bool {
    matches!(def.geometry, BlockGeometry::Model(ModelId::RedstoneTorch))
}

pub fn is_button_geometry(def: Option<&BlockDef>) -> bool {
    def.is_some_and(|d| matches!(d.geometry, BlockGeometry::Model(ModelId::Button)))
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

pub fn is_observer(def: &BlockDef) -> bool {
    matches!(
        def.circuit,
        Some(CircuitNodeDef {
            kind: CircuitKind::Observer { .. },
        })
    )
}

pub fn is_piston(def: &BlockDef) -> bool {
    matches!(
        def.circuit,
        Some(CircuitNodeDef {
            kind: CircuitKind::Piston { .. },
        })
    )
}
