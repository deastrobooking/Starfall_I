// Configurable, scene-lit grass for Starfall's large outdoor world.
// Wind is vertex-only and spatially phased, so the CPU never updates blades.

#import bevy_pbr::mesh_view_bindings::{view, globals, lights}
#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_normal_local_to_world}

struct GrassSettings {
    tip_color: vec4<f32>,
    root_color: vec4<f32>,
    // X speed, Y strength, Z spatial frequency, W secondary-wave scale.
    wind: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> grass: GrassSettings;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) height_mix: f32,
};

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(in.instance_index);
    var world_position = world_from_local * vec4<f32>(in.position, 1.0);

    // Bevy Rectangle UVs are 0 at the top and 1 at the bottom. This keeps the
    // root fixed for any blade height instead of relying on a mesh magic number.
    let height_mix = clamp(1.0 - in.uv.y, 0.0, 1.0);
    let phase = globals.time * grass.wind.x
        + world_position.x * grass.wind.z
        + world_position.z * grass.wind.z * 0.73;
    let primary = sin(phase);
    let secondary = sin(phase * 1.71 + world_position.z * 0.19);
    let sway = (primary + secondary * 0.32) * grass.wind.y * height_mix * height_mix;
    world_position.x += sway;
    world_position.z += sway * grass.wind.w;

    // A small vertical normal correction follows the bend so highlights do not
    // remain completely static while the blade moves.
    let base_normal = mesh_normal_local_to_world(in.normal, in.instance_index);
    out.world_normal = normalize(base_normal + vec3<f32>(0.0, -sway * 0.8, 0.0));
    out.position = view.clip_from_world * world_position;
    out.height_mix = height_mix;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    var direction_sum = vec3<f32>(0.45, 1.0, 0.30) * 0.0001;
    var color_sum = vec3<f32>(0.0001);
    for (var index = 0u; index < lights.n_directional_lights; index += 1u) {
        let scene_light = lights.directional_lights[index];
        let energy = max(max(scene_light.color.r, scene_light.color.g), scene_light.color.b);
        direction_sum += scene_light.direction_to_light * energy;
        color_sum += scene_light.color.rgb;
    }
    let light = normalize(direction_sum);
    let light_tint = color_sum / max(max(color_sum.r, color_sum.g), color_sum.b);
    let ndotl = abs(dot(normal, light));
    let diffuse = mix(0.30, 1.0, ndotl);
    let blade_color = mix(grass.root_color.rgb, grass.tip_color.rgb, in.height_mix);
    let scene_tint = mix(vec3<f32>(1.0), light_tint, 0.18);
    return vec4<f32>(blade_color * diffuse * scene_tint, grass.tip_color.a);
}
