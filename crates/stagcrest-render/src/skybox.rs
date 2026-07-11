//! Procedural sky rendered after the opaque pass.

use bevy::asset::{load_internal_asset, uuid_handle, Handle};
use bevy::camera::Viewport;
use bevy::core_pipeline::{
    core_3d::{main_opaque_pass_3d, main_transparent_pass_3d, CORE_3D_DEPTH_FORMAT},
    schedule::{Core3d, Core3dSystems},
};
use bevy::prelude::*;
use bevy::render::extract_component::{
    ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
    UniformComponentPlugin,
};
use bevy::render::render_resource::{
    binding_types::uniform_buffer, BindGroupEntries, BindGroupLayoutDescriptor,
    BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites,
    CompareFunction, DepthBiasState, DepthStencilState, FragmentState, MultisampleState,
    PipelineCache, RenderPassDescriptor, RenderPipelineDescriptor,
    ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines,
    StencilFaceState, StencilState, StoreOp, TextureFormat, VertexState,
};
use bevy::render::renderer::{RenderContext, ViewQuery};
use bevy::render::view::{
    ExtractedView, Msaa, ViewDepthTexture, ViewTarget, ViewUniformOffset, ViewUniforms,
};
use bevy::render::{GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems};
use bevy::render::camera::ExtractedCamera;
use bevy::shader::Shader;

use crate::scene_lighting::SceneLightingUniform;
use crate::voxel_material::SCENE_LIGHTING_SHADER_HANDLE;

const SKYBOX_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("e5f6a7b8-c9d0-1234-5678-90abcdef1234");

/// Per-view sky uniforms synced from [`SceneLighting`].
#[derive(Component, Clone, Copy, Default, ExtractComponent, ShaderType)]
pub struct SkyMaterial {
    pub scene_lighting: SceneLightingUniform,
}

pub struct SkyboxPlugin;

impl Plugin for SkyboxPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            SCENE_LIGHTING_SHADER_HANDLE,
            "../../../assets/shaders/scene_lighting.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            SKYBOX_SHADER_HANDLE,
            "../../../assets/shaders/skybox.wgsl",
            Shader::from_wgsl
        );

        app.add_plugins((
            ExtractComponentPlugin::<SkyMaterial>::default(),
            UniformComponentPlugin::<SkyMaterial>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_gpu_resource::<SpecializedRenderPipelines<SkyboxPipeline>>()
            .add_systems(RenderStartup, init_skybox_pipeline)
            .add_systems(
                Render,
                prepare_skybox_pipelines.in_set(RenderSystems::Prepare),
            )
            .add_systems(
                Core3d,
                skybox_pass
                    .after(main_opaque_pass_3d)
                    .before(main_transparent_pass_3d)
                    .in_set(Core3dSystems::MainPass),
            );
    }
}

#[derive(Resource)]
pub(crate) struct SkyboxPipeline {
    layout: BindGroupLayoutDescriptor,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub(crate) struct SkyboxPipelineKey {
    target_format: TextureFormat,
    samples: u32,
    depth_format: TextureFormat,
}

impl SpecializedRenderPipeline for SkyboxPipeline {
    type Key = SkyboxPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("procedural_skybox_pipeline".into()),
            layout: vec![self.layout.clone()],
            vertex: VertexState {
                shader: SKYBOX_SHADER_HANDLE,
                shader_defs: vec![],
                entry_point: Some("vertex".into()),
                buffers: vec![],
            },
            fragment: Some(FragmentState {
                shader: SKYBOX_SHADER_HANDLE,
                shader_defs: vec![],
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState {
                    format: key.target_format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            depth_stencil: Some(DepthStencilState {
                format: key.depth_format,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: MultisampleState {
                count: key.samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            ..default()
        }
    }
}

#[derive(Component)]
pub(crate) struct SkyboxPipelineId(CachedRenderPipelineId);

fn init_skybox_pipeline(mut commands: Commands) {
    let layout = BindGroupLayoutDescriptor::new(
        "procedural_skybox_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (
                uniform_buffer::<bevy::render::view::ViewUniform>(true),
                uniform_buffer::<SkyMaterial>(true),
            ),
        ),
    );

    commands.insert_resource(SkyboxPipeline { layout });
}

fn prepare_skybox_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<SkyboxPipeline>>,
    pipeline: Res<SkyboxPipeline>,
    views: Query<(Entity, &ExtractedView, &Msaa), With<SkyMaterial>>,
) {
    for (entity, view, msaa) in &views {
        let pipeline_id = pipelines.specialize(
            &pipeline_cache,
            &pipeline,
            SkyboxPipelineKey {
                target_format: view.target_format,
                samples: msaa.samples(),
                depth_format: CORE_3D_DEPTH_FORMAT,
            },
        );
        commands.entity(entity).insert(SkyboxPipelineId(pipeline_id));
    }
}

pub fn skybox_pass(
    view: ViewQuery<(
        &ExtractedCamera,
        &ViewTarget,
        &ViewDepthTexture,
        &SkyMaterial,
        &DynamicUniformIndex<SkyMaterial>,
        &ViewUniformOffset,
        &SkyboxPipelineId,
    )>,
    pipeline: Res<SkyboxPipeline>,
    pipeline_cache: Res<PipelineCache>,
    view_uniforms: Res<ViewUniforms>,
    sky_uniforms: Res<ComponentUniforms<SkyMaterial>>,
    mut ctx: RenderContext,
) {
    let (
        camera,
        target,
        depth,
        _sky,
        sky_index,
        view_uniform_offset,
        pipeline_id,
    ) = view.into_inner();

    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(pipeline_id.0) else {
        return;
    };
    let Some(view_binding) = view_uniforms.uniforms.binding() else {
        return;
    };
    let Some(sky_binding) = sky_uniforms.uniforms().binding() else {
        return;
    };

    let bind_group_layout = pipeline_cache.get_bind_group_layout(&pipeline.layout);
    let bind_group = ctx.render_device().create_bind_group(
        "procedural_skybox_bind_group",
        &bind_group_layout,
        &BindGroupEntries::sequential((view_binding.clone(), sky_binding.clone())),
    );

    let color_attachment = target.get_color_attachment();
    let mut render_pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("procedural_skybox_pass"),
        color_attachments: &[Some(color_attachment)],
        depth_stencil_attachment: Some(depth.get_attachment(StoreOp::Store)),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });

    if let Some(viewport) = Viewport::from_viewport_and_override(camera.viewport.as_ref(), None) {
        render_pass.set_camera_viewport(&viewport);
    }

    render_pass.set_render_pipeline(render_pipeline);
    render_pass.set_bind_group(
        0,
        &bind_group,
        &[view_uniform_offset.offset, sky_index.index()],
    );
    render_pass.draw(0..3, 0..1);
}
