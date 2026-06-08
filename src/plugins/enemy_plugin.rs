use bevy::prelude::*;
use rand::Rng;

use crate::characters::{enemy_config, spawn_cartoon_character};
use crate::components::armor::ArmorSet;
use crate::components::enemy::{
    BossEnemy, CitySpyDrone, DeadEnemy, DragonBoss, Enemy, EnemyAIState, EnemyAttackVfx,
    EnemyProjectile, EnemyProjectileKind, EnemyStateMachine, EnemyType, FlyingDrone,
};
use crate::components::faction::{Faction, NamedCharacter};
use crate::components::inventory::Inventory;
use crate::components::player::{ParryState, Player, PlayerIndex, PlayerStats};
use crate::components::world::WorldLoot;
use crate::damage::{DamageInfo, DamageType, Damageable, Health};
use crate::events::*;
use crate::rendering::PbrBundle;
use crate::resources::{PlaySessionTransition, WaveInfo};
use crate::robot_pets::{salvage_for_enemy, RobotPetCollection};
use crate::state::AppState;

#[derive(Resource, Clone)]
struct EnemyAttackAssets {
    laser_mesh: Handle<Mesh>,
    fireball_mesh: Handle<Mesh>,
    beam_mesh: Handle<Mesh>,
    shockwave_mesh: Handle<Mesh>,
    laser_mat: Handle<StandardMaterial>,
    fire_mat: Handle<StandardMaterial>,
    shockwave_mat: Handle<StandardMaterial>,
}

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaveInfo>()
            .add_systems(
                OnEnter(AppState::Playing),
                (setup_enemies, setup_enemy_attack_assets),
            )
            .add_systems(OnEnter(AppState::MainMenu), cleanup_enemies_for_menu)
            .add_systems(OnExit(AppState::Playing), cleanup_enemies)
            .add_systems(
                Update,
                (
                    enemy_ai_system,
                    flying_drone_attack_system,
                    dragon_boss_system,
                    enemy_projectile_update_system,
                    enemy_attack_vfx_cleanup,
                    enemy_attack_system,
                    enemy_dead_cleanup,
                    enemy_killed_reward,
                    robot_salvage_reward_system,
                    enemy_loot_drop_system,
                    loot_bob_system,
                    loot_pickup_system,
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

// ── Initial Setup ─────────────────────────────────────────────────────────────
// Starfall I: chapter director drives all spawns. Reset only the population
// counter; no enemies are pre-spawned here.
fn setup_enemies(mut wave: ResMut<WaveInfo>, transition: Res<PlaySessionTransition>) {
    if transition.resuming_from_pause {
        return;
    }

    *wave = WaveInfo::new();
}

fn setup_enemy_attack_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    transition: Res<PlaySessionTransition>,
) {
    if transition.resuming_from_pause {
        return;
    }

    let laser_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.38, 0.92, 1.0, 0.86),
        emissive: LinearRgba::new(0.2, 2.4, 3.6, 1.0),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let fire_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.32, 0.06, 0.90),
        emissive: LinearRgba::new(5.0, 1.1, 0.0, 1.0),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let shockwave_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.68, 0.18, 0.45),
        emissive: LinearRgba::new(2.8, 1.2, 0.12, 1.0),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    commands.insert_resource(EnemyAttackAssets {
        laser_mesh: meshes.add(Sphere::new(0.20)),
        fireball_mesh: meshes.add(Sphere::new(0.70)),
        beam_mesh: meshes.add(Cylinder::new(0.22, 1.0)),
        shockwave_mesh: meshes.add(Cylinder::new(1.0, 0.10)),
        laser_mat,
        fire_mat,
        shockwave_mat,
    });
}

fn cleanup_enemies(
    mut commands: Commands,
    transition: Res<PlaySessionTransition>,
    enemy_q: Query<Entity, With<Enemy>>,
    projectile_q: Query<Entity, With<EnemyProjectile>>,
    vfx_q: Query<Entity, With<EnemyAttackVfx>>,
    loot_q: Query<Entity, With<WorldLoot>>,
) {
    if transition.pausing {
        return;
    }

    for entity in enemy_q
        .iter()
        .chain(projectile_q.iter())
        .chain(vfx_q.iter())
        .chain(loot_q.iter())
    {
        commands.entity(entity).despawn();
    }
}

fn cleanup_enemies_for_menu(
    mut commands: Commands,
    enemy_q: Query<Entity, With<Enemy>>,
    projectile_q: Query<Entity, With<EnemyProjectile>>,
    vfx_q: Query<Entity, With<EnemyAttackVfx>>,
    loot_q: Query<Entity, With<WorldLoot>>,
) {
    for entity in enemy_q
        .iter()
        .chain(projectile_q.iter())
        .chain(vfx_q.iter())
        .chain(loot_q.iter())
    {
        commands.entity(entity).despawn();
    }
}

