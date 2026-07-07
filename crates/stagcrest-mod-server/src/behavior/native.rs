use stagcrest_protocol::{
    piston_extended, piston_facing, piston_front_pos, piston_head_facing, BehaviorRef, BlockDef,
    BlockPos, BlockState, NativeBehaviorId, PushReaction,
};

pub fn default_push_reaction(def: &BlockDef) -> PushReaction {
    match def.behavior {
        Some(BehaviorRef::Native {
            id: NativeBehaviorId::Bedrock,
        }) => PushReaction::Block,
        Some(BehaviorRef::Native {
            id: NativeBehaviorId::PistonHead,
        }) => PushReaction::Block,
        _ if def.solid && def.opaque => PushReaction::Normal,
        _ => PushReaction::Destroy,
    }
}

pub fn redstone_powerable_for(def: &BlockDef) -> bool {
    def.redstone_powerable()
}

pub fn dynamic_light_for(def: &BlockDef, state: BlockState) -> u8 {
    def.resolved_light_emission(state)
}

pub fn break_positions_for(def: &BlockDef, pos: BlockPos, state: BlockState) -> Vec<BlockPos> {
    let mut positions = vec![pos];
    match def.behavior {
        Some(BehaviorRef::Native {
            id: NativeBehaviorId::PistonHead,
        }) => {
            let facing = piston_head_facing(state);
            positions.push(piston_front_pos(pos, facing.opposite()));
        }
        Some(BehaviorRef::Native {
            id: NativeBehaviorId::RedstonePiston { .. },
        })
        | Some(BehaviorRef::Native {
            id: NativeBehaviorId::PistonBody,
        }) if piston_extended(state) => {
            positions.push(piston_front_pos(pos, piston_facing(state)));
        }
        _ => {}
    }
    positions
}
