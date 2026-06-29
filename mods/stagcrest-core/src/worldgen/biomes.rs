use stagcrest_mod_sdk::{
    BiomeDimension, BiomeEnvironment, ContentRegistrar, NoiseRange, RegisterBiomeRequest,
};

fn env(
    fog: [u8; 3],
    fog_d: f32,
    water: [u8; 3],
    water_fog: [u8; 3],
    sky: [u8; 3],
) -> BiomeEnvironment {
    BiomeEnvironment {
        fog_color: fog,
        fog_density: fog_d,
        water_color: water,
        water_fog_color: water_fog,
        sky_color: sky,
        grass_color: None,
        foliage_color: None,
    }
}

fn surface_biome(
    id: &str,
    temp: f32,
    humid: f32,
    cont: (f32, f32),
    erosion: (f32, f32),
    weird: (f32, f32),
    top: &str,
    under: &str,
    depth: u8,
    underwater: &str,
    environment: BiomeEnvironment,
    offset: f32,
) -> RegisterBiomeRequest {
    RegisterBiomeRequest {
        namespaced_id: format!("stagcrest:{id}"),
        dimension: BiomeDimension::Surface,
        temperature: NoiseRange::new(temp - 0.15, temp + 0.15),
        humidity: NoiseRange::new(humid - 0.15, humid + 0.15),
        continentalness: NoiseRange::new(cont.0, cont.1),
        erosion: NoiseRange::new(erosion.0, erosion.1),
        depth: NoiseRange::new(0.0, 0.4),
        weirdness: NoiseRange::new(weird.0, weird.1),
        offset,
        surface_top: format!("stagcrest:{top}"),
        surface_under: format!("stagcrest:{under}"),
        surface_depth: depth,
        underwater_top: Some(format!("stagcrest:{underwater}")),
        underwater_under: None,
        environment,
    }
}

fn ocean_biome(id: &str, temp: f32, deep: bool) -> RegisterBiomeRequest {
    let cont = if deep { (-1.0, -0.7) } else { (-0.7, -0.45) };
    RegisterBiomeRequest {
        namespaced_id: format!("stagcrest:{id}"),
        dimension: BiomeDimension::Surface,
        temperature: NoiseRange::new(temp - 0.1, temp + 0.1),
        humidity: NoiseRange::new(0.4, 0.9),
        continentalness: NoiseRange::new(cont.0, cont.1),
        erosion: NoiseRange::new(-0.5, 0.5),
        depth: NoiseRange::new(0.5, 1.0),
        weirdness: NoiseRange::new(-0.5, 0.5),
        offset: if deep { 0.1 } else { 0.0 },
        surface_top: "stagcrest:sand".into(),
        surface_under: "stagcrest:sand".into(),
        surface_depth: 3,
        underwater_top: Some("stagcrest:sand".into()),
        underwater_under: Some("stagcrest:gravel".into()),
        environment: env(
            [100, 140, 200],
            0.02,
            [60, 100, 180],
            [20, 50, 100],
            [120, 160, 220],
        ),
    }
}

fn cave_biome(id: &str, depth: (f32, f32), humid: (f32, f32)) -> RegisterBiomeRequest {
    RegisterBiomeRequest {
        namespaced_id: format!("stagcrest:{id}"),
        dimension: BiomeDimension::Underground,
        temperature: NoiseRange::new(0.4, 0.9),
        humidity: NoiseRange::new(humid.0, humid.1),
        continentalness: NoiseRange::new(-0.5, 0.5),
        erosion: NoiseRange::new(-0.5, 0.5),
        depth: NoiseRange::new(depth.0, depth.1),
        weirdness: NoiseRange::new(-0.5, 0.5),
        offset: 0.0,
        surface_top: "stagcrest:stone".into(),
        surface_under: "stagcrest:stone".into(),
        surface_depth: 1,
        underwater_top: None,
        underwater_under: None,
        environment: env(
            [20, 20, 30],
            0.05,
            [40, 60, 100],
            [10, 20, 40],
            [10, 10, 20],
        ),
    }
}

