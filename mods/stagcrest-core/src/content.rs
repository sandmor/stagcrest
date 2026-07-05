use stagcrest_mod_sdk::{
    behavior_from_circuit, BehaviorKindRequest, CircuitKindRequest, ContentRegistrar,
    NativeBehaviorRequest, RegisterBehaviorRequest, RegisterBlockRequest,
    RegisterTextureRequest, RenderLayer,
};
use stagcrest_protocol::default_map_color;

pub fn register_content(reg: &mut impl ContentRegistrar) {
    register_textures(reg);
    crate::blocks_extra::register_extra_textures(reg);
    register_blocks(reg);
    crate::blocks_extra::register_extra_blocks(reg);
    crate::worldgen::register_worldgen(reg);
    crate::commands::register_commands();
    reg.log("stagcrest-core registered");
}

fn fluid_mask_texture(reg: &mut impl ContentRegistrar, name: &str, alpha: u8) {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..(16 * 16) {
        rgba.extend_from_slice(&[255, 255, 255, alpha]);
    }
    reg.register_texture(RegisterTextureRequest {
        namespaced_id: name.to_string(),
        width: 16,
        height: 16,
        rgba,
    });
}

fn register_fluid_texture_from_pack(reg: &mut impl ContentRegistrar, id: &str, _mc: &str) {
    // Host preloads fluid textures (large animation strips) before mod init.
    // register_texture skips when the namespaced id is already registered.
    fluid_mask_texture(reg, id, 180);
}

fn solid_color_texture(reg: &mut impl ContentRegistrar, name: &str) {
    let [r, g, b] = default_map_color(name);
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..(16 * 16) {
        rgba.extend_from_slice(&[r, g, b, 255]);
    }
    reg.register_texture(RegisterTextureRequest {
        namespaced_id: name.to_string(),
        width: 16,
        height: 16,
        rgba,
    });
}

fn cutout_fallback_texture(reg: &mut impl ContentRegistrar, name: &str) {
    let [r, g, b] = default_map_color(name);
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for z in 0..16u8 {
        for x in 0..16u8 {
            let on_cross = (i16::from(x) - i16::from(z)).unsigned_abs() <= 2
                || (i16::from(x) + i16::from(z) - 15).unsigned_abs() <= 2;
            let alpha = if on_cross { 255 } else { 0 };
            rgba.extend_from_slice(&[r, g, b, alpha]);
        }
    }
    reg.register_texture(RegisterTextureRequest {
        namespaced_id: name.to_string(),
        width: 16,
        height: 16,
        rgba,
    });
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub(crate) fn register_plant_texture_from_pack(
    reg: &mut impl ContentRegistrar,
    id: &str,
    mc_name: &str,
) {
    #[cfg(target_arch = "wasm32")]
    {
        if reg.register_texture_from_pack(id, mc_name) != 0 {
            return;
        }
    }
    cutout_fallback_texture(reg, id);
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub(crate) fn register_texture_from_pack(reg: &mut impl ContentRegistrar, id: &str, mc_name: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if reg.register_texture_from_pack(id, mc_name) != 0 {
            return;
        }
    }
    solid_color_texture(reg, id);
}

/// Register a texture only when the resource pack provides it (no solid-color fallback).
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub(crate) fn register_optional_texture_from_pack(
    reg: &mut impl ContentRegistrar,
    id: &str,
    mc_name: &str,
) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = reg.register_texture_from_pack(id, mc_name);
    }
}

