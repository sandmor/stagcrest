#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions::get_world_from_local,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}

@group(2) @binding(0) var atlas_tex: texture_2d<f32>;
@group(2) @binding(1) var atlas_s: sampler;
@group(2) @binding(2) var<uniform> grass_tint: vec4<f32>;
@group(2) @binding(3) var<uniform> foliage_tint: vec4<f32>;
@group(2) @binding(4) var<uniform> power_tint_dark: vec4<f32>;
@group(2) @binding(5) var<uniform> power_tint_bright: vec4<f32>;
@group(2) @binding(6) var<uniform> material_flags: vec4<f32>;
@group(2) @binding(7) var<uniform> water_tint: vec4<f32>;
@group(2) @binding(8) var<uniform> fluid_anim: vec4<f32>;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) overlay_uv: vec2<f32>,
    @location(3) tint: f32,
    @location(4) overlay_tint: f32,
    @location(5) tint_mul: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) overlay_uv: vec2<f32>,
    @location(2) tint: f32,
    @location(3) overlay_tint: f32,
    @location(4) tint_mul: vec3<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(0u);
    let world_position = world_from_local * vec4(vertex.position, 1.0);
    out.clip_position = position_world_to_clip(world_position.xyz);
    out.uv = vertex.uv;
    out.overlay_uv = vertex.overlay_uv;
    out.tint = vertex.tint;
    out.overlay_tint = vertex.overlay_tint;
    out.tint_mul = vertex.tint_mul;
    return out;
}

// Matches stagcrest_protocol::TINT_WATER (4.5), above max power tint (4.0).
fn is_water(tint: f32) -> bool {
    return tint >= 4.25 && tint < 5.0;
}

fn uses_vertex_tint_mul(tint_mul: vec3<f32>) -> bool {
    return length(tint_mul - vec3(1.0)) > 0.001;
}

fn apply_tint(rgb: vec3<f32>, tint: f32, tint_mul: vec3<f32>) -> vec3<f32> {
    if is_water(tint) {
        return rgb * water_tint.rgb;
    }
    if tint >= 3.0 {
        let power = clamp(tint - 3.0, 0.0, 1.0);
        let rs = mix(power_tint_dark.rgb, power_tint_bright.rgb, power);
        return rgb * rs;
    }
    if tint >= 1.5 {
        if uses_vertex_tint_mul(tint_mul) {
            return rgb * tint_mul;
        }
        return rgb * foliage_tint.rgb;
    }
    if tint >= 0.5 {
        if uses_vertex_tint_mul(tint_mul) {
            return rgb * tint_mul;
        }
        return rgb * grass_tint.rgb;
    }
    return rgb;
}

fn animated_uv(uv: vec2<f32>, tint: f32) -> vec2<f32> {
    if is_water(tint) && fluid_anim.x > 1.0 {
        let frame = floor(fluid_anim.w / fluid_anim.z) % fluid_anim.x;
        return vec2<f32>(uv.x, uv.y + frame * fluid_anim.y);
    }
    return uv;
}

fn shade_water(uv: vec2<f32>, tint: f32) -> vec4<f32> {
    let sample_uv = animated_uv(uv, tint);
    let tex = textureSample(atlas_tex, atlas_s, sample_uv);
    return vec4(vec3<f32>(tex.r) * water_tint.rgb, tex.a);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if is_water(in.tint) {
        return shade_water(in.uv, in.tint);
    }
    let uv = animated_uv(in.uv, in.tint);
    var base = textureSample(atlas_tex, atlas_s, uv);
    if material_flags.x > 0.5 && base.a < 0.5 {
        discard;
    }
    var rgb = apply_tint(base.rgb, in.tint, in.tint_mul);

    if in.overlay_tint >= 0.5 {
        let ov_uv = animated_uv(in.overlay_uv, in.overlay_tint);
        let ov = textureSample(atlas_tex, atlas_s, ov_uv);
        if material_flags.x > 0.5 && ov.a < 0.5 {
            discard;
        }
        let tinted_overlay = apply_tint(ov.rgb, in.overlay_tint, in.tint_mul);
        rgb = mix(rgb, tinted_overlay, ov.a);
    }

    return vec4(rgb, base.a);
}
