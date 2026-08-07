#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ── Item Types ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemType {
    Weapon,
    Armor,
    Consumable,
    Ammo,
    KeyItem,
    Currency,
    Material,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemRarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

// ── Item Definition ───────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct ItemDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub item_type: ItemType,
    pub rarity: ItemRarity,
    pub max_stack: u32,
    pub value: u32,
    pub description: &'static str,
}

// All pre-defined items
pub fn all_items() -> Vec<ItemDefinition> {
    let mut items = vec![
        ItemDefinition {
            id: "credits",
            name: "Credits",
            item_type: ItemType::Currency,
            rarity: ItemRarity::Common,
            max_stack: 9999,
            value: 1,
            description: "Standard currency.",
        },
        ItemDefinition {
            id: "health_pack",
            name: "Health Pack",
            item_type: ItemType::Consumable,
            rarity: ItemRarity::Common,
            max_stack: 10,
            value: 50,
            description: "Heals 50 HP.",
        },
        ItemDefinition {
            id: "armor_shard",
            name: "Armor Shard",
            item_type: ItemType::Consumable,
            rarity: ItemRarity::Common,
            max_stack: 10,
            value: 30,
            description: "Restores 25 armor.",
        },
        ItemDefinition {
            id: "plasma_cell",
            name: "Star Cell",
            item_type: ItemType::Ammo,
            rarity: ItemRarity::Common,
            max_stack: 200,
            value: 2,
            description: "Star-beam energy.",
        },
        ItemDefinition {
            id: "kinetic_rounds",
            name: "Comet Sparks",
            item_type: ItemType::Ammo,
            rarity: ItemRarity::Common,
            max_stack: 200,
            value: 1,
            description: "Tiny comet charges.",
        },
        ItemDefinition {
            id: "rocket_ammo",
            name: "Nova Orbs",
            item_type: ItemType::Ammo,
            rarity: ItemRarity::Uncommon,
            max_stack: 20,
            value: 15,
            description: "Arcing nova charges.",
        },
        ItemDefinition {
            id: "grenade_pack",
            name: "Star Bubbles",
            item_type: ItemType::Ammo,
            rarity: ItemRarity::Uncommon,
            max_stack: 12,
            value: 20,
            description: "Bouncy energy bubbles.",
        },
        ItemDefinition {
            id: "shield_booster",
            name: "Shield Booster",
            item_type: ItemType::Consumable,
            rarity: ItemRarity::Rare,
            max_stack: 5,
            value: 100,
            description: "Doubles armor temporarily.",
        },
        ItemDefinition {
            id: "damage_amp",
            name: "Damage Amp",
            item_type: ItemType::Consumable,
            rarity: ItemRarity::Rare,
            max_stack: 5,
            value: 120,
            description: "+50% damage for 20s.",
        },
        ItemDefinition {
            id: "xp_chip",
            name: "XP Chip",
            item_type: ItemType::Consumable,
            rarity: ItemRarity::Common,
            max_stack: 50,
            value: 25,
            description: "Grants 25 XP.",
        },
        ItemDefinition {
            id: "gear",
            name: "Gear",
            item_type: ItemType::Material,
            rarity: ItemRarity::Common,
            max_stack: 999,
            value: 8,
            description: "Universal upgrade currency salvaged from machines.",
        },
        ItemDefinition {
            id: "bio_essence",
            name: "Bio Essence",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 99,
            value: 30,
            description: "Organic residue used to capture and care for bio-creatures.",
        },
        ItemDefinition {
            id: "weapon_part_pistol",
            name: "Pistol Part",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 99,
            value: 18,
            description: "Salvaged star-pistol component.",
        },
        ItemDefinition {
            id: "weapon_part_rifle",
            name: "Rifle Part",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 99,
            value: 22,
            description: "Salvaged pulse-rifle component.",
        },
        ItemDefinition {
            id: "weapon_part_shotgun",
            name: "Shotgun Part",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 99,
            value: 22,
            description: "Salvaged scatter-blaster component.",
        },
        ItemDefinition {
            id: "weapon_part_rocket",
            name: "Rocket Part",
            item_type: ItemType::Material,
            rarity: ItemRarity::Rare,
            max_stack: 50,
            value: 35,
            description: "Salvaged nova-launcher component.",
        },
        ItemDefinition {
            id: "weapon_part_laser",
            name: "Laser Part",
            item_type: ItemType::Material,
            rarity: ItemRarity::Rare,
            max_stack: 50,
            value: 35,
            description: "Salvaged photon-beam component.",
        },
        ItemDefinition {
            id: "weapon_part_grenade",
            name: "Grenade Part",
            item_type: ItemType::Material,
            rarity: ItemRarity::Rare,
            max_stack: 50,
            value: 32,
            description: "Salvaged fusion-grenade component.",
        },
        ItemDefinition {
            id: "bio_seed",
            name: "Bio Seed",
            item_type: ItemType::Material,
            rarity: ItemRarity::Common,
            max_stack: 99,
            value: 6,
            description: "Engineered seed for a sanctuary garden plot.",
        },
        ItemDefinition {
            id: "bio_crop",
            name: "Bio Crop",
            item_type: ItemType::Consumable,
            rarity: ItemRarity::Uncommon,
            max_stack: 99,
            value: 18,
            description: "Sanctuary harvest that can be eaten or refined into feed.",
        },
        ItemDefinition {
            id: "animaton_feed",
            name: "Animaton Feed",
            item_type: ItemType::Consumable,
            rarity: ItemRarity::Rare,
            max_stack: 50,
            value: 60,
            description: "Refined bio-crop blend that strengthens a companion.",
        },
        ItemDefinition {
            id: "power_jewel_rough",
            name: "Rough Power Jewel",
            item_type: ItemType::Material,
            rarity: ItemRarity::Epic,
            max_stack: 9,
            value: 600,
            description: "Mount on a ranged weapon for 15% more damage.",
        },
        ItemDefinition {
            id: "power_jewel_cut",
            name: "Cut Power Jewel",
            item_type: ItemType::Material,
            rarity: ItemRarity::Legendary,
            max_stack: 9,
            value: 1_500,
            description: "Mount on a ranged weapon for 30% more damage.",
        },
        ItemDefinition {
            id: "power_jewel_flawless",
            name: "Flawless Power Jewel",
            item_type: ItemType::Material,
            rarity: ItemRarity::Legendary,
            max_stack: 9,
            value: 4_000,
            description: "Mount on a ranged weapon for 50% more damage.",
        },
        // Crafting materials
        ItemDefinition {
            id: "scrap_metal",
            name: "Scrap Metal",
            item_type: ItemType::Material,
            rarity: ItemRarity::Common,
            max_stack: 99,
            value: 5,
            description: "Common material for frames and armor shells.",
        },
        ItemDefinition {
            id: "energy_core",
            name: "Energy Core",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 50,
            value: 25,
            description: "Compact power source for weapons and robot pets.",
        },
        ItemDefinition {
            id: "nano_fiber",
            name: "Nano Fiber",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 50,
            value: 20,
            description: "Uncommon material.",
        },
        ItemDefinition {
            id: "circuit_board",
            name: "Circuit Board",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 50,
            value: 30,
            description: "Logic board for tools, drones, and robot pets.",
        },
        ItemDefinition {
            id: "robot_scrap_frame",
            name: "Robot Scrap Frame",
            item_type: ItemType::Material,
            rarity: ItemRarity::Common,
            max_stack: 99,
            value: 8,
            description: "Frame stock for building robot pets and vehicles.",
        },
        ItemDefinition {
            id: "robot_servo_motor",
            name: "Servo Motor",
            item_type: ItemType::Material,
            rarity: ItemRarity::Common,
            max_stack: 99,
            value: 12,
            description: "Joint motor for pets, cars, motorcycles, and mechs.",
        },
        ItemDefinition {
            id: "robot_circuit_board",
            name: "Robot Circuit Board",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 50,
            value: 30,
            description: "Control board for robot pet behaviors.",
        },
        ItemDefinition {
            id: "robot_energy_core",
            name: "Robot Energy Core",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 50,
            value: 35,
            description: "Stable power cell for combined forms.",
        },
        ItemDefinition {
            id: "robot_hover_jet",
            name: "Hover Jet",
            item_type: ItemType::Material,
            rarity: ItemRarity::Rare,
            max_stack: 30,
            value: 70,
            description: "Lift module for jets, ships, and megaships.",
        },
        ItemDefinition {
            id: "robot_tread_unit",
            name: "Tread Unit",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 50,
            value: 28,
            description: "Heavy traction part for tanks and rugged builds.",
        },
        ItemDefinition {
            id: "robot_hull_plate",
            name: "Hull Plate",
            item_type: ItemType::Material,
            rarity: ItemRarity::Uncommon,
            max_stack: 50,
            value: 32,
            description: "Sealed plating for boats, submarines, and ships.",
        },
        ItemDefinition {
            id: "robot_pressure_seal",
            name: "Pressure Seal",
            item_type: ItemType::Material,
            rarity: ItemRarity::Rare,
            max_stack: 30,
            value: 65,
            description: "Deep-water seal for submarine forms.",
        },
        ItemDefinition {
            id: "robot_star_drive",
            name: "Star Drive",
            item_type: ItemType::Material,
            rarity: ItemRarity::Epic,
            max_stack: 20,
            value: 150,
            description: "Rare propulsion heart for space-capable builds.",
        },
        ItemDefinition {
            id: "robot_command_deck",
            name: "Command Deck",
            item_type: ItemType::Material,
            rarity: ItemRarity::Epic,
            max_stack: 10,
            value: 180,
            description: "Bridge module for large ships and megaships.",
        },
        ItemDefinition {
            id: "bio_sample",
            name: "Bio Sample",
            item_type: ItemType::Material,
            rarity: ItemRarity::Rare,
            max_stack: 30,
            value: 40,
            description: "Rare material.",
        },
        ItemDefinition {
            id: "crystal_shard",
            name: "Crystal Shard",
            item_type: ItemType::Material,
            rarity: ItemRarity::Rare,
            max_stack: 30,
            value: 50,
            description: "Rare material.",
        },
        ItemDefinition {
            id: "dark_matter",
            name: "Dark Matter",
            item_type: ItemType::Material,
            rarity: ItemRarity::Legendary,
            max_stack: 10,
            value: 200,
            description: "Legendary material.",
        },
    ];

    // The world-content catalog carries the complete Heavy Water crafting,
    // vendor, building, farming, and jewel result set. Native Starfall entries
    // above win on names/descriptions; only missing stable IDs are appended.
    for ported in crate::world::heavy_economy::ITEM_CATALOG {
        if items.iter().any(|item| item.id == ported.id) {
            continue;
        }
        use crate::world::heavy_economy::{ItemKind as PortedKind, ItemRarity as PortedRarity};
        let item_type = match ported.kind {
            PortedKind::Weapon => ItemType::Weapon,
            PortedKind::Armor => ItemType::Armor,
            PortedKind::Consumable => ItemType::Consumable,
            PortedKind::Ammo => ItemType::Ammo,
            PortedKind::KeyItem => ItemType::KeyItem,
            PortedKind::Currency => ItemType::Currency,
            PortedKind::Material => ItemType::Material,
        };
        let rarity = match ported.rarity {
            PortedRarity::Common => ItemRarity::Common,
            PortedRarity::Uncommon => ItemRarity::Uncommon,
            PortedRarity::Rare => ItemRarity::Rare,
            PortedRarity::Epic => ItemRarity::Epic,
            PortedRarity::Legendary => ItemRarity::Legendary,
        };
        items.push(ItemDefinition {
            id: ported.id,
            name: ported.name,
            item_type,
            rarity,
            max_stack: ported.max_stack,
            value: ported.value,
            description: "Heavy Water continuity item.",
        });
    }
    items
}