fn register_textures(reg: &mut impl ContentRegistrar) {
    solid_color_texture(reg, "stagcrest:air");
    register_texture_from_pack(reg, "stagcrest:stone", "stone");
    register_texture_from_pack(reg, "stagcrest:dirt", "dirt");
    register_texture_from_pack(reg, "stagcrest:grass_top", "grass_block_top");
    register_texture_from_pack(reg, "stagcrest:grass_side", "grass_block_side");
    register_texture_from_pack(
        reg,
        "stagcrest:grass_side_overlay",
        "grass_block_side_overlay",
    );
    register_texture_from_pack(reg, "stagcrest:cobblestone", "cobblestone");
    register_texture_from_pack(reg, "stagcrest:oak_planks", "oak_planks");
    register_texture_from_pack(reg, "stagcrest:glass", "glass");
    register_fluid_texture_from_pack(reg, "stagcrest:water_still", "water_still");
    register_fluid_texture_from_pack(reg, "stagcrest:water_flow", "water_flow");
    register_texture_from_pack(reg, "stagcrest:bedrock", "bedrock");
    register_texture_from_pack(reg, "stagcrest:redstone_dust_dot", "redstone_dust_dot");
    register_texture_from_pack(reg, "stagcrest:redstone_dust_line", "redstone_dust_line0");
    register_texture_from_pack(reg, "stagcrest:redstone_dust_line1", "redstone_dust_line1");
    register_optional_texture_from_pack(
        reg,
        "stagcrest:redstone_dust_overlay",
        "redstone_dust_overlay",
    );
    register_texture_from_pack(reg, "stagcrest:redstone_torch_off", "redstone_torch_off");
    register_texture_from_pack(reg, "stagcrest:redstone_torch_on", "redstone_torch");
    register_texture_from_pack(reg, "stagcrest:redstone_block", "redstone_block");
    register_texture_from_pack(reg, "stagcrest:redstone_lamp", "redstone_lamp");
    register_optional_texture_from_pack(reg, "stagcrest:redstone_lamp_on", "redstone_lamp_on");
    register_optional_texture_from_pack(reg, "stagcrest:redstone_lamp_on", "lit_redstone_lamp");
    register_texture_from_pack(reg, "stagcrest:lever", "lever");
    register_texture_from_pack(reg, "stagcrest:repeater", "repeater");
    register_texture_from_pack(reg, "stagcrest:repeater_on", "repeater_on");
    register_texture_from_pack(reg, "stagcrest:observer_front", "observer_front");
    register_texture_from_pack(reg, "stagcrest:observer_back", "observer_back");
    register_texture_from_pack(reg, "stagcrest:observer_side", "observer_side");
    register_texture_from_pack(reg, "stagcrest:observer_top", "observer_top");
    register_optional_texture_from_pack(reg, "stagcrest:observer_back_on", "observer_back_on");
    register_texture_from_pack(reg, "stagcrest:piston_top", "piston_top");
    register_texture_from_pack(reg, "stagcrest:piston_top_sticky", "piston_top_sticky");
    register_texture_from_pack(reg, "stagcrest:piston_side", "piston_side");
    register_texture_from_pack(reg, "stagcrest:piston_bottom", "piston_bottom");
    register_texture_from_pack(reg, "stagcrest:piston_inner", "piston_inner");
    register_texture_from_pack(reg, "stagcrest:slime_block", "slime_block");
    register_texture_from_pack(reg, "stagcrest:honey_block_top", "honey_block_top");
    register_texture_from_pack(reg, "stagcrest:honey_block_side", "honey_block_side");
    register_texture_from_pack(reg, "stagcrest:honey_block_bottom", "honey_block_bottom");
    register_texture_from_pack(reg, "stagcrest:smooth_stone", "smooth_stone");
    register_texture_from_pack(reg, "stagcrest:sand", "sand");
    register_texture_from_pack(reg, "stagcrest:iron_ore", "iron_ore");
    register_texture_from_pack(reg, "stagcrest:oak_log", "oak_log");
    register_texture_from_pack(reg, "stagcrest:oak_log_top", "oak_log_top");
    register_plant_texture_from_pack(reg, "stagcrest:oak_leaves", "oak_leaves");
    register_plant_texture_from_pack(reg, "stagcrest:short_grass", "short_grass");
    register_plant_texture_from_pack(reg, "stagcrest:tall_grass_bottom", "tall_grass_bottom");
    register_plant_texture_from_pack(reg, "stagcrest:tall_grass_top", "tall_grass_top");
    register_plant_texture_from_pack(reg, "stagcrest:dandelion", "dandelion");
    register_plant_texture_from_pack(reg, "stagcrest:poppy", "poppy");
    register_texture_from_pack(reg, "stagcrest:cactus_side", "cactus_side");
    register_texture_from_pack(reg, "stagcrest:cactus_top", "cactus_top");
    register_plant_texture_from_pack(reg, "stagcrest:dead_bush", "dead_bush");
}

fn register_layered_cross_plant(
    reg: &mut impl ContentRegistrar,
    id: &str,
    name: &str,
    bottom_texture: &str,
    top_texture: &str,
) {
    reg.register_block(RegisterBlockRequest {
        namespaced_id: id.to_string(),
        display_name: name.to_string(),
        opaque: false,
        transparent: true,
        solid: false,
        hardness: 1.0,
        top_texture: top_texture.to_string(),
        bottom_texture: bottom_texture.to_string(),
        sides_texture: bottom_texture.to_string(),
        placeable: false,
        fluid: false,
        render_layer: None,
        geometry: Some("cross".into()),
        behavior: None,
        callbacks: Default::default(),
        map_color: default_map_color(id),
        light_emission: 0,
        light_attenuation: 0,
    });
}

