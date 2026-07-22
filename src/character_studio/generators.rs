//! Preset generator pipeline: `CharacterSpec → [PresetGenerator] → CharacterPatch`.
//!
//! Generators never touch entities. Each contributes named morphs, named
//! material colours, and mesh-slot assignments to a [`CharacterPatch`]; the
//! mesh builder (`human_mesh.rs`) consumes the patch. When a rigged,
//! shape-keyed `.glb` base lands later, the same named morphs bind to Bevy
//! `MorphWeights` by name — the editor and presets don't change.

use bevy::prelude::*;
use std::collections::HashMap;

use super::spec::{
    ArmorStyle, BottomStyle, CharacterSpec, FlairStyle, FootStyle, HairStyle, HandStyle, Sex,
    TopStyle, WardrobeSpec,
};

// ── Patch ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CharacterSlot {
    Hair,
    Torso,
    Legs,
    Feet,
    Hands,
    Helmet,
}

#[derive(Default, Debug)]
pub struct CharacterPatch {
    /// Named morph weights, `0.0..=1.0` (0.5 neutral).
    pub morphs: HashMap<&'static str, f32>,
    /// Named material colours ("skin", "eyes", "hair", "primary", "secondary").
    pub materials: HashMap<&'static str, Color>,
    /// What each outfit slot contains this build.
    pub slots: HashMap<CharacterSlot, SlotContent>,
    pub sex: Sex,
    pub wardrobe: WardrobeSpec,
    pub hair: HairStyle,
    pub flair: FlairStyle,
    /// Non-destructive modifier stack applied to every generated part mesh.
    pub modifiers: Vec<crate::mesh_modifiers::MeshModifier>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotContent {
    None,
    Cloth,
    Suit,
    Mecha,
}

impl CharacterPatch {
    pub fn morph(&self, name: &str) -> f32 {
        self.morphs.get(name).copied().unwrap_or(0.5)
    }

