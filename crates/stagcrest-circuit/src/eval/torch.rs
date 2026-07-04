//! Evaluates [`CircuitKind::Inverter`] blocks with torch attachment geometry:
//! input from support-block power, 2-tick output delay, burnout after rapid toggling.

use crate::eval::sync::is_torch_geometry;
use crate::power::{block_power_at, inverter_support_block};

use super::{EvalContext, EvalResult};

pub const TORCH_DELAY_TICKS: u64 = 2;
pub const BURNOUT_TOGGLE_LIMIT: u8 = 8;
pub const BURNOUT_WINDOW_TICKS: u64 = 60;

pub fn evaluate(ctx: &EvalContext<'_>, output: u8, prev_input: u8, burnt_out: bool) -> EvalResult {
    if burnt_out {
        return EvalResult::Publish(0);
    }

    if !ctx
        .registry
        .block(ctx.world.get_block(ctx.pos).0)
        .is_some_and(is_torch_geometry)
    {
        return super::inverter::evaluate_generic(ctx, output);
    }

    let support = inverter_support_block(ctx.pos, ctx.state);
    let input = block_power_at(ctx.circuit, support, ctx.world, ctx.registry).effective();
    let target = if input > 0 { 0 } else { output };
    let current = ctx.circuit.power_at(ctx.pos);

    if input == prev_input && current == target {
        return EvalResult::Unchanged;
    }

    EvalResult::ArmDelay {
        input,
        target,
        delay_ticks: TORCH_DELAY_TICKS,
    }
}
