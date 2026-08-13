//! Versioned, deterministic spacecraft recipes and their procedural compiler.
//!
//! The recipe is deliberately independent from Starfall's legacy vehicle and
//! robot-assembly enums.  A Forge design can therefore describe everything
//! from a one-seat fighter to an immobile orbital base without changing the
//! campaign-save representation of the shipped ATV/fighter implementation.
//! Editor previews and future runtime consumers both call [`spawn_spacecraft`]
//! so authored dimensions cannot drift from the generated silhouette.

#![allow(dead_code)] // Runtime placement adapters consume this catalog in later slices.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::engine::rendering::{PbrBundle, SpatialBundle};

pub const SPACECRAFT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpacecraftClass {
    #[default]
    Fighter,
    Bomber,
    Destroyer,
    CommandShip,
    Base,
}

impl SpacecraftClass {
    pub const ALL: [Self; 5] = [
        Self::Fighter,
        Self::Bomber,
        Self::Destroyer,
        Self::CommandShip,
        Self::Base,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Fighter => "Fighter",
            Self::Bomber => "Bomber",
            Self::Destroyer => "Destroyer",
            Self::CommandShip => "Command Ship",
            Self::Base => "Space Base",
        }
    }

    const fn reactor_output(self) -> f32 {
        match self {
            Self::Fighter => 320.0,
            Self::Bomber => 760.0,
            Self::Destroyer => 4_600.0,
            Self::CommandShip => 12_500.0,
            Self::Base => 22_000.0,
        }
    }

    const fn thrust_factor(self) -> f32 {
        match self {
            Self::Fighter => 1_300.0,
            Self::Bomber => 980.0,
            Self::Destroyer => 1_100.0,
            Self::CommandShip => 940.0,
            Self::Base => 0.0,
        }
    }

    const fn command_factor(self) -> u32 {
        match self {
            Self::Fighter => 2,
            Self::Bomber => 5,
            Self::Destroyer => 18,
            Self::CommandShip => 60,
            Self::Base => 90,
        }
    }

    const fn hangar_factor(self) -> u32 {
        match self {
            Self::Fighter => 1,
            Self::Bomber => 2,
            Self::Destroyer => 8,
            Self::CommandShip => 18,
            Self::Base => 28,
        }
    }
}

/// Serializable sRGB palette. Arrays keep the authored document independent
/// from Bevy's evolving color representation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpacecraftPalette {
    pub primary: [f32; 4],
    pub secondary: [f32; 4],
    pub accent: [f32; 4],
    pub emissive: [f32; 4],
}

impl Default for SpacecraftPalette {
    fn default() -> Self {
        Self {
            primary: [0.20, 0.28, 0.40, 1.0],
            secondary: [0.06, 0.08, 0.14, 1.0],
            accent: [0.82, 0.20, 0.36, 1.0],
            emissive: [0.12, 0.86, 1.0, 1.0],
        }
    }
}

impl SpacecraftPalette {
    pub const PRESETS: [Self; 5] = [
        Self {
            primary: [0.20, 0.28, 0.40, 1.0],
            secondary: [0.06, 0.08, 0.14, 1.0],
            accent: [0.82, 0.20, 0.36, 1.0],
            emissive: [0.12, 0.86, 1.0, 1.0],
        },
        Self {
            primary: [0.62, 0.66, 0.72, 1.0],
            secondary: [0.12, 0.14, 0.20, 1.0],
            accent: [0.95, 0.56, 0.10, 1.0],
            emissive: [1.0, 0.32, 0.08, 1.0],
        },
        Self {
            primary: [0.14, 0.34, 0.28, 1.0],
            secondary: [0.04, 0.10, 0.10, 1.0],
            accent: [0.42, 0.92, 0.56, 1.0],
            emissive: [0.20, 1.0, 0.62, 1.0],
        },
        Self {
            primary: [0.30, 0.18, 0.46, 1.0],
            secondary: [0.08, 0.04, 0.16, 1.0],
            accent: [0.74, 0.48, 1.0, 1.0],
            emissive: [0.78, 0.32, 1.0, 1.0],
        },
        Self {
            primary: [0.68, 0.58, 0.22, 1.0],
            secondary: [0.12, 0.10, 0.08, 1.0],
            accent: [1.0, 0.84, 0.30, 1.0],
            emissive: [1.0, 0.66, 0.10, 1.0],
        },
    ];

    pub fn next(self) -> Self {
        let current = Self::PRESETS
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        Self::PRESETS[(current + 1) % Self::PRESETS.len()]
    }

    fn is_valid(self) -> bool {
        [self.primary, self.secondary, self.accent, self.emissive]
            .into_iter()
            .flatten()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }

