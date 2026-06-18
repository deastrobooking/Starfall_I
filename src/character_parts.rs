use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

use crate::components::character::{
    default_joint_for_part, CartoonPart, CartoonPartKind, JointKind,
};
use crate::procedural_meshes::superellipsoid_mesh;
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
            BodyPreset::StandardMech => "Arc Knight",
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
            ArmPreset::MechArms => "Knight Arms",
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
            LegPreset::MechLegs => "Hero Boots",
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

/// Head / helmet preset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum HeadPreset {
    /// Natural humanoid face — exposed head, hair, glowing eyes.
    #[default]
    OpenFace,
    /// Angular tactical shell with forehead brow and eye-slit visor.
    CombatHelmet,
    /// Enclosed power-armor dome with glowing horizontal visor strip.
    FullHelm,
    /// Dark oversized skull with two glowing void eye sockets.
    VoidMask,
}

impl HeadPreset {
    pub fn label(self) -> &'static str {
        match self {
            HeadPreset::OpenFace => "Open Face",
            HeadPreset::CombatHelmet => "Combat Helmet",
            HeadPreset::FullHelm => "Full Helm",
            HeadPreset::VoidMask => "Void Mask",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            HeadPreset::OpenFace => HeadPreset::CombatHelmet,
            HeadPreset::CombatHelmet => HeadPreset::FullHelm,
            HeadPreset::FullHelm => HeadPreset::VoidMask,
            HeadPreset::VoidMask => HeadPreset::OpenFace,
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
    pub head: HeadPreset,
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
    pub skin: Color,
    pub hair: Color,
    pub eye: Color,
    pub has_hood: bool,
    pub has_cape: bool,
    pub has_gloves: bool,
    pub has_boots: bool,
    pub has_visor: bool,
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
        BodyPreset::StandardMech => {
            spawn_body_standard_mech(commands, meshes, materials, root, cfg, y_lift)
        }
        BodyPreset::HeavyPlate => {
            spawn_body_heavy_plate(commands, meshes, materials, root, cfg, y_lift)
        }
        BodyPreset::ScoutVest => {
            spawn_body_scout_vest(commands, meshes, materials, root, cfg, y_lift)
        }
        BodyPreset::VoidArmor => {
            spawn_body_void_armor(commands, meshes, materials, root, cfg, y_lift)
        }
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
        ShoulderPreset::DomePauldrons => {
            spawn_shoulders_dome(commands, meshes, materials, root, cfg, y_lift)
        }
        ShoulderPreset::SpikedPauldrons => {
            spawn_shoulders_spiked(commands, meshes, materials, root, cfg, y_lift)
        }
        ShoulderPreset::PlateEpaulettes => {
            spawn_shoulders_plate(commands, meshes, materials, root, cfg, y_lift)
        }
        ShoulderPreset::None => {}
    }
}

