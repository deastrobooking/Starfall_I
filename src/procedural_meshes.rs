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
}
