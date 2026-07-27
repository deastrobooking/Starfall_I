//! Native math-modeled character builder.
//!
//! A component-based, fully-procedural (pure-Rust) hero builder layered on the
//! socket-assembly system in [`crate::character::modular`]. Each hero is assembled
//! from math primitives ([`superellipsoid_mesh`]) snapped together at named
//! sockets: base limbs in an under-suit, then armor *components* (chest plate,
//! pauldrons, helmet, visor, gauntlets, greaves, cape, belt) added on top.
//!
//! The GLBs under `assets/Characters/` are **visual reference only** — never
//! loaded. Their analysis drives this builder: palettes sampled from the baked
//! textures, heroic/leggy proportions from AMP's skeleton, and the layered-plate
//! silhouette of the Tripo models. Per-hero variation is read from the existing
//! [`CartoonCharacterConfig`] (palette + feature flags + build width).
//!
//! Figure space is normalized: feet at `y = 0`, head top at `y ≈ 1.0`, torso
//! centre at [`TORSO_CENTER_Y`]. At attach time the rig is scaled/offset to fill
//! the player's physics capsule and parented under the player entity.

use bevy::prelude::*;

use crate::character::modular::{
    spawn_character_under, Attachment, CharacterRecipe, PartPrefab, PartRegistry, Socket,
};
use crate::character::parts::{
    ArmPreset, BodyPreset, CharacterLoadout, HeadPreset, LegPreset, ShoulderPreset,
};
use crate::character::presets::{emissive_mat, CartoonCharacterConfig};
use crate::character::procedural_meshes::superellipsoid_mesh;

/// Torso centre height in normalized figure space (feet = 0, head top ≈ 1.0).
/// Raised alongside the Mœbius leg elongation so longer legs still land the feet
/// on the capsule bottom.
const TORSO_CENTER_Y: f32 = 0.68;

// ── Mœbius × Mana silhouette (style constants) ────────────────────────────────
//
// Full-humanoid fantasy-sci silhouette in the spirit of Mœbius (Jean Giraud)
// line-art heroes and Secret of Mana's luminous fantasy: elongated legs, a long
// lean line through the figure, slimmer limbs, and a slightly smaller head
// (~7.5 head-heights instead of the stockier ~6). Applied globally on top of
// every per-preset/per-profile proportion so hero identity is preserved.

/// Leg segment elongation (thigh + shin).
const ELEGANT_LEG: f32 = 1.14;
/// Arm segment elongation (upper + forearm).
const ELEGANT_ARM: f32 = 1.07;
/// Limb cross-section slimming — long thin lines, not bulk.
const ELEGANT_SLIM: f32 = 0.90;
/// Head/helmet reduction — reads as taller, more heroic figure.
const ELEGANT_HEAD: f32 = 0.93;

// ── Mesh primitives ────────────────────────────────────────────────────────────

/// Soft organic volume (limbs, head) — puffy, slightly boxy.
fn organic(meshes: &mut Assets<Mesh>, radii: Vec3) -> Handle<Mesh> {
    meshes.add(superellipsoid_mesh(radii, 0.82, 0.82, 16, 22))
}
/// Hard armor plate — boxy with rounded bevels.
fn plate(meshes: &mut Assets<Mesh>, radii: Vec3) -> Handle<Mesh> {
    meshes.add(superellipsoid_mesh(radii, 0.42, 0.42, 14, 18))
}
/// Faceted gem (eyes, visor, buckle) for emissive accents.
fn gem(meshes: &mut Assets<Mesh>, radii: Vec3) -> Handle<Mesh> {
    meshes.add(superellipsoid_mesh(radii, 0.55, 0.55, 12, 16))
}

fn sock(name: &'static str, x: f32, y: f32, z: f32) -> Socket {
    Socket::new(name, Transform::from_xyz(x, y, z))
}
fn sock_rot(name: &'static str, x: f32, y: f32, z: f32, rot: Quat) -> Socket {
    Socket::new(name, Transform::from_xyz(x, y, z).with_rotation(rot))
}

// ── Colour helpers ───────────────────────────────────────────────────────────

fn scl(c: Color, f: f32) -> Color {
    let s = c.to_srgba();
    Color::srgb(
        (s.red * f).clamp(0.0, 1.0),
        (s.green * f).clamp(0.0, 1.0),
        (s.blue * f).clamp(0.0, 1.0),
    )
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    let x = a.to_srgba();
    let y = b.to_srgba();
    Color::srgb(
        x.red + (y.red - x.red) * t,
        x.green + (y.green - x.green) * t,
        x.blue + (y.blue - x.blue) * t,
    )
}

/// Metallic-armor material (brass tinted toward the accent), distinct from the
/// matte cloth used elsewhere.
fn metal_mat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.42,
        metallic: 0.45,
        reflectance: 0.38,
        ..default()
    })
}

/// Painterly flat cloth for the hero (Mœbius/Mana look): very matte, low
/// reflectance, and a lifted self-emissive floor so colours stay luminous and
/// flat in shadow instead of going muddy. Local to the hero builder — enemies
/// keep the shared `char_mat` look.
fn cloth_mat(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    let lin = color.to_linear();
    materials.add(StandardMaterial {
        base_color: color,
        emissive: LinearRgba::new(lin.red * 0.22, lin.green * 0.22, lin.blue * 0.22, 1.0),
        perceptual_roughness: 0.92,
        metallic: 0.0,
        reflectance: 0.06,
        ..default()
    })
}

/// Mœbius shadows go indigo, not black: darken a colour by pulling it toward a
/// deep blue-violet rather than scaling to grey.
fn indigo_shade(color: Color, t: f32) -> Color {
    mix(scl(color, 0.72), Color::srgb(0.16, 0.14, 0.34), t)
}

// ── Hero design (component spec derived from config) ────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlbReferenceProfile {
    Amp,
    Antonio,
    Chroma,
    Daria,
    Vincenzo,
}

