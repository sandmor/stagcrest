use stagcrest_mesh::{
    bake_block_model, bake_cross_plant, bake_unit_quad, bake_wire_quad, BakedMesh, GpuMeshVertex,
};
use stagcrest_mod_client::{BlockRegistry, ModelRegistry};
use stagcrest_protocol::ModelId;

use crate::gpu_voxel::bucket::{DrawBucketRegistry, MODEL_BUCKET_BASE};

#[derive(Clone, Debug)]
pub struct GpuMeshLibrary {
    pub vertices: Vec<GpuMeshVertex>,
    pub indices: Vec<u32>,
    pub mesh_ranges: Vec<MeshRange>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MeshRange {
    pub first_index: u32,
    pub index_count: u32,
    pub base_vertex: u32,
}

impl GpuMeshLibrary {
    pub fn build(
        registry: &BlockRegistry,
        models: &ModelRegistry,
        buckets: &mut DrawBucketRegistry,
    ) -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut mesh_ranges = vec![MeshRange::default(); buckets.buckets.len()];

        let prototypes: [(u32, fn() -> BakedMesh); 5] = [
            (0, bake_unit_quad),
            (1, bake_unit_quad),
            (2, bake_unit_quad),
            (3, bake_cross_plant),
            (4, bake_wire_quad),
        ];

        for (mesh_index, bake_fn) in prototypes {
            let baked = bake_fn();
            append_mesh(&mut vertices, &mut indices, &baked, mesh_index, &mut mesh_ranges);
        }

        let model_defs: [(ModelId, usize); 7] = [
            (ModelId::RedstoneTorch, models.variant_count(ModelId::RedstoneTorch)),
            (ModelId::Lever, models.lever_count()),
            (ModelId::Button, models.button_count()),
            (ModelId::Repeater, models.repeater_count()),
            (ModelId::Observer, models.observer_count()),
            (ModelId::Piston, models.piston_count()),
            (ModelId::PistonHead, models.piston_head_count()),
        ];

        let mut mesh_index = 5u32;
        for (model_id, count) in model_defs {
            let base_bucket = buckets.model_bucket_base(model_id);
            for variant in 0..count {
                let model = models.get(model_id, variant as u8);
                let block_id = default_block_for_model(model_id, registry);
                // Bake each variant with the textures for a state that maps to it,
                // so lit/powered variants (torch on, repeater on, ...) sample their
                // lit textures instead of always using the unlit state-0 textures.
                let rep_state = stagcrest_mod_client::representative_state(model_id, variant as u8);
                let faces = registry
                    .block_face_textures_for_state(block_id, rep_state)
                    .or_else(|| registry.block(block_id).map(|d| d.face_textures));
                let faces = faces.unwrap_or_else(|| {
                    let tex = registry.texture_by_name("stagcrest:stone").unwrap_or(stagcrest_protocol::TextureId(0));
                    let face = stagcrest_protocol::FaceTexture {
                        texture: tex,
                        overlay: None,
                        tint: stagcrest_protocol::TintKind::None,
                        overlay_tint: stagcrest_protocol::TintKind::None,
                    };
                    stagcrest_protocol::BlockFaceTextures {
                        top: face,
                        bottom: face,
                        sides: face,
                    }
                });
                let baked = bake_block_model(model, &faces, registry);
                let bucket_id = base_bucket + variant as u32;
                append_mesh(
                    &mut vertices,
                    &mut indices,
                    &baked,
                    mesh_index,
                    &mut mesh_ranges,
                );
                if let Some(bucket) = buckets.buckets.iter_mut().find(|b| b.id == bucket_id) {
                    let range = mesh_ranges[mesh_index as usize];
                    bucket.index_count = range.index_count;
                    bucket.first_index = range.first_index;
                    bucket.base_vertex = range.base_vertex;
                    bucket.mesh_index = mesh_index;
                }
                mesh_index += 1;
            }
        }

        for bucket in &mut buckets.buckets {
            if bucket.id < MODEL_BUCKET_BASE {
                let range = mesh_ranges[bucket.mesh_index as usize];
                bucket.index_count = range.index_count;
                bucket.first_index = range.first_index;
                bucket.base_vertex = range.base_vertex;
            }
        }

        Self {
            vertices,
            indices,
            mesh_ranges,
        }
    }
}

fn append_mesh(
    vertices: &mut Vec<GpuMeshVertex>,
    indices: &mut Vec<u32>,
    baked: &BakedMesh,
    mesh_index: u32,
    mesh_ranges: &mut [MeshRange],
) {
    let base_vertex = vertices.len() as u32;
    let first_index = indices.len() as u32;
    vertices.extend_from_slice(&baked.vertices);
    // Keep indices mesh-local; the draw call applies `base_vertex` via the
    // indirect args (draw_indexed_indirect adds base_vertex to each index).
    for idx in &baked.indices {
        indices.push(*idx);
    }
    if (mesh_index as usize) < mesh_ranges.len() {
        mesh_ranges[mesh_index as usize] = MeshRange {
            first_index,
            index_count: baked.indices.len() as u32,
            base_vertex,
        };
    }
}

fn default_block_for_model(model_id: ModelId, registry: &BlockRegistry) -> stagcrest_protocol::BlockId {
    let name = match model_id {
        ModelId::RedstoneTorch => "stagcrest:redstone_torch",
        ModelId::Lever => "stagcrest:lever",
        ModelId::Button => "stagcrest:stone_button",
        ModelId::Repeater => "stagcrest:repeater",
        ModelId::Observer => "stagcrest:observer",
        ModelId::Piston => "stagcrest:piston",
        ModelId::PistonHead => "stagcrest:piston_head",
    };
    registry.block_by_name(name).unwrap_or(stagcrest_protocol::BlockId(0))
}
