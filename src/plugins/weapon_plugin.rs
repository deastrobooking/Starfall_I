use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use crate::combat_data::{ActiveMelee, MeleeChain, MeleePhase, MoveLibrary};
use crate::components::armor::ArmorSet;
use crate::components::enemy::{DeadEnemy, Enemy};
use crate::components::player::*;
use crate::components::weapon::*;
use crate::components::world::NpcRoadVehicle;
use crate::damage::{
    apply_damage, area_damage_falloff, DamageInfo, DamageType, Damageable, Health,
};
use crate::events::*;
use crate::game_rng::GameRng;
use crate::hacking::HackedUnit;
use crate::hitstop::HitstopState;
use crate::rendering::{EnergyMaterial, EnergyMaterialUniform, EnergyPbrBundle, PbrBundle};
use crate::resources::DungeonCrawlState;
use crate::state::AppState;
use crate::upgrades::UpgradeLedger;

// ── Hit Particle ──────────────────────────────────────────────────────────────
#[derive(Component)]
pub struct HitParticle {
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub velocity: Vec3,
}

#[derive(Component)]
struct PreserveParticleShape(Vec3);

#[derive(Component)]
struct SabreBladeVisual {
    owner: Entity,
}

/// World-space confirmation that a homing projectile has acquired a target.
/// It follows the enemy rather than the camera, so it remains readable in
/// four-player split screen without another per-viewport UI layer.
#[derive(Component)]
struct TargetLockVisual {
    missile: Entity,
}

// ── Projectile Asset Cache ────────────────────────────────────────────────────
#[derive(Resource)]
pub struct ProjectileAssets {
    // Mesh sizes
    pub sphere_sm: Handle<Mesh>,
    pub sphere_md: Handle<Mesh>,
    pub sphere_lg: Handle<Mesh>,
    pub sphere_xl: Handle<Mesh>,
    pub flash_sphere: Handle<Mesh>,
    pub lock_ring: Handle<Mesh>,
    // Base projectile materials
    pub mat_pistol: Handle<StandardMaterial>,
    pub mat_rifle: Handle<StandardMaterial>,
    pub mat_shotgun: Handle<StandardMaterial>,
    pub mat_rocket: Handle<StandardMaterial>,
    pub mat_laser: Handle<StandardMaterial>,
    pub mat_grenade: Handle<StandardMaterial>,
    pub mat_homing_star: Handle<StandardMaterial>,
    pub mat_energy: Handle<StandardMaterial>,
    pub mat_moon_bubble: Handle<StandardMaterial>,
    pub mat_sprite_shot: Handle<StandardMaterial>,
    pub mat_companion: Handle<StandardMaterial>,
    pub mat_melee_flash: Handle<StandardMaterial>,
    pub mat_hit_particle: Handle<StandardMaterial>,
    pub mat_critical_hit: Handle<StandardMaterial>,
    pub mat_missile_lock: Handle<StandardMaterial>,
    pub mat_magic_lock: Handle<StandardMaterial>,
    // Charge blast materials (2-3x emissive of base)
    pub mat_charge_pistol: Handle<StandardMaterial>,
    pub mat_charge_rifle: Handle<StandardMaterial>,
    pub mat_charge_shotgun: Handle<StandardMaterial>,
    pub mat_charge_rocket: Handle<StandardMaterial>,
    pub mat_charge_laser: Handle<StandardMaterial>,
    pub mat_charge_grenade: Handle<StandardMaterial>,
    // VFX
    pub mat_charge_spark: Handle<StandardMaterial>,
    pub mat_muzzle_flash: Handle<StandardMaterial>,
    // Shared custom-shader palette. These five handles are reused by every
    // projectile and transient effect; firing never allocates a material.
    pub energy_plasma: Handle<EnergyMaterial>,
    pub energy_laser: Handle<EnergyMaterial>,
    pub energy_explosive: Handle<EnergyMaterial>,
    pub energy_magic: Handle<EnergyMaterial>,
    pub energy_sabre: Handle<EnergyMaterial>,
}

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct WeaponPlugin;

impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponRanks>()
            .add_systems(Startup, setup_weapon_assets)
            .add_systems(
                Update,
                update_aim_solution_system
                    .before(weapon_fire_system)
                    .before(special_weapon_system)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    apply_ranged_move_defs_system.before(weapon_fire_system),
                    apply_weapon_ranks_system,
                    weapon_select_system,
                    weapon_fire_system,
                    weapon_reload_system,
                    charge_spark_system,
                    special_weapon_system,
                    tracking_missile_system.before(projectile_update_system),
                    sync_target_lock_visual.after(tracking_missile_system),
                    projectile_update_system,
                    melee_combo_system,
                    beam_sabre_update_system,
                    sync_sabre_blade_visual.after(beam_sabre_update_system),
                    hit_particle_spawn_system,
                    critical_impact_spawn_system.after(hit_particle_spawn_system),
                    particle_update_system,
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

const AIM_MAX_DISTANCE: f32 = 120.0;

fn direction_to_aim_point(muzzle: Vec3, aim_point: Vec3, fallback: Vec3) -> Vec3 {
    let direction = (aim_point - muzzle).normalize_or_zero();
    if direction.length_squared() > 0.001 {
        direction
    } else {
        fallback.normalize_or(Vec3::NEG_Z)
    }
}

fn update_aim_solution_system(
    spatial_query: SpatialQuery,
    mut player_q: Query<
        (
            Entity,
            &GlobalTransform,
            &PlayerInput,
            &PlayerCameraRef,
            &PlayerProgression,
            &mut AimSolution,
        ),
        With<Player>,
    >,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    enemy_q: Query<
        (Entity, &GlobalTransform, &Health),
        (With<Enemy>, Without<DeadEnemy>, Without<HackedUnit>),
    >,
) {
    for (player_entity, player_transform, input, camera_ref, progression, mut aim) in
        player_q.iter_mut()
    {
        let upgrades = &progression.upgrades;
        let Ok(camera_transform) = cam_q.get(camera_ref.0) else {
            continue;
        };
        let camera_origin = camera_transform.translation();
        let camera_forward = camera_transform
            .forward()
            .as_vec3()
            .normalize_or(Vec3::NEG_Z);
        let muzzle_origin = star_muzzle_origin(player_transform, camera_forward);
        let filter = SpatialQueryFilter::from_excluded_entities([player_entity]);
        let world_hit = Dir3::new(camera_forward).ok().and_then(|direction| {
            spatial_query.cast_ray(camera_origin, direction, AIM_MAX_DISTANCE, false, &filter)
        });
        let unobstructed_distance = world_hit
            .as_ref()
            .map(|hit| hit.distance)
            .unwrap_or(AIM_MAX_DISTANCE);

        let range = AIM_MAX_DISTANCE + upgrades.gauntlet_aim_range_bonus();
        let base_cone = if input.aim { 0.78 } else { 0.90 };
        let cone_cos = (base_cone - upgrades.gauntlet_aim_cone_relax()).clamp(0.62, 0.96);
        let mut best: Option<(f32, Entity, Vec3)> = None;
        for (entity, transform, health) in enemy_q.iter() {
            if !health.is_alive() {
                continue;
            }
            let target_point = transform.translation() + Vec3::Y * 0.9;
            let offset = target_point - camera_origin;
            let distance = offset.length();
            if distance <= 0.01 || distance > range {
                continue;
            }
            let dot = offset.normalize_or_zero().dot(camera_forward);
            if dot < cone_cos {
                continue;
            }
            let target_direction = Dir3::new(offset.normalize_or_zero()).ok();
            let visible = target_direction.is_some_and(|direction| {
                spatial_query
                    .cast_ray(camera_origin, direction, distance + 0.75, false, &filter)
                    .is_none_or(|hit| hit.entity == entity || hit.distance + 0.75 >= distance)
            });
            if !visible {
                continue;
            }
            let score = dot * 3.0 - distance / range;
            if best.is_none_or(|(best_score, _, _)| score > best_score) {
                best = Some((score, entity, target_point));
            }
        }

        let (target, aim_point, obstructed) = if let Some((_, entity, point)) = best {
            (Some(entity), point, false)
        } else if let Some(hit) = world_hit {
            (
                None,
                camera_origin + camera_forward * hit.distance,
                hit.distance + 0.01 < AIM_MAX_DISTANCE,
            )
        } else {
            (
                None,
                camera_origin + camera_forward * AIM_MAX_DISTANCE,
                false,
            )
        };
        *aim = AimSolution {
            camera_origin,
            muzzle_origin,
            aim_point,
            direction: direction_to_aim_point(muzzle_origin, aim_point, camera_forward),
            target,
            actively_aiming: input.aim,
            obstructed: obstructed && unobstructed_distance < AIM_MAX_DISTANCE,
        };
    }
}

// ── Material helpers ──────────────────────────────────────────────────────────
fn mk_proj_mat(
    materials: &mut Assets<StandardMaterial>,
    r: f32,
    g: f32,
    b: f32,
    er: f32,
    eg: f32,
    eb: f32,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::srgb(r, g, b),
        emissive: LinearRgba::new(er, eg, eb, 1.0),
        unlit: true,
        ..default()
    })
}

