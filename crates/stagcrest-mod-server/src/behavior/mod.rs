//! Block behavior dispatch: native redstone behaviors and WASM callback routing.

mod native;
mod registry;

pub use native::{
    break_positions_for, default_push_reaction, redstone_powerable_for,
};
pub use registry::{BehaviorCtx, BehaviorRegistry, BehaviorResult};

use stagcrest_protocol::{BlockDef, BlockPos, BlockState, PushReaction};

/// Trait implemented by native block behaviors.
#[allow(dead_code)]
pub trait BlockBehavior: Send {
    fn on_place(&self, ctx: &mut BehaviorCtx<'_>) -> BehaviorResult {
        let _ = ctx;
        BehaviorResult::Ok
    }
    fn on_break(&self, ctx: &mut BehaviorCtx<'_>) -> BehaviorResult {
        let _ = ctx;
        BehaviorResult::Ok
    }
    fn on_use(&self, ctx: &mut BehaviorCtx<'_>) -> BehaviorResult {
        let _ = ctx;
        BehaviorResult::Ok
    }
    fn on_neighbor_changed(&self, ctx: &mut BehaviorCtx<'_>) -> BehaviorResult {
        let _ = ctx;
        BehaviorResult::Ok
    }
    fn on_scheduled_tick(&self, ctx: &mut BehaviorCtx<'_>) -> BehaviorResult {
        let _ = ctx;
        BehaviorResult::Ok
    }
    fn on_random_tick(&self, ctx: &mut BehaviorCtx<'_>) -> BehaviorResult {
        let _ = ctx;
        BehaviorResult::Ok
    }
    fn state_for_place(
        &self,
        ctx: &BehaviorCtx<'_>,
        face_normal: [i32; 3],
    ) -> Option<BlockState> {
        let _ = (ctx, face_normal);
        None
    }
    fn dynamic_light(&self, _ctx: &BehaviorCtx<'_>, _state: BlockState) -> u8 {
        0
    }
    fn push_reaction(&self, def: &BlockDef) -> PushReaction {
        default_push_reaction(def)
    }
    fn redstone_powerable(&self, def: &BlockDef) -> bool {
        redstone_powerable_for(def)
    }
}

pub fn block_push_reaction(def: &BlockDef) -> PushReaction {
    def.push_reaction()
}

pub fn block_redstone_powerable(def: &BlockDef) -> bool {
    def.redstone_powerable()
}

pub fn extra_break_positions(
    registry: &BehaviorRegistry,
    def: &BlockDef,
    pos: BlockPos,
    state: BlockState,
) -> Vec<BlockPos> {
    registry.break_positions(def, pos, state)
}

pub fn placement_state(
    registry: &BehaviorRegistry,
    def: &BlockDef,
    ctx: &BehaviorCtx<'_>,
) -> Option<BlockState> {
    registry.state_for_place(def, ctx)
}
