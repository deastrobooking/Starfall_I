//! Blender-inspired, clean-room non-destructive modifier stack for triangle
//! meshes.
//!
//! A modifier stack is an ordered list of deterministic operations applied to
//! a base mesh; the base is never mutated on disk, so editing the stack always
//! re-derives the result (Blender's core modifier idea, reimplemented from
//! scratch for Bevy meshes — no ported code). Every operation is pure and
//! seed-deterministic, so identical stacks always produce identical meshes on
//! every platform. Shared by the three design features: World Kit consumes it
//! first; Character Studio and Creature Forge adopt the same stack next.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModifierAxis {
    X,
    Y,
    Z,
}

impl ModifierAxis {
    pub fn unit(self) -> Vec3 {
        match self {
            Self::X => Vec3::X,
            Self::Y => Vec3::Y,
            Self::Z => Vec3::Z,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
        }
    }
}

/// One non-destructive operation in a modifier stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MeshModifier {
    /// Append a copy of the geometry mirrored across the axis plane through
    /// the local origin (winding flipped so faces stay outward).
    Mirror { axis: ModifierAxis },
    /// Total number of copies laid out along `offset` (count includes the
    /// original, clamped to 1..=16).
    Array { count: u32, offset: [f32; 3] },
    /// Rotate vertices around the axis proportionally to their normalized
    /// position along it; `degrees` is the total twist across the extent.
    Twist { axis: ModifierAxis, degrees: f32 },
    /// Midpoint 4-to-1 triangle subdivision, `levels` clamped to 1..=4.
    Subdivide { levels: u32 },
    /// Laplacian relaxation: each vertex moves toward the average of its edge
    /// neighbours by `factor` per iteration (iterations clamped to 1..=10).
    Smooth { iterations: u32, factor: f32 },
    /// Displace along vertex normals with smooth, seed-deterministic noise.
    NoiseDisplace {
        seed: u64,
        amplitude: f32,
        frequency: f32,
    },
}

impl MeshModifier {
    pub fn label(&self) -> String {
        match self {
            Self::Mirror { axis } => format!("Mirror {}", axis.label()),
            Self::Array { count, .. } => format!("Array x{count}"),
            Self::Twist { axis, degrees } => format!("Twist {}° {}", degrees, axis.label()),
            Self::Subdivide { levels } => format!("Subdivide x{levels}"),
            Self::Smooth { iterations, .. } => format!("Smooth x{iterations}"),
            Self::NoiseDisplace { seed, .. } => format!("Noise (seed {seed})"),
        }
    }
}

/// Plain triangle-mesh buffers the modifiers operate on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshBuffer {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub indices: Vec<u32>,
}

impl MeshBuffer {
    pub fn from_mesh(mesh: &Mesh) -> Option<Self> {
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)?
            .as_float3()?
            .iter()
            .map(|p| Vec3::from_array(*p))
            .collect::<Vec<_>>();
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|attr| attr.as_float3())
            .map(|values| values.iter().map(|n| Vec3::from_array(*n)).collect())
            .unwrap_or_else(|| vec![Vec3::Y; positions.len()]);
        let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(bevy::mesh::VertexAttributeValues::Float32x2(values)) => {
                values.iter().map(|uv| Vec2::from_array(*uv)).collect()
            }
            _ => vec![Vec2::ZERO; positions.len()],
        };
        let indices = match mesh.indices()? {
            Indices::U32(indices) => indices.clone(),
            Indices::U16(indices) => indices.iter().map(|i| *i as u32).collect(),
        };
        Some(Self {
            positions,
            normals,
            uvs,
            indices,
        })
    }

    pub fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            self.positions
                .iter()
                .map(|p| p.to_array())
                .collect::<Vec<_>>(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            self.normals
                .iter()
                .map(|n| n.to_array())
                .collect::<Vec<_>>(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_UV_0,
            self.uvs.iter().map(|uv| uv.to_array()).collect::<Vec<_>>(),
        );
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Area-weighted smooth normals recomputed from the triangle data.
    pub fn recompute_normals(&mut self) {
        let mut normals = vec![Vec3::ZERO; self.positions.len()];
        for triangle in self.indices.chunks_exact(3) {
            let [a, b, c] = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            let face = (self.positions[b] - self.positions[a])
                .cross(self.positions[c] - self.positions[a]);
            normals[a] += face;
            normals[b] += face;
            normals[c] += face;
        }
        self.normals = normals
            .into_iter()
            .map(|n| n.try_normalize().unwrap_or(Vec3::Y))
            .collect();
    }
}