pub(crate) fn register_solid_block(
    reg: &mut impl ContentRegistrar,
    id: &str,
    name: &str,
    texture: &str,
    opaque: bool,
    transparent: bool,
    solid: bool,
    placeable: bool,
    render_layer: Option<RenderLayer>,
    behavior: Option<RegisterBehaviorRequest>,
    geometry: Option<&str>,
) {
    reg.register_block(RegisterBlockRequest {
        namespaced_id: id.to_string(),
        display_name: name.to_string(),
        opaque,
        transparent,
        solid,
        hardness: 1.0,
        top_texture: texture.to_string(),
        bottom_texture: texture.to_string(),
        sides_texture: texture.to_string(),
        placeable,
        fluid: false,
        render_layer,
        geometry: geometry.map(str::to_string),
        behavior,
        callbacks: Default::default(),
        map_color: default_map_color(id),
        light_emission: 0,
        light_attenuation: 0,
    });
}

fn register_blocks(reg: &mut impl ContentRegistrar) {
    register_solid_block(
        reg,
        "stagcrest:air",
        "Air",
        "stagcrest:air",
        false,
        true,
        false,
        false,
        None,
        None,
        None,
    );
    register_solid_block(
        reg,
        "stagcrest:stone",
        "Stone",
        "stagcrest:stone",
        true,
        false,
        true,
        true,
        None,
        None,
        None,
    );
    register_solid_block(
        reg,
        "stagcrest:dirt",
        "Dirt",
        "stagcrest:dirt",
        true,
        false,
        true,
        true,
        None,
        None,
        None,
    );
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:grass_block".into(),
        display_name: "Grass Block".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 1.0,
        top_texture: "stagcrest:grass_top".into(),
        bottom_texture: "stagcrest:dirt".into(),
        sides_texture: "stagcrest:grass_side".into(),
        placeable: true,
        geometry: None,
        behavior: None,
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:grass_block"),
        light_emission: 0,
        light_attenuation: 0,
    });
    register_solid_block(
        reg,
        "stagcrest:cobblestone",
        "Cobblestone",
        "stagcrest:cobblestone",
        true,
        false,
        true,
        true,
        None,
        None,
        None,
    );
    register_solid_block(
        reg,
        "stagcrest:oak_planks",
        "Oak Planks",
        "stagcrest:oak_planks",
        true,
        false,
        true,
        true,
        None,
        None,
        None,
    );
    register_solid_block(
        reg,
        "stagcrest:glass",
        "Glass",
        "stagcrest:glass",
        false,
        true,
        true,
        true,
        Some(RenderLayer::Blend),
        None,
        None,
    );
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:water".into(),
        display_name: "Water".into(),
        opaque: false,
        transparent: true,
        solid: false,
        fluid: true,
        hardness: 1.0,
        top_texture: "stagcrest:water_still".into(),
        bottom_texture: "stagcrest:water_still".into(),
        sides_texture: "stagcrest:water_still".into(),
        placeable: false,
        geometry: None,
        behavior: None,
        callbacks: Default::default(),
        render_layer: Some(RenderLayer::Blend),
        map_color: default_map_color("stagcrest:water"),
        light_emission: 0,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:bedrock".into(),
        display_name: "Bedrock".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 1.0,
        top_texture: "stagcrest:bedrock".into(),
        bottom_texture: "stagcrest:bedrock".into(),
        sides_texture: "stagcrest:bedrock".into(),
        placeable: false,
        geometry: None,
        behavior: Some(RegisterBehaviorRequest {
            kind: BehaviorKindRequest::Native(NativeBehaviorRequest::Bedrock),
        }),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:bedrock"),
        light_emission: 0,
        light_attenuation: 0,
    });
    register_solid_block(
        reg,
        "stagcrest:redstone_dust",
        "Redstone Dust",
        "stagcrest:redstone_dust_dot",
        false,
        true,
        false,
        true,
        None,
        Some(behavior_from_circuit(CircuitKindRequest::Wire { falloff: 1 })),
        Some("flat"),
    );
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:redstone_torch".into(),
        display_name: "Redstone Torch".into(),
        opaque: false,
        transparent: true,
        solid: false,
        fluid: false,
        hardness: 1.0,
        top_texture: "stagcrest:redstone_torch_off".into(),
        bottom_texture: "stagcrest:redstone_torch_off".into(),
        sides_texture: "stagcrest:redstone_torch_off".into(),
        placeable: true,
        geometry: Some("model:redstone_torch".into()),
        behavior: Some(behavior_from_circuit(CircuitKindRequest::Inverter { output: 15 })),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:redstone_torch"),
        light_emission: 14,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:redstone_block".into(),
        display_name: "Redstone Block".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 1.0,
        top_texture: "stagcrest:redstone_block".into(),
        bottom_texture: "stagcrest:redstone_block".into(),
        sides_texture: "stagcrest:redstone_block".into(),
        placeable: true,
        geometry: None,
        behavior: Some(behavior_from_circuit(CircuitKindRequest::Source { level: 15 })),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:redstone_block"),
        light_emission: 0,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:redstone_lamp".into(),
        display_name: "Redstone Lamp".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 0.3,
        top_texture: "stagcrest:redstone_lamp".into(),
        bottom_texture: "stagcrest:redstone_lamp".into(),
        sides_texture: "stagcrest:redstone_lamp".into(),
        placeable: true,
        geometry: None,
        behavior: Some(behavior_from_circuit(CircuitKindRequest::Lamp)),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:redstone_lamp"),
        light_emission: 15,
        light_attenuation: 0,
    });
    // Lever: cobblestone base (top/bottom slots) + lever handle (sides slot),
    // rendered as a cutout model. Non-opaque so it doesn't cull neighbors, but
    // solid so it can be targeted for breaking/toggling.
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:lever".into(),
        display_name: "Lever".into(),
        opaque: false,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 0.5,
        top_texture: "stagcrest:cobblestone".into(),
        bottom_texture: "stagcrest:cobblestone".into(),
        sides_texture: "stagcrest:lever".into(),
        placeable: true,
        geometry: Some("model:lever".into()),
        behavior: Some(behavior_from_circuit(CircuitKindRequest::Switch { output: 15 })),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:lever"),
        light_emission: 0,
        light_attenuation: 0,
    });
    // Stone button: a small stone box that sinks when pressed.
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:stone_button".into(),
        display_name: "Stone Button".into(),
        opaque: false,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 0.5,
        top_texture: "stagcrest:stone".into(),
        bottom_texture: "stagcrest:stone".into(),
        sides_texture: "stagcrest:stone".into(),
        placeable: true,
        geometry: Some("model:stone_button".into()),
        behavior: Some(behavior_from_circuit(CircuitKindRequest::Switch { output: 15 })),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:stone_button"),
        light_emission: 0,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:repeater".into(),
        display_name: "Repeater".into(),
        opaque: false,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 0.0,
        top_texture: "stagcrest:repeater".into(),
        bottom_texture: "stagcrest:smooth_stone".into(),
        sides_texture: "stagcrest:redstone_torch_off".into(),
        placeable: true,
        geometry: Some("model:repeater".into()),
        behavior: Some(behavior_from_circuit(CircuitKindRequest::Repeater { output: 15 })),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:repeater"),
        light_emission: 0,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:observer".into(),
        display_name: "Observer".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 3.0,
        top_texture: "stagcrest:observer_back".into(),
        bottom_texture: "stagcrest:observer_side".into(),
        sides_texture: "stagcrest:observer_front".into(),
        placeable: true,
        geometry: Some("model:observer".into()),
        behavior: Some(behavior_from_circuit(CircuitKindRequest::Observer { output: 15 })),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:observer"),
        light_emission: 0,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:piston".into(),
        display_name: "Piston".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 0.5,
        top_texture: "stagcrest:piston_top".into(),
        bottom_texture: "stagcrest:piston_bottom".into(),
        sides_texture: "stagcrest:piston_side".into(),
        placeable: true,
        geometry: Some("model:piston".into()),
        behavior: Some(behavior_from_circuit(CircuitKindRequest::Piston { sticky: false })),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:piston"),
        light_emission: 0,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:sticky_piston".into(),
        display_name: "Sticky Piston".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 0.5,
        top_texture: "stagcrest:piston_top_sticky".into(),
        bottom_texture: "stagcrest:piston_bottom".into(),
        sides_texture: "stagcrest:piston_side".into(),
        placeable: true,
        geometry: Some("model:sticky_piston".into()),
        behavior: Some(behavior_from_circuit(CircuitKindRequest::Piston { sticky: true })),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:sticky_piston"),
        light_emission: 0,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:piston_head".into(),
        display_name: "Piston Head".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 0.5,
        top_texture: "stagcrest:piston_top".into(),
        bottom_texture: "stagcrest:piston_bottom".into(),
        sides_texture: "stagcrest:piston_side".into(),
        placeable: false,
        geometry: Some("model:piston_head".into()),
        behavior: Some(RegisterBehaviorRequest {
            kind: BehaviorKindRequest::Native(NativeBehaviorRequest::PistonHead),
        }),
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:piston_head"),
        light_emission: 0,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:slime_block".into(),
        display_name: "Slime Block".into(),
        opaque: true,
        transparent: true,
        solid: true,
        fluid: false,
        hardness: 0.0,
        top_texture: "stagcrest:slime_block".into(),
        bottom_texture: "stagcrest:slime_block".into(),
        sides_texture: "stagcrest:slime_block".into(),
        placeable: true,
        geometry: None,
        behavior: None,
        callbacks: Default::default(),
        render_layer: Some(RenderLayer::Blend),
        map_color: default_map_color("stagcrest:slime_block"),
        light_emission: 0,
        light_attenuation: 0,
    });
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:honey_block".into(),
        display_name: "Honey Block".into(),
        opaque: true,
        transparent: true,
        solid: true,
        fluid: false,
        hardness: 0.0,
        top_texture: "stagcrest:honey_block_top".into(),
        bottom_texture: "stagcrest:honey_block_bottom".into(),
        sides_texture: "stagcrest:honey_block_side".into(),
        placeable: true,
        geometry: None,
        behavior: None,
        callbacks: Default::default(),
        render_layer: Some(RenderLayer::Blend),
        map_color: default_map_color("stagcrest:honey_block"),
        light_emission: 0,
        light_attenuation: 0,
    });
    register_solid_block(
        reg,
        "stagcrest:sand",
        "Sand",
        "stagcrest:sand",
        true,
        false,
        true,
        true,
        None,
        None,
        None,
    );
    register_solid_block(
        reg,
        "stagcrest:iron_ore",
        "Iron Ore",
        "stagcrest:iron_ore",
        true,
        false,
        true,
        true,
        None,
        None,
        None,
    );
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:oak_log".into(),
        display_name: "Oak Log".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 1.0,
        top_texture: "stagcrest:oak_log_top".into(),
        bottom_texture: "stagcrest:oak_log_top".into(),
        sides_texture: "stagcrest:oak_log".into(),
        placeable: true,
        geometry: None,
        behavior: None,
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:oak_log"),
        light_emission: 0,
        light_attenuation: 0,
    });
    register_solid_block(
        reg,
        "stagcrest:oak_leaves",
        "Oak Leaves",
        "stagcrest:oak_leaves",
        false,
        true,
        false,
        true,
        Some(RenderLayer::Cutout),
        None,
        None,
    );
    register_solid_block(
        reg,
        "stagcrest:short_grass",
        "Short Grass",
        "stagcrest:short_grass",
        false,
        true,
        false,
        false,
        None,
        None,
        Some("cross"),
    );
    register_layered_cross_plant(
        reg,
        "stagcrest:tall_grass",
        "Tall Grass",
        "stagcrest:tall_grass_bottom",
        "stagcrest:tall_grass_top",
    );
    register_solid_block(
        reg,
        "stagcrest:dandelion",
        "Dandelion",
        "stagcrest:dandelion",
        false,
        true,
        false,
        false,
        None,
        None,
        Some("cross"),
    );
    register_solid_block(
        reg,
        "stagcrest:poppy",
        "Poppy",
        "stagcrest:poppy",
        false,
        true,
        false,
        false,
        None,
        None,
        Some("cross"),
    );
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:cactus".into(),
        display_name: "Cactus".into(),
        opaque: true,
        transparent: false,
        solid: true,
        fluid: false,
        hardness: 0.4,
        top_texture: "stagcrest:cactus_top".into(),
        bottom_texture: "stagcrest:cactus_top".into(),
        sides_texture: "stagcrest:cactus_side".into(),
        placeable: true,
        geometry: None,
        behavior: None,
        callbacks: Default::default(),
        render_layer: None,
        map_color: default_map_color("stagcrest:cactus"),
        light_emission: 0,
        light_attenuation: 0,
    });
    register_solid_block(
        reg,
        "stagcrest:dead_bush",
        "Dead Bush",
        "stagcrest:dead_bush",
        false,
        true,
        false,
        false,
        None,
        None,
        Some("cross"),
    );
}
