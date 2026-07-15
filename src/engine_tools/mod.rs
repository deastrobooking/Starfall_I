//! Shared foundations for Starfall's in-game creation tools.
//!
//! Gameplay generators remain recipe-first. This module supplies editor
//! services that can be reused by Character Studio, Creature Forge, World Kit
//! Forge, and the future Level Scene Composer without serializing transient ECS
//! entities or depending on Bevy's runtime `Entity` values.

mod persistence;

use std::collections::BTreeSet;

use bevy::ecs::system::SystemParam;
use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::input::keyboard::Key;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::time::Virtual;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow, Window};
use serde::{Deserialize, Serialize};

use crate::components::player::{Player, PlayerCamera, PlayerInput};
use crate::components::world::{
    DungeonCrawlGate, EnterableBuilding, SettlementBuildTerminal, SpeedLoopGuide, WorldAnchor,
    WorldRouteMarker,
};
use crate::physics::prelude::{Physics, PhysicsTime};
use crate::plugins::input_plugin::{NativeButton, NativeControllerState};
use crate::rendering::{
    EnergyMaterial, EnergyMaterialUniform, IceMaterial, IceMaterialUniform, LavaMaterial,
    LavaMaterialUniform, ShieldMaterial, ShieldMaterialUniform, ToonMaterial, ToonMaterialUniform,
    WaterMaterial, WaterMaterialUniform,
};
use crate::state::AppState;
use persistence::{
    validate_project, AdapterOverrideDraft, DraftPrimitive, EditorSceneDraft, ForgeProject,
    ProjectLoadSource, ProjectStore, SceneObjectDraft, TransformDraft,
};

#[derive(SystemParam)]
struct ForgeMaterialAssets<'w> {
    toon: ResMut<'w, Assets<ToonMaterial>>,
    water: ResMut<'w, Assets<WaterMaterial>>,
    energy: ResMut<'w, Assets<EnergyMaterial>>,
    shield: ResMut<'w, Assets<ShieldMaterial>>,
    ice: ResMut<'w, Assets<IceMaterial>>,
    lava: ResMut<'w, Assets<LavaMaterial>>,
}

pub struct EngineToolsPlugin;

impl Plugin for EngineToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<EngineToolMode>()
            .init_resource::<EditorSelection>()
            .init_resource::<EditorIdAllocator>()
            .init_resource::<EditorUndoStack>()
            .init_resource::<EditorFocus>()
            .init_resource::<EditorPendingActions>()
            .init_resource::<EditorRuntimeStatus>()
            .init_resource::<EditorDocumentState>()
            .init_resource::<EditorOutlinerFilter>()
            .init_resource::<EditorCameraRig>()
            .init_resource::<EditorGizmoSettings>()
            .init_resource::<EditorDragState>()
            .init_resource::<EditorPendingTransactions>()
            .init_resource::<EditorProjectSession>()
            .init_resource::<EditorRegistryState>()
            .init_resource::<EditorTextInputCapture>()
            .register_type::<EditorEntityId>()
            .add_systems(
                Update,
                toggle_editor_workspace.run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                OnEnter(EngineToolMode::Editing),
                (enter_editor_workspace, adapt_world_authorables).chain(),
            )
            .add_systems(OnExit(EngineToolMode::Editing), exit_editor_workspace)
            .add_systems(
                Update,
                (
                    adapt_world_authorables,
                    editor_button_interactions,
                    editor_search_input,
                    editor_controller_navigation,
                    editor_gizmo_drag,
                    editor_viewport_picking,
                    apply_editor_actions,
                    sync_world_adapters,
                    update_editor_workspace_text,
                    update_editor_button_style,
                    draw_editor_gizmos,
                    editor_camera_controls,
                )
                    .chain()
                    .run_if(in_state(EngineToolMode::Editing)),
            );
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

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
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

// ── ET2: safe editor workspace shell ─────────────────────────────────────────

#[derive(Component)]
pub struct Authorable;

