mod inverter;
mod observer;
mod piston;
mod repeater;
mod source;
mod switch;
mod sync;
mod torch;
mod wire;

pub use observer::OBSERVER_PULSE_TICKS;
pub use sync::{
    is_button_geometry, is_observer, is_piston, is_player_toggleable, is_repeater,
    is_torch_geometry, sync_block_state,
};
pub use torch::{BURNOUT_TOGGLE_LIMIT, BURNOUT_WINDOW_TICKS};

use crate::registry::BlockRegistry;
use stagcrest_protocol::{BlockPos, BlockState, CircuitKind};
use stagcrest_world::World;

use crate::world::CircuitWorld;

pub struct EvalContext<'a> {
    pub pos: BlockPos,
    pub state: BlockState,
    pub circuit: &'a CircuitWorld,
    pub world: &'a World,
    pub registry: &'a BlockRegistry,
}

pub enum EvalResult {
    Unchanged,
    Publish(u8),
    ArmDelay {
        input: u8,
        target: u8,
        delay_ticks: u64,
    },
}

pub fn dispatch(
    ctx: &EvalContext<'_>,
    kind: CircuitKind,
    prev_input: u8,
    burnt_out: bool,
) -> EvalResult {
    match kind {
        CircuitKind::Source { level } => source::evaluate(level),
        CircuitKind::Wire { falloff } => wire::evaluate(ctx, falloff),
        CircuitKind::Switch { output } => switch::evaluate(ctx, output),
        CircuitKind::Inverter { output } => torch::evaluate(ctx, output, prev_input, burnt_out),
        CircuitKind::Repeater { output } => repeater::evaluate(ctx, output, prev_input),
        CircuitKind::Observer { .. } => observer::evaluate(),
        CircuitKind::Piston { .. } => piston::evaluate(ctx),
    }
}