impl GlbReferenceProfile {
    fn from_config(cfg: &CartoonCharacterConfig, loadout: CharacterLoadout) -> Self {
        match cfg.name {
            "AMP" | "Amp" | "Angelo" | "Joseph" => GlbReferenceProfile::Amp,
            "Antonio" | "Fortuna" => GlbReferenceProfile::Antonio,
            "Daria" | "Gabriella" | "Aurora" => GlbReferenceProfile::Daria,
            "Vincenzo" => GlbReferenceProfile::Vincenzo,
            "Chroma" | "Nova" => GlbReferenceProfile::Chroma,
            _ => {
                if loadout == CharacterLoadout::legacy_stock()
                    || loadout
                        == CharacterLoadout::new(
                            BodyPreset::HeavyPlate,
                            ArmPreset::HeavyArms,
                            LegPreset::HeavyLegs,
                            ShoulderPreset::PlateEpaulettes,
                            HeadPreset::FullHelm,
                        )
                {
                    GlbReferenceProfile::Amp
                } else if loadout
                    == CharacterLoadout::new(
                        BodyPreset::RiftMantle,
                        ArmPreset::RiftTalons,
                        LegPreset::RiftBoots,
                        ShoulderPreset::RiftCloak,
                        HeadPreset::RiftCowl,
                    )
                {
                    GlbReferenceProfile::Antonio
                } else if loadout
                    == CharacterLoadout::new(
                        BodyPreset::DariaCore,
                        ArmPreset::DariaCannon,
                        LegPreset::DariaGreaves,
                        ShoulderPreset::DariaFlares,
                        HeadPreset::DariaHelm,
                    )
                {
                    GlbReferenceProfile::Daria
                } else {
                    GlbReferenceProfile::Chroma
                }
            }
        }
    }
}

struct HeroDesign {
    #[allow(dead_code)]
    profile: GlbReferenceProfile,
    /// X scale of torso/shoulders/hips (broad vs lean).
    width: f32,
    /// Cross-section multiplier for limbs (bulk).
    limb: f32,
    torso_height: f32,
    torso_depth: f32,
    chest_width: f32,
    chest_height: f32,
    cape_width: f32,
    cape_height: f32,
    head_width: f32,
    head_height: f32,
    helmet_width: f32,
    helmet_height: f32,
    helmet_brow_scale: f32,
    jaw_guard_scale: f32,
    visor_width: f32,
    shoulder_width: f32,
    shoulder_height: f32,
    upper_arm_length: f32,
    forearm_length: f32,
    hand_scale: f32,
    thigh_length: f32,
    shin_length: f32,
    foot_length: f32,
    chest_core_scale: f32,
    crest_width: f32,
    crest_height: f32,
    side_fin_width: f32,
    side_fin_height: f32,
    back_shard_width: f32,
    back_shard_height: f32,
    wrist_blade_length: f32,
    wrist_cannon_scale: f32,
    elbow_guard_scale: f32,
    hip_fin_scale: f32,
    knee_guard_scale: f32,
    shin_thruster_scale: f32,
    toe_spike_scale: f32,
    heel_spur_scale: f32,
    shoulder_fin_scale: f32,
    pauldrons: bool,
    cape: bool,
    visor: bool,
    gauntlets: bool,
    boots: bool,
    belt: bool,
}