    fn sanitized(self) -> Self {
        fn color(value: [f32; 4], fallback: [f32; 4]) -> [f32; 4] {
            std::array::from_fn(|index| {
                let channel = value[index];
                if channel.is_finite() {
                    channel.clamp(0.0, 1.0)
                } else {
                    fallback[index]
                }
            })
        }
        let fallback = Self::default();
        Self {
            primary: color(self.primary, fallback.primary),
            secondary: color(self.secondary, fallback.secondary),
            accent: color(self.accent, fallback.accent),
            emissive: color(self.emissive, fallback.emissive),
        }
    }
}

/// Authored physical/system description. Gameplay ratings are derived through
/// [`SpacecraftSpec::derived_stats`] rather than entered directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpacecraftSpec {
    pub schema_version: u32,
    pub content_id: String,
    pub display_name: String,
    pub seed: u64,
    pub class: SpacecraftClass,
    /// Physical dimensions in metres.
    pub hull_length: f32,
    pub hull_width: f32,
    pub hull_height: f32,
    pub wing_span: f32,
    /// Normalized silhouette controls.
    pub nose_taper: f32,
    pub wing_sweep: f32,
    pub armor_density: f32,
    /// Physical power-plant scale. Larger values add power but also mass.
    pub engine_scale: f32,
    pub reactor_scale: f32,
    pub engine_count: u8,
    pub weapon_mounts: u8,
    pub hangar_bays: u8,
    pub command_modules: u8,
    pub shield_emitters: u8,
    pub palette: SpacecraftPalette,
}

impl Default for SpacecraftSpec {
    fn default() -> Self {
        Self::preset(SpacecraftClass::Fighter)
    }
}

impl SpacecraftSpec {
    /// Curated, valid starting point for every requested spacecraft role.
    pub fn preset(class: SpacecraftClass) -> Self {
        let (
            content_id,
            display_name,
            hull_length,
            hull_width,
            hull_height,
            wing_span,
            nose_taper,
            wing_sweep,
            armor_density,
            engine_scale,
            reactor_scale,
            engine_count,
            weapon_mounts,
            hangar_bays,
            command_modules,
            shield_emitters,
            palette,
        ) = match class {
            SpacecraftClass::Fighter => (
                "starfall.spaceship.new_fighter",
                "Comet Lance Fighter",
                12.0,
                5.2,
                2.8,
                14.0,
                0.86,
                0.68,
                0.22,
                1.15,
                0.92,
                2,
                4,
                0,
                1,
                2,
                SpacecraftPalette::PRESETS[0],
            ),
            SpacecraftClass::Bomber => (
                "starfall.spaceship.new_bomber",
                "Nova Bomber",
                25.0,
                10.0,
                5.5,
                23.0,
                0.64,
                0.46,
                0.48,
                1.28,
                1.10,
                4,
                8,
                1,
                1,
                4,
                SpacecraftPalette::PRESETS[1],
            ),
            SpacecraftClass::Destroyer => (
                "starfall.spaceship.new_destroyer",
                "Vanguard Destroyer",
                96.0,
                30.0,
                17.0,
                42.0,
                0.52,
                0.25,
                0.72,
                1.45,
                1.30,
                6,
                14,
                2,
                3,
                10,
                SpacecraftPalette::PRESETS[2],
            ),
            SpacecraftClass::CommandShip => (
                "starfall.spaceship.new_command_ship",
                "Celestial Command Ship",
                182.0,
                68.0,
                34.0,
                92.0,
                0.42,
                0.18,
                0.84,
                1.65,
                1.62,
                8,
                18,
                7,
                10,
                18,
                SpacecraftPalette::PRESETS[3],
            ),
            SpacecraftClass::Base => (
                "starfall.spaceship.new_base",
                "Frontier Command Base",
                280.0,
                280.0,
                72.0,
                300.0,
                0.0,
                0.0,
                0.90,
                0.50,
                1.90,
                0,
                26,
                14,
                14,
                26,
                SpacecraftPalette::PRESETS[4],
            ),
        };
        Self {
            schema_version: SPACECRAFT_SCHEMA_VERSION,
            content_id: content_id.into(),
            display_name: display_name.into(),
            seed: class as u64 + 1,
            class,
            hull_length,
            hull_width,
            hull_height,
            wing_span,
            nose_taper,
            wing_sweep,
            armor_density,
            engine_scale,
            reactor_scale,
            engine_count,
            weapon_mounts,
            hangar_bays,
            command_modules,
            shield_emitters,
            palette,
        }
    }

