//! Concrete top-down arcade dungeon prototype.
//!
//! Target fantasy: Gauntlet wave pressure + Mario 3D World co-op pads +
//! Ninja Turtles arcade readability on one shared screen.
//!
//! This module is the build-from example for future dungeons:
//! 1. exterior world gate (portal only)
//! 2. isolated stage geometry with a unique room graph
//! 3. hard party teleport on enter/exit
//! 4. `DungeonCrawlState.arcade_rules` strips open-world traversal toys
//!
//! Expand later by authoring more [`ArcadeDungeonDef`] rows and spawners
//! rather than cloning the mountain-cave corridor template.

use bevy::prelude::*;

use crate::components::enemy::EnemyType;
use crate::components::world::{
    DungeonCrawlGate, DungeonEncounterDoor, DungeonEncounterReward, DungeonEnemySpawner,
    DungeonExitPortal, DungeonGateDoor, DungeonRoomKind, DungeonRoomPortal, DungeonRoomZone,
    MovingPlatform, WalkableSurface, WorldAnchor, WorldGeometry,
};
use crate::engine::physics::prelude::{Collider, RigidBody};
use crate::engine::rendering::{PbrBundle, PointLightBundle, SpatialBundle};

/// Stable identity for the first hard-boundary arcade stage.
pub const ARCADE_PROTOTYPE_GATE_ID: &str = "arcade_prototype_turtle_yard";
pub const ARCADE_PROTOTYPE_LABEL: &str = "Turtle Yard Arcade";
pub const ARCADE_PROTOTYPE_CHAPTER: u8 = 1;
pub const ARCADE_PROTOTYPE_ANCHOR_ID: &str = "arcade_prototype_turtle_yard_anchor";

/// Floating stage kept near the starter city so the example is easy to reach
/// in playtests without hunting mountain caves.
pub const ARCADE_PROTOTYPE_STAGE_ORIGIN: Vec3 = Vec3::new(145.0, 48.0, -95.0);

/// Exterior gate pedestal in the starter district.
pub const ARCADE_PROTOTYPE_GATE_ORIGIN: Vec3 = Vec3::new(42.0, 1.2, 28.0);

/// Marks a dungeon gate that must hard-teleport the party into an isolated
/// arcade stage instead of only flipping the shared camera flag in place.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArcadeDungeonPortal {
    pub stage_spawns: [Vec3; 4],
    pub return_positions: [Vec3; 4],
}

impl ArcadeDungeonPortal {
    pub fn prototype() -> Self {
        let stage = ARCADE_PROTOTYPE_STAGE_ORIGIN;
        let gate = ARCADE_PROTOTYPE_GATE_ORIGIN;
        Self {
            stage_spawns: party_formation(stage + Vec3::new(0.0, 1.35, -8.5), 1.7, 1.15),
            return_positions: party_formation(gate + Vec3::new(0.0, 1.3, -6.5), 1.8, 1.2),
        }
    }
}

