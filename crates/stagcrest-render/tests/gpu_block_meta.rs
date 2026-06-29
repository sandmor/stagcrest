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

#[test]
fn voxel_instance_size_matches_gpu_stride() {
    use bevy::render::render_resource::ShaderType;
    use stagcrest_render::gpu_voxel::types::VoxelInstance;
    let rust = std::mem::size_of::<VoxelInstance>();
    let shader = VoxelInstance::min_size().get() as usize;
    eprintln!("VoxelInstance rust={rust} shader={shader}");
    assert_eq!(rust, shader, "VoxelInstance CPU/GPU stride mismatch");
}

#[test]
fn wgsl_voxel_instance_bucket_id_offset_matches_rust() {
    use std::mem::offset_of;
    use stagcrest_render::gpu_voxel::types::VoxelInstance;

    let wgsl = include_str!("../../../assets/shaders/voxel_compact.wgsl");
    let mut parser = naga::front::wgsl::Frontend::new();
    let module = parser.parse(wgsl).expect("parse voxel_compact.wgsl");
    let mut layouter = naga::proc::Layouter::default();
    layouter
        .update(module.to_ctx())
        .expect("layout voxel_compact types");
    let rust_bucket = offset_of!(VoxelInstance, bucket_id);
    let rust_size = std::mem::size_of::<VoxelInstance>();

    for (handle, ty) in module.types.iter() {
        if ty.name.as_deref() != Some("VoxelInstance") {
            continue;
        }
        let layout = layouter[handle];
        let naga::TypeInner::Struct { members, span, .. } = &ty.inner else {
            panic!("VoxelInstance is not a struct");
        };
        let bucket_member = members
            .iter()
            .find(|m| m.name.as_deref() == Some("bucket_id"))
            .expect("bucket_id member in WGSL VoxelInstance");
        eprintln!(
            "VoxelInstance rust_size={rust_size} wgsl_size={span} rust_bucket_off={rust_bucket} wgsl_bucket_off={}",
            bucket_member.offset
        );
        assert_eq!(*span as usize, rust_size, "struct size mismatch");
        assert_eq!(
            bucket_member.offset as usize,
            rust_bucket,
            "bucket_id offset mismatch — south faces (world_pos.w==3) may route to cross bucket"
        );
        assert_eq!(layout.size as usize, rust_size, "layouter size mismatch");
        return;
    }
    panic!("VoxelInstance struct not found in voxel_compact.wgsl");
}
