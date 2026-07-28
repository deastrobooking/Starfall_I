//! Authoritative, renderer-independent triangle topology for mesh editing.
//!
//! Bevy [`Mesh`] values are import and render boundaries. Editing code uses
//! generational element IDs and explicit half-edge adjacency so selections and
//! future topology commands do not depend on transient vertex-buffer offsets.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};

use bevy::math::DVec3;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::Mesh;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawMeshId {
    pub slot: u32,
    pub generation: u32,
}

macro_rules! mesh_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(RawMeshId);

        impl $name {
            pub fn raw(self) -> RawMeshId {
                self.0
            }
        }

        impl From<RawMeshId> for $name {
            fn from(value: RawMeshId) -> Self {
                Self(value)
            }
        }
    };
}

mesh_id!(VertexId);
mesh_id!(EdgeId);
mesh_id!(HalfEdgeId);
mesh_id!(FaceId);

#[derive(Debug, Clone)]
struct ArenaSlot<T> {
    generation: u32,
    value: Option<T>,
}

#[derive(Debug, Clone)]
struct GenerationalArena<T> {
    slots: Vec<ArenaSlot<T>>,
    free: Vec<u32>,
}

impl<T> Default for GenerationalArena<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }
}

impl<T> GenerationalArena<T> {
    fn insert(&mut self, value: T) -> RawMeshId {
        if let Some(slot) = self.free.pop() {
            let entry = &mut self.slots[slot as usize];
            entry.value = Some(value);
            RawMeshId {
                slot,
                generation: entry.generation,
            }
        } else {
            let slot = self.slots.len() as u32;
            self.slots.push(ArenaSlot {
                generation: 0,
                value: Some(value),
            });
            RawMeshId {
                slot,
                generation: 0,
            }
        }
    }

    fn get(&self, id: RawMeshId) -> Option<&T> {
        let slot = self.slots.get(id.slot as usize)?;
        (slot.generation == id.generation)
            .then_some(slot.value.as_ref())
            .flatten()
    }

