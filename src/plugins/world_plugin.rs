use bevy::asset::RenderAssetUsages;
use bevy::image::{
    CompressedImageFormats, Image, ImageAddressMode, ImageLoaderSettings, ImageSampler,
    ImageSamplerDescriptor, ImageType,
};
use bevy::math::Affine2;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureFormat,
};
use bevy::shader::ShaderRef;
use std::f32::consts::TAU;
use std::sync::OnceLock;

use crate::chapters::{
    chapter_map_locations, map_settlements, MapSettlement, MapSettlementKind,
    EVEREST_RANGE_HALF_EXTENT, EVEREST_RANGE_WORLD_SIZE,
};
use crate::commands::CommandRegistry;
use crate::components::armor::ArmorSet;
use crate::components::discoverable::DiscoverableKind;
use crate::components::enemy::{CitySpyDrone, Enemy, EnemyStateMachine, EnemyType};
use crate::components::faction::Faction;
use crate::components::player::{
    BoardBoostState, ParryState, Player, PlayerIndex, PlayerInput, PlayerMovement,
    PlayerStateMachine, PlayerStats, TraversalMode, TraversalModeState,
};
use crate::components::world::*;
use crate::damage::{DamageInfo, DamageType, Damageable, Health};
use crate::discussion::{
    discussion_script, settlement_discussion_id, settlement_guardian_role, DiscussionState,
};
use crate::events::{PlayerDamagedEvent, PlayerParryEvent, UiMessageEvent};
use crate::final_war::{
    coordinated_raid_trigger_system, final_war_phase_announce_system, pressure_accumulation_system,
    win_condition_system,
};
use crate::lsystem::tree::{spawn_tree, TreeKind, TreeRoot, TreeTemplate};
use crate::plugins::chapter_plugin::spawn_discoverable_beacon;
use crate::plugins::enemy_plugin::spawn_enemy_entity;
use crate::raids::{
    static_defense_score_for_site, RaidId, RaidKind, RaidRegistry, RaidStartResult,
    RaidThreatMarker, RaidTickEvent, RaidUfoMarker, CLOUDRAIL_TUTORIAL_RAID_SITE,
};
use crate::rendering::{DirectionalLightBundle, PbrBundle, PointLightBundle};
use crate::resources::{
    initial_world_routes, initial_world_sites, ChapterProgress, CurrentChapter, DungeonCrawlState,
    DungeonRoomState, GameSettings, PlaySessionTransition, WorldRouteRegistry, WorldRouteState,
    WorldSiteRegistry, WorldSiteState,
};
use crate::robot_pets::RobotPetCollection;
use crate::settlement_economy::{settlement_build_def, SettlementBuildKind, SettlementEconomy};
use crate::state::AppState;

#[derive(Resource, Default)]
struct ColliderDebugState {
    enabled: bool,
}

#[derive(Component)]
struct ColliderDebugSource;

#[derive(Component)]
struct ColliderDebugVisual;

#[derive(Component, Debug, Clone, Copy)]
struct DebugColliderBox {
    size: Vec3,
}

/// Physics backends normally multiply collider shapes by `GlobalTransform` scale.
/// Scaled unit-cube roads/bridges below already specify collider half-extents
/// in world units, so this prevents invisible oversized blockers.
fn world_space_collider_scale() -> crate::physics::prelude::ColliderScale {
    crate::physics::prelude::ColliderScale::Absolute(Vec3::ONE)
}

// ── Grass wind material ───────────────────────────────────────────────────────

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct GrassMaterial {
    #[uniform(0)]
    color: Vec4, // linear RGBA packed for the WGSL uniform
}

