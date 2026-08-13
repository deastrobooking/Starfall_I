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
    /// GLB-based character import, inspection, and non-destructive sculpting.
    ImportedCharacterForge,
    /// Modular weapon designer — build a sabre from parts and watch the stats
    /// fall out of the physical design.
    WeaponForge,
    /// Ground-vehicle recipe authoring with a live compiled preview.
    VehicleForge,
    /// Spacecraft recipe authoring with a live compiled preview.
    SpaceshipForge,
    ChapterSelect,
    RobotGarage,
    Playing,
    Paused,
    GameOver,
    Victory,
}
