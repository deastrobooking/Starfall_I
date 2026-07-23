//! Human mesh generator — builds a full male/female human (plus clothes,
//! super suit, or mecha armor overlays) entirely from math, driven by a
//! [`CharacterPatch`]. No external assets: every part is generated here from
//! preset templates (lofted superellipse columns + ovoids), so the studio can
//! later export what it built (e.g. to `.glb`) because we own every vertex.
//!
//! Anthropometric layout (fractions of total height `H`, feet at y = 0):
//! hip 0.52 · waist 0.615 · chest 0.72 · shoulder 0.815 · head centre 0.925.
//! Sex, morphs and outfit layers perturb this base skeleton of proportions.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::cell::Cell;
use std::f32::consts::TAU;

use super::generators::{CharacterPatch, CharacterSlot, SlotContent};
use super::spec::{ArmorStyle, BottomStyle, FlairStyle, FootStyle, HairStyle, Sex, TopStyle};
use crate::procedural_meshes::{anime_eye_mesh, superellipsoid_mesh};

// ── Lofted column primitive ───────────────────────────────────────────────────

/// A vertical loft between two superellipse cross-sections with an optional
/// mid-height bulge — the workhorse for limbs and torso segments. `exponent`
/// 2.0 = elliptical (organic), lower = boxier (armor plating).
fn lofted_column(
    bottom: Vec2,
    top: Vec2,
    height: f32,
    bulge: f32,
    exponent: f32,
    rings: usize,
    sectors: usize,
) -> Mesh {
    let rings = rings.max(3);
    let sectors = sectors.max(8);
    let e = 2.0 / exponent.max(0.2);

    let mut positions = Vec::with_capacity((rings + 1) * (sectors + 1) + 2);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    let mut indices: Vec<u32> = Vec::new();

    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let y = height * (v - 0.5);
        // Smooth radius blend + sine bulge peaking mid-column.
        let base = bottom.lerp(top, v * v * (3.0 - 2.0 * v));
        let swell = 1.0 + bulge * (v * std::f32::consts::PI).sin();
        let radii = base * swell;
        for s in 0..=sectors {
            let u = s as f32 / sectors as f32;
            let a = u * TAU;
            let (sa, ca) = a.sin_cos();
            let px = radii.x * ca.signum() * ca.abs().powf(e);
            let pz = radii.y * sa.signum() * sa.abs().powf(e);
            positions.push([px, y, pz]);
            let n = Vec3::new(
                if radii.x > 0.001 { px / radii.x } else { 0.0 },
                0.0,
                if radii.y > 0.001 { pz / radii.y } else { 0.0 },
            )
            .normalize_or_zero();
            normals.push([n.x, n.y, n.z]);
            uvs.push([u, v]);
        }
    }
    let stride = (sectors + 1) as u32;
    for ring in 0..rings as u32 {
        for s in 0..sectors as u32 {
            let a = ring * stride + s;
            let b = a + stride;
            indices.extend_from_slice(&[a, b, a + 1, b, b + 1, a + 1]);
        }
    }

    // End caps (fan to centre points).
    for (ring_v, y, flip) in [(0.0f32, -height * 0.5, true), (1.0, height * 0.5, false)] {
        let centre = positions.len() as u32;
        positions.push([0.0, y, 0.0]);
        normals.push([0.0, if flip { -1.0 } else { 1.0 }, 0.0]);
        uvs.push([0.5, ring_v]);
        let ring_start = if flip { 0 } else { rings as u32 * stride };
        for s in 0..sectors as u32 {
            let a = ring_start + s;
            let b = ring_start + s + 1;
            if flip {
                indices.extend_from_slice(&[centre, b, a]);
            } else {
                indices.extend_from_slice(&[centre, a, b]);
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

fn ovoid(radii: Vec3, exponent: f32) -> Mesh {
    superellipsoid_mesh(radii, exponent, exponent, 18, 24)
}

// ── Proportions (patch → measurements) ────────────────────────────────────────

struct Proportions {
    h: f32,
    hip_y: f32,
    waist_y: f32,
    chest_y: f32,
    shoulder_y: f32,
    neck_top_y: f32,
    head_c_y: f32,
    knee_y: f32,
    ankle_y: f32,
    shoulder_hw: f32,
    waist_hw: f32,
    hip_hw: f32,
    chest_depth: f32,
    limb: f32,
    muscle: f32,
    weight: f32,
    chest_shape: f32,
    head_r: f32,
    female: bool,
}

fn m(patch: &CharacterPatch, name: &str) -> f32 {
    patch.morph(name)
}

/// Map a 0..1 morph to `centre ± span`.
fn spread(v: f32, centre: f32, span: f32) -> f32 {
    centre + (v - 0.5) * 2.0 * span
}

impl Proportions {
    fn from_patch(patch: &CharacterPatch) -> Self {
        let female = patch.sex == Sex::Female;
        let h =
            1.75 * spread(m(patch, "body_height"), 1.0, 0.09) * if female { 0.945 } else { 1.0 };
        let muscle = m(patch, "body_muscle");
        let weight = m(patch, "body_weight");
        let chest_shape = m(patch, "body_chest_shape");
        let limb = spread(m(patch, "limb_length"), 1.0, 0.07);

        let shoulder_base = if female { 0.108 } else { 0.125 };
        let hip_base = if female { 0.101 } else { 0.088 };
        let waist_base = if female { 0.066 } else { 0.076 };
        let bulk = 1.0 + (muscle - 0.5) * 0.22 + (weight - 0.5) * 0.18;

        Self {
            h,
            hip_y: 0.520 * h,
            waist_y: 0.615 * h,
            chest_y: 0.720 * h,
            shoulder_y: 0.815 * h,
            neck_top_y: 0.862 * h,
            head_c_y: 0.928 * h,
            knee_y: 0.285 * h,
            ankle_y: 0.042 * h,
            shoulder_hw: spread(m(patch, "shoulders_wide"), shoulder_base, 0.020) * h * bulk,
            waist_hw: spread(m(patch, "waist_width"), waist_base, 0.020)
                * h
                * (1.0 + (weight - 0.5) * 0.55),
            hip_hw: spread(m(patch, "hips_wide"), hip_base, 0.018)
                * h
                * (1.0 + (weight - 0.5) * 0.30),
            chest_depth: 0.062
                * h
                * bulk
                * spread(chest_shape, 1.0, 0.20)
                * if female { 0.96 } else { 1.0 },
            limb,
            muscle,
            weight,
            chest_shape,
            // Slightly larger head and shorter facial mass create the readable
            // heroic proportions common to hand-drawn 1980s anime casts.
            head_r: 0.0735 * h * if female { 0.98 } else { 1.0 },
            female,
        }
    }

    fn limb_radius(&self, base_frac: f32) -> f32 {
        base_frac
            * self.h
            * (1.0 + (self.muscle - 0.5) * 0.34 + (self.weight - 0.5) * 0.22)
            * if self.female { 0.90 } else { 1.0 }
    }
}

// ── Materials ─────────────────────────────────────────────────────────────────

struct StudioMats {
    skin: Handle<StandardMaterial>,
    eye_white: Handle<StandardMaterial>,
    iris: Handle<StandardMaterial>,
    hair: Handle<StandardMaterial>,
    brow: Handle<StandardMaterial>,
    primary: Handle<StandardMaterial>,
    secondary: Handle<StandardMaterial>,
    suit: Handle<StandardMaterial>,
    metal: Handle<StandardMaterial>,
    core: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    denim: Handle<StandardMaterial>,
    leather: Handle<StandardMaterial>,
    sole: Handle<StandardMaterial>,
    lip: Handle<StandardMaterial>,
    blush: Handle<StandardMaterial>,
    highlight: Handle<StandardMaterial>,
}

fn dim(color: Color, f: f32) -> Color {
    let c = color.to_srgba();
    Color::srgb(c.red * f, c.green * f, c.blue * f)
}

fn build_mats(materials: &mut Assets<StandardMaterial>, patch: &CharacterPatch) -> StudioMats {
    let skin_c = patch.material("skin");
    let hair_c = patch.material("hair");
    let primary_c = patch.material("primary");
    let secondary_c = patch.material("secondary");
    let matte = |materials: &mut Assets<StandardMaterial>, color: Color, rough: f32| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: rough,
            reflectance: 0.18,
            ..default()
        })
    };
    StudioMats {
        skin: materials.add(StandardMaterial {
            base_color: skin_c,
            perceptual_roughness: 0.62,
            reflectance: 0.22,
            clearcoat: 0.08,
            ..default()
        }),
        eye_white: materials.add(StandardMaterial {
            base_color: Color::srgb(0.96, 0.97, 1.0),
            perceptual_roughness: 0.18,
            reflectance: 0.42,
            clearcoat: 0.65,
            ..default()
        }),
        iris: materials.add(StandardMaterial {
            base_color: patch.material("eyes"),
            perceptual_roughness: 0.18,
            reflectance: 0.38,
            clearcoat: 0.5,
            ..default()
        }),
        hair: materials.add(StandardMaterial {
            base_color: hair_c,
            perceptual_roughness: 0.58,
            reflectance: 0.28,
            clearcoat: 0.16,
            ..default()
        }),
        brow: matte(materials, dim(hair_c, 0.65), 0.85),
        primary: matte(materials, primary_c, 0.85),
        secondary: matte(materials, secondary_c, 0.85),
        suit: materials.add(StandardMaterial {
            base_color: primary_c,
            perceptual_roughness: 0.45,
            reflectance: 0.35,
            clearcoat: 0.4,
            ..default()
        }),
        metal: materials.add(StandardMaterial {
            base_color: primary_c,
            perceptual_roughness: 0.35,
            metallic: 0.65,
            reflectance: 0.45,
            ..default()
        }),
        core: {
            let c = secondary_c.to_linear();
            materials.add(StandardMaterial {
                base_color: secondary_c,
                emissive: LinearRgba::new(c.red * 5.0, c.green * 5.0, c.blue * 5.0, 1.0),
                perceptual_roughness: 0.2,
                ..default()
            })
        },
        dark: matte(materials, Color::srgb(0.12, 0.12, 0.14), 0.9),
        denim: materials.add(StandardMaterial {
            base_color: dim(secondary_c, 0.72),
            perceptual_roughness: 0.96,
            reflectance: 0.08,
            ..default()
        }),
        leather: materials.add(StandardMaterial {
            base_color: dim(primary_c, 0.55),
            perceptual_roughness: 0.34,
            reflectance: 0.36,
            clearcoat: 0.3,
            ..default()
        }),
        sole: matte(materials, Color::srgb(0.055, 0.06, 0.075), 0.94),
        lip: materials.add(StandardMaterial {
            base_color: Color::srgb(0.58, 0.16, 0.20),
            perceptual_roughness: 0.48,
            reflectance: 0.24,
            ..default()
        }),
        blush: materials.add(StandardMaterial {
            base_color: Color::srgba(0.92, 0.34, 0.38, 0.68),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.8,
            ..default()
        }),
        highlight: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            emissive: LinearRgba::new(1.8, 1.8, 2.0, 1.0),
            perceptual_roughness: 0.08,
            ..default()
        }),
    }
}

