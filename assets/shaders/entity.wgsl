#import bevy_pbr::{
    mesh_functions::get_world_from_local,
    view_transformations::position_world_to_clip,
}
#import stagcrest::scene_lighting::{SceneLighting, shade_voxel}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var entity_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var entity_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> scene_lighting: SceneLighting;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> entity_light: vec4<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(vertex.instance_index);
    let world_position = world_from_local * vec4(vertex.position, 1.0);
    out.clip_position = position_world_to_clip(world_position.xyz);
    // Rigid transforms (rotation + uniform scale), so the model matrix suffices
    // for the normal without an inverse-transpose.
    out.world_normal = normalize((world_from_local * vec4(vertex.normal, 0.0)).xyz);
    out.uv = vertex.uv;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let base = textureSample(entity_tex, entity_s, in.uv);
    if base.a < entity_light.z {
        discard;
    }
    let normal = normalize(in.world_normal);
    let rgb = shade_voxel(
        base.rgb,
        normal,
        entity_light.x,
        entity_light.y,
        1.0,
        false,
        scene_lighting,
    );
    return vec4(rgb, base.a);
}
