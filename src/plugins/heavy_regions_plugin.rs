//! Runtime adapter for the save-backed Heavy Water region and prop domains.
//!
//! The domain module owns travel and cooldown rules; this plugin only gives
//! those rules a Bevy scheduling and messaging seam. It intentionally does not
//! mutate [`CurrentChapter`](crate::resources::CurrentChapter), teleport player
//! entities, spawn geometry, or hide render entities. A world adapter consumes
//! the ordered travel directive, performs teardown -> teleport -> mount, then
//! commits the staged transaction.

#![allow(dead_code)] // Public region/world/UI seam; physical consumers land incrementally.

use bevy::prelude::*;

use crate::engine::state::AppState;
use crate::world::heavy_regions::{
    region_definition, AtmosphereProfile, AtmosphereSample, LegacyPropKind, LegacyRegionId,
    PropLifecycleEvent, PropStateError, RegionMountPhase, RegionSession, RegionSessionError,
    RegionTravelAction, RegionTravelPlan, WorldPoint,
};
use crate::world::heavy_water::HeavyWaterProgress;

/// Derived presentation state for the currently committed Heavy Water region.
///
/// `active_region` is deliberately independent of Starfall's chapter state.
/// During a staged travel it continues to describe the committed region while
/// `transition` identifies the proposed destination.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct HeavyRegionRuntimeState {
    pub active_region: LegacyRegionId,
    pub generation: u64,
    pub mount_phase: RegionMountPhase,
    pub hides_shared_world: bool,
    pub suppresses_default_ground_director: bool,
    pub transition: Option<HeavyRegionTransitionStatus>,
}

impl HeavyRegionRuntimeState {
    pub fn from_session(session: &RegionSession) -> Self {
        let definition = region_definition(session.active.region);
        Self {
            active_region: session.active.region,
            generation: session.active.generation,
            mount_phase: session.active.phase,
            hides_shared_world: definition.hides_shared_world,
            suppresses_default_ground_director: definition.suppresses_default_ground_director,
            transition: None,
        }
    }

    fn replace_from_session(&mut self, session: &RegionSession) {
        *self = Self::from_session(session);
    }
}

impl FromWorld for HeavyRegionRuntimeState {
    fn from_world(world: &mut World) -> Self {
        Self::from_session(&world.resource::<HeavyWaterProgress>().regions)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyRegionTransitionStatus {
    pub request_id: u64,
    pub target: LegacyRegionId,
    pub generation: u64,
    pub action: RegionTravelAction,
}

/// Runtime-only clock and authored atmosphere profile for the committed region.
///
/// Heavy Water did not save sky-clock progress, so this resource is derived and
/// resets to the region's authored start hour when a travel is committed or a
/// loaded active region is reconciled. Deep space intentionally has no hour.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct HeavyRegionAtmosphere {
    pub region: LegacyRegionId,
    pub generation: u64,
    pub profile: AtmosphereProfile,
    pub current_hour: Option<f32>,
    pub fixed_ticks: u64,
}

impl HeavyRegionAtmosphere {
    pub fn from_session(session: &RegionSession) -> Self {
        let definition = region_definition(session.active.region);
        Self {
            region: session.active.region,
            generation: session.active.generation,
            profile: definition.atmosphere,
            current_hour: definition.atmosphere.normalized_start_hour(),
            fixed_ticks: 0,
        }
    }

    pub fn sample(&self) -> AtmosphereSample {
        self.profile.sample(self.current_hour.unwrap_or(0.0))
    }

    pub fn advance_fixed(&mut self, delta_seconds: f32) {
        if let Some(hour) = self.current_hour {
            self.current_hour = Some(self.profile.advance_hour(hour, delta_seconds));
        }
        self.fixed_ticks = self.fixed_ticks.saturating_add(1);
    }

    fn replace_from_session(&mut self, session: &RegionSession) {
        *self = Self::from_session(session);
    }