// ── Spawner ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioBodyRegion {
    Torso,
    Head,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

/// Independently animated facial pieces. Keeping this semantic tag beside the
/// generated geometry lets gameplay animate procedural faces without relying
/// on fragile entity names or a particular future GLB hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioFaceFeature {
    LeftEye,
    RightEye,
    LeftBrow,
    RightBrow,
    Mouth,
}

#[derive(Component, Debug, Clone)]
pub struct StudioHumanPart {
    pub root: Entity,
    pub rest: Transform,
    pub pivot: Vec3,
    pub region: StudioBodyRegion,
    pub face_feature: Option<StudioFaceFeature>,
}

#[derive(Component, Debug, Clone)]
pub struct PlayableStudioHuman {
    pub owner: Entity,
    pub rest: Transform,
}

fn classify_studio_part(p: &Proportions, position: Vec3) -> (StudioBodyRegion, Vec3) {
    if position.y >= p.neck_top_y - p.h * 0.035 {
        return (StudioBodyRegion::Head, Vec3::new(0.0, p.neck_top_y, 0.0));
    }
    if position.x.abs() >= p.shoulder_hw * 0.72 && position.y > p.hip_y * 0.82 {
        let side = position.x.signum();
        return (
            if side < 0.0 {
                StudioBodyRegion::LeftArm
            } else {
                StudioBodyRegion::RightArm
            },
            Vec3::new(side * p.shoulder_hw, p.shoulder_y, 0.0),
        );
    }
    if position.x.abs() >= p.hip_hw * 0.28 && position.y < p.hip_y + p.h * 0.025 {
        let side = position.x.signum();
        return (
            if side < 0.0 {
                StudioBodyRegion::LeftLeg
            } else {
                StudioBodyRegion::RightLeg
            },
            Vec3::new(side * p.hip_hw * 0.55, p.hip_y, 0.0),
        );
    }
    (StudioBodyRegion::Torso, Vec3::new(0.0, p.hip_y, 0.0))
}

