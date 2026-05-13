use super::designer::{HeadShape, LegStyle, RobotStyle, VisorStyle};
use bevy::prelude::*;

/// Procedurally generate a robot mesh hierarchy at the given world position.
/// Returns the root `Entity`. All sub-parts are children of the root.
///
/// The robot is built out of PBR box / sphere / cylinder meshes combined into
/// a parent `TransformNode` (a `SpatialBundle`).  Parts are scaled by
/// `style.scale / 100.0` so that the raw sizes (given in designer-units) map
/// to reasonable Bevy world units.
pub fn spawn_robot(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    style: &RobotStyle,
    position: Vec3,
) -> Entity {
    let s = style.scale / 100.0; // unit normaliser

    let primary_mat = materials.add(StandardMaterial {
        base_color: style.primary,
        metallic: 0.4,
        perceptual_roughness: 0.6,
        ..default()
    });
    let secondary_mat = materials.add(StandardMaterial {
        base_color: style.secondary,
        metallic: 0.3,
        perceptual_roughness: 0.7,
        ..default()
    });
    let emissive_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.02, 0.02, 0.02),
        emissive: style.emissive.into(),
        ..default()
    });

    // Root entity (physics, position)
    let root = commands
        .spawn((SpatialBundle::from_transform(Transform::from_translation(
            position,
        )),))
        .id();

    let tw = style.torso_width * s;
    let th = style.torso_height * s;
    let td = style.torso_depth * s;

    // ── Torso ─────────────────────────────────────────────────────────────────
    let torso = commands
        .spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(tw, th, td))),
            material: MeshMaterial3d(primary_mat.clone()),
            transform: Transform::from_xyz(0.0, th * 0.5, 0.0),
            ..default()
        })
        .id();
    commands.entity(root).add_child(torso);

    // ── Chest plate ───────────────────────────────────────────────────────────
    let cp = commands
        .spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(tw * 0.7, th * 0.5, td * 0.2))),
            material: MeshMaterial3d(secondary_mat.clone()),
            transform: Transform::from_xyz(0.0, th * 0.6, td * 0.55),
            ..default()
        })
        .id();
    commands.entity(root).add_child(cp);

    // ── Head ──────────────────────────────────────────────────────────────────
    let hs = style.head_size * s;
    let head_mesh: Mesh = match style.head_shape {
        HeadShape::Sphere => Sphere::new(hs * 0.55).into(),
        HeadShape::Cylinder => Cylinder::new(hs * 0.5, hs).into(),
        HeadShape::Cone => Cone {
            radius: hs * 0.5,
            height: hs * 1.3,
        }
        .into(),
        HeadShape::Box => Cuboid::new(hs, hs, hs).into(),
    };
    let head = commands
        .spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(head_mesh)),
            material: MeshMaterial3d(primary_mat.clone()),
            transform: Transform::from_xyz(0.0, th + hs * 0.5, 0.0),
            ..default()
        })
        .id();
    commands.entity(root).add_child(head);

    // ── Visor ─────────────────────────────────────────────────────────────────
    if style.has_visor {
        let (vw, vh, vd) = match style.visor_style {
            VisorStyle::Slit => (hs * 0.7, hs * 0.15, hs * 0.15),
            VisorStyle::Round => (hs * 0.5, hs * 0.35, hs * 0.15),
            VisorStyle::Full => (hs * 0.75, hs * 0.5, hs * 0.15),
        };
        let visor = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(vw, vh, vd))),
                material: MeshMaterial3d(emissive_mat.clone()),
                transform: Transform::from_xyz(0.0, th + hs * 0.5, hs * 0.55),
                ..default()
            })
            .id();
        commands.entity(root).add_child(visor);
    }

    // ── Arms ──────────────────────────────────────────────────────────────────
    let al = style.arm_length * s;
    let at = style.arm_thickness * s;
    for side in [-1.0_f32, 1.0] {
        let arm = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(at, al, at))),
                material: MeshMaterial3d(primary_mat.clone()),
                transform: Transform::from_xyz(side * (tw * 0.5 + at * 0.5), th * 0.65, 0.0),
                ..default()
            })
            .id();
        commands.entity(root).add_child(arm);

        // Shoulder pad
        let sp = style.shoulder_pad_size * s;
        let spad = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(sp * 1.4, sp * 0.6, sp * 1.1))),
                material: MeshMaterial3d(secondary_mat.clone()),
                transform: Transform::from_xyz(side * (tw * 0.5 + at * 0.5), th * 0.9, 0.0),
                ..default()
            })
            .id();
        commands.entity(root).add_child(spad);

        // Cannon (if enabled)
        if style.has_cannons {
            let cs = style.cannon_size * s;
            let cannon = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Cylinder::new(cs * 0.3, cs * 2.0))),
                    material: MeshMaterial3d(emissive_mat.clone()),
                    transform: Transform::from_xyz(
                        side * (tw * 0.5 + at * 0.5),
                        th * 0.5,
                        td * 0.6,
                    )
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
                        mesh: Mesh3d(meshes.add(Cylinder::new(lt * 0.8, lt * 0.3))),
                        material: MeshMaterial3d(secondary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.4, 0.0),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(pad);
                // Strut
                let strut = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Cuboid::new(lt * 0.25, ll * 0.5, lt * 0.25))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.15, 0.0),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(strut);
            }
            LegStyle::Digitigrade => {
                // Upper leg (thigh)
                let thigh = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Cuboid::new(lt, ll * 0.45, lt))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.2, 0.0)
                            .with_rotation(Quat::from_rotation_z(side * 0.15)),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(thigh);
                // Shin (angled forward)
                let shin = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Cuboid::new(lt * 0.8, ll * 0.45, lt * 0.8))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.65, lt * 0.3)
                            .with_rotation(Quat::from_rotation_x(-0.4)),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(shin);
            }
            LegStyle::Box => {
                // Thigh
                let thigh = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Cuboid::new(lt, ll * 0.45, lt))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.2, 0.0),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(thigh);
                // Shin
                let shin = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Cuboid::new(lt * 0.8, ll * 0.45, lt * 0.8))),
                        material: MeshMaterial3d(primary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.65, 0.0),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(shin);
                // Foot
                let foot = commands
                    .spawn(PbrBundle {
                        mesh: Mesh3d(meshes.add(Cuboid::new(lt * 1.1, lt * 0.4, lt * 1.5))),
                        material: MeshMaterial3d(secondary_mat.clone()),
                        transform: Transform::from_xyz(side * tw * 0.25, -ll * 0.9, lt * 0.2),
                        ..default()
                    })
                    .id();
                commands.entity(root).add_child(foot);
            }
        }
    }

    // ── Wings ─────────────────────────────────────────────────────────────────
    if style.has_wings {
        let ws = style.wing_span * s;
        let wa = style.wing_angle.to_radians();
        for side in [-1.0_f32, 1.0] {
            let wing = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(ws * 0.5, ws * 0.06, ws * 0.18))),
                    material: MeshMaterial3d(secondary_mat.clone()),
                    transform: Transform::from_xyz(side * (tw * 0.5 + ws * 0.25), th * 0.75, 0.0)
                        .with_rotation(Quat::from_rotation_z(side * wa)),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(wing);
            // Wing tip (emissive)
            let tip = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(ws * 0.08, ws * 0.04, ws * 0.08))),
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

    // ── Horns ─────────────────────────────────────────────────────────────────
    if style.has_horns {
        let hl = style.horn_length * s;
        for side in [-1.0_f32, 1.0] {
            let horn = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(hl * 0.2, hl, hl * 0.2))),
                    material: MeshMaterial3d(secondary_mat.clone()),
                    transform: Transform::from_xyz(side * hs * 0.35, th + hs + hl * 0.5, 0.0)
                        .with_rotation(Quat::from_rotation_z(side * 0.3)),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(horn);
        }
    }

    // ── Tail ──────────────────────────────────────────────────────────────────
    if style.has_tail {
        let tl = style.tail_length * s;
        let seg = style.tail_segments;
        let seg_len = tl / seg as f32;
        let mut tail_z = -td * 0.5;
        for i in 0..seg {
            let t = i as f32 / seg as f32;
            let thickness = (0.08 - t * 0.05).max(0.02) * tl;
            let segment = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(thickness, thickness, seg_len))),
                    material: MeshMaterial3d(secondary_mat.clone()),
                    transform: Transform::from_xyz(
                        0.0,
                        th * 0.3 - t * th * 0.4,
                        tail_z - seg_len * 0.5,
                    ),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(segment);
            tail_z -= seg_len;
        }
    }

    // ── Antennae ──────────────────────────────────────────────────────────────
    if style.has_antennae {
        let al = style.antenna_length * s;
        for side in [-1.0_f32, 1.0] {
            let ant = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(al * 0.06, al, al * 0.06))),
                    material: MeshMaterial3d(primary_mat.clone()),
                    transform: Transform::from_xyz(side * hs * 0.3, th + hs * 1.2, 0.0)
                        .with_rotation(Quat::from_rotation_z(side * 0.2)),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(ant);
            // Tip glow
            let tip = commands
                .spawn(PbrBundle {
                    mesh: Mesh3d(meshes.add(Sphere::new(al * 0.08))),
                    material: MeshMaterial3d(emissive_mat.clone()),
                    transform: Transform::from_xyz(
                        side * hs * 0.3 + side * al * 0.2 * 0.2,
                        th + hs * 1.2 + al * 0.5,
                        0.0,
                    ),
                    ..default()
                })
                .id();
            commands.entity(root).add_child(tip);
        }
    }

    // ── Backpack ──────────────────────────────────────────────────────────────
    if style.has_backpack {
        let bp = style.backpack_size * s;
        let pack = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(bp * 1.2, bp * 1.5, bp * 0.8))),
                material: MeshMaterial3d(secondary_mat.clone()),
                transform: Transform::from_xyz(0.0, th * 0.65, -td * 0.55 - bp * 0.4),
                ..default()
            })
            .id();
        commands.entity(root).add_child(pack);
    }

    // ── Shield ────────────────────────────────────────────────────────────────
    if style.has_shield {
        let ss = style.shield_size * s;
        let shield = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(ss, ss * 1.2, ss * 0.08))),
                material: MeshMaterial3d(emissive_mat.clone()),
                transform: Transform::from_xyz(-tw * 0.5 - ss * 0.5, th * 0.5, 0.0),
                ..default()
            })
            .id();
        commands.entity(root).add_child(shield);
    }

    // ── Extra Plating ─────────────────────────────────────────────────────────
    for i in 0..style.extra_plating {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let y_offset = th * (0.4 + i as f32 * 0.15);
        let plate = commands
            .spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(tw * 0.3, th * 0.12, td * 0.25))),
                material: MeshMaterial3d(secondary_mat.clone()),
                transform: Transform::from_xyz(side * tw * 0.3, y_offset, td * 0.55),
                ..default()
            })
            .id();
        commands.entity(root).add_child(plate);
    }

    // ── Core Glow ─────────────────────────────────────────────────────────────
    let core = commands
        .spawn(PbrBundle {
            mesh: Mesh3d(meshes.add(Sphere::new(tw * 0.08))),
            material: MeshMaterial3d(emissive_mat.clone()),
            transform: Transform::from_xyz(0.0, th * 0.55, td * 0.5),
            ..default()
        })
        .id();
    commands.entity(root).add_child(core);

    root
}
