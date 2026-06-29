//! Observer block geometry. Canonical output arrow points toward -Z; Y rotation maps facing.

use stagcrest_protocol::{
    repeater_facing_yaw, BlockModel, Facing, ModelElement, ModelFace, ModelRenderLayer,
    ModelTexture,
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

fn observer_model(_powered: bool, facing: Facing) -> BlockModel {
    let mut shell_faces = no_faces();
    shell_faces[DOWN] = Some(ModelFace::FULL);
    shell_faces[UP] = Some(ModelFace::FULL);
    shell_faces[WEST] = Some(ModelFace::FULL);
    shell_faces[EAST] = Some(ModelFace::FULL);
    let shell = ModelElement {
        from: coord([0.0, 0.0, 0.0]),
        to: coord([16.0, 16.0, 16.0]),
        rotation: None,
        faces: shell_faces,
        texture: ModelTexture::Bottom,
    };

    // Output face (canonical -Z); block `top` texture slot = observer_back.
    let mut output_faces = no_faces();
    output_faces[NORTH] = Some(ModelFace::FULL);
    let output = ModelElement {
        from: coord([0.0, 0.0, 0.0]),
        to: coord([16.0, 16.0, 16.0]),
        rotation: None,
        faces: output_faces,
        texture: ModelTexture::Top,
    };

    // Detecting face (canonical +Z); block `sides` texture slot = observer_front.
    let mut detect_faces = no_faces();
    detect_faces[SOUTH] = Some(ModelFace::FULL);
    let detecting = ModelElement {
        from: coord([0.0, 0.0, 0.0]),
        to: coord([16.0, 16.0, 16.0]),
        rotation: None,
        faces: detect_faces,
        texture: ModelTexture::Sides,
    };

    BlockModel {
        layer: ModelRenderLayer::Opaque,
        elements: vec![shell, output, detecting],
        rotation: [0.0, repeater_facing_yaw(facing), 0.0],
    }
}

pub const OBSERVER_VARIANT_COUNT: usize = 8;

fn decode_observer_variant(variant: usize) -> (bool, Facing) {
    let powered = (variant >> 2) & 1 != 0;
    let facing = Facing::from_bits((variant & 0b11) as u16);
    (powered, facing)
}

pub fn build_observer_models() -> Vec<BlockModel> {
    (0..OBSERVER_VARIANT_COUNT)
        .map(|variant| {
            let (powered, facing) = decode_observer_variant(variant);
            observer_model(powered, facing)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{observer_state, observer_variant};

    #[test]
    fn model_index_matches_observer_variant() {
        let models = build_observer_models();
        for powered in [false, true] {
            for facing in [Facing::North, Facing::South, Facing::East, Facing::West] {
                let state = observer_state(powered, facing);
                let variant = observer_variant(state) as usize;
                assert_eq!(models[variant].rotation[1], repeater_facing_yaw(facing));
            }
        }
    }
}
