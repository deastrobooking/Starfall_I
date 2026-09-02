//! Bounded four-player shared-screen platformer vertical slice.
//!
//! The course deliberately reuses Starfall's production jump, wall-climb,
//! ledge, stomp, melee, and local-controller systems. This module owns only the
//! format-specific contract: authored side-progressing geometry, P1-led bounds,
//! catch-up bubbles, co-op switches, compact battle pockets, checkpoints, and
//! workshop-part rewards.

use bevy::prelude::*;

use crate::combat::damage::Health;
use crate::components::enemy::{DeadEnemy, EnemyType};
use crate::components::faction::Faction;
use crate::components::player::{Player, PlayerIndex, PlayerInput, PlayerMovement, PlayerStats};
use crate::components::world::{
    CollapsePlatform, CollapsePlatformState, MovingPlatform, SlingShotPad, WalkableSurface,
    WorldGeometry,
};
use crate::engine::physics::prelude::{Collider, KinematicCharacterController, RigidBody};
use crate::engine::rendering::{PbrBundle, PointLightBundle};
use crate::engine::state::AppState;
use crate::events::UiMessageEvent;
use crate::plugins::enemy_plugin::spawn_enemy_entity;
use crate::resources::{PlayExperience, PlaySessionTransition};

/// Runtime rules for Heavy Water's bounded shared-screen platformer mode.
///
/// World scene generation remains a host concern during the transition, while
/// this plugin owns the mode's state and continuous gameplay rules. That seam
/// lets the scene-spawn path move without changing the public mode contract.
pub struct HeavyWaterPlatformerPlugin;

impl Plugin for HeavyWaterPlatformerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlatformerWorkshopProgress>()
            .add_systems(
                Update,
                (
                    platformer_party_boundary_and_bubble_system,
                    platformer_checkpoint_and_reward_system,
                    platformer_pressure_gate_system,
                    platformer_encounter_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

pub fn reset_platformer_progress(
    transition: Res<PlaySessionTransition>,
    experience: Res<PlayExperience>,
    mut progress: ResMut<PlatformerWorkshopProgress>,
) {
    if !transition.resuming_from_pause && *experience == PlayExperience::SharedPlatformer {
        *progress = PlatformerWorkshopProgress::default();
    }
}

pub const PLATFORMER_STAGE_ID: &str = "starbound_coop_course_01";
pub const PLATFORMER_STAGE_LABEL: &str = "Starbound Starter Courses";
pub const PLATFORMER_STAGE_ORIGIN: Vec3 = Vec3::new(0.0, 0.0, 0.0);
pub const PLATFORMER_MIN_X: f32 = -10.0;
pub const PLATFORMER_MAX_X: f32 = 170.0;
pub const PLATFORMER_HALF_DEPTH: f32 = 13.0;
pub const PLATFORMER_CAMERA_FOCUS: Vec3 = Vec3::new(80.0, 4.0, 0.0);
pub const PLATFORMER_CAMERA_RADIUS: f32 = 92.0;

/// The beginner course set is intentionally described as small lessons. These
/// definitions are also the contract mirrored by Forge's shared-screen level
/// templates, so runtime progression and designer vocabulary stay aligned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SharedPlatformerLevelDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub lesson: &'static str,
    pub start_x: f32,
    pub end_x: f32,
    pub checkpoint: Vec3,
}

pub const SIMPLE_PLATFORMER_LEVELS: [SharedPlatformerLevelDefinition; 3] = [
    SharedPlatformerLevelDefinition {
        id: "shared_screen_basics",
        label: "1-1 Star Steps",
        lesson: "Run together and climb the wide steps",
        start_x: -8.0,
        end_x: 52.0,
        checkpoint: Vec3::new(-5.5, 2.0, 0.0),
    },
    SharedPlatformerLevelDefinition {
        id: "shared_screen_jumps",
        label: "1-2 Cloud Hops",
        lesson: "Cross short gaps with roomy landings",
        start_x: 52.0,
        end_x: 108.0,
        checkpoint: Vec3::new(54.0, 2.0, 0.0),
    },
    SharedPlatformerLevelDefinition {
        id: "shared_screen_teamwork",
        label: "1-3 Moving Day",
        lesson: "Ride a slow lift and use the pulse spring",
        start_x: 108.0,
        end_x: 168.0,
        checkpoint: Vec3::new(110.0, 2.0, 0.0),
    },
];

pub fn platformer_spawn(index: u8) -> Vec3 {
    PLATFORMER_STAGE_ORIGIN
        + Vec3::new(
            -5.5 + index as f32 * 1.8,
            2.0,
            if index.is_multiple_of(2) { -1.2 } else { 1.2 },
        )
}

#[derive(Resource, Debug, Clone)]
pub struct PlatformerWorkshopProgress {
    pub star_parts: u32,
    pub checkpoint: Vec3,
    pub current_level: u8,
    pub finished: bool,
}

impl Default for PlatformerWorkshopProgress {
    fn default() -> Self {
        Self {
            star_parts: 0,
            checkpoint: platformer_spawn(0),
            current_level: 0,
            finished: false,
        }
    }
}

impl PlatformerWorkshopProgress {
    pub fn unlock_summary(&self) -> &'static str {
        match self.star_parts {
            0..=2 => "Collect 3 parts: prefab kit",
            3..=5 => "Prefab kit ready — collect 6 for weapon modules",
            6..=8 => "Weapon modules ready — collect 9 for armor shells",
            9..=11 => "Armor shells ready — collect 12 for pet/vehicle cores",
            _ => "Prefab, weapon, armor, pet, and vehicle cores ready",
        }
    }
}

