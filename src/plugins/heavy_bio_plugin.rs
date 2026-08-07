//! Bevy/runtime bridge for the Heavy Water Bio domain.
//!
//! The domain in [`crate::world::heavy_bio`] deliberately knows nothing about
//! ECS entities or Starfall's inventory. This module supplies that seam while
//! preserving two ownership rules:
//!
//! - [`HeavyWaterProgress`](crate::world::heavy_water::HeavyWaterProgress) owns
//!   durable Bio records, keyed by local player slot rather than ECS entity ID.
//! - [`Inventory`] remains the item authority. Bio operations apply atomic
//!   deltas to the current component; no inventory snapshot is mirrored into
//!   the Heavy Water save.
#![allow(dead_code)] // Public no-UI bridge API; gameplay callers land incrementally.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::inventory::{max_stack_for, Inventory};
use crate::components::player::{Player, PlayerIndex};
use crate::engine::state::AppState;
use crate::world::heavy_bio::{
    CaptureError, CaptureRequest, CaptureResolution, CareError, CareItem, CareOutcome,
    CompanionError, CompanionLifeState, CompanionRecord, CompanionRosterRecord,
    CompanionUpgradeCost, GardenError, HarvestYield, PlantOutcome, PlayerBioRecord, RescueReward,
    RescueUpdate,
};
use crate::world::heavy_water::{local_player_key, HeavyWaterProgress};

pub const BIO_ESSENCE_ITEM_ID: &str = "bio_essence";
pub const BIO_SEED_ITEM_ID: &str = "bio_seed";
pub const BIO_CROP_ITEM_ID: &str = "bio_crop";
pub const ANIMATON_FEED_ITEM_ID: &str = "animaton_feed";
pub const GEAR_ITEM_ID: &str = "gear";
pub const ENERGY_CORE_ITEM_ID: &str = "energy_core";

const NANOS_PER_MILLISECOND: u32 = 1_000_000;

/// Attaches a stable save key to a local player entity.
///
/// The string is intentionally derived from [`PlayerIndex`], never `Entity`,
/// so despawning and recreating a split-screen player does not fork progress.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct HeavyBioPlayerKey(pub String);

/// Deterministic gameplay clock used by absolute Bio Garden timestamps.
///
/// The fractional remainder is retained between frames, so thirty short
/// frames and one equivalent long frame advance gardens by exactly the same
/// integer number of milliseconds. Serialize this resource alongside
/// [`HeavyWaterProgress`] to preserve garden timing across save/load.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HeavyBioClock {
    pub game_time_ms: u64,
    submillisecond_nanos: u32,
}

impl HeavyBioClock {
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

    /// Add gameplay time exactly, saturating rather than wrapping malformed or
    /// extremely old saves.
    pub fn advance(&mut self, delta: Duration) -> u64 {
        self.normalize();
        let total_nanos = delta
            .as_nanos()
            .saturating_add(u128::from(self.submillisecond_nanos));
        let whole_ms = total_nanos / u128::from(NANOS_PER_MILLISECOND);
        let available_ms = u128::from(u64::MAX - self.game_time_ms);

        if whole_ms > available_ms {
            self.game_time_ms = u64::MAX;
            self.submillisecond_nanos = 0;
        } else {
            self.game_time_ms += whole_ms as u64;
            self.submillisecond_nanos = (total_nanos % u128::from(NANOS_PER_MILLISECOND)) as u32;
        }
        self.game_time_ms
    }

    /// Repair an externally deserialized fractional value without losing its
    /// whole milliseconds.
    pub fn normalize(&mut self) {
        let carry = self.submillisecond_nanos / NANOS_PER_MILLISECOND;
        self.submillisecond_nanos %= NANOS_PER_MILLISECOND;
        self.game_time_ms = self.game_time_ms.saturating_add(u64::from(carry));
    }
}

