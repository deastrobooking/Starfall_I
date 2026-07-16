//! Modular character mesh assembly.
//!
//! Full characters are built by snapping predefined prefab parts together at
//! named **sockets**. A part is authored in its own local space; each socket is
//! a local [`Transform`]. To glue child part *B* onto parent part *A* so that
//! `b_sock` coincides with `a_sock`, the child's transform relative to the
//! parent entity is:
//!
//! ```text
//! T_child = T_a_sock * T_b_sock⁻¹
//! ```
//!
//! Two assembly paths are provided:
//! * [`spawn_character`] — a live entity hierarchy with hot-swappable parts.
//! * [`bake_character_mesh`] — one merged mesh for the performance path.
//!
//! This module is intentionally self-contained (it depends only on `bevy` and
//! `std`) so the `examples/modular_character.rs` demo can include it directly.

#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use std::collections::HashMap;
use std::fmt;

// ── Part & socket definitions ─────────────────────────────────────────────────

/// A named attachment point in a part's local space.
#[derive(Clone, Debug)]
pub struct Socket {
    pub name: &'static str,
    /// Position + rotation (+ scale, usually 1.0) in the part's local frame.
    pub local: Transform,
}

impl Socket {
    pub fn new(name: &'static str, local: Transform) -> Self {
        Self { name, local }
    }
}

/// A prefab body part: a mesh + material + its sockets.
#[derive(Clone)]
pub struct PartPrefab {
    /// Stable id, e.g. `"arm_upper_slim"`.
    pub id: &'static str,
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    pub sockets: Vec<Socket>,
}

impl PartPrefab {
    pub fn socket(&self, name: &str) -> Option<&Socket> {
        self.sockets.iter().find(|s| s.name == name)
    }
}

/// Registry of all available prefab parts, keyed by id.
#[derive(Resource, Default)]
pub struct PartRegistry {
    pub parts: HashMap<&'static str, PartPrefab>,
}

impl PartRegistry {
    pub fn insert(&mut self, prefab: PartPrefab) {
        self.parts.insert(prefab.id, prefab);
    }

    pub fn get(&self, id: &str) -> Option<&PartPrefab> {
        self.parts.get(id)
    }
}

// ── Socket alignment math ──────────────────────────────────────────────────────

/// Transform of the child part *relative to the parent part's entity* so that
/// the child's socket coincides with the parent's socket.
///
/// `T_child = T_parent_socket * T_child_socket⁻¹`
pub fn align_socket(parent_socket: &Transform, child_socket: &Transform) -> Transform {
    let parent_mat = parent_socket.to_matrix();
    let child_inv = child_socket.to_matrix().inverse();
    Transform::from_matrix(parent_mat * child_inv)
}

/// Mirror a socket across the YZ plane (negate local X position and reflect the
/// rotation). Handy when a right limb reuses the left limb's authored mesh.
pub fn mirrored_socket_x(s: &Socket) -> Socket {
    let mut local = s.local;
    local.translation.x = -local.translation.x;
    let (axis, angle) = local.rotation.to_axis_angle();
    local.rotation = Quat::from_axis_angle(Vec3::new(-axis.x, axis.y, axis.z), -angle);
    Socket {
        name: s.name,
        local,
    }
}

// ── Character recipe (which parts go where) ────────────────────────────────────

/// One attachment instruction: take `child_part`, glue its `child_socket` onto
/// `parent_socket` of the part bound to `parent_slot`.
#[derive(Clone, Debug)]
pub struct Attachment {
    pub parent_slot: &'static str,
    pub parent_socket: &'static str,
    pub child_slot: &'static str,
    pub child_part: &'static str,
    pub child_socket: &'static str,
}

/// A full character description: a root part plus an ordered list of
/// attachments. A parent slot must appear before its children.
#[derive(Clone, Debug)]
pub struct CharacterRecipe {
    pub root_slot: &'static str,
    pub root_part: &'static str,
    pub attachments: Vec<Attachment>,
}

