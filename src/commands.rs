use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::raids::RaidId;
use crate::resources::{WorldRouteId, WorldSiteId};

// ── Asset Kinds ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandAssetKind {
    WorkerDrone,
    ScoutDrone,
    FighterDrone,
    TurretDrone,
    GroundMech,
    Boat,
    FighterJet,
    Ship,
    MegaShip,
}

impl CommandAssetKind {
    pub fn label(self) -> &'static str {
        match self {
            CommandAssetKind::WorkerDrone => "Worker Drone",
            CommandAssetKind::ScoutDrone => "Scout Drone",
            CommandAssetKind::FighterDrone => "Fighter Drone",
            CommandAssetKind::TurretDrone => "Turret Drone",
            CommandAssetKind::GroundMech => "Ground Mech",
            CommandAssetKind::Boat => "Patrol Boat",
            CommandAssetKind::FighterJet => "Fighter Jet",
            CommandAssetKind::Ship => "Ship",
            CommandAssetKind::MegaShip => "Megaship",
        }
    }

    pub fn base_defense(self) -> u32 {
        match self {
            CommandAssetKind::WorkerDrone => 0,
            CommandAssetKind::ScoutDrone => 1,
            CommandAssetKind::FighterDrone => 4,
            CommandAssetKind::TurretDrone => 5,
            CommandAssetKind::GroundMech => 6,
            CommandAssetKind::Boat => 2,
            CommandAssetKind::FighterJet => 5,
            CommandAssetKind::Ship => 8,
            CommandAssetKind::MegaShip => 12,
        }
    }
}

// ── Assignment ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CommandAssetAssignment {
    #[default]
    Unassigned,
    Site(WorldSiteId),
    Route(WorldRouteId),
    Raid(RaidId),
}

impl CommandAssetAssignment {
    pub fn label(self) -> String {
        match self {
            CommandAssetAssignment::Unassigned => "Reserve".to_string(),
            CommandAssetAssignment::Site(id) => format!("Site {}", id.0),
            CommandAssetAssignment::Route(id) => format!("Route {}", id.0),
            CommandAssetAssignment::Raid(id) => format!("Raid {}", id.0),
        }
    }
}

// ── Command Asset ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandAssetId(pub u16);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandAsset {
    pub id: CommandAssetId,
    pub kind: CommandAssetKind,
    /// 0–100; 0 means destroyed, needs repair.
    pub health: u32,
    /// 0–100; below 30 the asset is too depleted to contribute.
    pub readiness: u32,
    pub assignment: CommandAssetAssignment,
}

impl CommandAsset {
    pub fn new(id: CommandAssetId, kind: CommandAssetKind) -> Self {
        Self {
            id,
            kind,
            health: 100,
            readiness: 100,
            assignment: CommandAssetAssignment::default(),
        }
    }

    /// Defense points this asset contributes when assigned to a site or raid.
    pub fn defense_contribution(&self) -> u32 {
        if self.health == 0 || self.readiness < 30 {
            return 0;
        }
        let base = self.kind.base_defense();
        (base * self.readiness / 100).saturating_mul(self.health.min(100)) / 100
    }

    pub fn is_combat_ready(&self) -> bool {
        self.health > 0 && self.readiness >= 30
    }
}

// ── Compact Save Record ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandAssetSaveRecord {
    pub id: u16,
    pub kind: CommandAssetKind,
    pub health: u32,
    pub readiness: u32,
    pub assignment: CommandAssetAssignment,
}

// ── Registry ──────────────────────────────────────────────────────────────────

#[derive(Resource, Default, Debug, Clone)]
pub struct CommandRegistry {
    pub assets: Vec<CommandAsset>,
    next_id: u16,
}

impl CommandRegistry {
    pub fn get(&self, id: CommandAssetId) -> Option<&CommandAsset> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn get_mut(&mut self, id: CommandAssetId) -> Option<&mut CommandAsset> {
        self.assets.iter_mut().find(|a| a.id == id)
    }

    pub fn assign(&mut self, id: CommandAssetId, assignment: CommandAssetAssignment) {
        if let Some(asset) = self.get_mut(id) {
            asset.assignment = assignment;
        }
    }

    pub fn add_asset(&mut self, kind: CommandAssetKind) -> CommandAssetId {
        let id = self.allocate_id();
        self.assets.push(CommandAsset::new(id, kind));
        id
    }

    /// Total defense score contributed by assets assigned to `site_id`.
    pub fn defense_score_for_site(&self, site_id: WorldSiteId) -> u32 {
        self.assets
            .iter()
            .filter(|a| a.assignment == CommandAssetAssignment::Site(site_id))
            .map(|a| a.defense_contribution())
            .sum()
    }

    pub fn assets_at_site(&self, site_id: WorldSiteId) -> Vec<&CommandAsset> {
        self.assets
            .iter()
            .filter(|a| a.assignment == CommandAssetAssignment::Site(site_id))
            .collect()
    }