#[derive(Component, Debug, Clone, Copy)]
struct AuthorableBounds(f32);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum EditorPrimitive {
    Empty,
    Cube,
    Pillar,
    Beacon,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum EditorAccess {
    Editable,
    ReadOnly,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum EditorAdapterKind {
    WorldAnchor,
    CaveGate,
    SettlementTerminal,
    RouteMarker,
    RoadLoop,
    Building,
}

#[derive(Component, Debug, Clone, Copy)]
struct EditorWorldAdapter {
    kind: EditorAdapterKind,
    last_translation: Vec3,
}

#[derive(Component)]
struct EditorWorkspaceRoot;

#[derive(Component)]
struct EditorCamera;

#[derive(Component)]
struct EditorSummaryText;

#[derive(Component)]
struct EditorStatusText;

#[derive(Component)]
struct EditorSearchText;

#[derive(Component)]
struct EditorRegistryText;

#[derive(Component, Debug, Clone, Copy)]
struct EditorButton {
    order: usize,
    action: EditorAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorAction {
    Exit,
    Undo,
    Redo,
    SelectPrevious,
    SelectNext,
    FocusSearch,
    CreateAnchor,
    CreateCube,
    CreatePillar,
    CreateBeacon,
    Duplicate,
    Delete,
    MoveXNegative,
    MoveXPositive,
    MoveYNegative,
    MoveYPositive,
    MoveZNegative,
    MoveZPositive,
    RotateYNegative,
    RotateYPositive,
    ScaleDown,
    ScaleUp,
    FrameSelection,
    ToggleCameraMode,
    GizmoTranslate,
    GizmoRotate,
    GizmoScale,
    ToggleTransformSpace,
    CycleSnap,
    SaveProject,
    PublishProject,
    LoadProject,
    RecoverProject,
    RegistryPrevious,
    RegistryNext,
    RegistryRename,
    ValidateProject,
    CreateMaterialRecord,
    DuplicateRecord,
    DeleteRecord,
    NormalizeSourcePath,
    SearchRegistry,
}

#[derive(Resource, Default)]
struct EditorFocus(usize);

#[derive(Resource, Default)]
struct EditorPendingActions(Vec<EditorAction>);

#[derive(Resource)]
struct EditorRuntimeStatus {
    message: String,
}

#[derive(Resource, Default)]
struct EditorDocumentState {
    dirty: bool,
    exit_armed: bool,
}

#[derive(Resource, Default)]
struct EditorOutlinerFilter {
    active: bool,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorCameraMode {
    Fly,
    Orbit,
}

#[derive(Resource)]
struct EditorCameraRig {
    mode: EditorCameraMode,
    orbit_target: Vec3,
    orbit_distance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorGizmoMode {
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorTransformSpace {
    World,
    Local,
}

#[derive(Resource)]
struct EditorGizmoSettings {
    mode: EditorGizmoMode,
    space: EditorTransformSpace,
    snap_index: usize,
}

impl Default for EditorGizmoSettings {
    fn default() -> Self {
        Self {
            mode: EditorGizmoMode::Translate,
            space: EditorTransformSpace::World,
            snap_index: 1,
        }
    }
}

impl EditorGizmoSettings {
    const TRANSLATION_SNAPS: [f32; 4] = [0.25, 0.5, 1.0, 2.0];

    fn translation_snap(&self) -> f32 {
        Self::TRANSLATION_SNAPS[self.snap_index % Self::TRANSLATION_SNAPS.len()]
    }
}

#[derive(Debug, Clone)]
struct ActiveEditorDrag {
    axis: Vec3,
    axis_index: usize,
    start_cursor: Vec2,
    before: Vec<(EditorEntityId, Transform)>,
}

#[derive(Resource, Default)]
struct EditorDragState(Option<ActiveEditorDrag>);

#[derive(Resource, Default)]
struct EditorPendingTransactions(Vec<EditorTransaction>);

#[derive(Resource, Debug)]
struct EditorProjectSession {
    project: ForgeProject,
    store: ProjectStore,
}

#[derive(Resource, Default)]
struct EditorRegistryState {
    selected: usize,
    rename_active: bool,
    rename_buffer: String,
    search_active: bool,
    search_text: String,
}

#[derive(Resource, Default)]
struct EditorTextInputCapture(bool);

impl Default for EditorProjectSession {
    fn default() -> Self {
        Self {
            project: ForgeProject::default(),
            store: ProjectStore::default_workspace(),
        }
    }
}

impl Default for EditorCameraRig {
    fn default() -> Self {
        Self {
            mode: EditorCameraMode::Fly,
            orbit_target: Vec3::ZERO,
            orbit_distance: 12.0,
        }
    }
}

impl Default for EditorRuntimeStatus {
    fn default() -> Self {
        Self {
            message: "Workspace ready — create or select an authorable object".into(),
        }
    }
}

fn toggle_editor_workspace(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    mode: Res<State<EngineToolMode>>,
    mut next_mode: ResMut<NextState<EngineToolMode>>,
    mut pending: ResMut<EditorPendingActions>,
) {
    let controller_chord = gamepads.iter().any(|gamepad| {
        gamepad.pressed(GamepadButton::Select) && gamepad.just_pressed(GamepadButton::Start)
    }) || (native.pressed(NativeButton::Select)
        && native.just_pressed(NativeButton::Start));

    if keyboard.just_pressed(KeyCode::Tab) || controller_chord {
        match mode.get() {
            EngineToolMode::Playing => next_mode.set(EngineToolMode::Editing),
            EngineToolMode::Editing => pending.0.push(EditorAction::Exit),
        }
    }
}

fn enter_editor_workspace(
    mut commands: Commands,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut physics_time: ResMut<Time<Physics>>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut player_cameras: Query<(&Transform, &mut Camera), With<PlayerCamera>>,
    mut focus: ResMut<EditorFocus>,
    mut status: ResMut<EditorRuntimeStatus>,
    mut document: ResMut<EditorDocumentState>,
    mut filter: ResMut<EditorOutlinerFilter>,
    mut camera_rig: ResMut<EditorCameraRig>,
    mut registry: ResMut<EditorRegistryState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut material_assets: ForgeMaterialAssets,
) {
    virtual_time.pause();
    physics_time.pause();
    focus.0 = 0;
    document.exit_armed = false;
    filter.active = false;
    registry.rename_active = false;
    registry.search_active = false;
    status.message = "Simulation paused; gameplay input captured by editor".into();

    if let Ok(mut cursor) = cursors.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }

    let mut camera_transform =
        Transform::from_xyz(0.0, 12.0, 24.0).looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::Y);
    for (transform, mut camera) in &mut player_cameras {
        if camera.is_active {
            camera_transform = *transform;
        }
        camera.is_active = false;
    }
    camera_rig.mode = EditorCameraMode::Fly;
    camera_rig.orbit_target = camera_transform.translation + camera_transform.forward() * 12.0;
    camera_rig.orbit_distance = 12.0;

    commands.spawn((
        EditorCamera,
        Camera3d::default(),
        Camera {
            order: 100,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            far: 30_000.0,
            ..default()
        }),
        camera_transform,
    ));

    let preview_center = camera_transform.translation + camera_transform.forward() * 5.0;
    let preview_right = Vec3::from(camera_transform.right());
    commands.spawn((
        EditorWorkspaceRoot,
        Name::new("Forge Toon Material Preview"),
        Mesh3d(meshes.add(Sphere::new(0.75))),
        MeshMaterial3d(material_assets.toon.add(ToonMaterial {
            settings: ToonMaterialUniform::default(),
        })),
        Transform::from_translation(preview_center - preview_right * 1.8),
    ));
    commands.spawn((
        EditorWorkspaceRoot,
        Name::new("Forge Water Material Preview"),
        Mesh3d(meshes.add(Cuboid::new(1.4, 0.12, 1.4))),
        MeshMaterial3d(material_assets.water.add(WaterMaterial {
            settings: WaterMaterialUniform::default(),
        })),
        Transform::from_translation(preview_center + Vec3::Y * 0.05),
    ));
    commands.spawn((
        EditorWorkspaceRoot,
        Name::new("Forge Energy Material Preview"),
        Mesh3d(meshes.add(Sphere::new(0.62))),
        MeshMaterial3d(material_assets.energy.add(EnergyMaterial {
            settings: EnergyMaterialUniform::default(),
        })),
        Transform::from_translation(preview_center + preview_right * 1.8),
    ));
    commands.spawn((
        EditorWorkspaceRoot,
        Name::new("Forge Shield Material Preview"),
        Mesh3d(meshes.add(Sphere::new(0.82))),
        MeshMaterial3d(material_assets.shield.add(ShieldMaterial {
            settings: ShieldMaterialUniform::default(),
        })),
        Transform::from_translation(preview_center + preview_right * 3.5),
    ));
    commands.spawn((
        EditorWorkspaceRoot,
        Name::new("Forge Ice Material Preview"),
        Mesh3d(meshes.add(Sphere::new(0.72))),
        MeshMaterial3d(material_assets.ice.add(IceMaterial {
            settings: IceMaterialUniform::default(),
        })),
        Transform::from_translation(preview_center - preview_right * 1.8 + Vec3::Y * 1.9),
    ));
    commands.spawn((
        EditorWorkspaceRoot,
        Name::new("Forge Lava Material Preview"),
        Mesh3d(meshes.add(Cuboid::new(1.4, 0.18, 1.4))),
        MeshMaterial3d(material_assets.lava.add(LavaMaterial {
            settings: LavaMaterialUniform::default(),
        })),
        Transform::from_translation(preview_center + Vec3::Y * 1.9),
    ));

    spawn_editor_workspace_ui(&mut commands);
}

fn exit_editor_workspace(
    mut commands: Commands,
    roots: Query<Entity, With<EditorWorkspaceRoot>>,
    editor_cameras: Query<Entity, With<EditorCamera>>,
    mut player_cameras: Query<&mut Camera, With<PlayerCamera>>,
    mut players: Query<&mut PlayerInput, With<Player>>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut physics_time: ResMut<Time<Physics>>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    for entity in roots.iter().chain(editor_cameras.iter()) {
        commands.entity(entity).despawn();
    }
    for mut camera in &mut player_cameras {
        camera.is_active = true;
    }
    for mut input in &mut players {
        *input = PlayerInput::default();
    }
    virtual_time.unpause();
    physics_time.unpause();
    if let Ok(mut cursor) = cursors.single_mut() {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

#[allow(clippy::type_complexity)]
fn adapt_world_authorables(
    mut commands: Commands,
    mut allocator: ResMut<EditorIdAllocator>,
    candidates: Query<
        (
            Entity,
            &Transform,
            Option<&Name>,
            Option<&WorldAnchor>,
            Option<&DungeonCrawlGate>,
            Option<&SettlementBuildTerminal>,
            Option<&WorldRouteMarker>,
            Option<&SpeedLoopGuide>,
            Option<&EnterableBuilding>,
        ),
        (
            Without<EditorEntityId>,
            Or<(
                With<WorldAnchor>,
                With<DungeonCrawlGate>,
                With<SettlementBuildTerminal>,
                With<WorldRouteMarker>,
                With<SpeedLoopGuide>,
                With<EnterableBuilding>,
            )>,
        ),
    >,
) {
    for (entity, transform, name, anchor, gate, terminal, route, road_loop, building) in &candidates
    {
        let (kind, access, radius, generated_name) = if let Some(anchor) = anchor {
            (
                EditorAdapterKind::WorldAnchor,
                EditorAccess::Editable,
                2.5,
                format!("Anchor • {}", anchor.id),
            )
        } else if let Some(gate) = gate {
            (
                EditorAdapterKind::CaveGate,
                EditorAccess::Editable,
                4.0,
                format!("Cave • {}", gate.label),
            )
        } else if let Some(terminal) = terminal {
            (
                EditorAdapterKind::SettlementTerminal,
                EditorAccess::Editable,
                2.5,
                format!("Settlement • {}", terminal.settlement_name),
            )
        } else if let Some(route) = route {
            (
                EditorAdapterKind::RouteMarker,
                EditorAccess::Editable,
                3.0,
                format!("Route Marker • {}", route.id.0),
            )
        } else if let Some(road_loop) = road_loop {
            (
                EditorAdapterKind::RoadLoop,
                EditorAccess::ReadOnly,
                road_loop.radius.max(3.0),
                "Road Loop • generated recipe".into(),
            )
        } else if building.is_some() {
            (
                EditorAdapterKind::Building,
                EditorAccess::ReadOnly,
                8.0,
                "Building • generated recipe".into(),
            )
        } else {
            continue;
        };
        let id = allocator.allocate();
        let mut entity_commands = commands.entity(entity);
        entity_commands.insert((
            Authorable,
            access,
            AuthorableBounds(radius),
            EditorWorldAdapter {
                kind,
                last_translation: transform.translation,
            },
            id,
        ));
        if name.is_none() {
            entity_commands.insert(Name::new(generated_name));
        }
    }
}

fn sync_world_adapters(
    mut adapters: Query<
        (
            &Transform,
            &mut EditorWorldAdapter,
            Option<&mut DungeonCrawlGate>,
            Option<&mut SettlementBuildTerminal>,
        ),
        Changed<Transform>,
    >,
) {
    for (transform, mut adapter, gate, terminal) in &mut adapters {
        let delta = transform.translation - adapter.last_translation;
        if delta == Vec3::ZERO {
            continue;
        }
        if let Some(mut gate) = gate {
            gate.entry += delta;
            gate.focus += delta;
        }
        if let Some(mut terminal) = terminal {
            terminal.origin += delta;
        }
        adapter.last_translation = transform.translation;
    }
}

fn spawn_editor_workspace_ui(commands: &mut Commands) {
    commands
        .spawn((
            EditorWorkspaceRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            GlobalZIndex(5_000),
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    min_height: Val::Px(58.0),
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(18.0),
                    row_gap: Val::Px(6.0),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.035, 0.065, 0.96)),
                BorderColor::all(Color::srgb(0.18, 0.78, 0.95)),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Text::new("STARFALL FORGE  •  LEVEL WORKSPACE"),
                    TextFont {
                        font_size: FontSize::Px(20.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.35, 0.92, 1.0)),
                ));
                spawn_editor_button(bar, 0, EditorAction::Exit, "EXIT  [Tab]");
                spawn_editor_button(bar, 29, EditorAction::SaveProject, "SAVE  [⌘S]");
                spawn_editor_button(bar, 30, EditorAction::LoadProject, "LOAD");
                spawn_editor_button(bar, 31, EditorAction::RecoverProject, "RECOVER");
                spawn_editor_button(bar, 32, EditorAction::PublishProject, "PUBLISH");
                spawn_editor_button(bar, 1, EditorAction::Undo, "UNDO");
                spawn_editor_button(bar, 2, EditorAction::Redo, "REDO");
                spawn_editor_button(bar, 3, EditorAction::FrameSelection, "FRAME  [F]");
                spawn_editor_button(bar, 4, EditorAction::ToggleCameraMode, "FLY / ORBIT");
                spawn_editor_button(bar, 24, EditorAction::GizmoTranslate, "MOVE [G]");
                spawn_editor_button(bar, 25, EditorAction::GizmoRotate, "ROTATE [R]");
                spawn_editor_button(bar, 26, EditorAction::GizmoScale, "SCALE [S]");
                spawn_editor_button(bar, 27, EditorAction::ToggleTransformSpace, "WORLD/LOCAL");
                spawn_editor_button(bar, 28, EditorAction::CycleSnap, "SNAP");
            });

            root.spawn(Node {
                flex_grow: 1.0,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Stretch,
                padding: UiRect::vertical(Val::Px(12.0)),
                ..default()
            })
            .with_children(|body| {
                spawn_editor_panel(body, "OUTLINER", |panel| {
                    panel.spawn((
                        EditorSummaryText,
                        Text::new("Authorable objects: 0\nActive: none"),
                        TextFont {
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.82, 0.88, 0.98)),
                    ));
                    panel.spawn((
                        EditorSearchText,
                        Text::new("Filter: all objects"),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.82, 0.38)),
                    ));
                    spawn_editor_button(panel, 5, EditorAction::FocusSearch, "SEARCH / FILTER");
                    spawn_editor_button(panel, 6, EditorAction::SelectPrevious, "◀ PREVIOUS");
                    spawn_editor_button(panel, 7, EditorAction::SelectNext, "NEXT ▶");
                    spawn_editor_button(panel, 8, EditorAction::CreateAnchor, "+ EMPTY ANCHOR");
                    spawn_editor_button(panel, 9, EditorAction::CreateCube, "+ BLOCK");
                    spawn_editor_button(panel, 10, EditorAction::CreatePillar, "+ PILLAR");
                    spawn_editor_button(panel, 11, EditorAction::CreateBeacon, "+ BEACON");
                });

