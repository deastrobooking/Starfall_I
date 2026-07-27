//! PX3 shop transaction contract.
//!
//! Pure ownership + transaction rules shared by the pause-menu shop today and
//! intended for adoption by tech upgrades, perk training, and the Robot Garage
//! later. UI code renders card states from [`card_state`] and executes actions
//! through [`buy`] / [`equip`] / [`unequip`]; every protection (insufficient
//! funds, duplicate purchase, locked category) is enforced here regardless of
//! what the UI allows.
//!
//! Ownership policy: Outfits, Armor, and Weapons are owned per player (stored
//! in that player's [`crate::components::player::PlayerProgression`] and saved
//! per `PlayerIndex`). Vehicles are party-shared by campaign policy and are
//! built from parts in the Robot Garage, so the shop lists them as locked
//! showcase entries rather than selling a parallel copy.

use serde::{Deserialize, Serialize};

use crate::resources::{ShopCategory, ShopItem};

/// Per-player shop ownership: purchased item ids plus the equipped item per
/// equippable category. Persisted inside the per-player save record with
/// `serde(default)` so legacy saves load as "nothing owned yet".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopOwnership {
    #[serde(default)]
    pub owned: Vec<String>,
    #[serde(default)]
    pub equipped_outfit: Option<String>,
    #[serde(default)]
    pub equipped_armor: Option<String>,
    #[serde(default)]
    pub equipped_weapon: Option<String>,
}

impl ShopOwnership {
    pub fn owns(&self, item_id: &str) -> bool {
        self.owned.iter().any(|id| id == item_id)
    }

    pub fn equipped_for(&self, category: ShopCategory) -> Option<&str> {
        match category {
            ShopCategory::Outfits => self.equipped_outfit.as_deref(),
            ShopCategory::Armor => self.equipped_armor.as_deref(),
            ShopCategory::Weapons => self.equipped_weapon.as_deref(),
            ShopCategory::Vehicles => None,
        }
    }