pub fn spawn_head(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
    preset: HeadPreset,
) {
    match preset {
        HeadPreset::OpenFace => {
            spawn_head_open_face(commands, meshes, materials, root, cfg, y_lift)
        }
        HeadPreset::CombatHelmet => {
            spawn_head_combat(commands, meshes, materials, root, cfg, y_lift)
        }
        HeadPreset::FullHelm => {
            spawn_head_full_helm(commands, meshes, materials, root, cfg, y_lift)
        }
        HeadPreset::VoidMask => {
            spawn_head_void_mask(commands, meshes, materials, root, cfg, y_lift)
        }
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
    let leather = soft_mat(materials, darken(cfg.outfit, 0.46));
    let trim = robot_mat(materials, cfg.accent);
    let accent_glow = emissive_mat(materials, cfg.accent, 2.4);

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
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        superellipsoid_mesh(
            Vec3::new(0.36 * s * bw, 0.34 * s, 0.055 * s),
            0.46,
            0.72,
            14,
            24,
        ),
        armor.clone(),
        Transform::from_xyz(0.0, 0.15 * s + y_lift, -0.295 * s),
        PartSlotTag::Body,
    );
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Cylinder::new(0.255 * s * bw.max(0.82), 0.040 * s)),
        trim.clone(),
        Transform::from_xyz(0.0, 0.385 * s + y_lift, -0.010 * s)
            .with_scale(Vec3::new(1.12, 1.0, 0.62)),
        PartSlotTag::Body,
    );
    for (x, rot_z) in [(-0.155_f32, -0.24_f32), (0.155_f32, 0.24_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Cuboid::new(0.22 * s, 0.055 * s, 0.050 * s)),
            trim.clone(),
            Transform::from_xyz(x * s * bw, 0.302 * s + y_lift, -0.298 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Body,
        );
    }
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.025 * s, 0.76 * s)),
        leather.clone(),
        Transform::from_xyz(-0.020 * s * bw, -0.085 * s + y_lift, -0.325 * s)
            .with_rotation(Quat::from_rotation_z(0.55)),
        PartSlotTag::Body,
    );
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Cuboid::new(0.40 * s * bw, 0.145 * s, 0.050 * s)),
        plating.clone(),
        Transform::from_xyz(0.0, -0.180 * s + y_lift, -0.300 * s),
        PartSlotTag::Body,
    );
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
    for (x, rot_z) in [(-0.18_f32, -0.13_f32), (0.18_f32, 0.13_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            superellipsoid_mesh(
                Vec3::new(0.105 * s, 0.265 * s, 0.040 * s),
                0.44,
                0.70,
                10,
                18,
            ),
            leather.clone(),
            Transform::from_xyz(x * s * hip_w, -0.645 * s + y_lift, -0.255 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Body,
        );
    }
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
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.30 * s, 0.72 * s)),
        primary.clone(),
        Transform::from_xyz(0.0, -0.08 * s + y_lift, 0.0).with_scale(Vec3::new(
            1.22 * bw,
            1.0,
            0.82,
        )),
        PartSlotTag::Body,
    );
    // Large dome chest plate
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.5)),
        armor.clone(),
        Transform::from_xyz(0.0, 0.12 * s + y_lift, -0.24 * s).with_scale(Vec3::new(
            0.78 * s * bw,
            0.50 * s,
            0.14 * s,
        )),
        PartSlotTag::Body,
    );
    // Collar reinforcement ring
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Cylinder::new(0.22 * s * bw.max(0.9), 0.060 * s)),
        plating.clone(),
        Transform::from_xyz(0.0, 0.38 * s + y_lift, 0.0),
        PartSlotTag::Body,
    );
    // Wide angular hip guards (4 pieces)
    for (x, rot_z) in [(-0.30_f32, -0.28_f32), (0.30_f32, 0.28_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.5)),
            plating.clone(),
            Transform::from_xyz(x * s * hip_w, -0.50 * s + y_lift, -0.010 * s)
                .with_scale(Vec3::new(0.32 * s, 0.18 * s, 0.36 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Body,
        );
    }
    // Thick waist plate
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.36 * s * bw.max(0.9), 0.075 * s)),
        plating.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, 0.0),
        PartSlotTag::Body,
    );
    // Flat belt buckle plate
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Belt,
        Mesh::from(Sphere::new(0.5)),
        armor.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, -0.26 * s).with_scale(Vec3::new(
            0.24 * s,
            0.18 * s,
            0.05 * s,
        )),
        PartSlotTag::Body,
    );
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
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.24 * s, 0.68 * s)),
        primary.clone(),
        Transform::from_xyz(0.0, -0.10 * s + y_lift, 0.0).with_scale(Vec3::new(
            0.90 * bw,
            1.0,
            0.60,
        )),
        PartSlotTag::Body,
    );
    // Diagonal chest stripe (left shoulder to right hip)
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.022 * s, 0.55 * s)),
        stripe.clone(),
        Transform::from_xyz(-0.05 * s * bw, 0.0 * s + y_lift, -0.24 * s)
            .with_rotation(Quat::from_rotation_z(0.55)),
        PartSlotTag::Body,
    );
    // Small badge / sensor node
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.042 * s)),
        accent_glow.clone(),
        Transform::from_xyz(0.12 * s * bw, 0.15 * s + y_lift, -0.26 * s),
        PartSlotTag::Body,
    );
    // Minimal hip ridge (thin band)
    for (x, rot_z) in [(-0.18_f32, -0.12_f32), (0.18_f32, 0.12_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.5)),
            stripe.clone(),
            Transform::from_xyz(x * s * hip_w, -0.50 * s + y_lift, -0.008 * s)
                .with_scale(Vec3::new(0.16 * s, 0.08 * s, 0.20 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Body,
        );
    }
    // Thin belt
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.24 * s * bw.max(0.7), 0.028 * s)),
        stripe.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, 0.0),
        PartSlotTag::Body,
    );
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
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.26 * s, 0.72 * s)),
        dark.clone(),
        Transform::from_xyz(0.0, -0.10 * s + y_lift, 0.0).with_scale(Vec3::new(
            1.0 * bw,
            1.0,
            0.65,
        )),
        PartSlotTag::Body,
    );
    // Four vertical edge-glow strips
    for (x, z) in [
        (-0.27_f32, -0.14_f32),
        (0.27_f32, -0.14_f32),
        (-0.18_f32, -0.27_f32),
        (0.18_f32, -0.27_f32),
    ] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Capsule3d::new(0.014 * s, 0.60 * s)),
            accent_glow.clone(),
            Transform::from_xyz(x * s * bw, -0.10 * s + y_lift, z * s),
            PartSlotTag::Body,
        );
    }
    // Hex chest grid (6 small glowing nodes)
    for (nx, ny) in [
        (-0.12_f32, 0.14_f32),
        (0.0_f32, 0.18_f32),
        (0.12_f32, 0.14_f32),
        (-0.12_f32, -0.02_f32),
        (0.0_f32, 0.02_f32),
        (0.12_f32, -0.02_f32),
    ] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.026 * s)),
            accent_glow.clone(),
            Transform::from_xyz(nx * s * bw, ny * s + y_lift, -0.285 * s),
            PartSlotTag::Body,
        );
    }
    // Large core reactor (bright center)
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.080 * s)),
        accent_glow.clone(),
        Transform::from_xyz(0.0, 0.08 * s + y_lift, -0.295 * s)
            .with_scale(Vec3::new(1.0, 1.0, 0.45)),
        PartSlotTag::Body,
    );
    // Hip glow nodes
    for x in [-0.22_f32 * s * hip_w, 0.22_f32 * s * hip_w] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.048 * s)),
            glow_dim.clone(),
            Transform::from_xyz(x, -0.50 * s + y_lift, -0.20 * s),
            PartSlotTag::Body,
        );
    }
    // Thin void belt
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.25 * s * bw.max(0.8), 0.030 * s)),
        dark.clone(),
        Transform::from_xyz(0.0, -0.38 * s + y_lift, 0.0),
        PartSlotTag::Body,
    );
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Belt,
        Mesh::from(Cylinder::new(0.25 * s * bw.max(0.8), 0.015 * s)),
        accent_glow.clone(),
        Transform::from_xyz(0.0, -0.375 * s + y_lift, 0.0),
        PartSlotTag::Body,
    );
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

    let sleeve = soft_mat(materials, cfg.outfit);
    let joint = soft_mat(materials, darken(cfg.outfit, 0.76));
    let cuff = soft_mat(materials, darken(cfg.accent, 0.82));
    let guard = robot_mat(materials, darken(cfg.accent, 0.76));
    let stitch = emissive_mat(materials, cfg.accent, 1.4);
    let glove = if cfg.has_gloves {
        soft_mat(materials, darken(cfg.accent, 0.88))
    } else {
        soft_mat(materials, cfg.skin)
    };

    let arm_x = (0.34 + 0.072) * s * bw;
    for (kind_arm, kind_hand, x, sign) in [
        (
            CartoonPartKind::LeftArm,
            CartoonPartKind::LeftHand,
            -arm_x,
            -1.0_f32,
        ),
        (
            CartoonPartKind::RightArm,
            CartoonPartKind::RightHand,
            arm_x,
            1.0_f32,
        ),
    ] {
        let elbow_joint = if x < 0.0 {
            JointKind::LeftElbow
        } else {
            JointKind::RightElbow
        };
        // Round shoulder cap
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Sphere::new(0.152 * s)),
            sleeve.clone(),
            Transform::from_xyz(x, 0.20 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Upper arm — full round tube
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Capsule3d::new(0.092 * s, 0.30 * s * al)),
            sleeve.clone(),
            Transform::from_xyz(x, 0.04 * s - (al - 1.0) * 0.07 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Capsule3d::new(0.014 * s, 0.230 * s * al)),
            stitch.clone(),
            Transform::from_xyz(x, 0.035 * s - (al - 1.0) * 0.07 * s + y_lift, -0.088 * s),
            PartSlotTag::Arms,
        );
        // Elbow joint sphere — clearly distinct
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Sphere::new(0.108 * s)),
            joint.clone(),
            Transform::from_xyz(x, -0.14 * s - (al - 1.0) * 0.12 * s + y_lift, -0.004 * s),
            PartSlotTag::Arms,
        );
        // Forearm — slightly narrower
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Capsule3d::new(0.080 * s, 0.28 * s * al)),
            sleeve.clone(),
            Transform::from_xyz(x, -0.31 * s - (al - 1.0) * 0.17 * s + y_lift, -0.018 * s)
                .with_rotation(Quat::from_rotation_x(0.08)),
            PartSlotTag::Arms,
        );
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            superellipsoid_mesh(
                Vec3::new(0.092 * s, 0.185 * s * al, 0.036 * s),
                0.42,
                0.66,
                10,
                18,
            ),
            guard.clone(),
            Transform::from_xyz(x, -0.305 * s - (al - 1.0) * 0.17 * s + y_lift, -0.102 * s)
                .with_rotation(Quat::from_rotation_x(0.08)),
            PartSlotTag::Arms,
        );
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Cylinder::new(0.100 * s, 0.040 * s)),
            cuff.clone(),
            Transform::from_xyz(x, -0.455 * s - (al - 1.0) * 0.24 * s + y_lift, -0.020 * s)
                .with_scale(Vec3::new(1.04, 1.0, 0.78)),
            PartSlotTag::Arms,
        );
        spawn_cartoon_glove(
            commands,
            meshes,
            root,
            kind_hand,
            glove.clone(),
            cuff.clone(),
            Vec3::new(x, -0.53 * s - (al - 1.0) * 0.28 * s + y_lift, -0.030 * s),
            sign,
            Vec3::new(
                0.150 * s * cfg.hand_scale,
                0.128 * s * cfg.hand_scale,
                0.130 * s * cfg.hand_scale,
            ),
            0.032 * s * cfg.hand_scale,
            0.150 * s * cfg.hand_scale,
            0.106 * s,
            0.052 * s,
        );
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
    let armor = robot_mat(materials, darken(cfg.accent, 0.70));
    let dark = robot_mat(materials, darken(cfg.outfit, 0.58));

    let arm_x = (0.36 + 0.092) * s * bw;
    for (kind_arm, kind_hand, x, _sign) in [
        (
            CartoonPartKind::LeftArm,
            CartoonPartKind::LeftHand,
            -arm_x,
            -1.0_f32,
        ),
        (
            CartoonPartKind::RightArm,
            CartoonPartKind::RightHand,
            arm_x,
            1.0_f32,
        ),
    ] {
        let elbow_joint = if x < 0.0 {
            JointKind::LeftElbow
        } else {
            JointKind::RightElbow
        };
        // Giant shoulder sphere — Bowser-scale
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Sphere::new(0.200 * s)),
            armor.clone(),
            Transform::from_xyz(x, 0.24 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Thick upper arm — barely tapers
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Capsule3d::new(0.135 * s, 0.32 * s * al)),
            primary.clone(),
            Transform::from_xyz(x, 0.05 * s - (al - 1.0) * 0.07 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Elbow knob — protrudes behind for brawler silhouette
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Sphere::new(0.130 * s)),
            dark.clone(),
            Transform::from_xyz(x, -0.14 * s - (al - 1.0) * 0.12 * s + y_lift, 0.075 * s),
            PartSlotTag::Arms,
        );
        // Thick forearm — barely narrows
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Capsule3d::new(0.125 * s, 0.32 * s * al)),
            armor.clone(),
            Transform::from_xyz(x, -0.32 * s - (al - 1.0) * 0.17 * s + y_lift, -0.014 * s)
                .with_rotation(Quat::from_rotation_x(0.07)),
            PartSlotTag::Arms,
        );
        let hand_y = -0.56 * s - (al - 1.0) * 0.28 * s + y_lift;
        spawn_cartoon_glove(
            commands,
            meshes,
            root,
            kind_hand,
            armor.clone(),
            dark.clone(),
            Vec3::new(x, hand_y, -0.020 * s),
            _sign,
            Vec3::new(
                0.178 * s * cfg.hand_scale,
                0.132 * s * cfg.hand_scale,
                0.154 * s * cfg.hand_scale,
            ),
            0.037 * s * cfg.hand_scale,
            0.118 * s * cfg.hand_scale,
            0.148 * s,
            0.064 * s,
        );
        // Three knuckle bumps across the front
        for i in 0..3_i32 {
            spawn_part(
                commands,
                meshes,
                root,
                kind_hand,
                Mesh::from(Sphere::new(0.042 * s)),
                dark.clone(),
                Transform::from_xyz(x + (i as f32 - 1.0) * 0.070 * s, hand_y, -0.122 * s),
                PartSlotTag::Arms,
            );
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

    let primary = soft_mat(materials, cfg.outfit);
    let stripe = soft_mat(materials, darken(cfg.accent, 0.86));
    let glove = soft_mat(materials, darken(cfg.accent, 0.70));

    let arm_x = (0.32 + 0.065) * s * bw;
    for (kind_arm, kind_hand, x, sign) in [
        (
            CartoonPartKind::LeftArm,
            CartoonPartKind::LeftHand,
            -arm_x,
            -1.0_f32,
        ),
        (
            CartoonPartKind::RightArm,
            CartoonPartKind::RightHand,
            arm_x,
            1.0_f32,
        ),
    ] {
        let elbow_joint = if x < 0.0 {
            JointKind::LeftElbow
        } else {
            JointKind::RightElbow
        };
        // Small neat shoulder cap
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Sphere::new(0.106 * s)),
            primary.clone(),
            Transform::from_xyz(x, 0.17 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Long slim upper arm — round cross-section
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Capsule3d::new(0.072 * s, 0.34 * s * al)),
            primary.clone(),
            Transform::from_xyz(x, 0.02 * s - (al - 1.0) * 0.07 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Slim elbow joint
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Sphere::new(0.084 * s)),
            stripe.clone(),
            Transform::from_xyz(x, -0.14 * s - (al - 1.0) * 0.12 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Long slim forearm
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Capsule3d::new(0.062 * s, 0.32 * s * al)),
            primary.clone(),
            Transform::from_xyz(x, -0.31 * s - (al - 1.0) * 0.17 * s + y_lift, -0.014 * s)
                .with_rotation(Quat::from_rotation_x(0.09)),
            PartSlotTag::Arms,
        );
        // Accent stripe running the length of the forearm
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Capsule3d::new(0.018 * s, 0.22 * s * al)),
            stripe.clone(),
            Transform::from_xyz(x, -0.31 * s - (al - 1.0) * 0.17 * s + y_lift, -0.066 * s)
                .with_rotation(Quat::from_rotation_x(0.09)),
            PartSlotTag::Arms,
        );
        spawn_cartoon_glove(
            commands,
            meshes,
            root,
            kind_hand,
            glove.clone(),
            stripe.clone(),
            Vec3::new(x, -0.49 * s - (al - 1.0) * 0.28 * s + y_lift, -0.040 * s),
            sign,
            Vec3::new(
                0.118 * s * cfg.hand_scale,
                0.112 * s * cfg.hand_scale,
                0.144 * s * cfg.hand_scale,
            ),
            0.026 * s * cfg.hand_scale,
            0.140 * s * cfg.hand_scale,
            0.080 * s,
            0.040 * s,
        );
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
    let accent_glow = emissive_mat(materials, cfg.accent, 3.2);

    let arm_x = (0.34 + 0.075) * s * bw;
    for (kind_arm, kind_hand, x, sign) in [
        (
            CartoonPartKind::LeftArm,
            CartoonPartKind::LeftHand,
            -arm_x,
            -1.0_f32,
        ),
        (
            CartoonPartKind::RightArm,
            CartoonPartKind::RightHand,
            arm_x,
            1.0_f32,
        ),
    ] {
        let elbow_joint = if x < 0.0 {
            JointKind::LeftElbow
        } else {
            JointKind::RightElbow
        };
        // Shoulder joint ball
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Sphere::new(0.150 * s)),
            dark.clone(),
            Transform::from_xyz(x, 0.20 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Glowing energy ring at shoulder socket
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Cylinder::new(0.160 * s, 0.026 * s)),
            accent_glow.clone(),
            Transform::from_xyz(x, 0.14 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Upper arm
        spawn_part(
            commands,
            meshes,
            root,
            kind_arm,
            Mesh::from(Capsule3d::new(0.096 * s, 0.30 * s * al)),
            primary.clone(),
            Transform::from_xyz(x, 0.04 * s - (al - 1.0) * 0.07 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Elbow node
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Sphere::new(0.110 * s)),
            dark.clone(),
            Transform::from_xyz(x, -0.14 * s - (al - 1.0) * 0.12 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        // Forearm
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Capsule3d::new(0.088 * s, 0.30 * s * al)),
            dark.clone(),
            Transform::from_xyz(x, -0.31 * s - (al - 1.0) * 0.17 * s + y_lift, -0.018 * s)
                .with_rotation(Quat::from_rotation_x(0.08)),
            PartSlotTag::Arms,
        );
        // Glowing energy vein down forearm
        spawn_part_bound(
            commands,
            meshes,
            root,
            kind_arm,
            Some(elbow_joint),
            Mesh::from(Capsule3d::new(0.015 * s, 0.26 * s * al)),
            accent_glow.clone(),
            Transform::from_xyz(x, -0.31 * s - (al - 1.0) * 0.17 * s + y_lift, -0.088 * s)
                .with_rotation(Quat::from_rotation_x(0.08)),
            PartSlotTag::Arms,
        );
        // Rounded claw palm and wrist collar
        spawn_part(
            commands,
            meshes,
            root,
            kind_hand,
            Mesh::from(Cylinder::new(0.102 * s, 0.040 * s)),
            accent_glow.clone(),
            Transform::from_xyz(x, -0.445 * s - (al - 1.0) * 0.28 * s + y_lift, 0.0),
            PartSlotTag::Arms,
        );
        spawn_part(
            commands,
            meshes,
            root,
            kind_hand,
            superellipsoid_mesh(
                Vec3::new(0.118 * s, 0.100 * s, 0.118 * s),
                0.50,
                0.68,
                12,
                18,
            ),
            dark.clone(),
            Transform::from_xyz(x, -0.51 * s - (al - 1.0) * 0.28 * s + y_lift, -0.010 * s),
            PartSlotTag::Arms,
        );
        // Four splayed claw prongs — wide menacing spread
        let hand_y = -0.50 * s - (al - 1.0) * 0.28 * s + y_lift;
        for (angle_off, prong_len) in [
            (-0.55_f32, 0.20_f32),
            (-0.18_f32, 0.27_f32),
            (0.18_f32, 0.27_f32),
            (0.55_f32, 0.20_f32),
        ] {
            spawn_part(
                commands,
                meshes,
                root,
                kind_hand,
                Mesh::from(Capsule3d::new(0.022 * s, prong_len * s)),
                primary.clone(),
                Transform::from_xyz(
                    x + sign * angle_off * 0.118 * s,
                    hand_y - 0.11 * s,
                    -0.025 * s,
                )
                .with_rotation(
                    Quat::from_rotation_x(0.22) * Quat::from_rotation_z(sign * angle_off * 0.88),
                ),
                PartSlotTag::Arms,
            );
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
    let strap = soft_mat(materials, darken(cfg.outfit, 0.42));
    let trim = robot_mat(materials, cfg.accent);
    let accent_glow = emissive_mat(materials, cfg.accent, 2.4);

    let leg_x = 0.180 * s * hip_w;
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
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Capsule3d::new(0.120 * s, 0.40 * s * ll)),
            primary.clone(),
            Transform::from_xyz(x, -0.65 * s - (ll - 1.0) * 0.13 * s + y_lift, 0.0)
                .with_scale(Vec3::new(1.00, 1.0, 0.92)),
            PartSlotTag::Legs,
        );
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            superellipsoid_mesh(
                Vec3::new(0.112 * s, 0.185 * s * ll, 0.040 * s),
                0.45,
                0.68,
                10,
                18,
            ),
            strap.clone(),
            Transform::from_xyz(x, -0.635 * s - (ll - 1.0) * 0.13 * s + y_lift, -0.094 * s)
                .with_scale(Vec3::new(1.0, 1.0, 0.85)),
            PartSlotTag::Legs,
        );
        let knee_joint = if x < 0.0 {
            JointKind::LeftKnee
        } else {
            JointKind::RightKnee
        };
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            Mesh::from(Sphere::new(0.156 * s)),
            armor.clone(),
            Transform::from_xyz(x, -0.85 * s - (ll - 1.0) * 0.20 * s + y_lift, -0.012 * s),
            PartSlotTag::Legs,
        );
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            superellipsoid_mesh(
                Vec3::new(0.150 * s, 0.060 * s, 0.040 * s),
                0.44,
                0.66,
                8,
                16,
            ),
            trim.clone(),
            Transform::from_xyz(x, -0.865 * s - (ll - 1.0) * 0.20 * s + y_lift, -0.145 * s),
            PartSlotTag::Legs,
        );
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            Mesh::from(Capsule3d::new(0.128 * s, 0.39 * s * ll)),
            armor.clone(),
            Transform::from_xyz(x, -s - (ll - 1.0) * 0.26 * s + y_lift, -0.008 * s)
                .with_scale(Vec3::new(0.96, 1.0, 0.86)),
            PartSlotTag::Legs,
        );
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            superellipsoid_mesh(
                Vec3::new(0.118 * s, 0.235 * s * ll, 0.038 * s),
                0.42,
                0.66,
                10,
                18,
            ),
            armor.clone(),
            Transform::from_xyz(x, -1.010 * s - (ll - 1.0) * 0.26 * s + y_lift, -0.115 * s)
                .with_scale(Vec3::new(0.92, 1.0, 0.82)),
            PartSlotTag::Legs,
        );
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            Mesh::from(Capsule3d::new(0.016 * s, 0.22 * s * ll)),
            accent_glow.clone(),
            Transform::from_xyz(x, -s - (ll - 1.0) * 0.26 * s + y_lift, -0.082 * s)
                .with_scale(Vec3::new(0.4, 1.0, 0.3)),
            PartSlotTag::Legs,
        );
        spawn_part(
            commands,
            meshes,
            root,
            boot_k,
            Mesh::from(Cylinder::new(0.126 * s * cfg.foot_scale, 0.048 * s)),
            trim.clone(),
            Transform::from_xyz(x, -1.145 * s - (ll - 1.0) * 0.30 * s + y_lift, 0.028 * s)
                .with_scale(Vec3::new(1.0, 1.0, 0.76)),
            PartSlotTag::Legs,
        );
        spawn_cartoon_shoe(
            commands,
            meshes,
            root,
            foot_k,
            Some(boot_k),
            primary.clone(),
            if cfg.has_boots {
                armor.clone()
            } else {
                primary.clone()
            },
            accent_glow.clone(),
            Vec3::new(x, -1.23 * s - (ll - 1.0) * 0.31 * s + y_lift, -0.09 * s),
            if x < 0.0 { -1.0 } else { 1.0 },
            s,
            cfg.foot_scale,
            1.04,
            1.10,
            cfg.has_boots,
        );
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

    let leg_x = 0.195 * s * hip_w;
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
        // Wide thigh plate
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Capsule3d::new(0.170 * s, 0.40 * s * ll)),
            primary.clone(),
            Transform::from_xyz(x, -0.63 * s - (ll - 1.0) * 0.13 * s + y_lift, 0.0)
                .with_scale(Vec3::new(1.18, 1.0, 1.02)),
            PartSlotTag::Legs,
        );
        // Large knee guard
        let knee_joint = if x < 0.0 {
            JointKind::LeftKnee
        } else {
            JointKind::RightKnee
        };
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            Mesh::from(Sphere::new(0.210 * s)),
            armor.clone(),
            Transform::from_xyz(x, -0.86 * s - (ll - 1.0) * 0.20 * s + y_lift, -0.022 * s),
            PartSlotTag::Legs,
        );
        // Wide armored shin block
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            Mesh::from(Capsule3d::new(0.158 * s, 0.40 * s * ll)),
            armor.clone(),
            Transform::from_xyz(x, -s - (ll - 1.0) * 0.26 * s + y_lift, -0.010 * s)
                .with_scale(Vec3::new(1.14, 1.0, 0.98)),
            PartSlotTag::Legs,
        );
        spawn_cartoon_shoe(
            commands,
            meshes,
            root,
            foot_k,
            Some(boot_k),
            dark.clone(),
            if cfg.has_boots {
                armor.clone()
            } else {
                primary.clone()
            },
            primary.clone(),
            Vec3::new(x, -1.24 * s - (ll - 1.0) * 0.31 * s + y_lift, -0.10 * s),
            if x < 0.0 { -1.0 } else { 1.0 },
            s,
            cfg.foot_scale,
            1.34,
            1.22,
            cfg.has_boots,
        );
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

    let leg_x = 0.170 * s * hip_w;
    for (leg_k, foot_k, _, x) in [
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
        // Slender thigh
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Capsule3d::new(0.105 * s, 0.44 * s * ll)),
            primary.clone(),
            Transform::from_xyz(x, -0.65 * s - (ll - 1.0) * 0.14 * s + y_lift, 0.0)
                .with_scale(Vec3::new(0.90, 1.0, 0.82)),
            PartSlotTag::Legs,
        );
        // Slim shin
        let knee_joint = if x < 0.0 {
            JointKind::LeftKnee
        } else {
            JointKind::RightKnee
        };
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            Mesh::from(Capsule3d::new(0.098 * s, 0.40 * s * ll)),
            stripe.clone(),
            Transform::from_xyz(x, -1.02 * s - (ll - 1.0) * 0.26 * s + y_lift, -0.005 * s)
                .with_scale(Vec3::new(0.86, 1.0, 0.78)),
            PartSlotTag::Legs,
        );
        spawn_cartoon_shoe(
            commands,
            meshes,
            root,
            foot_k,
            None,
            stripe.clone(),
            primary.clone(),
            stripe.clone(),
            Vec3::new(x, -1.25 * s - (ll - 1.0) * 0.31 * s + y_lift, -0.07 * s),
            if x < 0.0 { -1.0 } else { 1.0 },
            s,
            cfg.foot_scale,
            0.88,
            1.04,
            false,
        );
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

    let leg_x = 0.180 * s * hip_w;
    for (leg_k, x) in [
        (CartoonPartKind::LeftLeg, -leg_x),
        (CartoonPartKind::RightLeg, leg_x),
    ] {
        let knee_joint = if x < 0.0 {
            JointKind::LeftKnee
        } else {
            JointKind::RightKnee
        };
        // Rear nozzle housing
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            Mesh::from(Cylinder::new(0.086 * s, 0.112 * s)),
            dark.clone(),
            Transform::from_xyz(x, -1.04 * s - (ll - 1.0) * 0.26 * s + y_lift, 0.088 * s)
                .with_rotation(Quat::from_rotation_x(0.22)),
            PartSlotTag::Legs,
        );
        // Nozzle exhaust glow
        spawn_part_bound(
            commands,
            meshes,
            root,
            leg_k,
            Some(knee_joint),
            Mesh::from(Cylinder::new(0.076 * s, 0.062 * s)),
            accent_glow.clone(),
            Transform::from_xyz(x, -1.02 * s - (ll - 1.0) * 0.26 * s + y_lift, 0.096 * s)
                .with_rotation(Quat::from_rotation_x(0.22)),
            PartSlotTag::Legs,
        );
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
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Sphere::new(0.22 * s)),
            primary.clone(),
            Transform::from_xyz(x, 0.26 * s + y_lift, -0.010 * s)
                .with_scale(Vec3::new(1.0, 0.65, 0.88))
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
            Transform::from_xyz(x, 0.20 * s + y_lift, -0.055 * s)
                .with_scale(Vec3::new(0.34 * s, 0.10 * s, 0.24 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Shoulders,
        );
        // Upward spike
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Capsule3d::new(0.040 * s, 0.20 * s)),
            dark.clone(),
            Transform::from_xyz(x, 0.48 * s + y_lift, -0.015 * s)
                .with_rotation(Quat::from_rotation_z(rot_z * 0.5)),
            PartSlotTag::Shoulders,
        );
        // Spike tip
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Sphere::new(0.028 * s)),
            armor.clone(),
            Transform::from_xyz(x, 0.62 * s + y_lift, -0.012 * s),
            PartSlotTag::Shoulders,
        );
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
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Cylinder::new(0.30 * s, 0.055 * s)),
            primary.clone(),
            Transform::from_xyz(x, 0.28 * s + y_lift, -0.005 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Shoulders,
        );
        // Edge trim strip
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Cylinder::new(0.302 * s, 0.022 * s)),
            edge.clone(),
            Transform::from_xyz(x, 0.30 * s + y_lift, -0.004 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Shoulders,
        );
        // Center accent disc
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Cylinder::new(0.10 * s, 0.065 * s)),
            accent_glow.clone(),
            Transform::from_xyz(x, 0.27 * s + y_lift, -0.010 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Shoulders,
        );
    }
}

