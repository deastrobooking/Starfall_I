use bevy::prelude::*;

use crate::robot_pets::{RobotPartKind, RobotPetRole};
use crate::upgrades::TechUpgradeId;

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
    /// Rescues a robot pet and adds it to the campaign-shared pet roster.
    RobotPetRescue {
        pet_id: &'static str,
        name: &'static str,
        role: RobotPetRole,
    },
    /// Unlocks the Star Sabre (locked by default in Starfall I).
    BeamSabreUnlock,
    /// A stolen relic shard recovered for one of the Starfall scientists.
    ScientistRelic {
        scientist: &'static str,
        relic_id: &'static str,
    },
    /// One of several small shards; collecting all shards restores the full relic.
    RelicFragment {
        scientist: &'static str,
        relic_id: &'static str,
        piece: u8,
        total: u8,
    },
    /// Optional cave route found inside a chapter's level space.
    SecretCave { chapter: u8, cave_id: &'static str },
    /// Hidden room or optional objective reward cache.
    HiddenReward {
        reward_id: &'static str,
        credits: u32,
        experience: u32,
        armor: u32,
        power_up: Option<&'static str>,
        special_ability: Option<&'static str>,
    },
    /// Data core recovered from a spy drone in a peaceful settlement.
    SpyData {
        data_id: &'static str,
        credits: u32,
        experience: u32,
        armor: u32,
    },
    /// Campaign-shared cache for robot parts, upgrade direction, and heal reserve.
    TechCache {
        cache_id: &'static str,
        parts: Vec<(RobotPartKind, u32)>,
        upgrade_hint: Option<TechUpgradeId>,
        rejuvenation_charge: u32,
    },
    /// Lore log; no mechanical effect, just radio chatter.
    LoreFragment(&'static str),
}

#[derive(Component, Debug, Clone)]
pub struct Discoverable {
    pub kind: DiscoverableKind,
    pub label: &'static str,
    pub base_y: f32,
    pub bob_phase: f32,
}

impl Discoverable {
    pub fn new(kind: DiscoverableKind, label: &'static str, base_y: f32) -> Self {
        Self {
            kind,
            label,
            base_y,
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

/// Moving obstacle spawned around small relic-fragment puzzles.
#[derive(Component, Debug, Clone)]
pub struct RelicFragmentObstacle {
    pub base: Vec3,
    pub travel: Vec3,
    pub speed: f32,
    pub phase: f32,
    pub spin_speed: f32,
}

/// Static temporary geometry belonging to a relic-fragment sub puzzle.
#[derive(Component, Debug, Clone)]
pub struct RelicFragmentPuzzlePiece {
    pub scientist: &'static str,
    pub relic_id: &'static str,
}
