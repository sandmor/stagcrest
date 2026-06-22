use stagcrest_protocol::repeater_delay_ticks;

use super::output::repeater_input_power;
use super::{EvalContext, EvalResult};

pub fn evaluate(ctx: &EvalContext<'_>, output: u8, prev_input: u8) -> EvalResult {
    let input = repeater_input_power(
        ctx.circuit,
        ctx.pos,
        ctx.state,
        ctx.world,
        ctx.registry,
    );
    if input == prev_input {
        return EvalResult::Unchanged;
    }
    let target = if input > 0 { output } else { 0 };
    EvalResult::ArmDelay {
        input,
        target,
        delay_ticks: repeater_delay_ticks(ctx.state) as u64,
    }
}
