#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// ViewTarget HDR color is unfilterable; use textureLoad (same as Bevy tonemapping).
@group(0) @binding(0) var source_texture: texture_2d<f32>;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let dims = textureDimensions(source_texture, 0);
    let coord = vec2<i32>(
        clamp(
            vec2<i32>(in.uv * vec2<f32>(dims)),
            vec2<i32>(0),
            vec2<i32>(dims) - vec2<i32>(1, 1),
        ),
    );
    return textureLoad(source_texture, coord, 0);
}
