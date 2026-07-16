use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::chapters::map_settlements;
use crate::components::faction::Faction;
use crate::resources::{WorldRouteId, WorldSiteId, WorldSiteRegistry, WorldSiteState};
use crate::settlement_economy::{SettlementBuildKind, SettlementEconomy};

pub const CLOUDRAIL_TUTORIAL_RAID_SITE: WorldSiteId = WorldSiteId(4);
pub const CLOUDRAIL_TUTORIAL_RAID_REQUIRED_DEFENSE: u32 = 10;
pub const CLOUDRAIL_TUTORIAL_RAID_WARNING_SECONDS: f32 = 12.0;
pub const CLOUDRAIL_TUTORIAL_RAID_ACTIVE_SECONDS: f32 = 120.0;
pub const CLOUDRAIL_TUTORIAL_RAID_WAVE_SIZE: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RaidKind {
    DroneSwarm,
    RobotLanding,
    FighterAttack,
    GiantUfoSiege,
    DragonDomainAssault,
    RouteBlockade,
}

impl RaidKind {
    pub fn label(self) -> &'static str {
        match self {
            RaidKind::DroneSwarm => "Drone Swarm",
            RaidKind::RobotLanding => "Robot Landing",
            RaidKind::FighterAttack => "Fighter Attack",
            RaidKind::GiantUfoSiege => "Giant UFO Siege",
            RaidKind::DragonDomainAssault => "Dragon-Domain Assault",
            RaidKind::RouteBlockade => "Route Blockade",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RaidPhase {
    Warning,
    Active,
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RaidConsequence {
    None,
    SiteDamaged,
    RouteBlocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RaidResolution {
    DefendedByStaticForces,
    ClearedByPlayers,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RaidId(pub u16);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidRecord {
    pub id: RaidId,
    pub kind: RaidKind,
    pub phase: RaidPhase,
    pub target_site: WorldSiteId,
    pub target_route: Option<WorldRouteId>,
    pub attacker: Faction,
    pub warning_timer: f32,
    pub active_timer: f32,
    pub required_defense_score: u32,
    pub spawned: bool,
    pub wave_size: u8,
    pub consequence: RaidConsequence,
    pub resolution: Option<RaidResolution>,
}

impl RaidRecord {
    pub fn cloudrail_tutorial(id: RaidId) -> Self {
        Self {
            id,
            kind: RaidKind::DroneSwarm,
            phase: RaidPhase::Warning,
            target_site: CLOUDRAIL_TUTORIAL_RAID_SITE,
            target_route: None,
            attacker: Faction::DimensionalAlien,
            warning_timer: CLOUDRAIL_TUTORIAL_RAID_WARNING_SECONDS,
            active_timer: CLOUDRAIL_TUTORIAL_RAID_ACTIVE_SECONDS,
            required_defense_score: CLOUDRAIL_TUTORIAL_RAID_REQUIRED_DEFENSE,
            spawned: false,
            wave_size: CLOUDRAIL_TUTORIAL_RAID_WAVE_SIZE,
            consequence: RaidConsequence::SiteDamaged,
            resolution: None,
        }
    }

    pub fn is_unresolved(&self) -> bool {
        self.phase != RaidPhase::Resolved
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaidStartResult {
    Started(RaidId),
    AutoResolved(RaidId),
    AlreadyExists,
    TargetNotLiberated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaidTickEvent {
    WarningExpired(RaidId),
    AutoDefended(RaidId, WorldSiteId),
    Failed(RaidId, WorldSiteId),
}

#[derive(Resource, Debug, Clone, Default)]
pub struct RaidRegistry {
    pub raids: Vec<RaidRecord>,
    pub next_id: u16,
}

impl RaidRegistry {
    pub fn from_records(records: Vec<RaidRecord>) -> Self {
        let next_id = records
            .iter()
            .map(|record| record.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        Self {
            raids: records,
            next_id,
        }
    }

    pub fn to_save_records(&self) -> Vec<RaidRecord> {
        self.raids.clone()
    }

    pub fn apply_save_records(&mut self, records: Vec<RaidRecord>) {
        *self = Self::from_records(records);
    }

    pub fn get(&self, id: RaidId) -> Option<&RaidRecord> {
        self.raids.iter().find(|raid| raid.id == id)
    }

    pub fn get_mut(&mut self, id: RaidId) -> Option<&mut RaidRecord> {
        self.raids.iter_mut().find(|raid| raid.id == id)
    }

    pub fn unresolved_for_site(&self, target_site: WorldSiteId) -> bool {
        self.raids
            .iter()
            .any(|raid| raid.target_site == target_site && raid.is_unresolved())
    }

    pub fn has_cloudrail_tutorial_record(&self) -> bool {
        self.raids.iter().any(|raid| {
            raid.kind == RaidKind::DroneSwarm && raid.target_site == CLOUDRAIL_TUTORIAL_RAID_SITE
        })
    }

    #[allow(dead_code)]
    pub fn active_spawn_requests(&self) -> Vec<(RaidId, WorldSiteId, RaidKind, u8)> {
        self.raids
            .iter()
            .filter(|raid| raid.phase == RaidPhase::Active && !raid.spawned)
            .map(|raid| (raid.id, raid.target_site, raid.kind, raid.wave_size))
            .collect()
    }

    pub fn spawned_active_ids(&self) -> Vec<RaidId> {
        self.raids
            .iter()
            .filter(|raid| raid.phase == RaidPhase::Active && raid.spawned)
            .map(|raid| raid.id)
            .collect()
    }

    pub fn mark_spawned(&mut self, id: RaidId) {
        if let Some(raid) = self.get_mut(id) {
            raid.spawned = true;
        }
    }

    pub fn is_resolved(&self, id: RaidId) -> bool {
        self.get(id)
            .map(|raid| raid.phase == RaidPhase::Resolved)
            .unwrap_or(true)
    }

    pub fn try_start_cloudrail_tutorial(
        &mut self,
        sites: &mut WorldSiteRegistry,
        defense_score: u32,
    ) -> RaidStartResult {
        if self.has_cloudrail_tutorial_record()
            || self.unresolved_for_site(CLOUDRAIL_TUTORIAL_RAID_SITE)
        {
            return RaidStartResult::AlreadyExists;
        }

        let Some(site) = sites.get_mut(CLOUDRAIL_TUTORIAL_RAID_SITE) else {
            return RaidStartResult::TargetNotLiberated;
        };
        if !site.is_liberated() {
            return RaidStartResult::TargetNotLiberated;
        }

        let id = self.allocate_id();
        let mut raid = RaidRecord::cloudrail_tutorial(id);
        if defense_score >= raid.required_defense_score {
            raid.phase = RaidPhase::Resolved;
            raid.resolution = Some(RaidResolution::DefendedByStaticForces);
            raid.consequence = RaidConsequence::None;
            site.state = WorldSiteState::Shielded;
            self.raids.push(raid);
            RaidStartResult::AutoResolved(id)
        } else {
            site.state = WorldSiteState::UnderAttack;
            self.raids.push(raid);
            RaidStartResult::Started(id)
        }
    }

    pub fn tick(
        &mut self,
        dt: f32,
        sites: &mut WorldSiteRegistry,
        defense_scores: &[(WorldSiteId, u32)],
    ) -> Vec<RaidTickEvent> {
        let mut events = Vec::new();
        let dt = dt.max(0.0);
        for raid in &mut self.raids {
            match raid.phase {
                RaidPhase::Warning => {
                    let defense_score = defense_scores
                        .iter()
                        .find(|(site, _)| *site == raid.target_site)
                        .map(|(_, score)| *score)
                        .unwrap_or(0);
                    if defense_score >= raid.required_defense_score {
                        raid.phase = RaidPhase::Resolved;
                        raid.resolution = Some(RaidResolution::DefendedByStaticForces);
                        raid.consequence = RaidConsequence::None;
                        if let Some(site) = sites.get_mut(raid.target_site) {
                            site.state = WorldSiteState::Shielded;
                        }
                        events.push(RaidTickEvent::AutoDefended(raid.id, raid.target_site));
                        continue;
                    }

                    raid.warning_timer = (raid.warning_timer - dt).max(0.0);
                    if raid.warning_timer <= 0.0 {
                        raid.phase = RaidPhase::Active;
                        events.push(RaidTickEvent::WarningExpired(raid.id));
                    }
                }
                RaidPhase::Active => {
                    raid.active_timer = (raid.active_timer - dt).max(0.0);
                    if raid.active_timer <= 0.0 {
                        raid.phase = RaidPhase::Resolved;
                        raid.resolution = Some(RaidResolution::Failed);
                        raid.consequence = RaidConsequence::SiteDamaged;
                        if let Some(site) = sites.get_mut(raid.target_site) {
                            site.state = WorldSiteState::Damaged;
                        }
                        events.push(RaidTickEvent::Failed(raid.id, raid.target_site));
                    }
                }
                RaidPhase::Resolved => {}
            }
        }
        events
    }

    pub fn resolve_combat_success(&mut self, id: RaidId, sites: &mut WorldSiteRegistry) -> bool {
        let Some(raid) = self.get_mut(id) else {
            return false;
        };
        if raid.phase != RaidPhase::Active {
            return false;
        }
        raid.phase = RaidPhase::Resolved;
        raid.resolution = Some(RaidResolution::ClearedByPlayers);
        raid.consequence = RaidConsequence::None;
        if let Some(site) = sites.get_mut(raid.target_site) {
            site.state = WorldSiteState::Liberated;
            site.owner = crate::resources::WorldSiteOwner::PlayerAlliance;
        }
        true
    }

    pub fn push_new_raid(
        &mut self,
        kind: RaidKind,
        target_site: WorldSiteId,
        attacker: crate::components::faction::Faction,
        warning_timer: f32,
        active_timer: f32,
        required_defense_score: u32,
        wave_size: u8,
        consequence: RaidConsequence,
    ) -> RaidId {
        let id = self.allocate_id();
        self.raids.push(RaidRecord {
            id,
            kind,
            phase: RaidPhase::Warning,
            target_site,
            target_route: None,
            attacker,
            warning_timer,
            active_timer,
            required_defense_score,
            spawned: false,
            wave_size,
            consequence,
            resolution: None,
        });
        id
    }

    fn allocate_id(&mut self) -> RaidId {
        self.next_id = self.next_id.max(1);
        let id = RaidId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct RaidThreatMarker {
    pub raid_id: RaidId,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct RaidUfoMarker {
    pub raid_id: RaidId,
}

pub fn static_defense_score_for_site(site_id: WorldSiteId, economy: &SettlementEconomy) -> u32 {
    let Some(settlement_id) = settlement_anchor_for_site(site_id) else {
        return 0;
    };
    economy.build_tier(settlement_id, SettlementBuildKind::DefenseOutpost) * 12
        + economy.build_tier(settlement_id, SettlementBuildKind::PowerPlant) * 4
        + economy.build_tier(settlement_id, SettlementBuildKind::Spaceport) * 5
        + economy.build_tier(settlement_id, SettlementBuildKind::ResearchLab) * 3
}

fn settlement_anchor_for_site(site_id: WorldSiteId) -> Option<&'static str> {
    map_settlements()
        .iter()
        .find(|settlement| settlement.site_id == Some(site_id.0))
        .map(|settlement| settlement.anchor_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{initial_world_sites, WorldSiteOwner};

    fn sites() -> WorldSiteRegistry {
        WorldSiteRegistry {
            sites: initial_world_sites(),
        }
    }

    #[test]
    fn liberated_site_can_start_warning_raid_and_becomes_under_attack() {
        let mut raids = RaidRegistry::default();
        let mut sites = sites();

        let result = raids.try_start_cloudrail_tutorial(&mut sites, 0);

        assert_eq!(result, RaidStartResult::Started(RaidId(1)));
        assert_eq!(
            sites.get(CLOUDRAIL_TUTORIAL_RAID_SITE).unwrap().state,
            WorldSiteState::UnderAttack
        );
        assert_eq!(raids.raids[0].phase, RaidPhase::Warning);
    }

    #[test]
    fn defense_score_can_auto_resolve_warning_raid() {
        let mut raids = RaidRegistry::default();
        let mut sites = sites();

        let result = raids
            .try_start_cloudrail_tutorial(&mut sites, CLOUDRAIL_TUTORIAL_RAID_REQUIRED_DEFENSE);

        assert_eq!(result, RaidStartResult::AutoResolved(RaidId(1)));
        assert_eq!(raids.raids[0].phase, RaidPhase::Resolved);
        assert_eq!(
            raids.raids[0].resolution,
            Some(RaidResolution::DefendedByStaticForces)
        );
        assert_eq!(
            sites.get(CLOUDRAIL_TUTORIAL_RAID_SITE).unwrap().state,
            WorldSiteState::Shielded
        );
    }

    #[test]
    fn active_raid_success_returns_site_to_liberated() {
        let mut raids = RaidRegistry::default();
        let mut sites = sites();
        raids.try_start_cloudrail_tutorial(&mut sites, 0);
        raids.tick(
            CLOUDRAIL_TUTORIAL_RAID_WARNING_SECONDS + 0.1,
            &mut sites,
            &[(CLOUDRAIL_TUTORIAL_RAID_SITE, 0)],
        );

        assert!(raids.resolve_combat_success(RaidId(1), &mut sites));
        let site = sites.get(CLOUDRAIL_TUTORIAL_RAID_SITE).unwrap();
        assert_eq!(site.state, WorldSiteState::Liberated);
        assert_eq!(site.owner, WorldSiteOwner::PlayerAlliance);
        assert_eq!(
            raids.get(RaidId(1)).unwrap().resolution,
            Some(RaidResolution::ClearedByPlayers)
        );
    }

    #[test]
    fn failed_raid_marks_site_damaged_not_enemy_held() {
        let mut raids = RaidRegistry::default();
        let mut sites = sites();
        raids.try_start_cloudrail_tutorial(&mut sites, 0);
        raids.tick(
            CLOUDRAIL_TUTORIAL_RAID_WARNING_SECONDS + 0.1,
            &mut sites,
            &[(CLOUDRAIL_TUTORIAL_RAID_SITE, 0)],
        );
        let events = raids.tick(
            CLOUDRAIL_TUTORIAL_RAID_ACTIVE_SECONDS + 0.1,
            &mut sites,
            &[(CLOUDRAIL_TUTORIAL_RAID_SITE, 0)],
        );

        assert_eq!(
            events,
            vec![RaidTickEvent::Failed(
                RaidId(1),
                CLOUDRAIL_TUTORIAL_RAID_SITE
            )]
        );
        assert_eq!(
            sites.get(CLOUDRAIL_TUTORIAL_RAID_SITE).unwrap().state,
            WorldSiteState::Damaged
        );
    }

    #[test]
    fn duplicate_active_raids_cannot_target_same_site() {
        let mut raids = RaidRegistry::default();
        let mut sites = sites();

        assert_eq!(
            raids.try_start_cloudrail_tutorial(&mut sites, 0),
            RaidStartResult::Started(RaidId(1))
        );
        assert_eq!(
            raids.try_start_cloudrail_tutorial(&mut sites, 0),
            RaidStartResult::AlreadyExists
        );
    }

    #[test]
    fn raid_save_records_round_trip() {
        let mut raids = RaidRegistry::default();
        let mut sites = sites();
        raids.try_start_cloudrail_tutorial(&mut sites, 0);
        raids.tick(
            CLOUDRAIL_TUTORIAL_RAID_WARNING_SECONDS + 0.1,
            &mut sites,
            &[(CLOUDRAIL_TUTORIAL_RAID_SITE, 0)],
        );

        let loaded = RaidRegistry::from_records(raids.to_save_records());

        assert_eq!(loaded.raids.len(), 1);
        assert_eq!(loaded.next_id, 2);
        assert_eq!(loaded.raids[0].id, RaidId(1));
        assert_eq!(loaded.raids[0].phase, RaidPhase::Active);
        assert_eq!(loaded.raids[0].target_site, CLOUDRAIL_TUTORIAL_RAID_SITE);
    }
}