// ── Head presets ──────────────────────────────────────────────────────────────

fn spawn_head_open_face(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let head_y = y_lift + 0.58 * s;

    let skin_mat = soft_mat(materials, cfg.skin);
    let hair_mat = soft_mat(materials, cfg.hair);
    let brow_mat = soft_mat(materials, darken(cfg.hair, 0.62));
    let mouth_mat = soft_mat(materials, darken(cfg.skin, 0.48));
    let eye_mat = emissive_mat(materials, cfg.eye, 5.0);
    let visor_mat = emissive_mat(materials, cfg.eye, 3.2);
    let purple = Color::srgb(0.42, 0.07, 0.62);
    let cape_mat = soft_mat(materials, purple);

    // Skin head
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Head,
        Mesh::from(Sphere::new(0.34 * s)),
        skin_mat.clone(),
        Transform::from_xyz(0.0, head_y, -0.005 * s).with_scale(Vec3::new(0.92, 1.08, 0.88)),
        PartSlotTag::Head,
    );

    // Hair crown cap — full rounded top, not flat
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hair,
        Mesh::from(Sphere::new(0.37 * s)),
        hair_mat.clone(),
        Transform::from_xyz(0.0, head_y + 0.05 * s, 0.02 * s)
            .with_scale(Vec3::new(1.00, 0.70, 0.96)),
        PartSlotTag::Head,
    );
    // Hair back volume — gives length behind the head
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hair,
        Mesh::from(Sphere::new(0.33 * s)),
        hair_mat.clone(),
        Transform::from_xyz(0.0, head_y - 0.06 * s, 0.22 * s)
            .with_scale(Vec3::new(0.90, 0.94, 0.68)),
        PartSlotTag::Head,
    );
    // Hair side-temples — fills in above the ears
    for sign in [-1.0_f32, 1.0_f32] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hair,
            Mesh::from(Sphere::new(0.28 * s)),
            hair_mat.clone(),
            Transform::from_xyz(sign * 0.22 * s, head_y + 0.02 * s, 0.05 * s)
                .with_scale(Vec3::new(0.52, 0.72, 0.80)),
            PartSlotTag::Head,
        );
    }
    // Sculpted bangs and loose locks, so the head reads as an anime hero instead
    // of a plain sphere.
    for (x, y, z, sx, sy, sz, rz) in [
        (
            -0.145_f32, 0.135_f32, -0.270_f32, 0.34_f32, 0.52_f32, 0.25_f32, -0.36_f32,
        ),
        (
            -0.035_f32, 0.115_f32, -0.300_f32, 0.28_f32, 0.58_f32, 0.22_f32, -0.08_f32,
        ),
        (
            0.085_f32, 0.120_f32, -0.292_f32, 0.30_f32, 0.54_f32, 0.23_f32, 0.22_f32,
        ),
        (
            0.190_f32, 0.055_f32, -0.185_f32, 0.24_f32, 0.50_f32, 0.22_f32, 0.42_f32,
        ),
    ] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hair,
            Mesh::from(Sphere::new(0.23 * s)),
            hair_mat.clone(),
            Transform::from_xyz(x * s, head_y + y * s, z * s)
                .with_scale(Vec3::new(sx, sy, sz))
                .with_rotation(Quat::from_rotation_z(rz)),
            PartSlotTag::Head,
        );
    }

    // Eyes
    for (kind, eye_x) in [
        (CartoonPartKind::LeftEye, -0.13_f32),
        (CartoonPartKind::RightEye, 0.13_f32),
    ] {
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Sphere::new(0.060 * s)),
            eye_mat.clone(),
            Transform::from_xyz(eye_x * s, head_y + 0.04 * s, -0.32 * s)
                .with_scale(Vec3::new(0.82, 1.18, 0.30)),
            PartSlotTag::Head,
        );
    }
    for (eye_x, rot_z) in [(-0.13_f32, 0.14_f32), (0.13_f32, -0.14_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hair,
            Mesh::from(Cuboid::new(0.135 * s, 0.024 * s, 0.020 * s)),
            brow_mat.clone(),
            Transform::from_xyz(eye_x * s, head_y + 0.125 * s, -0.345 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
            PartSlotTag::Head,
        );
    }
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Nose,
        superellipsoid_mesh(
            Vec3::new(0.030 * s, 0.048 * s, 0.026 * s),
            0.55,
            0.78,
            8,
            14,
        ),
        skin_mat.clone(),
        Transform::from_xyz(0.0, head_y - 0.016 * s, -0.352 * s),
        PartSlotTag::Head,
    );
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Mouth,
        Mesh::from(Cuboid::new(0.128 * s, 0.018 * s, 0.014 * s)),
        mouth_mat.clone(),
        Transform::from_xyz(0.0, head_y - 0.115 * s, -0.350 * s),
        PartSlotTag::Head,
    );
    for sign in [-1.0_f32, 1.0_f32] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Head,
            superellipsoid_mesh(
                Vec3::new(0.038 * s, 0.068 * s, 0.026 * s),
                0.52,
                0.72,
                8,
                14,
            ),
            skin_mat.clone(),
            Transform::from_xyz(sign * 0.302 * s, head_y - 0.025 * s, -0.010 * s)
                .with_rotation(Quat::from_rotation_z(sign * 0.10)),
            PartSlotTag::Head,
        );
    }
    if cfg.has_visor {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Visor,
            Mesh::from(Cuboid::new(0.345 * s, 0.055 * s, 0.025 * s)),
            visor_mat.clone(),
            Transform::from_xyz(0.0, head_y + 0.040 * s, -0.365 * s),
            PartSlotTag::HeadGear,
        );
    }

    if cfg.has_hood {
        // Hood — drapes over crown and back, face left open
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hood,
            Mesh::from(Sphere::new(0.42 * s)),
            cape_mat.clone(),
            Transform::from_xyz(0.0, head_y + 0.04 * s, 0.10 * s)
                .with_scale(Vec3::new(1.12, 1.02, 0.80)),
            PartSlotTag::HeadGear,
        );
        // Hood neck drape — hangs down the back of the neck
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hood,
            Mesh::from(Capsule3d::new(0.22 * s, 0.18 * s)),
            cape_mat.clone(),
            Transform::from_xyz(0.0, head_y - 0.26 * s, 0.20 * s)
                .with_scale(Vec3::new(1.0, 1.0, 0.55)),
            PartSlotTag::HeadGear,
        );
    }

    if cfg.has_cape {
        // Cape upper yoke — broad across the shoulders
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Cape,
            Mesh::from(Cuboid::new(0.82 * s, 0.24 * s, 0.07 * s)),
            cape_mat.clone(),
            Transform::from_xyz(0.0, y_lift + 0.22 * s, 0.20 * s),
            PartSlotTag::HeadGear,
        );
        // Cape body panel
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Cape,
            Mesh::from(Cuboid::new(0.72 * s, 0.62 * s, 0.06 * s)),
            cape_mat.clone(),
            Transform::from_xyz(0.0, y_lift - 0.18 * s, 0.22 * s),
            PartSlotTag::HeadGear,
        );
        // Cape lower taper
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Cape,
            Mesh::from(Cuboid::new(0.54 * s, 0.36 * s, 0.055 * s)),
            cape_mat.clone(),
            Transform::from_xyz(0.0, y_lift - 0.66 * s, 0.22 * s),
            PartSlotTag::HeadGear,
        );
    }
}

