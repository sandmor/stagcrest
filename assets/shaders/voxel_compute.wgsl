struct GpuBlockMeta {
    geometry_kind: u32,
    render_layer: u32,
    flags: u32,
    model_bucket_base: u32,
    texture_top: u32,
    texture_bottom: u32,
    texture_sides: u32,
    texture_overlay_top: u32,
    texture_overlay_bottom: u32,
    texture_overlay_sides: u32,
    tint_kinds: u32,
    overlay_tint_kinds: u32,
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

struct ChunkTintCell {
    grass: array<f32, 3>,
    foliage: array<f32, 3>,
}

struct GpuChunkTableEntry {
    blocks_offset: u32,
    power_offset: u32,
    scratch_offset: u32,
    tint_cells_offset: u32,
    origin_x: i32,
    origin_y: i32,
    origin_z: i32,
    air_id: u32,
    flags: u32,
}

struct EmitParams {
    dispatch_count: u32,
    bucket_slot_count: u32,
    chunk_scratch_capacity: u32,
    _pad: u32,
}

const BLOCK_FLAG_OPAQUE: u32 = 1u;
const BLOCK_FLAG_SOLID: u32 = 2u;
const BLOCK_FLAG_FLUID: u32 = 4u;
const BLOCK_FLAG_TRANSPARENT: u32 = 8u;
const BLOCK_FLAG_REDSTONE_DUST: u32 = 16u;
const CHUNK_TABLE_VALID: u32 = 1u;

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

const TINT_KIND_GRASS: u32 = 1u;
const TINT_KIND_FOLIAGE: u32 = 2u;
const TINT_WATER: f32 = 4.5;

const HALO_SIZE: i32 = 18;
const CHUNK_SIZE: i32 = 16;
const EMIT_TILE_SIZE: u32 = 4u;
const EMIT_TILES_PER_AXIS: u32 = 4u; // CHUNK_SIZE / EMIT_TILE_SIZE

const WIRE_NONE: u32 = 0u;
const WIRE_SIDE: u32 = 1u;
const WIRE_UP: u32 = 2u;

@group(0) @binding(0) var<storage, read> block_meta: array<GpuBlockMeta>;
@group(0) @binding(1) var<storage, read> chunk_blocks: array<GpuBlockCell>;
@group(0) @binding(2) var<storage, read> chunk_power: array<u32>;
@group(0) @binding(3) var<storage, read> chunk_table: array<GpuChunkTableEntry>;
@group(0) @binding(4) var<storage, read> dispatch_list: array<u32>;
@group(0) @binding(5) var<storage, read_write> scratch_instances: array<VoxelInstance>;
@group(0) @binding(6) var<storage, read_write> scratch_counters: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> scratch_overflow: array<atomic<u32>>;
@group(0) @binding(8) var<storage, read> chunk_tint_cells: array<ChunkTintCell>;
@group(0) @binding(9) var<uniform> emit_params: EmitParams;

var<private> chunk_entry: GpuChunkTableEntry;
var<private> chunk_slot: u32;

fn halo_index(lx: i32, ly: i32, lz: i32) -> u32 {
    return u32(lx + 1 + (ly + 1) * HALO_SIZE + (lz + 1) * HALO_SIZE * HALO_SIZE);
}

fn block_at(lx: i32, ly: i32, lz: i32) -> GpuBlockCell {
    return chunk_blocks[chunk_entry.blocks_offset + halo_index(lx, ly, lz)];
}

fn meta_for(id: u32) -> GpuBlockMeta {
    if id >= arrayLength(&block_meta) {
        return GpuBlockMeta(0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u);
    }
    return block_meta[id];
}

fn power_byte_at(byte_offset: u32) -> u32 {
    let word_index = byte_offset / 4u;
    let byte_in_word = byte_offset % 4u;
    let word = chunk_power[word_index];
    return (word >> (byte_in_word * 8u)) & 0xffu;
}

fn power_at(lx: i32, ly: i32, lz: i32) -> u32 {
    if lx < 0 || ly < 0 || lz < 0 || lx >= CHUNK_SIZE || ly >= CHUNK_SIZE || lz >= CHUNK_SIZE {
        return 0u;
    }
    let idx = u32(lx + lz * CHUNK_SIZE + ly * CHUNK_SIZE * CHUNK_SIZE);
    return power_byte_at(chunk_entry.power_offset + idx);
}

fn encode_power_tint(power: u32) -> f32 {
    return 3.0 + f32(power) / 15.0;
}

fn is_air(cell: GpuBlockCell) -> bool {
    return cell.id == chunk_entry.air_id;
}

fn neighbor_culls_face(block_flags: u32, neighbor_id: u32, neighbor_flags: u32) -> bool {
    if neighbor_id == chunk_entry.air_id {
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

fn cube_bucket_for_layer(layer: u32) -> u32 {
    if layer == LAYER_CUTOUT { return BUCKET_CUBE_CUTOUT; }
    if layer == LAYER_BLEND { return BUCKET_CUBE_BLEND; }
    return BUCKET_CUBE_OPAQUE;
}

fn push_instance(bucket_id: u32, inst: VoxelInstance) {
    loop {
        let prev = atomicLoad(&scratch_counters[chunk_slot]);
        if prev >= emit_params.chunk_scratch_capacity {
            atomicAdd(&scratch_overflow[chunk_slot], 1u);
            return;
        }
        let exchanged = atomicCompareExchangeWeak(
            &scratch_counters[chunk_slot],
            prev,
            prev + 1u,
        );
        if exchanged.exchanged {
            scratch_instances[chunk_entry.scratch_offset + prev] = inst;
            return;
        }
    }
}

// Matches Rust `pack_tint_kinds(top, bottom, sides)`: low byte = top, next = bottom, high = sides.
// Cube emit faces: 0 = bottom (-Y), 1 = top (+Y), 2..5 = sides.
fn unpack_tint_kind(packed: u32, face: u32) -> u32 {
    if face == 1u {
        return packed & 0xFFu;
    }
    if face == 0u {
        return (packed >> 8u) & 0xFFu;
    }
    return (packed >> 16u) & 0xFFu;
}

fn tint_kind_as_f32(kind: u32) -> f32 {
    if kind == 4u {
        return TINT_WATER;
    }
    return f32(kind);
}

fn biome_grid_index(lx: i32, ly: i32, lz: i32) -> u32 {
    let gx = clamp(lx / 4, 0, 3);
    let gy = clamp(ly / 4, 0, 3);
    let gz = clamp(lz / 4, 0, 3);
    return u32(gx + gy * 4 + gz * 16);
}

fn tint_mul_for_kind(lx: i32, ly: i32, lz: i32, kind: u32) -> vec3<f32> {
    let idx = chunk_entry.tint_cells_offset + biome_grid_index(lx, ly, lz);
    if idx >= arrayLength(&chunk_tint_cells) {
        return vec3<f32>(1.0, 1.0, 1.0);
    }
    let cell = chunk_tint_cells[idx];
    if kind == TINT_KIND_GRASS {
        return vec3<f32>(cell.grass[0], cell.grass[1], cell.grass[2]);
    }
    if kind == TINT_KIND_FOLIAGE {
        return vec3<f32>(cell.foliage[0], cell.foliage[1], cell.foliage[2]);
    }
    return vec3<f32>(1.0, 1.0, 1.0);
}

fn tint_mul_for_face(lx: i32, ly: i32, lz: i32, face: u32, block_meta: GpuBlockMeta) -> array<f32, 3> {
    let overlay_kind = unpack_tint_kind(block_meta.overlay_tint_kinds, face);
    var kind = unpack_tint_kind(block_meta.tint_kinds, face);
    if overlay_kind == TINT_KIND_GRASS || overlay_kind == TINT_KIND_FOLIAGE {
        kind = overlay_kind;
    }
    let mul = tint_mul_for_kind(lx, ly, lz, kind);
    return array<f32, 3>(mul.x, mul.y, mul.z);
}

fn face_tint(face: u32, block_meta: GpuBlockMeta) -> f32 {
    if (block_meta.flags & BLOCK_FLAG_FLUID) != 0u {
        return TINT_WATER;
    }
    return tint_kind_as_f32(unpack_tint_kind(block_meta.tint_kinds, face));
}

fn face_overlay_tint(face: u32, block_meta: GpuBlockMeta) -> f32 {
    return tint_kind_as_f32(unpack_tint_kind(block_meta.overlay_tint_kinds, face));
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
    let wx = chunk_entry.origin_x + lx;
    let wy = chunk_entry.origin_y + ly;
    let wz = chunk_entry.origin_z + lz;
    let power = power_at(lx, ly, lz);
    let tint = face_tint(face, block_meta);
    let overlay_tint = face_overlay_tint(face, block_meta);
    let tint_mul = tint_mul_for_face(lx, ly, lz, face, block_meta);
    push_instance(bucket_id, VoxelInstance(
        vec4<i32>(wx, wy, wz, i32(face)),
        cell.id,
        cell.state,
        array<u32, 3>(block_meta.texture_top, block_meta.texture_bottom, block_meta.texture_sides),
        tint,
        overlay_tint,
        tint_mul,
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

fn wire_tex(block_meta: GpuBlockMeta, tex_id: u32) -> array<u32, 3> {
    return array<u32, 3>(tex_id, tex_id, tex_id);
}

fn emit_wire_instances(lx: i32, ly: i32, lz: i32, cell: GpuBlockCell, block_meta: GpuBlockMeta) {
    let sides = wire_connections_at(lx, ly, lz);
    let wx = chunk_entry.origin_x + lx;
    let wy = chunk_entry.origin_y + ly;
    let wz = chunk_entry.origin_z + lz;
    let power = power_at(lx, ly, lz);
    let tint = encode_power_tint(power);
    let tex_dot = block_meta.texture_top;
    let tex_ew = block_meta.texture_sides;
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
fn main(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let dispatch_idx = wid.x / EMIT_TILES_PER_AXIS;
    if dispatch_idx >= emit_params.dispatch_count {
        return;
    }
    chunk_slot = dispatch_list[dispatch_idx];
    chunk_entry = chunk_table[chunk_slot];
    if (chunk_entry.flags & CHUNK_TABLE_VALID) == 0u {
        return;
    }

    let tile_x = wid.x % EMIT_TILES_PER_AXIS;
    let tile_y = wid.y;
    let tile_z = wid.z;
    let lx = i32(tile_x * EMIT_TILE_SIZE + lid.x);
    let ly = i32(tile_y * EMIT_TILE_SIZE + lid.y);
    let lz = i32(tile_z * EMIT_TILE_SIZE + lid.z);
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

    if block_meta.geometry_kind == GEO_CUBE {
        emit_cube_face(lx, ly, lz, cell, block_meta, 0u, lx, ly - 1, lz);
        emit_cube_face(lx, ly, lz, cell, block_meta, 1u, lx, ly + 1, lz);
        emit_cube_face(lx, ly, lz, cell, block_meta, 2u, lx, ly, lz - 1);
        emit_cube_face(lx, ly, lz, cell, block_meta, 3u, lx, ly, lz + 1);
        emit_cube_face(lx, ly, lz, cell, block_meta, 4u, lx - 1, ly, lz);
        emit_cube_face(lx, ly, lz, cell, block_meta, 5u, lx + 1, ly, lz);
    } else if block_meta.geometry_kind == GEO_CROSS {
        let sides_kind = unpack_tint_kind(block_meta.tint_kinds, 2u);
        let tint = tint_kind_as_f32(sides_kind);
        let tint_mul = tint_mul_for_kind(lx, ly, lz, sides_kind);
        push_instance(BUCKET_CROSS_CUTOUT, VoxelInstance(
            vec4<i32>(chunk_entry.origin_x + lx, chunk_entry.origin_y + ly, chunk_entry.origin_z + lz, 0),
            cell.id, cell.state,
            array<u32, 3>(block_meta.texture_top, block_meta.texture_bottom, block_meta.texture_sides),
            tint, 0.0, array<f32, 3>(tint_mul.x, tint_mul.y, tint_mul.z), power_at(lx, ly, lz), BUCKET_CROSS_CUTOUT,
        ));
    } else if block_meta.geometry_kind == GEO_WIRE {
        emit_wire_instances(lx, ly, lz, cell, block_meta);
    } else if block_meta.geometry_kind == GEO_MODEL {
        let bucket_id = block_meta.model_bucket_base + cell.variant;
        push_instance(bucket_id, VoxelInstance(
            vec4<i32>(chunk_entry.origin_x + lx, chunk_entry.origin_y + ly, chunk_entry.origin_z + lz, 0),
            cell.id, cell.state,
            array<u32, 3>(block_meta.texture_top, block_meta.texture_bottom, block_meta.texture_sides),
            0.0, 0.0, array<f32, 3>(1.0, 1.0, 1.0), power_at(lx, ly, lz), bucket_id,
        ));
    }
}
