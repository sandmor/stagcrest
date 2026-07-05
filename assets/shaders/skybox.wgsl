#import bevy_render::view::View
#import bevy_pbr::utils::coords_to_viewport_uv
#import stagcrest::scene_lighting::{SceneLighting, sample_sky_direction}

struct SkyMaterial {
    scene_lighting: SceneLighting,
}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> sky_material: SkyMaterial;

fn coords_to_ray_direction(position: vec2<f32>, viewport: vec4<f32>) -> vec3<f32> {
    let view_position_homogeneous = view.view_from_clip * vec4(
        coords_to_viewport_uv(position, viewport) * vec2(2.0, -2.0) + vec2(-1.0, 1.0),
        1.0,
        1.0,
    );
    var view_ray_direction = view_position_homogeneous.xyz / view_position_homogeneous.w;
    view_ray_direction = (view.world_from_view * vec4(view_ray_direction, 0.0)).xyz;
    return normalize(view_ray_direction);
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let clip_position = vec2(
        f32(vertex_index & 1u),
        f32((vertex_index >> 1u) & 1u),
    ) * 4.0 - vec2(1.0);
    return VertexOutput(vec4(clip_position, 0.0, 1.0));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let lighting = sky_material.scene_lighting;
    let dir = coords_to_ray_direction(in.position.xy, view.viewport);
    let col = sample_sky_direction(dir, lighting);
    return vec4(col, 1.0);
}
