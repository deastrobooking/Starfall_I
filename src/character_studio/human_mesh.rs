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
use std::f32::consts::TAU;

use super::generators::{CharacterPatch, CharacterSlot, SlotContent};
use super::spec::{HairStyle, Sex};
use crate::procedural_meshes::superellipsoid_mesh;

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

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
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
        let h = 1.75 * spread(m(patch, "body_height"), 1.0, 0.09) * if female { 0.945 } else { 1.0 };
        let muscle = m(patch, "body_muscle");
        let weight = m(patch, "body_weight");
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
            waist_hw: spread(m(patch, "waist_width"), waist_base, 0.020) * h
                * (1.0 + (weight - 0.5) * 0.55),
            hip_hw: spread(m(patch, "hips_wide"), hip_base, 0.018) * h
                * (1.0 + (weight - 0.5) * 0.30),
            chest_depth: 0.062 * h * bulk * if female { 0.94 } else { 1.0 },
            limb,
            muscle,
            weight,
            head_r: 0.0655 * h * if female { 0.97 } else { 1.0 },
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
        skin: matte(materials, skin_c, 0.72),
        eye_white: matte(materials, Color::srgb(0.94, 0.94, 0.92), 0.35),
        iris: matte(materials, patch.material("eyes"), 0.30),
        hair: matte(materials, hair_c, 0.80),
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
    }
}

// ── Spawner ───────────────────────────────────────────────────────────────────

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

    let mut part = |mesh: Mesh, mat: &Handle<StandardMaterial>, transform: Transform| {
        let e = commands
            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), transform))
            .id();
        commands.entity(root).add_child(e);
    };

    let mecha = matches!(patch.slots.get(&CharacterSlot::Torso), Some(SlotContent::Mecha));
    let suit = matches!(patch.slots.get(&CharacterSlot::Torso), Some(SlotContent::Suit));
    let cloth = matches!(patch.slots.get(&CharacterSlot::Torso), Some(SlotContent::Cloth));
    // Skin-tight layers recolor the body segments themselves.
    let body_mat = if suit { &mats.suit } else if mecha { &mats.secondary } else { &mats.skin };

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
    if p.female {
        for side in [-1.0f32, 1.0] {
            part(
                ovoid(
                    Vec3::new(0.052 * p.h * 0.62, 0.052 * p.h * 0.62, 0.052 * p.h * 0.55),
                    1.0,
                ),
                body_mat,
                Transform::from_xyz(side * p.shoulder_hw * 0.38, p.chest_y, -p.chest_depth * 0.72),
            );
        }
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
    spawn_head(&mut part, patch, &p, &mats);

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
        // Hand: gloved in suit/mecha, skin otherwise.
        let hand_mat = match patch.slots.get(&CharacterSlot::Hands) {
            Some(SlotContent::Suit) => &mats.primary,
            Some(SlotContent::Mecha) => &mats.metal,
            _ => &mats.skin,
        };
        let hand_scale = if mecha { 1.8 } else { 1.0 };
        part(
            ovoid(
                Vec3::new(
                    fore_r * 1.0 * hand_scale,
                    0.052 * p.h * 0.55 * hand_scale,
                    fore_r * 0.62 * hand_scale,
                ),
                1.15,
            ),
            hand_mat,
            Transform::from_xyz(x, wrist_y - 0.028 * p.h * hand_scale, 0.0),
        );
    }

    // ── Legs ─────────────────────────────────────────────────────────────────
    let thigh_r = p.limb_radius(0.044);
    let shin_r = p.limb_radius(0.031);
    let leg_x = p.hip_hw * 0.52;
    let legs_mecha = matches!(patch.slots.get(&CharacterSlot::Legs), Some(SlotContent::Mecha));
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
        // Foot / footwear.
        let (foot_mat, foot_scale) = match patch.slots.get(&CharacterSlot::Feet) {
            Some(SlotContent::Cloth) => (&mats.dark, 1.12),
            Some(SlotContent::Suit) => (&mats.secondary, 1.08),
            Some(SlotContent::Mecha) => (&mats.metal, 1.9),
            _ => (&mats.skin, 1.0),
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
            Transform::from_xyz(
                x,
                p.ankle_y * 0.55,
                -0.028 * p.h * foot_scale,
            ),
        );
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
    if cloth {
        spawn_clothes(&mut part, &p, &mats, upper_r, arm_len, elbow_y, thigh_r, leg_x);
    }
    if suit {
        spawn_super_suit(&mut part, &p, &mats);
    }
    if mecha {
        spawn_mecha_armor(&mut part, &p, &mats, upper_r);
    }

    root
}

// ── Head + face ───────────────────────────────────────────────────────────────

