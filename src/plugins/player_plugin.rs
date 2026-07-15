use bevy::camera::Hdr;
use bevy::camera::{PerspectiveProjection, Projection, Viewport};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::chapters::chapter_map_location;
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
use crate::components::armor::ArmorSet;
use crate::components::character::{CartoonPart, JointMarker};
use crate::components::enemy::{BossEnemy, DeadEnemy, Enemy, EnemyType, FlyingDrone};
use crate::components::inventory::Inventory;
use crate::components::player::*;
use crate::components::weapon::*;
use crate::components::world::{
    BoatPassenger, SpeedLoopGuide, SpeedRoadCheckpoint, WorldAnchor, WorldRouteMarker,
};
use crate::damage::{apply_damage, DamageInfo, DamageType, Damageable, Health};
use crate::events::*;
use crate::game_loop::{fixed_motor_off, fixed_motor_on, PreviousTickPosition, SimConfig};
use crate::input_buffer::PlayerInputBuffers;
use crate::hero_roster::{apply_hero_runtime, hero_power_profile, HeroPowerProfile, HeroPowerSet};
use crate::perks::PerkTree;
use crate::physics::prelude::*;
use crate::player_mesh::attach_modular_player_mesh;
use crate::rendering::{Camera3dBundle, ShieldMaterial, ShieldMaterialUniform, ShieldPbrBundle};
use crate::resources::{
    is_stale_reference_blueprint, reference_appearance_recipe, reference_body_recipe, CameraShake,
    ChapterProgress, CurrentChapter, DungeonCrawlState, LocalPlayerConfig, PlaySessionTransition,
    PlayerPartLoadout, PlayerSelectState, PlayerSlotConfig, WorldRouteRegistry, WorldRouteState,
};
use crate::robot_pets::RobotPetCollection;
use crate::state::AppState;
use crate::upgrades::UpgradeLedger;