impl Material for GrassMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/grass.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/grass.wgsl".into()
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None; // render both faces of each blade
        Ok(())
    }
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
            .add_plugins(MaterialPlugin::<GrassMaterial>::default())
            .add_systems(OnEnter(AppState::Playing), generate_city)
            .add_systems(OnEnter(AppState::MainMenu), cleanup_world_for_menu)
            .add_systems(
                Update,
                (
                    update_day_night,
                    animate_nature,
                    guardian_ship_patrol_system,
                    city_spy_drone_patrol_system,
                    city_spy_drone_data_drop_system,
                    discussion_interaction_system,
                    dungeon_crawl_gate_system,
                    dungeon_key_pickup_system,
                    dungeon_key_gate_system,
                    dungeon_enemy_spawner_system,
                    world_site_enemy_spawner_system,
                    site_liberation_system,
                    world_route_unlock_system,
                    moving_platform_system,
                    rotating_elevator_system,
                    sling_shot_system,
                    board_boost_pad_system,
                    laser_turret_system,
                    laser_beam_cleanup_system,
                    collider_debug_toggle_system,
                )
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
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if discussion.active {
        return;
    }

    for (player_index, player_transform, input) in player_q.iter() {
        if !input.interact {
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
    time: Res<Time>,
    mut dungeon: ResMut<DungeonCrawlState>,
    mut gate_q: Query<&mut DungeonCrawlGate>,
    mut door_q: Query<(&mut Transform, &DungeonGateDoor), Without<Player>>,
    player_q: Query<(&Transform, &PlayerInput), With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    let mut opened_chapters = Vec::new();

    for mut gate in gate_q.iter_mut() {
        let party_near_gate = player_q.iter().any(|(transform, _)| {
            transform.translation.distance(gate.entry) <= gate.interact_radius
        });
        let should_open = party_near_gate
            && player_q.iter().any(|(transform, input)| {
                input.interact && transform.translation.distance(gate.entry) <= gate.interact_radius
            });

        if should_open {
            if !gate.opened {
                msg_ev.write(UiMessageEvent {
                    text: format!("{} opened - top-down dungeon crawl linked.", gate.label),
                    duration: 3.0,
                });
            }
            gate.opened = true;
            dungeon.activate(
                crate::chapters::ChapterId(gate.chapter),
                gate.label,
                gate.focus,
                gate.entry,
                gate.radius,
            );
        }

        if gate.opened {
            opened_chapters.push(gate.chapter);
        }
    }

    if dungeon.active {
        let keep_active = player_q.iter().any(|(transform, _)| {
            transform.translation.distance(dungeon.focus) <= dungeon.radius + 70.0
        });
        if !keep_active {
            dungeon.clear();
            msg_ev.write(UiMessageEvent {
                text: "Dungeon camera released.".to_string(),
                duration: 1.6,
            });
        }
    }

    for (mut transform, door) in door_q.iter_mut() {
        let target = if opened_chapters.contains(&door.chapter) {
            door.open
        } else {
            door.closed
        };
        let blend = 1.0 - (-dt * 5.5).exp();
        transform.translation = transform.translation.lerp(target, blend);
    }
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
    mut spawner_q: Query<(&Transform, &mut DungeonEnemySpawner)>,
    player_q: Query<&Transform, With<Player>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if !dungeon.active {
        return;
    }
    for (spawner_xf, mut spawner) in spawner_q.iter_mut() {
        if spawner.spawned {
            continue;
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
            spawn_enemy_entity(
                &mut commands,
                &mut meshes,
                &mut materials,
                spawner.enemy_type,
                spawner_xf.translation + offset,
                spawner.difficulty,
                None,
            );
        }
        msg_ev.write(UiMessageEvent {
            text: "Enemies appear!".to_string(),
            duration: 1.8,
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
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let defense_score = static_defense_score_for_site(CLOUDRAIL_TUTORIAL_RAID_SITE, &economy)
        + command_registry.defense_score_for_site(CLOUDRAIL_TUTORIAL_RAID_SITE);
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
                    + command_registry.defense_score_for_site(raid.target_site),
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
            raid.phase == crate::raids::RaidPhase::Active
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

        let enemy_type = match kind {
            RaidKind::DroneSwarm | RaidKind::FighterAttack | RaidKind::GiantUfoSiege => {
                EnemyType::Drone
            }
            RaidKind::RobotLanding | RaidKind::RouteBlockade => EnemyType::Soldier,
            RaidKind::DragonDomainAssault => EnemyType::SpikeAlien,
        };
        let radius = 22.0;
        for i in 0..wave_size {
            let t = i as f32 / wave_size.max(1) as f32;
            let angle = t * std::f32::consts::TAU;
            let offset = Vec3::new(
                angle.cos() * radius,
                8.0 + (i % 3) as f32 * 2.2,
                angle.sin() * radius,
            );
            let enemy = spawn_enemy_entity(
                &mut commands,
                &mut meshes,
                &mut materials,
                enemy_type,
                center + offset,
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
            With<crate::physics::prelude::Collider>,
            With<WorldGeometry>,
            Without<ColliderDebugSource>,
            Without<ColliderDebugVisual>,
        ),
    >,
    box_collider_q: Query<
        (Entity, &DebugColliderBox),
        (
            With<crate::physics::prelude::Collider>,
            With<WorldGeometry>,
            Without<Mesh3d>,
            Without<ColliderDebugSource>,
        ),
    >,
    source_q: Query<Entity, With<ColliderDebugSource>>,
    visual_q: Query<Entity, With<ColliderDebugVisual>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if !keyboard.just_pressed(KeyCode::F9) {
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

fn board_boost_pad_system(
    time: Res<Time>,
    mut pad_q: Query<(&Transform, &mut BoardBoostPad)>,
    mut player_q: Query<(
        &Transform,
        &mut PlayerMovement,
        &mut TraversalModeState,
        &mut BoardBoostState,
        &mut PlayerStateMachine,
    )>,
) {
    let dt = time.delta_secs();
    for (pad_transform, mut pad) in pad_q.iter_mut() {
        pad.cooldown_timer = (pad.cooldown_timer - dt).max(0.0);
        if pad.cooldown_timer > 0.0 {
            continue;
        }

        let direction = pad.direction.with_y(0.0).normalize_or_zero();
        if direction == Vec3::ZERO {
            continue;
        }

        let inv_rot = pad_transform.rotation.inverse();
        for (player_transform, mut movement, mut traversal, mut boost, mut state) in
            player_q.iter_mut()
        {
            let local = inv_rot * (player_transform.translation - pad_transform.translation);
            let on_pad = local.x.abs() <= pad.half_width
                && local.z.abs() <= pad.half_length
                && local.y >= -2.2
                && local.y <= 4.2;
            if !on_pad {
                continue;
            }

            if pad.force_hoverboard {
                traversal.active = TraversalMode::Hoverboard;
            }

            boost.timer = boost.timer.max(pad.duration);
            boost.speed_mult = boost.speed_mult.max(pad.speed_mult);
            boost.direction = direction;

            let current_along = movement.ground_velocity.dot(direction);
            if current_along < pad.impulse {
                movement.ground_velocity += direction * (pad.impulse - current_along);
            }
            if pad.lift > 0.0 {
                movement.velocity.y = movement.velocity.y.max(pad.lift);
                movement.is_grounded = false;
                movement.coyote_timer = 0.0;
                state.force(crate::components::player::PlayerState::Jetpack);
            } else {
                state.transition(crate::components::player::PlayerState::Sprinting);
            }
            pad.cooldown_timer = pad.cooldown;
            break;
        }
    }
}

fn npc_road_vehicle_system(
    mut commands: Commands,
    time: Res<Time>,
    mut vehicle_q: Query<(Entity, &mut Transform, &mut NpcRoadVehicle, &Health)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut vehicle, health) in vehicle_q.iter_mut() {
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
                    &DamageInfo::new(turret.damage, DamageType::Laser),
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
    asset_server: Res<AssetServer>,
    transition: Res<PlaySessionTransition>,
    settings: Res<GameSettings>,
    current: Res<CurrentChapter>,
    economy: Res<SettlementEconomy>,
    existing_world: Query<Entity, With<WorldGeometry>>,
    mut world_site_registry: ResMut<WorldSiteRegistry>,
    mut world_route_registry: ResMut<WorldRouteRegistry>,
) {
    if transition.resuming_from_pause || !existing_world.is_empty() {
        return;
    }

    let seed = settings.world_seed;
    let m = &mut *mats;

    let pal = Palette::build(m, &asset_server);

    spawn_lighting(&mut commands, &mut meshes, m);
    spawn_ground_plane(&mut commands);
    spawn_terrain(&mut commands, &mut meshes, &pal, seed);
    spawn_downtown(&mut commands, &mut meshes, &pal, seed);
    spawn_industrial(&mut commands, &mut meshes, &pal, seed + 1, seed);
    spawn_residential(&mut commands, &mut meshes, &pal, seed + 2, seed);
    spawn_highways(&mut commands, &mut meshes, &pal);
    spawn_city_streets(&mut commands, &mut meshes, &pal);
    spawn_city_hidden_rooms(&mut commands, &mut meshes, m, &pal);
    spawn_traversal_courses(&mut commands, &mut meshes, m, &pal);
    spawn_sky_platforms(&mut commands, &mut meshes, &pal, seed + 3);
    spawn_sky_bridges(&mut commands, &mut meshes, &pal, seed + 4);
    spawn_moving_platforms(&mut commands, &mut meshes, &pal, seed + 15, seed);
    spawn_spaceports(&mut commands, &mut meshes, &pal, seed);
    spawn_laser_turrets(&mut commands, &mut meshes, &pal, seed + 16, seed);
    spawn_mountains(&mut commands, &mut meshes, &pal, seed + 5);
    spawn_everest_range_biomes(&mut commands, &mut meshes, &pal, seed);
    if current.id.0 == 1 {
        spawn_chapter_one_ocean_island(&mut commands, &mut meshes, &pal);
    }
    spawn_grasslands(&mut commands, &mut meshes, &mut grass_mats, seed + 10);
    spawn_neon_lights(&mut commands, seed + 6);
    spawn_street_lights(&mut commands, seed + 7);
    spawn_outer_districts(&mut commands, &mut meshes, &pal, seed + 8, seed);
    spawn_river(&mut commands, &mut meshes, &pal);
    spawn_water_gardens(&mut commands, &mut meshes, &pal, seed + 12, seed);
    spawn_mana_waterfalls(&mut commands, &mut meshes, &pal, seed + 18, seed);
    spawn_mana_forests(&mut commands, &mut meshes, &pal, seed + 19, seed);
    spawn_bio_city_tamborn(&mut commands, &mut meshes, &pal, seed + 20, seed);
    spawn_rock_fields(&mut commands, &mut meshes, &pal, seed + 13, seed);
    spawn_understory(&mut commands, &mut meshes, &pal, seed + 14, seed);
    spawn_trees(&mut commands, &mut meshes, &pal, seed + 9, seed);
    spawn_aurora_castle(&mut commands, &mut meshes, &pal, seed);
    spawn_collosar_castle(&mut commands, &mut meshes, &pal, seed);
    spawn_magic_crystals(&mut commands, &mut meshes, &pal, seed);
    spawn_secret_cave_systems(&mut commands, &mut meshes, &pal, seed);
    spawn_dragon_lair_dungeons(&mut commands, &mut meshes, m, &pal, seed);
    spawn_great_scientist_temples(&mut commands, &mut meshes, m, &pal, seed);
    spawn_exploration_settlements(&mut commands, &mut meshes, m, &pal, seed, &economy);
    spawn_chapter_map_locations(&mut commands, &mut meshes, &pal, seed);
    spawn_puzzle_anchors(&mut commands, seed);
    setup_world_site_registry(&mut world_site_registry);
    spawn_world_site_props(&mut commands, &mut meshes, m, &world_site_registry, seed);
    setup_world_route_registry(&mut world_route_registry);
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

    industrial_metal: Handle<StandardMaterial>,
    industrial_rust: Handle<StandardMaterial>,

    residential_a: Handle<StandardMaterial>,
    residential_b: Handle<StandardMaterial>,
    brick_line: Handle<StandardMaterial>,
    stone_block: Handle<StandardMaterial>,
    small_window_warm: Handle<StandardMaterial>,
    small_window_cool: Handle<StandardMaterial>,
    small_window_dark: Handle<StandardMaterial>,

    street_asphalt: Handle<StandardMaterial>,
    street_paint: Handle<StandardMaterial>,

    terrain_surface: Handle<StandardMaterial>,
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
    asset_server.load_with_settings(path, |settings: &mut ImageLoaderSettings| {
        *settings = ImageLoaderSettings {
            sampler: ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..default()
            }),
            ..default()
        };
    })
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
                base_color: Color::srgb(0.34, 0.36, 0.44),
                emissive: LinearRgba::new(0.06, 0.08, 0.14, 1.0),
                metallic: 0.88,
                perceptual_roughness: 0.22,
                reflectance: 0.78,
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

            industrial_metal: m.add(StandardMaterial {
                base_color: Color::srgb(0.50, 0.52, 0.58),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/metal.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(3.4)),
                emissive: LinearRgba::new(0.10, 0.12, 0.18, 1.0),
                metallic: 0.78,
                perceptual_roughness: 0.36,
                reflectance: 0.72,
                ..default()
            }),
            industrial_rust: m.add(StandardMaterial {
                base_color: Color::srgb(0.72, 0.42, 0.22),
                base_color_texture: Some(repeating_texture(asset_server, "Materials/Copper.png")),
                uv_transform: Affine2::from_scale(Vec2::splat(3.0)),
                emissive: LinearRgba::new(0.11, 0.05, 0.02, 1.0),
                metallic: 0.42,
                perceptual_roughness: 0.74,
                reflectance: 0.46,
                ..default()
            }),

            // Smaller buildings: painterly brick and cinder block with old-anime warmth.
            residential_a: m.add(StandardMaterial {
                base_color: Color::srgb(0.72, 0.46, 0.34),
                emissive: LinearRgba::new(0.12, 0.05, 0.03, 1.0),
                metallic: 0.02,
                perceptual_roughness: 0.93,
                reflectance: 0.20,
                ..default()
            }),
            residential_b: m.add(StandardMaterial {
                base_color: Color::srgb(0.62, 0.64, 0.68),
                emissive: LinearRgba::new(0.04, 0.04, 0.06, 1.0),
                metallic: 0.01,
                perceptual_roughness: 0.95,
                reflectance: 0.18,
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
            terrain_surface: m.add(StandardMaterial {
                base_color: Color::srgb(0.86, 0.94, 0.82),
                base_color_texture: Some(repeating_texture(
                    asset_server,
                    "Materials/rock_grass.png",
                )),
                uv_transform: Affine2::from_scale(Vec2::splat(96.0)),
                metallic: 0.0,
                perceptual_roughness: 0.96,
                reflectance: 0.18,
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
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

#[derive(Clone, Copy)]
struct SecretCaveSpec {
    chapter: u8,
    anchor_id: &'static str,
    x: f32,
    z: f32,
    yaw: f32,
    length: f32,
}

fn spawn_secret_cave_systems(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    let caves = [
        SecretCaveSpec {
            chapter: 1,
            anchor_id: "secret_cave_ch01",
            x: 42.0,
            z: 54.0,
            yaw: -0.55,
            length: 38.0,
        },
        SecretCaveSpec {
            chapter: 2,
            anchor_id: "secret_cave_ch02",
            x: 2340.0,
            z: 1780.0,
            yaw: -1.10,
            length: 42.0,
        },
        SecretCaveSpec {
            chapter: 3,
            anchor_id: "secret_cave_ch03",
            x: -2380.0,
            z: 2720.0,
            yaw: 0.70,
            length: 36.0,
        },
        SecretCaveSpec {
            chapter: 4,
            anchor_id: "secret_cave_ch04",
            x: 3200.0,
            z: -1800.0,
            yaw: 0.25,
            length: 40.0,
        },
        SecretCaveSpec {
            chapter: 5,
            anchor_id: "secret_cave_ch05",
            x: -3320.0,
            z: 790.0,
            yaw: 1.20,
            length: 40.0,
        },
        SecretCaveSpec {
            chapter: 6,
            anchor_id: "secret_cave_ch06",
            x: -8850.0,
            z: -8200.0,
            yaw: 0.78,
            length: 46.0,
        },
        SecretCaveSpec {
            chapter: 7,
            anchor_id: "secret_cave_ch07",
            x: -7950.0,
            z: 4200.0,
            yaw: -0.40,
            length: 44.0,
        },
        SecretCaveSpec {
            chapter: 8,
            anchor_id: "secret_cave_ch08",
            x: -6000.0,
            z: 8200.0,
            yaw: -0.90,
            length: 38.0,
        },
        SecretCaveSpec {
            chapter: 9,
            anchor_id: "secret_cave_ch09",
            x: 6300.0,
            z: 260.0,
            yaw: 1.00,
            length: 36.0,
        },
        SecretCaveSpec {
            chapter: 10,
            anchor_id: "secret_cave_ch10",
            x: 8900.0,
            z: 5200.0,
            yaw: -1.35,
            length: 46.0,
        },
        SecretCaveSpec {
            chapter: 11,
            anchor_id: "secret_cave_ch11",
            x: 2820.0,
            z: -8650.0,
            yaw: 0.95,
            length: 42.0,
        },
        SecretCaveSpec {
            chapter: 12,
            anchor_id: "secret_cave_ch12",
            x: 5750.0,
            z: -3450.0,
            yaw: 0.35,
            length: 38.0,
        },
        SecretCaveSpec {
            chapter: 13,
            anchor_id: "secret_cave_ch13",
            x: -4700.0,
            z: -6550.0,
            yaw: -0.20,
            length: 42.0,
        },
        SecretCaveSpec {
            chapter: 14,
            anchor_id: "secret_cave_ch14",
            x: 650.0,
            z: -8300.0,
            yaw: 0.0,
            length: 44.0,
        },
    ];

    for spec in caves {
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

fn spawn_secret_cave_solid(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
    ));
    if walkable {
        entity.insert(WalkableSurface);
    }
}

fn spawn_secret_cave_system(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    spec: SecretCaveSpec,
) {
    let floor_y = terrain_surface_y(spec.x, spec.z, seed) + 0.25;
    let entrance = Vec3::new(spec.x, floor_y, spec.z);
    let forward = Vec3::new(spec.yaw.sin(), 0.0, spec.yaw.cos());
    let right = Vec3::new(forward.z, 0.0, -forward.x);
    let rotation = Quat::from_rotation_y(spec.yaw);
    let accent = secret_cave_accent(pal, spec.chapter);
    let wall_mat = if matches!(spec.chapter, 5 | 11 | 13 | 14) {
        pal.rock_dark.clone()
    } else {
        pal.rock.clone()
    };
    let floor_mat = if matches!(spec.chapter, 3 | 9 | 14) {
        pal.marble.clone()
    } else if matches!(spec.chapter, 1..=5 | 12..=14) {
        pal.stone_block.clone()
    } else {
        pal.dragon_stone.clone()
    };
    let width = 10.0 + (spec.chapter % 3) as f32 * 1.5;

    for side in [-1.0, 1.0] {
        let pillar_pos = entrance + right * side * (width * 0.5 + 1.2) + Vec3::Y * 3.4;
        spawn_secret_cave_solid(
            commands,
            meshes,
            wall_mat.clone(),
            Vec3::new(2.0, 6.8, 2.4),
            Transform::from_translation(pillar_pos).with_rotation(rotation),
            false,
        );
    }
    spawn_secret_cave_solid(
        commands,
        meshes,
        wall_mat.clone(),
        Vec3::new(width + 4.5, 1.3, 2.8),
        Transform::from_translation(entrance + Vec3::Y * 7.0).with_rotation(rotation),
        false,
    );
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(4.2))),
            material: MeshMaterial3d(pal.rock_dark.clone()),
            transform: Transform {
                translation: entrance + forward * 3.2 + Vec3::Y * 2.9,
                scale: Vec3::new(1.25, 0.72, 0.30),
                rotation,
            },
            ..default()
        },
        WorldGeometry,
    ));

    for segment in 0..3 {
        let along = 7.0 + segment as f32 * 10.5;
        let center = entrance + forward * along + Vec3::Y * 0.05;
        spawn_secret_cave_solid(
            commands,
            meshes,
            floor_mat.clone(),
            Vec3::new(width, 0.55, 10.6),
            Transform::from_translation(center).with_rotation(rotation),
            true,
        );
        for side in [-1.0, 1.0] {
            spawn_secret_cave_solid(
                commands,
                meshes,
                wall_mat.clone(),
                Vec3::new(0.95, 4.0, 10.8),
                Transform::from_translation(
                    center + right * side * (width * 0.5 + 0.65) + Vec3::Y * 2.2,
                )
                .with_rotation(rotation),
                false,
            );
        }
        let rib_pos = center + Vec3::Y * 4.5 + forward * 4.4;
        spawn_secret_cave_solid(
            commands,
            meshes,
            pal.brushed_metal.clone(),
            Vec3::new(width + 1.6, 0.45, 0.75),
            Transform::from_translation(rib_pos).with_rotation(rotation),
            false,
        );
    }

    let chamber = entrance + forward * spec.length + Vec3::Y * 0.1;
    spawn_secret_cave_solid(
        commands,
        meshes,
        floor_mat.clone(),
        Vec3::new(width + 9.0, 0.65, 17.0),
        Transform::from_translation(chamber).with_rotation(rotation),
        true,
    );
    for side in [-1.0, 1.0] {
        spawn_secret_cave_solid(
            commands,
            meshes,
            wall_mat.clone(),
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
        Vec3::new(width + 9.0, 5.0, 1.0),
        Transform::from_translation(chamber + forward * 8.5 + Vec3::Y * 2.6)
            .with_rotation(rotation),
        false,
    );

    for i in 0..7 {
        let side = if i % 2 == 0 { -1.0 } else { 1.0 };
        let shard_h = 2.4 + seeded(seed, spec.chapter as u64 * 97 + i as u64) * 3.4;
        let shard_r = 0.45 + seeded(seed, spec.chapter as u64 * 101 + i as u64) * 0.85;
        let shard_mat = match (spec.chapter + i as u8) % 5 {
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
                        color: if matches!(spec.chapter, 6..=11) {
                            Color::srgb(1.0, 0.34, 0.12)
                        } else if matches!((spec.chapter + i as u8) % 5, 0) {
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
            crate::physics::prelude::Collider::cuboid(
                platform_size.x * 0.5,
                platform_size.y * 0.5,
                platform_size.z * 0.5,
            ),
            crate::physics::prelude::RigidBody::KinematicPositionBased,
            WorldGeometry,
            WalkableSurface,
            MovingPlatform {
                start: base + Vec3::Y * -1.0,
                end: base + Vec3::Y * 2.2,
                speed: 2.2 + i as f32 * 0.35,
                phase: spec.chapter as f32 * 0.31 + i as f32,
                size: platform_size,
            },
        ));
    }

    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: if matches!(spec.chapter, 6..=11) {
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
    gate_id: u8,
    anchor_id: &'static str,
    gate_label: &'static str,
    reward_id: &'static str,
    reward_label: &'static str,
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
            chapter: spec.gate_id,
            label: spec.gate_label,
            entry,
            focus,
            radius: 70.0,
            interact_radius: 18.0,
            opened: false,
        },
    ));

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
                chapter: spec.gate_id,
                closed,
                open,
            },
            crate::physics::prelude::RigidBody::KinematicPositionBased,
            crate::physics::prelude::Collider::cuboid(2.9, 4.2, 0.55),
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
            chapter: spec.chapter,
            label: spec.gate_label,
            entry,
            focus,
            radius: 66.0,
            interact_radius: 17.0,
            opened: false,
        },
    ));

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
                chapter: spec.chapter,
                closed,
                open,
            },
            crate::physics::prelude::RigidBody::KinematicPositionBased,
            crate::physics::prelude::Collider::cuboid(2.55, 3.7, 0.5),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(2.4, 1.4),
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
            crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
            crate::physics::prelude::RigidBody::KinematicPositionBased,
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
                crate::physics::prelude::RigidBody::Fixed,
                crate::physics::prelude::Collider::cylinder(0.7, 0.8),
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
const EVEREST_HEIGHTMAP_BYTES: &[u8] = include_bytes!("../../assets/terrain/everest.png");
static EVEREST_HEIGHTMAP: OnceLock<Option<EverestHeightmap>> = OnceLock::new();

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
const MOUNTAIN_ROUTES: &[&[(f32, f32)]] = &[
    ROUTE_CORE_AURORA,
    ROUTE_CORE_EVEREST,
    ROUTE_SOUTH_GLACIER,
    ROUTE_NORTHWEST_GLACIER,
    ROUTE_NORTH_HIGH_PASS,
    ROUTE_WEST_TEMPLE_PASS,
    ROUTE_EAST_ROCKIES_PASS,
    ROUTE_CROWN_UNDERPASS,
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
}

