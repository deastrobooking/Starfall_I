//! Rust-native economy and player-construction domain ported from Heavy Water.
#![allow(dead_code)] // Public port domain; runtime/UI adapters land incrementally.
//!
//! This module deliberately contains no Bevy systems or rendering types.  It is
//! the deterministic, serializable authority that UI, save, world, and pickup
//! adapters can call.  Mutating operations stage changes in clones and commit
//! only after every debit, credit, capacity, ownership, and placement check has
//! succeeded.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const HEAVY_ECONOMY_SCHEMA_VERSION: u16 = 1;
pub const DEFAULT_INVENTORY_SLOTS: u16 = 100;
pub const MAX_BASE_LEVEL: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OwnerId(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Vec3Record {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3Record {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn distance_squared(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }

    pub fn horizontal_distance_squared(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dz = self.z - other.z;
        dx * dx + dz * dz
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Weapon,
    Armor,
    Consumable,
    Ammo,
    KeyItem,
    Currency,
    Material,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemRarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ItemKind,
    pub rarity: ItemRarity,
    pub max_stack: u32,
    pub value: u32,
}

macro_rules! item {
    ($id:literal, $name:literal, $kind:ident, $rarity:ident, $stack:expr, $value:expr) => {
        ItemDefinition {
            id: $id,
            name: $name,
            kind: ItemKind::$kind,
            rarity: ItemRarity::$rarity,
            max_stack: $stack,
            value: $value,
        }
    };
}

/// Combined executable-source item catalog used by inventory, crafting,
/// jewels, vendors, mining, and construction.
pub static ITEM_CATALOG: &[ItemDefinition] = &[
    item!("credits", "Credits", Currency, Common, 9_999, 1),
    item!("health_pack", "Health Pack", Consumable, Common, 10, 25),
    item!("armor_shard", "Armor Shard", Consumable, Common, 10, 30),
    item!("plasma_cell", "Plasma Cell", Ammo, Common, 200, 5),
    item!("kinetic_rounds", "Kinetic Rounds", Ammo, Common, 200, 5),
    item!("rocket_ammo", "Rocket Ammo", Ammo, Uncommon, 20, 50),
    item!("grenade_pack", "Grenade Pack", Ammo, Uncommon, 12, 40),
    item!("shield_booster", "Shield Booster", Consumable, Rare, 5, 100),
    item!("damage_amp", "Damage Amplifier", Consumable, Rare, 5, 120),
    item!("xp_chip", "XP Chip", Material, Uncommon, 50, 15),
    item!("gear", "Gear", Material, Common, 999, 8),
    item!("bio_essence", "Bio Essence", Material, Uncommon, 99, 30),
    item!(
        "weapon_part_pistol",
        "Pistol Part",
        Material,
        Uncommon,
        99,
        18
    ),
    item!(
        "weapon_part_rifle",
        "Rifle Part",
        Material,
        Uncommon,
        99,
        22
    ),
    item!(
        "weapon_part_shotgun",
        "Shotgun Part",
        Material,
        Uncommon,
        99,
        22
    ),
    item!("weapon_part_rocket", "Rocket Part", Material, Rare, 50, 35),
    item!("weapon_part_laser", "Laser Part", Material, Rare, 50, 35),
    item!(
        "weapon_part_grenade",
        "Grenade Part",
        Material,
        Rare,
        50,
        32
    ),
    item!("bio_seed", "Bio Seed", Material, Common, 99, 6),
    item!("bio_crop", "Bio Crop", Consumable, Uncommon, 99, 18),
    item!("animaton_feed", "Animaton Feed", Consumable, Rare, 50, 60),
    item!(
        "power_jewel_rough",
        "Rough Power Jewel",
        Material,
        Epic,
        9,
        600
    ),
    item!(
        "power_jewel_cut",
        "Cut Power Jewel",
        Material,
        Legendary,
        9,
        1_500
    ),
    item!(
        "power_jewel_flawless",
        "Flawless Power Jewel",
        Material,
        Legendary,
        9,
        4_000
    ),
    item!("scrap_metal", "Scrap Metal", Material, Common, 99, 5),
    item!("energy_core", "Energy Core", Material, Uncommon, 50, 25),
    item!("nano_fiber", "Nano Fiber", Material, Uncommon, 50, 20),
    item!("circuit_board", "Circuit Board", Material, Uncommon, 50, 30),
    item!("bio_sample", "Bio Sample", Material, Rare, 30, 40),
    item!("crystal_shard", "Crystal Shard", Material, Rare, 30, 50),
    item!("dark_matter", "Dark Matter", Material, Legendary, 10, 200),
    item!(
        "weapon_mod_scope",
        "Precision Scope",
        Material,
        Uncommon,
        5,
        200
    ),
    item!(
        "weapon_mod_barrel",
        "Extended Barrel",
        Material,
        Uncommon,
        5,
        180
    ),
    item!(
        "armor_plate_basic",
        "Basic Armor Plate",
        Armor,
        Common,
        1,
        150
    ),
    item!(
        "armor_plate_reinforced",
        "Reinforced Armor Plate",
        Armor,
        Uncommon,
        1,
        350
    ),
    item!(
        "energy_shield_module",
        "Energy Shield Module",
        Armor,
        Rare,
        1,
        600
    ),
    item!("damage_mod", "Damage Mod", Material, Common, 10, 50),
    item!("fire_rate_mod", "Fire Rate Mod", Material, Common, 10, 50),
    item!(
        "ammo_capacity_mod",
        "Ammo Capacity Mod",
        Material,
        Common,
        10,
        50
    ),
    item!("basic_helmet", "Basic Helmet", Armor, Common, 1, 50),
    item!("basic_chestplate", "Basic Chestplate", Armor, Common, 1, 50),
    item!("basic_leggings", "Basic Leggings", Armor, Common, 1, 50),
    item!("basic_boots", "Basic Boots", Armor, Common, 1, 50),
    item!("wall_section", "Wall", Material, Common, 10, 50),
    item!("floor_panel", "Floor Panel", Material, Common, 10, 50),
    item!("auto_turret", "Auto-Turret", Material, Common, 10, 50),
    item!(
        "power_generator",
        "Power Generator",
        Material,
        Common,
        10,
        50
    ),
    item!("storage_crate", "Storage Crate", Material, Common, 10, 50),
    item!("medical_bay", "Medical Bay", Material, Common, 10, 50),
    item!(
        "advanced_health_pack",
        "Advanced Health Pack",
        Consumable,
        Common,
        10,
        50
    ),
    item!(
        "shield_battery",
        "Shield Battery",
        Consumable,
        Common,
        10,
        50
    ),
    item!(
        "damage_booster",
        "Damage Booster",
        Consumable,
        Common,
        10,
        50
    ),
    item!(
        "beam_sabre_core",
        "Beam Sabre Core",
        Material,
        Common,
        10,
        50
    ),
    item!(
        "helper_robot_frame",
        "Helper Robot Frame",
        Material,
        Common,
        10,
        50
    ),
    item!(
        "support_drone_kit",
        "Support Drone Kit",
        Material,
        Common,
        10,
        50
    ),
    item!(
        "repair_drone_kit",
        "Repair Drone Kit",
        Material,
        Common,
        10,
        50
    ),
    item!(
        "city_hall_beacon",
        "City Hall Beacon",
        Material,
        Common,
        10,
        50
    ),
    item!("spaceport_core", "Spaceport Core", Material, Common, 10, 50),
    item!(
        "shipyard_scaffold",
        "Shipyard Scaffold",
        Material,
        Common,
        10,
        50
    ),
    item!(
        "comet_fighter_frame",
        "Comet Fighter Frame",
        Material,
        Common,
        10,
        50
    ),
    item!(
        "battleship_spine",
        "Battleship Spine",
        Material,
        Common,
        10,
        50
    ),
];

pub fn item_definition(item_id: &str) -> Option<&'static ItemDefinition> {
    ITEM_CATALOG.iter().find(|item| item.id == item_id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStackRecord {
    pub item_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemQuantity {
    pub item_id: String,
    pub quantity: u32,
}

impl ItemQuantity {
    pub fn new(item_id: impl Into<String>, quantity: u32) -> Self {
        Self {
            item_id: item_id.into(),
            quantity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryRecord {
    pub max_slots: u16,
    pub slots: Vec<Option<ItemStackRecord>>,
}

impl Default for InventoryRecord {
    fn default() -> Self {
        Self::new(DEFAULT_INVENTORY_SLOTS)
    }
}

impl InventoryRecord {
    pub fn new(max_slots: u16) -> Self {
        Self {
            max_slots,
            slots: vec![None; usize::from(max_slots)],
        }
    }

    pub fn occupied_slots(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    pub fn item_count(&self, item_id: &str) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|stack| stack.item_id == item_id)
            .fold(0_u32, |total, stack| total.saturating_add(stack.quantity))
    }

    pub fn validate(&self) -> Result<(), EconomyError> {
        // A zero-slot record is a valid runtime projection when every
        // canonical Starfall slot is occupied by sequel-native, non-Heavy
        // items. It prevents those opaque stacks from becoming spendable or
        // falsely appearing as free Heavy economy capacity.
        if self.slots.len() != usize::from(self.max_slots) {
            return Err(EconomyError::MalformedRecord("inventory slot shape"));
        }
        for stack in self.slots.iter().flatten() {
            let definition = item_definition(&stack.item_id)
                .ok_or_else(|| EconomyError::UnknownItem(stack.item_id.clone()))?;
            if stack.quantity == 0 || stack.quantity > definition.max_stack {
                return Err(EconomyError::MalformedRecord("inventory stack quantity"));
            }
        }
        Ok(())
    }

    pub fn can_add(&self, item_id: &str, quantity: u32) -> Result<bool, EconomyError> {
        let definition = item_definition(item_id)
            .ok_or_else(|| EconomyError::UnknownItem(item_id.to_owned()))?;
        if quantity == 0 {
            return Err(EconomyError::InvalidQuantity);
        }
        let stack_room = self
            .slots
            .iter()
            .flatten()
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| definition.max_stack.saturating_sub(stack.quantity))
            .fold(0_u64, |total, room| total.saturating_add(u64::from(room)));
        let empty_room = self.slots.iter().filter(|slot| slot.is_none()).count() as u64
            * u64::from(definition.max_stack);
        Ok(stack_room.saturating_add(empty_room) >= u64::from(quantity))
    }

    pub fn add_exact(&mut self, item_id: &str, quantity: u32) -> Result<(), EconomyError> {
        let definition = item_definition(item_id)
            .ok_or_else(|| EconomyError::UnknownItem(item_id.to_owned()))?;
        if quantity == 0 {
            return Err(EconomyError::InvalidQuantity);
        }
        if !self.can_add(item_id, quantity)? {
            return Err(EconomyError::InventoryFull {
                item_id: item_id.to_owned(),
                quantity,
            });
        }

        let mut remaining = quantity;
        for stack in self.slots.iter_mut().flatten() {
            if stack.item_id != item_id || stack.quantity >= definition.max_stack {
                continue;
            }
            let amount = remaining.min(definition.max_stack - stack.quantity);
            stack.quantity += amount;
            remaining -= amount;
            if remaining == 0 {
                return Ok(());
            }
        }
        for slot in &mut self.slots {
            if slot.is_some() {
                continue;
            }
            let amount = remaining.min(definition.max_stack);
            *slot = Some(ItemStackRecord {
                item_id: item_id.to_owned(),
                quantity: amount,
            });
            remaining -= amount;
            if remaining == 0 {
                return Ok(());
            }
        }
        Err(EconomyError::MalformedRecord(
            "preflight inventory capacity diverged",
        ))
    }

    pub fn remove_exact(&mut self, item_id: &str, quantity: u32) -> Result<(), EconomyError> {
        if quantity == 0 {
            return Err(EconomyError::InvalidQuantity);
        }
        let available = self.item_count(item_id);
        if available < quantity {
            return Err(EconomyError::InsufficientItem {
                item_id: item_id.to_owned(),
                needed: quantity,
                available,
            });
        }
        let mut remaining = quantity;
        for slot in self.slots.iter_mut().rev() {
            let Some(stack) = slot.as_mut() else {
                continue;
            };
            if stack.item_id != item_id {
                continue;
            }
            let amount = remaining.min(stack.quantity);
            stack.quantity -= amount;
            remaining -= amount;
            if stack.quantity == 0 {
                *slot = None;
            }
            if remaining == 0 {
                return Ok(());
            }
        }
        Err(EconomyError::MalformedRecord(
            "preflight inventory debit diverged",
        ))
    }

    pub fn apply_bundle_atomic(
        &mut self,
        debits: &[ItemQuantity],
        credits: &[ItemQuantity],
    ) -> Result<(), EconomyError> {
        let mut staged = self.clone();
        for debit in debits {
            staged.remove_exact(&debit.item_id, debit.quantity)?;
        }
        for credit in credits {
            staged.add_exact(&credit.item_id, credit.quantity)?;
        }
        staged.validate()?;
        *self = staged;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EconomyError {
    MissingOwner(OwnerId),
    UnknownItem(String),
    UnknownRecipe(String),
    UnknownVendor(String),
    UnknownVendorItem(String),
    UnknownBuild(String),
    UnknownMiningNode(u64),
    UnknownChest(u64),
    InvalidQuantity,
    InvalidDamage,
    MalformedRecord(&'static str),
    InsufficientItem {
        item_id: String,
        needed: u32,
        available: u32,
    },
    InventoryFull {
        item_id: String,
        quantity: u32,
    },
    InsufficientCredits {
        needed: u64,
        available: u64,
    },
    LevelLocked {
        required: u8,
        actual: u8,
    },
    OutOfStock {
        item_id: String,
        requested: u32,
        available: u32,
    },
    NotMountable(String),
    AlreadyMounted,
    NothingMounted,
    AlreadyApplied(u64),
    NotOwner,
    AlreadyClaimed,
    PlacementRejected(PlacementValidationRecord),
    MaxBaseLevel,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub item_id: &'static str,
    pub quantity: u32,
}

impl Cost {
    pub const fn new(item_id: &'static str, quantity: u32) -> Self {
        Self { item_id, quantity }
    }

    fn owned(self) -> ItemQuantity {
        ItemQuantity::new(self.item_id, self.quantity)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeCategory {
    Weapon,
    Armor,
    Base,
    Upgrade,
    Consumable,
    Drone,
    Robot,
    City,
    Ship,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub category: RecipeCategory,
    pub materials: &'static [Cost],
    pub result: Cost,
    pub craft_seconds: u32,
    pub required_level: u8,
}

macro_rules! recipe {
    ($id:literal, $name:literal, $category:ident, [$($mat:literal => $qty:expr),* $(,)?], $result:literal => $result_qty:expr, $seconds:expr, $level:expr) => {
        RecipeDefinition {
            id: $id,
            name: $name,
            category: RecipeCategory::$category,
            materials: &[$(Cost::new($mat, $qty)),*],
            result: Cost::new($result, $result_qty),
            craft_seconds: $seconds,
            required_level: $level,
        }
    };
}

/// All 25 executable recipes from Heavy Water's `CraftingSystem.ts`.
pub static RECIPE_CATALOG: &[RecipeDefinition] = &[
    recipe!("damage_mod", "Damage Mod", Weapon, ["scrap_metal" => 5, "circuit_board" => 2], "damage_mod" => 1, 5, 2),
    recipe!("fire_rate_mod", "Fire Rate Mod", Weapon, ["circuit_board" => 3, "energy_core" => 1], "fire_rate_mod" => 1, 5, 3),
    recipe!("ammo_capacity_mod", "Ammo Capacity Mod", Weapon, ["scrap_metal" => 4, "nano_fiber" => 2], "ammo_capacity_mod" => 1, 4, 2),
    recipe!("basic_helmet", "Basic Helmet", Armor, ["scrap_metal" => 6, "nano_fiber" => 3], "basic_helmet" => 1, 8, 1),
    recipe!("basic_chestplate", "Basic Chestplate", Armor, ["scrap_metal" => 10, "nano_fiber" => 5], "basic_chestplate" => 1, 12, 1),
    recipe!("basic_leggings", "Basic Leggings", Armor, ["scrap_metal" => 8, "nano_fiber" => 4], "basic_leggings" => 1, 10, 1),
    recipe!("basic_boots", "Basic Boots", Armor, ["scrap_metal" => 4, "nano_fiber" => 2], "basic_boots" => 1, 6, 1),
    recipe!("wall_section", "Wall", Base, ["scrap_metal" => 8], "wall_section" => 1, 3, 1),
    recipe!("floor_panel", "Floor Panel", Base, ["scrap_metal" => 6], "floor_panel" => 1, 2, 1),
    recipe!("auto_turret", "Auto-Turret", Base, ["scrap_metal" => 12, "circuit_board" => 4, "energy_core" => 2], "auto_turret" => 1, 15, 5),
    recipe!("power_generator", "Power Generator", Base, ["scrap_metal" => 10, "energy_core" => 3, "circuit_board" => 2], "power_generator" => 1, 12, 3),
    recipe!("storage_crate", "Storage Crate", Base, ["scrap_metal" => 6, "nano_fiber" => 2], "storage_crate" => 1, 4, 1),
    recipe!("medical_bay", "Medical Bay", Base, ["scrap_metal" => 8, "bio_sample" => 5, "circuit_board" => 3], "medical_bay" => 1, 20, 4),
    recipe!("advanced_health_pack", "Advanced Health Pack", Consumable, ["bio_sample" => 3, "nano_fiber" => 1], "advanced_health_pack" => 2, 3, 2),
    recipe!("shield_battery", "Shield Battery", Consumable, ["energy_core" => 2, "crystal_shard" => 1], "shield_battery" => 1, 4, 3),
    recipe!("damage_booster", "Damage Booster", Consumable, ["crystal_shard" => 2, "energy_core" => 1], "damage_booster" => 1, 5, 4),
    recipe!("beam_sabre_core", "Beam Sabre Core", Upgrade, ["crystal_shard" => 5, "dark_matter" => 2, "energy_core" => 3], "beam_sabre_core" => 1, 30, 8),
    recipe!("helper_robot_frame", "Helper Robot Frame", Robot, ["scrap_metal" => 18, "circuit_board" => 4, "energy_core" => 2], "helper_robot_frame" => 1, 18, 4),
    recipe!("support_drone_kit", "Support Drone Kit", Drone, ["scrap_metal" => 14, "circuit_board" => 5, "nano_fiber" => 3], "support_drone_kit" => 1, 16, 4),
    recipe!("repair_drone_kit", "Repair Drone Kit", Drone, ["scrap_metal" => 16, "circuit_board" => 6, "bio_sample" => 3, "energy_core" => 2], "repair_drone_kit" => 1, 20, 5),
    recipe!("city_hall_beacon", "City Hall Beacon", City, ["scrap_metal" => 35, "circuit_board" => 8, "energy_core" => 5], "city_hall_beacon" => 1, 35, 6),
    recipe!("spaceport_core", "Spaceport Core", City, ["scrap_metal" => 60, "circuit_board" => 14, "energy_core" => 10, "crystal_shard" => 4], "spaceport_core" => 1, 55, 8),
    recipe!("shipyard_scaffold", "Shipyard Scaffold", Ship, ["scrap_metal" => 85, "nano_fiber" => 20, "circuit_board" => 18, "energy_core" => 12], "shipyard_scaffold" => 1, 75, 10),
    recipe!("comet_fighter_frame", "Comet Fighter Frame", Ship, ["scrap_metal" => 45, "nano_fiber" => 12, "circuit_board" => 10, "energy_core" => 8], "comet_fighter_frame" => 1, 45, 8),
    recipe!("battleship_spine", "Battleship Spine", Ship, ["scrap_metal" => 150, "nano_fiber" => 35, "circuit_board" => 28, "energy_core" => 24, "dark_matter" => 3], "battleship_spine" => 1, 120, 12),
];

pub fn recipe_definition(recipe_id: &str) -> Option<&'static RecipeDefinition> {
    RECIPE_CATALOG.iter().find(|recipe| recipe.id == recipe_id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CraftReceipt {
    pub transaction_id: u64,
    pub recipe_id: String,
    pub started_at_ms: u64,
    pub completes_at_ms: u64,
    /// Heavy Water grants the result immediately even though it records a
    /// finish time. This flag makes that legacy behavior explicit in saves.
    pub result_granted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JewelTier {
    Rough,
    Cut,
    Flawless,
}

impl JewelTier {
    pub const fn item_id(self) -> &'static str {
        match self {
            Self::Rough => "power_jewel_rough",
            Self::Cut => "power_jewel_cut",
            Self::Flawless => "power_jewel_flawless",
        }
    }

    pub const fn bonus_multiplier(self) -> f32 {
        match self {
            Self::Rough => 0.15,
            Self::Cut => 0.30,
            Self::Flawless => 0.50,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponSocket {
    Pistol,
    Rifle,
    Shotgun,
    Rocket,
    Laser,
    Grenade,
    TrackingMissile,
}

impl WeaponSocket {
    pub const ALL: [Self; 7] = [
        Self::Pistol,
        Self::Rifle,
        Self::Shotgun,
        Self::Rocket,
        Self::Laser,
        Self::Grenade,
        Self::TrackingMissile,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerAccount {
    pub owner: OwnerId,
    pub inventory: InventoryRecord,
    pub credits: u64,
    #[serde(default)]
    pub jewel_mounts: BTreeMap<WeaponSocket, JewelTier>,
    #[serde(default)]
    pub craft_history: Vec<CraftReceipt>,
    #[serde(default)]
    pub applied_transactions: BTreeSet<u64>,
}

impl OwnerAccount {
    pub fn new(owner: OwnerId) -> Self {
        Self {
            owner,
            inventory: InventoryRecord::default(),
            credits: 0,
            jewel_mounts: BTreeMap::new(),
            craft_history: Vec::new(),
            applied_transactions: BTreeSet::new(),
        }
    }

    fn begin_transaction(&self, transaction_id: u64) -> Result<(), EconomyError> {
        if self.applied_transactions.contains(&transaction_id) {
            Err(EconomyError::AlreadyApplied(transaction_id))
        } else {
            Ok(())
        }
    }

    fn commit_transaction(&mut self, transaction_id: u64) {
        self.applied_transactions.insert(transaction_id);
        // Save records remain bounded while retaining enough replay protection
        // for queued/network-delayed commands.
        while self.applied_transactions.len() > 2_048 {
            if let Some(oldest) = self.applied_transactions.first().copied() {
                self.applied_transactions.remove(&oldest);
            }
        }
    }

    pub fn mounted_damage_multiplier(&self, weapon: WeaponSocket) -> f32 {
        self.jewel_mounts
            .get(&weapon)
            .map_or(1.0, |tier| 1.0 + tier.bonus_multiplier())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorKind {
    Weapon,
    Armor,
    General,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorStockRecord {
    pub item_id: String,
    pub buy_price: u32,
    pub sell_price: u32,
    pub stock: u32,
    pub max_stock: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VendorRecord {
    pub id: String,
    pub name: String,
    pub kind: VendorKind,
    pub position: Vec3Record,
    pub items: Vec<VendorStockRecord>,
    #[serde(default)]
    pub restock_epoch: u64,
}

const WEAPON_VENDOR_ITEMS: &[&str] = &[
    "plasma_cell",
    "kinetic_rounds",
    "rocket_ammo",
    "grenade_pack",
    "damage_amp",
    "weapon_mod_scope",
    "weapon_mod_barrel",
    "weapon_part_pistol",
    "weapon_part_rifle",
    "weapon_part_shotgun",
    "weapon_part_rocket",
    "weapon_part_laser",
    "weapon_part_grenade",
];

const ARMOR_VENDOR_ITEMS: &[&str] = &[
    "armor_shard",
    "shield_booster",
    "nano_fiber",
    "armor_plate_basic",
    "armor_plate_reinforced",
    "energy_shield_module",
];

const GENERAL_VENDOR_ITEMS: &[&str] = &[
    "health_pack",
    "gear",
    "scrap_metal",
    "energy_core",
    "circuit_board",
    "nano_fiber",
    "bio_sample",
    "xp_chip",
    "crystal_shard",
];

fn vendor_item_ids(kind: VendorKind) -> &'static [&'static str] {
    match kind {
        VendorKind::Weapon => WEAPON_VENDOR_ITEMS,
        VendorKind::Armor => ARMOR_VENDOR_ITEMS,
        VendorKind::General => GENERAL_VENDOR_ITEMS,
    }
}

fn vendor_stock(item_id: &str) -> Result<VendorStockRecord, EconomyError> {
    let item =
        item_definition(item_id).ok_or_else(|| EconomyError::UnknownItem(item_id.to_owned()))?;
    let max_stock = match item.rarity {
        ItemRarity::Rare => 3,
        ItemRarity::Uncommon => 8,
        // This mirrors Heavy Water's source ternary. No Epic/Legendary jewel
        // appears in ordinary vendor stock.
        ItemRarity::Common | ItemRarity::Epic | ItemRarity::Legendary => 20,
    };
    Ok(VendorStockRecord {
        item_id: item_id.to_owned(),
        buy_price: item.value.saturating_mul(3).div_ceil(2),
        sell_price: item.value / 2,
        stock: max_stock,
        max_stock,
    })
}

pub fn make_vendor(
    id: impl Into<String>,
    name: impl Into<String>,
    kind: VendorKind,
    position: Vec3Record,
) -> Result<VendorRecord, EconomyError> {
    let items = vendor_item_ids(kind)
        .iter()
        .map(|item_id| vendor_stock(item_id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(VendorRecord {
        id: id.into(),
        name: name.into(),
        kind,
        position,
        items,
        restock_epoch: 0,
    })
}

pub fn heavy_water_default_vendors() -> Result<Vec<VendorRecord>, EconomyError> {
    Ok(vec![
        make_vendor(
            "weapon_shop_1",
            "Cyber Arms Dealer",
            VendorKind::Weapon,
            Vec3Record::new(320.0, 0.0, 120.0),
        )?,
        make_vendor(
            "armor_shop_1",
            "Titan Defense Shop",
            VendorKind::Armor,
            Vec3Record::new(280.0, 0.0, 160.0),
        )?,
        make_vendor(
            "general_shop_1",
            "Neon Market",
            VendorKind::General,
            Vec3Record::new(360.0, 0.0, 80.0),
        )?,
        make_vendor(
            "general_shop_village_1",
            "Outpost Trader",
            VendorKind::General,
            Vec3Record::new(450.0, 0.0, 300.0),
        )?,
        make_vendor(
            "weapon_shop_village_1",
            "Frontier Armory",
            VendorKind::Weapon,
            Vec3Record::new(150.0, 0.0, 350.0),
        )?,
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseStructureKind {
    Lab,
    Garden,
    Workshop,
    Farm,
    CityHall,
    RobotFoundry,
    Spaceport,
    Shipyard,
    DefenseGrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildClass {
    Block,
    Prefab,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Footprint {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrefabCategory {
    Defense,
    Housing,
    Industry,
    Decoration,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub class: BuildClass,
    pub category: Option<PrefabCategory>,
    pub footprint: Footprint,
    pub cost: &'static [Cost],
    pub health: f32,
    pub grid_size: f32,
    pub rotation_step_degrees: u16,
    pub max_build_distance: f32,
    pub max_slope_degrees: f32,
    pub base_kind: Option<BaseStructureKind>,
}

macro_rules! block {
    ($id:literal, $name:literal, $w:expr, $h:expr, $d:expr, $hp:expr, [$($mat:literal => $qty:expr),* $(,)?]) => {
        BuildDefinition {
            id: $id,
            name: $name,
            class: BuildClass::Block,
            category: None,
            footprint: Footprint { width: $w, height: $h, depth: $d },
            cost: &[$(Cost::new($mat, $qty)),*],
            health: $hp,
            grid_size: 2.0,
            rotation_step_degrees: 90,
            max_build_distance: 12.0,
            max_slope_degrees: 35.0,
            base_kind: None,
        }
    };
}

macro_rules! prefab {
    ($id:literal, $name:literal, $category:ident, $w:expr, $d:expr, [$($mat:literal => $qty:expr),* $(,)?] $(, $base:ident)?) => {
        BuildDefinition {
            id: $id,
            name: $name,
            class: BuildClass::Prefab,
            category: Some(PrefabCategory::$category),
            footprint: Footprint { width: $w, height: 0.2, depth: $d },
            cost: &[$(Cost::new($mat, $qty)),*],
            health: 1_000.0,
            grid_size: 2.0,
            rotation_step_degrees: 45,
            max_build_distance: 60.0,
            max_slope_degrees: 20.0,
            base_kind: prefab!(@base $($base)?),
        }
    };
    (@base) => { None };
    (@base $base:ident) => { Some(BaseStructureKind::$base) };
}

/// The 19 build-mode blocks from Heavy Water. Sizes, health, and material
/// costs match the executable catalog.
pub static BLOCK_CATALOG: &[BuildDefinition] = &[
    block!("metal_wall", "Metal Wall", 4.0, 3.0, 0.5, 200.0, ["scrap_metal" => 4]),
    block!("glass", "Glass Panel", 4.0, 3.0, 0.15, 80.0, ["crystal_shard" => 2]),
    block!("platform", "Platform", 4.0, 0.3, 4.0, 150.0, ["scrap_metal" => 3]),
    block!("ramp", "Ramp", 4.0, 3.0, 4.0, 180.0, ["scrap_metal" => 5]),
    block!("door", "Door", 2.0, 3.0, 0.3, 120.0, ["scrap_metal" => 3, "circuit_board" => 1]),
    block!("light", "Light Block", 1.0, 1.0, 1.0, 60.0, ["energy_core" => 1]),
    block!("cube", "Cube", 2.0, 2.0, 2.0, 120.0, ["scrap_metal" => 2]),
    block!("sphere", "Sphere", 2.0, 2.0, 2.0, 110.0, ["scrap_metal" => 2, "nano_fiber" => 1]),
    block!("pyramid", "Pyramid", 3.0, 3.0, 3.0, 200.0, ["scrap_metal" => 4]),
    block!("pillar", "Pillar", 1.0, 4.0, 1.0, 180.0, ["scrap_metal" => 3]),
    block!("foundation", "Foundation", 6.0, 0.6, 6.0, 300.0, ["scrap_metal" => 6]),
    block!("fence", "Fence", 4.0, 1.5, 0.2, 60.0, ["scrap_metal" => 1]),
    block!("neon_strip", "Neon Strip", 4.0, 0.3, 0.3, 40.0, ["energy_core" => 1, "circuit_board" => 1]),
    block!("brick", "Brick", 1.0, 0.6, 0.5, 90.0, ["scrap_metal" => 1]),
    block!("stairs", "Stairs", 4.0, 3.0, 4.0, 200.0, ["scrap_metal" => 6]),
    block!("window", "Window", 4.0, 3.0, 0.3, 90.0, ["crystal_shard" => 1, "scrap_metal" => 2]),
    block!("tower", "Tower", 1.8, 8.0, 1.8, 320.0, ["scrap_metal" => 8]),
    block!("cone_roof", "Cone Roof", 4.0, 3.0, 4.0, 140.0, ["scrap_metal" => 4]),
    block!("turret", "Turret", 2.4, 2.6, 2.4, 240.0, ["scrap_metal" => 6, "circuit_board" => 1, "energy_core" => 1]),
];

/// The 16 authored prefab plans from Heavy Water. Visual construction remains
/// an engine adapter concern; this catalog is the authoritative domain cost
/// and footprint source.
pub static PREFAB_CATALOG: &[BuildDefinition] = &[
    prefab!("def_watchtower", "Watchtower", Defense, 4.0, 4.0, ["scrap_metal" => 18, "energy_core" => 1]),
    prefab!("def_bunker", "Bunker", Defense, 5.0, 5.0, ["scrap_metal" => 20]),
    prefab!("def_gate_arch", "Gate Arch", Defense, 6.0, 1.0, ["scrap_metal" => 14]),
    prefab!("def_turret_pad", "Turret Emplacement", Defense, 3.0, 3.0, ["scrap_metal" => 10, "circuit_board" => 2]),
    prefab!("hou_house_small", "Small House", Housing, 5.0, 5.0, ["scrap_metal" => 12, "nano_fiber" => 4]),
    prefab!("hou_market_stall", "Market Stall", Housing, 4.0, 3.0, ["scrap_metal" => 6, "nano_fiber" => 2]),
    prefab!("hou_well", "Energy Well", Housing, 2.5, 2.5, ["scrap_metal" => 6, "energy_core" => 1]),
    prefab!("hou_lamp_post", "Lamp Post", Housing, 1.0, 1.0, ["scrap_metal" => 3, "energy_core" => 1]),
    prefab!("ind_power_pylon", "Power Pylon", Industry, 3.0, 3.0, ["scrap_metal" => 14, "energy_core" => 2]),
    prefab!("ind_comms_dish", "Comms Dish", Industry, 3.0, 3.0, ["scrap_metal" => 10, "circuit_board" => 3]),
    prefab!("ind_storage_silo", "Storage Silo", Industry, 3.0, 3.0, ["scrap_metal" => 16]),
    prefab!("dec_monument", "Monument", Decoration, 2.0, 2.0, ["scrap_metal" => 8]),
    prefab!("dec_banner", "Banner", Decoration, 4.0, 1.0, ["nano_fiber" => 3, "scrap_metal" => 4]),
    prefab!("base_lab", "Lab", Industry, 7.0, 7.0, ["scrap_metal" => 30, "circuit_board" => 4, "energy_core" => 2], Lab),
    prefab!("base_garden", "Garden Dome", Industry, 8.0, 8.0, ["scrap_metal" => 22, "nano_fiber" => 6, "energy_core" => 1], Garden),
    prefab!("city_block_small", "City Block", Housing, 14.0, 14.0, ["scrap_metal" => 60, "circuit_board" => 4, "energy_core" => 2, "nano_fiber" => 6]),
];

pub fn build_definition(build_id: &str) -> Option<&'static BuildDefinition> {
    BLOCK_CATALOG
        .iter()
        .chain(PREFAB_CATALOG.iter())
        .find(|definition| definition.id == build_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementIssue {
    NonFiniteTransform,
    OffGrid,
    InvalidRotation,
    TooFarFromOwner,
    BelowTerrain,
    MissingSupport,
    SlopeTooSteep,
    ProtectedZone,
    RoadKeepout,
    DynamicObstruction,
    OverlapsBuild,
    OwnerBuildLimit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementEnvironmentRecord {
    pub owner_position: Vec3Record,
    pub terrain_height: f32,
    pub slope_degrees: f32,
    pub has_support: bool,
    pub protected_zone: bool,
    pub road_keepout: bool,
    pub dynamic_obstruction: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementValidationRecord {
    pub policy_version: u16,
    pub checked_at_ms: u64,
    pub owner: OwnerId,
    pub build_id: String,
    pub requested_position: Vec3Record,
    pub requested_rotation_degrees: f32,
    pub environment: PlacementEnvironmentRecord,
    pub issues: Vec<PlacementIssue>,
}

impl PlacementValidationRecord {
    pub fn accepted(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedBuildRecord {
    pub id: u64,
    pub owner: OwnerId,
    pub definition_id: String,
    pub class: BuildClass,
    pub position: Vec3Record,
    pub rotation_degrees: f32,
    pub footprint: Footprint,
    pub health: f32,
    pub max_health: f32,
    pub base_kind: Option<BaseStructureKind>,
    pub base_level: u8,
    pub placed_at_ms: u64,
    pub validation: PlacementValidationRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildLedgerRecord {
    pub next_id: u64,
    pub max_builds_per_owner: u32,
    pub builds: BTreeMap<u64, PlacedBuildRecord>,
}

impl Default for BuildLedgerRecord {
    fn default() -> Self {
        Self {
            next_id: 1,
            max_builds_per_owner: 2_000,
            builds: BTreeMap::new(),
        }
    }
}

fn near_grid(value: f32, grid: f32) -> bool {
    let units = value / grid;
    (units - units.round()).abs() <= 0.001
}

fn normalized_degrees(degrees: f32) -> f32 {
    degrees.rem_euclid(360.0)
}

fn rotated_horizontal_half_extents(footprint: Footprint, rotation_degrees: f32) -> (f32, f32) {
    let radians = normalized_degrees(rotation_degrees).to_radians();
    let (sin, cos) = radians.sin_cos();
    let half_width = footprint.width * 0.5;
    let half_depth = footprint.depth * 0.5;
    (
        cos.abs() * half_width + sin.abs() * half_depth,
        sin.abs() * half_width + cos.abs() * half_depth,
    )
}

fn builds_overlap(
    a_position: Vec3Record,
    a_footprint: Footprint,
    a_rotation: f32,
    b: &PlacedBuildRecord,
) -> bool {
    let (a_x, a_z) = rotated_horizontal_half_extents(a_footprint, a_rotation);
    let (b_x, b_z) = rotated_horizontal_half_extents(b.footprint, b.rotation_degrees);
    let horizontal_overlap = (a_position.x - b.position.x).abs() + 0.001 < a_x + b_x
        && (a_position.z - b.position.z).abs() + 0.001 < a_z + b_z;
    let a_min_y = a_position.y;
    let a_max_y = a_position.y + a_footprint.height.max(0.05);
    let b_min_y = b.position.y;
    let b_max_y = b.position.y + b.footprint.height.max(0.05);
    horizontal_overlap && a_min_y + 0.001 < b_max_y && b_min_y + 0.001 < a_max_y
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MiningNodeKind {
    ScrapPile,
    CrystalCluster,
    BioPod,
    GearCache,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MiningNodeDefinition {
    pub kind: MiningNodeKind,
    pub max_health: f32,
    pub hit_radius: f32,
    pub drops: &'static [Cost],
    pub respawn_seconds: u32,
}

pub static MINING_NODE_CATALOG: &[MiningNodeDefinition] = &[
    MiningNodeDefinition {
        kind: MiningNodeKind::ScrapPile,
        max_health: 30.0,
        hit_radius: 1.6,
        drops: &[Cost::new("scrap_metal", 4), Cost::new("gear", 1)],
        respawn_seconds: 35,
    },
    MiningNodeDefinition {
        kind: MiningNodeKind::CrystalCluster,
        max_health: 50.0,
        hit_radius: 1.8,
        drops: &[
            Cost::new("energy_core", 2),
            Cost::new("scrap_metal", 2),
            Cost::new("circuit_board", 1),
        ],
        respawn_seconds: 50,
    },
    MiningNodeDefinition {
        kind: MiningNodeKind::BioPod,
        max_health: 40.0,
        hit_radius: 1.6,
        drops: &[Cost::new("bio_essence", 3), Cost::new("nano_fiber", 1)],
        respawn_seconds: 45,
    },
    MiningNodeDefinition {
        kind: MiningNodeKind::GearCache,
        max_health: 70.0,
        hit_radius: 2.0,
        drops: &[
            Cost::new("gear", 6),
            Cost::new("circuit_board", 2),
            Cost::new("weapon_part_rifle", 1),
        ],
        respawn_seconds: 60,
    },
];

pub fn mining_node_definition(kind: MiningNodeKind) -> &'static MiningNodeDefinition {
    MINING_NODE_CATALOG
        .iter()
        .find(|definition| definition.kind == kind)
        .expect("every MiningNodeKind has a catalog entry")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiningNodeRecord {
    pub id: u64,
    pub kind: MiningNodeKind,
    pub position: Vec3Record,
    pub health: f32,
    pub active: bool,
    pub respawn_at_ms: u64,
    pub shatter_cycle: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropBundle {
    pub items: Vec<ItemQuantity>,
}

impl DropBundle {
    pub fn from_costs(costs: &[Cost]) -> Self {
        Self {
            items: costs.iter().copied().map(Cost::owned).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum MiningHitOutcome {
    Inactive {
        respawn_at_ms: u64,
    },
    Damaged {
        remaining_health: f32,
    },
    Shattered {
        owner: OwnerId,
        drops: DropBundle,
        respawn_at_ms: u64,
        shatter_cycle: u32,
    },
}

/// Stable SplitMix64 stream: deterministic across platforms and independent
/// of Bevy/rand implementation changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    pub fn unit_f32(&mut self) -> f32 {
        let mantissa = (self.next_u64() >> 40) as u32;
        mantissa as f32 / (1_u32 << 24) as f32
    }

    pub fn inclusive_u32(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            return min;
        }
        let width = u64::from(max - min) + 1;
        min + (self.next_u64() % width) as u32
    }
}

fn domain_seed(world_seed: u64, source_id: u64, cycle: u32, domain: u64) -> u64 {
    world_seed
        ^ source_id.wrapping_mul(0xD6E8_FEB8_6659_FD93)
        ^ u64::from(cycle).wrapping_mul(0xA076_1D64_78BD_642F)
        ^ domain
}

/// Deterministic equivalent of BuildingSystem's destructible-world material
/// table: scrap 70%, circuit 30%, core 15%, fiber 20%, crystal 10%.
pub fn deterministic_salvage_drops(
    world_seed: u64,
    source_id: u64,
    destruction_cycle: u32,
) -> DropBundle {
    let mut rng = DeterministicRng::new(domain_seed(
        world_seed,
        source_id,
        destruction_cycle,
        0x5341_4C56_4147_4501,
    ));
    let table = [
        ("scrap_metal", 0.70_f32, 1, 3),
        ("circuit_board", 0.30, 1, 1),
        ("energy_core", 0.15, 1, 1),
        ("nano_fiber", 0.20, 1, 2),
        ("crystal_shard", 0.10, 1, 1),
    ];
    let mut items = Vec::new();
    for (item_id, chance, min, max) in table {
        if rng.unit_f32() < chance {
            items.push(ItemQuantity::new(item_id, rng.inclusive_u32(min, max)));
        }
    }
    DropBundle { items }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JewelDropSource {
    BossCaptain,
    CaptainOrCommander,
    AerialBattleship,
    RegularVault,
    BossVault,
}

fn weighted_jewel_tier(
    rng: &mut DeterministicRng,
    rough: u32,
    cut: u32,
    flawless: u32,
) -> JewelTier {
    let total = rough + cut + flawless;
    let roll = rng.unit_f32() * total as f32;
    if roll < flawless as f32 {
        JewelTier::Flawless
    } else if roll < (flawless + cut) as f32 {
        JewelTier::Cut
    } else {
        JewelTier::Rough
    }
}

/// Deterministic port of PickupSystem/EnemyBaseSystem jewel rolls. The source
/// ordering is preserved, including the regular-vault 5% rough / 1.5% cut /
/// 0.4% flawless single roll and the boss vault's independent bonus rolls.
pub fn deterministic_jewel_drops(
    world_seed: u64,
    source_id: u64,
    cycle: u32,
    source: JewelDropSource,
) -> Vec<JewelTier> {
    let mut rng = DeterministicRng::new(domain_seed(
        world_seed,
        source_id,
        cycle,
        0x4A45_5745_4C00_0001,
    ));
    match source {
        JewelDropSource::BossCaptain => {
            vec![weighted_jewel_tier(&mut rng, 60, 30, 10)]
        }
        JewelDropSource::CaptainOrCommander => {
            if rng.unit_f32() < 0.20 {
                vec![weighted_jewel_tier(&mut rng, 80, 17, 3)]
            } else {
                Vec::new()
            }
        }
        JewelDropSource::AerialBattleship => {
            if rng.unit_f32() < 0.35 {
                vec![weighted_jewel_tier(&mut rng, 70, 25, 5)]
            } else {
                Vec::new()
            }
        }
        JewelDropSource::RegularVault => {
            let roll = rng.unit_f32();
            if roll < 0.05 {
                vec![JewelTier::Rough]
            } else if roll < 0.065 {
                vec![JewelTier::Cut]
            } else if roll < 0.069 {
                vec![JewelTier::Flawless]
            } else {
                Vec::new()
            }
        }
        JewelDropSource::BossVault => {
            let mut jewels = vec![JewelTier::Rough];
            if rng.unit_f32() < 0.50 {
                jewels.push(JewelTier::Cut);
            }
            if rng.unit_f32() < 0.20 {
                jewels.push(JewelTier::Flawless);
            }
            jewels
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultReward {
    pub credits: u32,
    pub drops: DropBundle,
    pub jewels: Vec<JewelTier>,
}

pub fn deterministic_vault_reward(
    world_seed: u64,
    vault_id: u64,
    cycle: u32,
    boss: bool,
) -> VaultReward {
    let (credits, drops, source) = if boss {
        (
            1_500,
            vec![
                ItemQuantity::new("gear", 30),
                ItemQuantity::new("energy_core", 15),
                ItemQuantity::new("circuit_board", 15),
                ItemQuantity::new("nano_fiber", 10),
                ItemQuantity::new("weapon_part_rocket", 4),
                ItemQuantity::new("weapon_part_laser", 3),
                ItemQuantity::new("weapon_part_grenade", 3),
            ],
            JewelDropSource::BossVault,
        )
    } else {
        (
            250,
            vec![
                ItemQuantity::new("gear", 10),
                ItemQuantity::new("energy_core", 5),
                ItemQuantity::new("circuit_board", 5),
                ItemQuantity::new("nano_fiber", 3),
                ItemQuantity::new("weapon_part_rocket", 2),
                ItemQuantity::new("weapon_part_laser", 1),
            ],
            JewelDropSource::RegularVault,
        )
    };
    VaultReward {
        credits,
        drops: DropBundle { items: drops },
        jewels: deterministic_jewel_drops(world_seed, vault_id, cycle, source),
    }
}

pub fn deterministic_mining_layout(
    world_seed: u64,
    count: u32,
) -> Vec<(MiningNodeKind, Vec3Record)> {
    let mut rng = DeterministicRng::new(world_seed ^ 0x4D49_4E49_4E47_0001);
    let weighted_kinds = [
        MiningNodeKind::ScrapPile,
        MiningNodeKind::ScrapPile,
        MiningNodeKind::CrystalCluster,
        MiningNodeKind::BioPod,
        MiningNodeKind::GearCache,
    ];
    (0..count)
        .map(|_| {
            let kind = weighted_kinds[(rng.next_u64() % weighted_kinds.len() as u64) as usize];
            let angle = rng.unit_f32() * std::f32::consts::TAU;
            let radius = 80.0 + rng.unit_f32() * 360.0;
            (
                kind,
                Vec3Record::new(angle.cos() * radius, 0.0, angle.sin() * radius),
            )
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmmoWeapon {
    Pistol,
    Rifle,
    Shotgun,
    Rocket,
    Laser,
    Grenade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ChestReward {
    Credits { amount: u32 },
    Health { amount: u32 },
    Armor { amount: u32 },
    Ammo { weapon: AmmoWeapon, amount: u32 },
    WeaponUpgrade { amount: u32 },
}

pub fn deterministic_chest_reward(world_seed: u64, chest_id: u64) -> ChestReward {
    let mut rng =
        DeterministicRng::new(domain_seed(world_seed, chest_id, 0, 0x4348_4553_5400_0001));
    let roll = rng.unit_f32();
    if roll < 0.35 {
        ChestReward::Credits {
            amount: rng.inclusive_u32(50, 199),
        }
    } else if roll < 0.55 {
        ChestReward::Health {
            amount: rng.inclusive_u32(25, 74),
        }
    } else if roll < 0.70 {
        ChestReward::Armor {
            amount: rng.inclusive_u32(20, 59),
        }
    } else if roll < 0.90 {
        let weapons = [
            AmmoWeapon::Pistol,
            AmmoWeapon::Rifle,
            AmmoWeapon::Shotgun,
            AmmoWeapon::Rocket,
            AmmoWeapon::Laser,
            AmmoWeapon::Grenade,
        ];
        ChestReward::Ammo {
            weapon: weapons[(rng.next_u64() % weapons.len() as u64) as usize],
            amount: rng.inclusive_u32(20, 49),
        }
    } else {
        ChestReward::WeaponUpgrade { amount: 1 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChestRecord {
    pub id: u64,
    pub position: Vec3Record,
    pub reward: ChestReward,
    pub claimed_by: Option<OwnerId>,
    pub claimed_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeavyEconomyState {
    pub schema_version: u16,
    pub world_seed: u64,
    pub owners: BTreeMap<OwnerId, OwnerAccount>,
    pub vendors: BTreeMap<String, VendorRecord>,
    pub builds: BuildLedgerRecord,
    pub mining_nodes: BTreeMap<u64, MiningNodeRecord>,
    pub chests: BTreeMap<u64, ChestRecord>,
    pub next_mining_node_id: u64,
    pub next_chest_id: u64,
}

impl Default for HeavyEconomyState {
    fn default() -> Self {
        Self::new(0)
    }
}

impl HeavyEconomyState {
    pub fn new(world_seed: u64) -> Self {
        let vendors = heavy_water_default_vendors()
            .expect("the built-in Heavy Water vendor catalog must be internally valid")
            .into_iter()
            .map(|vendor| (vendor.id.clone(), vendor))
            .collect();
        Self {
            schema_version: HEAVY_ECONOMY_SCHEMA_VERSION,
            world_seed,
            owners: BTreeMap::new(),
            vendors,
            builds: BuildLedgerRecord::default(),
            mining_nodes: BTreeMap::new(),
            chests: BTreeMap::new(),
            next_mining_node_id: 1,
            next_chest_id: 1,
        }
    }

    pub fn validate(&self) -> Result<(), EconomyError> {
        if self.schema_version != HEAVY_ECONOMY_SCHEMA_VERSION {
            return Err(EconomyError::MalformedRecord(
                "unsupported Heavy economy schema version",
            ));
        }
        for (owner, account) in &self.owners {
            if account.owner != *owner {
                return Err(EconomyError::MalformedRecord("owner key mismatch"));
            }
            account.inventory.validate()?;
            if account.craft_history.iter().any(|receipt| {
                receipt.completes_at_ms < receipt.started_at_ms || !receipt.result_granted
            }) {
                return Err(EconomyError::MalformedRecord("invalid craft receipt"));
            }
        }
        for (vendor_id, vendor) in &self.vendors {
            if &vendor.id != vendor_id {
                return Err(EconomyError::MalformedRecord("vendor key mismatch"));
            }
            for stock in &vendor.items {
                if item_definition(&stock.item_id).is_none() || stock.stock > stock.max_stock {
                    return Err(EconomyError::MalformedRecord("invalid vendor stock"));
                }
            }
        }
        for build in self.builds.builds.values() {
            if !build.position.is_finite()
                || !build.rotation_degrees.is_finite()
                || build.health < 0.0
                || build.health > build.max_health
                || !(1..=MAX_BASE_LEVEL).contains(&build.base_level)
            {
                return Err(EconomyError::MalformedRecord("invalid placed build"));
            }
        }
        for node in self.mining_nodes.values() {
            let definition = mining_node_definition(node.kind);
            if !node.position.is_finite()
                || !node.health.is_finite()
                || node.health < 0.0
                || node.health > definition.max_health
            {
                return Err(EconomyError::MalformedRecord("invalid mining node"));
            }
        }
        Ok(())
    }

    pub fn register_owner(&mut self, owner: OwnerId) -> bool {
        if self.owners.contains_key(&owner) {
            return false;
        }
        self.owners.insert(owner, OwnerAccount::new(owner));
        true
    }

    pub fn account(&self, owner: OwnerId) -> Result<&OwnerAccount, EconomyError> {
        self.owners
            .get(&owner)
            .ok_or(EconomyError::MissingOwner(owner))
    }

    fn staged_account(
        &self,
        owner: OwnerId,
        transaction_id: u64,
    ) -> Result<OwnerAccount, EconomyError> {
        let account = self.account(owner)?.clone();
        account.inventory.validate()?;
        account.begin_transaction(transaction_id)?;
        Ok(account)
    }

    pub fn grant_items_atomic(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        items: &[ItemQuantity],
    ) -> Result<(), EconomyError> {
        let mut staged = self.staged_account(owner, transaction_id)?;
        staged.inventory.apply_bundle_atomic(&[], items)?;
        staged.commit_transaction(transaction_id);
        self.owners.insert(owner, staged);
        Ok(())
    }

    pub fn grant_credits_atomic(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        credits: u64,
    ) -> Result<u64, EconomyError> {
        if credits == 0 {
            return Err(EconomyError::InvalidQuantity);
        }
        let mut staged = self.staged_account(owner, transaction_id)?;
        staged.credits = staged
            .credits
            .checked_add(credits)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        staged.commit_transaction(transaction_id);
        let balance = staged.credits;
        self.owners.insert(owner, staged);
        Ok(balance)
    }

    pub fn craft(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        recipe_id: &str,
        player_level: u8,
        now_ms: u64,
    ) -> Result<CraftReceipt, EconomyError> {
        let recipe = recipe_definition(recipe_id)
            .ok_or_else(|| EconomyError::UnknownRecipe(recipe_id.to_owned()))?;
        if player_level < recipe.required_level {
            return Err(EconomyError::LevelLocked {
                required: recipe.required_level,
                actual: player_level,
            });
        }
        let completes_at_ms = now_ms
            .checked_add(u64::from(recipe.craft_seconds) * 1_000)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        let mut staged = self.staged_account(owner, transaction_id)?;
        let debits = recipe
            .materials
            .iter()
            .copied()
            .map(Cost::owned)
            .collect::<Vec<_>>();
        staged
            .inventory
            .apply_bundle_atomic(&debits, &[recipe.result.owned()])?;
        let receipt = CraftReceipt {
            transaction_id,
            recipe_id: recipe.id.to_owned(),
            started_at_ms: now_ms,
            completes_at_ms,
            result_granted: true,
        };
        staged.craft_history.push(receipt.clone());
        if staged.craft_history.len() > 64 {
            let excess = staged.craft_history.len() - 64;
            staged.craft_history.drain(0..excess);
        }
        staged.commit_transaction(transaction_id);
        self.owners.insert(owner, staged);
        Ok(receipt)
    }

    pub fn mount_jewel(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        weapon: WeaponSocket,
        tier: JewelTier,
    ) -> Result<(), EconomyError> {
        let mut staged = self.staged_account(owner, transaction_id)?;
        if staged.jewel_mounts.get(&weapon).copied() == Some(tier) {
            return Err(EconomyError::AlreadyMounted);
        }

        // Remove the incoming jewel first in the staged copy. This can free a
        // slot, after which returning the old jewel is attempted. Any failure
        // discards the staged copy, so neither jewel can be duplicated/lost.
        staged.inventory.remove_exact(tier.item_id(), 1)?;
        if let Some(previous) = staged.jewel_mounts.get(&weapon).copied() {
            staged.inventory.add_exact(previous.item_id(), 1)?;
        }
        staged.jewel_mounts.insert(weapon, tier);
        staged.commit_transaction(transaction_id);
        self.owners.insert(owner, staged);
        Ok(())
    }

    pub fn unmount_jewel(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        weapon: WeaponSocket,
    ) -> Result<JewelTier, EconomyError> {
        let mut staged = self.staged_account(owner, transaction_id)?;
        let tier = staged
            .jewel_mounts
            .get(&weapon)
            .copied()
            .ok_or(EconomyError::NothingMounted)?;
        staged.inventory.add_exact(tier.item_id(), 1)?;
        staged.jewel_mounts.remove(&weapon);
        staged.commit_transaction(transaction_id);
        self.owners.insert(owner, staged);
        Ok(tier)
    }

    pub fn buy_from_vendor(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        vendor_id: &str,
        item_id: &str,
        quantity: u32,
    ) -> Result<u64, EconomyError> {
        if quantity == 0 {
            return Err(EconomyError::InvalidQuantity);
        }
        let mut staged_account = self.staged_account(owner, transaction_id)?;
        let mut staged_vendor = self
            .vendors
            .get(vendor_id)
            .cloned()
            .ok_or_else(|| EconomyError::UnknownVendor(vendor_id.to_owned()))?;
        let stock = staged_vendor
            .items
            .iter_mut()
            .find(|stock| stock.item_id == item_id)
            .ok_or_else(|| EconomyError::UnknownVendorItem(item_id.to_owned()))?;
        if stock.stock < quantity {
            return Err(EconomyError::OutOfStock {
                item_id: item_id.to_owned(),
                requested: quantity,
                available: stock.stock,
            });
        }
        let total_cost = u64::from(stock.buy_price)
            .checked_mul(u64::from(quantity))
            .ok_or(EconomyError::ArithmeticOverflow)?;
        if staged_account.credits < total_cost {
            return Err(EconomyError::InsufficientCredits {
                needed: total_cost,
                available: staged_account.credits,
            });
        }
        staged_account.inventory.add_exact(item_id, quantity)?;
        staged_account.credits -= total_cost;
        stock.stock -= quantity;
        staged_account.commit_transaction(transaction_id);
        let balance = staged_account.credits;
        self.owners.insert(owner, staged_account);
        self.vendors.insert(vendor_id.to_owned(), staged_vendor);
        Ok(balance)
    }

    pub fn sell_to_vendor(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        vendor_id: &str,
        item_id: &str,
        quantity: u32,
    ) -> Result<u64, EconomyError> {
        if quantity == 0 {
            return Err(EconomyError::InvalidQuantity);
        }
        let mut staged_account = self.staged_account(owner, transaction_id)?;
        let mut staged_vendor = self
            .vendors
            .get(vendor_id)
            .cloned()
            .ok_or_else(|| EconomyError::UnknownVendor(vendor_id.to_owned()))?;
        let stocked_index = staged_vendor
            .items
            .iter()
            .position(|stock| stock.item_id == item_id);
        let unit_price = if let Some(index) = stocked_index {
            staged_vendor.items[index].sell_price
        } else {
            item_definition(item_id)
                .ok_or_else(|| EconomyError::UnknownItem(item_id.to_owned()))?
                .value
                .saturating_mul(2)
                / 5
        };
        staged_account.inventory.remove_exact(item_id, quantity)?;
        let proceeds = u64::from(unit_price)
            .checked_mul(u64::from(quantity))
            .ok_or(EconomyError::ArithmeticOverflow)?;
        staged_account.credits = staged_account
            .credits
            .checked_add(proceeds)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        if let Some(index) = stocked_index {
            let stock = &mut staged_vendor.items[index];
            stock.stock = stock.stock.saturating_add(quantity).min(stock.max_stock);
        }
        staged_account.commit_transaction(transaction_id);
        let balance = staged_account.credits;
        self.owners.insert(owner, staged_account);
        self.vendors.insert(vendor_id.to_owned(), staged_vendor);
        Ok(balance)
    }

    /// Idempotent deterministic restock. Replaying the same or an older epoch
    /// cannot duplicate stock; a newer epoch restores every line to max.
    pub fn restock_vendor(
        &mut self,
        vendor_id: &str,
        restock_epoch: u64,
    ) -> Result<bool, EconomyError> {
        let vendor = self
            .vendors
            .get_mut(vendor_id)
            .ok_or_else(|| EconomyError::UnknownVendor(vendor_id.to_owned()))?;
        if restock_epoch <= vendor.restock_epoch {
            return Ok(false);
        }
        for stock in &mut vendor.items {
            stock.stock = stock.max_stock;
        }
        vendor.restock_epoch = restock_epoch;
        Ok(true)
    }

    pub fn validate_placement(
        &self,
        owner: OwnerId,
        build_id: &str,
        position: Vec3Record,
        rotation_degrees: f32,
        environment: PlacementEnvironmentRecord,
        now_ms: u64,
    ) -> Result<PlacementValidationRecord, EconomyError> {
        self.account(owner)?;
        let definition = build_definition(build_id)
            .ok_or_else(|| EconomyError::UnknownBuild(build_id.to_owned()))?;
        let mut issues = Vec::new();
        let environment_finite = environment.owner_position.is_finite()
            && environment.terrain_height.is_finite()
            && environment.slope_degrees.is_finite();
        if !position.is_finite() || !rotation_degrees.is_finite() || !environment_finite {
            issues.push(PlacementIssue::NonFiniteTransform);
        } else {
            if !near_grid(position.x, definition.grid_size)
                || !near_grid(position.z, definition.grid_size)
                || (definition.class == BuildClass::Block
                    && !near_grid(position.y, definition.grid_size))
            {
                issues.push(PlacementIssue::OffGrid);
            }
            let rotation = normalized_degrees(rotation_degrees);
            let step = f32::from(definition.rotation_step_degrees);
            if (rotation / step - (rotation / step).round()).abs() > 0.001 {
                issues.push(PlacementIssue::InvalidRotation);
            }
            if position.horizontal_distance_squared(environment.owner_position)
                > definition.max_build_distance * definition.max_build_distance
            {
                issues.push(PlacementIssue::TooFarFromOwner);
            }
            if position.y + 0.05 < environment.terrain_height {
                issues.push(PlacementIssue::BelowTerrain);
            }
            let elevated = position.y > environment.terrain_height + 0.25;
            if elevated && !environment.has_support {
                issues.push(PlacementIssue::MissingSupport);
            }
            if !elevated && environment.slope_degrees > definition.max_slope_degrees {
                issues.push(PlacementIssue::SlopeTooSteep);
            }
        }
        if environment.protected_zone {
            issues.push(PlacementIssue::ProtectedZone);
        }
        if environment.road_keepout {
            issues.push(PlacementIssue::RoadKeepout);
        }
        if environment.dynamic_obstruction {
            issues.push(PlacementIssue::DynamicObstruction);
        }
        if self
            .builds
            .builds
            .values()
            .filter(|build| build.owner == owner)
            .count()
            >= self.builds.max_builds_per_owner as usize
        {
            issues.push(PlacementIssue::OwnerBuildLimit);
        }
        if position.is_finite()
            && rotation_degrees.is_finite()
            && self.builds.builds.values().any(|build| {
                builds_overlap(position, definition.footprint, rotation_degrees, build)
            })
        {
            issues.push(PlacementIssue::OverlapsBuild);
        }
        issues.sort_by_key(|issue| *issue as u8);
        issues.dedup();
        Ok(PlacementValidationRecord {
            policy_version: 1,
            checked_at_ms: now_ms,
            owner,
            build_id: build_id.to_owned(),
            requested_position: position,
            requested_rotation_degrees: normalized_degrees(rotation_degrees),
            environment,
            issues,
        })
    }

    pub fn place_build(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        build_id: &str,
        position: Vec3Record,
        rotation_degrees: f32,
        environment: PlacementEnvironmentRecord,
        now_ms: u64,
    ) -> Result<PlacedBuildRecord, EconomyError> {
        let validation = self.validate_placement(
            owner,
            build_id,
            position,
            rotation_degrees,
            environment,
            now_ms,
        )?;
        if !validation.accepted() {
            return Err(EconomyError::PlacementRejected(validation));
        }
        let definition = build_definition(build_id)
            .ok_or_else(|| EconomyError::UnknownBuild(build_id.to_owned()))?;
        let mut staged_account = self.staged_account(owner, transaction_id)?;
        let debits = definition
            .cost
            .iter()
            .copied()
            .map(Cost::owned)
            .collect::<Vec<_>>();
        staged_account.inventory.apply_bundle_atomic(&debits, &[])?;

        let mut staged_builds = self.builds.clone();
        let build_record_id = staged_builds.next_id;
        staged_builds.next_id = staged_builds
            .next_id
            .checked_add(1)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        let record = PlacedBuildRecord {
            id: build_record_id,
            owner,
            definition_id: build_id.to_owned(),
            class: definition.class,
            position,
            rotation_degrees: normalized_degrees(rotation_degrees),
            footprint: definition.footprint,
            health: definition.health,
            max_health: definition.health,
            base_kind: definition.base_kind,
            base_level: 1,
            placed_at_ms: now_ms,
            validation,
        };
        staged_builds.builds.insert(record.id, record.clone());
        staged_account.commit_transaction(transaction_id);
        self.owners.insert(owner, staged_account);
        self.builds = staged_builds;
        Ok(record)
    }

    /// Heavy Water removes prefabs without a material refund. Ownership is
    /// enforced here so one local/network player cannot dismantle another's
    /// settlement record.
    pub fn remove_build(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        build_record_id: u64,
    ) -> Result<PlacedBuildRecord, EconomyError> {
        let build = self
            .builds
            .builds
            .get(&build_record_id)
            .cloned()
            .ok_or_else(|| EconomyError::UnknownBuild(build_record_id.to_string()))?;
        if build.owner != owner {
            return Err(EconomyError::NotOwner);
        }
        let mut staged_account = self.staged_account(owner, transaction_id)?;
        let mut staged_builds = self.builds.clone();
        staged_builds.builds.remove(&build_record_id);
        staged_account.commit_transaction(transaction_id);
        self.owners.insert(owner, staged_account);
        self.builds = staged_builds;
        Ok(build)
    }

    pub fn upgrade_base_structure(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        build_record_id: u64,
    ) -> Result<u8, EconomyError> {
        let build = self
            .builds
            .builds
            .get(&build_record_id)
            .cloned()
            .ok_or_else(|| EconomyError::UnknownBuild(build_record_id.to_string()))?;
        if build.owner != owner {
            return Err(EconomyError::NotOwner);
        }
        let base_kind = build
            .base_kind
            .ok_or_else(|| EconomyError::UnknownBuild("not a base structure".to_owned()))?;
        if build.base_level >= MAX_BASE_LEVEL {
            return Err(EconomyError::MaxBaseLevel);
        }
        let next_level = build.base_level + 1;
        let costs = base_upgrade_cost(base_kind, next_level);
        let mut staged_account = self.staged_account(owner, transaction_id)?;
        staged_account.inventory.apply_bundle_atomic(&costs, &[])?;
        let mut staged_builds = self.builds.clone();
        let staged_build = staged_builds
            .builds
            .get_mut(&build_record_id)
            .ok_or_else(|| EconomyError::UnknownBuild(build_record_id.to_string()))?;
        staged_build.base_level = next_level;
        staged_account.commit_transaction(transaction_id);
        self.owners.insert(owner, staged_account);
        self.builds = staged_builds;
        Ok(next_level)
    }

    pub fn highest_base_level(&self, owner: OwnerId, kind: BaseStructureKind) -> u8 {
        self.builds
            .builds
            .values()
            .filter(|build| build.owner == owner && build.base_kind == Some(kind))
            .map(|build| build.base_level)
            .max()
            .unwrap_or(0)
    }

    pub fn lab_companion_cap(&self, owner: OwnerId) -> u8 {
        match self.highest_base_level(owner, BaseStructureKind::Lab) {
            1 => 3,
            2 => 5,
            3.. => 8,
            0 => 0,
        }
    }

    pub fn garden_capture_cap(&self, owner: OwnerId) -> u8 {
        match self.highest_base_level(owner, BaseStructureKind::Garden) {
            1 => 15,
            2 => 30,
            3.. => 50,
            0 => 0,
        }
    }

    pub fn garden_capture_bonus(&self, owner: OwnerId) -> f32 {
        match self.highest_base_level(owner, BaseStructureKind::Garden) {
            2 => 0.15,
            3.. => 0.30,
            _ => 0.0,
        }
    }

    pub fn settlement_strength(&self, owner: OwnerId) -> u32 {
        self.builds
            .builds
            .values()
            .filter(|build| build.owner == owner)
            .filter_map(|build| {
                build.base_kind.map(|kind| {
                    u32::from(settlement_strength_for(kind)) * u32::from(build.base_level)
                })
            })
            .sum()
    }

    pub fn spawn_mining_node(
        &mut self,
        kind: MiningNodeKind,
        position: Vec3Record,
    ) -> Result<u64, EconomyError> {
        if !position.is_finite() {
            return Err(EconomyError::MalformedRecord(
                "non-finite mining node position",
            ));
        }
        let id = self.next_mining_node_id;
        self.next_mining_node_id = self
            .next_mining_node_id
            .checked_add(1)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        let definition = mining_node_definition(kind);
        self.mining_nodes.insert(
            id,
            MiningNodeRecord {
                id,
                kind,
                position,
                health: definition.max_health,
                active: true,
                respawn_at_ms: 0,
                shatter_cycle: 0,
            },
        );
        Ok(id)
    }

    pub fn seed_mining_nodes(&mut self, count: u32) -> Result<Vec<u64>, EconomyError> {
        deterministic_mining_layout(self.world_seed, count)
            .into_iter()
            .map(|(kind, position)| self.spawn_mining_node(kind, position))
            .collect()
    }

    pub fn damage_mining_node(
        &mut self,
        owner: OwnerId,
        node_id: u64,
        damage: f32,
        now_ms: u64,
    ) -> Result<MiningHitOutcome, EconomyError> {
        self.account(owner)?;
        if !damage.is_finite() || damage <= 0.0 {
            return Err(EconomyError::InvalidDamage);
        }
        let node = self
            .mining_nodes
            .get_mut(&node_id)
            .ok_or(EconomyError::UnknownMiningNode(node_id))?;
        let definition = mining_node_definition(node.kind);
        if !node.active {
            if now_ms < node.respawn_at_ms {
                return Ok(MiningHitOutcome::Inactive {
                    respawn_at_ms: node.respawn_at_ms,
                });
            }
            node.active = true;
            node.health = definition.max_health;
            node.respawn_at_ms = 0;
        }
        node.health = (node.health - damage).max(0.0);
        if node.health > 0.0 {
            return Ok(MiningHitOutcome::Damaged {
                remaining_health: node.health,
            });
        }
        node.active = false;
        node.shatter_cycle = node.shatter_cycle.saturating_add(1);
        node.respawn_at_ms = now_ms
            .checked_add(u64::from(definition.respawn_seconds) * 1_000)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        Ok(MiningHitOutcome::Shattered {
            owner,
            drops: DropBundle::from_costs(definition.drops),
            respawn_at_ms: node.respawn_at_ms,
            shatter_cycle: node.shatter_cycle,
        })
    }

    pub fn respawn_due_mining_nodes(&mut self, now_ms: u64) -> Vec<u64> {
        let mut respawned = Vec::new();
        for node in self.mining_nodes.values_mut() {
            if node.active || now_ms < node.respawn_at_ms {
                continue;
            }
            node.active = true;
            node.health = mining_node_definition(node.kind).max_health;
            node.respawn_at_ms = 0;
            respawned.push(node.id);
        }
        respawned
    }

    pub fn spawn_chest(&mut self, position: Vec3Record) -> Result<u64, EconomyError> {
        if !position.is_finite() {
            return Err(EconomyError::MalformedRecord("non-finite chest position"));
        }
        let id = self.next_chest_id;
        self.next_chest_id = self
            .next_chest_id
            .checked_add(1)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        self.chests.insert(
            id,
            ChestRecord {
                id,
                position,
                reward: deterministic_chest_reward(self.world_seed, id),
                claimed_by: None,
                claimed_at_ms: None,
            },
        );
        Ok(id)
    }

    /// Claims a chest exactly once. Wallet credits are committed here; health,
    /// armor, weapon ammo, and weapon-upgrade effects are returned to the
    /// gameplay adapter so their native subsystems remain authoritative.
    pub fn claim_chest(
        &mut self,
        owner: OwnerId,
        transaction_id: u64,
        chest_id: u64,
        now_ms: u64,
    ) -> Result<ChestReward, EconomyError> {
        let mut staged_account = self.staged_account(owner, transaction_id)?;
        let mut staged_chest = self
            .chests
            .get(&chest_id)
            .cloned()
            .ok_or(EconomyError::UnknownChest(chest_id))?;
        if staged_chest.claimed_by.is_some() {
            return Err(EconomyError::AlreadyClaimed);
        }
        if let ChestReward::Credits { amount } = &staged_chest.reward {
            staged_account.credits = staged_account
                .credits
                .checked_add(u64::from(*amount))
                .ok_or(EconomyError::ArithmeticOverflow)?;
        }
        staged_chest.claimed_by = Some(owner);
        staged_chest.claimed_at_ms = Some(now_ms);
        staged_account.commit_transaction(transaction_id);
        let reward = staged_chest.reward.clone();
        self.owners.insert(owner, staged_account);
        self.chests.insert(chest_id, staged_chest);
        Ok(reward)
    }
}

pub fn base_upgrade_cost(kind: BaseStructureKind, next_level: u8) -> Vec<ItemQuantity> {
    let tier = u32::from(next_level.saturating_sub(1).max(1));
    let (gears, scrap, energy_cores) = match kind {
        BaseStructureKind::Lab => (20, 20, 2),
        BaseStructureKind::Garden => (16, 12, 1),
        BaseStructureKind::Farm => (14, 18, 1),
        BaseStructureKind::Workshop => (28, 32, 3),
        BaseStructureKind::CityHall => (40, 50, 4),
        BaseStructureKind::RobotFoundry => (55, 70, 7),
        BaseStructureKind::Spaceport => (70, 90, 9),
        BaseStructureKind::Shipyard => (95, 120, 12),
        BaseStructureKind::DefenseGrid => (60, 85, 8),
    };
    vec![
        ItemQuantity::new("gear", gears * tier),
        ItemQuantity::new("scrap_metal", scrap * tier),
        ItemQuantity::new("energy_core", energy_cores * tier),
    ]
}

pub const fn settlement_strength_for(kind: BaseStructureKind) -> u8 {
    match kind {
        BaseStructureKind::Lab => 4,
        BaseStructureKind::Garden => 3,
        BaseStructureKind::Workshop => 5,
        BaseStructureKind::Farm => 4,
        BaseStructureKind::CityHall => 8,
        BaseStructureKind::RobotFoundry => 9,
        BaseStructureKind::Spaceport => 10,
        BaseStructureKind::Shipyard => 14,
        BaseStructureKind::DefenseGrid => 12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_owners() -> HeavyEconomyState {
        let mut state = HeavyEconomyState::new(0x5EED_CAFE);
        assert!(state.register_owner(OwnerId(0)));
        assert!(state.register_owner(OwnerId(1)));
        state
    }

    fn add(state: &mut HeavyEconomyState, owner: OwnerId, item_id: &str, quantity: u32) {
        state
            .owners
            .get_mut(&owner)
            .unwrap()
            .inventory
            .add_exact(item_id, quantity)
            .unwrap();
    }

    fn clear_environment(owner_position: Vec3Record) -> PlacementEnvironmentRecord {
        PlacementEnvironmentRecord {
            owner_position,
            terrain_height: 0.0,
            slope_degrees: 0.0,
            has_support: true,
            protected_zone: false,
            road_keepout: false,
            dynamic_obstruction: false,
        }
    }

    #[test]
    fn source_catalogs_are_complete_and_keep_authored_economy_values() {
        assert_eq!(RECIPE_CATALOG.len(), 25);
        assert_eq!(BLOCK_CATALOG.len(), 19);
        assert_eq!(PREFAB_CATALOG.len(), 16);
        assert_eq!(heavy_water_default_vendors().unwrap().len(), 5);

        let jewel = item_definition("power_jewel_flawless").unwrap();
        assert_eq!(jewel.max_stack, 9);
        assert_eq!(jewel.value, 4_000);
        let battleship = recipe_definition("battleship_spine").unwrap();
        assert_eq!(battleship.craft_seconds, 120);
        assert_eq!(battleship.required_level, 12);
        assert!(battleship
            .materials
            .contains(&Cost::new("scrap_metal", 150)));
        let city = build_definition("city_block_small").unwrap();
        assert_eq!(city.footprint.width, 14.0);
        assert_eq!(
            city.cost,
            &[
                Cost::new("scrap_metal", 60),
                Cost::new("circuit_board", 4),
                Cost::new("energy_core", 2),
                Cost::new("nano_fiber", 6),
            ]
        );
    }

    #[test]
    fn inventory_stacks_to_source_limits_and_bundle_failure_is_atomic() {
        let opaque_only_projection = InventoryRecord::new(0);
        assert_eq!(opaque_only_projection.validate(), Ok(()));
        assert_eq!(opaque_only_projection.can_add("scrap_metal", 1), Ok(false));

        let mut inventory = InventoryRecord::new(2);
        inventory.add_exact("scrap_metal", 120).unwrap();
        assert_eq!(inventory.item_count("scrap_metal"), 120);
        assert_eq!(inventory.occupied_slots(), 2);
        assert_eq!(inventory.slots[0].as_ref().unwrap().quantity, 99);
        assert_eq!(inventory.slots[1].as_ref().unwrap().quantity, 21);

        let before = inventory.clone();
        let error = inventory
            .apply_bundle_atomic(
                &[ItemQuantity::new("scrap_metal", 5)],
                &[ItemQuantity::new("energy_core", 1)],
            )
            .unwrap_err();
        assert!(matches!(error, EconomyError::InventoryFull { .. }));
        assert_eq!(inventory, before);

        inventory.remove_exact("scrap_metal", 21).unwrap();
        assert!(inventory.slots[1].is_none());
        inventory.add_exact("energy_core", 50).unwrap();
        inventory.validate().unwrap();
    }

    #[test]
    fn crafting_is_level_gated_owner_scoped_atomic_and_idempotent() {
        let mut state = state_with_owners();
        add(&mut state, OwnerId(0), "scrap_metal", 5);
        add(&mut state, OwnerId(0), "circuit_board", 1);

        let before = state.account(OwnerId(0)).unwrap().clone();
        let error = state
            .craft(OwnerId(0), 10, "damage_mod", 2, 1_000)
            .unwrap_err();
        assert!(matches!(error, EconomyError::InsufficientItem { .. }));
        assert_eq!(state.account(OwnerId(0)).unwrap(), &before);

        add(&mut state, OwnerId(0), "circuit_board", 1);
        let receipt = state.craft(OwnerId(0), 10, "damage_mod", 2, 1_000).unwrap();
        assert_eq!(receipt.completes_at_ms, 6_000);
        assert_eq!(
            state
                .account(OwnerId(0))
                .unwrap()
                .inventory
                .item_count("damage_mod"),
            1
        );
        assert_eq!(
            state
                .account(OwnerId(1))
                .unwrap()
                .inventory
                .item_count("damage_mod"),
            0
        );
        assert_eq!(
            state
                .craft(OwnerId(0), 10, "damage_mod", 2, 1_000)
                .unwrap_err(),
            EconomyError::AlreadyApplied(10)
        );
    }

    #[test]
    fn jewel_swap_and_unmount_never_destroy_or_duplicate_a_mount() {
        let mut state = state_with_owners();
        let account = state.owners.get_mut(&OwnerId(0)).unwrap();
        account.inventory = InventoryRecord::new(1);
        account.inventory.add_exact("power_jewel_cut", 2).unwrap();
        account
            .jewel_mounts
            .insert(WeaponSocket::Rifle, JewelTier::Rough);
        let before = account.clone();

        let error = state
            .mount_jewel(OwnerId(0), 20, WeaponSocket::Rifle, JewelTier::Cut)
            .unwrap_err();
        assert!(matches!(error, EconomyError::InventoryFull { .. }));
        assert_eq!(state.account(OwnerId(0)).unwrap(), &before);

        state.owners.get_mut(&OwnerId(0)).unwrap().inventory = InventoryRecord::new(2);
        add(&mut state, OwnerId(0), "power_jewel_cut", 1);
        state
            .mount_jewel(OwnerId(0), 21, WeaponSocket::Rifle, JewelTier::Cut)
            .unwrap();
        let account = state.account(OwnerId(0)).unwrap();
        assert_eq!(account.jewel_mounts[&WeaponSocket::Rifle], JewelTier::Cut);
        assert_eq!(account.inventory.item_count("power_jewel_rough"), 1);
        assert_eq!(account.inventory.item_count("power_jewel_cut"), 0);
        assert_eq!(account.mounted_damage_multiplier(WeaponSocket::Rifle), 1.30);

        state
            .unmount_jewel(OwnerId(0), 22, WeaponSocket::Rifle)
            .unwrap();
        let account = state.account(OwnerId(0)).unwrap();
        assert!(!account.jewel_mounts.contains_key(&WeaponSocket::Rifle));
        assert_eq!(account.inventory.item_count("power_jewel_cut"), 1);
    }

    #[test]
    fn vendor_buy_sell_and_restock_commit_wallet_inventory_and_stock_together() {
        let mut state = state_with_owners();
        let account = state.owners.get_mut(&OwnerId(0)).unwrap();
        account.credits = 1_000;
        account.inventory = InventoryRecord::new(1);
        account.inventory.add_exact("armor_plate_basic", 1).unwrap();
        let vendor_before = state.vendors["weapon_shop_1"].clone();
        let account_before = state.account(OwnerId(0)).unwrap().clone();
        let error = state
            .buy_from_vendor(OwnerId(0), 30, "weapon_shop_1", "plasma_cell", 5)
            .unwrap_err();
        assert!(matches!(error, EconomyError::InventoryFull { .. }));
        assert_eq!(state.account(OwnerId(0)).unwrap(), &account_before);
        assert_eq!(state.vendors["weapon_shop_1"], vendor_before);

        state.owners.get_mut(&OwnerId(0)).unwrap().inventory = InventoryRecord::new(3);
        let balance = state
            .buy_from_vendor(OwnerId(0), 31, "weapon_shop_1", "plasma_cell", 5)
            .unwrap();
        assert_eq!(balance, 960);
        assert_eq!(
            state
                .account(OwnerId(0))
                .unwrap()
                .inventory
                .item_count("plasma_cell"),
            5
        );
        let balance = state
            .sell_to_vendor(OwnerId(0), 32, "weapon_shop_1", "plasma_cell", 2)
            .unwrap();
        assert_eq!(balance, 964);
        assert!(state.restock_vendor("weapon_shop_1", 1).unwrap());
        assert!(!state.restock_vendor("weapon_shop_1", 1).unwrap());
        assert!(state.vendors["weapon_shop_1"]
            .items
            .iter()
            .all(|stock| stock.stock == stock.max_stock));
    }

    #[test]
    fn placement_records_validation_and_failed_attempts_do_not_spend_materials() {
        let mut state = state_with_owners();
        add(&mut state, OwnerId(0), "scrap_metal", 20);
        let environment = clear_environment(Vec3Record::ZERO);
        let placed = state
            .place_build(
                OwnerId(0),
                40,
                "metal_wall",
                Vec3Record::ZERO,
                0.0,
                environment.clone(),
                500,
            )
            .unwrap();
        assert!(placed.validation.accepted());
        assert_eq!(
            state
                .account(OwnerId(0))
                .unwrap()
                .inventory
                .item_count("scrap_metal"),
            16
        );

        let before = state.account(OwnerId(0)).unwrap().clone();
        let mut blocked_environment = environment.clone();
        blocked_environment.road_keepout = true;
        let error = state
            .place_build(
                OwnerId(0),
                41,
                "metal_wall",
                Vec3Record::ZERO,
                0.0,
                blocked_environment,
                600,
            )
            .unwrap_err();
        let EconomyError::PlacementRejected(validation) = error else {
            panic!("expected placement rejection");
        };
        assert!(validation.issues.contains(&PlacementIssue::RoadKeepout));
        assert!(validation.issues.contains(&PlacementIssue::OverlapsBuild));
        assert_eq!(state.account(OwnerId(0)).unwrap(), &before);
        assert_eq!(state.builds.builds.len(), 1);
        assert_eq!(
            state.remove_build(OwnerId(1), 42, placed.id).unwrap_err(),
            EconomyError::NotOwner
        );
    }

    #[test]
    fn lab_and_garden_upgrades_preserve_source_caps_and_cost_scaling() {
        let mut state = state_with_owners();
        for (item_id, quantity) in [
            ("scrap_metal", 200),
            ("circuit_board", 10),
            ("energy_core", 20),
            ("gear", 100),
        ] {
            add(&mut state, OwnerId(0), item_id, quantity);
        }
        let lab = state
            .place_build(
                OwnerId(0),
                50,
                "base_lab",
                Vec3Record::new(20.0, 0.0, 0.0),
                0.0,
                clear_environment(Vec3Record::new(20.0, 0.0, 0.0)),
                1_000,
            )
            .unwrap();
        assert_eq!(state.lab_companion_cap(OwnerId(0)), 3);
        assert_eq!(
            base_upgrade_cost(BaseStructureKind::Lab, 2),
            vec![
                ItemQuantity::new("gear", 20),
                ItemQuantity::new("scrap_metal", 20),
                ItemQuantity::new("energy_core", 2),
            ]
        );
        state
            .upgrade_base_structure(OwnerId(0), 51, lab.id)
            .unwrap();
        state
            .upgrade_base_structure(OwnerId(0), 52, lab.id)
            .unwrap();
        assert_eq!(state.lab_companion_cap(OwnerId(0)), 8);
        assert_eq!(state.settlement_strength(OwnerId(0)), 12);
        assert_eq!(
            state
                .upgrade_base_structure(OwnerId(0), 53, lab.id)
                .unwrap_err(),
            EconomyError::MaxBaseLevel
        );
    }

    #[test]
    fn mining_rewards_and_respawn_are_deterministic_and_owner_attributed() {
        let mut state = state_with_owners();
        let node = state
            .spawn_mining_node(MiningNodeKind::GearCache, Vec3Record::ZERO)
            .unwrap();
        let first = state
            .damage_mining_node(OwnerId(1), node, 20.0, 1_000)
            .unwrap();
        assert_eq!(
            first,
            MiningHitOutcome::Damaged {
                remaining_health: 50.0
            }
        );
        let shattered = state
            .damage_mining_node(OwnerId(1), node, 50.0, 2_000)
            .unwrap();
        let MiningHitOutcome::Shattered {
            owner,
            drops,
            respawn_at_ms,
            ..
        } = shattered
        else {
            panic!("expected shatter");
        };
        assert_eq!(owner, OwnerId(1));
        assert_eq!(respawn_at_ms, 62_000);
        assert_eq!(
            drops,
            DropBundle {
                items: vec![
                    ItemQuantity::new("gear", 6),
                    ItemQuantity::new("circuit_board", 2),
                    ItemQuantity::new("weapon_part_rifle", 1),
                ]
            }
        );
        assert_eq!(
            state
                .damage_mining_node(OwnerId(0), node, 1.0, 61_999)
                .unwrap(),
            MiningHitOutcome::Inactive {
                respawn_at_ms: 62_000
            }
        );
        assert_eq!(state.respawn_due_mining_nodes(62_000), vec![node]);

        assert_eq!(
            deterministic_mining_layout(77, 12),
            deterministic_mining_layout(77, 12)
        );
        assert_eq!(
            deterministic_salvage_drops(77, 9, 3),
            deterministic_salvage_drops(77, 9, 3)
        );
        let boss_vault = deterministic_vault_reward(77, 9, 3, true);
        assert_eq!(boss_vault.credits, 1_500);
        assert_eq!(boss_vault.drops.items[0], ItemQuantity::new("gear", 30));
        assert_eq!(boss_vault.jewels.first(), Some(&JewelTier::Rough));
        assert_eq!(
            deterministic_jewel_drops(77, 11, 2, JewelDropSource::BossCaptain),
            deterministic_jewel_drops(77, 11, 2, JewelDropSource::BossCaptain)
        );
        assert_eq!(
            deterministic_jewel_drops(77, 11, 2, JewelDropSource::BossCaptain).len(),
            1
        );
    }

    #[test]
    fn chest_claim_is_exactly_once_and_wallet_credit_is_owner_scoped() {
        let mut state = state_with_owners();
        let mut credit_chest = None;
        for index in 0..128 {
            let id = state
                .spawn_chest(Vec3Record::new(index as f32, 0.5, 0.0))
                .unwrap();
            if matches!(state.chests[&id].reward, ChestReward::Credits { .. }) {
                credit_chest = Some(id);
                break;
            }
        }
        let chest_id = credit_chest.expect("deterministic sequence should include a credit chest");
        let expected = state.chests[&chest_id].reward.clone();
        let reward = state.claim_chest(OwnerId(1), 60, chest_id, 9_000).unwrap();
        assert_eq!(reward, expected);
        let ChestReward::Credits { amount } = reward else {
            unreachable!();
        };
        assert_eq!(
            state.account(OwnerId(1)).unwrap().credits,
            u64::from(amount)
        );
        assert_eq!(state.account(OwnerId(0)).unwrap().credits, 0);
        assert_eq!(
            state
                .claim_chest(OwnerId(0), 61, chest_id, 9_001)
                .unwrap_err(),
            EconomyError::AlreadyClaimed
        );
    }

    #[test]
    fn serde_snapshot_round_trip_preserves_authoritative_state() {
        let mut state = state_with_owners();
        add(&mut state, OwnerId(0), "scrap_metal", 12);
        state.grant_credits_atomic(OwnerId(1), 70, 500).unwrap();
        state.seed_mining_nodes(4).unwrap();
        state.spawn_chest(Vec3Record::new(3.0, 0.5, -7.0)).unwrap();
        state.validate().unwrap();

        let json = serde_json::to_string(&state).unwrap();
        let restored: HeavyEconomyState = serde_json::from_str(&json).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored, state);
    }
}