/// Last deterministic garden pass. This is diagnostic/gameplay state, not UI.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeavyBioTickReport {
    pub game_time_ms: u64,
    pub local_players_seen: usize,
    pub records_created: usize,
    /// Number of plots whose visible stage changed during this pass.
    pub plots_advanced: usize,
}

pub struct HeavyBioPlugin;

impl Plugin for HeavyBioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeavyWaterProgress>()
            .init_resource::<HeavyBioClock>()
            .init_resource::<HeavyBioTickReport>()
            .add_systems(
                Update,
                tick_heavy_bio_players.run_if(in_state(AppState::Playing)),
            );
    }
}

/// Advances and creates Bio records once per distinct local slot.
///
/// This public pure-ish bridge is also useful to deterministic simulations
/// that do not run a full Bevy schedule.
pub fn advance_local_player_gardens<I>(
    progress: &mut HeavyWaterProgress,
    player_slots: I,
    now_ms: u64,
) -> HeavyBioTickReport
where
    I: IntoIterator<Item = u8>,
{
    let player_keys: BTreeSet<String> = player_slots.into_iter().map(local_player_key).collect();
    let mut report = HeavyBioTickReport {
        game_time_ms: now_ms,
        local_players_seen: player_keys.len(),
        ..Default::default()
    };

    for player_key in player_keys {
        if progress.bio.player(&player_key).is_none() {
            report.records_created += 1;
        }
        report.plots_advanced += progress
            .bio
            .player_mut(player_key)
            .garden
            .advance_growth(now_ms);
    }
    report
}

/// Playing-only ECS bridge: binds stable slot keys, advances the save clock,
/// and grows every currently spawned local player's garden.
pub fn tick_heavy_bio_players(
    mut commands: Commands,
    time: Res<Time>,
    players: Query<(Entity, &PlayerIndex, Option<&HeavyBioPlayerKey>), With<Player>>,
    mut progress: ResMut<HeavyWaterProgress>,
    mut clock: ResMut<HeavyBioClock>,
    mut last_report: ResMut<HeavyBioTickReport>,
) {
    let now_ms = clock.advance(time.delta());
    let mut slots = Vec::new();

    for (entity, player_index, current_key) in &players {
        let expected_key = local_player_key(player_index.0);
        if current_key.map(|key| key.0.as_str()) != Some(expected_key.as_str()) {
            commands
                .entity(entity)
                .insert(HeavyBioPlayerKey(expected_key));
        }
        slots.push(player_index.0);
    }

    *last_report = advance_local_player_gardens(&mut progress, slots, now_ms);
}

/// A net set of changes to apply to Starfall's canonical inventory.
/// Positive quantities grant items; negative quantities consume them.
///
/// Entries are coalesced by stable item ID, which prevents ordering quirks
/// when an operation both consumes and awards the same item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryEffect {
    deltas: BTreeMap<&'static str, i64>,
}

impl InventoryEffect {
    pub fn grant(mut self, item_id: &'static str, quantity: u32) -> Self {
        self.add_delta(item_id, i64::from(quantity));
        self
    }

    pub fn consume(mut self, item_id: &'static str, quantity: u32) -> Self {
        self.add_delta(item_id, -i64::from(quantity));
        self
    }

