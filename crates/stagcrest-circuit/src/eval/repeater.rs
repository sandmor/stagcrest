use stagcrest_protocol::{facing_delta, repeater_delay_ticks, repeater_facing, repeater_powered};

use super::{EvalContext, EvalResult};
use crate::power::repeater_input_power;

pub fn evaluate(ctx: &EvalContext<'_>, output: u8, prev_input: u8) -> EvalResult {
    if is_locked(ctx) {
        return EvalResult::Unchanged;
    }

    let input = repeater_input_power(ctx.circuit, ctx.pos, ctx.state, ctx.world, ctx.registry);
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

fn is_locked(ctx: &EvalContext<'_>) -> bool {
    let facing = repeater_facing(ctx.state);
    let (fx, _, _fz) = facing_delta(facing);
    let side_offsets = if fx != 0 {
        [(0, 0, 1), (0, 0, -1)]
    } else {
        [(1, 0, 0), (-1, 0, 0)]
    };

    for (sx, sy, sz) in side_offsets {
        let side_pos =
            stagcrest_protocol::BlockPos::new(ctx.pos.x + sx, ctx.pos.y + sy, ctx.pos.z + sz);
        if !ctx.world.is_chunk_interactive(side_pos.chunk_pos()) {
            continue;
        }
        let (id, state) = ctx.world.get_block(side_pos);
        let Some(def) = ctx.registry.block(id) else {
            continue;
        };
        let Some(kind) = def.circuit_kind() else {
            continue;
        };
        let stagcrest_protocol::CircuitKind::Repeater { .. } = kind else {
            continue;
        };
        if !repeater_powered(state) && ctx.circuit.power_at(side_pos) == 0 {
            continue;
        }
        let side_facing = repeater_facing(state);
        let (ofx, _, ofz) = facing_delta(side_facing);
        let output_pos =
            stagcrest_protocol::BlockPos::new(side_pos.x + ofx, side_pos.y, side_pos.z + ofz);
        if output_pos == ctx.pos {
            return true;
        }
    }
    false
}
