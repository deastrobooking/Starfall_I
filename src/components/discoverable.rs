use bevy::prelude::*;

/// Things that can be picked up to permanently unlock something for the player.
/// Spawned by the chapter director at script-defined positions; collected on overlap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoverableKind {
    /// Vehicle blueprint (motorcycle, jet…). Stored by id in the save.
    Blueprint(&'static str),
    /// Modifies a primary weapon when equipped via PlayerLoadout.
    WeaponMod(&'static str),
    /// Modifies armor stats / adds resistance.
    ArmorMod(&'static str),
    /// Spawns the named companion as a permanent ally.
    CompanionRecruit(&'static str),
    /// Unlocks the Star Sabre (locked by default in Starfall I).
    BeamSabreUnlock,
    /// A stolen relic shard recovered for one of the Starfall scientists.
    ScientistRelic {
        scientist: &'static str,
        relic_id: &'static str,
    },
    /// Lore log; no mechanical effect, just radio chatter.
    LoreFragment(&'static str),
}

#[derive(Component, Debug, Clone)]
pub struct Discoverable {
    pub kind: DiscoverableKind,
    pub label: &'static str,
    pub bob_phase: f32,
}

impl Discoverable {
    pub fn new(kind: DiscoverableKind, label: &'static str) -> Self {
        Self {
            kind,
            label,
            bob_phase: 0.0,
        }
    }
}

#[derive(Component, Debug, Clone)]
pub enum PuzzleArchetype {
    OrderedSwitches,
    TimedCrystalChain {
        window_secs: f32,
    },
    CoOpFloorPlates {
        hold_secs: f32,
        required_players: usize,
    },
    BeamRouting,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PuzzleNodeKind {
    SwitchPylon,
    Crystal,
    FloorPlate,
    Relay,
}

#[derive(Component, Debug, Clone)]
pub struct PuzzleNode {
    pub relic_id: &'static str,
    pub scientist: &'static str,
    pub order: usize,
    pub kind: PuzzleNodeKind,
    pub active: bool,
    pub bob_phase: f32,
}

#[derive(Component, Debug, Clone)]
pub struct PuzzleRelicEncounter {
    pub relic_id: &'static str,
    pub scientist: &'static str,
    pub kind: DiscoverableKind,
    pub label: &'static str,
    pub hint: &'static str,
    pub archetype: PuzzleArchetype,
    pub reward_position: Vec3,
    pub total_nodes: usize,
    pub active_nodes: usize,
    pub next_switch_index: usize,
    pub timer_remaining: f32,
    pub hold_progress: f32,
    pub solved: bool,
    pub reward_spawned: bool,
}
