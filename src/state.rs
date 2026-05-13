use bevy::prelude::*;

/// Top-level game phase.
#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash)]
pub enum AppState {
    #[default]
    MainMenu,
    PlayerSelect,
    ChapterSelect,
    ChassisEditor,
    Playing,
    Paused,
    GameOver,
}
