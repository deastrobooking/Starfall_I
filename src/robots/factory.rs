#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use super::creature::{
    CreatureFaction, CreatureKind, CreatureRole, CreatureSpec, CreatureSurface, CreatureTopology,
    CreatureValidation,
};
use super::designer::{HeadShape, LegStyle, RobotStyle, VisorStyle};
use bevy::prelude::*;

use crate::rendering::{PbrBundle, SpatialBundle};

/// Procedurally generate a robot mesh hierarchy at the given world position.
/// Returns the root `Entity`. All sub-parts are children of the root.
///
/// Parts use smooth Capsule3d/Sphere/Cylinder primitives so every robot reads
/// as a rounded, mechanical silhouette rather than a hard-edged box stack.
pub fn spawn_robot(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    style: &RobotStyle,
    position: Vec3,
) -> Entity {
    spawn_robot_with_material_response(commands, meshes, materials, style, position, (0.55, 0.45))
}

/// Metadata attached to the root produced from a `CreatureSpec`.
#[derive(Component, Debug, Clone)]
pub struct GeneratedCreature {
    pub content_id: String,
    pub schema_version: u32,
    pub seed: u64,
    pub kind: CreatureKind,
    pub topology: CreatureTopology,
    pub role: CreatureRole,
    pub faction: CreatureFaction,
    pub surface: CreatureSurface,
}

/// Validate and spawn a versioned creature recipe through the proven robot
/// geometry factory. Existing `spawn_robot` callers remain unchanged.
pub fn spawn_creature(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    spec: &CreatureSpec,
    position: Vec3,
) -> Result<Entity, CreatureValidation> {
    let validation = spec.validate();
    if !validation.is_publishable() {
        return Err(validation);
    }
    let style = spec.compiled_style();
    let root = spawn_robot_with_material_response(
        commands,
        meshes,
        materials,
        &style,
        position,
        spec.surface.material_response(),
    );
    commands.entity(root).insert(GeneratedCreature {
        content_id: spec.content_id.clone(),
        schema_version: spec.schema_version,
        seed: spec.seed,
        kind: spec.kind,
        topology: spec.topology,
        role: spec.role,
        faction: spec.faction,
        surface: spec.surface,
    });
    Ok(root)
}

