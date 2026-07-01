use stagcrest_protocol::{tints, BlockFaceTextures, FaceTexture, TintKind};

pub fn apply_block_face_tints(
    block_id: &str,
    fluid: bool,
    face_textures: &mut BlockFaceTextures,
    registry: &crate::registry::BlockRegistry,
) {
    if tints::apply_biome_tints(block_id, fluid, face_textures) {
        return;
    }

    if block_id != "stagcrest:grass_block" {
        return;
    }

    if let Some(tex) = registry.texture_by_name("stagcrest:grass_top") {
        face_textures.top = FaceTexture {
            texture: tex,
            overlay: None,
            tint: TintKind::Grass,
            overlay_tint: TintKind::None,
        };
    }

    if let Some(tex) = registry.texture_by_name("stagcrest:dirt") {
        face_textures.bottom = FaceTexture::uniform(tex);
    }

    let side_base = registry.texture_by_name("stagcrest:grass_side");
    let side_overlay = registry.texture_by_name("stagcrest:grass_side_overlay");
    if let Some(tex) = side_base {
        face_textures.sides = FaceTexture {
            texture: tex,
            overlay: side_overlay,
            tint: TintKind::None,
            overlay_tint: TintKind::Grass,
        };
    }
}
