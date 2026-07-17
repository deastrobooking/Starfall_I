#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
/// Tree species definitions and Bevy spawn helpers.
///
/// Four species, each backed by a distinct L-system:
///
///   Oak      – 2-D spreading deciduous, 4 iterations, 3^4 = 81 branch segs.
///   Pine     – 3-D narrow conifer, 3 iterations, 4^3 = 64 branch segs.
///   Dead     – bare branching (no leaves), same grammar as Oak.
///   Cyber    – neon-emissive fantasy tree, same grammar as Pine.
///
/// All geometry is local to the tree root (origin = base of trunk).
/// Call `spawn_tree()` to instantiate a species at an arbitrary world position.
use bevy::prelude::*;

use crate::spatial_lod::{SpatialLod, SpatialLodProxy};

use super::{turtle::TurtleResult, LSystem};
use crate::components::world::WorldGeometry;
use crate::modular_character::bake_character_mesh as bake_meshes;
use crate::rendering::PbrBundle;

// ── Tree species ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    Oak,
    Pine,
    Dead,
    Cyber,
}

/// Cached geometry + materials for one tree species.
///
/// `branch_mesh`/`leaf_mesh` are shared *unit* primitives (radius 1, height 1)
/// reused by every segment and leaf of every tree; the per-instance size is
/// baked into the entity's `Transform.scale`. Reusing one mesh handle (instead
/// of allocating a fresh `Cylinder`/`Sphere` per primitive) keeps `Assets<Mesh>`
/// tiny and lets Bevy batch all branches/leaves into a handful of draw calls.
pub struct TreeTemplate {
    pub kind: TreeKind,
    pub geometry: TurtleResult,
    pub bark_mat: Handle<StandardMaterial>,
    pub leaf_mat: Option<Handle<StandardMaterial>>,
    pub branch_mesh: Handle<Mesh>,
    pub leaf_mesh: Handle<Mesh>,
}

impl TreeTemplate {
    /// A unit cylinder (radius 1, height 1) along +Y, shared by all branches.
    pub fn unit_branch_mesh(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        meshes.add(Cylinder::new(1.0, 1.0))
    }

    /// A unit sphere (radius 1), shared by all leaf clusters.
    pub fn unit_leaf_mesh(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        meshes.add(Sphere::new(1.0))
    }

    /// Build the template — evaluates the L-system and allocates materials once.
    pub fn new(
        kind: TreeKind,
        materials: &mut Assets<StandardMaterial>,
        meshes: &mut Assets<Mesh>,
    ) -> Self {
        let bark_mat = materials.add(bark_material(kind));
        let leaf_mat = leaf_material(kind).map(|m| materials.add(m));
        Self::new_with_materials(
            kind,
            bark_mat,
            leaf_mat,
            Self::unit_branch_mesh(meshes),
            Self::unit_leaf_mesh(meshes),
        )
    }

    pub fn new_with_materials(
        kind: TreeKind,
        bark_mat: Handle<StandardMaterial>,
        leaf_mat: Option<Handle<StandardMaterial>>,
        branch_mesh: Handle<Mesh>,
        leaf_mesh: Handle<Mesh>,
    ) -> Self {
        let ls = lsystem_for(kind);
        let geometry = ls.evaluate();
        Self {
            kind,
            geometry,
            bark_mat,
            leaf_mat,
            branch_mesh,
            leaf_mesh,
        }
    }
}

// ── L-system definitions ──────────────────────────────────────────────────────

fn lsystem_for(kind: TreeKind) -> LSystem {
    match kind {
        // ── Oak: classic 2-D symmetric bifurcation ─────────────────────────
        // Rule F → F[+F][-F] produces balanced binary branching in a plane.
        // 4 iterations → 3^4 = 81 branch segments.
        // Random Y-rotation at spawn gives 3-D variety across instances.
        TreeKind::Oak => LSystem::new(
            "F",
            &[("F", "F[+F][-F]")],
            4,
            22.5, // angle_deg
            1.0,  // step
            0.68, // length_scale – branches shorten each level
            0.14, // start_radius
            0.62, // radius_scale
        ),

        // ── Pine: 3-D four-way branching, upward sweep ─────────────────────
        // F → F[^F][-F][+F] adds forward, up, left, right children.
        // 3 iterations → 4^3 = 64 branch segments with good vertical spread.
        TreeKind::Pine => LSystem::new(
            "F",
            &[("F", "F[^F][-F][+F]")],
            3,
            25.0,
            1.0,
            0.72,
            0.12,
            0.65,
        ),

        // ── Dead: bare trunk with sparse forking ───────────────────────────
        // F → F[+F]F[-F] creates a slightly asymmetric skeleton without leaves.
        // 3 iterations is enough for a gaunt silhouette.
        TreeKind::Dead => LSystem::new("F", &[("F", "F[+F]F[-F]")], 3, 30.0, 1.0, 0.60, 0.10, 0.55),

        // ── Cyber: 3-D spread like Pine, but glowing leaf clusters ─────────
        // Identical grammar to Pine; visual difference is entirely in materials.
        TreeKind::Cyber => LSystem::new(
            "F",
            &[("F", "F[^F][-F][+F]")],
            3,
            28.0,
            1.0,
            0.70,
            0.13,
            0.60,
        ),
    }
}

