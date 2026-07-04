mod common;

use common::{place_wall_torch_not_gate, populate_chunks, settle, setup_registry, TestBlocks};
use stagcrest_circuit::{
    block_power_at, wire_network::wire_connections_at, CircuitWorld, WireLink,
};
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
        &[BlockPos::new(0, 0, 0), BlockPos::new(1, 0, 0), repeater_pos],
    );

    circuit.notify_block_changed(BlockPos::new(0, 0, 0), &mut world, &reg);
    circuit.tick(&mut world, &reg);
    let exported = circuit.export_chunk_snapshot(chunk);

    let mut restored = CircuitWorld::new();
    restored.set_tick(circuit.current_tick());
    restored.import_chunk_snapshot(chunk, exported, circuit.current_tick());
    assert_eq!(
        restored.power_at(repeater_pos),
        circuit.power_at(repeater_pos)
    );
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

    assert_eq!(circuit.power_at(positions[1]), 15);
    assert_eq!(circuit.power_at(positions[2]), 14);
    assert_eq!(circuit.power_at(positions[3]), 13);
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

    assert_eq!(circuit.power_at(positions[1]), 15);
    assert_eq!(circuit.power_at(positions[3]), 14);
}

#[test]
fn dust_does_not_connect_to_repeater_side_or_observer_input() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let wire_pos = BlockPos::new(0, 0, 0);
    let repeater_side = BlockPos::new(1, 0, 0);
    let observer_input = BlockPos::new(-1, 0, 0);

    world.set_block(wire_pos, blocks.wire, BlockState(0));
    world.set_block(
        repeater_side,
        blocks.repeater,
        repeater_state(false, Facing::North, 1),
    );
    world.set_block(
        observer_input,
        blocks.observer,
        observer_state(false, Facing::West),
    );
    populate_chunks(&mut world, &[wire_pos, repeater_side, observer_input]);

    let connections = wire_connections_at(&reg, &world, wire_pos);
    assert_eq!(
        connections.side(1),
        WireLink::None,
        "dust ignores repeater side"
    );
    assert_eq!(
        connections.side(3),
        WireLink::None,
        "dust ignores observer input face"
    );
}

#[test]
fn dust_connects_to_powerable_blocks_but_not_transparent_powerless_blocks() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let wire_pos = BlockPos::new(0, 0, 0);
    let stone_pos = BlockPos::new(1, 0, 0);
    let glass_pos = BlockPos::new(-1, 0, 0);
    let torch_pos = BlockPos::new(0, 0, -1);

    world.set_block(wire_pos, blocks.wire, BlockState(0));
    world.set_block(stone_pos, blocks.stone, BlockState(0));
    world.set_block(glass_pos, blocks.glass, BlockState(0));
    world.set_block(
        torch_pos,
        blocks.torch,
        torch_state(false, TorchAttachment::Floor),
    );
    populate_chunks(&mut world, &[wire_pos, stone_pos, glass_pos, torch_pos]);

    let connections = wire_connections_at(&reg, &world, wire_pos);
    assert_eq!(
        connections.side(0),
        WireLink::Side,
        "setup keeps the wire from mirroring east"
    );
    assert_eq!(
        connections.side(1),
        WireLink::Side,
        "Bedrock dust points into powerable blocks"
    );
    assert_eq!(
        connections.side(3),
        WireLink::None,
        "dust ignores non-powerable transparent blocks"
    );
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

    world.set_block(
        lever_pos,
        blocks.switch,
        mount_state(true, AttachFace::Wall, Facing::West),
    );
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

    world.set_block(
        lever_pos,
        blocks.switch,
        mount_state(true, AttachFace::Wall, Facing::West),
    );
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
    assert_eq!(circuit.power_at(wire_pos), 15);

    settle(&mut circuit, &mut world, &reg, 28);
    assert!(mount_on(world.get_block(button_pos).1));
    assert_eq!(circuit.power_at(wire_pos), 15);

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
    world.set_block(observer_pos, blocks.observer, observer_state(false, facing));
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
    assert_eq!(circuit.power_at(output_pos), 15);

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
    assert_eq!(circuit.power_at(output_pos), 15);
    assert_eq!(circuit.power_at(west_wire), 0);

    let source_pos = BlockPos::new(-1, 0, 0);
    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(watch_pos, blocks.wire, BlockState(0));
    populate_chunks(&mut world, &[source_pos, watch_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 1);
    assert_eq!(circuit.power_at(observer_pos), 15);
    assert_eq!(circuit.power_at(output_pos), 15);

    settle(&mut circuit, &mut world, &reg, 2);
    assert_eq!(circuit.power_at(observer_pos), 0);
    assert_eq!(circuit.power_at(output_pos), 0);
}

