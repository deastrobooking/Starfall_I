//! Non-destructive UV projection, transforms, and face-color painting.

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use super::character_records::{ImportedUvEdit, ImportedUvProjection};

pub fn apply_uv_edit(source: &Mesh, edit: &ImportedUvEdit) -> Option<Mesh> {
    let mut mesh = static_mesh_copy(source);
    let needs_split =
        edit.projection == ImportedUvProjection::BoxSeams || !edit.face_paints.is_empty();
    if needs_split {
        mesh.try_duplicate_vertices().ok()?;
    }
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(values) => values.clone(),
        _ => return None,
    };
    if positions.is_empty() {
        return None;
    }

    let mut uvs = match edit.projection {
        ImportedUvProjection::Authored => match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(values)) => values.clone(),
            _ => planar_uvs(&positions, 0, 2),
        },
        ImportedUvProjection::PlanarXy => planar_uvs(&positions, 0, 1),
        ImportedUvProjection::PlanarXz => planar_uvs(&positions, 0, 2),
        ImportedUvProjection::PlanarYz => planar_uvs(&positions, 1, 2),
        ImportedUvProjection::CylindricalY => cylindrical_y_uvs(&positions),
        ImportedUvProjection::BoxSeams => box_uvs(&positions),
    };
    let (sin, cos) = edit.rotation_radians.sin_cos();
    for uv in &mut uvs {
        let centered = Vec2::new(uv[0] - 0.5, uv[1] - 0.5);
        let rotated = Vec2::new(
            centered.x * cos - centered.y * sin,
            centered.x * sin + centered.y * cos,
        );
        uv[0] = rotated.x * edit.scale[0] + 0.5 + edit.offset[0];
        uv[1] = rotated.y * edit.scale[1] + 0.5 + edit.offset[1];
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);

    if !edit.face_paints.is_empty() {
        let mut colors = vec![[1.0_f32; 4]; positions.len()];
        for stroke in &edit.face_paints {
            for face in &stroke.face_indices {
                let start = *face as usize * 3;
                if let Some(vertices) = colors.get_mut(start..start.saturating_add(3)) {
                    vertices.fill(stroke.color);
                }
            }
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }

    mesh.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
    let _ = mesh.generate_tangents();
    Some(mesh)
}

pub fn static_mesh_copy(source: &Mesh) -> Mesh {
    let mut result = Mesh::new(source.primitive_topology(), source.asset_usage);
    result.enable_raytracing = false;
    for (attribute, values) in source.attributes() {
        if attribute.id == Mesh::ATTRIBUTE_JOINT_INDEX.id
            || attribute.id == Mesh::ATTRIBUTE_JOINT_WEIGHT.id
        {
            continue;
        }
        result.insert_attribute(*attribute, values.clone());
    }
    if let Some(indices) = source.indices() {
        result.insert_indices(indices.clone());
    }
    result
}

fn bounds(positions: &[[f32; 3]], axis: usize) -> (f32, f32) {
    positions
        .iter()
        .map(|position| position[axis])
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
            (min.min(value), max.max(value))
        })
}

fn normalized(value: f32, min: f32, max: f32) -> f32 {
    (value - min) / (max - min).abs().max(1.0e-6)
}

fn planar_uvs(positions: &[[f32; 3]], axis_u: usize, axis_v: usize) -> Vec<[f32; 2]> {
    let (min_u, max_u) = bounds(positions, axis_u);
    let (min_v, max_v) = bounds(positions, axis_v);
    positions
        .iter()
        .map(|position| {
            [
                normalized(position[axis_u], min_u, max_u),
                1.0 - normalized(position[axis_v], min_v, max_v),
            ]
        })
        .collect()
}

fn cylindrical_y_uvs(positions: &[[f32; 3]]) -> Vec<[f32; 2]> {
    let (min_y, max_y) = bounds(positions, 1);
    positions
        .iter()
        .map(|position| {
            [
                position[2].atan2(position[0]) / std::f32::consts::TAU + 0.5,
                1.0 - normalized(position[1], min_y, max_y),
            ]
        })
        .collect()
}

fn box_uvs(positions: &[[f32; 3]]) -> Vec<[f32; 2]> {
    let axis_bounds = [
        bounds(positions, 0),
        bounds(positions, 1),
        bounds(positions, 2),
    ];
    let mut uvs = vec![[0.0; 2]; positions.len()];
    for (face_index, face) in positions.chunks_exact(3).enumerate() {
        let [a, b, c] = face else { unreachable!() };
        let normal = (Vec3::from(*b) - Vec3::from(*a))
            .cross(Vec3::from(*c) - Vec3::from(*a))
            .abs();
        let (axis_u, axis_v) = if normal.x >= normal.y && normal.x >= normal.z {
            (2, 1)
        } else if normal.y >= normal.z {
            (0, 2)
        } else {
            (0, 1)
        };
        for (corner, position) in face.iter().enumerate() {
            let (min_u, max_u) = axis_bounds[axis_u];
            let (min_v, max_v) = axis_bounds[axis_v];
            uvs[face_index * 3 + corner] = [
                normalized(position[axis_u], min_u, max_u),
                1.0 - normalized(position[axis_v], min_v, max_v),
            ];
        }
    }
    uvs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_tools::character_records::ImportedFacePaint;

    fn edit(projection: ImportedUvProjection) -> ImportedUvEdit {
        ImportedUvEdit {
            source_asset: "imported_characters/test.glb".into(),
            mesh_index: 0,
            primitive_index: 0,
            projection,
            scale: [1.0; 2],
            offset: [0.0; 2],
            rotation_radians: 0.0,
            face_paints: Vec::new(),
        }
    }

    #[test]
    fn box_projection_splits_hard_seams_and_generates_uvs() {
        let source = Mesh::from(Cuboid::new(1.0, 2.0, 3.0));
        let output = apply_uv_edit(&source, &edit(ImportedUvProjection::BoxSeams)).unwrap();
        assert!(output.indices().is_none());
        assert!(matches!(
            output.attribute(Mesh::ATTRIBUTE_UV_0),
            Some(VertexAttributeValues::Float32x2(values)) if values.len() == output.count_vertices()
        ));
    }

    #[test]
    fn face_paint_only_changes_the_selected_triangle() {
        let source = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let mut edit = edit(ImportedUvProjection::Authored);
        edit.face_paints.push(ImportedFacePaint {
            face_indices: vec![1],
            color: [1.0, 0.0, 0.0, 1.0],
        });
        let output = apply_uv_edit(&source, &edit).unwrap();
        let Some(VertexAttributeValues::Float32x4(colors)) =
            output.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("missing paint colors");
        };
        assert_eq!(&colors[3..6], &[[1.0, 0.0, 0.0, 1.0]; 3]);
        assert_eq!(colors[0], [1.0; 4]);
    }
}
