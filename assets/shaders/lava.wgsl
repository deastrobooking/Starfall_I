// Texture-free animated lava with cooled crust cells and emissive seams.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{globals, lights}

struct LavaSettings {
    crust_color: vec4<f32>,
    hot_color: vec4<f32>,
    // X flow speed, Y cell scale, Z seam width, W emissive strength.
    motion: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> lava: LavaSettings;

fn lava_field(position: vec3<f32>) -> f32 {
    let flow = vec3<f32>(globals.time * lava.motion.x, 0.0, -globals.time * lava.motion.x * 0.63);
    let p = (position + flow) * lava.motion.y;
    let a = sin(p.x + cos(p.z * 1.31));
    let b = cos(p.z * 0.83 - sin(p.x * 1.57));
    return abs(a * b);
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
    let diffuse = mix(0.28, 0.88, max(dot(normal, light), 0.0));
    let field = lava_field(in.world_position.xyz);
    let seam = 1.0 - smoothstep(lava.motion.z, lava.motion.z + 0.20, field);
    let crust = lava.crust_color.rgb * diffuse * mix(vec3<f32>(1.0), light_tint, 0.20);
    let heat_pulse = sin(globals.time * 2.4 + in.world_position.x * 0.13) * 0.10 + 0.90;
    let molten = lava.hot_color.rgb * lava.motion.w * heat_pulse;
    return vec4<f32>(crust + molten * seam, 1.0);
}
