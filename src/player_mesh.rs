//! Native math-modeled character builder.
//!
//! A component-based, fully-procedural (pure-Rust) hero builder layered on the
//! socket-assembly system in [`crate::modular_character`]. Each hero is assembled
//! from math primitives ([`superellipsoid_mesh`]) snapped together at named
//! sockets: base limbs in an under-suit, then armor *components* (chest plate,
//! pauldrons, helmet, visor, gauntlets, greaves, cape, belt) added on top.
//!
//! The GLBs under `assets/Characters/` are **visual reference only** — never
//! loaded. Their analysis drives this builder: palettes sampled from the baked
//! textures, heroic/leggy proportions from AMP's skeleton, and the layered-plate
//! silhouette of the Tripo models. Per-hero variation is read from the existing
//! [`CartoonCharacterConfig`] (palette + feature flags + build width).
//!
//! Figure space is normalized: feet at `y = 0`, head top at `y ≈ 1.0`, torso
//! centre at [`TORSO_CENTER_Y`]. At attach time the rig is scaled/offset to fill
//! the player's physics capsule and parented under the player entity.

use bevy::prelude::*;

use crate::characters::{char_mat, emissive_mat, CartoonCharacterConfig};
use crate::modular_character::{
    spawn_character_under, Attachment, CharacterRecipe, PartPrefab, PartRegistry, Socket,
};
use crate::procedural_meshes::superellipsoid_mesh;

/// Torso centre height in normalized figure space (feet = 0, head top ≈ 1.0).
const TORSO_CENTER_Y: f32 = 0.65;

// ── Mesh primitives ────────────────────────────────────────────────────────────

/// Soft organic volume (limbs, head) — puffy, slightly boxy.
fn organic(meshes: &mut Assets<Mesh>, radii: Vec3) -> Handle<Mesh> {
    meshes.add(superellipsoid_mesh(radii, 0.82, 0.82, 16, 22))
}
/// Hard armor plate — boxy with rounded bevels.
fn plate(meshes: &mut Assets<Mesh>, radii: Vec3) -> Handle<Mesh> {
    meshes.add(superellipsoid_mesh(radii, 0.42, 0.42, 14, 18))
}
/// Faceted gem (eyes, visor, buckle) for emissive accents.
fn gem(meshes: &mut Assets<Mesh>, radii: Vec3) -> Handle<Mesh> {
    meshes.add(superellipsoid_mesh(radii, 0.55, 0.55, 12, 16))
}

fn sock(name: &'static str, x: f32, y: f32, z: f32) -> Socket {
    Socket::new(name, Transform::from_xyz(x, y, z))
}
fn sock_rot(name: &'static str, x: f32, y: f32, z: f32, rot: Quat) -> Socket {
    Socket::new(name, Transform::from_xyz(x, y, z).with_rotation(rot))
}

// ── Colour helpers ───────────────────────────────────────────────────────────

fn scl(c: Color, f: f32) -> Color {
    let s = c.to_srgba();
    Color::srgb(
        (s.red * f).clamp(0.0, 1.0),
        (s.green * f).clamp(0.0, 1.0),
        (s.blue * f).clamp(0.0, 1.0),
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

/// Metallic-armor material (steel tinted toward the accent), distinct from the
/// matte `char_mat` cloth used elsewhere.
fn metal_mat(
    materials: &mut Assets<StandardMaterial>,
    color: Color,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.36,
        metallic: 0.55,
        reflectance: 0.42,
        ..default()
    })
}

// ── Hero design (component spec derived from config) ────────────────────────────

struct HeroDesign {
    /// X scale of torso/shoulders/hips (broad vs lean).
    width: f32,
    /// Cross-section multiplier for limbs (bulk).
    limb: f32,
    pauldrons: bool,
    cape: bool,
    visor: bool,
    gauntlets: bool,
    boots: bool,
    belt: bool,
}

impl HeroDesign {
    fn from_config(cfg: &CartoonCharacterConfig) -> Self {
        let w = cfg.body_width;
        HeroDesign {
            width: w,
            limb: 0.88 + 0.34 * (w - 1.0),
            pauldrons: cfg.has_shoulder_pads,
            cape: cfg.has_cape,
            visor: cfg.has_visor,
            gauntlets: cfg.has_gloves,
            boots: cfg.has_boots,
            belt: cfg.has_belt,
        }
    }
}

struct Palette {
    /// Main armor / outfit colour (chest plate, helmet, pauldrons).
    primary: Handle<StandardMaterial>,
    /// Darker under-suit (torso base, limbs).
    suit: Handle<StandardMaterial>,
    /// Bright accent (cape, trim).
    accent: Handle<StandardMaterial>,
    /// Steel for hard surfaces (gauntlets, greaves).
    metal: Handle<StandardMaterial>,
    skin: Handle<StandardMaterial>,
    /// Emissive (eyes, visor, buckle).
    glow: Handle<StandardMaterial>,
}

