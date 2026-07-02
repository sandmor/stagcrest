use stagcrest_net::{BlockUpdate, GameMessage, ServerMessage};
use stagcrest_protocol::{BlockId, BlockPos, BlockState};
use stagcrest_server::{streaming_lru_capacity, ClientId, ClientRegistry, TerrainStreamState};

fn setup_client(registry: &mut ClientRegistry, cx: i32, cz: i32) -> ClientId {
    let id = registry.register_inprocess();
    let client = registry.get_mut(id).unwrap();
    client.handshake_complete = true;
    client.stream = TerrainStreamState {
        center_x: cx,
        center_y: 0,
        center_z: cz,
        valid: true,
    };
    id
}

#[test]
fn streaming_lru_scales_with_client_count() {
    let single = streaming_lru_capacity(8, 4, 1);
    let dual = streaming_lru_capacity(8, 4, 2);
    assert!(dual > single);
    assert_eq!(dual, single * 2);
}

#[test]
fn block_update_reaches_clients_in_same_area() {
    let mut registry = ClientRegistry::new(16);
    let a = setup_client(&mut registry, 0, 0);
    let b = setup_client(&mut registry, 0, 0);

    let update = BlockUpdate {
        pos: BlockPos::new(8, 64, 8),
        id: BlockId(1),
        state: BlockState(0),
    };
    registry.fanout_block_update(update, 8, 4);

    for id in [a, b] {
        let client = registry.get_mut(id).unwrap();
        let msgs = client.take_priority();
        assert_eq!(msgs.len(), 1);
        assert!(matches!(
            msgs[0],
            GameMessage::Server(ServerMessage::BlockUpdate(_))
        ));
    }
}

#[test]
fn block_update_skips_far_client() {
    let mut registry = ClientRegistry::new(16);
    let near_id = setup_client(&mut registry, 0, 0);
    let far_id = setup_client(&mut registry, 200, 200);

    let update = BlockUpdate {
        pos: BlockPos::new(8, 64, 8),
        id: BlockId(1),
        state: BlockState(0),
    };
    registry.fanout_block_update(update, 8, 4);

    assert_eq!(registry.get_mut(near_id).unwrap().take_priority().len(), 1);
    assert!(registry.get_mut(far_id).unwrap().take_priority().is_empty());
}

#[test]
fn circuit_batch_filtered_by_interest() {
    let mut registry = ClientRegistry::new(16);
    let near_id = setup_client(&mut registry, 0, 0);
    let far_id = setup_client(&mut registry, 200, 200);

    let near_pos = BlockPos::new(8, 64, 8);
    let far_pos = BlockPos::new(3200, 64, 3200);
    registry.fanout_circuit_batch(
        stagcrest_net::CircuitPowerBatch {
            updates: vec![(near_pos, 15), (far_pos, 7)],
        },
        8,
        4,
    );

    let near_msgs = registry.get_mut(near_id).unwrap().take_priority();
    assert_eq!(near_msgs.len(), 1);
    if let GameMessage::Server(ServerMessage::CircuitPowerBatch(batch)) = &near_msgs[0] {
        assert_eq!(batch.updates.len(), 1);
        assert_eq!(batch.updates[0].0, near_pos);
    } else {
        panic!("expected circuit batch");
    }

    let far_msgs = registry.get_mut(far_id).unwrap().take_priority();
    assert_eq!(far_msgs.len(), 1);
    if let GameMessage::Server(ServerMessage::CircuitPowerBatch(batch)) = &far_msgs[0] {
        assert_eq!(batch.updates.len(), 1);
        assert_eq!(batch.updates[0].0, far_pos);
    } else {
        panic!("expected circuit batch");
    }
}

#[test]
fn disconnect_removes_client_without_affecting_other() {
    let mut registry = ClientRegistry::new(16);
    let a = setup_client(&mut registry, 0, 0);
    let b = setup_client(&mut registry, 10, 10);
    assert_eq!(registry.len(), 2);

    registry.remove(a);
    assert_eq!(registry.len(), 1);
    assert!(registry.get(b).is_some());
}

#[test]
fn registry_respects_max_clients_capacity() {
    let mut registry = ClientRegistry::new(2);
    setup_client(&mut registry, 0, 0);
    setup_client(&mut registry, 1, 1);
    assert!(!registry.has_capacity());
}
