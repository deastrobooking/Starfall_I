//! Engine-neutral continuity rules for Heavy Water's optional combat systems.
//!
//! Audited executable sources:
//! `MeleeArsenalSystem.ts`, `ElementalSpecialsSystem.ts`,
//! `ArmorCapsuleSystem.ts`, `ArmorSystem.ts`, `WeaponsSystem.ts`,
//! `PlayerController.ts`, `ProgressSync.ts`, and the purchase/load paths in
//! `Game.tsx` from `/Users/home/Desktop/Games/Heavy_Water`.
//!
//! This module owns durable unlocks and exact balance data, but deliberately
//! does not own hit detection, health, projectiles, equipped armor, ammo, UI,
//! input, or rendering. Actions return deterministic descriptors for
//! Starfall's canonical collision/damage pipeline to execute.
#![allow(dead_code)] // Public port domain; runtime adapters land incrementally.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

pub const HEAVY_COMBAT_SCHEMA_VERSION: u16 = 1;
pub const MAX_ELEMENTAL_LEVEL: u8 = 20;
pub const MAX_PRIMARY_WEAPON_LEVEL: u8 = 5;

pub const GEAR_ITEM_ID: &str = "gear";
pub const ENERGY_CORE_ITEM_ID: &str = "energy_core";
pub const NANO_FIBER_ITEM_ID: &str = "nano_fiber";
pub const CIRCUIT_BOARD_ITEM_ID: &str = "circuit_board";

/// Explicit record of the systems intentionally left native to Starfall.
pub const NATIVE_SUPERSEDED_BEHAVIORS: &[&str] = &[
    "target discovery, collision tests, damage routing, resistances, and health mutation",
    "projectile entities, homing integration, explosions, ammo, reload, and weapon selection",
    "equipped armor pieces, armor durability, mitigation, elemental armor effects, and modules",
    "player-level, perk, jewel, vehicle, and authored encounter multipliers",
    "input bindings, cooldown presentation, animation, meshes, lights, audio, and UI",
];

// ---------------------------------------------------------------------------
// Canonical-neutral payment snapshots.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCost {
    pub item_id: String,
    pub quantity: u32,
}