impl HeroDesign {
    fn from_config(cfg: &CartoonCharacterConfig, loadout: CharacterLoadout) -> Self {
        let profile = GlbReferenceProfile::from_config(cfg, loadout);
        let body = cfg.body.validated();
        let body_width = match loadout.body {
            BodyPreset::HeavyPlate => 1.16,
            BodyPreset::RiftMantle => 0.88,
            BodyPreset::DariaCore => 1.04,
            BodyPreset::ChromaFrame => 0.96,
            BodyPreset::StandardMech => 1.08,
            BodyPreset::ScoutVest => 0.92,
            BodyPreset::VoidArmor => 1.00,
        };
        let (torso_height, torso_depth, chest_width, chest_height) = match loadout.body {
            BodyPreset::HeavyPlate => (1.06, 1.10, 1.16, 1.08),
            BodyPreset::RiftMantle => (1.14, 0.92, 0.86, 1.18),
            BodyPreset::DariaCore => (0.98, 0.98, 1.06, 0.92),
            BodyPreset::ChromaFrame => (1.02, 0.94, 0.96, 1.03),
            BodyPreset::StandardMech => (1.00, 1.04, 1.06, 1.00),
            BodyPreset::ScoutVest => (0.96, 0.90, 0.90, 0.92),
            BodyPreset::VoidArmor => (1.03, 0.96, 1.00, 1.04),
        };
        let arm_bulk = match loadout.arms {
            ArmPreset::HeavyArms => 1.24,
            ArmPreset::DariaCannon => 1.18,
            ArmPreset::RiftTalons => 0.92,
            ArmPreset::ChromaBlades => 0.98,
            ArmPreset::MechArms => 1.08,
            ArmPreset::ScoutArms => 0.88,
            ArmPreset::ClawArms => 1.02,
        };
        let (upper_arm_length, forearm_length, hand_scale) = match loadout.arms {
            ArmPreset::HeavyArms => (0.98, 1.00, 1.16),
            ArmPreset::DariaCannon => (0.94, 1.08, 1.30),
            ArmPreset::RiftTalons => (1.08, 1.18, 1.34),
            ArmPreset::ChromaBlades => (1.02, 1.14, 1.08),
            ArmPreset::MechArms => (1.00, 1.00, 1.05),
            ArmPreset::ScoutArms => (1.04, 1.05, 0.92),
            ArmPreset::ClawArms => (1.03, 1.10, 1.22),
        };
        let leg_bulk = match loadout.legs {
            LegPreset::HeavyLegs => 1.22,
            LegPreset::DariaGreaves => 1.12,
            LegPreset::RiftBoots => 0.94,
            LegPreset::ChromaStriders => 0.96,
            LegPreset::MechLegs => 1.06,
            LegPreset::ScoutLegs => 0.88,
            LegPreset::JetLegs => 1.00,
        };
        let (thigh_length, shin_length, foot_length) = match loadout.legs {
            LegPreset::HeavyLegs => (0.96, 0.98, 1.12),
            LegPreset::DariaGreaves => (1.00, 1.05, 1.18),
            LegPreset::RiftBoots => (1.06, 1.16, 1.28),
            LegPreset::ChromaStriders => (1.08, 1.18, 1.08),
            LegPreset::MechLegs => (1.00, 1.00, 1.05),
            LegPreset::ScoutLegs => (1.05, 1.08, 0.96),
            LegPreset::JetLegs => (1.02, 1.12, 1.10),
        };
        let (shoulder_width, shoulder_height, mantle_cape) = match loadout.shoulders {
            ShoulderPreset::PlateEpaulettes => (1.20, 0.82, false),
            ShoulderPreset::ChromaMantle => (1.02, 0.92, false),
            ShoulderPreset::RiftCloak => (0.96, 1.05, true),
            ShoulderPreset::DariaFlares => (1.18, 1.02, false),
            ShoulderPreset::DomePauldrons => (1.00, 1.00, false),
            ShoulderPreset::SpikedPauldrons => (1.08, 1.12, false),
            ShoulderPreset::None => (0.0, 0.0, false),
        };
        let (head_width, head_height, helmet_width, helmet_height, visor_width) = match loadout.head
        {
            HeadPreset::FullHelm => (1.02, 1.00, 1.14, 1.08, 1.04),
            HeadPreset::ChromaCrown => (0.98, 1.02, 1.18, 0.92, 1.18),
            HeadPreset::RiftCowl => (0.92, 1.12, 0.96, 1.18, 0.82),
            HeadPreset::DariaHelm => (1.02, 0.96, 1.08, 0.96, 1.08),
            HeadPreset::OpenFace => (1.00, 1.00, 0.96, 0.86, 0.82),
            HeadPreset::CombatHelmet => (1.00, 0.98, 1.06, 0.98, 1.00),
            HeadPreset::VoidMask => (1.04, 1.06, 1.08, 1.04, 0.92),
        };
        let helmet_brow_scale = match loadout.head {
            HeadPreset::OpenFace => 0.0,
            HeadPreset::CombatHelmet => 1.04,
            HeadPreset::FullHelm => 0.96,
            HeadPreset::VoidMask => 1.08,
            HeadPreset::ChromaCrown => 1.14,
            HeadPreset::RiftCowl => 0.76,
            HeadPreset::DariaHelm => 0.92,
        };
        let jaw_guard_scale = match loadout.head {
            HeadPreset::OpenFace => 0.0,
            HeadPreset::CombatHelmet => 0.76,
            HeadPreset::FullHelm => 1.02,
            HeadPreset::VoidMask => 0.74,
            HeadPreset::ChromaCrown => 0.70,
            HeadPreset::RiftCowl => 0.62,
            HeadPreset::DariaHelm => 0.98,
        };
        let (profile_width, profile_depth, profile_back, profile_crest, profile_side_fin) =
            match profile {
                GlbReferenceProfile::Amp => (1.18, 0.92, 0.00, 0.00, 0.00),
                GlbReferenceProfile::Antonio => (0.86, 1.22, 1.34, 0.92, 0.60),
                GlbReferenceProfile::Chroma => (0.98, 1.20, 1.12, 1.05, 0.72),
                GlbReferenceProfile::Daria => (1.02, 0.96, 0.42, 1.15, 1.10),
                GlbReferenceProfile::Vincenzo => (0.88, 1.42, 1.62, 0.78, 0.42),
            };
        let w = cfg.body_width
            * body_width
            * profile_width
            * (body.shoulder_width * 0.72 + body.chest_size * 0.28);
        let limb = (0.88 + 0.34 * (w - 1.0)) * (arm_bulk * 0.55 + leg_bulk * 0.45) * ELEGANT_SLIM;
        let chest_core_scale = match profile {
            GlbReferenceProfile::Amp => 1.20,
            GlbReferenceProfile::Antonio => 0.72,
            GlbReferenceProfile::Chroma => 1.05,
            GlbReferenceProfile::Daria => 0.95,
            GlbReferenceProfile::Vincenzo => 0.88,
        };
        let wrist_blade_length = match profile {
            GlbReferenceProfile::Antonio => 1.12,
            GlbReferenceProfile::Chroma => 0.92,
            GlbReferenceProfile::Vincenzo => 0.68,
            _ => 0.0,
        };
        let wrist_cannon_scale = match profile {
            GlbReferenceProfile::Daria => 1.12,
            GlbReferenceProfile::Amp => 0.86,
            _ => 0.0,
        };
        let elbow_guard_scale = match loadout.arms {
            ArmPreset::HeavyArms => 1.16,
            ArmPreset::DariaCannon => 1.08,
            ArmPreset::ChromaBlades => 0.82,
            ArmPreset::RiftTalons => 0.72,
            ArmPreset::MechArms => 0.96,
            ArmPreset::ClawArms => 0.88,
            ArmPreset::ScoutArms => 0.0,
        };
        let hip_fin_scale = match profile {
            GlbReferenceProfile::Antonio => 1.12,
            GlbReferenceProfile::Chroma => 0.92,
            GlbReferenceProfile::Daria => 1.04,
            GlbReferenceProfile::Vincenzo => 0.76,
            _ => 0.0,
        };
        let knee_guard_scale = match profile {
            GlbReferenceProfile::Amp => 1.10,
            GlbReferenceProfile::Daria => 0.96,
            GlbReferenceProfile::Chroma => 0.76,
            _ => 0.0,
        };
        let shin_thruster_scale = match loadout.legs {
            LegPreset::JetLegs => 1.22,
            LegPreset::ChromaStriders => 0.92,
            LegPreset::DariaGreaves => 0.84,
            LegPreset::HeavyLegs => 0.72,
            LegPreset::MechLegs => 0.78,
            LegPreset::RiftBoots => 0.52,
            LegPreset::ScoutLegs => 0.0,
        };
        let toe_spike_scale = match profile {
            GlbReferenceProfile::Antonio => 1.12,
            GlbReferenceProfile::Vincenzo => 1.28,
            GlbReferenceProfile::Chroma => 0.72,
            _ => 0.0,
        };
        let heel_spur_scale = match loadout.legs {
            LegPreset::RiftBoots => 1.02,
            LegPreset::DariaGreaves => 0.88,
            LegPreset::ChromaStriders => 0.78,
            LegPreset::HeavyLegs => 0.66,
            LegPreset::JetLegs => 0.86,
            LegPreset::MechLegs => 0.48,
            LegPreset::ScoutLegs => 0.0,
        };
        let shoulder_fin_scale = match loadout.shoulders {
            ShoulderPreset::DariaFlares => 1.22,
            ShoulderPreset::SpikedPauldrons => 1.08,
            ShoulderPreset::ChromaMantle => 0.84,
            ShoulderPreset::PlateEpaulettes => 0.62,
            ShoulderPreset::RiftCloak => 0.56,
            ShoulderPreset::DomePauldrons | ShoulderPreset::None => 0.0,
        };
        HeroDesign {
            profile,
            width: w,
            limb,
            torso_height,
            torso_depth: torso_depth * profile_depth,
            chest_width,
            chest_height,
            cape_width: (if mantle_cape { 1.20 } else { 1.0 }) * profile_depth.clamp(1.0, 1.32),
            cape_height: match loadout.shoulders {
                ShoulderPreset::RiftCloak => 1.32,
                ShoulderPreset::ChromaMantle => 1.08,
                ShoulderPreset::DariaFlares => 0.92,
                _ => 1.0,
            },
            head_width: head_width * ELEGANT_HEAD,
            head_height: head_height * ELEGANT_HEAD,
            helmet_width: helmet_width * ELEGANT_HEAD,
            helmet_height: helmet_height * ELEGANT_HEAD,
            helmet_brow_scale,
            jaw_guard_scale,
            visor_width: visor_width * ELEGANT_HEAD,
            shoulder_width,
            shoulder_height,
            upper_arm_length: upper_arm_length * body.arm_length * ELEGANT_ARM,
            forearm_length: forearm_length * body.arm_length * ELEGANT_ARM,
            hand_scale: hand_scale * body.hand_size,
            thigh_length: thigh_length * body.leg_length * ELEGANT_LEG,
            shin_length: shin_length * body.leg_length * ELEGANT_LEG,
            foot_length: foot_length * body.foot_size,
            chest_core_scale,
            crest_width: profile_crest,
            crest_height: match profile {
                GlbReferenceProfile::Antonio => 0.82,
                GlbReferenceProfile::Chroma => 1.12,
                GlbReferenceProfile::Daria => 1.05,
                GlbReferenceProfile::Vincenzo => 0.72,
                GlbReferenceProfile::Amp => 0.0,
            },
            side_fin_width: profile_side_fin,
            side_fin_height: match profile {
                GlbReferenceProfile::Daria => 1.06,
                GlbReferenceProfile::Chroma => 0.82,
                GlbReferenceProfile::Antonio => 0.72,
                GlbReferenceProfile::Vincenzo => 0.54,
                GlbReferenceProfile::Amp => 0.0,
            },
            back_shard_width: profile_back,
            back_shard_height: match profile {
                GlbReferenceProfile::Antonio => 1.34,
                GlbReferenceProfile::Chroma => 1.14,
                GlbReferenceProfile::Daria => 0.70,
                GlbReferenceProfile::Vincenzo => 1.60,
                GlbReferenceProfile::Amp => 0.0,
            },
            wrist_blade_length,
            wrist_cannon_scale,
            elbow_guard_scale,
            hip_fin_scale,
            knee_guard_scale,
            shin_thruster_scale,
            toe_spike_scale,
            heel_spur_scale,
            shoulder_fin_scale,
            pauldrons: cfg.has_shoulder_pads && loadout.shoulders != ShoulderPreset::None,
            cape: cfg.has_cape || mantle_cape || profile_back > 1.25,
            visor: cfg.has_visor,
            gauntlets: cfg.has_gloves,
            boots: cfg.has_boots,
            belt: cfg.has_belt,
        }
    }
}

