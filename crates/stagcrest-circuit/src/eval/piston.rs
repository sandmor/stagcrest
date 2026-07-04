use stagcrest_protocol::piston_extended;

use super::{EvalContext, EvalResult};
use crate::piston::piston_is_powered;

pub const PISTON_DELAY_TICKS: u64 = 1;

pub fn evaluate(ctx: &EvalContext<'_>) -> EvalResult {
    let powered = piston_is_powered(ctx);
    let extended = piston_extended(ctx.state);

    if powered == extended {
        return EvalResult::Unchanged;
    }

    if ctx.circuit.has_pending_delay(ctx.pos) {
        return EvalResult::Unchanged;
    }

    let target = if powered { 1 } else { 0 };
    EvalResult::ArmDelay {
        input: target,
        target,
        delay_ticks: PISTON_DELAY_TICKS,
    }
}