fn spawn_head_combat(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let head_y = y_lift + 0.58 * s;

    let shell = robot_mat(materials, cfg.outfit);
    let dark_shell = robot_mat(materials, darken(cfg.outfit, 0.60));
    let visor_glow = emissive_mat(materials, cfg.accent, 3.5);

    // Main dome — angled flat-top sphere
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hood,
        Mesh::from(Sphere::new(0.38 * s)),
        shell.clone(),
        Transform::from_xyz(0.0, head_y + 0.04 * s, 0.02 * s)
            .with_scale(Vec3::new(1.04, 0.94, 1.0)),
        PartSlotTag::HeadGear,
    );
    // Brow plate
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hood,
        Mesh::from(Cuboid::new(0.48 * s, 0.10 * s, 0.09 * s)),
        dark_shell.clone(),
        Transform::from_xyz(0.0, head_y + 0.22 * s, -0.26 * s),
        PartSlotTag::HeadGear,
    );
    // Eye-slit visor
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Visor,
        Mesh::from(Cuboid::new(0.30 * s, 0.040 * s, 0.055 * s)),
        visor_glow.clone(),
        Transform::from_xyz(0.0, head_y + 0.09 * s, -0.36 * s),
        PartSlotTag::HeadGear,
    );
    // Cheek guards
    for (cx, rz) in [(-0.30_f32, 0.08_f32), (0.30_f32, -0.08_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hood,
            Mesh::from(Cuboid::new(0.09 * s, 0.22 * s, 0.20 * s)),
            dark_shell.clone(),
            Transform::from_xyz(cx * s, head_y - 0.07 * s, -0.12 * s)
                .with_rotation(Quat::from_rotation_z(rz)),
            PartSlotTag::HeadGear,
        );
    }
    // Chin guard
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hood,
        Mesh::from(Cuboid::new(0.26 * s, 0.08 * s, 0.13 * s)),
        dark_shell.clone(),
        Transform::from_xyz(0.0, head_y - 0.26 * s, -0.22 * s),
        PartSlotTag::HeadGear,
    );
}