                spawn_editor_panel(body, "INSPECTOR", |panel| {
                    panel.spawn((
                        Text::new(
                            "TRANSFORM\nDrag colored handles\nToolbar controls mode / space / snap",
                        ),
                        TextFont {
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.82, 0.88, 0.98)),
                    ));
                    spawn_editor_button(panel, 12, EditorAction::Duplicate, "DUPLICATE");
                    spawn_editor_button(panel, 13, EditorAction::Delete, "DELETE");
                    spawn_editor_button(panel, 14, EditorAction::MoveXNegative, "X  −SNAP");
                    spawn_editor_button(panel, 15, EditorAction::MoveXPositive, "X  +SNAP");
                    spawn_editor_button(panel, 16, EditorAction::MoveYNegative, "Y  −SNAP");
                    spawn_editor_button(panel, 17, EditorAction::MoveYPositive, "Y  +SNAP");
                    spawn_editor_button(panel, 18, EditorAction::MoveZNegative, "Z  −SNAP");
                    spawn_editor_button(panel, 19, EditorAction::MoveZPositive, "Z  +SNAP");
                    spawn_editor_button(panel, 20, EditorAction::RotateYNegative, "ROTATE Y  −15°");
                    spawn_editor_button(panel, 21, EditorAction::RotateYPositive, "ROTATE Y  +15°");
                    spawn_editor_button(panel, 22, EditorAction::ScaleDown, "SCALE  −10%");
                    spawn_editor_button(panel, 23, EditorAction::ScaleUp, "SCALE  +10%");
                });

                spawn_editor_panel(body, "CONTENT REGISTRY", |panel| {
                    panel.spawn((
                        EditorRegistryText,
                        Text::new("Loading project registry…"),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.82, 0.88, 0.98)),
                    ));
                    spawn_editor_button(
                        panel,
                        33,
                        EditorAction::RegistryPrevious,
                        "◀ PREVIOUS RECORD",
                    );
                    spawn_editor_button(panel, 34, EditorAction::RegistryNext, "NEXT RECORD ▶");
                    spawn_editor_button(
                        panel,
                        35,
                        EditorAction::RegistryRename,
                        "RENAME CONTENT ID",
                    );
                    spawn_editor_button(
                        panel,
                        36,
                        EditorAction::ValidateProject,
                        "VALIDATE PROJECT",
                    );
                    spawn_editor_button(
                        panel,
                        37,
                        EditorAction::CreateMaterialRecord,
                        "+ TOON MATERIAL",
                    );
                    spawn_editor_button(
                        panel,
                        38,
                        EditorAction::DuplicateRecord,
                        "DUPLICATE RECORD",
                    );
                    spawn_editor_button(panel, 39, EditorAction::DeleteRecord, "DELETE RECORD");
                    spawn_editor_button(
                        panel,
                        40,
                        EditorAction::NormalizeSourcePath,
                        "SYNC SOURCE PATH",
                    );
                    spawn_editor_button(panel, 41, EditorAction::SearchRegistry, "SEARCH RECORDS");
                });
            });

            root.spawn((
                Node {
                    min_height: Val::Px(54.0),
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.035, 0.065, 0.96)),
            ))
            .with_children(|bar| {
                bar.spawn((
                    EditorStatusText,
                    Text::new("Editor ready"),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.62, 1.0, 0.68)),
                ));
                bar.spawn((
                    Text::new(
                        "Click objects • WASD/QE fly • RMB orbit/look • D-pad focus • A select",
                    ),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.7, 0.76, 0.9)),
                ));
            });
        });
}

fn spawn_editor_panel(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    content: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn((
            Node {
                width: Val::Px(286.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                padding: UiRect::all(Val::Px(14.0)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.035, 0.05, 0.085, 0.94)),
            BorderColor::all(Color::srgb(0.14, 0.42, 0.62)),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(title),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.78, 0.25)),
            ));
            content(panel);
        });
}

fn spawn_editor_button(
    parent: &mut ChildSpawnerCommands,
    order: usize,
    action: EditorAction,
    label: &str,
) {
    parent
        .spawn((
            Button,
            EditorButton { order, action },
            Node {
                min_width: Val::Px(112.0),
                min_height: Val::Px(26.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.14, 0.23)),
            BorderColor::all(Color::srgb(0.18, 0.42, 0.62)),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn editor_button_interactions(
    mut interactions: Query<(&Interaction, &EditorButton), Changed<Interaction>>,
    mut pending: ResMut<EditorPendingActions>,
    mut focus: ResMut<EditorFocus>,
) {
    for (interaction, button) in &mut interactions {
        if *interaction == Interaction::Pressed {
            focus.0 = button.order;
            pending.0.push(button.action);
        }
    }
}

fn editor_search_input(
    keys: Res<ButtonInput<Key>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    mut filter: ResMut<EditorOutlinerFilter>,
    mut registry: ResMut<EditorRegistryState>,
    mut session: ResMut<EditorProjectSession>,
    mut document: ResMut<EditorDocumentState>,
    mut status: ResMut<EditorRuntimeStatus>,
    mut capture: ResMut<EditorTextInputCapture>,
) {
    capture.0 = filter.active || registry.rename_active || registry.search_active;
    if !filter.active && !registry.rename_active && !registry.search_active {
        return;
    }

    let controller_accept = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::South))
        || native.just_pressed(NativeButton::South);
    let controller_clear = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::East))
        || native.just_pressed(NativeButton::East);

    if registry.search_active {
        if controller_clear {
            registry.search_text.clear();
            registry.search_active = false;
            status.message = "Registry search cleared".into();
            return;
        }
        if controller_accept {
            registry.search_active = false;
            status.message = "Registry search applied".into();
            return;
        }
        for key in keys.get_just_pressed() {
            match key {
                Key::Character(value) => {
                    for character in value.chars() {
                        if (character.is_alphanumeric()
                            || matches!(character, ' ' | '.' | '_' | '-'))
                            && registry.search_text.len() < 48
                        {
                            registry.search_text.extend(character.to_lowercase());
                        }
                    }
                }
                Key::Backspace => {
                    registry.search_text.pop();
                }
                Key::Enter => {
                    registry.search_active = false;
                    status.message = "Registry search applied".into();
                }
                Key::Escape => {
                    registry.search_text.clear();
                    registry.search_active = false;
                    status.message = "Registry search cleared".into();
                }
                _ => {}
            }
        }
        return;
    }

    if registry.rename_active {
        let mut accept = controller_accept;
        let mut cancel = controller_clear;
        for key in keys.get_just_pressed() {
            match key {
                Key::Character(value) => {
                    for character in value.chars() {
                        if (character.is_alphanumeric() || matches!(character, '.' | '_' | '-'))
                            && registry.rename_buffer.len() < 64
                        {
                            registry.rename_buffer.extend(character.to_lowercase());
                        }
                    }
                }
                Key::Backspace => {
                    registry.rename_buffer.pop();
                }
                Key::Enter => accept = true,
                Key::Escape => cancel = true,
                _ => {}
            }
        }
        if cancel {
            registry.rename_active = false;
            status.message = "Content rename cancelled".into();
        } else if accept {
            let old_id = session
                .project
                .records
                .get(registry.selected)
                .map(|record| record.content_id.clone());
            let new_id = registry.rename_buffer.trim().to_owned();
            match old_id {
                Some(old_id) if old_id == new_id => {
                    registry.rename_active = false;
                    status.message = "Content ID unchanged".into();
                }
                Some(old_id) => match session.project.rename_content(&old_id, &new_id) {
                    Ok(()) => {
                        registry.rename_active = false;
                        document.dirty = true;
                        status.message = format!("Renamed {old_id} to {new_id}");
                    }
                    Err(error) => {
                        status.message = format!("Rename rejected: {error:?}");
                    }
                },
                None => {
                    registry.rename_active = false;
                    status.message = "No registry record selected".into();
                }
            }
        }
        return;
    }

    if controller_clear {
        filter.text.clear();
        filter.active = false;
        status.message = "Outliner filter cleared".into();
        return;
    }
    if controller_accept {
        filter.active = false;
        status.message = "Outliner filter applied".into();
        return;
    }

    for key in keys.get_just_pressed() {
        match key {
            Key::Character(value) => {
                for character in value.chars() {
                    if (character.is_alphanumeric() || matches!(character, ' ' | '_' | '-'))
                        && filter.text.len() < 32
                    {
                        filter.text.extend(character.to_lowercase());
                    }
                }
            }
            Key::Backspace => {
                filter.text.pop();
            }
            Key::Enter => {
                filter.active = false;
                status.message = "Outliner filter applied".into();
            }
            Key::Escape => {
                filter.text.clear();
                filter.active = false;
                status.message = "Outliner filter cleared".into();
            }
            _ => {}
        }
    }
}

