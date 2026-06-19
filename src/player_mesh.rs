//! Native modular player mesh.
//!
//! A fully procedural (pure-Rust) humanoid built on the socket-assembly system
//! in [`crate::modular_character`]. The GLBs under `assets/Characters/` are used
//! only as **visual reference** for the silhouette — no glTF is loaded here.
//! Every part is a [`superellipsoid_mesh`] (the painterly, slightly-boxy
//! primitive the rest of the game uses) snapped together at named sockets.
//!
//! The figure is authored in a normalized space: feet at `y = 0`, head top at
//! `y ≈ 1.0`, torso centre at [`TORSO_CENTER_Y`]. At attach time the whole rig
//! is scaled and offset to fill the player's physics capsule, then parented
//! under the player entity so it inherits the player's world transform.

use bevy::prelude::*;

use crate::characters::{char_mat, emissive_mat, CartoonCharacterConfig};
use crate::modular_character::{
    spawn_character_under, Attachment, CharacterRecipe, PartPrefab, PartRegistry, Socket,
};
use crate::procedural_meshes::superellipsoid_mesh;

/// Torso centre height in normalized figure space (feet = 0, head top ≈ 1.0).
const TORSO_CENTER_Y: f32 = 0.63;

/// Rounded-but-boxy storybook silhouette; exponents < 1.0 puff the faces out.
const PUFF: f32 = 0.75;
const STACKS: usize = 18;
const SECTORS: usize = 24;

fn body_mesh(meshes: &mut Assets<Mesh>, radii: Vec3) -> Handle<Mesh> {
    meshes.add(superellipsoid_mesh(radii, PUFF, PUFF, STACKS, SECTORS))
}

fn socket(name: &'static str, x: f32, y: f32, z: f32) -> Socket {
    Socket::new(name, Transform::from_xyz(x, y, z))
}

/// Build a self-contained registry for one player, with materials coloured from
/// `config`. Kept local (not the shared global resource) so each player keeps
/// its own skin/outfit/accent palette.
fn build_player_registry(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    config: &CartoonCharacterConfig,
) -> PartRegistry {
    let skin = char_mat(materials, config.skin);
    let outfit = char_mat(materials, config.outfit);
    let accent = char_mat(materials, config.accent);
    let eye = emissive_mat(
        materials,
        config.eye_color,
        if config.emissive_eyes { 6.0 } else { 3.5 },
    );

    let mut reg = PartRegistry::default();

    // Torso (root part). Sockets at neck, both shoulders, both hips.
    reg.insert(PartPrefab {
        id: "torso",
        mesh: body_mesh(meshes, Vec3::new(0.135, 0.165, 0.085)),
        material: outfit.clone(),
        sockets: vec![
            socket("neck", 0.0, 0.19, 0.0),
            Socket::new(
                "shoulder_l",
                Transform::from_xyz(-0.135, 0.11, 0.0)
                    .with_rotation(Quat::from_rotation_z(-0.14)),
            ),
            Socket::new(
                "shoulder_r",
                Transform::from_xyz(0.135, 0.11, 0.0).with_rotation(Quat::from_rotation_z(0.14)),
            ),
            socket("hip_l", -0.07, -0.14, 0.0),
            socket("hip_r", 0.07, -0.14, 0.0),
        ],
    });

    // Head + emissive eye band.
    reg.insert(PartPrefab {
        id: "head",
        mesh: body_mesh(meshes, Vec3::new(0.092, 0.105, 0.098)),
        material: skin.clone(),
        sockets: vec![
            socket("neck_joint", 0.0, -0.08, 0.0),
            socket("face", 0.0, 0.0, -0.098),
        ],
    });
    reg.insert(PartPrefab {
        id: "eyes",
        mesh: body_mesh(meshes, Vec3::new(0.062, 0.02, 0.02)),
        material: eye,
        sockets: vec![socket("anchor", 0.0, 0.0, 0.018)],
    });

    // Arms: one symmetric prefab per segment, attached to both sides.
    reg.insert(PartPrefab {
        id: "arm_upper",
        mesh: body_mesh(meshes, Vec3::new(0.038, 0.105, 0.040)),
        material: outfit.clone(),
        sockets: vec![socket("shoulder", 0.0, 0.10, 0.0), socket("elbow", 0.0, -0.10, 0.0)],
    });
    reg.insert(PartPrefab {
        id: "arm_lower",
        mesh: body_mesh(meshes, Vec3::new(0.032, 0.10, 0.034)),
        material: skin.clone(),
        sockets: vec![socket("elbow", 0.0, 0.095, 0.0), socket("wrist", 0.0, -0.095, 0.0)],
    });
    reg.insert(PartPrefab {
        id: "hand",
        mesh: body_mesh(meshes, Vec3::new(0.040, 0.055, 0.028)),
        material: skin.clone(),
        sockets: vec![socket("wrist", 0.0, 0.055, 0.0)],
    });

    // Legs: thigh / shin / foot.
    reg.insert(PartPrefab {
        id: "thigh",
        mesh: body_mesh(meshes, Vec3::new(0.058, 0.115, 0.060)),
        material: outfit,
        sockets: vec![socket("hip", 0.0, 0.11, 0.0), socket("knee", 0.0, -0.11, 0.0)],
    });
    reg.insert(PartPrefab {
        id: "shin",
        mesh: body_mesh(meshes, Vec3::new(0.046, 0.115, 0.048)),
        material: accent.clone(),
        sockets: vec![socket("knee", 0.0, 0.11, 0.0), socket("ankle", 0.0, -0.11, 0.0)],
    });
    reg.insert(PartPrefab {
        id: "foot",
        // Longer along -Z so the toes point forward; ankle sits at the heel.
        mesh: body_mesh(meshes, Vec3::new(0.05, 0.035, 0.09)),
        material: accent,
        sockets: vec![socket("ankle", 0.0, 0.035, 0.05)],
    });

    reg
}

