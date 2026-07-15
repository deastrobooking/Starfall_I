use bevy::prelude::*;

use crate::components::armor::ArmorSet;
use crate::components::enemy::{DeadEnemy, Enemy};
use crate::components::player::*;
use crate::components::weapon::*;
use crate::components::world::NpcRoadVehicle;
use crate::damage::{
    apply_damage, area_damage_falloff, DamageInfo, DamageType, Damageable, Health,
};
use crate::events::*;
use crate::hacking::HackedUnit;
use crate::perks::PerkTree;
use crate::rendering::PbrBundle;
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
}

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct WeaponPlugin;

impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponRanks>()
            .add_systems(Startup, setup_weapon_assets)
            .add_systems(
                Update,
                (
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
    });
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

// ── Soft aim assist ───────────────────────────────────────────────────────────
// Nudges fire direction toward the nearest enemy within a narrow cone.
fn aim_assist_direction(
    raw_forward: Vec3,
    muzzle_pos: Vec3,
    enemy_q: &Query<
        (&GlobalTransform, &Health),
        (With<Enemy>, Without<DeadEnemy>, Without<HackedUnit>),
    >,
    actively_aiming: bool,
    range_bonus: f32,
    cone_relax: f32,
    strength_bonus: f32,
) -> Vec3 {
    const ASSIST_RANGE: f32 = 90.0;

    let assist_range = ASSIST_RANGE + range_bonus;
    let base_cone = if actively_aiming { 0.78 } else { 0.90 };
    let assist_cone_cos = (base_cone - cone_relax).clamp(0.62, 0.96);
    let assist_strength = if actively_aiming {
        1.0
    } else {
        (0.72 + strength_bonus).clamp(0.65, 0.95)
    };
    let mut best_score = f32::NEG_INFINITY;
    let mut best_dir: Option<Vec3> = None;

    for (e_transform, health) in enemy_q.iter() {
        if !health.is_alive() {
            continue;
        }
        // Enemies are rooted at their feet; aim into the torso so the reticle
        // and projectile collision agree visually.
        let to = e_transform.translation() + Vec3::Y * 0.9 - muzzle_pos;
        let distance = to.length();
        if distance > assist_range || distance <= 0.01 {
            continue;
        }
        let to_norm = to.normalize_or_zero();
        let dot = to_norm.dot(raw_forward);
        let score = dot * 3.0 - distance / assist_range;
        if dot >= assist_cone_cos && score > best_score {
            best_score = score;
            best_dir = Some(to_norm);
        }
    }

    if let Some(target_dir) = best_dir {
        (raw_forward * (1.0 - assist_strength) + target_dir * assist_strength).normalize_or_zero()
    } else {
        raw_forward
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
    ranks: Res<WeaponRanks>,
    mut player_q: Query<&mut WeaponInventory, With<Player>>,
) {
    if !ranks.is_changed() {
        return;
    }
    for mut inv in player_q.iter_mut() {
        for (i, weapon) in inv.slots.iter_mut().enumerate() {
            weapon.rank = ranks.ranks[i];
        }
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
    time: Res<Time>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    perks: Res<PerkTree>,
    upgrades: Res<UpgradeLedger>,
    dungeon: Res<DungeonCrawlState>,
    mut player_q: Query<
        (
            &GlobalTransform,
            &PlayerIndex,
            &mut WeaponInventory,
            &SpecialWeaponInventory,
            &mut PlayerStateMachine,
            &PlayerInput,
            &PlayerCameraRef,
            &ArmorSet,
            Option<&ArmCannonUser>,
            Option<&MagicBeamCaster>,
        ),
        With<Player>,
    >,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    enemy_q: Query<
        (&GlobalTransform, &Health),
        (With<Enemy>, Without<DeadEnemy>, Without<HackedUnit>),
    >,
    mut fired_ev: MessageWriter<WeaponFiredEvent>,
) {
    let dt = time.delta_secs();
    let perk_damage_mult = perks.damage_mult();

    for (
        player_transform,
        player_index,
        mut inv,
        special_inv,
        mut sm,
        pi,
        cam_ref,
        armor,
        arm_cannon,
        magic_caster,
    ) in player_q.iter_mut()
    {
        // A selected special weapon owns RT until normal weapon cycling or a
        // direct primary slot clears the selection.
        if special_inv.active_slot.is_some() {
            continue;
        }
        let Ok(cam) = cam_q.get(cam_ref.0) else {
            continue;
        };

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
                let raw_fwd = combat_forward(player_transform, cam, pi, dungeon.active);
                let pos = star_muzzle_origin(player_transform, raw_fwd);
                let aim_fwd = aim_assist_direction(
                    raw_fwd,
                    pos,
                    &enemy_q,
                    pi.aim,
                    upgrades.gauntlet_aim_range_bonus(),
                    upgrades.gauntlet_aim_cone_relax(),
                    upgrades.gauntlet_aim_strength_bonus(),
                );

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
                    &upgrades,
                    primary_fallback_damage_type(wt, explosive_weapon),
                );
                spawn_charge_blast(
                    &mut commands,
                    &proj_assets,
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

        let raw_fwd = combat_forward(player_transform, cam, pi, dungeon.active);
        let pos = star_muzzle_origin(player_transform, raw_fwd);
        let right = cam.right().as_vec3();
        let up = cam.up().as_vec3();
        let aim_fwd = aim_assist_direction(
            raw_fwd,
            pos,
            &enemy_q,
            pi.aim,
            upgrades.gauntlet_aim_range_bonus(),
            upgrades.gauntlet_aim_cone_relax(),
            upgrades.gauntlet_aim_strength_bonus(),
        );

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
            &upgrades,
            primary_fallback_damage_type(weapon.weapon_type, explosive_weapon),
        );

        let (mesh_h, base_mat_h) = base_proj_handles(weapon.weapon_type, &proj_assets);
        let magic_tracking = magic_caster.is_some() && !explosive_weapon;
        let mat_h = if magic_tracking {
            proj_assets.mat_laser.clone()
        } else {
            base_mat_h
        };
        let projectile_stretch = if magic_tracking {
            Vec3::new(stretch.x.max(0.8), stretch.y.max(0.8), stretch.z.max(5.0))
        } else {
            stretch
        };

        weapon.fire_timer = effective_fire_rate;

        for _ in 0..pellets {
            use rand::Rng;
            let mut rng = rand::thread_rng();
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

            let mut projectile_entity = commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(mesh_h.clone()),
                    material: MeshMaterial3d(mat_h.clone()),
                    transform: proj_transform,
                    ..default()
                },
                Projectile {
                    damage,
                    damage_type,
                    speed,
                    direction: dir,
                    lifetime: 3.0,
                    is_explosive,
                    explosion_radius,
                    weapon_type: ProjectileOwner::Player,
                    owner: None,
                    piercing: false,
                    gravity_affected,
                    vertical_velocity: if gravity_affected { 0.2 } else { 0.0 },
                },
            ));
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

fn charge_mat_handle(wt: WeaponType, assets: &ProjectileAssets) -> Handle<StandardMaterial> {
    match wt {
        WeaponType::Pistol => assets.mat_charge_pistol.clone(),
        WeaponType::Rifle => assets.mat_charge_rifle.clone(),
        WeaponType::Shotgun => assets.mat_charge_shotgun.clone(),
        WeaponType::Rocket => assets.mat_charge_rocket.clone(),
        WeaponType::Laser => assets.mat_charge_laser.clone(),
        WeaponType::Grenade => assets.mat_charge_grenade.clone(),
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
    commands: &mut Commands,
    assets: &ProjectileAssets,
    pos: Vec3,
    dir: Vec3,
    right: Vec3,
    wt: WeaponType,
    damage: f32,
    damage_type: DamageType,
    explosion_radius: f32,
    base_speed: f32,
) {
    let mat = charge_mat_handle(wt, assets);

    match wt {
        // Rifle: ultra-fast piercing bolt
        WeaponType::Rifle => {
            commands.spawn((
                PbrBundle {
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
                    owner: None,
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
                PbrBundle {
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
                    owner: None,
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
            let mut rng = rand::thread_rng();
            let up = dir.cross(right).normalize_or_zero();
            let pellet_dmg = damage / 14.0;
            for _ in 0..18u32 {
                let sx = rng.gen_range(-0.38f32..0.38);
                let sy = rng.gen_range(-0.38f32..0.38);
                let shot_dir = (dir + right * sx + up * sy).normalize_or_zero();
                commands.spawn((
                    PbrBundle {
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
                        owner: None,
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
                PbrBundle {
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
                    owner: None,
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
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    player_q: Query<(&GlobalTransform, &WeaponInventory, &PlayerCameraRef), With<Player>>,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    for (player_transform, inv, cam_ref) in player_q.iter() {
        let weapon = inv.active();
        if weapon.charge_progress < 0.1 {
            continue;
        }
        let Ok(cam) = cam_q.get(cam_ref.0) else {
            continue;
        };
        let fwd = cam.forward().as_vec3();
        let pos = star_muzzle_origin(player_transform, fwd);

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
    perks: Res<PerkTree>,
    mut player_q: Query<(&mut WeaponInventory, &mut SpecialWeaponInventory), With<Player>>,
) {
    let ammo_mult = perks.ammo_mult();
    for (mut weapons, mut specials) in player_q.iter_mut() {
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
    time: Res<Time>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    perks: Res<PerkTree>,
    upgrades: Res<UpgradeLedger>,
    mut player_q: Query<
        (
            Entity,
            &GlobalTransform,
            &PlayerIndex,
            &mut SpecialWeaponInventory,
            &PlayerInput,
            &PlayerCameraRef,
            &ArmorSet,
            &BeamSabre,
        ),
        With<Player>,
    >,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    mut fired_ev: MessageWriter<WeaponFiredEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    let perk_damage_mult = perks.damage_mult();
    for (player_entity, player_transform, player_index, mut inv, pi, cam_ref, armor, sabre) in
        player_q.iter_mut()
    {
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

        let fwd = cam.forward().as_vec3();
        let pos = star_muzzle_origin(player_transform, fwd);
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
                        gauntlet_projectile_damage_type(&upgrades, DamageType::Explosive);
                    inv.slot7.cooldown_timer = inv.slot7.cooldown;
                    commands.spawn((
                        PbrBundle {
                            mesh: Mesh3d(proj_assets.sphere_md.clone()),
                            material: MeshMaterial3d(proj_assets.mat_homing_star.clone()),
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
                    let damage_type =
                        gauntlet_projectile_damage_type(&upgrades, DamageType::Plasma);
                    inv.slot8.cooldown_timer = inv.slot8.cooldown;
                    use rand::Rng;
                    let mut rng = rand::thread_rng();
                    let right = cam.right().as_vec3();
                    let up = cam.up().as_vec3();
                    let burst_count = 3 + upgrades.gauntlet_extra_pellets();
                    for _ in 0..burst_count {
                        let sx = rng.gen_range(-0.05f32..0.05);
                        let sy = rng.gen_range(-0.05f32..0.05);
                        let dir = (fwd + right * sx + up * sy).normalize();
                        commands.spawn((
                            PbrBundle {
                                mesh: Mesh3d(proj_assets.sphere_sm.clone()),
                                material: MeshMaterial3d(proj_assets.mat_energy.clone()),
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
                                owner: None,
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
                        gauntlet_projectile_damage_type(&upgrades, DamageType::Explosive);
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
                    let damage_type =
                        gauntlet_projectile_damage_type(&upgrades, DamageType::Plasma);
                    inv.slot0.cooldown_timer = inv.slot0.cooldown;
                    use rand::Rng;
                    let mut rng = rand::thread_rng();
                    let right = cam.right().as_vec3();
                    let up = cam.up().as_vec3();
                    let burst_count = 5 + upgrades.gauntlet_extra_pellets();
                    for _ in 0..burst_count {
                        let sx = rng.gen_range(-0.08f32..0.08);
                        let sy = rng.gen_range(-0.08f32..0.08);
                        let dir = (fwd + right * sx + up * sy).normalize();
                        commands.spawn((
                            PbrBundle {
                                mesh: Mesh3d(proj_assets.sphere_sm.clone()),
                                material: MeshMaterial3d(proj_assets.mat_sprite_shot.clone()),
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
                                owner: None,
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
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(assets.sphere_sm.clone()),
                    material: MeshMaterial3d(if missile.magic_beam {
                        assets.mat_laser.clone()
                    } else {
                        assets.mat_homing_star.clone()
                    }),
                    transform: Transform::from_translation(
                        transform.translation - projectile.direction * 0.55,
                    )
                    .with_scale(if missile.magic_beam {
                        Vec3::new(0.75, 0.75, 3.2)
                    } else {
                        Vec3::splat(1.35)
                    }),
                    ..default()
                },
                HitParticle {
                    lifetime: 0.24,
                    max_lifetime: 0.24,
                    velocity: -projectile.direction * 1.8 + Vec3::Y * 0.35,
                },
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
const LIGHT_COMBO: &[(&str, f32, f32, f32)] = &[
    ("Star Jab", 15.0, 3.0, 0.4),
    ("Comet Cross", 20.0, 4.0, 0.45),
    ("Moon Uppercut", 30.0, 6.0, 0.6),
];

const HEAVY_COMBO: &[(&str, f32, f32, f32)] = &[
    ("Meteor Slam", 35.0, 8.0, 0.7),
    ("Orbit Sweep", 45.0, 10.0, 0.8),
];

fn melee_combo_system(
    time: Res<Time>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    dungeon: Res<DungeonCrawlState>,
    upgrades: Res<UpgradeLedger>,
    mut player_q: Query<
        (
            &GlobalTransform,
            &mut MeleeCombo,
            &mut PlayerStateMachine,
            &PlayerInput,
            &PlayerCameraRef,
            &ArmorSet,
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
    for (player_transform, mut combo, mut sm, pi, cam_ref, armor) in player_q.iter_mut() {
        let Ok(cam) = cam_q.get(cam_ref.0) else {
            continue;
        };

        combo.light_timer = (combo.light_timer - dt).max(0.0);
        combo.heavy_timer = (combo.heavy_timer - dt).max(0.0);
        combo.active_timer = (combo.active_timer - dt).max(0.0);

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

        if combo.active_timer <= 0.0 && combo.is_attacking {
            combo.is_attacking = false;
            sm.transition(PlayerState::Idle);
        }
        if combo.is_attacking {
            continue;
        }

        let do_light = combo.buffered_light;
        let do_heavy = combo.buffered_heavy;
        combo.buffered_light = false;
        combo.buffered_heavy = false;

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

        if do_light && combo.light_index < LIGHT_COMBO.len() {
            let (name, base_damage, knockback, duration) = LIGHT_COMBO[combo.light_index];
            let damage =
                base_damage * combo.damage_multiplier * armor_damage_mult * blade_damage_mult;
            let radius = (if dungeon.active { 4.1 } else { 3.0 }) + blade_reach_bonus;
            let offset = (if dungeon.active { 2.1 } else { 2.5 }) + blade_reach_bonus * 0.5;
            let arc_cos = if dungeon.active { -0.20 } else { 0.15 };

            execute_melee_hit(
                cam_pos,
                cam_fwd,
                radius,
                offset,
                arc_cos,
                damage,
                melee_damage_type,
                knockback,
                &mut enemy_q,
                &mut damaged_ev,
                &mut killed_ev,
            );
            spawn_melee_flash(&mut commands, &proj_assets, cam_pos + cam_fwd * 2.5);

            combo_ev.write(ComboHitEvent {
                combo_name: "Light".to_string(),
                attack_name: name.to_string(),
                combo_index: combo.light_index,
            });
            combo.light_index = (combo.light_index + 1) % LIGHT_COMBO.len();
            combo.light_timer = 1.5;
            combo.active_timer = duration;
            combo.is_attacking = true;
            sm.force(PlayerState::Attacking);

            if combo.light_index == 0 {
                finished_ev.write(ComboFinishedEvent {
                    combo_name: "Light".to_string(),
                });
            }
        } else if do_heavy && combo.heavy_index < HEAVY_COMBO.len() {
            let (name, base_damage, knockback, duration) = HEAVY_COMBO[combo.heavy_index];
            let damage =
                base_damage * combo.damage_multiplier * armor_damage_mult * blade_damage_mult;
            let radius = (if dungeon.active { 5.7 } else { 4.5 }) + blade_reach_bonus * 1.3;
            let offset = (if dungeon.active { 2.2 } else { 2.0 }) + blade_reach_bonus * 0.6;
            let arc_cos = if dungeon.active { -0.35 } else { 0.05 };

            execute_melee_hit(
                cam_pos,
                cam_fwd,
                radius,
                offset,
                arc_cos,
                damage,
                melee_damage_type,
                knockback,
                &mut enemy_q,
                &mut damaged_ev,
                &mut killed_ev,
            );
            spawn_melee_flash(&mut commands, &proj_assets, cam_pos + cam_fwd * 2.0);

            combo_ev.write(ComboHitEvent {
                combo_name: "Heavy".to_string(),
                attack_name: name.to_string(),
                combo_index: combo.heavy_index,
            });
            combo.heavy_index = (combo.heavy_index + 1) % HEAVY_COMBO.len();
            combo.heavy_timer = 2.0;
            combo.active_timer = duration;
            combo.is_attacking = true;
            sm.force(PlayerState::Attacking);

            if combo.heavy_index == 0 {
                finished_ev.write(ComboFinishedEvent {
                    combo_name: "Heavy".to_string(),
                });
            }
        }
    }
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
fn beam_sabre_update_system(
    time: Res<Time>,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    perks: Res<PerkTree>,
    upgrades: Res<UpgradeLedger>,
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
    let perk_damage_mult =
        perks.damage_mult() * upgrades.beam_damage_mult() * upgrades.gauntlet_energy_damage_mult();
    for (entity, player_transform, mut sabre, mut sm, pi, cam_ref, armor, locked_marker) in
        player_q.iter_mut()
    {
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
        let wave_damage_type = gauntlet_projectile_damage_type(&upgrades, DamageType::Laser);

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

        sabre.cooldown_timer = (sabre.cooldown_timer - dt).max(0.0);

        if sabre.is_slashing {
            sabre.slash_timer -= dt;
            if sabre.slash_timer <= 0.0 {
                sabre.slash_index += 1;
                if sabre.slash_index < sabre.slash_count {
                    let radius = if dungeon.active { 5.2 } else { 3.5 };
                    let offset = if dungeon.active { 2.0 } else { 2.5 };
                    let arc_cos = if dungeon.active { -0.40 } else { 0.10 };
                    execute_melee_hit(
                        origin,
                        fwd,
                        radius,
                        offset,
                        arc_cos,
                        sabre.slash_damage * armor_damage_mult,
                        blade_damage_type,
                        3.0,
                        &mut enemy_q,
                        &mut damaged_ev,
                        &mut killed_ev,
                    );
                    spawn_melee_flash(&mut commands, &proj_assets, origin + fwd * 2.5);
                    sabre.slash_timer = 0.25;
                } else {
                    sabre.is_slashing = false;
                    sabre.slash_index = 0;
                    sm.transition(PlayerState::Idle);
                }
            }
            continue;
        }

        if pi.fire_just && sabre.cooldown_timer <= 0.0 {
            sabre.is_slashing = true;
            sabre.slash_index = 0;
            sabre.cooldown_timer = sabre.cooldown;
            sabre.slash_timer = 0.25;
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
                sabre.slash_damage * armor_damage_mult,
                blade_damage_type,
                3.0,
                &mut enemy_q,
                &mut damaged_ev,
                &mut killed_ev,
            );
            spawn_melee_flash(&mut commands, &proj_assets, origin + fwd * 2.5);

            if sabre.fires_dual_wave() || dungeon.active {
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
                        PbrBundle {
                            mesh: Mesh3d(proj_assets.sphere_sm.clone()),
                            material: MeshMaterial3d(proj_assets.mat_melee_flash.clone()),
                            transform: Transform::from_translation(origin)
                                .looking_to(dir, Vec3::Y)
                                .with_scale(Vec3::new(0.8, 0.8, 2.5)),
                            ..default()
                        },
                        Projectile {
                            damage: sabre.wave_damage
                                * armor_damage_mult
                                * if dungeon.active { 0.72 } else { 1.0 },
                            damage_type: wave_damage_type,
                            speed: 20.0,
                            direction: dir,
                            lifetime: 1.5,
                            is_explosive: sabre.has_aoe_splash(),
                            explosion_radius: if sabre.has_aoe_splash() { 4.0 } else { 0.0 },
                            weapon_type: ProjectileOwner::Player,
                            owner: None,
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
            PbrBundle {
                mesh: Mesh3d(assets.sphere_sm.clone()),
                material: MeshMaterial3d(assets.mat_melee_flash.clone()),
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
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    mut damaged_ev: MessageReader<EnemyDamagedEvent>,
) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

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
    mut q: Query<(Entity, &mut Transform, &mut HitParticle)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut particle) in q.iter_mut() {
        particle.lifetime -= dt;
        if particle.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += particle.velocity * dt;
        particle.velocity.y -= 18.0 * dt;
        let t = (particle.lifetime / particle.max_lifetime).max(0.05);
        transform.scale = Vec3::splat(t);
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
