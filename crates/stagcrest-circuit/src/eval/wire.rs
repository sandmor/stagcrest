use crate::wire_network::max_wire_input;
use crate::power::signal_into;

use super::{EvalContext, EvalResult};

pub fn evaluate(ctx: &EvalContext<'_>, falloff: u8) -> EvalResult {
    let power = max_wire_input(ctx, falloff);
    EvalResult::Publish(power)
}

pub fn max_connected_input(ctx: &EvalContext<'_>) -> u8 {
    let mut max_power = 0u8;
    for npos in crate::neighbors(ctx.pos) {
        max_power = max_power.max(signal_into(
            ctx.circuit,
            npos,
            ctx.pos,
            ctx.world,
            ctx.registry,
        ));
    }
    max_power
}
