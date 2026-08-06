use avian3d::prelude::{
    Collider as AvianCollider, Sensor, ShapeCastConfig, SpatialQuery, SpatialQueryFilter,
};
use bevy::prelude::*;
use rand::Rng;

use crate::character::presets::{
    decorate_reptilian_character, enemy_config, spawn_cartoon_character,
};
use crate::combat::damage::{
    apply_damage, area_damage_falloff, DamageInfo, DamageResistance, DamageType, Damageable, Health,
};
use crate::combat::hitstop::hitstop_inactive;
use crate::components::armor::ArmorSet;
use crate::components::enemy::{
    boss_phase, BossEnemy, CitySpyDrone, DeadEnemy, DragonBoss, DragonCaste, DroneVariant, Enemy,
    EnemyAIState, EnemyAttackVfx, EnemyHomingProjectile, EnemyProjectile, EnemyProjectileKind,
    EnemyStateMachine, EnemyType, FlyingDrone, MechBoss, MountainSpiderMech, RiftBoss,
};
use crate::components::faction::{Faction, NamedCharacter};
use crate::components::inventory::Inventory;
use crate::components::player::{ParryState, Player, PlayerIndex, PlayerStats};
use crate::components::world::{NpcRoadVehicle, WorldLoot};
use crate::engine::game_rng::GameRng;
use crate::engine::physics::{
    prelude::{Collider, CollisionProfile, GameCollisionLayer, RigidBody},
    world_line_of_sight,
};
use crate::engine::rendering::{PbrBundle, SpatialBundle};
use crate::engine::state::AppState;
use crate::events::*;
use crate::plugins::world_plugin::terrain_surface_y;
use crate::resources::{GameSettings, PlaySessionTransition, WaveInfo};
use crate::world::hacking::{Hackable, HackedUnit};
use crate::world::robot_pets::{salvage_for_enemy, RobotPetCollection};

#[derive(Resource, Clone)]
struct EnemyAttackAssets {
    laser_mesh: Handle<Mesh>,
    fireball_mesh: Handle<Mesh>,
    shell_mesh: Handle<Mesh>,
    beam_mesh: Handle<Mesh>,
    shockwave_mesh: Handle<Mesh>,
    laser_mat: Handle<StandardMaterial>,
    fire_mat: Handle<StandardMaterial>,
    shell_mat: Handle<StandardMaterial>,
    shockwave_mat: Handle<StandardMaterial>,
}

const TANK_ROOT_CLEARANCE: f32 = 1.18;

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
                    enemy_ai_system.run_if(hitstop_inactive),
                    apply_enemy_knockback.run_if(hitstop_inactive),
                    ground_enemy_tanks_to_terrain
                        .after(enemy_ai_system)
                        .after(apply_enemy_knockback)
                        .run_if(hitstop_inactive),
                    flying_drone_attack_system.run_if(hitstop_inactive),
                    dragon_boss_system.run_if(hitstop_inactive),
                    rift_boss_system.run_if(hitstop_inactive),
                    mech_boss_system.run_if(hitstop_inactive),
                    assign_enemy_projectile_collision_profiles
                        .before(enemy_projectile_update_system),
                    enemy_projectile_update_system.run_if(hitstop_inactive),
                    enemy_attack_vfx_cleanup,
                    enemy_attack_system.run_if(hitstop_inactive),
                    enemy_dead_cleanup,
                    enemy_killed_reward,
                    robot_salvage_reward_system,
                    enemy_loot_drop_system,
                    loot_homing_system,
                    loot_pickup_system,
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

fn assign_enemy_projectile_collision_profiles(
    mut commands: Commands,
    projectile_q: Query<Entity, Added<EnemyProjectile>>,
) {
    for entity in projectile_q.iter() {
        commands
            .entity(entity)
            .insert(CollisionProfile::EnemyProjectile);
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
    let shell_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.34, 0.16, 0.035),
        emissive: LinearRgba::new(5.5, 2.1, 0.12, 1.0),
        metallic: 0.62,
        perceptual_roughness: 0.28,
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
        shell_mesh: meshes.add(Sphere::new(0.42)),
        beam_mesh: meshes.add(Cylinder::new(0.22, 1.0)),
        shockwave_mesh: meshes.add(Cylinder::new(1.0, 0.10)),
        laser_mat,
        fire_mat,
        shell_mat,
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
            (Faction::DragonRoyalty, EnemyType::Soldier) => return "Sunscale Lizard Legionary",
            (Faction::DragonRoyalty, EnemyType::SpikeAlien) => return "Cometclaw Raptor",
            (Faction::DragonRoyalty, EnemyType::Heavy | EnemyType::Hybrid) => {
                return "Crown Dragon Knight";
            }
            (Faction::DragonRoyalty, _) => return "Sunscale Lizard Scout",
            (Faction::DragonExile, EnemyType::Soldier) => return "Frostscale Lizard Legionary",
            (Faction::DragonExile, EnemyType::SpikeAlien) => return "Voidclaw Raptor",
            (Faction::DragonExile, EnemyType::Heavy | EnemyType::Hybrid) => {
                return "Exile Dragon Knight";
            }
            (Faction::DragonExile, _) => return "Frostscale Lizard Scout",
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
        EnemyType::Tank => "TankTitan",
        EnemyType::SpikeAlien => "InsectoidStalker",
        EnemyType::Hybrid => "HybridOmega",
    }
}

/// Faction/type-flavoured damage mitigation so weapon choice matters:
/// dragons shrug off fire, exiles resist the cold-blue laser family,
/// scallarian flesh soaks plasma, and drone chassis deflect kinetic rounds.
fn enemy_damageable(enemy: &Enemy, faction: Option<Faction>) -> Damageable {
    let res = |damage_type, reduction| DamageResistance {
        damage_type,
        reduction,
    };
    let mut resistances = Vec::new();
    match faction.unwrap_or_default() {
        Faction::DragonRoyalty => {
            resistances.push(res(DamageType::Fire, 0.6));
            resistances.push(res(DamageType::Melee, 0.15));
        }
        Faction::DragonExile => {
            resistances.push(res(DamageType::Fire, 0.3));
            resistances.push(res(DamageType::Laser, 0.25));
        }
        Faction::CorruptedHuman => resistances.push(res(DamageType::Rift, 0.4)),
        Faction::WizardScientist => resistances.push(res(DamageType::Electric, 0.5)),
        _ => resistances.push(res(DamageType::Plasma, 0.25)),
    }
    if matches!(enemy.enemy_type, EnemyType::Drone | EnemyType::SpyDrone) {
        resistances.push(res(DamageType::Kinetic, 0.3));
    }
    Damageable::with_defense(enemy.config.defense, resistances)
}

/// Drain the knockback impulse accumulated by `apply_damage` into a decaying
/// shove on the enemy transform — hits finally move their target.
fn apply_enemy_knockback(
    time: Res<Time>,
    mut enemy_q: Query<(&mut Transform, &mut Damageable), (With<Enemy>, Without<Player>)>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut damageable) in enemy_q.iter_mut() {
        if damageable.pending_knockback.length_squared() < 1e-4 {
            continue;
        }
        // Shove scales with the impulse; exponential decay over ~0.25 s.
        transform.translation += damageable.pending_knockback * dt * 2.4;
        let decay = (-dt * 9.0).exp();
        damageable.pending_knockback *= decay;
        if damageable.pending_knockback.length_squared() < 1e-4 {
            damageable.pending_knockback = Vec3::ZERO;
        }
    }
}

fn tank_root_y(terrain_y: f32, difficulty_scale: f32) -> f32 {
    terrain_y + TANK_ROOT_CLEARANCE * difficulty_scale.clamp(0.85, 1.8)
}

