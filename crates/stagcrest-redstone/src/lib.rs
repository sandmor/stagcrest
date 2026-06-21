use stagcrest_mod_host::{BlockRegistry, RedstonePowerLookup};
use stagcrest_protocol::{BlockPos, BlockState, set_torch_lit};
use stagcrest_world::World;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Default)]
pub struct RedstoneWorld {
    power: HashMap<BlockPos, u8>,
    scheduled: VecDeque<BlockPos>,
}

impl RedstoneWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn power_at(&self, pos: BlockPos) -> u8 {
        self.power.get(&pos).copied().unwrap_or(0)
    }

    pub fn queue_update(&mut self, pos: BlockPos) {
        self.scheduled.push_back(pos);
    }

    pub fn notify_block_changed(
        &mut self,
        pos: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) {
        self.queue_update(pos);
        for (dx, dy, dz) in neighbors() {
            let npos = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
            let (nid, _) = world.get_block(npos);
            if registry.block(nid).and_then(|d| d.redstone).is_some() {
                self.queue_update(npos);
            }
        }
    }

    pub fn tick(&mut self, world: &mut World, registry: &BlockRegistry) {
        const MAX_STEPS: usize = 4096;
        let mut steps = 0usize;
        while let Some(pos) = self.scheduled.pop_front() {
            if steps >= MAX_STEPS {
                break;
            }
            steps += 1;
            self.set_power_at(pos, world, registry);
        }
    }

    fn evaluate_power_at(&self, pos: BlockPos, world: &World, registry: &BlockRegistry) -> u8 {
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return 0;
        };
        let Some(rs) = def.redstone else {
            return 0;
        };

        if rs.always_on {
            return rs.emits;
        }

        if rs.invertible {
            let side = self.max_neighbor_power(pos, world, registry);
            return if side > 0 { 0 } else { rs.emits };
        }

        if rs.receives && !rs.conducts {
            return if state.0 > 0 { rs.emits.max(15) } else { 0 };
        }

        if rs.conducts {
            return self.max_neighbor_power(pos, world, registry).saturating_sub(1);
        }

        if rs.receives {
            return if state.0 > 0 { rs.emits } else { 0 };
        }

        0
    }

    fn max_neighbor_power(&self, pos: BlockPos, world: &World, registry: &BlockRegistry) -> u8 {
        let mut max_power = 0u8;
        for (dx, dy, dz) in neighbors() {
            let npos = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
            max_power = max_power.max(self.neighbor_output_power(npos, world, registry));
        }
        max_power
    }

    fn neighbor_output_power(
        &self,
        pos: BlockPos,
        world: &World,
        registry: &BlockRegistry,
    ) -> u8 {
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return 0;
        };
        let Some(rs) = def.redstone else {
            return 0;
        };

        if rs.always_on {
            return rs.emits;
        }
        if rs.invertible {
            return self.power_at(pos);
        }
        if rs.conducts {
            return self.power_at(pos);
        }
        if rs.receives && state.0 > 0 {
            return rs.emits;
        }
        self.power_at(pos)
    }

    fn set_power_at(&mut self, pos: BlockPos, world: &mut World, registry: &BlockRegistry) {
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        let Some(rs) = def.redstone else {
            return;
        };

        let current = self.power.get(&pos).copied().unwrap_or(0);
        let new_power = self.evaluate_power_at(pos, world, registry);

        if new_power != current {
            if rs.conducts {
                world.mark_dirty_and_neighbors(pos);
            }

            if new_power == 0 {
                self.power.remove(&pos);
            } else {
                self.power.insert(pos, new_power);
            }

            if rs.conducts || rs.receives {
                let state_val = if new_power > 0 { 1 } else { 0 };
                if def.namespaced_id == "stagcrest:redstone_torch" {
                    let new_state = set_torch_lit(state, state_val != 0);
                    if state.0 != new_state.0 {
                        world.set_block(pos, id, new_state);
                    }
                } else if state.0 != state_val {
                    world.set_block(pos, id, BlockState(state_val));
                }
            }
        }

        for (dx, dy, dz) in neighbors() {
            let npos = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
            let (nid, _) = world.get_block(npos);
            if registry.block(nid).and_then(|d| d.redstone).is_some() {
                self.scheduled.push_back(npos);
            }
        }
    }

    pub fn toggle_block(&mut self, pos: BlockPos, world: &mut World, registry: &BlockRegistry) {
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        let Some(rs) = def.redstone else {
            return;
        };
        if rs.always_on {
            return;
        }
        let on = state.0 & 1 != 0;
        let new_state = if def.namespaced_id == "stagcrest:redstone_torch" {
            set_torch_lit(state, !on)
        } else if on {
            BlockState(0)
        } else {
            BlockState(1)
        };
        world.set_block(pos, id, new_state);
        self.notify_block_changed(pos, world, registry);
    }

    pub fn propagate_from(&mut self, pos: BlockPos, _power: u8) {
        self.queue_update(pos);
    }
}

impl RedstonePowerLookup for RedstoneWorld {
    fn power_at(&self, pos: BlockPos) -> u8 {
        RedstoneWorld::power_at(self, pos)
    }
}

fn neighbors() -> [(i32, i32, i32); 6] {
    [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ]
}

pub fn init_redstone_blocks(redstone: &mut RedstoneWorld, world: &World, registry: &BlockRegistry) {
    for pos in find_redstone_blocks(world, registry) {
        redstone.notify_block_changed(pos, world, registry);
    }
}

fn find_redstone_blocks(world: &World, registry: &BlockRegistry) -> Vec<BlockPos> {
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
                    if registry.block(b.id).and_then(|d| d.redstone).is_some() {
                        out.push(BlockPos::new(base_x + x, base_y + y, base_z + z));
                    }
                }
            }
        }
    }
    out
}
