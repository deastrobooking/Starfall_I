#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::chapters::MapSettlementKind;
use crate::world::robot_pets::{PartCost, RobotPartKind, RobotPetCollection};

pub const MAX_SETTLEMENT_BUILD_TIER: u32 = 3;
pub const SETTLEMENT_ECONOMY_TICK_SECONDS: f32 = 8.0;
const RESOURCE_CAP: u32 = 99_999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SettlementBuildKind {
    Farm,
    Factory,
    Spaceport,
    PowerPlant,
    ResearchLab,
    DefenseOutpost,
    BridgeHub,
}

impl SettlementBuildKind {
    pub const ALL: [SettlementBuildKind; 7] = [
        SettlementBuildKind::Farm,
        SettlementBuildKind::Factory,
        SettlementBuildKind::Spaceport,
        SettlementBuildKind::PowerPlant,
        SettlementBuildKind::ResearchLab,
        SettlementBuildKind::DefenseOutpost,
        SettlementBuildKind::BridgeHub,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettlementBuildKind::Farm => "Farm",
            SettlementBuildKind::Factory => "Factory",
            SettlementBuildKind::Spaceport => "Spaceport",
            SettlementBuildKind::PowerPlant => "Power Plant",
            SettlementBuildKind::ResearchLab => "Research Lab",
            SettlementBuildKind::DefenseOutpost => "Defense Outpost",
            SettlementBuildKind::BridgeHub => "Bridge Hub",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettlementResourceKind {
    Credits,
    Food,
    Power,
    Alloy,
    Circuits,
    Research,
    FuelCells,
    Workers,
    HangarCapacity,
}

impl SettlementResourceKind {
    pub fn label(self) -> &'static str {
        match self {
            SettlementResourceKind::Credits => "credits",
            SettlementResourceKind::Food => "food",
            SettlementResourceKind::Power => "power",
            SettlementResourceKind::Alloy => "alloy",
            SettlementResourceKind::Circuits => "circuits",
            SettlementResourceKind::Research => "research",
            SettlementResourceKind::FuelCells => "fuel cells",
            SettlementResourceKind::Workers => "workers",
            SettlementResourceKind::HangarCapacity => "hangar capacity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementResources {
    pub credits: u32,
    pub food: u32,
    pub power: u32,
    pub alloy: u32,
    pub circuits: u32,
    pub research: u32,
    pub fuel_cells: u32,
    pub workers: u32,
    pub hangar_capacity: u32,
}

impl SettlementResources {
    pub const ZERO: Self = Self::new(0, 0, 0, 0, 0, 0, 0, 0, 0);

    pub const fn new(
        credits: u32,
        food: u32,
        power: u32,
        alloy: u32,
        circuits: u32,
        research: u32,
        fuel_cells: u32,
        workers: u32,
        hangar_capacity: u32,
    ) -> Self {
        Self {
            credits,
            food,
            power,
            alloy,
            circuits,
            research,
            fuel_cells,
            workers,
            hangar_capacity,
        }
    }

    pub fn starter_stockpile() -> Self {
        Self::new(420, 60, 45, 34, 20, 0, 8, 16, 2)
    }

    pub fn scaled(self, scale: u32) -> Self {
        Self::new(
            self.credits.saturating_mul(scale),
            self.food.saturating_mul(scale),
            self.power.saturating_mul(scale),
            self.alloy.saturating_mul(scale),
            self.circuits.saturating_mul(scale),
            self.research.saturating_mul(scale),
            self.fuel_cells.saturating_mul(scale),
            self.workers.saturating_mul(scale),
            self.hangar_capacity.saturating_mul(scale),
        )
    }

    pub fn add_capped(&mut self, output: Self) {
        self.credits = (self.credits + output.credits).min(RESOURCE_CAP);
        self.food = (self.food + output.food).min(RESOURCE_CAP);
        self.power = (self.power + output.power).min(RESOURCE_CAP);
        self.alloy = (self.alloy + output.alloy).min(RESOURCE_CAP);
        self.circuits = (self.circuits + output.circuits).min(RESOURCE_CAP);
        self.research = (self.research + output.research).min(RESOURCE_CAP);
        self.fuel_cells = (self.fuel_cells + output.fuel_cells).min(RESOURCE_CAP);
        self.workers = (self.workers + output.workers).min(RESOURCE_CAP);
        self.hangar_capacity = (self.hangar_capacity + output.hangar_capacity).min(RESOURCE_CAP);
    }

    fn amount(self, kind: SettlementResourceKind) -> u32 {
        match kind {
            SettlementResourceKind::Credits => self.credits,
            SettlementResourceKind::Food => self.food,
            SettlementResourceKind::Power => self.power,
            SettlementResourceKind::Alloy => self.alloy,
            SettlementResourceKind::Circuits => self.circuits,
            SettlementResourceKind::Research => self.research,
            SettlementResourceKind::FuelCells => self.fuel_cells,
            SettlementResourceKind::Workers => self.workers,
            SettlementResourceKind::HangarCapacity => self.hangar_capacity,
        }
    }

    fn spend(&mut self, cost: Self) {
        self.credits = self.credits.saturating_sub(cost.credits);
        self.food = self.food.saturating_sub(cost.food);
        self.power = self.power.saturating_sub(cost.power);
        self.alloy = self.alloy.saturating_sub(cost.alloy);
        self.circuits = self.circuits.saturating_sub(cost.circuits);
        self.research = self.research.saturating_sub(cost.research);
        self.fuel_cells = self.fuel_cells.saturating_sub(cost.fuel_cells);
        self.workers = self.workers.saturating_sub(cost.workers);
        self.hangar_capacity = self.hangar_capacity.saturating_sub(cost.hangar_capacity);
    }

    fn missing_for(self, cost: Self) -> Option<(SettlementResourceKind, u32, u32)> {
        for kind in [
            SettlementResourceKind::Credits,
            SettlementResourceKind::Food,
            SettlementResourceKind::Power,
            SettlementResourceKind::Alloy,
            SettlementResourceKind::Circuits,
            SettlementResourceKind::Research,
            SettlementResourceKind::FuelCells,
            SettlementResourceKind::Workers,
            SettlementResourceKind::HangarCapacity,
        ] {
            let have = self.amount(kind);
            let need = cost.amount(kind);
            if have < need {
                return Some((kind, need, have));
            }
        }
        None
    }

    pub fn summary(self) -> String {
        format!(
            "{}cr {}food {}pow {}alloy {}cir {}res {}fuel {}wrk {}hangar",
            self.credits,
            self.food,
            self.power,
            self.alloy,
            self.circuits,
            self.research,
            self.fuel_cells,
            self.workers,
            self.hangar_capacity
        )
    }
}

impl Default for SettlementResources {
    fn default() -> Self {
        Self::starter_stockpile()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SettlementBuildDef {
    pub kind: SettlementBuildKind,
    pub label: &'static str,
    pub description: &'static str,
    pub base_cost: SettlementResources,
    pub base_output: SettlementResources,
    pub part_costs: &'static [PartCost],
}

const FARM_PARTS: [PartCost; 0] = [];
const FACTORY_PARTS: [PartCost; 2] = [
    PartCost::new(RobotPartKind::ScrapFrame, 3),
    PartCost::new(RobotPartKind::ServoMotor, 1),
];
const SPACEPORT_PARTS: [PartCost; 2] = [
    PartCost::new(RobotPartKind::HoverJet, 2),
    PartCost::new(RobotPartKind::HullPlate, 2),
];
const POWER_PARTS: [PartCost; 1] = [PartCost::new(RobotPartKind::EnergyCore, 1)];
const RESEARCH_PARTS: [PartCost; 2] = [
    PartCost::new(RobotPartKind::CircuitBoard, 2),
    PartCost::new(RobotPartKind::EnergyCore, 1),
];
const DEFENSE_PARTS: [PartCost; 2] = [
    PartCost::new(RobotPartKind::ScrapFrame, 3),
    PartCost::new(RobotPartKind::CircuitBoard, 1),
];
const BRIDGE_PARTS: [PartCost; 2] = [
    PartCost::new(RobotPartKind::ScrapFrame, 4),
    PartCost::new(RobotPartKind::HullPlate, 1),
];

pub fn settlement_build_def(kind: SettlementBuildKind) -> SettlementBuildDef {
    match kind {
        SettlementBuildKind::Farm => SettlementBuildDef {
            kind,
            label: "Farm",
            description: "Food, herbs, and safe field routes for rescued people.",
            base_cost: SettlementResources::new(80, 0, 0, 4, 0, 0, 0, 2, 0),
            base_output: SettlementResources::new(6, 14, 0, 0, 0, 0, 0, 1, 0),
            part_costs: &FARM_PARTS,
        },
        SettlementBuildKind::Factory => SettlementBuildDef {
            kind,
            label: "Factory",
            description: "Alloy, circuits, and robot-part assembly lanes.",
            base_cost: SettlementResources::new(130, 8, 8, 8, 4, 0, 0, 3, 0),
            base_output: SettlementResources::new(4, 0, 0, 12, 5, 0, 0, 0, 0),
            part_costs: &FACTORY_PARTS,
        },
        SettlementBuildKind::Spaceport => SettlementBuildDef {
            kind,
            label: "Spaceport",
            description: "Hangars, launch ramps, fuel, and ship response capacity.",
            base_cost: SettlementResources::new(220, 0, 14, 14, 8, 0, 4, 4, 0),
            base_output: SettlementResources::new(5, 0, 0, 0, 0, 0, 8, 0, 3),
            part_costs: &SPACEPORT_PARTS,
        },
        SettlementBuildKind::PowerPlant => SettlementBuildDef {
            kind,
            label: "Power Plant",
            description: "Power cores for shields, rails, bridges, and city lights.",
            base_cost: SettlementResources::new(120, 0, 0, 7, 4, 0, 0, 3, 0),
            base_output: SettlementResources::new(4, 0, 20, 0, 0, 0, 2, 0, 0),
            part_costs: &POWER_PARTS,
        },
        SettlementBuildKind::ResearchLab => SettlementBuildDef {
            kind,
            label: "Research Lab",
            description: "Blueprints, enemy secrets, hacking tech, and prototype rooms.",
            base_cost: SettlementResources::new(160, 0, 10, 5, 8, 0, 0, 3, 0),
            base_output: SettlementResources::new(3, 0, 0, 0, 2, 13, 0, 0, 0),
            part_costs: &RESEARCH_PARTS,
        },
        SettlementBuildKind::DefenseOutpost => SettlementBuildDef {
            kind,
            label: "Defense Outpost",
            description: "Shield gates, turret decks, and drone patrol pads.",
            base_cost: SettlementResources::new(140, 0, 8, 10, 5, 0, 0, 2, 0),
            base_output: SettlementResources::new(2, 0, 0, 0, 0, 0, 0, 0, 2),
            part_costs: &DEFENSE_PARTS,
        },
        SettlementBuildKind::BridgeHub => SettlementBuildDef {
            kind,
            label: "Bridge Hub",
            description: "Repairable bridges, sky ramps, rails, and mountain-pass staging.",
            base_cost: SettlementResources::new(110, 0, 4, 10, 2, 0, 0, 2, 0),
            base_output: SettlementResources::new(3, 0, 0, 2, 0, 0, 1, 0, 1),
            part_costs: &BRIDGE_PARTS,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementBuildRecord {
    pub settlement_id: String,
    pub kind: SettlementBuildKind,
    pub tier: u32,
}

#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct SettlementEconomy {
    #[serde(default)]
    pub stockpile: SettlementResources,
    #[serde(default)]
    pub builds: Vec<SettlementBuildRecord>,
    #[serde(default)]
    pub tick_timer: f32,
}

impl Default for SettlementEconomy {
    fn default() -> Self {
        Self {
            stockpile: SettlementResources::starter_stockpile(),
            builds: Vec::new(),
            tick_timer: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettlementBuildError {
    MaxTier {
        kind: SettlementBuildKind,
        tier: u32,
    },
    MissingResource {
        resource: SettlementResourceKind,
        need: u32,
        have: u32,
    },
    MissingPart {
        part: RobotPartKind,
        need: u32,
        have: u32,
    },
}

impl SettlementBuildError {
    pub fn message(&self) -> String {
        match self {
            SettlementBuildError::MaxTier { kind, tier } => {
                format!("{} is already tier {}.", kind.label(), tier)
            }
            SettlementBuildError::MissingResource {
                resource,
                need,
                have,
            } => format!("Need {} {} (have {}).", need, resource.label(), have),
            SettlementBuildError::MissingPart { part, need, have } => {
                format!("Need {} {} (have {}).", need, part.label(), have)
            }
        }
    }
}

impl SettlementEconomy {
    pub fn build_tier(&self, settlement_id: &str, kind: SettlementBuildKind) -> u32 {
        self.builds
            .iter()
            .find(|record| record.settlement_id == settlement_id && record.kind == kind)
            .map(|record| record.tier)
            .unwrap_or(0)
    }

    pub fn builds_for<'a>(
        &'a self,
        settlement_id: &'a str,
    ) -> impl Iterator<Item = &'a SettlementBuildRecord> + 'a {
        self.builds
            .iter()
            .filter(move |record| record.settlement_id == settlement_id)
    }

    pub fn total_builds_for(&self, settlement_id: &str) -> usize {
        self.builds_for(settlement_id).count()
    }

    pub fn next_recommended_build(
        &self,
        settlement_id: &str,
        settlement_kind: MapSettlementKind,
    ) -> SettlementBuildKind {
        let order = settlement_build_sequence(settlement_kind);
        for kind in order {
            if self.build_tier(settlement_id, *kind) == 0 {
                return *kind;
            }
        }
        order
            .iter()
            .copied()
            .find(|kind| self.build_tier(settlement_id, *kind) < MAX_SETTLEMENT_BUILD_TIER)
            .unwrap_or(order[0])
    }

    pub fn preview_error(
        &self,
        kind: SettlementBuildKind,
        robot_pets: &RobotPetCollection,
        settlement_id: &str,
    ) -> Option<SettlementBuildError> {
        let current_tier = self.build_tier(settlement_id, kind);
        let next_tier = current_tier + 1;
        if current_tier >= MAX_SETTLEMENT_BUILD_TIER {
            return Some(SettlementBuildError::MaxTier {
                kind,
                tier: current_tier,
            });
        }

        let def = settlement_build_def(kind);
        let cost = def.base_cost.scaled(next_tier);
        if let Some((resource, need, have)) = self.stockpile.missing_for(cost) {
            return Some(SettlementBuildError::MissingResource {
                resource,
                need,
                have,
            });
        }

        for part_cost in scaled_part_costs(def.part_costs, next_tier) {
            let have = robot_pets.part_count(part_cost.kind);
            if have < part_cost.quantity {
                return Some(SettlementBuildError::MissingPart {
                    part: part_cost.kind,
                    need: part_cost.quantity,
                    have,
                });
            }
        }

        None
    }

    pub fn try_build(
        &mut self,
        settlement_id: &str,
        kind: SettlementBuildKind,
        robot_pets: &mut RobotPetCollection,
    ) -> Result<u32, SettlementBuildError> {
        if let Some(error) = self.preview_error(kind, robot_pets, settlement_id) {
            return Err(error);
        }

        let current_tier = self.build_tier(settlement_id, kind);
        let next_tier = current_tier + 1;
        let def = settlement_build_def(kind);
        self.stockpile.spend(def.base_cost.scaled(next_tier));
        let part_costs = scaled_part_costs(def.part_costs, next_tier);
        robot_pets
            .spend_parts(&part_costs)
            .expect("preview_error checked robot part affordability");

        if let Some(record) = self
            .builds
            .iter_mut()
            .find(|record| record.settlement_id == settlement_id && record.kind == kind)
        {
            record.tier = next_tier;
        } else {
            self.builds.push(SettlementBuildRecord {
                settlement_id: settlement_id.to_string(),
                kind,
                tier: next_tier,
            });
            self.builds.sort_by(|a, b| {
                (&a.settlement_id, a.kind as u8).cmp(&(&b.settlement_id, b.kind as u8))
            });
        }

        Ok(next_tier)
    }

    pub fn output_per_tick(&self) -> SettlementResources {
        let mut total = SettlementResources::ZERO;
        for record in &self.builds {
            let output = settlement_build_def(record.kind)
                .base_output
                .scaled(record.tier.max(1));
            total.add_capped(output);
        }
        total
    }

    pub fn tick(&mut self, dt: f32) -> u32 {
        if self.builds.is_empty() {
            return 0;
        }

        self.tick_timer += dt.max(0.0);
        let mut ticks = 0;
        while self.tick_timer >= SETTLEMENT_ECONOMY_TICK_SECONDS {
            self.tick_timer -= SETTLEMENT_ECONOMY_TICK_SECONDS;
            let output = self.output_per_tick();
            self.stockpile.add_capped(output);
            ticks += 1;
        }
        ticks
    }

    pub fn status_line(&self, settlement_id: &str) -> String {
        let count = self.total_builds_for(settlement_id);
        let output = self.output_per_tick();
        format!(
            "{} build{} | stock {} | tick +{}cr +{}food +{}pow +{}alloy +{}cir +{}res +{}fuel +{}hangar",
            count,
            if count == 1 { "" } else { "s" },
            self.stockpile.summary(),
            output.credits,
            output.food,
            output.power,
            output.alloy,
            output.circuits,
            output.research,
            output.fuel_cells,
            output.hangar_capacity
        )
    }
}

pub fn settlement_build_sequence(kind: MapSettlementKind) -> &'static [SettlementBuildKind] {
    match kind {
        MapSettlementKind::City => &[
            SettlementBuildKind::PowerPlant,
            SettlementBuildKind::DefenseOutpost,
            SettlementBuildKind::ResearchLab,
            SettlementBuildKind::Spaceport,
            SettlementBuildKind::Factory,
            SettlementBuildKind::Farm,
            SettlementBuildKind::BridgeHub,
        ],
        MapSettlementKind::Village => &[
            SettlementBuildKind::Farm,
            SettlementBuildKind::PowerPlant,
            SettlementBuildKind::DefenseOutpost,
            SettlementBuildKind::ResearchLab,
            SettlementBuildKind::BridgeHub,
            SettlementBuildKind::Factory,
            SettlementBuildKind::Spaceport,
        ],
        MapSettlementKind::Harbor => &[
            SettlementBuildKind::Spaceport,
            SettlementBuildKind::Factory,
            SettlementBuildKind::DefenseOutpost,
            SettlementBuildKind::PowerPlant,
            SettlementBuildKind::BridgeHub,
            SettlementBuildKind::Farm,
            SettlementBuildKind::ResearchLab,
        ],
        MapSettlementKind::Outpost => &[
            SettlementBuildKind::DefenseOutpost,
            SettlementBuildKind::PowerPlant,
            SettlementBuildKind::Factory,
            SettlementBuildKind::BridgeHub,
            SettlementBuildKind::ResearchLab,
            SettlementBuildKind::Farm,
            SettlementBuildKind::Spaceport,
        ],
    }
}

pub fn scaled_part_costs(costs: &[PartCost], scale: u32) -> Vec<PartCost> {
    costs
        .iter()
        .map(|cost| PartCost::new(cost.kind, cost.quantity.saturating_mul(scale)))
        .filter(|cost| cost.quantity > 0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rich_parts() -> RobotPetCollection {
        let mut pets = RobotPetCollection::default();
        for kind in RobotPartKind::ALL {
            pets.add_part(kind, 30);
        }
        pets
    }

    #[test]
    fn liberated_settlement_can_build_three_core_structures() {
        let mut economy = SettlementEconomy {
            stockpile: SettlementResources::new(2_000, 500, 500, 500, 500, 0, 100, 100, 10),
            ..SettlementEconomy::default()
        };
        let mut parts = rich_parts();

        assert_eq!(
            economy.try_build(
                "settlement_riftglass_village",
                SettlementBuildKind::Farm,
                &mut parts
            ),
            Ok(1)
        );
        assert_eq!(
            economy.try_build(
                "settlement_riftglass_village",
                SettlementBuildKind::PowerPlant,
                &mut parts
            ),
            Ok(1)
        );
        assert_eq!(
            economy.try_build(
                "settlement_riftglass_village",
                SettlementBuildKind::DefenseOutpost,
                &mut parts
            ),
            Ok(1)
        );

        assert_eq!(economy.total_builds_for("settlement_riftglass_village"), 3);
        assert!(economy.stockpile.credits < 2_000);
    }

    #[test]
    fn economy_tick_adds_deterministic_outputs() {
        let mut economy = SettlementEconomy {
            stockpile: SettlementResources::new(2_000, 500, 500, 500, 500, 0, 100, 100, 10),
            ..SettlementEconomy::default()
        };
        let mut parts = rich_parts();
        economy
            .try_build(
                "settlement_star_orchard",
                SettlementBuildKind::Farm,
                &mut parts,
            )
            .unwrap();
        let before = economy.stockpile.food;

        assert_eq!(economy.tick(SETTLEMENT_ECONOMY_TICK_SECONDS), 1);
        assert_eq!(
            economy.stockpile.food,
            before
                + settlement_build_def(SettlementBuildKind::Farm)
                    .base_output
                    .food
        );
    }

    #[test]
    fn missing_robot_parts_block_building_with_specific_reason() {
        let economy = SettlementEconomy::default();
        let parts = RobotPetCollection::default();

        let error = economy
            .preview_error(
                SettlementBuildKind::PowerPlant,
                &parts,
                "settlement_lantern_hamlet",
            )
            .expect("power plant should require an energy core");

        assert_eq!(
            error,
            SettlementBuildError::MissingPart {
                part: RobotPartKind::EnergyCore,
                need: 1,
                have: 0
            }
        );
        assert!(error.message().contains("Energy Core"));
    }
}
