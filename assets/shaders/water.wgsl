#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions::get_world_from_local,
    mesh_view_bindings::view,
    view_transformations::{
        depth_ndc_to_view_z,
        frag_coord_to_uv,
        position_world_to_view,
        position_world_to_clip,
    },
}
#import stagcrest::scene_lighting::{
    SceneLighting,
    decode_normal_axis,
    fetch_celestial_shadows,
    shade_voxel,
    unpack_light,
    medium_attenuation,
}
#import stagcrest::reflection::resolve_reflection

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var water_atlas_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var water_atlas_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var scene_color_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var scene_color_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var planar_color_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var planar_color_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var scene_depth_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var scene_depth_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var<uniform> water_tint: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var<uniform> fluid_anim: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var<uniform> scene_lighting: SceneLighting;
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var<uniform> reflection_params: vec4<f32>;

const VERTEX_FLAG_EMISSIVE: u32 = 1u;
// Reverse-Z far plane / sky (see bevy_pbr::view_transformations).
const SKY_DEPTH: f32 = 0.0001;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(8) normal: u32,
    @location(9) light: u32,
    @location(10) ao: u32,
    @location(11) flags: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) normal_axis: u32,
    @location(2) @interpolate(flat) light: u32,
    @location(3) @interpolate(flat) ao: u32,
    @location(4) @interpolate(flat) flags: u32,
    @location(5) world_position: vec3<f32>,
    @location(6) world_normal: vec3<f32>,
}

fn wave_normal(world_pos: vec3<f32>, base_normal: vec3<f32>, time: f32) -> vec3<f32> {
    let block = floor(world_pos);
    let wave = sin((block.x + time * 0.7) * 1.3) * 0.08
        + sin((block.z + time * 0.5) * 1.7) * 0.06;
    var n = base_normal;
    n.x += wave * 0.35;
    n.z += wave * 0.35;
    return normalize(n);
}

fn animated_uv(uv: vec2<f32>) -> vec2<f32> {
    if fluid_anim.x > 1.0 {
        let frame = floor(fluid_anim.w / fluid_anim.z) % fluid_anim.x;
        return vec2(uv.x, uv.y + frame * fluid_anim.y);
    }
    return uv;
}

fn ggx_sun_glint(normal: vec3<f32>, view_dir: vec3<f32>, sun_dir: vec3<f32>, roughness: f32) -> f32 {
    let half_vec = normalize(view_dir + sun_dir);
    let n_dot_h = max(dot(normal, half_vec), 0.0);
    let n_dot_v = max(dot(normal, view_dir), 0.0);
    let n_dot_l = max(dot(normal, sun_dir), 0.0);
    let alpha = roughness * roughness;
    let alpha2 = alpha * alpha;
    let denom = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
    let d = alpha2 / max(3.14159 * denom * denom, 0.0001);
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let g_v = n_dot_v / max(n_dot_v * (1.0 - k) + k, 0.0001);
    let g_l = n_dot_l / max(n_dot_l * (1.0 - k) + k, 0.0001);
    return d * g_v * g_l * n_dot_l;
}

fn screen_uv_from_frag_coord(frag_coord: vec2<f32>) -> vec2<f32> {
    return frag_coord_to_uv(frag_coord);
}

fn sample_scene_depth(uv: vec2<f32>) -> f32 {
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 0.0;
    }
    let dims = textureDimensions(scene_depth_tex, 0);
    let coord = vec2<i32>(clamp(uv * vec2<f32>(dims), vec2(0.0), vec2<f32>(dims) - vec2(1.0, 1.0)));
    return textureLoad(scene_depth_tex, coord, 0).r;
}

// View-space column thickness when opaque geometry sits behind this fragment (shore / side faces).
fn water_column_thickness(scene_ndc: f32, world_pos: vec3<f32>) -> f32 {
    if scene_ndc <= SKY_DEPTH {
        return 0.0;
    }
    let scene_vz = depth_ndc_to_view_z(scene_ndc);
    let surface_vz = position_world_to_view(world_pos).z;
    // View -z is forward; farther points are more negative.
    return max(surface_vz - scene_vz, 0.0);
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(vertex.instance_index);
    let world_position = world_from_local * vec4(vertex.position, 1.0);
    out.clip_position = position_world_to_clip(world_position.xyz);
    out.world_position = world_position.xyz;
    out.world_normal = normalize((world_from_local * vec4(decode_normal_axis(vertex.normal), 0.0)).xyz);
    out.uv = vertex.uv;
    out.normal_axis = vertex.normal;
    out.light = vertex.light;
    out.ao = vertex.ao;
    out.flags = vertex.flags;
    return out;
}

