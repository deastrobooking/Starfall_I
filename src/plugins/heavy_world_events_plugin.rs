//! Offline Bevy bridge for Heavy Water invasions, friendly dialogue, and the
//! field manual.
//!
//! The domain in [`crate::world::heavy_world_events`] owns every durable rule.
//! This adapter supplies a deterministic Playing-only clock, stable local and
//! campaign keys, and typed messages. It never mutates Starfall chapter/raid
//! resources and never spawns an encounter: a canonical world director must
//! consume [`HeavyInvasionEncounterDescriptor`] and decide what is appropriate
//! for the active Starfall campaign.
//!
//! [`HeavyWorldEventsStore`] is independently serializable so the eventual
//! aggregate-save adapter can persist it without ever recording ECS `Entity`
//! values. Secure-online authority is intentionally outside this module.

#![allow(dead_code)] // Consumers and aggregate-save wiring land incrementally.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::player::{Player, PlayerIndex};
use crate::engine::game_loop::PlayingSetupSet;
use crate::engine::state::AppState;
use crate::world::heavy_world_events::{
    DialogueCloseReason, FieldManualEvent, FieldManualProgress, FriendlyNpcId, GuideSectionId,
    HeavyWorldEventsSave, InvasionDirector, InvasionEvent, InvasionNormalizationReport,
    InvasionResolutionReason, NpcDialogueEvent, NpcDialogueNormalizationReport, NpcDialogueState,
    WorldEventDomainError, WorldEventPoint, WorldEventsNormalizationReport,
    HEAVY_WORLD_EVENTS_SCHEMA_VERSION,
};

pub const HEAVY_WORLD_EVENTS_BRIDGE_SCHEMA_VERSION: u16 = 1;
pub const MAX_HEAVY_LOCAL_PLAYERS: u8 = 4;
const NANOS_PER_MILLISECOND: u32 = 1_000_000;

fn bridge_schema_version() -> u16 {
    HEAVY_WORLD_EVENTS_BRIDGE_SCHEMA_VERSION
}

/// Stable local ownership key. Slot values map to `local:p1` through
/// `local:p4`; invalid component indices are rejected rather than clamped and
/// aliased onto another player's progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeavyLocalPlayerKey(u8);

impl HeavyLocalPlayerKey {
    pub const PLAYER_ONE: Self = Self(0);

    pub const fn from_player_index(player_index: u8) -> Option<Self> {
        if player_index < MAX_HEAVY_LOCAL_PLAYERS {
            Some(Self(player_index))
        } else {
            None
        }
    }

    pub const fn player_index(self) -> u8 {
        self.0
    }

    pub fn stable_id(self) -> String {
        format!("local:p{}", self.0 + 1)
    }

    pub const fn is_valid(self) -> bool {
        self.0 < MAX_HEAVY_LOCAL_PLAYERS
    }
}

impl Default for HeavyLocalPlayerKey {
    fn default() -> Self {
        Self::PLAYER_ONE
    }
}

/// Stable offline campaign/save-slot key. It is deliberately an opaque slot,
/// not a chapter entity or runtime encounter ID.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct HeavyCampaignKey(pub u16);

impl HeavyCampaignKey {
    pub const OFFLINE_MAIN: Self = Self(0);

    pub fn stable_id(self) -> String {
        format!("offline:campaign:{}", self.0)
    }
}

/// Optional convenience marker attached to a live player. Only the slot key is
/// retained; the entity that carries it is never copied into durable state or
/// emitted in a message.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyWorldEventPlayerKey(pub HeavyLocalPlayerKey);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeavyWorldEventClock {
    pub game_time_ms: u64,
    submillisecond_nanos: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeavyWorldEventClockAdvance {
    pub elapsed_ms: u64,
    pub now_ms: u64,
    pub now_epoch_seconds: u64,
}

impl HeavyWorldEventClock {
    pub fn from_parts(game_time_ms: u64, submillisecond_nanos: u32) -> Self {
        let mut clock = Self {
            game_time_ms,
            submillisecond_nanos,
        };
        clock.normalize();
        clock
    }

    pub const fn submillisecond_nanos(&self) -> u32 {
        self.submillisecond_nanos
    }

    pub const fn now_epoch_seconds(&self) -> u64 {
        self.game_time_ms / 1_000
    }

    /// Advances only by caller-supplied simulation time. Pauses, wall-clock
    /// changes, and render-frame cadence cannot age an invasion.
    pub fn advance(&mut self, delta: Duration) -> HeavyWorldEventClockAdvance {
        self.normalize();
        let total_nanos = delta
            .as_nanos()
            .saturating_add(u128::from(self.submillisecond_nanos));
        let whole_ms = total_nanos / u128::from(NANOS_PER_MILLISECOND);
        let available_ms = u128::from(u64::MAX - self.game_time_ms);
        let elapsed_ms;

        if whole_ms > available_ms {
            elapsed_ms = u64::MAX - self.game_time_ms;
            self.game_time_ms = u64::MAX;
            self.submillisecond_nanos = 0;
        } else {
            elapsed_ms = whole_ms as u64;
            self.game_time_ms += elapsed_ms;
            self.submillisecond_nanos = (total_nanos % u128::from(NANOS_PER_MILLISECOND)) as u32;
        }

        HeavyWorldEventClockAdvance {
            elapsed_ms,
            now_ms: self.game_time_ms,
            now_epoch_seconds: self.now_epoch_seconds(),
        }
    }

    /// Carries malformed fractional save data into whole milliseconds.
    pub fn normalize(&mut self) -> bool {
        let carry = self.submillisecond_nanos / NANOS_PER_MILLISECOND;
        self.submillisecond_nanos %= NANOS_PER_MILLISECOND;
        if carry == 0 {
            return false;
        }
        self.game_time_ms = self.game_time_ms.saturating_add(u64::from(carry));
        true
    }

