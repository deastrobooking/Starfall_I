use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

use crate::components::character::{default_joint_for_part, CartoonPart, CartoonPartKind};
use crate::rendering::PbrBundle;

// ── Slot identification ───────────────────────────────────────────────────────

/// Major visual group a CartoonPart belongs to.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PartSlotTag {
    Shadow,
    Body,
    Arms,
    Legs,
    Head,
    HeadGear,
    Shoulders,
    Accessories,
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

// ── Per-slot preset enums ─────────────────────────────────────────────────────

/// Torso/chest preset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum BodyPreset {
    /// Standard mech — reactor core, side vents, hip guards.
    #[default]
    StandardMech,
    /// Siege Armor — thick wide chassis, large dome chest plate, reinforced hips.
    HeavyPlate,
    /// Ranger Vest — slim torso, diagonal chest stripe, minimal hip ridge.
    ScoutVest,
    /// Void Weave — emissive edge strips, glowing chest grid, dark body.
    VoidArmor,
}

impl BodyPreset {
    pub fn label(self) -> &'static str {
        match self {
            BodyPreset::StandardMech => "Standard Mech",
            BodyPreset::HeavyPlate => "Siege Armor",
            BodyPreset::ScoutVest => "Ranger Vest",
            BodyPreset::VoidArmor => "Void Weave",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            BodyPreset::StandardMech => BodyPreset::HeavyPlate,
            BodyPreset::HeavyPlate => BodyPreset::ScoutVest,
            BodyPreset::ScoutVest => BodyPreset::VoidArmor,
            BodyPreset::VoidArmor => BodyPreset::StandardMech,
        }
    }
}

/// Arm/hand preset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum ArmPreset {
    /// Standard mech — shoulder ball, forearm armor, accent stripe.
    #[default]
    MechArms,
    /// Titan Arms — oversized tubes, knuckle guard fist, elbow spike.
    HeavyArms,
    /// Ranger Arms — thin tubes, no shoulder ball, light hand.
    ScoutArms,
    /// Predator Claws — standard upper arm, tri-claw finger spread.
    ClawArms,
}

impl ArmPreset {
    pub fn label(self) -> &'static str {
        match self {
            ArmPreset::MechArms => "Mech Arms",
            ArmPreset::HeavyArms => "Titan Arms",
            ArmPreset::ScoutArms => "Ranger Arms",
            ArmPreset::ClawArms => "Predator Claws",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            ArmPreset::MechArms => ArmPreset::HeavyArms,
            ArmPreset::HeavyArms => ArmPreset::ScoutArms,
            ArmPreset::ScoutArms => ArmPreset::ClawArms,
            ArmPreset::ClawArms => ArmPreset::MechArms,
        }
    }
}

/// Leg/boot preset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum LegPreset {
    /// Standard mech — thigh, knee, shin, ankle thrusters.
    #[default]
    MechLegs,
    /// Siege Legs — wide plates, large knee guard, heavy boot block.
    HeavyLegs,
    /// Sprint Legs — narrow tubes, no knee sphere, light foot.
    ScoutLegs,
    /// Rocket Legs — standard frame + rear shin nozzles.
    JetLegs,
}

impl LegPreset {
    pub fn label(self) -> &'static str {
        match self {
            LegPreset::MechLegs => "Mech Legs",
            LegPreset::HeavyLegs => "Siege Legs",
            LegPreset::ScoutLegs => "Sprint Legs",
            LegPreset::JetLegs => "Rocket Legs",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            LegPreset::MechLegs => LegPreset::HeavyLegs,
            LegPreset::HeavyLegs => LegPreset::ScoutLegs,
            LegPreset::ScoutLegs => LegPreset::JetLegs,
            LegPreset::JetLegs => LegPreset::MechLegs,
        }
    }
}

/// Shoulder pad preset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum ShoulderPreset {
    /// Dome pauldrons — classic domed shoulder caps with accent ring.
    #[default]
    DomePauldrons,
    /// Spiked pauldrons — dome base with upward spike.
    SpikedPauldrons,
    /// Plate spaulders — wide flat disc with edge strip.
    PlateEpaulettes,
    /// No shoulder pieces.
    None,
}

impl ShoulderPreset {
    pub fn label(self) -> &'static str {
        match self {
            ShoulderPreset::DomePauldrons => "Dome Pauldrons",
            ShoulderPreset::SpikedPauldrons => "Spiked Pauldrons",
            ShoulderPreset::PlateEpaulettes => "Plate Spaulders",
            ShoulderPreset::None => "None",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            ShoulderPreset::DomePauldrons => ShoulderPreset::SpikedPauldrons,
            ShoulderPreset::SpikedPauldrons => ShoulderPreset::PlateEpaulettes,
            ShoulderPreset::PlateEpaulettes => ShoulderPreset::None,
            ShoulderPreset::None => ShoulderPreset::DomePauldrons,
        }
    }
}

