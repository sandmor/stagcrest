use stagcrest_mod_host::{BlockRegistry, PowerLookup};
use stagcrest_protocol::{BlockPos, BlockState, CircuitKind, set_torch_lit};
use stagcrest_world::World;
use std::collections::HashMap;

use crate::eval::{dispatch, is_torch_geometry, sync_block_state, EvalContext, EvalResult};
use crate::event::{CircuitEvent, EventQueue};

pub const MAX_EVALS_PER_TICK: usize = 4096;

#[derive(Debug, Clone, Default)]
pub struct CircuitWorld {
    power: HashMap<BlockPos, u8>,
    queue: EventQueue,
    delay_input: HashMap<BlockPos, u8>,
    tick: u64,
}

impl CircuitWorld {
    pub fn new() -> Self {
        Self::default()
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
    }

    pub fn queue_update(&mut self, pos: BlockPos) {
        self.queue.enqueue_evaluate(pos);
    }

    pub fn notify_block_changed(
        &mut self,
        pos: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) {
        self.queue.cancel_delay(pos);
        self.delay_input.remove(&pos);

        let (id, _) = world.get_block(pos);
        if registry.block(id).and_then(|d| d.circuit).is_none() {
            self.power.remove(&pos);
        }

        self.queue_update(pos);
        self.enqueue_circuit_neighbors(pos, world, registry);
    }

    pub fn tick(&mut self, world: &mut World, registry: &BlockRegistry) {
        self.tick = self.tick.saturating_add(1);

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
            CircuitKind::Delay { .. } | CircuitKind::Repeater { .. } => {
                self.set_published_power(pos, output, id, state, def, node.kind, world, registry);
            }
            _ => {}
        }
    }

    fn evaluate_node(&mut self, pos: BlockPos, world: &mut World, registry: &BlockRegistry) {
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

        match dispatch(&ctx, node.kind, prev_input) {
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
            CircuitKind::Wire { .. } | CircuitKind::Delay { .. } | CircuitKind::Repeater { .. }
        ) {
            world.mark_dirty_and_neighbors(pos);
        }

        if new_power == 0 {
            self.power.remove(&pos);
        } else {
            self.power.insert(pos, new_power);
        }

        sync_block_state(world, pos, id, def, kind, state, new_power);
        self.enqueue_circuit_neighbors(pos, world, registry);
    }

    fn enqueue_circuit_neighbors(
        &mut self,
        pos: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) {
        for npos in crate::neighbors(pos) {
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
            CircuitKind::Switch { .. } => BlockState(state.0 ^ 1),
            CircuitKind::Inverter { .. }
            | CircuitKind::Wire { .. }
            | CircuitKind::Delay { .. }
            | CircuitKind::Repeater { .. } => return,
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
        // cancel in-flight timers (vanilla keeps timing when cycling delay).
        world.set_block(pos, id, new_state);
    }
}

