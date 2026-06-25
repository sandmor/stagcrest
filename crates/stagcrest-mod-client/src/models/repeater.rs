//! Redstone repeater geometry from vanilla minecraft models.
//! Canonical model arrow points toward -Z; whole-model Y rotation maps facing.

use stagcrest_protocol::{
    repeater_facing_yaw, BlockModel, Facing, ModelElement, ModelFace, ModelRenderLayer,
    ModelTexture,
};

const S: f32 = 1.0 / 16.0;

fn coord(p: [f32; 3]) -> [f32; 3] {
    [p[0] * S, p[1] * S, p[2] * S]
}

const UP: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;

fn no_faces() -> [Option<ModelFace>; 6] {
    [None; 6]
}

fn slab_side_uv() -> ModelFace {
    ModelFace::new(0.0, 14.0, 16.0, 16.0)
}

/// Stone slab base: sides + bottom use smooth stone; top uses repeater texture.
fn repeater_slab_elements() -> [ModelElement; 2] {
    let mut sides = no_faces();
    sides[NORTH] = Some(slab_side_uv());
    sides[SOUTH] = Some(slab_side_uv());
    sides[WEST] = Some(slab_side_uv());
    sides[EAST] = Some(slab_side_uv());
    // Down face uses bottom texture; omit cullface (engine has no face culling yet).
    sides[0] = Some(ModelFace::FULL);

    let stone_shell = ModelElement {
        from: coord([0.0, 0.0, 0.0]),
        to: coord([16.0, 2.0, 16.0]),
        rotation: None,
        faces: sides,
        texture: ModelTexture::Bottom,
    };

    let mut top_only = no_faces();
    top_only[UP] = Some(ModelFace::FULL);
    let top_cap = ModelElement {
        from: coord([0.0, 0.0, 0.0]),
        to: coord([16.0, 2.0, 16.0]),
        rotation: None,
        faces: top_only,
        texture: ModelTexture::Top,
    };

    [stone_shell, top_cap]
}

fn torch_box_uvs() -> [Option<ModelFace>; 6] {
    let mut faces = no_faces();
    faces[0] = Some(ModelFace::new(7.0, 13.0, 9.0, 15.0));
    faces[UP] = Some(ModelFace::new(7.0, 6.0, 9.0, 8.0));
    faces[NORTH] = Some(ModelFace::new(7.0, 6.0, 9.0, 11.0));
    faces[SOUTH] = Some(ModelFace::new(7.0, 6.0, 9.0, 11.0));
    faces[WEST] = Some(ModelFace::new(7.0, 6.0, 9.0, 11.0));
    faces[EAST] = Some(ModelFace::new(7.0, 6.0, 9.0, 11.0));
    faces
}

fn unlit_torch(z0: f32, z1: f32) -> ModelElement {
    ModelElement {
        from: coord([7.0, 2.0, z0]),
        to: coord([9.0, 7.0, z1]),
        rotation: None,
        faces: torch_box_uvs(),
        texture: ModelTexture::Sides,
    }
}

/// Lit torch cross-planes at a fixed Z span (from `repeater_*tick_on.json`).
fn lit_torch(z0: f32, z1: f32) -> Vec<ModelElement> {
    let cross_uv = ModelFace::new(6.0, 5.0, 10.0, 11.0);
    let cap_uv = ModelFace::new(7.0, 6.0, 9.0, 8.0);

    let mut cap = no_faces();
    cap[UP] = Some(cap_uv);
    let cap_el = ModelElement {
        from: coord([7.0, 7.0, z0]),
        to: coord([9.0, 7.0, z1]),
        rotation: None,
        faces: cap,
        texture: ModelTexture::Sides,
    };

    let mut we = no_faces();
    we[WEST] = Some(cross_uv);
    we[EAST] = Some(cross_uv);
    let we_el = ModelElement {
        from: coord([7.0, 2.0, z0 - 1.0]),
        to: coord([9.0, 8.0, z1 + 1.0]),
        rotation: None,
        faces: we,
        texture: ModelTexture::Sides,
    };

    let mut ns = no_faces();
    ns[NORTH] = Some(cross_uv);
    ns[SOUTH] = Some(cross_uv);
    let ns_el = ModelElement {
        from: coord([6.0, 2.0, z0]),
        to: coord([10.0, 8.0, z1]),
        rotation: None,
        faces: ns,
        texture: ModelTexture::Sides,
    };

    vec![cap_el, we_el, ns_el]
}

/// Rear torch Z range for each delay setting (vanilla `repeater_Ntick.json`).
fn rear_torch_z(delay_ticks: u8) -> (f32, f32) {
    match delay_ticks {
        1 => (6.0, 8.0),
        2 => (8.0, 10.0),
        3 => (10.0, 12.0),
        _ => (12.0, 14.0),
    }
}

fn repeater_model(delay_ticks: u8, powered: bool, facing: Facing) -> BlockModel {
    let (rz0, rz1) = rear_torch_z(delay_ticks);
    const FRONT_Z0: f32 = 2.0;
    const FRONT_Z1: f32 = 4.0;

    let mut elements = Vec::new();
    elements.extend_from_slice(&repeater_slab_elements());

    if powered {
        elements.extend(lit_torch(FRONT_Z0, FRONT_Z1));
        elements.extend(lit_torch(rz0, rz1));
    } else {
        elements.push(unlit_torch(FRONT_Z0, FRONT_Z1));
        elements.push(unlit_torch(rz0, rz1));
    }

    BlockModel {
        layer: ModelRenderLayer::Cutout,
        elements,
        rotation: [0.0, repeater_facing_yaw(facing), 0.0],
    }
}

pub const REPEATER_VARIANT_COUNT: usize = 32;

fn decode_repeater_variant(variant: usize) -> (u8, bool, Facing) {
    let delay_ticks = ((variant >> 3) & 0b11) as u8 + 1;
    let powered = (variant >> 2) & 1 != 0;
    let facing = Facing::from_bits((variant & 0b11) as u16);
    (delay_ticks, powered, facing)
}

pub fn build_repeater_models() -> Vec<BlockModel> {
    (0..REPEATER_VARIANT_COUNT)
        .map(|variant| {
            let (delay, powered, facing) = decode_repeater_variant(variant);
            repeater_model(delay, powered, facing)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{repeater_state, repeater_variant};

    #[test]
    fn model_index_matches_repeater_variant() {
        let models = build_repeater_models();
        for delay in 1..=4u8 {
            for powered in [false, true] {
                for facing in [Facing::North, Facing::South, Facing::East, Facing::West] {
                    let state = repeater_state(powered, facing, delay);
                    let variant = repeater_variant(state) as usize;
                    assert_eq!(models[variant].rotation[1], repeater_facing_yaw(facing));
                }
            }
        }
    }
}