/// Root marker for the isolated stage hierarchy (useful for future unload).
#[derive(Component, Debug, Clone, Copy)]
pub struct ArcadeDungeonStageRoot {
    #[allow(dead_code)] // Stable unload key; the first stage stays resident for now.
    pub gate_id: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ArcadeRoomDef {
    pub room_index: u8,
    pub label: &'static str,
    pub kind: DungeonRoomKind,
    pub local_focus: Vec3,
    pub camera_radius: f32,
    pub floor_half: Vec2,
}

#[derive(Debug, Clone, Copy)]
pub struct ArcadeWaveDef {
    pub room_index: u8,
    pub local_position: Vec3,
    pub enemy_type: EnemyType,
    pub count: u8,
    pub difficulty: f32,
    pub trigger_radius: f32,
    pub encounter_owned: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ArcadeDungeonDef {
    pub gate_id: &'static str,
    pub label: &'static str,
    pub chapter: u8,
    pub anchor_id: &'static str,
    pub stage_origin: Vec3,
    pub gate_origin: Vec3,
    pub rooms: &'static [ArcadeRoomDef],
    pub portals: &'static [(u8, u8)],
    pub waves: &'static [ArcadeWaveDef],
}

pub const ARCADE_PROTOTYPE_ROOMS: &[ArcadeRoomDef] = &[
    ArcadeRoomDef {
        room_index: 0,
        label: "Ticket Plaza",
        kind: DungeonRoomKind::Entrance,
        local_focus: Vec3::new(0.0, 1.0, -6.0),
        camera_radius: 28.0,
        floor_half: Vec2::new(12.0, 10.0),
    },
    ArcadeRoomDef {
        room_index: 1,
        label: "Coin Run",
        kind: DungeonRoomKind::Traversal,
        local_focus: Vec3::new(0.0, 1.0, 18.0),
        camera_radius: 30.0,
        floor_half: Vec2::new(9.0, 14.0),
    },
    ArcadeRoomDef {
        room_index: 2,
        label: "Pizza Pit",
        kind: DungeonRoomKind::Combat,
        local_focus: Vec3::new(0.0, 1.0, 46.0),
        camera_radius: 34.0,
        floor_half: Vec2::new(14.0, 13.0),
    },
];

pub const ARCADE_PROTOTYPE_PORTALS: &[(u8, u8)] = &[(0, 1), (1, 2)];

pub const ARCADE_PROTOTYPE_WAVES: &[ArcadeWaveDef] = &[
    ArcadeWaveDef {
        room_index: 1,
        local_position: Vec3::new(0.0, 1.2, 16.0),
        enemy_type: EnemyType::Soldier,
        count: 3,
        difficulty: 0.9,
        trigger_radius: 11.0,
        encounter_owned: false,
    },
    ArcadeWaveDef {
        room_index: 2,
        local_position: Vec3::new(-4.0, 1.2, 44.0),
        enemy_type: EnemyType::SpikeAlien,
        count: 4,
        difficulty: 1.05,
        trigger_radius: 13.0,
        encounter_owned: true,
    },
    ArcadeWaveDef {
        room_index: 2,
        local_position: Vec3::new(4.5, 1.2, 50.0),
        enemy_type: EnemyType::Soldier,
        count: 3,
        difficulty: 1.1,
        trigger_radius: 12.0,
        encounter_owned: true,
    },
];

pub const ARCADE_PROTOTYPE_DEF: ArcadeDungeonDef = ArcadeDungeonDef {
    gate_id: ARCADE_PROTOTYPE_GATE_ID,
    label: ARCADE_PROTOTYPE_LABEL,
    chapter: ARCADE_PROTOTYPE_CHAPTER,
    anchor_id: ARCADE_PROTOTYPE_ANCHOR_ID,
    stage_origin: ARCADE_PROTOTYPE_STAGE_ORIGIN,
    gate_origin: ARCADE_PROTOTYPE_GATE_ORIGIN,
    rooms: ARCADE_PROTOTYPE_ROOMS,
    portals: ARCADE_PROTOTYPE_PORTALS,
    waves: ARCADE_PROTOTYPE_WAVES,
};

pub fn party_formation(center: Vec3, spacing_x: f32, spacing_z: f32) -> [Vec3; 4] {
    [
        center + Vec3::new(-spacing_x, 0.0, -spacing_z),
        center + Vec3::new(spacing_x, 0.0, -spacing_z),
        center + Vec3::new(-spacing_x, 0.0, spacing_z),
        center + Vec3::new(spacing_x, 0.0, spacing_z),
    ]
}

pub fn arcade_room_world_focus(def: &ArcadeDungeonDef, room_index: u8) -> Option<Vec3> {
    def.rooms
        .iter()
        .find(|room| room.room_index == room_index)
        .map(|room| def.stage_origin + room.local_focus)
}

pub fn arcade_camera_radius(def: &ArcadeDungeonDef, room_index: u8) -> f32 {
    def.rooms
        .iter()
        .find(|room| room.room_index == room_index)
        .map(|room| room.camera_radius)
        .unwrap_or(30.0)
}

/// Validates that the prototype graph is a single forward chain with unique
/// room indices — the contract future authored stages should keep.
pub fn validate_arcade_def(def: &ArcadeDungeonDef) -> Result<(), String> {
    if def.rooms.is_empty() {
        return Err("arcade dungeon needs at least one room".into());
    }
    let mut seen = [false; 16];
    for room in def.rooms {
        if room.room_index as usize >= seen.len() {
            return Err(format!(
                "room_index {} out of prototype range",
                room.room_index
            ));
        }
        if seen[room.room_index as usize] {
            return Err(format!("duplicate room_index {}", room.room_index));
        }
        seen[room.room_index as usize] = true;
        if room.camera_radius < 24.0 {
            return Err(format!(
                "room {} camera_radius too tight for 4-player readability",
                room.room_index
            ));
        }
    }
    for &(a, b) in def.portals {
        let has_a = def.rooms.iter().any(|room| room.room_index == a);
        let has_b = def.rooms.iter().any(|room| room.room_index == b);
        if !has_a || !has_b {
            return Err(format!("portal {a}-{b} references a missing room"));
        }
        if a == b {
            return Err("portal cannot connect a room to itself".into());
        }
    }
    for wave in def.waves {
        if !def
            .rooms
            .iter()
            .any(|room| room.room_index == wave.room_index)
        {
            return Err(format!("wave references missing room {}", wave.room_index));
        }
        if wave.count == 0 {
            return Err("wave count must be > 0".into());
        }
    }
    Ok(())
}

pub fn spawn_arcade_prototype_dungeon(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let def = &ARCADE_PROTOTYPE_DEF;
    validate_arcade_def(def).expect("arcade prototype def must be valid");

    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.18, 0.28),
        perceptual_roughness: 0.78,
        ..default()
    });
    let wall_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.22, 0.55),
        perceptual_roughness: 0.62,
        ..default()
    });
    let accent_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.92, 0.42),
        emissive: LinearRgba::new(0.08, 1.4, 0.35, 1.0),
        ..default()
    });
    let hazard_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.45, 0.12),
        emissive: LinearRgba::new(1.6, 0.45, 0.05, 1.0),
        ..default()
    });
    let gate_stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.34, 0.40),
        perceptual_roughness: 0.85,
        ..default()
    });

    spawn_exterior_gate(
        commands,
        meshes,
        gate_stone.clone(),
        accent_mat.clone(),
        def,
    );
    spawn_stage_shell(
        commands, meshes, floor_mat, wall_mat, accent_mat, hazard_mat, def,
    );
}