fn place_horizontal_piston(
    world: &mut World,
    blocks: &TestBlocks,
    piston_pos: BlockPos,
    sticky: bool,
    powered: bool,
) -> BlockPos {
    use stagcrest_protocol::{piston_state, Facing6};
    let input_pos = BlockPos::new(piston_pos.x - 1, piston_pos.y, piston_pos.z);
    let front_pos = BlockPos::new(piston_pos.x + 1, piston_pos.y, piston_pos.z);
    let piston_id = if sticky {
        blocks.sticky_piston
    } else {
        blocks.piston
    };
    world.set_block(piston_pos, piston_id, piston_state(false, Facing6::East));
    if powered {
        world.set_block(input_pos, blocks.source, BlockState(0));
    }
    populate_chunks(world, &[piston_pos, input_pos, front_pos]);
    front_pos
}

#[test]
fn piston_extends_and_retracts() {
    use stagcrest_protocol::piston_extended;

    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let piston_pos = BlockPos::new(0, 0, 0);
    let front_pos = place_horizontal_piston(&mut world, &blocks, piston_pos, false, true);
    let push_target = BlockPos::new(2, 0, 0);
    world.set_block(front_pos, blocks.stone, BlockState(0));
    populate_chunks(&mut world, &[push_target]);

    circuit.notify_block_changed(piston_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 2);

    let (_, piston_state_after) = world.get_block(piston_pos);
    assert!(piston_extended(piston_state_after));
    let (head_id, _) = world.get_block(front_pos);
    assert_eq!(head_id, blocks.piston_head);
    let (stone_id, _) = world.get_block(push_target);
    assert_eq!(stone_id, blocks.stone);

    world.set_block(BlockPos::new(-1, 0, 0), blocks.stone, BlockState(0));
    circuit.notify_block_changed(BlockPos::new(-1, 0, 0), &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 2);

    let (_, retracted) = world.get_block(piston_pos);
    assert!(!piston_extended(retracted));
    let (front_id, _) = world.get_block(front_pos);
    assert_eq!(front_id, BlockId(0));
    let (beyond_id, _) = world.get_block(push_target);
    assert_eq!(beyond_id, blocks.stone);
}

#[test]
fn sticky_piston_pulls_block() {
    use stagcrest_protocol::{piston_extended, piston_head_state, piston_state, Facing6};

    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let piston_pos = BlockPos::new(0, 0, 0);
    let front_pos = BlockPos::new(1, 0, 0);
    let beyond = BlockPos::new(2, 0, 0);
    world.set_block(
        piston_pos,
        blocks.sticky_piston,
        piston_state(true, Facing6::East),
    );
    world.set_block(
        front_pos,
        blocks.piston_head,
        piston_head_state(Facing6::East, true),
    );
    world.set_block(beyond, blocks.stone, BlockState(0));
    populate_chunks(&mut world, &[piston_pos, front_pos, beyond]);

    circuit.notify_block_changed(piston_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 2);

    let (_, state) = world.get_block(piston_pos);
    assert!(!piston_extended(state));
    let (front_id, _) = world.get_block(front_pos);
    assert_eq!(front_id, blocks.stone);
    let (beyond_id, _) = world.get_block(beyond);
    assert_eq!(beyond_id, BlockId(0));
}

