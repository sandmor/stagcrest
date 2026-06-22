use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use stagcrest_mesh::{ChunkMesh, MeshCache};
use stagcrest_mod_host::TextureAtlas;
use stagcrest_protocol::ChunkPos;

use crate::voxel_material::{VoxelMaterial, voxel_vertex_layout};

#[derive(Resource, Default)]
pub struct MeshCacheResource(pub MeshCache);

#[derive(Resource)]
pub struct BlockAtlasResource {
    pub atlas: TextureAtlas,
    pub grass_tint: Color,
    pub foliage_tint: Color,
}

#[derive(Resource, Default)]
pub struct VoxelCamera {
    pub view_proj: glam::Mat4,
    pub position: glam::Vec3,
}

/// Render bucket: 0 = opaque, 1 = blend, 2 = cutout.
#[derive(Component)]
pub struct ChunkEntityMarker {
    pub pos: ChunkPos,
    pub bucket: u8,
}

type AtlasKey = (u32, u32, u32, u32, u32, u32);

pub struct VoxelRenderPlugin;

impl Plugin for VoxelRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MeshCacheResource>()
            .init_resource::<VoxelCamera>()
            .add_systems(Update, sync_chunk_meshes);
    }
}

fn block_atlas_image(atlas: &TextureAtlas) -> Image {
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

fn sync_chunk_meshes(
    mut commands: Commands,
    mut cache: ResMut<MeshCacheResource>,
    atlas: Option<Res<BlockAtlasResource>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VoxelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut opaque_mat: Local<Option<Handle<VoxelMaterial>>>,
    mut blend_mat: Local<Option<Handle<VoxelMaterial>>>,
    mut cutout_mat: Local<Option<Handle<VoxelMaterial>>>,
    mut last_atlas_key: Local<Option<AtlasKey>>,
    mut atlas_image: Local<Option<(AtlasKey, Handle<Image>)>>,
    existing: Query<(Entity, &ChunkEntityMarker)>,
) {
    let Some(atlas_res) = atlas else {
        return;
    };

    let grass = atlas_res.grass_tint.to_linear();
    let foliage = atlas_res.foliage_tint.to_linear();
    let atlas_key = (
        atlas_res.atlas.width,
        atlas_res.atlas.height,
        (grass.red * 255.0) as u32,
        (grass.green * 255.0) as u32,
        (foliage.red * 255.0) as u32,
        (foliage.green * 255.0) as u32,
    );

    let atlas_changed = *last_atlas_key != Some(atlas_key);
    if atlas_changed {
        *opaque_mat = None;
        *blend_mat = None;
        *cutout_mat = None;
        *last_atlas_key = Some(atlas_key);
        cache.0.mark_all_dirty();
    }

    let dirty = cache.0.take_dirty();
    if dirty.is_empty() {
        return;
    }

    let image_handle = match atlas_image.as_ref() {
        Some((key, handle)) if *key == atlas_key => handle.clone(),
        _ => {
            let handle = images.add(block_atlas_image(&atlas_res.atlas));
            *atlas_image = Some((atlas_key, handle.clone()));
            handle
        }
    };

    let base_tints = || VoxelMaterial {
        atlas: image_handle.clone(),
        grass_tint: grass,
        foliage_tint: foliage,
        power_tint_dark: LinearRgba::new(0.4, 0.0, 0.0, 1.0),
        power_tint_bright: LinearRgba::new(1.0, 0.0, 0.0, 1.0),
        alpha_cutout: 0,
        alpha_mode: AlphaMode::Opaque,
    };

    let opaque_handle = opaque_mat.get_or_insert_with(|| materials.add(base_tints()));
    let blend_handle = blend_mat.get_or_insert_with(|| {
        materials.add(VoxelMaterial {
            alpha_mode: AlphaMode::Blend,
            ..base_tints()
        })
    });
    let cutout_handle = cutout_mat.get_or_insert_with(|| {
        materials.add(VoxelMaterial {
            alpha_cutout: 1,
            alpha_mode: AlphaMode::Mask(0.5),
            ..base_tints()
        })
    });

    for pos in dirty {
        let Some(mesh) = cache.0.get(pos) else {
            for (entity, chunk) in &existing {
                if chunk.pos == pos {
                    commands.entity(entity).despawn();
                }
            }
            continue;
        };

        sync_bucket(
            &mut commands,
            &mut meshes,
            &existing,
            pos,
            0,
            bucket_has_vertices(&mesh, 0),
            &mesh,
            opaque_handle.clone(),
        );
        sync_bucket(
            &mut commands,
            &mut meshes,
            &existing,
            pos,
            1,
            bucket_has_vertices(&mesh, 1),
            &mesh,
            blend_handle.clone(),
        );
        sync_bucket(
            &mut commands,
            &mut meshes,
            &existing,
            pos,
            2,
            bucket_has_vertices(&mesh, 2),
            &mesh,
            cutout_handle.clone(),
        );
    }
}

fn bucket_has_vertices(mesh: &ChunkMesh, bucket: u8) -> bool {
    match bucket {
        1 => !mesh.transparent_vertices.is_empty(),
        2 => !mesh.cutout_vertices.is_empty(),
        _ => !mesh.opaque_vertices.is_empty(),
    }
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
            commands.entity(entity).insert((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(mat.clone()),
            ));
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
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;

    let (vertices, indices) = match bucket {
        1 => (
            &chunk.transparent_vertices,
            &chunk.transparent_indices,
        ),
        2 => (&chunk.cutout_vertices, &chunk.cutout_indices),
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
        vertices
            .iter()
            .map(|v| v.overlay_uv)
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_BLOCK_TINT,
        vertices.iter().map(|v| v.tint).collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        crate::voxel_material::ATTRIBUTE_OVERLAY_TINT,
        vertices.iter().map(|v| v.overlay_tint).collect::<Vec<_>>(),
    );
    mesh.insert_indices(Indices::U32(indices.clone()));

    let _ = voxel_vertex_layout();
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
