use crate::eval::EvalContext;
use crate::event::ScheduledEval;
use stagcrest_mod_server::block_push_reaction;
use crate::power::{block_power_at, is_redstone_powerable_block, signal_into};
use crate::registry::BlockRegistry;
use crate::world::CircuitWorld;
use stagcrest_protocol::{
    piston_extended, piston_facing, piston_front_pos, piston_head_state, piston_state,
    BlockGeometry, BlockPos, BlockState, ModelId, PushReaction,
};
use stagcrest_world::World;

pub const PISTON_PUSH_LIMIT: usize = 12;

fn piston_is_powered_at(
    circuit: &CircuitWorld,
    world: &World,
    registry: &BlockRegistry,
    pos: BlockPos,
    state: BlockState,
) -> bool {
    let ctx = EvalContext {
        pos,
        state,
        circuit,
        world,
        registry,
    };
    piston_is_powered(&ctx)
}

pub fn piston_is_powered(ctx: &EvalContext<'_>) -> bool {
    let facing = piston_facing(ctx.state);
    let (fx, fy, fz) = facing.delta();

    let mut result = false;

    for npos in crate::neighbors(ctx.pos) {
        let dx = npos.x - ctx.pos.x;
        let dy = npos.y - ctx.pos.y;
        let dz = npos.z - ctx.pos.z;
        if dx == fx && dy == fy && dz == fz {
            continue;
        }

        let sig = signal_into(ctx.circuit, npos, ctx.pos, ctx.world, ctx.registry);
        let (nid, _) = ctx.world.get_block(npos);
        let mut bp_powered = false;
        if let Some(ndef) = ctx.registry.block(nid) {
            if is_redstone_powerable_block(ndef) {
                bp_powered =
                    block_power_at(ctx.circuit, npos, ctx.world, ctx.registry).is_powered();
            }
        }
        if sig > 0 || bp_powered {
            result = true;
        }
    }

    result
}

fn is_slime_block(namespaced_id: &str) -> bool {
    namespaced_id.ends_with(":slime_block")
}

fn is_honey_block(namespaced_id: &str) -> bool {
    namespaced_id.ends_with(":honey_block")
}

fn is_piston_body(registry: &BlockRegistry, id: stagcrest_protocol::BlockId) -> bool {
    registry.block(id).is_some_and(|d| {
        d.circuit_kind()
            .is_some_and(|kind| matches!(kind, stagcrest_protocol::CircuitKind::Piston { .. }))
    })
}

fn is_piston_head(registry: &BlockRegistry, id: stagcrest_protocol::BlockId) -> bool {
    registry
        .block(id)
        .is_some_and(|d| matches!(d.geometry, BlockGeometry::Model(ModelId::PistonHead)))
}

fn piston_head_id(registry: &BlockRegistry) -> stagcrest_protocol::BlockId {
    registry
        .all_blocks()
        .find(|d| matches!(d.geometry, BlockGeometry::Model(ModelId::PistonHead)))
        .map(|d| d.id)
        .expect("piston_head block")
}

fn slime_honey_stick(a: &str, b: &str) -> bool {
    if is_slime_block(a) && is_honey_block(b) {
        return false;
    }
    if is_honey_block(a) && is_slime_block(b) {
        return false;
    }
    is_slime_block(a) || is_honey_block(a) || is_slime_block(b) || is_honey_block(b)
}

fn push_reaction_for(registry: &BlockRegistry, id: stagcrest_protocol::BlockId) -> PushReaction {
    registry
        .block(id)
        .map(block_push_reaction)
        .unwrap_or(PushReaction::Normal)
}

struct PushPlan {
    to_move: Vec<BlockPos>,
    destroy_at: Option<BlockPos>,
}

