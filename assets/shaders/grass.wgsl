// Wind-swaying grass blade shader.
//
// Vertex stage: applies a sin-wave sway in world-X that is zero at the blade
// root (local y ≤ 0) and grows toward the tip, giving the classic old-school
// grass animation without any CPU updates per frame.
//
// Fragment stage: Lambert diffuse against a fixed sun direction, with abs()
// on the dot product so both faces of the double-sided quad are lit.

#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_normal_local_to_world}

const WIND_SPEED:     f32 = 1.30;   // cycles per second
const WIND_STRENGTH:  f32 = 0.13;   // world-unit sway amplitude at the tip
const WIND_FREQUENCY: f32 = 0.60;   // spatial variation (world-X phase)

// Material uniform: one RGBA colour per blade variant.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       world_normal:  vec3<f32>,
}

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = get_world_from_local(in.instance_index);
    var world_pos        = world_from_local * vec4<f32>(in.position, 1.0);

    // Blades are 0.52–0.72 tall; local y runs from -half_h (root) to +half_h (tip).
    // Clamp so negative-y root vertices get zero sway and tip gets full sway.
    let sway_t = clamp(in.position.y / 0.4, 0.0, 1.0);
    let phase  = globals.time * WIND_SPEED + world_pos.x * WIND_FREQUENCY;
    let sway   = sin(phase) * WIND_STRENGTH * sway_t;
    world_pos.x += sway;
    world_pos.z += sway * 0.35;   // slight Z component for more organic feel

    out.clip_position = view.clip_from_world * world_pos;
    out.world_normal  = mesh_normal_local_to_world(in.normal, in.instance_index);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Fixed sun direction — roughly matches the directional light in spawn_lighting.
    let sun     = normalize(vec3<f32>(0.45, 1.0, 0.30));
    // abs() ensures the back face is lit the same as the front (double-sided).
    let diffuse = mix(0.22, 1.0, abs(dot(normalize(in.world_normal), sun)));
    return vec4<f32>(color.rgb * diffuse, 1.0);
}
