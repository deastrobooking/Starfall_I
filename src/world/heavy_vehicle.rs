//! Deterministic, engine-neutral port of Heavy Water's independent vehicles.
//!
//! The executable TypeScript exposes exactly two vehicle kinds and four
//! authored presets.  This module preserves their motion, turbo, orbital, and
//! Ghost Ride constants without importing Bevy, Avian, rendering, input, or
//! damage systems.  Runtime adapters own entity spawning and collision/damage
//! application; this domain returns typed descriptions of that work.

#![allow(dead_code)] // Runtime/save adapters are intentionally a later slice.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::{PI, TAU};

pub const HEAVY_VEHICLE_SCHEMA_VERSION: u16 = 1;
pub const MAX_HEAVY_VEHICLES: usize = 256;

pub const DEFAULT_VEHICLE_HP: f32 = 200.0;
pub const DEFAULT_MOUNT_RANGE: f32 = 5.5;

pub const ATV_GROUND_HEIGHT: f32 = 0.0;
pub const FIGHTER_MIN_ALTITUDE: f32 = 1.0;
pub const FIGHTER_MAX_ALTITUDE: f32 = 600.0;

pub const TURBO_DURATION_SECONDS: f32 = 0.7;
pub const TURBO_COOLDOWN_SECONDS: f32 = 1.6;
pub const ATV_TURBO_KICK_FLOOR: f32 = 24.0;
pub const ATV_TURBO_KICK: f32 = 16.0;
pub const FIGHTER_TURBO_KICK_FLOOR: f32 = 55.0;
pub const FIGHTER_TURBO_KICK: f32 = 28.0;

pub const DEFAULT_FORCED_CRUISE_SPEED: f32 = 55.0;
pub const ORBITAL_FORCED_CRUISE_SPEED: f32 = 28.0;
pub const ORBITAL_SPAWN_ALTITUDE: f32 = 300.0;
pub const ORBITAL_SPAWN_FORWARD_OFFSET: f32 = 4.0;

pub const GHOST_RIDE_SPEED_ATV: f32 = 60.0;
pub const GHOST_RIDE_SPEED_FIGHTER: f32 = 110.0;
pub const GHOST_RIDE_TTL_SECONDS: f32 = 6.0;
pub const GHOST_RIDE_HIT_RADIUS: f32 = 4.5;
pub const GHOST_RIDE_WALL_RADIUS: f32 = 2.5;
pub const GHOST_RIDE_BLAST_RADIUS: f32 = 18.0;
pub const GHOST_RIDE_DAMAGE: f32 = 420.0;
pub const GHOST_RIDE_EJECT_SIDE_SPEED: f32 = 11.0;
pub const GHOST_RIDE_EJECT_UP_SPEED: f32 = 8.0;
pub const GHOST_RIDE_EJECT_HEIGHT: f32 = 2.0;

pub const VEHICLE_WALL_HEADROOM: f32 = 2.5;
pub const VEHICLE_WALL_FOOTROOM: f32 = 0.5;
pub const VEHICLE_WALL_SEPARATION: f32 = 0.001;

pub const DEFAULT_RAIDER_ATV_POSITION: VehicleVec3 = VehicleVec3::new(-40.0, 0.6, -15.0);
pub const DEFAULT_COMET_FIGHTER_POSITION: VehicleVec3 = VehicleVec3::new(-55.0, 2.0, -15.0);
pub const RESPAWN_ATV_OFFSET_XZ: VehicleVec3 = VehicleVec3::new(-6.0, 0.6, -4.0);
pub const RESPAWN_FIGHTER_OFFSET_XZ: VehicleVec3 = VehicleVec3::new(6.0, 1.2, -4.0);

