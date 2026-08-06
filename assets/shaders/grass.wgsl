// Scene-lit grass blades for Starfall's open world.
//
// Geometry arrives pre-merged: one mesh holds every blade in a 16 m chunk, so
// the CPU submits one draw per chunk instead of one per blade. Everything that
// varies per blade — sway phase, height tint, ground colour — is baked into the
// vertex stream, and everything that varies per frame happens here.
//
// Vertex stream contract (see `world_plugin::grass::bake_chunk_mesh`):
//   POSITION  chunk-local blade vertex, already curved and leaned
//   NORMAL    blade surface normal, cupped across the width
//   UV_0      x = 0..1 across the blade width, y = 0 at the root, 1 at the tip
//   COLOR     rgb = terrain ground colour under the blade, a = per-blade hash

#import bevy_pbr::mesh_view_bindings::{view, globals, lights}
#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_normal_local_to_world}

struct GrassSettings {
    // RGB tip tint, A blade alpha.
    tip_color: vec4<f32>,
    // RGB root tint (multiplied into the baked ground colour), A root occlusion.
    root_color: vec4<f32>,
    // X wave speed, Y bend strength, Z spatial frequency, W flutter scale.
    wind: vec4<f32>,
    // XY wind direction on the XZ plane, Z gust frequency, W gust depth.
    flow: vec4<f32>,
    // X translucency, Y sheen, Z per-blade colour variation, W ambient scale.
    detail: vec4<f32>,
    // Camera-distance visibility window: X/Y fade in, Z/W fade out. Adjacent
    // LOD tiers overlap these windows so one dissolves in as the other leaves.
    range: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> grass: GrassSettings;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(5) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) height_mix: f32,
    @location(3) width_mix: f32,
    @location(4) ground_color: vec3<f32>,
    @location(5) blade_hash: f32,
};

const TAU: f32 = 6.2831853;

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(in.instance_index);
    var world_position = world_from_local * vec4<f32>(in.position, 1.0);

    // The bake puts the root at v = 0, so a blade of any height stays anchored
    // without the shader needing to know how tall it is.
    let height_mix = clamp(in.uv.y, 0.0, 1.0);
    let blade_hash = in.color.a;
    // Bend grows with the square of the height: the root barely moves, the tip
    // carries the whole arc. This is what separates grass from swinging sticks.
    let lever = height_mix * height_mix;

    let flow_dir = normalize(grass.flow.xy);
    let side_dir = vec2<f32>(-flow_dir.y, flow_dir.x);
    // Waves travel along the wind, so a gust visibly crosses the field instead
    // of every blade in the chunk pulsing in lockstep.
    let travel = dot(world_position.xz, flow_dir);
    let phase = globals.time * grass.wind.x - travel * grass.wind.z + blade_hash * TAU;

    // Three octaves keep the motion from reading as a single sine.
    let ripple = sin(phase) * 0.60
        + sin(phase * 2.31 + 1.70) * 0.25
        + sin(phase * 0.53 - 0.90) * 0.15;
    // A slow, low-frequency envelope on top so the field breathes in gusts
    // rather than shivering at a constant amplitude.
    let gust_phase = globals.time * grass.wind.x * 0.23 - travel * grass.flow.z;
    let gust = 1.0 - grass.flow.w + grass.flow.w * (sin(gust_phase) * 0.5 + 0.5);
    // Shorter blades are stiffer; the hash also keeps neighbours out of sync.
    let stiffness = 0.72 + blade_hash * 0.56;
    let bend = ripple * gust * grass.wind.y * lever * stiffness;

    // Sideways flutter perpendicular to the wind, at a higher frequency: this
    // is the detail that makes a still frame look alive rather than combed.
    let flutter = sin(phase * 3.70 + blade_hash * 19.0)
        * grass.wind.y * grass.wind.w * lever;

    world_position.x += flow_dir.x * bend + side_dir.x * flutter;
    world_position.z += flow_dir.y * bend + side_dir.y * flutter;
    // Blades bend, they do not stretch. Dropping the tip by the square of the
    // horizontal travel keeps the arc roughly length-preserving.
    world_position.y -= bend * bend * 0.35;

    // Tilt the normal with the bend so highlights sweep across the field as the
    // wind passes instead of staying pinned to the baked orientation.
    let base_normal = mesh_normal_local_to_world(in.normal, in.instance_index);
    let tilt = vec3<f32>(flow_dir.x, 0.0, flow_dir.y) * bend * 0.9;
    out.world_normal = normalize(base_normal - tilt + vec3<f32>(0.0, lever * 0.15, 0.0));

    out.world_position = world_position.xyz;
    out.position = view.clip_from_world * world_position;
    out.height_mix = height_mix;
    out.width_mix = in.uv.x;
    out.ground_color = in.color.rgb;
    out.blade_hash = blade_hash;
    return out;
}

