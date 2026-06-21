use stagcrest_mod_host::BlockRegistry;
use stagcrest_protocol::{BlockPos, BlockState};
use stagcrest_world::World;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Default)]
pub struct RedstoneWorld {
    power: HashMap<BlockPos, u8>,
    scheduled: VecDeque<(BlockPos, u8)>,
}

impl RedstoneWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn power_at(&self, pos: BlockPos) -> u8 {
        self.power.get(&pos).copied().unwrap_or(0)
    }

    pub fn queue_update(&mut self, pos: BlockPos, power: u8) {
        self.scheduled.push_back((pos, power));
    }

    pub fn tick(&mut self, world: &mut World, registry: &BlockRegistry) {
        let mut visited = HashSet::new();
        while let Some((pos, power)) = self.scheduled.pop_front() {
            if !visited.insert(pos) {
                continue;
            }
            self.set_power_at(pos, power, world, registry);
        }
    }

    fn set_power_at(
        &mut self,
        pos: BlockPos,
        power: u8,
        world: &mut World,
        registry: &BlockRegistry,
    ) {
        let (id, state) = world.get_block(pos);
        let Some(def) = registry.block(id) else {
            return;
        };
        let Some(rs) = def.redstone else {
            return;
        };

        let current = self.power.get(&pos).copied().unwrap_or(0);
        let new_power = if rs.always_on {
            rs.emits
        } else if rs.invertible {
            if power > 0 {
                0
            } else {
                rs.emits
            }
        } else if rs.conducts {
            power.saturating_sub(1)
        } else {
            power
        };

        if new_power == current {
            return;
        }

        if new_power == 0 {
            self.power.remove(&pos);
        } else {
            self.power.insert(pos, new_power);
        }

        if rs.conducts || rs.receives {
            let state_val = if new_power > 0 { 1 } else { 0 };
            if state.0 != state_val {
                world.set_block(pos, id, BlockState(state_val));
            }
        }

        for (dx, dy, dz) in neighbors() {
            let npos = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
            let (nid, _) = world.get_block(npos);
            if let Some(ndef) = registry.block(nid) {
                if ndef.redstone.is_some() {
                    let src = self.compute_source_power(pos, world, registry);
                    self.scheduled.push_back((npos, src));
                }
            }
        }
    }

    fn compute_source_power(&self, from: BlockPos, world: &World, registry: &BlockRegistry) -> u8 {
        let mut max_power = 0u8;
        for (dx, dy, dz) in neighbors() {
            let npos = BlockPos::new(from.x + dx, from.y + dy, from.z + dz);
            let p = self.power.get(&npos).copied().unwrap_or(0);
            let (nid, _) = world.get_block(npos);
            if let Some(ndef) = registry.block(nid) {
                if let Some(rs) = ndef.redstone {
                    if rs.conducts {
                        max_power = max_power.max(p);
                    } else if rs.emits > 0 && !rs.invertible {
                        max_power = max_power.max(rs.emits);
                    } else if p > 0 {
                        max_power = max_power.max(p);
                    }
                }
            }
        }
        let (id, _) = world.get_block(from);
        if let Some(def) = registry.block(id) {
            if let Some(rs) = def.redstone {
                if rs.always_on {
                    return rs.emits;
                }
            }
        }
        max_power
    }

    pub fn toggle_block(&mut self, pos: BlockPos, world: &World, registry: &BlockRegistry) {
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
        let on = state.0 > 0;
        let power = if on { 0 } else { rs.emits.max(15) };
        self.queue_update(pos, power);
        let _ = registry;
    }

    pub fn propagate_from(&mut self, pos: BlockPos, power: u8) {
        self.queue_update(pos, power);
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
        let (id, _) = world.get_block(pos);
        if let Some(def) = registry.block(id) {
            if let Some(rs) = def.redstone {
                if rs.always_on {
                    redstone.propagate_from(pos, rs.emits);
                }
            }
        }
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
