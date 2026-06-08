// Starfall intentionally carries design data, future robot presets, and event
// fields before every gameplay path consumes them.
#![allow(dead_code)]
// Bevy systems naturally use broad query/resource signatures. Keep clippy strict
// for correctness-oriented lints while allowing those ECS-heavy shapes.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_rapier3d::prelude::*;

mod chapters;
mod character_blueprint;
mod characters;
mod components;
mod damage;
mod events;
mod hero_roster;
mod lsystem;
mod perks;
mod plugins;
mod rendering;
mod resources;
mod robot_pets;
mod robots;
mod state;
mod upgrades;

use events::EventsPlugin;
use perks::PerkTree;
use plugins::{
    ArmorPlugin, ChapterPlugin, CharacterDesignPlugin, CharacterPlugin, ChassisEditorPlugin,
    ChestPlugin, CompanionPlugin, CraftingPlugin, DiscoverablePlugin, EnemyPlugin, InputPlugin,
    PlayerPlugin, RadioPlugin, SavePlugin, UiPlugin, VehiclePlugin, WeaponPlugin, WorldPlugin,
};
use resources::{
    CameraShake, CharacterDesignData, GameSettings, LocalPlayerConfig, PlaySessionTransition,
    PlayerScore, PlayerSelectState, WaveInfo,
};
use robot_pets::RobotPetCollection;
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
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
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
        .init_resource::<PerkTree>()
        .init_resource::<RobotPetCollection>()
        .init_resource::<UpgradeLedger>()
        // Event infrastructure
        .add_plugins(EventsPlugin)
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
            ChassisEditorPlugin,
        ));

    if std::env::var_os("STARFALL_AUTOSTART").is_some() {
        app.insert_state(AppState::Playing);
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