    /// Clamp preview/edit values without hiding schema, identity, or naming
    /// problems from validation.
    pub fn sanitized(&self) -> Self {
        fn finite(value: f32, fallback: f32) -> f32 {
            if value.is_finite() {
                value
            } else {
                fallback
            }
        }
        let fallback = Self::preset(self.class);
        let mut result = self.clone();
        result.content_id = result.content_id.trim().to_string();
        result.display_name = result.display_name.trim().to_string();
        result.hull_length = finite(result.hull_length, fallback.hull_length).clamp(4.0, 400.0);
        result.hull_width = finite(result.hull_width, fallback.hull_width).clamp(2.0, 400.0);
        result.hull_height = finite(result.hull_height, fallback.hull_height).clamp(1.0, 120.0);
        result.wing_span =
            finite(result.wing_span, fallback.wing_span).clamp(result.hull_width, 500.0);
        result.nose_taper = finite(result.nose_taper, fallback.nose_taper).clamp(0.0, 1.0);
        result.wing_sweep = finite(result.wing_sweep, fallback.wing_sweep).clamp(0.0, 1.0);
        result.armor_density = finite(result.armor_density, fallback.armor_density).clamp(0.0, 1.0);
        result.engine_scale = finite(result.engine_scale, fallback.engine_scale).clamp(0.4, 2.5);
        result.reactor_scale = finite(result.reactor_scale, fallback.reactor_scale).clamp(0.4, 2.5);
        result.engine_count = result.engine_count.min(12);
        if result.class != SpacecraftClass::Base {
            result.engine_count = result.engine_count.max(1);
        }
        result.weapon_mounts = result.weapon_mounts.min(32);
        result.hangar_bays = result.hangar_bays.min(16);
        result.command_modules = result.command_modules.min(16);
        result.shield_emitters = result.shield_emitters.min(32);
        result.palette = result.palette.sanitized();
        result
    }

    pub fn derived_stats(&self) -> SpacecraftDerivedStats {
        let spec = self.sanitized();
        let volume = spec.hull_length * spec.hull_width * spec.hull_height * 0.38;
        let mass_tons = volume
            * (0.18 + spec.armor_density * 0.36)
            * (1.0 + spec.reactor_scale * 0.05 + spec.engine_scale * 0.025);
        let reactor_output = spec.class.reactor_output() * spec.reactor_scale;
        let power_demand = f32::from(spec.engine_count) * 48.0 * spec.engine_scale
            + f32::from(spec.weapon_mounts) * 24.0
            + f32::from(spec.shield_emitters) * 18.0
            + f32::from(spec.hangar_bays) * 32.0
            + f32::from(spec.command_modules) * 20.0;
        let cruise_speed = if spec.class == SpacecraftClass::Base {
            0.0
        } else {
            (spec.class.thrust_factor() * f32::from(spec.engine_count) * spec.engine_scale
                / mass_tons.sqrt().max(1.0))
            .clamp(8.0, 380.0)
        };
        let maneuverability = if spec.class == SpacecraftClass::Base {
            0.0
        } else {
            (f32::from(spec.engine_count) * spec.engine_scale * 42.0
                / (mass_tons.cbrt().max(1.0) * (1.0 + spec.hull_length / 85.0)))
                .clamp(1.0, 100.0)
        };
        let firepower = (f32::from(spec.weapon_mounts)
            * (12.0 + spec.reactor_scale * 8.0)
            * (1.0 + spec.armor_density * 0.12))
            .round() as u32;
        let hull_integrity =
            (mass_tons.powf(0.55) * (75.0 + spec.armor_density * 150.0)).round() as u32;
        let shield_capacity =
            (f32::from(spec.shield_emitters) * (55.0 + spec.reactor_scale * 72.0)).round() as u32;
        let crew_capacity = (volume
            / match spec.class {
                SpacecraftClass::Fighter => 32.0,
                SpacecraftClass::Bomber => 26.0,
                SpacecraftClass::Destroyer => 20.0,
                SpacecraftClass::CommandShip => 17.0,
                SpacecraftClass::Base => 28.0,
            })
        .round()
        .clamp(1.0, u32::MAX as f32) as u32;
        SpacecraftDerivedStats {
            mass_tons,
            hull_integrity,
            shield_capacity,
            cruise_speed,
            maneuverability,
            firepower,
            hangar_capacity: u32::from(spec.hangar_bays) * spec.class.hangar_factor(),
            command_capacity: u32::from(spec.command_modules) * spec.class.command_factor(),
            crew_capacity,
            reactor_output,
            power_demand,
            power_margin: reactor_output - power_demand,
        }
    }

    pub fn estimated_parts(&self) -> u32 {
        let spec = self.sanitized();
        let base = if spec.class == SpacecraftClass::Base {
            10
        } else {
            5
        };
        base + u32::from(spec.engine_count.min(8))
            + u32::from(spec.weapon_mounts.min(12))
            + u32::from(spec.hangar_bays.min(4))
            + u32::from(spec.command_modules.min(4))
            + u32::from(spec.shield_emitters.min(8))
    }

