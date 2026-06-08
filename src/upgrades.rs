//! Campaign tech upgrades for weapons, turrets, health, and future mech systems.

use serde::{Deserialize, Serialize};

use crate::robot_pets::{PartCost, RobotPartKind, RobotPetCollection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TechUpgradeId {
    BeamCapacitors,
    NovaMissileForge,
    SpriteTurretLattice,
    ArmorPlating,
    RejuvenationMatrix,
    MechCommandLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TechUpgradeTrack {
    Weapons,
    Missiles,
    Turrets,
    Health,
    Mechs,
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
    pub const ALL: [TechUpgradeId; 6] = [
        TechUpgradeId::BeamCapacitors,
        TechUpgradeId::NovaMissileForge,
        TechUpgradeId::SpriteTurretLattice,
        TechUpgradeId::ArmorPlating,
        TechUpgradeId::RejuvenationMatrix,
        TechUpgradeId::MechCommandLink,
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
}

impl UpgradeLedger {
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
}
