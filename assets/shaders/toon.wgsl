// Starfall's configurable three-band anime material.
//
// The default Bevy mesh vertex stage supplies `VertexOutput`. Keeping this
// shader fragment-only preserves skinning, morph targets, instancing, and the
// renderer's visibility features while replacing the surface response.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view

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
    let light = normalize(toon.light_direction.xyz);
    let ndotl = max(dot(normal, light), 0.0);

    var band = toon.bands.z;
    if ndotl >= toon.bands.y {
        band = 1.0;
    } else if ndotl >= toon.bands.x {
        band = toon.bands.w;
    }

    let view_direction = normalize(view.world_position.xyz - in.world_position.xyz);
    let fresnel = pow(
        clamp(1.0 - max(dot(view_direction, normal), 0.0), 0.0, 1.0),
        max(toon.rim_shape.y, 0.001),
    );
    let rim_mask = step(toon.rim_shape.x, fresnel);
    let rim = toon.rim_color_strength.rgb
        * toon.rim_color_strength.w
        * rim_mask;

    let shaded = toon.color.rgb * band;
    return vec4<f32>(clamp(shaded + rim, vec3<f32>(0.0), vec3<f32>(1.0)), toon.color.a);
}