// Spawn helpers are pub so the chapter director can call them.
pub fn random_spawn_pos(player_pos: Vec3, rng: &mut impl Rng) -> Vec3 {
    let angle: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
    let dist: f32 = rng.gen_range(30.0..80.0);
    Vec3::new(
        player_pos.x + angle.cos() * dist,
        player_pos.y,
        player_pos.z + angle.sin() * dist,
    )
}

fn preset_for_type(enemy_type: EnemyType, faction: Option<Faction>) -> &'static str {
    // Faction first (gives flavor); fall back to type.
    if let Some(f) = faction {
        match (f, enemy_type) {
            (Faction::DimensionalAlien, EnemyType::Drone) => return "JetWarden",
            (Faction::DimensionalAlien, EnemyType::SpyDrone) => return "JetWarden",
            (Faction::DimensionalAlien, EnemyType::SpikeAlien) => return "InsectoidStalker",
            (Faction::DimensionalAlien, _) => return "HybridOmega",
            (Faction::DragonRoyalty, EnemyType::Drone) => return "WolfAnimaton",
            (Faction::DragonRoyalty, EnemyType::Heavy) => return "BruteForge",
            (Faction::DragonRoyalty, _) => return "TankTitan",
            (Faction::DragonExile, EnemyType::Heavy) => return "BruteForge",
            (Faction::DragonExile, _) => return "CharredCaptain",
            (Faction::CorruptedHuman, EnemyType::Drone) => return "Nero",
            (Faction::CorruptedHuman, _) => return "ScoutPrime",
            (Faction::HeroBrother, _) => return "ScoutPrime",
            (Faction::HeroSister, _) => return "ScoutPrime",
            (Faction::WizardScientist, _) => return "ScoutPrime",
            _ => {}
        }
    }
    match enemy_type {
        EnemyType::Drone => "JetWarden",
        EnemyType::SpyDrone => "JetWarden",
        EnemyType::Soldier => "ScoutPrime",
        EnemyType::Heavy => "TankTitan",
        EnemyType::SpikeAlien => "InsectoidStalker",
        EnemyType::Hybrid => "HybridOmega",
    }
}

pub fn spawn_enemy_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    enemy_type: EnemyType,
    position: Vec3,
    difficulty_scale: f32,
    faction: Option<Faction>,
) {
    let preset_name = preset_for_type(enemy_type, faction);
    let enemy_data = Enemy::new(enemy_type, position, difficulty_scale);
    let max_hp = enemy_data.scaled_health();

    let root = spawn_cartoon_character(
        commands,
        meshes,
        materials,
        enemy_config(
            enemy_type,
            faction,
            preset_name,
            difficulty_scale.clamp(0.85, 1.8),
        ),
        position,
    );
    commands.entity(root).insert((
        enemy_data,
        EnemyStateMachine::default(),
        Health::new(max_hp),
        Damageable::default(),
        faction.unwrap_or_default(),
    ));
    if enemy_type == EnemyType::Drone {
        commands.entity(root).insert(FlyingDrone::new(position));
    }
}

/// Spawn a story-named enemy (mid-boss or boss).
#[allow(clippy::too_many_arguments)]
pub fn spawn_named_enemy(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    _preset: &'static str,
    name: &'static str,
    faction: Faction,
    position: Vec3,
    scale: f32,
    is_boss: bool,
) {
    let is_dragon_boss =
        is_boss && matches!(faction, Faction::DragonRoyalty | Faction::DragonExile);
    let enemy_type = if is_boss {
        EnemyType::Hybrid
    } else {
        EnemyType::Heavy
    };
    let enemy_data = Enemy::new(enemy_type, position, scale);
    let max_hp = enemy_data.scaled_health()
        * if is_dragon_boss {
            4.5
        } else if is_boss {
            3.0
        } else {
            1.5
        };
    let visual_scale = scale.clamp(0.95, 2.2)
        * if is_dragon_boss {
            1.85
        } else if is_boss {
            1.2
        } else {
            1.0
        };

    let root = spawn_cartoon_character(
        commands,
        meshes,
        materials,
        enemy_config(enemy_type, Some(faction), name, visual_scale),
        position,
    );
    let mut e = commands.entity(root);
    e.insert((
        enemy_data,
        EnemyStateMachine::default(),
        Health::new(max_hp),
        Damageable::default(),
        faction,
        NamedCharacter {
            id: name,
            display_name: name,
            faction,
        },
    ));
    if is_boss {
        e.insert(BossEnemy);
    }
    if is_dragon_boss {
        e.insert(DragonBoss::new(position));
    }
}