fn setup_weapon_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut energy_materials: ResMut<Assets<EnergyMaterial>>,
) {
    let m = &mut *materials;
    commands.insert_resource(ProjectileAssets {
        sphere_sm: meshes.add(Sphere::new(0.08)),
        sphere_md: meshes.add(Sphere::new(0.22)),
        sphere_lg: meshes.add(Sphere::new(0.42)),
        sphere_xl: meshes.add(Sphere::new(0.72)),
        flash_sphere: meshes.add(Sphere::new(0.9)),
        lock_ring: meshes.add(Torus::new(0.82, 1.0)),
        // Base materials — emissive tuned for bloom
        mat_pistol: mk_proj_mat(m, 1.0, 0.95, 0.25, 4.0, 3.0, 0.4),
        mat_rifle: mk_proj_mat(m, 0.15, 0.9, 1.0, 0.4, 3.5, 5.0),
        mat_shotgun: mk_proj_mat(m, 1.0, 0.35, 0.85, 4.0, 0.7, 3.5),
        mat_rocket: mk_proj_mat(m, 0.95, 0.25, 1.0, 3.5, 0.6, 4.0),
        mat_laser: mk_proj_mat(m, 0.45, 1.0, 0.55, 1.5, 5.0, 2.0),
        mat_grenade: mk_proj_mat(m, 0.25, 0.55, 1.0, 0.6, 1.5, 4.0),
        mat_homing_star: mk_proj_mat(m, 1.0, 0.8, 0.15, 5.0, 2.5, 0.4),
        mat_energy: mk_proj_mat(m, 0.2, 1.0, 0.95, 0.5, 4.0, 3.5),
        mat_moon_bubble: mk_proj_mat(m, 0.7, 0.35, 1.0, 2.0, 0.8, 5.0),
        mat_sprite_shot: mk_proj_mat(m, 0.8, 1.0, 0.25, 2.5, 4.0, 0.6),
        mat_companion: mk_proj_mat(m, 1.0, 0.55, 0.2, 4.0, 1.6, 0.4),
        mat_melee_flash: mk_proj_mat(m, 1.0, 0.95, 0.35, 5.0, 3.2, 0.6),
        mat_hit_particle: mk_proj_mat(m, 1.0, 0.85, 0.2, 4.0, 2.5, 0.4),
        mat_critical_hit: mk_proj_mat(m, 1.0, 0.18, 0.72, 9.0, 0.8, 6.0),
        mat_missile_lock: mk_translucent_mat(m, 1.0, 0.72, 0.12, 5.5, 2.2, 0.2),
        mat_magic_lock: mk_translucent_mat(m, 0.28, 0.92, 1.0, 0.8, 4.8, 7.0),
        // Charge blast materials — ~2-3x emissive, near-white hot core
        mat_charge_pistol: mk_proj_mat(m, 1.0, 1.0, 0.6, 10.0, 8.0, 2.0),
        mat_charge_rifle: mk_proj_mat(m, 0.6, 0.95, 1.0, 2.0, 9.0, 12.0),
        mat_charge_shotgun: mk_proj_mat(m, 1.0, 0.7, 1.0, 10.0, 3.0, 9.0),
        mat_charge_rocket: mk_proj_mat(m, 1.0, 0.6, 1.0, 9.0, 2.0, 10.0),
        mat_charge_laser: mk_proj_mat(m, 0.7, 1.0, 0.7, 4.0, 12.0, 5.0),
        mat_charge_grenade: mk_proj_mat(m, 0.6, 0.8, 1.0, 2.0, 5.0, 11.0),
        // VFX
        mat_charge_spark: mk_proj_mat(m, 1.0, 1.0, 0.8, 8.0, 6.0, 2.0),
        mat_muzzle_flash: mk_proj_mat(m, 1.0, 1.0, 1.0, 10.0, 10.0, 6.0),
        energy_plasma: mk_energy_mat(
            &mut energy_materials,
            Vec4::new(0.78, 1.0, 1.0, 1.0),
            Vec4::new(0.05, 0.72, 1.0, 1.0),
            Vec4::new(3.1, 6.4, 4.2, 0.88),
        ),
        energy_laser: mk_energy_mat(
            &mut energy_materials,
            Vec4::new(0.92, 1.0, 0.78, 1.0),
            Vec4::new(0.12, 1.0, 0.42, 1.0),
            Vec4::new(4.4, 7.2, 5.4, 0.90),
        ),
        energy_explosive: mk_energy_mat(
            &mut energy_materials,
            Vec4::new(1.0, 0.96, 0.62, 1.0),
            Vec4::new(1.0, 0.18, 0.04, 1.0),
            Vec4::new(2.2, 5.2, 6.0, 0.92),
        ),
        energy_magic: mk_energy_mat(
            &mut energy_materials,
            Vec4::new(0.92, 0.88, 1.0, 1.0),
            Vec4::new(0.42, 0.12, 1.0, 1.0),
            Vec4::new(3.6, 4.5, 3.0, 0.86),
        ),
        energy_sabre: mk_energy_mat(
            &mut energy_materials,
            Vec4::new(1.0, 1.0, 0.88, 1.0),
            Vec4::new(0.18, 0.78, 1.0, 1.0),
            Vec4::new(5.2, 8.0, 7.0, 0.94),
        ),
    });
}

fn mk_energy_mat(
    materials: &mut Assets<EnergyMaterial>,
    core_color: Vec4,
    edge_color: Vec4,
    motion: Vec4,
) -> Handle<EnergyMaterial> {
    materials.add(EnergyMaterial {
        settings: EnergyMaterialUniform {
            core_color,
            edge_color,
            motion,
        },
    })
}

fn mk_translucent_mat(
    materials: &mut Assets<StandardMaterial>,
    r: f32,
    g: f32,
    b: f32,
    er: f32,
    eg: f32,
    eb: f32,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::srgba(r, g, b, 0.82),
        emissive: LinearRgba::new(er, eg, eb, 1.0),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    })
}

// ── Muzzle origin ─────────────────────────────────────────────────────────────
fn star_muzzle_origin(player_transform: &GlobalTransform, aim_forward: Vec3) -> Vec3 {
    let facing = aim_forward.with_y(0.0).normalize_or_zero();
    let facing = if facing.length_squared() > 0.001 {
        facing
    } else {
        player_transform
            .forward()
            .as_vec3()
            .with_y(0.0)
            .normalize_or_zero()
    };
    player_transform.translation() + Vec3::Y * 0.75 + facing * 0.75
}

fn combat_forward(
    player_transform: &GlobalTransform,
    camera_transform: &GlobalTransform,
    input: &PlayerInput,
    dungeon_active: bool,
) -> Vec3 {
    let player_forward = player_transform
        .forward()
        .as_vec3()
        .with_y(0.0)
        .normalize_or_zero();
    if dungeon_active {
        let input_dir = Vec3::new(input.move_axis.x, 0.0, -input.move_axis.y).normalize_or_zero();
        if input_dir.length_squared() > 0.01 {
            return input_dir;
        }
        if player_forward.length_squared() > 0.01 {
            return player_forward;
        }
    }
    let camera_forward = camera_transform
        .forward()
        .as_vec3()
        .with_y(0.0)
        .normalize_or_zero();
    if camera_forward.length_squared() > 0.01 {
        camera_forward
    } else {
        player_forward
    }
}

fn gauntlet_projectile_damage_type(upgrades: &UpgradeLedger, fallback: DamageType) -> DamageType {
    if upgrades.gauntlet_has_rift() {
        DamageType::Rift
    } else if upgrades.gauntlet_has_electric() {
        DamageType::Electric
    } else if upgrades.gauntlet_has_fire() {
        DamageType::Fire
    } else {
        fallback
    }
}

fn primary_fallback_damage_type(weapon_type: WeaponType, is_explosive: bool) -> DamageType {
    if is_explosive || matches!(weapon_type, WeaponType::Rocket | WeaponType::Grenade) {
        DamageType::Explosive
    } else if weapon_type == WeaponType::Laser {
        DamageType::Laser
    } else {
        DamageType::Plasma
    }
}

// ── Apply weapon ranks ────────────────────────────────────────────────────────
fn apply_weapon_ranks_system(
    mut player_q: Query<(&PlayerProgression, &mut WeaponInventory), With<Player>>,
) {
    for (progression, mut inv) in player_q.iter_mut() {
        for (i, weapon) in inv.slots.iter_mut().enumerate() {
            weapon.rank = progression.weapon_ranks.ranks[i];
        }
    }
}

// ── Apply ranged MoveDefs ─────────────────────────────────────────────────────
/// EC2: sync the data-driven ranged tuning (`MoveLibrary.ranged`, authored in
/// `assets/combat/moves.json`) onto every `WeaponInventory` slot. The
/// `Weapon::new` compile-time stats become fallbacks; the library is the
/// source of truth. Runs when the library (re)loads and for newly spawned
/// inventories, and never touches runtime state (ammo, timers, rank, charge).
fn apply_ranged_move_defs_system(
    library: Res<MoveLibrary>,
    mut inv_q: Query<&mut WeaponInventory>,
) {
    let library_changed = library.is_changed();
    for mut inv in inv_q.iter_mut() {
        if library_changed || inv.is_added() {
            apply_ranged_defs(&library, &mut inv);
        }
    }
}

/// Copy each slot's `RangedMoveDef` balance fields onto the live `Weapon`.
/// Identity/behavior fields (weapon type, automatic trigger, explosive flag,
/// ammo pools) stay on the component; frame-data-style balance moves here.
fn apply_ranged_defs(library: &MoveLibrary, inv: &mut WeaponInventory) {
    for (slot, weapon) in inv.slots.iter_mut().enumerate() {
        let Some(def) = library.ranged_slot(slot) else {
            continue;
        };
        weapon.damage = def.damage;
        weapon.fire_rate = def.fire_rate;
        weapon.speed = def.projectile_speed;
        weapon.spread = def.spread;
        weapon.pellets = def.pellets;
        weapon.explosion_radius = def.explosion_radius;
    }
}

// ── Weapon Select ─────────────────────────────────────────────────────────────
fn weapon_select_system(
    mut player_q: Query<
        (
            &PlayerInput,
            &mut WeaponInventory,
            &mut SpecialWeaponInventory,
        ),
        With<Player>,
    >,
    mut switched_ev: MessageWriter<WeaponSwitchedEvent>,
) {
    for (pi, mut inv, mut specials) in player_q.iter_mut() {
        let prev = inv.active_slot;
        let count = inv.slots.len();

        let mut new_slot = pi.weapon_slot;

        if pi.weapon_next {
            new_slot = Some((prev + 1) % count);
        } else if pi.weapon_prev && prev > 0 {
            new_slot = Some(prev - 1);
        } else if pi.weapon_prev && prev == 0 {
            new_slot = Some(count - 1);
        }

        if let Some(s) = new_slot {
            if s < count {
                specials.active_slot = None;
                inv.active_slot = s;
                if s != prev {
                    switched_ev.write(WeaponSwitchedEvent {
                        weapon_name: inv.active().weapon_type.display_name().to_string(),
                    });
                }
            }
        }
    }
}