/// Safety valve: stacks stop expanding geometry past this many triangles.
const MAX_TRIANGLES: usize = 262_144;

/// Apply an ordered modifier stack to a mesh buffer. Normals are recomputed
/// once at the end whenever any modifier changed geometry.
pub fn apply_stack(buffer: &mut MeshBuffer, stack: &[MeshModifier]) {
    if stack.is_empty() || buffer.positions.is_empty() {
        return;
    }
    for modifier in stack {
        match modifier {
            MeshModifier::Mirror { axis } => mirror(buffer, *axis),
            MeshModifier::Array { count, offset } => {
                array(buffer, *count, Vec3::from_array(*offset))
            }
            MeshModifier::Twist { axis, degrees } => twist(buffer, *axis, *degrees),
            MeshModifier::Subdivide { levels } => subdivide(buffer, *levels),
            MeshModifier::Smooth { iterations, factor } => smooth(buffer, *iterations, *factor),
            MeshModifier::NoiseDisplace {
                seed,
                amplitude,
                frequency,
            } => noise_displace(buffer, *seed, *amplitude, *frequency),
        }
    }
    buffer.recompute_normals();
}

/// Convenience: apply a stack to a Bevy mesh, returning the derived mesh.
pub fn apply_stack_to_mesh(mesh: &Mesh, stack: &[MeshModifier]) -> Option<Mesh> {
    let mut buffer = MeshBuffer::from_mesh(mesh)?;
    apply_stack(&mut buffer, stack);
    Some(buffer.into_mesh())
}

fn mirror(buffer: &mut MeshBuffer, axis: ModifierAxis) {
    let base_vertices = buffer.positions.len() as u32;
    if buffer.triangle_count() * 2 > MAX_TRIANGLES {
        return;
    }
    let flip = |mut v: Vec3| {
        match axis {
            ModifierAxis::X => v.x = -v.x,
            ModifierAxis::Y => v.y = -v.y,
            ModifierAxis::Z => v.z = -v.z,
        }
        v
    };
    let mirrored_positions: Vec<Vec3> = buffer.positions.iter().map(|p| flip(*p)).collect();
    let mirrored_normals: Vec<Vec3> = buffer.normals.iter().map(|n| flip(*n)).collect();
    buffer.positions.extend(mirrored_positions);
    buffer.normals.extend(mirrored_normals);
    buffer.uvs.extend_from_within(..);
    let mirrored_indices: Vec<u32> = buffer
        .indices
        .chunks_exact(3)
        .flat_map(|t| {
            [
                t[0] + base_vertices,
                t[2] + base_vertices,
                t[1] + base_vertices,
            ]
        })
        .collect();
    buffer.indices.extend(mirrored_indices);
}

fn array(buffer: &mut MeshBuffer, count: u32, offset: Vec3) {
    let count = count.clamp(1, 16) as usize;
    if count <= 1 || buffer.triangle_count() * count > MAX_TRIANGLES {
        return;
    }
    let base_vertices = buffer.positions.len() as u32;
    let base_positions = buffer.positions.clone();
    let base_normals = buffer.normals.clone();
    let base_uvs = buffer.uvs.clone();
    let base_indices = buffer.indices.clone();
    for copy in 1..count {
        let shift = offset * copy as f32;
        buffer
            .positions
            .extend(base_positions.iter().map(|p| *p + shift));
        buffer.normals.extend(base_normals.iter().copied());
        buffer.uvs.extend(base_uvs.iter().copied());
        let vertex_shift = base_vertices * copy as u32;
        buffer
            .indices
            .extend(base_indices.iter().map(|i| i + vertex_shift));
    }
}

