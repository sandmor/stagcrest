mod common;

use common::{place_wall_torch_not_gate, populate_chunks, settle, setup_registry};
use stagcrest_circuit::{block_power_at, CircuitWorld};
use stagcrest_protocol::{
    mount_on, mount_state, observer_state, repeater_state, torch_state, AttachFace, BlockId,
    BlockPos, BlockState, Facing, TorchAttachment, CHUNK_SIZE,
};
use stagcrest_world::World;

#[test]
fn chunk_not_ready_skips_evaluation() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let source_pos = BlockPos::new(0, 0, 0);
    let wire_pos = BlockPos::new(CHUNK_SIZE, 0, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(wire_pos, blocks.wire, BlockState(0));
    populate_chunks(&mut world, &[source_pos]);
    world.mark_chunk_terrain_ready(wire_pos.chunk_pos());
    assert!(!world.is_chunk_interactive(wire_pos.chunk_pos()));

    circuit.queue_update(wire_pos);
    circuit.tick(&mut world, &reg);
    assert_eq!(circuit.power_at(wire_pos), 0);
}

#[test]
fn snapshot_roundtrip() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let repeater_pos = BlockPos::new(2, 0, 0);
    let chunk = repeater_pos.chunk_pos();

    world.set_block(BlockPos::new(0, 0, 0), blocks.source, BlockState(0));
    world.set_block(BlockPos::new(1, 0, 0), blocks.wire, BlockState(0));
    world.set_block(
        repeater_pos,
        blocks.repeater,
        repeater_state(false, Facing::East, 2),
    );
    populate_chunks(
        &mut world,
        &[
            BlockPos::new(0, 0, 0),
            BlockPos::new(1, 0, 0),
            repeater_pos,
        ],
    );

    circuit.notify_block_changed(BlockPos::new(0, 0, 0), &mut world, &reg);
    circuit.tick(&mut world, &reg);
    let exported = circuit.export_chunk_snapshot(chunk);

    let mut restored = CircuitWorld::new();
    restored.set_tick(circuit.current_tick());
    restored.import_chunk_snapshot(chunk, exported, circuit.current_tick());
    assert_eq!(restored.power_at(repeater_pos), circuit.power_at(repeater_pos));
}

#[test]
fn wire_signal_falloff_over_line() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let positions = [
        BlockPos::new(0, 0, 0),
        BlockPos::new(1, 0, 0),
        BlockPos::new(2, 0, 0),
        BlockPos::new(3, 0, 0),
    ];
    world.set_block(positions[0], blocks.source, BlockState(0));
    world.set_block(positions[1], blocks.wire, BlockState(0));
    world.set_block(positions[2], blocks.wire, BlockState(0));
    world.set_block(positions[3], blocks.wire, BlockState(0));
    populate_chunks(&mut world, &positions);

    circuit.notify_block_changed(positions[0], &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);

    assert_eq!(circuit.power_at(positions[1]), 14);
    assert_eq!(circuit.power_at(positions[2]), 13);
    assert_eq!(circuit.power_at(positions[3]), 12);
}

#[test]
fn wire_signal_climbs_block_step() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let positions = [
        BlockPos::new(0, 0, 0),
        BlockPos::new(1, 0, 0),
        BlockPos::new(2, 0, 0),
        BlockPos::new(2, 1, 0),
    ];
    world.set_block(positions[0], blocks.source, BlockState(0));
    world.set_block(positions[1], blocks.wire, BlockState(0));
    world.set_block(positions[2], blocks.stone, BlockState(0));
    world.set_block(positions[3], blocks.wire, BlockState(0));
    populate_chunks(&mut world, &positions);

    circuit.notify_block_changed(positions[0], &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);

    assert_eq!(circuit.power_at(positions[1]), 14);
    assert_eq!(circuit.power_at(positions[3]), 13);
}

#[test]
fn inverter_off_when_support_block_powered() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let (torch_pos, lever_pos) = place_wall_torch_not_gate(&mut world, &blocks, false);
    circuit.notify_block_changed(torch_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 6);
    assert_eq!(circuit.power_at(torch_pos), 15);

    world.set_block(lever_pos, blocks.switch, BlockState(1));
    circuit.notify_block_changed(lever_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 6);
    assert_eq!(circuit.power_at(torch_pos), 0);
}

