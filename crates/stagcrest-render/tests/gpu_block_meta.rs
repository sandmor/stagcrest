use stagcrest_mod_client::{apply_block_face_tints, BlockRegistry, ModelRegistry};
use stagcrest_protocol::{
    BlockDef, BlockFaceTextures, BlockGeometry, BlockId, FaceTexture, ModelRenderLayer,
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

fn grass_registry() -> BlockRegistry {
    let mut reg = BlockRegistry::new();
    let grass_top = reg.register_texture("stagcrest:grass_top".into(), 16, 16, vec![0; 16 * 16 * 4]);
    let grass_side =
        reg.register_texture("stagcrest:grass_side".into(), 16, 16, vec![0; 16 * 16 * 4]);
    let grass_overlay = reg.register_texture(
        "stagcrest:grass_side_overlay".into(),
        16,
        16,
        vec![0; 16 * 16 * 4],
    );
    let dirt = reg.register_texture("stagcrest:dirt".into(), 16, 16, vec![0; 16 * 16 * 4]);
    let mut face_textures = BlockFaceTextures::uniform(grass_side);
    apply_block_face_tints("stagcrest:grass_block", false, &mut face_textures, &reg);
    reg.register_block(BlockDef {
        id: BlockId(2),
        namespaced_id: "stagcrest:grass_block".into(),
        display_name: "Grass Block".into(),
        face_textures,
        geometry: BlockGeometry::Cube,
        render_layer: ModelRenderLayer::Opaque,
        opaque: true,
        solid: true,
        transparent: false,
        fluid: false,
        placeable: true,
        hardness: 0.6,
        circuit: None,
        push_reaction: stagcrest_protocol::PushReaction::Block,
    });
    let _ = (grass_top, grass_overlay, dirt);
    reg
}

#[test]
fn gpu_meta_for_grass_block_has_tint_and_overlay() {
    let reg = grass_registry();
    let grass = reg.block_by_name("stagcrest:grass_block").expect("grass block");
    let models = ModelRegistry::new();
    let buckets = DrawBucketRegistry::build(&models);
    let tables = GpuBlockTables::build(&reg, &models, &buckets);
    let meta = tables.block_meta[grass.0 as usize];
    assert_eq!(meta.tint_kinds & 0xFF, TintKind::Grass as u32);
    assert_eq!((meta.tint_kinds >> 8) & 0xFF, TintKind::None as u32);
    assert_eq!((meta.overlay_tint_kinds >> 16) & 0xFF, TintKind::Grass as u32);
    assert_ne!(meta.texture_overlay_sides, 0);
}

#[test]
fn gpu_meta_for_acacia_leaves_has_foliage_tint() {
    let mut reg = BlockRegistry::new();
    let tex = reg.register_texture("stagcrest:acacia_leaves".into(), 16, 16, vec![0; 16 * 16 * 4]);
    let mut face_textures = BlockFaceTextures::uniform(tex);
    apply_block_face_tints("stagcrest:acacia_leaves", false, &mut face_textures, &reg);
    let leaves = reg.allocate_block_id();
    reg.register_block(BlockDef {
        id: leaves,
        namespaced_id: "stagcrest:acacia_leaves".into(),
        display_name: "Acacia Leaves".into(),
        face_textures,
        geometry: BlockGeometry::Cube,
        render_layer: ModelRenderLayer::Cutout,
        opaque: false,
        solid: true,
        transparent: true,
        fluid: false,
        placeable: true,
        hardness: 0.2,
        circuit: None,
        push_reaction: stagcrest_protocol::PushReaction::Block,
    });
    let models = ModelRegistry::new();
    let buckets = DrawBucketRegistry::build(&models);
    let tables = GpuBlockTables::build(&reg, &models, &buckets);
    let meta = tables.block_meta[leaves.0 as usize];
    assert_eq!(meta.tint_kinds & 0xFF, TintKind::Foliage as u32);
    assert_eq!((meta.tint_kinds >> 8) & 0xFF, TintKind::Foliage as u32);
    assert_eq!((meta.tint_kinds >> 16) & 0xFF, TintKind::Foliage as u32);
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
