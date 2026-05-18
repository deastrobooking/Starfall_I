use bevy::prelude::*;
use bevy::render::camera::Viewport;
use bevy::transform::TransformSystem;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy_rapier3d::prelude::*;

use crate::character_blueprint::{
    BodyRecipe, CartoonAppearanceRecipe, CharacterBlueprint, CharacterPaletteRecipe,
};
use crate::characters::{
    accent_preset, attach_cartoon_character, despawn_cartoon_character_parts, hair_preset,
    hero_config, hero_config_with_overrides, outfit_preset,
};
use crate::components::armor::ArmorSet;
use crate::components::character::CartoonPart;
use crate::components::inventory::Inventory;
use crate::components::player::*;
use crate::components::weapon::*;
use crate::components::world::BoatPassenger;
use crate::damage::{apply_damage, DamageInfo, Damageable, Health};
use crate::events::*;
use crate::perks::PerkTree;
use crate::rendering::Camera3dBundle;
use crate::resources::{
    CameraShake, LocalPlayerConfig, PlaySessionTransition, PlayerSelectState, PlayerSlotConfig,
};
use crate::state::AppState;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct PlayerPlugin;

fn third_person_camera_offset() -> Vec3 {
    Vec3::new(0.0, 2.2, 6.0)
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), (spawn_players, grab_cursor))
            .add_systems(OnEnter(AppState::MainMenu), cleanup_players_for_menu)
            .add_systems(OnExit(AppState::Playing), release_cursor)
            .add_systems(
                Update,
                (
                    dedupe_player_entities,
                    player_look,
                    camera_shake_system,
                    player_movement,
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
                    .before(TransformSystem::TransformPropagate)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

// ── Spawn helpers ─────────────────────────────────────────────────────────────

fn player_spawn_position(index: u8) -> Vec3 {
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

    (
        stats,
        movement,
        dodge,
        Collider::capsule_y(half_height.clamp(0.44, 0.86), radius.clamp(0.26, 0.50)),
    )
}

fn upgraded_player_blueprint(name: &'static str, slot: &PlayerSlotConfig) -> CharacterBlueprint {
    let base = hero_config(name);
    let body = match name {
        "Vincenzo" => BodyRecipe {
            height: 1.06,
            shoulder_width: 1.10,
            chest_size: 1.06,
            arm_length: 1.04,
            leg_length: 1.08,
            hand_size: 1.06,
            foot_size: 1.08,
            head_size: 1.03,
            mass: 1.08,
            muscle: 1.10,
            spine_posture: 0.06,
            ..BodyRecipe::default()
        },
        "Antonio" => BodyRecipe {
            height: 1.02,
            shoulder_width: 0.98,
            chest_size: 0.98,
            arm_length: 1.03,
            leg_length: 1.12,
            hand_size: 1.02,
            foot_size: 1.07,
            head_size: 1.02,
            mass: 0.95,
            muscle: 1.02,
            spine_posture: 0.03,
            ..BodyRecipe::default()
        },
        "Angelo" => BodyRecipe {
            height: 1.01,
            shoulder_width: 1.00,
            chest_size: 1.02,
            arm_length: 1.07,
            leg_length: 1.06,
            hand_size: 1.07,
            foot_size: 1.06,
            head_size: 1.04,
            mass: 0.98,
            muscle: 1.04,
            spine_posture: -0.02,
            ..BodyRecipe::default()
        },
        "Joseph" => BodyRecipe {
            height: 0.99,
            shoulder_width: 1.17,
            chest_size: 1.12,
            arm_length: 1.02,
            leg_length: 1.03,
            hand_size: 1.10,
            foot_size: 1.12,
            head_size: 1.05,
            mass: 1.16,
            muscle: 1.18,
            spine_posture: 0.08,
            ..BodyRecipe::default()
        },
        _ => BodyRecipe {
            height: 1.04,
            shoulder_width: 1.06,
            chest_size: 1.04,
            leg_length: 1.08,
            foot_size: 1.08,
            head_size: 1.03,
            mass: 1.04,
            muscle: 1.06,
            ..BodyRecipe::default()
        },
    };
    let palette = CharacterPaletteRecipe {
        skin: base.skin,
        outfit: slot.outfit_idx.map(outfit_preset).unwrap_or(base.outfit),
        accent: slot.accent_idx.map(accent_preset).unwrap_or(base.accent),
        hair: slot.hair_idx.map(hair_preset).unwrap_or(base.hair),
        eye: base.eye_color,
    };
    let appearance = CartoonAppearanceRecipe {
        has_hood: slot.has_hood.unwrap_or(true),
        has_cape: slot.has_cape.unwrap_or(true),
        has_gloves: slot.has_gloves.unwrap_or(true),
        has_boots: slot.has_boots.unwrap_or(true),
        has_shoulder_pads: slot
            .has_shoulder_pads
            .unwrap_or(matches!(name, "Vincenzo" | "Joseph")),
        has_visor: slot.has_visor.unwrap_or(false),
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
    window_q: Query<&Window, With<PrimaryWindow>>,
    existing_players: Query<Entity, With<Player>>,
    existing_parts: Query<(Entity, &CartoonPart)>,
) {
    if transition.resuming_from_pause || !existing_players.is_empty() {
        return;
    }

    let active = config.active.clamp(1, 4);
    let (win_w, win_h) = window_q
        .get_single()
        .map(|w| (w.physical_width(), w.physical_height()))
        .unwrap_or((1280, 720));

    for i in 0..active {
        let spawn_pos = player_spawn_position(i);
        let slot = &select.slots[i as usize];
        let character_name = select.character_name(i as usize);
        let runtime_blueprint = slot
            .blueprint
            .clone()
            .unwrap_or_else(|| upgraded_player_blueprint(character_name, slot));
        let (player_stats, player_movement, dodge_state, player_collider) =
            authored_player_defaults(Some(&runtime_blueprint));

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
                    offset: CharacterLength::Absolute(0.02),
                    slide: true,
                    autostep: Some(CharacterAutostep {
                        max_height: CharacterLength::Absolute(0.5),
                        min_width: CharacterLength::Absolute(0.2),
                        include_dynamic_bodies: false,
                    }),
                    snap_to_ground: Some(CharacterLength::Absolute(0.2)),
                    ..default()
                },
                KinematicCharacterControllerOutput::default(),
                player_stats.clone(),
                player_movement,
            ))
            .insert((
                JetpackState::default(),
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
                WeaponInventory::default(),
                SpecialWeaponInventory::default(),
                BeamSabre::default(),
                MeleeCombo::new(),
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
        attach_cartoon_character(
            &mut commands,
            &mut meshes,
            &mut materials,
            player,
            character_config,
            spawn_pos,
        );

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
                        hdr: true,
                        order: i as isize,
                        viewport,
                        ..default()
                    },
                    ..default()
                },
                PlayerCamera,
                CameraPitch::default(),
                bevy::core_pipeline::bloom::Bloom {
                    intensity: 0.25,
                    ..default()
                },
                DistanceFog {
                    color: Color::srgba(0.02, 0.02, 0.08, 1.0),
                    falloff: FogFalloff::ExponentialSquared { density: 0.0015 },
                    ..default()
                },
            ))
            .id();

        commands.entity(player).insert(PlayerCameraRef(cam_entity));
    }
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
        commands.entity(camera).try_despawn_recursive();
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
                commands.entity(kept).try_despawn_recursive();
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
                    commands.entity(camera_ref.0).try_despawn_recursive();
                }
                commands.entity(entity).try_despawn_recursive();
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
        commands.entity(camera_ref.0).try_despawn_recursive();
    }
    commands.entity(player).try_despawn_recursive();
}

