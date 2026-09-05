//! CPU oracle for the first bounded voxel/probe GPU experiment.
//!
//! Voxels contain linear outgoing radiance plus occupancy. Traversal tests the
//! transport substrate; no irradiance integration or surface GI is implied.

const MAX_CELLS: u64 = 128 * 128 * 128;
const MAX_RAYS: u64 = 2_097_152;
pub const SKY_RADIANCE: [f32; 3] = [0.03, 0.05, 0.08];

#[derive(Debug, Clone, Copy)]
pub struct ProbeConfig {
    pub voxel_dims: [u32; 3],
    pub voxel_origin: [f32; 3],
    pub cell_size: f32,
    pub probe_dims: [u32; 3],
    pub probe_origin: [f32; 3],
    pub probe_spacing: f32,
    pub rays_per_probe: u32,
    pub max_distance: f32,
    pub memory_budget_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocationPlan {
    pub voxel_count: u32,
    pub ray_count: u32,
    pub storage_bytes: u64,
    pub readback_bytes: u64,
    pub workgroups: u32,
}

impl ProbeConfig {
    pub fn allocation_plan(&self) -> Result<AllocationPlan, String> {
        if self.voxel_dims.iter().any(|dim| *dim == 0 || *dim > 128)
            || self.probe_dims.iter().any(|dim| *dim == 0 || *dim > 32)
            || !(1..=256).contains(&self.rays_per_probe)
        {
            return Err("Voxel, probe, or ray dimensions are outside supported bounds".into());
        }
        for values in [self.voxel_origin, self.probe_origin] {
            if values
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 1_000_000.0)
            {
                return Err("Volume origins must be finite and bounded".into());
            }
        }
        for value in [self.cell_size, self.probe_spacing, self.max_distance] {
            if !value.is_finite() || !(0.001..=10_000.0).contains(&value) {
                return Err(
                    "Cell size, spacing, and trace distance must be finite and positive".into(),
                );
            }
        }
        if self
            .voxel_origin
            .iter()
            .any(|origin| *origin + self.cell_size <= *origin)
            || self
                .probe_origin
                .iter()
                .any(|origin| *origin + self.probe_spacing <= *origin)
        {
            return Err("Volume spacing is below floating-point precision at this origin".into());
        }
        let product = |dims: [u32; 3]| dims.into_iter().map(u64::from).product::<u64>();
        let voxels = product(self.voxel_dims);
        let rays = product(self.probe_dims) * u64::from(self.rays_per_probe);
        if voxels > MAX_CELLS || rays > MAX_RAYS {
            return Err("Probe workload exceeds the bounded dispatch".into());
        }
        // RGBA32F voxels; two vec4s/ray; one vec4 result/ray; two vec4 uniforms.
        // This experiment owns no irradiance/history atlas yet.
        let storage_bytes = voxels * 16 + rays * (32 + 16) + 32;
        let readback_bytes = rays * 16;
        if storage_bytes + readback_bytes > self.memory_budget_bytes {
            return Err("Probe storage plus readback exceeds the memory budget".into());
        }
        Ok(AllocationPlan {
            voxel_count: voxels as u32,
            ray_count: rays as u32,
            storage_bytes,
            readback_bytes,
            workgroups: (rays as u32).div_ceil(64),
        })
    }

