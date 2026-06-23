#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: UnderwaterSettings;

struct UnderwaterSettings {
    tint_strength: vec4<f32>,
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let strength = settings.tint_strength.w;
    let tint = settings.tint_strength.rgb;

    let color = textureSample(screen_texture, screen_sampler, in.uv);
    let tinted = mix(color.rgb, color.rgb * tint, strength * 0.6);
    let murk = mix(tinted, tint * 0.35, strength * 0.45);
    let darkened = murk * mix(1.0, 0.55, strength);

    return vec4(darkened, color.a);
}