fn nearest_mountain_route_point(x: f32, z: f32) -> Option<RouteProjection> {
    let mut best: Option<RouteProjection> = None;
    for route in mountain_routes() {
        for pair in route.windows(2) {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let (point, distance) = project_point_to_segment_xz(x, z, ax, az, bx, bz);
            if best.is_none_or(|projection| distance < projection.distance) {
                best = Some(RouteProjection { point, distance });
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

const SPEED_ROAD_CHUNK_LEN: f32 = 360.0;
const SPEED_ROAD_CLEARANCE: f32 = 4.6;
const SPEED_ROAD_WIDTH: f32 = 38.0;
const SPEED_ROAD_TERRAIN_SAMPLE_SPACING: f32 = 90.0;
const SPEED_ROAD_EDGE_SAMPLE_FRACTION: f32 = 0.42;
const SPEED_ROAD_TRAFFIC_LANE_OFFSET: f32 = 9.5;
const SPEED_ROAD_PATROL_VEHICLE_COUNT: usize = 18;

fn speed_road_segment_subdivision_count(length: f32) -> usize {
    (length / SPEED_ROAD_CHUNK_LEN).ceil().max(1.0) as usize
}

fn speed_road_required_terrain_lift(
    start: Vec3,
    end: Vec3,
    width: f32,
    terrain_seed: u64,
    clearance: f32,
) -> f32 {
    let delta_xz = Vec2::new(end.x - start.x, end.z - start.z);
    let horizontal_len = delta_xz.length();
    if horizontal_len <= f32::EPSILON {
        return 0.0;
    }

    let dir = delta_xz / horizontal_len;
    let right = Vec2::new(dir.y, -dir.x);
    let samples = (horizontal_len / SPEED_ROAD_TERRAIN_SAMPLE_SPACING)
        .ceil()
        .clamp(1.0, 16.0) as usize;
    let edge_offset = width * SPEED_ROAD_EDGE_SAMPLE_FRACTION;
    let mut needed_lift: f32 = 0.0;

    for i in 0..=samples {
        let t = i as f32 / samples as f32;
        let center = Vec2::new(start.x + delta_xz.x * t, start.z + delta_xz.y * t);
        let road_y = start.y + (end.y - start.y) * t;
        for offset in [0.0_f32, -edge_offset, edge_offset] {
            let sample = center + right * offset;
            needed_lift = needed_lift
                .max(terrain_surface_y(sample.x, sample.y, terrain_seed) + clearance - road_y);
        }
    }

    needed_lift.max(0.0)
}

fn mountain_speed_road_segment_count() -> usize {
    mountain_routes()
        .iter()
        .flat_map(|route| route.windows(2))
        .map(|pair| {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            speed_road_segment_subdivision_count(Vec2::new(bx - ax, bz - az).length())
        })
        .sum()
}

fn settlement_speed_ring_radius_for_layout(
    kind: MapSettlementKind,
    layout: SettlementTerrainLayout,
) -> f32 {
    match kind {
        MapSettlementKind::City => (layout.platform_radius + 92.0).max(230.0),
        MapSettlementKind::Village => (layout.platform_radius + 54.0).max(116.0),
        MapSettlementKind::Harbor => (layout.platform_radius + 64.0).max(138.0),
        MapSettlementKind::Outpost => (layout.platform_radius + 50.0).max(104.0),
    }
}

fn settlement_speed_ring_radius(seed: u64, settlement: MapSettlement) -> f32 {
    let layout =
        settlement_layout_profile(seed, settlement, settlement_plaza_radius(settlement.kind));
    settlement_speed_ring_radius_for_layout(settlement.kind, layout)
}

fn distance_to_speed_road_network(x: f32, z: f32, seed: u64) -> f32 {
    let mut best = distance_to_mountain_route_network(x, z);
    let point = Vec2::new(x, z);

    for settlement in map_settlements().iter().copied() {
        let center = Vec2::new(settlement.x, settlement.z);
        let ring_radius = settlement_speed_ring_radius(seed, settlement);
        best = best.min((point.distance(center) - ring_radius).abs());

        if let Some(route_projection) = nearest_mountain_route_point(settlement.x, settlement.z) {
            let to_route = route_projection.point - center;
            if to_route.length_squared() > 0.01 {
                let spur_start = center + to_route.normalize() * ring_radius;
                best = best.min(distance_to_segment_xz(
                    x,
                    z,
                    spur_start.x,
                    spur_start.y,
                    route_projection.point.x,
                    route_projection.point.y,
                ));
            }
        }
    }

    best
}

fn speed_road_loop_gate_count() -> usize {
    let route_gate_count = mountain_routes()
        .iter()
        .enumerate()
        .flat_map(|(ri, route)| {
            route.windows(2).enumerate().filter(move |(si, pair)| {
                let (ax, az) = pair[0];
                let (bx, bz) = pair[1];
                Vec2::new(bx - ax, bz - az).length() > 1_500.0 && (ri + *si) % 2 == 0
            })
        })
        .count();

    route_gate_count + map_settlements().len()
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

/// Layered sine-wave height at world position (x, z).
/// The city centre is kept near-flat; elevation builds toward the outer ring.
fn terrain_height(x: f32, z: f32, seed: u64) -> f32 {
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

fn terrain_surface_y(x: f32, z: f32, seed: u64) -> f32 {
    terrain_height(x, z, seed)
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
    color = mix_rgba(color, trail, mountain_route_influence(x, z) * 0.72);

    scale_rgba(color, 0.90 + detail * 0.18)
}

fn spawn_terrain(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    const RES: usize = 320; // quads per side — 62.5 world units per cell over 200 miles
    const WORLD: f32 = TERRAIN_WORLD_SIZE;
    const CELL: f32 = WORLD / RES as f32;

    // ── Height grid ─────────────────────────────────────────────────────────
    let vcount = (RES + 1) * (RES + 1);
    let mut h = vec![0.0f32; vcount];
    for zi in 0..=RES {
        for xi in 0..=RES {
            let wx = (xi as f32 / RES as f32 - 0.5) * WORLD;
            let wz = (zi as f32 / RES as f32 - 0.5) * WORLD;
            h[zi * (RES + 1) + xi] = terrain_height(wx, wz, seed);
        }
    }

    // ── Mesh buffers ────────────────────────────────────────────────────────
    let mut positions = Vec::with_capacity(vcount);
    let mut normals = Vec::with_capacity(vcount);
    let mut uvs = Vec::with_capacity(vcount);
    let mut colors = Vec::with_capacity(vcount);

    for zi in 0..=RES {
        for xi in 0..=RES {
            let wx = (xi as f32 / RES as f32 - 0.5) * WORLD;
            let wz = (zi as f32 / RES as f32 - 0.5) * WORLD;
            let wy = h[zi * (RES + 1) + xi];
            positions.push([wx, wy, wz]);
            uvs.push([xi as f32 / RES as f32, zi as f32 / RES as f32]);

            // Finite-difference normal (upward).
            let hx0 = h[zi * (RES + 1) + xi.saturating_sub(1)];
            let hx1 = h[zi * (RES + 1) + (xi + 1).min(RES)];
            let hz0 = h[zi.saturating_sub(1) * (RES + 1) + xi];
            let hz1 = h[(zi + 1).min(RES) * (RES + 1) + xi];
            let dx = Vec3::new(2.0 * CELL, hx1 - hx0, 0.0);
            let dz = Vec3::new(0.0, hz1 - hz0, 2.0 * CELL);
            let n = dz.cross(dx).normalize();
            normals.push([n.x, n.y, n.z]);
            colors.push(terrain_vertex_color(wx, wz, wy, n, seed));
        }
    }

    // Two triangles per quad, CCW winding, normals pointing up.
    let mut tri_idx = Vec::with_capacity(RES * RES * 6);
    for zi in 0..RES {
        for xi in 0..RES {
            let tl = (zi * (RES + 1) + xi) as u32;
            let tr = tl + 1;
            let bl = tl + (RES + 1) as u32;
            let br = bl + 1;
            tri_idx.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
        }
    }

    // Build collider data before the Vecs are consumed by the mesh.
    let col_verts: Vec<Vec3> = positions.iter().map(|p| Vec3::from(*p)).collect();
    let col_tris: Vec<[u32; 3]> = tri_idx
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(tri_idx));

    let terrain_collider = crate::physics::prelude::Collider::trimesh(col_verts, col_tris)
        .expect("generated terrain mesh must produce a valid trimesh collider");

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(mesh)),
            material: MeshMaterial3d(pal.terrain_surface.clone()),
            transform: Transform::IDENTITY,
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::physics::prelude::RigidBody::Fixed,
        terrain_collider,
    ));
}

fn spawn_everest_range_biomes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    spawn_range_snowfields(commands, meshes, pal, seed);
    spawn_glacial_streams(commands, meshes, pal, seed);
    spawn_range_forests(commands, meshes, pal, seed);
    spawn_range_waylines(commands, meshes, pal, seed);
    spawn_mountain_path_network(commands, meshes, pal, seed);
    spawn_mountain_route_bridges(commands, meshes, pal, seed + 83, seed);
    spawn_speed_road_network(commands, meshes, pal, seed + 1_241, seed);
    spawn_range_outposts(commands, meshes, pal, seed);
    spawn_dragon_lair_silhouettes(commands, meshes, pal, seed);
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
                meshes,
                pal,
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
                mesh: Mesh3d(meshes.add(Sphere::new(1.0))),
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
    let leaf_mesh = meshes.add(Sphere::new(1.0));
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

        spawn_static_cylinder_between(commands, meshes, pal.vine.clone(), top, mid, vine_radius);
        spawn_static_cylinder_between(
            commands,
            meshes,
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
                    mesh: Mesh3d(leaf_mesh.clone()),
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
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
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
            mesh: Mesh3d(meshes.add(Cylinder::new(trunk_r, trunk_h))),
            material: MeshMaterial3d(bark.clone()),
            transform: Transform::from_xyz(base.x, base.y + trunk_h * 0.5, base.z)
                .with_rotation(Quat::from_rotation_y(yaw)),
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
                mesh: Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
                material: MeshMaterial3d(bark.clone()),
                transform: root_transform,
                ..default()
            },
            WorldGeometry,
        ));
    }

    let lower = Transform::from_xyz(base.x, base.y + trunk_h * 0.55 + lower_h * 0.5, base.z)
        .with_rotation(Quat::from_rotation_y(yaw));
    let lower_sway = NatureSway::new(&lower, seeded(seed, 11) * 6.0, 0.35, 0.010, 0.015, 0.006);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cone {
                radius: lower_r,
                height: lower_h,
            })),
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
            mesh: Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            material: MeshMaterial3d(bark.clone()),
            transform: branch_ring,
            ..default()
        },
        WorldGeometry,
    ));

    spawn_moss_pad(
        commands,
        meshes,
        pal,
        base + Vec3::Y * 0.04,
        trunk_r * 3.2,
        seed + 701,
    );
    if seed % 4 != 1 {
        spawn_vine_curtain(
            commands,
            meshes,
            pal,
            base + Vec3::Y * (trunk_h * 1.05),
            yaw + 0.4,
            lower_r * 0.42,
            trunk_h * 0.74,
            2 + seed % 3,
            seed + 907,
        );
    }

    let upper = Transform::from_xyz(base.x, base.y + trunk_h * 1.10 + lower_h * 0.58, base.z)
        .with_rotation(Quat::from_rotation_y(yaw + 0.7));
    let upper_sway = NatureSway::new(&upper, seeded(seed, 13) * 6.0, 0.42, 0.014, 0.018, 0.007);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cone {
                radius: upper_r,
                height: upper_h,
            })),
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
                meshes,
                pal,
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
    let center_x = -760.0_f32;
    let center_z = 1040.0_f32;
    let center_ground = terrain_surface_y(center_x, center_z, terrain_seed);
    let center = Vec3::new(center_x, center_ground, center_z);

    spawn_mother_tree_tamborn(commands, meshes, pal, center, seed);

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
            meshes,
            pal,
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
            meshes,
            pal,
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
            meshes,
            pal,
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
                meshes,
                pal,
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
            meshes,
            pal,
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
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    base: Vec3,
    seed: u64,
) {
    let trunk_mesh = meshes.add(Cylinder::new(1.0, 1.0));
    let sphere_mesh = meshes.add(Sphere::new(1.0));
    let yaw = seeded(seed, 31) * TAU;

    for (offset_y, height, radius, material) in [
        (92.0, 184.0, 26.0, pal.bark_dark.clone()),
        (226.0, 176.0, 18.5, pal.bark_mid.clone()),
        (328.0, 92.0, 12.5, pal.bark_light.clone()),
    ] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(trunk_mesh.clone()),
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
        spawn_static_cylinder_between(
            commands,
            meshes,
            pal.bark_dark.clone(),
            start,
            end,
            4.6 + seeded(seed, 180 + root) * 2.2,
        );
    }

    spawn_tamborn_platform(
        commands,
        meshes,
        pal.moss.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_LOWER_DECK,
        76.0,
        3.0,
        "Tamborn Mother Tree Lower Terrace",
    );
    spawn_tamborn_ring_rail(
        commands,
        meshes,
        pal.vine.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_LOWER_DECK,
        78.0,
        3.0,
        28,
        0.24,
    );
    spawn_tamborn_platform(
        commands,
        meshes,
        pal.bridge_deck.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_UPPER_DECK,
        58.0,
        2.6,
        "Tamborn Mother Tree Upper Terrace",
    );
    spawn_tamborn_ring_rail(
        commands,
        meshes,
        pal.vine.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_UPPER_DECK,
        60.0,
        3.0,
        24,
        0.20,
    );
    spawn_tamborn_platform(
        commands,
        meshes,
        pal.marble.clone(),
        base + Vec3::Y * TAMBORN_MOTHER_CROWN_DECK,
        38.0,
        2.3,
        "Tamborn Moon Crown Terrace",
    );
    spawn_tamborn_ring_rail(
        commands,
        meshes,
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
                mesh: Mesh3d(sphere_mesh.clone()),
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
            spawn_static_cylinder_between(
                commands,
                meshes,
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
        let pos = base
            + Vec3::new(
                angle.cos() * ring,
                TAMBORN_MOTHER_LOWER_DECK + 4.4,
                angle.sin() * ring,
            );
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cone {
                    radius: 2.0 + seeded(seed, 470 + crystal) * 1.2,
                    height: 8.0 + seeded(seed, 510 + crystal) * 6.0,
                })),
                material: MeshMaterial3d(pal.crystal_emerald.clone()),
                transform: Transform::from_translation(pos)
                    .with_rotation(Quat::from_rotation_y(angle)),
                ..default()
            },
            WorldGeometry,
            Name::new("Tamborn Heart Crystal"),
        ));
    }

    spawn_vine_curtain(
        commands,
        meshes,
        pal,
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
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    base: Vec3,
    height: f32,
    radius: f32,
    seed: u64,
) -> TambornTower {
    let trunk_mesh = meshes.add(Cylinder::new(1.0, 1.0));
    let sphere_mesh = meshes.add(Sphere::new(1.0));
    let cube_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
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
            mesh: Mesh3d(trunk_mesh.clone()),
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
        spawn_static_cylinder_between(commands, meshes, bark.clone(), start, end, radius * 0.34);
    }

    let lower_deck_y = base.y + height * 0.50;
    let upper_deck_y = base.y + height * 0.76;
    let lower_radius = radius * 3.1;
    let upper_radius = radius * 2.35;
    spawn_tamborn_platform(
        commands,
        meshes,
        pal.bridge_deck.clone(),
        Vec3::new(base.x, lower_deck_y, base.z),
        lower_radius,
        1.8,
        "Tamborn Canopy Platform",
    );
    spawn_tamborn_ring_rail(
        commands,
        meshes,
        pal.vine.clone(),
        Vec3::new(base.x, lower_deck_y, base.z),
        lower_radius + 1.0,
        2.2,
        16,
        0.13,
    );
    spawn_tamborn_platform(
        commands,
        meshes,
        pal.moss.clone(),
        Vec3::new(base.x, upper_deck_y, base.z),
        upper_radius,
        1.6,
        "Tamborn High Canopy Platform",
    );
    spawn_tamborn_ring_rail(
        commands,
        meshes,
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
            let mat = if (level + pod + seed) % 2 == 0 {
                pal.small_window_warm.clone()
            } else {
                pal.small_window_cool.clone()
            };
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(cube_mesh.clone()),
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
                mesh: Mesh3d(sphere_mesh.clone()),
                material: MeshMaterial3d(foliage.clone()),
                transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(&transform, a, 0.34, 0.008, 0.018, 0.006),
        ));

        if crown != 0 {
            spawn_static_cylinder_between(
                commands,
                meshes,
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

    spawn_vine_curtain(
        commands,
        meshes,
        pal,
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
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    center: Vec3,
    radius: f32,
    thickness: f32,
    name: &'static str,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(radius, thickness))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(center),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Name::new(name),
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(thickness * 0.5, radius),
    ));
}

