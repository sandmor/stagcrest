//! Dedicated water surface material with tiered reflections.

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
use crate::scene_reflection::SceneReflectionImages;
use crate::voxel_material::{ATTRIBUTE_AO, ATTRIBUTE_FLAGS, ATTRIBUTE_LIGHT, ATTRIBUTE_NORMAL};

pub const WATER_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("c8d9e0f1-a2b3-4567-89ab-cdef01234567");

pub const REFLECTION_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("d9e0f1a2-b3c4-5678-9abc-def012345678");

#[derive(Default)]
pub struct WaterMaterialPlugin;

impl Plugin for WaterMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            WATER_SHADER_HANDLE,
            "../../../assets/shaders/water.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default());
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub water_atlas: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub scene_color: Handle<Image>,
    #[texture(4)]
    #[sampler(5)]
    pub planar_color: Handle<Image>,
    #[texture(6)]
    #[sampler(7)]
    pub scene_depth: Handle<Image>,
    #[uniform(8)]
    pub water_tint: LinearRgba,
    #[uniform(9)]
    pub fluid_anim: Vec4,
    #[uniform(10)]
    pub scene_lighting: SceneLightingUniform,
    #[uniform(11)]
    pub reflection_params: Vec4,
    pub alpha_mode: AlphaMode,
}

impl Default for WaterMaterial {
    fn default() -> Self {
        Self {
            water_atlas: Handle::default(),
            scene_color: Handle::default(),
            scene_depth: Handle::default(),
            planar_color: Handle::default(),
            water_tint: LinearRgba::WHITE,
            fluid_anim: Vec4::ZERO,
            scene_lighting: SceneLightingUniform::default(),
            reflection_params: Vec4::new(1.0, 0.0, 1.0, 0.0),
            alpha_mode: AlphaMode::Blend,
        }
    }
}

impl WaterMaterial {
    /// GPU uniform: `.x` = reflection tier index, `.y` = planar enabled, `.z` = resolution scale.
    pub fn reflection_params(tier: f32, has_planar: bool) -> Vec4 {
        Vec4::new(tier, if has_planar { 1.0 } else { 0.0 }, 1.0, 0.0)
    }

    pub fn bind_reflection(
        &mut self,
        reflection: &SceneReflectionImages,
        params: Vec4,
    ) {
        self.scene_color = reflection.scene_color.clone();
        self.scene_depth = reflection.scene_depth.clone();
        self.planar_color = reflection.planar_color.clone();
        self.reflection_params = params;
    }

    pub fn with_reflection(
        mut base: Self,
        reflection: &SceneReflectionImages,
        params: Vec4,
    ) -> Self {
        base.bind_reflection(reflection, params);
        base
    }
}

/// Rebind reflection texture handles when graphics settings or reflection images change.
pub fn sync_water_reflection_bindings(
    graphics: Res<crate::graphics_settings::GraphicsSettings>,
    reflection: Res<SceneReflectionImages>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
) {
    if reflection.scene_color == Handle::default() || reflection.scene_depth == Handle::default() {
        return;
    }
    if !graphics.is_changed() && !reflection.is_changed() {
        return;
    }
    let params = WaterMaterial::reflection_params(
        graphics.reflection_tier_index(),
        reflection.has_planar,
    );
    for (_, mat) in water_materials.iter_mut() {
        mat.bind_reflection(&reflection, params);
    }
}

impl Material for WaterMaterial {
    /// Transparent surfaces are skipped by Bevy's prepass queue; custom prepass
    /// shaders would only produce invalid pipeline variants.
    fn enable_prepass() -> bool {
        false
    }

    fn vertex_shader() -> ShaderRef {
        WATER_SHADER_HANDLE.into()
    }

    fn fragment_shader() -> ShaderRef {
        WATER_SHADER_HANDLE.into()
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
            ATTRIBUTE_NORMAL.at_shader_location(8),
            ATTRIBUTE_LIGHT.at_shader_location(9),
            ATTRIBUTE_AO.at_shader_location(10),
            ATTRIBUTE_FLAGS.at_shader_location(11),
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
