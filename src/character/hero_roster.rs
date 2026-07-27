#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use bevy::prelude::*;

use crate::components::player::{DodgeState, JetpackState, PlayerMovement, PlayerStats};
use crate::components::weapon::{
    MeleeCombo, SpecialSlot, SpecialWeapon, SpecialWeaponInventory, WeaponInventory, WeaponType,
};
use crate::world::robot_pets::{RobotAssemblyForm, RobotPetCollection, RobotPetRole};

pub const HERO_NAMES: [&str; 8] = [
    "Vincenzo",
    "Antonio",
    "Angelo",
    "Joseph",
    "Gabriella",
    "Nova",
    "Aurora",
    "Fortuna",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroFamilyRole {
    Brother,
    Sister,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HeroPowerSet {
    pub speed: f32,
    pub strength: f32,
    pub flight: f32,
    pub magic: f32,
}

impl HeroPowerSet {
    pub const fn new(speed: f32, strength: f32, flight: f32, magic: f32) -> Self {
        Self {
            speed,
            strength,
            flight,
            magic,
        }
    }

    pub const fn one() -> Self {
        Self::new(1.0, 1.0, 1.0, 1.0)
    }

    fn bonus_zero() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    fn clamped(self, min: f32, max: f32) -> Self {
        Self {
            speed: self.speed.clamp(min, max),
            strength: self.strength.clamp(min, max),
            flight: self.flight.clamp(min, max),
            magic: self.magic.clamp(min, max),
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct HeroPowerProfile {
    pub name: &'static str,
    pub role: HeroFamilyRole,
    pub unique_weapon_name: &'static str,
    pub primary_weapon: WeaponType,
    pub special_slot: SpecialSlot,
    pub special_name: &'static str,
    pub core: HeroPowerSet,
    pub robot_affinity: HeroPowerSet,
}

impl HeroPowerProfile {
    pub fn amplified_powers(self, robots: &RobotPetCollection) -> HeroPowerSet {
        let bonus = robot_pet_power_bonus(robots);
        HeroPowerSet {
            speed: self.core.speed + bonus.speed * self.robot_affinity.speed,
            strength: self.core.strength + bonus.strength * self.robot_affinity.strength,
            flight: self.core.flight + bonus.flight * self.robot_affinity.flight,
            magic: self.core.magic + bonus.magic * self.robot_affinity.magic,
        }
        .clamped(0.85, 1.55)
    }
}

pub fn hero_power_profile(name: &str) -> HeroPowerProfile {
    match name {
        "Vincenzo" => HeroPowerProfile {
            name: "Vincenzo",
            role: HeroFamilyRole::Brother,
            unique_weapon_name: "Guardian Comet Lance",
            primary_weapon: WeaponType::Rifle,
            special_slot: SpecialSlot::Slot9,
            special_name: "Family Aegis Moon",
            core: HeroPowerSet::new(1.03, 1.10, 1.00, 1.04),
            robot_affinity: HeroPowerSet::new(0.90, 1.12, 0.95, 1.00),
        },
        "Antonio" => HeroPowerProfile {
            name: "Antonio",
            role: HeroFamilyRole::Brother,
            unique_weapon_name: "Rift Skater Star Fans",
            primary_weapon: WeaponType::Shotgun,
            special_slot: SpecialSlot::Slot7,
            special_name: "Shortcut Homing Star",
            core: HeroPowerSet::new(1.15, 0.98, 1.08, 1.02),
            robot_affinity: HeroPowerSet::new(1.18, 0.92, 1.08, 1.00),
        },
        "Angelo" => HeroPowerProfile {
            name: "Angelo",
            role: HeroFamilyRole::Brother,
            unique_weapon_name: "Harmonic Star Hammer",
            primary_weapon: WeaponType::Grenade,
            special_slot: SpecialSlot::Slot0,
            special_name: "Bridge Sprite Turret",
            core: HeroPowerSet::new(1.04, 1.07, 1.02, 1.11),
            robot_affinity: HeroPowerSet::new(0.98, 1.02, 1.00, 1.18),
        },
        "Joseph" => HeroPowerProfile {
            name: "Joseph",
            role: HeroFamilyRole::Brother,
            unique_weapon_name: "Little Joe Nova Gauntlets",
            primary_weapon: WeaponType::Rocket,
            special_slot: SpecialSlot::Slot8,
            special_name: "Pocket Tri-Star Burst",
            core: HeroPowerSet::new(1.00, 1.17, 0.99, 1.01),
            robot_affinity: HeroPowerSet::new(0.95, 1.20, 0.96, 1.00),
        },
        "Gabriella" => HeroPowerProfile {
            name: "Gabriella",
            role: HeroFamilyRole::Sister,
            unique_weapon_name: "Prism Crown Staff",
            primary_weapon: WeaponType::Laser,
            special_slot: SpecialSlot::Slot9,
            special_name: "Prism Moon Bloom",
            core: HeroPowerSet::new(1.04, 1.01, 1.08, 1.14),
            robot_affinity: HeroPowerSet::new(1.00, 0.98, 1.08, 1.16),
        },
        "Nova" => HeroPowerProfile {
            name: "Nova",
            role: HeroFamilyRole::Sister,
            unique_weapon_name: "Nova Rainbow Ray",
            primary_weapon: WeaponType::Laser,
            special_slot: SpecialSlot::Slot8,
            special_name: "Spectrum Tri-Star",
            core: HeroPowerSet::new(1.07, 1.00, 1.06, 1.16),
            robot_affinity: HeroPowerSet::new(1.02, 0.96, 1.04, 1.20),
        },
        "Aurora" => HeroPowerProfile {
            name: "Aurora",
            role: HeroFamilyRole::Sister,
            unique_weapon_name: "Aurora Halo Guard",
            primary_weapon: WeaponType::Pistol,
            special_slot: SpecialSlot::Slot0,
            special_name: "Halo Sprite Bastion",
            core: HeroPowerSet::new(1.02, 1.06, 1.05, 1.10),
            robot_affinity: HeroPowerSet::new(0.98, 1.08, 1.04, 1.10),
        },
        "Fortuna" => HeroPowerProfile {
            name: "Fortuna",
            role: HeroFamilyRole::Sister,
            unique_weapon_name: "Fortune Star Cards",
            primary_weapon: WeaponType::Shotgun,
            special_slot: SpecialSlot::Slot7,
            special_name: "Lucky Homing Draw",
            core: HeroPowerSet::new(1.09, 0.99, 1.04, 1.12),
            robot_affinity: HeroPowerSet::new(1.12, 0.98, 1.02, 1.14),
        },
        _ => HeroPowerProfile {
            name: "Starfall Hero",
            role: HeroFamilyRole::Brother,
            unique_weapon_name: "Starfall Beam Glove",
            primary_weapon: WeaponType::Pistol,
            special_slot: SpecialSlot::Slot7,
            special_name: "Homing Star",
            core: HeroPowerSet::one(),
            robot_affinity: HeroPowerSet::one(),
        },
    }
}

pub fn apply_hero_runtime(
    profile: HeroPowerProfile,
    powers: HeroPowerSet,
    stats: &mut PlayerStats,
    base_stats: &mut crate::components::player::PlayerBaseStats,
    movement: &mut PlayerMovement,
    jetpack: &mut JetpackState,
    dodge: &mut DodgeState,
    weapons: &mut WeaponInventory,
    specials: &mut SpecialWeaponInventory,
    melee: &mut MeleeCombo,
) {
    movement.walk_speed *= powers.speed;
    movement.sprint_speed *= powers.speed;
    movement.air_accel *= (powers.speed + powers.flight) * 0.5;
    movement.jump_force *= (powers.flight * 0.65 + powers.strength * 0.35).clamp(0.85, 1.35);
    dodge.dodge_speed *= powers.speed;

    let stamina_mult = ((powers.speed + powers.flight) * 0.5).clamp(0.85, 1.40);
    base_stats.max_health *= powers.strength.clamp(0.90, 1.35);
    stats.max_stamina *= stamina_mult;
    stats.stamina = stats.max_stamina;
    base_stats.max_armor *= (0.80 + powers.strength * 0.20).clamp(0.90, 1.25);

    jetpack.max_fuel *= powers.flight;
    jetpack.fuel = jetpack.max_fuel;
    jetpack.force *= (0.75 + powers.flight * 0.25).clamp(0.90, 1.25);
    jetpack.regen_rate *= powers.magic.clamp(0.90, 1.30);
    jetpack.max_vertical_vel *= powers.flight.clamp(0.90, 1.28);

    melee.damage_multiplier *= powers.strength.clamp(0.90, 1.35);
    weapons.active_slot = weapon_slot(profile.primary_weapon);
    for weapon in &mut weapons.slots {
        weapon.damage *= powers.strength.clamp(0.90, 1.35);
        weapon.speed *= powers.magic.clamp(0.90, 1.25);
    }

    tune_signature_special(
        specials,
        profile.special_slot,
        profile.special_name,
        powers.magic,
    );
    for special in [
        &mut specials.slot7,
        &mut specials.slot8,
        &mut specials.slot9,
        &mut specials.slot0,
    ] {
        special.base_damage *= powers.magic.clamp(0.90, 1.40);
    }
}

fn tune_signature_special(
    specials: &mut SpecialWeaponInventory,
    slot: SpecialSlot,
    name: &'static str,
    magic: f32,
) {
    let special = special_mut(specials, slot);
    special.name = name;
    special.base_damage *= 1.12;
    special.cooldown *= (1.0 / magic.clamp(0.95, 1.25)).clamp(0.78, 1.05);
    special.max_ammo = ((special.max_ammo as f32 * magic.clamp(1.0, 1.25)).round() as u32).max(1);
    special.ammo = special.max_ammo;
}

fn special_mut(specials: &mut SpecialWeaponInventory, slot: SpecialSlot) -> &mut SpecialWeapon {
    match slot {
        SpecialSlot::Slot7 => &mut specials.slot7,
        SpecialSlot::Slot8 => &mut specials.slot8,
        SpecialSlot::Slot9 => &mut specials.slot9,
        SpecialSlot::Slot0 => &mut specials.slot0,
    }
}

fn weapon_slot(weapon: WeaponType) -> usize {
    match weapon {
        WeaponType::Pistol => 0,
        WeaponType::Rifle => 1,
        WeaponType::Shotgun => 2,
        WeaponType::Rocket => 3,
        WeaponType::Laser => 4,
        WeaponType::Grenade => 5,
    }
}

fn robot_pet_power_bonus(robots: &RobotPetCollection) -> HeroPowerSet {
    let mut bonus = HeroPowerSet::bonus_zero();
    for pet in &robots.pets {
        let quality = 1.0 + pet.level as f32 * 0.04 + pet.affection.min(100) as f32 * 0.0015;
        match pet.role {
            RobotPetRole::Scout => {
                bonus.speed += 0.018 * quality;
                bonus.magic += 0.006 * quality;
            }
            RobotPetRole::Healer => {
                bonus.magic += 0.016 * quality;
                bonus.flight += 0.008 * quality;
            }
            RobotPetRole::Defender => {
                bonus.strength += 0.020 * quality;
                bonus.flight += 0.004 * quality;
            }
            RobotPetRole::Builder => {
                bonus.strength += 0.010 * quality;
                bonus.magic += 0.010 * quality;
            }
            RobotPetRole::Pilot => {
                bonus.flight += 0.020 * quality;
                bonus.speed += 0.008 * quality;
            }
        }
    }

    if let Some(assembly) = &robots.active_assembly {
        match assembly.form {
            RobotAssemblyForm::Car | RobotAssemblyForm::Motorcycle => bonus.speed += 0.06,
            RobotAssemblyForm::JetBike => {
                bonus.speed += 0.05;
                bonus.flight += 0.07;
            }
            RobotAssemblyForm::Tank => bonus.strength += 0.08,
            RobotAssemblyForm::Boat | RobotAssemblyForm::Submarine => bonus.magic += 0.05,
            RobotAssemblyForm::SpaceJet => bonus.flight += 0.10,
            RobotAssemblyForm::GiantMech => bonus.strength += 0.12,
            RobotAssemblyForm::SpaceShip => {
                bonus.flight += 0.10;
                bonus.magic += 0.08;
            }
            RobotAssemblyForm::MegaShip => {
                bonus.speed += 0.08;
                bonus.strength += 0.10;
                bonus.flight += 0.12;
                bonus.magic += 0.12;
            }
        }
    }

    HeroPowerSet {
        speed: bonus.speed.min(0.35),
        strength: bonus.strength.min(0.35),
        flight: bonus.flight.min(0.35),
        magic: bonus.magic.min(0.35),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::robot_pets::{RobotPetBlueprint, RobotPetRole};

    #[test]
    fn all_humans_have_unique_signature_names() {
        for (index, name) in HERO_NAMES.iter().enumerate() {
            let profile = hero_power_profile(name);
            assert_eq!(profile.name, *name);
            assert!(profile.core.speed > 0.0);
            assert!(profile.core.strength > 0.0);
            assert!(profile.core.flight > 0.0);
            assert!(profile.core.magic > 0.0);

            for other in HERO_NAMES.iter().skip(index + 1) {
                let other = hero_power_profile(other);
                assert_ne!(profile.unique_weapon_name, other.unique_weapon_name);
                assert_ne!(profile.special_name, other.special_name);
            }
        }
    }

    #[test]
    fn robot_pets_amplify_core_powers() {
        let profile = hero_power_profile("Antonio");
        let base = profile.amplified_powers(&RobotPetCollection::default());
        let mut robots = RobotPetCollection::default();
        robots.rescue_pet(RobotPetBlueprint::rescued(
            "spark",
            "Spark Pup",
            RobotPetRole::Scout,
        ));

        let boosted = profile.amplified_powers(&robots);

        assert!(boosted.speed > base.speed);
        assert!(boosted.magic > base.magic);
    }
}
