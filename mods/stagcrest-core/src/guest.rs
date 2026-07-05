//! WASM host import wrappers (wit-bindgen generate lives in lib.rs).

use stagcrest_mod_sdk::{
    BehaviorKindRequest, ContentRegistrar, NativeBehaviorRequest, RegisterBiomeFeatureRequest,
    RegisterBiomeRequest, RegisterBlockRequest, RegisterCaveConfigRequest,
    RegisterCommandRequest, RegisterFeatureRequest, RegisterRiverConfigRequest,
    RegisterRiverFeatureRequest, RegisterTextureRequest,
};

use crate::bindings::stagcrest::plugin::types::{
    BehaviorRef, BiomeDimension, BiomeEnvironment, CallbackFlags as WitCallbackFlags,
    FeatureKind, FeaturePlacement, HydrologyMode, NativeBehavior, NoiseRange,
    RegisterBiomeFeatureRequest as WitRegisterBiomeFeatureRequest,
    RegisterBiomeRequest as WitRegisterBiomeRequest, RegisterBlockRequest as WitRegisterBlockRequest,
    RegisterCaveConfigRequest as WitRegisterCaveConfigRequest,
    RegisterCommandRequest as WitRegisterCommandRequest,
    RegisterFeatureRequest as WitRegisterFeatureRequest,
    RegisterRiverConfigRequest as WitRegisterRiverConfigRequest,
    RegisterRiverFeatureRequest as WitRegisterRiverFeatureRequest,
    RegisterTextureRequest as WitRegisterTextureRequest, RenderLayer as WitRenderLayer,
    RiverFeatureSlot, TreeShape,
};

pub struct HostRegistrar;

impl ContentRegistrar for HostRegistrar {
    fn register_texture(&mut self, req: RegisterTextureRequest) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_texture(&WitRegisterTextureRequest {
            namespaced_id: req.namespaced_id,
            width: req.width,
            height: req.height,
            rgba: req.rgba,
        })
        .map(|_| 0)
        .unwrap_or(1)
    }

    fn register_texture_from_pack(&mut self, namespaced_id: &str, mc_name: &str) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_texture_from_pack(namespaced_id, mc_name)
    }

    fn register_block(&mut self, req: RegisterBlockRequest) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_block(&sdk_block_to_wit(req))
            .map(|_| 0)
            .unwrap_or(1)
    }

    fn register_biome(&mut self, req: RegisterBiomeRequest) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_biome(&sdk_biome_to_wit(req))
            .map(|_| 0)
            .unwrap_or(1)
    }

    fn register_feature(&mut self, req: RegisterFeatureRequest) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_feature(&sdk_feature_to_wit(req))
            .map(|_| 0)
            .unwrap_or(1)
    }

    fn register_river_config(&mut self, req: RegisterRiverConfigRequest) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_river_config(&sdk_river_config_to_wit(req))
            .map(|_| 0)
            .unwrap_or(1)
    }

    fn register_river_feature(&mut self, req: RegisterRiverFeatureRequest) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_river_feature(&sdk_river_feature_to_wit(req))
            .map(|_| 0)
            .unwrap_or(1)
    }

    fn register_cave_config(&mut self, req: RegisterCaveConfigRequest) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_cave_config(&sdk_cave_config_to_wit(req))
            .map(|_| 0)
            .unwrap_or(1)
    }

    fn register_command(&mut self, req: RegisterCommandRequest) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_command(&WitRegisterCommandRequest {
            name: req.name,
            description: req.description,
            usage: req.usage,
        })
    }

    fn register_biome_feature(&mut self, req: RegisterBiomeFeatureRequest) -> i32 {
        crate::bindings::stagcrest::plugin::host::register_biome_feature(&sdk_biome_feature_to_wit(req))
            .map(|_| 0)
            .unwrap_or(1)
    }

    fn log(&self, msg: &str) {
        crate::bindings::stagcrest::plugin::host::log(msg);
    }
}

pub fn command_name() -> Option<String> {
    crate::bindings::stagcrest::plugin::host::command_name()
}

pub fn command_args() -> Option<String> {
    crate::bindings::stagcrest::plugin::host::command_args()
}

