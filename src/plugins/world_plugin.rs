use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use rand::Rng;

use crate::components::world::*;
use crate::lsystem::tree::{spawn_tree, TreeKind, TreeRoot, TreeTemplate};
use crate::resources::GameSettings;
use crate::state::AppState;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), generate_city)
            .add_systems(OnExit(AppState::Playing), cleanup_world);
    }
}

fn cleanup_world(
    mut commands: Commands,
    world_q: Query<Entity, With<WorldGeometry>>,
    tree_q: Query<Entity, With<TreeRoot>>,
) {
    for e in world_q.iter() {
        commands.entity(e).despawn_recursive();
    }
    for e in tree_q.iter() {
        commands.entity(e).despawn_recursive();
    }
}

// ── World generation entry point ──────────────────────────────────────────────
fn generate_city(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    settings: Res<GameSettings>,
) {
    let seed = settings.world_seed;
    let m = &mut *mats;

    let pal = Palette::build(m);

    spawn_lighting(&mut commands);
    spawn_terrain(&mut commands, &mut meshes, &pal, seed);
    spawn_downtown(&mut commands, &mut meshes, &pal, seed);
    spawn_industrial(&mut commands, &mut meshes, &pal, seed + 1);
    spawn_residential(&mut commands, &mut meshes, &pal, seed + 2);
    spawn_highways(&mut commands, &mut meshes, &pal);
    spawn_sky_platforms(&mut commands, &mut meshes, &pal, seed + 3);
    spawn_sky_bridges(&mut commands, &mut meshes, &pal, seed + 4);
    spawn_spaceports(&mut commands, &mut meshes, &pal);
    spawn_mountains(&mut commands, &mut meshes, &pal, seed + 5);
    spawn_neon_lights(&mut commands, seed + 6);
    spawn_street_lights(&mut commands, seed + 7);
    spawn_outer_districts(&mut commands, &mut meshes, &pal, seed + 8);
    spawn_river(&mut commands, &mut meshes, &pal);
    spawn_trees(&mut commands, &mut meshes, m, seed + 9);
}

// ── Seeded RNG helper ─────────────────────────────────────────────────────────
fn seeded(seed: u64, index: u64) -> f32 {
    let x = ((seed.wrapping_mul(127) + index.wrapping_mul(311)).wrapping_mul(43758)) as f64;
    let frac = (x * 0.0000001) - (x * 0.0000001).floor();
    frac as f32
}

// ── Shared material palette ───────────────────────────────────────────────────
struct Palette {
    ground: Handle<StandardMaterial>,

    downtown_a: Handle<StandardMaterial>,
    downtown_b: Handle<StandardMaterial>,
    downtown_c: Handle<StandardMaterial>,
    downtown_facade: Handle<StandardMaterial>,

    industrial_metal: Handle<StandardMaterial>,
    industrial_rust: Handle<StandardMaterial>,

    residential_a: Handle<StandardMaterial>,
    residential_b: Handle<StandardMaterial>,

    highway: Handle<StandardMaterial>,
    sky_platform: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,

    rock: Handle<StandardMaterial>,
    snow: Handle<StandardMaterial>,

    /// Warm glowing window panels — warm office light (yellows/amber).
    window_warm: Handle<StandardMaterial>,
    /// Cool glowing window panels — blue-white corporate light.
    window_cool: Handle<StandardMaterial>,
    /// Rooftop antenna / water-tank material.
    rooftop: Handle<StandardMaterial>,
}

