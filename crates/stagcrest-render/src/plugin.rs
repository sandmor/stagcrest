use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use stagcrest_mesh::{ChunkMesh, MeshCache};
use stagcrest_mod_client::TextureAtlas;
use stagcrest_protocol::ChunkPos;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::reflection_camera::water_chunk_layers;
use crate::scene_reflection::SceneReflectionImages;
use crate::voxel_material::{VoxelMaterial, VoxelMaterialPlugin, MAX_ATLAS_PAGES};
use crate::water_material::{WaterMaterial, WaterMaterialPlugin};

static ATLAS_REVISION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Returns a unique revision id for each newly inserted block atlas resource.
pub fn next_atlas_revision() -> u64 {
    ATLAS_REVISION_COUNTER.fetch_add(1, Ordering::Relaxed) + 1
}

#[derive(Resource, Default)]
pub struct MeshCacheResource(pub MeshCache);

impl MeshCacheResource {
    pub fn light_grid(&self, pos: stagcrest_protocol::ChunkPos) -> Option<&stagcrest_mesh::ChunkLightGrid> {
        self.0.light_grid(pos)
    }
}

/// Chunks that must be remeshed after atlas/tint changes invalidate cached geometry.
#[derive(Resource, Default)]
pub struct PendingMeshRebuild {
    pub chunks: Vec<ChunkPos>,
}

#[derive(Resource)]
pub struct BlockAtlasResource {
    pub revision: u64,
    pub atlases: Vec<TextureAtlas>,
    pub grass_tint: Color,
    pub foliage_tint: Color,
    pub water_tint: Color,
    /// (frame_count, frame_uv_step, frametime_secs, elapsed_secs) — `.w` updated each frame
    pub fluid_anim: Vec4,
}

pub fn atlas_pixels_to_image(atlas: &TextureAtlas) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        atlas.pixels.clone(),
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );
    image.sampler = ImageSampler::nearest();
    image
}

/// Render bucket: 0 = opaque, 1 = blend, 2 = cutout, 3 = water.
#[derive(Component)]
pub struct ChunkEntityMarker {
    pub pos: ChunkPos,
    pub bucket: u8,
}

type AtlasKey = [u32; 32];

#[derive(Default)]
pub struct ChunkMaterialCache {
    opaque: Option<Handle<VoxelMaterial>>,
    blend: Option<Handle<VoxelMaterial>>,
    water: Option<Handle<WaterMaterial>>,
    cutout: Option<Handle<VoxelMaterial>>,
}

fn atlas_key_from(
    revision: u64,
    atlases: &[TextureAtlas],
    grass: LinearRgba,
    foliage: LinearRgba,
    water: LinearRgba,
    fluid_anim: Vec4,
) -> AtlasKey {
    let mut key = [0u32; 32];
    key[0] = (revision & 0xFFFF_FFFF) as u32;
    key[1] = (revision >> 32) as u32;
    key[2] = grass.red.to_bits();
    key[3] = grass.green.to_bits();
    key[4] = grass.blue.to_bits();
    key[5] = foliage.red.to_bits();
    key[6] = foliage.green.to_bits();
    key[7] = foliage.blue.to_bits();
    key[8] = water.red.to_bits();
    key[9] = water.green.to_bits();
    key[10] = water.blue.to_bits();
    key[11] = fluid_anim.x.to_bits();
    key[12] = fluid_anim.y.to_bits();
    key[13] = fluid_anim.z.to_bits();
    for (i, page) in atlases.iter().take(MAX_ATLAS_PAGES).enumerate() {
        key[14 + i * 2] = page.width;
        key[15 + i * 2] = page.height;
    }
    key
}

pub struct VoxelRenderPlugin;

/// Ordering anchor for systems that must run before chunk mesh materials are built.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SyncChunkMeshesSet;