pub fn command_reply(text: &str) {
    crate::bindings::stagcrest::plugin::host::command_reply(text);
}

pub fn set_world_time(time: f64) -> i32 {
    crate::bindings::stagcrest::plugin::host::set_world_time(time)
}

pub fn get_world_time() -> f64 {
    crate::bindings::stagcrest::plugin::host::get_world_time()
}

fn sdk_block_to_wit(req: RegisterBlockRequest) -> WitRegisterBlockRequest {
    let behavior = req.behavior.map(|b| match b.kind {
        BehaviorKindRequest::Native(native) => BehaviorRef::Native(native_to_wit(native)),
        BehaviorKindRequest::Wasm => BehaviorRef::Wasm,
    });
    WitRegisterBlockRequest {
        namespaced_id: req.namespaced_id,
        display_name: req.display_name,
        opaque: req.opaque,
        transparent: req.transparent,
        solid: req.solid,
        hardness: req.hardness,
        top_texture: req.top_texture,
        bottom_texture: req.bottom_texture,
        sides_texture: req.sides_texture,
        placeable: req.placeable,
        fluid: req.fluid,
        render_layer: req.render_layer.map(|l| match l {
            stagcrest_mod_sdk::RenderLayer::Opaque => WitRenderLayer::Opaque,
            stagcrest_mod_sdk::RenderLayer::Blend => WitRenderLayer::Blend,
            stagcrest_mod_sdk::RenderLayer::Cutout => WitRenderLayer::Cutout,
        }),
        geometry: req.geometry,
        map_color: (req.map_color[0], req.map_color[1], req.map_color[2]),
        light_emission: req.light_emission,
        light_attenuation: req.light_attenuation,
        behavior,
        callbacks: flags_to_wit(req.callbacks),
    }
}

fn flags_to_wit(flags: stagcrest_protocol::CallbackFlags) -> WitCallbackFlags {
    let mut out = WitCallbackFlags::empty();
    if flags.contains(stagcrest_protocol::CallbackFlags::ON_PLACE) {
        out |= WitCallbackFlags::ON_PLACE;
    }
    if flags.contains(stagcrest_protocol::CallbackFlags::ON_BREAK) {
        out |= WitCallbackFlags::ON_BREAK;
    }
    if flags.contains(stagcrest_protocol::CallbackFlags::ON_USE) {
        out |= WitCallbackFlags::ON_USE;
    }
    if flags.contains(stagcrest_protocol::CallbackFlags::ON_NEIGHBOR_CHANGED) {
        out |= WitCallbackFlags::ON_NEIGHBOR_CHANGED;
    }
    if flags.contains(stagcrest_protocol::CallbackFlags::ON_SCHEDULED_TICK) {
        out |= WitCallbackFlags::ON_SCHEDULED_TICK;
    }
    if flags.contains(stagcrest_protocol::CallbackFlags::ON_RANDOM_TICK) {
        out |= WitCallbackFlags::ON_RANDOM_TICK;
    }
    if flags.contains(stagcrest_protocol::CallbackFlags::STATE_FOR_PLACE) {
        out |= WitCallbackFlags::STATE_FOR_PLACE;
    }
    if flags.contains(stagcrest_protocol::CallbackFlags::DYNAMIC_LIGHT) {
        out |= WitCallbackFlags::DYNAMIC_LIGHT;
    }
    out
}

fn native_to_wit(native: NativeBehaviorRequest) -> NativeBehavior {
    match native {
        NativeBehaviorRequest::RedstoneSource { level } => NativeBehavior::RedstoneSource(level),
        NativeBehaviorRequest::RedstoneWire { falloff } => NativeBehavior::RedstoneWire(falloff),
        NativeBehaviorRequest::RedstoneInverter { output } => {
            NativeBehavior::RedstoneInverter(output)
        }
        NativeBehaviorRequest::RedstoneSwitch { output } => NativeBehavior::RedstoneSwitch(output),
        NativeBehaviorRequest::RedstoneRepeater { output } => {
            NativeBehavior::RedstoneRepeater(output)
        }
        NativeBehaviorRequest::RedstoneObserver { output } => {
            NativeBehavior::RedstoneObserver(output)
        }
        NativeBehaviorRequest::RedstonePiston { sticky } => NativeBehavior::RedstonePiston(sticky),
        NativeBehaviorRequest::RedstoneLamp => NativeBehavior::RedstoneLamp,
        NativeBehaviorRequest::Bedrock => NativeBehavior::Bedrock,
        NativeBehaviorRequest::PistonBody => NativeBehavior::PistonBody,
        NativeBehaviorRequest::PistonHead => NativeBehavior::PistonHead,
    }
}

