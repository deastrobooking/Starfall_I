//! Campaign tech upgrades for weapons, turrets, health, and future mech systems.

#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use serde::{Deserialize, Serialize};

use crate::world::robot_pets::{PartCost, RobotPartKind, RobotPetCollection};

pub const SABRE_RELIC_IDS: [&str; 13] = [
    "solar_sabre_glyph",
    "cyclone_slash_blueprint",
    "comet_dash_blueprint",
    "meteor_pound_blueprint",
    "rising_slash_blueprint",
    "spiral_slash_blueprint",
    "sabre_throw_blueprint",
    "tempest_wave_core",
    "solar_fire_gem",
    "storm_gem",
    "frost_gem",
    "void_gem",
    "legendary_starheart_gem",
];

/// The complete starter technique kit — the fighting moveset every player has
/// from their first minute, so the Saber's controls are learned once and never
/// change. Elemental gems, the thrown blade, and the tempest wave stay
/// exploration upgrades that layer on top.
pub const STARTER_SABRE_RELIC_IDS: [&str; 6] = [
    "solar_sabre_glyph",
    "cyclone_slash_blueprint",
    "comet_dash_blueprint",
    "meteor_pound_blueprint",
    "rising_slash_blueprint",
    "spiral_slash_blueprint",
];

pub fn is_sabre_relic(id: &str) -> bool {
    SABRE_RELIC_IDS.contains(&id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SabreRelicKind {
    Core,
    Technique,
    ElementalGem,
    LegendaryGem,
}

impl SabreRelicKind {
    /// ASCII shape layer remains readable with Bevy's fallback font and does
    /// not rely on color: circle=core, chevron=technique, diamond=gem, star=legendary.
    pub const fn shape(self) -> &'static str {
        match self {
            Self::Core => "(O)",
            Self::Technique => ">>",
            Self::ElementalGem => "<>",
            Self::LegendaryGem => "**",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SabreRelicDef {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: SabreRelicKind,
    pub effect: &'static str,
    pub input: &'static str,
    pub source_hint: &'static str,
}

pub const SABRE_RELIC_CATALOG: [SabreRelicDef; 13] = [
    SabreRelicDef {
        id: "solar_sabre_glyph",
        name: "Solar Sabre Glyph",
        kind: SabreRelicKind::Core,
        effect: "Upgrades the second-slash Star Wave into a wider, stronger multi-wave.",
        input: "RT / LMB while Saber is active",
        source_hint: "Solar Sabre Observatory",
    },
    SabreRelicDef {
        id: "cyclone_slash_blueprint",
        name: "Cyclone Slash Blueprint",
        kind: SabreRelicKind::Technique,
        effect: "Unlocks a wide 360-degree Saber strike.",
        input: "L3 / Heavy",
        source_hint: "Outer Solar Observatory",
    },
    SabreRelicDef {
        id: "comet_dash_blueprint",
        name: "Comet Dash Blueprint",
        kind: SabreRelicKind::Technique,
        effect: "Unlocks two advancing dash cuts.",
        input: "B / East / Q while grounded",
        source_hint: "Solar Observatory approach",
    },
    SabreRelicDef {
        id: "meteor_pound_blueprint",
        name: "Meteor Pound Blueprint",
        kind: SabreRelicKind::Technique,
        effect: "Converts the dash input into an airborne radial pound.",
        input: "B / East / Q while airborne",
        source_hint: "Solar Observatory ridge",
    },
    SabreRelicDef {
        id: "rising_slash_blueprint",
        name: "Rising Slash Blueprint",
        kind: SabreRelicKind::Technique,
        effect: "Unlocks a launching up-slash that opens aerial combos.",
        input: "Hold Up + RB while grounded",
        source_hint: "Skyward Trial spire",
    },
    SabreRelicDef {
        id: "spiral_slash_blueprint",
        name: "Spiral Slash Blueprint",
        kind: SabreRelicKind::Technique,
        effect: "Unlocks an airborne corkscrew cut that strikes twice.",
        input: "Heavy while airborne",
        source_hint: "Skyward Trial summit",
    },
    SabreRelicDef {
        id: "sabre_throw_blueprint",
        name: "Sabre Throw Blueprint",
        kind: SabreRelicKind::Technique,
        effect: "Hurls the blade as a returning projectile.",
        input: "Hold Down + RB while grounded",
        source_hint: "Rift Foundry vault",
    },
    SabreRelicDef {
        id: "tempest_wave_core",
        name: "Tempest Wave Core",
        kind: SabreRelicKind::Core,
        effect: "Adds a third-slash wave burst to the Saber chain.",
        input: "Third slash of the RB chain",
        source_hint: "Solar Sabre Observatory depths",
    },
    SabreRelicDef {
        id: "solar_fire_gem",
        name: "Solar Fire Gem",
        kind: SabreRelicKind::ElementalGem,
        effect: "Adds Fire damage and +12% Saber damage.",
        input: "Passive",
        source_hint: "Temple of the Sky Equation",
    },
    SabreRelicDef {
        id: "storm_gem",
        name: "Storm Gem",
        kind: SabreRelicKind::ElementalGem,
        effect: "Adds Electric damage and +12% Saber damage.",
        input: "Passive",
        source_hint: "Nova Missile Foundry",
    },
    SabreRelicDef {
        id: "frost_gem",
        name: "Frost Gem",
        kind: SabreRelicKind::ElementalGem,
        effect: "Adds +12% Saber damage; Frost status is a later upgrade.",
        input: "Passive",
        source_hint: "Sky Equation snowfield",
    },
    SabreRelicDef {
        id: "void_gem",
        name: "Void Gem",
        kind: SabreRelicKind::ElementalGem,
        effect: "Adds Rift damage and +12% Saber damage.",
        input: "Passive",
        source_hint: "Nova Foundry rift shelf",
    },
    SabreRelicDef {
        id: "legendary_starheart_gem",
        name: "Legendary Starheart Gem",
        kind: SabreRelicKind::LegendaryGem,
        effect: "Adds +35% Saber damage beyond elemental gem bonuses.",
        input: "Passive",
        source_hint: "Remote Everest landmark",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TechUpgradeId {
    BeamCapacitors,
    NovaMissileForge,
    SpriteTurretLattice,
    ArmorPlating,
    RejuvenationMatrix,
    MechCommandLink,
    MotionBootSuite,
    AegisArmorSuite,
    GauntletMatrix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TechUpgradeTrack {
    Weapons,
    Missiles,
    Turrets,
    Health,
    Mechs,
    Mobility,
    Armor,
    Gauntlets,
}

#[derive(Debug, Clone, Copy)]
pub struct TechUpgradeDef {
    pub id: TechUpgradeId,
    pub key_hint: &'static str,
    pub name: &'static str,
    pub track: TechUpgradeTrack,
    pub description: &'static str,
    pub max_rank: u32,
}

impl TechUpgradeId {
    pub const ALL: [TechUpgradeId; 9] = [
        TechUpgradeId::BeamCapacitors,
        TechUpgradeId::NovaMissileForge,
        TechUpgradeId::SpriteTurretLattice,
        TechUpgradeId::ArmorPlating,
        TechUpgradeId::RejuvenationMatrix,
        TechUpgradeId::MechCommandLink,
        TechUpgradeId::MotionBootSuite,
        TechUpgradeId::AegisArmorSuite,
        TechUpgradeId::GauntletMatrix,
    ];

    pub fn def(self) -> TechUpgradeDef {
        match self {
            TechUpgradeId::BeamCapacitors => TechUpgradeDef {
                id: self,
                key_hint: "Z",
                name: "Beam Capacitors",
                track: TechUpgradeTrack::Weapons,
                description: "+6% primary star-beam damage per rank.",
                max_rank: 5,
            },
            TechUpgradeId::NovaMissileForge => TechUpgradeDef {
                id: self,
                key_hint: "X",
                name: "Nova Missile Forge",
                track: TechUpgradeTrack::Missiles,
                description: "+10% explosive nova/missile damage per rank.",
                max_rank: 5,
            },
            TechUpgradeId::SpriteTurretLattice => TechUpgradeDef {
                id: self,
                key_hint: "C",
                name: "Sprite Turret Lattice",
                track: TechUpgradeTrack::Turrets,
                description: "+12% Sprite Turret damage per rank.",
                max_rank: 4,
            },
            TechUpgradeId::ArmorPlating => TechUpgradeDef {
                id: self,
                key_hint: "V",
                name: "Armor Plating",
                track: TechUpgradeTrack::Health,
                description: "+12 max HP through armor sync per rank.",
                max_rank: 5,
            },
            TechUpgradeId::RejuvenationMatrix => TechUpgradeDef {
                id: self,
                key_hint: "B",
                name: "Rejuvenation Matrix",
                track: TechUpgradeTrack::Health,
                description: "Adds paid healing reserve and improves regen efficiency.",
                max_rank: 5,
            },
            TechUpgradeId::MechCommandLink => TechUpgradeDef {
                id: self,
                key_hint: "N",
                name: "Mech Command Link",
                track: TechUpgradeTrack::Mechs,
                description: "Prepares robot pets for stronger vehicle/mech/ship forms.",
                max_rank: 4,
            },
            TechUpgradeId::MotionBootSuite => TechUpgradeDef {
                id: self,
                key_hint: "M",
                name: "Motion Boot Suite",
                track: TechUpgradeTrack::Mobility,
                description:
                    "Unlocks Speed Boots, Jump Boots, Rocket Blades, Blade Boots, and overdrive tuning.",
                max_rank: 5,
            },
            TechUpgradeId::AegisArmorSuite => TechUpgradeDef {
                id: self,
                key_hint: "L",
                name: "Aegis Armor Suite",
                track: TechUpgradeTrack::Armor,
                description:
                    "Adds shield sync, hardened plating, retaliation shock, and servo stat frames.",
                max_rank: 5,
            },
            TechUpgradeId::GauntletMatrix => TechUpgradeDef {
                id: self,
                key_hint: "K",
                name: "Gauntlet Matrix",
                track: TechUpgradeTrack::Gauntlets,
                description:
                    "Lets energy weapons inherit fire, electric, tracking, Sprite Burst, and rift effects.",
                max_rank: 5,
            },
        }
    }

    pub fn next_rank_cost(self, next_rank: u32) -> Vec<PartCost> {
        match self {
            TechUpgradeId::BeamCapacitors => vec![
                PartCost::new(RobotPartKind::CircuitBoard, 2 + next_rank * 2),
                PartCost::new(RobotPartKind::EnergyCore, next_rank),
            ],
            TechUpgradeId::NovaMissileForge => vec![
                PartCost::new(RobotPartKind::ScrapFrame, 4 + next_rank * 2),
                PartCost::new(RobotPartKind::EnergyCore, 1 + next_rank),
            ],
            TechUpgradeId::SpriteTurretLattice => vec![
                PartCost::new(RobotPartKind::CircuitBoard, 2 + next_rank),
                PartCost::new(RobotPartKind::ServoMotor, 2 + next_rank * 2),
            ],
            TechUpgradeId::ArmorPlating => vec![
                PartCost::new(RobotPartKind::ScrapFrame, 5 + next_rank * 3),
                PartCost::new(RobotPartKind::HullPlate, next_rank * 2),
            ],
            TechUpgradeId::RejuvenationMatrix => vec![
                PartCost::new(RobotPartKind::EnergyCore, 2 + next_rank),
                PartCost::new(RobotPartKind::CircuitBoard, next_rank),
            ],
            TechUpgradeId::MechCommandLink => vec![
                PartCost::new(RobotPartKind::ServoMotor, 4 + next_rank * 2),
                PartCost::new(RobotPartKind::StarDrive, next_rank),
                PartCost::new(RobotPartKind::CommandDeck, 1),
            ],
            TechUpgradeId::MotionBootSuite => {
                let mut cost = vec![
                    PartCost::new(RobotPartKind::ServoMotor, 3 + next_rank * 2),
                    PartCost::new(RobotPartKind::EnergyCore, 1 + next_rank),
                ];
                if next_rank >= 3 {
                    cost.push(PartCost::new(RobotPartKind::StarDrive, next_rank - 2));
                }
                cost
            }
            TechUpgradeId::AegisArmorSuite => {
                let mut cost = vec![
                    PartCost::new(RobotPartKind::HullPlate, 3 + next_rank * 3),
                    PartCost::new(RobotPartKind::ScrapFrame, 6 + next_rank * 2),
                ];
                if next_rank >= 2 {
                    cost.push(PartCost::new(RobotPartKind::EnergyCore, next_rank));
                }
                cost
            }
            TechUpgradeId::GauntletMatrix => {
                let mut cost = vec![
                    PartCost::new(RobotPartKind::CircuitBoard, 4 + next_rank * 2),
                    PartCost::new(RobotPartKind::EnergyCore, 1 + next_rank),
                ];
                if next_rank >= 4 {
                    cost.push(PartCost::new(RobotPartKind::ServoMotor, next_rank));
                }
                cost
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TechUpgradeError {
    MaxRank,
    MissingParts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechPurchaseReport {
    pub id: TechUpgradeId,
    pub new_rank: u32,
}

#[derive(bevy::prelude::Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpgradeLedger {
    pub ranks: Vec<(TechUpgradeId, u32)>,
    #[serde(default)]
    pub rejuvenation_charge: f32,
    /// Save-backed world relics, elemental gems, and technique blueprints.
    /// String ids keep authored discoveries extensible and old saves compatible.
    #[serde(default)]
    pub relics: Vec<String>,
}

impl UpgradeLedger {
    pub fn unlock_relic(&mut self, id: impl Into<String>) -> bool {
        let id = id.into();
        if self.relics.iter().any(|owned| owned == &id) {
            return false;
        }
        self.relics.push(id);
        self.relics.sort();
        true
    }

    pub fn has_relic(&self, id: &str) -> bool {
        self.relics.iter().any(|owned| owned == id)
    }

    pub fn sabre_wave_unlocked(&self) -> bool {
        self.has_relic("solar_sabre_glyph")
    }

    /// Tier zero is the starter second-slash wave. The Solar Glyph contributes
    /// the first upgrade and Beam Capacitors provide the remaining five tiers.
    pub fn sabre_wave_upgrade_tier(&self) -> u32 {
        u32::from(self.sabre_wave_unlocked()) + self.rank(TechUpgradeId::BeamCapacitors)
    }

    pub fn sabre_spin_unlocked(&self) -> bool {
        self.has_relic("cyclone_slash_blueprint")
    }

    pub fn sabre_dash_unlocked(&self) -> bool {
        self.has_relic("comet_dash_blueprint")
    }

    pub fn sabre_pound_unlocked(&self) -> bool {
        self.has_relic("meteor_pound_blueprint")
    }

    pub fn sabre_rising_unlocked(&self) -> bool {
        self.has_relic("rising_slash_blueprint")
    }

    pub fn sabre_spiral_unlocked(&self) -> bool {
        self.has_relic("spiral_slash_blueprint")
    }

    pub fn sabre_throw_unlocked(&self) -> bool {
        self.has_relic("sabre_throw_blueprint")
    }

    /// The Tempest Wave Core adds a third-slash wave burst on top of the
    /// second-slash wave every Saber ships with.
    pub fn sabre_third_slash_wave(&self) -> bool {
        self.has_relic("tempest_wave_core")
    }

    /// Whether a dodge press can resolve into a sabre technique right now:
    /// Comet Dash fires from any stance once its blueprint is owned, while
    /// Meteor Pound needs the player airborne. Both the technique trigger in
    /// `beam_sabre_update_system` and the dodge-roll suppression in
    /// `player_dodge_update` must share this predicate so a single press never
    /// fires both a roll and a technique.
    pub fn sabre_dodge_technique_applicable(&self, is_grounded: bool) -> bool {
        self.sabre_dash_unlocked() || (!is_grounded && self.sabre_pound_unlocked())
    }

    pub fn sabre_elemental_damage_mult(&self) -> f32 {
        let gems = ["solar_fire_gem", "storm_gem", "frost_gem", "void_gem"]
            .into_iter()
            .filter(|id| self.has_relic(id))
            .count() as f32;
        1.0 + gems * 0.12
            + if self.has_relic("legendary_starheart_gem") {
                0.35
            } else {
                0.0
            }
    }

    pub fn rank(&self, id: TechUpgradeId) -> u32 {
        self.ranks
            .iter()
            .find(|(upgrade_id, _)| *upgrade_id == id)
            .map(|(_, rank)| *rank)
            .unwrap_or(0)
    }

    pub fn try_purchase(
        &mut self,
        id: TechUpgradeId,
        robot_pets: &mut RobotPetCollection,
    ) -> Result<TechPurchaseReport, TechUpgradeError> {
        let def = id.def();
        let new_rank = self.rank(id) + 1;
        if new_rank > def.max_rank {
            return Err(TechUpgradeError::MaxRank);
        }

        let cost = id.next_rank_cost(new_rank);
        robot_pets
            .spend_parts(&cost)
            .map_err(|_| TechUpgradeError::MissingParts)?;

        if let Some((_, rank)) = self
            .ranks
            .iter_mut()
            .find(|(upgrade_id, _)| *upgrade_id == id)
        {
            *rank = new_rank;
        } else {
            self.ranks.push((id, new_rank));
            self.ranks.sort_by_key(|(upgrade_id, _)| *upgrade_id as u8);
        }

        if id == TechUpgradeId::RejuvenationMatrix {
            self.rejuvenation_charge += rejuvenation_charge_grant(new_rank);
        }

        Ok(TechPurchaseReport { id, new_rank })
    }

    pub fn beam_damage_mult(&self) -> f32 {
        1.0 + 0.06 * self.rank(TechUpgradeId::BeamCapacitors) as f32
    }

    pub fn missile_damage_mult(&self) -> f32 {
        1.0 + 0.10 * self.rank(TechUpgradeId::NovaMissileForge) as f32
    }

    pub fn turret_damage_mult(&self) -> f32 {
        1.0 + 0.12 * self.rank(TechUpgradeId::SpriteTurretLattice) as f32
    }

    pub fn armor_health_bonus(&self) -> f32 {
        12.0 * self.rank(TechUpgradeId::ArmorPlating) as f32
    }

    pub fn motion_boot_rank(&self) -> u32 {
        self.rank(TechUpgradeId::MotionBootSuite)
    }

    pub fn boot_ground_speed_mult(&self) -> f32 {
        1.0 + 0.06 * self.motion_boot_rank() as f32
    }

    pub fn boot_jump_mult(&self) -> f32 {
        let rank = self.motion_boot_rank();
        if rank >= 2 {
            1.0 + 0.08 + 0.02 * rank.saturating_sub(2) as f32
        } else {
            1.0
        }
    }

    pub fn boot_wall_jump_push_mult(&self) -> f32 {
        let rank = self.motion_boot_rank();
        if rank >= 2 {
            1.0 + 0.05 + 0.03 * rank.saturating_sub(2) as f32
        } else {
            1.0
        }
    }

    pub fn boot_air_motion_mult(&self) -> f32 {
        let rank = self.motion_boot_rank();
        if rank >= 3 {
            1.0 + 0.12 + 0.04 * rank.saturating_sub(3) as f32
        } else {
            1.0
        }
    }

    pub fn boot_jet_lift_mult(&self) -> f32 {
        let rank = self.motion_boot_rank();
        if rank >= 3 {
            1.0 + 0.08 + 0.03 * rank.saturating_sub(3) as f32
        } else {
            1.0
        }
    }

    pub fn blade_boot_rank(&self) -> u32 {
        self.motion_boot_rank().saturating_sub(3)
    }

    pub fn blade_boots_unlock_sabre(&self) -> bool {
        self.motion_boot_rank() >= 4
    }

    pub fn blade_boot_melee_damage_mult(&self) -> f32 {
        1.0 + 0.18 * self.blade_boot_rank() as f32
    }

    pub fn blade_boot_reach_bonus(&self) -> f32 {
        0.25 * self.blade_boot_rank() as f32
    }

    pub fn armor_suite_rank(&self) -> u32 {
        self.rank(TechUpgradeId::AegisArmorSuite)
    }

    pub fn armor_shield_defense_bonus(&self) -> f32 {
        let rank = self.armor_suite_rank();
        if rank >= 1 {
            15.0 + 10.0 * rank as f32
        } else {
            0.0
        }
    }

    pub fn armor_hardened_reduction(&self) -> f32 {
        let rank = self.armor_suite_rank();
        if rank >= 2 {
            (0.04 + 0.015 * rank.saturating_sub(2) as f32).min(0.12)
        } else {
            0.0
        }
    }

    pub fn armor_retaliation_damage(&self) -> f32 {
        let rank = self.armor_suite_rank();
        if rank >= 3 {
            14.0 + 6.0 * rank.saturating_sub(3) as f32
        } else {
            0.0
        }
    }

    pub fn armor_retaliation_radius(&self) -> f32 {
        let rank = self.armor_suite_rank();
        if rank >= 3 {
            3.6 + 0.45 * rank.saturating_sub(3) as f32
        } else {
            0.0
        }
    }

    pub fn armor_speed_mult(&self) -> f32 {
        let rank = self.armor_suite_rank();
        if rank >= 4 {
            1.0 + 0.04 + 0.02 * rank.saturating_sub(4) as f32
        } else {
            1.0
        }
    }

    pub fn armor_strength_mult(&self) -> f32 {
        let rank = self.armor_suite_rank();
        if rank >= 4 {
            1.0 + 0.06 + 0.03 * rank.saturating_sub(4) as f32
        } else {
            1.0
        }
    }

    pub fn armor_jump_mult(&self) -> f32 {
        let rank = self.armor_suite_rank();
        if rank >= 4 {
            1.0 + 0.05 + 0.02 * rank.saturating_sub(4) as f32
        } else {
            1.0
        }
    }

    pub fn gauntlet_rank(&self) -> u32 {
        self.rank(TechUpgradeId::GauntletMatrix)
    }

    pub fn gauntlet_has_fire(&self) -> bool {
        self.gauntlet_rank() >= 1
    }

    pub fn gauntlet_has_electric(&self) -> bool {
        self.gauntlet_rank() >= 2
    }

    pub fn gauntlet_has_tracking(&self) -> bool {
        self.gauntlet_rank() >= 3
    }

    pub fn gauntlet_has_sprite_burst(&self) -> bool {
        self.gauntlet_rank() >= 4
    }

    pub fn gauntlet_has_rift(&self) -> bool {
        self.gauntlet_rank() >= 5
    }

    pub fn gauntlet_energy_damage_mult(&self) -> f32 {
        1.0 + 0.04 * self.gauntlet_rank() as f32
    }

    pub fn gauntlet_explosive_damage_mult(&self) -> f32 {
        let rank = self.gauntlet_rank();
        if rank >= 3 {
            1.0 + 0.06 + 0.02 * rank.saturating_sub(3) as f32
        } else {
            1.0
        }
    }

    pub fn gauntlet_projectile_speed_mult(&self) -> f32 {
        let rank = self.gauntlet_rank();
        if rank >= 2 {
            1.0 + 0.04 + 0.02 * rank.saturating_sub(2) as f32
        } else {
            1.0
        }
    }

    pub fn gauntlet_explosion_radius_mult(&self) -> f32 {
        let rank = self.gauntlet_rank();
        if rank >= 3 {
            1.0 + 0.10 + 0.03 * rank.saturating_sub(3) as f32
        } else {
            1.0
        }
    }

    pub fn gauntlet_extra_pellets(&self) -> u32 {
        let rank = self.gauntlet_rank();
        if rank >= 4 {
            1 + rank.saturating_sub(4)
        } else {
            0
        }
    }

    pub fn gauntlet_aim_range_bonus(&self) -> f32 {
        let rank = self.gauntlet_rank();
        if rank >= 3 {
            8.0 + 2.0 * rank.saturating_sub(3) as f32
        } else {
            0.0
        }
    }

    pub fn gauntlet_aim_cone_relax(&self) -> f32 {
        let rank = self.gauntlet_rank();
        if rank >= 3 {
            0.035 + 0.010 * rank.saturating_sub(3) as f32
        } else {
            0.0
        }
    }

    pub fn gauntlet_aim_strength_bonus(&self) -> f32 {
        let rank = self.gauntlet_rank();
        if rank >= 3 {
            0.18 + 0.04 * rank.saturating_sub(3) as f32
        } else {
            0.0
        }
    }

    pub fn gauntlet_spread_mult(&self) -> f32 {
        if self.gauntlet_has_sprite_burst() {
            0.92
        } else {
            1.0
        }
    }

    pub fn rejuvenation_regen_per_sec(&self) -> f32 {
        0.25 * self.rank(TechUpgradeId::RejuvenationMatrix) as f32
    }

    pub fn rejuvenation_charge_per_hp(&self) -> f32 {
        (1.0 - 0.08 * self.rank(TechUpgradeId::RejuvenationMatrix) as f32).max(0.55)
    }

    pub fn consume_rejuvenation_for_heal(&mut self, requested_hp: f32) -> f32 {
        if requested_hp <= 0.0 || self.rejuvenation_charge <= 0.0 {
            return 0.0;
        }
        let charge_per_hp = self.rejuvenation_charge_per_hp();
        let affordable_hp = self.rejuvenation_charge / charge_per_hp;
        let healed_hp = requested_hp.min(affordable_hp);
        self.rejuvenation_charge = (self.rejuvenation_charge - healed_hp * charge_per_hp).max(0.0);
        healed_hp
    }
}

pub fn all_tech_upgrades() -> Vec<TechUpgradeDef> {
    TechUpgradeId::ALL
        .into_iter()
        .map(TechUpgradeId::def)
        .collect()
}

pub fn format_part_costs(costs: &[PartCost]) -> String {
    costs
        .iter()
        .map(|cost| format!("{} {}", cost.quantity, cost.kind.label()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn rejuvenation_charge_grant(rank: u32) -> f32 {
    65.0 + rank as f32 * 35.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::default;

    fn stocked_parts() -> RobotPetCollection {
        let mut robots = RobotPetCollection::default();
        for kind in RobotPartKind::ALL {
            robots.add_part(kind, 100);
        }
        robots
    }

    #[test]
    fn purchase_spends_parts_and_records_rank() {
        let mut upgrades = UpgradeLedger::default();
        let mut robots = stocked_parts();

        let report = upgrades
            .try_purchase(TechUpgradeId::BeamCapacitors, &mut robots)
            .expect("stocked parts should buy the upgrade");

        assert_eq!(report.new_rank, 1);
        assert_eq!(upgrades.rank(TechUpgradeId::BeamCapacitors), 1);
        assert_eq!(robots.part_count(RobotPartKind::CircuitBoard), 96);
        assert_eq!(robots.part_count(RobotPartKind::EnergyCore), 99);
    }

    #[test]
    fn missing_parts_block_purchase() {
        let mut upgrades = UpgradeLedger::default();
        let mut robots = RobotPetCollection::default();

        let result = upgrades.try_purchase(TechUpgradeId::NovaMissileForge, &mut robots);

        assert_eq!(result, Err(TechUpgradeError::MissingParts));
        assert_eq!(upgrades.rank(TechUpgradeId::NovaMissileForge), 0);
    }

    #[test]
    fn rejuvenation_purchase_adds_paid_charge_that_healing_consumes() {
        let mut upgrades = UpgradeLedger::default();
        let mut robots = stocked_parts();

        upgrades
            .try_purchase(TechUpgradeId::RejuvenationMatrix, &mut robots)
            .expect("stocked parts should buy rejuvenation");
        let starting_charge = upgrades.rejuvenation_charge;

        let healed = upgrades.consume_rejuvenation_for_heal(20.0);

        assert_eq!(upgrades.rank(TechUpgradeId::RejuvenationMatrix), 1);
        assert_eq!(healed, 20.0);
        assert!(upgrades.rejuvenation_charge < starting_charge);
    }

    #[test]
    fn max_rank_is_enforced() {
        let mut upgrades = UpgradeLedger::default();
        let mut robots = stocked_parts();

        for _ in 0..TechUpgradeId::SpriteTurretLattice.def().max_rank {
            upgrades
                .try_purchase(TechUpgradeId::SpriteTurretLattice, &mut robots)
                .expect("stocked parts should buy turret ranks");
        }

        assert_eq!(
            upgrades.try_purchase(TechUpgradeId::SpriteTurretLattice, &mut robots),
            Err(TechUpgradeError::MaxRank)
        );
    }

    #[test]
    fn equipment_suites_unlock_ranked_mechanics() {
        let upgrades = UpgradeLedger {
            ranks: vec![
                (TechUpgradeId::MotionBootSuite, 4),
                (TechUpgradeId::AegisArmorSuite, 5),
                (TechUpgradeId::GauntletMatrix, 5),
            ],
            ..default()
        };

        assert!(upgrades.blade_boots_unlock_sabre());
        assert!(upgrades.boot_ground_speed_mult() > 1.0);
        assert!(upgrades.armor_retaliation_damage() > 0.0);
        assert!(upgrades.armor_strength_mult() > 1.0);
        assert!(upgrades.gauntlet_has_rift());
        assert!(upgrades.gauntlet_extra_pellets() > 0);
    }

    #[test]
    fn sabre_relics_are_idempotent_and_compose_damage() {
        let mut upgrades = UpgradeLedger::default();
        assert!(upgrades.unlock_relic("solar_sabre_glyph"));
        assert!(!upgrades.unlock_relic("solar_sabre_glyph"));
        assert!(upgrades.sabre_wave_unlocked());

        upgrades.unlock_relic("cyclone_slash_blueprint");
        upgrades.unlock_relic("comet_dash_blueprint");
        upgrades.unlock_relic("meteor_pound_blueprint");
        upgrades.unlock_relic("solar_fire_gem");
        upgrades.unlock_relic("legendary_starheart_gem");

        assert!(upgrades.sabre_spin_unlocked());
        assert!(upgrades.sabre_dash_unlocked());
        assert!(upgrades.sabre_pound_unlocked());
        assert!(upgrades.sabre_elemental_damage_mult() > 1.45);
    }

    #[test]
    fn sabre_dodge_technique_applicability_tracks_stance_and_relics() {
        let mut upgrades = UpgradeLedger::default();
        // No blueprints: dodge stays a roll in every stance.
        assert!(!upgrades.sabre_dodge_technique_applicable(true));
        assert!(!upgrades.sabre_dodge_technique_applicable(false));

        // Meteor Pound alone only claims the press while airborne.
        upgrades.unlock_relic("meteor_pound_blueprint");
        assert!(!upgrades.sabre_dodge_technique_applicable(true));
        assert!(upgrades.sabre_dodge_technique_applicable(false));

        // Comet Dash claims it from any stance.
        upgrades.unlock_relic("comet_dash_blueprint");
        assert!(upgrades.sabre_dodge_technique_applicable(true));
        assert!(upgrades.sabre_dodge_technique_applicable(false));
    }

    #[test]
    fn sabre_relic_catalog_covers_every_save_id_once() {
        let mut catalog_ids: Vec<&str> = SABRE_RELIC_CATALOG.iter().map(|def| def.id).collect();
        catalog_ids.sort_unstable();
        catalog_ids.dedup();

        let mut save_ids = SABRE_RELIC_IDS.to_vec();
        save_ids.sort_unstable();
        assert_eq!(catalog_ids, save_ids);
        assert!(SABRE_RELIC_CATALOG.iter().all(|def| !def.effect.is_empty()
            && !def.input.is_empty()
            && !def.source_hint.is_empty()));
    }
}