/// Keep the kinematic siege-tank chassis planted after both AI locomotion and
/// combat knockback have changed its horizontal position. Other enemies retain
/// their existing movement model, and elevated road decks do not pull ground
/// tanks off the underlying terrain.
fn ground_enemy_tanks_to_terrain(
    settings: Res<GameSettings>,
    mut enemy_q: Query<(&mut Transform, &Enemy), (Without<Player>, Without<MountainSpiderMech>)>,
) {
    for (mut transform, enemy) in enemy_q.iter_mut() {
        if enemy.enemy_type != EnemyType::Tank {
            continue;
        }
        let terrain_y = terrain_surface_y(
            transform.translation.x,
            transform.translation.z,
            settings.world_seed,
        );
        transform.translation.y = tank_root_y(terrain_y, enemy.difficulty_scale);
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
) -> Entity {
    spawn_enemy_entity_with_drone_variant(
        commands,
        meshes,
        materials,
        enemy_type,
        position,
        difficulty_scale,
        faction,
        DroneVariant::Scout,
    )
}

/// Spawn one of the deliberately remote flying-craft classes. The underlying
/// enemy type remains `Drone`, preserving rewards, saves, and damage handling.
pub fn spawn_drone_variant_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    variant: DroneVariant,
    position: Vec3,
    difficulty_scale: f32,
    faction: Option<Faction>,
) -> Entity {
    spawn_enemy_entity_with_drone_variant(
        commands,
        meshes,
        materials,
        EnemyType::Drone,
        position,
        difficulty_scale,
        faction,
        variant,
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_enemy_entity_with_drone_variant(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    enemy_type: EnemyType,
    position: Vec3,
    difficulty_scale: f32,
    faction: Option<Faction>,
    drone_variant: DroneVariant,
) -> Entity {
    let preset_name = preset_for_type(enemy_type, faction);
    let mut enemy_data = Enemy::new(enemy_type, position, difficulty_scale);
    if enemy_type == EnemyType::Drone {
        tune_drone_variant(&mut enemy_data, drone_variant);
    }
    let max_hp = enemy_data.scaled_health();

    let root = match enemy_type {
        EnemyType::Drone => spawn_drone_craft(
            commands,
            meshes,
            materials,
            drone_variant,
            position,
            difficulty_scale,
        ),
        EnemyType::Tank => spawn_tank_craft(
            commands,
            meshes,
            materials,
            position,
            difficulty_scale.clamp(0.85, 1.8),
        ),
        _ => spawn_cartoon_character(
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
        ),
    };
    let dragon_caste = (!matches!(enemy_type, EnemyType::Drone | EnemyType::Tank))
        .then_some(faction)
        .flatten()
        .filter(|f| matches!(f, Faction::DragonRoyalty | Faction::DragonExile))
        .map(|_| DragonCaste::for_enemy(enemy_type));
    if let (Some(caste), Some(dragon_faction)) = (dragon_caste, faction) {
        decorate_reptilian_character(
            commands,
            meshes,
            materials,
            root,
            caste,
            dragon_faction,
            difficulty_scale.clamp(0.85, 1.8)
                * match enemy_type {
                    EnemyType::Drone => 0.82,
                    EnemyType::SpyDrone => 0.64,
                    EnemyType::Soldier => 1.0,
                    EnemyType::Heavy => 1.22,
                    EnemyType::Tank => 1.55,
                    EnemyType::SpikeAlien => 1.05,
                    EnemyType::Hybrid => 1.35,
                },
        );
    }
    let damageable = enemy_damageable(&enemy_data, faction);
    let drone_body_scale = drone_craft_scale(drone_variant, difficulty_scale);
    commands.entity(root).insert((
        enemy_data,
        EnemyStateMachine::default(),
        Health::new(max_hp),
        damageable,
        faction.unwrap_or_default(),
        RigidBody::KinematicPositionBased,
        match enemy_type {
            EnemyType::Drone | EnemyType::SpyDrone => drone_hurtbox(drone_body_scale),
            EnemyType::Tank => tank_hurtbox(difficulty_scale),
            _ => Collider::capsule_y(
                0.9 * difficulty_scale.clamp(0.85, 1.8),
                difficulty_scale.clamp(0.85, 1.8),
            ),
        },
        CollisionProfile::EnemyHurtbox,
        Sensor,
    ));
    if enemy_type == EnemyType::Drone {
        commands.entity(root).insert((
            FlyingDrone::for_variant(position, drone_variant),
            drone_variant,
            Hackable::scallarian_drone(),
        ));
    }
    if let Some(caste) = dragon_caste {
        commands.entity(root).insert(caste);
    }
    root
}

fn drone_variant_body_scale(variant: DroneVariant) -> f32 {
    match variant {
        DroneVariant::Scout => 1.0,
        DroneVariant::HeavyFighter => 1.55,
        DroneVariant::SiegeGunship => 2.25,
    }
}

fn drone_craft_scale(variant: DroneVariant, difficulty_scale: f32) -> f32 {
    drone_variant_body_scale(variant) * difficulty_scale.clamp(0.85, 1.8)
}

fn drone_hurtbox(body_scale: f32) -> Collider {
    cuboid_hurtbox_from_half_extents(drone_hurtbox_half_extents(body_scale))
}

fn drone_hurtbox_half_extents(body_scale: f32) -> Vec3 {
    // Hull rotation reaches ~0.61 high / 2.13 long; the banked wings reach
    // ~3.95 wide and the rear engines ~2.49 long. A flat craft-aligned box is
    // intentionally a little forgiving without creating invisible vertical
    // hits above and below the silhouette.
    Vec3::new(3.95, 0.68, 2.50) * body_scale
}

fn tank_hurtbox(difficulty_scale: f32) -> Collider {
    cuboid_hurtbox_from_half_extents(tank_hurtbox_half_extents(difficulty_scale))
}

fn tank_hurtbox_half_extents(difficulty_scale: f32) -> Vec3 {
    Vec3::new(1.9, 1.4, 3.2) * difficulty_scale.clamp(0.85, 1.8)
}

/// Gameplay collider dimensions are authored as half-extents, while Avian's
/// cuboid constructor ultimately consumes full axis lengths. The compatibility
/// collider retains half-extents and performs that conversion exactly once.
fn cuboid_hurtbox_from_half_extents(half_extents: Vec3) -> Collider {
    Collider::cuboid(half_extents.x, half_extents.y, half_extents.z)
}

fn tune_drone_variant(enemy: &mut Enemy, variant: DroneVariant) {
    match variant {
        DroneVariant::Scout => {}
        DroneVariant::HeavyFighter => {
            enemy.config.max_health *= 2.4;
            enemy.config.attack_damage *= 1.55;
            enemy.config.defense += 7.0;
            enemy.config.attack_cooldown *= 1.25;
            enemy.config.experience_value = 45;
            enemy.config.credits = 35;
        }
        DroneVariant::SiegeGunship => {
            enemy.config.max_health *= 4.5;
            enemy.config.attack_damage *= 2.15;
            enemy.config.defense += 13.0;
            enemy.config.attack_cooldown *= 1.6;
            enemy.config.detection_range += 12.0;
            enemy.config.chase_range += 14.0;
            enemy.config.experience_value = 90;
            enemy.config.credits = 75;
        }
    }
}

fn spawn_drone_craft(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    variant: DroneVariant,
    position: Vec3,
    difficulty_scale: f32,
) -> Entity {
    let (name, hull_color, glow_color) = match variant {
        DroneVariant::Scout => (
            "Scallarian Scout Drone",
            Color::srgb(0.12, 0.18, 0.22),
            Color::srgb(0.18, 0.92, 1.0),
        ),
        DroneVariant::HeavyFighter => (
            "Scallarian Heavy Fighter",
            Color::srgb(0.24, 0.12, 0.30),
            Color::srgb(1.0, 0.30, 0.72),
        ),
        DroneVariant::SiegeGunship => (
            "Scallarian Siege Gunship",
            Color::srgb(0.20, 0.08, 0.08),
            Color::srgb(1.0, 0.42, 0.08),
        ),
    };
    // The procedural mesh and hurtbox share this exact scale. In particular,
    // late-chapter difficulty must not create an invisible oversized capsule.
    let scale = drone_craft_scale(variant, difficulty_scale);
    let hull = materials.add(StandardMaterial {
        base_color: hull_color,
        metallic: 0.88,
        perceptual_roughness: 0.24,
        ..default()
    });
    let glow = materials.add(StandardMaterial {
        base_color: glow_color,
        emissive: LinearRgba::from(glow_color) * 4.0,
        unlit: true,
        ..default()
    });
    let body_mesh = meshes.add(Cuboid::new(2.8 * scale, 0.8 * scale, 4.2 * scale));
    let wing_mesh = meshes.add(Cuboid::new(3.2 * scale, 0.18 * scale, 1.6 * scale));
    let engine_mesh = meshes.add(Sphere::new(0.34 * scale));
    let root = commands
        .spawn((
            Transform::from_translation(position),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Name::new(name),
        ))
        .id();
    commands.entity(root).with_children(|craft| {
        craft.spawn((
            PbrBundle {
                mesh: Mesh3d(body_mesh),
                material: MeshMaterial3d(hull.clone()),
                transform: Transform::from_rotation(Quat::from_rotation_x(-0.10)),
                ..default()
            },
            Name::new("Drone Hull"),
        ));
        for side in [-1.0_f32, 1.0] {
            craft.spawn((
                PbrBundle {
                    mesh: Mesh3d(wing_mesh.clone()),
                    material: MeshMaterial3d(hull.clone()),
                    transform: Transform::from_xyz(side * 2.35 * scale, 0.0, 0.25 * scale)
                        .with_rotation(Quat::from_rotation_z(-side * 0.12)),
                    ..default()
                },
                Name::new("Drone Wing"),
            ));
            craft.spawn((
                PbrBundle {
                    mesh: Mesh3d(engine_mesh.clone()),
                    material: MeshMaterial3d(glow.clone()),
                    transform: Transform::from_xyz(side * 1.05 * scale, 0.0, 2.15 * scale),
                    ..default()
                },
                Name::new("Drone Engine"),
            ));
        }
    });
    root
}

/// Procedural siege-tank silhouette. The authoritative enemy/collider lives
/// on the returned root; every tread, hull, turret, sensor, and barrel part is
/// a child so the ordinary enemy AI can move and face the whole vehicle.
fn spawn_tank_craft(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    ground_position: Vec3,
    scale: f32,
) -> Entity {
    let scale = scale.clamp(0.85, 1.8);
    let armor = materials.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.23, 0.21),
        metallic: 0.82,
        perceptual_roughness: 0.34,
        ..default()
    });
    let tread = materials.add(StandardMaterial {
        base_color: Color::srgb(0.035, 0.04, 0.05),
        metallic: 0.65,
        perceptual_roughness: 0.52,
        ..default()
    });
    let hostile_glow = materials.add(StandardMaterial {
        base_color: Color::srgb(0.92, 0.12, 0.035),
        emissive: LinearRgba::new(5.2, 0.34, 0.06, 1.0),
        metallic: 0.25,
        perceptual_roughness: 0.2,
        ..default()
    });

    let tread_mesh = meshes.add(Cuboid::new(0.88 * scale, 0.78 * scale, 5.4 * scale));
    let hull_mesh = meshes.add(Cuboid::new(3.2 * scale, 1.15 * scale, 4.6 * scale));
    let trim_mesh = meshes.add(Cuboid::new(0.08 * scale, 0.16 * scale, 4.35 * scale));
    let turret_base_mesh = meshes.add(Cylinder::new(1.16 * scale, 0.40 * scale));
    let turret_mesh = meshes.add(Cuboid::new(2.4 * scale, 0.95 * scale, 2.45 * scale));
    let barrel_mesh = meshes.add(Cylinder::new(0.22 * scale, 3.65 * scale));
    let muzzle_mesh = meshes.add(Cylinder::new(0.32 * scale, 0.42 * scale));
    let sensor_mesh = meshes.add(Cuboid::new(0.92 * scale, 0.18 * scale, 0.08 * scale));

    // Callers provide terrain-surface Y. Lift the authoritative root until
    // the lower faces of both treads meet that surface.
    let root_position = ground_position + Vec3::Y * TANK_ROOT_CLEARANCE * scale;
    let root = commands
        .spawn((
            SpatialBundle::from_transform(Transform::from_translation(root_position)),
            Name::new("Scallarian Siege Tank"),
        ))
        .id();
    commands.entity(root).with_children(|tank| {
        for side in [-1.0_f32, 1.0] {
            tank.spawn((
                PbrBundle {
                    mesh: Mesh3d(tread_mesh.clone()),
                    material: MeshMaterial3d(tread.clone()),
                    transform: Transform::from_xyz(side * 1.38 * scale, -0.78 * scale, 0.0),
                    ..default()
                },
                Name::new("Tank Tread"),
            ));
            tank.spawn((
                PbrBundle {
                    mesh: Mesh3d(trim_mesh.clone()),
                    material: MeshMaterial3d(hostile_glow.clone()),
                    transform: Transform::from_xyz(side * 1.63 * scale, -0.28 * scale, 0.0),
                    ..default()
                },
                Name::new("Tank Hull Trim"),
            ));
        }
        tank.spawn((
            PbrBundle {
                mesh: Mesh3d(hull_mesh),
                material: MeshMaterial3d(armor.clone()),
                transform: Transform::from_xyz(0.0, -0.24 * scale, 0.0),
                ..default()
            },
            Name::new("Tank Hull"),
        ));
        tank.spawn((
            PbrBundle {
                mesh: Mesh3d(turret_base_mesh),
                material: MeshMaterial3d(armor.clone()),
                transform: Transform::from_xyz(0.0, 0.56 * scale, 0.0),
                ..default()
            },
            Name::new("Tank Turret Ring"),
        ));
        tank.spawn((
            PbrBundle {
                mesh: Mesh3d(turret_mesh),
                material: MeshMaterial3d(armor.clone()),
                transform: Transform::from_xyz(0.0, 1.03 * scale, 0.08 * scale),
                ..default()
            },
            Name::new("Tank Turret"),
        ));
        tank.spawn((
            PbrBundle {
                mesh: Mesh3d(barrel_mesh),
                material: MeshMaterial3d(armor),
                transform: Transform::from_xyz(0.0, 1.08 * scale, -2.15 * scale)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ..default()
            },
            Name::new("Tank Barrel"),
        ));
        tank.spawn((
            PbrBundle {
                mesh: Mesh3d(muzzle_mesh),
                material: MeshMaterial3d(hostile_glow.clone()),
                transform: Transform::from_xyz(0.0, 1.08 * scale, -3.94 * scale)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ..default()
            },
            Name::new("Tank Muzzle"),
        ));
        tank.spawn((
            PbrBundle {
                mesh: Mesh3d(sensor_mesh),
                material: MeshMaterial3d(hostile_glow),
                transform: Transform::from_xyz(0.0, 1.28 * scale, -1.18 * scale),
                ..default()
            },
            Name::new("Tank Sensor"),
        ));
    });
    root
}

