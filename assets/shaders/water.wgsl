// Procedural scene-lit water designed for repeated split-screen rendering.
// It avoids screen textures while retaining layered waves, analytic normals,
// sky reflection, sun glints, forward scatter, crest foam, and optional shore foam.

#import bevy_pbr::mesh_view_bindings::{view, globals, lights}
#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_normal_local_to_world}

struct WaterSettings {
    deep_color: vec4<f32>,
    shallow_color: vec4<f32>,
    foam_color: vec4<f32>,
    horizon_color: vec4<f32>,
    // X speed, Y frequency, Z amplitude, W secondary-wave scale.
    wave: vec4<f32>,
    // X Fresnel exponent, Y base opacity, Z edge opacity, W light tint.
    surface: vec4<f32>,
    // X specular strength, Y highlight exponent, Z forward scatter, W reflection.
    optics: vec4<f32>,
    // X crest threshold, Y softness, Z shoreline UV width, W shoreline strength.
    foam: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> water: WaterSettings;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) wave_mix: f32,
    @location(3) steepness: f32,
    @location(4) uv: vec2<f32>,
};

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(in.instance_index);
    var world_position = world_from_local * vec4<f32>(in.position, 1.0);
    let base_normal = normalize(mesh_normal_local_to_world(in.normal, in.instance_index));
    let upness = abs(base_normal.y);
    let xz = world_position.xz;
    let frequency = water.wave.y;
    let speed = water.wave.x;
    let amplitude = water.wave.z;
    let secondary = water.wave.w;

    // Four constant-direction waves are cheap, deterministic, and avoid the
    // uniform rolling-sheet look of a single axis-aligned sine wave.
    let dir_a = normalize(vec2<f32>(1.0, 0.24));
    let dir_b = normalize(vec2<f32>(-0.31, 1.0));
    let dir_c = normalize(vec2<f32>(0.72, 0.69));
    let dir_d = normalize(vec2<f32>(-0.88, 0.47));
    let phase_a = dot(xz, dir_a) * frequency + globals.time * speed;
    let phase_b = dot(xz, dir_b) * frequency * 1.37 - globals.time * speed * 0.73;
    let phase_c = dot(xz, dir_c) * frequency * 2.11 + globals.time * speed * 1.31;
    let phase_d = dot(xz, dir_d) * frequency * 3.73 - globals.time * speed * 1.83;
    let height_a = sin(phase_a);
    let height_b = sin(phase_b) * secondary;
    let height_c = sin(phase_c) * 0.24;
    let height_d = sin(phase_d) * 0.10;
    let displacement = (height_a + height_b + height_c + height_d) * amplitude * upness;
    world_position.y += displacement;

    let gradient =
        dir_a * cos(phase_a) * frequency * amplitude
        + dir_b * cos(phase_b) * frequency * 1.37 * amplitude * secondary
        + dir_c * cos(phase_c) * frequency * 2.11 * amplitude * 0.24
        + dir_d * cos(phase_d) * frequency * 3.73 * amplitude * 0.10;
    let wave_normal = normalize(vec3<f32>(-gradient.x, 1.0, -gradient.y));
    out.world_normal = normalize(mix(base_normal, wave_normal, upness));
    out.world_position = world_position;
    out.wave_mix = clamp((height_a + height_b + height_c) / (2.0 + secondary) + 0.5, 0.0, 1.0);
    out.steepness = clamp(length(gradient) * 7.0, 0.0, 1.0);
    out.uv = in.uv;
    out.position = view.clip_from_world * world_position;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let view_direction = normalize(view.world_position.xyz - in.world_position.xyz);
    let ndotv = max(dot(view_direction, normal), 0.0);
    let fresnel = pow(clamp(1.0 - ndotv, 0.0, 1.0), max(water.surface.x, 0.001));

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

    // Approximate optical depth from the grazing view angle. This is not scene
    // depth refraction, but it gives convincing deep/shallow absorption without
    // allocating a color/depth texture for every split-screen camera.
    let optical_depth = clamp(1.0 - ndotv * 0.72, 0.0, 1.0);
    let body_mix = clamp(0.18 + in.wave_mix * 0.22 + (1.0 - optical_depth) * 0.38, 0.0, 1.0);
    let body = mix(water.deep_color.rgb, water.shallow_color.rgb, body_mix);
    let diffuse = mix(0.42, 1.0, ndotl);
    let lit_body = body * diffuse * mix(vec3<f32>(1.0), light_tint, water.surface.w);

    let reflected = reflect(-view_direction, normal);
    let sky_vertical = clamp(reflected.y * 0.5 + 0.5, 0.0, 1.0);
    let sky_color = mix(water.horizon_color.rgb, vec3<f32>(0.08, 0.20, 0.42), sky_vertical);
    let reflection = sky_color * fresnel * water.optics.w;

    let half_vector = normalize(light + view_direction);
    let sun_glint = pow(max(dot(normal, half_vector), 0.0), max(water.optics.y, 1.0))
        * water.optics.x;
    let scatter = pow(max(dot(view_direction, -light), 0.0), 4.0)
        * water.optics.z * (0.35 + in.wave_mix * 0.65);

    let crest_start = water.foam.x - water.foam.y;
    let crest_end = water.foam.x + water.foam.y;
    let crest_foam = smoothstep(crest_start, crest_end, in.steepness * (0.55 + in.wave_mix * 0.65));
    let uv_edge = min(min(in.uv.x, 1.0 - in.uv.x), min(in.uv.y, 1.0 - in.uv.y));
    let shoreline = (1.0 - smoothstep(0.0, max(water.foam.z, 0.0001), uv_edge)) * water.foam.w;
    let foam_amount = clamp(max(crest_foam, shoreline), 0.0, 1.0);
    let foam = water.foam_color.rgb * foam_amount * water.foam_color.a;

    let alpha = mix(water.surface.y, water.surface.z, fresnel);
    let color = lit_body + reflection + light_tint * sun_glint + water.shallow_color.rgb * scatter;
    return vec4<f32>(clamp(mix(color, foam, foam_amount * 0.82), vec3<f32>(0.0), vec3<f32>(1.0)), alpha);
}
