use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

use crate::components::character::{CartoonPart, CartoonPartKind};
use crate::rendering::PbrBundle;

// ── Slot identification ───────────────────────────────────────────────────────

/// Major visual group a CartoonPart belongs to — used for targeted swap/despawn.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PartSlotTag {
    Shadow,
    Body,        // torso core + belt
    Arms,        // upper arm, forearm, elbow joint, hands
    Legs,        // thigh, shin, knee joint, feet, boot shaft/accent
    Head,        // skull, hair, eyes, eyebrows, nose, mouth, neck
    HeadGear,    // hood, hat, visor, horns
    Shoulders,   // pauldrons
    Accessories, // cape, badge, spine ridges, tail
}

impl PartSlotTag {
    pub fn from_kind(kind: CartoonPartKind) -> Self {
        match kind {
            CartoonPartKind::Shadow => PartSlotTag::Shadow,
            CartoonPartKind::Body | CartoonPartKind::Belt => PartSlotTag::Body,
            CartoonPartKind::Head
            | CartoonPartKind::Hair
            | CartoonPartKind::LeftEye
            | CartoonPartKind::RightEye
            | CartoonPartKind::LeftEyebrow
            | CartoonPartKind::RightEyebrow
            | CartoonPartKind::Nose
            | CartoonPartKind::Mouth => PartSlotTag::Head,
            CartoonPartKind::Hood
            | CartoonPartKind::Hat
            | CartoonPartKind::Visor
            | CartoonPartKind::LeftHorn
            | CartoonPartKind::RightHorn => PartSlotTag::HeadGear,
            CartoonPartKind::LeftArm
            | CartoonPartKind::RightArm
            | CartoonPartKind::LeftHand
            | CartoonPartKind::RightHand => PartSlotTag::Arms,
            CartoonPartKind::LeftLeg
            | CartoonPartKind::RightLeg
            | CartoonPartKind::LeftFoot
            | CartoonPartKind::RightFoot
            | CartoonPartKind::LeftBoot
            | CartoonPartKind::RightBoot => PartSlotTag::Legs,
            CartoonPartKind::ShoulderPadLeft | CartoonPartKind::ShoulderPadRight => {
                PartSlotTag::Shoulders
            }
            CartoonPartKind::Cape
            | CartoonPartKind::StarBadge
            | CartoonPartKind::SpineRidge
            | CartoonPartKind::Tail => PartSlotTag::Accessories,
        }
    }
}

// ── Loadout types ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum CharacterPartStyle {
    #[default]
    HumanoidClothing,
    RobotMechanical,
}

/// Per-slot style selection. Attach to the character root entity.
/// Mutate a field to trigger `swap_character_parts` to rebuild that slot.
#[derive(Component, Clone, Default)]
pub struct CharacterLoadout {
    pub body: CharacterPartStyle,
    pub arms: CharacterPartStyle,
    pub legs: CharacterPartStyle,
    pub shoulders: CharacterPartStyle,
}

/// Cached visual parameters so `swap_character_parts` can re-spawn parts
/// without needing the original config.
#[derive(Component, Clone)]
pub struct CharacterVisualConfig {
    pub scale: f32,
    pub body_width: f32,
    pub hip_width: f32,
    pub arm_length: f32,
    pub leg_length: f32,
    pub hand_scale: f32,
    pub foot_scale: f32,
    pub outfit: Color,
    pub accent: Color,
    pub has_gloves: bool,
    pub has_boots: bool,
    pub visual_ground_lift: f32,
}

// ── Robot mechanical part spawners ─────────────────────────────────────────────
//
// All transforms include `y_lift` so swapped parts sit at the correct world
// height without needing a separate deferred world-command.

pub fn spawn_robot_body(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let bw = cfg.body_width;
    let hip_w = cfg.hip_width;

    let primary = robot_mat(materials, cfg.outfit);
    let plating = robot_mat(materials, darken(cfg.outfit, 0.70));
    let armor = robot_mat(materials, darken(cfg.accent, 0.78));
    let accent_glow = emissive_mat(materials, cfg.accent, 2.4);

    // Main chassis tube
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.28 * s, 0.70 * s)),
        primary.clone(),
        Transform::from_xyz(0.0, -0.10 * s + y_lift, 0.0).with_scale(Vec3::new(
            1.08 * bw,
            1.0,
            0.68,
        )),
        PartSlotTag::Body,
    );
    // Chest panel: protruding ellipsoid plate
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.5)),
        armor.clone(),
        Transform::from_xyz(0.0, 0.08 * s + y_lift, -0.260 * s).with_scale(Vec3::new(
            0.62 * s * bw,
            0.38 * s,
            0.095 * s,
        )),
        PartSlotTag::Body,
    );
    // Core reactor glow
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.068 * s)),
        accent_glow.clone(),
        Transform::from_xyz(0.0, 0.08 * s + y_lift, -0.295 * s)
            .with_scale(Vec3::new(0.80, 0.80, 0.40)),
        PartSlotTag::Body,
    );
    // Side vent strips
    for (x, rot_z) in [(-0.22_f32, -0.15_f32), (0.22_f32, 0.15_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Capsule3d::new(0.020 * s, 0.18 * s)),
            accent_glow.clone(),
            Transform::from_xyz(x * s * bw, -0.09 * s + y_lift, -0.285 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Body,
        );
    }
    // Hip guards: wide plating spheres
    for (x, rot_z) in [(-0.25_f32, -0.22_f32), (0.25_f32, 0.22_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.5)),
            plating.clone(),
            Transform::from_xyz(x * s * hip_w, -0.52 * s + y_lift, -0.015 * s)
                .with_scale(Vec3::new(0.26 * s, 0.14 * s, 0.32 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Body,
        );
    }
    // Waist ring
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.30 * s * bw.max(0.8), 0.045 * s)),
        plating.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, 0.0),
        PartSlotTag::Body,
    );
    // Belt buckle: emissive ellipsoid
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Belt,
        Mesh::from(Sphere::new(0.5)),
        accent_glow.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, -0.22 * s).with_scale(Vec3::new(
            0.18 * s,
            0.16 * s,
            0.06 * s,
        )),
        PartSlotTag::Body,
    );
}

