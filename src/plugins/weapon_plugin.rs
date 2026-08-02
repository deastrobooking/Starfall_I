use avian3d::prelude::{Collider as AvianCollider, RayHitData, SpatialQuery, SpatialQueryFilter};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;

use crate::audio::sfx::ModularActionSfxEvent;
use crate::components::character::{CartoonPart, CartoonPartKind, JointKind, JointMarker};
use crate::combat::blades::{
    apply_blade_to_stats, blade_for_id, BladeTrait, EquippedBlade, BLADE_COLOR_ORDER,
};
use crate::combat::damage::{
    apply_damage, area_damage_falloff, DamageInfo, DamageType, Damageable, Health,
};
use crate::combat::data::{ActiveMelee, MeleeChain, MeleePhase, MoveLibrary, RangedMoveDef};
use crate::combat::hitstop::HitstopState;
use crate::combat::upgrades::UpgradeLedger;
use crate::components::armor::ArmorSet;
use crate::components::enemy::{CitySpyDrone, DeadEnemy, Enemy, EnemyType, FlyingDrone};
use crate::components::player::*;
use crate::components::weapon::*;
use crate::components::world::NpcRoadVehicle;
use crate::engine::game_loop::GameSet;
use crate::engine::game_rng::GameRng;
use crate::engine::physics::{
    prelude::{CollisionProfile, GameCollisionLayer},
    world_line_of_sight,
};
use crate::engine::rendering::{SpatialBundle, EnergyMaterial, EnergyMaterialUniform, EnergyPbrBundle, PbrBundle};
use crate::engine::state::AppState;
use crate::events::*;
use crate::resources::DungeonCrawlState;
use crate::world::hacking::HackedUnit;

// ── Hit Particle ──────────────────────────────────────────────────────────────
#[derive(Component)]
pub struct HitParticle {
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub velocity: Vec3,
}

#[derive(Component)]
struct PreserveParticleShape(Vec3);

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct SabreTechniqueVfx {
    lifetime: f32,
    max_lifetime: f32,
    velocity: Vec3,
    spin: Vec3,
    base_scale: Vec3,
    expansion: f32,
}

#[derive(Resource, Debug, Clone, Copy)]
struct SabreVfxBudget {
    max_entities: usize,
}

pub(crate) const SABRE_VFX_ENTITY_BUDGET: usize = 24;

