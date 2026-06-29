use crate::content::{
    register_plant_texture_from_pack, register_solid_block, register_texture_from_pack,
};
use crate::fallback::fallback_color;
use stagcrest_mod_sdk::{ContentRegistrar, RegisterBlockRequest, RenderLayer};

fn reg_block(reg: &mut impl ContentRegistrar, id: &str, mc: &str, cross: bool) {
    let full_id = format!("stagcrest:{id}");
    let (r, g, b) = fallback_color(&full_id);
    if cross {
        register_plant_texture_from_pack(reg, &full_id, mc, (r, g, b));
        register_solid_block(
            reg,
            &full_id,
            id,
            &full_id,
            false,
            true,
            false,
            false,
            None,
            None,
            Some("cross"),
        );
    } else {
        register_texture_from_pack(reg, &full_id, mc, (r, g, b));
        register_solid_block(
            reg, &full_id, id, &full_id, true, false, true, true, None, None, None,
        );
    }
}

fn reg_faced_block(
    reg: &mut impl ContentRegistrar,
    id: &str,
    top_mc: &str,
    side_mc: &str,
    bottom_mc: &str,
) {
    let full_id = format!("stagcrest:{id}");
    let top_id = format!("stagcrest:{id}_top");
    let side_id = format!("stagcrest:{id}_side");
    let bottom_id = format!("stagcrest:{id}_bottom");
    let (r, g, b) = fallback_color(&full_id);
    register_texture_from_pack(reg, &top_id, top_mc, (r, g, b));
    register_texture_from_pack(reg, &side_id, side_mc, (r, g, b));
    register_texture_from_pack(reg, &bottom_id, bottom_mc, (r, g, b));
    reg.register_block(RegisterBlockRequest {
        namespaced_id: full_id,
        display_name: id.replace('_', " "),
        opaque: true,
        transparent: false,
        solid: true,
        hardness: 1.0,
        top_texture: top_id,
        bottom_texture: bottom_id,
        sides_texture: side_id,
        placeable: true,
        fluid: false,
        render_layer: None,
        geometry: None,
        circuit: None,
        push_reaction: None,
    });
}

fn reg_faced_textures(
    reg: &mut impl ContentRegistrar,
    id: &str,
    top_mc: &str,
    side_mc: &str,
    bottom_mc: &str,
) {
    let full_id = format!("stagcrest:{id}");
    let (r, g, b) = fallback_color(&full_id);
    register_texture_from_pack(reg, &format!("stagcrest:{id}_top"), top_mc, (r, g, b));
    register_texture_from_pack(reg, &format!("stagcrest:{id}_side"), side_mc, (r, g, b));
    register_texture_from_pack(reg, &format!("stagcrest:{id}_bottom"), bottom_mc, (r, g, b));
}

fn reg_leaves(reg: &mut impl ContentRegistrar, wood: &str) {
    let id = format!("stagcrest:{wood}_leaves");
    let mc = format!("{wood}_leaves");
    let (r, g, b) = fallback_color(&id);
    register_plant_texture_from_pack(reg, &id, &mc, (r, g, b));
    register_solid_block(
        reg,
        &id,
        &format!("{wood} leaves"),
        &id,
        false,
        true,
        false,
        true,
        Some(RenderLayer::Cutout),
        None,
        None,
    );
}

fn reg_log(reg: &mut impl ContentRegistrar, wood: &str) {
    let id = format!("stagcrest:{wood}_log");
    let mc = format!("{wood}_log");
    let top_mc = format!("{wood}_log_top");
    let (r, g, b) = fallback_color(&id);
    register_texture_from_pack(reg, &id, &mc, (r, g, b));
    let top_id = format!("stagcrest:{wood}_log_top");
    register_texture_from_pack(reg, &top_id, &top_mc, (r, g, b));
    reg.register_block(RegisterBlockRequest {
        namespaced_id: id.clone(),
        display_name: format!("{wood} log"),
        opaque: true,
        transparent: false,
        solid: true,
        hardness: 1.0,
        top_texture: top_id,
        bottom_texture: format!("stagcrest:{wood}_log_top"),
        sides_texture: id,
        placeable: true,
        fluid: false,
        render_layer: None,
        geometry: None,
        circuit: None,
        push_reaction: None,
    });
}

