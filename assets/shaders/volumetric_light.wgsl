#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct VolumetricLightSettings {
    sun_screen: vec4<f32>,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var scene_depth_tex: texture_depth_2d;
@group(0) @binding(3) var<uniform> settings: VolumetricLightSettings;

const SKY_DEPTH: f32 = 0.0001;

// 1.0 if the texel is sky (far plane in reverse-Z), 0.0 if geometry occludes it.
fn texel_unoccluded(coord: vec2<i32>, max_coord: vec2<i32>) -> f32 {
    let c = clamp(coord, vec2(0, 0), max_coord);
    let depth = textureLoad(scene_depth_tex, c, 0);
    return 1.0 - step(SKY_DEPTH, depth);
}

// Bilinearly filtered sky occlusion. Filtering in the occlusion domain (not the raw
// depth) turns hard silhouette edges into smooth 0..1 gradients, so the TAA-jittered
// depth prepass no longer flips whole steps on/off each frame (the "lightning" flash).
fn sky_occlusion(uv: vec2<f32>) -> f32 {
    let dims = vec2<f32>(textureDimensions(scene_depth_tex, 0));
    let max_coord = vec2<i32>(dims) - vec2(1, 1);
    let p = uv * dims - vec2(0.5);
    let base = floor(p);
    let f = p - base;
    let b = vec2<i32>(base);
    let c00 = texel_unoccluded(b + vec2(0, 0), max_coord);
    let c10 = texel_unoccluded(b + vec2(1, 0), max_coord);
    let c01 = texel_unoccluded(b + vec2(0, 1), max_coord);
    let c11 = texel_unoccluded(b + vec2(1, 1), max_coord);
    return mix(mix(c00, c10, f.x), mix(c01, c11, f.x), f.y);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let strength = settings.sun_screen.w;
    let scene = textureSample(screen_texture, screen_sampler, in.uv).rgb;

    if strength <= 0.001 {
        return vec4(scene, 1.0);
    }

    let sun = settings.sun_screen.xy;
    let steps = 14;
    var accum = 0.0;
    var weight = 1.0;

    for (var i: i32 = 0; i < steps; i++) {
        let t = (f32(i) + 0.37) / f32(steps);
        let sample_uv = mix(in.uv, sun, t * 0.88);
        // Bilinear sky occlusion is temporally stable, so a still camera produces no flashing.
        accum += sky_occlusion(sample_uv) * weight;
        weight *= 0.82;
    }
    accum /= f32(steps);

    let dist = length(in.uv - sun);
    // Tight hotspot around the sun; ignore distant screen regions.
    if dist > 0.42 {
        return vec4(scene, 1.0);
    }
    let radial = exp(-dist * dist * 22.0) * strength;
    let occlusion = pow(clamp(accum, 0.0, 1.0), 1.6);
    let shafts = occlusion * radial * vec3(1.0, 0.86, 0.55) * 0.28;

    // --- DEBUG: uncomment ONE line below ---
    // G) god-ray strength (daytime should be bright): return vec4(vec3(strength * 3.0), 1.0);
    // H) sun screen position (red dot on sun): if length(in.uv - sun) < 0.04 { return vec4(1.0, 0.0, 0.0, 1.0); } return vec4(scene, 1.0);
    // I) depth occlusion along march (white = unoccluded): return vec4(vec3(accum * 3.0), 1.0);

    return vec4(scene + shafts, 1.0);
}
