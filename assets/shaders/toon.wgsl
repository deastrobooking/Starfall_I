// Starfall's configurable three-band anime material.
//
// The default Bevy mesh vertex stage supplies `VertexOutput`. Keeping this
// shader fragment-only preserves skinning, morph targets, instancing, and the
// renderer's visibility features while replacing the surface response.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{view, lights}

struct ToonSettings {
    color: vec4<f32>,
    light_direction: vec4<f32>,
    // X/Y: band thresholds. Z/W: shadow and mid-tone intensity.
    bands: vec4<f32>,
    // RGB: rim tint. W: rim strength.
    rim_color_strength: vec4<f32>,
    // X: rim threshold. Y: rim exponent.
    rim_shape: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> toon: ToonSettings;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    var direction_sum = toon.light_direction.xyz * 0.0001;
    var color_sum = vec3<f32>(0.0001);
    for (var index = 0u; index < lights.n_directional_lights; index += 1u) {
        let scene_light = lights.directional_lights[index];
        let energy = max(max(scene_light.color.r, scene_light.color.g), scene_light.color.b);
        direction_sum += scene_light.direction_to_light * energy;
        color_sum += scene_light.color.rgb;
    }
    let light = normalize(direction_sum);
    let light_tint = color_sum / max(max(color_sum.r, color_sum.g), color_sum.b);
    let ndotl = max(dot(normal, light), 0.0);

    // Three crisp bands without per-fragment control-flow divergence.
    let shadow_to_mid = step(toon.bands.x, ndotl);
    let mid_to_light = step(toon.bands.y, ndotl);
    let band = toon.bands.z
        + shadow_to_mid * (toon.bands.w - toon.bands.z)
        + mid_to_light * (1.0 - toon.bands.w);

    let view_direction = normalize(view.world_position.xyz - in.world_position.xyz);
    let fresnel = pow(
        clamp(1.0 - max(dot(view_direction, normal), 0.0), 0.0, 1.0),
        max(toon.rim_shape.y, 0.001),
    );
    let rim_mask = step(toon.rim_shape.x, fresnel)
        * smoothstep(0.0, 0.24, ndotl);
    let rim = toon.rim_color_strength.rgb
        * toon.rim_color_strength.w
        * rim_mask;

    let ambient_tint = lights.ambient_color.rgb
        / max(max(max(lights.ambient_color.r, lights.ambient_color.g), lights.ambient_color.b), 0.0001);
    let scene_tint = mix(ambient_tint, light_tint, 0.72);
    let shaded = toon.color.rgb * band * mix(vec3<f32>(1.0), scene_tint, 0.22);
    return vec4<f32>(clamp(shaded + rim, vec3<f32>(0.0), vec3<f32>(1.0)), toon.color.a);
}
