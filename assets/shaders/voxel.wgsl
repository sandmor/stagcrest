#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions::get_world_from_local,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}
#import stagcrest::scene_lighting::{
    SceneLighting,
    decode_normal_axis,
    shade_voxel,
    sample_sky_direction,
    unpack_light,
    medium_attenuation,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas0_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas0_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var atlas1_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var atlas1_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var atlas2_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var atlas2_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var atlas3_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var atlas3_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var atlas4_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var atlas4_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var atlas5_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var atlas5_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var atlas6_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(13) var atlas6_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(14) var atlas7_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(15) var atlas7_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(16) var<uniform> grass_tint: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(17) var<uniform> foliage_tint: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(18) var<uniform> power_tint_dark: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(19) var<uniform> power_tint_bright: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(20) var<uniform> material_flags: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(21) var<uniform> water_tint: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(22) var<uniform> fluid_anim: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(23) var<uniform> scene_lighting: SceneLighting;

const VERTEX_FLAG_EMISSIVE: u32 = 1u;
const VERTEX_FLAG_FACES_FLUID: u32 = 2u;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) overlay_uv: vec2<f32>,
    @location(3) tint: f32,
    @location(4) overlay_tint: f32,
    @location(5) tint_mul: vec3<f32>,
    @location(6) atlas_index: f32,
    @location(7) overlay_atlas_index: f32,
    @location(8) normal: u32,
    @location(9) light: u32,
    @location(10) ao: u32,
    @location(11) flags: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) overlay_uv: vec2<f32>,
    @location(2) tint: f32,
    @location(3) overlay_tint: f32,
    @location(4) tint_mul: vec3<f32>,
    @location(5) atlas_index: f32,
    @location(6) overlay_atlas_index: f32,
    @location(7) @interpolate(flat) normal: u32,
    @location(8) @interpolate(flat) light: u32,
    @location(9) @interpolate(flat) ao: u32,
    @location(10) @interpolate(flat) flags: u32,
    @location(11) world_position: vec3<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(0u);
    let world_position = world_from_local * vec4(vertex.position, 1.0);
    out.clip_position = position_world_to_clip(world_position.xyz);
    out.world_position = world_position.xyz;
    out.uv = vertex.uv;
    out.overlay_uv = vertex.overlay_uv;
    out.tint = vertex.tint;
    out.overlay_tint = vertex.overlay_tint;
    out.tint_mul = vertex.tint_mul;
    out.atlas_index = vertex.atlas_index;
    out.overlay_atlas_index = vertex.overlay_atlas_index;
    out.normal = vertex.normal;
    out.light = vertex.light;
    out.ao = vertex.ao;
    out.flags = vertex.flags;
    return out;
}

fn sample_atlas(uv: vec2<f32>, atlas_index: u32) -> vec4<f32> {
    switch atlas_index {
        case 0u: { return textureSample(atlas0_tex, atlas0_s, uv); }
        case 1u: { return textureSample(atlas1_tex, atlas1_s, uv); }
        case 2u: { return textureSample(atlas2_tex, atlas2_s, uv); }
        case 3u: { return textureSample(atlas3_tex, atlas3_s, uv); }
        case 4u: { return textureSample(atlas4_tex, atlas4_s, uv); }
        case 5u: { return textureSample(atlas5_tex, atlas5_s, uv); }
        case 6u: { return textureSample(atlas6_tex, atlas6_s, uv); }
        case 7u: { return textureSample(atlas7_tex, atlas7_s, uv); }
        default: { return textureSample(atlas0_tex, atlas0_s, uv); }
    }
}

fn is_water(tint: f32) -> bool {
    return tint >= 4.25 && tint < 5.0;
}

fn is_power(tint: f32) -> bool {
    return tint >= 2.875 && tint <= 4.125;
}

fn uses_vertex_tint_mul(tint_mul: vec3<f32>) -> bool {
    return length(tint_mul - vec3(1.0)) > 0.001;
}

fn apply_tint(rgb: vec3<f32>, tint: f32, tint_mul: vec3<f32>) -> vec3<f32> {
    if is_water(tint) {
        return rgb * water_tint.rgb;
    }
    if is_power(tint) {
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

fn shade_water(
    uv: vec2<f32>,
    tint: f32,
    atlas_index: u32,
    normal_axis: u32,
    light: u32,
    ao_val: u32,
    flags: u32,
    world_pos: vec3<f32>,
) -> vec4<f32> {
    let sample_uv = animated_uv(uv, tint);
    let tex = sample_atlas(sample_uv, atlas_index);
    let base = vec3<f32>(tex.r) * water_tint.rgb;

    let normal = decode_normal_axis(normal_axis);
    let levels = unpack_light(light);
    let ao = 0.55 + f32(ao_val) / 3.0 * 0.45;

    let view_dir = normalize(view.world_position - world_pos);
    let fresnel = pow(1.0 - max(dot(normal, view_dir), 0.0), 3.0);
    let reflect_dir = reflect(-view_dir, normal);
    let sky = sample_sky_direction(reflect_dir, scene_lighting);

    let lit = shade_voxel(
        base,
        normal,
        levels.x,
        levels.y,
        ao,
        (flags & VERTEX_FLAG_EMISSIVE) != 0u,
        scene_lighting,
    );

    var rgb = mix(lit, sky * water_tint.rgb, fresnel * 0.55 * scene_lighting.params.x);

    if scene_lighting.params.z > 0.5 {
        let absorb = medium_attenuation(2.0, scene_lighting.water_absorption.rgb, scene_lighting.params.w);
        rgb *= absorb;
    }

    return vec4(rgb, tex.a);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let atlas_idx = u32(clamp(in.atlas_index, 0.0, 7.0));
    let overlay_idx = u32(clamp(in.overlay_atlas_index, 0.0, 7.0));
    let levels = unpack_light(in.light);
    let ao = 0.55 + f32(in.ao) / 3.0 * 0.45;
    let emissive = (in.flags & VERTEX_FLAG_EMISSIVE) != 0u;
    let normal = decode_normal_axis(in.normal);

    if is_water(in.tint) {
        return shade_water(
            in.uv,
            in.tint,
            atlas_idx,
            in.normal,
            in.light,
            in.ao,
            in.flags,
            in.world_position,
        );
    }

    let uv = animated_uv(in.uv, in.tint);
    var base = sample_atlas(uv, atlas_idx);
    if material_flags.x > 0.5 && base.a < 0.5 {
        discard;
    }
    var rgb = apply_tint(base.rgb, in.tint, in.tint_mul);

    if in.overlay_tint >= 0.5 {
        let ov_uv = animated_uv(in.overlay_uv, in.overlay_tint);
        let ov = sample_atlas(ov_uv, overlay_idx);
        if material_flags.x > 0.5 && ov.a < 0.5 {
            discard;
        }
        let tinted_overlay = apply_tint(ov.rgb, in.overlay_tint, in.tint_mul);
        rgb = mix(rgb, tinted_overlay, ov.a);
    }

    rgb = shade_voxel(rgb, normal, levels.x, levels.y, ao, emissive, scene_lighting);
    return vec4(rgb, base.a);
}
