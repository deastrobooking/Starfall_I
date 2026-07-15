// Texture-free snow and ice surface for cold biome terrain and Forge previews.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{view, lights}

struct IceSettings {
    snow_color: vec4<f32>,
    ice_color: vec4<f32>,
    // X detail scale, Y frost coverage, Z sparkle strength, W Fresnel strength.
    surface: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> ice: IceSettings;

fn surface_noise(position: vec3<f32>) -> f32 {
    let p = position * ice.surface.x;
    let first = sin(dot(p, vec3<f32>(0.83, 0.37, 0.61)));
    let second = cos(dot(p, vec3<f32>(0.29, 0.91, 0.47)) * 1.73);
    return first * 0.5 + second * 0.5;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let view_direction = normalize(view.world_position.xyz - in.world_position.xyz);

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
    let ndotl = max(dot(normal, light), 0.0);
    let diffuse = mix(0.34, 1.0, ndotl);
    let noise = surface_noise(in.world_position.xyz);
    let upward = smoothstep(0.20, 0.82, normal.y);
    let frost = smoothstep(ice.surface.y - 0.22, ice.surface.y + 0.22, noise * 0.5 + upward * 0.5);
    let fresnel = pow(clamp(1.0 - max(dot(view_direction, normal), 0.0), 0.0, 1.0), 3.2);
    let sparkle_seed = sin(dot(in.world_position.xyz, vec3<f32>(31.7, 17.3, 23.9)));
    let sparkle = step(0.985, sparkle_seed) * step(0.55, ndotl) * ice.surface.z;
    let base = mix(ice.ice_color.rgb, ice.snow_color.rgb, frost);
    let edge = ice.ice_color.rgb * fresnel * ice.surface.w;
    let color = base * diffuse * mix(vec3<f32>(1.0), light_tint, 0.24) + edge + sparkle;
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