fn sdk_biome_to_wit(req: RegisterBiomeRequest) -> WitRegisterBiomeRequest {
    WitRegisterBiomeRequest {
        namespaced_id: req.namespaced_id,
        dimension: match req.dimension {
            stagcrest_mod_sdk::BiomeDimension::Surface => BiomeDimension::Surface,
            stagcrest_mod_sdk::BiomeDimension::Underground => BiomeDimension::Underground,
            stagcrest_mod_sdk::BiomeDimension::SkyIsland => BiomeDimension::SkyIsland,
        },
        temperature: NoiseRange {
            min: req.temperature.min,
            max: req.temperature.max,
        },
        humidity: NoiseRange {
            min: req.humidity.min,
            max: req.humidity.max,
        },
        continentalness: NoiseRange {
            min: req.continentalness.min,
            max: req.continentalness.max,
        },
        erosion: NoiseRange {
            min: req.erosion.min,
            max: req.erosion.max,
        },
        depth: NoiseRange {
            min: req.depth.min,
            max: req.depth.max,
        },
        weirdness: NoiseRange {
            min: req.weirdness.min,
            max: req.weirdness.max,
        },
        offset: req.offset,
        surface_top: req.surface_top,
        surface_under: req.surface_under,
        surface_depth: req.surface_depth,
        underwater_top: req.underwater_top,
        underwater_under: req.underwater_under,
        environment: BiomeEnvironment {
            fog_color: tuple3(req.environment.fog_color),
            fog_density: req.environment.fog_density,
            water_color: tuple3(req.environment.water_color),
            water_fog_color: tuple3(req.environment.water_fog_color),
            sky_color: tuple3(req.environment.sky_color),
            grass_color: req.environment.grass_color.map(tuple3),
            foliage_color: req.environment.foliage_color.map(tuple3),
        },
    }
}

fn sdk_feature_to_wit(req: RegisterFeatureRequest) -> WitRegisterFeatureRequest {
    WitRegisterFeatureRequest {
        biome_id: req.biome_id,
        placement: sdk_placement_to_wit(req.placement),
        chance: req.chance,
    }
}

fn sdk_river_config_to_wit(req: RegisterRiverConfigRequest) -> WitRegisterRiverConfigRequest {
    WitRegisterRiverConfigRequest {
        width: req.width,
        bank_blocks: req.bank_blocks,
        river_biome_id: req.river_biome_id,
        frozen_river_biome_id: req.frozen_river_biome_id,
        riverbank_biome_id: req.riverbank_biome_id,
        hydrology_mode: match req.hydrology_mode {
            stagcrest_mod_sdk::HydrologyMode::Terrace => HydrologyMode::Terrace,
            stagcrest_mod_sdk::HydrologyMode::DrainageGrid => HydrologyMode::DrainageGrid,
        },
        terrace_step: req.terrace_step,
        terrace_offset: req.terrace_offset,
        drainage_cell_size: req.drainage_cell_size,
        drainage_relax_passes: req.drainage_relax_passes,
        waterfall_min_drop: req.waterfall_min_drop,
        channel_depth: req.channel_depth,
        max_channel_carve: req.max_channel_carve,
        mouth_sea_margin: req.mouth_sea_margin,
    }
}

fn sdk_river_feature_to_wit(req: RegisterRiverFeatureRequest) -> WitRegisterRiverFeatureRequest {
    WitRegisterRiverFeatureRequest {
        slot: match req.slot {
            stagcrest_mod_sdk::RiverFeatureSlot::WaterfallLip => RiverFeatureSlot::WaterfallLip,
            stagcrest_mod_sdk::RiverFeatureSlot::WaterfallBase => RiverFeatureSlot::WaterfallBase,
            stagcrest_mod_sdk::RiverFeatureSlot::PoolSurface => RiverFeatureSlot::PoolSurface,
        },
        placement: sdk_placement_to_wit(req.placement),
        chance: req.chance,
    }
}

