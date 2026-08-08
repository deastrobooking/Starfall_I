//! Aggregate durable state for the offline Heavy Water continuation domains.
//!
//! Individual modules keep their own schema/version rules. This resource is
//! the single Bevy/save seam, so gameplay never serializes ECS entity IDs and
//! older Starfall saves receive safe defaults for every new domain.

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

use super::heavy_bio::HeavyBioSave;
use super::heavy_combat::HeavyCombatSave;
use super::heavy_economy::HeavyEconomyState;
use super::heavy_regions::{LegacyRegionId, PropStateRegistry, RegionSession};
use super::heavy_vehicle::HeavyVehicleFleet;

pub const HEAVY_WATER_PROGRESS_SCHEMA_VERSION: u16 = 1;

#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeavyWaterProgress {
    pub schema_version: u16,
    pub economy: HeavyEconomyState,
    pub bio: HeavyBioSave,
    pub combat: HeavyCombatSave,
    pub regions: RegionSession,
    pub props: PropStateRegistry,
    pub vehicles: HeavyVehicleFleet,
    /// Monotonic source for replay-protected offline economy operations.
    /// Persisting it prevents a fresh process from reusing an applied ID.
    pub next_economy_transaction_id: u64,
}

impl Default for HeavyWaterProgress {
    fn default() -> Self {
        Self::new(0)
    }
}

impl HeavyWaterProgress {
    pub fn new(world_seed: u64) -> Self {
        Self {
            schema_version: HEAVY_WATER_PROGRESS_SCHEMA_VERSION,
            economy: HeavyEconomyState::new(world_seed),
            bio: HeavyBioSave::default(),
            combat: HeavyCombatSave::default(),
            regions: RegionSession::new(LegacyRegionId::DetroitStarCityFront),
            props: PropStateRegistry::default(),
            vehicles: HeavyVehicleFleet::default(),
            next_economy_transaction_id: 1,
        }
    }

    /// Normalize forward-compatible player records. A structurally invalid
    /// economy domain is reset in isolation so it cannot make the otherwise
    /// valid rotating campaign save unreadable.
    pub fn normalize(&mut self, fallback_world_seed: u64) -> bool {
        self.schema_version = HEAVY_WATER_PROGRESS_SCHEMA_VERSION;
        self.bio.normalize();
        self.combat.normalize();
        let mut repaired = false;
        if self.economy.validate().is_err() {
            self.economy = HeavyEconomyState::new(fallback_world_seed);
            repaired = true;
        } else {
            if self.economy.world_seed == 0 && fallback_world_seed != 0 {
                self.economy.world_seed = fallback_world_seed;
            }
        }
        if self.regions.validate().is_err() {
            self.regions = RegionSession::new(LegacyRegionId::DetroitStarCityFront);
            repaired = true;
        }
        if self.props.validate().is_err() {
            self.props = PropStateRegistry::default();
            repaired = true;
        }
        if self.vehicles.validate().is_err() {
            self.vehicles = HeavyVehicleFleet::default();
            repaired = true;
        }
        let minimum_next_transaction = self
            .economy
            .owners
            .values()
            .filter_map(|account| account.applied_transactions.last().copied())
            .max()
            .map_or(1, |last| last.saturating_add(1));
        if self.next_economy_transaction_id < minimum_next_transaction
            || self.next_economy_transaction_id == 0
        {
            self.next_economy_transaction_id = minimum_next_transaction;
            repaired = true;
        }
        repaired
    }

    pub fn allocate_economy_transaction_id(&mut self) -> Option<u64> {
        let transaction_id = self.next_economy_transaction_id;
        if transaction_id == 0 || transaction_id == u64::MAX {
            return None;
        }
        self.next_economy_transaction_id += 1;
        Some(transaction_id)
    }
}

/// Stable key shared by split-screen player slots and their Bio save records.
pub fn local_player_key(player_index: u8) -> String {
    format!("local:p{}", player_index.min(3) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_save_default_is_valid_and_accepts_runtime_world_seed() {
        let mut progress = HeavyWaterProgress::default();
        assert!(!progress.normalize(42));
        assert_eq!(progress.economy.world_seed, 42);
        assert_eq!(progress.schema_version, HEAVY_WATER_PROGRESS_SCHEMA_VERSION);
    }

    #[test]
    fn player_keys_are_stable_and_bounded_to_local_party_slots() {
        assert_eq!(local_player_key(0), "local:p1");
        assert_eq!(local_player_key(3), "local:p4");
        assert_eq!(local_player_key(99), "local:p4");
    }

    #[test]
    fn aggregate_progress_round_trips_without_ecs_identity() {
        let mut progress = HeavyWaterProgress::new(77);
        progress.bio.player_mut(local_player_key(2)).garden_level = 3;
        let json = serde_json::to_string(&progress).unwrap();
        assert!(!json.contains("Entity"));
        let decoded: HeavyWaterProgress = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, progress);
    }

    #[test]
    fn economy_transaction_ids_are_monotonic_and_repaired_from_history() {
        let mut progress = HeavyWaterProgress::default();
        assert_eq!(progress.allocate_economy_transaction_id(), Some(1));
        assert_eq!(progress.allocate_economy_transaction_id(), Some(2));

        let owner = crate::world::heavy_economy::OwnerId(0);
        progress.economy.register_owner(owner);
        progress
            .economy
            .owners
            .get_mut(&owner)
            .unwrap()
            .applied_transactions
            .insert(50);
        assert!(progress.normalize(0));
        assert_eq!(progress.allocate_economy_transaction_id(), Some(51));
    }
}
