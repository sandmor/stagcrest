#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var depth_texture: texture_depth_2d;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let dims = textureDimensions(depth_texture, 0);
    let coord = vec2<i32>(in.uv * vec2<f32>(dims));
    let depth = textureLoad(depth_texture, coord, 0);
    return vec4(depth, 0.0, 0.0, 1.0);
}
