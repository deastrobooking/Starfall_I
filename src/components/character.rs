use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CartoonRole {
    Hero,
    Wizard,
    Alien,
    Dragon,
    Rival,
}

#[derive(Component, Debug, Clone)]
pub struct CartoonCharacter {
    pub name: String,
    pub role: CartoonRole,
    pub scale: f32,
    pub stride: f32,
    pub agility: f32,
    pub visual_ground_lift: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CartoonPose {
    #[default]
    Idle,
    Walk,
    Run,
    Sprint,
    Jump,
    Fall,
    Fly,
    Glide,
    Hover,
    AirDash,
    WallSlide,
    Mantle,
    Grapple,
    Swing,
    Zip,
    Roll,
    Parry,
    Attack,
    SabreSlash,
    HardLand,
    Stun,
    Death,
    Hang,
}

#[derive(Component, Debug, Clone)]
pub struct CartoonAnimator {
    pub phase: f32,
    pub last_position: Vec3,
    pub speed: f32,
    pub pose: CartoonPose,
}

impl CartoonAnimator {
    pub fn new(position: Vec3) -> Self {
        Self {
            phase: 0.0,
            last_position: position,
            speed: 0.0,
            pose: CartoonPose::Idle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CartoonPartKind {
    Shadow,
    Body,
    Head,
    Hair,
    Hood,
    Hat,
    LeftEye,
    RightEye,
    LeftEyebrow,
    RightEyebrow,
    Nose,
    Mouth,
    Visor,
    LeftArm,
    RightArm,
    LeftHand,
    RightHand,
    LeftLeg,
    RightLeg,
    LeftFoot,
    RightFoot,
    LeftBoot,
    RightBoot,
    LeftHorn,
    RightHorn,
    ShoulderPadLeft,
    ShoulderPadRight,
    Cape,
    SpineRidge,
    Tail,
    StarBadge,
    Belt,
}

#[derive(Component, Debug, Clone)]
pub struct CartoonPart {
    pub root: Entity,
    pub kind: CartoonPartKind,
    pub base_translation: Vec3,
    pub base_rotation: Quat,
    pub base_scale: Vec3,
}

impl CartoonPart {
    pub fn new(root: Entity, kind: CartoonPartKind, transform: &Transform) -> Self {
        Self {
            root,
            kind,
            base_translation: transform.translation,
            base_rotation: transform.rotation,
            base_scale: transform.scale,
        }
    }
}