fn spawn_tamborn_ring_rail(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
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
        spawn_static_cylinder_between(commands, meshes, material.clone(), start, end, tube_radius);
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
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
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

    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let yaw = delta.x.atan2(delta.z);
    let pitch = (delta.y / horizontal_len).atan().clamp(-0.46, 0.46);
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    let side_rot = Quat::from_rotation_y(yaw);
    let center = start + delta * 0.5;
    let deck_thickness = 0.42;

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(unit_cube.clone()),
            material: MeshMaterial3d(pal.bridge_deck.clone()),
            transform: Transform::from_translation(center)
                .with_rotation(rot)
                .with_scale(Vec3::new(width, deck_thickness, horizontal_len)),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Name::new("Tamborn Rope Bridge"),
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(
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
                mesh: Mesh3d(unit_cube.clone()),
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
                spawn_static_cylinder_between(commands, meshes, pal.vine.clone(), last, high, 0.16);
            }
            if let Some(last) = last_low {
                spawn_static_cylinder_between(
                    commands,
                    meshes,
                    pal.bark_dark.clone(),
                    last,
                    low,
                    0.13,
                );
            }
            if cable_step % 3 == 0 {
                spawn_static_cylinder_between(commands, meshes, pal.vine.clone(), low, high, 0.08);
            }
            last_high = Some(high);
            last_low = Some(low);
        }
    }
}