struct Palette {
    /// Main armor / outfit colour (chest plate, helmet, pauldrons).
    primary: Handle<StandardMaterial>,
    /// Darker under-suit (torso base, limbs).
    suit: Handle<StandardMaterial>,
    /// Bright accent (cape, trim).
    accent: Handle<StandardMaterial>,
    /// Steel for hard surfaces (gauntlets, greaves).
    metal: Handle<StandardMaterial>,
    skin: Handle<StandardMaterial>,
    /// Emissive (eyes, visor, buckle).
    glow: Handle<StandardMaterial>,
}

impl Palette {
    /// Mœbius × Mana harmonization: hero hue identity is preserved (outfit,
    /// accent, skin come from config) but the relationships are curated —
    /// indigo shadows on the under-suit, cream-lifted trim, warm brass instead
    /// of grey steel, and flat luminous cloth throughout.
    fn build(materials: &mut Assets<StandardMaterial>, cfg: &CartoonCharacterConfig) -> Self {
        let outfit = cfg.outfit;
        let cream = Color::srgb(0.96, 0.92, 0.82);
        let brass = mix(Color::srgb(0.74, 0.62, 0.42), cfg.accent, 0.25);
        Palette {
            primary: cloth_mat(materials, mix(outfit, cream, 0.10)),
            suit: cloth_mat(materials, indigo_shade(outfit, 0.38)),
            accent: cloth_mat(materials, mix(cfg.accent, cream, 0.22)),
            metal: metal_mat(materials, brass),
            skin: cloth_mat(materials, cfg.skin),
            glow: emissive_mat(
                materials,
                cfg.eye_color,
                if cfg.emissive_eyes { 6.0 } else { 3.0 },
            ),
        }
    }
}

// ── Part registry (one per spawned hero, coloured from the palette) ─────────────

