use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

fn signed_pow(value: f32, exponent: f32) -> f32 {
    value.signum() * value.abs().powf(exponent)
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
}
