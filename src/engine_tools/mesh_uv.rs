//! Non-destructive UV projection, transforms, and face-color painting.

use std::collections::{BTreeSet, HashMap, VecDeque};

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use super::character_records::{ImportedUvEdit, ImportedUvProjection};
use super::mesh_selection::MeshTopology;

pub fn apply_uv_edit(source: &Mesh, edit: &ImportedUvEdit) -> Option<Mesh> {
    let mut mesh = static_mesh_copy(source);
    let source_topology = MeshTopology::from_mesh(&mesh);
    let source_positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(values) => values.clone(),
        _ => return None,
    };
    let needs_split = matches!(
        edit.projection,
        ImportedUvProjection::BoxSeams | ImportedUvProjection::SeamUnwrap
    ) || !edit.face_paints.is_empty();
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
        ImportedUvProjection::SeamUnwrap => {
            seam_unwrap_uvs(&source_positions, &source_topology?.faces, &edit.seam_edges)?
        }
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

    if !edit.face_paints.is_empty() && edit.baked_paint_texture.is_none() {
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

/// Rasterizes non-destructive face strokes into an RGBA texture using the
/// derived UV coordinates. Later strokes overwrite earlier strokes.
pub fn rasterize_face_paint(source: &Mesh, edit: &ImportedUvEdit, size: u32) -> Option<Vec<u8>> {
    if size < 8 || edit.face_paints.is_empty() {
        return None;
    }
    let mut preview_edit = edit.clone();
    preview_edit.baked_paint_texture = Some("preview.png".into());
    let mesh = apply_uv_edit(source, &preview_edit)?;
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0)? {
        VertexAttributeValues::Float32x2(values) => values,
        _ => return None,
    };
    let faces = MeshTopology::from_mesh(&mesh)?.faces;
    let mut pixels = vec![255_u8; size as usize * size as usize * 4];
    for stroke in &edit.face_paints {
        let rgba = stroke.color.map(|value| (value * 255.0).round() as u8);
        for face in &stroke.face_indices {
            let indices = faces.get(*face as usize)?;
            let triangle = indices.map(|index| {
                let uv = uvs[index as usize];
                Vec2::new(
                    uv[0].clamp(0.0, 1.0) * (size - 1) as f32,
                    uv[1].clamp(0.0, 1.0) * (size - 1) as f32,
                )
            });
            rasterize_triangle(&mut pixels, size, triangle, rgba);
        }
    }
    Some(pixels)
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

fn seam_unwrap_uvs(
    positions: &[[f32; 3]],
    faces: &[[u32; 3]],
    seam_edges: &[[u32; 2]],
) -> Option<Vec<[f32; 2]>> {
    if faces.is_empty() {
        return None;
    }
    let seams: BTreeSet<[u32; 2]> = seam_edges.iter().copied().collect();
    let mut edge_faces: HashMap<[u32; 2], Vec<usize>> = HashMap::new();
    for (face_index, face) in faces.iter().enumerate() {
        for edge in face_edges(*face) {
            edge_faces.entry(edge).or_default().push(face_index);
        }
    }
    let mut neighbors = vec![Vec::new(); faces.len()];
    for (edge, owners) in edge_faces {
        if seams.contains(&edge) {
            continue;
        }
        for &a in &owners {
            neighbors[a].extend(owners.iter().copied().filter(|b| *b != a));
        }
    }
    let mut islands = Vec::<Vec<usize>>::new();
    let mut visited = vec![false; faces.len()];
    for seed in 0..faces.len() {
        if visited[seed] {
            continue;
        }
        visited[seed] = true;
        let mut queue = VecDeque::from([seed]);
        let mut island = Vec::new();
        while let Some(face) = queue.pop_front() {
            island.push(face);
            for neighbor in &neighbors[face] {
                if !visited[*neighbor] {
                    visited[*neighbor] = true;
                    queue.push_back(*neighbor);
                }
            }
        }
        islands.push(island);
    }

    let columns = (islands.len() as f32).sqrt().ceil() as usize;
    let rows = islands.len().div_ceil(columns);
    let cell = Vec2::new(1.0 / columns as f32, 1.0 / rows as f32);
    let padding = cell.min_element() * 0.06;
    let mut output = vec![[0.0; 2]; faces.len() * 3];
    for (island_index, island) in islands.iter().enumerate() {
        let normal_sum = island.iter().fold(Vec3::ZERO, |sum, face_index| {
            let face = faces[*face_index];
            let [a, b, c] = face.map(|index| Vec3::from_array(positions[index as usize]));
            sum + (b - a).cross(c - a)
        });
        let normal = normal_sum.abs();
        let (axis_u, axis_v) = if normal.x >= normal.y && normal.x >= normal.z {
            (2, 1)
        } else if normal.y >= normal.z {
            (0, 2)
        } else {
            (0, 1)
        };
        let island_positions = island
            .iter()
            .flat_map(|face| faces[*face])
            .map(|index| positions[index as usize])
            .collect::<Vec<_>>();
        let (min_u, max_u) = bounds(&island_positions, axis_u);
        let (min_v, max_v) = bounds(&island_positions, axis_v);
        let cell_origin = Vec2::new(
            (island_index % columns) as f32 * cell.x,
            (island_index / columns) as f32 * cell.y,
        );
        for face_index in island {
            for (corner, vertex) in faces[*face_index].iter().enumerate() {
                let position = positions[*vertex as usize];
                let local = Vec2::new(
                    normalized(position[axis_u], min_u, max_u),
                    1.0 - normalized(position[axis_v], min_v, max_v),
                );
                let packed = cell_origin
                    + Vec2::splat(padding)
                    + local * (cell - Vec2::splat(2.0 * padding));
                output[*face_index * 3 + corner] = packed.to_array();
            }
        }
    }
    Some(output)
}

pub fn selection_boundary_edges(
    topology: &MeshTopology,
    selected: &BTreeSet<u32>,
) -> Vec<[u32; 2]> {
    let mut counts: HashMap<[u32; 2], usize> = HashMap::new();
    for face in selected {
        let Some(vertices) = topology.faces.get(*face as usize) else {
            continue;
        };
        for edge in face_edges(*vertices) {
            *counts.entry(edge).or_default() += 1;
        }
    }
    let mut edges = counts
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|(edge, _)| edge)
        .collect::<Vec<_>>();
    edges.sort_unstable();
    edges
}

