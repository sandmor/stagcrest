//! Piston body and head geometry. Canonical front points toward -Z; rotation maps Facing6.

use stagcrest_protocol::{
    piston_head_variant, piston_variant, BlockModel, Facing6, ModelElement, ModelFace,
    ModelRenderLayer, ModelTexture,
};

const S: f32 = 1.0 / 16.0;

fn coord(p: [f32; 3]) -> [f32; 3] {
    [p[0] * S, p[1] * S, p[2] * S]
}

const DOWN: usize = 0;
const UP: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;

fn no_faces() -> [Option<ModelFace>; 6] {
    [None; 6]
}

fn piston_body_model(extended: bool, facing: Facing6) -> BlockModel {
    let depth = if extended { 12.0 } else { 16.0 };
    let mut body_faces = no_faces();
    body_faces[DOWN] = Some(ModelFace::FULL);
    body_faces[UP] = Some(ModelFace::FULL);
    body_faces[WEST] = Some(ModelFace::FULL);
    body_faces[EAST] = Some(ModelFace::FULL);
    body_faces[SOUTH] = Some(ModelFace::FULL);
    if !extended {
        body_faces[NORTH] = Some(ModelFace::FULL);
    }

    let body = ModelElement {
        from: coord([0.0, 0.0, 16.0 - depth]),
        to: coord([16.0, 16.0, 16.0]),
        rotation: None,
        faces: body_faces,
        texture: ModelTexture::Sides,
    };

    let mut elements = vec![body];

    if extended {
        let mut inner_faces = no_faces();
        inner_faces[NORTH] = Some(ModelFace::FULL);
        elements.push(ModelElement {
            from: coord([0.0, 0.0, 0.0]),
            to: coord([16.0, 16.0, 16.0 - depth]),
            rotation: None,
            faces: inner_faces,
            texture: ModelTexture::Top,
        });
    } else {
        let mut front_faces = no_faces();
        front_faces[NORTH] = Some(ModelFace::FULL);
        elements.push(ModelElement {
            from: coord([0.0, 0.0, 0.0]),
            to: coord([16.0, 16.0, 16.0]),
            rotation: None,
            faces: front_faces,
            texture: ModelTexture::Top,
        });
    }

    let mut back_faces = no_faces();
    back_faces[SOUTH] = Some(ModelFace::FULL);
    elements.push(ModelElement {
        from: coord([0.0, 0.0, 0.0]),
        to: coord([16.0, 16.0, 16.0]),
        rotation: None,
        faces: back_faces,
        texture: ModelTexture::Bottom,
    });

    BlockModel {
        layer: ModelRenderLayer::Opaque,
        elements,
        rotation: facing.model_rotation(),
    }
}

fn piston_head_model(_sticky: bool, facing: Facing6) -> BlockModel {
    let mut plate_faces = no_faces();
    plate_faces[NORTH] = Some(ModelFace::FULL);
    let plate = ModelElement {
        from: coord([0.0, 0.0, 0.0]),
        to: coord([16.0, 16.0, 4.0]),
        rotation: None,
        faces: plate_faces,
        texture: ModelTexture::Top,
    };

    let mut arm_faces = no_faces();
    arm_faces[WEST] = Some(ModelFace::FULL);
    arm_faces[EAST] = Some(ModelFace::FULL);
    arm_faces[UP] = Some(ModelFace::FULL);
    arm_faces[DOWN] = Some(ModelFace::FULL);
    let arm = ModelElement {
        from: coord([6.0, 6.0, 4.0]),
        to: coord([10.0, 10.0, 16.0]),
        rotation: None,
        faces: arm_faces,
        texture: ModelTexture::Sides,
    };

    BlockModel {
        layer: ModelRenderLayer::Opaque,
        elements: vec![arm, plate],
        rotation: facing.model_rotation(),
    }
}

pub const PISTON_VARIANT_COUNT: usize = 16;
pub const PISTON_HEAD_VARIANT_COUNT: usize = 16;

pub fn build_piston_models() -> Vec<BlockModel> {
    (0..PISTON_VARIANT_COUNT)
        .map(|variant| {
            let extended = (variant >> 3) & 1 != 0;
            let facing = Facing6::from_bits((variant & 0b111) as u16);
            piston_body_model(extended, facing)
        })
        .collect()
}

pub fn build_piston_head_models() -> Vec<BlockModel> {
    (0..PISTON_HEAD_VARIANT_COUNT)
        .map(|variant| {
            let sticky = (variant >> 3) & 1 != 0;
            let facing = Facing6::from_bits((variant & 0b111) as u16);
            piston_head_model(sticky, facing)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{piston_head_state, piston_state};

    #[test]
    fn piston_variant_count_matches_models() {
        let models = build_piston_models();
        assert_eq!(models.len(), PISTON_VARIANT_COUNT);
        for extended in [false, true] {
            for facing in [
                Facing6::Down,
                Facing6::Up,
                Facing6::North,
                Facing6::South,
                Facing6::West,
                Facing6::East,
            ] {
                let state = piston_state(extended, facing);
                let variant = piston_variant(state) as usize;
                assert_eq!(models[variant].rotation, facing.model_rotation());
            }
        }
    }

    #[test]
    fn piston_head_variant_count_matches_models() {
        let models = build_piston_head_models();
        assert_eq!(models.len(), PISTON_HEAD_VARIANT_COUNT);
        for sticky in [false, true] {
            for facing in [Facing6::North, Facing6::South, Facing6::East, Facing6::West] {
                let state = piston_head_state(facing, sticky);
                let variant = piston_head_variant(state) as usize;
                assert_eq!(models[variant].rotation, facing.model_rotation());
            }
        }
    }
}