impl Palette {
    fn build(m: &mut Assets<StandardMaterial>) -> Self {
        let mk =
            |base: Color, emissive: LinearRgba, metallic: f32, roughness: f32| StandardMaterial {
                base_color: base,
                emissive,
                metallic,
                perceptual_roughness: roughness,
                reflectance: metallic * 0.8 + 0.1,
                ..default()
            };

        Self {
            ground: m.add(StandardMaterial {
                base_color: Color::srgb(0.07, 0.07, 0.09),
                metallic: 0.55,
                perceptual_roughness: 0.50,
                reflectance: 0.4,
                ..default()
            }),

            downtown_a: m.add(mk(
                Color::srgb(0.06, 0.09, 0.16),
                LinearRgba::new(0.0, 0.30, 0.70, 1.0),
                0.90,
                0.12,
            )),
            downtown_b: m.add(mk(
                Color::srgb(0.10, 0.07, 0.05),
                LinearRgba::new(0.60, 0.20, 0.0, 1.0),
                0.85,
                0.15,
            )),
            downtown_c: m.add(mk(
                Color::srgb(0.06, 0.11, 0.07),
                LinearRgba::new(0.0, 0.40, 0.15, 1.0),
                0.88,
                0.13,
            )),
            downtown_facade: m.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.18, 0.22),
                metallic: 0.30,
                perceptual_roughness: 0.65,
                ..default()
            }),

            industrial_metal: m.add(mk(
                Color::srgb(0.22, 0.18, 0.12),
                LinearRgba::new(0.50, 0.15, 0.0, 1.0),
                0.60,
                0.55,
            )),
            industrial_rust: m.add(mk(
                Color::srgb(0.30, 0.14, 0.05),
                LinearRgba::new(0.20, 0.04, 0.0, 1.0),
                0.10,
                0.92,
            )),

            residential_a: m.add(mk(
                Color::srgb(0.22, 0.20, 0.16),
                LinearRgba::new(0.15, 0.12, 0.04, 1.0),
                0.05,
                0.85,
            )),
            residential_b: m.add(mk(
                Color::srgb(0.18, 0.17, 0.20),
                LinearRgba::new(0.05, 0.05, 0.18, 1.0),
                0.08,
                0.80,
            )),

            highway: m.add(StandardMaterial {
                base_color: Color::srgb(0.10, 0.10, 0.13),
                metallic: 0.20,
                perceptual_roughness: 0.75,
                ..default()
            }),
            sky_platform: m.add(mk(
                Color::srgb(0.08, 0.11, 0.22),
                LinearRgba::new(0.0, 0.25, 0.60, 1.0),
                0.65,
                0.30,
            )),

            water: m.add(StandardMaterial {
                base_color: Color::srgba(0.02, 0.18, 0.35, 0.82),
                emissive: LinearRgba::new(0.0, 0.10, 0.28, 1.0),
                metallic: 0.90,
                perceptual_roughness: 0.08,
                reflectance: 0.95,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),

            rock: m.add(StandardMaterial {
                base_color: Color::srgb(0.28, 0.25, 0.22),
                metallic: 0.05,
                perceptual_roughness: 0.95,
                ..default()
            }),
            snow: m.add(StandardMaterial {
                base_color: Color::srgb(0.88, 0.92, 1.00),
                metallic: 0.0,
                perceptual_roughness: 0.80,
                ..default()
            }),

            window_warm: m.add(StandardMaterial {
                base_color: Color::srgba(0.92, 0.82, 0.55, 0.82),
                emissive: LinearRgba::new(2.8, 2.0, 0.9, 1.0),
                metallic: 0.6,
                perceptual_roughness: 0.15,
                reflectance: 0.85,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            window_cool: m.add(StandardMaterial {
                base_color: Color::srgba(0.60, 0.78, 1.0, 0.78),
                emissive: LinearRgba::new(0.8, 1.4, 3.2, 1.0),
                metallic: 0.7,
                perceptual_roughness: 0.10,
                reflectance: 0.90,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            rooftop: m.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.16, 0.14),
                metallic: 0.72,
                perceptual_roughness: 0.42,
                reflectance: 0.55,
                ..default()
            }),
        }
    }
}

// ── Lighting ──────────────────────────────────────────────────────────────────
fn spawn_lighting(commands: &mut Commands) {
    // Ambient — fills dark faces so characters and geometry are always readable.
    // A cool deep blue matches the neon-city night setting.
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.28, 0.32, 0.48),
        brightness: 180.0,
    });

    // Main directional key-light — warm moonlight-ish coming from upper-right.
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: Color::srgb(0.88, 0.92, 1.0),
            illuminance: 7200.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.65,
            0.55,
            0.0,
        )),
        ..default()
    });

    // Soft cyan fill-light from below-left for the neon-city under-glow.
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            color: Color::srgb(0.0, 0.72, 1.0),
            intensity: 800_000.0,
            range: 550.0,
            shadows_enabled: false,
            ..default()
        },
        transform: Transform::from_xyz(-80.0, 220.0, 60.0),
        ..default()
    });

    // Warm orange counter-light from the opposite side.
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            color: Color::srgb(1.0, 0.52, 0.12),
            intensity: 350_000.0,
            range: 450.0,
            shadows_enabled: false,
            ..default()
        },
        transform: Transform::from_xyz(120.0, 160.0, -80.0),
        ..default()
    });
}

// ── Terrain ───────────────────────────────────────────────────────────────────

