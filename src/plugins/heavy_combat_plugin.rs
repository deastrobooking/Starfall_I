//! Runtime boundary for Heavy Water continuity combat purchases and cooldowns.
//!
//! Durable unlocks live in [`HeavyWaterProgress`], while Starfall's
//! [`Inventory`] and [`PlayerStats`] remain the only material/wallet authority.
//! Every purchase below stages all three values and commits them together.

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::components::inventory::Inventory;
use crate::components::player::PlayerStats;
use crate::engine::state::AppState;
use crate::world::heavy_combat::{
    ArmorCapsuleUnlockEffect, ArmorCapsuleUpgradeId, CombatBalances, CombatCost,
    CombatPurchaseError, ElementalKind, ElementalUpgradeEffect, HeavyCombatRuntime,
    HeavyCombatSave, MeleeArsenalId, MeleeUnlockEffect, MeleeUnlockTier, PrimaryWeaponKind,
    PrimaryWeaponUpgradeEffect,
};

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct HeavyCombatRuntimeBridge {
    pub runtime: HeavyCombatRuntime,
}

pub struct HeavyCombatPlugin;

impl Plugin for HeavyCombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeavyCombatRuntimeBridge>().add_systems(
            Update,
            advance_heavy_combat_runtime.run_if(in_state(AppState::Playing)),
        );
    }
}

fn advance_heavy_combat_runtime(time: Res<Time>, mut runtime: ResMut<HeavyCombatRuntimeBridge>) {
    runtime.runtime.advance(time.delta());
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeavyCombatTransactionError {
    Domain(CombatPurchaseError),
    MissingCanonicalItem { item_id: String, quantity: u32 },
    CanonicalCreditCostOutOfRange { credits: u64 },
    CanonicalBalanceMismatch,
}

impl std::fmt::Display for HeavyCombatTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Domain(error) => write!(formatter, "Heavy combat purchase failed: {error:?}"),
            Self::MissingCanonicalItem { item_id, quantity } => {
                write!(
                    formatter,
                    "canonical inventory is missing {quantity} of `{item_id}`"
                )
            }
            Self::CanonicalCreditCostOutOfRange { credits } => write!(
                formatter,
                "combat credit cost {credits} exceeds Starfall's wallet range"
            ),
            Self::CanonicalBalanceMismatch => formatter.write_str(
                "canonical wallet/inventory diverged from the staged Heavy combat balance",
            ),
        }
    }
}

impl std::error::Error for HeavyCombatTransactionError {}

impl From<CombatPurchaseError> for HeavyCombatTransactionError {
    fn from(error: CombatPurchaseError) -> Self {
        Self::Domain(error)
    }
}

/// Projects the live canonical wallet and stacks without persisting a second
/// inventory. Duplicate item stacks are deliberately coalesced.
pub fn canonical_combat_balances(inventory: &Inventory, stats: &PlayerStats) -> CombatBalances {
    let mut items = BTreeMap::<String, u32>::new();
    for stack in inventory.slots.iter().flatten() {
        let total = items
            .get(&stack.item_id)
            .copied()
            .unwrap_or_default()
            .saturating_add(stack.quantity);
        items.insert(stack.item_id.clone(), total);
    }
    CombatBalances {
        credits: u64::from(stats.credits),
        items,
    }
}

fn run_purchase_atomic<T>(
    save: &mut HeavyCombatSave,
    inventory: &mut Inventory,
    stats: &mut PlayerStats,
    purchase: impl FnOnce(&mut HeavyCombatSave, &mut CombatBalances) -> Result<T, CombatPurchaseError>,
    cost: impl FnOnce(&T) -> &CombatCost,
) -> Result<T, HeavyCombatTransactionError> {
    let mut staged_save = save.clone();
    let mut staged_balances = canonical_combat_balances(inventory, stats);
    let outcome = purchase(&mut staged_save, &mut staged_balances)?;
    let purchase_cost = cost(&outcome);

    let mut staged_inventory = inventory.clone();
    for item in &purchase_cost.items {
        if !staged_inventory.remove_item(&item.item_id, item.quantity) {
            return Err(HeavyCombatTransactionError::MissingCanonicalItem {
                item_id: item.item_id.clone(),
                quantity: item.quantity,
            });
        }
    }
    let credit_cost = u32::try_from(purchase_cost.credits).map_err(|_| {
        HeavyCombatTransactionError::CanonicalCreditCostOutOfRange {
            credits: purchase_cost.credits,
        }
    })?;
    let Some(staged_credits) = stats.credits.checked_sub(credit_cost) else {
        return Err(HeavyCombatTransactionError::CanonicalCreditCostOutOfRange {
            credits: purchase_cost.credits,
        });
    };
    let mut staged_stats = stats.clone();
    staged_stats.credits = staged_credits;
    if canonical_combat_balances(&staged_inventory, &staged_stats) != staged_balances {
        return Err(HeavyCombatTransactionError::CanonicalBalanceMismatch);
    }

    *save = staged_save;
    *inventory = staged_inventory;
    stats.credits = staged_credits;
    Ok(outcome)
}

pub fn purchase_melee_unlock_canonical_atomic(
    save: &mut HeavyCombatSave,
    inventory: &mut Inventory,
    stats: &mut PlayerStats,
    owner_key: &str,
    weapon: MeleeArsenalId,
    tier: MeleeUnlockTier,
) -> Result<MeleeUnlockEffect, HeavyCombatTransactionError> {
    run_purchase_atomic(
        save,
        inventory,
        stats,
        |staged, balances| staged.purchase_melee_unlock(owner_key, balances, weapon, tier),
        |effect| &effect.cost,
    )
}

