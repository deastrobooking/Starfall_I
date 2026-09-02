use bevy::asset::RenderAssetUsages;
use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, SpatialScale, Volume};
use bevy::image::{
    CompressedImageFormats, Image, ImageAddressMode, ImageLoaderSettings, ImageSampler,
    ImageSamplerDescriptor, ImageType,
};
use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::math::Affine2;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TextureFormat,
};
use bevy::shader::ShaderRef;
use std::collections::{HashMap, VecDeque};
use std::f32::consts::TAU;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::audio::sfx::ModularActionSfxEvent;
use crate::chapters::{
    chapter_map_locations, map_settlements, MapSettlement, MapSettlementKind, SecretCaveLocation,
    EVEREST_RANGE_HALF_EXTENT, EVEREST_RANGE_WORLD_SIZE, RACE_REGION_TRAVEL_POINTS,
    SECRET_CAVE_LOCATIONS,
};
use crate::character::modular::bake_character_mesh;
use crate::combat::damage::{DamageInfo, DamageType, Damageable, Health};
use crate::combat::tricks::{ActiveTrick, TrickCategory, TrickDir, TrickLibrary, TrickState};
use crate::commands::CommandRegistry;
use crate::components::armor::ArmorSet;
use crate::components::discoverable::DiscoverableKind;
use crate::components::enemy::{
    CitySpyDrone, DroneVariant, Enemy, EnemyAIState, EnemyStateMachine, EnemyType,
    MountainSpiderMech, SpiderMechLeg,
};
use crate::components::faction::Faction;
use crate::components::player::{
    BoardBoostState, ClimbState, EdgeGrabState, FlightMode, GrappleHookState, GrappleSocket,
    GrappleTargetKind, JetpackState, ParryState, Player, PlayerIndex, PlayerInput, PlayerMovement,
    PlayerProgression, PlayerStateMachine, PlayerStats, RooftopTrialProgress, StuntRaceProgress,
    StuntRunState, TraversalMode, TraversalModeState, WaterTraversalState,
};
use crate::components::world::*;
use crate::engine::rendering::{
    DirectionalLightBundle, PbrBundle, PointLightBundle, SpatialBundle, TerrainMaterial,
    TerrainMaterialUniform, TerrainPbrBundle, WaterMaterial, WaterMaterialUniform,
};
use crate::engine::state::AppState;
use crate::engine_tools::ProceduralMaterialBinding;
use crate::events::{PlayerDamagedEvent, PlayerParryEvent, UiMessageEvent};
use crate::lsystem::tree::{spawn_tree, TreeKind, TreeRoot, TreeTemplate};
use crate::plugins::chapter_plugin::spawn_discoverable_beacon;
use crate::plugins::enemy_plugin::{spawn_drone_variant_entity, spawn_enemy_entity};
use crate::plugins::vehicle_plugin::{VehicleSet, VehicleState};
use crate::resources::{
    initial_world_routes, initial_world_sites, ChapterProgress, CurrentChapter, DungeonCrawlState,
    DungeonRoomState, GameSettings, PlayExperience, PlaySessionTransition, WorldRouteRegistry,
    WorldRouteState, WorldSiteRegistry, WorldSiteState,
};
use crate::world::arcade_dungeon::{spawn_arcade_prototype_dungeon, ArcadeDungeonPortal};
use crate::world::co_op_platformer::{
    platformer_checkpoint_and_reward_system, platformer_encounter_system,
    platformer_party_boundary_and_bubble_system, platformer_pressure_gate_system,
    spawn_shared_platformer_stage, PlatformerWorkshopProgress,
};
use crate::world::discussion::{
    discussion_script, settlement_discussion_id, settlement_guardian_role, DiscussionState,
};
use crate::world::final_war::{
    coordinated_raid_trigger_system, final_war_phase_announce_system, pressure_accumulation_system,
    win_condition_system,
};
use crate::world::published_craft::PublishedFleet;
use crate::world::raids::{
    static_defense_score_for_site, RaidId, RaidKind, RaidRegistry, RaidStartResult,
    RaidThreatMarker, RaidTickEvent, RaidUfoMarker, CLOUDRAIL_TUTORIAL_RAID_SITE,
};
use crate::world::robot_pets::RobotPetCollection;
use crate::world::settlement_economy::{
    settlement_build_def, SettlementBuildKind, SettlementEconomy,
};

mod grass;
mod roads;
use grass::*;
use roads::*;

#[derive(Resource, Default)]
struct ColliderDebugState {
    enabled: bool,
}

#[derive(Component)]
struct ColliderDebugSource;

#[derive(Component)]
struct ColliderDebugVisual;

/// Lazy encounter anchor for heavy aerial patrols. Every authored position is
/// well outside the protected starting region, so loading a game does not
/// populate or pressure the start point.
#[derive(Component, Debug, Clone, Copy)]
struct WastelandDronePatrol {
    spawned: bool,
    trigger_radius: f32,
}

const STARTING_REGION_SAFE_RADIUS: f32 = 2_000.0;
const WASTELAND_DRONE_PATROLS: &[Vec2] = &[
    Vec2::new(2_850.0, -2_350.0),
    Vec2::new(-3_250.0, 2_650.0),
    Vec2::new(5_450.0, 1_150.0),
    Vec2::new(-5_150.0, -1_850.0),
];

#[derive(Resource, Clone)]
struct WaterWakeAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Resource, Clone)]
struct CityRooftopRouteAssets {
    bridge: Handle<StandardMaterial>,
    guard: Handle<StandardMaterial>,
    zipline: Handle<StandardMaterial>,
    playground: Handle<StandardMaterial>,
    greenery: Handle<StandardMaterial>,
    bridge_marker: Handle<Mesh>,
    zipline_marker: Handle<Mesh>,
    planter_mesh: Handle<Mesh>,
    flower_mesh: Handle<Mesh>,
    spring_mesh: Handle<Mesh>,
    spring_arrow_mesh: Handle<Mesh>,
    trial_gate_mesh: Handle<Mesh>,
}

#[derive(Component)]
struct WaterWakeRipple {
    remaining: f32,
    duration: f32,
}

#[derive(Resource, Clone)]
struct WaterfallFxAssets {
    mist_mesh: Handle<Mesh>,
    droplet_mesh: Handle<Mesh>,
    mist_material: Handle<StandardMaterial>,
    audio: Handle<AudioSource>,
    volume: f32,
}

#[derive(Component)]
struct WaterfallFlowRibbon {
    base_translation: Vec3,
    base_scale: Vec3,
    phase: f32,
}

#[derive(Component)]
struct WaterfallParticle {
    origin: Vec3,
    velocity: Vec3,
    base_scale: Vec3,
    age: f32,
    lifetime: f32,
    gravity: f32,
    looped: bool,
    phase: f32,
}

#[derive(Component)]
struct WaterfallSplashZone {
    radius: f32,
    height: f32,
    occupied_players: u8,
}

#[derive(Component)]
struct BuildingRewardBeacon {
    reward_key: String,
}

#[derive(Component, Debug, Clone)]
struct CityRooftopRouteChallenge {
    reward_key: String,
    approach: [i8; 4],
    credits: u32,
    experience: u32,
    armor: u32,
}

#[derive(Component, Debug, Clone)]
struct CityRooftopRouteMarker {
    reward_key: String,
    kind: CityRooftopRouteKind,
    base_scale: Vec3,
    phase: f32,
}

#[derive(Component, Debug, Clone, Copy)]
struct CitySkyPlayground;

#[derive(Component, Debug, Clone)]
struct CityRooftopTrialGate {
    course_key: String,
    course_label: String,
    checkpoint_index: u8,
    checkpoint_count: u8,
    radius: f32,
    gold_time: f32,
    silver_time: f32,
    bronze_time: f32,
}

#[derive(Component, Debug, Clone, Copy)]
struct DebugColliderBox {
    size: Vec3,
}

#[derive(Component)]
struct BuildingLodClustered;

#[derive(Component)]
pub(crate) struct BuildingClusterLodProxy;

#[derive(Component)]
pub(crate) struct TerrainHighLodPatch;

#[derive(Component)]
pub(crate) struct TerrainProxyLodPatch;

/// Physics backends normally multiply collider shapes by `GlobalTransform` scale.
/// Scaled unit-cube roads/bridges below already specify collider half-extents
/// in world units, so this prevents invisible oversized blockers.
fn world_space_collider_scale() -> crate::engine::physics::prelude::ColliderScale {
    crate::engine::physics::prelude::ColliderScale::Absolute(Vec3::ONE)
}

#[derive(Component, Clone, Copy)]
struct NatureSway {
    base_translation: Vec3,
    base_rotation: Quat,
    base_scale: Vec3,
    phase: f32,
    speed: f32,
    sway: f32,
    bob: f32,
    pulse: f32,
}

impl NatureSway {
    fn new(transform: &Transform, phase: f32, speed: f32, sway: f32, bob: f32, pulse: f32) -> Self {
        Self {
            base_translation: transform.translation,
            base_rotation: transform.rotation,
            base_scale: transform.scale,
            phase,
            speed,
            sway,
            bob,
            pulse,
        }
    }
}

#[derive(Resource, Debug, Clone)]
struct DayNightCycle {
    time_of_day: f32,
    seconds_per_day: f32,
}

const DAY_NIGHT_SECONDS_PER_DAY: f32 = 1_800.0;

impl Default for DayNightCycle {
    fn default() -> Self {
        Self {
            // Late morning: players load into a readable world, then naturally
            // drift into afternoon, sunset, moonrise, and night.
            time_of_day: 0.38,
            seconds_per_day: DAY_NIGHT_SECONDS_PER_DAY,
        }
    }
}

#[derive(Component)]
struct SkyDome;

#[derive(Component)]
struct DayNightSunLight;

#[derive(Component)]
struct DayNightMoonLight;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CelestialKind {
    Sun,
    Moon,
}

#[derive(Component, Debug, Clone, Copy)]
struct CelestialVisual {
    kind: CelestialKind,
    phase_offset: f32,
    distance: f32,
}

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct WorldPlugin;

/// Last completed procedural-world boot. Kept as a resource so automated
/// smoke runs and logs can distinguish save I/O from world/mesh generation
/// stalls.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct WorldLoadDiagnostics {
    pub generation_seconds: f32,
    pub meshes_added: usize,
    pub standard_materials_added: usize,
}

/// Unit primitives shared by the complete generated speed-road network.
///
/// Road pieces express their authored dimensions through `Transform::scale`,
/// so one cube can back decks, dividers, boost pads, chevrons, and ramps. The
/// sphere is shared by every edge light. Keeping these handles at network
/// scope avoids allocating tens of thousands of identical `Mesh` assets while
/// preserving every visual entity and gameplay trigger.
#[derive(Debug, Clone)]
struct SpeedRoadMeshCache {
    unit_cube: Handle<Mesh>,
    unit_sphere: Handle<Mesh>,
}

impl SpeedRoadMeshCache {
    fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            unit_cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            unit_sphere: meshes.add(Sphere::new(1.0)),
        }
    }
}

/// Unit primitives shared by the downtown facade-detail pass.
///
/// Window grids and cladding deliberately remain separate entities so their
/// authored layout and materials are unchanged; only their repeated primitive
/// geometry is shared. Rooftop cylinders and beacons use the same approach.
#[derive(Debug, Clone)]
struct DowntownFacadeMeshCache {
    unit_cube: Handle<Mesh>,
    unit_cylinder: Handle<Mesh>,
    unit_sphere: Handle<Mesh>,
}

impl DowntownFacadeMeshCache {
    fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            unit_cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            unit_cylinder: meshes.add(Cylinder::new(1.0, 1.0)),
            unit_sphere: meshes.add(Sphere::new(1.0)),
        }
    }
}

/// Unit primitives shared by one generated forest or bio-city batch.
///
/// Authored dimensions stay in each entity's transform, so thousands of
/// trunks, roots, crowns, moss pads, vines, and rails can reuse four meshes
/// without changing their world-space silhouettes or animation bases.
#[derive(Debug, Clone)]
struct NaturePrimitiveMeshCache {
    unit_cube: Handle<Mesh>,
    unit_sphere: Handle<Mesh>,
    unit_cylinder: Handle<Mesh>,
    unit_cone: Handle<Mesh>,
}

impl NaturePrimitiveMeshCache {
    fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            unit_cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            unit_sphere: meshes.add(Sphere::new(1.0)),
            unit_cylinder: meshes.add(Cylinder::new(1.0, 1.0)),
            unit_cone: meshes.add(Cone::new(1.0, 1.0)),
        }
    }
}

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ColliderDebugState>()
            .init_resource::<DayNightCycle>()
            .init_resource::<DungeonCrawlState>()
            .init_resource::<DungeonRoomState>()
            .init_resource::<WorldSiteRegistry>()
            .init_resource::<WorldRouteRegistry>()
            .init_resource::<RaidRegistry>()
            .init_resource::<DiscussionState>()
            .init_resource::<PlatformerWorkshopProgress>()
            .add_plugins(GrassPlugin)
            .add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .add_systems(
                OnEnter(AppState::Playing),
                reset_platformer_progress.before(generate_city),
            )
            .add_systems(OnEnter(AppState::Playing), generate_city)
            .add_systems(
                OnEnter(AppState::Playing),
                (build_city_rooftop_routes, build_building_cluster_lods)
                    .chain()
                    .run_if(campaign_experience)
                    .after(generate_city),
            )
            .add_systems(OnEnter(AppState::MainMenu), cleanup_world_for_menu)
            .add_systems(
                Update,
                (
                    platformer_party_boundary_and_bubble_system,
                    platformer_checkpoint_and_reward_system,
                    platformer_pressure_gate_system,
                    platformer_encounter_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    update_day_night,
                    animate_nature,
                    guardian_ship_patrol_system,
                    mountain_spider_mech_movement_system,
                    animate_mountain_spider_legs,
                    city_spy_drone_patrol_system,
                    city_spy_drone_data_drop_system,
                    city_pedestrian_system,
                    discussion_interaction_system,
                    dungeon_key_pickup_system,
                    dungeon_key_gate_system,
                    world_site_enemy_spawner_system,
                    site_liberation_system,
                    world_route_unlock_system,
                    moving_platform_system,
                    rotating_elevator_system,
                    sling_shot_system,
                    laser_turret_system,
                    laser_beam_cleanup_system,
                    collider_debug_toggle_system,
                )
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    flip_platform_system,
                    collapse_platform_system,
                    spike_platform_hazard_system,
                )
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                wasteland_drone_patrol_system.run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    building_reward_bob_system,
                    building_reward_pickup_system,
                    city_rooftop_route_marker_system,
                    city_rooftop_route_challenge_system,
                    city_rooftop_time_trial_system,
                )
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    water_wake_spawn_system,
                    water_wake_update_system,
                    animate_waterfall_flow_system,
                    waterfall_particle_system,
                    waterfall_splash_zone_system,
                )
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    spring_jump_pad_system,
                    board_boost_pad_system,
                    stunt_grind_rail_system,
                    ApplyDeferred,
                    // Tricks resolve before the combo system accrues airtime
                    // and banks the run on landing.
                    hoverboard_trick_system,
                    stunt_combo_system,
                    stunt_race_gate_system,
                )
                    .chain()
                    .after(VehicleSet::State)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    dungeon_crawl_gate_system,
                    dungeon_room_progress_system,
                    dungeon_exit_portal_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    dungeon_enemy_spawner_system,
                    ApplyDeferred,
                    dungeon_encounter_progress_system,
                    dungeon_reward_pickup_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                npc_road_vehicle_system.run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    raid_counteroffensive_start_system,
                    raid_tick_system,
                    raid_spawn_system,
                    raid_combat_resolution_system,
                    raid_cleanup_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    pressure_accumulation_system,
                    coordinated_raid_trigger_system,
                    final_war_phase_announce_system,
                    win_condition_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    settlement_economy_tick_system,
                    settlement_build_terminal_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), cleanup_world);
    }
}

fn campaign_experience(experience: Res<PlayExperience>) -> bool {
    *experience == PlayExperience::Campaign
}

fn reset_platformer_progress(
    transition: Res<PlaySessionTransition>,
    experience: Res<PlayExperience>,
    mut progress: ResMut<PlatformerWorkshopProgress>,
) {
    if !transition.resuming_from_pause && *experience == PlayExperience::SharedPlatformer {
        *progress = PlatformerWorkshopProgress::default();
    }
}

fn update_day_night(
    time: Res<Time>,
    mut cycle: ResMut<DayNightCycle>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sun_q: Query<
        (&mut DirectionalLight, &mut Transform),
        (
            With<DayNightSunLight>,
            Without<DayNightMoonLight>,
            Without<CelestialVisual>,
        ),
    >,
    mut moon_light_q: Query<
        (&mut DirectionalLight, &mut Transform),
        (
            With<DayNightMoonLight>,
            Without<DayNightSunLight>,
            Without<CelestialVisual>,
        ),
    >,
    mut celestial_q: Query<
        (&CelestialVisual, &mut Transform, &mut Visibility),
        (Without<DayNightSunLight>, Without<DayNightMoonLight>),
    >,
    sky_q: Query<&MeshMaterial3d<StandardMaterial>, With<SkyDome>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    cycle.time_of_day =
        (cycle.time_of_day + time.delta_secs() / cycle.seconds_per_day.max(1.0)).fract();

    let phase = cycle.time_of_day;
    let sun_dir = celestial_direction(phase);
    let moon_dir = celestial_direction(phase + 0.50);
    let sun_elevation = sun_dir.y;
    let moon_elevation = moon_dir.y;
    let day = smoothstep(-0.08, 0.28, sun_elevation);
    let moonlight = smoothstep(-0.08, 0.30, moon_elevation) * (1.0 - day * 0.72);
    let sunset = (1.0 - (sun_elevation.abs() / 0.33).clamp(0.0, 1.0))
        * smoothstep(-0.22, 0.20, sun_elevation);
    let night = 1.0 - day;

    let night_sky = Color::srgb(0.015, 0.030, 0.110);
    let dawn_sky = Color::srgb(0.98, 0.34, 0.42);
    let gold_sky = Color::srgb(1.0, 0.58, 0.18);
    let day_sky = Color::srgb(0.22, 0.55, 0.96);
    let mut sky = mix_color(night_sky, day_sky, day);
    sky = mix_color(sky, dawn_sky, sunset * 0.62);
    sky = mix_color(sky, gold_sky, sunset * 0.30);
    clear.0 = sky;

    ambient.color = mix_color(
        Color::srgb(0.18, 0.22, 0.42),
        Color::srgb(0.72, 0.78, 0.92),
        day,
    );
    ambient.brightness = 55.0 + day * 245.0 + sunset * 105.0 + moonlight * 80.0;

    if let Ok((mut sun, mut transform)) = sun_q.single_mut() {
        sun.color = mix_color(
            Color::srgb(1.0, 0.42, 0.18),
            Color::srgb(1.0, 0.96, 0.82),
            day,
        );
        sun.illuminance = (2_000.0 + day * 34_000.0 + sunset * 12_000.0) * day.max(0.08);
        *transform = Transform::from_translation(sun_dir * 6_000.0).looking_at(Vec3::ZERO, Vec3::Y);
    }

    if let Ok((mut moon, mut transform)) = moon_light_q.single_mut() {
        moon.color = Color::srgb(0.56, 0.70, 1.0);
        moon.illuminance = 1_500.0 + moonlight * 13_500.0;
        *transform =
            Transform::from_translation(moon_dir * 6_000.0).looking_at(Vec3::ZERO, Vec3::Y);
    }

    for (visual, mut transform, mut visibility) in celestial_q.iter_mut() {
        let dir = celestial_direction(phase + visual.phase_offset);
        let elevation = dir.y;
        transform.translation = dir * visual.distance;
        transform.look_at(Vec3::ZERO, Vec3::Y);
        *visibility = match visual.kind {
            CelestialKind::Sun => {
                if elevation > -0.10 {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                }
            }
            CelestialKind::Moon => {
                if elevation > -0.12 && night > 0.18 {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                }
            }
        };
    }

    for sky_mat in sky_q.iter() {
        if let Some(mut material) = materials.get_mut(&sky_mat.0) {
            material.base_color = sky;
            let lin = sky.to_linear();
            material.emissive = LinearRgba::new(
                lin.red * (0.18 + night * 0.18),
                lin.green * (0.18 + night * 0.18),
                lin.blue * (0.22 + night * 0.38),
                1.0,
            );
        }
    }
}

fn celestial_direction(phase: f32) -> Vec3 {
    let angle = TAU * (phase.fract() - 0.25);
    Vec3::new(angle.cos() * 0.38, angle.sin(), angle.cos() * 0.92).normalize()
}

fn mix_color(a: Color, b: Color, t: f32) -> Color {
    let a = a.to_linear();
    let b = b.to_linear();
    let t = t.clamp(0.0, 1.0);
    Color::linear_rgb(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
    )
}

fn animate_nature(time: Res<Time>, mut q: Query<(&NatureSway, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (sway, mut transform) in q.iter_mut() {
        let wave = (t * sway.speed + sway.phase).sin();
        let cross = (t * sway.speed * 0.73 + sway.phase * 1.31).cos();
        transform.translation = sway.base_translation + Vec3::Y * wave * sway.bob;
        transform.rotation = sway.base_rotation
            * Quat::from_rotation_z(wave * sway.sway)
            * Quat::from_rotation_x(cross * sway.sway * 0.35);
        transform.scale = sway.base_scale
            * Vec3::new(
                1.0 + cross * sway.pulse * 0.35,
                1.0 + wave * sway.pulse,
                1.0 - cross * sway.pulse * 0.25,
            );
    }
}

fn guardian_ship_patrol_system(
    time: Res<Time>,
    mut ship_q: Query<(&FreePeopleGuardianShip, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    for (ship, mut transform) in ship_q.iter_mut() {
        let angle = ship.phase + t * ship.angular_speed;
        let bob = (t * 0.85 + ship.phase).sin() * ship.bob;
        transform.translation = Vec3::new(
            ship.center.x + angle.cos() * ship.radius,
            ship.center.y + ship.altitude + bob,
            ship.center.z + angle.sin() * ship.radius,
        );
        transform.rotation = Quat::from_rotation_y(-angle + std::f32::consts::FRAC_PI_2)
            * Quat::from_rotation_z((t * 1.4 + ship.phase).sin() * 0.04);
    }
}

fn city_spy_drone_patrol_system(
    time: Res<Time>,
    mut spy_q: Query<(&CitySpyDrone, &Health, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    for (spy, health, mut transform) in spy_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        let angle = spy.phase + t * spy.angular_speed;
        let jitter = (t * 1.7 + spy.phase * 3.1).sin() * 0.16;
        transform.translation = Vec3::new(
            spy.home.x + angle.cos() * spy.radius,
            spy.home.y + spy.altitude + (t * 2.1 + spy.phase).sin() * spy.bob,
            spy.home.z + angle.sin() * spy.radius,
        );
        transform.rotation = Quat::from_rotation_y(-angle + std::f32::consts::FRAC_PI_2 + jitter)
            * Quat::from_rotation_x((t * 2.4 + spy.phase).cos() * 0.06);
    }
}

fn city_spy_drone_data_drop_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    settings: Res<GameSettings>,
    mut spy_q: Query<(&Transform, &mut CitySpyDrone, &Health)>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (transform, mut spy, health) in spy_q.iter_mut() {
        if health.is_alive() || spy.data_spawned {
            continue;
        }

        spy.data_spawned = true;
        let ground_y = terrain_surface_y(
            transform.translation.x,
            transform.translation.z,
            settings.world_seed,
        );
        let drop_pos = transform.translation.with_y(ground_y + 2.5);
        spawn_discoverable_beacon(
            &mut commands,
            &mut meshes,
            &mut materials,
            DiscoverableKind::SpyData {
                data_id: spy.reward_id,
                credits: spy.credits,
                experience: spy.experience,
                armor: spy.armor,
            },
            spy.label,
            drop_pos,
        );
        msg_ev.write(UiMessageEvent {
            text: format!("Spy drone down: retrieve {}", spy.label),
            duration: 3.0,
        });
    }
}

fn settlement_economy_tick_system(time: Res<Time>, mut economy: ResMut<SettlementEconomy>) {
    economy.tick(time.delta_secs());
}

fn settlement_build_terminal_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    settings: Res<GameSettings>,
    progress: Res<ChapterProgress>,
    world_site_registry: Res<WorldSiteRegistry>,
    mut economy: ResMut<SettlementEconomy>,
    mut robot_pets: ResMut<RobotPetCollection>,
    terminal_q: Query<(&Transform, &SettlementBuildTerminal)>,
    player_q: Query<(&PlayerIndex, &Transform, &PlayerInput), With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (player_index, player_xf, input) in player_q.iter() {
        if !input.interact {
            continue;
        }

        let mut nearest: Option<(f32, &Transform, &SettlementBuildTerminal)> = None;
        for (terminal_xf, terminal) in terminal_q.iter() {
            let distance = player_xf.translation.distance(terminal_xf.translation);
            if distance <= terminal.interact_radius
                && nearest
                    .map(|(nearest_distance, _, _)| distance < nearest_distance)
                    .unwrap_or(true)
            {
                nearest = Some((distance, terminal_xf, terminal));
            }
        }

        let Some((_, _terminal_xf, terminal)) = nearest else {
            continue;
        };

        if !settlement_terminal_is_buildable(terminal, &progress, &world_site_registry) {
            msg_ev.write(UiMessageEvent {
                text: format!(
                    "P{}: {} needs its cache recovered or site liberated before building.",
                    player_index.0 + 1,
                    terminal.settlement_name
                ),
                duration: 3.8,
            });
            return;
        }

        let build_kind = economy.next_recommended_build(terminal.settlement_id, terminal.kind);
        let def = settlement_build_def(build_kind);
        match economy.try_build(terminal.settlement_id, build_kind, &mut robot_pets) {
            Ok(tier) => {
                let pal = Palette::build(&mut materials, &asset_server);
                let settlement = MapSettlement {
                    anchor_id: terminal.settlement_id,
                    name: terminal.settlement_name,
                    region: "",
                    kind: terminal.kind,
                    reward_id: terminal.reward_id,
                    x: terminal.origin.x,
                    z: terminal.origin.z,
                    facing_yaw: terminal.facing_yaw,
                    site_id: None,
                };
                let layout = settlement_layout_profile(
                    settings.world_seed,
                    settlement,
                    settlement_plaza_radius(terminal.kind),
                );
                let rot = Quat::from_rotation_y(terminal.facing_yaw);
                let base = settlement_build_module_base(
                    terminal.origin,
                    rot,
                    layout,
                    settings.world_seed,
                    build_kind,
                );
                spawn_settlement_build_module(
                    &mut commands,
                    &mut meshes,
                    &pal,
                    base,
                    rot,
                    build_kind,
                    tier,
                );
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "P{} built {} tier {} at {}. {}",
                        player_index.0 + 1,
                        def.label,
                        tier,
                        terminal.settlement_name,
                        economy.status_line(terminal.settlement_id)
                    ),
                    duration: 5.5,
                });
            }
            Err(error) => {
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "Cannot build {} at {}: {}",
                        def.label,
                        terminal.settlement_name,
                        error.message()
                    ),
                    duration: 4.5,
                });
            }
        }
        return;
    }
}

fn settlement_terminal_is_buildable(
    terminal: &SettlementBuildTerminal,
    progress: &ChapterProgress,
    world_site_registry: &WorldSiteRegistry,
) -> bool {
    progress.has_discoverable(terminal.reward_id)
        || world_site_registry
            .sites
            .iter()
            .any(|site| site.name == terminal.settlement_name && site.is_liberated())
}

fn discussion_interaction_system(
    mut discussion: ResMut<DiscussionState>,
    player_q: Query<(&PlayerIndex, &Transform, &PlayerInput), With<Player>>,
    npc_q: Query<(&Transform, &DiscussionNpc)>,
    gate_q: Query<&DungeonCrawlGate>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if discussion.active {
        return;
    }

    for (player_index, player_transform, input) in player_q.iter() {
        if !input.interact {
            continue;
        }
        // Dungeon entrances own the shared interact button inside their
        // authored radius. This prevents one press from opening a gate and
        // starting nearby NPC dialogue on the same frame.
        if gate_q
            .iter()
            .any(|gate| gate.contains_interaction(player_transform.translation))
        {
            continue;
        }

        let mut nearest: Option<(f32, &DiscussionNpc)> = None;
        for (npc_transform, npc) in npc_q.iter() {
            let distance = player_transform
                .translation
                .distance(npc_transform.translation);
            if distance <= npc.interact_radius
                && nearest
                    .map(|(nearest_distance, _)| distance < nearest_distance)
                    .unwrap_or(true)
            {
                nearest = Some((distance, npc));
            }
        }

        let Some((_, npc)) = nearest else {
            continue;
        };

        if discussion.start(npc.script_id, player_index.0) {
            let title = discussion
                .script()
                .map(|script| script.title)
                .unwrap_or(npc.role);
            msg_ev.write(UiMessageEvent {
                text: format!(
                    "P{} talking with {} - {}",
                    player_index.0 + 1,
                    npc.display_name,
                    title
                ),
                duration: 2.0,
            });
        }
        return;
    }
}

fn dungeon_crawl_gate_system(
    mut commands: Commands,
    time: Res<Time>,
    mut dungeon: ResMut<DungeonCrawlState>,
    mut room_state: ResMut<DungeonRoomState>,
    mut gate_q: Query<(&mut DungeonCrawlGate, Option<&ArcadeDungeonPortal>)>,
    mut door_q: Query<(&mut Transform, &DungeonGateDoor), Without<Player>>,
    mut player_q: Query<
        (
            Entity,
            &PlayerIndex,
            &mut Transform,
            &PlayerInput,
            &mut PlayerMovement,
            Option<&mut JetpackState>,
            Option<&mut GrappleHookState>,
            Option<&mut TraversalModeState>,
            Option<&mut ClimbState>,
            Option<&mut EdgeGrabState>,
            Option<&mut BoardBoostState>,
        ),
        With<Player>,
    >,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    let mut opened_gates = Vec::new();
    // Snapshot interact intents before arcade teleports rewrite transforms.
    let interact_positions = player_q
        .iter()
        .filter(|(_, _, _, input, ..)| input.interact)
        .map(|(_, _, transform, ..)| transform.translation)
        .collect::<Vec<_>>();
    let near_positions = player_q
        .iter()
        .map(|(_, _, transform, ..)| transform.translation)
        .collect::<Vec<_>>();

    let mut arcade_enter: Option<(
        &'static str,
        u8,
        &'static str,
        Vec3,
        f32,
        ArcadeDungeonPortal,
    )> = None;

    for (mut gate, arcade_portal) in gate_q.iter_mut() {
        let party_near_gate = near_positions
            .iter()
            .any(|position| gate.contains_interaction(*position));
        let should_open = party_near_gate
            && interact_positions
                .iter()
                .any(|position| gate.contains_interaction(*position));

        if should_open {
            let entering_fresh =
                !gate.opened || !dungeon.active || dungeon.gate_id != Some(gate.gate_id);
            gate.opened = true;
            if let Some(portal) = arcade_portal {
                if entering_fresh || !dungeon.arcade_rules {
                    arcade_enter = Some((
                        gate.gate_id,
                        gate.chapter,
                        gate.label,
                        gate.focus,
                        gate.radius,
                        *portal,
                    ));
                }
            } else {
                if entering_fresh {
                    msg_ev.write(UiMessageEvent {
                        text: format!("{} opened - top-down dungeon crawl linked.", gate.label),
                        duration: 3.0,
                    });
                }
                dungeon.activate(
                    gate.gate_id,
                    crate::chapters::ChapterId(gate.chapter),
                    gate.label,
                    gate.focus,
                    gate.entry,
                    gate.radius,
                );
            }
        }

        if gate.opened {
            opened_gates.push(gate.gate_id);
        }
    }

    if let Some((gate_id, chapter, label, focus, radius, portal)) = arcade_enter {
        for (
            entity,
            index,
            mut transform,
            _,
            mut movement,
            jetpack,
            grapple,
            traversal,
            climb,
            edge_grab,
            board_boost,
        ) in player_q.iter_mut()
        {
            transform.translation = portal.stage_spawns[usize::from(index.0.min(3))];
            strip_player_for_arcade_stage(
                &mut movement,
                jetpack,
                grapple,
                traversal,
                climb,
                edge_grab,
                board_boost,
            );
            commands.entity(entity).remove::<BoatPassenger>();
        }
        dungeon.activate_arcade(
            gate_id,
            crate::chapters::ChapterId(chapter),
            label,
            focus,
            portal.stage_spawns[0],
            radius.max(28.0),
        );
        room_state.clear_active();
        room_state.enter_room(gate_id, 0);
        msg_ev.write(UiMessageEvent {
            text: format!(
                "{label} — arcade stage linked! Hack-and-slash only (no jet/grapple/board)."
            ),
            duration: 3.4,
        });
    }

    for (mut transform, door) in door_q.iter_mut() {
        let target = if opened_gates.contains(&door.gate_id) {
            door.open
        } else {
            door.closed
        };
        let blend = 1.0 - (-dt * 5.5).exp();
        transform.translation = transform.translation.lerp(target, blend);
    }
}

fn strip_player_for_arcade_stage(
    movement: &mut PlayerMovement,
    jetpack: Option<Mut<JetpackState>>,
    grapple: Option<Mut<GrappleHookState>>,
    traversal: Option<Mut<TraversalModeState>>,
    climb: Option<Mut<ClimbState>>,
    edge_grab: Option<Mut<EdgeGrabState>>,
    board_boost: Option<Mut<BoardBoostState>>,
) {
    movement.velocity = Vec3::ZERO;
    movement.ground_velocity = Vec3::ZERO;
    movement.knockback_velocity = Vec3::ZERO;
    movement.clear_motor_delivery();
    movement.is_grounded = false;
    if let Some(mut jetpack) = jetpack {
        jetpack.is_active = false;
        jetpack.mode = FlightMode::Grounded;
        jetpack.air_dash_timer = 0.0;
    }
    if let Some(mut grapple) = grapple {
        grapple.begin_recovery();
    }
    if let Some(mut traversal) = traversal {
        // Keep a grounded-friendly mode selected; arcade rules block the toys.
        traversal.active = TraversalMode::Grapple;
    }
    if let Some(mut climb) = climb {
        climb.is_climbing = false;
    }
    if let Some(mut edge_grab) = edge_grab {
        edge_grab.release_hang();
    }
    if let Some(mut board_boost) = board_boost {
        board_boost.timer = 0.0;
        board_boost.speed_mult = 1.0;
        board_boost.direction = Vec3::ZERO;
        board_boost.manual_cooldown = 0.0;
    }
}

fn dungeon_room_progress_system(
    mut dungeon: ResMut<DungeonCrawlState>,
    mut room_state: ResMut<DungeonRoomState>,
    room_q: Query<&DungeonRoomZone>,
    portal_q: Query<&DungeonRoomPortal>,
    player_q: Query<&Transform, With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let Some(gate_id) = dungeon.gate_id.filter(|_| dungeon.active) else {
        room_state.clear_active();
        return;
    };
    let player_count = player_q.iter().count();
    if player_count == 0 {
        return;
    }
    let party_focus = player_q
        .iter()
        .map(|transform| transform.translation)
        .sum::<Vec3>()
        / player_count as f32;
    let rooms = room_q
        .iter()
        .filter(|room| room.gate_id == gate_id)
        .collect::<Vec<_>>();
    if rooms.is_empty() {
        room_state.clear_active();
        return;
    }
    let distance_to_party =
        |room: &DungeonRoomZone| (room.focus - party_focus).with_y(0.0).length_squared();
    let Some(nearest) = rooms
        .iter()
        .copied()
        .min_by(|a, b| distance_to_party(a).total_cmp(&distance_to_party(b)))
    else {
        room_state.clear_active();
        return;
    };

    let target = if room_state.active_gate_id == Some(gate_id) {
        room_state
            .active_room
            .and_then(|active| rooms.iter().copied().find(|room| room.room_index == active))
            .map(|current| {
                let current_distance = distance_to_party(current).sqrt();
                let nearest_distance = distance_to_party(nearest).sqrt();
                let adjacent = portal_q.iter().any(|portal| {
                    portal.gate_id == gate_id
                        && portal.connects(current.room_index, nearest.room_index)
                });
                if nearest.room_index != current.room_index
                    && adjacent
                    && nearest_distance + 2.5 < current_distance
                {
                    nearest
                } else {
                    current
                }
            })
            .unwrap_or(nearest)
    } else {
        nearest
    };

    if room_state.enter_room(gate_id, target.room_index) {
        dungeon.focus = target.focus;
        dungeon.radius = target.camera_radius.max(28.0);
        msg_ev.write(UiMessageEvent {
            text: format!("{} — {}", dungeon.label, target.label),
            duration: 1.8,
        });
    }
}

fn dungeon_exit_portal_system(
    mut commands: Commands,
    mut dungeon: ResMut<DungeonCrawlState>,
    mut room_state: ResMut<DungeonRoomState>,
    portal_q: Query<&DungeonExitPortal>,
    mut player_q: Query<
        (
            Entity,
            &PlayerIndex,
            &PlayerInput,
            &mut Transform,
            &mut PlayerMovement,
            Option<&mut JetpackState>,
            Option<&mut GrappleHookState>,
            Option<&mut ClimbState>,
            Option<&mut EdgeGrabState>,
            Option<&mut BoardBoostState>,
        ),
        With<Player>,
    >,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let Some(active_gate_id) = dungeon.gate_id.filter(|_| dungeon.active) else {
        return;
    };
    if !dungeon.exit_armed {
        dungeon.exit_armed = true;
        return;
    }
    let Some(portal) = portal_q
        .iter()
        .find(|portal| portal.gate_id == active_gate_id)
    else {
        return;
    };
    let exit_requested = player_q.iter().any(|(_, _, input, transform, ..)| {
        input.interact && transform.translation.distance(portal.position) <= portal.interact_radius
    });
    if !exit_requested {
        return;
    }

    let was_arcade = dungeon.arcade_rules;
    for (
        entity,
        index,
        _,
        mut transform,
        mut movement,
        jetpack,
        grapple,
        climb,
        edge_grab,
        board_boost,
    ) in player_q.iter_mut()
    {
        transform.translation = portal.return_positions[usize::from(index.0.min(3))];
        movement.velocity = Vec3::ZERO;
        movement.ground_velocity = Vec3::ZERO;
        movement.knockback_velocity = Vec3::ZERO;
        movement.clear_motor_delivery();
        movement.is_grounded = false;
        if let Some(mut jetpack) = jetpack {
            jetpack.is_active = false;
            jetpack.mode = FlightMode::Grounded;
            jetpack.air_dash_timer = 0.0;
        }
        if let Some(mut grapple) = grapple {
            grapple.begin_recovery();
        }
        if let Some(mut climb) = climb {
            climb.is_climbing = false;
        }
        if let Some(mut edge_grab) = edge_grab {
            edge_grab.release_hang();
        }
        if let Some(mut board_boost) = board_boost {
            board_boost.timer = 0.0;
            board_boost.speed_mult = 1.0;
            board_boost.direction = Vec3::ZERO;
        }
        commands.entity(entity).remove::<BoatPassenger>();
    }
    dungeon.clear();
    room_state.clear_active();
    msg_ev.write(UiMessageEvent {
        text: if was_arcade {
            "Party left the arcade stage.".to_string()
        } else {
            "Party returned to the mountain entrance.".to_string()
        },
        duration: 2.4,
    });
}

fn dungeon_return_positions(center: Vec3, right: Vec3, forward: Vec3) -> [Vec3; 4] {
    [
        center - right * 1.8 - forward * 1.2,
        center + right * 1.8 - forward * 1.2,
        center - right * 1.8 + forward * 1.2,
        center + right * 1.8 + forward * 1.2,
    ]
}

fn dungeon_key_pickup_system(
    mut commands: Commands,
    mut room_state: ResMut<DungeonRoomState>,
    mut key_q: Query<(Entity, &mut DungeonKeyPickup, &Transform)>,
    player_q: Query<&Transform, With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (entity, mut key, key_xf) in key_q.iter_mut() {
        if key.collected {
            continue;
        }
        let any_near = player_q
            .iter()
            .any(|p| p.translation.distance(key_xf.translation) <= key.pickup_radius);
        if any_near {
            key.collected = true;
            room_state.collect_key(key.chapter);
            commands.entity(entity).despawn();
            msg_ev.write(UiMessageEvent {
                text: "Crown Key collected! Seek the boss gate.".to_string(),
                duration: 3.5,
            });
        }
    }
}

fn dungeon_key_gate_system(
    room_state: Res<DungeonRoomState>,
    mut gate_q: Query<(&mut Transform, &DungeonKeyGate)>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (mut xf, gate) in gate_q.iter_mut() {
        let target = if room_state.has_key(gate.chapter) {
            gate.open
        } else {
            gate.closed
        };
        let blend = 1.0 - (-dt * 4.0).exp();
        xf.translation = xf.translation.lerp(target, blend);
    }
}

fn dungeon_enemy_spawner_system(
    dungeon: Res<DungeonCrawlState>,
    mut spawner_q: Query<(
        &Transform,
        &mut DungeonEnemySpawner,
        Option<&CreatureSpawnOverride>,
    )>,
    encounter_enemy_q: Query<&DungeonEncounterEnemy>,
    player_q: Query<&Transform, With<Player>>,
    creature_catalog: Res<crate::engine_tools::PublishedCreatureCatalog>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if !dungeon.active {
        return;
    }
    let dungeon_faction = dungeon.gate_id.and_then(|gate_id| {
        if !gate_id.starts_with("dragon_dungeon_") {
            return None;
        }
        Some(if dungeon.chapter.is_some_and(|chapter| chapter.0 >= 10) {
            Faction::DragonExile
        } else {
            Faction::DragonRoyalty
        })
    });
    let wave_states = spawner_q
        .iter_mut()
        .map(|(_, spawner, _)| (spawner.encounter, spawner.wave_index, spawner.spawned))
        .collect::<Vec<_>>();
    for (spawner_xf, mut spawner, creature_override) in spawner_q.iter_mut() {
        if spawner.spawned {
            continue;
        }
        if let Some(encounter) = spawner.encounter.filter(|_| spawner.wave_index > 0) {
            let live_enemies = encounter_enemy_q.iter().copied().collect::<Vec<_>>();
            if !ordered_dungeon_wave_ready(
                encounter,
                spawner.wave_index,
                &wave_states,
                &live_enemies,
            ) {
                continue;
            }
        }
        let any_near = player_q
            .iter()
            .any(|p| p.translation.distance(spawner_xf.translation) <= spawner.trigger_radius);
        if !any_near {
            continue;
        }
        spawner.spawned = true;
        let spread = spawner.count as f32 * 2.0;
        for i in 0..spawner.count {
            let offset = Vec3::new(
                (i as f32 - spawner.count as f32 * 0.5) * (spread / spawner.count as f32),
                0.0,
                0.0,
            );
            // A published Forge creature overrides the built-in enemy type;
            // unpublished ids fall back so encounters never spawn empty.
            let published_creature =
                creature_override.and_then(|over| creature_catalog.get(&over.0));
            let enemy = match published_creature {
                Some(spec) => crate::plugins::enemy_plugin::spawn_published_creature_enemy(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    spec,
                    spawner_xf.translation + offset,
                    spawner.difficulty,
                    None,
                ),
                None => spawn_enemy_entity(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    spawner.enemy_type,
                    spawner_xf.translation + offset,
                    spawner.difficulty,
                    dungeon_faction,
                ),
            };
            if let Some((gate_id, room_index)) = spawner.encounter {
                commands.entity(enemy).insert(DungeonEncounterEnemy {
                    gate_id,
                    room_index,
                    wave_index: spawner.wave_index,
                });
            }
        }
        msg_ev.write(UiMessageEvent {
            text: if spawner.encounter.is_some() {
                format!(
                    "Encounter wave {} — enemies appear!",
                    spawner.wave_index + 1
                )
            } else {
                "Enemies appear!".to_string()
            },
            duration: 1.8,
        });
    }
}

fn ordered_dungeon_wave_ready(
    encounter: (&'static str, u8),
    wave_index: u8,
    wave_states: &[(Option<(&'static str, u8)>, u8, bool)],
    live_enemies: &[DungeonEncounterEnemy],
) -> bool {
    if wave_index == 0 {
        return true;
    }
    let has_previous_wave = wave_states
        .iter()
        .any(|(other, wave, _)| *other == Some(encounter) && *wave < wave_index);
    let previous_waves_spawned = wave_states
        .iter()
        .all(|(other, wave, spawned)| *other != Some(encounter) || *wave >= wave_index || *spawned);
    let previous_enemy_alive = live_enemies.iter().any(|enemy| {
        (enemy.gate_id, enemy.room_index) == encounter && enemy.wave_index < wave_index
    });
    has_previous_wave && previous_waves_spawned && !previous_enemy_alive
}

fn dungeon_encounter_door_should_close(
    prerequisite_met: bool,
    encounter_started: bool,
    encounter_cleared: bool,
) -> bool {
    !prerequisite_met || encounter_started && !encounter_cleared
}

fn dungeon_encounter_progress_system(
    time: Res<Time>,
    dungeon: Res<DungeonCrawlState>,
    mut room_state: ResMut<DungeonRoomState>,
    spawner_q: Query<&DungeonEnemySpawner>,
    enemy_q: Query<&DungeonEncounterEnemy>,
    mut door_q: Query<(&mut Transform, &DungeonEncounterDoor)>,
    mut reward_q: Query<(&DungeonEncounterReward, &mut Visibility)>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let active_gate = dungeon.gate_id.filter(|_| dungeon.active);
    let dt = time.delta_secs();

    for (mut transform, door) in door_q.iter_mut() {
        let encounter_spawners = spawner_q
            .iter()
            .filter(|spawner| spawner.encounter == Some((door.gate_id, door.room_index)))
            .collect::<Vec<_>>();
        let started = active_gate == Some(door.gate_id)
            && encounter_spawners.iter().any(|spawner| spawner.spawned);
        let all_waves_spawned = !encounter_spawners.is_empty()
            && encounter_spawners.iter().all(|spawner| spawner.spawned);
        let enemies_alive = enemy_q
            .iter()
            .any(|enemy| enemy.gate_id == door.gate_id && enemy.room_index == door.room_index);
        let was_cleared = room_state
            .cleared_rooms
            .contains(&(door.gate_id, door.room_index));
        if started && all_waves_spawned && !enemies_alive && !was_cleared {
            room_state.mark_cleared(door.gate_id, door.room_index);
            msg_ev.write(UiMessageEvent {
                text: "Star Chamber cleared — ancient seals released and reward awakened!"
                    .to_string(),
                duration: 3.5,
            });
        }
        let cleared = was_cleared
            || room_state
                .cleared_rooms
                .contains(&(door.gate_id, door.room_index));
        let prerequisite_met = door
            .requires_room_clear
            .is_none_or(|required| room_state.cleared_rooms.contains(&(door.gate_id, required)));
        let target = if dungeon_encounter_door_should_close(prerequisite_met, started, cleared) {
            door.closed
        } else {
            door.open
        };
        let blend = 1.0 - (-dt * 5.0).exp();
        transform.translation = transform.translation.lerp(target, blend);
    }

    for (reward, mut visibility) in reward_q.iter_mut() {
        *visibility = if !reward.claimed
            && room_state
                .cleared_rooms
                .contains(&(reward.gate_id, reward.room_index))
        {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn dungeon_reward_pickup_system(
    dungeon: Res<DungeonCrawlState>,
    room_state: Res<DungeonRoomState>,
    mut reward_q: Query<(&Transform, &mut DungeonEncounterReward, &mut Visibility)>,
    mut player_q: Query<(&Transform, &mut Health, &mut PlayerStats), With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let Some(active_gate) = dungeon.gate_id.filter(|_| dungeon.active) else {
        return;
    };
    for (reward_transform, mut reward, mut visibility) in reward_q.iter_mut() {
        if reward.claimed
            || reward.gate_id != active_gate
            || !room_state
                .cleared_rooms
                .contains(&(reward.gate_id, reward.room_index))
        {
            continue;
        }
        let claimed = player_q.iter_mut().any(|(transform, _, _)| {
            transform.translation.distance(reward_transform.translation) <= 3.2
        });
        if !claimed {
            continue;
        }
        reward.claimed = true;
        *visibility = Visibility::Hidden;
        for (_, mut health, mut stats) in player_q.iter_mut() {
            health.heal(reward.healing);
            stats.credits = stats.credits.saturating_add(reward.credits);
        }
        msg_ev.write(UiMessageEvent {
            text: format!(
                "Relic cache claimed — party restored, +{} credits each!",
                reward.credits
            ),
            duration: 3.0,
        });
    }
}

// ── World Site Systems ────────────────────────────────────────────────────────

/// Initialise the registry from static data. Skips if already populated (e.g.
/// save plugin already applied records before Playing is entered).
fn setup_world_site_registry(registry: &mut WorldSiteRegistry) {
    if !registry.sites.is_empty() {
        return;
    }
    registry.sites = initial_world_sites();
}

/// Spawn visual markers and enemy sentinels for all registered world sites.
fn spawn_world_site_props(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    registry: &WorldSiteRegistry,
    seed: u64,
) {
    use crate::components::world::WorldSiteEnemySentinel;

    for site in &registry.sites {
        let ground_y = terrain_surface_y(site.world_x, site.world_z, seed);
        let center = Vec3::new(site.world_x, ground_y, site.world_z);

        if site.is_liberated() {
            spawn_site_terminal(commands, meshes, materials, site.id, center);
        } else {
            // Red watchtower pillar — visible marker for an enemy-held site.
            let tower_mat = materials.add(StandardMaterial {
                base_color: Color::srgb(0.65, 0.12, 0.08),
                perceptual_roughness: 0.85,
                ..Default::default()
            });
            commands.spawn((
                Mesh3d(meshes.add(Mesh::from(Cuboid::new(2.0, 6.0, 2.0)))),
                MeshMaterial3d(tower_mat),
                Transform::from_translation(center + Vec3::new(0.0, 3.0, 0.0)),
                WorldGeometry,
            ));

            commands.spawn((
                WorldSiteEnemySentinel {
                    site_id: site.id,
                    trigger_radius: 100.0,
                    spawned: false,
                    enemy_count: site.enemy_count_to_liberate,
                    liberated_spawned: false,
                },
                Transform::from_translation(center),
                GlobalTransform::default(),
                WorldGeometry,
            ));
        }
    }
}

fn setup_world_route_registry(registry: &mut WorldRouteRegistry) {
    if !registry.routes.is_empty() {
        return;
    }
    registry.routes = initial_world_routes();
}

fn spawn_wasteland_drone_patrol_anchors(commands: &mut Commands, seed: u64) {
    for &xz in WASTELAND_DRONE_PATROLS {
        debug_assert!(xz.length() > STARTING_REGION_SAFE_RADIUS);
        let y = terrain_surface_y(xz.x, xz.y, seed);
        commands.spawn((
            WastelandDronePatrol {
                spawned: false,
                trigger_radius: 260.0,
            },
            Transform::from_xyz(xz.x, y, xz.y),
            GlobalTransform::default(),
            WorldGeometry,
            Name::new("Wasteland Drone Patrol Anchor"),
        ));
    }
}

fn wasteland_drone_patrol_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut patrol_q: Query<(&Transform, &mut WastelandDronePatrol)>,
    player_q: Query<&Transform, With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    const FORMATION: [DroneVariant; 6] = [
        DroneVariant::Scout,
        DroneVariant::HeavyFighter,
        DroneVariant::Scout,
        DroneVariant::HeavyFighter,
        DroneVariant::Scout,
        DroneVariant::SiegeGunship,
    ];

    for (anchor, mut patrol) in &mut patrol_q {
        if patrol.spawned
            || !player_q.iter().any(|player| {
                player.translation.distance(anchor.translation) <= patrol.trigger_radius
            })
        {
            continue;
        }
        patrol.spawned = true;
        for (index, variant) in FORMATION.into_iter().enumerate() {
            let angle = index as f32 / FORMATION.len() as f32 * TAU;
            let radius = 22.0 + (index % 2) as f32 * 9.0;
            let position =
                anchor.translation + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
            spawn_drone_variant_entity(
                &mut commands,
                &mut meshes,
                &mut materials,
                variant,
                position,
                1.15,
                Some(Faction::DimensionalAlien),
            );
        }
        msg_ev.write(UiMessageEvent {
            text: "WASTELAND AIR PATROL — heavy fighters and siege gunship inbound".into(),
            duration: 4.0,
        });
    }
}

/// Opens routes whose `required_site` has just become Liberated.
fn world_route_unlock_system(
    site_registry: Res<WorldSiteRegistry>,
    mut route_registry: ResMut<WorldRouteRegistry>,
) {
    for route in route_registry.routes.iter_mut() {
        if route.state != WorldRouteState::Locked {
            continue;
        }
        if let Some(site) = site_registry.get(route.required_site) {
            if site.state == WorldSiteState::Liberated {
                route.state = WorldRouteState::Open;
            }
        }
    }
}

/// Spawn site defenders when players enter range. Tags each spawned enemy with
/// `WorldSiteMarker` so the liberation system can track them.
fn world_site_enemy_spawner_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut sentinel_q: Query<(&Transform, &mut WorldSiteEnemySentinel)>,
    player_q: Query<&Transform, With<Player>>,
    registry: Res<WorldSiteRegistry>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    use crate::components::world::WorldSiteMarker;

    for (sentinel_xf, mut sentinel) in sentinel_q.iter_mut() {
        if sentinel.spawned {
            continue;
        }
        if registry
            .get(sentinel.site_id)
            .map(|s| s.is_liberated())
            .unwrap_or(false)
        {
            continue;
        }
        let any_near = player_q
            .iter()
            .any(|p| p.translation.distance(sentinel_xf.translation) <= sentinel.trigger_radius);
        if !any_near {
            continue;
        }
        sentinel.spawned = true;
        let count = sentinel.enemy_count;
        let spread = count as f32 * 4.5;
        let site_name = registry
            .get(sentinel.site_id)
            .map(|s| s.name)
            .unwrap_or("Unknown");
        for i in 0..count {
            let t = if count > 1 {
                i as f32 / (count - 1) as f32
            } else {
                0.5
            };
            let offset = Vec3::new((t - 0.5) * spread, 0.0, (i as f32 % 2.0) * 3.0 - 1.5);
            let enemy = spawn_enemy_entity(
                &mut commands,
                &mut meshes,
                &mut materials,
                crate::components::enemy::EnemyType::Soldier,
                sentinel_xf.translation + offset,
                1.0,
                None,
            );
            commands.entity(enemy).insert(WorldSiteMarker {
                id: sentinel.site_id,
            });
        }
        msg_ev.write(UiMessageEvent {
            text: format!(
                "Enemy patrol at {}! Defeat them to liberate the site.",
                site_name
            ),
            duration: 3.5,
        });
    }
}

/// Detects when all site defenders are dead. Flips the registry state to
/// Liberated and spawns the command terminal + friendly NPC.
fn site_liberation_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut sentinel_q: Query<(&Transform, &mut WorldSiteEnemySentinel)>,
    enemy_q: Query<(&WorldSiteMarker, &Health)>,
    mut registry: ResMut<WorldSiteRegistry>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (sentinel_xf, mut sentinel) in sentinel_q.iter_mut() {
        if !sentinel.spawned || sentinel.liberated_spawned {
            continue;
        }
        if registry
            .get(sentinel.site_id)
            .map(|s| s.is_liberated())
            .unwrap_or(false)
        {
            sentinel.liberated_spawned = true;
            continue;
        }
        let alive = enemy_q
            .iter()
            .filter(|(marker, health)| marker.id == sentinel.site_id && health.is_alive())
            .count();
        if alive > 0 {
            continue;
        }
        if let Some(site) = registry.get_mut(sentinel.site_id) {
            site.enemies_defeated = site.enemy_count_to_liberate;
            site.state = crate::resources::WorldSiteState::Liberated;
            site.owner = crate::resources::WorldSiteOwner::PlayerAlliance;
            let site_name = site.name;
            sentinel.liberated_spawned = true;
            spawn_site_terminal(
                &mut commands,
                &mut meshes,
                &mut materials,
                sentinel.site_id,
                sentinel_xf.translation,
            );
            msg_ev.write(UiMessageEvent {
                text: format!("{} liberated! Command terminal online.", site_name),
                duration: 5.0,
            });
        }
    }
}

/// Spawn the glowing command terminal + a friendly NPC for a liberated site.
fn spawn_site_terminal(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    site_id: crate::resources::WorldSiteId,
    center: Vec3,
) {
    use crate::components::world::{DiscussionNpc, SiteCommandTerminal};

    let terminal_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.05, 0.75, 0.15),
        emissive: LinearRgba::new(0.0, 0.55, 0.05, 1.0),
        perceptual_roughness: 0.4,
        ..Default::default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Cuboid::new(1.0, 1.6, 0.4)))),
        MeshMaterial3d(terminal_mat),
        Transform::from_translation(center + Vec3::new(0.0, 0.8, 2.5)),
        SiteCommandTerminal {
            id: site_id,
            interact_radius: 3.5,
        },
        WorldGeometry,
    ));

    let npc_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.70, 0.95),
        perceptual_roughness: 0.7,
        ..Default::default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.35, 1.0)))),
        MeshMaterial3d(npc_mat),
        Transform::from_translation(center + Vec3::new(1.8, 0.85, 2.5)),
        DiscussionNpc {
            id: "site_commander",
            display_name: "Site Commander",
            role: "Liberation Officer",
            script_id: "site_liberated_greeting",
            interact_radius: 3.0,
        },
        WorldGeometry,
    ));
}

fn raid_counteroffensive_start_system(
    mut raids: ResMut<RaidRegistry>,
    mut sites: ResMut<WorldSiteRegistry>,
    economy: Res<SettlementEconomy>,
    command_registry: Res<CommandRegistry>,
    published_fleet: Option<Res<PublishedFleet>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let defense_score = static_defense_score_for_site(CLOUDRAIL_TUTORIAL_RAID_SITE, &economy)
        + command_registry.defense_score_for_site(CLOUDRAIL_TUTORIAL_RAID_SITE)
        + published_fleet.as_ref().map_or(0, |fleet| {
            fleet.defense_score_for_site(CLOUDRAIL_TUTORIAL_RAID_SITE)
        });
    match raids.try_start_cloudrail_tutorial(&mut sites, defense_score) {
        RaidStartResult::Started(_) => {
            msg_ev.write(UiMessageEvent {
                text: "Cloudrail City under attack! Scallarian drones are inbound.".to_string(),
                duration: 5.0,
            });
        }
        RaidStartResult::AutoResolved(_) => {
            msg_ev.write(UiMessageEvent {
                text: "Cloudrail defenses intercepted a Scallarian drone swarm.".to_string(),
                duration: 4.0,
            });
        }
        RaidStartResult::AlreadyExists | RaidStartResult::TargetNotLiberated => {}
    }
}

fn raid_tick_system(
    time: Res<Time>,
    mut raids: ResMut<RaidRegistry>,
    mut sites: ResMut<WorldSiteRegistry>,
    economy: Res<SettlementEconomy>,
    command_registry: Res<CommandRegistry>,
    published_fleet: Option<Res<PublishedFleet>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let defense_scores: Vec<_> = raids
        .raids
        .iter()
        .filter(|raid| raid.is_unresolved())
        .map(|raid| {
            (
                raid.target_site,
                static_defense_score_for_site(raid.target_site, &economy)
                    + command_registry.defense_score_for_site(raid.target_site)
                    + published_fleet
                        .as_ref()
                        .map_or(0, |fleet| fleet.defense_score_for_site(raid.target_site)),
            )
        })
        .collect();
    let events = raids.tick(time.delta_secs(), &mut sites, &defense_scores);
    for event in events {
        match event {
            RaidTickEvent::WarningExpired(id) => {
                let site_name = raid_site_name(&raids, &sites, id);
                msg_ev.write(UiMessageEvent {
                    text: format!("{site_name}: drone swarm has arrived! Clear the sky."),
                    duration: 4.5,
                });
            }
            RaidTickEvent::AutoDefended(_, site_id) => {
                let site_name = site_name(&sites, site_id);
                msg_ev.write(UiMessageEvent {
                    text: format!("{site_name} shield grid held. Raid repelled."),
                    duration: 4.0,
                });
            }
            RaidTickEvent::Failed(_, site_id) => {
                let site_name = site_name(&sites, site_id);
                msg_ev.write(UiMessageEvent {
                    text: format!("{site_name} damaged. Repair crews need help later."),
                    duration: 5.0,
                });
            }
        }
    }
}

fn raid_wave_enemy_type(kind: RaidKind, wave_index: u8) -> EnemyType {
    match kind {
        RaidKind::DroneSwarm | RaidKind::FighterAttack | RaidKind::GiantUfoSiege => {
            EnemyType::Drone
        }
        // Heavy Water's route blockades mix a siege tank into each four-unit
        // cadence while preserving the existing infantry composition.
        RaidKind::RouteBlockade if wave_index.is_multiple_of(4) => EnemyType::Tank,
        RaidKind::RobotLanding | RaidKind::RouteBlockade => EnemyType::Soldier,
        RaidKind::DragonDomainAssault => EnemyType::SpikeAlien,
    }
}

fn raid_spawn_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut raids: ResMut<RaidRegistry>,
    sites: Res<WorldSiteRegistry>,
    settings: Res<GameSettings>,
    threat_q: Query<&RaidThreatMarker>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let existing_threats: Vec<RaidId> = threat_q.iter().map(|marker| marker.raid_id).collect();
    let requests: Vec<_> = raids
        .raids
        .iter()
        .filter(|raid| {
            raid.phase == crate::world::raids::RaidPhase::Active
                && (!raid.spawned || !existing_threats.contains(&raid.id))
        })
        .map(|raid| (raid.id, raid.target_site, raid.kind, raid.wave_size))
        .collect();

    for (raid_id, site_id, kind, wave_size) in requests {
        let Some(site) = sites.get(site_id) else {
            continue;
        };
        let ground_y = terrain_surface_y(site.world_x, site.world_z, settings.world_seed);
        let center = Vec3::new(site.world_x, ground_y, site.world_z);
        spawn_raid_ufo_marker(
            &mut commands,
            &mut meshes,
            &mut materials,
            raid_id,
            center + Vec3::new(0.0, 34.0, 0.0),
        );

        let radius = 22.0;
        for i in 0..wave_size {
            let enemy_type = raid_wave_enemy_type(kind, i);
            let t = i as f32 / wave_size.max(1) as f32;
            let angle = t * std::f32::consts::TAU;
            let x = center.x + angle.cos() * radius;
            let z = center.z + angle.sin() * radius;
            let position = match enemy_type {
                // The tank prefab owns its chassis clearance; give it the
                // exact local surface instead of the airborne raid offset.
                EnemyType::Tank => Vec3::new(x, terrain_surface_y(x, z, settings.world_seed), z),
                _ => Vec3::new(x, center.y + 8.0 + (i % 3) as f32 * 2.2, z),
            };
            let enemy = spawn_enemy_entity(
                &mut commands,
                &mut meshes,
                &mut materials,
                enemy_type,
                position,
                1.12,
                Some(Faction::DimensionalAlien),
            );
            commands.entity(enemy).insert(RaidThreatMarker { raid_id });
        }
        raids.mark_spawned(raid_id);
        msg_ev.write(UiMessageEvent {
            text: format!("{} raid force deployed over {}.", kind.label(), site.name),
            duration: 3.5,
        });
    }
}

fn spawn_raid_ufo_marker(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    raid_id: RaidId,
    position: Vec3,
) {
    let hull = materials.add(StandardMaterial {
        base_color: Color::srgb(0.10, 0.12, 0.16),
        emissive: LinearRgba::new(0.08, 0.18, 0.22, 1.0),
        perceptual_roughness: 0.38,
        ..Default::default()
    });
    let glow = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.95, 1.0),
        emissive: LinearRgba::new(0.0, 0.70, 0.95, 1.0),
        ..Default::default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Cylinder::new(13.0, 1.2)))),
        MeshMaterial3d(hull),
        Transform::from_translation(position).with_scale(Vec3::new(1.45, 1.0, 0.48)),
        RaidUfoMarker { raid_id },
        WorldGeometry,
    ));
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Cylinder::new(5.2, 0.32)))),
        MeshMaterial3d(glow),
        Transform::from_translation(position + Vec3::new(0.0, -0.82, 0.0))
            .with_scale(Vec3::new(1.2, 1.0, 0.48)),
        RaidUfoMarker { raid_id },
        WorldGeometry,
    ));
}

fn raid_combat_resolution_system(
    mut raids: ResMut<RaidRegistry>,
    mut sites: ResMut<WorldSiteRegistry>,
    threat_q: Query<(&RaidThreatMarker, &Health)>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for raid_id in raids.spawned_active_ids() {
        let alive = threat_q
            .iter()
            .any(|(marker, health)| marker.raid_id == raid_id && health.is_alive());
        if alive {
            continue;
        }
        let site_id = raids
            .get(raid_id)
            .map(|raid| raid.target_site)
            .unwrap_or(CLOUDRAIL_TUTORIAL_RAID_SITE);
        if raids.resolve_combat_success(raid_id, &mut sites) {
            let site_name = site_name(&sites, site_id);
            msg_ev.write(UiMessageEvent {
                text: format!("{site_name} saved! The raid has been cleared."),
                duration: 5.0,
            });
        }
    }
}

fn raid_cleanup_system(
    mut commands: Commands,
    raids: Res<RaidRegistry>,
    threat_q: Query<(Entity, &RaidThreatMarker)>,
    ufo_q: Query<(Entity, &RaidUfoMarker)>,
) {
    for (entity, marker) in threat_q.iter() {
        if raids.is_resolved(marker.raid_id) {
            commands.entity(entity).try_despawn();
        }
    }
    for (entity, marker) in ufo_q.iter() {
        if raids.is_resolved(marker.raid_id) {
            commands.entity(entity).try_despawn();
        }
    }
}

fn site_name(sites: &WorldSiteRegistry, site_id: crate::resources::WorldSiteId) -> &'static str {
    sites
        .get(site_id)
        .map(|site| site.name)
        .unwrap_or("Unknown site")
}

fn raid_site_name(
    raids: &RaidRegistry,
    sites: &WorldSiteRegistry,
    raid_id: RaidId,
) -> &'static str {
    raids
        .get(raid_id)
        .map(|raid| site_name(sites, raid.target_site))
        .unwrap_or("Unknown site")
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn collider_debug_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<ColliderDebugState>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mesh_collider_q: Query<
        (Entity, &Mesh3d),
        (
            With<crate::engine::physics::prelude::Collider>,
            With<WorldGeometry>,
            Without<ColliderDebugSource>,
            Without<ColliderDebugVisual>,
        ),
    >,
    box_collider_q: Query<
        (Entity, &DebugColliderBox),
        (
            With<crate::engine::physics::prelude::Collider>,
            With<WorldGeometry>,
            Without<Mesh3d>,
            Without<ColliderDebugSource>,
        ),
    >,
    source_q: Query<Entity, With<ColliderDebugSource>>,
    visual_q: Query<Entity, With<ColliderDebugVisual>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    // Collider debug view: Designer edition only.
    if !cfg!(feature = "designer") || !keyboard.just_pressed(KeyCode::F9) {
        return;
    }

    state.enabled = !state.enabled;
    if !state.enabled {
        for entity in visual_q.iter() {
            commands.entity(entity).despawn();
        }
        for entity in source_q.iter() {
            commands.entity(entity).remove::<ColliderDebugSource>();
        }
        msg_ev.write(UiMessageEvent {
            text: "Collider debug: OFF".into(),
            duration: 1.5,
        });
        return;
    }

    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.15, 1.0, 0.42, 0.24),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    for (entity, mesh) in mesh_collider_q.iter() {
        commands.entity(entity).insert(ColliderDebugSource);
        commands.entity(entity).with_children(|parent| {
            parent.spawn((
                PbrBundle {
                    mesh: mesh.clone(),
                    material: MeshMaterial3d(material.clone()),
                    transform: Transform::from_scale(Vec3::splat(1.018)),
                    ..default()
                },
                ColliderDebugVisual,
            ));
        });
    }

    for (entity, debug_box) in box_collider_q.iter() {
        commands.entity(entity).insert(ColliderDebugSource);
        commands.entity(entity).with_children(|parent| {
            parent.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(
                        debug_box.size.x,
                        debug_box.size.y,
                        debug_box.size.z,
                    ))),
                    material: MeshMaterial3d(material.clone()),
                    transform: Transform::from_scale(Vec3::splat(1.006)),
                    ..default()
                },
                ColliderDebugVisual,
            ));
        });
    }

    msg_ev.write(UiMessageEvent {
        text: "Collider debug: ON (F9 toggles)".into(),
        duration: 2.0,
    });
}

fn moving_platform_system(
    time: Res<Time>,
    mut platform_q: Query<(&mut Transform, &MovingPlatform)>,
    mut player_q: Query<(&mut Transform, &PlayerMovement), (With<Player>, Without<MovingPlatform>)>,
) {
    let t = time.elapsed_secs();
    for (mut transform, platform) in platform_q.iter_mut() {
        let previous = transform.translation;
        let travel = platform.end - platform.start;
        let distance = travel.length().max(0.1);
        let cycle = t * platform.speed / distance + platform.phase;
        let eased = 0.5 - 0.5 * cycle.sin();
        let target = platform.start.lerp(platform.end, eased);
        let delta = target - previous;
        transform.translation = target;

        if delta.length_squared() <= 0.000001 {
            continue;
        }

        let half = platform.size * 0.5;
        let top = target.y + half.y;
        for (mut player_transform, movement) in player_q.iter_mut() {
            let rel = player_transform.translation - target;
            let horizontally_on_platform =
                rel.x.abs() <= half.x + 0.65 && rel.z.abs() <= half.z + 0.65;
            let feet_y = player_transform.translation.y - 0.95;
            let vertically_supported =
                movement.is_grounded && feet_y >= top - 0.35 && feet_y <= top + 0.65;
            let landing_on_platform = !movement.is_grounded
                && movement.velocity.y <= 0.0
                && feet_y >= top - 0.12
                && feet_y <= top + 0.28;
            if horizontally_on_platform && (vertically_supported || landing_on_platform) {
                player_transform.translation += delta;
            }
        }
    }
}

fn rotating_elevator_system(
    time: Res<Time>,
    mut elevator_q: Query<(&mut Transform, &RotatingElevator)>,
    mut player_q: Query<
        (&mut Transform, &PlayerMovement),
        (With<Player>, Without<RotatingElevator>),
    >,
) {
    let t = time.elapsed_secs();
    for (mut transform, elevator) in elevator_q.iter_mut() {
        let previous = transform.translation;
        let angle = t * elevator.angular_speed + elevator.phase;
        let vertical =
            (t * elevator.vertical_speed + elevator.phase).sin() * elevator.vertical_amplitude;
        let target = elevator.center
            + Vec3::new(
                angle.cos() * elevator.radius,
                vertical,
                angle.sin() * elevator.radius,
            );
        let delta = target - previous;
        transform.translation = target;
        transform.rotation = Quat::from_rotation_y(angle);

        if delta.length_squared() <= 0.000001 {
            continue;
        }

        let half = elevator.size * 0.5;
        let top = target.y + half.y;
        for (mut player_transform, movement) in player_q.iter_mut() {
            let rel = player_transform.translation - target;
            let horizontally_on_platform =
                rel.x.abs() <= half.x + 0.75 && rel.z.abs() <= half.z + 0.75;
            let feet_y = player_transform.translation.y - 0.95;
            let vertically_supported =
                movement.is_grounded && feet_y >= top - 0.42 && feet_y <= top + 0.70;
            let landing_on_platform = !movement.is_grounded
                && movement.velocity.y <= 0.0
                && feet_y >= top - 0.16
                && feet_y <= top + 0.34;
            if horizontally_on_platform && (vertically_supported || landing_on_platform) {
                player_transform.translation += delta;
            }
        }
    }
}

fn flip_platform_system(
    time: Res<Time>,
    mut panel_q: Query<(&mut Transform, &mut FlipPlatform)>,
    player_q: Query<(&Transform, &PlayerInput), (With<Player>, Without<FlipPlatform>)>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut panel) in &mut panel_q {
        let triggered = player_q.iter().any(|(player_transform, input)| {
            let offset = player_transform.translation - transform.translation;
            input.jump
                && offset.with_y(0.0).length() <= panel.trigger_radius
                && offset.y.abs() <= 3.2
        });
        advance_flip_platform(&mut panel, &mut transform, triggered, dt);
    }
}

fn advance_flip_platform(
    panel: &mut FlipPlatform,
    transform: &mut Transform,
    triggered: bool,
    dt: f32,
) {
    panel.cooldown_timer = (panel.cooldown_timer - dt).max(0.0);
    if triggered && panel.cooldown_timer <= 0.0 {
        panel.flipped = !panel.flipped;
        panel.cooldown_timer = panel.cooldown;
    }
    let target = if panel.flipped { 1.0 } else { 0.0 };
    let step = dt / panel.turn_seconds.max(0.05);
    panel.progress = if panel.progress < target {
        (panel.progress + step).min(target)
    } else {
        (panel.progress - step).max(target)
    };
    let eased = panel.progress * panel.progress * (3.0 - 2.0 * panel.progress);
    transform.rotation = panel.base_rotation * Quat::from_rotation_x(std::f32::consts::PI * eased);
}

fn player_supported_by_platform(
    player_position: Vec3,
    movement: &PlayerMovement,
    platform_position: Vec3,
    size: Vec3,
) -> bool {
    let relative = player_position - platform_position;
    let half = size * 0.5;
    let feet_y = player_position.y - 0.95;
    relative.x.abs() <= half.x + 0.55
        && relative.z.abs() <= half.z + 0.55
        && movement.is_grounded
        && feet_y >= platform_position.y + half.y - 0.38
        && feet_y <= platform_position.y + half.y + 0.68
}

fn advance_collapse_platform(
    platform: &mut CollapsePlatform,
    transform: &mut Transform,
    occupied: bool,
    dt: f32,
) {
    match platform.state {
        CollapsePlatformState::Armed if occupied => {
            platform.state = CollapsePlatformState::Warning;
            platform.timer = 0.0;
        }
        CollapsePlatformState::Warning => {
            platform.timer += dt;
            let shake = (platform.timer * 48.0).sin() * 0.07;
            transform.translation = platform.origin + Vec3::X * shake;
            if platform.timer >= platform.warning_seconds {
                platform.state = CollapsePlatformState::Fallen;
                platform.timer = 0.0;
            }
        }
        CollapsePlatformState::Fallen => {
            platform.timer += dt;
            let drop = (platform.timer * 8.0).min(platform.drop_distance);
            transform.translation = platform.origin - Vec3::Y * drop;
            if platform.timer >= platform.fallen_seconds {
                platform.state = CollapsePlatformState::Resetting;
                platform.timer = 0.0;
            }
        }
        CollapsePlatformState::Resetting => {
            platform.timer += dt;
            let progress = (platform.timer / 0.45).clamp(0.0, 1.0);
            let eased = progress * progress * (3.0 - 2.0 * progress);
            transform.translation =
                (platform.origin - Vec3::Y * platform.drop_distance).lerp(platform.origin, eased);
            if progress >= 1.0 {
                transform.translation = platform.origin;
                platform.state = CollapsePlatformState::Armed;
                platform.timer = 0.0;
            }
        }
        CollapsePlatformState::Armed => {}
    }
}

fn collapse_platform_system(
    time: Res<Time>,
    mut platform_q: Query<(&mut Transform, &mut CollapsePlatform)>,
    player_q: Query<(&Transform, &PlayerMovement), (With<Player>, Without<CollapsePlatform>)>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut platform) in &mut platform_q {
        let occupied = platform.state == CollapsePlatformState::Armed
            && player_q.iter().any(|(player_transform, movement)| {
                player_supported_by_platform(
                    player_transform.translation,
                    movement,
                    transform.translation,
                    platform.size,
                )
            });
        advance_collapse_platform(&mut platform, &mut transform, occupied, dt);
    }
}

fn spike_platform_hazard_system(
    time: Res<Time>,
    mut hazard_q: Query<(&Transform, &mut SpikePlatformHazard)>,
    mut player_q: Query<
        (
            &PlayerIndex,
            &Transform,
            &mut Health,
            &mut Damageable,
            &mut PlayerStats,
            &mut ParryState,
            &ArmorSet,
        ),
        With<Player>,
    >,
    mut damaged_ev: MessageWriter<PlayerDamagedEvent>,
    mut parry_ev: MessageWriter<PlayerParryEvent>,
) {
    let dt = time.delta_secs();
    for (hazard_transform, mut hazard) in &mut hazard_q {
        for timer in &mut hazard.cooldown_timers {
            *timer = (*timer - dt).max(0.0);
        }
        for (index, player_transform, mut health, mut damageable, mut stats, mut parry, armor) in
            &mut player_q
        {
            let slot = index.0 as usize;
            if slot >= hazard.cooldown_timers.len() || hazard.cooldown_timers[slot] > 0.0 {
                continue;
            }
            if !spike_platform_contact(
                player_transform.translation,
                hazard_transform.translation,
                hazard.size,
            ) {
                continue;
            }
            crate::plugins::player_plugin::damage_player(
                Some(index.0),
                &mut health,
                &mut damageable,
                &mut stats,
                &mut parry,
                armor,
                &DamageInfo::new(hazard.damage, DamageType::Collision)
                    .with_knockback(1.4)
                    .with_hit_direction(Vec3::Y),
                &mut damaged_ev,
                &mut parry_ev,
            );
            hazard.cooldown_timers[slot] = hazard.cooldown;
        }
    }
}

fn spike_platform_contact(player_position: Vec3, hazard_position: Vec3, size: Vec3) -> bool {
    let relative = player_position - hazard_position;
    let half = size * 0.5;
    let feet_y = player_position.y - 0.95;
    let spike_top = hazard_position.y + size.y * 0.72;
    relative.x.abs() <= half.x
        && relative.z.abs() <= half.z
        && feet_y >= spike_top - 0.42
        && feet_y <= spike_top + 0.48
}

fn sling_shot_system(
    time: Res<Time>,
    mut pad_q: Query<(&Transform, &mut SlingShotPad)>,
    mut player_q: Query<(
        &Transform,
        &PlayerInput,
        &mut PlayerMovement,
        &mut PlayerStateMachine,
    )>,
) {
    let dt = time.delta_secs();
    for (pad_transform, mut pad) in pad_q.iter_mut() {
        pad.cooldown_timer = (pad.cooldown_timer - dt).max(0.0);
        if pad.cooldown_timer > 0.0 {
            continue;
        }

        for (player_transform, input, mut movement, mut state) in player_q.iter_mut() {
            let offset = player_transform.translation - pad_transform.translation;
            let close = offset.with_y(0.0).length() <= pad.radius && offset.y.abs() <= 3.2;
            if close && (input.jump || input.interact) {
                movement.velocity.y = pad.launch_velocity.y.max(movement.jump_force);
                movement.ground_velocity = pad.launch_velocity.with_y(0.0);
                movement.is_grounded = false;
                movement.coyote_timer = 0.0;
                movement.jump_buffer_timer = 0.0;
                movement.wall_jump_lock_timer = movement.wall_jump_lock_time;
                state.force(crate::components::player::PlayerState::Jetpack);
                pad.cooldown_timer = pad.cooldown;
                break;
            }
        }
    }
}

fn spring_jump_pad_system(
    time: Res<Time>,
    vehicle_state: Option<Res<VehicleState>>,
    mut pad_q: Query<(&Transform, &mut SpringJumpPad)>,
    mut player_q: Query<(
        &PlayerIndex,
        &Transform,
        &mut PlayerMovement,
        &mut TraversalModeState,
        &mut PlayerStateMachine,
        Option<&BoatPassenger>,
    )>,
    mut action_sfx: MessageWriter<ModularActionSfxEvent>,
) {
    let dt = time.delta_secs();
    for (pad_transform, mut pad) in pad_q.iter_mut() {
        for timer in &mut pad.cooldown_timers {
            *timer = (*timer - dt).max(0.0);
        }
        let mut triggered = false;
        for (index, player_transform, mut movement, mut traversal, mut state, passenger) in
            player_q.iter_mut()
        {
            let slot = index.0 as usize;
            if slot >= pad.cooldown_timers.len() || pad.cooldown_timers[slot] > 0.0 {
                continue;
            }
            let offset = player_transform.translation - pad_transform.translation;
            let on_spring =
                offset.with_y(0.0).length() <= pad.radius && offset.y >= -1.4 && offset.y <= 3.8;
            if !on_spring || movement.velocity.y > 4.0 {
                continue;
            }
            let vehicle_active = passenger.is_some()
                || vehicle_state
                    .as_ref()
                    .is_some_and(|vehicles| vehicles.player_owns_active_vehicle(index.0));
            if pad.force_hoverboard && !vehicle_active {
                traversal.active = TraversalMode::Hoverboard;
            }
            movement.ground_velocity = pad.launch_velocity.with_y(0.0);
            movement.velocity.y = pad.launch_velocity.y.max(movement.jump_force * 1.15);
            movement.is_grounded = false;
            movement.coyote_timer = 0.0;
            movement.jump_buffer_timer = 0.0;
            state.force(crate::components::player::PlayerState::Jetpack);
            pad.cooldown_timers[slot] = pad.cooldown;
            triggered = true;
        }
        if triggered {
            action_sfx.write(ModularActionSfxEvent::new("hoverboard.spring"));
        }
    }
}

fn closest_point_on_stunt_rail(point: Vec3, rail: &StuntGrindRail) -> (f32, Vec3, f32) {
    let delta = rail.end - rail.start;
    let length_squared = delta.length_squared();
    if length_squared <= 0.0001 {
        return (0.0, rail.start, point.distance(rail.start));
    }
    let progress = ((point - rail.start).dot(delta) / length_squared).clamp(0.0, 1.0);
    let closest = rail.start + delta * progress;
    (progress, closest, point.distance(closest))
}

fn stunt_grind_rail_system(
    mut commands: Commands,
    time: Res<Time>,
    vehicle_state: Option<Res<VehicleState>>,
    rail_q: Query<(Entity, &StuntGrindRail)>,
    mut player_q: Query<
        (
            Entity,
            &PlayerIndex,
            &mut Transform,
            &PlayerInput,
            &mut PlayerMovement,
            &mut TraversalModeState,
            &mut PlayerStateMachine,
            Option<&RailGrindState>,
            Option<&BoatPassenger>,
        ),
        With<Player>,
    >,
    mut action_sfx: MessageWriter<ModularActionSfxEvent>,
) {
    let dt = time.delta_secs();
    for (
        entity,
        index,
        mut transform,
        input,
        mut movement,
        mut traversal,
        mut state,
        grinding,
        passenger,
    ) in player_q.iter_mut()
    {
        let vehicle_active = passenger.is_some()
            || vehicle_state
                .as_ref()
                .is_some_and(|vehicles| vehicles.player_owns_active_vehicle(index.0));
        if vehicle_active {
            if grinding.is_some() {
                commands.entity(entity).remove::<RailGrindState>();
            }
            continue;
        }
        if let Some(grinding) = grinding {
            let Ok((_, rail)) = rail_q.get(grinding.rail) else {
                commands.entity(entity).remove::<RailGrindState>();
                continue;
            };
            movement.clear_motor_delivery();
            let delta = rail.end - rail.start;
            let length = delta.length().max(0.001);
            let direction = delta / length * grinding.direction;
            if input.jump {
                movement.ground_velocity = direction.with_y(0.0).normalize_or_zero() * rail.speed;
                movement.velocity.y = movement.jump_force.max(rail.exit_lift);
                movement.is_grounded = false;
                commands.entity(entity).remove::<RailGrindState>();
                state.force(crate::components::player::PlayerState::Jetpack);
                action_sfx.write(ModularActionSfxEvent::new("hoverboard.grind_exit"));
                continue;
            }

            let next_progress = grinding.progress + grinding.direction * rail.speed * dt / length;
            if !(0.0..=1.0).contains(&next_progress) {
                let exit = if grinding.direction > 0.0 {
                    rail.end
                } else {
                    rail.start
                };
                transform.translation = exit + direction * 4.0 + Vec3::Y * 1.1;
                movement.ground_velocity =
                    direction.with_y(0.0).normalize_or_zero() * (rail.speed * 1.12);
                movement.velocity.y = rail.exit_lift;
                movement.is_grounded = false;
                commands.entity(entity).remove::<RailGrindState>();
                state.force(crate::components::player::PlayerState::Jetpack);
                action_sfx.write(ModularActionSfxEvent::new("hoverboard.grind_exit"));
                continue;
            }

            transform.translation = rail.start + delta * next_progress + Vec3::Y * 1.1;
            movement.ground_velocity = direction.with_y(0.0).normalize_or_zero() * rail.speed;
            movement.velocity.y = delta.y / length * rail.speed * grinding.direction;
            movement.is_grounded = false;
            traversal.active = TraversalMode::Hoverboard;
            commands.entity(entity).insert(RailGrindState {
                progress: next_progress,
                ..*grinding
            });
            state.force(crate::components::player::PlayerState::Sprinting);
            continue;
        }

        let nearest = rail_q
            .iter()
            .filter_map(|(rail_entity, rail)| {
                let (progress, closest, distance) =
                    closest_point_on_stunt_rail(transform.translation - Vec3::Y * 1.1, rail);
                (distance <= rail.snap_radius && movement.velocity.y <= 3.5).then_some((
                    distance,
                    rail_entity,
                    rail,
                    progress,
                    closest,
                ))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, rail_entity, rail, progress, closest)) = nearest else {
            continue;
        };
        let authored_direction = (rail.end - rail.start).normalize_or_zero();
        let velocity = movement.ground_velocity + Vec3::Y * movement.velocity.y;
        let direction = if velocity.dot(authored_direction) < -2.0 {
            -1.0
        } else {
            1.0
        };
        transform.translation = closest + Vec3::Y * 1.1;
        movement.clear_motor_delivery();
        movement.is_grounded = false;
        traversal.active = TraversalMode::Hoverboard;
        commands.entity(entity).insert(RailGrindState {
            rail: rail_entity,
            progress,
            direction,
        });
        state.force(crate::components::player::PlayerState::Sprinting);
        action_sfx.write(ModularActionSfxEvent::new("hoverboard.grind_start"));
    }
}

fn bank_stunt_run(run: &mut StuntRunState) -> u64 {
    if !run.active {
        return 0;
    }
    let rotations = (run.spin_degrees.abs() / 360.0).floor() as u32;
    if rotations > 0 {
        run.pending_score += rotations as f32 * 325.0;
        run.trick_count = run.trick_count.saturating_add(rotations as u16);
    }
    let banked = (run.pending_score * run.multiplier).round().max(0.0) as u64;
    run.total_score = run.total_score.saturating_add(banked);
    run.pending_score = 0.0;
    run.multiplier = 1.0;
    run.airtime = 0.0;
    run.spin_degrees = 0.0;
    run.trick_count = 0;
    run.active = false;
    banked
}

/// Which trick category a button press requests while riding the board.
/// Mirrors the Tony Hawk face-button layout (flip / grab / grind / manual).
fn requested_trick_category(input: &PlayerInput) -> Option<TrickCategory> {
    if input.melee_light {
        Some(TrickCategory::Flip)
    } else if input.melee_heavy {
        Some(TrickCategory::Grab)
    } else if input.dodge {
        Some(TrickCategory::Grind)
    } else if input.parry {
        Some(TrickCategory::Manual)
    } else {
        None
    }
}

/// Can this category start given the rider's current situation?
/// Air tricks need airtime, grinds need a rail, manuals need ground.
fn trick_category_available(category: TrickCategory, is_grounded: bool, is_grinding: bool) -> bool {
    match category {
        TrickCategory::Grab | TrickCategory::Flip | TrickCategory::Special => {
            !is_grounded && !is_grinding
        }
        TrickCategory::Grind => is_grinding,
        TrickCategory::Manual => is_grounded && !is_grinding,
        TrickCategory::Lip => !is_grounded,
    }
}

/// Score a landed trick: base × spin bonus ÷ repeat decay.
fn landed_trick_score(base_score: f32, spin_degrees: f32, times_already_used: u32) -> f32 {
    base_score * TrickLibrary::spin_bonus(spin_degrees)
        / TrickLibrary::repeat_divisor(times_already_used)
}

/// Hoverboard trick execution: turn button+direction into a named trick,
/// hold it, then land it (score) or blow it (bail). Runs before
/// `stunt_combo_system`, which owns airtime/grind accrual and banking.
#[allow(clippy::too_many_arguments)]
fn hoverboard_trick_system(
    time: Res<Time>,
    library: Res<TrickLibrary>,
    mut player_q: Query<
        (
            &PlayerIndex,
            &PlayerInput,
            &PlayerMovement,
            &TraversalModeState,
            Option<&RailGrindState>,
            &mut StuntRunState,
            &mut TrickState,
        ),
        With<Player>,
    >,
    mut msg_ev: MessageWriter<UiMessageEvent>,
    mut action_sfx: MessageWriter<ModularActionSfxEvent>,
) {
    let dt = time.delta_secs();
    for (index, input, movement, traversal, grinding, mut run, mut tricks) in player_q.iter_mut() {
        let on_board = traversal.active == TraversalMode::Hoverboard;
        if !on_board {
            // Stepping off the board abandons any held trick silently.
            tricks.active = None;
            tricks.bailed = false;
            continue;
        }
        tricks.bailed = false;
        let is_grinding = grinding.is_some();
        let grounded = movement.is_grounded;
        // The revert window only ticks down on the ground; airborne time is
        // already combo time, so it must not expire mid-flight.
        if grounded && tricks.revert_window > 0.0 {
            tricks.revert_window = (tricks.revert_window - dt).max(0.0);
            if tricks.revert_window <= 0.0 {
                tricks.reverting = false;
            }
        }

        // ── Resolve the held trick ───────────────────────────────────────
        if let Some(active) = tricks.active.clone() {
            let air_trick = active.category.needs_air();
            if air_trick && grounded {
                // Landed it: bank the trick into the pending combo.
                let times_used = tricks.times_used(active.index);
                let score = landed_trick_score(active.base_score, run.spin_degrees, times_used);
                run.active = true;
                run.pending_score += score;
                run.multiplier =
                    (run.multiplier + active.multiplier_bonus).min(library.max_multiplier);
                run.trick_count = run.trick_count.saturating_add(1);
                tricks.record_use(active.index);
                tricks.last_trick = Some(active.name.clone());
                tricks.active = None;
                // Open the revert window: another trick (or a rail) started
                // inside it links into this combo instead of banking it.
                tricks.revert_window = library.revert_window;
                tricks.reverting = false;
                action_sfx.write(ModularActionSfxEvent::new("hoverboard.trick_land"));
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "P{}  {} {}  +{}  x{:.2}",
                        index.0 + 1,
                        active.category.label().to_uppercase(),
                        active.name,
                        score.round() as u64,
                        run.multiplier
                    ),
                    duration: 1.4,
                });
            } else if air_trick && active.remaining <= 0.0 {
                // Held too long without landing — bail and lose the combo.
                tricks.reset_combo();
                tricks.bailed = true;
                tricks.last_trick = None;
                run.pending_score = 0.0;
                run.multiplier = 1.0;
                run.spin_degrees = 0.0;
                run.trick_count = 0;
                run.active = false;
                action_sfx.write(ModularActionSfxEvent::new("hoverboard.land"));
                msg_ev.write(UiMessageEvent {
                    text: format!("P{}  BAIL — combo lost", index.0 + 1),
                    duration: 1.6,
                });
            } else if !air_trick
                && !trick_category_available(active.category, grounded, is_grinding)
            {
                // Surface trick ended because its surface did (left the rail
                // or the ground): score it, no bail.
                let times_used = tricks.times_used(active.index);
                let score = landed_trick_score(active.base_score, run.spin_degrees, times_used);
                run.active = true;
                run.pending_score += score;
                run.multiplier =
                    (run.multiplier + active.multiplier_bonus).min(library.max_multiplier);
                run.trick_count = run.trick_count.saturating_add(1);
                tricks.record_use(active.index);
                tricks.last_trick = Some(active.name.clone());
                tricks.active = None;
            } else if let Some(held) = tricks.active.as_mut() {
                held.remaining -= dt;
            }
        }

        // ── Start a new trick ────────────────────────────────────────────
        if tricks.active.is_none() {
            if let Some(category) = requested_trick_category(input) {
                if trick_category_available(category, grounded, is_grinding) {
                    let direction = TrickDir::from_axis(input.move_axis);
                    if let Some((index_in_lib, def)) = library
                        .resolve(category, direction, run.multiplier)
                        .and_then(|def| library.index_of(&def.name).map(|i| (i, def)))
                    {
                        tricks.active = Some(ActiveTrick {
                            index: index_in_lib,
                            name: def.name.clone(),
                            category: def.category,
                            remaining: def.duration,
                            base_score: def.base_score,
                            multiplier_bonus: def.multiplier_bonus,
                        });
                        run.active = true;
                        // Chaining inside the revert window keeps the combo
                        // alive across the landing and pays a bonus.
                        if tricks.can_revert() {
                            tricks.reverting = true;
                            tricks.revert_window = library.revert_window;
                            run.multiplier = (run.multiplier + library.revert_multiplier_bonus)
                                .min(library.max_multiplier);
                        }
                        if def.category == TrickCategory::Grind {
                            action_sfx.write(ModularActionSfxEvent::new("hoverboard.grind_start"));
                        }
                    }
                }
            }
        }
    }
}

fn stunt_combo_system(
    time: Res<Time>,
    settings: Res<GameSettings>,
    gamepad_q: Query<Entity, With<Gamepad>>,
    mut player_q: Query<
        (
            &PlayerIndex,
            &PlayerInput,
            &PlayerMovement,
            &TraversalModeState,
            Option<&RailGrindState>,
            &mut StuntRunState,
            &mut TrickState,
        ),
        With<Player>,
    >,
    mut msg_ev: MessageWriter<UiMessageEvent>,
    mut rumble_ev: MessageWriter<GamepadRumbleRequest>,
    mut action_sfx: MessageWriter<ModularActionSfxEvent>,
) {
    let dt = time.delta_secs();
    let mut gamepads = gamepad_q.iter().collect::<Vec<_>>();
    gamepads.sort_by_key(|entity| entity.index());
    for (index, input, movement, traversal, grinding, mut run, mut tricks) in player_q.iter_mut() {
        let is_grinding = grinding.is_some();
        if is_grinding {
            if !run.was_grinding {
                run.active = true;
                run.pending_score += 250.0;
                run.multiplier = (run.multiplier + 0.25).min(8.0);
                run.trick_count = run.trick_count.saturating_add(1);
            }
            run.pending_score += 115.0 * dt;
        } else if traversal.active == TraversalMode::Hoverboard && !movement.is_grounded {
            run.active = true;
            run.airtime += dt;
            run.pending_score += 55.0 * dt;
            run.spin_degrees += input.move_axis.x * 270.0 * dt;
        }

        // Hold the bank while the revert window is open or a trick is still
        // being worked — that is what lets a combo survive a landing. The
        // condition is state-based rather than a landing edge so the run also
        // banks on the frame a revert window lapses (the rider is by then
        // already grounded, so an edge test would never fire again).
        let linking = tricks.can_revert() || tricks.active.is_some();
        let landed = movement.is_grounded && !is_grinding && !linking;
        if landed && run.active {
            tricks.reset_combo();
            let multiplier = run.multiplier;
            let banked = bank_stunt_run(&mut run);
            if banked > 0 {
                action_sfx.write(ModularActionSfxEvent::new("hoverboard.trick_land"));
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "P{} STUNT LANDED +{}  x{multiplier:.2}  TOTAL {}",
                        index.0 + 1,
                        banked,
                        run.total_score
                    ),
                    duration: 2.5,
                });
                if settings.rumble_on_hit {
                    if let Some(gamepad) = gamepads.get(usize::from(index.0)) {
                        let strength = (0.25 + multiplier * 0.08).min(0.9);
                        rumble_ev.write(GamepadRumbleRequest::Add {
                            gamepad: *gamepad,
                            duration: Duration::from_millis(110),
                            intensity: GamepadRumbleIntensity {
                                strong_motor: strength,
                                weak_motor: strength * 0.65,
                            },
                        });
                    }
                }
            }
        }
        run.was_grounded = movement.is_grounded;
        run.was_grinding = is_grinding;
    }
}

fn stunt_race_gate_system(
    time: Res<Time>,
    gate_q: Query<(&Transform, &StuntRaceGate), Without<Player>>,
    mut player_q: Query<
        (
            &Transform,
            &PlayerIndex,
            &TraversalModeState,
            &mut StuntRaceProgress,
        ),
        With<Player>,
    >,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    for (transform, index, traversal, mut progress) in player_q.iter_mut() {
        progress.gate_cooldown = (progress.gate_cooldown - dt).max(0.0);
        if progress.gate_cooldown > 0.0 || traversal.active != TraversalMode::Hoverboard {
            continue;
        }
        let gate = gate_q
            .iter()
            .find(|(gate_transform, gate)| {
                let valid_course = progress.course_id == Some(gate.course_id)
                    || (progress.course_id.is_none() && gate.gate_index == 0);
                valid_course
                    && gate.gate_index == progress.next_gate
                    && transform.translation.distance(gate_transform.translation) <= gate.radius
            })
            .map(|(_, gate)| gate);
        let Some(gate) = gate else {
            continue;
        };

        progress.gate_cooldown = 0.55;
        if progress.course_id.is_none() {
            progress.course_id = Some(gate.course_id);
            progress.lap = 1;
            progress.next_gate = 1;
            msg_ev.write(UiMessageEvent {
                text: format!("P{} {} — LAP 1/3 GO!", index.0 + 1, gate.course_label),
                duration: 2.4,
            });
            continue;
        }

        if gate.gate_index == 0 {
            if progress.lap >= 3 {
                progress.races_finished = progress.races_finished.saturating_add(1);
                progress.course_id = None;
                progress.next_gate = 0;
                progress.lap = 0;
                progress.gate_cooldown = 2.0;
                msg_ev.write(UiMessageEvent {
                    text: format!("P{} {} — FINISH!", index.0 + 1, gate.course_label),
                    duration: 3.5,
                });
            } else {
                progress.lap += 1;
                progress.next_gate = 1;
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "P{} {} — LAP {}/3",
                        index.0 + 1,
                        gate.course_label,
                        progress.lap
                    ),
                    duration: 2.0,
                });
            }
        } else {
            progress.next_gate = (gate.gate_index + 1) % gate.gate_count;
        }
    }
}

fn board_boost_pad_system(
    time: Res<Time>,
    vehicle_state: Option<Res<VehicleState>>,
    mut pad_q: Query<(&Transform, &mut BoardBoostPad)>,
    mut player_q: Query<(
        &PlayerIndex,
        &Transform,
        &PlayerProgression,
        &mut PlayerMovement,
        &mut TraversalModeState,
        &mut BoardBoostState,
        &mut PlayerStateMachine,
        Option<&BoatPassenger>,
    )>,
) {
    let dt = time.delta_secs();
    for (pad_transform, mut pad) in pad_q.iter_mut() {
        for timer in &mut pad.cooldown_timers {
            *timer = (*timer - dt).max(0.0);
        }

        let direction = pad.direction.with_y(0.0).normalize_or_zero();
        if direction == Vec3::ZERO {
            continue;
        }

        let inv_rot = pad_transform.rotation.inverse();
        for (
            index,
            player_transform,
            progression,
            mut movement,
            mut traversal,
            mut boost,
            mut state,
            passenger,
        ) in player_q.iter_mut()
        {
            let slot = index.0 as usize;
            if slot >= pad.cooldown_timers.len() || pad.cooldown_timers[slot] > 0.0 {
                continue;
            }
            let local = inv_rot * (player_transform.translation - pad_transform.translation);
            let on_pad = local.x.abs() <= pad.half_width
                && local.z.abs() <= pad.half_length
                && local.y >= -2.2
                && local.y <= 4.2;
            if !on_pad {
                continue;
            }

            let vehicle_active = passenger.is_some()
                || vehicle_state
                    .as_ref()
                    .is_some_and(|vehicles| vehicles.player_owns_active_vehicle(index.0));
            if pad.force_hoverboard && !vehicle_active {
                traversal.active = TraversalMode::Hoverboard;
            }

            let upgrade_mult = road_boost_upgrade_mult(&progression.upgrades);
            boost.timer = boost
                .timer
                .max(pad.duration * (1.0 + (upgrade_mult - 1.0) * 0.25));
            boost.speed_mult = boost.speed_mult.max(pad.speed_mult * upgrade_mult.sqrt());
            boost.direction = direction;

            let current_along = movement.ground_velocity.dot(direction);
            let upgraded_impulse = pad.impulse * upgrade_mult;
            if current_along < upgraded_impulse {
                movement.ground_velocity += direction * (upgraded_impulse - current_along);
            }
            if pad.lift > 0.0 {
                movement.velocity.y = movement.velocity.y.max(pad.lift);
                movement.is_grounded = false;
                movement.coyote_timer = 0.0;
                state.force(crate::components::player::PlayerState::Jetpack);
            } else {
                state.transition(crate::components::player::PlayerState::Sprinting);
            }
            pad.cooldown_timers[slot] = pad.cooldown;
        }
    }
}

fn road_boost_upgrade_mult(upgrades: &crate::combat::upgrades::UpgradeLedger) -> f32 {
    (upgrades.boot_ground_speed_mult() * upgrades.armor_speed_mult()).clamp(1.0, 1.40)
}

fn npc_road_vehicle_system(
    mut commands: Commands,
    time: Res<Time>,
    mut vehicle_q: Query<(
        Entity,
        &mut Transform,
        &mut NpcRoadVehicle,
        &Health,
        Option<&mut CityStreetTraffic>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut vehicle, health, city_traffic) in vehicle_q.iter_mut() {
        if vehicle.path.len() < 2 {
            continue;
        }

        if !health.is_alive() {
            vehicle.wreck_timer -= dt;
            transform.translation.y -= dt * 0.16;
            transform.rotate_local_z(dt * 0.42);
            if vehicle.wreck_timer <= 0.0 {
                commands.entity(entity).despawn();
            }
            continue;
        }

        if let Some(mut city_traffic) = city_traffic {
            update_city_traffic_signal(&mut vehicle, &mut city_traffic, dt);
        }
        advance_road_vehicle(&mut vehicle, dt);
        if let Some((position, yaw)) = road_vehicle_pose(&vehicle) {
            let route_dir = Quat::from_rotation_y(yaw) * Vec3::Z;
            let right = Vec3::new(route_dir.z, 0.0, -route_dir.x).normalize_or_zero();
            let bob = (time.elapsed_secs() * 5.3 + vehicle.progress * 0.017).sin() * 0.04;
            transform.translation = position + right * vehicle.lane_offset + Vec3::Y * bob;
            transform.rotation = Quat::from_rotation_y(yaw);
        }
    }
}

fn update_city_traffic_signal(
    vehicle: &mut NpcRoadVehicle,
    traffic: &mut CityStreetTraffic,
    dt: f32,
) {
    if traffic.stop_timer > 0.0 {
        traffic.stop_timer = (traffic.stop_timer - dt).max(0.0);
        vehicle.speed = 0.0;
        return;
    }

    vehicle.speed = traffic.cruise_speed;
    let len = vehicle.path.len();
    if len < 2 {
        return;
    }
    let start = vehicle.path[vehicle.segment % len];
    let end = vehicle.path[(vehicle.segment + 1) % len];
    let remaining = (start.distance(end) - vehicle.progress).max(0.0);
    let should_stop = (vehicle.segment + traffic.signal_phase).is_multiple_of(3);
    if should_stop && remaining <= 1.5 && traffic.last_stopped_segment != vehicle.segment {
        traffic.last_stopped_segment = vehicle.segment;
        traffic.stop_timer = 0.65 + traffic.signal_phase as f32 * 0.12;
        vehicle.speed = 0.0;
    }
}

fn city_pedestrian_system(
    time: Res<Time>,
    mut pedestrian_q: Query<(&mut Transform, &mut CityPedestrian)>,
) {
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();
    for (mut transform, mut pedestrian) in pedestrian_q.iter_mut() {
        if pedestrian.path.len() < 2 {
            continue;
        }
        if pedestrian.pause_timer > 0.0 {
            pedestrian.pause_timer = (pedestrian.pause_timer - dt).max(0.0);
            continue;
        }

        let previous_segment = pedestrian.segment;
        advance_city_pedestrian(&mut pedestrian, dt);
        if pedestrian.segment != previous_segment
            && (pedestrian.segment + pedestrian.phase as usize).is_multiple_of(4)
        {
            pedestrian.pause_timer = 0.35 + pedestrian.phase.fract() * 0.65;
        }
        if let Some((position, yaw)) = city_pedestrian_pose(&pedestrian) {
            let bob = (elapsed * 7.0 + pedestrian.phase * TAU).sin().abs() * 0.055;
            transform.translation = position + Vec3::Y * bob;
            transform.rotation = Quat::from_rotation_y(yaw);
        }
    }
}

fn advance_city_pedestrian(pedestrian: &mut CityPedestrian, dt: f32) {
    let mut remaining = pedestrian.speed.max(0.0) * dt;
    let len = pedestrian.path.len();
    while remaining > 0.0 && len >= 2 {
        let start = pedestrian.path[pedestrian.segment % len];
        let end = pedestrian.path[(pedestrian.segment + 1) % len];
        let segment_len = start.distance(end).max(0.01);
        let available = segment_len - pedestrian.progress;
        if remaining <= available {
            pedestrian.progress += remaining;
            break;
        }
        remaining -= available;
        pedestrian.segment = (pedestrian.segment + 1) % len;
        pedestrian.progress = 0.0;
    }
}

fn city_pedestrian_pose(pedestrian: &CityPedestrian) -> Option<(Vec3, f32)> {
    let len = pedestrian.path.len();
    if len < 2 {
        return None;
    }
    let start = pedestrian.path[pedestrian.segment % len];
    let end = pedestrian.path[(pedestrian.segment + 1) % len];
    let delta = end - start;
    let segment_len = delta.length().max(0.01);
    let position = start.lerp(end, (pedestrian.progress / segment_len).clamp(0.0, 1.0));
    Some((position, delta.x.atan2(delta.z)))
}

fn building_reward_bob_system(
    time: Res<Time>,
    mut reward_q: Query<(&mut Transform, &BuildingExplorationReward)>,
) {
    let elapsed = time.elapsed_secs();
    for (mut transform, reward) in reward_q.iter_mut() {
        transform.translation.y = reward.base_y + (elapsed * 2.4 + reward.bob_phase).sin() * 0.22;
        transform.rotate_y(time.delta_secs() * 1.15);
    }
}

fn apply_building_reward(
    stats: &mut PlayerStats,
    health: &mut Health,
    credits: u32,
    experience: u32,
    armor: u32,
) {
    stats.credits = stats.credits.saturating_add(credits);
    stats.experience = stats.experience.saturating_add(experience);
    stats.armor = (stats.armor + armor as f32).min(stats.max_armor);
    health.heal((armor as f32 * 0.25).max(1.0));
}

fn building_reward_pickup_system(
    mut commands: Commands,
    mut progress: ResMut<ChapterProgress>,
    reward_q: Query<(Entity, &Transform, &BuildingExplorationReward)>,
    beacon_q: Query<(Entity, &BuildingRewardBeacon)>,
    mut player_q: Query<(&PlayerIndex, &Transform, &mut PlayerStats, &mut Health), With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (reward_entity, reward_transform, reward) in reward_q.iter() {
        if progress.has_discoverable(&reward.reward_key) {
            commands.entity(reward_entity).despawn();
            for (beacon_entity, beacon) in beacon_q.iter() {
                if beacon.reward_key == reward.reward_key {
                    commands.entity(beacon_entity).despawn();
                }
            }
            continue;
        }

        let mut collected = None;
        for (player_index, player_transform, mut stats, mut health) in player_q.iter_mut() {
            if player_transform
                .translation
                .distance(reward_transform.translation)
                > reward.pickup_radius
            {
                continue;
            }
            apply_building_reward(
                &mut stats,
                &mut health,
                reward.credits,
                reward.experience,
                reward.armor,
            );
            collected = Some(player_index.0);
            break;
        }

        let Some(player_index) = collected else {
            continue;
        };
        progress.unlock(&reward.reward_key);
        for (beacon_entity, beacon) in beacon_q.iter() {
            if beacon.reward_key == reward.reward_key {
                commands.entity(beacon_entity).despawn();
            }
        }
        let route = match reward.location {
            BuildingRewardLocation::Interior => "interior cache",
            BuildingRewardLocation::Rooftop => "rooftop star cache",
        };
        msg_ev.write(UiMessageEvent {
            text: format!(
                "P{} found a {}: +{} credits, +{} XP, +{} armor",
                player_index + 1,
                route,
                reward.credits,
                reward.experience,
                reward.armor
            ),
            duration: 4.0,
        });
        commands.entity(reward_entity).despawn();
    }
}

fn city_rooftop_route_marker_system(
    time: Res<Time>,
    mut markers: Query<(&mut Transform, &CityRooftopRouteMarker)>,
) {
    let elapsed = time.elapsed_secs();
    for (mut transform, marker) in &mut markers {
        let pulse = 1.0 + (elapsed * 2.8 + marker.phase).sin() * 0.08;
        transform.scale = marker.base_scale * pulse;
        match marker.kind {
            CityRooftopRouteKind::Skybridge => {
                transform.rotate_local_y(time.delta_secs() * 0.65);
            }
            CityRooftopRouteKind::Zipline => {
                transform.rotate_local_z(time.delta_secs() * 0.9);
            }
        }
    }
}

fn rooftop_route_endpoint_crossed(approach: &mut i8, near_start: bool, near_end: bool) -> bool {
    match *approach {
        0 if near_end => {
            *approach = -1;
            true
        }
        1 if near_start => {
            *approach = -1;
            true
        }
        -1 if near_start && !near_end => {
            *approach = 0;
            false
        }
        -1 if near_end && !near_start => {
            *approach = 1;
            false
        }
        _ => false,
    }
}

fn city_rooftop_route_challenge_system(
    mut commands: Commands,
    mut progress: ResMut<ChapterProgress>,
    mut routes: Query<(&CityRooftopRoute, &mut CityRooftopRouteChallenge)>,
    markers: Query<(Entity, &CityRooftopRouteMarker)>,
    mut players: Query<(&PlayerIndex, &Transform, &mut PlayerStats, &mut Health), With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (route, mut challenge) in &mut routes {
        if progress.has_discoverable(&challenge.reward_key) {
            for (entity, marker) in &markers {
                if marker.reward_key == challenge.reward_key {
                    commands.entity(entity).despawn();
                }
            }
            continue;
        }

        let mut completed_by = None;
        for (player_index, transform, mut stats, mut health) in &mut players {
            let slot = player_index.0 as usize;
            let Some(approach) = challenge.approach.get_mut(slot) else {
                continue;
            };
            let position = transform.translation;
            let near_start = position.distance(route.start) <= 3.4;
            let near_end = position.distance(route.end) <= 3.4;
            if !rooftop_route_endpoint_crossed(approach, near_start, near_end) {
                continue;
            }
            apply_building_reward(
                &mut stats,
                &mut health,
                challenge.credits,
                challenge.experience,
                challenge.armor,
            );
            completed_by = Some(player_index.0);
            break;
        }

        let Some(player_index) = completed_by else {
            continue;
        };
        progress.unlock(&challenge.reward_key);
        for (entity, marker) in &markers {
            if marker.reward_key == challenge.reward_key {
                commands.entity(entity).despawn();
            }
        }
        let route_name = match route.kind {
            CityRooftopRouteKind::Skybridge => "skybridge",
            CityRooftopRouteKind::Zipline => "zipline",
        };
        msg_ev.write(UiMessageEvent {
            text: format!(
                "P{} completed a rooftop {}: +{} credits, +{} XP, +{} armor",
                player_index + 1,
                route_name,
                challenge.credits,
                challenge.experience,
                challenge.armor
            ),
            duration: 4.0,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CityRooftopTrialMedal {
    Gold,
    Silver,
    Bronze,
    Finish,
}

impl CityRooftopTrialMedal {
    fn label(self) -> &'static str {
        match self {
            Self::Gold => "GOLD",
            Self::Silver => "SILVER",
            Self::Bronze => "BRONZE",
            Self::Finish => "FINISH",
        }
    }
}

fn city_rooftop_trial_medal(
    elapsed: f32,
    gold_time: f32,
    silver_time: f32,
    bronze_time: f32,
) -> CityRooftopTrialMedal {
    if elapsed <= gold_time {
        CityRooftopTrialMedal::Gold
    } else if elapsed <= silver_time {
        CityRooftopTrialMedal::Silver
    } else if elapsed <= bronze_time {
        CityRooftopTrialMedal::Bronze
    } else {
        CityRooftopTrialMedal::Finish
    }
}

fn unlock_city_rooftop_trial_medal(
    progress: &mut ChapterProgress,
    course_key: &str,
    medal: CityRooftopTrialMedal,
) {
    let tiers: &[&str] = match medal {
        CityRooftopTrialMedal::Gold => &["bronze", "silver", "gold"],
        CityRooftopTrialMedal::Silver => &["bronze", "silver"],
        CityRooftopTrialMedal::Bronze => &["bronze"],
        CityRooftopTrialMedal::Finish => &[],
    };
    for tier in tiers {
        progress.unlock(&format!("{course_key}:medal:{tier}"));
    }
}

fn city_rooftop_time_trial_system(
    time: Res<Time>,
    mut chapter_progress: ResMut<ChapterProgress>,
    gates: Query<(&Transform, &CityRooftopTrialGate), Without<Player>>,
    mut players: Query<
        (
            &Transform,
            &PlayerIndex,
            &mut RooftopTrialProgress,
            &mut PlayerStats,
            &mut Health,
        ),
        With<Player>,
    >,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    for (transform, player_index, mut trial, mut stats, mut health) in &mut players {
        trial.gate_cooldown = (trial.gate_cooldown - dt).max(0.0);
        if trial.active_course.is_some() {
            trial.elapsed += dt;
        }
        if trial.gate_cooldown > 0.0 {
            continue;
        }

        let gate = gates
            .iter()
            .find(|(gate_transform, gate)| {
                let expected_course = trial
                    .active_course
                    .as_ref()
                    .is_some_and(|course| course == &gate.course_key);
                let can_start = trial.active_course.is_none() && gate.checkpoint_index == 0;
                (expected_course || can_start)
                    && gate.checkpoint_index == trial.next_checkpoint
                    && transform.translation.distance(gate_transform.translation) <= gate.radius
            })
            .map(|(_, gate)| gate);
        let Some(gate) = gate else {
            continue;
        };

        trial.gate_cooldown = 0.48;
        if trial.active_course.is_none() {
            trial.active_course = Some(gate.course_key.clone());
            trial.active_label = Some(gate.course_label.clone());
            trial.next_checkpoint = 1;
            trial.checkpoint_count = gate.checkpoint_count;
            trial.elapsed = 0.0;
            msg_ev.write(UiMessageEvent {
                text: format!(
                    "P{} {} — GO! Gold {:.1}s  Silver {:.1}s  Bronze {:.1}s",
                    player_index.0 + 1,
                    gate.course_label,
                    gate.gold_time,
                    gate.silver_time,
                    gate.bronze_time
                ),
                duration: 3.4,
            });
            continue;
        }

        if gate.checkpoint_index + 1 < gate.checkpoint_count {
            trial.next_checkpoint += 1;
            msg_ev.write(UiMessageEvent {
                text: format!(
                    "P{} {} — checkpoint {}/{}  {:.1}s",
                    player_index.0 + 1,
                    gate.course_label,
                    gate.checkpoint_index + 1,
                    gate.checkpoint_count - 1,
                    trial.elapsed
                ),
                duration: 1.35,
            });
            continue;
        }

        let elapsed = trial.elapsed;
        let course_key = gate.course_key.clone();
        let medal =
            city_rooftop_trial_medal(elapsed, gate.gold_time, gate.silver_time, gate.bronze_time);
        let new_best = trial.record_time(&course_key, elapsed);
        let completion_key = format!("{course_key}:complete");
        let first_clear = !chapter_progress.has_discoverable(&completion_key);
        chapter_progress.unlock(&completion_key);
        unlock_city_rooftop_trial_medal(&mut chapter_progress, &course_key, medal);
        if first_clear {
            apply_building_reward(&mut stats, &mut health, 120, 90, 12);
        }
        msg_ev.write(UiMessageEvent {
            text: format!(
                "P{} {} — {} {:.2}s{}{}",
                player_index.0 + 1,
                gate.course_label,
                medal.label(),
                elapsed,
                if new_best { "  NEW BEST" } else { "" },
                if first_clear {
                    "  +120 credits +90 XP +12 armor"
                } else {
                    ""
                }
            ),
            duration: 5.0,
        });
        trial.reset_active();
        trial.gate_cooldown = 1.4;
    }
}

fn advance_road_vehicle(vehicle: &mut NpcRoadVehicle, dt: f32) {
    let mut remaining = vehicle.speed.max(0.0) * dt;
    let len = vehicle.path.len();
    while remaining > 0.0 && len >= 2 {
        let start = vehicle.path[vehicle.segment % len];
        let end = vehicle.path[(vehicle.segment + 1) % len];
        let segment_len = start.distance(end).max(0.01);
        let available = segment_len - vehicle.progress;
        if remaining <= available {
            vehicle.progress += remaining;
            break;
        }

        remaining -= available;
        vehicle.segment = (vehicle.segment + 1) % len;
        vehicle.progress = 0.0;
    }
}

fn road_vehicle_pose(vehicle: &NpcRoadVehicle) -> Option<(Vec3, f32)> {
    let len = vehicle.path.len();
    if len < 2 {
        return None;
    }
    let start = vehicle.path[vehicle.segment % len];
    let end = vehicle.path[(vehicle.segment + 1) % len];
    let delta = end - start;
    let segment_len = delta.length().max(0.01);
    let t = (vehicle.progress / segment_len).clamp(0.0, 1.0);
    let position = start.lerp(end, t);
    let yaw = delta.x.atan2(delta.z);
    Some((position, yaw))
}

fn laser_turret_system(
    mut commands: Commands,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut turret_q: Query<(&mut Transform, &mut LaserTurret)>,
    player_pos_q: Query<(Entity, &Transform, &PlayerIndex), (With<Player>, Without<LaserTurret>)>,
    mut player_damage_q: Query<(
        &mut Health,
        &mut Damageable,
        &mut PlayerStats,
        &mut ParryState,
        &ArmorSet,
    )>,
    mut damaged_ev: MessageWriter<PlayerDamagedEvent>,
    mut parry_ev: MessageWriter<PlayerParryEvent>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut turret) in turret_q.iter_mut() {
        turret.cooldown_timer = (turret.cooldown_timer - dt).max(0.0);
        turret.windup_timer = (turret.windup_timer - dt).max(0.0);

        let origin = transform.translation + Vec3::Y * 0.55;
        let locked_target = turret.locked_target.and_then(|entity| {
            player_pos_q
                .get(entity)
                .ok()
                .and_then(|(_, transform, index)| {
                    let dist = origin.distance(transform.translation);
                    (dist <= turret.range).then_some((entity, transform.translation, index.0, dist))
                })
        });
        let target = locked_target.or_else(|| closest_player(origin, turret.range, &player_pos_q));
        let Some((target_entity, target_pos, target_index, target_distance)) = target else {
            turret.locked_target = None;
            turret.windup_timer = 0.0;
            continue;
        };

        let aim = target_pos + Vec3::Y * 0.65;
        transform.look_at(aim, Vec3::Y);

        if turret.cooldown_timer > 0.0 {
            continue;
        }

        let muzzle = origin + transform.forward().as_vec3() * 1.25;
        if turret.locked_target != Some(target_entity) {
            turret.locked_target = Some(target_entity);
            turret.windup_timer = turret.windup;
            spawn_beam_vfx(
                &mut commands,
                &mut meshes,
                turret.beam_material.clone(),
                muzzle,
                aim,
                0.035,
                turret.windup,
            );
            continue;
        }

        if turret.windup_timer > 0.0 {
            continue;
        }

        turret.cooldown_timer = turret.cooldown;
        turret.locked_target = None;
        spawn_beam_vfx(
            &mut commands,
            &mut meshes,
            turret.beam_material.clone(),
            muzzle,
            aim,
            0.08,
            0.16,
        );

        if target_distance <= turret.range {
            if let Ok((mut health, mut damageable, mut stats, mut parry, armor)) =
                player_damage_q.get_mut(target_entity)
            {
                crate::plugins::player_plugin::damage_player(
                    Some(target_index),
                    &mut health,
                    &mut damageable,
                    &mut stats,
                    &mut parry,
                    armor,
                    &DamageInfo::new(turret.damage, DamageType::Laser)
                        .with_knockback(2.2)
                        .with_hit_direction((aim - muzzle).normalize_or_zero()),
                    &mut damaged_ev,
                    &mut parry_ev,
                );
            }
        }
    }
}

fn laser_beam_cleanup_system(
    mut commands: Commands,
    time: Res<Time>,
    mut beam_q: Query<(Entity, &mut LaserBeamVfx)>,
) {
    let dt = time.delta_secs();
    for (entity, mut beam) in beam_q.iter_mut() {
        beam.timer -= dt;
        if beam.timer <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn closest_player<F: bevy::ecs::query::QueryFilter>(
    origin: Vec3,
    max_range: f32,
    player_q: &Query<(Entity, &Transform, &PlayerIndex), F>,
) -> Option<(Entity, Vec3, u8, f32)> {
    player_q
        .iter()
        .filter_map(|(entity, transform, index)| {
            let dist = origin.distance(transform.translation);
            (dist <= max_range).then_some((entity, transform.translation, index.0, dist))
        })
        .min_by(|a, b| a.3.total_cmp(&b.3))
}

fn spawn_beam_vfx(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    radius: f32,
    timer: f32,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.05 {
        return;
    }
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(radius, length))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(start + delta * 0.5)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, delta.normalize())),
            ..default()
        },
        WorldGeometry,
        LaserBeamVfx { timer },
    ));
}

fn cleanup_world(
    mut commands: Commands,
    transition: Res<PlaySessionTransition>,
    world_q: Query<Entity, (With<WorldGeometry>, Without<ChildOf>)>,
    tree_q: Query<Entity, (With<TreeRoot>, Without<ChildOf>, Without<WorldGeometry>)>,
) {
    if transition.pausing {
        return;
    }

    for e in world_q.iter() {
        commands.entity(e).despawn();
    }
    for e in tree_q.iter() {
        commands.entity(e).despawn();
    }
}

fn cleanup_world_for_menu(
    mut commands: Commands,
    world_q: Query<Entity, (With<WorldGeometry>, Without<ChildOf>)>,
    tree_q: Query<Entity, (With<TreeRoot>, Without<ChildOf>, Without<WorldGeometry>)>,
) {
    for e in world_q.iter() {
        commands.entity(e).despawn();
    }
    for e in tree_q.iter() {
        commands.entity(e).despawn();
    }
}

// ── World generation entry point ──────────────────────────────────────────────
fn generate_city(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut grass_mats: ResMut<Assets<GrassMaterial>>,
    mut grass_field: ResMut<GrassField>,
    mut terrain_mats: ResMut<Assets<TerrainMaterial>>,
    mut water_mats: ResMut<Assets<WaterMaterial>>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    asset_server: Res<AssetServer>,
    // Grouped: Bevy caps a system at 16 parameters.
    session: (
        Res<PlaySessionTransition>,
        Res<PlayExperience>,
        Res<PlatformerWorkshopProgress>,
    ),
    settings: Res<GameSettings>,
    current: Res<CurrentChapter>,
    economy: Res<SettlementEconomy>,
    existing_world: Query<Entity, With<WorldGeometry>>,
    mut world_site_registry: ResMut<WorldSiteRegistry>,
    mut world_route_registry: ResMut<WorldRouteRegistry>,
) {
    if session.0.resuming_from_pause || !existing_world.is_empty() {
        return;
    }

    if *session.1 == PlayExperience::SharedPlatformer {
        // Chunk-composed routes are the authored levels; the hand-built
        // beginner course remains the fallback so a route that fails
        // validation never leaves the party in an empty world.
        let route_index = platformer_route_index(&session.2);
        match crate::world::platformer_route_spawn::spawn_route_by_index(
            &mut commands,
            &mut meshes,
            &mut mats,
            route_index,
            crate::world::co_op_platformer::platformer_spawn(0) - Vec3::Y * 1.0,
        ) {
            Ok(built) => {
                // The session contract is "exactly one stage root", whichever
                // builder produced the level, so route-built levels register
                // one too and teardown/queries keep working unchanged.
                commands.spawn((
                    Name::new(built.label),
                    crate::world::co_op_platformer::PlatformerStageRoot,
                    WorldGeometry,
                ));
                let summary = crate::world::platformer_routes::route_by_id(built.id)
                    .map(|route| route.summary())
                    .unwrap_or_else(|| built.label.to_string());
                info!("shared-screen level ready: {summary} — {}", built.brief);
            }
            Err(error) => {
                warn!("route {route_index} unavailable ({error}); using the starter course");
                spawn_shared_platformer_stage(&mut commands, &mut meshes, &mut mats);
            }
        }
        return;
    }

    let load_started = std::time::Instant::now();
    let initial_mesh_count = meshes.len();
    let initial_standard_material_count = mats.len();

    let seed = settings.world_seed;
    let _ = ROAD_NETWORK_SEED.set(seed);
    let m = &mut *mats;

    let pal = Palette::build(m, &asset_server);
    commands.insert_resource(CityRooftopRouteAssets {
        bridge: pal.city_aqua.clone(),
        guard: pal.city_sun.clone(),
        zipline: pal.city_coral.clone(),
        playground: pal.city_lilac.clone(),
        greenery: pal.city_mint.clone(),
        bridge_marker: meshes.add(Torus::new(0.86, 1.08)),
        zipline_marker: meshes.add(Cuboid::new(1.5, 1.5, 0.18)),
        planter_mesh: meshes.add(Cylinder::new(0.82, 0.62)),
        flower_mesh: meshes.add(Sphere::new(0.34)),
        spring_mesh: meshes.add(Cylinder::new(1.65, 0.58)),
        spring_arrow_mesh: meshes.add(Cuboid::new(0.52, 0.16, 2.35)),
        trial_gate_mesh: meshes.add(Torus::new(1.62, 2.0)),
    });
    let terrain_material = terrain_mats.add(TerrainMaterial {
        settings: TerrainMaterialUniform::default(),
        grass_texture: Some(repeating_texture(&asset_server, "Materials/grass (1).png")),
        rock_texture: Some(repeating_texture(&asset_server, "Materials/stone (1).png")),
        snow_texture: Some(repeating_texture(
            &asset_server,
            "Materials/rock_snow (1).png",
        )),
        path_texture: Some(repeating_texture(&asset_server, "Materials/asphalt.png")),
    });
    let waterfall_fx = WaterfallFxAssets {
        mist_mesh: meshes.add(Sphere::new(1.0)),
        droplet_mesh: meshes.add(Sphere::new(0.34)),
        mist_material: pal.waterfall_mist.clone(),
        audio: audio_sources.add(AudioSource {
            bytes: crate::audio::synth::render_wav(
                &crate::audio::synth::preset_waterfall_ambience(),
            )
            .into(),
        }),
        volume: settings.sfx_volume.clamp(0.0, 1.0) * 0.24,
    };
    commands.insert_resource(waterfall_fx.clone());
    commands.insert_resource(WaterWakeAssets {
        mesh: meshes.add(Torus::new(0.72, 1.0)),
        material: pal.waterfall_mist.clone(),
    });
    let ocean_water = water_mats.add(WaterMaterial {
        settings: WaterMaterialUniform::default(),
    });
    let river_water = water_mats.add(WaterMaterial {
        settings: WaterMaterialUniform::river(),
    });
    let lake_water = water_mats.add(WaterMaterial {
        settings: WaterMaterialUniform::lake(),
    });

    spawn_lighting(&mut commands, &mut meshes, m);
    spawn_ground_plane(&mut commands);
    spawn_terrain(&mut commands, &mut meshes, &terrain_material, seed);
    let base_mesh_count = meshes.len();
    spawn_downtown(&mut commands, &mut meshes, &pal, seed);
    spawn_industrial(&mut commands, &mut meshes, &pal, seed + 1, seed);
    spawn_residential(&mut commands, &mut meshes, &pal, seed + 2, seed);
    spawn_highways(&mut commands, &mut meshes, &pal);
    spawn_city_streets(&mut commands, &mut meshes, &pal);
    spawn_city_life(&mut commands, &mut meshes, &pal, seed + 23);
    spawn_city_hidden_rooms(&mut commands, &mut meshes, m, &pal);
    spawn_traversal_courses(&mut commands, &mut meshes, m, &pal);
    let city_mesh_count = meshes.len();
    spawn_sky_platforms(&mut commands, &mut meshes, &pal, seed + 3);
    spawn_sky_bridges(&mut commands, &mut meshes, &pal, seed + 4);
    spawn_moving_platforms(&mut commands, &mut meshes, &pal, seed + 15, seed);
    spawn_spaceports(&mut commands, &mut meshes, &pal, seed);
    spawn_laser_turrets(&mut commands, &mut meshes, &pal, seed + 16, seed);
    let traversal_mesh_count = meshes.len();
    spawn_mountains(&mut commands, &mut meshes, &pal, seed + 5);
    let mountain_mesh_count = meshes.len();
    spawn_everest_range_biomes(&mut commands, &mut meshes, &pal, seed);
    let range_mesh_count = meshes.len();
    spawn_mountain_spider_mechs(&mut commands, &mut meshes, &pal, seed);
    spawn_perimeter_ocean_and_islands(&mut commands, &mut meshes, &pal, &ocean_water, seed);
    spawn_mountain_water_network(
        &mut commands,
        &mut meshes,
        &pal,
        &lake_water,
        &river_water,
        &waterfall_fx,
        seed,
    );
    if current.id.0 == 1 {
        spawn_chapter_one_ocean_island(&mut commands, &mut meshes, &pal, &ocean_water);
    }
    // Grass is streamed around the camera rather than spawned up front; this
    // hands the streamer the terrain seed and clears the previous world's chunks.
    arm_grass_field(&mut grass_field, &mut grass_mats, seed);
    spawn_neon_lights(&mut commands, seed + 6);
    spawn_street_lights(&mut commands, seed + 7);
    spawn_outer_districts(&mut commands, &mut meshes, &pal, seed + 8, seed);
    spawn_river(&mut commands, &mut meshes, &pal, &river_water);
    spawn_water_gardens(&mut commands, &mut meshes, &pal, seed + 12, seed);
    spawn_mana_waterfalls(
        &mut commands,
        &mut meshes,
        &pal,
        &waterfall_fx,
        seed + 18,
        seed,
    );
    spawn_mana_forests(&mut commands, &mut meshes, &pal, seed + 19, seed);
    spawn_bio_city_tamborn(&mut commands, &mut meshes, &pal, seed + 20, seed);
    spawn_rock_fields(&mut commands, &mut meshes, &pal, seed + 13, seed);
    spawn_understory(&mut commands, &mut meshes, &pal, seed + 14, seed);
    spawn_trees(&mut commands, &mut meshes, &pal, seed + 9, seed);
    let nature_mesh_count = meshes.len();
    spawn_aurora_castle(&mut commands, &mut meshes, &pal, seed);
    spawn_collosar_castle(&mut commands, &mut meshes, &pal, seed);
    spawn_magic_crystals(&mut commands, &mut meshes, &pal, seed);
    spawn_secret_cave_systems(&mut commands, &mut meshes, &pal, seed);
    spawn_dragon_lair_dungeons(&mut commands, &mut meshes, m, &pal, seed);
    spawn_great_scientist_temples(&mut commands, &mut meshes, m, &pal, seed);
    // Concrete build-from example: isolated top-down arcade stage near spawn.
    spawn_arcade_prototype_dungeon(&mut commands, &mut meshes, m);
    spawn_exploration_settlements(&mut commands, &mut meshes, m, &pal, seed, &economy);
    spawn_chapter_map_locations(&mut commands, &mut meshes, &pal, seed);
    spawn_puzzle_anchors(&mut commands, seed);
    setup_world_site_registry(&mut world_site_registry);
    spawn_world_site_props(&mut commands, &mut meshes, m, &world_site_registry, seed);
    spawn_wasteland_drone_patrol_anchors(&mut commands, seed);
    setup_world_route_registry(&mut world_route_registry);

    let diagnostics = WorldLoadDiagnostics {
        generation_seconds: load_started.elapsed().as_secs_f32(),
        meshes_added: meshes.len().saturating_sub(initial_mesh_count),
        standard_materials_added: m.len().saturating_sub(initial_standard_material_count),
    };
    info!(
        "World generation queued in {:.2}s ({} meshes: base {}, city {}, traversal {}, mountains {}, range {}, nature {}, landmarks {}; {} standard materials)",
        diagnostics.generation_seconds,
        diagnostics.meshes_added,
        base_mesh_count.saturating_sub(initial_mesh_count),
        city_mesh_count.saturating_sub(base_mesh_count),
        traversal_mesh_count.saturating_sub(city_mesh_count),
        mountain_mesh_count.saturating_sub(traversal_mesh_count),
        range_mesh_count.saturating_sub(mountain_mesh_count),
        nature_mesh_count.saturating_sub(range_mesh_count),
        meshes.len().saturating_sub(nature_mesh_count),
        diagnostics.standard_materials_added,
    );
    commands.insert_resource(diagnostics);
}

fn water_wake_spawn_system(
    mut commands: Commands,
    assets: Option<Res<WaterWakeAssets>>,
    mut player_q: Query<(&Transform, &mut WaterTraversalState), With<Player>>,
) {
    let Some(assets) = assets else {
        return;
    };
    for (transform, mut water) in player_q.iter_mut() {
        if !water.wake_requested {
            continue;
        }
        water.wake_requested = false;
        if !water.swimming {
            continue;
        }

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(assets.mesh.clone()),
                material: MeshMaterial3d(assets.material.clone()),
                transform: Transform::from_xyz(
                    transform.translation.x,
                    water.surface_y + 0.06,
                    transform.translation.z,
                )
                .with_scale(Vec3::new(0.8, 0.12, 0.8)),
                ..default()
            },
            WorldGeometry,
            WaterWakeRipple {
                remaining: 0.62,
                duration: 0.62,
            },
        ));
    }
}

fn water_wake_update_system(
    mut commands: Commands,
    time: Res<Time>,
    mut wake_q: Query<(Entity, &mut Transform, &mut WaterWakeRipple)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut ripple) in wake_q.iter_mut() {
        ripple.remaining -= dt;
        if ripple.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let life = 1.0 - ripple.remaining / ripple.duration;
        let radius = 0.8 + life * 2.4;
        transform.scale = Vec3::new(radius, 0.12, radius);
    }
}

fn animate_waterfall_flow_system(
    time: Res<Time>,
    mut ribbon_q: Query<(&WaterfallFlowRibbon, &mut Transform)>,
) {
    let elapsed = time.elapsed_secs();
    for (ribbon, mut transform) in ribbon_q.iter_mut() {
        let flow = (elapsed * 0.72 + ribbon.phase).fract();
        let pulse = (elapsed * 4.6 + ribbon.phase * TAU).sin();
        transform.translation = ribbon.base_translation + Vec3::Y * (-flow * 1.6 + pulse * 0.16);
        transform.scale = Vec3::new(
            ribbon.base_scale.x * (1.0 + pulse * 0.025),
            ribbon.base_scale.y * (1.0 + pulse * 0.018),
            ribbon.base_scale.z * (1.0 - pulse * 0.06),
        );
    }
}

fn waterfall_particle_system(
    mut commands: Commands,
    time: Res<Time>,
    mut particle_q: Query<(Entity, &mut Transform, &mut WaterfallParticle)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut particle) in particle_q.iter_mut() {
        particle.age += dt;
        if particle.age >= particle.lifetime {
            if particle.looped {
                particle.age %= particle.lifetime;
            } else {
                commands.entity(entity).despawn();
                continue;
            }
        }

        let t = particle.age;
        let life = (t / particle.lifetime).clamp(0.0, 1.0);
        let swirl = Vec3::new(
            (life * TAU * 1.4 + particle.phase).sin(),
            0.0,
            (life * TAU * 1.1 + particle.phase).cos(),
        ) * (0.16 + life * 0.42);
        transform.translation = particle.origin
            + particle.velocity * t
            + Vec3::Y * (0.5 * particle.gravity * t * t)
            + swirl;
        let fade = if particle.looped {
            (1.0 - life).max(0.12)
        } else {
            (1.0 - life).max(0.0)
        };
        transform.scale = particle.base_scale * fade;
    }
}

fn waterfall_splash_zone_system(
    mut commands: Commands,
    assets: Option<Res<WaterfallFxAssets>>,
    mut zone_q: Query<(&Transform, &mut WaterfallSplashZone)>,
    player_q: Query<(&Transform, &PlayerIndex), With<Player>>,
    mut action_sfx: MessageWriter<ModularActionSfxEvent>,
) {
    let Some(assets) = assets else {
        return;
    };
    for (zone_transform, mut zone) in zone_q.iter_mut() {
        let mut occupied = 0u8;
        for (player_transform, index) in player_q.iter() {
            let bit = 1u8 << index.0.min(7);
            if !waterfall_zone_contains(
                player_transform.translation - zone_transform.translation,
                zone.radius,
                zone.height,
            ) {
                continue;
            }
            occupied |= bit;
            if zone.occupied_players & bit != 0 {
                continue;
            }

            action_sfx.write(ModularActionSfxEvent::new("waterfall.splash"));
            for burst in 0..6u8 {
                let angle = burst as f32 / 6.0 * TAU + index.0 as f32 * 0.71;
                let velocity = Vec3::new(
                    angle.cos() * 3.2,
                    4.8 + burst as f32 * 0.18,
                    angle.sin() * 3.2,
                );
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(assets.droplet_mesh.clone()),
                        material: MeshMaterial3d(assets.mist_material.clone()),
                        transform: Transform::from_translation(
                            player_transform.translation + Vec3::Y * 0.35,
                        ),
                        ..default()
                    },
                    WorldGeometry,
                    WaterfallParticle {
                        origin: player_transform.translation + Vec3::Y * 0.35,
                        velocity,
                        base_scale: Vec3::splat(0.75),
                        age: 0.0,
                        lifetime: 0.72,
                        gravity: -7.5,
                        looped: false,
                        phase: angle,
                    },
                ));
            }
        }
        zone.occupied_players = occupied;
    }
}

fn waterfall_zone_contains(local: Vec3, radius: f32, height: f32) -> bool {
    local.y >= -1.5 && local.y <= height && local.x * local.x + local.z * local.z <= radius * radius
}

fn spawn_waterfall_fx(
    commands: &mut Commands,
    assets: &WaterfallFxAssets,
    base: Vec3,
    forward: Vec3,
    right: Vec3,
    width: f32,
    drop: f32,
    seed: u64,
) {
    let volume = assets.volume * (0.68 + (drop / 260.0).clamp(0.0, 1.0) * 0.32);
    commands.spawn((
        Transform::from_translation(base + forward * 4.0 + Vec3::Y * 2.0),
        GlobalTransform::default(),
        AudioPlayer::new(assets.audio.clone()),
        PlaybackSettings::LOOP
            .with_volume(Volume::Linear(volume))
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(0.018)),
        WaterfallSplashZone {
            radius: (width * 0.62).max(10.0),
            height: (drop * 0.24).clamp(12.0, 42.0),
            occupied_players: 0,
        },
        WorldGeometry,
    ));

    for particle_index in 0..14u64 {
        let lateral = (seeded(seed, particle_index * 7) - 0.5) * width * 0.92;
        let forward_offset = seeded(seed, particle_index * 7 + 1) * width * 0.34;
        let scale = width * (0.025 + seeded(seed, particle_index * 7 + 2) * 0.035);
        let lifetime = 1.4 + seeded(seed, particle_index * 7 + 3) * 1.5;
        let velocity = forward * (2.2 + seeded(seed, particle_index * 7 + 4) * 4.2)
            + right * ((seeded(seed, particle_index * 7 + 5) - 0.5) * 1.8)
            + Vec3::Y * (1.8 + seeded(seed, particle_index * 7 + 6) * 3.8);
        let origin = base + right * lateral + forward * (2.0 + forward_offset) + Vec3::Y * 0.8;
        let age = seeded(seed + 91, particle_index) * lifetime;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(if particle_index % 3 == 0 {
                    assets.mist_mesh.clone()
                } else {
                    assets.droplet_mesh.clone()
                }),
                material: MeshMaterial3d(assets.mist_material.clone()),
                transform: Transform::from_translation(origin).with_scale(Vec3::new(
                    scale * 1.5,
                    scale * 0.58,
                    scale,
                )),
                ..default()
            },
            WorldGeometry,
            WaterfallParticle {
                origin,
                velocity,
                base_scale: Vec3::new(scale * 1.5, scale * 0.58, scale),
                age,
                lifetime,
                gravity: -1.25,
                looped: true,
                phase: seeded(seed + 117, particle_index) * TAU,
            },
        ));
    }
}

// ── Seeded RNG helper ─────────────────────────────────────────────────────────
fn seeded(seed: u64, index: u64) -> f32 {
    let x = ((seed.wrapping_mul(127) + index.wrapping_mul(311)).wrapping_mul(43758)) as f64;
    let frac = (x * 0.0000001) - (x * 0.0000001).floor();
    frac as f32
}

fn starter_clear_zone(x: f32, z: f32, padding: f32) -> bool {
    let origin_clear = Vec2::new(x - 10.0, z - 10.0).length() <= 52.0 + padding;
    let ch1_lab_clear = Vec2::new(x - 26.0, z - 28.0).length() <= 34.0 + padding;
    origin_clear || ch1_lab_clear
}

// ── Shared material palette ───────────────────────────────────────────────────
struct Palette {
    #[allow(dead_code)]
    ground: Handle<StandardMaterial>,

    downtown_a: Handle<StandardMaterial>,
    downtown_b: Handle<StandardMaterial>,
    downtown_c: Handle<StandardMaterial>,
    downtown_facade: Handle<StandardMaterial>,
    glass_panel: Handle<StandardMaterial>,
    brushed_metal: Handle<StandardMaterial>,
    stainless_metal: Handle<StandardMaterial>,
    copper_metal: Handle<StandardMaterial>,
    city_metal_panel: Handle<StandardMaterial>,
    stone_brick: Handle<StandardMaterial>,
    mortar_line: Handle<StandardMaterial>,
    interior_wall: Handle<StandardMaterial>,
    interior_floor: Handle<StandardMaterial>,
    interior_trim: Handle<StandardMaterial>,

    industrial_metal: Handle<StandardMaterial>,
    industrial_rust: Handle<StandardMaterial>,

    residential_a: Handle<StandardMaterial>,
    residential_b: Handle<StandardMaterial>,
    city_coral: Handle<StandardMaterial>,
    city_sun: Handle<StandardMaterial>,
    city_mint: Handle<StandardMaterial>,
    city_lilac: Handle<StandardMaterial>,
    city_aqua: Handle<StandardMaterial>,
    brick_line: Handle<StandardMaterial>,
    stone_block: Handle<StandardMaterial>,
    small_window_warm: Handle<StandardMaterial>,
    small_window_cool: Handle<StandardMaterial>,
    small_window_dark: Handle<StandardMaterial>,

    street_asphalt: Handle<StandardMaterial>,
    street_paint: Handle<StandardMaterial>,

    mountain_path: Handle<StandardMaterial>,
    bridge_deck: Handle<StandardMaterial>,
    bridge_stone: Handle<StandardMaterial>,
    highway: Handle<StandardMaterial>,
    boost_lane: Handle<StandardMaterial>,
    boost_pad: Handle<StandardMaterial>,
    boost_ramp: Handle<StandardMaterial>,
    sky_platform: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    waterfall_mist: Handle<StandardMaterial>,

    rock: Handle<StandardMaterial>,
    rock_dark: Handle<StandardMaterial>,
    snow: Handle<StandardMaterial>,
    moss: Handle<StandardMaterial>,
    vine: Handle<StandardMaterial>,
    bark_light: Handle<StandardMaterial>,
    bark_mid: Handle<StandardMaterial>,
    bark_dark: Handle<StandardMaterial>,
    foliage_a: Handle<StandardMaterial>,
    foliage_b: Handle<StandardMaterial>,
    foliage_c: Handle<StandardMaterial>,
    reed: Handle<StandardMaterial>,
    flower_a: Handle<StandardMaterial>,
    flower_b: Handle<StandardMaterial>,

    /// Warm glowing window panels — warm office light (yellows/amber).
    window_warm: Handle<StandardMaterial>,
    /// Cool glowing window panels — blue-white corporate light.
    window_cool: Handle<StandardMaterial>,
    /// Rooftop antenna / water-tank material.
    rooftop: Handle<StandardMaterial>,

    castle_stone: Handle<StandardMaterial>, // Aurora castle — warm cream/lavender stone
    castle_trim: Handle<StandardMaterial>,  // Gold trim
    castle_roof: Handle<StandardMaterial>,  // Deep purple roof cones
    castle_window: Handle<StandardMaterial>, // Aurora magic glow windows (purple-cyan emissive)
    dragon_stone: Handle<StandardMaterial>, // Collosar — near-black fortress stone
    dragon_lava: Handle<StandardMaterial>,  // Lava moat (bright orange emissive)
    dragon_window: Handle<StandardMaterial>, // Red fire glow windows
    crystal_aurora: Handle<StandardMaterial>, // Glowing purple-blue crystals (alpha blend)
    crystal_dragon: Handle<StandardMaterial>, // Glowing orange-red crystals (alpha blend)
    crystal_emerald: Handle<StandardMaterial>, // Nature/cave emerald crystal
    marble: Handle<StandardMaterial>,
    guide_glow: Handle<StandardMaterial>, // Cyan route studs for bridges and mountain paths
    aurora_banner: Handle<StandardMaterial>, // Violet banner
    dragon_banner: Handle<StandardMaterial>, // Dark crimson banner
}

fn repeating_texture(asset_server: &AssetServer, path: &'static str) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            *settings = ImageLoaderSettings {
                sampler: ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..default()
                }),
                ..default()
            };
        })
        .load(path)
}

impl Palette {
    fn build(m: &mut Assets<StandardMaterial>, asset_server: &AssetServer) -> Self {
        let mk =
            |base: Color, emissive: LinearRgba, metallic: f32, roughness: f32| StandardMaterial {
                base_color: base,
                emissive,
                metallic,
                perceptual_roughness: roughness,
                reflectance: metallic * 0.8 + 0.1,
                ..default()
            };

        Self {
            ground: m.add(StandardMaterial {
                base_color: Color::srgb(0.07, 0.07, 0.09),
                metallic: 0.55,
                perceptual_roughness: 0.50,
                reflectance: 0.4,
                ..default()
            }),

            // Downtown tower bodies: cel-clean glass blocks with vintage anime
            // cyan/amber/mint tints so the skyline reads as playful, not realistic.
            downtown_a: m.add(StandardMaterial {
                base_color: Color::srgba(0.42, 0.72, 0.96, 0.72),
                emissive: LinearRgba::new(0.15, 0.45, 1.10, 1.0),
                metallic: 0.08,
                perceptual_roughness: 0.05,
                reflectance: 0.96,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            downtown_b: m.add(StandardMaterial {
                base_color: Color::srgba(0.96, 0.76, 0.52, 0.70),
                emissive: LinearRgba::new(1.10, 0.40, 0.10, 1.0),
                metallic: 0.05,
                perceptual_roughness: 0.06,
                reflectance: 0.94,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            downtown_c: m.add(StandardMaterial {
                base_color: Color::srgba(0.55, 0.86, 0.76, 0.70),
                emissive: LinearRgba::new(0.18, 0.75, 0.45, 1.0),
                metallic: 0.06,
                perceptual_roughness: 0.06,
                reflectance: 0.95,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            downtown_facade: m.add(StandardMaterial {
                base_color: Color::srgb(0.57, 0.34, 0.88),
                emissive: LinearRgba::new(0.24, 0.07, 0.54, 1.0),
                metallic: 0.42,
                perceptual_roughness: 0.38,
                reflectance: 0.62,
                ..default()
            }),
            glass_panel: m.add(StandardMaterial {
                base_color: Color::srgba(0.44, 0.86, 1.0, 0.48),
                emissive: LinearRgba::new(0.18, 0.55, 1.15, 1.0),
                metallic: 0.04,
                perceptual_roughness: 0.02,
                reflectance: 0.98,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            brushed_metal: m.add(StandardMaterial {
                base_color: Color::srgb(0.62, 0.67, 0.72),
                base_color_texture: Some(repeating_texture(
                    asset_server,
                    "Materials/stainless.png",
                )),
                uv_transform: Affine2::from_scale(Vec2::splat(2.4)),
                emissive: LinearRgba::new(0.05, 0.07, 0.10, 1.0),
                metallic: 0.92,
                perceptual_roughness: 0.26,
                reflectance: 0.82,
                ..default()
            }),
            stainless_metal: m.add(StandardMaterial {
                base_color: Color::srgb(0.82, 0.86, 0.88),
                base_color_texture: Some(repeating_texture(
                    asset_server,
                    "Materials/stainless.png",
                )),
                uv_transform: Affine2::from_scale(Vec2::splat(3.0)),
                emissive: LinearRgba::new(0.08, 0.09, 0.11, 1.0),
                metallic: 0.96,
                perceptual_roughness: 0.18,
                reflectance: 0.92,
                ..default()
            }),
            copper_metal: m.add(StandardMaterial {
                base_color: Color::srgb(0.95, 0.46, 0.20),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/Copper.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(2.6)),
                emissive: LinearRgba::new(0.18, 0.07, 0.02, 1.0),
                metallic: 0.88,
                perceptual_roughness: 0.30,
                reflectance: 0.84,
                ..default()
            }),
            city_metal_panel: m.add(StandardMaterial {
                base_color: Color::srgb(0.46, 0.49, 0.52),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/metal.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(4.0)),
                emissive: LinearRgba::new(0.05, 0.06, 0.08, 1.0),
                metallic: 0.86,
                perceptual_roughness: 0.34,
                reflectance: 0.74,
                ..default()
            }),
            stone_brick: m.add(StandardMaterial {
                base_color: Color::srgb(0.58, 0.55, 0.50),
                emissive: LinearRgba::new(0.04, 0.035, 0.03, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.98,
                reflectance: 0.12,
                ..default()
            }),
            mortar_line: m.add(StandardMaterial {
                base_color: Color::srgb(0.22, 0.21, 0.20),
                emissive: LinearRgba::new(0.015, 0.012, 0.010, 1.0),
                metallic: 0.0,
                perceptual_roughness: 1.0,
                reflectance: 0.06,
                ..default()
            }),
            interior_wall: m.add(StandardMaterial {
                base_color: Color::srgb(0.72, 0.69, 0.63),
                base_color_texture: Some(repeating_texture(
                    asset_server,
                    "Materials/stone (1).png",
                )),
                uv_transform: Affine2::from_scale(Vec2::splat(3.5)),
                perceptual_roughness: 0.92,
                reflectance: 0.12,
                ..default()
            }),
            interior_floor: m.add(StandardMaterial {
                base_color: Color::srgb(0.48, 0.32, 0.20),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/wood.jpg")),
                uv_transform: Affine2::from_scale(Vec2::splat(4.0)),
                perceptual_roughness: 0.78,
                reflectance: 0.18,
                ..default()
            }),
            interior_trim: m.add(StandardMaterial {
                base_color: Color::srgb(0.58, 0.31, 0.16),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/wood.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(2.8)),
                perceptual_roughness: 0.70,
                reflectance: 0.20,
                ..default()
            }),

            industrial_metal: m.add(StandardMaterial {
                base_color: Color::srgb(0.20, 0.78, 0.84),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/metal.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(3.4)),
                emissive: LinearRgba::new(0.04, 0.32, 0.42, 1.0),
                metallic: 0.64,
                perceptual_roughness: 0.36,
                reflectance: 0.72,
                ..default()
            }),
            industrial_rust: m.add(StandardMaterial {
                base_color: Color::srgb(0.98, 0.54, 0.18),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/Copper.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(3.0)),
                emissive: LinearRgba::new(0.38, 0.12, 0.02, 1.0),
                metallic: 0.42,
                perceptual_roughness: 0.74,
                reflectance: 0.46,
                ..default()
            }),

            // Smaller buildings: painterly brick and cinder block with old-anime warmth.
            residential_a: m.add(StandardMaterial {
                base_color: Color::srgb(0.98, 0.48, 0.58),
                emissive: LinearRgba::new(0.34, 0.06, 0.10, 1.0),
                metallic: 0.02,
                perceptual_roughness: 0.82,
                reflectance: 0.26,
                ..default()
            }),
            residential_b: m.add(StandardMaterial {
                base_color: Color::srgb(0.68, 0.82, 0.96),
                emissive: LinearRgba::new(0.08, 0.16, 0.28, 1.0),
                metallic: 0.01,
                perceptual_roughness: 0.82,
                reflectance: 0.28,
                ..default()
            }),
            city_coral: m.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.31, 0.48),
                emissive: LinearRgba::new(0.62, 0.06, 0.12, 1.0),
                perceptual_roughness: 0.62,
                ..default()
            }),
            city_sun: m.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.82, 0.16),
                emissive: LinearRgba::new(0.75, 0.38, 0.025, 1.0),
                perceptual_roughness: 0.58,
                ..default()
            }),
            city_mint: m.add(StandardMaterial {
                base_color: Color::srgb(0.24, 0.95, 0.63),
                emissive: LinearRgba::new(0.06, 0.62, 0.24, 1.0),
                perceptual_roughness: 0.56,
                ..default()
            }),
            city_lilac: m.add(StandardMaterial {
                base_color: Color::srgb(0.72, 0.42, 1.0),
                emissive: LinearRgba::new(0.34, 0.08, 0.72, 1.0),
                perceptual_roughness: 0.54,
                ..default()
            }),
            city_aqua: m.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.87, 1.0),
                emissive: LinearRgba::new(0.03, 0.58, 0.84, 1.0),
                perceptual_roughness: 0.50,
                ..default()
            }),
            brick_line: m.add(StandardMaterial {
                base_color: Color::srgb(0.34, 0.24, 0.20),
                emissive: LinearRgba::new(0.03, 0.02, 0.01, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.98,
                reflectance: 0.08,
                ..default()
            }),
            stone_block: m.add(StandardMaterial {
                base_color: Color::srgb(0.50, 0.50, 0.48),
                emissive: LinearRgba::new(0.03, 0.03, 0.03, 1.0),
                metallic: 0.02,
                perceptual_roughness: 0.96,
                reflectance: 0.14,
                ..default()
            }),
            small_window_warm: m.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.80, 0.40, 0.82),
                emissive: LinearRgba::new(2.3, 1.3, 0.35, 1.0),
                metallic: 0.05,
                perceptual_roughness: 0.08,
                reflectance: 0.78,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            small_window_cool: m.add(StandardMaterial {
                base_color: Color::srgba(0.42, 0.92, 1.0, 0.78),
                emissive: LinearRgba::new(0.35, 1.8, 2.6, 1.0),
                metallic: 0.05,
                perceptual_roughness: 0.08,
                reflectance: 0.80,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            small_window_dark: m.add(StandardMaterial {
                base_color: Color::srgba(0.05, 0.07, 0.10, 0.90),
                emissive: LinearRgba::new(0.01, 0.02, 0.04, 1.0),
                metallic: 0.02,
                perceptual_roughness: 0.18,
                reflectance: 0.40,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),

            street_asphalt: m.add(StandardMaterial {
                base_color: Color::srgb(0.16, 0.16, 0.19),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/asphalt.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(8.0)),
                emissive: LinearRgba::new(0.03, 0.03, 0.05, 1.0),
                metallic: 0.08,
                perceptual_roughness: 0.88,
                reflectance: 0.22,
                ..default()
            }),
            street_paint: m.add(StandardMaterial {
                base_color: Color::srgb(0.98, 0.86, 0.32),
                emissive: LinearRgba::new(0.32, 0.26, 0.05, 1.0),
                metallic: 0.02,
                perceptual_roughness: 0.72,
                reflectance: 0.28,
                ..default()
            }),
            mountain_path: m.add(StandardMaterial {
                base_color: Color::srgb(0.58, 0.52, 0.44),
                base_color_texture: Some(repeating_texture(
                    asset_server,
                    "Materials/stone (1).png",
                )),
                uv_transform: Affine2::from_scale(Vec2::splat(3.6)),
                emissive: LinearRgba::new(0.035, 0.030, 0.020, 1.0),
                metallic: 0.01,
                perceptual_roughness: 0.98,
                reflectance: 0.12,
                ..default()
            }),
            bridge_deck: m.add(StandardMaterial {
                base_color: Color::srgb(0.92, 0.78, 0.52),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/wood.png")),
                uv_transform: Affine2::from_scale(Vec2::new(2.6, 9.0)),
                emissive: LinearRgba::new(0.06, 0.035, 0.012, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.86,
                reflectance: 0.18,
                ..default()
            }),
            bridge_stone: m.add(StandardMaterial {
                base_color: Color::srgb(0.68, 0.66, 0.62),
                base_color_texture: Some(repeating_texture(
                    asset_server,
                    "Materials/stone (1).png",
                )),
                uv_transform: Affine2::from_scale(Vec2::splat(2.4)),
                emissive: LinearRgba::new(0.035, 0.032, 0.028, 1.0),
                metallic: 0.02,
                perceptual_roughness: 0.94,
                reflectance: 0.16,
                ..default()
            }),
            highway: m.add(StandardMaterial {
                base_color: Color::srgb(0.10, 0.10, 0.13),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/asphalt.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(10.0)),
                metallic: 0.20,
                perceptual_roughness: 0.75,
                ..default()
            }),
            boost_lane: m.add(StandardMaterial {
                base_color: Color::srgba(0.02, 0.24, 0.34, 0.84),
                emissive: LinearRgba::new(0.03, 1.45, 2.10, 1.0),
                metallic: 0.50,
                perceptual_roughness: 0.18,
                reflectance: 0.82,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            boost_pad: m.add(StandardMaterial {
                base_color: Color::srgba(0.18, 0.98, 1.0, 0.92),
                emissive: LinearRgba::new(0.15, 4.2, 5.6, 1.0),
                metallic: 0.35,
                perceptual_roughness: 0.10,
                reflectance: 0.96,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            boost_ramp: m.add(StandardMaterial {
                base_color: Color::srgb(0.92, 0.58, 0.18),
                emissive: LinearRgba::new(2.8, 0.72, 0.04, 1.0),
                metallic: 0.38,
                perceptual_roughness: 0.22,
                reflectance: 0.72,
                ..default()
            }),
            sky_platform: m.add(mk(
                Color::srgb(0.08, 0.11, 0.22),
                LinearRgba::new(0.0, 0.25, 0.60, 1.0),
                0.65,
                0.30,
            )),

            water: m.add(StandardMaterial {
                base_color: Color::srgba(0.02, 0.18, 0.35, 0.82),
                emissive: LinearRgba::new(0.0, 0.10, 0.28, 1.0),
                metallic: 0.90,
                perceptual_roughness: 0.08,
                reflectance: 0.95,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            waterfall_mist: m.add(StandardMaterial {
                base_color: Color::srgba(0.72, 0.96, 1.0, 0.36),
                emissive: LinearRgba::new(0.30, 1.15, 1.35, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.18,
                reflectance: 0.72,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),

            rock: m.add(StandardMaterial {
                base_color: Color::srgb(0.58, 0.54, 0.48),
                base_color_texture: Some(repeating_texture(
                    asset_server,
                    "Materials/stone (1).png",
                )),
                uv_transform: Affine2::from_scale(Vec2::splat(2.7)),
                metallic: 0.05,
                perceptual_roughness: 0.95,
                ..default()
            }),
            rock_dark: m.add(StandardMaterial {
                base_color: Color::srgb(0.34, 0.32, 0.29),
                base_color_texture: Some(repeating_texture(
                    asset_server,
                    "Materials/stone (1).png",
                )),
                uv_transform: Affine2::from_scale(Vec2::splat(3.2)),
                metallic: 0.02,
                perceptual_roughness: 0.98,
                ..default()
            }),
            snow: m.add(StandardMaterial {
                base_color: Color::srgb(0.96, 0.98, 1.00),
                base_color_texture: Some(repeating_texture(
                    asset_server,
                    "Materials/rock_snow (1).png",
                )),
                uv_transform: Affine2::from_scale(Vec2::splat(2.6)),
                metallic: 0.0,
                perceptual_roughness: 0.80,
                ..default()
            }),
            moss: m.add(StandardMaterial {
                base_color: Color::srgb(0.28, 0.50, 0.18),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/moss.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(2.8)),
                emissive: LinearRgba::new(0.02, 0.08, 0.02, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.92,
                ..default()
            }),
            vine: m.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.42, 0.12),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/leaves.png")),
                uv_transform: Affine2::from_scale(Vec2::new(0.9, 3.4)),
                emissive: LinearRgba::new(0.012, 0.060, 0.010, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.94,
                reflectance: 0.10,
                ..default()
            }),
            bark_light: m.add(StandardMaterial {
                base_color: Color::srgb(0.68, 0.54, 0.39),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/bark.png")),
                uv_transform: Affine2::from_scale(Vec2::new(2.0, 5.2)),
                emissive: LinearRgba::new(0.035, 0.025, 0.015, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.96,
                reflectance: 0.12,
                ..default()
            }),
            bark_mid: m.add(StandardMaterial {
                base_color: Color::srgb(0.43, 0.31, 0.21),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/bark.png")),
                uv_transform: Affine2::from_scale(Vec2::new(2.4, 6.0)),
                emissive: LinearRgba::new(0.025, 0.018, 0.010, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.98,
                reflectance: 0.10,
                ..default()
            }),
            bark_dark: m.add(StandardMaterial {
                base_color: Color::srgb(0.24, 0.18, 0.13),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/bark.png")),
                uv_transform: Affine2::from_scale(Vec2::new(3.0, 6.8)),
                emissive: LinearRgba::new(0.015, 0.010, 0.006, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.99,
                reflectance: 0.08,
                ..default()
            }),
            foliage_a: m.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.62, 0.20),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/leaves.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(1.9)),
                emissive: LinearRgba::new(0.01, 0.08, 0.02, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.86,
                ..default()
            }),
            foliage_b: m.add(StandardMaterial {
                base_color: Color::srgb(0.30, 0.76, 0.24),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/leaves.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(2.2)),
                emissive: LinearRgba::new(0.02, 0.10, 0.02, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.84,
                ..default()
            }),
            foliage_c: m.add(StandardMaterial {
                base_color: Color::srgb(0.16, 0.44, 0.32),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/leaves.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(2.5)),
                emissive: LinearRgba::new(0.02, 0.08, 0.05, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.88,
                ..default()
            }),
            reed: m.add(StandardMaterial {
                base_color: Color::srgb(0.42, 0.62, 0.24),
                emissive: LinearRgba::new(0.04, 0.08, 0.02, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.80,
                ..default()
            }),
            flower_a: m.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.35, 0.70),
                emissive: LinearRgba::new(0.55, 0.08, 0.28, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.72,
                ..default()
            }),
            flower_b: m.add(StandardMaterial {
                base_color: Color::srgb(0.40, 0.90, 1.0),
                emissive: LinearRgba::new(0.08, 0.50, 0.70, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.70,
                ..default()
            }),

            window_warm: m.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.86, 0.58, 0.70),
                emissive: LinearRgba::new(3.2, 2.2, 0.8, 1.0),
                metallic: 0.10,
                perceptual_roughness: 0.05,
                reflectance: 0.96,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            window_cool: m.add(StandardMaterial {
                base_color: Color::srgba(0.58, 0.84, 1.0, 0.70),
                emissive: LinearRgba::new(0.7, 1.6, 3.6, 1.0),
                metallic: 0.10,
                perceptual_roughness: 0.05,
                reflectance: 0.96,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            rooftop: m.add(StandardMaterial {
                base_color: Color::srgb(0.50, 0.52, 0.54),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/metal.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(2.2)),
                metallic: 0.84,
                perceptual_roughness: 0.30,
                reflectance: 0.68,
                ..default()
            }),
            castle_stone: m.add(StandardMaterial {
                base_color: Color::srgb(0.93, 0.92, 0.96),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/marble.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(3.0)),
                emissive: LinearRgba::new(0.08, 0.07, 0.12, 1.0),
                perceptual_roughness: 0.72,
                ..default()
            }),
            castle_trim: m.add(StandardMaterial {
                base_color: Color::srgb(0.95, 0.80, 0.22),
                emissive: LinearRgba::new(0.6, 0.5, 0.05, 1.0),
                metallic: 0.7,
                perceptual_roughness: 0.3,
                ..default()
            }),
            castle_roof: m.add(StandardMaterial {
                base_color: Color::srgb(0.28, 0.10, 0.48),
                emissive: LinearRgba::new(0.12, 0.04, 0.22, 1.0),
                perceptual_roughness: 0.60,
                ..default()
            }),
            castle_window: m.add(StandardMaterial {
                base_color: Color::srgba(0.55, 0.35, 1.0, 0.85),
                emissive: LinearRgba::new(1.8, 0.6, 4.0, 1.0),
                perceptual_roughness: 0.10,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            dragon_stone: m.add(StandardMaterial {
                base_color: Color::srgb(0.11, 0.08, 0.08),
                emissive: LinearRgba::new(0.04, 0.02, 0.02, 1.0),
                perceptual_roughness: 0.90,
                ..default()
            }),
            dragon_lava: m.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.38, 0.05),
                emissive: LinearRgba::new(4.5, 1.2, 0.0, 1.0),
                perceptual_roughness: 0.20,
                ..default()
            }),
            dragon_window: m.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.25, 0.05, 0.80),
                emissive: LinearRgba::new(4.0, 0.6, 0.0, 1.0),
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            crystal_aurora: m.add(StandardMaterial {
                base_color: Color::srgba(0.55, 0.28, 1.0, 0.72),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/crystal.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(1.8)),
                emissive: LinearRgba::new(1.6, 0.4, 4.5, 1.0),
                perceptual_roughness: 0.05,
                metallic: 0.25,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            crystal_dragon: m.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.32, 0.08, 0.70),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/crystal.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(1.7)),
                emissive: LinearRgba::new(4.5, 0.9, 0.0, 1.0),
                perceptual_roughness: 0.05,
                metallic: 0.25,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            crystal_emerald: m.add(StandardMaterial {
                base_color: Color::srgba(0.20, 1.0, 0.56, 0.74),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/emerald.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(1.9)),
                emissive: LinearRgba::new(0.12, 3.2, 1.35, 1.0),
                perceptual_roughness: 0.06,
                metallic: 0.20,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            marble: m.add(StandardMaterial {
                base_color: Color::srgb(0.90, 0.90, 0.94),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/marble.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(2.6)),
                emissive: LinearRgba::new(0.035, 0.035, 0.045, 1.0),
                metallic: 0.02,
                perceptual_roughness: 0.50,
                reflectance: 0.36,
                ..default()
            }),
            guide_glow: m.add(StandardMaterial {
                base_color: Color::srgba(0.38, 0.96, 1.0, 0.82),
                emissive: LinearRgba::new(0.30, 2.4, 3.6, 1.0),
                metallic: 0.0,
                perceptual_roughness: 0.10,
                reflectance: 0.85,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            aurora_banner: m.add(StandardMaterial {
                base_color: Color::srgb(0.60, 0.15, 0.95),
                emissive: LinearRgba::new(0.35, 0.06, 0.60, 1.0),
                perceptual_roughness: 0.80,
                ..default()
            }),
            dragon_banner: m.add(StandardMaterial {
                base_color: Color::srgb(0.55, 0.04, 0.04),
                emissive: LinearRgba::new(0.30, 0.02, 0.02, 1.0),
                perceptual_roughness: 0.80,
                ..default()
            }),
        }
    }
}

// ── Lighting / Day-Night Cycle ───────────────────────────────────────────────
fn spawn_lighting(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    commands.insert_resource(ClearColor(Color::srgb(0.20, 0.42, 0.82)));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.30, 0.34, 0.52),
        brightness: 180.0,
        affects_lightmapped_meshes: true,
    });

    let sky_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.20, 0.42, 0.82),
        unlit: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(7_500.0))),
            material: MeshMaterial3d(sky_mat),
            transform: Transform::from_scale(Vec3::new(-1.0, 1.0, 1.0)),
            ..default()
        },
        SkyDome,
        WorldGeometry,
    ));

    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                color: Color::srgb(1.0, 0.92, 0.76),
                illuminance: 28_000.0,
                shadow_maps_enabled: true,
                ..default()
            },
            ..default()
        },
        DayNightSunLight,
        WorldGeometry,
    ));
    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                color: Color::srgb(0.62, 0.76, 1.0),
                illuminance: 8_000.0,
                shadow_maps_enabled: false,
                ..default()
            },
            ..default()
        },
        DayNightMoonLight,
        WorldGeometry,
    ));

    let sun_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.86, 0.34),
        emissive: LinearRgba::new(9.0, 6.4, 2.6, 1.0),
        unlit: true,
        ..default()
    });
    let moon_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.78, 0.88, 1.0),
        emissive: LinearRgba::new(2.6, 3.8, 6.5, 1.0),
        unlit: true,
        ..default()
    });
    let moon_companion_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.62, 0.75, 1.0),
        emissive: LinearRgba::new(1.4, 2.3, 4.4, 1.0),
        unlit: true,
        ..default()
    });

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(115.0))),
            material: MeshMaterial3d(sun_mat),
            ..default()
        },
        CelestialVisual {
            kind: CelestialKind::Sun,
            phase_offset: 0.0,
            distance: 5_200.0,
        },
        WorldGeometry,
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(210.0))),
            material: MeshMaterial3d(moon_mat),
            ..default()
        },
        CelestialVisual {
            kind: CelestialKind::Moon,
            phase_offset: 0.50,
            distance: 4_900.0,
        },
        WorldGeometry,
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(92.0))),
            material: MeshMaterial3d(moon_companion_mat),
            ..default()
        },
        CelestialVisual {
            kind: CelestialKind::Moon,
            phase_offset: 0.58,
            distance: 4_650.0,
        },
        WorldGeometry,
    ));

    // Soft cyan fill-light from below-left for the neon-city under-glow.
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.0, 0.72, 1.0),
                intensity: 620_000.0,
                range: 550.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(-80.0, 220.0, 60.0),
            ..default()
        },
        WorldGeometry,
    ));

    // Warm orange counter-light from the opposite side, strongest near sunset.
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(1.0, 0.52, 0.12),
                intensity: 260_000.0,
                range: 450.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(120.0, 160.0, -80.0),
            ..default()
        },
        WorldGeometry,
    ));
}

/// N11b: merge nearby minimal building shells that share a material into a
/// single far-distance draw-call proxy. Original entities and colliders stay
/// intact and render only in the near tier.
fn building_cluster_cell(position: Vec3, cluster_size: f32) -> (i32, i32) {
    (
        (position.x / cluster_size).floor() as i32,
        (position.z / cluster_size).floor() as i32,
    )
}

fn building_material_is_clusterable(material: Option<&StandardMaterial>) -> bool {
    material.is_some_and(|material| matches!(material.alpha_mode, AlphaMode::Opaque))
}

const CITY_ROOFTOP_ROUTE_LIMIT: usize = 28;
const CITY_ROOFTOP_MAX_DEGREE: usize = 2;
const CITY_SKY_PLAYGROUND_LIMIT: usize = 8;
const CITY_ROOFTOP_TRIAL_LIMIT: usize = 4;
const CITY_ROOFTOP_TRIAL_MAX_CHECKPOINTS: usize = 8;

#[derive(Debug, Clone, Copy)]
struct CityRoofAnchor {
    center: Vec3,
    footprint: Vec2,
}

#[derive(Debug, Clone, Copy)]
struct CityRooftopRoutePlan {
    start: Vec3,
    end: Vec3,
    kind: CityRooftopRouteKind,
}

#[derive(Debug, Clone, Copy)]
struct CityRooftopRouteCandidate {
    first: usize,
    second: usize,
    score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CitySkyPlaygroundPlan {
    center: Vec3,
    radius: f32,
    launch_position: Vec3,
    launch_velocity: Vec3,
    launch_target: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
struct CityRooftopTrialPlan {
    course_key: String,
    course_label: String,
    checkpoints: Vec<Vec3>,
    gold_time: f32,
    silver_time: f32,
    bronze_time: f32,
}

fn city_rooftop_route_plans(anchors: &[CityRoofAnchor]) -> Vec<CityRooftopRoutePlan> {
    let mut candidates = Vec::new();
    for first in 0..anchors.len() {
        for second in (first + 1)..anchors.len() {
            let delta = anchors[second].center - anchors[first].center;
            let horizontal = Vec2::new(delta.x, delta.z).length();
            let vertical = delta.y.abs();
            if !(18.0..=115.0).contains(&horizontal) || vertical > 42.0 {
                continue;
            }
            candidates.push(CityRooftopRouteCandidate {
                first,
                second,
                score: horizontal + vertical * 1.8,
            });
        }
    }
    candidates.sort_by(|a, b| {
        a.score
            .total_cmp(&b.score)
            .then(a.first.cmp(&b.first))
            .then(a.second.cmp(&b.second))
    });

    let mut parent = (0..anchors.len()).collect::<Vec<_>>();
    let mut degree = vec![0usize; anchors.len()];
    let mut plans = Vec::new();
    for candidate in candidates {
        if plans.len() >= CITY_ROOFTOP_ROUTE_LIMIT
            || degree[candidate.first] >= CITY_ROOFTOP_MAX_DEGREE
            || degree[candidate.second] >= CITY_ROOFTOP_MAX_DEGREE
        {
            continue;
        }
        let first_root = rooftop_route_component_root(&mut parent, candidate.first);
        let second_root = rooftop_route_component_root(&mut parent, candidate.second);
        if first_root == second_root {
            continue;
        }
        parent[second_root] = first_root;
        degree[candidate.first] += 1;
        degree[candidate.second] += 1;

        let first = anchors[candidate.first];
        let second = anchors[candidate.second];
        let horizontal_delta = Vec2::new(
            second.center.x - first.center.x,
            second.center.z - first.center.z,
        );
        let direction = horizontal_delta.normalize_or_zero();
        let max_inset = horizontal_delta.length() * 0.28;
        let first_inset = (first.footprint.min_element() * 0.34).min(max_inset);
        let second_inset = (second.footprint.min_element() * 0.34).min(max_inset);
        let mut start =
            first.center + Vec3::new(direction.x * first_inset, 0.0, direction.y * first_inset);
        let mut end =
            second.center - Vec3::new(direction.x * second_inset, 0.0, direction.y * second_inset);
        let kind = if (start.y - end.y).abs() <= 7.5 {
            start.y += 0.45;
            end.y += 0.45;
            CityRooftopRouteKind::Skybridge
        } else {
            start.y += 2.4;
            end.y += 2.4;
            if start.y < end.y {
                std::mem::swap(&mut start, &mut end);
            }
            CityRooftopRouteKind::Zipline
        };
        plans.push(CityRooftopRoutePlan { start, end, kind });
    }
    plans
}

fn city_sky_playground_plans(
    anchors: &[CityRoofAnchor],
    routes: &[CityRooftopRoutePlan],
) -> Vec<CitySkyPlaygroundPlan> {
    let mut connections = vec![Vec::<Vec3>::new(); anchors.len()];
    for route in routes {
        let Some(first) = nearest_city_roof_anchor(anchors, route.start) else {
            continue;
        };
        let Some(second) = nearest_city_roof_anchor(anchors, route.end) else {
            continue;
        };
        if first == second {
            continue;
        }
        connections[first].push(anchors[second].center);
        connections[second].push(anchors[first].center);
    }

    anchors
        .iter()
        .enumerate()
        .filter_map(|(index, anchor)| {
            if connections[index].len() < 2 {
                return None;
            }
            let target = connections[index]
                .iter()
                .copied()
                .filter(|target| {
                    let horizontal =
                        Vec2::new(target.x - anchor.center.x, target.z - anchor.center.z).length();
                    target.y - anchor.center.y <= 14.0 && horizontal <= 82.0
                })
                .min_by(|a, b| {
                    a.distance_squared(anchor.center)
                        .total_cmp(&b.distance_squared(anchor.center))
                })?;
            let horizontal_delta =
                Vec2::new(target.x - anchor.center.x, target.z - anchor.center.z);
            let horizontal_distance = horizontal_delta.length();
            let direction = horizontal_delta.normalize_or_zero();
            let direction_3d = Vec3::new(direction.x, 0.0, direction.y);
            let roof_radius = anchor.footprint.min_element() * 0.5;
            let radius = (roof_radius * 0.5).clamp(3.8, 6.2);
            let launch_position =
                anchor.center + direction_3d * (roof_radius * 0.62) + Vec3::Y * 0.48;
            let launch_speed = (horizontal_distance * 0.48).clamp(23.0, 42.0);
            let launch_lift = (14.0 + (target.y - anchor.center.y).max(0.0) * 0.55).min(22.0);
            Some(CitySkyPlaygroundPlan {
                center: anchor.center,
                radius,
                launch_position,
                launch_velocity: direction_3d * launch_speed + Vec3::Y * launch_lift,
                launch_target: target,
            })
        })
        .take(CITY_SKY_PLAYGROUND_LIMIT)
        .collect()
}

fn nearest_city_roof_anchor(anchors: &[CityRoofAnchor], point: Vec3) -> Option<usize> {
    anchors
        .iter()
        .enumerate()
        .min_by(|(_, first), (_, second)| {
            first
                .center
                .distance_squared(point)
                .total_cmp(&second.center.distance_squared(point))
        })
        .map(|(index, _)| index)
}

fn city_rooftop_trial_plans(
    anchors: &[CityRoofAnchor],
    routes: &[CityRooftopRoutePlan],
) -> Vec<CityRooftopTrialPlan> {
    let mut adjacency = vec![Vec::<usize>::new(); anchors.len()];
    for route in routes {
        let Some(first) = nearest_city_roof_anchor(anchors, route.start) else {
            continue;
        };
        let Some(second) = nearest_city_roof_anchor(anchors, route.end) else {
            continue;
        };
        if first == second || adjacency[first].contains(&second) {
            continue;
        }
        adjacency[first].push(second);
        adjacency[second].push(first);
    }
    for connections in &mut adjacency {
        connections.sort_unstable();
    }

    let mut visited = vec![false; anchors.len()];
    let mut plans = Vec::new();
    for start in 0..anchors.len() {
        if visited[start] || adjacency[start].len() != 1 {
            continue;
        }
        let mut chain = Vec::new();
        let mut previous = None;
        let mut current = start;
        loop {
            chain.push(current);
            visited[current] = true;
            let next = adjacency[current]
                .iter()
                .copied()
                .find(|candidate| Some(*candidate) != previous && !visited[*candidate]);
            let Some(next) = next else {
                break;
            };
            previous = Some(current);
            current = next;
        }
        if chain.len() < 4 {
            continue;
        }
        chain.truncate(CITY_ROOFTOP_TRIAL_MAX_CHECKPOINTS);
        let checkpoints = chain
            .iter()
            .map(|index| anchors[*index].center + Vec3::Y * 1.6)
            .collect::<Vec<_>>();
        let distance = checkpoints
            .windows(2)
            .map(|pair| pair[0].distance(pair[1]))
            .sum::<f32>();
        let gold_time = (distance / 18.0 + checkpoints.len() as f32 * 1.5).max(14.0);
        let first = checkpoints[0];
        let last = checkpoints[checkpoints.len() - 1];
        let course_key = format!(
            "city_rooftop_trial:{}:{}:{}:{}:{}:{}",
            (first.x * 10.0).round() as i64,
            (first.y * 10.0).round() as i64,
            (first.z * 10.0).round() as i64,
            (last.x * 10.0).round() as i64,
            (last.y * 10.0).round() as i64,
            (last.z * 10.0).round() as i64,
        );
        plans.push(CityRooftopTrialPlan {
            course_key,
            course_label: format!("Skyline Sprint {}", plans.len() + 1),
            checkpoints,
            gold_time,
            silver_time: gold_time * 1.28,
            bronze_time: gold_time * 1.62,
        });
        if plans.len() >= CITY_ROOFTOP_TRIAL_LIMIT {
            break;
        }
    }
    plans
}

fn rooftop_route_component_root(parent: &mut [usize], index: usize) -> usize {
    if parent[index] != index {
        parent[index] = rooftop_route_component_root(parent, parent[index]);
    }
    parent[index]
}

fn city_rooftop_route_reward_key(plan: CityRooftopRoutePlan) -> String {
    fn quantized(point: Vec3) -> (i64, i64, i64) {
        (
            (point.x * 10.0).round() as i64,
            (point.y * 10.0).round() as i64,
            (point.z * 10.0).round() as i64,
        )
    }

    let mut first = quantized(plan.start);
    let mut second = quantized(plan.end);
    if first > second {
        std::mem::swap(&mut first, &mut second);
    }
    let kind = match plan.kind {
        CityRooftopRouteKind::Skybridge => "bridge",
        CityRooftopRouteKind::Zipline => "zipline",
    };
    format!(
        "city_rooftop:{kind}:{}:{}:{}:{}:{}:{}",
        first.0, first.1, first.2, second.0, second.1, second.2
    )
}

fn build_city_rooftop_routes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    assets: Res<CityRooftopRouteAssets>,
    buildings: Query<(&Transform, &Building, &EnterableBuilding)>,
    existing_routes: Query<(), With<CityRooftopRoute>>,
) {
    if !existing_routes.is_empty() {
        return;
    }

    let mut anchors = buildings
        .iter()
        .map(|(transform, building, enterable)| CityRoofAnchor {
            center: transform.translation + Vec3::Y * building.height,
            footprint: enterable.footprint,
        })
        .collect::<Vec<_>>();
    anchors.sort_by(|a, b| {
        a.center
            .x
            .total_cmp(&b.center.x)
            .then(a.center.z.total_cmp(&b.center.z))
            .then(a.center.y.total_cmp(&b.center.y))
    });

    let route_plans = city_rooftop_route_plans(&anchors);
    for plan in route_plans.iter().copied() {
        let reward_key = city_rooftop_route_reward_key(plan);
        match plan.kind {
            CityRooftopRouteKind::Skybridge => {
                spawn_city_rooftop_skybridge(&mut commands, &mut meshes, &assets, plan, &reward_key)
            }
            CityRooftopRouteKind::Zipline => {
                spawn_city_rooftop_zipline(&mut commands, &mut meshes, &assets, plan, &reward_key)
            }
        }
        spawn_city_rooftop_route_markers(&mut commands, &assets, plan, reward_key.as_str());
    }
    for playground in city_sky_playground_plans(&anchors, &route_plans) {
        spawn_city_sky_playground(&mut commands, &mut meshes, &assets, playground);
    }
    for trial in city_rooftop_trial_plans(&anchors, &route_plans) {
        spawn_city_rooftop_trial(&mut commands, &assets, trial);
    }
}

fn spawn_city_rooftop_skybridge(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &CityRooftopRouteAssets,
    plan: CityRooftopRoutePlan,
    reward_key: &str,
) {
    let delta = plan.end - plan.start;
    let length = delta.length();
    if length < 6.0 {
        return;
    }
    let center = plan.start.lerp(plan.end, 0.5);
    let rotation = Quat::from_rotation_arc(Vec3::Z, delta / length);
    let deck_size = Vec3::new(3.6, 0.34, length);
    commands.spawn((
        Name::new("Technicolor Rooftop Skybridge"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::from_size(deck_size))),
            material: MeshMaterial3d(assets.bridge.clone()),
            transform: Transform::from_translation(center).with_rotation(rotation),
            ..default()
        },
        CityRooftopRoute {
            kind: plan.kind,
            start: plan.start,
            end: plan.end,
        },
        CityRooftopRouteChallenge {
            reward_key: reward_key.to_string(),
            approach: [-1; 4],
            credits: 38,
            experience: 30,
            armor: 5,
        },
        WorldGeometry,
        WalkableSurface,
        DebugColliderBox { size: deck_size },
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            deck_size.x * 0.5,
            deck_size.y * 0.5,
            deck_size.z * 0.5,
        ),
    ));

    let horizontal = Vec3::new(delta.x, 0.0, delta.z).normalize_or_zero();
    let right = Vec3::new(horizontal.z, 0.0, -horizontal.x);
    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            Name::new("Skybridge Guardrail"),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.18, 0.9, length))),
                material: MeshMaterial3d(assets.guard.clone()),
                transform: Transform::from_translation(
                    center + right * side * 1.68 + Vec3::Y * 0.58,
                )
                .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_city_rooftop_zipline(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &CityRooftopRouteAssets,
    plan: CityRooftopRoutePlan,
    reward_key: &str,
) {
    let delta = plan.end - plan.start;
    let length = delta.length();
    if length < 8.0 {
        return;
    }
    commands.spawn((
        Name::new("Technicolor Rooftop Zipline"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(0.3, length))),
            material: MeshMaterial3d(assets.zipline.clone()),
            transform: Transform::from_translation(plan.start.lerp(plan.end, 0.5))
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, delta / length)),
            ..default()
        },
        CityRooftopRoute {
            kind: plan.kind,
            start: plan.start,
            end: plan.end,
        },
        CityRooftopRouteChallenge {
            reward_key: reward_key.to_string(),
            approach: [-1; 4],
            credits: 56,
            experience: 44,
            armor: 8,
        },
        StuntGrindRail {
            start: plan.start,
            end: plan.end,
            speed: 52.0,
            snap_radius: 3.0,
            exit_lift: 9.0,
        },
        GrappleSocket::new(GrappleTargetKind::SwingPoint)
            .with_radius(4.5)
            .with_priority(1.3),
        WorldGeometry,
    ));

    for endpoint in [plan.start, plan.end] {
        let pad_size = Vec3::new(4.4, 0.3, 4.4);
        commands.spawn((
            Name::new("Zipline Rooftop Landing"),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::from_size(pad_size))),
                material: MeshMaterial3d(assets.guard.clone()),
                transform: Transform::from_translation(endpoint - Vec3::Y * 2.25),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            DebugColliderBox { size: pad_size },
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                pad_size.x * 0.5,
                pad_size.y * 0.5,
                pad_size.z * 0.5,
            ),
        ));
    }
}

fn spawn_city_rooftop_route_markers(
    commands: &mut Commands,
    assets: &CityRooftopRouteAssets,
    plan: CityRooftopRoutePlan,
    reward_key: &str,
) {
    let horizontal =
        Vec3::new(plan.end.x - plan.start.x, 0.0, plan.end.z - plan.start.z).normalize_or_zero();
    for (endpoint_index, endpoint) in [plan.start, plan.end].into_iter().enumerate() {
        let inward = if endpoint_index == 0 {
            horizontal
        } else {
            -horizontal
        };
        let (mesh, material, position, rotation, name) = match plan.kind {
            CityRooftopRouteKind::Skybridge => (
                assets.bridge_marker.clone(),
                assets.guard.clone(),
                endpoint + Vec3::Y * 1.45,
                Quat::from_rotation_arc(Vec3::Y, inward),
                "Skybridge Route Beacon",
            ),
            CityRooftopRouteKind::Zipline => (
                assets.zipline_marker.clone(),
                assets.zipline.clone(),
                endpoint,
                Quat::from_rotation_arc(Vec3::Z, inward)
                    * Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
                "Zipline Route Beacon",
            ),
        };
        let base_scale = Vec3::splat(1.0);
        commands.spawn((
            Name::new(name),
            PbrBundle {
                mesh: Mesh3d(mesh),
                material: MeshMaterial3d(material),
                transform: Transform::from_translation(position)
                    .with_rotation(rotation)
                    .with_scale(base_scale),
                ..default()
            },
            CityRooftopRouteMarker {
                reward_key: reward_key.to_string(),
                kind: plan.kind,
                base_scale,
                phase: endpoint_index as f32 * std::f32::consts::PI,
            },
            WorldGeometry,
        ));
    }
}

fn spawn_city_sky_playground(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &CityRooftopRouteAssets,
    plan: CitySkyPlaygroundPlan,
) {
    let direction = plan.launch_velocity.with_y(0.0).normalize_or_zero();
    let right = Vec3::new(direction.z, 0.0, -direction.x);
    let plaza_size = Vec3::new(plan.radius * 2.0, 0.3, plan.radius * 2.0);
    commands.spawn((
        Name::new("Rooftop Sky Playground"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(plan.radius, plaza_size.y))),
            material: MeshMaterial3d(assets.playground.clone()),
            transform: Transform::from_translation(plan.center + Vec3::Y * (plaza_size.y * 0.5)),
            ..default()
        },
        CitySkyPlayground,
        WorldGeometry,
        WalkableSurface,
        DebugColliderBox { size: plaza_size },
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(plaza_size.y * 0.5, plan.radius),
    ));
    commands.spawn((
        Name::new("Playground Safety Halo"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Torus::new(plan.radius - 0.22, plan.radius))),
            material: MeshMaterial3d(assets.guard.clone()),
            transform: Transform::from_translation(plan.center + Vec3::Y * 0.36),
            ..default()
        },
        WorldGeometry,
    ));

    let shade_center = plan.center - direction * plan.radius * 0.34;
    let shade_height = 2.8;
    for side in [-1.0_f32, 1.0] {
        let position = shade_center + right * side * plan.radius * 0.43;
        commands.spawn((
            Name::new("Playground Shade Post"),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.28, shade_height, 0.28))),
                material: MeshMaterial3d(assets.guard.clone()),
                transform: Transform::from_translation(position + Vec3::Y * (shade_height * 0.5)),
                ..default()
            },
            WorldGeometry,
        ));
    }
    commands.spawn((
        Name::new("Playground Shade Arch"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(0.34, 0.34, plan.radius * 0.94))),
            material: MeshMaterial3d(assets.guard.clone()),
            transform: Transform::from_translation(shade_center + Vec3::Y * shade_height)
                .with_rotation(Quat::from_rotation_arc(Vec3::Z, right)),
            ..default()
        },
        WorldGeometry,
    ));

    for side in [-1.0_f32, 1.0] {
        let planter_position =
            plan.center + right * side * plan.radius * 0.62 - direction * plan.radius * 0.05;
        commands.spawn((
            Name::new("Sky Playground Planter"),
            PbrBundle {
                mesh: Mesh3d(assets.planter_mesh.clone()),
                material: MeshMaterial3d(assets.bridge.clone()),
                transform: Transform::from_translation(planter_position + Vec3::Y * 0.46),
                ..default()
            },
            WorldGeometry,
        ));
        commands.spawn((
            Name::new("Sky Playground Flower"),
            PbrBundle {
                mesh: Mesh3d(assets.flower_mesh.clone()),
                material: MeshMaterial3d(assets.greenery.clone()),
                transform: Transform::from_translation(planter_position + Vec3::Y * 0.98),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let spring_yaw = direction.x.atan2(direction.z);
    commands.spawn((
        Name::new("Sky Playground Spring"),
        PbrBundle {
            mesh: Mesh3d(assets.spring_mesh.clone()),
            material: MeshMaterial3d(assets.zipline.clone()),
            transform: Transform::from_translation(plan.launch_position)
                .with_rotation(Quat::from_rotation_y(spring_yaw)),
            ..default()
        },
        SpringJumpPad {
            launch_velocity: plan.launch_velocity,
            radius: 1.9,
            cooldown: 0.22,
            cooldown_timers: [0.0; 4],
            force_hoverboard: false,
        },
        GrappleSocket::new(GrappleTargetKind::ZipPoint)
            .with_radius(3.0)
            .with_priority(1.2),
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(0.29, 1.65),
    ));
    commands.spawn((
        Name::new("Sky Playground Launch Arrow"),
        PbrBundle {
            mesh: Mesh3d(assets.spring_arrow_mesh.clone()),
            material: MeshMaterial3d(assets.guard.clone()),
            transform: Transform::from_translation(plan.launch_position + Vec3::Y * 0.38)
                .with_rotation(Quat::from_rotation_y(spring_yaw)),
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_city_rooftop_trial(
    commands: &mut Commands,
    assets: &CityRooftopRouteAssets,
    plan: CityRooftopTrialPlan,
) {
    let checkpoint_count = plan.checkpoints.len() as u8;
    for (checkpoint_index, checkpoint) in plan.checkpoints.iter().copied().enumerate() {
        let direction = if let Some(next) = plan.checkpoints.get(checkpoint_index + 1) {
            (*next - checkpoint).with_y(0.0).normalize_or_zero()
        } else {
            (checkpoint - plan.checkpoints[checkpoint_index - 1])
                .with_y(0.0)
                .normalize_or_zero()
        };
        let material = if checkpoint_index == 0 {
            assets.greenery.clone()
        } else if checkpoint_index + 1 == plan.checkpoints.len() {
            assets.guard.clone()
        } else {
            assets.bridge.clone()
        };
        commands.spawn((
            Name::new(format!(
                "{} Checkpoint {}",
                plan.course_label,
                checkpoint_index + 1
            )),
            PbrBundle {
                mesh: Mesh3d(assets.trial_gate_mesh.clone()),
                material: MeshMaterial3d(material),
                transform: Transform::from_translation(checkpoint)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Y, direction)),
                ..default()
            },
            CityRooftopTrialGate {
                course_key: plan.course_key.clone(),
                course_label: plan.course_label.clone(),
                checkpoint_index: checkpoint_index as u8,
                checkpoint_count,
                radius: 4.2,
                gold_time: plan.gold_time,
                silver_time: plan.silver_time,
                bronze_time: plan.bronze_time,
            },
            WorldGeometry,
        ));
    }
}

fn build_building_cluster_lods(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    buildings: Query<
        (
            Entity,
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &Transform,
            &Building,
        ),
        (
            Without<ChildOf>,
            Without<EnterableBuilding>,
            Without<BuildingLodClustered>,
            Without<crate::engine::spatial_lod::SpatialLodProxy>,
        ),
    >,
) {
    const CLUSTER_SIZE: f32 = 800.0;

    struct Part {
        entity: Entity,
        mesh: Handle<Mesh>,
        transform: Transform,
        height: f32,
    }

    struct Cluster {
        material: Handle<StandardMaterial>,
        parts: Vec<Part>,
    }

    let mut clusters: HashMap<(i32, i32, WorldZone, AssetId<StandardMaterial>), Cluster> =
        HashMap::new();
    for (entity, mesh, material, transform, building) in &buildings {
        if !building_material_is_clusterable(materials.get(&material.0)) {
            continue;
        }
        if meshes
            .get(&mesh.0)
            .and_then(|mesh| mesh.attribute(Mesh::ATTRIBUTE_POSITION))
            .is_none()
        {
            continue;
        }
        let (cell_x, cell_z) = building_cluster_cell(transform.translation, CLUSTER_SIZE);
        clusters
            .entry((cell_x, cell_z, building.zone, material.id()))
            .or_insert_with(|| Cluster {
                material: material.0.clone(),
                parts: Vec::new(),
            })
            .parts
            .push(Part {
                entity,
                mesh: mesh.0.clone(),
                transform: *transform,
                height: building.height,
            });
    }

    for ((cell_x, cell_z, _, _), mut cluster) in clusters {
        if cluster.parts.len() < 2 {
            continue;
        }
        cluster.parts.sort_by_key(|part| part.entity.to_bits());
        let origin = Vec3::new(
            (cell_x as f32 + 0.5) * CLUSTER_SIZE,
            0.0,
            (cell_z as f32 + 0.5) * CLUSTER_SIZE,
        );
        let max_height = cluster
            .parts
            .iter()
            .map(|part| part.height)
            .fold(0.0_f32, f32::max);
        let models = cluster
            .parts
            .iter()
            .map(|part| {
                (
                    part.mesh.clone(),
                    Mat4::from_translation(-origin) * part.transform.to_matrix(),
                )
            })
            .collect::<Vec<_>>();
        let proxy_mesh = bake_character_mesh(&meshes, &models);
        if proxy_mesh.count_vertices() == 0 {
            continue;
        }
        let proxy_handle = meshes.add(proxy_mesh);

        for part in &cluster.parts {
            commands.entity(part.entity).insert((
                BuildingLodClustered,
                crate::engine::spatial_lod::SpatialLod::building_high(max_height),
            ));
        }
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(proxy_handle),
                material: MeshMaterial3d(cluster.material),
                transform: Transform::from_translation(origin),
                ..default()
            },
            WorldGeometry,
            crate::engine::spatial_lod::SpatialLod::building_proxy(max_height),
            crate::engine::spatial_lod::SpatialLodProxy,
            BuildingClusterLodProxy,
        ));
    }
}

// ── Ground plane ─────────────────────────────────────────────────────────────
/// A wide invisible cuboid at Y = 0 that guarantees solid footing across the
/// entire city area. The terrain trimesh sits on top of it for the outer hills,
/// but the flat city core (where players spawn) needs a reliable floor.
fn spawn_ground_plane(commands: &mut Commands) {
    let size = Vec3::new(TERRAIN_WORLD_SIZE, 1.0, TERRAIN_WORLD_SIZE);
    commands.spawn((
        Transform::from_xyz(0.0, -0.5, 0.0),
        GlobalTransform::default(),
        WorldGeometry,
        WalkableSurface,
        DebugColliderBox { size },
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
    ));
}

fn spawn_world_anchor(commands: &mut Commands, id: &'static str, position: Vec3) {
    commands.spawn((
        Transform::from_translation(position),
        GlobalTransform::default(),
        WorldGeometry,
        WorldAnchor { id },
    ));
}

fn spawn_chapter_map_locations(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    for location in chapter_map_locations() {
        let ground_y = terrain_surface_y(location.x, location.z, seed);
        let anchor = Vec3::new(location.x, ground_y + 0.35, location.z);
        spawn_world_anchor(commands, location.anchor_id, anchor);

        let marker_material = chapter_map_marker_material(pal, location.id.0);
        let marker_height = if location.id.0 == 6 { 13.0 } else { 8.0 };
        let marker_radius = if location.id.0 == 6 { 1.05 } else { 0.72 };

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(5.2, 0.32))),
                material: MeshMaterial3d(pal.street_paint.clone()),
                transform: Transform::from_xyz(location.x, ground_y + 0.16, location.z),
                ..default()
            },
            WorldGeometry,
            ChapterMapMarker {
                chapter: location.id.0,
                region: location.region,
            },
        ));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(marker_radius, marker_height))),
                material: MeshMaterial3d(marker_material.clone()),
                transform: Transform::from_xyz(
                    location.x,
                    ground_y + marker_height * 0.5 + 0.35,
                    location.z,
                ),
                ..default()
            },
            WorldGeometry,
            ChapterMapMarker {
                chapter: location.id.0,
                region: location.region,
            },
        ));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(marker_radius * 2.4))),
                material: MeshMaterial3d(marker_material),
                transform: Transform::from_xyz(
                    location.x,
                    ground_y + marker_height + 1.45,
                    location.z,
                ),
                ..default()
            },
            WorldGeometry,
            ChapterMapMarker {
                chapter: location.id.0,
                region: location.region,
            },
        ));

        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: chapter_map_marker_light(location.id.0),
                    intensity: if location.id.0 == 6 {
                        90_000.0
                    } else {
                        38_000.0
                    },
                    range: if location.id.0 == 6 { 110.0 } else { 62.0 },
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(
                    location.x,
                    ground_y + marker_height + 2.0,
                    location.z,
                ),
                ..default()
            },
            WorldGeometry,
            ChapterMapMarker {
                chapter: location.id.0,
                region: location.region,
            },
        ));
    }
}

fn chapter_map_marker_material(pal: &Palette, chapter: u8) -> Handle<StandardMaterial> {
    match chapter {
        6 | 7 | 8 | 10 | 11 => pal.crystal_dragon.clone(),
        3 | 9 | 12 | 14 => pal.crystal_aurora.clone(),
        13 => pal.castle_trim.clone(),
        _ => pal.window_cool.clone(),
    }
}

fn chapter_map_marker_light(chapter: u8) -> Color {
    match chapter {
        6 => Color::srgb(0.70, 0.90, 1.0),
        7 | 8 | 10 | 11 => Color::srgb(1.0, 0.40, 0.18),
        3 | 9 | 12 | 14 => Color::srgb(0.52, 0.75, 1.0),
        13 => Color::srgb(1.0, 0.86, 0.34),
        _ => Color::srgb(0.55, 0.92, 1.0),
    }
}

fn spawn_puzzle_anchors(commands: &mut Commands, seed: u64) {
    let anchors: &[(&str, f32, f32, f32)] = &[
        // Chapter 1: downtown lab promenade
        ("ch01_giacoma_reward", 26.0, 0.4, 28.0),
        ("ch01_giacoma_node_1", 6.0, 0.0, 18.0),
        ("ch01_giacoma_node_2", 14.0, 0.0, 24.0),
        ("ch01_giacoma_node_3", 22.0, 0.0, 18.0),
        // Chapter 4: training court plates
        ("ch04_giacoma_reward", 3070.0, 0.2, -1620.0),
        ("ch04_giacoma_plate_1", 2920.0, 0.0, -1580.0),
        ("ch04_giacoma_plate_2", 3000.0, 0.0, -1680.0),
        ("ch04_giacoma_plate_3", 3090.0, 0.0, -1575.0),
        // Chapter 9: aurora garden crystal run
        ("ch09_giovanni_reward", 6710.0, 0.8, 735.0),
        ("ch09_giovanni_node_1", 6460.0, 0.0, 645.0),
        ("ch09_giovanni_node_2", 6550.0, 0.0, 805.0),
        ("ch09_giovanni_node_3", 6670.0, 0.0, 630.0),
        ("ch09_giovanni_node_4", 6780.0, 0.0, 790.0),
        // Chapter 12: switchworks relay lane
        ("ch12_gabrio_reward", 5530.0, 0.6, -3140.0),
        ("ch12_gabrio_node_1", 5250.0, 0.0, -3020.0),
        ("ch12_gabrio_node_2", 5400.0, 0.0, -2960.0),
        ("ch12_gabrio_node_3", 5550.0, 0.0, -3050.0),
        ("ch12_gabrio_node_4", 5700.0, 0.0, -2980.0),
    ];

    for &(id, x, y_offset, z) in anchors {
        let y = terrain_surface_y(x, z, seed) + y_offset;
        spawn_world_anchor(commands, id, Vec3::new(x, y, z));
    }
}

fn spawn_secret_cave_systems(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    for spec in SECRET_CAVE_LOCATIONS {
        spawn_secret_cave_system(commands, meshes, pal, seed, spec);
    }
}

fn secret_cave_accent(pal: &Palette, chapter: u8) -> Handle<StandardMaterial> {
    match chapter {
        6..=8 | 10..=11 => pal.crystal_dragon.clone(),
        2 | 5 | 12 | 13 => pal.crystal_emerald.clone(),
        _ => pal.crystal_aurora.clone(),
    }
}

const ANCIENT_CAVE_GATE_HEIGHT: f32 = 13.5;

fn spawn_ancient_cave_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    gate_id: &'static str,
    entrance: Vec3,
    forward: Vec3,
    right: Vec3,
    rotation: Quat,
    clear_width: f32,
    wall_material: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
) {
    commands.spawn((
        Name::new(format!("{gate_id} Ancient Exterior Gate")),
        Transform::from_translation(entrance),
        GlobalTransform::default(),
        AncientCaveGate {
            gate_id,
            clear_width,
            height: ANCIENT_CAVE_GATE_HEIGHT,
        },
        WorldGeometry,
    ));

    // Massive battered pylons and a deep lintel make the entrance readable
    // against the mountain silhouette from road-travel distance.
    for side in [-1.0, 1.0] {
        let pylon = entrance + right * side * (clear_width * 0.5 + 2.35) - forward * 0.35
            + Vec3::Y * (ANCIENT_CAVE_GATE_HEIGHT * 0.5);
        spawn_secret_cave_solid(
            commands,
            meshes,
            wall_material.clone(),
            "ancient_gate_stone",
            Vec3::new(4.3, ANCIENT_CAVE_GATE_HEIGHT, 4.6),
            Transform::from_translation(pylon)
                .with_rotation(rotation * Quat::from_rotation_z(side * -0.035)),
            false,
        );
        let crown = pylon + Vec3::Y * (ANCIENT_CAVE_GATE_HEIGHT * 0.52);
        commands.spawn((
            Name::new(format!("{gate_id} Gate Crown Rune")),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Torus {
                    major_radius: 1.35,
                    minor_radius: 0.24,
                })),
                material: MeshMaterial3d(accent.clone()),
                transform: Transform::from_translation(crown - forward * 2.35)
                    .with_rotation(rotation * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ..default()
            },
            WorldGeometry,
        ));
    }
    spawn_secret_cave_solid(
        commands,
        meshes,
        wall_material,
        "ancient_gate_stone",
        Vec3::new(clear_width + 8.4, 2.4, 4.8),
        Transform::from_translation(
            entrance - forward * 0.35 + Vec3::Y * (ANCIENT_CAVE_GATE_HEIGHT - 0.9),
        )
        .with_rotation(rotation),
        false,
    );

    // A luminous keystone identifies the gate as interactive ancient
    // technology rather than an ordinary mountain decoration.
    commands.spawn((
        Name::new(format!("{gate_id} Gate Keystone")),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(1.25, 0.55))),
            material: MeshMaterial3d(accent),
            transform: Transform::from_translation(
                entrance - forward * 2.75 + Vec3::Y * (ANCIENT_CAVE_GATE_HEIGHT - 1.0),
            )
            .with_rotation(rotation * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ..default()
        },
        WorldGeometry,
    ));

    // Dark recessed mouth behind the opening adds depth while leaving the
    // physical tunnel and its interactive doors fully traversable.
    commands.spawn((
        Name::new(format!("{gate_id} Recessed Cave Mouth")),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(4.8))),
            material: MeshMaterial3d(pal.rock_dark.clone()),
            transform: Transform {
                translation: entrance + forward * 3.5 + Vec3::Y * 3.5,
                scale: Vec3::new(1.45, 0.88, 0.30),
                rotation,
            },
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_dungeon_exit_portal(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    gate_id: &'static str,
    position: Vec3,
    return_positions: [Vec3; 4],
) {
    commands.spawn((
        Name::new(format!("{} Return Portal", gate_id)),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Torus {
                major_radius: 1.75,
                minor_radius: 0.28,
            })),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(position - Vec3::Y * 0.95),
            ..default()
        },
        DungeonExitPortal {
            gate_id,
            position,
            return_positions,
            interact_radius: 5.5,
        },
        WorldGeometry,
    ));
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.72, 0.32, 1.0),
                intensity: 8_500.0,
                range: 13.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(position + Vec3::Y * 1.2),
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_secret_cave_solid(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    material_slot: &'static str,
    size: Vec3,
    transform: Transform,
    walkable: bool,
) {
    let mut entity = commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            material: MeshMaterial3d(material),
            transform,
            ..default()
        },
        WorldGeometry,
        ProceduralMaterialBinding::new("cave.secret_network", material_slot),
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
    ));
    if walkable {
        entity.insert(WalkableSurface);
    }
}

const SECRET_CAVE_TUNNEL_SPACING: f32 = 4.0;
const SECRET_CAVE_MAX_GRADE: f32 = 0.24;
const SECRET_CAVE_TERRAIN_CLEARANCE: f32 = 0.65;
const SECRET_CAVE_CHAMBER_HALF_LENGTH: f32 = 8.5;

#[derive(Debug, Clone)]
struct SecretCaveFloorProfile {
    stations: Vec<Vec3>,
    tunnel_end: f32,
    chamber_surface: Vec3,
}

fn secret_cave_required_surface_y(center: Vec2, right: Vec2, half_width: f32, seed: u64) -> f32 {
    [-half_width, 0.0, half_width]
        .into_iter()
        .map(|offset| {
            let point = center + right * offset;
            terrain_surface_y(point.x, point.y, seed)
        })
        .fold(f32::NEG_INFINITY, f32::max)
        + SECRET_CAVE_TERRAIN_CLEARANCE
}

fn secret_cave_floor_profile(
    spec: SecretCaveLocation,
    seed: u64,
    width: f32,
) -> SecretCaveFloorProfile {
    let forward = Vec2::new(spec.yaw.sin(), spec.yaw.cos());
    let right = Vec2::new(forward.y, -forward.x);
    let entrance = Vec2::new(spec.x, spec.z);
    let tunnel_end = (spec.length - SECRET_CAVE_CHAMBER_HALF_LENGTH).max(12.0);
    let segment_count = (tunnel_end / SECRET_CAVE_TUNNEL_SPACING).ceil() as usize;
    let station_count = segment_count + 1;
    let half_width = width * 0.5 + 0.8;

    let mut required = Vec::with_capacity(station_count);
    for station in 0..station_count {
        let along = tunnel_end * station as f32 / segment_count as f32;
        let center = entrance + forward * along;
        required.push(secret_cave_required_surface_y(
            center, right, half_width, seed,
        ));
    }

    let chamber_center = entrance + forward * spec.length;
    let chamber_half_width = (width + 9.0) * 0.5 + 0.7;
    let mut chamber_y = f32::NEG_INFINITY;
    for along in [
        -SECRET_CAVE_CHAMBER_HALF_LENGTH,
        0.0,
        SECRET_CAVE_CHAMBER_HALF_LENGTH,
    ] {
        for side in [-chamber_half_width, 0.0, chamber_half_width] {
            let point = chamber_center + forward * along + right * side;
            chamber_y = chamber_y.max(terrain_surface_y(point.x, point.y, seed));
        }
    }
    chamber_y += SECRET_CAVE_TERRAIN_CLEARANCE;
    if let Some(last) = required.last_mut() {
        *last = last.max(chamber_y);
    }

    // Raising lower stations in both directions produces a profile that never
    // cuts into terrain while respecting the maximum climb/descent grade.
    let station_run = tunnel_end / segment_count as f32;
    let max_delta = station_run * SECRET_CAVE_MAX_GRADE;
    for _ in 0..2 {
        for index in 1..required.len() {
            required[index] = required[index].max(required[index - 1] - max_delta);
        }
        for index in (0..required.len().saturating_sub(1)).rev() {
            required[index] = required[index].max(required[index + 1] - max_delta);
        }
    }

    let stations = required
        .into_iter()
        .enumerate()
        .map(|(station, y)| {
            let along = tunnel_end * station as f32 / segment_count as f32;
            let center = entrance + forward * along;
            Vec3::new(center.x, y, center.y)
        })
        .collect::<Vec<_>>();
    let chamber_surface_y = stations
        .last()
        .map(|station| station.y)
        .unwrap_or(chamber_y);

    SecretCaveFloorProfile {
        stations,
        tunnel_end,
        chamber_surface: Vec3::new(chamber_center.x, chamber_surface_y, chamber_center.y),
    }
}

fn secret_cave_profile_position(profile: &SecretCaveFloorProfile, along: f32) -> Vec3 {
    if profile.stations.len() < 2 {
        return profile
            .stations
            .first()
            .copied()
            .unwrap_or(profile.chamber_surface);
    }
    let normalized = (along / profile.tunnel_end).clamp(0.0, 1.0);
    let scaled = normalized * (profile.stations.len() - 1) as f32;
    let index = scaled.floor() as usize;
    let next = (index + 1).min(profile.stations.len() - 1);
    profile.stations[index].lerp(profile.stations[next], scaled - index as f32)
}

fn secret_cave_segment_rotation(yaw: f32, from: Vec3, to: Vec3) -> Quat {
    let delta = to - from;
    let horizontal = delta.with_y(0.0).length().max(0.001);
    let slope = delta.y.atan2(horizontal);
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-slope)
}

#[derive(Debug, Clone)]
struct SecretCaveRoomGraph {
    rooms: [DungeonRoomZone; 3],
    portals: [DungeonRoomPortal; 2],
}

fn secret_cave_room_graph(
    spec: SecretCaveLocation,
    profile: &SecretCaveFloorProfile,
) -> SecretCaveRoomGraph {
    let entrance_focus = secret_cave_profile_position(profile, 5.0) + Vec3::Y;
    let traversal_focus =
        secret_cave_profile_position(profile, profile.tunnel_end * 0.58) + Vec3::Y;
    let chamber_focus = profile.chamber_surface + Vec3::Y;
    SecretCaveRoomGraph {
        rooms: [
            DungeonRoomZone {
                gate_id: spec.anchor_id,
                room_index: 0,
                label: "Entrance Gallery",
                kind: DungeonRoomKind::Entrance,
                focus: entrance_focus,
                camera_radius: 30.0,
            },
            DungeonRoomZone {
                gate_id: spec.anchor_id,
                room_index: 1,
                label: "Traversal Tunnel",
                kind: DungeonRoomKind::Traversal,
                focus: traversal_focus,
                camera_radius: 30.0,
            },
            DungeonRoomZone {
                gate_id: spec.anchor_id,
                room_index: 2,
                label: "Star Chamber",
                kind: DungeonRoomKind::Combat,
                focus: chamber_focus,
                camera_radius: 34.0,
            },
        ],
        portals: [
            DungeonRoomPortal {
                gate_id: spec.anchor_id,
                room_a: 0,
                room_b: 1,
                position: entrance_focus.lerp(traversal_focus, 0.5),
            },
            DungeonRoomPortal {
                gate_id: spec.anchor_id,
                room_a: 1,
                room_b: 2,
                position: secret_cave_profile_position(profile, profile.tunnel_end) + Vec3::Y,
            },
        ],
    }
}

fn spawn_secret_cave_room_graph(commands: &mut Commands, graph: &SecretCaveRoomGraph) {
    for room in graph.rooms {
        commands.spawn((
            Name::new(format!("{} Room {}", room.gate_id, room.room_index)),
            Transform::from_translation(room.focus),
            GlobalTransform::default(),
            room,
            WorldGeometry,
        ));
    }
    for portal in graph.portals {
        commands.spawn((
            Name::new(format!(
                "{} Room Portal {}-{}",
                portal.gate_id, portal.room_a, portal.room_b
            )),
            Transform::from_translation(portal.position),
            GlobalTransform::default(),
            portal,
            WorldGeometry,
        ));
    }
}

fn spawn_secret_cave_system(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    spec: SecretCaveLocation,
) {
    let chapter = spec.chapter.0;
    let forward = Vec3::new(spec.yaw.sin(), 0.0, spec.yaw.cos());
    let right = Vec3::new(forward.z, 0.0, -forward.x);
    let rotation = Quat::from_rotation_y(spec.yaw);
    let accent = secret_cave_accent(pal, chapter);
    let wall_mat = if matches!(chapter, 5 | 11 | 13 | 14) {
        pal.rock_dark.clone()
    } else {
        pal.rock.clone()
    };
    let floor_mat = if matches!(chapter, 3 | 9 | 14) {
        pal.marble.clone()
    } else if matches!(chapter, 1..=5 | 12..=14) {
        pal.stone_block.clone()
    } else {
        pal.dragon_stone.clone()
    };
    let width = 10.0 + (chapter % 3) as f32 * 1.5;
    let floor_profile = secret_cave_floor_profile(spec, seed, width);
    let entrance = floor_profile.stations[0];

    spawn_ancient_cave_gate(
        commands,
        meshes,
        pal,
        spec.anchor_id,
        entrance,
        forward,
        right,
        rotation,
        width,
        wall_mat.clone(),
        accent.clone(),
    );

    for (segment, pair) in floor_profile.stations.windows(2).enumerate() {
        let from = pair[0];
        let to = pair[1];
        let segment_rotation = secret_cave_segment_rotation(spec.yaw, from, to);
        let segment_up = segment_rotation * Vec3::Y;
        let segment_length = from.distance(to) + 0.18;
        let center = from.lerp(to, 0.5) - segment_up * 0.275;
        spawn_secret_cave_solid(
            commands,
            meshes,
            floor_mat.clone(),
            "floor",
            Vec3::new(width, 0.55, segment_length),
            Transform::from_translation(center).with_rotation(segment_rotation),
            true,
        );
        for side in [-1.0, 1.0] {
            spawn_secret_cave_solid(
                commands,
                meshes,
                wall_mat.clone(),
                "rock",
                Vec3::new(0.95, 4.0, segment_length + 0.12),
                Transform::from_translation(
                    center + right * side * (width * 0.5 + 0.65) + segment_up * 2.2,
                )
                .with_rotation(segment_rotation),
                false,
            );
        }
        if segment % 2 == 1 || segment + 1 == floor_profile.stations.len() - 1 {
            let rib_pos = to + (segment_rotation * Vec3::Y) * 4.25;
            spawn_secret_cave_solid(
                commands,
                meshes,
                pal.brushed_metal.clone(),
                "accent",
                Vec3::new(width + 1.6, 0.45, 0.75),
                Transform::from_translation(rib_pos).with_rotation(segment_rotation),
                false,
            );
        }
    }

    let chamber = floor_profile.chamber_surface;
    let room_graph = secret_cave_room_graph(spec, &floor_profile);
    spawn_secret_cave_room_graph(commands, &room_graph);

    // This portcullis is initially raised so the party can enter. The chamber
    // wave lowers it behind the party, then permanently releases it when every
    // encounter-owned enemy has been defeated.
    let encounter_threshold = room_graph.portals[1].position - Vec3::Y;
    let encounter_closed = encounter_threshold + Vec3::Y * 2.8;
    let encounter_open = encounter_closed + Vec3::Y * 8.0;
    commands.spawn((
        Name::new(format!("{} Star Chamber Seal", spec.anchor_id)),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width + 0.8, 5.6, 0.7))),
            material: MeshMaterial3d(pal.brushed_metal.clone()),
            transform: Transform::from_translation(encounter_open).with_rotation(rotation),
            ..default()
        },
        DungeonEncounterDoor {
            gate_id: spec.anchor_id,
            room_index: 2,
            requires_room_clear: None,
            closed: encounter_closed,
            open: encounter_open,
        },
        crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
        crate::engine::physics::prelude::Collider::cuboid((width + 0.8) * 0.5, 2.8, 0.35),
        WorldGeometry,
    ));

    commands.spawn((
        Name::new(format!("{} Awakened Chamber Reward", spec.anchor_id)),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(1.05))),
            material: MeshMaterial3d(accent.clone()),
            transform: Transform::from_translation(chamber + forward * 5.5 + Vec3::Y * 1.8),
            visibility: Visibility::Hidden,
            ..default()
        },
        DungeonEncounterReward {
            gate_id: spec.anchor_id,
            room_index: 2,
            credits: 120 + u32::from(spec.chapter.0) * 20,
            healing: 30.0,
            claimed: false,
        },
        WorldGeometry,
    ));
    spawn_secret_cave_solid(
        commands,
        meshes,
        floor_mat.clone(),
        "floor",
        Vec3::new(width + 9.0, 0.65, 17.0),
        Transform::from_translation(chamber - Vec3::Y * 0.325).with_rotation(rotation),
        true,
    );
    for side in [-1.0, 1.0] {
        spawn_secret_cave_solid(
            commands,
            meshes,
            wall_mat.clone(),
            "rock",
            Vec3::new(1.2, 5.0, 17.0),
            Transform::from_translation(
                chamber + right * side * ((width + 9.0) * 0.5 + 0.5) + Vec3::Y * 2.6,
            )
            .with_rotation(rotation),
            false,
        );
    }
    spawn_secret_cave_solid(
        commands,
        meshes,
        wall_mat,
        "rock",
        Vec3::new(width + 9.0, 5.0, 1.0),
        Transform::from_translation(chamber + forward * 8.5 + Vec3::Y * 2.6)
            .with_rotation(rotation),
        false,
    );

    for i in 0..7 {
        let side = if i % 2 == 0 { -1.0 } else { 1.0 };
        let shard_h = 2.4 + seeded(seed, chapter as u64 * 97 + i as u64) * 3.4;
        let shard_r = 0.45 + seeded(seed, chapter as u64 * 101 + i as u64) * 0.85;
        let shard_mat = match (chapter + i as u8) % 5 {
            0 => pal.crystal_emerald.clone(),
            1 | 2 => accent.clone(),
            _ => pal.crystal_aurora.clone(),
        };
        let crystal_pos = chamber
            + right * side * (3.0 + i as f32 * 0.9)
            + forward * (-5.0 + i as f32 * 3.0)
            + Vec3::Y * (shard_h * 0.5);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cone {
                    radius: shard_r,
                    height: shard_h,
                })),
                material: MeshMaterial3d(shard_mat.clone()),
                transform: Transform::from_translation(crystal_pos).with_rotation(
                    rotation
                        * Quat::from_rotation_y(i as f32 * 0.7)
                        * Quat::from_rotation_z((side * 0.16).clamp(-0.22, 0.22)),
                ),
                ..default()
            },
            WorldGeometry,
        ));
        if i % 2 == 0 {
            commands.spawn((
                PointLightBundle {
                    point_light: PointLight {
                        color: if matches!(chapter, 6..=11) {
                            Color::srgb(1.0, 0.34, 0.12)
                        } else if matches!((chapter + i as u8) % 5, 0) {
                            Color::srgb(0.18, 1.0, 0.55)
                        } else {
                            Color::srgb(0.50, 0.78, 1.0)
                        },
                        intensity: 5_500.0,
                        range: 16.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    transform: Transform::from_translation(crystal_pos + Vec3::Y * shard_h * 0.45),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    for side in [-1.0, 1.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(2.6, 2.0, 0.18))),
                material: MeshMaterial3d(pal.glass_panel.clone()),
                transform: Transform::from_translation(
                    chamber + right * side * ((width + 9.0) * 0.5 + 0.62) + Vec3::Y * 2.8,
                )
                .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let platform_size = Vec3::new(4.2, 0.5, 4.2);
    for i in 0..2 {
        let base = chamber + right * (-3.5 + i as f32 * 7.0) + forward * -8.0 + Vec3::Y * 1.2;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(
                    platform_size.x,
                    platform_size.y,
                    platform_size.z,
                ))),
                material: MeshMaterial3d(accent.clone()),
                transform: Transform::from_translation(base).with_rotation(rotation),
                ..default()
            },
            crate::engine::physics::prelude::Collider::cuboid(
                platform_size.x * 0.5,
                platform_size.y * 0.5,
                platform_size.z * 0.5,
            ),
            crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
            WorldGeometry,
            WalkableSurface,
            MovingPlatform {
                start: base + Vec3::Y * -1.0,
                end: base + Vec3::Y * 2.2,
                speed: 2.2 + i as f32 * 0.35,
                phase: chapter as f32 * 0.31 + i as f32,
                size: platform_size,
            },
        ));
    }

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: if matches!(chapter, 6..=11) {
                    Color::srgb(1.0, 0.45, 0.18)
                } else {
                    Color::srgb(0.35, 0.92, 1.0)
                },
                intensity: 18_000.0,
                range: 38.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(chamber + Vec3::Y * 4.0),
            ..default()
        },
        WorldGeometry,
    ));

    // Every mountain cave is a real shared-screen dungeon entrance. The gate
    // reuses the established four-player top-down movement/camera/combat mode.
    let gate_entry = secret_cave_profile_position(&floor_profile, 1.5) + Vec3::Y * 1.2;
    commands.spawn((
        Transform::from_translation(gate_entry),
        DungeonCrawlGate {
            gate_id: spec.anchor_id,
            chapter,
            label: spec.label,
            entry: gate_entry,
            focus: chamber + Vec3::Y * 1.0,
            radius: 64.0,
            interact_radius: 7.5,
            opened: false,
        },
        WorldGeometry,
    ));
    let return_xz = entrance - forward * 4.5;
    let return_center = Vec3::new(
        return_xz.x,
        terrain_surface_y(return_xz.x, return_xz.z, seed) + 1.3,
        return_xz.z,
    );
    let portal_position = secret_cave_profile_position(&floor_profile, 6.0) + Vec3::Y * 1.2;
    spawn_dungeon_exit_portal(
        commands,
        meshes,
        accent.clone(),
        spec.anchor_id,
        portal_position,
        dungeon_return_positions(return_center, right, forward),
    );
    for side in [-1.0f32, 1.0] {
        let doorway = secret_cave_profile_position(&floor_profile, 2.1);
        let door_height = 8.2;
        let door_width = width * 0.49;
        let closed = doorway + right * side * (width * 0.245) + Vec3::Y * (door_height * 0.5);
        let open = closed + right * side * (width * 0.45 + 1.5);
        commands.spawn((
            Name::new(format!("{} Ancient Gate Door", spec.anchor_id)),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(door_width, door_height, 0.8))),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_translation(closed).with_rotation(rotation),
                ..default()
            },
            DungeonGateDoor {
                gate_id: spec.anchor_id,
                chapter,
                closed,
                open,
            },
            crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
            crate::engine::physics::prelude::Collider::cuboid(
                door_width * 0.5,
                door_height * 0.5,
                0.4,
            ),
            WorldGeometry,
        ));
    }

    // Gauntlet-style pressure waves at the tunnel and arena, scaled by
    // chapter while remaining readable for a shared four-player screen.
    for (index, position) in [
        secret_cave_profile_position(&floor_profile, spec.length * 0.52) + Vec3::Y * 1.0,
        chamber + forward * 1.5 + Vec3::Y * 1.0,
    ]
    .into_iter()
    .enumerate()
    {
        commands.spawn((
            Transform::from_translation(position),
            DungeonEnemySpawner {
                chapter,
                encounter: (index == 1).then_some((spec.anchor_id, 2)),
                wave_index: 0,
                enemy_type: if index == 0 {
                    EnemyType::Soldier
                } else if chapter >= 8 {
                    EnemyType::Heavy
                } else {
                    EnemyType::SpikeAlien
                },
                count: (2 + chapter / 4 + index as u8).min(6),
                trigger_radius: 13.0,
                difficulty: 0.82 + chapter as f32 * 0.055,
                spawned: false,
            },
            WorldGeometry,
        ));
    }

    // Mario-3D-World-style readable jump line: broad shared-screen pads with
    // alternating heights lead to the cave checkpoint and reward chamber.
    for step in 0..4 {
        let side = if step % 2 == 0 { -1.0 } else { 1.0 };
        let pad = chamber
            + forward * (-5.5 + step as f32 * 3.7)
            + right * side * 2.6
            + Vec3::Y * (0.65 + (step % 3) as f32 * 0.7);
        spawn_secret_cave_solid(
            commands,
            meshes,
            accent.clone(),
            "accent",
            Vec3::new(3.8, 0.55, 3.8),
            Transform::from_translation(pad).with_rotation(rotation),
            true,
        );
    }

    // Persistent visual checkpoint. Discovery of the matching cave id unlocks
    // its purple C button on the Everest fast-travel map.
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(1.4, 0.35))),
            material: MeshMaterial3d(accent.clone()),
            transform: Transform::from_translation(chamber + Vec3::Y * 0.45),
            ..default()
        },
        WorldGeometry,
    ));

    spawn_world_anchor(commands, spec.anchor_id, chamber + Vec3::Y * 1.4);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragonDungeonTheme {
    CrownIce,
    Ember,
    Fangroot,
    Garden,
    Granite,
    Icebreaker,
}

#[derive(Clone, Copy, Debug)]
struct DragonDungeonSpec {
    chapter: u8,
    anchor_id: &'static str,
    gate_label: &'static str,
    reward_id: &'static str,
    reward_label: &'static str,
    x: f32,
    z: f32,
    yaw: f32,
    theme: DragonDungeonTheme,
}

fn dragon_dungeon_specs() -> &'static [DragonDungeonSpec] {
    &[
        DragonDungeonSpec {
            chapter: 6,
            anchor_id: "dragon_dungeon_ch06",
            gate_label: "Collosar's Crown Gate",
            reward_id: "dragon_dungeon_ch06_cache",
            reward_label: "Crown Dungeon Hoard",
            x: -8500.0,
            z: -7800.0,
            yaw: 0.12,
            theme: DragonDungeonTheme::CrownIce,
        },
        DragonDungeonSpec {
            chapter: 7,
            anchor_id: "dragon_dungeon_ch07",
            gate_label: "Tarack's Ember Gate",
            reward_id: "dragon_dungeon_ch07_cache",
            reward_label: "Ember Dungeon Hoard",
            x: -7600.0,
            z: 3800.0,
            yaw: -0.52,
            theme: DragonDungeonTheme::Ember,
        },
        DragonDungeonSpec {
            chapter: 8,
            anchor_id: "dragon_dungeon_ch08",
            gate_label: "Shread's Fangroot Gate",
            reward_id: "dragon_dungeon_ch08_cache",
            reward_label: "Fangroot Dungeon Hoard",
            x: -5600.0,
            z: 7800.0,
            yaw: -1.05,
            theme: DragonDungeonTheme::Fangroot,
        },
        DragonDungeonSpec {
            chapter: 9,
            anchor_id: "dragon_dungeon_ch09",
            gate_label: "Pink Flame Garden Gate",
            reward_id: "dragon_dungeon_ch09_cache",
            reward_label: "Pink Flame Garden Hoard",
            x: 6600.0,
            z: 700.0,
            yaw: 0.84,
            theme: DragonDungeonTheme::Garden,
        },
        DragonDungeonSpec {
            chapter: 10,
            anchor_id: "dragon_dungeon_ch10",
            gate_label: "Ragar's Granite Gate",
            reward_id: "dragon_dungeon_ch10_cache",
            reward_label: "Granite Dungeon Hoard",
            x: 8500.0,
            z: 4800.0,
            yaw: -1.28,
            theme: DragonDungeonTheme::Granite,
        },
        DragonDungeonSpec {
            chapter: 11,
            anchor_id: "dragon_dungeon_ch11",
            gate_label: "Blackskull Ice Gate",
            reward_id: "dragon_dungeon_ch11_cache",
            reward_label: "Icebreaker Dungeon Hoard",
            x: 3200.0,
            z: -8200.0,
            yaw: 0.42,
            theme: DragonDungeonTheme::Icebreaker,
        },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScientistTempleTheme {
    Flight,
    Solar,
    Nova,
    Aegis,
}

#[derive(Clone, Copy, Debug)]
struct ScientistTempleSpec {
    /// Stable mechanism/temple identity; deliberately separate from campaign
    /// chapter semantics.
    gate_id: u8,
    /// Narrative chapter for dungeon scaling, HUD context, and persistence.
    story_chapter: u8,
    anchor_id: &'static str,
    gate_label: &'static str,
    reward_id: &'static str,
    reward_label: &'static str,
    #[allow(dead_code)]
    scientist: &'static str,
    x: f32,
    z: f32,
    yaw: f32,
    theme: ScientistTempleTheme,
    credits: u32,
    experience: u32,
    armor: u32,
    power_up: &'static str,
    special_ability: &'static str,
}

fn great_scientist_temple_specs() -> &'static [ScientistTempleSpec] {
    &[
        ScientistTempleSpec {
            gate_id: 101,
            story_chapter: 5,
            anchor_id: "great_scientist_temple_flight",
            gate_label: "Temple of the Sky Equation",
            reward_id: "ancient_flight_core",
            reward_label: "Ancient Flight Core",
            scientist: "Giacoma",
            x: -6400.0,
            z: -1800.0,
            yaw: 0.42,
            theme: ScientistTempleTheme::Flight,
            credits: 180,
            experience: 130,
            armor: 18,
            power_up: "ancient_flight_core",
            special_ability: "ancient_flight_core",
        },
        ScientistTempleSpec {
            gate_id: 102,
            story_chapter: 8,
            anchor_id: "great_scientist_temple_solar",
            gate_label: "Solar Sabre Observatory",
            reward_id: "solar_sabre_glyph",
            reward_label: "Solar Sabre Glyph",
            scientist: "Giovanni",
            x: -2100.0,
            z: 6400.0,
            yaw: -0.78,
            theme: ScientistTempleTheme::Solar,
            credits: 150,
            experience: 120,
            armor: 14,
            power_up: "solar_sabre_glyph",
            special_ability: "solar_sabre_glyph",
        },
        ScientistTempleSpec {
            gate_id: 103,
            story_chapter: 12,
            anchor_id: "great_scientist_temple_nova",
            gate_label: "Nova Missile Foundry",
            reward_id: "nova_missile_matrix",
            reward_label: "Nova Missile Matrix",
            scientist: "Gabrio",
            x: 7200.0,
            z: -5200.0,
            yaw: 1.12,
            theme: ScientistTempleTheme::Nova,
            credits: 170,
            experience: 135,
            armor: 16,
            power_up: "nova_missile_matrix",
            special_ability: "nova_missile_matrix",
        },
        ScientistTempleSpec {
            gate_id: 104,
            story_chapter: 14,
            anchor_id: "great_scientist_temple_aegis",
            gate_label: "Aegis Frame Archive",
            reward_id: "aegis_armor_frame",
            reward_label: "Aegis Armor Frame",
            scientist: "The Great Scientists",
            x: 1200.0,
            z: -6100.0,
            yaw: -0.35,
            theme: ScientistTempleTheme::Aegis,
            credits: 140,
            experience: 115,
            armor: 30,
            power_up: "aegis_armor_frame",
            special_ability: "aegis_armor_frame",
        },
    ]
}

fn spawn_great_scientist_temples(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    seed: u64,
) {
    for spec in great_scientist_temple_specs() {
        spawn_great_scientist_temple(commands, meshes, materials, pal, seed, *spec);
    }

    // A reusable first set of Saber progression objects. These are ordinary
    // save-backed HiddenRewards, so future chapters can author more gems and
    // blueprints without another bespoke pickup system.
    for (id, label, x, z) in [
        (
            "cyclone_slash_blueprint",
            "Cyclone Slash Blueprint",
            -2040.0,
            6315.0,
        ),
        (
            "comet_dash_blueprint",
            "Comet Dash Blueprint",
            -2185.0,
            6480.0,
        ),
        (
            "meteor_pound_blueprint",
            "Meteor Pound Blueprint",
            -1940.0,
            6560.0,
        ),
        ("solar_fire_gem", "Solar Fire Gem", -6250.0, -1710.0),
        ("storm_gem", "Storm Gem", 7040.0, -5060.0),
        ("frost_gem", "Frost Gem", -6760.0, -1930.0),
        ("void_gem", "Void Gem", 7380.0, -5410.0),
        (
            "legendary_starheart_gem",
            "Legendary Starheart Gem",
            3100.0,
            7000.0,
        ),
    ] {
        let position = Vec3::new(x, terrain_surface_y(x, z, seed) + 2.0, z);
        spawn_discoverable_beacon(
            commands,
            meshes,
            materials,
            DiscoverableKind::HiddenReward {
                reward_id: id,
                credits: 40,
                experience: 35,
                armor: 0,
                power_up: Some(id),
                special_ability: Some(id),
            },
            label,
            position,
        );
    }
}

fn scientist_temple_materials(
    pal: &Palette,
    theme: ScientistTempleTheme,
) -> (
    Handle<StandardMaterial>,
    Handle<StandardMaterial>,
    Handle<StandardMaterial>,
    Handle<StandardMaterial>,
) {
    match theme {
        ScientistTempleTheme::Flight => (
            pal.castle_stone.clone(),
            pal.glass_panel.clone(),
            pal.crystal_aurora.clone(),
            pal.window_cool.clone(),
        ),
        ScientistTempleTheme::Solar => (
            pal.castle_stone.clone(),
            pal.castle_trim.clone(),
            pal.crystal_aurora.clone(),
            pal.window_warm.clone(),
        ),
        ScientistTempleTheme::Nova => (
            pal.brushed_metal.clone(),
            pal.dragon_stone.clone(),
            pal.crystal_dragon.clone(),
            pal.dragon_window.clone(),
        ),
        ScientistTempleTheme::Aegis => (
            pal.stone_brick.clone(),
            pal.brushed_metal.clone(),
            pal.crystal_aurora.clone(),
            pal.window_cool.clone(),
        ),
    }
}

fn spawn_great_scientist_temple(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    seed: u64,
    spec: ScientistTempleSpec,
) {
    let floor_y = terrain_surface_y(spec.x, spec.z, seed) + 1.15;
    let origin = Vec3::new(spec.x, floor_y, spec.z);
    let rot = Quat::from_rotation_y(spec.yaw);
    let (stone_mat, floor_mat, accent_mat, glow_mat) = scientist_temple_materials(pal, spec.theme);

    for (local, size) in [
        (Vec3::new(0.0, 0.0, -46.0), Vec3::new(26.0, 0.7, 30.0)),
        (Vec3::new(0.0, 0.0, -14.0), Vec3::new(36.0, 0.7, 34.0)),
        (Vec3::new(-28.0, 0.6, 12.0), Vec3::new(24.0, 0.8, 26.0)),
        (Vec3::new(28.0, 0.6, 12.0), Vec3::new(24.0, 0.8, 26.0)),
        (Vec3::new(0.0, 1.4, 44.0), Vec3::new(42.0, 0.9, 28.0)),
    ] {
        spawn_dungeon_block(
            commands,
            meshes,
            floor_mat.clone(),
            origin,
            rot,
            local,
            size,
            true,
        );
    }

    for (local, size) in [
        (Vec3::new(-18.6, 4.0, -29.0), Vec3::new(1.2, 8.0, 64.0)),
        (Vec3::new(18.6, 4.0, -29.0), Vec3::new(1.2, 8.0, 64.0)),
        (Vec3::new(-41.0, 4.0, 12.0), Vec3::new(1.2, 8.0, 28.0)),
        (Vec3::new(41.0, 4.0, 12.0), Vec3::new(1.2, 8.0, 28.0)),
        (Vec3::new(-22.0, 5.2, 44.0), Vec3::new(1.4, 10.4, 30.0)),
        (Vec3::new(22.0, 5.2, 44.0), Vec3::new(1.4, 10.4, 30.0)),
        (Vec3::new(0.0, 6.0, 60.5), Vec3::new(48.0, 12.0, 1.4)),
    ] {
        spawn_dungeon_block(
            commands,
            meshes,
            stone_mat.clone(),
            origin,
            rot,
            local,
            size,
            false,
        );
    }

    spawn_scientist_temple_gate(
        commands,
        meshes,
        stone_mat.clone(),
        accent_mat.clone(),
        origin,
        rot,
        spec,
    );

    for (i, local) in [
        Vec3::new(-10.0, 2.2, -34.0),
        Vec3::new(10.0, 2.2, -22.0),
        Vec3::new(-30.0, 2.2, 12.0),
        Vec3::new(30.0, 2.2, 12.0),
        Vec3::new(-15.0, 3.2, 44.0),
        Vec3::new(15.0, 3.2, 44.0),
    ]
    .into_iter()
    .enumerate()
    {
        spawn_dungeon_pillar(
            commands,
            meshes,
            accent_mat.clone(),
            origin,
            rot,
            local,
            i as f32,
        );
    }

    spawn_scientist_temple_mechanism(commands, meshes, accent_mat.clone(), origin, rot, spec);

    for (radius, y, z) in [(4.2_f32, 5.4_f32, 43.0_f32), (2.2, 7.8, 43.0)] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(radius))),
                material: MeshMaterial3d(glow_mat.clone()),
                transform: Transform::from_translation(origin + rot * Vec3::new(0.0, y, z)),
                ..default()
            },
            WorldGeometry,
        ));
    }

    for local in [
        Vec3::new(0.0, 7.5, -34.0),
        Vec3::new(-28.0, 7.5, 12.0),
        Vec3::new(28.0, 7.5, 12.0),
        Vec3::new(0.0, 10.0, 44.0),
    ] {
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: scientist_temple_light_color(spec.theme),
                    intensity: 24_000.0,
                    range: 44.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_translation(origin + rot * local),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let reward = DiscoverableKind::HiddenReward {
        reward_id: spec.reward_id,
        credits: spec.credits,
        experience: spec.experience,
        armor: spec.armor,
        power_up: Some(spec.power_up),
        special_ability: Some(spec.special_ability),
    };
    spawn_discoverable_beacon(
        commands,
        meshes,
        materials,
        reward,
        spec.reward_label,
        origin + rot * Vec3::new(0.0, 3.2, 49.0),
    );
    spawn_world_anchor(
        commands,
        spec.anchor_id,
        origin + rot * Vec3::new(0.0, 3.0, 42.0),
    );
}

fn scientist_temple_light_color(theme: ScientistTempleTheme) -> Color {
    match theme {
        ScientistTempleTheme::Flight => Color::srgb(0.45, 0.86, 1.0),
        ScientistTempleTheme::Solar => Color::srgb(1.0, 0.82, 0.28),
        ScientistTempleTheme::Nova => Color::srgb(1.0, 0.32, 0.12),
        ScientistTempleTheme::Aegis => Color::srgb(0.62, 0.72, 1.0),
    }
}

fn spawn_scientist_temple_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    door_mat: Handle<StandardMaterial>,
    trim_mat: Handle<StandardMaterial>,
    origin: Vec3,
    rot: Quat,
    spec: ScientistTempleSpec,
) {
    let entry = origin + rot * Vec3::new(0.0, 2.4, -54.0);
    let focus = origin + rot * Vec3::new(0.0, 2.8, 2.0);

    commands.spawn((
        Transform::from_translation(entry),
        GlobalTransform::default(),
        WorldGeometry,
        DungeonCrawlGate {
            gate_id: spec.gate_label,
            chapter: spec.story_chapter,
            label: spec.gate_label,
            entry,
            focus,
            radius: 70.0,
            interact_radius: 18.0,
            opened: false,
        },
    ));
    let outward_center = origin + rot * Vec3::new(0.0, 2.4, -66.0);
    spawn_dungeon_exit_portal(
        commands,
        meshes,
        trim_mat.clone(),
        spec.gate_label,
        origin + rot * Vec3::new(0.0, 2.4, -48.0),
        dungeon_return_positions(outward_center, rot * Vec3::X, rot * Vec3::Z),
    );

    for side in [-1.0_f32, 1.0] {
        let closed = origin + rot * Vec3::new(side * 3.2, 4.2, -58.5);
        let open = origin + rot * Vec3::new(side * 9.8, 4.2, -58.5);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(5.8, 8.4, 1.1))),
                material: MeshMaterial3d(door_mat.clone()),
                transform: Transform::from_translation(closed).with_rotation(rot),
                ..default()
            },
            WorldGeometry,
            DungeonGateDoor {
                gate_id: spec.gate_label,
                chapter: spec.story_chapter,
                closed,
                open,
            },
            crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
            crate::engine::physics::prelude::Collider::cuboid(2.9, 4.2, 0.55),
        ));
    }

    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(1.1, 7.6))),
                material: MeshMaterial3d(trim_mat.clone()),
                transform: Transform::from_translation(
                    entry + rot * Vec3::new(side * 8.2, 3.8, 1.2),
                ),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_scientist_temple_mechanism(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    origin: Vec3,
    rot: Quat,
    spec: ScientistTempleSpec,
) {
    let axis = match spec.theme {
        ScientistTempleTheme::Flight => Vec3::new(0.0, 1.0, 0.0),
        ScientistTempleTheme::Solar => Vec3::new(1.0, 0.0, 0.0),
        ScientistTempleTheme::Nova => Vec3::new(0.0, 0.0, 1.0),
        ScientistTempleTheme::Aegis => Vec3::new(1.0, 0.0, 1.0).normalize(),
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Torus {
                major_radius: 7.0,
                minor_radius: 0.35,
            })),
            material: MeshMaterial3d(material.clone()),
            transform: Transform::from_translation(origin + rot * Vec3::new(0.0, 5.4, 36.0))
                .with_rotation(rot * Quat::from_axis_angle(axis, 0.85)),
            ..default()
        },
        WorldGeometry,
    ));

    let platform_size = Vec3::new(8.0, 0.7, 8.0);
    for i in 0..3u8 {
        let local = Vec3::new(-12.0 + i as f32 * 12.0, 1.2, 8.0 + i as f32 * 8.0);
        let base = origin + rot * local;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(
                    platform_size.x,
                    platform_size.y,
                    platform_size.z,
                ))),
                material: MeshMaterial3d(material.clone()),
                transform: Transform::from_translation(base).with_rotation(rot),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            MovingPlatform {
                start: base + Vec3::Y * -0.8,
                end: base + Vec3::Y * 2.4,
                speed: 1.7 + i as f32 * 0.35,
                phase: spec.gate_id as f32 * 0.1 + i as f32,
                size: platform_size,
            },
        ));
    }
}

fn spawn_dragon_lair_dungeons(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    seed: u64,
) {
    for spec in dragon_dungeon_specs() {
        spawn_dragon_lair_dungeon(commands, meshes, materials, pal, seed, *spec);
    }
}

fn dragon_dungeon_materials(
    pal: &Palette,
    theme: DragonDungeonTheme,
) -> (
    Handle<StandardMaterial>,
    Handle<StandardMaterial>,
    Handle<StandardMaterial>,
    Handle<StandardMaterial>,
) {
    match theme {
        DragonDungeonTheme::CrownIce | DragonDungeonTheme::Icebreaker => (
            pal.dragon_stone.clone(),
            pal.snow.clone(),
            pal.crystal_dragon.clone(),
            pal.window_cool.clone(),
        ),
        DragonDungeonTheme::Ember => (
            pal.dragon_stone.clone(),
            pal.rock_dark.clone(),
            pal.dragon_lava.clone(),
            pal.dragon_window.clone(),
        ),
        DragonDungeonTheme::Fangroot => (
            pal.rock_dark.clone(),
            pal.stone_brick.clone(),
            pal.crystal_dragon.clone(),
            pal.dragon_window.clone(),
        ),
        DragonDungeonTheme::Garden => (
            pal.stone_brick.clone(),
            pal.castle_stone.clone(),
            pal.crystal_aurora.clone(),
            pal.window_warm.clone(),
        ),
        DragonDungeonTheme::Granite => (
            pal.rock.clone(),
            pal.stone_brick.clone(),
            pal.dragon_lava.clone(),
            pal.dragon_window.clone(),
        ),
    }
}

fn dragon_dungeon_light_color(theme: DragonDungeonTheme) -> Color {
    match theme {
        DragonDungeonTheme::CrownIce | DragonDungeonTheme::Icebreaker => {
            Color::srgb(0.60, 0.78, 1.0)
        }
        DragonDungeonTheme::Garden => Color::srgb(1.0, 0.58, 0.88),
        DragonDungeonTheme::Granite => Color::srgb(1.0, 0.64, 0.22),
        DragonDungeonTheme::Ember | DragonDungeonTheme::Fangroot => Color::srgb(1.0, 0.32, 0.08),
    }
}

fn spawn_dragon_lair_dungeon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    seed: u64,
    spec: DragonDungeonSpec,
) {
    let floor_y = terrain_surface_y(spec.x, spec.z, seed) + 1.0;
    let origin = Vec3::new(spec.x, floor_y, spec.z);
    let rot = Quat::from_rotation_y(spec.yaw);
    let (wall_mat, floor_mat, accent_mat, beam_mat) = dragon_dungeon_materials(pal, spec.theme);
    let light_color = dragon_dungeon_light_color(spec.theme);

    for (local, size) in [
        (Vec3::new(0.0, 0.0, -54.0), Vec3::new(18.0, 0.7, 22.0)),
        (Vec3::new(0.0, 0.0, -28.0), Vec3::new(18.0, 0.7, 34.0)),
        (Vec3::new(-24.0, 0.0, -8.0), Vec3::new(28.0, 0.7, 24.0)),
        (Vec3::new(24.0, 0.0, -8.0), Vec3::new(28.0, 0.7, 24.0)),
        (Vec3::new(0.0, 0.0, 16.0), Vec3::new(20.0, 0.7, 36.0)),
        (Vec3::new(0.0, 1.6, 48.0), Vec3::new(34.0, 0.8, 26.0)),
    ] {
        spawn_dungeon_block(
            commands,
            meshes,
            floor_mat.clone(),
            origin,
            rot,
            local,
            size,
            true,
        );
    }

    let wall_specs = [
        (Vec3::new(-10.2, 3.0, -41.0), Vec3::new(1.2, 6.0, 54.0)),
        (Vec3::new(10.2, 3.0, -41.0), Vec3::new(1.2, 6.0, 54.0)),
        (Vec3::new(-39.0, 3.0, -8.0), Vec3::new(1.2, 6.0, 24.0)),
        (Vec3::new(-9.5, 3.0, -8.0), Vec3::new(1.2, 6.0, 24.0)),
        (Vec3::new(9.5, 3.0, -8.0), Vec3::new(1.2, 6.0, 24.0)),
        (Vec3::new(39.0, 3.0, -8.0), Vec3::new(1.2, 6.0, 24.0)),
        (Vec3::new(-10.6, 3.0, 16.0), Vec3::new(1.2, 6.0, 36.0)),
        (Vec3::new(10.6, 3.0, 16.0), Vec3::new(1.2, 6.0, 36.0)),
        (Vec3::new(0.0, 3.0, 65.0), Vec3::new(38.0, 6.0, 1.2)),
        (Vec3::new(-18.5, 4.4, 48.0), Vec3::new(1.2, 8.8, 28.0)),
        (Vec3::new(18.5, 4.4, 48.0), Vec3::new(1.2, 8.8, 28.0)),
    ];
    for (local, size) in wall_specs {
        spawn_dungeon_block(
            commands,
            meshes,
            wall_mat.clone(),
            origin,
            rot,
            local,
            size,
            false,
        );
    }

    for local in [
        Vec3::new(-5.0, 4.0, -64.0),
        Vec3::new(5.0, 4.0, -64.0),
        Vec3::new(0.0, 7.0, -63.4),
    ] {
        spawn_dungeon_block(
            commands,
            meshes,
            wall_mat.clone(),
            origin,
            rot,
            local,
            Vec3::new(5.0, 8.0, 2.2),
            false,
        );
    }
    spawn_dungeon_crawl_gate(
        commands,
        meshes,
        wall_mat.clone(),
        accent_mat.clone(),
        origin,
        rot,
        spec,
    );

    for (index, local) in [
        Vec3::new(-7.0, 1.2, -28.0),
        Vec3::new(7.0, 1.2, -16.0),
        Vec3::new(-30.0, 1.2, -8.0),
        Vec3::new(30.0, 1.2, -8.0),
        Vec3::new(-6.0, 1.2, 18.0),
        Vec3::new(6.0, 1.2, 30.0),
        Vec3::new(-13.0, 2.8, 49.0),
        Vec3::new(13.0, 2.8, 49.0),
    ]
    .into_iter()
    .enumerate()
    {
        spawn_dungeon_pillar(
            commands,
            meshes,
            accent_mat.clone(),
            origin,
            rot,
            local,
            index as f32,
        );
    }

    spawn_dungeon_hazard_strips(
        commands,
        meshes,
        accent_mat.clone(),
        origin,
        rot,
        spec.theme,
    );
    spawn_dungeon_moving_bridge(
        commands,
        meshes,
        accent_mat.clone(),
        origin,
        rot,
        spec.chapter,
    );
    spawn_dungeon_turrets(
        commands,
        meshes,
        wall_mat.clone(),
        beam_mat.clone(),
        origin,
        rot,
        spec.chapter,
    );

    for local in [
        Vec3::new(-24.0, 6.0, -8.0),
        Vec3::new(24.0, 6.0, -8.0),
        Vec3::new(0.0, 6.0, 18.0),
        Vec3::new(0.0, 9.0, 50.0),
    ] {
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: light_color,
                    intensity: 18_000.0,
                    range: 38.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_translation(origin + rot * local),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let reward = DiscoverableKind::HiddenReward {
        reward_id: spec.reward_id,
        credits: 120 + spec.chapter as u32 * 15,
        experience: 70 + spec.chapter as u32 * 10,
        armor: 8 + spec.chapter as u32,
        power_up: None,
        special_ability: None,
    };
    spawn_discoverable_beacon(
        commands,
        meshes,
        materials,
        reward,
        spec.reward_label,
        origin + rot * Vec3::new(0.0, 2.8, 54.0),
    );
    spawn_world_anchor(
        commands,
        spec.anchor_id,
        origin + rot * Vec3::new(0.0, 3.0, 48.0),
    );

    match spec.chapter {
        6 => spawn_ch6_crown_dungeon_extras(commands, meshes, materials, pal, origin, rot),
        7 => spawn_ch7_ember_dungeon_extras(commands, meshes, materials, pal, origin, rot),
        8 => spawn_ch8_fangroot_dungeon_extras(commands, meshes, materials, pal, origin, rot),
        9 => spawn_ch9_garden_dungeon_extras(commands, meshes, materials, pal, origin, rot),
        10 => spawn_ch10_granite_dungeon_extras(commands, meshes, materials, pal, origin, rot),
        11 => spawn_ch11_icebreaker_dungeon_extras(commands, meshes, materials, pal, origin, rot),
        _ => {}
    }
}

fn spawn_ch6_crown_dungeon_extras(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    origin: Vec3,
    rot: Quat,
) {
    // ── Key gem — glowing ice-blue orb in a side alcove off the mid room ──────
    let key_pos = origin + rot * Vec3::new(-12.0, 1.8, 5.0);
    let key_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.92, 1.0),
        emissive: LinearRgba::new(0.6, 2.4, 3.0, 1.0),
        ..default()
    });
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.55))),
            material: MeshMaterial3d(key_mat),
            transform: Transform::from_translation(key_pos),
            ..default()
        },
        DungeonKeyPickup {
            chapter: 6,
            collected: false,
            pickup_radius: 3.2,
        },
        WorldGeometry,
    ));
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.50, 0.90, 1.0),
                intensity: 8_000.0,
                range: 14.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(key_pos + Vec3::Y * 1.5),
            ..default()
        },
        WorldGeometry,
    ));

    // ── Boss key gate — two ice slabs blocking the throne corridor ────────────
    for side in [-1.0_f32, 1.0] {
        let closed = origin + rot * Vec3::new(side * 3.8, 3.5, 32.0);
        let open = origin + rot * Vec3::new(side * 10.0, 3.5, 32.0);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(6.5, 7.0, 1.0))),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_translation(closed),
                ..default()
            },
            DungeonKeyGate {
                chapter: 6,
                closed,
                open,
            },
            WorldGeometry,
        ));
    }

    // ── Enemy spawners: corridor, mid-room, back room ─────────────────────────
    for (local, count, enemy_type) in [
        (Vec3::new(0.0, 1.5, -38.0), 3u8, EnemyType::Soldier),
        (Vec3::new(0.0, 1.5, -8.0), 4u8, EnemyType::Hybrid),
        (Vec3::new(0.0, 1.5, 20.0), 3u8, EnemyType::SpikeAlien),
    ] {
        let world_pos = origin + rot * local;
        commands.spawn((
            Transform::from_translation(world_pos),
            GlobalTransform::default(),
            DungeonEnemySpawner {
                chapter: 6,
                encounter: None,
                wave_index: 0,
                enemy_type,
                count,
                trigger_radius: 24.0,
                difficulty: 1.35,
                spawned: false,
            },
        ));
    }
}

fn spawn_ch7_ember_dungeon_extras(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    origin: Vec3,
    rot: Quat,
) {
    // Key — ember-orange orb in a side alcove
    let key_pos = origin + rot * Vec3::new(-10.0, 1.8, 8.0);
    let key_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.98, 0.35, 0.05),
        emissive: LinearRgba::new(2.5, 0.6, 0.0, 1.0),
        ..default()
    });
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.55))),
            material: MeshMaterial3d(key_mat),
            transform: Transform::from_translation(key_pos),
            ..default()
        },
        DungeonKeyPickup {
            chapter: 7,
            collected: false,
            pickup_radius: 3.2,
        },
        WorldGeometry,
    ));
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(1.0, 0.42, 0.05),
                intensity: 9_000.0,
                range: 14.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(key_pos + Vec3::Y * 1.5),
            ..default()
        },
        WorldGeometry,
    ));

    // Gate — dark volcanic-stone slabs blocking the boss corridor
    for side in [-1.0_f32, 1.0] {
        let closed = origin + rot * Vec3::new(side * 3.8, 3.5, 30.0);
        let open = origin + rot * Vec3::new(side * 10.0, 3.5, 30.0);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(6.5, 7.0, 1.0))),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_translation(closed),
                ..default()
            },
            DungeonKeyGate {
                chapter: 7,
                closed,
                open,
            },
            WorldGeometry,
        ));
    }

    // Enemy spawners: entrance corridor, mid lava room, boss chamber
    for (local, count, enemy_type) in [
        (Vec3::new(0.0, 1.5, -38.0), 3u8, EnemyType::Soldier),
        (Vec3::new(0.0, 1.5, -8.0), 4u8, EnemyType::SpikeAlien),
        (Vec3::new(0.0, 1.5, 18.0), 2u8, EnemyType::Heavy),
    ] {
        commands.spawn((
            Transform::from_translation(origin + rot * local),
            GlobalTransform::default(),
            DungeonEnemySpawner {
                chapter: 7,
                encounter: None,
                wave_index: 0,
                enemy_type,
                count,
                trigger_radius: 24.0,
                difficulty: 1.40,
                spawned: false,
            },
        ));
    }
}

fn spawn_ch8_fangroot_dungeon_extras(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    origin: Vec3,
    rot: Quat,
) {
    // Key — vine-green crystal on a raised root platform
    let key_pos = origin + rot * Vec3::new(14.0, 1.8, 12.0);
    let key_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.28, 0.88, 0.18),
        emissive: LinearRgba::new(0.4, 2.2, 0.1, 1.0),
        ..default()
    });
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.55))),
            material: MeshMaterial3d(key_mat),
            transform: Transform::from_translation(key_pos),
            ..default()
        },
        DungeonKeyPickup {
            chapter: 8,
            collected: false,
            pickup_radius: 3.2,
        },
        WorldGeometry,
    ));
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.35, 0.90, 0.20),
                intensity: 8_500.0,
                range: 14.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(key_pos + Vec3::Y * 1.5),
            ..default()
        },
        WorldGeometry,
    ));

    // Gate — dark organic-root slabs
    for side in [-1.0_f32, 1.0] {
        let closed = origin + rot * Vec3::new(side * 3.8, 3.5, 32.0);
        let open = origin + rot * Vec3::new(side * 10.0, 3.5, 32.0);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(6.5, 7.0, 1.0))),
                material: MeshMaterial3d(pal.rock_dark.clone()),
                transform: Transform::from_translation(closed),
                ..default()
            },
            DungeonKeyGate {
                chapter: 8,
                closed,
                open,
            },
            WorldGeometry,
        ));
    }

    // Enemy spawners: vine-lined entrance, mid tangle room, lair boss chamber
    for (local, count, enemy_type) in [
        (Vec3::new(0.0, 1.5, -40.0), 5u8, EnemyType::Drone),
        (Vec3::new(0.0, 1.5, -10.0), 3u8, EnemyType::Soldier),
        (Vec3::new(0.0, 1.5, 20.0), 1u8, EnemyType::Hybrid),
    ] {
        commands.spawn((
            Transform::from_translation(origin + rot * local),
            GlobalTransform::default(),
            DungeonEnemySpawner {
                chapter: 8,
                encounter: None,
                wave_index: 0,
                enemy_type,
                count,
                trigger_radius: 24.0,
                difficulty: 1.50,
                spawned: false,
            },
        ));
    }
}

fn spawn_ch9_garden_dungeon_extras(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    origin: Vec3,
    rot: Quat,
) {
    // Key — pink-blossom orb on a garden plinth
    let key_pos = origin + rot * Vec3::new(12.0, 1.8, -4.0);
    let key_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.52, 0.88),
        emissive: LinearRgba::new(2.0, 0.6, 1.5, 1.0),
        ..default()
    });
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.55))),
            material: MeshMaterial3d(key_mat),
            transform: Transform::from_translation(key_pos),
            ..default()
        },
        DungeonKeyPickup {
            chapter: 9,
            collected: false,
            pickup_radius: 3.2,
        },
        WorldGeometry,
    ));
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(1.0, 0.50, 0.88),
                intensity: 8_000.0,
                range: 14.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(key_pos + Vec3::Y * 1.5),
            ..default()
        },
        WorldGeometry,
    ));

    // Gate — castle-stone arched slabs with garden detail
    for side in [-1.0_f32, 1.0] {
        let closed = origin + rot * Vec3::new(side * 3.8, 3.5, 30.0);
        let open = origin + rot * Vec3::new(side * 10.0, 3.5, 30.0);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(6.5, 7.0, 1.0))),
                material: MeshMaterial3d(pal.castle_stone.clone()),
                transform: Transform::from_translation(closed),
                ..default()
            },
            DungeonKeyGate {
                chapter: 9,
                closed,
                open,
            },
            WorldGeometry,
        ));
    }

    // Enemy spawners: garden path, fountain room, inner sanctum
    for (local, count, enemy_type) in [
        (Vec3::new(0.0, 1.5, -35.0), 3u8, EnemyType::Soldier),
        (Vec3::new(0.0, 1.5, -5.0), 3u8, EnemyType::SpikeAlien),
        (Vec3::new(0.0, 1.5, 22.0), 2u8, EnemyType::Heavy),
    ] {
        commands.spawn((
            Transform::from_translation(origin + rot * local),
            GlobalTransform::default(),
            DungeonEnemySpawner {
                chapter: 9,
                encounter: None,
                wave_index: 0,
                enemy_type,
                count,
                trigger_radius: 24.0,
                difficulty: 1.55,
                spawned: false,
            },
        ));
    }
}

fn spawn_ch10_granite_dungeon_extras(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    origin: Vec3,
    rot: Quat,
) {
    // Key — amber-granite gem in the side mineshaft
    let key_pos = origin + rot * Vec3::new(-14.0, 1.8, 10.0);
    let key_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.92, 0.55, 0.10),
        emissive: LinearRgba::new(2.8, 1.0, 0.0, 1.0),
        ..default()
    });
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.55))),
            material: MeshMaterial3d(key_mat),
            transform: Transform::from_translation(key_pos),
            ..default()
        },
        DungeonKeyPickup {
            chapter: 10,
            collected: false,
            pickup_radius: 3.2,
        },
        WorldGeometry,
    ));
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.95, 0.65, 0.15),
                intensity: 9_500.0,
                range: 16.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(key_pos + Vec3::Y * 1.5),
            ..default()
        },
        WorldGeometry,
    ));

    // Gate — heavy granite slabs blocking the throne vault
    for side in [-1.0_f32, 1.0] {
        let closed = origin + rot * Vec3::new(side * 3.8, 3.5, 30.0);
        let open = origin + rot * Vec3::new(side * 10.0, 3.5, 30.0);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(6.5, 7.0, 1.0))),
                material: MeshMaterial3d(pal.rock.clone()),
                transform: Transform::from_translation(closed),
                ..default()
            },
            DungeonKeyGate {
                chapter: 10,
                closed,
                open,
            },
            WorldGeometry,
        ));
    }

    // Enemy spawners: mine shaft, forge room, vault hall
    for (local, count, enemy_type) in [
        (Vec3::new(0.0, 1.5, -40.0), 2u8, EnemyType::Heavy),
        (Vec3::new(0.0, 1.5, -10.0), 5u8, EnemyType::Soldier),
        (Vec3::new(0.0, 1.5, 24.0), 2u8, EnemyType::Hybrid),
    ] {
        commands.spawn((
            Transform::from_translation(origin + rot * local),
            GlobalTransform::default(),
            DungeonEnemySpawner {
                chapter: 10,
                encounter: None,
                wave_index: 0,
                enemy_type,
                count,
                trigger_radius: 24.0,
                difficulty: 1.65,
                spawned: false,
            },
        ));
    }
}

fn spawn_ch11_icebreaker_dungeon_extras(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    origin: Vec3,
    rot: Quat,
) {
    // Key — cold-white ice shard — visually distinct from Ch.6's warmer ice-blue
    let key_pos = origin + rot * Vec3::new(10.0, 1.8, -6.0);
    let key_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.80, 0.95, 1.0),
        emissive: LinearRgba::new(1.5, 2.0, 3.0, 1.0),
        ..default()
    });
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.55))),
            material: MeshMaterial3d(key_mat),
            transform: Transform::from_translation(key_pos),
            ..default()
        },
        DungeonKeyPickup {
            chapter: 11,
            collected: false,
            pickup_radius: 3.2,
        },
        WorldGeometry,
    ));
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.75, 0.88, 1.0),
                intensity: 10_000.0,
                range: 16.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(key_pos + Vec3::Y * 1.5),
            ..default()
        },
        WorldGeometry,
    ));

    // Gate — dark fortress-ice slabs blocking the final vault
    for side in [-1.0_f32, 1.0] {
        let closed = origin + rot * Vec3::new(side * 3.8, 3.5, 34.0);
        let open = origin + rot * Vec3::new(side * 10.0, 3.5, 34.0);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(6.5, 7.0, 1.0))),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_translation(closed),
                ..default()
            },
            DungeonKeyGate {
                chapter: 11,
                closed,
                open,
            },
            WorldGeometry,
        ));
    }

    // Enemy spawners: frozen entrance hall, blizzard corridor, inner ice citadel
    for (local, count, enemy_type) in [
        (Vec3::new(0.0, 1.5, -42.0), 4u8, EnemyType::Soldier),
        (Vec3::new(0.0, 1.5, -8.0), 4u8, EnemyType::SpikeAlien),
        (Vec3::new(0.0, 1.5, 26.0), 2u8, EnemyType::Hybrid),
    ] {
        commands.spawn((
            Transform::from_translation(origin + rot * local),
            GlobalTransform::default(),
            DungeonEnemySpawner {
                chapter: 11,
                encounter: None,
                wave_index: 0,
                enemy_type,
                count,
                trigger_radius: 24.0,
                difficulty: 1.75,
                spawned: false,
            },
        ));
    }
}

fn spawn_dungeon_crawl_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    door_mat: Handle<StandardMaterial>,
    trim_mat: Handle<StandardMaterial>,
    origin: Vec3,
    rot: Quat,
    spec: DragonDungeonSpec,
) {
    let entry = origin + rot * Vec3::new(0.0, 2.4, -56.0);
    let focus = origin + rot * Vec3::new(0.0, 2.6, 0.0);

    commands.spawn((
        Transform::from_translation(entry),
        GlobalTransform::default(),
        WorldGeometry,
        DungeonCrawlGate {
            gate_id: spec.anchor_id,
            chapter: spec.chapter,
            label: spec.gate_label,
            entry,
            focus,
            radius: 66.0,
            interact_radius: 17.0,
            opened: false,
        },
    ));
    let outward_center = origin + rot * Vec3::new(0.0, 2.4, -68.0);
    spawn_dungeon_exit_portal(
        commands,
        meshes,
        trim_mat.clone(),
        spec.anchor_id,
        origin + rot * Vec3::new(0.0, 2.4, -49.0),
        dungeon_return_positions(outward_center, rot * Vec3::X, rot * Vec3::Z),
    );

    for side in [-1.0_f32, 1.0] {
        let closed = origin + rot * Vec3::new(side * 2.75, 3.7, -62.7);
        let open = origin + rot * Vec3::new(side * 8.4, 3.7, -62.7);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(5.1, 7.4, 1.0))),
                material: MeshMaterial3d(door_mat.clone()),
                transform: Transform::from_translation(closed).with_rotation(rot),
                ..default()
            },
            WorldGeometry,
            DungeonGateDoor {
                gate_id: spec.anchor_id,
                chapter: spec.chapter,
                closed,
                open,
            },
            crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
            crate::engine::physics::prelude::Collider::cuboid(2.55, 3.7, 0.5),
        ));
    }

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(1.25, 3.2))),
            material: MeshMaterial3d(trim_mat),
            transform: Transform::from_translation(entry + Vec3::Y * 1.4),
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_dungeon_block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    origin: Vec3,
    rot: Quat,
    local: Vec3,
    size: Vec3,
    walkable: bool,
) {
    spawn_castle_block(
        commands,
        meshes,
        material,
        origin + rot * local,
        size,
        rot,
        true,
        walkable,
    );
}

fn spawn_dungeon_pillar(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    origin: Vec3,
    rot: Quat,
    local: Vec3,
    phase: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(1.4, 4.8))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(origin + rot * local)
                .with_rotation(rot * Quat::from_rotation_y(phase * 0.4)),
            ..default()
        },
        WorldGeometry,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(2.4, 1.4),
    ));
}

fn spawn_dungeon_hazard_strips(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    origin: Vec3,
    rot: Quat,
    theme: DragonDungeonTheme,
) {
    let z_offsets: &[f32] = match theme {
        DragonDungeonTheme::Garden => &[-20.0, 4.0, 27.0],
        DragonDungeonTheme::CrownIce | DragonDungeonTheme::Icebreaker => &[-34.0, -2.0, 36.0],
        _ => &[-24.0, 8.0, 34.0],
    };
    for (i, z) in z_offsets.iter().enumerate() {
        spawn_castle_block(
            commands,
            meshes,
            material.clone(),
            origin + rot * Vec3::new(if i % 2 == 0 { -2.0 } else { 2.0 }, 0.5, *z),
            Vec3::new(12.0, 0.24, 2.2),
            rot,
            false,
            false,
        );
    }
}

fn spawn_dungeon_moving_bridge(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    origin: Vec3,
    rot: Quat,
    chapter: u8,
) {
    let size = Vec3::new(5.4, 0.55, 5.4);
    for (i, x) in [-6.0_f32, 0.0, 6.0].into_iter().enumerate() {
        let start = origin + rot * Vec3::new(x, 2.1, 3.0);
        let end = origin + rot * Vec3::new(x, 4.2, 3.0 + if i % 2 == 0 { 8.0 } else { -8.0 });
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(material.clone()),
                transform: Transform::from_translation(start).with_rotation(rot),
                ..default()
            },
            crate::engine::physics::prelude::Collider::cuboid(
                size.x * 0.5,
                size.y * 0.5,
                size.z * 0.5,
            ),
            crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
            WorldGeometry,
            WalkableSurface,
            MovingPlatform {
                start,
                end,
                speed: 2.2 + i as f32 * 0.35,
                phase: chapter as f32 * 0.4 + i as f32,
                size,
            },
        ));
    }
}

fn spawn_dungeon_turrets(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    body_mat: Handle<StandardMaterial>,
    beam_mat: Handle<StandardMaterial>,
    origin: Vec3,
    rot: Quat,
    chapter: u8,
) {
    for (i, local) in [Vec3::new(-15.0, 2.7, 34.0), Vec3::new(15.0, 2.7, 34.0)]
        .into_iter()
        .enumerate()
    {
        let turret = commands
            .spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cylinder::new(0.8, 1.4))),
                    material: MeshMaterial3d(body_mat.clone()),
                    transform: Transform::from_translation(origin + rot * local).with_rotation(rot),
                    ..default()
                },
                WorldGeometry,
                LaserTurret {
                    range: 48.0,
                    cooldown: 2.1 + i as f32 * 0.25,
                    cooldown_timer: chapter as f32 * 0.12 + i as f32 * 0.4,
                    windup: 0.50,
                    windup_timer: 0.0,
                    locked_target: None,
                    damage: 7.0 + chapter as f32 * 0.35,
                    beam_material: beam_mat.clone(),
                },
                crate::engine::physics::prelude::RigidBody::Fixed,
                crate::engine::physics::prelude::Collider::cylinder(0.7, 0.8),
            ))
            .id();
        commands.entity(turret).with_children(|parent| {
            parent.spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.25, 0.24, 1.8))),
                material: MeshMaterial3d(beam_mat.clone()),
                transform: Transform::from_xyz(0.0, 0.32, -0.85),
                ..default()
            });
        });
    }
}

// ── Terrain ───────────────────────────────────────────────────────────────────

const TERRAIN_WORLD_SIZE: f32 = EVEREST_RANGE_WORLD_SIZE;
const TERRAIN_MESH_RESOLUTION: usize = 320;
const TERRAIN_MESH_CELL_SIZE: f32 = TERRAIN_WORLD_SIZE / TERRAIN_MESH_RESOLUTION as f32;
const MAX_CACHED_WORLD_SEEDS: usize = 4;
const EVEREST_HEIGHTMAP_BYTES: &[u8] = include_bytes!("../../assets/terrain/everest.png");
const RACE_HEIGHTMAP_BYTES: &[u8] = include_bytes!("../../assets/terrain/RACE.png");
static EVEREST_HEIGHTMAP: OnceLock<Option<EverestHeightmap>> = OnceLock::new();
static RACE_HEIGHTMAP: OnceLock<Option<EverestHeightmap>> = OnceLock::new();
static TERRAIN_MESH_HEIGHT_GRIDS: OnceLock<Mutex<BoundedSeedCache<Vec<f32>>>> = OnceLock::new();
static SKY_ROAD_ACCESS_CORRIDORS: OnceLock<Mutex<BoundedSeedCache<Vec<(Vec2, Vec3)>>>> =
    OnceLock::new();

/// Small LRU used by world-generation caches. Forge may preview many seeds in
/// one process; keeping only recent seeds prevents those previews from growing
/// memory for the lifetime of the application.
struct BoundedSeedCache<T> {
    capacity: usize,
    values: HashMap<u64, Arc<T>>,
    recency: VecDeque<u64>,
}

impl<T> BoundedSeedCache<T> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            values: HashMap::new(),
            recency: VecDeque::new(),
        }
    }

    fn get(&mut self, seed: u64) -> Option<Arc<T>> {
        let value = Arc::clone(self.values.get(&seed)?);
        self.recency.retain(|cached| *cached != seed);
        self.recency.push_back(seed);
        Some(value)
    }

    fn insert(&mut self, seed: u64, value: Arc<T>) -> Arc<T> {
        if let Some(existing) = self.get(seed) {
            return existing;
        }
        while self.values.len() >= self.capacity {
            let Some(oldest) = self.recency.pop_front() else {
                break;
            };
            self.values.remove(&oldest);
        }
        self.recency.push_back(seed);
        self.values.insert(seed, Arc::clone(&value));
        value
    }
}

const ROUTE_CORE_AURORA: &[(f32, f32)] = &[
    (22.0, 28.0),
    (1300.0, 540.0),
    (3200.0, 920.0),
    (5400.0, 1220.0),
    (6600.0, 700.0),
    (8500.0, 4800.0),
];
const ROUTE_CORE_EVEREST: &[(f32, f32)] = &[
    (22.0, 28.0),
    (-1250.0, -840.0),
    (-3300.0, -2750.0),
    (-5750.0, -5150.0),
    (-8200.0, -7550.0),
];
const ROUTE_SOUTH_GLACIER: &[(f32, f32)] = &[
    (980.0, -660.0),
    (520.0, -2300.0),
    (1600.0, -4700.0),
    (300.0, -7800.0),
    (3300.0, -8400.0),
];
const ROUTE_NORTHWEST_GLACIER: &[(f32, f32)] = &[
    (22.0, 28.0),
    (-900.0, 2700.0),
    (-3300.0, 4300.0),
    (-5700.0, 5000.0),
    (-7800.0, 4200.0),
];
const ROUTE_NORTH_HIGH_PASS: &[(f32, f32)] = &[
    (2200.0, 1600.0),
    (5000.0, 3000.0),
    (7600.0, 3800.0),
    (9000.0, 6200.0),
];
const ROUTE_WEST_TEMPLE_PASS: &[(f32, f32)] = &[
    (-900.0, 2700.0),
    (-2800.0, 3300.0),
    (-6200.0, 6500.0),
    (-9000.0, 8000.0),
];
const ROUTE_EAST_ROCKIES_PASS: &[(f32, f32)] = &[
    (3000.0, -500.0),
    (5200.0, -1400.0),
    (6500.0, -3000.0),
    (8500.0, -5200.0),
];
const ROUTE_CROWN_UNDERPASS: &[(f32, f32)] =
    &[(-2500.0, -2300.0), (-4300.0, -6100.0), (-6500.0, -8600.0)];
// Cross-links turn the original spoke-like set of mountain roads into a
// navigable mesh.  Every connector terminates on an existing authored node so
// there are no almost-connected junctions or collider gaps at intersections.
const ROUTE_LINK_AURORA_NORTH: &[(f32, f32)] =
    &[(3200.0, 920.0), (3900.0, 2050.0), (5000.0, 3000.0)];
const ROUTE_LINK_CORE_SOUTH: &[(f32, f32)] =
    &[(-1250.0, -840.0), (-420.0, -1680.0), (520.0, -2300.0)];
const ROUTE_LINK_WEST_CROWN: &[(f32, f32)] =
    &[(-3300.0, -2750.0), (-4680.0, -4300.0), (-4300.0, -6100.0)];
const ROUTE_LINK_EAST_SOUTH: &[(f32, f32)] =
    &[(5200.0, -1400.0), (3900.0, -3150.0), (1600.0, -4700.0)];
const ROUTE_LINK_NORTHWEST_AURORA: &[(f32, f32)] =
    &[(-3300.0, 4300.0), (-900.0, 1500.0), (1300.0, 540.0)];
const ROUTE_LINK_OUTER_NORTH: &[(f32, f32)] =
    &[(-6200.0, 6500.0), (-7700.0, 6000.0), (-7800.0, 4200.0)];
// Starfall Grand Raceway. The imported RACE heightmap occupies the southeast
// range; these two access roads join it to the existing Rockies and Antarctic
// trunks, while the wide closed circuit and infield link make it a real map
// district instead of an isolated arena.
const ROUTE_RACE_NORTH_ACCESS: &[(f32, f32)] =
    &[(8500.0, -5200.0), (8400.0, -6100.0), (8150.0, -6500.0)];
const ROUTE_RACE_CIRCUIT: &[(f32, f32)] = &[
    (8150.0, -6500.0),
    (7600.0, -6350.0),
    (6900.0, -6550.0),
    (6200.0, -7100.0),
    (6000.0, -7900.0),
    (6350.0, -8600.0),
    (7100.0, -8950.0),
    (8000.0, -8750.0),
    (8650.0, -8150.0),
    (8800.0, -7350.0),
    (8450.0, -6800.0),
    (8150.0, -6500.0),
];
const ROUTE_RACE_WEST_ACCESS: &[(f32, f32)] =
    &[(3300.0, -8400.0), (5000.0, -8800.0), (6350.0, -8600.0)];
const ROUTE_RACE_INFIELD: &[(f32, f32)] =
    &[(6900.0, -6550.0), (7100.0, -7600.0), (7100.0, -8950.0)];
const BASE_MOUNTAIN_ROUTE_COUNT: usize = 14;
const RACE_CIRCUIT_ROUTE_INDEX: usize = BASE_MOUNTAIN_ROUTE_COUNT + 1;
const MOUNTAIN_ROUTES: &[&[(f32, f32)]] = &[
    ROUTE_CORE_AURORA,
    ROUTE_CORE_EVEREST,
    ROUTE_SOUTH_GLACIER,
    ROUTE_NORTHWEST_GLACIER,
    ROUTE_NORTH_HIGH_PASS,
    ROUTE_WEST_TEMPLE_PASS,
    ROUTE_EAST_ROCKIES_PASS,
    ROUTE_CROWN_UNDERPASS,
    ROUTE_LINK_AURORA_NORTH,
    ROUTE_LINK_CORE_SOUTH,
    ROUTE_LINK_WEST_CROWN,
    ROUTE_LINK_EAST_SOUTH,
    ROUTE_LINK_NORTHWEST_AURORA,
    ROUTE_LINK_OUTER_NORTH,
    ROUTE_RACE_NORTH_ACCESS,
    ROUTE_RACE_CIRCUIT,
    ROUTE_RACE_WEST_ACCESS,
    ROUTE_RACE_INFIELD,
];

struct EverestHeightmap {
    width: usize,
    height: usize,
    values: Vec<f32>,
}

impl EverestHeightmap {
    fn sample(&self, u: f32, v: f32) -> f32 {
        let x = u.clamp(0.0, 1.0) * (self.width.saturating_sub(1)) as f32;
        let y = v.clamp(0.0, 1.0) * (self.height.saturating_sub(1)) as f32;
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let tx = x - x0 as f32;
        let ty = y - y0 as f32;

        let a = self.values[y0 * self.width + x0];
        let b = self.values[y0 * self.width + x1];
        let c = self.values[y1 * self.width + x0];
        let d = self.values[y1 * self.width + x1];
        let top = a + (b - a) * tx;
        let bottom = c + (d - c) * tx;
        top + (bottom - top) * ty
    }

    fn smooth_sample(&self, u: f32, v: f32) -> f32 {
        let du = 1.0 / self.width.max(2) as f32;
        let dv = 1.0 / self.height.max(2) as f32;
        let mut sum = self.sample(u, v) * 4.0;
        let mut weight = 4.0;
        for &(ox, oy, w) in &[
            (-1.0_f32, 0.0_f32, 2.0_f32),
            (1.0, 0.0, 2.0),
            (0.0, -1.0, 2.0),
            (0.0, 1.0, 2.0),
            (-1.0, -1.0, 1.0),
            (1.0, -1.0, 1.0),
            (-1.0, 1.0, 1.0),
            (1.0, 1.0, 1.0),
        ] {
            sum += self.sample(u + ox * du, v + oy * dv) * w;
            weight += w;
        }
        sum / weight
    }
}

fn mountain_routes() -> &'static [&'static [(f32, f32)]] {
    MOUNTAIN_ROUTES
}

fn everest_heightmap() -> Option<&'static EverestHeightmap> {
    EVEREST_HEIGHTMAP
        .get_or_init(load_everest_heightmap)
        .as_ref()
}

fn load_everest_heightmap() -> Option<EverestHeightmap> {
    let image = Image::from_buffer(
        EVEREST_HEIGHTMAP_BYTES,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        RenderAssetUsages::default(),
    )
    .ok()?;

    heightmap_from_image(&image)
}

fn race_heightmap() -> Option<&'static EverestHeightmap> {
    RACE_HEIGHTMAP.get_or_init(load_race_heightmap).as_ref()
}

fn load_race_heightmap() -> Option<EverestHeightmap> {
    let image = Image::from_buffer(
        RACE_HEIGHTMAP_BYTES,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        RenderAssetUsages::default(),
    )
    .ok()?;

    heightmap_from_image(&image)
}

fn heightmap_from_image(image: &Image) -> Option<EverestHeightmap> {
    let width = image.width() as usize;
    let height = image.height() as usize;
    if width < 2 || height < 2 {
        return None;
    }

    let mut min_value = f32::MAX;
    let mut max_value = f32::MIN;
    let mut values = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let value = heightmap_luminance(image, y * width + x)?;
            min_value = min_value.min(value);
            max_value = max_value.max(value);
            values.push(value);
        }
    }

    let range = max_value - min_value;
    if range > f32::EPSILON {
        for value in values.iter_mut() {
            *value = (*value - min_value) / range;
        }
    }

    Some(EverestHeightmap {
        width,
        height,
        values,
    })
}

fn heightmap_luminance(image: &Image, pixel_index: usize) -> Option<f32> {
    let data = image.data.as_ref()?;
    match image.texture_descriptor.format {
        TextureFormat::R8Unorm | TextureFormat::R8Uint => {
            data.get(pixel_index).map(|v| *v as f32 / u8::MAX as f32)
        }
        TextureFormat::Rg8Unorm | TextureFormat::Rg8Uint => data
            .get(pixel_index * 2)
            .map(|v| *v as f32 / u8::MAX as f32),
        TextureFormat::R16Unorm | TextureFormat::R16Uint => {
            let offset = pixel_index * 2;
            let bytes = data.get(offset..offset + 2)?;
            Some(u16::from_le_bytes([bytes[0], bytes[1]]) as f32 / u16::MAX as f32)
        }
        TextureFormat::Rg16Unorm | TextureFormat::Rg16Uint => {
            let offset = pixel_index * 4;
            let bytes = data.get(offset..offset + 2)?;
            Some(u16::from_le_bytes([bytes[0], bytes[1]]) as f32 / u16::MAX as f32)
        }
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8Uint | TextureFormat::Rgba8UnormSrgb => {
            let offset = pixel_index * 4;
            let bytes = data.get(offset..offset + 3)?;
            Some(luminance(bytes[0], bytes[1], bytes[2]))
        }
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
            let offset = pixel_index * 4;
            let bytes = data.get(offset..offset + 3)?;
            Some(luminance(bytes[2], bytes[1], bytes[0]))
        }
        _ => None,
    }
}

fn luminance(r: u8, g: u8, b: u8) -> f32 {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / u8::MAX as f32
}

/// Smooth hermite interpolation.
fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn edge_fade01(t: f32, feather: f32) -> f32 {
    smoothstep(0.0, feather, t) * (1.0 - smoothstep(1.0 - feather, 1.0, t))
}

fn terrain_fbm_signed(x: f32, z: f32, seed: u64, salt: u64, base_freq: f32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut freq = base_freq;

    for octave in 0..4u64 {
        let idx = salt + octave * 37;
        let angle = seeded(seed, idx) * TAU;
        let phase = seeded(seed, idx + 11) * TAU;
        let dir = Vec2::new(angle.cos(), angle.sin());
        let cross = Vec2::new(-dir.y, dir.x);
        let wave_a = (x * dir.x + z * dir.y) * freq + phase;
        let wave_b = (x * cross.x + z * cross.y) * freq * 0.73 + phase * 1.37;
        sum += (wave_a.sin() * 0.62 + wave_b.cos() * 0.38) * amp;
        norm += amp;
        amp *= 0.52;
        freq *= 2.13;
    }

    if norm > f32::EPSILON {
        (sum / norm).clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn terrain_fbm_01(x: f32, z: f32, seed: u64, salt: u64, base_freq: f32) -> f32 {
    (terrain_fbm_signed(x, z, seed, salt, base_freq) * 0.5 + 0.5).clamp(0.0, 1.0)
}

fn terrain_domain_warp(
    x: f32,
    z: f32,
    seed: u64,
    salt: u64,
    strength: f32,
    base_freq: f32,
) -> Vec2 {
    Vec2::new(
        terrain_fbm_signed(x, z, seed, salt, base_freq),
        terrain_fbm_signed(x, z, seed, salt + 97, base_freq * 1.17),
    ) * strength
}

fn distance_to_segment_xz(px: f32, pz: f32, ax: f32, az: f32, bx: f32, bz: f32) -> f32 {
    let (_, distance) = project_point_to_segment_xz(px, pz, ax, az, bx, bz);
    distance
}

fn project_point_to_segment_xz(
    px: f32,
    pz: f32,
    ax: f32,
    az: f32,
    bx: f32,
    bz: f32,
) -> (Vec2, f32) {
    let vx = bx - ax;
    let vz = bz - az;
    let len_sq = vx * vx + vz * vz;
    if len_sq <= f32::EPSILON {
        let point = Vec2::new(ax, az);
        return (point, Vec2::new(px - ax, pz - az).length());
    }

    let t = (((px - ax) * vx + (pz - az) * vz) / len_sq).clamp(0.0, 1.0);
    let sx = ax + vx * t;
    let sz = az + vz * t;
    let point = Vec2::new(sx, sz);
    (point, Vec2::new(px - sx, pz - sz).length())
}

fn mountain_route_influence(x: f32, z: f32) -> f32 {
    let mut influence: f32 = 0.0;
    for route in mountain_routes() {
        for pair in route.windows(2) {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let distance = distance_to_segment_xz(x, z, ax, az, bx, bz);
            influence = influence.max(1.0 - smoothstep(62.0, 280.0, distance));
        }
    }
    influence
}

#[derive(Debug, Clone, Copy)]
struct RouteProjection {
    point: Vec2,
    distance: f32,
    tangent: Vec2,
}

/// Rounded centerline of every mountain route plus its expanded AABB
/// (min_x, min_z, max_x, max_z). Deterministic; computed once. Terrain
/// carving, prop avoidance, and settlement spur projection all measure
/// against these rounded polylines so they agree exactly with the deck
/// geometry the routes actually spawn.
fn rounded_mountain_route_network() -> &'static [(Vec<(f32, f32)>, [f32; 4])] {
    static NETWORK: OnceLock<Vec<(Vec<(f32, f32)>, [f32; 4])>> = OnceLock::new();
    NETWORK.get_or_init(|| {
        mountain_routes()
            .iter()
            .map(|route| {
                let rounded = rounded_speed_road_route(route);
                let mut aabb = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                for &(px, pz) in &rounded {
                    aabb[0] = aabb[0].min(px);
                    aabb[1] = aabb[1].min(pz);
                    aabb[2] = aabb[2].max(px);
                    aabb[3] = aabb[3].max(pz);
                }
                (rounded, aabb)
            })
            .collect()
    })
}

fn nearest_mountain_route_point(x: f32, z: f32) -> Option<RouteProjection> {
    let mut best: Option<RouteProjection> = None;
    for (route, aabb) in rounded_mountain_route_network() {
        // Reject whole routes whose bounding box cannot beat the best hit —
        // rounding multiplies segment counts, so this prefilter keeps the
        // terrain-sampling hot path close to its previous cost.
        let current = best
            .map(|projection| projection.distance)
            .unwrap_or(f32::MAX);
        let dx = (aabb[0] - x).max(x - aabb[2]).max(0.0);
        let dz = (aabb[1] - z).max(z - aabb[3]).max(0.0);
        if dx * dx + dz * dz > current * current {
            continue;
        }
        for pair in route.windows(2) {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let (point, distance) = project_point_to_segment_xz(x, z, ax, az, bx, bz);
            if best.is_none_or(|projection| distance < projection.distance) {
                best = Some(RouteProjection {
                    point,
                    distance,
                    tangent: Vec2::new(bx - ax, bz - az).normalize_or_zero(),
                });
            }
        }
    }
    best
}

fn distance_to_mountain_route_network(x: f32, z: f32) -> f32 {
    nearest_mountain_route_point(x, z)
        .map(|projection| projection.distance)
        .unwrap_or(f32::MAX)
}

fn everest_heightmap_relief(x: f32, z: f32, seed: u64) -> f32 {
    let Some(heightmap) = everest_heightmap() else {
        return 0.0;
    };

    // The Everest PNG is the world-scale range model. World X/Z maps across the
    // full image so the whole explorable space follows the imported mountain
    // photo instead of using it as a small decorative patch.
    let u = (x + EVEREST_RANGE_HALF_EXTENT) / EVEREST_RANGE_WORLD_SIZE;
    let v = (EVEREST_RANGE_HALF_EXTENT - z) / EVEREST_RANGE_WORLD_SIZE;
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return 0.0;
    }

    let edge = edge_fade01(u, 0.025) * edge_fade01(v, 0.025);
    let warp = terrain_domain_warp(x, z, seed, 1_901, 0.020, 0.00046) * edge;
    let warped_u = (u + warp.x).clamp(0.0, 1.0);
    let warped_v = (v + warp.y).clamp(0.0, 1.0);
    let raw_center = heightmap.sample(u, v);
    let smooth = heightmap.smooth_sample(u, v);
    let warped = heightmap.sample(warped_u, warped_v);
    let raw = (raw_center * 0.30 + smooth * 0.45 + warped * 0.25).clamp(0.0, 1.0);

    let du = 2.0 / heightmap.width.max(2) as f32;
    let dv = 2.0 / heightmap.height.max(2) as f32;
    let grad_x = heightmap.sample((warped_u + du).clamp(0.0, 1.0), warped_v)
        - heightmap.sample((warped_u - du).clamp(0.0, 1.0), warped_v);
    let grad_z = heightmap.sample(warped_u, (warped_v + dv).clamp(0.0, 1.0))
        - heightmap.sample(warped_u, (warped_v - dv).clamp(0.0, 1.0));
    let gradient = (grad_x * grad_x + grad_z * grad_z).sqrt();

    let foothill = smoothstep(0.06, 0.48, raw);
    let ridge = smoothstep(0.22, 0.86, raw);
    let high_ridge = smoothstep(0.56, 0.96, raw);
    let snowline = smoothstep(0.70, 0.985, raw);
    let fractured = smoothstep(0.020, 0.110, gradient) * smoothstep(0.18, 0.82, raw);
    let ridge_noise = terrain_fbm_01(x, z, seed, 1_973, 0.00135);
    let scree_noise = terrain_fbm_signed(x, z, seed, 2_011, 0.0062);
    let drainage_wave = (x * 0.00039 - z * 0.00027
        + terrain_fbm_signed(x, z, seed, 2_037, 0.00055) * 1.65)
        .sin()
        .abs();
    let ravine_cut = (1.0 - smoothstep(0.055, 0.24, drainage_wave)) * ridge * 54.0;
    let valley_cut = (1.0 - smoothstep(0.10, 0.38, raw)) * 34.0 + ravine_cut;
    let terraced_raw =
        ((raw * 11.0 + terrain_fbm_signed(x, z, seed, 2_059, 0.0011) * 0.22).floor() / 11.0)
            .clamp(0.0, 1.0);
    let terrace_offset = (terraced_raw - raw) * 82.0 * fractured * (1.0 - snowline * 0.35);
    let fold_lift = fractured * (28.0 + high_ridge * 105.0) * (0.72 + ridge_noise * 0.42);
    let scree_lift = scree_noise * 18.0 * smoothstep(0.32, 0.84, raw) * (1.0 - snowline * 0.45);

    let crown_distance = ((x + 8500.0).powi(2) + (z + 7800.0).powi(2)).sqrt();
    let authored_everest_summit = smoothstep(1800.0, 0.0, crown_distance) * 340.0;

    edge * (foothill * 130.0
        + ridge * 280.0
        + high_ridge * 360.0
        + snowline * 180.0
        + fold_lift
        + scree_lift
        + terrace_offset
        - valley_cut)
        + authored_everest_summit
}

const RACE_REGION_CENTER: Vec2 = Vec2::new(7_250.0, -7_650.0);
const RACE_REGION_HALF_EXTENT: f32 = 1_950.0;

/// Sample the new race terrain as a local map tile. The feathered replacement
/// preserves the surrounding Everest terrain at the tile edge, while the
/// imported dark basin becomes a broad drivable floor ringed by high ridges.
fn race_heightmap_surface(x: f32, z: f32, seed: u64) -> Option<(f32, f32)> {
    let heightmap = race_heightmap()?;
    let u =
        (x - (RACE_REGION_CENTER.x - RACE_REGION_HALF_EXTENT)) / (RACE_REGION_HALF_EXTENT * 2.0);
    let v =
        ((RACE_REGION_CENTER.y + RACE_REGION_HALF_EXTENT) - z) / (RACE_REGION_HALF_EXTENT * 2.0);
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return None;
    }

    let blend = edge_fade01(u, 0.14) * edge_fade01(v, 0.14);
    let raw =
        (heightmap.sample(u, v) * 0.35 + heightmap.smooth_sample(u, v) * 0.65).clamp(0.0, 1.0);
    let basin = smoothstep(0.08, 0.82, raw).powf(1.28);
    let detail = terrain_fbm_signed(x, z, seed, 3_301, 0.0022) * 7.5;
    Some(((34.0 + basin * 385.0 + detail).max(18.0), blend))
}

/// Layered sine-wave height at world position (x, z).
/// The city centre is kept near-flat; elevation builds toward the outer ring.
fn terrain_height_uncarved(x: f32, z: f32, seed: u64) -> f32 {
    // Phase offset so every world seed looks distinct.
    let so = seeded(seed, 7) * std::f32::consts::TAU;

    // Distance from city centre.
    let dist = (x * x + z * z).sqrt();

    // 1.0 in the city core, 0.0 beyond the outer threshold.
    let city_flat = 1.0 - smoothstep(180.0, 620.0, dist);

    // ── Layered octaves ─────────────────────────────────────────────────────
    // Large rolling hills (low frequency, high amplitude).
    let large = (x * 0.00062 + so).sin() * (z * 0.00078 + so * 0.7).cos() * 42.0;
    // Medium undulations.
    let med = (x * 0.0021 + z * 0.0018 + so * 0.3).sin() * 15.0;
    // Small surface variation.
    let small = (x * 0.006 - z * 0.0055 + so * 0.5).sin() * 4.2;
    // Micro detail.
    let micro = (x * 0.018 + z * 0.016).sin() * 1.1;

    // ── Ridge lines ─────────────────────────────────────────────────────────
    // abs(sin) creates sharp ridges; scale by distance to keep them in the outer world.
    let ridge_mask = smoothstep(1000.0, 6200.0, dist);
    let ridge = (x * 0.00125 + z * 0.001 + so).sin().abs() * 28.0 * ridge_mask;
    let warp = terrain_domain_warp(x, z, seed, 2_401, 230.0, 0.00038);
    let folded = terrain_fbm_signed(x + warp.x, z + warp.y, seed, 2_433, 0.00062).abs();
    let knife_ridges = (1.0 - smoothstep(0.12, 0.52, folded)) * 68.0 * ridge_mask;
    let basin_noise = terrain_fbm_01(x - warp.y * 0.35, z + warp.x * 0.35, seed, 2_467, 0.00034);
    let hanging_valleys =
        (1.0 - smoothstep(0.18, 0.58, basin_noise)) * 42.0 * smoothstep(1300.0, 7800.0, dist);

    let base = large + med + small + micro + ridge + knife_ridges - hanging_valleys;

    // Terrain rises significantly toward the world perimeter.
    let edge_lift = smoothstep(5200.0, 9800.0, dist) * 88.0;

    // Mountain peaks cluster in the four corners.
    let mut peak_boost = 0.0f32;
    for &(cx, cz) in &[
        (8600.0f32, 8600.0),
        (-8600.0, 8600.0),
        (8600.0, -8600.0),
        (-8600.0, -8600.0),
    ] {
        let d = ((x - cx) * (x - cx) + (z - cz) * (z - cz)).sqrt();
        peak_boost += smoothstep(2600.0, 0.0, d) * 180.0;
    }

    // Qilian foothills — east of city (positive X), gentle cartoon-cartoon hill range
    let qilian_blend = smoothstep(2800.0, 9000.0, x);
    let qilian = qilian_blend * 110.0 + (z * 0.001 + so).sin() * qilian_blend * 24.0;

    // Tibetan plateau — southwest quadrant (neg X, neg Z)
    let plat_x = smoothstep(-2400.0, -8600.0, x); // 0 near centre, rises west
    let plat_z = smoothstep(-1800.0, -7800.0, z); // 0 near centre, rises south
    let plateau = plat_x * plat_z * 160.0;

    // Full-range Everest heightmap relief, with an authored summit boost near Collosar.
    let everest = everest_heightmap_relief(x, z, seed);

    let mut raw_height = base + edge_lift + peak_boost + qilian + plateau + everest;
    if let Some((race_height, race_blend)) = race_heightmap_surface(x, z, seed) {
        raw_height = raw_height.lerp(race_height, race_blend);
    }
    raw_height += (raw_height * 0.034 + terrain_fbm_signed(x, z, seed, 2_503, 0.00115) * 0.82)
        .sin()
        * 8.0
        * smoothstep(180.0, 680.0, raw_height)
        * smoothstep(1600.0, 9000.0, dist);
    let route_cut = mountain_route_influence(x, z)
        * smoothstep(700.0, 6200.0, dist)
        * (28.0 + raw_height.max(0.0) * 0.06);
    let shaped_height = (raw_height - route_cut).max(0.0);

    // Flatten the city zone; outer areas get full amplitude. Keep the terrain
    // on or above the invisible gameplay floor so collision and visuals agree.
    (shaped_height * (1.0 - city_flat)).max(0.0)
}

fn secret_cave_terrain_inset(x: f32, z: f32, seed: u64, base_height: f32) -> f32 {
    let point = Vec2::new(x, z);
    let mut height = base_height;
    for spec in SECRET_CAVE_LOCATIONS {
        let forward = Vec2::new(spec.yaw.sin(), spec.yaw.cos());
        let start = Vec2::new(spec.x, spec.z) - forward * 12.0;
        let end = Vec2::new(spec.x, spec.z)
            + forward * (spec.length + SECRET_CAVE_CHAMBER_HALF_LENGTH + 12.0);
        let axis = end - start;
        let axis_len_sq = axis.length_squared().max(0.001);
        let t = ((point - start).dot(axis) / axis_len_sq).clamp(0.0, 1.0);
        let nearest = start + axis * t;
        let distance = point.distance(nearest);

        // The terrain mesh is sampled every 62.5 units. This inset must be
        // wider than a cell diagonal so all triangles beneath the much smaller
        // cave geometry receive compatible heights rather than slicing through
        // the room between sparse mesh vertices.
        let influence = 1.0 - smoothstep(160.0, 240.0, distance);
        if influence <= 0.0 {
            continue;
        }

        let entrance = Vec2::new(spec.x, spec.z);
        let entrance_height = terrain_height_uncarved(entrance.x, entrance.y, seed);
        height = height.lerp(entrance_height, influence);
    }
    height.max(0.0)
}

fn perimeter_coast_terrain_inset(x: f32, z: f32, base_height: f32) -> f32 {
    let point = Vec2::new(x, z);
    let mut height = base_height;
    for island in TRAVEL_ISLANDS {
        let distance = point.distance(island.coast_dock);
        let influence = 1.0 - smoothstep(170.0, 540.0, distance);
        if influence > 0.0 {
            height = height.lerp(PERIMETER_OCEAN_LEVEL - 1.4, influence);
        }
    }
    height
}

fn mountain_lake_surface_y(center_x: f32, center_z: f32, seed: u64) -> f32 {
    (terrain_height_uncarved(center_x, center_z, seed) - 2.0).max(1.0)
}

fn mountain_lake_terrain_inset(x: f32, z: f32, seed: u64, base_height: f32) -> f32 {
    let mut height = base_height;
    for &(_, center_x, center_z, radius_x, radius_z) in MOUNTAIN_LAKES {
        let normalized = Vec2::new((x - center_x) / radius_x, (z - center_z) / radius_z).length();
        let influence = 1.0 - smoothstep(0.72, 1.06, normalized);
        if influence <= 0.0 {
            continue;
        }
        let basin_floor = mountain_lake_surface_y(center_x, center_z, seed) - 4.5;
        height = height.lerp(height.min(basin_floor), influence);
    }
    height
}

fn final_terrain_insets(x: f32, z: f32, seed: u64, base_height: f32) -> f32 {
    let cave_height = secret_cave_terrain_inset(x, z, seed, base_height);
    let lake_height = mountain_lake_terrain_inset(x, z, seed, cave_height);
    perimeter_coast_terrain_inset(x, z, lake_height)
}

fn terrain_height(x: f32, z: f32, seed: u64) -> f32 {
    terrain_height_uncarved(x, z, seed)
}

pub(crate) fn terrain_surface_y(x: f32, z: f32, seed: u64) -> f32 {
    // Gameplay must query the same piecewise-planar surface used by the
    // rendered trimesh collider. Sampling the analytic height function here
    // can disagree by dozens of units on sharp ridges because the terrain mesh
    // only has one vertex every 62.5 world units.
    let grid_x = ((x + TERRAIN_WORLD_SIZE * 0.5) / TERRAIN_MESH_CELL_SIZE)
        .clamp(0.0, TERRAIN_MESH_RESOLUTION as f32);
    let grid_z = ((z + TERRAIN_WORLD_SIZE * 0.5) / TERRAIN_MESH_CELL_SIZE)
        .clamp(0.0, TERRAIN_MESH_RESOLUTION as f32);
    let cell_x = grid_x.floor().min((TERRAIN_MESH_RESOLUTION - 1) as f32);
    let cell_z = grid_z.floor().min((TERRAIN_MESH_RESOLUTION - 1) as f32);
    let tx = grid_x - cell_x;
    let tz = grid_z - cell_z;
    let stride = TERRAIN_MESH_RESOLUTION + 1;
    let ix = cell_x as usize;
    let iz = cell_z as usize;
    let heights = terrain_mesh_height_grid(seed);
    let h00 = heights[iz * stride + ix];
    let h10 = heights[iz * stride + ix + 1];
    let h01 = heights[(iz + 1) * stride + ix];
    let h11 = heights[(iz + 1) * stride + ix + 1];

    if tx + tz <= 1.0 {
        h00 + (h10 - h00) * tx + (h01 - h00) * tz
    } else {
        h11 + (h01 - h11) * (1.0 - tx) + (h10 - h11) * (1.0 - tz)
    }
}

fn terrain_mesh_height_grid(seed: u64) -> Arc<Vec<f32>> {
    let cache = TERRAIN_MESH_HEIGHT_GRIDS
        .get_or_init(|| Mutex::new(BoundedSeedCache::new(MAX_CACHED_WORLD_SEEDS)));
    if let Some(heights) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(seed)
    {
        return heights;
    }

    let stride = TERRAIN_MESH_RESOLUTION + 1;
    let mut heights = Vec::with_capacity(stride * stride);
    for zi in 0..=TERRAIN_MESH_RESOLUTION {
        for xi in 0..=TERRAIN_MESH_RESOLUTION {
            let x = xi as f32 * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5;
            let z = zi as f32 * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5;
            heights.push(terrain_mesh_vertex_height(x, z, seed));
        }
    }
    let heights = Arc::new(heights);
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.insert(seed, heights)
}

/// Cut a broad, blended shelf into the mountain mesh beneath authored roads.
/// Without this cross-slope terrace, a 64-unit-wide unbanked deck must float
/// above whichever edge of a steep mountain is highest. The shelf lets the
/// road genuinely follow and climb the mountain while leaving cliffs intact
/// outside the travel corridor.
fn terrain_mesh_vertex_height(x: f32, z: f32, seed: u64) -> f32 {
    let height = terrain_height(x, z, seed);
    let Some(projection) = nearest_mountain_route_point(x, z) else {
        return final_terrain_insets(x, z, seed, height);
    };
    if projection.distance >= 190.0 {
        return final_terrain_insets(x, z, seed, height);
    }
    // Smooth the shelf longitudinally so the mesh itself provides a curving
    // mountain ascent instead of preserving every sharp summit under the road.
    let sample = |offset: f32| {
        let point = projection.point + projection.tangent * offset;
        terrain_height(point.x, point.y, seed)
    };
    let route_height = (sample(-160.0)
        + sample(-80.0) * 2.0
        + sample(0.0) * 4.0
        + sample(80.0) * 2.0
        + sample(160.0))
        / 10.0;
    // The collider grid is 62.5 units wide, so the fully terraced band must be
    // wider than one cell or a route can pass between vertices without
    // affecting the actual mesh triangles beneath it.
    let shelf = 1.0 - smoothstep(96.0, 190.0, projection.distance);
    let road_terraced_height = height + (route_height - height) * shelf;
    // Cave interiors are the final terrain authority where a road shelf and a
    // cave mouth overlap; otherwise the road pass can reintroduce a mountain
    // wedge through an already validated dungeon floor.
    final_terrain_insets(x, z, seed, road_terraced_height)
}

fn mix_rgba(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

fn scale_rgba(a: [f32; 4], scale: f32) -> [f32; 4] {
    [
        (a[0] * scale).clamp(0.0, 1.0),
        (a[1] * scale).clamp(0.0, 1.0),
        (a[2] * scale).clamp(0.0, 1.0),
        a[3],
    ]
}

fn terrain_detail_noise(x: f32, z: f32, y: f32, seed: u64) -> f32 {
    let phase = seeded(seed, 313) * std::f32::consts::TAU;
    let broad = (x * 0.0017 + z * 0.0011 + phase).sin();
    let speckle = (x * 0.015 - z * 0.012 + phase * 0.37).sin();
    let strata = (y * 0.055 + x * 0.0025 - z * 0.0017).sin();
    let eroded = terrain_fbm_signed(x, z, seed, 2_701, 0.0028);
    ((broad * 0.34 + speckle * 0.22 + strata * 0.22 + eroded * 0.22) * 0.5 + 0.5).clamp(0.0, 1.0)
}

fn terrain_vertex_color(x: f32, z: f32, y: f32, normal: Vec3, seed: u64) -> [f32; 4] {
    let grass = [0.16, 0.34, 0.15, 1.0];
    let alpine_meadow = [0.34, 0.45, 0.26, 1.0];
    let exposed_rock = [0.34, 0.32, 0.29, 1.0];
    let dark_rock = [0.20, 0.19, 0.18, 1.0];
    let snow = [0.86, 0.91, 0.96, 1.0];
    let blue_ice = [0.62, 0.78, 0.88, 1.0];
    let trail = [0.38, 0.34, 0.27, 1.0];

    let slope = (1.0 - normal.y.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    let detail = terrain_detail_noise(x, z, y, seed);
    let altitude = smoothstep(100.0, 520.0, y);
    let rockline = smoothstep(0.10, 0.42, slope).max(smoothstep(360.0, 650.0, y) * 0.72);
    let snowline = smoothstep(470.0, 760.0, y) * (1.0 - smoothstep(0.44, 0.74, slope) * 0.45);
    let ice_line = smoothstep(620.0, 820.0, y) * smoothstep(0.18, 0.56, slope);
    let striation = smoothstep(0.63, 1.0, detail) * smoothstep(250.0, 760.0, y);

    let mut color = mix_rgba(grass, alpine_meadow, altitude * 0.82);
    color = mix_rgba(color, exposed_rock, rockline);
    color = mix_rgba(color, dark_rock, striation * 0.36);
    color = mix_rgba(color, snow, snowline);
    color = mix_rgba(color, blue_ice, ice_line * 0.28);
    let path_mask = mountain_route_influence(x, z);
    color = mix_rgba(color, trail, path_mask * 0.72);

    let mut color = scale_rgba(color, 0.90 + detail * 0.18);
    color[3] = path_mask;
    color
}

const TERRAIN_PATCH_CELLS: usize = 40;
const TERRAIN_PROXY_SAMPLE_STEP: usize = 4;

fn terrain_grid_normal(heights: &[f32], xi: usize, zi: usize) -> Vec3 {
    let res = TERRAIN_MESH_RESOLUTION;
    let stride = res + 1;
    let hx0 = heights[zi * stride + xi.saturating_sub(1)];
    let hx1 = heights[zi * stride + (xi + 1).min(res)];
    let hz0 = heights[zi.saturating_sub(1) * stride + xi];
    let hz1 = heights[(zi + 1).min(res) * stride + xi];
    let dx = Vec3::new(2.0 * TERRAIN_MESH_CELL_SIZE, hx1 - hx0, 0.0);
    let dz = Vec3::new(0.0, hz1 - hz0, 2.0 * TERRAIN_MESH_CELL_SIZE);
    dz.cross(dx).normalize()
}

fn terrain_patch_mesh(
    heights: &[f32],
    seed: u64,
    start_x: usize,
    start_z: usize,
    cells: usize,
    sample_step: usize,
) -> (Mesh, Vec3) {
    debug_assert!(sample_step > 0 && cells.is_multiple_of(sample_step));
    let res = TERRAIN_MESH_RESOLUTION;
    let stride = res + 1;
    let samples = cells / sample_step;
    let center_x = start_x + cells / 2;
    let center_z = start_z + cells / 2;
    let origin = Vec3::new(
        center_x as f32 * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5,
        heights[center_z * stride + center_x],
        center_z as f32 * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5,
    );
    let vertex_count = (samples + 1) * (samples + 1);
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);

    for local_z in 0..=samples {
        let zi = start_z + local_z * sample_step;
        for local_x in 0..=samples {
            let xi = start_x + local_x * sample_step;
            let wx = xi as f32 * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5;
            let wz = zi as f32 * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5;
            let wy = heights[zi * stride + xi];
            let normal = terrain_grid_normal(heights, xi, zi);
            positions.push([wx - origin.x, wy - origin.y, wz - origin.z]);
            normals.push(normal.to_array());
            uvs.push([xi as f32 / res as f32, zi as f32 / res as f32]);
            colors.push(terrain_vertex_color(wx, wz, wy, normal, seed));
        }
    }

    let mut indices = Vec::with_capacity(samples * samples * 6);
    let local_stride = samples + 1;
    for zi in 0..samples {
        for xi in 0..samples {
            let top_left = (zi * local_stride + xi) as u32;
            let top_right = top_left + 1;
            let bottom_left = top_left + local_stride as u32;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                bottom_left,
                top_right,
                top_right,
                bottom_left,
                bottom_right,
            ]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    (mesh, origin)
}

fn spawn_terrain(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    terrain_material: &Handle<TerrainMaterial>,
    seed: u64,
) {
    const RES: usize = TERRAIN_MESH_RESOLUTION;
    let h = terrain_mesh_height_grid(seed);

    // Preserve the exact full-resolution physics surface. Rendering is split
    // into spatial patches below, but traversal/collision semantics do not
    // change with camera distance.
    let mut collider_vertices = Vec::with_capacity((RES + 1) * (RES + 1));
    for zi in 0..=RES {
        for xi in 0..=RES {
            let wx = xi as f32 * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5;
            let wz = zi as f32 * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5;
            let wy = h[zi * (RES + 1) + xi];
            collider_vertices.push(Vec3::new(wx, wy, wz));
        }
    }

    let mut collider_triangles = Vec::with_capacity(RES * RES * 2);
    for zi in 0..RES {
        for xi in 0..RES {
            let tl = (zi * (RES + 1) + xi) as u32;
            let tr = tl + 1;
            let bl = tl + (RES + 1) as u32;
            let br = bl + 1;
            collider_triangles.push([tl, bl, tr]);
            collider_triangles.push([tr, bl, br]);
        }
    }

    let vert_count = collider_vertices.len();
    let tri_count = collider_triangles.len();
    let terrain_collider =
        crate::engine::physics::prelude::Collider::trimesh(collider_vertices, collider_triangles);
    let terrain_root = commands
        .spawn((SpatialBundle::default(), WorldGeometry, WalkableSurface))
        .id();
    match terrain_collider {
        Ok(collider) => {
            commands
                .entity(terrain_root)
                .insert((crate::engine::physics::prelude::RigidBody::Fixed, collider));
        }
        Err(err) => {
            warn!(
                "terrain mesh (seed {seed}, {vert_count} verts, {tri_count} tris, res {RES}) \
                 failed to produce a trimesh collider; spawning terrain without physics: {err:?}"
            );
        }
    }

    debug_assert!(RES.is_multiple_of(TERRAIN_PATCH_CELLS));
    commands.entity(terrain_root).with_children(|parent| {
        for start_z in (0..RES).step_by(TERRAIN_PATCH_CELLS) {
            for start_x in (0..RES).step_by(TERRAIN_PATCH_CELLS) {
                let (high_mesh, origin) =
                    terrain_patch_mesh(&h, seed, start_x, start_z, TERRAIN_PATCH_CELLS, 1);
                let (proxy_mesh, proxy_origin) = terrain_patch_mesh(
                    &h,
                    seed,
                    start_x,
                    start_z,
                    TERRAIN_PATCH_CELLS,
                    TERRAIN_PROXY_SAMPLE_STEP,
                );
                debug_assert_eq!(origin, proxy_origin);
                parent.spawn((
                    TerrainPbrBundle {
                        mesh: Mesh3d(meshes.add(high_mesh)),
                        material: MeshMaterial3d(terrain_material.clone()),
                        transform: Transform::from_translation(origin),
                        ..default()
                    },
                    ProceduralMaterialBinding::new("biome.main_world", "terrain"),
                    crate::engine::spatial_lod::SpatialLod::terrain_high(),
                    TerrainHighLodPatch,
                ));
                parent.spawn((
                    TerrainPbrBundle {
                        mesh: Mesh3d(meshes.add(proxy_mesh)),
                        material: MeshMaterial3d(terrain_material.clone()),
                        transform: Transform::from_translation(origin),
                        ..default()
                    },
                    ProceduralMaterialBinding::new("biome.main_world", "terrain"),
                    crate::engine::spatial_lod::SpatialLod::terrain_proxy(),
                    crate::engine::spatial_lod::SpatialLodProxy,
                    TerrainProxyLodPatch,
                ));
            }
        }
    });
}

fn spawn_everest_range_biomes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    let start_mesh_count = meshes.len();
    spawn_range_snowfields(commands, meshes, pal, seed);
    let snow_mesh_count = meshes.len();
    spawn_glacial_streams(commands, meshes, pal, seed);
    let stream_mesh_count = meshes.len();
    spawn_range_forests(commands, meshes, pal, seed);
    let forest_mesh_count = meshes.len();
    spawn_range_waylines(commands, meshes, pal, seed);
    let wayline_mesh_count = meshes.len();
    spawn_mountain_path_network(commands, meshes, pal, seed);
    let path_mesh_count = meshes.len();
    spawn_mountain_route_bridges(commands, meshes, pal, seed + 83, seed);
    let bridge_mesh_count = meshes.len();
    spawn_speed_road_network(commands, meshes, pal, seed + 1_241, seed);
    let road_mesh_count = meshes.len();
    spawn_range_outposts(commands, meshes, pal, seed);
    let outpost_mesh_count = meshes.len();
    spawn_dragon_lair_silhouettes(commands, meshes, pal, seed);
    info!(
        "Everest range meshes: snow {}, streams {}, forest {}, waylines {}, paths {}, bridges {}, speed roads {}, outposts {}, silhouettes {}",
        snow_mesh_count.saturating_sub(start_mesh_count),
        stream_mesh_count.saturating_sub(snow_mesh_count),
        forest_mesh_count.saturating_sub(stream_mesh_count),
        wayline_mesh_count.saturating_sub(forest_mesh_count),
        path_mesh_count.saturating_sub(wayline_mesh_count),
        bridge_mesh_count.saturating_sub(path_mesh_count),
        road_mesh_count.saturating_sub(bridge_mesh_count),
        outpost_mesh_count.saturating_sub(road_mesh_count),
        meshes.len().saturating_sub(outpost_mesh_count),
    );
}

const MOUNTAIN_SPIDER_MECH_SPAWNS: &[(&str, f32, f32, f32)] = &[
    ("Widow Engine Asterion", -7_050.0, -6_750.0, 520.0),
    ("Widow Engine Bellatrix", -5_450.0, 4_650.0, 430.0),
    ("Widow Engine Caligo", -2_950.0, -3_850.0, 470.0),
    ("Widow Engine Dreadstar", 2_900.0, 2_150.0, 500.0),
    ("Widow Engine Erebus", 5_950.0, -2_650.0, 460.0),
    ("Widow Engine Fang", 7_850.0, 4_250.0, 390.0),
    ("Widow Engine Gloom", 2_250.0, -7_650.0, 440.0),
    ("Widow Engine Hex", -7_250.0, 5_750.0, 410.0),
];

#[allow(dead_code)]
fn mountain_spider_mech_spawn_count() -> usize {
    MOUNTAIN_SPIDER_MECH_SPAWNS.len()
}

fn spawn_mountain_spider_mechs(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    let body_mesh = meshes.add(Cuboid::new(10.5, 3.4, 12.0));
    let abdomen_mesh = meshes.add(Sphere::new(1.0));
    let head_mesh = meshes.add(Cuboid::new(7.2, 3.0, 5.0));
    let turret_mesh = meshes.add(Cylinder::new(1.25, 3.8));
    let barrel_mesh = meshes.add(Cuboid::new(0.72, 0.72, 7.0));
    let eye_mesh = meshes.add(Sphere::new(0.48));
    let joint_mesh = meshes.add(Sphere::new(0.82));
    let leg_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let fang_mesh = meshes.add(Cuboid::new(0.55, 2.5, 0.55));

    for (index, &(name, anchor_x, anchor_z, patrol_radius)) in
        MOUNTAIN_SPIDER_MECH_SPAWNS.iter().enumerate()
    {
        let offset_angle = seeded(seed + 4_901, index as u64 * 3) * TAU;
        let offset_radius = 80.0 + seeded(seed + 4_901, index as u64 * 3 + 1) * 170.0;
        let x = anchor_x + offset_angle.cos() * offset_radius;
        let z = anchor_z + offset_angle.sin() * offset_radius;
        let body_clearance = 6.2;
        let position = Vec3::new(x, terrain_surface_y(x, z, seed) + body_clearance, z);

        let mut enemy = Enemy::new(EnemyType::Hybrid, position, 2.2);
        enemy.config.max_health = 1_100.0;
        enemy.config.attack_damage = 24.0;
        enemy.config.defense = 30.0;
        enemy.config.attack_cooldown = 2.8;
        enemy.config.attack_windup = 0.92;
        enemy.config.knockback_force = 950.0;
        enemy.config.experience_value = 360;
        enemy.config.credits = 260;
        enemy.config.detection_range = 280.0;
        enemy.config.chase_range = 420.0;
        enemy.config.attack_range = 32.0;
        let max_health = enemy.scaled_health();

        let root = commands
            .spawn((
                Name::new(name),
                SpatialBundle {
                    transform: Transform::from_translation(position),
                    ..default()
                },
                WorldGeometry,
                enemy,
                EnemyStateMachine::default(),
                Health::new(max_health),
                Damageable::with_defense(30.0, Vec::new()),
                Faction::DimensionalAlien,
                MountainSpiderMech {
                    home: position,
                    patrol_radius,
                    patrol_angle: offset_angle,
                    body_clearance,
                    gait_phase: index as f32 * 0.73,
                },
                crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
                crate::engine::physics::prelude::Collider::cuboid(6.0, 3.4, 7.2),
                crate::engine::physics::prelude::CollisionProfile::EnemyHurtbox,
                avian3d::prelude::Sensor,
            ))
            .id();

        commands.entity(root).with_children(|spider| {
            spider.spawn(PbrBundle {
                mesh: Mesh3d(body_mesh.clone()),
                material: MeshMaterial3d(pal.industrial_metal.clone()),
                transform: Transform::from_xyz(0.0, 0.3, 0.0),
                ..default()
            });
            spider.spawn(PbrBundle {
                mesh: Mesh3d(abdomen_mesh.clone()),
                material: MeshMaterial3d(pal.industrial_rust.clone()),
                transform: Transform::from_xyz(0.0, 0.5, -5.7).with_scale(Vec3::new(5.8, 3.5, 6.8)),
                ..default()
            });
            spider.spawn(PbrBundle {
                mesh: Mesh3d(head_mesh.clone()),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_xyz(0.0, 0.1, 6.4),
                ..default()
            });
            spider.spawn(PbrBundle {
                mesh: Mesh3d(turret_mesh.clone()),
                material: MeshMaterial3d(pal.industrial_rust.clone()),
                transform: Transform::from_xyz(0.0, 3.0, -0.4)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ..default()
            });
            spider.spawn(PbrBundle {
                mesh: Mesh3d(barrel_mesh.clone()),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_xyz(0.0, 3.4, 3.8),
                ..default()
            });

            for eye_x in [-2.25_f32, -0.75, 0.75, 2.25] {
                spider.spawn(PbrBundle {
                    mesh: Mesh3d(eye_mesh.clone()),
                    material: MeshMaterial3d(pal.boost_pad.clone()),
                    transform: Transform::from_xyz(eye_x, 0.65, 9.0),
                    ..default()
                });
            }
            for fang_x in [-1.65_f32, 1.65] {
                spider.spawn(PbrBundle {
                    mesh: Mesh3d(fang_mesh.clone()),
                    material: MeshMaterial3d(pal.boost_ramp.clone()),
                    transform: Transform::from_xyz(fang_x, -1.7, 8.4)
                        .with_rotation(Quat::from_rotation_z(fang_x.signum() * 0.28)),
                    ..default()
                });
            }

            for side in [-1.0_f32, 1.0] {
                for leg_index in 0..4 {
                    let z = -5.2 + leg_index as f32 * 3.5;
                    let sweep = (leg_index as f32 - 1.5) * 0.24;
                    let base_rotation = Quat::from_rotation_y(-side * sweep);
                    spider
                        .spawn((
                            SpatialBundle {
                                transform: Transform::from_xyz(side * 4.3, -0.2, z)
                                    .with_rotation(base_rotation),
                                ..default()
                            },
                            SpiderMechLeg {
                                mech: root,
                                base_rotation,
                                phase_offset: leg_index as f32 * std::f32::consts::FRAC_PI_2
                                    + if side > 0.0 {
                                        std::f32::consts::PI
                                    } else {
                                        0.0
                                    },
                                side,
                            },
                        ))
                        .with_children(|leg| {
                            leg.spawn(PbrBundle {
                                mesh: Mesh3d(joint_mesh.clone()),
                                material: MeshMaterial3d(pal.boost_lane.clone()),
                                transform: Transform::IDENTITY,
                                ..default()
                            });
                            leg.spawn(PbrBundle {
                                mesh: Mesh3d(leg_mesh.clone()),
                                material: MeshMaterial3d(pal.industrial_metal.clone()),
                                transform: Transform::from_xyz(side * 3.2, -0.9, 0.0)
                                    .with_rotation(Quat::from_rotation_z(side * -0.28))
                                    .with_scale(Vec3::new(6.8, 0.82, 0.92)),
                                ..default()
                            });
                            leg.spawn(PbrBundle {
                                mesh: Mesh3d(leg_mesh.clone()),
                                material: MeshMaterial3d(pal.industrial_rust.clone()),
                                transform: Transform::from_xyz(side * 7.7, -3.45, 0.0)
                                    .with_rotation(Quat::from_rotation_z(side * 0.55))
                                    .with_scale(Vec3::new(5.4, 0.72, 0.82)),
                                ..default()
                            });
                        });
                }
            }
        });
    }
}

fn mountain_spider_mech_movement_system(
    time: Res<Time>,
    settings: Res<GameSettings>,
    player_q: Query<&Transform, (With<Player>, Without<MountainSpiderMech>)>,
    mut mech_q: Query<
        (
            &mut Transform,
            &mut MountainSpiderMech,
            &mut Enemy,
            &mut EnemyStateMachine,
            &Health,
        ),
        Without<Player>,
    >,
) {
    let dt = time.delta_secs();
    for (mut transform, mut mech, mut enemy, mut state, health) in mech_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        state.timer += dt;
        enemy.attack_cooldown_timer = (enemy.attack_cooldown_timer - dt).max(0.0);

        let nearest_player = player_q
            .iter()
            .map(|player| {
                let delta = (player.translation - transform.translation).with_y(0.0);
                (player.translation, delta.length())
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));

        let (target, desired_state) = match nearest_player {
            Some((player, distance)) if distance <= enemy.config.attack_range => {
                (player, EnemyAIState::Attack)
            }
            Some((player, distance)) if distance <= enemy.config.detection_range => {
                (player, EnemyAIState::Chase)
            }
            _ => {
                mech.patrol_angle = (mech.patrol_angle + dt * 0.065) % TAU;
                (
                    mech.home
                        + Vec3::new(
                            mech.patrol_angle.cos() * mech.patrol_radius,
                            0.0,
                            mech.patrol_angle.sin() * mech.patrol_radius * 0.72,
                        ),
                    EnemyAIState::Patrol,
                )
            }
        };
        if state.current != desired_state {
            state.force(desired_state);
        }

        let to_target = (target - transform.translation).with_y(0.0);
        let direction = to_target.normalize_or_zero();
        let speed = match desired_state {
            EnemyAIState::Chase => 11.5,
            EnemyAIState::Patrol => 5.4,
            _ => 0.0,
        };
        if speed > 0.0 && direction.length_squared() > 0.001 {
            transform.translation += direction * speed * dt;
            mech.gait_phase = (mech.gait_phase + speed * dt * 0.48) % TAU;
            enemy.velocity = direction * speed;
        } else {
            enemy.velocity = Vec3::ZERO;
        }

        let ground_y = terrain_surface_y(
            transform.translation.x,
            transform.translation.z,
            settings.world_seed,
        );
        transform.translation.y = ground_y + mech.body_clearance;
        if direction.length_squared() > 0.001 {
            let normal = mountain_spider_terrain_normal(
                transform.translation.x,
                transform.translation.z,
                settings.world_seed,
            );
            let yaw = direction.x.atan2(direction.z);
            let target_rotation =
                Quat::from_rotation_arc(Vec3::Y, normal) * Quat::from_rotation_y(yaw);
            transform.rotation = transform
                .rotation
                .slerp(target_rotation, (dt * 3.2).clamp(0.0, 1.0));
        }
    }
}

fn animate_mountain_spider_legs(
    time: Res<Time>,
    mech_q: Query<&MountainSpiderMech>,
    mut leg_q: Query<(&SpiderMechLeg, &mut Transform)>,
) {
    let elapsed = time.elapsed_secs();
    for (leg, mut transform) in leg_q.iter_mut() {
        let Ok(mech) = mech_q.get(leg.mech) else {
            continue;
        };
        let phase = mech.gait_phase + leg.phase_offset;
        let stride = phase.sin() * 0.32;
        let lift = phase.cos().max(0.0) * 0.22;
        let menace = (elapsed * 1.7 + leg.phase_offset).sin() * 0.025;
        transform.rotation = leg.base_rotation
            * Quat::from_rotation_y(stride)
            * Quat::from_rotation_z(leg.side * (lift + menace));
    }
}

fn mountain_spider_terrain_normal(x: f32, z: f32, seed: u64) -> Vec3 {
    let sample = 5.0;
    let left = terrain_surface_y(x - sample, z, seed);
    let right = terrain_surface_y(x + sample, z, seed);
    let back = terrain_surface_y(x, z - sample, seed);
    let front = terrain_surface_y(x, z + sample, seed);
    Vec3::new(left - right, sample * 2.0, back - front).normalize_or_zero()
}

fn spawn_range_snowfields(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    let fields = [
        (-8500.0_f32, -7800.0_f32, 680.0_f32),
        (-7600.0, 3800.0, 420.0),
        (3200.0, -8200.0, 520.0),
        (-4300.0, -6100.0, 360.0),
        (8500.0, 4800.0, 300.0),
    ];

    for (fi, &(cx, cz, radius)) in fields.iter().enumerate() {
        for layer in 0..4u64 {
            let idx = fi as u64 * 19 + layer;
            let angle = seeded(seed, idx * 3) * std::f32::consts::TAU;
            let offset = radius * (0.10 + seeded(seed, idx * 3 + 1) * 0.34);
            let x = cx + angle.cos() * offset;
            let z = cz + angle.sin() * offset;
            let y = terrain_surface_y(x, z, seed) + 0.38 + layer as f32 * 0.08;
            let r = radius * (0.22 + seeded(seed, idx * 3 + 2) * 0.30);
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cylinder::new(r, 0.62))),
                    material: MeshMaterial3d(pal.snow.clone()),
                    transform: Transform::from_xyz(x, y, z),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

fn spawn_glacial_streams(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    spawn_glacial_stream(
        commands,
        meshes,
        pal,
        seed + 11,
        seed,
        &[
            (-9000.0, -8200.0),
            (-6800.0, -6600.0),
            (-4300.0, -4300.0),
            (-1700.0, -1800.0),
            (400.0, -620.0),
        ],
    );
    spawn_glacial_stream(
        commands,
        meshes,
        pal,
        seed + 17,
        seed,
        &[
            (-7800.0, 4200.0),
            (-5700.0, 5000.0),
            (-3300.0, 4300.0),
            (-900.0, 2700.0),
            (1800.0, 1500.0),
        ],
    );
    spawn_glacial_stream(
        commands,
        meshes,
        pal,
        seed + 31,
        seed,
        &[
            (8400.0, 5200.0),
            (6600.0, 3700.0),
            (5200.0, 1500.0),
            (4300.0, -900.0),
            (5400.0, -3200.0),
        ],
    );
    spawn_glacial_stream(
        commands,
        meshes,
        pal,
        seed + 47,
        seed,
        &[
            (3300.0, -8400.0),
            (2200.0, -6500.0),
            (900.0, -4700.0),
            (-800.0, -3300.0),
            (-2500.0, -2300.0),
        ],
    );
}

fn spawn_glacial_stream(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
    points: &[(f32, f32)],
) {
    for (i, pair) in points.windows(2).enumerate() {
        let (ax, az) = pair[0];
        let (bx, bz) = pair[1];
        let dx = bx - ax;
        let dz = bz - az;
        let len = (dx * dx + dz * dz).sqrt();
        if len <= 1.0 {
            continue;
        }

        let mx = (ax + bx) * 0.5;
        let mz = (az + bz) * 0.5;
        let ay = terrain_surface_y(ax, az, terrain_seed);
        let by = terrain_surface_y(bx, bz, terrain_seed);
        let y = ay.max(by) + 0.42;
        let width = 18.0 + seeded(seed, i as u64 * 5) * 26.0;
        let yaw = dx.atan2(dz);

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(width, 0.36, len))),
                material: MeshMaterial3d(pal.water.clone()),
                transform: Transform::from_xyz(mx, y, mz).with_rotation(Quat::from_rotation_y(yaw)),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_range_forests(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    let mesh_cache = NaturePrimitiveMeshCache::new(meshes);
    let patches = [
        (-6200.0_f32, 6500.0_f32, 860.0_f32, 42_u64),
        (-2800.0, 3300.0, 660.0, 34),
        (-7100.0, 2600.0, 720.0, 38),
        (-5000.0, -5400.0, 700.0, 34),
        (2500.0, 2400.0, 720.0, 38),
        (6300.0, 1200.0, 640.0, 30),
        (7900.0, 4200.0, 720.0, 36),
        (4900.0, -2500.0, 680.0, 32),
        (1800.0, -6000.0, 700.0, 34),
        (-1200.0, -4200.0, 640.0, 30),
        (-980.0, 980.0, 520.0, 28),
        (1040.0, -860.0, 540.0, 30),
    ];

    for (pi, &(cx, cz, spread, count)) in patches.iter().enumerate() {
        for i in 0..count {
            let idx = pi as u64 * 101 + i;
            let angle = seeded(seed, idx * 7) * std::f32::consts::TAU;
            let radius = spread * seeded(seed, idx * 7 + 1).sqrt();
            let x = cx + angle.cos() * radius;
            let z = cz + angle.sin() * radius;
            if starter_clear_zone(x, z, 180.0) {
                continue;
            }
            let ground = terrain_surface_y(x, z, seed);
            let scale = 1.65 + seeded(seed, idx * 7 + 2) * 2.35;
            spawn_alpine_tree(
                commands,
                pal,
                &mesh_cache,
                Vec3::new(x, ground, z),
                scale,
                seed + idx,
            );
        }
    }
}

fn spawn_moss_pad(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    radius: f32,
    seed: u64,
) {
    let unit_sphere = meshes.add(Sphere::new(1.0));
    spawn_moss_pad_cached(commands, pal, &unit_sphere, center, radius, seed);
}

fn spawn_moss_pad_cached(
    commands: &mut Commands,
    pal: &Palette,
    unit_sphere: &Handle<Mesh>,
    center: Vec3,
    radius: f32,
    seed: u64,
) {
    let patch_count = 2 + (seed % 3);
    for patch in 0..patch_count {
        let angle = seeded(seed, patch * 5) * TAU;
        let offset = radius * (0.18 + seeded(seed, patch * 5 + 1) * 0.58);
        let scale = radius * (0.28 + seeded(seed, patch * 5 + 2) * 0.36);
        let pos = center
            + Vec3::new(
                angle.cos() * offset,
                0.10 + patch as f32 * 0.018,
                angle.sin() * offset,
            );
        let transform = Transform::from_translation(pos)
            .with_rotation(Quat::from_rotation_y(angle))
            .with_scale(Vec3::new(scale * 1.55, 0.055, scale));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(unit_sphere.clone()),
                material: MeshMaterial3d(pal.moss.clone()),
                transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(&transform, angle, 0.8, 0.004, 0.006, 0.005),
        ));
    }
}

fn spawn_vine_curtain(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    anchor: Vec3,
    yaw: f32,
    radius: f32,
    height: f32,
    strands: u64,
    seed: u64,
) {
    let unit_sphere = meshes.add(Sphere::new(1.0));
    let unit_cylinder = meshes.add(Cylinder::new(1.0, 1.0));
    spawn_vine_curtain_cached(
        commands,
        pal,
        &unit_sphere,
        &unit_cylinder,
        anchor,
        yaw,
        radius,
        height,
        strands,
        seed,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_vine_curtain_cached(
    commands: &mut Commands,
    pal: &Palette,
    unit_sphere: &Handle<Mesh>,
    unit_cylinder: &Handle<Mesh>,
    anchor: Vec3,
    yaw: f32,
    radius: f32,
    height: f32,
    strands: u64,
    seed: u64,
) {
    let strand_count = strands.max(1);
    for strand in 0..strand_count {
        let angle = yaw + seeded(seed, strand * 17) * TAU;
        let radial = radius * (0.35 + seeded(seed, strand * 17 + 1) * 0.78);
        let top = anchor + Vec3::new(angle.cos() * radial, 0.0, angle.sin() * radial);
        let sag = height * (0.58 + seeded(seed, strand * 17 + 2) * 0.48);
        let drift_angle = angle + 0.8 + seeded(seed, strand * 17 + 3) * 1.2;
        let drift = Vec3::new(drift_angle.cos(), 0.0, drift_angle.sin())
            * radius
            * (0.12 + seeded(seed, strand * 17 + 4) * 0.24);
        let mid = top + drift * 0.65 - Vec3::Y * sag * 0.46;
        let bottom = top + drift - Vec3::Y * sag;
        let vine_radius = 0.035 + seeded(seed, strand * 17 + 5) * 0.050;

        spawn_static_cylinder_between_cached(
            commands,
            unit_cylinder,
            pal.vine.clone(),
            top,
            mid,
            vine_radius,
        );
        spawn_static_cylinder_between_cached(
            commands,
            unit_cylinder,
            pal.vine.clone(),
            mid,
            bottom,
            vine_radius * 0.84,
        );

        for leaf in 0..3u64 {
            let t = 0.24 + leaf as f32 * 0.24 + seeded(seed, strand * 31 + leaf) * 0.08;
            let p = top.lerp(bottom, t.clamp(0.0, 0.96));
            let leaf_scale = 0.16 + seeded(seed, strand * 37 + leaf) * 0.18;
            let transform = Transform::from_translation(p)
                .with_rotation(Quat::from_rotation_y(angle + leaf as f32 * 0.9))
                .with_scale(Vec3::new(leaf_scale * 1.45, leaf_scale * 0.52, leaf_scale));
            let mat = match (seed + strand + leaf) % 4 {
                0 => pal.moss.clone(),
                1 => pal.foliage_a.clone(),
                2 => pal.foliage_b.clone(),
                _ => pal.foliage_c.clone(),
            };
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_sphere.clone()),
                    material: MeshMaterial3d(mat),
                    transform,
                    ..default()
                },
                WorldGeometry,
                NatureSway::new(&transform, angle + leaf as f32, 1.8, 0.030, 0.018, 0.014),
            ));
        }
    }
}

fn spawn_alpine_tree(
    commands: &mut Commands,
    pal: &Palette,
    mesh_cache: &NaturePrimitiveMeshCache,
    base: Vec3,
    scale: f32,
    seed: u64,
) {
    let trunk_h = 12.5 * scale;
    let trunk_r = 1.10 * scale;
    let lower_h = 18.0 * scale;
    let upper_h = 14.0 * scale;
    let lower_r = 7.2 * scale;
    let upper_r = 5.2 * scale;
    let yaw = seeded(seed, 3) * std::f32::consts::TAU;
    let bark = match seed % 4 {
        0 => pal.bark_light.clone(),
        1 => pal.bark_mid.clone(),
        2 => pal.bark_dark.clone(),
        _ => pal.rock_dark.clone(),
    };
    let foliage = match seed % 3 {
        0 => pal.foliage_a.clone(),
        1 => pal.foliage_b.clone(),
        _ => pal.foliage_c.clone(),
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cylinder.clone()),
            material: MeshMaterial3d(bark.clone()),
            transform: Transform::from_xyz(base.x, base.y + trunk_h * 0.5, base.z)
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(Vec3::new(trunk_r, trunk_h, trunk_r)),
            ..default()
        },
        WorldGeometry,
    ));

    for root in 0..3u64 {
        let root_yaw = yaw + root as f32 * TAU / 3.0 + seeded(seed, 41 + root) * 0.35;
        let root_len = trunk_r * (3.4 + seeded(seed, 51 + root) * 1.8);
        let root_transform = Transform::from_xyz(base.x, base.y + trunk_r * 0.22, base.z)
            .with_rotation(Quat::from_rotation_y(root_yaw))
            .with_scale(Vec3::new(trunk_r * 0.45, trunk_r * 0.34, root_len));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_cube.clone()),
                material: MeshMaterial3d(bark.clone()),
                transform: root_transform,
                ..default()
            },
            WorldGeometry,
        ));
    }

    let lower = Transform::from_xyz(base.x, base.y + trunk_h * 0.55 + lower_h * 0.5, base.z)
        .with_rotation(Quat::from_rotation_y(yaw))
        .with_scale(Vec3::new(lower_r, lower_h, lower_r));
    let lower_sway = NatureSway::new(&lower, seeded(seed, 11) * 6.0, 0.35, 0.010, 0.015, 0.006);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cone.clone()),
            material: MeshMaterial3d(foliage.clone()),
            transform: lower,
            ..default()
        },
        WorldGeometry,
        lower_sway,
    ));

    let branch_ring = Transform::from_xyz(base.x, base.y + trunk_h * 0.92, base.z)
        .with_rotation(Quat::from_rotation_y(yaw + 0.35))
        .with_scale(Vec3::new(lower_r * 0.38, trunk_r * 0.30, lower_r * 0.95));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cube.clone()),
            material: MeshMaterial3d(bark.clone()),
            transform: branch_ring,
            ..default()
        },
        WorldGeometry,
    ));

    spawn_moss_pad_cached(
        commands,
        pal,
        &mesh_cache.unit_sphere,
        base + Vec3::Y * 0.04,
        trunk_r * 3.2,
        seed + 701,
    );
    if seed % 4 != 1 {
        spawn_vine_curtain_cached(
            commands,
            pal,
            &mesh_cache.unit_sphere,
            &mesh_cache.unit_cylinder,
            base + Vec3::Y * (trunk_h * 1.05),
            yaw + 0.4,
            lower_r * 0.42,
            trunk_h * 0.74,
            2 + seed % 3,
            seed + 907,
        );
    }

    let upper = Transform::from_xyz(base.x, base.y + trunk_h * 1.10 + lower_h * 0.58, base.z)
        .with_rotation(Quat::from_rotation_y(yaw + 0.7))
        .with_scale(Vec3::new(upper_r, upper_h, upper_r));
    let upper_sway = NatureSway::new(&upper, seeded(seed, 13) * 6.0, 0.42, 0.014, 0.018, 0.007);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cone.clone()),
            material: MeshMaterial3d(foliage),
            transform: upper,
            ..default()
        },
        WorldGeometry,
        upper_sway,
    ));
}

fn spawn_mana_forests(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let mesh_cache = NaturePrimitiveMeshCache::new(meshes);
    let groves = [
        (-520.0_f32, 380.0_f32, 360.0_f32, 24_u64, 1.25_f32),
        (520.0, -420.0, 340.0, 23, 1.22),
        (-880.0, -300.0, 300.0, 18, 1.08),
        (940.0, 720.0, 420.0, 26, 1.18),
        (-1820.0, 1520.0, 620.0, 30, 1.35),
        (2140.0, -1840.0, 660.0, 32, 1.38),
        (-3180.0, 2860.0, 720.0, 34, 1.42),
        (3820.0, 2100.0, 760.0, 36, 1.40),
        (-5520.0, 6120.0, 820.0, 38, 1.52),
        (6040.0, -1820.0, 720.0, 34, 1.44),
        (2100.0, -6200.0, 760.0, 36, 1.48),
        (-6120.0, -4680.0, 780.0, 36, 1.50),
    ];

    for (gi, &(cx, cz, radius, count, scale_bias)) in groves.iter().enumerate() {
        let grove_seed = seed + gi as u64 * 997;
        for i in 0..count {
            let angle = seeded(grove_seed, i * 11) * TAU;
            let dist = radius * seeded(grove_seed, i * 11 + 1).sqrt();
            let x = cx + angle.cos() * dist;
            let z = cz + angle.sin() * dist;
            if starter_clear_zone(x, z, 44.0) {
                continue;
            }
            let y = terrain_surface_y(x, z, terrain_seed);
            if y > 420.0 && gi < 4 {
                continue;
            }
            let old_growth = i % 7 == 0 || seeded(grove_seed, i * 11 + 2) > 0.86;
            let scale = if old_growth {
                scale_bias * (2.55 + seeded(grove_seed, i * 11 + 3) * 1.65)
            } else {
                scale_bias * (1.15 + seeded(grove_seed, i * 11 + 3) * 1.55)
            };
            spawn_giant_mana_tree(
                commands,
                pal,
                &mesh_cache,
                Vec3::new(x, y, z),
                scale,
                grove_seed + i * 37,
            );
        }
    }
}

const TAMBORN_MOTHER_LOWER_DECK: f32 = 86.0;
const TAMBORN_MOTHER_UPPER_DECK: f32 = 168.0;
const TAMBORN_MOTHER_CROWN_DECK: f32 = 238.0;

#[derive(Clone, Copy)]
struct TambornTower {
    base: Vec3,
    lower_deck_y: f32,
    upper_deck_y: f32,
    radius: f32,
}

fn spawn_bio_city_tamborn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let mesh_cache = NaturePrimitiveMeshCache::new(meshes);
    let center_x = -760.0_f32;
    let center_z = 1040.0_f32;
    let center_ground = terrain_surface_y(center_x, center_z, terrain_seed);
    let center = Vec3::new(center_x, center_ground, center_z);

    spawn_mother_tree_tamborn(commands, pal, &mesh_cache, center, seed);

    let tower_count = 13usize;
    let mut towers = Vec::with_capacity(tower_count);
    for i in 0..tower_count {
        let idx = i as u64;
        let angle = i as f32 / tower_count as f32 * TAU + 0.18;
        let ring = 218.0 + (i % 4) as f32 * 18.0 + seeded(seed, idx * 23) * 44.0;
        let x = center_x + angle.cos() * ring;
        let z = center_z + angle.sin() * ring;
        let ground = terrain_surface_y(x, z, terrain_seed);
        let height = 128.0 + seeded(seed, idx * 23 + 1) * 96.0 + (i % 3) as f32 * 18.0;
        let radius = 7.8 + seeded(seed, idx * 23 + 2) * 5.4 + (i % 2) as f32 * 1.8;
        towers.push(spawn_tamborn_tree_skyscraper(
            commands,
            pal,
            &mesh_cache,
            Vec3::new(x, ground, z),
            height,
            radius,
            seed + idx * 211,
        ));
    }

    for i in 0..towers.len() {
        let next = (i + 1) % towers.len();
        let tower_a = towers[i];
        let tower_b = towers[next];
        let use_upper = i % 2 == 1;
        let a_y = if use_upper {
            tower_a.upper_deck_y
        } else {
            tower_a.lower_deck_y
        };
        let b_y = if use_upper {
            tower_b.upper_deck_y
        } else {
            tower_b.lower_deck_y
        };
        let start = tamborn_bridge_anchor(tower_a.base, a_y, tower_b.base, tower_a.radius * 2.0);
        let end = tamborn_bridge_anchor(tower_b.base, b_y, tower_a.base, tower_b.radius * 2.0);
        spawn_rope_bridge_between(
            commands,
            pal,
            &mesh_cache,
            start,
            end,
            9.5 + (i % 3) as f32 * 0.8,
            seed + i as u64 * 503,
        );
    }

    for i in (0..towers.len()).step_by(3) {
        let tower = towers[i];
        let target_y = if i % 2 == 0 {
            center.y + TAMBORN_MOTHER_LOWER_DECK
        } else {
            center.y + TAMBORN_MOTHER_UPPER_DECK
        };
        let tower_y = if i % 2 == 0 {
            tower.lower_deck_y
        } else {
            tower.upper_deck_y
        };
        let start = tamborn_bridge_anchor(tower.base, tower_y, center, tower.radius * 2.2);
        let end = tamborn_bridge_anchor(center, target_y, tower.base, 36.0);
        spawn_rope_bridge_between(
            commands,
            pal,
            &mesh_cache,
            start,
            end,
            12.0,
            seed + i as u64 * 719 + 90,
        );
    }

    for (path_i, tower_i) in [2usize, 8usize].iter().copied().enumerate() {
        if let Some(tower) = towers.get(tower_i).copied() {
            let away = Vec2::new(tower.base.x - center.x, tower.base.z - center.z);
            let dir = if away.length_squared() > 0.001 {
                away.normalize()
            } else {
                Vec2::Y
            };
            let start_x = tower.base.x + dir.x * 156.0;
            let start_z = tower.base.z + dir.y * 156.0;
            let start = Vec3::new(
                start_x,
                terrain_surface_y(start_x, start_z, terrain_seed) + 2.4,
                start_z,
            );
            let end =
                tamborn_bridge_anchor(tower.base, tower.lower_deck_y, start, tower.radius * 2.4);
            spawn_rope_bridge_between(
                commands,
                pal,
                &mesh_cache,
                start,
                end,
                13.5,
                seed + path_i as u64 * 1_113 + 1_900,
            );
        }
    }

    for grove in 0..26u64 {
        let angle = seeded(seed, 8_000 + grove * 5) * TAU;
        let dist = 310.0 + seeded(seed, 8_000 + grove * 5 + 1) * 220.0;
        let x = center_x + angle.cos() * dist;
        let z = center_z + angle.sin() * dist;
        let ground = terrain_surface_y(x, z, terrain_seed);
        spawn_giant_mana_tree(
            commands,
            pal,
            &mesh_cache,
            Vec3::new(x, ground, z),
            2.2 + seeded(seed, 8_000 + grove * 5 + 2) * 1.9,
            seed + 5_000 + grove * 17,
        );
    }

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.34, 1.0, 0.64),
                intensity: 36_000.0,
                range: 260.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(center + Vec3::Y * 134.0),
            ..default()
        },
        WorldGeometry,
        Name::new("Bio City Tamborn"),
    ));
}

fn spawn_mother_tree_tamborn(
    commands: &mut Commands,
    pal: &Palette,
    mesh_cache: &NaturePrimitiveMeshCache,
    base: Vec3,
    seed: u64,
) {
    let yaw = seeded(seed, 31) * TAU;

    for (offset_y, height, radius, material) in [
        (92.0, 184.0, 26.0, pal.bark_dark.clone()),
        (226.0, 176.0, 18.5, pal.bark_mid.clone()),
        (328.0, 92.0, 12.5, pal.bark_light.clone()),
    ] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_cylinder.clone()),
                material: MeshMaterial3d(material),
                transform: Transform::from_xyz(base.x, base.y + offset_y, base.z)
                    .with_rotation(Quat::from_rotation_y(yaw))
                    .with_scale(Vec3::new(radius, height, radius)),
                ..default()
            },
            WorldGeometry,
            Name::new("Tamborn Mother Tree Trunk"),
        ));
    }

    for root in 0..11u64 {
        let root_yaw = yaw + root as f32 * TAU / 11.0 + seeded(seed, 100 + root) * 0.16;
        let dir = Vec3::new(root_yaw.cos(), 0.0, root_yaw.sin());
        let start = base + dir * 8.0 + Vec3::Y * 7.5;
        let end = base + dir * (78.0 + seeded(seed, 140 + root) * 52.0) + Vec3::Y * 1.6;
        spawn_static_cylinder_between_cached(
            commands,
            &mesh_cache.unit_cylinder,
            pal.bark_dark.clone(),
            start,
            end,
            4.6 + seeded(seed, 180 + root) * 2.2,
        );
    }

    spawn_tamborn_platform(
        commands,
        mesh_cache,
        pal.moss.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_LOWER_DECK,
        76.0,
        3.0,
        "Tamborn Mother Tree Lower Terrace",
    );
    spawn_tamborn_ring_rail(
        commands,
        mesh_cache,
        pal.vine.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_LOWER_DECK,
        78.0,
        3.0,
        28,
        0.24,
    );
    spawn_tamborn_platform(
        commands,
        mesh_cache,
        pal.bridge_deck.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_UPPER_DECK,
        58.0,
        2.6,
        "Tamborn Mother Tree Upper Terrace",
    );
    spawn_tamborn_ring_rail(
        commands,
        mesh_cache,
        pal.vine.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_UPPER_DECK,
        60.0,
        3.0,
        24,
        0.20,
    );
    spawn_tamborn_platform(
        commands,
        mesh_cache,
        pal.marble.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_CROWN_DECK,
        38.0,
        2.3,
        "Tamborn Moon Crown Terrace",
    );
    spawn_tamborn_ring_rail(
        commands,
        mesh_cache,
        pal.guide_glow.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_CROWN_DECK,
        40.0,
        2.8,
        20,
        0.16,
    );

    let canopy_center = base + Vec3::Y * 326.0;
    let canopy_specs = [
        (0.0_f32, 0.0_f32, 96.0_f32, 56.0_f32, pal.foliage_a.clone()),
        (0.0, 64.0, 70.0, 44.0, pal.foliage_b.clone()),
        (TAU * 0.10, 50.0, 62.0, 38.0, pal.foliage_c.clone()),
        (TAU * 0.24, 72.0, 68.0, 42.0, pal.foliage_a.clone()),
        (TAU * 0.40, 66.0, 64.0, 39.0, pal.moss.clone()),
        (TAU * 0.56, 70.0, 74.0, 43.0, pal.foliage_b.clone()),
        (TAU * 0.72, 62.0, 66.0, 40.0, pal.foliage_c.clone()),
        (TAU * 0.88, 58.0, 70.0, 41.0, pal.foliage_a.clone()),
    ];
    for (i, (angle, radial, width, height, material)) in canopy_specs.into_iter().enumerate() {
        let a = yaw + angle + seeded(seed, 260 + i as u64) * 0.12;
        let transform = Transform::from_translation(
            canopy_center + Vec3::new(a.cos() * radial, i as f32 * 5.5, a.sin() * radial),
        )
        .with_rotation(Quat::from_rotation_y(a))
        .with_scale(Vec3::new(width, height, width * 0.88));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_sphere.clone()),
                material: MeshMaterial3d(material),
                transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(&transform, a, 0.26, 0.006, 0.012, 0.004),
        ));

        if i > 1 {
            let branch_start =
                base + Vec3::new(a.cos() * 12.0, 178.0 + i as f32 * 8.0, a.sin() * 12.0);
            let branch_end =
                canopy_center + Vec3::new(a.cos() * radial * 0.72, -12.0, a.sin() * radial * 0.72);
            spawn_static_cylinder_between_cached(
                commands,
                &mesh_cache.unit_cylinder,
                pal.bark_mid.clone(),
                branch_start,
                branch_end,
                3.2,
            );
        }
    }

    for crystal in 0..10u64 {
        let angle = yaw + crystal as f32 * TAU / 10.0 + seeded(seed, 400 + crystal) * 0.18;
        let ring = 44.0 + seeded(seed, 430 + crystal) * 19.0;
        let crystal_radius = 2.0 + seeded(seed, 470 + crystal) * 1.2;
        let crystal_height = 8.0 + seeded(seed, 510 + crystal) * 6.0;
        let pos = base
            + Vec3::new(
                angle.cos() * ring,
                TAMBORN_MOTHER_LOWER_DECK + 4.4,
                angle.sin() * ring,
            );
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_cone.clone()),
                material: MeshMaterial3d(pal.crystal_emerald.clone()),
                transform: Transform::from_translation(pos)
                    .with_rotation(Quat::from_rotation_y(angle))
                    .with_scale(Vec3::new(crystal_radius, crystal_height, crystal_radius)),
                ..default()
            },
            WorldGeometry,
            Name::new("Tamborn Heart Crystal"),
        ));
    }

    spawn_vine_curtain_cached(
        commands,
        pal,
        &mesh_cache.unit_sphere,
        &mesh_cache.unit_cylinder,
        canopy_center + Vec3::Y * 32.0,
        yaw,
        84.0,
        236.0,
        16,
        seed + 820,
    );

    for (light_y, range, intensity) in [(62.0, 120.0, 18_000.0), (242.0, 150.0, 26_000.0)] {
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: Color::srgb(0.35, 1.0, 0.52),
                    intensity,
                    range,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_translation(base + Vec3::Y * light_y),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_tamborn_tree_skyscraper(
    commands: &mut Commands,
    pal: &Palette,
    mesh_cache: &NaturePrimitiveMeshCache,
    base: Vec3,
    height: f32,
    radius: f32,
    seed: u64,
) -> TambornTower {
    let yaw = seeded(seed, 8) * TAU;
    let bark = match seed % 4 {
        0 => pal.bark_light.clone(),
        1 => pal.bark_mid.clone(),
        2 => pal.bark_dark.clone(),
        _ => pal.rock_dark.clone(),
    };
    let foliage = match seed % 4 {
        0 => pal.foliage_a.clone(),
        1 => pal.foliage_b.clone(),
        2 => pal.foliage_c.clone(),
        _ => pal.moss.clone(),
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cylinder.clone()),
            material: MeshMaterial3d(bark.clone()),
            transform: Transform::from_xyz(base.x, base.y + height * 0.5, base.z)
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(Vec3::new(radius, height, radius)),
            ..default()
        },
        WorldGeometry,
        Name::new("Tamborn Tree Skyscraper"),
    ));

    for root in 0..6u64 {
        let root_yaw = yaw + root as f32 * TAU / 6.0 + seeded(seed, 50 + root) * 0.22;
        let dir = Vec3::new(root_yaw.cos(), 0.0, root_yaw.sin());
        let start = base + dir * radius * 0.42 + Vec3::Y * (radius * 0.46);
        let end = base
            + dir * (radius * (4.6 + seeded(seed, 70 + root) * 2.7))
            + Vec3::Y * (radius * 0.08);
        spawn_static_cylinder_between_cached(
            commands,
            &mesh_cache.unit_cylinder,
            bark.clone(),
            start,
            end,
            radius * 0.34,
        );
    }

    let lower_deck_y = base.y + height * 0.50;
    let upper_deck_y = base.y + height * 0.76;
    let lower_radius = radius * 3.1;
    let upper_radius = radius * 2.35;
    spawn_tamborn_platform(
        commands,
        mesh_cache,
        pal.bridge_deck.clone(),
        Vec3::new(base.x, lower_deck_y, base.z),
        lower_radius,
        1.8,
        "Tamborn Canopy Platform",
    );
    spawn_tamborn_ring_rail(
        commands,
        mesh_cache,
        pal.vine.clone(),
        Vec3::new(base.x, lower_deck_y, base.z),
        lower_radius + 1.0,
        2.2,
        16,
        0.13,
    );
    spawn_tamborn_platform(
        commands,
        mesh_cache,
        pal.moss.clone(),
        Vec3::new(base.x, upper_deck_y, base.z),
        upper_radius,
        1.6,
        "Tamborn High Canopy Platform",
    );
    spawn_tamborn_ring_rail(
        commands,
        mesh_cache,
        pal.vine.clone(),
        Vec3::new(base.x, upper_deck_y, base.z),
        upper_radius + 1.0,
        2.0,
        14,
        0.12,
    );

    for level in 0..5u64 {
        let pod_y = base.y + height * (0.22 + level as f32 * 0.12);
        let pods = 3 + (level % 2);
        for pod in 0..pods {
            if seeded(seed, 110 + level * 17 + pod) < 0.25 {
                continue;
            }
            let a = yaw + pod as f32 * TAU / pods as f32 + level as f32 * 0.37;
            let dir = Vec3::new(a.cos(), 0.0, a.sin());
            let mat = if (level + pod + seed).is_multiple_of(2) {
                pal.small_window_warm.clone()
            } else {
                pal.small_window_cool.clone()
            };
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(mesh_cache.unit_cube.clone()),
                    material: MeshMaterial3d(mat),
                    transform: Transform::from_translation(
                        base + dir * (radius + 0.28) + Vec3::Y * pod_y,
                    )
                    .with_rotation(Quat::from_rotation_y(a))
                    .with_scale(Vec3::new(radius * 0.44, 3.6, 0.20)),
                    ..default()
                },
                WorldGeometry,
                Name::new("Tamborn Living Window"),
            ));
        }
    }

    let canopy_base = base + Vec3::Y * (height + radius * 1.4);
    for crown in 0..5u64 {
        let a = yaw + crown as f32 * TAU / 5.0 + seeded(seed, 260 + crown) * 0.28;
        let radial = if crown == 0 {
            0.0
        } else {
            radius * (3.4 + seeded(seed, 280 + crown) * 1.3)
        };
        let crown_scale = radius * (3.0 + seeded(seed, 300 + crown) * 1.4);
        let transform = Transform::from_translation(
            canopy_base
                + Vec3::new(
                    a.cos() * radial,
                    crown as f32 * radius * 0.30,
                    a.sin() * radial,
                ),
        )
        .with_rotation(Quat::from_rotation_y(a))
        .with_scale(Vec3::new(
            crown_scale * 1.12,
            crown_scale * 0.74,
            crown_scale,
        ));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_sphere.clone()),
                material: MeshMaterial3d(foliage.clone()),
                transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(&transform, a, 0.34, 0.008, 0.018, 0.006),
        ));

        if crown != 0 {
            spawn_static_cylinder_between_cached(
                commands,
                &mesh_cache.unit_cylinder,
                bark.clone(),
                base + Vec3::new(
                    a.cos() * radius * 0.55,
                    height * 0.72,
                    a.sin() * radius * 0.55,
                ),
                canopy_base
                    + Vec3::new(
                        a.cos() * radial * 0.80,
                        -radius * 0.4,
                        a.sin() * radial * 0.80,
                    ),
                radius * 0.24,
            );
        }
    }

    spawn_vine_curtain_cached(
        commands,
        pal,
        &mesh_cache.unit_sphere,
        &mesh_cache.unit_cylinder,
        canopy_base + Vec3::Y * (radius * 2.2),
        yaw,
        radius * 4.4,
        height * 0.68,
        7 + seed % 4,
        seed + 620,
    );

    TambornTower {
        base,
        lower_deck_y,
        upper_deck_y,
        radius,
    }
}

fn spawn_tamborn_platform(
    commands: &mut Commands,
    mesh_cache: &NaturePrimitiveMeshCache,
    material: Handle<StandardMaterial>,
    center: Vec3,
    radius: f32,
    thickness: f32,
    name: &'static str,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cylinder.clone()),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(center)
                .with_scale(Vec3::new(radius, thickness, radius)),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Name::new(name),
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(thickness * 0.5, radius),
        world_space_collider_scale(),
    ));
}

fn spawn_tamborn_ring_rail(
    commands: &mut Commands,
    mesh_cache: &NaturePrimitiveMeshCache,
    material: Handle<StandardMaterial>,
    center: Vec3,
    radius: f32,
    y_offset: f32,
    segments: usize,
    tube_radius: f32,
) {
    let count = segments.max(6);
    for segment in 0..count {
        let a0 = segment as f32 / count as f32 * TAU;
        let a1 = (segment + 1) as f32 / count as f32 * TAU;
        let start = center + Vec3::new(a0.cos() * radius, y_offset, a0.sin() * radius);
        let end = center + Vec3::new(a1.cos() * radius, y_offset, a1.sin() * radius);
        spawn_static_cylinder_between_cached(
            commands,
            &mesh_cache.unit_cylinder,
            material.clone(),
            start,
            end,
            tube_radius,
        );
    }
}

fn tamborn_bridge_anchor(from: Vec3, y: f32, toward: Vec3, offset: f32) -> Vec3 {
    let dir = Vec2::new(toward.x - from.x, toward.z - from.z);
    let dir = if dir.length_squared() > 0.001 {
        dir.normalize()
    } else {
        Vec2::Y
    };
    Vec3::new(from.x + dir.x * offset, y, from.z + dir.y * offset)
}

fn spawn_rope_bridge_between(
    commands: &mut Commands,
    pal: &Palette,
    mesh_cache: &NaturePrimitiveMeshCache,
    start: Vec3,
    end: Vec3,
    width: f32,
    seed: u64,
) {
    let delta = end - start;
    let horizontal_len = Vec2::new(delta.x, delta.z).length();
    if horizontal_len <= 8.0 {
        return;
    }

    let yaw = delta.x.atan2(delta.z);
    let pitch = (delta.y / horizontal_len).atan().clamp(-0.46, 0.46);
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    let side_rot = Quat::from_rotation_y(yaw);
    let center = start + delta * 0.5;
    let deck_thickness = 0.42;

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cube.clone()),
            material: MeshMaterial3d(pal.bridge_deck.clone()),
            transform: Transform::from_translation(center)
                .with_rotation(rot)
                .with_scale(Vec3::new(width, deck_thickness, horizontal_len)),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Name::new("Tamborn Rope Bridge"),
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            width * 0.5,
            deck_thickness * 0.5,
            horizontal_len * 0.5,
        ),
        world_space_collider_scale(),
    ));

    let plank_count = (horizontal_len / 18.0).ceil().clamp(8.0, 28.0) as usize;
    let plank_depth = horizontal_len / plank_count as f32 * 0.58;
    for plank in 0..plank_count {
        let t = (plank as f32 + 0.5) / plank_count as f32;
        let local_z = -horizontal_len * 0.5 + horizontal_len * t;
        let centered = t * 2.0 - 1.0;
        let sag = -(1.0 - centered.abs()).powi(2) * 0.42;
        let twist = (seeded(seed, plank as u64 * 13) - 0.5) * 0.06;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_cube.clone()),
                material: MeshMaterial3d(pal.bridge_deck.clone()),
                transform: Transform::from_translation(
                    center + rot * Vec3::new(0.0, 0.44 + sag, local_z),
                )
                .with_rotation(rot * Quat::from_rotation_z(twist))
                .with_scale(Vec3::new(width * 0.96, 0.16, plank_depth)),
                ..default()
            },
            WorldGeometry,
        ));
    }

    for side in [-1.0_f32, 1.0] {
        let side_offset = side_rot * Vec3::new(side * width * 0.62, 0.0, 0.0);
        let mut last_high: Option<Vec3> = None;
        let mut last_low: Option<Vec3> = None;
        for cable_step in 0..=12 {
            let t = cable_step as f32 / 12.0;
            let mid_sag = (1.0 - (t * 2.0 - 1.0).abs()).powi(2);
            let high = start.lerp(end, t) + side_offset + Vec3::Y * (2.8 - mid_sag * 1.1);
            let low = start.lerp(end, t) + side_offset + Vec3::Y * (1.05 - mid_sag * 0.55);
            if let Some(last) = last_high {
                spawn_static_cylinder_between_cached(
                    commands,
                    &mesh_cache.unit_cylinder,
                    pal.vine.clone(),
                    last,
                    high,
                    0.16,
                );
            }
            if let Some(last) = last_low {
                spawn_static_cylinder_between_cached(
                    commands,
                    &mesh_cache.unit_cylinder,
                    pal.bark_dark.clone(),
                    last,
                    low,
                    0.13,
                );
            }
            if cable_step % 3 == 0 {
                spawn_static_cylinder_between_cached(
                    commands,
                    &mesh_cache.unit_cylinder,
                    pal.vine.clone(),
                    low,
                    high,
                    0.08,
                );
            }
            last_high = Some(high);
            last_low = Some(low);
        }
    }
}

fn spawn_giant_mana_tree(
    commands: &mut Commands,
    pal: &Palette,
    mesh_cache: &NaturePrimitiveMeshCache,
    base: Vec3,
    scale: f32,
    seed: u64,
) {
    let trunk_h = 38.0 * scale;
    let trunk_r = 3.2 * scale;
    let canopy_r = 12.5 * scale;
    let yaw = seeded(seed, 3) * TAU;
    let bark = match seed % 3 {
        0 => pal.bark_light.clone(),
        1 => pal.bark_mid.clone(),
        _ => pal.bark_dark.clone(),
    };
    let foliage = match seed % 4 {
        0 => pal.foliage_a.clone(),
        1 => pal.foliage_b.clone(),
        2 => pal.foliage_c.clone(),
        _ => pal.moss.clone(),
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cylinder.clone()),
            material: MeshMaterial3d(bark.clone()),
            transform: Transform::from_xyz(base.x, base.y + trunk_h * 0.5, base.z)
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(Vec3::new(trunk_r, trunk_h, trunk_r)),
            ..default()
        },
        WorldGeometry,
    ));

    for root in 0..7u64 {
        let root_yaw = yaw + root as f32 * TAU / 7.0 + seeded(seed, 40 + root) * 0.22;
        let root_len = trunk_r * (4.8 + seeded(seed, 70 + root) * 2.4);
        let root_transform = Transform::from_xyz(base.x, base.y + trunk_r * 0.20, base.z)
            .with_rotation(Quat::from_rotation_y(root_yaw))
            .with_scale(Vec3::new(trunk_r * 0.54, trunk_r * 0.36, root_len));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_cube.clone()),
                material: MeshMaterial3d(bark.clone()),
                transform: root_transform,
                ..default()
            },
            WorldGeometry,
        ));
    }

    let canopy_center = base + Vec3::Y * (trunk_h * 1.02);
    for branch in 0..5u64 {
        let branch_yaw = yaw + branch as f32 * TAU / 5.0 + seeded(seed, 120 + branch) * 0.34;
        let branch_start = base
            + Vec3::new(
                branch_yaw.cos() * trunk_r * 0.35,
                trunk_h * (0.60 + seeded(seed, 130 + branch) * 0.22),
                branch_yaw.sin() * trunk_r * 0.35,
            );
        let branch_end = canopy_center
            + Vec3::new(
                branch_yaw.cos() * canopy_r * 0.62,
                seeded(seed, 150 + branch) * canopy_r * 0.32,
                branch_yaw.sin() * canopy_r * 0.62,
            );
        spawn_static_cylinder_between_cached(
            commands,
            &mesh_cache.unit_cylinder,
            bark.clone(),
            branch_start,
            branch_end,
            trunk_r * (0.28 + seeded(seed, 170 + branch) * 0.10),
        );
    }

    let lobe_specs = [
        (0.0_f32, 0.0_f32, 0.35_f32, 1.28_f32, 0.90_f32),
        (0.0, 0.0, 1.18, 0.86, 0.72),
        (0.0, TAU * 0.10, 0.72, 0.96, 0.76),
        (TAU * 0.25, 0.0, 0.64, 0.92, 0.74),
        (TAU * 0.50, 0.0, 0.70, 0.98, 0.78),
        (TAU * 0.75, 0.0, 0.66, 0.94, 0.74),
    ];
    for (li, &(angle, extra_yaw, height_mul, width_mul, height_scale)) in
        lobe_specs.iter().enumerate()
    {
        let a = yaw + angle + extra_yaw + seeded(seed, 220 + li as u64) * 0.18;
        let offset = if li < 2 {
            Vec3::ZERO
        } else {
            Vec3::new(a.cos() * canopy_r * 0.72, 0.0, a.sin() * canopy_r * 0.72)
        };
        let lobe_scale = canopy_r * (width_mul + seeded(seed, 240 + li as u64) * 0.22);
        let transform =
            Transform::from_translation(canopy_center + offset + Vec3::Y * (canopy_r * height_mul))
                .with_rotation(Quat::from_rotation_y(a))
                .with_scale(Vec3::new(
                    lobe_scale * 1.15,
                    lobe_scale * height_scale,
                    lobe_scale,
                ));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_sphere.clone()),
                material: MeshMaterial3d(foliage.clone()),
                transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(&transform, a, 0.38, 0.010, 0.020, 0.006),
        ));
    }

    spawn_moss_pad_cached(
        commands,
        pal,
        &mesh_cache.unit_sphere,
        base + Vec3::Y * 0.08,
        trunk_r * 4.8,
        seed + 500,
    );
    spawn_vine_curtain_cached(
        commands,
        pal,
        &mesh_cache.unit_sphere,
        &mesh_cache.unit_cylinder,
        canopy_center + Vec3::Y * canopy_r * 0.45,
        yaw,
        canopy_r * 0.92,
        trunk_h * 0.82,
        6 + seed % 5,
        seed + 700,
    );

    if seed.is_multiple_of(5) {
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: Color::srgb(0.35, 1.0, 0.48),
                    intensity: 6_500.0 * scale.clamp(0.8, 3.0),
                    range: 18.0 * scale.clamp(1.0, 3.4),
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_translation(base + Vec3::Y * (trunk_h * 0.22)),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_mana_waterfalls(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    waterfall_fx: &WaterfallFxAssets,
    seed: u64,
    terrain_seed: u64,
) {
    let falls = [
        (-420.0_f32, 560.0_f32, 2.60_f32, 46.0_f32, 84.0_f32),
        (620.0, -520.0, -0.65, 42.0, 78.0),
        (-960.0, -780.0, 1.92, 54.0, 112.0),
        (1280.0, 980.0, -2.20, 58.0, 126.0),
        (-2860.0, 3420.0, 2.35, 78.0, 210.0),
        (3560.0, -2440.0, -0.86, 82.0, 224.0),
        (-6500.0, 6000.0, 2.75, 98.0, 290.0),
        (5600.0, 1180.0, -1.48, 88.0, 252.0),
    ];

    for (i, &(x, z, yaw, width, drop)) in falls.iter().enumerate() {
        let ground = terrain_surface_y(x, z, terrain_seed);
        spawn_mana_waterfall(
            commands,
            meshes,
            pal,
            waterfall_fx,
            Vec3::new(x, ground, z),
            yaw,
            width,
            drop * (0.85 + seeded(seed, i as u64 * 17) * 0.35),
            seed + i as u64 * 113,
        );
    }
}

fn spawn_mana_waterfall(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    waterfall_fx: &WaterfallFxAssets,
    base: Vec3,
    yaw: f32,
    width: f32,
    drop: f32,
    seed: u64,
) {
    let rot = Quat::from_rotation_y(yaw);
    let forward = rot * Vec3::Z;
    let right = rot * Vec3::X;
    let height = drop.max(48.0);
    let top_y = base.y + height + 8.0;
    let fall_center = Vec3::new(base.x, base.y + height * 0.5, base.z);

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width * 1.25, height + 18.0, 9.5))),
            material: MeshMaterial3d(pal.rock_dark.clone()),
            transform: Transform::from_translation(fall_center - forward * 4.8)
                .with_rotation(rot)
                .with_scale(Vec3::new(1.0, 1.0, 0.82)),
            ..default()
        },
        WorldGeometry,
    ));

    for strip in 0..4u64 {
        let offset = (strip as f32 - 1.5) * width * 0.21;
        let strip_w = width * (0.22 + seeded(seed, strip * 7) * 0.12);
        let strip_h = height * (0.88 + seeded(seed, strip * 7 + 1) * 0.16);
        let transform = Transform::from_translation(
            Vec3::new(base.x, base.y + strip_h * 0.5 + 2.0, base.z)
                + right * offset
                + forward * 0.8,
        )
        .with_rotation(rot)
        .with_scale(Vec3::new(1.0, 1.0, 1.0));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(strip_w, strip_h, 0.52))),
                material: MeshMaterial3d(pal.water.clone()),
                transform,
                ..default()
            },
            WorldGeometry,
            WaterfallFlowRibbon {
                base_translation: transform.translation,
                base_scale: transform.scale,
                phase: seeded(seed + 41, strip),
            },
        ));
    }

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(width * 0.72, 0.20))),
            material: MeshMaterial3d(pal.water.clone()),
            transform: Transform::from_translation(base + forward * 18.0 + Vec3::Y * 0.16)
                .with_rotation(rot)
                .with_scale(Vec3::new(1.45, 1.0, 0.72)),
            ..default()
        },
        WorldGeometry,
    ));

    let mist_mesh = meshes.add(Sphere::new(1.0));
    for puff in 0..7u64 {
        let a = yaw + (seeded(seed, 80 + puff) - 0.5) * 1.6;
        let dist = width * (0.12 + seeded(seed, 90 + puff) * 0.58);
        let puff_scale = width * (0.12 + seeded(seed, 100 + puff) * 0.18);
        let transform = Transform::from_translation(
            base + forward * (10.0 + seeded(seed, 110 + puff) * 20.0)
                + right * (a.sin() * dist)
                + Vec3::Y * (1.4 + seeded(seed, 120 + puff) * 7.0),
        )
        .with_scale(Vec3::new(puff_scale * 1.5, puff_scale * 0.50, puff_scale));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mist_mesh.clone()),
                material: MeshMaterial3d(pal.waterfall_mist.clone()),
                transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(&transform, a, 0.55, 0.012, 0.040, 0.012),
        ));
    }

    for side in [-1.0_f32, 1.0] {
        let ledge_pos = Vec3::new(base.x, top_y - 3.5, base.z) + right * side * width * 0.62;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(width * 0.18))),
                material: MeshMaterial3d(pal.moss.clone()),
                transform: Transform::from_translation(ledge_pos)
                    .with_scale(Vec3::new(1.4, 0.34, 0.9))
                    .with_rotation(rot),
                ..default()
            },
            WorldGeometry,
        ));
    }

    spawn_waterfall_fx(
        commands,
        waterfall_fx,
        base,
        forward,
        right,
        width,
        drop,
        seed + 2_701,
    );
}

fn spawn_range_waylines(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    for (ri, route) in mountain_routes().iter().enumerate() {
        for (ci, (x, z)) in route.iter().copied().enumerate() {
            let checkpoint = speed_road_terrain_point(x, z, seed, SPEED_ROAD_CLEARANCE + 2.0);
            commands.spawn((
                Transform::from_translation(checkpoint),
                GlobalTransform::default(),
                SpeedRoadCheckpoint { radius: 38.0 },
                WorldGeometry,
                Name::new(format!("Speed Road Checkpoint {ri}-{ci}")),
            ));
        }
        for (si, pair) in route.windows(2).enumerate() {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let dx = bx - ax;
            let dz = bz - az;
            let len = (dx * dx + dz * dz).sqrt();
            let count = (len / 1350.0).ceil().max(2.0) as u32;
            for i in 1..count {
                let t = i as f32 / count as f32;
                let wobble =
                    (seeded(seed, ri as u64 * 103 + si as u64 * 19 + i as u64) - 0.5) * 180.0;
                let x = ax + (bx - ax) * t + wobble;
                let z = az + (bz - az) * t - wobble * 0.55;
                let y = terrain_surface_y(x, z, seed) + 2.0;
                let height = 8.0 + seeded(seed, i as u64 + ri as u64 * 17 + si as u64 * 31) * 9.0;
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Cylinder::new(1.1, height))),
                        material: MeshMaterial3d(pal.crystal_aurora.clone()),
                        transform: Transform::from_xyz(x, y + height * 0.5, z),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }
    }
}

fn spawn_mountain_path_network(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    let slab_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let stud_mesh = meshes.add(Sphere::new(1.0));

    for (ri, route) in mountain_routes().iter().enumerate() {
        for (si, pair) in route.windows(2).enumerate() {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let dx = bx - ax;
            let dz = bz - az;
            let len = (dx * dx + dz * dz).sqrt();
            if len <= 10.0 {
                continue;
            }

            let steps = (len / 360.0).ceil().max(1.0) as usize;
            let dir_x = dx / len;
            let dir_z = dz / len;
            let perp_x = dir_z;
            let perp_z = -dir_x;
            let yaw = dx.atan2(dz);
            let width = 18.0 + (ri % 3) as f32 * 3.0;

            for step in 0..steps {
                let t0 = step as f32 / steps as f32;
                let t1 = (step + 1) as f32 / steps as f32;
                let x0 = ax + dx * t0;
                let z0 = az + dz * t0;
                let x1 = ax + dx * t1;
                let z1 = az + dz * t1;
                let mut mx = (x0 + x1) * 0.5;
                let mut mz = (z0 + z1) * 0.5;
                if Vec2::new(mx, mz).length() < 520.0 {
                    continue;
                }

                let side_wander = terrain_fbm_signed(
                    mx,
                    mz,
                    seed,
                    2_801 + ri as u64 * 37 + si as u64 * 11,
                    0.0014,
                ) * width
                    * 0.34;
                mx += perp_x * side_wander;
                mz += perp_z * side_wander;

                let slab_len = ((x1 - x0).powi(2) + (z1 - z0).powi(2)).sqrt() * 0.94;
                let y0 = terrain_surface_y(x0, z0, seed);
                let y1 = terrain_surface_y(x1, z1, seed);
                let ym = terrain_surface_y(mx, mz, seed);
                let y = y0.max(y1).max(ym) + 0.26;
                let lift = ((seeded(seed, ri as u64 * 701 + si as u64 * 53 + step as u64) - 0.5)
                    * 0.08)
                    .clamp(-0.04, 0.04);
                let slab_width = width
                    * (0.88 + seeded(seed, ri as u64 * 811 + si as u64 * 71 + step as u64) * 0.24);
                let pitch = ((y1 - y0) / slab_len.max(1.0)).atan().clamp(-0.18, 0.18);
                let roll = terrain_fbm_signed(
                    mx,
                    mz,
                    seed,
                    2_907 + ri as u64 * 31 + si as u64 * 13,
                    0.0031,
                ) * 0.025;
                let slab_rot = Quat::from_rotation_y(yaw)
                    * Quat::from_rotation_x(-pitch)
                    * Quat::from_rotation_z(roll);

                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(slab_mesh.clone()),
                        material: MeshMaterial3d(pal.mountain_path.clone()),
                        transform: Transform::from_xyz(mx, y + lift, mz)
                            .with_rotation(slab_rot)
                            .with_scale(Vec3::new(slab_width, 0.34, slab_len)),
                        ..default()
                    },
                    WorldGeometry,
                    WalkableSurface,
                    crate::engine::physics::prelude::RigidBody::Fixed,
                    crate::engine::physics::prelude::Collider::cuboid(
                        slab_width * 0.5,
                        0.24,
                        slab_len * 0.5,
                    ),
                    world_space_collider_scale(),
                ));

                if step % 3 == 1 {
                    let glow_radius = 1.05
                        + seeded(seed, 90_000 + ri as u64 * 79 + si as u64 * 11 + step as u64)
                            * 0.42;
                    let side_offset = slab_width * 0.72;
                    for side in [-1.0_f32, 1.0] {
                        let sx = mx + perp_x * side_offset * side;
                        let sz = mz + perp_z * side_offset * side;
                        let sy = terrain_surface_y(sx, sz, seed) + 1.35;
                        commands.spawn((
                            PbrBundle {
                                mesh: Mesh3d(stud_mesh.clone()),
                                material: MeshMaterial3d(pal.guide_glow.clone()),
                                transform: Transform::from_xyz(sx, sy, sz)
                                    .with_scale(Vec3::splat(glow_radius)),
                                ..default()
                            },
                            WorldGeometry,
                        ));
                    }
                }

                if step % 5 == 2 {
                    for side in [-1.0_f32, 1.0] {
                        let rock_x = mx + perp_x * slab_width * 0.58 * side;
                        let rock_z = mz + perp_z * slab_width * 0.58 * side;
                        let rock_y = terrain_surface_y(rock_x, rock_z, seed) + 0.72;
                        let rock_scale = 0.72
                            + seeded(seed, 94_000 + ri as u64 * 83 + si as u64 * 17 + step as u64)
                                * 0.64;
                        commands.spawn((
                            PbrBundle {
                                mesh: Mesh3d(stud_mesh.clone()),
                                material: MeshMaterial3d(pal.bridge_stone.clone()),
                                transform: Transform::from_xyz(rock_x, rock_y, rock_z).with_scale(
                                    Vec3::new(rock_scale * 1.2, rock_scale * 0.62, rock_scale),
                                ),
                                ..default()
                            },
                            WorldGeometry,
                        ));
                    }
                }

                if step % 8 == 3 {
                    commands.spawn(PointLightBundle {
                        point_light: PointLight {
                            color: Color::srgb(0.38, 0.96, 1.0),
                            intensity: 14_000.0,
                            range: 72.0,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        transform: Transform::from_xyz(mx, y + 7.5, mz),
                        ..default()
                    });
                }
            }
        }
    }
}

fn spawn_mountain_route_bridges(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    for (ri, route) in mountain_routes().iter().enumerate() {
        for (si, pair) in route.windows(2).enumerate() {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let dx = bx - ax;
            let dz = bz - az;
            let segment_len = (dx * dx + dz * dz).sqrt();
            if segment_len < 1250.0 {
                continue;
            }

            let roll = seeded(seed, ri as u64 * 97 + si as u64 * 17);
            if roll < 0.18 {
                continue;
            }

            let center_t = 0.40 + seeded(seed, ri as u64 * 131 + si as u64 * 29) * 0.20;
            let span_len = (segment_len * (0.22 + roll * 0.12)).clamp(340.0, 820.0);
            let half_t = (span_len * 0.5 / segment_len).min(0.22);
            let t0 = (center_t - half_t).clamp(0.04, 0.92);
            let t1 = (center_t + half_t).clamp(0.08, 0.96);
            if t1 <= t0 + 0.03 {
                continue;
            }

            let start_x = ax + dx * t0;
            let start_z = az + dz * t0;
            let end_x = ax + dx * t1;
            let end_z = az + dz * t1;
            if Vec2::new((start_x + end_x) * 0.5, (start_z + end_z) * 0.5).length() < 720.0 {
                continue;
            }

            let start_ground = terrain_surface_y(start_x, start_z, terrain_seed);
            let end_ground = terrain_surface_y(end_x, end_z, terrain_seed);
            let mid_x = (start_x + end_x) * 0.5;
            let mid_z = (start_z + end_z) * 0.5;
            let mid_ground = terrain_surface_y(mid_x, mid_z, terrain_seed);
            let clearance = 7.5 + seeded(seed, ri as u64 * 181 + si as u64 * 37) * 8.0;
            let mut start_y = start_ground + clearance;
            let mut end_y = end_ground + clearance;
            let mid_y = (start_y + end_y) * 0.5;
            let needed_lift = (mid_ground + clearance + 5.0 - mid_y).max(0.0);
            start_y += needed_lift;
            end_y += needed_lift;

            let start = Vec3::new(start_x, start_y, start_z);
            let end = Vec3::new(end_x, end_y, end_z);
            let delta = end - start;
            let horizontal_len = Vec2::new(delta.x, delta.z).length();
            if horizontal_len <= 10.0 {
                continue;
            }

            let yaw = delta.x.atan2(delta.z);
            let pitch = (delta.y / horizontal_len).atan().clamp(-0.28, 0.28);
            let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
            let side_rot = Quat::from_rotation_y(yaw);
            let width = 18.0 + (ri % 3) as f32 * 2.8;
            let center = start + delta * 0.5;

            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.bridge_deck.clone()),
                    transform: Transform::from_translation(center)
                        .with_rotation(rot)
                        .with_scale(Vec3::new(width, 0.68, horizontal_len)),
                    ..default()
                },
                WorldGeometry,
                WalkableSurface,
                crate::engine::physics::prelude::RigidBody::Fixed,
                crate::engine::physics::prelude::Collider::cuboid(
                    width * 0.5,
                    0.55,
                    horizontal_len * 0.5,
                ),
                world_space_collider_scale(),
            ));

            spawn_boost_road_span(
                commands,
                meshes,
                pal,
                center + Vec3::Y * 0.86,
                yaw,
                pitch,
                horizontal_len * 0.78,
                width * 0.58,
                true,
                seed + ri as u64 * 409 + si as u64 * 53,
            );

            let plank_count = 6;
            for plank in 0..plank_count {
                let t = (plank as f32 + 0.5) / plank_count as f32;
                let sag = -(1.0 - (t * 2.0 - 1.0).abs()).powi(2)
                    * (0.32 + seeded(seed, ri as u64 * 223 + si as u64 * 41) * 0.46);
                let p = start.lerp(end, t) + Vec3::Y * (0.78 + sag);
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(unit_cube.clone()),
                        material: MeshMaterial3d(pal.bridge_deck.clone()),
                        transform: Transform::from_translation(p)
                            .with_rotation(rot)
                            .with_scale(Vec3::new(width * 0.96, 0.18, horizontal_len / 7.4)),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }

            for side in [-1.0_f32, 1.0] {
                let local_side = Vec3::new(side * width * 0.58, 1.95, 0.0);
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(unit_cube.clone()),
                        material: MeshMaterial3d(pal.bridge_stone.clone()),
                        transform: Transform::from_translation(center + rot * local_side)
                            .with_rotation(rot)
                            .with_scale(Vec3::new(0.42, 0.52, horizontal_len * 0.94)),
                        ..default()
                    },
                    WorldGeometry,
                ));

                let mut last_cable: Option<Vec3> = None;
                for cable_step in 0..=6 {
                    let t = cable_step as f32 / 6.0;
                    let arch = 5.8 - (1.0 - (t * 2.0 - 1.0).abs()).powi(2) * 1.9;
                    let p =
                        start.lerp(end, t) + side_rot * Vec3::new(side * width * 0.68, arch, 0.0);
                    if let Some(last) = last_cable {
                        spawn_static_cylinder_between(
                            commands,
                            meshes,
                            pal.bridge_stone.clone(),
                            last,
                            p,
                            0.18,
                        );
                    }
                    last_cable = Some(p);
                }
            }

            for anchor in [start, end] {
                let ground = terrain_surface_y(anchor.x, anchor.z, terrain_seed);
                let tower_h = (anchor.y - ground + 2.4).max(5.0);
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(unit_cube.clone()),
                        material: MeshMaterial3d(pal.bridge_stone.clone()),
                        transform: Transform::from_xyz(anchor.x, ground + tower_h * 0.5, anchor.z)
                            .with_rotation(Quat::from_rotation_y(yaw))
                            .with_scale(Vec3::new(width * 0.72, tower_h, 4.8)),
                        ..default()
                    },
                    WorldGeometry,
                    crate::engine::physics::prelude::RigidBody::Fixed,
                    crate::engine::physics::prelude::Collider::cuboid(
                        width * 0.36,
                        tower_h * 0.5,
                        2.4,
                    ),
                    world_space_collider_scale(),
                ));
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Cone {
                            radius: width * 0.18,
                            height: 5.2,
                        })),
                        material: MeshMaterial3d(if (ri + si) % 2 == 0 {
                            pal.crystal_emerald.clone()
                        } else {
                            pal.crystal_aurora.clone()
                        }),
                        transform: Transform::from_xyz(anchor.x, anchor.y + 4.2, anchor.z)
                            .with_rotation(Quat::from_rotation_y(yaw)),
                        ..default()
                    },
                    WorldGeometry,
                ));
                commands.spawn((
                    PointLightBundle {
                        point_light: PointLight {
                            color: if (ri + si) % 2 == 0 {
                                Color::srgb(0.20, 1.0, 0.58)
                            } else {
                                Color::srgb(0.48, 0.86, 1.0)
                            },
                            intensity: 8_500.0,
                            range: 42.0,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        transform: Transform::from_xyz(anchor.x, anchor.y + 5.0, anchor.z),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }
    }
}

fn spawn_range_outposts(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    for location in chapter_map_locations() {
        if matches!(location.id.0, 6..=11) {
            continue;
        }
        spawn_range_outpost(commands, meshes, pal, seed, location);
    }
}

fn spawn_range_outpost(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    location: &crate::chapters::ChapterMapLocation,
) {
    let ground = terrain_surface_y(location.x, location.z, seed);
    let base_y = ground + 1.0;
    let yaw = location.facing_yaw;
    let rot = Quat::from_rotation_y(yaw);
    let width = if location.id.0 == 1 { 52.0 } else { 38.0 };
    let depth = if location.id.0 == 1 { 34.0 } else { 28.0 };
    let mat = match location.id.0 {
        3 => pal.castle_stone.clone(),
        5 => pal.rock_dark.clone(),
        12 => pal.brushed_metal.clone(),
        13 | 14 => pal.sky_platform.clone(),
        _ => pal.downtown_facade.clone(),
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width, 2.0, depth))),
            material: MeshMaterial3d(mat),
            transform: Transform::from_xyz(location.x, base_y, location.z).with_rotation(rot),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(width * 0.5, 1.0, depth * 0.5),
    ));

    for side in [-1.0_f32, 1.0] {
        let local = Vec3::new(side * (width * 0.32), 8.0, -depth * 0.18);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(5.0, 16.0, 5.0))),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_translation(
                    Vec3::new(location.x, base_y, location.z) + rot * local,
                )
                .with_rotation(rot),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let dome = Transform::from_xyz(location.x, base_y + 4.2, location.z)
        .with_rotation(rot)
        .with_scale(Vec3::new(2.4, 0.45, 1.6));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(7.0))),
            material: MeshMaterial3d(pal.glass_panel.clone()),
            transform: dome,
            ..default()
        },
        WorldGeometry,
    ));

    let mast_height = 20.0 + seeded(seed, location.id.0 as u64 * 17 + 5) * 12.0;
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(0.9, mast_height))),
            material: MeshMaterial3d(pal.crystal_aurora.clone()),
            transform: Transform::from_xyz(
                location.x,
                base_y + mast_height * 0.5 + 2.0,
                location.z,
            ),
            ..default()
        },
        WorldGeometry,
    ));

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.36, 0.86, 1.0),
                intensity: 35_000.0,
                range: 90.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(location.x, base_y + 18.0, location.z),
            ..default()
        },
        WorldGeometry,
    ));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettlementTerrainMode {
    Grounded,
    Terraced,
    SkyDistrict,
}

#[derive(Debug, Clone, Copy)]
struct SettlementTerrainLayout {
    mode: SettlementTerrainMode,
    center_y: f32,
    floor_y: f32,
    relief: f32,
    max_grade: f32,
    plaza_radius: f32,
    platform_radius: f32,
}

impl SettlementTerrainLayout {
    fn floor_y_at(&self, x: f32, z: f32, seed: u64) -> f32 {
        match self.mode {
            SettlementTerrainMode::SkyDistrict => self.floor_y,
            SettlementTerrainMode::Grounded => terrain_surface_y(x, z, seed),
            SettlementTerrainMode::Terraced => {
                let ground = terrain_surface_y(x, z, seed);
                let step = 7.5 + (self.relief * 0.035).clamp(0.0, 4.0);
                let terrace = self.center_y + ((ground - self.center_y) / step).ceil() * step;
                terrace.max(ground)
            }
        }
    }

    fn is_sky_district(&self) -> bool {
        matches!(self.mode, SettlementTerrainMode::SkyDistrict)
    }

    fn needs_mountain_inset(&self, settlement: MapSettlement) -> bool {
        settlement.kind != MapSettlementKind::Harbor
            && (self.relief > 18.0
                || self.max_grade > 0.075
                || matches!(self.mode, SettlementTerrainMode::SkyDistrict))
    }
}

fn settlement_layout_profile(
    seed: u64,
    settlement: MapSettlement,
    plaza_radius: f32,
) -> SettlementTerrainLayout {
    let center_y = terrain_surface_y(settlement.x, settlement.z, seed);
    let sample_radius = match settlement.kind {
        MapSettlementKind::City => plaza_radius * 2.2,
        MapSettlementKind::Village => plaza_radius * 1.9,
        MapSettlementKind::Harbor => plaza_radius * 1.55,
        MapSettlementKind::Outpost => plaza_radius * 1.8,
    };

    let rot = Quat::from_rotation_y(settlement.facing_yaw);
    let mut low = center_y;
    let mut high = center_y;
    let mut max_grade = 0.0_f32;
    for i in 0..12 {
        let angle = i as f32 * std::f32::consts::TAU / 12.0;
        for radius in [sample_radius * 0.55, sample_radius] {
            let local = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
            let world = Vec3::new(settlement.x, 0.0, settlement.z) + rot * local;
            let y = terrain_surface_y(world.x, world.z, seed);
            low = low.min(y);
            high = high.max(y);
            max_grade = max_grade.max((y - center_y).abs() / radius.max(1.0));
        }
    }

    let relief = high - low;
    let mode = if matches!(settlement.kind, MapSettlementKind::City) {
        SettlementTerrainMode::SkyDistrict
    } else if relief > 14.0 || max_grade > 0.055 {
        SettlementTerrainMode::Terraced
    } else {
        SettlementTerrainMode::Grounded
    };
    let platform_radius = match settlement.kind {
        MapSettlementKind::City => 136.0 + relief.clamp(0.0, 70.0) * 0.45,
        MapSettlementKind::Village => plaza_radius + 18.0,
        MapSettlementKind::Harbor => plaza_radius + 20.0,
        MapSettlementKind::Outpost => plaza_radius + 16.0,
    };
    let floor_y = match mode {
        SettlementTerrainMode::SkyDistrict => center_y + 88.0 + relief.clamp(0.0, 120.0) * 0.38,
        SettlementTerrainMode::Terraced => center_y,
        SettlementTerrainMode::Grounded => center_y,
    };

    SettlementTerrainLayout {
        mode,
        center_y,
        floor_y,
        relief,
        max_grade,
        plaza_radius,
        platform_radius,
    }
}

fn settlement_plaza_radius(kind: MapSettlementKind) -> f32 {
    match kind {
        MapSettlementKind::City => 58.0,
        MapSettlementKind::Village => 38.0,
        MapSettlementKind::Harbor => 46.0,
        MapSettlementKind::Outpost => 34.0,
    }
}

fn spawn_exploration_settlements(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    seed: u64,
    economy: &SettlementEconomy,
) {
    for settlement in map_settlements() {
        spawn_exploration_settlement(commands, meshes, materials, pal, seed, *settlement, economy);
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_exploration_settlement(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    economy: &SettlementEconomy,
) {
    let plaza_radius = settlement_plaza_radius(settlement.kind);
    let layout = settlement_layout_profile(seed, settlement, plaza_radius);
    let origin = Vec3::new(settlement.x, layout.floor_y, settlement.z);
    let rot = Quat::from_rotation_y(settlement.facing_yaw);

    if layout.is_sky_district() {
        spawn_settlement_sky_foundation(
            commands, meshes, pal, seed, settlement, layout, origin, rot,
        );
    } else if matches!(layout.mode, SettlementTerrainMode::Terraced) {
        spawn_settlement_terrace_foundation(commands, meshes, pal, layout, origin, rot);
    }

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(plaza_radius, 0.55))),
            material: MeshMaterial3d(match settlement.kind {
                MapSettlementKind::City => pal.downtown_facade.clone(),
                MapSettlementKind::Village => pal.stone_brick.clone(),
                MapSettlementKind::Harbor => pal.street_asphalt.clone(),
                MapSettlementKind::Outpost => pal.brushed_metal.clone(),
            }),
            transform: Transform::from_xyz(settlement.x, layout.floor_y + 0.28, settlement.z),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(0.28, plaza_radius),
    ));

    spawn_settlement_roads(commands, meshes, pal, seed, settlement, origin, rot, layout);
    spawn_settlement_buildings(commands, meshes, pal, seed, settlement, origin, rot, layout);
    spawn_settlement_landmark(commands, meshes, pal, seed, settlement, origin, rot, layout);
    spawn_settlement_cache(
        commands, meshes, materials, seed, settlement, origin, rot, layout,
    );
    spawn_settlement_discussion_npc(commands, meshes, pal, seed, settlement, origin, rot, layout);
    spawn_settlement_mountain_inset(commands, meshes, pal, seed, settlement, origin, rot, layout);
    spawn_settlement_build_terminal(commands, meshes, pal, seed, settlement, origin, rot, layout);
    spawn_settlement_economy_modules(
        commands, meshes, pal, seed, settlement, origin, rot, layout, economy,
    );
    if matches!(settlement.kind, MapSettlementKind::City) {
        spawn_city_guardian_ships(commands, meshes, pal, seed, settlement, origin);
        spawn_city_spy_drones(commands, meshes, pal, seed, settlement, origin);
    }
    spawn_world_anchor(commands, settlement.anchor_id, origin + Vec3::Y * 2.4);
}

#[allow(clippy::too_many_arguments)]
fn spawn_settlement_sky_foundation(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    layout: SettlementTerrainLayout,
    origin: Vec3,
    rot: Quat,
) {
    let platform_height = 5.2;
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(layout.platform_radius, platform_height))),
            material: MeshMaterial3d(pal.sky_platform.clone()),
            transform: Transform::from_xyz(origin.x, origin.y - platform_height * 0.5, origin.z),
            ..default()
        },
        WorldGeometry,
        SkyPlatform,
        WalkableSurface,
        Building {
            zone: WorldZone::SkyPlatform,
            height: platform_height,
        },
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(
            platform_height * 0.5,
            layout.platform_radius,
        ),
    ));

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(layout.platform_radius * 0.36, 18.0))),
            material: MeshMaterial3d(pal.glass_panel.clone()),
            transform: Transform::from_xyz(origin.x, origin.y - 13.0, origin.z),
            ..default()
        },
        WorldGeometry,
    ));

    for i in 0..12 {
        let angle = i as f32 * std::f32::consts::TAU / 12.0;
        let local = Vec3::new(
            angle.cos() * layout.platform_radius * 0.88,
            0.0,
            angle.sin() * layout.platform_radius * 0.88,
        );
        let pos = origin + local;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(1.25, 7.5))),
                material: MeshMaterial3d(pal.guide_glow.clone()),
                transform: Transform::from_xyz(pos.x, origin.y + 4.0, pos.z),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // Four world-cardinal approaches preserve an obvious route onto the
    // floating foundation from every district, independent of facade yaw.
    for i in 0..4 {
        let angle = i as f32 * std::f32::consts::FRAC_PI_2;
        let dir = Vec2::new(angle.sin(), angle.cos()).normalize();
        let start_x = settlement.x + dir.x * (layout.platform_radius + 260.0);
        let start_z = settlement.z + dir.y * (layout.platform_radius + 260.0);
        let start_ground = terrain_surface_y(start_x, start_z, seed);
        let end_x = settlement.x + dir.x * (layout.platform_radius - 12.0);
        let end_z = settlement.z + dir.y * (layout.platform_radius - 12.0);
        let start = Vec3::new(start_x, start_ground + 1.2, start_z);
        let end = Vec3::new(end_x, origin.y + 0.42, end_z);
        spawn_settlement_ramp_span(
            commands,
            meshes,
            pal,
            start,
            end,
            17.0,
            0.9,
            i as u64 + settlement.anchor_id.len() as u64 * 41,
        );
    }

    let gate_local = Vec3::new(0.0, 0.0, layout.platform_radius + 14.0);
    let gate = origin + rot * gate_local;
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.35, 0.90, 1.0),
                intensity: 42_000.0,
                range: 150.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(gate.x, origin.y + 12.0, gate.z),
            ..default()
        },
        WorldGeometry,
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_settlement_ramp_span(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    start: Vec3,
    end: Vec3,
    width: f32,
    thickness: f32,
    seed: u64,
) {
    let delta = end - start;
    let horizontal = Vec2::new(delta.x, delta.z).length().max(0.01);
    let length = delta.length().max(0.01);
    let yaw = delta.x.atan2(delta.z);
    let pitch = (delta.y / horizontal).atan();
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    let center = start.lerp(end, 0.5);

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width, thickness, length))),
            material: MeshMaterial3d(pal.mountain_path.clone()),
            transform: Transform::from_translation(center).with_rotation(rotation),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            width * 0.5,
            thickness * 0.5,
            length * 0.5,
        ),
    ));

    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.55, 1.4, length))),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_translation(
                    center + rotation * Vec3::new(side * width * 0.48, 0.82, 0.0),
                )
                .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let stud_count = (length / 42.0).ceil().clamp(3.0, 16.0) as usize;
    for i in 0..=stud_count {
        let t = i as f32 / stud_count as f32;
        let base = start.lerp(end, t);
        let pulse = 0.65 + seeded(seed, i as u64 * 5 + 9) * 0.55;
        for side in [-1.0_f32, 1.0] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Sphere::new(pulse))),
                    material: MeshMaterial3d(pal.guide_glow.clone()),
                    transform: Transform::from_translation(
                        base + rotation * Vec3::new(side * width * 0.38, 1.15, 0.0),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

fn spawn_settlement_terrace_foundation(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    layout: SettlementTerrainLayout,
    origin: Vec3,
    rot: Quat,
) {
    let ring_count = 10;
    for i in 0..ring_count {
        let angle = i as f32 * std::f32::consts::TAU / ring_count as f32;
        let radius = layout.plaza_radius + 8.0;
        let local = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
        let pos = origin + rot * local;
        let tangent_yaw = angle + std::f32::consts::FRAC_PI_2;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(18.0, 3.4, 2.2))),
                material: MeshMaterial3d(if i % 2 == 0 {
                    pal.stone_block.clone()
                } else {
                    pal.mortar_line.clone()
                }),
                transform: Transform::from_xyz(pos.x, origin.y + 1.4, pos.z)
                    .with_rotation(rot * Quat::from_rotation_y(tangent_yaw)),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_settlement_roads(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
    rot: Quat,
    layout: SettlementTerrainLayout,
) {
    let road_len = layout.plaza_radius * if layout.is_sky_district() { 3.35 } else { 2.85 };
    let segment_count = if layout.is_sky_district() { 8 } else { 6 };
    let segment_len = road_len / segment_count as f32;
    let road_width = match settlement.kind {
        MapSettlementKind::City => 12.0,
        MapSettlementKind::Village => 8.0,
        MapSettlementKind::Harbor => 10.5,
        MapSettlementKind::Outpost => 9.0,
    };
    let material = match settlement.kind {
        MapSettlementKind::Village => pal.mortar_line.clone(),
        MapSettlementKind::Harbor => pal.highway.clone(),
        _ => pal.street_asphalt.clone(),
    };

    for axis in 0..2 {
        let axis_rot = rot * Quat::from_rotation_y(axis as f32 * std::f32::consts::FRAC_PI_2);
        for segment in 0..segment_count {
            let center_offset = -road_len * 0.5 + segment_len * (segment as f32 + 0.5);
            let local = if axis == 0 {
                Vec3::new(center_offset, 0.0, 0.0)
            } else {
                Vec3::new(0.0, 0.0, center_offset)
            };
            let world = origin + rot * local;
            let floor_y = layout.floor_y_at(world.x, world.z, seed);
            let size = if axis == 0 {
                Vec3::new(segment_len * 0.94, 0.28, road_width)
            } else {
                Vec3::new(road_width, 0.28, segment_len * 0.94)
            };
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                    material: MeshMaterial3d(material.clone()),
                    transform: Transform::from_xyz(world.x, floor_y + 0.18, world.z)
                        .with_rotation(axis_rot),
                    ..default()
                },
                WorldGeometry,
                WalkableSurface,
            ));

            if layout.is_sky_district() && segment % 2 == 0 {
                let stud_pos = Vec3::new(world.x, floor_y + 0.92, world.z);
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Sphere::new(0.72))),
                        material: MeshMaterial3d(pal.guide_glow.clone()),
                        transform: Transform::from_translation(stud_pos),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }
    }
}

fn settlement_building_material(
    pal: &Palette,
    kind: MapSettlementKind,
    index: usize,
) -> Handle<StandardMaterial> {
    match kind {
        MapSettlementKind::City => match index % 4 {
            0 => pal.downtown_a.clone(),
            1 => pal.downtown_b.clone(),
            2 => pal.glass_panel.clone(),
            _ => pal.downtown_facade.clone(),
        },
        MapSettlementKind::Village => match index % 3 {
            0 => pal.residential_a.clone(),
            1 => pal.stone_brick.clone(),
            _ => pal.castle_stone.clone(),
        },
        MapSettlementKind::Harbor => match index % 3 {
            0 => pal.brushed_metal.clone(),
            1 => pal.residential_b.clone(),
            _ => pal.glass_panel.clone(),
        },
        MapSettlementKind::Outpost => match index % 3 {
            0 => pal.brushed_metal.clone(),
            1 => pal.sky_platform.clone(),
            _ => pal.downtown_facade.clone(),
        },
    }
}

fn spawn_settlement_buildings(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
    rot: Quat,
    layout: SettlementTerrainLayout,
) {
    let count = match settlement.kind {
        MapSettlementKind::City => 10,
        MapSettlementKind::Village => 7,
        MapSettlementKind::Harbor => 8,
        MapSettlementKind::Outpost => 5,
    };
    let ring = match settlement.kind {
        MapSettlementKind::City => 78.0,
        MapSettlementKind::Village => 54.0,
        MapSettlementKind::Harbor => 62.0,
        MapSettlementKind::Outpost => 46.0,
    };

    for i in 0..count {
        let angle = i as f32 * std::f32::consts::TAU / count as f32
            + seeded(seed, i as u64 + settlement.anchor_id.len() as u64) * 0.28;
        let radius = ring * (0.72 + seeded(seed, i as u64 * 7 + 3) * 0.38);
        let local = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
        let world = origin + rot * local;
        let floor_y = layout.floor_y_at(world.x, world.z, seed);
        let height = match settlement.kind {
            MapSettlementKind::City => 28.0 + seeded(seed, i as u64 * 13 + 1) * 58.0,
            MapSettlementKind::Village => 9.0 + seeded(seed, i as u64 * 13 + 1) * 12.0,
            MapSettlementKind::Harbor => 12.0 + seeded(seed, i as u64 * 13 + 1) * 20.0,
            MapSettlementKind::Outpost => 14.0 + seeded(seed, i as u64 * 13 + 1) * 24.0,
        };
        let width = match settlement.kind {
            MapSettlementKind::City => 12.0 + seeded(seed, i as u64 * 13 + 2) * 14.0,
            MapSettlementKind::Village => 10.0 + seeded(seed, i as u64 * 13 + 2) * 8.0,
            MapSettlementKind::Harbor => 12.0 + seeded(seed, i as u64 * 13 + 2) * 12.0,
            MapSettlementKind::Outpost => 10.0 + seeded(seed, i as u64 * 13 + 2) * 10.0,
        };
        let depth = width * (0.75 + seeded(seed, i as u64 * 13 + 3) * 0.55);
        let material = settlement_building_material(pal, settlement.kind, i);
        let yaw = settlement.facing_yaw + angle + std::f32::consts::FRAC_PI_2;

        spawn_settlement_building_pad(
            commands,
            meshes,
            pal,
            layout,
            Vec3::new(world.x, floor_y, world.z),
            width,
            depth,
            yaw,
            i,
        );

        let zone = match settlement.kind {
            MapSettlementKind::City => WorldZone::Downtown,
            MapSettlementKind::Village => WorldZone::Residential,
            MapSettlementKind::Harbor => WorldZone::OuterDistrict,
            MapSettlementKind::Outpost => WorldZone::Spaceport,
        };
        let interior_slot = if matches!(settlement.kind, MapSettlementKind::City) {
            i % 4 == 0
        } else {
            i == 0
        };
        let enterable = interior_slot
            && spawn_enterable_building(
                commands,
                meshes,
                pal,
                material.clone(),
                Vec3::new(world.x, floor_y + height * 0.5, world.z),
                width,
                height,
                depth,
                yaw,
                zone,
                seed + i as u64 * 127 + settlement.anchor_id.len() as u64,
            );
        if !enterable {
            spawn_building(
                commands,
                meshes,
                material,
                Vec3::new(world.x, floor_y + height * 0.5, world.z),
                width,
                height,
                depth,
                zone,
            );
        }

        if !enterable && !matches!(settlement.kind, MapSettlementKind::City) {
            spawn_small_building_details(
                commands,
                meshes,
                pal,
                Vec3::new(world.x, floor_y + height * 0.5, world.z),
                width,
                height,
                depth,
                seed + i as u64,
                i % 2 == 0,
            );
        }

        if !enterable
            && matches!(
                settlement.kind,
                MapSettlementKind::City | MapSettlementKind::Harbor
            )
            && i % 2 == 0
        {
            spawn_settlement_window_facades(
                commands,
                meshes,
                if i % 4 == 0 {
                    pal.window_warm.clone()
                } else {
                    pal.window_cool.clone()
                },
                world.x,
                floor_y,
                world.z,
                height,
                width,
                depth,
            );
        }

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(width * 0.35, 0.42, depth * 0.35))),
                material: MeshMaterial3d(pal.street_paint.clone()),
                transform: Transform::from_xyz(world.x, floor_y + height + 0.22, world.z)
                    .with_rotation(Quat::from_rotation_y(yaw)),
                ..default()
            },
            WorldGeometry,
        ));

        if !enterable && matches!(settlement.kind, MapSettlementKind::City) {
            spawn_sky_tower_accents(
                commands,
                meshes,
                pal,
                Vec3::new(world.x, floor_y + height * 0.5, world.z),
                width,
                height,
                depth,
                yaw,
                i,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_settlement_building_pad(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    layout: SettlementTerrainLayout,
    base: Vec3,
    width: f32,
    depth: f32,
    yaw: f32,
    index: usize,
) {
    let pad_extra = if layout.is_sky_district() { 7.0 } else { 3.5 };
    let pad_height = if layout.is_sky_district() { 1.25 } else { 0.75 };
    let material = if layout.is_sky_district() {
        pal.sky_platform.clone()
    } else if matches!(layout.mode, SettlementTerrainMode::Terraced) {
        pal.stone_block.clone()
    } else {
        pal.mountain_path.clone()
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(
                width + pad_extra,
                pad_height,
                depth + pad_extra,
            ))),
            material: MeshMaterial3d(material),
            transform: Transform::from_xyz(base.x, base.y + pad_height * 0.5, base.z)
                .with_rotation(Quat::from_rotation_y(yaw)),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            (width + pad_extra) * 0.5,
            pad_height * 0.5,
            (depth + pad_extra) * 0.5,
        ),
    ));

    if matches!(layout.mode, SettlementTerrainMode::Terraced) {
        let pillar_height = (base.y - layout.center_y).abs().clamp(2.0, 18.0);
        let pillar_radius = (width.min(depth) * 0.24).max(1.6);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(pillar_radius, pillar_height))),
                material: MeshMaterial3d(pal.rock_dark.clone()),
                transform: Transform::from_xyz(base.x, base.y - pillar_height * 0.5 + 0.15, base.z),
                ..default()
            },
            WorldGeometry,
        ));
    }

    if layout.is_sky_district() && index.is_multiple_of(2) {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(1.05, 24.0))),
                material: MeshMaterial3d(pal.guide_glow.clone()),
                transform: Transform::from_xyz(base.x, base.y - 13.0, base.z),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_sky_tower_accents(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    yaw: f32,
    index: usize,
) {
    let rot = Quat::from_rotation_y(yaw);
    let base_y = center.y - height * 0.5;
    let balcony_y = base_y + height * (0.42 + (index % 3) as f32 * 0.12).min(0.72);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width + 6.0, 0.36, depth + 6.0))),
            material: MeshMaterial3d(pal.glass_panel.clone()),
            transform: Transform::from_xyz(center.x, balcony_y, center.z).with_rotation(rot),
            ..default()
        },
        WorldGeometry,
    ));

    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(0.42, height * 0.82))),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_translation(
                    center + rot * Vec3::new(side * width * 0.54, 0.0, -depth * 0.54),
                ),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_settlement_window_facades(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mat: Handle<StandardMaterial>,
    x: f32,
    ground_y: f32,
    z: f32,
    h: f32,
    w: f32,
    d: f32,
) {
    let win_h = h * 0.55;
    let center_y = ground_y + h * 0.18 + win_h * 0.5;
    let thickness = 0.28;

    for &fz in &[z - d * 0.5 - thickness * 0.5, z + d * 0.5 + thickness * 0.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new((w - 0.8).max(1.0), win_h, thickness))),
                material: MeshMaterial3d(mat.clone()),
                transform: Transform::from_xyz(x, center_y, fz),
                ..default()
            },
            WorldGeometry,
        ));
    }
    for &fx in &[x - w * 0.5 - thickness * 0.5, x + w * 0.5 + thickness * 0.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(thickness, win_h, (d - 0.8).max(1.0)))),
                material: MeshMaterial3d(mat.clone()),
                transform: Transform::from_xyz(fx, center_y, z),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_settlement_landmark(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
    rot: Quat,
    layout: SettlementTerrainLayout,
) {
    let landmark_height = match settlement.kind {
        MapSettlementKind::City => 44.0,
        MapSettlementKind::Village => 20.0,
        MapSettlementKind::Harbor => 30.0,
        MapSettlementKind::Outpost => 34.0,
    };
    let marker = origin + rot * Vec3::new(0.0, 0.0, -18.0);
    let ground = layout.floor_y_at(marker.x, marker.z, seed);

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(2.0, landmark_height))),
            material: MeshMaterial3d(match settlement.kind {
                MapSettlementKind::Village => pal.crystal_aurora.clone(),
                MapSettlementKind::Harbor => pal.window_cool.clone(),
                MapSettlementKind::Outpost => pal.crystal_dragon.clone(),
                MapSettlementKind::City => pal.glass_panel.clone(),
            }),
            transform: Transform::from_xyz(
                marker.x,
                ground + landmark_height * 0.5 + 1.0,
                marker.z,
            ),
            ..default()
        },
        WorldGeometry,
    ));

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: match settlement.kind {
                    MapSettlementKind::City => Color::srgb(0.35, 0.82, 1.0),
                    MapSettlementKind::Village => Color::srgb(0.86, 0.65, 1.0),
                    MapSettlementKind::Harbor => Color::srgb(0.42, 0.90, 1.0),
                    MapSettlementKind::Outpost => Color::srgb(1.0, 0.46, 0.18),
                },
                intensity: 38_000.0,
                range: 120.0 + seeded(seed, settlement.anchor_id.len() as u64) * 80.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(marker.x, ground + landmark_height + 6.0, marker.z),
            ..default()
        },
        WorldGeometry,
    ));

    if matches!(settlement.kind, MapSettlementKind::Harbor) {
        for i in [-1.0_f32, 1.0] {
            let dock = origin + rot * Vec3::new(i * 18.0, 0.0, 54.0);
            let dock_ground = layout.floor_y_at(dock.x, dock.z, seed);
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(10.0, 0.8, 52.0))),
                    material: MeshMaterial3d(pal.rooftop.clone()),
                    transform: Transform::from_xyz(dock.x, dock_ground + 0.5, dock.z)
                        .with_rotation(rot),
                    ..default()
                },
                WorldGeometry,
                WalkableSurface,
            ));
        }
    }
}

fn spawn_settlement_cache(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
    rot: Quat,
    layout: SettlementTerrainLayout,
) {
    let (credits, experience, armor) = match settlement.kind {
        MapSettlementKind::City => (120, 70, 8),
        MapSettlementKind::Village => (70, 42, 6),
        MapSettlementKind::Harbor => (95, 55, 7),
        MapSettlementKind::Outpost => (105, 62, 9),
    };
    let cache_pos = origin + rot * Vec3::new(0.0, 3.0, 26.0);
    let cache_ground = layout.floor_y_at(cache_pos.x, cache_pos.z, seed);
    let reward = DiscoverableKind::HiddenReward {
        reward_id: settlement.reward_id,
        credits,
        experience,
        armor,
        power_up: None,
        special_ability: None,
    };
    spawn_discoverable_beacon(
        commands,
        meshes,
        materials,
        reward,
        settlement.name,
        Vec3::new(cache_pos.x, cache_ground + 3.2, cache_pos.z),
    );
}

fn spawn_settlement_discussion_npc(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
    rot: Quat,
    layout: SettlementTerrainLayout,
) {
    let script_id = settlement_discussion_id(settlement.anchor_id);
    let Some(script) = discussion_script(script_id) else {
        return;
    };

    let local = match settlement.kind {
        MapSettlementKind::City => Vec3::new(-22.0, 0.0, -12.0),
        MapSettlementKind::Village => Vec3::new(-14.0, 0.0, 16.0),
        MapSettlementKind::Harbor => Vec3::new(18.0, 0.0, 24.0),
        MapSettlementKind::Outpost => Vec3::new(-12.0, 0.0, -18.0),
    };
    let pos = origin + rot * local;
    let ground = layout.floor_y_at(pos.x, pos.z, seed);
    let facing = Transform::from_xyz(pos.x, ground + 1.9, pos.z).with_rotation(
        Quat::from_rotation_y(settlement.facing_yaw + std::f32::consts::PI),
    );
    let body_mat = match settlement.kind {
        MapSettlementKind::City => pal.window_cool.clone(),
        MapSettlementKind::Village => pal.residential_b.clone(),
        MapSettlementKind::Harbor => pal.glass_panel.clone(),
        MapSettlementKind::Outpost => pal.brushed_metal.clone(),
    };
    let role = if matches!(settlement.kind, MapSettlementKind::City) {
        settlement_guardian_role(settlement.kind)
    } else {
        script.role
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(0.85, 3.8))),
            material: MeshMaterial3d(body_mat),
            transform: facing,
            ..default()
        },
        WorldGeometry,
        DiscussionNpc {
            id: settlement.anchor_id,
            display_name: script.npc_name,
            role,
            script_id,
            interact_radius: 18.0,
        },
    ));

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.72))),
            material: MeshMaterial3d(pal.small_window_warm.clone()),
            transform: Transform::from_xyz(pos.x, ground + 4.25, pos.z),
            ..default()
        },
        WorldGeometry,
    ));

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(3.2, 0.08))),
            material: MeshMaterial3d(pal.crystal_aurora.clone()),
            transform: Transform::from_xyz(pos.x, ground + 0.18, pos.z),
            ..default()
        },
        WorldGeometry,
    ));

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.50, 0.92, 1.0),
                intensity: 9_000.0,
                range: 28.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(pos.x, ground + 4.4, pos.z),
            ..default()
        },
        WorldGeometry,
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_settlement_build_terminal(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
    rot: Quat,
    layout: SettlementTerrainLayout,
) {
    let local = settlement_build_terminal_local(settlement.kind);
    let raw = origin + rot * local;
    let ground = layout.floor_y_at(raw.x, raw.z, seed);
    let base = Vec3::new(raw.x, ground, raw.z);
    let yaw = Quat::from_rotation_y(settlement.facing_yaw + std::f32::consts::PI);

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(3.4, 0.34))),
            material: MeshMaterial3d(pal.sky_platform.clone()),
            transform: Transform::from_xyz(base.x, base.y + 0.18, base.z),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(0.17, 3.4),
    ));

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(1.25, 2.4))),
            material: MeshMaterial3d(pal.brushed_metal.clone()),
            transform: Transform::from_xyz(base.x, base.y + 1.4, base.z),
            ..default()
        },
        WorldGeometry,
        SettlementBuildTerminal {
            settlement_id: settlement.anchor_id,
            settlement_name: settlement.name,
            reward_id: settlement.reward_id,
            kind: settlement.kind,
            origin,
            facing_yaw: settlement.facing_yaw,
            interact_radius: 18.0,
        },
    ));

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(3.2, 1.45, 0.22))),
            material: MeshMaterial3d(pal.guide_glow.clone()),
            transform: Transform::from_translation(base + Vec3::Y * 2.35 + yaw * Vec3::Z * 1.08)
                .with_rotation(yaw),
            ..default()
        },
        WorldGeometry,
    ));

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.66))),
            material: MeshMaterial3d(pal.crystal_aurora.clone()),
            transform: Transform::from_xyz(base.x, base.y + 3.05, base.z),
            ..default()
        },
        WorldGeometry,
    ));

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.36, 0.96, 1.0),
                intensity: 18_000.0,
                range: 42.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(base.x, base.y + 4.1, base.z),
            ..default()
        },
        WorldGeometry,
    ));
}

fn settlement_build_terminal_local(kind: MapSettlementKind) -> Vec3 {
    match kind {
        MapSettlementKind::City => Vec3::new(28.0, 0.0, -30.0),
        MapSettlementKind::Village => Vec3::new(22.0, 0.0, -22.0),
        MapSettlementKind::Harbor => Vec3::new(-26.0, 0.0, 22.0),
        MapSettlementKind::Outpost => Vec3::new(18.0, 0.0, 21.0),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_settlement_economy_modules(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
    rot: Quat,
    layout: SettlementTerrainLayout,
    economy: &SettlementEconomy,
) {
    for record in economy.builds_for(settlement.anchor_id) {
        if record.tier == 0 {
            continue;
        }
        let base = settlement_build_module_base(origin, rot, layout, seed, record.kind);
        spawn_settlement_build_module(commands, meshes, pal, base, rot, record.kind, record.tier);
    }
}

fn settlement_build_module_base(
    origin: Vec3,
    rot: Quat,
    layout: SettlementTerrainLayout,
    seed: u64,
    kind: SettlementBuildKind,
) -> Vec3 {
    let raw = origin + rot * settlement_build_module_local(kind);
    let floor_y = layout.floor_y_at(raw.x, raw.z, seed);
    Vec3::new(raw.x, floor_y + 0.08, raw.z)
}

fn settlement_build_module_local(kind: SettlementBuildKind) -> Vec3 {
    match kind {
        SettlementBuildKind::Farm => Vec3::new(-42.0, 0.0, 40.0),
        SettlementBuildKind::Factory => Vec3::new(54.0, 0.0, -4.0),
        SettlementBuildKind::Spaceport => Vec3::new(0.0, 0.0, 62.0),
        SettlementBuildKind::PowerPlant => Vec3::new(44.0, 0.0, 36.0),
        SettlementBuildKind::ResearchLab => Vec3::new(-46.0, 0.0, -34.0),
        SettlementBuildKind::DefenseOutpost => Vec3::new(0.0, 0.0, -56.0),
        SettlementBuildKind::BridgeHub => Vec3::new(62.0, 0.0, -50.0),
    }
}

fn spawn_settlement_build_module(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    base: Vec3,
    rot: Quat,
    kind: SettlementBuildKind,
    tier: u32,
) {
    let tier = tier.clamp(1, 3);
    let pad_size = match kind {
        SettlementBuildKind::Farm => Vec3::new(24.0, 0.32, 18.0),
        SettlementBuildKind::Factory => Vec3::new(18.0, 0.38, 16.0),
        SettlementBuildKind::Spaceport => Vec3::new(28.0, 0.36, 28.0),
        SettlementBuildKind::PowerPlant => Vec3::new(17.0, 0.38, 17.0),
        SettlementBuildKind::ResearchLab => Vec3::new(18.0, 0.34, 18.0),
        SettlementBuildKind::DefenseOutpost => Vec3::new(16.0, 0.38, 16.0),
        SettlementBuildKind::BridgeHub => Vec3::new(26.0, 0.34, 14.0),
    };
    let pad_mat = match kind {
        SettlementBuildKind::Farm => pal.moss.clone(),
        SettlementBuildKind::BridgeHub => pal.mountain_path.clone(),
        SettlementBuildKind::Spaceport => pal.sky_platform.clone(),
        _ => pal.brushed_metal.clone(),
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(pad_size.x, pad_size.y, pad_size.z))),
            material: MeshMaterial3d(pad_mat),
            transform: Transform::from_translation(base + Vec3::Y * (pad_size.y * 0.5))
                .with_rotation(rot),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            pad_size.x * 0.5,
            pad_size.y * 0.5,
            pad_size.z * 0.5,
        ),
    ));

    match kind {
        SettlementBuildKind::Farm => {
            for row in -2..=2 {
                spawn_settlement_module_cube(
                    commands,
                    meshes,
                    pal.foliage_a.clone(),
                    base,
                    rot,
                    Vec3::new(1.0, 0.52, row as f32 * 3.0),
                    Vec3::new(18.0, 0.28, 1.15),
                );
            }
            spawn_settlement_module_cylinder(
                commands,
                meshes,
                pal.stone_brick.clone(),
                base,
                Vec3::new(-8.5, 2.15, 5.8),
                2.1,
                4.1,
            );
        }
        SettlementBuildKind::Factory => {
            spawn_settlement_module_cube(
                commands,
                meshes,
                pal.industrial_metal.clone(),
                base,
                rot,
                Vec3::new(0.0, 2.55, 0.0),
                Vec3::new(11.5, 4.8, 8.8),
            );
            spawn_settlement_module_cylinder(
                commands,
                meshes,
                pal.industrial_rust.clone(),
                base,
                Vec3::new(5.3, 5.0, -2.8),
                1.1,
                9.2,
            );
        }
        SettlementBuildKind::Spaceport => {
            spawn_settlement_module_cylinder(
                commands,
                meshes,
                pal.sky_platform.clone(),
                base,
                Vec3::new(0.0, 0.72, 0.0),
                11.0,
                0.8,
            );
            spawn_settlement_module_cube(
                commands,
                meshes,
                pal.guide_glow.clone(),
                base,
                rot,
                Vec3::new(0.0, 0.98, -8.4),
                Vec3::new(4.2, 0.32, 9.5),
            );
            spawn_settlement_module_cube(
                commands,
                meshes,
                pal.brushed_metal.clone(),
                base,
                rot,
                Vec3::new(-8.4, 3.1, 5.2),
                Vec3::new(3.0, 5.5, 3.0),
            );
        }
        SettlementBuildKind::PowerPlant => {
            spawn_settlement_module_cylinder(
                commands,
                meshes,
                pal.industrial_metal.clone(),
                base,
                Vec3::new(0.0, 3.1, 0.0),
                4.4,
                6.0,
            );
            spawn_settlement_module_sphere(
                commands,
                meshes,
                pal.crystal_aurora.clone(),
                base,
                Vec3::new(0.0, 6.7, 0.0),
                2.4,
            );
        }
        SettlementBuildKind::ResearchLab => {
            spawn_settlement_module_cylinder(
                commands,
                meshes,
                pal.glass_panel.clone(),
                base,
                Vec3::new(0.0, 1.7, 0.0),
                5.6,
                3.0,
            );
            spawn_settlement_module_sphere(
                commands,
                meshes,
                pal.small_window_cool.clone(),
                base,
                Vec3::new(0.0, 4.2, 0.0),
                4.2,
            );
        }
        SettlementBuildKind::DefenseOutpost => {
            spawn_settlement_module_cylinder(
                commands,
                meshes,
                pal.dragon_stone.clone(),
                base,
                Vec3::new(0.0, 4.0, 0.0),
                3.5,
                7.6,
            );
            spawn_settlement_module_cube(
                commands,
                meshes,
                pal.brushed_metal.clone(),
                base,
                rot,
                Vec3::new(0.0, 8.0, -2.8),
                Vec3::new(9.0, 1.25, 2.0),
            );
            spawn_settlement_module_sphere(
                commands,
                meshes,
                pal.guide_glow.clone(),
                base,
                Vec3::new(0.0, 8.4, 0.0),
                1.25,
            );
        }
        SettlementBuildKind::BridgeHub => {
            spawn_settlement_module_cube(
                commands,
                meshes,
                pal.mountain_path.clone(),
                base,
                rot,
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(28.0, 1.1, 4.8),
            );
            for side in [-1.0_f32, 1.0] {
                spawn_settlement_module_cube(
                    commands,
                    meshes,
                    pal.stone_block.clone(),
                    base,
                    rot,
                    Vec3::new(side * 11.0, 3.0, -4.4),
                    Vec3::new(2.7, 5.3, 2.7),
                );
                spawn_settlement_module_sphere(
                    commands,
                    meshes,
                    pal.guide_glow.clone(),
                    base,
                    rot * Vec3::new(side * 11.0, 6.0, -4.4),
                    0.9,
                );
            }
        }
    }

    for level in 0..tier {
        let offset = Vec3::new(
            -pad_size.x * 0.36 + level as f32 * 2.2,
            1.18,
            pad_size.z * 0.38,
        );
        spawn_settlement_module_sphere(
            commands,
            meshes,
            pal.guide_glow.clone(),
            base,
            rot * offset,
            0.62,
        );
    }

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.32, 0.88, 1.0),
                intensity: 8_000.0 + tier as f32 * 3_000.0,
                range: 28.0 + tier as f32 * 8.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(base + Vec3::Y * 6.5),
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_settlement_module_cube(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    base: Vec3,
    rot: Quat,
    local: Vec3,
    size: Vec3,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(base + rot * local).with_rotation(rot),
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_settlement_module_cylinder(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    base: Vec3,
    local: Vec3,
    radius: f32,
    height: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(radius, height))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(base + local),
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_settlement_module_sphere(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    base: Vec3,
    local: Vec3,
    radius: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(radius))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(base + local),
            ..default()
        },
        WorldGeometry,
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_settlement_mountain_inset(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
    rot: Quat,
    layout: SettlementTerrainLayout,
) {
    if !layout.needs_mountain_inset(settlement) {
        return;
    }

    let search_radius = match settlement.kind {
        MapSettlementKind::City => layout.platform_radius + 170.0,
        MapSettlementKind::Village => layout.plaza_radius + 78.0,
        MapSettlementKind::Harbor => layout.plaza_radius + 70.0,
        MapSettlementKind::Outpost => layout.plaza_radius + 84.0,
    };
    let mut best = Vec3::new(settlement.x, layout.center_y, settlement.z);
    let mut best_score = f32::MIN;
    for i in 0..12 {
        let angle = i as f32 * std::f32::consts::TAU / 12.0;
        let local = Vec3::new(
            angle.cos() * search_radius,
            0.0,
            angle.sin() * search_radius,
        );
        let world = Vec3::new(settlement.x, 0.0, settlement.z) + rot * local;
        let ground = terrain_surface_y(world.x, world.z, seed);
        let ridge_score = ground + (ground - layout.center_y).abs() * 0.45;
        let facing_bonus = if i == 0 || i == 11 { 28.0 } else { 0.0 };
        let score = ridge_score + facing_bonus + seeded(seed, i as u64 + 701) * 8.0;
        if score > best_score {
            best_score = score;
            best = Vec3::new(world.x, ground, world.z);
        }
    }

    let to_center = Vec2::new(settlement.x - best.x, settlement.z - best.z);
    let facing = if to_center.length_squared() > 0.001 {
        to_center.normalize()
    } else {
        Vec2::Y
    };
    let gate_yaw = facing.x.atan2(facing.y);
    let gate_rot = Quat::from_rotation_y(gate_yaw);
    let gate_base = best + Vec3::Y * 0.2;
    let wall_mat = if layout.is_sky_district() {
        pal.brushed_metal.clone()
    } else {
        pal.rock_dark.clone()
    };

    for side in [-1.0_f32, 1.0] {
        spawn_mountain_inset_piece(
            commands,
            meshes,
            wall_mat.clone(),
            gate_base,
            gate_rot,
            Vec3::new(side * 17.0, 12.0, 0.0),
            Vec3::new(9.0, 24.0, 8.0),
            true,
        );
        spawn_mountain_inset_piece(
            commands,
            meshes,
            pal.stone_block.clone(),
            gate_base,
            gate_rot,
            Vec3::new(side * 25.0, 7.0, 2.0),
            Vec3::new(8.0, 14.0, 10.0),
            true,
        );
    }

    spawn_mountain_inset_piece(
        commands,
        meshes,
        wall_mat,
        gate_base,
        gate_rot,
        Vec3::new(0.0, 25.5, 0.0),
        Vec3::new(50.0, 8.0, 9.0),
        true,
    );
    spawn_mountain_inset_piece(
        commands,
        meshes,
        pal.small_window_dark.clone(),
        gate_base,
        gate_rot,
        Vec3::new(0.0, 9.5, -3.9),
        Vec3::new(21.0, 17.0, 0.45),
        false,
    );
    spawn_mountain_inset_piece(
        commands,
        meshes,
        pal.mountain_path.clone(),
        gate_base,
        gate_rot,
        Vec3::new(0.0, 0.28, 12.0),
        Vec3::new(36.0, 0.56, 30.0),
        true,
    );

    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(2.3))),
                material: MeshMaterial3d(pal.guide_glow.clone()),
                transform: Transform::from_translation(
                    gate_base + gate_rot * Vec3::new(side * 10.0, 13.0, -4.6),
                ),
                ..default()
            },
            WorldGeometry,
        ));
    }

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.30, 0.86, 1.0),
                intensity: 20_000.0,
                range: 70.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(gate_base + Vec3::Y * 15.0),
            ..default()
        },
        WorldGeometry,
    ));

    if layout.is_sky_district() {
        let lift_height = (origin.y - gate_base.y).max(16.0);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(3.2, lift_height))),
                material: MeshMaterial3d(pal.glass_panel.clone()),
                transform: Transform::from_xyz(
                    gate_base.x,
                    gate_base.y + lift_height * 0.5,
                    gate_base.z,
                ),
                ..default()
            },
            WorldGeometry,
        ));
        spawn_settlement_ramp_span(
            commands,
            meshes,
            pal,
            Vec3::new(gate_base.x, origin.y + 0.35, gate_base.z),
            origin + gate_rot * Vec3::new(0.0, 0.35, -layout.platform_radius * 0.55),
            12.0,
            0.7,
            seed + settlement.anchor_id.len() as u64 * 97,
        );
    } else {
        let approach_start = origin + gate_rot * Vec3::new(0.0, 0.35, -layout.plaza_radius * 0.6);
        let approach_end = gate_base + gate_rot * Vec3::new(0.0, 0.55, 26.0);
        spawn_settlement_ramp_span(
            commands,
            meshes,
            pal,
            approach_start,
            approach_end,
            11.0,
            0.55,
            seed + settlement.anchor_id.len() as u64 * 73,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_mountain_inset_piece(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    base: Vec3,
    rotation: Quat,
    local: Vec3,
    size: Vec3,
    collider: bool,
) {
    let entity = commands
        .spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(material),
                transform: Transform::from_translation(base + rotation * local)
                    .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
        ))
        .id();

    if collider {
        commands.entity(entity).insert((
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                size.x * 0.5,
                size.y * 0.5,
                size.z * 0.5,
            ),
        ));
    }
}

fn spawn_city_guardian_ships(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
) {
    let body_mesh = meshes.add(Cuboid::new(19.0, 3.4, 6.0));
    let wing_mesh = meshes.add(Cuboid::new(9.5, 0.55, 15.5));
    let fin_mesh = meshes.add(Cuboid::new(3.2, 4.4, 0.6));
    let engine_mesh = meshes.add(Cylinder::new(1.15, 3.8));
    let glow_mesh = meshes.add(Sphere::new(1.15));

    for i in 0..3 {
        let radius = 135.0 + i as f32 * 36.0 + seeded(seed, i as u64 + 81) * 20.0;
        let altitude = 92.0 + i as f32 * 16.0 + seeded(seed, i as u64 + 91) * 18.0;
        let phase = settlement.facing_yaw + i as f32 * std::f32::consts::TAU / 3.0;
        let angle = phase;
        let start = Vec3::new(
            origin.x + angle.cos() * radius,
            origin.y + altitude,
            origin.z + angle.sin() * radius,
        );

        commands
            .spawn((
                PbrBundle {
                    mesh: Mesh3d(body_mesh.clone()),
                    material: MeshMaterial3d(pal.brushed_metal.clone()),
                    transform: Transform::from_translation(start)
                        .with_rotation(Quat::from_rotation_y(-angle + std::f32::consts::FRAC_PI_2)),
                    ..default()
                },
                WorldGeometry,
                FreePeopleGuardianShip {
                    center: origin,
                    radius,
                    altitude,
                    angular_speed: 0.10 + i as f32 * 0.025,
                    phase,
                    bob: 5.0 + i as f32,
                },
            ))
            .with_children(|ship| {
                ship.spawn(PbrBundle {
                    mesh: Mesh3d(wing_mesh.clone()),
                    material: MeshMaterial3d(pal.glass_panel.clone()),
                    transform: Transform::from_xyz(0.0, -0.25, 0.0),
                    ..default()
                });
                ship.spawn(PbrBundle {
                    mesh: Mesh3d(fin_mesh.clone()),
                    material: MeshMaterial3d(pal.window_cool.clone()),
                    transform: Transform::from_xyz(-6.4, 3.0, 0.0),
                    ..default()
                });
                for z in [-2.4_f32, 2.4] {
                    ship.spawn(PbrBundle {
                        mesh: Mesh3d(engine_mesh.clone()),
                        material: MeshMaterial3d(pal.downtown_b.clone()),
                        transform: Transform::from_xyz(-9.8, -0.2, z)
                            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                        ..default()
                    });
                    ship.spawn(PbrBundle {
                        mesh: Mesh3d(glow_mesh.clone()),
                        material: MeshMaterial3d(pal.crystal_aurora.clone()),
                        transform: Transform::from_xyz(-12.0, -0.2, z),
                        ..default()
                    });
                }
                ship.spawn(PointLightBundle {
                    point_light: PointLight {
                        color: Color::srgb(0.40, 0.88, 1.0),
                        intensity: 18_000.0,
                        range: 60.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    transform: Transform::from_xyz(0.0, 1.4, 0.0),
                    ..default()
                });
            });
    }
}

#[derive(Debug, Clone, Copy)]
struct CitySpyDroneSpec {
    id: &'static str,
    label: &'static str,
    reward_id: &'static str,
    radius: f32,
    altitude: f32,
    angular_speed: f32,
    phase: f32,
    credits: u32,
    experience: u32,
    armor: u32,
}

const CLOUDRAIL_SPY_DRONES: &[CitySpyDroneSpec] = &[
    CitySpyDroneSpec {
        id: "cloudrail_spy_drone_north",
        label: "Cloudrail Spy Data North",
        reward_id: "spy_data_cloudrail_north",
        radius: 76.0,
        altitude: 34.0,
        angular_speed: 0.34,
        phase: 0.4,
        credits: 75,
        experience: 55,
        armor: 5,
    },
    CitySpyDroneSpec {
        id: "cloudrail_spy_drone_market",
        label: "Cloudrail Spy Data Market",
        reward_id: "spy_data_cloudrail_market",
        radius: 104.0,
        altitude: 42.0,
        angular_speed: -0.27,
        phase: 2.7,
        credits: 90,
        experience: 65,
        armor: 6,
    },
];

const SWITCHWORK_SPY_DRONES: &[CitySpyDroneSpec] = &[
    CitySpyDroneSpec {
        id: "switchwork_spy_drone_hangar",
        label: "Switchwork Spy Data Hangar",
        reward_id: "spy_data_switchwork_hangar",
        radius: 82.0,
        altitude: 36.0,
        angular_speed: 0.31,
        phase: 1.1,
        credits: 80,
        experience: 60,
        armor: 5,
    },
    CitySpyDroneSpec {
        id: "switchwork_spy_drone_relay",
        label: "Switchwork Spy Data Relay",
        reward_id: "spy_data_switchwork_relay",
        radius: 112.0,
        altitude: 48.0,
        angular_speed: -0.24,
        phase: 3.6,
        credits: 95,
        experience: 70,
        armor: 7,
    },
];

fn city_spy_drone_specs(anchor_id: &str) -> &'static [CitySpyDroneSpec] {
    match anchor_id {
        "settlement_cloudrail_city" => CLOUDRAIL_SPY_DRONES,
        "settlement_switchwork_borough" => SWITCHWORK_SPY_DRONES,
        _ => &[],
    }
}

fn spawn_city_spy_drones(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    settlement: MapSettlement,
    origin: Vec3,
) {
    let specs = city_spy_drone_specs(settlement.anchor_id);
    if specs.is_empty() {
        return;
    }

    let body_mesh = meshes.add(Cuboid::new(3.8, 1.35, 2.4));
    let wing_mesh = meshes.add(Cuboid::new(2.6, 0.24, 6.4));
    let lens_mesh = meshes.add(Sphere::new(0.46));
    let antenna_mesh = meshes.add(Cylinder::new(0.08, 2.1));

    for (i, spec) in specs.iter().enumerate() {
        let phase = settlement.facing_yaw + spec.phase + seeded(seed, i as u64 + 230) * 0.35;
        let start = Vec3::new(
            origin.x + phase.cos() * spec.radius,
            origin.y + spec.altitude,
            origin.z + phase.sin() * spec.radius,
        );
        let enemy = Enemy::new(EnemyType::SpyDrone, start, 1.0);
        let max_hp = enemy.scaled_health();

        commands
            .spawn((
                PbrBundle {
                    mesh: Mesh3d(body_mesh.clone()),
                    material: MeshMaterial3d(pal.dragon_window.clone()),
                    transform: Transform::from_translation(start)
                        .with_rotation(Quat::from_rotation_y(-phase + std::f32::consts::FRAC_PI_2)),
                    ..default()
                },
                WorldGeometry,
                enemy,
                EnemyStateMachine::default(),
                Health::new(max_hp),
                Damageable::default(),
                Faction::DimensionalAlien,
                crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
                // Intentionally generous around the wings and bobbing flight
                // path so controller aim and fast projectiles register clearly.
                crate::engine::physics::prelude::Collider::cuboid(2.8, 1.35, 2.25),
                crate::engine::physics::prelude::CollisionProfile::EnemyHurtbox,
                avian3d::prelude::Sensor,
                CitySpyDrone {
                    id: spec.id,
                    label: spec.label,
                    reward_id: spec.reward_id,
                    home: origin,
                    radius: spec.radius,
                    altitude: spec.altitude,
                    angular_speed: spec.angular_speed,
                    phase,
                    bob: 2.6 + seeded(seed, i as u64 + 240) * 1.2,
                    credits: spec.credits,
                    experience: spec.experience,
                    armor: spec.armor,
                    data_spawned: false,
                    pressure_applied: false,
                },
            ))
            .with_children(|spy| {
                spy.spawn(PbrBundle {
                    mesh: Mesh3d(wing_mesh.clone()),
                    material: MeshMaterial3d(pal.industrial_rust.clone()),
                    transform: Transform::from_xyz(0.0, -0.08, 0.0),
                    ..default()
                });
                spy.spawn(PbrBundle {
                    mesh: Mesh3d(lens_mesh.clone()),
                    material: MeshMaterial3d(pal.crystal_dragon.clone()),
                    transform: Transform::from_xyz(2.2, 0.08, 0.0),
                    ..default()
                });
                for z in [-0.72_f32, 0.72] {
                    spy.spawn(PbrBundle {
                        mesh: Mesh3d(antenna_mesh.clone()),
                        material: MeshMaterial3d(pal.brushed_metal.clone()),
                        transform: Transform::from_xyz(-1.25, 0.92, z)
                            .with_rotation(Quat::from_rotation_x(0.38 * z.signum())),
                        ..default()
                    });
                }
                spy.spawn(PointLightBundle {
                    point_light: PointLight {
                        color: Color::srgb(1.0, 0.18, 0.08),
                        intensity: 7_500.0,
                        range: 22.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    transform: Transform::from_xyz(2.4, 0.2, 0.0),
                    ..default()
                });
            });
    }
}

fn spawn_dragon_lair_silhouettes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    for spec in dragon_dungeon_specs() {
        let rot = Quat::from_rotation_y(spec.yaw);
        let origin_y = terrain_surface_y(spec.x, spec.z, seed);
        let origin = Vec3::new(spec.x, origin_y, spec.z);
        let (wall_mat, _, accent_mat, glow_mat) = dragon_dungeon_materials(pal, spec.theme);

        for i in 0..7u64 {
            let angle = i as f32 * std::f32::consts::TAU / 7.0
                + seeded(seed, i + spec.chapter as u64) * 0.4;
            let radius = 95.0 + seeded(seed, i * 3 + spec.chapter as u64) * 115.0;
            let x = spec.x + angle.cos() * radius;
            let z = spec.z + angle.sin() * radius;
            let y = terrain_surface_y(x, z, seed);
            let h = 34.0 + seeded(seed, i * 3 + 1) * 72.0;
            let r = 7.0 + seeded(seed, i * 3 + 2) * 10.0;
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cone {
                        radius: r,
                        height: h,
                    })),
                    material: MeshMaterial3d(wall_mat.clone()),
                    transform: Transform::from_xyz(x, y + h * 0.5, z),
                    ..default()
                },
                WorldGeometry,
            ));
        }

        let entry = origin + rot * Vec3::new(0.0, 0.0, -98.0);
        for side in [-1.0_f32, 1.0] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(9.0, 32.0, 9.0))),
                    material: MeshMaterial3d(wall_mat.clone()),
                    transform: Transform::from_translation(
                        entry + rot * Vec3::new(side * 24.0, 16.0, 0.0),
                    )
                    .with_rotation(rot),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(58.0, 9.0, 10.0))),
                material: MeshMaterial3d(wall_mat.clone()),
                transform: Transform::from_translation(entry + rot * Vec3::new(0.0, 32.0, 0.0))
                    .with_rotation(rot),
                ..default()
            },
            WorldGeometry,
        ));

        for side in [-1.0_f32, 1.0] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Sphere::new(4.6))),
                    material: MeshMaterial3d(glow_mat.clone()),
                    transform: Transform::from_translation(
                        entry + rot * Vec3::new(side * 16.0, 19.0, -7.0),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(2.2, 10.0))),
                material: MeshMaterial3d(accent_mat),
                transform: Transform::from_translation(entry + rot * Vec3::new(0.0, 7.0, 8.0)),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

// ── Downtown ──────────────────────────────────────────────────────────────────
fn spawn_downtown(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    let facade_meshes = DowntownFacadeMeshCache::new(meshes);
    let glass = [
        &pal.downtown_a,
        &pal.downtown_b,
        &pal.downtown_c,
        &pal.downtown_facade,
    ];

    for i in 0..60u64 {
        let x = seeded(seed, i * 3) * 200.0 - 100.0;
        let z = seeded(seed, i * 3 + 1) * 200.0 - 100.0;
        let h = 30.0 + seeded(seed, i * 3 + 2) * 120.0;
        let w = 12.0 + seeded(seed, i * 5 + 3) * 18.0;
        let d = 12.0 + seeded(seed, i * 5 + 4) * 18.0;
        if starter_clear_zone(x, z, w.max(d) * 0.5) {
            continue;
        }

        let mat = if i % 13 == 0 {
            pal.stone_brick.clone()
        } else if i % 11 == 0 {
            pal.stainless_metal.clone()
        } else if i % 7 == 0 {
            pal.brushed_metal.clone()
        } else if i % 6 == 0 {
            pal.city_metal_panel.clone()
        } else if i % 5 == 0 {
            pal.glass_panel.clone()
        } else {
            glass[(i % 4) as usize].clone()
        };
        let enterable = i.is_multiple_of(4)
            && spawn_enterable_building(
                commands,
                meshes,
                pal,
                mat.clone(),
                Vec3::new(x, h * 0.5, z),
                w,
                h,
                d,
                0.0,
                WorldZone::Downtown,
                seed + i * 101,
            );
        if !enterable {
            spawn_building(
                commands,
                meshes,
                mat,
                Vec3::new(x, h * 0.5, z),
                w,
                h,
                d,
                WorldZone::Downtown,
            );
            if i.is_multiple_of(3) && building_footprint_clears_speed_roads(x, z, w, d) {
                spawn_city_roof_route_and_crown(
                    commands,
                    meshes,
                    pal,
                    Vec3::new(x, 0.0, z),
                    Quat::IDENTITY,
                    w,
                    h,
                    d,
                    (h / 1.25).floor() as usize,
                    seed + i * 101,
                );
            }
        }

        // Facade accent band near base
        if !enterable && i % 3 == 0 {
            spawn_building(
                commands,
                meshes,
                pal.downtown_facade.clone(),
                Vec3::new(x, 1.5, z),
                w + 0.5,
                3.0,
                d + 0.5,
                WorldZone::Downtown,
            );
        }

        // Window facade panels — cover the upper 70 % of each face with
        // emissive glass strips that read as lit office floors at night.
        let win_mat = if i % 2 == 0 {
            pal.window_warm.clone()
        } else {
            pal.window_cool.clone()
        };
        if !enterable {
            spawn_window_facades(commands, &facade_meshes.unit_cube, win_mat, x, z, h, w, d);
            spawn_modern_window_grid(
                commands,
                &facade_meshes.unit_cube,
                pal,
                x,
                z,
                h,
                w,
                d,
                seed + i * 17,
            );
        }
        if !enterable && i % 7 == 0 {
            spawn_metal_cladding_cached(
                commands,
                pal,
                &facade_meshes.unit_cube,
                Vec3::new(x, h * 0.5, z),
                w,
                h,
                d,
                seed + i,
            );
        }
        if !enterable && i % 17 == 0 {
            spawn_metal_cladding_cached(
                commands,
                pal,
                &facade_meshes.unit_cube,
                Vec3::new(x, h * 0.5, z),
                w + 0.7,
                h * 0.72,
                d + 0.7,
                seed + i * 5 + 3,
            );
        }
        if !enterable && i % 13 == 0 {
            spawn_stone_brick_courses_cached(
                commands,
                pal,
                &facade_meshes.unit_cube,
                Vec3::new(x, h * 0.5, z),
                w,
                h,
                d,
                true,
            );
        }

        // Rooftop details on taller buildings
        if h > 50.0 {
            spawn_rooftop_details(commands, &facade_meshes, pal, x, z, h, w, d, seed + i);
        }
    }
}

fn spawn_city_hidden_rooms(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
) {
    let rooms = [
        (
            Vec3::new(46.0, 0.22, 44.0),
            0.38,
            "Aurora Guard Cache",
            DiscoverableKind::HiddenReward {
                reward_id: "hidden_city_aurora_guard_cache",
                credits: 125,
                experience: 70,
                armor: 15,
                power_up: Some("aurora_guard_core"),
                special_ability: None,
            },
        ),
        (
            Vec3::new(-30.0, 0.22, 42.0),
            -0.55,
            "Transit Gold Vault",
            DiscoverableKind::HiddenReward {
                reward_id: "hidden_city_transit_gold_vault",
                credits: 220,
                experience: 45,
                armor: 5,
                power_up: None,
                special_ability: Some("homing_star_overdrive"),
            },
        ),
        (
            Vec3::new(14.0, 0.22, -38.0),
            1.05,
            "Moon Bubble Workshop",
            DiscoverableKind::HiddenReward {
                reward_id: "hidden_city_moon_bubble_workshop",
                credits: 90,
                experience: 95,
                armor: 8,
                power_up: Some("moon_bubble_capacitor"),
                special_ability: Some("moon_bubble_overcharge"),
            },
        ),
    ];

    for (center, yaw, label, reward) in rooms {
        spawn_hidden_reward_room(commands, meshes, pal, center, yaw);
        spawn_discoverable_beacon(commands, meshes, materials, reward, label, center);
    }
}

fn spawn_hidden_reward_room(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    yaw: f32,
) {
    let floor = Vec3::new(17.0, 0.34, 12.0);
    spawn_hidden_room_piece(
        commands,
        meshes,
        pal.stone_block.clone(),
        center + Vec3::Y * 0.02,
        yaw,
        Vec3::ZERO,
        floor,
        true,
        true,
    );

    for (local, size, material) in [
        (
            Vec3::new(0.0, 1.55, 6.2),
            Vec3::new(17.0, 3.0, 0.45),
            pal.stone_brick.clone(),
        ),
        (
            Vec3::new(-8.7, 1.55, 0.0),
            Vec3::new(0.45, 3.0, 12.0),
            pal.stone_brick.clone(),
        ),
        (
            Vec3::new(8.7, 1.55, 0.0),
            Vec3::new(0.45, 3.0, 12.0),
            pal.stone_brick.clone(),
        ),
        (
            Vec3::new(-5.9, 1.55, -6.2),
            Vec3::new(5.0, 3.0, 0.45),
            pal.downtown_facade.clone(),
        ),
        (
            Vec3::new(5.9, 1.55, -6.2),
            Vec3::new(5.0, 3.0, 0.45),
            pal.downtown_facade.clone(),
        ),
    ] {
        spawn_hidden_room_piece(
            commands, meshes, material, center, yaw, local, size, true, false,
        );
    }

    let rot = Quat::from_rotation_y(yaw);
    for (local, size, material) in [
        (
            Vec3::new(-3.0, 0.55, 1.8),
            Vec3::new(1.2, 0.8, 1.2),
            pal.window_warm.clone(),
        ),
        (
            Vec3::new(3.0, 0.55, 1.8),
            Vec3::new(1.2, 0.8, 1.2),
            pal.window_warm.clone(),
        ),
        (
            Vec3::new(0.0, 0.85, -1.8),
            Vec3::new(1.7, 1.1, 1.7),
            pal.window_cool.clone(),
        ),
    ] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(material),
                transform: Transform::from_translation(center + rot * local).with_rotation(rot),
                ..default()
            },
            WorldGeometry,
        ));
    }

    spawn_hidden_room_puzzle_dressing(commands, meshes, pal, center, yaw);

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(1.0, 0.76, 0.36),
                intensity: 18_000.0,
                range: 24.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(center + Vec3::Y * 3.2),
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_hidden_room_puzzle_dressing(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    yaw: f32,
) {
    let rotation = Quat::from_rotation_y(yaw);
    for (i, local) in [
        Vec3::new(-4.8, 0.42, -2.6),
        Vec3::new(0.0, 0.42, -3.6),
        Vec3::new(4.8, 0.42, -2.6),
    ]
    .into_iter()
    .enumerate()
    {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(1.15, 0.12))),
                material: MeshMaterial3d(if i == 1 {
                    pal.crystal_aurora.clone()
                } else {
                    pal.window_cool.clone()
                }),
                transform: Transform::from_translation(center + rotation * local)
                    .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
        ));
    }

    for (local, size) in [
        (Vec3::new(-6.2, 0.7, 3.4), Vec3::new(1.1, 1.0, 2.2)),
        (Vec3::new(6.2, 0.7, 3.4), Vec3::new(1.1, 1.0, 2.2)),
        (Vec3::new(0.0, 1.15, 4.9), Vec3::new(4.2, 1.2, 0.5)),
    ] {
        spawn_hidden_room_piece(
            commands,
            meshes,
            pal.downtown_facade.clone(),
            center,
            yaw,
            local,
            size,
            true,
            false,
        );
    }

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.45, 0.9, 1.0),
                intensity: 6_500.0,
                range: 12.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(center + rotation * Vec3::new(0.0, 1.8, -3.2)),
            ..default()
        },
        WorldGeometry,
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_hidden_room_piece(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    center: Vec3,
    yaw: f32,
    local: Vec3,
    size: Vec3,
    collider: bool,
    walkable: bool,
) {
    let rotation = Quat::from_rotation_y(yaw);
    let entity = commands
        .spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(material),
                transform: Transform::from_translation(center + rotation * local)
                    .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
        ))
        .id();

    if collider {
        commands.entity(entity).insert((
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                size.x * 0.5,
                size.y * 0.5,
                size.z * 0.5,
            ),
        ));
    }
    if walkable {
        commands.entity(entity).insert(WalkableSurface);
    }
}

fn spawn_traversal_courses(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
) {
    spawn_sling_tower_course(
        commands,
        meshes,
        materials,
        pal,
        Vec3::new(132.0, 0.0, 72.0),
    );
    spawn_ramp_brick_tower_course(
        commands,
        meshes,
        materials,
        pal,
        Vec3::new(-132.0, 0.0, 86.0),
    );
}

fn spawn_sling_tower_course(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    base: Vec3,
) {
    spawn_course_block(
        commands,
        meshes,
        pal.sky_platform.clone(),
        base + Vec3::new(0.0, 0.22, 0.0),
        Vec3::new(28.0, 0.44, 26.0),
        Quat::IDENTITY,
        true,
        true,
    );
    spawn_sling_pad(
        commands,
        meshes,
        pal.crystal_aurora.clone(),
        base + Vec3::new(-10.5, 0.38, -11.5),
        Vec3::new(1.35, 1.22, 1.25),
        4.8,
    );
    spawn_course_marker_pair(commands, meshes, pal, base + Vec3::new(-10.5, 0.9, -17.0));

    spawn_course_block(
        commands,
        meshes,
        pal.street_paint.clone(),
        base + Vec3::new(-0.4, 2.35, -10.0),
        Vec3::new(7.0, 0.55, 18.0),
        Quat::from_rotation_x(-0.32),
        true,
        true,
    );
    spawn_course_block(
        commands,
        meshes,
        pal.brushed_metal.clone(),
        base + Vec3::new(0.0, 0.18, 31.0),
        Vec3::new(34.0, 0.36, 8.0),
        Quat::IDENTITY,
        true,
        true,
    );
    spawn_sling_pad(
        commands,
        meshes,
        pal.window_warm.clone(),
        base + Vec3::new(0.0, 0.38, 30.5),
        Vec3::new(0.0, 1.05, -1.48),
        3.2,
    );

    for x in [-4.1, 4.1] {
        spawn_course_block(
            commands,
            meshes,
            pal.stone_brick.clone(),
            base + Vec3::new(x, 5.4, -0.8),
            Vec3::new(0.72, 10.6, 7.6),
            Quat::IDENTITY,
            true,
            false,
        );
    }

    for i in 0..5 {
        let i_f = i as f32;
        let start = base + Vec3::new(-8.0 + i_f * 4.0, 4.0 + i_f * 1.15, 5.0 + i_f * 1.7);
        let end = start + Vec3::new(0.0, 0.0, if i % 2 == 0 { 7.0 } else { -7.0 });
        spawn_moving_brick(
            commands,
            meshes,
            pal.window_cool.clone(),
            Vec3::new(3.4, 0.42, 3.4),
            start,
            end,
            3.2 + i_f * 0.28,
            0.2 + i_f * 0.6,
        );
    }

    spawn_rotating_elevator(
        commands,
        meshes,
        pal.window_warm.clone(),
        base + Vec3::new(0.0, 9.4, 13.0),
        7.5,
        Vec3::new(4.6, 0.48, 4.6),
        0.75,
        1.65,
        0.0,
    );
    spawn_course_block(
        commands,
        meshes,
        pal.downtown_facade.clone(),
        base + Vec3::new(0.0, 13.8, 17.0),
        Vec3::new(15.0, 0.6, 15.0),
        Quat::IDENTITY,
        true,
        true,
    );
    spawn_course_marker_pair(commands, meshes, pal, base + Vec3::new(0.0, 14.2, 24.8));

    spawn_discoverable_beacon(
        commands,
        meshes,
        materials,
        DiscoverableKind::HiddenReward {
            reward_id: "hidden_course_sling_tower_cache",
            credits: 180,
            experience: 120,
            armor: 14,
            power_up: Some("sling_tower_star_dash"),
            special_ability: Some("tri_star_splitter"),
        },
        "Sling Tower Cache",
        base + Vec3::new(0.0, 14.1, 17.0),
    );
}

fn spawn_ramp_brick_tower_course(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pal: &Palette,
    base: Vec3,
) {
    spawn_course_block(
        commands,
        meshes,
        pal.stone_block.clone(),
        base + Vec3::new(0.0, 0.24, 0.0),
        Vec3::new(30.0, 0.48, 24.0),
        Quat::IDENTITY,
        true,
        true,
    );
    spawn_sling_pad(
        commands,
        meshes,
        pal.crystal_dragon.clone(),
        base + Vec3::new(10.0, 0.4, -9.5),
        Vec3::new(-1.15, 1.08, 1.42),
        4.4,
    );
    spawn_course_marker_pair(commands, meshes, pal, base + Vec3::new(10.0, 0.9, -15.0));
    spawn_course_block(
        commands,
        meshes,
        pal.brushed_metal.clone(),
        base + Vec3::new(-18.0, 0.18, 12.0),
        Vec3::new(8.0, 0.36, 34.0),
        Quat::IDENTITY,
        true,
        true,
    );
    spawn_sling_pad(
        commands,
        meshes,
        pal.window_cool.clone(),
        base + Vec3::new(-18.0, 0.38, 23.0),
        Vec3::new(1.28, 1.0, -0.82),
        3.0,
    );

    for (i, z) in [-8.0_f32, -1.0, 6.0, 13.0].into_iter().enumerate() {
        let x = if i % 2 == 0 { -5.0 } else { 5.0 };
        let rot_z = if i % 2 == 0 { 0.18 } else { -0.18 };
        spawn_course_block(
            commands,
            meshes,
            if i % 2 == 0 {
                pal.highway.clone()
            } else {
                pal.brushed_metal.clone()
            },
            base + Vec3::new(x, 1.4 + i as f32 * 1.25, z),
            Vec3::new(9.0, 0.48, 9.5),
            Quat::from_rotation_z(rot_z) * Quat::from_rotation_x(-0.28),
            true,
            true,
        );
    }

    for (x, z) in [(-7.6, 7.8), (7.6, 7.8), (-7.6, 15.8), (7.6, 15.8)] {
        spawn_course_block(
            commands,
            meshes,
            pal.industrial_metal.clone(),
            base + Vec3::new(x, 5.5, z),
            Vec3::new(0.75, 10.5, 0.75),
            Quat::IDENTITY,
            true,
            false,
        );
    }

    for i in 0..6 {
        let i_f = i as f32;
        let start = base + Vec3::new(-9.0 + i_f * 3.6, 5.0 + (i_f * 0.85), 17.0);
        let end = start + Vec3::new(if i % 2 == 0 { 5.5 } else { -5.5 }, 0.0, 0.0);
        spawn_moving_brick(
            commands,
            meshes,
            pal.glass_panel.clone(),
            Vec3::new(3.0, 0.42, 3.0),
            start,
            end,
            2.8 + i_f * 0.20,
            0.4 + i_f * 0.43,
        );
    }

    spawn_rotating_elevator(
        commands,
        meshes,
        pal.window_warm.clone(),
        base + Vec3::new(0.0, 10.6, 20.0),
        6.0,
        Vec3::new(4.0, 0.48, 4.0),
        -0.82,
        1.35,
        1.7,
    );
    spawn_course_block(
        commands,
        meshes,
        pal.sky_platform.clone(),
        base + Vec3::new(0.0, 14.4, 24.5),
        Vec3::new(14.0, 0.6, 14.0),
        Quat::IDENTITY,
        true,
        true,
    );
    spawn_course_marker_pair(commands, meshes, pal, base + Vec3::new(0.0, 14.8, 31.8));

    spawn_discoverable_beacon(
        commands,
        meshes,
        materials,
        DiscoverableKind::HiddenReward {
            reward_id: "hidden_course_ramp_brick_tower_cache",
            credits: 160,
            experience: 135,
            armor: 16,
            power_up: Some("ramp_brick_wall_dancer"),
            special_ability: Some("sprite_turret_pack"),
        },
        "Ramp Brick Tower Cache",
        base + Vec3::new(0.0, 14.7, 24.5),
    );
}

fn spawn_course_marker_pair(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    position: Vec3,
) {
    for x in [-3.2_f32, 3.2] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(0.28, 2.2))),
                material: MeshMaterial3d(pal.window_warm.clone()),
                transform: Transform::from_translation(position + Vec3::new(x, 0.85, 0.0)),
                ..default()
            },
            WorldGeometry,
        ));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.55))),
                material: MeshMaterial3d(pal.crystal_aurora.clone()),
                transform: Transform::from_translation(position + Vec3::new(x, 2.05, 0.0)),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_moving_brick(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    size: Vec3,
    start: Vec3,
    end: Vec3,
    speed: f32,
    phase: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(start),
            ..default()
        },
        crate::engine::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
        crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
        WorldGeometry,
        WalkableSurface,
        MovingPlatform {
            start,
            end,
            speed,
            phase,
            size,
        },
    ));
}

fn spawn_rotating_elevator(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    center: Vec3,
    radius: f32,
    size: Vec3,
    angular_speed: f32,
    vertical_amplitude: f32,
    phase: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(center + Vec3::X * radius),
            ..default()
        },
        crate::engine::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
        crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
        WorldGeometry,
        WalkableSurface,
        RotatingElevator {
            center,
            radius,
            angular_speed,
            vertical_amplitude,
            vertical_speed: angular_speed.abs().max(0.35) * 1.35,
            phase,
            size,
        },
    ));
}

fn spawn_sling_pad(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    position: Vec3,
    launch_velocity: Vec3,
    radius: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(radius, 0.22))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(position),
            ..default()
        },
        crate::engine::physics::prelude::Collider::cylinder(0.11, radius),
        crate::engine::physics::prelude::RigidBody::Fixed,
        WorldGeometry,
        WalkableSurface,
        SlingShotPad {
            launch_velocity,
            radius: radius + 0.7,
            cooldown: 0.55,
            cooldown_timer: 0.0,
        },
    ));
}

fn spawn_course_block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    position: Vec3,
    size: Vec3,
    rotation: Quat,
    collider: bool,
    walkable: bool,
) {
    let entity = commands
        .spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(material),
                transform: Transform::from_translation(position).with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
        ))
        .id();

    if collider {
        commands.entity(entity).insert((
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                size.x * 0.5,
                size.y * 0.5,
                size.z * 0.5,
            ),
        ));
    }
    if walkable {
        commands.entity(entity).insert(WalkableSurface);
    }
}

// ── Window facades ────────────────────────────────────────────────────────────
/// Spawn four emissive panels (one per building face) covering the upper 70 %
/// of the building height. They simulate rows of lit windows from a distance.
fn spawn_window_facades(
    commands: &mut Commands,
    unit_cube: &Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    x: f32,
    z: f32,
    h: f32,
    w: f32,
    d: f32,
) {
    let win_h = h * 0.70;
    let win_base_y = h * 0.15; // bottom edge of the window section
    let center_y = win_base_y + win_h * 0.5;
    let thickness = 0.35;

    // Front and back faces (span full width, aligned along X)
    for &fz in &[z - d * 0.5 - thickness * 0.5, z + d * 0.5 + thickness * 0.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(unit_cube.clone()),
                material: MeshMaterial3d(mat.clone()),
                transform: unit_cuboid_transform(
                    Vec3::new(x, center_y, fz),
                    Quat::IDENTITY,
                    Vec3::new(w - 0.8, win_h, thickness),
                ),
                ..default()
            },
            WorldGeometry,
        ));
    }
    // Left and right faces (span full depth, aligned along Z)
    for &fx in &[x - w * 0.5 - thickness * 0.5, x + w * 0.5 + thickness * 0.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(unit_cube.clone()),
                material: MeshMaterial3d(mat.clone()),
                transform: unit_cuboid_transform(
                    Vec3::new(fx, center_y, z),
                    Quat::IDENTITY,
                    Vec3::new(thickness, win_h, d - 0.8),
                ),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_modern_window_grid(
    commands: &mut Commands,
    unit_cube: &Handle<Mesh>,
    pal: &Palette,
    x: f32,
    z: f32,
    h: f32,
    w: f32,
    d: f32,
    seed: u64,
) {
    let win_h = h * 0.70;
    let win_base_y = h * 0.15;
    let center_y = win_base_y + win_h * 0.5;
    let glass_thickness = 0.18;
    let frame_thickness = 0.16;
    let frame_mat = if seeded(seed, 3) > 0.35 {
        pal.brushed_metal.clone()
    } else {
        pal.downtown_facade.clone()
    };

    if seeded(seed, 7) > 0.35 {
        for &fz in &[z - d * 0.5 - 0.45, z + d * 0.5 + 0.45] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.glass_panel.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(x, center_y, fz),
                        Quat::IDENTITY,
                        Vec3::new(w - 1.2, win_h, glass_thickness),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        for &fx in &[x - w * 0.5 - 0.45, x + w * 0.5 + 0.45] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.glass_panel.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(fx, center_y, z),
                        Quat::IDENTITY,
                        Vec3::new(glass_thickness, win_h, d - 1.2),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    let rows = (h / 14.0).floor().clamp(3.0, 10.0) as i32;
    for row in 0..=rows {
        let y = win_base_y + row as f32 * win_h / rows as f32;
        for &fz in &[z - d * 0.5 - 0.58, z + d * 0.5 + 0.58] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(frame_mat.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(x, y, fz),
                        Quat::IDENTITY,
                        Vec3::new(w - 0.7, frame_thickness, 0.24),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        for &fx in &[x - w * 0.5 - 0.58, x + w * 0.5 + 0.58] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(frame_mat.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(fx, y, z),
                        Quat::IDENTITY,
                        Vec3::new(0.24, frame_thickness, d - 0.7),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    let front_cols = (w / 6.0).floor().clamp(2.0, 6.0) as i32;
    for col in 0..=front_cols {
        let x_off = -w * 0.5 + col as f32 * w / front_cols as f32;
        for &fz in &[z - d * 0.5 - 0.60, z + d * 0.5 + 0.60] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(frame_mat.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(x + x_off, center_y, fz),
                        Quat::IDENTITY,
                        Vec3::new(frame_thickness, win_h, 0.26),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    let side_cols = (d / 6.0).floor().clamp(2.0, 6.0) as i32;
    for col in 0..=side_cols {
        let z_off = -d * 0.5 + col as f32 * d / side_cols as f32;
        for &fx in &[x - w * 0.5 - 0.60, x + w * 0.5 + 0.60] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(frame_mat.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(fx, center_y, z + z_off),
                        Quat::IDENTITY,
                        Vec3::new(0.26, win_h, frame_thickness),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

fn spawn_metal_cladding(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    position: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    seed: u64,
) {
    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    spawn_metal_cladding_cached(
        commands, pal, &unit_cube, position, width, height, depth, seed,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_metal_cladding_cached(
    commands: &mut Commands,
    pal: &Palette,
    unit_cube: &Handle<Mesh>,
    position: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    seed: u64,
) {
    let rib_h = height * 0.84;
    let rib_y = position.y + height * 0.02;
    let rib_mat = match ((seeded(seed, 1) * 4.0).floor() as u8).min(3) {
        0 => pal.stainless_metal.clone(),
        1 => pal.copper_metal.clone(),
        2 => pal.city_metal_panel.clone(),
        _ => pal.industrial_metal.clone(),
    };
    let front_cols = (width / 5.5).floor().clamp(3.0, 8.0) as i32;
    for col in 0..=front_cols {
        let x_off = -width * 0.5 + col as f32 * width / front_cols as f32;
        for &fz in &[
            position.z - depth * 0.5 - 0.18,
            position.z + depth * 0.5 + 0.18,
        ] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(rib_mat.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(position.x + x_off, rib_y, fz),
                        Quat::IDENTITY,
                        Vec3::new(0.26, rib_h, 0.28),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    let side_cols = (depth / 5.5).floor().clamp(3.0, 8.0) as i32;
    for col in 0..=side_cols {
        let z_off = -depth * 0.5 + col as f32 * depth / side_cols as f32;
        for &fx in &[
            position.x - width * 0.5 - 0.18,
            position.x + width * 0.5 + 0.18,
        ] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(rib_mat.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(fx, rib_y, position.z + z_off),
                        Quat::IDENTITY,
                        Vec3::new(0.28, rib_h, 0.26),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    for y in [
        position.y - height * 0.40,
        position.y + height * 0.08,
        position.y + height * 0.43,
    ] {
        for &fz in &[
            position.z - depth * 0.5 - 0.20,
            position.z + depth * 0.5 + 0.20,
        ] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(rib_mat.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(position.x, y, fz),
                        Quat::IDENTITY,
                        Vec3::new(width + 0.55, 0.22, 0.30),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        for &fx in &[
            position.x - width * 0.5 - 0.20,
            position.x + width * 0.5 + 0.20,
        ] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(rib_mat.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(fx, y, position.z),
                        Quat::IDENTITY,
                        Vec3::new(0.30, 0.22, depth + 0.55),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

fn spawn_factory_window_ribbons(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    x: f32,
    ground_y: f32,
    z: f32,
    h: f32,
    w: f32,
    d: f32,
) {
    for y in [ground_y + h * 0.38, ground_y + h * 0.66] {
        for &fz in &[z - d * 0.5 - 0.24, z + d * 0.5 + 0.24] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(w * 0.62, 1.25, 0.20))),
                    material: MeshMaterial3d(pal.glass_panel.clone()),
                    transform: Transform::from_xyz(x, y, fz),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        for &fx in &[x - w * 0.5 - 0.24, x + w * 0.5 + 0.24] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.20, 1.25, d * 0.62))),
                    material: MeshMaterial3d(pal.glass_panel.clone()),
                    transform: Transform::from_xyz(fx, y, z),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

#[allow(dead_code)] // Retained for isolated/downtown callers that do not own a facade cache.
fn spawn_stone_brick_courses(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    position: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    heavy_corners: bool,
) {
    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    spawn_stone_brick_courses_cached(
        commands,
        pal,
        &unit_cube,
        position,
        width,
        height,
        depth,
        heavy_corners,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_stone_brick_courses_cached(
    commands: &mut Commands,
    pal: &Palette,
    unit_cube: &Handle<Mesh>,
    position: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    heavy_corners: bool,
) {
    let ground_y = position.y - height * 0.5;
    let row_count = (height / 3.4).floor().clamp(3.0, 18.0) as i32;
    let row_step = height / row_count as f32;
    for row in 1..row_count {
        let y = ground_y + row as f32 * row_step;
        for &fz in &[
            position.z - depth * 0.5 - 0.09,
            position.z + depth * 0.5 + 0.09,
        ] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.mortar_line.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(position.x, y, fz),
                        Quat::IDENTITY,
                        Vec3::new(width + 0.20, 0.08, 0.12),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        for &fx in &[
            position.x - width * 0.5 - 0.09,
            position.x + width * 0.5 + 0.09,
        ] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.mortar_line.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(fx, y, position.z),
                        Quat::IDENTITY,
                        Vec3::new(0.12, 0.08, depth + 0.20),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    let front_cols = (width / 4.4).floor().clamp(2.0, 8.0) as i32;
    let side_cols = (depth / 4.4).floor().clamp(2.0, 8.0) as i32;
    for row in 0..row_count {
        let y = ground_y + (row as f32 + 0.5) * row_step;
        let seam_h = row_step * 0.52;
        let offset = if row % 2 == 0 { 0.0 } else { 0.5 };
        for col in 0..front_cols {
            let x_off = -width * 0.5 + (col as f32 + offset) * width / front_cols as f32;
            if x_off.abs() > width * 0.45 {
                continue;
            }
            for &fz in &[
                position.z - depth * 0.5 - 0.11,
                position.z + depth * 0.5 + 0.11,
            ] {
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(unit_cube.clone()),
                        material: MeshMaterial3d(pal.mortar_line.clone()),
                        transform: unit_cuboid_transform(
                            Vec3::new(position.x + x_off, y, fz),
                            Quat::IDENTITY,
                            Vec3::new(0.08, seam_h, 0.14),
                        ),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }
        for col in 0..side_cols {
            let z_off = -depth * 0.5 + (col as f32 + offset) * depth / side_cols as f32;
            if z_off.abs() > depth * 0.45 {
                continue;
            }
            for &fx in &[
                position.x - width * 0.5 - 0.11,
                position.x + width * 0.5 + 0.11,
            ] {
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(unit_cube.clone()),
                        material: MeshMaterial3d(pal.mortar_line.clone()),
                        transform: unit_cuboid_transform(
                            Vec3::new(fx, y, position.z + z_off),
                            Quat::IDENTITY,
                            Vec3::new(0.14, seam_h, 0.08),
                        ),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }
    }

    if heavy_corners {
        for &x in &[
            position.x - width * 0.5 - 0.16,
            position.x + width * 0.5 + 0.16,
        ] {
            for &z in &[
                position.z - depth * 0.5 - 0.16,
                position.z + depth * 0.5 + 0.16,
            ] {
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(unit_cube.clone()),
                        material: MeshMaterial3d(pal.stone_block.clone()),
                        transform: unit_cuboid_transform(
                            Vec3::new(x, position.y, z),
                            Quat::IDENTITY,
                            Vec3::new(0.42, height * 0.92, 0.42),
                        ),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }
    }
}

// ── Rooftop details ───────────────────────────────────────────────────────────
/// Add an antenna or water-tank cluster to the top of a building.
fn spawn_rooftop_details(
    commands: &mut Commands,
    mesh_cache: &DowntownFacadeMeshCache,
    pal: &Palette,
    x: f32,
    z: f32,
    h: f32,
    w: f32,
    d: f32,
    seed: u64,
) {
    let top = h;
    let kind = seeded(seed, 0);

    if kind < 0.45 {
        // Antenna mast — thin tall cylinder with small beacon sphere on top
        let mast_h = 8.0 + seeded(seed, 1) * 14.0;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_cylinder.clone()),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: unit_cylinder_transform(
                    Vec3::new(x, top + mast_h * 0.5, z),
                    Quat::IDENTITY,
                    0.22,
                    mast_h,
                ),
                ..default()
            },
            WorldGeometry,
        ));
        // Blinking beacon (emissive red sphere)
        let beacon_color = if seeded(seed, 2) > 0.5 {
            Color::srgb(1.0, 0.18, 0.12)
        } else {
            Color::srgb(1.0, 0.65, 0.0)
        };
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_sphere.clone()),
                material: MeshMaterial3d(pal.window_warm.clone()),
                transform: unit_sphere_transform(Vec3::new(x, top + mast_h + 0.5, z), 0.45),
                ..default()
            },
            WorldGeometry,
        ));
        // Tiny point-light at the beacon
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: beacon_color,
                    intensity: 6_000.0,
                    range: 18.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(x, top + mast_h + 0.5, z),
                ..default()
            },
            WorldGeometry,
        ));
    } else if kind < 0.75 {
        // Water/coolant tank — short wide cylinder
        let tank_r = 1.8 + seeded(seed, 3) * 2.2;
        let tank_h = 2.5 + seeded(seed, 4) * 2.0;
        let ox = (seeded(seed, 5) - 0.5) * (w * 0.4);
        let oz = (seeded(seed, 6) - 0.5) * (d * 0.4);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_cylinder.clone()),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: unit_cylinder_transform(
                    Vec3::new(x + ox, top + tank_h * 0.5, z + oz),
                    Quat::IDENTITY,
                    tank_r,
                    tank_h,
                ),
                ..default()
            },
            WorldGeometry,
        ));
    } else {
        // Satellite dish — flat disc + support stub
        let dish_r = 2.2 + seeded(seed, 7) * 1.8;
        let ox = (seeded(seed, 8) - 0.5) * (w * 0.35);
        let oz = (seeded(seed, 9) - 0.5) * (d * 0.35);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_cylinder.clone()),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: unit_cylinder_transform(
                    Vec3::new(x + ox, top + 1.5, z + oz),
                    Quat::from_rotation_x(0.45),
                    dish_r,
                    0.25,
                ),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

// ── Industrial ────────────────────────────────────────────────────────────────
fn spawn_industrial(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    for i in 0..35u64 {
        let x = 120.0 + seeded(seed, i * 3) * 200.0;
        let z = seeded(seed, i * 3 + 1) * 400.0 - 200.0;
        let h = 15.0 + seeded(seed, i * 3 + 2) * 40.0;
        let w = 20.0 + seeded(seed, i * 4) * 30.0;
        let d = 20.0 + seeded(seed, i * 4 + 1) * 30.0;
        let ground_y = terrain_surface_y(x, z, terrain_seed);

        let body_mat = if i % 5 == 0 {
            pal.brushed_metal.clone()
        } else if i % 2 == 0 {
            pal.industrial_metal.clone()
        } else {
            pal.industrial_rust.clone()
        };
        let enterable = i.is_multiple_of(4)
            && spawn_enterable_building(
                commands,
                meshes,
                pal,
                body_mat.clone(),
                Vec3::new(x, ground_y + h * 0.5, z),
                w,
                h,
                d,
                0.0,
                WorldZone::Industrial,
                seed + i * 109,
            );
        if !enterable {
            spawn_building(
                commands,
                meshes,
                body_mat,
                Vec3::new(x, ground_y + h * 0.5, z),
                w,
                h,
                d,
                WorldZone::Industrial,
            );
            spawn_metal_cladding(
                commands,
                meshes,
                pal,
                Vec3::new(x, ground_y + h * 0.5, z),
                w,
                h,
                d,
                seed + i * 23,
            );
            if i.is_multiple_of(3) && building_footprint_clears_speed_roads(x, z, w, d) {
                spawn_city_roof_route_and_crown(
                    commands,
                    meshes,
                    pal,
                    Vec3::new(x, ground_y, z),
                    Quat::IDENTITY,
                    w,
                    h,
                    d,
                    (h / 1.25).floor() as usize,
                    seed + i * 109,
                );
            }
        }
        if !enterable && i % 3 == 0 {
            spawn_factory_window_ribbons(commands, meshes, pal, x, ground_y, z, h, w, d);
        }

        // Chimney
        spawn_building(
            commands,
            meshes,
            pal.industrial_rust.clone(),
            Vec3::new(x + 5.0, ground_y + h + 10.0, z + 5.0),
            3.0,
            20.0,
            3.0,
            WorldZone::Industrial,
        );
    }
}

// ── Residential ───────────────────────────────────────────────────────────────
fn spawn_residential(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    for i in 0..50u64 {
        let x = -120.0 - seeded(seed, i * 3) * 200.0;
        let z = seeded(seed, i * 3 + 1) * 400.0 - 200.0;
        let h = 8.0 + seeded(seed, i * 3 + 2) * 25.0;
        let w = 10.0 + seeded(seed, i * 4) * 14.0;
        let d = 8.0 + seeded(seed, i * 4 + 1) * 14.0;
        let ground_y = terrain_surface_y(x, z, terrain_seed);

        let stone_variant = i % 3 == 0;
        let mat = if stone_variant {
            pal.stone_brick.clone()
        } else if i % 8 == 0 {
            pal.brushed_metal.clone()
        } else if i % 2 == 0 {
            pal.residential_a.clone()
        } else {
            pal.residential_b.clone()
        };
        let enterable = i.is_multiple_of(4)
            && spawn_enterable_building(
                commands,
                meshes,
                pal,
                mat.clone(),
                Vec3::new(x, ground_y + h * 0.5, z),
                w,
                h,
                d,
                0.0,
                WorldZone::Residential,
                seed + i * 113,
            );
        if !enterable {
            spawn_building(
                commands,
                meshes,
                mat,
                Vec3::new(x, ground_y + h * 0.5, z),
                w,
                h,
                d,
                WorldZone::Residential,
            );
            spawn_small_building_details(
                commands,
                meshes,
                pal,
                Vec3::new(x, ground_y + h * 0.5, z),
                w,
                h,
                d,
                seed + i,
                stone_variant,
            );
            if i.is_multiple_of(3) && building_footprint_clears_speed_roads(x, z, w, d) {
                spawn_city_roof_route_and_crown(
                    commands,
                    meshes,
                    pal,
                    Vec3::new(x, ground_y, z),
                    Quat::IDENTITY,
                    w,
                    h,
                    d,
                    (h / 1.25).floor() as usize,
                    seed + i * 113,
                );
            }
        }
    }
}

// ── Highways ──────────────────────────────────────────────────────────────────
fn spawn_boost_road_span(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    yaw: f32,
    pitch: f32,
    length: f32,
    width: f32,
    include_ramps: bool,
    seed: u64,
) {
    let mesh_cache = SpeedRoadMeshCache::new(meshes);
    spawn_boost_road_span_cached(
        commands,
        pal,
        &mesh_cache,
        center,
        yaw,
        pitch,
        length,
        width,
        include_ramps,
        seed,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_boost_road_span_cached(
    commands: &mut Commands,
    pal: &Palette,
    mesh_cache: &SpeedRoadMeshCache,
    center: Vec3,
    yaw: f32,
    pitch: f32,
    length: f32,
    width: f32,
    include_ramps: bool,
    seed: u64,
) {
    let flat_rot = Quat::from_rotation_y(yaw);
    let rot = flat_rot * Quat::from_rotation_x(-pitch);
    let forward = flat_rot * Vec3::Z;
    let road_forward = rot * Vec3::Z;
    let right = flat_rot * Vec3::X;
    // Boosts are a fast optional line, not the whole road. The broad clean
    // shoulders remain available for carving, traffic, ramps, and rails.
    let lane_width = (width * 0.16).max(6.0);
    let side_offset = width * 0.24;

    // A physical median prevents a player boosted in one direction from
    // drifting across the centerline and striking the opposite arrows. It is
    // A continuous high center wall keeps opposite high-speed directions in
    // their own skate-racing corridors. Dedicated transfers provide the safe
    // route between sides instead of accidental boost-lane crossings.
    let divider_height = SPEED_ROAD_CENTER_WALL_HEIGHT;
    let divider_width = 2.4;
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cube.clone()),
            material: MeshMaterial3d(pal.brushed_metal.clone()),
            transform: Transform::from_translation(
                center + rot * Vec3::new(0.0, divider_height * 0.5 + 0.12, 0.0),
            )
            .with_rotation(rot)
            .with_scale(Vec3::new(divider_width, divider_height, length * 1.02)),
            ..default()
        },
        WorldGeometry,
        RoadSafetyBarrier::DirectionDivider,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            divider_width * 0.5,
            divider_height * 0.5,
            length * 0.51,
        ),
        world_space_collider_scale(),
    ));

    for (lane_index, side) in [(-1.0_f32), 1.0].iter().copied().enumerate() {
        let lane_center = center + right * side * side_offset;
        let lane_dir = if side > 0.0 { forward } else { -forward };
        let lane_yaw = if side > 0.0 {
            yaw
        } else {
            yaw + std::f32::consts::PI
        };

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(mesh_cache.unit_cube.clone()),
                material: MeshMaterial3d(pal.boost_lane.clone()),
                transform: Transform::from_translation(lane_center)
                    .with_rotation(rot)
                    .with_scale(Vec3::new(lane_width, 0.07, length * 0.98)),
                ..default()
            },
            WorldGeometry,
        ));

        let pad_count = (length / 88.0).floor().max(2.0) as i32;
        for pad in 0..pad_count {
            let t = (pad as f32 + 0.5) / pad_count as f32;
            let local_z = -length * 0.5 + length * t;
            let pad_len = 13.0 + seeded(seed, lane_index as u64 * 97 + pad as u64) * 5.5;
            let pad_width = lane_width * 0.84;
            let pad_center = lane_center + rot * Vec3::new(0.0, 0.0, local_z) + Vec3::Y * 0.08;
            spawn_board_boost_pad_cached(
                commands, pal, mesh_cache, pad_center, lane_yaw, pitch, lane_dir, pad_width,
                pad_len, 3.15, 3.10, 0.0, 1.70,
            );
        }

        let light_count = (length / 135.0).floor().max(2.0) as i32;
        for light in 0..=light_count {
            let t = light as f32 / light_count as f32;
            let local_z = -length * 0.5 + length * t;
            for edge in [-1.0_f32, 1.0] {
                let light_pos = lane_center
                    + rot * Vec3::new(edge * lane_width * 0.54, 0.0, local_z)
                    + Vec3::Y * 0.18;
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(mesh_cache.unit_sphere.clone()),
                        material: MeshMaterial3d(pal.boost_pad.clone()),
                        transform: Transform::from_translation(light_pos)
                            .with_scale(Vec3::splat(0.42)),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }

        if include_ramps {
            for end in [-1.0_f32, 1.0] {
                let lane_road_direction = if side > 0.0 {
                    road_forward
                } else {
                    -road_forward
                };
                let ramp_road_direction = if end > 0.0 {
                    lane_road_direction
                } else {
                    -lane_road_direction
                };
                // Anchor the entrance in the sloped road's local frame. The
                // former flat-forward + fixed-Y offset detached ramps from
                // steep chunks at either end of a span.
                let ramp_base = lane_center + rot * Vec3::new(0.0, 0.18, end * length * 0.43);
                spawn_board_boost_ramp_cached(
                    commands,
                    pal,
                    mesh_cache,
                    ramp_base,
                    ramp_road_direction,
                    lane_width * 0.92,
                    22.0,
                    4.2,
                    3.25,
                    0.92,
                );
            }
        }
    }
}

fn unit_cuboid_transform(translation: Vec3, rotation: Quat, size: Vec3) -> Transform {
    Transform::from_translation(translation)
        .with_rotation(rotation)
        .with_scale(size)
}

fn unit_cylinder_transform(
    translation: Vec3,
    rotation: Quat,
    radius: f32,
    height: f32,
) -> Transform {
    Transform::from_translation(translation)
        .with_rotation(rotation)
        .with_scale(Vec3::new(radius, height, radius))
}

fn unit_sphere_transform(translation: Vec3, radius: f32) -> Transform {
    Transform::from_translation(translation).with_scale(Vec3::splat(radius))
}

#[allow(clippy::too_many_arguments)]
fn spawn_board_boost_pad_cached(
    commands: &mut Commands,
    pal: &Palette,
    mesh_cache: &SpeedRoadMeshCache,
    center: Vec3,
    yaw: f32,
    pitch: f32,
    direction: Vec3,
    width: f32,
    length: f32,
    speed_mult: f32,
    impulse: f32,
    lift: f32,
    duration: f32,
) {
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cube.clone()),
            material: MeshMaterial3d(pal.boost_pad.clone()),
            transform: unit_cuboid_transform(center, rot, Vec3::new(width, 0.16, length)),
            ..default()
        },
        WorldGeometry,
        BoardBoostPad {
            direction,
            half_width: width * 0.5,
            half_length: length * 0.5,
            speed_mult,
            impulse,
            lift,
            duration,
            cooldown: 0.18,
            cooldown_timers: [0.0; 4],
            force_hoverboard: true,
        },
    ));

    for (i, offset) in [-0.26_f32, 0.0, 0.26].iter().copied().enumerate() {
        let z = offset * length;
        for side in [-1.0_f32, 1.0] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(mesh_cache.unit_cube.clone()),
                    material: MeshMaterial3d(pal.boost_ramp.clone()),
                    transform: unit_cuboid_transform(
                        center + rot * Vec3::new(side * width * 0.13, 0.115, z),
                        rot * Quat::from_rotation_y(side * 0.34)
                            * Quat::from_rotation_z(if i % 2 == 0 { 0.03 } else { -0.03 }),
                        Vec3::new(width * 0.34, 0.05, 0.82),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_board_boost_ramp_cached(
    commands: &mut Commands,
    pal: &Palette,
    mesh_cache: &SpeedRoadMeshCache,
    base_center: Vec3,
    direction: Vec3,
    width: f32,
    length: f32,
    rise: f32,
    impulse: f32,
    lift: f32,
) {
    let Some((center, rot, surface_length, boost_direction)) =
        board_boost_ramp_pose(base_center, direction, length, rise)
    else {
        return;
    };
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh_cache.unit_cube.clone()),
            material: MeshMaterial3d(pal.boost_ramp.clone()),
            transform: unit_cuboid_transform(center, rot, Vec3::new(width, 0.72, surface_length)),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        BoardBoostPad {
            direction: boost_direction,
            half_width: width * 0.5,
            half_length: surface_length * 0.5,
            speed_mult: 3.15,
            impulse: impulse.max(3.9),
            lift,
            duration: 1.85,
            cooldown: 0.20,
            cooldown_timers: [0.0; 4],
            force_hoverboard: true,
        },
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(width * 0.5, 0.36, surface_length * 0.5),
        world_space_collider_scale(),
    ));
}

/// Build a ramp from its low entrance and the grade of its supporting road.
/// `rise` is additional lift above that grade, not an absolute world pitch.
fn board_boost_ramp_pose(
    base_center: Vec3,
    road_direction: Vec3,
    horizontal_length: f32,
    rise: f32,
) -> Option<(Vec3, Quat, f32, Vec3)> {
    let horizontal = road_direction.with_y(0.0);
    let horizontal_magnitude = horizontal.length();
    if horizontal_magnitude <= f32::EPSILON || horizontal_length <= f32::EPSILON {
        return None;
    }

    let forward = horizontal / horizontal_magnitude;
    let road_grade = road_direction.y / horizontal_magnitude;
    let total_rise = road_grade * horizontal_length + rise;
    let longitudinal = forward * horizontal_length + Vec3::Y * total_rise;
    let surface_length = longitudinal.length();
    let yaw = forward.x.atan2(forward.z);
    let pitch = (total_rise / horizontal_length).atan();
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);

    Some((
        base_center + longitudinal * 0.5,
        rotation,
        surface_length,
        forward,
    ))
}

fn spawn_highways(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette) {
    let road_len = TERRAIN_WORLD_SIZE;
    let road_half = road_len * 0.5;
    // Main east-west highway
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(road_len, 0.8, 18.0))),
            material: MeshMaterial3d(pal.highway.clone()),
            transform: Transform::from_xyz(0.0, 8.0, 0.0),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Building {
            zone: WorldZone::Highway,
            height: 0.8,
        },
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(road_half, 0.4, 9.0),
    ));

    // North-south cross
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(18.0, 0.8, road_len))),
            material: MeshMaterial3d(pal.highway.clone()),
            transform: Transform::from_xyz(0.0, 6.0, 0.0),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Building {
            zone: WorldZone::Highway,
            height: 0.8,
        },
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(9.0, 0.4, road_half),
    ));

    spawn_boost_road_span(
        commands,
        meshes,
        pal,
        Vec3::new(0.0, 8.54, 0.0),
        std::f32::consts::FRAC_PI_2,
        0.0,
        road_len * 0.94,
        15.4,
        true,
        7_101,
    );
    spawn_boost_road_span(
        commands,
        meshes,
        pal,
        Vec3::new(0.0, 6.54, 0.0),
        0.0,
        0.0,
        road_len * 0.94,
        15.4,
        true,
        7_203,
    );

    // Support pillars
    let pillar_count = (road_half / 180.0).ceil() as i32;
    for i in -pillar_count..=pillar_count {
        let x = i as f32 * 180.0;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(2.5, 8.0, 2.5))),
                material: MeshMaterial3d(pal.industrial_metal.clone()),
                transform: Transform::from_xyz(x, 4.0, 0.0),
                ..default()
            },
            WorldGeometry,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(1.25, 4.0, 1.25),
        ));
    }
}

// ── City Streets ─────────────────────────────────────────────────────────────
fn spawn_city_streets(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette) {
    let street_y = 0.04;
    let avenue_spacing = 48.0_f32;
    let street_span = 220.0_f32;
    let street_width = 14.0_f32;

    for i in -4..=4i32 {
        let x = i as f32 * avenue_spacing;
        spawn_street_segment(
            commands,
            meshes,
            pal,
            Vec3::new(x, street_y, 0.0),
            street_width,
            0.18,
            street_span * 2.0,
            false,
        );
    }

    for i in -4..=4i32 {
        let z = i as f32 * avenue_spacing;
        spawn_street_segment(
            commands,
            meshes,
            pal,
            Vec3::new(0.0, street_y, z),
            street_span * 2.0,
            0.18,
            street_width,
            true,
        );
    }
}

fn city_street_loop(ring: i32, y: f32) -> Vec<Vec3> {
    let ring = ring.clamp(1, 4);
    let spacing = 48.0;
    let min = -ring;
    let max = ring;
    let mut path = Vec::with_capacity((ring * 8) as usize);
    for x in min..max {
        path.push(Vec3::new(x as f32 * spacing, y, min as f32 * spacing));
    }
    for z in min..max {
        path.push(Vec3::new(max as f32 * spacing, y, z as f32 * spacing));
    }
    for x in (min + 1..=max).rev() {
        path.push(Vec3::new(x as f32 * spacing, y, max as f32 * spacing));
    }
    for z in (min + 1..=max).rev() {
        path.push(Vec3::new(min as f32 * spacing, y, z as f32 * spacing));
    }
    path
}

fn city_sidewalk_loop(block_x: i32, block_z: i32, y: f32) -> Vec<Vec3> {
    let spacing = 48.0;
    let inset = 8.8;
    let x0 = block_x as f32 * spacing + inset;
    let x1 = (block_x + 1) as f32 * spacing - inset;
    let z0 = block_z as f32 * spacing + inset;
    let z1 = (block_z + 1) as f32 * spacing - inset;
    vec![
        Vec3::new(x0, y, z0),
        Vec3::new(x1, y, z0),
        Vec3::new(x1, y, z1),
        Vec3::new(x0, y, z1),
    ]
}

fn spawn_city_life(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    let traffic_body = meshes.add(Cuboid::new(3.8, 1.1, 6.8));
    let traffic_canopy = meshes.add(Cuboid::new(2.8, 0.75, 3.4));
    let traffic_light = meshes.add(Cuboid::new(3.1, 0.18, 0.22));

    for ring in 1..=4i32 {
        let path = city_street_loop(ring, 0.92);
        for car_index in 0..3usize {
            let accent = city_accent_material(pal, seed + (ring * 11) as u64 + car_index as u64);
            let segment = (car_index * path.len() / 3 + ring as usize) % path.len();
            let cruise_speed =
                11.5 + ring as f32 * 1.15 + seeded(seed + ring as u64, car_index as u64) * 2.2;
            let lane_offset = if car_index.is_multiple_of(2) {
                -3.25
            } else {
                3.25
            };
            let vehicle = NpcRoadVehicle {
                path: path.clone(),
                segment,
                progress: seeded(seed, (ring * 7) as u64 + car_index as u64) * 24.0,
                speed: cruise_speed,
                lane_offset,
                hit_radius: 3.6,
                wreck_timer: 4.0,
            };
            let (position, yaw) = road_vehicle_pose(&vehicle).unwrap_or((path[0], 0.0));
            let entity = commands
                .spawn((
                    Name::new(format!("City Hovercar R{ring}-{}", car_index + 1)),
                    PbrBundle {
                        mesh: Mesh3d(traffic_body.clone()),
                        material: MeshMaterial3d(accent),
                        transform: Transform::from_translation(position)
                            .with_rotation(Quat::from_rotation_y(yaw)),
                        ..default()
                    },
                    vehicle,
                    CityStreetTraffic {
                        cruise_speed,
                        stop_timer: 0.0,
                        last_stopped_segment: usize::MAX,
                        signal_phase: (car_index + ring as usize) % 3,
                    },
                    WorldGeometry,
                    Health::new(90.0),
                    Damageable::default(),
                    crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
                    crate::engine::physics::prelude::Collider::cuboid(1.9, 0.55, 3.4),
                    crate::engine::physics::prelude::CollisionProfile::VehicleHurtbox,
                ))
                .id();
            commands.entity(entity).with_children(|car| {
                car.spawn((
                    PbrBundle {
                        mesh: Mesh3d(traffic_canopy.clone()),
                        material: MeshMaterial3d(pal.glass_panel.clone()),
                        transform: Transform::from_xyz(0.0, 0.82, -0.25),
                        ..default()
                    },
                    WorldGeometry,
                ));
                for z in [-3.46_f32, 3.46] {
                    car.spawn((
                        PbrBundle {
                            mesh: Mesh3d(traffic_light.clone()),
                            material: MeshMaterial3d(if z < 0.0 {
                                pal.city_aqua.clone()
                            } else {
                                pal.city_coral.clone()
                            }),
                            transform: Transform::from_xyz(0.0, -0.12, z),
                            ..default()
                        },
                        WorldGeometry,
                    ));
                }
            });
        }
    }

    // Parked hovercars make curb lanes feel inhabited without adding simulation.
    for parked_index in 0..12u64 {
        let horizontal = parked_index.is_multiple_of(2);
        let road = (parked_index as i32 % 5) - 2;
        let along = ((parked_index as i32 * 29) % 330) as f32 - 165.0;
        let position = if horizontal {
            Vec3::new(along, 0.82, road as f32 * 48.0 + 4.8)
        } else {
            Vec3::new(road as f32 * 48.0 - 4.8, 0.82, along)
        };
        commands.spawn((
            Name::new("Parked City Hovercar"),
            PbrBundle {
                mesh: Mesh3d(traffic_body.clone()),
                material: MeshMaterial3d(city_accent_material(pal, seed + parked_index * 5)),
                transform: Transform::from_translation(position).with_rotation(if horizontal {
                    Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
                } else {
                    Quat::IDENTITY
                }),
                ..default()
            },
            CityParkedVehicle,
            WorldGeometry,
        ));
    }

    let pedestrian_body = meshes.add(Capsule3d::new(0.38, 1.05));
    let pedestrian_head = meshes.add(Sphere::new(0.38));
    let pedestrian_blocks = [
        (-3, -2),
        (-3, 0),
        (-2, -3),
        (-2, 1),
        (-1, 2),
        (0, -3),
        (0, 1),
        (1, -2),
        (1, 0),
        (2, -1),
        (2, 1),
        (2, 2),
    ];
    for (block_index, (block_x, block_z)) in pedestrian_blocks.into_iter().enumerate() {
        let path = city_sidewalk_loop(block_x, block_z, 1.12);
        for walker_index in 0..2usize {
            let phase = seeded(seed + block_index as u64, walker_index as u64) * 5.0;
            let pedestrian = CityPedestrian {
                path: path.clone(),
                segment: (block_index + walker_index * 2) % path.len(),
                progress: seeded(seed + 31, (block_index * 2 + walker_index) as u64) * 12.0,
                speed: 1.7 + seeded(seed + 47, block_index as u64) * 0.8,
                pause_timer: phase.fract() * 0.5,
                phase,
            };
            let (position, yaw) = city_pedestrian_pose(&pedestrian).unwrap_or((path[0], 0.0));
            let body_material =
                city_accent_material(pal, seed + (block_index * 2 + walker_index) as u64);
            let entity = commands
                .spawn((
                    Name::new("Friendly City Resident"),
                    PbrBundle {
                        mesh: Mesh3d(pedestrian_body.clone()),
                        material: MeshMaterial3d(body_material),
                        transform: Transform::from_translation(position)
                            .with_rotation(Quat::from_rotation_y(yaw)),
                        ..default()
                    },
                    pedestrian,
                    WorldGeometry,
                ))
                .id();
            commands.entity(entity).with_children(|resident| {
                resident.spawn((
                    PbrBundle {
                        mesh: Mesh3d(pedestrian_head.clone()),
                        material: MeshMaterial3d(pal.interior_wall.clone()),
                        transform: Transform::from_xyz(0.0, 1.06, 0.0),
                        ..default()
                    },
                    WorldGeometry,
                ));
            });
        }
    }
}

fn spawn_street_segment(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    position: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    horizontal: bool,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width, height, depth))),
            material: MeshMaterial3d(pal.street_asphalt.clone()),
            transform: Transform::from_translation(position),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
    ));

    let total = if horizontal { width } else { depth };
    let dash_len = 7.0_f32;
    let dash_gap = 9.0_f32;
    let dash_count = ((total / (dash_len + dash_gap)).floor() as i32).max(1);
    for i in -dash_count..=dash_count {
        let offset = i as f32 * (dash_len + dash_gap);
        let dash_pos = if horizontal {
            Vec3::new(position.x + offset, position.y + 0.11, position.z)
        } else {
            Vec3::new(position.x, position.y + 0.11, position.z + offset)
        };
        let dash_mesh = if horizontal {
            Cuboid::new(dash_len, 0.03, 0.45)
        } else {
            Cuboid::new(0.45, 0.03, dash_len)
        };
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(dash_mesh)),
                material: MeshMaterial3d(pal.street_paint.clone()),
                transform: Transform::from_translation(dash_pos),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

// ── Sky Platforms ─────────────────────────────────────────────────────────────
fn spawn_sky_platforms(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    for i in 0..12u64 {
        let x = seeded(seed, i * 4) * 600.0 - 300.0;
        let y = 40.0 + seeded(seed, i * 4 + 1) * 210.0;
        let z = seeded(seed, i * 4 + 2) * 600.0 - 300.0;
        let size = 25.0 + seeded(seed, i * 4 + 3) * 40.0;

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(size, 3.0))),
                material: MeshMaterial3d(pal.sky_platform.clone()),
                transform: Transform::from_xyz(x, y, z),
                ..default()
            },
            WorldGeometry,
            SkyPlatform,
            WalkableSurface,
            Building {
                zone: WorldZone::SkyPlatform,
                height: 3.0,
            },
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cylinder(1.5, size),
        ));
    }
}

// ── Sky Bridges ───────────────────────────────────────────────────────────────
fn unit_cylinder_between_transform(start: Vec3, end: Vec3, radius: f32) -> Option<Transform> {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.05 {
        return None;
    }

    Some(
        Transform::from_translation(start + delta * 0.5)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, delta / length))
            .with_scale(Vec3::new(radius, length, radius)),
    )
}

fn spawn_static_cylinder_between_cached(
    commands: &mut Commands,
    unit_cylinder: &Handle<Mesh>,
    material: Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    radius: f32,
) {
    let Some(transform) = unit_cylinder_between_transform(start, end, radius) else {
        return;
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(unit_cylinder.clone()),
            material: MeshMaterial3d(material),
            transform,
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_static_cylinder_between(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    radius: f32,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.05 {
        return;
    }

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(radius, length))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(start + delta * 0.5)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, delta.normalize())),
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_sky_bridges(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    for i in 0..12u64 {
        let x = seeded(seed, i * 7) * 620.0 - 310.0;
        let y = 58.0 + seeded(seed, i * 7 + 1) * 142.0;
        let z = seeded(seed, i * 7 + 2) * 620.0 - 310.0;
        let yaw = seeded(seed, i * 7 + 3) * TAU;
        let len = 96.0 + seeded(seed, i * 7 + 4) * 78.0;
        let width = 7.4 + seeded(seed, i * 7 + 5) * 4.8;
        let sag = 1.0 + seeded(seed, i * 7 + 6) * 2.8;
        let base = Vec3::new(x, y, z);
        let rot = Quat::from_rotation_y(yaw);

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(unit_cube.clone()),
                material: MeshMaterial3d(pal.bridge_stone.clone()),
                transform: Transform::from_translation(base)
                    .with_rotation(rot)
                    .with_scale(Vec3::new(width, 0.92, len)),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            Building {
                zone: WorldZone::SkyPlatform,
                height: 1.5,
            },
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(width * 0.5, 0.75, len * 0.5),
            world_space_collider_scale(),
        ));

        spawn_boost_road_span(
            commands,
            meshes,
            pal,
            base + Vec3::Y * 1.02,
            yaw,
            0.0,
            len * 0.82,
            width * 0.72,
            true,
            seed + i * 311,
        );

        let segments = 8;
        let plank_len = len / segments as f32;
        for segment in 0..segments {
            let t = (segment as f32 + 0.5) / segments as f32;
            let centered = t * 2.0 - 1.0;
            let local_z = -len * 0.5 + plank_len * (segment as f32 + 0.5);
            let sag_y = -(1.0 - centered.abs()).powi(2) * sag * 0.18;
            let wobble = (seeded(seed, i * 101 + segment as u64) - 0.5) * 0.16;
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.bridge_deck.clone()),
                    transform: Transform::from_translation(
                        base + rot * Vec3::new(0.0, 0.62 + sag_y + wobble, local_z),
                    )
                    .with_rotation(rot)
                    .with_scale(Vec3::new(
                        width * 0.94,
                        0.18,
                        plank_len * 0.82,
                    )),
                    ..default()
                },
                WorldGeometry,
            ));
        }

        for side in [-1.0_f32, 1.0] {
            let rail_x = side * width * 0.62;
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.bridge_stone.clone()),
                    transform: Transform::from_translation(
                        base + rot * Vec3::new(rail_x, 2.25, 0.0),
                    )
                    .with_rotation(rot)
                    .with_scale(Vec3::new(0.42, 0.48, len * 0.96)),
                    ..default()
                },
                WorldGeometry,
            ));

            for post in 0..6 {
                let t = post as f32 / 5.0;
                let local_z = -len * 0.5 + len * t;
                let sag_y = -(1.0 - (t * 2.0 - 1.0).abs()).powi(2) * sag * 0.24;
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(unit_cube.clone()),
                        material: MeshMaterial3d(pal.bridge_stone.clone()),
                        transform: Transform::from_translation(
                            base + rot * Vec3::new(rail_x, 1.75 + sag_y, local_z),
                        )
                        .with_rotation(rot)
                        .with_scale(Vec3::new(0.76, 3.2, 0.76)),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }

            let mut last_cable: Option<Vec3> = None;
            for cable_step in 0..=8 {
                let t = cable_step as f32 / 8.0;
                let local_z = -len * 0.5 + len * t;
                let arch = 5.2 - (1.0 - (t * 2.0 - 1.0).abs()).powi(2) * sag * 0.55;
                let point = base + rot * Vec3::new(rail_x, arch, local_z);
                if let Some(last) = last_cable {
                    spawn_static_cylinder_between(
                        commands,
                        meshes,
                        pal.guide_glow.clone(),
                        last,
                        point,
                        0.12,
                    );
                }
                last_cable = Some(point);
            }
        }

        for end in [-1.0_f32, 1.0] {
            let anchor = base + rot * Vec3::new(0.0, 0.0, end * len * 0.52);
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.stainless_metal.clone()),
                    transform: Transform::from_translation(anchor + Vec3::Y * 4.0)
                        .with_rotation(rot)
                        .with_scale(Vec3::new(width * 1.15, 8.0, 3.2)),
                    ..default()
                },
                WorldGeometry,
            ));
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cone {
                        radius: width * 0.38,
                        height: 4.0,
                    })),
                    material: MeshMaterial3d(pal.copper_metal.clone()),
                    transform: Transform::from_translation(anchor + Vec3::Y * 10.0)
                        .with_rotation(rot),
                    ..default()
                },
                WorldGeometry,
            ));
            commands.spawn((
                PointLightBundle {
                    point_light: PointLight {
                        color: Color::srgb(0.36, 0.92, 1.0),
                        intensity: 9_000.0,
                        range: 46.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    transform: Transform::from_translation(anchor + Vec3::Y * 7.0),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

fn spawn_moving_platforms(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let routes = [
        (
            Vec3::new(-64.0, 11.0, -42.0),
            Vec3::new(52.0, 24.0, -42.0),
            Vec3::new(13.0, 1.2, 7.0),
        ),
        (
            Vec3::new(120.0, 18.0, 96.0),
            Vec3::new(120.0, 46.0, 176.0),
            Vec3::new(12.0, 1.2, 12.0),
        ),
        (
            Vec3::new(-210.0, 16.0, 40.0),
            Vec3::new(-286.0, 34.0, 126.0),
            Vec3::new(14.0, 1.2, 8.0),
        ),
        (
            Vec3::new(286.0, 22.0, -132.0),
            Vec3::new(386.0, 54.0, -132.0),
            Vec3::new(16.0, 1.2, 8.0),
        ),
        (
            Vec3::new(430.0, 86.0, -40.0),
            Vec3::new(490.0, 126.0, -40.0),
            Vec3::new(12.0, 1.2, 8.0),
        ),
        (
            Vec3::new(-430.0, 70.0, -330.0),
            Vec3::new(-508.0, 116.0, -390.0),
            Vec3::new(14.0, 1.2, 9.0),
        ),
    ];

    for (i, &(start_offset, end_offset, size)) in routes.iter().enumerate() {
        let start = Vec3::new(
            start_offset.x,
            terrain_surface_y(start_offset.x, start_offset.z, terrain_seed) + start_offset.y,
            start_offset.z,
        );
        let end = Vec3::new(
            end_offset.x,
            terrain_surface_y(end_offset.x, end_offset.z, terrain_seed) + end_offset.y,
            end_offset.z,
        );
        let phase = seeded(seed, i as u64) * std::f32::consts::TAU;
        let speed = 5.5 + seeded(seed, i as u64 + 20) * 4.5;
        let initial = start.lerp(end, 0.5 - 0.5 * phase.sin());

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(pal.sky_platform.clone()),
                transform: Transform::from_translation(initial),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            MovingPlatform {
                start,
                end,
                speed,
                phase,
                size,
            },
            crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
            crate::engine::physics::prelude::Collider::cuboid(
                size.x * 0.5,
                size.y * 0.5,
                size.z * 0.5,
            ),
        ));

        for point in [start, end] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cylinder::new(0.55, 1.8))),
                    material: MeshMaterial3d(pal.window_cool.clone()),
                    transform: Transform::from_xyz(point.x, point.y - 1.4, point.z),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

fn spawn_laser_turrets(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let placements = [
        (-78.0_f32, 48.0_f32, 10.0_f32),
        (92.0, -68.0, 12.0),
        (238.0, 110.0, 18.0),
        (-255.0, 148.0, 16.0),
        (356.0, 352.0, 3.2),
        (-348.0, 344.0, 3.2),
        (430.0, -96.0, 20.0),
        (-426.0, -330.0, 22.0),
    ];

    for (i, &(x, z, y_offset)) in placements.iter().enumerate() {
        let y = terrain_surface_y(x, z, terrain_seed) + y_offset;
        let transform = Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_y(
            seeded(seed, i as u64) * std::f32::consts::TAU,
        ));
        let turret = commands
            .spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cylinder::new(0.9, 1.2))),
                    material: MeshMaterial3d(pal.rooftop.clone()),
                    transform,
                    ..default()
                },
                WorldGeometry,
                LaserTurret {
                    range: 62.0,
                    cooldown: 1.7 + seeded(seed, i as u64 + 30) * 0.9,
                    cooldown_timer: seeded(seed, i as u64 + 60) * 1.5,
                    windup: 0.34,
                    windup_timer: 0.0,
                    locked_target: None,
                    damage: 7.0 + seeded(seed, i as u64 + 90) * 5.0,
                    beam_material: pal.window_cool.clone(),
                },
                crate::engine::physics::prelude::RigidBody::Fixed,
                crate::engine::physics::prelude::Collider::cylinder(0.6, 0.9),
            ))
            .id();

        commands.entity(turret).with_children(|parent| {
            parent.spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.28, 0.24, 2.1))),
                material: MeshMaterial3d(pal.window_cool.clone()),
                transform: Transform::from_xyz(0.0, 0.35, -1.0),
                ..default()
            });
            parent.spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.26))),
                material: MeshMaterial3d(pal.dragon_lava.clone()),
                transform: Transform::from_xyz(0.0, 0.62, -0.78),
                ..default()
            });
        });
    }
}

// ── Spaceports ────────────────────────────────────────────────────────────────
fn spawn_spaceports(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    terrain_seed: u64,
) {
    let positions = [
        (350.0_f32, 350.0_f32),
        (-350.0, 350.0),
        (350.0, -350.0),
        (-350.0, -350.0),
    ];

    for (x, z) in positions {
        let y = terrain_surface_y(x, z, terrain_seed) + 1.0;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(50.0, 2.0))),
                material: MeshMaterial3d(pal.sky_platform.clone()),
                transform: Transform::from_xyz(x, y, z),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            Building {
                zone: WorldZone::Spaceport,
                height: 2.0,
            },
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cylinder(1.0, 50.0),
        ));
    }
}

// ── Mountains ─────────────────────────────────────────────────────────────────
/// Spawn layered rock spires and snow caps on top of the terrain heightmap.
/// Each "mountain" is several overlapping cones of varying radii and heights
/// so the silhouette reads as a proper rocky peak rather than a single cone.
fn spawn_mountains(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    let corners = [
        Vec3::new(8600.0, 0.0, 8600.0),
        Vec3::new(-8600.0, 0.0, 8600.0),
        Vec3::new(8600.0, 0.0, -8600.0),
        Vec3::new(-8600.0, 0.0, -8600.0),
    ];

    for (ci, &corner) in corners.iter().enumerate() {
        for i in 0..18u64 {
            let idx = ci as u64 * 20 + i;

            let ox = (seeded(seed, idx * 5) - 0.5) * 1700.0;
            let oz = (seeded(seed, idx * 5 + 1) - 0.5) * 1700.0;
            let wx = corner.x + ox;
            let wz = corner.z + oz;

            // Base Y from terrain so spires sit flush on the landscape.
            let ground_y = terrain_surface_y(wx, wz, seed - 5);

            let peak_h = 110.0 + seeded(seed, idx * 5 + 2) * 260.0;
            let base_r = 58.0 + seeded(seed, idx * 5 + 3) * 82.0;
            let yaw = seeded(seed, idx * 5 + 4) * TAU;
            let lean_x = (seeded(seed, idx * 5 + 5) - 0.5) * 0.16;
            let lean_z = (seeded(seed, idx * 5 + 6) - 0.5) * 0.16;
            let rock_mat = if seeded(seed, idx * 5 + 7) > 0.72 {
                pal.rock_dark.clone()
            } else {
                pal.rock.clone()
            };

            // ── Main spire ──────────────────────────────────────────────
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cone {
                        radius: base_r,
                        height: peak_h,
                    })),
                    material: MeshMaterial3d(rock_mat.clone()),
                    transform: Transform::from_xyz(wx, ground_y + peak_h * 0.5, wz).with_rotation(
                        Quat::from_rotation_y(yaw)
                            * Quat::from_rotation_x(lean_x)
                            * Quat::from_rotation_z(lean_z),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));

            for band in 0..3u64 {
                let band_t = 0.24 + band as f32 * 0.18;
                let band_r =
                    base_r * (0.92 - band_t * 0.55) * (0.78 + seeded(seed, idx * 29 + band) * 0.32);
                if band_r <= 8.0 {
                    continue;
                }
                let band_y = ground_y + peak_h * band_t;
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Cylinder::new(band_r, 0.86))),
                        material: MeshMaterial3d(pal.rock_dark.clone()),
                        transform: Transform::from_xyz(wx, band_y, wz)
                            .with_rotation(Quat::from_rotation_y(yaw + band as f32 * 0.7))
                            .with_scale(Vec3::new(1.0, 1.0, 0.46)),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }

            // ── Shoulder ridges (2 smaller cones offset from the peak) ──
            for s in 0..2u64 {
                let sr = base_r * (0.45 + seeded(seed, idx * 5 + 4 + s) * 0.25);
                let sh = peak_h * (0.50 + seeded(seed, idx * 7 + s) * 0.30);
                let sdx = (seeded(seed, idx * 11 + s) - 0.5) * base_r * 1.2;
                let sdz = (seeded(seed, idx * 13 + s) - 0.5) * base_r * 1.2;
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Cone {
                            radius: sr,
                            height: sh,
                        })),
                        material: MeshMaterial3d(rock_mat.clone()),
                        transform: Transform::from_xyz(wx + sdx, ground_y + sh * 0.5, wz + sdz)
                            .with_rotation(
                                Quat::from_rotation_y(yaw + s as f32 * 1.7)
                                    * Quat::from_rotation_x(lean_x * 0.7)
                                    * Quat::from_rotation_z(lean_z * 0.7),
                            ),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }

            // ── Snow cap on tall peaks ───────────────────────────────────
            if peak_h > 190.0 {
                let cap_r = base_r * 0.28;
                let cap_h = peak_h * 0.18;
                let cap_y = ground_y + peak_h - cap_h * 0.35;
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Cone {
                            radius: cap_r,
                            height: cap_h,
                        })),
                        material: MeshMaterial3d(pal.snow.clone()),
                        transform: Transform::from_xyz(wx, cap_y, wz),
                        ..default()
                    },
                    WorldGeometry,
                ));

                // Secondary snow blobs for a less perfect cap.
                for b in 0..2u64 {
                    let br = cap_r * (0.4 + seeded(seed, idx * 17 + b) * 0.3);
                    let bdx = (seeded(seed, idx * 19 + b) - 0.5) * cap_r * 1.4;
                    let bdz = (seeded(seed, idx * 23 + b) - 0.5) * cap_r * 1.4;
                    commands.spawn((
                        PbrBundle {
                            mesh: Mesh3d(meshes.add(Sphere::new(br))),
                            material: MeshMaterial3d(pal.snow.clone()),
                            transform: Transform::from_xyz(wx + bdx, cap_y + br * 0.3, wz + bdz),
                            ..default()
                        },
                        WorldGeometry,
                    ));
                }
            }
        }
    }
}

// ── Neon Lights ───────────────────────────────────────────────────────────────
fn spawn_neon_lights(commands: &mut Commands, seed: u64) {
    let neon_colors = [
        Color::srgb(0.0, 1.0, 1.0),
        Color::srgb(1.0, 0.0, 0.8),
        Color::srgb(0.0, 1.0, 0.3),
        Color::srgb(1.0, 0.6, 0.0),
        Color::srgb(0.5, 0.0, 1.0),
    ];

    for i in 0..70u64 {
        let x = seeded(seed, i * 3) * 600.0 - 300.0;
        let y = 5.0 + seeded(seed, i * 3 + 1) * 60.0;
        let z = seeded(seed, i * 3 + 2) * 600.0 - 300.0;
        let ci = (i % 5) as usize;

        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: neon_colors[ci],
                    intensity: 20_000.0,
                    range: 25.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(x, y, z),
                ..default()
            },
            WorldGeometry,
            NeonLight,
        ));
    }
}

// ── Street Lights ─────────────────────────────────────────────────────────────
fn spawn_street_lights(commands: &mut Commands, seed: u64) {
    for i in 0..80u64 {
        let x = seeded(seed, i * 2) * 400.0 - 200.0;
        let z = seeded(seed, i * 2 + 1) * 400.0 - 200.0;

        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: Color::srgb(0.9, 0.85, 0.7),
                    intensity: 8_000.0,
                    range: 18.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(x, 7.0, z),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

// ── Outer Districts ───────────────────────────────────────────────────────────
fn spawn_outer_districts(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let offsets = [
        Vec3::new(300.0, 0.0, 0.0),
        Vec3::new(-300.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 300.0),
        Vec3::new(0.0, 0.0, -300.0),
        Vec3::new(250.0, 0.0, -250.0),
    ];

    for (di, &offset) in offsets.iter().enumerate() {
        for i in 0..20u64 {
            let idx = di as u64 * 20 + i;
            let ox = seeded(seed, idx * 3) * 120.0 - 60.0;
            let oz = seeded(seed, idx * 3 + 1) * 120.0 - 60.0;
            let h = 10.0 + seeded(seed, idx * 3 + 2) * 50.0;
            let w = 8.0 + seeded(seed, idx * 4) * 15.0;
            let d = 8.0 + seeded(seed, idx * 4 + 1) * 15.0;
            let x = offset.x + ox;
            let z = offset.z + oz;
            let ground_y = terrain_surface_y(x, z, terrain_seed);
            let stone_variant = (di + i as usize).is_multiple_of(2);
            let mat = if stone_variant {
                pal.stone_brick.clone()
            } else if idx.is_multiple_of(7) {
                pal.brushed_metal.clone()
            } else if di % 2 == 0 {
                pal.residential_a.clone()
            } else {
                pal.residential_b.clone()
            };

            let enterable = idx.is_multiple_of(5)
                && spawn_enterable_building(
                    commands,
                    meshes,
                    pal,
                    mat.clone(),
                    Vec3::new(x, ground_y + h * 0.5, z),
                    w,
                    h,
                    d,
                    0.0,
                    WorldZone::OuterDistrict,
                    seed + idx * 127,
                );
            if !enterable {
                spawn_building(
                    commands,
                    meshes,
                    mat,
                    Vec3::new(x, ground_y + h * 0.5, z),
                    w,
                    h,
                    d,
                    WorldZone::OuterDistrict,
                );
                spawn_small_building_details(
                    commands,
                    meshes,
                    pal,
                    Vec3::new(x, ground_y + h * 0.5, z),
                    w,
                    h,
                    d,
                    seed + idx,
                    stone_variant,
                );
                if idx.is_multiple_of(3) && building_footprint_clears_speed_roads(x, z, w, d) {
                    spawn_city_roof_route_and_crown(
                        commands,
                        meshes,
                        pal,
                        Vec3::new(x, ground_y, z),
                        Quat::IDENTITY,
                        w,
                        h,
                        d,
                        (h / 1.25).floor() as usize,
                        seed + idx * 127,
                    );
                }
            }
        }
    }
}

// ── River ─────────────────────────────────────────────────────────────────────

const PERIMETER_OCEAN_LEVEL: f32 = 126.0;

#[derive(Clone, Copy)]
struct TravelIslandSpec {
    id: &'static str,
    label: &'static str,
    center: Vec2,
    coast_dock: Vec2,
    island_dock: Vec2,
    radius: f32,
}

const TRAVEL_ISLANDS: &[TravelIslandSpec] = &[
    TravelIslandSpec {
        id: "island_aurora_reach",
        label: "Aurora Reach",
        center: Vec2::new(850.0, 11_200.0),
        coast_dock: Vec2::new(120.0, 9_350.0),
        island_dock: Vec2::new(760.0, 10_830.0),
        radius: 185.0,
    },
    TravelIslandSpec {
        id: "island_stormglass",
        label: "Stormglass Isle",
        center: Vec2::new(11_200.0, -1_700.0),
        coast_dock: Vec2::new(9_350.0, -1_180.0),
        island_dock: Vec2::new(10_820.0, -1_590.0),
        radius: 210.0,
    },
    TravelIslandSpec {
        id: "island_emberwake",
        label: "Emberwake Atoll",
        center: Vec2::new(1_900.0, -11_200.0),
        coast_dock: Vec2::new(1_250.0, -9_350.0),
        island_dock: Vec2::new(1_790.0, -10_820.0),
        radius: 195.0,
    },
    TravelIslandSpec {
        id: "island_moonwreck",
        label: "Moonwreck Cay",
        center: Vec2::new(-11_200.0, 2_450.0),
        coast_dock: Vec2::new(-9_350.0, 1_850.0),
        island_dock: Vec2::new(-10_820.0, 2_320.0),
        radius: 175.0,
    },
];

const MOUNTAIN_LAKES: &[(&str, f32, f32, f32, f32)] = &[
    ("Lake Crownmirror", -5_900.0, 6_050.0, 245.0, 175.0),
    ("Lake Riftglass", -3_050.0, -3_350.0, 210.0, 150.0),
    ("Lake Starfall", 3_550.0, 1_450.0, 280.0, 185.0),
    ("Lake Dragontear", 6_250.0, -3_650.0, 230.0, 165.0),
    ("Lake Southlight", 1_150.0, -6_550.0, 260.0, 170.0),
    ("Lake Aurora", -1_300.0, 4_250.0, 225.0, 155.0),
];

#[allow(dead_code)]
fn perimeter_travel_island_count() -> usize {
    TRAVEL_ISLANDS.len()
}

#[allow(dead_code)]
fn mountain_lake_count() -> usize {
    MOUNTAIN_LAKES.len()
}

fn spawn_perimeter_ocean_and_islands(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    ocean_water: &Handle<WaterMaterial>,
    seed: u64,
) {
    // Four overlapping slabs form a continuous sea around the 20 km terrain
    // square without putting transparent water beneath the whole playable map.
    for (center, size) in [
        (Vec2::new(0.0, 11_000.0), Vec2::new(26_000.0, 4_000.0)),
        (Vec2::new(0.0, -11_000.0), Vec2::new(26_000.0, 4_000.0)),
        (Vec2::new(11_000.0, 0.0), Vec2::new(4_000.0, 18_000.0)),
        (Vec2::new(-11_000.0, 0.0), Vec2::new(4_000.0, 18_000.0)),
    ] {
        commands.spawn((
            Name::new("Perimeter Ocean"),
            Mesh3d(meshes.add(Cuboid::new(size.x, 0.10, size.y))),
            MeshMaterial3d(ocean_water.clone()),
            Transform::from_xyz(center.x, PERIMETER_OCEAN_LEVEL, center.y),
            WorldGeometry,
            WaterBody {
                kind: WaterBodyKind::Ocean,
                surface_y: PERIMETER_OCEAN_LEVEL,
                depth: 90.0,
                navigable: true,
                footprint: WaterFootprint::Rectangle {
                    half_extents: size * 0.5,
                },
            },
        ));
    }

    for (index, spec) in TRAVEL_ISLANDS.iter().copied().enumerate() {
        spawn_travel_island(commands, meshes, pal, spec, seed + index as u64 * 113);
    }
}

fn spawn_travel_island(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    spec: TravelIslandSpec,
    seed: u64,
) {
    let island_y = PERIMETER_OCEAN_LEVEL + 1.8;
    let island_dock = Vec3::new(
        spec.island_dock.x,
        PERIMETER_OCEAN_LEVEL + 0.52,
        spec.island_dock.y,
    );
    commands.spawn((
        Name::new(spec.label),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(spec.radius, 3.6))),
            material: MeshMaterial3d(pal.moss.clone()),
            transform: Transform::from_xyz(spec.center.x, island_y, spec.center.y),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        TravelIsland {
            id: spec.id,
            label: spec.label,
            dock_position: island_dock,
        },
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(1.8, spec.radius),
    ));

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cone {
                radius: spec.radius * 0.52,
                height: 58.0,
            })),
            material: MeshMaterial3d(pal.rock.clone()),
            transform: Transform::from_xyz(
                spec.center.x,
                PERIMETER_OCEAN_LEVEL + 29.0,
                spec.center.y,
            ),
            ..default()
        },
        WorldGeometry,
    ));

    for prop in 0..10u64 {
        let angle = prop as f32 / 10.0 * TAU + seeded(seed, prop) * 0.35;
        let radius = spec.radius * (0.42 + seeded(seed, 20 + prop) * 0.38);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(5.0 + seeded(seed, 40 + prop) * 7.0))),
                material: MeshMaterial3d(if prop % 3 == 0 {
                    pal.crystal_emerald.clone()
                } else {
                    pal.rock_dark.clone()
                }),
                transform: Transform::from_xyz(
                    spec.center.x + angle.cos() * radius,
                    PERIMETER_OCEAN_LEVEL + 5.0,
                    spec.center.y + angle.sin() * radius,
                )
                .with_scale(Vec3::new(1.0, 1.8, 1.0)),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let coast_dock = Vec3::new(
        spec.coast_dock.x,
        PERIMETER_OCEAN_LEVEL + 0.52,
        spec.coast_dock.y,
    );
    spawn_ferry_route(commands, meshes, pal, coast_dock, island_dock);
    spawn_world_anchor(commands, spec.id, island_dock + Vec3::Y * 1.0);
}

fn spawn_ferry_route(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    coast_dock: Vec3,
    island_dock: Vec3,
) {
    let route = island_dock - coast_dock;
    let route_length = route.with_y(0.0).length();
    let forward = route.with_y(0.0).normalize_or_zero();
    let yaw = forward.x.atan2(forward.z);
    let midpoint = coast_dock.lerp(island_dock, 0.5);

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(26.0, 0.05, route_length))),
            material: MeshMaterial3d(pal.glass_panel.clone()),
            transform: Transform::from_translation(midpoint + Vec3::Y * 0.04)
                .with_rotation(Quat::from_rotation_y(yaw)),
            ..default()
        },
        WorldGeometry,
    ));

    for dock in [coast_dock, island_dock] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(34.0, 0.7, 46.0))),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_translation(dock)
                    .with_rotation(Quat::from_rotation_y(yaw)),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(17.0, 0.35, 23.0),
        ));
    }

    let boat_start = coast_dock + forward * 34.0;
    let boat = commands
        .spawn((
            Name::new("Island Ferry"),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(7.0, 1.1, 13.5))),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: Transform::from_translation(boat_start)
                    .with_rotation(Quat::from_rotation_y(yaw)),
                ..default()
            },
            WorldGeometry,
            BoatVehicle {
                embark_radius: 11.0,
                passenger_radius: 18.0,
                dock_radius: 28.0,
                route_half_width: 30.0,
                speed: 34.0,
                dock_position: coast_dock,
                island_position: island_dock,
            },
        ))
        .id();
    commands.entity(boat).with_children(|ferry| {
        ferry.spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(5.8, 2.4, 4.4))),
            material: MeshMaterial3d(pal.window_cool.clone()),
            transform: Transform::from_xyz(0.0, 1.6, -1.0),
            ..default()
        });
        ferry.spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(0.14, 5.5))),
            material: MeshMaterial3d(pal.stainless_metal.clone()),
            transform: Transform::from_xyz(0.0, 3.8, 0.8),
            ..default()
        });
        ferry.spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.55))),
            material: MeshMaterial3d(pal.boost_pad.clone()),
            transform: Transform::from_xyz(0.0, 6.5, 0.8),
            ..default()
        });
    });
}

fn spawn_mountain_water_network(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    lake_water: &Handle<WaterMaterial>,
    river_water: &Handle<WaterMaterial>,
    waterfall_fx: &WaterfallFxAssets,
    seed: u64,
) {
    for (lake_index, &(label, x, z, radius_x, radius_z)) in MOUNTAIN_LAKES.iter().enumerate() {
        let surface_y = mountain_lake_surface_y(x, z, seed);
        let radius = radius_x.max(radius_z);
        commands.spawn((
            Name::new(label),
            Mesh3d(meshes.add(Cylinder::new(radius, 0.12))),
            MeshMaterial3d(lake_water.clone()),
            Transform::from_xyz(x, surface_y, z).with_scale(Vec3::new(
                radius_x / radius,
                1.0,
                radius_z / radius,
            )),
            WorldGeometry,
            WaterBody {
                kind: WaterBodyKind::Lake,
                surface_y,
                depth: 5.0,
                navigable: true,
                footprint: WaterFootprint::Ellipse {
                    radii: Vec2::new(radius_x, radius_z),
                },
            },
        ));

        let outlet_direction =
            Vec2::from_angle(seeded(seed + 8_117, lake_index as u64) * TAU).normalize_or_zero();
        let outlet = Vec2::new(x, z) + outlet_direction * radius_x.max(radius_z) * 0.80;
        spawn_downhill_stream(
            commands,
            meshes,
            pal,
            river_water,
            waterfall_fx,
            outlet,
            seed,
            lake_index as u64,
        );
    }
}

fn spawn_downhill_stream(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    river_water: &Handle<WaterMaterial>,
    waterfall_fx: &WaterfallFxAssets,
    source: Vec2,
    seed: u64,
    stream_index: u64,
) {
    let mut current = source;
    let mut previous_direction =
        Vec2::from_angle(seeded(seed + 8_117, stream_index) * TAU).normalize_or_zero();
    for step in 0..18u64 {
        let current_height = terrain_surface_y(current.x, current.y, seed);
        let Some((next, next_height)) =
            downhill_stream_step(current, previous_direction, seed, stream_index * 31 + step)
        else {
            break;
        };
        let direction = (next - current).normalize_or_zero();
        previous_direction = direction;
        let start = Vec3::new(current.x, current_height + 0.32, current.y);
        let end = Vec3::new(next.x, next_height + 0.32, next.y);
        let drop = current_height - next_height;
        if drop >= 7.0 {
            spawn_world_waterfall(commands, meshes, pal, waterfall_fx, start, end, drop);
        } else {
            spawn_river_water_segment(commands, meshes, river_water, start, end);
        }
        current = next;
    }
}

fn downhill_stream_step(
    current: Vec2,
    previous_direction: Vec2,
    seed: u64,
    salt: u64,
) -> Option<(Vec2, f32)> {
    let current_height = terrain_surface_y(current.x, current.y, seed);
    let step_length = 125.0;
    (0..12)
        .map(|candidate| {
            let angle = candidate as f32 / 12.0 * TAU + seeded(seed + salt, candidate) * 0.08;
            let direction = Vec2::from_angle(angle);
            let point = current + direction * step_length;
            let height = terrain_surface_y(point.x, point.y, seed);
            let turn_penalty = (1.0 - direction.dot(previous_direction).max(-0.5)) * 2.5;
            (point, height, height + turn_penalty)
        })
        .filter(|(_, height, _)| *height <= current_height + 1.5)
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(point, height, _)| (point, height))
}

fn spawn_river_water_segment(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    river_water: &Handle<WaterMaterial>,
    start: Vec3,
    end: Vec3,
) {
    let delta = end - start;
    let horizontal = delta.with_y(0.0);
    let horizontal_length = horizontal.length();
    if horizontal_length <= 0.1 {
        return;
    }
    let yaw = horizontal.x.atan2(horizontal.z);
    let pitch = (delta.y / horizontal_length).atan();
    let midpoint = start.lerp(end, 0.5);
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(24.0, 0.10, delta.length() * 1.06))),
        MeshMaterial3d(river_water.clone()),
        Transform::from_translation(midpoint)
            .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch)),
        WorldGeometry,
        WaterBody {
            kind: WaterBodyKind::River,
            surface_y: midpoint.y,
            depth: 2.2,
            navigable: false,
            footprint: WaterFootprint::Rectangle {
                half_extents: Vec2::new(12.0, delta.length() * 0.53),
            },
        },
    ));
}

fn spawn_world_waterfall(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    waterfall_fx: &WaterfallFxAssets,
    start: Vec3,
    end: Vec3,
    drop: f32,
) {
    let horizontal = (end - start).with_y(0.0);
    let forward = horizontal.normalize_or_zero();
    let yaw = forward.x.atan2(forward.z);
    let center = Vec3::new(end.x, end.y + drop * 0.5, end.z);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(26.0, drop, 1.2))),
            material: MeshMaterial3d(pal.water.clone()),
            transform: Transform::from_translation(center)
                .with_rotation(Quat::from_rotation_y(yaw)),
            ..default()
        },
        WorldGeometry,
        WaterfallFlowRibbon {
            base_translation: center,
            base_scale: Vec3::ONE,
            phase: seeded(
                start.x.to_bits() as u64 ^ start.z.to_bits() as u64,
                end.y.to_bits() as u64,
            ),
        },
        WaterBody {
            kind: WaterBodyKind::Waterfall,
            surface_y: start.y,
            depth: drop,
            navigable: false,
            footprint: WaterFootprint::Rectangle {
                half_extents: Vec2::new(13.0, 0.6),
            },
        },
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(7.0))),
            material: MeshMaterial3d(pal.waterfall_mist.clone()),
            transform: Transform::from_translation(end + Vec3::Y * 1.8)
                .with_scale(Vec3::new(2.4, 0.55, 1.4)),
            ..default()
        },
        WorldGeometry,
    ));
    let right = Vec3::new(forward.z, 0.0, -forward.x).normalize_or_zero();
    spawn_waterfall_fx(
        commands,
        waterfall_fx,
        end,
        forward,
        right,
        26.0,
        drop,
        start.x.to_bits() as u64 ^ end.z.to_bits() as u64,
    );
}

fn spawn_river(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    water: &Handle<WaterMaterial>,
) {
    for i in -15..=15i32 {
        let z = i as f32 * 40.0;
        let x = (z * 0.05).sin() * 60.0;

        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(25.0, 0.1, 42.0))),
            MeshMaterial3d(water.clone()),
            Transform::from_xyz(x, -0.1, z),
            WorldGeometry,
            WaterBody {
                kind: WaterBodyKind::River,
                surface_y: -0.1,
                depth: 2.0,
                navigable: false,
                footprint: WaterFootprint::Rectangle {
                    half_extents: Vec2::new(12.5, 21.0),
                },
            },
        ));
    }

    for (z, style) in [(-440.0_f32, 0_u64), (0.0, 1), (440.0, 2)] {
        let x = (z * 0.05).sin() * 60.0;
        spawn_river_bridge(commands, meshes, pal, Vec3::new(x, 0.82, z), style);
    }
}

fn spawn_river_bridge(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    style: u64,
) {
    let deck_len = 74.0 + style as f32 * 8.0;
    let deck_width = 11.0;
    let deck_height = 0.72;
    let yaw = if style.is_multiple_of(2) { 0.08 } else { -0.11 };
    let rot = Quat::from_rotation_y(yaw);
    let deck_mat = if style == 1 {
        pal.bridge_deck.clone()
    } else {
        pal.marble.clone()
    };

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(deck_len, deck_height, deck_width))),
            material: MeshMaterial3d(deck_mat),
            transform: Transform::from_translation(center).with_rotation(rot),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            deck_len * 0.5,
            deck_height * 0.5,
            deck_width * 0.5,
        ),
    ));

    spawn_boost_road_span(
        commands,
        meshes,
        pal,
        center + Vec3::Y * (deck_height * 0.5 + 0.18),
        yaw + std::f32::consts::FRAC_PI_2,
        0.0,
        deck_len * 0.70,
        deck_width * 0.76,
        true,
        8_900 + style,
    );

    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(deck_len * 0.96, 0.72, 0.58))),
                material: MeshMaterial3d(pal.copper_metal.clone()),
                transform: Transform::from_translation(
                    center + rot * Vec3::new(0.0, 1.0, side * deck_width * 0.62),
                )
                .with_rotation(rot),
                ..default()
            },
            WorldGeometry,
        ));
    }

    for end in [-1.0_f32, 1.0] {
        for side in [-1.0_f32, 1.0] {
            let local = Vec3::new(end * deck_len * 0.42, 1.55, side * deck_width * 0.55);
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(2.4, 3.1, 2.4))),
                    material: MeshMaterial3d(pal.stainless_metal.clone()),
                    transform: Transform::from_translation(center + rot * local).with_rotation(rot),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

fn spawn_chapter_one_ocean_island(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    water: &Handle<WaterMaterial>,
) {
    let ocean_transform = Transform::from_xyz(0.0, 0.18, 760.0);
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(840.0, 0.08, 430.0))),
        MeshMaterial3d(water.clone()),
        ocean_transform,
        WorldGeometry,
        WaterBody {
            kind: WaterBodyKind::Ocean,
            surface_y: ocean_transform.translation.y,
            depth: 28.0,
            navigable: true,
            footprint: WaterFootprint::Rectangle {
                half_extents: Vec2::new(420.0, 215.0),
            },
        },
        NatureSway::new(&ocean_transform, 1.7, 0.7, 0.001, 0.035, 0.004),
    ));

    // A visible moon-glass wake doubles as the boat route collider, so the
    // player never feels blocked by an invisible bridge across the ocean.
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(30.0, 0.14, 314.0))),
            material: MeshMaterial3d(pal.glass_panel.clone()),
            transform: Transform::from_xyz(0.0, 0.09, 725.0),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        TravelIsland {
            id: "ch01_ocean_island",
            label: "North-Coast Island",
            dock_position: Vec3::new(0.0, 0.52, 826.0),
        },
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(15.0, 0.07, 157.0),
    ));

    for (z, label_offset) in [(574.0_f32, -18.0_f32), (826.0, 18.0)] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(24.0, 0.42, 38.0))),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_xyz(0.0, 0.31, z),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(12.0, 0.21, 19.0),
        ));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(3.0, 2.2, 3.0))),
                material: MeshMaterial3d(pal.window_warm.clone()),
                transform: Transform::from_xyz(-13.0, 1.32, z + label_offset),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let island_center = Vec3::new(0.0, 0.32, 890.0);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(74.0, 0.72))),
            material: MeshMaterial3d(pal.moss.clone()),
            transform: Transform::from_translation(island_center),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(0.36, 74.0),
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cone {
                radius: 46.0,
                height: 18.0,
            })),
            material: MeshMaterial3d(pal.rock.clone()),
            transform: Transform::from_xyz(0.0, 9.0, 905.0),
            ..default()
        },
        WorldGeometry,
    ));
    for i in 0..8u64 {
        let angle = i as f32 * std::f32::consts::TAU / 8.0;
        let radius = 30.0 + seeded(91_000, i) * 26.0;
        let x = angle.cos() * radius;
        let z = 890.0 + angle.sin() * radius;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(1.2 + seeded(91_000, i + 20) * 1.6))),
                material: MeshMaterial3d(pal.rock_dark.clone()),
                transform: Transform::from_xyz(x, 0.84, z)
                    .with_scale(Vec3::new(1.8, 0.35, 1.2))
                    .with_rotation(Quat::from_rotation_y(angle)),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let boat_entity = commands
        .spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(5.4, 0.7, 9.5))),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: Transform::from_xyz(0.0, 0.52, 604.0),
                ..default()
            },
            WorldGeometry,
            BoatVehicle {
                embark_radius: 9.0,
                passenger_radius: 15.0,
                dock_radius: 18.0,
                route_half_width: 22.0,
                speed: 26.0,
                dock_position: Vec3::new(0.0, 0.52, 604.0),
                island_position: Vec3::new(0.0, 0.52, 826.0),
            },
        ))
        .id();

    commands.entity(boat_entity).with_children(|boat| {
        boat.spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(6.4, 0.32, 1.1))),
            material: MeshMaterial3d(pal.window_cool.clone()),
            transform: Transform::from_xyz(0.0, 0.48, -3.0),
            ..default()
        });
        boat.spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(0.12, 4.0))),
            material: MeshMaterial3d(pal.brushed_metal.clone()),
            transform: Transform::from_xyz(0.0, 2.2, 0.4),
            ..default()
        });
        boat.spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Rectangle::new(3.4, 3.0))),
            material: MeshMaterial3d(pal.window_warm.clone()),
            transform: Transform::from_xyz(0.0, 2.5, 0.9)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
            ..default()
        });
    });

    spawn_world_anchor(commands, "ch01_ocean_dock", Vec3::new(0.0, 0.6, 574.0));
    spawn_world_anchor(commands, "ch01_island_landing", Vec3::new(0.0, 0.9, 826.0));
}

fn spawn_water_gardens(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let ponds = [
        (185.0_f32, 150.0_f32, 24.0_f32, 1.45_f32, 0.70_f32),
        (-235.0, 130.0, 32.0, 1.20, 0.82),
        (420.0, -118.0, 30.0, 1.55, 0.62),
        (-305.0, -245.0, 28.0, 1.30, 0.75),
        (92.0, -285.0, 22.0, 0.95, 1.25),
        (-70.0, 335.0, 26.0, 1.50, 0.70),
    ];

    for (i, &(x, z, r, sx, sz)) in ponds.iter().enumerate() {
        let y = terrain_surface_y(x, z, terrain_seed) + 0.08;
        let rot = seeded(seed, i as u64) * std::f32::consts::TAU;
        let transform = Transform::from_xyz(x, y, z)
            .with_rotation(Quat::from_rotation_y(rot))
            .with_scale(Vec3::new(sx, 1.0, sz));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(r, 0.12))),
                material: MeshMaterial3d(pal.water.clone()),
                transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(
                &transform,
                seeded(seed, i as u64 + 10) * std::f32::consts::TAU,
                0.9,
                0.002,
                0.025,
                0.010,
            ),
        ));

        for stone in 0..8u64 {
            let a = stone as f32 * std::f32::consts::TAU / 8.0 + rot * 0.2;
            let rr = r * (0.78 + seeded(seed, i as u64 * 30 + stone) * 0.34);
            let px = x + a.cos() * rr * sx;
            let pz = z + a.sin() * rr * sz;
            let py = terrain_surface_y(px, pz, terrain_seed) + 0.12;
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Sphere::new(1.2 + seeded(seed, 400 + stone) * 1.1))),
                    material: MeshMaterial3d(if stone % 3 == 0 {
                        pal.moss.clone()
                    } else {
                        pal.rock.clone()
                    }),
                    transform: Transform::from_xyz(px, py, pz)
                        .with_scale(Vec3::new(1.6, 0.28, 1.05))
                        .with_rotation(Quat::from_rotation_y(a)),
                    ..default()
                },
                WorldGeometry,
            ));
        }

        spawn_reed_ring(
            commands,
            meshes,
            pal,
            seed + i as u64 * 100,
            terrain_seed,
            x,
            z,
            r,
            sx,
            sz,
        );
    }
}

fn spawn_reed_ring(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
    cx: f32,
    cz: f32,
    radius: f32,
    scale_x: f32,
    scale_z: f32,
) {
    let reed_mesh = meshes.add(Cylinder::new(0.045, 1.0));
    for i in 0..26u64 {
        let a = seeded(seed, i * 3) * std::f32::consts::TAU;
        let rr = radius * (0.82 + seeded(seed, i * 3 + 1) * 0.36);
        let h = 0.8 + seeded(seed, i * 3 + 2) * 1.2;
        let x = cx + a.cos() * rr * scale_x;
        let z = cz + a.sin() * rr * scale_z;
        let y = terrain_surface_y(x, z, terrain_seed);
        let transform = Transform::from_xyz(x, y + h * 0.5, z)
            .with_rotation(Quat::from_rotation_z((seeded(seed, i + 40) - 0.5) * 0.22))
            .with_scale(Vec3::new(1.0, h, 1.0));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(reed_mesh.clone()),
                material: MeshMaterial3d(pal.reed.clone()),
                transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(
                &transform,
                seeded(seed, i + 80) * std::f32::consts::TAU,
                2.0,
                0.08,
                0.02,
                0.012,
            ),
        ));
    }
}

fn spawn_rock_fields(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let rock_mesh = meshes.add(Sphere::new(1.0));
    for i in 0..170u64 {
        let x = seeded(seed, i * 4) * 1100.0 - 550.0;
        let z = seeded(seed, i * 4 + 1) * 1100.0 - 550.0;
        let dist = (x * x + z * z).sqrt();
        if dist < 135.0 && seeded(seed, i * 4 + 2) < 0.72 {
            continue;
        }
        if starter_clear_zone(x, z, 8.0) {
            continue;
        }
        let y = terrain_surface_y(x, z, terrain_seed);
        if y > 155.0 {
            continue;
        }
        let sx = 0.8 + seeded(seed, i * 7) * 3.4;
        let sy = 0.28 + seeded(seed, i * 7 + 1) * 1.1;
        let sz = 0.7 + seeded(seed, i * 7 + 2) * 2.7;
        let transform = Transform::from_xyz(x, y + sy * 0.45, z)
            .with_scale(Vec3::new(sx, sy, sz))
            .with_rotation(Quat::from_rotation_y(
                seeded(seed, i * 7 + 3) * std::f32::consts::TAU,
            ));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(rock_mesh.clone()),
                material: MeshMaterial3d(if i % 5 == 0 {
                    pal.rock_dark.clone()
                } else {
                    pal.rock.clone()
                }),
                transform,
                ..default()
            },
            WorldGeometry,
        ));
        if i % 4 == 0 {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(rock_mesh.clone()),
                    material: MeshMaterial3d(pal.moss.clone()),
                    transform: Transform::from_xyz(x, y + sy * 0.95, z).with_scale(Vec3::new(
                        sx * 0.46,
                        0.08,
                        sz * 0.40,
                    )),
                    ..default()
                },
                WorldGeometry,
                NatureSway::new(
                    &Transform::from_xyz(x, y + sy * 0.95, z).with_scale(Vec3::new(
                        sx * 0.46,
                        0.08,
                        sz * 0.40,
                    )),
                    seeded(seed, i + 500) * std::f32::consts::TAU,
                    1.4,
                    0.003,
                    0.010,
                    0.012,
                ),
            ));
        }
    }
}

fn spawn_understory(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let bush_mesh = meshes.add(Sphere::new(1.0));
    let flower_mesh = meshes.add(Sphere::new(0.20));
    let stem_mesh = meshes.add(Cylinder::new(0.025, 0.55));
    let twig_mesh = meshes.add(Cylinder::new(0.05, 1.0));

    for i in 0..1_180u64 {
        let x = seeded(seed, i * 5) * 2200.0 - 1100.0;
        let z = seeded(seed, i * 5 + 1) * 2200.0 - 1100.0;
        let dist = (x * x + z * z).sqrt();
        if dist < 75.0 && seeded(seed, i * 5 + 2) < 0.55 {
            continue;
        }
        if starter_clear_zone(x, z, 6.0) {
            continue;
        }
        let y = terrain_surface_y(x, z, terrain_seed);
        if y > 190.0 {
            continue;
        }
        let phase = seeded(seed, i + 300) * std::f32::consts::TAU;
        if i % 3 == 0 {
            let s = 0.75 + seeded(seed, i * 5 + 3) * 2.05;
            let mat = match i % 9 {
                0 | 1 => pal.foliage_a.clone(),
                2..=4 => pal.foliage_b.clone(),
                5 | 6 => pal.foliage_c.clone(),
                _ => pal.moss.clone(),
            };
            let twig_mat = match i % 4 {
                0 => pal.bark_light.clone(),
                1 => pal.bark_mid.clone(),
                _ => pal.bark_dark.clone(),
            };
            if i % 4 != 0 {
                let twig_h = s * (0.75 + seeded(seed, i * 11) * 0.65);
                let twig_transform = Transform::from_xyz(x, y + twig_h * 0.5, z)
                    .with_rotation(Quat::from_rotation_y(phase))
                    .with_scale(Vec3::new(s * 1.4, twig_h, s * 1.4));
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(twig_mesh.clone()),
                        material: MeshMaterial3d(twig_mat),
                        transform: twig_transform,
                        ..default()
                    },
                    WorldGeometry,
                    NatureSway::new(&twig_transform, phase, 1.0, 0.018, 0.010, 0.006),
                ));
            }
            for lobe in 0..3u64 {
                let ox = (seeded(seed, i * 9 + lobe) - 0.5) * s * 0.9;
                let oz = (seeded(seed, i * 9 + lobe + 3) - 0.5) * s * 0.9;
                let lobe_scale = s * (0.7 + seeded(seed, i * 9 + lobe + 6) * 0.55);
                let transform = Transform::from_xyz(x + ox, y + lobe_scale * 0.48, z + oz)
                    .with_scale(Vec3::new(lobe_scale, lobe_scale * 0.75, lobe_scale));
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(bush_mesh.clone()),
                        material: MeshMaterial3d(mat.clone()),
                        transform,
                        ..default()
                    },
                    WorldGeometry,
                    NatureSway::new(
                        &transform,
                        phase + lobe as f32,
                        1.1 + seeded(seed, i + lobe) * 0.8,
                        0.035,
                        0.025,
                        0.020,
                    ),
                ));
            }
            if i % 11 == 0 {
                spawn_moss_pad(
                    commands,
                    meshes,
                    pal,
                    Vec3::new(x, y + 0.035, z),
                    s * 0.82,
                    seed + i * 23,
                );
            }
            if i % 19 == 0 {
                spawn_vine_curtain(
                    commands,
                    meshes,
                    pal,
                    Vec3::new(x, y + s * 1.10, z),
                    phase,
                    s * 0.72,
                    s * 1.18,
                    2 + i % 2,
                    seed + i * 31,
                );
            }
            if i % 17 == 0 {
                let stone_transform = Transform::from_xyz(x + s * 0.8, y + 0.16, z - s * 0.6)
                    .with_scale(Vec3::new(s * 1.2, 0.12, s * 0.9));
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(bush_mesh.clone()),
                        material: MeshMaterial3d(pal.moss.clone()),
                        transform: stone_transform,
                        ..default()
                    },
                    WorldGeometry,
                    NatureSway::new(&stone_transform, phase + 1.7, 1.2, 0.006, 0.006, 0.006),
                ));
            }
        } else {
            let h = 0.35 + seeded(seed, i * 5 + 3) * 0.55;
            let stem_transform =
                Transform::from_xyz(x, y + h * 0.5, z).with_scale(Vec3::new(1.0, h, 1.0));
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(stem_mesh.clone()),
                    material: MeshMaterial3d(pal.reed.clone()),
                    transform: stem_transform,
                    ..default()
                },
                WorldGeometry,
                NatureSway::new(&stem_transform, phase, 1.8, 0.055, 0.018, 0.010),
            ));
            let flower_transform = Transform::from_xyz(x, y + h + 0.10, z)
                .with_scale(Vec3::splat(0.9 + seeded(seed, i * 7) * 0.7));
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(flower_mesh.clone()),
                    material: MeshMaterial3d(if i % 4 == 0 {
                        pal.flower_a.clone()
                    } else {
                        pal.flower_b.clone()
                    }),
                    transform: flower_transform,
                    ..default()
                },
                WorldGeometry,
                NatureSway::new(&flower_transform, phase + 0.8, 2.2, 0.080, 0.030, 0.025),
            ));
        }
    }
}

// ── Trees ─────────────────────────────────────────────────────────────────────
fn spawn_trees(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let mesh_cache = NaturePrimitiveMeshCache::new(meshes);
    // Build one template per species (L-system evaluated once, materials allocated once).
    // All species share a single unit cylinder + unit sphere mesh so the
    // thousands of branch/leaf entities batch into a handful of draw calls.
    let branch_mesh = mesh_cache.unit_cylinder.clone();
    let leaf_mesh = mesh_cache.unit_sphere.clone();
    let oak = TreeTemplate::new_with_materials(
        TreeKind::Oak,
        pal.bark_mid.clone(),
        Some(pal.foliage_b.clone()),
        branch_mesh.clone(),
        leaf_mesh.clone(),
    );
    let pine = TreeTemplate::new_with_materials(
        TreeKind::Pine,
        pal.bark_dark.clone(),
        Some(pal.foliage_c.clone()),
        branch_mesh.clone(),
        leaf_mesh.clone(),
    );
    let dead = TreeTemplate::new_with_materials(
        TreeKind::Dead,
        pal.bark_light.clone(),
        None,
        branch_mesh.clone(),
        leaf_mesh.clone(),
    );
    let cyber = TreeTemplate::new_with_materials(
        TreeKind::Cyber,
        pal.city_metal_panel.clone(),
        Some(pal.guide_glow.clone()),
        branch_mesh.clone(),
        leaf_mesh.clone(),
    );

    // (zone_offset, count, template_ref, scale_base, scale_range)
    struct Placement<'a> {
        offset: Vec3,
        spread: f32,
        count: u64,
        template: &'a TreeTemplate,
        scale_base: f32,
        scale_range: f32,
    }

    let placements = [
        // Oak — residential west, scattered at ground level
        Placement {
            offset: Vec3::new(-150.0, 0.0, 0.0),
            spread: 210.0,
            count: 46,
            template: &oak,
            scale_base: 1.65,
            scale_range: 1.05,
        },
        // Oak — outer district north
        Placement {
            offset: Vec3::new(0.0, 0.0, 250.0),
            spread: 250.0,
            count: 44,
            template: &oak,
            scale_base: 1.45,
            scale_range: 0.95,
        },
        // Pine — near mountain corners (NE)
        Placement {
            offset: Vec3::new(320.0, 0.0, 320.0),
            spread: 240.0,
            count: 38,
            template: &pine,
            scale_base: 1.85,
            scale_range: 1.25,
        },
        // Pine — mountain corner SW
        Placement {
            offset: Vec3::new(-320.0, 0.0, -320.0),
            spread: 240.0,
            count: 38,
            template: &pine,
            scale_base: 1.85,
            scale_range: 1.25,
        },
        // Dead — industrial zone east, gaunt silhouettes
        Placement {
            offset: Vec3::new(200.0, 0.0, 0.0),
            spread: 140.0,
            count: 16,
            template: &dead,
            scale_base: 1.35,
            scale_range: 0.65,
        },
        // Cyber — downtown fringe, neon accent trees
        Placement {
            offset: Vec3::new(60.0, 0.0, 60.0),
            spread: 130.0,
            count: 20,
            template: &cyber,
            scale_base: 0.95,
            scale_range: 0.45,
        },
        // Mixed forest pockets between the city and fantasy domains.
        Placement {
            offset: Vec3::new(260.0, 0.0, -120.0),
            spread: 260.0,
            count: 44,
            template: &oak,
            scale_base: 1.25,
            scale_range: 1.05,
        },
        Placement {
            offset: Vec3::new(-260.0, 0.0, 230.0),
            spread: 260.0,
            count: 40,
            template: &oak,
            scale_base: 1.30,
            scale_range: 0.95,
        },
        Placement {
            offset: Vec3::new(720.0, 0.0, 520.0),
            spread: 320.0,
            count: 38,
            template: &pine,
            scale_base: 1.70,
            scale_range: 1.10,
        },
        Placement {
            offset: Vec3::new(-720.0, 0.0, -580.0),
            spread: 320.0,
            count: 38,
            template: &oak,
            scale_base: 1.55,
            scale_range: 1.10,
        },
    ];

    let mut idx = 0u64;
    for p in &placements {
        for _ in 0..p.count {
            let ox = (seeded(seed, idx * 4) - 0.5) * 2.0 * p.spread;
            let oz = (seeded(seed, idx * 4 + 1) - 0.5) * 2.0 * p.spread;
            let rot = seeded(seed, idx * 4 + 2) * std::f32::consts::TAU;
            let sc = p.scale_base + seeded(seed, idx * 4 + 3) * p.scale_range;
            let x = p.offset.x + ox;
            let z = p.offset.z + oz;
            let y = terrain_surface_y(x, z, terrain_seed);
            let pos = Vec3::new(x, y, z);

            // Keep the vehicle corridor clear: no trees on or beside the
            // speed-road network (they were the objects vehicles crashed into).
            if distance_to_speed_road_network(x, z, terrain_seed) < SPEED_ROAD_WIDTH * 0.5 + 10.0 {
                idx += 1;
                continue;
            }

            let tree = spawn_tree(commands, meshes, p.template, pos, rot, sc);
            commands.entity(tree).insert(NatureSway {
                base_translation: pos,
                base_rotation: Quat::from_rotation_y(rot),
                base_scale: Vec3::splat(sc),
                phase: seeded(seed, idx * 4 + 4) * std::f32::consts::TAU,
                speed: 0.55 + seeded(seed, idx * 4 + 5) * 0.55,
                sway: 0.012 + seeded(seed, idx * 4 + 6) * 0.018,
                bob: 0.0,
                pulse: 0.002,
            });
            if !matches!(p.template.kind, TreeKind::Cyber) {
                spawn_moss_pad_cached(
                    commands,
                    pal,
                    &mesh_cache.unit_sphere,
                    pos + Vec3::Y * 0.035,
                    sc * (1.75 + seeded(seed, idx * 4 + 7) * 1.05),
                    seed + idx * 43,
                );
            }
            match p.template.kind {
                TreeKind::Oak if idx % 3 != 1 => spawn_vine_curtain_cached(
                    commands,
                    pal,
                    &mesh_cache.unit_sphere,
                    &mesh_cache.unit_cylinder,
                    pos + Vec3::Y * (sc * 3.5),
                    rot,
                    sc * 1.9,
                    sc * 2.7,
                    2 + idx % 3,
                    seed + idx * 47,
                ),
                TreeKind::Pine if idx.is_multiple_of(4) => spawn_vine_curtain_cached(
                    commands,
                    pal,
                    &mesh_cache.unit_sphere,
                    &mesh_cache.unit_cylinder,
                    pos + Vec3::Y * (sc * 3.1),
                    rot + 0.5,
                    sc * 1.25,
                    sc * 2.2,
                    2,
                    seed + idx * 53,
                ),
                TreeKind::Dead if idx.is_multiple_of(2) => spawn_vine_curtain_cached(
                    commands,
                    pal,
                    &mesh_cache.unit_sphere,
                    &mesh_cache.unit_cylinder,
                    pos + Vec3::Y * (sc * 2.4),
                    rot + 0.8,
                    sc * 1.35,
                    sc * 2.0,
                    1 + idx % 2,
                    seed + idx * 59,
                ),
                _ => {}
            }
            idx += 1;
        }
        idx += 100; // separate seed regions between placement groups
    }
}

// ── Building helper ───────────────────────────────────────────────────────────

/// World seed used by [`spawn_building`] to test footprints against the speed
/// road network. Set once during world spawn; defaults to the standard seed so
/// isolated callers (tests) still match the shipped world.
static ROAD_NETWORK_SEED: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

fn road_network_seed() -> u64 {
    *ROAD_NETWORK_SEED.get_or_init(|| 42_195)
}

/// Vehicle/board headroom inside a building tunnel gateway.
const BUILDING_TUNNEL_CLEARANCE: f32 = 10.0;
/// Drivable opening width through a tunnel building.
const BUILDING_TUNNEL_PASSAGE: f32 = 30.0;

fn spawn_enterable_piece(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    base: Vec3,
    rotation: Quat,
    local: Vec3,
    size: Vec3,
    walkable: bool,
) {
    let mut piece = commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(base + rotation * local).with_rotation(rotation),
            ..default()
        },
        WorldGeometry,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
    ));
    if walkable {
        piece.insert(WalkableSurface);
    }
}

fn enterable_floor_layout(height: f32) -> (usize, f32) {
    let floor_count = ((height / 5.6).floor() as usize).clamp(2, 12);
    let story_height = ((height - 0.8) / floor_count as f32).clamp(4.5, 6.8);
    (floor_count, story_height)
}

#[derive(Debug, Clone, Copy)]
struct CityAccessPlan {
    balcony_floor: usize,
    balcony_y: f32,
    balcony_width: f32,
    stair_run: f32,
    ladder_rungs: usize,
}

fn city_access_plan(
    width: f32,
    height: f32,
    floor_count: usize,
    story_height: f32,
) -> CityAccessPlan {
    let balcony_floor = 1.min(floor_count.saturating_sub(1));
    let balcony_y = balcony_floor as f32 * story_height + 0.18;
    CityAccessPlan {
        balcony_floor,
        balcony_y,
        balcony_width: (width * 0.62).clamp(6.5, 11.0),
        stair_run: (balcony_y * 1.75).clamp(7.5, 13.0),
        ladder_rungs: (height / 1.25).floor().max(4.0) as usize,
    }
}

fn building_interior_kind(zone: WorldZone, seed: u64) -> BuildingInteriorKind {
    match zone {
        WorldZone::Downtown => {
            if seed.is_multiple_of(3) {
                BuildingInteriorKind::Market
            } else {
                BuildingInteriorKind::Lobby
            }
        }
        WorldZone::Industrial => BuildingInteriorKind::Laboratory,
        WorldZone::Residential => {
            if seed.is_multiple_of(5) {
                BuildingInteriorKind::Market
            } else {
                BuildingInteriorKind::Home
            }
        }
        WorldZone::OuterDistrict => match seed % 3 {
            0 => BuildingInteriorKind::Home,
            1 => BuildingInteriorKind::Market,
            _ => BuildingInteriorKind::Laboratory,
        },
        _ => BuildingInteriorKind::Lobby,
    }
}

fn building_roof_hatch_size(depth: f32, opening_width: f32) -> Vec2 {
    Vec2::new((opening_width - 0.35).max(2.7), 3.2_f32.min(depth - 2.0))
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BuildingRewardPlan {
    location: BuildingRewardLocation,
    local_position: Vec3,
    credits: u32,
    experience: u32,
    armor: u32,
}

fn building_reward_plan(
    zone: WorldZone,
    seed: u64,
    width: f32,
    height: f32,
    depth: f32,
    floor_count: usize,
    story_height: f32,
) -> BuildingRewardPlan {
    let location = if seed.is_multiple_of(2) {
        BuildingRewardLocation::Rooftop
    } else {
        BuildingRewardLocation::Interior
    };
    let zone_bonus = match zone {
        WorldZone::Downtown => 18,
        WorldZone::Industrial => 14,
        WorldZone::OuterDistrict => 12,
        _ => 8,
    };
    let local_position = match location {
        BuildingRewardLocation::Rooftop => Vec3::new(
            (seeded(seed, 251) - 0.5) * width * 0.35,
            height + 1.1,
            (seeded(seed, 252) - 0.5) * depth * 0.35,
        ),
        BuildingRewardLocation::Interior => {
            let upper_floors = floor_count.saturating_sub(1).max(1);
            let floor = 1 + (seed % upper_floors as u64) as usize;
            Vec3::new(
                (-width * 0.23).clamp(-5.5, -3.0),
                floor.min(floor_count - 1) as f32 * story_height + 1.0,
                (depth * 0.19).clamp(1.8, 4.2),
            )
        }
    };
    BuildingRewardPlan {
        location,
        local_position,
        credits: 32 + zone_bonus + (seed % 17) as u32,
        experience: 24 + zone_bonus + (seed % 13) as u32,
        armor: 4 + (seed % 7) as u32,
    }
}

fn building_reward_key(zone: WorldZone, base: Vec3) -> String {
    let zone_id = match zone {
        WorldZone::Downtown => "downtown",
        WorldZone::Industrial => "industrial",
        WorldZone::Residential => "residential",
        WorldZone::OuterDistrict => "outer",
        WorldZone::Highway => "highway",
        WorldZone::Mountain => "mountain",
        WorldZone::SkyPlatform => "sky",
        WorldZone::Spaceport => "spaceport",
        WorldZone::Ground => "ground",
    };
    format!(
        "building_explore:{zone_id}:{}:{}",
        (base.x * 10.0).round() as i64,
        (base.z * 10.0).round() as i64
    )
}

fn building_footprint_clears_speed_roads(x: f32, z: f32, width: f32, depth: f32) -> bool {
    let road_dist = distance_to_speed_road_network(x, z, road_network_seed());
    let half_diag = Vec2::new(width, depth).length() * 0.5;
    road_dist >= half_diag.max(SPEED_ROAD_WIDTH * 0.5)
        && building_footprint_clears_star_city(x, z, half_diag)
}

fn building_footprint_clears_star_city(x: f32, z: f32, half_diag: f32) -> bool {
    signed_distance_to_star_city_road_footprint(x, z) >= half_diag
}

#[allow(clippy::too_many_arguments)]
fn spawn_enterable_building(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    exterior: Handle<StandardMaterial>,
    center: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    yaw: f32,
    zone: WorldZone,
    seed: u64,
) -> bool {
    if width < 11.5 || depth < 11.5 || height < 9.0 {
        return false;
    }
    if !building_footprint_clears_speed_roads(center.x, center.z, width, depth) {
        return false;
    }

    let rotation = Quat::from_rotation_y(yaw);
    let base_y = center.y - height * 0.5;
    let base = Vec3::new(center.x, base_y, center.z);
    let wall = 0.52;
    let door_width = 3.8_f32.min(width - 3.0);
    let door_height = 3.8_f32.min(height - 1.0);
    let front_segment = ((width - door_width) * 0.5).max(1.0);
    let (floor_count, story_height) = enterable_floor_layout(height);
    let access = city_access_plan(width, height, floor_count, story_height);
    let interior_kind = building_interior_kind(zone, seed);

    commands.spawn((
        Transform::from_translation(base).with_rotation(rotation),
        GlobalTransform::default(),
        WorldGeometry,
        Building { zone, height },
        EnterableBuilding {
            accessible_floors: floor_count as u8,
            footprint: Vec2::new(width, depth),
        },
        interior_kind,
        Name::new(format!("Explorable {:?} Building", zone)),
    ));

    // Three full walls and a banded front form a hollow shell. Both the ground
    // door and the upper balcony door are true openings through the collider.
    spawn_enterable_piece(
        commands,
        meshes,
        exterior.clone(),
        base,
        rotation,
        Vec3::new(0.0, height * 0.5, depth * 0.5),
        Vec3::new(width, height, wall),
        false,
    );
    for side in [-1.0_f32, 1.0] {
        spawn_enterable_piece(
            commands,
            meshes,
            exterior.clone(),
            base,
            rotation,
            Vec3::new(side * width * 0.5, height * 0.5, 0.0),
            Vec3::new(wall, height, depth),
            false,
        );
    }
    let facade_story_count = floor_count;
    for floor in 0..facade_story_count {
        let bottom = floor as f32 * story_height;
        let top = ((floor + 1) as f32 * story_height).min(height);
        let band_height = top - bottom;
        let has_door = floor == 0 || floor == access.balcony_floor;
        if has_door {
            for side in [-1.0_f32, 1.0] {
                spawn_enterable_piece(
                    commands,
                    meshes,
                    exterior.clone(),
                    base,
                    rotation,
                    Vec3::new(
                        side * (door_width * 0.5 + front_segment * 0.5),
                        bottom + door_height * 0.5,
                        -depth * 0.5,
                    ),
                    Vec3::new(front_segment, door_height, wall),
                    false,
                );
            }
            let header_height = band_height - door_height;
            if header_height > 0.2 {
                spawn_enterable_piece(
                    commands,
                    meshes,
                    exterior.clone(),
                    base,
                    rotation,
                    Vec3::new(
                        0.0,
                        bottom + door_height + header_height * 0.5,
                        -depth * 0.5,
                    ),
                    Vec3::new(width, header_height, wall),
                    false,
                );
            }
        } else {
            spawn_enterable_piece(
                commands,
                meshes,
                exterior.clone(),
                base,
                rotation,
                Vec3::new(0.0, bottom + band_height * 0.5, -depth * 0.5),
                Vec3::new(width, band_height, wall),
                false,
            );
        }
    }
    let facade_top = facade_story_count as f32 * story_height;
    if facade_top < height {
        spawn_enterable_piece(
            commands,
            meshes,
            exterior.clone(),
            base,
            rotation,
            Vec3::new(0.0, facade_top + (height - facade_top) * 0.5, -depth * 0.5),
            Vec3::new(width, height - facade_top, wall),
            false,
        );
    }

    let opening_width = (width * 0.27).clamp(3.2, 4.6);
    let opening_depth = (depth - 3.0).max(7.0);
    let opening_x = (width * 0.22).min(width * 0.5 - opening_width * 0.5 - 0.9);
    let opening_left = opening_x - opening_width * 0.5;
    let opening_right = opening_x + opening_width * 0.5;
    let left_width = opening_left + width * 0.5;
    let right_width = width * 0.5 - opening_right;
    let end_depth = ((depth - opening_depth) * 0.5).max(0.8);

    // Each floor is built around the stairwell rather than as one solid slab.
    for floor in 0..floor_count {
        let y = 0.16 + floor as f32 * story_height;
        for (x, z, size) in [
            (
                -width * 0.5 + left_width * 0.5,
                0.0,
                Vec3::new(left_width, 0.32, depth - wall * 2.0),
            ),
            (
                opening_right + right_width * 0.5,
                0.0,
                Vec3::new(right_width, 0.32, depth - wall * 2.0),
            ),
            (
                opening_x,
                -depth * 0.5 + end_depth * 0.5,
                Vec3::new(opening_width, 0.32, end_depth),
            ),
            (
                opening_x,
                depth * 0.5 - end_depth * 0.5,
                Vec3::new(opening_width, 0.32, end_depth),
            ),
        ] {
            if size.x > 0.25 && size.z > 0.25 {
                spawn_enterable_piece(
                    commands,
                    meshes,
                    pal.interior_floor.clone(),
                    base,
                    rotation,
                    Vec3::new(x, y, z),
                    size,
                    true,
                );
            }
        }

        // Two room wings with a generous central doorway preserve navigation
        // for four players while making each level read as actual rooms.
        let partition_z = if floor.is_multiple_of(2) {
            depth * 0.23
        } else {
            -depth * 0.20
        };
        let room_door_x = -width * 0.20;
        let room_door_width = 2.8;
        let partition_height = (story_height - 0.55).max(3.3);
        let left_end = room_door_x - room_door_width * 0.5;
        let right_start = room_door_x + room_door_width * 0.5;
        for (x, segment_width) in [
            ((-width * 0.5 + left_end) * 0.5, left_end + width * 0.5),
            (
                (right_start + opening_left) * 0.5,
                opening_left - right_start,
            ),
        ] {
            if segment_width > 0.5 {
                spawn_enterable_piece(
                    commands,
                    meshes,
                    pal.interior_wall.clone(),
                    base,
                    rotation,
                    Vec3::new(x, y + 0.16 + partition_height * 0.5, partition_z),
                    Vec3::new(segment_width, partition_height, 0.28),
                    false,
                );
            }
        }

        // Sparse furniture gives the rooms scale without turning their floors
        // into a field of movement-blocking collision bumps.
        for side in [-1.0_f32, 1.0] {
            let local = Vec3::new(
                -width * 0.28,
                y + 0.58,
                side * depth * (0.29 + seeded(seed, floor as u64 * 7 + 2) * 0.04),
            );
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(2.4, 0.82, 0.9))),
                    material: MeshMaterial3d(pal.interior_trim.clone()),
                    transform: Transform::from_translation(base + rotation * local)
                        .with_rotation(rotation),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        if floor == 0 || (floor as u64 + seed).is_multiple_of(3) {
            spawn_building_interior_dressing(
                commands,
                meshes,
                pal,
                base,
                rotation,
                width,
                depth,
                y,
                interior_kind,
                seed + floor as u64 * 19,
            );
        }

        if floor.is_multiple_of(2) {
            commands.spawn((
                PointLightBundle {
                    point_light: PointLight {
                        color: if floor.is_multiple_of(4) {
                            Color::srgb(1.0, 0.78, 0.45)
                        } else {
                            Color::srgb(0.48, 0.82, 1.0)
                        },
                        intensity: 5_500.0,
                        range: width.max(depth) * 1.25,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    transform: Transform::from_translation(
                        base + rotation * Vec3::new(0.0, y + story_height * 0.72, 0.0),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    // One smooth collider per flight avoids the tiny step collisions that
    // previously robbed speed. Thin non-colliding treads provide stair detail.
    let stair_run = opening_depth - 1.1;
    let stair_length = (stair_run * stair_run + story_height * story_height).sqrt();
    let stair_angle = (story_height / stair_run).atan();
    for upper_floor in 1..floor_count {
        let low_z = if upper_floor.is_multiple_of(2) {
            stair_run * 0.5
        } else {
            -stair_run * 0.5
        };
        let high_z = -low_z;
        let pitch = if high_z > low_z {
            -stair_angle
        } else {
            stair_angle
        };
        let stair_center = Vec3::new(
            opening_x,
            (upper_floor as f32 - 0.5) * story_height + 0.24,
            0.0,
        );
        let stair_rotation = rotation * Quat::from_rotation_x(pitch);
        let mut stair = commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(opening_width - 0.45, 0.34, stair_length))),
                material: MeshMaterial3d(pal.interior_trim.clone()),
                transform: Transform::from_translation(base + rotation * stair_center)
                    .with_rotation(stair_rotation),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                (opening_width - 0.45) * 0.5,
                0.17,
                stair_length * 0.5,
            ),
        ));
        stair.insert(Name::new("Smooth Interior Stair Flight"));

        for tread in 0..8 {
            let t = (tread as f32 + 0.5) / 8.0;
            let z = low_z + (high_z - low_z) * t;
            let y = (upper_floor as f32 - 1.0) * story_height + story_height * t + 0.36;
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(
                        opening_width - 0.28,
                        0.08,
                        stair_run / 8.0 * 0.72,
                    ))),
                    material: MeshMaterial3d(pal.brushed_metal.clone()),
                    transform: Transform::from_translation(
                        base + rotation * Vec3::new(opening_x, y, z),
                    )
                    .with_rotation(rotation),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    spawn_enterable_roof_with_hatch(
        commands,
        meshes,
        exterior.clone(),
        pal,
        base,
        rotation,
        width,
        height,
        depth,
        opening_x,
        opening_width,
    );

    // Exterior frame bands break up large cuboid silhouettes and visibly use
    // the textured facade material without sealing the doorway or windows.
    for floor in 1..floor_count {
        let y = floor as f32 * story_height - 0.16;
        for z in [-depth * 0.5 - 0.05, depth * 0.5 + 0.05] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(width + 0.32, 0.18, 0.16))),
                    material: MeshMaterial3d(pal.city_metal_panel.clone()),
                    transform: Transform::from_translation(base + rotation * Vec3::new(0.0, y, z))
                        .with_rotation(rotation),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    for floor in 0..floor_count {
        let y = floor as f32 * story_height + story_height * 0.58;
        let window_material = if (floor as u64 + seed).is_multiple_of(2) {
            pal.window_warm.clone()
        } else {
            pal.window_cool.clone()
        };
        for side in [-1.0_f32, 1.0] {
            // Back windows and upper front windows leave the entrance visually open.
            for z in [depth * 0.5 + 0.055, -depth * 0.5 - 0.055] {
                if floor == 0 && z < 0.0 {
                    continue;
                }
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Cuboid::new(2.2, 1.45, 0.09))),
                        material: MeshMaterial3d(window_material.clone()),
                        transform: Transform::from_translation(
                            base + rotation * Vec3::new(side * width * 0.27, y, z),
                        )
                        .with_rotation(rotation),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.09, 1.45, 2.2))),
                    material: MeshMaterial3d(window_material.clone()),
                    transform: Transform::from_translation(
                        base + rotation * Vec3::new(side * (width * 0.5 + 0.055), y, 0.0),
                    )
                    .with_rotation(rotation),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    spawn_enterable_city_access(
        commands, meshes, pal, base, rotation, width, height, depth, access, seed,
    );
    spawn_building_lift(
        commands, meshes, pal, base, rotation, width, height, depth, seed,
    );
    spawn_building_exploration_reward(
        commands,
        meshes,
        pal,
        base,
        rotation,
        width,
        height,
        depth,
        floor_count,
        story_height,
        zone,
        seed,
    );

    true
}

#[allow(clippy::too_many_arguments)]
fn spawn_building_interior_dressing(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    base: Vec3,
    rotation: Quat,
    width: f32,
    depth: f32,
    floor_y: f32,
    kind: BuildingInteriorKind,
    seed: u64,
) {
    let accent = city_accent_material(pal, seed);
    let x = (-width * 0.27).clamp(-6.0, -3.2);
    let z = (depth * 0.17).clamp(1.6, 4.2);
    let mut spawn_prop =
        |local: Vec3, size: Vec3, material: Handle<StandardMaterial>, name: &'static str| {
            commands.spawn((
                Name::new(name),
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::from_size(size))),
                    material: MeshMaterial3d(material),
                    transform: Transform::from_translation(base + rotation * local)
                        .with_rotation(rotation),
                    ..default()
                },
                WorldGeometry,
            ));
        };

    match kind {
        BuildingInteriorKind::Lobby => {
            spawn_prop(
                Vec3::new(x, floor_y + 0.68, z),
                Vec3::new(3.4, 1.05, 1.15),
                accent.clone(),
                "Lobby Welcome Desk",
            );
            spawn_prop(
                Vec3::new(x - 2.7, floor_y + 0.34, z + 1.8),
                Vec3::new(1.5, 0.42, 2.8),
                pal.city_mint.clone(),
                "Lobby Lounge Seat",
            );
        }
        BuildingInteriorKind::Market => {
            for side in [-1.0_f32, 1.0] {
                spawn_prop(
                    Vec3::new(x + side * 2.0, floor_y + 0.72, z),
                    Vec3::new(2.4, 1.15, 1.45),
                    if side < 0.0 {
                        pal.city_sun.clone()
                    } else {
                        pal.city_coral.clone()
                    },
                    "Market Display Stall",
                );
            }
        }
        BuildingInteriorKind::Home => {
            spawn_prop(
                Vec3::new(x, floor_y + 0.43, z),
                Vec3::new(3.0, 0.52, 1.75),
                pal.interior_trim.clone(),
                "Apartment Daybed",
            );
            spawn_prop(
                Vec3::new(x - 2.25, floor_y + 0.45, z - 1.4),
                Vec3::new(1.15, 0.62, 1.15),
                accent.clone(),
                "Apartment Side Table",
            );
        }
        BuildingInteriorKind::Laboratory => {
            spawn_prop(
                Vec3::new(x, floor_y + 0.74, z),
                Vec3::new(3.2, 1.22, 1.0),
                pal.city_aqua.clone(),
                "Laboratory Console",
            );
            for side in [-1.0_f32, 1.0] {
                spawn_prop(
                    Vec3::new(x + side * 2.15, floor_y + 0.72, z + 1.55),
                    Vec3::new(0.72, 1.12, 0.72),
                    if side < 0.0 {
                        pal.city_lilac.clone()
                    } else {
                        pal.city_mint.clone()
                    },
                    "Laboratory Sample Pod",
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_enterable_roof_with_hatch(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    exterior: Handle<StandardMaterial>,
    pal: &Palette,
    base: Vec3,
    rotation: Quat,
    width: f32,
    height: f32,
    depth: f32,
    hatch_x: f32,
    opening_width: f32,
) {
    let hatch_size = building_roof_hatch_size(depth, opening_width);
    let hatch_width = hatch_size.x;
    let hatch_depth = hatch_size.y;
    let hatch_left = hatch_x - hatch_width * 0.5;
    let hatch_right = hatch_x + hatch_width * 0.5;
    let left_width = hatch_left + width * 0.5;
    let right_width = width * 0.5 - hatch_right;
    let end_depth = (depth - hatch_depth) * 0.5;
    for (x, z, size) in [
        (
            -width * 0.5 + left_width * 0.5,
            0.0,
            Vec3::new(left_width, 0.46, depth),
        ),
        (
            hatch_right + right_width * 0.5,
            0.0,
            Vec3::new(right_width, 0.46, depth),
        ),
        (
            hatch_x,
            -depth * 0.5 + end_depth * 0.5,
            Vec3::new(hatch_width, 0.46, end_depth),
        ),
        (
            hatch_x,
            depth * 0.5 - end_depth * 0.5,
            Vec3::new(hatch_width, 0.46, end_depth),
        ),
    ] {
        if size.x > 0.2 && size.z > 0.2 {
            spawn_enterable_piece(
                commands,
                meshes,
                exterior.clone(),
                base,
                rotation,
                Vec3::new(x, height, z),
                size,
                true,
            );
        }
    }

    for x in [hatch_x - hatch_width * 0.5, hatch_x + hatch_width * 0.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.18, 0.42, hatch_depth + 0.35))),
                material: MeshMaterial3d(pal.city_sun.clone()),
                transform: Transform::from_translation(
                    base + rotation * Vec3::new(x, height + 0.34, 0.0),
                )
                .with_rotation(rotation),
                ..default()
            },
            BuildingRoofHatch,
            WorldGeometry,
        ));
    }
    for z in [-hatch_depth * 0.5, hatch_depth * 0.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(hatch_width + 0.35, 0.42, 0.18))),
                material: MeshMaterial3d(pal.city_aqua.clone()),
                transform: Transform::from_translation(
                    base + rotation * Vec3::new(hatch_x, height + 0.34, z),
                )
                .with_rotation(rotation),
                ..default()
            },
            BuildingRoofHatch,
            WorldGeometry,
        ));
    }
    for rung in 0..5 {
        let local = Vec3::new(
            hatch_x,
            height - 0.55 - rung as f32 * 0.72,
            hatch_depth * 0.5 - 0.16,
        );
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(hatch_width * 0.72, 0.11, 0.16))),
                material: MeshMaterial3d(pal.city_coral.clone()),
                transform: Transform::from_translation(base + rotation * local)
                    .with_rotation(rotation),
                ..default()
            },
            BuildingRoofHatch,
            WorldGeometry,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_building_lift(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    base: Vec3,
    rotation: Quat,
    width: f32,
    height: f32,
    depth: f32,
    seed: u64,
) {
    let local_x = -width * 0.5 - 2.15;
    let local_z = (seeded(seed, 211) - 0.5) * depth * 0.28;
    let platform_size = Vec3::new(3.2, 0.38, 3.4);
    let start = base + rotation * Vec3::new(local_x, 0.48, local_z);
    let end = base + rotation * Vec3::new(local_x, height + 0.48, local_z);
    commands.spawn((
        Name::new("Exterior Building Lift"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::from_size(platform_size))),
            material: MeshMaterial3d(city_accent_material(pal, seed + 5)),
            transform: Transform::from_translation(start).with_rotation(rotation),
            ..default()
        },
        BuildingLift,
        MovingPlatform {
            start,
            end,
            speed: 4.4,
            phase: seeded(seed, 223) * TAU,
            size: platform_size,
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::KinematicPositionBased,
        crate::engine::physics::prelude::Collider::cuboid(
            platform_size.x * 0.5,
            platform_size.y * 0.5,
            platform_size.z * 0.5,
        ),
    ));

    for z_side in [-1.0_f32, 1.0] {
        let local = Vec3::new(local_x, height * 0.5, local_z + z_side * 1.52);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.16, height + 0.8, 0.16))),
                material: MeshMaterial3d(pal.city_lilac.clone()),
                transform: Transform::from_translation(base + rotation * local)
                    .with_rotation(rotation),
                ..default()
            },
            BuildingLift,
            WorldGeometry,
        ));
    }
    for landing_y in [0.12, height + 0.16] {
        spawn_enterable_piece(
            commands,
            meshes,
            pal.city_aqua.clone(),
            base,
            rotation,
            Vec3::new(-width * 0.5 - 0.78, landing_y, local_z),
            Vec3::new(2.7, 0.32, 3.4),
            true,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_building_exploration_reward(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    base: Vec3,
    rotation: Quat,
    width: f32,
    height: f32,
    depth: f32,
    floor_count: usize,
    story_height: f32,
    zone: WorldZone,
    seed: u64,
) {
    let plan = building_reward_plan(zone, seed, width, height, depth, floor_count, story_height);
    let reward_key = building_reward_key(zone, base);
    let reward_position = base + rotation * plan.local_position;
    let material = match plan.location {
        BuildingRewardLocation::Interior => pal.city_mint.clone(),
        BuildingRewardLocation::Rooftop => pal.city_sun.clone(),
    };
    let reward_entity = commands
        .spawn((
            Name::new(match plan.location {
                BuildingRewardLocation::Interior => "Interior Exploration Cache",
                BuildingRewardLocation::Rooftop => "Rooftop Star Cache",
            }),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.72))),
                material: MeshMaterial3d(material.clone()),
                transform: Transform::from_translation(reward_position),
                ..default()
            },
            BuildingExplorationReward {
                reward_key: reward_key.clone(),
                location: plan.location,
                credits: plan.credits,
                experience: plan.experience,
                armor: plan.armor,
                pickup_radius: 2.4,
                base_y: reward_position.y,
                bob_phase: seeded(seed, 271) * TAU,
            },
            WorldGeometry,
        ))
        .id();
    let ring_mesh = meshes.add(Torus::new(0.82, 1.06));
    commands.entity(reward_entity).with_children(|reward| {
        reward.spawn((
            PbrBundle {
                mesh: Mesh3d(ring_mesh),
                material: MeshMaterial3d(material.clone()),
                transform: Transform::from_rotation(Quat::from_rotation_x(
                    std::f32::consts::FRAC_PI_2,
                )),
                ..default()
            },
            WorldGeometry,
        ));
    });

    // A slim route beacon above the ground entrance tells street-level players
    // whether this explorable building still contains a cache.
    let beacon_position = base + rotation * Vec3::new(0.0, 5.2, -depth * 0.5 - 0.5);
    commands.spawn((
        Name::new("Building Exploration Beacon"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Capsule3d::new(0.18, 1.4))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(beacon_position),
            ..default()
        },
        BuildingRewardBeacon { reward_key },
        WorldGeometry,
    ));
}

fn city_accent_material(pal: &Palette, seed: u64) -> Handle<StandardMaterial> {
    match seed % 5 {
        0 => pal.city_coral.clone(),
        1 => pal.city_sun.clone(),
        2 => pal.city_mint.clone(),
        3 => pal.city_lilac.clone(),
        _ => pal.city_aqua.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_enterable_city_access(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    base: Vec3,
    rotation: Quat,
    width: f32,
    height: f32,
    depth: f32,
    access: CityAccessPlan,
    seed: u64,
) {
    let accent = city_accent_material(pal, seed);
    let trim = city_accent_material(pal, seed + 2);
    let front_z = -depth * 0.5;
    let balcony_depth = 2.8;
    let balcony_z = front_z - balcony_depth * 0.5 + 0.12;

    // Candy-colored portal frames make both true openings readable at speed.
    for (door_y, kind) in [
        (0.0, CityAccessKind::GroundEntrance),
        (access.balcony_y, CityAccessKind::BalconyEntrance),
    ] {
        for side in [-1.0_f32, 1.0] {
            let local = Vec3::new(side * 2.08, door_y + 1.95, front_z - 0.34);
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.34, 3.9, 0.34))),
                    material: MeshMaterial3d(accent.clone()),
                    transform: Transform::from_translation(base + rotation * local)
                        .with_rotation(rotation),
                    ..default()
                },
                WorldGeometry,
                kind,
            ));
        }
        let local = Vec3::new(0.0, door_y + 3.9, front_z - 0.34);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(4.5, 0.34, 0.34))),
                material: MeshMaterial3d(trim.clone()),
                transform: Transform::from_translation(base + rotation * local)
                    .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
            kind,
        ));
    }

    // The balcony aligns exactly with the opened facade band and interior floor.
    let balcony_size = Vec3::new(access.balcony_width, 0.34, balcony_depth);
    let balcony_local = Vec3::new(0.0, access.balcony_y, balcony_z);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::from_size(balcony_size))),
            material: MeshMaterial3d(accent.clone()),
            transform: Transform::from_translation(base + rotation * balcony_local)
                .with_rotation(rotation),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        CityAccessKind::Balcony,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            balcony_size.x * 0.5,
            balcony_size.y * 0.5,
            balcony_size.z * 0.5,
        ),
    ));

    // Low rails preserve the playful open-air route without hiding its doorway.
    for side in [-1.0_f32, 1.0] {
        let local =
            balcony_local + Vec3::new(side * (access.balcony_width * 0.5 - 0.12), 0.72, 0.0);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.18, 1.25, balcony_depth))),
                material: MeshMaterial3d(trim.clone()),
                transform: Transform::from_translation(base + rotation * local)
                    .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
        ));
    }

    let low_z = front_z - access.stair_run - balcony_depth + 0.4;
    let high_z = balcony_z - balcony_depth * 0.32;
    let run = high_z - low_z;
    let stair_end_world = base + rotation * Vec3::new(0.0, 0.0, low_z);
    let road_clearance =
        distance_to_speed_road_network(stair_end_world.x, stair_end_world.z, road_network_seed());
    if road_clearance > SPEED_ROAD_WIDTH * 0.5 + 2.5 {
        let stair_length = (run * run + access.balcony_y * access.balcony_y).sqrt();
        let pitch = -(access.balcony_y / run).atan();
        let stair_center = Vec3::new(0.0, access.balcony_y * 0.5, (low_z + high_z) * 0.5);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(
                    (access.balcony_width - 1.2).min(5.4),
                    0.34,
                    stair_length,
                ))),
                material: MeshMaterial3d(pal.city_aqua.clone()),
                transform: Transform::from_translation(base + rotation * stair_center)
                    .with_rotation(rotation * Quat::from_rotation_x(pitch)),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            CityAccessKind::ExteriorStair,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                ((access.balcony_width - 1.2).min(5.4)) * 0.5,
                0.17,
                stair_length * 0.5,
            ),
        ));
        for tread in 0..10u32 {
            let t = (tread as f32 + 0.5) / 10.0;
            let local = Vec3::new(0.0, access.balcony_y * t + 0.26, low_z + run * t);
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(
                        (access.balcony_width - 0.8).min(5.8),
                        0.08,
                        run / 10.0 * 0.72,
                    ))),
                    material: MeshMaterial3d(if tread.is_multiple_of(2) {
                        accent.clone()
                    } else {
                        trim.clone()
                    }),
                    transform: Transform::from_translation(base + rotation * local)
                        .with_rotation(rotation),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    spawn_city_roof_route_and_crown(
        commands,
        meshes,
        pal,
        base,
        rotation,
        width,
        height,
        depth,
        access.ladder_rungs,
        seed,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_city_roof_route_and_crown(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    base: Vec3,
    rotation: Quat,
    width: f32,
    height: f32,
    depth: f32,
    ladder_rungs: usize,
    seed: u64,
) {
    let accent = city_accent_material(pal, seed + 1);
    let ladder_x = width * 0.5 + 0.30;
    let ladder_z = (seeded(seed, 77) - 0.5) * depth * 0.35;
    for rung in 0..ladder_rungs {
        let y = 0.75 + rung as f32 * 1.25;
        if y >= height + 0.2 {
            break;
        }
        let local = Vec3::new(ladder_x, y, ladder_z);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.18, 0.13, 2.1))),
                material: MeshMaterial3d(accent.clone()),
                transform: Transform::from_translation(base + rotation * local)
                    .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
            CityAccessKind::Ladder,
        ));
    }
    for side in [-1.0_f32, 1.0] {
        let local = Vec3::new(ladder_x, height * 0.5, ladder_z + side * 0.92);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.16, height, 0.16))),
                material: MeshMaterial3d(pal.city_sun.clone()),
                transform: Transform::from_translation(base + rotation * local)
                    .with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
            CityAccessKind::Ladder,
        ));
    }

    let landing_size = Vec3::new(2.8, 0.32, 3.4);
    let landing_local = Vec3::new(width * 0.5 + 1.15, height + 0.12, ladder_z);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::from_size(landing_size))),
            material: MeshMaterial3d(accent.clone()),
            transform: Transform::from_translation(base + rotation * landing_local)
                .with_rotation(rotation),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        CityAccessKind::RoofLanding,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(
            landing_size.x * 0.5,
            landing_size.y * 0.5,
            landing_size.z * 0.5,
        ),
    ));

    // A sun-disc and tapered fantasy spire give the skyline its optimistic
    // seventies storybook-sci-fi silhouette without compromising the roof.
    let crown_x = (seeded(seed, 81) - 0.5) * width * 0.28;
    let crown_z = (seeded(seed, 82) - 0.5) * depth * 0.28;
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(1.3, 2.4))),
            material: MeshMaterial3d(pal.city_lilac.clone()),
            transform: Transform::from_translation(
                base + rotation * Vec3::new(crown_x, height + 1.2, crown_z),
            ),
            ..default()
        },
        WorldGeometry,
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cone::new(2.8, 3.2))),
            material: MeshMaterial3d(accent),
            transform: Transform::from_translation(
                base + rotation * Vec3::new(crown_x, height + 4.0, crown_z),
            ),
            ..default()
        },
        WorldGeometry,
    ));
}

fn spawn_building(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mat: Handle<StandardMaterial>,
    position: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    zone: WorldZone,
) {
    let seed = road_network_seed();
    let road_dist = distance_to_speed_road_network(position.x, position.z, seed);
    let half_diag = Vec2::new(width, depth).length() * 0.5;

    // The legacy field encodes every road against the 256-unit trunk width,
    // so it cannot add a small building's footprint margin around Star City's
    // compact elevated ring/ramps. Keep those silhouettes fully off the deck
    // rather than generating a ground-level tunnel through an aerial merge.
    if !building_footprint_clears_star_city(position.x, position.z, half_diag) {
        return;
    }

    if road_dist < half_diag.max(SPEED_ROAD_WIDTH * 0.5) {
        // Road runs through this footprint. Estimate the road tangent from the
        // distance-field gradient (tangent is perpendicular to gradient) so the
        // gateway opening lines up with the route.
        let e = 6.0;
        let gx = distance_to_speed_road_network(position.x + e, position.z, seed)
            - distance_to_speed_road_network(position.x - e, position.z, seed);
        let gz = distance_to_speed_road_network(position.x, position.z + e, seed)
            - distance_to_speed_road_network(position.x, position.z - e, seed);
        let grad = Vec2::new(gx, gz).normalize_or_zero();

        let across = width.max(depth);
        let too_small = across < BUILDING_TUNNEL_PASSAGE + 12.0
            || height < BUILDING_TUNNEL_CLEARANCE + 4.0
            || grad.length_squared() < 0.5;
        if too_small {
            // Can't straddle the road — the building yields to the route.
            return;
        }

        // Local X axis along the gradient (across the road).
        let yaw = (-grad.x).atan2(grad.y);
        let rot = Quat::from_rotation_y(yaw);
        let ground_y = position.y - height * 0.5;
        let along = width.min(depth).max(14.0);
        let flank_w = (across - BUILDING_TUNNEL_PASSAGE) * 0.5;
        let flank_center = BUILDING_TUNNEL_PASSAGE * 0.5 + flank_w * 0.5;

        // Two flank towers on either side of the road.
        for side in [-1.0f32, 1.0] {
            let offset = rot * Vec3::new(side * flank_center, 0.0, 0.0);
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(flank_w, height, along))),
                    material: MeshMaterial3d(mat.clone()),
                    transform: Transform::from_translation(position + offset).with_rotation(rot),
                    ..default()
                },
                WorldGeometry,
                WalkableSurface,
                Building { zone, height },
                crate::engine::spatial_lod::SpatialLod::landmark(height),
                ProceduralMaterialBinding::new("building.world", "exterior"),
                crate::engine::physics::prelude::RigidBody::Fixed,
                crate::engine::physics::prelude::Collider::cuboid(
                    flank_w * 0.5,
                    height * 0.5,
                    along * 0.5,
                ),
            ));
        }

        // Header deck bridging the passage above vehicle clearance — the
        // tunnel through the building.
        let header_h = height - BUILDING_TUNNEL_CLEARANCE;
        if header_h > 1.0 {
            let header_w = BUILDING_TUNNEL_PASSAGE + flank_w * 0.2;
            let header_y = ground_y + BUILDING_TUNNEL_CLEARANCE + header_h * 0.5;
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(header_w, header_h, along))),
                    material: MeshMaterial3d(mat),
                    transform: Transform::from_xyz(position.x, header_y, position.z)
                        .with_rotation(rot),
                    ..default()
                },
                WorldGeometry,
                WalkableSurface,
                Building {
                    zone,
                    height: header_h,
                },
                crate::engine::spatial_lod::SpatialLod::landmark(height),
                ProceduralMaterialBinding::new("building.world", "exterior"),
                crate::engine::physics::prelude::RigidBody::Fixed,
                crate::engine::physics::prelude::Collider::cuboid(
                    header_w * 0.5,
                    header_h * 0.5,
                    along * 0.5,
                ),
            ));
        }
        return;
    }

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width, height, depth))),
            material: MeshMaterial3d(mat),
            transform: Transform::from_translation(position),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Building { zone, height },
        crate::engine::spatial_lod::SpatialLod::landmark(height),
        ProceduralMaterialBinding::new("building.world", "exterior"),
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(width * 0.5, height * 0.5, depth * 0.5),
    ));
}

fn spawn_small_building_details(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    position: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    seed: u64,
    stone_variant: bool,
) {
    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let ground_y = position.y - height * 0.5;
    let top_y = position.y + height * 0.5;
    let trim = if stone_variant {
        pal.stone_block.clone()
    } else {
        pal.brick_line.clone()
    };
    let course_line = if stone_variant {
        pal.mortar_line.clone()
    } else {
        pal.brick_line.clone()
    };

    // Stone plinth and roof cap keep the smaller buildings grounded and tactile.
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(unit_cube.clone()),
            material: MeshMaterial3d(pal.stone_block.clone()),
            transform: unit_cuboid_transform(
                Vec3::new(position.x, ground_y + 0.21, position.z),
                Quat::IDENTITY,
                Vec3::new(width + 0.45, 0.42, depth + 0.45),
            ),
            ..default()
        },
        WorldGeometry,
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(unit_cube.clone()),
            material: MeshMaterial3d(trim.clone()),
            transform: unit_cuboid_transform(
                Vec3::new(position.x, top_y + 0.17, position.z),
                Quat::IDENTITY,
                Vec3::new(width + 0.65, 0.34, depth + 0.65),
            ),
            ..default()
        },
        WorldGeometry,
    ));

    if seeded(seed, 91) > 0.45 {
        let moss_transform = unit_cuboid_transform(
            Vec3::new(position.x, top_y + 0.42, position.z),
            Quat::IDENTITY,
            Vec3::new(width * 0.62, 0.16, depth * 0.52),
        );
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(unit_cube.clone()),
                material: MeshMaterial3d(pal.moss.clone()),
                transform: moss_transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(
                &moss_transform,
                seeded(seed, 92) * std::f32::consts::TAU,
                1.2,
                0.004,
                0.015,
                0.015,
            ),
        ));
    }

    let row_count = ((height - 2.0) / 3.2).floor().clamp(1.0, 9.0) as i32;
    for row in 1..=row_count {
        let y = ground_y + row as f32 * height / (row_count as f32 + 1.0);
        let row_h = if stone_variant { 0.10 } else { 0.045 };
        for &z in &[
            position.z - depth * 0.5 - 0.055,
            position.z + depth * 0.5 + 0.055,
        ] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(course_line.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(position.x, y, z),
                        Quat::IDENTITY,
                        Vec3::new(width + 0.10, row_h, 0.09),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        for &x in &[
            position.x - width * 0.5 - 0.055,
            position.x + width * 0.5 + 0.055,
        ] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(course_line.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(x, y, position.z),
                        Quat::IDENTITY,
                        Vec3::new(0.09, row_h, depth + 0.10),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    // Corner blocks read as stone quoins from a distance.
    for &x in &[
        position.x - width * 0.5 - 0.08,
        position.x + width * 0.5 + 0.08,
    ] {
        for &z in &[
            position.z - depth * 0.5 - 0.08,
            position.z + depth * 0.5 + 0.08,
        ] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.stone_block.clone()),
                    transform: unit_cuboid_transform(
                        Vec3::new(x, ground_y + height * 0.44, z),
                        Quat::IDENTITY,
                        Vec3::new(0.22, height * 0.88, 0.22),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    if stone_variant {
        spawn_stone_brick_courses_cached(
            commands, pal, &unit_cube, position, width, height, depth, false,
        );
    }

    let floors = ((height - 3.0) / 4.6).floor().clamp(1.0, 5.0) as i32;
    let front_cols = ((width - 2.0) / 4.0).floor().clamp(1.0, 4.0) as i32;
    let side_cols = ((depth - 2.0) / 4.0).floor().clamp(1.0, 4.0) as i32;
    let win_w = 1.15;
    let win_h = 1.55;

    for floor in 0..floors {
        let y = ground_y + 2.7 + floor as f32 * 4.4;
        if y > top_y - 1.0 {
            continue;
        }

        for col in 0..front_cols {
            let x = position.x + (col as f32 - (front_cols - 1) as f32 * 0.5) * 3.6;
            let mat = small_window_material(pal, seed + floor as u64 * 11 + col as u64);
            for &z in &[
                position.z - depth * 0.5 - 0.12,
                position.z + depth * 0.5 + 0.12,
            ] {
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(unit_cube.clone()),
                        material: MeshMaterial3d(mat.clone()),
                        transform: unit_cuboid_transform(
                            Vec3::new(x, y, z),
                            Quat::IDENTITY,
                            Vec3::new(win_w, win_h, 0.12),
                        ),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }

        for col in 0..side_cols {
            let z = position.z + (col as f32 - (side_cols - 1) as f32 * 0.5) * 3.6;
            let mat = small_window_material(pal, seed + 200 + floor as u64 * 13 + col as u64);
            for &x in &[
                position.x - width * 0.5 - 0.12,
                position.x + width * 0.5 + 0.12,
            ] {
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(unit_cube.clone()),
                        material: MeshMaterial3d(mat.clone()),
                        transform: unit_cuboid_transform(
                            Vec3::new(x, y, z),
                            Quat::IDENTITY,
                            Vec3::new(0.12, win_h, win_w),
                        ),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }
    }
}

fn small_window_material(pal: &Palette, seed: u64) -> Handle<StandardMaterial> {
    let r = seeded(seed, 17);
    if r < 0.18 {
        pal.small_window_dark.clone()
    } else if r < 0.58 {
        pal.small_window_warm.clone()
    } else {
        pal.small_window_cool.clone()
    }
}

// ── Tower helper ──────────────────────────────────────────────────────────────
fn spawn_tower(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    body_mat: Handle<StandardMaterial>,
    cap_mat: Handle<StandardMaterial>,
    base: Vec3,
    radius: f32,
    body_h: f32,
    cap_h: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(radius, body_h))),
            material: MeshMaterial3d(body_mat),
            transform: Transform::from_xyz(base.x, base.y + body_h * 0.5, base.z),
            ..default()
        },
        WorldGeometry,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(body_h * 0.5, radius),
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cone {
                radius: radius * 1.25,
                height: cap_h,
            })),
            material: MeshMaterial3d(cap_mat),
            transform: Transform::from_xyz(base.x, base.y + body_h + cap_h * 0.5, base.z),
            ..default()
        },
        WorldGeometry,
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_castle_block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    position: Vec3,
    size: Vec3,
    rotation: Quat,
    collider: bool,
    walkable: bool,
) {
    let entity = commands
        .spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(material),
                transform: Transform::from_translation(position).with_rotation(rotation),
                ..default()
            },
            WorldGeometry,
        ))
        .id();

    if collider {
        commands.entity(entity).insert((
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(
                size.x * 0.5,
                size.y * 0.5,
                size.z * 0.5,
            ),
        ));
    }
    if walkable {
        commands.entity(entity).insert(WalkableSurface);
    }
}

fn spawn_castle_window_frame(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    yaw: f32,
    width: f32,
    height: f32,
) {
    let rot = Quat::from_rotation_y(yaw);
    spawn_castle_block(
        commands,
        meshes,
        pal.castle_window.clone(),
        center,
        Vec3::new(width, height, 0.45),
        rot,
        false,
        false,
    );

    let trim_specs = [
        (
            Vec3::new(0.0, height * 0.5 + 0.34, 0.05),
            Vec3::new(width + 1.1, 0.42, 0.72),
        ),
        (
            Vec3::new(0.0, -height * 0.5 - 0.34, 0.05),
            Vec3::new(width + 1.1, 0.42, 0.72),
        ),
        (
            Vec3::new(-width * 0.5 - 0.34, 0.0, 0.05),
            Vec3::new(0.42, height + 1.0, 0.72),
        ),
        (
            Vec3::new(width * 0.5 + 0.34, 0.0, 0.05),
            Vec3::new(0.42, height + 1.0, 0.72),
        ),
    ];
    for (local, size) in trim_specs {
        spawn_castle_block(
            commands,
            meshes,
            pal.castle_trim.clone(),
            center + rot * local,
            size,
            rot,
            false,
            false,
        );
    }

    for i in -2..=2 {
        let x = i as f32 * (width + 1.2) / 4.0;
        spawn_castle_block(
            commands,
            meshes,
            pal.mortar_line.clone(),
            center + rot * Vec3::new(x, height * 0.5 + 0.92, 0.08),
            Vec3::new(0.58, 0.52, 0.76),
            rot,
            false,
            false,
        );
    }
}

fn spawn_wind_banner(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    position: Vec3,
    height: f32,
    phase: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(0.30, height))),
            material: MeshMaterial3d(pal.castle_trim.clone()),
            transform: Transform::from_xyz(position.x, position.y + height * 0.5, position.z),
            ..default()
        },
        WorldGeometry,
    ));

    let flag_transform =
        Transform::from_xyz(position.x + 3.2, position.y + height - 2.2, position.z)
            .with_scale(Vec3::new(1.0, 1.0, 1.0));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(6.4, 4.2, 0.20))),
            material: MeshMaterial3d(pal.aurora_banner.clone()),
            transform: flag_transform,
            ..default()
        },
        NatureSway::new(&flag_transform, phase, 2.4, 0.055, 0.05, 0.075),
        WorldGeometry,
    ));
}

fn spawn_castle_bridge_and_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    cx: f32,
    floor: f32,
    gate_z: f32,
) {
    spawn_castle_block(
        commands,
        meshes,
        pal.stone_brick.clone(),
        Vec3::new(cx, floor + 0.28, gate_z + 43.0),
        Vec3::new(18.0, 0.56, 82.0),
        Quat::IDENTITY,
        true,
        true,
    );
    for x in [-10.2_f32, 10.2] {
        spawn_castle_block(
            commands,
            meshes,
            pal.castle_trim.clone(),
            Vec3::new(cx + x, floor + 1.2, gate_z + 43.0),
            Vec3::new(0.7, 2.0, 82.0),
            Quat::IDENTITY,
            true,
            false,
        );
    }

    for i in 0..8 {
        let z = gate_z + 9.0 + i as f32 * 9.6;
        for x in [-7.7_f32, 7.7] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Sphere::new(0.72))),
                    material: MeshMaterial3d(pal.guide_glow.clone()),
                    transform: Transform::from_xyz(cx + x, floor + 2.55, z),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        if i % 2 == 0 {
            commands.spawn(PointLightBundle {
                point_light: PointLight {
                    color: Color::srgb(0.38, 0.96, 1.0),
                    intensity: 12_000.0,
                    range: 54.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(cx, floor + 6.6, z),
                ..default()
            });
        }
    }

    for x in [-5.2_f32, 5.2] {
        spawn_castle_block(
            commands,
            meshes,
            pal.rooftop.clone(),
            Vec3::new(cx + x, floor + 6.5, gate_z + 1.9),
            Vec3::new(4.9, 13.0, 0.75),
            Quat::from_rotation_y(if x < 0.0 { -0.58 } else { 0.58 }),
            true,
            false,
        );
    }

    for x in [-3.6_f32, -1.8, 0.0, 1.8, 3.6] {
        spawn_castle_block(
            commands,
            meshes,
            pal.brushed_metal.clone(),
            Vec3::new(cx + x, floor + 17.2, gate_z + 0.7),
            Vec3::new(0.28, 9.5, 0.32),
            Quat::IDENTITY,
            false,
            false,
        );
    }
    spawn_castle_block(
        commands,
        meshes,
        pal.castle_trim.clone(),
        Vec3::new(cx, floor + 22.6, gate_z + 0.7),
        Vec3::new(13.0, 0.75, 0.45),
        Quat::IDENTITY,
        false,
        false,
    );
}

fn spawn_aurora_keep_rooms(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    cx: f32,
    cz: f32,
    floor: f32,
) {
    let width = 54.0_f32;
    let depth = 42.0_f32;
    let wall_h = 18.0_f32;
    let wt = 2.2_f32;
    let roof_y = floor + wall_h + 1.0;

    spawn_castle_block(
        commands,
        meshes,
        pal.stone_brick.clone(),
        Vec3::new(cx, floor + 0.26, cz),
        Vec3::new(width + 2.0, 0.52, depth + 2.0),
        Quat::IDENTITY,
        true,
        true,
    );

    let north_z = cz - depth * 0.5;
    let south_z = cz + depth * 0.5;
    let west_x = cx - width * 0.5;
    let east_x = cx + width * 0.5;

    for (pos, size) in [
        (
            Vec3::new(cx, floor + wall_h * 0.5, north_z),
            Vec3::new(width, wall_h, wt),
        ),
        (
            Vec3::new(west_x, floor + wall_h * 0.5, cz),
            Vec3::new(wt, wall_h, depth),
        ),
        (
            Vec3::new(east_x, floor + wall_h * 0.5, cz),
            Vec3::new(wt, wall_h, depth),
        ),
        (
            Vec3::new(cx - 17.0, floor + wall_h * 0.5, south_z),
            Vec3::new(20.0, wall_h, wt),
        ),
        (
            Vec3::new(cx + 17.0, floor + wall_h * 0.5, south_z),
            Vec3::new(20.0, wall_h, wt),
        ),
    ] {
        spawn_castle_block(
            commands,
            meshes,
            pal.stone_brick.clone(),
            pos,
            size,
            Quat::IDENTITY,
            true,
            false,
        );
    }

    for (pos, size) in [
        (
            Vec3::new(cx - 12.0, floor + 3.5, cz - 5.8),
            Vec3::new(1.2, 7.0, 21.0),
        ),
        (
            Vec3::new(cx + 12.0, floor + 3.5, cz - 5.8),
            Vec3::new(1.2, 7.0, 21.0),
        ),
        (
            Vec3::new(cx - 19.5, floor + 3.5, cz + 6.5),
            Vec3::new(13.0, 7.0, 1.1),
        ),
        (
            Vec3::new(cx + 19.5, floor + 3.5, cz + 6.5),
            Vec3::new(13.0, 7.0, 1.1),
        ),
        (
            Vec3::new(cx, floor + 3.5, cz + 11.8),
            Vec3::new(14.0, 7.0, 1.1),
        ),
    ] {
        spawn_castle_block(
            commands,
            meshes,
            pal.downtown_facade.clone(),
            pos,
            size,
            Quat::IDENTITY,
            true,
            false,
        );
    }

    for (pos, size) in [
        (
            Vec3::new(cx - 18.0, floor + 8.2, cz - 10.0),
            Vec3::new(15.0, 0.55, 12.0),
        ),
        (
            Vec3::new(cx + 18.0, floor + 8.2, cz - 10.0),
            Vec3::new(15.0, 0.55, 12.0),
        ),
        (
            Vec3::new(cx, floor + 8.2, cz + 10.0),
            Vec3::new(18.0, 0.55, 10.0),
        ),
    ] {
        spawn_castle_block(
            commands,
            meshes,
            pal.castle_stone.clone(),
            pos,
            size,
            Quat::IDENTITY,
            true,
            true,
        );
    }

    for (x, z, yaw) in [
        (cx - 17.0, south_z + 1.18, 0.0),
        (cx + 17.0, south_z + 1.18, 0.0),
        (cx - 16.0, north_z - 1.18, std::f32::consts::PI),
        (cx + 16.0, north_z - 1.18, std::f32::consts::PI),
        (west_x - 1.18, cz - 7.0, std::f32::consts::FRAC_PI_2),
        (east_x + 1.18, cz - 7.0, std::f32::consts::FRAC_PI_2),
    ] {
        spawn_castle_window_frame(
            commands,
            meshes,
            pal,
            Vec3::new(x, floor + 11.0, z),
            yaw,
            3.0,
            4.8,
        );
    }

    spawn_castle_block(
        commands,
        meshes,
        pal.castle_trim.clone(),
        Vec3::new(cx, roof_y, north_z),
        Vec3::new(width + 1.6, 2.0, wt + 0.9),
        Quat::IDENTITY,
        false,
        false,
    );
    spawn_castle_block(
        commands,
        meshes,
        pal.castle_trim.clone(),
        Vec3::new(cx, roof_y, south_z),
        Vec3::new(width + 1.6, 2.0, wt + 0.9),
        Quat::IDENTITY,
        false,
        false,
    );
    spawn_castle_block(
        commands,
        meshes,
        pal.castle_trim.clone(),
        Vec3::new(west_x, roof_y, cz),
        Vec3::new(wt + 0.9, 2.0, depth + 1.6),
        Quat::IDENTITY,
        false,
        false,
    );
    spawn_castle_block(
        commands,
        meshes,
        pal.castle_trim.clone(),
        Vec3::new(east_x, roof_y, cz),
        Vec3::new(wt + 0.9, 2.0, depth + 1.6),
        Quat::IDENTITY,
        false,
        false,
    );

    for (x, z) in [
        (cx - 18.0, cz - 12.0),
        (cx + 18.0, cz - 12.0),
        (cx - 18.0, cz + 9.0),
        (cx + 18.0, cz + 9.0),
        (cx, cz + 10.0),
    ] {
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: Color::srgb(0.55, 0.85, 1.0),
                    intensity: 9_000.0,
                    range: 18.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(x, floor + 4.8, z),
                ..default()
            },
            WorldGeometry,
        ));
    }

    for (x, z) in [
        (cx - 19.0, cz - 14.0),
        (cx + 19.0, cz - 14.0),
        (cx - 19.0, cz + 13.0),
        (cx + 19.0, cz + 13.0),
    ] {
        spawn_castle_block(
            commands,
            meshes,
            pal.window_warm.clone(),
            Vec3::new(x, floor + 0.95, z),
            Vec3::new(2.2, 1.4, 2.2),
            Quat::IDENTITY,
            false,
            false,
        );
    }
}

// ── Aurora Castle ─────────────────────────────────────────────────────────────
fn spawn_aurora_castle(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    let cx = 490.0_f32;
    let cz = -40.0_f32;
    let ground = terrain_surface_y(cx, cz, seed);
    let floor = ground + 8.0; // castle floor sits atop the mesa
    let hw = 38.0_f32; // half-width of the curtain wall square
    let wt = 4.0_f32; // wall thickness
    spawn_world_anchor(
        commands,
        "aurora_castle_gate",
        Vec3::new(cx, floor + 0.5, cz + hw + 11.0),
    );

    // ── Mesa / Foundation platform ─────────────────────────────────────────
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(92.0, 8.0))),
            material: MeshMaterial3d(pal.castle_stone.clone()),
            transform: Transform::from_xyz(cx, ground + 4.0, cz),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(4.0, 92.0),
    ));
    // Mesa trim ring
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(93.5, 1.0))),
            material: MeshMaterial3d(pal.castle_trim.clone()),
            transform: Transform::from_xyz(cx, ground + 8.5, cz),
            ..default()
        },
        WorldGeometry,
    ));

    // ── Curtain walls ──────────────────────────────────────────────────────
    let wall_h = 15.0_f32;
    let wall_specs: &[(f32, f32, f32, f32)] = &[
        (cx, cz - hw, hw * 2.0 + wt * 2.0, wt), // north
        (cx - hw, cz, wt, hw * 2.0),            // west
        (cx + hw, cz, wt, hw * 2.0),            // east
        (cx - 25.0, cz + hw, 34.0, wt),         // south-left gate wall
        (cx + 25.0, cz + hw, 34.0, wt),         // south-right gate wall
    ];
    for &(wx, wz, ww, wd) in wall_specs {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(ww, wall_h, wd))),
                material: MeshMaterial3d(pal.stone_brick.clone()),
                transform: Transform::from_xyz(wx, floor + wall_h * 0.5, wz),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(ww * 0.5, wall_h * 0.5, wd * 0.5),
        ));
        // Gold parapet atop each wall
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(ww + 0.6, 2.0, wd + 0.6))),
                material: MeshMaterial3d(pal.castle_trim.clone()),
                transform: Transform::from_xyz(wx, floor + wall_h + 1.0, wz),
                ..default()
            },
            WorldGeometry,
        ));
    }
    for (x, z, yaw) in [
        (cx - 24.0, cz - hw - 2.4, std::f32::consts::PI),
        (cx, cz - hw - 2.4, std::f32::consts::PI),
        (cx + 24.0, cz - hw - 2.4, std::f32::consts::PI),
        (cx - hw - 2.4, cz - 18.0, std::f32::consts::FRAC_PI_2),
        (cx - hw - 2.4, cz + 18.0, std::f32::consts::FRAC_PI_2),
        (cx + hw + 2.4, cz - 18.0, std::f32::consts::FRAC_PI_2),
        (cx + hw + 2.4, cz + 18.0, std::f32::consts::FRAC_PI_2),
        (cx - 25.0, cz + hw + 2.4, 0.0),
        (cx + 25.0, cz + hw + 2.4, 0.0),
    ] {
        spawn_castle_window_frame(
            commands,
            meshes,
            pal,
            Vec3::new(x, floor + 8.2, z),
            yaw,
            3.2,
            4.2,
        );
    }

    // ── Corner towers ──────────────────────────────────────────────────────
    for &(tx, tz) in &[
        (cx - hw, cz - hw),
        (cx + hw, cz - hw),
        (cx - hw, cz + hw),
        (cx + hw, cz + hw),
    ] {
        spawn_tower(
            commands,
            meshes,
            pal.stone_brick.clone(),
            pal.castle_roof.clone(),
            Vec3::new(tx, floor, tz),
            7.0,
            52.0,
            22.0,
        );
        // Glowing window slit
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(3.5, 3.5, 0.5))),
                material: MeshMaterial3d(pal.castle_window.clone()),
                transform: Transform::from_xyz(tx, floor + 28.0, tz + 7.2),
                ..default()
            },
            WorldGeometry,
        ));
        // Trim ring mid-tower
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(7.8, 1.2))),
                material: MeshMaterial3d(pal.castle_trim.clone()),
                transform: Transform::from_xyz(tx, floor + 34.0, tz),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // ── Gatehouse (south entry, flanking the south wall gap) ──────────────
    let gate_z = cz + hw + 3.0;
    for &gx in &[cx - 7.0_f32, cx + 7.0] {
        spawn_tower(
            commands,
            meshes,
            pal.stone_brick.clone(),
            pal.castle_roof.clone(),
            Vec3::new(gx, floor, gate_z),
            4.5,
            22.0,
            10.0,
        );
    }
    // Arch lintel
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(17.0, 3.5, 3.0))),
            material: MeshMaterial3d(pal.castle_trim.clone()),
            transform: Transform::from_xyz(cx, floor + 22.5, gate_z),
            ..default()
        },
        WorldGeometry,
    ));
    spawn_castle_bridge_and_gate(commands, meshes, pal, cx, floor, gate_z);

    // ── Great central tower ────────────────────────────────────────────────
    spawn_tower(
        commands,
        meshes,
        pal.stone_brick.clone(),
        pal.castle_roof.clone(),
        Vec3::new(cx, floor, cz),
        13.0,
        88.0,
        32.0,
    );
    // Trim ring at 2/3 height
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(14.2, 1.5))),
            material: MeshMaterial3d(pal.castle_trim.clone()),
            transform: Transform::from_xyz(cx, floor + 60.0, cz),
            ..default()
        },
        WorldGeometry,
    ));
    // Four tall glowing windows on great tower
    for angle in [
        0.0_f32,
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::PI,
        3.0 * std::f32::consts::FRAC_PI_2,
    ] {
        let win_x = cx + angle.cos() * 13.6;
        let win_z = cz + angle.sin() * 13.6;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(4.5, 9.0, 0.5))),
                material: MeshMaterial3d(pal.castle_window.clone()),
                transform: Transform::from_xyz(win_x, floor + 48.0, win_z)
                    .with_rotation(Quat::from_rotation_y(angle)),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // ── Hollow Keep / Great Hall Rooms ─────────────────────────────────────
    spawn_aurora_keep_rooms(commands, meshes, pal, cx, cz, floor);

    // ── Moat ring (magical cyan water) ────────────────────────────────────
    for i in 0..18u32 {
        let angle = i as f32 * std::f32::consts::TAU / 18.0;
        let mr = 78.0_f32;
        let mx = cx + angle.cos() * mr;
        let mz = cz + angle.sin() * mr;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(30.0, 0.6, 13.0))),
                material: MeshMaterial3d(pal.water.clone()),
                transform: Transform::from_xyz(mx, floor - 1.5, mz)
                    .with_rotation(Quat::from_rotation_y(angle)),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // ── Aurora magic point lights ─────────────────────────────────────────
    let aurora_lights: &[(f32, f32, f32, Color, f32)] = &[
        (
            cx,
            floor + 92.0,
            cz,
            Color::srgb(0.55, 0.25, 1.0),
            120_000.0,
        ),
        (
            cx - 32.0,
            floor + 18.0,
            cz - 32.0,
            Color::srgb(0.15, 0.70, 1.0),
            60_000.0,
        ),
        (
            cx + 32.0,
            floor + 18.0,
            cz + 32.0,
            Color::srgb(0.70, 0.35, 1.0),
            60_000.0,
        ),
        (cx, floor + 20.0, cz, Color::srgb(0.30, 0.80, 1.0), 40_000.0),
    ];
    for &(lx, ly, lz, ref col, intensity) in aurora_lights {
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: *col,
                    intensity,
                    range: 70.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(lx, ly, lz),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // ── Banner poles ──────────────────────────────────────────────────────
    let banner_positions: &[(f32, f32, f32)] = &[
        (cx, floor + 92.0, cz),           // great tower top
        (cx + hw, floor + 55.0, cz - hw), // NE corner tower
        (cx - hw, floor + 55.0, cz + hw), // SW corner tower
        (cx, floor + 22.0, gate_z + 2.0), // above gatehouse
    ];
    for (i, &(bx, by, bz)) in banner_positions.iter().enumerate() {
        spawn_wind_banner(
            commands,
            meshes,
            pal,
            Vec3::new(bx, by, bz),
            9.0,
            i as f32 * 0.77,
        );
    }

    // ── Crystal spires ringing the mesa ───────────────────────────────────
    for i in 0..8u32 {
        let angle = i as f32 * std::f32::consts::TAU / 8.0 + 0.3;
        let cr = 85.0_f32;
        let ck_x = cx + angle.cos() * cr;
        let ck_z = cz + angle.sin() * cr;
        let ch = 10.0 + seeded(seed + i as u64 * 7, 0) * 14.0;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cone {
                    radius: 1.5 + seeded(seed + i as u64, 1) * 1.5,
                    height: ch,
                })),
                material: MeshMaterial3d(pal.crystal_aurora.clone()),
                transform: Transform::from_xyz(ck_x, ground + ch * 0.5 + 4.0, ck_z),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

// ── Collosar Castle ───────────────────────────────────────────────────────────
fn spawn_collosar_castle(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    let cx = -8800.0_f32;
    let cz = -8020.0_f32;
    let ground = terrain_surface_y(cx, cz, seed);
    let floor = ground + 6.0;
    let hw = 42.0_f32;
    let wt = 6.0_f32;
    spawn_world_anchor(
        commands,
        "collosar_castle_gate",
        Vec3::new(cx, floor + 0.5, cz + hw + 13.0),
    );

    // ── Castle platform / base ─────────────────────────────────────────────
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(82.0, 6.0))),
            material: MeshMaterial3d(pal.dragon_stone.clone()),
            transform: Transform::from_xyz(cx, ground + 3.0, cz),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(3.0, 82.0),
    ));
    // Rocky outcrop ring around platform (jagged stone spires)
    for i in 0..10u32 {
        let ang = i as f32 * std::f32::consts::TAU / 10.0;
        let rr = 80.0_f32 + seeded(seed + i as u64, 99) * 15.0;
        let rh = 20.0 + seeded(seed + i as u64, 98) * 30.0;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cone {
                    radius: rh * 0.25,
                    height: rh,
                })),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_xyz(
                    cx + ang.cos() * rr,
                    ground + rh * 0.5,
                    cz + ang.sin() * rr,
                ),
                ..default()
            },
            WorldGeometry,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cylinder(rh * 0.5, rh * 0.1),
        ));
    }

    // ── Outer walls ────────────────────────────────────────────────────────
    let wall_h = 20.0_f32;
    let wall_specs: &[(f32, f32, f32, f32)] = &[
        (cx, cz - hw, hw * 2.0 + wt * 2.0, wt),
        (cx, cz + hw, hw * 2.0 + wt * 2.0, wt),
        (cx - hw, cz, wt, hw * 2.0),
        (cx + hw, cz, wt, hw * 2.0),
    ];
    for &(wx, wz, ww, wd) in wall_specs {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(ww, wall_h, wd))),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_xyz(wx, floor + wall_h * 0.5, wz),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(ww * 0.5, wall_h * 0.5, wd * 0.5),
        ));
    }
    // Battlements (notched crenellations along north/south walls)
    for i in 0..12i32 {
        let bx = cx - hw + 4.0 + i as f32 * 7.0;
        for &bz_wall in &[cz - hw, cz + hw] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(4.0, 5.0, wt + 1.0))),
                    material: MeshMaterial3d(pal.dragon_stone.clone()),
                    transform: Transform::from_xyz(bx, floor + wall_h + 2.5, bz_wall),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    // ── Dragon corner towers ───────────────────────────────────────────────
    for &(tx, tz) in &[
        (cx - hw, cz - hw),
        (cx + hw, cz - hw),
        (cx - hw, cz + hw),
        (cx + hw, cz + hw),
    ] {
        // Tower body
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(11.0, 92.0))),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_xyz(tx, floor + 46.0, tz),
                ..default()
            },
            WorldGeometry,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cylinder(46.0, 11.0),
        ));
        // Fang-like spire cap
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cone {
                    radius: 8.0,
                    height: 42.0,
                })),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_xyz(tx, floor + 92.0 + 21.0, tz),
                ..default()
            },
            WorldGeometry,
        ));
        // Red fire slit window
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(2.8, 7.0, 0.6))),
                material: MeshMaterial3d(pal.dragon_window.clone()),
                transform: Transform::from_xyz(tx, floor + 52.0, tz + 11.2),
                ..default()
            },
            WorldGeometry,
        ));
        // Dragon skull atop tower (white sphere + small red eye spheres)
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(5.0))),
                material: MeshMaterial3d(pal.snow.clone()),
                transform: Transform::from_xyz(tx, floor + 136.0, tz),
                ..default()
            },
            WorldGeometry,
        ));
        for &ex in &[-2.2_f32, 2.2] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Sphere::new(1.4))),
                    material: MeshMaterial3d(pal.dragon_window.clone()),
                    transform: Transform::from_xyz(tx + ex, floor + 137.0, tz - 5.2),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    // ── Great Keep ─────────────────────────────────────────────────────────
    let keep_h = 28.0_f32;
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(58.0, keep_h, 48.0))),
            material: MeshMaterial3d(pal.dragon_stone.clone()),
            transform: Transform::from_xyz(cx, floor + keep_h * 0.5, cz),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cuboid(29.0, keep_h * 0.5, 24.0),
    ));

    // ── Collosar's Sanctum Spire ───────────────────────────────────────────
    // The tallest structure in Collosar's Tibet domain.
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(16.0, 135.0))),
            material: MeshMaterial3d(pal.dragon_stone.clone()),
            transform: Transform::from_xyz(cx, floor + 67.5, cz),
            ..default()
        },
        WorldGeometry,
        crate::engine::physics::prelude::RigidBody::Fixed,
        crate::engine::physics::prelude::Collider::cylinder(67.5, 16.0),
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cone {
                radius: 11.0,
                height: 58.0,
            })),
            material: MeshMaterial3d(pal.dragon_stone.clone()),
            transform: Transform::from_xyz(cx, floor + 135.0 + 29.0, cz),
            ..default()
        },
        WorldGeometry,
    ));
    // Dragon eyes on sanctum (large emissive orbs facing south toward city)
    for &ex in &[-7.0_f32, 7.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(3.2))),
                material: MeshMaterial3d(pal.dragon_window.clone()),
                transform: Transform::from_xyz(cx + ex, floor + 100.0, cz - 16.5),
                ..default()
            },
            WorldGeometry,
        ));
    }
    // Sanctum trim band
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(17.0, 1.8))),
            material: MeshMaterial3d(pal.dragon_lava.clone()),
            transform: Transform::from_xyz(cx, floor + 80.0, cz),
            ..default()
        },
        WorldGeometry,
    ));

    // ── Dragon wing-perch arms ──────────────────────────────────────────────
    for &(px, pz) in &[(cx - 38.0, cz), (cx + 38.0, cz)] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(40.0, 3.5, 7.0))),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_xyz(px, floor + 88.0, pz),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(20.0, 1.75, 3.5),
        ));
    }

    // ── Lava moat ──────────────────────────────────────────────────────────
    for i in 0..20u32 {
        let angle = i as f32 * std::f32::consts::TAU / 20.0;
        let mr = 70.0_f32;
        let mx = cx + angle.cos() * mr;
        let mz = cz + angle.sin() * mr;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(26.0, 0.9, 12.0))),
                material: MeshMaterial3d(pal.dragon_lava.clone()),
                transform: Transform::from_xyz(mx, floor - 2.0, mz)
                    .with_rotation(Quat::from_rotation_y(angle)),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // ── Cave entrance (dark arch cut into south wall) ──────────────────────
    let cave_z = cz + hw + 5.0;
    for &gx in &[cx - 9.0_f32, cx + 9.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(6.0, 22.0, 6.0))),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_xyz(gx, floor + 11.0, cave_z),
                ..default()
            },
            WorldGeometry,
            crate::engine::physics::prelude::RigidBody::Fixed,
            crate::engine::physics::prelude::Collider::cuboid(3.0, 11.0, 3.0),
        ));
    }
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(25.0, 5.0, 6.0))),
            material: MeshMaterial3d(pal.dragon_stone.clone()),
            transform: Transform::from_xyz(cx, floor + 23.5, cave_z),
            ..default()
        },
        WorldGeometry,
    ));
    // Cave darkness sphere (emissive void)
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(16.0))),
            material: MeshMaterial3d(pal.dragon_stone.clone()),
            transform: Transform::from_xyz(cx, floor + 6.0, cave_z + 12.0),
            ..default()
        },
        WorldGeometry,
    ));

    // ── Dragon fire point lights ───────────────────────────────────────────
    let fire_lights: &[(f32, f32, f32, f32)] = &[
        (cx, floor + 140.0, cz, 250_000.0),
        (cx - 44.0, floor + 24.0, cz - 44.0, 90_000.0),
        (cx + 44.0, floor + 24.0, cz + 44.0, 90_000.0),
        (cx - 44.0, floor + 24.0, cz + 44.0, 70_000.0),
        (cx + 44.0, floor + 24.0, cz - 44.0, 70_000.0),
        (cx, floor + 12.0, cz, 40_000.0),
    ];
    for &(lx, ly, lz, intensity) in fire_lights {
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: Color::srgb(1.0, 0.38, 0.06),
                    intensity,
                    range: 90.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(lx, ly, lz),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // ── Dragon banners ────────────────────────────────────────────────────
    for &(bx, by, bz) in &[
        (cx - hw, floor + 96.0, cz - hw),
        (cx + hw, floor + 96.0, cz - hw),
        (cx, floor + 143.0, cz),
    ] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(0.4, 12.0))),
                material: MeshMaterial3d(pal.dragon_stone.clone()),
                transform: Transform::from_xyz(bx, by + 6.0, bz),
                ..default()
            },
            WorldGeometry,
        ));
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(7.0, 4.5, 0.25))),
                material: MeshMaterial3d(pal.dragon_banner.clone()),
                transform: Transform::from_xyz(bx + 3.5, by + 11.0, bz),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // ── Everest snow cap (decorative summit near Collosar's domain) ───────
    let ex_x = -9000.0_f32;
    let ex_z = -8350.0_f32;
    let ev_ground = terrain_surface_y(ex_x, ex_z, seed);
    // Snow cap spheres sitting on the Everest terrain peak
    for &(sox, soz, sr) in &[
        (0.0_f32, 0.0, 95.0_f32),
        (-80.0, 54.0, 62.0),
        (72.0, -66.0, 58.0),
        (0.0, -46.0, 78.0),
    ] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(sr))),
                material: MeshMaterial3d(pal.snow.clone()),
                transform: Transform::from_xyz(ex_x + sox, ev_ground - sr * 0.4, ex_z + soz),
                ..default()
            },
            WorldGeometry,
        ));
    }
    // Cold blue glow at Everest peak
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.6, 0.8, 1.0),
                intensity: 320_000.0,
                range: 420.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(ex_x, ev_ground + 5.0, ex_z),
            ..default()
        },
        WorldGeometry,
    ));
}

// ── Magic Crystals ────────────────────────────────────────────────────────────
fn spawn_magic_crystals(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    // Crystal fields: (zone_center_x, zone_center_z, count, is_dragon_crystal)
    let zones: &[(f32, f32, u64, bool)] = &[
        (320.0, -20.0, 14, false),  // Qilian approach (aurora)
        (390.0, 30.0, 10, false),   // Qilian mid (aurora)
        (440.0, -70.0, 8, false),   // Castle approach (aurora)
        (-200.0, -150.0, 8, true),  // Tibetan border (dragon)
        (-320.0, -260.0, 10, true), // Dragon approach
        (-80.0, 200.0, 6, false),   // Aurora domain north (aurora)
        (220.0, 360.0, 9, false),   // Emerald forest pocket
        (-460.0, 180.0, 7, false),  // River-bank crystals
    ];

    for (zi, &(zx, zz, count, is_dragon)) in zones.iter().enumerate() {
        for i in 0..count {
            let idx = zi as u64 * 30 + i;
            let ox = (seeded(seed + 50, idx * 3) - 0.5) * 70.0;
            let oz = (seeded(seed + 50, idx * 3 + 1) - 0.5) * 70.0;
            let h = 5.0 + seeded(seed + 50, idx * 3 + 2) * 16.0;
            let r = 1.2 + seeded(seed + 50, idx * 4) * 2.8;
            let wx = zx + ox;
            let wz = zz + oz;
            let wy = terrain_surface_y(wx, wz, seed);

            let mat = if !is_dragon && (zi + i as usize).is_multiple_of(4) {
                pal.crystal_emerald.clone()
            } else if is_dragon {
                pal.crystal_dragon.clone()
            } else {
                pal.crystal_aurora.clone()
            };

            // Main crystal spire
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cone {
                        radius: r,
                        height: h,
                    })),
                    material: MeshMaterial3d(mat.clone()),
                    transform: Transform::from_xyz(wx, wy + h * 0.5, wz)
                        .with_rotation(Quat::from_rotation_z((seeded(seed + 51, idx) - 0.5) * 0.3)),
                    ..default()
                },
                WorldGeometry,
            ));
            // Companion shard
            if i % 3 != 0 {
                let h2 = h * 0.55;
                let r2 = r * 0.60;
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Cone {
                            radius: r2,
                            height: h2,
                        })),
                        material: MeshMaterial3d(mat.clone()),
                        transform: Transform::from_xyz(wx + r * 1.8, wy + h2 * 0.5, wz + r)
                            .with_rotation(Quat::from_rotation_z(0.22)),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
            // Point light every other crystal
            if i % 2 == 0 {
                let light_col = if !is_dragon && (zi + i as usize).is_multiple_of(4) {
                    Color::srgb(0.18, 1.0, 0.55)
                } else if is_dragon {
                    Color::srgb(1.0, 0.35, 0.08)
                } else {
                    Color::srgb(0.55, 0.25, 1.0)
                };
                commands.spawn((
                    PointLightBundle {
                        point_light: PointLight {
                            color: light_col,
                            intensity: 14_000.0,
                            range: 24.0,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        transform: Transform::from_xyz(wx, wy + h + 1.0, wz),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }
        }
    }
}

/// Which authored route the party's progression has reached.
///
/// Progression is deliberately simple for the shared-screen format: levels are
/// an ordered list and the party's furthest checkpoint selects one. Indices
/// past the last authored route clamp to it, so finishing the set leaves the
/// party in the final level rather than in nothing.
fn platformer_route_index(progress: &PlatformerWorkshopProgress) -> usize {
    let last = crate::world::platformer_routes::ROUTES.len().saturating_sub(1);
    (progress.current_level as usize).min(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_road_mesh_cache_allocates_exactly_two_shared_assets() {
        let mut meshes = Assets::<Mesh>::default();
        let before = meshes.len();
        let cache = SpeedRoadMeshCache::new(&mut meshes);

        assert_eq!(meshes.len() - before, 2);
        assert!(meshes.get(&cache.unit_cube).is_some());
        assert!(meshes.get(&cache.unit_sphere).is_some());

        let another_pad = cache.unit_cube.clone();
        let another_light = cache.unit_sphere.clone();
        assert_eq!(another_pad.id(), cache.unit_cube.id());
        assert_eq!(another_light.id(), cache.unit_sphere.id());
        assert_eq!(meshes.len() - before, 2);
    }

    #[test]
    fn nature_cache_allocates_four_meshes_and_preserves_cylinder_transform() {
        let mut meshes = Assets::<Mesh>::default();
        let before = meshes.len();
        let cache = NaturePrimitiveMeshCache::new(&mut meshes);

        assert_eq!(meshes.len() - before, 4);
        assert!(meshes.get(&cache.unit_cube).is_some());
        assert!(meshes.get(&cache.unit_sphere).is_some());
        assert!(meshes.get(&cache.unit_cylinder).is_some());
        assert!(meshes.get(&cache.unit_cone).is_some());

        let another_root = cache.unit_cube.clone();
        let another_leaf = cache.unit_sphere.clone();
        let another_trunk = cache.unit_cylinder.clone();
        let another_crown = cache.unit_cone.clone();
        assert_eq!(another_root.id(), cache.unit_cube.id());
        assert_eq!(another_leaf.id(), cache.unit_sphere.id());
        assert_eq!(another_trunk.id(), cache.unit_cylinder.id());
        assert_eq!(another_crown.id(), cache.unit_cone.id());
        assert_eq!(meshes.len() - before, 4);

        let start = Vec3::new(-12.0, 3.5, 8.0);
        let end = Vec3::new(27.0, 44.0, -19.0);
        let radius = 2.75;
        let delta = end - start;
        let transform = unit_cylinder_between_transform(start, end, radius).unwrap();

        assert!(transform.translation.abs_diff_eq((start + end) * 0.5, 1e-5));
        assert!(transform
            .scale
            .abs_diff_eq(Vec3::new(radius, delta.length(), radius), 1e-5));
        assert!((transform.rotation * Vec3::Y).abs_diff_eq(delta.normalize(), 1e-5));

        let lower = transform.transform_point(Vec3::new(0.0, -0.5, 0.0));
        let upper = transform.transform_point(Vec3::new(0.0, 0.5, 0.0));
        assert!(lower.abs_diff_eq(start, 1e-4));
        assert!(upper.abs_diff_eq(end, 1e-4));
        assert!(unit_cylinder_between_transform(start, start, radius).is_none());
    }

    #[test]
    fn downtown_facade_mesh_cache_allocates_exactly_three_shared_assets() {
        let mut meshes = Assets::<Mesh>::default();
        let before = meshes.len();
        let cache = DowntownFacadeMeshCache::new(&mut meshes);

        assert_eq!(meshes.len() - before, 3);
        assert!(meshes.get(&cache.unit_cube).is_some());
        assert!(meshes.get(&cache.unit_cylinder).is_some());
        assert!(meshes.get(&cache.unit_sphere).is_some());

        let another_window = cache.unit_cube.clone();
        let another_tank = cache.unit_cylinder.clone();
        let another_beacon = cache.unit_sphere.clone();
        assert_eq!(another_window.id(), cache.unit_cube.id());
        assert_eq!(another_tank.id(), cache.unit_cylinder.id());
        assert_eq!(another_beacon.id(), cache.unit_sphere.id());
        assert_eq!(meshes.len() - before, 3);
    }

    #[test]
    fn scaled_unit_primitives_preserve_facade_and_rooftop_dimensions() {
        let panel_center = Vec3::new(14.0, 38.5, -22.0);
        let panel_size = Vec3::new(21.4, 53.2, 0.18);
        let panel = unit_cuboid_transform(panel_center, Quat::IDENTITY, panel_size);
        assert_eq!(panel.translation, panel_center);
        assert_eq!(panel.rotation, Quat::IDENTITY);
        assert_eq!(panel.scale, panel_size);

        let dish_center = Vec3::new(-7.0, 92.0, 11.0);
        let dish_rotation = Quat::from_rotation_x(0.45);
        let dish = unit_cylinder_transform(dish_center, dish_rotation, 3.6, 0.25);
        assert_eq!(dish.translation, dish_center);
        assert_eq!(dish.rotation, dish_rotation);
        assert_eq!(dish.scale, Vec3::new(3.6, 0.25, 3.6));

        let beacon_center = Vec3::new(4.0, 121.5, -9.0);
        let beacon = unit_sphere_transform(beacon_center, 0.45);
        assert_eq!(beacon.translation, beacon_center);
        assert_eq!(beacon.scale, Vec3::splat(0.45));
    }

    #[test]
    fn scaled_unit_cube_preserves_boost_ramp_visual_and_collider_extents() {
        let center = Vec3::new(18.0, 7.5, -42.0);
        let rotation = Quat::from_rotation_y(0.72) * Quat::from_rotation_x(-0.18);
        let width = 14.0;
        let surface_length = 37.5;
        let transform =
            unit_cuboid_transform(center, rotation, Vec3::new(width, 0.72, surface_length));

        assert_eq!(transform.translation, center);
        assert_eq!(transform.rotation, rotation);
        assert_eq!(transform.scale, Vec3::new(width, 0.72, surface_length));
        assert_eq!(
            transform.scale * 0.5,
            Vec3::new(width * 0.5, 0.36, surface_length * 0.5)
        );
        assert_eq!(
            world_space_collider_scale(),
            crate::engine::physics::prelude::ColliderScale::Absolute(Vec3::ONE)
        );
    }

    #[test]
    fn scaled_roof_moss_keeps_its_authored_nature_sway_scale() {
        let size = Vec3::new(12.4, 0.16, 8.32);
        let transform = unit_cuboid_transform(Vec3::new(-18.0, 26.42, 7.0), Quat::IDENTITY, size);
        let sway = NatureSway::new(&transform, 0.4, 1.2, 0.004, 0.015, 0.015);

        assert_eq!(sway.base_translation, transform.translation);
        assert_eq!(sway.base_rotation, transform.rotation);
        assert_eq!(sway.base_scale, size);
    }

    #[test]
    fn route_blockades_mix_periodic_tanks_without_changing_other_raids() {
        for index in 0..12 {
            let expected = if index % 4 == 0 {
                EnemyType::Tank
            } else {
                EnemyType::Soldier
            };
            assert_eq!(
                raid_wave_enemy_type(RaidKind::RouteBlockade, index),
                expected
            );
        }

        for index in 0..12 {
            assert_eq!(
                raid_wave_enemy_type(RaidKind::RobotLanding, index),
                EnemyType::Soldier
            );
            assert_eq!(
                raid_wave_enemy_type(RaidKind::DroneSwarm, index),
                EnemyType::Drone
            );
            assert_eq!(
                raid_wave_enemy_type(RaidKind::DragonDomainAssault, index),
                EnemyType::SpikeAlien
            );
        }
    }

    #[test]
    fn rooftop_routes_are_deterministic_bounded_and_limit_roof_degree() {
        let anchors = (0..48)
            .map(|index| CityRoofAnchor {
                center: Vec3::new(
                    (index % 8) as f32 * 32.0,
                    20.0 + (index % 3) as f32 * 2.0,
                    (index / 8) as f32 * 32.0,
                ),
                footprint: Vec2::splat(18.0),
            })
            .collect::<Vec<_>>();

        let first = city_rooftop_route_plans(&anchors);
        let second = city_rooftop_route_plans(&anchors);
        assert_eq!(first.len(), second.len());
        assert!(first.len() <= CITY_ROOFTOP_ROUTE_LIMIT);
        for (left, right) in first.iter().zip(second.iter()) {
            assert_eq!(left.kind, right.kind);
            assert!(left.start.distance(right.start) < 0.001);
            assert!(left.end.distance(right.end) < 0.001);
        }

        let mut degree = vec![0usize; anchors.len()];
        for route in &first {
            for endpoint in [route.start, route.end] {
                let nearest = anchors
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let a_distance =
                            Vec2::new(endpoint.x - a.center.x, endpoint.z - a.center.z).length();
                        let b_distance =
                            Vec2::new(endpoint.x - b.center.x, endpoint.z - b.center.z).length();
                        a_distance.total_cmp(&b_distance)
                    })
                    .map(|(index, _)| index)
                    .unwrap();
                degree[nearest] += 1;
            }
        }
        assert!(degree
            .into_iter()
            .all(|connections| { connections <= CITY_ROOFTOP_MAX_DEGREE }));
    }

    #[test]
    fn rooftop_routes_use_bridges_for_gentle_links_and_ziplines_for_drops() {
        let anchors = [
            CityRoofAnchor {
                center: Vec3::new(0.0, 20.0, 0.0),
                footprint: Vec2::splat(18.0),
            },
            CityRoofAnchor {
                center: Vec3::new(50.0, 23.0, 0.0),
                footprint: Vec2::splat(18.0),
            },
            CityRoofAnchor {
                center: Vec3::new(100.0, 45.0, 0.0),
                footprint: Vec2::splat(18.0),
            },
        ];

        let plans = city_rooftop_route_plans(&anchors);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].kind, CityRooftopRouteKind::Skybridge);
        assert_eq!(plans[1].kind, CityRooftopRouteKind::Zipline);
        assert!(plans[1].start.y > plans[1].end.y);
    }

    #[test]
    fn rooftop_route_reward_keys_are_stable_in_either_travel_direction() {
        let forward = CityRooftopRoutePlan {
            start: Vec3::new(12.34, 40.0, -8.0),
            end: Vec3::new(55.0, 28.0, 91.2),
            kind: CityRooftopRouteKind::Zipline,
        };
        let reverse = CityRooftopRoutePlan {
            start: forward.end,
            end: forward.start,
            kind: forward.kind,
        };

        assert_eq!(
            city_rooftop_route_reward_key(forward),
            city_rooftop_route_reward_key(reverse)
        );
    }

    #[test]
    fn rooftop_route_completion_requires_opposite_endpoints() {
        let mut approach = -1;
        assert!(!rooftop_route_endpoint_crossed(&mut approach, true, false));
        assert_eq!(approach, 0);
        assert!(!rooftop_route_endpoint_crossed(&mut approach, true, false));
        assert!(rooftop_route_endpoint_crossed(&mut approach, false, true));
        assert_eq!(approach, -1);

        assert!(!rooftop_route_endpoint_crossed(&mut approach, false, true));
        assert_eq!(approach, 1);
        assert!(rooftop_route_endpoint_crossed(&mut approach, true, false));
    }

    #[test]
    fn connected_rooftop_junctions_create_bounded_directional_playgrounds() {
        let anchors = (0..20)
            .map(|index| CityRoofAnchor {
                center: Vec3::new(index as f32 * 34.0, 24.0, 0.0),
                footprint: Vec2::splat(20.0),
            })
            .collect::<Vec<_>>();
        let routes = city_rooftop_route_plans(&anchors);

        let playgrounds = city_sky_playground_plans(&anchors, &routes);
        assert_eq!(playgrounds.len(), CITY_SKY_PLAYGROUND_LIMIT);
        for playground in playgrounds {
            let launch_direction = playground.launch_velocity.with_y(0.0).normalize_or_zero();
            let target_direction = (playground.launch_target - playground.center)
                .with_y(0.0)
                .normalize_or_zero();
            assert!(launch_direction.dot(target_direction) > 0.999);
            assert!(playground.launch_velocity.y >= 14.0);
            assert!(playground.radius <= 6.2);
            assert!(
                playground
                    .launch_position
                    .with_y(0.0)
                    .distance(playground.center.with_y(0.0))
                    < 7.0
            );
        }
    }

    #[test]
    fn rooftop_trial_courses_are_deterministic_bounded_and_medal_timed() {
        let anchors = (0..18)
            .map(|index| CityRoofAnchor {
                center: Vec3::new(index as f32 * 34.0, 24.0, 0.0),
                footprint: Vec2::splat(20.0),
            })
            .collect::<Vec<_>>();
        let routes = city_rooftop_route_plans(&anchors);

        let first = city_rooftop_trial_plans(&anchors, &routes);
        let second = city_rooftop_trial_plans(&anchors, &routes);
        assert_eq!(first, second);
        assert!(first.len() <= CITY_ROOFTOP_TRIAL_LIMIT);
        assert!(!first.is_empty());
        for course in first {
            assert!((4..=CITY_ROOFTOP_TRIAL_MAX_CHECKPOINTS).contains(&course.checkpoints.len()));
            assert!(course.gold_time < course.silver_time);
            assert!(course.silver_time < course.bronze_time);
            assert!(course
                .checkpoints
                .windows(2)
                .all(|pair| pair[0].distance(pair[1]) >= 18.0));
        }
    }

    #[test]
    fn rooftop_trial_medals_unlock_lower_tiers() {
        assert_eq!(
            city_rooftop_trial_medal(19.9, 20.0, 25.0, 32.0),
            CityRooftopTrialMedal::Gold
        );
        assert_eq!(
            city_rooftop_trial_medal(24.0, 20.0, 25.0, 32.0),
            CityRooftopTrialMedal::Silver
        );
        assert_eq!(
            city_rooftop_trial_medal(30.0, 20.0, 25.0, 32.0),
            CityRooftopTrialMedal::Bronze
        );
        assert_eq!(
            city_rooftop_trial_medal(40.0, 20.0, 25.0, 32.0),
            CityRooftopTrialMedal::Finish
        );

        let mut progress = ChapterProgress::default();
        unlock_city_rooftop_trial_medal(&mut progress, "trial:test", CityRooftopTrialMedal::Gold);
        for tier in ["bronze", "silver", "gold"] {
            assert!(progress.has_discoverable(&format!("trial:test:medal:{tier}")));
        }
    }

    #[test]
    fn rooftop_trial_enforces_checkpoints_and_rewards_only_first_clear() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.init_resource::<ChapterProgress>();
        app.add_message::<UiMessageEvent>();
        app.add_systems(Update, city_rooftop_time_trial_system);
        for checkpoint_index in 0..4u8 {
            app.world_mut().spawn((
                Transform::from_xyz(checkpoint_index as f32 * 20.0, 0.0, 0.0),
                CityRooftopTrialGate {
                    course_key: "trial:test".to_string(),
                    course_label: "Test Skyline".to_string(),
                    checkpoint_index,
                    checkpoint_count: 4,
                    radius: 4.0,
                    gold_time: 20.0,
                    silver_time: 25.0,
                    bronze_time: 32.0,
                },
            ));
        }
        let player = app
            .world_mut()
            .spawn((
                Player,
                PlayerIndex(0),
                Transform::default(),
                RooftopTrialProgress::default(),
                PlayerStats {
                    armor: 0.0,
                    ..default()
                },
                Health::new(100.0),
            ))
            .id();

        let cross = |app: &mut App, checkpoint_index: u8| {
            app.world_mut()
                .get_mut::<Transform>(player)
                .unwrap()
                .translation = Vec3::X * (checkpoint_index as f32 * 20.0);
            app.world_mut()
                .get_mut::<RooftopTrialProgress>(player)
                .unwrap()
                .gate_cooldown = 0.0;
            app.update();
        };

        cross(&mut app, 0);
        cross(&mut app, 2);
        assert_eq!(
            app.world()
                .get::<RooftopTrialProgress>(player)
                .unwrap()
                .next_checkpoint,
            1
        );
        cross(&mut app, 1);
        cross(&mut app, 2);
        app.world_mut()
            .get_mut::<RooftopTrialProgress>(player)
            .unwrap()
            .elapsed = 19.0;
        cross(&mut app, 3);

        let stats = app.world().get::<PlayerStats>(player).unwrap();
        assert_eq!(stats.credits, 120);
        assert_eq!(stats.experience, 90);
        assert_eq!(stats.armor, 12.0);
        let progress = app.world().resource::<ChapterProgress>();
        assert!(progress.has_discoverable("trial:test:complete"));
        assert!(progress.has_discoverable("trial:test:medal:gold"));

        for checkpoint in 0..3u8 {
            cross(&mut app, checkpoint);
        }
        app.world_mut()
            .get_mut::<RooftopTrialProgress>(player)
            .unwrap()
            .elapsed = 24.0;
        cross(&mut app, 3);
        let stats = app.world().get::<PlayerStats>(player).unwrap();
        assert_eq!(stats.credits, 120);
        assert_eq!(stats.experience, 90);
        assert_eq!(
            app.world()
                .get::<RooftopTrialProgress>(player)
                .unwrap()
                .best_times[0]
                .1,
            19.0
        );
    }

    #[test]
    fn boost_ramp_pose_inherits_road_grade_and_entrance_height() {
        let base = Vec3::new(10.0, 40.0, -6.0);
        let road_direction = Vec3::new(0.0, 0.25, 1.0).normalize();
        let horizontal_length = 20.0;
        let added_rise = 4.0;
        let (center, rotation, surface_length, boost_direction) =
            board_boost_ramp_pose(base, road_direction, horizontal_length, added_rise).unwrap();

        let expected_total_rise = 0.25 * horizontal_length + added_rise;
        let expected_exit = base + Vec3::new(0.0, expected_total_rise, horizontal_length);
        let actual_entrance = center - rotation * Vec3::Z * (surface_length * 0.5);
        let actual_exit = center + rotation * Vec3::Z * (surface_length * 0.5);

        assert!(actual_entrance.distance(base) < 0.001);
        assert!(actual_exit.distance(expected_exit) < 0.001);
        assert!(boost_direction.distance(Vec3::Z) < 0.001);
    }

    #[test]
    fn road_profile_tangent_preserves_local_slope() {
        let profile = [
            Vec3::new(0.0, 10.0, 0.0),
            Vec3::new(0.0, 15.0, 20.0),
            Vec3::new(20.0, 15.0, 20.0),
        ];

        let tangent = road_profile_tangent_at(&profile, 0.5, 8.0).unwrap();
        let expected = Vec3::new(0.0, 5.0, 20.0).normalize();
        assert!(tangent.distance(expected) < 0.001);
    }

    #[test]
    fn dungeon_return_slots_are_distinct_and_centered() {
        let center = Vec3::new(12.0, 4.0, -8.0);
        let right = Vec3::new(0.0, 0.0, -1.0);
        let forward = Vec3::X;
        let positions = dungeon_return_positions(center, right, forward);

        for a in 0..positions.len() {
            for b in (a + 1)..positions.len() {
                assert!(positions[a].distance(positions[b]) >= 2.3);
            }
        }
        let average = positions.into_iter().sum::<Vec3>() / 4.0;
        assert!(average.distance(center) <= 0.001);
    }

    #[test]
    fn dungeon_exit_returns_all_four_players_and_releases_camera_state() {
        let mut app = App::new();
        app.add_message::<UiMessageEvent>();
        app.init_resource::<DungeonRoomState>();
        app.add_systems(Update, dungeon_exit_portal_system);
        let mut dungeon = DungeonCrawlState::default();
        dungeon.activate(
            "cave_test",
            crate::chapters::ChapterId(1),
            "Test Cave",
            Vec3::new(0.0, 0.0, 30.0),
            Vec3::ZERO,
            64.0,
        );
        app.insert_resource(dungeon);

        let returns = dungeon_return_positions(Vec3::new(0.0, 2.0, -8.0), Vec3::X, Vec3::Z);
        app.world_mut().spawn(DungeonExitPortal {
            gate_id: "cave_test",
            position: Vec3::ZERO,
            return_positions: returns,
            interact_radius: 5.5,
        });
        let mut players = Vec::new();
        for index in 0..4u8 {
            players.push(
                app.world_mut()
                    .spawn((
                        Player,
                        PlayerIndex(index),
                        PlayerInput {
                            interact: index == 0,
                            ..default()
                        },
                        Transform::from_xyz(index as f32 * 20.0, 2.0, 0.0),
                        PlayerMovement::default(),
                    ))
                    .id(),
            );
        }

        // Activation first arms the portal; a later intentional interaction
        // performs the return instead of reusing the gate-opening press.
        app.update();
        assert!(app.world().resource::<DungeonCrawlState>().active);
        app.update();

        assert!(!app.world().resource::<DungeonCrawlState>().active);
        assert!(app
            .world()
            .resource::<DungeonCrawlState>()
            .gate_id
            .is_none());
        for (index, entity) in players.into_iter().enumerate() {
            let transform = app.world().get::<Transform>(entity).unwrap();
            let movement = app.world().get::<PlayerMovement>(entity).unwrap();
            assert_eq!(transform.translation, returns[index]);
            assert_eq!(movement.velocity, Vec3::ZERO);
            assert_eq!(movement.ground_velocity, Vec3::ZERO);
        }
    }

    #[test]
    fn every_secret_cave_profile_is_continuous_graded_and_terrain_safe() {
        let seed = GameSettings::default().world_seed;
        for spec in SECRET_CAVE_LOCATIONS {
            let width = 10.0 + (spec.chapter.0 % 3) as f32 * 1.5;
            let profile = secret_cave_floor_profile(spec, seed, width);
            let forward = Vec3::new(spec.yaw.sin(), 0.0, spec.yaw.cos());
            let right = Vec3::new(forward.z, 0.0, -forward.x);

            assert!(
                profile.stations.len() >= 4,
                "{} has too few stations",
                spec.label
            );
            for pair in profile.stations.windows(2) {
                let horizontal = (pair[1] - pair[0]).with_y(0.0).length();
                let grade = (pair[1].y - pair[0].y).abs() / horizontal.max(0.001);
                assert!(
                    horizontal <= SECRET_CAVE_TUNNEL_SPACING + 0.01,
                    "{} has an oversized floor seam",
                    spec.label
                );
                assert!(
                    grade <= SECRET_CAVE_MAX_GRADE + 0.001,
                    "{} exceeds safe grade: {grade}",
                    spec.label
                );
            }

            let chamber_front = profile.chamber_surface - forward * SECRET_CAVE_CHAMBER_HALF_LENGTH;
            let tunnel_end = *profile.stations.last().unwrap();
            assert!(
                tunnel_end.distance(chamber_front) <= 0.01,
                "{} tunnel does not meet its chamber",
                spec.label
            );

            let half_width = width * 0.5 + 0.75;
            let sample_count = (profile.tunnel_end / 1.0).ceil() as usize;
            let mut maximum_clearance = 0.0_f32;
            for sample in 0..=sample_count {
                let along = profile.tunnel_end * sample as f32 / sample_count as f32;
                let floor = secret_cave_profile_position(&profile, along);
                for side in [-half_width, 0.0, half_width] {
                    let point = floor + right * side;
                    let clearance = floor.y - terrain_surface_y(point.x, point.z, seed);
                    maximum_clearance = maximum_clearance.max(clearance);
                    assert!(
                        clearance >= SECRET_CAVE_TERRAIN_CLEARANCE - 0.08,
                        "{} intersects terrain at {along:.1}: clearance {clearance:.2}",
                        spec.label
                    );
                }
            }
            assert!(
                maximum_clearance <= SECRET_CAVE_TERRAIN_CLEARANCE + 0.08,
                "{} floats above its terrain inset: {maximum_clearance:.2}",
                spec.label
            );

            let chamber_half_width = (width + 9.0) * 0.5 + 0.65;
            for along in [-8.5, 0.0, 8.5] {
                for side in [-chamber_half_width, 0.0, chamber_half_width] {
                    let point = profile.chamber_surface + forward * along + right * side;
                    assert!(
                        profile.chamber_surface.y
                            >= terrain_surface_y(point.x, point.z, seed)
                                + SECRET_CAVE_TERRAIN_CLEARANCE
                                - 0.01,
                        "{} chamber intersects terrain",
                        spec.label
                    );
                }
            }
        }
    }

    #[test]
    fn every_secret_cave_room_graph_is_connected_and_ordered() {
        let seed = GameSettings::default().world_seed;
        for spec in SECRET_CAVE_LOCATIONS {
            let width = 10.0 + (spec.chapter.0 % 3) as f32 * 1.5;
            let profile = secret_cave_floor_profile(spec, seed, width);
            let graph = secret_cave_room_graph(spec, &profile);

            assert_eq!(graph.rooms.map(|room| room.room_index), [0, 1, 2]);
            assert!(graph.portals[0].connects(0, 1));
            assert!(graph.portals[1].connects(1, 2));
            assert!(!graph.portals.iter().any(|portal| portal.connects(0, 2)));
            for pair in graph.rooms.windows(2) {
                assert!(
                    pair[1].focus.distance(pair[0].focus) <= pair[0].camera_radius,
                    "{} camera zones leave a party-pull gap",
                    spec.label
                );
            }
        }
    }

    #[test]
    fn every_mountain_cave_has_monumental_exterior_gate_dimensions() {
        const { assert!(ANCIENT_CAVE_GATE_HEIGHT >= 12.0) };
        for spec in SECRET_CAVE_LOCATIONS {
            let clear_width = 10.0 + (spec.chapter.0 % 3) as f32 * 1.5;
            let gate = AncientCaveGate {
                gate_id: spec.anchor_id,
                clear_width,
                height: ANCIENT_CAVE_GATE_HEIGHT,
            };
            assert_eq!(gate.gate_id, spec.anchor_id);
            assert!(gate.clear_width >= 10.0);
            assert!(gate.height > gate.clear_width * 0.9);
        }
    }

    #[test]
    fn chamber_encounter_stays_locked_until_owned_enemies_are_defeated() {
        let mut app = App::new();
        app.add_message::<UiMessageEvent>();
        app.init_resource::<Time>();
        app.init_resource::<DungeonRoomState>();
        app.add_systems(Update, dungeon_encounter_progress_system);
        let mut dungeon = DungeonCrawlState::default();
        dungeon.activate(
            "encounter_test",
            crate::chapters::ChapterId(1),
            "Encounter Test",
            Vec3::ZERO,
            Vec3::ZERO,
            32.0,
        );
        app.insert_resource(dungeon);
        app.world_mut().spawn(DungeonEnemySpawner {
            chapter: 1,
            encounter: Some(("encounter_test", 2)),
            wave_index: 0,
            enemy_type: EnemyType::Soldier,
            count: 1,
            trigger_radius: 10.0,
            difficulty: 1.0,
            spawned: true,
        });
        let second_spawner = app
            .world_mut()
            .spawn(DungeonEnemySpawner {
                chapter: 1,
                encounter: Some(("encounter_test", 2)),
                wave_index: 1,
                enemy_type: EnemyType::Heavy,
                count: 1,
                trigger_radius: 10.0,
                difficulty: 1.1,
                spawned: false,
            })
            .id();
        let enemy = app
            .world_mut()
            .spawn(DungeonEncounterEnemy {
                gate_id: "encounter_test",
                room_index: 2,
                wave_index: 0,
            })
            .id();
        app.world_mut().spawn((
            Transform::from_translation(Vec3::Y * 8.0),
            DungeonEncounterDoor {
                gate_id: "encounter_test",
                room_index: 2,
                requires_room_clear: None,
                closed: Vec3::ZERO,
                open: Vec3::Y * 8.0,
            },
        ));
        let reward = app
            .world_mut()
            .spawn((
                DungeonEncounterReward {
                    gate_id: "encounter_test",
                    room_index: 2,
                    credits: 100,
                    healing: 25.0,
                    claimed: false,
                },
                Visibility::Hidden,
            ))
            .id();

        app.update();
        assert!(!app
            .world()
            .resource::<DungeonRoomState>()
            .cleared_rooms
            .contains(&("encounter_test", 2)));
        assert_eq!(
            app.world().get::<Visibility>(reward),
            Some(&Visibility::Hidden)
        );

        app.world_mut().entity_mut(enemy).despawn();
        app.update();
        assert!(!app
            .world()
            .resource::<DungeonRoomState>()
            .cleared_rooms
            .contains(&("encounter_test", 2)));

        app.world_mut()
            .get_mut::<DungeonEnemySpawner>(second_spawner)
            .unwrap()
            .spawned = true;
        let final_enemy = app
            .world_mut()
            .spawn(DungeonEncounterEnemy {
                gate_id: "encounter_test",
                room_index: 2,
                wave_index: 1,
            })
            .id();
        app.update();
        assert!(!app
            .world()
            .resource::<DungeonRoomState>()
            .cleared_rooms
            .contains(&("encounter_test", 2)));
        app.world_mut().entity_mut(final_enemy).despawn();
        app.update();
        assert!(app
            .world()
            .resource::<DungeonRoomState>()
            .cleared_rooms
            .contains(&("encounter_test", 2)));
        assert_eq!(
            app.world().get::<Visibility>(reward),
            Some(&Visibility::Visible)
        );
    }

    #[test]
    fn ordered_dungeon_waves_wait_for_spawn_and_clear_of_previous_beat() {
        let encounter = ("wave_test", 3);
        let mut states = vec![(Some(encounter), 0, false), (Some(encounter), 1, false)];
        assert!(!ordered_dungeon_wave_ready(encounter, 1, &states, &[]));

        states[0].2 = true;
        let living_first_wave = [DungeonEncounterEnemy {
            gate_id: encounter.0,
            room_index: encounter.1,
            wave_index: 0,
        }];
        assert!(!ordered_dungeon_wave_ready(
            encounter,
            1,
            &states,
            &living_first_wave
        ));
        assert!(ordered_dungeon_wave_ready(encounter, 1, &states, &[]));
        assert!(!ordered_dungeon_wave_ready(
            ("unrelated", 3),
            1,
            &states,
            &[]
        ));
        assert!(dungeon_encounter_door_should_close(false, false, false));
        assert!(dungeon_encounter_door_should_close(true, true, false));
        assert!(!dungeon_encounter_door_should_close(true, false, false));
        assert!(!dungeon_encounter_door_should_close(true, true, true));
    }

    #[test]
    fn cleared_dungeon_relic_cache_restores_and_rewards_every_player_once() {
        let mut app = App::new();
        app.add_message::<UiMessageEvent>();
        let mut dungeon = DungeonCrawlState::default();
        dungeon.activate_arcade(
            "reward_test",
            crate::chapters::ChapterId(1),
            "Reward Test",
            Vec3::ZERO,
            Vec3::ZERO,
            32.0,
        );
        app.insert_resource(dungeon);
        let mut rooms = DungeonRoomState::default();
        rooms.mark_cleared("reward_test", 3);
        app.insert_resource(rooms);
        app.add_systems(Update, dungeon_reward_pickup_system);

        let reward = app
            .world_mut()
            .spawn((
                Transform::default(),
                DungeonEncounterReward {
                    gate_id: "reward_test",
                    room_index: 3,
                    credits: 75,
                    healing: 30.0,
                    claimed: false,
                },
                Visibility::Visible,
            ))
            .id();
        let players = (0..2)
            .map(|index| {
                app.world_mut()
                    .spawn((
                        Player,
                        Transform::from_xyz(index as f32, 0.0, 0.0),
                        Health {
                            current: 40.0,
                            max: 100.0,
                        },
                        PlayerStats {
                            credits: 5,
                            ..default()
                        },
                    ))
                    .id()
            })
            .collect::<Vec<_>>();

        app.update();
        assert!(app
            .world()
            .get::<DungeonEncounterReward>(reward)
            .is_some_and(|reward| reward.claimed));
        assert_eq!(
            app.world().get::<Visibility>(reward),
            Some(&Visibility::Hidden)
        );
        for player in &players {
            assert_eq!(app.world().get::<Health>(*player).unwrap().current, 70.0);
            assert_eq!(app.world().get::<PlayerStats>(*player).unwrap().credits, 80);
        }

        app.update();
        for player in players {
            assert_eq!(app.world().get::<Health>(player).unwrap().current, 70.0);
            assert_eq!(app.world().get::<PlayerStats>(player).unwrap().credits, 80);
        }
    }

    #[test]
    fn dungeon_room_progression_moves_camera_through_adjacent_rooms() {
        let mut app = App::new();
        app.add_message::<UiMessageEvent>();
        app.init_resource::<DungeonRoomState>();
        app.add_systems(Update, dungeon_room_progress_system);

        let spec = SECRET_CAVE_LOCATIONS[0];
        let width = 10.0 + (spec.chapter.0 % 3) as f32 * 1.5;
        let profile = secret_cave_floor_profile(spec, GameSettings::default().world_seed, width);
        let graph = secret_cave_room_graph(spec, &profile);
        for room in graph.rooms {
            app.world_mut().spawn(room);
        }
        for portal in graph.portals {
            app.world_mut().spawn(portal);
        }
        let mut dungeon = DungeonCrawlState::default();
        dungeon.activate(
            spec.anchor_id,
            spec.chapter,
            spec.label,
            graph.rooms[0].focus,
            graph.rooms[0].focus,
            64.0,
        );
        app.insert_resource(dungeon);
        let players = (0..4u8)
            .map(|index| {
                app.world_mut()
                    .spawn((
                        Player,
                        PlayerIndex(index),
                        Transform::from_translation(
                            graph.rooms[0].focus + Vec3::X * (index as f32 - 1.5),
                        ),
                    ))
                    .id()
            })
            .collect::<Vec<_>>();

        app.update();
        assert_eq!(
            app.world().resource::<DungeonRoomState>().active_room,
            Some(0)
        );

        for (index, entity) in players.iter().enumerate() {
            app.world_mut()
                .get_mut::<Transform>(*entity)
                .unwrap()
                .translation = graph.rooms[1].focus + Vec3::X * (index as f32 - 1.5);
        }
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<DungeonRoomState>().active_room,
            Some(1)
        );
        assert_eq!(
            app.world().resource::<DungeonCrawlState>().focus,
            graph.rooms[1].focus
        );

        for (index, entity) in players.iter().enumerate() {
            app.world_mut()
                .get_mut::<Transform>(*entity)
                .unwrap()
                .translation = graph.rooms[2].focus + Vec3::X * (index as f32 - 1.5);
        }
        app.update();
        app.update();
        let room_state = app.world().resource::<DungeonRoomState>();
        assert_eq!(room_state.active_room, Some(2));
        assert_eq!(
            room_state.visited_rooms,
            vec![
                (spec.anchor_id, 0),
                (spec.anchor_id, 1),
                (spec.anchor_id, 2)
            ]
        );
        assert_eq!(
            app.world().resource::<DungeonCrawlState>().radius,
            graph.rooms[2].camera_radius
        );

        // A direct cave fast-travel arrival has no active room yet and must
        // initialize from the chamber instead of forcing the entrance node.
        app.world_mut()
            .resource_mut::<DungeonRoomState>()
            .clear_active();
        app.update();
        assert_eq!(
            app.world().resource::<DungeonRoomState>().active_room,
            Some(2)
        );
        {
            let mut room_state = app.world_mut().resource_mut::<DungeonRoomState>();
            room_state.mark_cleared(spec.anchor_id, 2);
            room_state.mark_cleared(spec.anchor_id, 2);
            assert_eq!(room_state.cleared_rooms, vec![(spec.anchor_id, 2)]);
        }
    }

    #[test]
    fn everest_heightmap_asset_loads() {
        assert!(everest_heightmap().is_some());
    }

    #[test]
    fn race_heightmap_asset_loads_and_is_bounded_to_southeast_region() {
        assert!(race_heightmap().is_some());
        let (center_height, center_blend) =
            race_heightmap_surface(RACE_REGION_CENTER.x, RACE_REGION_CENTER.y, 42)
                .expect("race tile center should sample");
        assert!((18.0..=430.0).contains(&center_height));
        assert!(center_blend > 0.99);
        assert!(race_heightmap_surface(0.0, 0.0, 42).is_none());
    }

    #[test]
    fn race_region_has_wide_connected_roads_and_two_travel_points() {
        assert_eq!(RACE_REGION_TRAVEL_POINTS.len(), 2);
        assert!(speed_road_width_for_route(RACE_CIRCUIT_ROUTE_INDEX) > SPEED_ROAD_WIDTH);
        const {
            assert!(SPEED_ROAD_WIDTH >= 256.0);
            assert!(RACE_REGION_ROAD_WIDTH >= 384.0);
            assert!(SPEED_ROAD_TRAFFIC_LANE_OFFSET >= 50.0);
            assert!(SPEED_ROAD_OUTER_WALL_HEIGHT >= 12.0);
            assert!(SPEED_ROAD_CENTER_WALL_HEIGHT >= 7.0);
        }
        assert_eq!(
            ROUTE_RACE_CIRCUIT.first(),
            ROUTE_RACE_CIRCUIT.last(),
            "race circuit must close"
        );
        for point in RACE_REGION_TRAVEL_POINTS {
            assert!(ROUTE_RACE_CIRCUIT.contains(&(point.x, point.z)));
            assert!(
                (point.x - RACE_REGION_CENTER.x).abs() <= RACE_REGION_HALF_EXTENT
                    && (point.z - RACE_REGION_CENTER.y).abs() <= RACE_REGION_HALF_EXTENT
            );
        }
        assert!(ROUTE_RACE_NORTH_ACCESS.contains(&ROUTE_RACE_CIRCUIT[0]));
        assert!(ROUTE_RACE_WEST_ACCESS.contains(&ROUTE_RACE_CIRCUIT[5]));
    }

    #[test]
    fn road_boosts_scale_with_motion_and_armor_logic_upgrades() {
        let base = crate::combat::upgrades::UpgradeLedger::default();
        let upgraded = crate::combat::upgrades::UpgradeLedger {
            ranks: vec![
                (crate::combat::upgrades::TechUpgradeId::MotionBootSuite, 5),
                (crate::combat::upgrades::TechUpgradeId::AegisArmorSuite, 5),
            ],
            ..default()
        };
        assert_eq!(road_boost_upgrade_mult(&base), 1.0);
        assert!(road_boost_upgrade_mult(&upgraded) > 1.30);
        assert!(road_boost_upgrade_mult(&upgraded) <= 1.40);
    }

    #[test]
    fn city_core_terrain_stays_flat() {
        assert_eq!(terrain_height(0.0, 0.0, 42), 0.0);
    }

    #[test]
    fn outer_world_terrain_keeps_relief() {
        assert!(terrain_height(-8500.0, -7800.0, 42) > 220.0);
    }

    #[test]
    fn terrain_surface_query_matches_generated_trimesh_triangles() {
        let seed = 42;
        let cell_x = 173.0;
        let cell_z = 91.0;
        let origin_x = cell_x * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5;
        let origin_z = cell_z * TERRAIN_MESH_CELL_SIZE - TERRAIN_WORLD_SIZE * 0.5;
        assert!(
            (terrain_surface_y(origin_x, origin_z, seed)
                - terrain_mesh_vertex_height(origin_x, origin_z, seed))
            .abs()
                < 0.001
        );

        let tx = 0.27;
        let tz = 0.31;
        let h00 = terrain_mesh_vertex_height(origin_x, origin_z, seed);
        let h10 = terrain_mesh_vertex_height(origin_x + TERRAIN_MESH_CELL_SIZE, origin_z, seed);
        let h01 = terrain_mesh_vertex_height(origin_x, origin_z + TERRAIN_MESH_CELL_SIZE, seed);
        let expected = h00 + (h10 - h00) * tx + (h01 - h00) * tz;
        let sampled = terrain_surface_y(
            origin_x + TERRAIN_MESH_CELL_SIZE * tx,
            origin_z + TERRAIN_MESH_CELL_SIZE * tz,
            seed,
        );
        assert!((sampled - expected).abs() < 0.001);
    }

    #[test]
    fn mountain_routes_cover_multiple_passes() {
        assert!(mountain_routes().len() >= 14);
        assert!(mountain_routes().iter().all(|route| route.len() >= 2));
    }

    #[test]
    fn mountain_road_cross_links_create_real_network_loops() {
        let links = [
            ROUTE_LINK_AURORA_NORTH,
            ROUTE_LINK_CORE_SOUTH,
            ROUTE_LINK_WEST_CROWN,
            ROUTE_LINK_EAST_SOUTH,
            ROUTE_LINK_NORTHWEST_AURORA,
            ROUTE_LINK_OUTER_NORTH,
        ];
        assert_eq!(links.len(), 6);
        for link in links {
            let start = link[0];
            let end = link[link.len() - 1];
            let start_connections = mountain_routes()
                .iter()
                .filter(|route| route.contains(&start))
                .count();
            let end_connections = mountain_routes()
                .iter()
                .filter(|route| route.contains(&end))
                .count();
            assert!(start_connections >= 2);
            assert!(end_connections >= 2);
        }
    }

    #[test]
    fn mountain_route_influence_marks_path_corridors() {
        assert!(mountain_route_influence(3200.0, 920.0) > 0.85);
        assert!(mountain_route_influence(9900.0, -1000.0) < 0.20);
    }

    #[test]
    fn star_city_building_keepout_includes_the_building_footprint() {
        let direction = Vec2::new(
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        );
        let just_outside = direction * (STAR_CITY_RING_RADIUS + STAR_CITY_RING_WIDTH * 0.5 + 2.0);
        assert!(!building_footprint_clears_star_city(
            just_outside.x,
            just_outside.y,
            30.0,
        ));

        let clear = direction * (STAR_CITY_RING_RADIUS + STAR_CITY_RING_WIDTH * 0.5 + 30.1);
        assert!(building_footprint_clears_star_city(clear.x, clear.y, 30.0,));
    }

    #[test]
    fn mountain_speed_roads_cover_authored_routes() {
        assert!(mountain_speed_road_segment_count() >= 70);

        for &(x, z) in ROUTE_CORE_EVEREST {
            assert!(
                distance_to_speed_road_network(x, z, 42) < 3.0,
                "core Everest route node should sit on the speed-road network"
            );
        }
    }

    #[test]
    fn settlement_speed_roads_create_access_rings_and_spurs() {
        for settlement in map_settlements().iter().copied() {
            let ring_radius = settlement_speed_ring_radius(42, settlement);
            let ring_access =
                distance_to_speed_road_network(settlement.x + ring_radius, settlement.z, 42);
            assert!(
                ring_access < 8.0,
                "{} should have a nearby speed-road ring",
                settlement.name
            );

            let projection = nearest_mountain_route_point(settlement.x, settlement.z)
                .expect("settlement speed spurs need a route trunk");
            let to_route = projection.point - Vec2::new(settlement.x, settlement.z);
            assert!(to_route.length() > ring_radius);
            let spur_mid =
                Vec2::new(settlement.x, settlement.z) + to_route.normalize() * ring_radius * 1.35;
            assert!(
                distance_to_speed_road_network(spur_mid.x, spur_mid.y, 42) < 8.0,
                "{} should connect its ring back to the mountain trunk",
                settlement.name
            );
        }
    }

    #[test]
    fn speed_roads_include_loop_gates_for_routes_and_settlements() {
        assert!(speed_road_loop_gate_count() >= map_settlements().len() + 6);
    }

    #[test]
    fn speed_roads_populate_full_vertical_loops() {
        assert!(
            full_speed_road_loop_count() >= 6,
            "authored road spans should qualify multiple full boost loops"
        );
    }

    #[test]
    fn mountain_ranges_spawn_a_pack_of_giant_spider_mechs() {
        assert_eq!(mountain_spider_mech_spawn_count(), 8);
        for &(_, x, z, patrol_radius) in MOUNTAIN_SPIDER_MECH_SPAWNS {
            assert!(x.abs() > 2_000.0 || z.abs() > 2_000.0);
            assert!(patrol_radius >= 350.0);
        }
    }

    #[test]
    fn spider_mech_terrain_normal_is_stable_and_upward() {
        let normal = mountain_spider_terrain_normal(-5_200.0, 4_300.0, 42);
        assert!((normal.length() - 1.0).abs() < 0.001);
        assert!(normal.y > 0.2);
    }

    #[test]
    fn water_network_populates_lakes_and_reachable_islands() {
        assert_eq!(mountain_lake_count(), 6);
        assert_eq!(perimeter_travel_island_count(), 4);
        for island in TRAVEL_ISLANDS {
            assert!(island.center.length() > EVEREST_RANGE_HALF_EXTENT * 0.9);
            assert!(island.coast_dock.distance(island.island_dock) > 1_000.0);
            assert!(island.radius >= 170.0);
            let coast_y = terrain_surface_y(island.coast_dock.x, island.coast_dock.y, 42_195);
            assert!(
                (coast_y - PERIMETER_OCEAN_LEVEL).abs() < 4.0,
                "{} needs a walkable shoreline at its ferry dock",
                island.label
            );
        }
    }

    #[test]
    fn waterfall_splash_zone_respects_radius_and_vertical_bounds() {
        assert!(waterfall_zone_contains(Vec3::new(4.0, 3.0, 2.0), 8.0, 12.0));
        assert!(!waterfall_zone_contains(
            Vec3::new(8.1, 3.0, 0.0),
            8.0,
            12.0
        ));
        assert!(!waterfall_zone_contains(
            Vec3::new(0.0, 12.1, 0.0),
            8.0,
            12.0
        ));
        assert!(!waterfall_zone_contains(
            Vec3::new(0.0, -1.6, 0.0),
            8.0,
            12.0
        ));
    }

    #[test]
    fn mountain_lakes_find_bounded_downhill_outlets() {
        let mut outlet_count = 0;
        for (index, &(_, x, z, _, _)) in MOUNTAIN_LAKES.iter().enumerate() {
            let source = Vec2::new(x, z);
            assert!(
                terrain_surface_y(x, z, 42) < mountain_lake_surface_y(x, z, 42) - 3.5,
                "lake center needs a visible basin below its waterline"
            );
            let direction = Vec2::from_angle(index as f32);
            if let Some((next, next_height)) =
                downhill_stream_step(source, direction, 42, index as u64)
            {
                outlet_count += 1;
                assert!((next - source).length() <= 125.01);
                assert!(next_height <= terrain_surface_y(x, z, 42) + 1.5);
            }
        }
        assert!(outlet_count >= 4);
    }

    #[test]
    fn speed_roads_are_wide_enough_for_patrol_traffic() {
        const { assert!(SPEED_ROAD_WIDTH >= 60.0) };
        const { assert!(SPEED_ROAD_CHUNK_LEN <= 32.0) };
        const { assert!(SPEED_ROAD_TRAFFIC_LANE_OFFSET * 2.0 < SPEED_ROAD_WIDTH) };
        assert_eq!(SPEED_ROAD_PATROL_VEHICLE_COUNT, 18);
    }

    #[test]
    fn mountain_speed_road_centerlines_round_authored_corners() {
        let route = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)];
        let rounded = rounded_speed_road_route(&route);

        assert!(rounded.len() > route.len());
        assert_eq!(rounded.first().copied(), route.first().copied());
        assert_eq!(rounded.last().copied(), route.last().copied());
        assert!(rounded.iter().any(|&(x, z)| {
            (x > 1.0 && x < 99.0 && z.abs() > 0.1)
                || (z > 1.0 && z < 99.0 && (x - 100.0).abs() > 0.1)
        }));
    }

    #[test]
    fn rounded_corners_cut_deep_arcs_not_shallow_bevels() {
        // Mountain-scale right angle: a genuinely round corner must depart
        // far more than the old 18 m chord clamp allowed.
        let route = [(0.0, 0.0), (600.0, 0.0), (600.0, 600.0)];
        let rounded = rounded_speed_road_route(&route);
        let apex = Vec2::new(600.0, 0.0);
        let apex_clearance = rounded
            .iter()
            .map(|&(x, z)| Vec2::new(x, z).distance(apex))
            .fold(f32::MAX, f32::min);
        assert!(
            apex_clearance > 25.0,
            "corner arc should cut inside the apex, got {apex_clearance}"
        );
        assert!(apex_clearance < 300.0, "arc must stay near the corner");

        // Adaptive density keeps direction changes gradual through the turn.
        let mut max_turn_degrees = 0.0_f32;
        for window in rounded.windows(3) {
            let a = Vec2::new(window[0].0, window[0].1);
            let b = Vec2::new(window[1].0, window[1].1);
            let c = Vec2::new(window[2].0, window[2].1);
            let d1 = (b - a).normalize_or_zero();
            let d2 = (c - b).normalize_or_zero();
            max_turn_degrees =
                max_turn_degrees.max(d1.dot(d2).clamp(-1.0, 1.0).acos().to_degrees());
        }
        assert!(
            max_turn_degrees < 25.0,
            "per-segment turn should stay gradual, got {max_turn_degrees}°"
        );
    }

    #[test]
    fn centripetal_rounding_stays_bounded_on_uneven_spacing() {
        // Uniform Catmull-Rom overshoots when leg lengths differ wildly;
        // the centripetal form must stay inside the control hull's box.
        let route = [(0.0, 0.0), (60.0, 0.0), (960.0, 40.0), (980.0, 640.0)];
        let rounded = rounded_speed_road_route(&route);
        // Centripetal CR guarantees no cusps or loops, not strict hull
        // containment — asymmetric knot spacing may bow a few meters wide
        // on long spans (~1-2% here). Bound it at 2.5% of the route extent,
        // far under the uniform parameterization's overshoot on this data.
        for &(x, z) in &rounded {
            assert!(
                (-24.0..=1_004.0).contains(&x) && (-24.0..=664.0).contains(&z),
                "sample ({x}, {z}) escaped the control-point bounds"
            );
        }
    }

    #[test]
    fn route_projection_tracks_the_rounded_centerline() {
        // The distance field must agree with the rounded deck geometry:
        // every rounded sample of every route sits on the network.
        let (route, _) = &rounded_mountain_route_network()[0];
        let mid = route[route.len() / 2];
        let distance = distance_to_mountain_route_network(mid.0, mid.1);
        assert!(
            distance < 1.0,
            "rounded centerline point should project onto the network, got {distance}"
        );
    }

    #[test]
    fn speed_road_lift_keeps_sampled_decks_above_heightmap() {
        let terrain_seed = 42;
        for pair in ROUTE_CORE_EVEREST.windows(2) {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let delta = Vec2::new(bx - ax, bz - az);
            let chunk_count = speed_road_segment_subdivision_count(delta.length());
            for chunk in 0..chunk_count {
                let t0 = chunk as f32 / chunk_count as f32;
                let t1 = (chunk + 1) as f32 / chunk_count as f32;
                let mut start = speed_road_terrain_point(
                    ax + (bx - ax) * t0,
                    az + (bz - az) * t0,
                    terrain_seed,
                    SPEED_ROAD_CLEARANCE,
                );
                let mut end = speed_road_terrain_point(
                    ax + (bx - ax) * t1,
                    az + (bz - az) * t1,
                    terrain_seed,
                    SPEED_ROAD_CLEARANCE,
                );
                let lift = speed_road_required_terrain_lift(
                    start,
                    end,
                    SPEED_ROAD_WIDTH,
                    terrain_seed,
                    SPEED_ROAD_CLEARANCE + 1.0,
                );
                start.y += lift;
                end.y += lift;
                assert_speed_road_clearance_samples(
                    start,
                    end,
                    SPEED_ROAD_WIDTH,
                    terrain_seed,
                    SPEED_ROAD_CLEARANCE + SPEED_ROAD_TERRAIN_ENVELOPE - 0.02,
                );
            }
        }
    }

    #[test]
    fn complete_road_profiles_clear_mountains_and_share_junction_heights() {
        let terrain_seed = 42;
        let profiles = speed_road_network_profiles(terrain_seed);
        assert_eq!(profiles.len(), mountain_routes().len());

        let mut station_heights: HashMap<(i32, i32), Vec<f32>> = HashMap::new();
        for (route_index, profile) in profiles.iter().enumerate() {
            let width = speed_road_width_for_route(route_index);
            for point in profile {
                station_heights
                    .entry((point.x.round() as i32, point.z.round() as i32))
                    .or_default()
                    .push(point.y);
            }
            for points in profile.windows(2) {
                assert_speed_road_clearance_samples(
                    points[0],
                    points[1],
                    width,
                    terrain_seed,
                    SPEED_ROAD_CLEARANCE + SPEED_ROAD_TERRAIN_ENVELOPE - 0.02,
                );
                let horizontal =
                    Vec2::new(points[1].x - points[0].x, points[1].z - points[0].z).length();
                assert!(
                    (points[1].y - points[0].y).abs() <= SPEED_ROAD_MAX_GRADE * horizontal + 0.01
                );
            }
        }

        let shared_stations: Vec<&Vec<f32>> = station_heights
            .values()
            .filter(|heights| heights.len() > 1)
            .collect();
        assert!(shared_stations.len() >= 12);
        for heights in shared_stations {
            let low = heights.iter().copied().fold(f32::INFINITY, f32::min);
            let high = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            assert!(high - low <= 0.01, "road junction must be vertically flush");
        }

        let total_stations = profiles
            .iter()
            .map(|profile| profile.len().saturating_sub(1))
            .sum::<usize>();
        let aerial_support_stations = profiles
            .iter()
            .flat_map(|profile| profile.windows(2))
            .filter(|points| {
                let midpoint = points[0].lerp(points[1], 0.5);
                midpoint.y - terrain_surface_y(midpoint.x, midpoint.z, terrain_seed) >= 8.0
            })
            .count();
        assert!(
            aerial_support_stations >= 40,
            "the elevated network should produce frequent structural pylons"
        );
        // Quadruple-width racing corridors span multiple terrain ridges at
        // once, so the mountain network is intentionally a mostly elevated,
        // supported Mario-Kart-style viaduct. Preserve periodic grade contact
        // for approaches and transfers without forcing unsafe ridge clipping.
        assert!(
            total_stations - aerial_support_stations >= total_stations * 4 / 100,
            "roads still need periodic terrain approaches: {aerial_support_stations}/{total_stations} stations are aerial"
        );
    }

    #[test]
    fn scaled_travel_colliders_keep_world_space_extents() {
        assert_eq!(
            world_space_collider_scale(),
            crate::engine::physics::prelude::ColliderScale::Absolute(Vec3::ONE)
        );
    }

    #[test]
    fn mountain_routes_generate_sweeper_curves_and_trick_ramps() {
        assert!(speed_road_sweeper_curve_count() >= 20);
        assert!(hoverboard_trick_ramp_count() >= 16);
        assert!(stunt_grind_rail_count() >= 12);
        assert!(stunt_grind_rail_count() <= STUNT_GRIND_RAIL_LIMIT);
    }

    #[test]
    fn grind_rail_projection_clamps_to_authored_line() {
        let rail = StuntGrindRail {
            start: Vec3::ZERO,
            end: Vec3::new(10.0, 5.0, 0.0),
            speed: 60.0,
            snap_radius: 3.0,
            exit_lift: 9.0,
        };
        let (progress, closest, distance) =
            closest_point_on_stunt_rail(Vec3::new(5.0, 4.5, 2.0), &rail);
        assert!((0.5..0.7).contains(&progress));
        assert!(distance < 3.0);
        let (before, _, _) = closest_point_on_stunt_rail(Vec3::new(-30.0, 0.0, 0.0), &rail);
        let (after, _, _) = closest_point_on_stunt_rail(Vec3::new(30.0, 8.0, 0.0), &rail);
        assert_eq!(before, 0.0);
        assert_eq!(after, 1.0);
        assert!(closest.x > 5.0);
    }

    #[test]
    fn players_attach_to_grind_rails_and_can_jump_off() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.add_message::<ModularActionSfxEvent>();
        app.add_systems(Update, stunt_grind_rail_system);
        let rail = app
            .world_mut()
            .spawn(StuntGrindRail {
                start: Vec3::ZERO,
                end: Vec3::X * 30.0,
                speed: 60.0,
                snap_radius: 3.0,
                exit_lift: 10.0,
            })
            .id();
        let player = app
            .world_mut()
            .spawn((
                Player,
                PlayerIndex(0),
                Transform::from_xyz(4.0, 1.1, 0.5),
                PlayerInput::default(),
                PlayerMovement::default(),
                TraversalModeState::default(),
                PlayerStateMachine::default(),
            ))
            .id();

        app.update();
        assert_eq!(
            app.world().get::<RailGrindState>(player).unwrap().rail,
            rail
        );
        assert_eq!(
            app.world()
                .get::<TraversalModeState>(player)
                .unwrap()
                .active,
            TraversalMode::Hoverboard
        );

        app.world_mut().get_mut::<PlayerInput>(player).unwrap().jump = true;
        app.update();
        assert!(app.world().get::<RailGrindState>(player).is_none());
        assert!(
            app.world()
                .get::<PlayerMovement>(player)
                .unwrap()
                .velocity
                .y
                >= 10.0
        );
    }

    #[test]
    fn active_vehicles_do_not_attach_to_hoverboard_grind_rails() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.add_message::<ModularActionSfxEvent>();
        app.add_systems(Update, stunt_grind_rail_system);
        let mut vehicles = VehicleState::default();
        vehicles.active_owner = Some(0);
        vehicles.ground_mode = crate::plugins::vehicle_plugin::GroundMode::Tank;
        app.insert_resource(vehicles);
        app.world_mut().spawn(StuntGrindRail {
            start: Vec3::ZERO,
            end: Vec3::X * 30.0,
            speed: 60.0,
            snap_radius: 3.0,
            exit_lift: 10.0,
        });
        let player = app
            .world_mut()
            .spawn((
                Player,
                PlayerIndex(0),
                Transform::from_xyz(4.0, 1.1, 0.5),
                PlayerInput::default(),
                PlayerMovement::default(),
                TraversalModeState {
                    active: TraversalMode::Vehicle,
                    ..default()
                },
                PlayerStateMachine::default(),
            ))
            .id();

        app.update();
        assert!(app.world().get::<RailGrindState>(player).is_none());
        assert_eq!(
            app.world()
                .get::<TraversalModeState>(player)
                .unwrap()
                .active,
            TraversalMode::Vehicle
        );
    }

    #[test]
    fn road_boost_pad_cooldowns_are_per_player_and_vehicle_safe() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.add_systems(Update, board_boost_pad_system);
        let mut vehicles = VehicleState::default();
        vehicles.active_owner = Some(0);
        vehicles.ground_mode = crate::plugins::vehicle_plugin::GroundMode::Tank;
        app.insert_resource(vehicles);
        let pad = app
            .world_mut()
            .spawn((
                Transform::default(),
                BoardBoostPad {
                    direction: Vec3::X,
                    half_width: 8.0,
                    half_length: 8.0,
                    speed_mult: 3.0,
                    impulse: 4.0,
                    lift: 0.0,
                    duration: 1.5,
                    cooldown: 0.18,
                    cooldown_timers: [0.0; 4],
                    force_hoverboard: true,
                },
            ))
            .id();
        let players = (0..2_u8)
            .map(|index| {
                app.world_mut()
                    .spawn((
                        PlayerIndex(index),
                        Transform::from_xyz(index as f32, 0.5, 0.0),
                        PlayerProgression::default(),
                        PlayerMovement::default(),
                        TraversalModeState {
                            active: if index == 0 {
                                TraversalMode::Vehicle
                            } else {
                                TraversalMode::Grapple
                            },
                            ..default()
                        },
                        BoardBoostState::default(),
                        PlayerStateMachine::default(),
                    ))
                    .id()
            })
            .collect::<Vec<_>>();

        app.update();

        let vehicle_traversal = app.world().get::<TraversalModeState>(players[0]).unwrap();
        let on_foot_traversal = app.world().get::<TraversalModeState>(players[1]).unwrap();
        assert_eq!(vehicle_traversal.active, TraversalMode::Vehicle);
        assert_eq!(on_foot_traversal.active, TraversalMode::Hoverboard);
        for player in players {
            assert!(app.world().get::<BoardBoostState>(player).unwrap().timer > 0.0);
            assert!(
                app.world()
                    .get::<PlayerMovement>(player)
                    .unwrap()
                    .ground_velocity
                    .x
                    > 0.0
            );
        }
        assert!(app
            .world()
            .get::<BoardBoostPad>(pad)
            .unwrap()
            .cooldown_timers[..2]
            .iter()
            .all(|timer| *timer > 0.0));
    }

    #[test]
    fn sonic_springs_launch_the_whole_local_party_on_contact() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.add_message::<ModularActionSfxEvent>();
        app.add_systems(Update, spring_jump_pad_system);
        app.world_mut().spawn((
            Transform::default(),
            SpringJumpPad {
                launch_velocity: Vec3::new(30.0, 18.0, 0.0),
                radius: 3.5,
                cooldown: 0.16,
                cooldown_timers: [0.0; 4],
                force_hoverboard: true,
            },
        ));
        let players = (0..4)
            .map(|index| {
                app.world_mut()
                    .spawn((
                        PlayerIndex(index),
                        Transform::from_xyz(index as f32 * 0.4, 1.0, 0.0),
                        PlayerMovement::default(),
                        TraversalModeState::default(),
                        PlayerStateMachine::default(),
                    ))
                    .id()
            })
            .collect::<Vec<_>>();

        app.update();
        for player in players {
            let movement = app.world().get::<PlayerMovement>(player).unwrap();
            assert_eq!(movement.ground_velocity, Vec3::X * 30.0);
            assert!(movement.velocity.y >= 18.0);
            assert_eq!(
                app.world()
                    .get::<TraversalModeState>(player)
                    .unwrap()
                    .active,
                TraversalMode::Hoverboard
            );
        }
    }

    #[test]
    fn sonic_spring_cooldowns_do_not_block_staggered_party_arrivals() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.add_message::<ModularActionSfxEvent>();
        app.add_systems(Update, spring_jump_pad_system);
        let pad = app
            .world_mut()
            .spawn((
                Transform::default(),
                SpringJumpPad {
                    launch_velocity: Vec3::new(24.0, 16.0, 0.0),
                    radius: 3.0,
                    cooldown: 0.5,
                    cooldown_timers: [0.0; 4],
                    force_hoverboard: false,
                },
            ))
            .id();
        let leader = app
            .world_mut()
            .spawn((
                PlayerIndex(0),
                Transform::from_xyz(0.0, 1.0, 0.0),
                PlayerMovement::default(),
                TraversalModeState::default(),
                PlayerStateMachine::default(),
            ))
            .id();
        let follower = app
            .world_mut()
            .spawn((
                PlayerIndex(1),
                Transform::from_xyz(10.0, 1.0, 0.0),
                PlayerMovement::default(),
                TraversalModeState::default(),
                PlayerStateMachine::default(),
            ))
            .id();

        app.update();
        assert_eq!(
            app.world()
                .get::<PlayerMovement>(leader)
                .unwrap()
                .ground_velocity,
            Vec3::X * 24.0
        );

        app.world_mut()
            .get_mut::<Transform>(leader)
            .unwrap()
            .translation
            .x = 10.0;
        app.world_mut()
            .get_mut::<Transform>(follower)
            .unwrap()
            .translation
            .x = 0.0;
        app.update();

        assert_eq!(
            app.world()
                .get::<PlayerMovement>(follower)
                .unwrap()
                .ground_velocity,
            Vec3::X * 24.0
        );
        let timers = &app
            .world()
            .get::<SpringJumpPad>(pad)
            .unwrap()
            .cooldown_timers;
        assert!(timers[0] > 0.0);
        assert!(timers[1] > 0.0);
    }

    #[test]
    fn trick_buttons_map_to_categories_in_tony_hawk_order() {
        let mut input = PlayerInput::default();
        assert_eq!(requested_trick_category(&input), None);
        input.melee_light = true;
        assert_eq!(requested_trick_category(&input), Some(TrickCategory::Flip));
        input.melee_light = false;
        input.melee_heavy = true;
        assert_eq!(requested_trick_category(&input), Some(TrickCategory::Grab));
        input.melee_heavy = false;
        input.dodge = true;
        assert_eq!(requested_trick_category(&input), Some(TrickCategory::Grind));
        input.dodge = false;
        input.parry = true;
        assert_eq!(
            requested_trick_category(&input),
            Some(TrickCategory::Manual)
        );
    }

    #[test]
    fn trick_categories_require_their_surface() {
        // Air tricks: airborne and off a rail.
        assert!(trick_category_available(TrickCategory::Grab, false, false));
        assert!(!trick_category_available(TrickCategory::Grab, true, false));
        assert!(!trick_category_available(TrickCategory::Flip, false, true));
        // Grinds need a rail.
        assert!(trick_category_available(TrickCategory::Grind, false, true));
        assert!(!trick_category_available(
            TrickCategory::Grind,
            false,
            false
        ));
        // Manuals need ground, not a rail.
        assert!(trick_category_available(TrickCategory::Manual, true, false));
        assert!(!trick_category_available(
            TrickCategory::Manual,
            false,
            false
        ));
        assert!(!trick_category_available(TrickCategory::Manual, true, true));
    }

    #[test]
    fn combo_banks_only_once_nothing_is_linking_anymore() {
        // Mirrors the bank gate in stunt_combo_system: grounded, off a rail,
        // and neither a held trick nor an open revert window.
        let bankable = |grounded: bool, grinding: bool, tricks: &TrickState| {
            grounded && !grinding && !(tricks.can_revert() || tricks.active.is_some())
        };

        let idle = TrickState::default();
        assert!(bankable(true, false, &idle), "plain landing banks");
        assert!(!bankable(false, false, &idle), "still airborne");
        assert!(!bankable(true, true, &idle), "still grinding");

        // An open revert window holds the bank so the combo survives.
        let reverting = TrickState {
            revert_window: 0.4,
            ..default()
        };
        assert!(!bankable(true, false, &reverting));

        // Once it lapses on the ground the run banks — this is why the gate
        // is state-based, not a landing edge (the rider is already grounded).
        let lapsed = TrickState {
            revert_window: 0.0,
            ..default()
        };
        assert!(bankable(true, false, &lapsed));
    }

    #[test]
    fn landed_trick_score_applies_spin_bonus_and_repeat_decay() {
        // Clean first landing, no spin: base score.
        assert!((landed_trick_score(400.0, 0.0, 0) - 400.0).abs() < 1e-3);
        // A full rotation doubles it.
        assert!((landed_trick_score(400.0, 360.0, 0) - 800.0).abs() < 1e-3);
        // Repeating the same trick in one combo is worth less each time.
        let first = landed_trick_score(400.0, 0.0, 0);
        let second = landed_trick_score(400.0, 0.0, 1);
        let third = landed_trick_score(400.0, 0.0, 2);
        let fourth = landed_trick_score(400.0, 0.0, 3);
        assert!(second < first && third < second && fourth < third);
        assert!((second - 400.0 / 1.33).abs() < 1e-3);
        assert!((fourth - 200.0).abs() < 1e-3);
    }

    #[test]
    fn stunt_combo_banks_rotations_and_multiplier_only_on_landing() {
        let mut run = StuntRunState {
            pending_score: 1_000.0,
            multiplier: 2.0,
            spin_degrees: 720.0,
            active: true,
            ..default()
        };
        assert_eq!(run.total_score, 0);
        let banked = bank_stunt_run(&mut run);
        assert_eq!(banked, 3_300);
        assert_eq!(run.total_score, 3_300);
        assert!(!run.active);
        assert_eq!(run.pending_score, 0.0);
        assert_eq!(run.multiplier, 1.0);
    }

    #[test]
    fn settlement_races_supply_four_gates_and_two_rivals_each() {
        assert_eq!(stunt_race_gate_count(), map_settlements().len() * 4);
        assert_eq!(stunt_race_opponent_count(), map_settlements().len() * 2);
        assert!(stunt_race_gate_count() >= 32);
        assert!(stunt_race_opponent_count() >= 16);
    }

    #[test]
    fn three_lap_race_requires_every_gate_in_order() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.add_message::<UiMessageEvent>();
        app.add_systems(Update, stunt_race_gate_system);
        for gate_index in 0..4u8 {
            app.world_mut().spawn((
                Transform::from_xyz(gate_index as f32 * 20.0, 0.0, 0.0),
                StuntRaceGate {
                    course_id: "test_ring",
                    course_label: "Test Ring",
                    gate_index,
                    gate_count: 4,
                    radius: 4.0,
                },
            ));
        }
        let player = app
            .world_mut()
            .spawn((
                Player,
                PlayerIndex(0),
                Transform::default(),
                TraversalModeState {
                    active: TraversalMode::Hoverboard,
                    ..default()
                },
                StuntRaceProgress::default(),
            ))
            .id();

        let cross_gate = |app: &mut App, gate_index: u8| {
            app.world_mut()
                .get_mut::<Transform>(player)
                .unwrap()
                .translation = Vec3::X * (gate_index as f32 * 20.0);
            app.world_mut()
                .get_mut::<StuntRaceProgress>(player)
                .unwrap()
                .gate_cooldown = 0.0;
            app.update();
        };

        cross_gate(&mut app, 0);
        assert_eq!(app.world().get::<StuntRaceProgress>(player).unwrap().lap, 1);
        // Gate 2 cannot be skipped while gate 1 is expected.
        cross_gate(&mut app, 2);
        assert_eq!(
            app.world()
                .get::<StuntRaceProgress>(player)
                .unwrap()
                .next_gate,
            1
        );
        for lap in 1..=3u8 {
            cross_gate(&mut app, 1);
            cross_gate(&mut app, 2);
            cross_gate(&mut app, 3);
            cross_gate(&mut app, 0);
            if lap < 3 {
                assert_eq!(
                    app.world().get::<StuntRaceProgress>(player).unwrap().lap,
                    lap + 1
                );
            }
        }
        let progress = app.world().get::<StuntRaceProgress>(player).unwrap();
        assert_eq!(progress.races_finished, 1);
        assert!(progress.course_id.is_none());
    }

    #[test]
    fn sustained_sky_roads_receive_ground_access_corridors() {
        let terrain_seed = 42;
        let corridors = sky_road_access_corridors(terrain_seed);
        assert!(
            corridors.len() >= 20,
            "the aerial network needs frequent driveable access ramps"
        );
        for &(outer, merge) in corridors.iter() {
            let merge_xz = Vec2::new(merge.x, merge.z);
            let length = outer.distance(merge_xz);
            assert!(
                (119.0..=722.0).contains(&length),
                "unexpected sky access length {length}"
            );
            assert!(
                terrain_surface_y(outer.x, outer.y, terrain_seed) + 0.5 < merge.y,
                "sky access must rise from ground toward the road"
            );
            let midpoint = outer.lerp(merge_xz, 0.5);
            assert!(distance_to_speed_road_network(midpoint.x, midpoint.y, terrain_seed) < 0.1);
        }
    }

    #[test]
    fn city_street_loops_follow_the_avenue_grid_and_scale_by_ring() {
        for ring in 1..=4 {
            let path = city_street_loop(ring, 0.92);
            assert_eq!(path.len(), ring as usize * 8);
            for pair in path
                .iter()
                .copied()
                .zip(path.iter().copied().cycle().skip(1))
                .take(path.len())
            {
                let delta = pair.1 - pair.0;
                assert!(delta.x.abs() < 0.001 || delta.z.abs() < 0.001);
                assert!((delta.length() - 48.0).abs() < 0.001);
            }
        }
    }

    #[test]
    fn city_traffic_signals_stop_once_then_release_each_segment() {
        let mut vehicle = NpcRoadVehicle {
            path: vec![Vec3::ZERO, Vec3::Z * 48.0],
            segment: 0,
            progress: 47.0,
            speed: 14.0,
            lane_offset: 0.0,
            hit_radius: 2.0,
            wreck_timer: 4.0,
        };
        let mut traffic = CityStreetTraffic {
            cruise_speed: 14.0,
            stop_timer: 0.0,
            last_stopped_segment: usize::MAX,
            signal_phase: 0,
        };

        update_city_traffic_signal(&mut vehicle, &mut traffic, 0.0);
        assert_eq!(vehicle.speed, 0.0);
        assert!(traffic.stop_timer > 0.0);
        update_city_traffic_signal(&mut vehicle, &mut traffic, 2.0);
        update_city_traffic_signal(&mut vehicle, &mut traffic, 0.0);
        assert_eq!(vehicle.speed, traffic.cruise_speed);
    }

    #[test]
    fn pedestrian_routes_wrap_without_leaving_the_sidewalk_loop() {
        let path = city_sidewalk_loop(-2, 1, 1.12);
        let mut pedestrian = CityPedestrian {
            path: path.clone(),
            segment: 3,
            progress: path[3].distance(path[0]) - 0.1,
            speed: 2.0,
            pause_timer: 0.0,
            phase: 0.0,
        };

        advance_city_pedestrian(&mut pedestrian, 0.2);
        assert_eq!(pedestrian.segment, 0);
        let (position, _) = city_pedestrian_pose(&pedestrian).expect("closed sidewalk pose");
        let min_x = path
            .iter()
            .map(|point| point.x)
            .fold(f32::INFINITY, f32::min);
        let max_x = path
            .iter()
            .map(|point| point.x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x..=max_x).contains(&position.x));
    }

    #[test]
    fn road_vehicle_paths_return_along_authored_routes() {
        let path = road_vehicle_route_path(ROUTE_CORE_AURORA, 42);
        assert!(path.len() > ROUTE_CORE_AURORA.len() * 2);
        assert_eq!(path.first().map(|p| (p.x, p.z)), Some(ROUTE_CORE_AURORA[0]));
        assert_eq!(path.last().map(|p| (p.x, p.z)), Some(ROUTE_CORE_AURORA[0]));

        let max_horizontal_step = path
            .windows(2)
            .map(|pair| Vec2::new(pair[1].x - pair[0].x, pair[1].z - pair[0].z).length())
            .fold(0.0_f32, f32::max);
        assert!(max_horizontal_step <= SPEED_ROAD_CHUNK_LEN + 1.0);

        for point in path {
            let ground = terrain_surface_y(point.x, point.z, 42);
            assert!(point.y >= ground + SPEED_ROAD_CLEARANCE + 1.9);
        }
    }

    fn assert_speed_road_clearance_samples(
        start: Vec3,
        end: Vec3,
        width: f32,
        terrain_seed: u64,
        min_clearance: f32,
    ) {
        let delta_xz = Vec2::new(end.x - start.x, end.z - start.z);
        let horizontal_len = delta_xz.length();
        let dir = delta_xz / horizontal_len.max(0.001);
        let right = Vec2::new(dir.y, -dir.x);
        // Deliberately denser and independently shaped from the production
        // sampler so this test cannot repeat the same blind spots.
        let samples = (horizontal_len / 2.0).ceil().clamp(1.0, 512.0) as usize;
        for i in 0..=samples {
            let t = i as f32 / samples as f32;
            let center = Vec2::new(start.x + delta_xz.x * t, start.z + delta_xz.y * t);
            let road_y = start.y + (end.y - start.y) * t;
            for lateral_step in 0..=12 {
                let offset = -width * 0.5 + width * lateral_step as f32 / 12.0;
                let sample = center + right * offset;
                let ground = terrain_surface_y(sample.x, sample.y, terrain_seed);
                assert!(
                    road_y >= ground + min_clearance,
                    "speed road sample sank below terrain clearance at ({:.1}, {:.1}): road {:.3}, required {:.3}",
                    sample.x,
                    sample.y,
                    road_y,
                    ground + min_clearance,
                );
            }
        }
    }

    #[test]
    fn terrain_vertex_color_adds_high_snowline() {
        let low = terrain_vertex_color(900.0, 1400.0, 40.0, Vec3::Y, 42);
        let high = terrain_vertex_color(-8400.0, -7800.0, 780.0, Vec3::Y, 42);
        let route = ROUTE_CORE_AURORA[1];
        let path = terrain_vertex_color(route.0, route.1, 220.0, Vec3::Y, 42);

        assert!(high[0] > low[0]);
        assert!(high[2] > low[2]);
        assert!(path[3] > 0.99, "route vertices carry the shader path mask");
        assert_eq!(low[3], 0.0, "off-route terrain has no path mask");
    }

    #[test]
    fn dragon_lair_dungeons_cover_dragon_chapters() {
        let chapters: Vec<u8> = dragon_dungeon_specs()
            .iter()
            .map(|spec| spec.chapter)
            .collect();

        assert_eq!(chapters, vec![6, 7, 8, 9, 10, 11]);
    }

    #[test]
    fn great_scientist_temples_cover_core_mechanic_rewards() {
        let reward_ids: Vec<&str> = great_scientist_temple_specs()
            .iter()
            .map(|spec| spec.reward_id)
            .collect();

        assert_eq!(
            reward_ids,
            vec![
                "ancient_flight_core",
                "solar_sabre_glyph",
                "nova_missile_matrix",
                "aegis_armor_frame",
            ]
        );
    }

    #[test]
    fn great_scientist_temple_gates_are_unique() {
        let mut gate_ids: Vec<u8> = great_scientist_temple_specs()
            .iter()
            .map(|spec| spec.gate_id)
            .collect();
        gate_ids.sort_unstable();
        gate_ids.dedup();

        assert_eq!(gate_ids.len(), great_scientist_temple_specs().len());
    }

    #[test]
    fn great_scientist_temples_use_valid_story_chapters_not_mechanism_ids() {
        let specs = great_scientist_temple_specs();
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.story_chapter)
                .collect::<Vec<_>>(),
            vec![5, 8, 12, 14]
        );
        for spec in specs {
            assert!((1..=14).contains(&spec.story_chapter));
            assert_ne!(spec.story_chapter, spec.gate_id);
        }
    }

    #[test]
    fn peaceful_city_spies_exist_for_each_mega_city() {
        for city in map_settlements()
            .iter()
            .filter(|settlement| matches!(settlement.kind, MapSettlementKind::City))
        {
            assert_eq!(
                city_spy_drone_specs(city.anchor_id).len(),
                2,
                "{} should have two city spy drones",
                city.name
            );
        }
    }

    #[test]
    fn mega_city_layouts_float_above_terrain() {
        for city in map_settlements()
            .iter()
            .copied()
            .filter(|settlement| matches!(settlement.kind, MapSettlementKind::City))
        {
            let layout = settlement_layout_profile(42, city, settlement_plaza_radius(city.kind));

            assert_eq!(layout.mode, SettlementTerrainMode::SkyDistrict);
            assert!(layout.floor_y > layout.center_y + 80.0);
            assert_eq!(
                layout.floor_y_at(city.x + 32.0, city.z - 18.0, 42),
                layout.floor_y
            );
        }
    }

    #[test]
    fn settlement_floor_profiles_do_not_sink_below_terrain() {
        for settlement in map_settlements().iter().copied() {
            let layout =
                settlement_layout_profile(42, settlement, settlement_plaza_radius(settlement.kind));
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.0;
                let radius = layout.plaza_radius * 1.35;
                let x = settlement.x + angle.cos() * radius;
                let z = settlement.z + angle.sin() * radius;
                let ground = terrain_surface_y(x, z, 42);
                let floor = layout.floor_y_at(x, z, 42);

                assert!(floor + 0.01 >= ground);
            }
        }
    }

    #[test]
    fn rough_and_sky_settlements_request_mountain_insets() {
        let inset_count = map_settlements()
            .iter()
            .copied()
            .filter(|settlement| {
                let layout = settlement_layout_profile(
                    42,
                    *settlement,
                    settlement_plaza_radius(settlement.kind),
                );
                layout.needs_mountain_inset(*settlement)
            })
            .count();

        assert!(inset_count >= 2);
    }

    #[test]
    fn peaceful_city_spy_data_rewards_are_unique() {
        let mut reward_ids: Vec<&str> = map_settlements()
            .iter()
            .flat_map(|settlement| city_spy_drone_specs(settlement.anchor_id))
            .map(|spec| spec.reward_id)
            .collect();
        let original_len = reward_ids.len();
        reward_ids.sort_unstable();
        reward_ids.dedup();

        assert_eq!(reward_ids.len(), original_len);
        assert_eq!(original_len, 4);
    }

    #[test]
    fn enterable_building_floors_stay_navigable_and_bounded() {
        let (small_floors, small_story) = enterable_floor_layout(12.0);
        let (tower_floors, tower_story) = enterable_floor_layout(140.0);

        assert!(small_floors >= 2);
        assert!((4.5..=6.8).contains(&small_story));
        assert_eq!(tower_floors, 12);
        assert!((4.5..=6.8).contains(&tower_story));
    }

    #[test]
    fn city_access_plan_aligns_balconies_and_scales_roof_ladders() {
        let (house_floors, house_story) = enterable_floor_layout(18.0);
        let (tower_floors, tower_story) = enterable_floor_layout(90.0);
        let house = city_access_plan(14.0, 18.0, house_floors, house_story);
        let tower = city_access_plan(26.0, 90.0, tower_floors, tower_story);

        assert_eq!(house.balcony_floor, 1);
        assert!((house.balcony_y - (house_story + 0.18)).abs() < 0.001);
        assert!((6.5..=11.0).contains(&house.balcony_width));
        assert!((7.5..=13.0).contains(&house.stair_run));
        assert!(tower.ladder_rungs > house.ladder_rungs);
    }

    #[test]
    fn building_interior_kinds_match_district_identity() {
        assert_eq!(
            building_interior_kind(WorldZone::Industrial, 11),
            BuildingInteriorKind::Laboratory
        );
        assert_eq!(
            building_interior_kind(WorldZone::Residential, 11),
            BuildingInteriorKind::Home
        );
        assert_eq!(
            building_interior_kind(WorldZone::Downtown, 12),
            BuildingInteriorKind::Market
        );
        let outer_kinds = (0..3)
            .map(|seed| building_interior_kind(WorldZone::OuterDistrict, seed))
            .collect::<Vec<_>>();
        assert_eq!(
            outer_kinds,
            vec![
                BuildingInteriorKind::Home,
                BuildingInteriorKind::Market,
                BuildingInteriorKind::Laboratory,
            ]
        );
    }

    #[test]
    fn roof_hatches_stay_inside_qualifying_building_roofs() {
        for (depth, opening_width) in [(11.5, 3.2), (18.0, 4.1), (30.0, 4.6)] {
            let hatch = building_roof_hatch_size(depth, opening_width);
            assert!(hatch.x >= 2.7 && hatch.x < opening_width);
            assert!(hatch.y > 0.0 && hatch.y <= 3.2);
            assert!(hatch.y < depth - 1.0);
        }
    }

    #[test]
    fn building_rewards_alternate_between_interiors_and_rooftops() {
        let rooftop = building_reward_plan(WorldZone::Downtown, 12, 20.0, 48.0, 18.0, 8, 5.6);
        let interior = building_reward_plan(WorldZone::Residential, 13, 16.0, 22.0, 14.0, 4, 5.3);

        assert_eq!(rooftop.location, BuildingRewardLocation::Rooftop);
        assert!(rooftop.local_position.y > 48.0);
        assert_eq!(interior.location, BuildingRewardLocation::Interior);
        assert!(interior.local_position.y > 5.3 && interior.local_position.y < 22.0);
        assert!(rooftop.credits > interior.credits);
        assert!((4..=10).contains(&interior.armor));
    }

    #[test]
    fn building_reward_keys_are_stable_and_location_unique() {
        let base = Vec3::new(12.25, 0.0, -83.75);
        let first = building_reward_key(WorldZone::Downtown, base);
        assert_eq!(first, building_reward_key(WorldZone::Downtown, base));
        assert_ne!(
            first,
            building_reward_key(WorldZone::Downtown, base + Vec3::X)
        );
        assert_ne!(first, building_reward_key(WorldZone::Industrial, base));
    }

    #[test]
    fn building_rewards_grant_stats_without_exceeding_caps() {
        let mut stats = PlayerStats {
            credits: u32::MAX - 2,
            experience: 10,
            armor: 98.0,
            ..default()
        };
        let mut health = Health::new(100.0);
        health.apply_damage(8.0);

        apply_building_reward(&mut stats, &mut health, 20, 30, 9);

        assert_eq!(stats.credits, u32::MAX);
        assert_eq!(stats.experience, 40);
        assert_eq!(stats.armor, stats.max_armor);
        assert!(health.current > 92.0 && health.current <= health.max);
    }

    #[test]
    fn building_cluster_cells_handle_negative_world_coordinates() {
        assert_eq!(building_cluster_cell(Vec3::ZERO, 800.0), (0, 0));
        assert_eq!(
            building_cluster_cell(Vec3::new(799.9, 0.0, -0.1), 800.0),
            (0, -1)
        );
        assert_eq!(
            building_cluster_cell(Vec3::new(-800.0, 0.0, 1_600.0), 800.0),
            (-1, 2)
        );
    }

    #[test]
    fn building_clusters_reject_transparent_materials() {
        let opaque = StandardMaterial::default();
        let transparent = StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..default()
        };

        assert!(building_material_is_clusterable(Some(&opaque)));
        assert!(!building_material_is_clusterable(Some(&transparent)));
        assert!(!building_material_is_clusterable(None));
    }

    #[test]
    fn terrain_patch_tiers_share_origins_and_reduce_geometry() {
        assert!(TERRAIN_MESH_RESOLUTION.is_multiple_of(TERRAIN_PATCH_CELLS));
        assert!(TERRAIN_PATCH_CELLS.is_multiple_of(TERRAIN_PROXY_SAMPLE_STEP));
        let stride = TERRAIN_MESH_RESOLUTION + 1;
        let heights = vec![0.0; stride * stride];
        let (high, high_origin) = terrain_patch_mesh(&heights, 42, 0, 0, TERRAIN_PATCH_CELLS, 1);
        let (proxy, proxy_origin) = terrain_patch_mesh(
            &heights,
            42,
            0,
            0,
            TERRAIN_PATCH_CELLS,
            TERRAIN_PROXY_SAMPLE_STEP,
        );

        assert_eq!(high_origin, proxy_origin);
        assert_eq!(high.count_vertices(), 41 * 41);
        assert_eq!(proxy.count_vertices(), 11 * 11);
        assert!(proxy.count_vertices() * 10 < high.count_vertices());
        assert_eq!(high.indices().unwrap().len(), 40 * 40 * 6);
        assert_eq!(proxy.indices().unwrap().len(), 10 * 10 * 6);
    }

    #[test]
    fn seed_cache_is_bounded_and_refreshes_recent_entries() {
        let mut cache = BoundedSeedCache::new(2);
        cache.insert(1, Arc::new("one"));
        cache.insert(2, Arc::new("two"));
        assert_eq!(cache.get(1).as_deref(), Some(&"one"));

        cache.insert(3, Arc::new("three"));
        assert!(cache.get(2).is_none());
        assert_eq!(cache.get(1).as_deref(), Some(&"one"));
        assert_eq!(cache.get(3).as_deref(), Some(&"three"));
        assert_eq!(cache.values.len(), 2);
    }

    #[test]
    fn heavy_drone_patrols_stay_outside_the_starting_region() {
        assert!(!WASTELAND_DRONE_PATROLS.is_empty());
        for position in WASTELAND_DRONE_PATROLS {
            assert!(
                position.length() > STARTING_REGION_SAFE_RADIUS,
                "patrol at {position:?} intrudes on the protected start"
            );
        }
    }

    #[test]
    fn collapse_platform_warns_falls_and_resets_without_drift() {
        let origin = Vec3::new(4.0, 8.0, -3.0);
        let mut platform = CollapsePlatform {
            origin,
            size: Vec3::new(8.0, 0.5, 3.0),
            warning_seconds: 0.5,
            fallen_seconds: 0.8,
            drop_distance: 6.0,
            timer: 0.0,
            state: CollapsePlatformState::Armed,
        };
        let mut transform = Transform::from_translation(origin);

        advance_collapse_platform(&mut platform, &mut transform, true, 0.01);
        assert_eq!(platform.state, CollapsePlatformState::Warning);
        advance_collapse_platform(&mut platform, &mut transform, false, 0.6);
        assert_eq!(platform.state, CollapsePlatformState::Fallen);
        advance_collapse_platform(&mut platform, &mut transform, false, 0.9);
        assert_eq!(platform.state, CollapsePlatformState::Resetting);
        advance_collapse_platform(&mut platform, &mut transform, false, 0.5);
        assert_eq!(platform.state, CollapsePlatformState::Armed);
        assert_eq!(transform.translation, origin);
    }

    #[test]
    fn collapse_contact_requires_grounded_feet_on_the_deck() {
        let platform = Vec3::new(0.0, 4.0, 0.0);
        let size = Vec3::new(8.0, 0.5, 3.0);
        let grounded = PlayerMovement {
            is_grounded: true,
            ..default()
        };
        let airborne = PlayerMovement {
            is_grounded: false,
            ..default()
        };
        let supported_player = Vec3::new(0.0, platform.y + size.y * 0.5 + 0.95, 0.0);

        assert!(player_supported_by_platform(
            supported_player,
            &grounded,
            platform,
            size
        ));
        assert!(!player_supported_by_platform(
            supported_player,
            &airborne,
            platform,
            size
        ));
        assert!(!player_supported_by_platform(
            supported_player + Vec3::X * 8.0,
            &grounded,
            platform,
            size
        ));
    }

    #[test]
    fn flip_platform_debounces_jump_input_and_reaches_opposite_face() {
        let base_rotation = Quat::from_rotation_y(0.4);
        let mut panel = FlipPlatform {
            base_rotation,
            flipped: false,
            progress: 0.0,
            turn_seconds: 0.25,
            trigger_radius: 3.0,
            cooldown: 0.3,
            cooldown_timer: 0.0,
        };
        let mut transform = Transform::from_rotation(base_rotation);

        advance_flip_platform(&mut panel, &mut transform, true, 0.05);
        assert!(panel.flipped);
        advance_flip_platform(&mut panel, &mut transform, true, 0.05);
        assert!(panel.flipped, "held jump must not immediately toggle back");
        advance_flip_platform(&mut panel, &mut transform, false, 0.3);
        assert_eq!(panel.progress, 1.0);
        let expected = base_rotation * Quat::from_rotation_x(std::f32::consts::PI);
        assert!(transform.rotation.angle_between(expected) < 0.0001);
    }

    #[test]
    fn spike_contact_uses_bridge_footprint_and_player_feet() {
        let hazard = Vec3::new(2.0, 4.0, -3.0);
        let size = Vec3::new(12.0, 1.1, 3.2);
        let on_spikes = hazard + Vec3::new(0.0, size.y * 0.72 + 0.95, 0.0);

        assert!(spike_platform_contact(on_spikes, hazard, size));
        assert!(!spike_platform_contact(
            on_spikes + Vec3::X * 7.0,
            hazard,
            size
        ));
        assert!(!spike_platform_contact(
            on_spikes + Vec3::Y * 2.0,
            hazard,
            size
        ));
    }
}