// ── Materials ─────────────────────────────────────────────────────────────────

fn bark_material(kind: TreeKind) -> StandardMaterial {
    match kind {
        TreeKind::Oak => StandardMaterial {
            base_color: Color::srgb(0.22, 0.14, 0.08),
            metallic: 0.0,
            perceptual_roughness: 0.95,
            ..default()
        },
        TreeKind::Pine => StandardMaterial {
            base_color: Color::srgb(0.30, 0.18, 0.09),
            metallic: 0.0,
            perceptual_roughness: 0.90,
            ..default()
        },
        TreeKind::Dead => StandardMaterial {
            base_color: Color::srgb(0.35, 0.30, 0.26),
            metallic: 0.05,
            perceptual_roughness: 0.98,
            ..default()
        },
        TreeKind::Cyber => StandardMaterial {
            base_color: Color::srgb(0.10, 0.10, 0.18),
            emissive: LinearRgba::new(0.0, 0.15, 0.40, 1.0),
            metallic: 0.6,
            perceptual_roughness: 0.4,
            ..default()
        },
    }
}

fn leaf_material(kind: TreeKind) -> Option<StandardMaterial> {
    match kind {
        TreeKind::Oak => Some(StandardMaterial {
            base_color: Color::srgb(0.12, 0.28, 0.10),
            emissive: LinearRgba::new(0.04, 0.14, 0.02, 1.0),
            metallic: 0.0,
            perceptual_roughness: 0.85,
            ..default()
        }),
        TreeKind::Pine => Some(StandardMaterial {
            base_color: Color::srgb(0.08, 0.22, 0.08),
            emissive: LinearRgba::new(0.02, 0.10, 0.02, 1.0),
            metallic: 0.0,
            perceptual_roughness: 0.90,
            ..default()
        }),
        TreeKind::Dead => None, // no leaves
        TreeKind::Cyber => Some(StandardMaterial {
            base_color: Color::srgb(0.05, 0.30, 0.60),
            emissive: LinearRgba::new(0.0, 1.2, 3.0, 1.0),
            metallic: 0.2,
            perceptual_roughness: 0.5,
            alpha_mode: AlphaMode::Add,
            ..default()
        }),
    }
}

// ── Spawn ─────────────────────────────────────────────────────────────────────

/// Marker component so the world cleanup system can despawn trees with the rest.
#[derive(Component)]
pub struct TreeRoot;

#[derive(Debug, Clone, Copy)]
struct TreeProxyShape {
    trunk_center_y: f32,
    trunk_height: f32,
    trunk_radius: f32,
    canopy_center: Vec3,
    canopy_half_extents: Vec3,
}

fn tree_proxy_shape(geometry: &TurtleResult) -> TreeProxyShape {
    let mut branch_min = Vec3::splat(f32::INFINITY);
    let mut branch_max = Vec3::splat(f32::NEG_INFINITY);
    let mut trunk_radius = 0.04f32;
    for branch in &geometry.branches {
        branch_min = branch_min.min(branch.start).min(branch.end);
        branch_max = branch_max.max(branch.start).max(branch.end);
        trunk_radius = trunk_radius.max(branch.radius);
    }
    if !branch_min.is_finite() || !branch_max.is_finite() {
        branch_min = Vec3::ZERO;
        branch_max = Vec3::Y;
    }

    let mut canopy_min = Vec3::splat(f32::INFINITY);
    let mut canopy_max = Vec3::splat(f32::NEG_INFINITY);
    for leaf in &geometry.leaves {
        let radius = Vec3::splat(leaf.size);
        canopy_min = canopy_min.min(leaf.position - radius);
        canopy_max = canopy_max.max(leaf.position + radius);
    }
    if !canopy_min.is_finite() || !canopy_max.is_finite() {
        canopy_min = branch_min;
        canopy_max = branch_max;
    }

    TreeProxyShape {
        trunk_center_y: (branch_min.y + branch_max.y) * 0.5,
        trunk_height: (branch_max.y - branch_min.y).max(0.1),
        trunk_radius: (trunk_radius * 1.2).max(0.05),
        canopy_center: (canopy_min + canopy_max) * 0.5,
        canopy_half_extents: ((canopy_max - canopy_min) * 0.5).max(Vec3::splat(0.1)),
    }
}