/// Base gameplay statline for a Forge creature, chosen by its authored role.
pub fn creature_enemy_base_type(role: crate::robots::creature::CreatureRole) -> EnemyType {
    use crate::robots::creature::CreatureRole;
    match role {
        CreatureRole::Civilian | CreatureRole::Ally | CreatureRole::Pet | CreatureRole::Scout => {
            EnemyType::Soldier
        }
        CreatureRole::Bruiser => EnemyType::Heavy,
        CreatureRole::Artillery => EnemyType::SpikeAlien,
        CreatureRole::Boss => EnemyType::Hybrid,
    }
}

/// Spawn a published Creature Forge recipe as a combat-ready enemy: the
/// forge-built robot hierarchy carries the standard enemy gameplay component
/// set, with the statline derived from the creature's authored role.
pub fn spawn_published_creature_enemy(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    spec: &crate::robots::creature::CreatureSpec,
    position: Vec3,
    difficulty_scale: f32,
    faction: Option<Faction>,
) -> Entity {
    let enemy_type = creature_enemy_base_type(spec.role);
    let enemy_data = Enemy::new(enemy_type, position, difficulty_scale);
    let max_hp = enemy_data.scaled_health();
    let root =
        match crate::robots::factory::spawn_creature(commands, meshes, materials, spec, position) {
            Ok(root) => root,
            // Published records passed validation at save time; if a stale one
            // fails now, still field a fighter from its raw style.
            Err(_) => crate::robots::factory::spawn_robot(
                commands,
                meshes,
                materials,
                &spec.compiled_style(),
                position,
            ),
        };
    let damageable = enemy_damageable(&enemy_data, faction);
    let body_scale = difficulty_scale.clamp(0.85, 1.8);
    commands.entity(root).insert((
        enemy_data,
        EnemyStateMachine::default(),
        Health::new(max_hp),
        damageable,
        faction.unwrap_or_default(),
        RigidBody::KinematicPositionBased,
        Collider::capsule_y(0.9 * body_scale, body_scale),
        CollisionProfile::EnemyHurtbox,
        Sensor,
    ));
    root
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
    if matches!(faction, Faction::DragonRoyalty | Faction::DragonExile) {
        let caste = DragonCaste::HumanoidDragon;
        decorate_reptilian_character(
            commands,
            meshes,
            materials,
            root,
            caste,
            faction,
            visual_scale,
        );
        commands.entity(root).insert(caste);
    }
    let damageable = enemy_damageable(&enemy_data, Some(faction));
    let mut e = commands.entity(root);
    e.insert((
        enemy_data,
        EnemyStateMachine::default(),
        Health::new(max_hp),
        damageable,
        faction,
        NamedCharacter {
            id: name,
            display_name: name,
            faction,
        },
        RigidBody::KinematicPositionBased,
        Collider::capsule_y(0.9 * visual_scale, visual_scale),
        CollisionProfile::EnemyHurtbox,
        Sensor,
    ));
    if is_boss {
        e.insert(BossEnemy);
    }
    if is_dragon_boss {
        e.insert(DragonBoss::new(position));
    } else if is_boss {
        // Faction-distinct boss brains: corrupted humans/wizard rivals pilot
        // reactor mechs; Scallarians (and anything else) fight as rift
        // champions. Dragons keep their flight controller above.
        match faction {
            Faction::CorruptedHuman | Faction::WizardScientist => {
                e.insert(MechBoss::new(position));
            }
            _ => {
                e.insert(RiftBoss::new(position));
            }
        }
    }
}