    fn equipped_slot_mut(&mut self, category: ShopCategory) -> Option<&mut Option<String>> {
        match category {
            ShopCategory::Outfits => Some(&mut self.equipped_outfit),
            ShopCategory::Armor => Some(&mut self.equipped_armor),
            ShopCategory::Weapons => Some(&mut self.equipped_weapon),
            ShopCategory::Vehicles => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopTransactionError {
    /// The buyer's credits cannot cover the price.
    InsufficientFunds { price: u32, credits: u32 },
    /// The buyer already owns this item.
    AlreadyOwned,
    /// The item's category cannot be bought or equipped here.
    Locked,
    /// Equip was requested for an item the player does not own.
    NotOwned,
}

impl ShopTransactionError {
    /// Kid-friendly message for the pause-menu toast line.
    pub fn message(self) -> String {
        match self {
            Self::InsufficientFunds { price, credits } => {
                format!("Not enough credits yet: need {price}, have {credits}.")
            }
            Self::AlreadyOwned => "Already in your collection!".to_string(),
            Self::Locked => "Vehicle frames are built in the Robot Garage.".to_string(),
            Self::NotOwned => "Buy it first to equip it.".to_string(),
        }
    }
}

/// Whether a category is sold through the shop at all. Vehicles are
/// party-shared and assembled in the Robot Garage, so they stay locked here.
pub fn category_locked(category: ShopCategory) -> bool {
    matches!(category, ShopCategory::Vehicles)
}

/// Display state for one item card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    Equipped,
    Owned,
    Locked,
    Affordable,
    Unaffordable,
}

impl CardState {
    pub fn badge(self) -> &'static str {
        match self {
            Self::Equipped => "EQUIPPED",
            Self::Owned => "OWNED",
            Self::Locked => "GARAGE",
            Self::Affordable => "BUY",
            Self::Unaffordable => "SAVE UP",
        }
    }
}

pub fn card_state(ownership: &ShopOwnership, item: &ShopItem, credits: u32) -> CardState {
    if ownership.equipped_for(item.category) == Some(item.id) {
        CardState::Equipped
    } else if ownership.owns(item.id) {
        CardState::Owned
    } else if category_locked(item.category) {
        CardState::Locked
    } else if credits >= item.price_credits {
        CardState::Affordable
    } else {
        CardState::Unaffordable
    }
}

/// Purchase `item` for the player, debiting `credits` on success.
pub fn buy(
    ownership: &mut ShopOwnership,
    credits: &mut u32,
    item: &ShopItem,
) -> Result<(), ShopTransactionError> {
    if category_locked(item.category) {
        return Err(ShopTransactionError::Locked);
    }
    if ownership.owns(item.id) {
        return Err(ShopTransactionError::AlreadyOwned);
    }
    if *credits < item.price_credits {
        return Err(ShopTransactionError::InsufficientFunds {
            price: item.price_credits,
            credits: *credits,
        });
    }
    *credits -= item.price_credits;
    ownership.owned.push(item.id.to_string());
    Ok(())
}

/// Equip an owned item into its category slot, replacing any previous choice.
pub fn equip(ownership: &mut ShopOwnership, item: &ShopItem) -> Result<(), ShopTransactionError> {
    if !ownership.owns(item.id) {
        return Err(ShopTransactionError::NotOwned);
    }
    let Some(slot) = ownership.equipped_slot_mut(item.category) else {
        return Err(ShopTransactionError::Locked);
    };
    *slot = Some(item.id.to_string());
    Ok(())
}

/// Clear the equipped slot for the item's category.
pub fn unequip(ownership: &mut ShopOwnership, item: &ShopItem) -> Result<(), ShopTransactionError> {
    let Some(slot) = ownership.equipped_slot_mut(item.category) else {
        return Err(ShopTransactionError::Locked);
    };
    if slot.as_deref() != Some(item.id) {
        return Err(ShopTransactionError::NotOwned);
    }
    *slot = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::ShopCatalog;

    fn item(category: ShopCategory, price: u32) -> ShopItem {
        ShopItem::new(
            "test_item",
            "Test Item",
            category,
            "A test item.",
            price,
            None,
        )
    }

    #[test]
    fn buy_rejects_insufficient_funds_and_leaves_state_untouched() {
        let mut ownership = ShopOwnership::default();
        let mut credits = 50;
        let outfit = item(ShopCategory::Outfits, 120);
        assert_eq!(
            buy(&mut ownership, &mut credits, &outfit),
            Err(ShopTransactionError::InsufficientFunds {
                price: 120,
                credits: 50,
            })
        );
        assert_eq!(credits, 50);
        assert!(ownership.owned.is_empty());
    }

    #[test]
    fn buy_rejects_duplicate_purchase_without_double_charge() {
        let mut ownership = ShopOwnership::default();
        let mut credits = 300;
        let outfit = item(ShopCategory::Outfits, 120);
        assert_eq!(buy(&mut ownership, &mut credits, &outfit), Ok(()));
        assert_eq!(credits, 180);
        assert_eq!(
            buy(&mut ownership, &mut credits, &outfit),
            Err(ShopTransactionError::AlreadyOwned)
        );
        assert_eq!(credits, 180);
        assert_eq!(ownership.owned.len(), 1);
    }

    #[test]
    fn vehicles_are_locked_party_shared_showcase_entries() {
        let mut ownership = ShopOwnership::default();
        let mut credits = 10_000;
        let frame = item(ShopCategory::Vehicles, 900);
        assert_eq!(
            buy(&mut ownership, &mut credits, &frame),
            Err(ShopTransactionError::Locked)
        );
        assert_eq!(card_state(&ownership, &frame, credits), CardState::Locked);
    }

    #[test]
    fn equip_requires_ownership() {
        let mut ownership = ShopOwnership::default();
        let armor = item(ShopCategory::Armor, 80);
        assert_eq!(
            equip(&mut ownership, &armor),
            Err(ShopTransactionError::NotOwned)
        );
    }

    #[test]
    fn buy_equip_unequip_cycle_updates_card_state() {
        let mut ownership = ShopOwnership::default();
        let mut credits = 100;
        let armor = item(ShopCategory::Armor, 80);

        assert_eq!(
            card_state(&ownership, &armor, credits),
            CardState::Affordable
        );
        assert_eq!(buy(&mut ownership, &mut credits, &armor), Ok(()));
        assert_eq!(credits, 20);
        assert_eq!(card_state(&ownership, &armor, credits), CardState::Owned);

        assert_eq!(equip(&mut ownership, &armor), Ok(()));
        assert_eq!(
            ownership.equipped_for(ShopCategory::Armor),
            Some("test_item")
        );
        assert_eq!(card_state(&ownership, &armor, credits), CardState::Equipped);

        assert_eq!(unequip(&mut ownership, &armor), Ok(()));
        assert_eq!(ownership.equipped_for(ShopCategory::Armor), None);
        assert_eq!(card_state(&ownership, &armor, credits), CardState::Owned);
        assert_eq!(
            unequip(&mut ownership, &armor),
            Err(ShopTransactionError::NotOwned)
        );
    }

    #[test]
    fn ownership_serde_round_trips_and_defaults_for_legacy_records() {
        let mut ownership = ShopOwnership {
            owned: vec!["scout_visor".to_string()],
            equipped_outfit: Some("scout_visor".to_string()),
            ..Default::default()
        };
        ownership.owned.push("nova_plating".to_string());
        let json = serde_json::to_string(&ownership).unwrap();
        let back: ShopOwnership = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ownership);

        // Legacy player records carry no shop data at all.
        let legacy: ShopOwnership = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy, ShopOwnership::default());
    }

    #[test]
    fn every_shop_blade_id_resolves_to_a_real_blade() {
        use crate::combat::blades::{blade_for_id, BLADE_CATALOG, STARTER_BLADE};
        let catalog = crate::resources::ShopCatalog::default();

        // Every sellable blade must appear in the shop, or it is unbuyable.
        for blade in BLADE_CATALOG.iter().filter(|b| b.id != STARTER_BLADE.id) {
            assert!(
                catalog.items.iter().any(|item| item.id == blade.id),
                "{} is in the blade catalog but not for sale",
                blade.name
            );
        }

        // And every shop weapon that names a blade must resolve to it — a
        // typo would silently sell the starter blade at full price.
        for item in catalog
            .items
            .iter()
            .filter(|i| i.category == crate::resources::ShopCategory::Weapons)
        {
            if let Some(blade) = BLADE_CATALOG.iter().find(|b| b.id == item.id) {
                assert_eq!(blade_for_id(Some(item.id)).id, item.id);
                assert_eq!(
                    blade.price_credits, item.price_credits,
                    "{} price differs between the shop and the blade catalog",
                    item.name
                );
            }
        }
    }

    #[test]
    fn buying_and_equipping_a_blade_changes_what_the_sabre_resolves_to() {
        use crate::combat::blades::blade_for_id;
        let catalog = crate::resources::ShopCatalog::default();
        let item = catalog
            .items
            .iter()
            .find(|i| i.id == "weapon_crimson_edge")
            .expect("crimson edge is sold");

        let mut owned = ShopOwnership::default();
        // Nothing equipped resolves to the neutral, never-sold starter blade.
        assert_eq!(
            blade_for_id(owned.equipped_weapon.as_deref()).id,
            "blade_standard_issue"
        );

        let mut credits = item.price_credits + 100;
        buy(&mut owned, &mut credits, item).expect("purchase succeeds");
        equip(&mut owned, item).expect("equip succeeds");

        assert_eq!(
            blade_for_id(owned.equipped_weapon.as_deref()).id,
            "weapon_crimson_edge",
            "equipping must change the blade the sabre resolves"
        );
        assert_eq!(credits, 100, "exactly the listed price was charged");
    }

    #[test]
    fn default_catalog_ids_are_unique_and_lists_are_deterministic() {
        let catalog = ShopCatalog::default();
        let mut ids: Vec<_> = catalog.items.iter().map(|i| i.id).collect();
        let first_pass: Vec<_> = catalog
            .items_for(ShopCategory::Outfits)
            .map(|i| i.id)
            .collect();
        let second_pass: Vec<_> = catalog
            .items_for(ShopCategory::Outfits)
            .map(|i| i.id)
            .collect();
        assert_eq!(first_pass, second_pass);
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "catalog item ids must be unique");
    }
}
