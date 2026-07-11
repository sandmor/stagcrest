//! Screen-space volumetric light shafts toward the sun.

use bevy::asset::{load_internal_asset, uuid_handle, Handle};
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::{
    schedule::{Core3d, Core3dSystems},
    FullscreenShader,
};
use bevy::post_process::bloom::bloom;
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_resource::{
    binding_types::{sampler, texture_2d, uniform_buffer},
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId,
    ColorTargetState, ColorWrites, FragmentState, Operations, PipelineCache,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType, TextureFormat,
    TextureSampleType, UniformBuffer,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::view::ViewTarget;
use bevy::render::{RenderApp, RenderStartup};
use bevy::shader::Shader;

use crate::prepass_depth::prepass_depth_sample_view;
use crate::reflection_camera::{MainViewCamera, PlanarReflectionCamera};
use crate::skybox::SkyMaterial;

const VOLUMETRIC_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("e1f2a3b4-c5d6-7890-abcd-ef1234567890");

pub struct VolumetricLightPlugin;

impl Plugin for VolumetricLightPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            VOLUMETRIC_SHADER_HANDLE,
            "../../../assets/shaders/volumetric_light.wgsl",
            Shader::from_wgsl
        );

        app.add_plugins(ExtractComponentPlugin::<VolumetricLightSettings>::default());

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_systems(RenderStartup, init_volumetric_pipeline)
            .add_systems(
                Core3d,
                volumetric_light_pass
                    .before(bloom)
                    .in_set(Core3dSystems::PostProcess),
            );
    }
}

#[derive(Component, Clone, Copy, Default, ExtractComponent, ShaderType)]
pub struct VolumetricLightSettings {
    /// xy = sun screen position (0..1), w = strength 0..1
    pub sun_screen: Vec4,
}

impl VolumetricLightSettings {
    pub fn disabled() -> Self {
        Self {
            sun_screen: Vec4::ZERO,
        }
    }

    pub fn set(&mut self, sun_screen: Vec2, strength: f32) {
        self.sun_screen = Vec4::new(sun_screen.x, sun_screen.y, 0.0, strength.clamp(0.0, 1.0));
    }
}

#[derive(Resource)]
pub(crate) struct VolumetricPipeline {
    layout: BindGroupLayoutDescriptor,
    screen_sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

fn init_volumetric_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "volumetric_light_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: false }),
                sampler(SamplerBindingType::NonFiltering),
                texture_2d(TextureSampleType::Depth),
                uniform_buffer::<VolumetricLightSettings>(false),
            ),
        ),
    );

    let screen_sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("volumetric_light_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: VOLUMETRIC_SHADER_HANDLE,
            shader_defs: vec![],
            entry_point: Some("fragment".into()),
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        ..default()
    });

    commands.insert_resource(VolumetricPipeline {
        layout,
        screen_sampler,
        pipeline_id,
    });
}

pub(crate) fn volumetric_light_pass(
    view: ViewQuery<
        (
            &ViewTarget,
            &ViewPrepassTextures,
            &VolumetricLightSettings,
        ),
        (With<SkyMaterial>, With<MainViewCamera>, Without<PlanarReflectionCamera>),
    >,
    pipeline: Res<VolumetricPipeline>,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut ctx: RenderContext,
) {
    let (view_target, prepass, vol_settings) = view.into_inner();

    let Some(depth_view) = prepass_depth_sample_view(prepass) else {
        return;
    };

    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(pipeline.pipeline_id) else {
        return;
    };

    let mut uniform = UniformBuffer::from(*vol_settings);
    uniform.write_buffer(&render_device, &render_queue);
    let Some(settings_binding) = uniform.binding() else {
        return;
    };

    let post = view_target.post_process_write();
    let bind_group_layout = pipeline_cache.get_bind_group_layout(&pipeline.layout);
    let bind_group = ctx.render_device().create_bind_group(
        "volumetric_light_bind_group",
        &bind_group_layout,
        &BindGroupEntries::sequential((
            post.source,
            &pipeline.screen_sampler,
            &depth_view,
            settings_binding,
        )),
    );

    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("volumetric_light_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_render_pipeline(render_pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..3, 0..1);
}