fn twist(buffer: &mut MeshBuffer, axis: ModifierAxis, degrees: f32) {
    let unit = axis.unit();
    let along = |v: Vec3| v.dot(unit);
    let (min, max) =
        buffer
            .positions
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), p| {
                let value = along(*p);
                (min.min(value), max.max(value))
            });
    let extent = max - min;
    if extent <= f32::EPSILON {
        return;
    }
    for position in &mut buffer.positions {
        let t = (along(*position) - min) / extent;
        let rotation = Quat::from_axis_angle(unit, (degrees * t).to_radians());
        *position = rotation * *position;
    }
}

fn subdivide(buffer: &mut MeshBuffer, levels: u32) {
    for _ in 0..levels.clamp(1, 4) {
        if buffer.triangle_count() * 4 > MAX_TRIANGLES {
            return;
        }
        let mut midpoints: HashMap<(u32, u32), u32> = HashMap::new();
        let old_indices = std::mem::take(&mut buffer.indices);
        let mut midpoint = |buffer: &mut MeshBuffer, a: u32, b: u32| -> u32 {
            let key = (a.min(b), a.max(b));
            *midpoints.entry(key).or_insert_with(|| {
                let index = buffer.positions.len() as u32;
                let (a, b) = (a as usize, b as usize);
                buffer
                    .positions
                    .push((buffer.positions[a] + buffer.positions[b]) * 0.5);
                buffer
                    .normals
                    .push((buffer.normals[a] + buffer.normals[b]) * 0.5);
                buffer.uvs.push((buffer.uvs[a] + buffer.uvs[b]) * 0.5);
                index
            })
        };
        let mut new_indices = Vec::with_capacity(old_indices.len() * 4);
        for triangle in old_indices.chunks_exact(3) {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
            let ab = midpoint(buffer, a, b);
            let bc = midpoint(buffer, b, c);
            let ca = midpoint(buffer, c, a);
            new_indices.extend_from_slice(&[a, ab, ca, ab, b, bc, ca, bc, c, ab, bc, ca]);
        }
        buffer.indices = new_indices;
    }
}

fn smooth(buffer: &mut MeshBuffer, iterations: u32, factor: f32) {
    let factor = factor.clamp(0.0, 1.0);
    if factor <= 0.0 {
        return;
    }
    let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); buffer.positions.len()];
    for triangle in buffer.indices.chunks_exact(3) {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            if !neighbours[a as usize].contains(&b) {
                neighbours[a as usize].push(b);
            }
            if !neighbours[b as usize].contains(&a) {
                neighbours[b as usize].push(a);
            }
        }
    }
    for _ in 0..iterations.clamp(1, 10) {
        let snapshot = buffer.positions.clone();
        for (index, position) in buffer.positions.iter_mut().enumerate() {
            let linked = &neighbours[index];
            if linked.is_empty() {
                continue;
            }
            let average = linked
                .iter()
                .fold(Vec3::ZERO, |sum, i| sum + snapshot[*i as usize])
                / linked.len() as f32;
            *position = position.lerp(average, factor);
        }
    }
}