// ── Primary Weapon Fire ───────────────────────────────────────────────────────
fn weapon_fire_system(
    mut game_rng: ResMut<GameRng>,
    time: Res<Time>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    library: Res<MoveLibrary>,
    mut player_q: Query<
        (
            Entity,
            &PlayerIndex,
            &mut WeaponInventory,
            &SpecialWeaponInventory,
            &mut PlayerStateMachine,
            &PlayerInput,
            &PlayerCameraRef,
            &AimSolution,
            &ArmorSet,
            &PlayerProgression,
            Option<&ArmCannonUser>,
            Option<&MagicBeamCaster>,
        ),
        With<Player>,
    >,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    mut fired_ev: MessageWriter<WeaponFiredEvent>,
) {
    let dt = time.delta_secs();
    for (
        player_entity,
        player_index,
        mut inv,
        special_inv,
        mut sm,
        pi,
        cam_ref,
        aim,
        armor,
        progression,
        arm_cannon,
        magic_caster,
    ) in player_q.iter_mut()
    {
        let perks = &progression.perks;
        let upgrades = &progression.upgrades;
        let perk_damage_mult = perks.damage_mult();
        // A selected special weapon owns RT until normal weapon cycling or a
        // direct primary slot clears the selection.
        if special_inv.active_slot.is_some() {
            continue;
        }
        let Ok(cam) = cam_q.get(cam_ref.0) else {
            continue;
        };

        // EC2: per-slot projectile lifetime from the move library (legacy
        // hardcoded value was a shared 3.0 s).
        let projectile_lifetime = library
            .ranged_slot(inv.active_slot)
            .map_or(3.0, |def| def.projectile_lifetime);
        let weapon = inv.active_mut();
        weapon.fire_timer = (weapon.fire_timer - dt).max(0.0);

        // ── Charge tracking ──────────────────────────────────────────────────
        let can_charge = arm_cannon.is_some();
        let charge_released = can_charge && weapon.charge_held && !pi.fire;
        if can_charge && pi.fire {
            weapon.charge_progress =
                (weapon.charge_progress + dt / weapon.min_charge_time()).min(1.0);
            weapon.charge_held = true;
        } else if charge_released {
            weapon.charge_held = false;
            if weapon.charge_progress >= 1.0 && weapon.fire_timer <= 0.0 {
                let pos = aim.muzzle_origin;
                let aim_fwd = aim.direction;

                let explosive_weapon =
                    weapon.is_explosive || weapon.weapon_type == WeaponType::Rocket;
                let tech_mult = if explosive_weapon {
                    upgrades.missile_damage_mult()
                } else {
                    upgrades.beam_damage_mult()
                };
                let gauntlet_mult = if explosive_weapon {
                    upgrades.gauntlet_explosive_damage_mult()
                } else {
                    upgrades.gauntlet_energy_damage_mult()
                };
                let charge_dmg = armor.modified_outgoing_damage(
                    weapon.damage
                        * weapon.rank_damage_mult()
                        * perk_damage_mult
                        * tech_mult
                        * gauntlet_mult
                        * weapon.charge_damage_mult(),
                );
                let wt = weapon.weapon_type;
                let charge_radius =
                    weapon.charge_explosion_radius() * upgrades.gauntlet_explosion_radius_mult();
                let base_speed = weapon.speed * upgrades.gauntlet_projectile_speed_mult();
                let damage_type = gauntlet_projectile_damage_type(
                    upgrades,
                    primary_fallback_damage_type(wt, explosive_weapon),
                );
                spawn_charge_blast(
                    game_rng.combat(),
                    &mut commands,
                    &proj_assets,
                    player_entity,
                    pos,
                    aim_fwd,
                    cam.right().as_vec3(),
                    wt,
                    charge_dmg,
                    damage_type,
                    charge_radius,
                    base_speed,
                );

                weapon.fire_timer = weapon.rank_effective_fire_rate() * 3.0;
                weapon.charge_progress = 0.0;
                sm.transition(PlayerState::Attacking);
                fired_ev.write(WeaponFiredEvent);
                continue;
            }
            weapon.charge_progress = 0.0;
        } else if !can_charge {
            weapon.charge_progress = 0.0;
            weapon.charge_held = false;
        }

        // ── Normal fire ───────────────────────────────────────────────────────
        let should_fire = if weapon.automatic {
            pi.fire
        } else {
            pi.fire_just
        };
        if !should_fire || !weapon.can_fire() {
            continue;
        }

        let pos = aim.muzzle_origin;
        let right = cam.right().as_vec3();
        let up = cam.up().as_vec3();
        let aim_fwd = aim.direction;

        let explosive_weapon = weapon.is_explosive || weapon.weapon_type == WeaponType::Rocket;
        let tech_damage_mult = if explosive_weapon {
            upgrades.missile_damage_mult()
        } else {
            upgrades.beam_damage_mult()
        };
        let gauntlet_damage_mult = if explosive_weapon {
            upgrades.gauntlet_explosive_damage_mult()
        } else {
            upgrades.gauntlet_energy_damage_mult()
        };
        let damage = armor.modified_outgoing_damage(
            weapon.damage
                * weapon.rank_damage_mult()
                * perk_damage_mult
                * tech_damage_mult
                * gauntlet_damage_mult,
        );
        let speed = weapon.speed * upgrades.gauntlet_projectile_speed_mult();
        let extra_pellets = if explosive_weapon {
            0
        } else {
            upgrades.gauntlet_extra_pellets()
        };
        let spread_floor = if extra_pellets > 0 { 0.025 } else { 0.0 };
        let spread = weapon.spread.max(spread_floor) * upgrades.gauntlet_spread_mult();
        let pellets = weapon.pellets + extra_pellets;
        let is_explosive = weapon.is_explosive;
        let explosion_radius = weapon.explosion_radius * upgrades.gauntlet_explosion_radius_mult();
        let gravity_affected = weapon.weapon_type == WeaponType::Grenade;
        let stretch = weapon.proj_stretch();
        let visual_profile = weapon.visual_profile();
        let effective_fire_rate = weapon.rank_effective_fire_rate();
        let damage_type = gauntlet_projectile_damage_type(
            upgrades,
            primary_fallback_damage_type(weapon.weapon_type, explosive_weapon),
        );

        let (mesh_h, base_mat_h) = base_proj_handles(weapon.weapon_type, &proj_assets);
        let magic_tracking = magic_caster.is_some() && !explosive_weapon;
        let projectile_stretch = if magic_tracking {
            Vec3::new(stretch.x.max(0.8), stretch.y.max(0.8), stretch.z.max(5.0))
        } else {
            stretch
        };

        weapon.fire_timer = effective_fire_rate;

        for _ in 0..pellets {
            use rand::Rng;
            let rng = game_rng.combat();
            let (sx, sy) = if spread > 0.0 {
                (
                    rng.gen_range(-spread..spread),
                    rng.gen_range(-spread..spread),
                )
            } else {
                (0.0, 0.0)
            };
            let dir = (aim_fwd + right * sx + up * sy).normalize();

            // Orient along travel direction and apply per-weapon stretch
            let proj_transform = Transform::from_translation(pos)
                .looking_to(dir, Vec3::Y)
                .with_scale(projectile_stretch);

            let projectile = Projectile {
                damage,
                damage_type,
                speed,
                direction: dir,
                lifetime: projectile_lifetime,
                is_explosive,
                explosion_radius,
                weapon_type: ProjectileOwner::Player,
                owner: Some(player_entity),
                piercing: false,
                gravity_affected,
                vertical_velocity: if gravity_affected { 0.2 } else { 0.0 },
            };
            let mut projectile_entity = if magic_tracking {
                commands.spawn((
                    EnergyPbrBundle {
                        mesh: Mesh3d(mesh_h.clone()),
                        material: MeshMaterial3d(proj_assets.energy_magic.clone()),
                        transform: proj_transform,
                        ..default()
                    },
                    projectile,
                ))
            } else {
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(mesh_h.clone()),
                        material: MeshMaterial3d(base_mat_h.clone()),
                        transform: proj_transform,
                        ..default()
                    },
                    projectile,
                ))
            };
            if magic_tracking {
                projectile_entity.insert(TrackingMissile::magic_beam(player_index.0));
            }
        }

        spawn_muzzle_flash_scaled(
            &mut commands,
            &proj_assets,
            pos,
            visual_profile.muzzle_scale,
        );
        sm.transition(PlayerState::Attacking);
        fired_ev.write(WeaponFiredEvent);
    }
}

fn base_proj_handles(
    wt: WeaponType,
    assets: &ProjectileAssets,
) -> (Handle<Mesh>, Handle<StandardMaterial>) {
    match wt {
        WeaponType::Pistol => (assets.sphere_sm.clone(), assets.mat_pistol.clone()),
        WeaponType::Rifle => (assets.sphere_sm.clone(), assets.mat_rifle.clone()),
        WeaponType::Shotgun => (assets.sphere_sm.clone(), assets.mat_shotgun.clone()),
        WeaponType::Rocket => (assets.sphere_md.clone(), assets.mat_rocket.clone()),
        WeaponType::Laser => (assets.sphere_sm.clone(), assets.mat_laser.clone()),
        WeaponType::Grenade => (assets.sphere_md.clone(), assets.mat_grenade.clone()),
    }
}

fn charge_energy_handle(wt: WeaponType, assets: &ProjectileAssets) -> Handle<EnergyMaterial> {
    match wt {
        WeaponType::Rocket | WeaponType::Grenade => assets.energy_explosive.clone(),
        WeaponType::Laser => assets.energy_laser.clone(),
        WeaponType::Pistol | WeaponType::Rifle | WeaponType::Shotgun => {
            assets.energy_plasma.clone()
        }
    }
}

fn spawn_muzzle_flash(commands: &mut Commands, assets: &ProjectileAssets, pos: Vec3) {
    spawn_muzzle_flash_scaled(commands, assets, pos, 1.0);
}

fn spawn_muzzle_flash_scaled(
    commands: &mut Commands,
    assets: &ProjectileAssets,
    pos: Vec3,
    scale: f32,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(assets.flash_sphere.clone()),
            material: MeshMaterial3d(assets.mat_muzzle_flash.clone()),
            transform: Transform::from_translation(pos).with_scale(Vec3::splat(0.35 * scale)),
            ..default()
        },
        HitParticle {
            lifetime: 0.06,
            max_lifetime: 0.06,
            velocity: Vec3::ZERO,
        },
    ));
}

fn spawn_charge_blast(
    rng: &mut rand::rngs::StdRng,
    commands: &mut Commands,
    assets: &ProjectileAssets,
    owner: Entity,
    pos: Vec3,
    dir: Vec3,
    right: Vec3,
    wt: WeaponType,
    damage: f32,
    damage_type: DamageType,
    explosion_radius: f32,
    base_speed: f32,
) {
    let mat = charge_energy_handle(wt, assets);

    match wt {
        // Rifle: ultra-fast piercing bolt
        WeaponType::Rifle => {
            commands.spawn((
                EnergyPbrBundle {
                    mesh: Mesh3d(assets.sphere_sm.clone()),
                    material: MeshMaterial3d(mat),
                    transform: Transform::from_translation(pos)
                        .looking_to(dir, Vec3::Y)
                        .with_scale(Vec3::new(0.5, 0.5, 8.0)),
                    ..default()
                },
                Projectile {
                    damage,
                    damage_type,
                    speed: base_speed * 2.2,
                    direction: dir,
                    lifetime: 3.0,
                    is_explosive: false,
                    explosion_radius: 0.0,
                    weapon_type: ProjectileOwner::Player,
                    owner: Some(owner),
                    piercing: true,
                    gravity_affected: false,
                    vertical_velocity: 0.0,
                },
                ChargeBlastTag,
            ));
        }
        // Laser: piercing bolt that explodes on expiry
        WeaponType::Laser => {
            commands.spawn((
                EnergyPbrBundle {
                    mesh: Mesh3d(assets.sphere_sm.clone()),
                    material: MeshMaterial3d(mat),
                    transform: Transform::from_translation(pos)
                        .looking_to(dir, Vec3::Y)
                        .with_scale(Vec3::new(0.45, 0.45, 9.0)),
                    ..default()
                },
                Projectile {
                    damage,
                    damage_type,
                    speed: base_speed * 1.6,
                    direction: dir,
                    lifetime: 2.5,
                    is_explosive: true,
                    explosion_radius,
                    weapon_type: ProjectileOwner::Player,
                    owner: Some(owner),
                    piercing: true,
                    gravity_affected: false,
                    vertical_velocity: 0.0,
                },
                ChargeBlastTag,
            ));
        }
        // Shotgun: nova burst of 18 wide-spread pellets
        WeaponType::Shotgun => {
            use rand::Rng;
            let up = dir.cross(right).normalize_or_zero();
            let pellet_dmg = damage / 14.0;
            for _ in 0..18u32 {
                let sx = rng.gen_range(-0.38f32..0.38);
                let sy = rng.gen_range(-0.38f32..0.38);
                let shot_dir = (dir + right * sx + up * sy).normalize_or_zero();
                commands.spawn((
                    EnergyPbrBundle {
                        mesh: Mesh3d(assets.sphere_sm.clone()),
                        material: MeshMaterial3d(mat.clone()),
                        transform: Transform::from_translation(pos)
                            .looking_to(shot_dir, Vec3::Y)
                            .with_scale(Vec3::new(1.0, 1.0, 1.8)),
                        ..default()
                    },
                    Projectile {
                        damage: pellet_dmg,
                        damage_type,
                        speed: base_speed * 1.1,
                        direction: shot_dir,
                        lifetime: 1.8,
                        is_explosive: false,
                        explosion_radius: 0.0,
                        weapon_type: ProjectileOwner::Player,
                        owner: Some(owner),
                        piercing: false,
                        gravity_affected: false,
                        vertical_velocity: 0.0,
                    },
                    ChargeBlastTag,
                ));
            }
        }
        // All others: large explosive orb
        _ => {
            commands.spawn((
                EnergyPbrBundle {
                    mesh: Mesh3d(assets.sphere_xl.clone()),
                    material: MeshMaterial3d(mat),
                    transform: Transform::from_translation(pos).with_scale(Vec3::splat(1.4)),
                    ..default()
                },
                Projectile {
                    damage,
                    damage_type,
                    speed: base_speed * 1.2,
                    direction: dir,
                    lifetime: 4.5,
                    is_explosive: true,
                    explosion_radius,
                    weapon_type: ProjectileOwner::Player,
                    owner: Some(owner),
                    piercing: false,
                    gravity_affected: false,
                    vertical_velocity: 0.0,
                },
                ChargeBlastTag,
            ));
        }
    }

    // Big white flash for all charge blasts
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(assets.sphere_xl.clone()),
            material: MeshMaterial3d(assets.mat_muzzle_flash.clone()),
            transform: Transform::from_translation(pos).with_scale(Vec3::splat(1.1)),
            ..default()
        },
        HitParticle {
            lifetime: 0.12,
            max_lifetime: 0.12,
            velocity: Vec3::ZERO,
        },
    ));
}