fn spawn_giant_mana_tree(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    base: Vec3,
    scale: f32,
    seed: u64,
) {
    let trunk_h = 38.0 * scale;
    let trunk_r = 3.2 * scale;
    let canopy_r = 12.5 * scale;
    let yaw = seeded(seed, 3) * TAU;
    let trunk_mesh = meshes.add(Cylinder::new(1.0, 1.0));
    let sphere_mesh = meshes.add(Sphere::new(1.0));
    let cube_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
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
            mesh: Mesh3d(trunk_mesh.clone()),
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
                mesh: Mesh3d(cube_mesh.clone()),
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
        spawn_static_cylinder_between(
            commands,
            meshes,
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
                mesh: Mesh3d(sphere_mesh.clone()),
                material: MeshMaterial3d(foliage.clone()),
                transform,
                ..default()
            },
            WorldGeometry,
            NatureSway::new(&transform, a, 0.38, 0.010, 0.020, 0.006),
        ));
    }

    spawn_moss_pad(
        commands,
        meshes,
        pal,
        base + Vec3::Y * 0.08,
        trunk_r * 4.8,
        seed + 500,
    );
    spawn_vine_curtain(
        commands,
        meshes,
        pal,
        canopy_center + Vec3::Y * canopy_r * 0.45,
        yaw,
        canopy_r * 0.92,
        trunk_h * 0.82,
        6 + seed % 5,
        seed + 700,
    );

    if seed % 5 == 0 {
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
            NatureSway::new(
                &transform,
                seed as f32 * 0.001 + strip as f32,
                1.1,
                0.010,
                0.045,
                0.006,
            ),
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
}

