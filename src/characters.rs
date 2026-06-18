use bevy::prelude::*;
use std::f32::consts::PI;

use crate::character_blueprint::{BodyRecipe, CharacterBlueprint};
use crate::character_parts::{
    spawn_cartoon_glove, spawn_cartoon_shoe, CharacterLoadout, CharacterVisualConfig, PartSlotTag,
};
use crate::components::character::{
    default_joint_for_part, CartoonAnimator, CartoonCharacter, CartoonPart, CartoonPartKind,
    CartoonRole, CharacterIkPose, JointKind, JointMarker, SkeletonRig,
};
use crate::components::enemy::EnemyType;
use crate::components::faction::Faction;
use crate::rendering::PbrBundle;

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
    pub body: BodyRecipe,
    /// Multiplies the body (and proportionally arms/legs) in the X axis.
    pub body_width: f32,
    pub has_hat: bool,
    pub has_hood: bool,
    pub has_horns: bool,
    pub extra_horns: bool,
    pub has_tail: bool,
    pub has_star_badge: bool,
    pub has_visor: bool,
    pub has_gloves: bool,
    pub has_boots: bool,
    pub has_shoulder_pads: bool,
    pub has_cape: bool,
    pub has_spine_ridges: bool,
    pub has_belt: bool,
    pub has_mouth: bool,
    pub eye_color: Color,
    pub emissive_eyes: bool,
}

impl CartoonCharacterConfig {
    pub fn with_body_recipe(mut self, body: BodyRecipe) -> Self {
        self.body = body.validated();
        self
    }

    pub fn with_blueprint(mut self, blueprint: &CharacterBlueprint) -> Self {
        self.body = blueprint.body.validated();
        if let Some(color) = blueprint.material_color("skin") {
            self.skin = color;
        }
        if let Some(color) = blueprint.material_color("outfit") {
            self.outfit = color;
        }
        if let Some(color) = blueprint.material_color("accent") {
            self.accent = color;
        }
        if let Some(color) = blueprint.material_color("hair") {
            self.hair = color;
        }
        if let Some(color) = blueprint.material_color("eye") {
            self.eye_color = color;
        }
        let appearance = blueprint.cartoon_appearance;
        self.has_hood = appearance.has_hood;
        self.has_cape = appearance.has_cape;
        self.has_gloves = appearance.has_gloves;
        self.has_boots = appearance.has_boots;
        self.has_shoulder_pads = appearance.has_shoulder_pads;
        self.has_visor = appearance.has_visor;
        self
    }
}

// ── Hero configs ──────────────────────────────────────────────────────────────

