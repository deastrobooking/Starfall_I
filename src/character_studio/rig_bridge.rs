//! Contract between external skinned glTF humanoids and Starfall's stable
//! character specification. The bridge is deliberately data-only: gameplay
//! continues to own the canonical [`JointKind`] skeleton, while an imported
//! scene may supply a skinned visual when it satisfies this mapping.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::components::character::JointKind;

/// Stable morph names emitted by `generators.rs`. A production Blender file
/// should export shape keys with these exact names. Aliases belong in the asset
/// conversion pipeline, not in player save data.
#[allow(dead_code)] // Public DCC/export contract; consumed by validation tests until morph import lands.
pub const CANONICAL_MORPHS: [&str; 26] = [
    "body_height",
    "body_muscle",
    "body_weight",
    "shoulders_wide",
    "waist_width",
    "hips_wide",
    "limb_length",
    "body_chest_shape",
    "face_length",
    "face_jaw_wide",
    "face_chin_long",
    "face_chin_wide",
    "face_nose_long",
    "face_nose_wide",
    "face_nose_bridge",
    "face_nose_tip",
    "face_brow_heavy",
    "face_cheek_full",
    "face_eye_large",
    "face_eye_shape",
    "face_eye_spacing",
    "face_eye_tilt",
    "face_eye_depth",
    "face_brow_angle",
    "face_mouth_wide",
    "face_lip_full",
];

#[allow(dead_code)] // Offline/profile validation API; runtime binding reports the component status below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigValidation {
    pub mapped: HashMap<JointKind, String>,
    pub missing: Vec<JointKind>,
    pub duplicate_targets: Vec<JointKind>,
}

/// Opts a loaded world-asset hierarchy into Starfall's canonical humanoid
/// adapter. The scene remains the visual source; gameplay owns animation.
#[derive(Component, Debug, Clone)]
pub struct ImportedHumanoidRig {
    pub source: String,
}

impl ImportedHumanoidRig {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }
}

/// Inspectable import diagnostics used by Character Studio and future editor
/// tooling. Invalid rigs remain visible but are never partially animated.
#[derive(Component, Debug, Clone, PartialEq, Eq, Default)]
pub enum ImportedRigStatus {
    #[default]
    Pending,
    Ready {
        mapped_joint_count: usize,
    },
    Invalid {
        missing: Vec<JointKind>,
        duplicate_targets: Vec<JointKind>,
        unresolved_hierarchy: Vec<JointKind>,
    },
}

#[allow(dead_code)]
impl RigValidation {
    pub fn is_usable(&self) -> bool {
        self.missing.is_empty() && self.duplicate_targets.is_empty()
    }
}

/// Map the AMP armature naming convention and the documented `SF_*`
/// production convention onto the runtime's 17-joint humanoid contract.
pub fn canonical_joint_for_bone(name: &str) -> Option<JointKind> {
    let normalized = normalize_bone_name(name);
    Some(match normalized.as_str() {
        "sf_pelvis" | "pelvis" | "hips" | "hip" => JointKind::Pelvis,
        "sf_spine" | "spine" | "spine01" | "spine_01" | "spine1" => JointKind::Spine,
        "sf_chest" | "spine02" | "spine_02" | "spine2" | "chest" | "upperchest" | "upper_chest" => {
            JointKind::Chest
        }
        "sf_neck" | "neck" | "necktwist01" | "neck_twist_01" => JointKind::Neck,
        "sf_head" | "head" => JointKind::Head,
        "sf_shoulder_l" | "l_upperarm" | "leftupperarm" | "leftarm" | "upperarm_l"
        | "upper_arm_l" | "def_upper_arm_l" => JointKind::LeftShoulder,
        "sf_elbow_l" | "l_forearm" | "leftforearm" | "leftlowerarm" | "lowerarm_l"
        | "forearm_l" | "def_forearm_l" => JointKind::LeftElbow,
        "sf_wrist_l" | "l_hand" | "lefthand" | "hand_l" | "def_hand_l" => JointKind::LeftWrist,
        "sf_shoulder_r" | "r_upperarm" | "rightupperarm" | "rightarm" | "upperarm_r"
        | "upper_arm_r" | "def_upper_arm_r" => JointKind::RightShoulder,
        "sf_elbow_r" | "r_forearm" | "rightforearm" | "rightlowerarm" | "lowerarm_r"
        | "forearm_r" | "def_forearm_r" => JointKind::RightElbow,
        "sf_wrist_r" | "r_hand" | "righthand" | "hand_r" | "def_hand_r" => JointKind::RightWrist,
        "sf_hip_l" | "l_thigh" | "leftupleg" | "leftthigh" | "thigh_l" | "def_thigh_l" => {
            JointKind::LeftHip
        }
        "sf_knee_l" | "l_calf" | "leftleg" | "calf_l" | "shin_l" | "def_shin_l" => {
            JointKind::LeftKnee
        }
        "sf_ankle_l" | "l_foot" | "leftfoot" | "foot_l" | "def_foot_l" => JointKind::LeftAnkle,
        "sf_hip_r" | "r_thigh" | "rightupleg" | "rightthigh" | "thigh_r" | "def_thigh_r" => {
            JointKind::RightHip
        }
        "sf_knee_r" | "r_calf" | "rightleg" | "calf_r" | "shin_r" | "def_shin_r" => {
            JointKind::RightKnee
        }
        "sf_ankle_r" | "r_foot" | "rightfoot" | "foot_r" | "def_foot_r" => JointKind::RightAnkle,
        _ => return None,
    })
}

fn normalize_bone_name(name: &str) -> String {
    let leaf = name.trim().rsplit(':').next().unwrap_or(name.trim());
    let mut normalized = leaf
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    while normalized.contains("__") {
        normalized = normalized.replace("__", "_");
    }
    normalized = normalized.trim_matches('_').to_string();
    for prefix in ["mixamorig_", "mixamorig", "armature_"] {
        if let Some(stripped) = normalized.strip_prefix(prefix) {
            normalized = stripped.trim_start_matches('_').to_string();
            break;
        }
    }
    normalized
}

#[allow(dead_code)] // Retained for import-wizard preflight and unit-level asset audits.
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

    #[test]
    fn common_external_rig_names_map_without_per_asset_code() {
        assert_eq!(
            canonical_joint_for_bone("mixamorig:LeftArm"),
            Some(JointKind::LeftShoulder)
        );
        assert_eq!(
            canonical_joint_for_bone("upperarm_r"),
            Some(JointKind::RightShoulder)
        );
        assert_eq!(
            canonical_joint_for_bone("DEF-forearm.L"),
            Some(JointKind::LeftElbow)
        );
        assert_eq!(
            canonical_joint_for_bone("thigh_r"),
            Some(JointKind::RightHip)
        );
    }
}