/// Smooth hermite interpolation.
fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Layered sine-wave height at world position (x, z).
/// The city centre is kept near-flat; elevation builds toward the outer ring.
fn terrain_height(x: f32, z: f32, seed: u64) -> f32 {
    // Phase offset so every world seed looks distinct.
    let so = seeded(seed, 7) * std::f32::consts::TAU;

    // Distance from city centre.
    let dist = (x * x + z * z).sqrt();

    // 1.0 in the city core, 0.0 beyond the outer threshold.
    let city_flat = 1.0 - smoothstep(120.0, 260.0, dist);

    // ── Layered octaves ─────────────────────────────────────────────────────
    // Large rolling hills (low frequency, high amplitude).
    let large = (x * 0.007 + so).sin() * (z * 0.008 + so * 0.7).cos() * 16.0;
    // Medium undulations.
    let med = (x * 0.022 + z * 0.019 + so * 0.3).sin() * 7.0;
    // Small surface variation.
    let small = (x * 0.055 - z * 0.048 + so * 0.5).sin() * 2.8;
    // Micro detail.
    let micro = (x * 0.13 + z * 0.11).sin() * 0.9;

    // ── Ridge lines ─────────────────────────────────────────────────────────
    // abs(sin) creates sharp ridges; scale by distance to keep them in the outer world.
    let ridge_mask = smoothstep(200.0, 400.0, dist);
    let ridge = (x * 0.011 + z * 0.009 + so).sin().abs() * 10.0 * ridge_mask;

    let base = large + med + small + micro + ridge;

    // Terrain rises significantly toward the world perimeter.
    let edge_lift = smoothstep(300.0, 520.0, dist) * 28.0;

    // Mountain peaks cluster in the four corners.
    let mut peak_boost = 0.0f32;
    for &(cx, cz) in &[
        (480.0f32, 480.0),
        (-480.0, 480.0),
        (480.0, -480.0),
        (-480.0, -480.0),
    ] {
        let d = ((x - cx) * (x - cx) + (z - cz) * (z - cz)).sqrt();
        peak_boost += smoothstep(220.0, 0.0, d) * 60.0;
    }

    // Flatten the city zone; outer areas get full amplitude.
    (base + edge_lift + peak_boost) * (1.0 - city_flat)
}

fn spawn_terrain(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    const RES: usize = 160; // quads per side — ~7.5 m per cell
    const WORLD: f32 = 1200.0;
    const CELL: f32 = WORLD / RES as f32;

    // ── Height grid ─────────────────────────────────────────────────────────
    let vcount = (RES + 1) * (RES + 1);
    let mut h = vec![0.0f32; vcount];
    for zi in 0..=RES {
        for xi in 0..=RES {
            let wx = (xi as f32 / RES as f32 - 0.5) * WORLD;
            let wz = (zi as f32 / RES as f32 - 0.5) * WORLD;
            h[zi * (RES + 1) + xi] = terrain_height(wx, wz, seed);
        }
    }

    // ── Mesh buffers ────────────────────────────────────────────────────────
    let mut positions = Vec::with_capacity(vcount);
    let mut normals = Vec::with_capacity(vcount);
    let mut uvs = Vec::with_capacity(vcount);

    for zi in 0..=RES {
        for xi in 0..=RES {
            let wx = (xi as f32 / RES as f32 - 0.5) * WORLD;
            let wz = (zi as f32 / RES as f32 - 0.5) * WORLD;
            let wy = h[zi * (RES + 1) + xi];
            positions.push([wx, wy, wz]);
            uvs.push([xi as f32 / RES as f32, zi as f32 / RES as f32]);

            // Finite-difference normal (upward).
            let hx0 = h[zi * (RES + 1) + xi.saturating_sub(1)];
            let hx1 = h[zi * (RES + 1) + (xi + 1).min(RES)];
            let hz0 = h[zi.saturating_sub(1) * (RES + 1) + xi];
            let hz1 = h[(zi + 1).min(RES) * (RES + 1) + xi];
            let dx = Vec3::new(2.0 * CELL, hx1 - hx0, 0.0);
            let dz = Vec3::new(0.0, hz1 - hz0, 2.0 * CELL);
            let n = dz.cross(dx).normalize();
            normals.push([n.x, n.y, n.z]);
        }
    }

    // Two triangles per quad, CCW winding, normals pointing up.
    let mut tri_idx = Vec::with_capacity(RES * RES * 6);
    for zi in 0..RES {
        for xi in 0..RES {
            let tl = (zi * (RES + 1) + xi) as u32;
            let tr = tl + 1;
            let bl = tl + (RES + 1) as u32;
            let br = bl + 1;
            tri_idx.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
        }
    }

    // Build collider data before the Vecs are consumed by the mesh.
    let col_verts: Vec<Vec3> = positions.iter().map(|p| Vec3::from(*p)).collect();
    let col_tris: Vec<[u32; 3]> = tri_idx
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(tri_idx));

    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(mesh)),
            material: MeshMaterial3d(pal.ground.clone()),
            transform: Transform::IDENTITY,
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        bevy_rapier3d::prelude::RigidBody::Fixed,
        bevy_rapier3d::prelude::Collider::trimesh(col_verts, col_tris),
    ));
}

