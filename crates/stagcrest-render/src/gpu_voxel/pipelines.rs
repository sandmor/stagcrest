#![allow(dead_code)] // encase `ShaderType::check` helpers

use bevy::asset::{load_internal_asset, uuid_handle, Handle};
use bevy::mesh::VertexBufferLayout;
use bevy::prelude::*;
use bevy::render::render_resource::{
    binding_types::{sampler, storage_buffer, storage_buffer_read_only, storage_buffer_read_only_sized, texture_2d, uniform_buffer},
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BlendComponent, BlendFactor, BlendOperation,
    BlendState, CachedComputePipelineId, ColorTargetState, ColorWrites,
    ComputePipelineDescriptor, Face, FragmentState, FrontFace, MultisampleState, PipelineCache,
    PolygonMode, PrimitiveState, RenderPipelineDescriptor, SamplerBindingType, SpecializedRenderPipeline,
    ShaderStages, ShaderType, TextureFormat, TextureSampleType, VertexFormat,
    VertexState, VertexStepMode,
};
use bevy::core_pipeline::core_3d::CORE_3D_DEPTH_FORMAT;
use bevy::shader::Shader;
use bevy::render::render_resource::SamplerDescriptor;
use bevy::render::renderer::RenderDevice;
use stagcrest_mesh::GpuMeshVertex;

use crate::gpu_voxel::types::{
    BucketFinalizeMeta, CompactUniform, EmitUniform, FinalizeUniform, GpuAtlasRect, GpuBlockCell,
    GpuBlockMeta, GpuBucketRegion, GpuCameraUniform, GpuChunkTableEntry, GpuChunkTintCell,
    VoxelInstance,
};

pub const VOXEL_COMPUTE_SHADER: Handle<Shader> =
    uuid_handle!("b1c2d3e4-f5a6-4b7c-8d9e-0f1a2b3c4d5e");
pub const VOXEL_INSTANCE_SHADER: Handle<Shader> =
    uuid_handle!("c2d3e4f5-a6b7-4c8d-9e0f-1a2b3c4d5e6f");
pub const VOXEL_FINALIZE_SHADER: Handle<Shader> =
    uuid_handle!("d3e4f5a6-b7c8-4d9e-0f1a-2b3c4d5e6f7a");

pub const VOXEL_COMPACT_SHADER: Handle<Shader> =
    uuid_handle!("e4f5a6b7-c8d9-4e0f-1a2b-3c4d5e6f7a8b");

#[derive(Resource)]
pub struct VoxelComputePipeline {
    pub layout: BindGroupLayout,
    pub pipeline_id: CachedComputePipelineId,
}

#[derive(Resource)]
pub struct VoxelFinalizePipeline {
    pub layout: BindGroupLayout,
    pub pipeline_id: CachedComputePipelineId,
}

#[derive(Resource)]
pub struct VoxelCompactPipeline {
    pub layout: BindGroupLayout,
    pub pipeline_id: CachedComputePipelineId,
}

#[derive(Resource)]
pub struct VoxelDrawPipeline {
    pub layout: BindGroupLayout,
    pub layout_descriptor: BindGroupLayoutDescriptor,
    pub vertex_layout: VertexBufferLayout,
    pub sampler: bevy::render::render_resource::Sampler,
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct VoxelDrawPipelineKey {
    pub target_format: TextureFormat,
    pub samples: u32,
    /// 0 = opaque, 1 = cutout, 2 = blend
    pub layer: u32,
}

impl SpecializedRenderPipeline for VoxelDrawPipeline {
    type Key = VoxelDrawPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let cutout = key.layer == 1;
        let blend = if key.layer == 2 {
            Some(voxel_blend_state())
        } else {
            None
        };
        let label = match key.layer {
            0 => "voxel_opaque",
            1 => "voxel_cutout",
            _ => "voxel_blend",
        };
        build_voxel_pipeline_descriptor(
            label,
            &self.layout_descriptor,
            &self.vertex_layout,
            blend,
            cutout,
            key.samples,
            key.target_format,
        )
    }
}

/// Maps `Msaa::samples()` (1, 2, 4, 8) to a pipeline table index.
pub fn msaa_pipeline_index(samples: u32) -> usize {
    match samples {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        _ => 2,
    }
}

#[derive(ShaderType, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct MaterialUniformsGpu {
    pub grass_tint: Vec4,
    pub foliage_tint: Vec4,
    pub power_tint_dark: Vec4,
    pub power_tint_bright: Vec4,
    pub material_flags: Vec4,
    pub water_tint: Vec4,
    pub fluid_anim: Vec4,
    pub atlas_size: Vec4,
}

