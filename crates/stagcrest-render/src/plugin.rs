use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use stagcrest_mesh::{ChunkMesh, MeshCache};
use stagcrest_mod_host::TextureAtlas;

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

#[derive(Component)]
pub struct ChunkEntityMarker {
    pub pos: stagcrest_protocol::ChunkPos,
    pub transparent: bool,
}

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
    cache: Res<MeshCacheResource>,
    atlas: Option<Res<BlockAtlasResource>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VoxelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut opaque_mat: Local<Option<Handle<VoxelMaterial>>>,
    mut trans_mat: Local<Option<Handle<VoxelMaterial>>>,
    mut last_atlas_key: Local<Option<(u32, u32, u32, u32, u32, u32)>>,
    existing: Query<(Entity, &ChunkEntityMarker)>,
) {
    let Some(atlas_res) = atlas else { return };

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
    if *last_atlas_key != Some(atlas_key) {
        *opaque_mat = None;
        *trans_mat = None;
        *last_atlas_key = Some(atlas_key);
    }

    let image_handle = {
        let image = block_atlas_image(&atlas_res.atlas);
        images.add(image)
    };

    let opaque_handle = opaque_mat.get_or_insert_with(|| {
        materials.add(VoxelMaterial {
            atlas: image_handle.clone(),
            grass_tint: grass,
            foliage_tint: foliage,
            alpha_mode: AlphaMode::Opaque,
        })
    });

    let trans_handle = trans_mat.get_or_insert_with(|| {
        materials.add(VoxelMaterial {
            atlas: image_handle.clone(),
            grass_tint: grass,
            foliage_tint: foliage,
            alpha_mode: AlphaMode::Blend,
        })
    });

    let mut live: std::collections::HashSet<(stagcrest_protocol::ChunkPos, bool)> =
        std::collections::HashSet::new();

    for (pos, mesh) in cache.0.meshes() {
        if !mesh.opaque_vertices.is_empty() {
            live.insert((*pos, false));
            sync_one(
                &mut commands,
                &mut meshes,
                &existing,
                *pos,
                false,
                chunk_to_mesh(mesh, false),
                opaque_handle.clone(),
            );
        }
        if !mesh.transparent_vertices.is_empty() {
            live.insert((*pos, true));
            sync_one(
                &mut commands,
                &mut meshes,
                &existing,
                *pos,
                true,
                chunk_to_mesh(mesh, true),
                trans_handle.clone(),
            );
        }
    }

    for (entity, chunk) in &existing {
        if !live.contains(&(chunk.pos, chunk.transparent)) {
            commands.entity(entity).despawn();
        }
    }
}

fn sync_one(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    existing: &Query<(Entity, &ChunkEntityMarker)>,
    pos: stagcrest_protocol::ChunkPos,
    transparent: bool,
    mesh_data: Mesh,
    mat: Handle<VoxelMaterial>,
) {
    let mesh_handle = meshes.add(mesh_data);
    for (entity, chunk) in existing {
        if chunk.pos == pos && chunk.transparent == transparent {
            commands.entity(entity).insert((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(mat.clone()),
            ));
            return;
        }
    }
    commands.spawn((
        ChunkEntityMarker { pos, transparent },
        Mesh3d(mesh_handle),
        MeshMaterial3d(mat),
    ));
}

fn chunk_to_mesh(chunk: &ChunkMesh, transparent: bool) -> Mesh {
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;

    let vertices = if transparent {
        &chunk.transparent_vertices
    } else {
        &chunk.opaque_vertices
    };
    let indices = if transparent {
        &chunk.transparent_indices
    } else {
        &chunk.opaque_indices
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
