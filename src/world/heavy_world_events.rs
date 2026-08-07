//! Offline Heavy Water world-event, friendly-NPC, and field-manual domain.
//!
//! The TypeScript source mixed durable rules with Babylon/React presentation
//! and a process-global event bus.  This module keeps the authored data and
//! deterministic state machines while returning plain outcomes for Bevy
//! adapters to translate into encounters, UI, audio, and save requests.
//!
//! Time deltas are integer milliseconds so replay and save/load behavior do
//! not depend on frame-rate floating-point accumulation.  Wall-clock stamps
//! are supplied by the caller and remain integer epoch seconds, matching the
//! source snapshots.  No online authority or cloud-save policy lives here.

#![allow(dead_code)] // Runtime/UI adapters are deliberately a later port seam.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const HEAVY_WORLD_EVENTS_SCHEMA_VERSION: u16 = 1;

pub const HEAVY_REBUILD_ZONE_COUNT: usize = 11;
pub const HEAVY_INVASION_ELIGIBLE_ZONE_COUNT: usize = 9;
pub const HEAVY_ZONE_STATE_COUNT: usize = 6;
pub const HEAVY_INVASION_LIFECYCLE_EVENT_COUNT: usize = 4;

pub const INITIAL_PEACE_MS: u64 = 15 * 60 * 1_000;
pub const REPEAT_PEACE_MS: u64 = 20 * 60 * 1_000;
pub const WARNING_DURATION_MS: u64 = 2 * 60 * 1_000;
pub const CAMPAIGN_UNLOCK_MINIMUM_PEACE_MS: u64 = 10 * 60 * 1_000;
pub const SETTLEMENT_STRENGTH_STEP: u32 = 18;
pub const MAX_SETTLEMENT_THREAT_DAMPENER: u32 = 2;
pub const HELPER_STRENGTH_MULTIPLIER: u32 = 2;
pub const MAX_CAPTURED_CREATURE_STRENGTH: u32 = 20;

pub const HEAVY_FRIENDLY_NPC_COUNT: usize = 6;
pub const HEAVY_FRIENDLY_PALETTE_COUNT: usize = 6;
pub const HEAVY_NPC_DIALOGUE_LINE_COUNT: usize = 24;
pub const NPC_INTERACT_RANGE_METERS: f32 = 5.5;
pub const NPC_STAY_RANGE_METERS: f32 = 9.0;
pub const NPC_FACE_RANGE_METERS: f32 = 20.0;
pub const NPC_DEFAULT_BOB_SPEED_RADIANS_PER_SECOND: f32 = 1.6;
pub const NPC_BOB_AMPLITUDE_METERS: f32 = 0.06;
pub const NPC_FACE_TURN_RATE_PER_SECOND: f32 = 4.0;
pub const NPC_HEAD_UI_OFFSET_METERS: f32 = 2.1;

pub const HEAVY_GUIDE_SECTION_COUNT: usize = 11;
pub const HEAVY_GUIDE_ROW_COUNT: usize = 41;

fn current_schema_version() -> u16 {
    HEAVY_WORLD_EVENTS_SCHEMA_VERSION
}

fn initial_peace_ms() -> u64 {
    INITIAL_PEACE_MS
}

/// Stable source ids for the eleven Heavy Water rebuild fronts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldZoneId {
    DetroitStarCity,
    DetroitHoldLine,
    DetroitVoid,
    AshurSanctuary,
    OrbitalFront,
    PontiacLab,
    SwarmsLair,
    SaginawLab,
    ZugIsland,
    AnnArbor,
    MichiganWilds,
}