/// Instantiate a tree from a pre-built `TreeTemplate` at `position` with the
/// given Y-axis `rotation_y` (radians).
///
/// All branch segments are baked into a **single** bark mesh and all leaf
/// clusters into a single foliage mesh (both in tree-local space). A tree is
/// therefore 1–2 entities instead of ~100, which keeps transform propagation
/// and render extraction cheap when hundreds of trees populate the world. The
/// merged meshes ride on the root/child transforms so `NatureSway` still
/// animates the whole tree.
pub fn spawn_tree(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    template: &TreeTemplate,
    position: Vec3,
    rotation_y: f32,
    scale: f32,
) -> Entity {
    let root_transform = Transform::from_translation(position)
        .with_rotation(Quat::from_rotation_y(rotation_y))
        .with_scale(Vec3::splat(scale));

    // ── Bake every branch segment into one bark mesh (tree-local space) ───────
    let branch_parts: Vec<(Handle<Mesh>, Mat4)> = template
        .geometry
        .branches
        .iter()
        .filter_map(|seg| {
            let dir = seg.end - seg.start;
            let length = dir.length();
            if length < 0.001 {
                return None;
            }
            let mid = (seg.start + seg.end) * 0.5;
            // Unit cylinder (axis +Y): X/Z = radius, Y = length, oriented to `dir`.
            let model = Mat4::from_scale_rotation_translation(
                Vec3::new(seg.radius, length, seg.radius),
                super::turtle::orient_along(dir),
                mid,
            );
            Some((template.branch_mesh.clone(), model))
        })
        .collect();
    let bark_mesh = bake_meshes(meshes, &branch_parts);
    let bark_handle = meshes.add(bark_mesh);

    let root = commands
        .spawn((
            PbrBundle {
                mesh: Mesh3d(bark_handle),
                material: MeshMaterial3d(template.bark_mat.clone()),
                transform: root_transform,
                ..default()
            },
            WorldGeometry,
            TreeRoot,
            SpatialLod::foliage_high(),
        ))
        .id();

    // ── Bake every leaf cluster into one foliage mesh ─────────────────────────
    if let Some(leaf_mat) = &template.leaf_mat {
        let leaf_parts: Vec<(Handle<Mesh>, Mat4)> = template
            .geometry
            .leaves
            .iter()
            .map(|leaf| {
                let model = Mat4::from_scale_rotation_translation(
                    Vec3::splat(leaf.size),
                    Quat::IDENTITY,
                    leaf.position,
                );
                (template.leaf_mesh.clone(), model)
            })
            .collect();
        if !leaf_parts.is_empty() {
            let foliage_mesh = bake_meshes(meshes, &leaf_parts);
            let foliage_handle = meshes.add(foliage_mesh);
            let leaf_mat = leaf_mat.clone();
            commands.entity(root).with_children(|parent| {
                parent.spawn(PbrBundle {
                    mesh: Mesh3d(foliage_handle),
                    material: MeshMaterial3d(leaf_mat),
                    transform: Transform::IDENTITY,
                    ..default()
                });
            });
        }
    }

    // N11b: a two-primitive proxy replaces dozens of merged branches and leaf
    // clusters after the near tier fades. The primitive handles and materials
    // are shared by the whole species, so this adds no per-tree mesh assets.
    let proxy = tree_proxy_shape(&template.geometry);
    commands.entity(root).with_children(|parent| {
        parent.spawn((
            PbrBundle {
                mesh: Mesh3d(template.branch_mesh.clone()),
                material: MeshMaterial3d(template.bark_mat.clone()),
                transform: Transform::from_xyz(0.0, proxy.trunk_center_y, 0.0).with_scale(
                    Vec3::new(proxy.trunk_radius, proxy.trunk_height, proxy.trunk_radius),
                ),
                ..default()
            },
            WorldGeometry,
            SpatialLod::foliage_proxy(),
            SpatialLodProxy,
        ));

        if let Some(leaf_mat) = &template.leaf_mat {
            parent.spawn((
                PbrBundle {
                    mesh: Mesh3d(template.leaf_mesh.clone()),
                    material: MeshMaterial3d(leaf_mat.clone()),
                    transform: Transform::from_translation(proxy.canopy_center)
                        .with_scale(proxy.canopy_half_extents),
                    ..default()
                },
                WorldGeometry,
                SpatialLod::foliage_proxy(),
                SpatialLodProxy,
            ));
        }
    });

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsystem::turtle::{BranchSegment, LeafCluster};

    #[test]
    fn proxy_shape_encloses_authored_canopy_and_trunk() {
        let geometry = TurtleResult {
            branches: vec![BranchSegment {
                start: Vec3::ZERO,
                end: Vec3::new(0.0, 8.0, 0.0),
                radius: 0.5,
                depth: 0,
            }],
            leaves: vec![LeafCluster {
                position: Vec3::new(2.0, 7.0, -1.0),
                size: 2.0,
                depth: 2,
            }],
        };

        let proxy = tree_proxy_shape(&geometry);
        assert_eq!(proxy.trunk_center_y, 4.0);
        assert_eq!(proxy.trunk_height, 8.0);
        assert!(proxy.trunk_radius >= 0.5);
        assert_eq!(proxy.canopy_center, Vec3::new(2.0, 7.0, -1.0));
        assert_eq!(proxy.canopy_half_extents, Vec3::splat(2.0));
    }
}
