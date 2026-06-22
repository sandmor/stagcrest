use stagcrest_mod_sdk::{
    ContentRegistrar, FeatureKind, RegisterBiomeFeatureRequest, RegisterBiomeRequest,
};

pub fn register_worldgen(reg: &mut impl ContentRegistrar) {
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:plains".into(),
        temperature: 0.8,
        downfall: 0.4,
        surface_top: "stagcrest:grass_block".into(),
        surface_under: "stagcrest:dirt".into(),
        surface_depth: 3,
        underwater_top: Some("stagcrest:sand".into()),
    });
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:forest".into(),
        temperature: 0.7,
        downfall: 0.8,
        surface_top: "stagcrest:grass_block".into(),
        surface_under: "stagcrest:dirt".into(),
        surface_depth: 3,
        underwater_top: Some("stagcrest:sand".into()),
    });
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:desert".into(),
        temperature: 2.0,
        downfall: 0.0,
        surface_top: "stagcrest:sand".into(),
        surface_under: "stagcrest:sand".into(),
        surface_depth: 4,
        underwater_top: Some("stagcrest:sand".into()),
    });
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:taiga".into(),
        temperature: 0.25,
        downfall: 0.8,
        surface_top: "stagcrest:grass_block".into(),
        surface_under: "stagcrest:dirt".into(),
        surface_depth: 3,
        underwater_top: Some("stagcrest:sand".into()),
    });
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:savanna".into(),
        temperature: 1.2,
        downfall: 0.0,
        surface_top: "stagcrest:grass_block".into(),
        surface_under: "stagcrest:dirt".into(),
        surface_depth: 2,
        underwater_top: Some("stagcrest:sand".into()),
    });

    let plains = "stagcrest:plains";
    let forest = "stagcrest:forest";
    let desert = "stagcrest:desert";
    let taiga = "stagcrest:taiga";
    let savanna = "stagcrest:savanna";

    for (biome, kind, chance) in [
        (plains, FeatureKind::ShortGrass, 0.25),
        (plains, FeatureKind::Dandelion, 0.04),
        (plains, FeatureKind::Poppy, 0.03),
        (plains, FeatureKind::OakTree, 0.01),
        (forest, FeatureKind::TallGrass, 0.15),
        (forest, FeatureKind::OakTree, 0.08),
        (desert, FeatureKind::Cactus, 0.02),
        (desert, FeatureKind::DeadBush, 0.05),
        (taiga, FeatureKind::TallGrass, 0.1),
        (taiga, FeatureKind::OakTree, 0.06),
        (savanna, FeatureKind::TallGrass, 0.12),
        (savanna, FeatureKind::OakTree, 0.02),
    ] {
        reg.register_biome_feature(RegisterBiomeFeatureRequest {
            biome_id: biome.into(),
            feature_kind: kind,
            chance,
        });
    }
}
