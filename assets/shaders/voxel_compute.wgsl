struct GpuBlockMeta {
    geometry_kind: u32,
    render_layer: u32,
    flags: u32,
    model_bucket_base: u32,
    texture_top: u32,
    texture_bottom: u32,
    texture_sides: u32,
    block_type_id: u32,
}

struct GpuBlockCell {
    id: u32,
    state: u32,
    variant: u32,
    _pad: u32,
}

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

struct GpuChunkUniform {
    chunk_origin: vec4<i32>,
    air_id: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

struct GpuCameraUniform {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
}

const BLOCK_FLAG_OPAQUE: u32 = 1u;
const BLOCK_FLAG_SOLID: u32 = 2u;
const BLOCK_FLAG_FLUID: u32 = 4u;
const BLOCK_FLAG_TRANSPARENT: u32 = 8u;
const BLOCK_FLAG_REDSTONE_DUST: u32 = 16u;

const GEO_CUBE: u32 = 0u;
const GEO_WIRE: u32 = 1u;
const GEO_CROSS: u32 = 2u;
const GEO_MODEL: u32 = 3u;

const LAYER_OPAQUE: u32 = 0u;
const LAYER_CUTOUT: u32 = 1u;
const LAYER_BLEND: u32 = 2u;

const BUCKET_CUBE_OPAQUE: u32 = 0u;
const BUCKET_CUBE_CUTOUT: u32 = 1u;
const BUCKET_CUBE_BLEND: u32 = 2u;
const BUCKET_CROSS_CUTOUT: u32 = 3u;
const BUCKET_WIRE_CUTOUT: u32 = 4u;

const TINT_WATER: f32 = 4.5;

const HALO_SIZE: i32 = 18;
const CHUNK_SIZE: i32 = 16;

const WIRE_NONE: u32 = 0u;
const WIRE_SIDE: u32 = 1u;
const WIRE_UP: u32 = 2u;

@group(0) @binding(0) var<uniform> camera: GpuCameraUniform;
@group(0) @binding(1) var<storage, read> block_meta: array<GpuBlockMeta>;
@group(0) @binding(2) var<storage, read> chunk_blocks: array<GpuBlockCell>;
@group(0) @binding(3) var<storage, read> chunk_power: array<u32>;
@group(0) @binding(4) var<uniform> chunk_uniform: GpuChunkUniform;
@group(0) @binding(5) var<storage, read_write> instances: array<VoxelInstance>;
@group(0) @binding(6) var<storage, read_write> counters: array<atomic<u32>>;

struct BucketRegion {
    offset: u32,
    capacity: u32,
}
@group(0) @binding(7) var<storage, read> bucket_regions: array<BucketRegion>;

fn halo_index(lx: i32, ly: i32, lz: i32) -> u32 {
    return u32(lx + 1 + (ly + 1) * HALO_SIZE + (lz + 1) * HALO_SIZE * HALO_SIZE);
}

fn block_at(lx: i32, ly: i32, lz: i32) -> GpuBlockCell {
    return chunk_blocks[halo_index(lx, ly, lz)];
}

fn meta_for(id: u32) -> GpuBlockMeta {
    if id >= arrayLength(&block_meta) {
        return GpuBlockMeta(0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u);
    }
    return block_meta[id];
}

fn power_at(lx: i32, ly: i32, lz: i32) -> u32 {
    if lx < 0 || ly < 0 || lz < 0 || lx >= CHUNK_SIZE || ly >= CHUNK_SIZE || lz >= CHUNK_SIZE {
        return 0u;
    }
    let idx = u32(lx + lz * CHUNK_SIZE + ly * CHUNK_SIZE * CHUNK_SIZE);
    if idx >= arrayLength(&chunk_power) {
        return 0u;
    }
    return chunk_power[idx] & 0xFFu;
}

fn encode_power_tint(power: u32) -> f32 {
    return 3.0 + f32(power) / 15.0;
}

fn is_air(cell: GpuBlockCell) -> bool {
    return cell.id == chunk_uniform.air_id;
}

fn neighbor_culls_face(block_flags: u32, neighbor_id: u32, neighbor_flags: u32) -> bool {
    if neighbor_id == chunk_uniform.air_id {
        return false;
    }
    let block_fluid = (block_flags & BLOCK_FLAG_FLUID) != 0u;
    let neighbor_fluid = (neighbor_flags & BLOCK_FLAG_FLUID) != 0u;
    if block_fluid && neighbor_fluid {
        return true;
    }
    let neighbor_opaque = (neighbor_flags & BLOCK_FLAG_OPAQUE) != 0u;
    let neighbor_solid = (neighbor_flags & BLOCK_FLAG_SOLID) != 0u;
    return neighbor_opaque && neighbor_solid;
}

fn frustum_visible(world_min: vec3<f32>, world_max: vec3<f32>) -> bool {
    let corners = array<vec4<f32>, 8>(
        vec4<f32>(world_min.x, world_min.y, world_min.z, 1.0),
        vec4<f32>(world_max.x, world_min.y, world_min.z, 1.0),
        vec4<f32>(world_min.x, world_max.y, world_min.z, 1.0),
        vec4<f32>(world_max.x, world_max.y, world_min.z, 1.0),
        vec4<f32>(world_min.x, world_min.y, world_max.z, 1.0),
        vec4<f32>(world_max.x, world_min.y, world_max.z, 1.0),
        vec4<f32>(world_min.x, world_max.y, world_max.z, 1.0),
        vec4<f32>(world_max.x, world_max.y, world_max.z, 1.0),
    );
    for (var i = 0; i < 8; i++) {
        let clip = camera.view_proj * corners[i];
        if (clip.x >= -clip.w && clip.x <= clip.w &&
            clip.y >= -clip.w && clip.y <= clip.w &&
            clip.z >= -clip.w && clip.z <= clip.w) {
            return true;
        }
    }
    return false;
}

fn cube_bucket_for_layer(layer: u32) -> u32 {
    if layer == LAYER_CUTOUT { return BUCKET_CUBE_CUTOUT; }
    if layer == LAYER_BLEND { return BUCKET_CUBE_BLEND; }
    return BUCKET_CUBE_OPAQUE;
}

fn push_instance(bucket_id: u32, inst: VoxelInstance) {
    if bucket_id >= arrayLength(&bucket_regions) {
        return;
    }
    let region = bucket_regions[bucket_id];
    let slot = atomicAdd(&counters[bucket_id], 1u);
    if slot >= region.capacity {
        return;
    }
    let global_idx = region.offset + slot;
    if global_idx >= arrayLength(&instances) {
        return;
    }
    instances[global_idx] = inst;
}

fn cube_face_tint(block_meta: GpuBlockMeta) -> f32 {
    if (block_meta.flags & BLOCK_FLAG_FLUID) != 0u {
        return TINT_WATER;
    }
    return 0.0;
}

fn emit_cube_face(
    lx: i32, ly: i32, lz: i32,
    cell: GpuBlockCell,
    block_meta: GpuBlockMeta,
    face: u32,
    nlx: i32, nly: i32, nlz: i32,
) {
    let neighbor = block_at(nlx, nly, nlz);
    let neighbor_meta = meta_for(neighbor.id);
    if neighbor_culls_face(block_meta.flags, neighbor.id, neighbor_meta.flags) {
        return;
    }
    let bucket_id = cube_bucket_for_layer(block_meta.render_layer);
    let wx = chunk_uniform.chunk_origin.x + lx;
    let wy = chunk_uniform.chunk_origin.y + ly;
    let wz = chunk_uniform.chunk_origin.z + lz;
    let power = power_at(lx, ly, lz);
    push_instance(bucket_id, VoxelInstance(
        vec4<i32>(wx, wy, wz, i32(face)),
        cell.id,
        cell.state,
        array<u32, 3>(block_meta.texture_top, block_meta.texture_bottom, block_meta.texture_sides),
        cube_face_tint(block_meta),
        0.0,
        array<f32, 3>(1.0, 1.0, 1.0),
        power,
        bucket_id,
    ));
}

fn wire_connectable_at(lx: i32, ly: i32, lz: i32, dx: i32, dy: i32, dz: i32) -> bool {
    let cell = block_at(lx + dx, ly + dy, lz + dz);
    if is_air(cell) {
        return false;
    }
    let block_meta = meta_for(cell.id);
    return (block_meta.flags & BLOCK_FLAG_REDSTONE_DUST) != 0u || block_meta.geometry_kind == GEO_WIRE;
}

fn wire_full_cube_at(lx: i32, ly: i32, lz: i32, dx: i32, dy: i32, dz: i32) -> bool {
    let cell = block_at(lx + dx, ly + dy, lz + dz);
    if is_air(cell) {
        return false;
    }
    let block_meta = meta_for(cell.id);
    return (block_meta.flags & BLOCK_FLAG_OPAQUE) != 0u && (block_meta.flags & BLOCK_FLAG_SOLID) != 0u;
}

fn wire_link_for_direction(lx: i32, ly: i32, lz: i32, dx: i32, dz: i32) -> u32 {
    if wire_connectable_at(lx, ly, lz, dx, 0, dz) {
        return WIRE_SIDE;
    }
    if wire_full_cube_at(lx, ly, lz, dx, 0, dz) {
        if wire_connectable_at(lx, ly, lz, dx, 1, dz) && !wire_full_cube_at(lx, ly, lz, 0, 1, 0) {
            return WIRE_UP;
        }
    } else if wire_connectable_at(lx, ly, lz, dx, -1, dz) {
        return WIRE_SIDE;
    }
    return WIRE_NONE;
}

fn wire_connections_at(lx: i32, ly: i32, lz: i32) -> vec4<u32> {
    var sides = vec4<u32>(0u, 0u, 0u, 0u);
    sides.x = wire_link_for_direction(lx, ly, lz, 0, -1);
    sides.y = wire_link_for_direction(lx, ly, lz, 1, 0);
    sides.z = wire_link_for_direction(lx, ly, lz, 0, 1);
    sides.w = wire_link_for_direction(lx, ly, lz, -1, 0);
    let active_count = select(0u, 1u, sides.x != WIRE_NONE) +
        select(0u, 1u, sides.y != WIRE_NONE) +
        select(0u, 1u, sides.z != WIRE_NONE) +
        select(0u, 1u, sides.w != WIRE_NONE);
    if active_count == 1u {
        if sides.x != WIRE_NONE && sides.z == WIRE_NONE { sides.z = WIRE_SIDE; }
        if sides.y != WIRE_NONE && sides.w == WIRE_NONE { sides.w = WIRE_SIDE; }
        if sides.z != WIRE_NONE && sides.x == WIRE_NONE { sides.x = WIRE_SIDE; }
        if sides.w != WIRE_NONE && sides.y == WIRE_NONE { sides.y = WIRE_SIDE; }
    }
    return sides;
}

// Texture slots packed by the CPU for redstone dust:
//   texture_top = centre dot, texture_bottom = N/S line, texture_sides = E/W line.
fn wire_tex(block_meta: GpuBlockMeta, tex_id: u32) -> array<u32, 3> {
    return array<u32, 3>(tex_id, tex_id, tex_id);
}

fn emit_wire_instances(lx: i32, ly: i32, lz: i32, cell: GpuBlockCell, block_meta: GpuBlockMeta) {
    let sides = wire_connections_at(lx, ly, lz);
    let wx = chunk_uniform.chunk_origin.x + lx;
    let wy = chunk_uniform.chunk_origin.y + ly;
    let wz = chunk_uniform.chunk_origin.z + lz;
    let power = power_at(lx, ly, lz);
    let tint = encode_power_tint(power);

    let tex_dot = block_meta.texture_top;
    let tex_ew = block_meta.texture_sides;

    // Classify connections (directions 0/2 are N/S, 1/3 are E/W).
    var side_count = 0u;
    var up_count = 0u;
    var ns = false;
    var ew = false;
    for (var dir = 0u; dir < 4u; dir++) {
        let link = sides[dir];
        if link == WIRE_SIDE {
            side_count += 1u;
            if dir == 0u || dir == 2u { ns = true; } else { ew = true; }
        } else if link == WIRE_UP {
            up_count += 1u;
        }
    }

    // Centre dot is hidden on straight, single-axis runs (matches CPU renderer).
    var show_center = true;
    if up_count == 0u {
        if side_count == 2u && ns && !ew { show_center = false; }
        if side_count == 2u && ew && !ns { show_center = false; }
    }
    let total_links = side_count + up_count;
    if total_links == 0u { show_center = true; }

    if show_center {
        push_instance(BUCKET_WIRE_CUTOUT, VoxelInstance(
            vec4<i32>(wx, wy, wz, 0),
            cell.id, cell.state,
            wire_tex(block_meta, tex_dot),
            tint, 0.0, array<f32, 3>(1.0, 1.0, 1.0), power, BUCKET_WIRE_CUTOUT,
        ));
    }
    // A single line texture is used for every segment; the shader rotates the
    // UV 90 degrees for north/south faces so the line runs along the right axis.
    let line_tex = tex_ew;
    for (var dir = 0u; dir < 4u; dir++) {
        let link = sides[dir];
        if link == WIRE_NONE { continue; }
        push_instance(BUCKET_WIRE_CUTOUT, VoxelInstance(
            vec4<i32>(wx, wy, wz, i32(dir + 1)),
            cell.id, cell.state,
            wire_tex(block_meta, line_tex),
            tint, 0.0, array<f32, 3>(1.0, 1.0, 1.0), power, BUCKET_WIRE_CUTOUT,
        ));
        if link == WIRE_UP {
            push_instance(BUCKET_WIRE_CUTOUT, VoxelInstance(
                vec4<i32>(wx, wy, wz, i32(dir + 5)),
                cell.id, cell.state,
                wire_tex(block_meta, line_tex),
                tint, 0.0, array<f32, 3>(1.0, 1.0, 1.0), power, BUCKET_WIRE_CUTOUT,
            ));
        }
    }
}

@compute @workgroup_size(4, 4, 4)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let lx = i32(gid.x);
    let ly = i32(gid.y);
    let lz = i32(gid.z);
    if lx >= CHUNK_SIZE || ly >= CHUNK_SIZE || lz >= CHUNK_SIZE {
        return;
    }

