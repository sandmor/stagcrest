#define_import_path stagcrest::scene_lighting

// Position directions (scene → celestial body), not incoming light. See scene_lighting_conventions.md.
struct SceneLighting {
    sun_position_dir: vec4<f32>,
    moon_position_dir: vec4<f32>,
    sun_color: vec4<f32>,
    moon_color: vec4<f32>,
    ambient_color: vec4<f32>,
    params: vec4<f32>,
    water_absorption: vec4<f32>,
    horizon_color: vec4<f32>,
}

fn sun_diffuse(normal: vec3<f32>, sun_position_dir: vec3<f32>) -> f32 {
    return max(dot(normalize(normal), normalize(sun_position_dir)), 0.0);
}

fn moon_diffuse(normal: vec3<f32>, moon_position_dir: vec3<f32>) -> f32 {
    return max(dot(normalize(normal), normalize(moon_position_dir)), 0.0) * 0.35;
}

fn biome_mood(albedo: vec3<f32>, ambient_color: vec4<f32>, day_factor: f32) -> vec3<f32> {
    let ambient = ambient_color.rgb * ambient_color.a;
    let night = 1.0 - day_factor;
    return albedo * (vec3<f32>(1.0) + ambient * (0.5 + day_factor * 0.5)) * (0.35 + day_factor * 0.65 + night * 0.15);
}

fn sample_sky_direction(dir: vec3<f32>, lighting: SceneLighting) -> vec3<f32> {
    let up = max(dir.y, 0.0);
    let horizon = 1.0 - abs(dir.y);
    let zenith = mix(lighting.horizon_color.rgb, lighting.ambient_color.rgb, 0.3);
    var col = mix(lighting.horizon_color.rgb, zenith, up);
    col = mix(col, lighting.horizon_color.rgb * 0.6, horizon * 0.4);

    let sun = normalize(lighting.sun_position_dir.xyz);
    let sun_dot = max(dot(normalize(dir), sun), 0.0);
    let sun_glow = pow(sun_dot, 128.0) * lighting.params.x;
    col += lighting.sun_color.rgb * sun_glow * 2.0;

    let moon = normalize(lighting.moon_position_dir.xyz);
    let moon_dot = max(dot(normalize(dir), moon), 0.0);
    let moon_glow = pow(moon_dot, 64.0) * (1.0 - lighting.params.x);
    col += lighting.moon_color.rgb * moon_glow;

    return col;
}

fn medium_attenuation(dist: f32, absorption: vec3<f32>, strength: f32) -> vec3<f32> {
    return exp(-absorption * dist * strength);
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

fn shade_voxel(
    albedo: vec3<f32>,
    normal: vec3<f32>,
    sky_level: f32,
    block_level: f32,
    ao: f32,
    emissive: bool,
    lighting: SceneLighting,
) -> vec3<f32> {
    if emissive {
        return albedo * (1.2 + block_level * 0.8);
    }
    let day = lighting.params.x;
    let sun = sun_diffuse(normal, lighting.sun_position_dir.xyz) * day;
    let moon = moon_diffuse(normal, lighting.moon_position_dir.xyz) * (1.0 - day);
    let sky_contrib = sky_level * (sun + moon + 0.15);
    let block_contrib = block_level;
    let lit = albedo * (block_contrib + sky_contrib) * ao;
    return biome_mood(lit, lighting.ambient_color, day);
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
