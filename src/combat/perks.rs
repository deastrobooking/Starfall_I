//! Perk tree - Heart / Star / Acrobat branches.
//! Each level-up grants 1 unspent point; spend in chapter-select UI.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PerkBranch {
    Heart,   // HP / healing
    Star,    // Beam damage / charges
    Acrobat, // Dodge / parry / stamina
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerkDef {
    pub id: &'static str,
    pub name: &'static str,
    pub branch: PerkBranch,
    pub description: &'static str,
    pub max_rank: u32,
}

pub fn all_perks() -> Vec<PerkDef> {
    vec![
        PerkDef {
            id: "heart_vitality",
            name: "Family Vitality",
            branch: PerkBranch::Heart,
            description: "+15 max HP per rank.",
            max_rank: 5,
        },
        PerkDef {
            id: "heart_regen",
            name: "Second Wind",
            branch: PerkBranch::Heart,
            description: "+0.5 HP/sec while rejuvenation reserve is available.",
            max_rank: 4,
        },
        PerkDef {
            id: "star_focus",
            name: "Star Focus",
            branch: PerkBranch::Star,
            description: "+5% beam damage per rank.",
            max_rank: 5,
        },
        PerkDef {
            id: "star_charges",
            name: "Pocket Constellation",
            branch: PerkBranch::Star,
            description: "+15% max charges per rank.",
            max_rank: 3,
        },
        PerkDef {
            id: "acro_evasion",
            name: "Wall-Dancer Evasion",
            branch: PerkBranch::Acrobat,
            description: "-10% dodge stamina cost.",
            max_rank: 3,
        },
        PerkDef {
            id: "acro_parry",
            name: "Lucky Parry",
            branch: PerkBranch::Acrobat,
            description: "+0.05s parry window per rank.",
            max_rank: 3,
        },
    ]
}

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerkTree {
    pub points_unspent: u32,
    pub ranks: Vec<(String, u32)>, // (perk id, rank)
}

impl PerkTree {
    pub fn rank(&self, perk_id: &str) -> u32 {
        self.ranks
            .iter()
            .find(|(id, _)| id == perk_id)
            .map(|(_, r)| *r)
            .unwrap_or(0)
    }
    pub fn try_spend(&mut self, perk_id: &str) -> bool {
        if self.points_unspent == 0 {
            return false;
        }
        let perks = all_perks();
        let Some(def) = perks.iter().find(|p| p.id == perk_id) else {
            return false;
        };
        let cur = self.rank(perk_id);
        if cur >= def.max_rank {
            return false;
        }
        if let Some(entry) = self.ranks.iter_mut().find(|(id, _)| id == perk_id) {
            entry.1 += 1;
        } else {
            self.ranks.push((perk_id.to_string(), 1));
        }
        self.points_unspent -= 1;
        true
    }
    pub fn award(&mut self, points: u32) {
        self.points_unspent += points;
    }
    pub fn damage_mult(&self) -> f32 {
        1.0 + 0.05 * self.rank("star_focus") as f32
    }
    #[allow(dead_code)] // orphaned by the unlimited-ammo change; retained for ammo-cap re-enable
    pub fn ammo_mult(&self) -> f32 {
        1.0 + 0.15 * self.rank("star_charges") as f32
    }
    pub fn hp_bonus(&self) -> f32 {
        15.0 * self.rank("heart_vitality") as f32
    }
    pub fn regen_per_sec(&self) -> f32 {
        0.5 * self.rank("heart_regen") as f32
    }
    pub fn dodge_cost_mult(&self) -> f32 {
        1.0 - 0.10 * self.rank("acro_evasion") as f32
    }
    pub fn parry_window_bonus(&self) -> f32 {
        0.05 * self.rank("acro_parry") as f32
    }
}