fn sdk_cave_config_to_wit(req: RegisterCaveConfigRequest) -> WitRegisterCaveConfigRequest {
    WitRegisterCaveConfigRequest {
        cheese_threshold: req.cheese_threshold,
        spaghetti_threshold: req.spaghetti_threshold,
        noodle_threshold: req.noodle_threshold,
        lush_cave_biome_id: req.lush_cave_biome_id,
        dripstone_cave_biome_id: req.dripstone_cave_biome_id,
        deep_dark_biome_id: req.deep_dark_biome_id,
    }
}

fn sdk_biome_feature_to_wit(req: RegisterBiomeFeatureRequest) -> WitRegisterBiomeFeatureRequest {
    WitRegisterBiomeFeatureRequest {
        biome_id: req.biome_id,
        feature_kind: match req.feature_kind {
            stagcrest_mod_sdk::FeatureKind::ShortGrass => FeatureKind::ShortGrass,
            stagcrest_mod_sdk::FeatureKind::TallGrass => FeatureKind::TallGrass,
            stagcrest_mod_sdk::FeatureKind::Dandelion => FeatureKind::Dandelion,
            stagcrest_mod_sdk::FeatureKind::Poppy => FeatureKind::Poppy,
            stagcrest_mod_sdk::FeatureKind::Cactus => FeatureKind::Cactus,
            stagcrest_mod_sdk::FeatureKind::DeadBush => FeatureKind::DeadBush,
            stagcrest_mod_sdk::FeatureKind::OakTree => FeatureKind::OakTree,
        },
        chance: req.chance,
    }
}

fn sdk_placement_to_wit(p: stagcrest_mod_sdk::FeaturePlacement) -> FeaturePlacement {
    use stagcrest_mod_sdk::FeaturePlacement as F;
    match p {
        F::Plant { block, tall } => FeaturePlacement::Plant((block, tall)),
        F::Patch { block, radius, density } => FeaturePlacement::Patch((block, radius, density)),
        F::Tree { trunk, leaves, shape, height } => FeaturePlacement::Tree((
            trunk,
            leaves,
            match shape {
                stagcrest_mod_sdk::TreeShape::Oak => TreeShape::Oak,
                stagcrest_mod_sdk::TreeShape::Birch => TreeShape::Birch,
                stagcrest_mod_sdk::TreeShape::Spruce => TreeShape::Spruce,
                stagcrest_mod_sdk::TreeShape::Pine => TreeShape::Pine,
                stagcrest_mod_sdk::TreeShape::Jungle => TreeShape::Jungle,
                stagcrest_mod_sdk::TreeShape::Acacia => TreeShape::Acacia,
                stagcrest_mod_sdk::TreeShape::DarkOak => TreeShape::DarkOak,
                stagcrest_mod_sdk::TreeShape::Mangrove => TreeShape::Mangrove,
                stagcrest_mod_sdk::TreeShape::Cherry => TreeShape::Cherry,
                stagcrest_mod_sdk::TreeShape::Azalea => TreeShape::Azalea,
                stagcrest_mod_sdk::TreeShape::Bamboo => TreeShape::Bamboo,
            },
            height,
        )),
        F::Boulder { block } => FeaturePlacement::Boulder(block),
        F::Column { block, height } => FeaturePlacement::Column((block, height)),
        F::IceSpike => FeaturePlacement::IceSpike,
        F::Stalagmite { block } => FeaturePlacement::Stalagmite(block),
        F::Stalactite { block } => FeaturePlacement::Stalactite(block),
        F::SurfacePatch { block } => FeaturePlacement::SurfacePatch(block),
        F::GlowFlora { block } => FeaturePlacement::GlowFlora(block),
        F::WaterfallSheet { lip_block } => FeaturePlacement::WaterfallSheet(lip_block),
        F::WaterfallPool { block, radius } => FeaturePlacement::WaterfallPool((block, radius)),
    }
}

fn tuple3(t: [u8; 3]) -> (u8, u8, u8) {
    (t[0], t[1], t[2])
}