impl Default for SabreVfxBudget {
    fn default() -> Self {
        // Six transient shapes per local player in the four-player worst case.
        Self {
            max_entities: SABRE_VFX_ENTITY_BUDGET,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SabreBladeLayer {
    Aura,
    Core,
}

#[derive(Component)]
struct SabreBladeVisual {
    owner: Entity,
    layer: SabreBladeLayer,
    /// 0→1 ignition progress. The blade extends out of the emitter on draw
    /// instead of appearing at full length.
    ignition: f32,
}

/// The physical Star Sabre handle, parented to the character's actual hand.
///
/// Before this existed the blade was placed by guessing where the hand was
/// (`player.translation() + fixed offsets`), so it ignored the animated arm
/// entirely and there was no object being gripped — the hand posed as if
/// holding something invisible. The hilt is a real child of the hand entity,
/// so it inherits every pose, swing, and body-proportion change for free, and
/// the blade hangs off the hilt's emitter rather than off the player root.
#[derive(Component)]
struct SabreHilt {
    /// The player entity that owns this hilt (not the anchor it is parented to).
    owner: Entity,
    /// Whether this hilt is currently in hand or stowed on the body.
    carry: HiltCarry,
}

/// Where a hilt can be mounted, best first. The modular character can be built
/// from cartoon parts or an imported joint rig, so the sabre resolves whichever
/// this character actually has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandMount {
    /// A rigged skeleton's wrist joint — the most accurate mount.
    Wrist,
    /// The cartoon-part hand mesh.
    HandPart,
    /// Sheathed: clipped to the hip via the pelvis joint.
    HipJoint,
    /// Sheathed: clipped to the belt mesh.
    BeltPart,
}

/// Whether the sabre is in hand or stowed. A sheathed sabre still exists as a
/// physical object on the character — a weapon that vanishes when put away
/// reads as a magic trick rather than equipment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HiltCarry {
    Drawn,
    Holstered,
}

/// Choose the mount for a character, preferring rigged joints over cartoon
/// meshes. Pure so the fallback order is testable without spawning a rig.
fn resolve_hand_mount(
    carry: HiltCarry,
    has_wrist_joint: bool,
    has_hand_part: bool,
    has_pelvis_joint: bool,
    has_belt_part: bool,
) -> Option<HandMount> {
    match carry {
        HiltCarry::Drawn => {
            if has_wrist_joint {
                Some(HandMount::Wrist)
            } else if has_hand_part {
                Some(HandMount::HandPart)
            } else {
                None
            }
        }
        HiltCarry::Holstered => {
            if has_pelvis_joint {
                Some(HandMount::HipJoint)
            } else if has_belt_part {
                Some(HandMount::BeltPart)
            } else {
                None
            }
        }
    }
}

/// Local offset and orientation of the hilt inside its mount. A wrist joint
/// sits at the forearm end, so the grip needs pushing further into the palm
/// than it does on a hand mesh whose origin is already the palm.
fn hilt_local_transform(mount: HandMount) -> Transform {
    match mount {
        // In hand: grip lies along the hand's forward axis so the blade
        // projects out of the fist rather than through the wrist.
        HandMount::Wrist => Transform::from_translation(Vec3::new(0.0, -0.06, -0.05))
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        HandMount::HandPart => Transform::from_translation(Vec3::new(0.0, -0.02, -0.02))
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        // Holstered: clipped vertically against the right hip, tilted back a
        // little so it reads as hanging rather than welded on.
        HandMount::HipJoint => Transform::from_translation(Vec3::new(0.19, -0.06, 0.04))
            .with_rotation(Quat::from_rotation_z(-0.22)),
        HandMount::BeltPart => Transform::from_translation(Vec3::new(0.20, -0.04, 0.05))
            .with_rotation(Quat::from_rotation_z(-0.22)),
    }
}

impl HandMount {
    /// Holstered mounts hang the hilt on the body; drawn mounts put it in the
    /// hand. Only a drawn hilt ignites a blade.
    fn carry(self) -> HiltCarry {
        match self {
            HandMount::Wrist | HandMount::HandPart => HiltCarry::Drawn,
            HandMount::HipJoint | HandMount::BeltPart => HiltCarry::Holstered,
        }
    }
}

/// World-space confirmation that a homing projectile has acquired a target.
/// It follows the enemy rather than the camera, so it remains readable in
/// four-player split screen without another per-viewport UI layer.
#[derive(Component)]
struct TargetLockVisual {
    missile: Entity,
}

// ── Projectile Asset Cache ────────────────────────────────────────────────────
#[allow(dead_code)] // Full material palette pre-built at startup; several variants await their VFX consumers.
#[derive(Resource)]
pub struct ProjectileAssets {
    // Mesh sizes
    pub sphere_sm: Handle<Mesh>,
    pub sphere_md: Handle<Mesh>,
    pub sphere_lg: Handle<Mesh>,
    pub sphere_xl: Handle<Mesh>,
    pub flash_sphere: Handle<Mesh>,
    pub lock_ring: Handle<Mesh>,
    // Star Sabre hilt: a real handle the character's hand actually holds.
    pub hilt_grip: Handle<Mesh>,
    pub hilt_ring: Handle<Mesh>,
    pub hilt_pommel: Handle<Mesh>,
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
    pub energy_sabre_core: Handle<EnergyMaterial>,
    /// One aura material per [`BladeColor`], built once at startup and indexed
    /// by `BladeColor as usize`, so equipping a blade recolours the sabre
    /// without allocating a material per swing.
    pub energy_sabre_blades: Vec<Handle<EnergyMaterial>>,
}

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct WeaponPlugin;

impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponRanks>()
            .init_resource::<SabreVfxBudget>()
            .add_systems(Startup, setup_weapon_assets)
            .add_systems(
                Update,
                update_aim_solution_system
                    .before(weapon_fire_system)
                    .before(special_weapon_system)
                    .in_set(GameSet::Combat)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    apply_ranged_move_defs_system.before(weapon_fire_system),
                    apply_sabre_blade_system.before(beam_sabre_update_system),
                    apply_weapon_ranks_system,
                    weapon_select_system,
                    weapon_fire_system,
                    weapon_reload_system,
                    charge_spark_system,
                    special_weapon_system,
                    tracking_missile_system.before(projectile_update_system),
                    sync_target_lock_visual.after(tracking_missile_system),
                    assign_projectile_collision_profiles.before(projectile_update_system),
                    // Grouped: Bevy caps a system tuple at 20 entries.
                    (
                        projectile_pulse_system.before(projectile_update_system),
                        projectile_update_system,
                    ),
                    melee_combo_system,
                    beam_sabre_update_system,
                    mount_sabre_hilt_system.after(beam_sabre_update_system),
                    sync_sabre_blade_visual.after(mount_sabre_hilt_system),
                    sabre_technique_vfx_system.after(beam_sabre_update_system),
                    hit_particle_spawn_system,
                    critical_impact_spawn_system.after(hit_particle_spawn_system),
                    particle_update_system,
                )
                    // EC0 canonical order: attacks resolve in Combat, after the
                    // Motor-set player actions have consumed their inputs.
                    .in_set(GameSet::Combat)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

fn assign_projectile_collision_profiles(
    mut commands: Commands,
    projectile_q: Query<Entity, Added<Projectile>>,
) {
    for entity in projectile_q.iter() {
        commands
            .entity(entity)
            .insert(CollisionProfile::PlayerProjectile);
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

fn aim_assist_target(root: Vec3, airborne: bool) -> Vec3 {
    root + Vec3::Y * if airborne { 0.0 } else { 0.9 }
}

fn aim_assist_cone_cos(base_cone_cos: f32, airborne: bool) -> f32 {
    if airborne {
        (base_cone_cos - 0.10).max(0.58)
    } else {
        base_cone_cos
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
        (
            Entity,
            &GlobalTransform,
            &Health,
            Option<&FlyingDrone>,
            Option<&CitySpyDrone>,
        ),
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
        for (entity, transform, health, drone, city_spy) in enemy_q.iter() {
            if !health.is_alive() {
                continue;
            }
            let airborne = drone.is_some() || city_spy.is_some();
            // Ground enemies are rooted at their feet, while both drone
            // families are rooted at the center of their hurtbox. A torso
            // offset put spy-drone assist just above its shallow collider.
            let target_point = aim_assist_target(transform.translation(), airborne);
            let offset = target_point - camera_origin;
            let distance = offset.length();
            if distance <= 0.01 || distance > range {
                continue;
            }
            let dot = offset.normalize_or_zero().dot(camera_forward);
            // Fast, elevated targets get modest extra magnetism without
            // changing acquisition for grounded combatants.
            let target_cone_cos = aim_assist_cone_cos(cone_cos, airborne);
            if dot < target_cone_cos {
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
        // Grip runs along local Y; the mount rotates it into the palm.
        hilt_grip: meshes.add(Cylinder::new(0.030, 0.26)),
        hilt_ring: meshes.add(Cylinder::new(0.042, 0.030)),
        hilt_pommel: meshes.add(Sphere::new(0.038)),
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
            Vec4::new(1.8, 2.2, 2.8, 1.0),
            Vec4::new(0.12, 1.15, 3.8, 1.0),
            Vec4::new(6.4, 7.0, 8.5, 0.78),
        ),
        energy_sabre_core: mk_energy_mat(
            &mut energy_materials,
            Vec4::new(4.8, 4.8, 4.2, 1.0),
            Vec4::new(0.55, 2.8, 5.2, 1.0),
            Vec4::new(8.0, 10.0, 10.5, 1.0),
        ),
        // The aura carries each blade's identity; the core stays hot and
        // near-white so the weapon reads clearly at speed in 4-player co-op.
        energy_sabre_blades: BLADE_COLOR_ORDER
            .iter()
            .map(|color| {
                let (r, g, b) = color.aura_rgb();
                mk_energy_mat(
                    &mut energy_materials,
                    Vec4::new(1.8 * r + 0.6, 1.8 * g + 0.6, 1.8 * b + 0.6, 1.0),
                    Vec4::new(r * 3.6, g * 3.6, b * 3.6, 1.0),
                    Vec4::new(6.4, 7.0, 8.5, 0.78),
                )
            })
            .collect(),
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

// ── Apply the equipped Star Sabre blade ───────────────────────────────────────
/// Stamp the blade bought in the shop onto the player's sabre.
///
/// This is the link that makes `ShopOwnership::equipped_weapon` mean something:
/// before it existed the field was saved but never read, so buying a blade
/// changed nothing. Blade multipliers compose *on top of* `BeamSabre::set_level`
/// rather than replacing it, so progression and loadout both keep working.
///
/// Idempotent by `EquippedBlade`: the restat only happens when the choice
/// actually changes (or the sabre levels up), never every frame.
fn apply_sabre_blade_system(
    mut commands: Commands,
    mut player_q: Query<
        (
            Entity,
            &PlayerProgression,
            &mut BeamSabre,
            Option<&EquippedBlade>,
        ),
        With<Player>,
    >,
) {
    for (entity, progression, mut sabre, current) in player_q.iter_mut() {
        let blade = blade_for_id(progression.shop.equipped_weapon.as_deref());
        let changed = current.map(|c| c.0) != Some(blade.id);
        if !changed && !sabre.is_changed() {
            continue;
        }
        // Recompute the level baseline first so blade swaps never compound.
        let level = sabre.level;
        sabre.set_level(level);
        let (slash, wave, count, cooldown) = apply_blade_to_stats(
            &blade,
            sabre.slash_damage,
            sabre.wave_damage,
            sabre.slash_count,
            sabre.cooldown,
        );
        sabre.slash_damage = slash;
        sabre.wave_damage = wave;
        sabre.slash_count = count;
        sabre.cooldown = cooldown;
        if changed {
            commands.entity(entity).insert(EquippedBlade(blade.id));
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
/// Number of special weapon slots the D-pad cycle walks through.
const SPECIAL_SLOT_COUNT: u8 = 4;

/// Cycle the equipped special: `None` (primary weapon) → 0 → 1 → 2 → 3 → `None`.
/// Keeping "no special" inside the ring means the primary is always reachable
/// by continuing to press the same direction — no separate unequip input.
fn cycle_special_slot(current: Option<u8>, forward: bool) -> Option<u8> {
    match (current, forward) {
        (None, true) => Some(0),
        (None, false) => Some(SPECIAL_SLOT_COUNT - 1),
        (Some(slot), true) if slot + 1 >= SPECIAL_SLOT_COUNT => None,
        (Some(slot), true) => Some(slot + 1),
        (Some(0), false) => None,
        (Some(slot), false) => Some(slot - 1),
    }
}

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
        // ── Special cycle (D-pad up/down) ─────────────────────────────────
        // Walks None → 0 → 1 → 2 → 3 → None, so the primary weapon is always
        // reachable by continuing to cycle rather than a separate unequip.
        if pi.special_next || pi.special_prev {
            specials.active_slot = cycle_special_slot(specials.active_slot, pi.special_next);
            let weapon_name = specials
                .selected()
                .map(|special| special.name.to_string())
                .unwrap_or_else(|| inv.active().weapon_type.display_name().to_string());
            switched_ev.write(WeaponSwitchedEvent { weapon_name });
            continue;
        }

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
/// Arm-cannon characters charge this much faster — a perk, not a gate.
const ARM_CANNON_CHARGE_RATE: f32 = 1.6;
/// How fast an unreleased charge bleeds away once the trigger is let go.
const CHARGE_DECAY_RATE: f32 = 1.5;

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

        let active_slot = inv.active_slot;
        // EC2: per-slot projectile lifetime from the move library (legacy
        // hardcoded value was a shared 3.0 s).
        let projectile_lifetime = library
            .ranged_slot(active_slot)
            .map_or(3.0, |def| def.projectile_lifetime);
        let weapon = inv.active_mut();
        weapon.fire_timer = (weapon.fire_timer - dt).max(0.0);

        // ── Charge tracking ──────────────────────────────────────────────────
        // Every blaster charges. Arm-cannon characters simply wind up faster,
        // so the hardware is an advantage rather than a gate: charging used to
        // require a DariaCannon arm, which hid the mechanic from most of the
        // roster entirely.
        let charge_rate = if arm_cannon.is_some() {
            ARM_CANNON_CHARGE_RATE
        } else {
            1.0
        };
        let charge_released = weapon.charge_held && !pi.fire;
        if pi.fire {
            weapon.charge_progress = (weapon.charge_progress
                + dt * charge_rate / weapon.min_charge_time())
            .min(1.0);
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
        } else {
            // Trigger up and nothing released: bleed any partial charge away
            // rather than banking it between bursts.
            weapon.charge_progress =
                (weapon.charge_progress - dt * CHARGE_DECAY_RATE).max(0.0);
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

        // Authored steering and bolt size for this slot.
        let (tracking_strength, blast_scale) = library
            .ranged_slot(active_slot)
            .map_or((0.45, 1.0), |def| (def.tracking_strength, def.blast_scale));
        let (mesh_h, base_mat_h) = base_proj_handles(weapon.weapon_type, &proj_assets);
        let magic_tracking = magic_caster.is_some() && !explosive_weapon;
        let projectile_stretch = if magic_tracking {
            Vec3::new(stretch.x.max(0.8), stretch.y.max(0.8), stretch.z.max(5.0))
        } else {
            stretch
        } * blast_scale;

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
            } else if let Some(tracking) =
                TrackingMissile::primary(player_index.0, tracking_strength)
            {
                // Every primary steers a little; strength is authored per
                // weapon in moves.json.
                projectile_entity.insert(tracking);
            }
            // Bolts breathe as they fly so energy reads as energy.
            projectile_entity.insert(ProjectilePulse::new(projectile_stretch));
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
/// Muzzle build-up while a shot charges.
///
/// The sparks now converge *inward* instead of spraying outward, and a bright
/// core swells with the charge: energy gathering reads as gathering, and the
/// moment it tops out is unmistakable without watching the HUD.
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
        let charge = weapon.charge_progress;
        if charge < 0.1 {
            continue;
        }
        let pos = aim.muzzle_origin;
        let ready = charge >= 1.0;

        // Gathering motes: spawned out on a shell and pulled toward the muzzle.
        let count = (charge * 4.5) as u32 + 1;
        for _ in 0..count {
            let radius = 0.55 + charge * 0.55;
            let offset = Vec3::new(
                rng.gen_range(-1.0f32..1.0),
                rng.gen_range(-1.0f32..1.0),
                rng.gen_range(-1.0f32..1.0),
            )
            .normalize_or(Vec3::Y)
                * radius;
            // Velocity points back at the muzzle, so the mote falls inward
            // over its short life.
            let inward = -offset * (2.6 + charge * 3.4);
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(proj_assets.sphere_sm.clone()),
                    material: MeshMaterial3d(proj_assets.mat_charge_spark.clone()),
                    transform: Transform::from_translation(pos + offset)
                        .with_scale(Vec3::splat(0.22 + charge * 0.40)),
                    ..default()
                },
                HitParticle {
                    lifetime: 0.20,
                    max_lifetime: 0.20,
                    velocity: inward,
                },
            ));
        }

        // Core swell at the muzzle, brightest and largest at full charge.
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(proj_assets.flash_sphere.clone()),
                material: MeshMaterial3d(if ready {
                    proj_assets.mat_critical_hit.clone()
                } else {
                    proj_assets.mat_charge_spark.clone()
                }),
                transform: Transform::from_translation(pos)
                    .with_scale(Vec3::splat(0.30 + charge * 0.95)),
                ..default()
            },
            HitParticle {
                lifetime: 0.09,
                max_lifetime: 0.09,
                velocity: Vec3::ZERO,
            },
        ));

        // At full charge a ring snaps out once per frame as an unmistakable
        // "release me" tell.
        if ready {
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(proj_assets.lock_ring.clone()),
                    material: MeshMaterial3d(proj_assets.mat_critical_hit.clone()),
                    transform: Transform::from_translation(pos)
                        .looking_to(aim.direction, Vec3::Y)
                        .with_scale(Vec3::splat(0.85)),
                    ..default()
                },
                HitParticle {
                    lifetime: 0.16,
                    max_lifetime: 0.16,
                    velocity: Vec3::ZERO,
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

#[allow(dead_code)] // orphaned by the unlimited-ammo change; retained for ammo-cap re-enable
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

#[allow(dead_code)] // orphaned by the unlimited-ammo change; retained for ammo-cap re-enable
fn rescale_special_ammo_cap(weapon: &mut SpecialWeapon, ammo_mult: f32) {
    let base_max = SpecialWeapon::new(weapon.slot).max_ammo;
    rescale_ammo_cap(&mut weapon.ammo, &mut weapon.max_ammo, base_max, ammo_mult);
}

#[allow(dead_code)] // orphaned by the unlimited-ammo change; retained for ammo-cap re-enable
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
            &PlayerProgression,
        ),
        With<Player>,
    >,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    mut fired_ev: MessageWriter<WeaponFiredEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    for (player_entity, player_index, mut inv, pi, cam_ref, aim, armor, progression) in
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
                    text: format!("Selected {name} — RT to fire; D-pad Left returns to primaries"),
                    duration: 2.2,
                });
            }
            continue;
        }
        let Some(slot) = inv.active_slot else {
            continue;
        };
        if !pi.fire_just {
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
/// Makes an energy bolt breathe: a small scale pulse plus a brightness swell,
/// so shots read as contained plasma rather than moving props.
///
/// The base scale is captured at spawn because the per-weapon stretch already
/// encodes the bolt's shape — pulsing a *multiplier* keeps a long thin laser
/// long and thin while a fat orb stays fat.
#[derive(Component, Debug, Clone, Copy)]
struct ProjectilePulse {
    base_scale: Vec3,
    elapsed: f32,
    /// Radians/sec of the pulse. Deliberately fast: at projectile speeds a
    /// slow pulse never completes a cycle before impact.
    rate: f32,
    amplitude: f32,
}

impl ProjectilePulse {
    fn new(base_scale: Vec3) -> Self {
        Self {
            base_scale,
            elapsed: 0.0,
            rate: 17.0,
            amplitude: 0.16,
        }
    }

    /// Scale multiplier at the current phase, in 1±amplitude.
    fn scale_factor(&self) -> f32 {
        1.0 + (self.elapsed * self.rate).sin() * self.amplitude
    }
}

/// Drive the pulse. Runs before the projectile moves so a bolt is never drawn
/// at a stale size after its final step.
fn projectile_pulse_system(
    time: Res<Time>,
    mut pulses: Query<(&mut Transform, &mut ProjectilePulse)>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut pulse) in pulses.iter_mut() {
        pulse.elapsed += dt;
        let factor = pulse.scale_factor();
        // Swell mostly across the bolt, barely along it, so a laser pulses
        // thicker without visibly growing longer.
        transform.scale = Vec3::new(
            pulse.base_scale.x * factor,
            pulse.base_scale.y * factor,
            pulse.base_scale.z * (1.0 + (factor - 1.0) * 0.25),
        );
    }
}

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

fn sort_projectile_collisions(collisions: &mut [RayHitData]) {
    collisions.sort_by(|a, b| a.distance.total_cmp(&b.distance));
}

/// Continuous point-vs-sphere sweep used as a defensive damage proxy for
/// small, fast-moving drones. Avian remains the canonical collision path; this
/// closes the one-frame gap when a drone moves across a projectile ray between
/// physics synchronization and combat resolution.
fn segment_sphere_hit_fraction(start: Vec3, end: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let displacement = end - start;
    let from_center = start - center;
    let a = displacement.length_squared();
    if a <= 1e-8 {
        return (from_center.length_squared() <= radius * radius).then_some(0.0);
    }
    let c = from_center.length_squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let b = from_center.dot(displacement);
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let fraction = (-b - discriminant.sqrt()) / a;
    (0.0..=1.0).contains(&fraction).then_some(fraction)
}