// ── Charge build VFX ─────────────────────────────────────────────────────────
fn charge_spark_system(
    mut game_rng: ResMut<GameRng>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    player_q: Query<(&WeaponInventory, &AimSolution), With<Player>>,
) {
    use rand::Rng;
    let rng = game_rng.cosmetic();

    for (inv, aim) in player_q.iter() {
        let weapon = inv.active();
        if weapon.charge_progress < 0.1 {
            continue;
        }
        let pos = aim.muzzle_origin;

        let count = (weapon.charge_progress * 3.5) as u32 + 1;
        for _ in 0..count {
            let vel = Vec3::new(
                rng.gen_range(-4.0f32..4.0),
                rng.gen_range(1.5f32..6.0),
                rng.gen_range(-4.0f32..4.0),
            );
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(proj_assets.sphere_sm.clone()),
                    material: MeshMaterial3d(proj_assets.mat_charge_spark.clone()),
                    transform: Transform::from_translation(
                        pos + Vec3::new(
                            rng.gen_range(-0.3f32..0.3),
                            rng.gen_range(-0.2f32..0.2),
                            rng.gen_range(-0.3f32..0.3),
                        ),
                    )
                    .with_scale(Vec3::splat(0.3 + weapon.charge_progress * 0.5)),
                    ..default()
                },
                HitParticle {
                    lifetime: 0.22,
                    max_lifetime: 0.22,
                    velocity: vel,
                },
            ));
        }
    }
}

// ── Reload ────────────────────────────────────────────────────────────────────
fn weapon_reload_system(
    mut player_q: Query<(&PlayerInput, &mut WeaponInventory), With<Player>>,
    mut reload_ev: MessageWriter<WeaponReloadedEvent>,
) {
    for (pi, mut inv) in player_q.iter_mut() {
        if pi.reload {
            inv.active_mut().reload();
            reload_ev.write(WeaponReloadedEvent);
        }
    }
}

fn apply_perk_ammo_caps_system(
    mut player_q: Query<
        (
            &mut WeaponInventory,
            &mut SpecialWeaponInventory,
            &PlayerProgression,
        ),
        With<Player>,
    >,
) {
    for (mut weapons, mut specials, progression) in player_q.iter_mut() {
        let ammo_mult = progression.perks.ammo_mult();
        for weapon in weapons.slots.iter_mut() {
            let base_max = Weapon::new(weapon.weapon_type).max_ammo;
            rescale_ammo_cap(&mut weapon.ammo, &mut weapon.max_ammo, base_max, ammo_mult);
        }
        rescale_special_ammo_cap(&mut specials.slot7, ammo_mult);
        rescale_special_ammo_cap(&mut specials.slot8, ammo_mult);
        rescale_special_ammo_cap(&mut specials.slot9, ammo_mult);
        rescale_special_ammo_cap(&mut specials.slot0, ammo_mult);
    }
}

fn rescale_special_ammo_cap(weapon: &mut SpecialWeapon, ammo_mult: f32) {
    let base_max = SpecialWeapon::new(weapon.slot).max_ammo;
    rescale_ammo_cap(&mut weapon.ammo, &mut weapon.max_ammo, base_max, ammo_mult);
}

fn rescale_ammo_cap(current: &mut u32, max: &mut u32, base_max: u32, ammo_mult: f32) {
    let new_max = ((base_max as f32 * ammo_mult).round() as u32).max(1);
    if *max == new_max {
        return;
    }
    let ratio = if *max > 0 {
        *current as f32 / *max as f32
    } else {
        1.0
    };
    *max = new_max;
    *current = ((*max as f32 * ratio).round() as u32).min(*max);
}

// ── Special Energy Tools ──────────────────────────────────────────────────────
fn special_weapon_system(
    mut game_rng: ResMut<GameRng>,
    time: Res<Time>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    mut player_q: Query<
        (
            Entity,
            &PlayerIndex,
            &mut SpecialWeaponInventory,
            &PlayerInput,
            &PlayerCameraRef,
            &AimSolution,
            &ArmorSet,
            &BeamSabre,
            &PlayerProgression,
        ),
        With<Player>,
    >,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    mut fired_ev: MessageWriter<WeaponFiredEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    for (player_entity, player_index, mut inv, pi, cam_ref, aim, armor, sabre, progression) in
        player_q.iter_mut()
    {
        let upgrades = &progression.upgrades;
        let perk_damage_mult = progression.perks.damage_mult();
        inv.slot7.cooldown_timer = (inv.slot7.cooldown_timer - dt).max(0.0);
        inv.slot8.cooldown_timer = (inv.slot8.cooldown_timer - dt).max(0.0);
        inv.slot9.cooldown_timer = (inv.slot9.cooldown_timer - dt).max(0.0);
        inv.slot0.cooldown_timer = (inv.slot0.cooldown_timer - dt).max(0.0);

        if let Some(slot) = pi.special_slot {
            if let Some(selected) = inv.select(slot) {
                let name = selected.name;
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "Selected {name} — RT to fire; RB/D-pad Left returns to primaries"
                    ),
                    duration: 2.2,
                });
            }
            continue;
        }
        let Some(slot) = inv.active_slot else {
            continue;
        };
        if !pi.fire_just || sabre.active {
            continue;
        }
        let Ok(cam) = cam_q.get(cam_ref.0) else {
            continue;
        };

        let fwd = aim.direction;
        let pos = aim.muzzle_origin;
        let armor_damage_mult = armor.modified_outgoing_damage(perk_damage_mult);

        match slot {
            // Slot 7 — Homing Star
            0 => {
                if inv.slot7.can_fire() {
                    let dmg = inv.slot7.effective_damage()
                        * armor_damage_mult
                        * upgrades.missile_damage_mult()
                        * upgrades.gauntlet_explosive_damage_mult();
                    let damage_type =
                        gauntlet_projectile_damage_type(upgrades, DamageType::Explosive);
                    inv.slot7.cooldown_timer = inv.slot7.cooldown;
                    commands.spawn((
                        EnergyPbrBundle {
                            mesh: Mesh3d(proj_assets.sphere_md.clone()),
                            material: MeshMaterial3d(proj_assets.energy_explosive.clone()),
                            transform: Transform::from_translation(pos),
                            ..default()
                        },
                        Projectile {
                            damage: dmg,
                            damage_type,
                            speed: 35.0 * upgrades.gauntlet_projectile_speed_mult(),
                            direction: fwd,
                            lifetime: 5.0,
                            is_explosive: true,
                            explosion_radius: 5.0 * upgrades.gauntlet_explosion_radius_mult(),
                            weapon_type: ProjectileOwner::HomingStar,
                            owner: Some(player_entity),
                            piercing: false,
                            gravity_affected: false,
                            vertical_velocity: 0.0,
                        },
                        TrackingMissile::new(player_index.0),
                    ));
                    spawn_muzzle_flash(&mut commands, &proj_assets, pos);
                    fired_ev.write(WeaponFiredEvent);
                    msg_ev.write(UiMessageEvent {
                        text: "Homing Star! [unlimited]".to_string(),
                        duration: 1.5,
                    });
                } else {
                    msg_ev.write(UiMessageEvent {
                        text: "Homing Star recharging!".to_string(),
                        duration: 1.0,
                    });
                }
            }
            // Slot 8 — Tri-Star Burst
            1 => {
                if inv.slot8.can_fire() {
                    let dmg = inv.slot8.effective_damage()
                        * armor_damage_mult
                        * upgrades.beam_damage_mult()
                        * upgrades.gauntlet_energy_damage_mult();
                    let damage_type = gauntlet_projectile_damage_type(upgrades, DamageType::Plasma);
                    inv.slot8.cooldown_timer = inv.slot8.cooldown;
                    use rand::Rng;
                    let rng = game_rng.combat();
                    let right = cam.right().as_vec3();
                    let up = cam.up().as_vec3();
                    let burst_count = 3 + upgrades.gauntlet_extra_pellets();
                    for _ in 0..burst_count {
                        let sx = rng.gen_range(-0.05f32..0.05);
                        let sy = rng.gen_range(-0.05f32..0.05);
                        let dir = (fwd + right * sx + up * sy).normalize();
                        commands.spawn((
                            EnergyPbrBundle {
                                mesh: Mesh3d(proj_assets.sphere_sm.clone()),
                                material: MeshMaterial3d(proj_assets.energy_plasma.clone()),
                                transform: Transform::from_translation(pos)
                                    .looking_to(dir, Vec3::Y)
                                    .with_scale(Vec3::new(0.8, 0.8, 2.5)),
                                ..default()
                            },
                            Projectile {
                                damage: dmg / burst_count as f32,
                                damage_type,
                                speed: 60.0 * upgrades.gauntlet_projectile_speed_mult(),
                                direction: dir,
                                lifetime: 2.0,
                                is_explosive: false,
                                explosion_radius: 0.0,
                                weapon_type: ProjectileOwner::TriStarBurst,
                                owner: Some(player_entity),
                                piercing: true,
                                gravity_affected: false,
                                vertical_velocity: 0.0,
                            },
                        ));
                    }
                    spawn_muzzle_flash(&mut commands, &proj_assets, pos);
                    fired_ev.write(WeaponFiredEvent);
                    msg_ev.write(UiMessageEvent {
                        text: "Tri-Star Burst! [unlimited]".to_string(),
                        duration: 1.5,
                    });
                } else {
                    msg_ev.write(UiMessageEvent {
                        text: "Tri-Star Burst recharging!".to_string(),
                        duration: 1.0,
                    });
                }
            }
            // Slot 9 — Moon Bubble
            2 => {
                if inv.slot9.can_fire() {
                    let dmg = inv.slot9.effective_damage()
                        * armor_damage_mult
                        * upgrades.missile_damage_mult()
                        * upgrades.gauntlet_explosive_damage_mult();
                    let damage_type =
                        gauntlet_projectile_damage_type(upgrades, DamageType::Explosive);
                    inv.slot9.cooldown_timer = inv.slot9.cooldown;
                    commands.spawn((
                        PbrBundle {
                            mesh: Mesh3d(proj_assets.sphere_lg.clone()),
                            material: MeshMaterial3d(proj_assets.mat_moon_bubble.clone()),
                            transform: Transform::from_translation(pos),
                            ..default()
                        },
                        Projectile {
                            damage: dmg,
                            damage_type,
                            speed: 12.0 * upgrades.gauntlet_projectile_speed_mult(),
                            direction: fwd,
                            lifetime: 3.5,
                            is_explosive: true,
                            explosion_radius: 12.0 * upgrades.gauntlet_explosion_radius_mult(),
                            weapon_type: ProjectileOwner::MoonBubble,
                            owner: None,
                            piercing: false,
                            gravity_affected: true,
                            vertical_velocity: 0.1,
                        },
                    ));
                    spawn_muzzle_flash(&mut commands, &proj_assets, pos);
                    fired_ev.write(WeaponFiredEvent);
                    msg_ev.write(UiMessageEvent {
                        text: "Moon Bubble! [unlimited]".to_string(),
                        duration: 1.5,
                    });
                } else {
                    msg_ev.write(UiMessageEvent {
                        text: "Moon Bubble recharging!".to_string(),
                        duration: 1.0,
                    });
                }
            }
            // Slot 0 — Sprite Turret
            3 => {
                if inv.slot0.can_fire() {
                    let dmg = inv.slot0.effective_damage()
                        * armor_damage_mult
                        * upgrades.turret_damage_mult()
                        * upgrades.gauntlet_energy_damage_mult();
                    let damage_type = gauntlet_projectile_damage_type(upgrades, DamageType::Plasma);
                    inv.slot0.cooldown_timer = inv.slot0.cooldown;
                    use rand::Rng;
                    let rng = game_rng.combat();
                    let right = cam.right().as_vec3();
                    let up = cam.up().as_vec3();
                    let burst_count = 5 + upgrades.gauntlet_extra_pellets();
                    for _ in 0..burst_count {
                        let sx = rng.gen_range(-0.08f32..0.08);
                        let sy = rng.gen_range(-0.08f32..0.08);
                        let dir = (fwd + right * sx + up * sy).normalize();
                        commands.spawn((
                            EnergyPbrBundle {
                                mesh: Mesh3d(proj_assets.sphere_sm.clone()),
                                material: MeshMaterial3d(proj_assets.energy_magic.clone()),
                                transform: Transform::from_translation(pos)
                                    .looking_to(dir, Vec3::Y)
                                    .with_scale(Vec3::new(0.8, 0.8, 2.0)),
                                ..default()
                            },
                            Projectile {
                                damage: dmg / burst_count as f32,
                                damage_type,
                                speed: 45.0 * upgrades.gauntlet_projectile_speed_mult(),
                                direction: dir,
                                lifetime: 3.0,
                                is_explosive: false,
                                explosion_radius: 0.0,
                                weapon_type: ProjectileOwner::SpriteTurret,
                                owner: Some(player_entity),
                                piercing: false,
                                gravity_affected: false,
                                vertical_velocity: 0.0,
                            },
                        ));
                    }
                    spawn_muzzle_flash(&mut commands, &proj_assets, pos);
                    fired_ev.write(WeaponFiredEvent);
                    msg_ev.write(UiMessageEvent {
                        text: "Sprite Turret! [unlimited]".to_string(),
                        duration: 1.5,
                    });
                } else {
                    msg_ev.write(UiMessageEvent {
                        text: "Sprite Turret recharging!".to_string(),
                        duration: 1.0,
                    });
                }
            }
            _ => {}
        }
    }
}