fn spawn_head(
    part: &mut impl FnMut(Mesh, &Handle<StandardMaterial>, Transform),
    patch: &CharacterPatch,
    p: &Proportions,
    mats: &StudioMats,
) {
    let hr = p.head_r;
    let cheek = spread(m(patch, "face_cheek_full"), 1.0, 0.14);
    let face_z = -hr * 0.86; // faces -Z
    let head_c = Vec3::new(0.0, p.head_c_y, 0.0);

    // Cranium.
    part(
        ovoid(Vec3::new(hr * 0.82 * cheek, hr, hr * 0.88), 1.0),
        &mats.skin,
        Transform::from_translation(head_c),
    );
    // Jaw + chin.
    let jaw_w = spread(m(patch, "face_jaw_wide"), 0.62, 0.16);
    let chin = spread(m(patch, "face_chin_long"), 1.0, 0.35);
    part(
        ovoid(
            Vec3::new(hr * jaw_w * cheek, hr * 0.46 * chin, hr * 0.62),
            1.05,
        ),
        &mats.skin,
        Transform::from_translation(head_c + Vec3::new(0.0, -hr * 0.62, -hr * 0.12)),
    );
    // Nose.
    let nose_len = spread(m(patch, "face_nose_long"), 1.0, 0.4);
    let nose_w = spread(m(patch, "face_nose_wide"), 1.0, 0.4);
    part(
        ovoid(
            Vec3::new(hr * 0.11 * nose_w, hr * 0.20 * nose_len, hr * 0.14),
            1.3,
        ),
        &mats.skin,
        Transform::from_translation(head_c + Vec3::new(0.0, -hr * 0.16, face_z - hr * 0.06)),
    );
    // Brow bar.
    let brow = spread(m(patch, "face_brow_heavy"), 1.0, 0.5);
    part(
        ovoid(Vec3::new(hr * 0.52, hr * 0.07 * brow, hr * 0.10), 0.9),
        &mats.brow,
        Transform::from_translation(head_c + Vec3::new(0.0, hr * 0.22, face_z + hr * 0.04)),
    );
    // Eyes.
    let eye = spread(m(patch, "face_eye_large"), 1.0, 0.3);
    for side in [-1.0f32, 1.0] {
        let eye_pos = head_c + Vec3::new(side * hr * 0.30, hr * 0.06, face_z + hr * 0.07);
        part(
            ovoid(Vec3::new(hr * 0.115 * eye, hr * 0.085 * eye, hr * 0.05), 1.0),
            &mats.eye_white,
            Transform::from_translation(eye_pos),
        );
        part(
            ovoid(Vec3::splat(hr * 0.048 * eye), 1.0),
            &mats.iris,
            Transform::from_translation(eye_pos + Vec3::new(0.0, 0.0, -hr * 0.035)),
        );
    }
    // Mouth.
    let mouth = spread(m(patch, "face_mouth_wide"), 1.0, 0.4);
    part(
        ovoid(Vec3::new(hr * 0.26 * mouth, hr * 0.035, hr * 0.05), 1.0),
        &mats.brow,
        Transform::from_translation(head_c + Vec3::new(0.0, -hr * 0.52, face_z - hr * 0.02)),
    );
    // Ears.
    for side in [-1.0f32, 1.0] {
        part(
            ovoid(Vec3::new(hr * 0.07, hr * 0.16, hr * 0.10), 1.0),
            &mats.skin,
            Transform::from_translation(head_c + Vec3::new(side * hr * 0.85 * cheek, 0.0, 0.0)),
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
    }
}

// ── Outfit layers ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn spawn_clothes(
    part: &mut impl FnMut(Mesh, &Handle<StandardMaterial>, Transform),
    p: &Proportions,
    mats: &StudioMats,
    upper_r: f32,
    arm_len: f32,
    elbow_y: f32,
    thigh_r: f32,
    leg_x: f32,
) {
    // Shirt: torso overlay + short sleeves.
    part(
        lofted_column(
            Vec2::new(p.waist_hw * 1.14, p.chest_depth * 1.05),
            Vec2::new(p.shoulder_hw * 1.02, p.chest_depth * 1.12),
            (p.shoulder_y - p.waist_y) * 1.06,
            0.03,
            2.0,
            5,
            22,
        ),
        &mats.primary,
        Transform::from_xyz(0.0, (p.waist_y + p.shoulder_y) * 0.5, 0.0),
    );
    for side in [-1.0f32, 1.0] {
        let x = side * (p.shoulder_hw + upper_r * 0.35);
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
            &mats.primary,
            Transform::from_xyz(x, p.shoulder_y - arm_len * 0.28, 0.0),
        );
        let _ = elbow_y;
    }
    // Pants: hips → shins.
    part(
        lofted_column(
            Vec2::new(p.hip_hw * 1.10, p.chest_depth * 1.02),
            Vec2::new(p.waist_hw * 1.12, p.chest_depth * 0.96),
            (p.waist_y - p.hip_y) * 1.15,
            0.0,
            2.0,
            3,
            18,
        ),
        &mats.secondary,
        Transform::from_xyz(0.0, (p.hip_y + p.waist_y) * 0.5, 0.0),
    );
    for side in [-1.0f32, 1.0] {
        let x = side * leg_x;
        part(
            lofted_column(
                Vec2::splat(thigh_r * 0.95),
                Vec2::new(thigh_r * 1.12, thigh_r * 1.16),
                p.hip_y - p.knee_y * 0.82,
                0.0,
                2.0,
                4,
                16,
            ),
            &mats.secondary,
            Transform::from_xyz(x, (p.hip_y + p.knee_y * 0.82) * 0.5, 0.0),
        );
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
}
