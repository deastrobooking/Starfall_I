use bevy::prelude::*;
use bevy::render::camera::Viewport;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy_rapier3d::prelude::*;

use crate::characters::{attach_cartoon_character, hero_config};
use crate::components::armor::ArmorSet;
use crate::components::inventory::Inventory;
use crate::components::player::*;
use crate::components::weapon::*;
use crate::damage::{apply_damage, DamageInfo, Damageable, Health};
use crate::events::*;
use crate::resources::{CameraShake, LocalPlayerConfig, PlayerSelectState};
use crate::state::AppState;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct PlayerPlugin;

fn third_person_camera_offset() -> Vec3 {
    Vec3::new(0.0, 2.2, 6.0)
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), (spawn_players, grab_cursor))
            .add_systems(OnExit(AppState::Playing), release_cursor)
            .add_systems(
                Update,
                (
                    player_look,
                    camera_shake_system,
                    player_movement,
                    player_dodge_update,
                    player_parry_update,
                    player_state_update,
                    player_stamina_regen,
                    player_invulnerability_update,
                    player_level_up,
                    player_died_check,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

// ── Spawn helpers ─────────────────────────────────────────────────────────────

fn player_spawn_position(index: u8) -> Vec3 {
    let base = Vec3::new(350.0, 15.0, 150.0);
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

// ── Spawn ─────────────────────────────────────────────────────────────────────
fn spawn_players(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    config: Res<LocalPlayerConfig>,
    select: Res<PlayerSelectState>,
    window_q: Query<&Window, With<PrimaryWindow>>,
) {
    let active = config.active.clamp(1, 4);
    let (win_w, win_h) = window_q
        .get_single()
        .map(|w| (w.physical_width(), w.physical_height()))
        .unwrap_or((1280, 720));

    for i in 0..active {
        let spawn_pos = player_spawn_position(i);

        let player = commands
            .spawn((
                Player,
                PlayerIndex(i),
                PlayerInput::default(),
                Transform::from_translation(spawn_pos),
                GlobalTransform::default(),
                RigidBody::KinematicPositionBased,
                Collider::capsule_y(0.6, 0.35),
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
                PlayerStats::default(),
                PlayerMovement::default(),
            ))
            .insert((
                JetpackState::default(),
                EdgeGrabState::new(),
                DodgeState::new(),
                ParryState::new(),
                PlayerStateMachine::default(),
                Health::new(100.0),
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

        attach_cartoon_character(
            &mut commands,
            &mut meshes,
            &mut materials,
            player,
            hero_config(select.character_name(i as usize)),
            spawn_pos,
        );

        let viewport = player_viewport(i, active, win_w, win_h);

        let cam_entity = commands
            .spawn((
                Camera3dBundle {
                    transform: Transform::from_translation(third_person_camera_offset()),
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
            .set_parent(player)
            .id();

        commands.entity(player).insert(PlayerCameraRef(cam_entity));
    }
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
    mut cam_q: Query<&mut Transform, With<PlayerCamera>>,
    mut damage_ev: EventReader<PlayerDamagedEvent>,
) {
    for ev in damage_ev.read() {
        let trauma = (ev.amount / 25.0).clamp(0.12, 0.65);
        shake.add_trauma(trauma);
    }

    shake.trauma = (shake.trauma - time.delta_secs() * 2.0).max(0.0);

    let base = third_person_camera_offset();
    if shake.trauma > 0.01 {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mag = shake.trauma * shake.trauma * 0.18;
        for mut cam in cam_q.iter_mut() {
            cam.translation = base
                + Vec3::new(
                    rng.gen_range(-1.0f32..1.0) * mag,
                    rng.gen_range(-0.5f32..0.5) * mag,
                    rng.gen_range(-1.0f32..1.0) * mag,
                );
        }
    } else {
        for mut cam in cam_q.iter_mut() {
            cam.translation = base;
        }
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
        mut dodge,
        transform,
        mut state,
        pi,
    ) in player_q.iter_mut()
    {
        movement.is_grounded = output.grounded;
        edge_grab.cooldown_timer = (edge_grab.cooldown_timer - dt).max(0.0);
        edge_grab.wall_contact_timer = (edge_grab.wall_contact_timer - dt).max(0.0);

        if movement.is_grounded {
            jetpack.fuel = (jetpack.fuel + jetpack.regen_rate * dt).min(jetpack.max_fuel);
            movement.velocity.y = movement.velocity.y.max(0.0);
            edge_grab.is_hanging = false;
            edge_grab.hang_timer = 0.0;
            edge_grab.wall_contact_timer = 0.0;
        }

        let fwd = transform.forward().as_vec3().with_y(0.0).normalize_or_zero();
        let right = transform.right().as_vec3().with_y(0.0).normalize_or_zero();
        let input = (fwd * pi.move_axis.y + right * pi.move_axis.x).normalize_or_zero();

        let sprinting = pi.sprint && stats.stamina > 0.0 && input.length_squared() > 0.0;
        let speed = if sprinting { movement.sprint_speed } else { movement.walk_speed };

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

        if edge_grab.is_hanging {
            edge_grab.hang_timer += dt;
            stats.stamina = (stats.stamina - edge_grab.stamina_drain_per_sec * dt).max(0.0);
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity = Vec3::ZERO;
            jetpack.is_active = false;

            if pi.jump {
                let jump_dir = (edge_grab.wall_normal + input * 0.25)
                    .with_y(0.0)
                    .normalize_or_zero();
                movement.velocity.y = edge_grab.wall_jump_vertical;
                movement.ground_velocity = jump_dir * edge_grab.wall_jump_push;
                edge_grab.is_hanging = false;
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
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

        if pi.jump && !movement.is_grounded && has_wall_contact && edge_grab.cooldown_timer <= 0.0 {
            let jump_dir = (edge_grab.wall_normal + input * 0.25)
                .with_y(0.0)
                .normalize_or_zero();
            movement.velocity.y = edge_grab.wall_jump_vertical;
            movement.ground_velocity = jump_dir * edge_grab.wall_jump_push;
            edge_grab.cooldown_timer = edge_grab.grab_cooldown;
            state.force(PlayerState::Jetpack);
        } else if pi.jump && movement.is_grounded {
            movement.velocity.y = movement.jump_force;
            movement.is_grounded = false;
            state.transition(PlayerState::Jetpack);
        }

        let can_grab_edge = !movement.is_grounded
            && !dodge.is_dodging
            && edge_grab.cooldown_timer <= 0.0
            && movement.velocity.y <= -0.02
            && pushing_into_wall
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

        if pi.jetpack && !movement.is_grounded && jetpack.fuel > 0.0 {
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
            movement.velocity.y -= movement.gravity;
            movement.velocity.y = movement.velocity.y.max(-2.0);
            if pushing_into_wall && movement.velocity.y < -0.35 {
                movement.velocity.y = -0.35;
                state.transition(PlayerState::WallSliding);
            }
        }

        let mut h_vel = if movement.is_grounded {
            let v = input * speed;
            movement.ground_velocity = v;
            v
        } else {
            let target = input * speed;
            let air_control = if edge_grab.cooldown_timer > 0.0 { 0.04 } else { 0.15 };
            movement.ground_velocity = movement.ground_velocity.lerp(target, air_control);
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

// ── Dodge Update ──────────────────────────────────────────────────────────────
fn player_dodge_update(
    time: Res<Time>,
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
    for (mut dodge, mut stats, mut damageable, transform, mut state, pi) in player_q.iter_mut() {
        dodge.cooldown_timer = (dodge.cooldown_timer - dt).max(0.0);

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
            && stats.stamina >= dodge.dodge_cost
            && state.current != PlayerState::Dead
        {
            let fwd = transform.forward().as_vec3().with_y(0.0).normalize_or_zero();
            let right = transform.right().as_vec3().with_y(0.0).normalize_or_zero();
            // Dodge in the direction the player is moving, or backward if idle.
            let input = (fwd * pi.move_axis.y + right * pi.move_axis.x).normalize_or_zero();
            dodge.dodge_direction = if input.length_squared() > 0.01 { -input } else { -fwd };
            dodge.is_dodging = true;
            dodge.dodge_timer = dodge.dodge_duration;
            dodge.cooldown_timer = dodge.dodge_cooldown;
            stats.stamina -= dodge.dodge_cost;
            state.force(PlayerState::Dodging);
            dodge_ev.send(PlayerDodgeEvent);
        }
    }
}

// ── Parry Update ──────────────────────────────────────────────────────────────
fn player_parry_update(
    time: Res<Time>,
    mut player_q: Query<(&mut ParryState, &mut PlayerStateMachine, &PlayerInput), With<Player>>,
) {
    let dt = time.delta_secs();
    for (mut parry, mut state, pi) in player_q.iter_mut() {
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
            parry.parry_timer = parry.parry_window;
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
            ev.send(PlayerStaminaChangedEvent { stamina: stats.stamina });
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
        amount: result.damage_amount,
        remaining: health.current,
    });
}
