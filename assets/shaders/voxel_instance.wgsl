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

fn instance_tint_mul(inst: VoxelInstance) -> vec3<f32> {
    return vec3<f32>(inst.tint_mul[0], inst.tint_mul[1], inst.tint_mul[2]);
}

struct GpuAtlasRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

struct GpuCameraUniform {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
}

struct MaterialUniforms {
    grass_tint: vec4<f32>,
    foliage_tint: vec4<f32>,
    power_tint_dark: vec4<f32>,
    power_tint_bright: vec4<f32>,
    material_flags: vec4<f32>,
    water_tint: vec4<f32>,
    fluid_anim: vec4<f32>,
    atlas_size: vec4<f32>,
}

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) overlay_uv: vec2<f32>,
    @location(3) tint: f32,
    @location(4) overlay_tint: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) overlay_uv: vec2<f32>,
    @location(2) tint: f32,
    @location(3) overlay_tint: f32,
    @location(4) tint_mul: vec3<f32>,
}

@group(0) @binding(0) var<uniform> camera: GpuCameraUniform;
@group(0) @binding(1) var<storage, read> instances: array<VoxelInstance>;
@group(0) @binding(2) var<storage, read> atlas_rects: array<GpuAtlasRect>;
@group(0) @binding(3) var atlas_tex: texture_2d<f32>;
@group(0) @binding(4) var atlas_s: sampler;
@group(0) @binding(5) var<uniform> mats: MaterialUniforms;

const MODEL_BUCKET_BASE: u32 = 16u;

const FACE_NORMALS: array<vec3<f32>, 6> = array<vec3<f32>, 6>(
    vec3<f32>(0.0, -1.0, 0.0),
    vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, 0.0, -1.0),
    vec3<f32>(0.0, 0.0, 1.0),
    vec3<f32>(-1.0, 0.0, 0.0),
    vec3<f32>(1.0, 0.0, 0.0),
);

fn face_uv(face: u32, corner: u32) -> vec2<f32> {
    let uvs = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 0.0),
    );
    return uvs[corner % 4u];
}

// Map the shared prototype quad (local x,y in [0,1]) onto the requested cube
// face of the unit cell at `origin`. Each mapping is chosen so that the quad's
// winding (CCW with indices [0,1,2,0,2,3]) yields the outward-facing normal,
// matching FrontFace::Ccw + cull Back.
fn transform_cube_vertex(inst: VoxelInstance, local: vec3<f32>, face: u32) -> vec3<f32> {
    let origin = vec3<f32>(f32(inst.world_pos.x), f32(inst.world_pos.y), f32(inst.world_pos.z));
    let u = local.x;
    let v = local.y;
    var p = vec3<f32>(u, v, 1.0);
    switch face {
        case 0u: { p = vec3<f32>(u, 0.0, v); }          // -Y bottom
        case 1u: { p = vec3<f32>(u, 1.0, 1.0 - v); }    // +Y top
        case 2u: { p = vec3<f32>(1.0 - u, v, 0.0); }    // -Z north
        case 3u: { p = vec3<f32>(u, v, 1.0); }          // +Z south
        case 4u: { p = vec3<f32>(0.0, v, u); }          // -X west
        case 5u: { p = vec3<f32>(1.0, v, 1.0 - u); }    // +X east
        default: { p = vec3<f32>(u, v, 1.0); }
    }
    return origin + p;
}

// Lay the shared flat unit quad onto redstone-wire geometry. Faces 0..4 are
// flat-on-floor quads (centre + the four planar connections); faces 5..8 climb
// the wall in the connection direction. The mappings are chosen so the quad's
// CCW winding faces the viewer (up for floor quads, inward for wall quads),
// matching FrontFace::Ccw + cull Back.
fn transform_wire_vertex(inst: VoxelInstance, local: vec3<f32>, face: u32) -> vec3<f32> {
    let origin = vec3<f32>(f32(inst.world_pos.x), f32(inst.world_pos.y), f32(inst.world_pos.z));
    let u = local.x;
    let w = local.y;
    let h = 0.02;
    var p = vec3<f32>(w, h, u); // floor quad facing +Y
    if face >= 5u {
        let dir = face - 5u;
        if dir == 0u {
            p = vec3<f32>(u, w, h);             // -Z wall, faces +Z
        } else if dir == 1u {
            p = vec3<f32>(1.0 - h, w, u);        // +X wall, faces -X
        } else if dir == 2u {
            p = vec3<f32>(1.0 - u, w, 1.0 - h);  // +Z wall, faces -Z
        } else {
            p = vec3<f32>(h, w, 1.0 - u);        // -X wall, faces +X
        }
    }
    return origin + p;
}

fn is_water(tint: f32) -> bool {
    return tint >= 4.25 && tint < 5.0;
}

fn atlas_uv(tex_id: u32, uv: vec2<f32>, water: bool) -> vec2<f32> {
    if tex_id >= arrayLength(&atlas_rects) {
        return uv;
    }
    let rect = atlas_rects[tex_id];
    let aw = mats.atlas_size.x;
    let ah = mats.atlas_size.y;
    // Animated fluids stack frames vertically in the atlas. Map quad UVs to a single
    // frame's sub-rect (matching CPU mesh atlas_uv_bounds); animated_uv() scrolls
    // between frames in the fragment shader.
    var rect_h = rect.h;
    if water && mats.fluid_anim.x > 1.0 {
        rect_h = mats.fluid_anim.y * ah;
    }
    // Inset to texel centers so quad corners never bleed into neighboring atlas texels.
    let u0 = rect.x + 0.5;
    let v0 = rect.y + 0.5;
    let u1 = rect.x + rect.w - 0.5;
    let v1 = rect.y + rect_h - 0.5;
    return vec2<f32>(
        (u0 + uv.x * (u1 - u0)) / aw,
        (v0 + uv.y * (v1 - v0)) / ah,
    );
}