fn editor_controller_navigation(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    buttons: Query<&EditorButton>,
    mut focus: ResMut<EditorFocus>,
    mut pending: ResMut<EditorPendingActions>,
    filter: Res<EditorOutlinerFilter>,
    registry: Res<EditorRegistryState>,
    text_capture: Res<EditorTextInputCapture>,
) {
    if filter.active || registry.rename_active || registry.search_active || text_capture.0 {
        return;
    }
    let command_modifier = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);
    if keyboard.just_pressed(KeyCode::Escape) {
        pending.0.push(EditorAction::Exit);
    }
    if command_modifier && keyboard.just_pressed(KeyCode::KeyZ) {
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            pending.0.push(EditorAction::Redo);
        } else {
            pending.0.push(EditorAction::Undo);
        }
    }
    if command_modifier && keyboard.just_pressed(KeyCode::KeyS) {
        let action =
            if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
                EditorAction::PublishProject
            } else {
                EditorAction::SaveProject
            };
        pending.0.push(action);
    }
    if command_modifier && keyboard.just_pressed(KeyCode::KeyO) {
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            pending.0.push(EditorAction::RecoverProject);
        } else {
            pending.0.push(EditorAction::LoadProject);
        }
    }
    if command_modifier && keyboard.just_pressed(KeyCode::KeyD) {
        pending.0.push(EditorAction::Duplicate);
    }
    if keyboard.just_pressed(KeyCode::Delete) || keyboard.just_pressed(KeyCode::Backspace) {
        pending.0.push(EditorAction::Delete);
    }
    if keyboard.just_pressed(KeyCode::KeyF) {
        pending.0.push(EditorAction::FrameSelection);
    }
    if keyboard.just_pressed(KeyCode::KeyO) && !command_modifier {
        pending.0.push(EditorAction::ToggleCameraMode);
    }
    if keyboard.just_pressed(KeyCode::KeyG) {
        pending.0.push(EditorAction::GizmoTranslate);
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        pending.0.push(EditorAction::GizmoRotate);
    }
    if keyboard.just_pressed(KeyCode::KeyS) && !command_modifier {
        pending.0.push(EditorAction::GizmoScale);
    }
    if keyboard.just_pressed(KeyCode::KeyL) {
        pending.0.push(EditorAction::ToggleTransformSpace);
    }
    if keyboard.just_pressed(KeyCode::BracketRight) {
        pending.0.push(EditorAction::CycleSnap);
    }

    let previous = keyboard.just_pressed(KeyCode::ArrowUp)
        || keyboard.just_pressed(KeyCode::ArrowLeft)
        || gamepads.iter().any(|gamepad| {
            gamepad.just_pressed(GamepadButton::DPadUp)
                || gamepad.just_pressed(GamepadButton::DPadLeft)
        })
        || native.just_pressed(NativeButton::DPadUp)
        || native.just_pressed(NativeButton::DPadLeft);
    let next = keyboard.just_pressed(KeyCode::ArrowDown)
        || keyboard.just_pressed(KeyCode::ArrowRight)
        || gamepads.iter().any(|gamepad| {
            gamepad.just_pressed(GamepadButton::DPadDown)
                || gamepad.just_pressed(GamepadButton::DPadRight)
        })
        || native.just_pressed(NativeButton::DPadDown)
        || native.just_pressed(NativeButton::DPadRight);
    let activate = keyboard.just_pressed(KeyCode::Enter)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::South))
        || native.just_pressed(NativeButton::South);

    let count = buttons.iter().count();
    if count == 0 {
        return;
    }
    if previous {
        focus.0 = focus.0.checked_sub(1).unwrap_or(count - 1);
    } else if next {
        focus.0 = (focus.0 + 1) % count;
    }
    if activate {
        if let Some(button) = buttons.iter().find(|button| button.order == focus.0) {
            pending.0.push(button.action);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn editor_gizmo_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<EditorCamera>>,
    selection: Res<EditorSelection>,
    settings: Res<EditorGizmoSettings>,
    mut drag_state: ResMut<EditorDragState>,
    mut pending: ResMut<EditorPendingTransactions>,
    mut status: ResMut<EditorRuntimeStatus>,
    mut authorables: Query<(
        &EditorEntityId,
        &mut Transform,
        &GlobalTransform,
        &EditorAccess,
    )>,
) {
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) && drag_state.0.is_none() {
        let Some(active) = selection.active() else {
            return;
        };
        let Some((_, _, global, access)) =
            authorables.iter_mut().find(|(id, _, _, _)| **id == active)
        else {
            return;
        };
        if *access == EditorAccess::ReadOnly {
            return;
        }
        let origin = global.translation();
        let Ok(origin_screen) = camera.world_to_viewport(camera_transform, origin) else {
            return;
        };
        let (_, rotation, _) = global.to_scale_rotation_translation();
        let base_axes = [Vec3::X, Vec3::Y, Vec3::Z];
        let mut hit: Option<(f32, usize, Vec3)> = None;
        for (axis_index, base_axis) in base_axes.into_iter().enumerate() {
            let axis = match settings.space {
                EditorTransformSpace::World => base_axis,
                EditorTransformSpace::Local => rotation * base_axis,
            };
            let Ok(endpoint) = camera.world_to_viewport(camera_transform, origin + axis * 4.0)
            else {
                continue;
            };
            let distance = point_segment_distance(cursor, origin_screen, endpoint);
            if distance <= 18.0
                && hit
                    .as_ref()
                    .is_none_or(|(nearest, _, _)| distance < *nearest)
            {
                hit = Some((distance, axis_index, axis.normalize_or_zero()));
            }
        }
        if let Some((_, axis_index, axis)) = hit {
            let before = authorables
                .iter_mut()
                .filter(|(id, _, _, access)| {
                    selection.contains(**id) && **access == EditorAccess::Editable
                })
                .map(|(id, transform, _, _)| (*id, *transform))
                .collect::<Vec<_>>();
            if !before.is_empty() {
                drag_state.0 = Some(ActiveEditorDrag {
                    axis,
                    axis_index,
                    start_cursor: cursor,
                    before,
                });
                status.message = format!("Dragging {:?} axis", settings.mode);
            }
        }
    }

    let Some(drag) = drag_state.0.as_ref() else {
        return;
    };
    if mouse.pressed(MouseButton::Left) {
        let pixel_delta = cursor - drag.start_cursor;
        let raw_amount = (pixel_delta.x - pixel_delta.y) * 0.02;
        for (id, before) in &drag.before {
            let Some((_, mut transform, _, _)) = authorables
                .iter_mut()
                .find(|(candidate, _, _, _)| *candidate == id)
            else {
                continue;
            };
            let mut after = *before;
            match settings.mode {
                EditorGizmoMode::Translate => {
                    let snap = settings.translation_snap();
                    let amount = (raw_amount / snap).round() * snap;
                    after.translation += drag.axis * amount;
                }
                EditorGizmoMode::Rotate => {
                    let snap = 15.0_f32.to_radians();
                    let amount = (raw_amount / snap).round() * snap;
                    after.rotation = Quat::from_axis_angle(drag.axis, amount) * before.rotation;
                }
                EditorGizmoMode::Scale => {
                    let amount = (raw_amount / 0.1).round() * 0.1;
                    after.scale[drag.axis_index] =
                        (before.scale[drag.axis_index] + amount).max(0.1);
                }
            }
            *transform = after;
        }
    }

    if mouse.just_released(MouseButton::Left) {
        let drag = drag_state.0.take().expect("drag exists");
        let mut commands = Vec::new();
        for (id, before) in drag.before {
            if let Some((_, transform, _, _)) = authorables
                .iter_mut()
                .find(|(candidate, _, _, _)| **candidate == id)
            {
                let after = *transform;
                if after != before {
                    commands.push(EditorCommand::SetTransform { id, before, after });
                }
            }
        }
        if commands.is_empty() {
            status.message = "Transform drag cancelled".into();
        } else {
            pending.0.push(EditorTransaction {
                description: format!("Drag {:?} {} object(s)", settings.mode, commands.len()),
                commands,
            });
        }
    }
}

fn point_segment_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let t = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

#[allow(clippy::too_many_arguments)]
fn editor_viewport_picking(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    filter: Res<EditorOutlinerFilter>,
    drag: Res<EditorDragState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<EditorCamera>>,
    authorables: Query<(
        &EditorEntityId,
        &GlobalTransform,
        &AuthorableBounds,
        Option<&Name>,
    )>,
    mut selection: ResMut<EditorSelection>,
    mut status: ResMut<EditorRuntimeStatus>,
) {
    if filter.active || drag.0.is_some() || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    // Do not ray-pick through the toolbar, side panels, or status bar.
    if cursor.y < 126.0
        || cursor.y > window.height() - 68.0
        || cursor.x < 314.0
        || cursor.x > window.width() - 314.0
    {
        return;
    }
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
        return;
    };

    let direction = Vec3::from(ray.direction);
    let mut nearest: Option<(f32, EditorEntityId, String)> = None;
    for (id, transform, bounds, name) in &authorables {
        let (scale, _, position) = transform.to_scale_rotation_translation();
        let radius = bounds.0 * scale.abs().max_element().max(0.1);
        let Some(distance) = ray_sphere_distance(ray.origin, direction, position, radius) else {
            continue;
        };
        if nearest
            .as_ref()
            .is_none_or(|(nearest_distance, _, _)| distance < *nearest_distance)
        {
            nearest = Some((
                distance,
                *id,
                name.map(|value| value.as_str().to_owned())
                    .unwrap_or_else(|| format!("Object {}", id.0)),
            ));
        }
    }

    if let Some((_, id, name)) = nearest {
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            selection.toggle(id);
        } else {
            selection.replace(id);
        }
        status.message = format!("Viewport selected {name}");
    } else if !keyboard.pressed(KeyCode::ShiftLeft) && !keyboard.pressed(KeyCode::ShiftRight) {
        selection.clear();
        status.message = "Selection cleared".into();
    }
}

fn ray_sphere_distance(origin: Vec3, direction: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let offset = origin - center;
    let b = offset.dot(direction);
    let c = offset.length_squared() - radius * radius;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return None;
    }
    let near = -b - discriminant.sqrt();
    if near >= 0.0 {
        Some(near)
    } else {
        let far = -b + discriminant.sqrt();
        (far >= 0.0).then_some(far)
    }
}

fn apply_editor_actions(world: &mut World) {
    let actions = std::mem::take(&mut world.resource_mut::<EditorPendingActions>().0);
    for action in actions {
        apply_editor_action(world, action);
    }
    let transactions = std::mem::take(&mut world.resource_mut::<EditorPendingTransactions>().0);
    for transaction in transactions {
        let description = transaction.description.clone();
        let result = world.resource_scope(|world, mut stack: Mut<EditorUndoStack>| {
            stack.execute(world, transaction)
        });
        if result.is_ok() {
            world.resource_mut::<EditorDocumentState>().dirty = true;
            set_editor_status(world, format!("{description} complete"));
        } else {
            set_editor_status(world, format!("{description} failed: {result:?}"));
        }
    }
}

