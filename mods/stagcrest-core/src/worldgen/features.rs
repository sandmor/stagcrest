use stagcrest_mod_sdk::{ContentRegistrar, FeaturePlacement, RegisterFeatureRequest, TreeShape};

macro_rules! feature {
    ($reg:expr, $biome:expr, $chance:expr, $placement:expr) => {
        $reg.register_feature(RegisterFeatureRequest {
            biome_id: format!("stagcrest:{}", $biome),
            placement: $placement,
            chance: $chance,
        });
    };
}

pub fn register_all_features(reg: &mut impl ContentRegistrar) {
    let grass = |tall: bool| FeaturePlacement::Plant {
        block: if tall {
            "stagcrest:tall_grass".into()
        } else {
            "stagcrest:short_grass".into()
        },
        tall,
    };

    // Plains
    feature!(reg, "plains", 0.25, grass(false));
    feature!(
        reg,
        "plains",
        0.04,
        FeaturePlacement::Plant {
            block: "stagcrest:dandelion".into(),
            tall: false
        }
    );
    feature!(
        reg,
        "plains",
        0.03,
        FeaturePlacement::Plant {
            block: "stagcrest:poppy".into(),
            tall: false
        }
    );
    feature!(
        reg,
        "plains",
        0.01,
        FeaturePlacement::Tree {
            trunk: "stagcrest:oak_log".into(),
            leaves: "stagcrest:oak_leaves".into(),
            shape: TreeShape::Oak,
            height: 7,
        }
    );
    feature!(
        reg,
        "sunflower_plains",
        0.02,
        FeaturePlacement::Plant {
            block: "stagcrest:sunflower".into(),
            tall: true
        }
    );

    // Forests
    for biome in [
        "forest",
        "flower_forest",
        "birch_forest",
        "old_growth_birch_forest",
        "dark_forest",
        "taiga",
        "old_growth_pine_taiga",
        "old_growth_spruce_taiga",
        "windswept_forest",
        "grove",
    ] {
        feature!(reg, biome, 0.15, grass(true));
    }
    feature!(
        reg,
        "forest",
        0.08,
        FeaturePlacement::Tree {
            trunk: "stagcrest:oak_log".into(),
            leaves: "stagcrest:oak_leaves".into(),
            shape: TreeShape::Oak,
            height: 7,
        }
    );
    feature!(
        reg,
        "birch_forest",
        0.08,
        FeaturePlacement::Tree {
            trunk: "stagcrest:birch_log".into(),
            leaves: "stagcrest:birch_leaves".into(),
            shape: TreeShape::Birch,
            height: 7,
        }
    );
    feature!(
        reg,
        "dark_forest",
        0.06,
        FeaturePlacement::Tree {
            trunk: "stagcrest:dark_oak_log".into(),
            leaves: "stagcrest:dark_oak_leaves".into(),
            shape: TreeShape::DarkOak,
            height: 6,
        }
    );
    feature!(
        reg,
        "taiga",
        0.06,
        FeaturePlacement::Tree {
            trunk: "stagcrest:spruce_log".into(),
            leaves: "stagcrest:spruce_leaves".into(),
            shape: TreeShape::Spruce,
            height: 9,
        }
    );
    feature!(
        reg,
        "old_growth_pine_taiga",
        0.08,
        FeaturePlacement::Tree {
            trunk: "stagcrest:spruce_log".into(),
            leaves: "stagcrest:spruce_leaves".into(),
            shape: TreeShape::Pine,
            height: 12,
        }
    );
    feature!(
        reg,
        "old_growth_spruce_taiga",
        0.08,
        FeaturePlacement::Tree {
            trunk: "stagcrest:spruce_log".into(),
            leaves: "stagcrest:spruce_leaves".into(),
            shape: TreeShape::Spruce,
            height: 14,
        }
    );
    feature!(
        reg,
        "jungle",
        0.1,
        FeaturePlacement::Tree {
            trunk: "stagcrest:jungle_log".into(),
            leaves: "stagcrest:jungle_leaves".into(),
            shape: TreeShape::Jungle,
            height: 10,
        }
    );
    feature!(
        reg,
        "bamboo_jungle",
        0.08,
        FeaturePlacement::Column {
            block: "stagcrest:bamboo".into(),
            height: 8,
        }
    );
    feature!(
        reg,
        "flower_forest",
        0.06,
        FeaturePlacement::Plant {
            block: "stagcrest:allium".into(),
            tall: false
        }
    );
    feature!(
        reg,
        "flower_forest",
        0.05,
        FeaturePlacement::Plant {
            block: "stagcrest:cornflower".into(),
            tall: false
        }
    );
    feature!(
        reg,
        "pale_garden",
        0.04,
        FeaturePlacement::SurfacePatch {
            block: "stagcrest:pink_petals".into()
        }
    );
    feature!(
        reg,
        "cherry_grove",
        0.06,
        FeaturePlacement::Tree {
            trunk: "stagcrest:cherry_log".into(),
            leaves: "stagcrest:cherry_leaves".into(),
            shape: TreeShape::Cherry,
            height: 7,
        }
    );

    // Savanna
    for biome in ["savanna", "savanna_plateau", "windswept_savanna"] {
        feature!(reg, biome, 0.12, grass(true));
        feature!(
            reg,
            biome,
            0.02,
            FeaturePlacement::Tree {
                trunk: "stagcrest:acacia_log".into(),
                leaves: "stagcrest:acacia_leaves".into(),
                shape: TreeShape::Acacia,
                height: 7,
            }
        );
    }

    // Snow
    feature!(reg, "ice_spikes", 0.03, FeaturePlacement::IceSpike);
    feature!(
        reg,
        "snowy_taiga",
        0.06,
        FeaturePlacement::Tree {
            trunk: "stagcrest:spruce_log".into(),
            leaves: "stagcrest:spruce_leaves".into(),
            shape: TreeShape::Spruce,
            height: 9,
        }
    );

    // Desert
    feature!(
        reg,
        "desert",
        0.02,
        FeaturePlacement::Column {
            block: "stagcrest:cactus".into(),
            height: 3
        }
    );
    feature!(
        reg,
        "desert",
        0.05,
        FeaturePlacement::Plant {
            block: "stagcrest:dead_bush".into(),
            tall: false
        }
    );

    // Swamp
    feature!(
        reg,
        "swamp",
        0.08,
        FeaturePlacement::SurfacePatch {
            block: "stagcrest:lily_pad".into()
        }
    );
    feature!(
        reg,
        "mangrove_swamp",
        0.06,
        FeaturePlacement::Tree {
            trunk: "stagcrest:mangrove_log".into(),
            leaves: "stagcrest:mangrove_leaves".into(),
            shape: TreeShape::Mangrove,
            height: 6,
        }
    );

    // Meadow
    feature!(
        reg,
        "meadow",
        0.1,
        FeaturePlacement::Plant {
            block: "stagcrest:allium".into(),
            tall: false
        }
    );

    // Sky island
    feature!(
        reg,
        "sky_island",
        0.04,
        FeaturePlacement::Tree {
            trunk: "stagcrest:oak_log".into(),
            leaves: "stagcrest:oak_leaves".into(),
            shape: TreeShape::Oak,
            height: 6,
        }
    );

    // Cave biomes
    feature!(
        reg,
        "lush_caves",
        0.08,
        FeaturePlacement::GlowFlora {
            block: "stagcrest:glow_lichen".into()
        }
    );
    feature!(
        reg,
        "lush_caves",
        0.04,
        FeaturePlacement::Plant {
            block: "stagcrest:azalea".into(),
            tall: false
        }
    );
    feature!(
        reg,
        "dripstone_caves",
        0.05,
        FeaturePlacement::Stalagmite {
            block: "stagcrest:pointed_dripstone".into()
        }
    );
    feature!(
        reg,
        "deep_dark",
        0.02,
        FeaturePlacement::GlowFlora {
            block: "stagcrest:sculk_vein".into()
        }
    );
}