/// Collect blocks to move for a push starting at `start` along `direction`.
fn compute_push_plan(
    world: &World,
    registry: &BlockRegistry,
    air: stagcrest_protocol::BlockId,
    start: BlockPos,
    direction: (i32, i32, i32),
    exclude_piston_pos: Option<BlockPos>,
) -> Option<PushPlan> {
    let (dx, dy, dz) = direction;
    let mut chain = Vec::new();
    let mut pos = start;
    let mut destroy_at = None;

    loop {
        if !world.is_chunk_interactive(pos.chunk_pos()) {
            return None;
        }
        let (id, _) = world.get_block(pos);
        if id == air {
            break;
        }
        if registry.block(id).is_none() {
            return None;
        }

        match push_reaction_for(registry, id) {
            PushReaction::Block => return None,
            PushReaction::Destroy => {
                destroy_at = Some(pos);
                break;
            }
            PushReaction::Normal => chain.push(pos),
        }

        if chain.len() > PISTON_PUSH_LIMIT {
            return None;
        }

        pos = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
    }

    if expand_slime_honey_group(world, registry, air, &mut chain, exclude_piston_pos).is_none() {
        return None;
    }
    if chain.len() > PISTON_PUSH_LIMIT {
        return None;
    }
    Some(PushPlan {
        to_move: chain,
        destroy_at,
    })
}

fn expand_slime_honey_group(
    world: &World,
    registry: &BlockRegistry,
    air: stagcrest_protocol::BlockId,
    blocks: &mut Vec<BlockPos>,
    exclude_piston_pos: Option<BlockPos>,
) -> Option<()> {
    use std::collections::{HashSet, VecDeque};

    let mut set: HashSet<BlockPos> = blocks.iter().copied().collect();
    let mut queue: VecDeque<BlockPos> = blocks
        .iter()
        .filter(|pos| {
            let (id, _) = world.get_block(**pos);
            registry.block(id).is_some_and(|d| {
                is_slime_block(&d.namespaced_id) || is_honey_block(&d.namespaced_id)
            })
        })
        .copied()
        .collect();

    while let Some(pos) = queue.pop_front() {
        let (seed_id, _) = world.get_block(pos);
        let seed_name = registry.block(seed_id)?.namespaced_id.as_str();

        for npos in crate::neighbors(pos) {
            if set.contains(&npos) {
                continue;
            }
            if !world.is_chunk_interactive(npos.chunk_pos()) {
                continue;
            }
            let (nid, _) = world.get_block(npos);
            if nid == air {
                continue;
            }
            let Some(ndef) = registry.block(nid) else {
                return None;
            };
            if is_piston_head(registry, nid) {
                continue;
            }
            if is_piston_body(registry, nid) && exclude_piston_pos == Some(npos) {
                continue;
            }
            if !slime_honey_stick(seed_name, &ndef.namespaced_id) {
                continue;
            }
            match block_push_reaction(ndef) {
                PushReaction::Block => return None,
                PushReaction::Destroy => continue,
                PushReaction::Normal => {}
            }
            set.insert(npos);
            let queued = is_slime_block(&ndef.namespaced_id) || is_honey_block(&ndef.namespaced_id);
            if queued {
                queue.push_back(npos);
            }
        }
    }

    *blocks = set.into_iter().collect();
    Some(())
}

fn air_id(registry: &BlockRegistry) -> stagcrest_protocol::BlockId {
    registry
        .block_by_name("stagcrest:air")
        .unwrap_or(stagcrest_protocol::BlockId(0))
}

fn validate_extend_plan(
    world: &World,
    registry: &BlockRegistry,
    piston_pos: BlockPos,
    facing: stagcrest_protocol::Facing6,
    plan: &PushPlan,
) -> bool {
    let (dx, dy, dz) = facing.delta();
    let air = air_id(registry);

    let dot = |p: BlockPos| {
        (p.x - piston_pos.x) * dx + (p.y - piston_pos.y) * dy + (p.z - piston_pos.z) * dz
    };

    let mut order: Vec<BlockPos> = plan.to_move.clone();
    order.sort_by_key(|p| std::cmp::Reverse(dot(*p)));

    let mut vacating: std::collections::HashSet<BlockPos> = std::collections::HashSet::new();
    if let Some(destroy_pos) = plan.destroy_at {
        if !world.is_chunk_interactive(destroy_pos.chunk_pos()) {
            return false;
        }
        vacating.insert(destroy_pos);
    }

    for pos in order {
        let dest = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
        if !world.is_chunk_interactive(dest.chunk_pos()) {
            return false;
        }
        if dest == piston_pos {
            return false;
        }
        let (dest_id, _) = world.get_block(dest);
        if dest_id != air && !vacating.contains(&dest) {
            return false;
        }
        vacating.insert(pos);
        vacating.remove(&dest);
    }

    let _ = (registry, air);
    true
}