impl Plugin for VoxelRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((VoxelMaterialPlugin, WaterMaterialPlugin))
            .init_resource::<MeshCacheResource>()
            .configure_sets(Update, SyncChunkMeshesSet)
            .add_systems(Update, sync_chunk_meshes.in_set(SyncChunkMeshesSet));
    }
}

fn block_atlas_image(atlas: &TextureAtlas) -> Image {
    atlas_pixels_to_image(atlas)
}

pub fn sync_chunk_meshes(
    mut commands: Commands,
    mut cache: ResMut<MeshCacheResource>,
    atlas: Option<Res<BlockAtlasResource>>,
    reflection: Option<Res<SceneReflectionImages>>,
    graphics: Option<Res<crate::graphics_settings::GraphicsSettings>>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut voxel_materials: ResMut<Assets<VoxelMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: Local<ChunkMaterialCache>,
    mut atlas_image_handles: Local<Vec<Handle<Image>>>,
    mut last_atlas_key: Local<Option<AtlasKey>>,
    existing: Query<(Entity, &ChunkEntityMarker)>,
) {
    let Some(atlas_res) = atlas else {
        return;
    };

    let grass = atlas_res.grass_tint.to_linear();
    let foliage = atlas_res.foliage_tint.to_linear();
    let water = atlas_res.water_tint.to_linear();
    let fluid_time = time.elapsed_secs();
    let mut fluid_anim = atlas_res.fluid_anim;
    fluid_anim.w = fluid_time;
    let atlas_key = atlas_key_from(
        atlas_res.revision,
        &atlas_res.atlases,
        grass,
        foliage,
        water,
        fluid_anim,
    );

    let atlas_changed = *last_atlas_key != Some(atlas_key);
    if atlas_changed {
        *materials = ChunkMaterialCache::default();
        *last_atlas_key = Some(atlas_key);
        let stale = cache.0.clear_meshes();
        if !stale.is_empty() {
            commands.insert_resource(PendingMeshRebuild { chunks: stale });
        }
    }

    if let Some(handle) = materials.opaque.as_ref() {
        if let Some(mut mat) = voxel_materials.get_mut(handle) {
            mat.fluid_anim.w = fluid_time;
        }
    }
    if let Some(handle) = materials.blend.as_ref() {
        if let Some(mut mat) = voxel_materials.get_mut(handle) {
            mat.fluid_anim.w = fluid_time;
        }
    }
    if let Some(handle) = materials.water.as_ref() {
        if let Some(mut mat) = water_materials.get_mut(handle) {
            mat.fluid_anim.w = fluid_time;
        }
    }
    if let Some(handle) = materials.cutout.as_ref() {
        if let Some(mut mat) = voxel_materials.get_mut(handle) {
            mat.fluid_anim.w = fluid_time;
        }
    }

    let dirty = cache.0.take_dirty();
    if dirty.is_empty() {
        return;
    }

    let image_handles = if atlas_changed || atlas_image_handles.is_empty() {
        let mut handles = Vec::with_capacity(MAX_ATLAS_PAGES);
        for page in atlas_res.atlases.iter().take(MAX_ATLAS_PAGES) {
            handles.push(images.add(block_atlas_image(page)));
        }
        while handles.len() < MAX_ATLAS_PAGES {
            let fallback = handles
                .first()
                .cloned()
                .unwrap_or_else(|| images.add(Image::default()));
            handles.push(fallback);
        }
        *atlas_image_handles = handles;
        atlas_image_handles.clone()
    } else {
        atlas_image_handles.clone()
    };

    let base_tints = || VoxelMaterial {
        atlas0: image_handles[0].clone(),
        atlas1: image_handles[1].clone(),
        atlas2: image_handles[2].clone(),
        atlas3: image_handles[3].clone(),
        atlas4: image_handles[4].clone(),
        atlas5: image_handles[5].clone(),
        atlas6: image_handles[6].clone(),
        atlas7: image_handles[7].clone(),
        grass_tint: grass,
        foliage_tint: foliage,
        power_tint_dark: LinearRgba::new(0.4, 0.0, 0.0, 1.0),
        power_tint_bright: LinearRgba::new(1.0, 0.0, 0.0, 1.0),
        material_flags: Vec4::ZERO,
        water_tint: water,
        fluid_anim,
        scene_lighting: default(),
        alpha_mode: AlphaMode::Opaque,
    };

    if materials.opaque.is_none() {
        materials.opaque = Some(voxel_materials.add(base_tints()));
    }
    if materials.blend.is_none() {
        materials.blend = Some(voxel_materials.add(VoxelMaterial {
            alpha_mode: AlphaMode::Blend,
            ..base_tints()
        }));
    }
    let reflection_state = reflection.as_deref().cloned().unwrap_or_default();
    let reflection_tier = graphics
        .as_ref()
        .map(|g| g.reflection_tier_index())
        .unwrap_or(1.0);
    let has_planar = graphics.as_ref().is_some_and(|g| {
        matches!(
            g.reflection_tier,
            crate::graphics_settings::ReflectionTier::Planar
        )
    });
    let reflection_params = WaterMaterial::reflection_params(reflection_tier, has_planar);
    if materials.water.is_none() {
        materials.water = Some(water_materials.add(WaterMaterial::with_reflection(
            WaterMaterial {
                water_atlas: image_handles[0].clone(),
                water_tint: water,
                fluid_anim,
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            &reflection_state,
            reflection_params,
        )));
    }
    if materials.cutout.is_none() {
        materials.cutout = Some(voxel_materials.add(VoxelMaterial {
            material_flags: Vec4::new(1.0, 0.0, 0.0, 0.0),
            alpha_mode: AlphaMode::Mask(0.5),
            ..base_tints()
        }));
    }

    let opaque_handle = materials.opaque.as_ref().unwrap().clone();
    let blend_handle = materials.blend.as_ref().unwrap().clone();
    let water_handle = materials.water.as_ref().unwrap().clone();
    let cutout_handle = materials.cutout.as_ref().unwrap().clone();

    for pos in &dirty {
        let Some(mesh) = cache.0.get(*pos) else {
            for (entity, chunk) in &existing {
                if chunk.pos == *pos {
                    commands.entity(entity).despawn();
                }
            }
            continue;
        };

        sync_bucket(
            &mut commands,
            &mut meshes,
            &existing,
            *pos,
            0,
            bucket_has_vertices(&mesh, 0),
            &mesh,
            opaque_handle.clone(),
        );
        sync_bucket(
            &mut commands,
            &mut meshes,
            &existing,
            *pos,
            1,
            bucket_has_vertices(&mesh, 1),
            &mesh,
            blend_handle.clone(),
        );
        sync_bucket(
            &mut commands,
            &mut meshes,
            &existing,
            *pos,
            2,
            bucket_has_vertices(&mesh, 2),
            &mesh,
            cutout_handle.clone(),
        );
        sync_water_bucket(
            &mut commands,
            &mut meshes,
            &existing,
            *pos,
            bucket_has_vertices(&mesh, 3),
            &mesh,
            water_handle.clone(),
        );
    }
}

fn bucket_has_vertices(mesh: &ChunkMesh, bucket: u8) -> bool {
    match bucket {
        1 => !mesh.transparent_vertices.is_empty(),
        2 => !mesh.cutout_vertices.is_empty(),
        3 => !mesh.water_vertices.is_empty(),
        _ => !mesh.opaque_vertices.is_empty(),
    }
}

fn sync_water_bucket(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    existing: &Query<(Entity, &ChunkEntityMarker)>,
    pos: ChunkPos,
    has_vertices: bool,
    mesh: &ChunkMesh,
    mat: Handle<WaterMaterial>,
) {
    if !has_vertices {
        despawn_bucket(commands, existing, pos, 3);
        return;
    }
    let mesh_data = chunk_to_mesh(mesh, 3);
    let mesh_handle = meshes.add(mesh_data);
    for (entity, chunk) in existing {
        if chunk.pos == pos && chunk.bucket == 3 {
            commands
                .entity(entity)
                .insert((Mesh3d(mesh_handle.clone()), MeshMaterial3d(mat.clone())));
            return;
        }
    }
    commands.spawn((
        ChunkEntityMarker { pos, bucket: 3 },
        water_chunk_layers(),
        Mesh3d(mesh_handle),
        MeshMaterial3d(mat),
    ));
}

fn sync_bucket(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    existing: &Query<(Entity, &ChunkEntityMarker)>,
    pos: ChunkPos,
    bucket: u8,
    has_vertices: bool,
    mesh: &ChunkMesh,
    mat: Handle<VoxelMaterial>,
) {
    if !has_vertices {
        despawn_bucket(commands, existing, pos, bucket);
        return;
    }

    sync_one(
        commands,
        meshes,
        existing,
        pos,
        bucket,
        chunk_to_mesh(mesh, bucket),
        mat,
    );
}

fn despawn_bucket(
    commands: &mut Commands,
    existing: &Query<(Entity, &ChunkEntityMarker)>,
    pos: ChunkPos,
    bucket: u8,
) {
    for (entity, chunk) in existing {
        if chunk.pos == pos && chunk.bucket == bucket {
            commands.entity(entity).despawn();
            return;
        }
    }
}

fn sync_one(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    existing: &Query<(Entity, &ChunkEntityMarker)>,
    pos: ChunkPos,
    bucket: u8,
    mesh_data: Mesh,
    mat: Handle<VoxelMaterial>,
) {
    let mesh_handle = meshes.add(mesh_data);
    for (entity, chunk) in existing {
        if chunk.pos == pos && chunk.bucket == bucket {
            commands
                .entity(entity)
                .insert((Mesh3d(mesh_handle.clone()), MeshMaterial3d(mat.clone())));
            return;
        }
    }
    commands.spawn((
        ChunkEntityMarker { pos, bucket },
        Mesh3d(mesh_handle),
        MeshMaterial3d(mat),
    ));
}

fn chunk_to_mesh(chunk: &ChunkMesh, bucket: u8) -> Mesh {
    use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};

    let (vertices, indices) = match bucket {
        1 => (&chunk.transparent_vertices, &chunk.transparent_indices),
        2 => (&chunk.cutout_vertices, &chunk.cutout_indices),
        3 => (&chunk.water_vertices, &chunk.water_indices),
        _ => (&chunk.opaque_vertices, &chunk.opaque_indices),
    };

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vertices.iter().map(|v| v.position).collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vertices.iter().map(|v| v.uv).collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_OVERLAY_UV,
        vertices.iter().map(|v| v.overlay_uv).collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_BLOCK_TINT,
        vertices.iter().map(|v| v.tint).collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_OVERLAY_TINT,
        vertices.iter().map(|v| v.overlay_tint).collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_TINT_MUL,
        vertices.iter().map(|v| v.tint_mul).collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_ATLAS_INDEX,
        vertices.iter().map(|v| v.atlas_index).collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_OVERLAY_ATLAS_INDEX,
        vertices
            .iter()
            .map(|v| v.overlay_atlas_index)
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_NORMAL,
        VertexAttributeValues::Uint8(vertices.iter().map(|v| v.normal).collect()),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_LIGHT,
        VertexAttributeValues::Uint8(vertices.iter().map(|v| v.light).collect()),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_AO,
        VertexAttributeValues::Uint8(vertices.iter().map(|v| v.ao).collect()),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_FLAGS,
        VertexAttributeValues::Uint8(vertices.iter().map(|v| v.flags).collect()),
    );
    mesh.insert_indices(Indices::U32(indices.clone()));

    mesh
}

pub fn despawn_chunk_entities(
    commands: &mut Commands,
    query: &Query<Entity, With<ChunkEntityMarker>>,
) {
    for entity in query {
        commands.entity(entity).despawn();
    }
}