impl ItemCost {
    fn new(item_id: &str, quantity: u32) -> Self {
        Self {
            item_id: item_id.to_owned(),
            quantity,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatCost {
    pub credits: u64,
    pub items: Vec<ItemCost>,
}

impl CombatCost {
    fn from_parts(credits: u64, items: &[(&str, u32)]) -> Self {
        Self {
            credits,
            items: items
                .iter()
                .filter(|(_, quantity)| *quantity != 0)
                .map(|(item_id, quantity)| ItemCost::new(item_id, *quantity))
                .collect(),
        }
    }

    pub fn item_quantity(&self, item_id: &str) -> u32 {
        self.items
            .iter()
            .filter(|cost| cost.item_id == item_id)
            .map(|cost| cost.quantity)
            .sum()
    }
}

/// Ephemeral view of the caller's canonical wallet and material counts.
///
/// This type is intentionally absent from [`HeavyCombatSave`]. A runtime
/// adapter should stage it from Starfall's live `PlayerStats`/`Inventory`, run
/// an atomic domain purchase, then commit the returned cost to those canonical
/// components. It must never be serialized as a second inventory authority.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CombatBalances {
    pub credits: u64,
    pub items: BTreeMap<String, u32>,
}

impl CombatBalances {
    pub fn new<I, S>(credits: u64, items: I) -> Self
    where
        I: IntoIterator<Item = (S, u32)>,
        S: Into<String>,
    {
        let mut balances = Self {
            credits,
            ..Default::default()
        };
        for (item_id, quantity) in items {
            let item_id = item_id.into();
            let next = balances
                .items
                .get(&item_id)
                .copied()
                .unwrap_or_default()
                .saturating_add(quantity);
            balances.items.insert(item_id, next);
        }
        balances
    }

    pub fn item_count(&self, item_id: &str) -> u32 {
        self.items.get(item_id).copied().unwrap_or_default()
    }

    fn spend(&mut self, cost: &CombatCost) -> Result<(), CombatPurchaseError> {
        if self.credits < cost.credits {
            return Err(CombatPurchaseError::InsufficientCredits {
                needed: cost.credits,
                available: self.credits,
            });
        }

        let mut requested = BTreeMap::<&str, u32>::new();
        for item in &cost.items {
            let total = requested
                .get(item.item_id.as_str())
                .copied()
                .unwrap_or_default()
                .checked_add(item.quantity)
                .ok_or_else(|| CombatPurchaseError::CostOverflow(item.item_id.clone()))?;
            requested.insert(item.item_id.as_str(), total);
        }
        for (item_id, needed) in &requested {
            let available = self.item_count(item_id);
            if available < *needed {
                return Err(CombatPurchaseError::InsufficientItem {
                    item_id: (*item_id).to_owned(),
                    needed: *needed,
                    available,
                });
            }
        }

        self.credits -= cost.credits;
        for (item_id, quantity) in requested {
            let remaining = self.item_count(item_id) - quantity;
            if remaining == 0 {
                self.items.remove(item_id);
            } else {
                self.items.insert(item_id.to_owned(), remaining);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CombatPurchaseError {
    EmptyOwnerKey,
    AlreadyUnlocked,
    MaxLevel,
    ToolNotUpgradeable,
    InsufficientCredits {
        needed: u64,
        available: u64,
    },
    InsufficientItem {
        item_id: String,
        needed: u32,
        available: u32,
    },
    CostOverflow(String),
}

// ---------------------------------------------------------------------------
// Owner-keyed save state.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeavyCombatSave {
    pub schema_version: u16,
    pub players: BTreeMap<String, HeavyCombatPlayerRecord>,
}

impl Default for HeavyCombatSave {
    fn default() -> Self {
        Self {
            schema_version: HEAVY_COMBAT_SCHEMA_VERSION,
            players: BTreeMap::new(),
        }
    }
}

impl HeavyCombatSave {
    pub fn player(&self, owner_key: &str) -> Option<&HeavyCombatPlayerRecord> {
        self.players.get(owner_key)
    }

    pub fn player_mut(&mut self, owner_key: impl Into<String>) -> &mut HeavyCombatPlayerRecord {
        self.players.entry(owner_key.into()).or_default()
    }

    pub fn normalize(&mut self) {
        self.schema_version = HEAVY_COMBAT_SCHEMA_VERSION;
        self.players.retain(|key, _| !key.trim().is_empty());
        for player in self.players.values_mut() {
            player.normalize();
        }
    }

    pub fn purchase_melee_unlock(
        &mut self,
        owner_key: &str,
        balances: &mut CombatBalances,
        weapon: MeleeArsenalId,
        tier: MeleeUnlockTier,
    ) -> Result<MeleeUnlockEffect, CombatPurchaseError> {
        validate_owner_key(owner_key)?;
        let mut staged_player = self.player(owner_key).cloned().unwrap_or_default();
        let state = staged_player.melee.weapon(weapon);
        if state.has_tier(tier) {
            return Err(CombatPurchaseError::AlreadyUnlocked);
        }
        let cost = melee_unlock_cost(weapon, tier);
        let mut staged_balances = balances.clone();
        staged_balances.spend(&cost)?;
        let resulting_state = staged_player.melee.unlock(weapon, tier);

        self.players.insert(owner_key.to_owned(), staged_player);
        *balances = staged_balances;
        Ok(MeleeUnlockEffect {
            weapon,
            tier,
            cost,
            resulting_state,
        })
    }

    pub fn purchase_elemental_level(
        &mut self,
        owner_key: &str,
        balances: &mut CombatBalances,
        kind: ElementalKind,
    ) -> Result<ElementalUpgradeEffect, CombatPurchaseError> {
        validate_owner_key(owner_key)?;
        let mut staged_player = self.player(owner_key).cloned().unwrap_or_default();
        let previous_level = staged_player.elementals.level(kind);
        if previous_level >= MAX_ELEMENTAL_LEVEL {
            return Err(CombatPurchaseError::MaxLevel);
        }
        let cost = CombatCost::from_parts(elemental_upgrade_cost(previous_level), &[]);
        let mut staged_balances = balances.clone();
        staged_balances.spend(&cost)?;
        let new_level = previous_level + 1;
        staged_player.elementals.set_level(kind, new_level);

        self.players.insert(owner_key.to_owned(), staged_player);
        *balances = staged_balances;
        Ok(ElementalUpgradeEffect {
            kind,
            previous_level,
            new_level,
            cost,
            stats: elemental_stats(kind, new_level),
        })
    }

    pub fn purchase_primary_weapon_level(
        &mut self,
        owner_key: &str,
        balances: &mut CombatBalances,
        weapon: PrimaryWeaponKind,
    ) -> Result<PrimaryWeaponUpgradeEffect, CombatPurchaseError> {
        validate_owner_key(owner_key)?;
        if !weapon.is_upgradeable() {
            return Err(CombatPurchaseError::ToolNotUpgradeable);
        }
        let mut staged_player = self.player(owner_key).cloned().unwrap_or_default();
        let previous_level = staged_player.primary_weapons.level(weapon);
        if previous_level >= MAX_PRIMARY_WEAPON_LEVEL {
            return Err(CombatPurchaseError::MaxLevel);
        }
        let new_level = previous_level + 1;
        let cost = primary_weapon_upgrade_cost(weapon, new_level);
        let mut staged_balances = balances.clone();
        staged_balances.spend(&cost)?;
        staged_player.primary_weapons.set_level(weapon, new_level);

        self.players.insert(owner_key.to_owned(), staged_player);
        *balances = staged_balances;
        Ok(PrimaryWeaponUpgradeEffect {
            weapon,
            previous_level,
            new_level,
            cost,
            stats: primary_weapon_stats(weapon, new_level),
        })
    }

    pub fn purchase_capsule_upgrade(
        &mut self,
        owner_key: &str,
        balances: &mut CombatBalances,
        upgrade: ArmorCapsuleUpgradeId,
    ) -> Result<ArmorCapsuleUnlockEffect, CombatPurchaseError> {
        validate_owner_key(owner_key)?;
        let mut staged_player = self.player(owner_key).cloned().unwrap_or_default();
        if staged_player.armor_capsule.is_applied(upgrade) {
            return Err(CombatPurchaseError::AlreadyUnlocked);
        }
        let definition = armor_capsule_upgrade(upgrade);
        let cost = CombatCost::from_parts(u64::from(definition.cost_credits), &[]);
        let mut staged_balances = balances.clone();
        staged_balances.spend(&cost)?;
        staged_player.armor_capsule.mark_applied(upgrade);

        self.players.insert(owner_key.to_owned(), staged_player);
        *balances = staged_balances;
        Ok(ArmorCapsuleUnlockEffect {
            upgrade,
            cost,
            effects: definition.effects,
        })
    }
}

fn validate_owner_key(owner_key: &str) -> Result<(), CombatPurchaseError> {
    if owner_key.trim().is_empty() {
        Err(CombatPurchaseError::EmptyOwnerKey)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeavyCombatPlayerRecord {
    pub melee: MeleeArsenalRecord,
    pub elementals: ElementalLevelsRecord,
    pub primary_weapons: PrimaryWeaponLevelsRecord,
    pub armor_capsule: ArmorCapsuleRecord,
}

impl HeavyCombatPlayerRecord {
    fn normalize(&mut self) {
        self.melee.normalize();
        self.elementals.normalize();
        self.primary_weapons.normalize();
        self.armor_capsule.normalize();
    }
}

// ---------------------------------------------------------------------------
// Alternate melee arsenal.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeArsenalId {
    Glaive,
    Daggers,
    Axe,
    Whip,
}

impl MeleeArsenalId {
    pub const ALL: [Self; 4] = [Self::Glaive, Self::Daggers, Self::Axe, Self::Whip];
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeWeaponDef {
    pub id: MeleeArsenalId,
    pub stable_id: &'static str,
    pub name: &'static str,
    pub primary_name: &'static str,
    pub combo_name: &'static str,
    pub special_name: &'static str,
    pub base_damage: f64,
    /// Authored visual reach. Heavy Water collision used `primary_radius`.
    pub visual_reach: f64,
    pub full_cone_radians: f64,
    pub primary_radius: f64,
    pub base_cooldown_ms: u32,
    pub knockback: f64,
}

pub const MELEE_WEAPONS: [MeleeWeaponDef; 4] = [
    MeleeWeaponDef {
        id: MeleeArsenalId::Glaive,
        stable_id: "glaive",
        name: "Beam Glaive",
        primary_name: "Sweep",
        combo_name: "Triple Sweep",
        special_name: "Comet Spin",
        base_damage: 200.0,
        visual_reach: 4.6,
        full_cone_radians: std::f64::consts::PI * 0.78,
        primary_radius: 8.0,
        base_cooldown_ms: 550,
        knockback: 9.0,
    },
    MeleeWeaponDef {
        id: MeleeArsenalId::Daggers,
        stable_id: "daggers",
        name: "Twin Beam Daggers",
        primary_name: "Rapid Stab",
        combo_name: "Phantom Step",
        special_name: "Phantom Storm",
        base_damage: 95.0,
        visual_reach: 3.4,
        full_cone_radians: std::f64::consts::PI * 0.34,
        primary_radius: 4.5,
        base_cooldown_ms: 180,
        knockback: 4.0,
    },
    MeleeWeaponDef {
        id: MeleeArsenalId::Axe,
        stable_id: "axe",
        name: "Plasma War Axe",
        primary_name: "Heavy Cleave",
        combo_name: "Cleave + Upper",
        special_name: "Ground Slam",
        base_damage: 380.0,
        visual_reach: 4.2,
        full_cone_radians: std::f64::consts::PI * 0.5,
        primary_radius: 6.5,
        base_cooldown_ms: 950,
        knockback: 18.0,
    },
    MeleeWeaponDef {
        id: MeleeArsenalId::Whip,
        stable_id: "whip",
        name: "Spiked Chain Whip",
        primary_name: "Long Lash",
        combo_name: "Pull-In Lash",
        special_name: "Flail Spin",
        base_damage: 140.0,
        visual_reach: 7.5,
        full_cone_radians: std::f64::consts::PI * 0.18,
        primary_radius: 16.0,
        base_cooldown_ms: 500,
        knockback: 7.0,
    },
];

pub fn melee_weapon(id: MeleeArsenalId) -> &'static MeleeWeaponDef {
    &MELEE_WEAPONS[id as usize]
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MeleeWeaponUnlockRecord {
    pub unlocked: bool,
    pub combo_unlocked: bool,
    pub special_unlocked: bool,
}

impl MeleeWeaponUnlockRecord {
    pub const fn has_tier(self, tier: MeleeUnlockTier) -> bool {
        match tier {
            MeleeUnlockTier::Own => self.unlocked,
            MeleeUnlockTier::Combo => self.combo_unlocked,
            MeleeUnlockTier::Special => self.special_unlocked,
        }
    }

    pub const fn weapon_level(self) -> u8 {
        1 + self.combo_unlocked as u8 + self.special_unlocked as u8
    }

    pub const fn damage_multiplier(self) -> f64 {
        1.0 + 0.40 * (self.weapon_level() as f64 - 1.0)
    }

    pub const fn reach_multiplier(self) -> f64 {
        1.0 + 0.10 * (self.weapon_level() as f64 - 1.0)
    }

    fn normalize(&mut self) {
        if self.combo_unlocked || self.special_unlocked {
            self.unlocked = true;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeleeArsenalRecord {
    pub glaive: MeleeWeaponUnlockRecord,
    pub daggers: MeleeWeaponUnlockRecord,
    pub axe: MeleeWeaponUnlockRecord,
    pub whip: MeleeWeaponUnlockRecord,
}

impl MeleeArsenalRecord {
    pub const fn weapon(&self, id: MeleeArsenalId) -> MeleeWeaponUnlockRecord {
        match id {
            MeleeArsenalId::Glaive => self.glaive,
            MeleeArsenalId::Daggers => self.daggers,
            MeleeArsenalId::Axe => self.axe,
            MeleeArsenalId::Whip => self.whip,
        }
    }

    pub fn unlock(&mut self, id: MeleeArsenalId, tier: MeleeUnlockTier) -> MeleeWeaponUnlockRecord {
        let state = match id {
            MeleeArsenalId::Glaive => &mut self.glaive,
            MeleeArsenalId::Daggers => &mut self.daggers,
            MeleeArsenalId::Axe => &mut self.axe,
            MeleeArsenalId::Whip => &mut self.whip,
        };
        match tier {
            MeleeUnlockTier::Own => state.unlocked = true,
            MeleeUnlockTier::Combo => {
                state.unlocked = true;
                state.combo_unlocked = true;
            }
            MeleeUnlockTier::Special => {
                state.unlocked = true;
                state.special_unlocked = true;
            }
        }
        *state
    }

    fn normalize(&mut self) {
        for id in MeleeArsenalId::ALL {
            match id {
                MeleeArsenalId::Glaive => self.glaive.normalize(),
                MeleeArsenalId::Daggers => self.daggers.normalize(),
                MeleeArsenalId::Axe => self.axe.normalize(),
                MeleeArsenalId::Whip => self.whip.normalize(),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeUnlockTier {
    Own,
    Combo,
    Special,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeleeUnlockEffect {
    pub weapon: MeleeArsenalId,
    pub tier: MeleeUnlockTier,
    pub cost: CombatCost,
    pub resulting_state: MeleeWeaponUnlockRecord,
}

pub fn melee_unlock_cost(weapon: MeleeArsenalId, tier: MeleeUnlockTier) -> CombatCost {
    let (gears, cores, nano, circuits) = match (weapon, tier) {
        (MeleeArsenalId::Glaive, MeleeUnlockTier::Own) => (60, 10, 6, 0),
        (MeleeArsenalId::Glaive, MeleeUnlockTier::Combo) => (110, 20, 14, 0),
        (MeleeArsenalId::Glaive, MeleeUnlockTier::Special) => (160, 30, 22, 10),
        (MeleeArsenalId::Daggers, MeleeUnlockTier::Own) => (70, 12, 8, 0),
        (MeleeArsenalId::Daggers, MeleeUnlockTier::Combo) => (120, 22, 16, 0),
        (MeleeArsenalId::Daggers, MeleeUnlockTier::Special) => (170, 32, 24, 10),
        (MeleeArsenalId::Axe, MeleeUnlockTier::Own) => (90, 16, 10, 0),
        (MeleeArsenalId::Axe, MeleeUnlockTier::Combo) => (140, 26, 18, 8),
        (MeleeArsenalId::Axe, MeleeUnlockTier::Special) => (200, 38, 28, 14),
        (MeleeArsenalId::Whip, MeleeUnlockTier::Own) => (80, 14, 9, 0),
        (MeleeArsenalId::Whip, MeleeUnlockTier::Combo) => (130, 24, 17, 8),
        (MeleeArsenalId::Whip, MeleeUnlockTier::Special) => (190, 36, 26, 12),
    };
    CombatCost::from_parts(
        0,
        &[
            (GEAR_ITEM_ID, gears),
            (ENERGY_CORE_ITEM_ID, cores),
            (NANO_FIBER_ITEM_ID, nano),
            (CIRCUIT_BOARD_ITEM_ID, circuits),
        ],
    )
}

/// Heavy Water's common forward-hit policy. Starfall should feed these values
/// into its own target query instead of accepting target identities here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ForwardConePolicy {
    pub radius: f64,
    /// Full authored cone angle; collision compares against half of it.
    pub full_angle_radians: f64,
    pub point_blank_radius: f64,
    pub point_blank_min_cosine: f64,
    pub default_target_radius: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimedMeleeHit {
    pub at_ms: u32,
    pub damage: f64,
    pub knockback: f64,
    pub critical_hint: bool,
    pub cone: ForwardConePolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "pattern", rename_all = "snake_case")]
pub enum MeleePrimaryPattern {
    ForwardCombo {
        hits: Vec<TimedMeleeHit>,
        /// The executable only emitted an afterimage at this offset; it did
        /// not authoritatively move the player despite its old comment.
        phantom_step_cue_distance: Option<f64>,
        pull_closest_target_distance: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeleePrimaryAction {
    pub weapon: MeleeArsenalId,
    pub weapon_level: u8,
    pub pattern: MeleePrimaryPattern,
    /// Cooldown does not begin until the source chain callback completes.
    pub cooldown_begins_at_ms: u32,
    pub cooldown_ms: u32,
    pub busy_until_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "pattern", rename_all = "snake_case")]
pub enum MeleeSpecialPattern {
    OrbitRing {
        orbit_radius: f64,
        collision_tolerance: f64,
        duration_ms: u32,
        repeat_hit_interval_ms: u32,
        damage: f64,
        knockback: f64,
    },
    NearestTargetSequence {
        range: f64,
        strikes: Vec<TimedTargetStrike>,
    },
    ExpandingGroundRing {
        start_radius: f64,
        expansion_distance: f64,
        duration_ms: u32,
        collision_half_width: f64,
        vertical_half_band: f64,
        default_target_radius: f64,
        damage: f64,
        knockback: f64,
        one_hit_per_target: bool,
    },
    RadialTicks {
        radius: f64,
        visual_duration_ms: u32,
        ticks: Vec<TimedRadialHit>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimedTargetStrike {
    pub target_rank: u8,
    pub at_ms: u32,
    pub damage: f64,
    pub knockback: f64,
    pub critical_hint: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimedRadialHit {
    pub at_ms: u32,
    pub damage: f64,
    pub knockback: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeleeSpecialAction {
    pub weapon: MeleeArsenalId,
    pub weapon_level: u8,
    pub pattern: MeleeSpecialPattern,
    pub cooldown_begins_at_ms: u32,
    pub cooldown_ms: u32,
    pub busy_until_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeleeActionError {
    WeaponLocked,
    SpecialLocked,
    NoTarget,
}

pub fn melee_primary_action(
    weapon: MeleeArsenalId,
    unlocks: MeleeWeaponUnlockRecord,
) -> Result<MeleePrimaryAction, MeleeActionError> {
    if !unlocks.unlocked {
        return Err(MeleeActionError::WeaponLocked);
    }
    let definition = melee_weapon(weapon);
    let damage_mul = unlocks.damage_multiplier();
    let reach_mul = unlocks.reach_multiplier();
    let cone = |radius_mul: f64| ForwardConePolicy {
        radius: definition.primary_radius * reach_mul * radius_mul,
        full_angle_radians: definition.full_cone_radians,
        point_blank_radius: 2.0,
        point_blank_min_cosine: -0.2,
        default_target_radius: 1.5,
    };

    let (hits, cue, pull, cooldown_begins_at_ms, cooldown_ms, busy_until_ms) = match weapon {
        MeleeArsenalId::Glaive => {
            let count = if unlocks.combo_unlocked { 3 } else { 1 };
            let hits = (0..count)
                .map(|index| TimedMeleeHit {
                    at_ms: index * 130,
                    damage: definition.base_damage * damage_mul,
                    knockback: definition.knockback,
                    critical_hint: false,
                    cone: cone(1.0),
                })
                .collect();
            (
                hits,
                None,
                None,
                count * 130,
                definition.base_cooldown_ms,
                count * 130,
            )
        }
        MeleeArsenalId::Daggers => {
            let count = if unlocks.combo_unlocked { 6 } else { 4 };
            let hits = (0..count)
                .map(|index| TimedMeleeHit {
                    at_ms: index * 70,
                    damage: definition.base_damage * damage_mul * 0.55,
                    knockback: definition.knockback,
                    critical_hint: false,
                    cone: cone(1.0),
                })
                .collect();
            (
                hits,
                unlocks.combo_unlocked.then_some(2.5),
                None,
                count * 70,
                288,
                count * 70,
            )
        }
        MeleeArsenalId::Axe => {
            let mut hits = vec![TimedMeleeHit {
                at_ms: 0,
                damage: definition.base_damage * damage_mul,
                knockback: definition.knockback,
                critical_hint: true,
                cone: cone(1.0),
            }];
            if unlocks.combo_unlocked {
                hits.push(TimedMeleeHit {
                    at_ms: 280,
                    damage: definition.base_damage * damage_mul * 0.85,
                    knockback: definition.knockback,
                    critical_hint: true,
                    cone: cone(1.05),
                });
            }
            let chain_end = if unlocks.combo_unlocked { 280 } else { 320 };
            (
                hits,
                None,
                None,
                chain_end,
                definition.base_cooldown_ms,
                chain_end,
            )
        }
        MeleeArsenalId::Whip => (
            vec![TimedMeleeHit {
                at_ms: 0,
                damage: definition.base_damage * damage_mul,
                knockback: definition.knockback,
                critical_hint: false,
                cone: cone(1.0),
            }],
            None,
            unlocks.combo_unlocked.then_some(3.0),
            380,
            definition.base_cooldown_ms,
            380,
        ),
    };

    Ok(MeleePrimaryAction {
        weapon,
        weapon_level: unlocks.weapon_level(),
        pattern: MeleePrimaryPattern::ForwardCombo {
            hits,
            phantom_step_cue_distance: cue,
            pull_closest_target_distance: pull,
        },
        cooldown_begins_at_ms,
        cooldown_ms,
        busy_until_ms,
    })
}

pub fn melee_special_action(
    weapon: MeleeArsenalId,
    unlocks: MeleeWeaponUnlockRecord,
    available_targets: usize,
) -> Result<MeleeSpecialAction, MeleeActionError> {
    if !unlocks.unlocked {
        return Err(MeleeActionError::WeaponLocked);
    }
    if !unlocks.special_unlocked {
        return Err(MeleeActionError::SpecialLocked);
    }
    let definition = melee_weapon(weapon);
    let damage_mul = unlocks.damage_multiplier();
    let reach_mul = unlocks.reach_multiplier();

    let (pattern, cooldown_begins_at_ms, cooldown_ms, busy_until_ms) = match weapon {
        MeleeArsenalId::Glaive => (
            MeleeSpecialPattern::OrbitRing {
                orbit_radius: 6.0,
                collision_tolerance: 1.5,
                duration_ms: 1_400,
                repeat_hit_interval_ms: 220,
                damage: definition.base_damage * damage_mul * 0.7,
                knockback: definition.knockback,
            },
            0,
            4_000,
            0,
        ),
        MeleeArsenalId::Daggers => {
            let count = available_targets.min(3);
            if count == 0 {
                return Err(MeleeActionError::NoTarget);
            }
            let strikes = (0..count)
                .map(|index| TimedTargetStrike {
                    target_rank: index as u8,
                    at_ms: index as u32 * 140,
                    damage: definition.base_damage * damage_mul * 1.8,
                    knockback: definition.knockback,
                    critical_hint: true,
                })
                .collect();
            let chain_end = count as u32 * 140;
            (
                MeleeSpecialPattern::NearestTargetSequence {
                    range: 18.0,
                    strikes,
                },
                chain_end,
                3_500,
                chain_end,
            )
        }
        MeleeArsenalId::Axe => (
            MeleeSpecialPattern::ExpandingGroundRing {
                start_radius: 1.0,
                expansion_distance: 14.0,
                duration_ms: 850,
                collision_half_width: 1.5,
                vertical_half_band: 3.0,
                default_target_radius: 1.5,
                damage: definition.base_damage * damage_mul * 1.4,
                knockback: definition.knockback * 1.5,
                one_hit_per_target: true,
            },
            0,
            4_500,
            600,
        ),
        MeleeArsenalId::Whip => {
            let ticks = (0..4)
                .map(|index| TimedRadialHit {
                    at_ms: index * 220,
                    damage: definition.base_damage * damage_mul * 0.6,
                    knockback: definition.knockback,
                })
                .collect();
            (
                MeleeSpecialPattern::RadialTicks {
                    radius: 9.0 * reach_mul,
                    visual_duration_ms: 1_000,
                    ticks,
                },
                0,
                4_000,
                660,
            )
        }
    };

    Ok(MeleeSpecialAction {
        weapon,
        weapon_level: unlocks.weapon_level(),
        pattern,
        cooldown_begins_at_ms,
        cooldown_ms,
        busy_until_ms,
    })
}

// ---------------------------------------------------------------------------
// Elemental active specials.

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ElementalKind {
    #[default]
    Lightning,
    Ice,
    Fireball,
    Inferno,
    Windstorm,
    Psychic,
}

impl ElementalKind {
    pub const ALL: [Self; 6] = [
        Self::Lightning,
        Self::Ice,
        Self::Fireball,
        Self::Inferno,
        Self::Windstorm,
        Self::Psychic,
    ];

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementalCategory {
    Tracking,
    Dome,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementalDef {
    pub kind: ElementalKind,
    pub stable_id: &'static str,
    pub name: &'static str,
    pub category: ElementalCategory,
    /// Original keyboard hint; input ownership remains native.
    pub source_key: &'static str,
    pub base_cooldown_ms: u32,
    pub base_damage: u32,
    pub base_radius: f64,
    pub base_targets: u16,
}

pub const ELEMENTAL_SPECIALS: [ElementalDef; 6] = [
    ElementalDef {
        kind: ElementalKind::Lightning,
        stable_id: "lightning",
        name: "Lightning Strike",
        category: ElementalCategory::Tracking,
        source_key: "KeyZ",
        base_cooldown_ms: 4_000,
        base_damage: 90,
        base_radius: 4.0,
        base_targets: 2,
    },
    ElementalDef {
        kind: ElementalKind::Ice,
        stable_id: "ice",
        name: "Ice Strike",
        category: ElementalCategory::Tracking,
        source_key: "KeyI",
        base_cooldown_ms: 4_500,
        base_damage: 75,
        base_radius: 4.0,
        base_targets: 2,
    },
    ElementalDef {
        kind: ElementalKind::Fireball,
        stable_id: "fireball",
        name: "Fireball",
        category: ElementalCategory::Tracking,
        source_key: "KeyN",
        base_cooldown_ms: 3_500,
        base_damage: 110,
        base_radius: 5.0,
        base_targets: 2,
    },
    ElementalDef {
        kind: ElementalKind::Inferno,
        stable_id: "inferno",
        name: "Flame Inferno",
        category: ElementalCategory::Dome,
        source_key: "KeyU",
        base_cooldown_ms: 8_000,
        base_damage: 140,
        base_radius: 12.0,
        base_targets: 999,
    },
    ElementalDef {
        kind: ElementalKind::Windstorm,
        stable_id: "windstorm",
        name: "Windstorm",
        category: ElementalCategory::Dome,
        source_key: "KeyT",
        base_cooldown_ms: 7_000,
        base_damage: 95,
        base_radius: 14.0,
        base_targets: 999,
    },
    ElementalDef {
        kind: ElementalKind::Psychic,
        stable_id: "psychic",
        name: "Psychic Shockwave",
        category: ElementalCategory::Dome,
        source_key: "KeyM",
        base_cooldown_ms: 9_000,
        base_damage: 165,
        base_radius: 16.0,
        base_targets: 999,
    },
];

pub fn elemental_special(kind: ElementalKind) -> &'static ElementalDef {
    &ELEMENTAL_SPECIALS[kind.index()]
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ElementalStats {
    pub kind: ElementalKind,
    pub level: u8,
    pub damage_per_hit: u32,
    pub radius: f64,
    pub max_targets: u16,
    pub projectiles_per_target: u8,
    /// Quantized to one microsecond for deterministic runtime countdowns.
    pub cooldown_micros: u64,
}

impl ElementalStats {
    pub fn cooldown_ms(self) -> f64 {
        self.cooldown_micros as f64 / 1_000.0
    }
}

pub fn elemental_upgrade_cost(current_level: u8) -> u64 {
    let level = current_level.clamp(1, MAX_ELEMENTAL_LEVEL);
    // floor(300 * (1 + level * 0.5)); every legal integer level is exact.
    300 + 150 * u64::from(level)
}

pub fn elemental_stats(kind: ElementalKind, level: u8) -> ElementalStats {
    let level = level.clamp(1, MAX_ELEMENTAL_LEVEL);
    let definition = elemental_special(kind);
    let tier = f64::from(level - 1);
    let damage_per_hit = (f64::from(definition.base_damage) * (1.0 + 0.35 * tier)).round() as u32;
    let radius_per_level = match definition.category {
        ElementalCategory::Tracking => 0.14,
        ElementalCategory::Dome => 0.20,
    };
    let radius = definition.base_radius * (1.0 + radius_per_level * tier);
    let (max_targets, projectiles_per_target) = match definition.category {
        ElementalCategory::Dome => (999, 0),
        ElementalCategory::Tracking => {
            let span = 10.0 - f64::from(definition.base_targets);
            let targets = (f64::from(definition.base_targets)
                + span * tier / f64::from(MAX_ELEMENTAL_LEVEL - 1))
            .round()
            .min(10.0) as u16;
            let projectiles = (1 + (level - 1) / 2).min(8);
            (targets, projectiles)
        }
    };
    let cooldown_ms =
        (f64::from(definition.base_cooldown_ms) * 0.92_f64.powi(i32::from(level - 1))).max(800.0);
    ElementalStats {
        kind,
        level,
        damage_per_hit,
        radius,
        max_targets,
        projectiles_per_target,
        cooldown_micros: (cooldown_ms * 1_000.0).round() as u64,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ElementalLevelsRecord {
    pub lightning: u8,
    pub ice: u8,
    pub fireball: u8,
    pub inferno: u8,
    pub windstorm: u8,
    pub psychic: u8,
}

impl Default for ElementalLevelsRecord {
    fn default() -> Self {
        Self {
            lightning: 1,
            ice: 1,
            fireball: 1,
            inferno: 1,
            windstorm: 1,
            psychic: 1,
        }
    }
}

impl ElementalLevelsRecord {
    pub const fn level(&self, kind: ElementalKind) -> u8 {
        match kind {
            ElementalKind::Lightning => self.lightning,
            ElementalKind::Ice => self.ice,
            ElementalKind::Fireball => self.fireball,
            ElementalKind::Inferno => self.inferno,
            ElementalKind::Windstorm => self.windstorm,
            ElementalKind::Psychic => self.psychic,
        }
    }

    pub fn set_level(&mut self, kind: ElementalKind, level: u8) {
        let level = level.clamp(1, MAX_ELEMENTAL_LEVEL);
        match kind {
            ElementalKind::Lightning => self.lightning = level,
            ElementalKind::Ice => self.ice = level,
            ElementalKind::Fireball => self.fireball = level,
            ElementalKind::Inferno => self.inferno = level,
            ElementalKind::Windstorm => self.windstorm = level,
            ElementalKind::Psychic => self.psychic = level,
        }
    }

    fn normalize(&mut self) {
        for kind in ElementalKind::ALL {
            let level = self.level(kind);
            self.set_level(kind, level);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElementalUpgradeEffect {
    pub kind: ElementalKind,
    pub previous_level: u8,
    pub new_level: u8,
    pub cost: CombatCost,
    pub stats: ElementalStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "pattern", rename_all = "snake_case")]
pub enum ElementalActionPattern {
    /// The executable replaced its older projectile swarm with one 220 m
    /// mega-beam. `damage` folds damage × target cap × volley size exactly as
    /// that replacement does, and each target may be hit once per cast.
    TrackingMegaBeam {
        length: f64,
        beam_radius: f64,
        damage: u32,
        lifetime_ms: u32,
        damage_scan_interval_ms: u32,
        one_hit_per_target: bool,
    },
    PlayerCenteredDome {
        radius: f64,
        damage: u32,
        damage_ticks: u8,
        tick_interval_ms: u32,
        lifetime_ms: u32,
        first_hit_multiplier: f64,
        repeated_hit_multiplier: f64,
        follows_owner: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementalCastEffect {
    pub kind: ElementalKind,
    pub stats: ElementalStats,
    pub pattern: ElementalActionPattern,
}

pub fn elemental_cast_effect(kind: ElementalKind, level: u8) -> ElementalCastEffect {
    let stats = elemental_stats(kind, level);
    let pattern = match elemental_special(kind).category {
        ElementalCategory::Tracking => {
            let damage = stats
                .damage_per_hit
                .saturating_mul(u32::from(stats.max_targets))
                .saturating_mul(u32::from(stats.projectiles_per_target.max(1)));
            ElementalActionPattern::TrackingMegaBeam {
                length: 220.0,
                beam_radius: stats.radius.max(5.0),
                damage,
                lifetime_ms: 1_400,
                damage_scan_interval_ms: 80,
                one_hit_per_target: true,
            }
        }
        ElementalCategory::Dome => {
            let (damage_ticks, tick_interval_ms, lifetime_ms) = match kind {
                ElementalKind::Inferno => (3, 250, 1_100),
                ElementalKind::Windstorm => (4, 200, 1_100),
                ElementalKind::Psychic => (1, 250, 700),
                _ => unreachable!("tracking kinds were handled above"),
            };
            ElementalActionPattern::PlayerCenteredDome {
                radius: stats.radius,
                damage: stats.damage_per_hit,
                damage_ticks,
                tick_interval_ms,
                lifetime_ms,
                first_hit_multiplier: 1.0,
                repeated_hit_multiplier: 0.35,
                follows_owner: true,
            }
        }
    };
    ElementalCastEffect {
        kind,
        stats,
        pattern,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementalCastError {
    CoolingDown { remaining_micros: u64 },
}

/// Session-only elemental selection/cooldowns. Heavy Water persisted levels,
/// but intentionally started casts ready and selected Lightning after load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementalRuntimeState {
    pub selected: ElementalKind,
    cooldown_remaining_micros: [u64; 6],
    submicrosecond_nanos: u16,
}

impl Default for ElementalRuntimeState {
    fn default() -> Self {
        Self {
            selected: ElementalKind::Lightning,
            cooldown_remaining_micros: [0; 6],
            submicrosecond_nanos: 0,
        }
    }
}

impl ElementalRuntimeState {
    pub fn cooldown_remaining_micros(&self, kind: ElementalKind) -> u64 {
        self.cooldown_remaining_micros[kind.index()]
    }

    pub fn cycle(&mut self, direction: i32) -> ElementalKind {
        let count = ElementalKind::ALL.len() as i32;
        let next = (self.selected.index() as i32 + direction).rem_euclid(count) as usize;
        self.selected = ElementalKind::ALL[next];
        self.selected
    }

    pub fn advance(&mut self, delta: Duration) {
        let total_nanos = delta
            .as_nanos()
            .saturating_add(u128::from(self.submicrosecond_nanos));
        let whole_micros = (total_nanos / 1_000).min(u128::from(u64::MAX)) as u64;
        self.submicrosecond_nanos = (total_nanos % 1_000) as u16;
        for remaining in &mut self.cooldown_remaining_micros {
            *remaining = remaining.saturating_sub(whole_micros);
        }
    }

    pub fn try_cast(
        &mut self,
        levels: &ElementalLevelsRecord,
        kind: ElementalKind,
    ) -> Result<ElementalCastEffect, ElementalCastError> {
        let remaining = self.cooldown_remaining_micros(kind);
        if remaining != 0 {
            return Err(ElementalCastError::CoolingDown {
                remaining_micros: remaining,
            });
        }
        let effect = elemental_cast_effect(kind, levels.level(kind));
        self.cooldown_remaining_micros[kind.index()] = effect.stats.cooldown_micros;
        Ok(effect)
    }

    pub fn try_cast_selected(
        &mut self,
        levels: &ElementalLevelsRecord,
    ) -> Result<ElementalCastEffect, ElementalCastError> {
        self.try_cast(levels, self.selected)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeavyCombatRuntime {
    pub players: BTreeMap<String, ElementalRuntimeState>,
}

impl HeavyCombatRuntime {
    pub fn player_mut(&mut self, owner_key: impl Into<String>) -> &mut ElementalRuntimeState {
        self.players.entry(owner_key.into()).or_default()
    }

    pub fn advance(&mut self, delta: Duration) {
        for runtime in self.players.values_mut() {
            runtime.advance(delta);
        }
    }
}

// ---------------------------------------------------------------------------
// Heavy Water primary-weapon balance and upgrade identity.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimaryWeaponKind {
    Pistol,
    Rifle,
    Shotgun,
    Rocket,
    Laser,
    Grenade,
    TrackingMissile,
    CaptureNet,
}

impl PrimaryWeaponKind {
    pub const ALL: [Self; 8] = [
        Self::Pistol,
        Self::Rifle,
        Self::Shotgun,
        Self::Rocket,
        Self::Laser,
        Self::Grenade,
        Self::TrackingMissile,
        Self::CaptureNet,
    ];

    const fn index(self) -> usize {
        self as usize
    }

    pub const fn is_upgradeable(self) -> bool {
        !matches!(self, Self::CaptureNet)
    }

    pub const fn source_projectile_lifetime_ms(self) -> u32 {
        match self {
            Self::TrackingMissile => 6_000,
            Self::CaptureNet => 0,
            _ => 3_000,
        }
    }

    pub const fn source_auto_target_eligible(self) -> bool {
        !matches!(self, Self::Grenade | Self::CaptureNet)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimaryFireBehavior {
    Projectile,
    PelletBurst,
    ExplosiveProjectile,
    ArcingExplosive,
    HomingExplosive,
    CaptureTool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimarySourceGlobalRules {
    pub simulation_hz: f64,
    pub default_target_hit_radius: f64,
    pub explosive_linear_falloff: bool,
    pub tracking_homing_range: f64,
    pub tracking_steer_per_60hz_frame: f64,
    pub grenade_gravity_per_60hz_frame: f64,
    pub auto_target_half_angle_degrees: f64,
    pub auto_target_range: f64,
    pub auto_target_pull: f64,
    pub vehicle_fire_interval_multiplier: f64,
    pub vehicle_damage_multiplier: f64,
    pub vehicle_projectile_size_multiplier: f64,
    pub vehicle_explosion_radius_multiplier: f64,
    pub vehicle_projectile_speed_multiplier: f64,
    pub vehicle_projectile_lifetime_multiplier: f64,
}

pub const PRIMARY_SOURCE_GLOBAL_RULES: PrimarySourceGlobalRules = PrimarySourceGlobalRules {
    simulation_hz: 60.0,
    default_target_hit_radius: 1.5,
    explosive_linear_falloff: true,
    tracking_homing_range: 120.0,
    tracking_steer_per_60hz_frame: 0.18,
    grenade_gravity_per_60hz_frame: 0.01,
    auto_target_half_angle_degrees: 25.0,
    auto_target_range: 140.0,
    auto_target_pull: 0.55,
    vehicle_fire_interval_multiplier: 0.8,
    vehicle_damage_multiplier: 1.5,
    vehicle_projectile_size_multiplier: 1.5,
    vehicle_explosion_radius_multiplier: 1.5,
    vehicle_projectile_speed_multiplier: 2.5,
    vehicle_projectile_lifetime_multiplier: 2.0,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimaryWeaponDef {
    pub kind: PrimaryWeaponKind,
    pub stable_id: &'static str,
    pub name: &'static str,
    pub base_damage: f64,
    pub base_fire_interval_ms: f64,
    /// HUD capacity from Heavy Water. Its executable refilled this capacity on
    /// every shot, so Starfall's canonical ammo/reload logic supersedes it.
    pub source_max_ammo: u32,
    pub range: f64,
    /// Source units per 60 Hz frame, retained for migration/balance identity.
    pub projectile_speed_per_frame: f64,
    pub base_spread: f64,
    pub automatic: bool,
    pub base_explosion_radius: f64,
    pub behavior: PrimaryFireBehavior,
    pub projectiles_per_shot: u8,
    pub upgrade_part_item_id: &'static str,
}

pub const PRIMARY_WEAPONS: [PrimaryWeaponDef; 8] = [
    PrimaryWeaponDef {
        kind: PrimaryWeaponKind::Pistol,
        stable_id: "pistol",
        name: "Plasma Pistol",
        base_damage: 15.0,
        base_fire_interval_ms: 300.0,
        source_max_ammo: 50,
        range: 100.0,
        projectile_speed_per_frame: 2.0,
        base_spread: 0.02,
        automatic: false,
        base_explosion_radius: 0.0,
        behavior: PrimaryFireBehavior::Projectile,
        projectiles_per_shot: 1,
        upgrade_part_item_id: "weapon_part_pistol",
    },
    PrimaryWeaponDef {
        kind: PrimaryWeaponKind::Rifle,
        stable_id: "rifle",
        name: "Pulse Rifle",
        base_damage: 25.0,
        base_fire_interval_ms: 100.0,
        source_max_ammo: 120,
        range: 150.0,
        projectile_speed_per_frame: 3.0,
        base_spread: 0.03,
        automatic: true,
        base_explosion_radius: 0.0,
        behavior: PrimaryFireBehavior::Projectile,
        projectiles_per_shot: 1,
        upgrade_part_item_id: "weapon_part_rifle",
    },
    PrimaryWeaponDef {
        kind: PrimaryWeaponKind::Shotgun,
        stable_id: "shotgun",
        name: "Scatter Blaster",
        base_damage: 8.0,
        base_fire_interval_ms: 800.0,
        source_max_ammo: 24,
        range: 30.0,
        projectile_speed_per_frame: 2.5,
        base_spread: 0.15,
        automatic: false,
        base_explosion_radius: 0.0,
        behavior: PrimaryFireBehavior::PelletBurst,
        projectiles_per_shot: 8,
        upgrade_part_item_id: "weapon_part_shotgun",
    },
    PrimaryWeaponDef {
        kind: PrimaryWeaponKind::Rocket,
        stable_id: "rocket",
        name: "Nova Launcher",
        base_damage: 100.0,
        base_fire_interval_ms: 1_500.0,
        source_max_ammo: 8,
        range: 200.0,
        projectile_speed_per_frame: 1.0,
        base_spread: 0.0,
        automatic: false,
        base_explosion_radius: 5.0,
        behavior: PrimaryFireBehavior::ExplosiveProjectile,
        projectiles_per_shot: 1,
        upgrade_part_item_id: "weapon_part_rocket",
    },
    PrimaryWeaponDef {
        kind: PrimaryWeaponKind::Laser,
        stable_id: "laser",
        name: "Photon Beam",
        base_damage: 40.0,
        base_fire_interval_ms: 50.0,
        source_max_ammo: 200,
        range: 300.0,
        projectile_speed_per_frame: 10.0,
        base_spread: 0.0,
        automatic: true,
        base_explosion_radius: 0.0,
        behavior: PrimaryFireBehavior::Projectile,
        projectiles_per_shot: 1,
        upgrade_part_item_id: "weapon_part_laser",
    },
    PrimaryWeaponDef {
        kind: PrimaryWeaponKind::Grenade,
        stable_id: "grenade",
        name: "Fusion Grenades",
        base_damage: 80.0,
        base_fire_interval_ms: 1_000.0,
        source_max_ammo: 6,
        range: 50.0,
        projectile_speed_per_frame: 0.5,
        base_spread: 0.0,
        automatic: false,
        base_explosion_radius: 4.0,
        behavior: PrimaryFireBehavior::ArcingExplosive,
        projectiles_per_shot: 1,
        upgrade_part_item_id: "weapon_part_grenade",
    },
    PrimaryWeaponDef {
        kind: PrimaryWeaponKind::TrackingMissile,
        stable_id: "tracking_missile",
        name: "Hunter Missile",
        base_damage: 120.0,
        base_fire_interval_ms: 1_300.0,
        source_max_ammo: 6,
        range: 220.0,
        projectile_speed_per_frame: 0.9,
        base_spread: 0.0,
        automatic: false,
        base_explosion_radius: 6.0,
        behavior: PrimaryFireBehavior::HomingExplosive,
        projectiles_per_shot: 1,
        upgrade_part_item_id: "weapon_part_rocket",
    },
    PrimaryWeaponDef {
        kind: PrimaryWeaponKind::CaptureNet,
        stable_id: "capture_net",
        name: "Capture Net",
        base_damage: 0.0,
        base_fire_interval_ms: 600.0,
        source_max_ammo: 1,
        range: 22.0,
        projectile_speed_per_frame: 0.0,
        base_spread: 0.0,
        automatic: false,
        base_explosion_radius: 0.0,
        behavior: PrimaryFireBehavior::CaptureTool,
        projectiles_per_shot: 0,
        // Source carried this benign map entry only to satisfy TypeScript's
        // Record type; the tool is filtered and cannot actually upgrade.
        upgrade_part_item_id: "weapon_part_pistol",
    },
];

pub fn primary_weapon(kind: PrimaryWeaponKind) -> &'static PrimaryWeaponDef {
    &PRIMARY_WEAPONS[kind.index()]
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PrimaryWeaponStats {
    pub kind: PrimaryWeaponKind,
    pub level: u8,
    pub damage_per_projectile: f64,
    pub fire_interval_ms: f64,
    pub spread: f64,
    pub explosion_radius: f64,
    pub knockback_bonus: f64,
    pub projectiles_per_shot: u8,
    pub range: f64,
    pub projectile_speed_per_frame: f64,
    pub projectile_lifetime_ms: u32,
    pub auto_target_eligible: bool,
    pub automatic: bool,
    pub behavior: PrimaryFireBehavior,
}

impl PrimaryWeaponStats {
    pub fn maximum_direct_damage_per_shot(self) -> f64 {
        self.damage_per_projectile * f64::from(self.projectiles_per_shot)
    }
}

pub fn primary_weapon_stats(kind: PrimaryWeaponKind, level: u8) -> PrimaryWeaponStats {
    let definition = primary_weapon(kind);
    let level = if kind.is_upgradeable() {
        level.clamp(1, MAX_PRIMARY_WEAPON_LEVEL)
    } else {
        1
    };
    let tier = f64::from(level - 1);
    let explosion_bonus = if level >= 3 {
        definition.base_explosion_radius * 0.15 * f64::from(level - 2)
    } else {
        0.0
    };
    let knockback_bonus = if level >= 3 {
        1.0 + 0.25 * f64::from(level - 2)
    } else {
        0.0
    };
    PrimaryWeaponStats {
        kind,
        level,
        damage_per_projectile: definition.base_damage * (1.0 + 0.22 * tier),
        fire_interval_ms: definition.base_fire_interval_ms * 0.88_f64.powi(i32::from(level - 1)),
        spread: (definition.base_spread * 0.85_f64.powi(i32::from(level - 1))).max(0.0),
        explosion_radius: definition.base_explosion_radius + explosion_bonus,
        knockback_bonus,
        projectiles_per_shot: definition.projectiles_per_shot,
        range: definition.range,
        projectile_speed_per_frame: definition.projectile_speed_per_frame,
        projectile_lifetime_ms: kind.source_projectile_lifetime_ms(),
        auto_target_eligible: kind.source_auto_target_eligible(),
        automatic: definition.automatic,
        behavior: definition.behavior,
    }
}

pub fn primary_weapon_upgrade_cost(kind: PrimaryWeaponKind, next_level: u8) -> CombatCost {
    if !kind.is_upgradeable() {
        return CombatCost::default();
    }
    let next_level = next_level.clamp(2, MAX_PRIMARY_WEAPON_LEVEL);
    let tier = u32::from(next_level - 1);
    CombatCost::from_parts(
        0,
        &[
            (GEAR_ITEM_ID, 6 * tier),
            (primary_weapon(kind).upgrade_part_item_id, 2 * tier),
        ],
    )
}

/// Heavy's persisted level 1..5 maps directly to Starfall's rank 0..4.
pub const fn heavy_level_to_starfall_rank(level: u8) -> u32 {
    let level = if level < 1 {
        1
    } else if level > MAX_PRIMARY_WEAPON_LEVEL {
        MAX_PRIMARY_WEAPON_LEVEL
    } else {
        level
    };
    level as u32 - 1
}

pub const fn starfall_rank_to_heavy_level(rank: u32) -> u8 {
    let max_rank = (MAX_PRIMARY_WEAPON_LEVEL - 1) as u32;
    if rank > max_rank {
        MAX_PRIMARY_WEAPON_LEVEL
    } else {
        rank as u8 + 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrimaryWeaponLevelsRecord {
    pub pistol: u8,
    pub rifle: u8,
    pub shotgun: u8,
    pub rocket: u8,
    pub laser: u8,
    pub grenade: u8,
    pub tracking_missile: u8,
    pub capture_net: u8,
}

impl Default for PrimaryWeaponLevelsRecord {
    fn default() -> Self {
        Self {
            pistol: 1,
            rifle: 1,
            shotgun: 1,
            rocket: 1,
            laser: 1,
            grenade: 1,
            tracking_missile: 1,
            capture_net: 1,
        }
    }
}

impl PrimaryWeaponLevelsRecord {
    pub const fn level(&self, kind: PrimaryWeaponKind) -> u8 {
        match kind {
            PrimaryWeaponKind::Pistol => self.pistol,
            PrimaryWeaponKind::Rifle => self.rifle,
            PrimaryWeaponKind::Shotgun => self.shotgun,
            PrimaryWeaponKind::Rocket => self.rocket,
            PrimaryWeaponKind::Laser => self.laser,
            PrimaryWeaponKind::Grenade => self.grenade,
            PrimaryWeaponKind::TrackingMissile => self.tracking_missile,
            PrimaryWeaponKind::CaptureNet => self.capture_net,
        }
    }

    pub fn set_level(&mut self, kind: PrimaryWeaponKind, level: u8) {
        let level = if kind.is_upgradeable() {
            level.clamp(1, MAX_PRIMARY_WEAPON_LEVEL)
        } else {
            1
        };
        match kind {
            PrimaryWeaponKind::Pistol => self.pistol = level,
            PrimaryWeaponKind::Rifle => self.rifle = level,
            PrimaryWeaponKind::Shotgun => self.shotgun = level,
            PrimaryWeaponKind::Rocket => self.rocket = level,
            PrimaryWeaponKind::Laser => self.laser = level,
            PrimaryWeaponKind::Grenade => self.grenade = level,
            PrimaryWeaponKind::TrackingMissile => self.tracking_missile = level,
            PrimaryWeaponKind::CaptureNet => self.capture_net = 1,
        }
    }

    fn normalize(&mut self) {
        for kind in PrimaryWeaponKind::ALL {
            let level = self.level(kind);
            self.set_level(kind, level);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrimaryWeaponUpgradeEffect {
    pub weapon: PrimaryWeaponKind,
    pub previous_level: u8,
    pub new_level: u8,
    pub cost: CombatCost,
    pub stats: PrimaryWeaponStats,
}

// ---------------------------------------------------------------------------
// Armor Capsule purchase roster.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmorCapsuleUpgradeId {
    FlightArmor,
    SpeedBoost,
    TitanDefense,
    FireInfusion,
    ElectricInfusion,
    QuantumArmor,
}

impl ArmorCapsuleUpgradeId {
    pub const ALL: [Self; 6] = [
        Self::FlightArmor,
        Self::SpeedBoost,
        Self::TitanDefense,
        Self::FireInfusion,
        Self::ElectricInfusion,
        Self::QuantumArmor,
    ];

    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::FlightArmor => "flight_armor",
            Self::SpeedBoost => "speed_boost",
            Self::TitanDefense => "titan_defense",
            Self::FireInfusion => "fire_infusion",
            Self::ElectricInfusion => "electric_infusion",
            Self::QuantumArmor => "quantum_armor",
        }
    }

    pub fn from_stable_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.stable_id() == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapsuleArmorSlot {
    Chest,
    Boots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapsuleArmorRarity {
    Rare,
    Epic,
    Legendary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapsuleArmorElement {
    Fire,
    Electric,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CapsuleArmorPieceDef {
    pub stable_id: &'static str,
    pub name: &'static str,
    pub slot: CapsuleArmorSlot,
    pub defense: f64,
    pub health_bonus: f64,
    pub stamina_bonus: f64,
    pub level: u8,
    pub rarity: CapsuleArmorRarity,
    pub max_modules: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ArmorCapsuleEffects {
    pub movement_speed_bonus: f64,
    pub flight_capability: bool,
    /// Declared by the capsule catalog. Heavy Water did not feed this field
    /// into `ArmorSystem`; a Starfall adapter may map it to native armor
    /// upgrade state deliberately rather than silently double-counting it.
    pub declared_defense_bonus: f64,
    pub special_ability: Option<&'static str>,
    pub granted_piece: Option<CapsuleArmorPieceDef>,
    pub set_element: Option<CapsuleArmorElement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ArmorCapsuleUpgradeDef {
    pub id: ArmorCapsuleUpgradeId,
    pub stable_id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub tier: u8,
    pub cost_credits: u32,
    pub effects: ArmorCapsuleEffects,
}

const FLIGHT_CHESTPLATE: CapsuleArmorPieceDef = CapsuleArmorPieceDef {
    stable_id: "flight_chestplate",
    name: "Aero-Flight Chestplate",
    slot: CapsuleArmorSlot::Chest,
    defense: 20.0,
    health_bonus: 30.0,
    stamina_bonus: 15.0,
    level: 2,
    rarity: CapsuleArmorRarity::Rare,
    max_modules: 2,
};

const KINETIC_BOOTS: CapsuleArmorPieceDef = CapsuleArmorPieceDef {
    stable_id: "kinetic_boots",
    name: "Kinetic Boots",
    slot: CapsuleArmorSlot::Boots,
    defense: 12.0,
    health_bonus: 15.0,
    stamina_bonus: 25.0,
    level: 2,
    rarity: CapsuleArmorRarity::Rare,
    max_modules: 2,
};

const TITAN_CHESTPLATE: CapsuleArmorPieceDef = CapsuleArmorPieceDef {
    stable_id: "titan_chestplate",
    name: "Titan Chestplate",
    slot: CapsuleArmorSlot::Chest,
    defense: 50.0,
    health_bonus: 80.0,
    stamina_bonus: 20.0,
    level: 4,
    rarity: CapsuleArmorRarity::Epic,
    max_modules: 2,
};

const QUANTUM_EXO_SUIT: CapsuleArmorPieceDef = CapsuleArmorPieceDef {
    stable_id: "quantum_full_set",
    name: "Quantum Exo-Suit",
    slot: CapsuleArmorSlot::Chest,
    defense: 60.0,
    health_bonus: 100.0,
    stamina_bonus: 30.0,
    level: 5,
    rarity: CapsuleArmorRarity::Legendary,
    max_modules: 3,
};

pub const ARMOR_CAPSULE_UPGRADES: [ArmorCapsuleUpgradeDef; 6] = [
    ArmorCapsuleUpgradeDef {
        id: ArmorCapsuleUpgradeId::FlightArmor,
        stable_id: "flight_armor",
        name: "Aero-Flight Module",
        description: "Grants triple-jump flight capability. Soar through Detroit's skies.",
        tier: 1,
        cost_credits: 0,
        effects: ArmorCapsuleEffects {
            movement_speed_bonus: 0.0,
            flight_capability: true,
            declared_defense_bonus: 10.0,
            special_ability: None,
            granted_piece: Some(FLIGHT_CHESTPLATE),
            set_element: None,
        },
    },
    ArmorCapsuleUpgradeDef {
        id: ArmorCapsuleUpgradeId::SpeedBoost,
        stable_id: "speed_boost",
        name: "Kinetic Accelerator",
        description: "Enhances movement speed by 25%. Sprint like the wind.",
        tier: 2,
        cost_credits: 500,
        effects: ArmorCapsuleEffects {
            movement_speed_bonus: 0.25,
            flight_capability: false,
            declared_defense_bonus: 0.0,
            special_ability: None,
            granted_piece: Some(KINETIC_BOOTS),
            set_element: None,
        },
    },
    ArmorCapsuleUpgradeDef {
        id: ArmorCapsuleUpgradeId::TitanDefense,
        stable_id: "titan_defense",
        name: "Titan Defense Matrix",
        description: "Massive defense upgrade. Reduces incoming damage significantly.",
        tier: 3,
        cost_credits: 1_200,
        effects: ArmorCapsuleEffects {
            movement_speed_bonus: 0.0,
            flight_capability: false,
            declared_defense_bonus: 40.0,
            special_ability: None,
            granted_piece: Some(TITAN_CHESTPLATE),
            set_element: None,
        },
    },
    ArmorCapsuleUpgradeDef {
        id: ArmorCapsuleUpgradeId::FireInfusion,
        stable_id: "fire_infusion",
        name: "Inferno Core",
        description: "Infuses armor with fire element. Burn enemies on contact.",
        tier: 3,
        cost_credits: 1_500,
        effects: ArmorCapsuleEffects {
            movement_speed_bonus: 0.0,
            flight_capability: false,
            declared_defense_bonus: 15.0,
            special_ability: Some("Fire Aura - burns nearby enemies"),
            granted_piece: None,
            set_element: Some(CapsuleArmorElement::Fire),
        },
    },
    ArmorCapsuleUpgradeDef {
        id: ArmorCapsuleUpgradeId::ElectricInfusion,
        stable_id: "electric_infusion",
        name: "Storm Conductor",
        description: "Infuses armor with electric element. Chain lightning on kills.",
        tier: 4,
        cost_credits: 2_000,
        effects: ArmorCapsuleEffects {
            movement_speed_bonus: 0.0,
            flight_capability: false,
            declared_defense_bonus: 20.0,
            special_ability: Some("Lightning Shield - shocks attackers"),
            granted_piece: None,
            set_element: Some(CapsuleArmorElement::Electric),
        },
    },
    ArmorCapsuleUpgradeDef {
        id: ArmorCapsuleUpgradeId::QuantumArmor,
        stable_id: "quantum_armor",
        name: "Quantum Exo-Suit",
        description: "Ultimate armor upgrade. Quantum-level protection and abilities.",
        tier: 5,
        cost_credits: 5_000,
        effects: ArmorCapsuleEffects {
            movement_speed_bonus: 0.15,
            flight_capability: true,
            declared_defense_bonus: 80.0,
            special_ability: Some("Quantum Dash - short-range teleport"),
            granted_piece: Some(QUANTUM_EXO_SUIT),
            set_element: None,
        },
    },
];

pub fn armor_capsule_upgrade(id: ArmorCapsuleUpgradeId) -> &'static ArmorCapsuleUpgradeDef {
    &ARMOR_CAPSULE_UPGRADES[id as usize]
}

/// Source-compatible applied-id list. Unknown future IDs are retained on
/// round-trip but never treated as a known unlock by this schema version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArmorCapsuleRecord {
    pub applied_upgrade_ids: BTreeSet<String>,
}

impl ArmorCapsuleRecord {
    pub fn is_applied(&self, id: ArmorCapsuleUpgradeId) -> bool {
        self.applied_upgrade_ids.contains(id.stable_id())
    }

    pub fn mark_applied(&mut self, id: ArmorCapsuleUpgradeId) -> bool {
        self.applied_upgrade_ids.insert(id.stable_id().to_owned())
    }

    pub fn has_flight_armor(&self) -> bool {
        self.is_applied(ArmorCapsuleUpgradeId::FlightArmor)
            || self.is_applied(ArmorCapsuleUpgradeId::QuantumArmor)
    }

    fn normalize(&mut self) {
        self.applied_upgrade_ids.retain(|id| !id.trim().is_empty());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArmorCapsuleUnlockEffect {
    pub upgrade: ArmorCapsuleUpgradeId,
    pub cost: CombatCost,
    /// Apply through Starfall's canonical armor/mobility systems only after
    /// the purchase commits. Never replay these effects merely to load the
    /// applied-ID bookkeeping record.
    pub effects: ArmorCapsuleEffects,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    fn rich_balances() -> CombatBalances {
        CombatBalances::new(
            50_000,
            [
                (GEAR_ITEM_ID, 5_000),
                (ENERGY_CORE_ITEM_ID, 5_000),
                (NANO_FIBER_ITEM_ID, 5_000),
                (CIRCUIT_BOARD_ITEM_ID, 5_000),
                ("weapon_part_pistol", 500),
                ("weapon_part_rifle", 500),
                ("weapon_part_shotgun", 500),
                ("weapon_part_rocket", 500),
                ("weapon_part_laser", 500),
                ("weapon_part_grenade", 500),
            ],
        )
    }

    #[test]
    fn melee_catalog_and_all_twelve_prices_match_the_executable() {
        assert_eq!(MELEE_WEAPONS.len(), 4);
        assert_eq!(
            MELEE_WEAPONS.map(|weapon| weapon.stable_id),
            ["glaive", "daggers", "axe", "whip"]
        );
        assert_eq!(melee_weapon(MeleeArsenalId::Glaive).base_damage, 200.0);
        assert_eq!(melee_weapon(MeleeArsenalId::Daggers).base_cooldown_ms, 180);
        assert_eq!(melee_weapon(MeleeArsenalId::Axe).knockback, 18.0);
        assert_eq!(melee_weapon(MeleeArsenalId::Whip).primary_radius, 16.0);

        let expected = [
            (
                MeleeArsenalId::Glaive,
                [(60, 10, 6, 0), (110, 20, 14, 0), (160, 30, 22, 10)],
            ),
            (
                MeleeArsenalId::Daggers,
                [(70, 12, 8, 0), (120, 22, 16, 0), (170, 32, 24, 10)],
            ),
            (
                MeleeArsenalId::Axe,
                [(90, 16, 10, 0), (140, 26, 18, 8), (200, 38, 28, 14)],
            ),
            (
                MeleeArsenalId::Whip,
                [(80, 14, 9, 0), (130, 24, 17, 8), (190, 36, 26, 12)],
            ),
        ];
        for (weapon, costs) in expected {
            for (tier, (gears, cores, nano, circuits)) in [
                MeleeUnlockTier::Own,
                MeleeUnlockTier::Combo,
                MeleeUnlockTier::Special,
            ]
            .into_iter()
            .zip(costs)
            {
                let cost = melee_unlock_cost(weapon, tier);
                assert_eq!(cost.item_quantity(GEAR_ITEM_ID), gears);
                assert_eq!(cost.item_quantity(ENERGY_CORE_ITEM_ID), cores);
                assert_eq!(cost.item_quantity(NANO_FIBER_ITEM_ID), nano);
                assert_eq!(cost.item_quantity(CIRCUIT_BOARD_ITEM_ID), circuits);
                assert_eq!(cost.credits, 0);
            }
        }
    }

    #[test]
    fn melee_unlock_purchase_is_atomic_and_higher_tiers_imply_ownership() {
        let mut save = HeavyCombatSave::default();
        let mut poor = CombatBalances::new(
            0,
            [
                (GEAR_ITEM_ID, 110),
                (ENERGY_CORE_ITEM_ID, 19),
                (NANO_FIBER_ITEM_ID, 14),
            ],
        );
        let original = poor.clone();
        assert_eq!(
            save.purchase_melee_unlock(
                "local:p1",
                &mut poor,
                MeleeArsenalId::Glaive,
                MeleeUnlockTier::Combo,
            ),
            Err(CombatPurchaseError::InsufficientItem {
                item_id: ENERGY_CORE_ITEM_ID.to_owned(),
                needed: 20,
                available: 19,
            })
        );
        assert_eq!(poor, original);
        assert!(save.players.is_empty());

        let mut rich = rich_balances();
        let effect = save
            .purchase_melee_unlock(
                "local:p1",
                &mut rich,
                MeleeArsenalId::Glaive,
                MeleeUnlockTier::Combo,
            )
            .unwrap();
        assert!(effect.resulting_state.unlocked);
        assert!(effect.resulting_state.combo_unlocked);
        assert!(!effect.resulting_state.special_unlocked);
        assert_eq!(rich.item_count(GEAR_ITEM_ID), 4_890);

        let before_save = save.clone();
        let before_rich = rich.clone();
        assert_eq!(
            save.purchase_melee_unlock(
                "local:p1",
                &mut rich,
                MeleeArsenalId::Glaive,
                MeleeUnlockTier::Combo,
            ),
            Err(CombatPurchaseError::AlreadyUnlocked)
        );
        assert_eq!(save, before_save);
        assert_eq!(rich, before_rich);
    }

    #[test]
    fn melee_primary_descriptors_preserve_combo_counts_timing_and_modifiers() {
        let base = MeleeWeaponUnlockRecord {
            unlocked: true,
            ..Default::default()
        };
        let combo = MeleeWeaponUnlockRecord {
            unlocked: true,
            combo_unlocked: true,
            ..Default::default()
        };

        let glaive = melee_primary_action(MeleeArsenalId::Glaive, combo).unwrap();
        let MeleePrimaryPattern::ForwardCombo { hits, .. } = glaive.pattern;
        assert_eq!(hits.len(), 3);
        assert_eq!(
            hits.iter().map(|hit| hit.at_ms).collect::<Vec<_>>(),
            [0, 130, 260]
        );
        assert_close(hits[0].damage, 280.0);
        assert_close(hits[0].cone.radius, 8.8);
        assert_eq!(glaive.cooldown_begins_at_ms, 390);

        let daggers = melee_primary_action(MeleeArsenalId::Daggers, combo).unwrap();
        let MeleePrimaryPattern::ForwardCombo {
            hits,
            phantom_step_cue_distance,
            ..
        } = daggers.pattern;
        assert_eq!(hits.len(), 6);
        assert_eq!(hits.last().unwrap().at_ms, 350);
        assert_close(hits[0].damage, 73.15);
        assert_eq!(phantom_step_cue_distance, Some(2.5));
        assert_eq!(daggers.cooldown_ms, 288);
        assert_eq!(daggers.cooldown_begins_at_ms, 420);

        let axe = melee_primary_action(MeleeArsenalId::Axe, combo).unwrap();
        let MeleePrimaryPattern::ForwardCombo { hits, .. } = axe.pattern;
        assert_eq!(hits.len(), 2);
        assert_close(hits[1].damage, 452.2);
        assert_close(hits[1].cone.radius, 6.5 * 1.1 * 1.05);

        let whip = melee_primary_action(MeleeArsenalId::Whip, combo).unwrap();
        let MeleePrimaryPattern::ForwardCombo {
            pull_closest_target_distance,
            ..
        } = whip.pattern;
        assert_eq!(pull_closest_target_distance, Some(3.0));
        assert_eq!(whip.busy_until_ms, 380);

        assert_eq!(
            melee_primary_action(MeleeArsenalId::Axe, MeleeWeaponUnlockRecord::default()),
            Err(MeleeActionError::WeaponLocked)
        );
        assert_eq!(
            melee_primary_action(MeleeArsenalId::Glaive, base)
                .unwrap()
                .weapon_level,
            1
        );
    }

    #[test]
    fn melee_special_descriptors_preserve_each_signature_rule() {
        let unlocked = MeleeWeaponUnlockRecord {
            unlocked: true,
            combo_unlocked: true,
            special_unlocked: true,
        };
        let glaive = melee_special_action(MeleeArsenalId::Glaive, unlocked, 0).unwrap();
        match glaive.pattern {
            MeleeSpecialPattern::OrbitRing {
                orbit_radius,
                duration_ms,
                repeat_hit_interval_ms,
                damage,
                ..
            } => {
                assert_eq!(orbit_radius, 6.0);
                assert_eq!(duration_ms, 1_400);
                assert_eq!(repeat_hit_interval_ms, 220);
                assert_close(damage, 252.0);
            }
            other => panic!("wrong glaive pattern: {other:?}"),
        }

        assert_eq!(
            melee_special_action(MeleeArsenalId::Daggers, unlocked, 0),
            Err(MeleeActionError::NoTarget)
        );
        let daggers = melee_special_action(MeleeArsenalId::Daggers, unlocked, 9).unwrap();
        match daggers.pattern {
            MeleeSpecialPattern::NearestTargetSequence { range, strikes } => {
                assert_eq!(range, 18.0);
                assert_eq!(strikes.len(), 3);
                assert_eq!(strikes[2].at_ms, 280);
                assert_close(strikes[0].damage, 307.8);
            }
            other => panic!("wrong dagger pattern: {other:?}"),
        }
        assert_eq!(daggers.cooldown_begins_at_ms, 420);

        let axe = melee_special_action(MeleeArsenalId::Axe, unlocked, 0).unwrap();
        match axe.pattern {
            MeleeSpecialPattern::ExpandingGroundRing {
                start_radius,
                expansion_distance,
                vertical_half_band,
                damage,
                knockback,
                ..
            } => {
                assert_eq!(start_radius, 1.0);
                assert_eq!(expansion_distance, 14.0);
                assert_eq!(vertical_half_band, 3.0);
                assert_close(damage, 957.6);
                assert_close(knockback, 27.0);
            }
            other => panic!("wrong axe pattern: {other:?}"),
        }

        let whip = melee_special_action(MeleeArsenalId::Whip, unlocked, 0).unwrap();
        match whip.pattern {
            MeleeSpecialPattern::RadialTicks {
                radius,
                visual_duration_ms,
                ticks,
            } => {
                assert_close(radius, 10.8);
                assert_eq!(visual_duration_ms, 1_000);
                assert_eq!(
                    ticks.iter().map(|tick| tick.at_ms).collect::<Vec<_>>(),
                    [0, 220, 440, 660]
                );
                assert_close(ticks[0].damage, 151.2);
            }
            other => panic!("wrong whip pattern: {other:?}"),
        }
    }

    #[test]
    fn elemental_catalog_scaling_and_breakpoints_match_source_math() {
        assert_eq!(
            ELEMENTAL_SPECIALS.map(|element| element.stable_id),
            [
                "lightning",
                "ice",
                "fireball",
                "inferno",
                "windstorm",
                "psychic"
            ]
        );
        let expected_tracking_breakpoints = [
            (1, 2, 1),
            (2, 2, 1),
            (3, 3, 2),
            (5, 4, 3),
            (10, 6, 5),
            (15, 8, 8),
            (19, 10, 8),
            (20, 10, 8),
        ];
        for (level, targets, projectiles) in expected_tracking_breakpoints {
            let stats = elemental_stats(ElementalKind::Lightning, level);
            assert_eq!(stats.max_targets, targets);
            assert_eq!(stats.projectiles_per_target, projectiles);
        }
        let lightning_20 = elemental_stats(ElementalKind::Lightning, 20);
        assert_eq!(lightning_20.damage_per_hit, 689);
        assert_close(lightning_20.radius, 14.64);
        assert_eq!(lightning_20.cooldown_micros, 820_406);

        let fireball_20 = elemental_stats(ElementalKind::Fireball, 20);
        assert_eq!(fireball_20.damage_per_hit, 841);
        assert_eq!(fireball_20.cooldown_micros, 800_000);

        let psychic_20 = elemental_stats(ElementalKind::Psychic, 20);
        assert_eq!(psychic_20.max_targets, 999);
        assert_eq!(psychic_20.projectiles_per_target, 0);
        assert_eq!(psychic_20.damage_per_hit, 1_262);
        assert_close(psychic_20.radius, 76.8);
        assert_eq!(elemental_upgrade_cost(1), 450);
        assert_eq!(elemental_upgrade_cost(19), 3_150);
    }

    #[test]
    fn elemental_purchases_are_atomic_capped_and_owner_isolated() {
        let mut save = HeavyCombatSave::default();
        let mut balance = CombatBalances::new(449, std::iter::empty::<(&str, u32)>());
        let original = balance.clone();
        assert_eq!(
            save.purchase_elemental_level("local:p2", &mut balance, ElementalKind::Ice),
            Err(CombatPurchaseError::InsufficientCredits {
                needed: 450,
                available: 449,
            })
        );
        assert_eq!(balance, original);
        assert!(save.players.is_empty());

        balance.credits = 450;
        let upgraded = save
            .purchase_elemental_level("local:p2", &mut balance, ElementalKind::Ice)
            .unwrap();
        assert_eq!(upgraded.new_level, 2);
        assert_eq!(balance.credits, 0);
        assert_eq!(save.player("local:p2").unwrap().elementals.ice, 2);
        assert!(save.player("local:p1").is_none());

        save.player_mut("local:p2").elementals.ice = MAX_ELEMENTAL_LEVEL;
        let before = save.clone();
        assert_eq!(
            save.purchase_elemental_level("local:p2", &mut balance, ElementalKind::Ice),
            Err(CombatPurchaseError::MaxLevel)
        );
        assert_eq!(save, before);
    }

    #[test]
    fn elemental_casts_return_beam_or_dome_and_cooldowns_are_deterministic() {
        let levels = ElementalLevelsRecord {
            lightning: 3,
            ..ElementalLevelsRecord::default()
        };
        let mut split = ElementalRuntimeState::default();
        let beam = split.try_cast(&levels, ElementalKind::Lightning).unwrap();
        match beam.pattern {
            ElementalActionPattern::TrackingMegaBeam {
                length,
                beam_radius,
                damage,
                lifetime_ms,
                damage_scan_interval_ms,
                ..
            } => {
                assert_eq!(length, 220.0);
                assert_close(beam_radius, 5.12);
                assert_eq!(damage, 153 * 3 * 2);
                assert_eq!(lifetime_ms, 1_400);
                assert_eq!(damage_scan_interval_ms, 80);
            }
            other => panic!("wrong tracking pattern: {other:?}"),
        }
        assert!(matches!(
            split.try_cast(&levels, ElementalKind::Lightning),
            Err(ElementalCastError::CoolingDown { .. })
        ));

        let cooldown = elemental_stats(ElementalKind::Lightning, 3).cooldown_micros;
        split.advance(Duration::from_nanos(400));
        split.advance(Duration::from_nanos(600));
        split.advance(Duration::from_micros(cooldown - 1));
        assert_eq!(split.cooldown_remaining_micros(ElementalKind::Lightning), 0);

        let mut whole = ElementalRuntimeState::default();
        whole.try_cast(&levels, ElementalKind::Lightning).unwrap();
        whole.advance(Duration::from_micros(cooldown));
        assert_eq!(split, whole);

        let dome = elemental_cast_effect(ElementalKind::Windstorm, 1);
        match dome.pattern {
            ElementalActionPattern::PlayerCenteredDome {
                radius,
                damage_ticks,
                tick_interval_ms,
                lifetime_ms,
                repeated_hit_multiplier,
                ..
            } => {
                assert_eq!(radius, 14.0);
                assert_eq!(damage_ticks, 4);
                assert_eq!(tick_interval_ms, 200);
                assert_eq!(lifetime_ms, 1_100);
                assert_eq!(repeated_hit_multiplier, 0.35);
            }
            other => panic!("wrong dome pattern: {other:?}"),
        }
    }

    #[test]
    fn primary_weapon_catalog_preserves_all_balance_identities() {
        assert_eq!(PRIMARY_WEAPONS.len(), 8);
        assert_eq!(
            PRIMARY_WEAPONS.map(|weapon| weapon.stable_id),
            [
                "pistol",
                "rifle",
                "shotgun",
                "rocket",
                "laser",
                "grenade",
                "tracking_missile",
                "capture_net",
            ]
        );
        let shotgun = primary_weapon_stats(PrimaryWeaponKind::Shotgun, 1);
        assert_eq!(shotgun.projectiles_per_shot, 8);
        assert_eq!(shotgun.maximum_direct_damage_per_shot(), 64.0);
        assert_eq!(shotgun.behavior, PrimaryFireBehavior::PelletBurst);

        let missile = primary_weapon(PrimaryWeaponKind::TrackingMissile);
        assert_eq!(missile.base_damage, 120.0);
        assert_eq!(missile.base_explosion_radius, 6.0);
        assert_eq!(missile.upgrade_part_item_id, "weapon_part_rocket");
        let missile_stats = primary_weapon_stats(PrimaryWeaponKind::TrackingMissile, 1);
        assert_eq!(missile_stats.projectile_lifetime_ms, 6_000);
        assert!(missile_stats.auto_target_eligible);

        let net = primary_weapon_stats(PrimaryWeaponKind::CaptureNet, 5);
        assert_eq!(net.level, 1);
        assert_eq!(net.damage_per_projectile, 0.0);
        assert_eq!(net.behavior, PrimaryFireBehavior::CaptureTool);
        assert_eq!(net.projectiles_per_shot, 0);
        assert_eq!(net.projectile_lifetime_ms, 0);
        assert!(!net.auto_target_eligible);
        assert!(!primary_weapon_stats(PrimaryWeaponKind::Grenade, 1).auto_target_eligible);

        assert_eq!(PRIMARY_SOURCE_GLOBAL_RULES.tracking_homing_range, 120.0);
        assert_eq!(
            PRIMARY_SOURCE_GLOBAL_RULES.tracking_steer_per_60hz_frame,
            0.18
        );
        assert_eq!(
            PRIMARY_SOURCE_GLOBAL_RULES.auto_target_half_angle_degrees,
            25.0
        );
        assert_eq!(PRIMARY_SOURCE_GLOBAL_RULES.auto_target_range, 140.0);
        assert_eq!(PRIMARY_SOURCE_GLOBAL_RULES.auto_target_pull, 0.55);
        assert_eq!(PRIMARY_SOURCE_GLOBAL_RULES.vehicle_damage_multiplier, 1.5);
        assert_eq!(
            PRIMARY_SOURCE_GLOBAL_RULES.vehicle_projectile_speed_multiplier,
            2.5
        );
    }

    #[test]
    fn primary_level_scaling_costs_and_rank_mapping_are_exact() {
        let rocket = primary_weapon_stats(PrimaryWeaponKind::Rocket, 5);
        assert_close(rocket.damage_per_projectile, 188.0);
        assert_close(rocket.fire_interval_ms, 1_500.0 * 0.88_f64.powi(4));
        assert_close(rocket.explosion_radius, 7.25);
        assert_close(rocket.knockback_bonus, 1.75);

        for (next_level, gears, parts) in [(2, 6, 2), (3, 12, 4), (4, 18, 6), (5, 24, 8)] {
            let cost = primary_weapon_upgrade_cost(PrimaryWeaponKind::Rifle, next_level);
            assert_eq!(cost.item_quantity(GEAR_ITEM_ID), gears);
            assert_eq!(cost.item_quantity("weapon_part_rifle"), parts);
        }
        assert_eq!(heavy_level_to_starfall_rank(1), 0);
        assert_eq!(heavy_level_to_starfall_rank(5), 4);
        assert_eq!(starfall_rank_to_heavy_level(0), 1);
        assert_eq!(starfall_rank_to_heavy_level(99), 5);
    }

    #[test]
    fn primary_upgrade_purchase_is_atomic_and_tools_cannot_spend_parts() {
        let mut save = HeavyCombatSave::default();
        let mut poor = CombatBalances::new(0, [(GEAR_ITEM_ID, 6), ("weapon_part_rocket", 1)]);
        let original = poor.clone();
        assert_eq!(
            save.purchase_primary_weapon_level(
                "local:p1",
                &mut poor,
                PrimaryWeaponKind::TrackingMissile,
            ),
            Err(CombatPurchaseError::InsufficientItem {
                item_id: "weapon_part_rocket".to_owned(),
                needed: 2,
                available: 1,
            })
        );
        assert_eq!(poor, original);
        assert!(save.players.is_empty());

        let mut rich = rich_balances();
        let effect = save
            .purchase_primary_weapon_level(
                "local:p1",
                &mut rich,
                PrimaryWeaponKind::TrackingMissile,
            )
            .unwrap();
        assert_eq!(effect.new_level, 2);
        assert_eq!(effect.cost.item_quantity("weapon_part_rocket"), 2);

        let before = rich.clone();
        assert_eq!(
            save.purchase_primary_weapon_level(
                "local:p1",
                &mut rich,
                PrimaryWeaponKind::CaptureNet,
            ),
            Err(CombatPurchaseError::ToolNotUpgradeable)
        );
        assert_eq!(rich, before);
    }

    #[test]
    fn armor_capsule_catalog_and_effect_descriptors_match_source() {
        assert_eq!(
            ARMOR_CAPSULE_UPGRADES.map(|upgrade| upgrade.stable_id),
            [
                "flight_armor",
                "speed_boost",
                "titan_defense",
                "fire_infusion",
                "electric_infusion",
                "quantum_armor",
            ]
        );
        assert_eq!(
            ARMOR_CAPSULE_UPGRADES.map(|upgrade| upgrade.cost_credits),
            [0, 500, 1_200, 1_500, 2_000, 5_000]
        );
        let flight = armor_capsule_upgrade(ArmorCapsuleUpgradeId::FlightArmor);
        assert!(flight.effects.flight_capability);
        assert_eq!(flight.effects.declared_defense_bonus, 10.0);
        assert_eq!(flight.effects.granted_piece, Some(FLIGHT_CHESTPLATE));

        let speed = armor_capsule_upgrade(ArmorCapsuleUpgradeId::SpeedBoost);
        assert_eq!(speed.effects.movement_speed_bonus, 0.25);
        assert_eq!(speed.effects.granted_piece, Some(KINETIC_BOOTS));

        let quantum = armor_capsule_upgrade(ArmorCapsuleUpgradeId::QuantumArmor);
        assert_eq!(quantum.effects.declared_defense_bonus, 80.0);
        assert_eq!(quantum.effects.movement_speed_bonus, 0.15);
        assert!(quantum.effects.flight_capability);
        assert_eq!(quantum.effects.granted_piece, Some(QUANTUM_EXO_SUIT));
    }

    #[test]
    fn armor_capsule_unlock_is_atomic_idempotent_and_has_no_tier_prerequisite() {
        let mut save = HeavyCombatSave::default();
        let mut poor = CombatBalances::new(4_999, std::iter::empty::<(&str, u32)>());
        let original = poor.clone();
        assert_eq!(
            save.purchase_capsule_upgrade(
                "local:p3",
                &mut poor,
                ArmorCapsuleUpgradeId::QuantumArmor,
            ),
            Err(CombatPurchaseError::InsufficientCredits {
                needed: 5_000,
                available: 4_999,
            })
        );
        assert_eq!(poor, original);
        assert!(save.players.is_empty());

        poor.credits = 5_000;
        let effect = save
            .purchase_capsule_upgrade("local:p3", &mut poor, ArmorCapsuleUpgradeId::QuantumArmor)
            .unwrap();
        assert_eq!(poor.credits, 0);
        assert!(effect.effects.flight_capability);
        let record = &save.player("local:p3").unwrap().armor_capsule;
        assert!(record.is_applied(ArmorCapsuleUpgradeId::QuantumArmor));
        assert!(record.has_flight_armor());
        assert!(!record.is_applied(ArmorCapsuleUpgradeId::FlightArmor));

        let before_save = save.clone();
        let before_balance = poor.clone();
        assert_eq!(
            save.purchase_capsule_upgrade(
                "local:p3",
                &mut poor,
                ArmorCapsuleUpgradeId::QuantumArmor,
            ),
            Err(CombatPurchaseError::AlreadyUnlocked)
        );
        assert_eq!(save, before_save);
        assert_eq!(poor, before_balance);
    }

    #[test]
    fn save_round_trip_is_owner_keyed_normalized_and_excludes_session_state() {
        let mut save = HeavyCombatSave::default();
        let p1 = save.player_mut("local:p1");
        p1.melee.whip.special_unlocked = true;
        p1.elementals.psychic = 99;
        p1.primary_weapons.rifle = 0;
        p1.primary_weapons.capture_net = 5;
        p1.armor_capsule
            .applied_upgrade_ids
            .insert("future_capsule_upgrade".to_owned());
        save.players
            .insert("   ".to_owned(), HeavyCombatPlayerRecord::default());
        save.normalize();

        assert!(save.player("   ").is_none());
        let p1 = save.player("local:p1").unwrap();
        assert!(p1.melee.whip.unlocked);
        assert_eq!(p1.elementals.psychic, 20);
        assert_eq!(p1.primary_weapons.rifle, 1);
        assert_eq!(p1.primary_weapons.capture_net, 1);
        assert!(p1
            .armor_capsule
            .applied_upgrade_ids
            .contains("future_capsule_upgrade"));

        let json = serde_json::to_string(&save).unwrap();
        assert!(!json.contains("cooldown"));
        assert!(!json.contains("selected"));
        assert!(!json.contains("equipped"));
        let decoded: HeavyCombatSave = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, save);
    }

    #[test]
    fn empty_owner_never_creates_a_record_or_charges() {
        let mut save = HeavyCombatSave::default();
        let mut balances = rich_balances();
        let before = balances.clone();
        assert_eq!(
            save.purchase_capsule_upgrade(" ", &mut balances, ArmorCapsuleUpgradeId::FlightArmor,),
            Err(CombatPurchaseError::EmptyOwnerKey)
        );
        assert!(save.players.is_empty());
        assert_eq!(balances, before);
    }
}