@vertex
fn vertex(vertex: Vertex, @builtin(instance_index) inst_idx: u32) -> VertexOutput {
    var out: VertexOutput;
    let inst = instances[inst_idx];
    let face = u32(inst.world_pos.w);
    // Only the cube buckets (0,1,2) use the shared prototype quad that needs
    // face transformation. Cross/wire/model geometry is pre-baked in cell-local
    // space and only needs translation by the cell origin.
    let is_cube = inst.bucket_id < 3u;
    var world_pos = vertex.position;
    if is_cube {
        world_pos = transform_cube_vertex(inst, vertex.position, face);
    } else if inst.bucket_id == 4u {
        world_pos = transform_wire_vertex(inst, vertex.position, face);
    } else {
        world_pos = vec3<f32>(
            f32(inst.world_pos.x) + vertex.position.x,
            f32(inst.world_pos.y) + vertex.position.y,
            f32(inst.world_pos.z) + vertex.position.z,
        );
    }
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    // Prototype geometry (cube/cross/wire, bucket_id < MODEL_BUCKET_BASE) carries
    // local [0,1] UVs and a per-instance texture index, so remap into the atlas
    // rect here. Model geometry already bakes atlas-space UVs into the mesh.
    if inst.bucket_id == 4u {
        // Wire packs the same texture in every slot; rotate the line 90 degrees
        // for north/south faces (side faces 1/3, wall-up faces 5/7).
        var luv = vertex.uv;
        if face == 1u || face == 3u || face == 5u || face == 7u {
            luv = vec2<f32>(luv.y, 1.0 - luv.x);
        }
        out.uv = atlas_uv(inst.tex_indices[0], luv, false);
    } else if inst.bucket_id < MODEL_BUCKET_BASE {
        var tex_id = inst.tex_indices[2];
        if face == 1u {
            tex_id = inst.tex_indices[0];
        } else if face == 0u {
            tex_id = inst.tex_indices[1];
        }
        let water = is_water(inst.tint);
        out.uv = atlas_uv(tex_id, vertex.uv, water);
    } else {
        out.uv = vertex.uv;
    }
    out.overlay_uv = vertex.overlay_uv;
    var tint = vertex.tint;
    // Redstone dust carries its power tint in inst.tint; models/cubes leave it 0
    // and must not be tinted just because they sit on a powered cell.
    if tint == 0.0 && inst.tint > 0.0 {
        tint = inst.tint;
    }
    out.tint = tint;
    out.overlay_tint = vertex.overlay_tint;
    out.tint_mul = instance_tint_mul(inst);
    return out;
}

fn is_power(tint: f32) -> bool {
    return tint >= 2.875 && tint <= 4.125;
}

fn uses_vertex_tint_mul(tint_mul: vec3<f32>) -> bool {
    return length(tint_mul - vec3<f32>(1.0)) > 0.001;
}

fn apply_tint(rgb: vec3<f32>, tint: f32, tint_mul: vec3<f32>) -> vec3<f32> {
    if is_water(tint) {
        return rgb * mats.water_tint.rgb;
    }
    if is_power(tint) {
        let power = clamp(tint - 3.0, 0.0, 1.0);
        let rs = mix(mats.power_tint_dark.rgb, mats.power_tint_bright.rgb, power);
        return rgb * rs;
    }
    if tint >= 1.5 {
        if uses_vertex_tint_mul(tint_mul) {
            return rgb * tint_mul;
        }
        return rgb * mats.foliage_tint.rgb;
    }
    if tint >= 0.5 {
        if uses_vertex_tint_mul(tint_mul) {
            return rgb * tint_mul;
        }
        return rgb * mats.grass_tint.rgb;
    }
    return rgb;
}

fn animated_uv(uv: vec2<f32>, tint: f32) -> vec2<f32> {
    if is_water(tint) && mats.fluid_anim.x > 1.0 {
        let frame = floor(mats.fluid_anim.w / mats.fluid_anim.z) % mats.fluid_anim.x;
        return vec2<f32>(uv.x, uv.y + frame * mats.fluid_anim.y);
    }
    return uv;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if is_water(in.tint) {
        let sample_uv = animated_uv(in.uv, in.tint);
        let tex = textureSample(atlas_tex, atlas_s, sample_uv);
        return vec4<f32>(vec3<f32>(tex.r) * mats.water_tint.rgb, tex.a);
    }
    let uv = animated_uv(in.uv, in.tint);
    var base = textureSample(atlas_tex, atlas_s, uv);
    if mats.material_flags.x > 0.5 && base.a < 0.5 {
        discard;
    }
    var rgb = apply_tint(base.rgb, in.tint, in.tint_mul);
    if in.overlay_tint >= 0.5 {
        let ov_uv = animated_uv(in.overlay_uv, in.overlay_tint);
        let ov = textureSample(atlas_tex, atlas_s, ov_uv);
        if mats.material_flags.x > 0.5 && ov.a < 0.5 {
            discard;
        }
        let tinted_overlay = apply_tint(ov.rgb, in.overlay_tint, in.tint_mul);
        rgb = mix(rgb, tinted_overlay, ov.a);
    }
    return vec4<f32>(rgb, base.a);
}