pub fn register_all_biomes(reg: &mut impl ContentRegistrar) {
    let plains_env = env(
        [192, 216, 255],
        0.01,
        [63, 118, 228],
        [5, 44, 82],
        [120, 167, 255],
    );
    let forest_env = env(
        [180, 210, 240],
        0.015,
        [50, 100, 200],
        [5, 40, 70],
        [100, 150, 230],
    );
    let desert_env = env(
        [255, 230, 180],
        0.008,
        [80, 140, 220],
        [30, 60, 100],
        [200, 180, 120],
    );
    let snow_env = env(
        [220, 240, 255],
        0.02,
        [60, 100, 180],
        [20, 50, 90],
        [200, 220, 255],
    );
    let swamp_env = env(
        [100, 140, 120],
        0.04,
        [50, 80, 60],
        [20, 40, 30],
        [120, 150, 130],
    );
    let mountain_env = env(
        [200, 220, 255],
        0.025,
        [70, 120, 200],
        [20, 50, 90],
        [150, 180, 240],
    );
    let badlands_env = env(
        [255, 200, 150],
        0.01,
        [100, 150, 220],
        [40, 70, 110],
        [220, 160, 100],
    );

    // Forests & woodlands
    for (id, temp, humid, top, under) in [
        ("forest", 0.7, 0.8, "grass_block", "dirt"),
        ("flower_forest", 0.7, 0.8, "grass_block", "dirt"),
        ("birch_forest", 0.6, 0.6, "grass_block", "dirt"),
        ("old_growth_birch_forest", 0.6, 0.7, "grass_block", "dirt"),
        ("dark_forest", 0.7, 0.8, "grass_block", "dirt"),
        ("pale_garden", 0.7, 0.75, "grass_block", "dirt"),
        ("taiga", 0.25, 0.8, "grass_block", "dirt"),
        ("old_growth_pine_taiga", 0.25, 0.8, "podzol", "dirt"),
        ("old_growth_spruce_taiga", 0.25, 0.8, "podzol", "dirt"),
        ("jungle", 0.95, 0.9, "grass_block", "dirt"),
        ("sparse_jungle", 0.95, 0.8, "grass_block", "dirt"),
        ("bamboo_jungle", 0.95, 0.9, "grass_block", "dirt"),
    ] {
        reg.register_biome(surface_biome(
            id,
            temp,
            humid,
            (-0.1, 0.4),
            (-0.3, 0.3),
            (-0.3, 0.3),
            top,
            under,
            3,
            "sand",
            forest_env.clone(),
            0.0,
        ));
    }

    // Plains & savannas
    for (id, temp, humid) in [
        ("plains", 0.8, 0.4),
        ("sunflower_plains", 0.8, 0.4),
        ("savanna", 1.2, 0.0),
        ("savanna_plateau", 1.0, 0.0),
        ("windswept_savanna", 1.1, 0.1),
    ] {
        reg.register_biome(surface_biome(
            id,
            temp,
            humid,
            (-0.1, 0.3),
            (0.2, 0.8),
            (-0.3, 0.3),
            "grass_block",
            "dirt",
            3,
            "sand",
            plains_env.clone(),
            0.0,
        ));
    }

    // Snow & ice
    for (id, temp) in [
        ("snowy_plains", 0.0),
        ("ice_spikes", 0.0),
        ("snowy_taiga", 0.05),
    ] {
        reg.register_biome(surface_biome(
            id,
            temp,
            0.5,
            (-0.1, 0.3),
            (-0.2, 0.4),
            (-0.3, 0.3),
            "snow_block",
            "dirt",
            2,
            "sand",
            snow_env.clone(),
            0.0,
        ));
    }

    // Mountains & hills
    for (id, temp, weird) in [
        ("meadow", 0.5, (-0.2, 0.2)),
        ("cherry_grove", 0.5, (-0.1, 0.3)),
        ("grove", 0.3, (-0.2, 0.2)),
        ("snowy_slopes", 0.1, (0.0, 0.4)),
        ("frozen_peaks", 0.0, (0.5, 1.0)),
        ("jagged_peaks", 0.0, (0.6, 1.0)),
        ("stony_peaks", 0.2, (0.4, 0.9)),
        ("windswept_hills", 0.4, (-0.1, 0.4)),
        ("windswept_gravelly_hills", 0.4, (0.0, 0.5)),
        ("windswept_forest", 0.4, (-0.1, 0.3)),
    ] {
        reg.register_biome(surface_biome(
            id,
            temp,
            0.5,
            (0.2, 0.7),
            (-0.8, -0.2),
            weird,
            if id.contains("peaks") || id.contains("slopes") {
                "stone"
            } else {
                "grass_block"
            },
            if id.contains("peaks") {
                "stone"
            } else {
                "dirt"
            },
            2,
            "gravel",
            mountain_env.clone(),
            0.05,
        ));
    }

    // Deserts & badlands
    reg.register_biome(surface_biome(
        "desert",
        2.0,
        0.0,
        (-0.1, 0.4),
        (0.3, 0.9),
        (-0.3, 0.5),
        "sand",
        "sand",
        4,
        "sand",
        desert_env.clone(),
        0.0,
    ));
    for (id, top, under) in [
        ("badlands", "red_sand", "terracotta"),
        ("wooded_badlands", "coarse_dirt", "terracotta"),
        ("eroded_badlands", "red_sand", "terracotta"),
    ] {
        reg.register_biome(surface_biome(
            id,
            2.0,
            0.0,
            (0.1, 0.5),
            (0.4, 0.9),
            (0.2, 0.8),
            top,
            under,
            4,
            "sand",
            badlands_env.clone(),
            0.1,
        ));
    }

    // Swamps & islands
    reg.register_biome(surface_biome(
        "swamp",
        0.8,
        0.9,
        (-0.2, 0.2),
        (0.0, 0.5),
        (-0.3, 0.3),
        "grass_block",
        "dirt",
        3,
        "clay",
        swamp_env.clone(),
        0.0,
    ));
    reg.register_biome(surface_biome(
        "mangrove_swamp",
        0.8,
        0.9,
        (-0.15, 0.15),
        (0.0, 0.4),
        (-0.2, 0.3),
        "mud",
        "dirt",
        3,
        "clay",
        swamp_env.clone(),
        0.05,
    ));
    reg.register_biome(surface_biome(
        "mushroom_fields",
        0.9,
        1.0,
        (-0.3, 0.0),
        (-0.2, 0.3),
        (-0.5, 0.5),
        "mycelium",
        "dirt",
        3,
        "sand",
        env(
            [200, 180, 220],
            0.02,
            [100, 80, 160],
            [40, 30, 80],
            [180, 160, 200],
        ),
        0.2,
    ));

    // Shores & beaches
    for (id, temp, top) in [
        ("beach", 0.8, "sand"),
        ("snowy_beach", 0.05, "sand"),
        ("stony_shore", 0.5, "gravel"),
    ] {
        reg.register_biome(surface_biome(
            id,
            temp,
            0.3,
            (-0.22, -0.12),
            (-0.3, 0.5),
            (-0.3, 0.3),
            top,
            top,
            2,
            "sand",
            plains_env.clone(),
            0.0,
        ));
    }

    // Oceans
    for (id, temp, deep) in [
        ("warm_ocean", 0.5, false),
        ("lukewarm_ocean", 0.5, false),
        ("deep_lukewarm_ocean", 0.5, true),
        ("ocean", 0.5, false),
        ("deep_ocean", 0.5, true),
        ("cold_ocean", 0.2, false),
        ("deep_cold_ocean", 0.2, true),
        ("frozen_ocean", 0.0, false),
        ("deep_frozen_ocean", 0.0, true),
    ] {
        reg.register_biome(ocean_biome(id, temp, deep));
    }

    // Rivers — climate ranges are unreachable so biome_at never picks these;
    // they are assigned only via hydrology in build_biome_grid.
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:river".into(),
        dimension: BiomeDimension::Surface,
        temperature: NoiseRange::new(100.0, 101.0),
        humidity: NoiseRange::new(100.0, 101.0),
        continentalness: NoiseRange::new(100.0, 101.0),
        erosion: NoiseRange::new(100.0, 101.0),
        depth: NoiseRange::new(100.0, 101.0),
        weirdness: NoiseRange::new(100.0, 101.0),
        offset: 0.0,
        surface_top: "stagcrest:sand".into(),
        surface_under: "stagcrest:clay".into(),
        surface_depth: 2,
        underwater_top: Some("stagcrest:sand".into()),
        underwater_under: Some("stagcrest:clay".into()),
        environment: env(
            [180, 210, 240],
            0.015,
            [63, 118, 228],
            [5, 44, 82],
            [120, 167, 255],
        ),
    });
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:frozen_river".into(),
        dimension: BiomeDimension::Surface,
        temperature: NoiseRange::new(100.0, 101.0),
        humidity: NoiseRange::new(100.0, 101.0),
        continentalness: NoiseRange::new(100.0, 101.0),
        erosion: NoiseRange::new(100.0, 101.0),
        depth: NoiseRange::new(100.0, 101.0),
        weirdness: NoiseRange::new(100.0, 101.0),
        offset: 0.0,
        surface_top: "stagcrest:gravel".into(),
        surface_under: "stagcrest:gravel".into(),
        surface_depth: 2,
        underwater_top: Some("stagcrest:gravel".into()),
        underwater_under: None,
        environment: snow_env.clone(),
    });
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:riverbank".into(),
        dimension: BiomeDimension::Surface,
        temperature: NoiseRange::new(100.0, 101.0),
        humidity: NoiseRange::new(100.0, 101.0),
        continentalness: NoiseRange::new(100.0, 101.0),
        erosion: NoiseRange::new(100.0, 101.0),
        depth: NoiseRange::new(100.0, 101.0),
        weirdness: NoiseRange::new(100.0, 101.0),
        offset: 0.0,
        surface_top: "stagcrest:sand".into(),
        surface_under: "stagcrest:dirt".into(),
        surface_depth: 2,
        underwater_top: Some("stagcrest:sand".into()),
        underwater_under: None,
        environment: plains_env.clone(),
    });

    // Underground biomes
    reg.register_biome(cave_biome("lush_caves", (0.3, 0.7), (0.6, 1.0)));
    reg.register_biome(cave_biome("dripstone_caves", (0.4, 0.8), (0.2, 0.6)));
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:deep_dark".into(),
        dimension: BiomeDimension::Underground,
        temperature: NoiseRange::new(0.4, 0.8),
        humidity: NoiseRange::new(0.3, 0.7),
        continentalness: NoiseRange::new(-0.5, 0.5),
        erosion: NoiseRange::new(-0.5, 0.5),
        depth: NoiseRange::new(0.7, 1.0),
        weirdness: NoiseRange::new(-0.5, 0.5),
        offset: 0.0,
        surface_top: "stagcrest:deepslate".into(),
        surface_under: "stagcrest:deepslate".into(),
        surface_depth: 1,
        underwater_top: None,
        underwater_under: None,
        environment: env([5, 5, 10], 0.08, [20, 30, 50], [5, 10, 20], [5, 5, 15]),
    });

    // Sky island
    reg.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:sky_island".into(),
        dimension: BiomeDimension::SkyIsland,
        temperature: NoiseRange::new(0.5, 0.9),
        humidity: NoiseRange::new(0.3, 0.7),
        continentalness: NoiseRange::new(-0.5, 0.5),
        erosion: NoiseRange::new(-0.5, 0.5),
        depth: NoiseRange::new(0.0, 0.5),
        weirdness: NoiseRange::new(-0.5, 0.5),
        offset: 0.0,
        surface_top: "stagcrest:grass_block".into(),
        surface_under: "stagcrest:dirt".into(),
        surface_depth: 3,
        underwater_top: None,
        underwater_under: None,
        environment: env(
            [200, 230, 255],
            0.01,
            [100, 150, 220],
            [40, 70, 120],
            [180, 210, 255],
        ),
    });
}
