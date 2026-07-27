use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::audio::SpatialListener;
use bevy::camera::Hdr;
use bevy::camera::{PerspectiveProjection, Projection, Viewport};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::chapters::{chapter_map_location, EVEREST_RANGE_HALF_EXTENT};
use crate::character_blueprint::{
    CartoonAppearanceRecipe, CharacterBlueprint, CharacterPaletteRecipe,
};
use crate::character_parts::CharacterLoadout;
use crate::character_studio::generators::build_character_patch;
use crate::character_studio::human_mesh::{spawn_human, PlayableStudioHuman};
use crate::characters::{
    accent_preset, attach_native_playable_character, attach_player_gameplay_rig,
    despawn_cartoon_character_parts, eye_preset, hair_preset, hero_config,
    hero_config_with_overrides, outfit_preset, skin_preset,
};
use crate::components::armor::{ArmorRechargeState, ArmorSet};
use crate::components::character::{CartoonPart, JointMarker};
use crate::components::enemy::{BossEnemy, DeadEnemy, Enemy, EnemyType, FlyingDrone};
use crate::components::inventory::{Inventory, QuickItemSlot};
use crate::components::player::*;
use crate::components::weapon::*;
use crate::components::world::{
    BoatPassenger, RailGrindState, SpeedLoopGuide, SpeedRoadCheckpoint, WaterBody, WaterBodyKind,
    WorldAnchor, WorldRouteMarker,
};
use crate::damage::{apply_damage, DamageInfo, DamageType, Damageable, Health};
use crate::events::*;
use crate::game_loop::{fixed_motor_off, fixed_motor_on, GameSet, PreviousTickPosition, SimConfig};
use crate::hero_roster::{apply_hero_runtime, hero_power_profile, HeroPowerProfile, HeroPowerSet};
use crate::hitstop::hitstop_inactive;
use crate::input_buffer::PlayerInputBuffers;
use crate::physics::prelude::*;
use crate::player_mesh::attach_modular_player_mesh;
use crate::plugins::world_plugin::terrain_surface_y;
use crate::rendering::{
    Camera3dBundle, PbrBundle, ShieldMaterial, ShieldMaterialUniform, ShieldPbrBundle,
    SpatialBundle,
};
use crate::resources::{
    is_stale_reference_blueprint, reference_appearance_recipe, reference_body_recipe,
    ChapterProgress, CurrentChapter, DungeonCrawlState, GameSettings, LocalPlayerConfig,
    PlaySessionTransition, PlayerPartLoadout, PlayerSelectState, PlayerSlotConfig,
    WorldRouteRegistry, WorldRouteState,
};
use crate::robot_pets::RobotPetCollection;
use crate::sfx::ModularActionSfxEvent;
use crate::state::AppState;

/// Route the player's visual through the new native modular humanoid
/// ([`crate::player_mesh`]) instead of the legacy `character_parts` meshes.
/// Flip to `false` to fall back to the original system.
const USE_MODULAR_PLAYER_MESH: bool = true;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct PlayerPlugin;

#[derive(Component, Default)]
struct UnderwaterCameraBlend(f32);

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
    reversion_cooldown: f32,
}

/// The rocket hoverboard's rendered deck. Pose inputs (contact normal, stick
/// axis, speed) are noisy per frame, so the visual keeps its own damped state
/// and eases toward the targets instead of snapping — see
/// [`update_rocket_hoverboard_visuals`].
#[derive(Component, Debug, Clone, Copy)]
struct RocketHoverboardVisual {
    owner: u8,
    /// Damped surface normal in player-local space. Starts level; eases toward
    /// the contact normal while grounded and back to level in the air.
    smoothed_normal: Vec3,
    /// Damped carve/bank roll (radians).
    smoothed_bank: f32,
    /// Damped nose pitch (radians).
    smoothed_pitch: f32,
    /// Damped trick spin (radians) so a snapped `spin_degrees` never pops.
    smoothed_spin: f32,
}

impl RocketHoverboardVisual {
    fn new(owner: u8) -> Self {
        Self {
            owner,
            smoothed_normal: Vec3::Y,
            smoothed_bank: 0.0,
            smoothed_pitch: 0.0,
            smoothed_spin: 0.0,
        }
    }
}

/// Frame-rate independent easing factor for a given half-life-ish rate.
/// Matches the `1 - exp(-rate * dt)` idiom used by the camera smoothing.
fn damp_factor(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt.max(0.0)).exp()
}

