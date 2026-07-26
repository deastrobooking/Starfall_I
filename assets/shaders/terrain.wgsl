// Scene-lit tri-planar terrain. World-space projection avoids stretched UVs
// on cliffs while height, slope, and the authored vertex path mask select the
// biome response without changing the terrain mesh or physics surface.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{view, lights}

struct TerrainSettings {
    grass_tint: vec4<f32>,
    rock_tint: vec4<f32>,
    snow_tint: vec4<f32>,
    ice_tint: vec4<f32>,
    path_tint: vec4<f32>,
    // X texture scale, Y tri-planar sharpness, Z normal detail, W path strength.
    detail: vec4<f32>,
    // X/Y rock slope blend, Z/W snow altitude blend.
    biome: vec4<f32>,
    // X ice start altitude, Y ice slope start, Z wetness height, W specular.
    climate: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> terrain: TerrainSettings;
@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var grass_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2)
var grass_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3)
var rock_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4)
var rock_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5)
var snow_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6)
var snow_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7)
var path_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8)
var path_sampler: sampler;

fn triplanar_weights(normal: vec3<f32>) -> vec3<f32> {
    let weights = pow(max(abs(normal), vec3<f32>(0.001)), vec3<f32>(terrain.detail.y));
    return weights / max(weights.x + weights.y + weights.z, 0.001);
}

fn triplanar_sample(
    image: texture_2d<f32>,
    image_sampler: sampler,
    position: vec3<f32>,
    normal: vec3<f32>,
) -> vec3<f32> {
    let scale = terrain.detail.x;
    let weights = triplanar_weights(normal);
    let projected_x = textureSample(image, image_sampler, position.zy * scale).rgb;
    let projected_y = textureSample(image, image_sampler, position.xz * scale).rgb;
    let projected_z = textureSample(image, image_sampler, position.xy * scale).rgb;
    return projected_x * weights.x + projected_y * weights.y + projected_z * weights.z;
}

fn detail_normal(position: vec3<f32>, base_normal: vec3<f32>) -> vec3<f32> {
    let phase = position * terrain.detail.x * 8.0;
    let ripple = vec3<f32>(
        sin(phase.z + cos(phase.y * 0.71)),
        0.0,
        cos(phase.x - sin(phase.y * 0.63)),
    );
    return normalize(base_normal + ripple * terrain.detail.z);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let geometric_normal = normalize(in.world_normal);
    let normal = detail_normal(in.world_position.xyz, geometric_normal);
    let world_y = in.world_position.y;
    let slope = clamp(1.0 - geometric_normal.y, 0.0, 1.0);

    let grass_sample = triplanar_sample(
        grass_texture,
        grass_sampler,
        in.world_position.xyz,
        geometric_normal,
    );
    let rock_sample = triplanar_sample(
        rock_texture,
        rock_sampler,
        in.world_position.xyz,
        geometric_normal,
    );
    let snow_sample = triplanar_sample(
        snow_texture,
        snow_sampler,
        in.world_position.xyz,
        geometric_normal,
    );
    let path_sample = triplanar_sample(
        path_texture,
        path_sampler,
        in.world_position.xyz,
        geometric_normal,
    );

    let rock_weight = max(
        smoothstep(terrain.biome.x, terrain.biome.y, slope),
        smoothstep(360.0, 680.0, world_y) * 0.64,
    );
    let snow_weight = smoothstep(terrain.biome.z, terrain.biome.w, world_y)
        * (1.0 - smoothstep(0.44, 0.74, slope) * 0.45);
    let ice_weight = smoothstep(terrain.climate.x, terrain.biome.w + 90.0, world_y)
        * smoothstep(terrain.climate.y, 0.58, slope)
        * 0.48;
    let path_weight = clamp(in.color.a * terrain.detail.w, 0.0, 1.0);

    let grass = grass_sample * terrain.grass_tint.rgb;
    let rock = rock_sample * terrain.rock_tint.rgb;
    let snow = snow_sample * terrain.snow_tint.rgb;
    let ice = snow_sample * terrain.ice_tint.rgb;
    let path = path_sample * terrain.path_tint.rgb;
    var albedo = mix(grass, rock, rock_weight);
    albedo = mix(albedo, snow, snow_weight);
    albedo = mix(albedo, ice, ice_weight);
    albedo = mix(albedo, path, path_weight);

    // Preserve the large-scale authored regional variation stored in vertex
    // RGB while the texture layers supply close-range material detail.
    let authored_tint = clamp(in.color.rgb * 2.35, vec3<f32>(0.72), vec3<f32>(1.18));
    albedo *= mix(vec3<f32>(1.0), authored_tint, 0.20);

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
    let diffuse = mix(0.32, 1.0, ndotl);

    let roughness = clamp(
        mix(0.86, 0.58, rock_weight)
            - ice_weight * 0.32
            + path_weight * 0.12,
        0.18,
        0.96,
    );
    let view_direction = normalize(view.world_position.xyz - in.world_position.xyz);
    let half_vector = normalize(light + view_direction);
    let specular_power = mix(96.0, 10.0, roughness);
    let specular = pow(max(dot(normal, half_vector), 0.0), specular_power)
        * terrain.climate.w
        * (1.0 - roughness);
    let wetness = 1.0 - smoothstep(terrain.climate.z, terrain.climate.z + 110.0, world_y);
    let wet_darkening = mix(1.0, 0.86, wetness * (1.0 - path_weight * 0.5));
    let scene_tint = mix(vec3<f32>(1.0), light_tint, 0.22);
    let color = albedo * diffuse * scene_tint * wet_darkening
        + light_tint * specular * (1.0 + ice_weight);
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
