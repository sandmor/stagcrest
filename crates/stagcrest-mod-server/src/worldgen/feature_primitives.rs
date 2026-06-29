use crate::registry::BlockRegistry;
use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use crate::worldgen::occupancy::OccupancyMap;
use crate::worldgen::seed::WorldSeed;
use stagcrest_mod_sdk::{FeaturePlacement, RiverFeatureSlot};
use stagcrest_protocol::{BlockId, BlockPos, BlockState};

pub fn hash_river_chance(
    seed: WorldSeed,
    wx: i32,
    wz: i32,
    slot: RiverFeatureSlot,
    placement: &FeaturePlacement,
) -> f32 {
    let tag = placement_discriminant(placement);
    let slot_tag = slot as u64;
    let h = seed
        .0
        .wrapping_add(wx as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(wz as u64)
        .wrapping_add(tag.wrapping_mul(0x517C_C1B7_2722_0A95))
        .wrapping_add(slot_tag.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    let mixed = (h ^ (h >> 33)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (mixed as f32 / u64::MAX as f32).clamp(0.0, 0.9999)
}

pub fn hash_biome_chance(seed: WorldSeed, wx: i32, wz: i32, placement: &FeaturePlacement) -> f32 {
    let tag = placement_discriminant(placement);
    let h = seed
        .0
        .wrapping_add(wx as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(wz as u64)
        .wrapping_add(tag.wrapping_mul(0x517C_C1B7_2722_0A95));
    let mixed = (h ^ (h >> 33)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (mixed as f32 / u64::MAX as f32).clamp(0.0, 0.9999)
}

pub fn placement_discriminant(placement: &FeaturePlacement) -> u64 {
    use FeaturePlacement::*;
    match placement {
        Plant { .. } => 1,
        Patch { .. } => 2,
        Tree { .. } => 3,
        Boulder { .. } => 4,
        Column { .. } => 5,
        IceSpike => 6,
        Stalagmite { .. } => 7,
        Stalactite { .. } => 8,
        SurfacePatch { .. } => 9,
        GlowFlora { .. } => 10,
        WaterfallSheet { .. } => 11,
        WaterfallPool { .. } => 12,
    }
}

pub fn place_patch(
    block: BlockId,
    radius: u8,
    density: f32,
    wx: i32,
    wy: i32,
    wz: i32,
    seed: WorldSeed,
    air: BlockId,
    snapshot: &DecorateSnapshot,
    occupancy: &mut OccupancyMap,
    features: &mut Vec<(BlockPos, BlockId, BlockState)>,
) {
    let r = radius as i32;
    for dz in -r..=r {
        for dx in -r..=r {
            if dx.abs() + dz.abs() > r {
                continue;
            }
            let roll = hash_biome_chance(
                seed,
                wx + dx,
                wz + dz,
                &FeaturePlacement::Patch {
                    block: String::new(),
                    radius,
                    density,
                },
            );
            if roll >= density {
                continue;
            }
            let pos = BlockPos::new(wx + dx, wy, wz + dz);
            if !occupancy.can_place(snapshot, pos) {
                continue;
            }
            features.push((pos, block, BlockState(0)));
            occupancy.place(pos, block);
        }
    }
    let _ = air;
}

pub fn place_surface_patch(
    block: BlockId,
    pos: BlockPos,
    snapshot: &DecorateSnapshot,
    occupancy: &mut OccupancyMap,
    features: &mut Vec<(BlockPos, BlockId, BlockState)>,
) {
    if occupancy.can_place(snapshot, pos) {
        features.push((pos, block, BlockState(0)));
        occupancy.place(pos, block);
    }
}

pub fn place_column_up(
    block: BlockId,
    height: u8,
    wx: i32,
    wy: i32,
    wz: i32,
    snapshot: &DecorateSnapshot,
    occupancy: &mut OccupancyMap,
    features: &mut Vec<(BlockPos, BlockId, BlockState)>,
) {
    for dy in 0..height as i32 {
        let pos = BlockPos::new(wx, wy + dy, wz);
        if !occupancy.can_place(snapshot, pos) {
            break;
        }
        features.push((pos, block, BlockState(0)));
        occupancy.place(pos, block);
    }
}

pub fn resolve_block(registry: &BlockRegistry, name: &str, air: BlockId) -> BlockId {
    registry.block_by_name(name).unwrap_or(air)
}