/// Smooth 0→1 ramp over `[edge0, edge1]`, used instead of hard speed
/// thresholds so a value hovering at the edge cannot flicker between two
/// pose targets on consecutive frames.
fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Default for SharedEncounterCamera {
    fn default() -> Self {
        Self {
            active: false,
            reason: SharedEncounterReason::None,
            anchor: Vec3::ZERO,
            focus: Vec3::ZERO,
            radius: 24.0,
            reversion_cooldown: 0.0,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct CameraDisplayTransition {
    /// 0.0 = fully split, 1.0 = fully shared
    pub progress: f32,
    pub last_was_dungeon: bool,
}

impl Default for CameraDisplayTransition {
    fn default() -> Self {
        Self {
            progress: 0.0,
            last_was_dungeon: false,
        }
    }
}

fn third_person_camera_offset() -> Vec3 {
    Vec3::new(0.0, 4.5, 11.0)
}

#[derive(Component, Debug, Clone, Copy)]
#[allow(dead_code)]
struct GrappleCableVisual {
    owner: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
struct PlayerShieldVisual {
    owner_index: u8,
}

/// Four stable material instances keep split-screen shield feedback owned and
/// bounded. Damage only changes uniforms; it never allocates a new material.
#[derive(Resource)]
struct PlayerShieldVfxAssets {
    mesh: Handle<Mesh>,
    materials: [Handle<ShieldMaterial>; 4],
    pulse_remaining: [f32; 4],
    pulse_strength: [f32; 4],
}

fn setup_player_shield_vfx(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ShieldMaterial>>,
) {
    let colors = [
        Vec4::new(0.12, 0.78, 1.0, 1.0),
        Vec4::new(1.0, 0.34, 0.74, 1.0),
        Vec4::new(0.40, 1.0, 0.42, 1.0),
        Vec4::new(1.0, 0.72, 0.14, 1.0),
    ];
    let handles = std::array::from_fn(|index| {
        materials.add(ShieldMaterial {
            settings: ShieldMaterialUniform {
                color: colors[index],
                ..default()
            },
        })
    });
    commands.insert_resource(PlayerShieldVfxAssets {
        mesh: meshes.add(Sphere::new(1.15)),
        materials: handles,
        pulse_remaining: [0.0; 4],
        pulse_strength: [0.0; 4],
    });
}

fn set_player_shield_pulse(
    remaining: &mut [f32; 4],
    strength: &mut [f32; 4],
    player_index: Option<u8>,
    duration: f32,
    intensity: f32,
) {
    if let Some(index) = player_index.filter(|index| *index < 4) {
        remaining[index as usize] = duration;
        strength[index as usize] = intensity;
    } else {
        remaining.fill(duration);
        strength.fill(intensity);
    }
}

fn player_shield_feedback_system(
    mut commands: Commands,
    time: Res<Time>,
    mut assets: ResMut<PlayerShieldVfxAssets>,
    mut shield_materials: ResMut<Assets<ShieldMaterial>>,
    mut damaged: MessageReader<PlayerDamagedEvent>,
    mut parried: MessageReader<PlayerParryEvent>,
    players: Query<(&PlayerIndex, &GlobalTransform), With<Player>>,
    mut visuals: Query<
        (Entity, &PlayerShieldVisual, &mut Transform, &mut Visibility),
        Without<Player>,
    >,
) {
    let assets = &mut *assets;
    for event in damaged.read() {
        let strength = (0.38 + event.amount / 42.0).clamp(0.42, 1.0);
        set_player_shield_pulse(
            &mut assets.pulse_remaining,
            &mut assets.pulse_strength,
            event.player_index,
            0.30,
            strength,
        );
    }
    for event in parried.read().filter(|event| event.success) {
        set_player_shield_pulse(
            &mut assets.pulse_remaining,
            &mut assets.pulse_strength,
            event.player_index,
            0.42,
            1.35,
        );
    }

    let dt = time.delta_secs();
    let mut represented = [false; 4];
    for (entity, visual, mut transform, mut visibility) in visuals.iter_mut() {
        let Some((_, player_transform)) = players
            .iter()
            .find(|(index, _)| index.0 == visual.owner_index)
        else {
            commands.entity(entity).despawn();
            continue;
        };
        let slot = visual.owner_index as usize;
        if slot >= represented.len() {
            commands.entity(entity).despawn();
            continue;
        }
        represented[slot] = true;
        assets.pulse_remaining[slot] = (assets.pulse_remaining[slot] - dt).max(0.0);
        let envelope = (assets.pulse_remaining[slot] / 0.30).clamp(0.0, 1.0);
        let impact = envelope * assets.pulse_strength[slot];
        *visibility = if impact > 0.01 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        transform.translation = player_transform.translation() + Vec3::Y * 0.95;
        transform.scale = Vec3::new(0.82, 1.18, 0.82) * (1.0 + impact * 0.08);
        if let Some(mut material) = shield_materials.get_mut(&assets.materials[slot]) {
            material.settings.edge.z = impact;
            material.settings.pattern.w = 0.24 + impact.min(1.0) * 0.34;
        }
    }

    for (index, player_transform) in players.iter() {
        let slot = index.0 as usize;
        if slot >= represented.len() || represented[slot] {
            continue;
        }
        commands.spawn((
            ShieldPbrBundle {
                mesh: Mesh3d(assets.mesh.clone()),
                material: MeshMaterial3d(assets.materials[slot].clone()),
                transform: Transform::from_translation(
                    player_transform.translation() + Vec3::Y * 0.95,
                )
                .with_scale(Vec3::new(0.82, 1.18, 0.82)),
                visibility: Visibility::Hidden,
                ..default()
            },
            PlayerShieldVisual {
                owner_index: index.0,
            },
        ));
    }
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SharedEncounterCamera>()
            .init_resource::<DungeonCrawlState>()
            .init_resource::<CameraDisplayTransition>()
            .add_systems(Startup, setup_player_shield_vfx)
            .add_systems(OnEnter(AppState::Playing), (spawn_players, grab_cursor))
            .add_systems(OnEnter(AppState::MainMenu), cleanup_players_for_menu)
            .add_systems(OnExit(AppState::Playing), release_cursor)
            .add_systems(
                Update,
                player_shield_feedback_system.run_if(in_state(AppState::Playing)),
            )
            // EC1b OFF path (default): the original single chain, unchanged.
            .add_systems(
                Update,
                (
                    dedupe_player_entities,
                    player_look,
                    update_camera_post_processing,
                    update_rocket_hoverboard_visuals,
                    traversal_mode_switch_update.run_if(hitstop_inactive),
                    grapple_hook_update.run_if(hitstop_inactive),
                    player_movement.run_if(hitstop_inactive),
                    speed_loop_traversal_system.run_if(hitstop_inactive),
                    road_checkpoint_recovery_system.run_if(hitstop_inactive),
                    terrain_fall_recovery_system.run_if(hitstop_inactive),
                    grapple_hook_impact_system.run_if(hitstop_inactive),
                    shared_encounter_camera_mode_system,
                    shared_encounter_party_pull_system,
                    dungeon_crawl_party_pull_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(fixed_motor_off),
            )
            // EC1b ON path: presentation/look stays per-frame in Update…
            .add_systems(
                Update,
                (
                    dedupe_player_entities,
                    player_look,
                    update_camera_post_processing,
                    update_rocket_hoverboard_visuals,
                    shared_encounter_camera_mode_system,
                    shared_encounter_party_pull_system,
                    dungeon_crawl_party_pull_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(fixed_motor_on),
            )
            // …and the simulation (traversal → grapple → motor → impact) runs at
            // the fixed tick. Translation is flushed to the physics controller in PostUpdate.
            .add_systems(
                FixedUpdate,
                (
                    cache_previous_tick_positions,
                    traversal_mode_switch_update,
                    grapple_hook_update,
                    player_movement,
                    speed_loop_traversal_system,
                    road_checkpoint_recovery_system,
                    terrain_fall_recovery_system,
                    grapple_hook_impact_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(fixed_motor_on)
                    .run_if(hitstop_inactive),
            )
            .add_systems(
                Update,
                (
                    player_knockback_intake,
                    player_pushbox_separation,
                    player_dodge_update,
                    player_parry_update,
                    water_survival_system,
                    player_state_update,
                    player_stamina_regen,
                    player_perk_health_regen,
                    player_quick_item_system,
                    player_invulnerability_update,
                    player_level_up,
                    player_died_check,
                    hero_affinity_update_system,
                )
                    .chain()
                    // EC0 canonical order: player actions resolve in Motor,
                    // before Combat — the dodge-roll suppression in
                    // `player_dodge_update` and the sabre technique trigger
                    // read shared state in a deterministic order.
                    .in_set(GameSet::Motor)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                PostUpdate,
                player_camera_follow_system
                    .after(PhysicsCompatSet::CharacterController)
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(AppState::Playing)),
            )
            // EC1b: flush accumulated fixed-tick translation once per
            // frame, before the physics step reads it.
            .add_systems(
                PostUpdate,
                flush_motor_translation
                    .before(PhysicsCompatSet::CharacterController)
                    .run_if(in_state(AppState::Playing))
                    .run_if(fixed_motor_on),
            );
    }
}

fn player_quick_item_system(
    mut players: Query<
        (
            &PlayerIndex,
            &PlayerInput,
            &mut Inventory,
            &mut QuickItemSlot,
            &mut Health,
            &mut PlayerStats,
        ),
        With<Player>,
    >,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (index, input, mut inventory, mut quick, mut health, mut stats) in players.iter_mut() {
        if !input.use_quick_item {
            continue;
        }
        let Some(item_id) = quick.item_id.clone() else {
            msg_ev.write(UiMessageEvent {
                text: format!("P{} has no quick item equipped", index.0 + 1),
                duration: 1.4,
            });
            continue;
        };
        if !inventory.has(&item_id, 1) {
            quick.item_id = None;
            msg_ev.write(UiMessageEvent {
                text: format!("P{} quick item is empty", index.0 + 1),
                duration: 1.4,
            });
            continue;
        }

        let used = match item_id.as_str() {
            "health_pack" if health.current < health.max => {
                health.heal(50.0);
                Some("Health Pack")
            }
            "armor_shard" if stats.armor < stats.max_armor => {
                stats.armor = (stats.armor + 25.0).min(stats.max_armor);
                Some("Armor Shard")
            }
            "shield_booster" if stats.armor < stats.max_armor => {
                stats.armor = stats.max_armor;
                Some("Shield Booster")
            }
            "xp_chip" => {
                stats.experience = stats.experience.saturating_add(25);
                Some("XP Chip")
            }
            _ => None,
        };
        let Some(label) = used else {
            msg_ev.write(UiMessageEvent {
                text: format!("P{} cannot use that item right now", index.0 + 1),
                duration: 1.4,
            });
            continue;
        };
        inventory.remove_item(&item_id, 1);
        if !inventory.has(&item_id, 1) {
            quick.item_id = None;
        }
        msg_ev.write(UiMessageEvent {
            text: format!("P{} used {}", index.0 + 1, label),
            duration: 1.5,
        });
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
) -> (
    PlayerStats,
    PlayerBaseStats,
    PlayerMovement,
    DodgeState,
    Collider,
) {
    let mut stats = PlayerStats::default();
    let mut base_stats = PlayerBaseStats::default();
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

        base_stats.max_health = gameplay.max_health;
        stats.max_health = base_stats.max_health;
        stats.max_stamina = gameplay.max_stamina;
        stats.stamina = gameplay.max_stamina;
        base_stats.max_armor = gameplay.max_armor;
        stats.max_armor = base_stats.max_armor;
        stats.armor = base_stats.max_armor;

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
        base_stats,
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
    let mut body = reference_body_recipe(name);
    body.leg_length = (body.leg_length + 0.03).min(1.45);
    body.hip_width = (body.hip_width * 1.03).min(1.28);
    body.foot_size = (body.foot_size * 1.04).min(1.36);
    body.muscle = (body.muscle * 1.02).min(1.40);
    body.asymmetry = body.asymmetry.max(0.06);

    let palette = CharacterPaletteRecipe {
        skin: slot.skin_idx.map(skin_preset).unwrap_or(base.skin),
        outfit: slot.outfit_idx.map(outfit_preset).unwrap_or(base.outfit),
        accent: slot.accent_idx.map(accent_preset).unwrap_or_else(|| {
            if matches!(name, "Vincenzo" | "Antonio") {
                Color::srgb(0.12, 0.88, 1.0)
            } else {
                base.accent
            }
        }),
        hair: slot.hair_idx.map(hair_preset).unwrap_or(base.hair),
        eye: slot.eye_idx.map(eye_preset).unwrap_or(base.eye_color),
    };
    let reference_appearance = reference_appearance_recipe(name);
    let preserve_slot_appearance = slot
        .part_loadout
        .is_some_and(|loadout| !loadout.is_stale_native_default())
        || slot
            .blueprint
            .as_ref()
            .is_some_and(|blueprint| !is_stale_reference_blueprint(name, blueprint));
    let appearance = if preserve_slot_appearance {
        CartoonAppearanceRecipe {
            has_hood: slot.has_hood.unwrap_or(reference_appearance.has_hood),
            has_cape: slot.has_cape.unwrap_or(reference_appearance.has_cape),
            has_gloves: slot.has_gloves.unwrap_or(reference_appearance.has_gloves),
            has_boots: slot.has_boots.unwrap_or(reference_appearance.has_boots),
            has_shoulder_pads: slot
                .has_shoulder_pads
                .unwrap_or(reference_appearance.has_shoulder_pads),
            has_visor: slot.has_visor.unwrap_or(reference_appearance.has_visor),
        }
    } else {
        reference_appearance
    };

    CharacterBlueprint::hero(name, body, palette, appearance)
}

// ── Spawn ─────────────────────────────────────────────────────────────────────
fn seed_discovered_sabre_relics(
    progress: &ChapterProgress,
    player_progression: &mut PlayerProgression,
) -> usize {
    crate::upgrades::SABRE_RELIC_IDS
        .into_iter()
        .filter(|relic_id| progress.has_discoverable(relic_id))
        .filter(|relic_id| player_progression.upgrades.unlock_relic(*relic_id))
        .count()
}

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
    existing_visuals: Query<
        (Entity, Option<&CartoonPart>, Option<&JointMarker>),
        Or<(With<CartoonPart>, With<JointMarker>)>,
    >,
) {
    if transition.resuming_from_pause || !existing_players.is_empty() {
        return;
    }

    let active = config.active.clamp(1, 4);
    let (win_w, win_h) = window_q
        .single()
        .map(|w| (w.physical_width(), w.physical_height()))
        .unwrap_or((1280, 720));
    let board_deck_mesh = meshes.add(Cuboid::new(0.84, 0.13, 2.18));
    let board_rail_mesh = meshes.add(Cuboid::new(0.11, 0.11, 1.82));
    let board_thruster_mesh = meshes.add(Cuboid::new(0.34, 0.22, 0.28));
    let board_flame_mesh = meshes.add(Cuboid::new(0.18, 0.14, 0.48));
    let board_shell = materials.add(StandardMaterial {
        base_color: Color::srgb(0.055, 0.075, 0.13),
        metallic: 0.88,
        perceptual_roughness: 0.24,
        ..default()
    });
    let board_trim = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.72, 1.0),
        emissive: LinearRgba::new(0.08, 1.4, 3.4, 1.0),
        metallic: 0.62,
        perceptual_roughness: 0.20,
        ..default()
    });
    let board_flame = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.36, 0.05),
        emissive: LinearRgba::new(4.8, 0.72, 0.04, 1.0),
        unlit: true,
        ..default()
    });

    for i in 0..active {
        let spawn_pos = player_spawn_position(i, &current, &chapter_anchor_q);
        let slot = &select.slots[i as usize];
        let character_name = select.character_name(i as usize);
        let runtime_blueprint = slot
            .blueprint
            .as_ref()
            .filter(|blueprint| !is_stale_reference_blueprint(character_name, blueprint))
            .cloned()
            .unwrap_or_else(|| upgraded_player_blueprint(character_name, slot));
        let hero_profile = hero_power_profile(character_name);
        let hero_powers = hero_profile.amplified_powers(&robot_pets);
        let character_visual_scale = hero_config(character_name).scale;
        let (
            mut player_stats,
            mut player_base_stats,
            mut player_movement,
            mut dodge_state,
            player_collider,
        ) = authored_player_defaults(Some(&runtime_blueprint), character_visual_scale);
        let mut jetpack = JetpackState::default();
        let mut weapon_inventory = WeaponInventory::default();
        let mut special_inventory = SpecialWeaponInventory::default();
        let mut melee_combo = MeleeCombo::new();
        apply_hero_runtime(
            hero_profile,
            hero_powers,
            &mut player_stats,
            &mut player_base_stats,
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
        let mut starter_inventory = Inventory::default();
        starter_inventory.add_item("health_pack", 2, 10);
        starter_inventory.add_item("armor_shard", 2, 10);

        let mut player_progression = slot.progression.clone();
        // Campaign discoveries are party-wide, while the resulting ownership
        // is copied into each player's save-backed progression component.
        seed_discovered_sabre_relics(&progress, &mut player_progression);
        let initial_caps = player_base_stats.derived_caps(
            player_stats.level,
            0.0,
            player_progression.perks.hp_bonus(),
            player_progression.upgrades.armor_health_bonus(),
            0.0,
        );
        player_stats.max_health = initial_caps.max_health;
        player_stats.max_armor = initial_caps.max_armor;
        player_stats.armor = initial_caps.max_armor;
        for (weapon, rank) in weapon_inventory
            .slots
            .iter_mut()
            .zip(player_progression.weapon_ranks.ranks)
        {
            weapon.rank = rank;
        }

        let player = commands
            .spawn((
                Player,
                PlayerIndex(i),
                PlayerInput::default(),
                AimSolution::default(),
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
            .insert(CollisionProfile::Player)
            .insert(player_base_stats)
            .insert(player_progression)
            .insert((
                hero_profile,
                hero_powers,
                jetpack,
                TraversalModeState::default(),
                BoardBoostState::default(),
                PlatformerMoveState::default(),
                SpeedLoopTraversalState::default(),
                GrappleHookState::default(),
                EdgeGrabState::new(),
                (ClimbState::default(), PreviousTickPosition(spawn_pos)),
                dodge_state,
                ParryState::new(),
                PlayerStateMachine::default(),
                Health::new(player_stats.max_health),
                Damageable::default(),
            ))
            .insert(WaterTraversalState::default())
            .insert((
                RoadRecoveryState::default(),
                TerrainRecoveryState::new(spawn_pos),
                StuntRunState::default(),
                StuntRaceProgress::default(),
                RooftopTrialProgress::default(),
                crate::tricks::TrickState::default(),
                ArmorSet::default(),
                ArmorRechargeState::default(),
                starter_inventory,
                QuickItemSlot::default(),
                weapon_inventory,
                special_inventory,
                BeamSabre::default(),
                melee_combo,
            ))
            .id();

        attach_rocket_hoverboard(
            &mut commands,
            player,
            i,
            &board_deck_mesh,
            &board_rail_mesh,
            &board_thruster_mesh,
            &board_flame_mesh,
            &board_shell,
            &board_trim,
            &board_flame,
        );

        despawn_cartoon_character_parts(&mut commands, player, &existing_visuals);
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
        let visual_loadout =
            PlayerPartLoadout::resolve_for_hero(character_name, slot.part_loadout, *part_loadout);
        if CharacterLoadout::from(visual_loadout).arms
            == crate::character_parts::ArmPreset::DariaCannon
        {
            commands.entity(player).insert(ArmCannonUser);
        }
        if hero_powers.magic >= 1.10 {
            commands.entity(player).insert(MagicBeamCaster);
        }
        if let Some(studio_spec) = slot.studio_spec {
            attach_player_gameplay_rig(&mut commands, player, &character_config, spawn_pos);
            let body = runtime_blueprint.body.validated();
            let scale = character_config.scale;
            let half_height = (0.6 * (body.height * 0.72 + body.leg_length * 0.28) * scale)
                .clamp(0.44 * scale, 0.86 * scale);
            let radius = (0.35
                * (body.shoulder_width * 0.55 + body.chest_size * 0.25 + body.hip_width * 0.20)
                * scale)
                .clamp(0.26 * scale, 0.50 * scale);
            let capsule_total = 2.0 * (half_height + radius);
            let authored_height = 1.75
                * (0.91 + studio_spec.body.height * 0.18)
                * if matches!(studio_spec.sex, crate::character_studio::spec::Sex::Female) {
                    0.945
                } else {
                    1.0
                };
            let visual_transform = Transform::from_xyz(0.0, -(half_height + radius), 0.0)
                .with_scale(Vec3::splat(capsule_total / authored_height.max(0.5)));
            let patch = build_character_patch(&studio_spec);
            let visual = spawn_human(
                &mut commands,
                &mut meshes,
                &mut materials,
                &patch,
                visual_transform,
            );
            commands.entity(visual).insert(PlayableStudioHuman {
                owner: player,
                rest: visual_transform,
            });
            commands.entity(player).add_child(visual);
        } else if USE_MODULAR_PLAYER_MESH {
            // New native modular humanoid (built on the socket-assembly system).
            // Keep the gameplay rig so weapon/IK attach points still work.
            attach_player_gameplay_rig(&mut commands, player, &character_config, spawn_pos);
            attach_modular_player_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                player,
                &character_config,
                CharacterLoadout::from(visual_loadout),
            );
        } else {
            attach_native_playable_character(
                &mut commands,
                &mut meshes,
                &mut materials,
                player,
                character_config,
                spawn_pos,
                CharacterLoadout::from(visual_loadout),
            );
        }

        let viewport = player_viewport(i, active, win_w, win_h);

        let cam_entity = commands
            .spawn((
                Camera3dBundle {
                    transform: player_camera_transform(
                        &Transform::from_translation(spawn_pos),
                        0.0,
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
                UnderwaterCameraBlend::default(),
                DistanceFog {
                    color: Color::srgba(0.02, 0.02, 0.08, 1.0),
                    falloff: FogFalloff::ExponentialSquared { density: 0.00018 },
                    ..default()
                },
            ))
            .id();
        if i == 0 {
            commands
                .entity(cam_entity)
                .insert(SpatialListener::new(2.0));
        }

        commands.entity(player).insert(PlayerCameraRef(cam_entity));
    }
}

#[allow(clippy::too_many_arguments)]
fn attach_rocket_hoverboard(
    commands: &mut Commands,
    player: Entity,
    owner: u8,
    deck_mesh: &Handle<Mesh>,
    rail_mesh: &Handle<Mesh>,
    thruster_mesh: &Handle<Mesh>,
    flame_mesh: &Handle<Mesh>,
    shell: &Handle<StandardMaterial>,
    trim: &Handle<StandardMaterial>,
    flame: &Handle<StandardMaterial>,
) {
    commands.entity(player).with_children(|player_root| {
        player_root
            .spawn((
                SpatialBundle {
                    // The board root is at sole height, not calf height. Keeping
                    // this on the player root also makes it follow every imported
                    // or procedural rig without depending on a named foot bone.
                    transform: Transform::from_xyz(0.0, -1.24, 0.0),
                    visibility: Visibility::Hidden,
                    ..default()
                },
                RocketHoverboardVisual::new(owner),
                Name::new(format!("P{} Rocket Hoverboard", owner + 1)),
            ))
            .with_children(|board| {
                board.spawn(PbrBundle {
                    mesh: Mesh3d(deck_mesh.clone()),
                    material: MeshMaterial3d(shell.clone()),
                    transform: Transform::default(),
                    ..default()
                });
                for side in [-1.0_f32, 1.0] {
                    board.spawn(PbrBundle {
                        mesh: Mesh3d(rail_mesh.clone()),
                        material: MeshMaterial3d(trim.clone()),
                        transform: Transform::from_xyz(side * 0.40, 0.09, 0.0),
                        ..default()
                    });
                    board.spawn(PbrBundle {
                        mesh: Mesh3d(thruster_mesh.clone()),
                        material: MeshMaterial3d(shell.clone()),
                        transform: Transform::from_xyz(side * 0.31, -0.08, 0.84),
                        ..default()
                    });
                    board.spawn(PbrBundle {
                        mesh: Mesh3d(flame_mesh.clone()),
                        material: MeshMaterial3d(flame.clone()),
                        transform: Transform::from_xyz(side * 0.31, -0.08, 1.20),
                        ..default()
                    });
                }
            });
    });
}

fn update_rocket_hoverboard_visuals(
    time: Res<Time>,
    players: Query<
        (
            &PlayerIndex,
            &TraversalModeState,
            &PlayerMovement,
            &JetpackState,
            &BoardBoostState,
            &PlayerInput,
            &StuntRunState,
            &KinematicCharacterControllerOutput,
            &Transform,
        ),
        With<Player>,
    >,
    mut boards: Query<
        (&mut RocketHoverboardVisual, &mut Visibility, &mut Transform),
        Without<Player>,
    >,
) {
    let dt = time.delta_secs();
    for (mut board, mut visibility, mut transform) in boards.iter_mut() {
        let Some((_, traversal, movement, jetpack, boost, input, stunt, output, player_transform)) =
            players.iter().find(|(index, ..)| index.0 == board.owner)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let active = traversal.active == TraversalMode::Hoverboard;
        *visibility = if active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if !active {
            continue;
        }
        let speed = movement.ground_velocity.length();
        let rocket = jetpack.is_active || !movement.is_grounded;
        let pulse = (time.elapsed_secs() * if rocket { 18.0 } else { 8.0 }).sin();
        let landing_phase = (1.0 - boost.landing_timer / 0.34).clamp(0.0, 1.0);
        let landing_compression = if boost.landing_timer > 0.0 {
            (landing_phase * std::f32::consts::PI).sin()
        } else {
            0.0
        };
        transform.translation.y =
            -1.24 + pulse * if rocket { 0.045 } else { 0.016 } - landing_compression * 0.12;
        // ── Damped pose ──────────────────────────────────────────────────────
        // Every input here is noisy frame to frame: the contact normal pops
        // between trimesh triangles (and vanishes entirely on brief airborne
        // frames), the stick axis is analog-noisy, and `spin_degrees` resets
        // on landing. Easing toward the targets keeps the deck steady while
        // still reacting within a couple of frames.
        let airborne = !movement.is_grounded;
        let spin_target = if airborne {
            stunt.spin_degrees.to_radians()
        } else {
            0.0
        };
        // Smooth speed ramp instead of a hard `speed > 0.35` branch, which
        // flickered the bank angle when cruising near the threshold.
        let carve_authority = 0.24 + 0.18 * smoothstep(0.15, 0.65, speed);
        let bank_target = if airborne {
            -input.move_axis.x * 0.34
        } else {
            -input.move_axis.x * carve_authority + pulse * 0.018
        };
        let pitch_target = (-movement.velocity.y * 0.12 - speed * 0.018
            + boost.landing_approach * 0.14
            - landing_compression * 0.10)
            .clamp(-0.30, 0.30);
        // Level out in the air rather than snapping to identity when the
        // controller reports no upright contact.
        let normal_target = if airborne {
            Vec3::Y
        } else {
            ground_normal_from_controller_output(output)
                .map(|normal| (player_transform.rotation.inverse() * normal).normalize_or(Vec3::Y))
                .unwrap_or(board.smoothed_normal)
        };

        board.smoothed_normal = board
            .smoothed_normal
            .lerp(normal_target, damp_factor(14.0, dt))
            .normalize_or(Vec3::Y);
        board.smoothed_bank += (bank_target - board.smoothed_bank) * damp_factor(16.0, dt);
        board.smoothed_pitch += (pitch_target - board.smoothed_pitch) * damp_factor(16.0, dt);
        // Spin tracks fast (it is the trick read) but still never teleports.
        board.smoothed_spin += (spin_target - board.smoothed_spin) * damp_factor(26.0, dt);

        let surface_tilt = Quat::from_rotation_arc(Vec3::Y, board.smoothed_normal);
        transform.rotation = surface_tilt
            * Quat::from_rotation_y(board.smoothed_spin)
            * Quat::from_rotation_x(board.smoothed_pitch)
            * Quat::from_rotation_z(board.smoothed_bank);
        let boost_scale = if boost.timer > 0.0 { 1.08 } else { 1.0 };
        let board_scale = 1.10;
        transform.scale = Vec3::new(
            board_scale * boost_scale,
            board_scale * (1.0 - landing_compression * 0.08),
            board_scale * (1.0 + landing_compression * 0.04),
        );
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
    jetpack.max_fuel = jetpack.max_fuel.max(1200.0);
    jetpack.fuel = jetpack.max_fuel;
    jetpack.force = jetpack.force.max(0.085);
    jetpack.regen_rate = jetpack.regen_rate.max(60.0);
    jetpack.max_vertical_vel = jetpack.max_vertical_vel.max(0.52);
    jetpack.glide_fall_speed = jetpack.glide_fall_speed.min(0.62);
    jetpack.boost_forward_speed = jetpack.boost_forward_speed.max(1.18);
    jetpack.air_dash_speed = jetpack.air_dash_speed.max(1.92);
    jetpack.air_dash_cooldown = jetpack.air_dash_cooldown.min(0.42);
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

fn interpolate_viewport(
    from: Option<Viewport>,
    to: Option<Viewport>,
    t: f32,
    win_w: u32,
    win_h: u32,
) -> Option<Viewport> {
    if from.is_none() && to.is_none() {
        return None;
    }
    let from_vp = from.unwrap_or(Viewport {
        physical_position: UVec2::ZERO,
        physical_size: UVec2::new(win_w, win_h),
        ..default()
    });
    let to_vp = to.unwrap_or(Viewport {
        physical_position: UVec2::ZERO,
        physical_size: UVec2::new(win_w, win_h),
        ..default()
    });

    let px = (from_vp.physical_position.x as f32
        + t * (to_vp.physical_position.x as f32 - from_vp.physical_position.x as f32))
        .round() as u32;
    let py = (from_vp.physical_position.y as f32
        + t * (to_vp.physical_position.y as f32 - from_vp.physical_position.y as f32))
        .round() as u32;
    let sx = (from_vp.physical_size.x as f32
        + t * (to_vp.physical_size.x as f32 - from_vp.physical_size.x as f32))
        .round() as u32;
    let sy = (from_vp.physical_size.y as f32
        + t * (to_vp.physical_size.y as f32 - from_vp.physical_size.y as f32))
        .round() as u32;

    Some(Viewport {
        physical_position: UVec2::new(px, py),
        physical_size: UVec2::new(sx.max(1), sy.max(1)),
        ..default()
    })
}

fn player_camera_follow_system(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<SimConfig>,
    fixed_time: Res<Time<Fixed>>,
    mut transition: ResMut<CameraDisplayTransition>,
    dungeon: Res<DungeonCrawlState>,
    shared_camera: Res<SharedEncounterCamera>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    player_q: Query<
        (
            &PlayerIndex,
            &Transform,
            &PlayerCameraRef,
            Option<&GrappleHookState>,
            Option<&JetpackState>,
            Option<&TraversalModeState>,
            Option<&PreviousTickPosition>,
        ),
        (With<Player>, Without<PlayerCamera>),
    >,
    stunt_q: Query<(&PlayerIndex, &StuntRunState), With<Player>>,
    mut cam_q: Query<
        (
            Entity,
            &mut Transform,
            &CameraPitch,
            &mut Camera,
            &mut Projection,
        ),
        (With<PlayerCamera>, Without<Player>),
    >,
) {
    let mut referenced = Vec::new();
    let active_players = player_q.iter().count().max(1) as u8;
    let (win_w, win_h) = window_q
        .single()
        .map(|w| (w.physical_width(), w.physical_height()))
        .unwrap_or((1280, 720));

    let dungeon_or_boss_active = dungeon.active || (shared_camera.active && active_players > 1);

    if dungeon.active {
        transition.last_was_dungeon = true;
    } else if shared_camera.active && active_players > 1 {
        transition.last_was_dungeon = false;
    }

    // Dynamic S-curve display transition rate: ~0.45s and Hermite smoothstep
    let dt = time.delta_secs();
    let target = if dungeon_or_boss_active { 1.0 } else { 0.0 };
    if transition.progress < target {
        transition.progress = (transition.progress + dt * 2.2).min(target);
    } else if transition.progress > target {
        transition.progress = (transition.progress - dt * 2.2).max(target);
    }
    let p = transition.progress;
    let s = 3.0 * p * p - 2.0 * p * p * p; // Smoothstep S-curve

    // Compute target single screen/unified shared viewport transform
    let shared_target_transform = if transition.last_was_dungeon {
        let party_focus = average_positions(
            &player_q
                .iter()
                .map(|(_, transform, _, _, _, _, _)| transform.translation)
                .collect::<Vec<_>>(),
        )
        .unwrap_or(dungeon.focus);
        let dungeon_focus =
            clamp_to_dungeon_focus(party_focus, dungeon.focus, dungeon.radius * 0.62);
        dungeon_crawl_camera_transform(dungeon_focus, dungeon.radius)
    } else {
        shared_boss_camera_transform(shared_camera.focus, shared_camera.radius)
    };

    let lead_camera = player_q
        .iter()
        .min_by_key(|(index, _, _, _, _, _, _)| index.0)
        .map(|(_, _, camera_ref, _, _, _, _)| camera_ref.0);

    for (index, player_transform, camera_ref, grapple, jetpack, traversal, prev_tick) in
        player_q.iter()
    {
        // EC1b render interpolation: while the fixed motor is on, follow a
        // position lerped from the last tick-start toward the live transform by
        // the fixed-clock overstep, hiding the tick staircase above FIXED_HZ.
        let interp_holder;
        let player_transform = match prev_tick {
            Some(prev) if sim.fixed_motor => {
                let alpha = fixed_time.overstep_fraction();
                interp_holder = Transform {
                    translation: prev.0.lerp(player_transform.translation, alpha),
                    ..*player_transform
                };
                &interp_holder
            }
            _ => player_transform,
        };
        referenced.push(camera_ref.0);
        if let Ok((camera_entity, mut camera_transform, pitch, mut camera, mut projection)) =
            cam_q.get_mut(camera_ref.0)
        {
            let mut local_ind_transform = player_camera_transform(player_transform, pitch.0);
            let hook_pullback = grapple
                .map(|g| if g.is_active() { 3.0 } else { 0.0 })
                .unwrap_or(0.0);
            let flight_lift = jetpack
                .map(|j| if j.is_active { 0.9 } else { 0.0 })
                .unwrap_or(0.0);
            let board_pullback = traversal
                .map(|t| {
                    if t.active == TraversalMode::Hoverboard {
                        1.6
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0);
            let stunt_intensity = stunt_q
                .iter()
                .find(|(stunt_index, _)| stunt_index.0 == index.0)
                .map(|(_, stunt)| {
                    if stunt.active {
                        (stunt.multiplier - 1.0 + stunt.airtime * 0.35).clamp(0.0, 3.5)
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0);
            local_ind_transform.translation += player_transform.rotation
                * Vec3::new(
                    0.0,
                    flight_lift,
                    hook_pullback + board_pullback + stunt_intensity * 0.85,
                );
            if let Projection::Perspective(ref mut perspective) = *projection {
                let target_fov =
                    (58.0 + hook_pullback * 1.2 + board_pullback * 1.6 + stunt_intensity * 2.1)
                        .to_radians();
                perspective.fov += (target_fov - perspective.fov) * (1.0 - (-dt * 8.0).exp());
            }
            let is_lead = Some(camera_entity) == lead_camera;

            if p >= 0.999 {
                // Fully transitioned: Only the lead camera is active, filling screen completely
                camera.is_active = is_lead;
                camera.viewport = None;
                if is_lead {
                    *camera_transform = shared_target_transform;
                }
            } else if p <= 0.001 {
                // Fully split-screen mode
                camera.is_active = true;
                camera.viewport = player_viewport(index.0, active_players, win_w, win_h);
                camera_transform.translation = smooth_camera_position(
                    camera_transform.translation,
                    local_ind_transform.translation,
                    dt,
                );
                camera_transform.rotation = local_ind_transform.rotation;
                camera_transform.scale = Vec3::ONE;
            } else {
                // In transition: Keep both active to perform the blending
                camera.is_active = true;

                // Position & Rotation Hermite interpolation
                let blend_pos = local_ind_transform
                    .translation
                    .lerp(shared_target_transform.translation, s);
                let blend_rot = local_ind_transform
                    .rotation
                    .slerp(shared_target_transform.rotation, s);
                *camera_transform = Transform {
                    translation: blend_pos,
                    rotation: blend_rot,
                    scale: Vec3::ONE,
                };

                // Viewport sliding/collapsing lerp
                let split_vp = player_viewport(index.0, active_players, win_w, win_h);
                if is_lead {
                    // P1 Lead camera expands seamlessly to cover full viewport
                    camera.viewport = interpolate_viewport(split_vp, None, s, win_w, win_h);
                } else {
                    // Secondary players collapse smoothly to 1x1 corners to clear the master panel
                    if let Some(split) = split_vp {
                        let target_vp = Some(Viewport {
                            physical_position: split.physical_position,
                            physical_size: UVec2::new(1, 1),
                            ..default()
                        });
                        camera.viewport =
                            interpolate_viewport(Some(split), target_vp, s, win_w, win_h);
                    } else {
                        camera.viewport = None;
                    }
                }
            }
        }
    }

    for (camera, mut camera_transform, _, mut camera_component, _) in cam_q.iter_mut() {
        if !referenced.contains(&camera) {
            camera_component.is_active = false;
            camera_transform.translation = Vec3::new(0.0, -10_000.0, 0.0);
            commands.entity(camera).try_despawn();
        }
    }
}

fn player_camera_transform(player_transform: &Transform, pitch: f32) -> Transform {
    let local_offset = third_person_camera_offset();
    Transform {
        translation: player_transform.translation + player_transform.rotation * local_offset,
        rotation: player_transform.rotation * Quat::from_rotation_x(pitch),
        scale: Vec3::ONE,
    }
}

fn smooth_camera_position(current: Vec3, target: Vec3, dt: f32) -> Vec3 {
    if current.distance_squared(target) > 24.0 * 24.0 {
        return target;
    }
    current.lerp(target, 1.0 - (-dt.max(0.0) * 20.0).exp())
}

fn shared_boss_camera_transform(focus: Vec3, radius: f32) -> Transform {
    let distance = (radius * 1.25).clamp(34.0, 96.0);
    let height = (radius * 0.72 + 14.0).clamp(24.0, 72.0);
    let translation = focus + Vec3::new(0.0, height, distance);
    Transform::from_translation(translation).looking_at(focus + Vec3::Y * 2.2, Vec3::Y)
}

fn dungeon_crawl_camera_transform(focus: Vec3, radius: f32) -> Transform {
    let height = (radius * 1.12).clamp(46.0, 92.0);
    let z_offset = (radius * 0.22).clamp(10.0, 22.0);
    let translation = focus + Vec3::new(0.0, height, z_offset);
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
    time: Res<Time>,
    mut mode: ResMut<SharedEncounterCamera>,
    player_q: Query<&Transform, With<Player>>,
    boss_q: Query<&Transform, (With<BossEnemy>, Without<DeadEnemy>)>,
    drone_q: Query<&Transform, (With<FlyingDrone>, Without<BossEnemy>, Without<DeadEnemy>)>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    if mode.reversion_cooldown > 0.0 {
        mode.reversion_cooldown = (mode.reversion_cooldown - dt).max(0.0);
    }

    let players: Vec<Vec3> = player_q
        .iter()
        .map(|transform| transform.translation)
        .collect();
    if players.len() <= 1 {
        if mode.active {
            mode.active = false;
            mode.reason = SharedEncounterReason::None;
            mode.reversion_cooldown = 0.0;
        }
        return;
    }

    let boss_positions: Vec<Vec3> = boss_q
        .iter()
        .map(|transform| transform.translation)
        .collect();

    // Dual-threshold hysteresis check for FlyingDrone threats
    let is_currently_active = mode.active && mode.reason == SharedEncounterReason::DroneWing;
    let drone_positions = if is_currently_active {
        // Hysteresis exit / retention boundary: within 88.0 meters
        drone_q
            .iter()
            .map(|transform| transform.translation)
            .filter(|drone| players.iter().any(|player| player.distance(*drone) <= 88.0))
            .collect::<Vec<Vec3>>()
    } else {
        // Hysteresis entry / activation boundary: within 72.0 meters
        drone_q
            .iter()
            .map(|transform| transform.translation)
            .filter(|drone| players.iter().any(|player| player.distance(*drone) <= 72.0))
            .collect::<Vec<Vec3>>()
    };

    let drone_with_hysteresis = if is_currently_active {
        // Retention count threshold: keep camera active if at least 2 active drones remain
        drone_positions.len() >= 2
    } else {
        // Activation count threshold: trigger camera if at least 3 active drones group here
        drone_positions.len() >= 3
    };

    let (reason, threats): (SharedEncounterReason, &[Vec3]) = if !boss_positions.is_empty() {
        (SharedEncounterReason::Boss, &boss_positions)
    } else if drone_with_hysteresis {
        (SharedEncounterReason::DroneWing, &drone_positions)
    } else {
        (SharedEncounterReason::None, &[])
    };

    if reason == SharedEncounterReason::None {
        if mode.active {
            if mode.reversion_cooldown > 0.0 {
                // Return and let cooldown tick down before reverting
                return;
            } else if mode.reason != SharedEncounterReason::None {
                // Seed reversion cooldown
                mode.reversion_cooldown = 2.5;
                mode.reason = SharedEncounterReason::None;
                return;
            } else {
                // Cooldown elapsed fully
                mode.active = false;
                msg_ev.write(UiMessageEvent {
                    text: "Party camera released.".to_string(),
                    duration: 1.6,
                });
            }
        }
        return;
    }

    // Entering or remaining in an active threat mode: reset any reversion timing
    mode.reversion_cooldown = 0.0;

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

#[allow(dead_code)]
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
    let anchor = average_positions(threats)
        .or_else(|| average_positions(players))
        .unwrap_or(Vec3::ZERO);
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
    mut player_q: Query<
        (
            &PlayerIndex,
            &mut Transform,
            &mut PlayerMovement,
            Option<&BoatPassenger>,
        ),
        With<Player>,
    >,
) {
    if !mode.active {
        return;
    }

    let dt = time.delta_secs();
    let soft_radius = 54.0;
    let hard_radius = 108.0;
    for (index, mut transform, mut movement, boat_passenger) in player_q.iter_mut() {
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
            movement.clear_motor_delivery();
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
    mut player_q: Query<
        (
            &PlayerIndex,
            &mut Transform,
            &mut PlayerMovement,
            Option<&BoatPassenger>,
        ),
        With<Player>,
    >,
) {
    if !dungeon.active {
        return;
    }

    let dt = time.delta_secs();
    let soft_radius = dungeon.radius * 0.52;
    let hard_radius = dungeon.radius * 0.92;
    for (index, mut transform, mut movement, boat_passenger) in player_q.iter_mut() {
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
            movement.clear_motor_delivery();
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

fn traversal_mode_switch_update(
    sim: Res<SimConfig>,
    buffers: Res<PlayerInputBuffers>,
    mut player_q: Query<(&PlayerIndex, &PlayerInput, &mut TraversalModeState), With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (idx, input, mut traversal) in player_q.iter_mut() {
        // EC1b: fixed tick reads the buffered edge; Update path reads live input.
        let switch = if sim.fixed_motor {
            buffers.fixed(idx.0).and_then(|f| f.edges.traversal)
        } else {
            input.traversal_mode_switch
        };
        let Some(mode) = switch else {
            continue;
        };
        if traversal.active == mode {
            continue;
        }
        traversal.active = mode;
        msg_ev.write(UiMessageEvent {
            text: format!("Traversal mode: {}", mode.label()),
            duration: 1.2,
        });
    }
}

// ── Movement & Physics ────────────────────────────────────────────────────────
/// EC1b interpolation source: record each player's position at the start of the
/// fixed tick, before the motor moves anything. Camera presentation lerps from
/// here to the live transform by the fixed-clock overstep fraction.
fn cache_previous_tick_positions(
    mut q: Query<(&Transform, &mut PreviousTickPosition), With<Player>>,
) {
    for (transform, mut prev) in q.iter_mut() {
        prev.0 = transform.translation;
    }
}

/// Fraction of the outstanding motor carry delivered each frame. Below 1.0 a
/// tick's worth of motion is spread over a couple of frames, which removes the
/// fixed-tick stutter; the remainder stays banked so nothing is lost.
///
/// With alternating tick delivery the steady-state ratio between a busy and a
/// quiet frame settles at `1 / (1 - DELIVERY)`, so 0.4 lands at ~1.7x (versus
/// unbounded for a raw flush) while costing only ~2.5 frames of latency.
const MOTOR_CARRY_DELIVERY: f32 = 0.4;
/// Distance under which the carry is delivered outright instead of chasing an
/// ever-halving remainder (also stops denormal drift while standing still).
const MOTOR_CARRY_SNAP: f32 = 1.0e-4;

/// Split the outstanding carry into (deliver-now, keep-for-later).
///
/// A 64 Hz simulation rendered at a higher refresh rate hands some frames a
/// whole tick of translation and others none, so a flush that delivered the
/// raw accumulation made the player stutter while the interpolated camera
/// glided — the "jitters when you move" report. Delivering a fixed fraction
/// per frame smooths the motion without changing where the player ends up.
fn split_motor_carry(carry: Vec3) -> (Vec3, Vec3) {
    if carry.length() <= MOTOR_CARRY_SNAP {
        return (carry, Vec3::ZERO);
    }
    let deliver = carry * MOTOR_CARRY_DELIVERY;
    (deliver, carry - deliver)
}

/// Flush per-tick accumulated translation (EC1b fixed-motor mode) onto the
/// controller once per frame. Physics steps per-frame, so this is where many
/// fixed ticks (or zero) collapse into a single move-and-slide input — see
/// [`split_motor_carry`] for why that is smoothed rather than applied raw.
fn flush_motor_translation(
    mut q: Query<(&mut KinematicCharacterController, &mut PlayerMovement), With<Player>>,
) {
    for (mut controller, mut movement) in q.iter_mut() {
        let accumulated = movement.motor_accum;
        movement.motor_carry += accumulated;
        movement.motor_accum = Vec3::ZERO;
        let (deliver, remainder) = split_motor_carry(movement.motor_carry);
        movement.motor_carry = remainder;
        controller.translation = Some(deliver);
    }
}

fn hoverboard_landing_approach(ground_distance: f32) -> f32 {
    (1.0 - ground_distance.max(0.0) / 4.8).clamp(0.0, 1.0)
}

fn hoverboard_landing_descent_cap(approach: f32) -> f32 {
    -(0.58 - approach.clamp(0.0, 1.0) * 0.46)
}

fn sabre_claims_movement_dodge(
    sabre: &BeamSabre,
    progression: &PlayerProgression,
    traversal: TraversalMode,
    is_grounded: bool,
) -> bool {
    sabre.active
        && traversal != TraversalMode::Hoverboard
        && progression
            .upgrades
            .sabre_dodge_technique_applicable(is_grounded)
}

fn sabre_claims_movement_heavy(sabre: &BeamSabre, progression: &PlayerProgression) -> bool {
    sabre.active && progression.upgrades.sabre_spin_unlocked()
}

#[derive(Debug, Clone, Copy)]
struct LedgeCandidate {
    anchor: Vec3,
    top: Vec3,
    normal: Vec3,
}

fn ledge_anchor_from_top(top: Vec3, wall_normal: Vec3) -> Vec3 {
    top + wall_normal.with_y(0.0).normalize_or_zero() * 0.52 - Vec3::Y * 0.76
}

fn ledge_height_is_reachable(player_y: f32, top_y: f32) -> bool {
    let rise = top_y - player_y;
    (0.28..=1.55).contains(&rise)
}

fn hoverboard_overdrive_requested(
    traversal: TraversalMode,
    dodge_pressed: bool,
    rail_bound: bool,
    cooldown: f32,
) -> bool {
    traversal == TraversalMode::Hoverboard && dodge_pressed && !rail_bound && cooldown <= 0.0
}

/// Validate a real ledge rather than treating every vertical contact as one:
/// chest ray hits a wall, head ray clears it, then a downward probe finds a
/// walkable top within reach. The returned root anchor stays fixed throughout
/// the hang so triangle-to-triangle contact noise cannot move the player.
fn find_ledge_candidate(
    spatial_query: &SpatialQuery,
    player_entity: Entity,
    player_position: Vec3,
    wall_normal: Vec3,
) -> Option<LedgeCandidate> {
    let normal = wall_normal.with_y(0.0).normalize_or_zero();
    if normal.length_squared() < 0.5 {
        return None;
    }
    let inward = -normal;
    let filter = SpatialQueryFilter::from_mask(GameCollisionLayer::World)
        .with_excluded_entities([player_entity]);

    let chest_origin = player_position + Vec3::Y * 0.34 + normal * 0.12;
    let chest =
        spatial_query.cast_ray(chest_origin, Dir3::new(inward).ok()?, 1.15, false, &filter)?;
    if chest.normal.y.abs() > 0.48 {
        return None;
    }

    let head_origin = player_position + Vec3::Y * 1.42 + normal * 0.12;
    if spatial_query
        .cast_ray(head_origin, Dir3::new(inward).ok()?, 1.05, false, &filter)
        .is_some()
    {
        return None;
    }

    let wall_point = chest_origin + inward * chest.distance;
    let top_origin = wall_point + inward * 0.42 + Vec3::Y * 1.65;
    let top_hit = spatial_query.cast_ray(top_origin, Dir3::NEG_Y, 2.0, false, &filter)?;
    if top_hit.normal.y < 0.58 {
        return None;
    }
    let top = top_origin + Vec3::NEG_Y * top_hit.distance;
    if !ledge_height_is_reachable(player_position.y, top.y) {
        return None;
    }

    Some(LedgeCandidate {
        anchor: ledge_anchor_from_top(top, normal),
        top,
        normal,
    })
}

#[derive(Debug, Clone, Copy)]
struct WaterContact {
    entity: Entity,
    surface_y: f32,
}

fn water_contact_at(
    position: Vec3,
    water_q: &Query<(Entity, &Transform, &WaterBody), Without<Player>>,
) -> Option<WaterContact> {
    water_q
        .iter()
        .filter(|(_, _, body)| body.kind != WaterBodyKind::Waterfall)
        .filter_map(|(entity, transform, body)| {
            let delta = position - transform.translation;
            let local = transform.rotation.inverse() * delta;
            let inside = body.footprint.contains(Vec2::new(local.x, local.z))
                && position.y <= body.surface_y + 0.9
                && position.y >= body.surface_y - body.depth - 1.0;
            inside.then_some(WaterContact {
                entity,
                surface_y: body.surface_y,
            })
        })
        .max_by(|a, b| a.surface_y.total_cmp(&b.surface_y))
}

fn player_movement(
    time: Res<Time>,
    spatial_query: SpatialQuery,
    water_q: Query<(Entity, &Transform, &WaterBody), Without<Player>>,
    dungeon: Res<DungeonCrawlState>,
    shared_camera: Res<SharedEncounterCamera>,
    player_config: Res<LocalPlayerConfig>,
    sim: Res<SimConfig>,
    buffers: Res<PlayerInputBuffers>,
    mut action_sfx: MessageWriter<ModularActionSfxEvent>,
    mut player_q: Query<
        (
            (Entity, &mut KinematicCharacterController),
            &KinematicCharacterControllerOutput,
            &mut PlayerMovement,
            &mut PlayerStats,
            &mut JetpackState,
            &mut GrappleHookState,
            &TraversalModeState,
            (&mut BoardBoostState, &mut PlatformerMoveState),
            &mut EdgeGrabState,
            &mut ClimbState,
            &mut DodgeState,
            &mut Transform,
            &mut PlayerStateMachine,
            (&PlayerIndex, &PlayerInput),
            (
                Option<&BoatPassenger>,
                &PlayerProgression,
                &BeamSabre,
                Option<&RailGrindState>,
                &mut WaterTraversalState,
            ),
        ),
        With<Player>,
    >,
) {
    let dt = time.delta_secs();
    for (
        (player_entity, mut controller),
        output,
        mut movement,
        mut stats,
        mut jetpack,
        mut grapple,
        traversal,
        (mut board_boost, mut platformer),
        mut edge_grab,
        mut climb,
        dodge,
        mut transform,
        mut state,
        (player_idx, pi),
        (boat_passenger, progression, sabre, rail_grind, mut water),
    ) in player_q.iter_mut()
    {
        // EC1b: on the fixed tick, consume the latched command buffer so edge
        // presses fire exactly once per tick at any render frame rate. The
        // legacy Update path (fixed_motor off) keeps the live PlayerInput.
        let buffered;
        let pi = if sim.fixed_motor {
            buffered = buffers
                .fixed(player_idx.0)
                .map(|f| f.overlay(pi))
                .unwrap_or_else(|| pi.clone());
            &buffered
        } else {
            pi
        };
        if boat_passenger.is_some() {
            water.body = None;
            water.swimming = false;
            water.submerged = false;
            water.wake_requested = false;
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity = Vec3::ZERO;
            movement.clear_motor_delivery();
            movement.is_grounded = true;
            movement.coyote_timer = movement.coyote_time;
            movement.jump_buffer_timer = 0.0;
            movement.wall_jump_lock_timer = 0.0;
            movement.wall_jump_charges = movement.max_wall_jump_charges;
            jetpack.is_active = false;
            jetpack.mode = FlightMode::Grounded;
            grapple.begin_recovery();
            edge_grab.release_hang();
            climb.is_climbing = false;
            *platformer = PlatformerMoveState::default();
            if sim.fixed_motor {
                movement.motor_accum += Vec3::ZERO;
            } else {
                controller.translation = Some(Vec3::ZERO);
            }
            state.force(PlayerState::Idle);
            continue;
        }

        if let Some(contact) = water_contact_at(transform.translation, &water_q) {
            water.body = Some(contact.entity);
            water.surface_y = contact.surface_y;
            water.swimming = true;
            water.submerged = transform.translation.y + 1.35 < contact.surface_y;
            water.wake_cooldown = (water.wake_cooldown - dt).max(0.0);

            jetpack.is_active = false;
            jetpack.mode = FlightMode::Grounded;
            grapple.begin_recovery();
            edge_grab.release_hang();
            climb.is_climbing = false;
            movement.is_grounded = false;
            movement.coyote_timer = 0.0;
            movement.jump_buffer_timer = 0.0;
            movement.wall_jump_lock_timer = 0.0;
            movement.wall_jump_charges = movement.max_wall_jump_charges;
            *platformer = PlatformerMoveState::default();

            let (forward, right) = if dungeon.active {
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
            let (swim_input, input_strength) =
                movement_input_from_axes(forward, right, pi.move_axis);
            let swim_speed = if pi.sprint { 0.42 } else { 0.30 };
            let target_horizontal = swim_input * swim_speed * input_strength;
            movement.ground_velocity =
                approach_vec3(movement.ground_velocity, target_horizontal, 2.8 * dt);

            let rest_y = contact.surface_y - 0.72;
            let buoyancy = ((rest_y - transform.translation.y) * 0.20).clamp(-0.20, 0.24);
            let target_vertical = if pi.jump {
                0.34
            } else if pi.dodge {
                -0.24
            } else if water.breath <= 2.0 {
                buoyancy.max(0.22)
            } else {
                buoyancy
            };
            movement.velocity.y = approach_f32(movement.velocity.y, target_vertical, 2.6 * dt);

            let translation =
                (movement.ground_velocity + Vec3::Y * movement.velocity.y) * dt * 60.0;
            if sim.fixed_motor {
                movement.motor_accum += translation;
            } else {
                controller.translation = Some(translation);
            }
            if state.current != PlayerState::Swimming {
                state.force(PlayerState::Swimming);
                action_sfx.write(ModularActionSfxEvent::new("water.enter"));
            } else if water.wake_cooldown <= 0.0 && movement.ground_velocity.length_squared() > 0.02
            {
                water.wake_cooldown = 0.42;
                water.wake_requested = true;
                action_sfx.write(ModularActionSfxEvent::new("water.swim"));
            }
            platformer.was_grounded = false;
            continue;
        }

        water.body = None;
        water.swimming = false;
        water.submerged = false;
        water.wake_requested = false;
        water.wake_cooldown = (water.wake_cooldown - dt).max(0.0);
        if state.current == PlayerState::Swimming {
            state.force(PlayerState::Idle);
            movement.velocity.y = movement.velocity.y.max(0.08);
            action_sfx.write(ModularActionSfxEvent::new("water.exit"));
        }

        jetpack.jump_tap_timer = (jetpack.jump_tap_timer - dt).max(0.0);
        if pi.jump {
            jetpack.register_jump_tap();
            movement.jump_buffer_timer = movement.jump_buffer_time;
        } else {
            movement.jump_buffer_timer = (movement.jump_buffer_timer - dt).max(0.0);
        }
        movement.wall_jump_lock_timer = (movement.wall_jump_lock_timer - dt).max(0.0);
        jetpack.air_dash_timer = (jetpack.air_dash_timer - dt).max(0.0);
        jetpack.air_dash_cooldown_timer = (jetpack.air_dash_cooldown_timer - dt).max(0.0);
        board_boost.timer = (board_boost.timer - dt).max(0.0);
        board_boost.manual_cooldown = (board_boost.manual_cooldown - dt).max(0.0);
        board_boost.landing_timer = (board_boost.landing_timer - dt).max(0.0);
        if board_boost.timer <= 0.0 {
            board_boost.speed_mult = 1.0;
            board_boost.direction = Vec3::ZERO;
        }

        movement.is_grounded = output.grounded;
        let just_landed = movement.is_grounded && !platformer.was_grounded;
        if traversal.active == TraversalMode::Hoverboard {
            if movement.is_grounded {
                if just_landed && board_boost.airborne_time >= 0.12 {
                    board_boost.landing_timer = 0.34;
                    action_sfx.write(ModularActionSfxEvent::new("hoverboard.land"));
                }
                board_boost.airborne_time = 0.0;
                board_boost.landing_approach = 0.0;
            } else {
                board_boost.airborne_time += dt;
            }
        } else {
            board_boost.airborne_time = 0.0;
            board_boost.landing_approach = 0.0;
        }
        let landed_stomp =
            movement.is_grounded && !platformer.was_grounded && platformer.stomp_active;
        let sabre_claims_dodge =
            sabre_claims_movement_dodge(sabre, progression, traversal.active, movement.is_grounded);
        let sabre_claims_heavy = sabre_claims_movement_heavy(sabre, progression);
        platformer.roll_timer = (platformer.roll_timer - dt).max(0.0);
        edge_grab.cooldown_timer = (edge_grab.cooldown_timer - dt).max(0.0);
        edge_grab.wall_contact_timer = (edge_grab.wall_contact_timer - dt).max(0.0);

        if movement.is_grounded {
            movement.coyote_timer = movement.coyote_time;
            movement.wall_jump_charges = movement.max_wall_jump_charges;
            movement.wall_jump_lock_timer = 0.0;
            jetpack.fuel = (jetpack.fuel + jetpack.regen_rate * dt).min(jetpack.max_fuel);
            climb.energy = (climb.energy + climb.regen_per_sec * dt).min(climb.max_energy);
            movement.velocity.y = movement.velocity.y.max(0.0);
            jetpack.mode = FlightMode::Grounded;
            edge_grab.release_hang();
            edge_grab.hang_timer = 0.0;
            edge_grab.wall_clasp_timer = 0.0;
            edge_grab.wall_contact_timer = 0.0;
            if landed_stomp {
                movement.velocity.y = platformer.stomp_bounce_force;
                movement.is_grounded = false;
                movement.coyote_timer = 0.0;
                platformer.stomp_active = false;
                state.force(PlayerState::Jetpack);
            }
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
        let (mut input, mut input_strength) = movement_input_from_axes(fwd, right, pi.move_axis);
        if hoverboard_overdrive_requested(
            traversal.active,
            pi.dodge,
            rail_grind.is_some(),
            board_boost.manual_cooldown,
        ) {
            let boost_direction = if input.length_squared() > 0.05 {
                input
            } else {
                fwd
            };
            board_boost.timer = traversal.hoverboard_manual_boost_duration;
            board_boost.manual_cooldown = traversal.hoverboard_manual_boost_duration + 0.28;
            board_boost.speed_mult = traversal.hoverboard_manual_boost_mult;
            board_boost.direction = boost_direction.normalize_or_zero();
            let minimum_launch = movement.sprint_speed * traversal.hoverboard_speed_mult * 1.85;
            let along = movement.ground_velocity.dot(board_boost.direction);
            if along < minimum_launch {
                movement.ground_velocity += board_boost.direction * (minimum_launch - along);
            }
            action_sfx.write(ModularActionSfxEvent::new("hoverboard.overdrive"));
        }
        let board_boost_active =
            board_boost.timer > 0.0 && board_boost.direction.length_squared() > 0.25;
        if board_boost_active && input_strength < 0.20 {
            input = board_boost.direction.normalize_or_zero();
            input_strength = 1.0;
        } else if board_boost_active {
            input = (input + board_boost.direction.normalize_or_zero() * 0.35).normalize_or_zero();
            input_strength = input_strength.max(0.85);
        }
        if movement.is_grounded
            && traversal.active == TraversalMode::Hoverboard
            && input_strength > 0.05
        {
            jetpack.mode = FlightMode::Hoverboard;
        }

        // Elastic speed-dampening scale based on player-focus distance for local co-op
        let mut speed_factor = 1.0;
        let active_coop = dungeon.active || (shared_camera.active && player_config.active > 1);
        if active_coop {
            let (anchor, soft_r, hard_r) = if dungeon.active {
                (dungeon.focus, dungeon.radius * 0.52, dungeon.radius * 0.92)
            } else {
                (shared_camera.anchor, 54.0, 108.0)
            };

            let to_anchor = (anchor - transform.translation).with_y(0.0);
            let distance = to_anchor.length();
            if distance > soft_r {
                let away_dir = -to_anchor.normalize_or_zero();
                let dot = input.dot(away_dir);
                if dot > 0.0 {
                    let progress = ((distance - soft_r) / (hard_r - soft_r)).clamp(0.0, 1.0);
                    speed_factor = 1.0 - progress * dot * 0.85;
                }
            }
        }

        let sprinting =
            pi.sprint && stats.stamina > 0.0 && input_strength >= movement.analog_sprint_threshold;
        let mode_speed_mult = if traversal.active == TraversalMode::Hoverboard {
            traversal.hoverboard_speed_mult
        } else {
            1.0
        } * board_boost.speed_mult.max(1.0);
        let speed = if sprinting {
            movement.sprint_speed
        } else {
            movement.walk_speed
        } * speed_factor
            * mode_speed_mult;

        if sprinting {
            stats.stamina = (stats.stamina - 15.0 * dt).max(0.0);
        }

        if movement.is_grounded
            && pi.dodge
            && !sabre_claims_dodge
            && traversal.active != TraversalMode::Hoverboard
            && movement.ground_velocity.length() >= platformer.roll_min_speed
        {
            platformer.rolling = true;
            platformer.roll_timer = 0.72;
        }
        if platformer.rolling
            && (platformer.roll_timer <= 0.0
                || movement.ground_velocity.length() < platformer.roll_min_speed * 0.55)
        {
            platformer.rolling = false;
        }

        if !movement.is_grounded
            && pi.melee_heavy
            && !sabre_claims_heavy
            && !platformer.stomp_active
        {
            platformer.stomp_active = true;
            movement.velocity.y = -platformer.stomp_speed;
            jetpack.is_active = false;
            jetpack.mode = FlightMode::Slam;
            state.force(PlayerState::Jetpack);
        }

        if movement.is_grounded && (sprinting || traversal.active == TraversalMode::Hoverboard) {
            if let Some(ground_normal) = ground_normal_from_controller_output(output) {
                let downhill = downhill_direction(ground_normal);
                let slope = (1.0 - ground_normal.y.clamp(0.0, 1.0)).max(0.0);
                if traversal.active == TraversalMode::Hoverboard {
                    let uphill = -downhill;
                    let uphill_intent = input.dot(uphill).max(0.0);
                    // Preserve the downhill surf, but add enough uphill drive and
                    // crest lift to make steep terrain read as a rideable wave.
                    movement.ground_velocity += downhill * slope * dt * 1.35;
                    movement.ground_velocity +=
                        uphill * slope * uphill_intent * traversal.hoverboard_uphill_assist * dt;
                    if slope > 0.06 && uphill_intent > 0.18 {
                        let wave_lift =
                            slope * uphill_intent * movement.ground_velocity.length() * 0.16;
                        movement.velocity.y = movement.velocity.y.max(wave_lift.min(0.30));
                    }
                } else {
                    movement.ground_velocity += downhill * slope * dt * 2.4;
                }
            }
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
            movement.clear_motor_delivery();
            jetpack.is_active = false;
            let Some(anchor) = edge_grab.ledge_anchor else {
                edge_grab.release_hang();
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
                movement.velocity.y = -0.12;
                state.force(PlayerState::Jetpack);
                continue;
            };
            transform.translation = anchor;

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
                edge_grab.release_hang();
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
                started_jump = true;
                state.force(PlayerState::Jetpack);
            } else if pi.interact {
                if let Some(top) = edge_grab.ledge_top {
                    transform.translation = top + edge_grab.wall_normal * 0.52 + Vec3::Y * 0.16;
                    movement.clear_motor_delivery();
                    movement.velocity.y = edge_grab.climb_boost;
                    movement.ground_velocity = -edge_grab.wall_normal * 0.18;
                } else {
                    let climb =
                        Vec3::Y * edge_grab.climb_boost * dt * 60.0 + edge_grab.wall_normal * 0.25;
                    if sim.fixed_motor {
                        movement.motor_accum += climb;
                    } else {
                        controller.translation = Some(climb);
                    }
                }
                movement.is_grounded = false;
                edge_grab.release_hang();
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
                state.force(PlayerState::Moving);
                continue;
            } else if pi.dodge && !sabre_claims_dodge
                || pi.move_axis.y < -0.35
                || stats.stamina <= 0.0
                || edge_grab.hang_timer >= edge_grab.max_hang_time
            {
                edge_grab.release_hang();
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
                movement.velocity.y = -0.12;
                state.transition(PlayerState::Jetpack);
            } else {
                if sim.fixed_motor {
                    movement.motor_accum += Vec3::ZERO;
                } else {
                    controller.translation = Some(Vec3::ZERO);
                }
                state.force(PlayerState::Hanging);
                continue;
            }
        }

        // ── Free climbing (mountains + buildings) ─────────────────────────────
        // Start: push firmly forward into a vertical surface with energy in the
        // climb bar. The wall-jump path below stays fully available mid-climb
        // (jump buffer → leap off the face), so climbing adds to — never
        // replaces — the double-jump-off-buildings verb.
        if !climb.is_climbing
            && !edge_grab.is_hanging
            && !dodge.is_dodging
            && has_wall_contact
            && input.dot(-edge_grab.wall_normal) > 0.35
            && climb.energy > climb.min_start_energy
            && edge_grab.cooldown_timer <= 0.0
            && !matches!(
                grapple.mode,
                GrappleHookMode::Swinging | GrappleHookMode::Zipping
            )
        {
            climb.is_climbing = true;
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity = Vec3::ZERO;
        }

        if climb.is_climbing {
            let wall_lost = !has_wall_contact;
            let bail =
                pi.dodge && !sabre_claims_dodge || pi.move_axis.y < -0.6 && movement.is_grounded;
            let exhausted = climb.energy <= 0.0;

            if movement.jump_buffer_timer > 0.0 && movement.wall_jump_charges > 0 {
                // Leap off the face — identical to the wall jump so the feel
                // and charge economy stay consistent.
                let jump_dir = (edge_grab.wall_normal + input * 0.25)
                    .with_y(0.0)
                    .normalize_or_zero();
                climb.is_climbing = false;
                movement.velocity.y = edge_grab.wall_jump_vertical;
                movement.ground_velocity = jump_dir * edge_grab.wall_jump_push;
                movement.wall_jump_charges = movement.wall_jump_charges.saturating_sub(1);
                movement.wall_jump_lock_timer = movement.wall_jump_lock_time;
                movement.jump_buffer_timer = 0.0;
                movement.coyote_timer = 0.0;
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
                state.force(PlayerState::Jetpack);
            } else if wall_lost {
                // Crested the top: vault up-and-over so ledges feel generous.
                climb.is_climbing = false;
                movement.velocity.y = climb.vault_boost;
                movement.ground_velocity = -edge_grab.wall_normal * 0.30;
                state.force(PlayerState::Jetpack);
            } else if bail || exhausted {
                climb.is_climbing = false;
                edge_grab.cooldown_timer = edge_grab.grab_cooldown;
                movement.velocity.y = movement.velocity.y.min(0.0);
                state.force(PlayerState::Jetpack);
            } else {
                climb.energy = (climb.energy - climb.drain_per_sec * dt).max(0.0);
                movement.is_grounded = false;
                movement.velocity = Vec3::ZERO;
                movement.ground_velocity = Vec3::ZERO;
                jetpack.is_active = false;

                // Move in the wall plane: stick Y climbs/descends, stick X
                // shimmies along the face; a light inward pull keeps contact.
                let up = Vec3::Y;
                let lateral = up.cross(edge_grab.wall_normal).normalize_or_zero();
                let climb_vel = up * pi.move_axis.y * climb.climb_speed
                    + lateral * -pi.move_axis.x * climb.lateral_speed
                    - edge_grab.wall_normal * 0.06;
                let translation = climb_vel * dt * 60.0;
                if sim.fixed_motor {
                    movement.motor_accum += translation;
                } else {
                    controller.translation = Some(translation);
                }
                state.force(PlayerState::Climbing);
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
            movement.velocity.y = movement.jump_force
                * if traversal.active == TraversalMode::Hoverboard {
                    traversal.hoverboard_jump_mult
                } else {
                    1.0
                };
            movement.jump_buffer_timer = 0.0;
            movement.coyote_timer = 0.0;
            movement.wall_jump_lock_timer = 0.0;
            movement.is_grounded = false;
            started_jump = true;
            state.transition(PlayerState::Jetpack);
        }

        let ledge_candidate = (!movement.is_grounded
            && !dodge.is_dodging
            && traversal.active != TraversalMode::Hoverboard
            && edge_grab.cooldown_timer <= 0.0
            && movement.velocity.y <= -0.02
            && pushing_into_wall
            && pi.interact
            && stats.stamina > 5.0)
            .then(|| {
                find_ledge_candidate(
                    &spatial_query,
                    player_entity,
                    transform.translation,
                    edge_grab.wall_normal,
                )
            })
            .flatten();

        if let Some(ledge) = ledge_candidate {
            edge_grab.begin_hang(ledge.anchor, ledge.top, ledge.normal);
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity = Vec3::ZERO;
            movement.clear_motor_delivery();
            transform.translation = ledge.anchor;
            controller.translation = Some(Vec3::ZERO);
            state.force(PlayerState::Hanging);
            continue;
        }

        let wants_air_traversal = pi.jetpack && !started_jump && !movement.is_grounded;
        jetpack.is_active = false;
        if jetpack.air_dash_timer > 0.0 {
            jetpack.mode = FlightMode::AirDash;
            movement.velocity.y = movement.velocity.y.max(0.0);
            movement.ground_velocity = approach_vec3(
                movement.ground_velocity,
                fwd * jetpack.air_dash_speed,
                dt * 9.0,
            );
            state.transition(PlayerState::Jetpack);
        } else if wants_air_traversal && jetpack.fuel > 0.0 {
            match traversal.active {
                TraversalMode::HoverJet => {
                    if jetpack.hover_mode_enabled {
                        movement.velocity.y *= (1.0 - 7.5 * dt).max(0.0);
                    } else {
                        movement.velocity.y = (movement.velocity.y + jetpack.force * 0.86)
                            .min(jetpack.max_vertical_vel);
                    }
                    jetpack.fuel -= jetpack.fuel_cost_per_sec * 0.72 * dt;
                    jetpack.is_active = true;
                    jetpack.mode =
                        if jetpack.hover_mode_enabled || movement.velocity.y.abs() < 0.075 {
                            FlightMode::Hover
                        } else {
                            FlightMode::JetBoost
                        };
                    state.transition(PlayerState::Jetpack);
                }
                TraversalMode::Flight => {
                    if jetpack.hover_mode_enabled {
                        movement.velocity.y *= (1.0 - 8.5 * dt).max(0.0);
                        movement.ground_velocity = approach_vec3(
                            movement.ground_velocity,
                            input * jetpack.boost_forward_speed * 0.58,
                            dt * 4.5,
                        );
                        jetpack.fuel -= jetpack.fuel_cost_per_sec * 0.34 * dt;
                        jetpack.is_active = true;
                        jetpack.mode = FlightMode::Hover;
                    } else if pi.dodge
                        && !sabre_claims_dodge
                        && jetpack.air_dash_cooldown_timer <= 0.0
                        && jetpack.fuel >= 18.0
                    {
                        jetpack.air_dash_timer = jetpack.air_dash_duration;
                        jetpack.air_dash_cooldown_timer = jetpack.air_dash_cooldown;
                        jetpack.fuel -= 18.0;
                        movement.ground_velocity = fwd * jetpack.air_dash_speed;
                        movement.velocity.y = movement.velocity.y.max(0.08);
                        jetpack.mode = FlightMode::AirDash;
                    } else if pi.move_axis.y < -0.55 && movement.velocity.y < -0.20 {
                        movement.velocity.y = -jetpack.slam_speed;
                        jetpack.mode = FlightMode::Slam;
                    } else if pi.sprint {
                        movement.velocity.y =
                            (movement.velocity.y + jetpack.force * 0.45).min(0.18);
                        movement.ground_velocity = approach_vec3(
                            movement.ground_velocity,
                            fwd * jetpack.boost_forward_speed,
                            dt * 3.8,
                        );
                        jetpack.fuel -= jetpack.fuel_cost_per_sec * 0.82 * dt;
                        jetpack.is_active = true;
                        jetpack.mode = FlightMode::JetBoost;
                    } else {
                        movement.velocity.y = movement.velocity.y.max(-jetpack.glide_fall_speed);
                        jetpack.fuel -= jetpack.fuel_cost_per_sec * 0.28 * dt;
                        jetpack.is_active = true;
                        jetpack.mode = FlightMode::Glide;
                    }
                    state.transition(PlayerState::Jetpack);
                }
                TraversalMode::Hoverboard => {
                    let forward_target = if input.length_squared() > 0.01 {
                        input
                    } else {
                        fwd
                    } * jetpack.boost_forward_speed
                        * traversal.hoverboard_rocket_forward_mult
                        * if pi.sprint { 1.30 } else { 0.82 };
                    movement.ground_velocity = approach_vec3(
                        movement.ground_velocity,
                        forward_target,
                        dt * if pi.sprint { 6.8 } else { 4.2 },
                    );
                    movement.velocity.y = (movement.velocity.y
                        + jetpack.force * traversal.hoverboard_rocket_lift_mult)
                        .min(jetpack.max_vertical_vel * 0.82);
                    jetpack.fuel -=
                        jetpack.fuel_cost_per_sec * traversal.hoverboard_rocket_fuel_mult * dt;
                    jetpack.is_active = true;
                    jetpack.mode = FlightMode::Hoverboard;
                    state.transition(PlayerState::Jetpack);
                }
                _ => {
                    movement.velocity.y =
                        (movement.velocity.y + jetpack.force).min(jetpack.max_vertical_vel);
                    jetpack.fuel -= jetpack.fuel_cost_per_sec * dt;
                    jetpack.is_active = true;
                    jetpack.mode = FlightMode::JetBoost;
                    state.transition(PlayerState::Jetpack);
                }
            }
            jetpack.fuel = jetpack.fuel.max(0.0);
        } else if !movement.is_grounded {
            jetpack.mode = if movement.velocity.y >= 0.0 {
                FlightMode::Jump
            } else {
                FlightMode::Fall
            };
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
            if traversal.active == TraversalMode::Hoverboard {
                gravity *= traversal.hoverboard_gravity_mult;
                if movement.velocity.y < 0.0 {
                    let filter = SpatialQueryFilter::from_mask(GameCollisionLayer::World);
                    let ground_distance = spatial_query
                        .cast_ray(
                            transform.translation + Vec3::Y * 0.15,
                            Dir3::NEG_Y,
                            4.8,
                            true,
                            &filter,
                        )
                        .map(|hit| hit.distance);
                    board_boost.landing_approach = ground_distance
                        .map(hoverboard_landing_approach)
                        .unwrap_or(0.0);
                } else {
                    board_boost.landing_approach = 0.0;
                }
            }
            movement.velocity.y -= gravity;
            if traversal.active == TraversalMode::Hoverboard
                && board_boost.landing_approach > 0.0
                && movement.velocity.y < 0.0
            {
                movement.velocity.y = movement
                    .velocity
                    .y
                    .max(hoverboard_landing_descent_cap(board_boost.landing_approach));
            }
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

        let mut target_h_vel = input * speed * input_strength;
        if platformer.rolling {
            let roll_direction = movement.ground_velocity.normalize_or_zero();
            let steered = (roll_direction + input * platformer.roll_steer).normalize_or_zero();
            target_h_vel = steered * movement.ground_velocity.length();
        }
        if traversal.active == TraversalMode::Hoverboard
            && movement.is_grounded
            && input_strength > 0.05
        {
            let current_speed = movement.ground_velocity.length();
            let target_speed = target_h_vel.length();
            let boosted_cap = movement.sprint_speed
                * traversal.hoverboard_speed_mult
                * board_boost.speed_mult.max(1.0)
                * 1.62;
            let cruise_cap = boosted_cap.max(movement.sprint_speed * 2.35);
            if current_speed > target_speed {
                target_h_vel = input * current_speed.min(cruise_cap);
            } else if sprinting {
                target_h_vel += input * 0.16;
            }
        }
        if traversal.active == TraversalMode::Hoverboard && !movement.is_grounded {
            let current_speed = movement.ground_velocity.length();
            if input_strength <= 0.05 {
                target_h_vel = movement.ground_velocity * 0.998;
            } else if current_speed > 0.05 {
                let current_direction = movement.ground_velocity.normalize_or_zero();
                let surf_direction = (current_direction * 0.74 + input * 0.26).normalize_or_zero();
                target_h_vel = surf_direction * current_speed.max(target_h_vel.length());
            }
        }
        let mut h_vel = if movement.is_grounded {
            let accel = if input.length_squared() > 0.01 {
                if traversal.active == TraversalMode::Hoverboard {
                    movement.ground_accel * 0.82
                } else {
                    movement.ground_accel
                }
            } else {
                if platformer.rolling {
                    platformer.roll_decel
                } else if traversal.active == TraversalMode::Hoverboard {
                    movement.ground_decel * 0.16
                } else {
                    movement.ground_decel
                }
            };
            movement.ground_velocity =
                approach_vec3(movement.ground_velocity, target_h_vel, accel * dt);
            movement.ground_velocity
        } else {
            let has_air_input = input_strength > 0.05;
            let air_accel = if !has_air_input {
                if traversal.active == TraversalMode::Hoverboard {
                    movement.air_decel * 0.08
                } else {
                    movement.air_decel
                }
            } else if edge_grab.cooldown_timer > 0.0 {
                movement.air_accel * 0.35
            } else if movement.wall_jump_lock_timer > 0.0 {
                movement.air_accel * 0.22
            } else if traversal.active == TraversalMode::Hoverboard {
                movement.air_accel * traversal.hoverboard_air_control_mult
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

        if let Some(grapple_velocity) = grapple_drive_velocity(
            &mut grapple,
            transform.translation,
            h_vel + Vec3::Y * movement.velocity.y,
            input,
            pi,
            dt,
        ) {
            h_vel = grapple_velocity.with_y(0.0);
            movement.velocity.y = grapple_velocity.y;
            movement.ground_velocity = h_vel;
            movement.is_grounded = false;
            edge_grab.release_hang();
            state.force(PlayerState::Grappling);
        }

        let mut translation = (h_vel + Vec3::new(0.0, movement.velocity.y, 0.0)) * dt * 60.0;
        if movement.knockback_velocity.length_squared() > 1e-4 {
            // Received-knockback shove (world-units/sec): same decay tuning as
            // the enemy drain, but integrated through the controller so the
            // shove respects collisions instead of teleporting the transform.
            translation += movement.knockback_velocity * dt;
            movement.knockback_velocity *= (-dt * 9.0).exp();
            if movement.knockback_velocity.length_squared() < 1e-4 {
                movement.knockback_velocity = Vec3::ZERO;
            }
        }
        if sim.fixed_motor {
            movement.motor_accum += translation;
        } else {
            controller.translation = Some(translation);
        }

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
        platformer.was_grounded = movement.is_grounded;
    }
}

fn speed_loop_traversal_system(
    time: Res<Time>,
    mut player_q: Query<
        (
            &mut Transform,
            &mut KinematicCharacterController,
            &mut PlayerMovement,
            &TraversalModeState,
            &mut SpeedLoopTraversalState,
        ),
        With<Player>,
    >,
    guide_q: Query<(Entity, &Transform, &SpeedLoopGuide), Without<Player>>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut controller, mut movement, traversal, mut loop_state) in
        player_q.iter_mut()
    {
        loop_state.cooldown = (loop_state.cooldown - dt).max(0.0);

        if loop_state.guide.is_none()
            && loop_state.cooldown <= 0.0
            && traversal.active == TraversalMode::Hoverboard
        {
            let speed = movement.ground_velocity.length();
            for (guide_entity, guide_transform, guide) in guide_q.iter() {
                if speed < guide.entry_speed {
                    continue;
                }
                let forward = Quat::from_rotation_y(guide.yaw) * Vec3::Z;
                let right = Vec3::new(forward.z, 0.0, -forward.x);
                let offset = transform.translation - guide_transform.translation;
                if offset.dot(right).abs() <= guide.lane_half_width
                    && offset.dot(forward).abs() <= 9.0
                    && offset.y.abs() <= 4.5
                    && movement.ground_velocity.normalize_or_zero().dot(forward) > 0.45
                {
                    loop_state.guide = Some(guide_entity);
                    loop_state.progress = 0.0;
                    loop_state.speed = speed.max(guide.entry_speed);
                    break;
                }
            }
        }

        let Some(guide_entity) = loop_state.guide else {
            continue;
        };
        let Ok((_, guide_transform, guide)) = guide_q.get(guide_entity) else {
            loop_state.guide = None;
            loop_state.cooldown = 0.4;
            continue;
        };

        let forward = Quat::from_rotation_y(guide.yaw) * Vec3::Z;
        loop_state.progress += loop_state.speed * 60.0 / guide.radius.max(1.0) * dt;
        let phi = loop_state.progress.min(std::f32::consts::TAU);
        let center = guide_transform.translation;
        let target = speed_loop_position(center, forward, guide.radius, phi);
        transform.translation = target;
        controller.translation = Some(Vec3::ZERO);
        movement.clear_motor_delivery();
        movement.velocity = Vec3::ZERO;
        movement.is_grounded = false;

        if loop_state.progress >= std::f32::consts::TAU {
            transform.translation = center + forward * 5.0 + Vec3::Y * 1.25;
            movement.ground_velocity = forward * loop_state.speed;
            movement.velocity.y = 0.06;
            loop_state.guide = None;
            loop_state.cooldown = 0.65;
        }
    }
}

fn speed_loop_position(center: Vec3, forward: Vec3, radius: f32, phi: f32) -> Vec3 {
    center
        + forward.normalize_or_zero() * (radius * phi.sin())
        + Vec3::Y * (radius * (1.0 - phi.cos()) + 1.25)
}

fn road_checkpoint_recovery_system(
    time: Res<Time>,
    checkpoint_q: Query<(&Transform, &SpeedRoadCheckpoint), Without<Player>>,
    mut player_q: Query<
        (
            &mut Transform,
            &mut PlayerMovement,
            &TraversalModeState,
            &SpeedLoopTraversalState,
            &mut RoadRecoveryState,
        ),
        With<Player>,
    >,
) {
    let dt = time.delta_secs();
    for (mut transform, mut movement, traversal, loop_state, mut recovery) in player_q.iter_mut() {
        recovery.cooldown = (recovery.cooldown - dt).max(0.0);
        if traversal.active != TraversalMode::Hoverboard || loop_state.guide.is_some() {
            continue;
        }

        for (checkpoint_transform, checkpoint) in checkpoint_q.iter() {
            if transform
                .translation
                .distance(checkpoint_transform.translation)
                <= checkpoint.radius
            {
                recovery.last_checkpoint = Some(checkpoint_transform.translation);
                break;
            }
        }

        let Some(checkpoint) = recovery.last_checkpoint else {
            continue;
        };
        let fell_below_route = transform.translation.y < checkpoint.y - 85.0;
        let lost_from_route =
            transform.translation.distance(checkpoint) > 780.0 && movement.velocity.y < -0.8;
        if recovery.cooldown <= 0.0 && (fell_below_route || lost_from_route) {
            transform.translation = checkpoint + Vec3::Y * 2.0;
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity *= 0.35;
            movement.clear_motor_delivery();
            recovery.cooldown = 1.0;
        }
    }
}

/// Recovers rare kinematic-controller misses beneath the main heightfield.
/// This is intentionally a deep-penetration guard rather than a replacement
/// for collision: ordinary jumps, slopes, flight, roads, and climbing continue
/// to be resolved by the character controller.
fn terrain_fall_recovery_system(
    time: Res<Time>,
    settings: Res<GameSettings>,
    dungeon: Res<DungeonCrawlState>,
    mut player_q: Query<
        (
            &mut Transform,
            &mut KinematicCharacterController,
            &mut PlayerMovement,
            &mut GrappleHookState,
            &mut EdgeGrabState,
            &mut ClimbState,
            &mut PreviousTickPosition,
            &mut TerrainRecoveryState,
        ),
        With<Player>,
    >,
) {
    const RECOVERY_DEPTH: f32 = 8.0;
    const RECOVERY_LIFT: f32 = 1.6;

    let dt = time.delta_secs();
    for (
        mut transform,
        mut controller,
        mut movement,
        mut grapple,
        mut edge_grab,
        mut climb,
        mut previous,
        mut recovery,
    ) in player_q.iter_mut()
    {
        recovery.cooldown = (recovery.cooldown - dt).max(0.0);
        let position = transform.translation;
        let inside_main_terrain = position.x.abs() <= EVEREST_RANGE_HALF_EXTENT
            && position.z.abs() <= EVEREST_RANGE_HALF_EXTENT;
        if dungeon.active || !inside_main_terrain {
            continue;
        }

        let surface_y = terrain_surface_y(position.x, position.z, settings.world_seed);
        // The controller's grounded flag can lag for a frame after tunnelling.
        // Only remember positions that are still on or above the authored surface.
        if movement.is_grounded && position.y >= surface_y - 1.0 {
            recovery.last_safe_position = position;
        }

        let fell_through = position.y < surface_y - RECOVERY_DEPTH && movement.velocity.y <= 0.0;
        if recovery.cooldown > 0.0 || !fell_through {
            continue;
        }

        let safe = recovery.last_safe_position;
        let safe_inside =
            safe.x.abs() <= EVEREST_RANGE_HALF_EXTENT && safe.z.abs() <= EVEREST_RANGE_HALF_EXTENT;
        let safe_surface_y = terrain_surface_y(safe.x, safe.z, settings.world_seed);
        let safe_is_valid = safe_inside && safe.y >= safe_surface_y - 1.0;
        let recovered_position = if safe_is_valid {
            safe + Vec3::Y * RECOVERY_LIFT
        } else {
            Vec3::new(position.x, surface_y + RECOVERY_LIFT, position.z)
        };

        transform.translation = recovered_position;
        previous.0 = recovered_position;
        controller.translation = Some(Vec3::ZERO);
        movement.velocity = Vec3::ZERO;
        movement.ground_velocity *= 0.25;
        movement.clear_motor_delivery();
        movement.is_grounded = false;
        grapple.begin_recovery();
        edge_grab.release_hang();
        climb.is_climbing = false;
        recovery.cooldown = 0.8;
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

fn ground_normal_from_controller_output(
    output: &KinematicCharacterControllerOutput,
) -> Option<Vec3> {
    output
        .collisions
        .iter()
        .filter_map(|collision| collision.hit.details.map(|details| details.normal2))
        .filter(|normal| normal.y > 0.45)
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .map(Vec3::normalize_or_zero)
}

fn downhill_direction(ground_normal: Vec3) -> Vec3 {
    let normal = ground_normal.normalize_or_zero();
    let gravity_on_plane = Vec3::NEG_Y - normal * Vec3::NEG_Y.dot(normal);
    gravity_on_plane.with_y(0.0).normalize_or_zero()
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

fn approach_f32(current: f32, target: f32, max_delta: f32) -> f32 {
    current + (target - current).clamp(-max_delta, max_delta)
}

fn grapple_drive_velocity(
    grapple: &mut GrappleHookState,
    position: Vec3,
    current_velocity: Vec3,
    move_input: Vec3,
    input: &PlayerInput,
    _dt: f32,
) -> Option<Vec3> {
    let attach_point = grapple.attach_point?;
    let to_anchor = attach_point - position;
    let distance = to_anchor.length();
    if distance <= 0.001 {
        grapple.begin_recovery();
        return None;
    }
    let dir = to_anchor / distance;

    match grapple.mode {
        GrappleHookMode::Zipping => {
            if input.jump || input.dodge {
                let carry = dir * grapple.zip_speed * 0.55 + move_input * 0.35;
                grapple.begin_recovery();
                return Some(carry);
            }
            if distance <= grapple.arrival_radius {
                return Some(current_velocity * 0.35);
            }
            let speed = match grapple.target_kind {
                GrappleTargetKind::MountainPull | GrappleTargetKind::RouteSocket => {
                    grapple.mountain_pull_speed
                }
                GrappleTargetKind::EnemyPull | GrappleTargetKind::BossWeakPoint => {
                    grapple.attack_pull_speed
                }
                _ => grapple.zip_speed,
            };
            let lift = Vec3::Y * (0.18 + dir.y.max(0.0) * 0.24);
            Some((dir * speed + lift).clamp_length_max(4.0))
        }
        GrappleHookMode::Swinging => {
            if input.jump || input.dodge {
                let release = (current_velocity
                    + move_input * 0.65
                    + dir.cross(Vec3::Y).normalize_or_zero() * 0.18)
                    .clamp_length_max(3.2);
                grapple.begin_recovery();
                return Some(release);
            }

            let radial_velocity = dir * current_velocity.dot(dir);
            let tangent_velocity = current_velocity - radial_velocity;
            let stretch = distance - grapple.cable_length;
            let radial_correction =
                dir * (stretch * grapple.swing_spring * 0.025).clamp(-1.35, 1.35);
            let damping = -radial_velocity * grapple.swing_damping * 0.025;
            let pump = move_input * 0.42 + Vec3::Y * input.move_axis.y.max(0.0) * 0.08;
            Some(
                (tangent_velocity * 0.992 + radial_correction + damping + pump)
                    .clamp_length_max(3.4),
            )
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct GrappleCandidate {
    point: Vec3,
    normal: Vec3,
    entity: Option<Entity>,
    kind: GrappleTargetKind,
    distance: f32,
}

fn grapple_candidate_score(
    origin: Vec3,
    aim_dir: Vec3,
    point: Vec3,
    max_distance: f32,
    priority: f32,
    radius: f32,
) -> Option<(f32, f32)> {
    let to_target = point - origin;
    let distance = to_target.length();
    if distance <= 1.2 || distance > max_distance + radius {
        return None;
    }
    let aim = aim_dir.dot(to_target / distance);
    if aim < 0.18 {
        return None;
    }
    let distance_score = 1.0 - (distance / max_distance).clamp(0.0, 1.0);
    Some((
        aim * 2.2 + priority + distance_score * 0.7 + radius * 0.03,
        distance,
    ))
}

fn grapple_mode_for_target(
    kind: GrappleTargetKind,
    distance: f32,
    aim_dir: Vec3,
) -> GrappleHookMode {
    match kind {
        GrappleTargetKind::SwingPoint => GrappleHookMode::Swinging,
        GrappleTargetKind::RouteSocket | GrappleTargetKind::BroadSurface
            if distance > 28.0 && aim_dir.y > 0.08 =>
        {
            GrappleHookMode::Swinging
        }
        _ => GrappleHookMode::Zipping,
    }
}

fn enemy_grapple_kind(
    enemy: &Enemy,
    boss: Option<&BossEnemy>,
    drone: Option<&FlyingDrone>,
) -> GrappleTargetKind {
    if boss.is_some() || matches!(enemy.enemy_type, EnemyType::Heavy | EnemyType::Hybrid) {
        GrappleTargetKind::BossWeakPoint
    } else if drone.is_some() {
        GrappleTargetKind::ZipPoint
    } else {
        GrappleTargetKind::EnemyPull
    }
}

// ── Grapple Hook Foundation ──────────────────────────────────────────────────
fn grapple_hook_update(
    time: Res<Time>,
    spatial_query: SpatialQuery,
    route_registry: Res<WorldRouteRegistry>,
    sim: Res<SimConfig>,
    buffers: Res<PlayerInputBuffers>,
    mut player_q: Query<
        (
            Entity,
            &Transform,
            (&PlayerIndex, &PlayerInput),
            &TraversalModeState,
            &mut GrappleHookState,
            &mut PlayerStateMachine,
        ),
        With<Player>,
    >,
    socket_q: Query<
        (
            Entity,
            &Transform,
            Option<&GrappleSocket>,
            Option<&WorldRouteMarker>,
        ),
        Or<(With<GrappleSocket>, With<WorldRouteMarker>)>,
    >,
    enemy_q: Query<
        (
            Entity,
            &Transform,
            &Enemy,
            Option<&BossEnemy>,
            Option<&FlyingDrone>,
        ),
        Without<DeadEnemy>,
    >,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    for (entity, transform, (idx, input), traversal, mut grapple, mut state) in player_q.iter_mut()
    {
        grapple.tick_foundation(dt);

        // EC1b: fixed tick reads the buffered edge; Update path reads live input.
        let grapple_just = if sim.fixed_motor {
            buffers
                .fixed(idx.0)
                .map(|f| f.edges.grapple)
                .unwrap_or(false)
        } else {
            input.grapple_just
        };
        if grapple_just && state.current != PlayerState::Dead && grapple.request_fire() {
            state.force(PlayerState::Grappling);
        }

        if grapple.mode == GrappleHookMode::Searching {
            let origin = transform.translation + Vec3::Y * 1.0;
            let aim_dir = transform.forward().as_vec3().normalize_or_zero();
            let max_distance = grapple.max_cable_length.min(86.0);
            let mut best: Option<(GrappleCandidate, f32)> = None;

            for (target_entity, target_transform, socket, route_marker) in socket_q.iter() {
                if target_entity == entity {
                    continue;
                }
                let mut priority = socket.map(|s| s.priority).unwrap_or(0.72);
                let radius = socket.map(|s| s.radius).unwrap_or(2.5);
                if let Some(marker) = route_marker {
                    let Some(route) = route_registry.get(marker.id) else {
                        continue;
                    };
                    if matches!(
                        route.state,
                        WorldRouteState::Locked | WorldRouteState::Blocked
                    ) {
                        continue;
                    }
                    priority += match route.state {
                        WorldRouteState::Open => 0.45,
                        WorldRouteState::Contested => 0.18,
                        WorldRouteState::Blocked | WorldRouteState::Locked => 0.0,
                    };
                }

                let point = target_transform.translation + Vec3::Y * radius.min(7.5) * 0.4;
                let Some((score, distance)) =
                    grapple_candidate_score(origin, aim_dir, point, max_distance, priority, radius)
                else {
                    continue;
                };
                let kind = socket
                    .map(|s| s.kind)
                    .unwrap_or(GrappleTargetKind::RouteSocket);
                let candidate = GrappleCandidate {
                    point,
                    normal: (origin - point).normalize_or_zero(),
                    entity: Some(target_entity),
                    kind,
                    distance,
                };
                if best.is_none_or(|(_, best_score)| score > best_score) {
                    best = Some((candidate, score));
                }
            }

            for (target_entity, target_transform, enemy, boss, drone) in enemy_q.iter() {
                let point = target_transform.translation
                    + Vec3::Y * if drone.is_some() { 1.2 } else { 0.9 };
                let kind = enemy_grapple_kind(enemy, boss, drone);
                let priority = match kind {
                    GrappleTargetKind::BossWeakPoint => 1.75,
                    GrappleTargetKind::ZipPoint => 1.25,
                    _ => 1.05,
                };
                let Some((score, distance)) =
                    grapple_candidate_score(origin, aim_dir, point, max_distance, priority, 1.5)
                else {
                    continue;
                };
                let candidate = GrappleCandidate {
                    point,
                    normal: (origin - point).normalize_or_zero(),
                    entity: Some(target_entity),
                    kind,
                    distance,
                };
                if best.is_none_or(|(_, best_score)| score > best_score) {
                    best = Some((candidate, score));
                }
            }

            if best.is_none() && traversal.active == TraversalMode::Grapple && aim_dir.y > -0.35 {
                let max_surface_distance = max_distance.min(42.0);
                let filter = SpatialQueryFilter::from_mask(GameCollisionLayer::World)
                    .with_excluded_entities([entity]);
                if let Ok(direction) = Dir3::new(aim_dir) {
                    if let Some(hit) = spatial_query.cast_ray(
                        origin,
                        direction,
                        max_surface_distance,
                        false,
                        &filter,
                    ) {
                        let point = origin + direction.as_vec3() * hit.distance;
                        let kind = if hit.normal.y < -0.45 && aim_dir.y > 0.12 {
                            GrappleTargetKind::SwingPoint
                        } else {
                            GrappleTargetKind::BroadSurface
                        };
                        best = Some((
                            GrappleCandidate {
                                point,
                                normal: hit.normal,
                                entity: None,
                                kind,
                                distance: hit.distance,
                            },
                            0.15,
                        ));
                    }
                }
            }

            if let Some((candidate, _)) = best {
                let mode = grapple_mode_for_target(candidate.kind, candidate.distance, aim_dir);
                grapple.attach(
                    candidate.point,
                    candidate.normal,
                    candidate.entity,
                    candidate.kind,
                    mode,
                    transform.translation,
                );
                state.force(PlayerState::Grappling);
                let label = match candidate.kind {
                    GrappleTargetKind::EnemyPull => "Enemy pull",
                    GrappleTargetKind::BossWeakPoint => "Boss weak point",
                    GrappleTargetKind::SwingPoint => "Swing",
                    GrappleTargetKind::MountainPull => "Mountain pull",
                    GrappleTargetKind::RouteSocket => "Route socket",
                    GrappleTargetKind::ZipPoint => "Zip",
                    GrappleTargetKind::UtilityPull => "Utility pull",
                    GrappleTargetKind::BroadSurface => "Surface latch",
                    GrappleTargetKind::Denied => "Denied",
                };
                msg_ev.write(UiMessageEvent {
                    text: format!("{label} locked."),
                    duration: 1.1,
                });
            } else {
                grapple.begin_recovery();
                msg_ev.write(UiMessageEvent {
                    text: "No clean hook target.".to_string(),
                    duration: 1.2,
                });
            }
        }

        if state.current == PlayerState::Grappling && !grapple.wants_animation_pose() {
            state.transition(PlayerState::Idle);
        }
    }
}

fn grapple_hook_impact_system(
    spatial_query: SpatialQuery,
    mut player_q: Query<
        (
            Entity,
            &mut Transform,
            &mut KinematicCharacterController,
            &mut PlayerMovement,
            &TraversalModeState,
            &mut GrappleHookState,
            &mut EdgeGrabState,
            &mut PlayerStateMachine,
        ),
        With<Player>,
    >,
    mut enemy_q: Query<
        (
            Entity,
            &mut Transform,
            &mut Health,
            &mut Damageable,
            &Enemy,
            Option<&BossEnemy>,
        ),
        Without<Player>,
    >,
    mut damaged_ev: MessageWriter<EnemyDamagedEvent>,
    mut killed_ev: MessageWriter<EnemyKilledEvent>,
) {
    for (
        player_entity,
        mut player_transform,
        mut controller,
        mut movement,
        traversal,
        mut grapple,
        mut edge_grab,
        mut state,
    ) in player_q.iter_mut()
    {
        if !matches!(grapple.mode, GrappleHookMode::Zipping) {
            continue;
        }

        let Some(point) = grapple.attach_point else {
            grapple.begin_recovery();
            continue;
        };
        let arrived = player_transform.translation.distance(point) <= grapple.arrival_radius + 0.35;
        let combat_target = matches!(
            grapple.target_kind,
            GrappleTargetKind::EnemyPull | GrappleTargetKind::BossWeakPoint
        );

        // World surfaces and authored traversal sockets complete as movement
        // targets, not enemy impacts. In Grapple mode, a validated nearby lip
        // becomes a stable hang/mantle handoff.
        if !combat_target {
            if !arrived {
                continue;
            }
            if traversal.active == TraversalMode::Grapple {
                if let Some(ledge) = find_ledge_candidate(
                    &spatial_query,
                    player_entity,
                    player_transform.translation,
                    grapple.attach_normal,
                ) {
                    movement.velocity = Vec3::ZERO;
                    movement.ground_velocity = Vec3::ZERO;
                    movement.clear_motor_delivery();
                    controller.translation = Some(Vec3::ZERO);
                    let rise = ledge.top.y - player_transform.translation.y;
                    if rise <= 0.62 {
                        player_transform.translation =
                            ledge.top + ledge.normal * 0.52 + Vec3::Y * 0.16;
                        movement.velocity.y = 0.18;
                        movement.ground_velocity = -ledge.normal * 0.24;
                        edge_grab.release_hang();
                        state.force(PlayerState::Moving);
                    } else {
                        player_transform.translation = ledge.anchor;
                        edge_grab.begin_hang(ledge.anchor, ledge.top, ledge.normal);
                        state.force(PlayerState::Hanging);
                    }
                }
            }
            grapple.begin_recovery();
            continue;
        }

        let Some(target_entity) = grapple.target_entity else {
            grapple.begin_recovery();
            continue;
        };
        let Ok((entity, mut target_transform, mut health, mut damageable, enemy, boss)) =
            enemy_q.get_mut(target_entity)
        else {
            grapple.begin_recovery();
            continue;
        };

        let player_pos = player_transform.translation;
        let distance = player_pos.distance(target_transform.translation);
        if distance > grapple.arrival_radius + 1.2 {
            continue;
        }

        let is_heavy = boss.is_some()
            || matches!(enemy.enemy_type, EnemyType::Heavy | EnemyType::Hybrid)
            || matches!(grapple.target_kind, GrappleTargetKind::BossWeakPoint);
        let damage = if is_heavy { 28.0 } else { 18.0 };
        let result = apply_damage(
            &mut health,
            &mut damageable,
            &DamageInfo::new(damage, DamageType::Kinetic),
        );
        damaged_ev.write(EnemyDamagedEvent {
            entity,
            damage: result.damage_amount,
            position: target_transform.translation,
        });
        if !is_heavy {
            target_transform.translation = target_transform
                .translation
                .lerp(player_pos + Vec3::Y * 0.7, 0.48);
        }
        if result.was_killed {
            killed_ev.write(EnemyKilledEvent {
                enemy_type: enemy.enemy_type.as_str().to_string(),
                credits: enemy.config.credits,
                experience: enemy.config.experience_value,
                position: target_transform.translation,
            });
        }
        grapple.begin_recovery();
    }
}

// ── Dodge Update ──────────────────────────────────────────────────────────────
/// Impulse → shove conversion for player-received knockback. Matches the
/// enemy drain's 2.4× impulse-to-velocity scale; flattened so gravity and the
/// grounded logic keep owning vertical motion.
fn received_knockback_shove(pending: Vec3) -> Vec3 {
    (pending * 2.4).with_y(0.0)
}

/// Horizontal distance under which two players start shoving apart — just
/// over two shoulder radii of the player capsule (0.26–0.50).
const PUSHBOX_SEPARATION_DISTANCE: f32 = 1.1;
/// Full-overlap separation push in world-units/sec.
const PUSHBOX_PUSH_SPEED: f32 = 4.5;
/// Vertical gap beyond which players are on different levels (one jumping
/// over the other) and should not shove each other.
const PUSHBOX_HEIGHT_GAP: f32 = 2.2;

/// The full-strength separation push for a pair of players at horizontal
/// offset `delta` (a→b), or `None` when they are far enough apart.
/// Perfectly stacked players break the tie along +X deterministically.
fn pushbox_separation_push(delta: Vec3) -> Option<Vec3> {
    if delta.y.abs() > PUSHBOX_HEIGHT_GAP {
        return None;
    }
    let flat = delta.with_y(0.0);
    let dist = flat.length();
    if dist >= PUSHBOX_SEPARATION_DISTANCE {
        return None;
    }
    let dir = if dist > 0.01 { flat / dist } else { Vec3::X };
    let overlap = 1.0 - dist / PUSHBOX_SEPARATION_DISTANCE;
    Some(dir * (overlap * PUSHBOX_PUSH_SPEED))
}

/// EC3 pushbox-lite: co-op players gently shoulder past each other instead of
/// occupying the same spot. Distance-based (2–4 players, no extra colliders);
/// the shove routes through `knockback_velocity`, so it resolves through the
/// character controller and fades once they separate. The per-frame addition
/// is scaled by the shove channel's own decay rate, making the steady-state
/// push ≈ `PUSHBOX_PUSH_SPEED` regardless of frame rate.
fn player_pushbox_separation(
    time: Res<Time>,
    mut player_q: Query<(&Transform, &mut PlayerMovement), With<Player>>,
) {
    // Matches the exp(-9t) knockback decay in player_movement.
    const DECAY_RATE: f32 = 9.0;
    let dt = time.delta_secs();
    let mut pairs = player_q.iter_combinations_mut();
    while let Some([(a_transform, mut a_movement), (b_transform, mut b_movement)]) =
        pairs.fetch_next()
    {
        let Some(push) = pushbox_separation_push(b_transform.translation - a_transform.translation)
        else {
            continue;
        };
        let step = push * (DECAY_RATE * dt);
        a_movement.knockback_velocity -= step;
        b_movement.knockback_velocity += step;
    }
}

/// EC2: drain `Damageable.pending_knockback` (accumulated by `apply_damage`
/// on every damage path) into the motor's decaying shove. Runs in `Motor`
/// before the state machine so a shoved player reacts the same frame.
fn player_knockback_intake(
    mut player_q: Query<(&mut Damageable, &mut PlayerMovement), With<Player>>,
) {
    for (mut damageable, mut movement) in player_q.iter_mut() {
        if damageable.pending_knockback.length_squared() < 1e-6 {
            continue;
        }
        movement.knockback_velocity += received_knockback_shove(damageable.pending_knockback);
        damageable.pending_knockback = Vec3::ZERO;
    }
}

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
            &PlayerProgression,
            Option<(&BeamSabre, &PlayerMovement, &TraversalModeState)>,
        ),
        With<Player>,
    >,
    mut dodge_ev: MessageWriter<PlayerDodgeEvent>,
) {
    let dt = time.delta_secs();
    for (mut dodge, mut stats, mut damageable, transform, mut state, pi, progression, sabre_ctx) in
        player_q.iter_mut()
    {
        dodge.cooldown_timer = (dodge.cooldown_timer - dt).max(0.0);
        let dodge_cost = dodge.dodge_cost * progression.perks.dodge_cost_mult();

        if dodge.is_dodging {
            dodge.dodge_timer -= dt;
            damageable.is_invulnerable = true;
            if dodge.dodge_timer <= 0.0 {
                dodge.is_dodging = false;
                damageable.is_invulnerable = false;
                state.transition(PlayerState::Idle);
            }
        }

        // While the Star Sabre is drawn and a dodge technique (Comet Dash /
        // Meteor Pound) applies to the current stance, the dodge press belongs
        // to `beam_sabre_update_system` — suppress the roll so one press never
        // fires both. Deliberately independent of the technique cooldown so the
        // outcome does not depend on cross-plugin system ordering.
        let sabre_claims_dodge = sabre_ctx.is_some_and(|(sabre, movement, traversal)| {
            sabre.active
                && traversal.active != TraversalMode::Hoverboard
                && progression
                    .upgrades
                    .sabre_dodge_technique_applicable(movement.is_grounded)
        });
        // Riding the board rebinds dodge to the grind trick.
        let board_claims_dodge = sabre_ctx.is_some_and(|(_, _, traversal)| {
            crate::tricks::hoverboard_claims_trick_input(traversal.active)
        });

        if pi.dodge
            && !sabre_claims_dodge
            && !board_claims_dodge
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
    mut player_q: Query<
        (
            &mut ParryState,
            &mut PlayerStateMachine,
            &PlayerInput,
            &PlayerProgression,
            Option<&TraversalModeState>,
        ),
        With<Player>,
    >,
) {
    let dt = time.delta_secs();
    for (mut parry, state, pi, progression, traversal) in player_q.iter_mut() {
        parry.cooldown_timer = (parry.cooldown_timer - dt).max(0.0);
        // Riding the board rebinds parry to the manual trick.
        if traversal.is_some_and(|t| crate::tricks::hoverboard_claims_trick_input(t.active)) {
            continue;
        }

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
            parry.parry_timer = parry.parry_window + progression.perks.parry_window_bonus();
            parry.cooldown_timer = parry.parry_cooldown;
        }
    }
}

// ── State Update ──────────────────────────────────────────────────────────────
fn water_survival_system(
    time: Res<Time>,
    mut player_q: Query<
        (
            &PlayerIndex,
            &mut WaterTraversalState,
            &mut Health,
            &mut Damageable,
        ),
        With<Player>,
    >,
    mut damaged_ev: MessageWriter<PlayerDamagedEvent>,
    mut ui_ev: MessageWriter<UiMessageEvent>,
) {
    let dt = time.delta_secs();
    for (index, mut water, mut health, mut damageable) in player_q.iter_mut() {
        if water.submerged {
            water.breath = (water.breath - dt).max(0.0);
            if water.breath <= 3.0 && !water.low_air_warned {
                water.low_air_warned = true;
                ui_ev.write(UiMessageEvent {
                    text: format!("P{} AIR LOW — jump toward the surface", index.0 + 1),
                    duration: 2.2,
                });
            }
            if water.breath <= 0.0 {
                water.drowning_tick -= dt;
                if water.drowning_tick <= 0.0 {
                    water.drowning_tick = 1.0;
                    let result = apply_damage(
                        &mut health,
                        &mut damageable,
                        &DamageInfo::new(10.0, DamageType::Drowning),
                    );
                    if result.damage_amount > 0.0 {
                        damaged_ev.write(PlayerDamagedEvent {
                            player_index: Some(index.0),
                            amount: result.damage_amount,
                            remaining: health.current,
                        });
                    }
                }
            }
        } else {
            water.breath = (water.breath + dt * 4.0).min(water.max_breath);
            water.drowning_tick = 1.0;
            if water.breath > 4.5 {
                water.low_air_warned = false;
            }
        }
    }
}

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
    mut q: Query<(&mut Health, &PlayerStateMachine, &mut PlayerProgression), With<Player>>,
) {
    let dt = time.delta_secs();
    for (mut health, state, mut progression) in q.iter_mut() {
        let regen =
            progression.perks.regen_per_sec() + progression.upgrades.rejuvenation_regen_per_sec();
        if state.current != PlayerState::Dead && health.is_alive() && health.current < health.max {
            let missing = health.max - health.current;
            let requested = (regen * dt).min(missing);
            let healed = progression
                .upgrades
                .consume_rejuvenation_for_heal(requested);
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
    mut q: Query<(&mut PlayerStats, &mut Health, &mut PlayerProgression), With<Player>>,
    mut level_ev: MessageWriter<PlayerLevelUpEvent>,
) {
    for (mut stats, mut health, mut progression) in q.iter_mut() {
        let xp_needed = stats.xp_for_next_level();
        if stats.experience >= xp_needed {
            stats.experience -= xp_needed;
            stats.level += 1;
            stats.max_stamina += 5.0;
            // The derived-cap sync observes the new level and expands the cap.
            // Filling the old cap first preserves the existing full-heal
            // behavior when that sync applies its fill ratio.
            health.current = health.max;
            stats.stamina = stats.max_stamina;
            progression.perks.award(1);
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

/// Recomputes each player's `HeroPowerSet` from their hero profile + current
/// robot pet collection whenever pets change mid-session.
fn hero_affinity_update_system(
    robot_pets: Res<RobotPetCollection>,
    mut player_q: Query<(&HeroPowerProfile, &mut HeroPowerSet), With<Player>>,
) {
    if !robot_pets.is_changed() {
        return;
    }
    for (profile, mut powers) in player_q.iter_mut() {
        *powers = profile.amplified_powers(&robot_pets);
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
        parry_ev.write(PlayerParryEvent {
            player_index,
            success: true,
        });
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

fn update_camera_post_processing(
    time: Res<Time>,
    chapter: Res<CurrentChapter>,
    player_q: Query<(&PlayerCameraRef, &WaterTraversalState), With<Player>>,
    mut cameras: Query<
        (
            &mut bevy::post_process::bloom::Bloom,
            &mut DistanceFog,
            &mut UnderwaterCameraBlend,
        ),
        With<PlayerCamera>,
    >,
) {
    let (base_bloom, base_fog) = chapter.biome.atmosphere_settings();
    let (_, base_fog_color, _, _) = chapter.biome.palette();
    for (camera_ref, water) in player_q.iter() {
        let Ok((mut bloom, mut fog, mut underwater)) = cameras.get_mut(camera_ref.0) else {
            continue;
        };
        let target = if water.submerged { 1.0 } else { 0.0 };
        underwater.0 = approach_f32(underwater.0, target, time.delta_secs() * 2.8);
        let (bloom_intensity, fog_density, fog_color) =
            underwater_post_process(base_bloom, base_fog, base_fog_color, underwater.0);
        bloom.intensity = bloom_intensity;
        fog.color = fog_color;
        fog.falloff = FogFalloff::ExponentialSquared {
            density: fog_density,
        };
    }
}

fn underwater_post_process(
    base_bloom: f32,
    base_fog: f32,
    base_fog_color: Color,
    blend: f32,
) -> (f32, f32, Color) {
    let blend = blend.clamp(0.0, 1.0);
    let underwater_color = Color::srgba(0.005, 0.16, 0.22, 1.0);
    (
        base_bloom * (1.0 - blend * 0.38),
        base_fog + (0.0065 - base_fog).max(0.0) * blend,
        lerp_srgba(base_fog_color, underwater_color, blend),
    )
}

fn lerp_srgba(from: Color, to: Color, amount: f32) -> Color {
    let from = from.to_srgba();
    let to = to.to_srgba();
    let amount = amount.clamp(0.0, 1.0);
    Color::srgba(
        from.red + (to.red - from.red) * amount,
        from.green + (to.green - from.green) * amount,
        from.blue + (to.blue - from.blue) * amount,
        from.alpha + (to.alpha - from.alpha) * amount,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource)]
    struct LedgeProbeFixture {
        player: Entity,
        candidate: Option<LedgeCandidate>,
    }

    fn sample_ledge_probe(spatial_query: SpatialQuery, mut fixture: ResMut<LedgeProbeFixture>) {
        fixture.candidate = find_ledge_candidate(
            &spatial_query,
            fixture.player,
            Vec3::new(0.9, -0.2, 0.0),
            Vec3::X,
        );
    }

    #[test]
    fn motor_carry_smooths_fixed_tick_stutter_without_losing_distance() {
        // 64 Hz sim rendered at ~120 Hz: alternating frames receive one
        // tick of translation or none. Raw delivery is what made the player
        // stutter while the interpolated camera glided.
        let tick_step = Vec3::new(0.25, 0.0, 0.0);
        let frames = 240;
        let mut carry = Vec3::ZERO;
        let mut delivered = Vec::new();
        let mut total = Vec3::ZERO;
        for frame in 0..frames {
            if frame % 2 == 0 {
                carry += tick_step;
            }
            let (deliver, remainder) = split_motor_carry(carry);
            carry = remainder;
            delivered.push(deliver.x);
            total += deliver;
        }

        // Conservation: everything accumulated is eventually delivered, so
        // the player ends up exactly where the simulation put them.
        let accumulated = tick_step.x * (frames as f32 / 2.0);
        assert!(
            (total.x + carry.x - accumulated).abs() < 1e-4,
            "carry must not lose or invent distance"
        );

        // Smoothness: no frame is left completely starved once the carry is
        // primed, and the busiest frame is far below a full raw tick.
        let steady = &delivered[8..];
        let min = steady.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = steady.iter().cloned().fold(0.0_f32, f32::max);
        assert!(min > 0.0, "every frame must move: {min}");
        assert!(max < tick_step.x * 0.85, "frame spike remains: {max}");
        // Raw delivery swings from 0.0 to a full tick (unbounded ratio);
        // smoothed, the steady state is 1/(1 - DELIVERY) ≈ 1.67.
        assert!(max / min < 1.8, "still uneven: {max} vs {min}");
    }

    #[test]
    fn motor_carry_snaps_the_last_sliver_instead_of_halving_forever() {
        let (deliver, remainder) = split_motor_carry(Vec3::new(1.0e-5, 0.0, 0.0));
        assert_eq!(remainder, Vec3::ZERO);
        assert!(deliver.x > 0.0);
        assert_eq!(split_motor_carry(Vec3::ZERO), (Vec3::ZERO, Vec3::ZERO));
    }

    #[test]
    fn hard_traversal_stop_clears_accumulated_and_smoothed_motion() {
        let mut movement = PlayerMovement {
            motor_accum: Vec3::new(0.4, 0.2, -0.1),
            motor_carry: Vec3::new(0.3, 0.0, 0.2),
            ..default()
        };
        movement.clear_motor_delivery();
        assert_eq!(movement.motor_accum, Vec3::ZERO);
        assert_eq!(movement.motor_carry, Vec3::ZERO);
    }

    #[test]
    fn ledge_contract_requires_reachable_top_and_builds_stable_anchor() {
        assert!(ledge_height_is_reachable(10.0, 10.30));
        assert!(ledge_height_is_reachable(10.0, 11.50));
        assert!(!ledge_height_is_reachable(10.0, 10.20));
        assert!(!ledge_height_is_reachable(10.0, 11.70));

        let top = Vec3::new(4.0, 11.0, -3.0);
        let anchor = ledge_anchor_from_top(top, Vec3::X);
        assert_eq!(anchor, Vec3::new(4.52, 10.24, -3.0));
    }

    #[test]
    fn ledge_probe_requires_real_wall_clearance_and_walkable_top() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            PhysicsPlugins::default(),
        ))
        .init_asset::<Mesh>()
        .add_message::<avian3d::prelude::CollisionStart>()
        .add_message::<avian3d::prelude::CollisionEnd>();
        let player = app.world_mut().spawn_empty().id();
        app.insert_resource(LedgeProbeFixture {
            player,
            candidate: None,
        })
        .add_systems(Update, sample_ledge_probe);
        app.world_mut().spawn((
            avian3d::prelude::Collider::cuboid(0.5, 1.2, 3.0),
            CollisionProfile::World.layers(),
            avian3d::prelude::RigidBody::Static,
            Transform::default(),
        ));

        app.update();
        app.update();

        let candidate = app
            .world()
            .resource::<LedgeProbeFixture>()
            .candidate
            .expect("short wall with open headroom should produce a ledge");
        assert!(candidate.normal.dot(Vec3::X) > 0.9);
        assert!((candidate.top.y - 0.6).abs() < 0.02);
        assert!((candidate.anchor.y + 0.16).abs() < 0.02);
    }

    #[test]
    fn rail_bound_grind_input_does_not_also_trigger_board_overdrive() {
        assert!(hoverboard_overdrive_requested(
            TraversalMode::Hoverboard,
            true,
            false,
            0.0,
        ));
        assert!(!hoverboard_overdrive_requested(
            TraversalMode::Hoverboard,
            true,
            true,
            0.0,
        ));
        assert!(!hoverboard_overdrive_requested(
            TraversalMode::Grapple,
            true,
            false,
            0.0,
        ));
    }

    #[test]
    fn board_pose_damping_rejects_per_frame_contact_normal_noise() {
        // Reproduces the jitter: a trimesh contact normal that pops between
        // two triangles (and vanishes on brief airborne frames) used to be
        // written straight into the deck rotation. Damped, an alternating
        // target must never move the pose more than a fraction of the gap.
        let dt = 1.0 / 120.0;
        let a = Vec3::new(0.12, 0.99, 0.0).normalize();
        let b = Vec3::new(-0.12, 0.99, 0.0).normalize();
        let mut smoothed = Vec3::Y;
        let mut max_step: f32 = 0.0;
        for frame in 0..120 {
            let target = if frame % 2 == 0 { a } else { b };
            let previous = smoothed;
            smoothed = smoothed
                .lerp(target, damp_factor(14.0, dt))
                .normalize_or(Vec3::Y);
            max_step = max_step.max(previous.angle_between(smoothed));
        }
        // Per-frame swing stays far below the 0.24 rad gap between targets.
        assert!(max_step < 0.03, "damped normal still snaps: {max_step}");
        // And it settles between the two, not pinned to whichever came last.
        assert!(smoothed.x.abs() < 0.05);
        assert!(smoothed.y > 0.9);
    }

    #[test]
    fn damp_factor_is_frame_rate_independent_and_bounded() {
        // Same elapsed time reached in one big step or many small ones must
        // land in the same place (within tolerance), so the pose does not
        // depend on frame rate.
        let coarse = damp_factor(16.0, 0.1);
        let mut fine = 0.0_f32;
        for _ in 0..10 {
            fine += (1.0 - fine) * damp_factor(16.0, 0.01);
        }
        assert!((coarse - fine).abs() < 1e-5);
        assert_eq!(damp_factor(16.0, 0.0), 0.0);
        assert!(damp_factor(16.0, 10.0) <= 1.0);
    }

    #[test]
    fn smoothstep_removes_the_bank_angle_threshold_flicker() {
        // The old code branched on `speed > 0.35`, so a speed dithering
        // around the edge flipped bank between 0.24 and 0.42 every frame.
        assert_eq!(smoothstep(0.15, 0.65, 0.10), 0.0);
        assert_eq!(smoothstep(0.15, 0.65, 0.90), 1.0);
        let just_below = smoothstep(0.15, 0.65, 0.349);
        let just_above = smoothstep(0.15, 0.65, 0.351);
        assert!((just_above - just_below).abs() < 0.02, "still steps");
        // Monotonic across the range.
        assert!(smoothstep(0.15, 0.65, 0.3) < smoothstep(0.15, 0.65, 0.5));
    }

    #[test]
    fn pushbox_separation_pushes_apart_only_when_close_and_level() {
        // Overlapping on the same level: push points from a toward b.
        let push = pushbox_separation_push(Vec3::new(0.5, 0.0, 0.0)).unwrap();
        assert!(push.x > 0.0);
        assert_eq!(push.y, 0.0);
        // Deeper overlap pushes harder.
        let deeper = pushbox_separation_push(Vec3::new(0.2, 0.0, 0.0)).unwrap();
        assert!(deeper.x > push.x);
        // Perfectly stacked players break the tie deterministically (+X).
        let stacked = pushbox_separation_push(Vec3::ZERO).unwrap();
        assert!(stacked.x > 0.0);
        // Far enough apart, or on different levels: no shove.
        assert!(pushbox_separation_push(Vec3::new(1.2, 0.0, 0.0)).is_none());
        assert!(pushbox_separation_push(Vec3::new(0.5, 3.0, 0.0)).is_none());
    }

    #[test]
    fn received_knockback_shove_scales_and_stays_horizontal() {
        // Matches the enemy drain's 2.4x impulse-to-velocity conversion.
        let shove = received_knockback_shove(Vec3::new(3.0, 5.0, -4.0));
        assert!((shove.x - 7.2).abs() < 1e-5);
        assert!((shove.z + 9.6).abs() < 1e-5);
        // Vertical impulse is dropped: gravity and grounded logic own Y.
        assert_eq!(shove.y, 0.0);
        // The decay in player_movement uses exp(-9 dt): after ~0.25 s the
        // shove is down to ~10%, mirroring apply_enemy_knockback's feel.
        let decayed = shove * (-0.25_f32 * 9.0).exp();
        assert!(decayed.length() < shove.length() * 0.12);
    }

    #[test]
    fn hoverboard_landing_assist_eases_descent_near_the_surface() {
        let far = hoverboard_landing_approach(4.8);
        let near = hoverboard_landing_approach(0.25);

        assert_eq!(far, 0.0);
        assert!(near > 0.9);
        assert!(hoverboard_landing_descent_cap(near) > -0.2);
        assert!(hoverboard_landing_descent_cap(far) < -0.5);
    }

    #[test]
    fn late_joining_player_inherits_only_discovered_sabre_relics() {
        let mut chapter = ChapterProgress::default();
        chapter.unlock("solar_sabre_glyph");
        chapter.unlock("storm_gem");
        chapter.unlock("unrelated_world_secret");
        let mut player = PlayerProgression::default();
        player.upgrades.unlock_relic("cyclone_slash_blueprint");

        assert_eq!(seed_discovered_sabre_relics(&chapter, &mut player), 2);
        assert!(player.upgrades.has_relic("solar_sabre_glyph"));
        assert!(player.upgrades.has_relic("storm_gem"));
        assert!(player.upgrades.has_relic("cyclone_slash_blueprint"));
        assert!(!player.upgrades.has_relic("unrelated_world_secret"));
        assert_eq!(seed_discovered_sabre_relics(&chapter, &mut player), 0);
    }

    #[test]
    fn blueprint_caps_become_stable_authored_bases() {
        let body = crate::character_blueprint::BodyRecipe::default();
        let mut blueprint = CharacterBlueprint::hero(
            "Cap Contract",
            body,
            crate::character_blueprint::CharacterPaletteRecipe {
                skin: Color::WHITE,
                outfit: Color::WHITE,
                accent: Color::WHITE,
                hair: Color::WHITE,
                eye: Color::WHITE,
            },
            CartoonAppearanceRecipe::default(),
        );
        blueprint.gameplay_stats.max_health = 142.0;
        blueprint.gameplay_stats.max_armor = 76.0;

        let (stats, base, _, _, _) = authored_player_defaults(Some(&blueprint), 1.0);

        assert_eq!(base.max_health, 142.0);
        assert_eq!(base.max_armor, 76.0);
        assert_eq!(stats.max_health, base.max_health);
        assert_eq!(stats.max_armor, base.max_armor);
    }

    #[test]
    fn level_up_changes_level_without_mutating_the_effective_health_cache() {
        let mut app = App::new();
        app.add_message::<PlayerLevelUpEvent>();
        app.add_systems(Update, player_level_up);
        let entity = app
            .world_mut()
            .spawn((
                Player,
                PlayerStats {
                    max_health: 142.0,
                    experience: 100,
                    ..default()
                },
                Health {
                    current: 50.0,
                    max: 142.0,
                },
                PlayerProgression::default(),
            ))
            .id();

        app.update();

        let stats = app.world().get::<PlayerStats>(entity).unwrap();
        let health = app.world().get::<Health>(entity).unwrap();
        assert_eq!(stats.level, 2);
        assert_eq!(stats.max_health, 142.0);
        assert_eq!(health.max, 142.0);
        assert_eq!(health.current, 142.0);
        assert_eq!(
            PlayerBaseStats {
                max_health: 142.0,
                max_armor: 100.0,
            }
            .derived_caps(stats.level, 0.0, 0.0, 0.0, 0.0)
            .max_health,
            152.0
        );
    }

    #[test]
    fn quick_item_heals_and_consumes_only_one_stack_item() {
        let mut app = App::new();
        app.add_message::<UiMessageEvent>();
        app.add_systems(Update, player_quick_item_system);

        let mut inventory = Inventory::default();
        inventory.add_item("health_pack", 2, 10);
        let mut health = Health::new(100.0);
        health.current = 20.0;
        let player = app
            .world_mut()
            .spawn((
                Player,
                PlayerIndex(1),
                PlayerInput {
                    use_quick_item: true,
                    ..default()
                },
                inventory,
                QuickItemSlot {
                    item_id: Some("health_pack".to_string()),
                },
                health,
                PlayerStats::default(),
            ))
            .id();

        app.update();

        let world = app.world();
        assert_eq!(world.get::<Health>(player).unwrap().current, 70.0);
        assert_eq!(
            world.get::<Inventory>(player).unwrap().count("health_pack"),
            1
        );
        assert_eq!(
            world
                .get::<QuickItemSlot>(player)
                .unwrap()
                .item_id
                .as_deref(),
            Some("health_pack")
        );
    }

    #[test]
    fn shield_pulse_updates_only_the_owned_local_player() {
        let mut remaining = [0.0; 4];
        let mut strength = [0.0; 4];

        set_player_shield_pulse(&mut remaining, &mut strength, Some(2), 0.42, 1.35);

        assert_eq!(remaining, [0.0, 0.0, 0.42, 0.0]);
        assert_eq!(strength, [0.0, 0.0, 1.35, 0.0]);
    }

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
    fn drawn_sabre_claims_every_matching_movement_input() {
        let sabre = BeamSabre {
            active: true,
            ..default()
        };
        let mut progression = PlayerProgression::default();
        progression.upgrades.unlock_relic("cyclone_slash_blueprint");
        progression.upgrades.unlock_relic("comet_dash_blueprint");

        assert!(sabre_claims_movement_heavy(&sabre, &progression));
        assert!(sabre_claims_movement_dodge(
            &sabre,
            &progression,
            TraversalMode::Flight,
            false,
        ));
        assert!(!sabre_claims_movement_dodge(
            &sabre,
            &progression,
            TraversalMode::Hoverboard,
            true,
        ));
    }

    #[test]
    fn upgraded_vincenzo_blueprint_uses_reference_silhouette() {
        let slot = PlayerSlotConfig::default();
        let blueprint = upgraded_player_blueprint("Vincenzo", &slot);

        assert!(blueprint.body.leg_length >= 1.45);
        assert!(blueprint.body.arm_length >= 1.30);
        assert!(blueprint.body.mass < 0.90);
        assert!(!blueprint.cartoon_appearance.has_hood);
        assert!(!blueprint.cartoon_appearance.has_cape);
        assert!(blueprint.cartoon_appearance.has_visor);
        assert!(blueprint.cartoon_appearance.has_shoulder_pads);
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
        assert!(jetpack.air_dash_speed > JetpackState::default().air_dash_speed);
    }

    #[test]
    fn grapple_candidate_prefers_forward_targets() {
        let origin = Vec3::ZERO;
        let aim = Vec3::NEG_Z;

        let forward =
            grapple_candidate_score(origin, aim, Vec3::new(0.0, 0.0, -16.0), 48.0, 1.0, 1.0);
        let behind =
            grapple_candidate_score(origin, aim, Vec3::new(0.0, 0.0, 16.0), 48.0, 1.0, 1.0);

        assert!(forward.is_some());
        assert!(behind.is_none());
    }

    #[test]
    fn grapple_mode_selects_swing_for_high_long_targets() {
        assert_eq!(
            grapple_mode_for_target(GrappleTargetKind::SwingPoint, 18.0, Vec3::NEG_Z),
            GrappleHookMode::Swinging
        );
        assert_eq!(
            grapple_mode_for_target(
                GrappleTargetKind::BroadSurface,
                38.0,
                Vec3::new(0.0, 0.2, -1.0).normalize()
            ),
            GrappleHookMode::Swinging
        );
        assert_eq!(
            grapple_mode_for_target(GrappleTargetKind::EnemyPull, 18.0, Vec3::NEG_Z),
            GrappleHookMode::Zipping
        );
    }

    #[test]
    fn downhill_direction_follows_slope_not_world_axis() {
        let normal = Vec3::new(0.4, 0.9, 0.0).normalize();
        let downhill = downhill_direction(normal);
        assert!(downhill.x > 0.9);
        assert!(downhill.y.abs() < 0.001);
    }

    #[test]
    fn platformer_state_has_readable_roll_and_stomp_tuning() {
        let state = PlatformerMoveState::default();
        assert!(state.roll_min_speed > 0.0);
        assert!(state.roll_decel < PlayerMovement::default().ground_decel);
        assert!(state.stomp_speed > PlayerMovement::default().jump_force);
        assert!(state.stomp_bounce_force >= PlayerMovement::default().jump_force);
    }

    #[test]
    fn speed_loop_path_returns_to_entry_after_full_turn() {
        let center = Vec3::new(4.0, 8.0, 12.0);
        let entry = speed_loop_position(center, Vec3::Z, 24.0, 0.0);
        let top = speed_loop_position(center, Vec3::Z, 24.0, std::f32::consts::PI);
        let exit = speed_loop_position(center, Vec3::Z, 24.0, std::f32::consts::TAU);
        assert!(entry.distance(exit) < 0.001);
        assert!((top.y - entry.y - 48.0).abs() < 0.001);
    }

    #[test]
    fn underwater_post_process_thickens_fog_and_reduces_bloom() {
        let base_color = Color::srgb(0.30, 0.38, 0.52);
        let dry = underwater_post_process(0.25, 0.0002, base_color, 0.0);
        let submerged = underwater_post_process(0.25, 0.0002, base_color, 1.0);

        assert_eq!(dry.0, 0.25);
        assert_eq!(dry.1, 0.0002);
        assert_eq!(dry.2, base_color);
        assert!(submerged.0 < dry.0);
        assert!(submerged.1 > dry.1);
        assert!(submerged.2.to_srgba().blue > submerged.2.to_srgba().red);
    }
}
#[test]
fn camera_follow_damps_small_locomotion_corrections_and_snaps_large_warps() {
    let current = Vec3::ZERO;
    let nearby = Vec3::new(0.0, 0.2, 0.4);
    let smoothed = smooth_camera_position(current, nearby, 1.0 / 60.0);
    assert!(smoothed.length() > 0.0);
    assert!(smoothed.length() < nearby.length());

    let warp = Vec3::new(30.0, 0.0, 0.0);
    assert_eq!(smooth_camera_position(current, warp, 1.0 / 60.0), warp);
}