// ── AI System ─────────────────────────────────────────────────────────────────
fn enemy_ai_system(
    mut game_rng: ResMut<GameRng>,
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
            (Option<&DragonBoss>, Option<&RiftBoss>, Option<&MechBoss>),
        ),
        (
            Without<Player>,
            Without<HackedUnit>,
            Without<MountainSpiderMech>,
            Without<crate::combat::feedback::Flinch>,
        ),
    >,
) {
    let dt = time.delta_secs();
    let rng = game_rng.world();

    for (
        mut transform,
        mut enemy,
        mut sm,
        health,
        drone,
        city_spy,
        (dragon_boss, rift_boss, mech_boss),
    ) in enemy_q.iter_mut()
    {
        if !health.is_alive() {
            continue;
        }

        sm.timer += dt;
        enemy.attack_cooldown_timer = (enemy.attack_cooldown_timer - dt).max(0.0);

        if dragon_boss.is_some() || rift_boss.is_some() || mech_boss.is_some() || city_spy.is_some()
        {
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
                    if !enemy.patrol_waypoints.is_empty() {
                        enemy.patrol_index =
                            (enemy.patrol_index + 1) % enemy.patrol_waypoints.len();
                        enemy.patrol_target = enemy.patrol_waypoints[enemy.patrol_index];
                    } else {
                        let a: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
                        let d: f32 = rng.gen_range(10.0..20.0);
                        enemy.patrol_target =
                            enemy.spawn_origin + Vec3::new(a.cos() * d, 0.0, a.sin() * d);
                    }
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
    drone.burst_timer = (drone.burst_timer - dt).max(0.0);
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
    } * drone.speed_multiplier;
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
    mut drone_q: Query<
        (
            &Transform,
            &mut Enemy,
            &mut FlyingDrone,
            &EnemyStateMachine,
            &Health,
            Option<&DroneVariant>,
        ),
        Without<HackedUnit>,
    >,
) {
    for (transform, mut enemy, mut drone, sm, health, variant) in drone_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        if sm.current != EnemyAIState::Attack {
            drone.burst_shots_remaining = 0;
            drone.burst_timer = 0.0;
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
        let variant = variant.copied().unwrap_or(DroneVariant::Scout);

        // Heavy-fighter bolts are separated by 120 ms so the burst reads as
        // three deliberate shots rather than one over-bright projectile.
        if variant == DroneVariant::HeavyFighter && drone.burst_shots_remaining > 0 {
            if drone.burst_timer > 0.0 {
                continue;
            }
            let shot_index = 3 - drone.burst_shots_remaining;
            spawn_heavy_fighter_burst_shot(
                &mut commands,
                &assets,
                transform,
                target,
                enemy.scaled_damage(),
                shot_index,
            );
            drone.burst_shots_remaining -= 1;
            if drone.burst_shots_remaining == 0 {
                arm_drone_fire_cooldown(&mut enemy, &mut drone);
            } else {
                drone.burst_timer = 0.12;
            }
            continue;
        }

        if drone.fire_timer > 0.0 || enemy.attack_cooldown_timer > 0.0 {
            continue;
        }

        match variant {
            DroneVariant::Scout => {
                spawn_drone_laser(
                    &mut commands,
                    &assets,
                    transform,
                    target,
                    DroneLaserShot {
                        damage: enemy.scaled_damage() * 0.85,
                        speed: 36.0,
                        lateral_muzzle: 0.0,
                        lateral_aim: 0.0,
                    },
                );
                arm_drone_fire_cooldown(&mut enemy, &mut drone);
            }
            DroneVariant::HeavyFighter => {
                spawn_heavy_fighter_burst_shot(
                    &mut commands,
                    &assets,
                    transform,
                    target,
                    enemy.scaled_damage(),
                    0,
                );
                drone.burst_shots_remaining = 2;
                drone.burst_timer = 0.12;
            }
            DroneVariant::SiegeGunship => {
                for (lateral_muzzle, lateral_aim) in
                    [(-2.2, -0.48), (-0.75, -0.16), (0.75, 0.16), (2.2, 0.48)]
                {
                    spawn_drone_laser(
                        &mut commands,
                        &assets,
                        transform,
                        target,
                        DroneLaserShot {
                            damage: enemy.scaled_damage() * 0.38,
                            speed: 32.0,
                            lateral_muzzle,
                            lateral_aim,
                        },
                    );
                }
                arm_drone_fire_cooldown(&mut enemy, &mut drone);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DroneLaserShot {
    damage: f32,
    speed: f32,
    lateral_muzzle: f32,
    lateral_aim: f32,
}

fn spawn_drone_laser(
    commands: &mut Commands,
    assets: &EnemyAttackAssets,
    craft: &Transform,
    target: Vec3,
    shot: DroneLaserShot,
) {
    let right = craft.right().as_vec3();
    let muzzle = craft.translation
        + Vec3::Y * 0.15
        + craft.forward().as_vec3() * 0.8
        + right * shot.lateral_muzzle;
    let direction = (target + right * shot.lateral_aim - muzzle).normalize_or_zero();
    if direction.length_squared() <= 0.001 {
        return;
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
            damage: shot.damage,
            speed: shot.speed,
            direction,
            lifetime: 1.8,
            hit_radius: 0.75,
            splash_radius: 0.0,
        },
    ));
}

fn spawn_heavy_fighter_burst_shot(
    commands: &mut Commands,
    assets: &EnemyAttackAssets,
    craft: &Transform,
    target: Vec3,
    base_damage: f32,
    shot_index: u8,
) {
    let index = shot_index.min(2) as usize;
    let lateral_muzzle = [-0.82_f32, 0.82, -0.82][index];
    let lateral_aim = [-0.28_f32, 0.0, 0.28][index];
    spawn_drone_laser(
        commands,
        assets,
        craft,
        target,
        DroneLaserShot {
            damage: base_damage * 0.48,
            speed: 42.0,
            lateral_muzzle,
            lateral_aim,
        },
    );
}

fn arm_drone_fire_cooldown(enemy: &mut Enemy, drone: &mut FlyingDrone) {
    drone.fire_timer = 0.45 * drone.fire_interval_multiplier;
    enemy.attack_cooldown_timer = enemy.config.attack_cooldown * 0.55;
}

fn dragon_boss_system(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<EnemyAttackAssets>,
    spatial_query: SpatialQuery,
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
                4.0,
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
                &spatial_query,
                center,
                radius,
                enemy.scaled_damage() * (0.20 + phase * 0.06),
                DamageType::Collision,
                5.5,
                &mut player_damage_q,
                &player_pos_q,
                &mut damaged_ev,
                &mut parry_ev,
            );
            boss.slam_timer = 6.6 - phase * 0.7;
        }
    }
}

/// Scallarian rift champion: teleporting summoner. Hovers in a weaving drift,
/// blinks to a new bearing around its target (portal flash at both ends),
/// fires widening rift-laser volleys, and from phase 2 tears open portals
/// that pull in reinforcements. Phase 3 blinks end in a radial nova.
#[allow(clippy::too_many_arguments)]
fn rift_boss_system(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<EnemyAttackAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player_pos_q: Query<(Entity, &Transform, &PlayerIndex), (With<Player>, Without<BossEnemy>)>,
    mut boss_q: Query<
        (&mut Transform, &Enemy, &mut RiftBoss, &Health),
        (With<BossEnemy>, Without<Player>),
    >,
) {
    let dt = time.delta_secs();
    for (mut transform, enemy, mut boss, health) in boss_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        let Some((_e, player_pos, _idx, distance)) =
            closest_indexed_player(transform.translation, 150.0, &player_pos_q)
        else {
            continue;
        };

        boss.phase = boss_phase(health.current, health.max);
        let phase = boss.phase as f32;
        boss.weave_angle += dt * (0.9 + phase * 0.25);
        boss.volley_timer -= dt;
        boss.blink_timer -= dt;
        boss.summon_timer -= dt;

        // Menacing hover-weave around the current position (movement is
        // mostly teleports; the drift keeps it alive between blinks).
        let hover = Vec3::new(
            boss.weave_angle.sin() * 2.2,
            2.6 + (boss.weave_angle * 1.7).sin() * 0.8,
            boss.weave_angle.cos() * 2.2,
        );
        let anchor = boss.home + (player_pos - boss.home).clamp_length_max(70.0);
        let desired = Vec3::new(anchor.x, player_pos.y, anchor.z) + hover;
        let to_desired = desired - transform.translation;
        if to_desired.length_squared() > 0.04 {
            transform.translation += to_desired.clamp_length_max((3.0 + phase) * dt);
        }
        transform.look_at(player_pos + Vec3::Y * 1.0, Vec3::Y);

        let muzzle = transform.translation + Vec3::Y * 1.2 + transform.forward().as_vec3() * 1.6;

        // Rift volley: fan of lasers that tightens with phase.
        if boss.volley_timer <= 0.0 {
            let count = 2 + boss.phase as i32;
            for i in 0..count {
                let spread = (i as f32 - (count - 1) as f32 * 0.5) * (0.14 - phase * 0.02);
                let direction = ((player_pos + Vec3::Y * 0.9 - muzzle)
                    + transform.right().as_vec3() * spread * distance.min(36.0))
                .normalize_or_zero();
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(assets.laser_mesh.clone()),
                        material: MeshMaterial3d(assets.laser_mat.clone()),
                        transform: Transform::from_translation(muzzle),
                        ..default()
                    },
                    EnemyProjectile {
                        kind: EnemyProjectileKind::Laser,
                        damage: enemy.scaled_damage() * (0.26 + phase * 0.07),
                        speed: 22.0 + phase * 3.0,
                        direction,
                        lifetime: 3.2,
                        hit_radius: 1.0,
                        splash_radius: 0.0,
                    },
                ));
            }
            boss.volley_timer = 2.8 - phase * 0.45;
        }

        // Blink: reposition on a shrinking ring around the player, with a
        // portal flash at both ends. Phase 3 arrivals detonate a laser nova.
        if boss.blink_timer <= 0.0 && distance > 6.0 {
            spawn_shockwave_vfx(&mut commands, &assets, transform.translation, 2.4, 0.30);
            let ring = 20.0 - phase * 4.0;
            let angle = boss.weave_angle * 1.9;
            let arrival =
                player_pos + Vec3::new(angle.cos() * ring, 2.2 + phase * 0.6, angle.sin() * ring);
            transform.translation = arrival;
            spawn_shockwave_vfx(&mut commands, &assets, arrival, 2.8, 0.30);

            if boss.phase >= 3 {
                for i in 0..8 {
                    let theta = i as f32 * std::f32::consts::TAU / 8.0;
                    let direction = Vec3::new(theta.cos(), -0.12, theta.sin()).normalize();
                    commands.spawn((
                        PbrBundle {
                            mesh: Mesh3d(assets.laser_mesh.clone()),
                            material: MeshMaterial3d(assets.laser_mat.clone()),
                            transform: Transform::from_translation(arrival),
                            ..default()
                        },
                        EnemyProjectile {
                            kind: EnemyProjectileKind::Laser,
                            damage: enemy.scaled_damage() * 0.30,
                            speed: 20.0,
                            direction,
                            lifetime: 2.4,
                            hit_radius: 1.0,
                            splash_radius: 0.0,
                        },
                    ));
                }
            }
            boss.blink_timer = 5.5 - phase * 0.8;
        }

        // Portal reinforcements from phase 2 on.
        if boss.phase >= 2 && boss.summon_timer <= 0.0 {
            let kinds: &[EnemyType] = if boss.phase >= 3 {
                &[EnemyType::Soldier, EnemyType::Drone]
            } else {
                &[EnemyType::Drone]
            };
            for (i, kind) in kinds.iter().enumerate() {
                let theta = boss.weave_angle * 2.3 + i as f32 * 2.4;
                let spot =
                    transform.translation + Vec3::new(theta.cos() * 5.0, -1.2, theta.sin() * 5.0);
                spawn_shockwave_vfx(&mut commands, &assets, spot, 2.0, 0.35);
                spawn_enemy_entity(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    *kind,
                    spot,
                    enemy.difficulty_scale,
                    Some(Faction::default()),
                );
            }
            boss.summon_timer = 10.0 - phase * 1.2;
        }
    }
}