// ── Tracking Missile ──────────────────────────────────────────────────────────
fn tracking_missile_system(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<ProjectileAssets>,
    mut missile_q: Query<
        (&mut Transform, &mut Projectile, &mut TrackingMissile),
        With<TrackingMissile>,
    >,
    target_q: Query<
        (Entity, &GlobalTransform, &Health),
        (With<Enemy>, Without<Projectile>, Without<HackedUnit>),
    >,
) {
    let dt = time.delta_secs();
    for (mut transform, mut projectile, mut missile) in missile_q.iter_mut() {
        missile.reacquire_timer = (missile.reacquire_timer - dt).max(0.0);
        missile.trail_timer -= dt;

        let target_position = missile.target.and_then(|entity| {
            target_q
                .get(entity)
                .ok()
                .filter(|(_, _, health)| health.is_alive())
                .map(|(_, target_transform, _)| target_transform.translation() + Vec3::Y * 0.9)
        });

        let target_position = if target_position.is_some() {
            target_position
        } else if missile.reacquire_timer <= 0.0 {
            missile.reacquire_timer = 0.16;
            let acquired = acquire_tracking_target(
                transform.translation,
                projectile.direction,
                missile.acquisition_range,
                missile.acquisition_cone_cos,
                target_q
                    .iter()
                    .filter(|(_, _, health)| health.is_alive())
                    .map(|(entity, target_transform, _)| {
                        (entity, target_transform.translation() + Vec3::Y * 0.9)
                    }),
            );
            missile.target = acquired.map(|(entity, _)| entity);
            acquired.map(|(_, position)| position)
        } else {
            None
        };

        if let Some(target_position) = target_position {
            let desired = (target_position - transform.translation).normalize_or_zero();
            projectile.direction = steer_toward_direction(
                projectile.direction,
                desired,
                missile.turn_rate_radians * dt,
            );
            transform.rotation = Transform::IDENTITY
                .looking_to(projectile.direction, Vec3::Y)
                .rotation;
        }

        if missile.trail_timer <= 0.0 {
            missile.trail_timer = 0.055;
            let transform =
                Transform::from_translation(transform.translation - projectile.direction * 0.55)
                    .with_scale(if missile.magic_beam {
                        Vec3::new(0.75, 0.75, 3.2)
                    } else {
                        Vec3::splat(1.35)
                    });
            let particle = HitParticle {
                lifetime: 0.24,
                max_lifetime: 0.24,
                velocity: -projectile.direction * 1.8 + Vec3::Y * 0.35,
            };
            let trail_scale = transform.scale;
            commands.spawn((
                EnergyPbrBundle {
                    mesh: Mesh3d(assets.sphere_sm.clone()),
                    material: MeshMaterial3d(if missile.magic_beam {
                        assets.energy_magic.clone()
                    } else {
                        assets.energy_explosive.clone()
                    }),
                    transform,
                    ..default()
                },
                particle,
                PreserveParticleShape(trail_scale),
            ));
        }
    }
}

fn sync_target_lock_visual(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<ProjectileAssets>,
    missile_q: Query<(Entity, &TrackingMissile)>,
    target_q: Query<&GlobalTransform, (With<Enemy>, Without<TargetLockVisual>)>,
    mut visual_q: Query<(Entity, &TargetLockVisual, &mut Transform), Without<Enemy>>,
) {
    let mut represented = Vec::new();
    for (visual_entity, visual, mut transform) in visual_q.iter_mut() {
        let Ok((_, missile)) = missile_q.get(visual.missile) else {
            commands.entity(visual_entity).despawn();
            continue;
        };
        let Some(target) = missile.target else {
            commands.entity(visual_entity).despawn();
            continue;
        };
        let Ok(target_transform) = target_q.get(target) else {
            commands.entity(visual_entity).despawn();
            continue;
        };

        represented.push(visual.missile);
        let pulse = 1.0 + (time.elapsed_secs() * 8.0 + missile.owner_player as f32).sin() * 0.12;
        transform.translation = target_transform.translation() + Vec3::Y * 1.05;
        transform.rotation *= Quat::from_rotation_y(time.delta_secs() * 2.8);
        transform.scale = Vec3::splat(pulse * if missile.magic_beam { 1.35 } else { 1.15 });
    }

    for (missile_entity, missile) in missile_q.iter() {
        if represented.contains(&missile_entity) {
            continue;
        }
        let Some(target) = missile.target else {
            continue;
        };
        let Ok(target_transform) = target_q.get(target) else {
            continue;
        };
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(assets.lock_ring.clone()),
                material: MeshMaterial3d(if missile.magic_beam {
                    assets.mat_magic_lock.clone()
                } else {
                    assets.mat_missile_lock.clone()
                }),
                transform: Transform::from_translation(target_transform.translation() + Vec3::Y),
                ..default()
            },
            TargetLockVisual {
                missile: missile_entity,
            },
        ));
    }
}

fn acquire_tracking_target(
    origin: Vec3,
    forward: Vec3,
    range: f32,
    cone_cos: f32,
    targets: impl Iterator<Item = (Entity, Vec3)>,
) -> Option<(Entity, Vec3)> {
    let aim = forward.normalize_or_zero();
    targets
        .filter_map(|(entity, position)| {
            let offset = position - origin;
            let distance = offset.length();
            if distance <= 0.01 || distance > range {
                return None;
            }
            let alignment = aim.dot(offset / distance);
            if alignment < cone_cos {
                return None;
            }
            // Prefer what the player aimed at, while mildly favoring nearer
            // threats when two targets overlap in the reticle.
            let score = alignment * 2.0 - distance / range;
            Some((entity, position, score))
        })
        .max_by(|(_, _, a), (_, _, b)| a.total_cmp(b))
        .map(|(entity, position, _)| (entity, position))
}

fn steer_toward_direction(current: Vec3, desired: Vec3, max_radians: f32) -> Vec3 {
    let current = current.normalize_or_zero();
    let desired = desired.normalize_or_zero();
    if current.length_squared() <= 0.01 {
        return desired;
    }
    if desired.length_squared() <= 0.01 {
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

fn segment_point_distance_squared(start: Vec3, end: Vec3, point: Vec3) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return start.distance_squared(point);
    }
    let t = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    (start + segment * t).distance_squared(point)
}

// ── Projectile Update ─────────────────────────────────────────────────────────
fn projectile_update_system(
    mut commands: Commands,
    time: Res<Time>,
    mut proj_q: Query<(
        Entity,
        &mut Transform,
        &mut Projectile,
        Option<&ChargeBlastTag>,
    )>,
    mut enemy_q: Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        (
            With<Enemy>,
            Without<Projectile>,
            Without<HackedUnit>,
            Without<NpcRoadVehicle>,
        ),
    >,
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
            Without<Projectile>,
            Without<Enemy>,
            Without<HackedUnit>,
        ),
    >,
    mut enemy_damaged_ev: MessageWriter<EnemyDamagedEvent>,
    mut enemy_killed_ev: MessageWriter<EnemyKilledEvent>,
    mut impact_ev: MessageWriter<CombatImpactEvent>,
) {
    let dt = time.delta_secs();

    for (proj_entity, mut proj_transform, mut proj, charge_blast) in proj_q.iter_mut() {
        let is_critical = charge_blast.is_some();
        let previous_position = proj_transform.translation;
        proj_transform.translation += proj.direction * proj.speed * dt;

        if proj.gravity_affected {
            proj.vertical_velocity -= 9.8 * dt;
            proj_transform.translation.y += proj.vertical_velocity * dt;
        }

        proj.lifetime -= dt;
        if proj.lifetime <= 0.0 {
            if proj.is_explosive {
                explode(
                    &proj_transform.translation,
                    proj.explosion_radius,
                    proj.damage,
                    proj.damage_type,
                    is_critical,
                    &mut enemy_q,
                    &mut enemy_damaged_ev,
                    &mut enemy_killed_ev,
                    &mut impact_ev,
                );
                damage_road_vehicles_in_radius(
                    &proj_transform.translation,
                    proj.explosion_radius,
                    proj.damage,
                    proj.damage_type,
                    &mut road_vehicle_q,
                );
            }
            commands.entity(proj_entity).despawn();
            continue;
        }

        if proj_transform.translation.y < 0.0 {
            if proj.is_explosive {
                explode(
                    &proj_transform.translation,
                    proj.explosion_radius,
                    proj.damage,
                    proj.damage_type,
                    is_critical,
                    &mut enemy_q,
                    &mut enemy_damaged_ev,
                    &mut enemy_killed_ev,
                    &mut impact_ev,
                );
                damage_road_vehicles_in_radius(
                    &proj_transform.translation,
                    proj.explosion_radius,
                    proj.damage,
                    proj.damage_type,
                    &mut road_vehicle_q,
                );
            }
            commands.entity(proj_entity).despawn();
            continue;
        }

        let mut hit = false;
        let mut explosion: Option<(Vec3, f32, f32, DamageType)> = None;

        for (e_entity, e_transform, mut e_health, mut e_damageable, enemy) in enemy_q.iter_mut() {
            if !e_health.is_alive() {
                continue;
            }
            let target_center = e_transform.translation + Vec3::Y * 0.9;
            if segment_point_distance_squared(
                previous_position,
                proj_transform.translation,
                target_center,
            ) < 1.75_f32.powi(2)
            {
                if proj.is_explosive {
                    explosion = Some((
                        proj_transform.translation,
                        proj.explosion_radius,
                        proj.damage,
                        proj.damage_type,
                    ));
                    hit = true;
                    break;
                } else {
                    let push = (e_transform.translation - proj_transform.translation)
                        .with_y(0.0)
                        .normalize_or_zero()
                        + Vec3::Y * 0.2;
                    let mut info = DamageInfo::new(proj.damage, proj.damage_type)
                        .with_knockback(2.2)
                        .with_hit_direction(push);
                    if is_critical {
                        info = info.critical();
                    }
                    let result = apply_damage(&mut e_health, &mut e_damageable, &info);
                    enemy_damaged_ev.write(EnemyDamagedEvent {
                        entity: e_entity,
                        damage: result.damage_amount,
                        position: e_transform.translation,
                    });
                    impact_ev.write(CombatImpactEvent {
                        position: e_transform.translation,
                        damage: result.damage_amount,
                        damage_type: proj.damage_type,
                        is_critical: result.was_critical,
                    });
                    if result.was_killed {
                        enemy_killed_ev.write(EnemyKilledEvent {
                            enemy_type: enemy.enemy_type.as_str().to_string(),
                            credits: enemy.config.credits,
                            experience: enemy.config.experience_value,
                            position: e_transform.translation,
                        });
                    }
                }
                if !proj.piercing {
                    hit = true;
                    break;
                }
            }
        }

        if !hit {
            for (_, v_transform, mut v_health, mut v_damageable, vehicle) in
                road_vehicle_q.iter_mut()
            {
                if !v_health.is_alive() {
                    continue;
                }
                if segment_point_distance_squared(
                    previous_position,
                    proj_transform.translation,
                    v_transform.translation,
                ) < vehicle.hit_radius.powi(2)
                {
                    if proj.is_explosive {
                        explosion = Some((
                            proj_transform.translation,
                            proj.explosion_radius,
                            proj.damage,
                            proj.damage_type,
                        ));
                    } else {
                        let info = DamageInfo::new(proj.damage, proj.damage_type);
                        apply_damage(&mut v_health, &mut v_damageable, &info);
                    }
                    if !proj.piercing {
                        hit = true;
                        break;
                    }
                }
            }
        }

        if let Some((pos, radius, dmg, damage_type)) = explosion {
            explode(
                &pos,
                radius,
                dmg,
                damage_type,
                is_critical,
                &mut enemy_q,
                &mut enemy_damaged_ev,
                &mut enemy_killed_ev,
                &mut impact_ev,
            );
            damage_road_vehicles_in_radius(&pos, radius, dmg, damage_type, &mut road_vehicle_q);
        }
        if hit {
            commands.entity(proj_entity).despawn();
        }
    }
}