    pub fn rays(&self) -> Result<Vec<ProbeRay>, String> {
        let plan = self.allocation_plan()?;
        let mut rays = Vec::with_capacity(plan.ray_count as usize);
        for z in 0..self.probe_dims[2] {
            for y in 0..self.probe_dims[1] {
                for x in 0..self.probe_dims[0] {
                    let origin = std::array::from_fn(|axis| {
                        self.probe_origin[axis] + [x, y, z][axis] as f32 * self.probe_spacing
                    });
                    for index in 0..self.rays_per_probe {
                        // Midpoint sampling is defined even when there is one ray.
                        let y = 1.0 - 2.0 * (index as f32 + 0.5) / self.rays_per_probe as f32;
                        let radius = (1.0 - y * y).max(0.0).sqrt();
                        let angle = index as f32 * 2.399_963_1;
                        rays.push(ProbeRay::new(
                            origin,
                            [angle.cos() * radius, y, angle.sin() * radius],
                            self.max_distance,
                        )?);
                    }
                }
            }
        }
        Ok(rays)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProbeRay {
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
}

impl ProbeRay {
    pub fn new(origin: [f32; 3], direction: [f32; 3], max_distance: f32) -> Result<Self, String> {
        if origin
            .iter()
            .chain(direction.iter())
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
            || !max_distance.is_finite()
            || !(0.001..=10_000.0).contains(&max_distance)
        {
            return Err("Ray values must be finite and bounded".into());
        }
        let length = direction.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length < 1e-8 {
            return Err("Ray direction cannot be zero".into());
        }
        Ok(Self {
            origin,
            direction: direction.map(|v| v / length),
            max_distance,
        })
    }

    pub fn origin_and_distance(self) -> [f32; 4] {
        [
            self.origin[0],
            self.origin[1],
            self.origin[2],
            self.max_distance,
        ]
    }

    pub fn direction(self) -> [f32; 4] {
        [self.direction[0], self.direction[1], self.direction[2], 0.0]
    }
}

#[derive(Debug, Clone)]
pub struct VoxelScene {
    config: ProbeConfig,
    voxels: Vec<[f32; 4]>,
}

impl VoxelScene {
    pub fn new(config: ProbeConfig, voxels: Vec<[f32; 4]>) -> Result<Self, String> {
        let plan = config.allocation_plan()?;
        if voxels.len() != plan.voxel_count as usize
            || voxels.iter().any(|cell| {
                cell.iter().any(|v| !v.is_finite() || *v < 0.0) || !matches!(cell[3], 0.0 | 1.0)
            })
        {
            return Err(
                "Voxel buffer must match the volume and contain finite radiance/occupancy".into(),
            );
        }
        Ok(Self { config, voxels })
    }

    pub fn config(&self) -> ProbeConfig {
        self.config
    }
    pub fn voxels(&self) -> &[[f32; 4]] {
        &self.voxels
    }

    /// Exact cell traversal. RGB is outgoing radiance, W is hit distance or -1
    /// for the sky. Zero direction axes and entry from outside are explicit.
    pub fn trace(&self, ray: ProbeRay) -> [f32; 4] {
        let miss = [SKY_RADIANCE[0], SKY_RADIANCE[1], SKY_RADIANCE[2], -1.0];
        let mut enter = 0.0_f32;
        let mut exit = ray.max_distance;
        for axis in 0..3 {
            let min = self.config.voxel_origin[axis];
            let max = min + self.config.voxel_dims[axis] as f32 * self.config.cell_size;
            let dir = ray.direction[axis];
            if dir.abs() < 1e-8 {
                if ray.origin[axis] < min || ray.origin[axis] >= max {
                    return miss;
                }
            } else {
                let a = (min - ray.origin[axis]) / dir;
                let b = (max - ray.origin[axis]) / dir;
                enter = enter.max(a.min(b));
                exit = exit.min(a.max(b));
            }
        }
        if exit <= enter {
            return miss;
        }
        let mut cell = [0_i32; 3];
        let mut step = [0_i32; 3];
        let mut next = [f32::INFINITY; 3];
        let mut delta = [f32::INFINITY; 3];
        for axis in 0..3 {
            let p = ray.origin[axis] + ray.direction[axis] * enter;
            cell[axis] = (((p - self.config.voxel_origin[axis]) / self.config.cell_size).floor()
                as i32)
                .clamp(0, self.config.voxel_dims[axis] as i32 - 1);
            if ray.direction[axis].abs() >= 1e-8 {
                step[axis] = if ray.direction[axis] > 0.0 { 1 } else { -1 };
                let boundary = self.config.voxel_origin[axis]
                    + (cell[axis] + i32::from(step[axis] > 0)) as f32 * self.config.cell_size;
                next[axis] = (boundary - ray.origin[axis]) / ray.direction[axis];
                delta[axis] = self.config.cell_size / ray.direction[axis].abs();
            }
        }
        let mut distance = enter;
        let max_steps = self.config.voxel_dims.iter().sum::<u32>() + 3;
        for _ in 0..max_steps {
            if (0..3)
                .any(|axis| cell[axis] < 0 || cell[axis] >= self.config.voxel_dims[axis] as i32)
            {
                return miss;
            }
            let index = cell[0] as u32
                + self.config.voxel_dims[0]
                    * (cell[1] as u32 + self.config.voxel_dims[1] * cell[2] as u32);
            let sample = self.voxels[index as usize];
            if sample[3] > 0.5 {
                return [sample[0], sample[1], sample[2], distance];
            }
            distance = next.into_iter().fold(f32::INFINITY, f32::min);
            if distance > exit {
                return miss;
            }
            for axis in 0..3 {
                if next[axis] <= distance {
                    cell[axis] += step[axis];
                    next[axis] += delta[axis];
                }
            }
        }
        miss
    }
}

/// Small deterministic volume shared by CPU and GPU verification. The ray
/// count deliberately leaves a partially occupied final compute workgroup.
pub fn validation_fixture() -> Result<(VoxelScene, Vec<ProbeRay>), String> {
    let config = ProbeConfig {
        voxel_dims: [16; 3],
        voxel_origin: [0.0; 3],
        cell_size: 0.5,
        probe_dims: [2; 3],
        probe_origin: [2.0; 3],
        probe_spacing: 4.0,
        rays_per_probe: 65,
        max_distance: 20.0,
        memory_budget_bytes: 1_048_576,
    };
    let plan = config.allocation_plan()?;
    let mut voxels = vec![[0.0; 4]; plan.voxel_count as usize];
    for z in 0..16 {
        for y in 0..16 {
            voxels[8 + 16 * (y + 16 * z)] = [0.8, 0.2, 0.1, 1.0];
        }
    }
    let rays = config.rays()?;
    Ok((VoxelScene::new(config, voxels)?, rays))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocations_include_readback_and_partial_dispatch() {
        let (scene, rays) = validation_fixture().unwrap();
        let mut config = scene.config();
        let plan = config.allocation_plan().unwrap();
        assert_eq!(plan.ray_count, 520);
        assert_eq!(plan.workgroups, 9);
        assert_eq!(rays.len(), 520);
        config.memory_budget_bytes = plan.storage_bytes + plan.readback_bytes - 1;
        assert!(config.allocation_plan().is_err());
        config.voxel_dims = [u32::MAX; 3];
        assert!(config.allocation_plan().is_err());
    }

    #[test]
    fn single_ray_is_finite_and_zero_dimensions_are_rejected() {
        let (scene, _) = validation_fixture().unwrap();
        let mut config = scene.config();
        config.rays_per_probe = 1;
        assert_eq!(config.rays().unwrap().len(), 8);
        config.rays_per_probe = 0;
        assert!(config.rays().is_err());
        config.rays_per_probe = 1;
        config.cell_size = f32::NAN;
        assert!(config.allocation_plan().is_err());
    }

    #[test]
    fn axis_aligned_and_outside_rays_hit_nearest_cell_at_world_distance() {
        let (scene, _) = validation_fixture().unwrap();
        for (origin, direction, expected) in [
            ([2.0, 2.0, 2.0], [1.0, 0.0, 0.0], 2.0),
            ([6.0, 2.0, 2.0], [-1.0, 0.0, 0.0], 1.5),
            ([-2.0, 2.0, 2.0], [1.0, 0.0, 0.0], 6.0),
            ([8.0, 2.0, 2.0], [-1.0, 0.0, 0.0], 3.5),
            ([4.2, 2.0, 2.0], [1.0, 0.0, 0.0], 0.0),
        ] {
            let result = scene.trace(ProbeRay::new(origin, direction, 20.0).unwrap());
            assert_eq!(&result[..3], &[0.8, 0.2, 0.1]);
            assert!((result[3] - expected).abs() < 1e-5);
        }
    }

    #[test]
    fn parallel_outside_and_short_rays_do_not_sample_clamped_edges() {
        let (scene, _) = validation_fixture().unwrap();
        for (origin, direction, distance) in [
            ([2.0, 2.0, 2.0], [-1.0, 0.0, 0.0], 20.0),
            ([2.0, 8.0, 2.0], [1.0, 0.0, 0.0], 20.0),
            ([8.0, 2.0, 2.0], [1.0, 0.0, 0.0], 20.0),
            ([2.0, 2.0, 2.0], [1.0, 0.0, 0.0], 1.0),
        ] {
            assert_eq!(
                scene.trace(ProbeRay::new(origin, direction, distance).unwrap())[3],
                -1.0
            );
        }
        assert!(ProbeRay::new([0.0; 3], [0.0; 3], 1.0).is_err());
        assert!(ProbeRay::new([f32::INFINITY; 3], [1.0; 3], 1.0).is_err());
    }

    #[test]
    fn diagonal_and_empty_scene_traversal_terminate() {
        let (scene, rays) = validation_fixture().unwrap();
        for ray in &rays {
            assert!(scene.trace(*ray).iter().all(|v| v.is_finite()));
        }
        let empty = VoxelScene::new(scene.config(), vec![[0.0; 4]; scene.voxels().len()]).unwrap();
        for ray in rays {
            assert_eq!(empty.trace(ray)[3], -1.0);
        }
        assert!(VoxelScene::new(scene.config(), vec![]).is_err());
        let mut cells = scene.voxels().to_vec();
        cells[0][0] = f32::NAN;
        assert!(VoxelScene::new(scene.config(), cells).is_err());
    }
}
