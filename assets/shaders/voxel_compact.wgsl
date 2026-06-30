struct VoxelInstance {
    world_pos: vec4<i32>,
    block_id: u32,
    block_state: u32,
    tex_indices: array<u32, 3>,
    tint: f32,
    overlay_tint: f32,
    tint_mul: array<f32, 3>,
    power: u32,
    bucket_id: u32,
}

struct BucketRegion {
    offset: u32,
    capacity: u32,
}

struct CompactParams {
    visible_count: u32,
    bucket_slot_count: u32,
    max_scratch_slot_capacity: u32,
    _pad: u32,
}

@group(0) @binding(0) var<storage, read> chunk_table: array<GpuChunkTableEntry>;
@group(0) @binding(1) var<storage, read> visible_list: array<u32>;
@group(0) @binding(2) var<storage, read> scratch_instances: array<VoxelInstance>;
@group(0) @binding(3) var<storage, read> scratch_counters: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> draw_instances: array<VoxelInstance>;
@group(0) @binding(5) var<storage, read_write> global_counters: array<atomic<u32>>;
@group(0) @binding(6) var<storage, read> bucket_regions: array<BucketRegion>;
@group(0) @binding(7) var<uniform> params: CompactParams;
@group(0) @binding(8) var<storage, read_write> global_overflow: array<atomic<u32>>;
@group(0) @binding(9) var<storage, read> scratch_slot_offsets: array<u32>;
@group(0) @binding(10) var<storage, read> scratch_slot_capacities: array<u32>;

struct GpuChunkTableEntry {
    origin_x: i32,
    origin_y: i32,
    origin_z: i32,
    air_id: u32,
    flags: u32,
}

// Batched compact: workgroup_id.x = visible chunk, workgroup_id.y = instance batch.
@compute @workgroup_size(64, 1, 1)
fn main(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let vis_idx = wid.x;
    if vis_idx >= params.visible_count {
        return;
    }
    let slot = visible_list[vis_idx];
    let cap = scratch_slot_capacities[slot];
    let scratch_base = scratch_slot_offsets[slot];
    let inst_idx = wid.y * 64u + lid.x;
    if inst_idx >= cap || inst_idx >= params.max_scratch_slot_capacity {
        return;
    }
    let count = atomicLoad(&scratch_counters[slot]);
    if inst_idx >= count {
        return;
    }
    let inst = scratch_instances[scratch_base + inst_idx];
    let bucket_id = inst.bucket_id;
    if bucket_id >= arrayLength(&bucket_regions) {
        return;
    }
    let region = bucket_regions[bucket_id];
    let draw_slot = atomicAdd(&global_counters[bucket_id], 1u);
    if draw_slot >= region.capacity {
        atomicAdd(&global_overflow[bucket_id], 1u);
        return;
    }
    draw_instances[region.offset + draw_slot] = inst;
}
