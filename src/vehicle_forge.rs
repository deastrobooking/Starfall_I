//! Runtime-safe recipes produced by the Vehicle Forge.
//!
//! This module deliberately describes authored craft without selecting a
//! gameplay motor. The current game has separate robot-assembly, authored
//! boat, and Heavy Water vehicle runtimes; consumers may adapt a published
//! recipe to one of those systems explicitly without making editor data own
//! save progression or physics state.

#![cfg_attr(not(feature = "designer"), allow(dead_code))]

use std::collections::BTreeMap;

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

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
}