fn build_registry(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cfg: &CartoonCharacterConfig,
    d: &HeroDesign,
) -> PartRegistry {
    let pal = Palette::build(materials, cfg);
    let w = d.width;
    let l = d.limb;
    let mut reg = PartRegistry::default();

    let mut add = |id, mesh, material, sockets| {
        reg.insert(PartPrefab {
            id,
            mesh,
            material,
            sockets,
        })
    };

    // Torso (root): under-suit core with mount sockets for limbs + armor.
    add(
        "torso",
        organic(
            meshes,
            Vec3::new(0.145 * w, 0.175 * d.torso_height, 0.097 * d.torso_depth),
        ),
        pal.suit.clone(),
        vec![
            sock("neck", 0.0, 0.18 * d.torso_height, 0.0),
            sock_rot(
                "shoulder_l",
                -0.135 * w,
                0.13 * d.torso_height,
                0.0,
                Quat::from_rotation_z(-0.12),
            ),
            sock_rot(
                "shoulder_r",
                0.135 * w,
                0.13 * d.torso_height,
                0.0,
                Quat::from_rotation_z(0.12),
            ),
            sock("pauldron_l", -0.16 * w, 0.155 * d.torso_height, 0.0),
            sock("pauldron_r", 0.16 * w, 0.155 * d.torso_height, 0.0),
            sock("hip_l", -0.072 * w, -0.165 * d.torso_height, 0.0),
            sock("hip_r", 0.072 * w, -0.165 * d.torso_height, 0.0),
            sock("chest", 0.0, 0.05 * d.torso_height, -0.092 * d.torso_depth),
            sock("back", 0.0, 0.10 * d.torso_height, 0.088 * d.torso_depth),
            sock(
                "back_shard_c",
                0.0,
                0.13 * d.torso_height,
                0.112 * d.torso_depth,
            ),
            sock(
                "back_shard_l",
                -0.060 * w,
                0.11 * d.torso_height,
                0.106 * d.torso_depth,
            ),
            sock(
                "back_shard_r",
                0.060 * w,
                0.11 * d.torso_height,
                0.106 * d.torso_depth,
            ),
            sock("belt", 0.0, -0.15 * d.torso_height, -0.04),
            sock("hip_fin_l", -0.106 * w, -0.13 * d.torso_height, -0.02),
            sock("hip_fin_r", 0.106 * w, -0.13 * d.torso_height, -0.02),
        ],
    );

    // Chest plate (front armor).
    add(
        "chest_plate",
        plate(
            meshes,
            Vec3::new(0.125 * w * d.chest_width, 0.135 * d.chest_height, 0.03),
        ),
        pal.primary.clone(),
        vec![
            sock("mount", 0.0, 0.0, 0.028),
            sock("core", 0.0, 0.01, -0.032),
        ],
    );
    add(
        "chest_core",
        gem(
            meshes,
            Vec3::new(
                0.038 * d.chest_core_scale.max(0.2),
                0.046 * d.chest_core_scale.max(0.2),
                0.020,
            ),
        ),
        pal.glow.clone(),
        vec![sock("mount", 0.0, 0.0, 0.014)],
    );
    // Cape (hangs behind).
    add(
        "cape",
        plate(
            meshes,
            Vec3::new(0.155 * w * d.cape_width, 0.30 * d.cape_height, 0.016),
        ),
        pal.accent.clone(),
        vec![sock("mount", 0.0, 0.27 * d.cape_height, 0.0)],
    );
    // Belt buckle (emissive accent).
    add(
        "buckle",
        gem(meshes, Vec3::new(0.045, 0.035, 0.03)),
        pal.glow.clone(),
        vec![sock("mount", 0.0, 0.0, 0.03)],
    );

    // Head + helmet + face.
    add(
        "head",
        organic(
            meshes,
            Vec3::new(0.088 * d.head_width, 0.10 * d.head_height, 0.094),
        ),
        pal.skin.clone(),
        vec![
            sock("neck_joint", 0.0, -0.085 * d.head_height, 0.0),
            sock("crown", 0.0, 0.06 * d.head_height, 0.012),
            sock("crest", 0.0, 0.105 * d.head_height, 0.006),
            sock(
                "side_fin_l",
                -0.072 * d.head_width,
                0.030 * d.head_height,
                -0.002,
            ),
            sock(
                "side_fin_r",
                0.072 * d.head_width,
                0.030 * d.head_height,
                -0.002,
            ),
            sock("face", 0.0, 0.005, -0.094),
            sock("face_l", -0.036 * d.head_width, 0.012, -0.09),
            sock("face_r", 0.036 * d.head_width, 0.012, -0.09),
        ],
    );
    add(
        "helmet",
        plate(
            meshes,
            Vec3::new(0.10 * d.helmet_width, 0.072 * d.helmet_height, 0.10),
        ),
        pal.primary.clone(),
        vec![
            sock("rim", 0.0, -0.03 * d.helmet_height, 0.0),
            sock("brow", 0.0, 0.006 * d.helmet_height, -0.086),
            sock(
                "jaw_l",
                -0.060 * d.helmet_width,
                -0.055 * d.helmet_height,
                -0.062,
            ),
            sock(
                "jaw_r",
                0.060 * d.helmet_width,
                -0.055 * d.helmet_height,
                -0.062,
            ),
        ],
    );
    add(
        "helmet_brow",
        plate(
            meshes,
            Vec3::new(
                0.072 * d.visor_width * d.helmet_brow_scale.max(0.25),
                0.018 * d.helmet_brow_scale.max(0.25),
                0.018,
            ),
        ),
        pal.metal.clone(),
        vec![sock("mount", 0.0, 0.0, 0.014)],
    );
    add(
        "jaw_guard",
        plate(
            meshes,
            Vec3::new(
                0.018 * d.jaw_guard_scale.max(0.25),
                0.052 * d.jaw_guard_scale.max(0.25),
                0.026,
            ),
        ),
        pal.primary.clone(),
        vec![sock("mount", 0.0, 0.030 * d.jaw_guard_scale.max(0.25), 0.0)],
    );
    add(
        "visor",
        gem(meshes, Vec3::new(0.072 * d.visor_width, 0.022, 0.022)),
        pal.glow.clone(),
        vec![sock("mount", 0.0, 0.0, 0.015)],
    );
    add(
        "eye",
        gem(meshes, Vec3::new(0.017, 0.022, 0.013)),
        pal.glow.clone(),
        vec![sock("mount", 0.0, 0.0, 0.01)],
    );
    add(
        "head_crest",
        plate(
            meshes,
            Vec3::new(
                0.030 * d.crest_width.max(0.25),
                0.072 * d.crest_height.max(0.25),
                0.018,
            ),
        ),
        pal.accent.clone(),
        vec![sock("mount", 0.0, -0.056 * d.crest_height.max(0.25), 0.0)],
    );
    add(
        "head_side_fin",
        plate(
            meshes,
            Vec3::new(
                0.018 * d.side_fin_width.max(0.25),
                0.050 * d.side_fin_height.max(0.25),
                0.050 * d.side_fin_width.max(0.25),
            ),
        ),
        pal.primary.clone(),
        vec![sock("mount", 0.0, -0.018, 0.0)],
    );
    add(
        "back_shard",
        plate(
            meshes,
            Vec3::new(
                0.030 * w * d.back_shard_width.max(0.25),
                0.185 * d.back_shard_height.max(0.25),
                0.015,
            ),
        ),
        pal.accent.clone(),
        vec![sock(
            "mount",
            0.0,
            0.168 * d.back_shard_height.max(0.25),
            0.0,
        )],
    );
    add(
        "hip_fin",
        plate(
            meshes,
            Vec3::new(
                0.040 * d.hip_fin_scale.max(0.25),
                0.095 * d.hip_fin_scale.max(0.25),
                0.018,
            ),
        ),
        pal.primary.clone(),
        vec![sock("mount", 0.0, 0.040 * d.hip_fin_scale.max(0.25), 0.0)],
    );

    // Pauldron (shoulder dome).
    add(
        "pauldron",
        plate(
            meshes,
            Vec3::new(0.085 * w * d.shoulder_width, 0.06 * d.shoulder_height, 0.09),
        ),
        pal.primary.clone(),
        vec![
            sock("cap", 0.0, -0.03 * d.shoulder_height, 0.0),
            sock("fin", 0.0, 0.032 * d.shoulder_height, 0.020),
        ],
    );
    add(
        "shoulder_fin",
        plate(
            meshes,
            Vec3::new(
                0.020 * w * d.shoulder_fin_scale.max(0.25),
                0.082 * d.shoulder_fin_scale.max(0.25),
                0.038 * d.shoulder_fin_scale.max(0.25),
            ),
        ),
        pal.accent.clone(),
        vec![sock(
            "mount",
            0.0,
            -0.052 * d.shoulder_fin_scale.max(0.25),
            0.0,
        )],
    );

    // Arms.
    let upper_arm_y = 0.105 * d.upper_arm_length;
    let forearm_y = 0.095 * d.forearm_length;
    add(
        "arm_upper",
        organic(meshes, Vec3::new(0.04 * l, upper_arm_y, 0.042 * l)),
        pal.suit.clone(),
        vec![
            sock("shoulder", 0.0, upper_arm_y * 0.95, 0.0),
            sock("elbow", 0.0, -upper_arm_y * 0.95, 0.0),
        ],
    );
    add(
        "arm_lower",
        organic(meshes, Vec3::new(0.034 * l, forearm_y, 0.036 * l)),
        pal.suit.clone(),
        vec![
            sock("elbow", 0.0, forearm_y * 0.95, 0.0),
            sock("wrist", 0.0, -forearm_y * 0.95, 0.0),
            sock("elbow_guard", 0.0, forearm_y * 0.58, -0.040 * l),
            sock("blade", 0.042 * l, -forearm_y * 0.22, -0.010),
            sock("cannon", 0.010, -forearm_y * 0.16, -0.045 * l),
        ],
    );
    add(
        "hand",
        organic(
            meshes,
            Vec3::new(
                0.044 * l * d.hand_scale,
                0.055 * d.hand_scale,
                0.032 * l * d.hand_scale,
            ),
        ),
        if d.gauntlets {
            pal.metal.clone()
        } else {
            pal.skin.clone()
        },
        vec![sock("wrist", 0.0, 0.052 * d.hand_scale, 0.0)],
    );
    add(
        "wrist_blade",
        plate(
            meshes,
            Vec3::new(
                0.014,
                0.082 * d.wrist_blade_length.max(0.25),
                0.032 * d.wrist_blade_length.max(0.25),
            ),
        ),
        pal.accent.clone(),
        vec![sock(
            "mount",
            0.0,
            0.060 * d.wrist_blade_length.max(0.25),
            0.0,
        )],
    );
    add(
        "wrist_cannon",
        plate(
            meshes,
            Vec3::new(
                0.050 * d.wrist_cannon_scale.max(0.25),
                0.080 * d.wrist_cannon_scale.max(0.25),
                0.055 * d.wrist_cannon_scale.max(0.25),
            ),
        ),
        pal.metal.clone(),
        vec![sock(
            "mount",
            0.0,
            0.050 * d.wrist_cannon_scale.max(0.25),
            0.0,
        )],
    );
    add(
        "elbow_guard",
        plate(
            meshes,
            Vec3::new(
                0.048 * l * d.elbow_guard_scale.max(0.25),
                0.030 * d.elbow_guard_scale.max(0.25),
                0.022,
            ),
        ),
        pal.primary.clone(),
        vec![sock("mount", 0.0, 0.0, 0.016)],
    );

    // Legs.
    let thigh_y = 0.105 * d.thigh_length;
    let shin_y = 0.125 * d.shin_length;
    add(
        "thigh",
        organic(meshes, Vec3::new(0.06 * l, thigh_y, 0.062 * l)),
        pal.suit.clone(),
        vec![
            sock("hip", 0.0, thigh_y * 0.90, 0.0),
            sock("knee", 0.0, -thigh_y * 0.90, 0.0),
        ],
    );
    add(
        "shin",
        organic(meshes, Vec3::new(0.05 * l, shin_y, 0.052 * l)),
        if d.boots {
            pal.metal.clone()
        } else {
            pal.suit.clone()
        },
        vec![
            sock("knee", 0.0, shin_y * 0.96, 0.0),
            sock("ankle", 0.0, -shin_y * 0.96, 0.0),
            sock("knee_guard", 0.0, shin_y * 0.70, -0.050 * l),
            sock("shin_thruster", 0.0, -shin_y * 0.18, 0.054 * l),
        ],
    );
    add(
        "foot",
        plate(meshes, Vec3::new(0.052 * l, 0.04, 0.10 * d.foot_length)),
        if d.boots {
            pal.metal.clone()
        } else {
            pal.accent.clone()
        },
        vec![
            sock("ankle", 0.0, 0.04, 0.05 * d.foot_length),
            sock("toe", 0.0, -0.008, -0.088 * d.foot_length),
            sock("heel", 0.0, -0.004, 0.104 * d.foot_length),
        ],
    );
    add(
        "knee_guard",
        plate(
            meshes,
            Vec3::new(
                0.044 * l * d.knee_guard_scale.max(0.25),
                0.036 * d.knee_guard_scale.max(0.25),
                0.022,
            ),
        ),
        pal.primary.clone(),
        vec![sock("mount", 0.0, 0.0, 0.014)],
    );
    add(
        "toe_spike",
        plate(
            meshes,
            Vec3::new(
                0.034 * l * d.toe_spike_scale.max(0.25),
                0.018,
                0.065 * d.toe_spike_scale.max(0.25),
            ),
        ),
        pal.accent.clone(),
        vec![sock("mount", 0.0, 0.0, 0.048 * d.toe_spike_scale.max(0.25))],
    );
    add(
        "shin_thruster",
        plate(
            meshes,
            Vec3::new(
                0.038 * l * d.shin_thruster_scale.max(0.25),
                0.062 * d.shin_thruster_scale.max(0.25),
                0.030,
            ),
        ),
        pal.glow.clone(),
        vec![sock(
            "mount",
            0.0,
            0.034 * d.shin_thruster_scale.max(0.25),
            0.0,
        )],
    );
    add(
        "heel_spur",
        plate(
            meshes,
            Vec3::new(
                0.036 * l * d.heel_spur_scale.max(0.25),
                0.018,
                0.055 * d.heel_spur_scale.max(0.25),
            ),
        ),
        pal.primary.clone(),
        vec![sock(
            "mount",
            0.0,
            0.0,
            -0.042 * d.heel_spur_scale.max(0.25),
        )],
    );

    reg
}

