use bevy::prelude::*;

use crate::components::character::{
    CartoonAnimator, CartoonCharacter, CartoonPart, CartoonPartKind, CartoonRole,
};
use crate::components::enemy::EnemyType;
use crate::components::faction::Faction;

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CartoonCharacterConfig {
    pub name: &'static str,
    pub role: CartoonRole,
    pub skin: Color,
    pub outfit: Color,
    pub accent: Color,
    pub hair: Color,
    pub scale: f32,
    /// Multiplies the body (and proportionally arms/legs) in the X axis.
    pub body_width: f32,
    pub has_hat: bool,
    pub has_horns: bool,
    pub extra_horns: bool,
    pub has_tail: bool,
    pub has_star_badge: bool,
    pub has_visor: bool,
    pub has_shoulder_pads: bool,
    pub has_cape: bool,
    pub has_spine_ridges: bool,
    pub eye_color: Color,
    pub emissive_eyes: bool,
}

// ── Hero configs ──────────────────────────────────────────────────────────────

pub fn hero_config(name: &'static str) -> CartoonCharacterConfig {
    match name {
        "Vincenzo" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            // Warm tan skin, navy-blue military jacket, gold trim
            skin: Color::srgb(0.93, 0.70, 0.48),
            outfit: Color::srgb(0.09, 0.28, 0.80),
            accent: Color::srgb(0.95, 0.78, 0.12),
            hair: Color::srgb(0.12, 0.07, 0.03),
            scale: 1.0,
            body_width: 1.0,
            has_hat: false,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_shoulder_pads: true,
            has_cape: false,
            has_spine_ridges: false,
            eye_color: Color::srgb(0.06, 0.06, 0.12),
            emissive_eyes: false,
        },
        "Antonio" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            // Light olive skin, blue-teal lab coat, cyan highlights — the tech guy
            skin: Color::srgb(0.90, 0.74, 0.56),
            outfit: Color::srgb(0.16, 0.48, 0.82),
            accent: Color::srgb(0.10, 0.92, 0.98),
            hair: Color::srgb(0.42, 0.28, 0.10),
            scale: 1.0,
            body_width: 0.92,
            has_hat: false,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: true,
            has_shoulder_pads: false,
            has_cape: false,
            has_spine_ridges: false,
            eye_color: Color::srgb(0.04, 0.06, 0.14),
            emissive_eyes: false,
        },
        "Angelo" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            // Medium brown skin, forest-green acrobat suit, silver-chrome accent
            skin: Color::srgb(0.88, 0.68, 0.48),
            outfit: Color::srgb(0.08, 0.52, 0.20),
            accent: Color::srgb(0.78, 0.86, 0.92),
            hair: Color::srgb(0.85, 0.85, 0.90),
            scale: 1.0,
            body_width: 0.94,
            has_hat: false,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_shoulder_pads: false,
            has_cape: true,
            has_spine_ridges: false,
            eye_color: Color::srgb(0.03, 0.04, 0.10),
            emissive_eyes: false,
        },
        "Joseph" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            // Olive skin, deep-crimson heavy outfit, off-white trim — the big brother
            skin: Color::srgb(0.86, 0.65, 0.44),
            outfit: Color::srgb(0.75, 0.12, 0.08),
            accent: Color::srgb(0.92, 0.90, 0.84),
            hair: Color::srgb(0.60, 0.58, 0.56),
            scale: 1.0,
            body_width: 1.18,
            has_hat: false,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_shoulder_pads: true,
            has_cape: false,
            has_spine_ridges: false,
            eye_color: Color::srgb(0.05, 0.04, 0.10),
            emissive_eyes: false,
        },
        _ => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.95, 0.68, 0.45),
            outfit: Color::srgb(0.15, 0.45, 0.95),
            accent: Color::srgb(1.0, 0.86, 0.2),
            hair: Color::srgb(0.10, 0.06, 0.03),
            scale: 1.0,
            body_width: 1.0,
            has_hat: false,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_shoulder_pads: false,
            has_cape: false,
            has_spine_ridges: false,
            eye_color: Color::srgb(0.03, 0.03, 0.05),
            emissive_eyes: false,
        },
    }
}