/// Generate and spawn the full human under a fresh root entity. Feet rest at
/// the root's origin. Returns the root.
pub fn spawn_human(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    patch: &CharacterPatch,
    root_transform: Transform,
) -> Entity {
    let p = Proportions::from_patch(patch);
    let mats = build_mats(materials, patch);

    let root = commands
        .spawn((
            root_transform,
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .id();

    let pending_face_feature = Cell::new(None);
    let mut part = |mesh: Mesh, mat: &Handle<StandardMaterial>, transform: Transform| {
        let mesh = if patch.modifiers.is_empty() {
            mesh
        } else {
            crate::mesh_modifiers::apply_stack_to_mesh(&mesh, &patch.modifiers).unwrap_or(mesh)
        };
        let (region, pivot) = classify_studio_part(&p, transform.translation);
        let e = commands
            .spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(mat.clone()),
                transform,
                StudioHumanPart {
                    root,
                    rest: transform,
                    pivot,
                    region,
                    face_feature: pending_face_feature.take(),
                },
            ))
            .id();
        commands.entity(root).add_child(e);
    };

    let mecha = matches!(
        patch.slots.get(&CharacterSlot::Torso),
        Some(SlotContent::Mecha)
    );
    let suit = matches!(
        patch.slots.get(&CharacterSlot::Torso),
        Some(SlotContent::Suit)
    );
    // Skin-tight layers recolor the body segments themselves.
    let body_mat = if suit {
        &mats.suit
    } else if mecha {
        &mats.secondary
    } else {
        &mats.skin
    };

    // ── Torso ────────────────────────────────────────────────────────────────
    let crotch_y = p.hip_y - 0.035 * p.h;
    part(
        lofted_column(
            Vec2::new(p.hip_hw * 0.96, p.chest_depth * 0.92),
            Vec2::new(p.hip_hw, p.chest_depth),
            p.hip_y - crotch_y + 0.04 * p.h,
            0.0,
            2.0,
            4,
            22,
        ),
        body_mat,
        Transform::from_xyz(0.0, (crotch_y + p.hip_y) * 0.5 + 0.02 * p.h, 0.0),
    );
    part(
        lofted_column(
            Vec2::new(p.hip_hw, p.chest_depth),
            Vec2::new(p.waist_hw, p.chest_depth * 0.88),
            p.waist_y - p.hip_y,
            0.0,
            2.0,
            5,
            22,
        ),
        body_mat,
        Transform::from_xyz(0.0, (p.hip_y + p.waist_y) * 0.5, 0.0),
    );
    // Belly (weight) sits over the waist seam.
    if p.weight > 0.62 {
        part(
            ovoid(
                Vec3::new(
                    p.waist_hw * 0.9,
                    (p.waist_y - p.hip_y) * 0.72,
                    p.chest_depth * (0.55 + (p.weight - 0.62) * 1.3),
                ),
                1.0,
            ),
            body_mat,
            Transform::from_xyz(0.0, (p.hip_y + p.waist_y) * 0.5, -p.chest_depth * 0.35),
        );
    }
    part(
        lofted_column(
            Vec2::new(p.waist_hw, p.chest_depth * 0.88),
            Vec2::new(p.shoulder_hw * 0.92, p.chest_depth),
            p.shoulder_y - p.waist_y,
            0.05 + p.muscle * 0.05,
            2.0,
            6,
            24,
        ),
        body_mat,
        Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5, 0.0),
    );
    // Sex-aware chest definition remains driven by an explicit neutral slider:
    // softer paired forms for the female base and broader pectoral planes for
    // the male base. Clothing uses the same chest depth, so outer layers keep
    // coverage as the shape changes.
    let chest_definition = spread(p.chest_shape, 1.0, 0.34);
    for side in [-1.0f32, 1.0] {
        let (width, height, depth, x, y) = if p.female {
            (
                p.shoulder_hw * 0.34,
                0.055 * p.h * chest_definition,
                p.chest_depth * 0.58 * chest_definition,
                p.shoulder_hw * 0.29,
                p.chest_y - 0.012 * p.h,
            )
        } else {
            (
                p.shoulder_hw * 0.40,
                0.045 * p.h * chest_definition,
                p.chest_depth * 0.34 * chest_definition,
                p.shoulder_hw * 0.31,
                p.chest_y + 0.006 * p.h,
            )
        };
        part(
            ovoid(
                Vec3::new(width, height, depth),
                if p.female { 1.15 } else { 1.45 },
            ),
            body_mat,
            Transform::from_xyz(side * x, y, -p.chest_depth * 0.72),
        );
    }
    // Trapezius wedge: fills the neck→shoulder gap so the silhouette flows.
    part(
        ovoid(
            Vec3::new(
                p.shoulder_hw * 0.72,
                0.030 * p.h * (1.0 + p.muscle * 0.5),
                p.chest_depth * 0.70,
            ),
            1.1,
        ),
        body_mat,
        Transform::from_xyz(0.0, p.shoulder_y + 0.008 * p.h, p.chest_depth * 0.08),
    );
    // Clavicles bridge the sternum to the deltoids and keep broad/slim morphs
    // looking anatomically connected instead of like arms attached to a tube.
    for side in [-1.0f32, 1.0] {
        part(
            ovoid(
                Vec3::new(p.shoulder_hw * 0.34, 0.010 * p.h, 0.012 * p.h),
                1.15,
            ),
            body_mat,
            Transform::from_xyz(
                side * p.shoulder_hw * 0.36,
                p.shoulder_y - 0.015 * p.h,
                -p.chest_depth * 0.82,
            )
            .with_rotation(Quat::from_rotation_z(-side * 0.10)),
        );
    }
    if p.female {
        for side in [-1.0f32, 1.0] {
            part(
                ovoid(
                    Vec3::new(0.052 * p.h * 0.62, 0.052 * p.h * 0.62, 0.052 * p.h * 0.55),
                    1.0,
                ),
                body_mat,
                Transform::from_xyz(
                    side * p.shoulder_hw * 0.38,
                    p.chest_y,
                    -p.chest_depth * 0.72,
                ),
            );
        }
    } else {
        // Pectoral plates give the male chest a front plane instead of a tube.
        for side in [-1.0f32, 1.0] {
            part(
                ovoid(
                    Vec3::new(
                        p.shoulder_hw * 0.42,
                        0.045 * p.h * (0.85 + p.muscle * 0.5),
                        p.chest_depth * 0.30,
                    ),
                    1.15,
                ),
                body_mat,
                Transform::from_xyz(
                    side * p.shoulder_hw * 0.40,
                    p.chest_y + 0.01 * p.h,
                    -p.chest_depth * 0.78,
                ),
            );
        }
    }
    // Glutes: the pelvis reads human from behind instead of a cut cylinder.
    for side in [-1.0f32, 1.0] {
        part(
            ovoid(
                Vec3::new(p.hip_hw * 0.52, 0.052 * p.h, p.chest_depth * 0.42),
                1.0,
            ),
            body_mat,
            Transform::from_xyz(
                side * p.hip_hw * 0.42,
                p.hip_y - 0.015 * p.h,
                p.chest_depth * 0.55,
            ),
        );
    }

    // Neck + head.
    part(
        lofted_column(
            Vec2::splat(p.head_r * 0.52),
            Vec2::splat(p.head_r * 0.48),
            (p.neck_top_y - p.shoulder_y) * 1.35,
            0.0,
            2.0,
            3,
            16,
        ),
        &mats.skin,
        Transform::from_xyz(0.0, (p.shoulder_y + p.neck_top_y) * 0.5, 0.0),
    );
    spawn_head(&mut part, &pending_face_feature, patch, &p, &mats);

    // ── Arms ─────────────────────────────────────────────────────────────────
    let upper_r = p.limb_radius(0.030);
    let fore_r = p.limb_radius(0.024);
    let arm_len = 0.165 * p.h * p.limb;
    let fore_len = 0.150 * p.h * p.limb;
    let elbow_y = p.shoulder_y - arm_len;
    let wrist_y = elbow_y - fore_len;
    for side in [-1.0f32, 1.0] {
        let x = side * (p.shoulder_hw + upper_r * 0.35);
        // Deltoid cap.
        part(
            ovoid(Vec3::splat(upper_r * (1.35 + p.muscle * 0.4)), 1.0),
            body_mat,
            Transform::from_xyz(x, p.shoulder_y - upper_r * 0.2, 0.0),
        );
        part(
            lofted_column(
                Vec2::splat(fore_r * 1.08),
                Vec2::splat(upper_r),
                arm_len,
                p.muscle * 0.16,
                2.0,
                5,
                16,
            ),
            body_mat,
            Transform::from_xyz(x, (p.shoulder_y + elbow_y) * 0.5, 0.0),
        );
        // Buster cannon replaces the right forearm+hand in mecha mode.
        if mecha && side > 0.0 {
            part(
                lofted_column(
                    Vec2::splat(fore_r * 2.6),
                    Vec2::splat(fore_r * 2.2),
                    fore_len * 1.15,
                    0.0,
                    1.4,
                    5,
                    18,
                ),
                &mats.metal,
                Transform::from_xyz(x, (elbow_y + wrist_y) * 0.5 - fore_len * 0.05, 0.0),
            );
            part(
                ovoid(Vec3::new(fore_r * 1.6, fore_r * 0.9, fore_r * 1.6), 0.8),
                &mats.core,
                Transform::from_xyz(x, wrist_y - fore_len * 0.12, 0.0),
            );
            continue;
        }
        part(
            lofted_column(
                Vec2::splat(fore_r * 0.82),
                Vec2::splat(fore_r * 1.15),
                fore_len,
                p.muscle * 0.10,
                2.0,
                5,
                16,
            ),
            body_mat,
            Transform::from_xyz(x, (elbow_y + wrist_y) * 0.5, 0.0),
        );
        part(
            ovoid(Vec3::new(fore_r * 1.12, fore_r * 0.92, fore_r * 1.05), 1.0),
            body_mat,
            Transform::from_xyz(x, elbow_y, fore_r * 0.12),
        );
        // Hand: gloved in suit, armored gauntlet in mecha, skin otherwise.
        let hand_content = patch.slots.get(&CharacterSlot::Hands).copied();
        let hand_mat = match hand_content {
            Some(SlotContent::Suit) => &mats.primary,
            Some(SlotContent::Mecha) => &mats.metal,
            _ => &mats.skin,
        };
        // Wrist: a short connector loft so the forearm flows into the palm
        // instead of butt-joining it.
        let wrist_len = 0.020 * p.h;
        part(
            lofted_column(
                Vec2::new(fore_r * 0.72, fore_r * 0.52),
                Vec2::splat(fore_r * 0.82),
                wrist_len * 1.4,
                0.0,
                1.8,
                3,
                12,
            ),
            hand_mat,
            Transform::from_xyz(x, wrist_y - wrist_len * 0.4, 0.0),
        );
        // Gauntlet cuff flares over the wrist seam.
        if matches!(hand_content, Some(SlotContent::Mecha)) {
            part(
                lofted_column(
                    Vec2::splat(fore_r * 1.5),
                    Vec2::splat(fore_r * 1.15),
                    fore_len * 0.4,
                    0.0,
                    1.4,
                    3,
                    14,
                ),
                &mats.metal,
                Transform::from_xyz(x, wrist_y + fore_len * 0.12, 0.0),
            );
        }
        let hand_scale = if mecha { 1.6 } else { 1.0 };
        spawn_hand(
            &mut part,
            &p,
            hand_mat,
            side,
            Vec3::new(x, wrist_y - wrist_len * 0.9, 0.0),
            hand_scale,
        );
    }

    // ── Legs ─────────────────────────────────────────────────────────────────
    let thigh_r = p.limb_radius(0.044);
    let shin_r = p.limb_radius(0.031);
    let leg_x = p.hip_hw * 0.52;
    let legs_mecha = matches!(
        patch.slots.get(&CharacterSlot::Legs),
        Some(SlotContent::Mecha)
    );
    for side in [-1.0f32, 1.0] {
        let x = side * leg_x;
        part(
            lofted_column(
                Vec2::splat(shin_r * 1.25),
                Vec2::new(thigh_r, thigh_r * 1.05),
                p.hip_y - p.knee_y,
                p.muscle * 0.10,
                2.0,
                6,
                18,
            ),
            body_mat,
            Transform::from_xyz(x, (p.hip_y + p.knee_y) * 0.5, 0.0),
        );
        part(
            lofted_column(
                Vec2::splat(shin_r * 0.62),
                Vec2::splat(shin_r * 1.12),
                p.knee_y - p.ankle_y,
                0.18 * (0.5 + p.muscle * 0.5),
                2.0,
                6,
                16,
            ),
            body_mat,
            Transform::from_xyz(x, (p.knee_y + p.ankle_y) * 0.5, 0.0),
        );
        // Knee cap and rear calf volume make the leg read as a jointed limb in
        // profile while remaining enclosed by generated trousers and boots.
        part(
            ovoid(Vec3::new(shin_r * 1.05, shin_r * 0.82, shin_r * 0.72), 1.0),
            body_mat,
            Transform::from_xyz(x, p.knee_y, -shin_r * 0.48),
        );
        part(
            ovoid(Vec3::new(shin_r * 0.82, 0.075 * p.h, shin_r * 0.68), 1.0),
            body_mat,
            Transform::from_xyz(x, p.ankle_y + (p.knee_y - p.ankle_y) * 0.48, shin_r * 0.42),
        );
        // Foot / footwear (wardrobe-aware; armor overrides).
        let armor = patch.wardrobe.armor;
        let feet_style = patch.wardrobe.feet;
        let (foot_mat, foot_scale) = match (armor, feet_style) {
            (ArmorStyle::MechaArmor | ArmorStyle::CrystalMecha | ArmorStyle::DragonMecha, _) => {
                (&mats.metal, 1.9)
            }
            (ArmorStyle::SuperSuit, _) => (&mats.secondary, 1.08),
            (_, FootStyle::Barefoot) => (&mats.skin, 1.0),
            (_, FootStyle::Shoes | FootStyle::Loafers) => (&mats.leather, 1.10),
            (_, FootStyle::Sneakers | FootStyle::HighTops) => (&mats.primary, 1.14),
            (
                _,
                FootStyle::Boots
                | FootStyle::TallBoots
                | FootStyle::CombatBoots
                | FootStyle::HeeledBoots,
            ) => (&mats.leather, 1.18),
        };
        part(
            ovoid(
                Vec3::new(
                    shin_r * 1.05 * foot_scale,
                    p.ankle_y * 0.72 * foot_scale.min(1.4),
                    0.075 * p.h * foot_scale,
                ),
                1.25,
            ),
            foot_mat,
            Transform::from_xyz(x, p.ankle_y * 0.55, -0.028 * p.h * foot_scale),
        );
        // Separate outsole gives every shoe a grounded silhouette and a
        // material break instead of looking like a recolored bare foot.
        if armor == ArmorStyle::None && feet_style != FootStyle::Barefoot {
            part(
                ovoid(
                    Vec3::new(
                        shin_r * 1.14 * foot_scale,
                        0.010 * p.h,
                        0.080 * p.h * foot_scale,
                    ),
                    1.1,
                ),
                &mats.sole,
                Transform::from_xyz(x, 0.012 * p.h, -0.030 * p.h * foot_scale),
            );
        }
        // Boot shafts.
        if armor == ArmorStyle::None {
            let shaft_top = match feet_style {
                FootStyle::HighTops => Some(p.ankle_y + (p.knee_y - p.ankle_y) * 0.16),
                FootStyle::Boots => Some(p.ankle_y + (p.knee_y - p.ankle_y) * 0.34),
                FootStyle::CombatBoots => Some(p.ankle_y + (p.knee_y - p.ankle_y) * 0.48),
                FootStyle::TallBoots | FootStyle::HeeledBoots => Some(p.knee_y * 0.96),
                _ => None,
            };
            if let Some(top) = shaft_top {
                let hshaft = top - p.ankle_y * 0.4;
                part(
                    lofted_column(
                        Vec2::splat(shin_r * 1.32),
                        Vec2::splat(shin_r * 1.22),
                        hshaft,
                        0.10,
                        1.9,
                        4,
                        14,
                    ),
                    if feet_style == FootStyle::HighTops {
                        &mats.primary
                    } else {
                        &mats.leather
                    },
                    Transform::from_xyz(x, p.ankle_y * 0.4 + hshaft * 0.5, 0.0),
                );
            }
            if matches!(feet_style, FootStyle::Sneakers | FootStyle::HighTops) {
                for lace in 0..3 {
                    part(
                        ovoid(Vec3::new(shin_r * 0.72, 0.004 * p.h, 0.006 * p.h), 1.0),
                        &mats.highlight,
                        Transform::from_xyz(
                            x,
                            p.ankle_y * (0.58 + lace as f32 * 0.12),
                            -0.075 * p.h,
                        ),
                    );
                }
            }
            if feet_style == FootStyle::CombatBoots {
                for buckle in 0..2 {
                    part(
                        ovoid(Vec3::new(shin_r * 1.38, 0.006 * p.h, 0.006 * p.h), 1.0),
                        &mats.secondary,
                        Transform::from_xyz(
                            x,
                            p.ankle_y + (buckle as f32 + 1.0) * 0.045 * p.h,
                            -shin_r * 1.12,
                        ),
                    );
                }
            }
            if feet_style == FootStyle::HeeledBoots {
                part(
                    ovoid(Vec3::new(0.014 * p.h, 0.035 * p.h, 0.014 * p.h), 1.2),
                    &mats.sole,
                    Transform::from_xyz(x, 0.030 * p.h, 0.030 * p.h),
                );
            }
        }
        if legs_mecha {
            // Oversized boot cuff.
            part(
                lofted_column(
                    Vec2::splat(shin_r * 2.1),
                    Vec2::splat(shin_r * 1.7),
                    (p.knee_y - p.ankle_y) * 0.62,
                    0.0,
                    1.4,
                    4,
                    16,
                ),
                &mats.metal,
                Transform::from_xyz(x, p.ankle_y + (p.knee_y - p.ankle_y) * 0.31, 0.0),
            );
        }
    }

    // ── Outfit overlays ──────────────────────────────────────────────────────
    if patch.wardrobe.armor == ArmorStyle::None {
        spawn_wardrobe(
            &mut part, patch, &p, &mats, upper_r, fore_r, arm_len, fore_len, thigh_r, leg_x,
        );
    }
    if suit {
        spawn_super_suit(&mut part, &p, &mats);
    }
    if mecha {
        spawn_mecha_armor(&mut part, &p, &mats, upper_r, patch.wardrobe.armor);
    }
    spawn_fantasy_flair(&mut part, &p, &mats, patch.flair);

    root
}