#[test]
fn inverter_ignores_colinear_wire_not_support() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let lever_pos = BlockPos::new(0, 0, 0);
    let wire_pos = BlockPos::new(1, 0, 0);
    let torch_pos = BlockPos::new(2, 0, 0);

    world.set_block(lever_pos, blocks.switch, BlockState(1));
    world.set_block(wire_pos, blocks.wire, BlockState(0));
    world.set_block(
        torch_pos,
        blocks.torch,
        torch_state(false, TorchAttachment::Floor),
    );
    populate_chunks(&mut world, &[lever_pos, wire_pos, torch_pos]);

    circuit.notify_block_changed(lever_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 6);
    assert_eq!(circuit.power_at(torch_pos), 15);
}

#[test]
fn inverter_output_delay_two_ticks() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let (torch_pos, lever_pos) = place_wall_torch_not_gate(&mut world, &blocks, false);
    circuit.notify_block_changed(torch_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);
    assert_eq!(circuit.power_at(torch_pos), 15);

    world.set_block(lever_pos, blocks.switch, BlockState(1));
    circuit.notify_block_changed(lever_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 1);
    assert_eq!(circuit.power_at(torch_pos), 15);

    settle(&mut circuit, &mut world, &reg, 2);
    assert_eq!(circuit.power_at(torch_pos), 0);
}

#[test]
fn switch_pulse_releases_after_hold_ticks() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let support = BlockPos::new(0, 0, 0);
    let button_pos = BlockPos::new(1, 0, 0);
    let wire_pos = BlockPos::new(2, 0, 0);

    world.set_block(support, blocks.stone, BlockState(0));
    world.set_block(
        button_pos,
        blocks.button,
        mount_state(false, AttachFace::Wall, Facing::East),
    );
    world.set_block(wire_pos, blocks.wire, BlockState(0));
    populate_chunks(&mut world, &[support, button_pos, wire_pos]);

    circuit.toggle_block(button_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 1);
    assert!(mount_on(world.get_block(button_pos).1));
    assert_eq!(circuit.power_at(wire_pos), 14);

    settle(&mut circuit, &mut world, &reg, 28);
    assert!(mount_on(world.get_block(button_pos).1));
    assert_eq!(circuit.power_at(wire_pos), 14);

    settle(&mut circuit, &mut world, &reg, 2);
    assert!(!mount_on(world.get_block(button_pos).1));
    assert_eq!(circuit.power_at(wire_pos), 0);
}

#[test]
fn repeater_locks_when_side_output_faces_in() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let forward_input = BlockPos::new(0, 0, 0);
    let forward_pos = BlockPos::new(2, 0, 0);
    let forward_output = BlockPos::new(3, 0, 0);
    let side_input = BlockPos::new(2, 0, 2);
    let side_pos = BlockPos::new(2, 0, 1);

    world.set_block(forward_input, blocks.source, BlockState(0));
    world.set_block(BlockPos::new(1, 0, 0), blocks.wire, BlockState(0));
    world.set_block(
        forward_pos,
        blocks.repeater,
        repeater_state(false, Facing::East, 1),
    );
    world.set_block(forward_output, blocks.wire, BlockState(0));
    world.set_block(side_input, blocks.source, BlockState(0));
    world.set_block(BlockPos::new(2, 0, 3), blocks.wire, BlockState(0));
    world.set_block(
        side_pos,
        blocks.repeater,
        repeater_state(false, Facing::North, 1),
    );
    populate_chunks(
        &mut world,
        &[
            forward_input,
            BlockPos::new(1, 0, 0),
            forward_pos,
            forward_output,
            side_input,
            BlockPos::new(2, 0, 3),
            side_pos,
        ],
    );

    circuit.notify_block_changed(forward_input, &mut world, &reg);
    circuit.notify_block_changed(side_input, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);
    assert_eq!(circuit.power_at(forward_pos), 15);

    world.set_block(forward_input, BlockId(0), BlockState(0));
    world.set_block(BlockPos::new(1, 0, 0), BlockId(0), BlockState(0));
    circuit.notify_block_changed(forward_input, &mut world, &reg);
    circuit.notify_block_changed(BlockPos::new(1, 0, 0), &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);
    assert_eq!(circuit.power_at(forward_pos), 15);
}