    pub fn validate(&self) -> SpacecraftValidation {
        let mut validation = SpacecraftValidation {
            derived: self.derived_stats(),
            estimated_parts: self.estimated_parts(),
            ..Default::default()
        };
        let power_margin = validation.derived.power_margin;
        let mut issue = |severity, code, message: String| {
            validation.issues.push(SpacecraftValidationIssue {
                severity,
                code,
                message,
            });
        };

        if self.schema_version != SPACECRAFT_SCHEMA_VERSION {
            issue(
                SpacecraftValidationSeverity::Error,
                "schema.unsupported",
                format!(
                    "Spacecraft schema {} needs migration to {}",
                    self.schema_version, SPACECRAFT_SCHEMA_VERSION
                ),
            );
        }
        if !valid_content_id(&self.content_id) {
            issue(
                SpacecraftValidationSeverity::Error,
                "identity.content_id",
                "Content ID must be 3–96 lowercase ASCII letters, digits, '.', '_' or '-'".into(),
            );
        }
        if self.display_name.trim().is_empty() || self.display_name.chars().count() > 64 {
            issue(
                SpacecraftValidationSeverity::Error,
                "identity.display_name",
                "Display name must contain 1–64 characters".into(),
            );
        }
        for (label, value, min, max) in [
            ("hull length", self.hull_length, 4.0, 400.0),
            ("hull width", self.hull_width, 2.0, 400.0),
            ("hull height", self.hull_height, 1.0, 120.0),
            ("wing span", self.wing_span, self.hull_width, 500.0),
            ("nose taper", self.nose_taper, 0.0, 1.0),
            ("wing sweep", self.wing_sweep, 0.0, 1.0),
            ("armor density", self.armor_density, 0.0, 1.0),
            ("engine scale", self.engine_scale, 0.4, 2.5),
            ("reactor scale", self.reactor_scale, 0.4, 2.5),
        ] {
            if !value.is_finite() || value < min || value > max {
                issue(
                    SpacecraftValidationSeverity::Error,
                    "geometry.range",
                    format!("{label} must stay in {min:.1}..={max:.1}"),
                );
            }
        }
        if !self.palette.is_valid() {
            issue(
                SpacecraftValidationSeverity::Error,
                "palette.range",
                "Every palette channel must be finite and in 0..=1".into(),
            );
        }
        for (label, value, maximum) in [
            ("engines", self.engine_count, 12),
            ("weapon mounts", self.weapon_mounts, 32),
            ("hangar bays", self.hangar_bays, 16),
            ("command modules", self.command_modules, 16),
            ("shield emitters", self.shield_emitters, 32),
        ] {
            if value > maximum {
                issue(
                    SpacecraftValidationSeverity::Error,
                    "systems.budget",
                    format!("{label} exceeds the supported maximum of {maximum}"),
                );
            }
        }
        if self.class != SpacecraftClass::Base && self.engine_count == 0 {
            issue(
                SpacecraftValidationSeverity::Error,
                "systems.engines",
                "Mobile spacecraft need at least one engine".into(),
            );
        }
        if self.class == SpacecraftClass::Base && self.engine_count > 0 {
            issue(
                SpacecraftValidationSeverity::Warning,
                "role.base_engines",
                "Base engines are station-keeping thrusters; cruise speed remains zero".into(),
            );
        }
        if self.weapon_mounts == 0 {
            issue(
                SpacecraftValidationSeverity::Warning,
                "role.unarmed",
                "This design has no weapon mounts".into(),
            );
        }
        if matches!(
            self.class,
            SpacecraftClass::CommandShip | SpacecraftClass::Base
        ) && self.command_modules < 4
        {
            issue(
                SpacecraftValidationSeverity::Warning,
                "role.command_capacity",
                "Command ships and bases read best with at least four command modules".into(),
            );
        }
        if power_margin < 0.0 {
            issue(
                SpacecraftValidationSeverity::Warning,
                "systems.power_deficit",
                format!(
                    "Systems exceed reactor output by {:.0} power",
                    -power_margin
                ),
            );
        }
        validation
    }
}