/// The humanoid recipe: a root torso plus limbs/head attached at sockets.
/// Parents are always listed before their children.
fn player_recipe() -> CharacterRecipe {
    let att = |parent_slot, parent_socket, child_slot, child_part, child_socket| Attachment {
        parent_slot,
        parent_socket,
        child_slot,
        child_part,
        child_socket,
    };
    CharacterRecipe {
        root_slot: "torso",
        root_part: "torso",
        attachments: vec![
            att("torso", "neck", "head", "head", "neck_joint"),
            att("head", "face", "eyes", "eyes", "anchor"),
            att("torso", "shoulder_l", "arm_upper_l", "arm_upper", "shoulder"),
            att("torso", "shoulder_r", "arm_upper_r", "arm_upper", "shoulder"),
            att("arm_upper_l", "elbow", "arm_lower_l", "arm_lower", "elbow"),
            att("arm_upper_r", "elbow", "arm_lower_r", "arm_lower", "elbow"),
            att("arm_lower_l", "wrist", "hand_l", "hand", "wrist"),
            att("arm_lower_r", "wrist", "hand_r", "hand", "wrist"),
            att("torso", "hip_l", "thigh_l", "thigh", "hip"),
            att("torso", "hip_r", "thigh_r", "thigh", "hip"),
            att("thigh_l", "knee", "shin_l", "shin", "knee"),
            att("thigh_r", "knee", "shin_r", "shin", "knee"),
            att("shin_l", "ankle", "foot_l", "foot", "ankle"),
            att("shin_r", "ankle", "foot_r", "foot", "ankle"),
        ],
    }
}

/// Attach a fully-procedural modular humanoid as the visual for `root` (the
/// player entity), sized to fill its physics capsule. Call
/// [`crate::characters::attach_player_gameplay_rig`] alongside this to keep the
/// skeleton / weapon-attach points alive.
pub fn attach_modular_player_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    config: &CartoonCharacterConfig,
) {
    let registry = build_player_registry(meshes, materials, config);
    let recipe = player_recipe();

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
    // Place the torso so scaled feet (TORSO_CENTER_Y below the torso) land at the
    // capsule bottom: feet_y = torso_y - TORSO_CENTER_Y * scale = -(hh + rr).
    let torso_y = -(half_height + radius) + TORSO_CENTER_Y * capsule_total;
    let local =
        Transform::from_xyz(0.0, torso_y, 0.0).with_scale(Vec3::splat(capsule_total));

    if let Err(err) = spawn_character_under(commands, &registry, &recipe, root, local) {
        error!("modular player mesh assembly failed: {err}");
    }
}
