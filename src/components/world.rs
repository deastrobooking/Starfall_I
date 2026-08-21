#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
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

/// Marks a generated building whose shell, floors, stairs, and rooms are
/// physically explorable rather than represented by one solid collider.
#[derive(Component, Debug, Clone, Copy)]
pub struct EnterableBuilding {
    pub accessible_floors: u8,
    pub footprint: Vec2,
}

/// Traversal connection generated between two explorable city rooftops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CityRooftopRouteKind {
    Skybridge,
    Zipline,
}

/// Marks the primary geometry for a generated rooftop traversal connection.
#[derive(Component, Debug, Clone, Copy)]
pub struct CityRooftopRoute {
    pub kind: CityRooftopRouteKind,
    pub start: Vec3,
    pub end: Vec3,
}

/// Deterministic room-dressing family used by an explorable city building.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingInteriorKind {
    Lobby,
    Market,
    Home,
    Laboratory,
}

/// Marks an exterior lift serving an explorable building.
#[derive(Component, Debug, Clone, Copy)]
pub struct BuildingLift;

/// Marks the real opening and readable trim of a building roof hatch.
#[derive(Component, Debug, Clone, Copy)]
pub struct BuildingRoofHatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingRewardLocation {
    Interior,
    Rooftop,
}

/// One-time, save-backed reward for following an authored building route.
#[derive(Component, Debug, Clone)]
pub struct BuildingExplorationReward {
    pub reward_key: String,
    pub location: BuildingRewardLocation,
    pub credits: u32,
    pub experience: u32,
    pub armor: u32,
    pub pickup_radius: f32,
    pub base_y: f32,
    pub bob_phase: f32,
}

/// Identifies the readable traversal pieces attached to city buildings.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CityAccessKind {
    GroundEntrance,
    ExteriorStair,
    Balcony,
    BalconyEntrance,
    Ladder,
    RoofLanding,
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

/// Automatic Sonic-style spring embedded in stunt roads. Unlike a slingshot,
/// it fires on contact and preserves an authored forward race direction.
#[derive(Component, Debug, Clone)]
pub struct SpringJumpPad {
    pub launch_velocity: Vec3,
    pub radius: f32,
    pub cooldown: f32,
    /// Independent retrigger windows keep a leading split-screen rider from
    /// disabling the spring for teammates arriving a few frames later.
    pub cooldown_timers: [f32; 4],
    pub force_hoverboard: bool,
}