// ── Inventory Slot ────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventorySlot {
    pub item_id: String,
    pub quantity: u32,
}

// ── Inventory Component ───────────────────────────────────────────────────────
#[derive(Component, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Inventory {
    pub slots: Vec<Option<InventorySlot>>,
    pub max_slots: usize,
}

/// Per-player item equipped for future one-button use. The loadout menu owns
/// selection; storing the stable item ID keeps the choice independent of slot
/// compaction as stacks are consumed or rearranged.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickItemSlot {
    pub item_id: Option<String>,
}

impl Default for Inventory {
    fn default() -> Self {
        // Heavy Water deliberately raised its slot ceiling from 24 to 100 so
        // very rare jewels could never be silently lost to an otherwise full
        // bag. Starfall keeps that player-facing guarantee while retaining
        // per-item stack limits.
        let max = 100;
        Self {
            slots: vec![None; max],
            max_slots: max,
        }
    }
}

impl Inventory {
    /// Expand legacy inventories without compacting or reordering their slots.
    /// Save records created before the Heavy Water port commonly contain 24
    /// slots; hydration calls this before any rare rewards can be granted.
    pub fn ensure_capacity(&mut self, minimum_slots: usize) {
        let target = self.max_slots.max(self.slots.len()).max(minimum_slots);
        self.slots.resize(target, None);
        self.max_slots = target;
    }