/// Corrupted-human reactor mech: grounded brawler-artillery. Strafes at
/// standoff range, fires laser barrages, telegraphs a committed charge dash
/// that detonates a shockwave on arrival, and from phase 2 cycles a brief
/// invulnerable reactor shield.
#[allow(clippy::too_many_arguments)]
fn mech_boss_system(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<EnemyAttackAssets>,
    spatial_query: SpatialQuery,
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
        (
            &mut Transform,
            &Enemy,
            &mut MechBoss,
            &Health,
            &mut Damageable,
        ),
        (With<BossEnemy>, Without<Player>),
    >,
    mut damaged_ev: MessageWriter<PlayerDamagedEvent>,
    mut parry_ev: MessageWriter<PlayerParryEvent>,
) {
    let dt = time.delta_secs();
    for (mut transform, enemy, mut boss, health, mut damageable) in boss_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        let Some((_e, player_pos, _idx, distance)) =
            closest_indexed_player(transform.translation, 130.0, &player_pos_q)
        else {
            continue;
        };

        boss.phase = boss_phase(health.current, health.max);
        let phase = boss.phase as f32;
        boss.barrage_timer -= dt;
        boss.charge_timer -= dt;
        boss.shield_cycle_timer -= dt;

        // Reactor shield cycles (phase 2+): brief invulnerability with a
        // visible pulse so players learn to wait it out.
        if boss.shielded_remaining > 0.0 {
            boss.shielded_remaining -= dt;
            if boss.shielded_remaining <= 0.0 {
                damageable.is_invulnerable = false;
            } else if (boss.shielded_remaining * 3.0).fract() < dt * 3.0 {
                spawn_shockwave_vfx(&mut commands, &assets, transform.translation, 3.4, 0.22);
            }
        } else if boss.phase >= 2 && boss.shield_cycle_timer <= 0.0 {
            boss.shielded_remaining = 2.2;
            damageable.is_invulnerable = true;
            boss.shield_cycle_timer = 9.0 - phase * 1.0;
        }

        // Committed charge dash.
        if boss.charging > 0.0 {
            boss.charging -= dt;
            transform.translation += boss.charge_dir * (26.0 + phase * 5.0) * dt;
            if boss.charging <= 0.0 {
                let center = transform.translation;
                let radius = 7.0 + phase * 1.6;
                spawn_shockwave_vfx(&mut commands, &assets, center, radius, 0.45);
                damage_players_in_radius(
                    &spatial_query,
                    center,
                    radius,
                    enemy.scaled_damage() * (0.24 + phase * 0.07),
                    DamageType::Collision,
                    5.5,
                    &mut player_damage_q,
                    &player_pos_q,
                    &mut damaged_ev,
                    &mut parry_ev,
                );
            }
            continue;
        }

        // Grounded standoff strafing.
        let standoff = 14.0 - phase * 1.5;
        let to_player = (player_pos - transform.translation).with_y(0.0);
        let dist_flat = to_player.length().max(0.01);
        let toward = to_player / dist_flat;
        let tangent = Vec3::new(-toward.z, 0.0, toward.x) * boss.strafe_dir;
        let range_correction = (dist_flat - standoff) * 0.55;
        let velocity = (toward * range_correction + tangent * (4.5 + phase))
            .clamp_length_max(9.0 + phase * 2.0);
        transform.translation += velocity * dt;
        transform.look_at(player_pos + Vec3::Y * 0.8, Vec3::Y);
        // Flip strafe direction on a slow deterministic cadence.
        if (time.elapsed_secs() * 0.31).sin() > 0.995 {
            boss.strafe_dir = -boss.strafe_dir;
        }

        // Laser barrage.
        if boss.barrage_timer <= 0.0 {
            let muzzle =
                transform.translation + Vec3::Y * 1.6 + transform.forward().as_vec3() * 1.8;
            let count = 3 + boss.phase as i32;
            for i in 0..count {
                let spread = (i as f32 - (count - 1) as f32 * 0.5) * 0.07;
                let direction = ((player_pos + Vec3::Y * 0.8 - muzzle)
                    + transform.right().as_vec3() * spread * distance.min(30.0))
                .normalize_or_zero();
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(assets.laser_mesh.clone()),
                        material: MeshMaterial3d(assets.laser_mat.clone()),
                        transform: Transform::from_translation(muzzle),
                        ..default()
                    },
                    EnemyProjectile {
                        kind: EnemyProjectileKind::Laser,
                        damage: enemy.scaled_damage() * (0.22 + phase * 0.06),
                        speed: 26.0 + phase * 2.5,
                        direction,
                        lifetime: 2.8,
                        hit_radius: 1.0,
                        splash_radius: 0.0,
                    },
                ));
            }
            boss.barrage_timer = 2.6 - phase * 0.35;
        }

        // Telegraph + commit the charge.
        if boss.charge_timer <= 0.0 && distance > 8.0 && distance < 60.0 {
            spawn_shockwave_vfx(&mut commands, &assets, transform.translation, 2.2, 0.4);
            boss.charge_dir = toward;
            boss.charging = 0.75 + phase * 0.05;
            boss.charge_timer = 7.5 - phase * 0.9;
        }
    }
}

fn enemy_projectile_update_system(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<EnemyAttackAssets>,
    spatial_query: SpatialQuery,
    mut projectile_q: Query<(
        Entity,
        &mut Transform,
        &mut EnemyProjectile,
        Option<&mut EnemyHomingProjectile>,
    )>,
    player_pos_q: Query<
        (Entity, &Transform, &PlayerIndex),
        (With<Player>, Without<EnemyProjectile>),
    >,
    mut player_health_q: ParamSet<(
        Query<&Health, With<Player>>,
        Query<
            (
                &mut Health,
                &mut Damageable,
                &mut PlayerStats,
                &mut ParryState,
                &ArmorSet,
            ),
            With<Player>,
        >,
    )>,
    mut road_vehicle_q: Query<
        (
            Entity,
            &Transform,
            &mut Health,
            &mut Damageable,
            &NpcRoadVehicle,
        ),
        (
            With<NpcRoadVehicle>,
            Without<Player>,
            Without<EnemyProjectile>,
        ),
    >,
    mut damaged_ev: MessageWriter<PlayerDamagedEvent>,
    mut parry_ev: MessageWriter<PlayerParryEvent>,
) {
    let dt = time.delta_secs();
    let player_targets = {
        let health_q = player_health_q.p0();
        player_pos_q
            .iter()
            .filter_map(|(entity, transform, index)| {
                health_q
                    .get(entity)
                    .ok()
                    .map(|health| (entity, transform.translation, index.0, health.is_alive()))
            })
            .collect::<Vec<_>>()
    };
    for (entity, mut transform, mut projectile, homing) in projectile_q.iter_mut() {
        if let Some(mut homing) = homing {
            let locked_target = homing
                .target
                .and_then(|target| {
                    player_targets
                        .iter()
                        .find(|(entity, _, _, alive)| *entity == target && *alive)
                })
                .map(|(entity, position, _, _)| (*entity, *position));
            let target = locked_target.or_else(|| {
                closest_living_indexed_player(
                    transform.translation,
                    homing.acquisition_range,
                    player_targets.iter().copied(),
                )
                .map(|(entity, position, _, _)| (entity, position))
            });
            if let Some((target_entity, target_position)) = target {
                homing.target = Some(target_entity);
                let desired =
                    (target_position + Vec3::Y * 0.8 - transform.translation).normalize_or_zero();
                projectile.direction = steer_enemy_projectile(
                    projectile.direction,
                    desired,
                    homing.turn_rate_radians * dt,
                );
                if projectile.direction.length_squared() > 0.001 {
                    transform.look_to(projectile.direction, Vec3::Y);
                }
            } else {
                homing.target = None;
            }
        }
        let previous_position = transform.translation;
        transform.translation += projectile.direction * projectile.speed * dt;
        projectile.lifetime -= dt;

        let mut impact = projectile.lifetime <= 0.0 || transform.translation.y <= 0.2;
        let mut hit_player = None;
        let mut hit_vehicle = false;
        if !impact {
            let displacement = transform.translation - previous_position;
            let filter = SpatialQueryFilter::from_mask([
                GameCollisionLayer::World,
                GameCollisionLayer::Player,
            ]);
            let collision = Dir3::new(displacement).ok().and_then(|direction| {
                spatial_query.cast_shape(
                    &AvianCollider::sphere(projectile.hit_radius.max(0.05)),
                    previous_position,
                    Quat::IDENTITY,
                    direction,
                    &ShapeCastConfig::from_max_distance(displacement.length()),
                    &filter,
                )
            });
            if let Some(collision) = collision {
                transform.translation =
                    previous_position + displacement.normalize_or_zero() * collision.distance;
                impact = true;
                if let Ok((player_entity, _, player_index)) = player_pos_q.get(collision.entity) {
                    hit_player = Some((player_entity, player_index.0));
                } else if let Ok((_, _, mut health, mut damageable, _)) =
                    road_vehicle_q.get_mut(collision.entity)
                {
                    hit_vehicle = true;
                    if health.is_alive() && matches!(projectile.kind, EnemyProjectileKind::Laser) {
                        let info = DamageInfo::new(projectile.damage, DamageType::Laser);
                        apply_damage(&mut health, &mut damageable, &info);
                    }
                }
            }
        }

        if impact {
            match projectile.kind {
                EnemyProjectileKind::Laser => {
                    if let Some((player_entity, player_index)) = hit_player.filter(|_| !hit_vehicle)
                    {
                        let mut player_damage_q = player_health_q.p1();
                        if let Ok((mut health, mut damageable, mut stats, mut parry, armor)) =
                            player_damage_q.get_mut(player_entity)
                        {
                            // Mirror the player projectile knockback (2.2),
                            // shoving along the shot's travel direction.
                            crate::plugins::player_plugin::damage_player(
                                Some(player_index),
                                &mut health,
                                &mut damageable,
                                &mut stats,
                                &mut parry,
                                armor,
                                &DamageInfo::new(projectile.damage, DamageType::Laser)
                                    .with_knockback(2.2)
                                    .with_hit_direction(projectile.direction),
                                &mut damaged_ev,
                                &mut parry_ev,
                            );
                        }
                    }
                }
                EnemyProjectileKind::Fireball | EnemyProjectileKind::Shell => {
                    let (damage_type, knockback) = match projectile.kind {
                        EnemyProjectileKind::Fireball => (DamageType::Fire, 3.5),
                        EnemyProjectileKind::Shell => (DamageType::Explosive, 6.5),
                        EnemyProjectileKind::Laser => unreachable!("laser handled above"),
                    };
                    let radius = projectile.splash_radius.max(projectile.hit_radius);
                    spawn_shockwave_vfx(
                        &mut commands,
                        &assets,
                        transform.translation,
                        radius,
                        0.35,
                    );
                    let mut player_damage_q = player_health_q.p1();
                    damage_players_in_radius(
                        &spatial_query,
                        transform.translation,
                        radius,
                        projectile.damage,
                        damage_type,
                        knockback,
                        &mut player_damage_q,
                        &player_pos_q,
                        &mut damaged_ev,
                        &mut parry_ev,
                    );
                    damage_road_vehicles_in_radius(
                        &spatial_query,
                        transform.translation,
                        radius,
                        projectile.damage,
                        damage_type,
                        &mut road_vehicle_q,
                    );
                }
            }
            commands.entity(entity).despawn();
        }
    }
}