#[test]
fn normal_piston_does_not_pull() {
    use stagcrest_protocol::{piston_head_state, piston_state, Facing6};

    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let piston_pos = BlockPos::new(0, 0, 0);
    let front_pos = BlockPos::new(1, 0, 0);
    let beyond = BlockPos::new(2, 0, 0);
    world.set_block(piston_pos, blocks.piston, piston_state(true, Facing6::East));
    world.set_block(
        front_pos,
        blocks.piston_head,
        piston_head_state(Facing6::East, false),
    );
    world.set_block(beyond, blocks.stone, BlockState(0));
    populate_chunks(&mut world, &[piston_pos, front_pos, beyond]);

    circuit.notify_block_changed(piston_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 2);

    let (beyond_id, _) = world.get_block(beyond);
    assert_eq!(beyond_id, blocks.stone);
}

#[test]
fn piston_push_limit_and_bedrock() {
    use stagcrest_protocol::{piston_state, Facing6};

    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let piston_pos = BlockPos::new(0, 0, 0);
    world.set_block(BlockPos::new(-1, 0, 0), blocks.source, BlockState(0));
    world.set_block(
        piston_pos,
        blocks.piston,
        piston_state(false, Facing6::East),
    );
    let mut positions = vec![piston_pos, BlockPos::new(-1, 0, 0)];
    for i in 1..=12 {
        let pos = BlockPos::new(i, 0, 0);
        world.set_block(pos, blocks.stone, BlockState(0));
        positions.push(pos);
    }
    populate_chunks(&mut world, &positions);

    circuit.notify_block_changed(piston_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 2);
    let (_, extended) = world.get_block(piston_pos);
    assert!(stagcrest_protocol::piston_extended(extended));
    let (end_id, _) = world.get_block(BlockPos::new(13, 0, 0));
    assert_eq!(end_id, blocks.stone);

    world.set_block(
        piston_pos,
        blocks.piston,
        piston_state(false, Facing6::East),
    );
    world.set_block(BlockPos::new(13, 0, 0), blocks.bedrock, BlockState(0));
    for i in 1..=12 {
        world.set_block(BlockPos::new(i, 0, 0), blocks.stone, BlockState(0));
    }
    circuit.notify_block_changed(piston_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 2);
    let (_, not_extended) = world.get_block(piston_pos);
    assert!(!stagcrest_protocol::piston_extended(not_extended));
}

#[test]
fn piston_pushes_slime_only() {
    use stagcrest_protocol::{piston_state, Facing6};

    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let piston_pos = BlockPos::new(0, 0, 0);
    let front_pos = BlockPos::new(1, 0, 0);
    world.set_block(BlockPos::new(-1, 0, 0), blocks.source, BlockState(0));
    world.set_block(
        piston_pos,
        blocks.piston,
        piston_state(false, Facing6::East),
    );
    world.set_block(front_pos, blocks.slime, BlockState(0));
    populate_chunks(
        &mut world,
        &[
            BlockPos::new(-1, 0, 0),
            piston_pos,
            front_pos,
            BlockPos::new(2, 0, 0),
        ],
    );

    circuit.notify_block_changed(piston_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 2);

    assert_eq!(world.get_block(BlockPos::new(2, 0, 0)).0, blocks.slime);
}

#[test]
fn slime_block_drags_adjacent_stone() {
    use stagcrest_protocol::{piston_state, Facing6};

    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();
    let piston_pos = BlockPos::new(0, 0, 0);
    let slime_pos = BlockPos::new(1, 0, 0);
    let rider_pos = BlockPos::new(1, 1, 0);
    let dest_slime = BlockPos::new(2, 0, 0);
    let dest_rider = BlockPos::new(2, 1, 0);
    world.set_block(BlockPos::new(-1, 0, 0), blocks.source, BlockState(0));
    world.set_block(
        piston_pos,
        blocks.piston,
        piston_state(false, Facing6::East),
    );
    world.set_block(slime_pos, blocks.slime, BlockState(0));
    world.set_block(rider_pos, blocks.stone, BlockState(0));
    populate_chunks(
        &mut world,
        &[
            BlockPos::new(-1, 0, 0),
            piston_pos,
            slime_pos,
            rider_pos,
            dest_slime,
            dest_rider,
        ],
    );

    circuit.notify_block_changed(piston_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 2);

    assert_eq!(world.get_block(dest_slime).0, blocks.slime);
    assert_eq!(world.get_block(dest_rider).0, blocks.stone);
}