impl WorldZoneId {
    pub const ALL: [Self; HEAVY_REBUILD_ZONE_COUNT] = [
        Self::DetroitStarCity,
        Self::DetroitHoldLine,
        Self::DetroitVoid,
        Self::AshurSanctuary,
        Self::OrbitalFront,
        Self::PontiacLab,
        Self::SwarmsLair,
        Self::SaginawLab,
        Self::ZugIsland,
        Self::AnnArbor,
        Self::MichiganWilds,
    ];

    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::DetroitStarCity => "detroit_star_city",
            Self::DetroitHoldLine => "detroit_hold_line",
            Self::DetroitVoid => "detroit_void",
            Self::AshurSanctuary => "ashur_sanctuary",
            Self::OrbitalFront => "orbital_front",
            Self::PontiacLab => "pontiac_lab",
            Self::SwarmsLair => "swarms_lair",
            Self::SaginawLab => "saginaw_lab",
            Self::ZugIsland => "zug_island",
            Self::AnnArbor => "ann_arbor",
            Self::MichiganWilds => "michigan_wilds",
        }
    }

    pub const fn world_level(self) -> u8 {
        match self {
            Self::DetroitStarCity => 1,
            Self::DetroitHoldLine => 2,
            Self::DetroitVoid => 3,
            Self::AshurSanctuary => 4,
            Self::OrbitalFront => 5,
            Self::PontiacLab => 6,
            Self::SwarmsLair => 7,
            Self::SaginawLab => 8,
            Self::ZugIsland => 9,
            Self::AnnArbor => 10,
            Self::MichiganWilds => 11,
        }
    }

    pub fn from_world_level(level: u8) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|zone| zone.world_level() == level)
    }

    pub const fn definition(self) -> &'static RebuildZoneDefinition {
        &REBUILD_ZONES[(self.world_level() - 1) as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebuildZoneDefinition {
    pub id: WorldZoneId,
    pub world_level: u8,
    pub name: &'static str,
    pub invasion_eligible: bool,
    pub base_threat: u32,
}

pub static REBUILD_ZONES: [RebuildZoneDefinition; HEAVY_REBUILD_ZONE_COUNT] = [
    RebuildZoneDefinition {
        id: WorldZoneId::DetroitStarCity,
        world_level: 1,
        name: "Detroit Star City Front",
        invasion_eligible: true,
        base_threat: 1,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::DetroitHoldLine,
        world_level: 2,
        name: "Detroit Hold the Line",
        invasion_eligible: true,
        base_threat: 2,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::DetroitVoid,
        world_level: 3,
        name: "Detroit Void Front",
        invasion_eligible: true,
        base_threat: 3,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::AshurSanctuary,
        world_level: 4,
        name: "Ashur Sanctuary",
        invasion_eligible: false,
        base_threat: 1,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::OrbitalFront,
        world_level: 5,
        name: "Orbital Front",
        invasion_eligible: true,
        base_threat: 4,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::PontiacLab,
        world_level: 6,
        name: "Pontiac Secret Lab",
        invasion_eligible: false,
        base_threat: 2,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::SwarmsLair,
        world_level: 7,
        name: "Swarms Lair",
        invasion_eligible: true,
        base_threat: 5,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::SaginawLab,
        world_level: 8,
        name: "Saginaw Underwater Lab",
        invasion_eligible: true,
        base_threat: 5,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::ZugIsland,
        world_level: 9,
        name: "Zug Island Legion",
        invasion_eligible: true,
        base_threat: 6,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::AnnArbor,
        world_level: 10,
        name: "Ann Arbor Apocalypse",
        invasion_eligible: true,
        base_threat: 6,
    },
    RebuildZoneDefinition {
        id: WorldZoneId::MichiganWilds,
        world_level: 11,
        name: "Michigan Wildlands",
        invasion_eligible: true,
        base_threat: 7,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneState {
    Uncleared,
    Liberated,
    Peaceful,
    Threatened,
    Invaded,
    Purified,
}

impl ZoneState {
    pub const ALL: [Self; HEAVY_ZONE_STATE_COUNT] = [
        Self::Uncleared,
        Self::Liberated,
        Self::Peaceful,
        Self::Threatened,
        Self::Invaded,
        Self::Purified,
    ];

    pub const fn can_be_invaded(self) -> bool {
        matches!(self, Self::Liberated | Self::Peaceful | Self::Purified)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneProgress {
    pub id: WorldZoneId,
    pub world_level: u8,
    pub state: ZoneState,
    pub threat_level: u32,
    pub victories: u32,
    pub liberated_at_epoch_seconds: Option<u64>,
    pub peace_started_at_epoch_seconds: Option<u64>,
    pub last_resolved_at_epoch_seconds: Option<u64>,
}

impl ZoneProgress {
    pub fn from_definition(definition: &RebuildZoneDefinition) -> Self {
        Self {
            id: definition.id,
            world_level: definition.world_level,
            state: ZoneState::Uncleared,
            threat_level: definition.base_threat,
            victories: 0,
            liberated_at_epoch_seconds: None,
            peace_started_at_epoch_seconds: None,
            last_resolved_at_epoch_seconds: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveInvasion {
    pub zone_id: WorldZoneId,
    pub world_level: u8,
    pub threat_level: u32,
    pub started_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvasionResolutionReason {
    Defended,
    FortressCleared,
    GeneralDefeated,
}

/// The source grants strategic progress, not credits or inventory items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvasionStrategicReward {
    pub victories_gained: u32,
    pub peace_restored: bool,
    pub purification_granted: bool,
    pub next_threat_level: u32,
    pub next_peace_window_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum InvasionEvent {
    ZoneStateChanged {
        zone_id: WorldZoneId,
        previous_state: ZoneState,
        state: ZoneState,
        threat_level: u32,
        victories: u32,
    },
    Warning {
        zone_id: WorldZoneId,
        world_level: u8,
        threat_level: u32,
        warning_duration_ms: u64,
    },
    Started {
        zone_id: WorldZoneId,
        world_level: u8,
        threat_level: u32,
        started_at_epoch_seconds: u64,
    },
    Resolved {
        zone_id: WorldZoneId,
        world_level: u8,
        reason: InvasionResolutionReason,
        reward: InvasionStrategicReward,
    },
    /// The source announces this through its UI message channel; keeping it
    /// as a plain outcome lets an adapter choose the presentation.
    RebuildEraUnlocked { newly_unlocked: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvasionDirector {
    #[serde(default = "current_schema_version")]
    pub schema_version: u16,
    #[serde(default)]
    pub main_campaign_complete: bool,
    #[serde(default = "initial_peace_ms")]
    pub next_invasion_in_ms: u64,
    #[serde(default)]
    pub warning_zone_id: Option<WorldZoneId>,
    #[serde(default)]
    pub warning_elapsed_ms: u64,
    #[serde(default)]
    pub active_invasion: Option<ActiveInvasion>,
    #[serde(default)]
    pub zones: BTreeMap<WorldZoneId, ZoneProgress>,
}

impl Default for InvasionDirector {
    fn default() -> Self {
        let zones = REBUILD_ZONES
            .iter()
            .map(|definition| (definition.id, ZoneProgress::from_definition(definition)))
            .collect();
        Self {
            schema_version: HEAVY_WORLD_EVENTS_SCHEMA_VERSION,
            main_campaign_complete: false,
            next_invasion_in_ms: INITIAL_PEACE_MS,
            warning_zone_id: None,
            warning_elapsed_ms: 0,
            active_invasion: None,
            zones,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldEventDomainError {
    UnknownWorldLevel(u8),
    NonFinitePlayerPosition,
    InvalidState(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InvasionNormalizationReport {
    pub schema_updated: bool,
    pub zones_inserted: u16,
    pub zone_identity_repairs: u16,
    pub threat_repairs: u16,
    pub active_invasion_cleared: bool,
    pub warning_cleared: bool,
    pub warning_timer_clamped: bool,
    pub transient_zone_repairs: u16,
}

impl InvasionDirector {
    pub fn zone(&self, zone_id: WorldZoneId) -> &ZoneProgress {
        // A normalized/default director always has the complete authored map.
        // This accessor deliberately fails loudly if an adapter skipped
        // normalization after decoding a hand-edited partial save.
        self.zones
            .get(&zone_id)
            .expect("invasion director is missing an authored zone; call normalize()")
    }

    pub fn active_invasion(&self) -> Option<ActiveInvasion> {
        self.active_invasion
    }

    pub fn calculate_threat_level(&self, zone_id: WorldZoneId, settlement_strength: u32) -> u32 {
        let zone = self.zone(zone_id);
        let dampener =
            (settlement_strength / SETTLEMENT_STRENGTH_STEP).min(MAX_SETTLEMENT_THREAT_DAMPENER);
        zone_id
            .definition()
            .base_threat
            .saturating_add(zone.victories)
            .saturating_sub(dampener)
            .max(1)
    }

    /// Source-equivalent strength provider helper:
    /// base + helper_count * 2 + min(20, captured_creatures).
    pub fn settlement_strength(
        base_strength: u32,
        helper_count: u32,
        captured_creature_count: u32,
    ) -> u32 {
        base_strength
            .saturating_add(helper_count.saturating_mul(HELPER_STRENGTH_MULTIPLIER))
            .saturating_add(captured_creature_count.min(MAX_CAPTURED_CREATURE_STRENGTH))
    }

    /// Advances at most one source state-machine branch per call.  In
    /// particular, excess time that reaches zero on the peace timer does not
    /// also consume warning time, matching the TypeScript `return`s.
    pub fn tick(
        &mut self,
        elapsed_ms: u64,
        now_epoch_seconds: u64,
        settlement_strength: u32,
    ) -> Vec<InvasionEvent> {
        if !self.main_campaign_complete || elapsed_ms == 0 || self.active_invasion.is_some() {
            return Vec::new();
        }

        if let Some(zone_id) = self.warning_zone_id {
            self.warning_elapsed_ms = self.warning_elapsed_ms.saturating_add(elapsed_ms);
            if self.warning_elapsed_ms < WARNING_DURATION_MS {
                return Vec::new();
            }

            self.warning_zone_id = None;
            self.warning_elapsed_ms = 0;
            return self.start_invasion(zone_id, now_epoch_seconds);
        }

        self.next_invasion_in_ms = self.next_invasion_in_ms.saturating_sub(elapsed_ms);
        if self.next_invasion_in_ms > 0 {
            return Vec::new();
        }

        let Some(target) = self.choose_invasion_target() else {
            self.next_invasion_in_ms = REPEAT_PEACE_MS;
            return Vec::new();
        };
        self.start_warning(target, settlement_strength)
    }

    pub fn complete_level(
        &mut self,
        world_level: u8,
        final_clear: bool,
        now_epoch_seconds: u64,
    ) -> Result<Vec<InvasionEvent>, WorldEventDomainError> {
        let zone_id = WorldZoneId::from_world_level(world_level)
            .ok_or(WorldEventDomainError::UnknownWorldLevel(world_level))?;
        let mut events = self.mark_zone_liberated(zone_id, false, now_epoch_seconds);

        if world_level == 3 || final_clear {
            let newly_unlocked = !self.main_campaign_complete;
            self.main_campaign_complete = true;
            self.next_invasion_in_ms = self
                .next_invasion_in_ms
                .max(CAMPAIGN_UNLOCK_MINIMUM_PEACE_MS);
            events.push(InvasionEvent::RebuildEraUnlocked { newly_unlocked });
        }
        Ok(events)
    }

    pub fn boss_fortress_cleared(
        &mut self,
        current_world_level: u8,
        now_epoch_seconds: u64,
    ) -> Vec<InvasionEvent> {
        let Some(active) = self.active_invasion else {
            return Vec::new();
        };
        if active.world_level != current_world_level {
            return Vec::new();
        }
        self.resolve_invasion(
            active.zone_id,
            InvasionResolutionReason::FortressCleared,
            now_epoch_seconds,
        )
    }

    pub fn swarms_general_defeated(&mut self, now_epoch_seconds: u64) -> Vec<InvasionEvent> {
        let swarms = WorldZoneId::SwarmsLair;
        let mut events = self.mark_zone_liberated(swarms, true, now_epoch_seconds);
        if self.active_invasion.map(|active| active.zone_id) == Some(swarms) {
            events.extend(self.resolve_invasion(
                swarms,
                InvasionResolutionReason::GeneralDefeated,
                now_epoch_seconds,
            ));
        }
        events
    }

    /// Migrates the two legacy campaign checkpoints used by the source save.
    pub fn hydrate_legacy_progress(
        &mut self,
        world_level: Option<u8>,
        swarms_general_defeated: bool,
        now_epoch_seconds: u64,
    ) -> Vec<InvasionEvent> {
        let mut events = Vec::new();
        match world_level {
            Some(2) => events.extend(self.mark_zone_liberated(
                WorldZoneId::DetroitStarCity,
                false,
                now_epoch_seconds,
            )),
            Some(3) => {
                events.extend(self.mark_zone_liberated(
                    WorldZoneId::DetroitStarCity,
                    false,
                    now_epoch_seconds,
                ));
                events.extend(self.mark_zone_liberated(
                    WorldZoneId::DetroitHoldLine,
                    false,
                    now_epoch_seconds,
                ));
            }
            _ => {}
        }
        if swarms_general_defeated {
            events.extend(self.mark_zone_liberated(
                WorldZoneId::SwarmsLair,
                true,
                now_epoch_seconds,
            ));
        }
        events
    }

    pub fn resolve_invasion(
        &mut self,
        zone_id: WorldZoneId,
        reason: InvasionResolutionReason,
        now_epoch_seconds: u64,
    ) -> Vec<InvasionEvent> {
        let definition = zone_id.definition();
        let Some(zone) = self.zones.get_mut(&zone_id) else {
            return Vec::new();
        };
        if !matches!(zone.state, ZoneState::Invaded | ZoneState::Threatened) {
            return Vec::new();
        }

        let previous_state = zone.state;
        zone.state = ZoneState::Purified;
        zone.victories = zone.victories.saturating_add(1);
        zone.threat_level = definition
            .base_threat
            .max(zone.threat_level.saturating_add(1));
        zone.last_resolved_at_epoch_seconds = Some(now_epoch_seconds);
        zone.peace_started_at_epoch_seconds = Some(now_epoch_seconds);
        let threat_level = zone.threat_level;
        let victories = zone.victories;

        if self.active_invasion.map(|active| active.zone_id) == Some(zone_id) {
            self.active_invasion = None;
        }
        if self.warning_zone_id == Some(zone_id) {
            self.warning_zone_id = None;
            self.warning_elapsed_ms = 0;
        }
        self.next_invasion_in_ms = REPEAT_PEACE_MS;

        vec![
            InvasionEvent::ZoneStateChanged {
                zone_id,
                previous_state,
                state: ZoneState::Purified,
                threat_level,
                victories,
            },
            InvasionEvent::Resolved {
                zone_id,
                world_level: definition.world_level,
                reason,
                reward: InvasionStrategicReward {
                    victories_gained: 1,
                    peace_restored: true,
                    purification_granted: true,
                    next_threat_level: threat_level,
                    next_peace_window_ms: REPEAT_PEACE_MS,
                },
            },
        ]
    }

    pub fn validate(&self) -> Result<(), WorldEventDomainError> {
        if self.schema_version != HEAVY_WORLD_EVENTS_SCHEMA_VERSION {
            return Err(WorldEventDomainError::InvalidState(format!(
                "unsupported world-event schema {}",
                self.schema_version
            )));
        }
        if self.zones.len() != HEAVY_REBUILD_ZONE_COUNT {
            return Err(WorldEventDomainError::InvalidState(format!(
                "expected {HEAVY_REBUILD_ZONE_COUNT} invasion zones, found {}",
                self.zones.len()
            )));
        }
        for definition in &REBUILD_ZONES {
            let Some(zone) = self.zones.get(&definition.id) else {
                return Err(WorldEventDomainError::InvalidState(format!(
                    "missing invasion zone {}",
                    definition.id.stable_id()
                )));
            };
            if zone.id != definition.id || zone.world_level != definition.world_level {
                return Err(WorldEventDomainError::InvalidState(format!(
                    "identity mismatch for invasion zone {}",
                    definition.id.stable_id()
                )));
            }
            if zone.threat_level == 0 {
                return Err(WorldEventDomainError::InvalidState(format!(
                    "zero threat for invasion zone {}",
                    definition.id.stable_id()
                )));
            }
        }
        if self.warning_elapsed_ms > WARNING_DURATION_MS {
            return Err(WorldEventDomainError::InvalidState(
                "warning timer exceeds authored duration".to_owned(),
            ));
        }
        if self.warning_zone_id.is_none() && self.warning_elapsed_ms != 0 {
            return Err(WorldEventDomainError::InvalidState(
                "warning timer exists without a warning zone".to_owned(),
            ));
        }
        if self.warning_zone_id.is_some() && self.active_invasion.is_some() {
            return Err(WorldEventDomainError::InvalidState(
                "warning and active invasion overlap".to_owned(),
            ));
        }
        if let Some(zone_id) = self.warning_zone_id {
            if !self.main_campaign_complete
                || !zone_id.definition().invasion_eligible
                || self.zone(zone_id).state != ZoneState::Threatened
            {
                return Err(WorldEventDomainError::InvalidState(
                    "invalid warning-zone gate".to_owned(),
                ));
            }
        }
        if let Some(active) = self.active_invasion {
            if !self.main_campaign_complete
                || !active.zone_id.definition().invasion_eligible
                || active.world_level != active.zone_id.world_level()
                || active.threat_level == 0
                || self.zone(active.zone_id).state != ZoneState::Invaded
                || self.zone(active.zone_id).threat_level != active.threat_level
            {
                return Err(WorldEventDomainError::InvalidState(
                    "invalid active-invasion gate".to_owned(),
                ));
            }
        }
        for zone in self.zones.values() {
            if zone.state == ZoneState::Threatened && self.warning_zone_id != Some(zone.id) {
                return Err(WorldEventDomainError::InvalidState(format!(
                    "stranded threatened zone {}",
                    zone.id.stable_id()
                )));
            }
            if zone.state == ZoneState::Invaded
                && self.active_invasion.map(|active| active.zone_id) != Some(zone.id)
            {
                return Err(WorldEventDomainError::InvalidState(format!(
                    "stranded invaded zone {}",
                    zone.id.stable_id()
                )));
            }
        }
        Ok(())
    }

    /// Repairs hand-edited, partial, and legacy snapshots into one canonical
    /// offline state. Unknown enum strings are rejected by serde before here.
    pub fn normalize(&mut self) -> InvasionNormalizationReport {
        let mut report = InvasionNormalizationReport::default();
        if self.schema_version != HEAVY_WORLD_EVENTS_SCHEMA_VERSION {
            self.schema_version = HEAVY_WORLD_EVENTS_SCHEMA_VERSION;
            report.schema_updated = true;
        }

        for definition in &REBUILD_ZONES {
            let zone = self.zones.entry(definition.id).or_insert_with(|| {
                report.zones_inserted = report.zones_inserted.saturating_add(1);
                ZoneProgress::from_definition(definition)
            });
            if zone.id != definition.id || zone.world_level != definition.world_level {
                zone.id = definition.id;
                zone.world_level = definition.world_level;
                report.zone_identity_repairs = report.zone_identity_repairs.saturating_add(1);
            }
            if zone.threat_level == 0 {
                zone.threat_level = 1;
                report.threat_repairs = report.threat_repairs.saturating_add(1);
            }
        }

        let active_is_valid = self.active_invasion.is_some_and(|active| {
            self.main_campaign_complete && active.zone_id.definition().invasion_eligible
        });
        if !active_is_valid && self.active_invasion.is_some() {
            self.active_invasion = None;
            report.active_invasion_cleared = true;
        }

        if let Some(mut active) = self.active_invasion {
            let zone = self
                .zones
                .get_mut(&active.zone_id)
                .expect("authored active zone was inserted above");
            active.world_level = active.zone_id.world_level();
            active.threat_level = active.threat_level.max(1);
            zone.state = ZoneState::Invaded;
            zone.threat_level = active.threat_level;
            self.active_invasion = Some(active);
            if self.warning_zone_id.is_some() || self.warning_elapsed_ms != 0 {
                report.warning_cleared = true;
            }
            self.warning_zone_id = None;
            self.warning_elapsed_ms = 0;
        } else {
            let warning_is_valid = self.warning_zone_id.is_some_and(|zone_id| {
                self.main_campaign_complete && zone_id.definition().invasion_eligible
            });
            if !warning_is_valid {
                if self.warning_zone_id.is_some() || self.warning_elapsed_ms != 0 {
                    report.warning_cleared = true;
                }
                self.warning_zone_id = None;
                self.warning_elapsed_ms = 0;
            } else if let Some(zone_id) = self.warning_zone_id {
                self.zones
                    .get_mut(&zone_id)
                    .expect("authored warning zone was inserted above")
                    .state = ZoneState::Threatened;
                if self.warning_elapsed_ms > WARNING_DURATION_MS {
                    self.warning_elapsed_ms = WARNING_DURATION_MS;
                    report.warning_timer_clamped = true;
                }
            }
        }

        let active_zone = self.active_invasion.map(|active| active.zone_id);
        for zone in self.zones.values_mut() {
            let stranded = (zone.state == ZoneState::Threatened
                && self.warning_zone_id != Some(zone.id))
                || (zone.state == ZoneState::Invaded && active_zone != Some(zone.id));
            if stranded {
                zone.state = settled_state_after_transient_repair(zone);
                report.transient_zone_repairs = report.transient_zone_repairs.saturating_add(1);
            }
        }
        report
    }

    fn mark_zone_liberated(
        &mut self,
        zone_id: WorldZoneId,
        purified: bool,
        now_epoch_seconds: u64,
    ) -> Vec<InvasionEvent> {
        let definition = zone_id.definition();
        let zone = self
            .zones
            .get_mut(&zone_id)
            .expect("default/normalized director contains every authored zone");
        let next_state = if purified {
            ZoneState::Purified
        } else {
            ZoneState::Peaceful
        };
        if matches!(zone.state, ZoneState::Invaded | ZoneState::Threatened)
            || zone.state == next_state
            || zone.state == ZoneState::Purified
        {
            return Vec::new();
        }

        let previous_state = zone.state;
        zone.state = next_state;
        zone.liberated_at_epoch_seconds =
            zone.liberated_at_epoch_seconds.or(Some(now_epoch_seconds));
        zone.peace_started_at_epoch_seconds = Some(now_epoch_seconds);
        zone.threat_level = zone.threat_level.max(definition.base_threat);
        vec![InvasionEvent::ZoneStateChanged {
            zone_id,
            previous_state,
            state: zone.state,
            threat_level: zone.threat_level,
            victories: zone.victories,
        }]
    }

    fn choose_invasion_target(&self) -> Option<WorldZoneId> {
        let mut candidates: Vec<&ZoneProgress> = self
            .zones
            .values()
            .filter(|zone| zone.id.definition().invasion_eligible && zone.state.can_be_invaded())
            .collect();
        candidates.sort_by_key(|zone| {
            (
                zone.last_resolved_at_epoch_seconds.unwrap_or(0),
                zone.world_level,
            )
        });
        candidates.first().map(|zone| zone.id)
    }

    fn start_warning(
        &mut self,
        zone_id: WorldZoneId,
        settlement_strength: u32,
    ) -> Vec<InvasionEvent> {
        let threat_level = self.calculate_threat_level(zone_id, settlement_strength);
        let definition = zone_id.definition();
        let zone = self
            .zones
            .get_mut(&zone_id)
            .expect("selected invasion target exists");
        let previous_state = zone.state;
        zone.state = ZoneState::Threatened;
        zone.threat_level = threat_level;
        self.warning_zone_id = Some(zone_id);
        self.warning_elapsed_ms = 0;
        vec![
            InvasionEvent::ZoneStateChanged {
                zone_id,
                previous_state,
                state: ZoneState::Threatened,
                threat_level,
                victories: zone.victories,
            },
            InvasionEvent::Warning {
                zone_id,
                world_level: definition.world_level,
                threat_level,
                warning_duration_ms: WARNING_DURATION_MS,
            },
        ]
    }

    fn start_invasion(
        &mut self,
        zone_id: WorldZoneId,
        now_epoch_seconds: u64,
    ) -> Vec<InvasionEvent> {
        let definition = zone_id.definition();
        let Some(zone) = self.zones.get_mut(&zone_id) else {
            return Vec::new();
        };
        let previous_state = zone.state;
        zone.state = ZoneState::Invaded;
        let active = ActiveInvasion {
            zone_id,
            world_level: definition.world_level,
            threat_level: zone.threat_level,
            started_at_epoch_seconds: now_epoch_seconds,
        };
        self.active_invasion = Some(active);
        vec![
            InvasionEvent::ZoneStateChanged {
                zone_id,
                previous_state,
                state: ZoneState::Invaded,
                threat_level: zone.threat_level,
                victories: zone.victories,
            },
            InvasionEvent::Started {
                zone_id,
                world_level: definition.world_level,
                threat_level: zone.threat_level,
                started_at_epoch_seconds: now_epoch_seconds,
            },
        ]
    }
}

fn settled_state_after_transient_repair(zone: &ZoneProgress) -> ZoneState {
    if zone.victories > 0 {
        ZoneState::Purified
    } else if zone.liberated_at_epoch_seconds.is_some() {
        ZoneState::Peaceful
    } else {
        ZoneState::Uncleared
    }
}

// ---------------------------------------------------------------------------
// Friendly cast and dialogue state

pub const NPC_HUMANOID_HEIGHT_METERS: f32 = 1.7;
pub const NPC_HEAD_SCALE: f32 = 0.42;
pub const NPC_SHOULDER_WIDTH_METERS: f32 = 0.55;
pub const NPC_CHEST_WIDTH_METERS: f32 = 0.6;
pub const NPC_ARM_LENGTH_METERS: f32 = 0.7;
pub const NPC_LEG_LENGTH_METERS: f32 = 0.85;
pub const NPC_EMISSIVE_SCALE: f32 = 0.45;
pub const NPC_HALO_DIAMETER_METERS: f32 = 1.4;
pub const NPC_HALO_THICKNESS_METERS: f32 = 0.08;
pub const NPC_HALO_HEIGHT_METERS: f32 = 0.08;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldEventPoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl WorldEventPoint {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn horizontal_distance_squared(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dz = self.z - other.z;
        dx * dx + dz * dz
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NpcRgb {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl NpcRgb {
    pub const fn new(red: f32, green: f32, blue: f32) -> Self {
        Self { red, green, blue }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FriendlyPalette {
    pub name: &'static str,
    pub primary: NpcRgb,
    pub secondary: NpcRgb,
    pub hair: NpcRgb,
}

pub static FRIENDLY_PALETTES: [FriendlyPalette; HEAVY_FRIENDLY_PALETTE_COUNT] = [
    FriendlyPalette {
        name: "Sunshine",
        primary: NpcRgb::new(1.0, 0.85, 0.15),
        secondary: NpcRgb::new(1.0, 0.55, 0.15),
        hair: NpcRgb::new(1.0, 0.95, 0.55),
    },
    FriendlyPalette {
        name: "Bubblegum",
        primary: NpcRgb::new(1.0, 0.45, 0.75),
        secondary: NpcRgb::new(1.0, 0.85, 0.95),
        hair: NpcRgb::new(1.0, 0.7, 0.95),
    },
    FriendlyPalette {
        name: "Aqua",
        primary: NpcRgb::new(0.25, 0.95, 0.95),
        secondary: NpcRgb::new(0.6, 1.0, 0.85),
        hair: NpcRgb::new(0.5, 0.95, 1.0),
    },
    FriendlyPalette {
        name: "Lime",
        primary: NpcRgb::new(0.55, 1.0, 0.35),
        secondary: NpcRgb::new(1.0, 1.0, 0.35),
        hair: NpcRgb::new(0.8, 1.0, 0.3),
    },
    FriendlyPalette {
        name: "Magenta",
        primary: NpcRgb::new(0.95, 0.35, 1.0),
        secondary: NpcRgb::new(0.55, 0.35, 1.0),
        hair: NpcRgb::new(1.0, 0.55, 1.0),
    },
    FriendlyPalette {
        name: "Orange",
        primary: NpcRgb::new(1.0, 0.55, 0.2),
        secondary: NpcRgb::new(1.0, 0.85, 0.4),
        hair: NpcRgb::new(1.0, 0.7, 0.3),
    },
];

pub const NPC_FRIENDLY_SKIN: NpcRgb = NpcRgb::new(1.0, 0.82, 0.7);
pub const NPC_HALO_DIFFUSE: NpcRgb = NpcRgb::new(1.0, 0.85, 0.25);
pub const NPC_HALO_EMISSIVE: NpcRgb = NpcRgb::new(1.0, 0.7, 0.2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendlyNpcId {
    CaptainIris,
    MerchantBex,
    EngineerJuno,
    RiderKazu,
    MysticOri,
    ScoutPip,
}

impl FriendlyNpcId {
    pub const ALL: [Self; HEAVY_FRIENDLY_NPC_COUNT] = [
        Self::CaptainIris,
        Self::MerchantBex,
        Self::EngineerJuno,
        Self::RiderKazu,
        Self::MysticOri,
        Self::ScoutPip,
    ];

    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::CaptainIris => "captain_iris",
            Self::MerchantBex => "merchant_bex",
            Self::EngineerJuno => "engineer_juno",
            Self::RiderKazu => "rider_kazu",
            Self::MysticOri => "mystic_ori",
            Self::ScoutPip => "scout_pip",
        }
    }

    pub const fn definition(self) -> &'static FriendlyNpcDefinition {
        &FRIENDLY_NPCS[match self {
            Self::CaptainIris => 0,
            Self::MerchantBex => 1,
            Self::EngineerJuno => 2,
            Self::RiderKazu => 3,
            Self::MysticOri => 4,
            Self::ScoutPip => 5,
        }]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FriendlyNpcDefinition {
    pub id: FriendlyNpcId,
    pub position: WorldEventPoint,
    pub display_name: &'static str,
    pub dialogue: &'static [&'static str],
    pub palette_index: usize,
}

pub static CAPTAIN_IRIS_DIALOGUE: [&str; 4] = [
    "Welcome to Heavy Water, pilot.",
    "Detroit is overrun with hybrid organoids — ground swarms, aerial fortresses, the works.",
    "Your job: defend the city. Push back, level up, get strong.",
    "Open the upgrade bay with TAB. Buy resources at any shop with E.",
];

pub static MERCHANT_BEX_DIALOGUE: [&str; 4] = [
    "Looking to spend some hard-earned credits?",
    "General shop sells gears, scrap, cores, circuits, nano fiber — every upgrade material.",
    "Weapon shop stocks the parts your guns need to level up.",
    "Sell what you don't need from the inventory tab.",
];

pub static ENGINEER_JUNO_DIALOGUE: [&str; 4] = [
    "Helper bots not pulling their weight?",
    "Open TAB → ROBOTS to level them up with gears + cores.",
    "Each bot also has a HELPER WEAPON tier — separate upgrade, much faster fire.",
    "Build new bots at the lab once you've got materials.",
];

pub static RIDER_KAZU_DIALOGUE: [&str; 4] = [
    "Hold SHIFT for two seconds — rocket skates engage.",
    "Top speed nearly doubles. Great for crossing the open biomes.",
    "Triple-jump tap into free flight. The sky racetrack is ringed with ramps.",
    "Press Y for the beam sabre — dash → slash for a signature combo.",
];

pub static MYSTIC_ORI_DIALOGUE: [&str; 4] = [
    "Six elementals sleep in your hands.",
    "Lightning, ice, fireball — they track. Flame, wind, psychic — they erupt around you.",
    "Cycle with the elemental hotkeys, cast with the dedicated trigger.",
    "Level them up in TAB → SPECIALS for radius and damage.",
];

pub static SCOUT_PIP_DIALOGUE: [&str; 4] = [
    "Beyond the city: four biomes, all hostile.",
    "Hybrid organoid bases, loot vaults, mining nodes — and flying fortresses overhead.",
    "Don't aggro the sky unless you're ready. Once routed, fortresses regroup for five minutes.",
    "Watch your minimap. Stay sharp out there.",
];

pub static FRIENDLY_NPCS: [FriendlyNpcDefinition; HEAVY_FRIENDLY_NPC_COUNT] = [
    FriendlyNpcDefinition {
        id: FriendlyNpcId::CaptainIris,
        position: WorldEventPoint::new(20.0, 0.0, 20.0),
        display_name: "CAPT. IRIS",
        dialogue: &CAPTAIN_IRIS_DIALOGUE,
        palette_index: 0,
    },
    FriendlyNpcDefinition {
        id: FriendlyNpcId::MerchantBex,
        position: WorldEventPoint::new(345.0, 0.0, 100.0),
        display_name: "MERCHANT BEX",
        dialogue: &MERCHANT_BEX_DIALOGUE,
        palette_index: 1,
    },
    FriendlyNpcDefinition {
        id: FriendlyNpcId::EngineerJuno,
        position: WorldEventPoint::new(-15.0, 0.0, 60.0),
        display_name: "ENGINEER JUNO",
        dialogue: &ENGINEER_JUNO_DIALOGUE,
        palette_index: 2,
    },
    FriendlyNpcDefinition {
        id: FriendlyNpcId::RiderKazu,
        position: WorldEventPoint::new(60.0, 0.0, -40.0),
        display_name: "RIDER KAZU",
        dialogue: &RIDER_KAZU_DIALOGUE,
        palette_index: 3,
    },
    FriendlyNpcDefinition {
        id: FriendlyNpcId::MysticOri,
        position: WorldEventPoint::new(-45.0, 0.0, -25.0),
        display_name: "MYSTIC ORI",
        dialogue: &MYSTIC_ORI_DIALOGUE,
        palette_index: 4,
    },
    FriendlyNpcDefinition {
        id: FriendlyNpcId::ScoutPip,
        position: WorldEventPoint::new(140.0, 0.0, 200.0),
        display_name: "SCOUT PIP",
        dialogue: &SCOUT_PIP_DIALOGUE,
        palette_index: 5,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogueCloseReason {
    Completed,
    FocusLost,
    PlayerDismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum NpcDialogueEvent {
    FocusChanged {
        previous: Option<FriendlyNpcId>,
        current: Option<FriendlyNpcId>,
    },
    Opened {
        npc_id: FriendlyNpcId,
        line_index: u8,
    },
    Advanced {
        npc_id: FriendlyNpcId,
        line_index: u8,
    },
    Closed {
        npc_id: FriendlyNpcId,
        reason: DialogueCloseReason,
    },
}

/// Presentation-independent counterpart to the source's focused NPC and
/// speech-bubble fields. It is serializable for save snapshots, although an
/// adapter may reasonably reset modal dialogue when loading a scene.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcDialogueState {
    #[serde(default)]
    pub focused_npc: Option<FriendlyNpcId>,
    #[serde(default)]
    pub dialogue_open: bool,
    #[serde(default)]
    pub dialogue_index: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NpcDialogueNormalizationReport {
    pub dialogue_index_clamped: bool,
    pub invalid_open_dialogue_closed: bool,
}

impl NpcDialogueState {
    pub fn current_line(&self) -> Option<&'static str> {
        if !self.dialogue_open {
            return None;
        }
        let npc = self.focused_npc?;
        npc.definition()
            .dialogue
            .get(usize::from(self.dialogue_index))
            .copied()
    }

    pub const fn prompt_available(&self, input_blocked: bool) -> bool {
        self.focused_npc.is_some() && !self.dialogue_open && !input_blocked
    }

    /// Applies the source's 5.5 m acquire / 9 m release hysteresis. Distance
    /// is horizontal, and exact ties favor the earlier authored cast entry.
    pub fn update_focus(
        &mut self,
        player_position: WorldEventPoint,
    ) -> Result<Vec<NpcDialogueEvent>, WorldEventDomainError> {
        if !player_position.is_finite() {
            return Err(WorldEventDomainError::NonFinitePlayerPosition);
        }

        let mut nearest = None;
        let mut nearest_distance_squared = f32::INFINITY;
        for definition in &FRIENDLY_NPCS {
            let distance_squared = definition
                .position
                .horizontal_distance_squared(player_position);
            if distance_squared < nearest_distance_squared {
                nearest = Some(definition.id);
                nearest_distance_squared = distance_squared;
            }
        }

        let previous = self.focused_npc;
        let mut next = previous;
        if let Some(focused) = previous {
            if Some(focused) != nearest
                || nearest_distance_squared > NPC_STAY_RANGE_METERS * NPC_STAY_RANGE_METERS
            {
                next = None;
            }
        }
        if next.is_none()
            && nearest_distance_squared <= NPC_INTERACT_RANGE_METERS * NPC_INTERACT_RANGE_METERS
        {
            next = nearest;
        }

        if next == previous {
            return Ok(Vec::new());
        }

        let mut events = Vec::new();
        if self.dialogue_open {
            if let Some(npc_id) = previous {
                events.push(NpcDialogueEvent::Closed {
                    npc_id,
                    reason: DialogueCloseReason::FocusLost,
                });
            }
        }
        self.dialogue_open = false;
        self.focused_npc = next;
        self.dialogue_index = 0;
        events.push(NpcDialogueEvent::FocusChanged {
            previous,
            current: next,
        });
        Ok(events)
    }

    /// Opens at line zero, advances one line per activation, then closes after
    /// the final line. Shop/menu ownership is represented by `input_blocked`.
    pub fn advance(&mut self, input_blocked: bool) -> Option<NpcDialogueEvent> {
        if input_blocked {
            return None;
        }
        let npc_id = self.focused_npc?;
        let line_count = npc_id.definition().dialogue.len() as u8;
        if !self.dialogue_open {
            self.dialogue_open = true;
            self.dialogue_index = 0;
            return Some(NpcDialogueEvent::Opened {
                npc_id,
                line_index: 0,
            });
        }

        self.dialogue_index = self.dialogue_index.saturating_add(1);
        if self.dialogue_index >= line_count {
            self.dialogue_open = false;
            Some(NpcDialogueEvent::Closed {
                npc_id,
                reason: DialogueCloseReason::Completed,
            })
        } else {
            Some(NpcDialogueEvent::Advanced {
                npc_id,
                line_index: self.dialogue_index,
            })
        }
    }

    pub fn dismiss(&mut self) -> Option<NpcDialogueEvent> {
        if !self.dialogue_open {
            return None;
        }
        self.dialogue_open = false;
        Some(NpcDialogueEvent::Closed {
            npc_id: self.focused_npc?,
            reason: DialogueCloseReason::PlayerDismissed,
        })
    }

    pub fn validate(&self) -> Result<(), WorldEventDomainError> {
        if self.dialogue_open && self.focused_npc.is_none() {
            return Err(WorldEventDomainError::InvalidState(
                "dialogue is open without a focused NPC".to_owned(),
            ));
        }
        if let Some(npc_id) = self.focused_npc {
            let line_count = npc_id.definition().dialogue.len() as u8;
            if self.dialogue_index > line_count
                || (self.dialogue_open && self.dialogue_index >= line_count)
            {
                return Err(WorldEventDomainError::InvalidState(format!(
                    "invalid dialogue index for {}",
                    npc_id.stable_id()
                )));
            }
        }
        Ok(())
    }

    pub fn normalize(&mut self) -> NpcDialogueNormalizationReport {
        let mut report = NpcDialogueNormalizationReport::default();
        let Some(npc_id) = self.focused_npc else {
            if self.dialogue_open {
                self.dialogue_open = false;
                report.invalid_open_dialogue_closed = true;
            }
            if self.dialogue_index != 0 {
                self.dialogue_index = 0;
                report.dialogue_index_clamped = true;
            }
            return report;
        };

        let line_count = npc_id.definition().dialogue.len() as u8;
        if self.dialogue_index > line_count {
            self.dialogue_index = line_count;
            report.dialogue_index_clamped = true;
        }
        if self.dialogue_open && self.dialogue_index >= line_count {
            self.dialogue_open = false;
            report.invalid_open_dialogue_closed = true;
        }
        report
    }
}

// ---------------------------------------------------------------------------
// Field manual and durable first-run acknowledgement

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideRuntimeSeam {
    /// Authored gameplay text can be shown unchanged by an engine adapter.
    Portable,
    /// The Heavy Water binding is retained as source data but must be mapped
    /// through Starfall's current input/action labels before presentation.
    NeedsInputMapping,
    /// Informational source copy only; secure online/cloud authority is
    /// intentionally outside this offline module.
    SecureOnlineReference,
    /// Browser console/WebGPU advice from the TypeScript build, not a native
    /// Bevy graphics setting.
    LegacyBrowserOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuideRow {
    pub label: Option<&'static str>,
    pub keys: Option<&'static str>,
    pub text: Option<&'static str>,
    pub runtime_seam: GuideRuntimeSeam,
}

impl GuideRow {
    pub const fn binding(label: &'static str, keys: &'static str) -> Self {
        Self {
            label: Some(label),
            keys: Some(keys),
            text: None,
            runtime_seam: GuideRuntimeSeam::NeedsInputMapping,
        }
    }

    pub const fn labeled_text(
        label: &'static str,
        text: &'static str,
        runtime_seam: GuideRuntimeSeam,
    ) -> Self {
        Self {
            label: Some(label),
            keys: None,
            text: Some(text),
            runtime_seam,
        }
    }

    pub const fn text(text: &'static str) -> Self {
        Self {
            label: None,
            keys: None,
            text: Some(text),
            runtime_seam: GuideRuntimeSeam::Portable,
        }
    }

    pub const fn text_with_seam(text: &'static str, runtime_seam: GuideRuntimeSeam) -> Self {
        Self {
            label: None,
            keys: None,
            text: Some(text),
            runtime_seam,
        }
    }

    pub const fn binding_with_seam(
        label: &'static str,
        keys: &'static str,
        runtime_seam: GuideRuntimeSeam,
    ) -> Self {
        Self {
            label: Some(label),
            keys: Some(keys),
            text: None,
            runtime_seam,
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum GuideSectionId {
    #[default]
    Movement,
    Ranged,
    Melee,
    Specials,
    Travel,
    Elements,
    Creatures,
    World,
    Base,
    Music,
    Graphics,
}

impl GuideSectionId {
    pub const ALL: [Self; HEAVY_GUIDE_SECTION_COUNT] = [
        Self::Movement,
        Self::Ranged,
        Self::Melee,
        Self::Specials,
        Self::Travel,
        Self::Elements,
        Self::Creatures,
        Self::World,
        Self::Base,
        Self::Music,
        Self::Graphics,
    ];

    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::Movement => "movement",
            Self::Ranged => "ranged",
            Self::Melee => "melee",
            Self::Specials => "specials",
            Self::Travel => "travel",
            Self::Elements => "elements",
            Self::Creatures => "creatures",
            Self::World => "world",
            Self::Base => "base",
            Self::Music => "music",
            Self::Graphics => "graphics",
        }
    }

    pub const fn definition(self) -> &'static GuideSectionDefinition {
        &FIELD_MANUAL_SECTIONS[match self {
            Self::Movement => 0,
            Self::Ranged => 1,
            Self::Melee => 2,
            Self::Specials => 3,
            Self::Travel => 4,
            Self::Elements => 5,
            Self::Creatures => 6,
            Self::World => 7,
            Self::Base => 8,
            Self::Music => 9,
            Self::Graphics => 10,
        }]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuideSectionDefinition {
    pub id: GuideSectionId,
    pub title: &'static str,
    pub rows: &'static [GuideRow],
}

pub static MOVEMENT_GUIDE_ROWS: [GuideRow; 6] = [
    GuideRow::binding("Move", "WASD"),
    GuideRow::binding("Sprint", "SHIFT"),
    GuideRow::binding("Jump / Triple-Jump", "SPACE"),
    GuideRow::binding("Boost Dash", "SHIFT + SPACE (mid-air)"),
    GuideRow::binding("Look", "MOUSE"),
    GuideRow::labeled_text(
        "Rocket Skates flight",
        "Triple-jump in the air to enter flight. WASD steers, SPACE climbs, CTRL dives.",
        GuideRuntimeSeam::NeedsInputMapping,
    ),
];

pub static RANGED_GUIDE_ROWS: [GuideRow; 5] = [
    GuideRow::binding("Fire", "LEFT MOUSE"),
    GuideRow::binding("Reload", "R"),
    GuideRow::binding("Cycle weapon", "MOUSE WHEEL"),
    GuideRow::binding("Switch weapon", "1 – 6"),
    GuideRow::text("All ranged weapons have unlimited ammo. Reload only resets the magazine."),
];

pub static MELEE_GUIDE_ROWS: [GuideRow; 5] = [
    GuideRow::binding("Light slash", "V"),
    GuideRow::binding("Heavy slash", "B"),
    GuideRow::binding("Dodge", "Q"),
    GuideRow::binding("Parry", "F"),
    GuideRow::text("Beam Sabre slashes chain into combos. Time the next press to keep the chain alive — finishers deal bonus damage."),
];

pub static SPECIALS_GUIDE_ROWS: [GuideRow; 4] = [
    GuideRow::binding(
        "Mega Beam Cannon",
        "LT + RT  ·  (homing missiles + Kamehameha laser)",
    ),
    GuideRow::binding(
        "Fury Slash",
        "LT + Y   /   ;   ·  5 wide rapid slashes, 1.4× damage",
    ),
    GuideRow::binding(
        "Smash Lash",
        "LT + X   /   '   ·  heavy smash + 12 cyan waves radiating omnidirectionally",
    ),
    GuideRow::text_with_seam(
        "Combos preempt any in-flight regular slash, so the natural \"hold trigger, tap face button\" order works reliably on both keyboard and gamepad.",
        GuideRuntimeSeam::NeedsInputMapping,
    ),
];

pub static TRAVEL_GUIDE_ROWS: [GuideRow; 6] = [
    GuideRow::binding("Open menu", "TAB"),
    GuideRow::text("Switch to the TRAVEL tab to warp between six zones: three combat fronts (Star City, Hold the Line, Purge the Void), the peaceful Ashur Sanctuary, the Orbital Front (starfield combat with Earth on the horizon, drifting asteroids, and drone-orbited motherships), and the Pontiac Secret Lab (a covert pre-war research bunker with cryo pods, server racks, holo terminals and Dr. Cynthia You). Inventory, upgrades and built structures are preserved across warps."),
    GuideRow::text("Default helper-bot loadout cap is 3. Upgrade the Lab to raise it. The Sanctuary now has a glowing cyan plinth that opens the deploy / capture UI directly — no need to build a Garden first."),
    GuideRow::text_with_seam(
        "After signing in, the main menu shows a Cloud Save card under the buttons with your level, credits, kills, current zone and last-saved time so you know exactly what START MISSION will resume.",
        GuideRuntimeSeam::SecureOnlineReference,
    ),
    GuideRow::binding("Plant / harvest", "E (sanctuary plots)"),
    GuideRow::text("The sanctuary has 5 farm plots, three NPCs and a signpost. Plant a bio_seed, wait through 3 growth stages, harvest a bio_crop. You receive 5 starter bio_seeds the first time you enter."),
];

pub static ELEMENTS_GUIDE_ROWS: [GuideRow; 2] = [
    GuideRow::binding("Cast element", "U  I  O  P  K  L"),
    GuideRow::text("Six elements unlock as you progress: Inferno, Frost, Storm, Earth, Light, Void. Each fires a Tracking Strike (homes in on enemies) and triggers Dome Explosions on impact."),
];

pub static CREATURES_GUIDE_ROWS: [GuideRow; 2] = [
    GuideRow::binding("Throw Capture Orb", "H"),
    GuideRow::text("125+ collectible robotic creatures roam the world. Weaken one to low HP, then throw a Capture Orb. Rarer creatures need stronger orbs and lower HP thresholds."),
];

pub static WORLD_GUIDE_ROWS: [GuideRow; 4] = [
    GuideRow::binding("Interact / Mount vehicle", "E"),
    GuideRow::binding("Map", "M"),
    GuideRow::binding("Inventory & Upgrades", "TAB"),
    GuideRow::text("1200×1200 open world: central city, four biomes, mountain ring, and four hidden stepped-pyramid temples. ATVs and space fighters are mountable when you find them."),
];

pub static BASE_GUIDE_ROWS: [GuideRow; 1] = [GuideRow::text_with_seam(
    "Open the upgrade menu (TAB) to unlock build mode. Structures are grid-snapped, multi-level, and serialized into prefabs you can save / share.",
    GuideRuntimeSeam::NeedsInputMapping,
)];

pub static MUSIC_GUIDE_ROWS: [GuideRow; 2] = [
    GuideRow::binding("Previous / Next track", "[   ]"),
    GuideRow::binding("Play / Pause", "\\"),
];

pub static GRAPHICS_GUIDE_ROWS: [GuideRow; 4] = [
    GuideRow::text_with_seam(
        "WebGPU backend is opt-in and EXPERIMENTAL. Open the browser console and run:",
        GuideRuntimeSeam::LegacyBrowserOnly,
    ),
    GuideRow::binding_with_seam(
        "Enable",
        "localStorage.setItem('heavywater:webgpu','1')",
        GuideRuntimeSeam::LegacyBrowserOnly,
    ),
    GuideRow::binding_with_seam(
        "Disable",
        "localStorage.removeItem('heavywater:webgpu')",
        GuideRuntimeSeam::LegacyBrowserOnly,
    ),
    GuideRow::text_with_seam(
        "Then refresh. Several custom shaders (ink outline, sky, city) are still GLSL-ES-1.0 and will not render — and may throw — on WebGPU. Bloom, FXAA, sharpen, and chromatic aberration all work. If WebGPU init fails the game falls back to WebGL2 automatically.",
        GuideRuntimeSeam::LegacyBrowserOnly,
    ),
];

pub static FIELD_MANUAL_SECTIONS: [GuideSectionDefinition; HEAVY_GUIDE_SECTION_COUNT] = [
    GuideSectionDefinition {
        id: GuideSectionId::Movement,
        title: "MOVEMENT",
        rows: &MOVEMENT_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::Ranged,
        title: "RANGED COMBAT",
        rows: &RANGED_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::Melee,
        title: "MELEE & BEAM SABRE",
        rows: &MELEE_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::Specials,
        title: "BEAM SABRE COMBOS",
        rows: &SPECIALS_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::Travel,
        title: "FAST TRAVEL & SANCTUARY",
        rows: &TRAVEL_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::Elements,
        title: "ELEMENTAL CASTING",
        rows: &ELEMENTS_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::Creatures,
        title: "BIO-CREATURES",
        rows: &CREATURES_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::World,
        title: "WORLD & TRAVERSAL",
        rows: &WORLD_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::Base,
        title: "BASE BUILDING",
        rows: &BASE_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::Music,
        title: "MUSIC",
        rows: &MUSIC_GUIDE_ROWS,
    },
    GuideSectionDefinition {
        id: GuideSectionId::Graphics,
        title: "GRAPHICS (advanced)",
        rows: &GRAPHICS_GUIDE_ROWS,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum FieldManualEvent {
    Opened {
        active_section: GuideSectionId,
        first_open: bool,
    },
    SectionViewed {
        section: GuideSectionId,
        first_view: bool,
    },
    FirstRunCompleted {
        newly_completed: bool,
    },
}

/// The React guide did not persist navigation. This additive offline record
/// supplies the durable first-run completion flag requested by the port while
/// keeping completion explicit: merely browsing a section never awards items
/// or silently dismisses onboarding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldManualProgress {
    #[serde(default)]
    pub active_section: GuideSectionId,
    #[serde(default)]
    pub viewed_sections: BTreeSet<GuideSectionId>,
    #[serde(default)]
    pub first_opened_at_epoch_seconds: Option<u64>,
    #[serde(default)]
    pub first_run_completed_at_epoch_seconds: Option<u64>,
}

impl Default for FieldManualProgress {
    fn default() -> Self {
        Self {
            active_section: GuideSectionId::Movement,
            viewed_sections: BTreeSet::new(),
            first_opened_at_epoch_seconds: None,
            first_run_completed_at_epoch_seconds: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FieldManualNormalizationReport {
    pub inferred_first_open_timestamp: bool,
    pub repaired_timestamp_order: bool,
}

impl FieldManualProgress {
    pub fn open(&mut self, now_epoch_seconds: u64) -> FieldManualEvent {
        let first_open = self.first_opened_at_epoch_seconds.is_none();
        self.first_opened_at_epoch_seconds = self
            .first_opened_at_epoch_seconds
            .or(Some(now_epoch_seconds));
        self.viewed_sections.insert(self.active_section);
        FieldManualEvent::Opened {
            active_section: self.active_section,
            first_open,
        }
    }

    pub fn view_section(
        &mut self,
        section: GuideSectionId,
        now_epoch_seconds: u64,
    ) -> FieldManualEvent {
        self.first_opened_at_epoch_seconds = self
            .first_opened_at_epoch_seconds
            .or(Some(now_epoch_seconds));
        self.active_section = section;
        let first_view = self.viewed_sections.insert(section);
        FieldManualEvent::SectionViewed {
            section,
            first_view,
        }
    }

    pub fn complete_first_run(&mut self, now_epoch_seconds: u64) -> FieldManualEvent {
        self.first_opened_at_epoch_seconds = self
            .first_opened_at_epoch_seconds
            .or(Some(now_epoch_seconds));
        let newly_completed = self.first_run_completed_at_epoch_seconds.is_none();
        self.first_run_completed_at_epoch_seconds = self
            .first_run_completed_at_epoch_seconds
            .or(Some(now_epoch_seconds));
        FieldManualEvent::FirstRunCompleted { newly_completed }
    }

    pub const fn first_run_complete(&self) -> bool {
        self.first_run_completed_at_epoch_seconds.is_some()
    }

    pub fn validate(&self) -> Result<(), WorldEventDomainError> {
        if let Some(completed) = self.first_run_completed_at_epoch_seconds {
            let Some(opened) = self.first_opened_at_epoch_seconds else {
                return Err(WorldEventDomainError::InvalidState(
                    "field manual completed without an open timestamp".to_owned(),
                ));
            };
            if opened > completed {
                return Err(WorldEventDomainError::InvalidState(
                    "field manual completion predates first open".to_owned(),
                ));
            }
        }
        Ok(())
    }

    pub fn normalize(&mut self) -> FieldManualNormalizationReport {
        let mut report = FieldManualNormalizationReport::default();
        if let Some(completed) = self.first_run_completed_at_epoch_seconds {
            match self.first_opened_at_epoch_seconds {
                None => {
                    self.first_opened_at_epoch_seconds = Some(completed);
                    report.inferred_first_open_timestamp = true;
                }
                Some(opened) if opened > completed => {
                    self.first_opened_at_epoch_seconds = Some(completed);
                    report.repaired_timestamp_order = true;
                }
                Some(_) => {}
            }
        }
        report
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeavyWorldEventsSave {
    #[serde(default = "current_schema_version")]
    pub schema_version: u16,
    #[serde(default)]
    pub invasion: InvasionDirector,
    #[serde(default)]
    pub npc_dialogue: NpcDialogueState,
    #[serde(default)]
    pub field_manual: FieldManualProgress,
}

impl Default for HeavyWorldEventsSave {
    fn default() -> Self {
        Self {
            schema_version: HEAVY_WORLD_EVENTS_SCHEMA_VERSION,
            invasion: InvasionDirector::default(),
            npc_dialogue: NpcDialogueState::default(),
            field_manual: FieldManualProgress::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldEventsNormalizationReport {
    pub schema_updated: bool,
    pub invasion: InvasionNormalizationReport,
    pub npc_dialogue: NpcDialogueNormalizationReport,
    pub field_manual: FieldManualNormalizationReport,
}

impl HeavyWorldEventsSave {
    pub fn validate(&self) -> Result<(), WorldEventDomainError> {
        if self.schema_version != HEAVY_WORLD_EVENTS_SCHEMA_VERSION {
            return Err(WorldEventDomainError::InvalidState(format!(
                "unsupported aggregate world-event schema {}",
                self.schema_version
            )));
        }
        self.invasion.validate()?;
        self.npc_dialogue.validate()?;
        self.field_manual.validate()
    }

    pub fn normalize(&mut self) -> WorldEventsNormalizationReport {
        let schema_updated = self.schema_version != HEAVY_WORLD_EVENTS_SCHEMA_VERSION;
        self.schema_version = HEAVY_WORLD_EVENTS_SCHEMA_VERSION;
        WorldEventsNormalizationReport {
            schema_updated,
            invasion: self.invasion.normalize(),
            npc_dialogue: self.npc_dialogue.normalize(),
            field_manual: self.field_manual.normalize(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlock_detroit_campaign(director: &mut InvasionDirector) {
        director.complete_level(1, false, 10).unwrap();
        director.complete_level(2, false, 20).unwrap();
        director.complete_level(3, false, 30).unwrap();
    }

    fn set_only_candidate(director: &mut InvasionDirector, candidate: WorldZoneId) {
        for zone in director.zones.values_mut() {
            zone.state = ZoneState::Uncleared;
            zone.liberated_at_epoch_seconds = None;
            zone.last_resolved_at_epoch_seconds = None;
        }
        let zone = director.zones.get_mut(&candidate).unwrap();
        zone.state = ZoneState::Peaceful;
        zone.liberated_at_epoch_seconds = Some(1);
    }

    #[test]
    fn source_catalog_counts_and_ids_are_exact() {
        assert_eq!(REBUILD_ZONES.len(), HEAVY_REBUILD_ZONE_COUNT);
        assert_eq!(ZoneState::ALL.len(), HEAVY_ZONE_STATE_COUNT);
        assert_eq!(HEAVY_INVASION_LIFECYCLE_EVENT_COUNT, 4);
        assert_eq!(
            REBUILD_ZONES
                .iter()
                .filter(|zone| zone.invasion_eligible)
                .count(),
            HEAVY_INVASION_ELIGIBLE_ZONE_COUNT
        );
        assert_eq!(
            REBUILD_ZONES
                .iter()
                .filter(|zone| !zone.invasion_eligible)
                .map(|zone| zone.id)
                .collect::<Vec<_>>(),
            vec![WorldZoneId::AshurSanctuary, WorldZoneId::PontiacLab]
        );
        assert_eq!(
            WorldZoneId::ALL.map(WorldZoneId::stable_id),
            [
                "detroit_star_city",
                "detroit_hold_line",
                "detroit_void",
                "ashur_sanctuary",
                "orbital_front",
                "pontiac_lab",
                "swarms_lair",
                "saginaw_lab",
                "zug_island",
                "ann_arbor",
                "michigan_wilds",
            ]
        );
        assert_eq!(
            REBUILD_ZONES.map(|zone| zone.base_threat),
            [1, 2, 3, 1, 4, 2, 5, 5, 6, 6, 7]
        );
        assert_eq!(INITIAL_PEACE_MS, 900_000);
        assert_eq!(REPEAT_PEACE_MS, 1_200_000);
        assert_eq!(WARNING_DURATION_MS, 120_000);
        assert_eq!(CAMPAIGN_UNLOCK_MINIMUM_PEACE_MS, 600_000);
    }

    #[test]
    fn campaign_gate_and_invasion_lifecycle_match_source() {
        let mut director = InvasionDirector::default();
        assert!(director.tick(INITIAL_PEACE_MS, 50, 0).is_empty());
        assert_eq!(director.next_invasion_in_ms, INITIAL_PEACE_MS);

        unlock_detroit_campaign(&mut director);
        assert!(director.main_campaign_complete);
        assert_eq!(director.next_invasion_in_ms, INITIAL_PEACE_MS);

        let warning = director.tick(INITIAL_PEACE_MS, 100, 0);
        assert_eq!(director.warning_zone_id, Some(WorldZoneId::DetroitStarCity));
        assert_eq!(
            director.zone(WorldZoneId::DetroitStarCity).state,
            ZoneState::Threatened
        );
        assert!(matches!(
            warning.as_slice(),
            [
                InvasionEvent::ZoneStateChanged {
                    state: ZoneState::Threatened,
                    ..
                },
                InvasionEvent::Warning {
                    zone_id: WorldZoneId::DetroitStarCity,
                    warning_duration_ms: WARNING_DURATION_MS,
                    ..
                }
            ]
        ));

        assert!(director.tick(WARNING_DURATION_MS - 1, 101, 0).is_empty());
        let started = director.tick(1, 102, 0);
        assert!(matches!(
            started.last(),
            Some(InvasionEvent::Started {
                zone_id: WorldZoneId::DetroitStarCity,
                started_at_epoch_seconds: 102,
                ..
            })
        ));
        let active = director.active_invasion().unwrap();
        assert_eq!(active.zone_id, WorldZoneId::DetroitStarCity);

        let timer_while_active = director.next_invasion_in_ms;
        assert!(director.tick(50_000, 103, 999).is_empty());
        assert_eq!(director.next_invasion_in_ms, timer_while_active);
        assert!(director.boss_fortress_cleared(2, 104).is_empty());

        let resolved = director.boss_fortress_cleared(1, 105);
        assert!(matches!(
            resolved.last(),
            Some(InvasionEvent::Resolved {
                reason: InvasionResolutionReason::FortressCleared,
                reward: InvasionStrategicReward {
                    victories_gained: 1,
                    peace_restored: true,
                    purification_granted: true,
                    next_peace_window_ms: REPEAT_PEACE_MS,
                    ..
                },
                ..
            })
        ));
        let zone = director.zone(WorldZoneId::DetroitStarCity);
        assert_eq!(zone.state, ZoneState::Purified);
        assert_eq!(zone.victories, 1);
        assert_eq!(zone.threat_level, 2);
        assert_eq!(zone.last_resolved_at_epoch_seconds, Some(105));
        assert_eq!(director.next_invasion_in_ms, REPEAT_PEACE_MS);
        director.validate().unwrap();
    }

    #[test]
    fn target_selection_is_oldest_resolution_then_world_level() {
        let mut director = InvasionDirector::default();
        director.main_campaign_complete = true;
        for zone in director.zones.values_mut() {
            zone.state = ZoneState::Uncleared;
        }

        for id in [
            WorldZoneId::DetroitStarCity,
            WorldZoneId::DetroitHoldLine,
            WorldZoneId::DetroitVoid,
        ] {
            director.zones.get_mut(&id).unwrap().state = ZoneState::Purified;
        }
        director
            .zones
            .get_mut(&WorldZoneId::DetroitStarCity)
            .unwrap()
            .last_resolved_at_epoch_seconds = Some(80);
        director
            .zones
            .get_mut(&WorldZoneId::DetroitHoldLine)
            .unwrap()
            .last_resolved_at_epoch_seconds = None;
        director
            .zones
            .get_mut(&WorldZoneId::DetroitVoid)
            .unwrap()
            .last_resolved_at_epoch_seconds = None;

        director.next_invasion_in_ms = 0;
        director.tick(1, 100, 0);
        assert_eq!(director.warning_zone_id, Some(WorldZoneId::DetroitHoldLine));
    }

    #[test]
    fn settlement_strength_dampens_threat_by_at_most_two() {
        let mut director = InvasionDirector::default();
        let wilds = director.zones.get_mut(&WorldZoneId::MichiganWilds).unwrap();
        wilds.victories = 5;
        assert_eq!(
            InvasionDirector::settlement_strength(10, 4, 100),
            10 + 8 + 20
        );
        assert_eq!(
            director.calculate_threat_level(WorldZoneId::MichiganWilds, 0),
            12
        );
        assert_eq!(
            director.calculate_threat_level(WorldZoneId::MichiganWilds, 18),
            11
        );
        assert_eq!(
            director.calculate_threat_level(WorldZoneId::MichiganWilds, 36),
            10
        );
        assert_eq!(
            director.calculate_threat_level(WorldZoneId::MichiganWilds, u32::MAX),
            10
        );
    }

    #[test]
    fn no_eligible_target_restarts_repeat_peace_window() {
        let mut director = InvasionDirector::default();
        director.main_campaign_complete = true;
        director.next_invasion_in_ms = 1;
        assert!(director.tick(1, 50, 0).is_empty());
        assert_eq!(director.next_invasion_in_ms, REPEAT_PEACE_MS);
        assert!(director.warning_zone_id.is_none());
    }

    #[test]
    fn legacy_and_boss_hooks_respect_their_source_gates() {
        let mut director = InvasionDirector::default();
        director.hydrate_legacy_progress(Some(3), true, 200);
        assert_eq!(
            director.zone(WorldZoneId::DetroitStarCity).state,
            ZoneState::Peaceful
        );
        assert_eq!(
            director.zone(WorldZoneId::DetroitHoldLine).state,
            ZoneState::Peaceful
        );
        assert_eq!(
            director.zone(WorldZoneId::DetroitVoid).state,
            ZoneState::Uncleared
        );
        assert_eq!(
            director.zone(WorldZoneId::SwarmsLair).state,
            ZoneState::Purified
        );

        director.main_campaign_complete = true;
        set_only_candidate(&mut director, WorldZoneId::SwarmsLair);
        director.next_invasion_in_ms = 0;
        director.tick(1, 201, 0);
        director.tick(WARNING_DURATION_MS, 202, 0);
        let resolved = director.swarms_general_defeated(203);
        assert!(matches!(
            resolved.last(),
            Some(InvasionEvent::Resolved {
                reason: InvasionResolutionReason::GeneralDefeated,
                ..
            })
        ));
        assert!(director.active_invasion.is_none());
        assert_eq!(
            director.zone(WorldZoneId::SwarmsLair).state,
            ZoneState::Purified
        );
    }

    #[test]
    fn direct_defense_can_resolve_during_warning() {
        let mut director = InvasionDirector::default();
        director.main_campaign_complete = true;
        set_only_candidate(&mut director, WorldZoneId::OrbitalFront);
        director.next_invasion_in_ms = 0;
        director.tick(1, 300, 0);
        let events = director.resolve_invasion(
            WorldZoneId::OrbitalFront,
            InvasionResolutionReason::Defended,
            301,
        );
        assert!(matches!(
            events.last(),
            Some(InvasionEvent::Resolved { .. })
        ));
        assert!(director.warning_zone_id.is_none());
        assert_eq!(director.warning_elapsed_ms, 0);
    }

    #[test]
    fn invasion_normalization_repairs_partial_and_transient_saves() {
        let mut director = InvasionDirector::default();
        director.schema_version = 0;
        director.zones.remove(&WorldZoneId::MichiganWilds);
        let detroit = director
            .zones
            .get_mut(&WorldZoneId::DetroitStarCity)
            .unwrap();
        detroit.id = WorldZoneId::AnnArbor;
        detroit.world_level = 99;
        detroit.threat_level = 0;
        detroit.state = ZoneState::Invaded;
        director.warning_zone_id = Some(WorldZoneId::AshurSanctuary);
        director.warning_elapsed_ms = WARNING_DURATION_MS + 50;
        director.active_invasion = Some(ActiveInvasion {
            zone_id: WorldZoneId::PontiacLab,
            world_level: 99,
            threat_level: 0,
            started_at_epoch_seconds: 10,
        });

        assert!(director.validate().is_err());
        let report = director.normalize();
        assert!(report.schema_updated);
        assert_eq!(report.zones_inserted, 1);
        assert_eq!(report.zone_identity_repairs, 1);
        assert_eq!(report.threat_repairs, 1);
        assert!(report.active_invasion_cleared);
        assert!(report.warning_cleared);
        assert_eq!(report.transient_zone_repairs, 1);
        director.validate().unwrap();

        director.main_campaign_complete = true;
        director.active_invasion = Some(ActiveInvasion {
            zone_id: WorldZoneId::DetroitStarCity,
            world_level: 77,
            threat_level: 0,
            started_at_epoch_seconds: 20,
        });
        director.warning_zone_id = Some(WorldZoneId::DetroitHoldLine);
        director.warning_elapsed_ms = 12;
        director.normalize();
        assert_eq!(
            director.active_invasion,
            Some(ActiveInvasion {
                zone_id: WorldZoneId::DetroitStarCity,
                world_level: 1,
                threat_level: 1,
                started_at_epoch_seconds: 20,
            })
        );
        assert!(director.warning_zone_id.is_none());
        director.validate().unwrap();
    }

    #[test]
    fn friendly_cast_preserves_six_npcs_six_palettes_and_twenty_four_lines() {
        assert_eq!(FRIENDLY_NPCS.len(), HEAVY_FRIENDLY_NPC_COUNT);
        assert_eq!(FRIENDLY_PALETTES.len(), HEAVY_FRIENDLY_PALETTE_COUNT);
        assert_eq!(
            FRIENDLY_NPCS
                .iter()
                .map(|npc| npc.dialogue.len())
                .sum::<usize>(),
            HEAVY_NPC_DIALOGUE_LINE_COUNT
        );
        assert_eq!(
            FriendlyNpcId::ALL.map(FriendlyNpcId::stable_id),
            [
                "captain_iris",
                "merchant_bex",
                "engineer_juno",
                "rider_kazu",
                "mystic_ori",
                "scout_pip",
            ]
        );
        assert_eq!(
            FriendlyNpcId::CaptainIris.definition().position,
            WorldEventPoint::new(20.0, 0.0, 20.0)
        );
        assert_eq!(
            FriendlyNpcId::ScoutPip.definition().dialogue[3],
            "Watch your minimap. Stay sharp out there."
        );
        assert_eq!(NPC_INTERACT_RANGE_METERS, 5.5);
        assert_eq!(NPC_STAY_RANGE_METERS, 9.0);
    }

    #[test]
    fn npc_focus_uses_horizontal_hysteresis_and_walkaway_closes() {
        let mut state = NpcDialogueState::default();
        let acquired = state
            .update_focus(WorldEventPoint::new(20.0, 500.0, 20.0))
            .unwrap();
        assert_eq!(state.focused_npc, Some(FriendlyNpcId::CaptainIris));
        assert_eq!(acquired.len(), 1);

        state.advance(false);
        assert!(state.dialogue_open);
        assert!(state
            .update_focus(WorldEventPoint::new(26.0, -100.0, 20.0))
            .unwrap()
            .is_empty());
        assert!(state.dialogue_open);

        let released = state
            .update_focus(WorldEventPoint::new(40.0, 0.0, 20.0))
            .unwrap();
        assert_eq!(state.focused_npc, None);
        assert!(!state.dialogue_open);
        assert!(matches!(
            released.first(),
            Some(NpcDialogueEvent::Closed {
                reason: DialogueCloseReason::FocusLost,
                ..
            })
        ));
        assert!(state
            .update_focus(WorldEventPoint::new(f32::NAN, 0.0, 0.0))
            .is_err());
    }

    #[test]
    fn npc_dialogue_opens_advances_and_closes_after_last_line() {
        let mut state = NpcDialogueState::default();
        state
            .update_focus(FriendlyNpcId::CaptainIris.definition().position)
            .unwrap();
        assert!(state.advance(true).is_none());
        assert!(!state.dialogue_open);

        assert!(matches!(
            state.advance(false),
            Some(NpcDialogueEvent::Opened { line_index: 0, .. })
        ));
        assert_eq!(state.current_line(), Some("Welcome to Heavy Water, pilot."));
        for expected_index in 1..4 {
            assert!(matches!(
                state.advance(false),
                Some(NpcDialogueEvent::Advanced { line_index, .. }) if line_index == expected_index
            ));
        }
        assert!(matches!(
            state.advance(false),
            Some(NpcDialogueEvent::Closed {
                reason: DialogueCloseReason::Completed,
                ..
            })
        ));
        assert!(!state.dialogue_open);
        assert_eq!(state.dialogue_index, 4);
        state.validate().unwrap();
    }

    #[test]
    fn field_manual_catalog_preserves_eleven_sections_and_forty_one_rows() {
        assert_eq!(FIELD_MANUAL_SECTIONS.len(), HEAVY_GUIDE_SECTION_COUNT);
        assert_eq!(
            FIELD_MANUAL_SECTIONS
                .iter()
                .map(|section| section.rows.len())
                .sum::<usize>(),
            HEAVY_GUIDE_ROW_COUNT
        );
        assert_eq!(
            GuideSectionId::ALL.map(GuideSectionId::stable_id),
            [
                "movement",
                "ranged",
                "melee",
                "specials",
                "travel",
                "elements",
                "creatures",
                "world",
                "base",
                "music",
                "graphics",
            ]
        );
        assert_eq!(MOVEMENT_GUIDE_ROWS.len(), 6);
        assert_eq!(TRAVEL_GUIDE_ROWS.len(), 6);
        assert_eq!(GRAPHICS_GUIDE_ROWS.len(), 4);
        assert_eq!(
            TRAVEL_GUIDE_ROWS[3].runtime_seam,
            GuideRuntimeSeam::SecureOnlineReference
        );
        assert!(GRAPHICS_GUIDE_ROWS
            .iter()
            .all(|row| row.runtime_seam == GuideRuntimeSeam::LegacyBrowserOnly));
    }

    #[test]
    fn field_manual_completion_is_explicit_durable_and_idempotent() {
        let mut progress = FieldManualProgress::default();
        assert!(!progress.first_run_complete());
        assert_eq!(
            progress.open(1_000),
            FieldManualEvent::Opened {
                active_section: GuideSectionId::Movement,
                first_open: true,
            }
        );
        assert_eq!(
            progress.view_section(GuideSectionId::Travel, 1_010),
            FieldManualEvent::SectionViewed {
                section: GuideSectionId::Travel,
                first_view: true,
            }
        );
        assert!(!progress.first_run_complete());
        assert_eq!(
            progress.complete_first_run(1_020),
            FieldManualEvent::FirstRunCompleted {
                newly_completed: true,
            }
        );
        assert_eq!(
            progress.complete_first_run(2_000),
            FieldManualEvent::FirstRunCompleted {
                newly_completed: false,
            }
        );
        assert_eq!(progress.first_run_completed_at_epoch_seconds, Some(1_020));
        progress.validate().unwrap();
    }

    #[test]
    fn aggregate_state_round_trips_and_normalizes_legacy_defaults() {
        let mut save = HeavyWorldEventsSave::default();
        unlock_detroit_campaign(&mut save.invasion);
        save.field_manual.open(500);
        save.field_manual.complete_first_run(501);
        save.npc_dialogue
            .update_focus(FriendlyNpcId::MysticOri.definition().position)
            .unwrap();
        save.npc_dialogue.advance(false);

        let json = serde_json::to_string(&save).unwrap();
        let decoded: HeavyWorldEventsSave = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, save);
        decoded.validate().unwrap();

        let mut missing_fields: HeavyWorldEventsSave = serde_json::from_str(
            r#"{
                "schema_version": 0,
                "invasion": { "schema_version": 0, "zones": {} },
                "npc_dialogue": { "dialogue_open": true, "dialogue_index": 99 },
                "field_manual": { "first_run_completed_at_epoch_seconds": 44 }
            }"#,
        )
        .unwrap();
        let report = missing_fields.normalize();
        assert!(report.schema_updated);
        assert_eq!(
            report.invasion.zones_inserted,
            HEAVY_REBUILD_ZONE_COUNT as u16
        );
        assert!(report.npc_dialogue.invalid_open_dialogue_closed);
        assert!(report.field_manual.inferred_first_open_timestamp);
        missing_fields.validate().unwrap();
    }
}