fn spawn_robot_with_material_response(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    style: &RobotStyle,
    position: Vec3,
    material_response: (f32, f32),
) -> Entity {
    let s = style.scale / 100.0; // unit normaliser
    let (metallic, roughness) = material_response;

    let primary_mat = materials.add(StandardMaterial {
        base_color: style.primary,
        metallic,
        perceptual_roughness: roughness,
        ..default()
    });
    let secondary_mat = materials.add(StandardMaterial {
        base_color: style.secondary,
        metallic: (metallic * 0.72).clamp(0.0, 1.0),
        perceptual_roughness: (roughness + 0.10).clamp(0.0, 1.0),
        ..default()
    });
    let emissive_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.02, 0.02, 0.02),
        emissive: style.emissive.into(),
        ..default()
    });

    // Root entity
    let root = commands
        .spawn((SpatialBundle::from_transform(Transform::from_translation(
            position,
        )),))
        .id();

    let tw = style.torso_width * s;
    let th = style.torso_height * s;
    let td = style.torso_depth * s;

    // ── Torso: Capsule3d main body ─────────────────────────────────────────────
    let torso = commands
        .spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Capsule3d::new(tw * 0.46, th * 0.55))),
            material: MeshMaterial3d(primary_mat.clone()),
            transform: Transform::from_xyz(0.0, th * 0.5, 0.0).with_scale(Vec3::new(
                1.0,
                1.0,
                td / (tw * 0.92),
            )),
            ..default()
        })
        .id();
    commands.entity(root).add_child(torso);

    // ── Chest plate: protruding ellipsoid ─────────────────────────────────────
    let cp = commands
        .spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(0.5))),
            material: MeshMaterial3d(secondary_mat.clone()),
            transform: Transform::from_xyz(0.0, th * 0.60, td * 0.52).with_scale(Vec3::new(
                tw * 0.68,
                th * 0.42,
                td * 0.22,
            )),
            ..default()
        })
        .id();
    commands.entity(root).add_child(cp);

    // Core glow reactor
    let core = commands
        .spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(tw * 0.10))),
            material: MeshMaterial3d(emissive_mat.clone()),
            transform: Transform::from_xyz(0.0, th * 0.55, td * 0.55),
            ..default()
        })
        .id();
    commands.entity(root).add_child(core);

    // ── Head ──────────────────────────────────────────────────────────────────
    let hs = style.head_size * s;
    let (head_mesh, head_scale): (Mesh, Vec3) = match style.head_shape {
        HeadShape::Sphere => (Sphere::new(hs * 0.55).into(), Vec3::ONE),
        HeadShape::Cylinder => (Cylinder::new(hs * 0.48, hs).into(), Vec3::ONE),
        HeadShape::Cone => (
            Cone {
                radius: hs * 0.48,
                height: hs * 1.25,
            }
            .into(),
            Vec3::ONE,
        ),
        // Round "Box" head into a squashed sphere
        HeadShape::Box => (Sphere::new(hs * 0.52).into(), Vec3::new(1.0, 0.88, 0.90)),
    };
    let head = commands
        .spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(head_mesh)),
            material: MeshMaterial3d(primary_mat.clone()),
            transform: Transform::from_xyz(0.0, th + hs * 0.5, 0.0).with_scale(head_scale),
            ..default()
        })
        .id();
    commands.entity(root).add_child(head);

    // ── Visor ─────────────────────────────────────────────────────────────────
    if style.has_visor {
        let (visor_mesh, visor_tf): (Mesh, Transform) = match style.visor_style {
            VisorStyle::Slit => (
                Cylinder::new(hs * 0.38, hs * 0.12).into(),
                Transform::from_xyz(0.0, th + hs * 0.5, hs * 0.52)
                    .with_scale(Vec3::new(1.55, 1.0, 1.0))
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ),
            VisorStyle::Round => (
                Sphere::new(hs * 0.30).into(),
                Transform::from_xyz(0.0, th + hs * 0.5, hs * 0.52),
            ),
            VisorStyle::Full => (
                Sphere::new(0.5).into(),
                Transform::from_xyz(0.0, th + hs * 0.5, hs * 0.52).with_scale(Vec3::new(
                    hs * 0.72,
                    hs * 0.48,
                    hs * 0.18,
                )),
            ),
        };
        let visor = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(visor_mesh)),
                material: MeshMaterial3d(emissive_mat.clone()),
                transform: visor_tf,
                ..default()
            })
            .id();
        commands.entity(root).add_child(visor);
    }

    // ── Arms: Capsule3d segments + Sphere joints ───────────────────────────────
    let al = style.arm_length * s;
    let at = style.arm_thickness * s;
    for side in [-1.0_f32, 1.0] {
        let arm_x = side * (tw * 0.5 + at * 0.5);

        // Shoulder ball joint
        let sball = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(at * 0.65))),
                material: MeshMaterial3d(secondary_mat.clone()),
                transform: Transform::from_xyz(arm_x, th * 0.90, 0.0),
                ..default()
            })
            .id();
        commands.entity(root).add_child(sball);

        // Upper arm capsule
        let arm = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Capsule3d::new(at * 0.46, al * 0.44))),
                material: MeshMaterial3d(primary_mat.clone()),
                transform: Transform::from_xyz(arm_x, th * 0.65, 0.0),
                ..default()
            })
            .id();
        commands.entity(root).add_child(arm);

        // Elbow joint sphere
        let elbow = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(at * 0.52))),
                material: MeshMaterial3d(secondary_mat.clone()),
                transform: Transform::from_xyz(arm_x, th * 0.65 - al * 0.24, 0.0),
                ..default()
            })
            .id();
        commands.entity(root).add_child(elbow);

        // Forearm capsule
        let forearm = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Capsule3d::new(at * 0.40, al * 0.44))),
                material: MeshMaterial3d(primary_mat.clone()),
                transform: Transform::from_xyz(arm_x, th * 0.65 - al * 0.48, 0.0),
                ..default()
            })
            .id();
        commands.entity(root).add_child(forearm);

        // Shoulder pad: dome sphere
        let sp = style.shoulder_pad_size * s;
        let spad = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(sp * 0.62))),
                material: MeshMaterial3d(secondary_mat.clone()),
                transform: Transform::from_xyz(arm_x, th * 0.92, 0.0)
                    .with_scale(Vec3::new(1.0, 0.60, 0.95)),
                ..default()
            })
            .id();
        commands.entity(root).add_child(spad);

        // Cannon (forward-pointing cylinder)
        if style.has_cannons {
            let cs = style.cannon_size * s;
            let cannon = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Cylinder::new(cs * 0.30, cs * 2.0))),
                    material: MeshMaterial3d(emissive_mat.clone()),
                    transform: Transform::from_xyz(arm_x, th * 0.5, td * 0.6)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(cannon);
        }
    }

    // ── Legs ──────────────────────────────────────────────────────────────────
    let ll = style.leg_length * s;
    let lt = style.leg_thickness * s;
    for side in [-1.0_f32, 1.0] {
        match style.leg_style {
            LegStyle::Hoverpads => {
                // Floating disc
                let pad = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Cylinder::new(lt * 0.80, lt * 0.28))),
                        material: MeshMaterial3d(secondary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.42, 0.0),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(pad);
                // Strut: thin Capsule3d
                let strut = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Capsule3d::new(lt * 0.12, ll * 0.44))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.18, 0.0),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(strut);
            }
            LegStyle::Digitigrade => {
                // Thigh
                let thigh = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Capsule3d::new(lt * 0.46, ll * 0.42))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.20, 0.0)
                            .with_rotation(Quat::from_rotation_z(side * 0.15)),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(thigh);
                // Knee sphere
                let knee = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Sphere::new(lt * 0.52))),
                        material: MeshMaterial3d(secondary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.46, lt * 0.08),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(knee);
                // Shin angled forward
                let shin = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Capsule3d::new(lt * 0.38, ll * 0.42))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.68, lt * 0.24)
                            .with_rotation(Quat::from_rotation_x(-0.38)),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(shin);
            }
            LegStyle::Box => {
                // Thigh
                let thigh = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Capsule3d::new(lt * 0.46, ll * 0.42))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.20, 0.0),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(thigh);
                // Knee sphere
                let knee = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Sphere::new(lt * 0.52))),
                        material: MeshMaterial3d(secondary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.46, 0.0),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(knee);
                // Shin
                let shin = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Capsule3d::new(lt * 0.38, ll * 0.42))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.68, 0.0),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(shin);
                // Foot: horizontal Capsule3d
                let foot = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Capsule3d::new(lt * 0.40, lt * 1.30))),
                        material: MeshMaterial3d(secondary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.92, lt * 0.18)
                            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(foot);
            }
        }
    }

    // ── Wings: thin Capsule3d panels ──────────────────────────────────────────
    if style.has_wings {
        let ws = style.wing_span * s;
        let wa = style.wing_angle.to_radians();
        for side in [-1.0_f32, 1.0] {
            let wing = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Capsule3d::new(ws * 0.04, ws * 0.46))),
                    material: MeshMaterial3d(secondary_mat.clone()),
                    transform: Transform::from_xyz(side * (tw * 0.5 + ws * 0.25), th * 0.75, 0.0)
                        .with_rotation(
                            Quat::from_rotation_z(side * wa)
                                * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                        ),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(wing);
            // Wing-tip glow
            let tip = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Sphere::new(ws * 0.055))),
                    material: MeshMaterial3d(emissive_mat.clone()),
                    transform: Transform::from_xyz(
                        side * (tw * 0.5 + ws * 0.5),
                        th * 0.75 + wa.sin() * ws * 0.5 * side.signum(),
                        0.0,
                    ),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(tip);
        }
    }

    // ── Horns: Capsule3d pillars ───────────────────────────────────────────────
    if style.has_horns {
        let hl = style.horn_length * s;
        for side in [-1.0_f32, 1.0] {
            let horn = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Capsule3d::new(hl * 0.08, hl * 0.78))),
                    material: MeshMaterial3d(secondary_mat.clone()),
                    transform: Transform::from_xyz(side * hs * 0.36, th + hs + hl * 0.5, 0.0)
                        .with_rotation(Quat::from_rotation_z(side * 0.30))
                        .with_scale(Vec3::new(1.0, 1.0, 0.55)),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(horn);
        }
    }

    // ── Tail: Capsule3d segments tapering toward tip ───────────────────────────
    if style.has_tail {
        let tl = style.tail_length * s;
        let seg = style.tail_segments;
        let seg_len = tl / seg as f32;
        let mut tail_z = -td * 0.5;
        for i in 0..seg {
            let t = i as f32 / seg as f32;
            let radius = ((0.07 - t * 0.05) * tl).max(0.012 * tl);
            let segment = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Capsule3d::new(radius, seg_len * 0.82))),
                    material: MeshMaterial3d(secondary_mat.clone()),
                    transform: Transform::from_xyz(
                        0.0,
                        th * 0.3 - t * th * 0.4,
                        tail_z - seg_len * 0.5,
                    )
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(segment);
            tail_z -= seg_len;
        }
    }

    // ── Antennae: thin Capsule3d + Sphere glow tip ────────────────────────────
    if style.has_antennae {
        let ant_l = style.antenna_length * s;
        for side in [-1.0_f32, 1.0] {
            let ant = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Capsule3d::new(ant_l * 0.032, ant_l * 0.88))),
                    material: MeshMaterial3d(primary_mat.clone()),
                    transform: Transform::from_xyz(side * hs * 0.30, th + hs * 1.22, 0.0)
                        .with_rotation(Quat::from_rotation_z(side * 0.20)),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(ant);
            let tip = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Sphere::new(ant_l * 0.09))),
                    material: MeshMaterial3d(emissive_mat.clone()),
                    transform: Transform::from_xyz(
                        side * hs * 0.30 + side * ant_l * 0.20 * 0.20,
                        th + hs * 1.22 + ant_l * 0.50,
                        0.0,
                    ),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(tip);
        }
    }

    // ── Backpack: rounded Capsule3d ───────────────────────────────────────────
    if style.has_backpack {
        let bp = style.backpack_size * s;
        let pack = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Capsule3d::new(bp * 0.52, bp * 1.20))),
                material: MeshMaterial3d(secondary_mat.clone()),
                transform: Transform::from_xyz(0.0, th * 0.65, -td * 0.55 - bp * 0.4)
                    .with_scale(Vec3::new(1.15, 1.0, 0.72)),
                ..default()
            })
            .id();
        commands.entity(root).add_child(pack);
    }

    // ── Shield: ellipsoid disc ────────────────────────────────────────────────
    if style.has_shield {
        let ss = style.shield_size * s;
        let shield = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.5))),
                material: MeshMaterial3d(emissive_mat.clone()),
                transform: Transform::from_xyz(-tw * 0.5 - ss * 0.5, th * 0.5, 0.0)
                    .with_scale(Vec3::new(ss, ss * 1.18, ss * 0.10)),
                ..default()
            })
            .id();
        commands.entity(root).add_child(shield);
    }

    // ── Extra Plating: curved Sphere panels ───────────────────────────────────
    for i in 0..style.extra_plating {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let y_offset = th * (0.40 + i as f32 * 0.15);
        let plate = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.5))),
                material: MeshMaterial3d(secondary_mat.clone()),
                transform: Transform::from_xyz(side * tw * 0.28, y_offset, td * 0.52)
                    .with_scale(Vec3::new(tw * 0.30, th * 0.10, td * 0.22)),
                ..default()
            })
            .id();
        commands.entity(root).add_child(plate);
    }

    root
}
