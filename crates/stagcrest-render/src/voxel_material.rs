use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef};
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
    VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode,
};
use bevy::asset::{load_internal_asset, weak_handle, Handle};
use bevy::render::render_resource::Shader;

pub const VOXEL_SHADER_HANDLE: Handle<Shader> =
    weak_handle!("a7c3e891-4f2b-4d1e-9c8a-0123456789ab");

#[derive(Default)]
pub struct VoxelMaterialPlugin;

impl Plugin for VoxelMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            VOXEL_SHADER_HANDLE,
            "../../../assets/shaders/voxel.wgsl",
            Shader::from_wgsl
        );
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct VoxelMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas: Handle<Image>,
    #[uniform(2)]
    pub grass_tint: LinearRgba,
    #[uniform(3)]
    pub foliage_tint: LinearRgba,
    #[uniform(4)]
    pub redstone_tint_dark: LinearRgba,
    #[uniform(5)]
    pub redstone_tint_bright: LinearRgba,
    #[uniform(6)]
    pub alpha_cutout: u32,
    pub alpha_mode: AlphaMode,
}

impl Material for VoxelMaterial {
    fn vertex_shader() -> ShaderRef {
        VOXEL_SHADER_HANDLE.into()
    }

    fn fragment_shader() -> ShaderRef {
        VOXEL_SHADER_HANDLE.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![voxel_vertex_layout()];
        Ok(())
    }
}

pub const ATTRIBUTE_OVERLAY_UV: MeshVertexAttribute =
    MeshVertexAttribute::new("OverlayUv", 988301001, VertexFormat::Float32x2);
pub const ATTRIBUTE_BLOCK_TINT: MeshVertexAttribute =
    MeshVertexAttribute::new("BlockTint", 988301002, VertexFormat::Float32);
pub const ATTRIBUTE_OVERLAY_TINT: MeshVertexAttribute =
    MeshVertexAttribute::new("OverlayTint", 988301003, VertexFormat::Float32);

pub fn voxel_vertex_layout() -> VertexBufferLayout {
    VertexBufferLayout {
        array_stride: std::mem::size_of::<stagcrest_mesh::VoxelVertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: vec![
            VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 12,
                shader_location: 1,
            },
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 20,
                shader_location: 2,
            },
            VertexAttribute {
                format: VertexFormat::Float32,
                offset: 28,
                shader_location: 3,
            },
            VertexAttribute {
                format: VertexFormat::Float32,
                offset: 32,
                shader_location: 4,
            },
        ],
    }
}