#[derive(Component, Debug, Default, Clone, Copy)]
pub struct CoopBubbleState {
    pub active: bool,
    pub release_timer: f32,
}

#[derive(Component)]
pub struct CoopBubbleVisual(pub u8);

#[derive(Component)]
pub struct PlatformerStageRoot;

#[derive(Component, Debug, Clone, Copy)]
pub struct PlatformerCheckpoint {
    pub order: u8,
    pub respawn: Vec3,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PlatformerPartPickup {
    pub value: u32,
    pub collected: bool,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PlatformerPressurePlate {
    pub puzzle: u8,
    pub slot: u8,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PlatformerPuzzleGate {
    pub puzzle: u8,
    pub closed: Vec3,
    pub open: Vec3,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PlatformerEncounterSpawner {
    pub encounter: u8,
    pub trigger_x: f32,
    pub enemy_type: EnemyType,
    pub count: u8,
    pub spawned: bool,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PlatformerEncounterEnemy(pub u8);

#[derive(Component, Debug, Clone, Copy)]
pub struct PlatformerBattleGate {
    pub encounter: u8,
    pub closed: Vec3,
    pub open: Vec3,
}

#[derive(Component, Debug, Default, Clone, Copy)]
pub struct PlatformerFinishBeacon {
    pub claimed: bool,
}

#[derive(Clone)]
struct StageMaterials {
    floor: Handle<StandardMaterial>,
    aqua: Handle<StandardMaterial>,
    pink: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
    hazard: Handle<StandardMaterial>,
    bubble: Handle<StandardMaterial>,
}

/// Spawn three short, contiguous beginner levels under one shared camera. A
/// contiguous set keeps couch co-op instant (no loading or party teardown),
/// while checkpoints and level announcements make each lesson feel discrete.
pub fn spawn_shared_platformer_stage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mats = StageMaterials {
        floor: materials.add(toon_material(Color::srgb(0.16, 0.25, 0.52), 0.18)),
        aqua: materials.add(toon_material(Color::srgb(0.08, 0.92, 0.92), 1.8)),
        pink: materials.add(toon_material(Color::srgb(1.0, 0.20, 0.66), 1.5)),
        gold: materials.add(toon_material(Color::srgb(1.0, 0.76, 0.10), 2.0)),
        hazard: materials.add(toon_material(Color::srgb(0.72, 0.08, 0.24), 1.0)),
        bubble: materials.add(StandardMaterial {
            base_color: Color::srgba(0.15, 0.86, 1.0, 0.26),
            emissive: LinearRgba::new(0.05, 0.85, 1.6, 1.0),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    };

    commands.spawn((
        Name::new(PLATFORMER_STAGE_LABEL),
        PlatformerStageRoot,
        WorldGeometry,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 13_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0)),
        WorldGeometry,
    ));
    for (position, color) in [
        (Vec3::new(24.0, 16.0, 7.0), Color::srgb(0.1, 0.8, 1.0)),
        (Vec3::new(80.0, 16.0, -6.0), Color::srgb(1.0, 0.2, 0.7)),
        (Vec3::new(140.0, 18.0, 5.0), Color::srgb(1.0, 0.75, 0.1)),
    ] {
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color,
                    intensity: 2_800_000.0,
                    range: 54.0,
                    shadow_maps_enabled: true,
                    ..default()
                },
                transform: Transform::from_translation(position),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // 1-1 Star Steps: uninterrupted ground, then broad low steps. Four-player
    // depth is retained everywhere so learning movement never becomes a queue.
    spawn_solid(
        commands,
        meshes,
        Vec3::new(6.0, 0.0, 0.0),
        Vec3::new(28.0, 1.0, 22.0),
        mats.floor.clone(),
    );
    for i in 0..5 {
        let height = 1.0 + i as f32 * 0.8;
        spawn_solid(
            commands,
            meshes,
            Vec3::new(22.0 + i as f32 * 4.0, height * 0.5, 0.0),
            Vec3::new(4.4, height, 18.0),
            mats.aqua.clone(),
        );
    }
    spawn_solid(
        commands,
        meshes,
        Vec3::new(44.0, 2.1, 0.0),
        Vec3::new(12.0, 1.0, 20.0),
        mats.aqua.clone(),
    );
    spawn_solid(
        commands,
        meshes,
        Vec3::new(51.0, 0.8, 0.0),
        Vec3::new(4.0, 1.0, 20.0),
        mats.floor.clone(),
    );

    // 1-2 Cloud Hops: every gap is short and every landing is wider than a
    // four-player group, producing a gentle first jumping level.
    for (center, height, width) in [
        (57.0, 0.0, 10.0),
        (69.0, 1.0, 9.0),
        (80.5, 2.0, 9.0),
        (92.0, 1.0, 9.0),
        (103.0, 0.0, 10.0),
    ] {
        spawn_solid(
            commands,
            meshes,
            Vec3::new(center, height, 0.0),
            Vec3::new(width, 1.0, 18.0),
            if center == 80.5 {
                mats.pink.clone()
            } else {
                mats.floor.clone()
            },
        );
    }

    // 1-3 Moving Day: one slow horizontal lift followed by a forgiving spring
    // and a large finish deck. This introduces motion without lethal panels or
    // combat gates competing for attention.
    spawn_solid(
        commands,
        meshes,
        Vec3::new(112.0, 0.0, 0.0),
        Vec3::new(10.0, 1.0, 20.0),
        mats.floor.clone(),
    );
    let lift_size = Vec3::new(8.0, 0.7, 14.0);
    let lift_start = Vec3::new(121.0, 1.0, 0.0);
    let lift_end = Vec3::new(130.0, 3.0, 0.0);
    commands.spawn((
        Name::new("Beginner Four-Player Lift"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(lift_size.x, lift_size.y, lift_size.z))),
            material: MeshMaterial3d(mats.aqua.clone()),
            transform: Transform::from_translation(lift_start),
            ..default()
        },
        RigidBody::KinematicPositionBased,
        Collider::cuboid(lift_size.x * 0.5, lift_size.y * 0.5, lift_size.z * 0.5),
        WalkableSurface,
        MovingPlatform {
            start: lift_start,
            end: lift_end,
            speed: 1.35,
            phase: 0.0,
            size: lift_size,
        },
        WorldGeometry,
    ));
    spawn_solid(
        commands,
        meshes,
        Vec3::new(138.0, 2.5, 0.0),
        Vec3::new(10.0, 1.0, 20.0),
        mats.pink.clone(),
    );
    commands.spawn((
        Name::new("Beginner Pulse Spring"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(3.4, 0.8))),
            material: MeshMaterial3d(mats.pink.clone()),
            transform: Transform::from_translation(Vec3::new(146.0, 1.0, 0.0)),
            ..default()
        },
        RigidBody::Fixed,
        Collider::cylinder(0.4, 3.4),
        SlingShotPad {
            launch_velocity: Vec3::new(0.62, 0.82, 0.0),
            radius: 4.2,
            cooldown: 0.4,
            cooldown_timer: 0.0,
        },
        WorldGeometry,
    ));
    spawn_solid(
        commands,
        meshes,
        Vec3::new(158.0, 1.0, 0.0),
        Vec3::new(20.0, 1.0, 22.0),
        mats.gold.clone(),
    );