// ── Recipe (component graph; parents listed before children) ─────────────────────

fn build_recipe(d: &HeroDesign) -> CharacterRecipe {
    let a = |parent_slot, parent_socket, child_slot, child_part, child_socket| Attachment {
        parent_slot,
        parent_socket,
        child_slot,
        child_part,
        child_socket,
    };
    let mut at = vec![
        a("torso", "neck", "head", "head", "neck_joint"),
        a("head", "crown", "helmet", "helmet", "rim"),
        a("torso", "chest", "chest_plate", "chest_plate", "mount"),
        a("chest_plate", "core", "chest_core", "chest_core", "mount"),
        // Arms.
        a(
            "torso",
            "shoulder_l",
            "arm_upper_l",
            "arm_upper",
            "shoulder",
        ),
        a(
            "torso",
            "shoulder_r",
            "arm_upper_r",
            "arm_upper",
            "shoulder",
        ),
        a("arm_upper_l", "elbow", "arm_lower_l", "arm_lower", "elbow"),
        a("arm_upper_r", "elbow", "arm_lower_r", "arm_lower", "elbow"),
        a("arm_lower_l", "wrist", "hand_l", "hand", "wrist"),
        a("arm_lower_r", "wrist", "hand_r", "hand", "wrist"),
        // Legs.
        a("torso", "hip_l", "thigh_l", "thigh", "hip"),
        a("torso", "hip_r", "thigh_r", "thigh", "hip"),
        a("thigh_l", "knee", "shin_l", "shin", "knee"),
        a("thigh_r", "knee", "shin_r", "shin", "knee"),
        a("shin_l", "ankle", "foot_l", "foot", "ankle"),
        a("shin_r", "ankle", "foot_r", "foot", "ankle"),
    ];

    // Face: visor band or twin eyes.
    if d.visor {
        at.push(a("head", "face", "visor", "visor", "mount"));
    } else {
        at.push(a("head", "face_l", "eye_l", "eye", "mount"));
        at.push(a("head", "face_r", "eye_r", "eye", "mount"));
    }
    if d.crest_height > 0.0 {
        at.push(a("head", "crest", "head_crest", "head_crest", "mount"));
    }
    if d.side_fin_width > 0.0 {
        at.push(a(
            "head",
            "side_fin_l",
            "head_side_fin_l",
            "head_side_fin",
            "mount",
        ));
        at.push(a(
            "head",
            "side_fin_r",
            "head_side_fin_r",
            "head_side_fin",
            "mount",
        ));
    }
    if d.helmet_brow_scale > 0.0 {
        at.push(a("helmet", "brow", "helmet_brow", "helmet_brow", "mount"));
    }
    if d.jaw_guard_scale > 0.0 {
        at.push(a("helmet", "jaw_l", "jaw_guard_l", "jaw_guard", "mount"));
        at.push(a("helmet", "jaw_r", "jaw_guard_r", "jaw_guard", "mount"));
    }
    if d.pauldrons {
        at.push(a("torso", "pauldron_l", "pauldron_l", "pauldron", "cap"));
        at.push(a("torso", "pauldron_r", "pauldron_r", "pauldron", "cap"));
    }
    if d.pauldrons && d.shoulder_fin_scale > 0.0 {
        at.push(a(
            "pauldron_l",
            "fin",
            "shoulder_fin_l",
            "shoulder_fin",
            "mount",
        ));
        at.push(a(
            "pauldron_r",
            "fin",
            "shoulder_fin_r",
            "shoulder_fin",
            "mount",
        ));
    }
    if d.cape {
        at.push(a("torso", "back", "cape", "cape", "mount"));
    }
    if d.back_shard_height > 0.0 {
        at.push(a(
            "torso",
            "back_shard_c",
            "back_shard_c",
            "back_shard",
            "mount",
        ));
    }
    if d.back_shard_width > 0.85 {
        at.push(a(
            "torso",
            "back_shard_l",
            "back_shard_l",
            "back_shard",
            "mount",
        ));
        at.push(a(
            "torso",
            "back_shard_r",
            "back_shard_r",
            "back_shard",
            "mount",
        ));
    }
    if d.wrist_blade_length > 0.0 {
        at.push(a(
            "arm_lower_l",
            "blade",
            "wrist_blade_l",
            "wrist_blade",
            "mount",
        ));
        at.push(a(
            "arm_lower_r",
            "blade",
            "wrist_blade_r",
            "wrist_blade",
            "mount",
        ));
    }
    if d.wrist_cannon_scale > 0.0 {
        at.push(a(
            "arm_lower_r",
            "cannon",
            "wrist_cannon_r",
            "wrist_cannon",
            "mount",
        ));
    }
    if d.elbow_guard_scale > 0.0 {
        at.push(a(
            "arm_lower_l",
            "elbow_guard",
            "elbow_guard_l",
            "elbow_guard",
            "mount",
        ));
        at.push(a(
            "arm_lower_r",
            "elbow_guard",
            "elbow_guard_r",
            "elbow_guard",
            "mount",
        ));
    }
    if d.hip_fin_scale > 0.0 {
        at.push(a("torso", "hip_fin_l", "hip_fin_l", "hip_fin", "mount"));
        at.push(a("torso", "hip_fin_r", "hip_fin_r", "hip_fin", "mount"));
    }
    if d.knee_guard_scale > 0.0 {
        at.push(a(
            "shin_l",
            "knee_guard",
            "knee_guard_l",
            "knee_guard",
            "mount",
        ));
        at.push(a(
            "shin_r",
            "knee_guard",
            "knee_guard_r",
            "knee_guard",
            "mount",
        ));
    }
    if d.toe_spike_scale > 0.0 {
        at.push(a("foot_l", "toe", "toe_spike_l", "toe_spike", "mount"));
        at.push(a("foot_r", "toe", "toe_spike_r", "toe_spike", "mount"));
    }
    if d.shin_thruster_scale > 0.0 {
        at.push(a(
            "shin_l",
            "shin_thruster",
            "shin_thruster_l",
            "shin_thruster",
            "mount",
        ));
        at.push(a(
            "shin_r",
            "shin_thruster",
            "shin_thruster_r",
            "shin_thruster",
            "mount",
        ));
    }
    if d.heel_spur_scale > 0.0 {
        at.push(a("foot_l", "heel", "heel_spur_l", "heel_spur", "mount"));
        at.push(a("foot_r", "heel", "heel_spur_r", "heel_spur", "mount"));
    }
    if d.belt {
        at.push(a("torso", "belt", "buckle", "buckle", "mount"));
    }

    CharacterRecipe {
        root_slot: "torso",
        root_part: "torso",
        attachments: at,
    }
}