    fn get_mut(&mut self, id: RawMeshId) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.slot as usize)?;
        (slot.generation == id.generation)
            .then_some(slot.value.as_mut())
            .flatten()
    }

    fn remove(&mut self, id: RawMeshId) -> Option<T> {
        let slot = self.slots.get_mut(id.slot as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take()?;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.slot);
        Some(value)
    }

    fn active_ids(&self) -> Vec<RawMeshId> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| {
                entry.value.as_ref().map(|_| RawMeshId {
                    slot: slot as u32,
                    generation: entry.generation,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct EditableVertex {
    pub position: DVec3,
    pub outgoing: Option<HalfEdgeId>,
    pub source_vertex: u32,
    pub semantic_region: u32,
}

#[derive(Debug, Clone)]
pub struct EditableHalfEdge {
    pub origin: VertexId,
    pub twin: Option<HalfEdgeId>,
    pub next: HalfEdgeId,
    pub prev: HalfEdgeId,
    pub edge: EdgeId,
    pub face: FaceId,
}

#[derive(Debug, Clone)]
pub struct EditableEdge {
    pub halfedge: HalfEdgeId,
    pub crease_weight: f64,
}

#[derive(Debug, Clone)]
pub struct EditableFace {
    pub halfedge: HalfEdgeId,
    pub material_slot: u32,
    pub semantic_region: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MeshRevisions {
    pub topology: u64,
    pub positions: u64,
    pub corner_attributes: u64,
    pub materials: u64,
    pub skinning: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CategoricalTransferPolicy {
    #[default]
    KeepStart,
    KeepEnd,
    RequireEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CreaseTransferPolicy {
    #[default]
    Preserve,
    Reset,
}

/// Transfer rules for the categorical and edge attributes currently owned by
/// the topology kernel. Corner UVs, colors, and skin weights will extend this
/// contract when those stores migrate off render buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttributeTransferPolicy {
    pub semantic_region: CategoricalTransferPolicy,
    pub crease: CreaseTransferPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CollapseTarget {
    KeepStart,
    KeepEnd,
    Midpoint,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeshCommand {
    Split {
        edge: EdgeId,
        t: f64,
    },
    Flip {
        edge: EdgeId,
    },
    Collapse {
        edge: EdgeId,
        target: CollapseTarget,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditableMeshError {
    NotTriangleList,
    MissingPositions,
    UnsupportedPositions,
    MalformedIndexBuffer,
    EmptySurface,
    VertexOutOfBounds(u32),
    NonFiniteVertex(u32),
    RepeatedFaceVertex(u32),
    DegenerateFace(u32),
    DuplicateDirectedEdge(u32, u32),
    NonManifoldEdge(u32, u32),
    UnresolvedElement(&'static str),
    BrokenFaceLoop(FaceId),
    BrokenTwin(HalfEdgeId),
    InvalidParameter(&'static str),
    BoundaryEdge(EdgeId),
    ExistingDiagonal(VertexId, VertexId),
    LinkConditionFailed(EdgeId),
    FaceInversion(FaceId),
    AttributeConflict(&'static str),
    HistoryDiverged,
}

impl fmt::Display for EditableMeshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for EditableMeshError {}

#[derive(Debug, Clone)]
pub struct BakedEditableGeometry {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub triangle_faces: Vec<FaceId>,
    pub canonical_vertices: Vec<VertexId>,
    pub source_vertices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct MeshDelta {
    before: Box<EditableMeshDocument>,
    after: Box<EditableMeshDocument>,
    pub command: MeshCommand,
    pub created_vertices: Vec<VertexId>,
    pub removed_vertices: Vec<VertexId>,
}

impl MeshDelta {
    pub fn undo(&self, document: &mut EditableMeshDocument) -> Result<(), EditableMeshError> {
        if document.state_fingerprint()? != self.after.state_fingerprint()? {
            return Err(EditableMeshError::HistoryDiverged);
        }
        *document = (*self.before).clone();
        Ok(())
    }

    pub fn redo(&self, document: &mut EditableMeshDocument) -> Result<(), EditableMeshError> {
        if document.state_fingerprint()? != self.before.state_fingerprint()? {
            return Err(EditableMeshError::HistoryDiverged);
        }
        *document = (*self.after).clone();
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MeshCommandHistory {
    undo: Vec<MeshDelta>,
    redo: Vec<MeshDelta>,
    max_depth: usize,
}

impl Default for MeshCommandHistory {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_depth: 128,
        }
    }
}

impl MeshCommandHistory {
    pub fn execute(
        &mut self,
        document: &mut EditableMeshDocument,
        command: MeshCommand,
        policy: AttributeTransferPolicy,
    ) -> Result<(), EditableMeshError> {
        let delta = document.apply_command(command, policy)?;
        self.undo.push(delta);
        self.redo.clear();
        if self.undo.len() > self.max_depth {
            self.undo.remove(0);
        }
        Ok(())
    }

    pub fn undo(&mut self, document: &mut EditableMeshDocument) -> Result<bool, EditableMeshError> {
        let Some(delta) = self.undo.pop() else {
            return Ok(false);
        };
        if let Err(error) = delta.undo(document) {
            self.undo.push(delta);
            return Err(error);
        }
        self.redo.push(delta);
        Ok(true)
    }

    pub fn redo(&mut self, document: &mut EditableMeshDocument) -> Result<bool, EditableMeshError> {
        let Some(delta) = self.redo.pop() else {
            return Ok(false);
        };
        if let Err(error) = delta.redo(document) {
            self.redo.push(delta);
            return Err(error);
        }
        self.undo.push(delta);
        Ok(true)
    }
}

#[derive(Debug, Clone, Copy)]
struct FaceSpec {
    id: Option<FaceId>,
    vertices: [VertexId; 3],
    material_slot: u32,
    semantic_region: u32,
}

#[derive(Debug, Clone)]
pub struct EditableMeshDocument {
    vertices: GenerationalArena<EditableVertex>,
    halfedges: GenerationalArena<EditableHalfEdge>,
    edges: GenerationalArena<EditableEdge>,
    faces: GenerationalArena<EditableFace>,
    vertex_order: Vec<VertexId>,
    edge_order: Vec<EdgeId>,
    halfedge_order: Vec<HalfEdgeId>,
    face_order: Vec<FaceId>,
    revisions: MeshRevisions,
}

impl EditableMeshDocument {
    pub fn from_bevy_mesh(mesh: &Mesh) -> Result<Self, EditableMeshError> {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
            return Err(EditableMeshError::NotTriangleList);
        }
        let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(values)) => values.as_slice(),
            Some(_) => return Err(EditableMeshError::UnsupportedPositions),
            None => return Err(EditableMeshError::MissingPositions),
        };
        let indices = match mesh.indices() {
            Some(Indices::U16(values)) => values
                .iter()
                .map(|index| u32::from(*index))
                .collect::<Vec<_>>(),
            Some(Indices::U32(values)) => values.clone(),
            None => (0..positions.len() as u32).collect(),
        };
        if !indices.len().is_multiple_of(3) {
            return Err(EditableMeshError::MalformedIndexBuffer);
        }
        let triangles = indices
            .chunks_exact(3)
            .map(|face| [face[0], face[1], face[2]])
            .collect::<Vec<_>>();
        Self::from_triangles(positions, &triangles)
    }

    pub fn from_triangles(
        positions: &[[f32; 3]],
        triangles: &[[u32; 3]],
    ) -> Result<Self, EditableMeshError> {
        if triangles.is_empty() {
            return Err(EditableMeshError::EmptySurface);
        }

        let mut document = Self {
            vertices: Default::default(),
            halfedges: Default::default(),
            edges: Default::default(),
            faces: Default::default(),
            vertex_order: Vec::with_capacity(positions.len()),
            edge_order: Vec::new(),
            halfedge_order: Vec::with_capacity(triangles.len() * 3),
            face_order: Vec::with_capacity(triangles.len()),
            revisions: MeshRevisions {
                topology: 1,
                positions: 1,
                ..Default::default()
            },
        };
        for (source_vertex, position) in positions.iter().enumerate() {
            let position = DVec3::from_array(position.map(f64::from));
            if !position.is_finite() {
                return Err(EditableMeshError::NonFiniteVertex(source_vertex as u32));
            }
            let id = VertexId::from(document.vertices.insert(EditableVertex {
                position,
                outgoing: None,
                source_vertex: source_vertex as u32,
                semantic_region: 0,
            }));
            document.vertex_order.push(id);
        }

        let mut directed = HashMap::<(u32, u32), HalfEdgeId>::new();
        let mut undirected = HashMap::<(u32, u32), EdgeId>::new();
        for (face_index, triangle) in triangles.iter().copied().enumerate() {
            let [a, b, c] = triangle;
            if [a, b, c]
                .into_iter()
                .any(|index| index as usize >= document.vertex_order.len())
            {
                return Err(EditableMeshError::VertexOutOfBounds(
                    [a, b, c]
                        .into_iter()
                        .find(|index| *index as usize >= document.vertex_order.len())
                        .unwrap_or(a),
                ));
            }
            if a == b || b == c || c == a {
                return Err(EditableMeshError::RepeatedFaceVertex(face_index as u32));
            }
            let [pa, pb, pc] = [a, b, c].map(|index| {
                document
                    .vertex(document.vertex_order[index as usize])
                    .expect("validated vertex")
                    .position
            });
            if (pb - pa).cross(pc - pa).length_squared() <= 1.0e-24 {
                return Err(EditableMeshError::DegenerateFace(face_index as u32));
            }

            let face_id = FaceId::from(document.faces.insert(EditableFace {
                halfedge: HalfEdgeId::from(RawMeshId {
                    slot: u32::MAX,
                    generation: u32::MAX,
                }),
                material_slot: 0,
                semantic_region: 0,
            }));
            let vertex_ids = [a, b, c].map(|index| document.vertex_order[index as usize]);
            let placeholder_edge = EdgeId::from(RawMeshId {
                slot: u32::MAX,
                generation: u32::MAX,
            });
            let placeholder_halfedge = HalfEdgeId::from(RawMeshId {
                slot: u32::MAX,
                generation: u32::MAX,
            });
            let halfedge_ids = vertex_ids.map(|origin| {
                let id = HalfEdgeId::from(document.halfedges.insert(EditableHalfEdge {
                    origin,
                    twin: None,
                    next: placeholder_halfedge,
                    prev: placeholder_halfedge,
                    edge: placeholder_edge,
                    face: face_id,
                }));
                document.halfedge_order.push(id);
                id
            });
            for corner in 0..3 {
                let halfedge = document
                    .halfedges
                    .get_mut(halfedge_ids[corner].raw())
                    .expect("new half-edge");
                halfedge.next = halfedge_ids[(corner + 1) % 3];
                halfedge.prev = halfedge_ids[(corner + 2) % 3];
                let vertex = document
                    .vertices
                    .get_mut(vertex_ids[corner].raw())
                    .expect("validated vertex");
                vertex.outgoing.get_or_insert(halfedge_ids[corner]);
            }
            document
                .faces
                .get_mut(face_id.raw())
                .expect("new face")
                .halfedge = halfedge_ids[0];
            document.face_order.push(face_id);

            for corner in 0..3 {
                let from = triangle[corner];
                let to = triangle[(corner + 1) % 3];
                let key = (from.min(to), from.max(to));
                if let Some(edge_id) = undirected.get(&key).copied() {
                    let primary = document.edge(edge_id)?.halfedge;
                    if document.halfedge(primary)?.twin.is_some() {
                        return Err(EditableMeshError::NonManifoldEdge(key.0, key.1));
                    }
                }
                if directed.contains_key(&(from, to)) {
                    return Err(EditableMeshError::DuplicateDirectedEdge(from, to));
                }
                let halfedge_id = halfedge_ids[corner];
                let edge_id = if let Some(edge_id) = undirected.get(&key).copied() {
                    let primary = document.edge(edge_id)?.halfedge;
                    if document.halfedge(primary)?.twin.is_some() {
                        return Err(EditableMeshError::NonManifoldEdge(key.0, key.1));
                    }
                    let Some(twin) = directed.get(&(to, from)).copied() else {
                        return Err(EditableMeshError::DuplicateDirectedEdge(from, to));
                    };
                    document
                        .halfedges
                        .get_mut(halfedge_id.raw())
                        .expect("new half-edge")
                        .twin = Some(twin);
                    document
                        .halfedges
                        .get_mut(twin.raw())
                        .expect("existing twin")
                        .twin = Some(halfedge_id);
                    edge_id
                } else {
                    let edge_id = EdgeId::from(document.edges.insert(EditableEdge {
                        halfedge: halfedge_id,
                        crease_weight: 0.0,
                    }));
                    document.edge_order.push(edge_id);
                    undirected.insert(key, edge_id);
                    edge_id
                };
                document
                    .halfedges
                    .get_mut(halfedge_id.raw())
                    .expect("new half-edge")
                    .edge = edge_id;
                directed.insert((from, to), halfedge_id);
            }
        }
        document.validate()?;
        Ok(document)
    }

    pub fn revisions(&self) -> MeshRevisions {
        self.revisions
    }

    pub fn vertex(&self, id: VertexId) -> Result<&EditableVertex, EditableMeshError> {
        self.vertices
            .get(id.raw())
            .ok_or(EditableMeshError::UnresolvedElement("vertex"))
    }

    pub fn edge(&self, id: EdgeId) -> Result<&EditableEdge, EditableMeshError> {
        self.edges
            .get(id.raw())
            .ok_or(EditableMeshError::UnresolvedElement("edge"))
    }

    pub fn halfedge(&self, id: HalfEdgeId) -> Result<&EditableHalfEdge, EditableMeshError> {
        self.halfedges
            .get(id.raw())
            .ok_or(EditableMeshError::UnresolvedElement("half-edge"))
    }

    pub fn face(&self, id: FaceId) -> Result<&EditableFace, EditableMeshError> {
        self.faces
            .get(id.raw())
            .ok_or(EditableMeshError::UnresolvedElement("face"))
    }

    pub fn face_ids(&self) -> &[FaceId] {
        &self.face_order
    }

    pub fn vertex_ids(&self) -> &[VertexId] {
        &self.vertex_order
    }

    pub fn edge_ids(&self) -> &[EdgeId] {
        &self.edge_order
    }

    pub fn edge_vertices(&self, edge: EdgeId) -> Result<[VertexId; 2], EditableMeshError> {
        let halfedge = self.edge(edge)?.halfedge;
        Ok([
            self.halfedge(halfedge)?.origin,
            self.halfedge(self.halfedge(halfedge)?.next)?.origin,
        ])
    }

    pub fn find_edge(&self, a: VertexId, b: VertexId) -> Option<EdgeId> {
        self.edge_order.iter().copied().find(|edge| {
            self.edge_vertices(*edge)
                .is_ok_and(|vertices| vertices == [a, b] || vertices == [b, a])
        })
    }

    pub fn face_vertices(&self, face: FaceId) -> Result<[VertexId; 3], EditableMeshError> {
        let first = self.face(face)?.halfedge;
        let second = self.halfedge(first)?.next;
        let third = self.halfedge(second)?.next;
        if self.halfedge(third)?.next != first {
            return Err(EditableMeshError::BrokenFaceLoop(face));
        }
        Ok([
            self.halfedge(first)?.origin,
            self.halfedge(second)?.origin,
            self.halfedge(third)?.origin,
        ])
    }

    /// Applies one position transaction. Invalid geometry is rolled back and
    /// the position revision advances only after validation succeeds.
    pub fn set_vertex_position(
        &mut self,
        vertex: VertexId,
        position: DVec3,
    ) -> Result<(), EditableMeshError> {
        if !position.is_finite() {
            return Err(EditableMeshError::NonFiniteVertex(vertex.raw().slot));
        }
        let previous = self.vertex(vertex)?.position;
        self.vertices
            .get_mut(vertex.raw())
            .ok_or(EditableMeshError::UnresolvedElement("vertex"))?
            .position = position;
        if let Err(error) = self.validate() {
            self.vertices
                .get_mut(vertex.raw())
                .expect("transaction vertex remains allocated")
                .position = previous;
            return Err(error);
        }
        self.revisions.positions = self.revisions.positions.saturating_add(1);
        Ok(())
    }

    pub fn apply_command(
        &mut self,
        command: MeshCommand,
        policy: AttributeTransferPolicy,
    ) -> Result<MeshDelta, EditableMeshError> {
        let before = self.clone();
        let mut candidate = self.clone();
        let (created_vertices, removed_vertices, positions_changed) = match command {
            MeshCommand::Split { edge, t } => {
                let created = candidate.split_edge(edge, t, policy)?;
                (vec![created], Vec::new(), true)
            }
            MeshCommand::Flip { edge } => {
                candidate.flip_edge(edge)?;
                (Vec::new(), Vec::new(), false)
            }
            MeshCommand::Collapse { edge, target } => {
                let removed = candidate.collapse_edge(edge, target, policy)?;
                (Vec::new(), vec![removed], true)
            }
        };
        candidate.validate()?;
        candidate.revisions.topology = candidate.revisions.topology.saturating_add(1);
        candidate.revisions.corner_attributes =
            candidate.revisions.corner_attributes.saturating_add(1);
        if positions_changed {
            candidate.revisions.positions = candidate.revisions.positions.saturating_add(1);
            candidate.revisions.skinning = candidate.revisions.skinning.saturating_add(1);
        }
        let after = candidate.clone();
        *self = candidate;
        Ok(MeshDelta {
            before: Box::new(before),
            after: Box::new(after),
            command,
            created_vertices,
            removed_vertices,
        })
    }

    fn split_edge(
        &mut self,
        edge: EdgeId,
        t: f64,
        policy: AttributeTransferPolicy,
    ) -> Result<VertexId, EditableMeshError> {
        if !t.is_finite() || !(0.0..1.0).contains(&t) {
            return Err(EditableMeshError::InvalidParameter(
                "edge split t must be finite and strictly between zero and one",
            ));
        }
        let [start, end] = self.edge_vertices(edge)?;
        let start_vertex = self.vertex(start)?.clone();
        let end_vertex = self.vertex(end)?.clone();
        let semantic_region = transfer_category(
            start_vertex.semantic_region,
            end_vertex.semantic_region,
            policy.semantic_region,
            "vertex semantic region",
        )?;
        let new_vertex = VertexId::from(self.vertices.insert(EditableVertex {
            position: start_vertex.position.lerp(end_vertex.position, t),
            outgoing: None,
            source_vertex: u32::MAX,
            semantic_region,
        }));
        self.vertex_order.push(new_vertex);

        let old_crease = self.edge(edge)?.crease_weight;
        let mut specs = Vec::with_capacity(self.face_order.len() + 2);
        for face in self.face_specs()? {
            if let Some((from, to, opposite)) = oriented_edge_in_triangle(face.vertices, start, end)
            {
                specs.push(FaceSpec {
                    id: face.id,
                    vertices: [from, new_vertex, opposite],
                    ..face
                });
                specs.push(FaceSpec {
                    id: None,
                    vertices: [new_vertex, to, opposite],
                    ..face
                });
            } else {
                specs.push(face);
            }
        }
        self.rebuild_topology(specs)?;
        let inherited_crease = match policy.crease {
            CreaseTransferPolicy::Preserve => old_crease,
            CreaseTransferPolicy::Reset => 0.0,
        };
        for endpoint in [start, end] {
            let child = self
                .find_edge(endpoint, new_vertex)
                .ok_or(EditableMeshError::UnresolvedElement("split child edge"))?;
            self.edges
                .get_mut(child.raw())
                .expect("resolved split child edge")
                .crease_weight = inherited_crease;
        }
        Ok(new_vertex)
    }

    fn flip_edge(&mut self, edge: EdgeId) -> Result<(), EditableMeshError> {
        let primary = self.edge(edge)?.halfedge;
        let twin = self
            .halfedge(primary)?
            .twin
            .ok_or(EditableMeshError::BoundaryEdge(edge))?;
        let start = self.halfedge(primary)?.origin;
        let end = self.halfedge(self.halfedge(primary)?.next)?.origin;
        let opposite_primary = self.halfedge(self.halfedge(primary)?.prev)?.origin;
        let opposite_twin = self.halfedge(self.halfedge(twin)?.prev)?.origin;
        if self
            .find_edge(opposite_primary, opposite_twin)
            .is_some_and(|candidate| candidate != edge)
        {
            return Err(EditableMeshError::ExistingDiagonal(
                opposite_primary,
                opposite_twin,
            ));
        }
        let primary_face = self.halfedge(primary)?.face;
        let twin_face = self.halfedge(twin)?.face;
        let mut specs = self.face_specs()?;
        for face in &mut specs {
            if face.id == Some(primary_face) {
                let replacement = [opposite_primary, opposite_twin, end];
                self.ensure_orientation_preserved(primary_face, face.vertices, replacement)?;
                face.vertices = replacement;
            } else if face.id == Some(twin_face) {
                let replacement = [opposite_twin, opposite_primary, start];
                self.ensure_orientation_preserved(twin_face, face.vertices, replacement)?;
                face.vertices = replacement;
            }
        }
        self.rebuild_topology(specs)
    }

    fn collapse_edge(
        &mut self,
        edge: EdgeId,
        target: CollapseTarget,
        policy: AttributeTransferPolicy,
    ) -> Result<VertexId, EditableMeshError> {
        self.validate_collapse_link(edge)?;
        let [start, end] = self.edge_vertices(edge)?;
        let (keep, remove, position) = match target {
            CollapseTarget::KeepStart => (start, end, self.vertex(start)?.position),
            CollapseTarget::KeepEnd => (end, start, self.vertex(end)?.position),
            CollapseTarget::Midpoint => (
                start,
                end,
                (self.vertex(start)?.position + self.vertex(end)?.position) * 0.5,
            ),
        };
        let start_region = self.vertex(start)?.semantic_region;
        let end_region = self.vertex(end)?.semantic_region;
        let semantic_region = transfer_category(
            start_region,
            end_region,
            policy.semantic_region,
            "vertex semantic region",
        )?;
        let original_specs = self.face_specs()?;
        let original_normals = original_specs
            .iter()
            .map(|face| {
                Ok((
                    face.id.expect("document face spec has an ID"),
                    self.triangle_normal(face.vertices)?,
                ))
            })
            .collect::<Result<HashMap<_, _>, EditableMeshError>>()?;
        self.vertices
            .get_mut(keep.raw())
            .ok_or(EditableMeshError::UnresolvedElement("collapse target"))?
            .position = position;
        self.vertices
            .get_mut(keep.raw())
            .expect("resolved collapse target")
            .semantic_region = semantic_region;
        let specs = original_specs
            .into_iter()
            .map(|mut face| {
                let contains_keep = face.vertices.contains(&keep);
                let contains_remove = face.vertices.contains(&remove);
                if contains_keep && contains_remove {
                    return Ok(None);
                }
                for vertex in &mut face.vertices {
                    if *vertex == remove {
                        *vertex = keep;
                    }
                }
                if contains_remove {
                    let face_id = face.id.expect("document face spec has an ID");
                    let replacement_normal = self.triangle_normal(face.vertices)?;
                    if replacement_normal.length_squared() <= 1.0e-24
                        || original_normals[&face_id].dot(replacement_normal) <= 0.0
                    {
                        return Err(EditableMeshError::FaceInversion(face_id));
                    }
                }
                Ok(Some(face))
            })
            .collect::<Result<Vec<_>, EditableMeshError>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if specs.is_empty() {
            return Err(EditableMeshError::EmptySurface);
        }
        self.vertex_order.retain(|vertex| *vertex != remove);
        self.vertices
            .remove(remove.raw())
            .ok_or(EditableMeshError::UnresolvedElement("collapsed vertex"))?;
        self.rebuild_topology(specs)?;
        Ok(remove)
    }

    fn validate_collapse_link(&self, edge: EdgeId) -> Result<(), EditableMeshError> {
        let [start, end] = self.edge_vertices(edge)?;
        let neighbors = |vertex| -> Result<BTreeSet<VertexId>, EditableMeshError> {
            let mut result = BTreeSet::new();
            for face in &self.face_order {
                let vertices = self.face_vertices(*face)?;
                if vertices.contains(&vertex) {
                    result.extend(vertices.into_iter().filter(|item| *item != vertex));
                }
            }
            Ok(result)
        };
        let start_neighbors = neighbors(start)?;
        let end_neighbors = neighbors(end)?;
        let shared = start_neighbors
            .intersection(&end_neighbors)
            .copied()
            .collect::<BTreeSet<_>>();
        let mut opposite = BTreeSet::new();
        for face in &self.face_order {
            let vertices = self.face_vertices(*face)?;
            if vertices.contains(&start) && vertices.contains(&end) {
                opposite.extend(
                    vertices
                        .into_iter()
                        .filter(|vertex| *vertex != start && *vertex != end),
                );
            }
        }
        if shared != opposite || opposite.is_empty() || opposite.len() > 2 {
            return Err(EditableMeshError::LinkConditionFailed(edge));
        }
        Ok(())
    }

    fn face_specs(&self) -> Result<Vec<FaceSpec>, EditableMeshError> {
        self.face_order
            .iter()
            .map(|face_id| {
                let face = self.face(*face_id)?;
                Ok(FaceSpec {
                    id: Some(*face_id),
                    vertices: self.face_vertices(*face_id)?,
                    material_slot: face.material_slot,
                    semantic_region: face.semantic_region,
                })
            })
            .collect()
    }

    fn ensure_orientation_preserved(
        &self,
        face: FaceId,
        previous: [VertexId; 3],
        replacement: [VertexId; 3],
    ) -> Result<(), EditableMeshError> {
        let previous_normal = self.triangle_normal(previous)?;
        let replacement_normal = self.triangle_normal(replacement)?;
        if replacement_normal.length_squared() <= 1.0e-24
            || previous_normal.dot(replacement_normal) <= 0.0
        {
            return Err(EditableMeshError::FaceInversion(face));
        }
        Ok(())
    }

    fn triangle_normal(&self, vertices: [VertexId; 3]) -> Result<DVec3, EditableMeshError> {
        let [a, b, c] = [
            self.vertex(vertices[0])?.position,
            self.vertex(vertices[1])?.position,
            self.vertex(vertices[2])?.position,
        ];
        Ok((b - a).cross(c - a))
    }

    fn rebuild_topology(&mut self, specs: Vec<FaceSpec>) -> Result<(), EditableMeshError> {
        let old_edges = self
            .edge_order
            .iter()
            .map(|edge| Ok((ordered_pair(self.edge_vertices(*edge)?), *edge)))
            .collect::<Result<HashMap<_, _>, EditableMeshError>>()?;
        for halfedge in self.halfedges.active_ids() {
            self.halfedges.remove(halfedge);
        }
        self.halfedge_order.clear();
        for vertex in &self.vertex_order {
            self.vertices
                .get_mut(vertex.raw())
                .ok_or(EditableMeshError::UnresolvedElement("topology vertex"))?
                .outgoing = None;
        }

        let retained_faces = specs
            .iter()
            .filter_map(|face| face.id)
            .collect::<HashSet<_>>();
        for face in self.face_order.clone() {
            if !retained_faces.contains(&face) {
                self.faces.remove(face.raw());
            }
        }
        self.face_order.clear();

        let placeholder_edge = EdgeId::from(RawMeshId {
            slot: u32::MAX,
            generation: u32::MAX,
        });
        let placeholder_halfedge = HalfEdgeId::from(RawMeshId {
            slot: u32::MAX,
            generation: u32::MAX,
        });
        let mut directed = HashMap::<(VertexId, VertexId), HalfEdgeId>::new();
        let mut used_edges = HashSet::<EdgeId>::new();
        for spec in specs {
            for vertex in spec.vertices {
                self.vertex(vertex)?;
            }
            let face_id = if let Some(face_id) = spec.id {
                face_id
            } else {
                FaceId::from(self.faces.insert(EditableFace {
                    halfedge: placeholder_halfedge,
                    material_slot: spec.material_slot,
                    semantic_region: spec.semantic_region,
                }))
            };
            let halfedges = spec.vertices.map(|origin| {
                let id = HalfEdgeId::from(self.halfedges.insert(EditableHalfEdge {
                    origin,
                    twin: None,
                    next: placeholder_halfedge,
                    prev: placeholder_halfedge,
                    edge: placeholder_edge,
                    face: face_id,
                }));
                self.halfedge_order.push(id);
                id
            });
            for corner in 0..3 {
                let from = spec.vertices[corner];
                let to = spec.vertices[(corner + 1) % 3];
                if directed.contains_key(&(from, to)) {
                    return Err(EditableMeshError::DuplicateDirectedEdge(
                        from.raw().slot,
                        to.raw().slot,
                    ));
                }
                let halfedge = self
                    .halfedges
                    .get_mut(halfedges[corner].raw())
                    .expect("new rebuilt half-edge");
                halfedge.next = halfedges[(corner + 1) % 3];
                halfedge.prev = halfedges[(corner + 2) % 3];
                self.vertices
                    .get_mut(from.raw())
                    .expect("validated rebuilt vertex")
                    .outgoing
                    .get_or_insert(halfedges[corner]);
                directed.insert((from, to), halfedges[corner]);
            }
            self.faces
                .get_mut(face_id.raw())
                .ok_or(EditableMeshError::UnresolvedElement("rebuilt face"))?
                .halfedge = halfedges[0];
            let face = self
                .faces
                .get_mut(face_id.raw())
                .expect("resolved rebuilt face");
            face.material_slot = spec.material_slot;
            face.semantic_region = spec.semantic_region;
            self.face_order.push(face_id);
        }

        let mut edge_for_pair = HashMap::<(VertexId, VertexId), EdgeId>::new();
        for halfedge_id in self.halfedge_order.clone() {
            let from = self.halfedge(halfedge_id)?.origin;
            let to = self.halfedge(self.halfedge(halfedge_id)?.next)?.origin;
            let pair = ordered_pair([from, to]);
            let edge = if let Some(edge) = edge_for_pair.get(&pair).copied() {
                let primary = self.edge(edge)?.halfedge;
                if self.halfedge(primary)?.twin.is_some() {
                    return Err(EditableMeshError::NonManifoldEdge(
                        pair.0.raw().slot,
                        pair.1.raw().slot,
                    ));
                }
                let twin = directed.get(&(to, from)).copied().ok_or(
                    EditableMeshError::DuplicateDirectedEdge(from.raw().slot, to.raw().slot),
                )?;
                self.halfedges
                    .get_mut(halfedge_id.raw())
                    .expect("rebuilt half-edge")
                    .twin = Some(twin);
                self.halfedges
                    .get_mut(twin.raw())
                    .expect("rebuilt twin")
                    .twin = Some(halfedge_id);
                edge
            } else {
                let edge = if let Some(edge) = old_edges.get(&pair).copied() {
                    self.edges
                        .get_mut(edge.raw())
                        .ok_or(EditableMeshError::UnresolvedElement("preserved edge"))?
                        .halfedge = halfedge_id;
                    edge
                } else {
                    EdgeId::from(self.edges.insert(EditableEdge {
                        halfedge: halfedge_id,
                        crease_weight: 0.0,
                    }))
                };
                edge_for_pair.insert(pair, edge);
                edge
            };
            used_edges.insert(edge);
            self.halfedges
                .get_mut(halfedge_id.raw())
                .expect("rebuilt half-edge")
                .edge = edge;
        }
        for edge in self.edge_order.clone() {
            if !used_edges.contains(&edge) {
                self.edges.remove(edge.raw());
            }
        }
        self.edge_order = edge_for_pair.values().copied().collect();
        self.edge_order.sort_unstable();
        self.validate()
    }

    pub fn validate(&self) -> Result<(), EditableMeshError> {
        for vertex in &self.vertex_order {
            let value = self.vertex(*vertex)?;
            if !value.position.is_finite() {
                return Err(EditableMeshError::NonFiniteVertex(value.source_vertex));
            }
        }
        for face in &self.face_order {
            let [a, b, c] = self.face_vertices(*face)?;
            let [pa, pb, pc] = [
                self.vertex(a)?.position,
                self.vertex(b)?.position,
                self.vertex(c)?.position,
            ];
            if (pb - pa).cross(pc - pa).length_squared() <= 1.0e-24 {
                return Err(EditableMeshError::DegenerateFace(face.raw().slot));
            }
            let first = self.face(*face)?.halfedge;
            let mut current = first;
            for _ in 0..3 {
                let halfedge = self.halfedge(current)?;
                if halfedge.face != *face || self.halfedge(halfedge.next)?.prev != current {
                    return Err(EditableMeshError::BrokenFaceLoop(*face));
                }
                self.edge(halfedge.edge)?;
                if let Some(twin) = halfedge.twin {
                    let twin_value = self.halfedge(twin)?;
                    if twin_value.twin != Some(current)
                        || self.halfedge(twin_value.next)?.origin != halfedge.origin
                    {
                        return Err(EditableMeshError::BrokenTwin(current));
                    }
                }
                current = halfedge.next;
            }
            if current != first {
                return Err(EditableMeshError::BrokenFaceLoop(*face));
            }
        }
        Ok(())
    }

    pub fn bake_geometry(&self) -> Result<BakedEditableGeometry, EditableMeshError> {
        let mut dense_vertices = HashMap::<VertexId, u32>::new();
        let mut positions = Vec::with_capacity(self.vertex_order.len());
        let mut source_vertices = Vec::with_capacity(self.vertex_order.len());
        for vertex_id in &self.vertex_order {
            let vertex = self.vertex(*vertex_id)?;
            dense_vertices.insert(*vertex_id, positions.len() as u32);
            positions.push(vertex.position.as_vec3().to_array());
            source_vertices.push(vertex.source_vertex);
        }
        let mut indices = Vec::with_capacity(self.face_order.len() * 3);
        for face in &self.face_order {
            for vertex in self.face_vertices(*face)? {
                indices.push(
                    *dense_vertices
                        .get(&vertex)
                        .ok_or(EditableMeshError::UnresolvedElement("bake vertex"))?,
                );
            }
        }
        Ok(BakedEditableGeometry {
            positions,
            indices,
            triangle_faces: self.face_order.clone(),
            canonical_vertices: self.vertex_order.clone(),
            source_vertices,
        })
    }

    fn state_fingerprint(&self) -> Result<u64, EditableMeshError> {
        let mut hasher = DefaultHasher::new();
        self.revisions.topology.hash(&mut hasher);
        self.revisions.positions.hash(&mut hasher);
        self.revisions.corner_attributes.hash(&mut hasher);
        self.revisions.materials.hash(&mut hasher);
        self.revisions.skinning.hash(&mut hasher);
        for vertex_id in &self.vertex_order {
            vertex_id.hash(&mut hasher);
            let vertex = self.vertex(*vertex_id)?;
            vertex.position.x.to_bits().hash(&mut hasher);
            vertex.position.y.to_bits().hash(&mut hasher);
            vertex.position.z.to_bits().hash(&mut hasher);
            vertex.semantic_region.hash(&mut hasher);
        }
        for face_id in &self.face_order {
            face_id.hash(&mut hasher);
            self.face_vertices(*face_id)?.hash(&mut hasher);
            let face = self.face(*face_id)?;
            face.material_slot.hash(&mut hasher);
            face.semantic_region.hash(&mut hasher);
        }
        Ok(hasher.finish())
    }
}

fn ordered_pair<T: Ord + Copy>(pair: [T; 2]) -> (T, T) {
    if pair[0] <= pair[1] {
        (pair[0], pair[1])
    } else {
        (pair[1], pair[0])
    }
}

fn oriented_edge_in_triangle(
    triangle: [VertexId; 3],
    a: VertexId,
    b: VertexId,
) -> Option<(VertexId, VertexId, VertexId)> {
    for corner in 0..3 {
        let from = triangle[corner];
        let to = triangle[(corner + 1) % 3];
        if (from == a && to == b) || (from == b && to == a) {
            return Some((from, to, triangle[(corner + 2) % 3]));
        }
    }
    None
}

fn transfer_category(
    start: u32,
    end: u32,
    policy: CategoricalTransferPolicy,
    label: &'static str,
) -> Result<u32, EditableMeshError> {
    match policy {
        CategoricalTransferPolicy::KeepStart => Ok(start),
        CategoricalTransferPolicy::KeepEnd => Ok(end),
        CategoricalTransferPolicy::RequireEqual if start == end => Ok(start),
        CategoricalTransferPolicy::RequireEqual => Err(EditableMeshError::AttributeConflict(label)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Cuboid;

    #[test]
    fn stale_generational_ids_never_resolve_after_slot_reuse() {
        let mut arena = GenerationalArena::default();
        let first = arena.insert("first");
        assert_eq!(arena.remove(first), Some("first"));
        let replacement = arena.insert("replacement");
        assert_eq!(first.slot, replacement.slot);
        assert_ne!(first.generation, replacement.generation);
        assert!(arena.get(first).is_none());
        assert_eq!(arena.get(replacement), Some(&"replacement"));
    }

    #[test]
    fn cube_builds_valid_halfedge_topology_and_face_mapping() {
        let mesh = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let document = EditableMeshDocument::from_bevy_mesh(&mesh).unwrap();
        document.validate().unwrap();
        let baked = document.bake_geometry().unwrap();
        assert_eq!(baked.triangle_faces.len(), 12);
        assert_eq!(baked.indices.len(), 36);
        assert_eq!(baked.positions.len(), mesh.count_vertices());
    }

    #[test]
    fn invalid_triangle_inputs_are_rejected_before_becoming_documents() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        assert!(matches!(
            EditableMeshDocument::from_triangles(&positions, &[[0, 1, 2]]),
            Err(EditableMeshError::DegenerateFace(0))
        ));
        assert!(matches!(
            EditableMeshDocument::from_triangles(&positions, &[[0, 1, 9]]),
            Err(EditableMeshError::VertexOutOfBounds(9))
        ));
    }

    #[test]
    fn duplicate_and_nonmanifold_edges_are_rejected() {
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        assert!(matches!(
            EditableMeshDocument::from_triangles(&positions, &[[0, 1, 2], [0, 1, 3]]),
            Err(EditableMeshError::DuplicateDirectedEdge(0, 1))
        ));
        assert!(matches!(
            EditableMeshDocument::from_triangles(&positions, &[[0, 1, 2], [1, 0, 3], [0, 1, 4]]),
            Err(EditableMeshError::NonManifoldEdge(0, 1))
        ));
    }

    #[test]
    fn failed_position_transaction_rolls_back_without_advancing_revision() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let mut document = EditableMeshDocument::from_triangles(&positions, &[[0, 1, 2]]).unwrap();
        let vertex = document.vertex_order[2];
        let before = document.revisions();
        let original = document.vertex(vertex).unwrap().position;
        assert_eq!(
            document.set_vertex_position(vertex, DVec3::new(0.5, 0.0, 0.0)),
            Err(EditableMeshError::DegenerateFace(0))
        );
        assert_eq!(document.vertex(vertex).unwrap().position, original);
        assert_eq!(document.revisions(), before);
    }

    fn quad_document() -> EditableMeshDocument {
        EditableMeshDocument::from_triangles(
            &[
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            &[[0, 1, 2], [0, 2, 3]],
        )
        .unwrap()
    }

    #[test]
    fn split_edge_preserves_old_elements_and_creates_valid_children() {
        let mut document = quad_document();
        let vertices = document.vertex_ids().to_vec();
        let diagonal = document.find_edge(vertices[0], vertices[2]).unwrap();
        let [split_start, split_end] = document.edge_vertices(diagonal).unwrap();
        let expected_position = document
            .vertex(split_start)
            .unwrap()
            .position
            .lerp(document.vertex(split_end).unwrap().position, 0.25);
        document
            .edges
            .get_mut(diagonal.raw())
            .unwrap()
            .crease_weight = 0.75;
        let before = document.revisions();
        let delta = document
            .apply_command(
                MeshCommand::Split {
                    edge: diagonal,
                    t: 0.25,
                },
                AttributeTransferPolicy::default(),
            )
            .unwrap();
        let created = delta.created_vertices[0];
        assert_eq!(document.vertex_ids().len(), 5);
        assert_eq!(document.face_ids().len(), 4);
        assert_eq!(
            document.vertex(created).unwrap().position,
            expected_position
        );
        assert_eq!(
            document
                .edge(document.find_edge(vertices[0], created).unwrap())
                .unwrap()
                .crease_weight,
            0.75
        );
        assert_eq!(document.vertex(vertices[1]).unwrap().source_vertex, 1);
        assert_eq!(document.revisions().topology, before.topology + 1);
        assert!(document.edge(diagonal).is_err());
        document.validate().unwrap();
    }

    #[test]
    fn flip_edge_replaces_only_the_diagonal_and_preserves_face_ids() {
        let mut document = quad_document();
        let vertices = document.vertex_ids().to_vec();
        let faces = document.face_ids().to_vec();
        let diagonal = document.find_edge(vertices[0], vertices[2]).unwrap();
        document
            .apply_command(
                MeshCommand::Flip { edge: diagonal },
                AttributeTransferPolicy::default(),
            )
            .unwrap();
        assert!(document.find_edge(vertices[0], vertices[2]).is_none());
        assert!(document.find_edge(vertices[1], vertices[3]).is_some());
        assert_eq!(document.face_ids(), faces);
        assert_eq!(document.vertex_ids(), vertices);
        document.validate().unwrap();
    }

    #[test]
    fn boundary_collapse_obeys_link_condition_and_invalidates_removed_vertex() {
        let mut document = quad_document();
        let vertices = document.vertex_ids().to_vec();
        let boundary = document.find_edge(vertices[0], vertices[1]).unwrap();
        let delta = document
            .apply_command(
                MeshCommand::Collapse {
                    edge: boundary,
                    target: CollapseTarget::KeepStart,
                },
                AttributeTransferPolicy::default(),
            )
            .unwrap();
        assert_eq!(delta.removed_vertices, [vertices[1]]);
        assert!(document.vertex(vertices[1]).is_err());
        assert_eq!(document.vertex_ids().len(), 3);
        assert_eq!(document.face_ids().len(), 1);
        document.validate().unwrap();
    }

    #[test]
    fn command_history_undoes_and_redoes_exact_topology_snapshots() {
        let mut document = quad_document();
        let original = document.bake_geometry().unwrap();
        let vertices = document.vertex_ids().to_vec();
        let diagonal = document.find_edge(vertices[0], vertices[2]).unwrap();
        let mut history = MeshCommandHistory::default();
        history
            .execute(
                &mut document,
                MeshCommand::Split {
                    edge: diagonal,
                    t: 0.5,
                },
                AttributeTransferPolicy::default(),
            )
            .unwrap();
        let split = document.bake_geometry().unwrap();
        assert!(history.undo(&mut document).unwrap());
        assert_eq!(document.bake_geometry().unwrap().indices, original.indices);
        assert!(history.redo(&mut document).unwrap());
        assert_eq!(document.bake_geometry().unwrap().indices, split.indices);
        assert!(!history.redo(&mut document).unwrap());
    }

    #[test]
    fn failed_commands_leave_document_and_revisions_untouched() {
        let mut document = quad_document();
        let before = document.bake_geometry().unwrap();
        let revisions = document.revisions();
        let boundary = document.edge_ids().iter().copied().find(|edge| {
            let halfedge = document.edge(*edge).unwrap().halfedge;
            document.halfedge(halfedge).unwrap().twin.is_none()
        });
        assert!(matches!(
            document.apply_command(
                MeshCommand::Flip {
                    edge: boundary.unwrap()
                },
                AttributeTransferPolicy::default()
            ),
            Err(EditableMeshError::BoundaryEdge(_))
        ));
        assert_eq!(document.revisions(), revisions);
        assert_eq!(document.bake_geometry().unwrap().indices, before.indices);
    }

    #[test]
    fn categorical_policy_can_reject_ambiguous_semantic_splits() {
        let mut document = quad_document();
        let vertices = document.vertex_ids().to_vec();
        document
            .vertices
            .get_mut(vertices[0].raw())
            .unwrap()
            .semantic_region = 10;
        document
            .vertices
            .get_mut(vertices[2].raw())
            .unwrap()
            .semantic_region = 20;
        let diagonal = document.find_edge(vertices[0], vertices[2]).unwrap();
        let before = document.state_fingerprint().unwrap();
        assert!(matches!(
            document.apply_command(
                MeshCommand::Split {
                    edge: diagonal,
                    t: 0.5,
                },
                AttributeTransferPolicy {
                    semantic_region: CategoricalTransferPolicy::RequireEqual,
                    ..Default::default()
                },
            ),
            Err(EditableMeshError::AttributeConflict(
                "vertex semantic region"
            ))
        ));
        assert_eq!(document.state_fingerprint().unwrap(), before);
    }

    #[test]
    fn history_rejects_a_same_revision_but_different_document() {
        let mut expected = quad_document();
        let expected_vertices = expected.vertex_ids().to_vec();
        let expected_diagonal = expected
            .find_edge(expected_vertices[0], expected_vertices[2])
            .unwrap();
        let delta = expected
            .apply_command(
                MeshCommand::Split {
                    edge: expected_diagonal,
                    t: 0.25,
                },
                AttributeTransferPolicy::default(),
            )
            .unwrap();

        let mut different = quad_document();
        let different_vertices = different.vertex_ids().to_vec();
        let different_diagonal = different
            .find_edge(different_vertices[0], different_vertices[2])
            .unwrap();
        different
            .apply_command(
                MeshCommand::Split {
                    edge: different_diagonal,
                    t: 0.75,
                },
                AttributeTransferPolicy::default(),
            )
            .unwrap();
        assert_eq!(different.revisions(), expected.revisions());
        assert_eq!(
            delta.undo(&mut different),
            Err(EditableMeshError::HistoryDiverged)
        );
    }
}
