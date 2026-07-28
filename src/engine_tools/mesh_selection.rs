//! Topology-aware triangle selection shared by mesh editing tools.

use std::collections::{BTreeSet, HashMap};

use bevy::math::Affine3A;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

#[derive(Debug, Clone)]
pub struct MeshTopology {
    pub faces: Vec<[u32; 3]>,
    pub centers: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    neighbors: Vec<BTreeSet<u32>>,
}

impl MeshTopology {
    pub fn from_mesh(mesh: &Mesh) -> Option<Self> {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
            return None;
        }
        let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
            VertexAttributeValues::Float32x3(values) => values,
            _ => return None,
        };
        let indices: Vec<u32> = match mesh.indices() {
            Some(Indices::U16(values)) => values.iter().map(|value| u32::from(*value)).collect(),
            Some(Indices::U32(values)) => values.clone(),
            None => (0..positions.len() as u32).collect(),
        };
        let faces = indices
            .chunks_exact(3)
            .map(|face| [face[0], face[1], face[2]])
            .collect::<Vec<_>>();
        if faces.is_empty()
            || faces
                .iter()
                .flatten()
                .any(|index| *index as usize >= positions.len())
        {
            return None;
        }

        let mut centers = Vec::with_capacity(faces.len());
        let mut normals = Vec::with_capacity(faces.len());
        let mut vertex_faces: HashMap<u32, Vec<u32>> = HashMap::new();
        for (face_index, face) in faces.iter().enumerate() {
            let [a, b, c] = face.map(|index| Vec3::from_array(positions[index as usize]));
            centers.push((a + b + c) / 3.0);
            normals.push((b - a).cross(c - a).normalize_or_zero());
            for vertex in face {
                vertex_faces
                    .entry(*vertex)
                    .or_default()
                    .push(face_index as u32);
            }
        }
        let mut neighbors = vec![BTreeSet::new(); faces.len()];
        for connected in vertex_faces.values() {
            for face in connected {
                neighbors[*face as usize].extend(
                    connected
                        .iter()
                        .copied()
                        .filter(|candidate| candidate != face),
                );
            }
        }
        Some(Self {
            faces,
            centers,
            normals,
            neighbors,
        })
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    pub fn linked(&self, seed: u32) -> BTreeSet<u32> {
        if seed as usize >= self.faces.len() {
            return BTreeSet::new();
        }
        let mut selected = BTreeSet::from([seed]);
        let mut frontier = vec![seed];
        while let Some(face) = frontier.pop() {
            for neighbor in &self.neighbors[face as usize] {
                if selected.insert(*neighbor) {
                    frontier.push(*neighbor);
                }
            }
        }
        selected
    }

    pub fn grow(&self, selected: &BTreeSet<u32>) -> BTreeSet<u32> {
        let mut grown = selected.clone();
        for face in selected {
            if let Some(neighbors) = self.neighbors.get(*face as usize) {
                grown.extend(neighbors);
            }
        }
        grown
    }

    pub fn shrink(&self, selected: &BTreeSet<u32>) -> BTreeSet<u32> {
        selected
            .iter()
            .copied()
            .filter(|face| {
                self.neighbors
                    .get(*face as usize)
                    .is_some_and(|neighbors| neighbors.iter().all(|item| selected.contains(item)))
            })
            .collect()
    }

    pub fn invert(&self, selected: &BTreeSet<u32>) -> BTreeSet<u32> {
        (0..self.faces.len() as u32)
            .filter(|face| !selected.contains(face))
            .collect()
    }

    pub fn similar_normal(&self, seed: u32, cosine_threshold: f32) -> BTreeSet<u32> {
        let Some(seed_normal) = self.normals.get(seed as usize) else {
            return BTreeSet::new();
        };
        self.normals
            .iter()
            .enumerate()
            .filter(|(_, normal)| seed_normal.dot(**normal) >= cosine_threshold)
            .map(|(index, _)| index as u32)
            .collect()
    }

    pub fn mirrored_x(&self, selected: &BTreeSet<u32>) -> BTreeSet<u32> {
        let mut mirrored = selected.clone();
        for face in selected {
            let Some(center) = self.centers.get(*face as usize) else {
                continue;
            };
            let target = Vec3::new(-center.x, center.y, center.z);
            if let Some((index, _)) = self.centers.iter().enumerate().min_by(|(_, a), (_, b)| {
                a.distance_squared(target)
                    .total_cmp(&b.distance_squared(target))
            }) {
                mirrored.insert(index as u32);
            }
        }
        mirrored
    }
}