fn steer_enemy_projectile(current: Vec3, desired: Vec3, max_radians: f32) -> Vec3 {
    let current = current.normalize_or_zero();
    let desired = desired.normalize_or_zero();
    if current.length_squared() <= 0.001 {
        return desired;
    }
    if desired.length_squared() <= 0.001 {
        return current;
    }
    let angle = current.dot(desired).clamp(-1.0, 1.0).acos();
    if angle <= max_radians.max(0.0) || angle <= 0.0001 {
        desired
    } else {
        current
            .lerp(desired, (max_radians / angle).clamp(0.0, 1.0))
            .normalize_or_zero()
    }
}

fn damage_road_vehicles_in_radius(
    spatial_query: &SpatialQuery,
    center: Vec3,
    radius: f32,
    base_damage: f32,
    damage_type: DamageType,
    road_vehicle_q: &mut Query<
        (
            Entity,
            &Transform,
            &mut Health,
            &mut Damageable,
            &NpcRoadVehicle,
        ),
        (
            With<NpcRoadVehicle>,
            Without<Player>,
            Without<EnemyProjectile>,
        ),
    >,
) {
    for (entity, transform, mut health, mut damageable, vehicle) in road_vehicle_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        let dist = center.distance(transform.translation);
        if dist <= radius + vehicle.hit_radius
            && world_line_of_sight(
                spatial_query,
                center,
                transform.translation + Vec3::Y * 0.5,
                Some(entity),
            )
        {
            let falloff_radius = (radius + vehicle.hit_radius).max(0.01);
            let damage = area_damage_falloff(base_damage, dist, falloff_radius).max(1.0);
            let info = DamageInfo::new(damage, damage_type);
            apply_damage(&mut health, &mut damageable, &info);
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

fn closest_living_indexed_player(
    origin: Vec3,
    max_range: f32,
    players: impl Iterator<Item = (Entity, Vec3, u8, bool)>,
) -> Option<(Entity, Vec3, u8, f32)> {
    players
        .filter_map(|(entity, position, index, is_alive)| {
            if !is_alive {
                return None;
            }
            let distance = origin.distance(position);
            (distance <= max_range).then_some((entity, position, index, distance))
        })
        .min_by(|a, b| a.3.total_cmp(&b.3))
}

fn damage_players_in_radius<
    DamageFilter: bevy::ecs::query::QueryFilter,
    PositionFilter: bevy::ecs::query::QueryFilter,
>(
    spatial_query: &SpatialQuery,
    center: Vec3,
    radius: f32,
    damage: f32,
    damage_type: DamageType,
    knockback: f32,
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
        if !world_line_of_sight(
            spatial_query,
            center,
            player_transform.translation + Vec3::Y * 0.7,
            Some(player_entity),
        ) {
            continue;
        }
        let falloff = 1.0 - (dist / radius).clamp(0.0, 0.8);
        // Blast shove pushes radially out from the center, falloff-scaled
        // like the damage (mirrors player-dealt explosion knockback).
        let shove = (player_transform.translation - center).with_y(0.0);
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
                &DamageInfo::new(damage * falloff, damage_type)
                    .with_knockback(knockback * falloff)
                    .with_hit_direction(shove),
                damaged_ev,
                parry_ev,
            );
        }
    }
}

/// EC2 enemy attack volume: resolve an enemy melee strike as a shape
/// intersection on the Player layer (the reserved `EnemyHitbox` collision
/// role) with an optional facing arc and a World-cover check — the same
/// shape as the player's `execute_melee_hit`, pointed the other way. Every
/// player inside the volume is struck (not just the closest). Returns the
/// number of players hit.
#[allow(clippy::too_many_arguments)]
fn execute_enemy_melee_hit<
    DamageFilter: bevy::ecs::query::QueryFilter,
    PositionFilter: bevy::ecs::query::QueryFilter,
>(
    spatial_query: &SpatialQuery,
    origin: Vec3,
    forward: Vec3,
    radius: f32,
    arc_cos: f32,
    damage: f32,
    damage_type: DamageType,
    knockback: f32,
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
) -> usize {
    let hitbox = AvianCollider::sphere(radius.max(0.1));
    let filter = SpatialQueryFilter::from_mask(CollisionProfile::EnemyHitbox.layers().filters);
    let mut candidates =
        spatial_query.shape_intersections(&hitbox, origin, Quat::IDENTITY, &filter);
    candidates.sort_by_key(|entity| entity.to_bits());
    candidates.dedup();

    let forward = forward.with_y(0.0).normalize_or_zero();
    let mut hit_count = 0;
    for candidate in candidates {
        let Ok((player_entity, player_transform, player_index)) = player_pos_q.get(candidate)
        else {
            continue;
        };
        let to_player = (player_transform.translation - origin).with_y(0.0);
        let dist = to_player.length();
        if arc_cos > -1.0
            && dist > 0.01
            && forward.length_squared() > 0.5
            && (to_player / dist).dot(forward) < arc_cos
        {
            continue;
        }
        if !world_line_of_sight(
            spatial_query,
            origin + Vec3::Y * 0.7,
            player_transform.translation + Vec3::Y * 0.7,
            Some(player_entity),
        ) {
            continue;
        }
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
                &DamageInfo::new(damage, damage_type)
                    .with_knockback(knockback)
                    .with_hit_direction(to_player),
                damaged_ev,
                parry_ev,
            );
            hit_count += 1;
        }
    }
    hit_count
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
    knockback: f32,
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
        // Beam/breath shove pushes away from the emitter, falloff-scaled.
        let shove = (to_player / dist).with_y(0.0);
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
                &DamageInfo::new(damage * falloff, damage_type)
                    .with_knockback(knockback * falloff)
                    .with_hit_direction(shove),
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

fn spawn_tank_shell(
    commands: &mut Commands,
    assets: &EnemyAttackAssets,
    tank: &Transform,
    target_position: Vec3,
    damage: f32,
    tank_scale: f32,
) {
    let tank_scale = tank_scale.clamp(0.85, 1.8);
    let muzzle = tank.translation
        + Vec3::Y * 1.08 * tank_scale
        + tank.forward().as_vec3() * 3.9 * tank_scale;
    let target = target_position + Vec3::Y * 0.8;
    let direction = (target - muzzle).normalize_or_zero();
    if direction.length_squared() <= 0.001 {
        return;
    }
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(assets.shell_mesh.clone()),
            material: MeshMaterial3d(assets.shell_mat.clone()),
            transform: Transform::from_translation(muzzle)
                .looking_to(direction, Vec3::Y)
                .with_scale(Vec3::new(0.82, 0.82, 1.35)),
            ..default()
        },
        EnemyProjectile {
            kind: EnemyProjectileKind::Shell,
            damage,
            speed: 32.0,
            direction,
            lifetime: 3.2,
            hit_radius: 0.58,
            splash_radius: 5.5,
        },
    ));
    spawn_enemy_muzzle_flash(commands, assets, muzzle, &assets.shell_mat, 1.55);
}