fn extend_pushes_blocks(
    world: &World,
    registry: &BlockRegistry,
    piston_pos: BlockPos,
) -> bool {
    let (_, state) = world.get_block(piston_pos);
    if piston_extended(state) {
        return false;
    }
    let facing = piston_facing(state);
    let (dx, dy, dz) = facing.delta();
    let air = air_id(registry);
    let front = piston_front_pos(piston_pos, facing);
    compute_push_plan(world, registry, air, front, (dx, dy, dz), Some(piston_pos))
        .is_some_and(|plan| !plan.to_move.is_empty())
}

/// Resolve all due piston scheduled actions for this tick in deterministic order.
/// Unobstructed extensions (nothing to push) run before extensions that must shove
/// blocks, so competing pistons behave like Bedrock block events.
pub fn resolve_piston_actions(
    circuit: &mut CircuitWorld,
    world: &mut World,
    registry: &BlockRegistry,
    mut actions: Vec<ScheduledEval>,
) {
    actions.sort_by_key(|action| {
        let pushes = if action.output == 1 {
            extend_pushes_blocks(world, registry, action.pos)
        } else {
            false
        };
        (pushes, action.seq, action.pos.x, action.pos.y, action.pos.z)
    });

    for action in actions {
        let (id, _state) = world.get_block(action.pos);
        let Some(def) = registry.block(id) else {
            continue;
        };
        let Some(stagcrest_protocol::CircuitKind::Piston { sticky }) = def.circuit_kind() else {
            continue;
        };
        if action.output == 1 {
            try_extend(circuit, world, registry, action.pos, sticky);
        } else {
            try_retract(circuit, world, registry, action.pos, sticky);
        }
    }
}

pub fn try_extend(
    circuit: &mut CircuitWorld,
    world: &mut World,
    registry: &BlockRegistry,
    piston_pos: BlockPos,
    sticky: bool,
) {
    let (piston_id, state) = world.get_block(piston_pos);
    if piston_extended(state) {
        return;
    }
    let facing = piston_facing(state);
    let (dx, dy, dz) = facing.delta();
    let air = air_id(registry);
    let head_id = piston_head_id(registry);

    let front = piston_front_pos(piston_pos, facing);
    let plan = match compute_push_plan(world, registry, air, front, (dx, dy, dz), Some(piston_pos))
    {
        Some(plan) => plan,
        None => return,
    };

    if !validate_extend_plan(world, registry, piston_pos, facing, &plan) {
        return;
    }

    let dot = |p: BlockPos| {
        (p.x - piston_pos.x) * dx + (p.y - piston_pos.y) * dy + (p.z - piston_pos.z) * dz
    };

    let mut moves: Vec<(BlockPos, BlockPos, stagcrest_protocol::BlockId, BlockState)> = Vec::new();
    for pos in &plan.to_move {
        let (id, st) = world.get_block(*pos);
        let dest = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
        moves.push((dest, *pos, id, st));
    }
    moves.sort_by_key(|(dest, _, _, _)| std::cmp::Reverse(dot(*dest)));

    if let Some(destroy_pos) = plan.destroy_at {
        world.set_block(destroy_pos, air, BlockState(0));
        circuit.record_block_move(world, registry, destroy_pos, air, BlockState(0));
    }

    let mut moved_observers: Vec<BlockPos> = Vec::new();
    for (dest, src, id, st) in moves {
        let moved_state = circuit.take_moved_node_state(src);
        world.set_block(dest, id, st);
        circuit.record_block_move(world, registry, dest, id, st);
        world.set_block(src, air, BlockState(0));
        circuit.record_block_move(world, registry, src, air, BlockState(0));
        circuit.apply_moved_node_state(dest, moved_state, world, registry);
        if registry.block(id).is_some_and(crate::is_observer) {
            moved_observers.push(dest);
        }
    }

    let new_state = piston_state(true, facing);
    world.set_block(piston_pos, piston_id, new_state);
    circuit.record_block_move(world, registry, piston_pos, piston_id, new_state);

    let head_state = piston_head_state(facing, sticky);
    world.set_block(front, head_id, head_state);
    circuit.record_block_move(world, registry, front, head_id, head_state);

    let sustained =
        piston_is_powered_at(circuit, world, registry, piston_pos, piston_state(true, facing));
    circuit.record_piston_extended(piston_pos, sustained);

    for obs in moved_observers {
        circuit.fire_moved_observer(obs, world, registry);
    }
}