/// Creates a static, display-only mesh containing only the selected triangles.
///
/// Vertex buffers stay intact and only the index buffer is filtered. Skin and
/// morph metadata are deliberately omitted because the editor preview does not
/// spawn the source GLTF's deformation components.
pub fn selected_face_mesh(source: &Mesh, selected: &BTreeSet<u32>) -> Option<Mesh> {
    let topology = MeshTopology::from_mesh(source)?;
    let indices = selected
        .iter()
        .filter_map(|face| topology.faces.get(*face as usize))
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    if indices.is_empty() {
        return None;
    }
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
    result.insert_indices(Indices::U32(indices));
    Some(result)
}

/// Filters faces while retaining morph deltas when vertex buffers remain
/// one-to-one with the authored source.
pub fn selected_face_mesh_preserving_morphs(
    source: &Mesh,
    selected: &BTreeSet<u32>,
) -> Option<Mesh> {
    let mut result = selected_face_mesh(source, selected)?;
    if result.count_vertices() != source.count_vertices() {
        return Some(result);
    }
    if let Some(targets) = source.morph_targets() {
        result.set_morph_targets(targets.clone());
    }
    if let Some(names) = source.morph_target_names() {
        result.set_morph_target_names(names.to_vec());
    }
    Some(result)
}

/// Returns the nearest source triangle hit by a world-space ray.
pub fn pick_face(
    source: &Mesh,
    mesh_to_world: Affine3A,
    origin: Vec3,
    direction: Vec3,
) -> Option<(u32, f32)> {
    let positions = match source.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(values) => values,
        _ => return None,
    };
    let topology = MeshTopology::from_mesh(source)?;
    topology
        .faces
        .iter()
        .enumerate()
        .filter_map(|(face_index, face)| {
            let triangle = face.map(|index| {
                mesh_to_world.transform_point3(Vec3::from_array(positions[index as usize]))
            });
            ray_triangle_distance(origin, direction, triangle)
                .map(|distance| (face_index as u32, distance))
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
}

fn ray_triangle_distance(origin: Vec3, direction: Vec3, triangle: [Vec3; 3]) -> Option<f32> {
    let edge_ab = triangle[1] - triangle[0];
    let edge_ac = triangle[2] - triangle[0];
    let cross = direction.cross(edge_ac);
    let determinant = edge_ab.dot(cross);
    if determinant.abs() < 1.0e-7 {
        return None;
    }
    let inverse = determinant.recip();
    let from_a = origin - triangle[0];
    let u = from_a.dot(cross) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = from_a.cross(edge_ab);
    let v = direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = edge_ac.dot(q) * inverse;
    (distance >= 0.0).then_some(distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_selection_can_grow_invert_and_filter() {
        let mesh = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let topology = MeshTopology::from_mesh(&mesh).unwrap();
        assert_eq!(topology.face_count(), 12);
        let one = BTreeSet::from([0]);
        assert!(topology.grow(&one).len() > 1);
        assert_eq!(topology.invert(&one).len(), 11);
        let filtered = selected_face_mesh(&mesh, &one).unwrap();
        assert_eq!(filtered.indices().unwrap().len(), 3);
    }

    #[test]
    fn linked_selection_stops_at_split_vertex_seams() {
        let mesh = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let topology = MeshTopology::from_mesh(&mesh).unwrap();
        // Bevy's cube duplicates vertices at hard-normal seams, so each side
        // is a separate two-triangle topology island.
        assert_eq!(topology.linked(0).len(), 2);
    }

    #[test]
    fn ray_pick_returns_the_nearest_cube_face() {
        let mesh = Mesh::from(Cuboid::new(2.0, 2.0, 2.0));
        let hit = pick_face(
            &mesh,
            Affine3A::IDENTITY,
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::NEG_Z,
        );
        assert!(hit.is_some());
        assert!((hit.unwrap().1 - 4.0).abs() < 0.001);
    }

    #[test]
    fn face_filter_can_preserve_morph_targets() {
        let mut source = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let vertex_count = source.count_vertices();
        source.set_morph_targets(vec![default(); vertex_count * 2]);
        source.set_morph_target_names(vec!["Wide".into(), "Narrow".into()]);
        let filtered = selected_face_mesh_preserving_morphs(&source, &BTreeSet::from([0])).unwrap();
        assert_eq!(filtered.morph_targets().unwrap().len(), vertex_count * 2);
        assert_eq!(filtered.morph_target_names().unwrap(), ["Wide", "Narrow"]);
    }
}
