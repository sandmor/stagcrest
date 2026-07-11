//! Copies the main HDR color buffer and depth for SSR / water reflections.

use bevy::asset::RenderAssetUsages;
use bevy::asset::{load_internal_asset, uuid_handle, Handle};
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::{
    core_3d::main_transparent_pass_3d,
    schedule::{Core3d, Core3dSystems},
    FullscreenShader,
};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    binding_types::texture_2d, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
    CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FragmentState, LoadOp,
    Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, ShaderStages, ShaderType, StoreOp, TextureDimension, TextureFormat,
    TextureSampleType, TextureUsages,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::view::ViewTarget;
use bevy::render::{RenderApp, RenderStartup};
use bevy::shader::Shader;
use bevy::window::PrimaryWindow;

use crate::graphics_settings::ReflectionTier;
use crate::prepass_depth::prepass_depth_sample_view;
use crate::reflection_camera::{MainViewCamera, PlanarReflectionCamera};
use crate::skybox::{skybox_pass, SkyMaterial};

const SCENE_COPY_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("b2c3d4e5-f6a7-8901-bcde-f12345678901");

const SCENE_DEPTH_COPY_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("c3d4e5f6-a7b8-9012-cdef-234567890123");

/// GPU image handles mirrored on the main world for water materials.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct SceneReflectionImages {
    pub scene_color: Handle<Image>,
    pub scene_depth: Handle<Image>,
    pub planar_color: Handle<Image>,
    pub tier: ReflectionTier,
    pub has_planar: bool,
}

#[derive(Component, Clone, Copy, Default, ShaderType)]
pub struct SceneReflectionParams {
    pub params: Vec4,
}

impl SceneReflectionParams {
    pub fn new(tier: ReflectionTier, has_planar: bool, time: f32) -> Self {
        Self {
            params: Vec4::new(
                tier_index(tier),
                if has_planar { 1.0 } else { 0.0 },
                1.0,
                time,
            ),
        }
    }
}

fn tier_index(tier: ReflectionTier) -> f32 {
    match tier {
        ReflectionTier::SkyOnly => 0.0,
        ReflectionTier::Ssr => 1.0,
        ReflectionTier::Planar => 2.0,
    }
}

pub struct SceneReflectionPlugin;

impl Plugin for SceneReflectionPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            SCENE_COPY_SHADER_HANDLE,
            "../../../assets/shaders/scene_copy.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            SCENE_DEPTH_COPY_SHADER_HANDLE,
            "../../../assets/shaders/scene_depth_copy.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            crate::water_material::REFLECTION_SHADER_HANDLE,
            "../../../assets/shaders/reflection.wgsl",
            Shader::from_wgsl
        );

        app.init_resource::<SceneReflectionImages>()
            .add_plugins(ExtractResourcePlugin::<SceneReflectionImages>::default());

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.init_resource::<SceneReflectionImages>();

        render_app
            .init_resource::<SceneReflectionPipeline>()
            .add_systems(RenderStartup, init_scene_reflection_pipelines)
            .add_systems(
                Core3d,
                scene_reflection_copy_pass
                    .after(skybox_pass)
                    .before(main_transparent_pass_3d)
                    .in_set(Core3dSystems::MainPass),
            );
    }
}

#[derive(Resource, Default)]
struct SceneReflectionPipeline {
    color_layout: Option<BindGroupLayoutDescriptor>,
    depth_layout: Option<BindGroupLayoutDescriptor>,
    color_pipeline_id: Option<CachedRenderPipelineId>,
    depth_pipeline_id: Option<CachedRenderPipelineId>,
}

fn init_scene_reflection_pipelines(
    mut pipeline: ResMut<SceneReflectionPipeline>,
    _render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    fullscreen_shader: Res<FullscreenShader>,
) {
    let color_layout = BindGroupLayoutDescriptor::new(
        "scene_copy_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (texture_2d(TextureSampleType::Float { filterable: false }),),
        ),
    );

    let depth_layout = BindGroupLayoutDescriptor::new(
        "scene_depth_copy_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (texture_2d(TextureSampleType::Depth),),
        ),
    );

    let color_pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("scene_color_copy_pipeline".into()),
        layout: vec![color_layout.clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: SCENE_COPY_SHADER_HANDLE,
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

    let depth_pipeline_id = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("scene_depth_copy_pipeline".into()),
        layout: vec![depth_layout.clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: SCENE_DEPTH_COPY_SHADER_HANDLE,
            shader_defs: vec![],
            entry_point: Some("fragment".into()),
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::R32Float,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        ..default()
    });

    pipeline.color_layout = Some(color_layout);
    pipeline.depth_layout = Some(depth_layout);
    pipeline.color_pipeline_id = Some(color_pipeline_id);
    pipeline.depth_pipeline_id = Some(depth_pipeline_id);
}