#[test]
fn flying_machine_cycle_advances() {
    use stagcrest_protocol::{observer_state, piston_state, Facing, Facing6};

    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let piston_pos = BlockPos::new(0, 0, 0);
    let slime_pos = BlockPos::new(1, 0, 0);
    let observer_pos = BlockPos::new(1, 0, 1);
    let watch_pos = BlockPos::new(2, 0, 1);

    world.set_block(
        piston_pos,
        blocks.sticky_piston,
        piston_state(false, Facing6::East),
    );
    world.set_block(slime_pos, blocks.slime, BlockState(0));
    world.set_block(
        observer_pos,
        blocks.observer,
        observer_state(false, Facing::East),
    );
    world.set_block(watch_pos, blocks.stone, BlockState(0));
    world.set_block(BlockPos::new(-1, 0, 0), blocks.source, BlockState(0));
    populate_chunks(
        &mut world,
        &[
            BlockPos::new(-1, 0, 0),
            piston_pos,
            slime_pos,
            observer_pos,
            watch_pos,
            BlockPos::new(2, 0, 0),
            BlockPos::new(3, 0, 0),
        ],
    );

    circuit.notify_block_changed(piston_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 3);

    assert_eq!(world.get_block(BlockPos::new(2, 0, 0)).0, blocks.slime);
    assert_eq!(world.get_block(BlockPos::new(2, 0, 1)).0, blocks.observer);
    assert!(stagcrest_protocol::piston_extended(
        world.get_block(piston_pos).1
    ));

    world.set_block(BlockPos::new(-1, 0, 0), blocks.stone, BlockState(0));
    circuit.notify_block_changed(BlockPos::new(-1, 0, 0), &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 3);

    assert!(!stagcrest_protocol::piston_extended(
        world.get_block(piston_pos).1
    ));
    assert_eq!(world.get_block(BlockPos::new(1, 0, 0)).0, blocks.slime);
}