/// Errors produced while resolving a recipe against a registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssemblyError {
    MissingPart(&'static str),
    MissingSocket {
        part: &'static str,
        socket: &'static str,
    },
    /// An attachment referenced a parent slot that has not been bound yet.
    UnknownParentSlot(&'static str),
}

impl fmt::Display for AssemblyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssemblyError::MissingPart(id) => write!(f, "no prefab registered for part '{id}'"),
            AssemblyError::MissingSocket { part, socket } => {
                write!(f, "part '{part}' has no socket '{socket}'")
            }
            AssemblyError::UnknownParentSlot(slot) => write!(
                f,
                "attachment references parent slot '{slot}' that is not bound yet \
                 (list parents before their children)"
            ),
        }
    }
}

impl std::error::Error for AssemblyError {}

// ── ECS components / messages / resources ──────────────────────────────────────

/// Root entity of an assembled character.
#[derive(Component)]
pub struct CharacterRoot;

/// Logical slot a part entity occupies (e.g. `"arm_upper_l"`).
#[derive(Component, Clone, Copy)]
pub struct BodySlot(pub &'static str);

/// Prefab id currently bound to this entity's slot (used for debug + swapping).
#[derive(Component, Clone, Copy)]
pub struct SlotPart(pub &'static str);

/// Back-pointer to the character root every part belongs to.
#[derive(Component, Clone, Copy)]
pub struct CharacterMember(pub Entity);

/// The recipe an assembled character was built from, kept on its root so parts
/// can be hot-swapped later.
#[derive(Component, Clone)]
pub struct CharacterAssembly {
    pub recipe: CharacterRecipe,
}

/// Request to replace the part bound to `slot` (and re-attach its descendants).
#[derive(Message, Clone)]
pub struct SwapPart {
    pub root: Entity,
    pub slot: &'static str,
    pub new_part: &'static str,
}

/// Toggle for the debug socket gizmo overlay.
#[derive(Resource, Default)]
pub struct DebugSockets(pub bool);

// ── Hierarchy assembly (live, swappable) ───────────────────────────────────────

/// Spawn a character as an entity hierarchy. Returns the root entity, or an
/// [`AssemblyError`] if the recipe references a missing part/socket.
pub fn spawn_character(
    commands: &mut Commands,
    registry: &PartRegistry,
    recipe: &CharacterRecipe,
    root_transform: Transform,
) -> Result<Entity, AssemblyError> {
    let root_prefab = registry
        .get(recipe.root_part)
        .ok_or(AssemblyError::MissingPart(recipe.root_part))?;

    let root = commands
        .spawn((
            Mesh3d(root_prefab.mesh.clone()),
            MeshMaterial3d(root_prefab.material.clone()),
            root_transform,
            CharacterRoot,
            BodySlot(recipe.root_slot),
            SlotPart(recipe.root_part),
            CharacterAssembly {
                recipe: recipe.clone(),
            },
        ))
        .id();
    // CharacterMember needs the just-allocated id, so attach it afterwards.
    commands.entity(root).insert(CharacterMember(root));

    attach_children(commands, registry, recipe, root)?;
    Ok(root)
}

/// Assemble a character as a child of an existing `parent` entity, returning the
/// assembly's own root entity (the one bound to the recipe's root slot).
///
/// Unlike [`spawn_character`], the parent here is some pre-existing entity that
/// owns its own transform/components (e.g. a physics-driven player). The whole
/// rig is parented under it via `local`, so it inherits the parent's world
/// transform and moves with it.
pub fn spawn_character_under(
    commands: &mut Commands,
    registry: &PartRegistry,
    recipe: &CharacterRecipe,
    parent: Entity,
    local: Transform,
) -> Result<Entity, AssemblyError> {
    let root_prefab = registry
        .get(recipe.root_part)
        .ok_or(AssemblyError::MissingPart(recipe.root_part))?;

    let root = commands
        .spawn((
            Mesh3d(root_prefab.mesh.clone()),
            MeshMaterial3d(root_prefab.material.clone()),
            local,
            CharacterRoot,
            BodySlot(recipe.root_slot),
            SlotPart(recipe.root_part),
            CharacterAssembly {
                recipe: recipe.clone(),
            },
        ))
        .id();
    commands.entity(root).insert(CharacterMember(root));
    commands.entity(parent).add_child(root);

    attach_children(commands, registry, recipe, root)?;
    Ok(root)
}

/// Spawn every attachment of `recipe` under an already-existing `root` entity
/// (which must hold the root slot/part). Shared by initial spawn and swapping.
fn attach_children(
    commands: &mut Commands,
    registry: &PartRegistry,
    recipe: &CharacterRecipe,
    root: Entity,
) -> Result<(), AssemblyError> {
    let mut slot_entities: HashMap<&'static str, Entity> = HashMap::new();
    let mut slot_parts: HashMap<&'static str, &'static str> = HashMap::new();
    slot_entities.insert(recipe.root_slot, root);
    slot_parts.insert(recipe.root_slot, recipe.root_part);

    for att in &recipe.attachments {
        let parent_entity = *slot_entities
            .get(att.parent_slot)
            .ok_or(AssemblyError::UnknownParentSlot(att.parent_slot))?;
        let parent_part_id = *slot_parts
            .get(att.parent_slot)
            .ok_or(AssemblyError::UnknownParentSlot(att.parent_slot))?;

        let parent_prefab = registry
            .get(parent_part_id)
            .ok_or(AssemblyError::MissingPart(parent_part_id))?;
        let child_prefab = registry
            .get(att.child_part)
            .ok_or(AssemblyError::MissingPart(att.child_part))?;

        let p_sock =
            parent_prefab
                .socket(att.parent_socket)
                .ok_or(AssemblyError::MissingSocket {
                    part: parent_part_id,
                    socket: att.parent_socket,
                })?;
        let c_sock = child_prefab
            .socket(att.child_socket)
            .ok_or(AssemblyError::MissingSocket {
                part: att.child_part,
                socket: att.child_socket,
            })?;

        let local = align_socket(&p_sock.local, &c_sock.local);

        let child = commands
            .spawn((
                Mesh3d(child_prefab.mesh.clone()),
                MeshMaterial3d(child_prefab.material.clone()),
                local,
                BodySlot(att.child_slot),
                SlotPart(att.child_part),
                CharacterMember(root),
            ))
            .id();
        commands.entity(parent_entity).add_child(child);

        slot_entities.insert(att.child_slot, child);
        slot_parts.insert(att.child_slot, att.child_part);
    }

    Ok(())
}

// ── Baked single-mesh assembly (performance path) ──────────────────────────────

/// World/model matrix per slot, accumulated through the socket graph.
pub fn accumulate_part_matrices(
    registry: &PartRegistry,
    recipe: &CharacterRecipe,
) -> Result<HashMap<&'static str, Mat4>, AssemblyError> {
    // Validate the root part up front so an all-root recipe still errors.
    if registry.get(recipe.root_part).is_none() {
        return Err(AssemblyError::MissingPart(recipe.root_part));
    }

    let mut mats: HashMap<&'static str, Mat4> = HashMap::new();
    let mut slot_parts: HashMap<&'static str, &'static str> = HashMap::new();
    mats.insert(recipe.root_slot, Mat4::IDENTITY);
    slot_parts.insert(recipe.root_slot, recipe.root_part);

    for att in &recipe.attachments {
        let parent_mat = *mats
            .get(att.parent_slot)
            .ok_or(AssemblyError::UnknownParentSlot(att.parent_slot))?;
        let parent_part_id = slot_parts[att.parent_slot];

        let parent_prefab = registry
            .get(parent_part_id)
            .ok_or(AssemblyError::MissingPart(parent_part_id))?;
        let child_prefab = registry
            .get(att.child_part)
            .ok_or(AssemblyError::MissingPart(att.child_part))?;

        let p = parent_prefab
            .socket(att.parent_socket)
            .ok_or(AssemblyError::MissingSocket {
                part: parent_part_id,
                socket: att.parent_socket,
            })?
            .local;
        let c = child_prefab
            .socket(att.child_socket)
            .ok_or(AssemblyError::MissingSocket {
                part: att.child_part,
                socket: att.child_socket,
            })?
            .local;

        let local = p.to_matrix() * c.to_matrix().inverse();
        mats.insert(att.child_slot, parent_mat * local);
        slot_parts.insert(att.child_slot, att.child_part);
    }

    Ok(mats)
}

/// Merge multiple `(mesh, model_matrix)` pairs into a single [`Mesh`].
///
/// Inputs must be `TriangleList` with `POSITION`; `NORMAL`/`UV_0` are merged
/// when present. Normals use the inverse-transpose; a negative-determinant
/// model matrix (mirroring) flips triangle winding so faces stay outward.
pub fn bake_character_mesh(meshes: &Assets<Mesh>, parts: &[(Handle<Mesh>, Mat4)]) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for (handle, model) in parts {
        let Some(mesh) = meshes.get(handle) else {
            warn!("bake_character_mesh: mesh not loaded, skipping part");
            continue;
        };
        let base = positions.len() as u32;

        let normal_mat = Mat3::from_mat4(*model).inverse().transpose();
        let flipped = model.determinant() < 0.0;

        if let Some(VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            for p in pos {
                let v = model.transform_point3(Vec3::from(*p));
                positions.push(v.to_array());
            }
        }
        if let Some(VertexAttributeValues::Float32x3(norm)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        {
            for n in norm {
                let n = (normal_mat * Vec3::from(*n)).normalize_or_zero();
                normals.push(n.to_array());
            }
        }
        if let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            uvs.extend(uv.iter().copied());
        }

        match mesh.indices() {
            Some(Indices::U32(idx)) => {
                push_indices(&mut indices, idx.iter().map(|i| base + i), flipped)
            }
            Some(Indices::U16(idx)) => {
                push_indices(&mut indices, idx.iter().map(|&i| base + i as u32), flipped)
            }
            None => {
                let count = positions.len() as u32 - base;
                push_indices(&mut indices, base..base + count, flipped)
            }
        }
    }

    let mut out = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    out.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    if !normals.is_empty() {
        out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }
    if !uvs.is_empty() {
        out.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }
    out.insert_indices(Indices::U32(indices));
    out
}

