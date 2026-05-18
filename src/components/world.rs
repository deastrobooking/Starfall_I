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

/// Kinematic platform that travels between two authored points.
#[derive(Component, Debug, Clone)]
pub struct MovingPlatform {
    pub start: Vec3,
    pub end: Vec3,
    pub speed: f32,
    pub phase: f32,
    pub size: Vec3,
}

/// Defensive world turret that tracks players and fires a hitscan-style beam.
#[derive(Component, Debug, Clone)]
pub struct LaserTurret {
    pub range: f32,
    pub cooldown: f32,
    pub cooldown_timer: f32,
    pub windup: f32,
    pub windup_timer: f32,
    pub locked_target: Option<Entity>,
    pub damage: f32,
    pub beam_material: Handle<StandardMaterial>,
}

/// Short-lived visual beam spawned by turrets and boss attacks.
#[derive(Component, Debug, Clone)]
pub struct LaserBeamVfx {
    pub timer: f32,
}

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

/// Usable boat placed at an authored dock. Press the vehicle input near it to
/// ride across water routes.
#[derive(Component, Debug, Clone)]
pub struct BoatVehicle {
    pub embark_radius: f32,
    pub dock_position: Vec3,
    pub island_position: Vec3,
}
