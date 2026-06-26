//! Visual gallery for the math-modeled character builder.
//!
//! Spawns the eight roster heroes in a row using the same component recipe and
//! per-hero palettes as `src/player_mesh.rs`, then writes a screenshot to
//! `/tmp/hero_gallery.png` and exits. Self-contained: it includes the pure
//! `procedural_meshes` + `modular_character` modules and inlines the builder
//! (parameterized by colours) so it doesn't need the full game crate.
//!
//! Run with:  `cargo run --example hero_gallery`

#![allow(dead_code)]

#[path = "../src/modular_character.rs"]
mod modular_character;
#[path = "../src/procedural_meshes.rs"]
mod procedural_meshes;

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use modular_character::*;
use procedural_meshes::superellipsoid_mesh;

const TORSO_CENTER_Y: f32 = 0.65;

fn organic(m: &mut Assets<Mesh>, r: Vec3) -> Handle<Mesh> {
    m.add(superellipsoid_mesh(r, 0.82, 0.82, 16, 22))
}
fn plate(m: &mut Assets<Mesh>, r: Vec3) -> Handle<Mesh> {
    m.add(superellipsoid_mesh(r, 0.42, 0.42, 14, 18))
}
fn gem(m: &mut Assets<Mesh>, r: Vec3) -> Handle<Mesh> {
    m.add(superellipsoid_mesh(r, 0.55, 0.55, 12, 16))
}
fn s(name: &'static str, x: f32, y: f32, z: f32) -> Socket {
    Socket::new(name, Transform::from_xyz(x, y, z))
}
fn sr(name: &'static str, x: f32, y: f32, z: f32, q: Quat) -> Socket {
    Socket::new(name, Transform::from_xyz(x, y, z).with_rotation(q))
}
fn scl(c: Color, f: f32) -> Color {
    let s = c.to_srgba();
    Color::srgb(
        (s.red * f).min(1.0),
        (s.green * f).min(1.0),
        (s.blue * f).min(1.0),
    )
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    let x = a.to_srgba();
    let y = b.to_srgba();
    Color::srgb(
        x.red + (y.red - x.red) * t,
        x.green + (y.green - x.green) * t,
        x.blue + (y.blue - x.blue) * t,
    )
}
fn matte(m: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    m.add(StandardMaterial {
        base_color: c,
        perceptual_roughness: 0.82,
        reflectance: 0.16,
        ..default()
    })
}
fn metal(m: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    m.add(StandardMaterial {
        base_color: c,
        perceptual_roughness: 0.36,
        metallic: 0.55,
        reflectance: 0.42,
        ..default()
    })
}
fn glow(m: &mut Assets<StandardMaterial>, c: Color, k: f32) -> Handle<StandardMaterial> {
    let l = c.to_linear();
    m.add(StandardMaterial {
        base_color: c,
        emissive: LinearRgba::new(l.red * k, l.green * k, l.blue * k, 1.0),
        perceptual_roughness: 0.2,
        clearcoat: 0.9,
        ..default()
    })
}

struct Hero {
    name: &'static str,
    outfit: Color,
    accent: Color,
    skin: Color,
    eye: Color,
    emissive_eyes: bool,
    width: f32,
    pauldrons: bool,
    cape: bool,
    visor: bool,
    gauntlets: bool,
    boots: bool,
    belt: bool,
}