    pub const fn validate(&self) -> bool {
        self.submillisecond_nanos < NANOS_PER_MILLISECOND
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeavyPlayerWorldEvents {
    pub npc_dialogue: NpcDialogueState,
    pub field_manual: FieldManualProgress,
}

impl HeavyPlayerWorldEvents {
    pub fn validate(&self) -> Result<(), WorldEventDomainError> {
        self.npc_dialogue.validate()?;
        self.field_manual.validate()
    }

    pub fn normalize(&mut self) -> HeavyPlayerWorldEventsNormalizationReport {
        HeavyPlayerWorldEventsNormalizationReport {
            npc_dialogue: self.npc_dialogue.normalize(),
            field_manual_changed: {
                let report = self.field_manual.normalize();
                report.inferred_first_open_timestamp || report.repaired_timestamp_order
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeavyPlayerWorldEventsNormalizationReport {
    pub npc_dialogue: NpcDialogueNormalizationReport,
    pub field_manual_changed: bool,
}

/// Save-ready offline state. Campaign invasion state is global per campaign;
/// dialogue and guide progress are local-player scoped.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeavyWorldEventsStore {
    #[serde(default = "bridge_schema_version")]
    pub schema_version: u16,
    #[serde(default)]
    pub clock: HeavyWorldEventClock,
    #[serde(default)]
    pub campaigns: BTreeMap<HeavyCampaignKey, InvasionDirector>,
    #[serde(default)]
    pub players: BTreeMap<HeavyLocalPlayerKey, HeavyPlayerWorldEvents>,
}

impl Default for HeavyWorldEventsStore {
    fn default() -> Self {
        Self {
            schema_version: HEAVY_WORLD_EVENTS_BRIDGE_SCHEMA_VERSION,
            clock: HeavyWorldEventClock::default(),
            campaigns: BTreeMap::from([(
                HeavyCampaignKey::OFFLINE_MAIN,
                InvasionDirector::default(),
            )]),
            players: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeavyWorldEventsStoreError {
    UnsupportedSchema(u16),
    InvalidClock,
    ClockPrecedesPersistedState {
        clock_ms: u64,
        required_ms: u64,
    },
    InvalidLocalPlayerKey(u8),
    InvalidCampaign {
        campaign_key: HeavyCampaignKey,
        source: WorldEventDomainError,
    },
    InvalidPlayer {
        player_key: HeavyLocalPlayerKey,
        source: WorldEventDomainError,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeavyWorldEventsStoreNormalizationReport {
    pub schema_updated: bool,
    pub clock_repaired: bool,
    pub default_campaign_inserted: bool,
    pub invalid_players_removed: usize,
    pub campaign_reports: BTreeMap<HeavyCampaignKey, InvasionNormalizationReport>,
    pub player_reports: BTreeMap<HeavyLocalPlayerKey, HeavyPlayerWorldEventsNormalizationReport>,
}

impl HeavyWorldEventsStore {
    pub fn campaign(&self, key: HeavyCampaignKey) -> Option<&InvasionDirector> {
        self.campaigns.get(&key)
    }

    pub fn campaign_mut(&mut self, key: HeavyCampaignKey) -> &mut InvasionDirector {
        self.campaigns.entry(key).or_default()
    }

    pub fn player(&self, key: HeavyLocalPlayerKey) -> Option<&HeavyPlayerWorldEvents> {
        self.players.get(&key)
    }

    pub fn player_mut(&mut self, key: HeavyLocalPlayerKey) -> &mut HeavyPlayerWorldEvents {
        self.players.entry(key).or_default()
    }

    /// Produces the domain's single-player aggregate without changing either
    /// side. The deterministic bridge clock remains a sibling save field.
    pub fn domain_snapshot(
        &self,
        campaign_key: HeavyCampaignKey,
        player_key: HeavyLocalPlayerKey,
    ) -> Option<HeavyWorldEventsSave> {
        let invasion = self.campaign(campaign_key)?.clone();
        let player = self.player(player_key).cloned().unwrap_or_default();
        Some(HeavyWorldEventsSave {
            schema_version: HEAVY_WORLD_EVENTS_SCHEMA_VERSION,
            invasion,
            npc_dialogue: player.npc_dialogue,
            field_manual: player.field_manual,
        })
    }

    /// Imports a normalized domain snapshot into explicit stable scopes.
    pub fn apply_domain_snapshot(
        &mut self,
        campaign_key: HeavyCampaignKey,
        player_key: HeavyLocalPlayerKey,
        mut snapshot: HeavyWorldEventsSave,
    ) -> WorldEventsNormalizationReport {
        let report = snapshot.normalize();
        self.campaigns.insert(campaign_key, snapshot.invasion);
        self.players.insert(
            player_key,
            HeavyPlayerWorldEvents {
                npc_dialogue: snapshot.npc_dialogue,
                field_manual: snapshot.field_manual,
            },
        );
        self.reconcile_clock_with_persisted_state();
        report
    }

    pub fn validate(&self) -> Result<(), HeavyWorldEventsStoreError> {
        if self.schema_version != HEAVY_WORLD_EVENTS_BRIDGE_SCHEMA_VERSION {
            return Err(HeavyWorldEventsStoreError::UnsupportedSchema(
                self.schema_version,
            ));
        }
        if !self.clock.validate() {
            return Err(HeavyWorldEventsStoreError::InvalidClock);
        }
        let required_ms = self.latest_persisted_epoch_seconds().saturating_mul(1_000);
        if self.clock.game_time_ms < required_ms {
            return Err(HeavyWorldEventsStoreError::ClockPrecedesPersistedState {
                clock_ms: self.clock.game_time_ms,
                required_ms,
            });
        }
        for (campaign_key, campaign) in &self.campaigns {
            campaign
                .validate()
                .map_err(|source| HeavyWorldEventsStoreError::InvalidCampaign {
                    campaign_key: *campaign_key,
                    source,
                })?;
        }
        for (player_key, player) in &self.players {
            if !player_key.is_valid() {
                return Err(HeavyWorldEventsStoreError::InvalidLocalPlayerKey(
                    player_key.0,
                ));
            }
            player
                .validate()
                .map_err(|source| HeavyWorldEventsStoreError::InvalidPlayer {
                    player_key: *player_key,
                    source,
                })?;
        }
        Ok(())
    }

    pub fn normalize(&mut self) -> HeavyWorldEventsStoreNormalizationReport {
        let mut report = HeavyWorldEventsStoreNormalizationReport::default();
        if self.schema_version != HEAVY_WORLD_EVENTS_BRIDGE_SCHEMA_VERSION {
            self.schema_version = HEAVY_WORLD_EVENTS_BRIDGE_SCHEMA_VERSION;
            report.schema_updated = true;
        }
        report.clock_repaired = self.clock.normalize();
        if self.campaigns.is_empty() {
            self.campaigns
                .insert(HeavyCampaignKey::OFFLINE_MAIN, InvasionDirector::default());
            report.default_campaign_inserted = true;
        }
        for (key, campaign) in &mut self.campaigns {
            report.campaign_reports.insert(*key, campaign.normalize());
        }

        let previous_player_count = self.players.len();
        self.players.retain(|key, _| key.is_valid());
        report.invalid_players_removed = previous_player_count - self.players.len();
        for (key, player) in &mut self.players {
            report.player_reports.insert(*key, player.normalize());
        }
        report.clock_repaired |= self.reconcile_clock_with_persisted_state();
        report
    }

    fn latest_persisted_epoch_seconds(&self) -> u64 {
        let campaign_timestamp = self
            .campaigns
            .values()
            .flat_map(|campaign| {
                let active_started = campaign
                    .active_invasion
                    .map(|active| active.started_at_epoch_seconds);
                campaign
                    .zones
                    .values()
                    .flat_map(|zone| {
                        [
                            zone.liberated_at_epoch_seconds,
                            zone.peace_started_at_epoch_seconds,
                            zone.last_resolved_at_epoch_seconds,
                        ]
                        .into_iter()
                        .flatten()
                    })
                    .chain(active_started)
            })
            .max()
            .unwrap_or_default();
        let player_timestamp = self
            .players
            .values()
            .flat_map(|player| {
                [
                    player.field_manual.first_opened_at_epoch_seconds,
                    player.field_manual.first_run_completed_at_epoch_seconds,
                ]
                .into_iter()
                .flatten()
            })
            .max()
            .unwrap_or_default();
        campaign_timestamp.max(player_timestamp)
    }

    fn reconcile_clock_with_persisted_state(&mut self) -> bool {
        let required_ms = self.latest_persisted_epoch_seconds().saturating_mul(1_000);
        if self.clock.game_time_ms >= required_ms {
            return false;
        }
        self.clock.game_time_ms = required_ms;
        true
    }
}

// ---------------------------------------------------------------------------
// Invasion authority seam

#[derive(Resource, Debug, Default)]
pub struct HeavyWorldEventsBridgeState {
    settlement_strengths: BTreeMap<HeavyCampaignKey, u32>,
    pub active_local_players: BTreeSet<HeavyLocalPlayerKey>,
    pub duplicate_local_players: BTreeSet<HeavyLocalPlayerKey>,
    pub rejected_player_indices: BTreeSet<u8>,
    announced_first_run: BTreeSet<HeavyLocalPlayerKey>,
    pub fixed_ticks: u64,
    pub store_normalizations: u64,
    pub last_store_normalization: Option<HeavyWorldEventsStoreNormalizationReport>,
}

impl HeavyWorldEventsBridgeState {
    pub fn settlement_strength(&self, campaign_key: HeavyCampaignKey) -> u32 {
        self.settlement_strengths
            .get(&campaign_key)
            .copied()
            .unwrap_or_default()
    }

    pub fn settlement_strengths(&self) -> &BTreeMap<HeavyCampaignKey, u32> {
        &self.settlement_strengths
    }

    pub fn first_run_announced(&self, player_key: HeavyLocalPlayerKey) -> bool {
        self.announced_first_run.contains(&player_key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavySettlementStrengthInputs {
    pub base_strength: u32,
    pub helper_count: u32,
    pub captured_creature_count: u32,
}

impl HeavySettlementStrengthInputs {
    pub fn total(self) -> u32 {
        InvasionDirector::settlement_strength(
            self.base_strength,
            self.helper_count,
            self.captured_creature_count,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeavyInvasionRequestAction {
    /// A fact supplied by Starfall's authoritative campaign director.
    CampaignLevelCompleted { world_level: u8, final_clear: bool },
    /// A fact supplied by the authoritative fortress/raid encounter.
    BossFortressCleared { current_world_level: u8 },
    /// A fact supplied by the authoritative Swarms encounter.
    SwarmsGeneralDefeated,
    /// Explicit defense completion from the canonical raid adapter.
    Resolve {
        zone_id: crate::world::heavy_world_events::WorldZoneId,
        reason: InvasionResolutionReason,
    },
    /// One-time compatibility import from the Heavy Water campaign save.
    HydrateLegacy {
        world_level: Option<u8>,
        swarms_general_defeated: bool,
    },
    /// Updates a runtime provider; it does not persist a second copy of
    /// canonical base, companion, or creature state.
    ReportSettlementStrength(HeavySettlementStrengthInputs),
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HeavyInvasionRequest {
    pub request_id: u64,
    pub campaign_key: HeavyCampaignKey,
    pub action: HeavyInvasionRequestAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeavyWorldEventsAdapterError {
    Domain(WorldEventDomainError),
    InvalidLocalPlayerKey(u8),
}

impl From<WorldEventDomainError> for HeavyWorldEventsAdapterError {
    fn from(source: WorldEventDomainError) -> Self {
        Self::Domain(source)
    }
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HeavyInvasionRequestOutcome {
    pub request_id: u64,
    pub campaign_key: HeavyCampaignKey,
    pub result: Result<Vec<InvasionEvent>, HeavyWorldEventsAdapterError>,
}

/// Every domain lifecycle result is forwarded in source order. Consumers can
/// translate these into UI/audio/save requests without reading mutable state.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HeavyInvasionLifecycleMessage {
    pub campaign_key: HeavyCampaignKey,
    pub triggering_request_id: Option<u64>,
    pub event: InvasionEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeavyInvasionEncounterKey {
    pub campaign_key: HeavyCampaignKey,
    pub zone_id: crate::world::heavy_world_events::WorldZoneId,
    pub started_at_epoch_seconds: u64,
}

impl HeavyInvasionEncounterKey {
    pub fn stable_id(self) -> String {
        format!(
            "{}:invasion:{}:{}",
            self.campaign_key.stable_id(),
            self.zone_id.stable_id(),
            self.started_at_epoch_seconds
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyEncounterAuthority {
    /// The descriptor is advisory. Existing Starfall campaign/raid systems
    /// retain spawn, completion, reward, and cleanup authority.
    StarfallCampaignAndRaid,
}

/// Spawn-free request for the canonical encounter director. The descriptor
/// contains no ECS identity and is stable across save/load.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyInvasionEncounterDescriptor {
    pub encounter_key: HeavyInvasionEncounterKey,
    pub authority: HeavyEncounterAuthority,
    pub world_level: u8,
    pub threat_level: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyWorldEventsCheckpointReason {
    Warning,
    Started,
    Resolved,
}

/// Mirrors Heavy Water's force-save boundary without directly invoking the
/// Starfall save system or any secure-online service.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyWorldEventsCheckpointRequested {
    pub campaign_key: HeavyCampaignKey,
    pub reason: HeavyWorldEventsCheckpointReason,
}

pub fn invasion_encounter_descriptor(
    campaign_key: HeavyCampaignKey,
    event: &InvasionEvent,
) -> Option<HeavyInvasionEncounterDescriptor> {
    let InvasionEvent::Started {
        zone_id,
        world_level,
        threat_level,
        started_at_epoch_seconds,
    } = *event
    else {
        return None;
    };
    Some(HeavyInvasionEncounterDescriptor {
        encounter_key: HeavyInvasionEncounterKey {
            campaign_key,
            zone_id,
            started_at_epoch_seconds,
        },
        authority: HeavyEncounterAuthority::StarfallCampaignAndRaid,
        world_level,
        threat_level,
    })
}

pub fn invasion_checkpoint_request(
    campaign_key: HeavyCampaignKey,
    event: &InvasionEvent,
) -> Option<HeavyWorldEventsCheckpointRequested> {
    let reason = match event {
        InvasionEvent::Warning { .. } => HeavyWorldEventsCheckpointReason::Warning,
        InvasionEvent::Started { .. } => HeavyWorldEventsCheckpointReason::Started,
        InvasionEvent::Resolved { .. } => HeavyWorldEventsCheckpointReason::Resolved,
        InvasionEvent::ZoneStateChanged { .. } | InvasionEvent::RebuildEraUnlocked { .. } => {
            return None;
        }
    };
    Some(HeavyWorldEventsCheckpointRequested {
        campaign_key,
        reason,
    })
}

/// Pure request adapter. It records only domain changes and runtime settlement
/// strength; it cannot touch chapters, raids, entities, inventory, or rewards.
pub fn apply_heavy_invasion_request(
    store: &mut HeavyWorldEventsStore,
    bridge: &mut HeavyWorldEventsBridgeState,
    request: &HeavyInvasionRequest,
) -> HeavyInvasionRequestOutcome {
    let now_epoch_seconds = store.clock.now_epoch_seconds();
    let result = match request.action {
        HeavyInvasionRequestAction::ReportSettlementStrength(inputs) => {
            bridge
                .settlement_strengths
                .insert(request.campaign_key, inputs.total());
            Ok(Vec::new())
        }
        HeavyInvasionRequestAction::CampaignLevelCompleted {
            world_level,
            final_clear,
        } => store
            .campaign_mut(request.campaign_key)
            .complete_level(world_level, final_clear, now_epoch_seconds)
            .map_err(HeavyWorldEventsAdapterError::from),
        HeavyInvasionRequestAction::BossFortressCleared {
            current_world_level,
        } => Ok(store
            .campaign_mut(request.campaign_key)
            .boss_fortress_cleared(current_world_level, now_epoch_seconds)),
        HeavyInvasionRequestAction::SwarmsGeneralDefeated => Ok(store
            .campaign_mut(request.campaign_key)
            .swarms_general_defeated(now_epoch_seconds)),
        HeavyInvasionRequestAction::Resolve { zone_id, reason } => Ok(store
            .campaign_mut(request.campaign_key)
            .resolve_invasion(zone_id, reason, now_epoch_seconds)),
        HeavyInvasionRequestAction::HydrateLegacy {
            world_level,
            swarms_general_defeated,
        } => Ok(store
            .campaign_mut(request.campaign_key)
            .hydrate_legacy_progress(world_level, swarms_general_defeated, now_epoch_seconds)),
    };

    HeavyInvasionRequestOutcome {
        request_id: request.request_id,
        campaign_key: request.campaign_key,
        result,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeavyScheduledInvasionBatch {
    pub clock: HeavyWorldEventClockAdvance,
    pub events: Vec<HeavyInvasionLifecycleMessage>,
}

/// Advances all campaigns in stable key order. The provided strength map is a
/// read-only projection from canonical gameplay state.
pub fn advance_heavy_invasion_schedule(
    store: &mut HeavyWorldEventsStore,
    settlement_strengths: &BTreeMap<HeavyCampaignKey, u32>,
    delta: Duration,
) -> HeavyScheduledInvasionBatch {
    let clock = store.clock.advance(delta);
    if clock.elapsed_ms == 0 {
        return HeavyScheduledInvasionBatch {
            clock,
            events: Vec::new(),
        };
    }

    let mut events = Vec::new();
    for (campaign_key, director) in &mut store.campaigns {
        let strength = settlement_strengths
            .get(campaign_key)
            .copied()
            .unwrap_or_default();
        events.extend(
            director
                .tick(clock.elapsed_ms, clock.now_epoch_seconds, strength)
                .into_iter()
                .map(|event| HeavyInvasionLifecycleMessage {
                    campaign_key: *campaign_key,
                    triggering_request_id: None,
                    event,
                }),
        );
    }

    HeavyScheduledInvasionBatch { clock, events }
}

// ---------------------------------------------------------------------------
// Friendly-NPC proximity and dialogue seam

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeavyNpcRequestAction {
    /// The world adapter reports player position only while the authored cast
    /// is mounted. `cast_present = false` releases any stale focus on teardown.
    SampleProximity {
        player_position: WorldEventPoint,
        cast_present: bool,
    },
    Advance {
        input_blocked: bool,
    },
    Dismiss,
}

#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyNpcRequest {
    pub request_id: u64,
    pub player_key: HeavyLocalPlayerKey,
    pub action: HeavyNpcRequestAction,
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HeavyNpcRequestOutcome {
    pub request_id: u64,
    pub player_key: HeavyLocalPlayerKey,
    pub result: Result<Vec<NpcDialogueEvent>, HeavyWorldEventsAdapterError>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeavyNpcPresentation {
    FocusChanged {
        previous: Option<FriendlyNpcId>,
        current: Option<FriendlyNpcId>,
    },
    DialogueLine {
        npc_id: FriendlyNpcId,
        npc_stable_id: &'static str,
        display_name: &'static str,
        line_index: u8,
        line_count: u8,
        text: &'static str,
        is_last_line: bool,
    },
    Closed {
        npc_id: FriendlyNpcId,
        reason: DialogueCloseReason,
    },
}

/// Rendering/UI descriptor only. It contains a stable player key and authored
/// data, never the player or NPC ECS entity that a presentation layer may use.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct HeavyNpcPresentationDescriptor {
    pub player_key: HeavyLocalPlayerKey,
    pub presentation: HeavyNpcPresentation,
}

pub fn release_heavy_npc_focus(state: &mut NpcDialogueState) -> Vec<NpcDialogueEvent> {
    let Some(previous) = state.focused_npc else {
        state.dialogue_open = false;
        state.dialogue_index = 0;
        return Vec::new();
    };

    let mut events = Vec::new();
    if state.dialogue_open {
        events.push(NpcDialogueEvent::Closed {
            npc_id: previous,
            reason: DialogueCloseReason::FocusLost,
        });
    }
    state.focused_npc = None;
    state.dialogue_open = false;
    state.dialogue_index = 0;
    events.push(NpcDialogueEvent::FocusChanged {
        previous: Some(previous),
        current: None,
    });
    events
}

pub fn npc_presentation_descriptor(
    player_key: HeavyLocalPlayerKey,
    event: &NpcDialogueEvent,
) -> Option<HeavyNpcPresentationDescriptor> {
    let presentation = match *event {
        NpcDialogueEvent::FocusChanged { previous, current } => {
            HeavyNpcPresentation::FocusChanged { previous, current }
        }
        NpcDialogueEvent::Opened { npc_id, line_index }
        | NpcDialogueEvent::Advanced { npc_id, line_index } => {
            let definition = npc_id.definition();
            let text = definition.dialogue.get(usize::from(line_index)).copied()?;
            let line_count = definition.dialogue.len() as u8;
            HeavyNpcPresentation::DialogueLine {
                npc_id,
                npc_stable_id: npc_id.stable_id(),
                display_name: definition.display_name,
                line_index,
                line_count,
                text,
                is_last_line: line_index.saturating_add(1) == line_count,
            }
        }
        NpcDialogueEvent::Closed { npc_id, reason } => {
            HeavyNpcPresentation::Closed { npc_id, reason }
        }
    };
    Some(HeavyNpcPresentationDescriptor {
        player_key,
        presentation,
    })
}

pub fn apply_heavy_npc_request(
    store: &mut HeavyWorldEventsStore,
    request: &HeavyNpcRequest,
) -> HeavyNpcRequestOutcome {
    if !request.player_key.is_valid() {
        return HeavyNpcRequestOutcome {
            request_id: request.request_id,
            player_key: request.player_key,
            result: Err(HeavyWorldEventsAdapterError::InvalidLocalPlayerKey(
                request.player_key.0,
            )),
        };
    }

    let state = &mut store.player_mut(request.player_key).npc_dialogue;
    let result = match request.action {
        HeavyNpcRequestAction::SampleProximity {
            player_position,
            cast_present: true,
        } => state
            .update_focus(player_position)
            .map_err(HeavyWorldEventsAdapterError::from),
        HeavyNpcRequestAction::SampleProximity {
            cast_present: false,
            ..
        } => Ok(release_heavy_npc_focus(state)),
        HeavyNpcRequestAction::Advance { input_blocked } => {
            Ok(state.advance(input_blocked).into_iter().collect())
        }
        HeavyNpcRequestAction::Dismiss => Ok(state.dismiss().into_iter().collect()),
    };

    HeavyNpcRequestOutcome {
        request_id: request.request_id,
        player_key: request.player_key,
        result,
    }
}

// ---------------------------------------------------------------------------
// Field-manual / first-run seam

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyFieldManualRequestAction {
    Open,
    ViewSection(GuideSectionId),
    CompleteFirstRun,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyFieldManualRequest {
    pub request_id: u64,
    pub player_key: HeavyLocalPlayerKey,
    pub action: HeavyFieldManualRequestAction,
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HeavyFieldManualRequestOutcome {
    pub request_id: u64,
    pub player_key: HeavyLocalPlayerKey,
    pub result: Result<FieldManualEvent, HeavyWorldEventsAdapterError>,
    pub first_run_complete: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyFirstRunGuideRequired {
    pub player_key: HeavyLocalPlayerKey,
    pub initial_section: GuideSectionId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeavyFieldManualPresentationDescriptor {
    pub player_key: HeavyLocalPlayerKey,
    pub event: FieldManualEvent,
    pub first_run_complete: bool,
}

pub fn apply_heavy_field_manual_request(
    store: &mut HeavyWorldEventsStore,
    request: &HeavyFieldManualRequest,
) -> HeavyFieldManualRequestOutcome {
    if !request.player_key.is_valid() {
        return HeavyFieldManualRequestOutcome {
            request_id: request.request_id,
            player_key: request.player_key,
            result: Err(HeavyWorldEventsAdapterError::InvalidLocalPlayerKey(
                request.player_key.0,
            )),
            first_run_complete: false,
        };
    }

    let now_epoch_seconds = store.clock.now_epoch_seconds();
    let progress = &mut store.player_mut(request.player_key).field_manual;
    let event = match request.action {
        HeavyFieldManualRequestAction::Open => progress.open(now_epoch_seconds),
        HeavyFieldManualRequestAction::ViewSection(section) => {
            progress.view_section(section, now_epoch_seconds)
        }
        HeavyFieldManualRequestAction::CompleteFirstRun => {
            progress.complete_first_run(now_epoch_seconds)
        }
    };
    let first_run_complete = progress.first_run_complete();

    HeavyFieldManualRequestOutcome {
        request_id: request.request_id,
        player_key: request.player_key,
        result: Ok(event),
        first_run_complete,
    }
}

// ---------------------------------------------------------------------------
// Bevy scheduling

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeavyWorldEventsUpdateSet {
    BindStablePlayers,
    ApplyInvasionRequests,
    ApplyNpcRequests,
    ApplyFieldManualRequests,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeavyWorldEventsFixedSet {
    AdvancePersistentClockAndInvasions,
}

pub struct HeavyWorldEventsPlugin;

impl Plugin for HeavyWorldEventsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeavyWorldEventsStore>()
            .init_resource::<HeavyWorldEventsBridgeState>()
            .add_message::<HeavyInvasionRequest>()
            .add_message::<HeavyInvasionRequestOutcome>()
            .add_message::<HeavyInvasionLifecycleMessage>()
            .add_message::<HeavyInvasionEncounterDescriptor>()
            .add_message::<HeavyWorldEventsCheckpointRequested>()
            .add_message::<HeavyNpcRequest>()
            .add_message::<HeavyNpcRequestOutcome>()
            .add_message::<HeavyNpcPresentationDescriptor>()
            .add_message::<HeavyFieldManualRequest>()
            .add_message::<HeavyFieldManualRequestOutcome>()
            .add_message::<HeavyFieldManualPresentationDescriptor>()
            .add_message::<HeavyFirstRunGuideRequired>()
            .add_systems(
                OnEnter(AppState::Playing),
                normalize_heavy_world_events_store.in_set(PlayingSetupSet::InitializeDomains),
            )
            .configure_sets(
                Update,
                (
                    HeavyWorldEventsUpdateSet::BindStablePlayers,
                    HeavyWorldEventsUpdateSet::ApplyInvasionRequests,
                    HeavyWorldEventsUpdateSet::ApplyNpcRequests,
                    HeavyWorldEventsUpdateSet::ApplyFieldManualRequests,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                bind_heavy_world_event_player_keys
                    .in_set(HeavyWorldEventsUpdateSet::BindStablePlayers)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                process_heavy_invasion_requests
                    .in_set(HeavyWorldEventsUpdateSet::ApplyInvasionRequests)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                process_heavy_npc_requests
                    .in_set(HeavyWorldEventsUpdateSet::ApplyNpcRequests)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                process_heavy_field_manual_requests
                    .in_set(HeavyWorldEventsUpdateSet::ApplyFieldManualRequests)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                FixedUpdate,
                tick_heavy_invasion_schedule
                    .in_set(HeavyWorldEventsFixedSet::AdvancePersistentClockAndInvasions)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

/// Every loaded store crosses the domain normalization boundary before any
/// Playing-frame request or fixed invasion tick can observe it.
pub fn normalize_heavy_world_events_store(
    mut store: ResMut<HeavyWorldEventsStore>,
    mut bridge: ResMut<HeavyWorldEventsBridgeState>,
) {
    let report = store.normalize();
    bridge.store_normalizations = bridge.store_normalizations.saturating_add(1);
    bridge.last_store_normalization = Some(report);
}

pub fn bind_heavy_world_event_player_keys(
    mut commands: Commands,
    players: Query<(Entity, &PlayerIndex, Option<&HeavyWorldEventPlayerKey>), With<Player>>,
    mut store: ResMut<HeavyWorldEventsStore>,
    mut bridge: ResMut<HeavyWorldEventsBridgeState>,
    mut first_run_required: MessageWriter<HeavyFirstRunGuideRequired>,
) {
    let mut valid = Vec::new();
    let mut counts = BTreeMap::<HeavyLocalPlayerKey, usize>::new();
    bridge.rejected_player_indices.clear();

    for (entity, player_index, current_key) in &players {
        let Some(expected_key) = HeavyLocalPlayerKey::from_player_index(player_index.0) else {
            bridge.rejected_player_indices.insert(player_index.0);
            if current_key.is_some() {
                commands.entity(entity).remove::<HeavyWorldEventPlayerKey>();
            }
            continue;
        };
        *counts.entry(expected_key).or_default() += 1;
        valid.push((entity, expected_key, current_key.copied()));
    }

    bridge.active_local_players = counts.keys().copied().collect();
    bridge.duplicate_local_players = counts
        .iter()
        .filter_map(|(key, count)| (*count > 1).then_some(*key))
        .collect();

    // Query order cannot affect persistence: duplicate entities converge on
    // one stable player record and are diagnosed above.
    valid.sort_by_key(|(_, key, _)| *key);
    for (entity, key, current_key) in valid {
        if current_key.map(|current| current.0) != Some(key) {
            commands
                .entity(entity)
                .insert(HeavyWorldEventPlayerKey(key));
        }

        let progress = store.player_mut(key);
        if !progress.field_manual.first_run_complete() && bridge.announced_first_run.insert(key) {
            first_run_required.write(HeavyFirstRunGuideRequired {
                player_key: key,
                initial_section: progress.field_manual.active_section,
            });
        }
    }
}

pub fn process_heavy_invasion_requests(
    mut store: ResMut<HeavyWorldEventsStore>,
    mut bridge: ResMut<HeavyWorldEventsBridgeState>,
    mut requests: MessageReader<HeavyInvasionRequest>,
    mut outcomes: MessageWriter<HeavyInvasionRequestOutcome>,
    mut lifecycle: MessageWriter<HeavyInvasionLifecycleMessage>,
    mut encounters: MessageWriter<HeavyInvasionEncounterDescriptor>,
    mut checkpoints: MessageWriter<HeavyWorldEventsCheckpointRequested>,
) {
    for request in requests.read() {
        let outcome = apply_heavy_invasion_request(&mut store, &mut bridge, request);
        if let Ok(events) = &outcome.result {
            for event in events {
                lifecycle.write(HeavyInvasionLifecycleMessage {
                    campaign_key: request.campaign_key,
                    triggering_request_id: Some(request.request_id),
                    event: event.clone(),
                });
                if let Some(descriptor) = invasion_encounter_descriptor(request.campaign_key, event)
                {
                    encounters.write(descriptor);
                }
                if let Some(checkpoint) = invasion_checkpoint_request(request.campaign_key, event) {
                    checkpoints.write(checkpoint);
                }
            }
        }
        outcomes.write(outcome);
    }
}

pub fn process_heavy_npc_requests(
    mut store: ResMut<HeavyWorldEventsStore>,
    mut requests: MessageReader<HeavyNpcRequest>,
    mut outcomes: MessageWriter<HeavyNpcRequestOutcome>,
    mut presentations: MessageWriter<HeavyNpcPresentationDescriptor>,
) {
    for request in requests.read() {
        let outcome = apply_heavy_npc_request(&mut store, request);
        if let Ok(events) = &outcome.result {
            for event in events {
                if let Some(descriptor) = npc_presentation_descriptor(request.player_key, event) {
                    presentations.write(descriptor);
                }
            }
        }
        outcomes.write(outcome);
    }
}

pub fn process_heavy_field_manual_requests(
    mut store: ResMut<HeavyWorldEventsStore>,
    mut requests: MessageReader<HeavyFieldManualRequest>,
    mut outcomes: MessageWriter<HeavyFieldManualRequestOutcome>,
    mut presentations: MessageWriter<HeavyFieldManualPresentationDescriptor>,
) {
    for request in requests.read() {
        let outcome = apply_heavy_field_manual_request(&mut store, request);
        if let Ok(event) = outcome.result {
            presentations.write(HeavyFieldManualPresentationDescriptor {
                player_key: request.player_key,
                event,
                first_run_complete: outcome.first_run_complete,
            });
        }
        outcomes.write(outcome);
    }
}

pub fn tick_heavy_invasion_schedule(
    fixed_time: Res<Time<Fixed>>,
    mut store: ResMut<HeavyWorldEventsStore>,
    mut bridge: ResMut<HeavyWorldEventsBridgeState>,
    mut lifecycle: MessageWriter<HeavyInvasionLifecycleMessage>,
    mut encounters: MessageWriter<HeavyInvasionEncounterDescriptor>,
    mut checkpoints: MessageWriter<HeavyWorldEventsCheckpointRequested>,
) {
    let batch = advance_heavy_invasion_schedule(
        &mut store,
        &bridge.settlement_strengths,
        fixed_time.delta(),
    );
    bridge.fixed_ticks = bridge.fixed_ticks.saturating_add(1);

    for message in batch.events {
        if let Some(descriptor) =
            invasion_encounter_descriptor(message.campaign_key, &message.event)
        {
            encounters.write(descriptor);
        }
        if let Some(checkpoint) = invasion_checkpoint_request(message.campaign_key, &message.event)
        {
            checkpoints.write(checkpoint);
        }
        lifecycle.write(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::heavy_world_events::{
        InvasionStrategicReward, WorldZoneId, ZoneState, INITIAL_PEACE_MS, REPEAT_PEACE_MS,
        WARNING_DURATION_MS,
    };
    use bevy::state::app::StatesPlugin;

    fn campaign_completion_request(
        request_id: u64,
        campaign_key: HeavyCampaignKey,
        world_level: u8,
    ) -> HeavyInvasionRequest {
        HeavyInvasionRequest {
            request_id,
            campaign_key,
            action: HeavyInvasionRequestAction::CampaignLevelCompleted {
                world_level,
                final_clear: false,
            },
        }
    }

    fn unlock_campaign(
        store: &mut HeavyWorldEventsStore,
        bridge: &mut HeavyWorldEventsBridgeState,
        campaign_key: HeavyCampaignKey,
    ) {
        for world_level in 1..=3 {
            let outcome = apply_heavy_invasion_request(
                store,
                bridge,
                &campaign_completion_request(u64::from(world_level), campaign_key, world_level),
            );
            assert!(outcome.result.is_ok());
        }
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<AppState>()
            .add_plugins(HeavyWorldEventsPlugin);
        app
    }

    fn all_messages<M: Message + Clone>(app: &App) -> Vec<M> {
        let messages = app.world().resource::<Messages<M>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).cloned().collect()
    }

    #[test]
    fn local_and_campaign_keys_are_stable_and_never_entities() {
        assert_eq!(
            HeavyLocalPlayerKey::from_player_index(0)
                .unwrap()
                .stable_id(),
            "local:p1"
        );
        assert_eq!(
            HeavyLocalPlayerKey::from_player_index(3)
                .unwrap()
                .stable_id(),
            "local:p4"
        );
        assert_eq!(HeavyLocalPlayerKey::from_player_index(4), None);
        assert_eq!(
            HeavyCampaignKey::OFFLINE_MAIN.stable_id(),
            "offline:campaign:0"
        );

        let mut store = HeavyWorldEventsStore::default();
        store.player_mut(HeavyLocalPlayerKey::PLAYER_ONE);
        let json = serde_json::to_string(&store).unwrap();
        assert!(!json.contains("Entity"));
        assert!(!json.contains("index_and_generation"));
        let decoded: HeavyWorldEventsStore = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, store);
        decoded.validate().unwrap();
    }

    #[test]
    fn persistent_clock_retains_fractional_time_and_is_split_invariant() {
        let mut split = HeavyWorldEventClock::default();
        let first = split.advance(Duration::from_nanos(500_500));
        assert_eq!(first.elapsed_ms, 0);
        assert_eq!(split.submillisecond_nanos(), 500_500);
        let second = split.advance(Duration::from_nanos(500_500));
        assert_eq!(second.elapsed_ms, 1);
        assert_eq!(split.game_time_ms, 1);
        assert_eq!(split.submillisecond_nanos(), 1_000);

        let mut combined = HeavyWorldEventClock::default();
        combined.advance(Duration::from_nanos(1_001_000));
        assert_eq!(split, combined);

        let mut malformed = HeavyWorldEventClock::from_parts(4, 2_500_001);
        assert_eq!(malformed.game_time_ms, 6);
        assert_eq!(malformed.submillisecond_nanos(), 500_001);
        assert!(!malformed.normalize());
        assert!(malformed.validate());
    }

    #[test]
    fn store_clock_catches_up_to_imported_domain_timestamps() {
        let mut store = HeavyWorldEventsStore::default();
        store
            .campaign_mut(HeavyCampaignKey::OFFLINE_MAIN)
            .complete_level(1, false, 500)
            .unwrap();
        assert!(matches!(
            store.validate(),
            Err(HeavyWorldEventsStoreError::ClockPrecedesPersistedState {
                clock_ms: 0,
                required_ms: 500_000,
            })
        ));
        let report = store.normalize();
        assert!(report.clock_repaired);
        assert_eq!(store.clock.game_time_ms, 500_000);
        store.validate().unwrap();
    }

    #[test]
    fn store_scopes_global_campaign_and_per_player_progress() {
        let mut store = HeavyWorldEventsStore::default();
        let p1 = HeavyLocalPlayerKey::from_player_index(0).unwrap();
        let p2 = HeavyLocalPlayerKey::from_player_index(1).unwrap();
        store.player_mut(p1).field_manual.complete_first_run(10);
        store.player_mut(p2).field_manual.open(11);
        store
            .campaign_mut(HeavyCampaignKey::OFFLINE_MAIN)
            .complete_level(1, false, 12)
            .unwrap();

        let p1_snapshot = store
            .domain_snapshot(HeavyCampaignKey::OFFLINE_MAIN, p1)
            .unwrap();
        let p2_snapshot = store
            .domain_snapshot(HeavyCampaignKey::OFFLINE_MAIN, p2)
            .unwrap();
        assert_eq!(p1_snapshot.invasion, p2_snapshot.invasion);
        assert!(p1_snapshot.field_manual.first_run_complete());
        assert!(!p2_snapshot.field_manual.first_run_complete());

        let alternate_campaign = HeavyCampaignKey(7);
        let mut imported = p1_snapshot.clone();
        imported.invasion.complete_level(2, false, 20).unwrap();
        store.apply_domain_snapshot(alternate_campaign, p2, imported.clone());
        assert_eq!(
            store
                .domain_snapshot(alternate_campaign, p2)
                .unwrap()
                .invasion,
            imported.invasion
        );
        store.validate().unwrap();
    }

    #[test]
    fn invasion_requests_accept_facts_without_taking_campaign_authority() {
        let mut store = HeavyWorldEventsStore::default();
        let mut bridge = HeavyWorldEventsBridgeState::default();
        let key = HeavyCampaignKey::OFFLINE_MAIN;

        let strength = apply_heavy_invasion_request(
            &mut store,
            &mut bridge,
            &HeavyInvasionRequest {
                request_id: 1,
                campaign_key: key,
                action: HeavyInvasionRequestAction::ReportSettlementStrength(
                    HeavySettlementStrengthInputs {
                        base_strength: 10,
                        helper_count: 4,
                        captured_creature_count: 100,
                    },
                ),
            },
        );
        assert_eq!(strength.result, Ok(Vec::new()));
        assert_eq!(bridge.settlement_strength(key), 38);

        let invalid = apply_heavy_invasion_request(
            &mut store,
            &mut bridge,
            &campaign_completion_request(2, key, 99),
        );
        assert!(matches!(
            invalid.result,
            Err(HeavyWorldEventsAdapterError::Domain(
                WorldEventDomainError::UnknownWorldLevel(99)
            ))
        ));

        unlock_campaign(&mut store, &mut bridge, key);
        assert!(store.campaign(key).unwrap().main_campaign_complete);
        assert_eq!(store.players.len(), 0);
    }

    #[test]
    fn scheduled_invasions_are_split_invariant_and_emit_spawn_free_descriptor() {
        let key = HeavyCampaignKey::OFFLINE_MAIN;
        let mut one_step = HeavyWorldEventsStore::default();
        let mut split = HeavyWorldEventsStore::default();
        let mut bridge = HeavyWorldEventsBridgeState::default();
        unlock_campaign(&mut one_step, &mut bridge, key);
        unlock_campaign(&mut split, &mut bridge, key);

        let strengths = BTreeMap::from([(key, 36)]);
        let warning = advance_heavy_invasion_schedule(
            &mut one_step,
            &strengths,
            Duration::from_millis(INITIAL_PEACE_MS),
        );
        assert!(warning.events.iter().any(|message| matches!(
            message.event,
            InvasionEvent::Warning {
                zone_id: WorldZoneId::DetroitStarCity,
                threat_level: 1,
                ..
            }
        )));

        assert!(advance_heavy_invasion_schedule(
            &mut split,
            &strengths,
            Duration::from_millis(INITIAL_PEACE_MS / 2),
        )
        .events
        .is_empty());
        let split_warning = advance_heavy_invasion_schedule(
            &mut split,
            &strengths,
            Duration::from_millis(INITIAL_PEACE_MS / 2),
        );
        assert_eq!(split_warning.events, warning.events);
        assert_eq!(split, one_step);

        let started = advance_heavy_invasion_schedule(
            &mut one_step,
            &strengths,
            Duration::from_millis(WARNING_DURATION_MS),
        );
        let start_event = started
            .events
            .iter()
            .find_map(|message| {
                matches!(message.event, InvasionEvent::Started { .. }).then_some(&message.event)
            })
            .unwrap();
        let descriptor = invasion_encounter_descriptor(key, start_event).unwrap();
        assert_eq!(
            descriptor.authority,
            HeavyEncounterAuthority::StarfallCampaignAndRaid
        );
        assert_eq!(
            descriptor.encounter_key.zone_id,
            WorldZoneId::DetroitStarCity
        );
        assert_eq!(
            descriptor.encounter_key.stable_id(),
            "offline:campaign:0:invasion:detroit_star_city:1020"
        );
        assert_eq!(
            one_step.clock.game_time_ms,
            INITIAL_PEACE_MS + WARNING_DURATION_MS
        );
    }

    #[test]
    fn lifecycle_events_request_checkpoints_without_direct_save_calls() {
        let key = HeavyCampaignKey(3);
        let warning = InvasionEvent::Warning {
            zone_id: WorldZoneId::OrbitalFront,
            world_level: 5,
            threat_level: 4,
            warning_duration_ms: WARNING_DURATION_MS,
        };
        assert_eq!(
            invasion_checkpoint_request(key, &warning),
            Some(HeavyWorldEventsCheckpointRequested {
                campaign_key: key,
                reason: HeavyWorldEventsCheckpointReason::Warning,
            })
        );
        assert!(invasion_encounter_descriptor(key, &warning).is_none());

        let resolved = InvasionEvent::Resolved {
            zone_id: WorldZoneId::OrbitalFront,
            world_level: 5,
            reason: InvasionResolutionReason::Defended,
            reward: InvasionStrategicReward {
                victories_gained: 1,
                peace_restored: true,
                purification_granted: true,
                next_threat_level: 5,
                next_peace_window_ms: REPEAT_PEACE_MS,
            },
        };
        assert_eq!(
            invasion_checkpoint_request(key, &resolved).unwrap().reason,
            HeavyWorldEventsCheckpointReason::Resolved
        );
        assert!(invasion_encounter_descriptor(key, &resolved).is_none());
    }

    #[test]
    fn npc_adapter_acquires_advances_describes_and_releases_by_player_key() {
        let mut store = HeavyWorldEventsStore::default();
        let player_key = HeavyLocalPlayerKey::from_player_index(2).unwrap();
        let acquire = apply_heavy_npc_request(
            &mut store,
            &HeavyNpcRequest {
                request_id: 10,
                player_key,
                action: HeavyNpcRequestAction::SampleProximity {
                    player_position: FriendlyNpcId::CaptainIris.definition().position,
                    cast_present: true,
                },
            },
        );
        assert!(matches!(
            acquire.result.as_deref(),
            Ok([NpcDialogueEvent::FocusChanged {
                current: Some(FriendlyNpcId::CaptainIris),
                ..
            }])
        ));

        let opened = apply_heavy_npc_request(
            &mut store,
            &HeavyNpcRequest {
                request_id: 11,
                player_key,
                action: HeavyNpcRequestAction::Advance {
                    input_blocked: false,
                },
            },
        );
        let open_event = opened.result.as_ref().unwrap().first().unwrap();
        let descriptor = npc_presentation_descriptor(player_key, open_event).unwrap();
        assert!(matches!(
            descriptor.presentation,
            HeavyNpcPresentation::DialogueLine {
                npc_stable_id: "captain_iris",
                display_name: "CAPT. IRIS",
                line_index: 0,
                line_count: 4,
                text: "Welcome to Heavy Water, pilot.",
                ..
            }
        ));

        let released = apply_heavy_npc_request(
            &mut store,
            &HeavyNpcRequest {
                request_id: 12,
                player_key,
                action: HeavyNpcRequestAction::SampleProximity {
                    player_position: WorldEventPoint::default(),
                    cast_present: false,
                },
            },
        );
        assert!(matches!(
            released.result.as_deref(),
            Ok([
                NpcDialogueEvent::Closed {
                    reason: DialogueCloseReason::FocusLost,
                    ..
                },
                NpcDialogueEvent::FocusChanged { current: None, .. }
            ])
        ));
        assert!(store
            .player(player_key)
            .unwrap()
            .npc_dialogue
            .focused_npc
            .is_none());
    }

    #[test]
    fn field_manual_completion_is_durable_per_local_player() {
        let mut store = HeavyWorldEventsStore::default();
        store.clock.game_time_ms = 42_000;
        let p1 = HeavyLocalPlayerKey::from_player_index(0).unwrap();
        let p2 = HeavyLocalPlayerKey::from_player_index(1).unwrap();

        let opened = apply_heavy_field_manual_request(
            &mut store,
            &HeavyFieldManualRequest {
                request_id: 20,
                player_key: p1,
                action: HeavyFieldManualRequestAction::Open,
            },
        );
        assert!(matches!(
            opened.result,
            Ok(FieldManualEvent::Opened {
                first_open: true,
                ..
            })
        ));
        let completed = apply_heavy_field_manual_request(
            &mut store,
            &HeavyFieldManualRequest {
                request_id: 21,
                player_key: p1,
                action: HeavyFieldManualRequestAction::CompleteFirstRun,
            },
        );
        assert!(completed.first_run_complete);
        assert_eq!(
            store
                .player(p1)
                .unwrap()
                .field_manual
                .first_run_completed_at_epoch_seconds,
            Some(42)
        );
        assert!(!store.player_mut(p2).field_manual.first_run_complete());

        let encoded = serde_json::to_string(&store).unwrap();
        let decoded: HeavyWorldEventsStore = serde_json::from_str(&encoded).unwrap();
        assert!(decoded
            .player(p1)
            .unwrap()
            .field_manual
            .first_run_complete());
        assert!(!decoded
            .player(p2)
            .unwrap()
            .field_manual
            .first_run_complete());
    }

    #[test]
    fn normalization_repairs_clock_domains_and_drops_invalid_local_scope() {
        let mut store: HeavyWorldEventsStore = serde_json::from_str(
            r#"{
                "schema_version": 0,
                "clock": { "game_time_ms": 5, "submillisecond_nanos": 2000001 },
                "campaigns": { "2": { "schema_version": 0, "zones": {} } },
                "players": { "9": {}, "0": { "npc_dialogue": { "dialogue_open": true } } }
            }"#,
        )
        .unwrap();
        assert!(store.validate().is_err());
        let report = store.normalize();
        assert!(report.schema_updated);
        assert!(report.clock_repaired);
        assert_eq!(report.invalid_players_removed, 1);
        assert_eq!(
            report.campaign_reports[&HeavyCampaignKey(2)].zones_inserted,
            crate::world::heavy_world_events::HEAVY_REBUILD_ZONE_COUNT as u16
        );
        assert!(
            report.player_reports[&HeavyLocalPlayerKey::PLAYER_ONE]
                .npc_dialogue
                .invalid_open_dialogue_closed
        );
        store.validate().unwrap();
    }

    #[test]
    fn app_binding_is_playing_gated_stable_and_announces_first_run_once() {
        let mut app = test_app();
        let p1 = app.world_mut().spawn((Player, PlayerIndex(0))).id();
        let p1_duplicate = app.world_mut().spawn((Player, PlayerIndex(0))).id();
        let rejected = app.world_mut().spawn((Player, PlayerIndex(9))).id();

        app.update();
        assert!(app.world().get::<HeavyWorldEventPlayerKey>(p1).is_none());

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();

        assert_eq!(
            app.world().get::<HeavyWorldEventPlayerKey>(p1),
            Some(&HeavyWorldEventPlayerKey(HeavyLocalPlayerKey::PLAYER_ONE))
        );
        assert_eq!(
            app.world().get::<HeavyWorldEventPlayerKey>(p1_duplicate),
            Some(&HeavyWorldEventPlayerKey(HeavyLocalPlayerKey::PLAYER_ONE))
        );
        assert!(app
            .world()
            .get::<HeavyWorldEventPlayerKey>(rejected)
            .is_none());
        let bridge = app.world().resource::<HeavyWorldEventsBridgeState>();
        assert_eq!(bridge.store_normalizations, 1);
        assert!(bridge.last_store_normalization.is_some());
        assert_eq!(
            bridge.active_local_players,
            BTreeSet::from([HeavyLocalPlayerKey::PLAYER_ONE])
        );
        assert_eq!(
            bridge.duplicate_local_players,
            BTreeSet::from([HeavyLocalPlayerKey::PLAYER_ONE])
        );
        assert_eq!(bridge.rejected_player_indices, BTreeSet::from([9]));
        assert_eq!(
            all_messages::<HeavyFirstRunGuideRequired>(&app),
            vec![HeavyFirstRunGuideRequired {
                player_key: HeavyLocalPlayerKey::PLAYER_ONE,
                initial_section: GuideSectionId::Movement,
            }]
        );
        app.update();
        assert_eq!(all_messages::<HeavyFirstRunGuideRequired>(&app).len(), 1);
    }

    #[test]
    fn app_processes_typed_requests_but_only_emits_encounter_descriptors() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();
        let entity_count_before = app.world().entities().len();
        let key = HeavyCampaignKey::OFFLINE_MAIN;

        for level in 1..=3 {
            app.world_mut()
                .resource_mut::<Messages<HeavyInvasionRequest>>()
                .write(campaign_completion_request(u64::from(level), key, level));
        }
        app.world_mut()
            .resource_mut::<Messages<HeavyNpcRequest>>()
            .write(HeavyNpcRequest {
                request_id: 50,
                player_key: HeavyLocalPlayerKey::PLAYER_ONE,
                action: HeavyNpcRequestAction::SampleProximity {
                    player_position: FriendlyNpcId::CaptainIris.definition().position,
                    cast_present: true,
                },
            });
        app.world_mut()
            .resource_mut::<Messages<HeavyFieldManualRequest>>()
            .write(HeavyFieldManualRequest {
                request_id: 51,
                player_key: HeavyLocalPlayerKey::PLAYER_ONE,
                action: HeavyFieldManualRequestAction::CompleteFirstRun,
            });
        app.update();

        assert!(
            app.world()
                .resource::<HeavyWorldEventsStore>()
                .campaign(key)
                .unwrap()
                .main_campaign_complete
        );
        assert_eq!(all_messages::<HeavyInvasionRequestOutcome>(&app).len(), 3);
        assert!(all_messages::<HeavyNpcPresentationDescriptor>(&app)
            .iter()
            .any(|descriptor| matches!(
                descriptor.presentation,
                HeavyNpcPresentation::FocusChanged {
                    current: Some(FriendlyNpcId::CaptainIris),
                    ..
                }
            )));
        assert!(all_messages::<HeavyFieldManualPresentationDescriptor>(&app)
            .iter()
            .any(|descriptor| descriptor.first_run_complete));
        assert_eq!(app.world().entities().len(), entity_count_before);

        // The plugin has no chapter/raid resources in this minimal app; its
        // success proves requests do not require or mutate those authorities.
        assert!(all_messages::<HeavyInvasionEncounterDescriptor>(&app).is_empty());
    }

    #[test]
    fn app_holds_requests_until_playing_instead_of_mutating_from_menu() {
        let mut app = test_app();
        app.world_mut()
            .resource_mut::<Messages<HeavyInvasionRequest>>()
            .write(campaign_completion_request(
                77,
                HeavyCampaignKey::OFFLINE_MAIN,
                1,
            ));
        app.update();
        assert_eq!(
            app.world()
                .resource::<HeavyWorldEventsStore>()
                .campaign(HeavyCampaignKey::OFFLINE_MAIN)
                .unwrap()
                .zone(WorldZoneId::DetroitStarCity)
                .state,
            ZoneState::Uncleared
        );

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();
        assert_eq!(
            app.world()
                .resource::<HeavyWorldEventsStore>()
                .campaign(HeavyCampaignKey::OFFLINE_MAIN)
                .unwrap()
                .zone(WorldZoneId::DetroitStarCity)
                .state,
            ZoneState::Peaceful
        );
    }
}