fn explode(
    center: &Vec3,
    radius: f32,
    base_damage: f32,
    damage_type: DamageType,
    is_critical: bool,
    enemy_q: &mut Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        (
            With<Enemy>,
            Without<Projectile>,
            Without<HackedUnit>,
            Without<NpcRoadVehicle>,
        ),
    >,
    damaged_ev: &mut MessageWriter<EnemyDamagedEvent>,
    killed_ev: &mut MessageWriter<EnemyKilledEvent>,
    impact_ev: &mut MessageWriter<CombatImpactEvent>,
) {
    for (e_entity, e_transform, mut e_health, mut e_damageable, enemy) in enemy_q.iter_mut() {
        if !e_health.is_alive() {
            continue;
        }
        let dist = center.distance(e_transform.translation);
        if dist <= radius {
            let damage = area_damage_falloff(base_damage, dist, radius).max(1.0);
            let blast = (e_transform.translation - *center)
                .with_y(0.0)
                .normalize_or_zero()
                + Vec3::Y * 0.35;
            // Blast knockback falls off with distance like the damage does.
            let force = 4.5 * (1.0 - (dist / radius).clamp(0.0, 1.0)) + 1.0;
            let mut info = DamageInfo::new(damage, damage_type)
                .with_knockback(force)
                .with_hit_direction(blast);
            if is_critical {
                info = info.critical();
            }
            let result = apply_damage(&mut e_health, &mut e_damageable, &info);
            damaged_ev.write(EnemyDamagedEvent {
                entity: e_entity,
                damage: result.damage_amount,
                position: e_transform.translation,
            });
            impact_ev.write(CombatImpactEvent {
                position: e_transform.translation,
                damage: result.damage_amount,
                damage_type,
                is_critical: result.was_critical,
            });
            if result.was_killed {
                killed_ev.write(EnemyKilledEvent {
                    enemy_type: enemy.enemy_type.as_str().to_string(),
                    credits: enemy.config.credits,
                    experience: enemy.config.experience_value,
                    position: e_transform.translation,
                });
            }
        }
    }
}

fn damage_road_vehicles_in_radius(
    center: &Vec3,
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
            Without<Projectile>,
            Without<Enemy>,
            Without<HackedUnit>,
        ),
    >,
) {
    for (_, transform, mut health, mut damageable, vehicle) in road_vehicle_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        let dist = center.distance(transform.translation);
        if dist <= radius + vehicle.hit_radius {
            let damage =
                area_damage_falloff(base_damage, dist, radius + vehicle.hit_radius).max(1.0);
            let info = DamageInfo::new(damage, damage_type);
            apply_damage(&mut health, &mut damageable, &info);
        }
    }
}

// ── Melee Combo ───────────────────────────────────────────────────────────────
fn melee_combo_system(
    time: Res<Time>,
    library: Res<MoveLibrary>,
    mut hitstop: ResMut<HitstopState>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    dungeon: Res<DungeonCrawlState>,
    mut player_q: Query<
        (
            &GlobalTransform,
            &mut MeleeCombo,
            &mut PlayerStateMachine,
            &PlayerInput,
            &PlayerCameraRef,
            &ArmorSet,
            &PlayerProgression,
        ),
        With<Player>,
    >,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    mut enemy_q: Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        Without<HackedUnit>,
    >,
    mut combo_ev: MessageWriter<ComboHitEvent>,
    mut finished_ev: MessageWriter<ComboFinishedEvent>,
    mut damaged_ev: MessageWriter<EnemyDamagedEvent>,
    mut killed_ev: MessageWriter<EnemyKilledEvent>,
) {
    let dt = time.delta_secs();
    for (player_transform, mut combo, mut sm, pi, cam_ref, armor, progression) in
        player_q.iter_mut()
    {
        let upgrades = &progression.upgrades;
        let Ok(cam) = cam_q.get(cam_ref.0) else {
            continue;
        };

        combo.light_timer = (combo.light_timer - dt).max(0.0);
        combo.heavy_timer = (combo.heavy_timer - dt).max(0.0);

        if pi.melee_light {
            combo.buffered_light = true;
        }
        if pi.melee_heavy {
            combo.buffered_heavy = true;
        }

        if combo.light_timer <= 0.0 {
            combo.light_index = 0;
        }
        if combo.heavy_timer <= 0.0 {
            combo.heavy_index = 0;
        }

        let cam_fwd = combat_forward(player_transform, cam, pi, dungeon.active);
        let cam_pos = star_muzzle_origin(player_transform, cam_fwd);
        let armor_damage_mult = armor.modified_outgoing_damage(1.0);
        let blade_rank = upgrades.blade_boot_rank();
        let blade_damage_mult = upgrades.blade_boot_melee_damage_mult();
        let blade_reach_bonus = upgrades.blade_boot_reach_bonus();
        let melee_damage_type = if blade_rank > 0 {
            DamageType::Laser
        } else {
            DamageType::Melee
        };

        // Per-chain reach/arc tuning (data-adjacent, chain-wide constants).
        let reach = |chain: MeleeChain| -> (f32, f32, f32) {
            match chain {
                MeleeChain::Light => (
                    (if dungeon.active { 4.1 } else { 3.0 }) + blade_reach_bonus,
                    (if dungeon.active { 2.1 } else { 2.5 }) + blade_reach_bonus * 0.5,
                    if dungeon.active { -0.20 } else { 0.15 },
                ),
                MeleeChain::Heavy => (
                    (if dungeon.active { 5.7 } else { 4.5 }) + blade_reach_bonus * 1.3,
                    (if dungeon.active { 2.2 } else { 2.0 }) + blade_reach_bonus * 0.6,
                    if dungeon.active { -0.35 } else { 0.05 },
                ),
            }
        };

        // ── Frame-data phase machine ──────────────────────────────────────────
        if let Some(mut active) = combo.active {
            let Some(def) = library.get(active.chain, active.index).cloned() else {
                combo.active = None;
                continue;
            };
            active.timer -= dt;
            match active.phase {
                MeleePhase::Startup => {
                    if active.timer <= 0.0 {
                        // The strike lands at the start of the active window.
                        let damage = def.damage
                            * combo.damage_multiplier
                            * armor_damage_mult
                            * blade_damage_mult;
                        let (radius, offset, arc_cos) = reach(active.chain);
                        execute_melee_hit(
                            cam_pos,
                            cam_fwd,
                            radius,
                            offset,
                            arc_cos,
                            damage,
                            melee_damage_type,
                            def.knockback,
                            &mut enemy_q,
                            &mut damaged_ev,
                            &mut killed_ev,
                        );
                        spawn_melee_flash(&mut commands, &proj_assets, cam_pos + cam_fwd * 2.5);
                        hitstop.remaining = hitstop.remaining.max(def.hitstop);

                        let chain_name = match active.chain {
                            MeleeChain::Light => "Light",
                            MeleeChain::Heavy => "Heavy",
                        };
                        combo_ev.write(ComboHitEvent {
                            combo_name: chain_name.to_string(),
                            attack_name: def.name.clone(),
                            combo_index: active.index,
                        });

                        // Advance the chain and refresh its follow-up window.
                        let len = library.chain_len(active.chain).max(1);
                        match active.chain {
                            MeleeChain::Light => {
                                combo.light_index = (active.index + 1) % len;
                                combo.light_timer = 1.5;
                                if combo.light_index == 0 {
                                    finished_ev.write(ComboFinishedEvent {
                                        combo_name: "Light".to_string(),
                                    });
                                }
                            }
                            MeleeChain::Heavy => {
                                combo.heavy_index = (active.index + 1) % len;
                                combo.heavy_timer = 2.0;
                                if combo.heavy_index == 0 {
                                    finished_ev.write(ComboFinishedEvent {
                                        combo_name: "Heavy".to_string(),
                                    });
                                }
                            }
                        }
                        active.phase = MeleePhase::Active;
                        active.timer = def.active;
                    }
                    combo.active = Some(active);
                }
                MeleePhase::Active => {
                    if active.timer <= 0.0 {
                        active.phase = MeleePhase::Recovery;
                        active.timer = def.recovery;
                    }
                    combo.active = Some(active);
                }
                MeleePhase::Recovery => {
                    let elapsed = (def.recovery - active.timer).max(0.0);
                    let can_cancel = elapsed >= def.cancel_after;
                    if can_cancel && (combo.buffered_light || combo.buffered_heavy) {
                        // Cancel window: chain straight into the buffered move.
                        let chain = if combo.buffered_light {
                            MeleeChain::Light
                        } else {
                            MeleeChain::Heavy
                        };
                        combo.buffered_light = false;
                        combo.buffered_heavy = false;
                        combo.active = start_melee_move(&library, &mut combo, chain, &mut sm);
                    } else if active.timer <= 0.0 {
                        combo.active = None;
                        sm.transition(PlayerState::Idle);
                    } else {
                        combo.active = Some(active);
                    }
                }
            }
            continue;
        }

        // ── Idle: start a buffered attack ────────────────────────────────────
        let do_light = combo.buffered_light;
        let do_heavy = combo.buffered_heavy;
        combo.buffered_light = false;
        combo.buffered_heavy = false;

        if do_light {
            combo.active = start_melee_move(&library, &mut combo, MeleeChain::Light, &mut sm);
        } else if do_heavy {
            combo.active = start_melee_move(&library, &mut combo, MeleeChain::Heavy, &mut sm);
        }
    }
}

