//! Public plugin composition boundaries for the current framework transition.
//!
//! These groups preserve the proven all-in-one registration order while giving
//! future games and feature gates named seams. They are intentionally broader
//! than the final capability bundles: dependency cleanup will split them only
//! after focused consumers prove each smaller contract.

use bevy::app::{PluginGroup, PluginGroupBuilder};

use crate::audio::{player::MusicPlayerPlugin, sfx::SfxPlugin};
use crate::character::modular::ModularCharacterPlugin;
use crate::character_studio::CharacterStudioPlugin;
use crate::combat::{
    data::CombatDataPlugin, feedback::CombatFeedbackPlugin, hitstop::HitstopPlugin,
    tricks::TrickDataPlugin,
};
use crate::engine::{
    game_loop::GameLoopPlugin, input_buffer::InputBufferPlugin, spatial_lod::SpatialLodPlugin,
    vfx::VfxPlugin,
};
use crate::engine_tools::{tool_windows::ToolWindowsPlugin, EngineToolsPlugin};
use crate::events::EventsPlugin;
use crate::graph::GraphRegistryPlugin;
use crate::plugins::*;
use crate::world::co_op_platformer::HeavyWaterPlatformerPlugin;
use crate::world::{missions::MissionPlugin, published_content::PublishedContentPlugin};

/// Shared runtime and authoring substrate currently used by every Starfall
/// application profile.
pub struct StarfallFoundationPlugins;

impl PluginGroup for StarfallFoundationPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(EventsPlugin)
            .add(GraphRegistryPlugin)
            .add(EngineToolsPlugin)
            .add(SpatialLodPlugin)
            .add(ToolWindowsPlugin)
            .add(GameLoopPlugin)
            .add(HitstopPlugin)
            .add(CombatDataPlugin)
            .add(TrickDataPlugin)
            .add(CombatFeedbackPlugin)
            .add(VfxPlugin)
            .add(SfxPlugin)
            .add(MusicPlayerPlugin)
            .add(InputBufferPlugin)
    }
}

/// Complete Heavy Water runtime composition. Heavy Water is the first-party
/// framework consumer and the default native demo project.
pub struct HeavyWaterDemoPlugins;

impl PluginGroup for HeavyWaterDemoPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(InputPlugin)
            .add(UiPlugin)
            .add(HeavyWaterPlatformerPlugin)
            .add(WorldPlugin)
            .add(PlayerPlugin)
            .add(CharacterPlugin)
            .add(CharacterDesignPlugin)
            .add(WeaponPlugin)
            .add(EnemyPlugin)
            .add(HackingPlugin)
            .add(ChestPlugin)
            .add(CompanionPlugin)
            .add(ArmorPlugin)
            .add(CraftingPlugin)
            .add(MissionPlugin)
            .add(PublishedContentPlugin)
            .add(PublishedVehicleRuntimePlugin)
            .add(PublishedSpacecraftRuntimePlugin)
            .add(SavePlugin)
            .add(HeavyEconomyPlugin)
            .add(HeavyBioPlugin)
            .add(HeavyCombatPlugin)
            .add(HeavyRegionsPlugin)
            .add(HeavyVehiclePlugin)
            .add(HeavyWorldEventsPlugin)
            .add(ChapterPlugin)
            .add(DiscoverablePlugin)
            .add(RadioPlugin)
            .add(VehiclePlugin)
            .add(RobotGaragePlugin)
            .add(ModularCharacterPlugin)
            .add(CharacterStudioPlugin)
            .add(ImportedCharacterForgePlugin)
    }
}

/// Native authoring screens compiled into the Designer profile.
#[cfg(feature = "designer")]
pub struct StarfallForgePlugins;

#[cfg(feature = "designer")]
impl PluginGroup for StarfallForgePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(CreatureForgePlugin)
            .add(WeaponForgePlugin)
            .add(VehicleForgePlugin)
            .add(SpaceshipForgePlugin)
    }
}
