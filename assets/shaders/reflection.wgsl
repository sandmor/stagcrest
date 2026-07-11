#define_import_path stagcrest::reflection

#import bevy_pbr::{
    mesh_view_bindings::view,
    view_transformations::{
        position_world_to_view,
        position_view_to_clip,
        direction_world_to_view,
    },
}
#import stagcrest::scene_lighting::{SceneLighting, sample_sky_direction}

const SKY_DEPTH: f32 = 0.0001;

fn ndc_to_uv(ndc: vec2<f32>) -> vec2<f32> {
    return ndc * vec2(0.5, -0.5) + vec2(0.5);
}

fn sample_scene_color(uv: vec2<f32>, tex: texture_2d<f32>, samp: sampler) -> vec3<f32> {
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return vec3(0.0);
    }
    return textureSampleLevel(tex, samp, uv, 0.0).rgb;
}

fn sample_scene_depth(uv: vec2<f32>, tex: texture_2d<f32>) -> f32 {
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 0.0;
    }
    let dims = textureDimensions(tex, 0);
    let coord = vec2<i32>(clamp(uv * vec2<f32>(dims), vec2(0.0), vec2<f32>(dims) - vec2(1.0, 1.0)));
    return textureLoad(tex, coord, 0).r;
}

fn ssr_edge_fade(uv: vec2<f32>) -> f32 {
    let r_edge = vec2(0.6, 0.55);
    let abs_pos = abs(uv - vec2(0.5));
    let cdist = abs_pos / r_edge;
    return clamp(1.0 - pow(max(cdist.x, cdist.y), 50.0), 0.0, 1.0);
}

// View-space SSR with binary refinement (MCExample2 Method 1, Bevy reverse-Z).
fn trace_ssr(
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
    view_dir: vec3<f32>,
    fresnel: f32,
    scene_color: texture_2d<f32>,
    scene_depth: texture_2d<f32>,
    scene_sampler: sampler,
    max_steps: i32,
) -> vec3<f32> {
    let view_start = position_world_to_view(world_pos);
    let view_normal = normalize(direction_world_to_view(world_normal));
    let world_reflect = normalize(reflect(-view_dir, world_normal));
    let view_reflect = normalize(direction_world_to_view(world_reflect));

    var start = view_start + view_normal * (0.05 + 0.025 * (1.0 - fresnel));
    var vector = view_reflect * 0.5;
    var tvector = vector;
    var view_pos_rt = start + vector;
    var sr = 0;
    var hit_uv = vec2(0.0);
    var hit = false;

    for (var i: i32 = 0; i < max_steps; i++) {
        let clip = position_view_to_clip(view_pos_rt);
        if clip.w <= 0.0 {
            break;
        }
        let ndc = clip.xyz / clip.w;
        if ndc.z < 0.0 || ndc.z > 1.0 {
            break;
        }
        let uv = ndc_to_uv(ndc.xy);
        if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
            break;
        }

        let scene_z = sample_scene_depth(uv, scene_depth);
        // Skip sky pixels — no geometry to reflect.
        if scene_z <= SKY_DEPTH {
            vector *= 2.0;
            tvector += vector;
            view_pos_rt = start + tvector;
            continue;
        }

        // Reverse-Z: nearer surfaces have larger depth. Hit when the sample passes behind the surface.
        if ndc.z < scene_z - 0.001 && ndc.z > scene_z - 0.35 {
            sr += 1;
            if sr >= 4 {
                hit_uv = uv;
                hit = true;
                break;
            }
            tvector -= vector;
            vector *= 0.15;
        } else {
            vector *= 2.0;
            tvector += vector;
        }
        view_pos_rt = start + tvector;
    }

    if !hit {
        return vec3(0.0);
    }

    let border = ssr_edge_fade(hit_uv);
    if border <= 0.001 {
        return vec3(0.0);
    }

    let color = sample_scene_color(hit_uv, scene_color, scene_sampler);
    let edge = abs(hit_uv - vec2(0.5)) / vec2(0.6, 0.55);
    let edge_factor = 1.0 - pow(max(edge.x, edge.y), 4.0);
    return color * border * edge_factor;
}

fn resolve_reflection(
    world_pos: vec3<f32>,
    normal: vec3<f32>,
    view_dir: vec3<f32>,
    screen_uv: vec2<f32>,
    lighting: SceneLighting,
    scene_color: texture_2d<f32>,
    scene_depth: texture_2d<f32>,
    planar_color: texture_2d<f32>,
    scene_sampler: sampler,
    params: vec4<f32>, // x=tier, y=has_planar, z=resolution_scale, w=time
) -> vec3<f32> {
    let tier = params.x;
    let has_planar = params.y > 0.5;
    let fresnel = pow(1.0 - max(dot(normal, view_dir), 0.0), 4.0);

    var sky = sample_sky_direction(normalize(reflect(-view_dir, normal)), lighting);

    if tier < 0.5 {
        return sky;
    }

    var color = vec3(0.0);
    if tier >= 0.5 {
        color = trace_ssr(
            world_pos,
            normal,
            view_dir,
            fresnel,
            scene_color,
            scene_depth,
            scene_sampler,
            20,
        );
    }

    if length(color) < 0.001 && has_planar && tier >= 1.5 {
        let planar = textureSampleLevel(planar_color, scene_sampler, screen_uv, 0.0).rgb;
        if length(planar) > 0.001 {
            color = planar;
        }
    }

    if length(color) < 0.001 {
        return sky;
    }
    return mix(sky, color, 0.9);
}