pub fn spawn_robot_arms(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let bw = cfg.body_width;
    let al = cfg.arm_length;

    let primary = robot_mat(materials, cfg.outfit);
    let armor = robot_mat(materials, darken(cfg.accent, 0.78));
    let accent_glow = emissive_mat(materials, cfg.accent, 2.4);

    let arm_x = (0.34 + 0.075) * s * bw;
    for (kind_arm, kind_hand, x) in [
        (CartoonPartKind::LeftArm, CartoonPartKind::LeftHand, -arm_x),
        (CartoonPartKind::RightArm, CartoonPartKind::RightHand, arm_x),
    ] {
        // Shoulder ball joint
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Sphere::new(0.11 * s)),
            armor.clone(),
            Transform::from_xyz(x, 0.18 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Upper arm tube
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Capsule3d::new(0.072 * s, 0.35 * s * al)),
            primary.clone(),
            Transform::from_xyz(x, 0.02 * s - (al - 1.0) * 0.07 * s + y_lift, 0.0)
                .with_scale(Vec3::new(0.88, 1.0, 0.80)),
            PartSlotTag::Arms,
        );
        // Forearm tube (armor plating)
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Capsule3d::new(0.080 * s, 0.33 * s * al)),
            armor.clone(),
            Transform::from_xyz(x, -0.30 * s - (al - 1.0) * 0.17 * s + y_lift, -0.005 * s)
                .with_scale(Vec3::new(0.86, 1.0, 0.78)),
            PartSlotTag::Arms,
        );
        // Elbow joint sphere
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Sphere::new(0.095 * s)),
            primary.clone(),
            Transform::from_xyz(x, -0.14 * s - (al - 1.0) * 0.12 * s + y_lift, -0.008 * s),
            PartSlotTag::Arms,
        );
        // Forearm accent stripe
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Capsule3d::new(0.016 * s, 0.14 * s * al)),
            accent_glow.clone(),
            Transform::from_xyz(x, -0.30 * s - (al - 1.0) * 0.17 * s + y_lift, -0.080 * s)
                .with_scale(Vec3::new(0.5, 1.0, 0.4)),
            PartSlotTag::Arms,
        );
        // Hand: rounded fist
        spawn_part(
            commands,
            meshes,
            root,
            kind_hand,
            Mesh::from(Sphere::new(0.110 * s * cfg.hand_scale)),
            if cfg.has_gloves {
                armor.clone()
            } else {
                primary.clone()
            },
            Transform::from_xyz(x, -0.49 * s - (al - 1.0) * 0.28 * s + y_lift, 0.02 * s)
                .with_scale(Vec3::new(0.88, 1.10, 0.80)),
            PartSlotTag::Arms,
        );
    }
}