/// The eight roster heroes (palettes mirror `hero_config` in `characters.rs`).
fn roster() -> Vec<Hero> {
    let h = |name, outfit, accent, skin, eye, em, width, pauldrons, cape, visor| Hero {
        name,
        outfit,
        accent,
        skin,
        eye,
        emissive_eyes: em,
        width,
        pauldrons,
        cape,
        visor,
        gauntlets: true,
        boots: true,
        belt: true,
    };
    vec![
        h(
            "Vincenzo",
            Color::srgb(0.09, 0.28, 0.80),
            Color::srgb(0.95, 0.78, 0.12),
            Color::srgb(0.93, 0.70, 0.48),
            Color::srgb(0.08, 0.18, 0.72),
            false,
            1.0,
            true,
            true,
            false,
        ),
        h(
            "Antonio",
            Color::srgb(0.16, 0.48, 0.82),
            Color::srgb(0.10, 0.92, 0.98),
            Color::srgb(0.90, 0.74, 0.56),
            Color::srgb(0.08, 0.52, 0.86),
            false,
            0.92,
            false,
            true,
            false,
        ),
        h(
            "Angelo",
            Color::srgb(0.08, 0.52, 0.20),
            Color::srgb(0.78, 0.86, 0.92),
            Color::srgb(0.88, 0.68, 0.48),
            Color::srgb(0.12, 0.52, 0.22),
            false,
            0.94,
            false,
            true,
            false,
        ),
        h(
            "Joseph",
            Color::srgb(0.75, 0.12, 0.08),
            Color::srgb(0.92, 0.90, 0.84),
            Color::srgb(0.76, 0.54, 0.36),
            Color::srgb(0.65, 0.38, 0.08),
            false,
            1.18,
            true,
            true,
            false,
        ),
        h(
            "Gabriella",
            Color::srgb(0.48, 0.12, 0.72),
            Color::srgb(1.0, 0.68, 0.95),
            Color::srgb(0.91, 0.69, 0.50),
            Color::srgb(0.42, 0.08, 0.58),
            false,
            0.98,
            true,
            true,
            false,
        ),
        h(
            "Nova",
            Color::srgb(0.10, 0.18, 0.82),
            Color::srgb(0.10, 1.0, 0.88),
            Color::srgb(0.84, 0.64, 0.50),
            Color::srgb(0.04, 0.82, 0.92),
            true,
            0.94,
            false,
            true,
            true,
        ),
        h(
            "Aurora",
            Color::srgb(0.16, 0.58, 0.42),
            Color::srgb(0.78, 1.0, 0.62),
            Color::srgb(0.90, 0.72, 0.55),
            Color::srgb(0.14, 0.62, 0.28),
            false,
            1.02,
            true,
            true,
            false,
        ),
        h(
            "Fortuna",
            Color::srgb(0.68, 0.22, 0.10),
            Color::srgb(1.0, 0.92, 0.30),
            Color::srgb(0.92, 0.70, 0.52),
            Color::srgb(0.68, 0.48, 0.06),
            false,
            0.96,
            false,
            true,
            false,
        ),
    ]
}