fn face_edges([a, b, c]: [u32; 3]) -> [[u32; 2]; 3] {
    [sorted_edge(a, b), sorted_edge(b, c), sorted_edge(c, a)]
}

fn sorted_edge(a: u32, b: u32) -> [u32; 2] {
    if a < b {
        [a, b]
    } else {
        [b, a]
    }
}

fn rasterize_triangle(pixels: &mut [u8], size: u32, triangle: [Vec2; 3], rgba: [u8; 4]) {
    let min = triangle
        .iter()
        .copied()
        .fold(Vec2::splat(f32::INFINITY), Vec2::min)
        .floor()
        .max(Vec2::ZERO);
    let max = triangle
        .iter()
        .copied()
        .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max)
        .ceil()
        .min(Vec2::splat((size - 1) as f32));
    let edge = |a: Vec2, b: Vec2, point: Vec2| {
        (point.x - a.x) * (b.y - a.y) - (point.y - a.y) * (b.x - a.x)
    };
    let area = edge(triangle[0], triangle[1], triangle[2]);
    if area.abs() < 1.0e-6 {
        return;
    }
    for y in min.y as u32..=max.y as u32 {
        for x in min.x as u32..=max.x as u32 {
            let point = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let weights = [
                edge(triangle[1], triangle[2], point) / area,
                edge(triangle[2], triangle[0], point) / area,
                edge(triangle[0], triangle[1], point) / area,
            ];
            if weights.iter().all(|weight| *weight >= -1.0e-4) {
                let index = (y as usize * size as usize + x as usize) * 4;
                pixels[index..index + 4].copy_from_slice(&rgba);
            }
        }
    }
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
            seam_edges: Vec::new(),
            face_paints: Vec::new(),
            baked_paint_texture: None,
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

    #[test]
    fn selection_boundary_marks_only_outer_edges() {
        let mesh = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let topology = MeshTopology::from_mesh(&mesh).unwrap();
        let edges = selection_boundary_edges(&topology, &BTreeSet::from([0, 1]));
        assert_eq!(edges.len(), 4);
    }

    #[test]
    fn face_paint_rasterizes_actual_pixels() {
        let source = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let mut edit = edit(ImportedUvProjection::BoxSeams);
        edit.face_paints.push(ImportedFacePaint {
            face_indices: vec![0],
            color: [1.0, 0.0, 0.0, 1.0],
        });
        let pixels = rasterize_face_paint(&source, &edit, 64).unwrap();
        assert!(pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [255, 0, 0, 255]));
        assert!(pixels
            .chunks_exact(4)
            .any(|pixel| pixel == [255, 255, 255, 255]));
    }
}
