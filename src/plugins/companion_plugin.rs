use bevy::prelude::*;

use crate::components::companion::*;
use crate::components::enemy::Enemy;
use crate::components::player::{Player, PlayerIndex};
use crate::damage::{apply_damage, DamageInfo, DamageType, Damageable, Health};
use crate::events::{
    CompanionRecruitedEvent, EnemyDamagedEvent, EnemyKilledEvent, PlayerHealedEvent,
};
use crate::plugins::weapon_plugin::ProjectileAssets;
use crate::rendering::PbrBundle;
use crate::resources::PlaySessionTransition;
use crate::state::AppState;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct CompanionPlugin;

impl Plugin for CompanionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), setup_companions)
            .add_systems(OnEnter(AppState::MainMenu), cleanup_companions_for_menu)
            .add_systems(OnExit(AppState::Playing), cleanup_companions)
            .add_systems(
                Update,
                (
                    companion_follow_system,
                    companion_combat_system,
                    companion_heal_system,
                    companion_projectile_system,
                    recruit_companion_system,
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

// ── Setup ─────────────────────────────────────────────────────────────────────
fn setup_companions(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    transition: Res<PlaySessionTransition>,
    player_q: Query<(&PlayerIndex, &Transform), With<Player>>,
    existing_companions: Query<Entity, With<Companion>>,
) {
    if transition.resuming_from_pause || !existing_companions.is_empty() {
        return;
    }

    for (index, pt) in player_q.iter() {
        let base = pt.translation;
        let owner = index.0;

        spawn_companion_entity(
            &mut commands,
            &mut meshes,
            &mut materials,
            Companion::medic_drone(format!("P{} MedicDrone", owner + 1)).with_owner(owner),
            base + Vec3::new(3.0, 2.0, 0.0),
        );
        spawn_companion_entity(
            &mut commands,
            &mut meshes,
            &mut materials,
            Companion::pet(format!("P{} SparkPup", owner + 1)).with_owner(owner),
            base + Vec3::new(-3.0, 0.0, 0.0),
        );
    }
}

fn spawn_companion_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    companion: Companion,
    position: Vec3,
) {
    let (color, emissive_col) = match companion.kind {
        CompanionKind::Ally => (Color::srgb(0.1, 0.6, 0.7), Color::srgb(0.0, 1.0, 1.0)),
        CompanionKind::Pet => (Color::srgb(0.85, 0.7, 0.1), Color::srgb(1.0, 0.9, 0.0)),
        CompanionKind::MedicDrone => (Color::srgb(0.1, 0.55, 0.2), Color::srgb(0.0, 1.0, 0.4)),
    };

    let mat = materials.add(StandardMaterial {
        base_color: color,
        emissive: emissive_col.into(),
        metallic: 0.4,
        ..default()
    });

    // Simple body mesh (companion is represented by a small robot-like shape)
    let size = if companion.kind == CompanionKind::Pet {
        0.4
    } else {
        0.7
    };
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(size, size * 1.5, size))),
            material: MeshMaterial3d(mat),
            transform: Transform::from_translation(position),
            ..default()
        },
        companion,
    ));
}

fn cleanup_companions(
    mut commands: Commands,
    transition: Res<PlaySessionTransition>,
    companion_q: Query<Entity, With<Companion>>,
    projectile_q: Query<Entity, With<CompanionProjectile>>,
) {
    if transition.pausing {
        return;
    }

    for entity in companion_q.iter().chain(projectile_q.iter()) {
        commands.entity(entity).despawn();
    }
}

fn cleanup_companions_for_menu(
    mut commands: Commands,
    companion_q: Query<Entity, With<Companion>>,
    projectile_q: Query<Entity, With<CompanionProjectile>>,
) {
    for entity in companion_q.iter().chain(projectile_q.iter()) {
        commands.entity(entity).despawn();
    }
}

// ── Follow ────────────────────────────────────────────────────────────────────
fn companion_follow_system(
    time: Res<Time>,
    player_q: Query<(&PlayerIndex, &Transform), With<Player>>,
    mut companion_q: Query<(&mut Transform, &mut Companion), Without<Player>>,
) {
    for (mut transform, mut companion) in companion_q.iter_mut() {
        if !companion.is_alive {
            continue;
        }
        let Some((_, pt)) = player_q
            .iter()
            .find(|(index, _)| index.0 == companion.owner)
        else {
            continue;
        };
        let player_pos = pt.translation;

        companion.orbit_angle += time.delta_secs() * 0.8;

        let angle = companion.orbit_angle;
        let target = Vec3::new(
            player_pos.x + angle.cos() * companion.follow_distance,
            player_pos.y
                + if companion.kind == CompanionKind::Pet {
                    (time.elapsed_secs() * 3.0 + angle).sin() * 0.3
                } else {
                    1.5
                },
            player_pos.z + angle.sin() * companion.follow_distance,
        );

        transform.translation = transform.translation.lerp(target, time.delta_secs() * 5.0);
    }
}