pub(crate) fn valid_content_id(value: &str) -> bool {
    value
        .strip_prefix("starfall.spaceship.")
        .is_some_and(|suffix| !suffix.is_empty())
        && (3..=96).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SpacecraftDerivedStats {
    pub mass_tons: f32,
    pub hull_integrity: u32,
    pub shield_capacity: u32,
    pub cruise_speed: f32,
    pub maneuverability: f32,
    pub firepower: u32,
    pub hangar_capacity: u32,
    pub command_capacity: u32,
    pub crew_capacity: u32,
    pub reactor_output: f32,
    pub power_demand: f32,
    pub power_margin: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacecraftValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpacecraftValidationIssue {
    pub severity: SpacecraftValidationSeverity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpacecraftValidation {
    pub issues: Vec<SpacecraftValidationIssue>,
    pub estimated_parts: u32,
    pub derived: SpacecraftDerivedStats,
}

impl SpacecraftValidation {
    pub fn is_publishable(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == SpacecraftValidationSeverity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.severity == SpacecraftValidationSeverity::Error)
            .count()
    }
}

/// Runtime-safe published designs keyed by their stable authored identity.
/// Corrupt/unsupported specs are ignored even if a caller bypasses the normal
/// project publisher and writes JSON manually.
#[derive(Resource, Debug, Clone, Default)]
pub struct PublishedSpacecraftCatalog {
    entries: BTreeMap<String, SpacecraftSpec>,
}

impl PublishedSpacecraftCatalog {
    pub fn get(&self, content_id: &str) -> Option<&SpacecraftSpec> {
        self.entries.get(content_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn content_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Seed only an empty catalog so a Designer session can keep its live
    /// project catalog authoritative over a stale baked generation.
    pub fn seed_from_published(
        &mut self,
        specs: impl IntoIterator<Item = SpacecraftSpec>,
    ) -> usize {
        if !self.entries.is_empty() {
            return 0;
        }
        self.replace(specs)
    }

    /// Replace the complete snapshot, filtering anything that is not safe to
    /// compile. Duplicate IDs deterministically resolve to the last item.
    pub fn replace(&mut self, specs: impl IntoIterator<Item = SpacecraftSpec>) -> usize {
        self.entries.clear();
        for spec in specs {
            if spec.validate().is_publishable() {
                self.entries.insert(spec.content_id.clone(), spec);
            }
        }
        self.entries.len()
    }
}

/// Metadata carried by every generated root, allowing runtime adapters to map
/// the visual/physical compiler output back to stable authored content.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct GeneratedSpacecraft {
    pub content_id: String,
    pub schema_version: u32,
    pub class: SpacecraftClass,
    pub stats: SpacecraftDerivedStats,
}

/// Strong handles allocated while compiling one spacecraft hierarchy.
///
/// Bevy does not immediately reclaim procedurally inserted `Assets` when the
/// entities that reference them are despawned. Runtime adapters and Forge
/// previews can pass this component to [`despawn_generated_spacecraft`] so
/// repeated reconciliation does not leave orphan meshes/materials behind.
#[derive(Component, Debug, Clone, Default)]
pub struct GeneratedSpacecraftAssets {
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
}

impl GeneratedSpacecraftAssets {
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    pub fn release(&self, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
        for mesh in &self.meshes {
            meshes.remove(mesh.id());
        }
        for material in &self.materials {
            materials.remove(material.id());
        }
    }
}

/// Release compiler-owned assets and recursively despawn a generated root.
/// `try_despawn` makes this safe to compose with broad world teardown systems.
pub fn despawn_generated_spacecraft(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    generated_assets: Option<&GeneratedSpacecraftAssets>,
) {
    if let Some(generated_assets) = generated_assets {
        generated_assets.release(meshes, materials);
    }
    commands.entity(root).try_despawn();
}

fn srgb(value: [f32; 4]) -> Color {
    Color::srgba(value[0], value[1], value[2], value[3])
}

fn add_part(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    generated_assets: &mut GeneratedSpacecraftAssets,
    material: Handle<StandardMaterial>,
    mesh: impl Into<Mesh>,
    transform: Transform,
    name: &'static str,
) {
    let mesh = meshes.add(mesh.into());
    generated_assets.meshes.push(mesh.clone());
    let child = commands
        .spawn((
            PbrBundle {
                mesh: Mesh3d(mesh),
                material: MeshMaterial3d(material),
                transform,
                ..default()
            },
            Name::new(name),
        ))
        .id();
    commands.entity(parent).add_child(child);
}

/// Compile one validated recipe into a procedural mesh hierarchy at its real
/// authored metre scale. Editor previews uniformly scale the returned root to
/// fit the viewport; runtime consumers may leave it at scale one.
pub fn spawn_spacecraft(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    spec: &SpacecraftSpec,
    position: Vec3,
) -> Result<Entity, SpacecraftValidation> {
    let validation = spec.validate();
    if !validation.is_publishable() {
        return Err(validation);
    }
    let spec = spec.sanitized();
    let stats = spec.derived_stats();

    let primary = materials.add(StandardMaterial {
        base_color: srgb(spec.palette.primary),
        metallic: 0.78,
        perceptual_roughness: 0.28,
        ..default()
    });
    let secondary = materials.add(StandardMaterial {
        base_color: srgb(spec.palette.secondary),
        metallic: 0.62,
        perceptual_roughness: 0.38,
        ..default()
    });
    let accent = materials.add(StandardMaterial {
        base_color: srgb(spec.palette.accent),
        metallic: 0.70,
        perceptual_roughness: 0.22,
        ..default()
    });
    let emissive_color = srgb(spec.palette.emissive);
    let emissive = materials.add(StandardMaterial {
        base_color: emissive_color,
        emissive: LinearRgba::from(emissive_color) * 4.5,
        metallic: 0.25,
        perceptual_roughness: 0.14,
        ..default()
    });
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgba(0.08, 0.50, 0.78, 0.70),
        emissive: LinearRgba::new(0.03, 0.38, 0.72, 1.0),
        metallic: 0.20,
        perceptual_roughness: 0.10,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let mut generated_assets = GeneratedSpacecraftAssets {
        meshes: Vec::new(),
        materials: vec![
            primary.clone(),
            secondary.clone(),
            accent.clone(),
            emissive.clone(),
            glass.clone(),
        ],
    };

    let root = commands
        .spawn((
            SpatialBundle::from_transform(Transform::from_translation(position)),
            GeneratedSpacecraft {
                content_id: spec.content_id.clone(),
                schema_version: spec.schema_version,
                class: spec.class,
                stats,
            },
            Name::new(spec.display_name.clone()),
        ))
        .id();

    if spec.class == SpacecraftClass::Base {
        spawn_base_geometry(
            commands,
            meshes,
            &mut generated_assets,
            root,
            &spec,
            primary,
            secondary,
            accent,
            emissive,
            glass,
        );
    } else {
        spawn_ship_geometry(
            commands,
            meshes,
            &mut generated_assets,
            root,
            &spec,
            primary,
            secondary,
            accent,
            emissive,
            glass,
        );
    }
    commands.entity(root).insert(generated_assets);
    Ok(root)
}

#[allow(clippy::too_many_arguments)]
fn spawn_ship_geometry(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    generated_assets: &mut GeneratedSpacecraftAssets,
    root: Entity,
    spec: &SpacecraftSpec,
    primary: Handle<StandardMaterial>,
    secondary: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    emissive: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
) {
    let l = spec.hull_length;
    let w = spec.hull_width;
    let h = spec.hull_height;
    add_part(
        commands,
        root,
        meshes,
        generated_assets,
        primary.clone(),
        Sphere::new(1.0),
        Transform::from_xyz(0.0, 0.0, l * 0.02).with_scale(Vec3::new(w * 0.42, h * 0.48, l * 0.42)),
        "Spacecraft Main Hull",
    );
    add_part(
        commands,
        root,
        meshes,
        generated_assets,
        secondary.clone(),
        Cuboid::new(w * 0.48, h * 0.34, l * 0.64),
        Transform::from_xyz(0.0, -h * 0.15, l * 0.04),
        "Spacecraft Keel",
    );
    let nose_scale = 0.16 + spec.nose_taper * 0.18;
    add_part(
        commands,
        root,
        meshes,
        generated_assets,
        accent.clone(),
        Sphere::new(1.0),
        Transform::from_xyz(0.0, 0.0, -l * 0.43).with_scale(Vec3::new(
            w * (0.18 + (1.0 - spec.nose_taper) * 0.12),
            h * 0.31,
            l * nose_scale,
        )),
        "Spacecraft Nose",
    );

    let exposed_wing = ((spec.wing_span - w * 0.42) * 0.5).max(w * 0.12);
    let wing_chord = l * (0.18 + (1.0 - spec.wing_sweep) * 0.12);
    for side in [-1.0_f32, 1.0] {
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            primary.clone(),
            Cuboid::new(exposed_wing, (h * 0.09).max(0.16), wing_chord),
            Transform::from_xyz(side * (w * 0.20 + exposed_wing * 0.50), -h * 0.08, l * 0.06)
                .with_rotation(Quat::from_rotation_y(side * spec.wing_sweep * 0.22)),
            "Spacecraft Wing",
        );
    }

    add_part(
        commands,
        root,
        meshes,
        generated_assets,
        glass,
        Sphere::new(1.0),
        Transform::from_xyz(0.0, h * 0.42, -l * 0.16).with_scale(Vec3::new(
            w * 0.22,
            h * 0.24,
            l * 0.14,
        )),
        "Spacecraft Canopy",
    );
    add_part(
        commands,
        root,
        meshes,
        generated_assets,
        accent.clone(),
        Cuboid::new((w * 0.06).max(0.18), h * 0.78, l * 0.20),
        Transform::from_xyz(0.0, h * 0.43, l * 0.30),
        "Spacecraft Tail Fin",
    );

    let visible_engines = spec.engine_count.min(8);
    for index in 0..visible_engines {
        let fraction = if visible_engines <= 1 {
            0.0
        } else {
            f32::from(index) / f32::from(visible_engines - 1) - 0.5
        };
        let radius = (w / f32::from(visible_engines.max(2)) * 0.24 * spec.engine_scale)
            .clamp(0.22, h * 0.24);
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            emissive.clone(),
            Cylinder::new(radius, (l * 0.10 * spec.engine_scale).max(0.45)),
            Transform::from_xyz(fraction * w * 0.72, -h * 0.12, l * 0.47)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            "Spacecraft Engine",
        );
    }

    let visible_weapons = spec.weapon_mounts.min(12);
    for index in 0..visible_weapons {
        let side = if index % 2 == 0 { -1.0 } else { 1.0 };
        let rank = f32::from(index / 2);
        let x = side * (w * 0.28 + (rank % 3.0) * w * 0.07);
        let z = -l * 0.22 + (rank / 3.0).floor() * l * 0.16;
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            emissive.clone(),
            Cylinder::new((h * 0.025).max(0.10), (l * 0.10).clamp(0.7, 5.0)),
            Transform::from_xyz(x, h * 0.08, z)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            "Spacecraft Weapon Mount",
        );
    }

    for index in 0..spec.hangar_bays.min(4) {
        let side = if index % 2 == 0 { -1.0 } else { 1.0 };
        let z = (f32::from(index / 2) - 0.5) * l * 0.24;
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            secondary.clone(),
            Cuboid::new(w * 0.08, h * 0.24, l * 0.16),
            Transform::from_xyz(side * w * 0.43, -h * 0.08, z),
            "Spacecraft Hangar Bay",
        );
    }
    for index in 0..spec.command_modules.min(4) {
        let x = (f32::from(index) - f32::from(spec.command_modules.min(4) - 1) * 0.5) * w * 0.13;
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            accent.clone(),
            Cuboid::new(w * 0.10, h * 0.28, l * 0.08),
            Transform::from_xyz(x, h * 0.51, l * 0.04),
            "Spacecraft Command Module",
        );
    }
    for index in 0..spec.shield_emitters.min(8) {
        let angle =
            f32::from(index) / f32::from(spec.shield_emitters.clamp(1, 8)) * std::f32::consts::TAU;
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            emissive.clone(),
            Sphere::new((h * 0.055).max(0.12)),
            Transform::from_xyz(angle.cos() * w * 0.38, h * 0.30, angle.sin() * l * 0.30),
            "Spacecraft Shield Emitter",
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_base_geometry(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    generated_assets: &mut GeneratedSpacecraftAssets,
    root: Entity,
    spec: &SpacecraftSpec,
    primary: Handle<StandardMaterial>,
    secondary: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    emissive: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
) {
    let radius = spec.hull_width.min(spec.hull_length) * 0.34;
    let h = spec.hull_height;
    add_part(
        commands,
        root,
        meshes,
        generated_assets,
        primary.clone(),
        Torus::new(radius * 0.58, radius),
        Transform::default(),
        "Space Base Habitat Ring",
    );
    add_part(
        commands,
        root,
        meshes,
        generated_assets,
        secondary.clone(),
        Cylinder::new(radius * 0.16, h * 0.92),
        Transform::default(),
        "Space Base Central Spire",
    );
    add_part(
        commands,
        root,
        meshes,
        generated_assets,
        glass,
        Sphere::new(radius * 0.20),
        Transform::from_xyz(0.0, h * 0.52, 0.0).with_scale(Vec3::new(1.0, 0.52, 1.0)),
        "Space Base Command Dome",
    );
    for arm in 0..4 {
        let angle = arm as f32 * std::f32::consts::FRAC_PI_2;
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            accent.clone(),
            Cuboid::new(radius * 1.55, h * 0.09, h * 0.18),
            Transform::from_xyz(
                angle.cos() * radius * 0.44,
                0.0,
                angle.sin() * radius * 0.44,
            )
            .with_rotation(Quat::from_rotation_y(-angle)),
            "Space Base Docking Arm",
        );
    }
    for index in 0..spec.hangar_bays.min(8) {
        let angle =
            f32::from(index) / f32::from(spec.hangar_bays.clamp(1, 8)) * std::f32::consts::TAU;
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            secondary.clone(),
            Cuboid::new(radius * 0.18, h * 0.20, radius * 0.28),
            Transform::from_xyz(angle.cos() * radius, -h * 0.10, angle.sin() * radius)
                .with_rotation(Quat::from_rotation_y(-angle)),
            "Space Base Hangar",
        );
    }
    for index in 0..spec.weapon_mounts.min(12) {
        let angle =
            f32::from(index) / f32::from(spec.weapon_mounts.clamp(1, 12)) * std::f32::consts::TAU;
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            emissive.clone(),
            Cylinder::new((h * 0.025).max(0.20), h * 0.22),
            Transform::from_xyz(
                angle.cos() * radius * 1.08,
                h * 0.10,
                angle.sin() * radius * 1.08,
            ),
            "Space Base Defense Mount",
        );
    }
    for index in 0..spec.shield_emitters.min(8) {
        let angle =
            f32::from(index) / f32::from(spec.shield_emitters.clamp(1, 8)) * std::f32::consts::TAU;
        add_part(
            commands,
            root,
            meshes,
            generated_assets,
            emissive.clone(),
            Sphere::new((h * 0.045).max(0.22)),
            Transform::from_xyz(
                angle.cos() * radius * 0.72,
                h * 0.35,
                angle.sin() * radius * 0.72,
            ),
            "Space Base Shield Emitter",
        );
    }
}

