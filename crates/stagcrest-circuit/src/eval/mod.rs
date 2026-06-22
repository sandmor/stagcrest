mod delay;
mod inverter;
mod output;
mod repeater;
mod source;
mod switch;
mod sync;
mod wire;

pub use output::neighbor_output_into;
pub use sync::{is_player_toggleable, is_repeater, is_torch_geometry, sync_block_state};

use stagcrest_mod_host::BlockRegistry;
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

pub fn dispatch(ctx: &EvalContext<'_>, kind: CircuitKind, prev_input: u8) -> EvalResult {
    match kind {
        CircuitKind::Source { level } => source::evaluate(level),
        CircuitKind::Wire { falloff } => wire::evaluate(ctx, falloff),
        CircuitKind::Switch { output } => switch::evaluate(ctx.state, output),
        CircuitKind::Inverter { output } => inverter::evaluate(ctx, output),
        CircuitKind::Delay { output, delay } => {
            delay::evaluate(ctx, output, delay, prev_input)
        }
        CircuitKind::Repeater { output } => repeater::evaluate(ctx, output, prev_input),
    }
}

pub fn max_neighbor_input(ctx: &EvalContext<'_>) -> u8 {
    let mut max_power = 0u8;
    for npos in crate::neighbors(ctx.pos) {
        max_power = max_power.max(neighbor_output_into(
            ctx.circuit,
            npos,
            ctx.pos,
            ctx.world,
            ctx.registry,
        ));
    }
    max_power
}
