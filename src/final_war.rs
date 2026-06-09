use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::faction::Faction;
use crate::raids::{RaidConsequence, RaidKind, RaidRegistry};
use crate::resources::{WorldSiteOwner, WorldSiteRegistry, WorldSiteState};

const PRESSURE_TICK_INTERVAL: f32 = 10.0;
const ESCALATING_THRESHOLD: u32 = 50;
const ACTIVE_THRESHOLD: u32 = 150;
const CLIMAX_THRESHOLD: u32 = 300;

const COORDINATED_RAID_WARNING: f32 = 45.0;
const COORDINATED_RAID_ACTIVE: f32 = 120.0;
const COORDINATED_RAID_DEFENSE_REQUIRED: u32 = 18;
const COORDINATED_RAID_WAVE_SIZE: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FinalWarPhase {
    #[default]
    Dormant,
    Escalating,
    Active,
    Climax,
}

impl FinalWarPhase {
    pub fn label(self) -> &'static str {
        match self {
            FinalWarPhase::Dormant => "Dormant",
            FinalWarPhase::Escalating => "Escalating",
            FinalWarPhase::Active => "Active",
            FinalWarPhase::Climax => "Climax",
        }
    }

    pub fn coordinated_raid_cooldown(self) -> f32 {
        match self {
            FinalWarPhase::Dormant => f32::MAX,
            FinalWarPhase::Escalating => 120.0,
            FinalWarPhase::Active => 60.0,
            FinalWarPhase::Climax => 30.0,
        }
    }

    /// Max sites targeted per coordinated wave, capped by available liberated sites.
    pub fn raid_target_count(self) -> usize {
        match self {
            FinalWarPhase::Dormant => 0,
            FinalWarPhase::Escalating => 1,
            FinalWarPhase::Active => 2,
            FinalWarPhase::Climax => 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionPressure {
    pub faction: Faction,
    pub pressure: u32,
}

#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct FinalWarRegistry {
    pub phase: FinalWarPhase,
    pub faction_pressures: Vec<FactionPressure>,
    pub pressure_tick_timer: f32,
    pub coordinated_raid_timer: f32,
}

impl Default for FinalWarRegistry {
    fn default() -> Self {
        Self {
            phase: FinalWarPhase::Dormant,
            faction_pressures: initial_faction_pressures(),
            pressure_tick_timer: PRESSURE_TICK_INTERVAL,
            coordinated_raid_timer: FinalWarPhase::Escalating.coordinated_raid_cooldown(),
        }
    }
}

impl FinalWarRegistry {
    pub fn pressure_for(&self, faction: Faction) -> u32 {
        self.faction_pressures
            .iter()
            .find(|fp| fp.faction == faction)
            .map(|fp| fp.pressure)
            .unwrap_or(0)
    }

    pub fn add_pressure(&mut self, faction: Faction, amount: u32) {
        if let Some(fp) = self
            .faction_pressures
            .iter_mut()
            .find(|fp| fp.faction == faction)
        {
            fp.pressure = fp.pressure.saturating_add(amount);
        } else {
            self.faction_pressures
                .push(FactionPressure { faction, pressure: amount });
        }
        self.update_phase();
    }

    pub fn total_pressure(&self) -> u32 {
        self.faction_pressures.iter().map(|fp| fp.pressure).sum()
    }

    pub fn dominant_faction(&self) -> Option<Faction> {
        self.faction_pressures
            .iter()
            .max_by_key(|fp| fp.pressure)
            .filter(|fp| fp.pressure > 0)
            .map(|fp| fp.faction)
    }

    pub fn to_save_record(&self) -> FinalWarSaveRecord {
        FinalWarSaveRecord {
            phase: self.phase,
            faction_pressures: self.faction_pressures.clone(),
            pressure_tick_timer: self.pressure_tick_timer,
            coordinated_raid_timer: self.coordinated_raid_timer,
        }
    }

    pub fn apply_save_record(&mut self, record: FinalWarSaveRecord) {
        self.phase = record.phase;
        self.faction_pressures = if record.faction_pressures.is_empty() {
            initial_faction_pressures()
        } else {
            record.faction_pressures
        };
        self.pressure_tick_timer = record.pressure_tick_timer;
        self.coordinated_raid_timer = record.coordinated_raid_timer;
    }

    fn update_phase(&mut self) {
        let total = self.total_pressure();
        let new_phase = if total >= CLIMAX_THRESHOLD {
            FinalWarPhase::Climax
        } else if total >= ACTIVE_THRESHOLD {
            FinalWarPhase::Active
        } else if total >= ESCALATING_THRESHOLD {
            FinalWarPhase::Escalating
        } else {
            FinalWarPhase::Dormant
        };
        if new_phase != self.phase {
            self.coordinated_raid_timer = new_phase.coordinated_raid_cooldown();
            self.phase = new_phase;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FinalWarSaveRecord {
    #[serde(default)]
    pub phase: FinalWarPhase,
    #[serde(default)]
    pub faction_pressures: Vec<FactionPressure>,
    #[serde(default)]
    pub pressure_tick_timer: f32,
    #[serde(default)]
    pub coordinated_raid_timer: f32,
}

pub fn initial_faction_pressures() -> Vec<FactionPressure> {
    vec![
        FactionPressure {
            faction: Faction::DimensionalAlien,
            pressure: 0,
        },
        FactionPressure {
            faction: Faction::DragonRoyalty,
            pressure: 0,
        },
        FactionPressure {
            faction: Faction::DragonExile,
            pressure: 0,
        },
    ]
}

/// Maps a site owner to the enemy faction driving pressure from that site.
fn owner_faction(owner: WorldSiteOwner) -> Option<Faction> {
    match owner {
        WorldSiteOwner::Scallarian => Some(Faction::DimensionalAlien),
        WorldSiteOwner::DragonDomain => Some(Faction::DragonRoyalty),
        WorldSiteOwner::FreePeoples
        | WorldSiteOwner::Neutral
        | WorldSiteOwner::PlayerAlliance => None,
    }
}

/// Maps a faction to the most appropriate raid kind for coordinated attacks.
fn faction_raid_kind(faction: Faction) -> RaidKind {
    match faction {
        Faction::DimensionalAlien => RaidKind::GiantUfoSiege,
        Faction::DragonRoyalty | Faction::DragonExile => RaidKind::DragonDomainAssault,
        _ => RaidKind::DroneSwarm,
    }
}

// ── Systems ───────────────────────────────────────────────────────────────────

/// Accumulates faction pressure every `PRESSURE_TICK_INTERVAL` seconds based
/// on how many sites each enemy faction still holds.
pub fn pressure_accumulation_system(
    time: Res<Time>,
    site_registry: Res<WorldSiteRegistry>,
    mut final_war: ResMut<FinalWarRegistry>,
) {
    final_war.pressure_tick_timer -= time.delta_secs();
    if final_war.pressure_tick_timer > 0.0 {
        return;
    }
    final_war.pressure_tick_timer += PRESSURE_TICK_INTERVAL;

    for site in &site_registry.sites {
        if site.state != WorldSiteState::EnemyHeld && site.state != WorldSiteState::OccupiedAgain {
            continue;
        }
        if let Some(faction) = owner_faction(site.owner) {
            if let Some(fp) = final_war
                .faction_pressures
                .iter_mut()
                .find(|fp| fp.faction == faction)
            {
                fp.pressure = fp.pressure.saturating_add(1);
            }
        }
    }
    final_war.update_phase();
}

/// Triggers synchronized raids on multiple liberated sites when pressure
/// thresholds are met and the coordinated raid cooldown expires.
pub fn coordinated_raid_trigger_system(
    time: Res<Time>,
    site_registry: Res<WorldSiteRegistry>,
    mut final_war: ResMut<FinalWarRegistry>,
    mut raid_registry: ResMut<RaidRegistry>,
) {
    if final_war.phase == FinalWarPhase::Dormant {
        return;
    }

    final_war.coordinated_raid_timer -= time.delta_secs();
    if final_war.coordinated_raid_timer > 0.0 {
        return;
    }
    final_war.coordinated_raid_timer = final_war.phase.coordinated_raid_cooldown();

    let Some(attacker) = final_war.dominant_faction() else {
        return;
    };
    let raid_kind = faction_raid_kind(attacker);
    let max_targets = final_war.phase.raid_target_count();

    let liberated_sites: Vec<_> = site_registry
        .sites
        .iter()
        .filter(|site| {
            site.state == WorldSiteState::Liberated
                && !raid_registry.unresolved_for_site(site.id)
        })
        .map(|site| site.id)
        .take(max_targets)
        .collect();

    for site_id in liberated_sites {
        raid_registry.push_new_raid(
            raid_kind,
            site_id,
            attacker,
            COORDINATED_RAID_WARNING,
            COORDINATED_RAID_ACTIVE,
            COORDINATED_RAID_DEFENSE_REQUIRED,
            COORDINATED_RAID_WAVE_SIZE,
            RaidConsequence::SiteDamaged,
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_escalates_phase_correctly() {
        let mut reg = FinalWarRegistry::default();
        assert_eq!(reg.phase, FinalWarPhase::Dormant);

        reg.add_pressure(Faction::DimensionalAlien, 50);
        assert_eq!(reg.phase, FinalWarPhase::Escalating);

        reg.add_pressure(Faction::DimensionalAlien, 100);
        assert_eq!(reg.phase, FinalWarPhase::Active);

        reg.add_pressure(Faction::DragonRoyalty, 200);
        assert_eq!(reg.phase, FinalWarPhase::Climax);
    }

    #[test]
    fn dominant_faction_returns_highest_pressure() {
        let mut reg = FinalWarRegistry::default();
        reg.add_pressure(Faction::DimensionalAlien, 80);
        reg.add_pressure(Faction::DragonRoyalty, 120);

        assert_eq!(reg.dominant_faction(), Some(Faction::DragonRoyalty));
    }

    #[test]
    fn dominant_faction_none_when_all_zero() {
        let reg = FinalWarRegistry::default();
        assert_eq!(reg.dominant_faction(), None);
    }

    #[test]
    fn save_round_trip_preserves_phase_and_pressure() {
        let mut reg = FinalWarRegistry::default();
        reg.add_pressure(Faction::DimensionalAlien, 160);
        reg.pressure_tick_timer = 7.3;
        reg.coordinated_raid_timer = 42.0;

        let record = reg.to_save_record();
        let json = serde_json::to_string(&record).expect("should serialize");
        let loaded: FinalWarSaveRecord =
            serde_json::from_str(&json).expect("should deserialize");

        let mut restored = FinalWarRegistry::default();
        restored.apply_save_record(loaded);

        assert_eq!(restored.phase, FinalWarPhase::Active);
        assert_eq!(
            restored.pressure_for(Faction::DimensionalAlien),
            160
        );
        assert_eq!(restored.pressure_tick_timer, 7.3);
        assert_eq!(restored.coordinated_raid_timer, 42.0);
    }

    #[test]
    fn coordinated_raid_cooldown_decreases_with_phase_escalation() {
        assert!(
            FinalWarPhase::Climax.coordinated_raid_cooldown()
                < FinalWarPhase::Active.coordinated_raid_cooldown()
        );
        assert!(
            FinalWarPhase::Active.coordinated_raid_cooldown()
                < FinalWarPhase::Escalating.coordinated_raid_cooldown()
        );
    }
}