fn apply_editor_action(world: &mut World, action: EditorAction) {
    if action != EditorAction::Exit {
        world.resource_mut::<EditorDocumentState>().exit_armed = false;
    }
    match action {
        EditorAction::Exit => {
            let document = world.resource::<EditorDocumentState>();
            if document.dirty && !document.exit_armed {
                world.resource_mut::<EditorDocumentState>().exit_armed = true;
                set_editor_status(
                    world,
                    "UNSAVED DRAFT — press Exit/Tab again to return to play mode",
                );
                return;
            }
            world
                .resource_mut::<NextState<EngineToolMode>>()
                .set(EngineToolMode::Playing);
            set_editor_status(world, "Returning to play mode");
        }
        EditorAction::Undo => {
            let result =
                world.resource_scope(|world, mut stack: Mut<EditorUndoStack>| stack.undo(world));
            if matches!(result, Ok(true)) {
                world.resource_mut::<EditorDocumentState>().dirty = true;
            }
            set_editor_status(world, command_result_message("Undo", result));
        }
        EditorAction::Redo => {
            let result =
                world.resource_scope(|world, mut stack: Mut<EditorUndoStack>| stack.redo(world));
            if matches!(result, Ok(true)) {
                world.resource_mut::<EditorDocumentState>().dirty = true;
            }
            set_editor_status(world, command_result_message("Redo", result));
        }
        EditorAction::SelectPrevious | EditorAction::SelectNext => {
            let ids = filtered_authorable_ids(world);
            if ids.is_empty() {
                set_editor_status(world, "No authorable objects match the filter");
                return;
            }
            let active = world.resource::<EditorSelection>().active();
            let current = active
                .and_then(|id| ids.iter().position(|candidate| *candidate == id))
                .unwrap_or(0);
            let index = if action == EditorAction::SelectPrevious {
                current.checked_sub(1).unwrap_or(ids.len() - 1)
            } else {
                (current + 1) % ids.len()
            };
            world.resource_mut::<EditorSelection>().replace(ids[index]);
            set_editor_status(world, format!("Selected object #{}", ids[index].0));
        }
        EditorAction::FocusSearch => {
            {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.rename_active = false;
                registry.search_active = false;
            }
            let mut filter = world.resource_mut::<EditorOutlinerFilter>();
            filter.active = !filter.active;
            let message = if filter.active {
                "Type an object name; Enter applies, Escape/B clears"
            } else {
                "Outliner filter applied"
            };
            set_editor_status(world, message);
        }
        EditorAction::CreateAnchor => spawn_editor_primitive(world, EditorPrimitive::Empty),
        EditorAction::CreateCube => spawn_editor_primitive(world, EditorPrimitive::Cube),
        EditorAction::CreatePillar => spawn_editor_primitive(world, EditorPrimitive::Pillar),
        EditorAction::CreateBeacon => spawn_editor_primitive(world, EditorPrimitive::Beacon),
        EditorAction::Duplicate => duplicate_active(world),
        EditorAction::Delete => delete_selection(world),
        EditorAction::MoveXNegative => transform_selection(
            world,
            TransformEdit::Translate(
                Vec3::NEG_X * world.resource::<EditorGizmoSettings>().translation_snap(),
            ),
        ),
        EditorAction::MoveXPositive => transform_selection(
            world,
            TransformEdit::Translate(
                Vec3::X * world.resource::<EditorGizmoSettings>().translation_snap(),
            ),
        ),
        EditorAction::MoveYNegative => transform_selection(
            world,
            TransformEdit::Translate(
                Vec3::NEG_Y * world.resource::<EditorGizmoSettings>().translation_snap(),
            ),
        ),
        EditorAction::MoveYPositive => transform_selection(
            world,
            TransformEdit::Translate(
                Vec3::Y * world.resource::<EditorGizmoSettings>().translation_snap(),
            ),
        ),
        EditorAction::MoveZNegative => transform_selection(
            world,
            TransformEdit::Translate(
                Vec3::NEG_Z * world.resource::<EditorGizmoSettings>().translation_snap(),
            ),
        ),
        EditorAction::MoveZPositive => transform_selection(
            world,
            TransformEdit::Translate(
                Vec3::Z * world.resource::<EditorGizmoSettings>().translation_snap(),
            ),
        ),
        EditorAction::RotateYNegative => {
            transform_selection(world, TransformEdit::RotateY(-15.0_f32.to_radians()))
        }
        EditorAction::RotateYPositive => {
            transform_selection(world, TransformEdit::RotateY(15.0_f32.to_radians()))
        }
        EditorAction::ScaleDown => transform_selection(world, TransformEdit::Scale(0.9)),
        EditorAction::ScaleUp => transform_selection(world, TransformEdit::Scale(1.1)),
        EditorAction::FrameSelection => frame_editor_selection(world),
        EditorAction::ToggleCameraMode => {
            let mut rig = world.resource_mut::<EditorCameraRig>();
            rig.mode = match rig.mode {
                EditorCameraMode::Fly => EditorCameraMode::Orbit,
                EditorCameraMode::Orbit => EditorCameraMode::Fly,
            };
            let label = match rig.mode {
                EditorCameraMode::Fly => "Fly",
                EditorCameraMode::Orbit => "Orbit",
            };
            set_editor_status(world, format!("Camera mode: {label}"));
        }
        EditorAction::GizmoTranslate => {
            world.resource_mut::<EditorGizmoSettings>().mode = EditorGizmoMode::Translate;
            set_editor_status(world, "Transform gizmo: Translate");
        }
        EditorAction::GizmoRotate => {
            world.resource_mut::<EditorGizmoSettings>().mode = EditorGizmoMode::Rotate;
            set_editor_status(world, "Transform gizmo: Rotate");
        }
        EditorAction::GizmoScale => {
            world.resource_mut::<EditorGizmoSettings>().mode = EditorGizmoMode::Scale;
            set_editor_status(world, "Transform gizmo: Scale");
        }
        EditorAction::ToggleTransformSpace => {
            let space = {
                let mut gizmo = world.resource_mut::<EditorGizmoSettings>();
                gizmo.space = match gizmo.space {
                    EditorTransformSpace::World => EditorTransformSpace::Local,
                    EditorTransformSpace::Local => EditorTransformSpace::World,
                };
                gizmo.space
            };
            set_editor_status(world, format!("Transform space: {space:?}"));
        }
        EditorAction::CycleSnap => {
            let snap = {
                let mut gizmo = world.resource_mut::<EditorGizmoSettings>();
                gizmo.snap_index =
                    (gizmo.snap_index + 1) % EditorGizmoSettings::TRANSLATION_SNAPS.len();
                gizmo.translation_snap()
            };
            set_editor_status(world, format!("Translation snap: {snap:.2} m"));
        }
        EditorAction::SaveProject => save_editor_project(world, false),
        EditorAction::PublishProject => save_editor_project(world, true),
        EditorAction::LoadProject => load_editor_project(world, false),
        EditorAction::RecoverProject => load_editor_project(world, true),
        EditorAction::RegistryPrevious | EditorAction::RegistryNext => {
            let search = world.resource::<EditorRegistryState>().search_text.clone();
            let indices = filtered_registry_indices(
                &world.resource::<EditorProjectSession>().project,
                &search,
            );
            if indices.is_empty() {
                set_editor_status(world, "No registry records match the search");
                return;
            }
            let selected = {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                let current = indices.iter().position(|index| *index == registry.selected);
                let position = if action == EditorAction::RegistryPrevious {
                    current
                        .and_then(|position| position.checked_sub(1))
                        .unwrap_or(indices.len() - 1)
                } else {
                    current.map_or(0, |position| (position + 1) % indices.len())
                };
                registry.selected = indices[position];
                registry.selected
            };
            let content_id = world.resource::<EditorProjectSession>().project.records[selected]
                .content_id
                .clone();
            set_editor_status(world, format!("Selected registry record {content_id}"));
        }
        EditorAction::RegistryRename => {
            let selected = world.resource::<EditorRegistryState>().selected;
            let content_id = world
                .resource::<EditorProjectSession>()
                .project
                .records
                .get(selected)
                .map(|record| record.content_id.clone());
            if let Some(content_id) = content_id {
                world.resource_mut::<EditorOutlinerFilter>().active = false;
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.search_active = false;
                registry.rename_buffer = content_id;
                registry.rename_active = true;
                set_editor_status(
                    world,
                    "Edit the content ID; Enter/A accepts and Escape/B cancels",
                );
            } else {
                set_editor_status(world, "No registry record selected");
            }
        }
        EditorAction::ValidateProject => {
            let (errors, source_diagnostics) = {
                let session = world.resource::<EditorProjectSession>();
                (
                    validate_project(&session.project),
                    session.store.source_diagnostics(&session.project),
                )
            };
            let issue_count = errors.len() + source_diagnostics.len();
            if issue_count == 0 {
                set_editor_status(world, "Project validation passed with no errors");
            } else {
                let preview = errors
                    .iter()
                    .map(|error| format!("{error:?}"))
                    .chain(source_diagnostics.iter().cloned())
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" • ");
                set_editor_status(
                    world,
                    format!("Validation found {issue_count} issue(s): {preview}"),
                );
            }
        }
        EditorAction::CreateMaterialRecord => {
            let result = world
                .resource_mut::<EditorProjectSession>()
                .project
                .create_content(persistence::ContentCategory::Material, "New Toon Material");
            match result {
                Ok(content_id) => {
                    let index = world
                        .resource::<EditorProjectSession>()
                        .project
                        .records
                        .iter()
                        .position(|record| record.content_id == content_id)
                        .unwrap_or(0);
                    world.resource_mut::<EditorRegistryState>().selected = index;
                    world.resource_mut::<EditorDocumentState>().dirty = true;
                    set_editor_status(world, format!("Created {content_id}"));
                }
                Err(error) => set_editor_status(world, format!("Create rejected: {error}")),
            }
        }
        EditorAction::DuplicateRecord => {
            let selected = world.resource::<EditorRegistryState>().selected;
            let content_id = world
                .resource::<EditorProjectSession>()
                .project
                .records
                .get(selected)
                .map(|record| record.content_id.clone());
            let Some(content_id) = content_id else {
                set_editor_status(world, "No registry record selected");
                return;
            };
            let result = world
                .resource_mut::<EditorProjectSession>()
                .project
                .duplicate_content(&content_id);
            match result {
                Ok(new_id) => {
                    let index = world
                        .resource::<EditorProjectSession>()
                        .project
                        .records
                        .iter()
                        .position(|record| record.content_id == new_id)
                        .unwrap_or(selected);
                    world.resource_mut::<EditorRegistryState>().selected = index;
                    world.resource_mut::<EditorDocumentState>().dirty = true;
                    set_editor_status(world, format!("Duplicated as {new_id}"));
                }
                Err(error) => set_editor_status(world, format!("Duplicate rejected: {error}")),
            }
        }
        EditorAction::DeleteRecord => {
            let selected = world.resource::<EditorRegistryState>().selected;
            let content_id = world
                .resource::<EditorProjectSession>()
                .project
                .records
                .get(selected)
                .map(|record| record.content_id.clone());
            let Some(content_id) = content_id else {
                set_editor_status(world, "No registry record selected");
                return;
            };
            let result = world
                .resource_mut::<EditorProjectSession>()
                .project
                .delete_content(&content_id);
            match result {
                Ok(_) => {
                    let count = world
                        .resource::<EditorProjectSession>()
                        .project
                        .records
                        .len();
                    world.resource_mut::<EditorRegistryState>().selected =
                        selected.min(count.saturating_sub(1));
                    world.resource_mut::<EditorDocumentState>().dirty = true;
                    set_editor_status(
                        world,
                        format!("Deleted {content_id}; its source remains recoverable on disk"),
                    );
                }
                Err(error) => set_editor_status(world, format!("Delete rejected: {error}")),
            }
        }
        EditorAction::NormalizeSourcePath => {
            let selected = world.resource::<EditorRegistryState>().selected;
            let content_id = world
                .resource::<EditorProjectSession>()
                .project
                .records
                .get(selected)
                .map(|record| record.content_id.clone());
            let Some(content_id) = content_id else {
                set_editor_status(world, "No registry record selected");
                return;
            };
            let store = world.resource::<EditorProjectSession>().store.clone();
            let result = store.move_source_to_canonical(
                &mut world.resource_mut::<EditorProjectSession>().project,
                &content_id,
            );
            match result {
                Ok(path) => {
                    world.resource_mut::<EditorDocumentState>().dirty = true;
                    set_editor_status(world, format!("Source path synchronized to {path}"));
                }
                Err(error) => set_editor_status(world, format!("Source move rejected: {error}")),
            }
        }
        EditorAction::SearchRegistry => {
            world.resource_mut::<EditorOutlinerFilter>().active = false;
            let active = {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.rename_active = false;
                registry.search_active = !registry.search_active;
                registry.search_active
            };
            set_editor_status(
                world,
                if active {
                    "Type a record ID, name, or category; Enter/A applies, Escape/B clears"
                } else {
                    "Registry search applied"
                },
            );
        }
    }
}

fn filtered_registry_indices(project: &ForgeProject, search: &str) -> Vec<usize> {
    let search = search.trim().to_lowercase();
    project
        .records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            let matches = search.is_empty()
                || record.content_id.to_lowercase().contains(&search)
                || record.display_name.to_lowercase().contains(&search)
                || format!("{:?}", record.category)
                    .to_lowercase()
                    .contains(&search);
            matches.then_some(index)
        })
        .collect()
}

impl From<EditorPrimitive> for DraftPrimitive {
    fn from(value: EditorPrimitive) -> Self {
        match value {
            EditorPrimitive::Empty => Self::Empty,
            EditorPrimitive::Cube => Self::Cube,
            EditorPrimitive::Pillar => Self::Pillar,
            EditorPrimitive::Beacon => Self::Beacon,
        }
    }
}

impl From<DraftPrimitive> for EditorPrimitive {
    fn from(value: DraftPrimitive) -> Self {
        match value {
            DraftPrimitive::Empty => Self::Empty,
            DraftPrimitive::Cube => Self::Cube,
            DraftPrimitive::Pillar => Self::Pillar,
            DraftPrimitive::Beacon => Self::Beacon,
        }
    }
}

impl From<&Transform> for TransformDraft {
    fn from(value: &Transform) -> Self {
        Self {
            translation: value.translation.to_array(),
            rotation_xyzw: value.rotation.to_array(),
            scale: value.scale.to_array(),
        }
    }
}

