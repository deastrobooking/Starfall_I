//! Contract between external skinned glTF humanoids and Starfall's stable
//! character specification. The bridge is deliberately data-only: gameplay
//! continues to own the canonical [`JointKind`] skeleton, while an imported
//! scene may supply a skinned visual when it satisfies this mapping.

use std::collections::{HashMap, HashSet};

use crate::components::character::JointKind;

/// Stable morph names emitted by `generators.rs`. A production Blender file
/// should export shape keys with these exact names. Aliases belong in the asset
/// conversion pipeline, not in player save data.
pub const CANONICAL_MORPHS: [&str; 15] = [
    "body_height",
    "body_muscle",
    "body_weight",
    "shoulders_wide",
    "waist_width",
    "hips_wide",
    "limb_length",
    "face_jaw_wide",
    "face_chin_long",
    "face_nose_long",
    "face_nose_wide",
    "face_brow_heavy",
    "face_cheek_full",
    "face_eye_large",
    "face_mouth_wide",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigValidation {
    pub mapped: HashMap<JointKind, String>,
    pub missing: Vec<JointKind>,
    pub duplicate_targets: Vec<JointKind>,
}

impl RigValidation {
    pub fn is_usable(&self) -> bool {
        self.missing.is_empty() && self.duplicate_targets.is_empty()
    }
}

/// Map the AMP armature naming convention and the documented `SF_*`
/// production convention onto the runtime's 17-joint humanoid contract.
pub fn canonical_joint_for_bone(name: &str) -> Option<JointKind> {
    let normalized = name
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-', '.'], "_");
    Some(match normalized.as_str() {
        "sf_pelvis" | "pelvis" => JointKind::Pelvis,
        "sf_spine" | "spine01" | "spine_01" => JointKind::Spine,
        "sf_chest" | "spine02" | "spine_02" | "chest" => JointKind::Chest,
        "sf_neck" | "neck" | "necktwist01" | "neck_twist_01" => JointKind::Neck,
        "sf_head" | "head" => JointKind::Head,
        "sf_shoulder_l" | "l_upperarm" | "leftupperarm" => JointKind::LeftShoulder,
        "sf_elbow_l" | "l_forearm" | "leftforearm" => JointKind::LeftElbow,
        "sf_wrist_l" | "l_hand" | "lefthand" => JointKind::LeftWrist,
        "sf_shoulder_r" | "r_upperarm" | "rightupperarm" => JointKind::RightShoulder,
        "sf_elbow_r" | "r_forearm" | "rightforearm" => JointKind::RightElbow,
        "sf_wrist_r" | "r_hand" | "righthand" => JointKind::RightWrist,
        "sf_hip_l" | "l_thigh" | "leftupleg" => JointKind::LeftHip,
        "sf_knee_l" | "l_calf" | "leftleg" => JointKind::LeftKnee,
        "sf_ankle_l" | "l_foot" | "leftfoot" => JointKind::LeftAnkle,
        "sf_hip_r" | "r_thigh" | "rightupleg" => JointKind::RightHip,
        "sf_knee_r" | "r_calf" | "rightleg" => JointKind::RightKnee,
        "sf_ankle_r" | "r_foot" | "rightfoot" => JointKind::RightAnkle,
        _ => return None,
    })
}

pub fn validate_humanoid_bones<'a>(names: impl IntoIterator<Item = &'a str>) -> RigValidation {
    let mut mapped = HashMap::new();
    let mut duplicates = HashSet::new();
    for name in names {
        let Some(joint) = canonical_joint_for_bone(name) else {
            continue;
        };
        if mapped.insert(joint, name.to_string()).is_some() {
            duplicates.insert(joint);
        }
    }
    let missing = JointKind::HUMANOID
        .into_iter()
        .filter(|joint| !mapped.contains_key(joint))
        .collect();
    let mut duplicate_targets: Vec<_> = duplicates.into_iter().collect();
    duplicate_targets.sort_by_key(|joint| *joint as u8);
    RigValidation {
        mapped,
        missing,
        duplicate_targets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character_studio::{generators::build_character_patch, spec::CharacterSpec};

    #[test]
    fn amp_armature_satisfies_the_starfall_humanoid_contract() {
        let amp = [
            "Pelvis",
            "Spine01",
            "Spine02",
            "NeckTwist01",
            "Head",
            "L_Upperarm",
            "L_Forearm",
            "L_Hand",
            "R_Upperarm",
            "R_Forearm",
            "R_Hand",
            "L_Thigh",
            "L_Calf",
            "L_Foot",
            "R_Thigh",
            "R_Calf",
            "R_Foot",
        ];
        let validation = validate_humanoid_bones(amp);
        assert!(validation.is_usable(), "{validation:?}");
        assert_eq!(validation.mapped.len(), JointKind::HUMANOID.len());
    }

    #[test]
    fn canonical_morph_contract_matches_the_generator_patch() {
        let patch = build_character_patch(&CharacterSpec::default());
        for morph in CANONICAL_MORPHS {
            assert!(patch.morphs.contains_key(morph), "missing {morph}");
        }
    }

    #[test]
    fn duplicate_joint_targets_make_a_rig_invalid() {
        let validation = validate_humanoid_bones(["SF_HEAD", "Head"]);
        assert_eq!(validation.duplicate_targets, vec![JointKind::Head]);
        assert!(!validation.is_usable());
    }
}
