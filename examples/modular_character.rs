//! Runnable demo for the modular character assembly system.
//!
//! Run with:  `cargo run --example modular_character`
//!
//! Controls:
//!   1 / 2 / 3   swap both arms to slim / standard / heavy upper-arm prefabs
//!   B           toggle baked single-mesh preview (spawned beside the rig)
//!   G           toggle socket debug gizmos
//!
//! The core system lives in `src/modular_character.rs` and depends only on
//! bevy, so the example includes it directly rather than via a library target.

// The shared module exposes more API (glTF loading, mirroring helpers) than this
// demo exercises; silence dead-code noise like the main binary does.
#![allow(dead_code)]

#[path = "../src/modular_character.rs"]
mod modular_character;

use bevy::prelude::*;
use modular_character::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(ModularCharacterPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (controls, spin_camera))
        .run();
}

#[derive(Resource)]
struct DemoCharacter {
    root: Entity,
    baked: Option<Entity>,
}

#[derive(Component)]
struct DemoCamera;

/// Demo recipe shared by the hierarchy and baked paths.
fn demo_recipe() -> CharacterRecipe {
    CharacterRecipe {
        root_slot: "torso",
        root_part: "torso_medium",
        attachments: vec![
            Attachment {
                parent_slot: "torso",
                parent_socket: "neck",
                child_slot: "head",
                child_part: "head_round",
                child_socket: "neck_joint",
            },
            Attachment {
                parent_slot: "torso",
                parent_socket: "shoulder_l",
                child_slot: "arm_upper_l",
                child_part: "arm_upper",
                child_socket: "shoulder_joint",
            },
            Attachment {
                parent_slot: "torso",
                parent_socket: "shoulder_r",
                child_slot: "arm_upper_r",
                child_part: "arm_upper",
                child_socket: "shoulder_joint",
            },
            Attachment {
                parent_slot: "arm_upper_l",
                parent_socket: "elbow",
                child_slot: "hand_l",
                child_part: "hand",
                child_socket: "wrist",
            },
            Attachment {
                parent_slot: "arm_upper_r",
                parent_socket: "elbow",
                child_slot: "hand_r",
                child_part: "hand",
                child_socket: "wrist",
            },
            Attachment {
                parent_slot: "torso",
                parent_socket: "hip_l",
                child_slot: "leg_l",
                child_part: "leg",
                child_socket: "hip_joint",
            },
            Attachment {
                parent_slot: "torso",
                parent_socket: "hip_r",
                child_slot: "leg_r",
                child_part: "leg",
                child_socket: "hip_joint",
            },
        ],
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut registry: ResMut<PartRegistry>,
) {
    // Camera + light.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 1.2, 4.0).looking_at(Vec3::new(0.0, 0.8, 0.0), Vec3::Y),
        DemoCamera,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let skin = materials.add(StandardMaterial::from(Color::srgb(0.85, 0.72, 0.6)));
    let cloth = materials.add(StandardMaterial::from(Color::srgb(0.30, 0.45, 0.80)));
    let metal = materials.add(StandardMaterial {
        base_color: Color::srgb(0.6, 0.62, 0.7),
        metallic: 0.7,
        perceptual_roughness: 0.4,
        ..default()
    });

    // Torso: sockets at neck, both shoulders, both hips.
    registry.insert(PartPrefab {
        id: "torso_medium",
        mesh: meshes.add(Mesh::from(Capsule3d::new(0.25, 0.5))),
        material: cloth.clone(),
        sockets: vec![
            Socket::new("neck", Transform::from_xyz(0.0, 0.55, 0.0)),
            Socket::new(
                "shoulder_l",
                Transform::from_xyz(-0.3, 0.4, 0.0)
                    .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            ),
            Socket::new(
                "shoulder_r",
                Transform::from_xyz(0.3, 0.4, 0.0)
                    .with_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
            ),
            Socket::new("hip_l", Transform::from_xyz(-0.12, -0.55, 0.0)),
            Socket::new("hip_r", Transform::from_xyz(0.12, -0.55, 0.0)),
        ],
    });

    registry.insert(PartPrefab {
        id: "head_round",
        mesh: meshes.add(Mesh::from(Sphere::new(0.18))),
        material: skin.clone(),
        sockets: vec![Socket::new(
            "neck_joint",
            Transform::from_xyz(0.0, -0.18, 0.0),
        )],
    });

    // Three upper-arm variants for hot-swapping. Each is authored pointing down
    // local -Y with a shoulder socket at the top and an elbow socket at the
    // bottom, so the recipe never changes when we swap them.
    for (id, radius, mat) in [
        ("arm_upper", 0.07_f32, cloth.clone()),
        ("arm_slim", 0.05_f32, skin.clone()),
        ("arm_heavy", 0.11_f32, metal.clone()),
    ] {
        registry.insert(PartPrefab {
            id,
            mesh: meshes.add(Mesh::from(Capsule3d::new(radius, 0.3))),
            material: mat,
            sockets: vec![
                Socket::new("shoulder_joint", Transform::from_xyz(0.0, 0.2, 0.0)),
                Socket::new("elbow", Transform::from_xyz(0.0, -0.2, 0.0)),
            ],
        });
    }

    registry.insert(PartPrefab {
        id: "hand",
        mesh: meshes.add(Mesh::from(Sphere::new(0.08))),
        material: skin.clone(),
        sockets: vec![Socket::new("wrist", Transform::from_xyz(0.0, 0.08, 0.0))],
    });

    registry.insert(PartPrefab {
        id: "leg",
        mesh: meshes.add(Mesh::from(Capsule3d::new(0.09, 0.4))),
        material: metal.clone(),
        sockets: vec![Socket::new(
            "hip_joint",
            Transform::from_xyz(0.0, 0.28, 0.0),
        )],
    });

    let recipe = demo_recipe();
    match spawn_character(
        &mut commands,
        &registry,
        &recipe,
        Transform::from_xyz(-0.8, 1.0, 0.0),
    ) {
        Ok(root) => commands.insert_resource(DemoCharacter { root, baked: None }),
        Err(err) => error!("failed to assemble demo character: {err}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut swaps: MessageWriter<SwapPart>,
    mut debug_sockets: ResMut<DebugSockets>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mesh_assets: Res<Assets<Mesh>>,
    registry: Res<PartRegistry>,
    mut demo: ResMut<DemoCharacter>,
) {
    let arm = if keys.just_pressed(KeyCode::Digit1) {
        Some("arm_slim")
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some("arm_upper")
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some("arm_heavy")
    } else {
        None
    };
    if let Some(part) = arm {
        for slot in ["arm_upper_l", "arm_upper_r"] {
            swaps.write(SwapPart {
                root: demo.root,
                slot,
                new_part: part,
            });
        }
    }

    if keys.just_pressed(KeyCode::KeyG) {
        debug_sockets.0 = !debug_sockets.0;
        info!("socket gizmos: {}", debug_sockets.0);
    }

    if keys.just_pressed(KeyCode::KeyB) {
        if let Some(entity) = demo.baked.take() {
            commands.entity(entity).despawn();
        } else {
            match bake_recipe(&mesh_assets, &registry, &demo_recipe()) {
                Ok(mesh) => {
                    let entity = commands
                        .spawn((
                            Mesh3d(meshes.add(mesh)),
                            MeshMaterial3d(
                                materials.add(StandardMaterial::from(Color::srgb(0.8, 0.5, 0.3))),
                            ),
                            Transform::from_xyz(0.8, 1.0, 0.0),
                        ))
                        .id();
                    demo.baked = Some(entity);
                }
                Err(err) => error!("bake failed: {err}"),
            }
        }
    }
}

fn spin_camera(time: Res<Time>, mut cam: Query<&mut Transform, With<DemoCamera>>) {
    let angle = time.elapsed_secs() * 0.4;
    for mut tf in &mut cam {
        let r = 4.0;
        *tf = Transform::from_xyz(angle.sin() * r, 1.4, angle.cos() * r)
            .looking_at(Vec3::new(0.0, 0.8, 0.0), Vec3::Y);
    }
}
