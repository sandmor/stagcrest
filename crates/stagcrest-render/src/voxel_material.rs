use bevy::asset::{load_internal_asset, uuid_handle, Handle};
use bevy::mesh::Mesh;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef};
use bevy::render::render_resource::{
    AsBindGroup, Face, FrontFace, PolygonMode, PrimitiveState, RenderPipelineDescriptor,
    SpecializedMeshPipelineError, VertexFormat,
};
use bevy::shader::{Shader, ShaderRef};

use crate::scene_lighting::SceneLightingUniform;

pub const VOXEL_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("a7c3e891-4f2b-4d1e-9c8a-0123456789ab");

pub const VOXEL_PREPASS_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("b8d4f902-5c3c-5e2f-ad9b-123456789abc");

pub const SCENE_LIGHTING_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("d1e2f3a4-b5c6-7890-abcd-ef1234567890");

pub const MAX_ATLAS_PAGES: usize = 8;

#[derive(Default)]
pub struct VoxelMaterialPlugin;

impl Plugin for VoxelMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            SCENE_LIGHTING_SHADER_HANDLE,
            "../../../assets/shaders/scene_lighting.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            VOXEL_SHADER_HANDLE,
            "../../../assets/shaders/voxel.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            VOXEL_PREPASS_SHADER_HANDLE,
            "../../../assets/shaders/voxel_prepass.wgsl",
            Shader::from_wgsl
        );
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct VoxelMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub atlas0: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub atlas1: Handle<Image>,
    #[texture(4)]
    #[sampler(5)]
    pub atlas2: Handle<Image>,
    #[texture(6)]
    #[sampler(7)]
    pub atlas3: Handle<Image>,
    #[texture(8)]
    #[sampler(9)]
    pub atlas4: Handle<Image>,
    #[texture(10)]
    #[sampler(11)]
    pub atlas5: Handle<Image>,
    #[texture(12)]
    #[sampler(13)]
    pub atlas6: Handle<Image>,
    #[texture(14)]
    #[sampler(15)]
    pub atlas7: Handle<Image>,
    #[uniform(16)]
    pub grass_tint: LinearRgba,
    #[uniform(17)]
    pub foliage_tint: LinearRgba,
    #[uniform(18)]
    pub power_tint_dark: LinearRgba,
    #[uniform(19)]
    pub power_tint_bright: LinearRgba,
    #[uniform(20)]
    pub material_flags: Vec4,
    #[uniform(21)]
    pub water_tint: LinearRgba,
    #[uniform(22)]
    pub fluid_anim: Vec4,
    #[uniform(23)]
    pub scene_lighting: SceneLightingUniform,
    pub alpha_mode: AlphaMode,
}

impl VoxelMaterial {
    pub fn with_atlas_handles(mut base: Self, handles: &[Handle<Image>]) -> Self {
        let fallback = handles.first().cloned().unwrap_or(base.atlas0.clone());
        let pick = |i: usize| handles.get(i).cloned().unwrap_or_else(|| fallback.clone());
        base.atlas0 = pick(0);
        base.atlas1 = pick(1);
        base.atlas2 = pick(2);
        base.atlas3 = pick(3);
        base.atlas4 = pick(4);
        base.atlas5 = pick(5);
        base.atlas6 = pick(6);
        base.atlas7 = pick(7);
        base
    }
}

impl Material for VoxelMaterial {
    fn vertex_shader() -> ShaderRef {
        VOXEL_SHADER_HANDLE.into()
    }

    fn fragment_shader() -> ShaderRef {
        VOXEL_SHADER_HANDLE.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        VOXEL_PREPASS_SHADER_HANDLE.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        VOXEL_PREPASS_SHADER_HANDLE.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            ATTRIBUTE_OVERLAY_UV.at_shader_location(2),
            ATTRIBUTE_BLOCK_TINT.at_shader_location(3),
            ATTRIBUTE_OVERLAY_TINT.at_shader_location(4),
            ATTRIBUTE_TINT_MUL.at_shader_location(5),
            ATTRIBUTE_ATLAS_INDEX.at_shader_location(6),
            ATTRIBUTE_OVERLAY_ATLAS_INDEX.at_shader_location(7),
            ATTRIBUTE_NORMAL.at_shader_location(8),
            ATTRIBUTE_LIGHT.at_shader_location(9),
            ATTRIBUTE_AO.at_shader_location(10),
            ATTRIBUTE_FLAGS.at_shader_location(11),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        descriptor.primitive = PrimitiveState {
            cull_mode: Some(Face::Back),
            front_face: FrontFace::Ccw,
            polygon_mode: PolygonMode::Fill,
            ..default()
        };
        Ok(())
    }
}

pub const ATTRIBUTE_OVERLAY_UV: MeshVertexAttribute =
    MeshVertexAttribute::new("OverlayUv", 988301001, VertexFormat::Float32x2);
pub const ATTRIBUTE_BLOCK_TINT: MeshVertexAttribute =
    MeshVertexAttribute::new("BlockTint", 988301002, VertexFormat::Float32);
pub const ATTRIBUTE_OVERLAY_TINT: MeshVertexAttribute =
    MeshVertexAttribute::new("OverlayTint", 988301003, VertexFormat::Float32);
pub const ATTRIBUTE_TINT_MUL: MeshVertexAttribute =
    MeshVertexAttribute::new("TintMul", 988301004, VertexFormat::Float32x3);
pub const ATTRIBUTE_ATLAS_INDEX: MeshVertexAttribute =
    MeshVertexAttribute::new("AtlasIndex", 988301005, VertexFormat::Float32);
pub const ATTRIBUTE_OVERLAY_ATLAS_INDEX: MeshVertexAttribute =
    MeshVertexAttribute::new("OverlayAtlasIndex", 988301006, VertexFormat::Float32);
pub const ATTRIBUTE_NORMAL: MeshVertexAttribute =
    MeshVertexAttribute::new("Normal", 988301007, VertexFormat::Uint8);
pub const ATTRIBUTE_LIGHT: MeshVertexAttribute =
    MeshVertexAttribute::new("Light", 988301008, VertexFormat::Uint8);
pub const ATTRIBUTE_AO: MeshVertexAttribute =
    MeshVertexAttribute::new("Ao", 988301009, VertexFormat::Uint8);
pub const ATTRIBUTE_FLAGS: MeshVertexAttribute =
    MeshVertexAttribute::new("Flags", 988301010, VertexFormat::Uint8);
