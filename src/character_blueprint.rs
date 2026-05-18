use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const CHARACTER_BLUEPRINT_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CharacterBlueprint {
    pub schema_version: u32,
    pub name: String,
    pub archetype: CharacterArchetype,
    pub body: BodyRecipe,
    #[serde(default)]
    pub cartoon_appearance: CartoonAppearanceRecipe,
    pub parts: Vec<PartRecipe>,
    pub materials: Vec<MaterialRecipe>,
    pub rig: RigRecipe,
    pub sockets: Vec<SocketRecipe>,
    pub animation_profile: AnimationProfile,
    pub movement_profile: MovementProfile,
    pub gameplay_stats: GameplayStatsRecipe,
    pub equipment: EquipmentRecipe,
    pub editor: CharacterEditorRecipe,
}

impl CharacterBlueprint {
    pub fn hero(
        name: &str,
        body: BodyRecipe,
        palette: CharacterPaletteRecipe,
        cartoon_appearance: CartoonAppearanceRecipe,
    ) -> Self {
        let body = body.validated();
        let mut blueprint = Self {
            schema_version: CHARACTER_BLUEPRINT_SCHEMA_VERSION,
            name: name.to_string(),
            archetype: CharacterArchetype::Human,
            body,
            cartoon_appearance,
            parts: default_humanoid_parts(),
            materials: default_hero_materials(palette),
            rig: RigRecipe::humanoid(),
            sockets: default_humanoid_sockets(),
            animation_profile: AnimationProfile::platformer_default(),
            movement_profile: MovementProfile::from_body(&body),
            gameplay_stats: GameplayStatsRecipe::from_body(&body),
            equipment: EquipmentRecipe::hero_default(),
            editor: CharacterEditorRecipe::default(),
        };
        blueprint.refresh_derived_profiles();
        blueprint
    }

    pub fn refresh_derived_profiles(&mut self) {
        self.body.validate();
        self.movement_profile = MovementProfile::from_body(&self.body);
        self.gameplay_stats = GameplayStatsRecipe::from_body(&self.body);
    }