// ── Downtown ──────────────────────────────────────────────────────────────────
fn spawn_downtown(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    let glass = [
        &pal.downtown_a,
        &pal.downtown_b,
        &pal.downtown_c,
        &pal.downtown_facade,
    ];
    let mut rng = rand::thread_rng();

    for i in 0..60u64 {
        let x = seeded(seed, i * 3) * 200.0 - 100.0;
        let z = seeded(seed, i * 3 + 1) * 200.0 - 100.0;
        let h = 30.0 + seeded(seed, i * 3 + 2) * 120.0;
        let w = 12.0 + rng.gen_range(0.0f32..18.0);
        let d = 12.0 + rng.gen_range(0.0f32..18.0);

        let mat = glass[(i % 4) as usize].clone();
        spawn_building(
            commands,
            meshes,
            mat,
            Vec3::new(x, h * 0.5, z),
            w,
            h,
            d,
            WorldZone::Downtown,
        );

        // Facade accent band near base
        if i % 3 == 0 {
            spawn_building(
                commands,
                meshes,
                pal.downtown_facade.clone(),
                Vec3::new(x, 1.5, z),
                w + 0.5,
                3.0,
                d + 0.5,
                WorldZone::Downtown,
            );
        }

        // Window facade panels — cover the upper 70 % of each face with
        // emissive glass strips that read as lit office floors at night.
        let win_mat = if i % 2 == 0 {
            pal.window_warm.clone()
        } else {
            pal.window_cool.clone()
        };
        spawn_window_facades(commands, meshes, win_mat, x, z, h, w, d);

        // Rooftop details on taller buildings
        if h > 50.0 {
            spawn_rooftop_details(commands, meshes, pal, x, z, h, w, d, seed + i);
        }
    }
}

// ── Window facades ────────────────────────────────────────────────────────────
/// Spawn four emissive panels (one per building face) covering the upper 70 %
/// of the building height. They simulate rows of lit windows from a distance.
fn spawn_window_facades(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mat: Handle<StandardMaterial>,
    x: f32,
    z: f32,
    h: f32,
    w: f32,
    d: f32,
) {
    let win_h = h * 0.70;
    let win_base_y = h * 0.15; // bottom edge of the window section
    let center_y = win_base_y + win_h * 0.5;
    let thickness = 0.35;

    // Front and back faces (span full width, aligned along X)
    for &fz in &[z - d * 0.5 - thickness * 0.5, z + d * 0.5 + thickness * 0.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(w - 0.8, win_h, thickness))),
                material: MeshMaterial3d(mat.clone()),
                transform: Transform::from_xyz(x, center_y, fz),
                ..default()
            },
            WorldGeometry,
        ));
    }
    // Left and right faces (span full depth, aligned along Z)
    for &fx in &[x - w * 0.5 - thickness * 0.5, x + w * 0.5 + thickness * 0.5] {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(thickness, win_h, d - 0.8))),
                material: MeshMaterial3d(mat.clone()),
                transform: Transform::from_xyz(fx, center_y, z),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