// ── Loadout types ─────────────────────────────────────────────────────────────

/// Per-slot preset selection. Mutate to trigger `swap_character_parts`.
#[derive(Component, Clone, Default)]
pub struct CharacterLoadout {
    pub body: BodyPreset,
    pub arms: ArmPreset,
    pub legs: LegPreset,
    pub shoulders: ShoulderPreset,
}

/// Cached visual parameters for part re-spawning.
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

// ── Public dispatch functions ─────────────────────────────────────────────────

pub fn spawn_body(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
    preset: BodyPreset,
) {
    match preset {
        BodyPreset::StandardMech => spawn_body_standard_mech(commands, meshes, materials, root, cfg, y_lift),
        BodyPreset::HeavyPlate => spawn_body_heavy_plate(commands, meshes, materials, root, cfg, y_lift),
        BodyPreset::ScoutVest => spawn_body_scout_vest(commands, meshes, materials, root, cfg, y_lift),
        BodyPreset::VoidArmor => spawn_body_void_armor(commands, meshes, materials, root, cfg, y_lift),
    }
}

pub fn spawn_arms(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
    preset: ArmPreset,
) {
    match preset {
        ArmPreset::MechArms => spawn_arms_mech(commands, meshes, materials, root, cfg, y_lift),
        ArmPreset::HeavyArms => spawn_arms_heavy(commands, meshes, materials, root, cfg, y_lift),
        ArmPreset::ScoutArms => spawn_arms_scout(commands, meshes, materials, root, cfg, y_lift),
        ArmPreset::ClawArms => spawn_arms_claw(commands, meshes, materials, root, cfg, y_lift),
    }
}

pub fn spawn_legs(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
    preset: LegPreset,
) {
    match preset {
        LegPreset::MechLegs => spawn_legs_mech(commands, meshes, materials, root, cfg, y_lift),
        LegPreset::HeavyLegs => spawn_legs_heavy(commands, meshes, materials, root, cfg, y_lift),
        LegPreset::ScoutLegs => spawn_legs_scout(commands, meshes, materials, root, cfg, y_lift),
        LegPreset::JetLegs => spawn_legs_jet(commands, meshes, materials, root, cfg, y_lift),
    }
}

pub fn spawn_shoulders(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
    preset: ShoulderPreset,
) {
    match preset {
        ShoulderPreset::DomePauldrons => spawn_shoulders_dome(commands, meshes, materials, root, cfg, y_lift),
        ShoulderPreset::SpikedPauldrons => spawn_shoulders_spiked(commands, meshes, materials, root, cfg, y_lift),
        ShoulderPreset::PlateEpaulettes => spawn_shoulders_plate(commands, meshes, materials, root, cfg, y_lift),
        ShoulderPreset::None => {}
    }
}

// ── Body presets ──────────────────────────────────────────────────────────────

fn spawn_body_standard_mech(
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

    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.28 * s, 0.70 * s)), primary.clone(),
        Transform::from_xyz(0.0, -0.10 * s + y_lift, 0.0)
            .with_scale(Vec3::new(1.08 * bw, 1.0, 0.68)), PartSlotTag::Body);
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.5)), armor.clone(),
        Transform::from_xyz(0.0, 0.08 * s + y_lift, -0.260 * s)
            .with_scale(Vec3::new(0.62 * s * bw, 0.38 * s, 0.095 * s)), PartSlotTag::Body);
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.068 * s)), accent_glow.clone(),
        Transform::from_xyz(0.0, 0.08 * s + y_lift, -0.295 * s)
            .with_scale(Vec3::new(0.80, 0.80, 0.40)), PartSlotTag::Body);
    for (x, rot_z) in [(-0.22_f32, -0.15_f32), (0.22_f32, 0.15_f32)] {
        spawn_part(commands, meshes, root, CartoonPartKind::Body,
            Mesh::from(Capsule3d::new(0.020 * s, 0.18 * s)), accent_glow.clone(),
            Transform::from_xyz(x * s * bw, -0.09 * s + y_lift, -0.285 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Body);
    }
    for (x, rot_z) in [(-0.25_f32, -0.22_f32), (0.25_f32, 0.22_f32)] {
        spawn_part(commands, meshes, root, CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.5)), plating.clone(),
            Transform::from_xyz(x * s * hip_w, -0.52 * s + y_lift, -0.015 * s)
                .with_scale(Vec3::new(0.26 * s, 0.14 * s, 0.32 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Body);
    }
    spawn_part(commands, meshes, root, CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.30 * s * bw.max(0.8), 0.045 * s)), plating.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, 0.0), PartSlotTag::Body);
    spawn_part(commands, meshes, root, CartoonPartKind::Belt,
        Mesh::from(Sphere::new(0.5)), accent_glow.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, -0.22 * s)
            .with_scale(Vec3::new(0.18 * s, 0.16 * s, 0.06 * s)), PartSlotTag::Body);
}