pub fn material_uniforms_from(
    source: Option<&crate::plugin::VoxelMaterialSource>,
    cutout: bool,
) -> MaterialUniformsGpu {
    let color_vec4 = |c: Color| {
        let l: bevy::color::LinearRgba = c.into();
        Vec4::new(l.red, l.green, l.blue, l.alpha)
    };
    let (grass, foliage, water, fluid_anim, atlas_w, atlas_h) = source
        .map(|s| {
            (
                s.grass_tint,
                s.foliage_tint,
                s.water_tint,
                s.fluid_anim,
                s.atlas_width,
                s.atlas_height,
            )
        })
        .unwrap_or((Color::WHITE, Color::WHITE, Color::WHITE, Vec4::ZERO, 1.0, 1.0));
    MaterialUniformsGpu {
        grass_tint: color_vec4(grass),
        foliage_tint: color_vec4(foliage),
        water_tint: color_vec4(water),
        power_tint_dark: Vec4::new(0.4, 0.0, 0.0, 1.0),
        power_tint_bright: Vec4::new(1.0, 0.0, 0.0, 1.0),
        material_flags: Vec4::new(if cutout { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0),
        fluid_anim,
        atlas_size: Vec4::new(atlas_w, atlas_h, 0.0, 0.0),
    }
}

pub fn load_voxel_shaders(app: &mut App) {
    load_internal_asset!(
        app,
        VOXEL_COMPUTE_SHADER,
        "../../../../assets/shaders/voxel_compute.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        VOXEL_INSTANCE_SHADER,
        "../../../../assets/shaders/voxel_instance.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        VOXEL_FINALIZE_SHADER,
        "../../../../assets/shaders/voxel_finalize.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        VOXEL_COMPACT_SHADER,
        "../../../../assets/shaders/voxel_compact.wgsl",
        Shader::from_wgsl
    );
}

pub fn voxel_mesh_vertex_layout() -> VertexBufferLayout {
    VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuMeshVertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: vec![
            bevy::render::render_resource::VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            bevy::render::render_resource::VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 12,
                shader_location: 1,
            },
            bevy::render::render_resource::VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 20,
                shader_location: 2,
            },
            bevy::render::render_resource::VertexAttribute {
                format: VertexFormat::Float32,
                offset: 28,
                shader_location: 3,
            },
            bevy::render::render_resource::VertexAttribute {
                format: VertexFormat::Float32,
                offset: 32,
                shader_location: 4,
            },
        ],
    }
}

