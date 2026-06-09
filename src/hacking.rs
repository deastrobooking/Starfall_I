use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::commands::{CommandAssetId, CommandAssetKind, CommandRegistry};
use crate::components::faction::Faction;
use crate::components::player::PlayerIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HackTargetClass {
    SmallDrone,
    WorkerRobot,
    CombatRobot,
    Mech,
    FighterShip,
    UfoSystem,
}

impl HackTargetClass {
    pub fn label(self) -> &'static str {
        match self {
            HackTargetClass::SmallDrone => "small drone",
            HackTargetClass::WorkerRobot => "worker robot",
            HackTargetClass::CombatRobot => "combat robot",
            HackTargetClass::Mech => "mech",
            HackTargetClass::FighterShip => "fighter ship",
            HackTargetClass::UfoSystem => "UFO system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TakeoverProfile {
    DisableOnly,
    RemoteDrone,
    TemporaryAlly,
    BoardingBridge,
    BossPhaseSystem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HackBlueprintRecord {
    pub id: String,
    pub source: HackTargetClass,
    pub discovered_by: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HackCaptureRecord {
    pub player_index: u8,
    pub source: HackTargetClass,
    pub command_asset: Option<CommandAssetKind>,
    pub blueprint_id: String,
}

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct HackingRegistry {
    #[serde(default)]
    pub blueprints: Vec<HackBlueprintRecord>,
    #[serde(default)]
    pub captures: Vec<HackCaptureRecord>,
}

impl HackingRegistry {
    pub fn has_blueprint(&self, id: &str) -> bool {
        self.blueprints.iter().any(|record| record.id == id)
    }

    pub fn learn_blueprint(
        &mut self,
        id: impl Into<String>,
        source: HackTargetClass,
        player_index: PlayerIndex,
    ) -> bool {
        let id = id.into();
        if self.has_blueprint(&id) {
            return false;
        }
        self.blueprints.push(HackBlueprintRecord {
            id,
            source,
            discovered_by: player_index.0,
        });
        true
    }

    pub fn record_capture(
        &mut self,
        player_index: PlayerIndex,
        source: HackTargetClass,
        command_asset: Option<CommandAssetKind>,
        blueprint_id: impl Into<String>,
    ) {
        self.captures.push(HackCaptureRecord {
            player_index: player_index.0,
            source,
            command_asset,
            blueprint_id: blueprint_id.into(),
        });
    }
}

#[derive(Component, Debug, Clone)]
pub struct Hackable {
    pub difficulty: u8,
    pub faction: Faction,
    pub target_class: HackTargetClass,
    pub required_range: f32,
    pub line_of_sight_required: bool,
    pub blueprint_reward: &'static str,
    pub takeover_profile: TakeoverProfile,
    pub takeover_seconds: f32,
    pub command_asset_reward: Option<CommandAssetKind>,
    pub progress: Option<HackProgress>,
}

impl Hackable {
    pub fn scallarian_drone() -> Self {
        Self {
            difficulty: 1,
            faction: Faction::DimensionalAlien,
            target_class: HackTargetClass::SmallDrone,
            required_range: 14.0,
            line_of_sight_required: false,
            blueprint_reward: "blueprint_scallarian_drone_core",
            takeover_profile: TakeoverProfile::RemoteDrone,
            takeover_seconds: 18.0,
            command_asset_reward: Some(CommandAssetKind::ScoutDrone),
            progress: None,
        }
    }

    pub fn hack_seconds(&self) -> f32 {
        0.85 + f32::from(self.difficulty) * 0.45
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HackProgress {
    pub player_index: PlayerIndex,
    pub remaining: f32,
}

#[derive(Component, Debug, Clone)]
pub struct HackedUnit {
    pub owner: PlayerIndex,
    pub timer: f32,
    pub max_timer: f32,
    pub ability_cooldown: f32,
    pub source: HackTargetClass,
    pub blueprint_id: String,
    pub command_asset_id: Option<CommandAssetId>,
}

pub fn complete_small_drone_hack(
    registry: &mut HackingRegistry,
    command_registry: &mut CommandRegistry,
    player_index: PlayerIndex,
    hackable: &Hackable,
) -> HackedUnit {
    registry.learn_blueprint(
        hackable.blueprint_reward,
        hackable.target_class,
        player_index,
    );
    let command_asset_id = hackable
        .command_asset_reward
        .map(|kind| command_registry.add_asset(kind));
    registry.record_capture(
        player_index,
        hackable.target_class,
        hackable.command_asset_reward,
        hackable.blueprint_reward,
    );
    HackedUnit {
        owner: player_index,
        timer: hackable.takeover_seconds,
        max_timer: hackable.takeover_seconds,
        ability_cooldown: 0.0,
        source: hackable.target_class,
        blueprint_id: hackable.blueprint_reward.to_string(),
        command_asset_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blueprint_rewards_are_unique_by_id() {
        let mut registry = HackingRegistry::default();

        assert!(registry.learn_blueprint(
            "blueprint_scallarian_drone_core",
            HackTargetClass::SmallDrone,
            PlayerIndex(0),
        ));
        assert!(!registry.learn_blueprint(
            "blueprint_scallarian_drone_core",
            HackTargetClass::SmallDrone,
            PlayerIndex(1),
        ));
        assert_eq!(registry.blueprints.len(), 1);
        assert_eq!(registry.blueprints[0].discovered_by, 0);
    }

    #[test]
    fn completing_small_drone_hack_adds_capture_and_command_asset() {
        let mut registry = HackingRegistry::default();
        let mut commands = CommandRegistry::default();
        let hacked = complete_small_drone_hack(
            &mut registry,
            &mut commands,
            PlayerIndex(2),
            &Hackable::scallarian_drone(),
        );

        assert_eq!(hacked.owner, PlayerIndex(2));
        assert!(hacked.command_asset_id.is_some());
        assert_eq!(registry.blueprints.len(), 1);
        assert_eq!(registry.captures.len(), 1);
        assert_eq!(commands.assets.len(), 1);
    }
}
