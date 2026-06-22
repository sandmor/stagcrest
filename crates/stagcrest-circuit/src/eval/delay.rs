use super::{max_neighbor_input, EvalContext, EvalResult};

pub fn evaluate(ctx: &EvalContext<'_>, output: u8, delay: u8, prev_input: u8) -> EvalResult {
    let input = max_neighbor_input(ctx);
    if input == prev_input {
        return EvalResult::Unchanged;
    }
    let target = if input > 0 { output } else { 0 };
    EvalResult::ArmDelay {
        input,
        target,
        delay_ticks: delay as u64,
    }
}