fn spawn_range_waylines(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    for (ri, route) in mountain_routes().iter().enumerate() {
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
                    crate::physics::prelude::RigidBody::Fixed,
                    crate::physics::prelude::Collider::cuboid(
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
                crate::physics::prelude::RigidBody::Fixed,
                crate::physics::prelude::Collider::cuboid(width * 0.5, 0.55, horizontal_len * 0.5),
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
                    crate::physics::prelude::RigidBody::Fixed,
                    crate::physics::prelude::Collider::cuboid(width * 0.36, tower_h * 0.5, 2.4),
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

fn spawn_speed_road_network(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let deck_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    for (ri, route) in mountain_routes().iter().enumerate() {
        for (si, pair) in route.windows(2).enumerate() {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let delta = Vec2::new(bx - ax, bz - az);
            let segment_len = delta.length();
            if segment_len <= 20.0 {
                continue;
            }

            let chunk_count = speed_road_segment_subdivision_count(segment_len);
            let width = SPEED_ROAD_WIDTH + (ri % 3) as f32 * 2.4;
            for chunk in 0..chunk_count {
                let t0 = chunk as f32 / chunk_count as f32;
                let t1 = (chunk + 1) as f32 / chunk_count as f32;
                let start = speed_road_terrain_point(
                    ax + (bx - ax) * t0,
                    az + (bz - az) * t0,
                    terrain_seed,
                    SPEED_ROAD_CLEARANCE,
                );
                let end = speed_road_terrain_point(
                    ax + (bx - ax) * t1,
                    az + (bz - az) * t1,
                    terrain_seed,
                    SPEED_ROAD_CLEARANCE,
                );
                spawn_speed_road_deck_between(
                    commands,
                    meshes,
                    pal,
                    &deck_mesh,
                    start,
                    end,
                    width,
                    true,
                    chunk % 3 == 1 || chunk_count <= 2,
                    seed + ri as u64 * 7_001 + si as u64 * 503 + chunk as u64 * 19,
                    terrain_seed,
                );
            }

            if segment_len > 1_500.0 && (ri + si) % 2 == 0 {
                let t = 0.48 + seeded(seed, ri as u64 * 179 + si as u64 * 23) * 0.10;
                let x = ax + (bx - ax) * t;
                let z = az + (bz - az) * t;
                let center =
                    speed_road_terrain_point(x, z, terrain_seed, SPEED_ROAD_CLEARANCE + 1.0);
                spawn_speed_loop_gate(
                    commands,
                    meshes,
                    pal,
                    center,
                    delta.x.atan2(delta.y),
                    22.0 + (ri % 2) as f32 * 5.0,
                    width,
                    seed + ri as u64 * 811 + si as u64 * 97,
                );
            }
        }
    }

    spawn_mountain_wrap_ramps(
        commands,
        meshes,
        pal,
        &deck_mesh,
        seed + 30_000,
        terrain_seed,
    );
    spawn_route_sweeper_curves(
        commands,
        meshes,
        pal,
        &deck_mesh,
        seed + 41_000,
        terrain_seed,
    );
    spawn_hoverboard_trick_ramps(commands, meshes, pal, seed + 52_000, terrain_seed);

    for (si, settlement) in map_settlements().iter().copied().enumerate() {
        let plaza_radius = settlement_plaza_radius(settlement.kind);
        let layout = settlement_layout_profile(terrain_seed, settlement, plaza_radius);
        let ring_radius = settlement_speed_ring_radius_for_layout(settlement.kind, layout);
        spawn_settlement_speed_ring(
            commands,
            meshes,
            pal,
            &deck_mesh,
            settlement,
            layout,
            ring_radius,
            seed + si as u64 * 2_003,
            terrain_seed,
        );
        spawn_settlement_speed_spur(
            commands,
            meshes,
            pal,
            &deck_mesh,
            settlement,
            layout,
            ring_radius,
            seed + si as u64 * 2_311,
            terrain_seed,
        );
    }

    spawn_npc_road_vehicles(commands, meshes, pal, seed + 63_000, terrain_seed);
}

fn speed_road_terrain_point(x: f32, z: f32, terrain_seed: u64, clearance: f32) -> Vec3 {
    Vec3::new(x, terrain_surface_y(x, z, terrain_seed) + clearance, z)
}

#[allow(clippy::too_many_arguments)]
fn spawn_speed_road_deck_between(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    mut start: Vec3,
    mut end: Vec3,
    width: f32,
    boost_overlay: bool,
    include_ramps: bool,
    seed: u64,
    terrain_seed: u64,
) {
    let mut delta = end - start;
    let horizontal_len = Vec2::new(delta.x, delta.z).length();
    if horizontal_len <= 4.0 {
        return;
    }

    let needed_lift = speed_road_required_terrain_lift(
        start,
        end,
        width,
        terrain_seed,
        SPEED_ROAD_CLEARANCE + 1.0,
    );
    start.y += needed_lift;
    end.y += needed_lift;
    delta = end - start;

    let length = delta.length();
    let yaw = delta.x.atan2(delta.z);
    let pitch = (delta.y / horizontal_len).atan().clamp(-0.34, 0.34);
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    let center = start + delta * 0.5;
    let deck_thickness = 0.86;

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(deck_mesh.clone()),
            material: MeshMaterial3d(pal.highway.clone()),
            transform: Transform::from_translation(center)
                .with_rotation(rot)
                .with_scale(Vec3::new(width, deck_thickness, length)),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(width * 0.5, deck_thickness * 0.5, length * 0.5),
        world_space_collider_scale(),
    ));

    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(deck_mesh.clone()),
                material: MeshMaterial3d(pal.brushed_metal.clone()),
                transform: Transform::from_translation(
                    center + rot * Vec3::new(side * width * 0.52, 0.86, 0.0),
                )
                .with_rotation(rot)
                .with_scale(Vec3::new(0.52, 1.35, length * 0.96)),
                ..default()
            },
            WorldGeometry,
        ));
    }

    if boost_overlay {
        spawn_boost_road_span(
            commands,
            meshes,
            pal,
            center + Vec3::Y * 0.58,
            yaw,
            pitch,
            length * 0.90,
            width * 0.72,
            include_ramps,
            seed,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_settlement_speed_ring(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    settlement: MapSettlement,
    layout: SettlementTerrainLayout,
    ring_radius: f32,
    seed: u64,
    terrain_seed: u64,
) {
    let segment_count = match settlement.kind {
        MapSettlementKind::City => 18,
        MapSettlementKind::Harbor => 14,
        _ => 12,
    };
    let width = match settlement.kind {
        MapSettlementKind::City => 34.0,
        MapSettlementKind::Harbor => 31.0,
        MapSettlementKind::Village => 28.0,
        MapSettlementKind::Outpost => 26.0,
    };

    for segment in 0..segment_count {
        let a0 = segment as f32 * TAU / segment_count as f32;
        let a1 = (segment + 1) as f32 * TAU / segment_count as f32;
        let start = settlement_speed_ring_point(settlement, layout, ring_radius, a0, terrain_seed);
        let end = settlement_speed_ring_point(settlement, layout, ring_radius, a1, terrain_seed);
        spawn_speed_road_deck_between(
            commands,
            meshes,
            pal,
            deck_mesh,
            start,
            end,
            width,
            true,
            false,
            seed + segment as u64 * 31,
            terrain_seed,
        );
    }

    let gate_angle = std::f32::consts::FRAC_PI_2 - settlement.facing_yaw;
    let gate_pos =
        settlement_speed_ring_point(settlement, layout, ring_radius, gate_angle, terrain_seed);
    let tangent = Vec2::new(-gate_angle.sin(), gate_angle.cos()).normalize_or_zero();
    let yaw = tangent.x.atan2(tangent.y);
    spawn_speed_loop_gate(
        commands,
        meshes,
        pal,
        gate_pos + Vec3::Y * 0.72,
        yaw,
        if matches!(settlement.kind, MapSettlementKind::City) {
            27.0
        } else {
            20.0
        },
        width,
        seed + 997,
    );
}

fn settlement_speed_ring_point(
    settlement: MapSettlement,
    layout: SettlementTerrainLayout,
    ring_radius: f32,
    angle: f32,
    terrain_seed: u64,
) -> Vec3 {
    let x = settlement.x + angle.cos() * ring_radius;
    let z = settlement.z + angle.sin() * ring_radius;
    let y =
        layout.floor_y_at(x, z, terrain_seed) + if layout.is_sky_district() { 3.2 } else { 2.4 };
    Vec3::new(x, y, z)
}

#[allow(clippy::too_many_arguments)]
fn spawn_settlement_speed_spur(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    settlement: MapSettlement,
    layout: SettlementTerrainLayout,
    ring_radius: f32,
    seed: u64,
    terrain_seed: u64,
) {
    let Some(route_projection) = nearest_mountain_route_point(settlement.x, settlement.z) else {
        return;
    };
    let center = Vec2::new(settlement.x, settlement.z);
    let to_route = route_projection.point - center;
    if to_route.length_squared() <= 0.01 {
        return;
    }

    let dir = to_route.normalize();
    let start_angle = dir.y.atan2(dir.x);
    let start =
        settlement_speed_ring_point(settlement, layout, ring_radius, start_angle, terrain_seed);
    let end = speed_road_terrain_point(
        route_projection.point.x,
        route_projection.point.y,
        terrain_seed,
        SPEED_ROAD_CLEARANCE + 0.6,
    );
    let total_len = Vec2::new(end.x - start.x, end.z - start.z).length();
    let chunk_count = speed_road_segment_subdivision_count(total_len).max(1);

    for chunk in 0..chunk_count {
        let t0 = chunk as f32 / chunk_count as f32;
        let t1 = (chunk + 1) as f32 / chunk_count as f32;
        spawn_speed_road_deck_between(
            commands,
            meshes,
            pal,
            deck_mesh,
            start.lerp(end, t0),
            start.lerp(end, t1),
            SPEED_ROAD_WIDTH * 0.86,
            true,
            chunk == 0 || chunk + 1 == chunk_count,
            seed + chunk as u64 * 41,
            terrain_seed,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_route_sweeper_curves(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    seed: u64,
    terrain_seed: u64,
) {
    for (ri, route) in mountain_routes().iter().enumerate() {
        for vi in 1..route.len().saturating_sub(1) {
            let (px, pz) = route[vi - 1];
            let (cx, cz) = route[vi];
            let (nx, nz) = route[vi + 1];
            let into = Vec2::new(cx - px, cz - pz).normalize_or_zero();
            let out = Vec2::new(nx - cx, nz - cz).normalize_or_zero();
            if into.length_squared() <= 0.01 || out.length_squared() <= 0.01 {
                continue;
            }
            let turn_strength = (1.0 - into.dot(out)).clamp(0.0, 2.0);
            if turn_strength < 0.10 {
                continue;
            }

            let p0 = Vec2::new(cx, cz) - into * 230.0;
            let p2 = Vec2::new(cx, cz) + out * 230.0;
            let outside = Vec2::new(-into.y - out.y, into.x + out.x).normalize_or_zero();
            let control = Vec2::new(cx, cz)
                + outside * (95.0 + turn_strength * 95.0)
                + (out - into).normalize_or_zero() * 36.0;
            let steps = 7usize;
            let mut last =
                speed_road_terrain_point(p0.x, p0.y, terrain_seed, SPEED_ROAD_CLEARANCE + 1.2);

            for step in 1..=steps {
                let t = step as f32 / steps as f32;
                let curve = quadratic_bezier_xz(p0, control, p2, t);
                let next = speed_road_terrain_point(
                    curve.x,
                    curve.y,
                    terrain_seed,
                    SPEED_ROAD_CLEARANCE + 1.2,
                );
                spawn_speed_road_deck_between(
                    commands,
                    meshes,
                    pal,
                    deck_mesh,
                    last,
                    next,
                    SPEED_ROAD_WIDTH * (1.04 + turn_strength * 0.08),
                    true,
                    step == 2 || step == steps - 1,
                    seed + ri as u64 * 1_009 + vi as u64 * 67 + step as u64,
                    terrain_seed,
                );
                last = next;
            }
        }
    }
}

fn quadratic_bezier_xz(a: Vec2, control: Vec2, b: Vec2, t: f32) -> Vec2 {
    let inv = 1.0 - t;
    a * inv * inv + control * 2.0 * inv * t + b * t * t
}

fn speed_road_sweeper_curve_count() -> usize {
    mountain_routes()
        .iter()
        .map(|route| route.len().saturating_sub(2))
        .sum()
}

fn spawn_hoverboard_trick_ramps(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let mut spawned = 0usize;
    for (ri, route) in mountain_routes().iter().enumerate() {
        for (si, pair) in route.windows(2).enumerate() {
            let (ax, az) = pair[0];
            let (bx, bz) = pair[1];
            let delta = Vec2::new(bx - ax, bz - az);
            let len = delta.length();
            if len < 1_250.0 {
                continue;
            }
            let yaw = delta.x.atan2(delta.y);
            let dir = Vec3::new(delta.x, 0.0, delta.y).normalize_or_zero();
            let right = Vec3::new(dir.z, 0.0, -dir.x).normalize_or_zero();
            for (slot, t) in [0.32_f32, 0.68].into_iter().enumerate() {
                if spawned >= hoverboard_trick_ramp_count() {
                    return;
                }
                if (ri + si + slot) % 2 == 1 {
                    continue;
                }
                let x = ax + (bx - ax) * t;
                let z = az + (bz - az) * t;
                let lane = if slot == 0 { -1.0 } else { 1.0 };
                let center = speed_road_terrain_point(
                    x + right.x * lane * SPEED_ROAD_TRAFFIC_LANE_OFFSET,
                    z + right.z * lane * SPEED_ROAD_TRAFFIC_LANE_OFFSET,
                    terrain_seed,
                    SPEED_ROAD_CLEARANCE + 1.05,
                );
                spawn_board_boost_ramp(
                    commands, meshes, pal, center, yaw, dir, 18.0, 54.0, 18.0, 7.2, 2.35,
                );
                spawn_speed_loop_gate(
                    commands,
                    meshes,
                    pal,
                    center + dir * 42.0 + Vec3::Y * 2.4,
                    yaw,
                    18.0 + seeded(seed, spawned as u64 * 17) * 7.0,
                    24.0,
                    seed + spawned as u64 * 97,
                );
                spawned += 1;
            }
        }
    }
}

fn hoverboard_trick_ramp_count() -> usize {
    16
}

#[allow(clippy::too_many_arguments)]
fn spawn_mountain_wrap_ramps(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    seed: u64,
    terrain_seed: u64,
) {
    let mut spawned = 0usize;
    for (ri, route) in mountain_routes().iter().enumerate() {
        for vertex in route
            .iter()
            .copied()
            .enumerate()
            .skip(1)
            .take(route.len().saturating_sub(2))
        {
            if spawned >= 8 {
                return;
            }
            let (vi, (x, z)) = vertex;
            if (ri + vi) % 2 != 0 || terrain_surface_y(x, z, terrain_seed) < 90.0 {
                continue;
            }

            let radius = 88.0 + seeded(seed, ri as u64 * 211 + vi as u64 * 17) * 46.0;
            let height = 28.0 + seeded(seed, ri as u64 * 227 + vi as u64 * 19) * 28.0;
            let start_angle = seeded(seed, ri as u64 * 239 + vi as u64 * 23) * TAU;
            spawn_helical_speed_ramp(
                commands,
                meshes,
                pal,
                deck_mesh,
                Vec2::new(x, z),
                radius,
                SPEED_ROAD_WIDTH * 0.82,
                1.08,
                height * 1.28,
                start_angle,
                seed + ri as u64 * 997 + vi as u64 * 71,
                terrain_seed,
            );
            spawned += 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_helical_speed_ramp(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    deck_mesh: &Handle<Mesh>,
    center: Vec2,
    radius: f32,
    width: f32,
    turns: f32,
    height: f32,
    start_angle: f32,
    seed: u64,
    terrain_seed: u64,
) {
    let steps = (turns * 14.0).ceil().clamp(8.0, 18.0) as usize;
    let angle_span = turns * TAU;
    for step in 0..steps {
        let t0 = step as f32 / steps as f32;
        let t1 = (step + 1) as f32 / steps as f32;
        let a0 = start_angle + angle_span * t0;
        let a1 = start_angle + angle_span * t1;
        let point = |angle: f32, t: f32| {
            let x = center.x + angle.cos() * radius;
            let z = center.y + angle.sin() * radius;
            let y = terrain_surface_y(x, z, terrain_seed) + SPEED_ROAD_CLEARANCE + 1.4 + height * t;
            Vec3::new(x, y, z)
        };

        spawn_speed_road_deck_between(
            commands,
            meshes,
            pal,
            deck_mesh,
            point(a0, t0),
            point(a1, t1),
            width,
            true,
            step == 0 || step + 1 == steps,
            seed + step as u64 * 29,
            terrain_seed,
        );
    }
}

fn spawn_speed_loop_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    yaw: f32,
    radius: f32,
    width: f32,
    seed: u64,
) {
    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let yaw_rot = Quat::from_rotation_y(yaw);
    let forward = yaw_rot * Vec3::Z;
    let segments = 18usize;
    let arc_len = TAU * radius / segments as f32 * 1.07;
    let segment_width = (width * 0.36).clamp(7.0, 10.0);

    for segment in 0..segments {
        let a0 = segment as f32 * TAU / segments as f32;
        let a1 = (segment + 1) as f32 * TAU / segments as f32;
        let angle = (a0 + a1) * 0.5 - std::f32::consts::FRAC_PI_2;
        let local = Vec3::new(0.0, radius + angle.sin() * radius, angle.cos() * radius);
        let pos = center + yaw_rot * local;
        let rot = yaw_rot * Quat::from_rotation_x(angle + std::f32::consts::FRAC_PI_2);
        let transform = Transform::from_translation(pos)
            .with_rotation(rot)
            .with_scale(Vec3::new(segment_width, 0.62, arc_len));

        if angle.sin() < -0.20 {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(pal.boost_ramp.clone()),
                    transform,
                    ..default()
                },
                WorldGeometry,
                WalkableSurface,
                crate::physics::prelude::RigidBody::Fixed,
                crate::physics::prelude::Collider::cuboid(segment_width * 0.5, 0.31, arc_len * 0.5),
                world_space_collider_scale(),
            ));
        } else {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(unit_cube.clone()),
                    material: MeshMaterial3d(if segment % 2 == 0 {
                        pal.boost_lane.clone()
                    } else {
                        pal.guide_glow.clone()
                    }),
                    transform,
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    spawn_board_boost_pad(
        commands,
        meshes,
        pal,
        center + Vec3::Y * 0.34,
        yaw,
        0.0,
        forward,
        width * 0.54,
        radius * 1.45,
        3.35,
        5.0,
        0.75,
        1.65,
    );

    for end in [-1.0_f32, 1.0] {
        let ramp_dir = if end > 0.0 { forward } else { -forward };
        let ramp_yaw = if end > 0.0 {
            yaw
        } else {
            yaw + std::f32::consts::PI
        };
        spawn_board_boost_ramp(
            commands,
            meshes,
            pal,
            center + forward * end * radius * 1.16 + Vec3::Y * 0.80,
            ramp_yaw,
            ramp_dir,
            width * 0.48,
            radius * 1.05,
            7.0 + seeded(seed, end.to_bits() as u64) * 2.0,
            5.2,
            1.15,
        );
    }
}

fn spawn_npc_road_vehicles(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
    terrain_seed: u64,
) {
    let vehicle_mesh = meshes.add(Cuboid::new(8.2, 2.5, 13.2));
    let mut spawned = 0usize;

    for (ri, route) in mountain_routes().iter().enumerate() {
        if route.len() < 2 {
            continue;
        }
        let path = road_vehicle_route_path(route, terrain_seed);
        if path.len() < 2 {
            continue;
        }

        for lane in [-1.0_f32, 1.0] {
            if spawned >= SPEED_ROAD_PATROL_VEHICLE_COUNT {
                return;
            }
            let speed = 34.0 + seeded(seed, spawned as u64 * 13 + 1) * 18.0;
            let initial_distance = seeded(seed, spawned as u64 * 13 + 2) * 1_800.0;
            let lane_offset = lane * SPEED_ROAD_TRAFFIC_LANE_OFFSET;
            let mut vehicle = NpcRoadVehicle {
                path: path.clone(),
                segment: spawned % path.len(),
                progress: 0.0,
                speed,
                lane_offset,
                hit_radius: 7.5,
                wreck_timer: 5.0,
            };
            advance_road_vehicle(&mut vehicle, initial_distance / speed.max(0.01));
            let (position, yaw) =
                road_vehicle_pose(&vehicle).unwrap_or((path[0], route_heading_yaw(route)));
            let route_dir = Quat::from_rotation_y(yaw) * Vec3::Z;
            let right = Vec3::new(route_dir.z, 0.0, -route_dir.x).normalize_or_zero();
            let color_mat = match (ri + spawned) % 4 {
                0 => pal.brushed_metal.clone(),
                1 => pal.industrial_metal.clone(),
                2 => pal.city_metal_panel.clone(),
                _ => pal.crystal_aurora.clone(),
            };

            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(vehicle_mesh.clone()),
                    material: MeshMaterial3d(color_mat),
                    transform: Transform::from_translation(position + right * lane_offset)
                        .with_rotation(Quat::from_rotation_y(yaw)),
                    ..default()
                },
                WorldGeometry,
                vehicle,
                Health::new(85.0),
                Damageable::default(),
                crate::physics::prelude::RigidBody::KinematicPositionBased,
                crate::physics::prelude::Collider::cuboid(4.1, 1.25, 6.6),
            ));
            spawned += 1;
        }
    }
}

fn road_vehicle_route_path(route: &[(f32, f32)], terrain_seed: u64) -> Vec<Vec3> {
    let mut path = Vec::new();
    append_speed_road_path_samples(
        &mut path,
        route.iter().copied(),
        terrain_seed,
        SPEED_ROAD_CLEARANCE + 2.0,
    );
    append_speed_road_path_samples(
        &mut path,
        route.iter().rev().copied(),
        terrain_seed,
        SPEED_ROAD_CLEARANCE + 2.0,
    );
    path
}

fn append_speed_road_path_samples<I>(
    path: &mut Vec<Vec3>,
    points: I,
    terrain_seed: u64,
    clearance: f32,
) where
    I: IntoIterator<Item = (f32, f32)>,
{
    let points: Vec<(f32, f32)> = points.into_iter().collect();
    for pair in points.windows(2) {
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
                clearance,
            );
            let mut end = speed_road_terrain_point(
                ax + (bx - ax) * t1,
                az + (bz - az) * t1,
                terrain_seed,
                clearance,
            );
            let lift = speed_road_required_terrain_lift(
                start,
                end,
                SPEED_ROAD_WIDTH,
                terrain_seed,
                clearance,
            );
            start.y += lift;
            end.y += lift;
            push_speed_road_path_point(path, start);
            push_speed_road_path_point(path, end);
        }
    }
}

fn push_speed_road_path_point(path: &mut Vec<Vec3>, point: Vec3) {
    if let Some(last) = path.last_mut() {
        let same_xz = Vec2::new(last.x - point.x, last.z - point.z).length_squared() < 0.01;
        if same_xz {
            last.y = last.y.max(point.y);
            return;
        }
    }
    path.push(point);
}

fn route_heading_yaw(route: &[(f32, f32)]) -> f32 {
    if route.len() < 2 {
        return 0.0;
    }
    let (ax, az) = route[0];
    let (bx, bz) = route[1];
    (bx - ax).atan2(bz - az)
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(width * 0.5, 1.0, depth * 0.5),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(0.28, plaza_radius),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(platform_height * 0.5, layout.platform_radius),
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

    for i in 0..3 {
        let angle = settlement.facing_yaw + i as f32 * std::f32::consts::TAU / 3.0;
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(width * 0.5, thickness * 0.5, length * 0.5),
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

        spawn_building(
            commands,
            meshes,
            material,
            Vec3::new(world.x, floor_y + height * 0.5, world.z),
            width,
            height,
            depth,
            match settlement.kind {
                MapSettlementKind::City => WorldZone::Downtown,
                MapSettlementKind::Village => WorldZone::Residential,
                MapSettlementKind::Harbor => WorldZone::OuterDistrict,
                MapSettlementKind::Outpost => WorldZone::Spaceport,
            },
        );

        if !matches!(settlement.kind, MapSettlementKind::City) {
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

        if matches!(
            settlement.kind,
            MapSettlementKind::City | MapSettlementKind::Harbor
        ) && i % 2 == 0
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

        if matches!(settlement.kind, MapSettlementKind::City) {
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(0.17, 3.4),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
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

        // Facade accent band near base
        if i % 3 == 0 {
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
        spawn_window_facades(commands, meshes, win_mat, x, z, h, w, d);
        spawn_modern_window_grid(commands, meshes, pal, x, z, h, w, d, seed + i * 17);
        if i % 7 == 0 {
            spawn_metal_cladding(
                commands,
                meshes,
                pal,
                Vec3::new(x, h * 0.5, z),
                w,
                h,
                d,
                seed + i,
            );
        }
        if i % 17 == 0 {
            spawn_metal_cladding(
                commands,
                meshes,
                pal,
                Vec3::new(x, h * 0.5, z),
                w + 0.7,
                h * 0.72,
                d + 0.7,
                seed + i * 5 + 3,
            );
        }
        if i % 13 == 0 {
            spawn_stone_brick_courses(
                commands,
                meshes,
                pal,
                Vec3::new(x, h * 0.5, z),
                w,
                h,
                d,
                true,
            );
        }

        // Rooftop details on taller buildings
        if h > 50.0 {
            spawn_rooftop_details(commands, meshes, pal, x, z, h, w, d, seed + i);
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
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
        crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
        crate::physics::prelude::RigidBody::KinematicPositionBased,
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
        crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
        crate::physics::prelude::RigidBody::KinematicPositionBased,
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
        crate::physics::prelude::Collider::cylinder(0.11, radius),
        crate::physics::prelude::RigidBody::Fixed,
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
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
    meshes: &mut Assets<Mesh>,
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
                mesh: Mesh3d(meshes.add(Cuboid::new(w - 0.8, win_h, thickness))),
                material: MeshMaterial3d(mat.clone()),
                transform: Transform::from_xyz(x, center_y, fz),
                ..default()
            },
            WorldGeometry,
        ));
    }
    // Left and right faces (span full depth, aligned along Z)
    for &fx in &[x - w * 0.5 - thickness * 0.5, x + w * 0.5 + thickness * 0.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(thickness, win_h, d - 0.8))),
                material: MeshMaterial3d(mat.clone()),
                transform: Transform::from_xyz(fx, center_y, z),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

fn spawn_modern_window_grid(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(w - 1.2, win_h, glass_thickness))),
                    material: MeshMaterial3d(pal.glass_panel.clone()),
                    transform: Transform::from_xyz(x, center_y, fz),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        for &fx in &[x - w * 0.5 - 0.45, x + w * 0.5 + 0.45] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(glass_thickness, win_h, d - 1.2))),
                    material: MeshMaterial3d(pal.glass_panel.clone()),
                    transform: Transform::from_xyz(fx, center_y, z),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(w - 0.7, frame_thickness, 0.24))),
                    material: MeshMaterial3d(frame_mat.clone()),
                    transform: Transform::from_xyz(x, y, fz),
                    ..default()
                },
                WorldGeometry,
            ));
        }
        for &fx in &[x - w * 0.5 - 0.58, x + w * 0.5 + 0.58] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.24, frame_thickness, d - 0.7))),
                    material: MeshMaterial3d(frame_mat.clone()),
                    transform: Transform::from_xyz(fx, y, z),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(frame_thickness, win_h, 0.26))),
                    material: MeshMaterial3d(frame_mat.clone()),
                    transform: Transform::from_xyz(x + x_off, center_y, fz),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.26, win_h, frame_thickness))),
                    material: MeshMaterial3d(frame_mat.clone()),
                    transform: Transform::from_xyz(fx, center_y, z + z_off),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.26, rib_h, 0.28))),
                    material: MeshMaterial3d(rib_mat.clone()),
                    transform: Transform::from_xyz(position.x + x_off, rib_y, fz),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.28, rib_h, 0.26))),
                    material: MeshMaterial3d(rib_mat.clone()),
                    transform: Transform::from_xyz(fx, rib_y, position.z + z_off),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(width + 0.55, 0.22, 0.30))),
                    material: MeshMaterial3d(rib_mat.clone()),
                    transform: Transform::from_xyz(position.x, y, fz),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.30, 0.22, depth + 0.55))),
                    material: MeshMaterial3d(rib_mat.clone()),
                    transform: Transform::from_xyz(fx, y, position.z),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(width + 0.20, 0.08, 0.12))),
                    material: MeshMaterial3d(pal.mortar_line.clone()),
                    transform: Transform::from_xyz(position.x, y, fz),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.12, 0.08, depth + 0.20))),
                    material: MeshMaterial3d(pal.mortar_line.clone()),
                    transform: Transform::from_xyz(fx, y, position.z),
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
                        mesh: Mesh3d(meshes.add(Cuboid::new(0.08, seam_h, 0.14))),
                        material: MeshMaterial3d(pal.mortar_line.clone()),
                        transform: Transform::from_xyz(position.x + x_off, y, fz),
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
                        mesh: Mesh3d(meshes.add(Cuboid::new(0.14, seam_h, 0.08))),
                        material: MeshMaterial3d(pal.mortar_line.clone()),
                        transform: Transform::from_xyz(fx, y, position.z + z_off),
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
                        mesh: Mesh3d(meshes.add(Cuboid::new(0.42, height * 0.92, 0.42))),
                        material: MeshMaterial3d(pal.stone_block.clone()),
                        transform: Transform::from_xyz(x, position.y, z),
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
    meshes: &mut Assets<Mesh>,
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
                mesh: Mesh3d(meshes.add(Cylinder::new(0.22, mast_h))),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: Transform::from_xyz(x, top + mast_h * 0.5, z),
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
                mesh: Mesh3d(meshes.add(Sphere::new(0.45))),
                material: MeshMaterial3d(pal.window_warm.clone()),
                transform: Transform::from_xyz(x, top + mast_h + 0.5, z),
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
                mesh: Mesh3d(meshes.add(Cylinder::new(tank_r, tank_h))),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: Transform::from_xyz(x + ox, top + tank_h * 0.5, z + oz),
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
                mesh: Mesh3d(meshes.add(Cylinder::new(dish_r, 0.25))),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: Transform::from_xyz(x + ox, top + 1.5, z + oz)
                    .with_rotation(Quat::from_rotation_x(0.45)),
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
        if i % 3 == 0 {
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
    let flat_rot = Quat::from_rotation_y(yaw);
    let rot = flat_rot * Quat::from_rotation_x(-pitch);
    let forward = flat_rot * Vec3::Z;
    let right = flat_rot * Vec3::X;
    let lane_width = (width * 0.40).max(3.2);
    let side_offset = width * 0.24;
    let unit_cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let light_mesh = meshes.add(Sphere::new(1.0));

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
                mesh: Mesh3d(unit_cube.clone()),
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
            spawn_board_boost_pad(
                commands, meshes, pal, pad_center, lane_yaw, pitch, lane_dir, pad_width, pad_len,
                3.15, 3.10, 0.0, 1.70,
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
                        mesh: Mesh3d(light_mesh.clone()),
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
                let ramp_dir = if end > 0.0 { lane_dir } else { -lane_dir };
                let ramp_yaw = if end > 0.0 {
                    lane_yaw
                } else {
                    lane_yaw + std::f32::consts::PI
                };
                let ramp_center =
                    lane_center + forward * end * length * 0.43 + Vec3::Y * (1.0 + end.abs());
                spawn_board_boost_ramp(
                    commands,
                    meshes,
                    pal,
                    ramp_center,
                    ramp_yaw,
                    ramp_dir,
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

fn spawn_board_boost_pad(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
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
            mesh: Mesh3d(meshes.add(Cuboid::new(width, 0.16, length))),
            material: MeshMaterial3d(pal.boost_pad.clone()),
            transform: Transform::from_translation(center).with_rotation(rot),
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
            cooldown_timer: 0.0,
            force_hoverboard: true,
        },
    ));

    let chevron_mesh = meshes.add(Cuboid::new(width * 0.34, 0.05, 0.82));
    for (i, offset) in [-0.26_f32, 0.0, 0.26].iter().copied().enumerate() {
        let z = offset * length;
        for side in [-1.0_f32, 1.0] {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(chevron_mesh.clone()),
                    material: MeshMaterial3d(pal.boost_ramp.clone()),
                    transform: Transform::from_translation(
                        center + rot * Vec3::new(side * width * 0.13, 0.115, z),
                    )
                    .with_rotation(
                        rot * Quat::from_rotation_y(side * 0.34)
                            * Quat::from_rotation_z(if i % 2 == 0 { 0.03 } else { -0.03 }),
                    ),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }
}

fn spawn_board_boost_ramp(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    center: Vec3,
    yaw: f32,
    direction: Vec3,
    width: f32,
    length: f32,
    rise: f32,
    impulse: f32,
    lift: f32,
) {
    let pitch = (rise / length.max(0.1)).atan();
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width, 0.72, length))),
            material: MeshMaterial3d(pal.boost_ramp.clone()),
            transform: Transform::from_translation(center).with_rotation(rot),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        BoardBoostPad {
            direction,
            half_width: width * 0.5,
            half_length: length * 0.5,
            speed_mult: 3.15,
            impulse: impulse.max(3.9),
            lift,
            duration: 1.85,
            cooldown: 0.20,
            cooldown_timer: 0.0,
            force_hoverboard: true,
        },
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(width * 0.5, 0.36, length * 0.5),
    ));
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(road_half, 0.4, 9.0),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(9.0, 0.4, road_half),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(1.25, 4.0, 1.25),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cylinder(1.5, size),
        ));
    }
}