fn spawn_body_heavy_plate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let bw = cfg.body_width * 1.18;
    let hip_w = cfg.hip_width * 1.22;

    let primary = robot_mat(materials, darken(cfg.outfit, 0.85));
    let plating = robot_mat(materials, darken(cfg.outfit, 0.60));
    let armor = robot_mat(materials, darken(cfg.accent, 0.70));

    // Wide barrel torso
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.30 * s, 0.72 * s)), primary.clone(),
        Transform::from_xyz(0.0, -0.08 * s + y_lift, 0.0)
            .with_scale(Vec3::new(1.22 * bw, 1.0, 0.82)), PartSlotTag::Body);
    // Large dome chest plate
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.5)), armor.clone(),
        Transform::from_xyz(0.0, 0.12 * s + y_lift, -0.24 * s)
            .with_scale(Vec3::new(0.78 * s * bw, 0.50 * s, 0.14 * s)), PartSlotTag::Body);
    // Collar reinforcement ring
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Cylinder::new(0.22 * s * bw.max(0.9), 0.060 * s)), plating.clone(),
        Transform::from_xyz(0.0, 0.38 * s + y_lift, 0.0), PartSlotTag::Body);
    // Wide angular hip guards (4 pieces)
    for (x, rot_z) in [(-0.30_f32, -0.28_f32), (0.30_f32, 0.28_f32)] {
        spawn_part(commands, meshes, root, CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.5)), plating.clone(),
            Transform::from_xyz(x * s * hip_w, -0.50 * s + y_lift, -0.010 * s)
                .with_scale(Vec3::new(0.32 * s, 0.18 * s, 0.36 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Body);
    }
    // Thick waist plate
    spawn_part(commands, meshes, root, CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.36 * s * bw.max(0.9), 0.075 * s)), plating.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, 0.0), PartSlotTag::Body);
    // Flat belt buckle plate
    spawn_part(commands, meshes, root, CartoonPartKind::Belt,
        Mesh::from(Sphere::new(0.5)), armor.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, -0.26 * s)
            .with_scale(Vec3::new(0.24 * s, 0.18 * s, 0.05 * s)), PartSlotTag::Body);
}

fn spawn_body_scout_vest(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let bw = cfg.body_width * 0.88;
    let hip_w = cfg.hip_width;

    let primary = robot_mat(materials, cfg.outfit);
    let stripe = robot_mat(materials, darken(cfg.accent, 0.88));
    let accent_glow = emissive_mat(materials, cfg.accent, 1.8);

    // Slim torso
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.24 * s, 0.68 * s)), primary.clone(),
        Transform::from_xyz(0.0, -0.10 * s + y_lift, 0.0)
            .with_scale(Vec3::new(0.90 * bw, 1.0, 0.60)), PartSlotTag::Body);
    // Diagonal chest stripe (left shoulder to right hip)
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.022 * s, 0.55 * s)), stripe.clone(),
        Transform::from_xyz(-0.05 * s * bw, 0.0 * s + y_lift, -0.24 * s)
            .with_rotation(Quat::from_rotation_z(0.55)), PartSlotTag::Body);
    // Small badge / sensor node
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.042 * s)), accent_glow.clone(),
        Transform::from_xyz(0.12 * s * bw, 0.15 * s + y_lift, -0.26 * s), PartSlotTag::Body);
    // Minimal hip ridge (thin band)
    for (x, rot_z) in [(-0.18_f32, -0.12_f32), (0.18_f32, 0.12_f32)] {
        spawn_part(commands, meshes, root, CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.5)), stripe.clone(),
            Transform::from_xyz(x * s * hip_w, -0.50 * s + y_lift, -0.008 * s)
                .with_scale(Vec3::new(0.16 * s, 0.08 * s, 0.20 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Body);
    }
    // Thin belt
    spawn_part(commands, meshes, root, CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.24 * s * bw.max(0.7), 0.028 * s)), stripe.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, 0.0), PartSlotTag::Body);
}

