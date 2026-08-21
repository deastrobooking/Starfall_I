use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

fn signed_pow(value: f32, exponent: f32) -> f32 {
    value.signum() * value.abs().powf(exponent)
}

/// A cubic Bézier path used by semantic character parts such as hair, horns,
/// tails, cables, and future limbs. Keeping the curve in generator space makes
/// it cheap to regenerate a single part when an authored control changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier3 {
    pub start: Vec3,
    pub control_start: Vec3,
    pub control_end: Vec3,
    pub end: Vec3,
}

impl CubicBezier3 {
    pub fn sample(self, t: f32) -> Vec3 {
        let t = t.clamp(0.0, 1.0);
        let one_minus_t = 1.0 - t;
        self.start * one_minus_t.powi(3)
            + self.control_start * (3.0 * one_minus_t.powi(2) * t)
            + self.control_end * (3.0 * one_minus_t * t * t)
            + self.end * t.powi(3)
    }

    pub fn tangent(self, t: f32) -> Vec3 {
        let t = t.clamp(0.0, 1.0);
        let one_minus_t = 1.0 - t;
        ((self.control_start - self.start) * (3.0 * one_minus_t.powi(2))
            + (self.control_end - self.control_start) * (6.0 * one_minus_t * t)
            + (self.end - self.control_end) * (3.0 * t * t))
            .try_normalize()
            .unwrap_or_else(|| (self.end - self.start).try_normalize().unwrap_or(Vec3::Y))
    }
}

/// Cubic Bézier interpolation for the two radii of an elliptical or
/// superelliptical sweep profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezierRadii {
    pub start: Vec2,
    pub control_start: Vec2,
    pub control_end: Vec2,
    pub end: Vec2,
}

impl CubicBezierRadii {
    pub fn tapered(start: Vec2, end: Vec2) -> Self {
        Self {
            start,
            control_start: start.lerp(end, 0.28),
            control_end: start.lerp(end, 0.72),
            end,
        }
    }

    pub fn sample(self, t: f32) -> Vec2 {
        let t = t.clamp(0.0, 1.0);
        let one_minus_t = 1.0 - t;
        (self.start * one_minus_t.powi(3)
            + self.control_start * (3.0 * one_minus_t.powi(2) * t)
            + self.control_end * (3.0 * one_minus_t * t * t)
            + self.end * t.powi(3))
        .max(Vec2::splat(0.001))
    }
}

/// Tessellation and silhouette controls for [`bezier_sweep_mesh`]. Vertex
/// counts remain stable while the path and radius control points change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepMeshSettings {
    pub rings: usize,
    pub sectors: usize,
    /// `2.0` is elliptical; lower values produce boxier sci-fi profiles.
    pub profile_exponent: f32,
    pub cap_ends: bool,
}