// ── Sky Bridges ───────────────────────────────────────────────────────────────
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(width * 0.5, 0.75, len * 0.5),
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
            crate::physics::prelude::RigidBody::KinematicPositionBased,
            crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
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
                crate::physics::prelude::RigidBody::Fixed,
                crate::physics::prelude::Collider::cylinder(0.6, 0.9),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cylinder(1.0, 50.0),
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

// ── Grasslands ───────────────────────────────────────────────────────────────
//
// L-system grass: axiom F, rule F→F[+F][-F], 2 iterations.
// The turtle runs on the XZ floor plane (Y is up). Each F step records a blade
// position. At each position two crossed vertical Rectangle quads are spawned,
// giving the old-school cross-billboard tuft look without any camera-facing math.
fn spawn_grasslands(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    grass_mats: &mut Assets<GrassMaterial>,
    seed: u64,
) {
    // ── Shared blade meshes (two sizes for variety) ───────────────────────
    // Rectangle lives in local XY; standing upright in world space by default.
    let blade_tall = meshes.add(Rectangle::new(0.07, 0.72));
    let blade_wide = meshes.add(Rectangle::new(0.09, 0.52));

    // ── Three wind-grass material variants (linear sRGB greens) ──────────
    let mats: [Handle<GrassMaterial>; 3] = [
        grass_mats.add(GrassMaterial {
            color: Vec4::new(0.09, 0.27, 0.07, 1.0),
        }),
        grass_mats.add(GrassMaterial {
            color: Vec4::new(0.13, 0.35, 0.10, 1.0),
        }),
        grass_mats.add(GrassMaterial {
            color: Vec4::new(0.07, 0.22, 0.05, 1.0),
        }),
    ];

    // ── L-system string, evaluated once ──────────────────────────────────
    // F → F[+F][-F], 2 iterations → 9 F nodes per tuft.
    let ls: String = {
        let mut s = String::from("F");
        for _ in 0..2 {
            s = s
                .chars()
                .flat_map(|c| {
                    if c == 'F' {
                        "F[+F][-F]".chars().collect::<Vec<_>>()
                    } else {
                        vec![c]
                    }
                })
                .collect();
        }
        s
    };

    const ANGLE: f32 = 22.0 * std::f32::consts::PI / 180.0;
    const BASE_STEP: f32 = 0.38;
    const STEP_SCALE: f32 = 0.76; // branches shrink each push level

    for i in 0..1_260u64 {
        let x = seeded(seed, i * 4) * 2300.0 - 1150.0;
        let z = seeded(seed, i * 4 + 1) * 2300.0 - 1150.0;
        let dist = (x * x + z * z).sqrt();
        if dist < 95.0 {
            continue;
        }

        let y = terrain_surface_y(x, z, seed - 10);
        if !(0.05..115.0).contains(&y) {
            continue;
        }

        let root_dir = seeded(seed, i * 5) * std::f32::consts::TAU;
        let mat = &mats[(i % 3) as usize];
        let is_tall = seeded(seed, i * 7) > 0.45;
        let blade_mesh = if is_tall { &blade_tall } else { &blade_wide };
        let blade_h = if is_tall { 0.72_f32 } else { 0.52_f32 };

        // ── XZ-plane turtle ───────────────────────────────────────────────
        // State: (local_x, local_z, direction_radians, step_length)
        let mut stack: Vec<(f32, f32, f32, f32)> = Vec::new();
        let mut lx = 0.0_f32;
        let mut lz = 0.0_f32;
        let mut dir = root_dir;
        let mut step = BASE_STEP;

        for ch in ls.chars() {
            match ch {
                'F' => {
                    lx += step * dir.sin();
                    lz += step * dir.cos();
                    let wx = x + lx;
                    let wz = z + lz;
                    // Two crossed quads: one at dir, one rotated 90°
                    for cross in [0.0_f32, std::f32::consts::FRAC_PI_2] {
                        commands.spawn((
                            Mesh3d(blade_mesh.clone()),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_xyz(wx, y + blade_h * 0.5, wz)
                                .with_rotation(Quat::from_rotation_y(dir + cross)),
                            WorldGeometry,
                        ));
                    }
                }
                '+' => dir += ANGLE,
                '-' => dir -= ANGLE,
                '[' => {
                    stack.push((lx, lz, dir, step));
                    step *= STEP_SCALE;
                }
                ']' => {
                    if let Some((sx, sz, sd, ss)) = stack.pop() {
                        lx = sx;
                        lz = sz;
                        dir = sd;
                        step = ss;
                    }
                }
                _ => {}
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
        }
    }
}

// ── River ─────────────────────────────────────────────────────────────────────
fn spawn_river(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette) {
    for i in -15..=15i32 {
        let z = i as f32 * 40.0;
        let x = (z * 0.05).sin() * 60.0;

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(25.0, 0.1, 42.0))),
                material: MeshMaterial3d(pal.water.clone()),
                transform: Transform::from_xyz(x, -0.1, z),
                ..default()
            },
            WorldGeometry,
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
    let yaw = if style % 2 == 0 { 0.08 } else { -0.11 };
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(
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
) {
    let ocean_transform = Transform::from_xyz(0.0, 0.18, 760.0);
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(840.0, 0.08, 430.0))),
            material: MeshMaterial3d(pal.water.clone()),
            transform: ocean_transform,
            ..default()
        },
        WorldGeometry,
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(15.0, 0.07, 157.0),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(12.0, 0.21, 19.0),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(0.36, 74.0),
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
    // Build one template per species (L-system evaluated once, materials allocated once).
    // All species share a single unit cylinder + unit sphere mesh so the
    // thousands of branch/leaf entities batch into a handful of draw calls.
    let branch_mesh = TreeTemplate::unit_branch_mesh(meshes);
    let leaf_mesh = TreeTemplate::unit_leaf_mesh(meshes);
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
                spawn_moss_pad(
                    commands,
                    meshes,
                    pal,
                    pos + Vec3::Y * 0.035,
                    sc * (1.75 + seeded(seed, idx * 4 + 7) * 1.05),
                    seed + idx * 43,
                );
            }
            match p.template.kind {
                TreeKind::Oak if idx % 3 != 1 => spawn_vine_curtain(
                    commands,
                    meshes,
                    pal,
                    pos + Vec3::Y * (sc * 3.5),
                    rot,
                    sc * 1.9,
                    sc * 2.7,
                    2 + idx % 3,
                    seed + idx * 47,
                ),
                TreeKind::Pine if idx % 4 == 0 => spawn_vine_curtain(
                    commands,
                    meshes,
                    pal,
                    pos + Vec3::Y * (sc * 3.1),
                    rot + 0.5,
                    sc * 1.25,
                    sc * 2.2,
                    2,
                    seed + idx * 53,
                ),
                TreeKind::Dead if idx % 2 == 0 => spawn_vine_curtain(
                    commands,
                    meshes,
                    pal,
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(width * 0.5, height * 0.5, depth * 0.5),
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
            mesh: Mesh3d(meshes.add(Cuboid::new(width + 0.45, 0.42, depth + 0.45))),
            material: MeshMaterial3d(pal.stone_block.clone()),
            transform: Transform::from_xyz(position.x, ground_y + 0.21, position.z),
            ..default()
        },
        WorldGeometry,
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width + 0.65, 0.34, depth + 0.65))),
            material: MeshMaterial3d(trim.clone()),
            transform: Transform::from_xyz(position.x, top_y + 0.17, position.z),
            ..default()
        },
        WorldGeometry,
    ));

    if seeded(seed, 91) > 0.45 {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(width * 0.62, 0.16, depth * 0.52))),
                material: MeshMaterial3d(pal.moss.clone()),
                transform: Transform::from_xyz(position.x, top_y + 0.42, position.z),
                ..default()
            },
            WorldGeometry,
            NatureSway::new(
                &Transform::from_xyz(position.x, top_y + 0.42, position.z),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(width + 0.10, row_h, 0.09))),
                    material: MeshMaterial3d(course_line.clone()),
                    transform: Transform::from_xyz(position.x, y, z),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.09, row_h, depth + 0.10))),
                    material: MeshMaterial3d(course_line.clone()),
                    transform: Transform::from_xyz(x, y, position.z),
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
                    mesh: Mesh3d(meshes.add(Cuboid::new(0.22, height * 0.88, 0.22))),
                    material: MeshMaterial3d(pal.stone_block.clone()),
                    transform: Transform::from_xyz(x, ground_y + height * 0.44, z),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    if stone_variant {
        spawn_stone_brick_courses(commands, meshes, pal, position, width, height, depth, false);
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
                        mesh: Mesh3d(meshes.add(Cuboid::new(win_w, win_h, 0.12))),
                        material: MeshMaterial3d(mat.clone()),
                        transform: Transform::from_xyz(x, y, z),
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
                        mesh: Mesh3d(meshes.add(Cuboid::new(0.12, win_h, win_w))),
                        material: MeshMaterial3d(mat.clone()),
                        transform: Transform::from_xyz(x, y, z),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(body_h * 0.5, radius),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(4.0, 92.0),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(ww * 0.5, wall_h * 0.5, wd * 0.5),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(3.0, 82.0),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cylinder(rh * 0.5, rh * 0.1),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(ww * 0.5, wall_h * 0.5, wd * 0.5),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cylinder(46.0, 11.0),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cuboid(29.0, keep_h * 0.5, 24.0),
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
        crate::physics::prelude::RigidBody::Fixed,
        crate::physics::prelude::Collider::cylinder(67.5, 16.0),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(20.0, 1.75, 3.5),
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
            crate::physics::prelude::RigidBody::Fixed,
            crate::physics::prelude::Collider::cuboid(3.0, 11.0, 3.0),
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

            let mat = if !is_dragon && (zi + i as usize) % 4 == 0 {
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
                let light_col = if !is_dragon && (zi + i as usize) % 4 == 0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everest_heightmap_asset_loads() {
        assert!(everest_heightmap().is_some());
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
    fn mountain_routes_cover_multiple_passes() {
        assert!(mountain_routes().len() >= 8);
        assert!(mountain_routes().iter().all(|route| route.len() >= 2));
    }

    #[test]
    fn mountain_route_influence_marks_path_corridors() {
        assert!(mountain_route_influence(3200.0, 920.0) > 0.85);
        assert!(mountain_route_influence(9900.0, -1000.0) < 0.20);
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
    fn speed_roads_are_wide_enough_for_patrol_traffic() {
        assert!(SPEED_ROAD_WIDTH >= 36.0);
        assert!(SPEED_ROAD_TRAFFIC_LANE_OFFSET * 2.0 < SPEED_ROAD_WIDTH);
        assert_eq!(SPEED_ROAD_PATROL_VEHICLE_COUNT, 18);
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
                    SPEED_ROAD_CLEARANCE + 0.98,
                );
            }
        }
    }

    #[test]
    fn scaled_travel_colliders_keep_world_space_extents() {
        assert_eq!(
            world_space_collider_scale(),
            crate::physics::prelude::ColliderScale::Absolute(Vec3::ONE)
        );
    }

    #[test]
    fn mountain_routes_generate_sweeper_curves_and_trick_ramps() {
        assert!(speed_road_sweeper_curve_count() >= 20);
        assert!(hoverboard_trick_ramp_count() >= 16);
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
        let samples = (horizontal_len / SPEED_ROAD_TERRAIN_SAMPLE_SPACING)
            .ceil()
            .clamp(1.0, 16.0) as usize;
        let edge_offset = width * SPEED_ROAD_EDGE_SAMPLE_FRACTION;
        for i in 0..=samples {
            let t = i as f32 / samples as f32;
            let center = Vec2::new(start.x + delta_xz.x * t, start.z + delta_xz.y * t);
            let road_y = start.y + (end.y - start.y) * t;
            for offset in [0.0_f32, -edge_offset, edge_offset] {
                let sample = center + right * offset;
                let ground = terrain_surface_y(sample.x, sample.y, terrain_seed);
                assert!(
                    road_y >= ground + min_clearance,
                    "speed road sample sank below terrain clearance at ({:.1}, {:.1})",
                    sample.x,
                    sample.y
                );
            }
        }
    }

    #[test]
    fn terrain_vertex_color_adds_high_snowline() {
        let low = terrain_vertex_color(900.0, 1400.0, 40.0, Vec3::Y, 42);
        let high = terrain_vertex_color(-8400.0, -7800.0, 780.0, Vec3::Y, 42);

        assert!(high[0] > low[0]);
        assert!(high[2] > low[2]);
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
}
