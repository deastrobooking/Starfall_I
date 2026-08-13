//! Runtime-safe recipes produced by the Vehicle Forge.
//!
//! This module deliberately describes authored craft without selecting a
//! gameplay motor. The current game has separate robot-assembly, authored
//! boat, and Heavy Water vehicle runtimes; consumers may adapt a published
//! recipe to one of those systems explicitly without making editor data own
//! save progression or physics state.

#![cfg_attr(not(feature = "designer"), allow(dead_code))]

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::engine::rendering::{PbrBundle, SpatialBundle};

pub const VEHICLE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VehicleClass {
    Boat,
    #[default]
    Car,
    Truck,
    Motorcycle,
}

impl VehicleClass {
    pub const ALL: [Self; 4] = [Self::Boat, Self::Car, Self::Truck, Self::Motorcycle];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Boat => "Boat",
            Self::Car => "Car",
            Self::Truck => "Truck",
            Self::Motorcycle => "Motorcycle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DriveLayout {
    #[default]
    RearWheel,
    FrontWheel,
    AllWheel,
    JetDrive,
}

impl DriveLayout {
    pub const ALL: [Self; 4] = [
        Self::RearWheel,
        Self::FrontWheel,
        Self::AllWheel,
        Self::JetDrive,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::RearWheel => "Rear-wheel",
            Self::FrontWheel => "Front-wheel",
            Self::AllWheel => "All-wheel",
            Self::JetDrive => "Jet drive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BodyStyle {
    #[default]
    Sport,
    Utility,
    Armored,
    Touring,
}

impl BodyStyle {
    pub const ALL: [Self; 4] = [Self::Sport, Self::Utility, Self::Armored, Self::Touring];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Sport => "Sport",
            Self::Utility => "Utility",
            Self::Armored => "Armored",
            Self::Touring => "Touring",
        }
    }
}

/// Renderer-neutral RGBA palette. Keeping raw channels here avoids serializing
/// Bevy material handles or transient GPU assets; consumers choose the color
/// space appropriate for their renderer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehiclePalette {
    pub primary: [f32; 4],
    pub secondary: [f32; 4],
    pub accent: [f32; 4],
    pub emissive: [f32; 4],
}