// ── Enemy configs ─────────────────────────────────────────────────────────────

pub fn enemy_config(
    enemy_type: EnemyType,
    faction: Option<Faction>,
    name: &'static str,
    scale: f32,
) -> CartoonCharacterConfig {
    let faction = faction.unwrap_or_default();

    let (role, outfit, accent, skin, hair, has_horns, extra_horns, has_tail, has_spine_ridges,
        eye_color, emissive_eyes) = match faction {
        Faction::DragonRoyalty => (
            CartoonRole::Dragon,
            Color::srgb(0.92, 0.16, 0.06),   // crimson scales
            Color::srgb(1.0, 0.72, 0.12),    // molten gold
            Color::srgb(0.72, 0.20, 0.10),
            Color::srgb(0.12, 0.02, 0.01),
            true, true, true, true,           // horns + extra horns + tail + spine
            Color::srgb(1.0, 0.58, 0.0),     // amber glow
            true,
        ),
        Faction::DragonExile => (
            CartoonRole::Dragon,
            Color::srgb(0.18, 0.28, 0.58),   // midnight blue
            Color::srgb(0.70, 0.88, 1.0),    // ice silver
            Color::srgb(0.38, 0.50, 0.78),
            Color::srgb(0.03, 0.04, 0.10),
            true, false, true, true,          // horns + tail + spine
            Color::srgb(0.3, 0.75, 1.0),     // cold blue glow
            true,
        ),
        Faction::CorruptedHuman => (
            CartoonRole::Rival,
            Color::srgb(0.32, 0.06, 0.40),   // deep purple
            Color::srgb(1.0, 0.18, 0.88),    // hot pink
            Color::srgb(0.75, 0.55, 0.48),
            Color::srgb(0.05, 0.02, 0.06),
            false, false, false, false,
            Color::srgb(1.0, 0.1, 0.9),      // magenta glow
            true,
        ),
        Faction::WizardScientist => (
            CartoonRole::Wizard,
            Color::srgb(0.28, 0.72, 0.92),   // sky blue robe
            Color::srgb(1.0, 0.94, 0.52),    // starlight yellow
            Color::srgb(0.92, 0.68, 0.45),
            Color::srgb(0.22, 0.16, 0.08),
            false, false, false, false,
            Color::srgb(0.0, 0.85, 1.0),     // cyan sci-fi glow
            true,
        ),
        Faction::HeroSister => (
            CartoonRole::Hero,
            Color::srgb(0.92, 0.30, 0.80),   // magenta
            Color::srgb(0.45, 0.95, 1.0),    // sky blue
            Color::srgb(0.92, 0.68, 0.45),
            Color::srgb(0.18, 0.05, 0.20),
            false, false, false, false,
            Color::srgb(0.05, 0.04, 0.10),
            false,
        ),
        Faction::HeroBrother => (
            CartoonRole::Hero,
            Color::srgb(0.12, 0.42, 0.92),
            Color::srgb(1.0, 0.85, 0.18),
            Color::srgb(0.93, 0.70, 0.48),
            Color::srgb(0.10, 0.06, 0.03),
            false, false, false, false,
            Color::srgb(0.05, 0.05, 0.10),
            false,
        ),
        _ => (
            // Dimensional Aliens — neon green with purple accents
            CartoonRole::Alien,
            Color::srgb(0.20, 0.90, 0.42),
            Color::srgb(0.82, 0.22, 1.0),
            Color::srgb(0.32, 0.88, 0.52),
            Color::srgb(0.06, 0.18, 0.09),
            matches!(enemy_type, EnemyType::Heavy | EnemyType::Hybrid),
            false,
            matches!(enemy_type, EnemyType::SpikeAlien | EnemyType::Hybrid),
            matches!(enemy_type, EnemyType::Hybrid),
            Color::srgb(0.2, 1.0, 0.4),     // bright alien green
            true,
        ),
    };

    let type_scale = match enemy_type {
        EnemyType::Drone => 0.82,
        EnemyType::Soldier => 1.0,
        EnemyType::Heavy => 1.22,
        EnemyType::SpikeAlien => 1.05,
        EnemyType::Hybrid => 1.35,
    };
    let type_width = match enemy_type {
        EnemyType::Heavy => 1.15,
        EnemyType::Hybrid => 1.28,
        _ => 1.0,
    };

    CartoonCharacterConfig {
        name,
        role,
        skin,
        outfit,
        accent,
        hair,
        scale: scale * type_scale,
        body_width: type_width,
        has_hat: role == CartoonRole::Wizard,
        has_horns,
        extra_horns,
        has_tail,
        has_star_badge: matches!(role, CartoonRole::Hero | CartoonRole::Wizard),
        has_visor: false,
        has_shoulder_pads: matches!(enemy_type, EnemyType::Heavy | EnemyType::Hybrid),
        has_cape: false,
        has_spine_ridges,
        eye_color,
        emissive_eyes,
    }
}