impl From<TransformDraft> for Transform {
    fn from(value: TransformDraft) -> Self {
        Self {
            translation: Vec3::from_array(value.translation),
            rotation: Quat::from_array(value.rotation_xyzw).normalize(),
            scale: Vec3::from_array(value.scale),
        }
    }
}

fn adapter_key(
    anchor: Option<&WorldAnchor>,
    gate: Option<&DungeonCrawlGate>,
    terminal: Option<&SettlementBuildTerminal>,
    route: Option<&WorldRouteMarker>,
) -> Option<String> {
    if let Some(anchor) = anchor {
        Some(format!("anchor:{}", anchor.id))
    } else if let Some(gate) = gate {
        Some(format!("cave:{}", gate.gate_id))
    } else if let Some(terminal) = terminal {
        Some(format!("settlement:{}", terminal.settlement_id))
    } else {
        route.map(|route| format!("route:{}", route.id.0))
    }
}

#[allow(clippy::type_complexity)]
fn save_editor_project(world: &mut World, publish: bool) {
    let mut objects = world
        .query::<(&EditorEntityId, &EditorPrimitive, &Transform, Option<&Name>)>()
        .iter(world)
        .map(|(id, primitive, transform, name)| SceneObjectDraft {
            editor_id: id.0,
            name: name
                .map(|name| name.as_str().to_owned())
                .unwrap_or_else(|| format!("{} {}", primitive_label(*primitive), id.0)),
            primitive: (*primitive).into(),
            transform: transform.into(),
        })
        .collect::<Vec<_>>();
    objects.sort_by_key(|object| object.editor_id);

    let mut adapter_overrides = world
        .query::<(
            &Transform,
            &EditorAccess,
            Option<&WorldAnchor>,
            Option<&DungeonCrawlGate>,
            Option<&SettlementBuildTerminal>,
            Option<&WorldRouteMarker>,
        )>()
        .iter(world)
        .filter_map(|(transform, access, anchor, gate, terminal, route)| {
            if *access != EditorAccess::Editable {
                return None;
            }
            Some(AdapterOverrideDraft {
                adapter_key: adapter_key(anchor, gate, terminal, route)?,
                transform: transform.into(),
            })
        })
        .collect::<Vec<_>>();
    adapter_overrides.sort_by(|left, right| left.adapter_key.cmp(&right.adapter_key));

    let scene = EditorSceneDraft {
        objects,
        adapter_overrides,
    };
    let store = world.resource::<EditorProjectSession>().store.clone();
    let path = store.path().display().to_string();
    let result = {
        let mut session = world.resource_mut::<EditorProjectSession>();
        session.project.scene = scene;
        if let Err(error) = publish
            .then(|| session.project.publish_drafts())
            .transpose()
        {
            Err(error)
        } else {
            store.save(&mut session.project)
        }
    };
    match result {
        Ok(()) => {
            let mut document = world.resource_mut::<EditorDocumentState>();
            document.dirty = false;
            document.exit_armed = false;
            let verb = if publish { "published" } else { "saved" };
            set_editor_status(world, format!("Project {verb} atomically to {path}"));
        }
        Err(error) => set_editor_status(world, format!("Save failed: {error}")),
    }
}

fn load_editor_project(world: &mut World, recovery_only: bool) {
    let store = world.resource::<EditorProjectSession>().store.clone();
    let result = if recovery_only {
        store
            .load_recovery(1)
            .map(|project| (project, ProjectLoadSource::Recovery(1)))
    } else {
        store.load_with_recovery()
    };
    let (project, source) = match result {
        Ok(loaded) => loaded,
        Err(error) => {
            set_editor_status(world, format!("Load failed: {error}"));
            return;
        }
    };

    let primitive_entities = world
        .query_filtered::<Entity, With<EditorPrimitive>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in primitive_entities {
        world.despawn(entity);
    }
    world.resource_mut::<EditorSelection>().clear();
    world.resource_mut::<EditorUndoStack>().clear();

    for object in &project.scene.objects {
        let id = EditorEntityId(object.editor_id);
        world.resource_mut::<EditorIdAllocator>().reserve(id);
        spawn_editor_primitive_record(
            world,
            id,
            object.primitive.into(),
            object.name.clone(),
            object.transform.into(),
        );
    }

    let mut applied_adapters = 0_usize;
    let mut adapter_query = world.query::<(
        &mut Transform,
        Option<&WorldAnchor>,
        Option<&DungeonCrawlGate>,
        Option<&SettlementBuildTerminal>,
        Option<&WorldRouteMarker>,
    )>();
    for (mut transform, anchor, gate, terminal, route) in adapter_query.iter_mut(world) {
        let Some(key) = adapter_key(anchor, gate, terminal, route) else {
            continue;
        };
        let Some(saved) = project
            .scene
            .adapter_overrides
            .iter()
            .find(|override_draft| override_draft.adapter_key == key)
        else {
            continue;
        };
        *transform = saved.transform.into();
        applied_adapters += 1;
    }

    let missing_adapters = project
        .scene
        .adapter_overrides
        .len()
        .saturating_sub(applied_adapters);
    world.resource_mut::<EditorProjectSession>().project = project;
    {
        let count = world
            .resource::<EditorProjectSession>()
            .project
            .records
            .len();
        let mut registry = world.resource_mut::<EditorRegistryState>();
        registry.selected = registry.selected.min(count.saturating_sub(1));
        registry.rename_active = false;
        registry.search_active = false;
    }
    {
        let mut document = world.resource_mut::<EditorDocumentState>();
        document.dirty = false;
        document.exit_armed = false;
    }
    let source_label = match source {
        ProjectLoadSource::Primary => "primary project".to_owned(),
        ProjectLoadSource::Recovery(index) => format!("recovery snapshot {index}"),
    };
    let suffix = if missing_adapters == 0 {
        String::new()
    } else {
        format!("; {missing_adapters} world adapters were not present")
    };
    set_editor_status(world, format!("Loaded {source_label}{}", suffix));
}

fn filtered_authorable_ids(world: &mut World) -> Vec<EditorEntityId> {
    let filter = world.resource::<EditorOutlinerFilter>().text.to_lowercase();
    let mut ids = world
        .query_filtered::<(&EditorEntityId, Option<&Name>), With<Authorable>>()
        .iter(world)
        .filter_map(|(id, name)| {
            let matches = filter.is_empty()
                || name.is_some_and(|value| value.as_str().to_lowercase().contains(&filter));
            matches.then_some(*id)
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

fn spawn_editor_primitive(world: &mut World, primitive: EditorPrimitive) {
    let id = world.resource_mut::<EditorIdAllocator>().allocate();
    let camera_transform = world
        .query_filtered::<&Transform, With<EditorCamera>>()
        .iter(world)
        .next()
        .copied()
        .unwrap_or_else(|| Transform::from_xyz(0.0, 8.0, 12.0));
    let mut transform = Transform::from_translation(
        camera_transform.translation + camera_transform.forward() * 8.0,
    );
    if primitive == EditorPrimitive::Beacon {
        transform.scale = Vec3::splat(1.25);
    }
    let kind = primitive_label(primitive);
    spawn_editor_primitive_record(world, id, primitive, format!("{kind} {}", id.0), transform);
    world.resource_mut::<EditorSelection>().replace(id);
    world.resource_mut::<EditorDocumentState>().dirty = true;
    set_editor_status(world, format!("Created {kind} {}", id.0));
}

fn primitive_label(primitive: EditorPrimitive) -> &'static str {
    match primitive {
        EditorPrimitive::Empty => "Anchor",
        EditorPrimitive::Cube => "Block",
        EditorPrimitive::Pillar => "Pillar",
        EditorPrimitive::Beacon => "Beacon",
    }
}

fn spawn_editor_primitive_record(
    world: &mut World,
    id: EditorEntityId,
    primitive: EditorPrimitive,
    name: String,
    transform: Transform,
) {
    let (radius, mesh, color) = match primitive {
        EditorPrimitive::Empty => (0.55, None, Color::WHITE),
        EditorPrimitive::Cube => (
            1.75,
            Some(
                world
                    .resource_mut::<Assets<Mesh>>()
                    .add(Cuboid::new(2.5, 2.5, 2.5)),
            ),
            Color::srgb(0.15, 0.68, 0.94),
        ),
        EditorPrimitive::Pillar => (
            2.7,
            Some(
                world
                    .resource_mut::<Assets<Mesh>>()
                    .add(Cylinder::new(1.15, 5.0)),
            ),
            Color::srgb(0.92, 0.52, 0.14),
        ),
        EditorPrimitive::Beacon => (
            1.4,
            Some(world.resource_mut::<Assets<Mesh>>().add(Sphere::new(1.0))),
            Color::srgb(0.35, 1.0, 0.58),
        ),
    };

    let render_parts = mesh.map(|mesh| {
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: color,
                metallic: 0.15,
                perceptual_roughness: 0.42,
                ..default()
            });
        (Mesh3d(mesh), MeshMaterial3d(material))
    });
    let mut entity = world.spawn((
        Authorable,
        EditorAccess::Editable,
        AuthorableBounds(radius),
        primitive,
        id,
        Name::new(name),
        transform,
        GlobalTransform::default(),
    ));
    if let Some(render_parts) = render_parts {
        entity.insert(render_parts);
    }
}

fn duplicate_active(world: &mut World) {
    let Some(active) = world.resource::<EditorSelection>().active() else {
        set_editor_status(world, "Select an object before duplicating");
        return;
    };
    let Some(entity) = resolve_editor_entity(world, active) else {
        set_editor_status(world, "Selected object is missing");
        return;
    };
    if world.get::<EditorWorldAdapter>(entity).is_some() {
        set_editor_status(world, "World recipe adapters cannot be duplicated directly");
        return;
    }
    let Some(mut transform) = world.get::<Transform>(entity).copied() else {
        set_editor_status(world, "Selected object has no transform");
        return;
    };
    let primitive = world
        .get::<EditorPrimitive>(entity)
        .copied()
        .unwrap_or(EditorPrimitive::Empty);
    let access = world
        .get::<EditorAccess>(entity)
        .copied()
        .unwrap_or(EditorAccess::Editable);
    let bounds = world
        .get::<AuthorableBounds>(entity)
        .copied()
        .unwrap_or(AuthorableBounds(0.55));
    let mesh = world.get::<Mesh3d>(entity).cloned();
    let material = world
        .get::<MeshMaterial3d<StandardMaterial>>(entity)
        .cloned();
    transform.translation.x += 1.0;
    let id = world.resource_mut::<EditorIdAllocator>().allocate();
    let mut duplicate = world.spawn((
        Authorable,
        access,
        bounds,
        primitive,
        id,
        Name::new(format!("{:?} {}", primitive, id.0)),
        transform,
        GlobalTransform::default(),
    ));
    if let Some(mesh) = mesh {
        duplicate.insert(mesh);
    }
    if let Some(material) = material {
        duplicate.insert(material);
    }
    world.resource_mut::<EditorSelection>().replace(id);
    world.resource_mut::<EditorDocumentState>().dirty = true;
    set_editor_status(world, format!("Duplicated as object #{}", id.0));
}

