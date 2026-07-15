// Starfall intentionally carries design data, future robot presets, and event
// fields before every gameplay path consumes them.
#![allow(dead_code)]
// Bevy systems naturally use broad query/resource signatures. Keep clippy strict
// for correctness-oriented lints while allowing those ECS-heavy shapes.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::physics::prelude::*;
use bevy::prelude::*;
use bevy::window::WindowResolution;

mod chapters;
mod character_blueprint;
mod character_parts;
mod character_studio;
mod characters;
mod commands;
mod components;
mod damage;
mod discussion;
mod engine_tools;
mod events;
mod final_war;
mod game_loop;
mod hacking;
mod hero_roster;
mod input_buffer;
mod lsystem;
mod modular_character;
mod perks;
mod physics;
mod player_mesh;
mod plugins;
mod procedural_meshes;
mod raids;
mod rendering;
mod resources;
mod robot_pets;
mod robots;
mod settlement_economy;
mod state;
mod upgrades;

use character_blueprint::{BodyRecipe, CartoonAppearanceRecipe};
use character_parts::{
    ArmPreset, BodyPreset, CharacterLoadout, HeadPreset, LegPreset, ShoulderPreset,
};
use commands::{CommandOverlayState, CommandRegistry};
use engine_tools::{EngineToolMode, EngineToolsPlugin};
use events::EventsPlugin;
use final_war::FinalWarRegistry;
use hacking::HackingRegistry;
use modular_character::ModularCharacterPlugin;
use perks::PerkTree;
use plugins::{
    ArmorPlugin, ChapterPlugin, CharacterDesignPlugin, CharacterPlugin, ChestPlugin,
    CompanionPlugin, CraftingPlugin, DiscoverablePlugin, EnemyPlugin, HackingPlugin, InputPlugin,
    PlayerPlugin, RadioPlugin, RobotGaragePlugin, SavePlugin, UiPlugin, VehiclePlugin,
    WeaponPlugin, WorldPlugin,
};
use raids::RaidRegistry;
use resources::{
    CameraShake, CharacterBaseModel, CharacterBaseModelCatalog, CharacterDesignData,
    CharacterDesignSnapshot, GameSettings, LocalPlayerConfig, PlaySessionTransition,
    PlayerPartLoadout, PlayerScore, PlayerSelectState, ShopCatalog, WaveInfo, WorldRouteRegistry,
    WorldSiteRegistry,
};
use robot_pets::RobotPetCollection;
use settlement_economy::SettlementEconomy;
use state::AppState;
use upgrades::UpgradeLedger;

fn main() {
    install_crash_logger();

    let mut app = App::new();

    app
        // Default plugins with window setup
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Starfall I".to_string(),
                        resolution: WindowResolution::new(1280, 720),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
                    ..default()
                }),
        )
        // Physics
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(PhysicsCompatPlugin)
        // State
        .init_state::<AppState>()
        // Global resources
        .init_resource::<WaveInfo>()
        .init_resource::<GameSettings>()
        .init_resource::<PlayerScore>()
        .init_resource::<CameraShake>()
        .init_resource::<PlaySessionTransition>()
        .init_resource::<LocalPlayerConfig>()
        .init_resource::<PlayerSelectState>()
        .init_resource::<CharacterDesignData>()
        .init_resource::<CharacterBaseModelCatalog>()
        .init_resource::<PerkTree>()
        .init_resource::<RobotPetCollection>()
        .init_resource::<SettlementEconomy>()
        .init_resource::<UpgradeLedger>()
        .init_resource::<PlayerPartLoadout>()
        .init_resource::<ShopCatalog>()
        .init_resource::<WorldSiteRegistry>()
        .init_resource::<WorldRouteRegistry>()
        .init_resource::<RaidRegistry>()
        .init_resource::<CommandRegistry>()
        .init_resource::<CommandOverlayState>()
        .init_resource::<HackingRegistry>()
        .init_resource::<FinalWarRegistry>()
        .register_type::<BodyRecipe>()
        .register_type::<CartoonAppearanceRecipe>()
        .register_type::<BodyPreset>()
        .register_type::<ArmPreset>()
        .register_type::<LegPreset>()
        .register_type::<ShoulderPreset>()
        .register_type::<HeadPreset>()
        .register_type::<CharacterLoadout>()
        .register_type::<CharacterBaseModel>()
        .register_type::<PlayerPartLoadout>()
        .register_type::<CharacterDesignSnapshot>()
        // Event infrastructure
        .add_plugins(EventsPlugin)
        // Shared recipe-editor services: tool mode, stable IDs, selection,
        // and transactional undo/redo. Player-facing workspace UI lands ET2.
        .add_plugins(EngineToolsPlugin)
        // EC0: engine-core loop scaffolding (GameSet ordering + profiling overlay).
        .add_plugins(game_loop::GameLoopPlugin)
        // EC1: deterministic per-player input buffer (additive; consumed by motor in EC1b).
        .add_plugins(input_buffer::InputBufferPlugin)
        .add_plugins(MaterialPlugin::<rendering::ToonMaterial>::default())
        // Game plugins
        .add_plugins((
            InputPlugin,
            UiPlugin,
            WorldPlugin,
            PlayerPlugin,
            CharacterPlugin,
            CharacterDesignPlugin,
            WeaponPlugin,
            EnemyPlugin,
            HackingPlugin,
            ChestPlugin,
            CompanionPlugin,
            ArmorPlugin,
            CraftingPlugin,
            SavePlugin,
        ))
        .add_plugins((
            ChapterPlugin,
            DiscoverablePlugin,
            RadioPlugin,
            VehiclePlugin,
            RobotGaragePlugin,
            ModularCharacterPlugin,
            character_studio::CharacterStudioPlugin,
        ));

    if std::env::var_os("STARFALL_AUTOSTART").is_some() {
        app.insert_state(AppState::Playing);
    }
    // Boot straight into the Character Studio for design-tool iteration.
    if std::env::var_os("STARFALL_STUDIO").is_some() {
        app.insert_state(AppState::CharacterStudio);
    }
    // Boot directly into a live chapter with Starfall Forge open for editor
    // smoke tests and rapid tool iteration.
    if std::env::var_os("STARFALL_EDITOR").is_some() {
        app.insert_state(AppState::Playing);
        app.insert_state(EngineToolMode::Editing);
    }

    app.run();
}

fn install_crash_logger() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let log = format!("Starfall I crash\n\n{panic_info}\n\nBacktrace:\n{backtrace}\n");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("starfall_crash.log");
        let _ = std::fs::write(path, log);
        default_hook(panic_info);
    }));
}
