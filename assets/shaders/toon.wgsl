#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::pbr_types
#import bevy_pbr::mesh_bindings
#import bevy_pbr::mesh_functions

@group(1) @binding(0)
var<uniform> color: vec4<f32>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Simple cel-shading placeholder
    let N = normalize(in.world_normal);
    let L = normalize(vec3<f32>(0.5, 1.0, 0.5)); // Hardcoded light dir for now
    let NdL = max(dot(N, L), 0.0);
    
    // Step function for sharp toon shadow
    var light_intensity = 0.3;
    if NdL > 0.5 {
        light_intensity = 1.0;
    } else if NdL > 0.1 {
        light_intensity = 0.6;
    }
    
    // Rim lighting
    let V = normalize(view.world_position.xyz - in.world_position.xyz);
    let rim = 1.0 - max(dot(V, N), 0.0);
    var rim_intensity = 0.0;
    if rim > 0.7 {
        rim_intensity = 0.5;
    }
    
    let base_color = color.rgb * light_intensity;
    let final_color = base_color + vec3<f32>(rim_intensity);
    
    return vec4<f32>(final_color, color.a);
}
