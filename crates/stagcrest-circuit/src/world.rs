use crate::power::inverter_support_block;
use crate::registry::BlockRegistry;
use stagcrest_protocol::{
    mount_on, observer_facing, observer_watches, set_torch_lit, BlockPos, BlockState, ChunkPos,
    CircuitKind, LocalBlockPos,
};
use stagcrest_storage::ChunkCircuitSnapshot;
use stagcrest_world::World;
use std::collections::{HashMap, HashSet};

use crate::eval::{
    dispatch, is_button_geometry, is_torch_geometry, sync_block_state, EvalContext, EvalResult,
    BURNOUT_TOGGLE_LIMIT, BURNOUT_WINDOW_TICKS, OBSERVER_PULSE_TICKS,
};
use crate::event::{CircuitEvent, EventQueue};
use crate::piston::{try_extend, try_retract};

pub const MAX_EVALS_PER_TICK: usize = 4096;
pub const BUTTON_HOLD_TICKS: u64 = 30;

#[derive(Debug, Clone, Copy, Default)]
struct TorchFlicker {
    count: u8,
    window_start: u64,
}

/// Position-keyed circuit state captured when a piston moves a block, so it can
/// travel with the block to its destination cell.
#[derive(Default)]
pub(crate) struct MovedNodeState {
    power: Option<u8>,
    delay_input: Option<u8>,
    pending_delay: Option<crate::event::ScheduledEval>,
    button_release: Option<u64>,
    torch_burnt_out: bool,
    torch_flicker: Option<TorchFlicker>,
}

#[derive(Debug, Clone, Default)]
pub struct CircuitWorld {
    power: HashMap<BlockPos, u8>,
    queue: EventQueue,
    delay_input: HashMap<BlockPos, u8>,
    pending_button_release: HashMap<BlockPos, u64>,
    torch_burnt_out: HashSet<BlockPos>,
    torch_flicker: HashMap<BlockPos, TorchFlicker>,
    tick: u64,
    visual_updates: Vec<(stagcrest_protocol::BlockId, BlockPos, BlockState)>,
    power_updates: Vec<(BlockPos, u8)>,
    dirty_chunks: HashSet<ChunkPos>,
}

