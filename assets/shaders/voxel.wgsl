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
@group(2) @binding(4) var<uniform> redstone_tint_dark: vec4<f32>;
@group(2) @binding(5) var<uniform> redstone_tint_bright: vec4<f32>;
@group(2) @binding(6) var<uniform> alpha_cutout: u32;

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
    return out;
}

fn apply_tint(rgb: vec3<f32>, tint: f32) -> vec3<f32> {
    if tint >= 3.0 {
        let power = clamp(tint - 3.0, 0.0, 1.0);
        let rs = mix(redstone_tint_dark.rgb, redstone_tint_bright.rgb, power);
        return rgb * rs;
    }
    if tint >= 1.5 {
        return rgb * foliage_tint.rgb;
    }
    if tint >= 0.5 {
        return rgb * grass_tint.rgb;
    }
    return rgb;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var base = textureSample(atlas_tex, atlas_s, in.uv);
    if alpha_cutout != 0u && base.a < 0.5 {
        discard;
    }
    var rgb = apply_tint(base.rgb, in.tint);

    if in.overlay_tint >= 0.5 {
        let ov = textureSample(atlas_tex, atlas_s, in.overlay_uv);
        if alpha_cutout != 0u && ov.a < 0.5 {
            discard;
        }
        let tinted_overlay = apply_tint(ov.rgb, in.overlay_tint);
        rgb = mix(rgb, tinted_overlay, ov.a);
    }

    return vec4(rgb, base.a);
}
