use bevy::prelude::*;
use std::collections::HashMap;

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
    Hoverboard,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JointKind {
    Pelvis,
    Spine,
    Chest,
    Neck,
    Head,
    LeftShoulder,
    LeftElbow,
    LeftWrist,
    RightShoulder,
    RightElbow,
    RightWrist,
    LeftHip,
    LeftKnee,
    LeftAnkle,
    RightHip,
    RightKnee,
    RightAnkle,
}

impl JointKind {
    pub const HUMANOID: [JointKind; 17] = [
        JointKind::Pelvis,
        JointKind::Spine,
        JointKind::Chest,
        JointKind::Neck,
        JointKind::Head,
        JointKind::LeftShoulder,
        JointKind::LeftElbow,
        JointKind::LeftWrist,
        JointKind::RightShoulder,
        JointKind::RightElbow,
        JointKind::RightWrist,
        JointKind::LeftHip,
        JointKind::LeftKnee,
        JointKind::LeftAnkle,
        JointKind::RightHip,
        JointKind::RightKnee,
        JointKind::RightAnkle,
    ];
}

#[derive(Component, Debug, Clone, Copy)]
pub struct JointMarker {
    pub root: Entity,
    pub kind: JointKind,
    pub local_translation: Vec3,
    pub rest_translation: Vec3,
}

#[derive(Component, Debug, Clone, Default)]
pub struct SkeletonRig {
    pub joints: HashMap<JointKind, Entity>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct IkTarget {
    pub joint: JointKind,
    /// Root-local visual target. IK systems must never treat this as gameplay
    /// authority or move the character root to satisfy it.
    pub target: Vec3,
    pub pole: Option<Vec3>,
    pub weight: f32,
}

impl IkTarget {
    pub fn new(joint: JointKind, target: Vec3, weight: f32) -> Self {
        Self {
            joint,
            target,
            pole: None,
            weight: weight.clamp(0.0, 1.0),
        }
    }

    pub fn with_pole(mut self, pole: Vec3) -> Self {
        self.pole = Some(pole);
        self
    }
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterIkPose {
    pub left_foot: Option<IkTarget>,
    pub right_foot: Option<IkTarget>,
    pub left_hand: Option<IkTarget>,
    pub right_hand: Option<IkTarget>,
    pub look_at: Option<Vec3>,
}

#[derive(Component, Debug, Clone)]
pub struct CartoonPart {
    pub root: Entity,
    pub kind: CartoonPartKind,
    pub joint: Option<JointKind>,
    pub base_translation: Vec3,
    pub base_rotation: Quat,
    pub base_scale: Vec3,
}

impl CartoonPart {
    pub fn new(root: Entity, kind: CartoonPartKind, transform: &Transform) -> Self {
        Self::new_bound(root, kind, None, transform)
    }

    pub fn new_bound(
        root: Entity,
        kind: CartoonPartKind,
        joint: Option<JointKind>,
        transform: &Transform,
    ) -> Self {
        Self {
            root,
            kind,
            joint,
            base_translation: transform.translation,
            base_rotation: transform.rotation,
            base_scale: transform.scale,
        }
    }
}

pub fn default_joint_for_part(kind: CartoonPartKind) -> Option<JointKind> {
    match kind {
        CartoonPartKind::Shadow => None,
        CartoonPartKind::Body | CartoonPartKind::Belt | CartoonPartKind::StarBadge => {
            Some(JointKind::Chest)
        }
        CartoonPartKind::Cape | CartoonPartKind::SpineRidge => Some(JointKind::Spine),
        CartoonPartKind::Head
        | CartoonPartKind::Hair
        | CartoonPartKind::Hood
        | CartoonPartKind::Hat
        | CartoonPartKind::LeftEye
        | CartoonPartKind::RightEye
        | CartoonPartKind::LeftEyebrow
        | CartoonPartKind::RightEyebrow
        | CartoonPartKind::Nose
        | CartoonPartKind::Mouth
        | CartoonPartKind::Visor
        | CartoonPartKind::LeftHorn
        | CartoonPartKind::RightHorn => Some(JointKind::Head),
        CartoonPartKind::LeftArm | CartoonPartKind::ShoulderPadLeft => {
            Some(JointKind::LeftShoulder)
        }
        CartoonPartKind::RightArm | CartoonPartKind::ShoulderPadRight => {
            Some(JointKind::RightShoulder)
        }
        CartoonPartKind::LeftHand => Some(JointKind::LeftWrist),
        CartoonPartKind::RightHand => Some(JointKind::RightWrist),
        CartoonPartKind::LeftLeg => Some(JointKind::LeftHip),
        CartoonPartKind::RightLeg => Some(JointKind::RightHip),
        CartoonPartKind::LeftFoot | CartoonPartKind::LeftBoot => Some(JointKind::LeftAnkle),
        CartoonPartKind::RightFoot | CartoonPartKind::RightBoot => Some(JointKind::RightAnkle),
        CartoonPartKind::Tail => Some(JointKind::Pelvis),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanoid_joint_table_has_expected_count() {
        assert_eq!(JointKind::HUMANOID.len(), 17);
    }

    #[test]
    fn visual_parts_have_stable_default_joint_bindings() {
        assert_eq!(
            default_joint_for_part(CartoonPartKind::RightHand),
            Some(JointKind::RightWrist)
        );
        assert_eq!(
            default_joint_for_part(CartoonPartKind::LeftBoot),
            Some(JointKind::LeftAnkle)
        );
        assert_eq!(default_joint_for_part(CartoonPartKind::Shadow), None);
    }

    #[test]
    fn ik_target_weight_is_clamped() {
        assert_eq!(
            IkTarget::new(JointKind::LeftAnkle, Vec3::ZERO, 4.0).weight,
            1.0
        );
        assert_eq!(
            IkTarget::new(JointKind::RightWrist, Vec3::ZERO, -2.0).weight,
            0.0
        );
    }
}