impl Palette {
    fn build(materials: &mut Assets<StandardMaterial>, cfg: &CartoonCharacterConfig) -> Self {
        let outfit = cfg.outfit;
        let steel = mix(Color::srgb(0.60, 0.63, 0.70), cfg.accent, 0.30);
        Palette {
            primary: char_mat(materials, outfit),
            suit: char_mat(materials, scl(outfit, 0.55)),
            accent: char_mat(materials, cfg.accent),
            metal: metal_mat(materials, steel),
            skin: char_mat(materials, cfg.skin),
            glow: emissive_mat(
                materials,
                cfg.eye_color,
                if cfg.emissive_eyes { 6.0 } else { 3.0 },
            ),
        }
    }
}

// ── Part registry (one per spawned hero, coloured from the palette) ─────────────

fn build_registry(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cfg: &CartoonCharacterConfig,
    d: &HeroDesign,
) -> PartRegistry {
    let pal = Palette::build(materials, cfg);
    let w = d.width;
    let l = d.limb;
    let mut reg = PartRegistry::default();

    let mut add = |id, mesh, material, sockets| {
        reg.insert(PartPrefab {
            id,
            mesh,
            material,
            sockets,
        })
    };

    // Torso (root): under-suit core with mount sockets for limbs + armor.
    add(
        "torso",
        organic(meshes, Vec3::new(0.145 * w, 0.175, 0.097)),
        pal.suit.clone(),
        vec![
            sock("neck", 0.0, 0.18, 0.0),
            sock_rot("shoulder_l", -0.135 * w, 0.13, 0.0, Quat::from_rotation_z(-0.12)),
            sock_rot("shoulder_r", 0.135 * w, 0.13, 0.0, Quat::from_rotation_z(0.12)),
            sock("pauldron_l", -0.16 * w, 0.155, 0.0),
            sock("pauldron_r", 0.16 * w, 0.155, 0.0),
            sock("hip_l", -0.072 * w, -0.165, 0.0),
            sock("hip_r", 0.072 * w, -0.165, 0.0),
            sock("chest", 0.0, 0.05, -0.092),
            sock("back", 0.0, 0.10, 0.088),
            sock("belt", 0.0, -0.15, -0.04),
        ],
    );

    // Chest plate (front armor).
    add(
        "chest_plate",
        plate(meshes, Vec3::new(0.125 * w, 0.135, 0.03)),
        pal.primary.clone(),
        vec![sock("mount", 0.0, 0.0, 0.028)],
    );
    // Cape (hangs behind).
    add(
        "cape",
        plate(meshes, Vec3::new(0.155 * w, 0.30, 0.016)),
        pal.accent.clone(),
        vec![sock("mount", 0.0, 0.27, 0.0)],
    );
    // Belt buckle (emissive accent).
    add(
        "buckle",
        gem(meshes, Vec3::new(0.045, 0.035, 0.03)),
        pal.glow.clone(),
        vec![sock("mount", 0.0, 0.0, 0.03)],
    );

    // Head + helmet + face.
    add(
        "head",
        organic(meshes, Vec3::new(0.088, 0.10, 0.094)),
        pal.skin.clone(),
        vec![
            sock("neck_joint", 0.0, -0.085, 0.0),
            sock("crown", 0.0, 0.06, 0.012),
            sock("face", 0.0, 0.005, -0.094),
            sock("face_l", -0.036, 0.012, -0.09),
            sock("face_r", 0.036, 0.012, -0.09),
        ],
    );
    add(
        "helmet",
        plate(meshes, Vec3::new(0.10, 0.072, 0.10)),
        pal.primary.clone(),
        vec![sock("rim", 0.0, -0.03, 0.0)],
    );
    add(
        "visor",
        gem(meshes, Vec3::new(0.072, 0.022, 0.022)),
        pal.glow.clone(),
        vec![sock("mount", 0.0, 0.0, 0.015)],
    );
    add(
        "eye",
        gem(meshes, Vec3::new(0.017, 0.022, 0.013)),
        pal.glow.clone(),
        vec![sock("mount", 0.0, 0.0, 0.01)],
    );

    // Pauldron (shoulder dome).
    add(
        "pauldron",
        plate(meshes, Vec3::new(0.085 * w, 0.06, 0.09)),
        pal.primary.clone(),
        vec![sock("cap", 0.0, -0.03, 0.0)],
    );

    // Arms.
    add(
        "arm_upper",
        organic(meshes, Vec3::new(0.04 * l, 0.105, 0.042 * l)),
        pal.suit.clone(),
        vec![sock("shoulder", 0.0, 0.10, 0.0), sock("elbow", 0.0, -0.10, 0.0)],
    );
    add(
        "arm_lower",
        organic(meshes, Vec3::new(0.034 * l, 0.095, 0.036 * l)),
        pal.suit.clone(),
        vec![sock("elbow", 0.0, 0.09, 0.0), sock("wrist", 0.0, -0.09, 0.0)],
    );
    add(
        "hand",
        organic(meshes, Vec3::new(0.044 * l, 0.055, 0.032 * l)),
        if d.gauntlets { pal.metal.clone() } else { pal.skin.clone() },
        vec![sock("wrist", 0.0, 0.052, 0.0)],
    );

    // Legs.
    add(
        "thigh",
        organic(meshes, Vec3::new(0.06 * l, 0.105, 0.062 * l)),
        pal.suit.clone(),
        vec![sock("hip", 0.0, 0.095, 0.0), sock("knee", 0.0, -0.095, 0.0)],
    );
    add(
        "shin",
        organic(meshes, Vec3::new(0.05 * l, 0.125, 0.052 * l)),
        if d.boots { pal.metal.clone() } else { pal.suit.clone() },
        vec![sock("knee", 0.0, 0.12, 0.0), sock("ankle", 0.0, -0.12, 0.0)],
    );
    add(
        "foot",
        plate(meshes, Vec3::new(0.052, 0.04, 0.10)),
        if d.boots { pal.metal.clone() } else { pal.accent.clone() },
        vec![sock("ankle", 0.0, 0.04, 0.05)],
    );

    reg
}

