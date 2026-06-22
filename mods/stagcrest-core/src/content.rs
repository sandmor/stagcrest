use stagcrest_mod_sdk::{
    CircuitKindRequest, ContentRegistrar, RegisterBlockRequest, RegisterCircuitRequest,
    RegisterTextureRequest, RenderLayer,
};

pub fn register_content(reg: &mut impl ContentRegistrar) {
    register_textures(reg);
    register_blocks(reg);
    crate::worldgen::register_worldgen(reg);
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

fn register_fluid_texture_from_pack(
    reg: &mut impl ContentRegistrar,
    id: &str,
    _mc_name: &str,
) {
    // Host preloads fluid textures (large animation strips) before mod init.
    // register_texture skips when the namespaced id is already registered.
    fluid_mask_texture(reg, id, 180);
}

fn solid_color_texture(reg: &mut impl ContentRegistrar, name: &str, r: u8, g: u8, b: u8) {
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

fn cutout_fallback_texture(reg: &mut impl ContentRegistrar, name: &str, r: u8, g: u8, b: u8) {
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
fn register_plant_texture_from_pack(
    reg: &mut impl ContentRegistrar,
    id: &str,
    mc_name: &str,
    fallback: (u8, u8, u8),
) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some((w, h, rgba)) = stagcrest_mod_sdk::load_texture_from_pack(mc_name) {
            reg.register_texture(RegisterTextureRequest {
                namespaced_id: id.to_string(),
                width: w,
                height: h,
                rgba,
            });
            return;
        }
    }
    let (r, g, b) = fallback;
    cutout_fallback_texture(reg, id, r, g, b);
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
fn register_texture_from_pack(
    reg: &mut impl ContentRegistrar,
    id: &str,
    mc_name: &str,
    fallback: (u8, u8, u8),
) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some((w, h, rgba)) = stagcrest_mod_sdk::load_texture_from_pack(mc_name) {
            reg.register_texture(RegisterTextureRequest {
                namespaced_id: id.to_string(),
                width: w,
                height: h,
                rgba,
            });
            return;
        }
    }
    let (r, g, b) = fallback;
    solid_color_texture(reg, id, r, g, b);
}

