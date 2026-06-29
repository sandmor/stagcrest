#![allow(dead_code)] // encase `ShaderType::check` helpers

use bytemuck::{Pod, Zeroable};
use stagcrest_mod_client::ChunkTintCell;
use stagcrest_protocol::{BlockId, ChunkPos};

pub const MAX_BLOCK_IDS: usize = 4096;
pub const MAX_TEXTURES: usize = 4096;
pub const HALO_SIZE: i32 = 18;
pub const HALO_VOLUME: usize = (HALO_SIZE * HALO_SIZE * HALO_SIZE) as usize;
pub const CHUNK_BLOCKS: usize = 4096;
pub const BIOME_GRID_CELLS: usize = 64;

/// Max instances a single chunk may emit into its scratch region (all buckets).
pub const CHUNK_SCRATCH_CAPACITY: u32 = 8192;

/// wgpu `max_storage_buffer_binding_size` on common backends (2 GiB − 1).
pub const MAX_STORAGE_BUFFER_BINDING: u64 = 2147483647;

/// Largest chunk slot count such that the concatenated scratch SSBO stays bindable.
pub fn max_chunk_store_slots() -> u32 {
    let per_chunk = chunk_scratch_instance_bytes();
    (MAX_STORAGE_BUFFER_BINDING / per_chunk) as u32
}

/// Bytes per chunk in the concatenated blocks SSBO (16 bytes per GpuBlockCell).
pub const CHUNK_BLOCKS_BYTES: u64 = (HALO_VOLUME * 16) as u64;
/// Bytes per chunk in the concatenated power SSBO (one byte per block).
pub const CHUNK_POWER_BYTES: u64 = CHUNK_BLOCKS as u64;
/// Bytes per chunk in the concatenated biome tint SSBO.
pub const CHUNK_TINT_CELLS_BYTES: u64 =
    (BIOME_GRID_CELLS * std::mem::size_of::<GpuChunkTintCell>()) as u64;

/// Per-bucket instance region capacity (in instances) in the partitioned
/// instances SSBO. Cube/cross/wire buckets are shared across every chunk and
/// need huge regions, while per-model buckets are sparse and need very little.
/// Capacities must be multiples of 4 so that `offset * sizeof(VoxelInstance)`
/// stays aligned to the 256-byte dynamic storage-buffer offset requirement.
pub fn bucket_capacity_slots(bucket_id: u32) -> u32 {
    match bucket_id {
        0 => 1 << 21,       // cube opaque (largest by far)
        1 | 2 | 3 => 1 << 19, // cube cutout/blend, cross cutout
        4 => 1 << 18,       // wire cutout
        _ => 1 << 12,       // per-model buckets (sparse)
    }
}

/// Prefix-summed instance region for a single bucket.
#[derive(Clone, Copy, Debug, Default)]
pub struct BucketRegion {
    /// Start slot (in instances) within the partitioned instances buffer.
    pub offset: u32,
    /// Capacity (in instances) of this bucket's region.
    pub capacity: u32,
}

/// Builds the per-bucket region table (offset/capacity in instances) and
/// returns it together with the total slot count across all buckets.
pub fn build_bucket_regions(bucket_slot_count: u32) -> (Vec<BucketRegion>, u64) {
    let mut regions = Vec::with_capacity(bucket_slot_count as usize);
    let mut offset: u64 = 0;
    for id in 0..bucket_slot_count {
        let capacity = bucket_capacity_slots(id);
        regions.push(BucketRegion {
            offset: offset as u32,
            capacity,
        });
        offset += capacity as u64;
    }
    (regions, offset)
}

/// GPU-side bucket region descriptor uploaded to the compute shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct GpuBucketRegion {
    pub offset: u32,
    pub capacity: u32,
}

/// Geometry classification for GPU culling branches.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuGeometryKind {
    Cube = 0,
    Wire = 1,
    Cross = 2,
    Model = 3,
}

/// Render layer matching opaque / cutout / blend passes.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuRenderLayer {
    Opaque = 0,
    Cutout = 1,
    Blend = 2,
}

impl GpuRenderLayer {
    pub const COUNT: usize = 3;