// ── Public attach ────────────────────────────────────────────────────────────

/// Attach a fully math-modeled, component-based hero as the visual for `root`
/// (the player entity), sized to fill its physics capsule. Call
/// [`crate::character::presets::attach_player_gameplay_rig`] alongside this to keep the
/// skeleton / weapon-attach points alive.
pub fn attach_modular_player_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    config: &CartoonCharacterConfig,
    loadout: CharacterLoadout,
) {
    let design = HeroDesign::from_config(config, loadout);
    let registry = build_registry(meshes, materials, config, &design);
    let recipe = build_recipe(&design);

    // Mirror the capsule extents from `authored_player_defaults` so the figure
    // exactly fills the collider: feet at the capsule bottom, head at the top.
    let s = config.scale;
    let body = config.body.validated();
    let half_height =
        (0.6 * (body.height * 0.72 + body.leg_length * 0.28) * s).clamp(0.44 * s, 0.86 * s);
    let radius =
        (0.35 * (body.shoulder_width * 0.55 + body.chest_size * 0.25 + body.hip_width * 0.20) * s)
            .clamp(0.26 * s, 0.50 * s);

    let capsule_total = 2.0 * (half_height + radius);
    let torso_y = -(half_height + radius) + TORSO_CENTER_Y * capsule_total;
    let local = Transform::from_xyz(0.0, torso_y, 0.0).with_scale(Vec3::splat(capsule_total));

    if let Err(err) = spawn_character_under(commands, &registry, &recipe, root, local) {
        error!("modular player mesh assembly failed: {err}");
    }
}