fn delete_selection(world: &mut World) {
    let ids = world
        .resource::<EditorSelection>()
        .iter()
        .collect::<Vec<_>>();
    if ids.is_empty() {
        set_editor_status(world, "Nothing selected to delete");
        return;
    }
    let mut deleted = 0;
    let mut protected = 0;
    for id in &ids {
        if let Some(entity) = resolve_editor_entity(world, *id) {
            if world.get::<EditorWorldAdapter>(entity).is_some() {
                protected += 1;
            } else {
                world.despawn(entity);
                deleted += 1;
            }
        }
    }
    if protected == 0 {
        world.resource_mut::<EditorSelection>().clear();
    }
    if deleted > 0 {
        world.resource_mut::<EditorDocumentState>().dirty = true;
    }
    set_editor_status(
        world,
        format!("Deleted {deleted} object(s); {protected} recipe adapter(s) protected"),
    );
}

#[derive(Debug, Clone, Copy)]
enum TransformEdit {
    Translate(Vec3),
    RotateY(f32),
    Scale(f32),
}

fn transform_selection(world: &mut World, edit: TransformEdit) {
    let ids = world
        .resource::<EditorSelection>()
        .iter()
        .collect::<Vec<_>>();
    let mut commands = Vec::new();
    for id in ids {
        if let Some(entity) = resolve_editor_entity(world, id) {
            if world.get::<EditorAccess>(entity) == Some(&EditorAccess::ReadOnly) {
                continue;
            }
            if let Some(before) = world.get::<Transform>(entity).copied() {
                let mut after = before;
                match edit {
                    TransformEdit::Translate(delta) => after.translation += delta,
                    TransformEdit::RotateY(radians) => after.rotate_y(radians),
                    TransformEdit::Scale(factor) => {
                        after.scale = (after.scale * factor).max(Vec3::splat(0.1));
                    }
                }
                commands.push(EditorCommand::SetTransform { id, before, after });
            }
        }
    }
    if commands.is_empty() {
        set_editor_status(world, "Select a transformable object first");
        return;
    }
    let transaction = EditorTransaction {
        description: format!("Move {} object(s)", commands.len()),
        commands,
    };
    let result = world
        .resource_scope(|world, mut stack: Mut<EditorUndoStack>| stack.execute(world, transaction));
    if result.is_ok() {
        world.resource_mut::<EditorDocumentState>().dirty = true;
    }
    set_editor_status(world, command_result_message("Move", result.map(|_| true)));
}

fn frame_editor_selection(world: &mut World) {
    let Some(active) = world.resource::<EditorSelection>().active() else {
        set_editor_status(world, "Select an object to frame");
        return;
    };
    let Some(entity) = resolve_editor_entity(world, active) else {
        set_editor_status(world, "Selected object is missing");
        return;
    };
    let Some(target) = world
        .get::<Transform>(entity)
        .map(|value| value.translation)
    else {
        set_editor_status(world, "Selected object has no transform");
        return;
    };
    let radius = world
        .get::<AuthorableBounds>(entity)
        .map_or(1.0, |bounds| bounds.0);
    let distance = (radius * 5.0).clamp(6.0, 80.0);
    let camera_position = target + Vec3::new(distance * 0.72, distance * 0.5, distance);
    let camera_entity = world
        .query_filtered::<Entity, With<EditorCamera>>()
        .iter(world)
        .next();
    if let Some(camera_entity) = camera_entity {
        if let Some(mut camera) = world.get_mut::<Transform>(camera_entity) {
            *camera = Transform::from_translation(camera_position).looking_at(target, Vec3::Y);
        }
    }
    let mut rig = world.resource_mut::<EditorCameraRig>();
    rig.orbit_target = target;
    rig.orbit_distance = camera_position.distance(target);
    set_editor_status(world, format!("Framed object #{}", active.0));
}

fn command_result_message(verb: &str, result: Result<bool, EditorCommandError>) -> String {
    match result {
        Ok(true) => format!("{verb} complete"),
        Ok(false) => format!("Nothing to {}", verb.to_lowercase()),
        Err(error) => format!("{verb} failed: {error:?}"),
    }
}

fn set_editor_status(world: &mut World, message: impl Into<String>) {
    world.resource_mut::<EditorRuntimeStatus>().message = message.into();
}

