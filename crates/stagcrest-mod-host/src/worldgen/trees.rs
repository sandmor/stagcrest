use crate::worldgen::config::TerrainConfig;
use crate::worldgen::occupancy::OccupancyMap;
use crate::worldgen::terrain::ColumnBlocks;
use stagcrest_protocol::{BlockId, BlockPos, BlockState};
use stagcrest_world::World;

pub trait TrunkPlacer {
    fn place_trunk(
        &self,
        base: BlockPos,
        world: &World,
        occupancy: &mut OccupancyMap,
        out: &mut Vec<(BlockPos, BlockId, BlockState)>,
        config: &TerrainConfig,
    );
}

pub struct StraightTrunkPlacer {
    pub log: BlockId,
    pub height: i32,
}

impl TrunkPlacer for StraightTrunkPlacer {
    fn place_trunk(
        &self,
        base: BlockPos,
        world: &World,
        occupancy: &mut OccupancyMap,
        out: &mut Vec<(BlockPos, BlockId, BlockState)>,
        config: &TerrainConfig,
    ) {
        for dy in 0..self.height {
            let pos = BlockPos::new(base.x, base.y + dy, base.z);
            if pos.y > config.world_max_y {
                break;
            }
            if !occupancy.can_place(world, pos) {
                break;
            }
            out.push((pos, self.log, BlockState(0)));
            occupancy.place(pos, self.log);
        }
    }
}

pub struct LeafCanopyPlacer {
    pub leaves: BlockId,
}

impl LeafCanopyPlacer {
    pub fn place_oak_canopy(
        &self,
        trunk_top: BlockPos,
        world: &World,
        occupancy: &mut OccupancyMap,
        out: &mut Vec<(BlockPos, BlockId, BlockState)>,
        config: &TerrainConfig,
    ) {
        if self.leaves == BlockId(0) {
            return;
        }

        let crown_y = trunk_top.y + 1;
        for layer in 0..2i32 {
            let y = crown_y + layer;
            if y > config.world_max_y {
                break;
            }
            let radius = if layer == 0 { 2i32 } else { 1i32 };
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    if layer == 0 && dx.abs() == 2 && dz.abs() == 2 {
                        continue;
                    }
                    if layer == 1 && (dx.abs() + dz.abs()) > 1 {
                        continue;
                    }
                    let pos = BlockPos::new(trunk_top.x + dx, y, trunk_top.z + dz);
                    if pos.x == trunk_top.x && pos.z == trunk_top.z && y <= trunk_top.y {
                        continue;
                    }
                    if !occupancy.can_place(world, pos) {
                        continue;
                    }
                    out.push((pos, self.leaves, BlockState(0)));
                    occupancy.place(pos, self.leaves);
                }
            }
        }
    }
}

pub fn place_oak_tree(
    wx: i32,
    above_y: i32,
    wz: i32,
    height: i32,
    blocks: &ColumnBlocks,
    world: &World,
    occupancy: &mut OccupancyMap,
    out: &mut Vec<(BlockPos, BlockId, BlockState)>,
    config: &TerrainConfig,
) {
    let base = BlockPos::new(wx, above_y, wz);
    if !occupancy.can_place(world, base) {
        return;
    }

    let trunk = StraightTrunkPlacer {
        log: blocks.oak_log,
        height,
    };
    trunk.place_trunk(base, world, occupancy, out, config);

    let placed_height = out
        .iter()
        .filter(|(pos, id, _)| pos.x == wx && pos.z == wz && *id == blocks.oak_log)
        .count() as i32;
    if placed_height == 0 {
        return;
    }

    let trunk_top = BlockPos::new(wx, above_y + placed_height - 1, wz);
    let leaves = LeafCanopyPlacer {
        leaves: blocks.oak_leaves,
    };
    leaves.place_oak_canopy(trunk_top, world, occupancy, out, config);
}