fn spawn_body_void_armor(
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

    let dark = robot_mat(materials, darken(cfg.outfit, 0.30));
    let accent_glow = emissive_mat(materials, cfg.accent, 3.2);
    let glow_dim = emissive_mat(materials, cfg.accent, 1.6);

    // Dark slender torso
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.26 * s, 0.72 * s)), dark.clone(),
        Transform::from_xyz(0.0, -0.10 * s + y_lift, 0.0)
            .with_scale(Vec3::new(1.0 * bw, 1.0, 0.65)), PartSlotTag::Body);
    // Four vertical edge-glow strips
    for (x, z) in [(-0.27_f32, -0.14_f32), (0.27_f32, -0.14_f32),
                   (-0.18_f32, -0.27_f32), (0.18_f32, -0.27_f32)] {
        spawn_part(commands, meshes, root, CartoonPartKind::Body,
            Mesh::from(Capsule3d::new(0.014 * s, 0.60 * s)), accent_glow.clone(),
            Transform::from_xyz(x * s * bw, -0.10 * s + y_lift, z * s), PartSlotTag::Body);
    }
    // Hex chest grid (6 small glowing nodes)
    for (nx, ny) in [(-0.12_f32, 0.14_f32), (0.0_f32, 0.18_f32),
                     (0.12_f32, 0.14_f32), (-0.12_f32, -0.02_f32),
                     (0.0_f32, 0.02_f32), (0.12_f32, -0.02_f32)] {
        spawn_part(commands, meshes, root, CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.026 * s)), accent_glow.clone(),
            Transform::from_xyz(nx * s * bw, ny * s + y_lift, -0.285 * s), PartSlotTag::Body);
    }
    // Large core reactor (bright center)
    spawn_part(commands, meshes, root, CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.080 * s)), accent_glow.clone(),
        Transform::from_xyz(0.0, 0.08 * s + y_lift, -0.295 * s)
            .with_scale(Vec3::new(1.0, 1.0, 0.45)), PartSlotTag::Body);
    // Hip glow nodes
    for x in [-0.22_f32 * s * hip_w, 0.22_f32 * s * hip_w] {
        spawn_part(commands, meshes, root, CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.048 * s)), glow_dim.clone(),
            Transform::from_xyz(x, -0.50 * s + y_lift, -0.20 * s), PartSlotTag::Body);
    }
    // Thin void belt
    spawn_part(commands, meshes, root, CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.25 * s * bw.max(0.8), 0.030 * s)), dark.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, 0.0), PartSlotTag::Body);
    spawn_part(commands, meshes, root, CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.25 * s * bw.max(0.8), 0.015 * s)), accent_glow.clone(),
        Transform::from_xyz(0.0, -0.375 * s + y_lift, 0.0), PartSlotTag::Body);
}

// ── Arm presets ───────────────────────────────────────────────────────────────

fn spawn_arms_mech(
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
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Sphere::new(0.11 * s)), armor.clone(),
            Transform::from_xyz(x, 0.18 * s + y_lift, 0.0), PartSlotTag::Arms);
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.072 * s, 0.35 * s * al)), primary.clone(),
            Transform::from_xyz(x, 0.02 * s - (al - 1.0) * 0.07 * s + y_lift, 0.0)
                .with_scale(Vec3::new(0.88, 1.0, 0.80)), PartSlotTag::Arms);
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.080 * s, 0.33 * s * al)), armor.clone(),
            Transform::from_xyz(x, -0.30 * s - (al - 1.0) * 0.17 * s + y_lift, -0.005 * s)
                .with_scale(Vec3::new(0.86, 1.0, 0.78)), PartSlotTag::Arms);
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Sphere::new(0.095 * s)), primary.clone(),
            Transform::from_xyz(x, -0.14 * s - (al - 1.0) * 0.12 * s + y_lift, -0.008 * s),
            PartSlotTag::Arms);
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.016 * s, 0.14 * s * al)), accent_glow.clone(),
            Transform::from_xyz(x, -0.30 * s - (al - 1.0) * 0.17 * s + y_lift, -0.080 * s)
                .with_scale(Vec3::new(0.5, 1.0, 0.4)), PartSlotTag::Arms);
        spawn_part(commands, meshes, root, kind_hand,
            Mesh::from(Sphere::new(0.110 * s * cfg.hand_scale)),
            if cfg.has_gloves { armor.clone() } else { primary.clone() },
            Transform::from_xyz(x, -0.49 * s - (al - 1.0) * 0.28 * s + y_lift, 0.02 * s)
                .with_scale(Vec3::new(0.88, 1.10, 0.80)), PartSlotTag::Arms);
    }
}

