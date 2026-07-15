// Procedural energy surface for beams, weapon cores, and magic projectiles.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{view, globals}

struct EnergySettings {
    core_color: vec4<f32>,
    edge_color: vec4<f32>,
    // X flow speed, Y spatial frequency, Z pulse speed, W opacity.
    motion: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> energy: EnergySettings;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let view_direction = normalize(view.world_position.xyz - in.world_position.xyz);
    let rim = pow(1.0 - max(dot(view_direction, normal), 0.0), 2.2);
    let flow_phase = dot(in.world_position.xyz, vec3<f32>(0.37, 0.91, 0.23))
        * energy.motion.y
        - globals.time * energy.motion.x;
    let flow = sin(flow_phase) * 0.5 + 0.5;
    let pulse = sin(globals.time * energy.motion.z) * 0.12 + 0.88;
    let core = smoothstep(0.18, 0.82, flow) * pulse;
    let color = mix(energy.core_color.rgb, energy.edge_color.rgb, rim);
    let intensity = 0.72 + core * 0.78 + rim * 0.65;
    let alpha = energy.motion.w * clamp(0.48 + core * 0.34 + rim * 0.28, 0.0, 1.0);
    return vec4<f32>(color * intensity, alpha);
}