// ── Recipe (component graph; parents listed before children) ─────────────────────

fn build_recipe(d: &HeroDesign) -> CharacterRecipe {
    let a = |parent_slot, parent_socket, child_slot, child_part, child_socket| Attachment {
        parent_slot,
        parent_socket,
        child_slot,
        child_part,
        child_socket,
    };
    let mut at = vec![
        a("torso", "neck", "head", "head", "neck_joint"),
        a("head", "crown", "helmet", "helmet", "rim"),
        a("torso", "chest", "chest_plate", "chest_plate", "mount"),
        // Arms.
        a("torso", "shoulder_l", "arm_upper_l", "arm_upper", "shoulder"),
        a("torso", "shoulder_r", "arm_upper_r", "arm_upper", "shoulder"),
        a("arm_upper_l", "elbow", "arm_lower_l", "arm_lower", "elbow"),
        a("arm_upper_r", "elbow", "arm_lower_r", "arm_lower", "elbow"),
        a("arm_lower_l", "wrist", "hand_l", "hand", "wrist"),
        a("arm_lower_r", "wrist", "hand_r", "hand", "wrist"),
        // Legs.
        a("torso", "hip_l", "thigh_l", "thigh", "hip"),
        a("torso", "hip_r", "thigh_r", "thigh", "hip"),
        a("thigh_l", "knee", "shin_l", "shin", "knee"),
        a("thigh_r", "knee", "shin_r", "shin", "knee"),
        a("shin_l", "ankle", "foot_l", "foot", "ankle"),
        a("shin_r", "ankle", "foot_r", "foot", "ankle"),
    ];

    // Face: visor band or twin eyes.
    if d.visor {
        at.push(a("head", "face", "visor", "visor", "mount"));
    } else {
        at.push(a("head", "face_l", "eye_l", "eye", "mount"));
        at.push(a("head", "face_r", "eye_r", "eye", "mount"));
    }
    if d.pauldrons {
        at.push(a("torso", "pauldron_l", "pauldron_l", "pauldron", "cap"));
        at.push(a("torso", "pauldron_r", "pauldron_r", "pauldron", "cap"));
    }
    if d.cape {
        at.push(a("torso", "back", "cape", "cape", "mount"));
    }
    if d.belt {
        at.push(a("torso", "belt", "buckle", "buckle", "mount"));
    }

    CharacterRecipe {
        root_slot: "torso",
        root_part: "torso",
        attachments: at,
    }
}

// ── Public attach ────────────────────────────────────────────────────────────

/// Attach a fully math-modeled, component-based hero as the visual for `root`
/// (the player entity), sized to fill its physics capsule. Call
/// [`crate::characters::attach_player_gameplay_rig`] alongside this to keep the
/// skeleton / weapon-attach points alive.
pub fn attach_modular_player_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    config: &CartoonCharacterConfig,
) {
    let design = HeroDesign::from_config(config);
    let registry = build_registry(meshes, materials, config, &design);
    let recipe = build_recipe(&design);

    // Mirror the capsule extents from `authored_player_defaults` so the figure
    // exactly fills the collider: feet at the capsule bottom, head at the top.
    let s = config.scale;
    let body = config.body.validated();
    let half_height =
        (0.6 * (body.height * 0.72 + body.leg_length * 0.28) * s).clamp(0.44 * s, 0.86 * s);
    let radius = (0.35
        * (body.shoulder_width * 0.55 + body.chest_size * 0.25 + body.hip_width * 0.20)
        * s)
        .clamp(0.26 * s, 0.50 * s);

    let capsule_total = 2.0 * (half_height + radius);
    let torso_y = -(half_height + radius) + TORSO_CENTER_Y * capsule_total;
    let local = Transform::from_xyz(0.0, torso_y, 0.0).with_scale(Vec3::splat(capsule_total));

    if let Err(err) = spawn_character_under(commands, &registry, &recipe, root, local) {
        error!("modular player mesh assembly failed: {err}");
    }
}
