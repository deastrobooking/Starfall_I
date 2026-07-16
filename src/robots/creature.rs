//! Versioned, deterministic recipes for generated robots and monsters.
//!
//! `CreatureSpec` wraps the existing `RobotStyle` instead of replacing it, so
//! every working drone and legacy preset remains available to the new forge.

#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use super::designer::{HeadShape, LegStyle, RobotArchetype, RobotStyle};

pub const CREATURE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CreatureKind {
    #[default]
    Robot,
    Monster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CreatureTopology {
    #[default]
    Humanoid,
    Quadruped,
    Flyer,
    Crawler,
    Serpent,
    Drone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CreatureRole {
    Civilian,
    Ally,
    #[default]
    Scout,
    Bruiser,
    Artillery,
    Boss,
    Pet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CreatureFaction {
    #[default]
    Starfall,
    Raider,
    Swarm,
    Caveborn,
    Ancient,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CreatureSurface {
    #[default]
    PaintedMetal,
    BareAlloy,
    Crystal,
    Organic,
    Energy,
}

impl CreatureSurface {
    /// PBR values consumed by the existing procedural robot factory.
    pub fn material_response(self) -> (f32, f32) {
        match self {
            Self::PaintedMetal => (0.55, 0.45),
            Self::BareAlloy => (0.88, 0.24),
            Self::Crystal => (0.18, 0.16),
            Self::Organic => (0.04, 0.82),
            Self::Energy => (0.12, 0.08),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatureSpec {
    pub schema_version: u32,
    pub content_id: String,
    pub display_name: String,
    pub seed: u64,
    pub kind: CreatureKind,
    pub topology: CreatureTopology,
    pub role: CreatureRole,
    pub faction: CreatureFaction,
    pub surface: CreatureSurface,
    /// Exact legacy morphology compiled by the existing robot factory.
    pub style: RobotStyle,
}

impl Default for CreatureSpec {
    fn default() -> Self {
        Self {
            schema_version: CREATURE_SCHEMA_VERSION,
            content_id: "starfall.scout.generated".into(),
            display_name: "Generated Scout".into(),
            seed: 0,
            kind: CreatureKind::Robot,
            topology: CreatureTopology::Humanoid,
            role: CreatureRole::Scout,
            faction: CreatureFaction::Starfall,
            surface: CreatureSurface::PaintedMetal,
            style: RobotStyle::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub severity: ValidationSeverity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct CreatureValidation {
    pub issues: Vec<ValidationIssue>,
    pub estimated_parts: u32,
}

impl CreatureValidation {
    pub fn is_publishable(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error)
    }
}

impl CreatureSpec {
    /// Bring any existing named drone/boss style into the versioned forge
    /// without changing its proven geometry.
    pub fn from_legacy(
        content_id: impl Into<String>,
        display_name: impl Into<String>,
        style: RobotStyle,
    ) -> Self {
        let topology = match style.archetype {
            RobotArchetype::Flyer => CreatureTopology::Flyer,
            RobotArchetype::Insectoid => CreatureTopology::Crawler,
            RobotArchetype::Pet => CreatureTopology::Quadruped,
            _ if style.leg_style == LegStyle::Hoverpads => CreatureTopology::Drone,
            _ => CreatureTopology::Humanoid,
        };
        let role = match style.archetype {
            RobotArchetype::Ally => CreatureRole::Ally,
            RobotArchetype::Brute => CreatureRole::Bruiser,
            RobotArchetype::Tank => CreatureRole::Artillery,
            RobotArchetype::Hybrid => CreatureRole::Boss,
            RobotArchetype::Pet => CreatureRole::Pet,
            _ => CreatureRole::Scout,
        };
        Self {
            content_id: content_id.into(),
            display_name: display_name.into(),
            topology,
            role,
            style,
            ..Default::default()
        }
    }

    /// Validate authoring data without mutating it. The factory calls
    /// `compiled_style` afterward to clamp safe numerical limits.
    pub fn validate(&self) -> CreatureValidation {
        let mut result = CreatureValidation {
            estimated_parts: estimate_parts(&self.style),
            ..Default::default()
        };
        let mut issue = |severity, code, message: &str| {
            result.issues.push(ValidationIssue {
                severity,
                code,
                message: message.into(),
            });
        };

        if self.schema_version != CREATURE_SCHEMA_VERSION {
            issue(
                ValidationSeverity::Error,
                "schema.unsupported",
                "Creature recipe needs migration before publishing",
            );
        }
        if self.content_id.trim().is_empty()
            || !self.content_id.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')
            })
        {
            issue(
                ValidationSeverity::Error,
                "id.invalid",
                "Content ID must use lowercase letters, numbers, dots, dashes, or underscores",
            );
        }
        if self.display_name.trim().is_empty() {
            issue(
                ValidationSeverity::Error,
                "name.empty",
                "Display name cannot be empty",
            );
        }
        if !(0.3..=3.0).contains(&self.style.scale) {
            issue(
                ValidationSeverity::Error,
                "bounds.scale",
                "Scale must remain inside the supported 0.3 to 3.0 range",
            );
        }
        let dimensions_valid = (10.0..=50.0).contains(&self.style.torso_width)
            && (20.0..=70.0).contains(&self.style.torso_height)
            && (8.0..=35.0).contains(&self.style.torso_depth)
            && (5.0..=25.0).contains(&self.style.head_size)
            && (15.0..=55.0).contains(&self.style.arm_length)
            && (20.0..=55.0).contains(&self.style.leg_length);
        if !dimensions_valid {
            issue(
                ValidationSeverity::Error,
                "bounds.morphology",
                "Core body dimensions exceed the supported factory envelope",
            );
        }
        if !(2..=8).contains(&self.style.tail_segments) {
            issue(
                ValidationSeverity::Error,
                "bounds.tail_segments",
                "Tail segment count must remain between 2 and 8",
            );
        }
        if self.topology == CreatureTopology::Flyer && !self.style.has_wings {
            issue(
                ValidationSeverity::Warning,
                "topology.flyer_without_wings",
                "Flyer compiler will add wings",
            );
        }
        if self.topology == CreatureTopology::Drone && self.style.leg_style != LegStyle::Hoverpads {
            issue(
                ValidationSeverity::Warning,
                "topology.drone_with_legs",
                "Drone compiler will replace legs with hover pads",
            );
        }
        if result.estimated_parts > 70 {
            issue(
                ValidationSeverity::Warning,
                "budget.parts",
                "Generated part count exceeds the four-player preview target",
            );
        }
        result
    }

    /// Compile semantic topology into the existing, proven robot geometry
    /// controls. This is deterministic and never mutates the saved recipe.
    pub fn compiled_style(&self) -> RobotStyle {
        let mut style = self.style.clone();
        match self.topology {
            CreatureTopology::Humanoid => {}
            CreatureTopology::Quadruped => {
                style.archetype = RobotArchetype::Pet;
                style.leg_style = LegStyle::Digitigrade;
                style.has_tail = true;
                style.torso_height *= 0.72;
                style.torso_depth *= 1.35;
            }
            CreatureTopology::Flyer => {
                style.archetype = RobotArchetype::Flyer;
                style.has_wings = true;
                style.has_backpack = true;
            }
            CreatureTopology::Crawler => {
                style.archetype = RobotArchetype::Insectoid;
                style.leg_style = LegStyle::Digitigrade;
                style.has_antennae = true;
                style.torso_height *= 0.78;
            }
            CreatureTopology::Serpent => {
                style.archetype = RobotArchetype::Hybrid;
                style.leg_style = LegStyle::Hoverpads;
                style.has_tail = true;
                style.tail_segments = style.tail_segments.max(7);
                style.tail_length = style.tail_length.max(38.0);
            }
            CreatureTopology::Drone => {
                style.archetype = RobotArchetype::Scout;
                style.leg_style = LegStyle::Hoverpads;
                style.has_backpack = true;
            }
        }
        style.validate();
        style
    }
}

fn estimate_parts(style: &RobotStyle) -> u32 {
    let mut count = 20 + style.tail_segments.min(8);
    count += u32::from(style.has_wings) * 4;
    count += u32::from(style.has_cannons) * 2;
    count += u32::from(style.has_horns) * 2;
    count += u32::from(style.has_antennae) * 4;
    count += u32::from(style.has_backpack);
    count += u32::from(style.has_shield);
    count + style.extra_plating.min(3)
}

/// Create a repeatable variant. The same seed and semantic inputs produce the
/// same complete recipe on every run.
pub fn generate_seeded(
    seed: u64,
    kind: CreatureKind,
    topology: CreatureTopology,
    role: CreatureRole,
    faction: CreatureFaction,
) -> CreatureSpec {
    let mut rng = StdRng::seed_from_u64(seed);
    let archetype = match role {
        CreatureRole::Ally | CreatureRole::Civilian => RobotArchetype::Ally,
        CreatureRole::Bruiser => RobotArchetype::Brute,
        CreatureRole::Artillery => RobotArchetype::Tank,
        CreatureRole::Boss => RobotArchetype::Hybrid,
        CreatureRole::Pet => RobotArchetype::Pet,
        CreatureRole::Scout => RobotArchetype::Scout,
    };
    let mut style = RobotStyle::for_archetype(archetype);
    style.scale = (style.scale * rng.gen_range(0.88..=1.14)).clamp(0.3, 3.0);
    style.torso_width *= rng.gen_range(0.86..=1.16);
    style.torso_height *= rng.gen_range(0.90..=1.12);
    style.torso_depth *= rng.gen_range(0.88..=1.14);
    style.head_size *= rng.gen_range(0.86..=1.18);
    style.arm_length *= rng.gen_range(0.90..=1.12);
    style.leg_length *= rng.gen_range(0.90..=1.12);
    style.head_shape =
        [HeadShape::Box, HeadShape::Sphere, HeadShape::Cylinder][rng.gen_range(0..3)];
    style.has_cannons |= matches!(role, CreatureRole::Artillery | CreatureRole::Boss);
    style.has_shield |= matches!(role, CreatureRole::Ally | CreatureRole::Boss);
    apply_faction_palette(&mut style, faction);

    let surface = match kind {
        CreatureKind::Robot => CreatureSurface::PaintedMetal,
        CreatureKind::Monster => match faction {
            CreatureFaction::Ancient => CreatureSurface::Crystal,
            CreatureFaction::Caveborn => CreatureSurface::Organic,
            _ => CreatureSurface::BareAlloy,
        },
    };
    let noun = match topology {
        CreatureTopology::Humanoid => "Unit",
        CreatureTopology::Quadruped => "Beast",
        CreatureTopology::Flyer => "Wing",
        CreatureTopology::Crawler => "Crawler",
        CreatureTopology::Serpent => "Serpent",
        CreatureTopology::Drone => "Drone",
    };
    CreatureSpec {
        schema_version: CREATURE_SCHEMA_VERSION,
        content_id: format!("generated.{faction:?}.{noun}.{seed:016x}").to_ascii_lowercase(),
        display_name: format!("{faction:?} {noun} {:04X}", seed as u16),
        seed,
        kind,
        topology,
        role,
        faction,
        surface,
        style,
    }
}

fn apply_faction_palette(style: &mut RobotStyle, faction: CreatureFaction) {
    use bevy::prelude::Color;
    let (primary, secondary, emissive) = match faction {
        CreatureFaction::Starfall => ((0.12, 0.48, 0.82), (0.82, 0.88, 0.94), (0.10, 0.92, 1.0)),
        CreatureFaction::Raider => ((0.62, 0.08, 0.10), (0.10, 0.08, 0.12), (1.0, 0.18, 0.04)),
        CreatureFaction::Swarm => ((0.20, 0.04, 0.30), (0.06, 0.02, 0.10), (0.90, 0.0, 0.72)),
        CreatureFaction::Caveborn => ((0.24, 0.34, 0.12), (0.12, 0.16, 0.07), (0.52, 0.92, 0.08)),
        CreatureFaction::Ancient => ((0.64, 0.52, 0.20), (0.16, 0.25, 0.30), (0.18, 0.92, 0.88)),
        CreatureFaction::Neutral => ((0.42, 0.44, 0.46), (0.16, 0.17, 0.19), (0.72, 0.78, 0.82)),
    };
    style.primary = Color::srgb(primary.0, primary.1, primary.2);
    style.secondary = Color::srgb(secondary.0, secondary.1, secondary.2);
    style.emissive = Color::srgb(emissive.0, emissive.1, emissive.2);
}

/// Curated forge starting points. Each resolves through the same seeded
/// generator as random variants and remains fully editable afterward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreaturePreset {
    StarGuardian,
    RaiderGunner,
    RetroMecha,
    CrystalGolem,
    SkyManta,
    CaveCrawler,
}

impl CreaturePreset {
    pub const ALL: [Self; 6] = [
        Self::StarGuardian,
        Self::RaiderGunner,
        Self::RetroMecha,
        Self::CrystalGolem,
        Self::SkyManta,
        Self::CaveCrawler,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::StarGuardian => "Star Guardian",
            Self::RaiderGunner => "Raider Gunner",
            Self::RetroMecha => "Retro Mecha",
            Self::CrystalGolem => "Crystal Golem",
            Self::SkyManta => "Sky Manta",
            Self::CaveCrawler => "Cave Crawler",
        }
    }

    pub fn generate(self) -> CreatureSpec {
        let (seed, kind, topology, role, faction) = match self {
            Self::StarGuardian => (
                0x51A2,
                CreatureKind::Robot,
                CreatureTopology::Humanoid,
                CreatureRole::Ally,
                CreatureFaction::Starfall,
            ),
            Self::RaiderGunner => (
                0xBAD1,
                CreatureKind::Robot,
                CreatureTopology::Humanoid,
                CreatureRole::Artillery,
                CreatureFaction::Raider,
            ),
            Self::RetroMecha => (
                0x1978,
                CreatureKind::Robot,
                CreatureTopology::Humanoid,
                CreatureRole::Bruiser,
                CreatureFaction::Ancient,
            ),
            Self::CrystalGolem => (
                0xC2757A1,
                CreatureKind::Monster,
                CreatureTopology::Humanoid,
                CreatureRole::Bruiser,
                CreatureFaction::Ancient,
            ),
            Self::SkyManta => (
                0x5A7A,
                CreatureKind::Monster,
                CreatureTopology::Flyer,
                CreatureRole::Scout,
                CreatureFaction::Neutral,
            ),
            Self::CaveCrawler => (
                0xCA8E,
                CreatureKind::Monster,
                CreatureTopology::Crawler,
                CreatureRole::Bruiser,
                CreatureFaction::Caveborn,
            ),
        };
        let mut spec = generate_seeded(seed, kind, topology, role, faction);
        spec.content_id = format!(
            "starfall.preset.{}",
            self.label().to_ascii_lowercase().replace(' ', "_")
        );
        spec.display_name = self.label().into();
        if self == Self::RetroMecha {
            spec.style.has_cannons = true;
            spec.style.has_backpack = true;
            spec.style.shoulder_pad_size = 12.0;
        }
        if self == Self::SkyManta {
            spec.style.wing_span = 58.0;
            spec.style.torso_height *= 0.68;
            spec.style.torso_width *= 1.45;
        }
        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_generation_is_byte_for_byte_repeatable() {
        let a = generate_seeded(
            42,
            CreatureKind::Monster,
            CreatureTopology::Crawler,
            CreatureRole::Bruiser,
            CreatureFaction::Caveborn,
        );
        let b = generate_seeded(
            42,
            CreatureKind::Monster,
            CreatureTopology::Crawler,
            CreatureRole::Bruiser,
            CreatureFaction::Caveborn,
        );
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn recipe_round_trip_preserves_semantics_and_style() {
        let spec = generate_seeded(
            7,
            CreatureKind::Robot,
            CreatureTopology::Flyer,
            CreatureRole::Ally,
            CreatureFaction::Starfall,
        );
        let json = serde_json::to_string_pretty(&spec).unwrap();
        let loaded: CreatureSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.seed, 7);
        assert_eq!(loaded.topology, CreatureTopology::Flyer);
        assert_eq!(loaded.style.torso_width, spec.style.torso_width);
    }

    #[test]
    fn topology_compiler_adds_required_geometry_without_mutating_recipe() {
        let spec = CreatureSpec {
            topology: CreatureTopology::Flyer,
            ..Default::default()
        };
        assert!(!spec.style.has_wings);
        assert!(spec.compiled_style().has_wings);
        assert!(spec.validate().is_publishable());
    }

    #[test]
    fn invalid_content_ids_block_publish() {
        let spec = CreatureSpec {
            content_id: "Bad ID With Spaces".into(),
            ..Default::default()
        };
        assert!(!spec.validate().is_publishable());
    }

    #[test]
    fn curated_library_contains_robot_and_monster_recipes() {
        let recipes: Vec<_> = CreaturePreset::ALL
            .into_iter()
            .map(CreaturePreset::generate)
            .collect();
        assert!(recipes.iter().any(|spec| spec.kind == CreatureKind::Robot));
        assert!(recipes
            .iter()
            .any(|spec| spec.kind == CreatureKind::Monster));
        assert!(recipes.iter().all(|spec| spec.validate().is_publishable()));
    }

    #[test]
    fn legacy_robot_styles_enter_the_new_recipe_without_geometry_loss() {
        let style = crate::robots::presets::jet_warden();
        let spec =
            CreatureSpec::from_legacy("starfall.legacy.jet_warden", "Jet Warden", style.clone());
        assert_eq!(spec.topology, CreatureTopology::Flyer);
        assert_eq!(spec.style.wing_span, style.wing_span);
        assert_eq!(spec.style.cannon_size, style.cannon_size);
    }
}
