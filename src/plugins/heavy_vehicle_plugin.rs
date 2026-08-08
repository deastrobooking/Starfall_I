//! Save-backed runtime seam for Heavy Water's independent ATV and fighter.
//!
//! [`HeavyWaterProgress::vehicles`] is the durable rule authority. This plugin
//! binds local players to stable [`VehicleOperatorId`] values, applies typed
//! vehicle requests transactionally, and advances the engine-neutral fleet on
//! `FixedUpdate`. It deliberately never writes `Transform`, Avian bodies,
//! [`PlayerInput`](crate::components::player::PlayerInput), Starfall's
//! [`VehicleState`](crate::plugins::vehicle_plugin::VehicleState), or combat
//! health. Instead it emits [`HeavyVehicleEcsCommand`] descriptors.
//!
//! Consumer contract:
//!
//! - spawn/despawn commands own the stable `VehicleId -> Entity` presentation
//!   map;
//! - mount/eject/Ghost Ride commands may drive camera and avatar presentation,
//!   but a gameplay consumer must arbitrate against the existing
//!   `VehiclePlugin` before granting either motor ownership;
//! - motion commands are kinematic intent for the chosen Starfall/Avian motor,
//!   never a second transform writer;
//! - Ghost Ride detonation commands contain visual/audio and radial-damage
//!   descriptors, but only Starfall's combat pipeline may resolve that damage.
//!
//! This boundary lets the source-accurate Heavy Water simulation remain
//! deterministic and serializable while Starfall physics and damage stay
//! authoritative.

#![allow(dead_code)] // Public ECS/physics/combat seams land incrementally.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;

use crate::components::player::{Player, PlayerIndex};
use crate::engine::state::AppState;
use crate::world::heavy_vehicle::{
    GhostRideDetonationDescriptor, HeavyVehicleError, HeavyVehicleFleet, VehicleAabb,
    VehicleAccess, VehicleEffect, VehicleEjectDescriptor, VehicleHitboxDescriptor, VehicleId,
    VehicleInputState, VehicleKind, VehicleMotionDescriptor, VehicleOperatorId, VehicleOwnership,
    VehiclePresetId, VehicleTickInput, VehicleUnmountReason, VehicleVec3,
};
use crate::world::heavy_water::HeavyWaterProgress;

pub const HEAVY_LOCAL_VEHICLE_OPERATOR_COUNT: u8 = 4;

/// Stable local-player binding. Entity identities never enter saved state.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyVehicleOperator(pub VehicleOperatorId);

