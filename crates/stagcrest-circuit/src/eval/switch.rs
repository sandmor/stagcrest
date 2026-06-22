use stagcrest_protocol::BlockState;

use super::EvalResult;

pub fn evaluate(state: BlockState, output: u8) -> EvalResult {
    let power = if state.0 & 1 != 0 { output } else { 0 };
    EvalResult::Publish(power)
}