// ── Combat ────────────────────────────────────────────────────────────────────
fn companion_combat_system(
    time: Res<Time>,
    mut commands: Commands,
    proj_assets: Option<Res<ProjectileAssets>>,
    mut companion_q: Query<(&Transform, &mut Companion), Without<Player>>,
    player_q: Query<(&PlayerIndex, &Transform), With<Player>>,
    enemy_q: Query<(Entity, &Transform, &Health), With<Enemy>>,
) {
    let dt = time.delta_secs();
    let Some(assets) = proj_assets else { return };

    for (c_transform, mut companion) in companion_q.iter_mut() {
        if !companion.can_attack || !companion.is_alive {
            continue;
        }

        companion.attack_timer = (companion.attack_timer - dt).max(0.0);
        if companion.attack_timer > 0.0 {
            continue;
        }
        let Some((_, owner_transform)) = player_q
            .iter()
            .find(|(index, _)| index.0 == companion.owner)
        else {
            continue;
        };

        // Find nearest alive enemy in range
        let mut nearest: Option<(Entity, Vec3, f32)> = None;
        for (e_entity, e_transform, health) in enemy_q.iter() {
            if !health.is_alive() {
                continue;
            }
            let dist = c_transform.translation.distance(e_transform.translation);
            let owner_dist = owner_transform
                .translation
                .distance(e_transform.translation);
            if dist <= companion.attack_range && owner_dist <= companion.attack_range + 10.0 {
                if nearest.map_or(true, |(_, _, d)| dist < d) {
                    nearest = Some((e_entity, e_transform.translation, dist));
                }
            }
        }

        if let Some((_, target_pos, _)) = nearest {
            let dir = (target_pos - c_transform.translation).normalize_or_zero();
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(assets.sphere_sm.clone()),
                    material: MeshMaterial3d(assets.mat_companion.clone()),
                    transform: Transform::from_translation(c_transform.translation + dir * 0.5),
                    ..default()
                },
                CompanionProjectile {
                    owner: companion.owner,
                    damage: companion.attack_damage,
                    speed: 25.0,
                    direction: dir,
                    lifetime: 3.0,
                },
            ));
            companion.attack_timer = companion.attack_cooldown;
        }
    }
}

// ── Heal ──────────────────────────────────────────────────────────────────────
fn companion_heal_system(
    time: Res<Time>,
    mut companion_q: Query<&mut Companion>,
    mut player_q: Query<(&PlayerIndex, &mut Health), With<Player>>,
    mut heal_ev: MessageWriter<PlayerHealedEvent>,
) {
    let dt = time.delta_secs();

    for mut companion in companion_q.iter_mut() {
        if !companion.can_heal || !companion.is_alive {
            continue;
        }

        companion.heal_timer = (companion.heal_timer - dt).max(0.0);
        if companion.heal_timer > 0.0 {
            continue;
        }

        let Some((_, mut p_health)) = player_q
            .iter_mut()
            .find(|(index, _)| index.0 == companion.owner)
        else {
            continue;
        };

        p_health.heal(companion.heal_amount);
        companion.heal_timer = companion.heal_cooldown;
        heal_ev.write(PlayerHealedEvent {
            amount: companion.heal_amount,
            health: p_health.current,
        });
    }
}

// ── Companion Projectile ──────────────────────────────────────────────────────
fn companion_projectile_system(
    mut commands: Commands,
    time: Res<Time>,
    mut proj_q: Query<(Entity, &mut Transform, &mut CompanionProjectile)>,
    mut enemy_q: Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        Without<CompanionProjectile>,
    >,
    mut damaged_ev: MessageWriter<EnemyDamagedEvent>,
    mut killed_ev: MessageWriter<EnemyKilledEvent>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut proj) in proj_q.iter_mut() {
        transform.translation += proj.direction * proj.speed * dt;
        proj.lifetime -= dt;

        if proj.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let mut hit = false;
        for (e_entity, e_transform, mut health, mut damageable, enemy) in enemy_q.iter_mut() {
            if !health.is_alive() {
                continue;
            }
            if transform.translation.distance(e_transform.translation) < 1.5 {
                let info = DamageInfo::new(proj.damage, DamageType::Plasma);
                let result = apply_damage(&mut health, &mut damageable, &info);
                damaged_ev.write(EnemyDamagedEvent {
                    entity: e_entity,
                    damage: result.damage_amount,
                    position: e_transform.translation,
                });
                if result.was_killed {
                    killed_ev.write(EnemyKilledEvent {
                        enemy_type: enemy.enemy_type.as_str().to_string(),
                        credits: enemy.config.credits,
                        experience: enemy.config.experience_value,
                        position: e_transform.translation,
                    });
                }
                hit = true;
                break;
            }
        }
        if hit {
            commands.entity(entity).despawn();
        }
    }
}

// ── Recruit Listener ──────────────────────────────────────────────────────────
fn recruit_companion_system(
    mut events: MessageReader<CompanionRecruitedEvent>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player_q: Query<(&PlayerIndex, &Transform), With<Player>>,
) {
    for ev in events.read() {
        let Some((_, pt)) = player_q
            .iter()
            .find(|(index, _)| index.0 == ev.player_index)
        else {
            continue;
        };
        let pos = pt.translation + Vec3::new(2.0, 0.0, 2.0);
        spawn_companion_entity(
            &mut commands,
            &mut meshes,
            &mut materials,
            Companion::ally(ev.name.clone()).with_owner(ev.player_index),
            pos,
        );
    }
}
