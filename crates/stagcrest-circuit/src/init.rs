use stagcrest_mod_host::BlockRegistry;
use stagcrest_world::World;

use crate::world::CircuitWorld;

pub fn init_circuit_blocks(circuit: &mut CircuitWorld, world: &World, registry: &BlockRegistry) {
    for pos in find_circuit_blocks(world, registry) {
        circuit.queue_update(pos);
    }
}

fn find_circuit_blocks(world: &World, registry: &BlockRegistry) -> Vec<stagcrest_protocol::BlockPos> {
    let mut out = Vec::new();
    for (cpos, chunk) in world.chunks() {
        let base_x = cpos.x * stagcrest_protocol::CHUNK_SIZE;
        let base_y = cpos.y * stagcrest_protocol::CHUNK_SIZE;
        let base_z = cpos.z * stagcrest_protocol::CHUNK_SIZE;
        for y in 0..stagcrest_protocol::CHUNK_SIZE {
            for z in 0..stagcrest_protocol::CHUNK_SIZE {
                for x in 0..stagcrest_protocol::CHUNK_SIZE {
                    let local = stagcrest_protocol::LocalBlockPos {
                        x: x as u8,
                        y: y as u8,
                        z: z as u8,
                    };
                    let b = chunk.get(local);
                    if registry.block(b.id).and_then(|d| d.circuit).is_some() {
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
