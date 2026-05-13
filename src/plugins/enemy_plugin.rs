use bevy::prelude::*;
use rand::Rng;

use crate::characters::{enemy_config, spawn_cartoon_character};
use crate::components::enemy::{
    BossEnemy, DeadEnemy, Enemy, EnemyAIState, EnemyStateMachine, EnemyType,
};
use crate::components::faction::{Faction, NamedCharacter};
use crate::components::inventory::Inventory;
use crate::components::player::{Player, PlayerStats};
use crate::components::world::WorldLoot;
use crate::damage::{DamageInfo, DamageType, Damageable, Health};
use crate::events::*;
use crate::resources::WaveInfo;
use crate::state::AppState;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaveInfo>()
            .add_systems(OnEnter(AppState::Playing), setup_enemies)
            .add_systems(
                Update,
                (
                    enemy_ai_system,
                    enemy_attack_system,
                    enemy_dead_cleanup,
                    enemy_killed_reward,
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
fn setup_enemies(mut wave: ResMut<WaveInfo>) {
    *wave = WaveInfo::new();
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
    let enemy_type = if is_boss {
        EnemyType::Hybrid
    } else {
        EnemyType::Heavy
    };
    let enemy_data = Enemy::new(enemy_type, position, scale);
    let max_hp = enemy_data.scaled_health() * if is_boss { 3.0 } else { 1.5 };

    let root = spawn_cartoon_character(
        commands,
        meshes,
        materials,
        enemy_config(
            enemy_type,
            Some(faction),
            name,
            scale.clamp(0.95, 2.2) * if is_boss { 1.2 } else { 1.0 },
        ),
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
}

// ── AI System ─────────────────────────────────────────────────────────────────
fn enemy_ai_system(
    time: Res<Time>,
    player_q: Query<&Transform, With<Player>>,
    mut enemy_q: Query<
        (&mut Transform, &mut Enemy, &mut EnemyStateMachine, &Health),
        Without<Player>,
    >,
) {
    let dt = time.delta_secs();
    let Ok(player_transform) = player_q.get_single() else {
        return;
    };
    let player_pos = player_transform.translation;
    let mut rng = rand::thread_rng();

    for (mut transform, mut enemy, mut sm, health) in enemy_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }

        sm.timer += dt;
        enemy.attack_cooldown_timer = (enemy.attack_cooldown_timer - dt).max(0.0);

        let dist_to_player = transform.translation.distance(player_pos);
        let config = &enemy.config;

        match sm.current {
            EnemyAIState::Idle => {
                if dist_to_player < config.detection_range {
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
                if dist_to_player < config.detection_range {
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
                    transform.translation += move_dir * config.patrol_speed * dt * 60.0;
                    transform.look_at(pos + move_dir, Vec3::Y);
                }
            }
            EnemyAIState::Chase => {
                if dist_to_player > config.chase_range * 1.5 {
                    sm.transition(EnemyAIState::Patrol);
                } else if dist_to_player <= config.attack_range {
                    sm.transition(EnemyAIState::Attack);
                } else {
                    let to_player = (player_pos - transform.translation)
                        .with_y(0.0)
                        .normalize_or_zero();
                    transform.translation += to_player * config.chase_speed * dt * 60.0;
                    if to_player.length_squared() > 0.001 {
                        let pos = transform.translation;
                        transform.look_at(pos + to_player, Vec3::Y);
                    }
                    if enemy.enemy_type == EnemyType::Drone {
                        transform.translation.y = player_pos.y
                            + 5.0
                            + (time.elapsed_secs() * 2.0 + transform.translation.x).sin() * 0.5;
                    }
                }
            }
            EnemyAIState::Attack => {
                if dist_to_player > config.attack_range * 1.3 {
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

// ── Attack System ─────────────────────────────────────────────────────────────
fn enemy_attack_system(
    player_q: Query<&Transform, With<Player>>,
    mut enemy_q: Query<(&Transform, &mut Enemy, &EnemyStateMachine, &Health), Without<Player>>,
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
    mut damaged_ev: EventWriter<PlayerDamagedEvent>,
    mut parry_ev: EventWriter<PlayerParryEvent>,
) {
    let Ok(player_transform) = player_q.get_single() else {
        return;
    };
    let player_pos = player_transform.translation;
    let mut total_damage = 0.0;

    for (e_transform, mut enemy, sm, health) in enemy_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        if sm.current != EnemyAIState::Attack {
            continue;
        }
        if enemy.attack_cooldown_timer > 0.0 {
            continue;
        }
        let dist = e_transform.translation.distance(player_pos);
        if dist <= enemy.config.attack_range {
            total_damage += enemy.scaled_damage();
            enemy.attack_cooldown_timer = enemy.config.attack_cooldown;
        }
    }

    if total_damage > 0.0 {
        if let Ok((mut health, mut damageable, mut stats, mut parry, armor)) =
            player_damage_q.get_single_mut()
        {
            crate::plugins::player_plugin::damage_player(
                &mut health,
                &mut damageable,
                &mut stats,
                &mut parry,
                &armor,
                &DamageInfo::new(total_damage, DamageType::Kinetic),
                &mut damaged_ev,
                &mut parry_ev,
            );
        }
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
            commands.entity(entity).despawn_recursive();
            wave.enemy_count = wave.enemy_count.saturating_sub(1);
        }
    }
}

// ── Rewards on Kill ───────────────────────────────────────────────────────────
fn enemy_killed_reward(
    mut killed_ev: EventReader<EnemyKilledEvent>,
    mut player_q: Query<&mut PlayerStats, With<Player>>,
    mut enemy_q: Query<(Entity, &mut EnemyStateMachine, &Health), Without<Player>>,
    mut commands: Commands,
) {
    let Ok(mut stats) = player_q.get_single_mut() else {
        return;
    };
    for ev in killed_ev.read() {
        stats.credits += ev.credits;
        stats.experience += ev.experience;
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

// ── Loot Drop on Kill ─────────────────────────────────────────────────────────
fn enemy_loot_drop_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut killed_ev: EventReader<EnemyKilledEvent>,
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
    player_q: Query<&Transform, With<Player>>,
    mut inventory_q: Query<&mut Inventory, With<Player>>,
    loot_q: Query<(Entity, &Transform, &WorldLoot)>,
    mut msg_ev: EventWriter<UiMessageEvent>,
    mut loot_ev: EventWriter<LootCollectedEvent>,
) {
    let Ok(player_transform) = player_q.get_single() else {
        return;
    };
    let Ok(mut inventory) = inventory_q.get_single_mut() else {
        return;
    };
    let player_pos = player_transform.translation;
    let item_defs = crate::components::inventory::all_items();

    for (entity, loot_transform, loot) in loot_q.iter() {
        let dist = player_pos.distance(loot_transform.translation);
        if dist > loot.pickup_radius {
            continue;
        }

        let max_stack = item_defs
            .iter()
            .find(|i| i.id == loot.item_id)
            .map(|i| i.max_stack)
            .unwrap_or(10);

        let leftover = inventory.add_item(loot.item_id, loot.quantity, max_stack);
        let picked = loot.quantity.saturating_sub(leftover);
        if picked > 0 {
            msg_ev.send(UiMessageEvent {
                text: format!("Picked up {}x {}", picked, loot.item_id.replace('_', " ")),
                duration: 1.8,
            });
            loot_ev.send(LootCollectedEvent {
                loot_type: loot.item_id.to_string(),
                amount: picked,
            });
            commands.entity(entity).despawn_recursive();
        }
    }
}
