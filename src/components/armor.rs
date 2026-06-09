use bevy::prelude::*;

// ── Element Type ──────────────────────────────────────────────────────────────
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum ElementType {
    #[default]
    None,
    Fire,
    Ice,
    Electric,
    DarkEnergy,
    Rift,
}

impl ElementType {
    pub fn display_name(&self) -> &'static str {
        match self {
            ElementType::None => "None",
            ElementType::Fire => "Fire",
            ElementType::Ice => "Ice",
            ElementType::Electric => "Electric",
            ElementType::DarkEnergy => "Dark Energy",
            ElementType::Rift => "Rift",
        }
    }

    /// Strength bonus (outgoing damage multiplier).
    pub fn strength_bonus(&self) -> f32 {
        match self {
            ElementType::None => 0.0,
            ElementType::Fire => 0.15,
            ElementType::Ice => 0.05,
            ElementType::Electric => 0.10,
            ElementType::DarkEnergy => 0.20,
            ElementType::Rift => 0.08,
        }
    }

    /// Defense bonus (incoming damage reduction).
    pub fn defense_bonus(&self) -> f32 {
        match self {
            ElementType::None => 0.0,
            ElementType::Fire => 0.05,
            ElementType::Ice => 0.15,
            ElementType::Electric => 0.10,
            ElementType::DarkEnergy => 0.0,
            ElementType::Rift => 0.20,
        }
    }

    /// Poison DPS.
    pub fn poison_dps(&self) -> f32 {
        match self {
            ElementType::None => 0.0,
            ElementType::Fire => 8.0,
            ElementType::Ice => 4.0,
            ElementType::Electric => 12.0,
            ElementType::DarkEnergy => 15.0,
            ElementType::Rift => 6.0,
        }
    }

    /// Poison duration.
    pub fn poison_duration(&self) -> f32 {
        match self {
            ElementType::None => 0.0,
            ElementType::Fire => 3.0,
            ElementType::Ice => 5.0,
            ElementType::Electric => 2.0,
            ElementType::DarkEnergy => 4.0,
            ElementType::Rift => 6.0,
        }
    }
}

// ── Armor Slot ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArmorSlot {
    Helmet,
    Chest,
    Legs,
    Boots,
}

// ── Armor Tier ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArmorTier {
    Iron,
    Steel,
    Titanium,
    Plasma,
    Quantum,
}

impl ArmorTier {
    pub fn defense(&self, slot: ArmorSlot) -> f32 {
        let chest_base = match self {
            ArmorTier::Iron => 8.0,
            ArmorTier::Steel => 16.0,
            ArmorTier::Titanium => 28.0,
            ArmorTier::Plasma => 42.0,
            ArmorTier::Quantum => 60.0,
        };
        let slot_mult = match slot {
            ArmorSlot::Chest => 1.0,
            ArmorSlot::Legs => 0.8,
            ArmorSlot::Helmet => 0.6,
            ArmorSlot::Boots => 0.5,
        };
        chest_base * slot_mult
    }

    pub fn health_bonus(&self, slot: ArmorSlot) -> f32 {
        let chest_base = match self {
            ArmorTier::Iron => 15.0,
            ArmorTier::Steel => 30.0,
            ArmorTier::Titanium => 50.0,
            ArmorTier::Plasma => 75.0,
            ArmorTier::Quantum => 100.0,
        };
        let slot_mult = match slot {
            ArmorSlot::Chest => 1.0,
            ArmorSlot::Legs => 0.7,
            ArmorSlot::Helmet => 0.5,
            ArmorSlot::Boots => 0.3,
        };
        chest_base * slot_mult
    }
}

// ── Armor Piece ───────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct ArmorPiece {
    pub slot: ArmorSlot,
    pub tier: ArmorTier,
    pub defense: f32,
    pub health_bonus: f32,
    pub stamina_bonus: f32,
}