fn spawn_spider_missile_salvo(
    commands: &mut Commands,
    assets: &EnemyAttackAssets,
    spider: &Transform,
    target_entity: Entity,
    target_position: Vec3,
    total_damage: f32,
) {
    // The wilderness spider visual was authored facing local +Z, while Bevy's
    // `Transform::forward` is -Z. Derive launch axes from the live target so
    // missiles leave the visible front pods regardless of that legacy basis.
    let launch_forward = (target_position - spider.translation)
        .with_y(0.0)
        .normalize_or_zero();
    if launch_forward.length_squared() <= 0.001 {
        return;
    }
    let right = Vec3::new(launch_forward.z, 0.0, -launch_forward.x);
    for (side, lateral_aim) in [(-1.0_f32, -0.32_f32), (1.0, 0.32)] {
        let muzzle =
            spider.translation + Vec3::Y * 3.15 + launch_forward * 3.0 + right * side * 2.35;
        let target = target_position + Vec3::Y * 0.8 + right * lateral_aim;
        let direction = (target - muzzle).normalize_or_zero();
        if direction.length_squared() <= 0.001 {
            continue;
        }
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(assets.fireball_mesh.clone()),
                material: MeshMaterial3d(assets.fire_mat.clone()),
                transform: Transform::from_translation(muzzle)
                    .looking_to(direction, Vec3::Y)
                    .with_scale(Vec3::new(0.72, 0.72, 1.55)),
                ..default()
            },
            EnemyProjectile {
                kind: EnemyProjectileKind::Fireball,
                damage: total_damage * 0.55,
                speed: 18.0,
                direction,
                lifetime: 6.5,
                hit_radius: 1.0,
                splash_radius: 6.0,
            },
            EnemyHomingProjectile::new(target_entity),
        ));
        spawn_enemy_muzzle_flash(commands, assets, muzzle, &assets.fire_mat, 1.25);
    }
}

fn spawn_enemy_muzzle_flash(
    commands: &mut Commands,
    assets: &EnemyAttackAssets,
    muzzle: Vec3,
    material: &Handle<StandardMaterial>,
    scale: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(assets.shell_mesh.clone()),
            material: MeshMaterial3d(material.clone()),
            transform: Transform::from_translation(muzzle).with_scale(Vec3::splat(scale)),
            ..default()
        },
        EnemyAttackVfx { timer: 0.14 },
    ));
}

// ── Attack System ─────────────────────────────────────────────────────────────
fn enemy_attack_system(
    time: Res<Time>,
    mut commands: Commands,
    assets: Res<EnemyAttackAssets>,
    spatial_query: SpatialQuery,
    player_q: Query<(Entity, &Transform, &PlayerIndex), With<Player>>,
    mut enemy_q: Query<
        (
            &Transform,
            &mut Enemy,
            &EnemyStateMachine,
            &Health,
            Option<&FlyingDrone>,
            Option<&MountainSpiderMech>,
            (Option<&DragonBoss>, Option<&RiftBoss>, Option<&MechBoss>),
        ),
        (
            Without<Player>,
            Without<HackedUnit>,
            Without<crate::combat::feedback::Flinch>,
        ),
    >,
    mut player_health_q: ParamSet<(
        Query<&Health, With<Player>>,
        Query<
            (
                &mut crate::combat::damage::Health,
                &mut Damageable,
                &mut PlayerStats,
                &mut crate::components::player::ParryState,
                &crate::components::armor::ArmorSet,
            ),
            With<Player>,
        >,
    )>,
    mut damaged_ev: MessageWriter<PlayerDamagedEvent>,
    mut parry_ev: MessageWriter<PlayerParryEvent>,
) {
    let dt = time.delta_secs();
    let player_targets = {
        let health_q = player_health_q.p0();
        player_q
            .iter()
            .filter_map(|(entity, transform, index)| {
                health_q
                    .get(entity)
                    .ok()
                    .map(|health| (entity, transform.translation, index.0, health.is_alive()))
            })
            .collect::<Vec<_>>()
    };
    for (
        e_transform,
        mut enemy,
        sm,
        health,
        drone,
        mountain_spider,
        (dragon_boss, rift_boss, mech_boss),
    ) in enemy_q.iter_mut()
    {
        if !health.is_alive() {
            continue;
        }
        if drone.is_some() || dragon_boss.is_some() || rift_boss.is_some() || mech_boss.is_some() {
            continue;
        }

        // Siege platforms own real projectiles and never fall through to the
        // giant generic melee sphere. This also clears a stale committed swing
        // if an older session/system armed one before the ranged brain ran.
        if enemy.enemy_type == EnemyType::Tank || mountain_spider.is_some() {
            enemy.attack_windup_timer = None;
            if sm.current != EnemyAIState::Attack || enemy.attack_cooldown_timer > 0.0 {
                continue;
            }
            let Some((target_entity, target_position, _target_index, _distance)) =
                closest_living_indexed_player(
                    e_transform.translation,
                    enemy.config.attack_range,
                    player_targets.iter().copied(),
                )
            else {
                continue;
            };
            if mountain_spider.is_some() {
                spawn_spider_missile_salvo(
                    &mut commands,
                    &assets,
                    e_transform,
                    target_entity,
                    target_position,
                    enemy.scaled_damage(),
                );
            } else {
                spawn_tank_shell(
                    &mut commands,
                    &assets,
                    e_transform,
                    target_position,
                    enemy.scaled_damage(),
                    enemy.difficulty_scale,
                );
            }
            enemy.attack_cooldown_timer = enemy.config.attack_cooldown;
            continue;
        }

        // ── Committed swing: tick the telegraph, then resolve the volume ──
        if let Some(timer) = enemy.attack_windup_timer {
            if sm.current != EnemyAIState::Attack {
                // Target lost mid-windup (fled, died): abort the swing, with a
                // short cooldown so the telegraph can't be re-spammed.
                enemy.attack_windup_timer = None;
                enemy.attack_cooldown_timer = enemy.attack_cooldown_timer.max(0.4);
                continue;
            }
            let timer = timer - dt;
            if timer > 0.0 {
                enemy.attack_windup_timer = Some(timer);
                continue;
            }
            enemy.attack_windup_timer = None;
            // EC2 hitbox producer: the strike is a Player-layer volume around
            // the enemy — every player inside is hit, and World cover blocks
            // it. Whiffing still spends the cooldown: a dodged telegraph is
            // the player's reward.
            // knockback_force is authored on a legacy 100x scale (120-800);
            // world knockback units run ~1-10 (player melee 3-10).
            let mut player_damage_q = player_health_q.p1();
            execute_enemy_melee_hit(
                &spatial_query,
                e_transform.translation,
                e_transform.forward().as_vec3(),
                enemy.config.attack_range,
                -1.0,
                enemy.scaled_damage(),
                DamageType::Kinetic,
                enemy.config.knockback_force / 100.0,
                &mut player_damage_q,
                &player_q,
                &mut damaged_ev,
                &mut parry_ev,
            );
            enemy.attack_cooldown_timer = enemy.config.attack_cooldown;
            continue;
        }

        if sm.current != EnemyAIState::Attack {
            continue;
        }
        if enemy.attack_cooldown_timer > 0.0 {
            continue;
        }
        // Commit only when someone is actually in reach, then telegraph the
        // strike zone for the windup duration before the volume resolves.
        if closest_indexed_player(
            e_transform.translation,
            enemy.config.attack_range,
            &player_q,
        )
        .is_none()
        {
            continue;
        }
        enemy.attack_windup_timer = Some(enemy.config.attack_windup);
        spawn_shockwave_vfx(
            &mut commands,
            &assets,
            e_transform.translation + Vec3::Y * 0.12,
            enemy.config.attack_range,
            enemy.config.attack_windup,
        );
    }
}

// ── Dead Cleanup ──────────────────────────────────────────────────────────────
fn enemy_dead_cleanup(
    mut commands: Commands,
    time: Res<Time>,
    mut dead_q: Query<(Entity, &mut DeadEnemy, Option<&MountainSpiderMech>)>,
    mut wave: ResMut<WaveInfo>,
) {
    let dt = time.delta_secs();
    for (entity, mut dead, mountain_spider) in dead_q.iter_mut() {
        dead.despawn_timer -= dt;
        if dead.despawn_timer <= 0.0 {
            commands.entity(entity).despawn();
            if mountain_spider.is_none() {
                wave.enemy_count = wave.enemy_count.saturating_sub(1);
            }
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
    mut game_rng: ResMut<GameRng>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut killed_ev: MessageReader<EnemyKilledEvent>,
) {
    let rng = game_rng.loot();

    for ev in killed_ev.read() {
        // Tiered drops: tougher enemies drop more, better, and more often —
        // a rift champion is a jackpot, a scout is pocket change.
        let (drop_chance, extra_rolls, quantity_mult, guaranteed_core) =
            match ev.enemy_type.as_str() {
                "Scallarian rift champion" => (1.0_f32, 2usize, 2.0_f32, true),
                "Scallarian siege tank" => (0.9, 1, 1.7, false),
                "dragon brute" => (0.85, 1, 1.5, false),
                "Scallarian spike alien" | "Scallarian invader" => (0.65, 0, 1.0, false),
                _ => (0.45, 0, 0.8, false), // scouts / spy drones
            };

        let rolls = 1 + extra_rolls + usize::from(guaranteed_core);
        for drop_index in 0..rolls {
            // The guaranteed core rides the final roll slot.
            let force_core = guaranteed_core && drop_index == rolls - 1;
            let roll: f32 = rng.gen();
            if !force_core && roll > drop_chance {
                continue;
            }

            let (item_id, quantity, r, g, b): (&'static str, u32, f32, f32, f32) = if force_core {
                ("energy_core", 1, 1.0, 0.8, 0.0)
            } else if roll < 0.10 {
                ("health_pack", 1, 0.2, 1.0, 0.3)
            } else if roll < 0.20 {
                ("armor_shard", 1, 0.3, 0.5, 1.0)
            } else if roll < 0.35 {
                (
                    "plasma_cell",
                    ((rng.gen_range(10..25) as f32 * quantity_mult) as u32).max(1),
                    0.0,
                    0.6,
                    1.0,
                )
            } else if roll < 0.48 {
                (
                    "scrap_metal",
                    ((rng.gen_range(1..4) as f32 * quantity_mult) as u32).max(1),
                    0.6,
                    0.5,
                    0.3,
                )
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
                        ev.position.x + drop_index as f32 * 0.8 - extra_rolls as f32 * 0.4,
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
                    velocity: Vec3::new(
                        (drop_index as f32 - extra_rolls as f32 * 0.5) * 2.4,
                        5.5,
                        0.0,
                    ),
                    age: 0.0,
                },
            ));
        }
    }
}

