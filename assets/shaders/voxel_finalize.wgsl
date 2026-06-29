struct DrawIndexedIndirectArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

struct BucketFinalizeMeta {
    layer: u32,
    indirect_index: u32,
    capacity: u32,
    _pad1: u32,
}

struct FinalizeUniform {
    bucket_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<storage, read> counters: array<atomic<u32>>;
@group(0) @binding(1) var<storage, read_write> indirect_opaque: array<DrawIndexedIndirectArgs>;
@group(0) @binding(2) var<storage, read_write> indirect_cutout: array<DrawIndexedIndirectArgs>;
@group(0) @binding(3) var<storage, read_write> indirect_blend: array<DrawIndexedIndirectArgs>;
@group(0) @binding(4) var<storage, read> bucket_meta: array<BucketFinalizeMeta>;
@group(0) @binding(5) var<uniform> params: FinalizeUniform;

const LAYER_OPAQUE: u32 = 0u;
const LAYER_CUTOUT: u32 = 1u;
const LAYER_BLEND: u32 = 2u;

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let bucket_id = gid.x;
    if bucket_id >= params.bucket_count {
        return;
    }
    let bucket = bucket_meta[bucket_id];
    if bucket.indirect_index == 0xFFFFFFFFu {
        return;
    }
    let count = min(atomicLoad(&counters[bucket_id]), bucket.capacity);
    if bucket.layer == LAYER_OPAQUE {
        indirect_opaque[bucket.indirect_index].instance_count = count;
    } else if bucket.layer == LAYER_CUTOUT {
        indirect_cutout[bucket.indirect_index].instance_count = count;
    } else if bucket.layer == LAYER_BLEND {
        indirect_blend[bucket.indirect_index].instance_count = count;
    }
}
