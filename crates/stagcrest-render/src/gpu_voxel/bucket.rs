use stagcrest_protocol::ModelId;
use stagcrest_mod_client::ModelRegistry;

use crate::gpu_voxel::types::GpuRenderLayer;

pub const BUCKET_CUBE_OPAQUE: u32 = 0;
pub const BUCKET_CUBE_CUTOUT: u32 = 1;
pub const BUCKET_CUBE_BLEND: u32 = 2;
pub const BUCKET_CROSS_CUTOUT: u32 = 3;
pub const BUCKET_WIRE_CUTOUT: u32 = 4;
pub const MODEL_BUCKET_BASE: u32 = 16;

#[derive(Clone, Debug)]
pub struct DrawBucket {
    pub id: u32,
    pub layer: GpuRenderLayer,
    pub mesh_index: u32,
    pub index_count: u32,
    pub first_index: u32,
    pub base_vertex: u32,
}

#[derive(Clone, Debug)]
pub struct DrawBucketRegistry {
    pub buckets: Vec<DrawBucket>,
    pub model_bucket_base: [u32; 8],
}

impl DrawBucketRegistry {
    pub fn build(models: &ModelRegistry) -> Self {
        let mut buckets = Vec::new();

        for (id, layer) in [
            (BUCKET_CUBE_OPAQUE, GpuRenderLayer::Opaque),
            (BUCKET_CUBE_CUTOUT, GpuRenderLayer::Cutout),
            (BUCKET_CUBE_BLEND, GpuRenderLayer::Blend),
            (BUCKET_CROSS_CUTOUT, GpuRenderLayer::Cutout),
            (BUCKET_WIRE_CUTOUT, GpuRenderLayer::Cutout),
        ] {
            buckets.push(DrawBucket {
                id,
                layer,
                mesh_index: id,
                index_count: 0,
                first_index: 0,
                base_vertex: 0,
            });
        }

        let mut model_bucket_base = [0u32; 8];
        let mut next_id = MODEL_BUCKET_BASE;
        let mut next_mesh = 5u32;

        let model_defs: [(ModelId, usize); 7] = [
            (ModelId::RedstoneTorch, models.variant_count(ModelId::RedstoneTorch)),
            (ModelId::Lever, models.lever_count()),
            (ModelId::Button, models.button_count()),
            (ModelId::Repeater, models.repeater_count()),
            (ModelId::Observer, models.observer_count()),
            (ModelId::Piston, models.piston_count()),
            (ModelId::PistonHead, models.piston_head_count()),
        ];

        for (i, (model_id, count)) in model_defs.iter().enumerate() {
            model_bucket_base[i] = next_id;
            let layer = model_layer_for_id(*model_id, models);
            for v in 0..*count {
                buckets.push(DrawBucket {
                    id: next_id + v as u32,
                    layer,
                    mesh_index: next_mesh + v as u32,
                    index_count: 0,
                    first_index: 0,
                    base_vertex: 0,
                });
            }
            next_id += *count as u32;
            next_mesh += *count as u32;
        }

        Self {
            buckets,
            model_bucket_base,
        }
    }

    pub fn wire_bucket_id(&self) -> u32 {
        BUCKET_WIRE_CUTOUT
    }

    pub fn cross_bucket_id(&self) -> u32 {
        BUCKET_CROSS_CUTOUT
    }

    pub fn model_bucket_base(&self, model_id: ModelId) -> u32 {
        let idx = model_id_index(model_id);
        self.model_bucket_base[idx]
    }
}

fn model_id_index(model_id: ModelId) -> usize {
    match model_id {
        ModelId::RedstoneTorch => 0,
        ModelId::Lever => 1,
        ModelId::Button => 2,
        ModelId::Repeater => 3,
        ModelId::Observer => 4,
        ModelId::Piston => 5,
        ModelId::PistonHead => 6,
    }
}

fn model_layer_for_id(model_id: ModelId, models: &ModelRegistry) -> GpuRenderLayer {
    let model = models.get(model_id, 0);
    GpuRenderLayer::from_protocol(model.layer)
}