    for (order, level) in SIMPLE_PLATFORMER_LEVELS.iter().enumerate() {
        let respawn = level.checkpoint;
        commands.spawn((
            Name::new(format!("{} Checkpoint", level.label)),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Torus::new(1.1, 1.5))),
                material: MeshMaterial3d(mats.aqua.clone()),
                transform: Transform::from_translation(respawn + Vec3::Y * 1.8)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ..default()
            },
            PlatformerCheckpoint {
                order: order as u8,
                respawn,
            },
            WorldGeometry,
        ));
    }

    for (i, position) in [
        Vec3::new(8.0, 2.0, -3.0),
        Vec3::new(24.0, 3.0, 3.0),
        Vec3::new(36.0, 6.0, -3.0),
        Vec3::new(46.0, 4.2, 3.0),
        Vec3::new(58.0, 2.0, -3.0),
        Vec3::new(69.0, 3.0, 3.0),
        Vec3::new(80.5, 4.0, -3.0),
        Vec3::new(103.0, 2.0, 3.0),
        Vec3::new(112.0, 2.0, -3.0),
        Vec3::new(128.0, 5.0, 3.0),
        Vec3::new(140.0, 4.5, -3.0),
        Vec3::new(160.0, 3.0, 3.0),
    ]
    .into_iter()
    .enumerate()
    {
        commands.spawn((
            Name::new(format!("Starter Course Star Part {i}")),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.72))),
                material: MeshMaterial3d(mats.gold.clone()),
                transform: Transform::from_translation(position),
                ..default()
            },
            PlatformerPartPickup {
                value: 1,
                collected: false,
            },
            WorldGeometry,
        ));
    }

    commands.spawn((
        Name::new("Starter Courses Finish Star"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(3.0, 7.0))),
            material: MeshMaterial3d(mats.gold.clone()),
            transform: Transform::from_xyz(164.0, 5.0, 0.0),
            ..default()
        },
        PlatformerFinishBeacon::default(),
        WorldGeometry,
    ));

    let bubble_mesh = meshes.add(Sphere::new(1.65));
    for owner in 0..4u8 {
        commands.spawn((
            Name::new(format!("P{} Catch-up Bubble", owner + 1)),
            PbrBundle {
                mesh: Mesh3d(bubble_mesh.clone()),
                material: MeshMaterial3d(mats.bubble.clone()),
                visibility: Visibility::Hidden,
                ..default()
            },
            CoopBubbleVisual(owner),
            WorldGeometry,
        ));
    }

    let course_center = (PLATFORMER_MIN_X + PLATFORMER_MAX_X) * 0.5;
    let course_length = PLATFORMER_MAX_X - PLATFORMER_MIN_X;
    for z in [-PLATFORMER_HALF_DEPTH, PLATFORMER_HALF_DEPTH] {
        spawn_solid(
            commands,
            meshes,
            Vec3::new(course_center, 3.0, z),
            Vec3::new(course_length, 6.0, 0.8),
            mats.aqua.clone(),
        );
    }
    for x in [PLATFORMER_MIN_X, PLATFORMER_MAX_X] {
        spawn_solid(
            commands,
            meshes,
            Vec3::new(x, 8.0, 0.0),
            Vec3::new(1.0, 18.0, PLATFORMER_HALF_DEPTH * 2.0),
            mats.hazard.clone(),
        );
    }
}