/// Begin a move's startup phase from the chain's current index.
fn start_melee_move(
    library: &MoveLibrary,
    combo: &mut MeleeCombo,
    chain: MeleeChain,
    sm: &mut PlayerStateMachine,
) -> Option<ActiveMelee> {
    let index = match chain {
        MeleeChain::Light => combo.light_index,
        MeleeChain::Heavy => combo.heavy_index,
    }
    .min(library.chain_len(chain).saturating_sub(1));
    let def = library.get(chain, index)?;
    sm.force(PlayerState::Attacking);
    Some(ActiveMelee {
        chain,
        index,
        phase: MeleePhase::Startup,
        timer: def.startup,
    })
}

fn spawn_melee_flash(commands: &mut Commands, assets: &ProjectileAssets, position: Vec3) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(assets.flash_sphere.clone()),
            material: MeshMaterial3d(assets.mat_melee_flash.clone()),
            transform: Transform::from_translation(position),
            ..default()
        },
        HitParticle {
            lifetime: 0.12,
            max_lifetime: 0.12,
            velocity: Vec3::ZERO,
        },
    ));
}

#[allow(clippy::too_many_arguments)]
fn execute_melee_hit(
    origin: Vec3,
    forward: Vec3,
    radius: f32,
    offset: f32,
    arc_cos: f32,
    damage: f32,
    damage_type: DamageType,
    knockback: f32,
    enemy_q: &mut Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        Without<HackedUnit>,
    >,
    damaged_ev: &mut MessageWriter<EnemyDamagedEvent>,
    killed_ev: &mut MessageWriter<EnemyKilledEvent>,
) {
    let forward = forward.with_y(0.0).normalize_or_zero();
    let hit_center = origin + forward * offset;
    for (e_entity, e_transform, mut health, mut damageable, enemy) in enemy_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        let to_enemy = (e_transform.translation - origin).with_y(0.0);
        let in_arc = to_enemy.length() <= radius + offset
            && to_enemy.normalize_or_zero().dot(forward) >= arc_cos;
        if in_arc || hit_center.distance(e_transform.translation) <= radius {
            let push = to_enemy.normalize_or_zero() + Vec3::Y * 0.25;
            let info = DamageInfo::new(damage, damage_type)
                .with_knockback(knockback)
                .with_hit_direction(push);
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
        }
    }
}

// ── Star Sabre ────────────────────────────────────────────────────────────────
/// Level-scaling multipliers applied on top of the authored sabre MoveDefs:
/// `BeamSabre::set_level` keeps the level tables, and the `MoveLibrary`
/// carries the level-1 base numbers, so `(slash, wave)` scales stay 1.0 at
/// level 1 and edits to `moves.json` retune every level proportionally.
fn sabre_level_scale(sabre: &BeamSabre) -> (f32, f32) {
    let base = BeamSabre::default();
    (
        sabre.slash_damage / base.slash_damage,
        sabre.wave_damage / base.wave_damage,
    )
}

fn beam_sabre_update_system(
    time: Res<Time>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    library: Res<MoveLibrary>,
    mut hitstop: ResMut<HitstopState>,
    dungeon: Res<DungeonCrawlState>,
    mut player_q: Query<
        (
            Entity,
            &GlobalTransform,
            &mut BeamSabre,
            &mut PlayerStateMachine,
            &PlayerInput,
            &PlayerCameraRef,
            &ArmorSet,
            &PlayerProgression,
            Option<&BeamSabreLocked>,
        ),
        With<Player>,
    >,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    mut enemy_q: Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        Without<HackedUnit>,
    >,
    mut damaged_ev: MessageWriter<EnemyDamagedEvent>,
    mut killed_ev: MessageWriter<EnemyKilledEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    for (
        entity,
        player_transform,
        mut sabre,
        mut sm,
        pi,
        cam_ref,
        armor,
        progression,
        locked_marker,
    ) in player_q.iter_mut()
    {
        let upgrades = &progression.upgrades;
        let perk_damage_mult = progression.perks.damage_mult()
            * upgrades.beam_damage_mult()
            * upgrades.gauntlet_energy_damage_mult();
        if upgrades.blade_boots_unlock_sabre() && !sabre.unlocked {
            sabre.unlocked = true;
            if locked_marker.is_some() {
                commands.entity(entity).remove::<BeamSabreLocked>();
            }
        }
        let Ok(cam) = cam_q.get(cam_ref.0) else {
            continue;
        };
        let fwd = combat_forward(player_transform, cam, pi, dungeon.active);
        let origin = star_muzzle_origin(player_transform, fwd);
        let armor_damage_mult = armor.modified_outgoing_damage(perk_damage_mult);
        let blade_damage_type = if upgrades.blade_boot_rank() > 0 {
            DamageType::Laser
        } else {
            DamageType::Melee
        };
        let wave_damage_type = gauntlet_projectile_damage_type(upgrades, DamageType::Laser);

        if pi.sabre_toggle {
            if sabre.unlocked {
                sabre.active = !sabre.active;
                msg_ev.write(UiMessageEvent {
                    text: if sabre.active {
                        "Star Sabre active — RT to slash".into()
                    } else {
                        "Star Sabre holstered".into()
                    },
                    duration: 1.8,
                });
            } else {
                msg_ev.write(UiMessageEvent {
                    text: "Star Sabre locked — recover the Solar Sabre Glyph".into(),
                    duration: 2.4,
                });
            }
            continue;
        }

        if !sabre.unlocked || !sabre.active {
            continue;
        }

        // EC2: sabre frame data comes from the move library; `BeamSabre`
        // keeps runtime state (level scaling, cooldown, slash progress).
        let (slash_scale, wave_scale) = sabre_level_scale(&sabre);

        sabre.cooldown_timer = (sabre.cooldown_timer - dt).max(0.0);

        if sabre.is_slashing {
            sabre.slash_timer -= dt;
            if sabre.slash_timer <= 0.0 {
                sabre.slash_index += 1;
                if sabre.slash_index < sabre.slash_count {
                    let Some(def) = library.sabre_slash(sabre.slash_index as usize) else {
                        sabre.is_slashing = false;
                        sabre.slash_index = 0;
                        sm.transition(PlayerState::Idle);
                        continue;
                    };
                    let radius = if dungeon.active { 5.2 } else { 3.5 };
                    let offset = if dungeon.active { 2.0 } else { 2.5 };
                    let arc_cos = if dungeon.active { -0.40 } else { 0.10 };
                    execute_melee_hit(
                        origin,
                        fwd,
                        radius,
                        offset,
                        arc_cos,
                        def.damage * slash_scale * armor_damage_mult,
                        blade_damage_type,
                        def.knockback,
                        &mut enemy_q,
                        &mut damaged_ev,
                        &mut killed_ev,
                    );
                    spawn_melee_flash(&mut commands, &proj_assets, origin + fwd * 2.5);
                    hitstop.remaining = hitstop.remaining.max(def.hitstop);
                    sabre.slash_timer = def.total_duration();
                } else {
                    sabre.is_slashing = false;
                    sabre.slash_index = 0;
                    sm.transition(PlayerState::Idle);
                }
            }
            continue;
        }

        if pi.fire_just && sabre.cooldown_timer <= 0.0 {
            let Some(def) = library.sabre_slash(0) else {
                continue;
            };
            sabre.is_slashing = true;
            sabre.slash_index = 0;
            sabre.cooldown_timer = sabre.cooldown;
            sabre.slash_timer = def.total_duration();
            sm.force(PlayerState::Attacking);

            let radius = if dungeon.active { 5.2 } else { 3.5 };
            let offset = if dungeon.active { 2.0 } else { 2.5 };
            let arc_cos = if dungeon.active { -0.40 } else { 0.10 };
            execute_melee_hit(
                origin,
                fwd,
                radius,
                offset,
                arc_cos,
                def.damage * slash_scale * armor_damage_mult,
                blade_damage_type,
                def.knockback,
                &mut enemy_q,
                &mut damaged_ev,
                &mut killed_ev,
            );
            spawn_melee_flash(&mut commands, &proj_assets, origin + fwd * 2.5);
            hitstop.remaining = hitstop.remaining.max(def.hitstop);

            if sabre.fires_dual_wave() || dungeon.active {
                let wave = &library.sabre_wave;
                let right = cam.right().as_vec3();
                let wave_offsets: &[f32] = if sabre.fires_dual_wave() {
                    &[-0.4, 0.4]
                } else {
                    &[0.0]
                };
                for wave_offset in wave_offsets {
                    let dir = (fwd + right.with_y(0.0).normalize_or_zero() * *wave_offset)
                        .with_y(0.0)
                        .normalize_or_zero();
                    commands.spawn((
                        EnergyPbrBundle {
                            mesh: Mesh3d(proj_assets.sphere_sm.clone()),
                            material: MeshMaterial3d(proj_assets.energy_sabre.clone()),
                            transform: Transform::from_translation(origin)
                                .looking_to(dir, Vec3::Y)
                                .with_scale(Vec3::new(0.8, 0.8, 2.5)),
                            ..default()
                        },
                        Projectile {
                            damage: wave.damage
                                * wave_scale
                                * armor_damage_mult
                                * if dungeon.active { 0.72 } else { 1.0 },
                            damage_type: wave_damage_type,
                            speed: wave.projectile_speed,
                            direction: dir,
                            lifetime: wave.projectile_lifetime,
                            is_explosive: sabre.has_aoe_splash(),
                            explosion_radius: if sabre.has_aoe_splash() {
                                wave.explosion_radius
                            } else {
                                0.0
                            },
                            weapon_type: ProjectileOwner::Player,
                            owner: Some(entity),
                            piercing: sabre.is_piercing(),
                            gravity_affected: false,
                            vertical_velocity: 0.0,
                        },
                    ));
                }
            }
        }
    }
}

fn sync_sabre_blade_visual(
    mut commands: Commands,
    assets: Res<ProjectileAssets>,
    player_q: Query<(Entity, &GlobalTransform, &BeamSabre), With<Player>>,
    mut visual_q: Query<(Entity, &SabreBladeVisual, &mut Transform), Without<Player>>,
) {
    let mut represented = Vec::new();
    for (visual_entity, visual, mut transform) in visual_q.iter_mut() {
        let Ok((_, player_transform, sabre)) = player_q.get(visual.owner) else {
            commands.entity(visual_entity).despawn();
            continue;
        };
        if !sabre.active || !sabre.unlocked {
            commands.entity(visual_entity).despawn();
            continue;
        }
        represented.push(visual.owner);
        *transform = sabre_blade_transform(player_transform, sabre);
    }

    for (player_entity, player_transform, sabre) in player_q.iter() {
        if !sabre.active || !sabre.unlocked || represented.contains(&player_entity) {
            continue;
        }
        commands.spawn((
            EnergyPbrBundle {
                mesh: Mesh3d(assets.sphere_sm.clone()),
                material: MeshMaterial3d(assets.energy_sabre.clone()),
                transform: sabre_blade_transform(player_transform, sabre),
                ..default()
            },
            SabreBladeVisual {
                owner: player_entity,
            },
        ));
    }
}

fn sabre_blade_transform(player: &GlobalTransform, sabre: &BeamSabre) -> Transform {
    let forward = player.forward().as_vec3().with_y(0.0).normalize_or_zero();
    let right = player.right().as_vec3().with_y(0.0).normalize_or_zero();
    let swing = if sabre.is_slashing {
        if sabre.slash_index.is_multiple_of(2) {
            0.72
        } else {
            -0.72
        }
    } else {
        0.22
    };
    let blade_direction = (forward + right * swing + Vec3::Y * 0.12).normalize_or_zero();
    Transform::from_translation(player.translation() + Vec3::Y * 1.15 + forward * 0.9 + right * 0.5)
        .looking_to(blade_direction, Vec3::Y)
        .with_scale(Vec3::new(1.8, 1.8, 18.0))
}

