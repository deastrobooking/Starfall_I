//! `CharacterSpec` — the single source of truth for a generated human.
//!
//! Sliders/presets/randomize edit this struct; nothing else mutates entities.
//! The spec is serialized (JSON) for save/load, and the generator pipeline
//! (`generators.rs`) turns it into a [`super::patch::CharacterPatch`] each time
//! it changes. Morph fields are normalized `0.0..=1.0` with `0.5` = neutral.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Sex {
    #[default]
    Male,
    Female,
}

impl Sex {
    pub fn label(self) -> &'static str {
        match self {
            Sex::Male => "Male",
            Sex::Female => "Female",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum HairStyle {
    Bald,
    Buzz,
    #[default]
    Short,
    Long,
    Ponytail,
}

impl HairStyle {
    pub const ALL: [HairStyle; 5] = [
        HairStyle::Bald,
        HairStyle::Buzz,
        HairStyle::Short,
        HairStyle::Long,
        HairStyle::Ponytail,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HairStyle::Bald => "Bald",
            HairStyle::Buzz => "Buzz",
            HairStyle::Short => "Short",
            HairStyle::Long => "Long",
            HairStyle::Ponytail => "Ponytail",
        }
    }
}

/// Outfit layers stack conceptually: the body always exists; clothes/suits/
/// armor are generated overlay meshes on top of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum OutfitLayer {
    /// Base underlayer (fitted shorts + top band) — the sculpting view.
    Base,
    #[default]
    Clothes,
    /// Skin-tight hero suit with emblem, belt and trim.
    SuperSuit,
    /// Megaman-style mecha: helmet + crest, chest core, buster gauntlets,
    /// oversized boots.
    MechaArmor,
}

impl OutfitLayer {
    pub const ALL: [OutfitLayer; 4] = [
        OutfitLayer::Base,
        OutfitLayer::Clothes,
        OutfitLayer::SuperSuit,
        OutfitLayer::MechaArmor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OutfitLayer::Base => "Base Layer",
            OutfitLayer::Clothes => "Clothes",
            OutfitLayer::SuperSuit => "Super Suit",
            OutfitLayer::MechaArmor => "Mecha Armor",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BodySpec {
    pub height: f32,
    pub muscle: f32,
    pub weight: f32,
    pub shoulder_width: f32,
    pub waist_width: f32,
    pub hip_width: f32,
    pub limb_length: f32,
}

impl Default for BodySpec {
    fn default() -> Self {
        Self {
            height: 0.5,
            muscle: 0.5,
            weight: 0.5,
            shoulder_width: 0.5,
            waist_width: 0.5,
            hip_width: 0.5,
            limb_length: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FaceSpec {
    pub jaw_width: f32,
    pub chin_length: f32,
    pub nose_length: f32,
    pub nose_width: f32,
    pub brow_depth: f32,
    pub cheek_fullness: f32,
    pub eye_size: f32,
    pub mouth_width: f32,
}

impl Default for FaceSpec {
    fn default() -> Self {
        Self {
            jaw_width: 0.5,
            chin_length: 0.5,
            nose_length: 0.5,
            nose_width: 0.5,
            brow_depth: 0.5,
            cheek_fullness: 0.5,
            eye_size: 0.5,
            mouth_width: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct StyleSpec {
    /// Index into the studio skin-tone palette.
    pub skin_tone: usize,
    pub eye_color: usize,
    pub hair: HairStyle,
    pub hair_color: usize,
    pub outfit: OutfitLayer,
    /// Primary outfit/suit/armor colour index.
    pub primary_color: usize,
    /// Secondary/trim colour index.
    pub secondary_color: usize,
}

/// The whole character. Serialize THIS, never the generated meshes.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct CharacterSpec {
    pub sex: Sex,
    pub body: BodySpec,
    pub face: FaceSpec,
    pub style: StyleSpec,
}

/// Every editable scalar morph, in UI order. The editor cursor walks this
/// list; generators translate each into a named morph in the patch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MorphField {
    Height,
    Muscle,
    Weight,
    ShoulderWidth,
    WaistWidth,
    HipWidth,
    LimbLength,
    JawWidth,
    ChinLength,
    NoseLength,
    NoseWidth,
    BrowDepth,
    CheekFullness,
    EyeSize,
    MouthWidth,
}

impl MorphField {
    pub const ALL: [MorphField; 15] = [
        MorphField::Height,
        MorphField::Muscle,
        MorphField::Weight,
        MorphField::ShoulderWidth,
        MorphField::WaistWidth,
        MorphField::HipWidth,
        MorphField::LimbLength,
        MorphField::JawWidth,
        MorphField::ChinLength,
        MorphField::NoseLength,
        MorphField::NoseWidth,
        MorphField::BrowDepth,
        MorphField::CheekFullness,
        MorphField::EyeSize,
        MorphField::MouthWidth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MorphField::Height => "Height",
            MorphField::Muscle => "Muscle",
            MorphField::Weight => "Weight",
            MorphField::ShoulderWidth => "Shoulders",
            MorphField::WaistWidth => "Waist",
            MorphField::HipWidth => "Hips",
            MorphField::LimbLength => "Limb Length",
            MorphField::JawWidth => "Jaw Width",
            MorphField::ChinLength => "Chin",
            MorphField::NoseLength => "Nose Length",
            MorphField::NoseWidth => "Nose Width",
            MorphField::BrowDepth => "Brow",
            MorphField::CheekFullness => "Cheeks",
            MorphField::EyeSize => "Eyes",
            MorphField::MouthWidth => "Mouth",
        }
    }

    pub fn get(self, spec: &CharacterSpec) -> f32 {
        match self {
            MorphField::Height => spec.body.height,
            MorphField::Muscle => spec.body.muscle,
            MorphField::Weight => spec.body.weight,
            MorphField::ShoulderWidth => spec.body.shoulder_width,
            MorphField::WaistWidth => spec.body.waist_width,
            MorphField::HipWidth => spec.body.hip_width,
            MorphField::LimbLength => spec.body.limb_length,
            MorphField::JawWidth => spec.face.jaw_width,
            MorphField::ChinLength => spec.face.chin_length,
            MorphField::NoseLength => spec.face.nose_length,
            MorphField::NoseWidth => spec.face.nose_width,
            MorphField::BrowDepth => spec.face.brow_depth,
            MorphField::CheekFullness => spec.face.cheek_fullness,
            MorphField::EyeSize => spec.face.eye_size,
            MorphField::MouthWidth => spec.face.mouth_width,
        }
    }

    pub fn set(self, spec: &mut CharacterSpec, value: f32) {
        let v = value.clamp(0.0, 1.0);
        match self {
            MorphField::Height => spec.body.height = v,
            MorphField::Muscle => spec.body.muscle = v,
            MorphField::Weight => spec.body.weight = v,
            MorphField::ShoulderWidth => spec.body.shoulder_width = v,
            MorphField::WaistWidth => spec.body.waist_width = v,
            MorphField::HipWidth => spec.body.hip_width = v,
            MorphField::LimbLength => spec.body.limb_length = v,
            MorphField::JawWidth => spec.face.jaw_width = v,
            MorphField::ChinLength => spec.face.chin_length = v,
            MorphField::NoseLength => spec.face.nose_length = v,
            MorphField::NoseWidth => spec.face.nose_width = v,
            MorphField::BrowDepth => spec.face.brow_depth = v,
            MorphField::CheekFullness => spec.face.cheek_fullness = v,
            MorphField::EyeSize => spec.face.eye_size = v,
            MorphField::MouthWidth => spec.face.mouth_width = v,
        }
    }
}