fn spawn_arms_heavy(
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
    let armor = robot_mat(materials, darken(cfg.accent, 0.72));
    let dark = robot_mat(materials, darken(cfg.outfit, 0.65));

    let arm_x = (0.36 + 0.09) * s * bw;
    for (kind_arm, kind_hand, x) in [
        (CartoonPartKind::LeftArm, CartoonPartKind::LeftHand, -arm_x),
        (CartoonPartKind::RightArm, CartoonPartKind::RightHand, arm_x),
    ] {
        // Large shoulder ball
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Sphere::new(0.145 * s)), armor.clone(),
            Transform::from_xyz(x, 0.22 * s + y_lift, 0.0), PartSlotTag::Arms);
        // Wide upper arm
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.095 * s, 0.35 * s * al)), primary.clone(),
            Transform::from_xyz(x, 0.02 * s - (al - 1.0) * 0.07 * s + y_lift, 0.0)
                .with_scale(Vec3::new(1.10, 1.0, 0.92)), PartSlotTag::Arms);
        // Elbow spike
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.036 * s, 0.12 * s)), dark.clone(),
            Transform::from_xyz(x, -0.14 * s - (al - 1.0) * 0.12 * s + y_lift, 0.085 * s)
                .with_rotation(Quat::from_rotation_x(-0.7)), PartSlotTag::Arms);
        // Wide armored forearm
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.100 * s, 0.34 * s * al)), armor.clone(),
            Transform::from_xyz(x, -0.30 * s - (al - 1.0) * 0.17 * s + y_lift, -0.005 * s)
                .with_scale(Vec3::new(1.08, 1.0, 0.90)), PartSlotTag::Arms);
        // Knuckle gauntlet fist
        spawn_part(commands, meshes, root, kind_hand,
            Mesh::from(Sphere::new(0.145 * s * cfg.hand_scale)), armor.clone(),
            Transform::from_xyz(x, -0.52 * s - (al - 1.0) * 0.28 * s + y_lift, 0.02 * s)
                .with_scale(Vec3::new(1.05, 0.88, 1.10)), PartSlotTag::Arms);
        // Knuckle ridge bumps
        for dy in [-0.02_f32, 0.02_f32] {
            spawn_part(commands, meshes, root, kind_hand,
                Mesh::from(Sphere::new(0.038 * s)), dark.clone(),
                Transform::from_xyz(x + dy * 1.8 * s, -0.52 * s - (al - 1.0) * 0.28 * s + y_lift, -0.10 * s),
                PartSlotTag::Arms);
        }
    }
}

fn spawn_arms_scout(
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
    let stripe = robot_mat(materials, darken(cfg.accent, 0.85));

    let arm_x = (0.32 + 0.07) * s * bw;
    for (kind_arm, kind_hand, x) in [
        (CartoonPartKind::LeftArm, CartoonPartKind::LeftHand, -arm_x),
        (CartoonPartKind::RightArm, CartoonPartKind::RightHand, arm_x),
    ] {
        // Thin upper arm
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.055 * s, 0.36 * s * al)), primary.clone(),
            Transform::from_xyz(x, 0.02 * s - (al - 1.0) * 0.07 * s + y_lift, 0.0)
                .with_scale(Vec3::new(0.80, 1.0, 0.72)), PartSlotTag::Arms);
        // Thin forearm
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.060 * s, 0.34 * s * al)), stripe.clone(),
            Transform::from_xyz(x, -0.30 * s - (al - 1.0) * 0.17 * s + y_lift, -0.003 * s)
                .with_scale(Vec3::new(0.78, 1.0, 0.70)), PartSlotTag::Arms);
        // Light hand
        spawn_part(commands, meshes, root, kind_hand,
            Mesh::from(Sphere::new(0.082 * s * cfg.hand_scale)), primary.clone(),
            Transform::from_xyz(x, -0.49 * s - (al - 1.0) * 0.28 * s + y_lift, 0.01 * s)
                .with_scale(Vec3::new(0.85, 1.05, 0.78)), PartSlotTag::Arms);
    }
}

fn spawn_arms_claw(
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
    let dark = robot_mat(materials, darken(cfg.outfit, 0.55));
    let accent_glow = emissive_mat(materials, cfg.accent, 2.8);

    let arm_x = (0.34 + 0.075) * s * bw;
    for (kind_arm, kind_hand, x, sign) in [
        (CartoonPartKind::LeftArm, CartoonPartKind::LeftHand, -arm_x, -1.0_f32),
        (CartoonPartKind::RightArm, CartoonPartKind::RightHand, arm_x, 1.0_f32),
    ] {
        // Shoulder joint
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Sphere::new(0.11 * s)), dark.clone(),
            Transform::from_xyz(x, 0.18 * s + y_lift, 0.0), PartSlotTag::Arms);
        // Upper arm with ridge
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.075 * s, 0.35 * s * al)), primary.clone(),
            Transform::from_xyz(x, 0.02 * s - (al - 1.0) * 0.07 * s + y_lift, 0.0)
                .with_scale(Vec3::new(0.88, 1.0, 0.80)), PartSlotTag::Arms);
        // Forearm with glow vein
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.070 * s, 0.33 * s * al)), dark.clone(),
            Transform::from_xyz(x, -0.30 * s - (al - 1.0) * 0.17 * s + y_lift, -0.004 * s)
                .with_scale(Vec3::new(0.84, 1.0, 0.76)), PartSlotTag::Arms);
        spawn_part(commands, meshes, root, kind_arm,
            Mesh::from(Capsule3d::new(0.012 * s, 0.28 * s * al)), accent_glow.clone(),
            Transform::from_xyz(x, -0.30 * s - (al - 1.0) * 0.17 * s + y_lift, -0.072 * s)
                .with_scale(Vec3::new(0.4, 1.0, 0.35)), PartSlotTag::Arms);
        // Wrist node
        spawn_part(commands, meshes, root, kind_hand,
            Mesh::from(Sphere::new(0.075 * s)), dark.clone(),
            Transform::from_xyz(x, -0.49 * s - (al - 1.0) * 0.28 * s + y_lift, 0.0),
            PartSlotTag::Arms);
        // Three elongated claw prongs
        let hand_y = -0.50 * s - (al - 1.0) * 0.28 * s + y_lift;
        for (angle_off, prong_len) in [(-0.38_f32, 0.22_f32), (0.0_f32, 0.26_f32), (0.38_f32, 0.22_f32)] {
            spawn_part(commands, meshes, root, kind_hand,
                Mesh::from(Capsule3d::new(0.020 * s, prong_len * s)),
                dark.clone(),
                Transform::from_xyz(
                    x + sign * angle_off * 0.12 * s,
                    hand_y - 0.10 * s,
                    -0.04 * s,
                ).with_rotation(Quat::from_rotation_x(0.18) * Quat::from_rotation_z(sign * angle_off)),
                PartSlotTag::Arms);
        }
    }
}

