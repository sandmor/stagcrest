use crate::{BlockFaceTextures, FaceTexture, TintKind};

/// Applies biome-based tints for leaves, fluids, and specific plants.
/// Does NOT handle grass_block (which needs texture resolution from a registry).
/// Returns true if a tint was applied (caller should skip grass_block handling).
pub fn apply_biome_tints(
    block_id: &str,
    fluid: bool,
    face_textures: &mut BlockFaceTextures,
) -> bool {
    if fluid {
        apply_fluid_tints(face_textures);
        return true;
    }

    if (block_id.ends_with("_leaves")
        && !block_id.ends_with("azalea_leaves")
        && !block_id.ends_with("cherry_leaves")
        && !block_id.starts_with("stagcrest:bamboo_"))
        || matches!(
            block_id,
            "stagcrest:short_grass"
                | "stagcrest:tall_grass"
                | "stagcrest:dandelion"
                | "stagcrest:poppy"
        )
    {
        apply_foliage_flat_tint(face_textures);
        return true;
    }

    false
}

pub fn apply_foliage_flat_tint(face_textures: &mut BlockFaceTextures) {
    face_textures.top.tint = TintKind::Foliage;
    face_textures.bottom.tint = TintKind::Foliage;
    face_textures.sides.tint = TintKind::Foliage;
}

pub fn apply_fluid_tints(face_textures: &mut BlockFaceTextures) {
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