// ── Hand ──────────────────────────────────────────────────────────────────────

/// A real hand: palm slab + four fingers (individual lengths, slight relaxed
/// curl toward the body) + opposable thumb angled forward. Hands hang at the
/// sides with the palm facing the thigh (thin axis = X), fingers pointing
/// down. `scale > 1` produces the chunky mecha gauntlet.
fn spawn_hand(
    part: &mut impl FnMut(Mesh, &Handle<StandardMaterial>, Transform),
    p: &Proportions,
    mat: &Handle<StandardMaterial>,
    side: f32,
    wrist: Vec3,
    scale: f32,
) {
    // Human hand ≈ 0.105 · height; palm ~55 %, fingers ~45 %.
    let hand_len = 0.105 * p.h * scale * if p.female { 0.94 } else { 1.0 };
    let palm_len = hand_len * 0.55;
    let finger_len = hand_len * 0.45;
    let palm_w = hand_len * 0.42; // across (front-back, Z)
    let palm_t = hand_len * 0.14; // thickness (toward thigh, X)

    // Palm slab, top edge meeting the wrist.
    part(
        ovoid(Vec3::new(palm_t * 0.5, palm_len * 0.5, palm_w * 0.5), 1.35),
        mat,
        Transform::from_translation(wrist + Vec3::new(0.0, -palm_len * 0.45, 0.0)),
    );

    // Four fingers along the palm's bottom edge (front→back: index…pinky),
    // curled a touch toward the body for a relaxed pose.
    let finger_r = palm_w * 0.115 * if scale > 1.2 { 1.5 } else { 1.0 };
    let lengths = [0.94f32, 1.0, 0.94, 0.76];
    let knuckle_y = wrist.y - palm_len * 0.92;
    for (i, len_mul) in lengths.into_iter().enumerate() {
        let z = -palm_w * 0.5 + palm_w * (0.14 + 0.24 * i as f32);
        let len = finger_len * len_mul;
        let curl = Quat::from_rotation_z(-side * 0.16); // fingertips toward thigh
        part(
            lofted_column(
                Vec2::splat(finger_r * 0.82),
                Vec2::splat(finger_r),
                len,
                0.06,
                2.0,
                3,
                10,
            ),
            mat,
            Transform::from_xyz(wrist.x, knuckle_y - len * 0.5, wrist.z + z).with_rotation(curl),
        );
    }

    // Thumb: rises from the palm's front edge, angled forward and outward.
    let thumb_len = finger_len * 0.82;
    part(
        lofted_column(
            Vec2::splat(finger_r * 1.15),
            Vec2::splat(finger_r * 1.3),
            thumb_len,
            0.05,
            2.0,
            3,
            10,
        ),
        mat,
        Transform::from_translation(wrist + Vec3::new(0.0, -palm_len * 0.42, -palm_w * 0.62))
            .with_rotation(Quat::from_rotation_x(0.85) * Quat::from_rotation_z(-side * 0.20)),
    );
}

// ── Head + face ───────────────────────────────────────────────────────────────

