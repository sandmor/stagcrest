use crate::worldgen::config::TerrainConfig;
use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use crate::worldgen::occupancy::OccupancyMap;
use crate::worldgen::seed::WorldSeed;
use crate::worldgen::terrain::ColumnBlocks;
use stagcrest_protocol::{BlockId, BlockPos, BlockState};

const SALT_TRUNK_HEIGHT: u32 = 0xA1B2_C3D4;
const SALT_BRANCH_COUNT: u32 = 0xB2C3_D4E5;
const SALT_BRANCH_DIR: u32 = 0xC3D4_E5F6;
const SALT_BRANCH_LEN: u32 = 0xD4E5_F607;
const SALT_LEAF_RADIUS: u32 = 0xE5F6_0718;

pub fn hash_u32(seed: WorldSeed, wx: i32, wz: i32, salt: u32) -> u32 {
    let h = seed
        .0
        .wrapping_add(wx as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(wz as u64)
        .wrapping_add(salt as u64)
        .wrapping_mul(0x517C_C1B7_2722_0A95);
    let mixed = (h ^ (h >> 33)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed as u32
}

pub struct HybridTreeGenerator;

impl HybridTreeGenerator {
    pub fn place_oak(
        wx: i32,
        base_y: i32,
        wz: i32,
        blocks: &ColumnBlocks,
        snapshot: &DecorateSnapshot,
        occupancy: &mut OccupancyMap,
        out: &mut Vec<(BlockPos, BlockId, BlockState)>,
        config: &TerrainConfig,
        seed: WorldSeed,
    ) {
        if blocks.oak_log == blocks.air || blocks.oak_leaves == blocks.air {
            return;
        }

        let base = BlockPos::new(wx, base_y, wz);
        if !occupancy.can_place(snapshot, base) {
            return;
        }

        let height = 4 + (hash_u32(seed, wx, wz, SALT_TRUNK_HEIGHT) % 5) as i32;
        let mut trunk_top = base;
        for dy in 0..height {
            let pos = BlockPos::new(wx, base_y + dy, wz);
            if pos.y > config.world_max_y {
                break;
            }
            if !occupancy.can_place(snapshot, pos) {
                break;
            }
            out.push((pos, blocks.oak_log, BlockState(0)));
            occupancy.place(pos, blocks.oak_log);
            trunk_top = pos;
        }

        if trunk_top == base {
            return;
        }

        let trunk_len = trunk_top.y - base_y + 1;
        let branch_count = hash_u32(seed, wx, wz, SALT_BRANCH_COUNT) % 4;
        let mut terminals = vec![trunk_top];

        for bi in 0..branch_count {
            let salt_dir = SALT_BRANCH_DIR.wrapping_add(bi);
            let salt_len = SALT_BRANCH_LEN.wrapping_add(bi);
            let h_dir = hash_u32(seed, wx, wz, salt_dir);
            let (dx, dy, dz) = branch_step_from_hash(h_dir);
            if dx == 0 && dz == 0 {
                continue;
            }

            let branch_len = 2 + (hash_u32(seed, wx, wz, salt_len) % 3) as i32;
            let attach_dy = if trunk_len > 0 {
                (hash_u32(seed, wx, wz, salt_dir) % trunk_len as u32) as i32
            } else {
                0
            };
            let attach_y = (base_y + attach_dy).min(trunk_top.y);
            let start = BlockPos::new(wx, attach_y, wz);
            if occupancy.block_at(snapshot, start) != blocks.oak_log {
                continue;
            }
            let end = place_branch_line(
                start,
                dx,
                dy,
                dz,
                branch_len,
                blocks.oak_log,
                snapshot,
                occupancy,
                out,
                config,
            );
            if end != start {
                terminals.push(end);
            }
        }

        for (ti, tip) in terminals.iter().enumerate() {
            let salt_rad = SALT_LEAF_RADIUS.wrapping_add(ti as u32);
            let h = hash_u32(seed, wx, wz, salt_rad);
            let rx = 2 + (h % 3) as i32;
            let ry = 2 + ((h >> 8) % 3) as i32;
            let rz = 2 + ((h >> 16) % 3) as i32;
            place_leaf_ellipsoid(
                *tip,
                rx,
                ry,
                rz,
                blocks.oak_leaves,
                snapshot,
                occupancy,
                out,
                config,
            );
        }
    }
}

/// One-voxel step direction for branches (each axis in {-1, 0, 1}, Chebyshev distance 1).
fn branch_step_from_hash(h: u32) -> (i32, i32, i32) {
    let mut dx = (h % 3) as i32 - 1;
    let dz = ((h / 3) % 3) as i32 - 1;
    let dy = ((h >> 4) & 0x1) as i32;
    if dx == 0 && dz == 0 {
        dx = if (h >> 5) & 0x1 == 0 { 1 } else { -1 };
    }
    (dx, dy, dz)
}

fn chebyshev_dist(a: BlockPos, b: BlockPos) -> i32 {
    (a.x - b.x)
        .abs()
        .max((a.y - b.y).abs())
        .max((a.z - b.z).abs())
}

fn place_branch_line(
    start: BlockPos,
    dx: i32,
    dy: i32,
    dz: i32,
    steps: i32,
    log: BlockId,
    snapshot: &DecorateSnapshot,
    occupancy: &mut OccupancyMap,
    out: &mut Vec<(BlockPos, BlockId, BlockState)>,
    config: &TerrainConfig,
) -> BlockPos {
    let mut pos = start;
    for _ in 0..steps {
        let next = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
        if next.y > config.world_max_y {
            break;
        }
        if chebyshev_dist(pos, next) != 1 {
            break;
        }
        if !occupancy.can_place(snapshot, next) {
            break;
        }
        out.push((next, log, BlockState(0)));
        occupancy.place(next, log);
        pos = next;
    }
    pos
}

fn place_leaf_ellipsoid(
    center: BlockPos,
    rx: i32,
    ry: i32,
    rz: i32,
    leaves: BlockId,
    snapshot: &DecorateSnapshot,
    occupancy: &mut OccupancyMap,
    out: &mut Vec<(BlockPos, BlockId, BlockState)>,
    config: &TerrainConfig,
) {
    let rx2 = (rx * rx) as i64;
    let ry2 = (ry * ry) as i64;
    let rz2 = (rz * rz) as i64;
    if rx2 == 0 || ry2 == 0 || rz2 == 0 {
        return;
    }

    for dy in -ry..=ry {
        for dz in -rz..=rz {
            for dx in -rx..=rx {
                let dist = (dx as i64 * dx as i64) * ry2 * rz2
                    + (dy as i64 * dy as i64) * rx2 * rz2
                    + (dz as i64 * dz as i64) * rx2 * ry2;
                let threshold = rx2 * ry2 * rz2;
                if dist > threshold {
                    continue;
                }
                let pos = BlockPos::new(center.x + dx, center.y + dy, center.z + dz);
                if pos.y > config.world_max_y {
                    continue;
                }
                if !occupancy.can_place(snapshot, pos) {
                    continue;
                }
                out.push((pos, leaves, BlockState(0)));
                occupancy.place(pos, leaves);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::config::TerrainConfig;
    use crate::worldgen::test_fixtures::test_blocks;

    #[test]
    fn hybrid_tree_is_deterministic() {
        let blocks = test_blocks();
        let config = TerrainConfig::default();
        let seed = WorldSeed(42);
        let snapshot = DecorateSnapshot::empty(blocks.air);
        let surface_y = 64;
        let wx = 8;
        let wz = 8;

        let surface_entries = vec![(
            BlockPos::new(wx, surface_y, wz),
            blocks.grass,
            BlockState(0),
        )];
        let pos = stagcrest_protocol::ChunkPos { x: 0, y: 4, z: 0 };

        let mut run_a = Vec::new();
        let mut occ_a = OccupancyMap::from_surface_entries(&surface_entries, pos, blocks.air);
        HybridTreeGenerator::place_oak(
            wx,
            surface_y + 1,
            wz,
            &blocks,
            &snapshot,
            &mut occ_a,
            &mut run_a,
            &config,
            seed,
        );

        let mut run_b = Vec::new();
        let mut occ_b = OccupancyMap::from_surface_entries(&surface_entries, pos, blocks.air);
        HybridTreeGenerator::place_oak(
            wx,
            surface_y + 1,
            wz,
            &blocks,
            &snapshot,
            &mut occ_b,
            &mut run_b,
            &config,
            seed,
        );

        assert_eq!(run_a, run_b);
        assert!(run_a.iter().any(|(_, id, _)| *id == blocks.oak_log));
        assert!(run_a.iter().any(|(_, id, _)| *id == blocks.oak_leaves));
    }

    #[test]
    fn branch_steps_are_single_voxel() {
        for h in 0..512u32 {
            let (dx, dy, dz) = branch_step_from_hash(h);
            assert!(
                dx.abs() <= 1 && dy.abs() <= 1 && dz.abs() <= 1,
                "step ({dx},{dy},{dz}) exceeds unit voxel"
            );
            assert!(dx != 0 || dy != 0 || dz != 0);
            let cheb = dx.abs().max(dy.abs()).max(dz.abs());
            assert_eq!(cheb, 1, "step ({dx},{dy},{dz}) must be face/edge adjacent");
        }
    }

    #[test]
    fn branch_logs_are_contiguous() {
        let blocks = test_blocks();
        let config = TerrainConfig::default();
        let snapshot = DecorateSnapshot::empty(blocks.air);
        let surface_y = 64;
        let wx = 10;
        let wz = 10;
        let surface_entries = vec![(
            BlockPos::new(wx, surface_y, wz),
            blocks.grass,
            BlockState(0),
        )];
        let chunk = stagcrest_protocol::ChunkPos { x: 0, y: 4, z: 0 };

        for seed_val in 0..64u64 {
            let mut out = Vec::new();
            let mut occ =
                OccupancyMap::from_surface_entries(&surface_entries, chunk, blocks.air);
            HybridTreeGenerator::place_oak(
                wx,
                surface_y + 1,
                wz,
                &blocks,
                &snapshot,
                &mut occ,
                &mut out,
                &config,
                WorldSeed(seed_val),
            );
            let logs: Vec<BlockPos> = out
                .iter()
                .filter(|(_, id, _)| *id == blocks.oak_log)
                .map(|(p, _, _)| *p)
                .collect();
            for log_pos in &logs {
                if log_pos.x == wx && log_pos.z == wz {
                    continue;
                }
                let touches_trunk = logs.iter().any(|t| {
                    t.x == wx
                        && t.z == wz
                        && chebyshev_dist(*log_pos, *t) == 1
                });
                let touches_branch = logs.iter().any(|other| {
                    other != log_pos && chebyshev_dist(*log_pos, *other) == 1
                });
                assert!(
                    touches_trunk || touches_branch,
                    "isolated branch log at {log_pos:?} seed={seed_val}"
                );
            }
        }
    }

    #[test]
    fn branches_attach_to_trunk_not_planned_height() {
        let blocks = test_blocks();
        let config = TerrainConfig::default();
        let seed = WorldSeed(77);
        let snapshot = DecorateSnapshot::empty(blocks.air);
        let surface_y = 64;
        let wx = 4;
        let wz = 4;
        let surface_entries = vec![
            (BlockPos::new(wx, surface_y, wz), blocks.grass, BlockState(0)),
            (
                BlockPos::new(wx, surface_y + 2, wz),
                blocks.stone,
                BlockState(0),
            ),
        ];
        let pos = stagcrest_protocol::ChunkPos { x: 0, y: 4, z: 0 };
        let mut out = Vec::new();
        let mut occ = OccupancyMap::from_surface_entries(&surface_entries, pos, blocks.air);
        HybridTreeGenerator::place_oak(
            wx,
            surface_y + 1,
            wz,
            &blocks,
            &snapshot,
            &mut occ,
            &mut out,
            &config,
            seed,
        );
        for (p, id, _) in &out {
            if *id != blocks.oak_log {
                continue;
            }
            let below = BlockPos::new(p.x, p.y - 1, p.z);
            let supported = out
                .iter()
                .any(|(bp, bid, _)| *bp == below && *bid == blocks.oak_log)
                || below.y == surface_y && below.x == wx && below.z == wz;
            assert!(
                supported || (p.x == wx && p.z == wz && *p == BlockPos::new(wx, surface_y + 1, wz)),
                "log at {:?} has no trunk or ground support",
                p
            );
        }
    }
}
