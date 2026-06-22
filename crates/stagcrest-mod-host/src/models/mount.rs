//! Geometry for face-mounted blocks (lever, button). Models are built in a
//! canonical floor-mounted, north-facing frame and rotated into place via the
//! whole-model orientation (`BlockModel::rotation`), so a single builder covers
//! all floor/ceiling/wall + facing combinations.

use stagcrest_protocol::{
    AttachFace, BlockModel, Facing, ModelAxis, ModelElement, ModelFace, ModelRenderLayer,
    ModelRotation, ModelTexture,
};

const S: f32 = 1.0 / 16.0;

fn coord(p: [f32; 3]) -> [f32; 3] {
    [p[0] * S, p[1] * S, p[2] * S]
}

fn full_faces() -> [Option<ModelFace>; 6] {
    ModelElement::all_faces(ModelFace::FULL)
}

/// BoxFace order: Down, Up, North, South, West, East.
const UP: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;

fn no_faces() -> [Option<ModelFace>; 6] {
    [None; 6]
}

/// Handle face UVs matching vanilla `lever.json` (partial sprite region).
fn lever_handle_faces() -> [Option<ModelFace>; 6] {
    let mut faces = no_faces();
    faces[UP] = Some(ModelFace::new(7.0, 6.0, 9.0, 8.0));
    faces[NORTH] = Some(ModelFace::new(7.0, 6.0, 9.0, 16.0));
    faces[SOUTH] = Some(ModelFace::new(7.0, 6.0, 9.0, 16.0));
    faces[WEST] = Some(ModelFace::new(7.0, 6.0, 9.0, 16.0));
    faces[EAST] = Some(ModelFace::new(7.0, 6.0, 9.0, 16.0));
    faces
}

/// How many distinct variants the mount state can encode (on, face, facing).
pub const MOUNT_VARIANT_COUNT: usize = 32;

fn facing_yaw(facing: Facing) -> f32 {
    match facing {
        Facing::South => 0.0,
        Facing::East => 90.0,
        Facing::North => 180.0,
        Facing::West => 270.0,
    }
}

/// Whole-model orientation (Euler degrees, X then Y then Z) that maps the
/// canonical floor/north model onto the requested surface and facing.
fn orientation(face: AttachFace, facing: Facing) -> [f32; 3] {
    let yaw = facing_yaw(facing);
    match face {
        AttachFace::Floor => [0.0, yaw, 0.0],
        AttachFace::Ceiling => [180.0, yaw, 0.0],
        AttachFace::Wall => [90.0, yaw, 0.0],
    }
}

fn decode_variant(variant: usize) -> (bool, AttachFace, Facing) {
    let on = variant & 0b1 != 0;
    let face = AttachFace::from_bits(((variant >> 1) & 0b11) as u16);
    let facing = Facing::from_bits(((variant >> 3) & 0b11) as u16);
    (on, face, facing)
}

/// Lever: a cobblestone base plate with a tilting handle. The handle leans one
/// way when off and the opposite way when on.
fn lever_model(on: bool, face: AttachFace, facing: Facing) -> BlockModel {
    let base = ModelElement {
        from: coord([5.0, 0.0, 4.0]),
        to: coord([11.0, 3.0, 12.0]),
        rotation: None,
        faces: full_faces(),
        texture: ModelTexture::Bottom,
    };

    let angle = if on { 45.0 } else { -45.0 };
    let handle = ModelElement {
        from: coord([7.0, 1.0, 7.0]),
        to: coord([9.0, 11.0, 9.0]),
        rotation: Some(ModelRotation {
            origin: coord([8.0, 1.0, 8.0]),
            axis: ModelAxis::X,
            angle,
            rescale: false,
        }),
        faces: lever_handle_faces(),
        texture: ModelTexture::Sides,
    };

    BlockModel {
        layer: ModelRenderLayer::Cutout,
        elements: vec![base, handle],
        rotation: orientation(face, facing),
    }
}

/// Button: a small flat box that sinks slightly when pressed (on).
fn button_model(on: bool, face: AttachFace, facing: Facing) -> BlockModel {
    let height = if on { 1.0 } else { 2.0 };
    let button = ModelElement {
        from: coord([5.0, 0.0, 6.0]),
        to: coord([11.0, height, 10.0]),
        rotation: None,
        faces: full_faces(),
        texture: ModelTexture::Sides,
    };

    BlockModel {
        layer: ModelRenderLayer::Opaque,
        elements: vec![button],
        rotation: orientation(face, facing),
    }
}

fn build_models(builder: impl Fn(bool, AttachFace, Facing) -> BlockModel) -> Vec<BlockModel> {
    (0..MOUNT_VARIANT_COUNT)
        .map(|variant| {
            let (on, face, facing) = decode_variant(variant);
            builder(on, face, facing)
        })
        .collect()
}

pub fn build_lever_models() -> Vec<BlockModel> {
    build_models(lever_model)
}

pub fn build_button_models() -> Vec<BlockModel> {
    build_models(button_model)
}