/// Reproduces user flying-machine layout (2x4 grid):
/// ```text
/// O S
/// R S
/// S P
/// S O
/// ```
#[test]
fn flying_machine_drags_sticky_piston_on_push() {
    use stagcrest_protocol::{observer_state, piston_state, Facing, Facing6};

    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    // Grid: x=0,1  z=0..3 (y=0). P at (1,0,1) pushes South (+Z).
    let p_pos = BlockPos::new(1, 0, 1);
    let r_pos = BlockPos::new(0, 0, 2);
    world.set_block(p_pos, blocks.piston, piston_state(false, Facing6::South));
    world.set_block(
        r_pos,
        blocks.sticky_piston,
        piston_state(false, Facing6::North),
    );
    world.set_block(BlockPos::new(0, 0, 0), blocks.slime, BlockState(0));
    world.set_block(BlockPos::new(0, 0, 1), blocks.slime, BlockState(0));
    world.set_block(
        BlockPos::new(1, 0, 0),
        blocks.observer,
        observer_state(false, Facing::South),
    );
    world.set_block(BlockPos::new(1, 0, 2), blocks.slime, BlockState(0));
    world.set_block(BlockPos::new(1, 0, 3), blocks.slime, BlockState(0));
    world.set_block(
        BlockPos::new(0, 0, 3),
        blocks.observer,
        observer_state(false, Facing::North),
    );
    world.set_block(BlockPos::new(1, 0, 4), blocks.stone, BlockState(0)); // stopper
    world.set_block(BlockPos::new(2, 0, 1), blocks.source, BlockState(0)); // power east of P

    let all = [
        BlockPos::new(2, 0, 1),
        BlockPos::new(1, 0, 0),
        BlockPos::new(0, 0, 0),
        BlockPos::new(0, 0, 1),
        BlockPos::new(0, 0, 2),
        BlockPos::new(0, 0, 3),
        BlockPos::new(1, 0, 0),
        p_pos,
        BlockPos::new(1, 0, 2),
        BlockPos::new(1, 0, 3),
        BlockPos::new(1, 0, 4),
        BlockPos::new(0, 0, 5),
        BlockPos::new(1, 0, 5),
    ];
    populate_chunks(&mut world, &all);

    circuit.notify_block_changed(p_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 2);

    // Minecraft semantics: the firing piston P is anchored and extends in place.
    assert_eq!(
        world.get_block(p_pos).0,
        blocks.piston,
        "pusher piston body stays anchored at its cell"
    );
    assert!(
        stagcrest_protocol::piston_extended(world.get_block(p_pos).1),
        "pusher piston should be extended"
    );
    assert_eq!(
        world.get_block(BlockPos::new(1, 0, 2)).0,
        blocks.piston_head,
        "piston head occupies the cell in front of P"
    );

    // The slime block in front of P must be pushed forward, NOT destroyed.
    assert_eq!(
        world.get_block(BlockPos::new(1, 0, 3)).0,
        blocks.slime,
        "front slime is pushed to (1,0,3), not turned to air"
    );
    assert_eq!(
        world.get_block(BlockPos::new(1, 0, 4)).0,
        blocks.slime,
        "second front slime is pushed to (1,0,4)"
    );

    // R is glued to the front slime, so it is dragged along the push direction.
    assert_eq!(
        world.get_block(BlockPos::new(0, 0, 3)).0,
        blocks.sticky_piston,
        "sticky piston R is dragged by the slime it is stuck to"
    );

    // Stickiness does NOT propagate through R (a non-slime block) to the puller
    // slimes behind it, so they stay put during a push (they only move when R
    // retracts). This matches Minecraft.
    assert_eq!(
        world.get_block(BlockPos::new(0, 0, 1)).0,
        blocks.slime,
        "puller slime at (0,0,1) stays during push"
    );
    assert_eq!(
        world.get_block(BlockPos::new(0, 0, 0)).0,
        blocks.slime,
        "puller slime at (0,0,0) stays during push"
    );
    // The observer behind P is only connected through R, so it stays put.
    assert_eq!(
        world.get_block(BlockPos::new(1, 0, 0)).0,
        blocks.observer,
        "observer behind P stays during push"
    );
}

// --- Redstone block-power rule tests ---

#[test]
fn strongly_powered_stone_does_not_power_adjacent_stone() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    // [Source] -> [Stone A] -> [Stone B]
    let source_pos = BlockPos::new(0, 0, 0);
    let stone_a = BlockPos::new(1, 0, 0);
    let stone_b = BlockPos::new(2, 0, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(stone_a, blocks.stone, BlockState(0));
    world.set_block(stone_b, blocks.stone, BlockState(0));
    populate_chunks(&mut world, &[source_pos, stone_a, stone_b]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);

    let bp_a = block_power_at(&circuit, stone_a, &world, &reg);
    assert!(
        bp_a.strong > 0,
        "Stone A should be strongly powered by source"
    );

    let bp_b = block_power_at(&circuit, stone_b, &world, &reg);
    assert_eq!(bp_b.strong, 0, "Stone B should NOT be strongly powered");
    assert_eq!(
        bp_b.weak, 0,
        "Stone B should NOT be weakly powered by Stone A"
    );
}