pub fn prepare_render_pipelines(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    existing_compute: Option<Res<VoxelComputePipeline>>,
    existing_compact: Option<Res<VoxelCompactPipeline>>,
    existing_finalize: Option<Res<VoxelFinalizePipeline>>,
    existing_draw: Option<Res<VoxelDrawPipeline>>,
) {
    if existing_compute.is_none() {
        let visibility = ShaderStages::COMPUTE;
        let entries = [
            storage_buffer_read_only::<GpuBlockMeta>(false).build(0, visibility),
            storage_buffer_read_only::<GpuBlockCell>(false).build(1, visibility),
            storage_buffer_read_only_sized(false, None).build(2, visibility),
            storage_buffer_read_only::<GpuChunkTableEntry>(false).build(3, visibility),
            storage_buffer_read_only::<u32>(false).build(4, visibility),
            storage_buffer::<VoxelInstance>(false).build(5, visibility),
            storage_buffer::<u32>(false).build(6, visibility),
            storage_buffer::<u32>(false).build(7, visibility),
            storage_buffer_read_only::<GpuChunkTintCell>(false).build(8, visibility),
            uniform_buffer::<EmitUniform>(false).build(9, visibility),
        ];
        let layout_descriptor =
            BindGroupLayoutDescriptor::new("voxel_compute_layout", &entries);
        let layout = pipeline_cache.get_bind_group_layout(&layout_descriptor);
        let pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("voxel_compute".into()),
            layout: vec![layout_descriptor],
            shader: VOXEL_COMPUTE_SHADER,
            shader_defs: vec![],
            entry_point: Some("main".into()),
            immediate_size: 0,
            zero_initialize_workgroup_memory: false,
        });
        commands.insert_resource(VoxelComputePipeline { layout, pipeline_id });
    }

    if existing_compact.is_none() {
        let entries = BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                storage_buffer_read_only::<GpuChunkTableEntry>(false),
                storage_buffer_read_only::<u32>(false),
                storage_buffer_read_only::<VoxelInstance>(false),
                storage_buffer_read_only::<u32>(false),
                storage_buffer::<VoxelInstance>(false),
                storage_buffer::<u32>(false),
                storage_buffer_read_only::<GpuBucketRegion>(false),
                uniform_buffer::<CompactUniform>(false),
                storage_buffer::<u32>(false),
            ),
        );
        let layout_descriptor =
            BindGroupLayoutDescriptor::new("voxel_compact_layout", &entries);
        let layout = pipeline_cache.get_bind_group_layout(&layout_descriptor);
        let pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("voxel_compact".into()),
            layout: vec![layout_descriptor],
            shader: VOXEL_COMPACT_SHADER,
            shader_defs: vec![],
            entry_point: Some("main".into()),
            immediate_size: 0,
            zero_initialize_workgroup_memory: false,
        });
        commands.insert_resource(VoxelCompactPipeline { layout, pipeline_id });
    }

    if existing_finalize.is_none() {
        let entries = BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                storage_buffer_read_only::<u32>(false),
                storage_buffer::<DrawIndexedIndirectArgsGpu>(false),
                storage_buffer::<DrawIndexedIndirectArgsGpu>(false),
                storage_buffer::<DrawIndexedIndirectArgsGpu>(false),
                storage_buffer_read_only::<BucketFinalizeMeta>(false),
                uniform_buffer::<FinalizeUniform>(false),
                storage_buffer::<u32>(false),
            ),
        );
        let layout_descriptor =
            BindGroupLayoutDescriptor::new("voxel_finalize_layout", &entries);
        let layout = pipeline_cache.get_bind_group_layout(&layout_descriptor);
        let pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("voxel_finalize".into()),
            layout: vec![layout_descriptor],
            shader: VOXEL_FINALIZE_SHADER,
            shader_defs: vec![],
            entry_point: Some("main".into()),
            immediate_size: 0,
            zero_initialize_workgroup_memory: false,
        });
        commands.insert_resource(VoxelFinalizePipeline { layout, pipeline_id });
    }

    if existing_draw.is_some() {
        return;
    }

    let draw_entries = BindGroupLayoutEntries::sequential(
        ShaderStages::VERTEX | ShaderStages::FRAGMENT,
        (
            uniform_buffer::<GpuCameraUniform>(false),
            storage_buffer_read_only::<VoxelInstance>(false),
            storage_buffer_read_only::<GpuAtlasRect>(false),
            texture_2d(TextureSampleType::Float { filterable: true }),
            sampler(SamplerBindingType::Filtering),
            uniform_buffer::<MaterialUniformsGpu>(false),
            storage_buffer_read_only::<GpuBlockMeta>(false),
        ),
    );
    let draw_layout_descriptor =
        BindGroupLayoutDescriptor::new("voxel_draw_layout", &draw_entries);
    let draw_layout = pipeline_cache.get_bind_group_layout(&draw_layout_descriptor);

    let vertex_layout = voxel_mesh_vertex_layout();

    let sampler = render_device.create_sampler(&SamplerDescriptor {
        label: Some("voxel_atlas_sampler"),
        mag_filter: bevy::render::render_resource::FilterMode::Nearest,
        min_filter: bevy::render::render_resource::FilterMode::Nearest,
        ..Default::default()
    });

    commands.insert_resource(VoxelDrawPipeline {
        layout: draw_layout,
        layout_descriptor: draw_layout_descriptor,
        vertex_layout,
        sampler,
    });
}

fn voxel_blend_state() -> BlendState {
    BlendState {
        color: BlendComponent {
            src_factor: BlendFactor::SrcAlpha,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        },
        alpha: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        },
    }
}

#[derive(ShaderType, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct DrawIndexedIndirectArgsGpu {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

fn build_voxel_pipeline_descriptor(
    label: &'static str,
    draw_layout: &BindGroupLayoutDescriptor,
    vertex_layout: &VertexBufferLayout,
    blend: Option<BlendState>,
    cutout: bool,
    msaa_samples: u32,
    target_format: TextureFormat,
) -> RenderPipelineDescriptor {
    let mut shader_defs = vec![];
    if cutout {
        shader_defs.push("CUTOUT".into());
    }
    RenderPipelineDescriptor {
        label: Some(label.into()),
        layout: vec![draw_layout.clone()],
        immediate_size: 0,
        vertex: VertexState {
            shader: VOXEL_INSTANCE_SHADER,
            shader_defs: shader_defs.clone(),
            entry_point: Some("vertex".into()),
            buffers: vec![vertex_layout.clone()],
        },
        fragment: Some(FragmentState {
            shader: VOXEL_INSTANCE_SHADER,
            shader_defs,
            entry_point: Some("fragment".into()),
            targets: vec![Some(ColorTargetState {
                format: target_format,
                blend,
                write_mask: ColorWrites::ALL,
            })],
        }),
        primitive: PrimitiveState {
            topology: bevy::render::render_resource::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: Some(Face::Back),
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(bevy::render::render_resource::DepthStencilState {
            format: CORE_3D_DEPTH_FORMAT,
            depth_write_enabled: Some(blend.is_none()),
            depth_compare: Some(bevy::render::render_resource::CompareFunction::GreaterEqual),
            stencil: bevy::render::render_resource::StencilState::default(),
            bias: bevy::render::render_resource::DepthBiasState::default(),
        }),
        multisample: MultisampleState {
            count: msaa_samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        zero_initialize_workgroup_memory: false,
    }
}