fn spawn_head_full_helm(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let head_y = y_lift + 0.58 * s;

    let shell = robot_mat(materials, darken(cfg.outfit, 0.78));
    let brow_mat = robot_mat(materials, darken(cfg.accent, 0.55));
    let visor_glow = emissive_mat(materials, cfg.accent, 6.0);
    let collar = robot_mat(materials, darken(cfg.outfit, 0.52));

    // Main ovoid shell
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hood,
        Mesh::from(Sphere::new(0.40 * s)),
        shell.clone(),
        Transform::from_xyz(0.0, head_y + 0.04 * s, 0.0).with_scale(Vec3::new(1.00, 1.10, 0.97)),
        PartSlotTag::HeadGear,
    );
    // Brow ridge
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hood,
        Mesh::from(Cuboid::new(0.52 * s, 0.078 * s, 0.075 * s)),
        brow_mat.clone(),
        Transform::from_xyz(0.0, head_y + 0.26 * s, -0.28 * s),
        PartSlotTag::HeadGear,
    );
    // Glowing visor slit
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Visor,
        Mesh::from(Cuboid::new(0.34 * s, 0.042 * s, 0.038 * s)),
        visor_glow.clone(),
        Transform::from_xyz(0.0, head_y + 0.10 * s, -0.38 * s),
        PartSlotTag::HeadGear,
    );
    // Neck collar ring
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hood,
        Mesh::from(Cylinder::new(0.30 * s, 0.10 * s)),
        collar.clone(),
        Transform::from_xyz(0.0, head_y - 0.38 * s, 0.0),
        PartSlotTag::HeadGear,
    );
}