fn drone_damage_proxy_radius(enemy_type: EnemyType) -> Option<f32> {
    match enemy_type {
        EnemyType::Drone => Some(2.8),
        EnemyType::SpyDrone => Some(3.6),
        _ => None,
    }
}

// ── Projectile Update ─────────────────────────────────────────────────────────
fn projectile_update_system(
    mut commands: Commands,
    time: Res<Time>,
    spatial_query: SpatialQuery,
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
    ignored_target_q: Query<(), Or<(With<DeadEnemy>, With<HackedUnit>)>>,
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
                    &spatial_query,
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
                    &spatial_query,
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
                    &spatial_query,
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
                    &spatial_query,
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

        let displacement = proj_transform.translation - previous_position;
        let query_filter =
            SpatialQueryFilter::from_mask([GameCollisionLayer::World, GameCollisionLayer::Enemy])
                .with_excluded_entities(proj.owner);
        let mut collisions = Dir3::new(displacement)
            .ok()
            .map(|direction| {
                spatial_query.ray_hits(
                    previous_position,
                    direction,
                    displacement.length(),
                    64,
                    true,
                    &query_filter,
                )
            })
            .unwrap_or_default();
        collisions.retain(|collision| !ignored_target_q.contains(collision.entity));
        sort_projectile_collisions(&mut collisions);

        let mut hit = false;
        let mut explosion: Option<(Vec3, f32, f32, DamageType)> = None;
        let nearest_physics_distance = collisions
            .first()
            .map(|collision| collision.distance)
            .unwrap_or(f32::INFINITY);
        let swept_drone = enemy_q
            .iter_mut()
            .filter(|(_, _, health, _, _)| health.is_alive())
            .filter_map(|(entity, transform, _, _, enemy)| {
                let radius = drone_damage_proxy_radius(enemy.enemy_type)?;
                let fraction = segment_sphere_hit_fraction(
                    previous_position,
                    proj_transform.translation,
                    transform.translation,
                    radius,
                )?;
                let distance = displacement.length() * fraction;
                (distance <= nearest_physics_distance + 0.001)
                    .then_some((distance, fraction, entity))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let forced_drone_entity = swept_drone.map(|(_, _, entity)| entity);

        if let Some((_, fraction, drone_entity)) = swept_drone {
            let impact_position = previous_position + displacement * fraction;
            if let Ok((e_entity, e_transform, mut e_health, mut e_damageable, enemy)) =
                enemy_q.get_mut(drone_entity)
            {
                if proj.is_explosive {
                    explosion = Some((
                        impact_position,
                        proj.explosion_radius,
                        proj.damage,
                        proj.damage_type,
                    ));
                } else {
                    let push = (e_transform.translation - impact_position)
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
                        position: impact_position,
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
                hit = proj.is_explosive || !proj.piercing;
                if hit {
                    proj_transform.translation = impact_position;
                }
            }
        }

        for collision in collisions {
            if hit || forced_drone_entity == Some(collision.entity) {
                continue;
            }
            let impact_position =
                previous_position + displacement.normalize_or_zero() * collision.distance;

            if let Ok((e_entity, e_transform, mut e_health, mut e_damageable, enemy)) =
                enemy_q.get_mut(collision.entity)
            {
                if e_health.is_alive() {
                    if proj.is_explosive {
                        explosion = Some((
                            impact_position,
                            proj.explosion_radius,
                            proj.damage,
                            proj.damage_type,
                        ));
                    } else {
                        let push = (e_transform.translation - impact_position)
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
                            position: impact_position,
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
                    hit = proj.is_explosive || !proj.piercing;
                    if hit {
                        proj_transform.translation = impact_position;
                        break;
                    }
                }
            } else if let Ok((_, _, mut health, mut damageable, _)) =
                road_vehicle_q.get_mut(collision.entity)
            {
                if health.is_alive() {
                    if proj.is_explosive {
                        explosion = Some((
                            impact_position,
                            proj.explosion_radius,
                            proj.damage,
                            proj.damage_type,
                        ));
                    } else {
                        apply_damage(
                            &mut health,
                            &mut damageable,
                            &DamageInfo::new(proj.damage, proj.damage_type),
                        );
                    }
                    hit = proj.is_explosive || !proj.piercing;
                    if hit {
                        proj_transform.translation = impact_position;
                        break;
                    }
                }
            } else {
                // The nearest collider is world geometry. Projectiles never
                // pass through it, including charged/piercing shots.
                if proj.is_explosive {
                    explosion = Some((
                        impact_position,
                        proj.explosion_radius,
                        proj.damage,
                        proj.damage_type,
                    ));
                }
                hit = true;
                proj_transform.translation = impact_position;
                break;
            }
        }

        if let Some((pos, radius, dmg, damage_type)) = explosion {
            explode(
                &spatial_query,
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
            damage_road_vehicles_in_radius(
                &spatial_query,
                &pos,
                radius,
                dmg,
                damage_type,
                &mut road_vehicle_q,
            );
        }
        if hit {
            commands.entity(proj_entity).despawn();
        }
    }
}

fn explode(
    spatial_query: &SpatialQuery,
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
        let target_point = e_transform.translation + Vec3::Y * 0.9;
        if dist <= radius
            && world_line_of_sight(spatial_query, *center, target_point, Some(e_entity))
        {
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
    spatial_query: &SpatialQuery,
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
    for (entity, transform, mut health, mut damageable, vehicle) in road_vehicle_q.iter_mut() {
        if !health.is_alive() {
            continue;
        }
        let dist = center.distance(transform.translation);
        if dist <= radius + vehicle.hit_radius
            && world_line_of_sight(
                spatial_query,
                *center,
                transform.translation + Vec3::Y * 0.5,
                Some(entity),
            )
        {
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
    spatial_query: SpatialQuery,
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
            Option<&BeamSabre>,
            &mut Damageable,
            Option<&TraversalModeState>,
        ),
        (With<Player>, Without<Enemy>),
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
    for (
        player_transform,
        mut combo,
        mut sm,
        pi,
        cam_ref,
        armor,
        progression,
        sabre,
        mut player_damageable,
        traversal,
    ) in player_q.iter_mut()
    {
        // Riding the board rebinds the melee buttons to flip/grab tricks.
        let board_claims_input = traversal
            .is_some_and(|t| crate::combat::tricks::hoverboard_claims_trick_input(t.active));
        let upgrades = &progression.upgrades;
        let Ok(cam) = cam_q.get(cam_ref.0) else {
            continue;
        };

        combo.light_timer = (combo.light_timer - dt).max(0.0);
        combo.heavy_timer = (combo.heavy_timer - dt).max(0.0);

        if pi.melee_light && !board_claims_input {
            combo.buffered_light = true;
        }
        if pi.melee_heavy && !board_claims_input && !sabre_claims_heavy_input(sabre, upgrades) {
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
                            &spatial_query,
                            cam_pos,
                            cam_fwd,
                            radius,
                            offset,
                            arc_cos,
                            damage,
                            melee_damage_type,
                            def.knockback,
                            Some(&mut combo.hit_entities),
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
                    let damage = def.damage
                        * combo.damage_multiplier
                        * armor_damage_mult
                        * blade_damage_mult;
                    let (radius, offset, arc_cos) = reach(active.chain);
                    let new_hits = execute_melee_hit(
                        &spatial_query,
                        cam_pos,
                        cam_fwd,
                        radius,
                        offset,
                        arc_cos,
                        damage,
                        melee_damage_type,
                        def.knockback,
                        Some(&mut combo.hit_entities),
                        &mut enemy_q,
                        &mut damaged_ev,
                        &mut killed_ev,
                    );
                    if new_hits > 0 {
                        hitstop.remaining = hitstop.remaining.max(def.hitstop);
                    }
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
                        combo.active = start_melee_move(
                            &library,
                            &mut combo,
                            chain,
                            &mut sm,
                            &mut player_damageable,
                        );
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
            combo.active = start_melee_move(
                &library,
                &mut combo,
                MeleeChain::Light,
                &mut sm,
                &mut player_damageable,
            );
        } else if do_heavy {
            combo.active = start_melee_move(
                &library,
                &mut combo,
                MeleeChain::Heavy,
                &mut sm,
                &mut player_damageable,
            );
        }
    }
}

/// While the Star Sabre is drawn and Cyclone Slash is owned, heavy input is
/// the cyclone verb — the fist Heavy chain must not also consume the press.
/// This is deliberately independent of the cyclone's cooldown so the outcome
/// does not depend on system ordering between the melee and sabre systems.
/// Which Saber technique the current input and stance request, if any.
///
/// Pure so the whole control scheme is testable without a world. Priority is
/// heavy → dodge → directional attack; a bare attack returns `None` and falls
/// through to the always-available slash chain, which is what keeps the base
/// moveset consistent while blueprints layer new verbs on top.
fn resolve_sabre_technique(
    upgrades: &UpgradeLedger,
    heavy: bool,
    dodge: bool,
    attack: bool,
    move_axis: Vec2,
    is_grounded: bool,
) -> Option<SabreTechnique> {
    const DIRECTION_DEAD_ZONE: f32 = 0.55;

    if heavy {
        if is_grounded && upgrades.sabre_spin_unlocked() {
            return Some(SabreTechnique::CycloneSlash);
        }
        if !is_grounded && upgrades.sabre_spiral_unlocked() {
            return Some(SabreTechnique::SpiralSlash);
        }
    }
    if dodge && upgrades.sabre_dodge_technique_applicable(is_grounded) {
        return Some(if !is_grounded && upgrades.sabre_pound_unlocked() {
            SabreTechnique::MeteorPound
        } else {
            SabreTechnique::CometDash
        });
    }
    // Directional attacks: holding up or down converts the swing into a
    // launcher or a throw. Both are grounded openers.
    if attack && is_grounded {
        if move_axis.y >= DIRECTION_DEAD_ZONE && upgrades.sabre_rising_unlocked() {
            return Some(SabreTechnique::RisingSlash);
        }
        if move_axis.y <= -DIRECTION_DEAD_ZONE && upgrades.sabre_throw_unlocked() {
            return Some(SabreTechnique::SabreThrow);
        }
    }
    None
}

/// Authored data for a technique.
fn sabre_technique_def(
    defs: &crate::combat::data::SabreTechniqueDefs,
    technique: SabreTechnique,
) -> Option<&crate::combat::data::SabreTechniqueDef> {
    Some(match technique {
        SabreTechnique::CycloneSlash => &defs.cyclone_slash,
        SabreTechnique::CometDash => &defs.comet_dash,
        SabreTechnique::MeteorPound => &defs.meteor_pound,
        SabreTechnique::RisingSlash => &defs.rising_slash,
        SabreTechnique::SpiralSlash => &defs.spiral_slash,
        SabreTechnique::SabreThrow => &defs.sabre_throw,
        SabreTechnique::Ready => return None,
    })
}

fn sabre_technique_sfx(technique: SabreTechnique) -> &'static str {
    match technique {
        SabreTechnique::CycloneSlash => "sabre.cyclone",
        SabreTechnique::CometDash => "sabre.comet_dash",
        SabreTechnique::MeteorPound => "sabre.meteor_pound",
        // The new verbs reuse the closest shipped one-shot until they get
        // dedicated samples.
        SabreTechnique::RisingSlash => "sabre.cyclone",
        SabreTechnique::SpiralSlash => "sabre.cyclone",
        SabreTechnique::SabreThrow => "sabre.wave",
        SabreTechnique::Ready => "sabre.cyclone",
    }
}

fn sabre_claims_heavy_input(sabre: Option<&BeamSabre>, upgrades: &UpgradeLedger) -> bool {
    sabre.is_some_and(|s| s.active) && upgrades.sabre_spin_unlocked()
}

fn sabre_recovery_should_advance(
    recovery: f32,
    cancel_after: f32,
    timer: f32,
    buffered_slash: bool,
) -> bool {
    let elapsed = (recovery - timer).max(0.0);
    timer <= 0.0 || buffered_slash && elapsed >= cancel_after
}

/// EC2 per-move i-frames: extend (never shorten) the shared invulnerability
/// window owned by `player_invulnerability_update`.
fn grant_iframes(damageable: &mut Damageable, iframes: f32) {
    if iframes > 0.0 {
        damageable.is_invulnerable = true;
        damageable.invulnerability_timer = damageable.invulnerability_timer.max(iframes);
    }
}

/// Begin a move's startup phase from the chain's current index.
fn start_melee_move(
    library: &MoveLibrary,
    combo: &mut MeleeCombo,
    chain: MeleeChain,
    sm: &mut PlayerStateMachine,
    damageable: &mut Damageable,
) -> Option<ActiveMelee> {
    let index = match chain {
        MeleeChain::Light => combo.light_index,
        MeleeChain::Heavy => combo.heavy_index,
    }
    .min(library.chain_len(chain).saturating_sub(1));
    let def = library.get(chain, index)?;
    combo.hit_entities.clear();
    sm.force(PlayerState::Attacking);
    grant_iframes(damageable, def.iframes);
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
    spatial_query: &SpatialQuery,
    origin: Vec3,
    forward: Vec3,
    radius: f32,
    offset: f32,
    arc_cos: f32,
    damage: f32,
    damage_type: DamageType,
    knockback: f32,
    mut already_hit: Option<&mut EntityHashSet>,
    enemy_q: &mut Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        Without<HackedUnit>,
    >,
    damaged_ev: &mut MessageWriter<EnemyDamagedEvent>,
    killed_ev: &mut MessageWriter<EnemyKilledEvent>,
) -> usize {
    let forward = forward.with_y(0.0).normalize_or_zero();
    let hit_center = origin + forward * offset;
    let hitbox = AvianCollider::sphere((radius + offset.abs()).max(0.1));
    let hitbox_layers = CollisionProfile::PlayerHitbox.layers();
    let filter = SpatialQueryFilter::from_mask(hitbox_layers.filters);
    let mut candidates =
        spatial_query.shape_intersections(&hitbox, origin, Quat::IDENTITY, &filter);
    candidates.sort_by_key(|entity| entity.to_bits());
    candidates.dedup();

    let mut hit_count = 0;
    for e_entity in candidates {
        if already_hit
            .as_ref()
            .is_some_and(|entities| entities.contains(&e_entity))
        {
            continue;
        }
        let Ok((_, e_transform, mut health, mut damageable, enemy)) = enemy_q.get_mut(e_entity)
        else {
            continue;
        };
        if !health.is_alive() {
            continue;
        }
        let to_enemy = (e_transform.translation - origin).with_y(0.0);
        let in_arc = to_enemy.length() <= radius + offset
            && to_enemy.normalize_or_zero().dot(forward) >= arc_cos;
        let within_hitbox = in_arc || hit_center.distance(e_transform.translation) <= radius;
        let unobstructed = world_line_of_sight(
            spatial_query,
            origin,
            e_transform.translation + Vec3::Y * 0.9,
            Some(e_entity),
        );
        if within_hitbox && unobstructed {
            if let Some(entities) = already_hit.as_deref_mut() {
                entities.insert(e_entity);
            }
            hit_count += 1;
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
    hit_count
}

// ── Star Sabre ────────────────────────────────────────────────────────────────
/// Level-scaling multipliers applied on top of the authored sabre MoveDefs:
/// `BeamSabre::set_level` keeps the level tables, and the `MoveLibrary`
/// carries the level-1 base numbers, so `(slash, wave)` scales stay 1.0 at
/// level 1 and edits to `moves.json` retune every level proportionally.
fn sabre_level_scale(sabre: &BeamSabre) -> (f32, f32) {
    (sabre.slash_damage / 25.0, sabre.wave_damage / 40.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SabreWaveProfile {
    projectile_count: usize,
    width: f32,
    length: f32,
    damage_mult: f32,
    speed_mult: f32,
    piercing: bool,
    explosive: bool,
}

fn sabre_wave_profile(
    sabre: &BeamSabre,
    upgrades: &UpgradeLedger,
    completed_slashes: u32,
    blade: &crate::combat::blades::BladeProfile,
) -> Option<SabreWaveProfile> {
    let tier = (upgrades.sabre_wave_upgrade_tier() + sabre.level.saturating_sub(1)).min(6);
    let is_second_slash = completed_slashes == 2;
    // The full starter combo now carries its momentum into a third-slash wave.
    // Tempest Wave Core remains useful by amplifying that third burst.
    let is_third_slash = completed_slashes == 3;
    let is_upgraded_fourth_slash = completed_slashes == 4 && tier >= 3;
    if !is_second_slash && !is_third_slash && !is_upgraded_fourth_slash {
        return None;
    }

    let tempest_boost = is_third_slash && upgrades.sabre_third_slash_wave();
    Some(SabreWaveProfile {
        projectile_count: ((1 + tier.div_ceil(2)) as usize + usize::from(tempest_boost)).min(4),
        width: 0.60 + tier as f32 * 0.11 + if tempest_boost { 0.18 } else { 0.0 },
        length: 1.80 + tier as f32 * 0.28,
        damage_mult: (0.30 + tier as f32 * 0.10) * if tempest_boost { 1.25 } else { 1.0 },
        speed_mult: 0.90 + tier as f32 * 0.04,
        // Blade traits grant these outright, so a wave-tuned hilt behaves like
        // a high-tier sabre even on a fresh save.
        piercing: sabre.is_piercing() || tier >= 3 || blade.trait_ == BladeTrait::PiercingWaves,
        explosive: sabre.has_aoe_splash()
            || tier >= 5
            || blade.trait_ == BladeTrait::ExplosiveWaves,
    })
}

fn sabre_vfx_material(
    upgrades: &UpgradeLedger,
    assets: &ProjectileAssets,
) -> Handle<StandardMaterial> {
    if upgrades.has_relic("solar_fire_gem") {
        assets.mat_melee_flash.clone()
    } else if upgrades.has_relic("storm_gem") {
        assets.mat_charge_laser.clone()
    } else if upgrades.has_relic("frost_gem") {
        assets.mat_energy.clone()
    } else if upgrades.has_relic("void_gem") {
        assets.mat_moon_bubble.clone()
    } else {
        assets.mat_homing_star.clone()
    }
}

fn sabre_wave_material(
    upgrades: &UpgradeLedger,
    assets: &ProjectileAssets,
) -> Handle<EnergyMaterial> {
    if upgrades.has_relic("solar_fire_gem") {
        assets.energy_explosive.clone()
    } else if upgrades.has_relic("storm_gem") {
        assets.energy_laser.clone()
    } else if upgrades.has_relic("frost_gem") {
        assets.energy_plasma.clone()
    } else if upgrades.has_relic("void_gem") {
        assets.energy_magic.clone()
    } else {
        assets.energy_sabre.clone()
    }
}

#[allow(clippy::too_many_arguments)]
fn try_spawn_sabre_vfx(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
    lifetime: f32,
    velocity: Vec3,
    spin: Vec3,
    expansion: f32,
    active: &mut usize,
    budget: usize,
) -> bool {
    if !reserve_sabre_vfx_slot(active, budget) {
        return false;
    }
    let base_scale = transform.scale;
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(mesh),
            material: MeshMaterial3d(material),
            transform,
            ..default()
        },
        SabreTechniqueVfx {
            lifetime,
            max_lifetime: lifetime,
            velocity,
            spin,
            base_scale,
            expansion,
        },
    ));
    true
}

fn reserve_sabre_vfx_slot(active: &mut usize, budget: usize) -> bool {
    if *active >= budget {
        return false;
    }
    *active += 1;
    true
}

#[allow(clippy::too_many_arguments)]
fn spawn_sabre_technique_vfx(
    commands: &mut Commands,
    assets: &ProjectileAssets,
    upgrades: &UpgradeLedger,
    technique: SabreTechnique,
    origin: Vec3,
    forward: Vec3,
    active: &mut usize,
    budget: usize,
) {
    let material = sabre_vfx_material(upgrades, assets);
    let right = Vec3::Y.cross(forward).normalize_or_zero();
    match technique {
        SabreTechnique::CycloneSlash => {
            for tilt in [-0.22_f32, 0.22] {
                try_spawn_sabre_vfx(
                    commands,
                    assets.lock_ring.clone(),
                    material.clone(),
                    Transform::from_translation(origin + Vec3::Y * 0.25)
                        .with_rotation(Quat::from_rotation_x(tilt))
                        .with_scale(Vec3::splat(3.6)),
                    0.44,
                    Vec3::Y * 0.35,
                    Vec3::new(0.0, 9.0, 0.0),
                    0.38,
                    active,
                    budget,
                );
            }
        }
        SabreTechnique::CometDash => {
            for (distance, side) in [(0.8, -0.55), (2.8, 0.55), (4.8, -0.35)] {
                let position = origin + forward * distance + right * side;
                try_spawn_sabre_vfx(
                    commands,
                    assets.sphere_lg.clone(),
                    material.clone(),
                    Transform::from_translation(position)
                        .looking_to(forward, Vec3::Y)
                        .with_scale(Vec3::new(0.65, 0.28, 3.4)),
                    0.30,
                    forward * 8.0,
                    Vec3::ZERO,
                    0.12,
                    active,
                    budget,
                );
            }
        }
        SabreTechnique::MeteorPound => {
            try_spawn_sabre_vfx(
                commands,
                assets.lock_ring.clone(),
                material.clone(),
                Transform::from_translation(origin - Vec3::Y * 0.65).with_scale(Vec3::splat(4.2)),
                0.52,
                Vec3::NEG_Y * 1.5,
                Vec3::new(0.0, 5.0, 0.0),
                0.75,
                active,
                budget,
            );
            try_spawn_sabre_vfx(
                commands,
                assets.sphere_lg.clone(),
                material,
                Transform::from_translation(origin - Vec3::Y * 0.1)
                    .with_scale(Vec3::new(1.1, 3.8, 1.1)),
                0.40,
                Vec3::NEG_Y * 5.0,
                Vec3::new(0.0, 3.0, 0.0),
                0.25,
                active,
                budget,
            );
        }
        // Rising Slash: a vertical blade column climbing with the rider.
        SabreTechnique::RisingSlash => {
            try_spawn_sabre_vfx(
                commands,
                assets.sphere_lg.clone(),
                material.clone(),
                Transform::from_translation(origin + forward * 0.6)
                    .with_scale(Vec3::new(0.9, 4.2, 0.9)),
                0.42,
                Vec3::Y * 6.5,
                Vec3::new(0.0, 6.0, 0.0),
                0.30,
                active,
                budget,
            );
            try_spawn_sabre_vfx(
                commands,
                assets.lock_ring.clone(),
                material,
                Transform::from_translation(origin - Vec3::Y * 0.4).with_scale(Vec3::splat(2.4)),
                0.34,
                Vec3::Y * 2.2,
                Vec3::new(0.0, 7.0, 0.0),
                0.45,
                active,
                budget,
            );
        }
        // Spiral Slash: stacked rings corkscrewing around the rider.
        SabreTechnique::SpiralSlash => {
            for (index, height) in [-0.5_f32, 0.15, 0.8].into_iter().enumerate() {
                try_spawn_sabre_vfx(
                    commands,
                    assets.lock_ring.clone(),
                    material.clone(),
                    Transform::from_translation(origin + Vec3::Y * height)
                        .with_rotation(Quat::from_rotation_y(index as f32 * 0.7))
                        .with_scale(Vec3::splat(3.0 + index as f32 * 0.35)),
                    0.46,
                    Vec3::Y * 1.4,
                    Vec3::new(0.0, 12.0, 0.0),
                    0.40,
                    active,
                    budget,
                );
            }
        }
        // Sabre Throw: the blade streaking away from the hand.
        SabreTechnique::SabreThrow => {
            try_spawn_sabre_vfx(
                commands,
                assets.sphere_lg.clone(),
                material,
                Transform::from_translation(origin + forward * 1.2)
                    .looking_to(forward, Vec3::Y)
                    .with_scale(Vec3::new(0.5, 0.5, 3.0)),
                0.36,
                forward * 14.0,
                Vec3::new(0.0, 0.0, 18.0),
                0.20,
                active,
                budget,
            );
        }
        SabreTechnique::Ready => {}
    }

    if technique != SabreTechnique::Ready && upgrades.has_relic("legendary_starheart_gem") {
        try_spawn_sabre_vfx(
            commands,
            assets.lock_ring.clone(),
            assets.mat_critical_hit.clone(),
            Transform::from_translation(origin + Vec3::Y * 0.35)
                .with_rotation(Quat::from_rotation_z(0.78))
                .with_scale(Vec3::splat(2.2)),
            0.34,
            Vec3::Y * 0.8,
            Vec3::new(4.0, 7.0, 2.0),
            0.55,
            active,
            budget,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_sabre_wave_vfx(
    commands: &mut Commands,
    assets: &ProjectileAssets,
    upgrades: &UpgradeLedger,
    origin: Vec3,
    forward: Vec3,
    active: &mut usize,
    budget: usize,
) {
    try_spawn_sabre_vfx(
        commands,
        assets.lock_ring.clone(),
        sabre_vfx_material(upgrades, assets),
        Transform::from_translation(origin + forward * 0.55)
            .looking_to(forward, Vec3::Y)
            .with_scale(Vec3::splat(1.45)),
        0.24,
        forward * 2.0,
        Vec3::new(0.0, 0.0, 8.0),
        0.45,
        active,
        budget,
    );

    if upgrades.has_relic("legendary_starheart_gem") {
        try_spawn_sabre_vfx(
            commands,
            assets.lock_ring.clone(),
            assets.mat_critical_hit.clone(),
            Transform::from_translation(origin + forward * 0.45)
                .looking_to(forward, Vec3::Y)
                .with_rotation(Quat::from_rotation_z(0.78))
                .with_scale(Vec3::splat(1.05)),
            0.28,
            forward * 2.5,
            Vec3::new(0.0, 0.0, -9.0),
            0.65,
            active,
            budget,
        );
    }
}

/// Hurl the blade as a piercing projectile (Sabre Throw). It rides the same
/// projectile pipeline as every other player shot, so world cover, sweeps, and
/// damage resolution all behave; piercing lets it cut a line of enemies the
/// way a thrown blade should.
#[allow(clippy::too_many_arguments)]
fn spawn_sabre_thrown_blade(
    commands: &mut Commands,
    assets: &ProjectileAssets,
    upgrades: &UpgradeLedger,
    origin: Vec3,
    forward: Vec3,
    owner: Entity,
    damage: f32,
    damage_type: DamageType,
    tech: &crate::combat::data::SabreTechniqueDef,
) {
    let direction = forward.normalize_or(Vec3::NEG_Z);
    commands.spawn((
        EnergyPbrBundle {
            mesh: Mesh3d(assets.sphere_md.clone()),
            material: MeshMaterial3d(sabre_wave_material(upgrades, assets)),
            transform: Transform::from_translation(origin)
                .looking_to(direction, Vec3::Y)
                .with_scale(Vec3::new(0.34, 0.34, 2.6)),
            ..default()
        },
        Projectile {
            damage,
            damage_type,
            speed: tech.throw_speed,
            direction,
            lifetime: tech.throw_lifetime,
            is_explosive: false,
            explosion_radius: 0.0,
            weapon_type: ProjectileOwner::Player,
            owner: Some(owner),
            piercing: true,
            gravity_affected: false,
            vertical_velocity: 0.0,
        },
    ));
}

fn spawn_sabre_wave_attack(
    commands: &mut Commands,
    assets: &ProjectileAssets,
    upgrades: &UpgradeLedger,
    wave: &RangedMoveDef,
    profile: SabreWaveProfile,
    origin: Vec3,
    forward: Vec3,
    right: Vec3,
    owner: Entity,
    damage_scale: f32,
    damage_type: DamageType,
    dungeon_active: bool,
    active_vfx: &mut usize,
    vfx_budget: usize,
) {
    let material = sabre_wave_material(upgrades, assets);
    let spread = 0.16 + profile.width * 0.08;
    let offsets: &[f32] = match profile.projectile_count {
        1 => &[0.0],
        2 => &[-spread, spread],
        3 => &[-spread, 0.0, spread],
        _ => &[-spread * 1.5, -spread * 0.5, spread * 0.5, spread * 1.5],
    };
    let launch_origin = origin + forward * 2.2;
    for offset in offsets {
        let direction = (forward + right * *offset).with_y(0.0).normalize_or_zero();
        commands.spawn((
            EnergyPbrBundle {
                mesh: Mesh3d(assets.sphere_md.clone()),
                material: MeshMaterial3d(material.clone()),
                transform: Transform::from_translation(launch_origin)
                    .looking_to(direction, Vec3::Y)
                    .with_scale(Vec3::new(
                        profile.width,
                        profile.width * 1.15,
                        profile.length,
                    )),
                ..default()
            },
            Projectile {
                damage: wave.damage
                    * damage_scale
                    * profile.damage_mult
                    * if dungeon_active { 0.72 } else { 1.0 },
                damage_type,
                speed: wave.projectile_speed * profile.speed_mult,
                direction,
                lifetime: wave.projectile_lifetime,
                is_explosive: profile.explosive,
                explosion_radius: if profile.explosive {
                    wave.explosion_radius * (0.65 + profile.width * 0.35)
                } else {
                    0.0
                },
                weapon_type: ProjectileOwner::Player,
                owner: Some(owner),
                piercing: profile.piercing,
                gravity_affected: false,
                vertical_velocity: 0.0,
            },
        ));
    }
    spawn_sabre_wave_vfx(
        commands,
        assets,
        upgrades,
        launch_origin,
        forward,
        active_vfx,
        vfx_budget,
    );
}

#[allow(clippy::too_many_arguments)]
fn beam_sabre_update_system(
    time: Res<Time>,
    spatial_query: SpatialQuery,
    mut commands: Commands,
    proj_assets: Res<ProjectileAssets>,
    library: Res<MoveLibrary>,
    mut hitstop: ResMut<HitstopState>,
    dungeon: Res<DungeonCrawlState>,
    vfx_budget: Res<SabreVfxBudget>,
    vfx_q: Query<Entity, With<SabreTechniqueVfx>>,
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
            &mut PlayerMovement,
            &TraversalModeState,
            Option<&BeamSabreLocked>,
            (&mut Damageable, &mut Health),
        ),
        (With<Player>, Without<Enemy>),
    >,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    mut enemy_q: Query<
        (Entity, &Transform, &mut Health, &mut Damageable, &Enemy),
        Without<HackedUnit>,
    >,
    mut damaged_ev: MessageWriter<EnemyDamagedEvent>,
    mut killed_ev: MessageWriter<EnemyKilledEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
    mut action_sfx: MessageWriter<ModularActionSfxEvent>,
) {
    let dt = time.delta_secs();
    let mut active_vfx = vfx_q.iter().count();
    for (
        entity,
        player_transform,
        mut sabre,
        mut sm,
        pi,
        cam_ref,
        armor,
        progression,
        mut movement,
        traversal,
        locked_marker,
        (mut player_damageable, mut player_health),
    ) in player_q.iter_mut()
    {
        let upgrades = &progression.upgrades;
        // The blade bought in the shop; falls back to the neutral starter.
        let equipped_blade = blade_for_id(progression.shop.equipped_weapon.as_deref());
        let perk_damage_mult = progression.perks.damage_mult()
            * upgrades.beam_damage_mult()
            * upgrades.gauntlet_energy_damage_mult()
            * upgrades.sabre_elemental_damage_mult();
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
        let blade_damage_type = if upgrades.has_relic("solar_fire_gem") {
            DamageType::Fire
        } else if upgrades.has_relic("storm_gem") {
            DamageType::Electric
        } else if upgrades.has_relic("void_gem") {
            DamageType::Rift
        } else if upgrades.blade_boot_rank() > 0 {
            DamageType::Laser
        } else {
            DamageType::Melee
        };
        let wave_damage_type = gauntlet_projectile_damage_type(upgrades, DamageType::Laser);

        if pi.sabre_toggle {
            if sabre.unlocked {
                sabre.active = !sabre.active;
                if !sabre.active {
                    sabre.is_slashing = false;
                    sabre.slash_index = 0;
                    sabre.slash_hits.clear();
                    sabre.buffered_slash = false;
                    sabre.technique_timer = 0.0;
                    sabre.technique = SabreTechnique::Ready;
                }
                msg_ev.write(UiMessageEvent {
                    text: if sabre.active {
                        "Star Sabre active — RB to slash; RT fires beam".into()
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
        sabre.technique_timer = (sabre.technique_timer - dt).max(0.0);
        if sabre.technique_timer <= 0.0 {
            sabre.technique = SabreTechnique::Ready;
        }
        if sabre.is_slashing && pi.sabre_attack {
            sabre.buffered_slash = true;
        }

        // ── Slash lifecycle: the same Startup→Active→Recovery machine as the
        // melee chains, with a persistent hit window and per-slash target
        // dedup. The chain auto-advances through the level-scaled slash count.
        if sabre.is_slashing {
            let Some(def) = library.sabre_slash(sabre.slash_index as usize) else {
                sabre.is_slashing = false;
                sabre.slash_index = 0;
                sm.transition(PlayerState::Idle);
                continue;
            };
            let radius = if dungeon.active { 6.4 } else { 4.6 };
            let offset = if dungeon.active { 2.5 } else { 3.1 };
            let arc_cos = if dungeon.active { -0.40 } else { 0.10 };
            let damage = def.damage * slash_scale * armor_damage_mult;
            sabre.slash_timer -= dt;
            match sabre.slash_phase {
                MeleePhase::Startup => {
                    if sabre.slash_timer <= 0.0 {
                        // The strike lands at the start of the active window.
                        execute_melee_hit(
                            &spatial_query,
                            origin,
                            fwd,
                            radius,
                            offset,
                            arc_cos,
                            damage,
                            blade_damage_type,
                            def.knockback,
                            Some(&mut sabre.slash_hits),
                            &mut enemy_q,
                            &mut damaged_ev,
                            &mut killed_ev,
                        );
                        // Lifesteal blades return a slice of the strike as
                        // health, capped by the player's own maximum.
                        let steal = equipped_blade.trait_.lifesteal_fraction();
                        if steal > 0.0 {
                            player_health.current =
                                (player_health.current + damage * steal).min(player_health.max);
                        }
                        let completed_slashes = sabre.slash_index + 1;
                        if let Some(profile) =
                            sabre_wave_profile(&sabre, upgrades, completed_slashes, &equipped_blade)
                        {
                            action_sfx.write(ModularActionSfxEvent::new("sabre.wave"));
                            spawn_sabre_wave_attack(
                                &mut commands,
                                &proj_assets,
                                upgrades,
                                &library.sabre_wave,
                                profile,
                                origin,
                                fwd,
                                cam.right().as_vec3().with_y(0.0).normalize_or_zero(),
                                entity,
                                wave_scale * armor_damage_mult,
                                wave_damage_type,
                                dungeon.active,
                                &mut active_vfx,
                                vfx_budget.max_entities,
                            );
                        }
                        spawn_melee_flash(&mut commands, &proj_assets, origin + fwd * 3.1);
                        hitstop.remaining = hitstop.remaining.max(def.hitstop);
                        sabre.slash_phase = MeleePhase::Active;
                        sabre.slash_timer = def.active;
                    }
                }
                MeleePhase::Active => {
                    // The window persists: enemies stepping into the arc are
                    // struck, while `slash_hits` keeps one hit per target.
                    let new_hits = execute_melee_hit(
                        &spatial_query,
                        origin,
                        fwd,
                        radius,
                        offset,
                        arc_cos,
                        damage,
                        blade_damage_type,
                        def.knockback,
                        Some(&mut sabre.slash_hits),
                        &mut enemy_q,
                        &mut damaged_ev,
                        &mut killed_ev,
                    );
                    if new_hits > 0 {
                        hitstop.remaining = hitstop.remaining.max(def.hitstop);
                    }
                    if sabre.slash_timer <= 0.0 {
                        sabre.slash_phase = MeleePhase::Recovery;
                        sabre.slash_timer = def.recovery;
                    }
                }
                MeleePhase::Recovery => {
                    if sabre_recovery_should_advance(
                        def.recovery,
                        def.cancel_after,
                        sabre.slash_timer,
                        sabre.buffered_slash,
                    ) {
                        sabre.buffered_slash = false;
                        sabre.slash_index += 1;
                        match library
                            .sabre_slash(sabre.slash_index as usize)
                            .filter(|_| sabre.slash_index < sabre.slash_count)
                        {
                            Some(next) => {
                                sabre.slash_hits.clear();
                                sabre.slash_phase = MeleePhase::Startup;
                                sabre.slash_timer = next.startup;
                                grant_iframes(&mut player_damageable, next.iframes);
                            }
                            None => {
                                sabre.is_slashing = false;
                                sabre.slash_index = 0;
                                sabre.slash_hits.clear();
                                sm.transition(PlayerState::Idle);
                            }
                        }
                    }
                }
            }
            continue;
        }

        // ── Technique verbs ──────────────────────────────────────────────────
        // Blueprints layer new verbs onto the always-available slash chain:
        // heavy spins (cyclone grounded / spiral airborne), dodge dashes or
        // pounds, and holding up/down on the attack launches or throws.
        // Riding the board rebinds these inputs to hoverboard tricks.
        let requested_technique = if traversal.active == TraversalMode::Hoverboard {
            None
        } else {
            resolve_sabre_technique(
                upgrades,
                pi.melee_heavy,
                pi.dodge,
                pi.sabre_attack,
                pi.move_axis,
                movement.is_grounded,
            )
        };
        if let Some(technique) = requested_technique.filter(|_| sabre.cooldown_timer <= 0.0) {
            let (Some(def), Some(tech)) = (
                library.sabre_slash(0),
                sabre_technique_def(&library.sabre_techniques, technique),
            ) else {
                continue;
            };
            // Movement impulses: plunge, dash, or launch.
            if tech.plunge_speed > 0.0 {
                movement.velocity.y = -tech.plunge_speed;
            }
            if tech.dash_speed > 0.0 {
                movement.ground_velocity = fwd * tech.dash_speed;
            }
            if tech.rise_speed > 0.0 {
                movement.velocity.y = tech.rise_speed;
                movement.is_grounded = false;
            }

            let damage = def.damage * slash_scale * armor_damage_mult * tech.damage_mult;
            if tech.throw_speed > 0.0 {
                // Thrown blade: a projectile carries the hit instead of an arc.
                spawn_sabre_thrown_blade(
                    &mut commands,
                    &proj_assets,
                    upgrades,
                    origin + fwd * 1.1,
                    fwd,
                    entity,
                    damage,
                    blade_damage_type,
                    tech,
                );
            } else {
                for strike in &tech.strikes {
                    execute_melee_hit(
                        &spatial_query,
                        origin + fwd * *strike,
                        fwd,
                        tech.radius,
                        tech.hit_offset,
                        tech.arc_cos,
                        damage,
                        blade_damage_type,
                        def.knockback * tech.knockback_mult,
                        None,
                        &mut enemy_q,
                        &mut damaged_ev,
                        &mut killed_ev,
                    );
                }
            }

            sabre.technique = technique;
            sabre.cooldown_timer = sabre.cooldown
                * tech.cooldown_mult
                * equipped_blade.trait_.technique_cooldown_mult();
            sabre.technique_timer = tech.technique_time;
            grant_iframes(&mut player_damageable, tech.iframes);
            sm.force(PlayerState::Attacking);
            spawn_sabre_technique_vfx(
                &mut commands,
                &proj_assets,
                upgrades,
                technique,
                origin,
                fwd,
                &mut active_vfx,
                vfx_budget.max_entities,
            );
            spawn_melee_flash(&mut commands, &proj_assets, origin + fwd * 3.0);
            hitstop.remaining = hitstop.remaining.max(tech.hitstop);
            action_sfx.write(ModularActionSfxEvent::new(sabre_technique_sfx(technique)));
            continue;
        }

        if pi.sabre_attack && sabre.cooldown_timer <= 0.0 {
            let Some(def) = library.sabre_slash(0) else {
                continue;
            };
            // Begin the first slash's startup; the strike itself lands when
            // the lifecycle above reaches the active window.
            sabre.is_slashing = true;
            sabre.slash_index = 0;
            sabre.slash_phase = MeleePhase::Startup;
            sabre.slash_timer = def.startup;
            sabre.slash_hits.clear();
            sabre.buffered_slash = false;
            sabre.cooldown_timer = sabre.cooldown;
            grant_iframes(&mut player_damageable, def.iframes);
            sm.force(PlayerState::Attacking);
        }
    }
}

/// Attach or remove the physical hilt on each player's sword hand.
///
/// The hilt is parented to the character's real hand entity — the rigged
/// `RightWrist` joint when the character has a skeleton, otherwise the
/// cartoon `RightHand` part — so it follows the animated arm exactly instead
/// of being placed by guesswork from the player root. That also means body
/// proportions edited in the designer move the weapon correctly for free.
#[allow(clippy::too_many_arguments)]
fn mount_sabre_hilt_system(
    mut commands: Commands,
    assets: Res<ProjectileAssets>,
    player_q: Query<(Entity, &BeamSabre, &PlayerProgression), With<Player>>,
    joint_q: Query<(Entity, &JointMarker)>,
    part_q: Query<(Entity, &CartoonPart)>,
    hilt_q: Query<(Entity, &SabreHilt)>,
) {
    // A hilt whose carry state no longer matches (drawn <-> holstered) is
    // dropped so it can be re-mounted on the correct anchor this frame.
    let mut mounted: Vec<Entity> = Vec::new();
    for (hilt_entity, hilt) in hilt_q.iter() {
        let wanted = player_q
            .get(hilt.owner)
            .ok()
            .filter(|(_, sabre, _)| sabre.unlocked)
            .map(|(_, sabre, _)| {
                if sabre.active {
                    HiltCarry::Drawn
                } else {
                    HiltCarry::Holstered
                }
            });
        if wanted == Some(hilt.carry) {
            mounted.push(hilt.owner);
        } else {
            commands.entity(hilt_entity).despawn();
        }
    }

    for (player_entity, sabre, progression) in player_q.iter() {
        if !sabre.unlocked || mounted.contains(&player_entity) {
            continue;
        }
        let carry = if sabre.active {
            HiltCarry::Drawn
        } else {
            HiltCarry::Holstered
        };
        // Find this character's hand. Rigs win over cartoon parts.
        let wrist = joint_q.iter().find(|(_, marker)| {
            marker.root == player_entity && marker.kind == JointKind::RightWrist
        });
        let hand_part = part_q.iter().find(|(_, part)| {
            part.root == player_entity && part.kind == CartoonPartKind::RightHand
        });
        let pelvis = joint_q
            .iter()
            .find(|(_, marker)| marker.root == player_entity && marker.kind == JointKind::Pelvis);
        let belt = part_q
            .iter()
            .find(|(_, part)| part.root == player_entity && part.kind == CartoonPartKind::Belt);
        let Some(mount) = resolve_hand_mount(
            carry,
            wrist.is_some(),
            hand_part.is_some(),
            pelvis.is_some(),
            belt.is_some(),
        ) else {
            // Anchor not spawned yet (mesh still assembling) — retry next frame.
            continue;
        };
        let parent = match mount {
            HandMount::Wrist => wrist.map(|(entity, _)| entity),
            HandMount::HandPart => hand_part.map(|(entity, _)| entity),
            HandMount::HipJoint => pelvis.map(|(entity, _)| entity),
            HandMount::BeltPart => belt.map(|(entity, _)| entity),
        };
        let Some(parent) = parent else { continue };

        let blade = blade_for_id(progression.shop.equipped_weapon.as_deref());
        let accent = assets
            .energy_sabre_blades
            .get(blade.color.index())
            .cloned()
            .unwrap_or_else(|| assets.energy_sabre.clone());

        commands.entity(parent).with_children(|hand| {
            hand.spawn((
                SpatialBundle {
                    transform: hilt_local_transform(mount),
                    ..default()
                },
                SabreHilt {
                    owner: player_entity,
                    // Derived from the resolved mount rather than the request,
                    // so a hip anchor can never be recorded as "in hand".
                    carry: mount.carry(),
                },
                Name::new("Star Sabre Hilt"),
            ))
            .with_children(|hilt| {
                // Grip — the part inside the fist.
                hilt.spawn(PbrBundle {
                    mesh: Mesh3d(assets.hilt_grip.clone()),
                    material: MeshMaterial3d(assets.mat_missile_lock.clone()),
                    transform: Transform::default(),
                    ..default()
                });
                // Emitter ring at the blade end, tinted to the equipped blade
                // so the handle reads as part of the same weapon.
                hilt.spawn(EnergyPbrBundle {
                    mesh: Mesh3d(assets.hilt_ring.clone()),
                    material: MeshMaterial3d(accent),
                    transform: Transform::from_xyz(0.0, 0.145, 0.0),
                    ..default()
                });
                // Pommel counterweight at the base.
                hilt.spawn(PbrBundle {
                    mesh: Mesh3d(assets.hilt_pommel.clone()),
                    material: MeshMaterial3d(assets.mat_missile_lock.clone()),
                    transform: Transform::from_xyz(0.0, -0.142, 0.0),
                    ..default()
                });
            });
        });
    }
}

/// Keep the energy blade attached to the hilt's emitter.
///
/// The blade is a child of the hilt, which is a child of the hand, so its
/// transform is purely local: it projects straight out of the emitter and
/// inherits the entire arm animation. Ignition/retraction is a length
/// animation along that local axis, which is what makes the blade look like
/// it is being *extended from* the handle rather than swapped in.
fn sync_sabre_blade_visual(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<ProjectileAssets>,
    player_q: Query<(&BeamSabre, &PlayerProgression), With<Player>>,
    hilt_q: Query<(Entity, &SabreHilt)>,
    mut visual_q: Query<(Entity, &mut SabreBladeVisual, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let mut represented = Vec::new();

    for (visual_entity, mut visual, mut transform) in visual_q.iter_mut() {
        let Ok((sabre, _)) = player_q.get(visual.owner) else {
            commands.entity(visual_entity).despawn();
            continue;
        };
        if !sabre.active || !sabre.unlocked {
            commands.entity(visual_entity).despawn();
            continue;
        }
        represented.push((visual.owner, visual.layer));
        // Ignite quickly on draw; the blade grows out of the emitter.
        visual.ignition = (visual.ignition + dt * 6.0).min(1.0);
        *transform = sabre_blade_local_transform(visual.layer, visual.ignition);
    }

    for (hilt_entity, hilt) in hilt_q.iter() {
        let Ok((sabre, progression)) = player_q.get(hilt.owner) else {
            continue;
        };
        // A holstered hilt is just the handle — no beam.
        if !sabre.active || !sabre.unlocked || hilt.carry != HiltCarry::Drawn {
            continue;
        }
        let blade = blade_for_id(progression.shop.equipped_weapon.as_deref());
        for layer in [SabreBladeLayer::Aura, SabreBladeLayer::Core] {
            if represented.contains(&(hilt.owner, layer)) {
                continue;
            }
            let material = match layer {
                SabreBladeLayer::Aura => assets
                    .energy_sabre_blades
                    .get(blade.color.index())
                    .cloned()
                    .unwrap_or_else(|| assets.energy_sabre.clone()),
                SabreBladeLayer::Core => assets.energy_sabre_core.clone(),
            };
            commands.entity(hilt_entity).with_children(|hilt_root| {
                hilt_root.spawn((
                    EnergyPbrBundle {
                        mesh: Mesh3d(assets.sphere_sm.clone()),
                        material: MeshMaterial3d(material),
                        transform: sabre_blade_local_transform(layer, 0.0),
                        ..default()
                    },
                    SabreBladeVisual {
                        owner: hilt.owner,
                        layer,
                        ignition: 0.0,
                    },
                ));
            });
        }
    }
}

/// Blade geometry in **hilt-local** space.
///
/// The old version computed a world transform from the player root plus fixed
/// offsets, which is why the blade never followed the animated arm. Now the
/// hilt provides position and orientation through the transform hierarchy, so
/// this only has to describe the beam itself: a thin column projecting along
/// the grip's axis, scaled by how far it has ignited.
fn sabre_blade_local_transform(layer: SabreBladeLayer, ignition: f32) -> Transform {
    let extend = ignition.clamp(0.0, 1.0);
    // Base mesh is a 0.08-radius sphere, so these scales read as blade
    // thickness in metres.
    let (thickness, length) = match layer {
        SabreBladeLayer::Aura => (0.55, 15.0),
        SabreBladeLayer::Core => (0.30, 14.4),
    };
    let length = length * extend;
    Transform::from_translation(Vec3::new(0.0, 0.145 + length * 0.08, 0.0))
        .with_scale(Vec3::new(thickness, length.max(0.001), thickness))
}

fn sabre_technique_vfx_system(
    mut commands: Commands,
    time: Res<Time>,
    mut vfx_q: Query<(Entity, &mut Transform, &mut SabreTechniqueVfx)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut vfx) in vfx_q.iter_mut() {
        vfx.lifetime -= dt;
        if vfx.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        transform.translation += vfx.velocity * dt;
        transform.rotate(Quat::from_euler(
            EulerRot::XYZ,
            vfx.spin.x * dt,
            vfx.spin.y * dt,
            vfx.spin.z * dt,
        ));
        let remaining = (vfx.lifetime / vfx.max_lifetime).clamp(0.0, 1.0);
        let progress = 1.0 - remaining;
        transform.scale = vfx.base_scale * (1.0 + vfx.expansion * progress) * remaining.sqrt();
    }
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
    fn swept_collisions_are_resolved_nearest_first() {
        let mut world = World::new();
        let near = world.spawn_empty().id();
        let far = world.spawn_empty().id();
        let mut collisions = vec![
            RayHitData {
                entity: far,
                distance: 14.0,
                normal: Vec3::NEG_Z,
            },
            RayHitData {
                entity: near,
                distance: 3.0,
                normal: Vec3::NEG_Z,
            },
        ];

        sort_projectile_collisions(&mut collisions);

        assert_eq!(collisions[0].entity, near);
        assert_eq!(collisions[1].entity, far);
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
    fn airborne_aim_assist_targets_hurtbox_center_with_wider_cone() {
        let root = Vec3::new(4.0, 32.0, -8.0);
        assert_eq!(aim_assist_target(root, true), root);
        assert_eq!(aim_assist_target(root, false), root + Vec3::Y * 0.9);
        assert!(aim_assist_cone_cos(0.78, true) < aim_assist_cone_cos(0.78, false));
        assert_eq!(aim_assist_cone_cos(0.62, true), 0.58);
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
    fn special_cycle_wraps_through_none_so_primary_is_always_reachable() {
        // Forward: none → 0 → 1 → 2 → 3 → none
        let mut slot = None;
        let forward: Vec<Option<u8>> = (0..5)
            .map(|_| {
                slot = cycle_special_slot(slot, true);
                slot
            })
            .collect();
        assert_eq!(
            forward,
            vec![Some(0), Some(1), Some(2), Some(3), None],
            "forward cycle must return to the primary weapon"
        );

        // Backward is the exact mirror.
        let mut slot = None;
        let backward: Vec<Option<u8>> = (0..5)
            .map(|_| {
                slot = cycle_special_slot(slot, false);
                slot
            })
            .collect();
        assert_eq!(backward, vec![Some(3), Some(2), Some(1), Some(0), None]);

        // Opposite directions undo each other.
        assert_eq!(
            cycle_special_slot(cycle_special_slot(Some(2), true), false),
            Some(2)
        );
    }

    #[test]
    fn dpad_special_cycle_swaps_the_equipped_special_without_touching_primaries() {
        let mut app = App::new();
        app.add_message::<WeaponSwitchedEvent>();
        app.add_systems(Update, weapon_select_system);
        let entity = app
            .world_mut()
            .spawn((
                Player,
                PlayerInput {
                    special_next: true,
                    ..Default::default()
                },
                WeaponInventory::default(),
                SpecialWeaponInventory::default(),
            ))
            .id();

        app.update();

        let specials = app.world().get::<SpecialWeaponInventory>(entity).unwrap();
        let primaries = app.world().get::<WeaponInventory>(entity).unwrap();
        assert_eq!(specials.active_slot, Some(0), "D-pad up equips a special");
        assert_eq!(primaries.active_slot, 0, "primary selection is untouched");
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

    /// Minimal app with just the asset stores the weapon setup needs.
    fn hilt_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<EnergyMaterial>()
            .add_systems(Startup, setup_weapon_assets)
            .add_systems(Update, mount_sabre_hilt_system);
        app
    }

    /// Spawns a player plus the hand *and* belt anchors, so both the drawn
    /// and holstered mounts have somewhere to go.
    fn spawn_sabre_player_with_belt(app: &mut App, active: bool) -> (Entity, Entity, Entity) {
        let (player, hand) = spawn_sabre_player(app, active);
        let belt = app
            .world_mut()
            .spawn((
                Transform::default(),
                CartoonPart::new(player, CartoonPartKind::Belt, &Transform::default()),
            ))
            .id();
        (player, hand, belt)
    }

    fn spawn_sabre_player(app: &mut App, active: bool) -> (Entity, Entity) {
        let player = app
            .world_mut()
            .spawn((
                Player,
                BeamSabre {
                    active,
                    unlocked: true,
                    ..Default::default()
                },
                PlayerProgression::default(),
            ))
            .id();
        // The character's hand mesh, as the modular assembler would spawn it.
        let hand = app
            .world_mut()
            .spawn((
                Transform::default(),
                CartoonPart::new(
                    player,
                    CartoonPartKind::RightHand,
                    &Transform::default(),
                ),
            ))
            .id();
        (player, hand)
    }

    #[test]
    fn drawing_the_sabre_puts_a_hilt_in_the_characters_actual_hand() {
        let mut app = hilt_test_app();
        let (player, hand) = spawn_sabre_player(&mut app, true);
        app.update();

        // A hilt exists, owned by the player…
        let mut hilts = app.world_mut().query::<(Entity, &SabreHilt)>();
        let found: Vec<(Entity, Entity)> = hilts
            .iter(app.world())
            .map(|(entity, hilt)| (entity, hilt.owner))
            .collect();
        assert_eq!(found.len(), 1, "expected exactly one hilt");
        assert_eq!(found[0].1, player);

        // …and it is parented to the hand, not the player root. This is the
        // whole point: the weapon inherits the animated arm.
        let parent = app.world().get::<ChildOf>(found[0].0).map(|c| c.parent());
        assert_eq!(parent, Some(hand), "hilt must hang off the hand entity");
    }

    #[test]
    fn sheathing_moves_the_hilt_to_the_belt_and_drawing_returns_it_to_the_hand() {
        let mut app = hilt_test_app();
        let (player, hand, belt) = spawn_sabre_player_with_belt(&mut app, false);

        // Sheathed: the handle still exists as equipment, worn on the belt.
        app.update();
        let stowed = single_hilt(&mut app);
        assert_eq!(stowed.1, HiltCarry::Holstered);
        assert_eq!(
            app.world().get::<ChildOf>(stowed.0).map(|c| c.parent()),
            Some(belt),
            "a sheathed sabre hangs on the body, it does not vanish"
        );

        // Drawing moves the same equipment into the hand.
        app.world_mut().get_mut::<BeamSabre>(player).unwrap().active = true;
        app.update();
        let drawn = single_hilt(&mut app);
        assert_eq!(drawn.1, HiltCarry::Drawn);
        assert_eq!(
            app.world().get::<ChildOf>(drawn.0).map(|c| c.parent()),
            Some(hand)
        );

        // And sheathing again puts it back on the belt.
        app.world_mut().get_mut::<BeamSabre>(player).unwrap().active = false;
        app.update();
        let restowed = single_hilt(&mut app);
        assert_eq!(restowed.1, HiltCarry::Holstered);
        assert_eq!(
            app.world().get::<ChildOf>(restowed.0).map(|c| c.parent()),
            Some(belt)
        );
    }

    #[test]
    fn a_character_with_nowhere_to_holster_simply_carries_nothing() {
        // No belt and no pelvis: sheathing has no anchor, which must be
        // handled gracefully rather than leaving a hilt floating at the origin.
        let mut app = hilt_test_app();
        spawn_sabre_player(&mut app, false);
        app.update();
        let mut hilts = app.world_mut().query::<&SabreHilt>();
        assert_eq!(hilts.iter(app.world()).count(), 0);
    }

    /// The one hilt in the world, with its carry state.
    fn single_hilt(app: &mut App) -> (Entity, HiltCarry) {
        let mut query = app.world_mut().query::<(Entity, &SabreHilt)>();
        let found: Vec<(Entity, HiltCarry)> = query
            .iter(app.world())
            .map(|(entity, hilt)| (entity, hilt.carry))
            .collect();
        assert_eq!(found.len(), 1, "expected exactly one hilt");
        found[0]
    }

    #[test]
    fn only_one_hilt_is_ever_mounted_no_matter_how_many_frames_run() {
        let mut app = hilt_test_app();
        spawn_sabre_player(&mut app, true);
        for _ in 0..8 {
            app.update();
        }
        let mut hilts = app.world_mut().query::<&SabreHilt>();
        assert_eq!(
            hilts.iter(app.world()).count(),
            1,
            "mounting must be idempotent across frames"
        );
    }

    #[test]
    fn hilt_mounts_to_a_rig_wrist_when_there_is_one() {
        // Rigged skeletons give the most accurate mount; cartoon parts are
        // the fallback; a half-built character mounts nothing yet.
        let drawn = |wrist, hand| resolve_hand_mount(HiltCarry::Drawn, wrist, hand, true, true);
        assert_eq!(drawn(true, true), Some(HandMount::Wrist));
        assert_eq!(drawn(true, false), Some(HandMount::Wrist));
        assert_eq!(drawn(false, true), Some(HandMount::HandPart));
        assert_eq!(drawn(false, false), None, "no hand yet: mount nothing");

        // Holstering anchors on the body, preferring the rigged pelvis, and
        // never falls back to a hand.
        let stowed =
            |pelvis, belt| resolve_hand_mount(HiltCarry::Holstered, true, true, pelvis, belt);
        assert_eq!(stowed(true, true), Some(HandMount::HipJoint));
        assert_eq!(stowed(false, true), Some(HandMount::BeltPart));
        assert_eq!(stowed(false, false), None);

        // Carry state round-trips through the mount.
        for mount in [HandMount::Wrist, HandMount::HandPart] {
            assert_eq!(mount.carry(), HiltCarry::Drawn);
        }
        for mount in [HandMount::HipJoint, HandMount::BeltPart] {
            assert_eq!(mount.carry(), HiltCarry::Holstered);
        }
    }

    #[test]
    fn hilt_sits_in_the_palm_and_points_the_blade_out_of_the_fist() {
        for mount in [HandMount::Wrist, HandMount::HandPart] {
            let transform = hilt_local_transform(mount);
            // The grip is modelled along local Y, so the mount must rotate it
            // to project forward out of the hand rather than through the wrist.
            let blade_axis = transform.rotation * Vec3::Y;
            assert!(
                blade_axis.z < -0.9,
                "{mount:?} blade should point out of the fist, got {blade_axis:?}"
            );
            assert!(transform.scale.x > 0.0);
        }
        // A wrist joint sits further up the forearm than a palm mesh, so the
        // grip is pushed deeper to land in the hand.
        let wrist = hilt_local_transform(HandMount::Wrist).translation;
        let hand = hilt_local_transform(HandMount::HandPart).translation;
        assert!(wrist.y < hand.y);
    }

    #[test]
    fn blade_extends_from_the_emitter_as_it_ignites() {
        let off = sabre_blade_local_transform(SabreBladeLayer::Aura, 0.0);
        let half = sabre_blade_local_transform(SabreBladeLayer::Aura, 0.5);
        let lit = sabre_blade_local_transform(SabreBladeLayer::Aura, 1.0);

        // Length grows with ignition, and the blade's centre travels outward
        // with it — that is what makes it look extended rather than swapped in.
        assert!(half.scale.y > off.scale.y);
        assert!(lit.scale.y > half.scale.y);
        assert!(lit.translation.y > half.translation.y);
        assert!(half.translation.y > off.translation.y);

        // Thickness is constant; only length animates.
        assert!((lit.scale.x - off.scale.x).abs() < 1e-6);

        // The core is thinner and slightly shorter than the aura, so the
        // bright centre always sits inside the glow.
        let aura = sabre_blade_local_transform(SabreBladeLayer::Aura, 1.0);
        let core = sabre_blade_local_transform(SabreBladeLayer::Core, 1.0);
        assert!(core.scale.x < aura.scale.x);
        assert!(core.scale.y < aura.scale.y);

        // Ignition is clamped, so an overshooting timer cannot stretch it.
        let over = sabre_blade_local_transform(SabreBladeLayer::Aura, 4.0);
        assert!((over.scale.y - lit.scale.y).abs() < 1e-6);
        // And a fully retracted blade still has positive scale (zero scale
        // makes Bevy transforms non-invertible).
        assert!(off.scale.y > 0.0);
    }

    #[test]
    fn sabre_technique_resolver_maps_stance_and_direction_to_verbs() {
        let mut up = UpgradeLedger::default();
        for id in crate::combat::upgrades::STARTER_SABRE_RELIC_IDS {
            up.unlock_relic(id);
        }
        let none = Vec2::ZERO;
        let hold_up = Vec2::new(0.0, 1.0);
        let hold_down = Vec2::new(0.0, -1.0);

        // Heavy: cyclone on the ground, spiral in the air.
        assert_eq!(
            resolve_sabre_technique(&up, true, false, false, none, true),
            Some(SabreTechnique::CycloneSlash)
        );
        assert_eq!(
            resolve_sabre_technique(&up, true, false, false, none, false),
            Some(SabreTechnique::SpiralSlash)
        );
        // Dodge: dash on the ground, pound in the air.
        assert_eq!(
            resolve_sabre_technique(&up, false, true, false, none, true),
            Some(SabreTechnique::CometDash)
        );
        assert_eq!(
            resolve_sabre_technique(&up, false, true, false, none, false),
            Some(SabreTechnique::MeteorPound)
        );
        // Attack + up launches; a bare attack falls through to the chain.
        assert_eq!(
            resolve_sabre_technique(&up, false, false, true, hold_up, true),
            Some(SabreTechnique::RisingSlash)
        );
        assert_eq!(
            resolve_sabre_technique(&up, false, false, true, none, true),
            None,
            "a plain swing must always reach the base slash chain"
        );
        // Throw is not in the starter kit, so down+attack is still a swing…
        assert_eq!(
            resolve_sabre_technique(&up, false, false, true, hold_down, true),
            None
        );
        // …until its blueprint is found.
        up.unlock_relic("sabre_throw_blueprint");
        assert_eq!(
            resolve_sabre_technique(&up, false, false, true, hold_down, true),
            Some(SabreTechnique::SabreThrow)
        );
        // Directional attacks are grounded openers only.
        assert_eq!(
            resolve_sabre_technique(&up, false, false, true, hold_up, false),
            None
        );
    }

    #[test]
    fn locked_techniques_never_resolve() {
        let empty = UpgradeLedger::default();
        for (heavy, dodge, attack, axis, grounded) in [
            (true, false, false, Vec2::ZERO, true),
            (true, false, false, Vec2::ZERO, false),
            (false, true, false, Vec2::ZERO, true),
            (false, false, true, Vec2::new(0.0, 1.0), true),
            (false, false, true, Vec2::new(0.0, -1.0), true),
        ] {
            assert_eq!(
                resolve_sabre_technique(&empty, heavy, dodge, attack, axis, grounded),
                None,
                "no blueprints should mean no techniques"
            );
        }
    }

    #[test]
    fn every_technique_has_authored_data_and_a_sound() {
        let lib = MoveLibrary::defaults();
        for technique in [
            SabreTechnique::CycloneSlash,
            SabreTechnique::CometDash,
            SabreTechnique::MeteorPound,
            SabreTechnique::RisingSlash,
            SabreTechnique::SpiralSlash,
            SabreTechnique::SabreThrow,
        ] {
            let def = sabre_technique_def(&lib.sabre_techniques, technique)
                .unwrap_or_else(|| panic!("{technique:?} has no authored data"));
            assert!(def.damage_mult > 0.0);
            assert!(!sabre_technique_sfx(technique).is_empty());
        }
        assert!(sabre_technique_def(&lib.sabre_techniques, SabreTechnique::Ready).is_none());
        // The throw is the only projectile verb; the rest resolve as arcs.
        assert!(lib.sabre_techniques.sabre_throw.throw_speed > 0.0);
        assert_eq!(lib.sabre_techniques.rising_slash.throw_speed, 0.0);
        assert!(lib.sabre_techniques.rising_slash.rise_speed > 0.0);
    }

    #[test]
    fn drawn_sabre_with_cyclone_relic_claims_heavy_input() {
        let mut upgrades = UpgradeLedger::default();
        let mut sabre = BeamSabre {
            active: true,
            ..Default::default()
        };

        // Without the Cyclone Slash blueprint the fist chain keeps heavy.
        assert!(!sabre_claims_heavy_input(Some(&sabre), &upgrades));

        upgrades.unlock_relic("cyclone_slash_blueprint");
        assert!(sabre_claims_heavy_input(Some(&sabre), &upgrades));

        // Holstered (or absent) sabre never claims it, even with the relic.
        sabre.active = false;
        assert!(!sabre_claims_heavy_input(Some(&sabre), &upgrades));
        assert!(!sabre_claims_heavy_input(None, &upgrades));

        // Cooldown state must not affect the claim: the gate has to agree
        // between systems regardless of which one ran first this frame.
        sabre.active = true;
        sabre.cooldown_timer = 10.0;
        assert!(sabre_claims_heavy_input(Some(&sabre), &upgrades));
    }

    #[test]
    fn buffered_sabre_slash_obeys_the_authored_cancel_window() {
        let recovery = 0.30;
        let cancel_after = 0.12;

        assert!(!sabre_recovery_should_advance(
            recovery,
            cancel_after,
            0.20,
            true,
        ));
        assert!(sabre_recovery_should_advance(
            recovery,
            cancel_after,
            0.18,
            true,
        ));
        assert!(!sabre_recovery_should_advance(
            recovery,
            cancel_after,
            0.05,
            false,
        ));
        assert!(sabre_recovery_should_advance(
            recovery,
            cancel_after,
            0.0,
            false,
        ));
    }

    #[test]
    fn starting_a_melee_move_clears_the_previous_active_window_hits() {
        let library = MoveLibrary::defaults();
        let mut combo = MeleeCombo::new();
        let stale_target = Entity::from_bits(42);
        combo.hit_entities.insert(stale_target);
        let mut state = PlayerStateMachine::default();

        let mut damageable = Damageable::default();
        let active = start_melee_move(
            &library,
            &mut combo,
            MeleeChain::Light,
            &mut state,
            &mut damageable,
        );

        assert!(active.is_some());
        assert!(combo.hit_entities.is_empty());
        assert_eq!(state.current, PlayerState::Attacking);
        // Default moves author no i-frames; the shared window is untouched.
        assert!(!damageable.is_invulnerable);
    }

    #[test]
    fn move_iframes_extend_but_never_shorten_the_shared_window() {
        let mut damageable = Damageable::default();
        grant_iframes(&mut damageable, 0.0);
        assert!(!damageable.is_invulnerable);

        grant_iframes(&mut damageable, 0.28);
        assert!(damageable.is_invulnerable);
        assert!((damageable.invulnerability_timer - 0.28).abs() < 1e-6);

        // A longer window already active (e.g. post-hit invulnerability) wins.
        damageable.invulnerability_timer = 0.5;
        grant_iframes(&mut damageable, 0.28);
        assert!((damageable.invulnerability_timer - 0.5).abs() < 1e-6);
    }

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

    #[test]
    fn starter_sabre_is_drawn_upgraded_and_combo_ready() {
        let sabre = BeamSabre::default();
        let library = MoveLibrary::defaults();

        assert!(sabre.unlocked);
        assert!(sabre.active);
        assert_eq!(sabre.level, 2);
        assert_eq!(sabre.slash_count, 4);
        assert!(library.sabre.len() >= 6);
    }

    #[test]
    fn starter_and_upgraded_sabre_waves_follow_combo_progression() {
        let sabre = BeamSabre::default();
        let base_upgrades = UpgradeLedger::default();
        assert!(sabre_wave_profile(
            &sabre,
            &base_upgrades,
            1,
            &crate::combat::blades::STARTER_BLADE
        )
        .is_none());
        let base = sabre_wave_profile(
            &sabre,
            &base_upgrades,
            2,
            &crate::combat::blades::STARTER_BLADE,
        )
        .expect("starter Saber earns a wave on its second slash");
        assert_eq!(base.projectile_count, 2);
        assert!((base.damage_mult - 0.40).abs() < 1e-6);
        assert!(!base.explosive);

        let upgraded = UpgradeLedger {
            ranks: vec![(crate::combat::upgrades::TechUpgradeId::BeamCapacitors, 5)],
            relics: vec!["solar_sabre_glyph".into()],
            ..default()
        };
        let strong =
            sabre_wave_profile(&sabre, &upgraded, 2, &crate::combat::blades::STARTER_BLADE)
                .unwrap();
        assert_eq!(strong.projectile_count, 4);
        assert!(strong.width > base.width);
        assert!(strong.length > base.length);
        assert!(strong.damage_mult > base.damage_mult);
        assert!(strong.explosive);
        assert!(sabre_wave_profile(
            &sabre,
            &base_upgrades,
            3,
            &crate::combat::blades::STARTER_BLADE
        )
        .is_some());
        assert!(
            sabre_wave_profile(&sabre, &upgraded, 4, &crate::combat::blades::STARTER_BLADE)
                .is_some()
        );
    }

    #[test]
    fn swept_drone_proxy_catches_fast_projectiles_between_frames() {
        let fraction = segment_sphere_hit_fraction(Vec3::ZERO, Vec3::X * 10.0, Vec3::X * 5.0, 1.0)
            .expect("segment crosses the proxy");
        assert!((fraction - 0.4).abs() < 1e-5);
        assert!(segment_sphere_hit_fraction(
            Vec3::ZERO,
            Vec3::X * 10.0,
            Vec3::new(5.0, 2.1, 0.0),
            1.0,
        )
        .is_none());
        assert_eq!(drone_damage_proxy_radius(EnemyType::Drone), Some(2.8));
        assert_eq!(drone_damage_proxy_radius(EnemyType::SpyDrone), Some(3.6));
        assert_eq!(drone_damage_proxy_radius(EnemyType::Soldier), None);
    }

    #[test]
    fn sabre_vfx_budget_never_reserves_more_than_its_cap() {
        let budget = SabreVfxBudget::default();
        let mut active = 0;
        for _ in 0..budget.max_entities {
            assert!(reserve_sabre_vfx_slot(&mut active, budget.max_entities));
        }
        assert_eq!(active, budget.max_entities);
        assert!(!reserve_sabre_vfx_slot(&mut active, budget.max_entities));
        assert_eq!(active, budget.max_entities);
    }
}
