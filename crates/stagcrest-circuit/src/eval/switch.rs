use super::{EvalContext, EvalResult};

pub fn evaluate(ctx: &EvalContext<'_>, output: u8) -> EvalResult {
    let power = if ctx.state.0 & 1 != 0 { output } else { 0 };
    EvalResult::Publish(power)
}