impl ArmorPiece {
    pub fn new(slot: ArmorSlot, tier: ArmorTier) -> Self {
        Self {
            slot,
            defense: tier.defense(slot),
            health_bonus: tier.health_bonus(slot),
            stamina_bonus: 5.0,
            tier,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmorUpgradeState {
    pub shield_defense_bonus: f32,
    pub hardened_reduction: f32,
    pub retaliation_damage: f32,
    pub retaliation_radius: f32,
    pub speed_mult: f32,
    pub strength_mult: f32,
    pub jump_mult: f32,
}

impl Default for ArmorUpgradeState {
    fn default() -> Self {
        Self {
            shield_defense_bonus: 0.0,
            hardened_reduction: 0.0,
            retaliation_damage: 0.0,
            retaliation_radius: 0.0,
            speed_mult: 1.0,
            strength_mult: 1.0,
            jump_mult: 1.0,
        }
    }
}

// ── Armor Set (on Player) ─────────────────────────────────────────────────────
#[derive(Component, Debug, Clone, Default)]
pub struct ArmorSet {
    pub helmet: Option<ArmorPiece>,
    pub chest: Option<ArmorPiece>,
    pub legs: Option<ArmorPiece>,
    pub boots: Option<ArmorPiece>,
    pub active_element: ElementType,
    pub upgrade_state: ArmorUpgradeState,
}

impl ArmorSet {
    pub fn total_defense(&self) -> f32 {
        let base = [&self.helmet, &self.chest, &self.legs, &self.boots]
            .iter()
            .filter_map(|s| s.as_ref())
            .map(|p| p.defense)
            .sum::<f32>();
        // Apply elemental defense bonus
        (base + self.upgrade_state.shield_defense_bonus)
            * (1.0 + self.active_element.defense_bonus())
    }

    pub fn total_health_bonus(&self) -> f32 {
        [&self.helmet, &self.chest, &self.legs, &self.boots]
            .iter()
            .filter_map(|s| s.as_ref())
            .map(|p| p.health_bonus)
            .sum()
    }

    pub fn total_stamina_bonus(&self) -> f32 {
        [&self.helmet, &self.chest, &self.legs, &self.boots]
            .iter()
            .filter_map(|s| s.as_ref())
            .map(|p| p.stamina_bonus)
            .sum()
    }

    /// Compute final incoming damage after armor reduction.
    /// Formula: baseReduction = totalDefense / (totalDefense + 100)
    pub fn calculate_damage_reduction(&self, damage: f32) -> f32 {
        let def = self.total_defense();
        let base_reduction = def / (def + 100.0);
        let hardened = self.upgrade_state.hardened_reduction.clamp(0.0, 0.35);
        let reduced = damage * (1.0 - base_reduction) * (1.0 - hardened);
        reduced.max(1.0)
    }

    /// Outgoing damage with elemental strength bonus.
    pub fn modified_outgoing_damage(&self, base: f32) -> f32 {
        base * (1.0 + self.active_element.strength_bonus()) * self.upgrade_state.strength_mult
    }

    pub fn movement_speed_mult(&self) -> f32 {
        self.upgrade_state.speed_mult
    }

    pub fn jump_mult(&self) -> f32 {
        self.upgrade_state.jump_mult
    }

    pub fn retaliation_damage(&self) -> f32 {
        self.upgrade_state.retaliation_damage
    }

    pub fn retaliation_radius(&self) -> f32 {
        self.upgrade_state.retaliation_radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armor_upgrade_state_shapes_defense_and_damage() {
        let mut armor = ArmorSet {
            active_element: ElementType::Fire,
            upgrade_state: ArmorUpgradeState {
                shield_defense_bonus: 40.0,
                hardened_reduction: 0.10,
                strength_mult: 1.12,
                ..default()
            },
            ..default()
        };
        armor.chest = Some(ArmorPiece::new(ArmorSlot::Chest, ArmorTier::Steel));

        assert!(armor.total_defense() > 40.0);
        assert!(armor.calculate_damage_reduction(50.0) < 50.0);
        assert!(armor.modified_outgoing_damage(10.0) > 11.0);
    }
}
