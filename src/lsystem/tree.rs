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

use super::{turtle::TurtleResult, LSystem};
use crate::components::world::WorldGeometry;

// ── Tree species ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    Oak,
    Pine,
    Dead,
    Cyber,
}

/// Cached geometry + materials for one tree species.
pub struct TreeTemplate {
    pub kind: TreeKind,
    pub geometry: TurtleResult,
    pub bark_mat: Handle<StandardMaterial>,
    pub leaf_mat: Option<Handle<StandardMaterial>>,
}

impl TreeTemplate {
    /// Build the template — evaluates the L-system and allocates materials once.
    pub fn new(kind: TreeKind, materials: &mut Assets<StandardMaterial>) -> Self {
        let ls = lsystem_for(kind);
        let geometry = ls.evaluate();
        let bark_mat = materials.add(bark_material(kind));
        let leaf_mat = leaf_material(kind).map(|m| materials.add(m));
        Self {
            kind,
            geometry,
            bark_mat,
            leaf_mat,
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

/// Instantiate a tree from a pre-built `TreeTemplate` at `position` with the
/// given Y-axis `rotation_y` (radians).  All branch/leaf entities are spawned
/// as children of the root so `despawn_recursive` removes the whole tree.
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

    let root = commands
        .spawn((
            SpatialBundle::from_transform(root_transform),
            WorldGeometry,
            TreeRoot,
        ))
        .id();

    commands.entity(root).with_children(|parent| {
        // ── Branch segments ──────────────────────────────────────────────
        for seg in &template.geometry.branches {
            let dir = seg.end - seg.start;
            let length = dir.length();
            if length < 0.001 {
                continue;
            }

            let mid = (seg.start + seg.end) * 0.5;
            let rot = super::turtle::orient_along(dir);

            parent.spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(seg.radius, length))),
                material: MeshMaterial3d(template.bark_mat.clone()),
                transform: Transform {
                    translation: mid,
                    rotation: rot,
                    scale: Vec3::ONE,
                },
                ..default()
            });
        }

        // ── Leaf clusters ────────────────────────────────────────────────
        if let Some(leaf_mat) = &template.leaf_mat {
            for leaf in &template.geometry.leaves {
                parent.spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Sphere::new(leaf.size))),
                    material: MeshMaterial3d(leaf_mat.clone()),
                    transform: Transform::from_translation(leaf.position),
                    ..default()
                });
            }
        }
    });

    root
}