// ── Leg presets ───────────────────────────────────────────────────────────────

fn spawn_legs_mech(
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
        (CartoonPartKind::LeftLeg, CartoonPartKind::LeftFoot, CartoonPartKind::LeftBoot, -leg_x),
        (CartoonPartKind::RightLeg, CartoonPartKind::RightFoot, CartoonPartKind::RightBoot, leg_x),
    ] {
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Capsule3d::new(0.088 * s, 0.40 * s * ll)), primary.clone(),
            Transform::from_xyz(x, -0.65 * s - (ll - 1.0) * 0.13 * s + y_lift, 0.0)
                .with_scale(Vec3::new(0.84, 1.0, 0.76)), PartSlotTag::Legs);
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Sphere::new(0.105 * s)), armor.clone(),
            Transform::from_xyz(x, -0.85 * s - (ll - 1.0) * 0.20 * s + y_lift, -0.012 * s),
            PartSlotTag::Legs);
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Capsule3d::new(0.082 * s, 0.37 * s * ll)), armor.clone(),
            Transform::from_xyz(x, -s - (ll - 1.0) * 0.26 * s + y_lift, -0.008 * s)
                .with_scale(Vec3::new(0.80, 1.0, 0.70)), PartSlotTag::Legs);
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Capsule3d::new(0.014 * s, 0.20 * s * ll)), accent_glow.clone(),
            Transform::from_xyz(x, -s - (ll - 1.0) * 0.26 * s + y_lift, -0.082 * s)
                .with_scale(Vec3::new(0.4, 1.0, 0.3)), PartSlotTag::Legs);
        spawn_part(commands, meshes, root, foot_k,
            Mesh::from(Capsule3d::new(0.092 * s, 0.14 * s * cfg.foot_scale)), armor.clone(),
            Transform::from_xyz(x, -1.23 * s - (ll - 1.0) * 0.31 * s + y_lift, -0.09 * s)
                .with_rotation(Quat::from_rotation_x(PI * 0.5))
                .with_scale(Vec3::new(0.84 * cfg.foot_scale, 1.0, 1.12)), PartSlotTag::Legs);
        spawn_part(commands, meshes, root, boot_k,
            Mesh::from(Cylinder::new(0.095 * s * cfg.foot_scale, 0.055 * s)), primary.clone(),
            Transform::from_xyz(x, -1.06 * s - (ll - 1.0) * 0.22 * s + y_lift, -0.020 * s)
                .with_scale(Vec3::new(0.86, 1.0, 0.76)), PartSlotTag::Legs);
        spawn_part(commands, meshes, root, boot_k,
            Mesh::from(Cylinder::new(0.105 * s * cfg.foot_scale, 0.032 * s)), accent_glow.clone(),
            Transform::from_xyz(x, -1.01 * s - (ll - 1.0) * 0.22 * s + y_lift, -0.020 * s),
            PartSlotTag::Legs);
    }
}