#[allow(clippy::too_many_arguments)]
fn update_editor_workspace_text(
    selection: Res<EditorSelection>,
    status: Res<EditorRuntimeStatus>,
    document: Res<EditorDocumentState>,
    filter: Res<EditorOutlinerFilter>,
    camera_rig: Res<EditorCameraRig>,
    gizmo: Res<EditorGizmoSettings>,
    session: Res<EditorProjectSession>,
    registry: Res<EditorRegistryState>,
    authorables: Query<(&EditorEntityId, Option<&Name>), With<Authorable>>,
    mut text_queries: ParamSet<(
        Query<&mut Text, With<EditorSummaryText>>,
        Query<&mut Text, With<EditorStatusText>>,
        Query<&mut Text, With<EditorSearchText>>,
        Query<&mut Text, With<EditorRegistryText>>,
    )>,
) {
    let count = authorables.iter().count();
    let active = selection
        .active()
        .map(|id| format!("#{}", id.0))
        .unwrap_or_else(|| "none".into());
    let mut matching = authorables
        .iter()
        .filter(|(_, name)| {
            filter.text.is_empty()
                || name.is_some_and(|value| {
                    value
                        .as_str()
                        .to_lowercase()
                        .contains(&filter.text.to_lowercase())
                })
        })
        .map(|(id, name)| {
            name.map(|value| value.as_str().to_owned())
                .unwrap_or_else(|| format!("Object {}", id.0))
        })
        .collect::<Vec<_>>();
    matching.sort();
    let listing = if matching.is_empty() {
        "  — no matching objects —".into()
    } else {
        matching
            .iter()
            .take(7)
            .map(|name| format!("  • {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    for mut text in &mut text_queries.p0() {
        *text = Text::new(format!(
            "Authorable objects: {count}\nSelected: {}\nActive: {active}\n\n{listing}",
            selection.iter().count(),
        ));
    }
    let dirty = if document.dirty {
        "UNSAVED DRAFT"
    } else {
        "CLEAN"
    };
    let camera_mode = match camera_rig.mode {
        EditorCameraMode::Fly => "FLY",
        EditorCameraMode::Orbit => "ORBIT",
    };
    for mut text in &mut text_queries.p1() {
        *text = Text::new(format!(
            "{dirty} • {camera_mode} • {:?}/{:?} • snap {:.2}m • {}",
            gizmo.mode,
            gizmo.space,
            gizmo.translation_snap(),
            status.message
        ));
    }
    let cursor = if filter.active { "_" } else { "" };
    let filter_label = if filter.text.is_empty() {
        format!("Filter: all objects{cursor}")
    } else {
        format!("Filter: {}{cursor}", filter.text)
    };
    for mut text in &mut text_queries.p2() {
        *text = Text::new(filter_label.clone());
    }

    let records = &session.project.records;
    let selected_index = registry.selected.min(records.len().saturating_sub(1));
    let registry_indices = filtered_registry_indices(&session.project, &registry.search_text);
    let record_listing = if registry_indices.is_empty() {
        "— no matching records —".to_owned()
    } else {
        registry_indices
            .iter()
            .take(8)
            .map(|index| {
                let marker = if *index == selected_index { "▶" } else { " " };
                format!("{marker} {}", records[*index].content_id)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let selected_details = records.get(selected_index).map_or_else(
        || "No selected record".to_owned(),
        |record| {
            let state = if record.draft_hash.is_empty() {
                "UNSAVED"
            } else if record.published_hash.as_ref() == Some(&record.draft_hash) {
                "PUBLISHED"
            } else {
                "DRAFT"
            };
            let hash = record.draft_hash.chars().take(8).collect::<String>();
            format!(
                "\n{:?} • {state}\n{}\nHash: {}",
                record.category,
                record.source_path,
                if hash.is_empty() { "—" } else { &hash }
            )
        },
    );
    let validation_errors = validate_project(&session.project);
    let source_diagnostics = session.store.source_diagnostics(&session.project);
    let validation_count = validation_errors.len() + source_diagnostics.len();
    let validation = if validation_count == 0 {
        "VALIDATION: PASS".to_owned()
    } else {
        let details = validation_errors
            .iter()
            .map(|error| format!("• {error:?}"))
            .chain(
                source_diagnostics
                    .iter()
                    .map(|diagnostic| format!("• {diagnostic}")),
            )
            .take(4)
            .collect::<Vec<_>>()
            .join("\n");
        format!("VALIDATION: {validation_count} ISSUE(S)\n{details}")
    };
    let rename = if registry.rename_active {
        format!("\nRENAME: {}_", registry.rename_buffer)
    } else {
        String::new()
    };
    let search = if registry.search_text.is_empty() && !registry.search_active {
        String::new()
    } else {
        let cursor = if registry.search_active { "_" } else { "" };
        format!("\nSEARCH: {}{cursor}", registry.search_text)
    };
    for mut text in &mut text_queries.p3() {
        *text = Text::new(format!(
            "Project: {}\nRecords: {} • Matches: {}{search}\n\n{record_listing}{selected_details}{rename}\n\n{validation}",
            session.project.display_name,
            records.len(),
            registry_indices.len()
        ));
    }
}

fn update_editor_button_style(
    focus: Res<EditorFocus>,
    mut buttons: Query<
        (
            &EditorButton,
            &Interaction,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<Button>,
    >,
) {
    for (button, interaction, mut background, mut border) in &mut buttons {
        let focused = button.order == focus.0;
        *background = if *interaction == Interaction::Pressed {
            BackgroundColor(Color::srgb(0.9, 0.46, 0.08))
        } else if focused || *interaction == Interaction::Hovered {
            BackgroundColor(Color::srgb(0.08, 0.42, 0.68))
        } else {
            BackgroundColor(Color::srgb(0.08, 0.14, 0.23))
        };
        *border = BorderColor::all(if focused {
            Color::srgb(1.0, 0.8, 0.24)
        } else {
            Color::srgb(0.18, 0.42, 0.62)
        });
    }
}

fn draw_editor_gizmos(
    mut gizmos: Gizmos,
    selection: Res<EditorSelection>,
    settings: Res<EditorGizmoSettings>,
    authorables: Query<
        (
            &EditorEntityId,
            &GlobalTransform,
            &AuthorableBounds,
            &EditorAccess,
        ),
        With<Authorable>,
    >,
) {
    let grid_color = Color::srgba(0.18, 0.32, 0.42, 0.35);
    for step in -20..=20 {
        let coordinate = step as f32;
        gizmos.line(
            Vec3::new(coordinate, 0.02, -20.0),
            Vec3::new(coordinate, 0.02, 20.0),
            grid_color,
        );
        gizmos.line(
            Vec3::new(-20.0, 0.02, coordinate),
            Vec3::new(20.0, 0.02, coordinate),
            grid_color,
        );
    }

    for (id, transform, bounds, access) in &authorables {
        if !selection.contains(*id) {
            continue;
        }
        let origin = transform.translation();
        let length = (bounds.0 * 1.8).clamp(2.0, 6.0);
        let (_, rotation, _) = transform.to_scale_rotation_translation();
        let axes = [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| match settings.space {
            EditorTransformSpace::World => axis,
            EditorTransformSpace::Local => rotation * axis,
        });
        let colors = [
            Color::srgb(1.0, 0.18, 0.18),
            Color::srgb(0.25, 1.0, 0.35),
            Color::srgb(0.22, 0.55, 1.0),
        ];
        if *access == EditorAccess::Editable {
            match settings.mode {
                EditorGizmoMode::Translate | EditorGizmoMode::Scale => {
                    for (axis, color) in axes.into_iter().zip(colors) {
                        gizmos.arrow(origin, origin + axis * length, color);
                    }
                }
                EditorGizmoMode::Rotate => {
                    for (axis_index, color) in colors.into_iter().enumerate() {
                        let axis_a = axes[(axis_index + 1) % 3];
                        let axis_b = axes[(axis_index + 2) % 3];
                        let ring = (0..48).map(|step| {
                            let angle = step as f32 / 48.0 * std::f32::consts::TAU;
                            origin + (axis_a * angle.cos() + axis_b * angle.sin()) * length * 0.72
                        });
                        gizmos.lineloop(ring, color);
                    }
                }
            }
        }
        let half = Vec3::splat(bounds.0.max(0.4));
        let corners = [
            origin + Vec3::new(-half.x, 0.0, -half.z),
            origin + Vec3::new(half.x, 0.0, -half.z),
            origin + Vec3::new(half.x, 0.0, half.z),
            origin + Vec3::new(-half.x, 0.0, half.z),
        ];
        gizmos.lineloop(corners, Color::srgb(1.0, 0.82, 0.18));
    }
}

#[allow(clippy::too_many_arguments)]
fn editor_camera_controls(
    real_time: Res<Time<Real>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    filter: Res<EditorOutlinerFilter>,
    mut rig: ResMut<EditorCameraRig>,
    mut cameras: Query<&mut Transform, With<EditorCamera>>,
) {
    let Ok(mut transform) = cameras.single_mut() else {
        return;
    };
    if filter.active {
        mouse_motion.clear();
        mouse_wheel.clear();
        return;
    }
    let dt = real_time.delta_secs();
    let gamepad = gamepads.iter().next();
    let stick_move = gamepad
        .map(|pad| {
            Vec2::new(
                pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0),
                pad.get(GamepadAxis::LeftStickY).unwrap_or(0.0),
            )
        })
        .unwrap_or(native.move_axis);
    let stick_look = gamepad
        .map(|pad| {
            Vec2::new(
                pad.get(GamepadAxis::RightStickX).unwrap_or(0.0),
                pad.get(GamepadAxis::RightStickY).unwrap_or(0.0),
            )
        })
        .unwrap_or(native.look_axis);

    let keyboard_move = Vec3::new(
        (keyboard.pressed(KeyCode::KeyD) as i8 - keyboard.pressed(KeyCode::KeyA) as i8) as f32,
        (keyboard.pressed(KeyCode::KeyE) as i8 - keyboard.pressed(KeyCode::KeyQ) as i8) as f32,
        (keyboard.pressed(KeyCode::KeyS) as i8 - keyboard.pressed(KeyCode::KeyW) as i8) as f32,
    );
    let local_move = keyboard_move + Vec3::new(stick_move.x, 0.0, -stick_move.y);
    let speed = if keyboard.pressed(KeyCode::ShiftLeft) {
        42.0
    } else {
        14.0
    };
    let mouse_delta = if mouse_buttons.pressed(MouseButton::Right) {
        mouse_motion
            .read()
            .fold(Vec2::ZERO, |sum, event| sum + event.delta)
            * 0.003
    } else {
        mouse_motion.clear();
        Vec2::ZERO
    };
    let look_delta = mouse_delta + Vec2::new(stick_look.x, -stick_look.y) * 2.2 * dt;
    match rig.mode {
        EditorCameraMode::Fly => {
            let movement = transform.rotation * local_move.normalize_or_zero() * speed * dt;
            transform.translation += movement;
            if look_delta != Vec2::ZERO {
                let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
                yaw -= look_delta.x;
                pitch = (pitch - look_delta.y).clamp(-1.52, 1.52);
                transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
            }
        }
        EditorCameraMode::Orbit => {
            let right = transform.right();
            let pan = (Vec3::from(right) * local_move.x + Vec3::Y * local_move.y) * speed * dt;
            rig.orbit_target += pan;
            let mut offset = transform.translation - rig.orbit_target;
            if look_delta != Vec2::ZERO {
                offset = Quat::from_rotation_y(-look_delta.x) * offset;
                offset = Quat::from_axis_angle(*right, -look_delta.y) * offset;
            }
            let wheel = mouse_wheel.read().map(|event| event.y).sum::<f32>();
            rig.orbit_distance =
                (offset.length() - wheel * 1.5 - local_move.z * speed * dt).clamp(1.5, 500.0);
            offset = offset.normalize_or(Vec3::Z) * rig.orbit_distance;
            transform.translation = rig.orbit_target + offset;
            transform.look_at(rig.orbit_target, Vec3::Y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn persistence_test_world(label: &str) -> (World, std::path::PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "starfall_forge_world_{label}_{}_{nonce}",
            std::process::id()
        ));
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.init_resource::<EditorSelection>();
        world.init_resource::<EditorIdAllocator>();
        world.init_resource::<EditorUndoStack>();
        world.init_resource::<EditorDocumentState>();
        world.init_resource::<EditorRegistryState>();
        world.insert_resource(EditorRuntimeStatus {
            message: String::new(),
        });
        world.insert_resource(EditorProjectSession {
            project: ForgeProject::default(),
            store: ProjectStore::new(root.join("project.json"), 3),
        });
        (world, root)
    }

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

    #[test]
    fn viewport_ray_hits_nearest_forward_sphere() {
        let distance = ray_sphere_distance(Vec3::ZERO, Vec3::Z, Vec3::Z * 10.0, 2.0);
        assert_eq!(distance, Some(8.0));
    }

    #[test]
    fn viewport_ray_rejects_sphere_behind_camera() {
        assert_eq!(
            ray_sphere_distance(Vec3::ZERO, Vec3::Z, Vec3::NEG_Z * 4.0, 1.0),
            None
        );
    }

    #[test]
    fn gizmo_hit_distance_uses_closest_point_on_handle() {
        assert_eq!(
            point_segment_distance(Vec2::new(5.0, 3.0), Vec2::ZERO, Vec2::X * 10.0),
            3.0
        );
        assert_eq!(
            point_segment_distance(Vec2::new(15.0, 0.0), Vec2::ZERO, Vec2::X * 10.0),
            5.0
        );
    }

    #[test]
    fn translation_snap_presets_cover_fine_to_coarse_edits() {
        let mut settings = EditorGizmoSettings::default();
        assert_eq!(settings.translation_snap(), 0.5);
        settings.snap_index = 0;
        assert_eq!(settings.translation_snap(), 0.25);
        settings.snap_index = 3;
        assert_eq!(settings.translation_snap(), 2.0);
    }

    #[test]
    fn editor_project_round_trip_restores_primitives_and_stable_adapters() {
        let (mut world, root) = persistence_test_world("round_trip");
        let object_id = EditorEntityId(71);
        let object_transform = Transform::from_xyz(4.0, 7.0, -12.0)
            .with_rotation(Quat::from_rotation_y(0.7))
            .with_scale(Vec3::new(1.2, 0.8, 2.0));
        spawn_editor_primitive_record(
            &mut world,
            object_id,
            EditorPrimitive::Cube,
            "Saved block".into(),
            object_transform,
        );
        let anchor_transform = Transform::from_xyz(33.0, 5.0, -8.0);
        let anchor = world
            .spawn((
                EditorAccess::Editable,
                WorldAnchor { id: "test_anchor" },
                anchor_transform,
            ))
            .id();

        save_editor_project(&mut world, false);
        assert!(!world.resource::<EditorDocumentState>().dirty);
        let saved = world
            .resource::<EditorProjectSession>()
            .store
            .load()
            .unwrap();
        assert_eq!(saved.scene.objects[0].editor_id, object_id.0);
        assert_eq!(
            saved.scene.adapter_overrides[0].adapter_key,
            "anchor:test_anchor"
        );

        world.get_mut::<Transform>(anchor).unwrap().translation = Vec3::ZERO;
        load_editor_project(&mut world, false);
        let loaded_entity = resolve_editor_entity(&mut world, object_id).unwrap();
        assert_eq!(
            world.get::<Transform>(loaded_entity).unwrap().translation,
            object_transform.translation
        );
        assert_eq!(
            world.get::<Transform>(anchor).unwrap().translation,
            anchor_transform.translation
        );
        assert_eq!(
            world.resource_mut::<EditorIdAllocator>().allocate(),
            EditorEntityId(72)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn registry_actions_cycle_records_without_losing_selection() {
        let (mut world, root) = persistence_test_world("registry_cycle");
        world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Material, "Test")
            .unwrap();

        apply_editor_action(&mut world, EditorAction::RegistryNext);
        assert_eq!(world.resource::<EditorRegistryState>().selected, 1);
        apply_editor_action(&mut world, EditorAction::RegistryNext);
        assert_eq!(world.resource::<EditorRegistryState>().selected, 0);
        apply_editor_action(&mut world, EditorAction::RegistryPrevious);
        assert_eq!(world.resource::<EditorRegistryState>().selected, 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn registry_lifecycle_actions_create_duplicate_and_delete_materials() {
        let (mut world, root) = persistence_test_world("registry_lifecycle");
        apply_editor_action(&mut world, EditorAction::CreateMaterialRecord);
        assert_eq!(
            world
                .resource::<EditorProjectSession>()
                .project
                .records
                .len(),
            2
        );
        assert_eq!(world.resource::<EditorRegistryState>().selected, 1);

        apply_editor_action(&mut world, EditorAction::DuplicateRecord);
        assert_eq!(
            world
                .resource::<EditorProjectSession>()
                .project
                .records
                .len(),
            3
        );
        apply_editor_action(&mut world, EditorAction::DeleteRecord);
        assert_eq!(
            world
                .resource::<EditorProjectSession>()
                .project
                .records
                .len(),
            2
        );
        assert!(world.resource::<EditorDocumentState>().dirty);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn registry_search_matches_ids_names_and_categories() {
        let mut project = ForgeProject::default();
        project
            .create_content(persistence::ContentCategory::Material, "Anime Ink")
            .unwrap();
        assert_eq!(filtered_registry_indices(&project, "main_world"), [0]);
        assert_eq!(filtered_registry_indices(&project, "anime"), [1]);
        assert_eq!(filtered_registry_indices(&project, "material"), [1]);
        assert!(filtered_registry_indices(&project, "missing").is_empty());
    }
}
