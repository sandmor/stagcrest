use stagcrest_mod_client::{BlockRegistry, ModelRegistry, TextureAtlas};
use stagcrest_protocol::{
    BlockDef, BlockFaceTextures, BlockGeometry, BlockId, BlockState, FaceTexture, ModelRenderLayer,
    TintKind,
};
use stagcrest_render::gpu_voxel::block_meta::GpuBlockTables;
use stagcrest_render::gpu_voxel::bucket::DrawBucketRegistry;
use stagcrest_render::gpu_voxel::types::{GpuGeometryKind, GpuRenderLayer, BLOCK_FLAG_OPAQUE, BLOCK_FLAG_SOLID};

fn stone_registry() -> BlockRegistry {
    let mut reg = BlockRegistry::new();
    let stone_tex = reg.register_texture("stagcrest:stone".into(), 16, 16, vec![0; 16 * 16 * 4]);
    reg.register_block(BlockDef {
        id: BlockId(1),
        namespaced_id: "stagcrest:stone".into(),
        display_name: "Stone".into(),
        face_textures: BlockFaceTextures {
            top: FaceTexture {
                texture: stone_tex,
                overlay: None,
                tint: TintKind::None,
                overlay_tint: TintKind::None,
            },
            bottom: FaceTexture {
                texture: stone_tex,
                overlay: None,
                tint: TintKind::None,
                overlay_tint: TintKind::None,
            },
            sides: FaceTexture {
                texture: stone_tex,
                overlay: None,
                tint: TintKind::None,
                overlay_tint: TintKind::None,
            },
        },
        geometry: BlockGeometry::Cube,
        render_layer: ModelRenderLayer::Opaque,
        opaque: true,
        solid: true,
        transparent: false,
        fluid: false,
        placeable: true,
        hardness: 1.0,
        circuit: None,
        push_reaction: stagcrest_protocol::PushReaction::Block,
    });
    reg
}

#[test]
fn gpu_meta_for_stone_is_cube_opaque() {
    let reg = stone_registry();
    let models = ModelRegistry::new();
    let buckets = DrawBucketRegistry::build(&models);
    let tables = GpuBlockTables::build(&reg, &models, &buckets);
    let meta = tables.block_meta[1];
    assert_eq!(meta.geometry_kind, GpuGeometryKind::Cube as u32);
    assert_eq!(meta.render_layer, GpuRenderLayer::Opaque as u32);
    assert!(meta.flags & BLOCK_FLAG_OPAQUE != 0);
    assert!(meta.flags & BLOCK_FLAG_SOLID != 0);
}

#[test]
fn model_bucket_registry_has_prototype_meshes() {
    let reg = stone_registry();
    let models = ModelRegistry::new();
    let buckets = DrawBucketRegistry::build(&models);
    assert!(buckets.buckets.len() > 5);
    let _ = GpuBlockTables::build(&reg, &models, &buckets);
}