// ── Rooftop details ───────────────────────────────────────────────────────────
/// Add an antenna or water-tank cluster to the top of a building.
fn spawn_rooftop_details(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    x: f32,
    z: f32,
    h: f32,
    w: f32,
    d: f32,
    seed: u64,
) {
    let top = h;
    let kind = seeded(seed, 0);

    if kind < 0.45 {
        // Antenna mast — thin tall cylinder with small beacon sphere on top
        let mast_h = 8.0 + seeded(seed, 1) * 14.0;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(0.22, mast_h))),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: Transform::from_xyz(x, top + mast_h * 0.5, z),
                ..default()
            },
            WorldGeometry,
        ));
        // Blinking beacon (emissive red sphere)
        let beacon_color = if seeded(seed, 2) > 0.5 {
            Color::srgb(1.0, 0.18, 0.12)
        } else {
            Color::srgb(1.0, 0.65, 0.0)
        };
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Sphere::new(0.45))),
                material: MeshMaterial3d(pal.window_warm.clone()),
                transform: Transform::from_xyz(x, top + mast_h + 0.5, z),
                ..default()
            },
            WorldGeometry,
        ));
        // Tiny point-light at the beacon
        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: beacon_color,
                    intensity: 6_000.0,
                    range: 18.0,
                    shadows_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(x, top + mast_h + 0.5, z),
                ..default()
            },
            WorldGeometry,
        ));
    } else if kind < 0.75 {
        // Water/coolant tank — short wide cylinder
        let tank_r = 1.8 + seeded(seed, 3) * 2.2;
        let tank_h = 2.5 + seeded(seed, 4) * 2.0;
        let ox = (seeded(seed, 5) - 0.5) * (w * 0.4);
        let oz = (seeded(seed, 6) - 0.5) * (d * 0.4);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(tank_r, tank_h))),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: Transform::from_xyz(x + ox, top + tank_h * 0.5, z + oz),
                ..default()
            },
            WorldGeometry,
        ));
    } else {
        // Satellite dish — flat disc + support stub
        let dish_r = 2.2 + seeded(seed, 7) * 1.8;
        let ox = (seeded(seed, 8) - 0.5) * (w * 0.35);
        let oz = (seeded(seed, 9) - 0.5) * (d * 0.35);
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(dish_r, 0.25))),
                material: MeshMaterial3d(pal.rooftop.clone()),
                transform: Transform::from_xyz(x + ox, top + 1.5, z + oz)
                    .with_rotation(Quat::from_rotation_x(0.45)),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

// ── Industrial ────────────────────────────────────────────────────────────────
fn spawn_industrial(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    for i in 0..35u64 {
        let x = 120.0 + seeded(seed, i * 3) * 200.0;
        let z = seeded(seed, i * 3 + 1) * 400.0 - 200.0;
        let h = 15.0 + seeded(seed, i * 3 + 2) * 40.0;
        let w = 20.0 + seeded(seed, i * 4) * 30.0;
        let d = 20.0 + seeded(seed, i * 4 + 1) * 30.0;

        let body_mat = if i % 2 == 0 {
            pal.industrial_metal.clone()
        } else {
            pal.industrial_rust.clone()
        };
        spawn_building(
            commands,
            meshes,
            body_mat,
            Vec3::new(x, h * 0.5, z),
            w,
            h,
            d,
            WorldZone::Industrial,
        );

        // Chimney
        spawn_building(
            commands,
            meshes,
            pal.industrial_rust.clone(),
            Vec3::new(x + 5.0, h + 10.0, z + 5.0),
            3.0,
            20.0,
            3.0,
            WorldZone::Industrial,
        );
    }
}

// ── Residential ───────────────────────────────────────────────────────────────
fn spawn_residential(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    for i in 0..50u64 {
        let x = -120.0 - seeded(seed, i * 3) * 200.0;
        let z = seeded(seed, i * 3 + 1) * 400.0 - 200.0;
        let h = 8.0 + seeded(seed, i * 3 + 2) * 25.0;
        let w = 10.0 + seeded(seed, i * 4) * 14.0;
        let d = 8.0 + seeded(seed, i * 4 + 1) * 14.0;

        let mat = if i % 2 == 0 {
            pal.residential_a.clone()
        } else {
            pal.residential_b.clone()
        };
        spawn_building(
            commands,
            meshes,
            mat,
            Vec3::new(x, h * 0.5, z),
            w,
            h,
            d,
            WorldZone::Residential,
        );
    }
}

// ── Highways ──────────────────────────────────────────────────────────────────
fn spawn_highways(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette) {
    // Main east-west highway
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(1200.0, 0.8, 18.0))),
            material: MeshMaterial3d(pal.highway.clone()),
            transform: Transform::from_xyz(0.0, 8.0, 0.0),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Building {
            zone: WorldZone::Highway,
            height: 0.8,
        },
        bevy_rapier3d::prelude::RigidBody::Fixed,
        bevy_rapier3d::prelude::Collider::cuboid(600.0, 0.4, 9.0),
    ));

    // North-south cross
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(18.0, 0.8, 1200.0))),
            material: MeshMaterial3d(pal.highway.clone()),
            transform: Transform::from_xyz(0.0, 6.0, 0.0),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Building {
            zone: WorldZone::Highway,
            height: 0.8,
        },
        bevy_rapier3d::prelude::RigidBody::Fixed,
        bevy_rapier3d::prelude::Collider::cuboid(9.0, 0.4, 600.0),
    ));

    // Support pillars
    for i in -6..=6i32 {
        let x = i as f32 * 100.0;
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(2.5, 8.0, 2.5))),
                material: MeshMaterial3d(pal.industrial_metal.clone()),
                transform: Transform::from_xyz(x, 4.0, 0.0),
                ..default()
            },
            WorldGeometry,
            bevy_rapier3d::prelude::RigidBody::Fixed,
            bevy_rapier3d::prelude::Collider::cuboid(1.25, 4.0, 1.25),
        ));
    }
}