// ── Hit Particles ─────────────────────────────────────────────────────────────
fn hit_particle_spawn_system(
    mut game_rng: ResMut<GameRng>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    mut damaged_ev: MessageReader<EnemyDamagedEvent>,
) {
    use rand::Rng;
    let rng = game_rng.cosmetic();

    for ev in damaged_ev.read() {
        let count = rng.gen_range(4usize..8);
        for _ in 0..count {
            let vel = Vec3::new(
                rng.gen_range(-5.0f32..5.0),
                rng.gen_range(3.0f32..9.0),
                rng.gen_range(-5.0f32..5.0),
            );
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(proj_assets.sphere_sm.clone()),
                    material: MeshMaterial3d(proj_assets.mat_hit_particle.clone()),
                    transform: Transform::from_translation(ev.position + Vec3::Y * 0.5),
                    ..default()
                },
                HitParticle {
                    lifetime: 0.45,
                    max_lifetime: 0.45,
                    velocity: vel,
                },
            ));
        }
    }
}

fn critical_impact_spawn_system(
    mut commands: Commands,
    assets: Res<ProjectileAssets>,
    mut impact_ev: MessageReader<CombatImpactEvent>,
) {
    for impact in impact_ev.read() {
        if !impact.is_critical {
            continue;
        }
        // Critical hits get a short, unmistakable hot-pink core whose size
        // reflects resolved (post-resistance) damage. The ordinary gold sparks
        // still render beneath it, keeping normal and critical hits distinct.
        let damage_scale = (impact.damage / 60.0).sqrt().clamp(0.85, 2.1);
        let elemental_scale = match impact.damage_type {
            DamageType::Explosive | DamageType::Fire => 1.25,
            DamageType::Melee => 0.9,
            _ => 1.0,
        };
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(assets.flash_sphere.clone()),
                material: MeshMaterial3d(assets.mat_critical_hit.clone()),
                transform: Transform::from_translation(impact.position + Vec3::Y * 0.8)
                    .with_scale(Vec3::splat(damage_scale * elemental_scale)),
                ..default()
            },
            HitParticle {
                lifetime: 0.14,
                max_lifetime: 0.14,
                velocity: Vec3::Y * 1.5,
            },
        ));
    }
}

fn particle_update_system(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(
        Entity,
        &mut Transform,
        &mut HitParticle,
        Option<&PreserveParticleShape>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut particle, preserved_shape) in q.iter_mut() {
        particle.lifetime -= dt;
        if particle.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += particle.velocity * dt;
        particle.velocity.y -= 18.0 * dt;
        let t = (particle.lifetime / particle.max_lifetime).max(0.05);
        transform.scale = preserved_shape.map_or(Vec3::splat(t), |shape| shape.0 * t);
    }
}

#[cfg(test)]
mod tracking_missile_tests {
    use super::*;

    #[test]
    fn acquisition_prefers_forward_aligned_target() {
        let mut world = World::new();
        let centered = world.spawn_empty().id();
        let offset = world.spawn_empty().id();
        let acquired = acquire_tracking_target(
            Vec3::ZERO,
            Vec3::Z,
            100.0,
            0.25,
            [
                (offset, Vec3::new(18.0, 0.0, 30.0)),
                (centered, Vec3::new(0.0, 0.0, 42.0)),
            ]
            .into_iter(),
        );
        assert_eq!(acquired.map(|(entity, _)| entity), Some(centered));
    }

    #[test]
    fn acquisition_rejects_targets_behind_missile() {
        let mut world = World::new();
        let behind = world.spawn_empty().id();
        assert!(acquire_tracking_target(
            Vec3::ZERO,
            Vec3::Z,
            100.0,
            0.25,
            [(behind, Vec3::new(0.0, 0.0, -10.0))].into_iter(),
        )
        .is_none());
    }

    #[test]
    fn steering_respects_turn_rate() {
        let steered = steer_toward_direction(Vec3::Z, Vec3::X, 0.2);
        assert!(steered.x > 0.0);
        assert!(steered.z > 0.0);
        assert!(Vec3::Z.angle_between(steered) <= 0.25);
    }

    #[test]
    fn swept_collision_catches_fast_projectile_between_frames() {
        let distance = segment_point_distance_squared(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 18.0),
            Vec3::new(0.4, 1.0, 9.0),
        );
        assert!(distance < 1.75_f32.powi(2));
    }

    #[test]
    fn muzzle_correction_preserves_vertical_camera_aim() {
        let camera_forward = Vec3::new(0.0, 0.5, -0.866_025_4).normalize();
        let camera_origin = Vec3::new(0.0, 3.0, 6.0);
        let muzzle = Vec3::new(0.7, 1.2, 0.0);
        let aim_point = camera_origin + camera_forward * AIM_MAX_DISTANCE;
        let direction = direction_to_aim_point(muzzle, aim_point, camera_forward);

        assert!(direction.y > 0.45);
        assert!(direction.z < -0.8);
        assert!((direction.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn zero_length_aim_uses_camera_fallback() {
        let fallback = Vec3::new(0.0, 0.2, -1.0).normalize();
        assert_eq!(
            direction_to_aim_point(Vec3::ONE, Vec3::ONE, fallback),
            fallback
        );
    }

    #[test]
    fn unlimited_weapons_ignore_stale_zero_ammo_counts() {
        let mut primary = Weapon::new(WeaponType::Pistol);
        primary.ammo = 0;
        assert!(primary.can_fire());

        let mut special = SpecialWeapon::new(SpecialSlot::Slot7);
        special.ammo = 0;
        assert!(special.can_fire());
    }

    #[test]
    fn primary_weapon_cycle_exits_persistent_tracking_missile_selection() {
        let mut app = App::new();
        app.add_message::<WeaponSwitchedEvent>();
        app.add_systems(Update, weapon_select_system);
        let entity = app
            .world_mut()
            .spawn((
                Player,
                PlayerInput {
                    weapon_next: true,
                    ..Default::default()
                },
                WeaponInventory::default(),
                SpecialWeaponInventory {
                    active_slot: Some(0),
                    ..Default::default()
                },
            ))
            .id();

        app.update();

        let specials = app.world().get::<SpecialWeaponInventory>(entity).unwrap();
        let primaries = app.world().get::<WeaponInventory>(entity).unwrap();
        assert_eq!(specials.active_slot, None);
        assert_eq!(primaries.active_slot, 1);
    }
}

#[cfg(test)]
mod move_def_wiring_tests {
    use super::*;

    #[test]
    fn ranged_defaults_mirror_legacy_weapon_new_tuning() {
        // The MoveLibrary numbers were derived from Weapon::new; if either
        // side drifts, the data-driven path would change gameplay feel.
        let lib = MoveLibrary::defaults();
        let slot_order = [
            WeaponType::Pistol,
            WeaponType::Rifle,
            WeaponType::Shotgun,
            WeaponType::Rocket,
            WeaponType::Laser,
            WeaponType::Grenade,
        ];
        for (slot, wt) in slot_order.into_iter().enumerate() {
            let legacy = Weapon::new(wt);
            let def = lib.ranged_slot(slot).expect("every slot authored");
            assert_eq!(def.name, wt.display_name(), "slot {slot} name");
            assert!(
                (def.damage - legacy.damage).abs() < 1e-6,
                "slot {slot} damage"
            );
            assert!(
                (def.fire_rate - legacy.fire_rate).abs() < 1e-6,
                "slot {slot} fire_rate"
            );
            assert!(
                (def.projectile_speed - legacy.speed).abs() < 1e-6,
                "slot {slot} speed"
            );
            assert!(
                (def.spread - legacy.spread).abs() < 1e-6,
                "slot {slot} spread"
            );
            assert_eq!(def.pellets, legacy.pellets, "slot {slot} pellets");
            assert!(
                (def.explosion_radius - legacy.explosion_radius).abs() < 1e-6,
                "slot {slot} explosion radius"
            );
        }
    }

    #[test]
    fn apply_ranged_defs_overwrites_stats_but_preserves_runtime_state() {
        let mut lib = MoveLibrary::defaults();
        lib.ranged[0].damage = 99.0;
        lib.ranged[0].fire_rate = 0.5;
        lib.ranged[0].pellets = 3;

        let mut inv = WeaponInventory::default();
        {
            let pistol = &mut inv.slots[0];
            pistol.ammo = 7;
            pistol.fire_timer = 0.42;
            pistol.rank = 2;
            pistol.charge_progress = 0.6;
        }

        apply_ranged_defs(&lib, &mut inv);

        let pistol = &inv.slots[0];
        assert_eq!(pistol.damage, 99.0);
        assert_eq!(pistol.fire_rate, 0.5);
        assert_eq!(pistol.pellets, 3);
        // Runtime state must survive a data reload.
        assert_eq!(pistol.ammo, 7);
        assert_eq!(pistol.fire_timer, 0.42);
        assert_eq!(pistol.rank, 2);
        assert_eq!(pistol.charge_progress, 0.6);
        // Identity/behavior flags stay owned by Weapon::new.
        assert_eq!(pistol.weapon_type, WeaponType::Pistol);
        assert!(!pistol.automatic);
        assert!(!pistol.is_explosive);
    }

    #[test]
    fn ranged_move_defs_sync_on_spawn_and_on_library_edits() {
        let mut app = App::new();
        let mut library = MoveLibrary::defaults();
        library.ranged[0].damage = 111.0;
        app.insert_resource(library);
        app.add_systems(Update, apply_ranged_move_defs_system);

        let entity = app.world_mut().spawn(WeaponInventory::default()).id();
        app.update();
        let inv = app.world().get::<WeaponInventory>(entity).unwrap();
        assert_eq!(inv.slots[0].damage, 111.0);

        // A library edit (hot reload path) re-syncs existing inventories.
        app.world_mut().resource_mut::<MoveLibrary>().ranged[1].fire_rate = 9.0;
        app.update();
        let inv = app.world().get::<WeaponInventory>(entity).unwrap();
        assert_eq!(inv.slots[1].fire_rate, 9.0);
    }

    #[test]
    fn sabre_level_scaling_reproduces_legacy_damage_tables() {
        // BeamSabre::set_level tables, expressed as multipliers over the
        // authored level-1 MoveDef base, must reproduce the legacy numbers.
        let lib = MoveLibrary::defaults();
        let slash = lib.sabre_slash(0).expect("sabre chain authored");
        let wave = &lib.sabre_wave;
        for (level, legacy_slash, legacy_wave) in [
            (1, 25.0_f32, 40.0_f32),
            (2, 35.0, 60.0),
            (3, 50.0, 80.0),
            (4, 65.0, 100.0),
            (5, 85.0, 150.0),
        ] {
            let mut sabre = BeamSabre::default();
            sabre.set_level(level);
            let (slash_scale, wave_scale) = sabre_level_scale(&sabre);
            assert!(
                (slash.damage * slash_scale - legacy_slash).abs() < 1e-3,
                "level {level} slash damage"
            );
            assert!(
                (wave.damage * wave_scale - legacy_wave).abs() < 1e-3,
                "level {level} wave damage"
            );
        }
        // Legacy inter-slash cadence was a hardcoded 0.25 s.
        assert!((slash.total_duration() - 0.25).abs() < 1e-4);
        assert!((slash.knockback - 3.0).abs() < 1e-6);
    }
}