fn spawn_head(
    part: &mut impl FnMut(Mesh, &Handle<StandardMaterial>, Transform),
    pending_face_feature: &Cell<Option<StudioFaceFeature>>,
    patch: &CharacterPatch,
    p: &Proportions,
    mats: &StudioMats,
) {
    let hr = p.head_r;
    let cheek = spread(m(patch, "face_cheek_full"), 1.0, 0.14);
    let face_length = spread(m(patch, "face_length"), 1.0, 0.16);
    let face_z = -hr * 0.86; // faces -Z
    let head_c = Vec3::new(0.0, p.head_c_y, 0.0);

    // Large cranium + tapered lower face: an animation-friendly 1980s anime
    // silhouette rather than a realistic scan or a round toy head.
    part(
        ovoid(Vec3::new(hr * 0.84 * cheek, hr * 1.04, hr * 0.88), 1.0),
        &mats.skin,
        Transform::from_translation(head_c),
    );
    // Jaw + chin — pulled up into the skull so the head reads as one form
    // (retro-RPG: soft rounded face, no bolted-on lower box).
    let jaw_w = spread(m(patch, "face_jaw_wide"), 0.61, 0.13);
    let chin = spread(m(patch, "face_chin_long"), 1.0, 0.30);
    let chin_w = spread(m(patch, "face_chin_wide"), 1.0, 0.42);
    part(
        ovoid(
            Vec3::new(hr * jaw_w * cheek, hr * 0.46 * chin, hr * 0.68),
            1.0,
        ),
        &mats.skin,
        Transform::from_translation(head_c + Vec3::new(0.0, -hr * 0.50 * face_length, -hr * 0.06)),
    );
    // Broad cheek planes bridge the cranium into the lower face. Their shallow
    // superellipse profile keeps a readable cel highlight from front and
    // three-quarter views without forming separate spherical "cheek balls".
    for side in [-1.0f32, 1.0] {
        part(
            ovoid(Vec3::new(hr * 0.29 * cheek, hr * 0.30, hr * 0.11), 1.45),
            &mats.skin,
            Transform::from_translation(
                head_c
                    + Vec3::new(
                        side * hr * 0.39,
                        -hr * 0.20 * face_length,
                        face_z + hr * 0.13,
                    ),
            )
            .with_rotation(Quat::from_rotation_z(-side * 0.18)),
        );
    }
    // Chin cap.
    part(
        ovoid(
            Vec3::new(
                hr * 0.18 * jaw_w.max(0.4) * chin_w,
                hr * 0.13 * chin,
                hr * 0.14,
            ),
            1.0,
        ),
        &mats.skin,
        Transform::from_translation(
            head_c + Vec3::new(0.0, -hr * (0.78 + 0.10 * chin) * face_length, -hr * 0.42),
        ),
    );
    // Minimal cel-animation nose: readable in profile without dominating the
    // front view.
    let nose_len = spread(m(patch, "face_nose_long"), 1.0, 0.4);
    let nose_w = spread(m(patch, "face_nose_wide"), 1.0, 0.4);
    let nose_bridge = spread(m(patch, "face_nose_bridge"), 1.0, 0.5);
    let nose_tip = spread(m(patch, "face_nose_tip"), 1.0, 0.45);
    part(
        ovoid(
            Vec3::new(
                hr * 0.038 * nose_w,
                hr * 0.18 * nose_bridge,
                hr * 0.032 * nose_bridge,
            ),
            1.45,
        ),
        &mats.skin,
        Transform::from_translation(
            head_c + Vec3::new(0.0, hr * 0.01, face_z - hr * 0.005 * nose_bridge),
        ),
    );
    part(
        ovoid(
            Vec3::new(
                hr * 0.060 * nose_w * nose_tip,
                hr * 0.085 * nose_len,
                hr * 0.070 * nose_tip,
            ),
            1.25,
        ),
        &mats.skin,
        Transform::from_translation(
            head_c
                + Vec3::new(
                    0.0,
                    -hr * 0.18 * face_length,
                    face_z - hr * 0.040 * nose_tip,
                ),
        ),
    );
    // Twin eyebrows (angled) — the old single bar read as a visor.
    let brow = spread(m(patch, "face_brow_heavy"), 1.0, 0.5);
    let brow_angle = (m(patch, "face_brow_angle") - 0.5) * 0.42;
    for side in [-1.0f32, 1.0] {
        pending_face_feature.set(Some(if side < 0.0 {
            StudioFaceFeature::LeftBrow
        } else {
            StudioFaceFeature::RightBrow
        }));
        part(
            ovoid(Vec3::new(hr * 0.17, hr * 0.038 * brow, hr * 0.045), 1.1),
            &mats.brow,
            Transform::from_translation(
                head_c + Vec3::new(side * hr * 0.30, hr * 0.235, face_z + hr * 0.035),
            )
            .with_rotation(Quat::from_rotation_z(side * (0.10 + brow_angle))),
        );
    }
    // Wide cel-anime eyes with a strong upper lash, large colored iris, dark
    // pupil, and twin catchlights. The layered depth remains legible under the
    // studio's moving light instead of depending on a painted texture.
    let eye = spread(m(patch, "face_eye_large"), 1.18, 0.30);
    let eye_shape = m(patch, "face_eye_shape");
    let eye_spacing = spread(m(patch, "face_eye_spacing"), 1.0, 0.22);
    let eye_tilt = (m(patch, "face_eye_tilt") - 0.5) * 0.30;
    let eye_depth = spread(m(patch, "face_eye_depth"), 1.0, 0.30);
    for side in [-1.0f32, 1.0] {
        let feature = if side < 0.0 {
            StudioFaceFeature::LeftEye
        } else {
            StudioFaceFeature::RightEye
        };
        let eye_pos = head_c
            + Vec3::new(
                side * hr * 0.29 * eye_spacing,
                hr * 0.05,
                face_z + hr * (0.05 + (eye_depth - 1.0) * 0.12),
            );
        pending_face_feature.set(Some(feature));
        part(
            anime_eye_mesh(
                Vec2::new(hr * 0.190 * eye, hr * 0.124 * eye),
                hr * 0.050,
                eye_shape,
                eye_tilt,
                side,
            ),
            &mats.eye_white,
            Transform::from_translation(eye_pos),
        );
        pending_face_feature.set(Some(feature));
        part(
            ovoid(
                Vec3::new(
                    hr * 0.087 * eye * eye_depth,
                    hr * 0.098 * eye * eye_depth,
                    hr * 0.028,
                ),
                1.0,
            ),
            &mats.iris,
            Transform::from_translation(eye_pos + Vec3::new(0.0, 0.0, -hr * 0.038))
                .with_rotation(Quat::from_rotation_z(-side * eye_tilt)),
        );
        pending_face_feature.set(Some(feature));
        part(
            ovoid(Vec3::splat(hr * 0.034 * eye), 1.0),
            &mats.dark,
            Transform::from_translation(eye_pos + Vec3::new(0.0, 0.0, -hr * 0.058)),
        );
        pending_face_feature.set(Some(feature));
        part(
            ovoid(Vec3::new(hr * 0.205 * eye, hr * 0.026, hr * 0.026), 1.0),
            &mats.brow,
            Transform::from_translation(eye_pos + Vec3::new(0.0, hr * 0.115 * eye, -hr * 0.060))
                .with_rotation(Quat::from_rotation_z(-side * (0.07 + eye_tilt))),
        );
        // Skin-toned lower lid and a short outer lash visually seat the eye in
        // the face instead of leaving a decal-like white oval.
        part(
            ovoid(Vec3::new(hr * 0.155 * eye, hr * 0.014, hr * 0.020), 1.3),
            &mats.skin,
            Transform::from_translation(eye_pos + Vec3::new(0.0, -hr * 0.112 * eye, -hr * 0.054))
                .with_rotation(Quat::from_rotation_z(-side * eye_tilt)),
        );
        part(
            ovoid(Vec3::new(hr * 0.060 * eye, hr * 0.015, hr * 0.018), 1.2),
            &mats.brow,
            Transform::from_translation(
                eye_pos
                    + Vec3::new(
                        side * hr * 0.175 * eye,
                        hr * (0.095 + eye_tilt * 0.2) * eye,
                        -hr * 0.061,
                    ),
            )
            .with_rotation(Quat::from_rotation_z(-side * (0.26 + eye_tilt))),
        );
        pending_face_feature.set(Some(feature));
        part(
            ovoid(Vec3::splat(hr * 0.025 * eye), 1.0),
            &mats.highlight,
            Transform::from_translation(
                eye_pos + Vec3::new(-side * hr * 0.035, hr * 0.035, -hr * 0.090),
            ),
        );
        pending_face_feature.set(Some(feature));
        part(
            ovoid(Vec3::splat(hr * 0.010 * eye), 1.0),
            &mats.highlight,
            Transform::from_translation(
                eye_pos + Vec3::new(side * hr * 0.030, -hr * 0.030, -hr * 0.090),
            ),
        );
        // Soft cheek mark supplies the classic warm cel-animation face read.
        part(
            ovoid(Vec3::new(hr * 0.12, hr * 0.035, hr * 0.018), 1.0),
            &mats.blush,
            Transform::from_translation(
                head_c + Vec3::new(side * hr * 0.42, -hr * 0.23, face_z - hr * 0.015),
            ),
        );
    }
    // Mouth: separate oral cavity and upper/lower lips provide enough geometry
    // for readable smiles, speech, and combat shouts.
    let mouth = spread(m(patch, "face_mouth_wide"), 1.0, 0.4);
    let lip = spread(m(patch, "face_lip_full"), 1.0, 0.65);
    let mouth_pos = head_c + Vec3::new(0.0, -hr * 0.52 * face_length, face_z - hr * 0.02);
    pending_face_feature.set(Some(StudioFaceFeature::Mouth));
    part(
        ovoid(Vec3::new(hr * 0.220 * mouth, hr * 0.025, hr * 0.028), 1.25),
        &mats.dark,
        Transform::from_translation(mouth_pos + Vec3::new(0.0, 0.0, hr * 0.012)),
    );
    pending_face_feature.set(Some(StudioFaceFeature::Mouth));
    part(
        ovoid(
            Vec3::new(hr * 0.21 * mouth, hr * 0.018 * lip, hr * 0.028),
            1.25,
        ),
        &mats.lip,
        Transform::from_translation(mouth_pos + Vec3::new(0.0, hr * 0.027 * lip, -hr * 0.01)),
    );
    pending_face_feature.set(Some(StudioFaceFeature::Mouth));
    part(
        ovoid(
            Vec3::new(hr * 0.19 * mouth, hr * 0.026 * lip, hr * 0.030),
            1.25,
        ),
        &mats.lip,
        Transform::from_translation(mouth_pos + Vec3::new(0.0, -hr * 0.030 * lip, -hr * 0.012)),
    );
    // Ears — small and tucked.
    for side in [-1.0f32, 1.0] {
        part(
            ovoid(Vec3::new(hr * 0.055, hr * 0.115, hr * 0.075), 1.0),
            &mats.skin,
            Transform::from_translation(
                head_c + Vec3::new(side * hr * 0.80 * cheek, -hr * 0.02, hr * 0.04),
            ),
        );
    }

    // Hair.
    match patch.hair {
        HairStyle::Bald => {}
        HairStyle::Buzz => part(
            ovoid(Vec3::new(hr * 0.86 * cheek, hr * 0.92, hr * 0.90), 1.0),
            &mats.hair,
            Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.16, hr * 0.05)),
        ),
        HairStyle::Short => {
            part(
                ovoid(Vec3::new(hr * 0.90 * cheek, hr * 0.72, hr * 0.94), 1.0),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.42, hr * 0.08)),
            );
        }
        HairStyle::Long => {
            part(
                ovoid(Vec3::new(hr * 0.92 * cheek, hr * 0.70, hr * 0.96), 1.0),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.44, hr * 0.10)),
            );
            part(
                lofted_column(
                    Vec2::new(hr * 0.55, hr * 0.30),
                    Vec2::new(hr * 0.80, hr * 0.42),
                    hr * 2.3,
                    0.0,
                    1.6,
                    4,
                    14,
                ),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(0.0, -hr * 0.55, hr * 0.72)),
            );
        }
        HairStyle::Ponytail => {
            part(
                ovoid(Vec3::new(hr * 0.90 * cheek, hr * 0.72, hr * 0.94), 1.0),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.42, hr * 0.08)),
            );
            part(
                lofted_column(
                    Vec2::splat(hr * 0.16),
                    Vec2::splat(hr * 0.30),
                    hr * 1.7,
                    0.0,
                    1.8,
                    4,
                    12,
                ),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(0.0, -hr * 0.1, hr * 0.95))
                    .with_rotation(Quat::from_rotation_x(0.35)),
            );
        }
        HairStyle::Feathered => {
            part(
                ovoid(Vec3::new(hr * 0.96 * cheek, hr * 0.76, hr), 1.0),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.40, hr * 0.08)),
            );
            for side in [-1.0f32, 1.0] {
                for layer in 0..3 {
                    part(
                        ovoid(Vec3::new(hr * 0.24, hr * 0.48, hr * 0.18), 1.0),
                        &mats.hair,
                        Transform::from_translation(
                            head_c
                                + Vec3::new(
                                    side * hr * (0.72 + layer as f32 * 0.10),
                                    hr * (0.35 - layer as f32 * 0.22),
                                    hr * 0.14,
                                ),
                        )
                        .with_rotation(Quat::from_rotation_z(side * (0.35 + layer as f32 * 0.1))),
                    );
                }
            }
        }
        HairStyle::Spiky => {
            part(
                ovoid(Vec3::new(hr * 0.90 * cheek, hr * 0.70, hr * 0.94), 1.0),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.38, hr * 0.10)),
            );
            for spike in 0..7 {
                let x = (spike as f32 - 3.0) * hr * 0.22;
                let lean = (spike as f32 - 3.0) * 0.07;
                part(
                    ovoid(Vec3::new(hr * 0.13, hr * 0.48, hr * 0.14), 0.72),
                    &mats.hair,
                    Transform::from_translation(
                        head_c + Vec3::new(x, hr * (0.88 + 0.08 * (3.0 - x.abs() / hr)), hr * 0.08),
                    )
                    .with_rotation(Quat::from_rotation_z(-lean)),
                );
            }
        }
        HairStyle::Bob => {
            part(
                ovoid(Vec3::new(hr * 0.96 * cheek, hr * 0.84, hr), 1.0),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.26, hr * 0.10)),
            );
            for side in [-1.0f32, 1.0] {
                part(
                    lofted_column(
                        Vec2::new(hr * 0.17, hr * 0.22),
                        Vec2::new(hr * 0.25, hr * 0.28),
                        hr * 1.2,
                        0.05,
                        1.6,
                        4,
                        12,
                    ),
                    &mats.hair,
                    Transform::from_translation(
                        head_c + Vec3::new(side * hr * 0.76, -hr * 0.18, hr * 0.04),
                    ),
                );
            }
        }
        HairStyle::SidePonytail => {
            part(
                ovoid(Vec3::new(hr * 0.92 * cheek, hr * 0.73, hr * 0.96), 1.0),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.40, hr * 0.08)),
            );
            part(
                lofted_column(
                    Vec2::splat(hr * 0.14),
                    Vec2::splat(hr * 0.27),
                    hr * 1.8,
                    0.04,
                    1.7,
                    5,
                    12,
                ),
                &mats.hair,
                Transform::from_translation(head_c + Vec3::new(hr * 0.82, -hr * 0.16, hr * 0.38))
                    .with_rotation(Quat::from_rotation_z(-0.22)),
            );
        }
    }
    spawn_anime_bangs(part, patch.hair, head_c, hr, face_z, cheek, mats);
}

fn spawn_anime_bangs(
    part: &mut impl FnMut(Mesh, &Handle<StandardMaterial>, Transform),
    style: HairStyle,
    head_c: Vec3,
    hr: f32,
    face_z: f32,
    cheek: f32,
    mats: &StudioMats,
) {
    if matches!(style, HairStyle::Bald | HairStyle::Buzz) {
        return;
    }

    let (count, length, sweep) = match style {
        HairStyle::Short => (4, 0.42, 0.08),
        HairStyle::Long | HairStyle::Bob => (5, 0.58, -0.04),
        HairStyle::Ponytail => (4, 0.50, 0.06),
        HairStyle::Feathered => (5, 0.46, 0.22),
        HairStyle::Spiky => (4, 0.38, -0.12),
        HairStyle::SidePonytail => (5, 0.52, 0.16),
        HairStyle::Bald | HairStyle::Buzz => unreachable!(),
    };
    for strand in 0..count {
        let t = if count == 1 {
            0.0
        } else {
            strand as f32 / (count - 1) as f32 * 2.0 - 1.0
        };
        let strand_len = hr * length * (1.0 - t.abs() * 0.18);
        let x = hr * t * 0.50 * cheek;
        part(
            lofted_column(
                Vec2::new(hr * 0.055, hr * 0.035),
                Vec2::new(hr * 0.13, hr * 0.055),
                strand_len,
                0.0,
                1.45,
                3,
                10,
            ),
            &mats.hair,
            Transform::from_translation(
                head_c + Vec3::new(x, hr * 0.57 - strand_len * 0.5, face_z + hr * 0.08),
            )
            .with_rotation(Quat::from_rotation_z(-t * 0.18 - sweep)),
        );
    }
}