pub fn spawn_robot_legs(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let ll = cfg.leg_length;
    let hip_w = cfg.hip_width;

    let primary = robot_mat(materials, cfg.outfit);
    let armor = robot_mat(materials, darken(cfg.accent, 0.78));
    let accent_glow = emissive_mat(materials, cfg.accent, 2.4);

    let leg_x = 0.165 * s * hip_w;
    for (leg_k, foot_k, boot_k, x) in [
        (
            CartoonPartKind::LeftLeg,
            CartoonPartKind::LeftFoot,
            CartoonPartKind::LeftBoot,
            -leg_x,
        ),
        (
            CartoonPartKind::RightLeg,
            CartoonPartKind::RightFoot,
            CartoonPartKind::RightBoot,
            leg_x,
        ),
    ] {
        // Thigh tube
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Capsule3d::new(0.088 * s, 0.40 * s * ll)),
            primary.clone(),
            Transform::from_xyz(x, -0.65 * s - (ll - 1.0) * 0.13 * s + y_lift, 0.0)
                .with_scale(Vec3::new(0.84, 1.0, 0.76)),
            PartSlotTag::Legs,
        );
        // Knee joint sphere
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Sphere::new(0.105 * s)),
            armor.clone(),
            Transform::from_xyz(x, -0.85 * s - (ll - 1.0) * 0.20 * s + y_lift, -0.012 * s),
            PartSlotTag::Legs,
        );
        // Shin tube (armor)
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Capsule3d::new(0.082 * s, 0.37 * s * ll)),
            armor.clone(),
            Transform::from_xyz(x, -s - (ll - 1.0) * 0.26 * s + y_lift, -0.008 * s)
                .with_scale(Vec3::new(0.80, 1.0, 0.70)),
            PartSlotTag::Legs,
        );
        // Shin accent stripe
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Capsule3d::new(0.014 * s, 0.20 * s * ll)),
            accent_glow.clone(),
            Transform::from_xyz(x, -s - (ll - 1.0) * 0.26 * s + y_lift, -0.082 * s)
                .with_scale(Vec3::new(0.4, 1.0, 0.3)),
            PartSlotTag::Legs,
        );
        // Foot: horizontal Capsule3d
        spawn_part(
            commands,
            meshes,
            root,
            foot_k,
            Mesh::from(Capsule3d::new(0.092 * s, 0.14 * s * cfg.foot_scale)),
            armor.clone(),
            Transform::from_xyz(x, -1.23 * s - (ll - 1.0) * 0.31 * s + y_lift, -0.09 * s)
                .with_rotation(Quat::from_rotation_x(PI * 0.5))
                .with_scale(Vec3::new(0.84 * cfg.foot_scale, 1.0, 1.12)),
            PartSlotTag::Legs,
        );
        // Ankle thruster cylinder
        spawn_part(
            commands,
            meshes,
            root,
            boot_k,
            Mesh::from(Cylinder::new(0.095 * s * cfg.foot_scale, 0.055 * s)),
            primary.clone(),
            Transform::from_xyz(x, -1.06 * s - (ll - 1.0) * 0.22 * s + y_lift, -0.020 * s)
                .with_scale(Vec3::new(0.86, 1.0, 0.76)),
            PartSlotTag::Legs,
        );
        // Thruster glow ring
        spawn_part(
            commands,
            meshes,
            root,
            boot_k,
            Mesh::from(Cylinder::new(0.105 * s * cfg.foot_scale, 0.032 * s)),
            accent_glow.clone(),
            Transform::from_xyz(x, -1.01 * s - (ll - 1.0) * 0.22 * s + y_lift, -0.020 * s),
            PartSlotTag::Legs,
        );
    }
}

pub fn spawn_robot_shoulders(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let arm_x = (0.34 + 0.075) * s * cfg.body_width;

    let primary = robot_mat(materials, cfg.outfit);
    let armor = robot_mat(materials, darken(cfg.accent, 0.78));
    let accent_glow = emissive_mat(materials, cfg.accent, 2.4);

    for (kind, x, rot_z) in [
        (CartoonPartKind::ShoulderPadLeft, -arm_x, -0.15_f32),
        (CartoonPartKind::ShoulderPadRight, arm_x, 0.15_f32),
    ] {
        // Main dome pauldron
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Sphere::new(0.22 * s)),
            primary.clone(),
            Transform::from_xyz(x, 0.26 * s + y_lift, -0.010 * s)
                .with_scale(Vec3::new(1.0, 0.62, 0.90))
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Shoulders,
        );
        // Inner plate
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Sphere::new(0.5)),
            armor.clone(),
            Transform::from_xyz(x, 0.20 * s + y_lift, -0.060 * s)
                .with_scale(Vec3::new(0.36 * s, 0.10 * s, 0.26 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Shoulders,
        );
        // Accent ring at dome base
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Cylinder::new(0.195 * s, 0.036 * s)),
            accent_glow.clone(),
            Transform::from_xyz(x, 0.168 * s + y_lift, -0.018 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Shoulders,
        );
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn spawn_part(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    root: Entity,
    kind: CartoonPartKind,
    mesh: Mesh,
    material: Handle<StandardMaterial>,
    transform: Transform,
    slot: PartSlotTag,
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
            slot,
        ))
        .id();
    commands.entity(root).add_child(entity);
}

fn robot_mat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    let lin = color.to_linear();
    materials.add(StandardMaterial {
        base_color: color,
        emissive: LinearRgba::new(lin.red * 0.08, lin.green * 0.08, lin.blue * 0.08, 1.0),
        perceptual_roughness: 0.40,
        metallic: 0.65,
        reflectance: 0.50,
        ..default()
    })
}

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
        perceptual_roughness: 0.38,
        ..default()
    })
}

fn darken(color: Color, factor: f32) -> Color {
    let lin = color.to_linear();
    Color::linear_rgb(lin.red * factor, lin.green * factor, lin.blue * factor)
}