impl Default for SweepMeshSettings {
    fn default() -> Self {
        Self {
            rings: 10,
            sectors: 12,
            profile_exponent: 2.0,
            cap_ends: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SweepFrame {
    tangent: Vec3,
    normal: Vec3,
    binormal: Vec3,
}

fn initial_sweep_frame(tangent: Vec3) -> SweepFrame {
    let tangent = tangent.try_normalize().unwrap_or(Vec3::Y);
    let reference = if tangent.dot(Vec3::Y).abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let normal = tangent.cross(reference).normalize_or_zero();
    let binormal = tangent.cross(normal).normalize_or_zero();
    SweepFrame {
        tangent,
        normal,
        binormal,
    }
}

fn transport_sweep_frame(previous: SweepFrame, tangent: Vec3) -> SweepFrame {
    let tangent = tangent.try_normalize().unwrap_or(previous.tangent);
    let cross = previous.tangent.cross(tangent);
    let dot = previous.tangent.dot(tangent).clamp(-1.0, 1.0);
    let mut normal = if cross.length_squared() > 1.0e-10 {
        let rotation = Quat::from_axis_angle(cross.normalize(), cross.length().atan2(dot));
        rotation * previous.normal
    } else if dot < 0.0 {
        Quat::from_axis_angle(previous.binormal, PI) * previous.normal
    } else {
        previous.normal
    };
    normal = (normal - tangent * normal.dot(tangent))
        .try_normalize()
        .unwrap_or(previous.normal);
    let binormal = tangent
        .cross(normal)
        .try_normalize()
        .unwrap_or(previous.binormal);
    SweepFrame {
        tangent,
        normal,
        binormal,
    }
}

/// Sweeps a tapered superellipse along a cubic Bézier path. Cross-sections use
/// parallel-transport frames, avoiding the sudden twisting that Frenet frames
/// produce around shallow or inflecting cartoon curves.
pub fn bezier_sweep_mesh(
    path: CubicBezier3,
    radii: CubicBezierRadii,
    settings: SweepMeshSettings,
) -> Mesh {
    let rings = settings.rings.max(2);
    let sectors = settings.sectors.max(6);
    let profile_power = 2.0 / settings.profile_exponent.max(0.2);
    let cap_vertices = usize::from(settings.cap_ends) * 2;
    let mut positions = Vec::with_capacity((rings + 1) * (sectors + 1) + cap_vertices);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    let mut indices =
        Vec::with_capacity(rings * sectors * 6 + usize::from(settings.cap_ends) * sectors * 6);

    let mut frame = initial_sweep_frame(path.tangent(0.0));
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        if ring > 0 {
            frame = transport_sweep_frame(frame, path.tangent(v));
        }
        let center = path.sample(v);
        let radius = radii.sample(v);
        for sector in 0..=sectors {
            let u = sector as f32 / sectors as f32;
            let angle = u * TAU;
            let (sin_angle, cos_angle) = angle.sin_cos();
            let x = radius.x * signed_pow(cos_angle, profile_power);
            let y = radius.y * signed_pow(sin_angle, profile_power);
            let position = center + frame.normal * x + frame.binormal * y;
            let normal = (frame.normal * (x / radius.x.powi(2))
                + frame.binormal * (y / radius.y.powi(2)))
            .try_normalize()
            .unwrap_or(frame.normal);
            positions.push(position.to_array());
            normals.push(normal.to_array());
            uvs.push([u, v]);
        }
    }

    let stride = (sectors + 1) as u32;
    for ring in 0..rings as u32 {
        for sector in 0..sectors as u32 {
            let a = ring * stride + sector;
            let b = a + stride;
            indices.extend_from_slice(&[a, b, a + 1, b, b + 1, a + 1]);
        }
    }

    if settings.cap_ends {
        for (ring, center, normal, flip) in [
            (
                0_u32,
                path.start,
                -initial_sweep_frame(path.tangent(0.0)).tangent,
                true,
            ),
            (rings as u32, path.end, frame.tangent, false),
        ] {
            let center_index = positions.len() as u32;
            positions.push(center.to_array());
            normals.push(normal.to_array());
            uvs.push([0.5, if flip { 0.0 } else { 1.0 }]);
            let ring_start = ring * stride;
            for sector in 0..sectors as u32 {
                let a = ring_start + sector;
                let b = a + 1;
                if flip {
                    indices.extend_from_slice(&[center_index, b, a]);
                } else {
                    indices.extend_from_slice(&[center_index, a, b]);
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Generates a smooth puffy ellipsoid. Exponents below 1.0 create the rounded,
/// slightly boxy silhouette used by cartoon gloves and shoes.
pub fn superellipsoid_mesh(
    radii: Vec3,
    vertical_exponent: f32,
    horizontal_exponent: f32,
    stacks: usize,
    sectors: usize,
) -> Mesh {
    let stacks = stacks.max(4);
    let sectors = sectors.max(8);
    let radii = radii.max(Vec3::splat(0.001));
    let vertical_exponent = vertical_exponent.max(0.05);
    let horizontal_exponent = horizontal_exponent.max(0.05);

    let mut positions = Vec::with_capacity((stacks + 1) * (sectors + 1));
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());

    for stack in 0..=stacks {
        let v = stack as f32 / stacks as f32;
        let eta = -PI * 0.5 + PI * v;
        let cos_eta = eta.cos();
        let sin_eta = eta.sin();
        let ring = signed_pow(cos_eta, vertical_exponent);
        let y = radii.y * signed_pow(sin_eta, vertical_exponent);

        for sector in 0..=sectors {
            let u = sector as f32 / sectors as f32;
            let omega = -PI + TAU * u;
            let x = radii.x * ring * signed_pow(omega.cos(), horizontal_exponent);
            let z = radii.z * ring * signed_pow(omega.sin(), horizontal_exponent);
            let position = Vec3::new(x, y, z);
            let normal = Vec3::new(x / radii.x, y / radii.y, z / radii.z)
                .try_normalize()
                .unwrap_or(Vec3::Y);

            positions.push([position.x, position.y, position.z]);
            normals.push([normal.x, normal.y, normal.z]);
            uvs.push([u, v]);
        }
    }

    let row = sectors + 1;
    let mut indices = Vec::with_capacity(stacks * sectors * 6);
    for stack in 0..stacks {
        for sector in 0..sectors {
            let a = (stack * row + sector) as u32;
            let b = ((stack + 1) * row + sector) as u32;
            let c = a + 1;
            let d = b + 1;
            indices.extend_from_slice(&[a, b, c, c, b, d]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Builds a shallow, forward-facing anime eye lens.
///
/// `pointiness` controls the signed-sine exponent of the upper/lower outline:
/// values near zero stay round while values near one pull the corners into the
/// graphic almond silhouette used by cel-styled characters. `corner_lift`
/// raises the outer corner without rotating the iris or catchlights with it.
pub fn anime_eye_mesh(
    half_size: Vec2,
    depth: f32,
    pointiness: f32,
    corner_lift: f32,
    side: f32,
) -> Mesh {
    const RINGS: usize = 4;
    const SECTORS: usize = 32;

    let half_size = half_size.max(Vec2::splat(0.001));
    let depth = depth.max(0.001);
    let outline_exponent = 0.82 + pointiness.clamp(0.0, 1.0) * 0.90;
    let corner_lift = corner_lift.clamp(-0.35, 0.35);
    let side = side.signum();

    let vertex_count = 1 + RINGS * (SECTORS + 1);
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    positions.push([0.0, 0.0, -depth]);
    normals.push([0.0, 0.0, -1.0]);
    uvs.push([0.5, 0.5]);

    for ring in 1..=RINGS {
        let radius = ring as f32 / RINGS as f32;
        for sector in 0..=SECTORS {
            let theta = TAU * sector as f32 / SECTORS as f32;
            let unit_x = theta.cos();
            let outline_y = signed_pow(theta.sin(), outline_exponent);
            let x = half_size.x * radius * unit_x;
            let lift = half_size.y * corner_lift * side * unit_x * radius;
            let y = half_size.y * radius * outline_y + lift;
            let z = -depth * (1.0 - radius * radius);
            let normal = Vec3::new(
                x / half_size.x.powi(2),
                (y - lift) / half_size.y.powi(2),
                -1.0 / depth,
            )
            .normalize_or_zero();

            positions.push([x, y, z]);
            normals.push([normal.x, normal.y, normal.z]);
            uvs.push([0.5 + x / (half_size.x * 2.0), 0.5 + y / (half_size.y * 2.0)]);
        }
    }

    let mut indices = Vec::with_capacity(SECTORS * 6 * RINGS);
    let first_ring = 1usize;
    for sector in 0..SECTORS {
        indices.extend_from_slice(&[
            0,
            (first_ring + sector + 1) as u32,
            (first_ring + sector) as u32,
        ]);
    }
    for ring in 1..RINGS {
        let inner = 1 + (ring - 1) * (SECTORS + 1);
        let outer = inner + SECTORS + 1;
        for sector in 0..SECTORS {
            let a = inner + sector;
            let b = inner + sector + 1;
            let c = outer + sector;
            let d = outer + sector + 1;
            indices
                .extend_from_slice(&[a as u32, d as u32, c as u32, a as u32, b as u32, d as u32]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn superellipsoid_counts_grid_vertices_and_triangles() {
        let mesh = superellipsoid_mesh(Vec3::ONE, 0.6, 0.7, 6, 10);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("positions should be present");
        assert_eq!(positions.len(), 77);
        assert_eq!(
            mesh.indices().expect("indices should be present").len(),
            360
        );
    }

    #[test]
    fn anime_eye_mesh_is_bounded_pointed_and_faces_forward() {
        let mesh = anime_eye_mesh(Vec2::new(2.0, 1.0), 0.2, 1.0, 0.2, 1.0);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("anime eye positions should be float3");
        assert_eq!(positions.len(), 133);
        assert!(positions
            .iter()
            .all(|position| position[0].abs() <= 2.001 && position[2] <= 0.001));
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|attribute| attribute.as_float3())
            .expect("anime eye normals should be float3");
        assert!(normals[0][2] < -0.99);
        assert_eq!(mesh.indices().expect("eye indices").len(), 672);
    }

    #[test]
    fn bezier_sweep_has_stable_topology_and_finite_attributes() {
        let path = CubicBezier3 {
            start: Vec3::ZERO,
            control_start: Vec3::new(0.2, 0.8, 0.1),
            control_end: Vec3::new(-0.3, 1.4, 0.6),
            end: Vec3::new(0.1, 2.0, 1.0),
        };
        let mesh = bezier_sweep_mesh(
            path,
            CubicBezierRadii::tapered(Vec2::splat(0.3), Vec2::splat(0.05)),
            SweepMeshSettings {
                rings: 8,
                sectors: 10,
                ..default()
            },
        );
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("sweep positions should be float3");
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|attribute| attribute.as_float3())
            .expect("sweep normals should be float3");
        assert_eq!(positions.len(), 101);
        assert_eq!(normals.len(), positions.len());
        assert_eq!(mesh.indices().expect("sweep indices").len(), 540);
        assert!(positions.iter().flatten().all(|value| value.is_finite()));
        assert!(normals.iter().flatten().all(|value| value.is_finite()));
    }

    #[test]
    fn parallel_transport_keeps_neighboring_frames_aligned() {
        let path = CubicBezier3 {
            start: Vec3::ZERO,
            control_start: Vec3::new(0.0, 0.7, 0.4),
            control_end: Vec3::new(0.0, 1.3, -0.4),
            end: Vec3::new(0.0, 2.0, 0.0),
        };
        let mut frame = initial_sweep_frame(path.tangent(0.0));
        for ring in 1..=24 {
            let next = transport_sweep_frame(frame, path.tangent(ring as f32 / 24.0));
            assert!(frame.normal.dot(next.normal) > 0.0);
            assert!(next.tangent.dot(next.normal).abs() < 1.0e-4);
            frame = next;
        }
    }
}
