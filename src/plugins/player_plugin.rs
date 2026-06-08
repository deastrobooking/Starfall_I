use bevy::camera::{PerspectiveProjection, Projection, Viewport};
use bevy::prelude::*;
use bevy::render::view::Hdr;
use bevy::transform::TransformSystems;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_rapier3d::prelude::*;

use crate::chapters::chapter_map_location;
use crate::character_blueprint::{
    BodyRecipe, CartoonAppearanceRecipe, CharacterBlueprint, CharacterPaletteRecipe,
};
use crate::characters::{
    accent_preset, attach_cartoon_character, despawn_cartoon_character_parts, hair_preset,
    hero_config, hero_config_with_overrides, outfit_preset,
};
use crate::components::armor::ArmorSet;
use crate::components::character::CartoonPart;
use crate::components::enemy::{BossEnemy, DeadEnemy, FlyingDrone};
use crate::components::inventory::Inventory;
use crate::components::player::*;
use crate::components::weapon::*;
use crate::components::world::{BoatPassenger, WorldAnchor};
use crate::damage::{apply_damage, DamageInfo, Damageable, Health};
use crate::events::*;
use crate::hero_roster::{apply_hero_runtime, hero_power_profile};
use crate::perks::PerkTree;
use crate::rendering::Camera3dBundle;
use crate::character_parts::CharacterLoadout;
use crate::resources::{
    CameraShake, ChapterProgress, CurrentChapter, DungeonCrawlState, LocalPlayerConfig,
    PlaySessionTransition, PlayerPartLoadout, PlayerSelectState, PlayerSlotConfig,
};
use crate::robot_pets::RobotPetCollection;
use crate::state::AppState;
use crate::upgrades::UpgradeLedger;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct PlayerPlugin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SharedEncounterReason {
    None,
    Boss,
    DroneWing,
}

#[derive(Resource, Debug, Clone)]
struct SharedEncounterCamera {
    active: bool,
    reason: SharedEncounterReason,
    anchor: Vec3,
    focus: Vec3,
    radius: f32,
}

impl Default for SharedEncounterCamera {
    fn default() -> Self {
        Self {
            active: false,
            reason: SharedEncounterReason::None,
            anchor: Vec3::ZERO,
            focus: Vec3::ZERO,
            radius: 24.0,
        }
    }
}

