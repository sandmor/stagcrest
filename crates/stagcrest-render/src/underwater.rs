//! Fullscreen underwater post-process pass.

use bevy::asset::{load_internal_asset, uuid_handle, Handle};
use bevy::core_pipeline::{
    schedule::{Core3d, Core3dSystems},
    tonemapping::tonemapping,
    upscaling::upscaling,
    FullscreenShader,
};
use bevy::prelude::*;
use bevy::render::extract_component::{
    ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
    UniformComponentPlugin,
};
use bevy::render::render_resource::{
    binding_types::{sampler, texture_2d, uniform_buffer},
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId,
    ColorTargetState, ColorWrites, FragmentState, Operations, PipelineCache,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
    Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType, TextureFormat,
    TextureSampleType,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::ViewTarget;
use bevy::render::{RenderApp, RenderStartup};
use bevy::shader::Shader;

const UNDERWATER_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("c4e8f2a1-9b3d-4e7c-a1f2-8d6e5c4b3a21");

pub struct UnderwaterPlugin;

impl Plugin for UnderwaterPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            UNDERWATER_SHADER_HANDLE,
            "../../../assets/shaders/underwater.wgsl",
            Shader::from_wgsl
        );

        app.add_plugins((
            ExtractComponentPlugin::<UnderwaterEffect>::default(),
            UniformComponentPlugin::<UnderwaterEffect>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_systems(RenderStartup, init_underwater_pipeline)
            .add_systems(
                Core3d,
                underwater_post_process
                    .after(tonemapping)
                    .before(upscaling)
                    .in_set(Core3dSystems::PostProcess),
            );
    }
}

pub fn underwater_post_process(
    view: ViewQuery<(
        &ViewTarget,
        &UnderwaterEffect,
        &DynamicUniformIndex<UnderwaterEffect>,
    )>,
    pipeline: Res<UnderwaterPipeline>,
    pipeline_cache: Res<PipelineCache>,
    settings_uniforms: Res<ComponentUniforms<UnderwaterEffect>>,
    mut ctx: RenderContext,
) {
    let (view_target, _effect, settings_index) = view.into_inner();

    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(pipeline.pipeline_id) else {
        return;
    };

    let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
        return;
    };

    let post_process = view_target.post_process_write();
    let bind_group_layout = pipeline_cache.get_bind_group_layout(&pipeline.layout);

    let bind_group = ctx.render_device().create_bind_group(
        "underwater_bind_group",
        &bind_group_layout,
        &BindGroupEntries::sequential((
            post_process.source,
            &pipeline.sampler,
            settings_binding.clone(),
        )),
    );

    let mut render_pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("underwater_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post_process.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });

    render_pass.set_render_pipeline(render_pipeline);
    render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
    render_pass.draw(0..3, 0..1);
}

#[derive(Resource)]
pub(crate) struct UnderwaterPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

fn init_underwater_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "underwater_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<UnderwaterEffect>(true),
            ),
        ),
    );

    let sampler = render_device.create_sampler(&SamplerDescriptor::default());

    let pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("underwater_pipeline".into()),
        layout: vec![layout.clone()],
        immediate_size: 0,
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: UNDERWATER_SHADER_HANDLE,
            shader_defs: vec![],
            entry_point: Some("fragment".into()),
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        ..default()
    });

    commands.insert_resource(UnderwaterPipeline {
        layout,
        sampler,
        pipeline_id,
    });
}

/// Fullscreen underwater tint. xyz = water tint, w = strength (0..1).
#[derive(Component, Clone, Copy, Default, ExtractComponent, ShaderType)]
pub struct UnderwaterEffect {
    pub tint_strength: Vec4,
}

impl UnderwaterEffect {
    pub fn set(&mut self, tint: [f32; 3], strength: f32) {
        self.tint_strength = Vec4::new(tint[0], tint[1], tint[2], strength.clamp(0.0, 1.0));
    }
}