fn spawn_head_void_mask(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &CharacterVisualConfig,
    y_lift: f32,
) {
    let s = cfg.scale;
    let head_y = y_lift + 0.58 * s;

    let void_shell = robot_mat(materials, darken(cfg.outfit, 0.20));
    let eye_glow = emissive_mat(materials, cfg.accent, 9.0);
    let crest_mat = robot_mat(materials, darken(cfg.accent, 0.42));

    // Large dark skull sphere
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hood,
        Mesh::from(Sphere::new(0.44 * s)),
        void_shell.clone(),
        Transform::from_xyz(0.0, head_y + 0.06 * s, -0.01 * s)
            .with_scale(Vec3::new(1.02, 1.08, 1.0)),
        PartSlotTag::HeadGear,
    );
    // Glowing eye sockets
    for eye_x in [-0.14_f32, 0.14_f32] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Visor,
            Mesh::from(Sphere::new(0.058 * s)),
            eye_glow.clone(),
            Transform::from_xyz(eye_x * s, head_y + 0.09 * s, -0.38 * s)
                .with_scale(Vec3::new(1.0, 1.35, 0.38)),
            PartSlotTag::HeadGear,
        );
    }
    // Top crest ridge
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hood,
        Mesh::from(Cuboid::new(0.07 * s, 0.18 * s, 0.30 * s)),
        crest_mat.clone(),
        Transform::from_xyz(0.0, head_y + 0.48 * s, -0.04 * s)
            .with_rotation(Quat::from_rotation_x(-0.12)),
        PartSlotTag::HeadGear,
    );
    // Lower jaw plate
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hood,
        Mesh::from(Sphere::new(0.5)),
        void_shell.clone(),
        Transform::from_xyz(0.0, head_y - 0.22 * s, -0.18 * s).with_scale(Vec3::new(
            0.36 * s,
            0.19 * s,
            0.25 * s,
        )),
        PartSlotTag::HeadGear,
    );
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