// ── Outfit layers ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn spawn_wardrobe(
    part: &mut impl FnMut(Mesh, &Handle<StandardMaterial>, Transform),
    patch: &CharacterPatch,
    p: &Proportions,
    mats: &StudioMats,
    upper_r: f32,
    fore_r: f32,
    arm_len: f32,
    fore_len: f32,
    thigh_r: f32,
    leg_x: f32,
) {
    let w = patch.wardrobe;
    let arm_x = p.shoulder_hw + upper_r * 0.35;
    let elbow_y = p.shoulder_y - arm_len;
    let wrist_y = elbow_y - fore_len;

    // ── Tops ─────────────────────────────────────────────────────────────────
    let torso_overlay = |hem_y: f32, flare: f32| {
        lofted_column(
            Vec2::new(p.waist_hw * 1.14 * flare, p.chest_depth * 1.05 * flare),
            Vec2::new(p.shoulder_hw * 1.02, p.chest_depth * 1.12),
            (p.shoulder_y - hem_y) * 1.02,
            0.03,
            2.0,
            5,
            22,
        )
    };
    let short_sleeves = |part: &mut dyn FnMut(Mesh, &Handle<StandardMaterial>, Transform),
                         mat: &Handle<StandardMaterial>| {
        for side in [-1.0f32, 1.0] {
            part(
                lofted_column(
                    Vec2::splat(upper_r * 1.30),
                    Vec2::splat(upper_r * 1.55),
                    arm_len * 0.55,
                    0.0,
                    2.0,
                    3,
                    14,
                ),
                mat,
                Transform::from_xyz(side * arm_x, p.shoulder_y - arm_len * 0.28, 0.0),
            );
        }
    };
    let long_sleeves = |part: &mut dyn FnMut(Mesh, &Handle<StandardMaterial>, Transform),
                        mat: &Handle<StandardMaterial>| {
        for side in [-1.0f32, 1.0] {
            part(
                lofted_column(
                    Vec2::splat(fore_r * 1.28),
                    Vec2::splat(upper_r * 1.45),
                    p.shoulder_y - wrist_y - 0.01 * p.h,
                    0.02,
                    2.0,
                    4,
                    14,
                ),
                mat,
                Transform::from_xyz(side * arm_x, (p.shoulder_y + wrist_y) * 0.5, 0.0),
            );
        }
    };

    match w.top {
        TopStyle::None => {}
        TopStyle::TShirt => {
            part(
                torso_overlay(p.waist_y, 1.0),
                &mats.primary,
                Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5, 0.0),
            );
            short_sleeves(part, &mats.primary);
        }
        TopStyle::LongShirt => {
            part(
                torso_overlay(p.waist_y, 1.0),
                &mats.primary,
                Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5, 0.0),
            );
            long_sleeves(part, &mats.primary);
        }
        TopStyle::Tunic => {
            let hem = p.hip_y - 0.04 * p.h;
            part(
                torso_overlay(hem, 1.08),
                &mats.primary,
                Transform::from_xyz(0.0, (hem + p.shoulder_y) * 0.5, 0.0),
            );
            short_sleeves(part, &mats.primary);
            // Belt over the tunic.
            part(
                lofted_column(
                    Vec2::new(p.waist_hw * 1.20, p.chest_depth * 1.10),
                    Vec2::new(p.waist_hw * 1.20, p.chest_depth * 1.10),
                    0.026 * p.h,
                    0.0,
                    1.7,
                    2,
                    18,
                ),
                &mats.dark,
                Transform::from_xyz(0.0, p.waist_y, 0.0),
            );
        }
        TopStyle::Jacket => {
            part(
                torso_overlay(p.waist_y - 0.02 * p.h, 1.06),
                &mats.denim,
                Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5, 0.0),
            );
            long_sleeves(part, &mats.denim);
            // Collar + open-front placket in the secondary colour.
            part(
                lofted_column(
                    Vec2::new(p.head_r * 0.78, p.head_r * 0.74),
                    Vec2::new(p.head_r * 0.92, p.head_r * 0.86),
                    0.030 * p.h,
                    0.0,
                    1.8,
                    2,
                    14,
                ),
                &mats.secondary,
                Transform::from_xyz(0.0, p.shoulder_y + 0.012 * p.h, 0.0),
            );
            part(
                ovoid(
                    Vec3::new(0.014 * p.h, (p.shoulder_y - p.waist_y) * 0.52, 0.008 * p.h),
                    1.4,
                ),
                &mats.secondary,
                Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5, -p.chest_depth * 1.14),
            );
        }
        TopStyle::Robe => {
            // Full-length robe: shoulders to shins, flared hem, full sleeves.
            let hem = p.knee_y * 0.55;
            part(
                lofted_column(
                    Vec2::new(p.hip_hw * 1.55, p.chest_depth * 1.55),
                    Vec2::new(p.shoulder_hw * 1.02, p.chest_depth * 1.14),
                    p.shoulder_y - hem,
                    0.02,
                    2.0,
                    6,
                    22,
                ),
                &mats.primary,
                Transform::from_xyz(0.0, (hem + p.shoulder_y) * 0.5, 0.0),
            );
            long_sleeves(part, &mats.primary);
            // Sash.
            part(
                lofted_column(
                    Vec2::new(p.waist_hw * 1.30, p.chest_depth * 1.24),
                    Vec2::new(p.waist_hw * 1.30, p.chest_depth * 1.24),
                    0.024 * p.h,
                    0.0,
                    1.7,
                    2,
                    18,
                ),
                &mats.secondary,
                Transform::from_xyz(0.0, p.waist_y, 0.0),
            );
        }
        TopStyle::TankTop => {
            part(
                torso_overlay(p.waist_y, 0.98),
                &mats.primary,
                Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5, 0.0),
            );
            for side in [-1.0f32, 1.0] {
                part(
                    ovoid(Vec3::new(0.018 * p.h, 0.080 * p.h, 0.010 * p.h), 1.2),
                    &mats.secondary,
                    Transform::from_xyz(
                        side * p.shoulder_hw * 0.62,
                        p.shoulder_y - 0.025 * p.h,
                        -p.chest_depth * 1.13,
                    ),
                );
            }
        }
        TopStyle::BomberJacket => {
            let hem = p.waist_y + 0.015 * p.h;
            part(
                torso_overlay(hem, 1.13),
                &mats.primary,
                Transform::from_xyz(0.0, (hem + p.shoulder_y) * 0.5, 0.0),
            );
            long_sleeves(part, &mats.primary);
            for y in [hem, p.shoulder_y + 0.005 * p.h] {
                part(
                    lofted_column(
                        Vec2::new(p.waist_hw * 1.27, p.chest_depth * 1.18),
                        Vec2::new(p.waist_hw * 1.27, p.chest_depth * 1.18),
                        0.025 * p.h,
                        0.0,
                        1.6,
                        2,
                        18,
                    ),
                    &mats.secondary,
                    Transform::from_xyz(0.0, y, 0.0),
                );
            }
        }
        TopStyle::MotoJacket => {
            part(
                torso_overlay(p.waist_y - 0.01 * p.h, 1.08),
                &mats.leather,
                Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5, 0.0),
            );
            long_sleeves(part, &mats.leather);
            part(
                ovoid(
                    Vec3::new(0.009 * p.h, (p.shoulder_y - p.waist_y) * 0.48, 0.007 * p.h),
                    1.2,
                ),
                &mats.secondary,
                Transform::from_xyz(
                    0.018 * p.h,
                    (p.waist_y + p.shoulder_y) * 0.5,
                    -p.chest_depth * 1.15,
                )
                .with_rotation(Quat::from_rotation_z(-0.22)),
            );
        }
        TopStyle::Blazer => {
            part(
                torso_overlay(p.waist_y - 0.025 * p.h, 1.05),
                &mats.primary,
                Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5, 0.0),
            );
            long_sleeves(part, &mats.primary);
            for side in [-1.0f32, 1.0] {
                part(
                    ovoid(Vec3::new(0.034 * p.h, 0.105 * p.h, 0.009 * p.h), 1.25),
                    &mats.secondary,
                    Transform::from_xyz(
                        side * 0.032 * p.h,
                        p.chest_y + 0.015 * p.h,
                        -p.chest_depth * 1.15,
                    )
                    .with_rotation(Quat::from_rotation_z(side * 0.28)),
                );
            }
            for button in 0..2 {
                part(
                    ovoid(Vec3::splat(0.008 * p.h), 1.0),
                    &mats.highlight,
                    Transform::from_xyz(
                        0.0,
                        p.waist_y + (button as f32 + 1.0) * 0.045 * p.h,
                        -p.chest_depth * 1.19,
                    ),
                );
            }
        }
        TopStyle::ArcaneCoat => {
            let hem = p.hip_y - 0.10 * p.h;
            part(
                torso_overlay(hem, 1.12),
                &mats.primary,
                Transform::from_xyz(0.0, (hem + p.shoulder_y) * 0.5, 0.0),
            );
            long_sleeves(part, &mats.primary);
            // High collar, luminous clasp, and split spell-coat tails.
            part(
                lofted_column(
                    Vec2::new(p.head_r * 0.84, p.head_r * 0.78),
                    Vec2::new(p.head_r * 0.96, p.head_r * 0.88),
                    0.045 * p.h,
                    0.0,
                    1.5,
                    3,
                    16,
                ),
                &mats.secondary,
                Transform::from_xyz(0.0, p.shoulder_y + 0.015 * p.h, 0.0),
            );
            for side in [-1.0f32, 1.0] {
                part(
                    ovoid(Vec3::new(0.075 * p.h, 0.19 * p.h, 0.018 * p.h), 1.25),
                    &mats.primary,
                    Transform::from_xyz(
                        side * 0.055 * p.h,
                        p.hip_y - 0.08 * p.h,
                        p.chest_depth * 1.12,
                    )
                    .with_rotation(Quat::from_rotation_z(-side * 0.12)),
                );
            }
            part(
                ovoid(Vec3::new(0.026 * p.h, 0.032 * p.h, 0.014 * p.h), 0.65),
                &mats.core,
                Transform::from_xyz(0.0, p.chest_y + 0.025 * p.h, -p.chest_depth * 1.22),
            );
        }
        TopStyle::StarKnightTunic => {
            let hem = p.hip_y - 0.03 * p.h;
            part(
                torso_overlay(hem, 1.06),
                &mats.primary,
                Transform::from_xyz(0.0, (hem + p.shoulder_y) * 0.5, 0.0),
            );
            short_sleeves(part, &mats.primary);
            // Layered shoulder guards and a diagonal heraldic sash.
            for side in [-1.0f32, 1.0] {
                part(
                    ovoid(Vec3::new(0.050 * p.h, 0.026 * p.h, 0.050 * p.h), 0.8),
                    &mats.metal,
                    Transform::from_xyz(
                        side * (p.shoulder_hw + 0.010 * p.h),
                        p.shoulder_y + 0.010 * p.h,
                        0.0,
                    ),
                );
            }
            part(
                ovoid(Vec3::new(0.018 * p.h, 0.15 * p.h, 0.010 * p.h), 1.15),
                &mats.secondary,
                Transform::from_xyz(0.0, p.chest_y - 0.025 * p.h, -p.chest_depth * 1.20)
                    .with_rotation(Quat::from_rotation_z(-0.48)),
            );
            part(
                ovoid(Vec3::new(0.030 * p.h, 0.034 * p.h, 0.014 * p.h), 0.62),
                &mats.core,
                Transform::from_xyz(0.0, p.chest_y + 0.018 * p.h, -p.chest_depth * 1.24),
            );
        }
    }

    // ── Bottoms ──────────────────────────────────────────────────────────────
    let hip_overlay = || {
        lofted_column(
            Vec2::new(p.hip_hw * 1.10, p.chest_depth * 1.02),
            Vec2::new(p.waist_hw * 1.12, p.chest_depth * 0.96),
            (p.waist_y - p.hip_y) * 1.15,
            0.0,
            2.0,
            3,
            18,
        )
    };
    let leg_sleeve = |bottom_y: f32, taper: f32| {
        lofted_column(
            Vec2::splat(thigh_r * taper),
            Vec2::new(thigh_r * 1.12, thigh_r * 1.16),
            p.hip_y - bottom_y,
            0.0,
            2.0,
            4,
            16,
        )
    };

    // Coverage-safe base layer. It is slightly smaller than every outer
    // garment, but fully encloses the pelvis and upper-thigh seam. This makes
    // Underwear a real garment and guarantees pants/skirts never reveal bare
    // glute geometry through gaps while the model turns or animates.
    let coverage_bottom = p.hip_y - 0.075 * p.h;
    part(
        lofted_column(
            Vec2::new(p.hip_hw * 1.035, p.chest_depth * 1.04),
            Vec2::new(p.waist_hw * 1.045, p.chest_depth * 0.98),
            p.waist_y - coverage_bottom,
            0.0,
            1.9,
            4,
            20,
        ),
        &mats.dark,
        Transform::from_xyz(0.0, (coverage_bottom + p.waist_y) * 0.5, 0.0),
    );
    for side in [-1.0f32, 1.0] {
        part(
            lofted_column(
                Vec2::splat(thigh_r * 1.015),
                Vec2::splat(thigh_r * 1.07),
                0.085 * p.h,
                0.0,
                1.9,
                3,
                16,
            ),
            &mats.dark,
            Transform::from_xyz(side * leg_x, p.hip_y - 0.045 * p.h, 0.0),
        );
    }
    match w.bottom {
        BottomStyle::Underwear => {}
        BottomStyle::Shorts => {
            part(
                hip_overlay(),
                &mats.secondary,
                Transform::from_xyz(0.0, (p.hip_y + p.waist_y) * 0.5, 0.0),
            );
            for side in [-1.0f32, 1.0] {
                let bottom = p.hip_y - (p.hip_y - p.knee_y) * 0.42;
                part(
                    leg_sleeve(bottom, 1.02),
                    &mats.secondary,
                    Transform::from_xyz(side * leg_x, (p.hip_y + bottom) * 0.5, 0.0),
                );
            }
        }
        BottomStyle::Pants => {
            part(
                hip_overlay(),
                &mats.secondary,
                Transform::from_xyz(0.0, (p.hip_y + p.knee_y * 0.82) * 0.5 + 0.05 * p.h, 0.0),
            );
            for side in [-1.0f32, 1.0] {
                let bottom = p.knee_y * 0.55;
                part(
                    leg_sleeve(bottom, 0.82),
                    &mats.secondary,
                    Transform::from_xyz(side * leg_x, (p.hip_y + bottom) * 0.5, 0.0),
                );
            }
        }
        BottomStyle::Skirt => {
            part(
                hip_overlay(),
                &mats.secondary,
                Transform::from_xyz(0.0, (p.hip_y + p.waist_y) * 0.5, 0.0),
            );
            let hem = p.knee_y * 1.05;
            part(
                lofted_column(
                    Vec2::new(p.hip_hw * 1.65, p.chest_depth * 1.75),
                    Vec2::new(p.hip_hw * 1.06, p.chest_depth * 1.00),
                    p.hip_y - hem,
                    0.0,
                    2.0,
                    4,
                    20,
                ),
                &mats.secondary,
                Transform::from_xyz(0.0, (hem + p.hip_y) * 0.5, 0.0),
            );
        }
        BottomStyle::Jeans | BottomStyle::CargoPants => {
            let mat = if w.bottom == BottomStyle::Jeans {
                &mats.denim
            } else {
                &mats.secondary
            };
            part(
                hip_overlay(),
                mat,
                Transform::from_xyz(0.0, (p.hip_y + p.waist_y) * 0.5, 0.0),
            );
            for side in [-1.0f32, 1.0] {
                let bottom = p.ankle_y * 1.15;
                part(
                    leg_sleeve(
                        bottom,
                        if w.bottom == BottomStyle::CargoPants {
                            1.02
                        } else {
                            0.78
                        },
                    ),
                    mat,
                    Transform::from_xyz(side * leg_x, (p.hip_y + bottom) * 0.5, 0.0),
                );
                if w.bottom == BottomStyle::CargoPants {
                    part(
                        ovoid(Vec3::new(thigh_r * 0.45, 0.045 * p.h, thigh_r * 0.22), 1.35),
                        &mats.primary,
                        Transform::from_xyz(
                            side * (leg_x + thigh_r * 1.02),
                            p.knee_y + 0.095 * p.h,
                            0.0,
                        ),
                    );
                }
            }
        }
        BottomStyle::FlaredPants => {
            part(
                hip_overlay(),
                &mats.secondary,
                Transform::from_xyz(0.0, (p.hip_y + p.waist_y) * 0.5, 0.0),
            );
            for side in [-1.0f32, 1.0] {
                let bottom = p.ankle_y * 0.75;
                part(
                    lofted_column(
                        Vec2::splat(thigh_r * 1.18),
                        Vec2::splat(thigh_r * 1.12),
                        p.hip_y - p.knee_y,
                        0.0,
                        1.9,
                        4,
                        16,
                    ),
                    &mats.secondary,
                    Transform::from_xyz(side * leg_x, (p.hip_y + p.knee_y) * 0.5, 0.0),
                );
                part(
                    lofted_column(
                        Vec2::splat(thigh_r * 1.34),
                        Vec2::splat(thigh_r * 0.90),
                        p.knee_y - bottom,
                        0.0,
                        1.8,
                        4,
                        16,
                    ),
                    &mats.secondary,
                    Transform::from_xyz(side * leg_x, (p.knee_y + bottom) * 0.5, 0.0),
                );
            }
        }
        BottomStyle::PleatedSkirt => {
            let hem = p.knee_y * 1.04;
            part(
                hip_overlay(),
                &mats.secondary,
                Transform::from_xyz(0.0, (p.hip_y + p.waist_y) * 0.5, 0.0),
            );
            part(
                lofted_column(
                    Vec2::new(p.hip_hw * 1.72, p.chest_depth * 1.78),
                    Vec2::new(p.hip_hw * 1.07, p.chest_depth * 1.02),
                    p.hip_y - hem,
                    0.0,
                    1.6,
                    5,
                    24,
                ),
                &mats.primary,
                Transform::from_xyz(0.0, (hem + p.hip_y) * 0.5, 0.0),
            );
            for pleat in -3..=3 {
                part(
                    ovoid(
                        Vec3::new(0.006 * p.h, (p.hip_y - hem) * 0.40, 0.007 * p.h),
                        1.2,
                    ),
                    &mats.secondary,
                    Transform::from_xyz(
                        pleat as f32 * p.hip_hw * 0.42,
                        (hem + p.hip_y) * 0.5,
                        -p.chest_depth * 1.68,
                    ),
                );
            }
        }
    }
}