fn spawn_legs_heavy(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let ll = cfg.leg_length;
    let hip_w = cfg.hip_width * 1.12;

    let primary = robot_mat(materials, darken(cfg.outfit, 0.85));
    let armor = robot_mat(materials, darken(cfg.accent, 0.70));
    let dark = robot_mat(materials, darken(cfg.outfit, 0.60));

    let leg_x = 0.180 * s * hip_w;
    for (leg_k, foot_k, boot_k, x) in [
        (CartoonPartKind::LeftLeg, CartoonPartKind::LeftFoot, CartoonPartKind::LeftBoot, -leg_x),
        (CartoonPartKind::RightLeg, CartoonPartKind::RightFoot, CartoonPartKind::RightBoot, leg_x),
    ] {
        // Wide thigh plate
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Capsule3d::new(0.108 * s, 0.38 * s * ll)), primary.clone(),
            Transform::from_xyz(x, -0.63 * s - (ll - 1.0) * 0.13 * s + y_lift, 0.0)
                .with_scale(Vec3::new(1.10, 1.0, 0.94)), PartSlotTag::Legs);
        // Large knee guard
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Sphere::new(0.138 * s)), armor.clone(),
            Transform::from_xyz(x, -0.86 * s - (ll - 1.0) * 0.20 * s + y_lift, -0.022 * s),
            PartSlotTag::Legs);
        // Wide armored shin block
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Capsule3d::new(0.098 * s, 0.38 * s * ll)), armor.clone(),
            Transform::from_xyz(x, -s - (ll - 1.0) * 0.26 * s + y_lift, -0.010 * s)
                .with_scale(Vec3::new(1.04, 1.0, 0.88)), PartSlotTag::Legs);
        // Heavy flat boot plate
        spawn_part(commands, meshes, root, foot_k,
            Mesh::from(Capsule3d::new(0.110 * s, 0.18 * s * cfg.foot_scale)), dark.clone(),
            Transform::from_xyz(x, -1.24 * s - (ll - 1.0) * 0.31 * s + y_lift, -0.10 * s)
                .with_rotation(Quat::from_rotation_x(PI * 0.5))
                .with_scale(Vec3::new(1.05 * cfg.foot_scale, 1.0, 1.20)), PartSlotTag::Legs);
        // Boot reinforcement cylinder
        spawn_part(commands, meshes, root, boot_k,
            Mesh::from(Cylinder::new(0.112 * s * cfg.foot_scale, 0.088 * s)), primary.clone(),
            Transform::from_xyz(x, -1.06 * s - (ll - 1.0) * 0.22 * s + y_lift, -0.025 * s)
                .with_scale(Vec3::new(1.0, 1.0, 0.86)), PartSlotTag::Legs);
    }
}

fn spawn_legs_scout(
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
    let stripe = robot_mat(materials, darken(cfg.accent, 0.88));

    let leg_x = 0.158 * s * hip_w;
    for (leg_k, foot_k, _, x) in [
        (CartoonPartKind::LeftLeg, CartoonPartKind::LeftFoot, CartoonPartKind::LeftBoot, -leg_x),
        (CartoonPartKind::RightLeg, CartoonPartKind::RightFoot, CartoonPartKind::RightBoot, leg_x),
    ] {
        // Slender thigh
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Capsule3d::new(0.065 * s, 0.42 * s * ll)), primary.clone(),
            Transform::from_xyz(x, -0.65 * s - (ll - 1.0) * 0.14 * s + y_lift, 0.0)
                .with_scale(Vec3::new(0.76, 1.0, 0.68)), PartSlotTag::Legs);
        // Slim shin
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Capsule3d::new(0.060 * s, 0.38 * s * ll)), stripe.clone(),
            Transform::from_xyz(x, -1.02 * s - (ll - 1.0) * 0.26 * s + y_lift, -0.005 * s)
                .with_scale(Vec3::new(0.72, 1.0, 0.64)), PartSlotTag::Legs);
        // Light foot
        spawn_part(commands, meshes, root, foot_k,
            Mesh::from(Capsule3d::new(0.072 * s, 0.12 * s * cfg.foot_scale)), primary.clone(),
            Transform::from_xyz(x, -1.25 * s - (ll - 1.0) * 0.31 * s + y_lift, -0.07 * s)
                .with_rotation(Quat::from_rotation_x(PI * 0.5))
                .with_scale(Vec3::new(0.78 * cfg.foot_scale, 1.0, 1.05)), PartSlotTag::Legs);
    }
}

fn spawn_legs_jet(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    // Standard mech frame + rear shin nozzles + exhaust glow
    spawn_legs_mech(commands, meshes, materials, root, cfg, y_lift);

    let s = cfg.scale;
    let ll = cfg.leg_length;
    let hip_w = cfg.hip_width;
    let accent_glow = emissive_mat(materials, cfg.accent, 3.0);
    let dark = robot_mat(materials, darken(cfg.outfit, 0.50));

    let leg_x = 0.165 * s * hip_w;
    for (leg_k, x) in [
        (CartoonPartKind::LeftLeg, -leg_x),
        (CartoonPartKind::RightLeg, leg_x),
    ] {
        // Rear nozzle housing
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Cylinder::new(0.075 * s, 0.10 * s)), dark.clone(),
            Transform::from_xyz(x, -1.04 * s - (ll - 1.0) * 0.26 * s + y_lift, 0.088 * s)
                .with_rotation(Quat::from_rotation_x(0.22)), PartSlotTag::Legs);
        // Nozzle exhaust glow
        spawn_part(commands, meshes, root, leg_k,
            Mesh::from(Cylinder::new(0.068 * s, 0.055 * s)), accent_glow.clone(),
            Transform::from_xyz(x, -1.02 * s - (ll - 1.0) * 0.26 * s + y_lift, 0.096 * s)
                .with_rotation(Quat::from_rotation_x(0.22)), PartSlotTag::Legs);
    }
}