// ── AI System ─────────────────────────────────────────────────────────────────
fn enemy_ai_system(
    time: Res<Time>,
    player_q: Query<(Entity, &Transform), (With<Player>, Without<Enemy>)>,
    mut enemy_q: Query<
        (
            &mut Transform,
            &mut Enemy,
            &mut EnemyStateMachine,
            &Health,
            Option<&mut FlyingDrone>,
            Option<&CitySpyDrone>,
            Option<&DragonBoss>,
        ),
        Without<Player>,
    >,
) {
    let dt = time.delta_secs();
    let mut rng = rand::thread_rng();

    for (mut transform, mut enemy, mut sm, health, drone, city_spy, dragon_boss) in
        enemy_q.iter_mut()
    {
        if !health.is_alive() {
            continue;
        }

        sm.timer += dt;
        enemy.attack_cooldown_timer = (enemy.attack_cooldown_timer - dt).max(0.0);

        if dragon_boss.is_some() || city_spy.is_some() {
            continue;
        }

        let Some((_player_entity, player_pos, dist_to_player)) =
            closest_player(transform.translation, f32::MAX, &player_q)
        else {
            continue;
        };

        if let Some(mut drone) = drone {
            update_flying_drone(
                &mut transform,
                &mut enemy,
                &mut sm,
                &mut drone,
                player_pos,
                dist_to_player,
                time.elapsed_secs(),
                dt,
            );
            continue;
        }

        let detection_range = enemy.config.detection_range;
        let chase_range = enemy.config.chase_range;
        let attack_range = enemy.config.attack_range;
        let patrol_speed = enemy.config.patrol_speed;
        let chase_speed = enemy.config.chase_speed;

        match sm.current {
            EnemyAIState::Idle => {
                if dist_to_player < detection_range {
                    sm.transition(EnemyAIState::Chase);
                } else if sm.timer > rng.gen_range(1.0..3.0) {
                    let angle: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
                    let dist: f32 = rng.gen_range(10.0..20.0);
                    enemy.patrol_target =
                        enemy.spawn_origin + Vec3::new(angle.cos() * dist, 0.0, angle.sin() * dist);
                    sm.transition(EnemyAIState::Patrol);
                }
            }
            EnemyAIState::Patrol => {
                if dist_to_player < detection_range {
                    sm.transition(EnemyAIState::Chase);
                    continue;
                }
                let to_target = enemy.patrol_target - transform.translation;
                let to_target_flat = Vec3::new(to_target.x, 0.0, to_target.z);
                if to_target_flat.length() < 1.0 {
                    sm.transition(EnemyAIState::Idle);
                } else {
                    let move_dir = to_target_flat.normalize();
                    let pos = transform.translation;
                    transform.translation += move_dir * patrol_speed * dt * 60.0;
                    transform.look_at(pos + move_dir, Vec3::Y);
                }
            }
            EnemyAIState::Chase => {
                if dist_to_player > chase_range * 1.5 {
                    sm.transition(EnemyAIState::Patrol);
                } else if dist_to_player <= attack_range {
                    sm.transition(EnemyAIState::Attack);
                } else {
                    let to_player = (player_pos - transform.translation)
                        .with_y(0.0)
                        .normalize_or_zero();
                    transform.translation += to_player * chase_speed * dt * 60.0;
                    if to_player.length_squared() > 0.001 {
                        let pos = transform.translation;
                        transform.look_at(pos + to_player, Vec3::Y);
                    }
                }
            }
            EnemyAIState::Attack => {
                if dist_to_player > attack_range * 1.3 {
                    sm.transition(EnemyAIState::Chase);
                } else {
                    let to_player = (player_pos - transform.translation)
                        .with_y(0.0)
                        .normalize_or_zero();
                    if to_player.length_squared() > 0.001 {
                        let pos = transform.translation;
                        transform.look_at(pos + to_player, Vec3::Y);
                    }
                }
            }
            EnemyAIState::Stunned => {
                if sm.timer >= 1.5 {
                    sm.transition(EnemyAIState::Chase);
                }
            }
            EnemyAIState::Dead => {}
        }
    }
}