/// Retained as an advanced internal reference course while the playable entry
/// point uses the smaller beginner set above.
#[allow(dead_code)]
pub fn spawn_shared_platformer_showcase_stage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mats = StageMaterials {
        floor: materials.add(toon_material(Color::srgb(0.16, 0.25, 0.52), 0.18)),
        aqua: materials.add(toon_material(Color::srgb(0.08, 0.92, 0.92), 1.8)),
        pink: materials.add(toon_material(Color::srgb(1.0, 0.20, 0.66), 1.5)),
        gold: materials.add(toon_material(Color::srgb(1.0, 0.76, 0.10), 2.0)),
        hazard: materials.add(toon_material(Color::srgb(0.72, 0.08, 0.24), 1.0)),
        bubble: materials.add(StandardMaterial {
            base_color: Color::srgba(0.15, 0.86, 1.0, 0.26),
            emissive: LinearRgba::new(0.05, 0.85, 1.6, 1.0),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    };

    commands.spawn((
        Name::new(PLATFORMER_STAGE_LABEL),
        PlatformerStageRoot,
        WorldGeometry,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 13_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0)),
        WorldGeometry,
    ));
    for (position, color) in [
        (Vec3::new(35.0, 18.0, 7.0), Color::srgb(0.1, 0.8, 1.0)),
        (Vec3::new(118.0, 24.0, -6.0), Color::srgb(1.0, 0.2, 0.7)),
        (Vec3::new(202.0, 20.0, 4.0), Color::srgb(1.0, 0.75, 0.1)),
    ] {
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color,
                    intensity: 3_500_000.0,
                    range: 70.0,
                    shadow_maps_enabled: true,
                    ..default()
                },
                transform: Transform::from_translation(position),
                ..default()
            },
            WorldGeometry,
        ));
    }

    // Wide but intentionally bounded 3D lanes. Gaps split the course into
    // readable screen-sized challenges rather than an open-world field.
    for (center, size, material) in [
        (
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::new(30.0, 1.0, 24.0),
            mats.floor.clone(),
        ),
        (
            Vec3::new(39.0, 2.0, 0.0),
            Vec3::new(22.0, 1.0, 20.0),
            mats.aqua.clone(),
        ),
        (
            Vec3::new(72.0, 5.0, 0.0),
            Vec3::new(24.0, 1.0, 18.0),
            mats.floor.clone(),
        ),
        (
            Vec3::new(112.0, 3.0, 0.0),
            Vec3::new(30.0, 1.0, 22.0),
            mats.pink.clone(),
        ),
        (
            Vec3::new(153.0, 7.0, 0.0),
            Vec3::new(28.0, 1.0, 18.0),
            mats.floor.clone(),
        ),
        (
            Vec3::new(196.0, 4.0, 0.0),
            Vec3::new(44.0, 1.0, 24.0),
            mats.gold.clone(),
        ),
    ] {
        spawn_solid(commands, meshes, center, size, material);
    }

    // Stairs into a climb tower, with generous landings for four bodies.
    for i in 0..6 {
        let size = Vec3::new(4.2, 0.8 + i as f32 * 0.55, 12.0);
        let center = Vec3::new(22.0 + i as f32 * 3.4, size.y * 0.5, 0.0);
        spawn_solid(commands, meshes, center, size, mats.aqua.clone());
    }
    spawn_solid(
        commands,
        meshes,
        Vec3::new(62.0, 10.0, 0.0),
        Vec3::new(4.0, 20.0, 15.0),
        mats.pink.clone(),
    );
    for y in [3.0, 8.0, 13.0, 18.0] {
        spawn_solid(
            commands,
            meshes,
            Vec3::new(66.0, y, if (y as i32 / 5) & 1 == 0 { -5.0 } else { 5.0 }),
            Vec3::new(9.0, 0.7, 5.0),
            mats.gold.clone(),
        );
    }

    // Moving lift across the first real gap.
    let lift_size = Vec3::new(7.0, 0.7, 9.0);
    let lift_start = Vec3::new(85.0, 5.0, -4.0);
    let lift_end = Vec3::new(96.0, 8.0, 4.0);
    commands.spawn((
        Name::new("Starbound Moving Lift"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(lift_size.x, lift_size.y, lift_size.z))),
            material: MeshMaterial3d(mats.aqua.clone()),
            transform: Transform::from_translation(lift_start),
            ..default()
        },
        RigidBody::KinematicPositionBased,
        Collider::cuboid(lift_size.x * 0.5, lift_size.y * 0.5, lift_size.z * 0.5),
        WalkableSurface,
        MovingPlatform {
            start: lift_start,
            end: lift_end,
            speed: 2.6,
            phase: 0.0,
            size: lift_size,
        },
        WorldGeometry,
    ));

    // Cooperative spring jump into a collapse-bridge timing puzzle.
    commands.spawn((
        Name::new("Four Player Pulse Spring"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(3.2, 0.8))),
            material: MeshMaterial3d(mats.pink.clone()),
            transform: Transform::from_translation(Vec3::new(126.0, 4.0, 0.0)),
            ..default()
        },
        RigidBody::Fixed,
        Collider::cylinder(0.4, 3.2),
        SlingShotPad {
            launch_velocity: Vec3::new(0.72, 1.02, 0.0),
            radius: 4.0,
            cooldown: 0.35,
            cooldown_timer: 0.0,
        },
        WorldGeometry,
    ));
    for i in 0..4 {
        let origin = Vec3::new(134.0 + i as f32 * 6.2, 7.0, 0.0);
        let size = Vec3::new(5.5, 0.65, 10.0);
        commands.spawn((
            Name::new(format!("Starbound Collapse Panel {i}")),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(mats.gold.clone()),
                transform: Transform::from_translation(origin),
                ..default()
            },
            RigidBody::KinematicPositionBased,
            Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
            WalkableSurface,
            CollapsePlatform {
                origin,
                size,
                warning_seconds: 0.8,
                fallen_seconds: 2.0,
                drop_distance: 18.0,
                timer: 0.0,
                state: CollapsePlatformState::Armed,
            },
            WorldGeometry,
        ));
    }

    // Two-player switch puzzle; solo play accepts either plate.
    for (slot, z) in [(0, -5.0), (1, 5.0)] {
        commands.spawn((
            Name::new(format!("Prism Sync Plate {slot}")),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(2.0, 0.35))),
                material: MeshMaterial3d(if slot == 0 {
                    mats.aqua.clone()
                } else {
                    mats.pink.clone()
                }),
                transform: Transform::from_translation(Vec3::new(166.0, 7.7, z)),
                ..default()
            },
            PlatformerPressurePlate { puzzle: 0, slot },
            WorldGeometry,
        ));
    }
    let puzzle_closed = Vec3::new(174.0, 11.0, 0.0);
    commands.spawn((
        Name::new("Prism Sync Gate"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(1.2, 9.0, 24.0))),
            material: MeshMaterial3d(mats.hazard.clone()),
            transform: Transform::from_translation(puzzle_closed),
            ..default()
        },
        RigidBody::KinematicPositionBased,
        Collider::cuboid(0.6, 4.5, 12.0),
        PlatformerPuzzleGate {
            puzzle: 0,
            closed: puzzle_closed,
            open: puzzle_closed + Vec3::Y * 12.0,
        },
        WorldGeometry,
    ));

    // Compact battle pockets separated by traversal instead of long arenas.
    for (encounter, trigger_x, enemy_type, count, gate_x) in [
        (0, 104.0, EnemyType::Soldier, 3, 121.0),
        (1, 184.0, EnemyType::SpikeAlien, 4, 207.0),
    ] {
        commands.spawn((
            Name::new(format!("Platformer Mini Battle {encounter}")),
            Transform::from_xyz(trigger_x + 6.0, 5.0, 0.0),
            PlatformerEncounterSpawner {
                encounter,
                trigger_x,
                enemy_type,
                count,
                spawned: false,
            },
            WorldGeometry,
        ));
        let closed = Vec3::new(gate_x, 8.0, 0.0);
        commands.spawn((
            Name::new(format!("Platformer Battle Gate {encounter}")),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(1.0, 8.0, 22.0))),
                material: MeshMaterial3d(mats.hazard.clone()),
                transform: Transform::from_translation(closed + Vec3::Y * 11.0),
                ..default()
            },
            RigidBody::KinematicPositionBased,
            Collider::cuboid(0.5, 4.0, 11.0),
            PlatformerBattleGate {
                encounter,
                closed,
                open: closed + Vec3::Y * 11.0,
            },
            WorldGeometry,
        ));
    }

    for (order, x, y) in [
        (0, -4.0, 2.0),
        (1, 68.0, 7.0),
        (2, 151.0, 9.0),
        (3, 183.0, 6.0),
    ] {
        let respawn = Vec3::new(x, y, 0.0);
        commands.spawn((
            Name::new(format!("Starbound Checkpoint {order}")),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Torus::new(1.1, 1.5))),
                material: MeshMaterial3d(mats.aqua.clone()),
                transform: Transform::from_translation(respawn + Vec3::Y * 1.8)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ..default()
            },
            PlatformerCheckpoint { order, respawn },
            WorldGeometry,
        ));
    }

    for (i, position) in [
        Vec3::new(14.0, 2.2, 0.0),
        Vec3::new(45.0, 5.0, -4.0),
        Vec3::new(68.0, 20.0, 4.0),
        Vec3::new(92.0, 10.0, 0.0),
        Vec3::new(114.0, 5.0, 5.0),
        Vec3::new(144.0, 10.0, -3.0),
        Vec3::new(166.0, 9.0, 0.0),
        Vec3::new(189.0, 6.0, -6.0),
        Vec3::new(200.0, 6.0, 6.0),
        Vec3::new(216.0, 6.0, 0.0),
        Vec3::new(78.0, 8.0, -5.0),
        Vec3::new(156.0, 9.0, 5.0),
    ]
    .into_iter()
    .enumerate()
    {
        commands.spawn((
            Name::new(format!("Workshop Star Part {i}")),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.72))),
                material: MeshMaterial3d(mats.gold.clone()),
                transform: Transform::from_translation(position),
                ..default()
            },
            PlatformerPartPickup {
                value: 1,
                collected: false,
            },
            WorldGeometry,
        ));
    }

    commands.spawn((
        Name::new("Starbound Workshop Beacon"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(3.0, 7.0))),
            material: MeshMaterial3d(mats.gold.clone()),
            transform: Transform::from_xyz(218.0, 7.8, 0.0),
            ..default()
        },
        PlatformerFinishBeacon::default(),
        WorldGeometry,
    ));

    let bubble_mesh = meshes.add(Sphere::new(1.65));
    for owner in 0..4u8 {
        commands.spawn((
            Name::new(format!("P{} Catch-up Bubble", owner + 1)),
            PbrBundle {
                mesh: Mesh3d(bubble_mesh.clone()),
                material: MeshMaterial3d(mats.bubble.clone()),
                visibility: Visibility::Hidden,
                ..default()
            },
            CoopBubbleVisual(owner),
            WorldGeometry,
        ));
    }

    // Visible boundary rails communicate the playable depth while tall,
    // invisible collision walls make the contract strict at speed.
    for z in [-PLATFORMER_HALF_DEPTH, PLATFORMER_HALF_DEPTH] {
        spawn_solid(
            commands,
            meshes,
            Vec3::new(107.0, 3.0, z),
            Vec3::new(234.0, 6.0, 0.8),
            mats.aqua.clone(),
        );
    }
    for x in [PLATFORMER_MIN_X, PLATFORMER_MAX_X] {
        spawn_solid(
            commands,
            meshes,
            Vec3::new(x, 8.0, 0.0),
            Vec3::new(1.0, 18.0, PLATFORMER_HALF_DEPTH * 2.0),
            mats.pink.clone(),
        );
    }
}

