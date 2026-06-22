use super::{max_neighbor_input, EvalContext, EvalResult};

pub fn evaluate(ctx: &EvalContext<'_>, output: u8) -> EvalResult {
    let input = max_neighbor_input(ctx);
    let power = if input > 0 { 0 } else { output };
    EvalResult::Publish(power)
}
