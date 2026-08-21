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
    Feathered,
    Spiky,
    Bob,
    SidePonytail,
}

impl HairStyle {
    pub const ALL: [HairStyle; 9] = [
        HairStyle::Bald,
        HairStyle::Buzz,
        HairStyle::Short,
        HairStyle::Long,
        HairStyle::Ponytail,
        HairStyle::Feathered,
        HairStyle::Spiky,
        HairStyle::Bob,
        HairStyle::SidePonytail,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HairStyle::Bald => "Bald",
            HairStyle::Buzz => "Buzz",
            HairStyle::Short => "Short",
            HairStyle::Long => "Long",
            HairStyle::Ponytail => "Ponytail",
            HairStyle::Feathered => "80s Feathered",
            HairStyle::Spiky => "Anime Spikes",
            HairStyle::Bob => "Anime Bob",
            HairStyle::SidePonytail => "Side Ponytail",
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
    TankTop,
    BomberJacket,
    MotoJacket,
    Blazer,
    ArcaneCoat,
    StarKnightTunic,
}

impl TopStyle {
    pub const ALL: [TopStyle; 12] = [
        TopStyle::None,
        TopStyle::TShirt,
        TopStyle::LongShirt,
        TopStyle::Tunic,
        TopStyle::Jacket,
        TopStyle::Robe,
        TopStyle::TankTop,
        TopStyle::BomberJacket,
        TopStyle::MotoJacket,
        TopStyle::Blazer,
        TopStyle::ArcaneCoat,
        TopStyle::StarKnightTunic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TopStyle::None => "None",
            TopStyle::TShirt => "T-Shirt",
            TopStyle::LongShirt => "Long Shirt",
            TopStyle::Tunic => "Tunic",
            TopStyle::Jacket => "Jacket",
            TopStyle::Robe => "Robe",
            TopStyle::TankTop => "Tank Top",
            TopStyle::BomberJacket => "Bomber Jacket",
            TopStyle::MotoJacket => "Moto Jacket",
            TopStyle::Blazer => "School Blazer",
            TopStyle::ArcaneCoat => "Arcane Coat",
            TopStyle::StarKnightTunic => "Star-Knight Tunic",
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
    Jeans,
    CargoPants,
    FlaredPants,
    PleatedSkirt,
}

impl BottomStyle {
    pub const ALL: [BottomStyle; 8] = [
        BottomStyle::Underwear,
        BottomStyle::Shorts,
        BottomStyle::Pants,
        BottomStyle::Skirt,
        BottomStyle::Jeans,
        BottomStyle::CargoPants,
        BottomStyle::FlaredPants,
        BottomStyle::PleatedSkirt,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BottomStyle::Underwear => "Underwear",
            BottomStyle::Shorts => "Shorts",
            BottomStyle::Pants => "Pants",
            BottomStyle::Skirt => "Skirt",
            BottomStyle::Jeans => "Jeans",
            BottomStyle::CargoPants => "Cargo Pants",
            BottomStyle::FlaredPants => "Flared Pants",
            BottomStyle::PleatedSkirt => "Pleated Skirt",
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
    Sneakers,
    HighTops,
    Loafers,
    CombatBoots,
    HeeledBoots,
}

impl FootStyle {
    pub const ALL: [FootStyle; 9] = [
        FootStyle::Barefoot,
        FootStyle::Shoes,
        FootStyle::Boots,
        FootStyle::TallBoots,
        FootStyle::Sneakers,
        FootStyle::HighTops,
        FootStyle::Loafers,
        FootStyle::CombatBoots,
        FootStyle::HeeledBoots,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FootStyle::Barefoot => "Barefoot",
            FootStyle::Shoes => "Shoes",
            FootStyle::Boots => "Boots",
            FootStyle::TallBoots => "Tall Boots",
            FootStyle::Sneakers => "Sneakers",
            FootStyle::HighTops => "High-Tops",
            FootStyle::Loafers => "Loafers",
            FootStyle::CombatBoots => "Combat Boots",
            FootStyle::HeeledBoots => "Heeled Boots",
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
    CrystalMecha,
    DragonMecha,
}

impl ArmorStyle {
    pub const ALL: [ArmorStyle; 5] = [
        ArmorStyle::None,
        ArmorStyle::SuperSuit,
        ArmorStyle::MechaArmor,
        ArmorStyle::CrystalMecha,
        ArmorStyle::DragonMecha,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ArmorStyle::None => "None",
            ArmorStyle::SuperSuit => "Super Suit",
            ArmorStyle::MechaArmor => "Mecha Armor",
            ArmorStyle::CrystalMecha => "Crystal Mecha",
            ArmorStyle::DragonMecha => "Dragon Mecha",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum FlairStyle {
    #[default]
    None,
    StarGem,
    MoonGem,
    RoyalMantle,
    ArcaneHalo,
    MechaWings,
}

impl FlairStyle {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::StarGem,
        Self::MoonGem,
        Self::RoyalMantle,
        Self::ArcaneHalo,
        Self::MechaWings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::StarGem => "Star Gem",
            Self::MoonGem => "Moon Gem",
            Self::RoyalMantle => "Royal Mantle",
            Self::ArcaneHalo => "Arcane Halo",
            Self::MechaWings => "Mecha Wings",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardrobeSpec {
    pub top: TopStyle,
    pub bottom: BottomStyle,
    pub feet: FootStyle,
    pub hands: HandStyle,
    pub armor: ArmorStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodySpec {
    pub height: f32,
    pub muscle: f32,
    pub weight: f32,
    pub shoulder_width: f32,
    pub waist_width: f32,
    pub hip_width: f32,
    pub limb_length: f32,
    pub chest_shape: f32,
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
            chest_shape: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FaceSpec {
    pub face_length: f32,
    pub jaw_width: f32,
    pub chin_length: f32,
    pub chin_width: f32,
    pub nose_length: f32,
    pub nose_width: f32,
    pub nose_bridge: f32,
    pub nose_tip: f32,
    pub brow_depth: f32,
    pub cheek_fullness: f32,
    pub eye_size: f32,
    pub eye_shape: f32,
    pub eye_spacing: f32,
    pub eye_tilt: f32,
    pub eye_depth: f32,
    pub brow_angle: f32,
    pub mouth_width: f32,
    pub lip_fullness: f32,
}

impl Default for FaceSpec {
    fn default() -> Self {
        Self {
            face_length: 0.5,
            jaw_width: 0.5,
            chin_length: 0.5,
            chin_width: 0.5,
            nose_length: 0.5,
            nose_width: 0.5,
            nose_bridge: 0.5,
            nose_tip: 0.5,
            brow_depth: 0.5,
            cheek_fullness: 0.5,
            eye_size: 0.5,
            eye_shape: 0.5,
            eye_spacing: 0.5,
            eye_tilt: 0.5,
            eye_depth: 0.5,
            brow_angle: 0.5,
            mouth_width: 0.5,
            lip_fullness: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StyleSpec {
    /// Index into the studio skin-tone palette.
    pub skin_tone: usize,
    pub eye_color: usize,
    pub hair: HairStyle,
    pub hair_color: usize,
    pub wardrobe: WardrobeSpec,
    pub flair: FlairStyle,
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
    Flair,
    PrimaryColor,
    SecondaryColor,
}

impl StyleField {
    pub const ALL: [StyleField; 13] = [
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
        StyleField::Flair,
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
            StyleField::Flair => "Fantasy Flair",
            StyleField::PrimaryColor => "Primary",
            StyleField::SecondaryColor => "Secondary",
        }
    }

    pub fn value_label(self, spec: &CharacterSpec) -> String {
        const SKIN_NAMES: [&str; 8] = [
            "Porcelain",
            "Fair",
            "Warm",
            "Tan",
            "Umber",
            "Deep",
            "Ebony",
            "Onyx",
        ];
        const EYE_NAMES: [&str; 8] = [
            "Blue", "Green", "Brown", "Grey", "Amber", "Violet", "Cyan", "Crimson",
        ];
        const HAIR_NAMES: [&str; 12] = [
            "Black",
            "Dark Brown",
            "Brown",
            "Blonde",
            "Auburn",
            "Silver",
            "Ginger",
            "Blue",
            "Pink",
            "Teal",
            "Violet",
            "Platinum",
        ];
        const OUTFIT_NAMES: [&str; 12] = [
            "Cobalt",
            "Crimson",
            "Emerald",
            "Gold",
            "Violet",
            "White",
            "Charcoal",
            "Cyan",
            "Neon Pink",
            "Orange",
            "Navy",
            "Tan",
        ];
        match self {
            StyleField::Sex => spec.sex.label().to_string(),
            StyleField::SkinTone => SKIN_NAMES[spec.style.skin_tone % SKIN_NAMES.len()].to_string(),
            StyleField::EyeColor => EYE_NAMES[spec.style.eye_color % EYE_NAMES.len()].to_string(),
            StyleField::Hair => spec.style.hair.label().to_string(),
            StyleField::HairColor => {
                HAIR_NAMES[spec.style.hair_color % HAIR_NAMES.len()].to_string()
            }
            StyleField::Top => spec.style.wardrobe.top.label().to_string(),
            StyleField::Bottom => spec.style.wardrobe.bottom.label().to_string(),
            StyleField::Feet => spec.style.wardrobe.feet.label().to_string(),
            StyleField::Hands => spec.style.wardrobe.hands.label().to_string(),
            StyleField::Armor => spec.style.wardrobe.armor.label().to_string(),
            StyleField::Flair => spec.style.flair.label().to_string(),
            StyleField::PrimaryColor => {
                OUTFIT_NAMES[spec.style.primary_color % OUTFIT_NAMES.len()].to_string()
            }
            StyleField::SecondaryColor => {
                OUTFIT_NAMES[spec.style.secondary_color % OUTFIT_NAMES.len()].to_string()
            }
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
            StyleField::EyeColor => spec.style.eye_color = step_idx(spec.style.eye_color, 8, dir),
            StyleField::Hair => spec.style.hair = step(&HairStyle::ALL, spec.style.hair, dir),
            StyleField::HairColor => {
                spec.style.hair_color = step_idx(spec.style.hair_color, 12, dir)
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
            StyleField::Flair => spec.style.flair = step(&FlairStyle::ALL, spec.style.flair, dir),
            StyleField::PrimaryColor => {
                spec.style.primary_color = step_idx(spec.style.primary_color, 12, dir)
            }
            StyleField::SecondaryColor => {
                spec.style.secondary_color = step_idx(spec.style.secondary_color, 12, dir)
            }
        }
    }

    /// Restore just this style slot to the default character value. Keeping
    /// this operation field-local makes the studio's reset control useful for
    /// experimentation without discarding the rest of a player's design.
    pub fn reset(self, spec: &mut CharacterSpec) {
        let defaults = CharacterSpec::default();
        match self {
            StyleField::Sex => spec.sex = defaults.sex,
            StyleField::SkinTone => spec.style.skin_tone = defaults.style.skin_tone,
            StyleField::EyeColor => spec.style.eye_color = defaults.style.eye_color,
            StyleField::Hair => spec.style.hair = defaults.style.hair,
            StyleField::HairColor => spec.style.hair_color = defaults.style.hair_color,
            StyleField::Top => spec.style.wardrobe.top = defaults.style.wardrobe.top,
            StyleField::Bottom => spec.style.wardrobe.bottom = defaults.style.wardrobe.bottom,
            StyleField::Feet => spec.style.wardrobe.feet = defaults.style.wardrobe.feet,
            StyleField::Hands => spec.style.wardrobe.hands = defaults.style.wardrobe.hands,
            StyleField::Armor => spec.style.wardrobe.armor = defaults.style.wardrobe.armor,
            StyleField::Flair => spec.style.flair = defaults.style.flair,
            StyleField::PrimaryColor => spec.style.primary_color = defaults.style.primary_color,
            StyleField::SecondaryColor => {
                spec.style.secondary_color = defaults.style.secondary_color
            }
        }
    }
}

/// The whole character. Serialize THIS, never the generated meshes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacterSpec {
    pub sex: Sex,
    pub body: BodySpec,
    pub face: FaceSpec,
    pub style: StyleSpec,
    /// Semantic geometry coordinates shared with the creature/enemy Forge.
    /// They drive generated proportions without exposing anonymous vertices.
    #[serde(default)]
    pub style_vector: crate::character::style_space::CharacterStyleVector,
    /// Blender-style non-destructive modifier slots applied to every generated
    /// body-part mesh (fixed size keeps the spec `Copy`; `None` slots are
    /// skipped).
    #[serde(default)]
    pub modifiers: [Option<crate::character::mesh_modifiers::MeshModifier>; 4],
}

/// Semantic invalidation mask for generated character content. It describes
/// dependencies, not anonymous mesh ranges, so it remains stable as topology
/// improves and can later route work to separate asynchronous generators.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharacterGeometryScope(u8);

impl CharacterGeometryScope {
    pub const NONE: Self = Self(0);
    pub const BODY: Self = Self(1 << 0);
    pub const FACE: Self = Self(1 << 1);
    pub const HAIR: Self = Self(1 << 2);
    pub const WARDROBE: Self = Self(1 << 3);
    pub const MATERIALS: Self = Self(1 << 4);
    pub const MODIFIERS: Self = Self(1 << 5);
    pub const ALL: Self = Self((1 << 6) - 1);

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Body proportions reposition every attached visual group; global mesh
    /// modifiers likewise rederive every generated part.
    pub const fn requires_full_hierarchy_rebuild(self) -> bool {
        self.0 & (Self::BODY.0 | Self::MODIFIERS.0) != 0
    }

    pub fn between(before: &CharacterSpec, after: &CharacterSpec) -> Self {
        let mut scope = Self::NONE;
        if before.sex != after.sex
            || before.body != after.body
            || before.style_vector != after.style_vector
        {
            scope = scope
                .union(Self::BODY)
                .union(Self::FACE)
                .union(Self::HAIR)
                .union(Self::WARDROBE);
        }
        if before.face != after.face {
            scope = scope.union(Self::FACE);
        }
        if before.style.hair != after.style.hair {
            scope = scope.union(Self::HAIR);
        }
        if before.style.wardrobe != after.style.wardrobe || before.style.flair != after.style.flair
        {
            scope = scope.union(Self::WARDROBE);
        }
        if before.style.skin_tone != after.style.skin_tone
            || before.style.eye_color != after.style.eye_color
            || before.style.hair_color != after.style.hair_color
            || before.style.primary_color != after.style.primary_color
            || before.style.secondary_color != after.style.secondary_color
        {
            scope = scope.union(Self::MATERIALS);
        }
        if before.modifiers != after.modifiers {
            scope = scope.union(Self::MODIFIERS);
        }
        scope
    }

    pub fn summary(self) -> String {
        if self.is_empty() {
            return "none".into();
        }
        let labels = [
            (Self::BODY, "body"),
            (Self::FACE, "face"),
            (Self::HAIR, "hair"),
            (Self::WARDROBE, "wardrobe"),
            (Self::MATERIALS, "materials"),
            (Self::MODIFIERS, "modifiers"),
        ];
        labels
            .into_iter()
            .filter_map(|(scope, label)| self.contains(scope).then_some(label))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

/// Every editable scalar morph, in UI order. The editor cursor walks this
/// list; generators translate each into a named morph in the patch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MorphField {
    Fantasy,
    Cute,
    Heroic,
    Mechanical,
    Creature,
    Height,
    Muscle,
    Weight,
    ShoulderWidth,
    WaistWidth,
    HipWidth,
    LimbLength,
    ChestShape,
    FaceLength,
    JawWidth,
    ChinLength,
    ChinWidth,
    NoseLength,
    NoseWidth,
    NoseBridge,
    NoseTip,
    BrowDepth,
    CheekFullness,
    EyeSize,
    EyeShape,
    EyeSpacing,
    EyeTilt,
    EyeDepth,
    BrowAngle,
    MouthWidth,
    LipFullness,
}

impl MorphField {
    pub const ALL: [MorphField; 31] = [
        MorphField::Fantasy,
        MorphField::Cute,
        MorphField::Heroic,
        MorphField::Mechanical,
        MorphField::Creature,
        MorphField::Height,
        MorphField::Muscle,
        MorphField::Weight,
        MorphField::ShoulderWidth,
        MorphField::WaistWidth,
        MorphField::HipWidth,
        MorphField::LimbLength,
        MorphField::ChestShape,
        MorphField::FaceLength,
        MorphField::JawWidth,
        MorphField::ChinLength,
        MorphField::ChinWidth,
        MorphField::NoseLength,
        MorphField::NoseWidth,
        MorphField::NoseBridge,
        MorphField::NoseTip,
        MorphField::BrowDepth,
        MorphField::CheekFullness,
        MorphField::EyeSize,
        MorphField::EyeShape,
        MorphField::EyeSpacing,
        MorphField::EyeTilt,
        MorphField::EyeDepth,
        MorphField::BrowAngle,
        MorphField::MouthWidth,
        MorphField::LipFullness,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MorphField::Fantasy => "Fantasy",
            MorphField::Cute => "Cute / Chibi",
            MorphField::Heroic => "Heroic",
            MorphField::Mechanical => "Mechanical",
            MorphField::Creature => "Creature",
            MorphField::Height => "Height",
            MorphField::Muscle => "Muscle",
            MorphField::Weight => "Weight",
            MorphField::ShoulderWidth => "Shoulders",
            MorphField::WaistWidth => "Waist",
            MorphField::HipWidth => "Hips",
            MorphField::LimbLength => "Limb Length",
            MorphField::ChestShape => "Chest Shape",
            MorphField::FaceLength => "Face Length",
            MorphField::JawWidth => "Jaw Width",
            MorphField::ChinLength => "Chin",
            MorphField::ChinWidth => "Chin Width",
            MorphField::NoseLength => "Nose Length",
            MorphField::NoseWidth => "Nose Width",
            MorphField::NoseBridge => "Nose Bridge",
            MorphField::NoseTip => "Nose Tip",
            MorphField::BrowDepth => "Brow",
            MorphField::CheekFullness => "Cheeks",
            MorphField::EyeSize => "Eyes",
            MorphField::EyeShape => "Eye Shape",
            MorphField::EyeSpacing => "Eye Spacing",
            MorphField::EyeTilt => "Eye Tilt",
            MorphField::EyeDepth => "Eye Depth",
            MorphField::BrowAngle => "Brow Angle",
            MorphField::MouthWidth => "Mouth",
            MorphField::LipFullness => "Lip Fullness",
        }
    }

    pub fn get(self, spec: &CharacterSpec) -> f32 {
        match self {
            MorphField::Fantasy => spec.style_vector.fantasy,
            MorphField::Cute => spec.style_vector.cute,
            MorphField::Heroic => spec.style_vector.heroic,
            MorphField::Mechanical => spec.style_vector.mechanical,
            MorphField::Creature => spec.style_vector.creature,
            MorphField::Height => spec.body.height,
            MorphField::Muscle => spec.body.muscle,
            MorphField::Weight => spec.body.weight,
            MorphField::ShoulderWidth => spec.body.shoulder_width,
            MorphField::WaistWidth => spec.body.waist_width,
            MorphField::HipWidth => spec.body.hip_width,
            MorphField::LimbLength => spec.body.limb_length,
            MorphField::ChestShape => spec.body.chest_shape,
            MorphField::FaceLength => spec.face.face_length,
            MorphField::JawWidth => spec.face.jaw_width,
            MorphField::ChinLength => spec.face.chin_length,
            MorphField::ChinWidth => spec.face.chin_width,
            MorphField::NoseLength => spec.face.nose_length,
            MorphField::NoseWidth => spec.face.nose_width,
            MorphField::NoseBridge => spec.face.nose_bridge,
            MorphField::NoseTip => spec.face.nose_tip,
            MorphField::BrowDepth => spec.face.brow_depth,
            MorphField::CheekFullness => spec.face.cheek_fullness,
            MorphField::EyeSize => spec.face.eye_size,
            MorphField::EyeShape => spec.face.eye_shape,
            MorphField::EyeSpacing => spec.face.eye_spacing,
            MorphField::EyeTilt => spec.face.eye_tilt,
            MorphField::EyeDepth => spec.face.eye_depth,
            MorphField::BrowAngle => spec.face.brow_angle,
            MorphField::MouthWidth => spec.face.mouth_width,
            MorphField::LipFullness => spec.face.lip_fullness,
        }
    }

    pub fn set(self, spec: &mut CharacterSpec, value: f32) {
        let v = value.clamp(0.0, 1.0);
        match self {
            MorphField::Fantasy => spec.style_vector.fantasy = v,
            MorphField::Cute => spec.style_vector.cute = v,
            MorphField::Heroic => spec.style_vector.heroic = v,
            MorphField::Mechanical => spec.style_vector.mechanical = v,
            MorphField::Creature => spec.style_vector.creature = v,
            MorphField::Height => spec.body.height = v,
            MorphField::Muscle => spec.body.muscle = v,
            MorphField::Weight => spec.body.weight = v,
            MorphField::ShoulderWidth => spec.body.shoulder_width = v,
            MorphField::WaistWidth => spec.body.waist_width = v,
            MorphField::HipWidth => spec.body.hip_width = v,
            MorphField::LimbLength => spec.body.limb_length = v,
            MorphField::ChestShape => spec.body.chest_shape = v,
            MorphField::FaceLength => spec.face.face_length = v,
            MorphField::JawWidth => spec.face.jaw_width = v,
            MorphField::ChinLength => spec.face.chin_length = v,
            MorphField::ChinWidth => spec.face.chin_width = v,
            MorphField::NoseLength => spec.face.nose_length = v,
            MorphField::NoseWidth => spec.face.nose_width = v,
            MorphField::NoseBridge => spec.face.nose_bridge = v,
            MorphField::NoseTip => spec.face.nose_tip = v,
            MorphField::BrowDepth => spec.face.brow_depth = v,
            MorphField::CheekFullness => spec.face.cheek_fullness = v,
            MorphField::EyeSize => spec.face.eye_size = v,
            MorphField::EyeShape => spec.face.eye_shape = v,
            MorphField::EyeSpacing => spec.face.eye_spacing = v,
            MorphField::EyeTilt => spec.face.eye_tilt = v,
            MorphField::EyeDepth => spec.face.eye_depth = v,
            MorphField::BrowAngle => spec.face.brow_angle = v,
            MorphField::MouthWidth => spec.face.mouth_width = v,
            MorphField::LipFullness => spec.face.lip_fullness = v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morph_fields_clamp_to_the_supported_sculpt_range() {
        let mut spec = CharacterSpec::default();
        MorphField::Height.set(&mut spec, 2.0);
        MorphField::JawWidth.set(&mut spec, -1.0);
        assert_eq!(spec.body.height, 1.0);
        assert_eq!(spec.face.jaw_width, 0.0);
    }

    #[test]
    fn style_reset_only_changes_the_selected_slot() {
        let mut spec = CharacterSpec::default();
        spec.style.hair = HairStyle::Ponytail;
        spec.style.primary_color = 6;

        StyleField::Hair.reset(&mut spec);

        assert_eq!(spec.style.hair, CharacterSpec::default().style.hair);
        assert_eq!(spec.style.primary_color, 6);
    }

    #[test]
    fn legacy_wardrobe_names_and_new_mvp_styles_round_trip() {
        let legacy = r#"{
            "sex":"Male",
            "body":{"height":0.5,"muscle":0.5,"weight":0.5,"shoulder_width":0.5,"waist_width":0.5,"hip_width":0.5,"limb_length":0.5},
            "face":{"jaw_width":0.5,"chin_length":0.5,"nose_length":0.5,"nose_width":0.5,"brow_depth":0.5,"cheek_fullness":0.5,"eye_size":0.5,"mouth_width":0.5},
            "style":{"skin_tone":0,"eye_color":0,"hair":"Short","hair_color":0,"wardrobe":{"top":"Jacket","bottom":"Pants","feet":"Boots","hands":"Bare","armor":"None"},"primary_color":0,"secondary_color":0}
        }"#;
        let old: CharacterSpec = serde_json::from_str(legacy).expect("legacy preset should load");
        assert_eq!(old.style.wardrobe.top, TopStyle::Jacket);
        assert_eq!(old.face.face_length, 0.5);
        assert_eq!(old.face.eye_spacing, 0.5);
        assert_eq!(old.face.eye_shape, 0.5);
        assert_eq!(old.face.eye_depth, 0.5);
        assert_eq!(old.face.nose_bridge, 0.5);
        assert_eq!(old.face.nose_tip, 0.5);
        assert_eq!(old.face.chin_width, 0.5);
        assert_eq!(old.face.lip_fullness, 0.5);
        assert_eq!(old.body.chest_shape, 0.5);
        assert_eq!(old.style.flair, FlairStyle::None);
        assert_eq!(
            old.style_vector,
            crate::character::style_space::CharacterStyleVector::default()
        );

        let mut modern = old;
        modern.style.hair = HairStyle::Feathered;
        modern.style.wardrobe.top = TopStyle::BomberJacket;
        modern.style.wardrobe.bottom = BottomStyle::CargoPants;
        modern.style.wardrobe.feet = FootStyle::HighTops;
        modern.style.flair = FlairStyle::StarGem;
        let json = serde_json::to_string(&modern).expect("modern preset should serialize");
        let loaded: CharacterSpec =
            serde_json::from_str(&json).expect("modern preset should deserialize");
        assert_eq!(loaded.style.hair, HairStyle::Feathered);
        assert_eq!(loaded.style.wardrobe.top, TopStyle::BomberJacket);
        assert_eq!(loaded.style.wardrobe.bottom, BottomStyle::CargoPants);
        assert_eq!(loaded.style.wardrobe.feet, FootStyle::HighTops);
        assert_eq!(loaded.style.flair, FlairStyle::StarGem);
    }

    #[test]
    fn color_cycles_cover_the_expanded_anime_palette() {
        let mut spec = CharacterSpec::default();
        for _ in 0..11 {
            StyleField::PrimaryColor.cycle(&mut spec, 1);
        }
        assert_eq!(spec.style.primary_color, 11);
        assert_eq!(StyleField::PrimaryColor.value_label(&spec), "Tan");
        StyleField::PrimaryColor.cycle(&mut spec, 1);
        assert_eq!(spec.style.primary_color, 0);
    }

    #[test]
    fn dependency_scope_distinguishes_face_hair_material_and_body_edits() {
        let base = CharacterSpec::default();

        let mut face = base;
        face.face.eye_size = 0.8;
        assert_eq!(
            CharacterGeometryScope::between(&base, &face),
            CharacterGeometryScope::FACE
        );

        let mut hair = base;
        hair.style.hair = HairStyle::Ponytail;
        assert_eq!(
            CharacterGeometryScope::between(&base, &hair),
            CharacterGeometryScope::HAIR
        );

        let mut material = base;
        material.style.primary_color = 4;
        assert_eq!(
            CharacterGeometryScope::between(&base, &material),
            CharacterGeometryScope::MATERIALS
        );

        let mut body = base;
        body.body.height = 0.8;
        let scope = CharacterGeometryScope::between(&base, &body);
        assert!(scope.contains(CharacterGeometryScope::BODY));
        assert!(scope.contains(CharacterGeometryScope::FACE));
        assert!(scope.contains(CharacterGeometryScope::HAIR));
        assert!(scope.contains(CharacterGeometryScope::WARDROBE));
        assert!(scope.requires_full_hierarchy_rebuild());
    }

    #[test]
    fn unchanged_specs_produce_no_regeneration_work() {
        let spec = CharacterSpec::default();
        assert!(CharacterGeometryScope::between(&spec, &spec).is_empty());
    }
}