    fn matches_session(&self, session: &RegionSession) -> bool {
        let definition = region_definition(session.active.region);
        self.region == session.active.region
            && self.generation == session.active.generation
            && self.profile == definition.atmosphere
    }
}

impl FromWorld for HeavyRegionAtmosphere {
    fn from_world(world: &mut World) -> Self {
        Self::from_session(&world.resource::<HeavyWaterProgress>().regions)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeavyRegionTravelIntent {
    Enter {
        target: LegacyRegionId,
        current_position: WorldPoint,
    },
    ReturnToSavedAnchor,
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyRegionTravelRequest {
    /// Caller-owned correlation id. The adapter treats it as opaque.
    pub request_id: u64,
    pub intent: HeavyRegionTravelIntent,
}

/// Operations a world bridge must execute in this exact vector order.
#[derive(Debug, Clone, PartialEq)]
pub enum HeavyRegionTravelOperation {
    TeardownPrevious {
        region: LegacyRegionId,
    },
    TeleportPlayers {
        destination: WorldPoint,
    },
    MountTarget {
        region: LegacyRegionId,
        generation: u64,
        action: RegionTravelAction,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeavyRegionTravelDirective {
    pub plan: RegionTravelPlan,
    pub operations: Vec<HeavyRegionTravelOperation>,
}

/// Builds the executable ordering contract without touching ECS or chapters.
pub fn ordered_heavy_region_travel_operations(
    plan: &RegionTravelPlan,
) -> Vec<HeavyRegionTravelOperation> {
    let mut operations = Vec::with_capacity(if plan.dispose_previous_before_mount {
        3
    } else {
        2
    });
    if plan.dispose_previous_before_mount {
        operations.push(HeavyRegionTravelOperation::TeardownPrevious { region: plan.from });
    }
    // Heavy Water's fast travel teleports first because mounting synchronously
    // reads the player's position for orbital fields and encounter placement.
    operations.push(HeavyRegionTravelOperation::TeleportPlayers {
        destination: plan.arrival,
    });
    operations.push(HeavyRegionTravelOperation::MountTarget {
        region: plan.to,
        generation: plan.generation,
        action: plan.action,
    });
    operations
}

/// Stages a domain transition in a clone. The durable session remains unchanged
/// until a consumer reports that every ordered operation succeeded.
pub fn stage_heavy_region_travel(
    current: &RegionSession,
    intent: &HeavyRegionTravelIntent,
) -> Result<(RegionSession, HeavyRegionTravelDirective), RegionSessionError> {
    let mut staged = current.clone();
    let plan = match intent {
        HeavyRegionTravelIntent::Enter {
            target,
            current_position,
        } => staged.travel_to(*target, *current_position)?,
        HeavyRegionTravelIntent::ReturnToSavedAnchor => staged.begin_return()?,
    };
    let operations = ordered_heavy_region_travel_operations(&plan);
    Ok((staged, HeavyRegionTravelDirective { plan, operations }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeavyRegionTravelFailure {
    Busy {
        pending_request_id: u64,
    },
    NoPendingTravel,
    RequestIdMismatch {
        expected: u64,
        received: u64,
    },
    ProgressChangedDuringTravel {
        expected_region: LegacyRegionId,
        expected_generation: u64,
        actual_region: LegacyRegionId,
        actual_generation: u64,
    },
    Domain(RegionSessionError),
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyRegionTravelPrepared {
    pub request_id: u64,
    pub result: Result<HeavyRegionTravelDirective, HeavyRegionTravelFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyRegionTravelResolutionKind {
    /// Send only after the directive's final `MountTarget` operation succeeds.
    CommitAfterOrderedOperations,
    /// Drops the staged domain change. External partial work remains the world
    /// adapter's responsibility to roll back.
    Cancel,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyRegionTravelResolution {
    pub request_id: u64,
    pub resolution: HeavyRegionTravelResolutionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyRegionTravelCompletion {
    Committed {
        region: LegacyRegionId,
        generation: u64,
    },
    Cancelled,
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HeavyRegionTravelFinished {
    pub request_id: u64,
    pub result: Result<HeavyRegionTravelCompletion, HeavyRegionTravelFailure>,
}

#[derive(Debug, Clone)]
struct PendingTravelTransaction {
    request_id: u64,
    base_session: RegionSession,
    staged_session: RegionSession,
    directive: HeavyRegionTravelDirective,
}

#[derive(Resource, Debug, Default)]
pub struct PendingHeavyRegionTravel {
    pending: Option<PendingTravelTransaction>,
}

impl PendingHeavyRegionTravel {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn request_id(&self) -> Option<u64> {
        self.pending.as_ref().map(|pending| pending.request_id)
    }

    pub fn directive(&self) -> Option<&HeavyRegionTravelDirective> {
        self.pending.as_ref().map(|pending| &pending.directive)
    }
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyPropRespawned {
    pub stable_id: String,
    pub kind: LegacyPropKind,
    pub region: LegacyRegionId,
    pub position: WorldPoint,
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HeavyRegionAdapterFault {
    pub error: PropStateError,
}

/// Pure bridge used by the fixed-tick system and deterministic simulations.
pub fn advance_heavy_prop_respawns(
    progress: &mut HeavyWaterProgress,
    delta_seconds: f32,
) -> Result<Vec<HeavyPropRespawned>, PropStateError> {
    let events = progress.props.tick(delta_seconds)?;
    let mut respawned = Vec::with_capacity(events.len());
    for event in events {
        let PropLifecycleEvent::Respawned { stable_id } = event;
        let Some(prop) = progress.props.get(&stable_id) else {
            // The domain emits after mutating the same registry, so absence can
            // only mean a future implementation changed that contract.
            continue;
        };
        respawned.push(HeavyPropRespawned {
            stable_id,
            kind: prop.kind,
            region: prop.region,
            position: prop.position,
        });
    }
    Ok(respawned)
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeavyRegionsRuntimeSet {
    SynchronizeDerivedState,
    PrepareTravel,
    ResolveTravel,
}

pub struct HeavyRegionsPlugin;

impl Plugin for HeavyRegionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeavyWaterProgress>()
            .init_resource::<HeavyRegionRuntimeState>()
            .init_resource::<HeavyRegionAtmosphere>()
            .init_resource::<PendingHeavyRegionTravel>()
            .add_message::<HeavyRegionTravelRequest>()
            .add_message::<HeavyRegionTravelPrepared>()
            .add_message::<HeavyRegionTravelResolution>()
            .add_message::<HeavyRegionTravelFinished>()
            .add_message::<HeavyPropRespawned>()
            .add_message::<HeavyRegionAdapterFault>()
            .configure_sets(
                Update,
                (
                    HeavyRegionsRuntimeSet::SynchronizeDerivedState,
                    HeavyRegionsRuntimeSet::PrepareTravel,
                    HeavyRegionsRuntimeSet::ResolveTravel,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                synchronize_derived_region_state
                    .in_set(HeavyRegionsRuntimeSet::SynchronizeDerivedState)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                prepare_region_travel_requests
                    .in_set(HeavyRegionsRuntimeSet::PrepareTravel)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                resolve_region_travel_requests
                    .in_set(HeavyRegionsRuntimeSet::ResolveTravel)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                FixedUpdate,
                (advance_region_atmosphere, tick_heavy_prop_respawns)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

fn synchronize_derived_region_state(
    progress: Res<HeavyWaterProgress>,
    pending: Res<PendingHeavyRegionTravel>,
    mut runtime: ResMut<HeavyRegionRuntimeState>,
    mut atmosphere: ResMut<HeavyRegionAtmosphere>,
) {
    // A staged transaction is not authoritative. Keep consumers on the
    // committed region until the ordered external work is acknowledged.
    if pending.is_pending() {
        return;
    }

    let expected_runtime = HeavyRegionRuntimeState::from_session(&progress.regions);
    if *runtime != expected_runtime {
        runtime.replace_from_session(&progress.regions);
    }
    if !atmosphere.matches_session(&progress.regions) {
        atmosphere.replace_from_session(&progress.regions);
    }
}

fn prepare_region_travel_requests(
    progress: Res<HeavyWaterProgress>,
    mut pending: ResMut<PendingHeavyRegionTravel>,
    mut runtime: ResMut<HeavyRegionRuntimeState>,
    mut requests: MessageReader<HeavyRegionTravelRequest>,
    mut prepared: MessageWriter<HeavyRegionTravelPrepared>,
) {
    for request in requests.read() {
        if let Some(pending_request_id) = pending.request_id() {
            prepared.write(HeavyRegionTravelPrepared {
                request_id: request.request_id,
                result: Err(HeavyRegionTravelFailure::Busy { pending_request_id }),
            });
            continue;
        }

        let base_session = progress.regions.clone();
        match stage_heavy_region_travel(&base_session, &request.intent) {
            Ok((staged_session, directive)) => {
                runtime.transition = Some(HeavyRegionTransitionStatus {
                    request_id: request.request_id,
                    target: directive.plan.to,
                    generation: directive.plan.generation,
                    action: directive.plan.action,
                });
                pending.pending = Some(PendingTravelTransaction {
                    request_id: request.request_id,
                    base_session,
                    staged_session,
                    directive: directive.clone(),
                });
                prepared.write(HeavyRegionTravelPrepared {
                    request_id: request.request_id,
                    result: Ok(directive),
                });
            }
            Err(error) => {
                prepared.write(HeavyRegionTravelPrepared {
                    request_id: request.request_id,
                    result: Err(HeavyRegionTravelFailure::Domain(error)),
                });
            }
        }
    }
}

fn resolve_region_travel_requests(
    mut progress: ResMut<HeavyWaterProgress>,
    mut pending: ResMut<PendingHeavyRegionTravel>,
    mut runtime: ResMut<HeavyRegionRuntimeState>,
    mut atmosphere: ResMut<HeavyRegionAtmosphere>,
    mut resolutions: MessageReader<HeavyRegionTravelResolution>,
    mut finished: MessageWriter<HeavyRegionTravelFinished>,
) {
    for resolution in resolutions.read() {
        let Some(expected_request_id) = pending.request_id() else {
            finished.write(HeavyRegionTravelFinished {
                request_id: resolution.request_id,
                result: Err(HeavyRegionTravelFailure::NoPendingTravel),
            });
            continue;
        };
        if expected_request_id != resolution.request_id {
            finished.write(HeavyRegionTravelFinished {
                request_id: resolution.request_id,
                result: Err(HeavyRegionTravelFailure::RequestIdMismatch {
                    expected: expected_request_id,
                    received: resolution.request_id,
                }),
            });
            continue;
        }

        let transaction = pending
            .pending
            .take()
            .expect("a matching request id requires a pending transaction");
        match resolution.resolution {
            HeavyRegionTravelResolutionKind::Cancel => {
                runtime.transition = None;
                finished.write(HeavyRegionTravelFinished {
                    request_id: resolution.request_id,
                    result: Ok(HeavyRegionTravelCompletion::Cancelled),
                });
            }
            HeavyRegionTravelResolutionKind::CommitAfterOrderedOperations => {
                if progress.regions != transaction.base_session {
                    let expected = transaction.base_session.active;
                    let actual = progress.regions.active.clone();
                    runtime.transition = None;
                    finished.write(HeavyRegionTravelFinished {
                        request_id: resolution.request_id,
                        result: Err(HeavyRegionTravelFailure::ProgressChangedDuringTravel {
                            expected_region: expected.region,
                            expected_generation: expected.generation,
                            actual_region: actual.region,
                            actual_generation: actual.generation,
                        }),
                    });
                    continue;
                }

                let mut staged = transaction.staged_session;
                if let Err(error) = staged.finish_mount() {
                    runtime.transition = None;
                    finished.write(HeavyRegionTravelFinished {
                        request_id: resolution.request_id,
                        result: Err(HeavyRegionTravelFailure::Domain(error)),
                    });
                    continue;
                }

                let region = staged.active.region;
                let generation = staged.active.generation;
                progress.regions = staged;
                runtime.replace_from_session(&progress.regions);
                atmosphere.replace_from_session(&progress.regions);
                finished.write(HeavyRegionTravelFinished {
                    request_id: resolution.request_id,
                    result: Ok(HeavyRegionTravelCompletion::Committed { region, generation }),
                });
            }
        }
    }
}

fn advance_region_atmosphere(
    fixed_time: Res<Time<Fixed>>,
    mut atmosphere: ResMut<HeavyRegionAtmosphere>,
) {
    atmosphere.advance_fixed(fixed_time.delta_secs());
}

fn tick_heavy_prop_respawns(
    fixed_time: Res<Time<Fixed>>,
    mut progress: ResMut<HeavyWaterProgress>,
    mut respawned: MessageWriter<HeavyPropRespawned>,
    mut faults: MessageWriter<HeavyRegionAdapterFault>,
) {
    match advance_heavy_prop_respawns(&mut progress, fixed_time.delta_secs()) {
        Ok(events) => {
            respawned.write_batch(events);
        }
        Err(error) => {
            faults.write(HeavyRegionAdapterFault { error });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::heavy_regions::{LegacyPropKind, PropState, RegionTravelAction, SkyMode};
    use bevy::state::app::StatesPlugin;

    fn enter_intent(target: LegacyRegionId) -> HeavyRegionTravelIntent {
        HeavyRegionTravelIntent::Enter {
            target,
            current_position: WorldPoint::new(12.0, 3.0, -8.0),
        }
    }

    #[test]
    fn changed_region_directive_is_teardown_then_teleport_then_mount() {
        let session = RegionSession::new(LegacyRegionId::DetroitStarCityFront);
        let (staged, directive) =
            stage_heavy_region_travel(&session, &enter_intent(LegacyRegionId::OrbitalFront))
                .unwrap();

        assert_eq!(session.active.region, LegacyRegionId::DetroitStarCityFront);
        assert_eq!(staged.active.region, LegacyRegionId::OrbitalFront);
        assert_eq!(staged.active.phase, RegionMountPhase::Mounting);
        assert_eq!(directive.operations.len(), 3);
        assert!(matches!(
            directive.operations[0],
            HeavyRegionTravelOperation::TeardownPrevious {
                region: LegacyRegionId::DetroitStarCityFront
            }
        ));
        assert!(matches!(
            directive.operations[1],
            HeavyRegionTravelOperation::TeleportPlayers {
                destination: WorldPoint { y: 300.0, .. }
            }
        ));
        assert!(matches!(
            directive.operations[2],
            HeavyRegionTravelOperation::MountTarget {
                region: LegacyRegionId::OrbitalFront,
                action: RegionTravelAction::Enter,
                ..
            }
        ));
    }

    #[test]
    fn same_region_reassert_skips_teardown_but_keeps_teleport_before_mount() {
        let session = RegionSession::new(LegacyRegionId::SaginawUnderwaterLab);
        let (_, directive) = stage_heavy_region_travel(
            &session,
            &enter_intent(LegacyRegionId::SaginawUnderwaterLab),
        )
        .unwrap();
        assert_eq!(directive.operations.len(), 2);
        assert!(matches!(
            directive.operations[0],
            HeavyRegionTravelOperation::TeleportPlayers { .. }
        ));
        assert!(matches!(
            directive.operations[1],
            HeavyRegionTravelOperation::MountTarget {
                action: RegionTravelAction::Reassert,
                ..
            }
        ));
    }

    #[test]
    fn derived_atmosphere_resets_from_region_and_advances_without_persistence() {
        let session = RegionSession::new(LegacyRegionId::DetroitHoldTheLine);
        let mut atmosphere = HeavyRegionAtmosphere::from_session(&session);
        assert_eq!(atmosphere.current_hour, Some(18.5));
        atmosphere.advance_fixed(5.0);
        assert_eq!(atmosphere.fixed_ticks, 1);
        assert_eq!(atmosphere.current_hour, Some(18.9));

        let orbital = RegionSession::new(LegacyRegionId::OrbitalFront);
        atmosphere.replace_from_session(&orbital);
        atmosphere.advance_fixed(99.0);
        assert_eq!(atmosphere.profile.sky_mode, SkyMode::DeepSpace);
        assert_eq!(atmosphere.current_hour, None);
        assert_eq!(
            atmosphere.sample().phase,
            crate::world::heavy_regions::SkyPhase::DeepSpace
        );
    }

    #[test]
    fn pure_prop_adapter_respawns_in_domain_stable_order() {
        let mut progress = HeavyWaterProgress::default();
        for stable_id in ["mining-z", "mining-a"] {
            let mut prop = PropState::new(
                stable_id,
                LegacyPropKind::ScrapPile,
                LegacyRegionId::MichiganWilds,
                WorldPoint::ZERO,
            )
            .unwrap();
            prop.damage(30.0).unwrap();
            progress.props.insert(prop).unwrap();
        }

        assert!(advance_heavy_prop_respawns(&mut progress, 34.0)
            .unwrap()
            .is_empty());
        let respawned = advance_heavy_prop_respawns(&mut progress, 1.0).unwrap();
        let ids: Vec<_> = respawned
            .iter()
            .map(|event| event.stable_id.as_str())
            .collect();
        assert_eq!(ids, ["mining-a", "mining-z"]);
        assert!(respawned
            .iter()
            .all(|event| event.kind == LegacyPropKind::ScrapPile));
    }

    #[test]
    fn plugin_stages_then_atomically_commits_without_touching_chapters() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<AppState>()
            .add_plugins(HeavyRegionsPlugin);
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();

        app.world_mut()
            .resource_mut::<Messages<HeavyRegionTravelRequest>>()
            .write(HeavyRegionTravelRequest {
                request_id: 41,
                intent: enter_intent(LegacyRegionId::PontiacSecretLab),
            });
        app.update();

        // Preparation is transactional: durable progress stays at Detroit.
        assert_eq!(
            app.world()
                .resource::<HeavyWaterProgress>()
                .regions
                .active
                .region,
            LegacyRegionId::DetroitStarCityFront
        );
        assert_eq!(
            app.world()
                .resource::<PendingHeavyRegionTravel>()
                .request_id(),
            Some(41)
        );
        assert_eq!(
            app.world()
                .resource::<HeavyRegionRuntimeState>()
                .transition
                .as_ref()
                .map(|transition| transition.target),
            Some(LegacyRegionId::PontiacSecretLab)
        );

        app.world_mut()
            .resource_mut::<Messages<HeavyRegionTravelResolution>>()
            .write(HeavyRegionTravelResolution {
                request_id: 41,
                resolution: HeavyRegionTravelResolutionKind::CommitAfterOrderedOperations,
            });
        app.update();

        let progress = app.world().resource::<HeavyWaterProgress>();
        assert_eq!(
            progress.regions.active.region,
            LegacyRegionId::PontiacSecretLab
        );
        assert_eq!(progress.regions.active.phase, RegionMountPhase::Active);
        let runtime = app.world().resource::<HeavyRegionRuntimeState>();
        assert_eq!(runtime.active_region, LegacyRegionId::PontiacSecretLab);
        assert!(runtime.hides_shared_world);
        assert!(runtime.suppresses_default_ground_director);
        assert!(runtime.transition.is_none());
    }

    #[test]
    fn cancellation_discards_staged_domain_state() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<AppState>()
            .add_plugins(HeavyRegionsPlugin);
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();
        let original = app.world().resource::<HeavyWaterProgress>().regions.clone();

        app.world_mut()
            .resource_mut::<Messages<HeavyRegionTravelRequest>>()
            .write(HeavyRegionTravelRequest {
                request_id: 7,
                intent: enter_intent(LegacyRegionId::MichiganWilds),
            });
        app.update();
        app.world_mut()
            .resource_mut::<Messages<HeavyRegionTravelResolution>>()
            .write(HeavyRegionTravelResolution {
                request_id: 7,
                resolution: HeavyRegionTravelResolutionKind::Cancel,
            });
        app.update();

        assert_eq!(
            app.world().resource::<HeavyWaterProgress>().regions,
            original
        );
        assert!(!app
            .world()
            .resource::<PendingHeavyRegionTravel>()
            .is_pending());
        assert!(app
            .world()
            .resource::<HeavyRegionRuntimeState>()
            .transition
            .is_none());
    }
}