fn push_indices(out: &mut Vec<u32>, iter: impl Iterator<Item = u32>, flip: bool) {
    let tri: Vec<u32> = iter.collect();
    for t in tri.chunks_exact(3) {
        if flip {
            out.extend_from_slice(&[t[0], t[2], t[1]]);
        } else {
            out.extend_from_slice(t);
        }
    }
}

/// Convenience: resolve a recipe and bake it into one mesh in a single call.
pub fn bake_recipe(
    meshes: &Assets<Mesh>,
    registry: &PartRegistry,
    recipe: &CharacterRecipe,
) -> Result<Mesh, AssemblyError> {
    let mats = accumulate_part_matrices(registry, recipe)?;

    let mut parts: Vec<(Handle<Mesh>, Mat4)> = Vec::new();
    let root_prefab = registry
        .get(recipe.root_part)
        .ok_or(AssemblyError::MissingPart(recipe.root_part))?;
    parts.push((root_prefab.mesh.clone(), mats[recipe.root_slot]));
    for att in &recipe.attachments {
        let prefab = registry
            .get(att.child_part)
            .ok_or(AssemblyError::MissingPart(att.child_part))?;
        parts.push((prefab.mesh.clone(), mats[att.child_slot]));
    }

    Ok(bake_character_mesh(meshes, &parts))
}

