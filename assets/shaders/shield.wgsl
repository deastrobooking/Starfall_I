// Transparent hex-energy shield with view-angle edge emphasis.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{view, globals}

struct ShieldSettings {
    color: vec4<f32>,
    // X grid scale, Y line width, Z pulse speed, W opacity.
    pattern: vec4<f32>,
    // X Fresnel exponent, Y edge strength, Z impact pulse, W reserved.
    edge: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> shield: ShieldSettings;

fn hex_lines(position: vec2<f32>) -> f32 {
    let p = position * shield.pattern.x;
    let q = vec2<f32>(p.x * 0.57735027 - p.y * 0.33333333, p.y * 0.66666667);
    let cell = abs(fract(q) - vec2<f32>(0.5));
    let edge_distance = max(cell.x * 0.8660254 + cell.y * 0.5, cell.y);
    return smoothstep(0.5 - shield.pattern.y, 0.5, edge_distance);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let view_direction = normalize(view.world_position.xyz - in.world_position.xyz);
    let fresnel = pow(
        clamp(1.0 - max(dot(view_direction, normal), 0.0), 0.0, 1.0),
        max(shield.edge.x, 0.001),
    );
    let projection_x = hex_lines(in.world_position.yz);
    let projection_y = hex_lines(in.world_position.xz);
    let projection_z = hex_lines(in.world_position.xy);
    let weights = abs(normal);
    let grid = (projection_x * weights.x + projection_y * weights.y + projection_z * weights.z)
        / max(weights.x + weights.y + weights.z, 0.001);
    let pulse = sin(globals.time * shield.pattern.z) * 0.12 + 0.88;
    let impact = shield.edge.z * exp(-fract(globals.time * 0.8) * 5.0);
    let intensity = grid * 0.44 * pulse + fresnel * shield.edge.y + impact;
    let alpha = shield.pattern.w * clamp(0.20 + grid * 0.42 + fresnel * 0.72, 0.0, 1.0);
    return vec4<f32>(shield.color.rgb * intensity, alpha);
}