pub fn hero_config(name: &'static str) -> CartoonCharacterConfig {
    match name {
        "Vincenzo" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.93, 0.70, 0.48),
            outfit: Color::srgb(0.09, 0.28, 0.80),
            accent: Color::srgb(0.95, 0.78, 0.12),
            hair: Color::srgb(0.06, 0.04, 0.02), // jet black
            scale: 2.0,
            body: BodyRecipe { arm_length: 1.30, leg_length: 1.30, ..BodyRecipe::default() },
            body_width: 1.0,
            has_hat: false,
            has_hood: true,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: true,
            has_cape: true,
            has_spine_ridges: false,
            has_belt: true,
            has_mouth: true,
            eye_color: Color::srgb(0.08, 0.18, 0.72), // cobalt blue
            emissive_eyes: false,
        },
        "Antonio" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.90, 0.74, 0.56),
            outfit: Color::srgb(0.16, 0.48, 0.82),
            accent: Color::srgb(0.10, 0.92, 0.98),
            hair: Color::srgb(0.70, 0.52, 0.18), // sandy blonde
            scale: 2.0,
            body: BodyRecipe { arm_length: 1.30, leg_length: 1.30, ..BodyRecipe::default() },
            body_width: 0.92,
            has_hat: false,
            has_hood: true,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: false,
            has_cape: true,
            has_spine_ridges: false,
            has_belt: true,
            has_mouth: true,
            eye_color: Color::srgb(0.08, 0.52, 0.86), // sky blue
            emissive_eyes: false,
        },
        "Angelo" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.88, 0.68, 0.48),
            outfit: Color::srgb(0.08, 0.52, 0.20),
            accent: Color::srgb(0.78, 0.86, 0.92),
            hair: Color::srgb(0.28, 0.16, 0.06), // medium brown
            scale: 2.0,
            body: BodyRecipe { arm_length: 1.30, leg_length: 1.30, ..BodyRecipe::default() },
            body_width: 0.94,
            has_hat: false,
            has_hood: true,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: false,
            has_cape: true,
            has_spine_ridges: false,
            has_belt: true,
            has_mouth: true,
            eye_color: Color::srgb(0.12, 0.52, 0.22), // forest green
            emissive_eyes: false,
        },
        "Joseph" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.76, 0.54, 0.36), // deeper warm brown
            outfit: Color::srgb(0.75, 0.12, 0.08),
            accent: Color::srgb(0.92, 0.90, 0.84),
            hair: Color::srgb(0.52, 0.48, 0.44), // gray-silver (veteran)
            scale: 2.0,
            body: BodyRecipe { arm_length: 1.30, leg_length: 1.30, ..BodyRecipe::default() },
            body_width: 1.18,
            has_hat: false,
            has_hood: true,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: true,
            has_cape: true,
            has_spine_ridges: false,
            has_belt: true,
            has_mouth: true,
            eye_color: Color::srgb(0.65, 0.38, 0.08), // warm amber
            emissive_eyes: false,
        },
        "Gabriella" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.91, 0.69, 0.50),
            outfit: Color::srgb(0.48, 0.12, 0.72),
            accent: Color::srgb(1.0, 0.68, 0.95),
            hair: Color::srgb(0.52, 0.08, 0.12), // deep crimson
            scale: 2.0,
            body: BodyRecipe { arm_length: 1.30, leg_length: 1.30, ..BodyRecipe::default() },
            body_width: 0.98,
            has_hat: false,
            has_hood: true,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: true,
            has_cape: true,
            has_spine_ridges: false,
            has_belt: true,
            has_mouth: true,
            eye_color: Color::srgb(0.42, 0.08, 0.58), // violet
            emissive_eyes: false,
        },
        "Nova" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.84, 0.64, 0.50),
            outfit: Color::srgb(0.10, 0.18, 0.82),
            accent: Color::srgb(0.10, 1.0, 0.88),
            hair: Color::srgb(0.06, 0.06, 0.28), // deep blue-black (sci-fi)
            scale: 2.0,
            body: BodyRecipe { arm_length: 1.30, leg_length: 1.30, ..BodyRecipe::default() },
            body_width: 0.94,
            has_hat: false,
            has_hood: true,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: true,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: false,
            has_cape: true,
            has_spine_ridges: false,
            has_belt: true,
            has_mouth: true,
            eye_color: Color::srgb(0.04, 0.82, 0.92), // bright cyan (glows)
            emissive_eyes: true,
        },
        "Aurora" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.90, 0.72, 0.55),
            outfit: Color::srgb(0.16, 0.58, 0.42),
            accent: Color::srgb(0.78, 1.0, 0.62),
            hair: Color::srgb(0.68, 0.52, 0.14), // warm golden-blonde
            scale: 2.0,
            body: BodyRecipe { arm_length: 1.30, leg_length: 1.30, ..BodyRecipe::default() },
            body_width: 1.02,
            has_hat: false,
            has_hood: true,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: true,
            has_cape: true,
            has_spine_ridges: false,
            has_belt: true,
            has_mouth: true,
            eye_color: Color::srgb(0.14, 0.62, 0.28), // bright green
            emissive_eyes: false,
        },
        "Fortuna" => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.92, 0.70, 0.52),
            outfit: Color::srgb(0.68, 0.22, 0.10),
            accent: Color::srgb(1.0, 0.92, 0.30),
            hair: Color::srgb(0.40, 0.08, 0.04), // dark auburn
            scale: 2.0,
            body: BodyRecipe { arm_length: 1.30, leg_length: 1.30, ..BodyRecipe::default() },
            body_width: 0.96,
            has_hat: false,
            has_hood: true,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: false,
            has_cape: true,
            has_spine_ridges: false,
            has_belt: true,
            has_mouth: true,
            eye_color: Color::srgb(0.68, 0.48, 0.06), // golden amber
            emissive_eyes: false,
        },
        _ => CartoonCharacterConfig {
            name,
            role: CartoonRole::Hero,
            skin: Color::srgb(0.95, 0.68, 0.45),
            outfit: Color::srgb(0.15, 0.45, 0.95),
            accent: Color::srgb(1.0, 0.86, 0.2),
            hair: Color::srgb(0.06, 0.04, 0.02),
            scale: 2.0,
            body: BodyRecipe { arm_length: 1.30, leg_length: 1.30, ..BodyRecipe::default() },
            body_width: 1.0,
            has_hat: false,
            has_hood: true,
            has_horns: false,
            extra_horns: false,
            has_tail: false,
            has_star_badge: true,
            has_visor: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: false,
            has_cape: true,
            has_spine_ridges: false,
            has_belt: true,
            has_mouth: true,
            eye_color: Color::srgb(0.10, 0.20, 0.70), // default: blue
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

    let (
        role,
        outfit,
        accent,
        skin,
        hair,
        has_horns,
        extra_horns,
        has_tail,
        has_spine_ridges,
        eye_color,
        emissive_eyes,
    ) = match faction {
        Faction::DragonRoyalty => (
            CartoonRole::Dragon,
            Color::srgb(0.92, 0.16, 0.06),
            Color::srgb(1.0, 0.72, 0.12),
            Color::srgb(0.72, 0.20, 0.10),
            Color::srgb(0.12, 0.02, 0.01),
            true,
            true,
            true,
            true,
            Color::srgb(1.0, 0.58, 0.0),
            true,
        ),
        Faction::DragonExile => (
            CartoonRole::Dragon,
            Color::srgb(0.18, 0.28, 0.58),
            Color::srgb(0.70, 0.88, 1.0),
            Color::srgb(0.38, 0.50, 0.78),
            Color::srgb(0.03, 0.04, 0.10),
            true,
            false,
            true,
            true,
            Color::srgb(0.3, 0.75, 1.0),
            true,
        ),
        Faction::CorruptedHuman => (
            CartoonRole::Rival,
            Color::srgb(0.32, 0.06, 0.40),
            Color::srgb(1.0, 0.18, 0.88),
            Color::srgb(0.75, 0.55, 0.48),
            Color::srgb(0.05, 0.02, 0.06),
            false,
            false,
            false,
            false,
            Color::srgb(1.0, 0.1, 0.9),
            true,
        ),
        Faction::WizardScientist => (
            CartoonRole::Wizard,
            Color::srgb(0.28, 0.72, 0.92),
            Color::srgb(1.0, 0.94, 0.52),
            Color::srgb(0.92, 0.68, 0.45),
            Color::srgb(0.22, 0.16, 0.08),
            false,
            false,
            false,
            false,
            Color::srgb(0.0, 0.85, 1.0),
            true,
        ),
        Faction::HeroSister => (
            CartoonRole::Hero,
            Color::srgb(0.92, 0.30, 0.80),
            Color::srgb(0.45, 0.95, 1.0),
            Color::srgb(0.92, 0.68, 0.45),
            Color::srgb(0.18, 0.05, 0.20),
            false,
            false,
            false,
            false,
            Color::srgb(0.05, 0.04, 0.10),
            false,
        ),
        Faction::HeroBrother => (
            CartoonRole::Hero,
            Color::srgb(0.12, 0.42, 0.92),
            Color::srgb(1.0, 0.85, 0.18),
            Color::srgb(0.93, 0.70, 0.48),
            Color::srgb(0.10, 0.06, 0.03),
            false,
            false,
            false,
            false,
            Color::srgb(0.05, 0.05, 0.10),
            false,
        ),
        _ => (
            CartoonRole::Alien,
            Color::srgb(0.20, 0.90, 0.42),
            Color::srgb(0.82, 0.22, 1.0),
            Color::srgb(0.32, 0.88, 0.52),
            Color::srgb(0.06, 0.18, 0.09),
            matches!(enemy_type, EnemyType::Heavy | EnemyType::Hybrid),
            false,
            matches!(enemy_type, EnemyType::SpikeAlien | EnemyType::Hybrid),
            matches!(enemy_type, EnemyType::Hybrid),
            Color::srgb(0.2, 1.0, 0.4),
            true,
        ),
    };

    let type_scale = match enemy_type {
        EnemyType::Drone => 0.82,
        EnemyType::SpyDrone => 0.64,
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
        body: BodyRecipe::default(),
        body_width: type_width,
        has_hat: role == CartoonRole::Wizard,
        has_hood: matches!(role, CartoonRole::Hero | CartoonRole::Wizard),
        has_horns,
        extra_horns,
        has_tail,
        has_star_badge: matches!(role, CartoonRole::Hero | CartoonRole::Wizard),
        has_visor: false,
        has_gloves: matches!(role, CartoonRole::Hero | CartoonRole::Wizard),
        has_boots: matches!(
            role,
            CartoonRole::Hero | CartoonRole::Wizard | CartoonRole::Rival
        ),
        has_shoulder_pads: matches!(enemy_type, EnemyType::Heavy | EnemyType::Hybrid),
        has_cape: false,
        has_spine_ridges,
        has_belt: false,
        has_mouth: false,
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
        .spawn((
            Transform::from_translation(position),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .id();
    attach_cartoon_character(commands, meshes, materials, root, config, position);
    root
}

pub fn spawn_skeleton_rig(
    commands: &mut Commands,
    root: Entity,
    scale: f32,
    body: &BodyRecipe,
) -> SkeletonRig {
    let body = body.validated();
    let mut rig = SkeletonRig::default();

    let pelvis_rest = humanoid_joint_rest_translation(JointKind::Pelvis, scale, &body);
    let pelvis = spawn_joint(
        commands,
        &mut rig,
        root,
        root,
        Vec3::ZERO,
        JointKind::Pelvis,
        pelvis_rest,
    );

    let left_hip_rest = humanoid_joint_rest_translation(JointKind::LeftHip, scale, &body);
    let left_hip = spawn_joint(
        commands,
        &mut rig,
        root,
        pelvis,
        pelvis_rest,
        JointKind::LeftHip,
        left_hip_rest,
    );
    let left_knee_rest = humanoid_joint_rest_translation(JointKind::LeftKnee, scale, &body);
    let left_knee = spawn_joint(
        commands,
        &mut rig,
        root,
        left_hip,
        left_hip_rest,
        JointKind::LeftKnee,
        left_knee_rest,
    );
    let left_ankle_rest = humanoid_joint_rest_translation(JointKind::LeftAnkle, scale, &body);
    spawn_joint(
        commands,
        &mut rig,
        root,
        left_knee,
        left_knee_rest,
        JointKind::LeftAnkle,
        left_ankle_rest,
    );

    let right_hip_rest = humanoid_joint_rest_translation(JointKind::RightHip, scale, &body);
    let right_hip = spawn_joint(
        commands,
        &mut rig,
        root,
        pelvis,
        pelvis_rest,
        JointKind::RightHip,
        right_hip_rest,
    );
    let right_knee_rest = humanoid_joint_rest_translation(JointKind::RightKnee, scale, &body);
    let right_knee = spawn_joint(
        commands,
        &mut rig,
        root,
        right_hip,
        right_hip_rest,
        JointKind::RightKnee,
        right_knee_rest,
    );
    let right_ankle_rest = humanoid_joint_rest_translation(JointKind::RightAnkle, scale, &body);
    spawn_joint(
        commands,
        &mut rig,
        root,
        right_knee,
        right_knee_rest,
        JointKind::RightAnkle,
        right_ankle_rest,
    );

    let spine_rest = humanoid_joint_rest_translation(JointKind::Spine, scale, &body);
    let spine = spawn_joint(
        commands,
        &mut rig,
        root,
        pelvis,
        pelvis_rest,
        JointKind::Spine,
        spine_rest,
    );
    let chest_rest = humanoid_joint_rest_translation(JointKind::Chest, scale, &body);
    let chest = spawn_joint(
        commands,
        &mut rig,
        root,
        spine,
        spine_rest,
        JointKind::Chest,
        chest_rest,
    );

    let neck_rest = humanoid_joint_rest_translation(JointKind::Neck, scale, &body);
    let neck = spawn_joint(
        commands,
        &mut rig,
        root,
        chest,
        chest_rest,
        JointKind::Neck,
        neck_rest,
    );
    let head_rest = humanoid_joint_rest_translation(JointKind::Head, scale, &body);
    spawn_joint(
        commands,
        &mut rig,
        root,
        neck,
        neck_rest,
        JointKind::Head,
        head_rest,
    );

    let left_shoulder_rest = humanoid_joint_rest_translation(JointKind::LeftShoulder, scale, &body);
    let left_shoulder = spawn_joint(
        commands,
        &mut rig,
        root,
        chest,
        chest_rest,
        JointKind::LeftShoulder,
        left_shoulder_rest,
    );
    let left_elbow_rest = humanoid_joint_rest_translation(JointKind::LeftElbow, scale, &body);
    let left_elbow = spawn_joint(
        commands,
        &mut rig,
        root,
        left_shoulder,
        left_shoulder_rest,
        JointKind::LeftElbow,
        left_elbow_rest,
    );
    let left_wrist_rest = humanoid_joint_rest_translation(JointKind::LeftWrist, scale, &body);
    spawn_joint(
        commands,
        &mut rig,
        root,
        left_elbow,
        left_elbow_rest,
        JointKind::LeftWrist,
        left_wrist_rest,
    );

    let right_shoulder_rest =
        humanoid_joint_rest_translation(JointKind::RightShoulder, scale, &body);
    let right_shoulder = spawn_joint(
        commands,
        &mut rig,
        root,
        chest,
        chest_rest,
        JointKind::RightShoulder,
        right_shoulder_rest,
    );
    let right_elbow_rest = humanoid_joint_rest_translation(JointKind::RightElbow, scale, &body);
    let right_elbow = spawn_joint(
        commands,
        &mut rig,
        root,
        right_shoulder,
        right_shoulder_rest,
        JointKind::RightElbow,
        right_elbow_rest,
    );
    let right_wrist_rest = humanoid_joint_rest_translation(JointKind::RightWrist, scale, &body);
    spawn_joint(
        commands,
        &mut rig,
        root,
        right_elbow,
        right_elbow_rest,
        JointKind::RightWrist,
        right_wrist_rest,
    );

    rig
}

pub fn attach_cartoon_character(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    config: CartoonCharacterConfig,
    root_position: Vec3,
) {
    let body = config.body.validated();
    let visual_ground_lift = visual_ground_lift(&body, config.scale);
    let skeleton = spawn_skeleton_rig(commands, root, config.scale, &body);
    commands.entity(root).insert((
        CartoonCharacter {
            name: config.name.to_string(),
            role: config.role,
            scale: config.scale,
            stride: body.stride_multiplier(),
            agility: body.agility_multiplier(),
            visual_ground_lift,
        },
        CartoonAnimator::new(root_position),
        Visibility::Visible,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        skeleton,
        CharacterIkPose::default(),
    ));

    // ── Materials ─────────────────────────────────────────────────────────────
    let skin = char_mat(materials, config.skin);
    let outfit = char_mat(materials, config.outfit);
    let accent = char_mat(materials, config.accent);
    let hair = char_mat(materials, config.hair);
    let shadow = materials.add(StandardMaterial {
        base_color: Color::srgba(0.03, 0.045, 0.035, 0.12),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let eye_mat = if config.emissive_eyes {
        emissive_mat(materials, config.eye_color, 6.0)
    } else {
        char_mat(materials, config.eye_color)
    };
    let brow_color = darken(config.hair, 0.7);
    let brow = char_mat(materials, brow_color);
    let suit_shadow = char_mat(materials, darken(config.outfit, 0.58));
    let armor = char_mat(materials, darken(config.accent, 0.78));
    let accent_glow = emissive_mat(materials, config.accent, 2.4);

    let s = config.scale;
    let height = body.height;
    let bw = config.body_width * (body.shoulder_width * 0.62 + body.chest_size * 0.38);
    let hip_width = config.body_width * body.hip_width;
    let head_scale = body.head_size;
    let arm_length = body.arm_length;
    let leg_length = body.leg_length;
    let hand_scale = body.hand_size;
    let foot_scale = body.foot_size;
    let posture = body.spine_posture;
    let head_y = 0.58 * s * height + (body.neck_length - 1.0) * 0.10 * s;
    let eye_y = head_y + 0.045 * s;
    let brow_y = head_y + 0.135 * s;
    let nose_y = head_y - 0.035 * s;
    let mouth_y = head_y - 0.115 * s;
    let face_z = -0.34 * s;

    // ── Shadow ────────────────────────────────────────────────────────────────
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Shadow,
        Mesh::from(Cylinder::new(0.42 * s * bw.max(foot_scale), 0.012 * s)),
        shadow,
        Transform::from_xyz(0.0, -1.30 * s * height.max(leg_length), 0.0),
    );

    // ── Body: Dreamcast-anime low-poly suit core with layered armor ───────────
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Capsule3d::new(0.30 * s, 0.72 * s * height)),
        outfit.clone(),
        Transform::from_xyz(0.0, -0.12 * s, 0.0)
            .with_scale(Vec3::new(1.12 * bw, 1.0, 0.72 * body.chest_size))
            .with_rotation(Quat::from_rotation_x(-posture * 0.18)),
    );
    // Chest plate: ellipsoid (same bounds as old Cuboid, smooth rounded edges)
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.5)),
        armor.clone(),
        Transform::from_xyz(0.0, 0.06 * s, -0.265 * s)
            .with_scale(Vec3::new(0.58 * s * bw, 0.36 * s, 0.080 * s))
            .with_rotation(Quat::from_rotation_x(-0.08 - posture * 0.12)),
    );
    // Lower torso plate: ellipsoid
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Body,
        Mesh::from(Sphere::new(0.5)),
        suit_shadow.clone(),
        Transform::from_xyz(0.0, -0.29 * s, -0.245 * s)
            .with_scale(Vec3::new(0.40 * s * bw, 0.25 * s, 0.055 * s))
            .with_rotation(Quat::from_rotation_x(-0.02 - posture * 0.10)),
    );
    // Side accent tubes: Capsule3d (smooth cylindrical glow strips)
    for (x, rot_z) in [(-0.23_f32, -0.18_f32), (0.23_f32, 0.18_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Capsule3d::new(0.022 * s, 0.20 * s)),
            accent_glow.clone(),
            Transform::from_xyz(x * s * bw, -0.10 * s, -0.292 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
        );
    }
    // Hip guards: ellipsoid (smooth rounded)
    for (x, rot_z) in [(-0.26_f32, -0.25_f32), (0.26_f32, 0.25_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Body,
            Mesh::from(Sphere::new(0.5)),
            armor.clone(),
            Transform::from_xyz(x * s * hip_width, -0.48 * s * height, -0.02 * s)
                .with_scale(Vec3::new(0.22 * s, 0.12 * s, 0.30 * s))
                .with_rotation(Quat::from_rotation_z(rot_z)),
        );
    }
    // ── Neck ─────────────────────────────────────────────────────────────────
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Head,
        Mesh::from(Capsule3d::new(0.10 * s, 0.08 * s)),
        skin.clone(),
        Transform::from_xyz(0.0, 0.40 * s, -0.010 * s).with_scale(Vec3::new(0.72, 1.0, 0.65)),
    );

    // ── Belt (waist strap + centre buckle) ────────────────────────────────────
    if config.has_belt {
        // Strap: ellipsoid band (smooth cross-section)
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Belt,
            Mesh::from(Sphere::new(0.5)),
            suit_shadow.clone(),
            Transform::from_xyz(0.0, -0.38 * s * height, 0.0).with_scale(Vec3::new(
                0.80 * s * hip_width,
                0.11 * s,
                0.44 * s,
            )),
        );
        // Buckle: rounded ellipsoid
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Belt,
            Mesh::from(Sphere::new(0.5)),
            accent_glow.clone(),
            Transform::from_xyz(0.0, -0.38 * s * height, -0.24 * s).with_scale(Vec3::new(
                0.19 * s,
                0.17 * s,
                0.07 * s,
            )),
        );
    }

    // ── Head ──────────────────────────────────────────────────────────────────
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Head,
        Mesh::from(Sphere::new(0.34 * s * head_scale)),
        skin.clone(),
        Transform::from_xyz(0.0, head_y, -0.005 * s).with_scale(Vec3::new(0.92, 1.08, 0.88)),
    );

    // ── Hair ──────────────────────────────────────────────────────────────────
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hair,
        Mesh::from(Sphere::new(0.36 * s * head_scale)),
        hair.clone(),
        Transform::from_xyz(0.0, head_y + 0.15 * s * head_scale, -0.015 * s)
            .with_scale(Vec3::new(1.04, 0.48, 0.88)),
    );
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hair,
        Mesh::from(Cuboid::new(0.36 * s, 0.50 * s, 0.13 * s)),
        hair.clone(),
        Transform::from_xyz(0.0, head_y - 0.22 * s, 0.14 * s)
            .with_rotation(Quat::from_rotation_x(0.18)),
    );
    for (x, rot_z) in [(-0.28_f32, 0.16_f32), (0.28_f32, -0.16_f32)] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hair,
            Mesh::from(Cylinder::new(0.068 * s, 0.36 * s)),
            hair.clone(),
            Transform::from_xyz(x * s, head_y - 0.17 * s, 0.06 * s)
                .with_rotation(Quat::from_rotation_z(rot_z)),
        );
    }
    for (x, rot_z, rot_x, spike_s) in [
        (-0.16_f32, 0.35_f32, -0.85_f32, 0.95_f32),
        (0.0, 0.0, -1.05, 1.10),
        (0.16, -0.35, -0.85, 0.95),
    ] {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hair,
            Mesh::from(Cone {
                radius: 0.075 * s * spike_s,
                height: 0.34 * s * spike_s,
            }),
            hair.clone(),
            Transform::from_xyz(x * s, head_y + 0.18 * s, -0.18 * s)
                .with_rotation(Quat::from_rotation_x(rot_x) * Quat::from_rotation_z(rot_z)),
        );
    }
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Hair,
        Mesh::from(Cylinder::new(0.052 * s, 0.24 * s)),
        hair.clone(),
        Transform::from_xyz(0.0, head_y + 0.20 * s, -0.26 * s)
            .with_rotation(Quat::from_rotation_x(1.35)),
    );

    // ── Eyes (large anime planes with iris highlights) ────────────────────────
    for (kind, eye_x) in [
        (CartoonPartKind::LeftEye, -0.13_f32),
        (CartoonPartKind::RightEye, 0.13_f32),
    ] {
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Sphere::new(0.074 * s)),
            char_mat(materials, Color::srgb(0.95, 0.95, 0.97)),
            Transform::from_xyz(eye_x * s, eye_y, face_z).with_scale(Vec3::new(0.82, 1.22, 0.30)),
        );
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Sphere::new(0.047 * s)),
            eye_mat.clone(),
            Transform::from_xyz(eye_x * s, eye_y - 0.003 * s, face_z - 0.035 * s)
                .with_scale(Vec3::new(0.76, 1.20, 0.25)),
        );
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Sphere::new(0.014 * s)),
            emissive_mat(materials, Color::srgb(1.0, 1.0, 1.0), 1.8),
            Transform::from_xyz(eye_x * s - 0.020 * s, eye_y + 0.028 * s, face_z - 0.055 * s)
                .with_scale(Vec3::new(0.80, 1.0, 0.35)),
        );
    }

    // ── Eyebrows (bold inward tilt for personality) ───────────────────────────
    for (kind, brow_x, tilt_z) in [
        (CartoonPartKind::LeftEyebrow, -0.13_f32, 0.22_f32),
        (CartoonPartKind::RightEyebrow, 0.13_f32, -0.22_f32),
    ] {
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Cuboid::new(0.125 * s, 0.036 * s, 0.055 * s)),
            brow.clone(),
            Transform::from_xyz(brow_x * s, brow_y, face_z + 0.010 * s)
                .with_rotation(Quat::from_rotation_z(tilt_z)),
        );
    }

    // ── Nose ─────────────────────────────────────────────────────────────────
    spawn_part(
        commands,
        meshes,
        root,
        CartoonPartKind::Nose,
        Mesh::from(Sphere::new(0.050 * s)),
        skin.clone(),
        Transform::from_xyz(0.0, nose_y, face_z - 0.038 * s)
            .with_scale(Vec3::new(0.55, 0.78, 0.90)),
    );

    // ── Mouth (neutral set; line + lower-lip bump) ────────────────────────────
    if config.has_mouth {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Mouth,
            Mesh::from(Cuboid::new(0.135 * s, 0.030 * s, 0.055 * s)),
            char_mat(materials, Color::srgb(0.17, 0.08, 0.08)),
            Transform::from_xyz(0.0, mouth_y, face_z - 0.040 * s),
        );
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Mouth,
            Mesh::from(Sphere::new(0.033 * s)),
            skin.clone(),
            Transform::from_xyz(0.0, mouth_y - 0.026 * s, face_z - 0.050 * s)
                .with_scale(Vec3::new(2.6, 0.85, 1.0)),
        );
    }

    // ── Hood (default hero silhouette, sized for bigger head) ────────────────
    if config.has_hood {
        // Main hood shell
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hood,
            Mesh::from(Sphere::new(0.40 * s)),
            outfit.clone(),
            Transform::from_xyz(0.0, head_y + 0.15 * s, 0.01 * s)
                .with_scale(Vec3::new(1.02, 0.82, 0.96)),
        );
        // Face-frame: soft Capsule3d arc (rounded instead of hard Cuboid edge)
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hood,
            Mesh::from(Capsule3d::new(0.10 * s, 0.12 * s)),
            armor.clone(),
            Transform::from_xyz(0.0, head_y - 0.04 * s, -0.30 * s)
                .with_scale(Vec3::new(1.55, 0.50, 0.38))
                .with_rotation(Quat::from_rotation_x(-0.18)),
        );
    }

    // ── Hat (wizard only) ─────────────────────────────────────────────────────
    if config.has_hat {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hat,
            Mesh::from(Cone {
                radius: 0.36 * s,
                height: 0.72 * s,
            }),
            accent.clone(),
            Transform::from_xyz(0.0, 1.12 * s, 0.0),
        );
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Hat,
            Mesh::from(Cylinder::new(0.44 * s, 0.06 * s)),
            accent.clone(),
            Transform::from_xyz(0.0, 0.80 * s, 0.0),
        );
    }

    // ── Arms + Hands: capsule limbs with elbow joints ────────────────────────
    let arm_x = (0.34 + 0.075) * s * bw;
    for (kind, x) in [
        (CartoonPartKind::LeftArm, -arm_x),
        (CartoonPartKind::RightArm, arm_x),
    ] {
        // Upper arm
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Capsule3d::new(0.075 * s, 0.36 * s * arm_length)),
            outfit.clone(),
            Transform::from_xyz(x, 0.02 * s - (arm_length - 1.0) * 0.07 * s, 0.0)
                .with_scale(Vec3::new(0.92, 1.0, 0.82)),
        );
        // Forearm
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Capsule3d::new(0.085 * s, 0.34 * s * arm_length)),
            armor.clone(),
            Transform::from_xyz(x, -0.31 * s - (arm_length - 1.0) * 0.17 * s, -0.005 * s)
                .with_scale(Vec3::new(0.88, 1.0, 0.80)),
        );
        // Elbow joint: Sphere (replaces flat Cuboid band)
        spawn_part(
            commands,
            meshes,
            root,
            kind,
            Mesh::from(Sphere::new(0.090 * s)),
            armor.clone(),
            Transform::from_xyz(x, -0.14 * s - (arm_length - 1.0) * 0.12 * s, -0.010 * s),
        );
    }
    let hand_mat = if config.has_gloves {
        armor.clone()
    } else {
        skin.clone()
    };
    let cuff_mat = if config.has_gloves {
        accent.clone()
    } else {
        outfit.clone()
    };
    for (kind, x, sign) in [
        (CartoonPartKind::LeftHand, -arm_x, -1.0_f32),
        (CartoonPartKind::RightHand, arm_x, 1.0_f32),
    ] {
        spawn_cartoon_glove(
            commands,
            meshes,
            root,
            kind,
            hand_mat.clone(),
            cuff_mat.clone(),
            Vec3::new(x, -0.50 * s - (arm_length - 1.0) * 0.28 * s, -0.020 * s),
            sign,
            Vec3::new(
                0.134 * s * hand_scale,
                0.126 * s * hand_scale,
                0.126 * s * hand_scale,
            ),
            0.030 * s * hand_scale,
            0.142 * s * hand_scale,
            0.094 * s,
            0.046 * s,
        );
    }

    // ── Legs + Boots: anime legs, knee joints, rounded boots ─────────────────
    let leg_x = 0.165 * s * hip_width;
    let boot_mat = if config.has_boots {
        armor.clone()
    } else {
        outfit.clone()
    };
    for (leg_k, foot_k, x) in [
        (CartoonPartKind::LeftLeg, CartoonPartKind::LeftFoot, -leg_x),
        (CartoonPartKind::RightLeg, CartoonPartKind::RightFoot, leg_x),
    ] {
        // Thigh
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Capsule3d::new(0.090 * s, 0.42 * s * leg_length)),
            outfit.clone(),
            Transform::from_xyz(x, -0.67 * s - (leg_length - 1.0) * 0.13 * s, 0.0)
                .with_scale(Vec3::new(0.86, 1.0, 0.78)),
        );
        // Shin
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Capsule3d::new(0.086 * s, 0.38 * s * leg_length)),
            armor.clone(),
            Transform::from_xyz(x, -1.02 * s - (leg_length - 1.0) * 0.26 * s, -0.010 * s)
                .with_scale(Vec3::new(0.82, 1.0, 0.72)),
        );
        // Knee joint: Sphere
        spawn_part(
            commands,
            meshes,
            root,
            leg_k,
            Mesh::from(Sphere::new(0.100 * s)),
            armor.clone(),
            Transform::from_xyz(x, -0.86 * s - (leg_length - 1.0) * 0.20 * s, -0.015 * s),
        );
        let boot_kind = if x < 0.0 {
            CartoonPartKind::LeftBoot
        } else {
            CartoonPartKind::RightBoot
        };
        spawn_cartoon_shoe(
            commands,
            meshes,
            root,
            foot_k,
            Some(boot_kind),
            suit_shadow.clone(),
            boot_mat.clone(),
            accent_glow.clone(),
            Vec3::new(x, -1.24 * s - (leg_length - 1.0) * 0.31 * s, -0.10 * s),
            if x < 0.0 { -1.0 } else { 1.0 },
            s,
            foot_scale,
            0.88,
            1.04,
            config.has_boots,
        );
    }

    // ── Horns ─────────────────────────────────────────────────────────────────
    if config.has_horns {
        for (kind, x, rot_z) in [
            (CartoonPartKind::LeftHorn, -0.22_f32, 0.40_f32),
            (CartoonPartKind::RightHorn, 0.22_f32, -0.40_f32),
        ] {
            spawn_part(
                commands,
                meshes,
                root,
                kind,
                Mesh::from(Cone {
                    radius: 0.10 * s,
                    height: 0.48 * s,
                }),
                accent.clone(),
                Transform::from_xyz(x * s, 0.98 * s, 0.04 * s)
                    .with_rotation(Quat::from_rotation_z(rot_z)),
            );
        }
        if config.extra_horns {
            for (kind, x, rot_z, rot_x) in [
                (CartoonPartKind::LeftHorn, -0.14_f32, 0.22_f32, -0.30_f32),
                (CartoonPartKind::RightHorn, 0.14_f32, -0.22_f32, -0.30_f32),
            ] {
                spawn_part(
                    commands,
                    meshes,
                    root,
                    kind,
                    Mesh::from(Cone {
                        radius: 0.065 * s,
                        height: 0.30 * s,
                    }),
                    accent.clone(),
                    Transform::from_xyz(x * s, 0.88 * s, -0.16 * s)
                        .with_rotation(Quat::from_rotation_z(rot_z) * Quat::from_rotation_x(rot_x)),
                );
            }
        }
    }

    // ── Tail ──────────────────────────────────────────────────────────────────
    if config.has_tail {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Tail,
            Mesh::from(Cuboid::new(0.18 * s, 0.18 * s, 0.55 * s)),
            accent.clone(),
            Transform::from_xyz(0.0, -0.44 * s, 0.42 * s)
                .with_rotation(Quat::from_rotation_x(0.40)),
        );
        spawn_part(
            commands,
            meshes,
            root,
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
                commands,
                meshes,
                root,
                CartoonPartKind::SpineRidge,
                Mesh::from(Cone {
                    radius: ridge_s * s * 0.45,
                    height: ridge_s * s,
                }),
                accent.clone(),
                Transform::from_xyz(0.0, ridge_y, z * s + 0.15 * s)
                    .with_rotation(Quat::from_rotation_x(-0.15)),
            );
        }
    }

    // ── Star Badge (6-pointed chest reactor + raised gem) ────────────────────
    if config.has_star_badge {
        for rot_z in [0.0_f32, PI / 3.0, -PI / 3.0] {
            spawn_part(
                commands,
                meshes,
                root,
                CartoonPartKind::StarBadge,
                Mesh::from(Cuboid::new(0.23 * s, 0.060 * s, 0.040 * s)),
                accent_glow.clone(),
                Transform::from_xyz(0.0, 0.080 * s, -0.315 * s)
                    .with_rotation(Quat::from_rotation_z(rot_z)),
            );
        }
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::StarBadge,
            Mesh::from(Sphere::new(0.055 * s)),
            emissive_mat(materials, config.accent, 4.2),
            Transform::from_xyz(0.0, 0.080 * s, -0.345 * s).with_scale(Vec3::new(1.0, 1.0, 0.42)),
        );
    }

    // ── Tech Visor ────────────────────────────────────────────────────────────
    if config.has_visor {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Visor,
            Mesh::from(Cuboid::new(0.35 * s, 0.085 * s, 0.040 * s)),
            accent_glow.clone(),
            Transform::from_xyz(0.0, eye_y, face_z - 0.052 * s),
        );
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Visor,
            Mesh::from(Cuboid::new(0.24 * s, 0.020 * s, 0.026 * s)),
            emissive_mat(materials, Color::srgb(1.0, 1.0, 1.0), 1.5),
            Transform::from_xyz(0.0, eye_y + 0.012 * s, face_z - 0.080 * s),
        );
    }

    // ── Shoulder pads (dome pauldrons) ───────────────────────────────────────
    if config.has_shoulder_pads {
        for (kind, x, rot_z) in [
            (CartoonPartKind::ShoulderPadLeft, -arm_x, -0.18_f32),
            (CartoonPartKind::ShoulderPadRight, arm_x, 0.18_f32),
        ] {
            // Dome: squashed Sphere pauldron (no more flat Cuboid or pointy Cone)
            spawn_part(
                commands,
                meshes,
                root,
                kind,
                Mesh::from(Sphere::new(0.20 * s)),
                armor.clone(),
                Transform::from_xyz(x, 0.24 * s, -0.015 * s)
                    .with_scale(Vec3::new(1.0, 0.58, 0.92))
                    .with_rotation(Quat::from_rotation_z(rot_z)),
            );
            // Accent ring at dome base
            spawn_part(
                commands,
                meshes,
                root,
                kind,
                Mesh::from(Cylinder::new(0.185 * s, 0.038 * s)),
                accent_glow.clone(),
                Transform::from_xyz(x, 0.162 * s, -0.022 * s)
                    .with_rotation(Quat::from_rotation_z(rot_z)),
            );
        }
    }

    // ── Cape (segmented scarf-coat silhouette) ───────────────────────────────
    if config.has_cape {
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Cape,
            Mesh::from(Cuboid::new(0.56 * s, 0.10 * s, 0.060 * s)),
            armor.clone(),
            Transform::from_xyz(0.0, 0.22 * s, 0.19 * s),
        );
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Cape,
            Mesh::from(Cuboid::new(0.48 * s, 0.52 * s, 0.040 * s)),
            accent.clone(),
            Transform::from_xyz(0.0, -0.06 * s, 0.28 * s)
                .with_rotation(Quat::from_rotation_x(0.20)),
        );
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Cape,
            Mesh::from(Cuboid::new(0.34 * s, 0.44 * s, 0.035 * s)),
            outfit.clone(),
            Transform::from_xyz(0.0, -0.50 * s, 0.35 * s)
                .with_rotation(Quat::from_rotation_x(0.40)),
        );
        spawn_part(
            commands,
            meshes,
            root,
            CartoonPartKind::Cape,
            Mesh::from(Cuboid::new(0.20 * s, 0.24 * s, 0.030 * s)),
            accent.clone(),
            Transform::from_xyz(0.0, -0.74 * s, 0.40 * s)
                .with_rotation(Quat::from_rotation_x(0.55)),
        );
    }

    // Store visual config for runtime part swaps, then lift all spawned parts.
    let visual_cfg = CharacterVisualConfig {
        scale: s,
        body_width: bw,
        hip_width,
        arm_length,
        leg_length,
        hand_scale,
        foot_scale,
        outfit: config.outfit,
        accent: config.accent,
        skin: config.skin,
        hair: config.hair,
        eye: config.eye_color,
        has_gloves: config.has_gloves,
        has_boots: config.has_boots,
        visual_ground_lift,
    };
    commands
        .entity(root)
        .insert((CharacterLoadout::default(), visual_cfg));

    if visual_ground_lift > 0.001 {
        commands.queue(move |world: &mut World| {
            let mut parts = world.query::<(&mut Transform, &mut CartoonPart)>();
            for (mut transform, mut part) in parts.iter_mut(world) {
                if part.root == root {
                    transform.translation.y += visual_ground_lift;
                    part.base_translation.y += visual_ground_lift;
                }
            }
        });
    }
}