fn toon_material(color: Color, glow: f32) -> StandardMaterial {
    let linear = color.to_linear();
    StandardMaterial {
        base_color: color,
        emissive: LinearRgba::new(
            linear.red * glow,
            linear.green * glow,
            linear.blue * glow,
            1.0,
        ),
        perceptual_roughness: 0.72,
        ..default()
    }
}

fn spawn_solid(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    center: Vec3,
    size: Vec3,
    material: Handle<StandardMaterial>,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(center),
            ..default()
        },
        RigidBody::Fixed,
        Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
        WalkableSurface,
        WorldGeometry,
    ));
}

pub fn platformer_party_boundary_and_bubble_system(
    time: Res<Time>,
    mode: Res<PlayExperience>,
    progress: Res<PlatformerWorkshopProgress>,
    mut player_q: Query<
        (
            &PlayerIndex,
            &PlayerInput,
            &mut Transform,
            &mut PlayerMovement,
            &mut KinematicCharacterController,
            &mut CoopBubbleState,
        ),
        With<Player>,
    >,
    mut bubble_q: Query<(&CoopBubbleVisual, &mut Transform, &mut Visibility), Without<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if *mode != PlayExperience::SharedPlatformer {
        return;
    }
    let dt = time.delta_secs();
    let Some(leader) = player_q
        .iter_mut()
        .find(|(index, _, _, _, _, _)| index.0 == 0)
        .map(|(_, _, transform, _, _, _)| transform.translation)
    else {
        return;
    };

    for (index, input, mut transform, mut movement, mut controller, mut bubble) in
        player_q.iter_mut()
    {
        let fell = transform.translation.y < -12.0;
        if index.0 == 0 {
            transform.translation.z = transform.translation.z.clamp(-11.5, 11.5);
            transform.translation.x = transform
                .translation
                .x
                .clamp(PLATFORMER_MIN_X + 1.2, PLATFORMER_MAX_X - 1.2);
            if fell {
                transform.translation = progress.checkpoint;
                movement.velocity = Vec3::ZERO;
                movement.ground_velocity = Vec3::ZERO;
                movement.clear_motor_delivery();
                controller.translation = Some(Vec3::ZERO);
                msg_ev.write(UiMessageEvent {
                    text: "P1 returned to the last star checkpoint.".into(),
                    duration: 2.0,
                });
            }
            continue;
        }

        let too_far = transform.translation.x < leader.x - 24.0
            || transform.translation.x > leader.x + 20.0
            || transform.translation.z.abs() > PLATFORMER_HALF_DEPTH
            || fell;
        if too_far && !bubble.active {
            bubble.active = true;
            bubble.release_timer = 1.15;
            msg_ev.write(UiMessageEvent {
                text: format!("P{} bubbled back to the leader!", index.0 + 1),
                duration: 1.5,
            });
        }
        if bubble.active {
            bubble.release_timer = (bubble.release_timer - dt).max(0.0);
            let slot = index.0 as f32 - 1.0;
            transform.translation = leader + Vec3::new(-2.0 + slot * 2.0, 4.0 + slot * 0.4, 0.0);
            movement.velocity = Vec3::ZERO;
            movement.ground_velocity = Vec3::ZERO;
            movement.clear_motor_delivery();
            controller.translation = Some(Vec3::ZERO);
            if bubble.release_timer <= 0.0 && (input.jump || input.interact || leader.y > -10.0) {
                bubble.active = false;
                transform.translation = leader + Vec3::new(-2.0 + slot * 1.6, 1.5, 0.0);
            }
        } else {
            transform.translation.z = transform.translation.z.clamp(-11.5, 11.5);
            transform.translation.x = transform
                .translation
                .x
                .clamp(PLATFORMER_MIN_X + 1.2, PLATFORMER_MAX_X - 1.2);
        }
    }

    for (visual, mut transform, mut visibility) in bubble_q.iter_mut() {
        let state = player_q
            .iter_mut()
            .find(|(index, _, _, _, _, _)| index.0 == visual.0)
            .map(|(_, _, player, _, _, bubble)| (player.translation, bubble.active));
        if let Some((position, active)) = state {
            transform.translation = position + Vec3::Y * 0.8;
            *visibility = if active {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

pub fn platformer_checkpoint_and_reward_system(
    mode: Res<PlayExperience>,
    mut progress: ResMut<PlatformerWorkshopProgress>,
    checkpoint_q: Query<&PlatformerCheckpoint>,
    mut pickup_q: Query<(&Transform, &mut PlatformerPartPickup)>,
    mut finish_q: Query<(&Transform, &mut PlatformerFinishBeacon)>,
    mut player_q: Query<(&PlayerIndex, &Transform, &mut PlayerStats), With<Player>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if *mode != PlayExperience::SharedPlatformer {
        return;
    }
    let leader_x = player_q
        .iter_mut()
        .find(|(index, _, _)| index.0 == 0)
        .map(|(_, transform, _)| transform.translation.x)
        .unwrap_or(PLATFORMER_MIN_X);
    if let Some(checkpoint) = checkpoint_q
        .iter()
        .filter(|checkpoint| checkpoint.respawn.x <= leader_x + 1.0)
        .max_by_key(|checkpoint| checkpoint.order)
    {
        progress.checkpoint = checkpoint.respawn;
        if checkpoint.order > progress.current_level {
            progress.current_level = checkpoint.order;
            if let Some(level) = SIMPLE_PLATFORMER_LEVELS.get(checkpoint.order as usize) {
                msg_ev.write(UiMessageEvent {
                    text: format!("{} — {}", level.label, level.lesson),
                    duration: 4.0,
                });
            }
        }
    }

    for (pickup_transform, mut pickup) in pickup_q.iter_mut() {
        if pickup.collected
            || !player_q.iter_mut().any(|(_, player, _)| {
                player.translation.distance(pickup_transform.translation) <= 2.2
            })
        {
            continue;
        }
        pickup.collected = true;
        progress.star_parts = progress.star_parts.saturating_add(pickup.value);
        for (_, _, mut stats) in player_q.iter_mut() {
            stats.credits = stats.credits.saturating_add(25 * pickup.value);
            stats.experience = stats.experience.saturating_add(12 * pickup.value);
        }
        msg_ev.write(UiMessageEvent {
            text: format!(
                "Workshop part {}/12 — {}",
                progress.star_parts.min(12),
                progress.unlock_summary()
            ),
            duration: 2.3,
        });
    }

    for (finish_transform, mut finish) in finish_q.iter_mut() {
        if finish.claimed
            || !player_q.iter_mut().any(|(_, player, _)| {
                player.translation.distance(finish_transform.translation) <= 4.0
            })
        {
            continue;
        }
        finish.claimed = true;
        progress.finished = true;
        for (_, _, mut stats) in player_q.iter_mut() {
            stats.credits = stats.credits.saturating_add(300);
            stats.experience = stats.experience.saturating_add(180);
        }
        msg_ev.write(UiMessageEvent {
            text: format!(
                "STARTER SET COMPLETE! {}. Rewards banked for new level builds.",
                progress.unlock_summary()
            ),
            duration: 6.0,
        });
    }
}

pub fn platformer_pressure_gate_system(
    time: Res<Time>,
    mode: Res<PlayExperience>,
    player_q: Query<&Transform, With<Player>>,
    plate_q: Query<
        (&Transform, &PlatformerPressurePlate),
        (Without<Player>, Without<PlatformerPuzzleGate>),
    >,
    mut gate_q: Query<
        (&mut Transform, &PlatformerPuzzleGate),
        (Without<Player>, Without<PlatformerPressurePlate>),
    >,
) {
    if *mode != PlayExperience::SharedPlatformer {
        return;
    }
    let player_count = player_q.iter().count();
    for (mut gate_transform, gate) in gate_q.iter_mut() {
        let occupied = plate_q
            .iter()
            .filter(|(_, plate)| plate.puzzle == gate.puzzle)
            .filter(|(plate_transform, _)| {
                player_q.iter().any(|player| {
                    (player.translation - plate_transform.translation)
                        .with_y(0.0)
                        .length()
                        <= 2.4
                        && (player.translation.y - plate_transform.translation.y).abs() <= 2.5
                })
            })
            .map(|(_, plate)| plate.slot)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let required = player_count.clamp(1, 2);
        let target = if occupied >= required {
            gate.open
        } else {
            gate.closed
        };
        gate_transform.translation = gate_transform
            .translation
            .lerp(target, 1.0 - (-time.delta_secs() * 5.0).exp());
    }
}

pub fn platformer_encounter_system(
    time: Res<Time>,
    mode: Res<PlayExperience>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut spawner_q: Query<
        (&Transform, &mut PlatformerEncounterSpawner),
        Without<PlatformerBattleGate>,
    >,
    enemy_q: Query<(&PlatformerEncounterEnemy, &Health), Without<DeadEnemy>>,
    player_q: Query<(&PlayerIndex, &Transform), With<Player>>,
    mut gate_q: Query<
        (&mut Transform, &PlatformerBattleGate),
        (Without<PlatformerEncounterSpawner>, Without<Player>),
    >,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if *mode != PlayExperience::SharedPlatformer {
        return;
    }
    let leader_x = player_q
        .iter()
        .find(|(index, _)| index.0 == 0)
        .map(|(_, transform)| transform.translation.x)
        .unwrap_or(PLATFORMER_MIN_X);
    for (transform, mut spawner) in spawner_q.iter_mut() {
        if spawner.spawned || leader_x < spawner.trigger_x {
            continue;
        }
        spawner.spawned = true;
        for i in 0..spawner.count {
            let offset = Vec3::new(
                i as f32 * 2.3 - spawner.count as f32,
                0.0,
                if i.is_multiple_of(2) { -3.0 } else { 3.0 },
            );
            let enemy = spawn_enemy_entity(
                &mut commands,
                &mut meshes,
                &mut materials,
                spawner.enemy_type,
                transform.translation + offset,
                0.9 + spawner.encounter as f32 * 0.12,
                Some(Faction::DimensionalAlien),
            );
            commands
                .entity(enemy)
                .insert(PlatformerEncounterEnemy(spawner.encounter));
        }
        msg_ev.write(UiMessageEvent {
            text: "MINI BATTLE — clear the pocket together!".into(),
            duration: 2.4,
        });
    }

    for (mut gate_transform, gate) in gate_q.iter_mut() {
        let triggered = spawner_q
            .iter_mut()
            .any(|(_, spawner)| spawner.encounter == gate.encounter && spawner.spawned);
        let live = enemy_q
            .iter()
            .any(|(enemy, health)| enemy.0 == gate.encounter && health.is_alive());
        let target = if triggered && live {
            gate.closed
        } else {
            gate.open
        };
        gate_transform.translation = gate_transform
            .translation
            .lerp(target, 1.0 - (-time.delta_secs() * 6.0).exp());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platformer_plugin_registers_its_owned_state() {
        let mut app = App::new();
        app.add_plugins(HeavyWaterPlatformerPlugin);
        assert!(app
            .world()
            .contains_resource::<PlatformerWorkshopProgress>());
    }

    #[test]
    fn platformer_spawns_are_inside_the_start_boundary() {
        for index in 0..4 {
            let spawn = platformer_spawn(index);
            assert!(spawn.x > PLATFORMER_MIN_X && spawn.x < PLATFORMER_MAX_X);
            assert!(spawn.z.abs() < PLATFORMER_HALF_DEPTH);
        }
    }

    #[test]
    fn beginner_levels_form_one_bounded_contiguous_course() {
        assert_eq!(SIMPLE_PLATFORMER_LEVELS.len(), 3);
        assert!(SIMPLE_PLATFORMER_LEVELS[0].start_x > PLATFORMER_MIN_X);
        assert!(SIMPLE_PLATFORMER_LEVELS[2].end_x < PLATFORMER_MAX_X);
        for pair in SIMPLE_PLATFORMER_LEVELS.windows(2) {
            assert_eq!(pair[0].end_x, pair[1].start_x);
        }
        for level in &SIMPLE_PLATFORMER_LEVELS {
            assert!(!level.id.is_empty());
            assert!(!level.label.is_empty());
            assert!(level.start_x < level.end_x);
            assert!(level.checkpoint.x >= level.start_x);
            assert!(level.checkpoint.x < level.end_x);
        }
    }

    #[test]
    fn workshop_unlocks_advance_in_four_clear_tiers() {
        let mut progress = PlatformerWorkshopProgress::default();
        assert!(progress.unlock_summary().contains("Collect 3"));
        progress.star_parts = 3;
        assert!(progress.unlock_summary().contains("Prefab kit ready"));
        progress.star_parts = 6;
        assert!(progress.unlock_summary().contains("Weapon modules ready"));
        progress.star_parts = 9;
        assert!(progress.unlock_summary().contains("Armor shells ready"));
        progress.star_parts = 12;
        assert!(progress.unlock_summary().contains("vehicle cores ready"));
    }
}
