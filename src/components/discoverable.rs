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