pub fn register_extra_textures(reg: &mut impl ContentRegistrar) {
    let terrain = [
        ("coarse_dirt", "coarse_dirt"),
        ("mud", "mud"),
        ("gravel", "gravel"),
        ("clay", "clay"),
        ("red_sand", "red_sand"),
        ("calcite", "calcite"),
        ("tuff", "tuff"),
        ("deepslate", "deepslate"),
        ("dripstone_block", "dripstone_block"),
        ("moss_block", "moss_block"),
        ("snow_block", "snow"),
        ("powder_snow", "powder_snow"),
        ("ice", "ice"),
        ("packed_ice", "packed_ice"),
        ("blue_ice", "blue_ice"),
        ("sculk", "sculk"),
        ("sculk_vein", "sculk_vein"),
        ("terracotta", "terracotta"),
        ("red_terracotta", "red_terracotta"),
        ("orange_terracotta", "orange_terracotta"),
        ("yellow_terracotta", "yellow_terracotta"),
        ("brown_terracotta", "brown_terracotta"),
        ("white_terracotta", "white_terracotta"),
        ("light_gray_terracotta", "light_gray_terracotta"),
    ];
    for (id, mc) in terrain {
        let full = format!("stagcrest:{id}");
        let (r, g, b) = fallback_color(&full);
        register_texture_from_pack(reg, &full, mc, (r, g, b));
    }

    reg_faced_textures(reg, "podzol", "podzol_top", "podzol_side", "dirt");
    reg_faced_textures(reg, "mycelium", "mycelium_top", "mycelium_side", "dirt");
    reg_faced_textures(
        reg,
        "sculk_catalyst",
        "sculk_catalyst_top",
        "sculk_catalyst_side",
        "sculk_catalyst_bottom",
    );
    reg_faced_textures(
        reg,
        "sculk_shrieker",
        "sculk_shrieker_top",
        "sculk_shrieker_side",
        "sculk_shrieker_bottom",
    );

    reg_block(reg, "pointed_dripstone", "pointed_dripstone_up_tip", true);

    for wood in [
        "birch", "spruce", "jungle", "acacia", "dark_oak", "mangrove", "cherry",
    ] {
        let planks = format!("stagcrest:{wood}_planks");
        let mc = format!("{wood}_planks");
        let (r, g, b) = fallback_color(&planks);
        register_texture_from_pack(reg, &planks, &mc, (r, g, b));
        reg_log(reg, wood);
        reg_leaves(reg, wood);
    }

    let plants = [
        ("glow_lichen", "glow_lichen"),
        ("azalea", "azalea_top"),
        ("flowering_azalea", "flowering_azalea_top"),
        ("big_dripleaf", "big_dripleaf_top"),
        ("small_dripleaf", "small_dripleaf_top"),
        ("spore_blossom", "spore_blossom"),
        ("lily_pad", "lily_pad"),
        ("sugar_cane", "sugar_cane"),
        ("bamboo", "bamboo_stalk"),
        ("sweet_berry_bush", "sweet_berry_bush"),
        ("cave_vines", "cave_vines"),
        ("glow_berries", "cave_vines_lit"),
        ("pink_petals", "pink_petals"),
        ("sunflower", "sunflower_top"),
        ("allium", "allium"),
        ("cornflower", "cornflower"),
        ("blue_orchid", "blue_orchid"),
        ("azure_bluet", "azure_bluet"),
        ("red_tulip", "red_tulip"),
        ("orange_tulip", "orange_tulip"),
        ("white_tulip", "white_tulip"),
        ("pink_tulip", "pink_tulip"),
        ("oxeye_daisy", "oxeye_daisy"),
        ("lilac", "lilac_top"),
        ("rose_bush", "rose_bush_top"),
        ("peony", "peony_top"),
        ("fern", "fern"),
        ("large_fern", "large_fern_top"),
    ];
    for (id, mc) in plants {
        let full = format!("stagcrest:{id}");
        let (r, g, b) = fallback_color(&full);
        register_plant_texture_from_pack(reg, &full, mc, (r, g, b));
    }
}