pub fn despawn_cartoon_character_parts(
    commands: &mut Commands,
    root: Entity,
    visuals: &Query<
        (Entity, Option<&CartoonPart>, Option<&JointMarker>),
        Or<(With<CartoonPart>, With<JointMarker>)>,
    >,
) {
    for (entity, part, marker) in visuals.iter() {
        let matches_root =
            part.is_some_and(|part| part.root == root) || marker.is_some_and(|m| m.root == root);
        if matches_root {
            commands.entity(entity).try_despawn();
        }
    }
    commands
        .entity(root)
        .remove::<(SkeletonRig, CharacterIkPose)>();
}

// ── Color presets for character designer ─────────────────────────────────────

pub const COLOR_PRESET_COUNT: usize = 8;

pub fn normalize_color_preset_index(index: usize) -> usize {
    index.min(COLOR_PRESET_COUNT - 1)
}

pub fn outfit_presets() -> [Color; 8] {
    [
        Color::srgb(0.09, 0.28, 0.80),
        Color::srgb(0.75, 0.12, 0.08),
        Color::srgb(0.08, 0.52, 0.20),
        Color::srgb(0.52, 0.10, 0.52),
        Color::srgb(0.80, 0.48, 0.06),
        Color::srgb(0.08, 0.42, 0.56),
        Color::srgb(0.18, 0.18, 0.22),
        Color::srgb(0.62, 0.56, 0.48),
    ]
}