// ── Sky Platforms ─────────────────────────────────────────────────────────────
fn spawn_sky_platforms(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    for i in 0..12u64 {
        let x = seeded(seed, i * 4) * 600.0 - 300.0;
        let y = 40.0 + seeded(seed, i * 4 + 1) * 210.0;
        let z = seeded(seed, i * 4 + 2) * 600.0 - 300.0;
        let size = 25.0 + seeded(seed, i * 4 + 3) * 40.0;

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(size, 3.0))),
                material: MeshMaterial3d(pal.sky_platform.clone()),
                transform: Transform::from_xyz(x, y, z),
                ..default()
            },
            WorldGeometry,
            SkyPlatform,
            WalkableSurface,
            Building {
                zone: WorldZone::SkyPlatform,
                height: 3.0,
            },
            bevy_rapier3d::prelude::RigidBody::Fixed,
            bevy_rapier3d::prelude::Collider::cylinder(1.5, size),
        ));
    }
}

// ── Sky Bridges ───────────────────────────────────────────────────────────────
fn spawn_sky_bridges(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    for i in 0..8u64 {
        let x = seeded(seed, i * 3) * 400.0 - 200.0;
        let y = 60.0 + seeded(seed, i * 3 + 1) * 100.0;
        let z = seeded(seed, i * 3 + 2) * 400.0 - 200.0;

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(80.0, 1.5, 6.0))),
                material: MeshMaterial3d(pal.sky_platform.clone()),
                transform: Transform::from_xyz(x, y, z),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            Building {
                zone: WorldZone::SkyPlatform,
                height: 1.5,
            },
            bevy_rapier3d::prelude::RigidBody::Fixed,
            bevy_rapier3d::prelude::Collider::cuboid(40.0, 0.75, 3.0),
        ));
    }
}

// ── Spaceports ────────────────────────────────────────────────────────────────
fn spawn_spaceports(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette) {
    let positions = [
        Vec3::new(350.0, 0.5, 350.0),
        Vec3::new(-350.0, 0.5, 350.0),
        Vec3::new(350.0, 0.5, -350.0),
        Vec3::new(-350.0, 0.5, -350.0),
    ];

    for pos in positions {
        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cylinder::new(50.0, 2.0))),
                material: MeshMaterial3d(pal.sky_platform.clone()),
                transform: Transform::from_translation(pos),
                ..default()
            },
            WorldGeometry,
            WalkableSurface,
            Building {
                zone: WorldZone::Spaceport,
                height: 2.0,
            },
            bevy_rapier3d::prelude::RigidBody::Fixed,
            bevy_rapier3d::prelude::Collider::cylinder(1.0, 50.0),
        ));
    }
}

