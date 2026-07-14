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

/// Kinematic platform that orbits around an authored center while bobbing up and
/// down, used as a rotating elevator in traversal courses.
#[derive(Component, Debug, Clone)]
pub struct RotatingElevator {
    pub center: Vec3,
    pub radius: f32,
    pub angular_speed: f32,
    pub vertical_amplitude: f32,
    pub vertical_speed: f32,
    pub phase: f32,
    pub size: Vec3,
}

/// Launch pad that throws players across authored gaps when jump/interact is
/// pressed near the pad.
#[derive(Component, Debug, Clone)]
pub struct SlingShotPad {
    pub launch_velocity: Vec3,
    pub radius: f32,
    pub cooldown: f32,
    pub cooldown_timer: f32,
}

/// Hoverboard boost pad embedded into express roads and racing bridges.
#[derive(Component, Debug, Clone)]
pub struct BoardBoostPad {
    pub direction: Vec3,
    pub half_width: f32,
    pub half_length: f32,
    pub speed_mult: f32,
    pub impulse: f32,
    pub lift: f32,
    pub duration: f32,
    pub cooldown: f32,
    pub cooldown_timer: f32,
    pub force_hoverboard: bool,
}

/// Authored traversal guide for a complete vertical speed-road loop. Geometry
/// remains collidable scenery; this guide supplies the non-world-up adhesion
/// frame required by the kinematic hoverboard controller.
#[derive(Component, Debug, Clone, Copy)]
pub struct SpeedLoopGuide {
    pub radius: f32,
    pub yaw: f32,
    pub entry_speed: f32,
    pub lane_half_width: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct SpeedRoadCheckpoint {
    pub radius: f32,
}

/// Civilian or patrol vehicle riding the generated speed-road network.
#[derive(Component, Debug, Clone)]
pub struct NpcRoadVehicle {
    pub path: Vec<Vec3>,
    pub segment: usize,
    pub progress: f32,
    pub speed: f32,
    pub lane_offset: f32,
    pub hit_radius: f32,
    pub wreck_timer: f32,
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

/// Visible world-map marker for a chapter fast-travel destination.
#[derive(Component, Debug, Clone)]
pub struct ChapterMapMarker {
    pub chapter: u8,
    pub region: &'static str,
}

/// Peaceful NPC that can open a discussion script and optional voice acting.
#[derive(Component, Debug, Clone)]
pub struct DiscussionNpc {
    pub id: &'static str,
    pub display_name: &'static str,
    pub role: &'static str,
    pub script_id: &'static str,
    pub interact_radius: f32,
}

/// Build terminal for the settlement economy layer. Today it keys into
/// `map_settlements()`; M5 can later back it with full `WorldSite` state.
#[derive(Component, Debug, Clone)]
pub struct SettlementBuildTerminal {
    pub settlement_id: &'static str,
    pub settlement_name: &'static str,
    pub reward_id: &'static str,
    pub kind: crate::chapters::MapSettlementKind,
    pub origin: Vec3,
    pub facing_yaw: f32,
    pub interact_radius: f32,
}

/// Flying Free Peoples ship that patrols above protected cities.
#[derive(Component, Debug, Clone, Copy)]
pub struct FreePeopleGuardianShip {
    pub center: Vec3,
    pub radius: f32,
    pub altitude: f32,
    pub angular_speed: f32,
    pub phase: f32,
    pub bob: f32,
}

/// A collectible key item inside a dungeon that unlocks the boss gate.
#[derive(Component, Debug, Clone)]
pub struct DungeonKeyPickup {
    pub chapter: u8,
    pub collected: bool,
    pub pickup_radius: f32,
}

/// A door slab inside a dungeon that slides aside once the matching key is held.
#[derive(Component, Debug, Clone)]
pub struct DungeonKeyGate {
    pub chapter: u8,
    pub closed: Vec3,
    pub open: Vec3,
}

/// A spawn point inside a dungeon that fires an enemy wave when players enter range.
#[derive(Component, Debug, Clone)]
pub struct DungeonEnemySpawner {
    pub chapter: u8,
    pub enemy_type: crate::components::enemy::EnemyType,
    pub count: u8,
    pub trigger_radius: f32,
    pub difficulty: f32,
    pub spawned: bool,
}

/// Interactable entrance to a single-screen top-down castle/dungeon crawl.
#[derive(Component, Debug, Clone)]
pub struct DungeonCrawlGate {
    pub gate_id: &'static str,
    pub chapter: u8,
    pub label: &'static str,
    pub entry: Vec3,
    pub focus: Vec3,
    pub radius: f32,
    pub interact_radius: f32,
    pub opened: bool,
}

/// Visual door slab that slides away once its matching dungeon gate opens.
#[derive(Component, Debug, Clone)]
pub struct DungeonGateDoor {
    pub gate_id: &'static str,
    pub chapter: u8,
    pub closed: Vec3,
    pub open: Vec3,
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
    pub passenger_radius: f32,
    pub dock_radius: f32,
    pub route_half_width: f32,
    pub speed: f32,
    pub dock_position: Vec3,
    pub island_position: Vec3,
}

/// Marks a player currently riding a boat. The vehicle system owns their
/// transform while this component is present.
#[derive(Component, Debug, Clone, Copy)]
pub struct BoatPassenger {
    pub boat: Entity,
    pub seat: u8,
    pub is_driver: bool,
}

/// Tags a world entity (enemy group, patrol, or visual marker) as belonging to
/// a WorldSite so the liberation system can track enemy deaths by site.
#[derive(Component, Debug, Clone, Copy)]
pub struct WorldSiteMarker {
    pub id: crate::resources::WorldSiteId,
}

/// Marks the command terminal that spawns at a liberated site.
#[derive(Component, Debug, Clone, Copy)]
pub struct SiteCommandTerminal {
    pub id: crate::resources::WorldSiteId,
    pub interact_radius: f32,
}

/// Invisible site anchor that triggers enemy spawning when players approach,
/// then tracks whether the site's defender group has been defeated.
#[derive(Component, Debug, Clone)]
pub struct WorldSiteEnemySentinel {
    pub site_id: crate::resources::WorldSiteId,
    pub trigger_radius: f32,
    pub spawned: bool,
    pub enemy_count: u8,
    pub liberated_spawned: bool,
}

/// Tags the beacon entity spawned at a world route midpoint.
#[derive(Component, Debug, Clone, Copy)]
pub struct WorldRouteMarker {
    pub id: crate::resources::WorldRouteId,
}