impl CircuitWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_tick(&self) -> u64 {
        self.tick
    }

    pub fn set_tick(&mut self, tick: u64) {
        self.tick = tick;
    }

    pub fn drain_dirty_chunks(&mut self) -> HashSet<ChunkPos> {
        std::mem::take(&mut self.dirty_chunks)
    }

    fn mark_chunk_dirty(&mut self, pos: BlockPos) {
        self.dirty_chunks.insert(pos.chunk_pos());
    }

    pub fn export_chunk_snapshot(&self, chunk: ChunkPos) -> ChunkCircuitSnapshot {
        let base_x = chunk.x * stagcrest_protocol::CHUNK_SIZE;
        let base_y = chunk.y * stagcrest_protocol::CHUNK_SIZE;
        let base_z = chunk.z * stagcrest_protocol::CHUNK_SIZE;

        let to_local = |pos: BlockPos| LocalBlockPos {
            x: (pos.x - base_x) as u8,
            y: (pos.y - base_y) as u8,
            z: (pos.z - base_z) as u8,
        };

        let power = self
            .power
            .iter()
            .filter(|(pos, _)| pos.chunk_pos() == chunk)
            .map(|(pos, &level)| (to_local(*pos), level))
            .collect();

        let delay_input = self
            .delay_input
            .iter()
            .filter(|(pos, _)| pos.chunk_pos() == chunk)
            .map(|(pos, &input)| (to_local(*pos), input))
            .collect();

        let pending_delays = self
            .queue
            .pending_delays_in_chunk(chunk)
            .into_iter()
            .map(|eval| {
                let remaining = eval
                    .fire_tick
                    .saturating_sub(self.tick)
                    .min(u64::from(u8::MAX)) as u8;
                stagcrest_storage::PendingDelaySnapshot {
                    local: to_local(eval.pos),
                    remaining_ticks: remaining,
                    output: eval.output,
                }
            })
            .collect();

        let button_release = self
            .pending_button_release
            .iter()
            .filter(|(pos, _)| pos.chunk_pos() == chunk)
            .map(|(pos, fire_tick)| {
                let remaining = fire_tick.saturating_sub(self.tick).min(u64::from(u8::MAX)) as u8;
                (to_local(*pos), remaining)
            })
            .collect();

        let torch_burnt_out = self
            .torch_burnt_out
            .iter()
            .filter(|pos| pos.chunk_pos() == chunk)
            .map(|pos| to_local(*pos))
            .collect();

        ChunkCircuitSnapshot {
            power,
            delay_input,
            pending_delays,
            button_release,
            torch_burnt_out,
        }
    }

    pub fn import_chunk_snapshot(
        &mut self,
        chunk: ChunkPos,
        snapshot: ChunkCircuitSnapshot,
        global_tick: u64,
    ) {
        let base_x = chunk.x * stagcrest_protocol::CHUNK_SIZE;
        let base_y = chunk.y * stagcrest_protocol::CHUNK_SIZE;
        let base_z = chunk.z * stagcrest_protocol::CHUNK_SIZE;

        let to_world = |local: LocalBlockPos| BlockPos {
            x: base_x + local.x as i32,
            y: base_y + local.y as i32,
            z: base_z + local.z as i32,
        };

        self.power.retain(|pos, _| pos.chunk_pos() != chunk);
        self.delay_input.retain(|pos, _| pos.chunk_pos() != chunk);
        self.pending_button_release
            .retain(|pos, _| pos.chunk_pos() != chunk);
        self.torch_burnt_out.retain(|pos| pos.chunk_pos() != chunk);
        self.torch_flicker.retain(|pos, _| pos.chunk_pos() != chunk);
        for eval in self.queue.pending_delays_in_chunk(chunk) {
            self.queue.cancel_delay(eval.pos);
        }

        for (local, level) in snapshot.power {
            let pos = to_world(local);
            if level == 0 {
                self.power.remove(&pos);
            } else {
                self.power.insert(pos, level);
            }
        }

        for (local, input) in snapshot.delay_input {
            self.delay_input.insert(to_world(local), input);
        }

        for delay in snapshot.pending_delays {
            let pos = to_world(delay.local);
            let fire_tick = global_tick.saturating_add(u64::from(delay.remaining_ticks));
            self.queue.schedule_delay(fire_tick, pos, delay.output);
        }

        for (local, remaining) in snapshot.button_release {
            let pos = to_world(local);
            let fire_tick = global_tick.saturating_add(u64::from(remaining));
            self.pending_button_release.insert(pos, fire_tick);
        }

        for local in snapshot.torch_burnt_out {
            self.torch_burnt_out.insert(to_world(local));
        }
    }

    pub fn power_at(&self, pos: BlockPos) -> u8 {
        self.power.get(&pos).copied().unwrap_or(0)
    }

    pub(crate) fn prev_delay_input(&self, pos: BlockPos) -> u8 {
        self.delay_input.get(&pos).copied().unwrap_or(0)
    }

    pub(crate) fn arm_delay(&mut self, pos: BlockPos, input: u8, delay_ticks: u64, target: u8) {
        self.delay_input.insert(pos, input);
        self.queue
            .schedule_delay(self.tick + delay_ticks, pos, target);
        self.mark_chunk_dirty(pos);
    }

    pub fn has_pending_delay(&self, pos: BlockPos) -> bool {
        self.queue.has_pending_delay(pos)
    }

    pub(crate) fn record_block_move(
        &mut self,
        world: &mut World,
        registry: &BlockRegistry,
        pos: BlockPos,
        id: stagcrest_protocol::BlockId,
        state: BlockState,
    ) {
        self.visual_updates.push((id, pos, state));
        self.mark_chunk_dirty(pos);
        self.notify_block_changed(pos, world, registry);
    }

    /// Remove all position-keyed circuit state from `pos` (used when a piston is
    /// about to move the block at `pos` to another cell). Returns the captured
    /// state so it can be re-applied at the destination via [`apply_moved_node_state`].
    pub(crate) fn take_moved_node_state(&mut self, pos: BlockPos) -> MovedNodeState {
        MovedNodeState {
            power: self.power.remove(&pos),
            delay_input: self.delay_input.remove(&pos),
            pending_delay: self.queue.take_delay(pos),
            button_release: self.pending_button_release.remove(&pos),
            torch_burnt_out: self.torch_burnt_out.remove(&pos),
            torch_flicker: self.torch_flicker.remove(&pos),
        }
    }

    /// Re-apply circuit state captured by [`take_moved_node_state`] at the block's
    /// new position. This keeps a moved redstone component's published power and
    /// in-flight pulse timer alive at its destination (Minecraft carries a moving
    /// component's signal/scheduled tick with it).
    pub(crate) fn apply_moved_node_state(
        &mut self,
        pos: BlockPos,
        moved: MovedNodeState,
        world: &World,
        registry: &BlockRegistry,
    ) {
        let had_power = moved.power.is_some();
        if let Some(p) = moved.power {
            if p == 0 {
                self.power.remove(&pos);
            } else {
                self.power.insert(pos, p);
            }
            self.power_updates.push((pos, p));
            self.mark_chunk_dirty(pos);
        }
        if let Some(d) = moved.delay_input {
            self.delay_input.insert(pos, d);
        }
        if let Some(eval) = moved.pending_delay {
            self.queue.schedule_delay(eval.fire_tick, pos, eval.output);
        }
        if let Some(t) = moved.button_release {
            self.pending_button_release.insert(pos, t);
        }
        if moved.torch_burnt_out {
            self.torch_burnt_out.insert(pos);
        }
        if let Some(f) = moved.torch_flicker {
            self.torch_flicker.insert(pos, f);
        }
        if had_power {
            self.enqueue_circuit_neighbors(pos, world, registry);
            self.enqueue_block_power_dependents(pos, world, registry);
        }
    }

    pub fn queue_update(&mut self, pos: BlockPos) {
        self.queue.enqueue_evaluate(pos);
    }

    pub fn notify_block_changed(
        &mut self,
        pos: BlockPos,
        world: &mut World,
        registry: &BlockRegistry,
    ) {
        self.queue.cancel_delay(pos);
        self.delay_input.remove(&pos);

        let (id, _) = world.get_block(pos);
        if registry.block(id).and_then(|d| d.circuit).is_none() {
            if self.power.remove(&pos).is_some() {
                self.power_updates.push((pos, 0));
                self.mark_chunk_dirty(pos);
            }
        }

        self.queue_update(pos);
        self.enqueue_circuit_neighbors(pos, world, registry);
        self.notify_observers_watching(pos, world, registry);
    }

    pub fn drain_visual_updates(
        &mut self,
    ) -> Vec<(BlockPos, stagcrest_protocol::BlockId, BlockState)> {
        self.visual_updates
            .drain(..)
            .map(|(id, pos, state)| (pos, id, state))
            .collect()
    }

    pub fn drain_power_updates(&mut self) -> Vec<(BlockPos, u8)> {
        self.power_updates.drain(..).collect()
    }

    pub fn power_in_chunk(&self, chunk: ChunkPos) -> Vec<(BlockPos, u8)> {
        self.power
            .iter()
            .filter(|(pos, _)| pos.chunk_pos() == chunk)
            .map(|(pos, &level)| (*pos, level))
            .collect()
    }

    pub fn tick(&mut self, world: &mut World, registry: &BlockRegistry) {
        self.tick = self.tick.saturating_add(1);

        self.drain_button_releases(world, registry);

        for scheduled in self.queue.drain_due_delays(self.tick) {
            self.apply_scheduled_output(scheduled.pos, scheduled.output, world, registry);
        }

        let mut steps = 0usize;
        while steps < MAX_EVALS_PER_TICK {
            let Some(CircuitEvent::Evaluate(pos)) = self.queue.dequeue() else {
                break;
            };
            steps += 1;
            self.evaluate_node(pos, world, registry);
        }
    }

    fn apply_scheduled_output(
        &mut self,
        pos: BlockPos,
        output: u8,
        world: &mut World,
        registry: &BlockRegistry,
    ) {
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        let Some(node) = def.circuit else {
            return;
        };
        match node.kind {
            CircuitKind::Repeater { .. }
            | CircuitKind::Inverter { .. }
            | CircuitKind::Observer { .. } => {
                self.set_published_power(pos, output, id, state, def, node.kind, world, registry);
            }
            CircuitKind::Piston { sticky } => {
                if output == 1 {
                    try_extend(self, world, registry, pos, sticky);
                } else {
                    try_retract(self, world, registry, pos, sticky);
                }
            }
            _ => {}
        }
    }

    fn notify_observers_watching(
        &mut self,
        changed_pos: BlockPos,
        world: &mut World,
        registry: &BlockRegistry,
    ) {
        for npos in crate::neighbors(changed_pos) {
            if !world.is_chunk_interactive(npos.chunk_pos()) {
                continue;
            }
            let (id, state) = world.get_block(npos);
            let Some(def) = registry.block(id) else {
                continue;
            };
            let Some(node) = def.circuit else {
                continue;
            };
            if !matches!(node.kind, CircuitKind::Observer { .. }) {
                continue;
            }
            if !observer_watches(npos, observer_facing(state), changed_pos) {
                continue;
            }
            self.trigger_observer(npos, world, registry);
        }
    }

    /// Fire an observer that was just moved by a piston. In Minecraft a moved
    /// observer emits a pulse at its new position; this is what self-sustains
    /// flying machines whose observers watch the air at the ends.
    pub(crate) fn fire_moved_observer(
        &mut self,
        pos: BlockPos,
        world: &mut World,
        registry: &BlockRegistry,
    ) {
        self.trigger_observer(pos, world, registry);
    }

    fn trigger_observer(&mut self, pos: BlockPos, world: &mut World, registry: &BlockRegistry) {
        if !world.is_chunk_interactive(pos.chunk_pos()) {
            return;
        }
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        let Some(node) = def.circuit else {
            return;
        };
        let kind = node.kind;
        if !matches!(kind, CircuitKind::Observer { .. }) {
            return;
        }

        self.queue.cancel_delay(pos);

        let current = self.power_at(pos);
        if current != 15 {
            self.set_published_power(pos, 15, id, state, def, kind, world, registry);
        }
        self.arm_delay(pos, 1, OBSERVER_PULSE_TICKS, 0);
    }

    fn drain_button_releases(&mut self, world: &mut World, registry: &BlockRegistry) {
        let due: Vec<BlockPos> = self
            .pending_button_release
            .iter()
            .filter(|(_, fire)| **fire <= self.tick)
            .map(|(pos, _)| *pos)
            .collect();

        for pos in due {
            self.pending_button_release.remove(&pos);
            let (id, state) = world.get_block(pos);
            let Some(def) = registry.block(id) else {
                continue;
            };
            if !def
                .circuit
                .is_some_and(|n| matches!(n.kind, CircuitKind::Switch { .. }))
            {
                continue;
            }
            if !mount_on(state) {
                continue;
            }
            let new_state = BlockState(state.0 & !1);
            world.set_block(pos, id, new_state);
            self.notify_block_changed(pos, world, registry);
        }
    }

    fn evaluate_node(&mut self, pos: BlockPos, world: &mut World, registry: &BlockRegistry) {
        if !world.is_chunk_interactive(pos.chunk_pos()) {
            return;
        }
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        let Some(node) = def.circuit else {
            return;
        };

        let prev_input = self.prev_delay_input(pos);
        let ctx = EvalContext {
            pos,
            state,
            circuit: self,
            world,
            registry,
        };

        let burnt_out = self.torch_burnt_out.contains(&pos);
        match dispatch(&ctx, node.kind, prev_input, burnt_out) {
            EvalResult::Unchanged => {}
            EvalResult::Publish(power) => {
                self.set_published_power(pos, power, id, state, def, node.kind, world, registry);
            }
            EvalResult::ArmDelay {
                input,
                target,
                delay_ticks,
            } => {
                self.arm_delay(pos, input, delay_ticks, target);
            }
        }
    }

    fn set_published_power(
        &mut self,
        pos: BlockPos,
        new_power: u8,
        id: stagcrest_protocol::BlockId,
        state: BlockState,
        def: &stagcrest_protocol::BlockDef,
        kind: CircuitKind,
        world: &mut World,
        registry: &BlockRegistry,
    ) {
        let current = self.power_at(pos);
        if new_power == current {
            return;
        }

        if matches!(
            kind,
            CircuitKind::Wire { .. } | CircuitKind::Repeater { .. } | CircuitKind::Observer { .. }
        ) {
            world.mark_dirty_face_neighbors(pos.chunk_pos());
        }

        if new_power == 0 {
            self.power.remove(&pos);
        } else {
            self.power.insert(pos, new_power);
        }
        self.power_updates.push((pos, new_power));
        self.mark_chunk_dirty(pos);

        if let Some(new_state) = sync_block_state(world, pos, id, def, kind, state, new_power) {
            self.visual_updates.push((id, pos, new_state));
            if matches!(kind, CircuitKind::Inverter { .. }) && is_torch_geometry(def) {
                self.record_torch_toggle(pos);
            }
            self.notify_observers_watching(pos, world, registry);
        }
        self.enqueue_circuit_neighbors(pos, world, registry);
        if matches!(kind, CircuitKind::Wire { .. }) {
            self.enqueue_wire_network_neighbors(pos, world, registry);
        }
        self.enqueue_block_power_dependents(pos, world, registry);
    }

    fn record_torch_toggle(&mut self, pos: BlockPos) {
        if self.torch_burnt_out.contains(&pos) {
            return;
        }
        let entry = self.torch_flicker.entry(pos).or_default();
        if self.tick.saturating_sub(entry.window_start) > BURNOUT_WINDOW_TICKS {
            entry.count = 0;
            entry.window_start = self.tick;
        }
        entry.count = entry.count.saturating_add(1);
        if entry.count > BURNOUT_TOGGLE_LIMIT {
            self.torch_burnt_out.insert(pos);
            self.torch_flicker.remove(&pos);
        }
    }

    fn enqueue_block_power_dependents(
        &mut self,
        pos: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) {
        self.enqueue_dependents_of_powered_block(pos, world, registry);
        for npos in crate::neighbors(pos) {
            self.enqueue_dependents_of_powered_block(npos, world, registry);
        }
    }

    fn enqueue_dependents_of_powered_block(
        &mut self,
        block_pos: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) {
        if !world.is_chunk_interactive(block_pos.chunk_pos()) {
            return;
        }
        let (id, _) = world.get_block(block_pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        if !def.redstone_powerable {
            return;
        }

        self.enqueue_attachment_nodes_on_block(block_pos, world, registry);
        for npos in crate::neighbors(block_pos) {
            if !world.is_chunk_interactive(npos.chunk_pos()) {
                continue;
            }
            let (nid, _) = world.get_block(npos);
            if registry.block(nid).and_then(|d| d.circuit).is_some() {
                self.queue.enqueue_evaluate(npos);
            }
        }
    }

    fn enqueue_attachment_nodes_on_block(
        &mut self,
        support: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) {
        if !world.is_chunk_interactive(support.chunk_pos()) {
            return;
        }
        let above = BlockPos::new(support.x, support.y + 1, support.z);
        self.maybe_queue_torch_on_support(above, support, world, registry);
        for npos in crate::neighbors(support) {
            self.maybe_queue_torch_on_support(npos, support, world, registry);
        }
    }

    fn maybe_queue_torch_on_support(
        &mut self,
        torch_pos: BlockPos,
        support: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) {
        if !world.is_chunk_interactive(torch_pos.chunk_pos()) {
            return;
        }
        let (id, state) = world.get_block(torch_pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        if !is_torch_geometry(def) {
            return;
        }
        if inverter_support_block(torch_pos, state) == support {
            self.queue.enqueue_evaluate(torch_pos);
        }
    }

    fn enqueue_circuit_neighbors(
        &mut self,
        pos: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) {
        for npos in crate::neighbors(pos) {
            if !world.is_chunk_interactive(npos.chunk_pos()) {
                continue;
            }
            let (nid, _) = world.get_block(npos);
            if registry.block(nid).and_then(|d| d.circuit).is_some() {
                self.queue.enqueue_evaluate(npos);
            }
        }
    }

    fn enqueue_wire_network_neighbors(
        &mut self,
        pos: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) {
        for npos in crate::wire_network::wire_network_neighbors(registry, world, pos) {
            if !world.is_chunk_interactive(npos.chunk_pos()) {
                continue;
            }
            let (nid, _) = world.get_block(npos);
            if registry.block(nid).and_then(|d| d.circuit).is_some() {
                self.queue.enqueue_evaluate(npos);
            }
        }
    }

    pub fn toggle_block(&mut self, pos: BlockPos, world: &mut World, registry: &BlockRegistry) {
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        let Some(node) = def.circuit else {
            return;
        };

        let new_state = match node.kind {
            CircuitKind::Source { .. } => return,
            CircuitKind::Inverter { .. } if is_torch_geometry(def) => {
                set_torch_lit(state, !stagcrest_protocol::torch_lit(state))
            }
            CircuitKind::Switch { .. } => {
                if is_button_geometry(Some(def)) {
                    if mount_on(state) {
                        return;
                    }
                    let on_state = BlockState(state.0 | 1);
                    world.set_block(pos, id, on_state);
                    self.pending_button_release
                        .insert(pos, self.tick + BUTTON_HOLD_TICKS);
                    self.notify_block_changed(pos, world, registry);
                    return;
                }
                BlockState(state.0 ^ 1)
            }
            CircuitKind::Inverter { .. }
            | CircuitKind::Wire { .. }
            | CircuitKind::Repeater { .. }
            | CircuitKind::Observer { .. }
            | CircuitKind::Piston { .. } => return,
        };

        world.set_block(pos, id, new_state);
        self.notify_block_changed(pos, world, registry);
    }

    /// Right-click: cycle repeater delay (1–4 ticks).
    pub fn cycle_repeater_delay(
        &mut self,
        pos: BlockPos,
        world: &mut World,
        registry: &BlockRegistry,
    ) {
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        let Some(node) = def.circuit else {
            return;
        };
        let CircuitKind::Repeater { .. } = node.kind else {
            return;
        };

        let new_state = stagcrest_protocol::cycle_repeater_delay(state);
        if new_state == state {
            return;
        }

        // Delay-only change: remesh via set_block, but don't reset delay_input or
        // cancel in-flight timers (output timing continues when cycling delay).
        world.set_block(pos, id, new_state);
    }
}