#[test]
fn source_strongly_powers_adjacent_opaque_block() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let source_pos = BlockPos::new(0, 0, 0);
    let block_pos = BlockPos::new(1, 0, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(block_pos, blocks.stone, BlockState(0));
    populate_chunks(&mut world, &[source_pos, block_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 1);

    let power = block_power_at(&circuit, block_pos, &mut world, &reg);
    assert_eq!(power.strong, 15);
}

#[test]
fn repeater_ignores_back_feed() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let repeater_pos = BlockPos::new(2, 0, 0);

    world.set_block(
        repeater_pos,
        blocks.repeater,
        repeater_state(false, Facing::East, 2),
    );
    world.set_block(BlockPos::new(3, 0, 0), blocks.source, BlockState(0));
    populate_chunks(&mut world, &[repeater_pos, BlockPos::new(3, 0, 0)]);

    circuit.notify_block_changed(BlockPos::new(3, 0, 0), &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 6);
    assert_eq!(circuit.power_at(repeater_pos), 0);
}

#[test]
fn repeater_clears_after_input_drops() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let repeater_pos = BlockPos::new(2, 0, 0);

    world.set_block(BlockPos::new(0, 0, 0), blocks.source, BlockState(0));
    world.set_block(BlockPos::new(1, 0, 0), blocks.wire, BlockState(0));
    world.set_block(
        repeater_pos,
        blocks.repeater,
        repeater_state(false, Facing::East, 2),
    );
    populate_chunks(
        &mut world,
        &[BlockPos::new(0, 0, 0), BlockPos::new(1, 0, 0), repeater_pos],
    );

    circuit.notify_block_changed(BlockPos::new(0, 0, 0), &mut world, &reg);
    circuit.tick(&mut world, &reg);

    world.set_block(BlockPos::new(0, 0, 0), BlockId(0), BlockState(0));
    circuit.notify_block_changed(BlockPos::new(0, 0, 0), &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);
    assert_eq!(circuit.power_at(repeater_pos), 0);
}

fn place_observer_gate(
    world: &mut World,
    blocks: &common::TestBlocks,
    facing: Facing,
) -> (BlockPos, BlockPos, BlockPos) {
    let watch_pos = BlockPos::new(0, 0, 0);
    let observer_pos = BlockPos::new(1, 0, 0);
    let output_pos = BlockPos::new(2, 0, 0);

    world.set_block(watch_pos, blocks.stone, BlockState(0));
    world.set_block(
        observer_pos,
        blocks.observer,
        observer_state(false, facing),
    );
    world.set_block(output_pos, blocks.wire, BlockState(0));
    populate_chunks(world, &[watch_pos, observer_pos, output_pos]);

    (watch_pos, observer_pos, output_pos)
}

#[test]
fn observer_behavior() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let (watch_pos, observer_pos, output_pos) =
        place_observer_gate(&mut world, &blocks, Facing::East);

    world.set_block(watch_pos, blocks.stone, BlockState(1));
    circuit.notify_block_changed(watch_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 1);
    assert_eq!(circuit.power_at(observer_pos), 15);
    assert_eq!(circuit.power_at(output_pos), 14);

    settle(&mut circuit, &mut world, &reg, 1);
    assert_eq!(circuit.power_at(observer_pos), 0);
    assert_eq!(circuit.power_at(output_pos), 0);

    circuit.notify_block_changed(watch_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 1);
    assert_eq!(circuit.power_at(observer_pos), 15);

    world.set_block(watch_pos, blocks.stone, BlockState(2));
    circuit.notify_block_changed(watch_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 1);
    assert_eq!(circuit.power_at(observer_pos), 15);

    settle(&mut circuit, &mut world, &reg, 2);
    assert_eq!(circuit.power_at(observer_pos), 0);
    assert_eq!(circuit.power_at(output_pos), 0);

    let west_wire = BlockPos::new(0, 0, 1);
    world.set_block(west_wire, blocks.wire, BlockState(0));
    populate_chunks(&mut world, &[west_wire]);
    world.set_block(watch_pos, blocks.stone, BlockState(3));
    circuit.notify_block_changed(watch_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 1);
    assert_eq!(circuit.power_at(output_pos), 14);
    assert_eq!(circuit.power_at(west_wire), 0);

    let source_pos = BlockPos::new(-1, 0, 0);
    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(watch_pos, blocks.wire, BlockState(0));
    populate_chunks(&mut world, &[source_pos, watch_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 1);
    assert_eq!(circuit.power_at(observer_pos), 15);
    assert_eq!(circuit.power_at(output_pos), 14);

    settle(&mut circuit, &mut world, &reg, 2);
    assert_eq!(circuit.power_at(observer_pos), 0);
    assert_eq!(circuit.power_at(output_pos), 0);
}
