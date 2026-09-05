// Bounded radiance/occupancy DDA oracle. This pass does not integrate irradiance.
struct Ray { origin_distance: vec4<f32>, direction: vec4<f32> }
struct Params { origin_cell_size: vec4<f32>, dims_ray_count: vec4<u32> }

@group(0) @binding(0) var<storage, read> voxels: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read> rays: array<Ray>;
@group(0) @binding(2) var<storage, read_write> results: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> params: Params;

fn trace(ray: Ray) -> vec4<f32> {
    let miss = vec4<f32>(0.03, 0.05, 0.08, -1.0);
    let origin = ray.origin_distance.xyz;
    let direction = ray.direction.xyz;
    let volume_min = params.origin_cell_size.xyz;
    let size = params.origin_cell_size.w;
    let dims = params.dims_ray_count.xyz;
    let volume_max = volume_min + vec3<f32>(dims) * size;
    var enter = 0.0;
    var exit = ray.origin_distance.w;
    for (var axis = 0u; axis < 3u; axis += 1u) {
        if abs(direction[axis]) < 1e-8 {
            if origin[axis] < volume_min[axis] || origin[axis] >= volume_max[axis] {
                return miss;
            }
        } else {
            let a = (volume_min[axis] - origin[axis]) / direction[axis];
            let b = (volume_max[axis] - origin[axis]) / direction[axis];
            enter = max(enter, min(a, b));
            exit = min(exit, max(a, b));
        }
    }
    if exit <= enter { return miss; }
    var cell = clamp(vec3<i32>(floor((origin + direction * enter - volume_min) / size)),
                     vec3<i32>(0), vec3<i32>(dims) - vec3<i32>(1));
    var step = vec3<i32>(0);
    var next = vec3<f32>(1e30);
    var delta = vec3<f32>(1e30);
    for (var axis = 0u; axis < 3u; axis += 1u) {
        if abs(direction[axis]) >= 1e-8 {
            step[axis] = select(-1, 1, direction[axis] > 0.0);
            let boundary = volume_min[axis] + f32(cell[axis] + select(0, 1, step[axis] > 0)) * size;
            next[axis] = (boundary - origin[axis]) / direction[axis];
            delta[axis] = size / abs(direction[axis]);
        }
    }
    var distance = enter;
    let max_steps = dims.x + dims.y + dims.z + 3u;
    for (var i = 0u; i < max_steps; i += 1u) {
        if any(cell < vec3<i32>(0)) || any(cell >= vec3<i32>(dims)) { return miss; }
        let index = u32(cell.x) + dims.x * (u32(cell.y) + dims.y * u32(cell.z));
        if index >= arrayLength(&voxels) { return miss; }
        let sample = voxels[index];
        if sample.w > 0.5 { return vec4<f32>(sample.xyz, distance); }
        distance = min(next.x, min(next.y, next.z));
        if distance > exit { return miss; }
        for (var axis = 0u; axis < 3u; axis += 1u) {
            if next[axis] <= distance {
                cell[axis] += step[axis];
                next[axis] += delta[axis];
            }
        }
    }
    return miss;
}

@compute @workgroup_size(64, 1, 1)
fn trace_probes(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= params.dims_ray_count.w || gid.x >= arrayLength(&rays) || gid.x >= arrayLength(&results) {
        return;
    }
    results[gid.x] = trace(rays[gid.x]);
}