fn spawn_super_suit(
    part: &mut impl FnMut(Mesh, &Handle<StandardMaterial>, Transform),
    p: &Proportions,
    mats: &StudioMats,
) {
    // The suit itself recolors the body; here we add emblem, belt and trim.
    part(
        ovoid(Vec3::new(0.055 * p.h, 0.06 * p.h, 0.014 * p.h), 0.7),
        &mats.core,
        Transform::from_xyz(0.0, p.chest_y + 0.015 * p.h, -p.chest_depth * 0.98),
    );
    part(
        lofted_column(
            Vec2::new(p.waist_hw * 1.10, p.chest_depth * 0.94),
            Vec2::new(p.waist_hw * 1.10, p.chest_depth * 0.94),
            0.03 * p.h,
            0.0,
            1.6,
            2,
            18,
        ),
        &mats.secondary,
        Transform::from_xyz(0.0, p.waist_y + 0.01 * p.h, 0.0),
    );
    // Shoulder trim.
    for side in [-1.0f32, 1.0] {
        part(
            ovoid(Vec3::splat(0.035 * p.h), 0.9),
            &mats.secondary,
            Transform::from_xyz(side * p.shoulder_hw * 1.02, p.shoulder_y, 0.0),
        );
    }
}

fn spawn_mecha_armor(
    part: &mut impl FnMut(Mesh, &Handle<StandardMaterial>, Transform),
    p: &Proportions,
    mats: &StudioMats,
    upper_r: f32,
    armor: ArmorStyle,
) {
    let hr = p.head_r;
    let head_c = Vec3::new(0.0, p.head_c_y, 0.0);
    // Helmet dome + crest fin + ear pods (face stays open).
    part(
        ovoid(Vec3::new(hr * 1.06, hr * 1.02, hr * 1.04), 1.0),
        &mats.metal,
        Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.22, hr * 0.10)),
    );
    part(
        ovoid(Vec3::new(hr * 0.08, hr * 0.55, hr * 0.65), 0.8),
        &mats.core,
        Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.95, hr * 0.15)),
    );
    for side in [-1.0f32, 1.0] {
        part(
            ovoid(Vec3::new(hr * 0.16, hr * 0.34, hr * 0.34), 0.8),
            &mats.metal,
            Transform::from_translation(head_c + Vec3::new(side * hr * 1.02, 0.05 * hr, 0.0)),
        );
    }
    // Chest cuirass + core.
    part(
        lofted_column(
            Vec2::new(p.waist_hw * 1.18, p.chest_depth * 1.16),
            Vec2::new(p.shoulder_hw * 1.06, p.chest_depth * 1.24),
            (p.shoulder_y - p.waist_y) * 0.94,
            0.02,
            1.35,
            4,
            18,
        ),
        &mats.metal,
        Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5 + 0.01 * p.h, 0.0),
    );
    part(
        ovoid(Vec3::new(0.040 * p.h, 0.040 * p.h, 0.016 * p.h), 0.7),
        &mats.core,
        Transform::from_xyz(0.0, p.chest_y + 0.02 * p.h, -p.chest_depth * 1.16),
    );
    // Shoulder spheres.
    for side in [-1.0f32, 1.0] {
        part(
            ovoid(Vec3::splat(upper_r * 2.3), 1.0),
            &mats.metal,
            Transform::from_xyz(
                side * (p.shoulder_hw + upper_r * 0.55),
                p.shoulder_y + upper_r * 0.25,
                0.0,
            ),
        );
    }
    // Pelvis girdle.
    part(
        lofted_column(
            Vec2::new(p.hip_hw * 1.20, p.chest_depth * 1.08),
            Vec2::new(p.hip_hw * 1.14, p.chest_depth * 1.00),
            (p.waist_y - p.hip_y) * 0.85,
            0.0,
            1.35,
            3,
            16,
        ),
        &mats.metal,
        Transform::from_xyz(0.0, (p.hip_y + p.waist_y) * 0.5 - 0.01 * p.h, 0.0),
    );

    match armor {
        ArmorStyle::CrystalMecha => {
            // Faceted constellation crystals reinforce the crest, shoulders,
            // gauntlet line, and chest core without replacing the base suit.
            for side in [-1.0f32, 1.0] {
                part(
                    ovoid(Vec3::new(0.020 * p.h, 0.065 * p.h, 0.020 * p.h), 0.58),
                    &mats.core,
                    Transform::from_xyz(
                        side * (p.shoulder_hw + upper_r * 0.70),
                        p.shoulder_y + 0.055 * p.h,
                        -0.015 * p.h,
                    )
                    .with_rotation(Quat::from_rotation_z(-side * 0.30)),
                );
                part(
                    ovoid(Vec3::new(0.015 * p.h, 0.040 * p.h, 0.014 * p.h), 0.60),
                    &mats.core,
                    Transform::from_xyz(
                        side * p.hip_hw * 0.82,
                        p.hip_y + 0.035 * p.h,
                        -p.chest_depth * 1.16,
                    ),
                );
            }
            for point in 0..5 {
                let angle = point as f32 * TAU / 5.0;
                part(
                    ovoid(Vec3::new(0.010 * p.h, 0.022 * p.h, 0.010 * p.h), 0.55),
                    &mats.core,
                    Transform::from_xyz(
                        angle.cos() * 0.047 * p.h,
                        p.chest_y + angle.sin() * 0.045 * p.h,
                        -p.chest_depth * 1.25,
                    )
                    .with_rotation(Quat::from_rotation_z(-angle)),
                );
            }
        }
        ArmorStyle::DragonMecha => {
            // Swept horns and layered blade pauldrons give this suite a
            // distinct dragon-knight silhouette.
            for side in [-1.0f32, 1.0] {
                part(
                    ovoid(Vec3::new(0.018 * p.h, 0.085 * p.h, 0.022 * p.h), 0.62),
                    &mats.metal,
                    Transform::from_translation(
                        head_c + Vec3::new(side * hr * 0.58, hr * 0.88, hr * 0.08),
                    )
                    .with_rotation(Quat::from_rotation_z(-side * 0.52)),
                );
                part(
                    ovoid(Vec3::new(0.095 * p.h, 0.020 * p.h, 0.045 * p.h), 0.62),
                    &mats.secondary,
                    Transform::from_xyz(
                        side * (p.shoulder_hw + 0.030 * p.h),
                        p.shoulder_y + 0.025 * p.h,
                        0.015 * p.h,
                    )
                    .with_rotation(Quat::from_rotation_z(side * 0.18)),
                );
            }
            part(
                ovoid(Vec3::new(0.028 * p.h, 0.046 * p.h, 0.016 * p.h), 0.56),
                &mats.core,
                Transform::from_xyz(0.0, p.chest_y + 0.018 * p.h, -p.chest_depth * 1.27),
            );
        }
        ArmorStyle::None | ArmorStyle::SuperSuit | ArmorStyle::MechaArmor => {}
    }
}

