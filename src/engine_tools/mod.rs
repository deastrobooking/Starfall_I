//! Shared foundations for Starfall's in-game creation tools.
//!
//! Gameplay generators remain recipe-first. This module supplies editor
//! services that can be reused by Character Studio, Creature Forge, World Kit
//! Forge, and the future Level Scene Composer without serializing transient ECS
//! entities or depending on Bevy's runtime `Entity` values.

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub struct EngineToolsPlugin;

impl Plugin for EngineToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<EngineToolMode>()
            .init_resource::<EditorSelection>()
            .init_resource::<EditorIdAllocator>()
            .init_resource::<EditorUndoStack>()
            .register_type::<EditorEntityId>();
    }
}

/// Independent of `AppState`: later workspaces can edit a loaded chapter while
/// retaining its game state. No global hotkey is installed until ET2 owns
/// camera, cursor, and gameplay-input capture.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineToolMode {
    #[default]
    Playing,
    Editing,
}

/// Persistent identity used by editor recipes and commands. Runtime Bevy
/// `Entity` values are intentionally never serialized as content references.
#[derive(
    Component,
    Reflect,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
)]
#[reflect(Component)]
pub struct EditorEntityId(pub u64);

#[derive(Resource, Debug, Clone)]
pub struct EditorIdAllocator {
    next: u64,
}

impl Default for EditorIdAllocator {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl EditorIdAllocator {
    pub fn allocate(&mut self) -> EditorEntityId {
        let id = EditorEntityId(self.next);
        self.next = self.next.saturating_add(1).max(1);
        id
    }

    /// Advance past an ID loaded from disk, avoiding collisions when new
    /// objects are placed into an authored scene.
    pub fn reserve(&mut self, id: EditorEntityId) {
        self.next = self.next.max(id.0.saturating_add(1)).max(1);
    }
}

/// Ordered selection makes outliner/property-panel output deterministic.
#[derive(Resource, Debug, Clone, Default)]
pub struct EditorSelection {
    selected: BTreeSet<EditorEntityId>,
    active: Option<EditorEntityId>,
}

impl EditorSelection {
    pub fn replace(&mut self, id: EditorEntityId) {
        self.selected.clear();
        self.selected.insert(id);
        self.active = Some(id);
    }

    pub fn toggle(&mut self, id: EditorEntityId) {
        if !self.selected.remove(&id) {
            self.selected.insert(id);
            self.active = Some(id);
        } else if self.active == Some(id) {
            self.active = self.selected.iter().next_back().copied();
        }
    }

    pub fn clear(&mut self) {
        self.selected.clear();
        self.active = None;
    }

    pub fn contains(&self, id: EditorEntityId) -> bool {
        self.selected.contains(&id)
    }

    pub fn active(&self) -> Option<EditorEntityId> {
        self.active
    }

    pub fn iter(&self) -> impl Iterator<Item = EditorEntityId> + '_ {
        self.selected.iter().copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorCommandError {
    MissingEntity(EditorEntityId),
    MissingTransform(EditorEntityId),
    EmptyTransaction,
}

/// First command family. Additional commands will target versioned recipes,
/// terrain layers, and spline points rather than arbitrary reflected memory.
#[derive(Debug, Clone)]
pub enum EditorCommand {
    SetTransform {
        id: EditorEntityId,
        before: Transform,
        after: Transform,
    },
}

impl EditorCommand {
    fn target(&self) -> EditorEntityId {
        match self {
            Self::SetTransform { id, .. } => *id,
        }
    }

    fn preflight(&self, world: &mut World) -> Result<(), EditorCommandError> {
        let id = self.target();
        let entity =
            resolve_editor_entity(world, id).ok_or(EditorCommandError::MissingEntity(id))?;
        world
            .get::<Transform>(entity)
            .map(|_| ())
            .ok_or(EditorCommandError::MissingTransform(id))
    }

    fn execute(&self, world: &mut World) -> Result<(), EditorCommandError> {
        match self {
            Self::SetTransform { id, after, .. } => set_transform(world, *id, *after),
        }
    }

    fn undo(&self, world: &mut World) -> Result<(), EditorCommandError> {
        match self {
            Self::SetTransform { id, before, .. } => set_transform(world, *id, *before),
        }
    }
}

/// One user gesture may edit many entities. Preflight makes the gesture atomic:
/// nothing is changed if any target is invalid.
#[derive(Debug, Clone)]
pub struct EditorTransaction {
    pub description: String,
    pub commands: Vec<EditorCommand>,
}

impl EditorTransaction {
    pub fn transform(
        description: impl Into<String>,
        id: EditorEntityId,
        before: Transform,
        after: Transform,
    ) -> Self {
        Self {
            description: description.into(),
            commands: vec![EditorCommand::SetTransform { id, before, after }],
        }
    }

    fn execute(&self, world: &mut World) -> Result<(), EditorCommandError> {
        if self.commands.is_empty() {
            return Err(EditorCommandError::EmptyTransaction);
        }
        for command in &self.commands {
            command.preflight(world)?;
        }
        for command in &self.commands {
            command.execute(world)?;
        }
        Ok(())
    }