// ── Mountains ─────────────────────────────────────────────────────────────────
/// Spawn layered rock spires and snow caps on top of the terrain heightmap.
/// Each "mountain" is several overlapping cones of varying radii and heights
/// so the silhouette reads as a proper rocky peak rather than a single cone.
fn spawn_mountains(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette, seed: u64) {
    let corners = [
        Vec3::new(460.0, 0.0, 460.0),
        Vec3::new(-460.0, 0.0, 460.0),
        Vec3::new(460.0, 0.0, -460.0),
        Vec3::new(-460.0, 0.0, -460.0),
    ];

    for (ci, &corner) in corners.iter().enumerate() {
        for i in 0..18u64 {
            let idx = ci as u64 * 20 + i;

            let ox = (seeded(seed, idx * 5) - 0.5) * 200.0;
            let oz = (seeded(seed, idx * 5 + 1) - 0.5) * 200.0;
            let wx = corner.x + ox;
            let wz = corner.z + oz;

            // Base Y from terrain so spires sit flush on the landscape.
            let ground_y = terrain_height(wx, wz, seed - 5);

            let peak_h = 50.0 + seeded(seed, idx * 5 + 2) * 130.0;
            let base_r = 22.0 + seeded(seed, idx * 5 + 3) * 35.0;

            // ── Main spire ──────────────────────────────────────────────
            commands.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cone {
                        radius: base_r,
                        height: peak_h,
                    })),
                    material: MeshMaterial3d(pal.rock.clone()),
                    transform: Transform::from_xyz(wx, ground_y + peak_h * 0.5, wz),
                    ..default()
                },
                WorldGeometry,
            ));

            // ── Shoulder ridges (2 smaller cones offset from the peak) ──
            for s in 0..2u64 {
                let sr = base_r * (0.45 + seeded(seed, idx * 5 + 4 + s) * 0.25);
                let sh = peak_h * (0.50 + seeded(seed, idx * 7 + s) * 0.30);
                let sdx = (seeded(seed, idx * 11 + s) - 0.5) * base_r * 1.2;
                let sdz = (seeded(seed, idx * 13 + s) - 0.5) * base_r * 1.2;
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Cone {
                            radius: sr,
                            height: sh,
                        })),
                        material: MeshMaterial3d(pal.rock.clone()),
                        transform: Transform::from_xyz(wx + sdx, ground_y + sh * 0.5, wz + sdz),
                        ..default()
                    },
                    WorldGeometry,
                ));
            }

            // ── Snow cap on tall peaks ───────────────────────────────────
            if peak_h > 90.0 {
                let cap_r = base_r * 0.28;
                let cap_h = peak_h * 0.18;
                let cap_y = ground_y + peak_h - cap_h * 0.35;
                commands.spawn((
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(Cone {
                            radius: cap_r,
                            height: cap_h,
                        })),
                        material: MeshMaterial3d(pal.snow.clone()),
                        transform: Transform::from_xyz(wx, cap_y, wz),
                        ..default()
                    },
                    WorldGeometry,
                ));

                // Secondary snow blobs for a less perfect cap.
                for b in 0..2u64 {
                    let br = cap_r * (0.4 + seeded(seed, idx * 17 + b) * 0.3);
                    let bdx = (seeded(seed, idx * 19 + b) - 0.5) * cap_r * 1.4;
                    let bdz = (seeded(seed, idx * 23 + b) - 0.5) * cap_r * 1.4;
                    commands.spawn((
                        PbrBundle {
                            mesh: Mesh3d(meshes.add(Sphere::new(br))),
                            material: MeshMaterial3d(pal.snow.clone()),
                            transform: Transform::from_xyz(wx + bdx, cap_y + br * 0.3, wz + bdz),
                            ..default()
                        },
                        WorldGeometry,
                    ));
                }
            }
        }
    }
}

// ── Neon Lights ───────────────────────────────────────────────────────────────
fn spawn_neon_lights(commands: &mut Commands, seed: u64) {
    let neon_colors = [
        Color::srgb(0.0, 1.0, 1.0),
        Color::srgb(1.0, 0.0, 0.8),
        Color::srgb(0.0, 1.0, 0.3),
        Color::srgb(1.0, 0.6, 0.0),
        Color::srgb(0.5, 0.0, 1.0),
    ];

    for i in 0..70u64 {
        let x = seeded(seed, i * 3) * 600.0 - 300.0;
        let y = 5.0 + seeded(seed, i * 3 + 1) * 60.0;
        let z = seeded(seed, i * 3 + 2) * 600.0 - 300.0;
        let ci = (i % 5) as usize;

        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: neon_colors[ci],
                    intensity: 20_000.0,
                    range: 25.0,
                    shadows_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(x, y, z),
                ..default()
            },
            WorldGeometry,
            NeonLight,
        ));
    }
}