fn spawn_exterior_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    stone: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    def: &ArcadeDungeonDef,
) {
    let portal = ArcadeDungeonPortal::prototype();
    let entry = def.gate_origin + Vec3::new(0.0, 1.4, 0.0);
    let entrance_focus = arcade_room_world_focus(def, 0).unwrap_or(def.stage_origin);

    // Pedestal + arch so the prototype is visible in the starter district.
    commands.spawn((
        Name::new("Arcade Prototype Gate Pedestal"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(10.0, 1.0, 8.0))),
            material: MeshMaterial3d(stone.clone()),
            transform: Transform::from_translation(def.gate_origin + Vec3::Y * 0.5),
            ..default()
        },
        RigidBody::Fixed,
        Collider::cuboid(5.0, 0.5, 4.0),
        WalkableSurface,
        WorldGeometry,
    ));
    for side in [-1.0_f32, 1.0] {
        commands.spawn((
            Name::new("Arcade Prototype Gate Pillar"),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(1.4, 7.0, 1.4))),
                material: MeshMaterial3d(stone.clone()),
                transform: Transform::from_translation(
                    def.gate_origin + Vec3::new(side * 3.4, 3.5, 1.2),
                ),
                ..default()
            },
            RigidBody::Fixed,
            Collider::cuboid(0.7, 3.5, 0.7),
            WorldGeometry,
        ));
    }
    commands.spawn((
        Name::new("Arcade Prototype Gate Lintel"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(8.4, 1.2, 1.6))),
            material: MeshMaterial3d(accent.clone()),
            transform: Transform::from_translation(def.gate_origin + Vec3::new(0.0, 7.2, 1.2)),
            ..default()
        },
        WorldGeometry,
    ));

    commands.spawn((
        Name::new("Arcade Prototype Crawl Gate"),
        Transform::from_translation(entry),
        GlobalTransform::default(),
        DungeonCrawlGate {
            gate_id: def.gate_id,
            chapter: def.chapter,
            label: def.label,
            entry,
            focus: entrance_focus,
            radius: arcade_camera_radius(def, 0),
            interact_radius: 8.0,
            opened: false,
        },
        portal,
        WorldGeometry,
    ));

    for side in [-1.0_f32, 1.0] {
        let closed = def.gate_origin + Vec3::new(side * 1.7, 3.6, 0.35);
        let open = closed + Vec3::new(side * 3.8, 0.0, 0.0);
        commands.spawn((
            Name::new("Arcade Prototype Gate Door"),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(3.2, 6.4, 0.55))),
                material: MeshMaterial3d(stone.clone()),
                transform: Transform::from_translation(closed),
                ..default()
            },
            DungeonGateDoor {
                gate_id: def.gate_id,
                chapter: def.chapter,
                closed,
                open,
            },
            RigidBody::KinematicPositionBased,
            Collider::cuboid(1.6, 3.2, 0.28),
            WorldGeometry,
        ));
    }

    // Exit lives on the stage so leaving requires intentional interact after
    // the hard teleport, matching other shared-screen dungeons.
    let exit_pos = def.stage_origin + Vec3::new(0.0, 1.4, -12.5);
    commands.spawn((
        Name::new("Arcade Prototype Exit Portal"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cylinder::new(1.35, 0.35))),
            material: MeshMaterial3d(accent),
            transform: Transform::from_translation(exit_pos),
            ..default()
        },
        DungeonExitPortal {
            gate_id: def.gate_id,
            position: exit_pos,
            return_positions: portal.return_positions,
            interact_radius: 5.5,
        },
        WorldGeometry,
    ));
}