    fn undo(&self, world: &mut World) -> Result<(), EditorCommandError> {
        for command in &self.commands {
            command.preflight(world)?;
        }
        for command in self.commands.iter().rev() {
            command.undo(world)?;
        }
        Ok(())
    }
}

#[derive(Resource, Debug)]
pub struct EditorUndoStack {
    undo: Vec<EditorTransaction>,
    redo: Vec<EditorTransaction>,
    max_depth: usize,
}

impl Default for EditorUndoStack {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_depth: 128,
        }
    }
}

impl EditorUndoStack {
    pub fn execute(
        &mut self,
        world: &mut World,
        transaction: EditorTransaction,
    ) -> Result<(), EditorCommandError> {
        transaction.execute(world)?;
        self.undo.push(transaction);
        self.redo.clear();
        if self.undo.len() > self.max_depth {
            self.undo.remove(0);
        }
        Ok(())
    }

    pub fn undo(&mut self, world: &mut World) -> Result<bool, EditorCommandError> {
        let Some(transaction) = self.undo.pop() else {
            return Ok(false);
        };
        if let Err(error) = transaction.undo(world) {
            self.undo.push(transaction);
            return Err(error);
        }
        self.redo.push(transaction);
        Ok(true)
    }

    pub fn redo(&mut self, world: &mut World) -> Result<bool, EditorCommandError> {
        let Some(transaction) = self.redo.pop() else {
            return Ok(false);
        };
        if let Err(error) = transaction.execute(world) {
            self.redo.push(transaction);
            return Err(error);
        }
        self.undo.push(transaction);
        Ok(true)
    }

    pub fn undo_description(&self) -> Option<&str> {
        self.undo.last().map(|entry| entry.description.as_str())
    }

    pub fn redo_description(&self) -> Option<&str> {
        self.redo.last().map(|entry| entry.description.as_str())
    }
}

fn resolve_editor_entity(world: &mut World, id: EditorEntityId) -> Option<Entity> {
    let mut query = world.query::<(Entity, &EditorEntityId)>();
    query
        .iter(world)
        .find_map(|(entity, candidate)| (*candidate == id).then_some(entity))
}

fn set_transform(
    world: &mut World,
    id: EditorEntityId,
    value: Transform,
) -> Result<(), EditorCommandError> {
    let entity = resolve_editor_entity(world, id).ok_or(EditorCommandError::MissingEntity(id))?;
    let mut transform = world
        .get_mut::<Transform>(entity)
        .ok_or(EditorCommandError::MissingTransform(id))?;
    *transform = value;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ids_allocate_and_reserve_without_collision() {
        let mut allocator = EditorIdAllocator::default();
        assert_eq!(allocator.allocate(), EditorEntityId(1));
        allocator.reserve(EditorEntityId(40));
        assert_eq!(allocator.allocate(), EditorEntityId(41));
    }

    #[test]
    fn selection_keeps_a_deterministic_active_entity() {
        let mut selection = EditorSelection::default();
        selection.toggle(EditorEntityId(9));
        selection.toggle(EditorEntityId(3));
        assert_eq!(
            selection.iter().collect::<Vec<_>>(),
            vec![EditorEntityId(3), EditorEntityId(9)]
        );
        assert_eq!(selection.active(), Some(EditorEntityId(3)));
        selection.toggle(EditorEntityId(3));
        assert_eq!(selection.active(), Some(EditorEntityId(9)));
    }

    #[test]
    fn transform_transaction_executes_undoes_and_redoes() {
        let mut world = World::new();
        let id = EditorEntityId(12);
        let entity = world.spawn((id, Transform::default())).id();
        let before = Transform::default();
        let after = Transform::from_xyz(4.0, 2.0, -7.0);
        let mut stack = EditorUndoStack::default();

        stack
            .execute(
                &mut world,
                EditorTransaction::transform("Move tower", id, before, after),
            )
            .unwrap();
        assert_eq!(
            world.get::<Transform>(entity).unwrap().translation,
            after.translation
        );
        assert!(stack.undo(&mut world).unwrap());
        assert_eq!(
            world.get::<Transform>(entity).unwrap().translation,
            before.translation
        );
        assert!(stack.redo(&mut world).unwrap());
        assert_eq!(
            world.get::<Transform>(entity).unwrap().translation,
            after.translation
        );
    }

    #[test]
    fn transaction_preflight_prevents_partial_multi_entity_edits() {
        let mut world = World::new();
        let valid = EditorEntityId(1);
        let entity = world.spawn((valid, Transform::default())).id();
        let mut stack = EditorUndoStack::default();
        let transaction = EditorTransaction {
            description: "Move selection".into(),
            commands: vec![
                EditorCommand::SetTransform {
                    id: valid,
                    before: Transform::default(),
                    after: Transform::from_xyz(8.0, 0.0, 0.0),
                },
                EditorCommand::SetTransform {
                    id: EditorEntityId(999),
                    before: Transform::default(),
                    after: Transform::from_xyz(2.0, 0.0, 0.0),
                },
            ],
        };

        assert_eq!(
            stack.execute(&mut world, transaction),
            Err(EditorCommandError::MissingEntity(EditorEntityId(999)))
        );
        assert_eq!(
            world.get::<Transform>(entity).unwrap().translation,
            Vec3::ZERO
        );
    }
}