/// Spawn a standalone modular player mesh for editor previews.
pub fn spawn_modular_player_preview(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    config: CartoonCharacterConfig,
    position: Vec3,
    loadout: CharacterLoadout,
) -> Entity {
    let root = commands
        .spawn((
            Transform::from_translation(position),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            loadout,
        ))
        .id();
    attach_modular_player_mesh(commands, meshes, materials, root, &config, loadout);
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::presets::hero_config;

    #[test]
    fn modular_design_responds_to_part_loadout() {
        let cfg = hero_config("Vincenzo");
        let chroma = HeroDesign::from_config(&cfg, CharacterLoadout::chroma_reference());
        let heavy = HeroDesign::from_config(
            &cfg,
            CharacterLoadout::new(
                BodyPreset::HeavyPlate,
                ArmPreset::HeavyArms,
                LegPreset::HeavyLegs,
                ShoulderPreset::PlateEpaulettes,
                HeadPreset::FullHelm,
            ),
        );
        let rift = HeroDesign::from_config(
            &cfg,
            CharacterLoadout::new(
                BodyPreset::RiftMantle,
                ArmPreset::RiftTalons,
                LegPreset::RiftBoots,
                ShoulderPreset::RiftCloak,
                HeadPreset::RiftCowl,
            ),
        );

        assert!(heavy.width > chroma.width);
        assert!(heavy.limb > chroma.limb);
        assert!(heavy.elbow_guard_scale > 0.0);
        assert!(heavy.shin_thruster_scale > 0.0);
        assert!(rift.cape);
        assert!(rift.foot_length > chroma.foot_length);
        assert!(rift.hand_scale > chroma.hand_scale);
        assert!(rift.heel_spur_scale > 0.0);
    }

    #[test]
    fn vincenzo_uses_own_deep_glb_profile_not_chroma_clone() {
        let vincenzo_cfg = hero_config("Vincenzo");
        let chroma_cfg = hero_config("Nova");
        let vincenzo = HeroDesign::from_config(&vincenzo_cfg, CharacterLoadout::chroma_reference());
        let chroma = HeroDesign::from_config(&chroma_cfg, CharacterLoadout::chroma_reference());

        assert_eq!(vincenzo.profile, GlbReferenceProfile::Vincenzo);
        assert_eq!(chroma.profile, GlbReferenceProfile::Chroma);
        assert!(vincenzo.torso_depth > chroma.torso_depth);
        assert!(vincenzo.back_shard_height > chroma.back_shard_height);
        assert!(vincenzo.toe_spike_scale > chroma.toe_spike_scale);
    }

    #[test]
    fn reference_recipes_include_glb_signature_parts() {
        let antonio = HeroDesign::from_config(
            &hero_config("Antonio"),
            CharacterLoadout::new(
                BodyPreset::RiftMantle,
                ArmPreset::RiftTalons,
                LegPreset::RiftBoots,
                ShoulderPreset::RiftCloak,
                HeadPreset::RiftCowl,
            ),
        );
        let mut daria_cfg = hero_config("Daria");
        daria_cfg.has_shoulder_pads = true;
        let daria = HeroDesign::from_config(
            &daria_cfg,
            CharacterLoadout::new(
                BodyPreset::DariaCore,
                ArmPreset::DariaCannon,
                LegPreset::DariaGreaves,
                ShoulderPreset::DariaFlares,
                HeadPreset::DariaHelm,
            ),
        );
        let amp = HeroDesign::from_config(
            &hero_config("Joseph"),
            CharacterLoadout::new(
                BodyPreset::HeavyPlate,
                ArmPreset::HeavyArms,
                LegPreset::HeavyLegs,
                ShoulderPreset::PlateEpaulettes,
                HeadPreset::FullHelm,
            ),
        );

        let antonio_recipe = build_recipe(&antonio);
        assert!(has_part(&antonio_recipe, "back_shard"));
        assert!(has_part(&antonio_recipe, "wrist_blade"));
        assert!(has_part(&antonio_recipe, "toe_spike"));
        assert!(has_part(&antonio_recipe, "hip_fin"));

        let daria_recipe = build_recipe(&daria);
        assert!(has_part(&daria_recipe, "head_side_fin"));
        assert!(has_part(&daria_recipe, "helmet_brow"));
        assert!(has_part(&daria_recipe, "jaw_guard"));
        assert!(has_part(&daria_recipe, "wrist_cannon"));
        assert!(has_part(&daria_recipe, "knee_guard"));
        assert!(has_part(&daria_recipe, "shoulder_fin"));
        assert!(has_part(&daria_recipe, "elbow_guard"));
        assert!(has_part(&daria_recipe, "shin_thruster"));
        assert!(has_part(&daria_recipe, "heel_spur"));

        let amp_recipe = build_recipe(&amp);
        assert!(has_part(&amp_recipe, "wrist_cannon"));
        assert!(has_part(&amp_recipe, "knee_guard"));
        assert!(has_part(&amp_recipe, "elbow_guard"));
        assert!(!has_part(&amp_recipe, "back_shard"));
    }

    fn has_part(recipe: &CharacterRecipe, part: &'static str) -> bool {
        recipe
            .attachments
            .iter()
            .any(|attachment| attachment.child_part == part)
    }
}