fn grab_cursor(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if let Ok(mut window) = windows.get_single_mut() {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
    }
}

fn release_cursor(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if let Ok(mut window) = windows.get_single_mut() {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
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
    mut damage_ev: EventReader<PlayerDamagedEvent>,
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
    player_q: Query<
        (&PlayerIndex, &Transform, &PlayerCameraRef),
        (With<Player>, Without<PlayerCamera>),
    >,
    mut cam_q: Query<(Entity, &mut Transform, &CameraPitch), (With<PlayerCamera>, Without<Player>)>,
) {
    let mut referenced = Vec::new();

    for (index, player_transform, camera_ref) in player_q.iter() {
        referenced.push(camera_ref.0);
        let shake_offset = camera_shake_offset(shake.trauma_for(index.0));
        if let Ok((_, mut camera_transform, pitch)) = cam_q.get_mut(camera_ref.0) {
            *camera_transform = player_camera_transform(player_transform, pitch.0, shake_offset);
        }
    }

    for (camera, mut camera_transform, _) in cam_q.iter_mut() {
        if !referenced.contains(&camera) {
            camera_transform.translation = Vec3::new(0.0, -10_000.0, 0.0);
            commands.entity(camera).try_despawn_recursive();
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

// ── Movement & Physics ────────────────────────────────────────────────────────
fn player_movement(
    time: Res<Time>,
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
            edge_grab.wall_contact_timer = 0.0;
        } else {
            movement.coyote_timer = (movement.coyote_timer - dt).max(0.0);
        }

        let fwd = transform
            .forward()
            .as_vec3()
            .with_y(0.0)
            .normalize_or_zero();
        let right = transform.right().as_vec3().with_y(0.0).normalize_or_zero();
        let input = (fwd * pi.move_axis.y + right * pi.move_axis.x).normalize_or_zero();

        let sprinting = pi.sprint && stats.stamina > 0.0 && input.length_squared() > 0.0;
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
            if wall_sliding && movement.velocity.y < -movement.wall_slide_speed {
                movement.velocity.y = -movement.wall_slide_speed;
                state.transition(PlayerState::WallSliding);
            }
        }

        let target_h_vel = input * speed;
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
            let air_accel = if edge_grab.cooldown_timer > 0.0 {
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
            if input.length_squared() > 0.01 {
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

        if best.map_or(true, |(_, s)| s < strength) {
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
    mut dodge_ev: EventWriter<PlayerDodgeEvent>,
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
            let input = (fwd * pi.move_axis.y + right * pi.move_axis.x).normalize_or_zero();
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
            dodge_ev.send(PlayerDodgeEvent);
        }
    }
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
    mut q: Query<(&mut PlayerStats, &DodgeState), With<Player>>,
    mut ev: EventWriter<PlayerStaminaChangedEvent>,
) {
    let dt = time.delta_secs();
    for (mut stats, dodge) in q.iter_mut() {
        if !dodge.is_dodging && stats.stamina < stats.max_stamina {
            stats.stamina = (stats.stamina + 10.0 * dt).min(stats.max_stamina);
            ev.send(PlayerStaminaChangedEvent {
                stamina: stats.stamina,
            });
        }
    }
}

fn player_perk_health_regen(
    time: Res<Time>,
    perks: Res<PerkTree>,
    mut q: Query<(&mut Health, &PlayerStateMachine), With<Player>>,
) {
    let regen = perks.regen_per_sec();
    if regen <= 0.0 {
        return;
    }
    let dt = time.delta_secs();
    for (mut health, state) in q.iter_mut() {
        if state.current != PlayerState::Dead && health.is_alive() && health.current < health.max {
            health.current = (health.current + regen * dt).min(health.max);
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
    mut level_ev: EventWriter<PlayerLevelUpEvent>,
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
            level_ev.send(PlayerLevelUpEvent { level: stats.level });
        }
    }
}

// ── Death Check ───────────────────────────────────────────────────────────────
// Game over when ALL players are dead.
fn player_died_check(
    mut q: Query<(&Health, &mut PlayerStateMachine), With<Player>>,
    mut died_ev: EventWriter<PlayerDiedEvent>,
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
        died_ev.send(PlayerDiedEvent);
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
    damaged_ev: &mut EventWriter<PlayerDamagedEvent>,
    parry_ev: &mut EventWriter<PlayerParryEvent>,
) {
    if !health.is_alive() || damageable.is_invulnerable {
        return;
    }

    if parry.is_parrying {
        parry.is_parrying = false;
        parry_ev.send(PlayerParryEvent { success: true });
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

    damaged_ev.send(PlayerDamagedEvent {
        player_index,
        amount: result.damage_amount,
        remaining: health.current,
    });
}