    pub fn unassigned(&self) -> Vec<&CommandAsset> {
        self.assets
            .iter()
            .filter(|a| a.assignment == CommandAssetAssignment::Unassigned)
            .collect()
    }

    pub fn to_save_records(&self) -> Vec<CommandAssetSaveRecord> {
        self.assets
            .iter()
            .map(|a| CommandAssetSaveRecord {
                id: a.id.0,
                kind: a.kind,
                health: a.health,
                readiness: a.readiness,
                assignment: a.assignment,
            })
            .collect()
    }

    pub fn apply_save_records(&mut self, records: &[CommandAssetSaveRecord]) {
        for rec in records {
            let id = CommandAssetId(rec.id);
            if let Some(asset) = self.get_mut(id) {
                asset.health = rec.health;
                asset.readiness = rec.readiness;
                asset.assignment = rec.assignment;
            } else {
                self.assets.push(CommandAsset {
                    id,
                    kind: rec.kind,
                    health: rec.health,
                    readiness: rec.readiness,
                    assignment: rec.assignment,
                });
                self.next_id = self.next_id.max(rec.id + 1);
            }
        }
    }

    fn allocate_id(&mut self) -> CommandAssetId {
        let next_free = self
            .assets
            .iter()
            .map(|asset| asset.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_id = self.next_id.max(next_free).max(1);
        let id = CommandAssetId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }
}

/// Starter commandable assets available at campaign begin.
pub fn initial_command_assets() -> Vec<CommandAsset> {
    vec![
        CommandAsset::new(CommandAssetId(1), CommandAssetKind::WorkerDrone),
        CommandAsset::new(CommandAssetId(2), CommandAssetKind::WorkerDrone),
        CommandAsset::new(CommandAssetId(3), CommandAssetKind::ScoutDrone),
    ]
}

// ── Overlay State ─────────────────────────────────────────────────────────────

#[derive(Resource, Default, Debug)]
pub struct CommandOverlayState {
    pub enabled: bool,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with_fighter_at_site(site_id: WorldSiteId) -> CommandRegistry {
        let mut reg = CommandRegistry::default();
        let mut asset = CommandAsset::new(CommandAssetId(1), CommandAssetKind::FighterDrone);
        asset.assignment = CommandAssetAssignment::Site(site_id);
        reg.assets.push(asset);
        reg
    }

    #[test]
    fn fighter_drone_at_site_contributes_defense() {
        let site = WorldSiteId(4);
        let reg = registry_with_fighter_at_site(site);
        assert!(reg.defense_score_for_site(site) > 0);
        assert_eq!(reg.defense_score_for_site(WorldSiteId(5)), 0);
    }

    #[test]
    fn low_readiness_asset_gives_no_defense() {
        let site = WorldSiteId(1);
        let mut reg = CommandRegistry::default();
        let mut asset = CommandAsset::new(CommandAssetId(1), CommandAssetKind::FighterDrone);
        asset.readiness = 10;
        asset.assignment = CommandAssetAssignment::Site(site);
        reg.assets.push(asset);
        assert_eq!(reg.defense_score_for_site(site), 0);
    }

    #[test]
    fn save_round_trip_preserves_assignment_and_health() {
        let site = WorldSiteId(4);
        let mut reg = CommandRegistry::default();
        let mut asset = CommandAsset::new(CommandAssetId(1), CommandAssetKind::Ship);
        asset.health = 75;
        asset.assignment = CommandAssetAssignment::Site(site);
        reg.assets.push(asset);

        let records = reg.to_save_records();
        let mut restored = CommandRegistry {
            assets: initial_command_assets(),
            ..CommandRegistry::default()
        };
        restored
            .assets
            .push(CommandAsset::new(CommandAssetId(1), CommandAssetKind::Ship));
        restored.apply_save_records(&records);

        let restored_asset = restored.get(CommandAssetId(1)).unwrap();
        assert_eq!(restored_asset.health, 75);
        assert_eq!(
            restored_asset.assignment,
            CommandAssetAssignment::Site(site)
        );
    }

    #[test]
    fn initial_command_assets_starts_with_worker_and_scout_drones() {
        let assets = initial_command_assets();
        let worker_count = assets
            .iter()
            .filter(|a| a.kind == CommandAssetKind::WorkerDrone)
            .count();
        let scout_count = assets
            .iter()
            .filter(|a| a.kind == CommandAssetKind::ScoutDrone)
            .count();
        assert_eq!(worker_count, 2);
        assert_eq!(scout_count, 1);
    }

    #[test]
    fn adding_asset_after_seeded_starters_uses_next_free_id() {
        let mut reg = CommandRegistry {
            assets: initial_command_assets(),
            ..CommandRegistry::default()
        };

        let id = reg.add_asset(CommandAssetKind::FighterDrone);

        assert_eq!(id, CommandAssetId(4));
        assert_eq!(reg.assets.last().unwrap().id, CommandAssetId(4));
    }
}