// ── Shoulder presets ──────────────────────────────────────────────────────────

fn spawn_shoulders_dome(
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
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Sphere::new(0.22 * s)), primary.clone(),
            Transform::from_xyz(x, 0.26 * s + y_lift, -0.010 * s)
                .with_scale(Vec3::new(1.0, 0.62, 0.90))
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Shoulders);
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Sphere::new(0.5)), armor.clone(),
            Transform::from_xyz(x, 0.20 * s + y_lift, -0.060 * s)
                .with_scale(Vec3::new(0.36 * s, 0.10 * s, 0.26 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Shoulders);
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Cylinder::new(0.195 * s, 0.036 * s)), accent_glow.clone(),
            Transform::from_xyz(x, 0.168 * s + y_lift, -0.018 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Shoulders);
    }
}

fn spawn_shoulders_spiked(
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
    let armor = robot_mat(materials, darken(cfg.accent, 0.72));
    let dark = robot_mat(materials, darken(cfg.outfit, 0.55));

    for (kind, x, rot_z) in [
        (CartoonPartKind::ShoulderPadLeft, -arm_x, -0.15_f32),
        (CartoonPartKind::ShoulderPadRight, arm_x, 0.15_f32),
    ] {
        // Dome base
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Sphere::new(0.22 * s)), primary.clone(),
            Transform::from_xyz(x, 0.26 * s + y_lift, -0.010 * s)
                .with_scale(Vec3::new(1.0, 0.65, 0.88))
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Shoulders);
        // Inner plate
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Sphere::new(0.5)), armor.clone(),
            Transform::from_xyz(x, 0.20 * s + y_lift, -0.055 * s)
                .with_scale(Vec3::new(0.34 * s, 0.10 * s, 0.24 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Shoulders);
        // Upward spike
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Capsule3d::new(0.040 * s, 0.20 * s)), dark.clone(),
            Transform::from_xyz(x, 0.48 * s + y_lift, -0.015 * s)
                .with_rotation(Quat::from_rotation_z(rot_z * 0.5)), PartSlotTag::Shoulders);
        // Spike tip
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Sphere::new(0.028 * s)), armor.clone(),
            Transform::from_xyz(x, 0.62 * s + y_lift, -0.012 * s), PartSlotTag::Shoulders);
    }
}

fn spawn_shoulders_plate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let arm_x = (0.34 + 0.08) * s * cfg.body_width;

    let primary = robot_mat(materials, cfg.outfit);
    let edge = robot_mat(materials, darken(cfg.accent, 0.80));
    let accent_glow = emissive_mat(materials, cfg.accent, 1.8);

    for (kind, x, rot_z) in [
        (CartoonPartKind::ShoulderPadLeft, -arm_x, -0.08_f32),
        (CartoonPartKind::ShoulderPadRight, arm_x, 0.08_f32),
    ] {
        // Wide flat disc
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Cylinder::new(0.30 * s, 0.055 * s)), primary.clone(),
            Transform::from_xyz(x, 0.28 * s + y_lift, -0.005 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Shoulders);
        // Edge trim strip
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Cylinder::new(0.302 * s, 0.022 * s)), edge.clone(),
            Transform::from_xyz(x, 0.30 * s + y_lift, -0.004 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Shoulders);
        // Center accent disc
        spawn_part(commands, meshes, root, kind,
            Mesh::from(Cylinder::new(0.10 * s, 0.065 * s)), accent_glow.clone(),
            Transform::from_xyz(x, 0.27 * s + y_lift, -0.010 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)), PartSlotTag::Shoulders);
    }
}

// ── Legacy aliases (keep callers in characters.rs building) ───────────────────

pub fn spawn_robot_body(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    spawn_body_standard_mech(commands, meshes, materials, root, cfg, y_lift);
}

pub fn spawn_robot_arms(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    spawn_arms_mech(commands, meshes, materials, root, cfg, y_lift);
}

pub fn spawn_robot_legs(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    spawn_legs_mech(commands, meshes, materials, root, cfg, y_lift);
}

pub fn spawn_robot_shoulders(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    spawn_shoulders_dome(commands, meshes, materials, root, cfg, y_lift);
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
    let part = CartoonPart::new_bound(root, kind, default_joint_for_part(kind), &transform);
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