fn spawn_stage_shell(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    floor_mat: Handle<StandardMaterial>,
    wall_mat: Handle<StandardMaterial>,
    accent_mat: Handle<StandardMaterial>,
    hazard_mat: Handle<StandardMaterial>,
    def: &ArcadeDungeonDef,
) {
    let root = commands
        .spawn((
            Name::new("Arcade Prototype Stage Root"),
            SpatialBundle::from_transform(Transform::from_translation(def.stage_origin)),
            ArcadeDungeonStageRoot {
                gate_id: def.gate_id,
            },
            WorldGeometry,
        ))
        .id();

    // Under-slab so the floating stage reads as a real isolated arcade set.
    commands.entity(root).with_children(|parent| {
        parent.spawn((
            Name::new("Arcade Stage Underplate"),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(42.0, 2.0, 78.0))),
                material: MeshMaterial3d(wall_mat.clone()),
                transform: Transform::from_translation(Vec3::new(0.0, -1.4, 18.0)),
                ..default()
            },
            RigidBody::Fixed,
            Collider::cuboid(21.0, 1.0, 39.0),
            WorldGeometry,
        ));
    });

    for room in def.rooms {
        spawn_arcade_room(
            commands,
            meshes,
            floor_mat.clone(),
            wall_mat.clone(),
            accent_mat.clone(),
            def,
            room,
            root,
        );
    }

    for &(a, b) in def.portals {
        let Some(from) = def.rooms.iter().find(|room| room.room_index == a) else {
            continue;
        };
        let Some(to) = def.rooms.iter().find(|room| room.room_index == b) else {
            continue;
        };
        let mid = def.stage_origin + from.local_focus.lerp(to.local_focus, 0.5);
        commands.spawn((
            Name::new(format!("Arcade Portal {}-{}", a, b)),
            Transform::from_translation(mid),
            GlobalTransform::default(),
            DungeonRoomPortal {
                gate_id: def.gate_id,
                room_a: a,
                room_b: b,
                position: mid,
            },
            WorldGeometry,
        ));
    }

    // Mario-style co-op pads in the traversal room.
    for (i, local) in [
        Vec3::new(-3.2, 0.55, 12.0),
        Vec3::new(3.2, 1.15, 17.5),
        Vec3::new(-2.4, 1.75, 23.0),
        Vec3::new(2.6, 2.35, 27.5),
    ]
    .into_iter()
    .enumerate()
    {
        let world = def.stage_origin + local;
        commands.spawn((
            Name::new(format!("Arcade Jump Pad {i}")),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(3.4, 0.5, 3.4))),
                material: MeshMaterial3d(accent_mat.clone()),
                transform: Transform::from_translation(world),
                ..default()
            },
            RigidBody::Fixed,
            Collider::cuboid(1.7, 0.25, 1.7),
            WalkableSurface,
            WorldGeometry,
        ));
    }

    // Shared moving bridge between traversal and combat.
    let bridge_base = def.stage_origin + Vec3::new(0.0, 1.6, 34.0);
    let bridge_size = Vec3::new(5.5, 0.55, 4.2);
    commands.spawn((
        Name::new("Arcade Moving Bridge"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(bridge_size.x, bridge_size.y, bridge_size.z))),
            material: MeshMaterial3d(hazard_mat.clone()),
            transform: Transform::from_translation(bridge_base),
            ..default()
        },
        RigidBody::KinematicPositionBased,
        Collider::cuboid(
            bridge_size.x * 0.5,
            bridge_size.y * 0.5,
            bridge_size.z * 0.5,
        ),
        WalkableSurface,
        MovingPlatform {
            start: bridge_base + Vec3::new(-4.5, 0.0, 0.0),
            end: bridge_base + Vec3::new(4.5, 0.0, 0.0),
            speed: 2.4,
            phase: 0.35,
            size: bridge_size,
        },
        WorldGeometry,
    ));

    // Combat seal between rooms 1 and 2.
    let seal_closed = def.stage_origin + Vec3::new(0.0, 2.8, 35.5);
    let seal_open = seal_closed + Vec3::Y * 8.5;
    commands.spawn((
        Name::new("Arcade Pizza Pit Seal"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(18.0, 5.5, 0.7))),
            material: MeshMaterial3d(hazard_mat),
            transform: Transform::from_translation(seal_open),
            ..default()
        },
        DungeonEncounterDoor {
            gate_id: def.gate_id,
            room_index: 2,
            closed: seal_closed,
            open: seal_open,
        },
        RigidBody::KinematicPositionBased,
        Collider::cuboid(9.0, 2.75, 0.35),
        WorldGeometry,
    ));

    commands.spawn((
        Name::new("Arcade Chamber Reward"),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(1.1))),
            material: MeshMaterial3d(accent_mat.clone()),
            transform: Transform::from_translation(def.stage_origin + Vec3::new(0.0, 2.4, 54.0)),
            visibility: Visibility::Hidden,
            ..default()
        },
        DungeonEncounterReward {
            gate_id: def.gate_id,
            room_index: 2,
        },
        WorldGeometry,
    ));

    for wave in def.waves {
        commands.spawn((
            Name::new(format!(
                "Arcade Wave R{} {:?}",
                wave.room_index, wave.enemy_type
            )),
            Transform::from_translation(def.stage_origin + wave.local_position),
            GlobalTransform::default(),
            DungeonEnemySpawner {
                chapter: def.chapter,
                encounter: wave
                    .encounter_owned
                    .then_some((def.gate_id, wave.room_index)),
                enemy_type: wave.enemy_type,
                count: wave.count,
                trigger_radius: wave.trigger_radius,
                difficulty: wave.difficulty,
                spawned: false,
            },
            WorldGeometry,
        ));
    }

    commands.spawn((
        Name::new("Arcade Stage Key Light"),
        PointLightBundle {
            point_light: PointLight {
                color: Color::srgb(0.55, 1.0, 0.72),
                intensity: 28_000.0,
                range: 55.0,
                shadow_maps_enabled: false,
                ..default()
            },
            transform: Transform::from_translation(def.stage_origin + Vec3::new(0.0, 14.0, 24.0)),
            ..default()
        },
        WorldGeometry,
    ));

    commands.spawn((
        Name::new("Arcade Stage Anchor"),
        Transform::from_translation(def.stage_origin + Vec3::new(0.0, 2.0, 46.0)),
        GlobalTransform::default(),
        WorldAnchor { id: def.anchor_id },
        WorldGeometry,
    ));
}

