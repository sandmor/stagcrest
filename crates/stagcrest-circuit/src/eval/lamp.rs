use super::{EvalContext, EvalResult};
use crate::power::block_power_at;
use stagcrest_protocol::{lamp_lit, set_lamp_lit, BlockState};

pub const LAMP_OFF_DELAY_TICKS: u64 = 6;

pub fn evaluate(ctx: &EvalContext<'_>) -> EvalResult {
    let powered = block_power_at(ctx.circuit, ctx.pos, ctx.world, ctx.registry).is_powered();
    let lit = lamp_lit(ctx.state);

    if powered {
        if !lit {
            return EvalResult::SetLit(true);
        }
        if ctx.circuit.has_pending_delay(ctx.pos) {
            return EvalResult::SetLit(true);
        }
        return EvalResult::Unchanged;
    }

    if lit && !ctx.circuit.has_pending_delay(ctx.pos) {
        return EvalResult::ArmDelay {
            input: 0,
            target: 0,
            delay_ticks: LAMP_OFF_DELAY_TICKS,
        };
    }

    EvalResult::Unchanged
}

pub fn apply_scheduled_off(state: BlockState) -> BlockState {
    set_lamp_lit(state, false)
}
