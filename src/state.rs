use bevy::prelude::*;

/// Top-level game phase.
#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash)]
pub enum AppState {
    #[default]
    MainMenu,
    /// Creator-facing launcher for opening a validated Forge workspace.
    ProjectHub,
    /// Creature recipe authoring screen (versioned `CreatureSpec` editing
    /// against a live robot-factory preview).
    CreatureForge,
    PlayerSelect,
    CharacterDesign,
    /// Human character generator studio — in-game mesh generation from preset
    /// templates (bodies, faces, clothes, super suits, mecha armor).
    CharacterStudio,
    ChapterSelect,
    RobotGarage,
    Playing,
    Paused,
    GameOver,
    Victory,
}
