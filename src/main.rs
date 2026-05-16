use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_rapier3d::prelude::*;

mod chapters;
mod characters;
mod components;
mod damage;
mod events;
mod lsystem;
mod perks;
mod plugins;
mod resources;
mod robots;
mod state;

use events::EventsPlugin;
use plugins::{
    ArmorPlugin, ChapterPlugin, CharacterDesignPlugin, CharacterPlugin, ChassisEditorPlugin,
    ChestPlugin, CompanionPlugin, CraftingPlugin, DiscoverablePlugin, EnemyPlugin, InputPlugin,
    PlayerPlugin, RadioPlugin, SavePlugin, UiPlugin, VehiclePlugin, WeaponPlugin, WorldPlugin,
};
use resources::{
    CameraShake, CharacterDesignData, GameSettings, LocalPlayerConfig, PlayerScore,
    PlayerSelectState, WaveInfo,
};
use state::AppState;

fn main() {
    App::new()
        // Default plugins with window setup
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Starfall I".to_string(),
                        resolution: WindowResolution::new(1280.0, 720.0),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
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
        .init_resource::<LocalPlayerConfig>()
        .init_resource::<PlayerSelectState>()
        .init_resource::<CharacterDesignData>()
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
        ))
        .run();
}
