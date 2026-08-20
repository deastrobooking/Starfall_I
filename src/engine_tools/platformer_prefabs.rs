//! Modular, original platforming geometry for the Starfall Forge level editor.
//!
//! Prefabs are recipe-first: the serialized scene stores a stable kind while
//! this module derives the preview/runtime mesh.  Designers can use the stock
//! recipe, alter its dimensions, or simply use the editor transform for broad
//! changes without baking transient Bevy handles into project files.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformerPrefabKind {
    Ladder,
    Stairs,
    Tower,
    Castle,
    FloatingIsland,
    MovingPlatform,
    SpringPlatform,
    FlipPanel,
    RotatingBridge,
    CollapseBridge,
    SpikeBridge,
}

/// Stable gameplay intent exported by a prefab recipe. Runtime adapters use
/// this instead of guessing behavior from a mesh or display name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlatformerBehavior {
    StaticTraversal,
    Climbable,
    Oscillate { distance: f32, seconds: f32 },
    Bounce { launch_speed: f32 },
    FlipOnJump,
    Rotate { degrees_per_second: f32 },
    Collapse { warning_seconds: f32 },
    Hazard,
}

impl PlatformerPrefabKind {
    pub const ALL: [Self; 11] = [
        Self::Ladder,
        Self::Stairs,
        Self::Tower,
        Self::Castle,
        Self::FloatingIsland,
        Self::MovingPlatform,
        Self::SpringPlatform,
        Self::FlipPanel,
        Self::RotatingBridge,
        Self::CollapseBridge,
        Self::SpikeBridge,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ladder => "Ladder",
            Self::Stairs => "Stairs",
            Self::Tower => "Star Tower",
            Self::Castle => "Star Castle",
            Self::FloatingIsland => "Floating Island",
            Self::MovingPlatform => "Moving Platform",
            Self::SpringPlatform => "Pulse Spring",
            Self::FlipPanel => "Flip Panel",
            Self::RotatingBridge => "Rotating Bridge",
            Self::CollapseBridge => "Collapse Bridge",
            Self::SpikeBridge => "Spike Bridge",
        }
    }

    pub fn accent_color(self) -> Color {
        match self {
            Self::Ladder => Color::srgb(0.93, 0.67, 0.24),
            Self::Stairs => Color::srgb(0.34, 0.72, 0.94),
            Self::Tower => Color::srgb(0.56, 0.48, 0.92),
            Self::Castle => Color::srgb(0.46, 0.57, 0.88),
            Self::FloatingIsland => Color::srgb(0.36, 0.82, 0.58),
            Self::MovingPlatform => Color::srgb(0.19, 0.84, 0.94),
            Self::SpringPlatform => Color::srgb(1.0, 0.32, 0.64),
            Self::FlipPanel => Color::srgb(0.96, 0.30, 0.38),
            Self::RotatingBridge => Color::srgb(0.98, 0.70, 0.20),
            Self::CollapseBridge => Color::srgb(0.83, 0.48, 0.22),
            Self::SpikeBridge => Color::srgb(0.75, 0.25, 0.36),
        }
    }

    pub fn behavior(self) -> PlatformerBehavior {
        match self {
            Self::Ladder => PlatformerBehavior::Climbable,
            Self::MovingPlatform => PlatformerBehavior::Oscillate {
                distance: 6.0,
                seconds: 3.0,
            },
            Self::SpringPlatform => PlatformerBehavior::Bounce { launch_speed: 1.1 },
            Self::FlipPanel => PlatformerBehavior::FlipOnJump,
            Self::RotatingBridge => PlatformerBehavior::Rotate {
                degrees_per_second: 42.0,
            },
            Self::CollapseBridge => PlatformerBehavior::Collapse {
                warning_seconds: 0.65,
            },
            Self::SpikeBridge => PlatformerBehavior::Hazard,
            Self::Stairs | Self::Tower | Self::Castle | Self::FloatingIsland => {
                PlatformerBehavior::StaticTraversal
            }
        }
    }
}

/// The reusable designer recipe. Dimensions are intentionally normalized so
/// scene transforms remain a useful shared authoring language across kinds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlatformerPrefabDesign {
    pub kind: PlatformerPrefabKind,
    pub size: Vec3,
    /// Repetition count: rungs, steps, battlements, bridge tiles, and so on.
    pub detail_count: u8,
}

