use super::wire::max_connected_input;
use super::{EvalContext, EvalResult};

pub fn evaluate_generic(ctx: &EvalContext<'_>, output: u8) -> EvalResult {
    let input = max_connected_input(ctx);
    let power = if input > 0 { 0 } else { output };
    EvalResult::Publish(power)
}
