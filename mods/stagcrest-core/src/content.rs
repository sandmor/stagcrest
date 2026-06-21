use stagcrest_mod_sdk::{
    ContentRegistrar, RegisterBlockRequest, RegisterRedstoneRequest, RegisterTextureRequest,
};

pub fn register_content(reg: &mut impl ContentRegistrar) {
    register_textures(reg);
    register_blocks(reg);
    reg.log("stagcrest-core registered");
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
    register_texture_from_pack(reg, "stagcrest:bedrock", "bedrock", (40, 40, 40));
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_dust_off",
        "redstone_dust_dot",
        (160, 0, 0),
    );
    register_texture_from_pack(
        reg,
        "stagcrest:redstone_dust_on",
        "redstone_dust_line0",
        (255, 0, 0),
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
    register_texture_from_pack(reg, "stagcrest:button", "lever", (120, 120, 120));
    register_texture_from_pack(
        reg,
        "stagcrest:repeater",
        "repeater",
        (180, 160, 140),
    );
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
    redstone: Option<RegisterRedstoneRequest>,
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
        redstone,
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
    );
    reg.register_block(RegisterBlockRequest {
        namespaced_id: "stagcrest:grass_block".into(),
        display_name: "Grass Block".into(),
        opaque: true,
        transparent: false,
        solid: true,
        hardness: 1.0,
        top_texture: "stagcrest:grass_top".into(),
        bottom_texture: "stagcrest:dirt".into(),
        sides_texture: "stagcrest:grass_side".into(),
        placeable: true,
        redstone: None,
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
        None,
    );
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
    );
    register_solid_block(
        reg,
        "stagcrest:redstone_dust",
        "Redstone Dust",
        "stagcrest:redstone_dust_off",
        false,
        true,
        false,
        true,
        Some(RegisterRedstoneRequest {
            emits: 0,
            receives: true,
            conducts: true,
            always_on: false,
            invertible: false,
            delay_ticks: 0,
        }),
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
        Some(RegisterRedstoneRequest {
            emits: 15,
            receives: true,
            conducts: false,
            always_on: false,
            invertible: true,
            delay_ticks: 0,
        }),
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
        Some(RegisterRedstoneRequest {
            emits: 15,
            receives: false,
            conducts: false,
            always_on: true,
            invertible: false,
            delay_ticks: 0,
        }),
    );
    register_solid_block(
        reg,
        "stagcrest:lever",
        "Lever",
        "stagcrest:lever",
        true,
        false,
        true,
        true,
        Some(RegisterRedstoneRequest {
            emits: 15,
            receives: false,
            conducts: false,
            always_on: false,
            invertible: false,
            delay_ticks: 0,
        }),
    );
    register_solid_block(
        reg,
        "stagcrest:stone_button",
        "Stone Button",
        "stagcrest:button",
        true,
        false,
        true,
        true,
        Some(RegisterRedstoneRequest {
            emits: 15,
            receives: false,
            conducts: false,
            always_on: false,
            invertible: false,
            delay_ticks: 0,
        }),
    );
    register_solid_block(
        reg,
        "stagcrest:repeater",
        "Repeater",
        "stagcrest:repeater",
        true,
        false,
        true,
        true,
        Some(RegisterRedstoneRequest {
            emits: 15,
            receives: true,
            conducts: false,
            always_on: false,
            invertible: false,
            delay_ticks: 2,
        }),
    );
}
