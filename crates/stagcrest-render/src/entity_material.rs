//! Material for Bedrock entity models: a single texture lit with the shared
//! `shade_voxel` day/night model so entities tone-match the voxel world.

use bevy::asset::{load_internal_asset, uuid_handle, Handle};
use bevy::mesh::Mesh;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::render::render_resource::{
    AsBindGroup, FrontFace, PolygonMode, PrimitiveState, RenderPipelineDescriptor,
    SpecializedMeshPipelineError,
};
use bevy::shader::{Shader, ShaderRef};

use crate::scene_lighting::SceneLightingUniform;

pub const ENTITY_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("f4a1c2d3-5b6e-4079-9a1b-2c3d4e5f6071");

pub const ENTITY_PREPASS_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("a3b2c1d0-e5f4-4321-abcd-9876543210fe");

#[derive(Default)]
pub struct EntityMaterialPlugin;

impl Plugin for EntityMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            ENTITY_SHADER_HANDLE,
            "../../../assets/shaders/entity.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            ENTITY_PREPASS_SHADER_HANDLE,
            "../../../assets/shaders/entity_prepass.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<EntityMaterial>::default());
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct EntityMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    #[uniform(2)]
    pub scene_lighting: SceneLightingUniform,
    /// `.x` = sky light 0..1, `.y` = block light 0..1, `.z` = alpha cutoff.
    #[uniform(3)]
    pub light: Vec4,
}

impl Material for EntityMaterial {
    fn vertex_shader() -> ShaderRef {
        ENTITY_SHADER_HANDLE.into()
    }

    fn fragment_shader() -> ShaderRef {
        ENTITY_SHADER_HANDLE.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        ENTITY_PREPASS_SHADER_HANDLE.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        ENTITY_PREPASS_SHADER_HANDLE.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(0.5)
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
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(3),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        descriptor.primitive = PrimitiveState {
            cull_mode: None,
            front_face: FrontFace::Ccw,
            polygon_mode: PolygonMode::Fill,
            ..default()
        };
        Ok(())
    }
}