fn update_flying_drone(
    transform: &mut Transform,
    enemy: &mut Enemy,
    sm: &mut EnemyStateMachine,
    drone: &mut FlyingDrone,
    player_pos: Vec3,
    dist_to_player: f32,
    elapsed: f32,
    dt: f32,
) {
    drone.fire_timer = (drone.fire_timer - dt).max(0.0);
    drone.orbit_phase += dt * 0.85;

    if dist_to_player < enemy.config.detection_range * 1.8 {
        if dist_to_player <= enemy.config.attack_range + 10.0 {
            sm.transition(EnemyAIState::Attack);
        } else {
            sm.transition(EnemyAIState::Chase);
        }
    } else if sm.timer > 2.0 {
        sm.transition(EnemyAIState::Patrol);
    }

    let hover = player_pos.y + drone.altitude + (elapsed * 2.2 + drone.orbit_phase).sin() * 0.7;
    let orbit = Vec3::new(drone.orbit_phase.cos(), 0.0, drone.orbit_phase.sin());
    let side = Vec3::new(-orbit.z, 0.0, orbit.x);
    let desired = match sm.current {
        EnemyAIState::Attack | EnemyAIState::Chase => {
            player_pos + orbit * drone.orbit_radius + side * (elapsed * 1.3).sin() * 3.0
        }
        EnemyAIState::Patrol => {
            enemy.spawn_origin
                + Vec3::new(
                    (elapsed * 0.7 + drone.orbit_phase).cos() * 10.0,
                    0.0,
                    (elapsed * 0.9 + drone.orbit_phase).sin() * 10.0,
                )
        }
        _ => transform.translation,
    }
    .with_y(hover.max(enemy.spawn_origin.y + 3.0));

    let to_desired = desired - transform.translation;
    let speed = if sm.current == EnemyAIState::Attack {
        13.0
    } else {
        9.0
    };
    if to_desired.length_squared() > 0.01 {
        transform.translation += to_desired.clamp_length_max(speed * dt);
    }

    let look = player_pos + Vec3::Y * 0.7;
    if transform.translation.distance_squared(look) > 0.01 {
        transform.look_at(look, Vec3::Y);
    }
}

fn flying_drone_attack_system(
    mut commands: Commands,
    assets: Res<EnemyAttackAssets>,
    player_q: Query<(Entity, &Transform), (With<Player>, Without<FlyingDrone>)>,
    mut drone_q: Query<(
        &Transform,
        &mut Enemy,
        &mut FlyingDrone,
        &EnemyStateMachine,
        &Health,
    )>,
) {
    for (transform, mut enemy, mut drone, sm, health) in drone_q.iter_mut() {
        if !health.is_alive() || sm.current != EnemyAIState::Attack {
            continue;
        }
        if drone.fire_timer > 0.0 || enemy.attack_cooldown_timer > 0.0 {
            continue;
        }

        let Some((_player_entity, player_pos, _distance)) = closest_player(
            transform.translation,
            enemy.config.detection_range * 2.4,
            &player_q,
        ) else {
            continue;
        };

        let target = player_pos + Vec3::Y * 0.75;
        let muzzle = transform.translation + Vec3::Y * 0.15 + transform.forward().as_vec3() * 0.8;
        let direction = (target - muzzle).normalize_or_zero();
        if direction.length_squared() <= 0.001 {
            continue;
        }

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(assets.laser_mesh.clone()),
                material: MeshMaterial3d(assets.laser_mat.clone()),
                transform: Transform::from_translation(muzzle),
                ..default()
            },
            EnemyProjectile {
                kind: EnemyProjectileKind::Laser,
                damage: enemy.scaled_damage() * 0.85,
                speed: 36.0,
                direction,
                lifetime: 1.8,
                hit_radius: 0.75,
                splash_radius: 0.0,
            },
        ));
        drone.fire_timer = 0.45;
        enemy.attack_cooldown_timer = enemy.config.attack_cooldown * 0.55;
    }
}