pub fn accent_presets() -> [Color; 8] {
    [
        Color::srgb(0.95, 0.78, 0.12),
        Color::srgb(0.10, 0.92, 0.98),
        Color::srgb(0.78, 0.86, 0.92),
        Color::srgb(0.92, 0.90, 0.84),
        Color::srgb(0.95, 0.40, 0.10),
        Color::srgb(0.65, 0.92, 0.30),
        Color::srgb(0.98, 0.28, 0.60),
        Color::srgb(0.20, 0.80, 1.00),
    ]
}

pub fn hair_presets() -> [Color; 8] {
    [
        Color::srgb(0.06, 0.04, 0.02),
        Color::srgb(0.20, 0.12, 0.06),
        Color::srgb(0.45, 0.28, 0.12),
        Color::srgb(0.70, 0.45, 0.18),
        Color::srgb(0.88, 0.76, 0.32),
        Color::srgb(0.85, 0.85, 0.90),
        Color::srgb(0.60, 0.58, 0.56),
        Color::srgb(0.18, 0.05, 0.20),
    ]
}

pub fn outfit_preset(index: usize) -> Color {
    outfit_presets()[normalize_color_preset_index(index)]
}

pub fn accent_preset(index: usize) -> Color {
    accent_presets()[normalize_color_preset_index(index)]
}

