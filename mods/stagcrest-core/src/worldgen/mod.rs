mod biomes;
mod features;

use stagcrest_mod_sdk::{ContentRegistrar, RegisterCaveConfigRequest, RegisterRiverConfigRequest};

pub fn register_worldgen(reg: &mut impl ContentRegistrar) {
    biomes::register_all_biomes(reg);
    features::register_all_features(reg);

    reg.register_river_config(RegisterRiverConfigRequest {
        width: 4.0,
        valley_depth: 6.0,
        bank_blocks: vec![
            "stagcrest:sand".into(),
            "stagcrest:gravel".into(),
            "stagcrest:clay".into(),
        ],
        river_biome_id: "stagcrest:river".into(),
        frozen_river_biome_id: "stagcrest:frozen_river".into(),
        riverbank_biome_id: "stagcrest:riverbank".into(),
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