fn spawn_fantasy_flair(
    part: &mut impl FnMut(Mesh, &Handle<StandardMaterial>, Transform),
    p: &Proportions,
    mats: &StudioMats,
    flair: FlairStyle,
) {
    match flair {
        FlairStyle::None => {}
        FlairStyle::StarGem => {
            part(
                ovoid(Vec3::new(0.032 * p.h, 0.040 * p.h, 0.015 * p.h), 0.56),
                &mats.core,
                Transform::from_xyz(0.0, p.chest_y + 0.020 * p.h, -p.chest_depth * 1.34),
            );
            for ray in 0..5 {
                let angle = ray as f32 * TAU / 5.0;
                part(
                    ovoid(Vec3::new(0.006 * p.h, 0.024 * p.h, 0.006 * p.h), 0.65),
                    &mats.highlight,
                    Transform::from_xyz(
                        angle.cos() * 0.042 * p.h,
                        p.chest_y + 0.020 * p.h + angle.sin() * 0.042 * p.h,
                        -p.chest_depth * 1.35,
                    )
                    .with_rotation(Quat::from_rotation_z(-angle)),
                );
            }
        }
        FlairStyle::MoonGem => {
            for side in [-1.0f32, 1.0] {
                part(
                    ovoid(Vec3::new(0.013 * p.h, 0.038 * p.h, 0.010 * p.h), 0.58),
                    &mats.core,
                    Transform::from_xyz(
                        side * 0.020 * p.h,
                        p.chest_y + side * 0.012 * p.h,
                        -p.chest_depth * 1.34,
                    )
                    .with_rotation(Quat::from_rotation_z(side * 0.45)),
                );
            }
        }
        FlairStyle::RoyalMantle => {
            for side in [-1.0f32, 1.0] {
                part(
                    ovoid(Vec3::new(0.035 * p.h, 0.026 * p.h, 0.030 * p.h), 0.72),
                    &mats.core,
                    Transform::from_xyz(
                        side * p.shoulder_hw * 0.72,
                        p.shoulder_y + 0.012 * p.h,
                        -0.010 * p.h,
                    ),
                );
            }
            part(
                ovoid(Vec3::new(0.16 * p.h, 0.24 * p.h, 0.018 * p.h), 1.35),
                &mats.secondary,
                Transform::from_xyz(0.0, p.chest_y - 0.08 * p.h, p.chest_depth * 1.36),
            );
        }
        FlairStyle::ArcaneHalo => {
            for gem in 0..8 {
                let angle = gem as f32 * TAU / 8.0;
                part(
                    ovoid(Vec3::splat(0.012 * p.h), 0.55),
                    if gem % 2 == 0 {
                        &mats.core
                    } else {
                        &mats.highlight
                    },
                    Transform::from_xyz(
                        angle.cos() * p.head_r * 1.22,
                        p.head_c_y + angle.sin() * p.head_r * 1.22,
                        p.head_r * 0.72,
                    ),
                );
            }
        }
        FlairStyle::MechaWings => {
            for side in [-1.0f32, 1.0] {
                for blade in 0..2 {
                    part(
                        ovoid(Vec3::new(0.030 * p.h, 0.14 * p.h, 0.018 * p.h), 0.60),
                        if blade == 0 { &mats.metal } else { &mats.core },
                        Transform::from_xyz(
                            side * (0.11 + blade as f32 * 0.045) * p.h,
                            p.chest_y + (0.02 - blade as f32 * 0.055) * p.h,
                            p.chest_depth * 1.30,
                        )
                        .with_rotation(Quat::from_rotation_z(side * (0.42 + blade as f32 * 0.18))),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character_studio::generators::build_character_patch;
    use crate::character_studio::spec::CharacterSpec;

    #[test]
    fn generated_parts_map_to_runtime_animation_regions() {
        let patch = build_character_patch(&CharacterSpec::default());
        let proportions = Proportions::from_patch(&patch);
        assert_eq!(
            classify_studio_part(
                &proportions,
                Vec3::new(proportions.shoulder_hw, proportions.shoulder_y, 0.0)
            )
            .0,
            StudioBodyRegion::RightArm
        );
        assert_eq!(
            classify_studio_part(
                &proportions,
                Vec3::new(-proportions.hip_hw, proportions.knee_y, 0.0)
            )
            .0,
            StudioBodyRegion::LeftLeg
        );
        assert_eq!(
            classify_studio_part(&proportions, Vec3::Y * proportions.head_c_y).0,
            StudioBodyRegion::Head
        );
    }

    #[test]
    fn chest_shape_and_sex_baselines_produce_distinct_bounded_depths() {
        let mut masculine = CharacterSpec::default();
        masculine.body.chest_shape = 0.0;
        let shallow = Proportions::from_patch(&build_character_patch(&masculine));
        masculine.body.chest_shape = 1.0;
        let deep = Proportions::from_patch(&build_character_patch(&masculine));
        assert!(deep.chest_depth > shallow.chest_depth);

        let mut feminine = masculine;
        feminine.sex = Sex::Female;
        let female = Proportions::from_patch(&build_character_patch(&feminine));
        assert!(female.female);
        assert!(female.chest_depth.is_finite() && female.chest_depth > 0.0);
    }
}
