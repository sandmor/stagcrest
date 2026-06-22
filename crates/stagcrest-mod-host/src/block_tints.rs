use stagcrest_protocol::{BlockFaceTextures, FaceTexture, TintKind};

pub fn apply_block_face_tints(
    block_id: &str,
    fluid: bool,
    face_textures: &mut BlockFaceTextures,
    registry: &crate::registry::BlockRegistry,
) {
    if fluid {
        apply_fluid_tints(face_textures);
        return;
    }

    if matches!(
        block_id,
        "stagcrest:short_grass"
            | "stagcrest:tall_grass"
            | "stagcrest:dandelion"
            | "stagcrest:poppy"
            | "stagcrest:oak_leaves"
    ) {
        apply_foliage_flat_tint(face_textures);
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

fn apply_foliage_flat_tint(face_textures: &mut BlockFaceTextures) {
    face_textures.top.tint = TintKind::Foliage;
    face_textures.bottom.tint = TintKind::Foliage;
    face_textures.sides.tint = TintKind::Foliage;
}

fn apply_fluid_tints(face_textures: &mut BlockFaceTextures) {
    face_textures.top.tint = TintKind::Water;
    face_textures.bottom.tint = TintKind::Water;
    face_textures.sides.tint = TintKind::Water;
}

pub fn face_texture_for(face_textures: &BlockFaceTextures, normal_y: f32) -> FaceTexture {
    if normal_y > 0.5 {
        face_textures.top
    } else if normal_y < -0.5 {
        face_textures.bottom
    } else {
        face_textures.sides
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{BlockId, TextureId};

    #[test]
    fn fluid_blocks_get_water_tint() {
        let mut registry = crate::registry::BlockRegistry::new();
        let tex =
            registry.register_texture("stagcrest:water_still".into(), 16, 16, vec![0; 16 * 16 * 4]);
        let mut faces = BlockFaceTextures::uniform(tex);
        apply_block_face_tints("stagcrest:water", true, &mut faces, &registry);
        assert_eq!(faces.top.tint, TintKind::Water);
        assert_eq!(faces.sides.tint, TintKind::Water);
        let _ = (BlockId(0), TextureId(0));
    }
}