impl PlatformerPrefabDesign {
    pub fn stock(kind: PlatformerPrefabKind) -> Self {
        let (size, detail_count) = match kind {
            PlatformerPrefabKind::Ladder => (Vec3::new(2.2, 6.0, 0.45), 7),
            PlatformerPrefabKind::Stairs => (Vec3::new(5.0, 3.2, 7.0), 7),
            PlatformerPrefabKind::Tower => (Vec3::new(6.0, 12.0, 6.0), 8),
            PlatformerPrefabKind::Castle => (Vec3::new(18.0, 9.0, 12.0), 8),
            PlatformerPrefabKind::FloatingIsland => (Vec3::new(8.0, 3.2, 7.0), 4),
            PlatformerPrefabKind::MovingPlatform => (Vec3::new(5.0, 0.6, 4.0), 4),
            PlatformerPrefabKind::SpringPlatform => (Vec3::new(3.2, 1.5, 3.2), 4),
            PlatformerPrefabKind::FlipPanel => (Vec3::new(4.0, 0.35, 4.0), 4),
            PlatformerPrefabKind::RotatingBridge => (Vec3::new(12.0, 0.65, 2.4), 6),
            PlatformerPrefabKind::CollapseBridge => (Vec3::new(12.0, 0.55, 3.0), 8),
            PlatformerPrefabKind::SpikeBridge => (Vec3::new(12.0, 1.1, 3.2), 8),
        };
        Self {
            kind,
            size,
            detail_count,
        }
    }

    pub fn validated(mut self) -> Self {
        self.size = self.size.clamp(Vec3::splat(0.25), Vec3::splat(100.0));
        self.detail_count = self.detail_count.clamp(2, 32);
        self
    }

    pub fn mesh(self) -> Mesh {
        build_mesh(self.validated())
    }

    pub fn selection_radius(self) -> f32 {
        self.size.length() * 0.55
    }
}