fn dragon_boss_system(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<EnemyAttackAssets>,
    player_pos_q: Query<(Entity, &Transform, &PlayerIndex), (With<Player>, Without<BossEnemy>)>,
    mut player_damage_q: Query<
        (
            &mut Health,
            &mut Damageable,
            &mut PlayerStats,
            &mut ParryState,
            &ArmorSet,
        ),
        (With<Player>, Without<BossEnemy>),
    >,
    mut boss_q: Query<
        (&mut Transform, &mut Enemy, &mut DragonBoss, &Health),
        (With<BossEnemy>, Without<Player>),
    >,
    mut damaged_ev: MessageWriter<PlayerDamagedEvent>,
    mut parry_ev: MessageWriter<PlayerParryEvent>,
) {
    let dt = time.delta_secs();
    for (mut transform, enemy, mut boss, health) in boss_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        let Some((_player_entity, player_pos, _player_index, distance)) =
            closest_indexed_player(transform.translation, 140.0, &player_pos_q)
        else {
            continue;
        };

        let health_ratio = (health.current / health.max).clamp(0.0, 1.0);
        boss.phase = if health_ratio < 0.33 {
            3
        } else if health_ratio < 0.66 {
            2
        } else {
            1
        };

        boss.orbit_angle += dt * (0.35 + boss.phase as f32 * 0.16);
        boss.fireball_timer -= dt;
        boss.breath_timer -= dt;
        boss.slam_timer -= dt;

        let phase = boss.phase as f32;
        let orbit_radius = 24.0 - phase * 3.0;
        let arena_focus = boss.home + (player_pos - boss.home).clamp_length_max(82.0);
        let desired = arena_focus
            + Vec3::new(
                boss.orbit_angle.cos() * orbit_radius,
                8.0 + phase * 2.0 + (time.elapsed_secs() * 2.4).sin(),
                boss.orbit_angle.sin() * orbit_radius,
            );
        let to_desired = desired - transform.translation;
        if to_desired.length_squared() > 0.05 {
            transform.translation += to_desired.clamp_length_max((7.5 + phase * 2.5) * dt);
        }
        transform.look_at(player_pos + Vec3::Y * 1.1, Vec3::Y);

        let mouth = transform.translation + Vec3::Y * 1.4 + transform.forward().as_vec3() * 2.4;
        if boss.fireball_timer <= 0.0 {
            let count = boss.phase as i32;
            for i in 0..count {
                let spread = (i as f32 - (count - 1) as f32 * 0.5) * 0.09;
                let direction = ((player_pos + Vec3::Y * 0.8 - mouth)
                    + transform.right().as_vec3() * spread * distance.min(40.0))
                .normalize_or_zero();
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(assets.fireball_mesh.clone()),
                        material: MeshMaterial3d(assets.fire_mat.clone()),
                        transform: Transform::from_translation(mouth),
                        ..default()
                    },
                    EnemyProjectile {
                        kind: EnemyProjectileKind::Fireball,
                        damage: enemy.scaled_damage() * (0.32 + phase * 0.08),
                        speed: 16.0 + phase * 2.0,
                        direction,
                        lifetime: 4.0,
                        hit_radius: 1.35,
                        splash_radius: 5.0 + phase * 1.5,
                    },
                ));
            }
            boss.fireball_timer = 3.2 - phase * 0.45;
        }

        if boss.breath_timer <= 0.0 && distance < 34.0 {
            let target = player_pos + Vec3::Y * 0.9;
            spawn_enemy_beam_vfx(&mut commands, &assets, mouth, target, 0.42);
            damage_players_in_cone(
                mouth,
                (target - mouth).normalize_or_zero(),
                34.0,
                0.82,
                enemy.scaled_damage() * (0.28 + phase * 0.08),
                DamageType::Fire,
                &mut player_damage_q,
                &player_pos_q,
                &mut damaged_ev,
                &mut parry_ev,
            );
            boss.breath_timer = 4.4 - phase * 0.55;
        }

        if boss.slam_timer <= 0.0 {
            let center = Vec3::new(player_pos.x, player_pos.y + 0.12, player_pos.z);
            let radius = 8.0 + phase * 2.2;
            spawn_shockwave_vfx(&mut commands, &assets, center, radius, 0.45);
            damage_players_in_radius(
                center,
                radius,
                enemy.scaled_damage() * (0.20 + phase * 0.06),
                DamageType::Collision,
                &mut player_damage_q,
                &player_pos_q,
                &mut damaged_ev,
                &mut parry_ev,
            );
            boss.slam_timer = 6.6 - phase * 0.7;
        }
    }
}