impl Default for VehiclePalette {
    fn default() -> Self {
        Self {
            primary: [0.10, 0.45, 0.82, 1.0],
            secondary: [0.035, 0.055, 0.09, 1.0],
            accent: [0.95, 0.72, 0.12, 1.0],
            emissive: [0.12, 0.86, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VehicleSpec {
    pub schema_version: u32,
    pub content_id: String,
    pub display_name: String,
    pub class: VehicleClass,
    pub drive: DriveLayout,
    pub body_style: BodyStyle,
    /// Overall length, width, and body height in metres.
    pub length: f32,
    pub width: f32,
    pub height: f32,
    /// Ground vehicles use this directly. Boats use it for decorative rollers
    /// in dry-dock preview but publish it so amphibious adapters stay possible.
    pub wheel_radius: f32,
    pub ground_clearance: f32,
    /// Rated output in kilowatts. Runtime adapters derive their own force units.
    pub power_kw: f32,
    pub mass_kg: f32,
    pub seats: u8,
    pub cargo_capacity_kg: f32,
    /// Normalized silhouette controls.
    pub cabin_position: f32,
    pub aero: f32,
    pub suspension: f32,
    pub armor: f32,
    pub palette: VehiclePalette,
}

impl Default for VehicleSpec {
    fn default() -> Self {
        Self::preset(VehicleClass::Car)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DerivedVehicleStats {
    pub estimated_top_speed_kph: f32,
    pub acceleration_score: f32,
    pub handling_score: f32,
    pub durability_score: f32,
    pub range_score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleIssueSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleValidationIssue {
    pub severity: VehicleIssueSeverity,
    pub code: &'static str,
    pub message: String,
}

impl VehicleSpec {
    pub fn preset(class: VehicleClass) -> Self {
        let mut spec = match class {
            VehicleClass::Boat => Self {
                schema_version: VEHICLE_SCHEMA_VERSION,
                content_id: "starfall.vehicle.new_boat".into(),
                display_name: "New Patrol Boat".into(),
                class,
                drive: DriveLayout::JetDrive,
                body_style: BodyStyle::Touring,
                length: 7.5,
                width: 2.6,
                height: 1.55,
                wheel_radius: 0.34,
                ground_clearance: 0.24,
                power_kw: 320.0,
                mass_kg: 2_900.0,
                seats: 6,
                cargo_capacity_kg: 700.0,
                cabin_position: 0.58,
                aero: 0.54,
                suspension: 0.35,
                armor: 0.28,
                palette: VehiclePalette {
                    primary: [0.08, 0.38, 0.66, 1.0],
                    ..Default::default()
                },
            },
            VehicleClass::Car => Self {
                schema_version: VEHICLE_SCHEMA_VERSION,
                content_id: "starfall.vehicle.new_car".into(),
                display_name: "New Star Car".into(),
                class,
                drive: DriveLayout::AllWheel,
                body_style: BodyStyle::Sport,
                length: 4.4,
                width: 1.9,
                height: 1.35,
                wheel_radius: 0.38,
                ground_clearance: 0.20,
                power_kw: 260.0,
                mass_kg: 1_480.0,
                seats: 4,
                cargo_capacity_kg: 280.0,
                cabin_position: 0.48,
                aero: 0.78,
                suspension: 0.58,
                armor: 0.18,
                palette: VehiclePalette::default(),
            },
            VehicleClass::Truck => Self {
                schema_version: VEHICLE_SCHEMA_VERSION,
                content_id: "starfall.vehicle.new_truck".into(),
                display_name: "New Cargo Truck".into(),
                class,
                drive: DriveLayout::AllWheel,
                body_style: BodyStyle::Utility,
                length: 6.7,
                width: 2.45,
                height: 2.45,
                wheel_radius: 0.56,
                ground_clearance: 0.36,
                power_kw: 390.0,
                mass_kg: 4_900.0,
                seats: 3,
                cargo_capacity_kg: 3_500.0,
                cabin_position: 0.25,
                aero: 0.32,
                suspension: 0.82,
                armor: 0.34,
                palette: VehiclePalette {
                    primary: [0.78, 0.28, 0.08, 1.0],
                    ..Default::default()
                },
            },
            VehicleClass::Motorcycle => Self {
                schema_version: VEHICLE_SCHEMA_VERSION,
                content_id: "starfall.vehicle.new_motorcycle".into(),
                display_name: "New Star Motorcycle".into(),
                class,
                drive: DriveLayout::RearWheel,
                body_style: BodyStyle::Sport,
                length: 2.25,
                width: 0.72,
                height: 1.12,
                wheel_radius: 0.36,
                ground_clearance: 0.18,
                power_kw: 128.0,
                mass_kg: 215.0,
                seats: 2,
                cargo_capacity_kg: 34.0,
                cabin_position: 0.50,
                aero: 0.72,
                suspension: 0.70,
                armor: 0.06,
                palette: VehiclePalette {
                    primary: [0.72, 0.08, 0.16, 1.0],
                    ..Default::default()
                },
            },
        };
        spec.sanitize_in_place();
        spec
    }

    pub fn sanitized(mut self) -> Self {
        self.sanitize_in_place();
        self
    }

    pub fn sanitize_in_place(&mut self) {
        self.length = finite_or(self.length, 4.0).clamp(1.6, 18.0);
        self.width = finite_or(self.width, 1.8).clamp(0.55, 5.0);
        self.height = finite_or(self.height, 1.4).clamp(0.55, 5.5);
        self.wheel_radius = finite_or(self.wheel_radius, 0.35).clamp(0.18, 1.25);
        self.ground_clearance = finite_or(self.ground_clearance, 0.2).clamp(0.05, 1.2);
        self.power_kw = finite_or(self.power_kw, 180.0).clamp(15.0, 2_500.0);
        self.mass_kg = finite_or(self.mass_kg, 1_000.0).clamp(90.0, 80_000.0);
        self.seats = self.seats.clamp(1, 24);
        self.cargo_capacity_kg = finite_or(self.cargo_capacity_kg, 0.0).clamp(0.0, 50_000.0);
        self.cabin_position = finite_or(self.cabin_position, 0.5).clamp(0.12, 0.88);
        self.aero = finite_or(self.aero, 0.5).clamp(0.0, 1.0);
        self.suspension = finite_or(self.suspension, 0.5).clamp(0.0, 1.0);
        self.armor = finite_or(self.armor, 0.2).clamp(0.0, 1.0);
        for color in [
            &mut self.palette.primary,
            &mut self.palette.secondary,
            &mut self.palette.accent,
            &mut self.palette.emissive,
        ] {
            for channel in color.iter_mut() {
                *channel = finite_or(*channel, 1.0).clamp(0.0, 1.0);
            }
        }
    }

    pub fn derived_stats(&self) -> DerivedVehicleStats {
        let spec = self.clone().sanitized();
        let power_to_mass = spec.power_kw / spec.mass_kg.max(1.0);
        let class_speed = match spec.class {
            VehicleClass::Boat => 105.0,
            VehicleClass::Car => 210.0,
            VehicleClass::Truck => 128.0,
            VehicleClass::Motorcycle => 225.0,
        };
        let drive_grip = match spec.drive {
            DriveLayout::RearWheel => 0.82,
            DriveLayout::FrontWheel => 0.86,
            DriveLayout::AllWheel => 1.0,
            DriveLayout::JetDrive => 0.92,
        };
        let (speed_style, handling_style, durability_style, range_style) = match spec.body_style {
            BodyStyle::Sport => (1.08, 1.08, 0.92, 0.95),
            BodyStyle::Utility => (0.94, 0.96, 1.08, 1.08),
            BodyStyle::Armored => (0.86, 0.82, 1.22, 0.88),
            BodyStyle::Touring => (0.98, 1.02, 1.0, 1.14),
        };
        let footprint = spec.length * spec.width;
        let acceleration = (power_to_mass * 145.0 * drive_grip * speed_style).clamp(0.0, 100.0);
        let handling = ((spec.aero * 38.0 + spec.suspension * 42.0 + drive_grip * 25.0)
            - footprint.sqrt() * 2.2
            - spec.armor * 8.0)
            * handling_style;
        let handling = handling.clamp(0.0, 100.0);
        let durability =
            (spec.armor * 58.0 + (spec.mass_kg / 900.0).sqrt() * 12.0 + spec.suspension * 18.0)
                * durability_style;
        let durability = durability.clamp(0.0, 100.0);
        let range = (45.0 + spec.aero * 25.0 + spec.cargo_capacity_kg.sqrt() * 0.45
            - power_to_mass * 30.0)
            * range_style;
        let range = range.clamp(0.0, 100.0);
        DerivedVehicleStats {
            estimated_top_speed_kph: (class_speed
                * (0.52 + spec.aero * 0.36 + power_to_mass.sqrt() * 0.55)
                * (1.0 - spec.armor * 0.09)
                * speed_style)
                .clamp(20.0, 420.0),
            acceleration_score: acceleration,
            handling_score: handling,
            durability_score: durability,
            range_score: range,
        }
    }

    pub fn validate(&self) -> Vec<VehicleValidationIssue> {
        let mut issues = Vec::new();
        let mut push = |severity, code, message: &str| {
            issues.push(VehicleValidationIssue {
                severity,
                code,
                message: message.to_owned(),
            });
        };
        if self.schema_version != VEHICLE_SCHEMA_VERSION {
            push(
                VehicleIssueSeverity::Error,
                "schema.unsupported",
                "Vehicle recipe needs migration before publishing",
            );
        }
        if !valid_content_id(&self.content_id) {
            push(
                VehicleIssueSeverity::Error,
                "id.invalid",
                "Content ID must use lowercase letters, numbers, dots, dashes, or underscores",
            );
        }
        if self.display_name.trim().is_empty() || self.display_name.chars().count() > 64 {
            push(
                VehicleIssueSeverity::Error,
                "name.invalid",
                "Display name must contain 1 to 64 characters",
            );
        }
        let sanitized = self.clone().sanitized();
        if !same_numeric_recipe(self, &sanitized) {
            push(
                VehicleIssueSeverity::Error,
                "dimensions.out_of_range",
                "One or more vehicle parameters are non-finite or outside supported ranges",
            );
        }
        if self.class == VehicleClass::Motorcycle && self.seats > 2 {
            push(
                VehicleIssueSeverity::Error,
                "motorcycle.seats",
                "Motorcycles support at most two seats",
            );
        }
        if self.class == VehicleClass::Boat && self.drive != DriveLayout::JetDrive {
            push(
                VehicleIssueSeverity::Warning,
                "boat.drive",
                "Boat runtime adapters normally expect jet drive",
            );
        }
        if self.class == VehicleClass::Truck && self.cargo_capacity_kg < 500.0 {
            push(
                VehicleIssueSeverity::Warning,
                "truck.cargo",
                "Truck has less cargo capacity than a typical utility frame",
            );
        }
        if self.mass_kg > 25_000.0 || self.length > 12.0 {
            push(
                VehicleIssueSeverity::Warning,
                "preview.large",
                "Large craft may need a dedicated runtime camera and collision profile",
            );
        }
        issues
    }

    pub fn is_publishable(&self) -> bool {
        !self
            .validate()
            .iter()
            .any(|issue| issue.severity == VehicleIssueSeverity::Error)
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

pub(crate) fn valid_content_id(value: &str) -> bool {
    value
        .strip_prefix("starfall.vehicle.")
        .is_some_and(|suffix| !suffix.is_empty())
        && value.len() <= 96
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
}

fn same_numeric_recipe(left: &VehicleSpec, right: &VehicleSpec) -> bool {
    left.length == right.length
        && left.width == right.width
        && left.height == right.height
        && left.wheel_radius == right.wheel_radius
        && left.ground_clearance == right.ground_clearance
        && left.power_kw == right.power_kw
        && left.mass_kg == right.mass_kg
        && left.seats == right.seats
        && left.cargo_capacity_kg == right.cargo_capacity_kg
        && left.cabin_position == right.cabin_position
        && left.aero == right.aero
        && left.suspension == right.suspension
        && left.armor == right.armor
        && left.palette == right.palette
}

/// Read-only game-side lookup for published vehicle recipes. It contains
/// source data, never editor entities, Bevy handles, or active motor state.
#[derive(Resource, Debug, Clone, Default)]
pub struct PublishedVehicleCatalog {
    entries: BTreeMap<String, VehicleSpec>,
}

#[allow(dead_code)] // Public seam for the next runtime vehicle-adapter slice.
impl PublishedVehicleCatalog {
    pub fn get(&self, content_id: &str) -> Option<&VehicleSpec> {
        self.entries.get(content_id)
    }

    pub fn contains(&self, content_id: &str) -> bool {
        self.entries.contains_key(content_id)
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

    /// Startup loader behavior: baked data seeds an empty catalog but never
    /// overwrites a live Designer workspace catalog.
    pub fn seed_from_published(&mut self, specs: impl IntoIterator<Item = VehicleSpec>) -> usize {
        if !self.entries.is_empty() {
            return 0;
        }
        self.replace(specs)
    }

    /// Replace the catalog atomically from a validated workspace or one
    /// immutable published generation. Invalid records are excluded.
    pub fn replace(&mut self, specs: impl IntoIterator<Item = VehicleSpec>) -> usize {
        let next = specs
            .into_iter()
            .filter(VehicleSpec::is_publishable)
            .map(|spec| (spec.content_id.clone(), spec))
            .collect::<BTreeMap<_, _>>();
        self.entries = next;
        self.entries.len()
    }
}

/// Stable metadata carried by every procedural Vehicle Forge hierarchy.
///
/// The complete sanitized recipe is retained as a session snapshot so a live
/// catalog refresh cannot change a parked or mounted craft underneath a
/// player.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct GeneratedVehicle {
    pub content_id: String,
    pub schema_version: u32,
    pub class: VehicleClass,
    pub stats: DerivedVehicleStats,
    pub spec: VehicleSpec,
}

/// Strong handles allocated by one procedural vehicle compilation.
///
/// Bevy's asset collections outlive the entities that reference them. Call
/// [`despawn_generated_vehicle`] when replacing previews or runtime visuals so
/// those per-instance assets are reclaimed as well.
#[derive(Component, Debug, Clone, Default)]
pub struct GeneratedVehicleAssets {
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
}

impl GeneratedVehicleAssets {
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

/// Validation failure returned by the runtime-safe procedural compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleSpawnError {
    pub issues: Vec<VehicleValidationIssue>,
}

impl std::fmt::Display for VehicleSpawnError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "vehicle recipe has {} blocking validation issue(s)",
            self.issues
                .iter()
                .filter(|issue| issue.severity == VehicleIssueSeverity::Error)
                .count()
        )
    }
}

impl std::error::Error for VehicleSpawnError {}

/// Release compiler-owned assets and recursively despawn a generated root.
/// `try_despawn` keeps this composable with broad world teardown systems.
pub fn despawn_generated_vehicle(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    generated_assets: Option<&GeneratedVehicleAssets>,
) {
    if let Some(generated_assets) = generated_assets {
        generated_assets.release(meshes, materials);
    }
    commands.entity(root).try_despawn();
}

fn vehicle_color(value: [f32; 4]) -> Color {
    Color::srgba(value[0], value[1], value[2], value[3])
}

fn add_vehicle_part(
    commands: &mut Commands,
    parent: Entity,
    meshes: &mut Assets<Mesh>,
    generated_assets: &mut GeneratedVehicleAssets,
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

/// Compile one validated recipe into a procedural hierarchy at authored metre
/// scale. Editor and runtime consumers uniformly scale only the returned root,
/// preserving every recipe proportion.
pub fn spawn_vehicle(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    spec: &VehicleSpec,
    position: Vec3,
) -> Result<Entity, VehicleSpawnError> {
    let issues = spec.validate();
    if issues
        .iter()
        .any(|issue| issue.severity == VehicleIssueSeverity::Error)
    {
        return Err(VehicleSpawnError { issues });
    }
    let spec = spec.clone().sanitized();

    let primary = materials.add(StandardMaterial {
        base_color: vehicle_color(spec.palette.primary),
        metallic: 0.72,
        perceptual_roughness: 0.32,
        ..default()
    });
    let secondary = materials.add(StandardMaterial {
        base_color: vehicle_color(spec.palette.secondary),
        metallic: 0.82,
        perceptual_roughness: 0.28,
        ..default()
    });
    let accent = materials.add(StandardMaterial {
        base_color: vehicle_color(spec.palette.accent),
        metallic: 0.55,
        perceptual_roughness: 0.30,
        ..default()
    });
    let emissive_color = vehicle_color(spec.palette.emissive);
    let glow = materials.add(StandardMaterial {
        base_color: emissive_color,
        emissive: LinearRgba::from(emissive_color) * 4.0,
        metallic: 0.18,
        perceptual_roughness: 0.18,
        ..default()
    });
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgba(0.12, 0.28, 0.38, 0.72),
        metallic: 0.25,
        perceptual_roughness: 0.12,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let mut generated_assets = GeneratedVehicleAssets {
        meshes: Vec::new(),
        materials: vec![
            primary.clone(),
            secondary.clone(),
            accent.clone(),
            glow.clone(),
            glass.clone(),
        ],
    };
    let root = commands
        .spawn((
            SpatialBundle::from_transform(Transform::from_translation(position)),
            GeneratedVehicle {
                content_id: spec.content_id.clone(),
                schema_version: spec.schema_version,
                class: spec.class,
                stats: spec.derived_stats(),
                spec: spec.clone(),
            },
            Name::new(spec.display_name.clone()),
        ))
        .id();

    match spec.class {
        VehicleClass::Boat => {
            let hull_height = spec.height * 0.48;
            add_vehicle_part(
                commands,
                root,
                meshes,
                &mut generated_assets,
                primary,
                Capsule3d::new(spec.width * 0.46, spec.length * 0.72),
                Transform::from_xyz(0.0, hull_height * 0.46, 0.0)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::new(1.0, 1.0, 0.42)),
                "Vehicle Boat Hull",
            );
            add_vehicle_part(
                commands,
                root,
                meshes,
                &mut generated_assets,
                glass,
                Cuboid::new(spec.width * 0.78, spec.height * 0.42, spec.length * 0.36),
                Transform::from_xyz(
                    0.0,
                    spec.height * 0.58,
                    (spec.cabin_position - 0.5) * spec.length * 0.8,
                ),
                "Vehicle Boat Cabin",
            );
            for side in [-1.0_f32, 1.0] {
                add_vehicle_part(
                    commands,
                    root,
                    meshes,
                    &mut generated_assets,
                    glow.clone(),
                    Cuboid::new(0.08, 0.12, spec.length * 0.72),
                    Transform::from_xyz(side * spec.width * 0.46, spec.height * 0.32, 0.0),
                    "Vehicle Boat Running Light",
                );
            }
        }
        VehicleClass::Motorcycle => {
            for z in [-spec.length * 0.34, spec.length * 0.34] {
                add_vehicle_part(
                    commands,
                    root,
                    meshes,
                    &mut generated_assets,
                    secondary.clone(),
                    Torus::new(spec.wheel_radius * 0.62, spec.wheel_radius),
                    Transform::from_xyz(0.0, spec.wheel_radius, z)
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                    "Vehicle Motorcycle Wheel",
                );
            }
            add_vehicle_part(
                commands,
                root,
                meshes,
                &mut generated_assets,
                primary,
                Capsule3d::new(spec.width * 0.22, spec.length * 0.58),
                Transform::from_xyz(0.0, spec.wheel_radius * 1.65, 0.0)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::new(1.0, 1.0, 0.68)),
                "Vehicle Motorcycle Frame",
            );
            add_vehicle_part(
                commands,
                root,
                meshes,
                &mut generated_assets,
                accent,
                Cuboid::new(spec.width * 0.68, 0.16, spec.length * 0.34),
                Transform::from_xyz(0.0, spec.wheel_radius * 2.05, spec.length * 0.05),
                "Vehicle Motorcycle Seat",
            );
            add_vehicle_part(
                commands,
                root,
                meshes,
                &mut generated_assets,
                glow,
                Sphere::new(0.11),
                Transform::from_xyz(0.0, spec.wheel_radius * 2.0, -spec.length * 0.38),
                "Vehicle Motorcycle Lamp",
            );
        }
        VehicleClass::Car | VehicleClass::Truck => {
            let body_y = spec.wheel_radius + spec.ground_clearance + spec.height * 0.28;
            let (body_width, body_height, body_length) = match spec.body_style {
                BodyStyle::Sport => (1.0, 0.82, 1.0),
                BodyStyle::Utility => (1.02, 1.0, 0.98),
                BodyStyle::Armored => (1.08, 1.12, 1.0),
                BodyStyle::Touring => (0.96, 0.94, 1.04),
            };
            add_vehicle_part(
                commands,
                root,
                meshes,
                &mut generated_assets,
                primary,
                Cuboid::new(
                    spec.width * body_width,
                    spec.height * 0.52 * body_height,
                    spec.length * body_length,
                ),
                Transform::from_xyz(0.0, body_y, 0.0),
                "Vehicle Body",
            );
            let cabin_length = if spec.class == VehicleClass::Truck {
                spec.length * 0.32
            } else {
                spec.length * 0.52
            };
            add_vehicle_part(
                commands,
                root,
                meshes,
                &mut generated_assets,
                glass,
                Cuboid::new(spec.width * 0.78, spec.height * 0.46, cabin_length),
                Transform::from_xyz(
                    0.0,
                    body_y + spec.height * 0.42,
                    (spec.cabin_position - 0.5) * (spec.length - cabin_length),
                ),
                "Vehicle Cabin",
            );
            if spec.class == VehicleClass::Truck {
                add_vehicle_part(
                    commands,
                    root,
                    meshes,
                    &mut generated_assets,
                    secondary.clone(),
                    Cuboid::new(spec.width * 0.92, spec.height * 0.42, spec.length * 0.46),
                    Transform::from_xyz(0.0, body_y + spec.height * 0.20, spec.length * 0.22),
                    "Vehicle Cargo Bed",
                );
            }
            let axle_z = spec.length * 0.34;
            for x in [-spec.width * 0.48, spec.width * 0.48] {
                for z in [-axle_z, axle_z] {
                    add_vehicle_part(
                        commands,
                        root,
                        meshes,
                        &mut generated_assets,
                        secondary.clone(),
                        Torus::new(spec.wheel_radius * 0.58, spec.wheel_radius),
                        Transform::from_xyz(x, spec.wheel_radius, z)
                            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                        "Vehicle Wheel",
                    );
                }
            }
            for x in [-spec.width * 0.30, spec.width * 0.30] {
                add_vehicle_part(
                    commands,
                    root,
                    meshes,
                    &mut generated_assets,
                    glow.clone(),
                    Sphere::new(0.11),
                    Transform::from_xyz(x, body_y, -spec.length * 0.51),
                    "Vehicle Lamp",
                );
            }
        }
    }
    debug_assert!(generated_assets.mesh_count() > 0);
    debug_assert_eq!(generated_assets.material_count(), 5);
    commands.entity(root).insert(generated_assets);
    Ok(root)
}

/// Uniformly scales an authored hierarchy into a bounded presentation extent.
pub fn vehicle_presentation_scale(spec: &VehicleSpec, longest_extent: f32) -> f32 {
    let spec = spec.clone().sanitized();
    longest_extent.clamp(1.0, 24.0) / spec.length.max(spec.width).max(spec.height).max(1.0)
}

/// Uniform viewport framing shared by every Vehicle Forge preview class.
pub fn vehicle_preview_scale(spec: &VehicleSpec) -> f32 {
    vehicle_presentation_scale(spec, 6.4).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_requested_vehicle_class_has_a_publishable_preset() {
        for class in VehicleClass::ALL {
            let spec = VehicleSpec::preset(class);
            assert_eq!(spec.class, class);
            assert!(
                spec.is_publishable(),
                "{}: {:?}",
                class.label(),
                spec.validate()
            );
        }
    }

    #[test]
    fn sanitization_handles_non_finite_and_extreme_inputs() {
        let spec = VehicleSpec {
            length: f32::NAN,
            mass_kg: f32::INFINITY,
            aero: -40.0,
            seats: u8::MAX,
            ..Default::default()
        };
        let clean = spec.sanitized();
        assert!(clean.length.is_finite());
        assert!(clean.mass_kg.is_finite());
        assert_eq!(clean.aero, 0.0);
        assert_eq!(clean.seats, 24);
    }

    #[test]
    fn serde_round_trip_preserves_recipe_and_derived_stats_are_finite() {
        let spec = VehicleSpec::preset(VehicleClass::Truck);
        let json = serde_json::to_string(&spec).unwrap();
        let decoded: VehicleSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, spec);
        let stats = decoded.derived_stats();
        assert!(stats.estimated_top_speed_kph.is_finite());
        assert!((0.0..=100.0).contains(&stats.handling_score));
    }

    #[test]
    fn published_catalog_is_stable_and_does_not_clobber_a_live_catalog() {
        let mut catalog = PublishedVehicleCatalog::default();
        let car = VehicleSpec::preset(VehicleClass::Car);
        assert_eq!(catalog.seed_from_published([car.clone()]), 1);
        assert!(catalog.contains(&car.content_id));
        assert_eq!(
            catalog.seed_from_published([VehicleSpec::preset(VehicleClass::Boat)]),
            0
        );
        assert_eq!(catalog.len(), 1);
    }

    #[test]
    fn every_class_compiles_with_owned_assets_and_metadata() {
        use bevy::asset::AssetPlugin;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>();

        for class in VehicleClass::ALL {
            let spec = VehicleSpec::preset(class);
            let root = app
                .world_mut()
                .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                    world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                        let mut commands = world.commands();
                        spawn_vehicle(
                            &mut commands,
                            &mut meshes,
                            &mut materials,
                            &spec,
                            Vec3::ZERO,
                        )
                        .expect("publishable preset compiles")
                    })
                });
            app.world_mut().flush();

            let generated = app.world().get::<GeneratedVehicle>(root).unwrap();
            assert_eq!(generated.class, class);
            assert_eq!(generated.spec, spec);
            let owned = app
                .world()
                .get::<GeneratedVehicleAssets>(root)
                .cloned()
                .unwrap();
            assert!(
                owned.mesh_count() >= 3,
                "{} needs a complete kit",
                class.label()
            );
            assert_eq!(owned.material_count(), 5);

            app.world_mut()
                .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                    world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                        owned.release(&mut meshes, &mut materials);
                        world.commands().entity(root).try_despawn();
                    });
                });
            app.world_mut().flush();
        }
        assert!(app.world().resource::<Assets<Mesh>>().is_empty());
        assert!(app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .is_empty());
    }
}