fn third_person_camera_offset() -> Vec3 {
    Vec3::new(0.0, 4.5, 11.0)
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SharedEncounterCamera>()
            .init_resource::<DungeonCrawlState>()
            .add_systems(OnEnter(AppState::Playing), (spawn_players, grab_cursor))
            .add_systems(OnEnter(AppState::MainMenu), cleanup_players_for_menu)
            .add_systems(OnExit(AppState::Playing), release_cursor)
            .add_systems(
                Update,
                (
                    dedupe_player_entities,
                    player_look,
                    camera_shake_system,
                    player_movement,
                    shared_encounter_camera_mode_system,
                    shared_encounter_party_pull_system,
                    dungeon_crawl_party_pull_system,
                    player_dodge_update,
                    player_parry_update,
                    player_state_update,
                    player_stamina_regen,
                    player_perk_health_regen,
                    player_invulnerability_update,
                    player_level_up,
                    player_died_check,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                PostUpdate,
                player_camera_follow_system
                    .after(PhysicsSet::Writeback)
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

// ── Spawn helpers ─────────────────────────────────────────────────────────────

fn player_spawn_position(
    index: u8,
    current: &CurrentChapter,
    anchor_q: &Query<(&WorldAnchor, &Transform)>,
) -> Vec3 {
    if let Some(location) = chapter_map_location(current.id) {
        if let Some((_, anchor_transform)) = anchor_q
            .iter()
            .find(|(anchor, _)| anchor.id == location.anchor_id)
        {
            return location.spawn_position(anchor_transform.translation, index);
        }
    }

    // City centre: terrain is flat at Y = 0, so spawn just above the ground
    // instead of dropping the controller through a long startup fall.
    let base = Vec3::new(10.0, 1.2, 10.0);
    match index {
        0 => base,
        1 => base + Vec3::new(3.0, 0.0, 0.0),
        2 => base + Vec3::new(0.0, 0.0, 3.0),
        _ => base + Vec3::new(3.0, 0.0, 3.0),
    }
}

/// Compute a camera viewport for this player given the number of active players
/// and the current window's physical pixel size.
///
/// Layout:
///   1 player  — full screen (no explicit viewport)
///   2 players — top half / bottom half
///   3 players — top-left, top-right, bottom full
///   4 players — four equal quadrants
fn player_viewport(index: u8, active: u8, win_w: u32, win_h: u32) -> Option<Viewport> {
    if active <= 1 {
        return None;
    }
    let h2 = win_h / 2;
    let w2 = win_w / 2;
    let (x, y, w, h) = match (index, active) {
        (0, 2) => (0, 0, win_w, h2),
        (1, 2) => (0, h2, win_w, h2),
        (0, 3) => (0, 0, w2, h2),
        (1, 3) => (w2, 0, w2, h2),
        (2, 3) => (0, h2, win_w, h2),
        (0, 4) => (0, 0, w2, h2),
        (1, 4) => (w2, 0, w2, h2),
        (2, 4) => (0, h2, w2, h2),
        (3, 4) => (w2, h2, w2, h2),
        _ => return None,
    };
    Some(Viewport {
        physical_position: UVec2::new(x, y),
        physical_size: UVec2::new(w, h),
        ..default()
    })
}

fn authored_player_defaults(
    blueprint: Option<&CharacterBlueprint>,
    visual_scale: f32,
) -> (PlayerStats, PlayerMovement, DodgeState, Collider) {
    let mut stats = PlayerStats::default();
    let mut movement = PlayerMovement::default();
    let mut dodge = DodgeState::new();
    let mut half_height = 0.6;
    let mut radius = 0.35;

    if let Some(blueprint) = blueprint {
        let body = blueprint.body.validated();
        let movement_profile = blueprint.movement_profile;
        let gameplay = blueprint.gameplay_stats;

        movement.walk_speed = movement_profile.walk_speed;
        movement.sprint_speed = movement_profile.sprint_speed;
        movement.jump_force = movement_profile.jump_force;
        movement.air_accel = movement_profile.air_control;

        dodge.dodge_speed = movement_profile.dodge_speed;
        dodge.dodge_cost = movement_profile.dodge_cost;

        stats.max_health = gameplay.max_health;
        stats.max_stamina = gameplay.max_stamina;
        stats.stamina = gameplay.max_stamina;
        stats.max_armor = gameplay.max_armor;
        stats.armor = gameplay.max_armor;

        half_height = 0.6 * (body.height * 0.72 + body.leg_length * 0.28);
        radius =
            0.35 * (body.shoulder_width * 0.55 + body.chest_size * 0.25 + body.hip_width * 0.20);
    }

    // Scale physical interaction parameters to match the visual character size.
    movement.autostep_height *= visual_scale;
    movement.autostep_min_width *= visual_scale;
    movement.ground_snap_distance *= visual_scale;
    movement.controller_offset *= visual_scale;

    (
        stats,
        movement,
        dodge,
        Collider::capsule_y(
            (half_height * visual_scale).clamp(0.44 * visual_scale, 0.86 * visual_scale),
            (radius * visual_scale).clamp(0.26 * visual_scale, 0.50 * visual_scale),
        ),
    )
}

fn upgraded_player_blueprint(name: &'static str, slot: &PlayerSlotConfig) -> CharacterBlueprint {
    let base = hero_config(name);
    let body = match name {
        "Vincenzo" => BodyRecipe {
            height: 1.14,
            shoulder_width: 1.08,
            chest_size: 1.04,
            arm_length: 1.08,
            leg_length: 1.18,
            hand_size: 1.02,
            foot_size: 1.02,
            head_size: 0.94,
            mass: 1.05,
            muscle: 1.10,
            spine_posture: 0.08,
            ..BodyRecipe::default()
        },
        "Antonio" => BodyRecipe {
            height: 1.12,
            shoulder_width: 0.98,
            chest_size: 0.96,
            arm_length: 1.07,
            leg_length: 1.22,
            hand_size: 0.98,
            foot_size: 1.00,
            head_size: 0.93,
            mass: 0.92,
            muscle: 1.00,
            spine_posture: 0.04,
            ..BodyRecipe::default()
        },
        "Angelo" => BodyRecipe {
            height: 1.10,
            shoulder_width: 1.00,
            chest_size: 1.00,
            arm_length: 1.10,
            leg_length: 1.17,
            hand_size: 1.02,
            foot_size: 1.01,
            head_size: 0.95,
            mass: 0.96,
            muscle: 1.04,
            spine_posture: -0.02,
            ..BodyRecipe::default()
        },
        "Joseph" => BodyRecipe {
            height: 1.11,
            shoulder_width: 1.16,
            chest_size: 1.10,
            arm_length: 1.06,
            leg_length: 1.14,
            hand_size: 1.05,
            foot_size: 1.05,
            head_size: 0.96,
            mass: 1.12,
            muscle: 1.18,
            spine_posture: 0.09,
            ..BodyRecipe::default()
        },
        "Gabriella" => BodyRecipe {
            height: 1.13,
            shoulder_width: 1.02,
            chest_size: 1.02,
            arm_length: 1.10,
            leg_length: 1.20,
            hand_size: 1.00,
            foot_size: 1.01,
            head_size: 0.94,
            mass: 0.98,
            muscle: 1.02,
            spine_posture: 0.05,
            ..BodyRecipe::default()
        },
        "Nova" => BodyRecipe {
            height: 1.12,
            shoulder_width: 0.96,
            chest_size: 0.96,
            arm_length: 1.08,
            leg_length: 1.23,
            hand_size: 0.98,
            foot_size: 0.99,
            head_size: 0.93,
            mass: 0.92,
            muscle: 0.98,
            spine_posture: 0.03,
            ..BodyRecipe::default()
        },
        "Aurora" => BodyRecipe {
            height: 1.12,
            shoulder_width: 1.06,
            chest_size: 1.03,
            arm_length: 1.07,
            leg_length: 1.18,
            hand_size: 1.03,
            foot_size: 1.03,
            head_size: 0.95,
            mass: 1.04,
            muscle: 1.06,
            spine_posture: 0.07,
            ..BodyRecipe::default()
        },
        "Fortuna" => BodyRecipe {
            height: 1.11,
            shoulder_width: 0.98,
            chest_size: 0.98,
            arm_length: 1.09,
            leg_length: 1.21,
            hand_size: 1.01,
            foot_size: 1.00,
            head_size: 0.94,
            mass: 0.94,
            muscle: 1.00,
            spine_posture: 0.02,
            ..BodyRecipe::default()
        },
        _ => BodyRecipe {
            height: 1.12,
            shoulder_width: 1.04,
            chest_size: 1.02,
            arm_length: 1.07,
            leg_length: 1.18,
            foot_size: 1.02,
            head_size: 0.94,
            mass: 1.00,
            muscle: 1.06,
            ..BodyRecipe::default()
        },
    };
    let palette = CharacterPaletteRecipe {
        skin: base.skin,
        outfit: slot.outfit_idx.map(outfit_preset).unwrap_or(base.outfit),
        accent: slot.accent_idx.map(accent_preset).unwrap_or_else(|| {
            if matches!(name, "Vincenzo" | "Antonio") {
                Color::srgb(0.12, 0.88, 1.0)
            } else {
                base.accent
            }
        }),
        hair: slot.hair_idx.map(hair_preset).unwrap_or(base.hair),
        eye: base.eye_color,
    };
    let appearance = CartoonAppearanceRecipe {
        has_hood: slot.has_hood.unwrap_or(true),
        has_cape: slot.has_cape.unwrap_or(true),
        has_gloves: slot.has_gloves.unwrap_or(true),
        has_boots: slot.has_boots.unwrap_or(true),
        has_shoulder_pads: slot.has_shoulder_pads.unwrap_or(matches!(
            name,
            "Vincenzo" | "Joseph" | "Gabriella" | "Aurora"
        )),
        has_visor: slot
            .has_visor
            .unwrap_or(matches!(name, "Vincenzo" | "Antonio" | "Nova")),
    };

    CharacterBlueprint::hero(name, body, palette, appearance)
}

// ── Spawn ─────────────────────────────────────────────────────────────────────
fn spawn_players(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    transition: Res<PlaySessionTransition>,
    config: Res<LocalPlayerConfig>,
    select: Res<PlayerSelectState>,
    current: Res<CurrentChapter>,
    progress: Res<ChapterProgress>,
    robot_pets: Res<RobotPetCollection>,
    part_loadout: Res<PlayerPartLoadout>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    chapter_anchor_q: Query<(&WorldAnchor, &Transform)>,
    existing_players: Query<Entity, With<Player>>,
    existing_parts: Query<(Entity, &CartoonPart)>,
) {
    if transition.resuming_from_pause || !existing_players.is_empty() {
        return;
    }

    let active = config.active.clamp(1, 4);
    let (win_w, win_h) = window_q
        .single()
        .map(|w| (w.physical_width(), w.physical_height()))
        .unwrap_or((1280, 720));

    for i in 0..active {
        let spawn_pos = player_spawn_position(i, &current, &chapter_anchor_q);
        let slot = &select.slots[i as usize];
        let character_name = select.character_name(i as usize);
        let runtime_blueprint = slot
            .blueprint
            .clone()
            .unwrap_or_else(|| upgraded_player_blueprint(character_name, slot));
        let hero_profile = hero_power_profile(character_name);
        let hero_powers = hero_profile.amplified_powers(&robot_pets);
        let character_visual_scale = hero_config(character_name).scale;
        let (mut player_stats, mut player_movement, mut dodge_state, player_collider) =
            authored_player_defaults(Some(&runtime_blueprint), character_visual_scale);
        let mut jetpack = JetpackState::default();
        let mut weapon_inventory = WeaponInventory::default();
        let mut special_inventory = SpecialWeaponInventory::default();
        let mut melee_combo = MeleeCombo::new();
        apply_hero_runtime(
            hero_profile,
            hero_powers,
            &mut player_stats,
            &mut player_movement,
            &mut jetpack,
            &mut dodge_state,
            &mut weapon_inventory,
            &mut special_inventory,
            &mut melee_combo,
        );
        apply_scientist_temple_progress(
            &progress,
            &mut player_movement,
            &mut jetpack,
            &mut weapon_inventory,
            &mut special_inventory,
            &mut melee_combo,
        );

        let player = commands
            .spawn((
                Player,
                PlayerIndex(i),
                PlayerInput::default(),
                Transform::from_translation(spawn_pos),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                RigidBody::KinematicPositionBased,
                player_collider,
                KinematicCharacterController {
                    up: Vec3::Y,
                    offset: CharacterLength::Absolute(player_movement.controller_offset),
                    slide: true,
                    autostep: Some(CharacterAutostep {
                        max_height: CharacterLength::Absolute(player_movement.autostep_height),
                        min_width: CharacterLength::Absolute(player_movement.autostep_min_width),
                        include_dynamic_bodies: false,
                    }),
                    snap_to_ground: Some(CharacterLength::Absolute(
                        player_movement.ground_snap_distance,
                    )),
                    ..default()
                },
                KinematicCharacterControllerOutput::default(),
                player_stats.clone(),
                player_movement,
            ))
            .insert((
                hero_profile,
                hero_powers,
                jetpack,
                EdgeGrabState::new(),
                dodge_state,
                ParryState::new(),
                PlayerStateMachine::default(),
                Health::new(player_stats.max_health),
                Damageable::default(),
            ))
            .insert((
                ArmorSet::default(),
                Inventory::default(),
                weapon_inventory,
                special_inventory,
                BeamSabre::default(),
                melee_combo,
            ))
            .id();

        despawn_cartoon_character_parts(&mut commands, player, &existing_parts);
        let mut character_config = hero_config_with_overrides(
            character_name,
            slot.outfit_idx.map(outfit_preset),
            slot.accent_idx.map(accent_preset),
            slot.hair_idx.map(hair_preset),
            slot.has_hood,
            slot.has_cape,
            slot.has_gloves,
            slot.has_boots,
            slot.has_shoulder_pads,
            slot.has_visor,
        );
        character_config = character_config.with_blueprint(&runtime_blueprint);
        character_config.emissive_eyes = character_config.has_visor;
        attach_cartoon_character(
            &mut commands,
            &mut meshes,
            &mut materials,
            player,
            character_config,
            spawn_pos,
        );
        // Apply chassis-editor slot choices — overrides the default humanoid loadout.
        commands.entity(player).insert(CharacterLoadout {
            body: part_loadout.body,
            arms: part_loadout.arms,
            legs: part_loadout.legs,
            shoulders: part_loadout.shoulders,
        });

        let viewport = player_viewport(i, active, win_w, win_h);

        let cam_entity = commands
            .spawn((
                Camera3dBundle {
                    transform: player_camera_transform(
                        &Transform::from_translation(spawn_pos),
                        0.0,
                        Vec3::ZERO,
                    ),
                    camera: Camera {
                        order: i as isize,
                        viewport,
                        ..default()
                    },
                    ..default()
                },
                PlayerCamera,
                CameraPitch::default(),
                Projection::Perspective(PerspectiveProjection {
                    far: 30_000.0,
                    ..default()
                }),
                Hdr,
                bevy::post_process::bloom::Bloom {
                    intensity: 0.25,
                    ..default()
                },
                DistanceFog {
                    color: Color::srgba(0.02, 0.02, 0.08, 1.0),
                    falloff: FogFalloff::ExponentialSquared { density: 0.00018 },
                    ..default()
                },
            ))
            .id();

        commands.entity(player).insert(PlayerCameraRef(cam_entity));
    }
}

pub fn apply_scientist_temple_progress(
    progress: &ChapterProgress,
    movement: &mut PlayerMovement,
    jetpack: &mut JetpackState,
    weapons: &mut WeaponInventory,
    specials: &mut SpecialWeaponInventory,
    melee: &mut MeleeCombo,
) {
    if progress.has_discoverable("ancient_flight_core") {
        apply_ancient_flight_core(movement, jetpack);
    }
    if progress.has_discoverable("solar_sabre_glyph") {
        melee.damage_multiplier = melee.damage_multiplier.max(1.18);
        weapons.slots[4].damage = weapons.slots[4].damage.max(34.0);
        weapons.slots[4].max_ammo = weapons.slots[4].max_ammo.max(260);
        weapons.slots[4].ammo = weapons.slots[4].max_ammo;
    }
    if progress.has_discoverable("nova_missile_matrix") {
        weapons.slots[3].damage = weapons.slots[3].damage.max(116.0);
        weapons.slots[3].explosion_radius = weapons.slots[3].explosion_radius.max(8.8);
        weapons.slots[3].max_ammo = weapons.slots[3].max_ammo.max(14);
        weapons.slots[3].ammo = weapons.slots[3].max_ammo;
        specials.slot7.level = specials.slot7.level.max(2);
    }
    if progress.has_discoverable("aegis_armor_frame") {
        movement.ground_snap_distance = movement.ground_snap_distance.max(0.34);
        movement.autostep_height = movement.autostep_height.max(0.52);
        movement.max_wall_jump_charges = movement.max_wall_jump_charges.max(3);
        movement.wall_jump_charges = movement
            .wall_jump_charges
            .max(movement.max_wall_jump_charges);
    }
}

pub fn apply_ancient_flight_core(movement: &mut PlayerMovement, jetpack: &mut JetpackState) {
    movement.air_accel = movement.air_accel.max(1.92);
    movement.max_fall_speed = movement.max_fall_speed.max(2.55);
    jetpack.max_fuel = jetpack.max_fuel.max(360.0);
    jetpack.fuel = jetpack.max_fuel;
    jetpack.force = jetpack.force.max(0.085);
    jetpack.regen_rate = jetpack.regen_rate.max(46.0);
    jetpack.max_vertical_vel = jetpack.max_vertical_vel.max(0.52);
}

fn cleanup_players_for_menu(
    mut commands: Commands,
    players: Query<(Entity, Option<&PlayerCameraRef>), With<Player>>,
    cameras: Query<Entity, With<PlayerCamera>>,
) {
    for (entity, camera_ref) in players.iter() {
        despawn_player_with_camera(&mut commands, entity, camera_ref);
    }
    for camera in cameras.iter() {
        commands.entity(camera).try_despawn();
    }
}

fn dedupe_player_entities(
    mut commands: Commands,
    mut seen: Local<[Option<(Entity, bool)>; 4]>,
    player_q: Query<(Entity, &PlayerIndex, Option<&PlayerCameraRef>), With<Player>>,
) {
    *seen = [None; 4];

    for (entity, index, camera_ref) in player_q.iter() {
        let slot = usize::from(index.0.min(3));
        let has_camera = camera_ref.is_some();

        match seen[slot] {
            None => {
                seen[slot] = Some((entity, has_camera));
            }
            Some((kept, false)) if has_camera => {
                warn!(
                    "Removing duplicate P{} entity {:?}; keeping camera-backed entity {:?}",
                    slot + 1,
                    kept,
                    entity
                );
                commands.entity(kept).try_despawn();
                seen[slot] = Some((entity, true));
            }
            Some((kept, _)) => {
                warn!(
                    "Removing duplicate P{} entity {:?}; {:?} is already active",
                    slot + 1,
                    entity,
                    kept
                );
                if let Some(camera_ref) = camera_ref {
                    commands.entity(camera_ref.0).try_despawn();
                }
                commands.entity(entity).try_despawn();
            }
        }
    }
}

fn despawn_player_with_camera(
    commands: &mut Commands,
    player: Entity,
    camera_ref: Option<&PlayerCameraRef>,
) {
    if let Some(camera_ref) = camera_ref {
        commands.entity(camera_ref.0).try_despawn();
    }
    commands.entity(player).try_despawn();
}

fn grab_cursor(mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut cursor) = cursors.single_mut() {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn release_cursor(mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut cursor) = cursors.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

// ── Mouse Look ────────────────────────────────────────────────────────────────
fn player_look(
    mut player_q: Query<
        (&PlayerInput, &mut Transform, &PlayerCameraRef),
        (With<Player>, Without<PlayerCamera>),
    >,
    mut cam_q: Query<(&mut Transform, &mut CameraPitch), With<PlayerCamera>>,
) {
    for (pi, mut pt, cam_ref) in player_q.iter_mut() {
        let delta = pi.look_delta;
        if delta == Vec2::ZERO {
            continue;
        }
        pt.rotate_y(-delta.x);
        if let Ok((mut ct, mut pitch)) = cam_q.get_mut(cam_ref.0) {
            pitch.0 = (pitch.0 - delta.y).clamp(
                -std::f32::consts::FRAC_PI_2 * 0.9,
                std::f32::consts::FRAC_PI_2 * 0.9,
            );
            ct.rotation = Quat::from_rotation_x(pitch.0);
        }
    }
}

// ── Camera Shake ──────────────────────────────────────────────────────────────
fn camera_shake_system(
    time: Res<Time>,
    mut shake: ResMut<CameraShake>,
    mut damage_ev: MessageReader<PlayerDamagedEvent>,
) {
    for ev in damage_ev.read() {
        let trauma = (ev.amount / 25.0).clamp(0.12, 0.65);
        if let Some(player_index) = ev.player_index {
            shake.add_player_trauma(player_index, trauma);
        } else {
            shake.add_trauma(trauma);
        }
    }

    shake.decay(time.delta_secs() * 2.0);
}

fn player_camera_follow_system(
    mut commands: Commands,
    shake: Res<CameraShake>,
    dungeon: Res<DungeonCrawlState>,
    shared_camera: Res<SharedEncounterCamera>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    player_q: Query<
        (&PlayerIndex, &Transform, &PlayerCameraRef),
        (With<Player>, Without<PlayerCamera>),
    >,
    mut cam_q: Query<
        (Entity, &mut Transform, &CameraPitch, &mut Camera),
        (With<PlayerCamera>, Without<Player>),
    >,
) {
    let mut referenced = Vec::new();
    let active_players = player_q.iter().count().max(1) as u8;
    let (win_w, win_h) = window_q
        .single()
        .map(|w| (w.physical_width(), w.physical_height()))
        .unwrap_or((1280, 720));

    if dungeon.active {
        let lead_camera = player_q
            .iter()
            .min_by_key(|(index, _, _)| index.0)
            .map(|(_, _, camera_ref)| camera_ref.0);
        let max_trauma = player_q
            .iter()
            .map(|(index, _, _)| shake.trauma_for(index.0))
            .fold(0.0_f32, f32::max);
        let shake_offset = camera_shake_offset(max_trauma);
        let party_focus = average_positions(
            &player_q
                .iter()
                .map(|(_, transform, _)| transform.translation)
                .collect::<Vec<_>>(),
        )
        .unwrap_or(dungeon.focus);
        let focus = clamp_to_dungeon_focus(party_focus, dungeon.focus, dungeon.radius * 0.62);

        for (_, _, camera_ref) in player_q.iter() {
            referenced.push(camera_ref.0);
            if let Ok((_, mut camera_transform, _, mut camera)) = cam_q.get_mut(camera_ref.0) {
                camera.is_active = Some(camera_ref.0) == lead_camera;
                camera.viewport = None;
                if camera.is_active {
                    *camera_transform =
                        dungeon_crawl_camera_transform(focus, dungeon.radius, shake_offset);
                }
            }
        }

        for (camera, mut camera_transform, _, mut camera_component) in cam_q.iter_mut() {
            if !referenced.contains(&camera) {
                camera_component.is_active = false;
                camera_transform.translation = Vec3::new(0.0, -10_000.0, 0.0);
                commands.entity(camera).try_despawn();
            }
        }
        return;
    }

    if shared_camera.active && active_players > 1 {
        let lead_camera = player_q
            .iter()
            .min_by_key(|(index, _, _)| index.0)
            .map(|(_, _, camera_ref)| camera_ref.0);
        let max_trauma = player_q
            .iter()
            .map(|(index, _, _)| shake.trauma_for(index.0))
            .fold(0.0_f32, f32::max);
        let shake_offset = camera_shake_offset(max_trauma);

        for (_, _, camera_ref) in player_q.iter() {
            referenced.push(camera_ref.0);
            if let Ok((_, mut camera_transform, _, mut camera)) = cam_q.get_mut(camera_ref.0) {
                camera.is_active = Some(camera_ref.0) == lead_camera;
                camera.viewport = None;
                if camera.is_active {
                    *camera_transform = shared_boss_camera_transform(
                        shared_camera.focus,
                        shared_camera.radius,
                        shake_offset,
                    );
                }
            }
        }

        for (camera, mut camera_transform, _, mut camera_component) in cam_q.iter_mut() {
            if !referenced.contains(&camera) {
                camera_component.is_active = false;
                camera_transform.translation = Vec3::new(0.0, -10_000.0, 0.0);
                commands.entity(camera).try_despawn();
            }
        }
        return;
    }

    for (index, player_transform, camera_ref) in player_q.iter() {
        referenced.push(camera_ref.0);
        let shake_offset = camera_shake_offset(shake.trauma_for(index.0));
        if let Ok((_, mut camera_transform, pitch, mut camera)) = cam_q.get_mut(camera_ref.0) {
            camera.is_active = true;
            camera.viewport = player_viewport(index.0, active_players, win_w, win_h);
            *camera_transform = player_camera_transform(player_transform, pitch.0, shake_offset);
        }
    }

    for (camera, mut camera_transform, _, mut camera_component) in cam_q.iter_mut() {
        if !referenced.contains(&camera) {
            camera_component.is_active = false;
            camera_transform.translation = Vec3::new(0.0, -10_000.0, 0.0);
            commands.entity(camera).try_despawn();
        }
    }
}

fn player_camera_transform(
    player_transform: &Transform,
    pitch: f32,
    shake_offset: Vec3,
) -> Transform {
    let local_offset = third_person_camera_offset() + shake_offset;
    Transform {
        translation: player_transform.translation + player_transform.rotation * local_offset,
        rotation: player_transform.rotation * Quat::from_rotation_x(pitch),
        scale: Vec3::ONE,
    }
}

fn camera_shake_offset(trauma: f32) -> Vec3 {
    if trauma > 0.01 {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mag = trauma * trauma * 0.18;
        Vec3::new(
            rng.gen_range(-1.0f32..1.0) * mag,
            rng.gen_range(-0.5f32..0.5) * mag,
            rng.gen_range(-1.0f32..1.0) * mag,
        )
    } else {
        Vec3::ZERO
    }
}

fn shared_boss_camera_transform(focus: Vec3, radius: f32, shake_offset: Vec3) -> Transform {
    let distance = (radius * 1.25).clamp(34.0, 96.0);
    let height = (radius * 0.72 + 14.0).clamp(24.0, 72.0);
    let translation = focus + Vec3::new(0.0, height, distance) + shake_offset;
    Transform::from_translation(translation).looking_at(focus + Vec3::Y * 2.2, Vec3::Y)
}

fn dungeon_crawl_camera_transform(focus: Vec3, radius: f32, shake_offset: Vec3) -> Transform {
    let height = (radius * 1.12).clamp(46.0, 92.0);
    let z_offset = (radius * 0.22).clamp(10.0, 22.0);
    let translation = focus + Vec3::new(0.0, height, z_offset) + shake_offset;
    Transform::from_translation(translation).looking_at(focus + Vec3::Y * 1.0, Vec3::Y)
}

fn clamp_to_dungeon_focus(position: Vec3, center: Vec3, radius: f32) -> Vec3 {
    let offset = (position - center).with_y(0.0);
    if offset.length() <= radius {
        position
    } else {
        center + offset.normalize_or_zero() * radius + Vec3::Y * (position.y - center.y)
    }
}

fn shared_encounter_camera_mode_system(
    mut mode: ResMut<SharedEncounterCamera>,
    player_q: Query<&Transform, With<Player>>,
    boss_q: Query<&Transform, (With<BossEnemy>, Without<DeadEnemy>)>,
    drone_q: Query<&Transform, (With<FlyingDrone>, Without<BossEnemy>, Without<DeadEnemy>)>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let players: Vec<Vec3> = player_q
        .iter()
        .map(|transform| transform.translation)
        .collect();
    if players.len() <= 1 {
        if mode.active {
            mode.active = false;
            mode.reason = SharedEncounterReason::None;
        }
        return;
    }

    let boss_positions: Vec<Vec3> = boss_q
        .iter()
        .map(|transform| transform.translation)
        .collect();
    let drone_positions = nearby_drone_threats(&players, &drone_q);

    let (reason, threats): (SharedEncounterReason, &[Vec3]) = if !boss_positions.is_empty() {
        (SharedEncounterReason::Boss, &boss_positions)
    } else if drone_positions.len() >= 3 {
        (SharedEncounterReason::DroneWing, &drone_positions)
    } else {
        (SharedEncounterReason::None, &[])
    };

    if reason == SharedEncounterReason::None {
        if mode.active {
            mode.active = false;
            mode.reason = SharedEncounterReason::None;
            msg_ev.write(UiMessageEvent {
                text: "Party camera released.".to_string(),
                duration: 1.6,
            });
        }
        return;
    }

    let was_active = mode.active;
    let previous_reason = mode.reason;
    let (anchor, focus, radius) = shared_encounter_frame(&players, threats);
    mode.active = true;
    mode.reason = reason;
    mode.anchor = anchor;
    mode.focus = focus;
    mode.radius = radius;

    if !was_active || previous_reason != reason {
        let text = match reason {
            SharedEncounterReason::Boss => "BOSS MODE - party camera linked.",
            SharedEncounterReason::DroneWing => "Aerial threat - party camera linked.",
            SharedEncounterReason::None => unreachable!(),
        };
        msg_ev.write(UiMessageEvent {
            text: text.to_string(),
            duration: 2.2,
        });
    }
}

fn nearby_drone_threats(
    players: &[Vec3],
    drone_q: &Query<&Transform, (With<FlyingDrone>, Without<BossEnemy>, Without<DeadEnemy>)>,
) -> Vec<Vec3> {
    drone_q
        .iter()
        .map(|transform| transform.translation)
        .filter(|drone| players.iter().any(|player| player.distance(*drone) <= 78.0))
        .collect()
}

fn shared_encounter_frame(players: &[Vec3], threats: &[Vec3]) -> (Vec3, Vec3, f32) {
    let anchor = average_positions(threats).unwrap_or_else(|| average_positions(players).unwrap());
    let mut weighted = Vec::with_capacity(players.len() + threats.len() * 2);
    weighted.extend_from_slice(players);
    for threat in threats {
        weighted.push(*threat);
        weighted.push(*threat);
    }
    let focus = average_positions(&weighted).unwrap_or(anchor);
    let radius = players
        .iter()
        .chain(threats.iter())
        .map(|pos| pos.distance(focus))
        .fold(18.0_f32, f32::max)
        + 14.0;
    (anchor, focus, radius.clamp(28.0, 92.0))
}

fn average_positions(positions: &[Vec3]) -> Option<Vec3> {
    if positions.is_empty() {
        return None;
    }
    Some(positions.iter().copied().sum::<Vec3>() / positions.len() as f32)
}

fn shared_encounter_party_pull_system(
    time: Res<Time>,
    mode: Res<SharedEncounterCamera>,
    mut player_q: Query<(&PlayerIndex, &mut Transform, Option<&BoatPassenger>), With<Player>>,
) {
    if !mode.active {
        return;
    }

    let dt = time.delta_secs();
    let soft_radius = 54.0;
    let hard_radius = 108.0;
    for (index, mut transform, boat_passenger) in player_q.iter_mut() {
        if boat_passenger.is_some() {
            continue;
        }

        let to_anchor = (mode.anchor - transform.translation).with_y(0.0);
        let distance = to_anchor.length();
        if distance <= soft_radius {
            continue;
        }

        let offset = boss_mode_player_slot_offset(index.0);
        if distance >= hard_radius {
            transform.translation = Vec3::new(
                mode.anchor.x + offset.x,
                mode.anchor.y.max(transform.translation.y) + 1.2,
                mode.anchor.z + offset.z,
            );
            continue;
        }

        let direction = to_anchor.normalize_or_zero();
        let pull = ((distance - soft_radius) * 0.55).min(28.0) * dt;
        transform.translation += direction * pull;
    }
}

fn dungeon_crawl_party_pull_system(
    time: Res<Time>,
    dungeon: Res<DungeonCrawlState>,
    mut player_q: Query<(&PlayerIndex, &mut Transform, Option<&BoatPassenger>), With<Player>>,
) {
    if !dungeon.active {
        return;
    }

    let dt = time.delta_secs();
    let soft_radius = dungeon.radius * 0.52;
    let hard_radius = dungeon.radius * 0.92;
    for (index, mut transform, boat_passenger) in player_q.iter_mut() {
        if boat_passenger.is_some() {
            continue;
        }

        let to_focus = (dungeon.focus - transform.translation).with_y(0.0);
        let distance = to_focus.length();
        if distance <= soft_radius {
            continue;
        }

        let offset = boss_mode_player_slot_offset(index.0);
        if distance >= hard_radius {
            transform.translation = Vec3::new(
                dungeon.focus.x + offset.x,
                dungeon.focus.y.max(transform.translation.y) + 1.2,
                dungeon.focus.z + offset.z,
            );
            continue;
        }

        let direction = to_focus.normalize_or_zero();
        let pull = ((distance - soft_radius) * 0.70).min(34.0) * dt;
        transform.translation += direction * pull;
    }
}

fn boss_mode_player_slot_offset(index: u8) -> Vec3 {
    let angle = index as f32 * std::f32::consts::TAU / 4.0 + std::f32::consts::FRAC_PI_4;
    Vec3::new(angle.cos() * 8.0, 0.0, angle.sin() * 8.0)
}

// ── Movement & Physics ────────────────────────────────────────────────────────
fn player_movement(
    time: Res<Time>,
    dungeon: Res<DungeonCrawlState>,
    mut player_q: Query<
        (
            &mut KinematicCharacterController,
            &KinematicCharacterControllerOutput,
            &mut PlayerMovement,
            &mut PlayerStats,
            &mut JetpackState,
            &mut EdgeGrabState,
            &mut DodgeState,
            &Transform,
            &mut PlayerStateMachine,
            &PlayerInput,
            Option<&BoatPassenger>,
        ),
        With<Player>,
    >,
) {
    let dt = time.delta_secs();
    for (
        mut controller,
        output,
        mut movement,
        mut stats,
        mut jetpack,
        mut edge_grab,
        dodge,
        transform,
        mut state,
        pi,
        boat_passenger,
    ) in player_q.iter_mut()
    {
        if boat_passenger.is_some() {
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity = Vec3::ZERO;
            movement.is_grounded = true;
            movement.coyote_timer = movement.coyote_time;
            movement.jump_buffer_timer = 0.0;
            movement.wall_jump_lock_timer = 0.0;
            movement.wall_jump_charges = movement.max_wall_jump_charges;
            jetpack.is_active = false;
            edge_grab.is_hanging = false;
            controller.translation = Some(Vec3::ZERO);
            state.force(PlayerState::Idle);
            continue;
        }

        if pi.jump {
            movement.jump_buffer_timer = movement.jump_buffer_time;
        } else {
            movement.jump_buffer_timer = (movement.jump_buffer_timer - dt).max(0.0);
        }
        movement.wall_jump_lock_timer = (movement.wall_jump_lock_timer - dt).max(0.0);

        movement.is_grounded = output.grounded;
        edge_grab.cooldown_timer = (edge_grab.cooldown_timer - dt).max(0.0);
        edge_grab.wall_contact_timer = (edge_grab.wall_contact_timer - dt).max(0.0);

        if movement.is_grounded {
            movement.coyote_timer = movement.coyote_time;
            movement.wall_jump_charges = movement.max_wall_jump_charges;
            movement.wall_jump_lock_timer = 0.0;
            jetpack.fuel = (jetpack.fuel + jetpack.regen_rate * dt).min(jetpack.max_fuel);
            movement.velocity.y = movement.velocity.y.max(0.0);
            edge_grab.is_hanging = false;
            edge_grab.hang_timer = 0.0;
            edge_grab.wall_clasp_timer = 0.0;
            edge_grab.wall_contact_timer = 0.0;
        } else {
            movement.coyote_timer = (movement.coyote_timer - dt).max(0.0);
        }

        let (fwd, right) = if dungeon.active {
            (Vec3::NEG_Z, Vec3::X)
        } else {
            (
                transform
                    .forward()
                    .as_vec3()
                    .with_y(0.0)
                    .normalize_or_zero(),
                transform.right().as_vec3().with_y(0.0).normalize_or_zero(),
            )
        };
        let (input, input_strength) = movement_input_from_axes(fwd, right, pi.move_axis);

        let sprinting =
            pi.sprint && stats.stamina > 0.0 && input_strength >= movement.analog_sprint_threshold;
        let speed = if sprinting {
            movement.sprint_speed
        } else {
            movement.walk_speed
        };

        if sprinting {
            stats.stamina = (stats.stamina - 15.0 * dt).max(0.0);
        }

        if let Some(normal) = wall_normal_from_controller_output(output) {
            edge_grab.wall_normal = normal;
            edge_grab.wall_contact_timer = 0.16;
        }

        let has_wall_contact =
            edge_grab.wall_contact_timer > 0.0 && edge_grab.wall_normal.length_squared() > 0.25;
        let pushing_into_wall = has_wall_contact && input.dot(-edge_grab.wall_normal) > 0.15;
        let wall_sliding = !movement.is_grounded
            && !edge_grab.is_hanging
            && !dodge.is_dodging
            && pushing_into_wall
            && movement.velocity.y <= 0.03;
        if wall_sliding {
            movement.wall_jump_charges = movement.max_wall_jump_charges;
            edge_grab.wall_clasp_timer += dt;
            stats.stamina =
                (stats.stamina - edge_grab.wall_slide_stamina_drain_per_sec * dt).max(0.0);
        } else if !edge_grab.is_hanging {
            edge_grab.wall_clasp_timer = 0.0;
        }

        let mut started_jump = false;

        if edge_grab.is_hanging {
            edge_grab.hang_timer += dt;
            stats.stamina = (stats.stamina - edge_grab.stamina_drain_per_sec * dt).max(0.0);
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity = Vec3::ZERO;
            jetpack.is_active = false;

            if movement.jump_buffer_timer > 0.0 {
                let jump_dir = (edge_grab.wall_normal + input * 0.25)
                    .with_y(0.0)
                    .normalize_or_zero();
                movement.velocity.y = edge_grab.wall_jump_vertical;
                movement.ground_velocity = jump_dir * edge_grab.wall_jump_push;
                movement.wall_jump_charges = movement.wall_jump_charges.saturating_sub(1);
                movement.wall_jump_lock_timer = movement.wall_jump_lock_time;
                movement.jump_buffer_timer = 0.0;
                movement.coyote_timer = 0.0;
                edge_grab.is_hanging = false;
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
                started_jump = true;
                state.force(PlayerState::Jetpack);
            } else if pi.interact {
                controller.translation = Some(
                    Vec3::Y * edge_grab.climb_boost * dt * 60.0 + edge_grab.wall_normal * 0.25,
                );
                movement.is_grounded = false;
                edge_grab.is_hanging = false;
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
                state.force(PlayerState::Moving);
                continue;
            } else if pi.dodge
                || pi.move_axis.y < -0.35
                || stats.stamina <= 0.0
                || edge_grab.hang_timer >= edge_grab.max_hang_time
            {
                edge_grab.is_hanging = false;
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
                movement.velocity.y = -0.12;
                state.transition(PlayerState::Jetpack);
            } else {
                controller.translation = Some(Vec3::ZERO);
                state.force(PlayerState::Hanging);
                continue;
            }
        }

        if movement.jump_buffer_timer > 0.0
            && !movement.is_grounded
            && has_wall_contact
            && edge_grab.cooldown_timer <= 0.0
            && movement.wall_jump_charges > 0
        {
            let jump_dir = (edge_grab.wall_normal + input * 0.25)
                .with_y(0.0)
                .normalize_or_zero();
            movement.velocity.y = edge_grab.wall_jump_vertical;
            movement.ground_velocity = jump_dir * edge_grab.wall_jump_push;
            movement.wall_jump_charges = movement.wall_jump_charges.saturating_sub(1);
            movement.wall_jump_lock_timer = movement.wall_jump_lock_time;
            movement.jump_buffer_timer = 0.0;
            movement.coyote_timer = 0.0;
            edge_grab.cooldown_timer = edge_grab.grab_cooldown;
            started_jump = true;
            state.force(PlayerState::Jetpack);
        } else if movement.jump_buffer_timer > 0.0 && movement.coyote_timer > 0.0 {
            movement.velocity.y = movement.jump_force;
            movement.jump_buffer_timer = 0.0;
            movement.coyote_timer = 0.0;
            movement.wall_jump_lock_timer = 0.0;
            movement.is_grounded = false;
            started_jump = true;
            state.transition(PlayerState::Jetpack);
        }

        let can_grab_edge = !movement.is_grounded
            && !dodge.is_dodging
            && edge_grab.cooldown_timer <= 0.0
            && movement.velocity.y <= -0.02
            && pushing_into_wall
            && pi.interact
            && stats.stamina > 5.0;

        if can_grab_edge {
            edge_grab.is_hanging = true;
            edge_grab.hang_timer = 0.0;
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity = Vec3::ZERO;
            controller.translation = Some(Vec3::ZERO);
            state.force(PlayerState::Hanging);
            continue;
        }

        if pi.jetpack && !started_jump && !movement.is_grounded && jetpack.fuel > 0.0 {
            movement.velocity.y =
                (movement.velocity.y + jetpack.force).min(jetpack.max_vertical_vel);
            jetpack.fuel -= jetpack.fuel_cost_per_sec * dt;
            jetpack.fuel = jetpack.fuel.max(0.0);
            jetpack.is_active = true;
            state.transition(PlayerState::Jetpack);
        } else {
            jetpack.is_active = false;
        }

        if !movement.is_grounded {
            if !pi.jetpack && movement.velocity.y > movement.jump_release_cutoff {
                movement.velocity.y = movement.jump_release_cutoff;
            }

            let mut gravity = if movement.velocity.y < 0.0 {
                movement.gravity * movement.fall_gravity_mult
            } else {
                movement.gravity
            };
            if movement.velocity.y.abs() < 0.08 {
                gravity *= movement.apex_gravity_mult;
            }
            movement.velocity.y -= gravity;
            movement.velocity.y = movement.velocity.y.max(-movement.max_fall_speed);
            let wall_slide_speed = if stats.stamina > 0.0 {
                movement.wall_slide_speed
            } else {
                movement.wall_slide_speed * edge_grab.exhausted_wall_slide_mult
            };
            if wall_sliding && movement.velocity.y < -wall_slide_speed {
                movement.velocity.y = -wall_slide_speed;
                state.transition(PlayerState::WallSliding);
            }
        }

        let target_h_vel = input * speed * input_strength;
        let mut h_vel = if movement.is_grounded {
            let accel = if input.length_squared() > 0.01 {
                movement.ground_accel
            } else {
                movement.ground_decel
            };
            movement.ground_velocity =
                approach_vec3(movement.ground_velocity, target_h_vel, accel * dt);
            movement.ground_velocity
        } else {
            let has_air_input = input_strength > 0.05;
            let air_accel = if !has_air_input {
                movement.air_decel
            } else if edge_grab.cooldown_timer > 0.0 {
                movement.air_accel * 0.35
            } else if movement.wall_jump_lock_timer > 0.0 {
                movement.air_accel * 0.22
            } else {
                movement.air_accel
            };
            movement.ground_velocity =
                approach_vec3(movement.ground_velocity, target_h_vel, air_accel * dt);
            movement.ground_velocity
        };

        if dodge.is_dodging {
            h_vel = dodge.dodge_direction * dodge.dodge_speed;
        }

        let translation = (h_vel + Vec3::new(0.0, movement.velocity.y, 0.0)) * dt * 60.0;
        controller.translation = Some(translation);

        if movement.is_grounded && !dodge.is_dodging {
            if input_strength > 0.05 {
                if sprinting {
                    state.transition(PlayerState::Sprinting);
                } else {
                    state.transition(PlayerState::Moving);
                }
            } else {
                state.transition(PlayerState::Idle);
            }
        }
    }
}

fn wall_normal_from_controller_output(output: &KinematicCharacterControllerOutput) -> Option<Vec3> {
    let desired = output.desired_translation.with_y(0.0).normalize_or_zero();
    let mut best: Option<(Vec3, f32)> = None;

    for collision in &output.collisions {
        let Some(details) = collision.hit.details else {
            continue;
        };
        let flat = details.normal2.with_y(0.0);
        let strength = flat.length_squared();
        if strength < 0.20 || details.normal2.y.abs() > 0.45 {
            continue;
        }

        let mut normal = flat.normalize();
        if desired.length_squared() > 0.0 && normal.dot(desired) > 0.0 {
            normal = -normal;
        }

        if best.is_none_or(|(_, s)| s < strength) {
            best = Some((normal, strength));
        }
    }

    best.map(|(normal, _)| normal)
}

fn approach_vec3(current: Vec3, target: Vec3, max_delta: f32) -> Vec3 {
    let delta = target - current;
    let dist = delta.length();
    if dist <= max_delta || dist <= f32::EPSILON {
        target
    } else {
        current + delta / dist * max_delta
    }
}

// ── Dodge Update ──────────────────────────────────────────────────────────────
fn player_dodge_update(
    time: Res<Time>,
    perks: Res<PerkTree>,
    mut player_q: Query<
        (
            &mut DodgeState,
            &mut PlayerStats,
            &mut Damageable,
            &Transform,
            &mut PlayerStateMachine,
            &PlayerInput,
        ),
        With<Player>,
    >,
    mut dodge_ev: MessageWriter<PlayerDodgeEvent>,
) {
    let dt = time.delta_secs();
    let dodge_cost_mult = perks.dodge_cost_mult();
    for (mut dodge, mut stats, mut damageable, transform, mut state, pi) in player_q.iter_mut() {
        dodge.cooldown_timer = (dodge.cooldown_timer - dt).max(0.0);
        let dodge_cost = dodge.dodge_cost * dodge_cost_mult;

        if dodge.is_dodging {
            dodge.dodge_timer -= dt;
            damageable.is_invulnerable = true;
            if dodge.dodge_timer <= 0.0 {
                dodge.is_dodging = false;
                damageable.is_invulnerable = false;
                state.transition(PlayerState::Idle);
            }
        }

        if pi.dodge
            && !dodge.is_dodging
            && dodge.cooldown_timer <= 0.0
            && stats.stamina >= dodge_cost
            && state.current != PlayerState::Dead
        {
            let fwd = transform
                .forward()
                .as_vec3()
                .with_y(0.0)
                .normalize_or_zero();
            let right = transform.right().as_vec3().with_y(0.0).normalize_or_zero();
            // Dodge in the direction the player is moving, or backward if idle.
            let (input, _) = movement_input_from_axes(fwd, right, pi.move_axis);
            dodge.dodge_direction = if input.length_squared() > 0.01 {
                input
            } else {
                -fwd
            };
            dodge.is_dodging = true;
            dodge.dodge_timer = dodge.dodge_duration;
            dodge.cooldown_timer = dodge.dodge_cooldown;
            stats.stamina -= dodge_cost;
            state.force(PlayerState::Dodging);
            dodge_ev.write(PlayerDodgeEvent);
        }
    }
}

fn movement_input_from_axes(forward: Vec3, right: Vec3, axes: Vec2) -> (Vec3, f32) {
    let raw = forward * axes.y + right * axes.x;
    (raw.normalize_or_zero(), raw.length().clamp(0.0, 1.0))
}

// ── Parry Update ──────────────────────────────────────────────────────────────
fn player_parry_update(
    time: Res<Time>,
    perks: Res<PerkTree>,
    mut player_q: Query<(&mut ParryState, &mut PlayerStateMachine, &PlayerInput), With<Player>>,
) {
    let dt = time.delta_secs();
    let parry_window_bonus = perks.parry_window_bonus();
    for (mut parry, state, pi) in player_q.iter_mut() {
        parry.cooldown_timer = (parry.cooldown_timer - dt).max(0.0);

        if parry.is_parrying {
            parry.parry_timer -= dt;
            if parry.parry_timer <= 0.0 {
                parry.is_parrying = false;
            }
        }

        if pi.parry
            && !parry.is_parrying
            && parry.cooldown_timer <= 0.0
            && state.current != PlayerState::Dead
        {
            parry.is_parrying = true;
            parry.parry_timer = parry.parry_window + parry_window_bonus;
            parry.cooldown_timer = parry.parry_cooldown;
        }
    }
}

// ── State Update ──────────────────────────────────────────────────────────────
fn player_state_update(time: Res<Time>, mut q: Query<&mut PlayerStateMachine, With<Player>>) {
    let dt = time.delta_secs();
    for mut sm in q.iter_mut() {
        sm.timer += dt;
    }
}

// ── Stamina Regen ─────────────────────────────────────────────────────────────
fn player_stamina_regen(
    time: Res<Time>,
    mut q: Query<
        (
            &mut PlayerStats,
            &DodgeState,
            &PlayerStateMachine,
            &EdgeGrabState,
        ),
        With<Player>,
    >,
    mut ev: MessageWriter<PlayerStaminaChangedEvent>,
) {
    let dt = time.delta_secs();
    for (mut stats, dodge, state, edge_grab) in q.iter_mut() {
        let traversal_hold = edge_grab.is_hanging
            || matches!(
                state.current,
                PlayerState::Hanging | PlayerState::WallSliding
            );
        if !dodge.is_dodging && !traversal_hold && stats.stamina < stats.max_stamina {
            stats.stamina = (stats.stamina + 10.0 * dt).min(stats.max_stamina);
            ev.write(PlayerStaminaChangedEvent {
                stamina: stats.stamina,
            });
        }
    }
}

fn player_perk_health_regen(
    time: Res<Time>,
    perks: Res<PerkTree>,
    mut upgrades: ResMut<UpgradeLedger>,
    mut q: Query<(&mut Health, &PlayerStateMachine), With<Player>>,
) {
    let regen = perks.regen_per_sec() + upgrades.rejuvenation_regen_per_sec();
    if regen <= 0.0 {
        return;
    }
    let dt = time.delta_secs();
    for (mut health, state) in q.iter_mut() {
        if state.current != PlayerState::Dead && health.is_alive() && health.current < health.max {
            let missing = health.max - health.current;
            let requested = (regen * dt).min(missing);
            let healed = upgrades.consume_rejuvenation_for_heal(requested);
            if healed > 0.0 {
                health.current = (health.current + healed).min(health.max);
            }
        }
    }
}

// ── Invulnerability Update ────────────────────────────────────────────────────
fn player_invulnerability_update(time: Res<Time>, mut q: Query<&mut Damageable, With<Player>>) {
    let dt = time.delta_secs();
    for mut dmg in q.iter_mut() {
        if dmg.invulnerability_timer > 0.0 {
            dmg.invulnerability_timer -= dt;
            if dmg.invulnerability_timer <= 0.0 {
                dmg.is_invulnerable = false;
                dmg.invulnerability_timer = 0.0;
            }
        }
    }
}

// ── Level Up ──────────────────────────────────────────────────────────────────
fn player_level_up(
    mut q: Query<(&mut PlayerStats, &mut Health), With<Player>>,
    mut perks: ResMut<PerkTree>,
    mut level_ev: MessageWriter<PlayerLevelUpEvent>,
) {
    for (mut stats, mut health) in q.iter_mut() {
        let xp_needed = stats.xp_for_next_level();
        if stats.experience >= xp_needed {
            stats.experience -= xp_needed;
            stats.level += 1;
            stats.max_health += 10.0;
            stats.max_stamina += 5.0;
            health.max = stats.max_health;
            health.current = health.max;
            stats.stamina = stats.max_stamina;
            perks.award(1);
            level_ev.write(PlayerLevelUpEvent { level: stats.level });
        }
    }
}

// ── Death Check ───────────────────────────────────────────────────────────────
// Game over when ALL players are dead.
fn player_died_check(
    mut q: Query<(&Health, &mut PlayerStateMachine), With<Player>>,
    mut died_ev: MessageWriter<PlayerDiedEvent>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let mut any_alive = false;
    let mut any_newly_dead = false;

    for (health, mut sm) in q.iter_mut() {
        if health.is_alive() {
            any_alive = true;
        } else if sm.current != PlayerState::Dead {
            sm.force(PlayerState::Dead);
            any_newly_dead = true;
        }
    }

    if any_newly_dead && !any_alive {
        died_ev.write(PlayerDiedEvent);
        next_state.set(AppState::GameOver);
    }
}

// ── Public helpers (called from other plugins) ────────────────────────────────

/// Apply damage to a player, respecting parry and armor.
pub fn damage_player(
    player_index: Option<u8>,
    health: &mut Health,
    damageable: &mut Damageable,
    stats: &mut PlayerStats,
    parry: &mut ParryState,
    armor_set: &ArmorSet,
    info: &DamageInfo,
    damaged_ev: &mut MessageWriter<PlayerDamagedEvent>,
    parry_ev: &mut MessageWriter<PlayerParryEvent>,
) {
    if !health.is_alive() || damageable.is_invulnerable {
        return;
    }

    if parry.is_parrying {
        parry.is_parrying = false;
        parry_ev.write(PlayerParryEvent { success: true });
        return;
    }

    let armor_reduced = armor_set.calculate_damage_reduction(info.amount);
    let armor_absorb = armor_reduced * 0.7;
    let health_portion = armor_reduced * 0.3;

    stats.armor = (stats.armor - armor_absorb).max(0.0);

    let result = apply_damage(
        health,
        damageable,
        &DamageInfo {
            amount: health_portion,
            ..info.clone()
        },
    );

    damageable.is_invulnerable = true;
    damageable.invulnerability_timer = 0.2;

    damaged_ev.write(PlayerDamagedEvent {
        player_index,
        amount: result.damage_amount,
        remaining: health.current,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_input_preserves_analog_strength() {
        let (direction, strength) =
            movement_input_from_axes(Vec3::NEG_Z, Vec3::X, Vec2::new(0.3, 0.4));

        assert!((strength - 0.5).abs() < 0.001);
        assert!((direction.length() - 1.0).abs() < 0.001);
        assert!(direction.x > 0.0);
        assert!(direction.z < 0.0);
    }

    #[test]
    fn movement_input_clamps_full_diagonal_strength() {
        let (_, strength) = movement_input_from_axes(Vec3::NEG_Z, Vec3::X, Vec2::new(1.0, 1.0));

        assert_eq!(strength, 1.0);
    }

    #[test]
    fn shared_encounter_frame_weights_threat_anchor() {
        let players = [Vec3::new(-20.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)];
        let threats = [Vec3::new(60.0, 0.0, 0.0)];

        let (anchor, focus, radius) = shared_encounter_frame(&players, &threats);

        assert_eq!(anchor, threats[0]);
        assert!(focus.x > 10.0);
        assert!(radius >= 28.0);
    }

    #[test]
    fn boss_mode_slot_offsets_are_distinct() {
        let a = boss_mode_player_slot_offset(0);
        let b = boss_mode_player_slot_offset(1);

        assert!(a.distance(b) > 8.0);
        assert_eq!(a.y, 0.0);
    }

    #[test]
    fn ancient_flight_core_progress_improves_jetpack() {
        let mut progress = ChapterProgress::default();
        progress.unlock("ancient_flight_core");
        let mut movement = PlayerMovement::default();
        let mut jetpack = JetpackState::default();
        let mut weapons = WeaponInventory::default();
        let mut specials = SpecialWeaponInventory::default();
        let mut melee = MeleeCombo::new();

        apply_scientist_temple_progress(
            &progress,
            &mut movement,
            &mut jetpack,
            &mut weapons,
            &mut specials,
            &mut melee,
        );

        assert!(movement.air_accel > PlayerMovement::default().air_accel);
        assert!(jetpack.max_fuel > JetpackState::default().max_fuel);
        assert_eq!(jetpack.fuel, jetpack.max_fuel);
    }
}