fn enemy_projectile_update_system(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<EnemyAttackAssets>,
    mut projectile_q: Query<(Entity, &mut Transform, &mut EnemyProjectile)>,
    player_pos_q: Query<
        (Entity, &Transform, &PlayerIndex),
        (With<Player>, Without<EnemyProjectile>),
    >,
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
    for (entity, mut transform, mut projectile) in projectile_q.iter_mut() {
        transform.translation += projectile.direction * projectile.speed * dt;
        projectile.lifetime -= dt;

        let mut impact = projectile.lifetime <= 0.0 || transform.translation.y <= 0.2;
        let mut hit_player = None;
        for (player_entity, player_transform, player_index) in player_pos_q.iter() {
            if transform
                .translation
                .distance(player_transform.translation + Vec3::Y * 0.7)
                <= projectile.hit_radius
            {
                hit_player = Some((player_entity, player_index.0));
                impact = true;
                break;
            }
        }

        if impact {
            match projectile.kind {
                EnemyProjectileKind::Laser => {
                    if let Some((player_entity, player_index)) = hit_player {
                        if let Ok((mut health, mut damageable, mut stats, mut parry, armor)) =
                            player_damage_q.get_mut(player_entity)
                        {
                            crate::plugins::player_plugin::damage_player(
                                Some(player_index),
                                &mut health,
                                &mut damageable,
                                &mut stats,
                                &mut parry,
                                armor,
                                &DamageInfo::new(projectile.damage, DamageType::Laser),
                                &mut damaged_ev,
                                &mut parry_ev,
                            );
                        }
                    }
                }
                EnemyProjectileKind::Fireball => {
                    let radius = projectile.splash_radius.max(projectile.hit_radius);
                    spawn_shockwave_vfx(
                        &mut commands,
                        &assets,
                        transform.translation,
                        radius,
                        0.35,
                    );
                    damage_players_in_radius(
                        transform.translation,
                        radius,
                        projectile.damage,
                        DamageType::Fire,
                        &mut player_damage_q,
                        &player_pos_q,
                        &mut damaged_ev,
                        &mut parry_ev,
                    );
                }
            }
            commands.entity(entity).despawn();
        }
    }
}

fn enemy_attack_vfx_cleanup(
    mut commands: Commands,
    time: Res<Time>,
    mut vfx_q: Query<(Entity, &mut EnemyAttackVfx)>,
) {
    let dt = time.delta_secs();
    for (entity, mut vfx) in vfx_q.iter_mut() {
        vfx.timer -= dt;
        if vfx.timer <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn closest_player<F: bevy::ecs::query::QueryFilter>(
    origin: Vec3,
    max_range: f32,
    player_q: &Query<(Entity, &Transform), F>,
) -> Option<(Entity, Vec3, f32)> {
    player_q
        .iter()
        .filter_map(|(entity, transform)| {
            let dist = origin.distance(transform.translation);
            (dist <= max_range).then_some((entity, transform.translation, dist))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
}

fn closest_indexed_player<F: bevy::ecs::query::QueryFilter>(
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

fn damage_players_in_radius<
    DamageFilter: bevy::ecs::query::QueryFilter,
    PositionFilter: bevy::ecs::query::QueryFilter,
>(
    center: Vec3,
    radius: f32,
    damage: f32,
    damage_type: DamageType,
    player_damage_q: &mut Query<
        (
            &mut Health,
            &mut Damageable,
            &mut PlayerStats,
            &mut ParryState,
            &ArmorSet,
        ),
        DamageFilter,
    >,
    player_pos_q: &Query<(Entity, &Transform, &PlayerIndex), PositionFilter>,
    damaged_ev: &mut MessageWriter<PlayerDamagedEvent>,
    parry_ev: &mut MessageWriter<PlayerParryEvent>,
) {
    for (player_entity, player_transform, player_index) in player_pos_q.iter() {
        let dist = center.distance(player_transform.translation);
        if dist > radius {
            continue;
        }
        let falloff = 1.0 - (dist / radius).clamp(0.0, 0.8);
        if let Ok((mut health, mut damageable, mut stats, mut parry, armor)) =
            player_damage_q.get_mut(player_entity)
        {
            crate::plugins::player_plugin::damage_player(
                Some(player_index.0),
                &mut health,
                &mut damageable,
                &mut stats,
                &mut parry,
                armor,
                &DamageInfo::new(damage * falloff, damage_type),
                damaged_ev,
                parry_ev,
            );
        }
    }
}

fn damage_players_in_cone<
    DamageFilter: bevy::ecs::query::QueryFilter,
    PositionFilter: bevy::ecs::query::QueryFilter,
>(
    origin: Vec3,
    direction: Vec3,
    range: f32,
    min_dot: f32,
    damage: f32,
    damage_type: DamageType,
    player_damage_q: &mut Query<
        (
            &mut Health,
            &mut Damageable,
            &mut PlayerStats,
            &mut ParryState,
            &ArmorSet,
        ),
        DamageFilter,
    >,
    player_pos_q: &Query<(Entity, &Transform, &PlayerIndex), PositionFilter>,
    damaged_ev: &mut MessageWriter<PlayerDamagedEvent>,
    parry_ev: &mut MessageWriter<PlayerParryEvent>,
) {
    if direction.length_squared() <= 0.001 {
        return;
    }
    for (player_entity, player_transform, player_index) in player_pos_q.iter() {
        let target = player_transform.translation + Vec3::Y * 0.7;
        let to_player = target - origin;
        let dist = to_player.length();
        if dist > range || dist <= 0.01 {
            continue;
        }
        if direction.dot(to_player / dist) < min_dot {
            continue;
        }
        let falloff = 1.0 - (dist / range).clamp(0.0, 0.7);
        if let Ok((mut health, mut damageable, mut stats, mut parry, armor)) =
            player_damage_q.get_mut(player_entity)
        {
            crate::plugins::player_plugin::damage_player(
                Some(player_index.0),
                &mut health,
                &mut damageable,
                &mut stats,
                &mut parry,
                armor,
                &DamageInfo::new(damage * falloff, damage_type),
                damaged_ev,
                parry_ev,
            );
        }
    }
}

fn spawn_enemy_beam_vfx(
    commands: &mut Commands,
    assets: &EnemyAttackAssets,
    start: Vec3,
    end: Vec3,
    timer: f32,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.05 {
        return;
    }
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(assets.beam_mesh.clone()),
            material: MeshMaterial3d(assets.fire_mat.clone()),
            transform: Transform::from_translation(start + delta * 0.5)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, delta.normalize()))
                .with_scale(Vec3::new(1.0, length, 1.0)),
            ..default()
        },
        EnemyAttackVfx { timer },
    ));
}