/// Route the player's visual through the new native modular humanoid
/// ([`crate::player_mesh`]) instead of the legacy `character_parts` meshes.
/// Flip to `false` to fall back to the original system.
const USE_MODULAR_PLAYER_MESH: bool = true;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct PlayerPlugin;

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
                    camera_shake_system,
                    update_camera_post_processing,
                    traversal_mode_switch_update,
                    grapple_hook_update,
                    player_movement,
                    speed_loop_traversal_system,
                    road_checkpoint_recovery_system,
                    grapple_hook_impact_system,
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
                    camera_shake_system,
                    update_camera_post_processing,
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
                    grapple_hook_impact_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(fixed_motor_on),
            )
            .add_systems(
                Update,
                (
                    player_dodge_update,
                    player_parry_update,
                    player_state_update,
                    player_stamina_regen,
                    player_perk_health_regen,
                    player_invulnerability_update,
                    player_level_up,
                    player_died_check,
                    hero_affinity_update_system,
                )
                    .chain()
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

    // Scale physical interaction parameters to match the visual character size.
    movement.autostep_height *= visual_scale;
    movement.autostep_min_width *= visual_scale;
    movement.ground_snap_distance *= visual_scale;
    movement.controller_offset *= visual_scale;

    (
        stats,
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
    perks: Res<PerkTree>,
    upgrades: Res<UpgradeLedger>,
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
        let (mut player_stats, mut player_movement, mut dodge_state, player_collider) =
            authored_player_defaults(Some(&runtime_blueprint), character_visual_scale);
        let mut jetpack = JetpackState::default();
        let mut weapon_inventory = WeaponInventory::default();
        let mut special_inventory = SpecialWeaponInventory::default();
        let mut melee_combo = MeleeCombo::new();
        apply_hero_runtime(
            hero_profile,
            hero_powers,
            &mut player_stats,
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

        // Apply perk and tech-upgrade HP bonuses to the authoritative max_health.
        player_stats.max_health += perks.hp_bonus() + upgrades.armor_health_bonus();

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
            .insert((
                RoadRecoveryState::default(),
                ArmorSet::default(),
                Inventory::default(),
                weapon_inventory,
                special_inventory,
                BeamSabre::default(),
                melee_combo,
            ))
            .id();

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
                        Vec3::ZERO,
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
                DistanceFog {
                    color: Color::srgba(0.02, 0.02, 0.08, 1.0),
                    falloff: FogFalloff::ExponentialSquared { density: 0.00018 },
                    ..default()
                },
            ))
            .id();

        commands.entity(player).insert(PlayerCameraRef(cam_entity));
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

// ── Camera Shake ──────────────────────────────────────────────────────────────
fn camera_shake_system(
    time: Res<Time>,
    mut shake: ResMut<CameraShake>,
    mut damage_ev: MessageReader<PlayerDamagedEvent>,
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
    shake: Res<CameraShake>,
    dungeon: Res<DungeonCrawlState>,
    shared_camera: Res<SharedEncounterCamera>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    player_q: Query<
        (
            &PlayerIndex,
            &Transform,
            &PlayerCameraRef,
            Option<&PlayerMovement>,
            Option<&GrappleHookState>,
            Option<&JetpackState>,
            Option<&TraversalModeState>,
            Option<&ClimbState>,
            Option<&PreviousTickPosition>,
        ),
        (With<Player>, Without<PlayerCamera>),
    >,
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

    let max_trauma = player_q
        .iter()
        .map(|(index, _, _, _, _, _, _, _, _)| shake.trauma_for(index.0))
        .fold(0.0_f32, f32::max);
    let shake_offset = camera_shake_offset(max_trauma);

    // Compute target single screen/unified shared viewport transform
    let shared_target_transform = if transition.last_was_dungeon {
        let party_focus = average_positions(
            &player_q
                .iter()
                .map(|(_, transform, _, _, _, _, _, _, _)| transform.translation)
                .collect::<Vec<_>>(),
        )
        .unwrap_or(dungeon.focus);
        let dungeon_focus =
            clamp_to_dungeon_focus(party_focus, dungeon.focus, dungeon.radius * 0.62);
        dungeon_crawl_camera_transform(dungeon_focus, dungeon.radius, shake_offset)
    } else {
        shared_boss_camera_transform(shared_camera.focus, shared_camera.radius, shake_offset)
    };

    let lead_camera = player_q
        .iter()
        .min_by_key(|(index, _, _, _, _, _, _, _, _)| index.0)
        .map(|(_, _, camera_ref, _, _, _, _, _, _)| camera_ref.0);

    for (
        index,
        player_transform,
        camera_ref,
        movement,
        grapple,
        jetpack,
        traversal,
        climb,
        prev_tick,
    ) in player_q.iter()
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
        let player_shake = camera_shake_offset(shake.trauma_for(index.0));

        if let Ok((camera_entity, mut camera_transform, pitch, mut camera, mut projection)) =
            cam_q.get_mut(camera_ref.0)
        {
            let mut local_ind_transform =
                player_camera_transform(player_transform, pitch.0, player_shake);
            let horizontal_speed = movement.map(|m| m.ground_velocity.length()).unwrap_or(0.0);
            let vertical_speed = movement.map(|m| m.velocity.y.abs()).unwrap_or(0.0);
            let speed_pullback = (horizontal_speed + vertical_speed * 0.35).clamp(0.0, 2.6);
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
            // Climbing: pull well back and lift so the face above (and drop
            // below) stays readable while scaling walls.
            let climb_pullback = climb
                .map(|c| if c.is_climbing { 4.6 } else { 0.0 })
                .unwrap_or(0.0);
            local_ind_transform.translation += player_transform.rotation
                * Vec3::new(
                    0.0,
                    flight_lift + speed_pullback * 0.18 + climb_pullback * 0.45,
                    hook_pullback + board_pullback + speed_pullback + climb_pullback,
                );
            if let Projection::Perspective(ref mut perspective) = *projection {
                let target_fov = (58.0
                    + speed_pullback * 4.0
                    + hook_pullback * 1.2
                    + board_pullback * 1.6
                    + climb_pullback * 2.2)
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
                *camera_transform = local_ind_transform;
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

fn shared_boss_camera_transform(focus: Vec3, radius: f32, shake_offset: Vec3) -> Transform {
    let distance = (radius * 1.25).clamp(34.0, 96.0);
    let height = (radius * 0.72 + 14.0).clamp(24.0, 72.0);
    let translation = focus + Vec3::new(0.0, height, distance) + shake_offset;
    Transform::from_translation(translation).looking_at(focus + Vec3::Y * 2.2, Vec3::Y)
}

fn dungeon_crawl_camera_transform(focus: Vec3, radius: f32, shake_offset: Vec3) -> Transform {
    let height = (radius * 1.12).clamp(46.0, 92.0);
    let z_offset = (radius * 0.22).clamp(10.0, 22.0);
    let translation = focus + Vec3::new(0.0, height, z_offset) + shake_offset;
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
    let anchor = average_positions(threats).unwrap_or_else(|| average_positions(players).unwrap());
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
    mut player_q: Query<(&PlayerIndex, &mut Transform, Option<&BoatPassenger>), With<Player>>,
) {
    if !mode.active {
        return;
    }

    let dt = time.delta_secs();
    let soft_radius = 54.0;
    let hard_radius = 108.0;
    for (index, mut transform, boat_passenger) in player_q.iter_mut() {
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
    mut player_q: Query<(&PlayerIndex, &mut Transform, Option<&BoatPassenger>), With<Player>>,
) {
    if !dungeon.active {
        return;
    }

    let dt = time.delta_secs();
    let soft_radius = dungeon.radius * 0.52;
    let hard_radius = dungeon.radius * 0.92;
    for (index, mut transform, boat_passenger) in player_q.iter_mut() {
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

/// Flush per-tick accumulated translation (EC1b fixed-motor mode) onto the
/// controller once per frame, then clear it. Physics steps per-frame, so this is
/// where many fixed ticks (or zero) collapse into a single move-and-slide input.
fn flush_motor_translation(
    mut q: Query<(&mut KinematicCharacterController, &mut PlayerMovement), With<Player>>,
) {
    for (mut controller, mut movement) in q.iter_mut() {
        controller.translation = Some(movement.motor_accum);
        movement.motor_accum = Vec3::ZERO;
    }
}

fn player_movement(
    time: Res<Time>,
    dungeon: Res<DungeonCrawlState>,
    shared_camera: Res<SharedEncounterCamera>,
    player_config: Res<LocalPlayerConfig>,
    sim: Res<SimConfig>,
    buffers: Res<PlayerInputBuffers>,
    mut player_q: Query<
        (
            &mut KinematicCharacterController,
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
            &Transform,
            &mut PlayerStateMachine,
            (&PlayerIndex, &PlayerInput),
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
        mut grapple,
        traversal,
        (mut board_boost, mut platformer),
        mut edge_grab,
        mut climb,
        dodge,
        transform,
        mut state,
        (player_idx, pi),
        boat_passenger,
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
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity = Vec3::ZERO;
            movement.is_grounded = true;
            movement.coyote_timer = movement.coyote_time;
            movement.jump_buffer_timer = 0.0;
            movement.wall_jump_lock_timer = 0.0;
            movement.wall_jump_charges = movement.max_wall_jump_charges;
            jetpack.is_active = false;
            jetpack.mode = FlightMode::Grounded;
            grapple.begin_recovery();
            edge_grab.is_hanging = false;
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
        if board_boost.timer <= 0.0 {
            board_boost.speed_mult = 1.0;
            board_boost.direction = Vec3::ZERO;
        }

        movement.is_grounded = output.grounded;
        let landed_stomp =
            movement.is_grounded && !platformer.was_grounded && platformer.stomp_active;
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
            edge_grab.is_hanging = false;
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
        let mode_speed_mult =
            if traversal.active == TraversalMode::Hoverboard && movement.is_grounded {
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

        if !movement.is_grounded && pi.melee_heavy && !platformer.stomp_active {
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
                movement.ground_velocity += downhill * slope * dt * 2.4;
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
                let climb =
                    Vec3::Y * edge_grab.climb_boost * dt * 60.0 + edge_grab.wall_normal * 0.25;
                if sim.fixed_motor {
                    movement.motor_accum += climb;
                } else {
                    controller.translation = Some(climb);
                }
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
            let bail = pi.dodge || pi.move_axis.y < -0.6 && movement.is_grounded;
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
            if sim.fixed_motor {
                movement.motor_accum += Vec3::ZERO;
            } else {
                controller.translation = Some(Vec3::ZERO);
            }
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
            movement.velocity.y -= gravity;
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
                movement.air_decel
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
            edge_grab.is_hanging = false;
            state.force(PlayerState::Grappling);
        }

        let translation = (h_vel + Vec3::new(0.0, movement.velocity.y, 0.0)) * dt * 60.0;
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
        movement.motor_accum = Vec3::ZERO;
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
            movement.motor_accum = Vec3::ZERO;
            recovery.cooldown = 1.0;
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
    for (entity, transform, (idx, input), traversal, mut grapple, mut state) in
        player_q.iter_mut()
    {
        grapple.tick_foundation(dt);

        // EC1b: fixed tick reads the buffered edge; Update path reads live input.
        let grapple_just = if sim.fixed_motor {
            buffers.fixed(idx.0).map(|f| f.edges.grapple).unwrap_or(false)
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
                let distance = max_distance.min(42.0);
                let point = origin + aim_dir * distance + Vec3::Y * (aim_dir.y.max(0.0) * 8.0);
                best = Some((
                    GrappleCandidate {
                        point,
                        normal: -aim_dir,
                        entity: None,
                        kind: if aim_dir.y > 0.12 {
                            GrappleTargetKind::SwingPoint
                        } else {
                            GrappleTargetKind::BroadSurface
                        },
                        distance,
                    },
                    0.15,
                ));
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
    mut player_q: Query<(&Transform, &mut GrappleHookState), With<Player>>,
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
    for (player_transform, mut grapple) in player_q.iter_mut() {
        if !matches!(grapple.mode, GrappleHookMode::Zipping) {
            continue;
        }
        let Some(target_entity) = grapple.target_entity else {
            if let Some(point) = grapple.attach_point {
                if player_transform.translation.distance(point) <= grapple.arrival_radius {
                    grapple.begin_recovery();
                }
            }
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
    mut dodge_ev: MessageWriter<PlayerDodgeEvent>,
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
    perks: Res<PerkTree>,
    mut upgrades: ResMut<UpgradeLedger>,
    mut q: Query<(&mut Health, &PlayerStateMachine), With<Player>>,
) {
    let regen = perks.regen_per_sec() + upgrades.rejuvenation_regen_per_sec();
    if regen <= 0.0 {
        return;
    }
    let dt = time.delta_secs();
    for (mut health, state) in q.iter_mut() {
        if state.current != PlayerState::Dead && health.is_alive() && health.current < health.max {
            let missing = health.max - health.current;
            let requested = (regen * dt).min(missing);
            let healed = upgrades.consume_rejuvenation_for_heal(requested);
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
    mut q: Query<(&mut PlayerStats, &mut Health), With<Player>>,
    mut perks: ResMut<PerkTree>,
    mut level_ev: MessageWriter<PlayerLevelUpEvent>,
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
    chapter: Res<CurrentChapter>,
    mut cameras: Query<
        (&mut bevy::post_process::bloom::Bloom, &mut DistanceFog),
        With<PlayerCamera>,
    >,
) {
    if chapter.is_changed() {
        let (bloom, fog) = chapter.biome.atmosphere_settings();
        let (_, fog_col, _, _) = chapter.biome.palette();
        for (mut b, mut f) in cameras.iter_mut() {
            b.intensity = bloom;
            f.color = fog_col;
            f.falloff = FogFalloff::ExponentialSquared { density: fog };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
