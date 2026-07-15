// Lightweight procedural water designed for repeated split-screen rendering.

#import bevy_pbr::mesh_view_bindings::{view, globals, lights}
#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_normal_local_to_world}

struct WaterSettings {
    deep_color: vec4<f32>,
    shallow_color: vec4<f32>,
    // X speed, Y frequency, Z amplitude, W secondary-wave scale.
    wave: vec4<f32>,
    // X Fresnel exponent, Y base opacity, Z edge opacity, W light tint.
    surface: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> water: WaterSettings;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) wave_mix: f32,
};

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(in.instance_index);
    var world_position = world_from_local * vec4<f32>(in.position, 1.0);
    let base_normal = normalize(mesh_normal_local_to_world(in.normal, in.instance_index));
    let upness = abs(base_normal.y);
    let phase_x = world_position.x * water.wave.y + globals.time * water.wave.x;
    let phase_z = world_position.z * water.wave.y * 0.81 - globals.time * water.wave.x * 0.73;
    let first = sin(phase_x);
    let second = cos(phase_z) * water.wave.w;
    let displacement = (first + second) * water.wave.z * upness;
    world_position.y += displacement;

    let slope_x = cos(phase_x) * water.wave.y * water.wave.z;
    let slope_z = -sin(phase_z) * water.wave.y * 0.81 * water.wave.z * water.wave.w;
    let wave_normal = normalize(vec3<f32>(-slope_x, 1.0, -slope_z));
    out.world_normal = normalize(mix(base_normal, wave_normal, upness));
    out.world_position = world_position;
    out.wave_mix = first * 0.5 + second * 0.5 + 0.5;
    out.position = view.clip_from_world * world_position;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let view_direction = normalize(view.world_position.xyz - in.world_position.xyz);
    let fresnel = pow(
        clamp(1.0 - max(dot(view_direction, normal), 0.0), 0.0, 1.0),
        max(water.surface.x, 0.001),
    );

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
    let diffuse = mix(0.38, 1.0, max(dot(normal, light), 0.0));
    let depth_mix = clamp(0.28 + fresnel * 0.58 + in.wave_mix * 0.14, 0.0, 1.0);
    let base = mix(water.deep_color.rgb, water.shallow_color.rgb, depth_mix);
    let lit = base * diffuse * mix(vec3<f32>(1.0), light_tint, water.surface.w);
    let crest = water.shallow_color.rgb * step(0.86, in.wave_mix) * 0.18;
    let alpha = mix(water.surface.y, water.surface.z, fresnel);
    return vec4<f32>(clamp(lit + crest, vec3<f32>(0.0), vec3<f32>(1.0)), alpha);
}