fn register_textures(reg: &mut impl ContentRegistrar) {
    solid_color_texture(reg, "stagcrest:air", 0, 0, 0);
    register_texture_from_pack(reg, "stagcrest:stone", "stone", (120, 120, 120));
    register_texture_from_pack(reg, "stagcrest:dirt", "dirt", (134, 96, 67));
    register_texture_from_pack(
        reg,
        "stagcrest:grass_top",
        "grass_block_top",
        (95, 159, 53),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:grass_side",
        "grass_block_side",
        (134, 96, 67),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:grass_side_overlay",
        "grass_block_side_overlay",
        (134, 96, 67),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:cobblestone",
        "cobblestone",
        (100, 100, 100),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:oak_planks",
        "oak_planks",
        (162, 130, 78),
    );
    register_texture_from_pack(reg, "stagcrest:glass", "glass", (200, 230, 255));
    register_fluid_texture_from_pack(reg, "stagcrest:water_still", "water_still");
    register_fluid_texture_from_pack(reg, "stagcrest:water_flow", "water_flow");
    register_texture_from_pack(reg, "stagcrest:bedrock", "bedrock", (40, 40, 40));
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_dust_dot",
        "redstone_dust_dot",
        (140, 0, 0),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_dust_line",
        "redstone_dust_line0",
        (180, 0, 0),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_dust_corner",
        "redstone_dust_corner0",
        (180, 0, 0),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_dust_t",
        "redstone_dust_t0",
        (180, 0, 0),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_dust_cross",
        "redstone_dust_cross0",
        (200, 0, 0),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_torch_off",
        "redstone_torch_off",
        (180, 80, 0),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_torch_on",
        "redstone_torch",
        (255, 120, 0),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_block",
        "redstone_block",
        (200, 0, 0),
    );
    register_texture_from_pack(reg, "stagcrest:lever", "lever", (100, 100, 100));
    register_texture_from_pack(
        reg,
        "stagcrest:repeater",
        "repeater",
        (180, 160, 140),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:repeater_on",
        "repeater_on",
        (200, 180, 160),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:smooth_stone",
        "smooth_stone",
        (160, 160, 160),
    );
    register_texture_from_pack(reg, "stagcrest:sand", "sand", (219, 207, 163));
    register_texture_from_pack(reg, "stagcrest:iron_ore", "iron_ore", (136, 129, 122));
    register_texture_from_pack(reg, "stagcrest:oak_log", "oak_log", (102, 81, 51));
    register_texture_from_pack(reg, "stagcrest:oak_log_top", "oak_log_top", (168, 134, 84));
    register_plant_texture_from_pack(reg, "stagcrest:oak_leaves", "oak_leaves", (60, 120, 40));
    register_plant_texture_from_pack(reg, "stagcrest:short_grass", "short_grass", (95, 159, 53));
    register_plant_texture_from_pack(
        reg,
        "stagcrest:tall_grass_bottom",
        "tall_grass_bottom",
        (95, 159, 53),
    );
    register_plant_texture_from_pack(
        reg,
        "stagcrest:tall_grass_top",
        "tall_grass_top",
        (110, 170, 60),
    );
    register_plant_texture_from_pack(reg, "stagcrest:dandelion", "dandelion", (255, 220, 0));
    register_plant_texture_from_pack(reg, "stagcrest:poppy", "poppy", (200, 40, 40));
    register_texture_from_pack(
        reg,
        "stagcrest:cactus_side",
        "cactus_side",
        (85, 140, 60),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:cactus_top",
        "cactus_top",
        (95, 150, 65),
    );
    register_plant_texture_from_pack(reg, "stagcrest:dead_bush", "dead_bush", (140, 110, 70));
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
        circuit: None,
    });
}

fn register_solid_block(
    reg: &mut impl ContentRegistrar,
    id: &str,
    name: &str,
    texture: &str,
    opaque: bool,
    transparent: bool,
    solid: bool,
    placeable: bool,
    render_layer: Option<RenderLayer>,
    circuit: Option<RegisterCircuitRequest>,
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
        circuit,
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
        circuit: None,
        render_layer: None,
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
        circuit: None,
        render_layer: Some(RenderLayer::Blend),
    });
    register_solid_block(
        reg,
        "stagcrest:bedrock",
        "Bedrock",
        "stagcrest:bedrock",
        true,
        false,
        true,
        false,
        None,
        None,
        None,
    );
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
        Some(RegisterCircuitRequest {
            kind: CircuitKindRequest::Wire { falloff: 1 },
        }),
        Some("flat"),
    );
    register_solid_block(
        reg,
        "stagcrest:redstone_torch",
        "Redstone Torch",
        "stagcrest:redstone_torch_off",
        false,
        true,
        false,
        true,
        None,
        Some(RegisterCircuitRequest {
            kind: CircuitKindRequest::Inverter { output: 15 },
        }),
        Some("model:redstone_torch"),
    );
    register_solid_block(
        reg,
        "stagcrest:redstone_block",
        "Redstone Block",
        "stagcrest:redstone_block",
        true,
        false,
        true,
        true,
        None,
        Some(RegisterCircuitRequest {
            kind: CircuitKindRequest::Source { level: 15 },
        }),
        None,
    );
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
        circuit: Some(RegisterCircuitRequest {
            kind: CircuitKindRequest::Switch { output: 15 },
        }),
        render_layer: None,
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
        circuit: Some(RegisterCircuitRequest {
            kind: CircuitKindRequest::Switch { output: 15 },
        }),
        render_layer: None,
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
        circuit: Some(RegisterCircuitRequest {
            kind: CircuitKindRequest::Repeater { output: 15 },
        }),
        render_layer: None,
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
        circuit: None,
        render_layer: None,
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
        circuit: None,
        render_layer: None,
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