/// Uniform presentation scale for an authored hierarchy. The compiled child
/// geometry remains in metres; only the generated root is scaled to a bounded
/// gameplay or editor envelope.
pub fn spacecraft_presentation_scale(spec: &SpacecraftSpec, longest_extent: f32) -> f32 {
    let spec = spec.sanitized();
    longest_extent.clamp(1.0, 80.0)
        / spec
            .hull_length
            .max(spec.wing_span)
            .max(spec.hull_width)
            .max(spec.hull_height)
            .max(1.0)
}

/// Uniform scale that frames any class in the Forge preview without changing
/// the compiler's real-metre geometry.
pub fn spacecraft_preview_scale(spec: &SpacecraftSpec) -> f32 {
    spacecraft_presentation_scale(spec, 9.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_requested_class_has_a_publishable_distinct_preset() {
        let presets = SpacecraftClass::ALL.map(SpacecraftSpec::preset);
        assert_eq!(presets.len(), 5);
        for (index, spec) in presets.iter().enumerate() {
            assert_eq!(spec.class, SpacecraftClass::ALL[index]);
            assert!(spec.validate().is_publishable(), "{:?}", spec.validate());
        }
        let ids = presets
            .iter()
            .map(|spec| spec.content_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), 5);
    }

    #[test]
    fn derived_stats_respond_to_physical_system_changes() {
        let base = SpacecraftSpec::preset(SpacecraftClass::Destroyer);
        let mut armed = base.clone();
        armed.weapon_mounts += 4;
        assert!(armed.derived_stats().firepower > base.derived_stats().firepower);

        let mut armored = base.clone();
        armored.armor_density = 1.0;
        assert!(armored.derived_stats().mass_tons > base.derived_stats().mass_tons);
        assert!(armored.derived_stats().hull_integrity > base.derived_stats().hull_integrity);
    }

    #[test]
    fn invalid_identity_schema_and_geometry_block_publish() {
        let mut spec = SpacecraftSpec::default();
        spec.schema_version += 1;
        spec.content_id = "INVALID ID".into();
        spec.hull_length = f32::NAN;
        let validation = spec.validate();
        assert!(!validation.is_publishable());
        assert!(validation.error_count() >= 3);
    }

    #[test]
    fn padded_content_ids_are_rejected_instead_of_stored_as_dirty_keys() {
        let mut spec = SpacecraftSpec::default();
        spec.content_id = format!(" {} ", spec.content_id);
        assert!(!spec.validate().is_publishable());
    }

    #[test]
    fn catalog_filters_invalid_specs_and_preserves_stable_lookup() {
        let valid = SpacecraftSpec::preset(SpacecraftClass::Bomber);
        let mut invalid = SpacecraftSpec::preset(SpacecraftClass::Base);
        invalid.content_id.clear();
        let mut catalog = PublishedSpacecraftCatalog::default();
        assert_eq!(catalog.replace([invalid, valid.clone()]), 1);
        assert_eq!(catalog.get(&valid.content_id), Some(&valid));
    }

    #[test]
    fn preview_scaling_preserves_real_recipe_dimensions() {
        let fighter = SpacecraftSpec::preset(SpacecraftClass::Fighter);
        let base = SpacecraftSpec::preset(SpacecraftClass::Base);
        assert!(spacecraft_preview_scale(&base) < spacecraft_preview_scale(&fighter));
        assert_eq!(base.hull_length, 280.0);
    }

    #[test]
    fn generated_asset_ownership_releases_compiler_allocations() {
        use bevy::asset::AssetPlugin;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>();
        let spec = SpacecraftSpec::preset(SpacecraftClass::Fighter);
        let root = app
            .world_mut()
            .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                    let mut commands = world.commands();
                    spawn_spacecraft(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &spec,
                        Vec3::ZERO,
                    )
                    .unwrap()
                })
            });
        app.world_mut().flush();
        let owned = app
            .world()
            .get::<GeneratedSpacecraftAssets>(root)
            .cloned()
            .unwrap();
        assert!(owned.mesh_count() > 0);
        assert_eq!(owned.material_count(), 5);

        app.world_mut()
            .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                    owned.release(&mut meshes, &mut materials);
                    world.commands().entity(root).try_despawn();
                });
            });
        app.world_mut().flush();
        assert!(app.world().resource::<Assets<Mesh>>().is_empty());
        assert!(app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .is_empty());
        assert!(app.world().get_entity(root).is_err());
    }
}