#[derive(Default)]
struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn cuboid(&mut self, center: Vec3, size: Vec3) {
        let h = size * 0.5;
        let faces = [
            (
                Vec3::X,
                [
                    Vec3::new(h.x, -h.y, -h.z),
                    Vec3::new(h.x, -h.y, h.z),
                    Vec3::new(h.x, h.y, h.z),
                    Vec3::new(h.x, h.y, -h.z),
                ],
            ),
            (
                Vec3::NEG_X,
                [
                    Vec3::new(-h.x, -h.y, h.z),
                    Vec3::new(-h.x, -h.y, -h.z),
                    Vec3::new(-h.x, h.y, -h.z),
                    Vec3::new(-h.x, h.y, h.z),
                ],
            ),
            (
                Vec3::Y,
                [
                    Vec3::new(-h.x, h.y, -h.z),
                    Vec3::new(h.x, h.y, -h.z),
                    Vec3::new(h.x, h.y, h.z),
                    Vec3::new(-h.x, h.y, h.z),
                ],
            ),
            (
                Vec3::NEG_Y,
                [
                    Vec3::new(-h.x, -h.y, h.z),
                    Vec3::new(h.x, -h.y, h.z),
                    Vec3::new(h.x, -h.y, -h.z),
                    Vec3::new(-h.x, -h.y, -h.z),
                ],
            ),
            (
                Vec3::Z,
                [
                    Vec3::new(h.x, -h.y, h.z),
                    Vec3::new(-h.x, -h.y, h.z),
                    Vec3::new(-h.x, h.y, h.z),
                    Vec3::new(h.x, h.y, h.z),
                ],
            ),
            (
                Vec3::NEG_Z,
                [
                    Vec3::new(-h.x, -h.y, -h.z),
                    Vec3::new(h.x, -h.y, -h.z),
                    Vec3::new(h.x, h.y, -h.z),
                    Vec3::new(-h.x, h.y, -h.z),
                ],
            ),
        ];
        for (normal, corners) in faces {
            let base = self.positions.len() as u32;
            self.positions
                .extend(corners.map(|p| (p + center).to_array()));
            self.normals.extend([normal.to_array(); 4]);
            self.uvs
                .extend([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
            self.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    fn pyramid(&mut self, center: Vec3, size: Vec3) {
        let h = size * 0.5;
        let base_points = [
            center + Vec3::new(-h.x, -h.y, -h.z),
            center + Vec3::new(h.x, -h.y, -h.z),
            center + Vec3::new(h.x, -h.y, h.z),
            center + Vec3::new(-h.x, -h.y, h.z),
        ];
        let tip = center + Vec3::Y * h.y;
        for i in 0..4 {
            let a = base_points[i];
            let b = base_points[(i + 1) % 4];
            let normal = (b - a).cross(tip - a).normalize_or_zero();
            let base = self.positions.len() as u32;
            self.positions
                .extend([a.to_array(), b.to_array(), tip.to_array()]);
            self.normals.extend([normal.to_array(); 3]);
            self.uvs.extend([[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]);
            self.indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
        let normal = Vec3::NEG_Y.to_array();
        let base = self.positions.len() as u32;
        self.positions
            .extend(base_points.map(|point| point.to_array()));
        self.normals.extend([normal; 4]);
        self.uvs
            .extend([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        self.indices
            .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    fn finish(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

fn build_mesh(design: PlatformerPrefabDesign) -> Mesh {
    let mut out = MeshBuilder::default();
    let s = design.size;
    let n = usize::from(design.detail_count);
    match design.kind {
        PlatformerPrefabKind::Ladder => {
            let rail = s.x * 0.12;
            for x in [-s.x * 0.42, s.x * 0.42] {
                out.cuboid(Vec3::new(x, 0.0, 0.0), Vec3::new(rail, s.y, s.z));
            }
            for i in 0..n {
                let y = -s.y * 0.42 + s.y * 0.84 * i as f32 / (n - 1) as f32;
                out.cuboid(Vec3::new(0.0, y, 0.0), Vec3::new(s.x, rail, s.z * 0.8));
            }
        }
        PlatformerPrefabKind::Stairs => {
            for i in 0..n {
                let t = (i + 1) as f32 / n as f32;
                let depth = s.z / n as f32;
                out.cuboid(
                    Vec3::new(
                        0.0,
                        -s.y * 0.5 + s.y * t * 0.5,
                        -s.z * 0.5 + depth * (i as f32 + 0.5),
                    ),
                    Vec3::new(s.x, s.y * t, depth),
                );
            }
        }
        PlatformerPrefabKind::Tower => {
            out.cuboid(Vec3::ZERO, Vec3::new(s.x * 0.72, s.y, s.z * 0.72));
            out.cuboid(Vec3::Y * s.y * 0.38, Vec3::new(s.x, s.y * 0.18, s.z));
            for x in [-0.42, 0.42] {
                for z in [-0.42, 0.42] {
                    out.cuboid(
                        Vec3::new(s.x * x, s.y * 0.53, s.z * z),
                        s * Vec3::new(0.16, 0.12, 0.16),
                    );
                }
            }
        }
        PlatformerPrefabKind::Castle => {
            out.cuboid(
                Vec3::new(0.0, -s.y * 0.12, 0.0),
                Vec3::new(s.x * 0.68, s.y * 0.7, s.z * 0.75),
            );
            for x in [-0.42, 0.42] {
                for z in [-0.36, 0.36] {
                    out.cuboid(
                        Vec3::new(s.x * x, 0.0, s.z * z),
                        Vec3::new(s.x * 0.18, s.y, s.z * 0.22),
                    );
                    out.cuboid(
                        Vec3::new(s.x * x, s.y * 0.48, s.z * z),
                        Vec3::new(s.x * 0.23, s.y * 0.12, s.z * 0.27),
                    );
                }
            }
            // Split doorway keeps the silhouette readable and traversable.
            out.cuboid(
                Vec3::new(-s.x * 0.19, -s.y * 0.28, s.z * 0.39),
                Vec3::new(s.x * 0.28, s.y * 0.45, s.z * 0.08),
            );
            out.cuboid(
                Vec3::new(s.x * 0.19, -s.y * 0.28, s.z * 0.39),
                Vec3::new(s.x * 0.28, s.y * 0.45, s.z * 0.08),
            );
        }
        PlatformerPrefabKind::FloatingIsland => {
            out.cuboid(Vec3::Y * s.y * 0.35, Vec3::new(s.x, s.y * 0.3, s.z));
            for i in 0..n {
                let t = i as f32 / n as f32;
                out.cuboid(
                    Vec3::new(0.0, s.y * (0.14 - t * 0.22), 0.0),
                    Vec3::new(s.x * (0.82 - t * 0.15), s.y * 0.2, s.z * (0.82 - t * 0.15)),
                );
            }
        }
        PlatformerPrefabKind::MovingPlatform => {
            out.cuboid(Vec3::ZERO, s);
            out.cuboid(
                Vec3::Y * s.y * 0.62,
                Vec3::new(s.x * 0.35, s.y * 0.22, s.z * 0.12),
            );
        }
        PlatformerPrefabKind::SpringPlatform => {
            out.cuboid(
                Vec3::new(0.0, -s.y * 0.38, 0.0),
                Vec3::new(s.x, s.y * 0.24, s.z),
            );
            for i in 0..n {
                let y = -s.y * 0.2 + i as f32 * s.y * 0.14;
                let x = if i % 2 == 0 { -s.x * 0.13 } else { s.x * 0.13 };
                out.cuboid(
                    Vec3::new(x, y, 0.0),
                    Vec3::new(s.x * 0.5, s.y * 0.09, s.z * 0.2),
                );
            }
            out.cuboid(
                Vec3::Y * s.y * 0.38,
                Vec3::new(s.x * 0.85, s.y * 0.18, s.z * 0.85),
            );
        }
        PlatformerPrefabKind::FlipPanel => {
            out.cuboid(Vec3::ZERO, s);
            out.cuboid(
                Vec3::Y * s.y * 0.65,
                Vec3::new(s.x * 0.72, s.y * 0.35, s.z * 0.72),
            );
        }
        PlatformerPrefabKind::RotatingBridge => {
            out.cuboid(Vec3::ZERO, s);
            out.cuboid(
                Vec3::new(0.0, -s.y * 0.8, 0.0),
                Vec3::new(s.z * 0.7, s.y * 1.1, s.z * 0.7),
            );
        }
        PlatformerPrefabKind::CollapseBridge | PlatformerPrefabKind::SpikeBridge => {
            let tile = s.x / n as f32;
            for i in 0..n {
                let x = -s.x * 0.5 + tile * (i as f32 + 0.5);
                out.cuboid(
                    Vec3::new(x, -s.y * 0.2, 0.0),
                    Vec3::new(tile * 0.88, s.y * 0.4, s.z),
                );
                if design.kind == PlatformerPrefabKind::SpikeBridge && i % 2 == 1 {
                    out.pyramid(
                        Vec3::new(x, s.y * 0.3, 0.0),
                        Vec3::new(tile * 0.65, s.y * 0.8, s.z * 0.55),
                    );
                }
            }
        }
    }
    out.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stock_prefab_builds_a_triangle_mesh() {
        for kind in PlatformerPrefabKind::ALL {
            let mesh = PlatformerPrefabDesign::stock(kind).mesh();
            assert_eq!(mesh.primitive_topology(), PrimitiveTopology::TriangleList);
            assert!(mesh.count_vertices() > 0, "{} was empty", kind.label());
        }
    }

    #[test]
    fn designer_clamps_unsafe_recipe_values() {
        let design = PlatformerPrefabDesign {
            kind: PlatformerPrefabKind::Stairs,
            size: Vec3::ZERO,
            detail_count: 0,
        }
        .validated();
        assert_eq!(design.size, Vec3::splat(0.25));
        assert_eq!(design.detail_count, 2);
    }

    #[test]
    fn kinetic_prefabs_export_explicit_gameplay_intent() {
        assert!(matches!(
            PlatformerPrefabKind::SpringPlatform.behavior(),
            PlatformerBehavior::Bounce { .. }
        ));
        assert!(matches!(
            PlatformerPrefabKind::MovingPlatform.behavior(),
            PlatformerBehavior::Oscillate { .. }
        ));
        assert_eq!(
            PlatformerPrefabKind::Ladder.behavior(),
            PlatformerBehavior::Climbable
        );
    }
}