// ── Street Lights ─────────────────────────────────────────────────────────────
fn spawn_street_lights(commands: &mut Commands, seed: u64) {
    for i in 0..80u64 {
        let x = seeded(seed, i * 2) * 400.0 - 200.0;
        let z = seeded(seed, i * 2 + 1) * 400.0 - 200.0;

        commands.spawn((
            PointLightBundle {
                point_light: PointLight {
                    color: Color::srgb(0.9, 0.85, 0.7),
                    intensity: 8_000.0,
                    range: 18.0,
                    shadows_enabled: false,
                    ..default()
                },
                transform: Transform::from_xyz(x, 7.0, z),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

// ── Outer Districts ───────────────────────────────────────────────────────────
fn spawn_outer_districts(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pal: &Palette,
    seed: u64,
) {
    let offsets = [
        Vec3::new(300.0, 0.0, 0.0),
        Vec3::new(-300.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 300.0),
        Vec3::new(0.0, 0.0, -300.0),
        Vec3::new(250.0, 0.0, -250.0),
    ];

    for (di, &offset) in offsets.iter().enumerate() {
        let mat = if di % 2 == 0 {
            pal.residential_a.clone()
        } else {
            pal.residential_b.clone()
        };

        for i in 0..20u64 {
            let idx = di as u64 * 20 + i;
            let ox = seeded(seed, idx * 3) * 120.0 - 60.0;
            let oz = seeded(seed, idx * 3 + 1) * 120.0 - 60.0;
            let h = 10.0 + seeded(seed, idx * 3 + 2) * 50.0;
            let w = 8.0 + seeded(seed, idx * 4) * 15.0;
            let d = 8.0 + seeded(seed, idx * 4 + 1) * 15.0;

            spawn_building(
                commands,
                meshes,
                mat.clone(),
                offset + Vec3::new(ox, h * 0.5, oz),
                w,
                h,
                d,
                WorldZone::OuterDistrict,
            );
        }
    }
}

// ── River ─────────────────────────────────────────────────────────────────────
fn spawn_river(commands: &mut Commands, meshes: &mut Assets<Mesh>, pal: &Palette) {
    for i in -15..=15i32 {
        let z = i as f32 * 40.0;
        let x = (z * 0.05).sin() * 60.0;

        commands.spawn((
            PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(25.0, 0.1, 42.0))),
                material: MeshMaterial3d(pal.water.clone()),
                transform: Transform::from_xyz(x, -0.1, z),
                ..default()
            },
            WorldGeometry,
        ));
    }
}

// ── Trees ─────────────────────────────────────────────────────────────────────
fn spawn_trees(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    seed: u64,
) {
    // Build one template per species (L-system evaluated once, materials allocated once).
    let oak = TreeTemplate::new(TreeKind::Oak, mats);
    let pine = TreeTemplate::new(TreeKind::Pine, mats);
    let dead = TreeTemplate::new(TreeKind::Dead, mats);
    let cyber = TreeTemplate::new(TreeKind::Cyber, mats);

    // (zone_offset, count, template_ref, scale_base, scale_range)
    struct Placement<'a> {
        offset: Vec3,
        spread: f32,
        count: u64,
        template: &'a TreeTemplate,
        scale_base: f32,
        scale_range: f32,
    }

    let placements = [
        // Oak — residential west, scattered at ground level
        Placement {
            offset: Vec3::new(-150.0, 0.0, 0.0),
            spread: 140.0,
            count: 12,
            template: &oak,
            scale_base: 0.9,
            scale_range: 0.6,
        },
        // Oak — outer district north
        Placement {
            offset: Vec3::new(0.0, 0.0, 250.0),
            spread: 100.0,
            count: 8,
            template: &oak,
            scale_base: 0.7,
            scale_range: 0.5,
        },
        // Pine — near mountain corners (NE)
        Placement {
            offset: Vec3::new(320.0, 0.0, 320.0),
            spread: 80.0,
            count: 8,
            template: &pine,
            scale_base: 1.0,
            scale_range: 0.8,
        },
        // Pine — mountain corner SW
        Placement {
            offset: Vec3::new(-320.0, 0.0, -320.0),
            spread: 80.0,
            count: 8,
            template: &pine,
            scale_base: 1.0,
            scale_range: 0.8,
        },
        // Dead — industrial zone east, gaunt silhouettes
        Placement {
            offset: Vec3::new(200.0, 0.0, 0.0),
            spread: 100.0,
            count: 6,
            template: &dead,
            scale_base: 0.8,
            scale_range: 0.4,
        },
        // Cyber — downtown fringe, neon accent trees
        Placement {
            offset: Vec3::new(60.0, 0.0, 60.0),
            spread: 60.0,
            count: 6,
            template: &cyber,
            scale_base: 0.6,
            scale_range: 0.4,
        },
    ];

    let mut idx = 0u64;
    for p in &placements {
        for i in 0..p.count {
            let ox = (seeded(seed, idx * 4) - 0.5) * 2.0 * p.spread;
            let oz = (seeded(seed, idx * 4 + 1) - 0.5) * 2.0 * p.spread;
            let rot = seeded(seed, idx * 4 + 2) * std::f32::consts::TAU;
            let sc = p.scale_base + seeded(seed, idx * 4 + 3) * p.scale_range;
            let pos = p.offset + Vec3::new(ox, 0.0, oz);

            spawn_tree(commands, meshes, p.template, pos, rot, sc);
            idx += 1;
        }
        idx += 100; // separate seed regions between placement groups
    }
}

// ── Building helper ───────────────────────────────────────────────────────────
fn spawn_building(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mat: Handle<StandardMaterial>,
    position: Vec3,
    width: f32,
    height: f32,
    depth: f32,
    zone: WorldZone,
) {
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(width, height, depth))),
            material: MeshMaterial3d(mat),
            transform: Transform::from_translation(position),
            ..default()
        },
        WorldGeometry,
        WalkableSurface,
        Building { zone, height },
        bevy_rapier3d::prelude::RigidBody::Fixed,
        bevy_rapier3d::prelude::Collider::cuboid(width * 0.5, height * 0.5, depth * 0.5),
    ));
}