fn build_registry(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    hero: &Hero,
) -> PartRegistry {
    let w = hero.width;
    let l = 0.88 + 0.34 * (w - 1.0);
    let primary = matte(materials, hero.outfit);
    let suit = matte(materials, scl(hero.outfit, 0.55));
    let accent = matte(materials, hero.accent);
    let steel = metal(
        materials,
        mix(Color::srgb(0.60, 0.63, 0.70), hero.accent, 0.30),
    );
    let skin = matte(materials, hero.skin);
    let glw = glow(
        materials,
        hero.eye,
        if hero.emissive_eyes { 6.0 } else { 3.0 },
    );

    let mut reg = PartRegistry::default();
    let mut add = |id, mesh, material, sockets| {
        reg.insert(PartPrefab {
            id,
            mesh,
            material,
            sockets,
        })
    };

    add(
        "torso",
        organic(meshes, Vec3::new(0.145 * w, 0.175, 0.097)),
        suit.clone(),
        vec![
            s("neck", 0.0, 0.18, 0.0),
            sr(
                "shoulder_l",
                -0.135 * w,
                0.13,
                0.0,
                Quat::from_rotation_z(-0.12),
            ),
            sr(
                "shoulder_r",
                0.135 * w,
                0.13,
                0.0,
                Quat::from_rotation_z(0.12),
            ),
            s("pauldron_l", -0.16 * w, 0.155, 0.0),
            s("pauldron_r", 0.16 * w, 0.155, 0.0),
            s("hip_l", -0.072 * w, -0.165, 0.0),
            s("hip_r", 0.072 * w, -0.165, 0.0),
            s("chest", 0.0, 0.05, -0.092),
            s("back", 0.0, 0.10, 0.088),
            s("belt", 0.0, -0.15, -0.04),
        ],
    );
    add(
        "chest_plate",
        plate(meshes, Vec3::new(0.125 * w, 0.135, 0.03)),
        primary.clone(),
        vec![s("mount", 0.0, 0.0, 0.028)],
    );
    add(
        "cape",
        plate(meshes, Vec3::new(0.13 * w, 0.24, 0.014)),
        accent.clone(),
        vec![s("mount", 0.0, 0.22, 0.0)],
    );
    add(
        "buckle",
        gem(meshes, Vec3::new(0.045, 0.035, 0.03)),
        glw.clone(),
        vec![s("mount", 0.0, 0.0, 0.03)],
    );
    add(
        "head",
        organic(meshes, Vec3::new(0.088, 0.10, 0.094)),
        skin.clone(),
        vec![
            s("neck_joint", 0.0, -0.085, 0.0),
            s("crown", 0.0, 0.06, 0.012),
            s("face", 0.0, 0.005, -0.094),
            s("face_l", -0.036, 0.012, -0.09),
            s("face_r", 0.036, 0.012, -0.09),
        ],
    );
    add(
        "helmet",
        plate(meshes, Vec3::new(0.10, 0.072, 0.10)),
        primary.clone(),
        vec![s("rim", 0.0, -0.03, 0.0)],
    );
    add(
        "visor",
        gem(meshes, Vec3::new(0.072, 0.022, 0.022)),
        glw.clone(),
        vec![s("mount", 0.0, 0.0, 0.015)],
    );
    add(
        "eye",
        gem(meshes, Vec3::new(0.017, 0.022, 0.013)),
        glw.clone(),
        vec![s("mount", 0.0, 0.0, 0.01)],
    );
    add(
        "pauldron",
        plate(meshes, Vec3::new(0.085 * w, 0.06, 0.09)),
        primary.clone(),
        vec![s("cap", 0.0, -0.03, 0.0)],
    );
    add(
        "arm_upper",
        organic(meshes, Vec3::new(0.04 * l, 0.105, 0.042 * l)),
        suit.clone(),
        vec![s("shoulder", 0.0, 0.10, 0.0), s("elbow", 0.0, -0.10, 0.0)],
    );
    add(
        "arm_lower",
        organic(meshes, Vec3::new(0.034 * l, 0.095, 0.036 * l)),
        suit.clone(),
        vec![s("elbow", 0.0, 0.09, 0.0), s("wrist", 0.0, -0.09, 0.0)],
    );
    add(
        "hand",
        organic(meshes, Vec3::new(0.044 * l, 0.055, 0.032 * l)),
        if hero.gauntlets {
            steel.clone()
        } else {
            skin.clone()
        },
        vec![s("wrist", 0.0, 0.052, 0.0)],
    );
    add(
        "thigh",
        organic(meshes, Vec3::new(0.06 * l, 0.105, 0.062 * l)),
        suit.clone(),
        vec![s("hip", 0.0, 0.095, 0.0), s("knee", 0.0, -0.095, 0.0)],
    );
    add(
        "shin",
        organic(meshes, Vec3::new(0.05 * l, 0.125, 0.052 * l)),
        if hero.boots {
            steel.clone()
        } else {
            suit.clone()
        },
        vec![s("knee", 0.0, 0.12, 0.0), s("ankle", 0.0, -0.12, 0.0)],
    );
    add(
        "foot",
        plate(meshes, Vec3::new(0.052, 0.04, 0.10)),
        if hero.boots {
            steel.clone()
        } else {
            accent.clone()
        },
        vec![s("ankle", 0.0, 0.04, 0.05)],
    );
    reg
}

