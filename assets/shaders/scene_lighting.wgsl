#define_import_path stagcrest::scene_lighting

#import bevy_pbr::{
    shadows,
}

// Position directions (scene → celestial body), not incoming light. See RENDERING.md §5.5.
struct SceneLighting {
    sun_position_dir: vec4<f32>,
    moon_position_dir: vec4<f32>,
    sun_color: vec4<f32>,
    moon_color: vec4<f32>,
    ambient_color: vec4<f32>,
    params: vec4<f32>,
    water_absorption: vec4<f32>,
    horizon_color: vec4<f32>,
    zenith_color: vec4<f32>,
    sky_params: vec4<f32>,
    shadow_params: vec4<f32>,
}

fn decode_normal_axis(axis: u32) -> vec3<f32> {
    switch axis {
        case 0u: { return vec3<f32>(0.0, 0.0, 1.0); }
        case 1u: { return vec3<f32>(0.0, 0.0, -1.0); }
        case 2u: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 3u: { return vec3<f32>(-1.0, 0.0, 0.0); }
        case 4u: { return vec3<f32>(0.0, 1.0, 0.0); }
        case 5u: { return vec3<f32>(0.0, -1.0, 0.0); }
        default: { return vec3<f32>(0.0, 1.0, 0.0); }
    }
}

fn unpack_light(light: u32) -> vec2<f32> {
    let sky = f32((light >> 4u) & 0xFu);
    let block = f32(light & 0xFu);
    return vec2<f32>(sky / 15.0, block / 15.0);
}

fn medium_attenuation(dist: f32, absorption: vec3<f32>, strength: f32) -> vec3<f32> {
    return exp(-absorption * dist * strength);
}

// Procedural star hash on the inner sky sphere.
fn star_field(dir: vec3<f32>, elapsed: f32, strength: f32) -> vec3<f32> {
    if strength <= 0.001 {
        return vec3(0.0);
    }
    let p = dir * 120.0 + vec3(17.0, 31.0, 53.0);
    let h = fract(sin(dot(floor(p), vec3(12.9898, 78.233, 45.164))) * 43758.5453);
    let star = step(0.992, h) * strength;
    let twinkle = 0.85 + 0.15 * sin(elapsed * 1.7 + h * 80.0);
    return vec3(star * twinkle);
}

fn sample_sky_direction(dir: vec3<f32>, lighting: SceneLighting) -> vec3<f32> {
    let n = normalize(dir);
    let up = max(n.y, 0.0);
    let horizon = 1.0 - abs(n.y);
    let sunset = lighting.sky_params.x;
    let stars = lighting.sky_params.y;
    let day = lighting.params.x;

    let zenith = lighting.zenith_color.rgb;
    let horizon_col = lighting.horizon_color.rgb;
    var col = mix(horizon_col, zenith, pow(up, 0.65));
    col = mix(col, horizon_col * vec3(1.1, 0.75, 0.45), horizon * sunset * 0.85);
    col *= 0.12 + 0.88 * day;

    // Sunset scatter lobe around the sun near the horizon.
    let sun = normalize(lighting.sun_position_dir.xyz);
    let sun_dot = max(dot(n, sun), 0.0);
    let sun_disc = lighting.sun_position_dir.w;
    let sun_glow = pow(sun_dot, 96.0) * max(day, sun_disc * 0.35) * sun_disc;
    col += lighting.sun_color.rgb * sun_glow * (2.5 + sunset * 2.0);

    let scatter = pow(sun_dot, 8.0) * sunset * (1.0 - up) * 0.65;
    col += vec3(1.0, 0.45, 0.15) * scatter;

    let moon = normalize(lighting.moon_position_dir.xyz);
    let moon_dot = max(dot(n, moon), 0.0);
    let moon_disc = lighting.moon_position_dir.w;
    let moon_above = smoothstep(-0.02, 0.08, lighting.moon_position_dir.y);
    let moon_glow = pow(moon_dot, 72.0) * (1.0 - day) * moon_disc * moon_above;
    col += lighting.moon_color.rgb * moon_glow;

    col += star_field(n, lighting.sky_params.w, stars * (1.0 - horizon * 0.35));

    return col;
}

fn fetch_celestial_shadows(
    lighting: SceneLighting,
    world_pos: vec4<f32>,
    normal: vec3<f32>,
    view_z: f32,
    frag_coord: vec2<f32>,
) -> vec2<f32> {
    var sun_s = 1.0;
    var moon_s = 1.0;
    let sun_weight = lighting.shadow_params.z;
    let moon_weight = lighting.shadow_params.w;
    if (sun_weight > 0.001) {
        let sampled = shadows::fetch_directional_shadow(
            u32(lighting.shadow_params.x),
            world_pos,
            normal,
            view_z,
            frag_coord,
        );
        sun_s = mix(1.0, sampled, sun_weight);
    }
    if (moon_weight > 0.001) {
        let sampled = shadows::fetch_directional_shadow(
            u32(lighting.shadow_params.y),
            world_pos,
            normal,
            view_z,
            frag_coord,
        );
        moon_s = mix(1.0, sampled, moon_weight);
    }
    return vec2(sun_s, moon_s);
}

/// Hybrid voxel lighting: baked skylight/blocklight ambient + shadow-mapped direct sun/moon.
fn shade_voxel(
    albedo: vec3<f32>,
    normal: vec3<f32>,
    sky_level: f32,
    block_level: f32,
    ao: f32,
    emissive: bool,
    lighting: SceneLighting,
    sun_shadow: f32,
    moon_shadow: f32,
) -> vec3<f32> {
    if emissive {
        return albedo * (1.3 + block_level * 0.9);
    }

    let day = lighting.params.x;
    let twilight = lighting.sky_params.x;
    let day_soft = mix(day, step(0.0, lighting.sun_position_dir.y), twilight * 0.3);
    let night = 1.0 - day_soft;
    let n = normalize(normal);

    let sun_dir = normalize(lighting.sun_position_dir.xyz);
    let moon_dir = normalize(lighting.moon_position_dir.xyz);
    let sun_ndotl = max(dot(n, sun_dir), 0.0);
    let moon_ndotl = max(dot(n, moon_dir), 0.0) * 0.4;

    let ambient = lighting.ambient_color.rgb * lighting.ambient_color.a;
    let baked_ambient = sky_level * ambient * (0.42 + day * 0.58);
    let torch = block_level * (0.85 + block_level * 0.35);

    let direct_sun = sun_ndotl * day_soft * sun_shadow * lighting.sun_color.rgb * 1.15;
    let direct_moon = moon_ndotl * night * moon_shadow * lighting.moon_color.rgb;

    let lit = albedo * (baked_ambient + torch + direct_sun + direct_moon) * ao;
    let tint = mix(vec3(1.0), lighting.sun_color.rgb, day_soft * 0.22 + lighting.sky_params.x * 0.35);
    return lit * tint * (0.5 + day_soft * 0.38 + night * 0.12);
}

fn encode_normal(normal: vec3<f32>) -> u32 {
    if normal.y > 0.5 { return 4u; }
    if normal.y < -0.5 { return 5u; }
    if normal.x > 0.5 { return 2u; }
    if normal.x < -0.5 { return 3u; }
    if normal.z > 0.5 { return 0u; }
    return 1u;
}

fn pack_light(sky: u32, block: u32) -> u32 {
    return ((sky & 0xFu) << 4u) | (block & 0xFu);
}