    /// Add items. Returns leftover quantity that didn't fit.
    pub fn add_item(&mut self, item_id: &str, quantity: u32, max_stack: u32) -> u32 {
        let mut remaining = quantity;

        // Try stacking onto existing slots
        for slot in self.slots.iter_mut().flatten() {
            if slot.item_id == item_id && slot.quantity < max_stack {
                let space = max_stack - slot.quantity;
                let add = remaining.min(space);
                slot.quantity += add;
                remaining -= add;
                if remaining == 0 {
                    return 0;
                }
            }
        }

        // Fill empty slots
        while remaining > 0 {
            if let Some(empty) = self.slots.iter_mut().find(|s| s.is_none()) {
                let add = remaining.min(max_stack);
                *empty = Some(InventorySlot {
                    item_id: item_id.to_string(),
                    quantity: add,
                });
                remaining -= add;
            } else {
                break; // inventory full
            }
        }
        remaining
    }

    pub fn remove_item(&mut self, item_id: &str, quantity: u32) -> bool {
        if self.count(item_id) < quantity {
            return false;
        }
        let mut to_remove = quantity;
        for slot in self.slots.iter_mut() {
            if let Some(s) = slot {
                if s.item_id == item_id {
                    let remove = to_remove.min(s.quantity);
                    s.quantity -= remove;
                    to_remove -= remove;
                    if s.quantity == 0 {
                        *slot = None;
                    }
                    if to_remove == 0 {
                        break;
                    }
                }
            }
        }
        true
    }

