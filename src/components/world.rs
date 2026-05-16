use bevy::prelude::*;

/// Marks static world geometry (buildings, ground, etc.)
#[derive(Component, Default)]
pub struct WorldGeometry;

/// Marks a building mesh.
#[derive(Component, Debug, Clone)]
pub struct Building {
    pub zone: WorldZone,
    pub height: f32,
}

/// Zone classification for buildings / terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WorldZone {
    #[default]
    Downtown,
    Industrial,
    Residential,
    Highway,
    Mountain,
    SkyPlatform,
    Spaceport,
    OuterDistrict,
    Ground,
}

/// Marks a chest entity.
#[derive(Component, Debug, Clone)]
pub struct Chest {
    pub is_open: bool,
    pub open_timer: f32,
    pub loot_type: LootType,
    pub loot_amount: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootType {
    Credits,
    Health,
    Armor,
    Ammo,
    WeaponUpgrade,
}

impl Chest {
    pub fn new(loot_type: LootType, amount: u32) -> Self {
        Self {
            is_open: false,
            open_timer: 0.0,
            loot_type,
            loot_amount: amount,
        }
    }
}

/// Neon light (for point-light signs).
#[derive(Component, Default)]
pub struct NeonLight;

/// Sky-city platform marker.
#[derive(Component, Default)]
pub struct SkyPlatform;

/// Walkable surface (used for raycast filtering equivalent).
#[derive(Component, Default)]
pub struct WalkableSurface;

/// Authored point used by chapter scripts to place bespoke encounters.
#[derive(Component, Debug, Clone)]
pub struct WorldAnchor {
    pub id: &'static str,
}

/// A world-space loot pickup spawned when enemies die.
#[derive(Component, Debug, Clone)]
pub struct WorldLoot {
    pub item_id: &'static str,
    pub quantity: u32,
    pub credits: u32,
    pub pickup_radius: f32,
    pub base_y: f32,
}
