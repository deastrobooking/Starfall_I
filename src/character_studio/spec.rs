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

// ── Wardrobe ──────────────────────────────────────────────────────────────────
// Each clothing region is an independent slot with many generated styles, so
// outfits mix and match. Armor (super suit / mecha) overrides the wardrobe
// visuals when active.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum TopStyle {
    None,
    #[default]
    TShirt,
    LongShirt,
    Tunic,
    Jacket,
    Robe,
}

impl TopStyle {
    pub const ALL: [TopStyle; 6] = [
        TopStyle::None,
        TopStyle::TShirt,
        TopStyle::LongShirt,
        TopStyle::Tunic,
        TopStyle::Jacket,
        TopStyle::Robe,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TopStyle::None => "None",
            TopStyle::TShirt => "T-Shirt",
            TopStyle::LongShirt => "Long Shirt",
            TopStyle::Tunic => "Tunic",
            TopStyle::Jacket => "Jacket",
            TopStyle::Robe => "Robe",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum BottomStyle {
    Underwear,
    Shorts,
    #[default]
    Pants,
    Skirt,
}

impl BottomStyle {
    pub const ALL: [BottomStyle; 4] = [
        BottomStyle::Underwear,
        BottomStyle::Shorts,
        BottomStyle::Pants,
        BottomStyle::Skirt,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BottomStyle::Underwear => "Underwear",
            BottomStyle::Shorts => "Shorts",
            BottomStyle::Pants => "Pants",
            BottomStyle::Skirt => "Skirt",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum FootStyle {
    Barefoot,
    #[default]
    Shoes,
    Boots,
    TallBoots,
}

impl FootStyle {
    pub const ALL: [FootStyle; 4] = [
        FootStyle::Barefoot,
        FootStyle::Shoes,
        FootStyle::Boots,
        FootStyle::TallBoots,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FootStyle::Barefoot => "Barefoot",
            FootStyle::Shoes => "Shoes",
            FootStyle::Boots => "Boots",
            FootStyle::TallBoots => "Tall Boots",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum HandStyle {
    #[default]
    Bare,
    Gloves,
    Gauntlets,
}

impl HandStyle {
    pub const ALL: [HandStyle; 3] = [HandStyle::Bare, HandStyle::Gloves, HandStyle::Gauntlets];

    pub fn label(self) -> &'static str {
        match self {
            HandStyle::Bare => "Bare",
            HandStyle::Gloves => "Gloves",
            HandStyle::Gauntlets => "Gauntlets",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ArmorStyle {
    #[default]
    None,
    SuperSuit,
    MechaArmor,
}

impl ArmorStyle {
    pub const ALL: [ArmorStyle; 3] = [
        ArmorStyle::None,
        ArmorStyle::SuperSuit,
        ArmorStyle::MechaArmor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ArmorStyle::None => "None",
            ArmorStyle::SuperSuit => "Super Suit",
            ArmorStyle::MechaArmor => "Mecha Armor",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct WardrobeSpec {
    pub top: TopStyle,
    pub bottom: BottomStyle,
    pub feet: FootStyle,
    pub hands: HandStyle,
    pub armor: ArmorStyle,
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
    pub wardrobe: WardrobeSpec,
    /// Primary outfit/suit/armor colour index.
    pub primary_color: usize,
    /// Secondary/trim colour index.
    pub secondary_color: usize,
}

/// Every cycling style option, in UI order — each renders as a `< value >`
/// row that works with mouse clicks or gamepad left/right.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StyleField {
    Sex,
    SkinTone,
    EyeColor,
    Hair,
    HairColor,
    Top,
    Bottom,
    Feet,
    Hands,
    Armor,
    PrimaryColor,
    SecondaryColor,
}

impl StyleField {
    pub const ALL: [StyleField; 12] = [
        StyleField::Sex,
        StyleField::SkinTone,
        StyleField::EyeColor,
        StyleField::Hair,
        StyleField::HairColor,
        StyleField::Top,
        StyleField::Bottom,
        StyleField::Feet,
        StyleField::Hands,
        StyleField::Armor,
        StyleField::PrimaryColor,
        StyleField::SecondaryColor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StyleField::Sex => "Body",
            StyleField::SkinTone => "Skin Tone",
            StyleField::EyeColor => "Eyes",
            StyleField::Hair => "Hair",
            StyleField::HairColor => "Hair Color",
            StyleField::Top => "Top",
            StyleField::Bottom => "Bottom",
            StyleField::Feet => "Footwear",
            StyleField::Hands => "Hands",
            StyleField::Armor => "Armor",
            StyleField::PrimaryColor => "Primary",
            StyleField::SecondaryColor => "Secondary",
        }
    }

    pub fn value_label(self, spec: &CharacterSpec) -> String {
        match self {
            StyleField::Sex => spec.sex.label().to_string(),
            StyleField::SkinTone => format!("Tone {}", spec.style.skin_tone + 1),
            StyleField::EyeColor => format!("Color {}", spec.style.eye_color + 1),
            StyleField::Hair => spec.style.hair.label().to_string(),
            StyleField::HairColor => format!("Color {}", spec.style.hair_color + 1),
            StyleField::Top => spec.style.wardrobe.top.label().to_string(),
            StyleField::Bottom => spec.style.wardrobe.bottom.label().to_string(),
            StyleField::Feet => spec.style.wardrobe.feet.label().to_string(),
            StyleField::Hands => spec.style.wardrobe.hands.label().to_string(),
            StyleField::Armor => spec.style.wardrobe.armor.label().to_string(),
            StyleField::PrimaryColor => format!("Color {}", spec.style.primary_color + 1),
            StyleField::SecondaryColor => format!("Color {}", spec.style.secondary_color + 1),
        }
    }

    /// Step the field forward (`dir = 1`) or back (`dir = -1`).
    pub fn cycle(self, spec: &mut CharacterSpec, dir: i32) {
        fn step<T: Copy + PartialEq>(all: &[T], current: T, dir: i32) -> T {
            let i = all.iter().position(|v| *v == current).unwrap_or(0) as i32;
            let n = all.len() as i32;
            all[((i + dir % n + n) % n) as usize]
        }
        fn step_idx(current: usize, len: usize, dir: i32) -> usize {
            let n = len as i32;
            ((current as i32 + dir % n + n) % n) as usize
        }
        match self {
            StyleField::Sex => {
                spec.sex = if spec.sex == Sex::Male {
                    Sex::Female
                } else {
                    Sex::Male
                }
            }
            StyleField::SkinTone => spec.style.skin_tone = step_idx(spec.style.skin_tone, 8, dir),
            StyleField::EyeColor => spec.style.eye_color = step_idx(spec.style.eye_color, 6, dir),
            StyleField::Hair => spec.style.hair = step(&HairStyle::ALL, spec.style.hair, dir),
            StyleField::HairColor => {
                spec.style.hair_color = step_idx(spec.style.hair_color, 8, dir)
            }
            StyleField::Top => {
                spec.style.wardrobe.top = step(&TopStyle::ALL, spec.style.wardrobe.top, dir)
            }
            StyleField::Bottom => {
                spec.style.wardrobe.bottom =
                    step(&BottomStyle::ALL, spec.style.wardrobe.bottom, dir)
            }
            StyleField::Feet => {
                spec.style.wardrobe.feet = step(&FootStyle::ALL, spec.style.wardrobe.feet, dir)
            }
            StyleField::Hands => {
                spec.style.wardrobe.hands = step(&HandStyle::ALL, spec.style.wardrobe.hands, dir)
            }
            StyleField::Armor => {
                spec.style.wardrobe.armor = step(&ArmorStyle::ALL, spec.style.wardrobe.armor, dir)
            }
            StyleField::PrimaryColor => {
                spec.style.primary_color = step_idx(spec.style.primary_color, 8, dir)
            }
            StyleField::SecondaryColor => {
                spec.style.secondary_color = step_idx(spec.style.secondary_color, 8, dir)
            }
        }
    }
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
