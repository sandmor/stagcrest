#import bevy_pbr::{
    mesh_functions::get_world_from_local,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}
#import stagcrest::scene_lighting::{SceneLighting, fetch_celestial_shadows, shade_voxel}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var entity_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var entity_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> scene_lighting: SceneLighting;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> entity_light: vec4<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(3) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_position: vec3<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(vertex.instance_index);
    let world_position = world_from_local * vec4(vertex.position, 1.0);
    out.clip_position = position_world_to_clip(world_position.xyz);
    out.world_position = world_position.xyz;
    out.world_normal = normalize((world_from_local * vec4(vertex.normal, 0.0)).xyz);
    out.uv = vertex.uv;
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
    let base = textureSample(entity_tex, entity_s, in.uv);
    if base.a < entity_light.z {
        discard;
    }
    let normal = normalize(in.world_normal);
    let world_pos = vec4(in.world_position, 1.0);
    let shadow = fetch_shadow(world_pos, normal, in.clip_position.xy);
    let rgb = shade_voxel(
        base.rgb,
        normal,
        entity_light.x,
        entity_light.y,
        1.0,
        false,
        scene_lighting,
        shadow.x,
        shadow.y,
    );
    return vec4(rgb, base.a);
}