fn build_recipe(hero: &Hero) -> CharacterRecipe {
    let a = |ps, psk, cs, cp, csk| Attachment {
        parent_slot: ps,
        parent_socket: psk,
        child_slot: cs,
        child_part: cp,
        child_socket: csk,
    };
    let mut at = vec![
        a("torso", "neck", "head", "head", "neck_joint"),
        a("head", "crown", "helmet", "helmet", "rim"),
        a("torso", "chest", "chest_plate", "chest_plate", "mount"),
        a(
            "torso",
            "shoulder_l",
            "arm_upper_l",
            "arm_upper",
            "shoulder",
        ),
        a(
            "torso",
            "shoulder_r",
            "arm_upper_r",
            "arm_upper",
            "shoulder",
        ),
        a("arm_upper_l", "elbow", "arm_lower_l", "arm_lower", "elbow"),
        a("arm_upper_r", "elbow", "arm_lower_r", "arm_lower", "elbow"),
        a("arm_lower_l", "wrist", "hand_l", "hand", "wrist"),
        a("arm_lower_r", "wrist", "hand_r", "hand", "wrist"),
        a("torso", "hip_l", "thigh_l", "thigh", "hip"),
        a("torso", "hip_r", "thigh_r", "thigh", "hip"),
        a("thigh_l", "knee", "shin_l", "shin", "knee"),
        a("thigh_r", "knee", "shin_r", "shin", "knee"),
        a("shin_l", "ankle", "foot_l", "foot", "ankle"),
        a("shin_r", "ankle", "foot_r", "foot", "ankle"),
    ];
    if hero.visor {
        at.push(a("head", "face", "visor", "visor", "mount"));
    } else {
        at.push(a("head", "face_l", "eye_l", "eye", "mount"));
        at.push(a("head", "face_r", "eye_r", "eye", "mount"));
    }
    if hero.pauldrons {
        at.push(a("torso", "pauldron_l", "pauldron_l", "pauldron", "cap"));
        at.push(a("torso", "pauldron_r", "pauldron_r", "pauldron", "cap"));
    }
    if hero.cape {
        at.push(a("torso", "back", "cape", "cape", "mount"));
    }
    if hero.belt {
        at.push(a("torso", "belt", "buckle", "buckle", "mount"));
    }
    CharacterRecipe {
        root_slot: "torso",
        root_part: "torso",
        attachments: at,
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(ModularCharacterPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, shutter)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let heroes = roster();
    let n = heroes.len();
    let spacing = 2.4;
    let total = spacing * (n as f32 - 1.0);

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 1.4, total * 0.62 + 4.0)
            .looking_at(Vec3::new(0.0, 0.85, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Fill light from the opposite side.
    commands.spawn((
        DirectionalLight {
            illuminance: 3500.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-5.0, 4.0, -3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Ground.
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Cuboid::new(total + 6.0, 0.1, 6.0)))),
        MeshMaterial3d(materials.add(StandardMaterial::from(Color::srgb(0.20, 0.22, 0.26)))),
        Transform::from_xyz(0.0, -0.05, 0.0),
    ));

    let scale = 1.85; // figure height in world units
    for (i, hero) in heroes.iter().enumerate() {
        let reg = build_registry(&mut meshes, &mut materials, hero);
        let recipe = build_recipe(hero);
        let x = -total * 0.5 + spacing * i as f32;
        let torso_y = TORSO_CENTER_Y * scale;
        // Rotate 180° about Y so the front (-Z) faces the +Z camera.
        let local = Transform::from_xyz(x, torso_y, 0.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
            .with_scale(Vec3::splat(scale));
        let parent = commands.spawn(Transform::default()).id();
        if let Err(e) = spawn_character_under(&mut commands, &reg, &recipe, parent, local) {
            error!("{}: {e}", hero.name);
        }
    }
}

fn shutter(
    time: Res<Time>,
    mut commands: Commands,
    mut shot: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    let t = time.elapsed_secs();
    if t > 0.8 && !*shot {
        *shot = true;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("/tmp/hero_gallery.png"));
    }
    if t > 2.0 {
        exit.write(AppExit::Success);
    }
}