    let cell = block_at(lx, ly, lz);
    if is_air(cell) {
        return;
    }
    let block_meta = meta_for(cell.id);
    if block_meta.flags == 0u && block_meta.geometry_kind == 0u {
        return;
    }
    let renderable = (block_meta.flags & BLOCK_FLAG_OPAQUE) != 0u ||
        (block_meta.flags & BLOCK_FLAG_TRANSPARENT) != 0u ||
        (block_meta.flags & BLOCK_FLAG_SOLID) != 0u;
    if !renderable {
        return;
    }

    let wx0 = f32(chunk_uniform.chunk_origin.x + lx);
    let wy0 = f32(chunk_uniform.chunk_origin.y + ly);
    let wz0 = f32(chunk_uniform.chunk_origin.z + lz);
    if !frustum_visible(vec3<f32>(wx0, wy0, wz0), vec3<f32>(wx0 + 1.0, wy0 + 1.0, wz0 + 1.0)) {
        return;
    }

    if block_meta.geometry_kind == GEO_CUBE {
        emit_cube_face(lx, ly, lz, cell, block_meta, 0u, lx, ly - 1, lz);
        emit_cube_face(lx, ly, lz, cell, block_meta, 1u, lx, ly + 1, lz);
        emit_cube_face(lx, ly, lz, cell, block_meta, 2u, lx, ly, lz - 1);
        emit_cube_face(lx, ly, lz, cell, block_meta, 3u, lx, ly, lz + 1);
        emit_cube_face(lx, ly, lz, cell, block_meta, 4u, lx - 1, ly, lz);
        emit_cube_face(lx, ly, lz, cell, block_meta, 5u, lx + 1, ly, lz);
    } else if block_meta.geometry_kind == GEO_CROSS {
        push_instance(BUCKET_CROSS_CUTOUT, VoxelInstance(
            vec4<i32>(chunk_uniform.chunk_origin.x + lx, chunk_uniform.chunk_origin.y + ly, chunk_uniform.chunk_origin.z + lz, 0),
            cell.id, cell.state,
            array<u32, 3>(block_meta.texture_top, block_meta.texture_bottom, block_meta.texture_sides),
            0.0, 0.0, array<f32, 3>(1.0, 1.0, 1.0), power_at(lx, ly, lz), BUCKET_CROSS_CUTOUT,
        ));
    } else if block_meta.geometry_kind == GEO_WIRE {
        emit_wire_instances(lx, ly, lz, cell, block_meta);
    } else if block_meta.geometry_kind == GEO_MODEL {
        let bucket_id = block_meta.model_bucket_base + cell.variant;
        push_instance(bucket_id, VoxelInstance(
            vec4<i32>(chunk_uniform.chunk_origin.x + lx, chunk_uniform.chunk_origin.y + ly, chunk_uniform.chunk_origin.z + lz, 0),
            cell.id, cell.state,
            array<u32, 3>(block_meta.texture_top, block_meta.texture_bottom, block_meta.texture_sides),
            0.0, 0.0, array<f32, 3>(1.0, 1.0, 1.0), power_at(lx, ly, lz), bucket_id,
        ));
    }
}
