#import bevy_pbr::{
    mesh_functions::{get_world_from_local, get_previous_world_from_local, mesh_position_local_to_world, mesh_normal_local_to_world},
    view_transformations::position_world_to_clip,
    prepass_bindings,
    mesh_view_bindings::view,
}

#ifdef PREPASS_FRAGMENT
struct FragmentOutput {
#ifdef NORMAL_PREPASS
    @location(0) normal: vec4<f32>,
#endif

#ifdef MOTION_VECTOR_PREPASS
    @location(1) motion_vector: vec2<f32>,
#endif

#ifdef DEFERRED_PREPASS
    @location(2) deferred: vec4<u32>,
    @location(3) deferred_lighting_pass_id: u32,
#endif

#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    @builtin(frag_depth) frag_depth: f32,
#endif
}
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var entity_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var entity_s: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> entity_light: vec4<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(3) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    @location(1) world_normal: vec3<f32>,
#endif
#ifdef MOTION_VECTOR_PREPASS
    @location(2) previous_world_position: vec4<f32>,
#endif
    @location(3) uv: vec2<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(vertex.instance_index);
    let world_normal = mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
    out.world_position = mesh_position_local_to_world(world_from_local, vec4(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = world_normal;
#endif

#ifdef MOTION_VECTOR_PREPASS
    let prev_model = get_previous_world_from_local(vertex.instance_index);
    out.previous_world_position = mesh_position_local_to_world(prev_model, vec4(vertex.position, 1.0));
#endif

    out.uv = vertex.uv;
    return out;
}

fn apply_cutout_discard(in: VertexOutput) {
    let base = textureSample(entity_tex, entity_s, in.uv);
    if base.a < entity_light.z {
        discard;
    }
}

#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    apply_cutout_discard(in);

    var out: FragmentOutput;

#ifdef NORMAL_PREPASS
    out.normal = vec4(normalize(in.world_normal) * 0.5 + vec3(0.5), 1.0);
#endif

#ifdef MOTION_VECTOR_PREPASS
    let clip_position_t = view.unjittered_clip_from_world * in.world_position;
    let clip_position = clip_position_t.xy / clip_position_t.w;
    let previous_clip_position_t = prepass_bindings::previous_view_uniforms.clip_from_world * in.previous_world_position;
    let previous_clip_position = previous_clip_position_t.xy / previous_clip_position_t.w;
    out.motion_vector = (clip_position - previous_clip_position) * vec2(0.5, -0.5);
#endif

    return out;
}
#else
#ifdef MAY_DISCARD
@fragment
fn fragment(in: VertexOutput) {
    apply_cutout_discard(in);
}
#endif
#endif