    pub fn material(&self, name: &str) -> Color {
        self.materials
            .get(name)
            .copied()
            .unwrap_or(Color::srgb(0.8, 0.2, 0.8))
    }
}

// ── Generator trait ───────────────────────────────────────────────────────────

pub trait PresetGenerator: Send + Sync {
    fn generate(&self, spec: &CharacterSpec, patch: &mut CharacterPatch);
}

/// Run the standard generator stack over a spec.
pub fn build_character_patch(spec: &CharacterSpec) -> CharacterPatch {
    let generators: [&dyn PresetGenerator; 5] = [
        &BodyGenerator,
        &FaceGenerator,
        &SkinGenerator,
        &HairGenerator,
        &OutfitGenerator,
    ];
    let mut patch = CharacterPatch {
        sex: spec.sex,
        wardrobe: spec.style.wardrobe,
        hair: spec.style.hair,
        flair: spec.style.flair,
        modifiers: spec.modifiers.iter().flatten().copied().collect(),
        ..default()
    };
    for generator in generators {
        generator.generate(spec, &mut patch);
    }
    patch
}

// ── Palettes ──────────────────────────────────────────────────────────────────

pub fn skin_tones() -> [Color; 8] {
    [
        Color::srgb(0.98, 0.86, 0.76),
        Color::srgb(0.95, 0.78, 0.66),
        Color::srgb(0.90, 0.71, 0.55),
        Color::srgb(0.82, 0.60, 0.44),
        Color::srgb(0.70, 0.48, 0.34),
        Color::srgb(0.55, 0.36, 0.25),
        Color::srgb(0.42, 0.27, 0.19),
        Color::srgb(0.30, 0.19, 0.14),
    ]
}

pub fn eye_colors() -> [Color; 8] {
    [
        Color::srgb(0.24, 0.42, 0.65), // blue
        Color::srgb(0.20, 0.45, 0.28), // green
        Color::srgb(0.42, 0.28, 0.14), // brown
        Color::srgb(0.30, 0.30, 0.32), // grey
        Color::srgb(0.55, 0.38, 0.16), // amber
        Color::srgb(0.36, 0.24, 0.42), // violet
        Color::srgb(0.10, 0.72, 0.82), // anime cyan
        Color::srgb(0.68, 0.16, 0.22), // anime crimson
    ]
}

pub fn hair_colors() -> [Color; 12] {
    [
        Color::srgb(0.08, 0.06, 0.05), // black
        Color::srgb(0.24, 0.14, 0.08), // dark brown
        Color::srgb(0.45, 0.28, 0.14), // brown
        Color::srgb(0.72, 0.52, 0.24), // blonde
        Color::srgb(0.55, 0.20, 0.10), // auburn
        Color::srgb(0.62, 0.62, 0.64), // silver
        Color::srgb(0.85, 0.32, 0.16), // ginger
        Color::srgb(0.28, 0.42, 0.72), // sci-fi blue
        Color::srgb(0.88, 0.48, 0.62), // cel pink
        Color::srgb(0.15, 0.62, 0.60), // teal
        Color::srgb(0.48, 0.28, 0.66), // violet
        Color::srgb(0.88, 0.84, 0.72), // platinum
    ]
}

pub fn outfit_colors() -> [Color; 12] {
    [
        Color::srgb(0.20, 0.35, 0.68), // cobalt
        Color::srgb(0.70, 0.16, 0.14), // crimson
        Color::srgb(0.16, 0.50, 0.32), // emerald
        Color::srgb(0.85, 0.66, 0.16), // gold
        Color::srgb(0.42, 0.24, 0.62), // violet
        Color::srgb(0.90, 0.90, 0.92), // white
        Color::srgb(0.16, 0.16, 0.20), // charcoal
        Color::srgb(0.10, 0.75, 0.78), // cyan
        Color::srgb(0.92, 0.22, 0.52), // neon pink
        Color::srgb(0.92, 0.38, 0.10), // sunset orange
        Color::srgb(0.07, 0.12, 0.30), // academy navy
        Color::srgb(0.58, 0.42, 0.28), // earth tan
    ]
}

fn pick(palette: &[Color], index: usize) -> Color {
    palette[index % palette.len()]
}

// ── Generators ────────────────────────────────────────────────────────────────

/// Height, mass, shoulders/waist/hips, limb length — plus the sex baseline
/// (female: narrower shoulders, wider hips, lighter musculature).
pub struct BodyGenerator;

impl PresetGenerator for BodyGenerator {
    fn generate(&self, spec: &CharacterSpec, patch: &mut CharacterPatch) {
        let b = &spec.body;
        patch.morphs.insert("body_height", b.height);
        patch.morphs.insert("body_muscle", b.muscle);
        patch.morphs.insert("body_weight", b.weight);
        patch.morphs.insert("shoulders_wide", b.shoulder_width);
        patch.morphs.insert("waist_width", b.waist_width);
        patch.morphs.insert("hips_wide", b.hip_width);
        patch.morphs.insert("limb_length", b.limb_length);
        patch.morphs.insert("body_chest_shape", b.chest_shape);
    }
}

/// Jaw, chin, nose, brow, cheeks, eyes, mouth.
pub struct FaceGenerator;

impl PresetGenerator for FaceGenerator {
    fn generate(&self, spec: &CharacterSpec, patch: &mut CharacterPatch) {
        let f = &spec.face;
        patch.morphs.insert("face_jaw_wide", f.jaw_width);
        patch.morphs.insert("face_chin_long", f.chin_length);
        patch.morphs.insert("face_chin_wide", f.chin_width);
        patch.morphs.insert("face_nose_long", f.nose_length);
        patch.morphs.insert("face_nose_wide", f.nose_width);
        patch.morphs.insert("face_nose_bridge", f.nose_bridge);
        patch.morphs.insert("face_brow_heavy", f.brow_depth);
        patch.morphs.insert("face_cheek_full", f.cheek_fullness);
        patch.morphs.insert("face_eye_large", f.eye_size);
        patch.morphs.insert("face_eye_spacing", f.eye_spacing);
        patch.morphs.insert("face_eye_tilt", f.eye_tilt);
        patch.morphs.insert("face_eye_depth", f.eye_depth);
        patch.morphs.insert("face_brow_angle", f.brow_angle);
        patch.morphs.insert("face_mouth_wide", f.mouth_width);
        patch.morphs.insert("face_lip_full", f.lip_fullness);
    }
}

pub struct SkinGenerator;

impl PresetGenerator for SkinGenerator {
    fn generate(&self, spec: &CharacterSpec, patch: &mut CharacterPatch) {
        patch
            .materials
            .insert("skin", pick(&skin_tones(), spec.style.skin_tone));
        patch
            .materials
            .insert("eyes", pick(&eye_colors(), spec.style.eye_color));
    }
}

pub struct HairGenerator;

impl PresetGenerator for HairGenerator {
    fn generate(&self, spec: &CharacterSpec, patch: &mut CharacterPatch) {
        patch
            .materials
            .insert("hair", pick(&hair_colors(), spec.style.hair_color));
        let content = if spec.style.hair == HairStyle::Bald {
            SlotContent::None
        } else {
            SlotContent::Cloth
        };
        patch.slots.insert(CharacterSlot::Hair, content);
    }
}

/// Fills the torso/legs/feet/hands/helmet slots per outfit layer and sets the
/// primary/secondary colours everything downstream tints with.
pub struct OutfitGenerator;

impl PresetGenerator for OutfitGenerator {
    fn generate(&self, spec: &CharacterSpec, patch: &mut CharacterPatch) {
        patch
            .materials
            .insert("primary", pick(&outfit_colors(), spec.style.primary_color));
        patch.materials.insert(
            "secondary",
            pick(&outfit_colors(), spec.style.secondary_color),
        );

        use CharacterSlot as S;
        use SlotContent as C;
        let w = spec.style.wardrobe;
        // Armor overrides the wardrobe visuals when active.
        let fill: [(S, C); 5] = match w.armor {
            ArmorStyle::SuperSuit => [
                (S::Torso, C::Suit),
                (S::Legs, C::Suit),
                (S::Feet, C::Suit),
                (S::Hands, C::Suit),
                (S::Helmet, C::None),
            ],
            ArmorStyle::MechaArmor | ArmorStyle::CrystalMecha | ArmorStyle::DragonMecha => [
                (S::Torso, C::Mecha),
                (S::Legs, C::Mecha),
                (S::Feet, C::Mecha),
                (S::Hands, C::Mecha),
                (S::Helmet, C::Mecha),
            ],
            ArmorStyle::None => [
                (
                    S::Torso,
                    if w.top == TopStyle::None {
                        C::None
                    } else {
                        C::Cloth
                    },
                ),
                (
                    S::Legs,
                    if w.bottom == BottomStyle::Underwear {
                        C::None
                    } else {
                        C::Cloth
                    },
                ),
                (
                    S::Feet,
                    if w.feet == FootStyle::Barefoot {
                        C::None
                    } else {
                        C::Cloth
                    },
                ),
                (
                    S::Hands,
                    match w.hands {
                        HandStyle::Bare => C::None,
                        HandStyle::Gloves => C::Suit,
                        HandStyle::Gauntlets => C::Mecha,
                    },
                ),
                (S::Helmet, C::None),
            ],
        };
        for (slot, content) in fill {
            patch.slots.insert(slot, content);
        }
    }
}

// ── Spec presets (the buttons) ────────────────────────────────────────────────

/// Realistic adult male baseline.
pub fn preset_male() -> CharacterSpec {
    let mut spec = CharacterSpec {
        sex: Sex::Male,
        ..default()
    };
    spec.body.shoulder_width = 0.62;
    spec.body.muscle = 0.55;
    spec.body.chest_shape = 0.58;
    spec.face.jaw_width = 0.58;
    spec.face.brow_depth = 0.60;
    spec.style.hair = HairStyle::Short;
    spec.style.hair_color = 1;
    spec.style.skin_tone = 2;
    spec.style.wardrobe = WardrobeSpec {
        top: TopStyle::TShirt,
        bottom: BottomStyle::Pants,
        feet: FootStyle::Boots,
        hands: HandStyle::Bare,
        armor: ArmorStyle::None,
    };
    spec
}

/// Realistic adult female baseline.
pub fn preset_female() -> CharacterSpec {
    let mut spec = CharacterSpec {
        sex: Sex::Female,
        ..default()
    };
    spec.body.shoulder_width = 0.40;
    spec.body.hip_width = 0.62;
    spec.body.waist_width = 0.38;
    spec.body.muscle = 0.42;
    spec.body.chest_shape = 0.58;
    spec.face.jaw_width = 0.38;
    spec.face.chin_length = 0.42;
    spec.face.brow_depth = 0.36;
    spec.face.eye_size = 0.60;
    spec.face.mouth_width = 0.55;
    spec.style.hair = HairStyle::Long;
    spec.style.hair_color = 3;
    spec.style.skin_tone = 1;
    spec.style.wardrobe = WardrobeSpec {
        top: TopStyle::Tunic,
        bottom: BottomStyle::Pants,
        feet: FootStyle::TallBoots,
        hands: HandStyle::Bare,
        armor: ArmorStyle::None,
    };
    spec
}

/// Bright, readable good-guy silhouette: fantasy tunic, heroic anime face,
/// and saturated 1970s space-opera colours.
pub fn preset_star_hero() -> CharacterSpec {
    let mut spec = preset_male();
    spec.body.height = 0.46;
    spec.body.muscle = 0.62;
    spec.body.limb_length = 0.44;
    spec.face.eye_size = 0.68;
    spec.face.eye_spacing = 0.54;
    spec.face.eye_tilt = 0.58;
    spec.face.brow_angle = 0.56;
    spec.face.jaw_width = 0.48;
    spec.face.chin_width = 0.48;
    spec.face.nose_bridge = 0.54;
    spec.face.lip_fullness = 0.42;
    spec.style.hair = HairStyle::Feathered;
    spec.style.hair_color = 3;
    spec.style.primary_color = 0;
    spec.style.secondary_color = 3;
    spec.style.flair = FlairStyle::StarGem;
    spec.style.wardrobe = WardrobeSpec {
        top: TopStyle::Tunic,
        bottom: BottomStyle::Pants,
        feet: FootStyle::TallBoots,
        hands: HandStyle::Gloves,
        armor: ArmorStyle::None,
    };
    spec
}

/// Villain/bad-guy seed with a sharper face, dark street-military clothing,
/// and crimson eyes. It remains fully editable after applying the preset.
pub fn preset_shadow_raider() -> CharacterSpec {
    let mut spec = preset_male();
    spec.body.height = 0.62;
    spec.body.muscle = 0.70;
    spec.face.jaw_width = 0.66;
    spec.face.chin_length = 0.64;
    spec.face.chin_width = 0.62;
    spec.face.nose_bridge = 0.68;
    spec.face.eye_depth = 0.64;
    spec.face.cheek_fullness = 0.28;
    spec.face.eye_size = 0.43;
    spec.face.eye_spacing = 0.47;
    spec.face.eye_tilt = 0.78;
    spec.face.brow_angle = 0.82;
    spec.face.brow_depth = 0.76;
    spec.style.eye_color = 7;
    spec.style.hair = HairStyle::Spiky;
    spec.style.hair_color = 0;
    spec.style.primary_color = 6;
    spec.style.secondary_color = 1;
    spec.style.wardrobe = WardrobeSpec {
        top: TopStyle::MotoJacket,
        bottom: BottomStyle::CargoPants,
        feet: FootStyle::CombatBoots,
        hands: HandStyle::Gloves,
        armor: ArmorStyle::None,
    };
    spec
}

/// Secret-of-Mana-style compact fantasy adventurer suitable for the shared
/// camera and top-down cave levels.
pub fn preset_mana_adventurer() -> CharacterSpec {
    let mut spec = preset_female();
    spec.body.height = 0.35;
    spec.body.limb_length = 0.38;
    spec.face.eye_size = 0.76;
    spec.face.eye_spacing = 0.57;
    spec.face.eye_tilt = 0.54;
    spec.face.cheek_fullness = 0.64;
    spec.face.lip_fullness = 0.48;
    spec.face.chin_width = 0.40;
    spec.face.nose_bridge = 0.42;
    spec.face.eye_depth = 0.42;
    spec.style.hair = HairStyle::SidePonytail;
    spec.style.hair_color = 4;
    spec.style.primary_color = 2;
    spec.style.secondary_color = 11;
    spec.style.wardrobe = WardrobeSpec {
        top: TopStyle::ArcaneCoat,
        bottom: BottomStyle::Shorts,
        feet: FootStyle::Boots,
        hands: HandStyle::Bare,
        armor: ArmorStyle::None,
    };
    spec.style.flair = FlairStyle::MoonGem;
    spec
}

/// Contemporary street-clothes seed for city characters and civilians.
pub fn preset_street_runner() -> CharacterSpec {
    let mut spec = preset_female();
    spec.body.muscle = 0.58;
    spec.body.limb_length = 0.58;
    spec.face.eye_size = 0.62;
    spec.face.eye_tilt = 0.60;
    spec.face.brow_angle = 0.60;
    spec.style.hair = HairStyle::Bob;
    spec.style.hair_color = 7;
    spec.style.primary_color = 9;
    spec.style.secondary_color = 10;
    spec.style.wardrobe = WardrobeSpec {
        top: TopStyle::BomberJacket,
        bottom: BottomStyle::Jeans,
        feet: FootStyle::HighTops,
        hands: HandStyle::Bare,
        armor: ArmorStyle::None,
    };
    spec
}

/// Promotes the generator's successful Mega-Man-like armored body into a
/// named preset rather than leaving it hidden in the armor style selector.
pub fn preset_mecha_robot() -> CharacterSpec {
    let mut spec = preset_male();
    spec.body.height = 0.52;
    spec.body.muscle = 0.82;
    spec.body.shoulder_width = 0.76;
    spec.body.waist_width = 0.38;
    spec.body.chest_shape = 0.72;
    spec.face.eye_size = 0.60;
    spec.face.eye_tilt = 0.66;
    spec.style.hair = HairStyle::Bald;
    spec.style.eye_color = 6;
    spec.style.primary_color = 0;
    spec.style.secondary_color = 7;
    spec.style.flair = FlairStyle::MechaWings;
    spec.style.wardrobe = WardrobeSpec {
        top: TopStyle::None,
        bottom: BottomStyle::Pants,
        feet: FootStyle::Boots,
        hands: HandStyle::Gauntlets,
        armor: ArmorStyle::CrystalMecha,
    };
    spec
}

/// Physique modifiers — applied on top of the current spec, preserving sex,
/// face and style.
pub fn apply_athletic(spec: &mut CharacterSpec) {
    spec.body.muscle = 0.78;
    spec.body.weight = 0.42;
    spec.body.shoulder_width = (spec.body.shoulder_width + 0.15).min(1.0);
    spec.body.waist_width = 0.35;
}

pub fn apply_heavy(spec: &mut CharacterSpec) {
    spec.body.weight = 0.85;
    spec.body.muscle = 0.55;
    spec.body.waist_width = 0.80;
    spec.body.hip_width = (spec.body.hip_width + 0.12).min(1.0);
}

pub fn apply_slim(spec: &mut CharacterSpec) {
    spec.body.weight = 0.22;
    spec.body.muscle = 0.35;
    spec.body.waist_width = 0.30;
    spec.body.shoulder_width = (spec.body.shoulder_width - 0.08).max(0.0);
    spec.body.limb_length = 0.62;
}

pub fn apply_soft_face(spec: &mut CharacterSpec) {
    spec.face.jaw_width = 0.38;
    spec.face.chin_length = 0.40;
    spec.face.cheek_fullness = 0.68;
    spec.face.brow_depth = 0.38;
    spec.face.nose_width = 0.45;
}

pub fn apply_sharp_face(spec: &mut CharacterSpec) {
    spec.face.jaw_width = 0.62;
    spec.face.chin_length = 0.62;
    spec.face.cheek_fullness = 0.30;
    spec.face.brow_depth = 0.66;
    spec.face.nose_length = 0.60;
}

/// Full random character (body + face + style), sex included.
pub fn randomize(spec: &mut CharacterSpec, rng: &mut impl rand::Rng) {
    let base = if rng.gen_bool(0.5) {
        preset_male()
    } else {
        preset_female()
    };
    *spec = base;
    let mut jitter = |v: &mut f32, amount: f32| {
        *v = (*v + rng.gen_range(-amount..amount)).clamp(0.0, 1.0);
    };
    jitter(&mut spec.body.height, 0.35);
    jitter(&mut spec.body.muscle, 0.30);
    jitter(&mut spec.body.weight, 0.30);
    jitter(&mut spec.body.shoulder_width, 0.20);
    jitter(&mut spec.body.waist_width, 0.22);
    jitter(&mut spec.body.hip_width, 0.20);
    jitter(&mut spec.body.limb_length, 0.20);
    jitter(&mut spec.body.chest_shape, 0.25);
    jitter(&mut spec.face.jaw_width, 0.30);
    jitter(&mut spec.face.chin_length, 0.30);
    jitter(&mut spec.face.chin_width, 0.30);
    jitter(&mut spec.face.nose_length, 0.35);
    jitter(&mut spec.face.nose_width, 0.35);
    jitter(&mut spec.face.nose_bridge, 0.30);
    jitter(&mut spec.face.brow_depth, 0.30);
    jitter(&mut spec.face.cheek_fullness, 0.30);
    jitter(&mut spec.face.eye_size, 0.25);
    jitter(&mut spec.face.eye_spacing, 0.20);
    jitter(&mut spec.face.eye_tilt, 0.25);
    jitter(&mut spec.face.eye_depth, 0.22);
    jitter(&mut spec.face.brow_angle, 0.28);
    jitter(&mut spec.face.mouth_width, 0.25);
    jitter(&mut spec.face.lip_fullness, 0.25);
    spec.style.skin_tone = rng.gen_range(0..8);
    spec.style.eye_color = rng.gen_range(0..6);
    spec.style.hair = super::spec::HairStyle::ALL[rng.gen_range(0..5)];
    spec.style.hair_color = rng.gen_range(0..8);
    spec.style.primary_color = rng.gen_range(0..8);
    spec.style.secondary_color = rng.gen_range(0..8);
    spec.style.flair = FlairStyle::ALL[rng.gen_range(0..FlairStyle::ALL.len())];
    spec.style.wardrobe = WardrobeSpec {
        top: TopStyle::ALL[rng.gen_range(1..TopStyle::ALL.len())],
        bottom: BottomStyle::ALL[rng.gen_range(1..BottomStyle::ALL.len())],
        feet: FootStyle::ALL[rng.gen_range(0..FootStyle::ALL.len())],
        hands: HandStyle::ALL[rng.gen_range(0..HandStyle::ALL.len())],
        armor: if rng.gen_bool(0.25) {
            ArmorStyle::ALL[rng.gen_range(1..ArmorStyle::ALL.len())]
        } else {
            ArmorStyle::None
        },
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faction_and_genre_presets_keep_distinct_playable_silhouettes() {
        let hero = preset_star_hero();
        let villain = preset_shadow_raider();
        let adventurer = preset_mana_adventurer();
        let street = preset_street_runner();

        assert_ne!(hero.style.primary_color, villain.style.primary_color);
        assert!(villain.face.brow_angle > hero.face.brow_angle);
        assert!(adventurer.face.eye_size > villain.face.eye_size);
        assert_eq!(street.style.wardrobe.top, TopStyle::BomberJacket);
        assert_eq!(street.style.wardrobe.bottom, BottomStyle::Jeans);
    }

    #[test]
    fn mecha_robot_is_a_named_full_armor_preset() {
        let spec = preset_mecha_robot();
        let patch = build_character_patch(&spec);
        assert_eq!(spec.style.wardrobe.armor, ArmorStyle::CrystalMecha);
        assert_eq!(spec.style.flair, FlairStyle::MechaWings);
        assert_eq!(
            patch.slots.get(&CharacterSlot::Torso),
            Some(&SlotContent::Mecha)
        );
        assert_eq!(
            patch.slots.get(&CharacterSlot::Helmet),
            Some(&SlotContent::Mecha)
        );
    }

    #[test]
    fn expanded_face_controls_reach_the_patch_contract() {
        let mut spec = CharacterSpec::default();
        spec.face.eye_spacing = 0.77;
        spec.face.eye_tilt = 0.68;
        spec.face.eye_depth = 0.57;
        spec.face.chin_width = 0.46;
        spec.face.nose_bridge = 0.72;
        spec.face.brow_angle = 0.81;
        spec.face.lip_fullness = 0.63;
        spec.body.chest_shape = 0.69;
        let patch = build_character_patch(&spec);
        assert_eq!(patch.morph("face_eye_spacing"), 0.77);
        assert_eq!(patch.morph("face_eye_tilt"), 0.68);
        assert_eq!(patch.morph("face_eye_depth"), 0.57);
        assert_eq!(patch.morph("face_chin_wide"), 0.46);
        assert_eq!(patch.morph("face_nose_bridge"), 0.72);
        assert_eq!(patch.morph("face_brow_angle"), 0.81);
        assert_eq!(patch.morph("face_lip_full"), 0.63);
        assert_eq!(patch.morph("body_chest_shape"), 0.69);
    }

    #[test]
    fn fantasy_styles_reach_the_patch_without_overriding_wardrobe_layers() {
        let mut spec = preset_mana_adventurer();
        spec.style.wardrobe.top = TopStyle::StarKnightTunic;
        spec.style.flair = FlairStyle::ArcaneHalo;
        let patch = build_character_patch(&spec);
        assert_eq!(patch.wardrobe.top, TopStyle::StarKnightTunic);
        assert_eq!(patch.flair, FlairStyle::ArcaneHalo);
        assert_eq!(
            patch.slots.get(&CharacterSlot::Torso),
            Some(&SlotContent::Cloth)
        );
    }
}
