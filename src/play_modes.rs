//! Renderer-neutral mode catalog. A host supplies the adapter for each mode;
//! genre and camera metadata never silently enable an unfinished runtime.

use std::collections::BTreeMap;
use std::fmt;

use crate::graph::StableId;

/// A mode's primary rules. Puzzles can also be composed into any of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayStyle {
    OpenWorld,
    Racing,
    Arena,
    Platformer,
    Puzzle,
    Custom,
}

/// Presentation policy implemented by a host's camera adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraStyle {
    PerPlayer3d,
    SharedArena3d,
    SharedPlatformer3d,
    Custom,
}

/// One selectable mode and its native runtime adapter. Registration means the
/// adapter is ready to launch; planned modes belong in documentation.
#[derive(Debug, Clone)]
pub struct PlayMode<A> {
    pub id: StableId,
    pub label: String,
    pub style: PlayStyle,
    pub camera: CameraStyle,
    pub min_players: u8,
    pub max_players: u8,
    pub adapter: A,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeError {
    InvalidDefinition,
    Duplicate(String),
    Unknown(String),
    PlayerCount { requested: u8, min: u8, max: u8 },
}

impl fmt::Display for ModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDefinition => {
                write!(f, "Mode needs a label and a player range within 1–4")
            }
            Self::Duplicate(id) => write!(f, "Mode '{id}' is already registered"),
            Self::Unknown(id) => write!(f, "Mode '{id}' is not installed"),
            Self::PlayerCount {
                requested,
                min,
                max,
            } => {
                write!(f, "Mode needs {min}–{max} players; party has {requested}")
            }
        }
    }
}

impl std::error::Error for ModeError {}

/// Deterministic catalog shared by launch menus, portals, and native callers.
#[derive(Debug, Clone)]
pub struct ModeRegistry<A> {
    entries: BTreeMap<String, PlayMode<A>>,
}

impl<A> Default for ModeRegistry<A> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

impl<A> ModeRegistry<A> {
    pub fn register(&mut self, mode: PlayMode<A>) -> Result<(), ModeError> {
        if mode.label.trim().is_empty()
            || mode.min_players == 0
            || mode.max_players > 4
            || mode.min_players > mode.max_players
        {
            return Err(ModeError::InvalidDefinition);
        }
        let id = mode.id.to_string();
        if self.entries.contains_key(&id) {
            return Err(ModeError::Duplicate(id));
        }
        self.entries.insert(id, mode);
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &PlayMode<A>> {
        self.entries.values()
    }

    /// Validate before a host captures progress or tears down the current scene.
    pub fn resolve(&self, id: &str, players: u8) -> Result<&PlayMode<A>, ModeError> {
        let mode = self
            .entries
            .get(id)
            .ok_or_else(|| ModeError::Unknown(id.into()))?;
        if !(mode.min_players..=mode.max_players).contains(&players) {
            return Err(ModeError::PlayerCount {
                requested: players,
                min: mode.min_players,
                max: mode.max_players,
            });
        }
        Ok(mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(id: &str, adapter: u8) -> PlayMode<u8> {
        PlayMode {
            id: StableId::new(id).unwrap(),
            label: "Custom race".into(),
            style: PlayStyle::Racing,
            camera: CameraStyle::PerPlayer3d,
            min_players: 2,
            max_players: 4,
            adapter,
        }
    }

    #[test]
    fn failed_registration_preserves_the_installed_adapter() {
        let mut registry = ModeRegistry::default();
        registry.register(mode("example.race", 1)).unwrap();
        assert!(registry.register(mode("example.race", 2)).is_err());
        assert_eq!(registry.resolve("example.race", 4).unwrap().adapter, 1);
        let mut invalid = mode("example.invalid", 3);
        invalid.max_players = 5;
        assert_eq!(
            registry.register(invalid),
            Err(ModeError::InvalidDefinition)
        );
        assert_eq!(registry.iter().count(), 1);
    }

    #[test]
    fn launch_rejects_missing_modes_and_incompatible_parties() {
        let mut registry = ModeRegistry::default();
        registry.register(mode("example.race", 1)).unwrap();
        assert!(matches!(
            registry.resolve("missing", 2),
            Err(ModeError::Unknown(_))
        ));
        for count in [0, 1, 5, 255] {
            assert!(matches!(
                registry.resolve("example.race", count),
                Err(ModeError::PlayerCount { .. })
            ));
        }
        assert!(registry.resolve("example.race", 2).is_ok());
    }
}
