#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions::get_world_from_local,
    view_transformations::position_world_to_clip,
}

struct VertexInput {
    @location(0) position: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(0u);
    let world_position = world_from_local * vec4(in.position, 1.0);
    out.clip_position = position_world_to_clip(world_position.xyz);
    // Nudge depth toward the camera so the outline renders in front of
    // coplanar block geometry (prevents z-fighting on overlay faces).
    out.clip_position.z += 0.0005 * out.clip_position.w;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return color;
}
