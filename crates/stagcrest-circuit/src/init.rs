use crate::registry::BlockRegistry;
use stagcrest_world::World;

use crate::world::CircuitWorld;

pub fn init_circuit_blocks(circuit: &mut CircuitWorld, world: &World, registry: &BlockRegistry) {
    for pos in find_circuit_blocks(world, registry) {
        circuit.queue_update(pos);
    }
}

fn find_circuit_blocks(
    world: &World,
    registry: &BlockRegistry,
) -> Vec<stagcrest_protocol::BlockPos> {
    let mut out = Vec::new();
    for cpos in world.loaded_chunk_positions() {
        if !world.is_generated(cpos) {
            continue;
        }
        let base_x = cpos.x * stagcrest_protocol::CHUNK_SIZE;
        let base_y = cpos.y * stagcrest_protocol::CHUNK_SIZE;
        let base_z = cpos.z * stagcrest_protocol::CHUNK_SIZE;
        for y in 0..stagcrest_protocol::CHUNK_SIZE {
            for z in 0..stagcrest_protocol::CHUNK_SIZE {
                for x in 0..stagcrest_protocol::CHUNK_SIZE {
                    let (id, _state) = world.get_block(stagcrest_protocol::BlockPos::new(
                        base_x + x,
                        base_y + y,
                        base_z + z,
                    ));
                    if registry.block(id).and_then(|d| d.circuit).is_some() {
                        out.push(stagcrest_protocol::BlockPos::new(
                            base_x + x,
                            base_y + y,
                            base_z + z,
                        ));
                    }
                }
            }
        }
    }
    out
}