pub fn spawn_cartoon_glove(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    root: Entity,
    kind: CartoonPartKind,
    hand_material: Handle<StandardMaterial>,
    cuff_material: Handle<StandardMaterial>,
    palm_center: Vec3,
    side: f32,
    palm_radii: Vec3,
    finger_radius: f32,
    finger_length: f32,
    cuff_radius: f32,
    cuff_depth: f32,
) {
    let side = if side < 0.0 { -1.0 } else { 1.0 };
    let scale_hint = palm_radii.max_element().max(0.001);
    let finger_spacing = palm_radii.x * 0.54;

    spawn_part(
        commands,
        meshes,
        root,
        kind,
        Mesh::from(Cylinder::new(cuff_radius, cuff_depth)),
        cuff_material.clone(),
        Transform::from_xyz(
            palm_center.x,
            palm_center.y + palm_radii.y * 0.92,
            palm_center.z + palm_radii.z * 0.10,
        )
        .with_scale(Vec3::new(1.05, 1.0, 0.78)),
        PartSlotTag::Arms,
    );
    spawn_part(
        commands,
        meshes,
        root,
        kind,
        superellipsoid_mesh(palm_radii, 0.55, 0.70, 14, 22),
        hand_material.clone(),
        Transform::from_translation(palm_center),
        PartSlotTag::Arms,
    );

    for (index, spread) in [(-1_i32, -1.0_f32), (0, 0.0), (1, 1.0)] {
        let x = palm_center.x + spread * finger_spacing;
        let y = palm_center.y - palm_radii.y * 0.82;
        let z = palm_center.z - palm_radii.z * 0.55 - index.abs() as f32 * 0.006 * scale_hint;
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Capsule3d::new(finger_radius, finger_length)),
            hand_material.clone(),
            Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_z(-spread * 0.16)),
            PartSlotTag::Arms,
        );
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            superellipsoid_mesh(
                Vec3::new(
                    finger_radius * 1.34,
                    finger_radius * 0.82,
                    finger_radius * 0.86,
                ),
                0.55,
                0.70,
                8,
                14,
            ),
            hand_material.clone(),
            Transform::from_xyz(x, palm_center.y - palm_radii.y * 0.14, z),
            PartSlotTag::Arms,
        );
    }

    spawn_part(
        commands,
        meshes,
        root,
        kind,
        Mesh::from(Capsule3d::new(finger_radius * 1.05, finger_length * 0.78)),
        hand_material,
        Transform::from_xyz(
            palm_center.x + side * palm_radii.x * 0.92,
            palm_center.y - palm_radii.y * 0.12,
            palm_center.z - palm_radii.z * 0.28,
        )
        .with_rotation(Quat::from_rotation_z(-side * PI * 0.5) * Quat::from_rotation_x(0.30)),
        PartSlotTag::Arms,
    );
}