/// Jump-triggered panel that rotates to its opposite traversal face. The
/// authored transform remains the stable base so repeated toggles never drift.
#[derive(Component, Debug, Clone)]
pub struct FlipPlatform {
    pub base_rotation: Quat,
    pub flipped: bool,
    pub progress: f32,
    pub turn_seconds: f32,
    pub trigger_radius: f32,
    pub cooldown: f32,
    pub cooldown_timer: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollapsePlatformState {
    #[default]
    Armed,
    Warning,
    Fallen,
    Resetting,
}

/// Timed bridge that warns, drops out, and restores itself. State is shared by
/// the platform while contact tests are per player, preventing co-op riders
/// from racing independent copies of the same bridge state.
#[derive(Component, Debug, Clone)]
pub struct CollapsePlatform {
    pub origin: Vec3,
    pub size: Vec3,
    pub warning_seconds: f32,
    pub fallen_seconds: f32,
    pub drop_distance: f32,
    pub timer: f32,
    pub state: CollapsePlatformState,
}

/// Environmental contact hazard with independent retrigger timers for P1–P4.
#[derive(Component, Debug, Clone)]
pub struct SpikePlatformHazard {
    pub size: Vec3,
    pub damage: f32,
    pub cooldown: f32,
    pub cooldown_timers: [f32; 4],
}

/// An authored grind line. Players and hoverboards snap to the line when they
/// land close to it, travel independently, and receive an exit impulse.
#[derive(Component, Debug, Clone, Copy)]
pub struct StuntGrindRail {
    pub start: Vec3,
    pub end: Vec3,
    pub speed: f32,
    pub snap_radius: f32,
    pub exit_lift: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct RailGrindState {
    pub rail: Entity,
    pub progress: f32,
    pub direction: f32,
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
    /// Owner-scoped contact cooldowns keep one local rider from consuming a
    /// wide road pad before the rest of the split-screen party reaches it.
    pub cooldown_timers: [f32; 4],
    pub force_hoverboard: bool,
}

/// Collidable road containment. Outer rails prevent falls; center dividers
/// keep opposite boost-arrow directions physically separated.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadSafetyBarrier {
    OuterRail,
    DirectionDivider,
    MergeRail,
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

/// Ordered checkpoint on a closed settlement-ring race course.
#[derive(Component, Debug, Clone, Copy)]
pub struct StuntRaceGate {
    pub course_id: &'static str,
    pub course_label: &'static str,
    pub gate_index: u8,
    pub gate_count: u8,
    pub radius: f32,
}

/// NPC road vehicle promoted to a named racing opponent.
#[derive(Component, Debug, Clone, Copy)]
pub struct StuntRaceOpponent {
    pub racer_id: u8,
    pub course_id: &'static str,
}

/// Hostile hoverboard rival following a race spline. Kept separate from the
/// ordinary combat AI so racing movement remains deterministic at extreme
/// speeds and can later receive dedicated board-combat behavior.
#[derive(Component, Debug, Clone, Copy)]
pub struct EnemyHoverboardSurfer;

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

/// Optional local-traffic behavior layered over the shared road-path follower.
#[derive(Component, Debug, Clone, Copy)]
pub struct CityStreetTraffic {
    pub cruise_speed: f32,
    pub stop_timer: f32,
    pub last_stopped_segment: usize,
    pub signal_phase: usize,
}

/// A friendly city resident following a closed sidewalk route.
#[derive(Component, Debug, Clone)]
pub struct CityPedestrian {
    pub path: Vec<Vec3>,
    pub segment: usize,
    pub progress: f32,
    pub speed: f32,
    pub pause_timer: f32,
    pub phase: f32,
}

/// Decorative parked city vehicle excluded from combat and traffic simulation.
#[derive(Component, Debug, Clone, Copy)]
pub struct CityParkedVehicle;

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

/// Optional companion to an enemy spawner: spawn a published Creature Forge
/// recipe (resolved by stable content id through `PublishedCreatureCatalog`)
/// instead of the spawner's built-in enemy type. Falls back to the built-in
/// type when the id is not published in the active project.
#[derive(Component, Debug, Clone)]
pub struct CreatureSpawnOverride(pub String);

/// A spawn point inside a dungeon that fires an enemy wave when players enter range.
#[derive(Component, Debug, Clone)]
pub struct DungeonEnemySpawner {
    pub chapter: u8,
    pub encounter: Option<(&'static str, u8)>,
    /// Ordered encounter beat. Wave N waits until every lower wave has
    /// spawned and its owned enemies have been defeated.
    pub wave_index: u8,
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

impl DungeonCrawlGate {
    pub fn contains_interaction(&self, position: Vec3) -> bool {
        position.distance(self.entry) <= self.interact_radius
    }
}

/// Marks the monumental exterior structure surrounding a mountain-cave gate.
/// Fast travel is optional; this world-space entrance remains the canonical
/// way to discover and enter its linked dungeon.
#[derive(Component, Debug, Clone, Copy)]
pub struct AncientCaveGate {
    pub gate_id: &'static str,
    pub clear_width: f32,
    pub height: f32,
}

/// An explicit return point for a linked shared-screen dungeon. Interacting
/// with the portal returns the complete local party to safe exterior slots.
#[derive(Component, Debug, Clone)]
pub struct DungeonExitPortal {
    pub gate_id: &'static str,
    pub position: Vec3,
    pub return_positions: [Vec3; 4],
    pub interact_radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DungeonRoomKind {
    Entrance,
    Traversal,
    Combat,
    Reward,
}

/// A camera/progression zone in a shared-screen dungeon room graph.
#[derive(Component, Debug, Clone, Copy)]
pub struct DungeonRoomZone {
    pub gate_id: &'static str,
    pub room_index: u8,
    pub label: &'static str,
    pub kind: DungeonRoomKind,
    pub focus: Vec3,
    pub camera_radius: f32,
}

/// An undirected graph edge connecting two dungeon rooms. The marker position
/// is also available to future door, lock, minimap, and encounter systems.
#[derive(Component, Debug, Clone, Copy)]
pub struct DungeonRoomPortal {
    pub gate_id: &'static str,
    pub room_a: u8,
    pub room_b: u8,
    pub position: Vec3,
}

/// Identifies an enemy spawned for one room's authored combat encounter.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonEncounterEnemy {
    pub gate_id: &'static str,
    pub room_index: u8,
    pub wave_index: u8,
}

/// A physical doorway that seals while its room encounter is active and
/// opens permanently after the encounter has been cleared this session.
#[derive(Component, Debug, Clone)]
pub struct DungeonEncounterDoor {
    pub gate_id: &'static str,
    pub room_index: u8,
    /// Optional prior room that must be cleared before this threshold opens.
    pub requires_room_clear: Option<u8>,
    pub closed: Vec3,
    pub open: Vec3,
}

/// A chamber reward that becomes visible when its encounter is cleared.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct DungeonEncounterReward {
    pub gate_id: &'static str,
    pub room_index: u8,
    pub credits: u32,
    pub healing: f32,
    pub claimed: bool,
}

impl DungeonRoomPortal {
    pub fn connects(&self, room_a: u8, room_b: u8) -> bool {
        (self.room_a == room_a && self.room_b == room_b)
            || (self.room_a == room_b && self.room_b == room_a)
    }
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
    /// World-space homing velocity. Loot starts with a short upward pop and
    /// then steers toward the nearest player like a reversed homing missile.
    pub velocity: Vec3,
    pub age: f32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterBodyKind {
    Ocean,
    Lake,
    River,
    Waterfall,
}

#[derive(Debug, Clone, Copy)]
pub enum WaterFootprint {
    Rectangle { half_extents: Vec2 },
    Ellipse { radii: Vec2 },
}

impl WaterFootprint {
    pub fn contains(self, local_xz: Vec2) -> bool {
        match self {
            Self::Rectangle { half_extents } => {
                local_xz.x.abs() <= half_extents.x && local_xz.y.abs() <= half_extents.y
            }
            Self::Ellipse { radii } => {
                let normalized = Vec2::new(
                    local_xz.x / radii.x.max(0.001),
                    local_xz.y / radii.y.max(0.001),
                );
                normalized.length_squared() <= 1.0
            }
        }
    }
}

/// Queryable gameplay metadata attached to rendered water surfaces.
#[derive(Component, Debug, Clone, Copy)]
pub struct WaterBody {
    pub kind: WaterBodyKind,
    pub surface_y: f32,
    pub depth: f32,
    pub navigable: bool,
    pub footprint: WaterFootprint,
}

/// Named offshore destination reached through an authored boat lane.
#[derive(Component, Debug, Clone, Copy)]
pub struct TravelIsland {
    pub id: &'static str,
    pub label: &'static str,
    pub dock_position: Vec3,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dungeon_gate_interaction_boundary_is_shared_by_gate_and_npc_arbitration() {
        let gate = DungeonCrawlGate {
            gate_id: "test_gate",
            chapter: 1,
            label: "Test Gate",
            entry: Vec3::new(10.0, 2.0, -4.0),
            focus: Vec3::ZERO,
            radius: 40.0,
            interact_radius: 6.0,
            opened: false,
        };
        assert!(gate.contains_interaction(gate.entry + Vec3::X * 6.0));
        assert!(!gate.contains_interaction(gate.entry + Vec3::X * 6.01));
    }

    #[test]
    fn rectangular_water_footprint_rejects_points_past_its_edges() {
        let footprint = WaterFootprint::Rectangle {
            half_extents: Vec2::new(8.0, 3.0),
        };

        assert!(footprint.contains(Vec2::new(7.9, -2.9)));
        assert!(!footprint.contains(Vec2::new(8.1, 0.0)));
        assert!(!footprint.contains(Vec2::new(0.0, 3.1)));
    }

    #[test]
    fn elliptical_water_footprint_follows_the_shoreline() {
        let footprint = WaterFootprint::Ellipse {
            radii: Vec2::new(10.0, 5.0),
        };

        assert!(footprint.contains(Vec2::new(6.0, 3.0)));
        assert!(!footprint.contains(Vec2::new(8.0, 4.0)));
    }
}

/// Tags the beacon entity spawned at a world route midpoint.
#[derive(Component, Debug, Clone, Copy)]
pub struct WorldRouteMarker {
    pub id: crate::resources::WorldRouteId,
}