    pub fn material_color(&self, id: &str) -> Option<Color> {
        self.materials
            .iter()
            .find(|material| material.id == id)
            .map(|material| material.base_color.to_color())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CharacterArchetype {
    #[default]
    Human,
    Android,
    Cyborg,
    Mech,
    Creature,
    Beast,
    Alien,
    Hybrid,
    Giant,
    SmallAgile,
    Flying,
    VehicleRider,
    BossScale,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct BodyRecipe {
    pub height: f32,
    pub shoulder_width: f32,
    pub chest_size: f32,
    pub arm_length: f32,
    pub leg_length: f32,
    pub hand_size: f32,
    pub foot_size: f32,
    pub head_size: f32,
    pub neck_length: f32,
    pub torso_curve: f32,
    pub hip_width: f32,
    pub spine_posture: f32,
    pub mass: f32,
    pub muscle: f32,
    pub body_fat: f32,
    pub asymmetry: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct CartoonAppearanceRecipe {
    pub has_hood: bool,
    pub has_cape: bool,
    pub has_gloves: bool,
    pub has_boots: bool,
    pub has_shoulder_pads: bool,
    pub has_visor: bool,
}

impl Default for CartoonAppearanceRecipe {
    fn default() -> Self {
        Self {
            has_hood: true,
            has_cape: true,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: false,
            has_visor: false,
        }
    }
}

impl Default for BodyRecipe {
    fn default() -> Self {
        Self {
            height: 1.0,
            shoulder_width: 1.0,
            chest_size: 1.0,
            arm_length: 1.0,
            leg_length: 1.0,
            hand_size: 1.0,
            foot_size: 1.0,
            head_size: 1.0,
            neck_length: 1.0,
            torso_curve: 0.0,
            hip_width: 1.0,
            spine_posture: 0.0,
            mass: 1.0,
            muscle: 1.0,
            body_fat: 1.0,
            asymmetry: 0.0,
        }
    }
}

impl BodyRecipe {
    pub fn validate(&mut self) {
        self.height = self.height.clamp(0.78, 1.28);
        self.shoulder_width = self.shoulder_width.clamp(0.72, 1.34);
        self.chest_size = self.chest_size.clamp(0.74, 1.30);
        self.arm_length = self.arm_length.clamp(0.72, 1.30);
        self.leg_length = self.leg_length.clamp(0.74, 1.30);
        self.hand_size = self.hand_size.clamp(0.70, 1.32);
        self.foot_size = self.foot_size.clamp(0.72, 1.36);
        self.head_size = self.head_size.clamp(0.76, 1.24);
        self.neck_length = self.neck_length.clamp(0.70, 1.25);
        self.torso_curve = self.torso_curve.clamp(-0.35, 0.35);
        self.hip_width = self.hip_width.clamp(0.74, 1.28);
        self.spine_posture = self.spine_posture.clamp(-0.35, 0.35);
        self.mass = self.mass.clamp(0.70, 1.45);
        self.muscle = self.muscle.clamp(0.70, 1.40);
        self.body_fat = self.body_fat.clamp(0.70, 1.40);
        self.asymmetry = self.asymmetry.clamp(0.0, 0.35);
    }

    pub fn validated(mut self) -> Self {
        self.validate();
        self
    }

    pub fn reach_multiplier(&self) -> f32 {
        (self.arm_length * 0.70 + self.height * 0.20 + self.hand_size * 0.10).clamp(0.75, 1.35)
    }

    pub fn stride_multiplier(&self) -> f32 {
        (self.leg_length * 0.62 + self.height * 0.24 + self.foot_size * 0.14).clamp(0.76, 1.35)
    }

    pub fn agility_multiplier(&self) -> f32 {
        ((1.16 / self.mass) * (1.04 / self.height) * (1.05 / self.body_fat)).clamp(0.72, 1.32)
    }

    pub fn armor_capacity_multiplier(&self) -> f32 {
        (self.shoulder_width * 0.35
            + self.chest_size * 0.30
            + self.mass * 0.25
            + self.muscle * 0.10)
            .clamp(0.76, 1.45)
    }

    pub fn knockback_resistance_multiplier(&self) -> f32 {
        (self.mass * 0.65 + self.muscle * 0.25 + self.foot_size * 0.10).clamp(0.75, 1.55)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Axis {
    #[default]
    X,
    Y,
    Z,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ShapePrimitive {
    Cuboid { size: Vec3Recipe },
    Sphere { radius: f32 },
    Capsule { radius: f32, half_height: f32 },
    Cylinder { radius: f32, height: f32 },
    Cone { radius: f32, height: f32 },
    BeveledBox { size: Vec3Recipe, bevel: f32 },
    LowPolyArmorPlate { size: Vec3Recipe, bevel: f32 },
    CurvedPanel { size: Vec3Recipe, curve: f32 },
    MechanicalJoint { radius: f32, thickness: f32 },
    Turret { radius: f32, barrel_length: f32 },
    Wing { span: f32, chord: f32 },
    Spike { radius: f32, length: f32 },
    EnergyCore { radius: f32 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MeshModifier {
    ScaleByAxis { axis: Axis, factor: f32 },
    Mirror { axis: Axis },
    Twist { axis: Axis, angle: f32 },
    Bend { axis: Axis, amount: f32 },
    Taper { axis: Axis, strength: f32 },
    Inflate { amount: f32 },
    Extrude { amount: f32 },
    Bevel { amount: f32, segments: u32 },
    Array { count: u32, offset: Vec3Recipe },
    RadialDuplicate { count: u32, radius: f32, axis: Axis },
    LatticeDeform { strength: Vec3Recipe },
    Noise { strength: f32, frequency: f32 },
    SymmetryLock { axis: Axis },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PartRecipe {
    pub id: String,
    pub layer: CharacterLayer,
    pub parent_socket: String,
    pub primitive: ShapePrimitive,
    pub transform: TransformRecipe,
    pub modifiers: Vec<MeshModifier>,
    pub material_id: String,
    pub collision: CollisionRecipe,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CharacterLayer {
    #[default]
    BaseBody,
    Armor,
    Clothing,
    Mechanical,
    Weapon,
    Fx,
    Collision,
    AnimationControl,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MaterialRecipe {
    pub id: String,
    pub base_color: ColorRecipe,
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: ColorRecipe,
    pub emissive_strength: f32,
    pub material_kind: MaterialKind,
    pub team_color: Option<ColorRecipe>,
    pub damage_overlay: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MaterialKind {
    #[default]
    Skin,
    Armor,
    Cloth,
    Hair,
    Energy,
    Metal,
    Rubber,
    Decal,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SocketRecipe {
    pub id: String,
    pub parent_bone: Option<String>,
    pub transform: TransformRecipe,
    pub symmetry_partner: Option<String>,
    pub allowed_layers: Vec<CharacterLayer>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RigRecipe {
    pub rig_kind: RigKind,
    pub bones: Vec<BoneRecipe>,
    pub constraints: Vec<BoneConstraintRecipe>,
    pub ik_targets: Vec<IkTargetRecipe>,
}

impl RigRecipe {
    pub fn humanoid() -> Self {
        Self {
            rig_kind: RigKind::Humanoid,
            bones: HUMANOID_BONES
                .iter()
                .map(|id| BoneRecipe {
                    id: (*id).to_string(),
                    parent: humanoid_parent(id).map(str::to_string),
                    rest: TransformRecipe::identity(),
                    chain_color: ChainColor::Default,
                })
                .collect(),
            constraints: Vec::new(),
            ik_targets: vec![
                IkTargetRecipe::new("left_foot_ik", "left_foot"),
                IkTargetRecipe::new("right_foot_ik", "right_foot"),
                IkTargetRecipe::new("left_hand_ik", "left_hand"),
                IkTargetRecipe::new("right_hand_ik", "right_hand"),
            ],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RigKind {
    #[default]
    Humanoid,
    Mechanical,
    Creature,
    Hybrid,
    Boss,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BoneRecipe {
    pub id: String,
    pub parent: Option<String>,
    pub rest: TransformRecipe,
    pub chain_color: ChainColor,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChainColor {
    #[default]
    Default,
    Spine,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Head,
    Tail,
    Wing,
    Weapon,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BoneConstraintRecipe {
    LimitRotation {
        bone: String,
        min: Vec3Recipe,
        max: Vec3Recipe,
    },
    CopyRotation {
        bone: String,
        target: String,
        influence: f32,
    },
    CopyPosition {
        bone: String,
        target: String,
        influence: f32,
    },
    Aim {
        bone: String,
        target: String,
        axis: Axis,
    },
    LookAt {
        bone: String,
        target: String,
        influence: f32,
    },
    Parent {
        bone: String,
        target: String,
        keep_offset: bool,
    },
    PoleVector {
        bone: String,
        target: String,
    },
    Stretch {
        bone: String,
        target: String,
        max_scale: f32,
    },
    Spring {
        bone: String,
        stiffness: f32,
        damping: f32,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IkTargetRecipe {
    pub id: String,
    pub end_bone: String,
    pub pole: Option<String>,
    pub chain_length: u8,
    pub weight: f32,
}

impl IkTargetRecipe {
    fn new(id: &str, end_bone: &str) -> Self {
        Self {
            id: id.to_string(),
            end_bone: end_bone.to_string(),
            pole: None,
            chain_length: 2,
            weight: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnimationProfile {
    pub states: Vec<AnimationStateRecipe>,
    pub layers: Vec<AnimationLayerRecipe>,
    pub events: Vec<AnimationEventRecipe>,
    pub pose_library: Vec<PoseRecipe>,
}

impl AnimationProfile {
    pub fn platformer_default() -> Self {
        Self {
            states: [
                AnimationStateKind::Idle,
                AnimationStateKind::Locomotion,
                AnimationStateKind::Jumping,
                AnimationStateKind::Falling,
                AnimationStateKind::Landing,
                AnimationStateKind::Climbing,
                AnimationStateKind::Combat,
                AnimationStateKind::Hurt,
                AnimationStateKind::Dead,
            ]
            .into_iter()
            .map(|kind| AnimationStateRecipe {
                kind,
                clip_id: format!("{kind:?}").to_lowercase(),
                blend_seconds: 0.12,
                looped: !matches!(kind, AnimationStateKind::Landing | AnimationStateKind::Dead),
            })
            .collect(),
            layers: vec![
                AnimationLayerRecipe::new(AnimationLayerKind::FullBody, 1.0),
                AnimationLayerRecipe::new(AnimationLayerKind::UpperBody, 0.0),
                AnimationLayerRecipe::new(AnimationLayerKind::Weapon, 0.0),
                AnimationLayerRecipe::new(AnimationLayerKind::AdditiveFx, 0.0),
            ],
            events: vec![
                AnimationEventRecipe::new("footstep_left", AnimationEventKind::Footstep, 0.18),
                AnimationEventRecipe::new("footstep_right", AnimationEventKind::Footstep, 0.68),
                AnimationEventRecipe::new("attack_hit", AnimationEventKind::AttackHitFrame, 0.42),
                AnimationEventRecipe::new("jump_launch", AnimationEventKind::JumpLaunch, 0.08),
                AnimationEventRecipe::new(
                    "landing_impact",
                    AnimationEventKind::LandingImpact,
                    0.12,
                ),
            ],
            pose_library: vec![
                PoseRecipe::named("idle"),
                PoseRecipe::named("run"),
                PoseRecipe::named("jump"),
                PoseRecipe::named("hang"),
                PoseRecipe::named("combat"),
            ],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnimationStateRecipe {
    pub kind: AnimationStateKind,
    pub clip_id: String,
    pub blend_seconds: f32,
    pub looped: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnimationStateKind {
    #[default]
    Idle,
    Locomotion,
    Jumping,
    Falling,
    Landing,
    Climbing,
    Swimming,
    Flying,
    Combat,
    Hurt,
    Dead,
    Cinematic,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnimationLayerRecipe {
    pub kind: AnimationLayerKind,
    pub weight: f32,
}

impl AnimationLayerRecipe {
    fn new(kind: AnimationLayerKind, weight: f32) -> Self {
        Self { kind, weight }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnimationLayerKind {
    #[default]
    FullBody,
    UpperBody,
    LowerBody,
    HeadNeck,
    Arms,
    Hands,
    Facial,
    Weapon,
    AdditiveFx,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnimationEventRecipe {
    pub id: String,
    pub kind: AnimationEventKind,
    pub normalized_time: f32,
}

impl AnimationEventRecipe {
    fn new(id: &str, kind: AnimationEventKind, normalized_time: f32) -> Self {
        Self {
            id: id.to_string(),
            kind,
            normalized_time,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnimationEventKind {
    #[default]
    Footstep,
    AttackHitFrame,
    WeaponTrailStart,
    WeaponTrailStop,
    JumpLaunch,
    LandingImpact,
    SoundTrigger,
    ParticleTrigger,
    CameraShake,
    DamageWindow,
    InvulnerabilityWindow,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PoseRecipe {
    pub id: String,
    pub keyed_bones: Vec<String>,
}

impl PoseRecipe {
    fn named(id: &str) -> Self {
        Self {
            id: id.to_string(),
            keyed_bones: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct MovementProfile {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub jump_force: f32,
    pub dodge_speed: f32,
    pub dodge_cost: f32,
    pub air_control: f32,
    pub reach_multiplier: f32,
    pub knockback_resistance: f32,
    pub armor_capacity: f32,
}

impl MovementProfile {
    pub fn from_body(body: &BodyRecipe) -> Self {
        let stride = body.stride_multiplier();
        let agility = body.agility_multiplier();
        Self {
            walk_speed: 0.38 * (stride * 0.72 + agility * 0.28),
            sprint_speed: 0.70 * (stride * 0.62 + agility * 0.38),
            jump_force: 0.64 * (body.leg_length * 0.44 + agility * 0.36 + body.foot_size * 0.20),
            dodge_speed: 1.20 * agility,
            dodge_cost: 20.0 * (1.18 / agility).clamp(0.82, 1.35),
            air_control: 1.45 * agility,
            reach_multiplier: body.reach_multiplier(),
            knockback_resistance: body.knockback_resistance_multiplier(),
            armor_capacity: body.armor_capacity_multiplier(),
        }
    }
}

impl Default for MovementProfile {
    fn default() -> Self {
        Self::from_body(&BodyRecipe::default())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct GameplayStatsRecipe {
    pub max_health: f32,
    pub max_stamina: f32,
    pub max_armor: f32,
    pub melee_reach: f32,
    pub knockback_resistance: f32,
    pub armor_capacity: f32,
}

impl GameplayStatsRecipe {
    pub fn from_body(body: &BodyRecipe) -> Self {
        Self {
            max_health: 100.0 * (body.mass * 0.45 + body.muscle * 0.35 + body.height * 0.20),
            max_stamina: 100.0 * body.agility_multiplier(),
            max_armor: 100.0 * body.armor_capacity_multiplier(),
            melee_reach: body.reach_multiplier(),
            knockback_resistance: body.knockback_resistance_multiplier(),
            armor_capacity: body.armor_capacity_multiplier(),
        }
    }
}

impl Default for GameplayStatsRecipe {
    fn default() -> Self {
        Self::from_body(&BodyRecipe::default())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EquipmentRecipe {
    pub armor_slots: Vec<EquipmentSlotRecipe>,
    pub weapon_sockets: Vec<String>,
}

impl EquipmentRecipe {
    fn hero_default() -> Self {
        Self {
            armor_slots: [
                "helmet",
                "chest",
                "back",
                "left_shoulder",
                "right_shoulder",
                "left_forearm",
                "right_forearm",
                "gloves",
                "belt",
                "left_boot",
                "right_boot",
                "cape",
                "wings",
                "jetpack",
            ]
            .into_iter()
            .map(|id| EquipmentSlotRecipe {
                id: id.to_string(),
                socket_id: id.to_string(),
                equipped_part_id: None,
                weight: 0.0,
                defense: 0.0,
            })
            .collect(),
            weapon_sockets: vec![
                "left_weapon".to_string(),
                "right_weapon".to_string(),
                "back_weapon".to_string(),
            ],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EquipmentSlotRecipe {
    pub id: String,
    pub socket_id: String,
    pub equipped_part_id: Option<String>,
    pub weight: f32,
    pub defense: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CharacterEditorRecipe {
    pub active_layer: CharacterLayer,
    pub visible_layers: Vec<CharacterLayer>,
    pub selected_part_id: Option<String>,
    pub gizmo_mode: TransformGizmoMode,
    pub transform_space: TransformSpace,
    pub snap_to_grid: bool,
    pub snap_to_socket: bool,
    pub mirror_axis: Option<Axis>,
    pub symmetry_lock: bool,
}

impl Default for CharacterEditorRecipe {
    fn default() -> Self {
        Self {
            active_layer: CharacterLayer::BaseBody,
            visible_layers: vec![
                CharacterLayer::BaseBody,
                CharacterLayer::Armor,
                CharacterLayer::Clothing,
                CharacterLayer::Mechanical,
                CharacterLayer::Weapon,
                CharacterLayer::Fx,
            ],
            selected_part_id: None,
            gizmo_mode: TransformGizmoMode::Move,
            transform_space: TransformSpace::Local,
            snap_to_grid: true,
            snap_to_socket: true,
            mirror_axis: Some(Axis::X),
            symmetry_lock: true,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransformGizmoMode {
    #[default]
    Move,
    Rotate,
    Scale,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransformSpace {
    #[default]
    Local,
    World,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CollisionRecipe {
    None,
    Capsule { radius: f32, half_height: f32 },
    Cuboid { half_extents: Vec3Recipe },
    Sphere { radius: f32 },
    MeshProxy { convex: bool },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub struct Vec3Recipe {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3Recipe {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn to_vec3(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

impl From<Vec3> for Vec3Recipe {
    fn from(v: Vec3) -> Self {
        Self::new(v.x, v.y, v.z)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct TransformRecipe {
    pub translation: Vec3Recipe,
    pub rotation_euler: Vec3Recipe,
    pub scale: Vec3Recipe,
}

impl TransformRecipe {
    pub const fn identity() -> Self {
        Self {
            translation: Vec3Recipe::ZERO,
            rotation_euler: Vec3Recipe::ZERO,
            scale: Vec3Recipe::ONE,
        }
    }

    pub fn from_translation(translation: Vec3Recipe) -> Self {
        Self {
            translation,
            ..Self::identity()
        }
    }

    pub fn to_transform(self) -> Transform {
        Transform {
            translation: self.translation.to_vec3(),
            rotation: Quat::from_euler(
                EulerRot::XYZ,
                self.rotation_euler.x,
                self.rotation_euler.y,
                self.rotation_euler.z,
            ),
            scale: self.scale.to_vec3(),
        }
    }
}

impl Default for TransformRecipe {
    fn default() -> Self {
        Self::identity()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ColorRecipe {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl ColorRecipe {
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_color(color: Color) -> Self {
        let c = color.to_srgba();
        Self::new(c.red, c.green, c.blue, c.alpha)
    }

    pub fn to_color(self) -> Color {
        Color::srgba(self.r, self.g, self.b, self.a)
    }
}

impl Default for ColorRecipe {
    fn default() -> Self {
        Self::WHITE
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CharacterPaletteRecipe {
    pub skin: Color,
    pub outfit: Color,
    pub accent: Color,
    pub hair: Color,
    pub eye: Color,
}

fn default_hero_materials(palette: CharacterPaletteRecipe) -> Vec<MaterialRecipe> {
    [
        ("skin", palette.skin, MaterialKind::Skin, 0.0),
        ("outfit", palette.outfit, MaterialKind::Cloth, 0.0),
        ("accent", palette.accent, MaterialKind::Energy, 1.6),
        ("hair", palette.hair, MaterialKind::Hair, 0.0),
        ("eye", palette.eye, MaterialKind::Energy, 2.2),
    ]
    .into_iter()
    .map(|(id, color, material_kind, glow)| MaterialRecipe {
        id: id.to_string(),
        base_color: ColorRecipe::from_color(color),
        metallic: if matches!(material_kind, MaterialKind::Armor | MaterialKind::Metal) {
            0.35
        } else {
            0.0
        },
        roughness: 0.72,
        emissive: ColorRecipe::from_color(color),
        emissive_strength: glow,
        material_kind,
        team_color: Some(ColorRecipe::from_color(palette.accent)),
        damage_overlay: 0.0,
    })
    .collect()
}

fn default_humanoid_parts() -> Vec<PartRecipe> {
    vec![
        PartRecipe {
            id: "body".to_string(),
            layer: CharacterLayer::BaseBody,
            parent_socket: "chest".to_string(),
            primitive: ShapePrimitive::Cuboid {
                size: Vec3Recipe::new(0.72, 0.86, 0.42),
            },
            transform: TransformRecipe::from_translation(Vec3Recipe::new(0.0, -0.12, 0.0)),
            modifiers: vec![
                MeshModifier::Taper {
                    axis: Axis::Y,
                    strength: 0.08,
                },
                MeshModifier::SymmetryLock { axis: Axis::X },
            ],
            material_id: "outfit".to_string(),
            collision: CollisionRecipe::Capsule {
                radius: 0.35,
                half_height: 0.60,
            },
        },
        PartRecipe {
            id: "head".to_string(),
            layer: CharacterLayer::BaseBody,
            parent_socket: "head".to_string(),
            primitive: ShapePrimitive::Sphere { radius: 0.38 },
            transform: TransformRecipe::from_translation(Vec3Recipe::new(0.0, 0.58, 0.0)),
            modifiers: vec![MeshModifier::SymmetryLock { axis: Axis::X }],
            material_id: "skin".to_string(),
            collision: CollisionRecipe::Sphere { radius: 0.38 },
        },
        PartRecipe {
            id: "cape_panel".to_string(),
            layer: CharacterLayer::Clothing,
            parent_socket: "back".to_string(),
            primitive: ShapePrimitive::Cuboid {
                size: Vec3Recipe::new(0.60, 0.56, 0.05),
            },
            transform: TransformRecipe::from_translation(Vec3Recipe::new(0.0, -0.06, 0.28)),
            modifiers: vec![MeshModifier::Bend {
                axis: Axis::X,
                amount: 0.20,
            }],
            material_id: "accent".to_string(),
            collision: CollisionRecipe::None,
        },
    ]
}

fn default_humanoid_sockets() -> Vec<SocketRecipe> {
    let data = [
        ("head", Some("head"), Vec3Recipe::new(0.0, 0.58, 0.0)),
        ("chest", Some("spine_2"), Vec3Recipe::new(0.0, 0.05, -0.24)),
        ("spine", Some("spine_2"), Vec3Recipe::new(0.0, 0.10, 0.0)),
        (
            "left_shoulder",
            Some("left_upper_arm"),
            Vec3Recipe::new(-0.45, 0.22, 0.0),
        ),
        (
            "right_shoulder",
            Some("right_upper_arm"),
            Vec3Recipe::new(0.45, 0.22, 0.0),
        ),
        (
            "left_wrist",
            Some("left_hand"),
            Vec3Recipe::new(-0.46, -0.50, 0.0),
        ),
        (
            "right_wrist",
            Some("right_hand"),
            Vec3Recipe::new(0.46, -0.50, 0.0),
        ),
        (
            "left_hip",
            Some("left_thigh"),
            Vec3Recipe::new(-0.20, -0.50, 0.0),
        ),
        (
            "right_hip",
            Some("right_thigh"),
            Vec3Recipe::new(0.20, -0.50, 0.0),
        ),
        (
            "left_knee",
            Some("left_calf"),
            Vec3Recipe::new(-0.20, -0.92, 0.0),
        ),
        (
            "right_knee",
            Some("right_calf"),
            Vec3Recipe::new(0.20, -0.92, 0.0),
        ),
        (
            "left_ankle",
            Some("left_foot"),
            Vec3Recipe::new(-0.20, -1.20, 0.0),
        ),
        (
            "right_ankle",
            Some("right_foot"),
            Vec3Recipe::new(0.20, -1.20, 0.0),
        ),
        ("back", Some("spine_2"), Vec3Recipe::new(0.0, 0.10, 0.28)),
        (
            "left_weapon",
            Some("left_hand"),
            Vec3Recipe::new(-0.46, -0.52, -0.10),
        ),
        (
            "right_weapon",
            Some("right_hand"),
            Vec3Recipe::new(0.46, -0.52, -0.10),
        ),
        (
            "wing_left",
            Some("spine_2"),
            Vec3Recipe::new(-0.32, 0.10, 0.32),
        ),
        (
            "wing_right",
            Some("spine_2"),
            Vec3Recipe::new(0.32, 0.10, 0.32),
        ),
        ("tail", Some("pelvis"), Vec3Recipe::new(0.0, -0.44, 0.42)),
    ];

    data.into_iter()
        .map(|(id, parent_bone, translation)| SocketRecipe {
            id: id.to_string(),
            parent_bone: parent_bone.map(str::to_string),
            transform: TransformRecipe::from_translation(translation),
            symmetry_partner: symmetry_partner(id).map(str::to_string),
            allowed_layers: vec![
                CharacterLayer::BaseBody,
                CharacterLayer::Armor,
                CharacterLayer::Clothing,
                CharacterLayer::Mechanical,
                CharacterLayer::Weapon,
                CharacterLayer::Fx,
            ],
        })
        .collect()
}

const HUMANOID_BONES: &[&str] = &[
    "root",
    "pelvis",
    "spine_1",
    "spine_2",
    "spine_3",
    "neck",
    "head",
    "left_clavicle",
    "left_upper_arm",
    "left_forearm",
    "left_hand",
    "right_clavicle",
    "right_upper_arm",
    "right_forearm",
    "right_hand",
    "left_thigh",
    "left_calf",
    "left_foot",
    "left_toes",
    "right_thigh",
    "right_calf",
    "right_foot",
    "right_toes",
];

fn humanoid_parent(id: &str) -> Option<&'static str> {
    match id {
        "pelvis" => Some("root"),
        "spine_1" => Some("pelvis"),
        "spine_2" => Some("spine_1"),
        "spine_3" => Some("spine_2"),
        "neck" => Some("spine_3"),
        "head" => Some("neck"),
        "left_clavicle" | "right_clavicle" => Some("spine_3"),
        "left_upper_arm" => Some("left_clavicle"),
        "left_forearm" => Some("left_upper_arm"),
        "left_hand" => Some("left_forearm"),
        "right_upper_arm" => Some("right_clavicle"),
        "right_forearm" => Some("right_upper_arm"),
        "right_hand" => Some("right_forearm"),
        "left_thigh" => Some("pelvis"),
        "left_calf" => Some("left_thigh"),
        "left_foot" => Some("left_calf"),
        "left_toes" => Some("left_foot"),
        "right_thigh" => Some("pelvis"),
        "right_calf" => Some("right_thigh"),
        "right_foot" => Some("right_calf"),
        "right_toes" => Some("right_foot"),
        _ => None,
    }
}

fn symmetry_partner(id: &str) -> Option<&'static str> {
    match id {
        "left_shoulder" => Some("right_shoulder"),
        "right_shoulder" => Some("left_shoulder"),
        "left_wrist" => Some("right_wrist"),
        "right_wrist" => Some("left_wrist"),
        "left_hip" => Some("right_hip"),
        "right_hip" => Some("left_hip"),
        "left_knee" => Some("right_knee"),
        "right_knee" => Some("left_knee"),
        "left_ankle" => Some("right_ankle"),
        "right_ankle" => Some("left_ankle"),
        "left_weapon" => Some("right_weapon"),
        "right_weapon" => Some("left_weapon"),
        "wing_left" => Some("wing_right"),
        "wing_right" => Some("wing_left"),
        _ => None,
    }
}