impl PowerLookup for CircuitWorld {
    fn power_at(&self, pos: BlockPos) -> u8 {
        CircuitWorld::power_at(self, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{
        BlockDef, BlockFaceTextures, BlockGeometry, BlockId, CircuitKind, CircuitNodeDef,
        Facing, ModelId, TextureId, repeater_state,
    };

    fn test_block(id: BlockId, kind: CircuitKind) -> BlockDef {
        test_block_with_geometry(id, kind, BlockGeometry::Cube)
    }

    fn test_block_with_geometry(id: BlockId, kind: CircuitKind, geometry: BlockGeometry) -> BlockDef {
        BlockDef {
            id,
            namespaced_id: format!("test:{id:?}"),
            display_name: "Test".into(),
            opaque: true,
            transparent: false,
            solid: true,
            hardness: 1.0,
            face_textures: BlockFaceTextures::uniform(TextureId(0)),
            circuit: Some(CircuitNodeDef { kind }),
            placeable: true,
            geometry,
            fluid: false,
        }
    }

    fn setup_registry() -> (BlockRegistry, BlockId, BlockId, BlockId, BlockId, BlockId, BlockId) {
        let mut reg = BlockRegistry::new();
        let source = BlockId(1);
        let wire = BlockId(2);
        let inverter = BlockId(3);
        let switch = BlockId(4);
        let delay = BlockId(5);
        let repeater = BlockId(6);

        reg.register_block(test_block(
            source,
            CircuitKind::Source { level: 15 },
        ));
        reg.register_block(test_block(wire, CircuitKind::Wire { falloff: 1 }));
        reg.register_block(test_block(
            inverter,
            CircuitKind::Inverter { output: 15 },
        ));
        reg.register_block(test_block(
            switch,
            CircuitKind::Switch { output: 15 },
        ));
        reg.register_block(test_block(
            delay,
            CircuitKind::Delay {
                output: 15,
                delay: 2,
            },
        ));
        reg.register_block(test_block_with_geometry(
            repeater,
            CircuitKind::Repeater { output: 15 },
            BlockGeometry::Model(ModelId::Repeater),
        ));

        (reg, source, wire, inverter, switch, delay, repeater)
    }

    fn settle(circuit: &mut CircuitWorld, world: &mut World, reg: &BlockRegistry, ticks: u64) {
        for _ in 0..ticks {
            circuit.tick(world, reg);
        }
    }

    #[test]
    fn wire_falloff_chain() {
        let (reg, source, wire, _, _, _, _) = setup_registry();
        let mut world = World::new(BlockId(0));
        let mut circuit = CircuitWorld::new();

        world.set_block(BlockPos::new(0, 0, 0), source, BlockState(0));
        world.set_block(BlockPos::new(1, 0, 0), wire, BlockState(0));
        world.set_block(BlockPos::new(2, 0, 0), wire, BlockState(0));
        world.set_block(BlockPos::new(3, 0, 0), wire, BlockState(0));

        circuit.notify_block_changed(BlockPos::new(0, 0, 0), &world, &reg);
        settle(&mut circuit, &mut world, &reg, 4);

        assert_eq!(circuit.power_at(BlockPos::new(1, 0, 0)), 14);
        assert_eq!(circuit.power_at(BlockPos::new(2, 0, 0)), 13);
        assert_eq!(circuit.power_at(BlockPos::new(3, 0, 0)), 12);
    }

    #[test]
    fn inverter_turns_off_when_powered() {
        let (reg, source, wire, inverter, _, _, _) = setup_registry();
        let mut world = World::new(BlockId(0));
        let mut circuit = CircuitWorld::new();

        world.set_block(BlockPos::new(0, 0, 0), source, BlockState(0));
        world.set_block(BlockPos::new(1, 0, 0), wire, BlockState(0));
        world.set_block(BlockPos::new(2, 0, 0), inverter, BlockState(0));

        circuit.notify_block_changed(BlockPos::new(0, 0, 0), &world, &reg);
        settle(&mut circuit, &mut world, &reg, 4);

        assert_eq!(circuit.power_at(BlockPos::new(2, 0, 0)), 0);
    }

    #[test]
    fn switch_emits_when_on() {
        let (reg, _, wire, _, switch, _, _) = setup_registry();
        let mut world = World::new(BlockId(0));
        let mut circuit = CircuitWorld::new();

        world.set_block(BlockPos::new(0, 0, 0), switch, BlockState(1));
        world.set_block(BlockPos::new(1, 0, 0), wire, BlockState(0));

        circuit.notify_block_changed(BlockPos::new(0, 0, 0), &world, &reg);
        settle(&mut circuit, &mut world, &reg, 2);

        assert_eq!(circuit.power_at(BlockPos::new(1, 0, 0)), 14);
    }

    #[test]
    fn generic_delay_applies_after_ticks() {
        let (reg, source, wire, _, _, delay, _) = setup_registry();
        let mut world = World::new(BlockId(0));
        let mut circuit = CircuitWorld::new();

        world.set_block(BlockPos::new(0, 0, 0), source, BlockState(0));
        world.set_block(BlockPos::new(1, 0, 0), wire, BlockState(0));
        world.set_block(BlockPos::new(2, 0, 0), delay, BlockState(0));
        world.set_block(BlockPos::new(3, 0, 0), wire, BlockState(0));

        circuit.notify_block_changed(BlockPos::new(0, 0, 0), &world, &reg);
        settle(&mut circuit, &mut world, &reg, 1);
        assert_eq!(circuit.power_at(BlockPos::new(2, 0, 0)), 0);

        settle(&mut circuit, &mut world, &reg, 2);
        assert_eq!(circuit.power_at(BlockPos::new(2, 0, 0)), 15);
        assert_eq!(circuit.power_at(BlockPos::new(3, 0, 0)), 14);
    }

    #[test]
    fn repeater_applies_after_ticks() {
        let (reg, source, wire, _, _, _, repeater) = setup_registry();
        let mut world = World::new(BlockId(0));
        let mut circuit = CircuitWorld::new();

        world.set_block(BlockPos::new(0, 0, 0), source, BlockState(0));
        world.set_block(BlockPos::new(1, 0, 0), wire, BlockState(0));
        world.set_block(
            BlockPos::new(2, 0, 0),
            repeater,
            repeater_state(false, Facing::East, 2),
        );
        world.set_block(BlockPos::new(3, 0, 0), wire, BlockState(0));

        circuit.notify_block_changed(BlockPos::new(0, 0, 0), &world, &reg);
        settle(&mut circuit, &mut world, &reg, 1);
        assert_eq!(circuit.power_at(BlockPos::new(2, 0, 0)), 0);

        settle(&mut circuit, &mut world, &reg, 2);
        assert_eq!(circuit.power_at(BlockPos::new(2, 0, 0)), 15);
        assert_eq!(circuit.power_at(BlockPos::new(3, 0, 0)), 14);
    }

    #[test]
    fn repeater_supersedes_when_input_drops() {
        let (reg, source, wire, _, _, _, repeater) = setup_registry();
        let mut world = World::new(BlockId(0));
        let mut circuit = CircuitWorld::new();
        let repeater_pos = BlockPos::new(2, 0, 0);

        world.set_block(BlockPos::new(0, 0, 0), source, BlockState(0));
        world.set_block(BlockPos::new(1, 0, 0), wire, BlockState(0));
        world.set_block(
            repeater_pos,
            repeater,
            repeater_state(false, Facing::East, 2),
        );

        circuit.notify_block_changed(BlockPos::new(0, 0, 0), &world, &reg);
        circuit.tick(&mut world, &reg);

        world.set_block(BlockPos::new(0, 0, 0), BlockId(0), BlockState(0));
        circuit.notify_block_changed(BlockPos::new(0, 0, 0), &world, &reg);
        settle(&mut circuit, &mut world, &reg, 4);

        assert_eq!(circuit.power_at(repeater_pos), 0);
    }

    #[test]
    fn repeater_ignores_signal_on_output_face() {
        let (reg, source, _, _, _, _, repeater) = setup_registry();
        let mut world = World::new(BlockId(0));
        let mut circuit = CircuitWorld::new();
        let repeater_pos = BlockPos::new(2, 0, 0);

        world.set_block(
            repeater_pos,
            repeater,
            repeater_state(false, Facing::East, 2),
        );
        world.set_block(BlockPos::new(3, 0, 0), source, BlockState(0));

        circuit.notify_block_changed(BlockPos::new(3, 0, 0), &world, &reg);
        settle(&mut circuit, &mut world, &reg, 8);

        assert_eq!(circuit.power_at(repeater_pos), 0);
    }

    #[test]
    fn repeater_cycle_delay_keeps_output_while_powered() {
        let (reg, source, wire, _, _, _, repeater) = setup_registry();
        let mut world = World::new(BlockId(0));
        let mut circuit = CircuitWorld::new();
        let repeater_pos = BlockPos::new(2, 0, 0);

        world.set_block(BlockPos::new(0, 0, 0), source, BlockState(0));
        world.set_block(BlockPos::new(1, 0, 0), wire, BlockState(0));
        world.set_block(
            repeater_pos,
            repeater,
            repeater_state(false, Facing::East, 2),
        );
        world.set_block(BlockPos::new(3, 0, 0), wire, BlockState(0));

        circuit.notify_block_changed(BlockPos::new(0, 0, 0), &world, &reg);
        settle(&mut circuit, &mut world, &reg, 3);
        assert_eq!(circuit.power_at(repeater_pos), 15);

        circuit.cycle_repeater_delay(repeater_pos, &mut world, &reg);
        assert_eq!(circuit.power_at(repeater_pos), 15);
        settle(&mut circuit, &mut world, &reg, 2);
        assert_eq!(circuit.power_at(repeater_pos), 15);
    }
}