    pub fn count(&self, item_id: &str) -> u32 {
        self.slots
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|s| s.item_id == item_id)
            .map(|s| s.quantity)
            .sum()
    }

    pub fn has(&self, item_id: &str, quantity: u32) -> bool {
        self.count(item_id) >= quantity
    }

    pub fn is_full(&self) -> bool {
        self.slots.iter().all(|s| s.is_some())
    }
}

pub fn item_definition(item_id: &str) -> Option<ItemDefinition> {
    all_items().into_iter().find(|item| item.id == item_id)
}

pub fn max_stack_for(item_id: &str) -> u32 {
    item_definition(item_id).map_or(99, |item| item.max_stack)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_inventory_has_heavy_water_rare_drop_capacity() {
        let inventory = Inventory::default();
        assert_eq!(inventory.max_slots, 100);
        assert_eq!(inventory.slots.len(), 100);
    }

    #[test]
    fn legacy_inventory_expands_without_moving_existing_stacks() {
        let mut inventory = Inventory {
            slots: vec![
                Some(InventorySlot {
                    item_id: "power_jewel_flawless".to_string(),
                    quantity: 1,
                });
                24
            ],
            max_slots: 24,
        };

        inventory.ensure_capacity(100);

        assert_eq!(inventory.max_slots, 100);
        assert_eq!(inventory.slots.len(), 100);
        assert_eq!(
            inventory.slots[0]
                .as_ref()
                .map(|slot| slot.item_id.as_str()),
            Some("power_jewel_flawless")
        );
        assert!(inventory.slots[24..].iter().all(Option::is_none));
    }

    #[test]
    fn heavy_water_material_catalog_ids_are_unique() {
        let items = all_items();
        let mut ids = std::collections::HashSet::new();
        assert!(items.iter().all(|item| ids.insert(item.id)));
        for item_id in [
            "gear",
            "bio_essence",
            "bio_seed",
            "bio_crop",
            "animaton_feed",
            "power_jewel_rough",
            "power_jewel_cut",
            "power_jewel_flawless",
        ] {
            assert!(item_definition(item_id).is_some(), "missing {item_id}");
        }
    }
}
