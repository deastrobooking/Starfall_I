#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::armor::ElementType;
use super::weapon::WeaponType;

/// A star-beam mod applied multiplicatively to the active primary weapon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponMod {
    pub id: String,
    pub name: String,
    pub damage_mult: f32,
    pub fire_rate_mult: f32,
    pub ammo_mult: f32,
    pub special: Option<WeaponSpecial>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WeaponSpecial {
    None,
    /// Adds a homing star tool to special slot 7 (chapter-2 discoverable).
    HomingStar,
    /// Star beams pierce one enemy.
    Piercing,
    /// Infuses fire/ice/etc. on hit.
    ElementalInfusion(ElementType),
}

impl WeaponMod {
    pub fn homing_star() -> Self {
        Self {
            id: "homing_star".into(),
            name: "Homing Star Focus".into(),
            damage_mult: 1.0,
            fire_rate_mult: 1.0,
            ammo_mult: 1.0,
            special: Some(WeaponSpecial::HomingStar),
        }
    }
    pub fn piercing_rounds() -> Self {
        Self {
            id: "piercing_rounds".into(),
            name: "Star Pierce".into(),
            damage_mult: 1.1,
            fire_rate_mult: 1.0,
            ammo_mult: 1.0,
            special: Some(WeaponSpecial::Piercing),
        }
    }
}

/// An armor mod slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmorMod {
    pub id: String,
    pub name: String,
    pub max_hp_bonus: f32,
    pub regen_per_sec: f32,
    pub element_resistance: Option<ElementType>,
}

impl ArmorMod {
    pub fn reactive_plating() -> Self {
        Self {
            id: "reactive_plating".into(),
            name: "Reactive Plating".into(),
            max_hp_bonus: 25.0,
            regen_per_sec: 0.0,
            element_resistance: None,
        }
    }
    pub fn coolant_weave() -> Self {
        Self {
            id: "coolant_weave".into(),
            name: "Coolant Weave".into(),
            max_hp_bonus: 10.0,
            regen_per_sec: 1.5,
            element_resistance: Some(ElementType::Fire),
        }
    }
}

/// Player-wide loadout. One mod per primary weapon, up to 3 armor mods.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerLoadout {
    pub weapon_mods: HashMap<String, WeaponMod>, // key: weapon-type display name
    pub armor_mods: Vec<ArmorMod>,
    pub blueprints: Vec<String>,
}

impl PlayerLoadout {
    pub fn weapon_mod_for(&self, w: WeaponType) -> Option<&WeaponMod> {
        self.weapon_mods.get(w.display_name())
    }
    pub fn equip_weapon_mod(&mut self, w: WeaponType, m: WeaponMod) {
        self.weapon_mods.insert(w.display_name().to_string(), m);
    }
    pub fn add_armor_mod(&mut self, m: ArmorMod) {
        if self.armor_mods.len() < 3 {
            self.armor_mods.push(m);
        }
    }
    pub fn has_blueprint(&self, id: &str) -> bool {
        self.blueprints.iter().any(|b| b == id)
    }
    pub fn add_blueprint(&mut self, id: impl Into<String>) {
        let id = id.into();
        if !self.has_blueprint(&id) {
            self.blueprints.push(id);
        }
    }
    /// Aggregate effective max-HP bonus from armor mods.
    pub fn total_armor_hp_bonus(&self) -> f32 {
        self.armor_mods.iter().map(|m| m.max_hp_bonus).sum()
    }
}