/// Starfall reserves Heavy Water operators 0..=3 for local party slots.
pub const fn heavy_vehicle_operator_for_player_index(
    player_index: u8,
) -> Option<VehicleOperatorId> {
    if player_index < HEAVY_LOCAL_VEHICLE_OPERATOR_COUNT {
        Some(VehicleOperatorId(player_index))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeavyVehicleCameraProbe {
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for HeavyVehicleCameraProbe {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

/// A deterministic, engine-neutral terrain sample supplied by a world bridge.
///
/// Overlaps select the greatest `priority`, then the lowest `stable_id`; vector
/// insertion order therefore cannot change a fixed-tick result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeavyVehicleGroundSurfaceProbe {
    pub stable_id: u64,
    pub priority: i16,
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
    pub height: f32,
}

impl HeavyVehicleGroundSurfaceProbe {
    pub const fn new(
        stable_id: u64,
        priority: i16,
        min_x: f32,
        max_x: f32,
        min_z: f32,
        max_z: f32,
        height: f32,
    ) -> Self {
        Self {
            stable_id,
            priority,
            min_x,
            max_x,
            min_z,
            max_z,
            height,
        }
    }

    fn contains(self, x: f32, z: f32) -> bool {
        (self.min_x..=self.max_x).contains(&x) && (self.min_z..=self.max_z).contains(&z)
    }

    fn is_valid(self) -> bool {
        [self.min_x, self.max_x, self.min_z, self.max_z, self.height]
            .into_iter()
            .all(f32::is_finite)
            && self.min_x <= self.max_x
            && self.min_z <= self.max_z
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeavyVehicleGroundProbe {
    pub fallback_height: f32,
    pub surfaces: Vec<HeavyVehicleGroundSurfaceProbe>,
}

impl Default for HeavyVehicleGroundProbe {
    fn default() -> Self {
        Self {
            fallback_height: 0.0,
            surfaces: Vec::new(),
        }
    }
}

impl HeavyVehicleGroundProbe {
    pub fn validate(&self) -> Result<(), HeavyVehicleProbeError> {
        if !self.fallback_height.is_finite() {
            return Err(HeavyVehicleProbeError::InvalidFallbackGroundHeight);
        }
        let mut ids = BTreeSet::new();
        for surface in &self.surfaces {
            if !surface.is_valid() {
                return Err(HeavyVehicleProbeError::InvalidGroundSurface(
                    surface.stable_id,
                ));
            }
            if !ids.insert(surface.stable_id) {
                return Err(HeavyVehicleProbeError::DuplicateGroundSurface(
                    surface.stable_id,
                ));
            }
        }
        Ok(())
    }

    pub fn sample(&self, x: f32, z: f32) -> f32 {
        self.surfaces
            .iter()
            .copied()
            .filter(|surface| surface.contains(x, z))
            .max_by(|left, right| {
                left.priority
                    .cmp(&right.priority)
                    .then_with(|| right.stable_id.cmp(&left.stable_id))
            })
            .map_or(self.fallback_height, |surface| surface.height)
    }
}

/// Read-only world/input snapshot consumed by the next Heavy vehicle tick.
///
/// The camera, controls, walls, targets, and terrain are plain descriptors; no
/// Bevy or Avian entity is retained in persistent state. A world integration
/// should refresh this resource before `FixedUpdate`.
#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct HeavyVehicleFixedProbes {
    pub controls_by_operator: BTreeMap<VehicleOperatorId, VehicleInputState>,
    pub camera_by_operator: BTreeMap<VehicleOperatorId, HeavyVehicleCameraProbe>,
    pub walls: Vec<VehicleAabb>,
    pub ghost_targets: Vec<crate::world::heavy_vehicle::GhostRideTargetProbe>,
    pub ground: HeavyVehicleGroundProbe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeavyVehicleProbeError {
    InvalidFallbackGroundHeight,
    InvalidGroundSurface(u64),
    DuplicateGroundSurface(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeavyVehicleTickFailure {
    OperatorNotBound(VehicleOperatorId),
    AmbiguousOperatorBinding(VehicleOperatorId),
    MissingCamera(VehicleOperatorId),
    Probe(HeavyVehicleProbeError),
    Domain(HeavyVehicleError),
}

impl From<HeavyVehicleProbeError> for HeavyVehicleTickFailure {
    fn from(value: HeavyVehicleProbeError) -> Self {
        Self::Probe(value)
    }
}

impl From<HeavyVehicleError> for HeavyVehicleTickFailure {
    fn from(value: HeavyVehicleError) -> Self {
        Self::Domain(value)
    }
}

/// Diagnostic state only; the durable fleet lives in `HeavyWaterProgress`.
#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct HeavyVehicleRuntimeState {
    pub bound_operators: BTreeSet<VehicleOperatorId>,
    pub duplicate_operators: BTreeSet<VehicleOperatorId>,
    pub rejected_player_indices: BTreeSet<u8>,
    pub fixed_ticks_attempted: u64,
    pub fixed_ticks_completed: u64,
    pub last_tick_failure: Option<HeavyVehicleTickFailure>,
}

impl HeavyVehicleRuntimeState {
    fn operator_failure(&self, operator: VehicleOperatorId) -> Option<HeavyVehicleActionFailure> {
        if self.duplicate_operators.contains(&operator) {
            Some(HeavyVehicleActionFailure::AmbiguousOperatorBinding(
                operator,
            ))
        } else if !self.bound_operators.contains(&operator) {
            Some(HeavyVehicleActionFailure::OperatorNotBound(operator))
        } else {
            None
        }
    }

    fn tick_operator_failure(
        &self,
        operator: VehicleOperatorId,
    ) -> Option<HeavyVehicleTickFailure> {
        if self.duplicate_operators.contains(&operator) {
            Some(HeavyVehicleTickFailure::AmbiguousOperatorBinding(operator))
        } else if !self.bound_operators.contains(&operator) {
            Some(HeavyVehicleTickFailure::OperatorNotBound(operator))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyVehicleActionKind {
    Spawn,
    Mount,
    Eject,
    Turbo,
    GhostRide,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeavyVehicleActionFailure {
    InvalidOperator(VehicleOperatorId),
    OperatorNotBound(VehicleOperatorId),
    AmbiguousOperatorBinding(VehicleOperatorId),
    NoMountableVehicle {
        operator: VehicleOperatorId,
        position: VehicleVec3,
        maximum_range: f32,
    },
    Domain(HeavyVehicleError),
}

impl From<HeavyVehicleError> for HeavyVehicleActionFailure {
    fn from(value: HeavyVehicleError) -> Self {
        Self::Domain(value)
    }
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyVehicleSpawnRequest {
    pub request_id: u64,
    pub preset: VehiclePresetId,
    pub position: VehicleVec3,
    pub ownership: VehicleOwnership,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeavyVehicleSpawnSuccess {
    pub vehicle_id: VehicleId,
    pub preset: VehiclePresetId,
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyVehicleSpawnOutcome {
    pub request_id: u64,
    pub result: Result<HeavyVehicleSpawnSuccess, HeavyVehicleActionFailure>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeavyVehicleMountTarget {
    Vehicle(VehicleId),
    Nearest {
        operator_position: VehicleVec3,
        maximum_range: f32,
    },
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyVehicleMountRequest {
    pub request_id: u64,
    pub operator: VehicleOperatorId,
    pub target: HeavyVehicleMountTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeavyVehicleMountSuccess {
    pub vehicle_id: VehicleId,
    pub operator: VehicleOperatorId,
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyVehicleMountOutcome {
    pub request_id: u64,
    pub result: Result<HeavyVehicleMountSuccess, HeavyVehicleActionFailure>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyVehicleEjectRequest {
    pub request_id: u64,
    pub operator: VehicleOperatorId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeavyVehicleEjectSuccess {
    pub vehicle_id: VehicleId,
    pub operator: VehicleOperatorId,
    pub eject: Option<VehicleEjectDescriptor>,
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyVehicleEjectOutcome {
    pub request_id: u64,
    pub result: Result<HeavyVehicleEjectSuccess, HeavyVehicleActionFailure>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyVehicleTurboRequest {
    pub request_id: u64,
    pub operator: VehicleOperatorId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeavyVehicleTurboSuccess {
    pub vehicle_id: VehicleId,
    pub kind: VehicleKind,
    pub duration_seconds: f32,
    pub cooldown_seconds: f32,
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyVehicleTurboOutcome {
    pub request_id: u64,
    pub result: Result<HeavyVehicleTurboSuccess, HeavyVehicleActionFailure>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyVehicleGhostRideRequest {
    pub request_id: u64,
    pub operator: VehicleOperatorId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeavyVehicleGhostRideSuccess {
    pub vehicle_id: VehicleId,
    pub operator: VehicleOperatorId,
    pub locked_heading: VehicleVec3,
    pub locked_speed: f32,
    pub ttl_seconds: f32,
    pub eject: VehicleEjectDescriptor,
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyVehicleGhostRideOutcome {
    pub request_id: u64,
    pub result: Result<HeavyVehicleGhostRideSuccess, HeavyVehicleActionFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyVehicleCommandOrigin {
    Request {
        request_id: u64,
        action: HeavyVehicleActionKind,
    },
    FixedTick(u64),
}

/// Typed work for presentation, motor, and combat consumers.
///
/// None of these variants is executed by this adapter. In particular,
/// `MotionIntent` does not write transforms and `GhostRideDetonationPresentation`
/// does not apply its `radial_damage` payload.
#[derive(Debug, Clone, PartialEq)]
pub enum HeavyVehicleEcsCommandKind {
    SpawnPresentation {
        vehicle_id: VehicleId,
        preset: VehiclePresetId,
        position: VehicleVec3,
        hitbox: VehicleHitboxDescriptor,
    },
    DespawnPresentation {
        vehicle_id: VehicleId,
    },
    MountedPresentation {
        vehicle_id: VehicleId,
        operator: VehicleOperatorId,
    },
    UnmountedPresentation {
        vehicle_id: VehicleId,
        operator: VehicleOperatorId,
        reason: VehicleUnmountReason,
        eject: Option<VehicleEjectDescriptor>,
    },
    TurboPresentation {
        vehicle_id: VehicleId,
        kind: VehicleKind,
        duration_seconds: f32,
        cooldown_seconds: f32,
    },
    ForcedCruisePresentation {
        enabled: bool,
        speed: f32,
    },
    MotionIntent(VehicleMotionDescriptor),
    WallContactPresentation {
        vehicle_id: VehicleId,
        wall: VehicleAabb,
    },
    DamagePresentation {
        vehicle_id: VehicleId,
        amount: f32,
        remaining_hp: f32,
    },
    DestroyedPresentation {
        vehicle_id: VehicleId,
        position: VehicleVec3,
    },
    GhostRideStartedPresentation {
        vehicle_id: VehicleId,
        operator: VehicleOperatorId,
        locked_heading: VehicleVec3,
        locked_speed: f32,
        ttl_seconds: f32,
        eject: VehicleEjectDescriptor,
    },
    GhostRideDetonationPresentation(Box<GhostRideDetonationDescriptor>),
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyVehicleEcsCommand {
    pub origin: HeavyVehicleCommandOrigin,
    pub command: HeavyVehicleEcsCommandKind,
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyVehicleAdapterFault {
    pub fixed_tick: u64,
    pub failure: HeavyVehicleTickFailure,
}

/// Convert a domain effect into an inert consumer descriptor.
pub fn heavy_vehicle_ecs_command(
    origin: HeavyVehicleCommandOrigin,
    effect: VehicleEffect,
) -> HeavyVehicleEcsCommand {
    let command = match effect {
        VehicleEffect::SpawnRequested {
            vehicle_id,
            preset,
            position,
            hitbox,
        } => HeavyVehicleEcsCommandKind::SpawnPresentation {
            vehicle_id,
            preset,
            position,
            hitbox,
        },
        VehicleEffect::DespawnRequested { vehicle_id } => {
            HeavyVehicleEcsCommandKind::DespawnPresentation { vehicle_id }
        }
        VehicleEffect::Mounted {
            vehicle_id,
            operator,
        } => HeavyVehicleEcsCommandKind::MountedPresentation {
            vehicle_id,
            operator,
        },
        VehicleEffect::Unmounted {
            vehicle_id,
            operator,
            reason,
            eject,
        } => HeavyVehicleEcsCommandKind::UnmountedPresentation {
            vehicle_id,
            operator,
            reason,
            eject,
        },
        VehicleEffect::TurboStarted {
            vehicle_id,
            kind,
            duration_seconds,
            cooldown_seconds,
        } => HeavyVehicleEcsCommandKind::TurboPresentation {
            vehicle_id,
            kind,
            duration_seconds,
            cooldown_seconds,
        },
        VehicleEffect::ForcedCruiseChanged { enabled, speed } => {
            HeavyVehicleEcsCommandKind::ForcedCruisePresentation { enabled, speed }
        }
        VehicleEffect::KinematicMotionRequested(motion) => {
            HeavyVehicleEcsCommandKind::MotionIntent(motion)
        }
        VehicleEffect::WallContact { vehicle_id, wall } => {
            HeavyVehicleEcsCommandKind::WallContactPresentation { vehicle_id, wall }
        }
        VehicleEffect::VehicleDamaged {
            vehicle_id,
            amount,
            remaining_hp,
        } => HeavyVehicleEcsCommandKind::DamagePresentation {
            vehicle_id,
            amount,
            remaining_hp,
        },
        VehicleEffect::VehicleDestroyed {
            vehicle_id,
            position,
        } => HeavyVehicleEcsCommandKind::DestroyedPresentation {
            vehicle_id,
            position,
        },
        VehicleEffect::GhostRideStarted {
            vehicle_id,
            operator,
            locked_heading,
            locked_speed,
            ttl_seconds,
            eject,
        } => HeavyVehicleEcsCommandKind::GhostRideStartedPresentation {
            vehicle_id,
            operator,
            locked_heading,
            locked_speed,
            ttl_seconds,
            eject,
        },
        VehicleEffect::GhostRideDetonated(descriptor) => {
            HeavyVehicleEcsCommandKind::GhostRideDetonationPresentation(Box::new(descriptor))
        }
    };
    HeavyVehicleEcsCommand { origin, command }
}

fn valid_local_operator(operator: VehicleOperatorId) -> bool {
    operator.0 < HEAVY_LOCAL_VEHICLE_OPERATOR_COUNT
}

fn validate_spawn_ownership(ownership: VehicleOwnership) -> Result<(), HeavyVehicleActionFailure> {
    if let Some(owner) = ownership
        .owner
        .filter(|owner| !valid_local_operator(*owner))
    {
        return Err(HeavyVehicleActionFailure::InvalidOperator(owner));
    }
    if ownership.access == VehicleAccess::OwnerOnly && ownership.owner.is_none() {
        // Keep the domain's exact error rather than inventing a second rule.
        return Err(HeavyVehicleActionFailure::Domain(
            HeavyVehicleError::OwnerRequired,
        ));
    }
    Ok(())
}

/// Atomic pure spawn bridge. The fleet commits only after post-validation.
pub fn spawn_heavy_vehicle_atomic(
    fleet: &mut HeavyVehicleFleet,
    preset: VehiclePresetId,
    position: VehicleVec3,
    ownership: VehicleOwnership,
) -> Result<(HeavyVehicleSpawnSuccess, VehicleEffect), HeavyVehicleActionFailure> {
    validate_spawn_ownership(ownership)?;
    let mut staged = fleet.clone();
    let outcome = staged.spawn_preset(preset, position, ownership)?;
    staged.validate()?;
    let success = HeavyVehicleSpawnSuccess {
        vehicle_id: outcome.vehicle_id,
        preset,
    };
    *fleet = staged;
    Ok((success, outcome.effect))
}

/// Atomic pure mount bridge, including source-accurate nearest selection.
pub fn mount_heavy_vehicle_atomic(
    fleet: &mut HeavyVehicleFleet,
    operator: VehicleOperatorId,
    target: &HeavyVehicleMountTarget,
) -> Result<(HeavyVehicleMountSuccess, VehicleEffect), HeavyVehicleActionFailure> {
    if !valid_local_operator(operator) {
        return Err(HeavyVehicleActionFailure::InvalidOperator(operator));
    }
    let mut staged = fleet.clone();
    let vehicle_id = match target {
        HeavyVehicleMountTarget::Vehicle(vehicle_id) => vehicle_id.clone(),
        HeavyVehicleMountTarget::Nearest {
            operator_position,
            maximum_range,
        } => staged
            .nearest_mountable(operator, *operator_position, *maximum_range)?
            .map(|vehicle| vehicle.id.clone())
            .ok_or(HeavyVehicleActionFailure::NoMountableVehicle {
                operator,
                position: *operator_position,
                maximum_range: *maximum_range,
            })?,
    };
    let effect = staged.mount(&vehicle_id, operator)?;
    staged.validate()?;
    *fleet = staged;
    Ok((
        HeavyVehicleMountSuccess {
            vehicle_id,
            operator,
        },
        effect,
    ))
}

pub fn eject_heavy_vehicle_atomic(
    fleet: &mut HeavyVehicleFleet,
    operator: VehicleOperatorId,
) -> Result<(HeavyVehicleEjectSuccess, VehicleEffect), HeavyVehicleActionFailure> {
    if !valid_local_operator(operator) {
        return Err(HeavyVehicleActionFailure::InvalidOperator(operator));
    }
    let mut staged = fleet.clone();
    let effect = staged.eject(operator)?;
    staged.validate()?;
    let VehicleEffect::Unmounted {
        vehicle_id,
        operator,
        eject,
        ..
    } = &effect
    else {
        return Err(HeavyVehicleActionFailure::Domain(
            HeavyVehicleError::FleetInvariantViolation,
        ));
    };
    let success = HeavyVehicleEjectSuccess {
        vehicle_id: vehicle_id.clone(),
        operator: *operator,
        eject: *eject,
    };
    *fleet = staged;
    Ok((success, effect))
}

pub fn turbo_heavy_vehicle_atomic(
    fleet: &mut HeavyVehicleFleet,
    operator: VehicleOperatorId,
) -> Result<(HeavyVehicleTurboSuccess, VehicleEffect), HeavyVehicleActionFailure> {
    if !valid_local_operator(operator) {
        return Err(HeavyVehicleActionFailure::InvalidOperator(operator));
    }
    let mut staged = fleet.clone();
    let effect = staged.trigger_turbo(operator)?;
    staged.validate()?;
    let VehicleEffect::TurboStarted {
        vehicle_id,
        kind,
        duration_seconds,
        cooldown_seconds,
    } = &effect
    else {
        return Err(HeavyVehicleActionFailure::Domain(
            HeavyVehicleError::FleetInvariantViolation,
        ));
    };
    let success = HeavyVehicleTurboSuccess {
        vehicle_id: vehicle_id.clone(),
        kind: *kind,
        duration_seconds: *duration_seconds,
        cooldown_seconds: *cooldown_seconds,
    };
    *fleet = staged;
    Ok((success, effect))
}

pub fn ghost_ride_heavy_vehicle_atomic(
    fleet: &mut HeavyVehicleFleet,
    operator: VehicleOperatorId,
) -> Result<(HeavyVehicleGhostRideSuccess, VehicleEffect), HeavyVehicleActionFailure> {
    if !valid_local_operator(operator) {
        return Err(HeavyVehicleActionFailure::InvalidOperator(operator));
    }
    let mut staged = fleet.clone();
    let effect = staged.start_ghost_ride(operator)?;
    staged.validate()?;
    let VehicleEffect::GhostRideStarted {
        vehicle_id,
        operator,
        locked_heading,
        locked_speed,
        ttl_seconds,
        eject,
    } = &effect
    else {
        return Err(HeavyVehicleActionFailure::Domain(
            HeavyVehicleError::FleetInvariantViolation,
        ));
    };
    let success = HeavyVehicleGhostRideSuccess {
        vehicle_id: vehicle_id.clone(),
        operator: *operator,
        locked_heading: *locked_heading,
        locked_speed: *locked_speed,
        ttl_seconds: *ttl_seconds,
        eject: *eject,
    };
    *fleet = staged;
    Ok((success, effect))
}

/// Pure fixed-step bridge. Probe or domain failure leaves the fleet byte-for-
/// byte equivalent to its pre-tick state.
pub fn advance_heavy_vehicle_fixed_tick(
    fleet: &mut HeavyVehicleFleet,
    probes: &HeavyVehicleFixedProbes,
    delta_seconds: f32,
) -> Result<Vec<VehicleEffect>, HeavyVehicleTickFailure> {
    probes.ground.validate()?;
    let mut staged = fleet.clone();
    let (camera_yaw, camera_pitch) = if let Some(mount) = &staged.active_mount {
        let camera = probes
            .camera_by_operator
            .get(&mount.operator)
            .copied()
            .ok_or(HeavyVehicleTickFailure::MissingCamera(mount.operator))?;
        staged.set_input(
            probes
                .controls_by_operator
                .get(&mount.operator)
                .copied()
                .unwrap_or_default(),
        );
        (camera.yaw, camera.pitch)
    } else {
        (0.0, 0.0)
    };

    let effects = staged.tick(
        VehicleTickInput {
            delta_seconds,
            camera_yaw,
            camera_pitch,
            walls: &probes.walls,
            ghost_targets: &probes.ghost_targets,
        },
        |x, z, _reference_y| probes.ground.sample(x, z),
    )?;
    staged.validate()?;
    *fleet = staged;
    Ok(effects)
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeavyVehicleUpdateSet {
    BindStableOperators,
    Spawn,
    Mount,
    Eject,
    Turbo,
    GhostRide,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeavyVehicleFixedSet {
    AdvanceDomain,
}

pub struct HeavyVehiclePlugin;

impl Plugin for HeavyVehiclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeavyWaterProgress>()
            .init_resource::<HeavyVehicleRuntimeState>()
            .init_resource::<HeavyVehicleFixedProbes>()
            .add_message::<HeavyVehicleSpawnRequest>()
            .add_message::<HeavyVehicleSpawnOutcome>()
            .add_message::<HeavyVehicleMountRequest>()
            .add_message::<HeavyVehicleMountOutcome>()
            .add_message::<HeavyVehicleEjectRequest>()
            .add_message::<HeavyVehicleEjectOutcome>()
            .add_message::<HeavyVehicleTurboRequest>()
            .add_message::<HeavyVehicleTurboOutcome>()
            .add_message::<HeavyVehicleGhostRideRequest>()
            .add_message::<HeavyVehicleGhostRideOutcome>()
            .add_message::<HeavyVehicleEcsCommand>()
            .add_message::<HeavyVehicleAdapterFault>()
            .configure_sets(
                Update,
                (
                    HeavyVehicleUpdateSet::BindStableOperators,
                    HeavyVehicleUpdateSet::Spawn,
                    HeavyVehicleUpdateSet::Mount,
                    HeavyVehicleUpdateSet::Eject,
                    HeavyVehicleUpdateSet::Turbo,
                    HeavyVehicleUpdateSet::GhostRide,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                bind_heavy_vehicle_operators
                    .in_set(HeavyVehicleUpdateSet::BindStableOperators)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                process_heavy_vehicle_spawn_requests
                    .in_set(HeavyVehicleUpdateSet::Spawn)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                process_heavy_vehicle_mount_requests
                    .in_set(HeavyVehicleUpdateSet::Mount)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                process_heavy_vehicle_eject_requests
                    .in_set(HeavyVehicleUpdateSet::Eject)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                process_heavy_vehicle_turbo_requests
                    .in_set(HeavyVehicleUpdateSet::Turbo)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                process_heavy_vehicle_ghost_ride_requests
                    .in_set(HeavyVehicleUpdateSet::GhostRide)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                FixedUpdate,
                tick_heavy_vehicles
                    .in_set(HeavyVehicleFixedSet::AdvanceDomain)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                OnExit(AppState::Playing),
                clear_heavy_vehicle_runtime_inputs,
            );
    }
}

pub fn bind_heavy_vehicle_operators(
    mut commands: Commands,
    players: Query<(Entity, &PlayerIndex, Option<&HeavyVehicleOperator>), With<Player>>,
    mut runtime: ResMut<HeavyVehicleRuntimeState>,
) {
    let mut bindings = Vec::new();
    let mut counts = BTreeMap::<VehicleOperatorId, usize>::new();
    let mut rejected = BTreeSet::new();

    for (entity, player_index, current) in &players {
        let Some(operator) = heavy_vehicle_operator_for_player_index(player_index.0) else {
            rejected.insert(player_index.0);
            if current.is_some() {
                commands.entity(entity).remove::<HeavyVehicleOperator>();
            }
            continue;
        };
        *counts.entry(operator).or_default() += 1;
        bindings.push((entity, operator, current.copied()));
    }

    // Sorting removes ECS query order from command emission and diagnostics.
    bindings.sort_by_key(|(entity, operator, _)| (*operator, entity.to_bits()));
    for (entity, operator, current) in bindings {
        if current.map(|value| value.0) != Some(operator) {
            commands
                .entity(entity)
                .insert(HeavyVehicleOperator(operator));
        }
    }

    runtime.bound_operators = counts.keys().copied().collect();
    runtime.duplicate_operators = counts
        .into_iter()
        .filter_map(|(operator, count)| (count > 1).then_some(operator))
        .collect();
    runtime.rejected_player_indices = rejected;
}

fn process_heavy_vehicle_spawn_requests(
    mut progress: ResMut<HeavyWaterProgress>,
    mut requests: MessageReader<HeavyVehicleSpawnRequest>,
    mut outcomes: MessageWriter<HeavyVehicleSpawnOutcome>,
    mut commands: MessageWriter<HeavyVehicleEcsCommand>,
) {
    for request in requests.read() {
        let result = spawn_heavy_vehicle_atomic(
            &mut progress.vehicles,
            request.preset,
            request.position,
            request.ownership,
        );
        match result {
            Ok((success, effect)) => {
                commands.write(heavy_vehicle_ecs_command(
                    HeavyVehicleCommandOrigin::Request {
                        request_id: request.request_id,
                        action: HeavyVehicleActionKind::Spawn,
                    },
                    effect,
                ));
                outcomes.write(HeavyVehicleSpawnOutcome {
                    request_id: request.request_id,
                    result: Ok(success),
                });
            }
            Err(failure) => {
                outcomes.write(HeavyVehicleSpawnOutcome {
                    request_id: request.request_id,
                    result: Err(failure),
                });
            }
        }
    }
}

fn process_heavy_vehicle_mount_requests(
    mut progress: ResMut<HeavyWaterProgress>,
    runtime: Res<HeavyVehicleRuntimeState>,
    mut requests: MessageReader<HeavyVehicleMountRequest>,
    mut outcomes: MessageWriter<HeavyVehicleMountOutcome>,
    mut commands: MessageWriter<HeavyVehicleEcsCommand>,
) {
    for request in requests.read() {
        let result = runtime.operator_failure(request.operator).map_or_else(
            || {
                mount_heavy_vehicle_atomic(
                    &mut progress.vehicles,
                    request.operator,
                    &request.target,
                )
            },
            Err,
        );
        match result {
            Ok((success, effect)) => {
                commands.write(heavy_vehicle_ecs_command(
                    HeavyVehicleCommandOrigin::Request {
                        request_id: request.request_id,
                        action: HeavyVehicleActionKind::Mount,
                    },
                    effect,
                ));
                outcomes.write(HeavyVehicleMountOutcome {
                    request_id: request.request_id,
                    result: Ok(success),
                });
            }
            Err(failure) => {
                outcomes.write(HeavyVehicleMountOutcome {
                    request_id: request.request_id,
                    result: Err(failure),
                });
            }
        }
    }
}

fn process_heavy_vehicle_eject_requests(
    mut progress: ResMut<HeavyWaterProgress>,
    runtime: Res<HeavyVehicleRuntimeState>,
    mut requests: MessageReader<HeavyVehicleEjectRequest>,
    mut outcomes: MessageWriter<HeavyVehicleEjectOutcome>,
    mut commands: MessageWriter<HeavyVehicleEcsCommand>,
) {
    for request in requests.read() {
        let result = runtime.operator_failure(request.operator).map_or_else(
            || eject_heavy_vehicle_atomic(&mut progress.vehicles, request.operator),
            Err,
        );
        match result {
            Ok((success, effect)) => {
                commands.write(heavy_vehicle_ecs_command(
                    HeavyVehicleCommandOrigin::Request {
                        request_id: request.request_id,
                        action: HeavyVehicleActionKind::Eject,
                    },
                    effect,
                ));
                outcomes.write(HeavyVehicleEjectOutcome {
                    request_id: request.request_id,
                    result: Ok(success),
                });
            }
            Err(failure) => {
                outcomes.write(HeavyVehicleEjectOutcome {
                    request_id: request.request_id,
                    result: Err(failure),
                });
            }
        }
    }
}

fn process_heavy_vehicle_turbo_requests(
    mut progress: ResMut<HeavyWaterProgress>,
    runtime: Res<HeavyVehicleRuntimeState>,
    mut requests: MessageReader<HeavyVehicleTurboRequest>,
    mut outcomes: MessageWriter<HeavyVehicleTurboOutcome>,
    mut commands: MessageWriter<HeavyVehicleEcsCommand>,
) {
    for request in requests.read() {
        let result = runtime.operator_failure(request.operator).map_or_else(
            || turbo_heavy_vehicle_atomic(&mut progress.vehicles, request.operator),
            Err,
        );
        match result {
            Ok((success, effect)) => {
                commands.write(heavy_vehicle_ecs_command(
                    HeavyVehicleCommandOrigin::Request {
                        request_id: request.request_id,
                        action: HeavyVehicleActionKind::Turbo,
                    },
                    effect,
                ));
                outcomes.write(HeavyVehicleTurboOutcome {
                    request_id: request.request_id,
                    result: Ok(success),
                });
            }
            Err(failure) => {
                outcomes.write(HeavyVehicleTurboOutcome {
                    request_id: request.request_id,
                    result: Err(failure),
                });
            }
        }
    }
}

fn process_heavy_vehicle_ghost_ride_requests(
    mut progress: ResMut<HeavyWaterProgress>,
    runtime: Res<HeavyVehicleRuntimeState>,
    mut requests: MessageReader<HeavyVehicleGhostRideRequest>,
    mut outcomes: MessageWriter<HeavyVehicleGhostRideOutcome>,
    mut commands: MessageWriter<HeavyVehicleEcsCommand>,
) {
    for request in requests.read() {
        let result = runtime.operator_failure(request.operator).map_or_else(
            || ghost_ride_heavy_vehicle_atomic(&mut progress.vehicles, request.operator),
            Err,
        );
        match result {
            Ok((success, effect)) => {
                commands.write(heavy_vehicle_ecs_command(
                    HeavyVehicleCommandOrigin::Request {
                        request_id: request.request_id,
                        action: HeavyVehicleActionKind::GhostRide,
                    },
                    effect,
                ));
                outcomes.write(HeavyVehicleGhostRideOutcome {
                    request_id: request.request_id,
                    result: Ok(success),
                });
            }
            Err(failure) => {
                outcomes.write(HeavyVehicleGhostRideOutcome {
                    request_id: request.request_id,
                    result: Err(failure),
                });
            }
        }
    }
}

fn tick_heavy_vehicles(
    fixed_time: Res<Time<Fixed>>,
    probes: Res<HeavyVehicleFixedProbes>,
    mut progress: ResMut<HeavyWaterProgress>,
    mut runtime: ResMut<HeavyVehicleRuntimeState>,
    mut commands: MessageWriter<HeavyVehicleEcsCommand>,
    mut faults: MessageWriter<HeavyVehicleAdapterFault>,
) {
    runtime.fixed_ticks_attempted = runtime.fixed_ticks_attempted.saturating_add(1);
    let fixed_tick = runtime.fixed_ticks_attempted;

    let binding_failure = progress
        .vehicles
        .active_mount
        .as_ref()
        .and_then(|mount| runtime.tick_operator_failure(mount.operator));
    let result = if let Some(failure) = binding_failure {
        Err(failure)
    } else {
        advance_heavy_vehicle_fixed_tick(&mut progress.vehicles, &probes, fixed_time.delta_secs())
    };

    match result {
        Ok(effects) => {
            runtime.fixed_ticks_completed = runtime.fixed_ticks_completed.saturating_add(1);
            runtime.last_tick_failure = None;
            for effect in effects {
                commands.write(heavy_vehicle_ecs_command(
                    HeavyVehicleCommandOrigin::FixedTick(fixed_tick),
                    effect,
                ));
            }
        }
        Err(failure) => {
            runtime.last_tick_failure = Some(failure.clone());
            faults.write(HeavyVehicleAdapterFault {
                fixed_tick,
                failure,
            });
        }
    }
}

fn clear_heavy_vehicle_runtime_inputs(
    mut probes: ResMut<HeavyVehicleFixedProbes>,
    mut runtime: ResMut<HeavyVehicleRuntimeState>,
) {
    probes.controls_by_operator.clear();
    probes.camera_by_operator.clear();
    runtime.bound_operators.clear();
    runtime.duplicate_operators.clear();
    runtime.rejected_player_indices.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    const OPERATOR: VehicleOperatorId = VehicleOperatorId(0);

    fn party_shared() -> VehicleOwnership {
        VehicleOwnership {
            owner: None,
            access: VehicleAccess::PartyShared,
        }
    }

    fn mounted_fleet() -> HeavyVehicleFleet {
        let mut fleet = HeavyVehicleFleet::default();
        let spawned = fleet
            .spawn_preset(
                VehiclePresetId::RaiderAtv,
                VehicleVec3::new(0.0, 0.0, 0.0),
                party_shared(),
            )
            .unwrap();
        fleet.mount(&spawned.vehicle_id, OPERATOR).unwrap();
        fleet
    }

    fn all_messages<M: Message + Clone>(app: &App) -> Vec<M> {
        let messages = app.world().resource::<Messages<M>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).cloned().collect()
    }

    #[test]
    fn operator_keys_are_stable_and_invalid_slots_never_alias_p4() {
        assert_eq!(
            heavy_vehicle_operator_for_player_index(0),
            Some(VehicleOperatorId(0))
        );
        assert_eq!(
            heavy_vehicle_operator_for_player_index(3),
            Some(VehicleOperatorId(3))
        );
        assert_eq!(heavy_vehicle_operator_for_player_index(4), None);
        assert_eq!(heavy_vehicle_operator_for_player_index(u8::MAX), None);
    }

    #[test]
    fn terrain_selection_is_independent_of_probe_insertion_order() {
        let low_id = HeavyVehicleGroundSurfaceProbe::new(2, 5, -10.0, 10.0, -10.0, 10.0, 7.0);
        let high_id = HeavyVehicleGroundSurfaceProbe::new(9, 5, -10.0, 10.0, -10.0, 10.0, 12.0);
        let priority = HeavyVehicleGroundSurfaceProbe::new(15, 6, -10.0, 10.0, -10.0, 10.0, 20.0);

        let mut ground = HeavyVehicleGroundProbe {
            fallback_height: -3.0,
            surfaces: vec![high_id, low_id],
        };
        assert_eq!(ground.sample(0.0, 0.0), 7.0);
        ground.surfaces.reverse();
        assert_eq!(ground.sample(0.0, 0.0), 7.0);
        ground.surfaces.push(priority);
        assert_eq!(ground.sample(0.0, 0.0), 20.0);
        assert_eq!(ground.sample(99.0, 99.0), -3.0);
    }

    #[test]
    fn failed_fixed_probe_is_atomic_and_valid_tick_emits_motion_intent() {
        let mut fleet = mounted_fleet();
        let original = fleet.clone();
        let mut probes = HeavyVehicleFixedProbes::default();
        probes.camera_by_operator.insert(
            OPERATOR,
            HeavyVehicleCameraProbe {
                yaw: 0.4,
                pitch: 0.0,
            },
        );
        probes
            .ground
            .surfaces
            .push(HeavyVehicleGroundSurfaceProbe::new(
                7, 0, 4.0, -4.0, -2.0, 2.0, 0.0,
            ));

        assert!(matches!(
            advance_heavy_vehicle_fixed_tick(&mut fleet, &probes, 0.1),
            Err(HeavyVehicleTickFailure::Probe(
                HeavyVehicleProbeError::InvalidGroundSurface(7)
            ))
        ));
        assert_eq!(fleet, original);

        probes.ground.surfaces.clear();
        probes.controls_by_operator.insert(
            OPERATOR,
            VehicleInputState {
                forward: true,
                ..Default::default()
            },
        );
        let effects = advance_heavy_vehicle_fixed_tick(&mut fleet, &probes, 0.1).unwrap();
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, VehicleEffect::KinematicMotionRequested(_))));
        assert_ne!(fleet, original);
    }

    #[test]
    fn ghost_detonation_conversion_is_a_descriptor_not_applied_damage() {
        let mut fleet = mounted_fleet();
        fleet.input.boost = true;
        let started = fleet.start_ghost_ride(OPERATOR).unwrap();
        let started_command = heavy_vehicle_ecs_command(
            HeavyVehicleCommandOrigin::Request {
                request_id: 10,
                action: HeavyVehicleActionKind::GhostRide,
            },
            started,
        );
        assert!(matches!(
            started_command.command,
            HeavyVehicleEcsCommandKind::GhostRideStartedPresentation { .. }
        ));

        let effects = fleet
            .tick(
                VehicleTickInput {
                    delta_seconds: crate::world::heavy_vehicle::GHOST_RIDE_TTL_SECONDS,
                    camera_yaw: 0.0,
                    camera_pitch: 0.0,
                    walls: &[],
                    ghost_targets: &[],
                },
                |_, _, _| 0.0,
            )
            .unwrap();
        let detonation = effects
            .into_iter()
            .find(|effect| matches!(effect, VehicleEffect::GhostRideDetonated(_)))
            .unwrap();
        let command =
            heavy_vehicle_ecs_command(HeavyVehicleCommandOrigin::FixedTick(1), detonation);
        let HeavyVehicleEcsCommandKind::GhostRideDetonationPresentation(descriptor) =
            command.command
        else {
            panic!("expected inert detonation descriptor");
        };
        assert!(!descriptor.radial_damage.damages_players);
        assert_eq!(descriptor.radial_damage.base_damage, 420.0);
    }

    #[test]
    fn app_chains_spawn_before_nearest_mount_and_emits_typed_outcomes() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<AppState>()
            .add_plugins(HeavyVehiclePlugin);
        app.world_mut().spawn((Player, PlayerIndex(0)));
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();

        app.world_mut()
            .resource_mut::<Messages<HeavyVehicleSpawnRequest>>()
            .write(HeavyVehicleSpawnRequest {
                request_id: 1,
                preset: VehiclePresetId::RaiderAtv,
                position: VehicleVec3::ZERO,
                ownership: party_shared(),
            });
        app.world_mut()
            .resource_mut::<Messages<HeavyVehicleMountRequest>>()
            .write(HeavyVehicleMountRequest {
                request_id: 2,
                operator: OPERATOR,
                target: HeavyVehicleMountTarget::Nearest {
                    operator_position: VehicleVec3::new(1.0, 0.0, 0.0),
                    maximum_range: 5.5,
                },
            });
        app.update();

        let progress = app.world().resource::<HeavyWaterProgress>();
        assert_eq!(progress.vehicles.vehicles.len(), 1);
        assert_eq!(
            progress
                .vehicles
                .active_mount
                .as_ref()
                .map(|mount| mount.operator),
            Some(OPERATOR)
        );
        assert!(all_messages::<HeavyVehicleSpawnOutcome>(&app)[0]
            .result
            .is_ok());
        assert!(all_messages::<HeavyVehicleMountOutcome>(&app)[0]
            .result
            .is_ok());
        let commands = all_messages::<HeavyVehicleEcsCommand>(&app);
        assert!(matches!(
            commands[0].command,
            HeavyVehicleEcsCommandKind::SpawnPresentation { .. }
        ));
        assert!(matches!(
            commands[1].command,
            HeavyVehicleEcsCommandKind::MountedPresentation { .. }
        ));
    }

    #[test]
    fn duplicate_player_slot_blocks_vehicle_lease_mutation() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<AppState>()
            .add_plugins(HeavyVehiclePlugin);
        app.world_mut().spawn((Player, PlayerIndex(0)));
        app.world_mut().spawn((Player, PlayerIndex(0)));
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();

        let spawned = {
            let mut progress = app.world_mut().resource_mut::<HeavyWaterProgress>();
            progress
                .vehicles
                .spawn_preset(
                    VehiclePresetId::RaiderAtv,
                    VehicleVec3::ZERO,
                    party_shared(),
                )
                .unwrap()
        };
        app.world_mut()
            .resource_mut::<Messages<HeavyVehicleMountRequest>>()
            .write(HeavyVehicleMountRequest {
                request_id: 8,
                operator: OPERATOR,
                target: HeavyVehicleMountTarget::Vehicle(spawned.vehicle_id),
            });
        app.update();

        assert!(app
            .world()
            .resource::<HeavyWaterProgress>()
            .vehicles
            .active_mount
            .is_none());
        assert!(matches!(
            all_messages::<HeavyVehicleMountOutcome>(&app)[0].result,
            Err(HeavyVehicleActionFailure::AmbiguousOperatorBinding(
                OPERATOR
            ))
        ));
    }
}