// ── glTF socket loading ────────────────────────────────────────────────────────

/// Build the socket list for a part from a loaded glTF's node graph. Empty
/// nodes whose name starts with `SOCKET_` become sockets; the prefix is
/// stripped to form the socket name (`SOCKET_elbow` → `elbow`).
///
/// Socket names are `&'static str` to match [`Socket`], so the trimmed names
/// are intentionally leaked. Call this once at load time, not per-frame.
pub fn sockets_from_gltf(
    gltf: &bevy::gltf::Gltf,
    nodes: &Assets<bevy::gltf::GltfNode>,
) -> Vec<Socket> {
    let mut sockets = Vec::new();
    for node_handle in &gltf.nodes {
        let Some(node) = nodes.get(node_handle) else {
            continue;
        };
        if let Some(socket_name) = node.name.strip_prefix("SOCKET_") {
            let leaked: &'static str = Box::leak(socket_name.to_owned().into_boxed_str());
            sockets.push(Socket {
                name: leaked,
                local: node.transform,
            });
        }
    }
    sockets
}

// ── Runtime systems ────────────────────────────────────────────────────────────

/// Apply [`SwapPart`] requests by rebuilding the affected character's parts.
pub fn handle_swap_part(
    mut commands: Commands,
    mut events: MessageReader<SwapPart>,
    registry: Res<PartRegistry>,
    mut roots: Query<(&mut CharacterAssembly, Option<&Children>)>,
) {
    for ev in events.read() {
        let Ok((mut assembly, children)) = roots.get_mut(ev.root) else {
            warn!("SwapPart: {:?} is not a character root", ev.root);
            continue;
        };

        let mut bound = false;
        for att in assembly.recipe.attachments.iter_mut() {
            if att.child_slot == ev.slot {
                att.child_part = ev.new_part;
                bound = true;
            }
        }
        if !bound {
            warn!("SwapPart: no slot '{}' on this character", ev.slot);
            continue;
        }

        // Despawn the existing part subtree (children despawn recursively), then
        // rebuild the whole hierarchy from the updated recipe.
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }

        let recipe = assembly.recipe.clone();
        if let Err(err) = attach_children(&mut commands, &registry, &recipe, ev.root) {
            error!("SwapPart rebuild failed: {err}");
        }
    }
}