/// Game.tsx uses the player's X/Z plus these offsets but authored absolute Y
/// spawn values, rather than adding the player's current altitude.
pub fn source_respawn_position(kind: VehicleKind, player_position: VehicleVec3) -> VehicleVec3 {
    let offset = match kind {
        VehicleKind::Atv => RESPAWN_ATV_OFFSET_XZ,
        VehicleKind::SpaceFighter => RESPAWN_FIGHTER_OFFSET_XZ,
    };
    VehicleVec3::new(
        player_position.x + offset.x,
        offset.y,
        player_position.z + offset.z,
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct VehicleVec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl VehicleVec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn distance_squared(self, other: Self) -> f32 {
        (self - other).length_squared()
    }

    pub fn horizontal_distance_squared(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dz = self.z - other.z;
        dx * dx + dz * dz
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn scaled(self, scale: f32) -> Self {
        Self::new(self.x * scale, self.y * scale, self.z * scale)
    }

    pub fn normalized(self) -> Option<Self> {
        let length_squared = self.length_squared();
        if !length_squared.is_finite() || length_squared <= f32::EPSILON {
            return None;
        }
        Some(self.scaled(length_squared.sqrt().recip()))
    }

    fn lerp(self, other: Self, fraction: f32) -> Self {
        self + (other - self).scaled(fraction)
    }
}

impl std::ops::Add for VehicleVec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl std::ops::Sub for VehicleVec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehicleRgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl VehicleRgb {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    fn is_valid(self) -> bool {
        [self.r, self.g, self.b]
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VehicleKind {
    Atv,
    SpaceFighter,
}

impl VehicleKind {
    pub const ALL: [Self; 2] = [Self::Atv, Self::SpaceFighter];

    pub const fn ghost_ride_speed_floor(self) -> f32 {
        match self {
            Self::Atv => GHOST_RIDE_SPEED_ATV,
            Self::SpaceFighter => GHOST_RIDE_SPEED_FIGHTER,
        }
    }

    pub const fn active_wall_radius(self) -> f32 {
        match self {
            Self::Atv => 2.5,
            Self::SpaceFighter => 3.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CockpitStyle {
    Bubble,
    Wedge,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehicleStyle {
    pub kind: VehicleKind,
    pub body_length: f32,
    pub body_width: f32,
    pub body_height: f32,
    pub primary_color: VehicleRgb,
    pub secondary_color: VehicleRgb,
    pub accent_color: VehicleRgb,
    pub emissive_color: VehicleRgb,
    pub wheel_count: Option<u8>,
    pub wheel_radius: Option<f32>,
    pub wheel_width: Option<f32>,
    pub has_roll_cage: Option<bool>,
    pub has_fenders: Option<bool>,
    pub has_headlights: Option<bool>,
    pub has_exhaust: Option<bool>,
    pub wing_span: Option<f32>,
    pub wing_chord: Option<f32>,
    pub wing_taper: Option<f32>,
    pub wing_tip_fin_height: Option<f32>,
    pub tail_fin_height: Option<f32>,
    pub cannon_count: Option<u8>,
    pub thruster_count: Option<u8>,
    pub cockpit_style: Option<CockpitStyle>,
    pub has_landing_skids: Option<bool>,
}

impl VehicleStyle {
    pub fn validate(self) -> Result<(), HeavyVehicleError> {
        let positive = [self.body_length, self.body_width, self.body_height]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0);
        if !positive
            || !self.primary_color.is_valid()
            || !self.secondary_color.is_valid()
            || !self.accent_color.is_valid()
            || !self.emissive_color.is_valid()
        {
            return Err(HeavyVehicleError::InvalidPresetStyle);
        }
        for value in [
            self.wheel_radius,
            self.wheel_width,
            self.wing_span,
            self.wing_chord,
            self.wing_taper,
            self.wing_tip_fin_height,
            self.tail_fin_height,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_finite() || value < 0.0 {
                return Err(HeavyVehicleError::InvalidPresetStyle);
            }
        }
        Ok(())
    }

    /// Exact invisible Babylon hitbox authored by `VehicleFactory.ts`.
    pub fn hitbox(self) -> VehicleHitboxDescriptor {
        match self.kind {
            VehicleKind::Atv => {
                let wheel_radius = self.wheel_radius.unwrap_or(0.55);
                let size = VehicleVec3::new(
                    self.body_width + 0.4,
                    self.body_height + wheel_radius * 2.0,
                    self.body_length,
                );
                VehicleHitboxDescriptor {
                    local_center: VehicleVec3::new(0.0, wheel_radius + self.body_height / 2.0, 0.0),
                    half_extents: size.scaled(0.5),
                }
            }
            VehicleKind::SpaceFighter => {
                let size = VehicleVec3::new(
                    self.wing_span.unwrap_or(self.body_width) + 0.4,
                    self.body_height + 0.4,
                    self.body_length + 0.4,
                );
                VehicleHitboxDescriptor {
                    local_center: VehicleVec3::ZERO,
                    half_extents: size.scaled(0.5),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehicleHitboxDescriptor {
    pub local_center: VehicleVec3,
    pub half_extents: VehicleVec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleDescriptor {
    pub legacy_key: &'static str,
    pub display_name: &'static str,
    pub style: VehicleStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehiclePresetId {
    RaiderAtv,
    StrikerAtv,
    CometFighter,
    NebulaInterceptor,
}

impl VehiclePresetId {
    pub const ALL: [Self; 4] = [
        Self::RaiderAtv,
        Self::StrikerAtv,
        Self::CometFighter,
        Self::NebulaInterceptor,
    ];

    pub const fn legacy_key(self) -> &'static str {
        match self {
            Self::RaiderAtv => "RaiderATV",
            Self::StrikerAtv => "StrikerATV",
            Self::CometFighter => "CometFighter",
            Self::NebulaInterceptor => "NebulaInterceptor",
        }
    }

    pub fn from_legacy_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.legacy_key() == key)
    }

    pub const fn descriptor(self) -> &'static VehicleDescriptor {
        match self {
            Self::RaiderAtv => &RAIDER_ATV,
            Self::StrikerAtv => &STRIKER_ATV,
            Self::CometFighter => &COMET_FIGHTER,
            Self::NebulaInterceptor => &NEBULA_INTERCEPTOR,
        }
    }

    pub const fn kind(self) -> VehicleKind {
        self.descriptor().style.kind
    }
}

const fn atv_style(
    body_length: f32,
    body_width: f32,
    body_height: f32,
    primary_color: VehicleRgb,
    secondary_color: VehicleRgb,
    accent_color: VehicleRgb,
    emissive_color: VehicleRgb,
    wheel_radius: f32,
    wheel_width: f32,
) -> VehicleStyle {
    VehicleStyle {
        kind: VehicleKind::Atv,
        body_length,
        body_width,
        body_height,
        primary_color,
        secondary_color,
        accent_color,
        emissive_color,
        wheel_count: Some(4),
        wheel_radius: Some(wheel_radius),
        wheel_width: Some(wheel_width),
        has_roll_cage: Some(true),
        has_fenders: Some(true),
        has_headlights: Some(true),
        has_exhaust: Some(true),
        wing_span: None,
        wing_chord: None,
        wing_taper: None,
        wing_tip_fin_height: None,
        tail_fin_height: None,
        cannon_count: None,
        thruster_count: None,
        cockpit_style: None,
        has_landing_skids: None,
    }
}

const fn fighter_style(
    body_length: f32,
    body_width: f32,
    body_height: f32,
    primary_color: VehicleRgb,
    secondary_color: VehicleRgb,
    accent_color: VehicleRgb,
    emissive_color: VehicleRgb,
    wing_span: f32,
    wing_chord: f32,
    wing_taper: f32,
    wing_tip_fin_height: f32,
    tail_fin_height: f32,
    cannon_count: u8,
    cockpit_style: CockpitStyle,
) -> VehicleStyle {
    VehicleStyle {
        kind: VehicleKind::SpaceFighter,
        body_length,
        body_width,
        body_height,
        primary_color,
        secondary_color,
        accent_color,
        emissive_color,
        wheel_count: None,
        wheel_radius: None,
        wheel_width: None,
        has_roll_cage: None,
        has_fenders: None,
        has_headlights: None,
        has_exhaust: None,
        wing_span: Some(wing_span),
        wing_chord: Some(wing_chord),
        wing_taper: Some(wing_taper),
        wing_tip_fin_height: Some(wing_tip_fin_height),
        tail_fin_height: Some(tail_fin_height),
        cannon_count: Some(cannon_count),
        thruster_count: Some(2),
        cockpit_style: Some(cockpit_style),
        has_landing_skids: Some(true),
    }
}

pub static RAIDER_ATV: VehicleDescriptor = VehicleDescriptor {
    legacy_key: "RaiderATV",
    display_name: "Raider ATV",
    style: atv_style(
        3.2,
        1.8,
        0.7,
        VehicleRgb::new(0.85, 0.35, 0.18),
        VehicleRgb::new(0.18, 0.18, 0.22),
        VehicleRgb::new(1.0, 0.8, 0.2),
        VehicleRgb::new(1.0, 0.5, 0.1),
        0.55,
        0.42,
    ),
};

pub static STRIKER_ATV: VehicleDescriptor = VehicleDescriptor {
    legacy_key: "StrikerATV",
    display_name: "Striker ATV",
    style: atv_style(
        3.0,
        1.7,
        0.65,
        VehicleRgb::new(0.18, 0.55, 0.85),
        VehicleRgb::new(0.12, 0.12, 0.16),
        VehicleRgb::new(0.3, 0.95, 1.0),
        VehicleRgb::new(0.2, 0.9, 1.0),
        0.5,
        0.38,
    ),
};

pub static COMET_FIGHTER: VehicleDescriptor = VehicleDescriptor {
    legacy_key: "CometFighter",
    display_name: "Comet Fighter",
    style: fighter_style(
        7.0,
        1.4,
        1.1,
        VehicleRgb::new(0.78, 0.78, 0.85),
        VehicleRgb::new(0.18, 0.2, 0.28),
        VehicleRgb::new(1.0, 0.25, 0.25),
        VehicleRgb::new(1.0, 0.5, 1.0),
        5.4,
        1.8,
        0.45,
        0.6,
        1.2,
        2,
        CockpitStyle::Wedge,
    ),
};

pub static NEBULA_INTERCEPTOR: VehicleDescriptor = VehicleDescriptor {
    legacy_key: "NebulaInterceptor",
    display_name: "Nebula Interceptor",
    style: fighter_style(
        6.4,
        1.2,
        1.0,
        VehicleRgb::new(0.15, 0.18, 0.32),
        VehicleRgb::new(0.05, 0.07, 0.12),
        VehicleRgb::new(0.4, 0.9, 1.0),
        VehicleRgb::new(0.3, 1.0, 1.0),
        4.6,
        1.4,
        0.35,
        0.8,
        1.0,
        4,
        CockpitStyle::Bubble,
    ),
};

pub static VEHICLE_PRESETS: [&VehicleDescriptor; 4] = [
    &RAIDER_ATV,
    &STRIKER_ATV,
    &COMET_FIGHTER,
    &NEBULA_INTERCEPTOR,
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VehicleId(pub String);

impl VehicleId {
    pub fn new(value: impl Into<String>) -> Result<Self, HeavyVehicleError> {
        let value = value.into();
        if !is_valid_stable_id(&value) {
            return Err(HeavyVehicleError::InvalidVehicleId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VehicleOperatorId(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleAccess {
    PartyShared,
    OwnerOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleOwnership {
    pub owner: Option<VehicleOperatorId>,
    pub access: VehicleAccess,
}

impl Default for VehicleOwnership {
    fn default() -> Self {
        Self {
            owner: None,
            access: VehicleAccess::PartyShared,
        }
    }
}

impl VehicleOwnership {
    fn permits(self, operator: VehicleOperatorId) -> bool {
        self.access == VehicleAccess::PartyShared || self.owner == Some(operator)
    }

    fn validate(self) -> Result<(), HeavyVehicleError> {
        if self.access == VehicleAccess::OwnerOnly && self.owner.is_none() {
            return Err(HeavyVehicleError::OwnerRequired);
        }
        Ok(())
    }
}

fn is_valid_stable_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehicleTransformRecord {
    pub position: VehicleVec3,
    pub velocity: VehicleVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    /// Signed forward speed in metres per second.
    pub speed: f32,
}

impl VehicleTransformRecord {
    pub fn parked(position: VehicleVec3) -> Self {
        Self {
            position,
            velocity: VehicleVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            speed: 0.0,
        }
    }

    fn validate(self) -> Result<(), HeavyVehicleError> {
        if !self.position.is_finite()
            || !self.velocity.is_finite()
            || !self.yaw.is_finite()
            || !self.pitch.is_finite()
            || !self.roll.is_finite()
            || !self.speed.is_finite()
        {
            return Err(HeavyVehicleError::InvalidTransform);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehicleHealthRecord {
    pub hp: f32,
    pub max_hp: f32,
}

impl Default for VehicleHealthRecord {
    fn default() -> Self {
        Self {
            hp: DEFAULT_VEHICLE_HP,
            max_hp: DEFAULT_VEHICLE_HP,
        }
    }
}

impl VehicleHealthRecord {
    fn validate(self) -> Result<(), HeavyVehicleError> {
        if !self.hp.is_finite()
            || !self.max_hp.is_finite()
            || self.max_hp <= 0.0
            || self.hp < 0.0
            || self.hp > self.max_hp
        {
            return Err(HeavyVehicleError::InvalidHealth);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GhostRideState {
    pub locked_heading: VehicleVec3,
    pub locked_speed: f32,
    pub ttl_seconds: f32,
}

impl GhostRideState {
    fn validate(self, kind: VehicleKind) -> Result<(), HeavyVehicleError> {
        let heading_length = self.locked_heading.length_squared();
        if !self.locked_heading.is_finite()
            || (heading_length - 1.0).abs() > 0.001
            || !self.locked_speed.is_finite()
            || self.locked_speed < kind.ghost_ride_speed_floor()
            || !self.ttl_seconds.is_finite()
            || !(0.0..=GHOST_RIDE_TTL_SECONDS).contains(&self.ttl_seconds)
        {
            return Err(HeavyVehicleError::InvalidGhostRide);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum VehicleLifecycle {
    Available,
    GhostRiding { ghost: GhostRideState },
    Destroyed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleRecord {
    pub id: VehicleId,
    pub preset: VehiclePresetId,
    pub ownership: VehicleOwnership,
    pub transform: VehicleTransformRecord,
    pub health: VehicleHealthRecord,
    pub lifecycle: VehicleLifecycle,
}

impl VehicleRecord {
    pub fn kind(&self) -> VehicleKind {
        self.preset.kind()
    }

    pub fn descriptor(&self) -> &'static VehicleDescriptor {
        self.preset.descriptor()
    }

    pub fn is_mountable(&self) -> bool {
        matches!(self.lifecycle, VehicleLifecycle::Available) && self.health.hp > 0.0
    }

    pub fn validate(&self) -> Result<(), HeavyVehicleError> {
        if !is_valid_stable_id(self.id.as_str()) {
            return Err(HeavyVehicleError::InvalidVehicleId(self.id.0.clone()));
        }
        self.ownership.validate()?;
        self.transform.validate()?;
        self.health.validate()?;
        match self.lifecycle {
            VehicleLifecycle::Available if self.health.hp <= 0.0 => {
                Err(HeavyVehicleError::HealthLifecycleMismatch)
            }
            VehicleLifecycle::Destroyed if self.health.hp != 0.0 => {
                Err(HeavyVehicleError::HealthLifecycleMismatch)
            }
            VehicleLifecycle::GhostRiding { ghost } if self.health.hp <= 0.0 => {
                let _ = ghost;
                Err(HeavyVehicleError::HealthLifecycleMismatch)
            }
            VehicleLifecycle::GhostRiding { ghost } => ghost.validate(self.kind()),
            VehicleLifecycle::Available | VehicleLifecycle::Destroyed => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleMountRecord {
    pub vehicle_id: VehicleId,
    pub operator: VehicleOperatorId,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleInputState {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub boost: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct VehicleTurboState {
    pub active_seconds: f32,
    pub cooldown_seconds: f32,
}

impl VehicleTurboState {
    pub fn is_active(self) -> bool {
        self.active_seconds > 0.0
    }

    fn clear(&mut self) {
        self.active_seconds = 0.0;
        self.cooldown_seconds = 0.0;
    }

    fn tick(&mut self, delta_seconds: f32) {
        self.active_seconds = timer_subtract(self.active_seconds, delta_seconds);
        self.cooldown_seconds = timer_subtract(self.cooldown_seconds, delta_seconds);
    }

    fn validate(self) -> Result<(), HeavyVehicleError> {
        if !self.active_seconds.is_finite()
            || !self.cooldown_seconds.is_finite()
            || !(0.0..=TURBO_DURATION_SECONDS).contains(&self.active_seconds)
            || !(0.0..=TURBO_COOLDOWN_SECONDS).contains(&self.cooldown_seconds)
        {
            return Err(HeavyVehicleError::InvalidTurboState);
        }
        Ok(())
    }
}

fn timer_subtract(timer: f32, delta_seconds: f32) -> f32 {
    let remaining = timer - delta_seconds;
    if remaining <= 0.000_001 {
        0.0
    } else {
        remaining
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ForcedCruiseState {
    pub enabled: bool,
    pub speed: f32,
}

impl Default for ForcedCruiseState {
    fn default() -> Self {
        Self {
            enabled: false,
            speed: DEFAULT_FORCED_CRUISE_SPEED,
        }
    }
}

impl ForcedCruiseState {
    fn validate(self) -> Result<(), HeavyVehicleError> {
        if !self.speed.is_finite() || self.speed <= 0.0 {
            return Err(HeavyVehicleError::InvalidForcedCruiseSpeed);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeavyVehicleFleet {
    pub schema_version: u16,
    pub next_serial: u64,
    pub vehicles: BTreeMap<VehicleId, VehicleRecord>,
    /// Heavy Water has one active vehicle for the whole game.  Keeping one
    /// explicit lease preserves that rule and makes operator ownership atomic.
    pub active_mount: Option<VehicleMountRecord>,
    pub input: VehicleInputState,
    pub turbo: VehicleTurboState,
    pub forced_cruise: ForcedCruiseState,
}

impl Default for HeavyVehicleFleet {
    fn default() -> Self {
        Self {
            schema_version: HEAVY_VEHICLE_SCHEMA_VERSION,
            next_serial: 0,
            vehicles: BTreeMap::new(),
            active_mount: None,
            input: VehicleInputState::default(),
            turbo: VehicleTurboState::default(),
            forced_cruise: ForcedCruiseState::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleAabb {
    pub min: VehicleVec3,
    pub max: VehicleVec3,
}

impl VehicleAabb {
    pub const fn new(min: VehicleVec3, max: VehicleVec3) -> Self {
        Self { min, max }
    }

    pub fn validate(&self) -> Result<(), HeavyVehicleError> {
        if !self.min.is_finite()
            || !self.max.is_finite()
            || self.min.x > self.max.x
            || self.min.y > self.max.y
            || self.min.z > self.max.z
        {
            return Err(HeavyVehicleError::InvalidWallAabb);
        }
        Ok(())
    }

    fn expanded_for_vehicle(&self, horizontal_radius: f32) -> Self {
        Self {
            min: VehicleVec3::new(
                self.min.x - horizontal_radius,
                self.min.y - VEHICLE_WALL_HEADROOM,
                self.min.z - horizontal_radius,
            ),
            max: VehicleVec3::new(
                self.max.x + horizontal_radius,
                self.max.y + VEHICLE_WALL_FOOTROOM,
                self.max.z + horizontal_radius,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GhostRideTargetClass {
    GroundEnemy,
    AerialUnit,
    BaseStructure,
}

impl GhostRideTargetClass {
    pub fn trigger_radius(self) -> f32 {
        match self {
            Self::GroundEnemy => GHOST_RIDE_HIT_RADIUS,
            Self::AerialUnit => GHOST_RIDE_HIT_RADIUS * 1.8_f32.sqrt(),
            Self::BaseStructure => GHOST_RIDE_HIT_RADIUS * 1.5_f32.sqrt(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GhostRideTargetProbe {
    pub stable_id: u64,
    pub class: GhostRideTargetClass,
    pub position: VehicleVec3,
    pub alive: bool,
}

impl GhostRideTargetProbe {
    fn validate(self) -> Result<(), HeavyVehicleError> {
        if !self.position.is_finite() {
            return Err(HeavyVehicleError::InvalidGhostTarget(self.stable_id));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VehicleTickInput<'a> {
    pub delta_seconds: f32,
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub walls: &'a [VehicleAabb],
    pub ghost_targets: &'a [GhostRideTargetProbe],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HeavyVehicleError {
    UnsupportedSchemaVersion {
        found: u16,
        expected: u16,
    },
    TooManyVehicles {
        found: usize,
        maximum: usize,
    },
    InvalidVehicleId(String),
    DuplicateVehicleRecordId(VehicleId),
    InvalidPresetStyle,
    InvalidTransform,
    InvalidHealth,
    HealthLifecycleMismatch,
    InvalidGhostRide,
    InvalidTurboState,
    InvalidForcedCruiseSpeed,
    InvalidWallAabb,
    InvalidGhostTarget(u64),
    DuplicateGhostTarget(u64),
    InvalidDeltaSeconds,
    InvalidCamera,
    InvalidGroundHeight,
    InvalidDamage,
    OwnerRequired,
    UnknownVehicle(VehicleId),
    VehicleNotMountable(VehicleId),
    VehicleNotDestroyed(VehicleId),
    VehicleAccessDenied {
        vehicle_id: VehicleId,
        operator: VehicleOperatorId,
    },
    MountAlreadyOwned {
        vehicle_id: VehicleId,
        operator: VehicleOperatorId,
    },
    MountOperatorMismatch {
        expected: VehicleOperatorId,
        received: VehicleOperatorId,
    },
    NoActiveMount,
    TurboActive,
    TurboCoolingDown,
    TurboDisabledByForcedCruise,
    GhostRideRequiresBoost,
    PresetKindMismatch {
        requested: VehicleKind,
        preset: VehiclePresetId,
    },
    FleetInvariantViolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleUnmountReason {
    Voluntary,
    GhostRide,
    Destroyed,
    Despawned,
    OrbitalTeardown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehicleEjectDescriptor {
    pub position: VehicleVec3,
    pub velocity: VehicleVec3,
    pub somersault: bool,
}

impl VehicleEjectDescriptor {
    /// Exact placement performed by Game.tsx after a normal `exit()`.
    pub fn normal_from(transform: VehicleTransformRecord) -> Self {
        Self {
            position: transform.position + VehicleVec3::new(2.5, 1.0, 0.0),
            velocity: VehicleVec3::ZERO,
            somersault: false,
        }
    }

    /// Exact side/up bail used by `startGhostRide()` + Game.tsx.
    pub fn ghost_ride_from(transform: VehicleTransformRecord) -> Self {
        let right = VehicleVec3::new(transform.yaw.cos(), 0.0, -transform.yaw.sin());
        Self {
            position: transform.position + VehicleVec3::new(0.0, GHOST_RIDE_EJECT_HEIGHT, 0.0),
            velocity: right.scaled(GHOST_RIDE_EJECT_SIDE_SPEED)
                + VehicleVec3::new(0.0, GHOST_RIDE_EJECT_UP_SPEED, 0.0),
            somersault: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleDamageKind {
    Explosive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplosionTier {
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExplosionVisualDescriptor {
    pub tier: ExplosionTier,
    pub radius: f32,
    pub color: VehicleRgb,
    pub camera_shake_intensity: f32,
    pub camera_shake_duration_seconds: f32,
    pub shockwave: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplosionAudioDescriptor {
    pub legacy_url: String,
    pub volume: f32,
    pub playback_rate: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GhostRideDamageDescriptor {
    pub base_damage: f32,
    pub blast_radius: f32,
    pub damage_kind: VehicleDamageKind,
    pub ground_minimum_falloff: f32,
    pub ground_knockback_force: f32,
    pub aerial_damage_multiplier: f32,
    pub aerial_radius_multiplier: f32,
    pub aerial_minimum_falloff: f32,
    pub base_minimum_falloff: f32,
    /// Heavy Water never dispatches its player-damage path for Ghost Ride.
    pub damages_players: bool,
}

impl GhostRideDamageDescriptor {
    pub const SOURCE: Self = Self {
        base_damage: GHOST_RIDE_DAMAGE,
        blast_radius: GHOST_RIDE_BLAST_RADIUS,
        damage_kind: VehicleDamageKind::Explosive,
        ground_minimum_falloff: 0.4,
        ground_knockback_force: 360.0,
        aerial_damage_multiplier: 0.85,
        aerial_radius_multiplier: 1.6,
        aerial_minimum_falloff: 0.35,
        base_minimum_falloff: 0.4,
        damages_players: false,
    };

    /// Exact source falloff. `None` means the target is outside that class's
    /// authored reach; the shared damage adapter should perform the hit.
    pub fn damage_at(self, class: GhostRideTargetClass, distance: f32) -> Option<f32> {
        if !distance.is_finite() || distance < 0.0 {
            return None;
        }
        match class {
            GhostRideTargetClass::GroundEnemy => {
                if distance > self.blast_radius {
                    return None;
                }
                let falloff = self
                    .ground_minimum_falloff
                    .max(1.0 - distance / self.blast_radius);
                Some(self.base_damage * falloff)
            }
            GhostRideTargetClass::AerialUnit => {
                let maximum_distance = self.blast_radius * self.aerial_radius_multiplier.sqrt();
                if distance > maximum_distance {
                    return None;
                }
                let falloff = self
                    .aerial_minimum_falloff
                    .max(1.0 - distance / (self.blast_radius * 1.3));
                Some(self.base_damage * self.aerial_damage_multiplier * falloff)
            }
            GhostRideTargetClass::BaseStructure => {
                if distance > self.blast_radius {
                    return None;
                }
                let falloff = self
                    .base_minimum_falloff
                    .max(1.0 - distance / self.blast_radius);
                Some(self.base_damage * falloff)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GhostRideTrigger {
    GroundImpact,
    WallImpact {
        wall: VehicleAabb,
    },
    TargetImpact {
        stable_id: u64,
        class: GhostRideTargetClass,
    },
    FuseExpired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GhostRideDetonationDescriptor {
    pub vehicle_id: VehicleId,
    pub kind: VehicleKind,
    pub center: VehicleVec3,
    pub trigger: GhostRideTrigger,
    pub visual: ExplosionVisualDescriptor,
    pub audio: ExplosionAudioDescriptor,
    pub radial_damage: GhostRideDamageDescriptor,
}

/// Kinematic intent for the ECS/Avian bridge.  The domain keeps a serializable
/// prediction for saves and deterministic rules, while the adapter remains
/// responsible for applying/reconciling the actual rigid-body transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleMotionDescriptor {
    pub vehicle_id: VehicleId,
    pub from: VehicleVec3,
    pub to: VehicleVec3,
    pub linear_velocity: VehicleVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub continuous_collision: bool,
}

impl GhostRideDetonationDescriptor {
    fn source(
        vehicle_id: VehicleId,
        kind: VehicleKind,
        center: VehicleVec3,
        trigger: GhostRideTrigger,
    ) -> Self {
        Self {
            vehicle_id,
            kind,
            center,
            trigger,
            visual: ExplosionVisualDescriptor {
                tier: ExplosionTier::Large,
                radius: GHOST_RIDE_BLAST_RADIUS,
                color: VehicleRgb::new(1.0, 0.55, 0.18),
                camera_shake_intensity: 0.7,
                camera_shake_duration_seconds: 0.45,
                shockwave: true,
            },
            audio: ExplosionAudioDescriptor {
                legacy_url: "/sounds/hit.mp3".to_string(),
                volume: 1.0,
                playback_rate: 0.5,
            },
            radial_damage: GhostRideDamageDescriptor::SOURCE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VehicleEffect {
    SpawnRequested {
        vehicle_id: VehicleId,
        preset: VehiclePresetId,
        position: VehicleVec3,
        hitbox: VehicleHitboxDescriptor,
    },
    DespawnRequested {
        vehicle_id: VehicleId,
    },
    Mounted {
        vehicle_id: VehicleId,
        operator: VehicleOperatorId,
    },
    Unmounted {
        vehicle_id: VehicleId,
        operator: VehicleOperatorId,
        reason: VehicleUnmountReason,
        eject: Option<VehicleEjectDescriptor>,
    },
    TurboStarted {
        vehicle_id: VehicleId,
        kind: VehicleKind,
        duration_seconds: f32,
        cooldown_seconds: f32,
    },
    ForcedCruiseChanged {
        enabled: bool,
        speed: f32,
    },
    KinematicMotionRequested(VehicleMotionDescriptor),
    WallContact {
        vehicle_id: VehicleId,
        wall: VehicleAabb,
    },
    VehicleDamaged {
        vehicle_id: VehicleId,
        amount: f32,
        remaining_hp: f32,
    },
    VehicleDestroyed {
        vehicle_id: VehicleId,
        position: VehicleVec3,
    },
    GhostRideStarted {
        vehicle_id: VehicleId,
        operator: VehicleOperatorId,
        locked_heading: VehicleVec3,
        locked_speed: f32,
        ttl_seconds: f32,
        eject: VehicleEjectDescriptor,
    },
    GhostRideDetonated(GhostRideDetonationDescriptor),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleSpawnOutcome {
    pub vehicle_id: VehicleId,
    pub effect: VehicleEffect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleRespawnOutcome {
    pub removed: Vec<VehicleId>,
    pub spawned: VehicleSpawnOutcome,
    pub effects: Vec<VehicleEffect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbitalFighterMountOutcome {
    pub vehicle_id: VehicleId,
    pub effects: Vec<VehicleEffect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleDamageOutcome {
    pub remaining_hp: f32,
    pub destroyed: bool,
    pub effects: Vec<VehicleEffect>,
}

impl HeavyVehicleFleet {
    pub fn validate(&self) -> Result<(), HeavyVehicleError> {
        if self.schema_version != HEAVY_VEHICLE_SCHEMA_VERSION {
            return Err(HeavyVehicleError::UnsupportedSchemaVersion {
                found: self.schema_version,
                expected: HEAVY_VEHICLE_SCHEMA_VERSION,
            });
        }
        if self.vehicles.len() > MAX_HEAVY_VEHICLES {
            return Err(HeavyVehicleError::TooManyVehicles {
                found: self.vehicles.len(),
                maximum: MAX_HEAVY_VEHICLES,
            });
        }
        self.turbo.validate()?;
        self.forced_cruise.validate()?;
        for (key, record) in &self.vehicles {
            record.validate()?;
            if key != &record.id {
                return Err(HeavyVehicleError::DuplicateVehicleRecordId(key.clone()));
            }
        }
        if let Some(mount) = &self.active_mount {
            let vehicle = self
                .vehicles
                .get(&mount.vehicle_id)
                .ok_or_else(|| HeavyVehicleError::UnknownVehicle(mount.vehicle_id.clone()))?;
            if !vehicle.is_mountable() || !vehicle.ownership.permits(mount.operator) {
                return Err(HeavyVehicleError::FleetInvariantViolation);
            }
        } else if self.turbo.is_active() {
            // The executable clears turbo on every normal or Ghost Ride exit.
            return Err(HeavyVehicleError::FleetInvariantViolation);
        }
        Ok(())
    }

    pub fn active_vehicle(&self) -> Option<&VehicleRecord> {
        self.active_mount
            .as_ref()
            .and_then(|mount| self.vehicles.get(&mount.vehicle_id))
    }

    pub fn active_vehicle_mut(&mut self) -> Option<&mut VehicleRecord> {
        let vehicle_id = self.active_mount.as_ref()?.vehicle_id.clone();
        self.vehicles.get_mut(&vehicle_id)
    }

    pub fn set_input(&mut self, input: VehicleInputState) {
        self.input = input;
    }

    pub fn patch_input(&mut self, patch: VehicleInputPatch) {
        if let Some(value) = patch.forward {
            self.input.forward = value;
        }
        if let Some(value) = patch.back {
            self.input.back = value;
        }
        if let Some(value) = patch.left {
            self.input.left = value;
        }
        if let Some(value) = patch.right {
            self.input.right = value;
        }
        if let Some(value) = patch.up {
            self.input.up = value;
        }
        if let Some(value) = patch.down {
            self.input.down = value;
        }
        if let Some(value) = patch.boost {
            self.input.boost = value;
        }
    }

    pub fn spawn_preset(
        &mut self,
        preset: VehiclePresetId,
        position: VehicleVec3,
        ownership: VehicleOwnership,
    ) -> Result<VehicleSpawnOutcome, HeavyVehicleError> {
        if self.vehicles.len() >= MAX_HEAVY_VEHICLES {
            return Err(HeavyVehicleError::TooManyVehicles {
                found: self.vehicles.len() + 1,
                maximum: MAX_HEAVY_VEHICLES,
            });
        }
        if !position.is_finite() {
            return Err(HeavyVehicleError::InvalidTransform);
        }
        ownership.validate()?;
        preset.descriptor().style.validate()?;

        let vehicle_id = self.next_vehicle_id()?;
        let record = VehicleRecord {
            id: vehicle_id.clone(),
            preset,
            ownership,
            transform: VehicleTransformRecord::parked(position),
            health: VehicleHealthRecord::default(),
            lifecycle: VehicleLifecycle::Available,
        };
        record.validate()?;
        self.vehicles.insert(vehicle_id.clone(), record);
        let effect = VehicleEffect::SpawnRequested {
            vehicle_id: vehicle_id.clone(),
            preset,
            position,
            hitbox: preset.descriptor().style.hitbox(),
        };
        Ok(VehicleSpawnOutcome { vehicle_id, effect })
    }

    fn next_vehicle_id(&mut self) -> Result<VehicleId, HeavyVehicleError> {
        for _ in 0..=self.vehicles.len() {
            let serial = self.next_serial;
            self.next_serial = self.next_serial.wrapping_add(1);
            let candidate = VehicleId(format!("vehicle_{serial}"));
            if !self.vehicles.contains_key(&candidate) {
                return Ok(candidate);
            }
        }
        Err(HeavyVehicleError::FleetInvariantViolation)
    }

    pub fn nearest_mountable(
        &self,
        operator: VehicleOperatorId,
        position: VehicleVec3,
        maximum_range: f32,
    ) -> Result<Option<&VehicleRecord>, HeavyVehicleError> {
        if !position.is_finite() || !maximum_range.is_finite() || maximum_range < 0.0 {
            return Err(HeavyVehicleError::InvalidTransform);
        }
        let maximum_squared = maximum_range * maximum_range;
        Ok(self
            .vehicles
            .values()
            .filter(|vehicle| {
                vehicle.is_mountable()
                    && vehicle.ownership.permits(operator)
                    && vehicle.transform.position.distance_squared(position) < maximum_squared
            })
            .min_by(|left, right| {
                let left_distance = left.transform.position.distance_squared(position);
                let right_distance = right.transform.position.distance_squared(position);
                left_distance
                    .total_cmp(&right_distance)
                    .then_with(|| left.id.cmp(&right.id))
            }))
    }

    pub fn mount(
        &mut self,
        vehicle_id: &VehicleId,
        operator: VehicleOperatorId,
    ) -> Result<VehicleEffect, HeavyVehicleError> {
        if let Some(mount) = &self.active_mount {
            return Err(HeavyVehicleError::MountAlreadyOwned {
                vehicle_id: mount.vehicle_id.clone(),
                operator: mount.operator,
            });
        }
        let vehicle = self
            .vehicles
            .get_mut(vehicle_id)
            .ok_or_else(|| HeavyVehicleError::UnknownVehicle(vehicle_id.clone()))?;
        if !vehicle.is_mountable() {
            return Err(HeavyVehicleError::VehicleNotMountable(vehicle_id.clone()));
        }
        if !vehicle.ownership.permits(operator) {
            return Err(HeavyVehicleError::VehicleAccessDenied {
                vehicle_id: vehicle_id.clone(),
                operator,
            });
        }

        // Entering in Heavy Water zeroes velocity and signed speed.  Resetting
        // the full input latch additionally prevents cross-operator key leaks.
        vehicle.transform.velocity = VehicleVec3::ZERO;
        vehicle.transform.speed = 0.0;
        self.input = VehicleInputState::default();
        self.turbo.clear();
        self.active_mount = Some(VehicleMountRecord {
            vehicle_id: vehicle_id.clone(),
            operator,
        });
        Ok(VehicleEffect::Mounted {
            vehicle_id: vehicle_id.clone(),
            operator,
        })
    }

    pub fn eject(
        &mut self,
        operator: VehicleOperatorId,
    ) -> Result<VehicleEffect, HeavyVehicleError> {
        self.eject_with_reason(operator, VehicleUnmountReason::Voluntary, true)
    }

    fn eject_with_reason(
        &mut self,
        operator: VehicleOperatorId,
        reason: VehicleUnmountReason,
        place_operator: bool,
    ) -> Result<VehicleEffect, HeavyVehicleError> {
        let mount = self
            .active_mount
            .clone()
            .ok_or(HeavyVehicleError::NoActiveMount)?;
        if mount.operator != operator {
            return Err(HeavyVehicleError::MountOperatorMismatch {
                expected: mount.operator,
                received: operator,
            });
        }
        let vehicle = self
            .vehicles
            .get_mut(&mount.vehicle_id)
            .ok_or(HeavyVehicleError::FleetInvariantViolation)?;
        let eject = place_operator.then(|| VehicleEjectDescriptor::normal_from(vehicle.transform));
        vehicle.transform.velocity = VehicleVec3::ZERO;
        vehicle.transform.speed = 0.0;
        self.active_mount = None;
        self.input = VehicleInputState::default();
        self.turbo.clear();
        Ok(VehicleEffect::Unmounted {
            vehicle_id: mount.vehicle_id,
            operator,
            reason,
            eject,
        })
    }

    pub fn set_forced_cruise(
        &mut self,
        enabled: bool,
        speed: Option<f32>,
    ) -> Result<VehicleEffect, HeavyVehicleError> {
        let speed = speed.unwrap_or(DEFAULT_FORCED_CRUISE_SPEED);
        if !speed.is_finite() || speed <= 0.0 {
            return Err(HeavyVehicleError::InvalidForcedCruiseSpeed);
        }
        self.forced_cruise = ForcedCruiseState { enabled, speed };
        if enabled {
            if let Some(vehicle) = self.active_vehicle_mut() {
                if vehicle.kind() == VehicleKind::SpaceFighter {
                    vehicle.transform.speed = speed;
                }
            }
        }
        Ok(VehicleEffect::ForcedCruiseChanged { enabled, speed })
    }

    pub fn trigger_turbo(
        &mut self,
        operator: VehicleOperatorId,
    ) -> Result<VehicleEffect, HeavyVehicleError> {
        let mount = self
            .active_mount
            .clone()
            .ok_or(HeavyVehicleError::NoActiveMount)?;
        if mount.operator != operator {
            return Err(HeavyVehicleError::MountOperatorMismatch {
                expected: mount.operator,
                received: operator,
            });
        }
        if self.turbo.active_seconds > 0.0 {
            return Err(HeavyVehicleError::TurboActive);
        }
        if self.turbo.cooldown_seconds > 0.0 {
            return Err(HeavyVehicleError::TurboCoolingDown);
        }
        let forced_cruise = self.forced_cruise;
        let vehicle = self
            .vehicles
            .get_mut(&mount.vehicle_id)
            .ok_or(HeavyVehicleError::FleetInvariantViolation)?;
        let kind = vehicle.kind();
        if forced_cruise.enabled && kind == VehicleKind::SpaceFighter {
            return Err(HeavyVehicleError::TurboDisabledByForcedCruise);
        }

        self.turbo.active_seconds = TURBO_DURATION_SECONDS;
        self.turbo.cooldown_seconds = TURBO_COOLDOWN_SECONDS;
        vehicle.transform.speed = match kind {
            VehicleKind::Atv => vehicle.transform.speed.max(ATV_TURBO_KICK_FLOOR) + ATV_TURBO_KICK,
            VehicleKind::SpaceFighter => {
                vehicle.transform.speed.max(FIGHTER_TURBO_KICK_FLOOR) + FIGHTER_TURBO_KICK
            }
        };
        Ok(VehicleEffect::TurboStarted {
            vehicle_id: mount.vehicle_id,
            kind,
            duration_seconds: TURBO_DURATION_SECONDS,
            cooldown_seconds: TURBO_COOLDOWN_SECONDS,
        })
    }

    pub fn is_boosting(&self, operator: VehicleOperatorId) -> bool {
        self.active_mount
            .as_ref()
            .is_some_and(|mount| mount.operator == operator)
            && (self.turbo.is_active() || self.input.boost)
    }
}

impl HeavyVehicleFleet {
    /// Advances Ghost Rides, turbo clocks, then the active vehicle in the same
    /// order as Heavy Water.  The mutation is staged and committed only after
    /// all AABBs, probes, camera values, and ground samples validate.
    pub fn tick<F>(
        &mut self,
        input: VehicleTickInput<'_>,
        mut ground_height: F,
    ) -> Result<Vec<VehicleEffect>, HeavyVehicleError>
    where
        F: FnMut(f32, f32, f32) -> f32,
    {
        validate_tick_input(input)?;
        let mut staged = self.clone();
        let mut effects = staged.tick_ghost_rides(
            input.delta_seconds,
            input.walls,
            input.ghost_targets,
            &mut ground_height,
        )?;

        // The source ticks these even when no vehicle is mounted.
        staged.turbo.tick(input.delta_seconds);
        if let Some(mount) = staged.active_mount.clone() {
            let turbo_active = staged.turbo.is_active();
            let controls = staged.input;
            let forced_cruise = staged.forced_cruise;
            let vehicle = staged
                .vehicles
                .get_mut(&mount.vehicle_id)
                .ok_or(HeavyVehicleError::FleetInvariantViolation)?;
            let motion_from = vehicle.transform.position;
            let mut wall_effects = match vehicle.kind() {
                VehicleKind::Atv => step_atv(
                    vehicle,
                    controls,
                    turbo_active,
                    input.camera_yaw,
                    input.delta_seconds,
                    input.walls,
                    &mut ground_height,
                )?,
                VehicleKind::SpaceFighter => step_fighter(
                    vehicle,
                    controls,
                    turbo_active,
                    forced_cruise,
                    input.camera_yaw,
                    input.camera_pitch,
                    input.delta_seconds,
                    input.walls,
                    &mut ground_height,
                )?,
            };
            effects.push(VehicleEffect::KinematicMotionRequested(
                VehicleMotionDescriptor {
                    vehicle_id: vehicle.id.clone(),
                    from: motion_from,
                    to: vehicle.transform.position,
                    linear_velocity: vehicle.transform.velocity,
                    yaw: vehicle.transform.yaw,
                    pitch: vehicle.transform.pitch,
                    roll: vehicle.transform.roll,
                    continuous_collision: true,
                },
            ));
            effects.append(&mut wall_effects);
        }
        staged.validate()?;
        *self = staged;
        Ok(effects)
    }

    fn tick_ghost_rides<F>(
        &mut self,
        delta_seconds: f32,
        walls: &[VehicleAabb],
        targets: &[GhostRideTargetProbe],
        ground_height: &mut F,
    ) -> Result<Vec<VehicleEffect>, HeavyVehicleError>
    where
        F: FnMut(f32, f32, f32) -> f32,
    {
        let ghost_ids: Vec<_> = self
            .vehicles
            .values()
            .filter(|vehicle| matches!(vehicle.lifecycle, VehicleLifecycle::GhostRiding { .. }))
            .map(|vehicle| vehicle.id.clone())
            .collect();
        let mut effects = Vec::new();
        let mut detonated = Vec::new();

        // BTreeMap order replaces the source array's insertion-order scan so
        // simultaneous rides are deterministic across save/load and peers.
        for vehicle_id in ghost_ids {
            let vehicle = self
                .vehicles
                .get_mut(&vehicle_id)
                .ok_or(HeavyVehicleError::FleetInvariantViolation)?;
            let VehicleLifecycle::GhostRiding { mut ghost } = vehicle.lifecycle else {
                return Err(HeavyVehicleError::FleetInvariantViolation);
            };
            let kind = vehicle.kind();
            let start = vehicle.transform.position;
            let travel_seconds = delta_seconds.min(ghost.ttl_seconds);
            let raw_end = start
                + ghost
                    .locked_heading
                    .scaled(ghost.locked_speed * travel_seconds);
            let mut end = raw_end;

            let ground = ground_height(end.x, end.z, end.y);
            if !ground.is_finite() {
                return Err(HeavyVehicleError::InvalidGroundHeight);
            }
            let mut candidates = Vec::new();
            if kind == VehicleKind::Atv {
                let target_y = ground + ATV_GROUND_HEIGHT;
                let smoothing = (travel_seconds * 12.0).min(1.0);
                end.y += (target_y - end.y) * smoothing;
            } else {
                let minimum_y = ground + FIGHTER_MIN_ALTITUDE;
                if end.y < minimum_y {
                    let fraction = crossing_fraction(start.y, end.y, minimum_y);
                    candidates.push(ImpactCandidate::new(
                        fraction,
                        GhostRideTrigger::GroundImpact,
                    ));
                }
            }

            if let Some((fraction, wall)) =
                earliest_wall_hit(start, end, GHOST_RIDE_WALL_RADIUS, walls)
            {
                candidates.push(ImpactCandidate::new(
                    fraction,
                    GhostRideTrigger::WallImpact { wall },
                ));
            }
            for target in targets.iter().copied().filter(|target| target.alive) {
                let radius = target.class.trigger_radius();
                if let Some(fraction) = segment_sphere_hit(start, end, target.position, radius) {
                    candidates.push(ImpactCandidate::new(
                        fraction,
                        GhostRideTrigger::TargetImpact {
                            stable_id: target.stable_id,
                            class: target.class,
                        },
                    ));
                }
            }
            if delta_seconds >= ghost.ttl_seconds {
                candidates.push(ImpactCandidate::new(1.0, GhostRideTrigger::FuseExpired));
            }

            if let Some(impact) = candidates.into_iter().min_by(compare_impact_candidates) {
                let center = start.lerp(end, impact.fraction);
                vehicle.transform.position = center;
                vehicle.transform.velocity = ghost.locked_heading.scaled(ghost.locked_speed);
                vehicle.transform.speed = ghost.locked_speed;
                effects.push(VehicleEffect::GhostRideDetonated(
                    GhostRideDetonationDescriptor::source(
                        vehicle_id.clone(),
                        kind,
                        center,
                        impact.trigger,
                    ),
                ));
                effects.push(VehicleEffect::DespawnRequested {
                    vehicle_id: vehicle_id.clone(),
                });
                detonated.push(vehicle_id);
            } else {
                ghost.ttl_seconds = (ghost.ttl_seconds - delta_seconds).max(0.0);
                vehicle.lifecycle = VehicleLifecycle::GhostRiding { ghost };
                vehicle.transform.position = end;
                vehicle.transform.velocity = ghost.locked_heading.scaled(ghost.locked_speed);
                vehicle.transform.speed = ghost.locked_speed;
                effects.push(VehicleEffect::KinematicMotionRequested(
                    VehicleMotionDescriptor {
                        vehicle_id: vehicle_id.clone(),
                        from: start,
                        to: end,
                        linear_velocity: vehicle.transform.velocity,
                        yaw: vehicle.transform.yaw,
                        pitch: vehicle.transform.pitch,
                        roll: vehicle.transform.roll,
                        continuous_collision: true,
                    },
                ));
            }
        }
        for vehicle_id in detonated {
            self.vehicles.remove(&vehicle_id);
        }
        Ok(effects)
    }
}

fn validate_tick_input(input: VehicleTickInput<'_>) -> Result<(), HeavyVehicleError> {
    if !input.delta_seconds.is_finite() || input.delta_seconds < 0.0 {
        return Err(HeavyVehicleError::InvalidDeltaSeconds);
    }
    if !input.camera_yaw.is_finite() || !input.camera_pitch.is_finite() {
        return Err(HeavyVehicleError::InvalidCamera);
    }
    for wall in input.walls {
        wall.validate()?;
    }
    let mut target_ids = BTreeSet::new();
    for target in input.ghost_targets.iter().copied() {
        target.validate()?;
        if !target_ids.insert(target.stable_id) {
            return Err(HeavyVehicleError::DuplicateGhostTarget(target.stable_id));
        }
    }
    Ok(())
}

fn step_atv<F>(
    vehicle: &mut VehicleRecord,
    input: VehicleInputState,
    turbo_active: bool,
    camera_yaw: f32,
    delta_seconds: f32,
    walls: &[VehicleAabb],
    ground_height: &mut F,
) -> Result<Vec<VehicleEffect>, HeavyVehicleError>
where
    F: FnMut(f32, f32, f32) -> f32,
{
    let transform = &mut vehicle.transform;
    let yaw_delta = shortest_angle_delta(transform.yaw, camera_yaw);
    transform.yaw += yaw_delta * (delta_seconds * 6.0).min(1.0);

    let acceleration = if turbo_active { 38.0 } else { 22.0 };
    let maximum_speed = if turbo_active {
        56.0
    } else if input.boost {
        38.0
    } else {
        24.0
    };
    if input.forward {
        transform.speed += acceleration * delta_seconds;
    }
    if input.back {
        transform.speed -= acceleration * 0.7 * delta_seconds;
    }
    if !input.forward && !input.back {
        approach_zero(&mut transform.speed, 4.0 * delta_seconds);
    }
    transform.speed = transform.speed.clamp(-maximum_speed * 0.4, maximum_speed);

    let strafe = directional_axis(input.left, input.right);
    let forward = forward_heading(VehicleKind::Atv, transform.yaw, 0.0);
    let right = right_heading(transform.yaw);
    transform.velocity = forward.scaled(transform.speed) + right.scaled(strafe * 6.0);
    let movement = move_with_walls(
        transform.position,
        transform.velocity,
        delta_seconds,
        VehicleKind::Atv.active_wall_radius(),
        walls,
    );
    transform.position = movement.position;
    transform.velocity = movement.velocity;
    if !movement.contacts.is_empty() {
        transform.speed *= 0.4;
    }

    let ground = ground_height(
        transform.position.x,
        transform.position.z,
        transform.position.y,
    );
    if !ground.is_finite() {
        return Err(HeavyVehicleError::InvalidGroundHeight);
    }
    let target_y = ground + ATV_GROUND_HEIGHT;
    transform.position.y += (target_y - transform.position.y) * (delta_seconds * 12.0).min(1.0);
    transform.roll += (-strafe * 0.15 - transform.roll) * (delta_seconds * 8.0).min(1.0);
    transform.pitch = 0.0;

    Ok(movement
        .contacts
        .into_iter()
        .map(|wall| VehicleEffect::WallContact {
            vehicle_id: vehicle.id.clone(),
            wall,
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn step_fighter<F>(
    vehicle: &mut VehicleRecord,
    input: VehicleInputState,
    turbo_active: bool,
    forced_cruise: ForcedCruiseState,
    camera_yaw: f32,
    camera_pitch: f32,
    delta_seconds: f32,
    walls: &[VehicleAabb],
    ground_height: &mut F,
) -> Result<Vec<VehicleEffect>, HeavyVehicleError>
where
    F: FnMut(f32, f32, f32) -> f32,
{
    let transform = &mut vehicle.transform;
    let yaw_delta = shortest_angle_delta(transform.yaw, camera_yaw);
    transform.yaw += yaw_delta * (delta_seconds * 4.0).min(1.0);
    transform.pitch += (camera_pitch - transform.pitch) * (delta_seconds * 4.0).min(1.0);

    let acceleration = 30.0;
    if forced_cruise.enabled {
        let maximum_speed = if input.boost {
            forced_cruise.speed * 1.75
        } else {
            forced_cruise.speed
        };
        if input.boost {
            transform.speed += acceleration * delta_seconds;
        }
        transform.speed = transform.speed.clamp(forced_cruise.speed, maximum_speed);
    } else {
        let maximum_speed = if turbo_active {
            140.0
        } else if input.boost {
            95.0
        } else {
            55.0
        };
        let forward_acceleration = if turbo_active {
            acceleration * 1.6
        } else {
            acceleration
        };
        if input.forward {
            transform.speed += forward_acceleration * delta_seconds;
        }
        if input.back {
            transform.speed -= acceleration * 0.6 * delta_seconds;
        }
        if !input.forward && !input.back && !turbo_active {
            approach_zero(&mut transform.speed, 3.0 * delta_seconds);
        }
        transform.speed = transform.speed.clamp(-maximum_speed * 0.4, maximum_speed);
    }

    let strafe = directional_axis(input.left, input.right);
    let forward = forward_heading(VehicleKind::SpaceFighter, transform.yaw, transform.pitch);
    let right = right_heading(transform.yaw);
    transform.velocity = forward.scaled(transform.speed) + right.scaled(strafe * 14.0);
    if input.up {
        transform.velocity.y += 18.0;
    }
    if input.down {
        transform.velocity.y -= 18.0;
    }

    let movement = move_with_walls(
        transform.position,
        transform.velocity,
        delta_seconds,
        VehicleKind::SpaceFighter.active_wall_radius(),
        walls,
    );
    transform.position = movement.position;
    transform.velocity = movement.velocity;
    if !movement.contacts.is_empty() {
        transform.speed *= 0.4;
    }
    let ground = ground_height(
        transform.position.x,
        transform.position.z,
        transform.position.y,
    );
    if !ground.is_finite() {
        return Err(HeavyVehicleError::InvalidGroundHeight);
    }
    transform.position.y = transform
        .position
        .y
        .clamp(ground + FIGHTER_MIN_ALTITUDE, FIGHTER_MAX_ALTITUDE);
    transform.roll += (-strafe * 0.5 - transform.roll) * (delta_seconds * 6.0).min(1.0);

    Ok(movement
        .contacts
        .into_iter()
        .map(|wall| VehicleEffect::WallContact {
            vehicle_id: vehicle.id.clone(),
            wall,
        })
        .collect())
}

fn directional_axis(negative: bool, positive: bool) -> f32 {
    f32::from(positive as u8) - f32::from(negative as u8)
}

fn approach_zero(value: &mut f32, amount: f32) {
    if *value > 0.0 {
        *value = (*value - amount).max(0.0);
    } else if *value < 0.0 {
        *value = (*value + amount).min(0.0);
    }
}

fn shortest_angle_delta(current: f32, target: f32) -> f32 {
    (target - current + PI).rem_euclid(TAU) - PI
}

fn forward_heading(kind: VehicleKind, yaw: f32, pitch: f32) -> VehicleVec3 {
    match kind {
        VehicleKind::Atv => VehicleVec3::new(yaw.sin(), 0.0, yaw.cos()),
        VehicleKind::SpaceFighter => {
            let cosine_pitch = (-pitch).cos();
            let sine_pitch = (-pitch).sin();
            VehicleVec3::new(
                yaw.sin() * cosine_pitch,
                sine_pitch,
                yaw.cos() * cosine_pitch,
            )
        }
    }
}

fn right_heading(yaw: f32) -> VehicleVec3 {
    VehicleVec3::new(yaw.cos(), 0.0, -yaw.sin())
}

#[derive(Debug)]
struct WallMotionOutcome {
    position: VehicleVec3,
    velocity: VehicleVec3,
    contacts: Vec<VehicleAabb>,
}

#[derive(Debug, Clone, Copy)]
enum HorizontalNormal {
    NegativeX,
    PositiveX,
    NegativeZ,
    PositiveZ,
}

impl HorizontalNormal {
    fn vector(self) -> VehicleVec3 {
        match self {
            Self::NegativeX => VehicleVec3::new(-1.0, 0.0, 0.0),
            Self::PositiveX => VehicleVec3::new(1.0, 0.0, 0.0),
            Self::NegativeZ => VehicleVec3::new(0.0, 0.0, -1.0),
            Self::PositiveZ => VehicleVec3::new(0.0, 0.0, 1.0),
        }
    }

    fn zero_component(self, velocity: &mut VehicleVec3) {
        match self {
            Self::NegativeX | Self::PositiveX => velocity.x = 0.0,
            Self::NegativeZ | Self::PositiveZ => velocity.z = 0.0,
        }
    }
}

#[derive(Debug)]
struct WallSweepHit {
    fraction: f32,
    normal: HorizontalNormal,
    wall: VehicleAabb,
}

fn move_with_walls(
    start: VehicleVec3,
    velocity: VehicleVec3,
    delta_seconds: f32,
    radius: f32,
    walls: &[VehicleAabb],
) -> WallMotionOutcome {
    if walls.is_empty() || delta_seconds <= 0.0 {
        return WallMotionOutcome {
            position: start + velocity.scaled(delta_seconds),
            velocity,
            contacts: Vec::new(),
        };
    }
    let mut position = start;
    let mut velocity = velocity;
    let mut seconds_left = delta_seconds;
    let mut contacts = Vec::new();
    // Four planes are enough to settle a point in an axis-aligned corner.
    for _ in 0..4 {
        if seconds_left <= f32::EPSILON
            || (velocity.x.abs() <= f32::EPSILON && velocity.z.abs() <= f32::EPSILON)
        {
            position = position + velocity.scaled(seconds_left);
            break;
        }
        let end = position + velocity.scaled(seconds_left);
        let hit = walls
            .iter()
            .filter_map(|wall| {
                segment_wall_hit(position, end, radius, wall).map(|(fraction, normal)| {
                    WallSweepHit {
                        fraction,
                        normal,
                        wall: wall.clone(),
                    }
                })
            })
            .min_by(compare_wall_sweep_hits);
        let Some(hit) = hit else {
            position = end;
            break;
        };

        position =
            position.lerp(end, hit.fraction) + hit.normal.vector().scaled(VEHICLE_WALL_SEPARATION);
        hit.normal.zero_component(&mut velocity);
        seconds_left *= (1.0 - hit.fraction).max(0.0);
        if !contacts.contains(&hit.wall) {
            contacts.push(hit.wall);
        }
    }
    WallMotionOutcome {
        position,
        velocity,
        contacts,
    }
}

fn compare_wall_sweep_hits(left: &WallSweepHit, right: &WallSweepHit) -> Ordering {
    left.fraction
        .total_cmp(&right.fraction)
        .then_with(|| compare_aabbs(&left.wall, &right.wall))
}

fn compare_aabbs(left: &VehicleAabb, right: &VehicleAabb) -> Ordering {
    [
        left.min.x.total_cmp(&right.min.x),
        left.min.y.total_cmp(&right.min.y),
        left.min.z.total_cmp(&right.min.z),
        left.max.x.total_cmp(&right.max.x),
        left.max.y.total_cmp(&right.max.y),
        left.max.z.total_cmp(&right.max.z),
    ]
    .into_iter()
    .find(|ordering| *ordering != Ordering::Equal)
    .unwrap_or(Ordering::Equal)
}

fn earliest_wall_hit(
    start: VehicleVec3,
    end: VehicleVec3,
    radius: f32,
    walls: &[VehicleAabb],
) -> Option<(f32, VehicleAabb)> {
    walls
        .iter()
        .filter_map(|wall| {
            segment_wall_hit(start, end, radius, wall).map(|(fraction, _)| (fraction, wall.clone()))
        })
        .min_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| compare_aabbs(&left.1, &right.1))
        })
}

/// Sweeps the vehicle center against Heavy Water's horizontally-expanded wall
/// prism.  Unlike the source's final-position overlap, this cannot tunnel
/// through a thin building at high speed.
fn segment_wall_hit(
    start: VehicleVec3,
    end: VehicleVec3,
    radius: f32,
    wall: &VehicleAabb,
) -> Option<(f32, HorizontalNormal)> {
    let bounds = wall.expanded_for_vehicle(radius);
    let delta = end - start;
    let (x_entry, x_exit, x_normal) = slab_interval(
        start.x,
        delta.x,
        bounds.min.x,
        bounds.max.x,
        HorizontalNormal::NegativeX,
        HorizontalNormal::PositiveX,
    )?;
    let (z_entry, z_exit, z_normal) = slab_interval(
        start.z,
        delta.z,
        bounds.min.z,
        bounds.max.z,
        HorizontalNormal::NegativeZ,
        HorizontalNormal::PositiveZ,
    )?;
    let (y_entry, y_exit) = scalar_slab_interval(start.y, delta.y, bounds.min.y, bounds.max.y)?;

    let horizontal_entry = x_entry.max(z_entry);
    let entry = horizontal_entry.max(y_entry).max(0.0);
    let exit = x_exit.min(z_exit).min(y_exit).min(1.0);
    if entry > exit || exit < 0.0 || entry > 1.0 {
        return None;
    }

    let normal = if start.x >= bounds.min.x
        && start.x <= bounds.max.x
        && start.z >= bounds.min.z
        && start.z <= bounds.max.z
    {
        inside_normal(start, delta, &bounds)
    } else if x_entry > z_entry + 0.000_001 {
        x_normal
    } else if z_entry > x_entry + 0.000_001 {
        z_normal
    } else if delta.x.abs() > delta.z.abs() + 0.001 {
        if delta.x > 0.0 {
            HorizontalNormal::NegativeX
        } else {
            HorizontalNormal::PositiveX
        }
    } else if delta.z > 0.0 {
        HorizontalNormal::NegativeZ
    } else {
        HorizontalNormal::PositiveZ
    };
    Some((entry, normal))
}

fn slab_interval(
    start: f32,
    delta: f32,
    minimum: f32,
    maximum: f32,
    minimum_normal: HorizontalNormal,
    maximum_normal: HorizontalNormal,
) -> Option<(f32, f32, HorizontalNormal)> {
    if delta.abs() <= f32::EPSILON {
        return (start >= minimum && start <= maximum).then_some((
            f32::NEG_INFINITY,
            f32::INFINITY,
            minimum_normal,
        ));
    }
    let minimum_t = (minimum - start) / delta;
    let maximum_t = (maximum - start) / delta;
    if minimum_t <= maximum_t {
        Some((minimum_t, maximum_t, minimum_normal))
    } else {
        Some((maximum_t, minimum_t, maximum_normal))
    }
}

fn scalar_slab_interval(start: f32, delta: f32, minimum: f32, maximum: f32) -> Option<(f32, f32)> {
    if delta.abs() <= f32::EPSILON {
        return (start >= minimum && start <= maximum)
            .then_some((f32::NEG_INFINITY, f32::INFINITY));
    }
    let a = (minimum - start) / delta;
    let b = (maximum - start) / delta;
    Some((a.min(b), a.max(b)))
}

fn inside_normal(point: VehicleVec3, delta: VehicleVec3, bounds: &VehicleAabb) -> HorizontalNormal {
    if delta.x.abs() > delta.z.abs() + 0.001 {
        return if delta.x > 0.0 {
            HorizontalNormal::NegativeX
        } else {
            HorizontalNormal::PositiveX
        };
    }
    if delta.z.abs() > delta.x.abs() + 0.001 {
        return if delta.z > 0.0 {
            HorizontalNormal::NegativeZ
        } else {
            HorizontalNormal::PositiveZ
        };
    }
    let distances = [
        (point.x - bounds.min.x, HorizontalNormal::NegativeX),
        (bounds.max.x - point.x, HorizontalNormal::PositiveX),
        (point.z - bounds.min.z, HorizontalNormal::NegativeZ),
        (bounds.max.z - point.z, HorizontalNormal::PositiveZ),
    ];
    distances
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, normal)| normal)
        .unwrap_or(HorizontalNormal::NegativeX)
}

fn segment_sphere_hit(
    start: VehicleVec3,
    end: VehicleVec3,
    center: VehicleVec3,
    radius: f32,
) -> Option<f32> {
    let movement = end - start;
    let relative = start - center;
    let radius_squared = radius * radius;
    if relative.length_squared() <= radius_squared {
        return Some(0.0);
    }
    let a = movement.length_squared();
    if a <= f32::EPSILON {
        return None;
    }
    let b = 2.0 * relative.dot(movement);
    let c = relative.length_squared() - radius_squared;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let first = (-b - root) / (2.0 * a);
    let second = (-b + root) / (2.0 * a);
    if (0.0..=1.0).contains(&first) {
        Some(first)
    } else if (0.0..=1.0).contains(&second) {
        Some(second)
    } else {
        None
    }
}

fn crossing_fraction(start: f32, end: f32, boundary: f32) -> f32 {
    let delta = end - start;
    if delta.abs() <= f32::EPSILON {
        0.0
    } else {
        ((boundary - start) / delta).clamp(0.0, 1.0)
    }
}

#[derive(Debug)]
struct ImpactCandidate {
    fraction: f32,
    trigger: GhostRideTrigger,
}

impl ImpactCandidate {
    fn new(fraction: f32, trigger: GhostRideTrigger) -> Self {
        Self {
            fraction: fraction.clamp(0.0, 1.0),
            trigger,
        }
    }
}

fn compare_impact_candidates(left: &ImpactCandidate, right: &ImpactCandidate) -> Ordering {
    left.fraction
        .total_cmp(&right.fraction)
        .then_with(|| impact_trigger_order(&left.trigger, &right.trigger))
}

fn impact_trigger_order(left: &GhostRideTrigger, right: &GhostRideTrigger) -> Ordering {
    let rank = |trigger: &GhostRideTrigger| match trigger {
        GhostRideTrigger::GroundImpact => 0,
        GhostRideTrigger::WallImpact { .. } => 1,
        GhostRideTrigger::TargetImpact {
            class: GhostRideTargetClass::GroundEnemy,
            ..
        } => 2,
        GhostRideTrigger::TargetImpact {
            class: GhostRideTargetClass::AerialUnit,
            ..
        } => 3,
        GhostRideTrigger::TargetImpact {
            class: GhostRideTargetClass::BaseStructure,
            ..
        } => 4,
        GhostRideTrigger::FuseExpired => 5,
    };
    rank(left)
        .cmp(&rank(right))
        .then_with(|| match (left, right) {
            (
                GhostRideTrigger::TargetImpact {
                    stable_id: left_id, ..
                },
                GhostRideTrigger::TargetImpact {
                    stable_id: right_id,
                    ..
                },
            ) => left_id.cmp(right_id),
            (
                GhostRideTrigger::WallImpact { wall: left_wall },
                GhostRideTrigger::WallImpact { wall: right_wall },
            ) => compare_aabbs(left_wall, right_wall),
            _ => Ordering::Equal,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPERATOR: VehicleOperatorId = VehicleOperatorId(0);

    fn shared() -> VehicleOwnership {
        VehicleOwnership::default()
    }

    fn spawn(
        fleet: &mut HeavyVehicleFleet,
        preset: VehiclePresetId,
        position: VehicleVec3,
    ) -> VehicleId {
        fleet
            .spawn_preset(preset, position, shared())
            .unwrap()
            .vehicle_id
    }

    fn mount(
        fleet: &mut HeavyVehicleFleet,
        preset: VehiclePresetId,
        position: VehicleVec3,
    ) -> VehicleId {
        let id = spawn(fleet, preset, position);
        fleet.mount(&id, OPERATOR).unwrap();
        id
    }

    fn tick(
        fleet: &mut HeavyVehicleFleet,
        delta_seconds: f32,
        camera_yaw: f32,
        camera_pitch: f32,
        walls: &[VehicleAabb],
        targets: &[GhostRideTargetProbe],
        ground: f32,
    ) -> Vec<VehicleEffect> {
        fleet
            .tick(
                VehicleTickInput {
                    delta_seconds,
                    camera_yaw,
                    camera_pitch,
                    walls,
                    ghost_targets: targets,
                },
                |_, _, _| ground,
            )
            .unwrap()
    }

    fn approximate(left: f32, right: f32) {
        assert!(
            (left - right).abs() <= 0.000_1,
            "expected {left} ~= {right}"
        );
    }

    #[test]
    fn executable_catalog_has_only_the_two_kinds_and_four_real_presets() {
        assert_eq!(
            VehicleKind::ALL,
            [VehicleKind::Atv, VehicleKind::SpaceFighter]
        );
        assert_eq!(VehiclePresetId::ALL.len(), 4);
        assert_eq!(VEHICLE_PRESETS.len(), 4);
        assert_eq!(
            serde_json::to_string(&VehicleKind::SpaceFighter).unwrap(),
            "\"spaceFighter\""
        );
        assert_eq!(
            VehiclePresetId::from_legacy_key("RaiderATV"),
            Some(VehiclePresetId::RaiderAtv)
        );
        assert_eq!(VehiclePresetId::from_legacy_key("Tank"), None);

        let raider = VehiclePresetId::RaiderAtv.descriptor();
        assert_eq!(raider.display_name, "Raider ATV");
        assert_eq!(raider.style.body_length, 3.2);
        assert_eq!(raider.style.wheel_radius, Some(0.55));
        assert_eq!(raider.style.wheel_width, Some(0.42));
        assert_eq!(raider.style.wheel_count, Some(4));

        let striker = VehiclePresetId::StrikerAtv.descriptor();
        assert_eq!(striker.style.body_width, 1.7);
        assert_eq!(striker.style.body_height, 0.65);

        let comet = VehiclePresetId::CometFighter.descriptor();
        assert_eq!(comet.display_name, "Comet Fighter");
        assert_eq!(comet.style.wing_span, Some(5.4));
        assert_eq!(comet.style.cannon_count, Some(2));
        assert_eq!(comet.style.cockpit_style, Some(CockpitStyle::Wedge));

        let nebula = VehiclePresetId::NebulaInterceptor.descriptor();
        assert_eq!(nebula.style.body_length, 6.4);
        assert_eq!(nebula.style.wing_span, Some(4.6));
        assert_eq!(nebula.style.cannon_count, Some(4));
        assert_eq!(nebula.style.cockpit_style, Some(CockpitStyle::Bubble));
        for preset in VehiclePresetId::ALL {
            preset.descriptor().style.validate().unwrap();
        }
        let player = VehicleVec3::new(20.0, 300.0, -10.0);
        assert_eq!(
            source_respawn_position(VehicleKind::Atv, player),
            VehicleVec3::new(14.0, 0.6, -14.0)
        );
        assert_eq!(
            source_respawn_position(VehicleKind::SpaceFighter, player),
            VehicleVec3::new(26.0, 1.2, -14.0)
        );
    }

    #[test]
    fn factory_hitboxes_match_authored_dimensions() {
        let atv = VehiclePresetId::RaiderAtv.descriptor().style.hitbox();
        assert_eq!(atv.local_center, VehicleVec3::new(0.0, 0.9, 0.0));
        assert_eq!(atv.half_extents, VehicleVec3::new(1.1, 0.9, 1.6));

        let fighter = VehiclePresetId::CometFighter.descriptor().style.hitbox();
        assert_eq!(fighter.local_center, VehicleVec3::ZERO);
        approximate(fighter.half_extents.x, 2.9);
        approximate(fighter.half_extents.y, 0.75);
        approximate(fighter.half_extents.z, 3.7);
    }

    #[test]
    fn serde_records_are_stable_and_strictly_validated() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = spawn(
            &mut fleet,
            VehiclePresetId::CometFighter,
            VehicleVec3::new(1.0, 2.0, 3.0),
        );
        fleet.mount(&id, OPERATOR).unwrap();
        fleet.validate().unwrap();
        let json = serde_json::to_string(&fleet).unwrap();
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"comet_fighter\""));
        let loaded: HeavyVehicleFleet = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, fleet);
        loaded.validate().unwrap();

        let mut invalid = loaded;
        invalid.schema_version = 99;
        assert_eq!(
            invalid.validate(),
            Err(HeavyVehicleError::UnsupportedSchemaVersion {
                found: 99,
                expected: HEAVY_VEHICLE_SCHEMA_VERSION,
            })
        );
    }

    #[test]
    fn owner_only_mount_and_global_control_lease_are_atomic() {
        let mut fleet = HeavyVehicleFleet::default();
        let private = fleet
            .spawn_preset(
                VehiclePresetId::RaiderAtv,
                VehicleVec3::ZERO,
                VehicleOwnership {
                    owner: Some(VehicleOperatorId(2)),
                    access: VehicleAccess::OwnerOnly,
                },
            )
            .unwrap()
            .vehicle_id;
        assert!(matches!(
            fleet.mount(&private, OPERATOR),
            Err(HeavyVehicleError::VehicleAccessDenied { .. })
        ));
        assert!(fleet.active_mount.is_none());

        fleet.mount(&private, VehicleOperatorId(2)).unwrap();
        let other = spawn(
            &mut fleet,
            VehiclePresetId::CometFighter,
            VehicleVec3::new(10.0, 2.0, 0.0),
        );
        let before = fleet.clone();
        assert!(matches!(
            fleet.mount(&other, VehicleOperatorId(3)),
            Err(HeavyVehicleError::MountAlreadyOwned { .. })
        ));
        assert_eq!(fleet, before);
    }

    #[test]
    fn normal_eject_uses_game_position_and_clears_every_control_latch() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = mount(
            &mut fleet,
            VehiclePresetId::RaiderAtv,
            VehicleVec3::new(7.0, 0.0, 9.0),
        );
        fleet.input = VehicleInputState {
            forward: true,
            boost: true,
            ..Default::default()
        };
        fleet.turbo = VehicleTurboState {
            active_seconds: 0.3,
            cooldown_seconds: 1.2,
        };
        let effect = fleet.eject(OPERATOR).unwrap();
        assert_eq!(
            effect,
            VehicleEffect::Unmounted {
                vehicle_id: id,
                operator: OPERATOR,
                reason: VehicleUnmountReason::Voluntary,
                eject: Some(VehicleEjectDescriptor {
                    position: VehicleVec3::new(9.5, 1.0, 9.0),
                    velocity: VehicleVec3::ZERO,
                    somersault: false,
                }),
            }
        );
        assert_eq!(fleet.input, VehicleInputState::default());
        assert_eq!(fleet.turbo, VehicleTurboState::default());
        assert!(fleet.active_mount.is_none());
        fleet.validate().unwrap();
    }

    #[test]
    fn nearest_mountable_uses_strict_source_range_and_stable_ties() {
        let mut fleet = HeavyVehicleFleet::default();
        let first = spawn(
            &mut fleet,
            VehiclePresetId::RaiderAtv,
            VehicleVec3::new(-3.0, 0.0, 0.0),
        );
        let second = spawn(
            &mut fleet,
            VehiclePresetId::StrikerAtv,
            VehicleVec3::new(3.0, 0.0, 0.0),
        );
        assert!(first < second);
        assert_eq!(
            fleet
                .nearest_mountable(OPERATOR, VehicleVec3::ZERO, DEFAULT_MOUNT_RANGE)
                .unwrap()
                .map(|vehicle| &vehicle.id),
            Some(&first)
        );
        fleet.vehicles.get_mut(&first).unwrap().transform.position.x = -5.5;
        fleet
            .vehicles
            .get_mut(&second)
            .unwrap()
            .transform
            .position
            .x = 5.5;
        assert!(fleet
            .nearest_mountable(OPERATOR, VehicleVec3::ZERO, 5.5)
            .unwrap()
            .is_none());
    }

    #[test]
    fn turbo_kicks_and_clocks_preserve_source_constants() {
        let mut fleet = HeavyVehicleFleet::default();
        let atv = mount(&mut fleet, VehiclePresetId::RaiderAtv, VehicleVec3::ZERO);
        let effect = fleet.trigger_turbo(OPERATOR).unwrap();
        assert!(matches!(effect, VehicleEffect::TurboStarted { .. }));
        assert_eq!(fleet.vehicles[&atv].transform.speed, 40.0);
        assert_eq!(fleet.turbo.active_seconds, 0.7);
        assert_eq!(fleet.turbo.cooldown_seconds, 1.6);
        assert_eq!(
            fleet.trigger_turbo(OPERATOR),
            Err(HeavyVehicleError::TurboActive)
        );
        tick(&mut fleet, 0.7, 0.0, 0.0, &[], &[], 0.0);
        assert_eq!(fleet.turbo.active_seconds, 0.0);
        approximate(fleet.turbo.cooldown_seconds, 0.9);
        assert_eq!(
            fleet.trigger_turbo(OPERATOR),
            Err(HeavyVehicleError::TurboCoolingDown)
        );
        tick(&mut fleet, 0.9, 0.0, 0.0, &[], &[], 0.0);
        fleet.trigger_turbo(OPERATOR).unwrap();
    }

    #[test]
    fn atv_motion_preserves_accel_caps_strafe_grounding_and_camera_steer() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = mount(
            &mut fleet,
            VehiclePresetId::RaiderAtv,
            VehicleVec3::new(0.0, 10.0, 0.0),
        );
        fleet.input.forward = true;
        fleet.input.right = true;
        tick(&mut fleet, 1.0, PI / 2.0, 0.0, &[], &[], 2.0);
        let transform = fleet.vehicles[&id].transform;
        approximate(transform.yaw, PI / 2.0);
        approximate(transform.speed, 22.0);
        approximate(transform.position.x, 22.0);
        approximate(transform.position.z, -6.0);
        approximate(transform.position.y, 2.0);
        approximate(transform.roll, -0.15);
        assert_eq!(transform.pitch, 0.0);

        tick(&mut fleet, 10.0, PI / 2.0, 0.0, &[], &[], 2.0);
        assert_eq!(fleet.vehicles[&id].transform.speed, 24.0);
    }

    #[test]
    fn fighter_motion_preserves_altitudes_and_orbital_cruise_band() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = mount(
            &mut fleet,
            VehiclePresetId::CometFighter,
            VehicleVec3::new(0.0, 599.0, 0.0),
        );
        fleet.input.forward = true;
        fleet.input.up = true;
        tick(&mut fleet, 1.0, 0.0, 0.0, &[], &[], 4.0);
        let transform = fleet.vehicles[&id].transform;
        assert_eq!(transform.speed, 30.0);
        assert_eq!(transform.position.y, FIGHTER_MAX_ALTITUDE);
        approximate(transform.position.z, 30.0);

        fleet.set_forced_cruise(true, Some(28.0)).unwrap();
        fleet.input.forward = false;
        fleet.input.back = true;
        fleet.input.boost = false;
        tick(&mut fleet, 1.0, 0.0, 0.0, &[], &[], 4.0);
        assert_eq!(fleet.vehicles[&id].transform.speed, 28.0);
        fleet.input.boost = true;
        tick(&mut fleet, 1.0, 0.0, 0.0, &[], &[], 4.0);
        assert_eq!(fleet.vehicles[&id].transform.speed, 49.0);
        assert_eq!(
            fleet.trigger_turbo(OPERATOR),
            Err(HeavyVehicleError::TurboDisabledByForcedCruise)
        );
    }

    #[test]
    fn swept_wall_input_prevents_high_speed_tunneling() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = mount(
            &mut fleet,
            VehiclePresetId::CometFighter,
            VehicleVec3::new(0.0, 2.0, 0.0),
        );
        fleet.vehicles.get_mut(&id).unwrap().transform.speed = 140.0;
        fleet.turbo.active_seconds = 0.5;
        fleet.turbo.cooldown_seconds = 1.0;
        let wall = VehicleAabb::new(
            VehicleVec3::new(-2.0, 0.0, 20.0),
            VehicleVec3::new(2.0, 10.0, 20.1),
        );
        let effects = tick(
            &mut fleet,
            0.25,
            0.0,
            0.0,
            std::slice::from_ref(&wall),
            &[],
            0.0,
        );
        let transform = fleet.vehicles[&id].transform;
        assert!(transform.position.z < wall.min.z - 3.0 + 0.01);
        assert_eq!(transform.velocity.z, 0.0);
        approximate(transform.speed, 56.0);
        assert!(matches!(
            effects[0],
            VehicleEffect::KinematicMotionRequested(_)
        ));
        assert_eq!(
            effects[1],
            VehicleEffect::WallContact {
                vehicle_id: id,
                wall,
            }
        );
    }

    #[test]
    fn ghost_ride_locks_heading_speed_and_exact_eject_kinematics() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = mount(
            &mut fleet,
            VehiclePresetId::CometFighter,
            VehicleVec3::new(4.0, 10.0, 8.0),
        );
        {
            let transform = &mut fleet.vehicles.get_mut(&id).unwrap().transform;
            transform.yaw = PI / 2.0;
            transform.pitch = -PI / 6.0;
            transform.speed = 70.0;
        }
        fleet.input.boost = true;
        let effect = fleet.start_ghost_ride(OPERATOR).unwrap();
        let VehicleEffect::GhostRideStarted {
            locked_heading,
            locked_speed,
            eject,
            ..
        } = effect
        else {
            panic!("expected GhostRideStarted");
        };
        approximate(locked_heading.x, (PI / 6.0).cos());
        approximate(locked_heading.y, 0.5);
        approximate(locked_heading.z, 0.0);
        assert_eq!(locked_speed, GHOST_RIDE_SPEED_FIGHTER);
        approximate(eject.position.y, 12.0);
        approximate(eject.velocity.x, 0.0);
        approximate(eject.velocity.y, 8.0);
        approximate(eject.velocity.z, -11.0);
        assert!(eject.somersault);
        assert!(fleet.active_mount.is_none());
        assert_eq!(fleet.input, VehicleInputState::default());
        assert!(matches!(
            fleet.vehicles[&id].lifecycle,
            VehicleLifecycle::GhostRiding { .. }
        ));
    }

    #[test]
    fn ghost_target_sweep_cannot_tunnel_and_returns_shared_explosion_descriptor() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = mount(&mut fleet, VehiclePresetId::RaiderAtv, VehicleVec3::ZERO);
        fleet.input.boost = true;
        fleet.start_ghost_ride(OPERATOR).unwrap();
        let target = GhostRideTargetProbe {
            stable_id: 44,
            class: GhostRideTargetClass::GroundEnemy,
            position: VehicleVec3::new(0.0, 0.0, 30.0),
            alive: true,
        };
        let effects = tick(&mut fleet, 1.0, 0.0, 0.0, &[], &[target], 0.0);
        assert!(!fleet.vehicles.contains_key(&id));
        let VehicleEffect::GhostRideDetonated(detonation) = &effects[0] else {
            panic!("expected detonation");
        };
        assert!(matches!(
            detonation.trigger,
            GhostRideTrigger::TargetImpact {
                stable_id: 44,
                class: GhostRideTargetClass::GroundEnemy
            }
        ));
        approximate(detonation.center.z, 25.5);
        assert_eq!(detonation.visual.radius, 18.0);
        assert_eq!(detonation.radial_damage.base_damage, 420.0);
        assert!(!detonation.radial_damage.damages_players);
        assert_eq!(
            effects[1],
            VehicleEffect::DespawnRequested { vehicle_id: id }
        );
    }

    #[test]
    fn ghost_wall_sweep_and_fuse_are_coarse_step_deterministic() {
        let wall = VehicleAabb::new(
            VehicleVec3::new(-1.0, -1.0, 50.0),
            VehicleVec3::new(1.0, 4.0, 50.05),
        );
        let mut coarse = HeavyVehicleFleet::default();
        mount(&mut coarse, VehiclePresetId::RaiderAtv, VehicleVec3::ZERO);
        coarse.input.boost = true;
        coarse.start_ghost_ride(OPERATOR).unwrap();
        let effects = tick(
            &mut coarse,
            2.0,
            0.0,
            0.0,
            std::slice::from_ref(&wall),
            &[],
            0.0,
        );
        let VehicleEffect::GhostRideDetonated(detonation) = &effects[0] else {
            panic!("expected detonation");
        };
        assert!(matches!(
            detonation.trigger,
            GhostRideTrigger::WallImpact { .. }
        ));
        approximate(detonation.center.z, 47.5);

        let mut fuse = HeavyVehicleFleet::default();
        mount(&mut fuse, VehiclePresetId::RaiderAtv, VehicleVec3::ZERO);
        fuse.input.boost = true;
        fuse.start_ghost_ride(OPERATOR).unwrap();
        let effects = tick(&mut fuse, 10.0, 0.0, 0.0, &[], &[], 0.0);
        let VehicleEffect::GhostRideDetonated(detonation) = &effects[0] else {
            panic!("expected detonation");
        };
        assert_eq!(detonation.trigger, GhostRideTrigger::FuseExpired);
        approximate(detonation.center.z, 360.0);
    }

    #[test]
    fn ghost_damage_descriptor_preserves_all_three_source_falloffs() {
        let damage = GhostRideDamageDescriptor::SOURCE;
        assert_eq!(
            damage.damage_at(GhostRideTargetClass::GroundEnemy, 0.0),
            Some(420.0)
        );
        approximate(
            damage
                .damage_at(GhostRideTargetClass::GroundEnemy, 18.0)
                .unwrap(),
            168.0,
        );
        approximate(
            damage
                .damage_at(GhostRideTargetClass::BaseStructure, 18.0)
                .unwrap(),
            168.0,
        );
        approximate(
            damage
                .damage_at(GhostRideTargetClass::AerialUnit, 0.0)
                .unwrap(),
            357.0,
        );
        approximate(
            damage
                .damage_at(GhostRideTargetClass::AerialUnit, 18.0 * 1.6_f32.sqrt())
                .unwrap(),
            357.0 * 0.35,
        );
        assert!(damage
            .damage_at(GhostRideTargetClass::GroundEnemy, 18.01)
            .is_none());
    }

    #[test]
    fn hp_destruction_forcibly_releases_mount_without_applying_ecs_damage() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = mount(
            &mut fleet,
            VehiclePresetId::StrikerAtv,
            VehicleVec3::new(2.0, 0.0, 3.0),
        );
        let first = fleet.apply_damage(&id, 75.0).unwrap();
        assert_eq!(first.remaining_hp, 125.0);
        assert!(!first.destroyed);
        let destroyed = fleet.apply_damage(&id, 999.0).unwrap();
        assert_eq!(destroyed.remaining_hp, 0.0);
        assert!(destroyed.destroyed);
        assert!(destroyed
            .effects
            .iter()
            .any(|effect| matches!(effect, VehicleEffect::VehicleDestroyed { .. })));
        assert!(fleet.active_mount.is_none());
        assert!(matches!(
            fleet.vehicles[&id].lifecycle,
            VehicleLifecycle::Destroyed
        ));
        fleet.validate().unwrap();
        assert_eq!(
            fleet.mount(&id, OPERATOR),
            Err(HeavyVehicleError::VehicleNotMountable(id.clone()))
        );
        let removed = fleet.remove_destroyed(&id).unwrap();
        assert_eq!(removed, VehicleEffect::DespawnRequested { vehicle_id: id });
    }

    #[test]
    fn invalid_tick_and_damage_are_transactional() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = mount(&mut fleet, VehiclePresetId::RaiderAtv, VehicleVec3::ZERO);
        let before = fleet.clone();
        let error = fleet
            .tick(
                VehicleTickInput {
                    delta_seconds: 1.0,
                    camera_yaw: 0.0,
                    camera_pitch: 0.0,
                    walls: &[],
                    ghost_targets: &[],
                },
                |_, _, _| f32::NAN,
            )
            .unwrap_err();
        assert_eq!(error, HeavyVehicleError::InvalidGroundHeight);
        assert_eq!(fleet, before);
        assert_eq!(
            fleet.apply_damage(&id, f32::NAN),
            Err(HeavyVehicleError::InvalidDamage)
        );
        assert_eq!(fleet, before);
    }

    #[test]
    fn respawn_removes_same_kind_ghosts_but_never_the_driven_kind() {
        let mut fleet = HeavyVehicleFleet::default();
        let ghost = mount(&mut fleet, VehiclePresetId::RaiderAtv, VehicleVec3::ZERO);
        fleet.input.boost = true;
        fleet.start_ghost_ride(OPERATOR).unwrap();
        let outcome = fleet
            .respawn_kind(
                VehicleKind::Atv,
                VehiclePresetId::StrikerAtv,
                VehicleVec3::new(5.0, 0.0, 0.0),
                shared(),
            )
            .unwrap();
        assert_eq!(outcome.removed, vec![ghost]);
        assert_eq!(fleet.vehicles.len(), 1);

        let new_id = outcome.spawned.vehicle_id;
        fleet.mount(&new_id, OPERATOR).unwrap();
        let before = fleet.clone();
        assert!(matches!(
            fleet.respawn_kind(
                VehicleKind::Atv,
                VehiclePresetId::RaiderAtv,
                VehicleVec3::ZERO,
                shared(),
            ),
            Err(HeavyVehicleError::MountAlreadyOwned { .. })
        ));
        assert_eq!(fleet, before);
    }

    #[test]
    fn orbital_spawn_mount_cruise_and_teardown_keep_source_order() {
        let mut fleet = HeavyVehicleFleet::default();
        let outcome = fleet
            .spawn_and_mount_orbital_fighter(OPERATOR, VehicleVec3::new(12.0, 60.0, -8.0))
            .unwrap();
        let record = &fleet.vehicles[&outcome.vehicle_id];
        assert_eq!(record.preset, VehiclePresetId::CometFighter);
        assert_eq!(
            record.transform.position,
            VehicleVec3::new(12.0, 300.0, -4.0)
        );
        assert_eq!(record.transform.speed, 28.0);
        assert_eq!(outcome.effects.len(), 3);
        assert!(matches!(
            outcome.effects[0],
            VehicleEffect::SpawnRequested { .. }
        ));
        assert!(matches!(outcome.effects[1], VehicleEffect::Mounted { .. }));
        assert_eq!(
            outcome.effects[2],
            VehicleEffect::ForcedCruiseChanged {
                enabled: true,
                speed: 28.0,
            }
        );

        let teardown = fleet.teardown_orbital_fighter(&outcome.vehicle_id).unwrap();
        assert_eq!(
            teardown[0],
            VehicleEffect::ForcedCruiseChanged {
                enabled: false,
                speed: 55.0,
            }
        );
        assert!(matches!(
            teardown[1],
            VehicleEffect::Unmounted {
                reason: VehicleUnmountReason::OrbitalTeardown,
                eject: None,
                ..
            }
        ));
        assert_eq!(
            teardown[2],
            VehicleEffect::DespawnRequested {
                vehicle_id: outcome.vehicle_id.clone(),
            }
        );
        assert!(!fleet.vehicles.contains_key(&outcome.vehicle_id));
        assert!(fleet.active_mount.is_none());
        fleet.validate().unwrap();
    }

    #[test]
    fn ghost_probe_validation_is_stable_and_rejects_duplicate_ids() {
        let mut fleet = HeavyVehicleFleet::default();
        let probe = GhostRideTargetProbe {
            stable_id: 9,
            class: GhostRideTargetClass::GroundEnemy,
            position: VehicleVec3::ZERO,
            alive: true,
        };
        let before = fleet.clone();
        let error = fleet
            .tick(
                VehicleTickInput {
                    delta_seconds: 0.1,
                    camera_yaw: 0.0,
                    camera_pitch: 0.0,
                    walls: &[],
                    ghost_targets: &[probe, probe],
                },
                |_, _, _| 0.0,
            )
            .unwrap_err();
        assert_eq!(error, HeavyVehicleError::DuplicateGhostTarget(9));
        assert_eq!(fleet, before);
    }

    #[test]
    fn id_generation_wraps_without_hanging_or_overwriting() {
        let mut fleet = HeavyVehicleFleet {
            next_serial: u64::MAX,
            ..Default::default()
        };
        let maximum = spawn(&mut fleet, VehiclePresetId::RaiderAtv, VehicleVec3::ZERO);
        assert_eq!(maximum.as_str(), "vehicle_18446744073709551615");
        fleet.next_serial = u64::MAX;
        let wrapped = spawn(
            &mut fleet,
            VehiclePresetId::StrikerAtv,
            VehicleVec3::new(2.0, 0.0, 0.0),
        );
        assert_eq!(wrapped.as_str(), "vehicle_0");
        assert_eq!(fleet.vehicles.len(), 2);
        fleet.validate().unwrap();
    }

    #[test]
    fn fighter_pitch_direction_and_soft_floor_match_source() {
        let mut fleet = HeavyVehicleFleet::default();
        let id = mount(
            &mut fleet,
            VehiclePresetId::NebulaInterceptor,
            VehicleVec3::new(0.0, 20.0, 0.0),
        );
        fleet.input.forward = true;
        tick(&mut fleet, 1.0, 0.0, -PI / 6.0, &[], &[], 0.0);
        let transform = fleet.vehicles[&id].transform;
        approximate(transform.pitch, -PI / 6.0);
        approximate(transform.velocity.y, 15.0);
        approximate(transform.position.y, 35.0);

        fleet.input.forward = false;
        fleet.input.down = true;
        fleet.vehicles.get_mut(&id).unwrap().transform.position.y = -100.0;
        tick(&mut fleet, 0.1, 0.0, 0.0, &[], &[], 10.0);
        assert_eq!(fleet.vehicles[&id].transform.position.y, 11.0);
    }

    #[test]
    fn serialized_ghost_ride_resumes_its_remaining_fuse_deterministically() {
        let mut fleet = HeavyVehicleFleet::default();
        mount(&mut fleet, VehiclePresetId::RaiderAtv, VehicleVec3::ZERO);
        fleet.input.boost = true;
        fleet.start_ghost_ride(OPERATOR).unwrap();
        let first = tick(&mut fleet, 2.0, 0.0, 0.0, &[], &[], 0.0);
        assert!(matches!(
            first[0],
            VehicleEffect::KinematicMotionRequested(_)
        ));
        let json = serde_json::to_string(&fleet).unwrap();
        let mut resumed: HeavyVehicleFleet = serde_json::from_str(&json).unwrap();
        resumed.validate().unwrap();
        let effects = tick(&mut resumed, 4.0, 0.0, 0.0, &[], &[], 0.0);
        let VehicleEffect::GhostRideDetonated(detonation) = &effects[0] else {
            panic!("expected resumed fuse detonation");
        };
        assert_eq!(detonation.trigger, GhostRideTrigger::FuseExpired);
        approximate(detonation.center.z, 360.0);
        assert_eq!(detonation.visual.color, VehicleRgb::new(1.0, 0.55, 0.18));
        assert_eq!(detonation.visual.camera_shake_intensity, 0.7);
        assert_eq!(detonation.visual.camera_shake_duration_seconds, 0.45);
        assert_eq!(detonation.audio.legacy_url, "/sounds/hit.mp3");
        assert_eq!(detonation.audio.volume, 1.0);
        assert_eq!(detonation.audio.playback_rate, 0.5);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleInputPatch {
    pub forward: Option<bool>,
    pub back: Option<bool>,
    pub left: Option<bool>,
    pub right: Option<bool>,
    pub up: Option<bool>,
    pub down: Option<bool>,
    pub boost: Option<bool>,
}

impl HeavyVehicleFleet {
    pub fn start_ghost_ride(
        &mut self,
        operator: VehicleOperatorId,
    ) -> Result<VehicleEffect, HeavyVehicleError> {
        let mount = self
            .active_mount
            .clone()
            .ok_or(HeavyVehicleError::NoActiveMount)?;
        if mount.operator != operator {
            return Err(HeavyVehicleError::MountOperatorMismatch {
                expected: mount.operator,
                received: operator,
            });
        }
        if !self.is_boosting(operator) {
            return Err(HeavyVehicleError::GhostRideRequiresBoost);
        }
        let vehicle = self
            .vehicles
            .get_mut(&mount.vehicle_id)
            .ok_or(HeavyVehicleError::FleetInvariantViolation)?;
        if !matches!(vehicle.lifecycle, VehicleLifecycle::Available) {
            return Err(HeavyVehicleError::VehicleNotMountable(
                mount.vehicle_id.clone(),
            ));
        }

        let transform = vehicle.transform;
        let kind = vehicle.kind();
        let heading = forward_heading(kind, transform.yaw, transform.pitch)
            .normalized()
            .ok_or(HeavyVehicleError::InvalidTransform)?;
        let locked_speed = transform.speed.abs().max(kind.ghost_ride_speed_floor());
        let ghost = GhostRideState {
            locked_heading: heading,
            locked_speed,
            ttl_seconds: GHOST_RIDE_TTL_SECONDS,
        };
        ghost.validate(kind)?;
        vehicle.lifecycle = VehicleLifecycle::GhostRiding { ghost };
        vehicle.transform.velocity = heading.scaled(locked_speed);
        vehicle.transform.speed = locked_speed;
        self.active_mount = None;
        self.input = VehicleInputState::default();
        self.turbo.clear();

        Ok(VehicleEffect::GhostRideStarted {
            vehicle_id: mount.vehicle_id,
            operator,
            locked_heading: heading,
            locked_speed,
            ttl_seconds: GHOST_RIDE_TTL_SECONDS,
            eject: VehicleEjectDescriptor::ghost_ride_from(transform),
        })
    }

    pub fn apply_damage(
        &mut self,
        vehicle_id: &VehicleId,
        amount: f32,
    ) -> Result<VehicleDamageOutcome, HeavyVehicleError> {
        if !amount.is_finite() || amount < 0.0 {
            return Err(HeavyVehicleError::InvalidDamage);
        }
        let mut staged = self.clone();
        let vehicle = staged
            .vehicles
            .get_mut(vehicle_id)
            .ok_or_else(|| HeavyVehicleError::UnknownVehicle(vehicle_id.clone()))?;
        if matches!(vehicle.lifecycle, VehicleLifecycle::Destroyed) {
            return Ok(VehicleDamageOutcome {
                remaining_hp: 0.0,
                destroyed: true,
                effects: Vec::new(),
            });
        }

        vehicle.health.hp = (vehicle.health.hp - amount).max(0.0);
        let actual_amount = (self
            .vehicles
            .get(vehicle_id)
            .ok_or(HeavyVehicleError::FleetInvariantViolation)?
            .health
            .hp
            - vehicle.health.hp)
            .max(0.0);
        let mut effects = vec![VehicleEffect::VehicleDamaged {
            vehicle_id: vehicle_id.clone(),
            amount: actual_amount,
            remaining_hp: vehicle.health.hp,
        }];
        let destroyed = vehicle.health.hp <= 0.0;
        let remaining_hp = vehicle.health.hp;
        if destroyed {
            vehicle.health.hp = 0.0;
            vehicle.lifecycle = VehicleLifecycle::Destroyed;
            vehicle.transform.velocity = VehicleVec3::ZERO;
            vehicle.transform.speed = 0.0;
            if staged
                .active_mount
                .as_ref()
                .is_some_and(|mount| mount.vehicle_id == *vehicle_id)
            {
                let mount = staged
                    .active_mount
                    .take()
                    .ok_or(HeavyVehicleError::FleetInvariantViolation)?;
                staged.input = VehicleInputState::default();
                staged.turbo.clear();
                effects.push(VehicleEffect::Unmounted {
                    vehicle_id: vehicle_id.clone(),
                    operator: mount.operator,
                    reason: VehicleUnmountReason::Destroyed,
                    eject: Some(VehicleEjectDescriptor::normal_from(vehicle.transform)),
                });
            }
            effects.push(VehicleEffect::VehicleDestroyed {
                vehicle_id: vehicle_id.clone(),
                position: vehicle.transform.position,
            });
        }
        staged.validate()?;
        *self = staged;
        Ok(VehicleDamageOutcome {
            remaining_hp,
            destroyed,
            effects,
        })
    }

    pub fn despawn(
        &mut self,
        vehicle_id: &VehicleId,
        reason: VehicleUnmountReason,
    ) -> Result<Vec<VehicleEffect>, HeavyVehicleError> {
        let record = self
            .vehicles
            .get(vehicle_id)
            .cloned()
            .ok_or_else(|| HeavyVehicleError::UnknownVehicle(vehicle_id.clone()))?;
        let mut effects = Vec::new();
        if let Some(mount) = self.active_mount.clone() {
            if mount.vehicle_id == *vehicle_id {
                self.active_mount = None;
                self.input = VehicleInputState::default();
                self.turbo.clear();
                effects.push(VehicleEffect::Unmounted {
                    vehicle_id: vehicle_id.clone(),
                    operator: mount.operator,
                    reason,
                    eject: None,
                });
            }
        }
        self.vehicles.remove(vehicle_id);
        effects.push(VehicleEffect::DespawnRequested {
            vehicle_id: record.id,
        });
        Ok(effects)
    }

    pub fn remove_destroyed(
        &mut self,
        vehicle_id: &VehicleId,
    ) -> Result<VehicleEffect, HeavyVehicleError> {
        let vehicle = self
            .vehicles
            .get(vehicle_id)
            .ok_or_else(|| HeavyVehicleError::UnknownVehicle(vehicle_id.clone()))?;
        if !matches!(vehicle.lifecycle, VehicleLifecycle::Destroyed) {
            return Err(HeavyVehicleError::VehicleNotDestroyed(vehicle_id.clone()));
        }
        self.vehicles.remove(vehicle_id);
        Ok(VehicleEffect::DespawnRequested {
            vehicle_id: vehicle_id.clone(),
        })
    }

    pub fn respawn_kind(
        &mut self,
        kind: VehicleKind,
        preset: VehiclePresetId,
        position: VehicleVec3,
        ownership: VehicleOwnership,
    ) -> Result<VehicleRespawnOutcome, HeavyVehicleError> {
        if preset.kind() != kind {
            return Err(HeavyVehicleError::PresetKindMismatch {
                requested: kind,
                preset,
            });
        }
        if self
            .active_mount
            .as_ref()
            .and_then(|mount| self.vehicles.get(&mount.vehicle_id))
            .is_some_and(|vehicle| vehicle.kind() == kind)
        {
            let mount = self
                .active_mount
                .as_ref()
                .ok_or(HeavyVehicleError::FleetInvariantViolation)?;
            return Err(HeavyVehicleError::MountAlreadyOwned {
                vehicle_id: mount.vehicle_id.clone(),
                operator: mount.operator,
            });
        }

        let mut staged = self.clone();
        let removed: Vec<_> = staged
            .vehicles
            .values()
            .filter(|vehicle| vehicle.kind() == kind)
            .map(|vehicle| vehicle.id.clone())
            .collect();
        let mut effects = Vec::with_capacity(removed.len() + 1);
        for vehicle_id in &removed {
            staged.vehicles.remove(vehicle_id);
            effects.push(VehicleEffect::DespawnRequested {
                vehicle_id: vehicle_id.clone(),
            });
        }
        let spawned = staged.spawn_preset(preset, position, ownership)?;
        effects.push(spawned.effect.clone());
        staged.validate()?;
        *self = staged;
        Ok(VehicleRespawnOutcome {
            removed,
            spawned,
            effects,
        })
    }

    /// Atomic counterpart to `SpaceLevelSystem.spawnAndEnterFighter()`.
    pub fn spawn_and_mount_orbital_fighter(
        &mut self,
        operator: VehicleOperatorId,
        player_position: VehicleVec3,
    ) -> Result<OrbitalFighterMountOutcome, HeavyVehicleError> {
        if !player_position.is_finite() {
            return Err(HeavyVehicleError::InvalidTransform);
        }
        let mut staged = self.clone();
        let spawn_position = VehicleVec3::new(
            player_position.x,
            ORBITAL_SPAWN_ALTITUDE,
            player_position.z + ORBITAL_SPAWN_FORWARD_OFFSET,
        );
        let spawned = staged.spawn_preset(
            VehiclePresetId::CometFighter,
            spawn_position,
            VehicleOwnership::default(),
        )?;
        let mounted = staged.mount(&spawned.vehicle_id, operator)?;
        let cruise = staged.set_forced_cruise(true, Some(ORBITAL_FORCED_CRUISE_SPEED))?;
        staged.validate()?;
        let vehicle_id = spawned.vehicle_id.clone();
        let effects = vec![spawned.effect, mounted, cruise];
        *self = staged;
        Ok(OrbitalFighterMountOutcome {
            vehicle_id,
            effects,
        })
    }

    /// Preserves SpaceLevelSystem teardown order: release cruise, clear the
    /// player's mount, then remove the ephemeral orbital fighter.
    pub fn teardown_orbital_fighter(
        &mut self,
        vehicle_id: &VehicleId,
    ) -> Result<Vec<VehicleEffect>, HeavyVehicleError> {
        let mut staged = self.clone();
        let mut effects = vec![staged.set_forced_cruise(false, None)?];
        if let Some(mount) = staged.active_mount.clone() {
            if mount.vehicle_id == *vehicle_id {
                effects.push(staged.eject_with_reason(
                    mount.operator,
                    VehicleUnmountReason::OrbitalTeardown,
                    false,
                )?);
            }
        }
        effects.extend(staged.despawn(vehicle_id, VehicleUnmountReason::OrbitalTeardown)?);
        staged.validate()?;
        *self = staged;
        Ok(effects)
    }
}