#[test]
fn dust_weak_powers_connected_adjacent_stone() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    // [Source] -> [Dust] -> [Stone]
    let source_pos = BlockPos::new(0, 0, 0);
    let dust_pos = BlockPos::new(1, 0, 0);
    let stone_pos = BlockPos::new(2, 0, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(dust_pos, blocks.wire, BlockState(0));
    world.set_block(stone_pos, blocks.stone, BlockState(0));
    populate_chunks(&mut world, &[source_pos, dust_pos, stone_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    circuit.notify_block_changed(dust_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);

    assert_eq!(
        circuit.power_at(dust_pos),
        15,
        "Dust should be powered directly by source"
    );

    let bp = block_power_at(&circuit, stone_pos, &world, &reg);
    assert_eq!(
        bp.strong, 0,
        "Stone should NOT receive strong power from dust"
    );
    assert_eq!(bp.weak, 15, "Connected dust should weak-power stone");
}

#[test]
fn dust_weak_powers_block_below() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let source_pos = BlockPos::new(0, 0, 0);
    let dust_pos = BlockPos::new(1, 0, 0);
    let stone_pos = BlockPos::new(1, -1, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(dust_pos, blocks.wire, BlockState(0));
    world.set_block(stone_pos, blocks.stone, BlockState(0));
    populate_chunks(&mut world, &[source_pos, dust_pos, stone_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    circuit.notify_block_changed(dust_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);

    assert!(circuit.power_at(dust_pos) > 0, "Dust should be powered");

    let bp = block_power_at(&circuit, stone_pos, &world, &reg);
    assert_eq!(
        bp.strong, 0,
        "Stone should NOT receive strong power from dust"
    );
    assert_eq!(bp.weak, 15, "Dust should weak-power its support block");
}

#[test]
fn strongly_powered_stone_feeds_adjacent_dust() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    // [Source] -> [Stone] -> [Dust]
    let source_pos = BlockPos::new(0, 0, 0);
    let stone_pos = BlockPos::new(1, 0, 0);
    let dust_pos = BlockPos::new(2, 0, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(stone_pos, blocks.stone, BlockState(0));
    world.set_block(dust_pos, blocks.wire, BlockState(0));
    populate_chunks(&mut world, &[source_pos, stone_pos, dust_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    circuit.notify_block_changed(dust_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 6);

    assert!(
        circuit.power_at(dust_pos) > 0,
        "Dust should receive signal from hard-powered stone"
    );
}

#[test]
fn weakly_powered_stone_does_not_feed_dust() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    // [Source] -> [Dust A] -> [Stone] -> [Dust B]
    let source_pos = BlockPos::new(0, 0, 0);
    let dust_a = BlockPos::new(1, 0, 0);
    let stone_b = BlockPos::new(2, 0, 0);
    let dust_pos = BlockPos::new(3, 0, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(dust_a, blocks.wire, BlockState(0));
    world.set_block(stone_b, blocks.stone, BlockState(0));
    world.set_block(dust_pos, blocks.wire, BlockState(0));
    populate_chunks(&mut world, &[source_pos, dust_a, stone_b, dust_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    circuit.notify_block_changed(dust_a, &mut world, &reg);
    circuit.notify_block_changed(dust_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 6);

    let bp_b = block_power_at(&circuit, stone_b, &world, &reg);
    assert!(bp_b.weak > 0, "Stone should be weakly powered by Dust A");
    assert_eq!(
        bp_b.strong, 0,
        "Stone should not be strongly powered by Dust A"
    );
    assert_eq!(
        circuit.power_at(dust_pos),
        0,
        "Dust should NOT receive signal from weakly-powered stone"
    );
}

#[test]
fn piston_activates_from_weakly_powered_adjacent_block() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    // [Source] -> [Dust] -> [Stone] <- [Piston facing down]
    let source_pos = BlockPos::new(0, 0, 0);
    let dust_pos = BlockPos::new(1, 0, 0);
    let stone_b = BlockPos::new(2, 0, 0);
    let piston_pos = BlockPos::new(2, 1, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(dust_pos, blocks.wire, BlockState(0));
    world.set_block(stone_b, blocks.stone, BlockState(0));
    world.set_block(
        piston_pos,
        blocks.piston,
        stagcrest_protocol::piston_state(false, stagcrest_protocol::Facing6::Up),
    );
    populate_chunks(&mut world, &[source_pos, dust_pos, stone_b, piston_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    circuit.notify_block_changed(dust_pos, &mut world, &reg);
    circuit.notify_block_changed(piston_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 8);

    assert!(
        stagcrest_protocol::piston_extended(world.get_block(piston_pos).1),
        "Piston should extend from weakly-powered adjacent stone"
    );
}

#[test]
fn lit_torch_hard_powers_above_but_not_side_blocks() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let torch_pos = BlockPos::new(0, 0, 0);
    let stone_above = BlockPos::new(0, 1, 0);
    let stone_beside = BlockPos::new(1, 0, 0);

    world.set_block(
        torch_pos,
        blocks.torch,
        torch_state(false, TorchAttachment::Floor),
    );
    world.set_block(stone_above, blocks.stone, BlockState(0));
    world.set_block(stone_beside, blocks.stone, BlockState(0));
    populate_chunks(&mut world, &[torch_pos, stone_above, stone_beside]);

    circuit.notify_block_changed(torch_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 6);

    assert_eq!(circuit.power_at(torch_pos), 15, "Torch should be lit");

    let bp_above = block_power_at(&circuit, stone_above, &world, &reg);
    assert_eq!(
        bp_above.strong, 15,
        "Block above torch should be hard-powered"
    );

    let bp_side = block_power_at(&circuit, stone_beside, &world, &reg);
    assert_eq!(
        bp_side.strong, 0,
        "Block beside torch should NOT be hard-powered"
    );
    assert_eq!(
        bp_side.weak, 0,
        "Block beside torch should NOT be weak-powered"
    );
}

#[test]
fn repeater_reads_weakly_powered_block_behind() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    // [Source] -> [Dust] -> [Stone] -> [Repeater B east]
    let source_pos = BlockPos::new(0, 0, 0);
    let dust_pos = BlockPos::new(1, 0, 0);
    let stone_b = BlockPos::new(2, 0, 0);
    let repeater_pos = BlockPos::new(3, 0, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(dust_pos, blocks.wire, BlockState(0));
    world.set_block(stone_b, blocks.stone, BlockState(0));
    world.set_block(
        repeater_pos,
        blocks.repeater,
        repeater_state(false, Facing::East, 1),
    );
    populate_chunks(&mut world, &[source_pos, dust_pos, stone_b, repeater_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    circuit.notify_block_changed(dust_pos, &mut world, &reg);
    circuit.notify_block_changed(repeater_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 8);

    let bp_b = block_power_at(&circuit, stone_b, &world, &reg);
    assert!(bp_b.weak > 0, "Stone B should be weakly powered");

    assert!(
        circuit.power_at(repeater_pos) > 0,
        "Repeater should read weakly-powered block behind it"
    );
}

#[test]
fn non_redstone_powerable_block_does_not_receive_block_power() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    // [Source] -> [Glass] -> [Stone] -- glass is a Bedrock redstone insulator
    let source_pos = BlockPos::new(0, 0, 0);
    let glass_pos = BlockPos::new(1, 0, 0);
    let stone_pos = BlockPos::new(2, 0, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(glass_pos, blocks.glass, BlockState(0));
    world.set_block(stone_pos, blocks.stone, BlockState(0));
    populate_chunks(&mut world, &[source_pos, glass_pos, stone_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 6);

    let bp_glass = block_power_at(&circuit, glass_pos, &world, &reg);
    assert_eq!(
        bp_glass.strong, 0,
        "Glass should NOT receive power (not redstone-powerable)"
    );
    assert_eq!(
        bp_glass.weak, 0,
        "Glass should NOT receive power (not redstone-powerable)"
    );

    let bp_stone = block_power_at(&circuit, stone_pos, &world, &reg);
    assert_eq!(
        bp_stone.strong, 0,
        "Stone behind glass should NOT receive power"
    );
    assert_eq!(
        bp_stone.weak, 0,
        "Stone behind glass should NOT receive power"
    );
}

#[test]
fn slime_block_conducts_strong_power_on_bedrock() {
    let (reg, blocks) = setup_registry();
    let mut world = World::new(BlockId(0));
    let mut circuit = CircuitWorld::new();

    let source_pos = BlockPos::new(0, 0, 0);
    let slime_pos = BlockPos::new(1, 0, 0);

    world.set_block(source_pos, blocks.source, BlockState(0));
    world.set_block(slime_pos, blocks.slime, BlockState(0));
    populate_chunks(&mut world, &[source_pos, slime_pos]);

    circuit.notify_block_changed(source_pos, &mut world, &reg);
    settle(&mut circuit, &mut world, &reg, 4);

    let bp = block_power_at(&circuit, slime_pos, &world, &reg);
    assert!(
        bp.strong > 0,
        "Bedrock slime blocks conduct strong power from adjacent sources"
    );
}