fn spawn_shockwave_vfx(
    commands: &mut Commands,
    assets: &EnemyAttackAssets,
    center: Vec3,
    radius: f32,
    timer: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(assets.shockwave_mesh.clone()),
            material: MeshMaterial3d(assets.shockwave_mat.clone()),
            transform: Transform::from_translation(center)
                .with_scale(Vec3::new(radius, 1.0, radius)),
            ..default()
        },
        EnemyAttackVfx { timer },
    ));
}

// ── Attack System ─────────────────────────────────────────────────────────────
fn enemy_attack_system(
    player_q: Query<(Entity, &Transform, &PlayerIndex), With<Player>>,
    mut enemy_q: Query<
        (
            &Transform,
            &mut Enemy,
            &EnemyStateMachine,
            &Health,
            Option<&FlyingDrone>,
            Option<&DragonBoss>,
        ),
        Without<Player>,
    >,
    mut player_damage_q: Query<
        (
            &mut crate::damage::Health,
            &mut Damageable,
            &mut PlayerStats,
            &mut crate::components::player::ParryState,
            &crate::components::armor::ArmorSet,
        ),
        With<Player>,
    >,
    mut damaged_ev: MessageWriter<PlayerDamagedEvent>,
    mut parry_ev: MessageWriter<PlayerParryEvent>,
) {
    for (e_transform, mut enemy, sm, health, drone, dragon_boss) in enemy_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        if drone.is_some() || dragon_boss.is_some() {
            continue;
        }
        if sm.current != EnemyAIState::Attack {
            continue;
        }
        if enemy.attack_cooldown_timer > 0.0 {
            continue;
        }

        let Some((player_entity, _player_pos, player_index, _distance)) = closest_indexed_player(
            e_transform.translation,
            enemy.config.attack_range,
            &player_q,
        ) else {
            continue;
        };

        if let Ok((mut health, mut damageable, mut stats, mut parry, armor)) =
            player_damage_q.get_mut(player_entity)
        {
            crate::plugins::player_plugin::damage_player(
                Some(player_index),
                &mut health,
                &mut damageable,
                &mut stats,
                &mut parry,
                armor,
                &DamageInfo::new(enemy.scaled_damage(), DamageType::Kinetic),
                &mut damaged_ev,
                &mut parry_ev,
            );
        }
        enemy.attack_cooldown_timer = enemy.config.attack_cooldown;
    }
}

// ── Dead Cleanup ──────────────────────────────────────────────────────────────
fn enemy_dead_cleanup(
    mut commands: Commands,
    time: Res<Time>,
    mut dead_q: Query<(Entity, &mut DeadEnemy)>,
    mut wave: ResMut<WaveInfo>,
) {
    let dt = time.delta_secs();
    for (entity, mut dead) in dead_q.iter_mut() {
        dead.despawn_timer -= dt;
        if dead.despawn_timer <= 0.0 {
            commands.entity(entity).despawn();
            wave.enemy_count = wave.enemy_count.saturating_sub(1);
        }
    }
}

// ── Rewards on Kill ───────────────────────────────────────────────────────────
fn enemy_killed_reward(
    mut killed_ev: MessageReader<EnemyKilledEvent>,
    mut player_q: Query<&mut PlayerStats, With<Player>>,
    mut enemy_q: Query<(Entity, &mut EnemyStateMachine, &Health), Without<Player>>,
    mut commands: Commands,
) {
    for ev in killed_ev.read() {
        for mut stats in player_q.iter_mut() {
            stats.credits += ev.credits;
            stats.experience += ev.experience;
        }
    }
    for (entity, mut sm, health) in enemy_q.iter_mut() {
        if !health.is_alive() && sm.current != EnemyAIState::Dead {
            sm.force(EnemyAIState::Dead);
            commands
                .entity(entity)
                .insert(DeadEnemy { despawn_timer: 2.0 });
        }
    }
}