fn scene_reflection_copy_pass(
    view: ViewQuery<
        (&ViewTarget, &ViewPrepassTextures, &SkyMaterial),
        (With<SkyMaterial>, With<MainViewCamera>, Without<PlanarReflectionCamera>),
    >,
    reflection: Res<SceneReflectionImages>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    pipeline: Res<SceneReflectionPipeline>,
    pipeline_cache: Res<PipelineCache>,
    mut ctx: RenderContext,
) {
    let (target, prepass, _sky) = view.into_inner();

    let Some(color_pipeline_id) = pipeline.color_pipeline_id else {
        return;
    };
    let Some(depth_pipeline_id) = pipeline.depth_pipeline_id else {
        return;
    };
    let Some(color_layout) = pipeline.color_layout.as_ref() else {
        return;
    };
    let Some(depth_layout) = pipeline.depth_layout.as_ref() else {
        return;
    };
    let Some(color_gpu) = gpu_images.get(&reflection.scene_color) else {
        return;
    };
    let Some(depth_gpu) = gpu_images.get(&reflection.scene_depth) else {
        return;
    };
    let Some(color_pipeline) = pipeline_cache.get_render_pipeline(color_pipeline_id) else {
        return;
    };
    let Some(depth_pipeline) = pipeline_cache.get_render_pipeline(depth_pipeline_id) else {
        return;
    };

    let source = target.main_texture_view();

    let color_bind_group_layout = pipeline_cache.get_bind_group_layout(color_layout);
    let color_bind_group = ctx.render_device().create_bind_group(
        "scene_copy_bind_group",
        &color_bind_group_layout,
        &BindGroupEntries::single(source),
    );

    let mut color_pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("scene_color_copy"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &color_gpu.texture_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(LinearRgba::BLACK.into()),
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    color_pass.set_render_pipeline(color_pipeline);
    color_pass.set_bind_group(0, &color_bind_group, &[]);
    color_pass.draw(0..3, 0..1);
    drop(color_pass);

    let Some(depth_view) = prepass_depth_sample_view(prepass) else {
        return;
    };

    let depth_bind_group_layout = pipeline_cache.get_bind_group_layout(depth_layout);
    let depth_bind_group = ctx.render_device().create_bind_group(
        "scene_depth_copy_bind_group",
        &depth_bind_group_layout,
        &BindGroupEntries::sequential((&depth_view,)),
    );

    let mut depth_pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("scene_depth_copy"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &depth_gpu.texture_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(LinearRgba::BLACK.into()),
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    depth_pass.set_render_pipeline(depth_pipeline);
    depth_pass.set_bind_group(0, &depth_bind_group, &[]);
    depth_pass.draw(0..3, 0..1);
}

const REFLECTION_TEXTURE_USAGE: TextureUsages = TextureUsages::TEXTURE_BINDING
    .union(TextureUsages::COPY_DST)
    .union(TextureUsages::RENDER_ATTACHMENT);

const REFLECTION_IMAGE_USAGE: RenderAssetUsages = RenderAssetUsages::all();

fn reflection_color_image(width: u32, height: u32) -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba16Float,
        REFLECTION_IMAGE_USAGE,
    );
    image.texture_descriptor.usage = REFLECTION_TEXTURE_USAGE;
    image.sampler = ImageSampler::linear();
    image
}

fn reflection_depth_image(width: u32, height: u32) -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::R32Float,
        REFLECTION_IMAGE_USAGE,
    );
    image.texture_descriptor.usage = REFLECTION_TEXTURE_USAGE;
    image.sampler = ImageSampler::linear();
    image
}

fn resize_reflection_image(
    images: &mut Assets<Image>,
    handle: &Handle<Image>,
    width: u32,
    height: u32,
    format: TextureFormat,
) {
    let needs_resize = images
        .get(handle)
        .is_none_or(|image| image.width() != width || image.height() != height);
    if !needs_resize {
        return;
    }

    let replacement = if format == TextureFormat::R32Float {
        reflection_depth_image(width, height)
    } else {
        reflection_color_image(width, height)
    };
    if let Some(mut image) = images.get_mut(handle) {
        *image = replacement;
    }
}

/// Ensure reflection image assets exist on the main world.
pub fn ensure_scene_reflection_images(
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<SceneReflectionImages>,
    settings: Res<crate::graphics_settings::GraphicsSettings>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    state.tier = settings.reflection_tier;
    state.has_planar = matches!(settings.reflection_tier, ReflectionTier::Planar);

    let Ok(window) = windows.single() else {
        return;
    };
    let width = window.physical_width().max(1);
    let height = window.physical_height().max(1);

    if state.scene_color == Handle::default() {
        state.scene_color = images.add(reflection_color_image(width, height));
    } else {
        resize_reflection_image(
            &mut images,
            &state.scene_color,
            width,
            height,
            TextureFormat::Rgba16Float,
        );
    }
    if state.scene_depth == Handle::default() {
        state.scene_depth = images.add(reflection_depth_image(width, height));
    } else {
        resize_reflection_image(
            &mut images,
            &state.scene_depth,
            width,
            height,
            TextureFormat::R32Float,
        );
    }
    if state.planar_color == Handle::default() {
        state.planar_color = images.add(reflection_color_image(
            (width / 2).max(1),
            (height / 2).max(1),
        ));
    } else if state.has_planar {
        resize_reflection_image(
            &mut images,
            &state.planar_color,
            (width / 2).max(1),
            (height / 2).max(1),
            TextureFormat::Rgba16Float,
        );
    }
}