// ── Automatic Loot Magnet ─────────────────────────────────────────────────────
fn loot_homing_system(
    time: Res<Time>,
    player_q: Query<&Transform, With<Player>>,
    mut loot_q: Query<(&mut Transform, &mut WorldLoot), Without<Player>>,
) {
    let dt = time.delta_secs().min(0.05);
    for (mut transform, mut loot) in loot_q.iter_mut() {
        loot.age += dt;
        let Some(player_transform) = player_q.iter().min_by(|a, b| {
            a.translation
                .distance_squared(transform.translation)
                .total_cmp(&b.translation.distance_squared(transform.translation))
        }) else {
            continue;
        };

        let target = player_transform.translation + Vec3::Y * 0.85;
        let offset = target - transform.translation;
        let distance = offset.length();
        if loot.age < 0.18 {
            // A brief readable prize pop before attraction takes over.
            loot.velocity.y -= 12.0 * dt;
        } else if distance > 0.001 {
            let homing_speed = (12.0 + distance * 0.42).clamp(12.0, 48.0);
            let desired_velocity = offset / distance * homing_speed;
            let steering = 1.0 - (-10.0 * dt).exp();
            loot.velocity = loot.velocity.lerp(desired_velocity, steering);
        }

        transform.translation += loot.velocity * dt;
        transform.rotate_y(5.5 * dt);
    }
}

// ── Loot Pickup ───────────────────────────────────────────────────────────────
fn loot_pickup_system(
    mut commands: Commands,
    player_q: Query<(Entity, &PlayerIndex, &Transform), With<Player>>,
    mut inventory_q: Query<&mut Inventory, With<Player>>,
    mut loot_q: Query<(Entity, &Transform, &mut WorldLoot)>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
    mut loot_ev: MessageWriter<LootCollectedEvent>,
) {
    let item_defs = crate::components::inventory::all_items();

    for (entity, loot_transform, mut loot) in loot_q.iter_mut() {
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
            if leftover == 0 {
                commands.entity(entity).despawn();
            } else {
                // Keep the uncollected remainder alive for another player or
                // for this player after inventory space becomes available.
                loot.quantity = leftover;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robots::creature::CreatureRole;
    use avian3d::prelude::SimpleCollider;

    fn converted_avian_cuboid_size(collider: Collider) -> Vec3 {
        let mut app = App::new();
        app.add_plugins(crate::engine::physics::PhysicsCompatPlugin);
        let entity = app.world_mut().spawn((collider, Transform::default())).id();
        app.world_mut().run_schedule(PreUpdate);
        app.world()
            .get::<AvianCollider>(entity)
            .expect("compatibility collider should convert during PreUpdate")
            .aabb(Vec3::ZERO, Quat::IDENTITY)
            .size()
    }

    #[test]
    fn creature_roles_map_to_escalating_enemy_statlines() {
        assert_eq!(
            creature_enemy_base_type(CreatureRole::Scout),
            EnemyType::Soldier
        );
        assert_eq!(
            creature_enemy_base_type(CreatureRole::Bruiser),
            EnemyType::Heavy
        );
        assert_eq!(
            creature_enemy_base_type(CreatureRole::Artillery),
            EnemyType::SpikeAlien
        );
        assert_eq!(
            creature_enemy_base_type(CreatureRole::Boss),
            EnemyType::Hybrid
        );
        // Non-combat roles still field a baseline fighter rather than panic.
        for role in [
            CreatureRole::Civilian,
            CreatureRole::Ally,
            CreatureRole::Pet,
        ] {
            assert_eq!(creature_enemy_base_type(role), EnemyType::Soldier);
        }
    }

    #[test]
    fn wasteland_drone_variants_are_stronger_and_slower_than_scouts() {
        let position = Vec3::new(3_000.0, 0.0, -2_000.0);
        let mut scout = Enemy::new(EnemyType::Drone, position, 1.0);
        let mut fighter = scout.clone();
        let mut gunship = scout.clone();
        tune_drone_variant(&mut scout, DroneVariant::Scout);
        tune_drone_variant(&mut fighter, DroneVariant::HeavyFighter);
        tune_drone_variant(&mut gunship, DroneVariant::SiegeGunship);

        assert!(fighter.scaled_health() > scout.scaled_health());
        assert!(gunship.scaled_health() > fighter.scaled_health());
        assert!(gunship.scaled_damage() > fighter.scaled_damage());
        let scout_flight = FlyingDrone::for_variant(position, DroneVariant::Scout);
        let fighter_flight = FlyingDrone::for_variant(position, DroneVariant::HeavyFighter);
        let gunship_flight = FlyingDrone::for_variant(position, DroneVariant::SiegeGunship);
        assert!(fighter_flight.speed_multiplier < scout_flight.speed_multiplier);
        assert!(gunship_flight.speed_multiplier < fighter_flight.speed_multiplier);
        assert_eq!(fighter_flight.burst_shots_remaining, 0);
        assert_eq!(fighter_flight.burst_timer, 0.0);
    }

    #[test]
    fn drone_visual_and_hurtbox_share_variant_and_difficulty_scale() {
        for (variant, variant_scale) in [
            (DroneVariant::Scout, 1.0),
            (DroneVariant::HeavyFighter, 1.55),
            (DroneVariant::SiegeGunship, 2.25),
        ] {
            for difficulty in [0.5_f32, 1.0, 1.8, 3.0] {
                let expected = variant_scale * difficulty.clamp(0.85, 1.8);
                let visual_scale = drone_craft_scale(variant, difficulty);
                assert!((visual_scale - expected).abs() < 0.0001);

                let expected_half_extents = Vec3::new(3.95, 0.68, 2.50) * visual_scale;
                assert_eq!(
                    drone_hurtbox_half_extents(visual_scale),
                    expected_half_extents
                );
                let collider = drone_hurtbox(visual_scale);
                assert_eq!(
                    collider,
                    Collider::cuboid(
                        expected_half_extents.x,
                        expected_half_extents.y,
                        expected_half_extents.z
                    )
                );
                let actual_size = converted_avian_cuboid_size(collider);
                assert!(actual_size.abs_diff_eq(expected_half_extents * 2.0, 0.0001));
            }
        }
    }

    #[test]
    fn tank_hurtbox_converts_authored_half_extents_to_avian_full_lengths() {
        for difficulty in [0.5_f32, 1.0, 1.8, 3.0] {
            let expected_half_extents = Vec3::new(1.9, 1.4, 3.2) * difficulty.clamp(0.85, 1.8);
            assert_eq!(tank_hurtbox_half_extents(difficulty), expected_half_extents);
            let actual_size = converted_avian_cuboid_size(tank_hurtbox(difficulty));
            assert!(actual_size.abs_diff_eq(expected_half_extents * 2.0, 0.0001));
        }
    }

    #[test]
    fn siege_targeting_skips_a_dead_nearest_player_for_a_living_teammate() {
        let mut world = World::new();
        let dead_nearest = world.spawn_empty().id();
        let living_farther = world.spawn_empty().id();
        let selected = closest_living_indexed_player(
            Vec3::ZERO,
            60.0,
            [
                (dead_nearest, Vec3::new(0.0, 0.0, 4.0), 0, false),
                (living_farther, Vec3::new(0.0, 0.0, 28.0), 1, true),
            ]
            .into_iter(),
        );

        assert_eq!(
            selected.map(|(entity, _, index, _)| (entity, index)),
            Some((living_farther, 1))
        );
    }

    #[test]
    fn homing_projectile_turn_is_bounded_and_moves_toward_target() {
        let current = Vec3::X;
        let desired = Vec3::Z;
        let max_turn = 0.2;
        let steered = steer_enemy_projectile(current, desired, max_turn);
        let actual_turn = current.dot(steered).clamp(-1.0, 1.0).acos();
        assert!(actual_turn <= max_turn + 0.0001);
        assert!(steered.dot(desired) > current.dot(desired));
        assert!((steered.length() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn tank_root_height_tracks_terrain_with_spawn_scale_clamps() {
        let terrain_y = 37.5;
        assert!((tank_root_y(terrain_y, 1.0) - (terrain_y + 1.18)).abs() < 0.0001);
        assert!(
            (tank_root_y(terrain_y, 0.2) - (terrain_y + TANK_ROOT_CLEARANCE * 0.85)).abs() < 0.0001
        );
        assert!(
            (tank_root_y(terrain_y, 3.0) - (terrain_y + TANK_ROOT_CLEARANCE * 1.8)).abs() < 0.0001
        );
    }
}
