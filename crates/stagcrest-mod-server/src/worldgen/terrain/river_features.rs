use crate::registry::BlockRegistry;
use crate::worldgen::biome::RiverConfig;
use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use crate::worldgen::feature_primitives::{
    hash_river_chance, place_column_up, place_patch, place_surface_patch, resolve_block,
};
use crate::worldgen::occupancy::OccupancyMap;
use crate::worldgen::seed::WorldSeed;
use crate::worldgen::terrain::density::DensitySampler;
use crate::worldgen::terrain::hydrology::{RiverColumn, RiverSegment};
use crate::worldgen::terrain::ColumnBlocks;
use stagcrest_mod_sdk::{FeaturePlacement, RiverFeatureSlot};
use stagcrest_protocol::{BlockId, BlockPos, BlockState, ChunkPos, CHUNK_SIZE};

pub struct RiverFeaturePlacer<'a> {
    density: &'a DensitySampler<'a>,
    river_config: &'a RiverConfig,
    blocks: ColumnBlocks,
    registry: &'a BlockRegistry,
    seed: WorldSeed,
}

impl<'a> RiverFeaturePlacer<'a> {
    pub fn new(
        density: &'a DensitySampler<'a>,
        river_config: &'a RiverConfig,
        blocks: ColumnBlocks,
        registry: &'a BlockRegistry,
        seed: WorldSeed,
    ) -> Self {
        Self {
            density,
            river_config,
            blocks,
            registry,
            seed,
        }
    }

    pub fn place(
        &self,
        snapshot: &DecorateSnapshot,
        pos: ChunkPos,
        surface_entries: &[(BlockPos, BlockId, BlockState)],
        occupancy: &mut OccupancyMap,
    ) -> Vec<(BlockPos, BlockId, BlockState)> {
        let base_x = pos.x * CHUNK_SIZE;
        let base_z = pos.z * CHUNK_SIZE;
        self.density.hydrology().ensure_region(
            base_x,
            base_z,
            base_x + CHUNK_SIZE - 1,
            base_z + CHUNK_SIZE - 1,
        );

        let mut features = Vec::new();

        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let wx = base_x + lx;
                let wz = base_z + lz;
                let Some(river) = self.density.river_column(wx, wz) else {
                    continue;
                };

                for feat in &self.river_config.features {
                    if !slot_matches_segment(feat.slot, river.segment) {
                        continue;
                    }
                    let roll = hash_river_chance(self.seed, wx, wz, feat.slot, &feat.placement);
                    if roll >= feat.chance {
                        continue;
                    }
                    self.place_at_slot(
                        feat.slot,
                        &feat.placement,
                        wx,
                        wz,
                        river,
                        snapshot,
                        occupancy,
                        &mut features,
                    );
                }
            }
        }

        let _ = surface_entries;
        features
    }

    fn place_at_slot(
        &self,
        slot: RiverFeatureSlot,
        placement: &FeaturePlacement,
        wx: i32,
        wz: i32,
        river: RiverColumn,
        snapshot: &DecorateSnapshot,
        occupancy: &mut OccupancyMap,
        features: &mut Vec<(BlockPos, BlockId, BlockState)>,
    ) {
        match slot {
            RiverFeatureSlot::WaterfallLip => {
                if river.segment != RiverSegment::Waterfall {
                    return;
                }
                let (lx, lz) = (
                    wx.saturating_sub(river.flow.0 as i32),
                    wz.saturating_sub(river.flow.1 as i32),
                );
                let lip_y = river.water_surface_y;
                match placement {
                    FeaturePlacement::WaterfallSheet { lip_block } => {
                        if let Some(name) = lip_block {
                            let id = resolve_block(self.registry, name, self.blocks.air);
                            if id != self.blocks.air {
                                place_surface_patch(
                                    id,
                                    BlockPos::new(lx, lip_y, lz),
                                    snapshot,
                                    occupancy,
                                    features,
                                );
                            }
                        }
                    }
                    FeaturePlacement::SurfacePatch { block } => {
                        let id = resolve_block(self.registry, block, self.blocks.air);
                        place_surface_patch(
                            id,
                            BlockPos::new(lx, lip_y, lz),
                            snapshot,
                            occupancy,
                            features,
                        );
                    }
                    _ => {}
                }
            }
            RiverFeatureSlot::WaterfallBase => {
                if river.segment != RiverSegment::Waterfall {
                    return;
                }
                let bx = wx + river.flow.0 as i32;
                let bz = wz + river.flow.1 as i32;
                let base_y = river.downstream_water_y.max(river.bed_y);
                match placement {
                    FeaturePlacement::WaterfallPool { block, radius } => {
                        let id = resolve_block(self.registry, block, self.blocks.air);
                        place_patch(
                            id,
                            *radius,
                            0.7,
                            bx,
                            base_y,
                            bz,
                            self.seed,
                            self.blocks.air,
                            snapshot,
                            occupancy,
                            features,
                        );
                    }
                    FeaturePlacement::Patch {
                        block,
                        radius,
                        density,
                    } => {
                        let id = resolve_block(self.registry, block, self.blocks.air);
                        place_patch(
                            id,
                            *radius,
                            *density,
                            bx,
                            base_y,
                            bz,
                            self.seed,
                            self.blocks.air,
                            snapshot,
                            occupancy,
                            features,
                        );
                    }
                    FeaturePlacement::SurfacePatch { block } => {
                        let id = resolve_block(self.registry, block, self.blocks.air);
                        place_surface_patch(
                            id,
                            BlockPos::new(bx, base_y, bz),
                            snapshot,
                            occupancy,
                            features,
                        );
                    }
                    _ => {}
                }
            }
            RiverFeatureSlot::PoolSurface => {
                if river.segment != RiverSegment::Pool {
                    return;
                }
                match placement {
                    FeaturePlacement::SurfacePatch { block } => {
                        let id = resolve_block(self.registry, block, self.blocks.air);
                        place_surface_patch(
                            id,
                            BlockPos::new(wx, river.water_surface_y, wz),
                            snapshot,
                            occupancy,
                            features,
                        );
                    }
                    FeaturePlacement::Plant { block, tall: _ } => {
                        let id = resolve_block(self.registry, block, self.blocks.air);
                        place_surface_patch(
                            id,
                            BlockPos::new(wx, river.water_surface_y + 1, wz),
                            snapshot,
                            occupancy,
                            features,
                        );
                    }
                    FeaturePlacement::Column { block, height } => {
                        let id = resolve_block(self.registry, block, self.blocks.air);
                        place_column_up(
                            id,
                            *height,
                            wx,
                            river.water_surface_y + 1,
                            wz,
                            snapshot,
                            occupancy,
                            features,
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}

fn slot_matches_segment(slot: RiverFeatureSlot, segment: RiverSegment) -> bool {
    match slot {
        RiverFeatureSlot::WaterfallLip | RiverFeatureSlot::WaterfallBase => {
            segment == RiverSegment::Waterfall
        }
        RiverFeatureSlot::PoolSurface => segment == RiverSegment::Pool,
    }
}