    pub fn delta(&self, item_id: &str) -> i64 {
        self.deltas.get(item_id).copied().unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.deltas.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, i64)> + '_ {
        self.deltas
            .iter()
            .map(|(item_id, delta)| (*item_id, *delta))
    }

    fn add_delta(&mut self, item_id: &'static str, delta: i64) {
        let next = self
            .deltas
            .get(item_id)
            .copied()
            .unwrap_or_default()
            .saturating_add(delta);
        if next == 0 {
            self.deltas.remove(item_id);
        } else {
            self.deltas.insert(item_id, next);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryEffectError {
    MissingItem {
        item_id: &'static str,
        needed: u32,
        available: u32,
    },
    InsufficientCapacity {
        item_id: &'static str,
        requested: u32,
        leftover: u32,
    },
    QuantityOutOfRange {
        item_id: &'static str,
    },
}

/// Applies an inventory effect as an all-or-nothing transaction.
///
/// The staged value starts from the live inventory and is committed only if
/// every removal and grant succeeds. It never replaces the component from Bio
/// save data, because no inventory copy exists there.
pub fn apply_inventory_effect(
    inventory: &mut Inventory,
    effect: &InventoryEffect,
) -> Result<(), InventoryEffectError> {
    let mut staged = inventory.clone();

    // Apply net consumption first so an operation can reuse newly empty slots.
    for (item_id, delta) in effect.iter().filter(|(_, delta)| *delta < 0) {
        let quantity = u32::try_from(delta.unsigned_abs())
            .map_err(|_| InventoryEffectError::QuantityOutOfRange { item_id })?;
        let available = staged.count(item_id);
        if available < quantity {
            return Err(InventoryEffectError::MissingItem {
                item_id,
                needed: quantity,
                available,
            });
        }
        let removed = staged.remove_item(item_id, quantity);
        debug_assert!(removed, "inventory preflight and removal diverged");
    }

    for (item_id, delta) in effect.iter().filter(|(_, delta)| *delta > 0) {
        let quantity = u32::try_from(delta)
            .map_err(|_| InventoryEffectError::QuantityOutOfRange { item_id })?;
        let leftover = staged.add_item(item_id, quantity, max_stack_for(item_id));
        if leftover != 0 {
            return Err(InventoryEffectError::InsufficientCapacity {
                item_id,
                requested: quantity,
                leftover,
            });
        }
    }

    *inventory = staged;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppliedInventoryEffect<T> {
    pub outcome: T,
    /// The already-applied effect, retained so callers can emit gameplay or
    /// inventory-changed messages without reconstructing the transaction.
    pub inventory_effect: InventoryEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BioRuntimeError {
    Inventory(InventoryEffectError),
    Capture(CaptureError),
    Garden(GardenError),
    Care(CareError),
    Companion(CompanionError),
    NoCareItem,
}

impl From<InventoryEffectError> for BioRuntimeError {
    fn from(value: InventoryEffectError) -> Self {
        Self::Inventory(value)
    }
}

impl From<CaptureError> for BioRuntimeError {
    fn from(value: CaptureError) -> Self {
        Self::Capture(value)
    }
}

impl From<GardenError> for BioRuntimeError {
    fn from(value: GardenError) -> Self {
        Self::Garden(value)
    }
}

impl From<CareError> for BioRuntimeError {
    fn from(value: CareError) -> Self {
        Self::Care(value)
    }
}

impl From<CompanionError> for BioRuntimeError {
    fn from(value: CompanionError) -> Self {
        Self::Companion(value)
    }
}

pub fn capture_inventory_effect(_resolution: &CaptureResolution) -> InventoryEffect {
    InventoryEffect::default().consume(BIO_ESSENCE_ITEM_ID, 1)
}

pub fn plant_inventory_effect(outcome: PlantOutcome) -> InventoryEffect {
    InventoryEffect::default().consume(BIO_SEED_ITEM_ID, u32::from(outcome.bio_seeds_spent))
}

pub fn harvest_inventory_effect(harvest: HarvestYield) -> InventoryEffect {
    InventoryEffect::default()
        .grant(BIO_CROP_ITEM_ID, u32::from(harvest.bio_crops))
        .grant(ANIMATON_FEED_ITEM_ID, u32::from(harvest.animaton_feed))
        .grant(BIO_SEED_ITEM_ID, u32::from(harvest.bio_seeds))
}

pub fn care_inventory_effect(outcome: CareOutcome) -> InventoryEffect {
    let item_id = match outcome.item_consumed {
        CareItem::BioCrop => BIO_CROP_ITEM_ID,
        CareItem::AnimatonFeed => ANIMATON_FEED_ITEM_ID,
    };
    InventoryEffect::default().consume(item_id, 1)
}

pub fn companion_upgrade_inventory_effect(cost: CompanionUpgradeCost) -> InventoryEffect {
    InventoryEffect::default()
        .consume(GEAR_ITEM_ID, cost.gears)
        .consume(ENERGY_CORE_ITEM_ID, cost.energy_cores)
}

fn commit_profile_inventory<T>(
    profile: &mut PlayerBioRecord,
    inventory: &mut Inventory,
    staged_profile: PlayerBioRecord,
    outcome: T,
    inventory_effect: InventoryEffect,
) -> Result<AppliedInventoryEffect<T>, BioRuntimeError> {
    apply_inventory_effect(inventory, &inventory_effect)?;
    *profile = staged_profile;
    Ok(AppliedInventoryEffect {
        outcome,
        inventory_effect,
    })
}

/// Attempts a capture and spends one Bio Essence only for a valid attempt.
/// Both success and breaking free consume the orb; invalid targets, bad health,
/// a full roster, or missing Essence leave both profile and inventory intact.
pub fn try_capture_creature(
    player_key: &str,
    profile: &mut PlayerBioRecord,
    inventory: &mut Inventory,
    request: CaptureRequest<'_>,
) -> Result<AppliedInventoryEffect<CaptureResolution>, BioRuntimeError> {
    let mut staged_profile = profile.clone();
    let outcome = staged_profile.attempt_capture(player_key, request)?;
    let inventory_effect = capture_inventory_effect(&outcome);
    commit_profile_inventory(
        profile,
        inventory,
        staged_profile,
        outcome,
        inventory_effect,
    )
}

pub fn try_plant_bio_seed(
    profile: &mut PlayerBioRecord,
    inventory: &mut Inventory,
    plot_index: usize,
    now_ms: u64,
) -> Result<AppliedInventoryEffect<PlantOutcome>, BioRuntimeError> {
    let mut staged_profile = profile.clone();
    let outcome = staged_profile.garden.plant(plot_index, now_ms)?;
    let inventory_effect = plant_inventory_effect(outcome);
    commit_profile_inventory(
        profile,
        inventory,
        staged_profile,
        outcome,
        inventory_effect,
    )
}

/// Harvests atomically: a full bag cannot clear a grown plot and lose rewards.
pub fn try_harvest_bio_plot(
    player_key: &str,
    profile: &mut PlayerBioRecord,
    inventory: &mut Inventory,
    plot_index: usize,
    now_ms: u64,
) -> Result<AppliedInventoryEffect<HarvestYield>, BioRuntimeError> {
    let mut staged_profile = profile.clone();
    let outcome = staged_profile
        .garden
        .harvest(player_key, plot_index, now_ms)?;
    let inventory_effect = harvest_inventory_effect(outcome);
    commit_profile_inventory(
        profile,
        inventory,
        staged_profile,
        outcome,
        inventory_effect,
    )
}

pub fn try_care_for_creature_with_item(
    profile: &mut PlayerBioRecord,
    inventory: &mut Inventory,
    creature_id: &str,
    item: CareItem,
) -> Result<AppliedInventoryEffect<CareOutcome>, BioRuntimeError> {
    let mut staged_profile = profile.clone();
    let outcome = staged_profile.care_for(creature_id, item)?;
    let inventory_effect = care_inventory_effect(outcome);
    commit_profile_inventory(
        profile,
        inventory,
        staged_profile,
        outcome,
        inventory_effect,
    )
}

/// Uses refined feed first and falls back to a crop, matching Heavy Water's
/// strongest-care-item behavior without introducing a UI selection policy.
pub fn try_care_for_creature(
    profile: &mut PlayerBioRecord,
    inventory: &mut Inventory,
    creature_id: &str,
) -> Result<AppliedInventoryEffect<CareOutcome>, BioRuntimeError> {
    let item = if inventory.has(ANIMATON_FEED_ITEM_ID, 1) {
        CareItem::AnimatonFeed
    } else if inventory.has(BIO_CROP_ITEM_ID, 1) {
        CareItem::BioCrop
    } else {
        return Err(BioRuntimeError::NoCareItem);
    };
    try_care_for_creature_with_item(profile, inventory, creature_id, item)
}

pub fn try_upgrade_companion(
    profile: &mut PlayerBioRecord,
    inventory: &mut Inventory,
    companion_id: &str,
) -> Result<AppliedInventoryEffect<CompanionUpgradeCost>, BioRuntimeError> {
    let mut staged_profile = profile.clone();
    let outcome = staged_profile.companions.upgrade_companion(companion_id)?;
    let inventory_effect = companion_upgrade_inventory_effect(outcome);
    commit_profile_inventory(
        profile,
        inventory,
        staged_profile,
        outcome,
        inventory_effect,
    )
}

pub fn try_upgrade_companion_weapon(
    profile: &mut PlayerBioRecord,
    inventory: &mut Inventory,
    companion_id: &str,
) -> Result<AppliedInventoryEffect<CompanionUpgradeCost>, BioRuntimeError> {
    let mut staged_profile = profile.clone();
    let outcome = staged_profile.companions.upgrade_weapon(companion_id)?;
    let inventory_effect = companion_upgrade_inventory_effect(outcome);
    commit_profile_inventory(
        profile,
        inventory,
        staged_profile,
        outcome,
        inventory_effect,
    )
}

/// Engine-neutral companion state returned after damage/revival operations.
/// An ECS companion bridge can consume this later without coupling the durable
/// roster to render entities or UI widgets.
#[derive(Debug, Clone, PartialEq)]
pub struct CompanionStateEffect {
    pub companion_id: String,
    pub preset_id: String,
    pub life: CompanionLifeState,
    pub current_health: f32,
    pub revive_count: u32,
}

pub fn companion_state_effect(
    roster: &CompanionRosterRecord,
    companion_id: &str,
) -> Result<CompanionStateEffect, CompanionError> {
    let record = roster
        .companions
        .get(companion_id)
        .ok_or_else(|| CompanionError::UnknownCompanion(companion_id.to_owned()))?;
    Ok(companion_state_effect_from_record(companion_id, record))
}

fn companion_state_effect_from_record(
    companion_id: &str,
    record: &CompanionRecord,
) -> CompanionStateEffect {
    CompanionStateEffect {
        companion_id: companion_id.to_owned(),
        preset_id: record.preset_id.clone(),
        life: record.life,
        current_health: record.current_health,
        revive_count: record.revive_count,
    }
}

pub fn apply_companion_damage(
    roster: &mut CompanionRosterRecord,
    companion_id: &str,
    amount: f32,
) -> Result<CompanionStateEffect, CompanionError> {
    roster.damage(companion_id, amount)?;
    companion_state_effect(roster, companion_id)
}

pub fn revive_companion(
    roster: &mut CompanionRosterRecord,
    companion_id: &str,
) -> Result<CompanionStateEffect, CompanionError> {
    roster.revive(companion_id)?;
    companion_state_effect(roster, companion_id)
}

pub fn revive_durable_companions(roster: &mut CompanionRosterRecord) -> Vec<CompanionStateEffect> {
    roster
        .revive_durable_after_player_respawn()
        .into_iter()
        .filter_map(|companion_id| companion_state_effect(roster, &companion_id).ok())
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RescueRewardEffect {
    pub granted_companion_ids: Vec<String>,
    pub already_owned_presets: Vec<&'static str>,
}

/// Applies rescue rewards idempotently. If the roster is at its current cap,
/// the one-shot legendary reward expands that cap by one (up to the durable
/// domain maximum) instead of silently disappearing.
pub fn apply_rescue_rewards(
    profile: &mut PlayerBioRecord,
    update: &RescueUpdate,
) -> Result<RescueRewardEffect, CompanionError> {
    let mut staged = profile.companions.clone();
    let mut effect = RescueRewardEffect::default();

    for reward in &update.rewards {
        let RescueReward::CompanionPreset(preset_id) = *reward;
        if staged
            .companions
            .values()
            .any(|record| record.preset_id == preset_id)
        {
            effect.already_owned_presets.push(preset_id);
            continue;
        }

        if staged.companions.len() >= usize::from(staged.capacity) {
            let expanded_capacity = staged.capacity.saturating_add(1).min(20);
            if expanded_capacity == staged.capacity {
                return Err(CompanionError::RosterFull {
                    capacity: staged.capacity,
                });
            }
            staged.set_capacity(expanded_capacity);
        }
        effect
            .granted_companion_ids
            .push(staged.add_companion(preset_id, false)?);
    }

    profile.companions = staged;
    Ok(effect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::inventory::InventorySlot;
    use crate::world::heavy_bio::{
        CapturedCreatureRecord, GrowthStage, RescueReward, GROWTH_STAGE_MS,
    };
    use bevy::state::app::StatesPlugin;

    fn inventory_with(items: &[(&str, u32)]) -> Inventory {
        let mut inventory = Inventory::default();
        for (item_id, quantity) in items {
            assert_eq!(
                inventory.add_item(item_id, *quantity, max_stack_for(item_id)),
                0
            );
        }
        inventory
    }

    fn captured_robofox() -> CapturedCreatureRecord {
        CapturedCreatureRecord {
            species_id: "robofox".to_owned(),
            name: "RoboFox".to_owned(),
            level: 1,
            current_health: 60.0,
            max_health: 60.0,
            attack: 10.0,
            speed: 1.0,
            bond_level: 0,
            care: 0,
        }
    }

    #[test]
    fn clock_is_partition_invariant_and_normalizes_fractional_state() {
        let mut split = HeavyBioClock::default();
        split.advance(Duration::from_nanos(400_400));
        split.advance(Duration::from_nanos(600_600));

        let mut whole = HeavyBioClock::default();
        whole.advance(Duration::from_nanos(1_001_000));
        assert_eq!(split, whole);
        assert_eq!(split.game_time_ms, 1);
        assert_eq!(split.submillisecond_nanos(), 1_000);

        assert_eq!(
            HeavyBioClock::from_parts(8, 2_500_000),
            HeavyBioClock::from_parts(10, 500_000)
        );
    }

    #[test]
    fn local_slots_create_one_record_each_and_advance_gardens_once() {
        let mut progress = HeavyWaterProgress::default();
        let key = local_player_key(0);
        progress
            .bio
            .player_mut(key.clone())
            .garden
            .plant(0, 0)
            .unwrap();

        let report = advance_local_player_gardens(
            &mut progress,
            [0, 0, 2],
            GROWTH_STAGE_MS.saturating_mul(2),
        );

        assert_eq!(report.local_players_seen, 2);
        assert_eq!(report.records_created, 1);
        assert_eq!(report.plots_advanced, 1);
        assert_eq!(
            progress.bio.player(&key).unwrap().garden.plots[0].stage,
            GrowthStage::Grown
        );
        assert!(progress.bio.player(&local_player_key(2)).is_some());
    }

    #[test]
    fn plugin_only_binds_and_ticks_players_while_playing() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<AppState>()
            .add_plugins(HeavyBioPlugin);
        let player = app.world_mut().spawn((Player, PlayerIndex(2))).id();

        app.update();
        assert!(app
            .world()
            .entity(player)
            .get::<HeavyBioPlayerKey>()
            .is_none());
        assert!(app
            .world()
            .resource::<HeavyWaterProgress>()
            .bio
            .players
            .is_empty());

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();

        assert_eq!(
            app.world()
                .entity(player)
                .get::<HeavyBioPlayerKey>()
                .map(|key| key.0.as_str()),
            Some("local:p3")
        );
        assert!(app
            .world()
            .resource::<HeavyWaterProgress>()
            .bio
            .player("local:p3")
            .is_some());
    }

    #[test]
    fn inventory_effects_are_atomic_for_missing_items_and_full_bags() {
        let mut missing = inventory_with(&[(BIO_SEED_ITEM_ID, 1)]);
        let original = missing.clone();
        let effect = InventoryEffect::default()
            .consume(BIO_SEED_ITEM_ID, 2)
            .grant(BIO_CROP_ITEM_ID, 2);
        assert!(matches!(
            apply_inventory_effect(&mut missing, &effect),
            Err(InventoryEffectError::MissingItem { .. })
        ));
        assert_eq!(missing, original);

        let mut full = Inventory {
            slots: vec![Some(InventorySlot {
                item_id: "scrap_metal".to_owned(),
                quantity: max_stack_for("scrap_metal"),
            })],
            max_slots: 1,
        };
        let original = full.clone();
        assert!(matches!(
            apply_inventory_effect(
                &mut full,
                &InventoryEffect::default().grant(BIO_CROP_ITEM_ID, 1)
            ),
            Err(InventoryEffectError::InsufficientCapacity { .. })
        ));
        assert_eq!(full, original);
    }

    #[test]
    fn planting_spends_a_seed_only_after_the_domain_accepts_the_plot() {
        let mut profile = PlayerBioRecord::default();
        let mut inventory = inventory_with(&[(BIO_SEED_ITEM_ID, 1)]);
        let before_profile = profile.clone();
        let before_inventory = inventory.clone();
        assert_eq!(
            try_plant_bio_seed(&mut profile, &mut inventory, 99, 10),
            Err(BioRuntimeError::Garden(GardenError::InvalidPlot(99)))
        );
        assert_eq!(profile, before_profile);
        assert_eq!(inventory, before_inventory);

        let applied = try_plant_bio_seed(&mut profile, &mut inventory, 0, 10).unwrap();
        assert_eq!(applied.outcome.bio_seeds_spent, 1);
        assert_eq!(inventory.count(BIO_SEED_ITEM_ID), 0);
        assert_eq!(profile.garden.plots[0].stage, GrowthStage::Seeded);
    }

    #[test]
    fn a_full_inventory_does_not_destroy_a_harvest() {
        let mut profile = PlayerBioRecord::default();
        profile.garden.plant(0, 0).unwrap();
        let before_profile = profile.clone();
        let mut inventory = Inventory {
            slots: vec![Some(InventorySlot {
                item_id: "scrap_metal".to_owned(),
                quantity: max_stack_for("scrap_metal"),
            })],
            max_slots: 1,
        };

        assert!(matches!(
            try_harvest_bio_plot(
                "local:p1",
                &mut profile,
                &mut inventory,
                0,
                GROWTH_STAGE_MS * 2
            ),
            Err(BioRuntimeError::Inventory(
                InventoryEffectError::InsufficientCapacity { .. }
            ))
        ));
        assert_eq!(profile, before_profile);

        let mut roomy_inventory = Inventory::default();
        let applied = try_harvest_bio_plot(
            "local:p1",
            &mut profile,
            &mut roomy_inventory,
            0,
            GROWTH_STAGE_MS * 2,
        )
        .unwrap();
        assert_eq!(
            roomy_inventory.count(BIO_CROP_ITEM_ID),
            u32::from(applied.outcome.bio_crops)
        );
        assert_eq!(profile.garden.plots[0].stage, GrowthStage::Empty);
    }

    #[test]
    fn valid_capture_attempts_spend_essence_but_invalid_ones_do_not() {
        let mut profile = PlayerBioRecord::default();
        let mut inventory = inventory_with(&[(BIO_ESSENCE_ITEM_ID, 2)]);
        let valid = CaptureRequest {
            species_id: "robofox",
            current_health: 10.0,
            max_health: 60.0,
            encounter_seed: 7,
            attempt_index: 0,
        };
        try_capture_creature("local:p1", &mut profile, &mut inventory, valid).unwrap();
        assert_eq!(inventory.count(BIO_ESSENCE_ITEM_ID), 1);

        let before_profile = profile.clone();
        let invalid = CaptureRequest {
            species_id: "not-a-species",
            ..valid
        };
        assert!(matches!(
            try_capture_creature("local:p1", &mut profile, &mut inventory, invalid),
            Err(BioRuntimeError::Capture(CaptureError::UnknownSpecies(_)))
        ));
        assert_eq!(inventory.count(BIO_ESSENCE_ITEM_ID), 1);
        assert_eq!(profile, before_profile);
    }

    #[test]
    fn care_prefers_feed_and_rolls_back_for_an_unknown_creature() {
        let mut profile = PlayerBioRecord::default();
        profile
            .creatures
            .insert("pet-1".to_owned(), captured_robofox());
        let mut inventory = inventory_with(&[(ANIMATON_FEED_ITEM_ID, 1), (BIO_CROP_ITEM_ID, 1)]);

        let applied = try_care_for_creature(&mut profile, &mut inventory, "pet-1").unwrap();
        assert_eq!(applied.outcome.item_consumed, CareItem::AnimatonFeed);
        assert_eq!(inventory.count(ANIMATON_FEED_ITEM_ID), 0);
        assert_eq!(inventory.count(BIO_CROP_ITEM_ID), 1);

        let before = inventory.clone();
        assert!(matches!(
            try_care_for_creature(&mut profile, &mut inventory, "missing"),
            Err(BioRuntimeError::Care(CareError::UnknownCreature(_)))
        ));
        assert_eq!(inventory, before);
    }

    #[test]
    fn companion_upgrades_are_atomic_and_state_effects_track_revival() {
        let mut profile = PlayerBioRecord::default();
        let companion_id = profile.companions.add_companion("SparkPup", false).unwrap();
        let mut inventory = Inventory::default();
        let before = profile.clone();
        assert!(matches!(
            try_upgrade_companion(&mut profile, &mut inventory, &companion_id),
            Err(BioRuntimeError::Inventory(
                InventoryEffectError::MissingItem { .. }
            ))
        ));
        assert_eq!(profile, before);

        inventory = inventory_with(&[(GEAR_ITEM_ID, 8), (ENERGY_CORE_ITEM_ID, 1)]);
        try_upgrade_companion(&mut profile, &mut inventory, &companion_id).unwrap();
        assert_eq!(profile.companions.companions[&companion_id].level, 2);
        assert_eq!(inventory.count(GEAR_ITEM_ID), 0);
        assert_eq!(inventory.count(ENERGY_CORE_ITEM_ID), 0);

        let downed =
            apply_companion_damage(&mut profile.companions, &companion_id, 10_000.0).unwrap();
        assert_eq!(downed.life, CompanionLifeState::Downed);
        let revived = revive_companion(&mut profile.companions, &companion_id).unwrap();
        assert_eq!(revived.life, CompanionLifeState::Active);
        assert_eq!(revived.revive_count, 1);
        assert!(revived.current_health > 0.0);
    }

    #[test]
    fn rescue_reward_expands_a_full_roster_and_is_idempotent() {
        let mut profile = PlayerBioRecord::default();
        for preset in ["MegaUnitX", "GuardianUnit", "MedicDrone"] {
            profile.companions.add_companion(preset, false).unwrap();
        }
        assert_eq!(profile.companions.capacity, 3);
        let update = RescueUpdate {
            rewards: vec![RescueReward::CompanionPreset("MiniGeneralVoidcrown")],
            ..Default::default()
        };

        let first = apply_rescue_rewards(&mut profile, &update).unwrap();
        assert_eq!(first.granted_companion_ids.len(), 1);
        assert_eq!(profile.companions.capacity, 4);
        assert_eq!(profile.companions.companions.len(), 4);

        let second = apply_rescue_rewards(&mut profile, &update).unwrap();
        assert!(second.granted_companion_ids.is_empty());
        assert_eq!(second.already_owned_presets, ["MiniGeneralVoidcrown"]);
        assert_eq!(profile.companions.companions.len(), 4);
    }
}