fn robot_salvage_reward_system(
    mut killed_ev: MessageReader<EnemyKilledEvent>,
    mut robots: ResMut<RobotPetCollection>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for ev in killed_ev.read() {
        let reward = salvage_for_enemy(&ev.enemy_type, ev.credits, ev.experience);
        robots.grant_salvage(reward);
        msg_ev.write(UiMessageEvent {
            text: format!(
                "Robot salvage: {}x {}",
                reward.quantity,
                reward.kind.label()
            ),
            duration: 1.4,
        });
    }
}

// ── Loot Drop on Kill ─────────────────────────────────────────────────────────
fn enemy_loot_drop_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut killed_ev: MessageReader<EnemyKilledEvent>,
) {
    let mut rng = rand::thread_rng();

    for ev in killed_ev.read() {
        // ~60% drop chance
        let roll: f32 = rng.gen();
        if roll > 0.60 {
            continue;
        }

        let (item_id, quantity, r, g, b): (&'static str, u32, f32, f32, f32) = if roll < 0.10 {
            ("health_pack", 1, 0.2, 1.0, 0.3)
        } else if roll < 0.20 {
            ("armor_shard", 1, 0.3, 0.5, 1.0)
        } else if roll < 0.35 {
            ("plasma_cell", rng.gen_range(10..25), 0.0, 0.6, 1.0)
        } else if roll < 0.48 {
            ("scrap_metal", rng.gen_range(1..4), 0.6, 0.5, 0.3)
        } else {
            ("energy_core", 1, 1.0, 0.8, 0.0)
        };

        let mat = materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            emissive: LinearRgba::new(r * 1.5, g * 1.5, b * 1.5, 1.0),
            unlit: false,
            metallic: 0.5,
            ..default()
        });

        let base_y = ev.position.y + 0.6;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.35))),
                material: MeshMaterial3d(mat),
                transform: Transform::from_translation(Vec3::new(
                    ev.position.x,
                    base_y,
                    ev.position.z,
                )),
                ..default()
            },
            WorldLoot {
                item_id,
                quantity,
                credits: 0,
                pickup_radius: 2.5,
                base_y,
            },
        ));
    }
}

// ── Loot Bob Animation ────────────────────────────────────────────────────────
fn loot_bob_system(time: Res<Time>, mut loot_q: Query<(&mut Transform, &WorldLoot)>) {
    for (mut transform, loot) in loot_q.iter_mut() {
        transform.translation.y = loot.base_y + (time.elapsed_secs() * 2.5).sin() * 0.25;
        transform.rotation = Quat::from_rotation_y(time.elapsed_secs());
    }
}

// ── Loot Pickup ───────────────────────────────────────────────────────────────
fn loot_pickup_system(
    mut commands: Commands,
    player_q: Query<(Entity, &PlayerIndex, &Transform), With<Player>>,
    mut inventory_q: Query<&mut Inventory, With<Player>>,
    loot_q: Query<(Entity, &Transform, &WorldLoot)>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
    mut loot_ev: MessageWriter<LootCollectedEvent>,
) {
    let item_defs = crate::components::inventory::all_items();

    for (entity, loot_transform, loot) in loot_q.iter() {
        let Some((player_entity, player_index, _)) = player_q
            .iter()
            .filter_map(|(player_entity, player_index, player_transform)| {
                let dist = player_transform
                    .translation
                    .distance(loot_transform.translation);
                (dist <= loot.pickup_radius).then_some((player_entity, player_index, dist))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
        else {
            continue;
        };

        let max_stack = item_defs
            .iter()
            .find(|i| i.id == loot.item_id)
            .map(|i| i.max_stack)
            .unwrap_or(10);

        let Ok(mut inventory) = inventory_q.get_mut(player_entity) else {
            continue;
        };
        let leftover = inventory.add_item(loot.item_id, loot.quantity, max_stack);
        let picked = loot.quantity.saturating_sub(leftover);
        if picked > 0 {
            msg_ev.write(UiMessageEvent {
                text: format!(
                    "P{} picked up {}x {}",
                    player_index.0 + 1,
                    picked,
                    loot.item_id.replace('_', " ")
                ),
                duration: 1.8,
            });
            loot_ev.write(LootCollectedEvent {
                loot_type: loot.item_id.to_string(),
                amount: picked,
            });
            commands.entity(entity).despawn();
        }
    }
}
