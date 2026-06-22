use bevy::asset::{load_internal_asset, weak_handle, Handle};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::mesh::{Mesh, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, Shader, ShaderRef, SpecializedMeshPipelineError,
};
use bevy::render::view::visibility::NoFrustumCulling;
use stagcrest_mesh::SelectionBounds;
use stagcrest_protocol::BlockPos;

pub const OUTLINE_SHADER_HANDLE: Handle<Shader> =
    weak_handle!("b8d4f012-5c6e-4a2f-9d1b-0123456789cd");

#[derive(Default)]
pub struct OutlineMaterialPlugin;

impl Plugin for OutlineMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            OUTLINE_SHADER_HANDLE,
            "../../../assets/shaders/outline.wgsl",
            Shader::from_wgsl
        );
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct OutlineMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
}

impl Material for OutlineMaterial {
    fn fragment_shader() -> ShaderRef {
        OUTLINE_SHADER_HANDLE.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = false;
        }
        Ok(())
    }
}

/// Build a `LineList` mesh for the 12 edges of an axis-aligned box in world space.
pub fn block_outline_mesh(bounds: SelectionBounds, block_pos: BlockPos, inflate: f32) -> Mesh {
    let ox = block_pos.x as f32;
    let oy = block_pos.y as f32;
    let oz = block_pos.z as f32;

    let min = [
        ox + bounds.min[0] - inflate,
        oy + bounds.min[1] - inflate,
        oz + bounds.min[2] - inflate,
    ];
    let max = [
        ox + bounds.max[0] + inflate,
        oy + bounds.max[1] + inflate,
        oz + bounds.max[2] + inflate,
    ];

    let corners = [
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], min[1], max[2]],
        [min[0], min[1], max[2]],
        [min[0], max[1], min[2]],
        [max[0], max[1], min[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ];

    const EDGES: [[usize; 2]; 12] = [
        [0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4], [0, 4], [1, 5], [2, 6],
        [3, 7],
    ];

    let mut vertices = Vec::with_capacity(24);
    for [a, b] in EDGES {
        vertices.push(corners[a]);
        vertices.push(corners[b]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
    mesh
}

/// Minimal valid line mesh for initial spawn before a target is acquired.
fn placeholder_outline_mesh() -> Mesh {
    block_outline_mesh(SelectionBounds::cube(), BlockPos::new(0, 0, 0), 0.0)
}

#[derive(Component)]
pub struct BlockOutlineMarker;

pub fn spawn_block_outline(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<OutlineMaterial>,
) -> Entity {
    let material = materials.add(OutlineMaterial {
        color: LinearRgba::WHITE,
    });
    let mesh = meshes.add(placeholder_outline_mesh());
    commands
        .spawn((
            BlockOutlineMarker,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            NoFrustumCulling,
            Visibility::Hidden,
        ))
        .id()
}