// Interleaved gradient noise: a cheap, temporally stable dither pattern. Used
// to fade distant chunks out per-pixel so blades thin away instead of popping.
fn dither_threshold(frag_xy: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(frag_xy, vec2<f32>(0.06711056, 0.00583715))));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    let to_view = view.world_position.xyz - in.world_position;
    let distance = length(to_view);
    let view_direction = to_view / max(distance, 0.0001);

    // Distance thinning. Doing it with `discard` keeps grass in the opaque pass,
    // so it still depth-sorts correctly against the world, and dithering means
    // a tier dissolves blade-by-blade instead of vanishing as a visible ring.
    let coverage = smoothstep(grass.range.x, grass.range.y, distance)
        * (1.0 - smoothstep(grass.range.z, grass.range.w, distance));
    if coverage < dither_threshold(in.position.xy) {
        discard;
    }

    // Blades are drawn double-sided; flip the normal toward whichever face the
    // rasteriser actually produced.
    var normal = normalize(in.world_normal);
    if !is_front {
        normal = -normal;
    }

    // Accumulate the scene's directional lights the same way the terrain and
    // water shaders do, so grass reacts to the day/night cycle.
    var direction_sum = vec3<f32>(0.45, 1.0, 0.30) * 0.0001;
    var color_sum = vec3<f32>(0.0001);
    for (var index = 0u; index < lights.n_directional_lights; index += 1u) {
        let scene_light = lights.directional_lights[index];
        let energy = max(max(scene_light.color.r, scene_light.color.g), scene_light.color.b);
        direction_sum += scene_light.direction_to_light * energy;
        color_sum += scene_light.color.rgb;
    }
    let light = normalize(direction_sum);
    let light_energy = max(max(color_sum.r, color_sum.g), color_sum.b);
    let light_tint = color_sum / light_energy;

    // Wrapped diffuse. A hard N·L terminator makes a field look like cut paper;
    // wrapping the falloff past the horizon gives the soft foliage rolloff.
    let ndotl = dot(normal, light);
    let wrap = 0.55;
    let diffuse = clamp((ndotl + wrap) / (1.0 + wrap), 0.0, 1.0);

    // Subsurface scattering approximation: sunlight passing *through* the blade
    // toward the camera. Tips are thinner, so they transmit more, which is what
    // produces the bright rim on back-lit grass.
    let back_scatter = pow(clamp(dot(view_direction, -light), 0.0, 1.0), 4.0);
    let thickness = mix(0.15, 1.0, in.height_mix);
    let translucency = back_scatter * thickness * grass.detail.x;

    // Wet-looking specular sheen along the blade, strongest near the tip.
    let half_vector = normalize(light + view_direction);
    let sheen = pow(clamp(dot(normal, half_vector), 0.0, 1.0), 48.0)
        * grass.detail.y * in.height_mix;

    // Two occlusion terms: the canopy shades the roots, and the curl across the
    // blade width darkens the edges so a flat quad reads as a curved ribbon.
    let root_occlusion = mix(1.0 - grass.root_color.a, 1.0, smoothstep(0.0, 0.45, in.height_mix));
    let curl = 0.82 + 0.18 * sin(clamp(in.width_mix, 0.0, 1.0) * 3.14159);

    // Roots take the terrain colour beneath them so the field grows out of the
    // ground rather than sitting on top of it; tips reach the authored green.
    let variation = 1.0 - grass.detail.z + in.blade_hash * grass.detail.z * 2.0;
    let root_rgb = in.ground_color * grass.root_color.rgb;
    let tip_rgb = grass.tip_color.rgb * variation;
    let blade_color = mix(root_rgb, tip_rgb, in.height_mix * in.height_mix);

    // Sky bounce keeps shadowed sides from going flat black at night.
    let ambient = mix(0.22, 0.34, clamp(normal.y * 0.5 + 0.5, 0.0, 1.0)) * grass.detail.w;
    let scene_tint = mix(vec3<f32>(1.0), light_tint, 0.25);

    var lit = blade_color * (diffuse + ambient) * root_occlusion * curl * scene_tint;
    lit += tip_rgb * translucency * light_tint;
    lit += vec3<f32>(sheen) * light_tint;

    return vec4<f32>(lit, grass.tip_color.a);
}