pub fn spawn_cartoon_shoe(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    root: Entity,
    foot_kind: CartoonPartKind,
    boot_kind: Option<CartoonPartKind>,
    sole_material: Handle<StandardMaterial>,
    upper_material: Handle<StandardMaterial>,
    accent_material: Handle<StandardMaterial>,
    foot_center: Vec3,
    side: f32,
    scale: f32,
    foot_scale: f32,
    width_mult: f32,
    length_mult: f32,
    boot_details: bool,
) {
    let side = if side < 0.0 { -1.0 } else { 1.0 };
    let scale = scale.max(0.1);
    let foot_scale = foot_scale.max(0.55);
    let width = width_mult.max(0.55);
    let length = length_mult.max(0.65);

    spawn_part(
        commands,
        meshes,
        root,
        foot_kind,
        superellipsoid_mesh(
            Vec3::new(
                0.165 * scale * foot_scale * width,
                0.045 * scale,
                0.285 * scale * foot_scale * length,
            ),
            0.42,
            0.64,
            12,
            24,
        ),
        sole_material.clone(),
        Transform::from_xyz(
            foot_center.x,
            foot_center.y - 0.038 * scale,
            foot_center.z - 0.010 * scale,
        ),
        PartSlotTag::Legs,
    );
    spawn_part(
        commands,
        meshes,
        root,
        foot_kind,
        superellipsoid_mesh(
            Vec3::new(
                0.142 * scale * foot_scale * width,
                0.092 * scale,
                0.222 * scale * foot_scale * length,
            ),
            0.48,
            0.68,
            12,
            22,
        ),
        upper_material.clone(),
        Transform::from_xyz(
            foot_center.x,
            foot_center.y + 0.028 * scale,
            foot_center.z - 0.060 * scale,
        )
        .with_rotation(Quat::from_rotation_x(-0.08)),
        PartSlotTag::Legs,
    );
    spawn_part(
        commands,
        meshes,
        root,
        foot_kind,
        superellipsoid_mesh(
            Vec3::new(
                0.155 * scale * foot_scale * width,
                0.100 * scale,
                0.132 * scale * foot_scale * length,
            ),
            0.42,
            0.62,
            12,
            20,
        ),
        upper_material.clone(),
        Transform::from_xyz(
            foot_center.x,
            foot_center.y + 0.038 * scale,
            foot_center.z - 0.220 * scale * length,
        )
        .with_rotation(Quat::from_rotation_x(-0.14)),
        PartSlotTag::Legs,
    );
    spawn_part(
        commands,
        meshes,
        root,
        foot_kind,
        superellipsoid_mesh(
            Vec3::new(
                0.112 * scale * foot_scale * width,
                0.070 * scale,
                0.092 * scale * foot_scale,
            ),
            0.55,
            0.70,
            10,
            18,
        ),
        sole_material.clone(),
        Transform::from_xyz(
            foot_center.x,
            foot_center.y + 0.005 * scale,
            foot_center.z + 0.130 * scale,
        ),
        PartSlotTag::Legs,
    );

    if !boot_details {
        return;
    }

    if let Some(boot_kind) = boot_kind {
        spawn_part(
            commands,
            meshes,
            root,
            boot_kind,
            Mesh::from(Cylinder::new(
                0.118 * scale * foot_scale * width,
                0.082 * scale,
            )),
            upper_material,
            Transform::from_xyz(
                foot_center.x,
                foot_center.y + 0.176 * scale,
                foot_center.z + 0.070 * scale,
            )
            .with_scale(Vec3::new(1.0, 1.0, 0.78)),
            PartSlotTag::Legs,
        );
        spawn_part(
            commands,
            meshes,
            root,
            boot_kind,
            Mesh::from(Cuboid::new(
                0.290 * scale * foot_scale * width,
                0.035 * scale,
                0.052 * scale,
            )),
            accent_material.clone(),
            Transform::from_xyz(
                foot_center.x,
                foot_center.y + 0.112 * scale,
                foot_center.z - 0.105 * scale,
            )
            .with_rotation(Quat::from_rotation_x(-0.10)),
            PartSlotTag::Legs,
        );
        spawn_part(
            commands,
            meshes,
            root,
            boot_kind,
            superellipsoid_mesh(
                Vec3::new(0.038 * scale, 0.030 * scale, 0.021 * scale),
                0.52,
                0.70,
                8,
                14,
            ),
            accent_material,
            Transform::from_xyz(
                foot_center.x + side * 0.135 * scale * foot_scale * width,
                foot_center.y + 0.114 * scale,
                foot_center.z - 0.112 * scale,
            ),
            PartSlotTag::Legs,
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
    spawn_part_bound(
        commands,
        meshes,
        root,
        kind,
        default_joint_for_part(kind),
        mesh,
        material,
        transform,
        slot,
    );
}

fn spawn_part_bound(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    root: Entity,
    kind: CartoonPartKind,
    joint: Option<JointKind>,
    mesh: Mesh,
    material: Handle<StandardMaterial>,
    transform: Transform,
    slot: PartSlotTag,
) {
    let part = CartoonPart::new_bound(root, kind, joint, &transform);
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

// ── Material kit: "Crystal Chronicles meets sci-fi" ──────────────────────────
//
// The look is soft and painterly, not hard cel: matte surfaces, a warm storybook
// shadow-lift so unlit faces never crush to black, a lacquered toy-tech sheen on
// armour, and bright crystalline accents that bloom (the player camera runs a
// Bloom pass). Hues come from each hero's palette — these helpers only grade the
// lighting response so every part reads as one cohesive, hand-painted world.

/// Faint, warm self-illumination so faces turned away from the key light glow
/// softly instead of crushing to black — the "filled shadow" that defines the
/// storybook look. `k` scales the lift; the colour is biased toward amber.
fn warm_floor(lin: LinearRgba, k: f32) -> LinearRgba {
    let base = Vec3::new(lin.red, lin.green, lin.blue);
    let amber = Vec3::new(1.0, 0.72, 0.42);
    let tint = base.lerp(amber, 0.22) * k;
    LinearRgba::new(tint.x, tint.y, tint.z, 1.0)
}

/// Tech armour / robot plating: semi-metallic with a lacquered clearcoat sheen
/// and soft, broad highlights — storybook toy-robot, never chrome.
fn robot_mat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    let lin = color.to_linear();
    materials.add(StandardMaterial {
        base_color: color,
        emissive: warm_floor(lin, 0.10),
        perceptual_roughness: 0.50,
        metallic: 0.45,
        reflectance: 0.42,
        clearcoat: 0.55,
        clearcoat_perceptual_roughness: 0.30,
        ..default()
    })
}

/// Soft organic material for skin / cloth / hair — matches `char_mat()` in
/// characters.rs. Very matte, low specular, with a touch of subsurface
/// light-bleed for that hand-painted softness.
fn soft_mat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    let lin = color.to_linear();
    materials.add(StandardMaterial {
        base_color: color,
        emissive: warm_floor(lin, 0.16),
        perceptual_roughness: 0.82,
        metallic: 0.0,
        reflectance: 0.16,
        diffuse_transmission: 0.12,
        ..default()
    })
}

/// Crystal / energy accents: a bright blooming core under a glassy clearcoat so
/// it reads like a polished gem — the sci-fi spark in the fantasy frame.
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
        perceptual_roughness: 0.22,
        metallic: 0.0,
        reflectance: 0.5,
        clearcoat: 0.9,
        clearcoat_perceptual_roughness: 0.12,
        ..default()
    })
}

fn darken(color: Color, factor: f32) -> Color {
    let lin = color.to_linear();
    Color::linear_rgb(lin.red * factor, lin.green * factor, lin.blue * factor)
}