/// Draw a small axis gizmo at every socket of every assembled part when the
/// [`DebugSockets`] flag is on.
pub fn draw_socket_gizmos(
    debug: Res<DebugSockets>,
    registry: Res<PartRegistry>,
    mut gizmos: Gizmos,
    parts: Query<(&GlobalTransform, &SlotPart)>,
) {
    if !debug.0 {
        return;
    }
    for (global, slot_part) in &parts {
        let Some(prefab) = registry.get(slot_part.0) else {
            continue;
        };
        let part_mat = global.to_matrix();
        for socket in &prefab.sockets {
            let world = part_mat * socket.local.to_matrix();
            let socket_tf = GlobalTransform::from(Transform::from_matrix(world));
            gizmos.axes(socket_tf, 0.12);
        }
    }
}

// ── Plugin ─────────────────────────────────────────────────────────────────────

/// Registers the modular character resources, the [`SwapPart`] message and the
/// swap/debug systems. Spawning characters is left to game code or the example.
pub struct ModularCharacterPlugin;

impl Plugin for ModularCharacterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartRegistry>()
            .init_resource::<DebugSockets>()
            .add_message::<SwapPart>()
            .add_systems(Update, (handle_swap_part, draw_socket_gizmos));
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_3;

    fn approx_eq_mat(a: Mat4, b: Mat4, eps: f32) -> bool {
        a.to_cols_array()
            .iter()
            .zip(b.to_cols_array().iter())
            .all(|(x, y)| (x - y).abs() <= eps)
    }

    #[test]
    fn align_socket_places_child_socket_onto_parent_socket() {
        // A parent socket with both translation and rotation.
        let parent_socket =
            Transform::from_xyz(-0.3, 0.4, 0.1).with_rotation(Quat::from_rotation_z(FRAC_PI_3));
        // A child socket authored elsewhere, also rotated.
        let child_socket =
            Transform::from_xyz(0.0, 0.2, 0.0).with_rotation(Quat::from_rotation_x(FRAC_PI_3));

        let child_transform = align_socket(&parent_socket, &child_socket);

        // The child socket, expressed in the parent's frame, must coincide with
        // the parent socket: T_child * T_child_socket ≈ T_parent_socket.
        let placed = child_transform.to_matrix() * child_socket.to_matrix();
        assert!(
            approx_eq_mat(placed, parent_socket.to_matrix(), 1e-4),
            "child socket did not land on parent socket"
        );
    }

    #[test]
    fn mirrored_socket_negates_x() {
        let s = Socket::new("shoulder", Transform::from_xyz(0.3, 0.4, 0.0));
        let m = mirrored_socket_x(&s);
        assert!((m.local.translation.x + 0.3).abs() < 1e-6);
        assert!((m.local.translation.y - 0.4).abs() < 1e-6);
    }

    #[test]
    fn bake_vertex_count_equals_sum_of_inputs() {
        let mut meshes = Assets::<Mesh>::default();
        let cuboid = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
        let sphere = meshes.add(Mesh::from(Sphere::new(0.5)));

        let count =
            |h: &Handle<Mesh>| match meshes.get(h).unwrap().attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(p)) => p.len(),
                _ => 0,
            };
        let expected = count(&cuboid) + count(&sphere);

        let parts = [
            (cuboid.clone(), Mat4::IDENTITY),
            (sphere.clone(), Mat4::from_translation(Vec3::X)),
        ];
        let baked = bake_character_mesh(&meshes, &parts);

        let baked_count = match baked.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => p.len(),
            _ => 0,
        };
        assert_eq!(baked_count, expected);
    }

    fn test_registry() -> PartRegistry {
        // Handles only need to be unique/non-null for graph math; the meshes
        // themselves are never dereferenced by accumulate_part_matrices.
        let mut meshes = Assets::<Mesh>::default();
        let mat = Handle::<StandardMaterial>::default();
        let mk = |id, mesh, sockets| PartPrefab {
            id,
            mesh,
            material: mat.clone(),
            sockets,
        };
        let mut reg = PartRegistry::default();
        reg.insert(mk(
            "torso",
            meshes.add(Mesh::from(Capsule3d::new(0.25, 0.5))),
            vec![Socket::new(
                "shoulder_l",
                Transform::from_xyz(-0.3, 0.4, 0.0),
            )],
        ));
        reg.insert(mk(
            "arm",
            meshes.add(Mesh::from(Capsule3d::new(0.07, 0.3))),
            vec![Socket::new(
                "shoulder_joint",
                Transform::from_xyz(0.0, 0.2, 0.0),
            )],
        ));
        reg
    }

    #[test]
    fn accumulate_matrices_positions_child_socket_at_parent_socket() {
        let reg = test_registry();
        let recipe = CharacterRecipe {
            root_slot: "torso",
            root_part: "torso",
            attachments: vec![Attachment {
                parent_slot: "torso",
                parent_socket: "shoulder_l",
                child_slot: "arm_l",
                child_part: "arm",
                child_socket: "shoulder_joint",
            }],
        };

        let mats = accumulate_part_matrices(&reg, &recipe).unwrap();
        let arm_mat = mats["arm_l"];
        // The arm's shoulder_joint in world space should sit at the torso's
        // shoulder_l (root is identity).
        let child_socket = reg.parts["arm"].socket("shoulder_joint").unwrap().local;
        let world_socket = arm_mat * child_socket.to_matrix();
        let parent_socket = reg.parts["torso"].socket("shoulder_l").unwrap().local;
        assert!(approx_eq_mat(world_socket, parent_socket.to_matrix(), 1e-4));
    }

    #[test]
    fn missing_part_is_reported() {
        let reg = test_registry();
        let recipe = CharacterRecipe {
            root_slot: "torso",
            root_part: "nope",
            attachments: vec![],
        };
        assert_eq!(
            accumulate_part_matrices(&reg, &recipe),
            Err(AssemblyError::MissingPart("nope"))
        );
    }
}
