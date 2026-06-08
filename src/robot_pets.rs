use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::enemy::EnemyType;
use crate::components::faction::Faction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RobotPartKind {
    ScrapFrame,
    ServoMotor,
    CircuitBoard,
    EnergyCore,
    HoverJet,
    TreadUnit,
    HullPlate,
    PressureSeal,
    StarDrive,
    CommandDeck,
}

impl RobotPartKind {
    pub const ALL: [RobotPartKind; 10] = [
        RobotPartKind::ScrapFrame,
        RobotPartKind::ServoMotor,
        RobotPartKind::CircuitBoard,
        RobotPartKind::EnergyCore,
        RobotPartKind::HoverJet,
        RobotPartKind::TreadUnit,
        RobotPartKind::HullPlate,
        RobotPartKind::PressureSeal,
        RobotPartKind::StarDrive,
        RobotPartKind::CommandDeck,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RobotPartKind::ScrapFrame => "Scrap Frame",
            RobotPartKind::ServoMotor => "Servo Motor",
            RobotPartKind::CircuitBoard => "Circuit Board",
            RobotPartKind::EnergyCore => "Energy Core",
            RobotPartKind::HoverJet => "Hover Jet",
            RobotPartKind::TreadUnit => "Tread Unit",
            RobotPartKind::HullPlate => "Hull Plate",
            RobotPartKind::PressureSeal => "Pressure Seal",
            RobotPartKind::StarDrive => "Star Drive",
            RobotPartKind::CommandDeck => "Command Deck",
        }
    }

    pub fn item_id(self) -> &'static str {
        match self {
            RobotPartKind::ScrapFrame => "robot_scrap_frame",
            RobotPartKind::ServoMotor => "robot_servo_motor",
            RobotPartKind::CircuitBoard => "robot_circuit_board",
            RobotPartKind::EnergyCore => "robot_energy_core",
            RobotPartKind::HoverJet => "robot_hover_jet",
            RobotPartKind::TreadUnit => "robot_tread_unit",
            RobotPartKind::HullPlate => "robot_hull_plate",
            RobotPartKind::PressureSeal => "robot_pressure_seal",
            RobotPartKind::StarDrive => "robot_star_drive",
            RobotPartKind::CommandDeck => "robot_command_deck",
        }
    }

    pub fn from_item_id(item_id: &str) -> Option<Self> {
        Some(match item_id {
            "scrap_metal" | "robot_scrap_frame" => Self::ScrapFrame,
            "servo_motor" | "robot_servo_motor" => Self::ServoMotor,
            "circuit_board" | "robot_circuit_board" => Self::CircuitBoard,
            "energy_core" | "robot_energy_core" => Self::EnergyCore,
            "hover_jet" | "robot_hover_jet" => Self::HoverJet,
            "tread_unit" | "robot_tread_unit" => Self::TreadUnit,
            "hull_plate" | "robot_hull_plate" => Self::HullPlate,
            "pressure_seal" | "robot_pressure_seal" => Self::PressureSeal,
            "star_drive" | "robot_star_drive" => Self::StarDrive,
            "command_deck" | "robot_command_deck" => Self::CommandDeck,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RobotPetRole {
    Scout,
    Healer,
    Defender,
    Builder,
    Pilot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RobotPetSource {
    Rescued,
    StoreBuilt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RobotAssemblyForm {
    Car,
    Motorcycle,
    Tank,
    Boat,
    Submarine,
    SpaceJet,
    GiantMech,
    SpaceShip,
    MegaShip,
}

impl RobotAssemblyForm {
    pub const ALL: [RobotAssemblyForm; 9] = [
        RobotAssemblyForm::Car,
        RobotAssemblyForm::Motorcycle,
        RobotAssemblyForm::Tank,
        RobotAssemblyForm::Boat,
        RobotAssemblyForm::Submarine,
        RobotAssemblyForm::SpaceJet,
        RobotAssemblyForm::GiantMech,
        RobotAssemblyForm::SpaceShip,
        RobotAssemblyForm::MegaShip,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RobotAssemblyForm::Car => "Car",
            RobotAssemblyForm::Motorcycle => "Motorcycle",
            RobotAssemblyForm::Tank => "Tank",
            RobotAssemblyForm::Boat => "Boat",
            RobotAssemblyForm::Submarine => "Submarine",
            RobotAssemblyForm::SpaceJet => "Space Jet",
            RobotAssemblyForm::GiantMech => "Giant Mech",
            RobotAssemblyForm::SpaceShip => "Space Ship",
            RobotAssemblyForm::MegaShip => "Megaship",
        }
    }

    pub fn required_pets(self) -> usize {
        match self {
            RobotAssemblyForm::Car | RobotAssemblyForm::Motorcycle | RobotAssemblyForm::Boat => 1,
            RobotAssemblyForm::Tank
            | RobotAssemblyForm::Submarine
            | RobotAssemblyForm::SpaceJet => 2,
            RobotAssemblyForm::GiantMech => 4,
            RobotAssemblyForm::SpaceShip => 6,
            RobotAssemblyForm::MegaShip => 10,
        }
    }

    pub fn recipe(self) -> &'static [PartCost] {
        match self {
            RobotAssemblyForm::Car => &CAR_RECIPE,
            RobotAssemblyForm::Motorcycle => &MOTORCYCLE_RECIPE,
            RobotAssemblyForm::Tank => &TANK_RECIPE,
            RobotAssemblyForm::Boat => &BOAT_RECIPE,
            RobotAssemblyForm::Submarine => &SUBMARINE_RECIPE,
            RobotAssemblyForm::SpaceJet => &SPACE_JET_RECIPE,
            RobotAssemblyForm::GiantMech => &GIANT_MECH_RECIPE,
            RobotAssemblyForm::SpaceShip => &SPACE_SHIP_RECIPE,
            RobotAssemblyForm::MegaShip => &MEGA_SHIP_RECIPE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartCost {
    pub kind: RobotPartKind,
    pub quantity: u32,
}

impl PartCost {
    pub const fn new(kind: RobotPartKind, quantity: u32) -> Self {
        Self { kind, quantity }
    }
}

const CAR_RECIPE: [PartCost; 3] = [
    PartCost::new(RobotPartKind::ScrapFrame, 4),
    PartCost::new(RobotPartKind::ServoMotor, 2),
    PartCost::new(RobotPartKind::EnergyCore, 1),
];
const MOTORCYCLE_RECIPE: [PartCost; 3] = [
    PartCost::new(RobotPartKind::ScrapFrame, 3),
    PartCost::new(RobotPartKind::ServoMotor, 3),
    PartCost::new(RobotPartKind::CircuitBoard, 1),
];
const TANK_RECIPE: [PartCost; 3] = [
    PartCost::new(RobotPartKind::ScrapFrame, 8),
    PartCost::new(RobotPartKind::TreadUnit, 4),
    PartCost::new(RobotPartKind::EnergyCore, 2),
];
const BOAT_RECIPE: [PartCost; 3] = [
    PartCost::new(RobotPartKind::ScrapFrame, 5),
    PartCost::new(RobotPartKind::HullPlate, 3),
    PartCost::new(RobotPartKind::EnergyCore, 1),
];
const SUBMARINE_RECIPE: [PartCost; 3] = [
    PartCost::new(RobotPartKind::HullPlate, 6),
    PartCost::new(RobotPartKind::PressureSeal, 4),
    PartCost::new(RobotPartKind::EnergyCore, 2),
];
const SPACE_JET_RECIPE: [PartCost; 3] = [
    PartCost::new(RobotPartKind::HoverJet, 4),
    PartCost::new(RobotPartKind::CircuitBoard, 3),
    PartCost::new(RobotPartKind::EnergyCore, 3),
];
const GIANT_MECH_RECIPE: [PartCost; 4] = [
    PartCost::new(RobotPartKind::ScrapFrame, 16),
    PartCost::new(RobotPartKind::ServoMotor, 10),
    PartCost::new(RobotPartKind::CircuitBoard, 6),
    PartCost::new(RobotPartKind::EnergyCore, 5),
];
const SPACE_SHIP_RECIPE: [PartCost; 4] = [
    PartCost::new(RobotPartKind::HullPlate, 12),
    PartCost::new(RobotPartKind::HoverJet, 8),
    PartCost::new(RobotPartKind::StarDrive, 2),
    PartCost::new(RobotPartKind::CommandDeck, 1),
];
const MEGA_SHIP_RECIPE: [PartCost; 5] = [
    PartCost::new(RobotPartKind::ScrapFrame, 40),
    PartCost::new(RobotPartKind::HullPlate, 30),
    PartCost::new(RobotPartKind::HoverJet, 16),
    PartCost::new(RobotPartKind::StarDrive, 6),
    PartCost::new(RobotPartKind::CommandDeck, 3),
];
const STORE_PET_RECIPE: [PartCost; 4] = [
    PartCost::new(RobotPartKind::ScrapFrame, 5),
    PartCost::new(RobotPartKind::ServoMotor, 2),
    PartCost::new(RobotPartKind::CircuitBoard, 1),
    PartCost::new(RobotPartKind::EnergyCore, 1),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotPetBlueprint {
    pub id: String,
    pub name: String,
    pub role: RobotPetRole,
    pub source: RobotPetSource,
    pub level: u32,
    pub affection: u32,
    pub power: u32,
    pub armor: u32,
    pub mobility: u32,
    pub unlocked_forms: Vec<RobotAssemblyForm>,
}

impl RobotPetBlueprint {
    pub fn rescued(id: impl Into<String>, name: impl Into<String>, role: RobotPetRole) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            role,
            source: RobotPetSource::Rescued,
            level: 1,
            affection: 20,
            power: 8,
            armor: 8,
            mobility: 8,
            unlocked_forms: default_unlocked_forms(role, RobotPetSource::Rescued),
        }
    }

    pub fn store_built(id: impl Into<String>, name: impl Into<String>, role: RobotPetRole) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            role,
            source: RobotPetSource::StoreBuilt,
            level: 1,
            affection: 0,
            power: 6,
            armor: 6,
            mobility: 6,
            unlocked_forms: default_unlocked_forms(role, RobotPetSource::StoreBuilt),
        }
    }

    pub fn can_support(&self, form: RobotAssemblyForm) -> bool {
        self.unlocked_forms.contains(&form)
            || matches!(
                form,
                RobotAssemblyForm::GiantMech
                    | RobotAssemblyForm::SpaceShip
                    | RobotAssemblyForm::MegaShip
            )
    }
}

fn default_unlocked_forms(role: RobotPetRole, source: RobotPetSource) -> Vec<RobotAssemblyForm> {
    let mut forms = vec![RobotAssemblyForm::Car];
    if source == RobotPetSource::Rescued {
        forms.push(RobotAssemblyForm::Motorcycle);
    }
    match role {
        RobotPetRole::Scout => {
            forms.push(RobotAssemblyForm::SpaceJet);
        }
        RobotPetRole::Healer => {
            forms.push(RobotAssemblyForm::Boat);
            forms.push(RobotAssemblyForm::Submarine);
        }
        RobotPetRole::Defender => {
            forms.push(RobotAssemblyForm::Tank);
        }
        RobotPetRole::Builder => {
            forms.push(RobotAssemblyForm::Boat);
            forms.push(RobotAssemblyForm::Tank);
        }
        RobotPetRole::Pilot => {
            forms.push(RobotAssemblyForm::Boat);
            forms.push(RobotAssemblyForm::SpaceJet);
        }
    }
    forms.sort_by_key(|form| *form as u8);
    forms.dedup();
    forms
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobotAssembly {
    pub form: RobotAssemblyForm,
    pub pet_ids: Vec<String>,
}

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct RobotPetCollection {
    pub pets: Vec<RobotPetBlueprint>,
    pub parts: Vec<(RobotPartKind, u32)>,
    pub active_assembly: Option<RobotAssembly>,
}

impl RobotPetCollection {
    pub fn part_count(&self, kind: RobotPartKind) -> u32 {
        self.parts
            .iter()
            .find(|(part, _)| *part == kind)
            .map(|(_, quantity)| *quantity)
            .unwrap_or(0)
    }

    pub fn add_part(&mut self, kind: RobotPartKind, quantity: u32) {
        if quantity == 0 {
            return;
        }
        if let Some((_, existing)) = self.parts.iter_mut().find(|(part, _)| *part == kind) {
            *existing = existing.saturating_add(quantity);
        } else {
            self.parts.push((kind, quantity));
            self.parts.sort_by_key(|(part, _)| *part as u8);
        }
    }

    pub fn grant_salvage(&mut self, reward: RobotPartReward) {
        self.add_part(reward.kind, reward.quantity);
    }

    pub fn rescue_pet(&mut self, pet: RobotPetBlueprint) -> bool {
        if self.pets.iter().any(|existing| existing.id == pet.id) {
            return false;
        }
        self.pets.push(pet);
        self.pets.sort_by(|a, b| a.id.cmp(&b.id));
        true
    }

    pub fn build_store_pet(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        role: RobotPetRole,
    ) -> Result<(), RobotPetError> {
        let id = id.into();
        if self.pets.iter().any(|pet| pet.id == id) {
            return Err(RobotPetError::DuplicatePet);
        }
        self.spend_parts(store_pet_recipe())?;
        self.rescue_pet(RobotPetBlueprint::store_built(id, name, role));
        Ok(())
    }

    pub fn can_afford(&self, recipe: &[PartCost]) -> bool {
        recipe
            .iter()
            .all(|cost| self.part_count(cost.kind) >= cost.quantity)
    }

    pub fn spend_parts(&mut self, recipe: &[PartCost]) -> Result<(), RobotPetError> {
        if !self.can_afford(recipe) {
            return Err(RobotPetError::MissingParts);
        }
        for cost in recipe {
            if let Some((_, quantity)) = self.parts.iter_mut().find(|(part, _)| *part == cost.kind)
            {
                *quantity = quantity.saturating_sub(cost.quantity);
            }
        }
        self.parts.retain(|(_, quantity)| *quantity > 0);
        Ok(())
    }

    pub fn unlock_form_for_pet(
        &mut self,
        pet_id: &str,
        form: RobotAssemblyForm,
    ) -> Result<(), RobotPetError> {
        let Some(pet) = self.pets.iter_mut().find(|pet| pet.id == pet_id) else {
            return Err(RobotPetError::UnknownPet);
        };
        if !pet.unlocked_forms.contains(&form) {
            pet.unlocked_forms.push(form);
            pet.unlocked_forms.sort_by_key(|form| *form as u8);
        }
        Ok(())
    }

    pub fn assemble(
        &mut self,
        form: RobotAssemblyForm,
        pet_ids: &[String],
    ) -> Result<RobotAssembly, RobotPetError> {
        if pet_ids.len() < form.required_pets() {
            return Err(RobotPetError::NotEnoughPets);
        }
        for id in pet_ids {
            let Some(pet) = self.pets.iter().find(|pet| &pet.id == id) else {
                return Err(RobotPetError::UnknownPet);
            };
            if !pet.can_support(form) {
                return Err(RobotPetError::UnsupportedForm);
            }
        }
        self.spend_parts(form.recipe())?;
        let assembly = RobotAssembly {
            form,
            pet_ids: pet_ids.to_vec(),
        };
        self.active_assembly = Some(assembly.clone());
        Ok(assembly)
    }

    pub fn disassemble(&mut self) {
        self.active_assembly = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotPetError {
    DuplicatePet,
    MissingParts,
    NotEnoughPets,
    UnknownPet,
    UnsupportedForm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RobotPartReward {
    pub kind: RobotPartKind,
    pub quantity: u32,
}

pub fn store_pet_recipe() -> &'static [PartCost] {
    &STORE_PET_RECIPE
}

pub fn salvage_for_enemy(enemy_type: &str, credits: u32, experience: u32) -> RobotPartReward {
    let base_quantity = 1 + u32::from(credits >= 20) + u32::from(experience >= 50);
    let kind = match enemy_type {
        "Drone" => RobotPartKind::CircuitBoard,
        "Soldier" => RobotPartKind::ScrapFrame,
        "Heavy" => RobotPartKind::TreadUnit,
        "SpikeAlien" => RobotPartKind::ServoMotor,
        "Hybrid" => RobotPartKind::EnergyCore,
        _ => RobotPartKind::ScrapFrame,
    };
    RobotPartReward {
        kind,
        quantity: base_quantity.min(4),
    }
}

pub fn rare_salvage_for_context(
    faction: Faction,
    enemy_type: EnemyType,
) -> Option<RobotPartReward> {
    let kind = match (faction, enemy_type) {
        (Faction::DragonRoyalty | Faction::DragonExile, EnemyType::Heavy) => {
            RobotPartKind::HullPlate
        }
        (Faction::DimensionalAlien, EnemyType::Drone) => RobotPartKind::HoverJet,
        (Faction::DimensionalAlien, EnemyType::Hybrid) => RobotPartKind::StarDrive,
        (Faction::CorruptedHuman, EnemyType::Hybrid) => RobotPartKind::CommandDeck,
        _ => return None,
    };
    Some(RobotPartReward { kind, quantity: 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded_collection() -> RobotPetCollection {
        let mut collection = RobotPetCollection::default();
        for kind in RobotPartKind::ALL {
            collection.add_part(kind, 100);
        }
        collection
    }

    #[test]
    fn store_build_consumes_parts_and_adds_pet() {
        let mut collection = RobotPetCollection::default();
        collection.add_part(RobotPartKind::ScrapFrame, 5);
        collection.add_part(RobotPartKind::ServoMotor, 2);
        collection.add_part(RobotPartKind::CircuitBoard, 1);
        collection.add_part(RobotPartKind::EnergyCore, 1);

        collection
            .build_store_pet("bot-01", "Bolt Pup", RobotPetRole::Scout)
            .expect("recipe should build one pet");

        assert_eq!(collection.pets.len(), 1);
        assert_eq!(collection.pets[0].source, RobotPetSource::StoreBuilt);
        assert_eq!(collection.part_count(RobotPartKind::ScrapFrame), 0);
        assert_eq!(collection.part_count(RobotPartKind::EnergyCore), 0);
    }

    #[test]
    fn duplicate_rescues_are_ignored() {
        let mut collection = RobotPetCollection::default();
        let pet = RobotPetBlueprint::rescued("spark", "Spark Pup", RobotPetRole::Healer);

        assert!(collection.rescue_pet(pet.clone()));
        assert!(!collection.rescue_pet(pet));
        assert_eq!(collection.pets.len(), 1);
    }

    #[test]
    fn giant_mech_requires_four_pets_and_spends_recipe() {
        let mut collection = loaded_collection();
        for index in 0..4 {
            collection.rescue_pet(RobotPetBlueprint::rescued(
                format!("pet-{index}"),
                format!("Pet {index}"),
                RobotPetRole::Pilot,
            ));
        }
        let ids: Vec<_> = collection.pets.iter().map(|pet| pet.id.clone()).collect();

        let assembly = collection
            .assemble(RobotAssemblyForm::GiantMech, &ids)
            .expect("four pets and parts should form a mech");

        assert_eq!(assembly.form, RobotAssemblyForm::GiantMech);
        assert_eq!(
            collection
                .active_assembly
                .as_ref()
                .map(|assembly| assembly.form),
            Some(RobotAssemblyForm::GiantMech)
        );
        assert_eq!(collection.active_assembly.as_ref().unwrap().pet_ids, ids);
        assert_eq!(collection.part_count(RobotPartKind::ScrapFrame), 84);
        assert_eq!(collection.part_count(RobotPartKind::EnergyCore), 95);
    }

    #[test]
    fn role_defaults_make_special_vehicle_forms_reachable() {
        let mut collection = loaded_collection();
        collection.rescue_pet(RobotPetBlueprint::rescued(
            "defender",
            "Defender Pup",
            RobotPetRole::Defender,
        ));
        collection.rescue_pet(RobotPetBlueprint::rescued(
            "builder",
            "Builder Pup",
            RobotPetRole::Builder,
        ));
        let ids = vec!["defender".to_string(), "builder".to_string()];

        let assembly = collection
            .assemble(RobotAssemblyForm::Tank, &ids)
            .expect("defender and builder roles should support tank forms");

        assert_eq!(assembly.form, RobotAssemblyForm::Tank);
        assert_eq!(collection.active_assembly.as_ref().unwrap().pet_ids, ids);
    }

    #[test]
    fn megaship_is_late_game_gated_by_pet_count() {
        let mut collection = loaded_collection();
        for index in 0..9 {
            collection.rescue_pet(RobotPetBlueprint::rescued(
                format!("pet-{index}"),
                format!("Pet {index}"),
                RobotPetRole::Pilot,
            ));
        }
        let ids: Vec<_> = collection.pets.iter().map(|pet| pet.id.clone()).collect();

        let result = collection.assemble(RobotAssemblyForm::MegaShip, &ids);

        assert_eq!(result, Err(RobotPetError::NotEnoughPets));
    }

    #[test]
    fn enemy_salvage_maps_generic_bad_guys_to_robot_parts() {
        let drone = salvage_for_enemy("Drone", 5, 15);
        let heavy = salvage_for_enemy("Heavy", 50, 75);

        assert_eq!(drone.kind, RobotPartKind::CircuitBoard);
        assert_eq!(drone.quantity, 1);
        assert_eq!(heavy.kind, RobotPartKind::TreadUnit);
        assert_eq!(heavy.quantity, 3);
    }
}
