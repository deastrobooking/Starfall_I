use bevy::prelude::*;

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
    vec![
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
    ]
}

// ── Inventory Slot ────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct InventorySlot {
    pub item_id: String,
    pub quantity: u32,
}

// ── Inventory Component ───────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct Inventory {
    pub slots: Vec<Option<InventorySlot>>,
    pub max_slots: usize,
}

impl Default for Inventory {
    fn default() -> Self {
        let max = 24;
        Self {
            slots: vec![None; max],
            max_slots: max,
        }
    }
}

impl Inventory {
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