/// SplitMix64 — tiny deterministic seed expander.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn noise_displace(buffer: &mut MeshBuffer, seed: u64, amplitude: f32, frequency: f32) {
    let mut state = seed;
    let phase_a = (splitmix64(&mut state) % 6_283) as f32 / 1_000.0;
    let phase_b = (splitmix64(&mut state) % 6_283) as f32 / 1_000.0;
    let phase_c = (splitmix64(&mut state) % 6_283) as f32 / 1_000.0;
    for (position, normal) in buffer.positions.iter_mut().zip(buffer.normals.iter()) {
        let sample = (position.x * frequency + phase_a).sin()
            * (position.y * frequency + phase_b).cos()
            * (position.z * frequency + phase_c).sin();
        *position += *normal * sample * amplitude;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> MeshBuffer {
        MeshBuffer {
            positions: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            normals: vec![Vec3::Z; 4],
            uvs: vec![Vec2::ZERO; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    #[test]
    fn mirror_doubles_geometry_and_flips_positions() {
        let mut buffer = quad();
        apply_stack(
            &mut buffer,
            &[MeshModifier::Mirror {
                axis: ModifierAxis::X,
            }],
        );
        assert_eq!(buffer.positions.len(), 8);
        assert_eq!(buffer.triangle_count(), 4);
        assert_eq!(buffer.positions[5].x, -1.0);
    }

    #[test]
    fn array_lays_out_total_count_copies() {
        let mut buffer = quad();
        apply_stack(
            &mut buffer,
            &[MeshModifier::Array {
                count: 3,
                offset: [2.0, 0.0, 0.0],
            }],
        );
        assert_eq!(buffer.positions.len(), 12);
        assert_eq!(buffer.triangle_count(), 6);
        assert_eq!(buffer.positions[8].x, 4.0);
    }

    #[test]
    fn subdivide_quadruples_triangles_per_level() {
        let mut buffer = quad();
        apply_stack(&mut buffer, &[MeshModifier::Subdivide { levels: 2 }]);
        assert_eq!(buffer.triangle_count(), 32);
        // Shared edge midpoints are welded, not duplicated.
        assert!(buffer.positions.len() < 3 * 32);
    }

    #[test]
    fn twist_rotates_top_extent_but_not_bottom() {
        let mut buffer = quad();
        let bottom_before = buffer.positions[1];
        apply_stack(
            &mut buffer,
            &[MeshModifier::Twist {
                axis: ModifierAxis::Y,
                degrees: 90.0,
            }],
        );
        assert!((buffer.positions[1] - bottom_before).length() < 1e-5);
        assert!((buffer.positions[2] - Vec3::new(1.0, 1.0, 0.0)).length() > 0.5);
    }

    #[test]
    fn smooth_pulls_vertices_toward_neighbours() {
        let mut buffer = quad();
        let before = buffer.positions.clone();
        apply_stack(
            &mut buffer,
            &[MeshModifier::Smooth {
                iterations: 2,
                factor: 0.5,
            }],
        );
        assert_ne!(buffer.positions, before);
        // Smoothing must stay inside the original bounding box.
        for position in &buffer.positions {
            assert!(position.x >= -1e-5 && position.x <= 1.0 + 1e-5);
            assert!(position.y >= -1e-5 && position.y <= 1.0 + 1e-5);
        }
    }

    #[test]
    fn noise_is_deterministic_per_seed() {
        let mut a = quad();
        let mut b = quad();
        let mut c = quad();
        let stack = |seed| {
            [MeshModifier::NoiseDisplace {
                seed,
                amplitude: 0.4,
                frequency: 3.0,
            }]
        };
        apply_stack(&mut a, &stack(7));
        apply_stack(&mut b, &stack(7));
        apply_stack(&mut c, &stack(8));
        assert_eq!(a.positions, b.positions);
        assert_ne!(a.positions, c.positions);
    }

    #[test]
    fn stack_round_trips_through_bevy_mesh_and_serde() {
        let stack = vec![
            MeshModifier::Mirror {
                axis: ModifierAxis::X,
            },
            MeshModifier::Subdivide { levels: 1 },
            MeshModifier::NoiseDisplace {
                seed: 42,
                amplitude: 0.1,
                frequency: 2.0,
            },
        ];
        let json = serde_json::to_string(&stack).unwrap();
        let decoded: Vec<MeshModifier> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, stack);

        let mesh = quad().into_mesh();
        let derived = apply_stack_to_mesh(&mesh, &decoded).expect("mesh converts");
        let buffer = MeshBuffer::from_mesh(&derived).expect("derived converts back");
        // 2 tris → mirror ×2 → subdivide ×4.
        assert_eq!(buffer.triangle_count(), 16);
    }

    #[test]
    fn empty_stack_and_empty_mesh_are_no_ops() {
        let mut buffer = quad();
        let before = buffer.clone();
        apply_stack(&mut buffer, &[]);
        assert_eq!(buffer, before);

        let mut empty = MeshBuffer::default();
        apply_stack(&mut empty, &[MeshModifier::Subdivide { levels: 2 }]);
        assert!(empty.positions.is_empty());
    }
}