fn fetch_shadow(world_pos: vec4<f32>, normal: vec3<f32>, frag_coord: vec2<f32>) -> vec2<f32> {
    let view_z = dot(vec4(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z,
    ), world_pos);
    return fetch_celestial_shadows(scene_lighting, world_pos, normal, view_z, frag_coord);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let sample_uv = animated_uv(in.uv);
    let tex = textureSample(water_atlas_tex, water_atlas_s, sample_uv);
    let base = vec3(tex.r) * water_tint.rgb;

    let time = fluid_anim.w;
    let normal = wave_normal(in.world_position, normalize(in.world_normal), time);
    let levels = unpack_light(in.light);
    let ao = 0.55 + f32(in.ao) / 3.0 * 0.45;
    let world_pos = vec4(in.world_position, 1.0);
    let shadow = fetch_shadow(world_pos, normal, in.clip_position.xy);

    let lit = shade_voxel(
        base,
        normal,
        levels.x,
        levels.y,
        ao,
        (in.flags & VERTEX_FLAG_EMISSIVE) != 0u,
        scene_lighting,
        shadow.x,
        shadow.y,
    );

    let view_dir = normalize(view.world_position - in.world_position);
    let fresnel = pow(1.0 - max(dot(normal, view_dir), 0.0), 3.0);
    let screen_uv = screen_uv_from_frag_coord(in.clip_position.xy);

    let reflected = resolve_reflection(
        in.world_position,
        normal,
        view_dir,
        screen_uv,
        scene_lighting,
        scene_color_tex,
        scene_depth_tex,
        planar_color_tex,
        scene_color_s,
        reflection_params,
    );

    var rgb = mix(lit, reflected * water_tint.rgb, fresnel * 0.82 + 0.08);

    let sun_dir = normalize(scene_lighting.sun_position_dir.xyz);
    let day = scene_lighting.params.x;
    let glint = ggx_sun_glint(normal, view_dir, sun_dir, 0.08) * day * shadow.x;
    rgb += scene_lighting.sun_color.rgb * glint * 0.35;

    if scene_lighting.params.z > 0.5 {
        let absorb = medium_attenuation(2.5, scene_lighting.water_absorption.rgb, scene_lighting.params.w);
        rgb *= absorb;
    }

    // Base opacity: visible water with stronger reflections at grazing angles.
    var alpha = tex.a * mix(0.62, 0.88, fresnel);

    let scene_ndc = sample_scene_depth(screen_uv);
    let thickness = water_column_thickness(scene_ndc, in.world_position);
    if thickness > 0.001 {
        let shallow = clamp(1.0 - exp(-thickness * 0.22), 0.0, 1.0);
        alpha = mix(alpha, clamp(alpha + 0.12 + shallow * 0.18, 0.0, 0.94), shallow);
        rgb = mix(rgb, rgb * vec3(1.06, 1.03, 0.98), shallow * 0.35);
    }

    // --- DEBUG: uncomment ONE line below, rebuild, test on water at grazing angle ---
    // A) copied scene color (should show terrain/sky, NOT black): return vec4(textureSampleLevel(scene_color_tex, scene_color_s, screen_uv, 0.0).rgb, 1.0);
    // B) reflection tier 0..2 as brightness (Planar = white): return vec4(vec3(reflection_params.x / 2.0), 1.0);
    // C) planar enabled? (Ultra = white): return vec4(vec3(reflection_params.y), 1.0);
    // D) planar buffer only: return vec4(textureSampleLevel(planar_color_tex, scene_color_s, screen_uv, 0.0).rgb, 1.0);
    // E) resolve_reflection output only: return vec4(reflected, 1.0);
    // F) green = reflection non-empty: return vec4(mix(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), step(0.02, length(reflected))), 1.0);

    return vec4(rgb, alpha);
}