pub fn hair_preset(index: usize) -> Color {
    hair_presets()[normalize_color_preset_index(index)]
}

/// Returns a hero config with per-slot customization overrides applied.
pub fn hero_config_with_overrides(
    name: &'static str,
    outfit: Option<Color>,
    accent: Option<Color>,
    hair: Option<Color>,
    has_hood: Option<bool>,
    has_cape: Option<bool>,
    has_gloves: Option<bool>,
    has_boots: Option<bool>,
    has_shoulder_pads: Option<bool>,
    has_visor: Option<bool>,
) -> CartoonCharacterConfig {
    let mut cfg = hero_config(name);
    if let Some(c) = outfit {
        cfg.outfit = c;
    }
    if let Some(c) = accent {
        cfg.accent = c;
    }
    if let Some(c) = hair {
        cfg.hair = c;
    }
    if let Some(v) = has_hood {
        cfg.has_hood = v;
    }
    if let Some(v) = has_cape {
        cfg.has_cape = v;
    }
    if let Some(v) = has_gloves {
        cfg.has_gloves = v;
    }
    if let Some(v) = has_boots {
        cfg.has_boots = v;
    }
    if let Some(v) = has_shoulder_pads {
        cfg.has_shoulder_pads = v;
    }
    if let Some(v) = has_visor {
        cfg.has_visor = v;
    }
    cfg
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
    let slot = PartSlotTag::from_kind(kind);
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

fn spawn_joint(
    commands: &mut Commands,
    rig: &mut SkeletonRig,
    root: Entity,
    parent: Entity,
    parent_rest: Vec3,
    kind: JointKind,
    rest_translation: Vec3,
) -> Entity {
    let local_translation = rest_translation - parent_rest;
    let entity = commands
        .spawn((
            Transform::from_translation(local_translation),
            GlobalTransform::default(),
            JointMarker {
                root,
                kind,
                local_translation,
                rest_translation,
            },
            Name::new(format!("Joint::{kind:?}")),
        ))
        .id();
    commands.entity(parent).add_child(entity);
    rig.joints.insert(kind, entity);
    entity
}

pub fn humanoid_joint_rest_translation(kind: JointKind, scale: f32, body: &BodyRecipe) -> Vec3 {
    let body = body.validated();
    let s = scale;
    let lift = visual_ground_lift(&body, s);
    let height = body.height;
    let shoulder_x = 0.415 * s * body.shoulder_width;
    let hip_x = 0.165 * s * body.hip_width;
    let arm_extra = (body.arm_length - 1.0) * s;
    let leg_extra = (body.leg_length - 1.0) * s;
    let head_y = 0.58 * s * height + (body.neck_length - 1.0) * 0.10 * s + lift;

    match kind {
        JointKind::Pelvis => Vec3::new(0.0, -0.48 * s * height + lift, 0.0),
        JointKind::Spine => Vec3::new(0.0, -0.18 * s * height + lift, 0.0),
        JointKind::Chest => Vec3::new(0.0, 0.12 * s * height + lift, -0.02 * s),
        JointKind::Neck => Vec3::new(0.0, 0.40 * s * height + lift, -0.01 * s),
        JointKind::Head => Vec3::new(0.0, head_y, -0.005 * s),
        JointKind::LeftShoulder => Vec3::new(-shoulder_x, 0.22 * s * height + lift, 0.0),
        JointKind::LeftElbow => {
            Vec3::new(-shoulder_x, -0.14 * s - arm_extra * 0.12 + lift, -0.01 * s)
        }
        JointKind::LeftWrist => {
            Vec3::new(-shoulder_x, -0.50 * s - arm_extra * 0.28 + lift, 0.02 * s)
        }
        JointKind::RightShoulder => Vec3::new(shoulder_x, 0.22 * s * height + lift, 0.0),
        JointKind::RightElbow => {
            Vec3::new(shoulder_x, -0.14 * s - arm_extra * 0.12 + lift, -0.01 * s)
        }
        JointKind::RightWrist => {
            Vec3::new(shoulder_x, -0.50 * s - arm_extra * 0.28 + lift, 0.02 * s)
        }
        JointKind::LeftHip => Vec3::new(-hip_x, -0.55 * s * height + lift, 0.0),
        JointKind::LeftKnee => Vec3::new(-hip_x, -0.86 * s - leg_extra * 0.20 + lift, -0.015 * s),
        JointKind::LeftAnkle => Vec3::new(-hip_x, -1.07 * s - leg_extra * 0.22 + lift, -0.025 * s),
        JointKind::RightHip => Vec3::new(hip_x, -0.55 * s * height + lift, 0.0),
        JointKind::RightKnee => Vec3::new(hip_x, -0.86 * s - leg_extra * 0.20 + lift, -0.015 * s),
        JointKind::RightAnkle => Vec3::new(hip_x, -1.07 * s - leg_extra * 0.22 + lift, -0.025 * s),
    }
}

fn visual_ground_lift(body: &BodyRecipe, scale: f32) -> f32 {
    // Mirror the scaled collider dimensions used in authored_player_defaults.
    let half_height = 0.6 * (body.height * 0.72 + body.leg_length * 0.28) * scale;
    let radius = 0.35
        * (body.shoulder_width * 0.55 + body.chest_size * 0.25 + body.hip_width * 0.20)
        * scale;
    let collider_bottom =
        -(half_height.clamp(0.44 * scale, 0.86 * scale) + radius.clamp(0.26 * scale, 0.50 * scale));
    let foot_center_y = -1.25 * scale - (body.leg_length - 1.0) * 0.31 * scale;
    let foot_bottom = foot_center_y - 0.06 * scale;

    (collider_bottom + 0.07 * scale - foot_bottom).clamp(0.0, 3.0)
}

/// Stylized PBR: satin low-poly surfaces with a faint self-emissive so
/// characters read clearly in shadowed areas.
/// Warm storybook shadow-lift so unlit faces glow softly instead of going black
/// — the painterly "filled shadow" of the Crystal Chronicles look.
fn warm_floor(lin: LinearRgba, k: f32) -> LinearRgba {
    let base = Vec3::new(lin.red, lin.green, lin.blue);
    let amber = Vec3::new(1.0, 0.72, 0.42);
    let tint = base.lerp(amber, 0.22) * k;
    LinearRgba::new(tint.x, tint.y, tint.z, 1.0)
}

/// Soft, matte, hand-painted material for skin / cloth / hair, with a touch of
/// subsurface light-bleed. Kept in lock-step with `soft_mat()` in character_parts.rs.
fn char_mat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
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

/// Brightly glowing crystal material for eyes, badge gems, visor, buckle —
/// a blooming core under a glassy clearcoat so it reads like a polished gem.
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