    pub fn from_protocol(layer: stagcrest_protocol::ModelRenderLayer) -> Self {
        match layer {
            stagcrest_protocol::ModelRenderLayer::Opaque => Self::Opaque,
            stagcrest_protocol::ModelRenderLayer::Cutout => Self::Cutout,
            stagcrest_protocol::ModelRenderLayer::Blend => Self::Blend,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct GpuBlockMeta {
    pub geometry_kind: u32,
    pub render_layer: u32,
    pub flags: u32,
    pub model_bucket_base: u32,
    pub texture_top: u32,
    pub texture_bottom: u32,
    pub texture_sides: u32,
    pub texture_overlay_top: u32,
    pub texture_overlay_bottom: u32,
    pub texture_overlay_sides: u32,
    pub tint_kinds: u32,
    pub overlay_tint_kinds: u32,
    pub block_type_id: u32,
}

pub const BLOCK_FLAG_OPAQUE: u32 = 1;
pub const BLOCK_FLAG_SOLID: u32 = 2;
pub const BLOCK_FLAG_FLUID: u32 = 4;
pub const BLOCK_FLAG_TRANSPARENT: u32 = 8;
pub const BLOCK_FLAG_REDSTONE_DUST: u32 = 16;

/// GPU-side biome tint cell (layout matches [`ChunkTintCell`]).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct GpuChunkTintCell {
    pub grass: [f32; 3],
    pub foliage: [f32; 3],
}

impl From<ChunkTintCell> for GpuChunkTintCell {
    fn from(value: ChunkTintCell) -> Self {
        Self {
            grass: value.grass,
            foliage: value.foliage,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct GpuAtlasRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct GpuBlockCell {
    pub id: u32,
    pub state: u32,
    pub variant: u32,
    pub _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct VoxelInstance {
    pub world_pos: [i32; 4],
    pub block_id: u32,
    pub block_state: u32,
    pub tex_indices: [u32; 3],
    pub tint: f32,
    pub overlay_tint: f32,
    pub tint_mul: [f32; 3],
    pub power: u32,
    pub bucket_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct DrawIndexedIndirectArgs {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
    pub first_instance: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct GpuChunkUniform {
    pub chunk_origin: [i32; 4],
    pub air_id: u32,
    pub _pad: [u32; 3],
}

/// Render-world chunk slot descriptor for batched compute / compaction.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct GpuChunkTableEntry {
    pub blocks_offset: u32,
    pub power_offset: u32,
    pub scratch_offset: u32,
    pub tint_cells_offset: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub origin_z: i32,
    pub air_id: u32,
    pub flags: u32,
}

pub const CHUNK_TABLE_VALID: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct EmitUniform {
    pub dispatch_count: u32,
    pub bucket_slot_count: u32,
    pub chunk_scratch_capacity: u32,
    pub _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct CompactUniform {
    pub visible_count: u32,
    pub bucket_slot_count: u32,
    pub chunk_scratch_capacity: u32,
    pub _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct GpuCameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub position: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct GpuChunkSlot {
    pub pos: ChunkPos,
    pub buffer_index: u32,
    pub dirty: bool,
}

#[derive(Clone, Debug)]
pub struct GpuChunkUpload {
    pub pos: ChunkPos,
    pub air: BlockId,
    pub blocks: [GpuBlockCell; HALO_VOLUME],
    pub power: [u8; CHUNK_BLOCKS],
    pub biome: [u8; BIOME_GRID_CELLS],
    pub climate: Option<GpuClimateData>,
    pub tint_cells: [GpuChunkTintCell; BIOME_GRID_CELLS],
}

#[derive(Clone, Copy, Debug)]
pub struct GpuClimateData {
    pub temperature: f32,
    pub downfall: f32,
}

pub fn halo_index(lx: i32, ly: i32, lz: i32) -> usize {
    (lx + 1 + (ly + 1) * HALO_SIZE + (lz + 1) * HALO_SIZE * HALO_SIZE) as usize
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct BucketFinalizeMeta {
    pub layer: u32,
    pub indirect_index: u32,
    /// Region capacity (in instances) used to clamp the emitted instance count.
    pub capacity: u32,
    pub _pad: u32,
}

pub const BUCKET_FINALIZE_INVALID: u32 = 0xFFFF_FFFF;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, bevy::render::render_resource::ShaderType)]
pub struct FinalizeUniform {
    pub bucket_count: u32,
    pub _pad: [u32; 3],
}

pub fn instance_slot_byte_size() -> u64 {
    std::mem::size_of::<VoxelInstance>() as u64
}

pub fn chunk_scratch_instance_bytes() -> u64 {
    CHUNK_SCRATCH_CAPACITY as u64 * instance_slot_byte_size()
}
