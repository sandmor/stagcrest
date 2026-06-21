use stagcrest_protocol::{
    BlockModel, ModelAxis, ModelElement, ModelFace, ModelId, ModelRenderLayer, ModelRotation,
    ModelVariant, TorchAttachment,
};

const S: f32 = 1.0 / 16.0;

fn bc(v: f32) -> f32 {
    v * S
}

fn coord(p: [f32; 3]) -> [f32; 3] {
    [bc(p[0]), bc(p[1]), bc(p[2])]
}

/// Indices into the 6-face array, in `BoxFace` order.
const DOWN: usize = 0;
const UP: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;

fn element(
    from: [f32; 3],
    to: [f32; 3],
    rotation: Option<ModelRotation>,
    faces: [Option<ModelFace>; 6],
) -> ModelElement {
    ModelElement {
        from: coord(from),
        to: coord(to),
        rotation,
        faces,
    }
}

fn no_faces() -> [Option<ModelFace>; 6] {
    [None; 6]
}

/// The three elements that make up a Minecraft torch (`template_torch`):
/// a small top cap plus two crossed full-height planes whose transparent
/// pixels are discarded by the cutout shader.
fn torch_elements(rotation: Option<ModelRotation>) -> Vec<ModelElement> {
    // Top cap: down + up faces.
    let mut cap = no_faces();
    cap[DOWN] = Some(ModelFace::new(7.0, 13.0, 9.0, 15.0));
    cap[UP] = Some(ModelFace::new(7.0, 6.0, 9.0, 8.0));

    // West/east plane (thin in X, full in Y/Z).
    let mut we = no_faces();
    we[WEST] = Some(ModelFace::FULL);
    we[EAST] = Some(ModelFace::FULL);

    // North/south plane (thin in Z, full in X/Y).
    let mut ns = no_faces();
    ns[NORTH] = Some(ModelFace::FULL);
    ns[SOUTH] = Some(ModelFace::FULL);

    vec![
        element([7.0, 0.0, 7.0], [9.0, 10.0, 9.0], rotation, cap),
        element([7.0, 0.0, 0.0], [9.0, 16.0, 16.0], rotation, we),
        element([0.0, 0.0, 7.0], [16.0, 16.0, 9.0], rotation, ns),
    ]
}

fn floor_torch_model() -> BlockModel {
    BlockModel {
        layer: ModelRenderLayer::Cutout,
        elements: torch_elements(None),
        y_rotation: 0.0,
    }
}

/// Wall torch, matching `template_torch_wall`: torch mounted on the west wall,
/// leaning out along +X via a -22.5° lean about Z at `[0, 3.5, 8]`. The whole
/// model is rotated about the vertical axis (`y_rotation`) to face the four
/// supported walls.
fn wall_torch_model(y_rotation: f32) -> BlockModel {
    let lean = ModelRotation {
        origin: coord([0.0, 3.5, 8.0]),
        axis: ModelAxis::Z,
        angle: -22.5,
        rescale: false,
    };

    let mut cap = no_faces();
    cap[DOWN] = Some(ModelFace::new(7.0, 13.0, 9.0, 15.0));
    cap[UP] = Some(ModelFace::new(7.0, 6.0, 9.0, 8.0));

    let mut we = no_faces();
    we[WEST] = Some(ModelFace::FULL);
    we[EAST] = Some(ModelFace::FULL);

    let mut ns = no_faces();
    ns[NORTH] = Some(ModelFace::FULL);
    ns[SOUTH] = Some(ModelFace::FULL);

    let elements = vec![
        element(
            [-1.0, 3.5, 7.0],
            [1.0, 13.5, 9.0],
            Some(lean),
            cap,
        ),
        element(
            [-1.0, 3.5, 0.0],
            [1.0, 19.5, 16.0],
            Some(lean),
            we,
        ),
        element(
            [-8.0, 3.5, 7.0],
            [8.0, 19.5, 9.0],
            Some(lean),
            ns,
        ),
    ];

    BlockModel {
        layer: ModelRenderLayer::Cutout,
        elements,
        y_rotation,
    }
}

pub fn torch_variant_from_attachment(attachment: TorchAttachment) -> ModelVariant {
    attachment as u8
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    redstone_torch: [BlockModel; 5],
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistry {
    pub fn new() -> Self {
        // Variant index follows `TorchAttachment as u8`:
        // 0 = Floor, 1 = WallNorth, 2 = WallSouth, 3 = WallEast, 4 = WallWest.
        // The base wall model points east (mounted on the west wall), so it
        // needs an extra Y rotation per facing.
        Self {
            redstone_torch: [
                floor_torch_model(),
                wall_torch_model(90.0),  // WallNorth: mounted +Z wall, leans -Z
                wall_torch_model(270.0), // WallSouth: mounted -Z wall, leans +Z
                wall_torch_model(0.0),   // WallEast: mounted -X wall, leans +X
                wall_torch_model(180.0), // WallWest: mounted +X wall, leans -X
            ],
        }
    }

    pub fn get(&self, id: ModelId, variant: ModelVariant) -> &BlockModel {
        match id {
            ModelId::RedstoneTorch => {
                let idx = variant.min(4) as usize;
                &self.redstone_torch[idx]
            }
        }
    }
}