pub fn try_retract(
    circuit: &mut CircuitWorld,
    world: &mut World,
    registry: &BlockRegistry,
    piston_pos: BlockPos,
    sticky: bool,
) {
    let (piston_id, state) = world.get_block(piston_pos);
    if !piston_extended(state) {
        return;
    }
    let facing = piston_facing(state);
    let (dx, dy, dz) = facing.delta();
    let air = air_id(registry);
    let head_pos = piston_front_pos(piston_pos, facing);

    let (head_id, _) = world.get_block(head_pos);
    if !is_piston_head(registry, head_id) {
        return;
    }

    world.set_block(head_pos, air, BlockState(0));
    circuit.record_block_move(world, registry, head_pos, air, BlockState(0));

    let should_drop = sticky && !circuit.piston_extend_was_sustained(piston_pos);

    if sticky && !should_drop {
        let pull_start = piston_front_pos(head_pos, facing);
        let (pull_id, _) = world.get_block(pull_start);
        let pull_reaction = push_reaction_for(registry, pull_id);
        if pull_id != air && pull_reaction == PushReaction::Normal {
            let mut pull_set = vec![pull_start];
            if expand_slime_honey_group(world, registry, air, &mut pull_set, Some(piston_pos))
                .is_some()
            {
                pull_set.sort_by_key(|p| {
                    (p.x - piston_pos.x) * dx
                        + (p.y - piston_pos.y) * dy
                        + (p.z - piston_pos.z) * dz
                });

                let mut moved_observers: Vec<BlockPos> = Vec::new();
                for pos in pull_set {
                    let dest = BlockPos::new(pos.x - dx, pos.y - dy, pos.z - dz);
                    if !world.is_chunk_interactive(dest.chunk_pos()) {
                        break;
                    }
                    let (dest_id, _) = world.get_block(dest);
                    if dest_id != air && dest != piston_pos {
                        break;
                    }
                    let (id, st) = world.get_block(pos);
                    let moved_state = circuit.take_moved_node_state(pos);
                    world.set_block(dest, id, st);
                    circuit.record_block_move(world, registry, dest, id, st);
                    world.set_block(pos, air, BlockState(0));
                    circuit.record_block_move(world, registry, pos, air, BlockState(0));
                    circuit.apply_moved_node_state(dest, moved_state, world, registry);
                    if registry.block(id).is_some_and(crate::is_observer) {
                        moved_observers.push(dest);
                    }
                }
                for obs in moved_observers {
                    circuit.fire_moved_observer(obs, world, registry);
                }
            }
        }
    }

    let new_state = piston_state(false, facing);
    world.set_block(piston_pos, piston_id, new_state);
    circuit.record_block_move(world, registry, piston_pos, piston_id, new_state);
    circuit.clear_piston_extended(piston_pos);
}