fn spawn_arcade_room(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    floor_mat: Handle<StandardMaterial>,
    wall_mat: Handle<StandardMaterial>,
    accent_mat: Handle<StandardMaterial>,
    def: &ArcadeDungeonDef,
    room: &ArcadeRoomDef,
    _root: Entity,
) {
    let center = def.stage_origin + Vec3::new(room.local_focus.x, 0.0, room.local_focus.z);
    let floor_size = Vec3::new(room.floor_half.x * 2.0, 0.7, room.floor_half.y * 2.0);

    commands.spawn((
        Name::new(format!("Arcade Room Floor {}", room.room_index)),
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(floor_size.x, floor_size.y, floor_size.z))),
            material: MeshMaterial3d(floor_mat),
            transform: Transform::from_translation(center),
            ..default()
        },
        RigidBody::Fixed,
        Collider::cuboid(floor_size.x * 0.5, floor_size.y * 0.5, floor_size.z * 0.5),
        WalkableSurface,
        WorldGeometry,
    ));

    // Open north/south corridors between chained rooms; seal the outer sides.
    let wall_h = 4.8;
    let thickness = 1.1;
    let half_x = room.floor_half.x + thickness * 0.5;
    let half_z = room.floor_half.y + thickness * 0.5;
    for (local, size) in [
        (
            Vec3::new(-half_x, wall_h * 0.5, 0.0),
            Vec3::new(thickness, wall_h, floor_size.z),
        ),
        (
            Vec3::new(half_x, wall_h * 0.5, 0.0),
            Vec3::new(thickness, wall_h, floor_size.z),
        ),
    ] {
        commands.spawn((
            Name::new(format!("Arcade Room Wall {}", room.room_index)),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(wall_mat.clone()),
                transform: Transform::from_translation(center + local),
                ..default()
            },
            RigidBody::Fixed,
            Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
            WorldGeometry,
        ));
    }

    // End caps only on the true entrance back wall and final arena far wall.
    if room.room_index == 0 || room.room_index == 2 {
        let z_sign = if room.room_index == 0 { -1.0 } else { 1.0 };
        let local = Vec3::new(0.0, wall_h * 0.5, z_sign * half_z);
        let size = Vec3::new(floor_size.x, wall_h, thickness);
        commands.spawn((
            Name::new(format!("Arcade Room Cap {}", room.room_index)),
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                material: MeshMaterial3d(wall_mat),
                transform: Transform::from_translation(center + local),
                ..default()
            },
            RigidBody::Fixed,
            Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
            WorldGeometry,
        ));
    }

    // Corner studs for TMNT/Gauntlet arcade readability under a top-down camera.
    for side_x in [-1.0_f32, 1.0] {
        for side_z in [-1.0_f32, 1.0] {
            let stud = center
                + Vec3::new(
                    side_x * (room.floor_half.x - 1.4),
                    1.2,
                    side_z * (room.floor_half.y - 1.4),
                );
            commands.spawn((
                Name::new("Arcade Corner Stud"),
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cylinder::new(0.55, 2.2))),
                    material: MeshMaterial3d(accent_mat.clone()),
                    transform: Transform::from_translation(stud),
                    ..default()
                },
                WorldGeometry,
            ));
        }
    }

    commands.spawn((
        Name::new(format!("Arcade Room Zone {}", room.label)),
        Transform::from_translation(def.stage_origin + room.local_focus),
        GlobalTransform::default(),
        DungeonRoomZone {
            gate_id: def.gate_id,
            room_index: room.room_index,
            label: room.label,
            kind: room.kind,
            focus: def.stage_origin + room.local_focus,
            camera_radius: room.camera_radius,
        },
        WorldGeometry,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcade_prototype_def_is_valid_and_chained() {
        validate_arcade_def(&ARCADE_PROTOTYPE_DEF).unwrap();
        assert_eq!(ARCADE_PROTOTYPE_DEF.rooms.len(), 3);
        assert_eq!(ARCADE_PROTOTYPE_DEF.portals, &[(0, 1), (1, 2)]);
        assert!(ARCADE_PROTOTYPE_DEF
            .waves
            .iter()
            .any(|wave| wave.encounter_owned && wave.room_index == 2));
    }

    #[test]
    fn arcade_party_formation_keeps_players_apart() {
        let slots = party_formation(Vec3::ZERO, 1.7, 1.15);
        for a in 0..slots.len() {
            for b in (a + 1)..slots.len() {
                assert!(slots[a].distance(slots[b]) >= 2.0);
            }
        }
    }

    #[test]
    fn arcade_portal_stage_and_return_are_separated() {
        let portal = ArcadeDungeonPortal::prototype();
        let stage_center = portal.stage_spawns.iter().copied().sum::<Vec3>() / 4.0;
        let return_center = portal.return_positions.iter().copied().sum::<Vec3>() / 4.0;
        assert!(stage_center.distance(return_center) > 40.0);
        assert!(stage_center.y > return_center.y + 10.0);
    }
}