pub fn purchase_elemental_level_canonical_atomic(
    save: &mut HeavyCombatSave,
    inventory: &mut Inventory,
    stats: &mut PlayerStats,
    owner_key: &str,
    kind: ElementalKind,
) -> Result<ElementalUpgradeEffect, HeavyCombatTransactionError> {
    run_purchase_atomic(
        save,
        inventory,
        stats,
        |staged, balances| staged.purchase_elemental_level(owner_key, balances, kind),
        |effect| &effect.cost,
    )
}

pub fn purchase_primary_weapon_level_canonical_atomic(
    save: &mut HeavyCombatSave,
    inventory: &mut Inventory,
    stats: &mut PlayerStats,
    owner_key: &str,
    weapon: PrimaryWeaponKind,
) -> Result<PrimaryWeaponUpgradeEffect, HeavyCombatTransactionError> {
    run_purchase_atomic(
        save,
        inventory,
        stats,
        |staged, balances| staged.purchase_primary_weapon_level(owner_key, balances, weapon),
        |effect| &effect.cost,
    )
}

pub fn purchase_capsule_upgrade_canonical_atomic(
    save: &mut HeavyCombatSave,
    inventory: &mut Inventory,
    stats: &mut PlayerStats,
    owner_key: &str,
    upgrade: ArmorCapsuleUpgradeId,
) -> Result<ArmorCapsuleUnlockEffect, HeavyCombatTransactionError> {
    run_purchase_atomic(
        save,
        inventory,
        stats,
        |staged, balances| staged.purchase_capsule_upgrade(owner_key, balances, upgrade),
        |effect| &effect.cost,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::inventory::{max_stack_for, InventorySlot};

    fn stocked_inventory() -> Inventory {
        let mut inventory = Inventory::default();
        for (item_id, quantity) in [
            ("gear", 400),
            ("energy_core", 100),
            ("nano_fiber", 100),
            ("circuit_board", 100),
            ("weapon_part_rifle", 20),
        ] {
            assert_eq!(inventory.add_item(item_id, quantity, max_stack_for(item_id)), 0);
        }
        inventory
    }

    #[test]
    fn balance_projection_coalesces_duplicate_stacks() {
        let inventory = Inventory {
            max_slots: 3,
            slots: vec![
                Some(InventorySlot {
                    item_id: "gear".to_owned(),
                    quantity: 8,
                }),
                None,
                Some(InventorySlot {
                    item_id: "gear".to_owned(),
                    quantity: 5,
                }),
            ],
        };
        let stats = PlayerStats {
            credits: 900,
            ..default()
        };
        let balances = canonical_combat_balances(&inventory, &stats);
        assert_eq!(balances.credits, 900);
        assert_eq!(balances.item_count("gear"), 13);
    }

    #[test]
    fn all_purchase_families_debit_canonical_authorities() {
        let mut save = HeavyCombatSave::default();
        let mut inventory = stocked_inventory();
        let mut stats = PlayerStats {
            credits: 10_000,
            ..default()
        };
        let owner = "local:p1";

        let melee = purchase_melee_unlock_canonical_atomic(
            &mut save,
            &mut inventory,
            &mut stats,
            owner,
            MeleeArsenalId::Glaive,
            MeleeUnlockTier::Own,
        )
        .unwrap();
        assert_eq!(
            inventory.count("gear"),
            400 - melee.cost.item_quantity("gear")
        );

        let elemental = purchase_elemental_level_canonical_atomic(
            &mut save,
            &mut inventory,
            &mut stats,
            owner,
            ElementalKind::Lightning,
        )
        .unwrap();
        assert_eq!(stats.credits, 10_000 - elemental.cost.credits as u32);

        let gear_before_primary = inventory.count("gear");
        let primary = purchase_primary_weapon_level_canonical_atomic(
            &mut save,
            &mut inventory,
            &mut stats,
            owner,
            PrimaryWeaponKind::Rifle,
        )
        .unwrap();
        assert_eq!(
            inventory.count("gear"),
            gear_before_primary - primary.cost.item_quantity("gear")
        );

        let credits_before_capsule = stats.credits;
        let capsule = purchase_capsule_upgrade_canonical_atomic(
            &mut save,
            &mut inventory,
            &mut stats,
            owner,
            ArmorCapsuleUpgradeId::SpeedBoost,
        )
        .unwrap();
        assert_eq!(
            stats.credits,
            credits_before_capsule - capsule.cost.credits as u32
        );
        assert!(save
            .player(owner)
            .unwrap()
            .armor_capsule
            .is_applied(ArmorCapsuleUpgradeId::SpeedBoost));
    }

    #[test]
    fn failed_purchase_rolls_back_save_inventory_and_wallet() {
        let mut save = HeavyCombatSave::default();
        let mut inventory = Inventory::default();
        let mut stats = PlayerStats {
            credits: 10_000,
            ..default()
        };
        let save_before = save.clone();
        let inventory_before = inventory.clone();

        assert!(purchase_melee_unlock_canonical_atomic(
            &mut save,
            &mut inventory,
            &mut stats,
            "local:p1",
            MeleeArsenalId::Axe,
            MeleeUnlockTier::Own,
        )
        .is_err());
        assert_eq!(save, save_before);
        assert_eq!(inventory, inventory_before);
        assert_eq!(stats.credits, 10_000);
    }
}
