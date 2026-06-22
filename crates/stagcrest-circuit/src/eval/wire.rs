use super::{max_neighbor_input, EvalContext, EvalResult};

pub fn evaluate(ctx: &EvalContext<'_>, falloff: u8) -> EvalResult {
    let power = max_neighbor_input(ctx).saturating_sub(falloff);
    EvalResult::Publish(power)
}