pub fn register_extra_blocks(reg: &mut impl ContentRegistrar) {
    for (id, mc) in [
        ("coarse_dirt", "coarse_dirt"),
        ("mud", "mud"),
        ("gravel", "gravel"),
        ("clay", "clay"),
        ("red_sand", "red_sand"),
        ("calcite", "calcite"),
        ("tuff", "tuff"),
        ("deepslate", "deepslate"),
        ("dripstone_block", "dripstone_block"),
        ("moss_block", "moss_block"),
        ("snow_block", "snow"),
        ("powder_snow", "powder_snow"),
        ("ice", "ice"),
        ("packed_ice", "packed_ice"),
        ("blue_ice", "blue_ice"),
        ("sculk", "sculk"),
        ("terracotta", "terracotta"),
        ("red_terracotta", "red_terracotta"),
        ("orange_terracotta", "orange_terracotta"),
        ("yellow_terracotta", "yellow_terracotta"),
        ("brown_terracotta", "brown_terracotta"),
        ("white_terracotta", "white_terracotta"),
        ("light_gray_terracotta", "light_gray_terracotta"),
    ] {
        reg_block(reg, id, mc, false);
    }

    reg_faced_block(reg, "podzol", "podzol_top", "podzol_side", "dirt");
    reg_faced_block(reg, "mycelium", "mycelium_top", "mycelium_side", "dirt");
    reg_faced_block(
        reg,
        "sculk_catalyst",
        "sculk_catalyst_top",
        "sculk_catalyst_side",
        "sculk_catalyst_bottom",
    );
    reg_faced_block(
        reg,
        "sculk_shrieker",
        "sculk_shrieker_top",
        "sculk_shrieker_side",
        "sculk_shrieker_bottom",
    );

    reg_block(reg, "pointed_dripstone", "pointed_dripstone_up_tip", true);
    reg_block(reg, "sculk_vein", "sculk_vein", false);

    for wood in [
        "birch", "spruce", "jungle", "acacia", "dark_oak", "mangrove", "cherry",
    ] {
        let planks = format!("stagcrest:{wood}_planks");
        register_solid_block(
            reg,
            &planks,
            &format!("{wood} planks"),
            &planks,
            true,
            false,
            true,
            true,
            None,
            None,
            None,
        );
    }

    for (id, mc) in [
        ("glow_lichen", "glow_lichen"),
        ("azalea", "azalea_top"),
        ("flowering_azalea", "flowering_azalea_top"),
        ("lily_pad", "lily_pad"),
        ("pink_petals", "pink_petals"),
        ("sunflower", "sunflower_top"),
        ("allium", "allium"),
        ("cornflower", "cornflower"),
        ("blue_orchid", "blue_orchid"),
        ("azure_bluet", "azure_bluet"),
        ("red_tulip", "red_tulip"),
        ("orange_tulip", "orange_tulip"),
        ("white_tulip", "white_tulip"),
        ("pink_tulip", "pink_tulip"),
        ("oxeye_daisy", "oxeye_daisy"),
        ("fern", "fern"),
    ] {
        reg_block(reg, id, mc, true);
    }

    reg_block(reg, "bamboo", "bamboo_stalk", false);
    reg_block(reg, "sugar_cane", "sugar_cane", true);
    reg_block(reg, "big_dripleaf", "big_dripleaf_top", true);
    reg_block(reg, "small_dripleaf", "small_dripleaf_top", true);
    reg_block(reg, "spore_blossom", "spore_blossom", true);
    reg_block(reg, "cave_vines", "cave_vines", true);
    reg_block(reg, "glow_berries", "cave_vines_lit", true);
    reg_block(reg, "sweet_berry_bush", "sweet_berry_bush", true);
    reg_block(reg, "lilac", "lilac_top", true);
    reg_block(reg, "rose_bush", "rose_bush_top", true);
    reg_block(reg, "peony", "peony_top", true);
    reg_block(reg, "large_fern", "large_fern_top", true);
}
