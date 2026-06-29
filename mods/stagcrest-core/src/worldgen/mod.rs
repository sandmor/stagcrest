mod biomes;
mod features;

use stagcrest_mod_sdk::{
    ContentRegistrar, FeaturePlacement, RegisterCaveConfigRequest, RegisterRiverConfigRequest,
    RegisterRiverFeatureRequest, RiverFeatureSlot,
};

pub fn register_worldgen(reg: &mut impl ContentRegistrar) {
    biomes::register_all_biomes(reg);
    features::register_all_features(reg);

    reg.register_river_config(RegisterRiverConfigRequest {
        width: 8.0,
        bank_blocks: vec![
            "stagcrest:sand".into(),
            "stagcrest:gravel".into(),
            "stagcrest:clay".into(),
        ],
        river_biome_id: "stagcrest:river".into(),
        frozen_river_biome_id: "stagcrest:frozen_river".into(),
        riverbank_biome_id: "stagcrest:riverbank".into(),
        hydrology_mode: stagcrest_mod_sdk::HydrologyMode::DrainageGrid,
        terrace_step: 6,
        terrace_offset: 0,
        drainage_cell_size: 64,
        drainage_relax_passes: 12,
        waterfall_min_drop: 4,
        channel_depth: 10,
        max_channel_carve: 12,
        mouth_sea_margin: 2,
    });

    reg.register_river_feature(RegisterRiverFeatureRequest {
        slot: RiverFeatureSlot::WaterfallLip,
        placement: FeaturePlacement::WaterfallSheet {
            lip_block: Some("stagcrest:gravel".into()),
        },
        chance: 0.6,
    });
    reg.register_river_feature(RegisterRiverFeatureRequest {
        slot: RiverFeatureSlot::WaterfallBase,
        placement: FeaturePlacement::WaterfallPool {
            block: "stagcrest:moss_block".into(),
            radius: 2,
        },
        chance: 0.4,
    });

    reg.register_cave_config(RegisterCaveConfigRequest {
        cheese_threshold: 0.55,
        spaghetti_threshold: 0.02,
        noodle_threshold: 0.015,
        lush_cave_biome_id: "stagcrest:lush_caves".into(),
        dripstone_cave_biome_id: "stagcrest:dripstone_caves".into(),
        deep_dark_biome_id: "stagcrest:deep_dark".into(),
    });
}