// ── Spawning ──────────────────────────────────────────────────────────────────

pub fn spawn_cartoon_character(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    config: CartoonCharacterConfig,
    position: Vec3,
) -> Entity {
    let root = commands
        .spawn((Transform::from_translation(position), GlobalTransform::default()))
        .id();
    attach_cartoon_character(commands, meshes, materials, root, config, position);
    root
}

pub fn attach_cartoon_character(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    config: CartoonCharacterConfig,
    root_position: Vec3,
) {
    commands.entity(root).insert((
        CartoonCharacter {
            name: config.name.to_string(),
            role: config.role,
            scale: config.scale,
        },
        CartoonAnimator::new(root_position),
    ));

    // ── Materials ─────────────────────────────────────────────────────────────
    let skin = char_mat(materials, config.skin);
    let outfit = char_mat(materials, config.outfit);
    let accent = char_mat(materials, config.accent);
    let hair = char_mat(materials, config.hair);
    let shadow = materials.add(StandardMaterial {
        base_color: Color::srgba(0.02, 0.02, 0.04, 0.35),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let eye_mat = if config.emissive_eyes {
        emissive_mat(materials, config.eye_color, 6.0)
    } else {
        char_mat(materials, config.eye_color)
    };
    // Eyebrow color: slightly darker than hair
    let brow_color = darken(config.hair, 0.7);
    let brow = char_mat(materials, brow_color);

    let s = config.scale;
    let bw = config.body_width; // body-width multiplier

    // ── Shadow ────────────────────────────────────────────────────────────────
    spawn_part(
        commands, meshes, root,
        CartoonPartKind::Shadow,
        Mesh::from(Cylinder::new(0.58 * s, 0.02 * s)),
        shadow,
        Transform::from_xyz(0.0, -0.92 * s, 0.0),
    );

    // ── Body ──────────────────────────────────────────────────────────────────
    spawn_part(
        commands, meshes, root,
        CartoonPartKind::Body,
        Mesh::from(Cuboid::new(0.72 * s * bw, 0.86 * s, 0.42 * s)),
        outfit.clone(),
        Transform::from_xyz(0.0, -0.12 * s, 0.0),
    );

    // ── Head ──────────────────────────────────────────────────────────────────
    spawn_part(
        commands, meshes, root,
        CartoonPartKind::Head,
        Mesh::from(Sphere::new(0.36 * s)),
        skin.clone(),
        Transform::from_xyz(0.0, 0.58 * s, 0.0),
    );

    // ── Hair (back dome, slightly larger than head) ───────────────────────────
    spawn_part(
        commands, meshes, root,
        CartoonPartKind::Hair,
        Mesh::from(Sphere::new(0.37 * s)),
        hair.clone(),
        Transform::from_xyz(0.0, 0.76 * s, -0.04 * s)
            .with_scale(Vec3::new(1.0, 0.44, 0.88)),
    );

    // ── Eyes ──────────────────────────────────────────────────────────────────
    for (kind, eye_x) in [
        (CartoonPartKind::LeftEye, -0.13_f32),
        (CartoonPartKind::RightEye, 0.13_f32),
    ] {
        // White sclera
        spawn_part(
            commands, meshes, root, kind,
            Mesh::from(Sphere::new(0.062 * s)),
            char_mat(materials, Color::srgb(0.95, 0.95, 0.97)),
            Transform::from_xyz(eye_x * s, 0.61 * s, -0.30 * s),
        );
        // Iris / pupil on top
        let iris_kind = if eye_x < 0.0 {
            CartoonPartKind::LeftEyebrow // reuse kind slot for iris overlay
        } else {
            CartoonPartKind::RightEyebrow
        };
        spawn_part(
            commands, meshes, root, iris_kind,
            Mesh::from(Sphere::new(0.044 * s)),
            eye_mat.clone(),
            Transform::from_xyz(eye_x * s, 0.61 * s, -0.34 * s),
        );
    }

    // ── Eyebrows ──────────────────────────────────────────────────────────────
    // Thin flat cuboids above the eyes; slight inward tilt gives personality.
    for (kind, brow_x, tilt_z) in [
        (CartoonPartKind::Nose, -0.13_f32, 0.18_f32),
        (CartoonPartKind::Visor, 0.13_f32, -0.18_f32),
    ] {
        spawn_part(
            commands, meshes, root, kind,
            Mesh::from(Cuboid::new(0.115 * s, 0.032 * s, 0.05 * s)),
            brow.clone(),
            Transform::from_xyz(brow_x * s, 0.685 * s, -0.30 * s)
                .with_rotation(Quat::from_rotation_z(tilt_z)),
        );
    }

    // ── Nose (small rounded bump on face) ─────────────────────────────────────
    spawn_part(
        commands, meshes, root,
        CartoonPartKind::SpineRidge, // temporarily borrow kind for nose geometry
        Mesh::from(Sphere::new(0.065 * s)),
        skin.clone(),
        Transform::from_xyz(0.0, 0.575 * s, -0.34 * s)
            .with_scale(Vec3::new(0.55, 0.55, 1.0)),
    );

    // ── Hat (wizard only) ─────────────────────────────────────────────────────
    if config.has_hat {
        spawn_part(
            commands, meshes, root,
            CartoonPartKind::Hat,
            Mesh::from(Cone { radius: 0.36 * s, height: 0.72 * s }),
            accent.clone(),
            Transform::from_xyz(0.0, 1.12 * s, 0.0),
        );
        // Hat brim ring
        spawn_part(
            commands, meshes, root,
            CartoonPartKind::Hat,
            Mesh::from(Cylinder::new(0.44 * s, 0.06 * s)),
            accent.clone(),
            Transform::from_xyz(0.0, 0.80 * s, 0.0),
        );
    }

    // ── Arms ──────────────────────────────────────────────────────────────────
    let arm_x = (0.36 + 0.085) * s * bw; // just outside body edge
    for (kind, x) in [
        (CartoonPartKind::LeftArm, -arm_x),
        (CartoonPartKind::RightArm, arm_x),
    ] {
        spawn_part(
            commands, meshes, root, kind,
            Mesh::from(Cuboid::new(0.17 * s, 0.62 * s, 0.17 * s)),
            skin.clone(),
            Transform::from_xyz(x, -0.12 * s, 0.0),
        );
    }

    // ── Legs + Feet ───────────────────────────────────────────────────────────
    let leg_x = 0.195 * s * bw;
    for (leg_k, foot_k, x) in [
        (CartoonPartKind::LeftLeg, CartoonPartKind::LeftFoot, -leg_x),
        (CartoonPartKind::RightLeg, CartoonPartKind::RightFoot, leg_x),
    ] {
        spawn_part(
            commands, meshes, root, leg_k,
            Mesh::from(Cuboid::new(0.20 * s, 0.56 * s, 0.20 * s)),
            outfit.clone(),
            Transform::from_xyz(x, -0.74 * s, 0.0),
        );
        spawn_part(
            commands, meshes, root, foot_k,
            Mesh::from(Cuboid::new(0.30 * s, 0.14 * s, 0.44 * s)),
            accent.clone(),
            Transform::from_xyz(x, -1.08 * s, -0.08 * s),
        );
    }

    // ── Horns ─────────────────────────────────────────────────────────────────
    if config.has_horns {
        // Primary horns — curved outward and upward
        for (kind, x, rot_z) in [
            (CartoonPartKind::LeftHorn, -0.22_f32, 0.40_f32),
            (CartoonPartKind::RightHorn, 0.22_f32, -0.40_f32),
        ] {
            spawn_part(
                commands, meshes, root, kind,
                Mesh::from(Cone { radius: 0.10 * s, height: 0.48 * s }),
                accent.clone(),
                Transform::from_xyz(x * s, 0.98 * s, 0.04 * s)
                    .with_rotation(Quat::from_rotation_z(rot_z)),
            );
        }
        if config.extra_horns {
            // Secondary smaller horns slightly forward
            for (kind, x, rot_z, rot_x) in [
                (CartoonPartKind::LeftHorn, -0.14_f32, 0.22_f32, -0.30_f32),
                (CartoonPartKind::RightHorn, 0.14_f32, -0.22_f32, -0.30_f32),
            ] {
                spawn_part(
                    commands, meshes, root, kind,
                    Mesh::from(Cone { radius: 0.065 * s, height: 0.30 * s }),
                    accent.clone(),
                    Transform::from_xyz(x * s, 0.88 * s, -0.16 * s)
                        .with_rotation(
                            Quat::from_rotation_z(rot_z) * Quat::from_rotation_x(rot_x),
                        ),
                );
            }
        }
    }

    // ── Tail ──────────────────────────────────────────────────────────────────
    if config.has_tail {
        // Tapering tail from three segments
        spawn_part(
            commands, meshes, root,
            CartoonPartKind::Tail,
            Mesh::from(Cuboid::new(0.18 * s, 0.18 * s, 0.55 * s)),
            accent.clone(),
            Transform::from_xyz(0.0, -0.44 * s, 0.42 * s)
                .with_rotation(Quat::from_rotation_x(0.40)),
        );
        spawn_part(
            commands, meshes, root,
            CartoonPartKind::Tail,
            Mesh::from(Cuboid::new(0.12 * s, 0.12 * s, 0.38 * s)),
            skin.clone(),
            Transform::from_xyz(0.0, -0.22 * s, 0.72 * s)
                .with_rotation(Quat::from_rotation_x(0.70)),
        );
    }

    // ── Spine ridges (dragons) ────────────────────────────────────────────────
    if config.has_spine_ridges {
        for (i, &(z, ridge_s)) in [
            (0.0_f32, 0.20_f32),
            (0.10, 0.18),
            (0.20, 0.15),
            (0.30, 0.12),
            (0.38, 0.09),
        ]
        .iter()
        .enumerate()
        {
            let ridge_y = 0.16 * s - i as f32 * 0.04 * s;
            spawn_part(
                commands, meshes, root,
                CartoonPartKind::SpineRidge,
                Mesh::from(Cone { radius: ridge_s * s * 0.45, height: ridge_s * s }),
                accent.clone(),
                Transform::from_xyz(0.0, ridge_y, z * s + 0.15 * s)
                    .with_rotation(Quat::from_rotation_x(-0.15)),
            );
        }
    }

    // ── Star Badge ────────────────────────────────────────────────────────────
    if config.has_star_badge {
        spawn_part(
            commands, meshes, root,
            CartoonPartKind::StarBadge,
            Mesh::from(Cuboid::new(0.22 * s, 0.22 * s, 0.05 * s)),
            accent.clone(),
            Transform::from_xyz(0.0, 0.05 * s, -0.23 * s)
                .with_rotation(Quat::from_rotation_z(0.78)),
        );
        // Small center gem on the badge
        spawn_part(
            commands, meshes, root,
            CartoonPartKind::StarBadge,
            Mesh::from(Sphere::new(0.055 * s)),
            emissive_mat(materials, config.accent, 2.5),
            Transform::from_xyz(0.0, 0.05 * s, -0.26 * s),
        );
    }

    // ── Tech Visor (Antonio) ──────────────────────────────────────────────────
    if config.has_visor {
        spawn_part(
            commands, meshes, root,
            CartoonPartKind::Visor,
            Mesh::from(Cuboid::new(0.38 * s, 0.10 * s, 0.05 * s)),
            emissive_mat(materials, config.accent, 1.8),
            Transform::from_xyz(0.0, 0.61 * s, -0.33 * s),
        );
    }

    // ── Shoulder pads ─────────────────────────────────────────────────────────
    if config.has_shoulder_pads {
        for (kind, x) in [
            (CartoonPartKind::ShoulderPadLeft, -arm_x),
            (CartoonPartKind::ShoulderPadRight, arm_x),
        ] {
            spawn_part(
                commands, meshes, root, kind,
                Mesh::from(Cuboid::new(0.30 * s, 0.12 * s, 0.30 * s)),
                accent.clone(),
                Transform::from_xyz(x, 0.22 * s, 0.0)
                    .with_scale(Vec3::new(1.0, 1.0, 1.2)),
            );
        }
    }

    // ── Cape (Angelo) ────────────────────────────────────────────────────────
    if config.has_cape {
        // Upper cape panel, attached behind shoulders
        spawn_part(
            commands, meshes, root,
            CartoonPartKind::Cape,
            Mesh::from(Cuboid::new(0.55 * s, 0.52 * s, 0.05 * s)),
            accent.clone(),
            Transform::from_xyz(0.0, 0.0 * s, 0.24 * s)
                .with_rotation(Quat::from_rotation_x(0.25)),
        );
        // Lower flap, offset and angled for a flowing look
        spawn_part(
            commands, meshes, root,
            CartoonPartKind::Cape,
            Mesh::from(Cuboid::new(0.42 * s, 0.36 * s, 0.04 * s)),
            outfit.clone(),
            Transform::from_xyz(0.0, -0.38 * s, 0.32 * s)
                .with_rotation(Quat::from_rotation_x(0.50)),
        );
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn spawn_part(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    root: Entity,
    kind: CartoonPartKind,
    mesh: Mesh,
    material: Handle<StandardMaterial>,
    transform: Transform,
) {
    let part = CartoonPart::new(root, kind, &transform);
    let entity = commands
        .spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(mesh)),
                material: MeshMaterial3d(material),
                transform,
                ..default()
            },
            part,
        ))
        .id();
    commands.entity(root).add_child(entity);
}

/// Stylised PBR: low roughness, slight self-emissive so characters stay
/// readable even in fully shadowed corners, no metallic.
fn char_mat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    let lin = color.to_linear();
    materials.add(StandardMaterial {
        base_color: color,
        emissive: LinearRgba::new(lin.red * 0.18, lin.green * 0.18, lin.blue * 0.18, 1.0),
        perceptual_roughness: 0.62,
        metallic: 0.0,
        reflectance: 0.18,
        ..default()
    })
}

/// Brightly glowing material for eyes, badge gems, visor lens.
fn emissive_mat(
    materials: &mut Assets<StandardMaterial>,
    color: Color,
    strength: f32,
) -> Handle<StandardMaterial> {
    let lin = color.to_linear();
    materials.add(StandardMaterial {
        base_color: color,
        emissive: LinearRgba::new(
            lin.red * strength,
            lin.green * strength,
            lin.blue * strength,
            1.0,
        ),
        perceptual_roughness: 0.4,
        ..default()
    })
}

fn darken(color: Color, factor: f32) -> Color {
    let lin = color.to_linear();
    Color::linear_rgb(lin.red * factor, lin.green * factor, lin.blue * factor)
}
