//! Shared foundations for Starfall's in-game creation tools.
//!
//! Gameplay generators remain recipe-first. This module supplies editor
//! services that can be reused by Character Studio, Creature Forge, World Kit
//! Forge, and the future Level Scene Composer without serializing transient ECS
//! entities or depending on Bevy's runtime `Entity` values.

#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
pub mod character_records;
pub mod creature_records;
mod persistence;
mod presets;
pub mod project_registry;

use std::collections::{BTreeMap, BTreeSet};

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
    GenericRecipeDraft, LevelTemplate, ProceduralRecipeDraft, ProjectLoadSource, ProjectStore,
    RoadJunctionDraft, RoadJunctionKind, RoadSplinePointDraft, SceneObjectDraft, TransformDraft,
    WorldTopologyEdgeDraft, WorldTopologyEdgeKind, WorldTopologyNodeDraft, WorldTopologyNodeKind,
    WorldTopologySocketDraft, WorldTopologySocketKind,
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

#[derive(SystemParam)]
struct ForgePreviewHandles<'w, 's> {
    toon: Query<'w, 's, &'static MeshMaterial3d<ToonMaterial>, With<EditorWorkspaceRoot>>,
    water: Query<'w, 's, &'static MeshMaterial3d<WaterMaterial>, With<EditorWorkspaceRoot>>,
    energy: Query<'w, 's, &'static MeshMaterial3d<EnergyMaterial>, With<EditorWorkspaceRoot>>,
    shield: Query<'w, 's, &'static MeshMaterial3d<ShieldMaterial>, With<EditorWorkspaceRoot>>,
    ice: Query<'w, 's, &'static MeshMaterial3d<IceMaterial>, With<EditorWorkspaceRoot>>,
    lava: Query<'w, 's, &'static MeshMaterial3d<LavaMaterial>, With<EditorWorkspaceRoot>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForgeMaterialFamily {
    Toon,
    Water,
    Energy,
    Shield,
    Ice,
    Lava,
}

impl ForgeMaterialFamily {
    const ALL: [Self; 6] = [
        Self::Toon,
        Self::Water,
        Self::Energy,
        Self::Shield,
        Self::Ice,
        Self::Lava,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Toon => "toon_v1",
            Self::Water => "water_v1",
            Self::Energy => "energy_v1",
            Self::Shield => "shield_v1",
            Self::Ice => "ice_v1",
            Self::Lava => "lava_v1",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Toon => "Toon",
            Self::Water => "Water",
            Self::Energy => "Energy",
            Self::Shield => "Shield",
            Self::Ice => "Ice / Snow",
            Self::Lava => "Lava",
        }
    }

    fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|family| family.id() == id)
            .unwrap_or(Self::Toon)
    }

    fn offset(self, delta: isize) -> Self {
        let index = Self::ALL
            .iter()
            .position(|family| *family == self)
            .unwrap_or(0);
        Self::ALL[(index as isize + delta).rem_euclid(Self::ALL.len() as isize) as usize]
    }
}

#[derive(Debug, Clone, Copy)]
struct ForgeMaterialControl {
    key: &'static str,
    label: &'static str,
    min: f32,
    max: f32,
    step: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct ForgeMaterialSpec {
    family: ForgeMaterialFamily,
    preset: usize,
    values: [f32; 6],
}

#[derive(Debug, Clone)]
enum PublishedMaterialHandle {
    Toon(Handle<ToonMaterial>),
    Water(Handle<WaterMaterial>),
    Energy(Handle<EnergyMaterial>),
    Shield(Handle<ShieldMaterial>),
    Ice(Handle<IceMaterial>),
    Lava(Handle<LavaMaterial>),
}

#[derive(Debug, Clone)]
struct PublishedMaterialEntry {
    source_hash: String,
    handle: PublishedMaterialHandle,
}

/// Runtime lookup keyed by stable Forge content IDs. Bevy handles never enter
/// project files or save data.
#[derive(Resource, Default)]
pub struct PublishedMaterialCatalog {
    entries: BTreeMap<String, PublishedMaterialEntry>,
}

impl PublishedMaterialCatalog {
    pub fn contains(&self, content_id: &str) -> bool {
        self.entries.contains_key(content_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProceduralRecipeKind {
    Biome,
    Road,
    Building,
    Cave,
    City,
}

impl ProceduralRecipeKind {
    fn from_category(category: persistence::ContentCategory) -> Option<Self> {
        match category {
            persistence::ContentCategory::Biome => Some(Self::Biome),
            persistence::ContentCategory::Road => Some(Self::Road),
            persistence::ContentCategory::Building => Some(Self::Building),
            persistence::ContentCategory::Cave => Some(Self::Cave),
            persistence::ContentCategory::City => Some(Self::City),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct PublishedProceduralRecipeEntry {
    source_hash: String,
    kind: ProceduralRecipeKind,
    seed: u64,
    revision: u64,
    material_slots: BTreeMap<String, String>,
}

/// Published Creature Forge recipes, keyed by stable content id. Enemy
/// spawners and encounter content resolve creatures through this catalog so
/// gameplay only ever consumes published (hashed, validated) records.
#[derive(Resource, Default)]
pub struct PublishedCreatureCatalog {
    entries: BTreeMap<String, crate::robots::creature::CreatureSpec>,
}

impl PublishedCreatureCatalog {
    pub fn contains(&self, content_id: &str) -> bool {
        self.entries.contains_key(content_id)
    }

    pub fn get(&self, content_id: &str) -> Option<&crate::robots::creature::CreatureSpec> {
        self.entries.get(content_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn content_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

/// Published deterministic generator inputs keyed by stable Forge content ID.
/// The catalog contains source data only; generated ECS entities remain
/// disposable and can be rebuilt without changing authored identity.
#[derive(Resource, Default)]
pub struct PublishedProceduralRecipeCatalog {
    entries: BTreeMap<String, PublishedProceduralRecipeEntry>,
}

impl PublishedProceduralRecipeCatalog {
    pub fn contains(&self, content_id: &str) -> bool {
        self.entries.contains_key(content_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn material_id(&self, recipe_id: &str, slot: &str) -> Option<&str> {
        self.entries
            .get(recipe_id)?
            .material_slots
            .get(slot)
            .map(String::as_str)
    }

    pub fn generation(&self, recipe_id: &str) -> Option<(ProceduralRecipeKind, u64, u64)> {
        let entry = self.entries.get(recipe_id)?;
        Some((entry.kind, entry.seed, entry.revision))
    }

    pub fn source_hash(&self, recipe_id: &str) -> Option<&str> {
        self.entries
            .get(recipe_id)
            .map(|entry| entry.source_hash.as_str())
    }
}

/// Stable appearance request attached to disposable generated geometry.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ProceduralMaterialBinding {
    pub recipe_id: String,
    pub slot: String,
}

impl ProceduralMaterialBinding {
    pub fn new(recipe_id: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            recipe_id: recipe_id.into(),
            slot: slot.into(),
        }
    }
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
struct ResolvedProceduralMaterialBinding {
    epoch: u64,
    recipe_id: String,
    slot: String,
    material_id: Option<String>,
}

#[derive(Component, Debug, Clone)]
struct ProceduralStandardMaterialFallback(Handle<StandardMaterial>);

#[derive(Resource, Default)]
struct PublishedContentCatalogEpoch(u64);

#[derive(Component, Debug, Clone, PartialEq, Eq)]
struct EditorMaterialBinding(String);

impl ForgeMaterialSpec {
    fn from_recipe(recipe: &GenericRecipeDraft) -> Self {
        let family = recipe
            .fields
            .get("shader")
            .and_then(serde_json::Value::as_str)
            .map(ForgeMaterialFamily::from_id)
            .unwrap_or(ForgeMaterialFamily::Toon);
        let preset = recipe
            .fields
            .get("preset")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize
            % 3;
        let mut spec = Self::preset(family, preset);
        for (index, control) in material_controls(family).iter().enumerate() {
            if let Some(value) = recipe
                .fields
                .get(control.key)
                .and_then(serde_json::Value::as_f64)
            {
                spec.values[index] = (value as f32).clamp(control.min, control.max);
            }
        }
        // Read the original R1 nested Toon payload without changing its codec.
        if family == ForgeMaterialFamily::Toon {
            if let Some(bands) = recipe.fields.get("bands") {
                for (index, key) in [
                    "shadow_threshold",
                    "light_threshold",
                    "shadow_level",
                    "mid_level",
                ]
                .iter()
                .enumerate()
                {
                    if recipe.fields.contains_key(*key) {
                        continue;
                    }
                    if let Some(value) = bands.get(*key).and_then(serde_json::Value::as_f64) {
                        let control = material_controls(family)[index];
                        spec.values[index] = (value as f32).clamp(control.min, control.max);
                    }
                }
            }
            if let Some(rim) = recipe.fields.get("rim") {
                if !recipe.fields.contains_key("rim_strength") {
                    if let Some(value) = rim.get("strength").and_then(serde_json::Value::as_f64) {
                        spec.values[4] = (value as f32).clamp(0.0, 1.5);
                    }
                }
                if !recipe.fields.contains_key("rim_exponent") {
                    if let Some(value) = rim.get("exponent").and_then(serde_json::Value::as_f64) {
                        spec.values[5] = (value as f32).clamp(0.5, 8.0);
                    }
                }
            }
        }
        spec.normalize();
        spec
    }

    fn preset(family: ForgeMaterialFamily, preset: usize) -> Self {
        let values = match (family, preset % 3) {
            (ForgeMaterialFamily::Toon, 0) => [0.12, 0.52, 0.28, 0.64, 0.48, 2.4],
            (ForgeMaterialFamily::Toon, 1) => [0.08, 0.42, 0.18, 0.58, 0.72, 3.4],
            (ForgeMaterialFamily::Toon, _) => [0.18, 0.62, 0.36, 0.74, 0.32, 1.8],
            (ForgeMaterialFamily::Water, 0) => [0.72, 0.055, 0.16, 0.63, 3.2, 0.66],
            (ForgeMaterialFamily::Water, 1) => [1.15, 0.085, 0.24, 0.82, 2.4, 0.72],
            (ForgeMaterialFamily::Water, _) => [0.34, 0.035, 0.08, 0.42, 4.6, 0.58],
            (ForgeMaterialFamily::Energy, 0) => [2.4, 5.0, 3.2, 0.82, 0.0, 0.0],
            (ForgeMaterialFamily::Energy, 1) => [4.6, 8.0, 6.2, 0.94, 0.0, 0.0],
            (ForgeMaterialFamily::Energy, _) => [1.3, 3.2, 2.1, 0.68, 0.0, 0.0],
            (ForgeMaterialFamily::Shield, 0) => [7.0, 0.08, 2.2, 0.38, 2.4, 0.85],
            (ForgeMaterialFamily::Shield, 1) => [11.0, 0.055, 4.0, 0.52, 3.2, 1.15],
            (ForgeMaterialFamily::Shield, _) => [4.5, 0.12, 1.4, 0.28, 1.8, 0.65],
            (ForgeMaterialFamily::Ice, 0) => [0.18, 0.42, 0.34, 0.38, 0.0, 0.0],
            (ForgeMaterialFamily::Ice, 1) => [0.32, 0.62, 0.58, 0.52, 0.0, 0.0],
            (ForgeMaterialFamily::Ice, _) => [0.10, 0.24, 0.16, 0.28, 0.0, 0.0],
            (ForgeMaterialFamily::Lava, 0) => [0.34, 0.21, 0.24, 1.65, 0.0, 0.0],
            (ForgeMaterialFamily::Lava, 1) => [0.72, 0.34, 0.15, 1.95, 0.0, 0.0],
            (ForgeMaterialFamily::Lava, _) => [0.16, 0.14, 0.34, 1.25, 0.0, 0.0],
        };
        Self {
            family,
            preset: preset % 3,
            values,
        }
    }

    fn write_to(&self, recipe: &mut GenericRecipeDraft) {
        recipe
            .fields
            .insert("shader".into(), serde_json::json!(self.family.id()));
        recipe
            .fields
            .insert("preset".into(), serde_json::json!(self.preset));
        for (index, control) in material_controls(self.family).iter().enumerate() {
            recipe
                .fields
                .insert(control.key.into(), serde_json::json!(self.values[index]));
        }
    }

    fn normalize(&mut self) {
        if self.family == ForgeMaterialFamily::Toon {
            self.values[1] = self.values[1].max(self.values[0] + 0.02).min(1.0);
            self.values[3] = self.values[3].max(self.values[2] + 0.02).min(0.95);
        }
    }
}

fn material_controls(family: ForgeMaterialFamily) -> &'static [ForgeMaterialControl] {
    const TOON: [ForgeMaterialControl; 6] = [
        ForgeMaterialControl {
            key: "shadow_threshold",
            label: "Shadow threshold",
            min: 0.0,
            max: 0.8,
            step: 0.02,
        },
        ForgeMaterialControl {
            key: "light_threshold",
            label: "Light threshold",
            min: 0.2,
            max: 1.0,
            step: 0.02,
        },
        ForgeMaterialControl {
            key: "shadow_level",
            label: "Shadow level",
            min: 0.05,
            max: 0.8,
            step: 0.02,
        },
        ForgeMaterialControl {
            key: "mid_level",
            label: "Mid level",
            min: 0.2,
            max: 0.95,
            step: 0.02,
        },
        ForgeMaterialControl {
            key: "rim_strength",
            label: "Rim strength",
            min: 0.0,
            max: 1.5,
            step: 0.05,
        },
        ForgeMaterialControl {
            key: "rim_exponent",
            label: "Rim exponent",
            min: 0.5,
            max: 8.0,
            step: 0.2,
        },
    ];
    const WATER: [ForgeMaterialControl; 6] = [
        ForgeMaterialControl {
            key: "wave_speed",
            label: "Wave speed",
            min: 0.0,
            max: 3.0,
            step: 0.1,
        },
        ForgeMaterialControl {
            key: "wave_frequency",
            label: "Wave frequency",
            min: 0.005,
            max: 0.3,
            step: 0.01,
        },
        ForgeMaterialControl {
            key: "wave_amplitude",
            label: "Wave amplitude",
            min: 0.0,
            max: 0.8,
            step: 0.03,
        },
        ForgeMaterialControl {
            key: "secondary_wave",
            label: "Secondary wave",
            min: 0.0,
            max: 1.5,
            step: 0.05,
        },
        ForgeMaterialControl {
            key: "fresnel_exponent",
            label: "Fresnel exponent",
            min: 0.5,
            max: 8.0,
            step: 0.2,
        },
        ForgeMaterialControl {
            key: "opacity",
            label: "Opacity",
            min: 0.15,
            max: 1.0,
            step: 0.04,
        },
    ];
    const ENERGY: [ForgeMaterialControl; 4] = [
        ForgeMaterialControl {
            key: "flow_speed",
            label: "Flow speed",
            min: 0.0,
            max: 8.0,
            step: 0.2,
        },
        ForgeMaterialControl {
            key: "spatial_frequency",
            label: "Flow frequency",
            min: 0.5,
            max: 14.0,
            step: 0.4,
        },
        ForgeMaterialControl {
            key: "pulse_speed",
            label: "Pulse speed",
            min: 0.0,
            max: 12.0,
            step: 0.3,
        },
        ForgeMaterialControl {
            key: "opacity",
            label: "Opacity",
            min: 0.1,
            max: 1.0,
            step: 0.04,
        },
    ];
    const SHIELD: [ForgeMaterialControl; 6] = [
        ForgeMaterialControl {
            key: "grid_scale",
            label: "Grid scale",
            min: 2.0,
            max: 18.0,
            step: 0.5,
        },
        ForgeMaterialControl {
            key: "line_width",
            label: "Line width",
            min: 0.02,
            max: 0.24,
            step: 0.01,
        },
        ForgeMaterialControl {
            key: "pulse_speed",
            label: "Pulse speed",
            min: 0.0,
            max: 10.0,
            step: 0.25,
        },
        ForgeMaterialControl {
            key: "opacity",
            label: "Opacity",
            min: 0.08,
            max: 0.9,
            step: 0.04,
        },
        ForgeMaterialControl {
            key: "fresnel_exponent",
            label: "Fresnel exponent",
            min: 0.5,
            max: 8.0,
            step: 0.2,
        },
        ForgeMaterialControl {
            key: "edge_strength",
            label: "Edge strength",
            min: 0.0,
            max: 2.0,
            step: 0.05,
        },
    ];
    const ICE: [ForgeMaterialControl; 4] = [
        ForgeMaterialControl {
            key: "detail_scale",
            label: "Detail scale",
            min: 0.02,
            max: 1.0,
            step: 0.03,
        },
        ForgeMaterialControl {
            key: "frost_coverage",
            label: "Frost coverage",
            min: 0.0,
            max: 1.0,
            step: 0.04,
        },
        ForgeMaterialControl {
            key: "sparkle_strength",
            label: "Sparkle strength",
            min: 0.0,
            max: 1.5,
            step: 0.05,
        },
        ForgeMaterialControl {
            key: "fresnel_strength",
            label: "Fresnel strength",
            min: 0.0,
            max: 1.5,
            step: 0.05,
        },
    ];
    const LAVA: [ForgeMaterialControl; 4] = [
        ForgeMaterialControl {
            key: "flow_speed",
            label: "Flow speed",
            min: 0.0,
            max: 2.0,
            step: 0.08,
        },
        ForgeMaterialControl {
            key: "cell_scale",
            label: "Cell scale",
            min: 0.03,
            max: 0.8,
            step: 0.03,
        },
        ForgeMaterialControl {
            key: "seam_width",
            label: "Seam width",
            min: 0.04,
            max: 0.7,
            step: 0.03,
        },
        ForgeMaterialControl {
            key: "emissive_strength",
            label: "Emissive strength",
            min: 0.2,
            max: 2.0,
            step: 0.08,
        },
    ];
    match family {
        ForgeMaterialFamily::Toon => &TOON,
        ForgeMaterialFamily::Water => &WATER,
        ForgeMaterialFamily::Energy => &ENERGY,
        ForgeMaterialFamily::Shield => &SHIELD,
        ForgeMaterialFamily::Ice => &ICE,
        ForgeMaterialFamily::Lava => &LAVA,
    }
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
            .init_resource::<WorldKitTopologyDragState>()
            .init_resource::<EditorPendingTransactions>()
            .init_resource::<EditorProjectSession>()
            .init_resource::<project_registry::ForgeProjectRegistry>()
            .init_resource::<PublishedMaterialCatalog>()
            .init_resource::<PublishedProceduralRecipeCatalog>()
            .init_resource::<PublishedCreatureCatalog>()
            .init_resource::<PublishedContentCatalogEpoch>()
            .init_resource::<EditorRegistryState>()
            .init_resource::<WorldKitPreviewState>()
            .init_resource::<WorldKitSandboxState>()
            .init_resource::<WorldKitSandboxAssets>()
            .init_resource::<EditorTextInputCapture>()
            .init_resource::<EditorPresetLibrary>()
            .init_resource::<EditorArmedModifier>()
            .init_resource::<EditorModifierParamCursor>()
            .init_resource::<EditorArmedLevelTemplate>()
            .init_resource::<EditorLevelDeleteArm>()
            .register_type::<EditorEntityId>()
            .add_systems(Startup, bootstrap_published_content_catalogs)
            .add_systems(
                Update,
                toggle_editor_workspace.run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                OnEnter(EngineToolMode::Editing),
                (
                    sync_active_project_session,
                    enter_editor_workspace,
                    adapt_world_authorables,
                )
                    .chain(),
            )
            .add_systems(OnExit(EngineToolMode::Editing), exit_editor_workspace)
            .add_systems(Update, resolve_procedural_material_bindings)
            .add_systems(
                Update,
                (
                    adapt_world_authorables,
                    editor_button_interactions,
                    editor_search_input,
                    editor_controller_navigation,
                    keep_editor_focus_in_view,
                    editor_gizmo_drag,
                    world_kit_topology_gizmo_drag,
                    editor_viewport_picking,
                    apply_editor_actions,
                    sync_forge_material_preview,
                    sync_world_kit_preview,
                    sync_world_kit_sandbox,
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
    MissingRecipe(String),
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
    SetProceduralRecipe {
        content_id: String,
        before: Box<ProceduralRecipeDraft>,
        after: Box<ProceduralRecipeDraft>,
    },
}

impl EditorCommand {
    fn target(&self) -> EditorEntityId {
        match self {
            Self::SetTransform { id, .. } => *id,
            Self::SetProceduralRecipe { .. } => EditorEntityId(0),
        }
    }

    fn preflight(&self, world: &mut World) -> Result<(), EditorCommandError> {
        if let Self::SetProceduralRecipe { content_id, .. } = self {
            let exists = world
                .resource::<EditorProjectSession>()
                .project
                .payloads
                .get(content_id)
                .is_some_and(|payload| payload.procedural_recipe().is_some());
            return exists
                .then_some(())
                .ok_or_else(|| EditorCommandError::MissingRecipe(content_id.clone()));
        }
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
            Self::SetProceduralRecipe {
                content_id, after, ..
            } => set_procedural_recipe(world, content_id, (**after).clone()),
        }
    }

    fn undo(&self, world: &mut World) -> Result<(), EditorCommandError> {
        match self {
            Self::SetTransform { id, before, .. } => set_transform(world, *id, *before),
            Self::SetProceduralRecipe {
                content_id, before, ..
            } => set_procedural_recipe(world, content_id, (**before).clone()),
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

fn set_procedural_recipe(
    world: &mut World,
    content_id: &str,
    value: ProceduralRecipeDraft,
) -> Result<(), EditorCommandError> {
    let mut session = world.resource_mut::<EditorProjectSession>();
    let payload = session
        .project
        .payloads
        .get_mut(content_id)
        .ok_or_else(|| EditorCommandError::MissingRecipe(content_id.into()))?;
    let recipe = payload
        .procedural_recipe_mut()
        .ok_or_else(|| EditorCommandError::MissingRecipe(content_id.into()))?;
    *recipe = value;
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
    PresetCapture,
    PresetNext,
    PresetApply,
    PresetCompare,
    PresetFork,
    PresetNewSeedVariant,
    ModifierCycleKind,
    ModifierAdd,
    ModifierRemoveLast,
    ModifierCycleParam,
    ModifierValueDown,
    ModifierValueUp,
    LevelCycleTemplate,
    LevelNew,
    LevelNext,
    LevelMoveEarlier,
    LevelMoveLater,
    LevelDelete,
    LevelSetStartup,
    PlaytestStartup,
    SearchRegistry,
    MaterialCycleFamily,
    MaterialCyclePreset,
    MaterialNextParameter,
    MaterialDecrease,
    MaterialIncrease,
    MaterialReset,
    ApplySelectedMaterial,
    ClearObjectMaterial,
    CycleRecipeKind,
    CreateRecipeRecord,
    CopySelectedMaterial,
    RecipeNextSlot,
    BindRecipeMaterial,
    ClearRecipeMaterial,
    RegenerateRecipePreview,
    RecipeNextParameter,
    RecipeParameterDecrease,
    RecipeParameterIncrease,
    RecipeResetParameters,
    RecipeSeedDecrease,
    RecipeSeedIncrease,
    ValidateSelectedRecipe,
    SeedDefaultTopology,
    TopologyPrevious,
    TopologyNext,
    TopologyMoveXNegative,
    TopologyMoveXPositive,
    TopologyMoveYNegative,
    TopologyMoveYPositive,
    TopologyMoveZNegative,
    TopologyMoveZPositive,
    TopologyMarkEdgeStart,
    TopologyCycleEdgeKind,
    TopologyConnectEdge,
    TopologyRemoveEdge,
    RoadTangentNextAxis,
    RoadTangentDecrease,
    RoadTangentIncrease,
    RoadTangentReset,
    RoadTangentAutoRoundAll,
    RoadMarkJunctionStart,
    RoadCycleJunctionKind,
    RoadConnectJunction,
    RoadRemoveJunction,
    SocketNext,
    SocketCycleKind,
    SocketCreate,
    SocketDelete,
    SocketMoveXNegative,
    SocketMoveXPositive,
    SocketMoveYNegative,
    SocketMoveYPositive,
    SocketMoveZNegative,
    SocketMoveZPositive,
    TerrainOriginXNegative,
    TerrainOriginXPositive,
    TerrainOriginZNegative,
    TerrainOriginZPositive,
    TerrainClearanceDecrease,
    TerrainClearanceIncrease,
    TerrainProjectSelected,
    TerrainProjectAll,
    CompileSandboxRoot,
    ClearSandboxRoot,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum EditorPanelKind {
    Outliner,
    Inspector,
    Registry,
}

#[derive(Component)]
struct WorldKitPreview;

#[derive(Component, Debug, Clone)]
struct WorldKitGeneratedRoot {
    content_id: String,
    signature: String,
}

#[derive(Component)]
struct WorldKitGeneratedPart;

#[derive(Resource, Default)]
struct WorldKitPreviewState {
    signature: Option<String>,
}

#[derive(Resource, Default)]
struct WorldKitSandboxState {
    active_content_id: Option<String>,
    applied_signature: Option<String>,
}

#[derive(Resource, Default)]
struct WorldKitSandboxAssets {
    cube: Option<Handle<Mesh>>,
    fallback: Option<Handle<StandardMaterial>>,
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

#[derive(Debug, Clone)]
struct ActiveTopologyDrag {
    content_id: String,
    category: persistence::ContentCategory,
    element_index: usize,
    axis_index: usize,
    start_cursor: Vec2,
    screen_axis: Vec2,
    recipe_units_per_pixel: f32,
    before: Box<ProceduralRecipeDraft>,
}

#[derive(Resource, Default)]
struct WorldKitTopologyDragState(Option<ActiveTopologyDrag>);

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
    material_parameter: usize,
    recipe_kind: usize,
    recipe_slot: usize,
    recipe_parameter: usize,
    topology_element: usize,
    topology_edge_start: Option<String>,
    topology_edge_start_socket: Option<String>,
    topology_edge_kind: usize,
    topology_socket: usize,
    topology_socket_kind: usize,
    road_tangent_axis: usize,
    road_junction_start: Option<String>,
    road_junction_kind: usize,
    material_clipboard: Option<String>,
}

#[derive(Resource, Default)]
struct EditorTextInputCapture(bool);

/// PM2: the loaded preset library for the active project plus the cursor used
/// by the preset action buttons.
#[derive(Resource, Default)]
struct EditorPresetLibrary {
    presets: Vec<presets::PresetRecord>,
    selected: usize,
}

/// Blender-style non-destructive modifier stack on one editor scene object.
/// The base primitive mesh is regenerated and the stack re-applied whenever
/// the stack changes, so the source data stays destructive-edit free.
#[derive(Component, Debug, Clone, Default)]
struct EditorModifierStack(Vec<crate::mesh_modifiers::MeshModifier>);

/// Which modifier kind the ADD MODIFIER button is armed with (cycled by
/// MODIFIER KIND).
#[derive(Resource, Default)]
struct EditorArmedModifier(usize);

/// Parameter cursor for MOD PARAM / MOD VALUE −/+ editing of the last
/// modifier on the selected object.
#[derive(Resource, Default)]
struct EditorModifierParamCursor(usize);

/// PM3: which level template the NEW LEVEL button is armed with.
#[derive(Resource, Default)]
struct EditorArmedLevelTemplate(usize);

/// Destructive level removal requires the same active level to be requested
/// twice. Any other editor action clears the arm.
#[derive(Resource, Default)]
struct EditorLevelDeleteArm(Option<String>);

/// Default instances for each armable modifier kind, in cycle order.
fn armed_modifier_template(index: usize) -> crate::mesh_modifiers::MeshModifier {
    crate::mesh_modifiers::MeshModifier::template(index)
}

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

fn forge_material_colors(family: ForgeMaterialFamily, preset: usize) -> (Vec4, Vec4) {
    match (family, preset % 3) {
        (ForgeMaterialFamily::Toon, 0) => (
            Vec4::new(0.18, 0.72, 1.0, 1.0),
            Vec4::new(0.55, 0.92, 1.0, 1.0),
        ),
        (ForgeMaterialFamily::Toon, 1) => (
            Vec4::new(1.0, 0.32, 0.68, 1.0),
            Vec4::new(1.0, 0.86, 0.32, 1.0),
        ),
        (ForgeMaterialFamily::Toon, _) => (
            Vec4::new(0.34, 0.92, 0.48, 1.0),
            Vec4::new(0.62, 1.0, 0.82, 1.0),
        ),
        (ForgeMaterialFamily::Water, 0) => (
            Vec4::new(0.015, 0.10, 0.28, 0.82),
            Vec4::new(0.05, 0.56, 0.72, 0.72),
        ),
        (ForgeMaterialFamily::Water, 1) => (
            Vec4::new(0.02, 0.20, 0.34, 0.86),
            Vec4::new(0.16, 0.82, 0.92, 0.76),
        ),
        (ForgeMaterialFamily::Water, _) => (
            Vec4::new(0.05, 0.08, 0.18, 0.80),
            Vec4::new(0.34, 0.26, 0.62, 0.68),
        ),
        (ForgeMaterialFamily::Energy, 0) => (
            Vec4::new(1.0, 0.96, 0.72, 1.0),
            Vec4::new(0.10, 0.72, 1.0, 1.0),
        ),
        (ForgeMaterialFamily::Energy, 1) => (
            Vec4::new(1.0, 0.82, 0.96, 1.0),
            Vec4::new(0.72, 0.08, 1.0, 1.0),
        ),
        (ForgeMaterialFamily::Energy, _) => (
            Vec4::new(0.82, 1.0, 0.74, 1.0),
            Vec4::new(0.06, 1.0, 0.36, 1.0),
        ),
        (ForgeMaterialFamily::Shield, 0) => (Vec4::new(0.20, 0.78, 1.0, 1.0), Vec4::ZERO),
        (ForgeMaterialFamily::Shield, 1) => (Vec4::new(1.0, 0.28, 0.76, 1.0), Vec4::ZERO),
        (ForgeMaterialFamily::Shield, _) => (Vec4::new(0.36, 1.0, 0.48, 1.0), Vec4::ZERO),
        (ForgeMaterialFamily::Ice, 0) => (
            Vec4::new(0.84, 0.94, 1.0, 1.0),
            Vec4::new(0.08, 0.42, 0.68, 1.0),
        ),
        (ForgeMaterialFamily::Ice, 1) => (
            Vec4::new(0.96, 0.98, 1.0, 1.0),
            Vec4::new(0.18, 0.68, 0.92, 1.0),
        ),
        (ForgeMaterialFamily::Ice, _) => (
            Vec4::new(0.72, 0.82, 0.94, 1.0),
            Vec4::new(0.22, 0.18, 0.54, 1.0),
        ),
        (ForgeMaterialFamily::Lava, 0) => (
            Vec4::new(0.055, 0.025, 0.018, 1.0),
            Vec4::new(1.0, 0.16, 0.015, 1.0),
        ),
        (ForgeMaterialFamily::Lava, 1) => (
            Vec4::new(0.075, 0.018, 0.025, 1.0),
            Vec4::new(1.0, 0.55, 0.03, 1.0),
        ),
        (ForgeMaterialFamily::Lava, _) => (
            Vec4::new(0.035, 0.025, 0.06, 1.0),
            Vec4::new(0.72, 0.08, 1.0, 1.0),
        ),
    }
}

fn add_published_material(world: &mut World, spec: &ForgeMaterialSpec) -> PublishedMaterialHandle {
    let (primary, secondary) = forge_material_colors(spec.family, spec.preset);
    match spec.family {
        ForgeMaterialFamily::Toon => {
            let mut rim_shape = ToonMaterialUniform::default().rim_shape;
            rim_shape.y = spec.values[5];
            let settings = ToonMaterialUniform {
                color: primary,
                bands: Vec4::new(
                    spec.values[0],
                    spec.values[1],
                    spec.values[2],
                    spec.values[3],
                ),
                rim_color_strength: Vec4::new(
                    secondary.x,
                    secondary.y,
                    secondary.z,
                    spec.values[4],
                ),
                rim_shape,
                ..default()
            };
            PublishedMaterialHandle::Toon(
                world
                    .resource_mut::<Assets<ToonMaterial>>()
                    .add(ToonMaterial { settings }),
            )
        }
        ForgeMaterialFamily::Water => {
            let mut surface = WaterMaterialUniform::default().surface;
            surface.x = spec.values[4];
            surface.y = spec.values[5];
            let settings = WaterMaterialUniform {
                deep_color: primary,
                shallow_color: secondary,
                wave: Vec4::new(
                    spec.values[0],
                    spec.values[1],
                    spec.values[2],
                    spec.values[3],
                ),
                surface,
                ..default()
            };
            PublishedMaterialHandle::Water(
                world
                    .resource_mut::<Assets<WaterMaterial>>()
                    .add(WaterMaterial { settings }),
            )
        }
        ForgeMaterialFamily::Energy => {
            PublishedMaterialHandle::Energy(world.resource_mut::<Assets<EnergyMaterial>>().add(
                EnergyMaterial {
                    settings: EnergyMaterialUniform {
                        core_color: primary,
                        edge_color: secondary,
                        motion: Vec4::new(
                            spec.values[0],
                            spec.values[1],
                            spec.values[2],
                            spec.values[3],
                        ),
                    },
                },
            ))
        }
        ForgeMaterialFamily::Shield => {
            let mut edge = ShieldMaterialUniform::default().edge;
            edge.x = spec.values[4];
            edge.y = spec.values[5];
            let settings = ShieldMaterialUniform {
                color: primary,
                pattern: Vec4::new(
                    spec.values[0],
                    spec.values[1],
                    spec.values[2],
                    spec.values[3],
                ),
                edge,
            };
            PublishedMaterialHandle::Shield(
                world
                    .resource_mut::<Assets<ShieldMaterial>>()
                    .add(ShieldMaterial { settings }),
            )
        }
        ForgeMaterialFamily::Ice => {
            PublishedMaterialHandle::Ice(world.resource_mut::<Assets<IceMaterial>>().add(
                IceMaterial {
                    settings: IceMaterialUniform {
                        snow_color: primary,
                        ice_color: secondary,
                        surface: Vec4::new(
                            spec.values[0],
                            spec.values[1],
                            spec.values[2],
                            spec.values[3],
                        ),
                    },
                },
            ))
        }
        ForgeMaterialFamily::Lava => {
            PublishedMaterialHandle::Lava(world.resource_mut::<Assets<LavaMaterial>>().add(
                LavaMaterial {
                    settings: LavaMaterialUniform {
                        crust_color: primary,
                        hot_color: secondary,
                        motion: Vec4::new(
                            spec.values[0],
                            spec.values[1],
                            spec.values[2],
                            spec.values[3],
                        ),
                    },
                },
            ))
        }
    }
}

fn rebuild_published_content_catalogs(world: &mut World, project: &ForgeProject) {
    let authored = project
        .records
        .iter()
        .filter(|record| {
            record.published_hash.as_ref() == Some(&record.draft_hash)
                && record.published_hash.is_some()
        })
        .filter_map(|record| {
            let persistence::ContentPayload::Material(recipe) =
                project.payloads.get(&record.content_id)?
            else {
                return None;
            };
            Some((
                record.content_id.clone(),
                record.draft_hash.clone(),
                ForgeMaterialSpec::from_recipe(recipe),
            ))
        })
        .collect::<Vec<_>>();
    let mut entries = BTreeMap::new();
    for (content_id, source_hash, spec) in authored {
        let handle = add_published_material(world, &spec);
        entries.insert(
            content_id,
            PublishedMaterialEntry {
                source_hash,
                handle,
            },
        );
    }
    world.resource_mut::<PublishedMaterialCatalog>().entries = entries;

    let recipes = project
        .records
        .iter()
        .filter(|record| {
            record.published_hash.as_ref() == Some(&record.draft_hash)
                && record.published_hash.is_some()
        })
        .filter_map(|record| {
            let kind = ProceduralRecipeKind::from_category(record.category)?;
            let recipe = project
                .payloads
                .get(&record.content_id)?
                .procedural_recipe()?;
            Some((
                record.content_id.clone(),
                PublishedProceduralRecipeEntry {
                    source_hash: record.draft_hash.clone(),
                    kind,
                    seed: recipe.seed,
                    revision: recipe.revision,
                    material_slots: recipe.material_slots.clone(),
                },
            ))
        })
        .collect();
    world
        .resource_mut::<PublishedProceduralRecipeCatalog>()
        .entries = recipes;

    let creatures = project
        .records
        .iter()
        .filter(|record| {
            record.category == persistence::ContentCategory::Creature
                && record.published_hash.as_ref() == Some(&record.draft_hash)
                && record.published_hash.is_some()
        })
        .filter_map(|record| {
            let spec = creature_records::load_creature(project, &record.content_id).ok()?;
            Some((record.content_id.clone(), spec))
        })
        .collect();
    world.resource_mut::<PublishedCreatureCatalog>().entries = creatures;

    let mut epoch = world.resource_mut::<PublishedContentCatalogEpoch>();
    epoch.0 = epoch.0.wrapping_add(1).max(1);
}

/// PM1: point the editor session at the registry's active project before the
/// workspace opens. A missing project file starts as an empty `ForgeProject`
/// (freshly created projects have a valid store on disk already); switching
/// projects also rebuilds the published-content catalogs from the new file.
fn sync_active_project_session(world: &mut World) {
    let active = world
        .resource::<project_registry::ForgeProjectRegistry>()
        .active
        .clone();
    let Some(active) = active else {
        return;
    };
    if world.resource::<EditorProjectSession>().store.path() == active.as_path() {
        return;
    }
    let store = ProjectStore::new(&active, 3);
    let project = match store.load_with_recovery() {
        Ok((project, _)) => project,
        Err(error) => {
            warn!(
                "Active project {} could not be loaded ({error:?}); starting empty.",
                active.display()
            );
            ForgeProject::default()
        }
    };
    rebuild_published_content_catalogs(world, &project);
    reload_preset_library(world, &active);
    let mut session = world.resource_mut::<EditorProjectSession>();
    session.project = project;
    session.store = store;
}

/// PM2: refresh the in-editor preset library from the project's `presets/`
/// folder.
fn reload_preset_library(world: &mut World, project_path: &std::path::Path) {
    let presets = presets::PresetStore::for_project(project_path).load_all();
    let mut library = world.resource_mut::<EditorPresetLibrary>();
    library.presets = presets;
    library.selected = 0;
}

fn bootstrap_published_content_catalogs(world: &mut World) {
    let store = world.resource::<EditorProjectSession>().store.clone();
    if !store.path().exists() {
        return;
    }
    let Ok((project, _)) = store.load_with_recovery() else {
        return;
    };
    rebuild_published_content_catalogs(world, &project);
    reload_preset_library(world, store.path());
    world.resource_mut::<EditorProjectSession>().project = project;
}

fn restore_procedural_standard_material(world: &mut World, entity: Entity) {
    let fallback = world
        .get::<ProceduralStandardMaterialFallback>(entity)
        .map(|fallback| fallback.0.clone());
    let Some(fallback) = fallback else {
        return;
    };
    let mut target = world.entity_mut(entity);
    remove_mesh_material_components(&mut target);
    target.insert(MeshMaterial3d(fallback));
}

fn resolve_procedural_material_bindings(world: &mut World) {
    let targets = world
        .query::<(
            Entity,
            &ProceduralMaterialBinding,
            Option<&ResolvedProceduralMaterialBinding>,
            Option<&MeshMaterial3d<StandardMaterial>>,
        )>()
        .iter(world)
        .map(|(entity, binding, resolved, standard)| {
            (
                entity,
                binding.clone(),
                resolved.cloned(),
                standard.map(|material| material.0.clone()),
            )
        })
        .collect::<Vec<_>>();

    for (entity, binding, resolved, standard) in targets {
        let epoch = world.resource::<PublishedContentCatalogEpoch>().0;
        if resolved.as_ref().is_some_and(|current| {
            current.epoch == epoch
                && current.recipe_id == binding.recipe_id
                && current.slot == binding.slot
        }) {
            continue;
        }
        let desired = world
            .resource::<PublishedProceduralRecipeCatalog>()
            .entries
            .get(&binding.recipe_id)
            .and_then(|recipe| recipe.material_slots.get(&binding.slot).cloned());
        let Some(material_id) = desired else {
            if resolved
                .as_ref()
                .is_some_and(|current| current.material_id.is_some())
            {
                restore_procedural_standard_material(world, entity);
            }
            world
                .entity_mut(entity)
                .insert(ResolvedProceduralMaterialBinding {
                    epoch,
                    recipe_id: binding.recipe_id,
                    slot: binding.slot,
                    material_id: None,
                });
            continue;
        };
        if !world
            .resource::<PublishedMaterialCatalog>()
            .contains(&material_id)
        {
            if resolved
                .as_ref()
                .is_some_and(|current| current.material_id.is_some())
            {
                restore_procedural_standard_material(world, entity);
            }
            world
                .entity_mut(entity)
                .insert(ResolvedProceduralMaterialBinding {
                    epoch,
                    recipe_id: binding.recipe_id,
                    slot: binding.slot,
                    material_id: None,
                });
            continue;
        }
        if world
            .get::<ProceduralStandardMaterialFallback>(entity)
            .is_none()
        {
            if let Some(standard) = standard {
                world
                    .entity_mut(entity)
                    .insert(ProceduralStandardMaterialFallback(standard));
            }
        }
        if apply_catalog_material(world, entity, &material_id) {
            world
                .entity_mut(entity)
                .remove::<EditorMaterialBinding>()
                .insert(ResolvedProceduralMaterialBinding {
                    epoch,
                    recipe_id: binding.recipe_id,
                    slot: binding.slot,
                    material_id: Some(material_id),
                });
        }
    }
}

fn remove_mesh_material_components(entity: &mut EntityWorldMut<'_>) {
    entity.remove::<MeshMaterial3d<StandardMaterial>>();
    entity.remove::<MeshMaterial3d<ToonMaterial>>();
    entity.remove::<MeshMaterial3d<WaterMaterial>>();
    entity.remove::<MeshMaterial3d<EnergyMaterial>>();
    entity.remove::<MeshMaterial3d<ShieldMaterial>>();
    entity.remove::<MeshMaterial3d<IceMaterial>>();
    entity.remove::<MeshMaterial3d<LavaMaterial>>();
}

fn apply_catalog_material(world: &mut World, entity: Entity, content_id: &str) -> bool {
    if world.get::<Mesh3d>(entity).is_none() {
        return false;
    }
    let handle = world
        .resource::<PublishedMaterialCatalog>()
        .entries
        .get(content_id)
        .map(|entry| entry.handle.clone());
    let Some(handle) = handle else {
        return false;
    };
    let mut target = world.entity_mut(entity);
    remove_mesh_material_components(&mut target);
    match handle {
        PublishedMaterialHandle::Toon(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Water(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Energy(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Shield(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Ice(handle) => target.insert(MeshMaterial3d(handle)),
        PublishedMaterialHandle::Lava(handle) => target.insert(MeshMaterial3d(handle)),
    };
    target.insert(EditorMaterialBinding(content_id.into()));
    true
}

fn primitive_standard_color(primitive: EditorPrimitive) -> Color {
    match primitive {
        EditorPrimitive::Empty => Color::WHITE,
        EditorPrimitive::Cube => Color::srgb(0.15, 0.68, 0.94),
        EditorPrimitive::Pillar => Color::srgb(0.92, 0.52, 0.14),
        EditorPrimitive::Beacon => Color::srgb(0.35, 1.0, 0.58),
    }
}

fn clear_catalog_material(world: &mut World, entity: Entity) -> bool {
    let Some(primitive) = world.get::<EditorPrimitive>(entity).copied() else {
        return false;
    };
    if world.get::<Mesh3d>(entity).is_none() {
        return false;
    }
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: primitive_standard_color(primitive),
            metallic: 0.15,
            perceptual_roughness: 0.42,
            ..default()
        });
    let mut target = world.entity_mut(entity);
    remove_mesh_material_components(&mut target);
    target.remove::<EditorMaterialBinding>();
    target.insert(MeshMaterial3d(material));
    true
}

fn sync_forge_material_preview(
    session: Res<EditorProjectSession>,
    registry: Res<EditorRegistryState>,
    mut materials: ForgeMaterialAssets,
    previews: ForgePreviewHandles,
) {
    let Some(record) = session.project.records.get(registry.selected) else {
        return;
    };
    let Some(persistence::ContentPayload::Material(recipe)) =
        session.project.payloads.get(&record.content_id)
    else {
        return;
    };
    let spec = ForgeMaterialSpec::from_recipe(recipe);
    let (primary, secondary) = forge_material_colors(spec.family, spec.preset);
    match spec.family {
        ForgeMaterialFamily::Toon => {
            for handle in previews.toon.iter() {
                if let Some(mut material) = materials.toon.get_mut(&handle.0) {
                    material.settings.color = primary;
                    material.settings.bands = Vec4::new(
                        spec.values[0],
                        spec.values[1],
                        spec.values[2],
                        spec.values[3],
                    );
                    material.settings.rim_color_strength =
                        Vec4::new(secondary.x, secondary.y, secondary.z, spec.values[4]);
                    material.settings.rim_shape.y = spec.values[5];
                }
            }
        }
        ForgeMaterialFamily::Water => {
            for handle in previews.water.iter() {
                if let Some(mut material) = materials.water.get_mut(&handle.0) {
                    material.settings.deep_color = primary;
                    material.settings.shallow_color = secondary;
                    material.settings.wave = Vec4::new(
                        spec.values[0],
                        spec.values[1],
                        spec.values[2],
                        spec.values[3],
                    );
                    material.settings.surface.x = spec.values[4];
                    material.settings.surface.y = spec.values[5];
                }
            }
        }
        ForgeMaterialFamily::Energy => {
            for handle in previews.energy.iter() {
                if let Some(mut material) = materials.energy.get_mut(&handle.0) {
                    material.settings.core_color = primary;
                    material.settings.edge_color = secondary;
                    material.settings.motion = Vec4::new(
                        spec.values[0],
                        spec.values[1],
                        spec.values[2],
                        spec.values[3],
                    );
                }
            }
        }
        ForgeMaterialFamily::Shield => {
            for handle in previews.shield.iter() {
                if let Some(mut material) = materials.shield.get_mut(&handle.0) {
                    material.settings.color = primary;
                    material.settings.pattern = Vec4::new(
                        spec.values[0],
                        spec.values[1],
                        spec.values[2],
                        spec.values[3],
                    );
                    material.settings.edge.x = spec.values[4];
                    material.settings.edge.y = spec.values[5];
                }
            }
        }
        ForgeMaterialFamily::Ice => {
            for handle in previews.ice.iter() {
                if let Some(mut material) = materials.ice.get_mut(&handle.0) {
                    material.settings.snow_color = primary;
                    material.settings.ice_color = secondary;
                    material.settings.surface = Vec4::new(
                        spec.values[0],
                        spec.values[1],
                        spec.values[2],
                        spec.values[3],
                    );
                }
            }
        }
        ForgeMaterialFamily::Lava => {
            for handle in previews.lava.iter() {
                if let Some(mut material) = materials.lava.get_mut(&handle.0) {
                    material.settings.crust_color = primary;
                    material.settings.hot_color = secondary;
                    material.settings.motion = Vec4::new(
                        spec.values[0],
                        spec.values[1],
                        spec.values[2],
                        spec.values[3],
                    );
                }
            }
        }
    }
}

fn sync_world_kit_preview(world: &mut World) {
    let selected = selected_procedural_recipe(world);
    let signature = selected
        .as_ref()
        .and_then(|(content_id, category, recipe)| {
            serde_json::to_string(&(content_id, category, recipe)).ok()
        });
    if world.resource::<WorldKitPreviewState>().signature == signature {
        return;
    }
    let old = world
        .query_filtered::<Entity, With<WorldKitPreview>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in old {
        world.despawn(entity);
    }
    world.resource_mut::<WorldKitPreviewState>().signature = signature;
    let Some((content_id, category, recipe)) = selected else {
        return;
    };
    let camera_transform = world
        .query_filtered::<&Transform, With<EditorCamera>>()
        .iter(world)
        .next()
        .copied()
        .unwrap_or_else(|| Transform::from_xyz(0.0, 4.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y));
    let center = camera_transform.translation + camera_transform.forward() * 7.0
        - camera_transform.up() * 2.2;
    let right = camera_transform.right();
    let up = camera_transform.up();
    let slots = persistence::procedural_material_slots(category);
    let variation =
        ((recipe.seed ^ recipe.revision.wrapping_mul(0x9e37_79b9)) % 101) as f32 / 100.0;
    let parameter = |key: &str| {
        persistence::procedural_parameter_specs(category)
            .iter()
            .find(|spec| spec.key == key)
            .map(|spec| persistence::procedural_parameter_value(&recipe, *spec) as f32)
            .unwrap_or_default()
    };
    for (index, slot) in slots.iter().enumerate() {
        let mesh = match category {
            persistence::ContentCategory::Biome => {
                let radius = (parameter("zone_radius") / 120.0).clamp(0.45, 2.5);
                world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(
                    1.8 * radius,
                    0.18 + parameter("relief") * 0.85,
                    1.8 * radius,
                ))
            }
            persistence::ContentCategory::Road => {
                world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(
                    1.7 * (parameter("width") / 18.0),
                    0.22,
                    2.2 + parameter("curve_radius") / 60.0 + variation,
                ))
            }
            persistence::ContentCategory::Building => {
                world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(
                    1.55 * parameter("width") / 24.0,
                    (parameter("floors") * 0.34).clamp(0.5, 5.0),
                    1.55 * parameter("depth") / 20.0,
                ))
            }
            persistence::ContentCategory::City => {
                world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(
                    1.55 * parameter("block_size") / 80.0,
                    0.8 + parameter("building_density") * 2.2 + variation,
                    1.55 * parameter("block_size") / 80.0,
                ))
            }
            persistence::ContentCategory::Cave => {
                world.resource_mut::<Assets<Mesh>>().add(Sphere::new(
                    0.85 * (parameter("tunnel_width") / 12.0).clamp(0.5, 2.5) + variation * 0.12,
                ))
            }
            _ => continue,
        };
        let fallback = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(Color::srgb(
                0.18 + index as f32 * 0.08,
                0.28 + variation * 0.2,
                0.42 + index as f32 * 0.09,
            ));
        let column = index % 2;
        let row = index / 2;
        let category_offset = match category {
            persistence::ContentCategory::Biome if index == 2 => {
                Vec3::from(up) * parameter("water_level") * 0.035
            }
            persistence::ContentCategory::Road => {
                Vec3::from(up) * index as f32 * parameter("max_grade_deg").to_radians().sin() * 0.65
            }
            persistence::ContentCategory::Cave => {
                Vec3::from(up) * index as f32 * parameter("vertical_layers") * 0.08
            }
            persistence::ContentCategory::City => {
                Vec3::from(right) * parameter("road_width") * 0.018
            }
            _ => Vec3::ZERO,
        };
        let position = center
            + Vec3::from(right) * (column as f32 * 2.2 - 1.1)
            + Vec3::from(up) * (row as f32 * 1.8)
            + category_offset;
        let rotation = if category == persistence::ContentCategory::Road {
            Quat::from_rotation_x(-parameter("max_grade_deg").to_radians() * 0.35)
                * Quat::from_rotation_y(
                    (parameter("loop_count") + parameter("jump_count") * 0.25)
                        * index as f32
                        * 0.08,
                )
        } else {
            Quat::IDENTITY
        };
        let entity = world
            .spawn((
                EditorWorkspaceRoot,
                WorldKitPreview,
                Name::new(format!("{} {} preview", world_recipe_label(category), slot)),
                Mesh3d(mesh),
                MeshMaterial3d(fallback),
                Transform::from_translation(position).with_rotation(rotation),
            ))
            .id();
        if let Some(material_id) = recipe.material_slots.get(*slot) {
            if apply_catalog_material(world, entity, material_id) {
                world.entity_mut(entity).remove::<EditorMaterialBinding>();
            }
        }
    }
    set_editor_status(
        world,
        format!(
            "Previewed {content_id} • seed {} • revision {}",
            recipe.seed, recipe.revision
        ),
    );
}

fn clear_world_kit_sandbox(world: &mut World) {
    let roots = world
        .query_filtered::<Entity, With<WorldKitGeneratedRoot>>()
        .iter(world)
        .collect::<Vec<_>>();
    for root in roots {
        world.despawn(root);
    }
    *world.resource_mut::<WorldKitSandboxState>() = WorldKitSandboxState::default();
}

fn sync_world_kit_sandbox(world: &mut World) {
    let active_content_id = world
        .resource::<WorldKitSandboxState>()
        .active_content_id
        .clone();
    let Some(content_id) = active_content_id else {
        return;
    };
    let selected = {
        let session = world.resource::<EditorProjectSession>();
        session
            .project
            .records
            .iter()
            .find(|record| record.content_id == content_id)
            .and_then(|record| {
                let recipe = session
                    .project
                    .payloads
                    .get(&record.content_id)?
                    .procedural_recipe()?
                    .clone();
                Some((record.category, recipe))
            })
    };
    let Some((category, recipe)) = selected else {
        set_editor_status(
            world,
            format!("Sandbox recipe {content_id} no longer exists"),
        );
        return;
    };
    let Ok(signature) = serde_json::to_string(&(content_id.as_str(), category, &recipe)) else {
        set_editor_status(world, "Sandbox recipe signature could not be serialized");
        return;
    };
    if world
        .resource::<WorldKitSandboxState>()
        .applied_signature
        .as_deref()
        == Some(signature.as_str())
    {
        return;
    }
    let errors = validate_project(&world.resource::<EditorProjectSession>().project)
        .into_iter()
        .map(|error| format!("{error:?}"))
        .filter(|error| error.contains(&content_id))
        .collect::<Vec<_>>();
    if let Some(error) = errors.first() {
        set_editor_status(
            world,
            format!("Sandbox retained last valid {content_id}: {error}"),
        );
        return;
    }
    let parts = match compile_world_kit(category, &recipe) {
        Ok(parts) => parts,
        Err(error) => {
            set_editor_status(
                world,
                format!("Sandbox compile rejected for {content_id}: {error}"),
            );
            return;
        }
    };

    let cube = if let Some(handle) = world.resource::<WorldKitSandboxAssets>().cube.clone() {
        handle
    } else {
        let handle = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(1.0, 1.0, 1.0));
        world.resource_mut::<WorldKitSandboxAssets>().cube = Some(handle.clone());
        handle
    };
    let fallback = if let Some(handle) = world.resource::<WorldKitSandboxAssets>().fallback.clone()
    {
        handle
    } else {
        let handle = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(Color::srgb(0.24, 0.36, 0.52));
        world.resource_mut::<WorldKitSandboxAssets>().fallback = Some(handle.clone());
        handle
    };
    let camera = world
        .query_filtered::<&Transform, With<EditorCamera>>()
        .iter(world)
        .next()
        .copied()
        .unwrap_or_else(|| Transform::from_xyz(0.0, 4.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y));
    let root_transform = Transform::from_translation(
        camera.translation + camera.forward() * 12.0 + camera.right() * 4.0,
    )
    .with_scale(Vec3::splat(0.10));
    let new_root = world
        .spawn((
            EditorWorkspaceRoot,
            WorldKitGeneratedRoot {
                content_id: content_id.clone(),
                signature: signature.clone(),
            },
            Name::new(format!("World Kit Sandbox • {content_id}")),
            root_transform,
            GlobalTransform::default(),
        ))
        .id();
    for part in &parts {
        let mut transform = part.transform;
        transform.scale = part.size;
        let child = world
            .spawn((
                WorldKitGeneratedPart,
                Name::new(part.name.clone()),
                Mesh3d(cube.clone()),
                MeshMaterial3d(fallback.clone()),
                transform,
                crate::physics::prelude::RigidBody::Fixed,
                crate::physics::prelude::Collider::cuboid(0.5, 0.5, 0.5),
            ))
            .id();
        if let Some(material_id) = recipe.material_slots.get(part.material_slot) {
            if apply_catalog_material(world, child, material_id) {
                world.entity_mut(child).remove::<EditorMaterialBinding>();
            }
        }
        world.entity_mut(new_root).add_child(child);
    }

    let old_roots = world
        .query::<(Entity, &WorldKitGeneratedRoot)>()
        .iter(world)
        .filter_map(|(entity, root)| {
            (entity != new_root).then_some((entity, root.signature.clone()))
        })
        .collect::<Vec<_>>();
    for (root, _) in old_roots {
        world.despawn(root);
    }
    world
        .resource_mut::<WorldKitSandboxState>()
        .applied_signature = Some(signature);
    set_editor_status(
        world,
        format!(
            "Atomically compiled {content_id} into {} sandbox parts",
            parts.len()
        ),
    );
}

#[allow(clippy::too_many_arguments)]
fn exit_editor_workspace(
    mut commands: Commands,
    roots: Query<Entity, With<EditorWorkspaceRoot>>,
    editor_cameras: Query<Entity, With<EditorCamera>>,
    mut player_cameras: Query<&mut Camera, With<PlayerCamera>>,
    mut players: Query<&mut PlayerInput, With<Player>>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut physics_time: ResMut<Time<Physics>>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut topology_drag: ResMut<WorldKitTopologyDragState>,
    mut session: ResMut<EditorProjectSession>,
) {
    if let Some(drag) = topology_drag.0.take() {
        if let Some(recipe) = session
            .project
            .payloads
            .get_mut(&drag.content_id)
            .and_then(persistence::ContentPayload::procedural_recipe_mut)
        {
            *recipe = *drag.before;
        }
    }
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
                spawn_editor_button(bar, 102, EditorAction::LevelNext, "NEXT LEVEL");
                spawn_editor_button(bar, 103, EditorAction::LevelCycleTemplate, "LEVEL TEMPLATE");
                spawn_editor_button(bar, 104, EditorAction::LevelNew, "NEW LEVEL");
                spawn_editor_button(bar, 107, EditorAction::LevelMoveEarlier, "LEVEL ◀");
                spawn_editor_button(bar, 108, EditorAction::LevelMoveLater, "LEVEL ▶");
                spawn_editor_button(bar, 109, EditorAction::LevelDelete, "DELETE LEVEL");
                spawn_editor_button(bar, 105, EditorAction::LevelSetStartup, "SET STARTUP");
                spawn_editor_button(bar, 106, EditorAction::PlaytestStartup, "PLAYTEST");
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
                spawn_editor_panel(body, EditorPanelKind::Outliner, "OUTLINER", |panel| {
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

                spawn_editor_panel(body, EditorPanelKind::Inspector, "INSPECTOR", |panel| {
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
                    spawn_editor_button(
                        panel,
                        96,
                        EditorAction::ModifierCycleKind,
                        "MODIFIER KIND",
                    );
                    spawn_editor_button(panel, 97, EditorAction::ModifierAdd, "ADD MODIFIER");
                    spawn_editor_button(
                        panel,
                        98,
                        EditorAction::ModifierRemoveLast,
                        "REMOVE MODIFIER",
                    );
                    spawn_editor_button(panel, 99, EditorAction::ModifierCycleParam, "MOD PARAM");
                    spawn_editor_button(panel, 100, EditorAction::ModifierValueDown, "MOD VALUE −");
                    spawn_editor_button(panel, 101, EditorAction::ModifierValueUp, "MOD VALUE +");
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

                spawn_editor_panel(
                    body,
                    EditorPanelKind::Registry,
                    "CONTENT REGISTRY",
                    |panel| {
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
                            "+ SHADER MATERIAL",
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
                        spawn_editor_button(
                            panel,
                            41,
                            EditorAction::SearchRegistry,
                            "SEARCH RECORDS",
                        );
                        spawn_editor_button(
                            panel,
                            90,
                            EditorAction::PresetCapture,
                            "CAPTURE PRESET",
                        );
                        spawn_editor_button(panel, 91, EditorAction::PresetNext, "NEXT PRESET");
                        spawn_editor_button(panel, 92, EditorAction::PresetApply, "APPLY PRESET");
                        spawn_editor_button(
                            panel,
                            93,
                            EditorAction::PresetCompare,
                            "COMPARE PRESET",
                        );
                        spawn_editor_button(panel, 94, EditorAction::PresetFork, "FORK PRESET");
                        spawn_editor_button(
                            panel,
                            95,
                            EditorAction::PresetNewSeedVariant,
                            "NEW-SEED VARIANT",
                        );
                        spawn_editor_button(
                            panel,
                            42,
                            EditorAction::MaterialCycleFamily,
                            "MATERIAL FAMILY",
                        );
                        spawn_editor_button(
                            panel,
                            43,
                            EditorAction::MaterialCyclePreset,
                            "MATERIAL PRESET",
                        );
                        spawn_editor_button(
                            panel,
                            44,
                            EditorAction::MaterialNextParameter,
                            "NEXT PARAMETER",
                        );
                        spawn_editor_button(panel, 45, EditorAction::MaterialDecrease, "VALUE  −");
                        spawn_editor_button(panel, 46, EditorAction::MaterialIncrease, "VALUE  +");
                        spawn_editor_button(
                            panel,
                            47,
                            EditorAction::MaterialReset,
                            "RESET MATERIAL",
                        );
                        spawn_editor_button(
                            panel,
                            48,
                            EditorAction::ApplySelectedMaterial,
                            "APPLY TO OBJECT",
                        );
                        spawn_editor_button(
                            panel,
                            49,
                            EditorAction::ClearObjectMaterial,
                            "CLEAR OBJECT MATERIAL",
                        );
                        spawn_editor_button(
                            panel,
                            50,
                            EditorAction::CycleRecipeKind,
                            "WORLD RECIPE TYPE",
                        );
                        spawn_editor_button(
                            panel,
                            51,
                            EditorAction::CreateRecipeRecord,
                            "+ WORLD RECIPE",
                        );
                        spawn_editor_button(
                            panel,
                            52,
                            EditorAction::CopySelectedMaterial,
                            "COPY MATERIAL",
                        );
                        spawn_editor_button(
                            panel,
                            53,
                            EditorAction::RecipeNextSlot,
                            "NEXT MATERIAL SLOT",
                        );
                        spawn_editor_button(
                            panel,
                            54,
                            EditorAction::BindRecipeMaterial,
                            "BIND COPIED MATERIAL",
                        );
                        spawn_editor_button(
                            panel,
                            55,
                            EditorAction::ClearRecipeMaterial,
                            "CLEAR RECIPE SLOT",
                        );
                        spawn_editor_button(
                            panel,
                            56,
                            EditorAction::RegenerateRecipePreview,
                            "REGENERATE PREVIEW",
                        );
                        spawn_editor_button(
                            panel,
                            57,
                            EditorAction::RecipeNextParameter,
                            "NEXT RECIPE PARAMETER",
                        );
                        spawn_editor_button(
                            panel,
                            58,
                            EditorAction::RecipeParameterDecrease,
                            "RECIPE VALUE  −",
                        );
                        spawn_editor_button(
                            panel,
                            59,
                            EditorAction::RecipeParameterIncrease,
                            "RECIPE VALUE  +",
                        );
                        spawn_editor_button(
                            panel,
                            60,
                            EditorAction::RecipeResetParameters,
                            "RESET RECIPE VALUES",
                        );
                        spawn_editor_button(panel, 61, EditorAction::RecipeSeedDecrease, "SEED  −");
                        spawn_editor_button(panel, 62, EditorAction::RecipeSeedIncrease, "SEED  +");
                        spawn_editor_button(
                            panel,
                            63,
                            EditorAction::ValidateSelectedRecipe,
                            "VALIDATE WORLD RECIPE",
                        );
                        spawn_editor_button(
                            panel,
                            64,
                            EditorAction::SeedDefaultTopology,
                            "SEED DEFAULT TOPOLOGY",
                        );
                        spawn_editor_button(
                            panel,
                            67,
                            EditorAction::TopologyPrevious,
                            "◀ PREVIOUS TOPOLOGY ITEM",
                        );
                        spawn_editor_button(
                            panel,
                            68,
                            EditorAction::TopologyNext,
                            "NEXT TOPOLOGY ITEM ▶",
                        );
                        spawn_editor_button(
                            panel,
                            69,
                            EditorAction::TopologyMoveXNegative,
                            "TOPOLOGY X  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            70,
                            EditorAction::TopologyMoveXPositive,
                            "TOPOLOGY X  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            71,
                            EditorAction::TopologyMoveYNegative,
                            "TOPOLOGY Y  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            72,
                            EditorAction::TopologyMoveYPositive,
                            "TOPOLOGY Y  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            73,
                            EditorAction::TopologyMoveZNegative,
                            "TOPOLOGY Z  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            74,
                            EditorAction::TopologyMoveZPositive,
                            "TOPOLOGY Z  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            75,
                            EditorAction::TopologyMarkEdgeStart,
                            "MARK EDGE START",
                        );
                        spawn_editor_button(
                            panel,
                            76,
                            EditorAction::TopologyCycleEdgeKind,
                            "EDGE TYPE",
                        );
                        spawn_editor_button(
                            panel,
                            77,
                            EditorAction::TopologyConnectEdge,
                            "CONNECT TO SELECTED NODE",
                        );
                        spawn_editor_button(
                            panel,
                            78,
                            EditorAction::TopologyRemoveEdge,
                            "REMOVE MATCHING EDGE",
                        );
                        spawn_editor_button(
                            panel,
                            79,
                            EditorAction::RoadTangentNextAxis,
                            "ROAD TANGENT AXIS",
                        );
                        spawn_editor_button(
                            panel,
                            80,
                            EditorAction::RoadTangentDecrease,
                            "TANGENT VALUE  −",
                        );
                        spawn_editor_button(
                            panel,
                            81,
                            EditorAction::RoadTangentIncrease,
                            "TANGENT VALUE  +",
                        );
                        spawn_editor_button(
                            panel,
                            82,
                            EditorAction::RoadTangentReset,
                            "RESET AUTO TANGENT",
                        );
                        spawn_editor_button(
                            panel,
                            120,
                            EditorAction::RoadTangentAutoRoundAll,
                            "AUTO ROUND ALL SPLINE POINTS",
                        );
                        spawn_editor_button(
                            panel,
                            83,
                            EditorAction::RoadMarkJunctionStart,
                            "MARK ROAD JUNCTION START",
                        );
                        spawn_editor_button(
                            panel,
                            84,
                            EditorAction::RoadCycleJunctionKind,
                            "ROAD JUNCTION TYPE",
                        );
                        spawn_editor_button(
                            panel,
                            85,
                            EditorAction::RoadConnectJunction,
                            "CONNECT ROAD JUNCTION",
                        );
                        spawn_editor_button(
                            panel,
                            86,
                            EditorAction::RoadRemoveJunction,
                            "REMOVE ROAD JUNCTION",
                        );
                        spawn_editor_button(
                            panel,
                            87,
                            EditorAction::SocketNext,
                            "NEXT NODE SOCKET",
                        );
                        spawn_editor_button(
                            panel,
                            88,
                            EditorAction::SocketCycleKind,
                            "NEW SOCKET TYPE",
                        );
                        spawn_editor_button(panel, 89, EditorAction::SocketCreate, "+ NODE SOCKET");
                        spawn_editor_button(
                            panel,
                            90,
                            EditorAction::SocketDelete,
                            "DELETE NODE SOCKET",
                        );
                        spawn_editor_button(
                            panel,
                            91,
                            EditorAction::SocketMoveXNegative,
                            "SOCKET X  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            92,
                            EditorAction::SocketMoveXPositive,
                            "SOCKET X  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            93,
                            EditorAction::SocketMoveYNegative,
                            "SOCKET Y  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            94,
                            EditorAction::SocketMoveYPositive,
                            "SOCKET Y  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            95,
                            EditorAction::SocketMoveZNegative,
                            "SOCKET Z  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            96,
                            EditorAction::SocketMoveZPositive,
                            "SOCKET Z  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            97,
                            EditorAction::TerrainOriginXNegative,
                            "TERRAIN ORIGIN X  −250M",
                        );
                        spawn_editor_button(
                            panel,
                            98,
                            EditorAction::TerrainOriginXPositive,
                            "TERRAIN ORIGIN X  +250M",
                        );
                        spawn_editor_button(
                            panel,
                            99,
                            EditorAction::TerrainOriginZNegative,
                            "TERRAIN ORIGIN Z  −250M",
                        );
                        spawn_editor_button(
                            panel,
                            100,
                            EditorAction::TerrainOriginZPositive,
                            "TERRAIN ORIGIN Z  +250M",
                        );
                        spawn_editor_button(
                            panel,
                            101,
                            EditorAction::TerrainClearanceDecrease,
                            "TERRAIN CLEARANCE  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            102,
                            EditorAction::TerrainClearanceIncrease,
                            "TERRAIN CLEARANCE  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            103,
                            EditorAction::TerrainProjectSelected,
                            "PROJECT SELECTED TO TERRAIN",
                        );
                        spawn_editor_button(
                            panel,
                            104,
                            EditorAction::TerrainProjectAll,
                            "PROJECT ALL TO TERRAIN",
                        );
                        spawn_editor_button(
                            panel,
                            65,
                            EditorAction::CompileSandboxRoot,
                            "COMPILE SANDBOX ROOT",
                        );
                        spawn_editor_button(
                            panel,
                            66,
                            EditorAction::ClearSandboxRoot,
                            "CLEAR SANDBOX ROOT",
                        );
                    },
                );
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
    kind: EditorPanelKind,
    title: &str,
    content: impl FnOnce(&mut ChildSpawnerCommands),
) {
    // Initial docking spots at the default 1280x720 window; every panel is a
    // movable, minimizable tool window from there on.
    let position = match kind {
        EditorPanelKind::Outliner => Vec2::new(12.0, 96.0),
        EditorPanelKind::Inspector => Vec2::new(497.0, 96.0),
        EditorPanelKind::Registry => Vec2::new(982.0, 96.0),
    };
    crate::tool_windows::spawn_tool_window(
        parent,
        title,
        position,
        crate::tool_windows::ToolWindowStyle::default(),
        (kind, ScrollPosition::default()),
        content,
    );
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

fn keep_editor_focus_in_view(
    focus: Res<EditorFocus>,
    buttons: Query<(&EditorButton, &GlobalTransform, &ComputedNode)>,
    mut panels: Query<(
        &EditorPanelKind,
        &GlobalTransform,
        &ComputedNode,
        &mut ScrollPosition,
    )>,
) {
    let panel_kind = match focus.0 {
        5..=11 => EditorPanelKind::Outliner,
        12..=23 => EditorPanelKind::Inspector,
        33.. => EditorPanelKind::Registry,
        _ => return,
    };
    let Some((_, button_transform, button_node)) = buttons
        .iter()
        .find(|(button, _, _)| button.order == focus.0)
    else {
        return;
    };
    let Some((_, panel_transform, panel_node, mut scroll)) = panels
        .iter_mut()
        .find(|(kind, _, _, _)| **kind == panel_kind)
    else {
        return;
    };
    let max_y = ((panel_node.content_size().y - panel_node.size().y)
        * panel_node.inverse_scale_factor())
    .max(0.0);
    if max_y <= 0.0 {
        return;
    }
    let margin = 18.0;
    let panel_center = panel_transform.translation().y;
    let panel_half = panel_node.size().y * 0.5;
    let visible_min = panel_center - panel_half + margin;
    let visible_max = panel_center + panel_half - margin;
    let button_center = button_transform.translation().y;
    let button_half = button_node.size().y * 0.5;
    let button_min = button_center - button_half;
    let button_max = button_center + button_half;
    let delta = editor_focus_reveal_delta(visible_min, visible_max, button_min, button_max);
    scroll.y = (scroll.y + delta).clamp(0.0, max_y);
}

fn editor_focus_reveal_delta(
    visible_min: f32,
    visible_max: f32,
    item_min: f32,
    item_max: f32,
) -> f32 {
    if item_min < visible_min {
        visible_min - item_min
    } else if item_max > visible_max {
        visible_max - item_max
    } else {
        0.0
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

fn topology_drag_amount(
    cursor_delta: Vec2,
    screen_axis: Vec2,
    recipe_units_per_pixel: f32,
    snap: f32,
) -> f32 {
    let raw = cursor_delta.dot(screen_axis) * recipe_units_per_pixel;
    (raw / snap).round() * snap
}

fn topology_position_mut(
    category: persistence::ContentCategory,
    recipe: &mut ProceduralRecipeDraft,
    element_index: usize,
) -> Option<&mut [f32; 3]> {
    if category == persistence::ContentCategory::Road {
        recipe
            .spline_points
            .get_mut(element_index)
            .map(|point| &mut point.position)
    } else {
        recipe
            .topology_nodes
            .get_mut(element_index)
            .map(|node| &mut node.position)
    }
}

#[allow(clippy::too_many_arguments)]
fn world_kit_topology_gizmo_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<EditorCamera>>,
    sandbox_roots: Query<(&WorldKitGeneratedRoot, &GlobalTransform)>,
    filter: Res<EditorOutlinerFilter>,
    text_capture: Res<EditorTextInputCapture>,
    editor_drag: Res<EditorDragState>,
    settings: Res<EditorGizmoSettings>,
    registry: Res<EditorRegistryState>,
    mut session: ResMut<EditorProjectSession>,
    mut drag_state: ResMut<WorldKitTopologyDragState>,
    mut pending: ResMut<EditorPendingTransactions>,
    mut status: ResMut<EditorRuntimeStatus>,
) {
    if filter.active
        || text_capture.0
        || registry.rename_active
        || registry.search_active
        || editor_drag.0.is_some()
    {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left)
        && drag_state.0.is_none()
        && cursor.y >= 126.0
        && cursor.y <= window.height() - 68.0
        && cursor.x >= 314.0
        && cursor.x <= window.width() - 314.0
    {
        let Some(record) = session.project.records.get(registry.selected) else {
            return;
        };
        let Some(recipe) = session
            .project
            .payloads
            .get(&record.content_id)
            .and_then(persistence::ContentPayload::procedural_recipe)
        else {
            return;
        };
        let count = topology_element_count(record.category, recipe);
        if count == 0 {
            return;
        }
        let element_index = registry.topology_element % count;
        let Some((element_id, position, _)) =
            selected_topology_element(record.category, recipe, element_index)
        else {
            return;
        };
        let Some((_, root_transform)) = sandbox_roots
            .iter()
            .find(|(root, _)| root.content_id == record.content_id)
        else {
            return;
        };
        let local = Vec3::from_array(position);
        let center = root_transform.transform_point(local);
        let Ok(center_screen) = camera.world_to_viewport(camera_transform, center) else {
            return;
        };
        let mut hit: Option<(f32, usize, Vec2, f32)> = None;
        for (axis_index, axis) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
            let endpoint = root_transform.transform_point(local + axis * 10.0);
            let Ok(endpoint_screen) = camera.world_to_viewport(camera_transform, endpoint) else {
                continue;
            };
            let screen_vector = endpoint_screen - center_screen;
            let screen_length = screen_vector.length();
            if screen_length <= 3.0 {
                continue;
            }
            let distance = point_segment_distance(cursor, center_screen, endpoint_screen);
            if distance <= 18.0
                && hit
                    .as_ref()
                    .is_none_or(|(nearest, _, _, _)| distance < *nearest)
            {
                hit = Some((
                    distance,
                    axis_index,
                    screen_vector / screen_length,
                    10.0 / screen_length,
                ));
            }
        }
        if let Some((_, axis_index, screen_axis, recipe_units_per_pixel)) = hit {
            status.message = format!("Dragging topology {element_id} axis {axis_index}");
            drag_state.0 = Some(ActiveTopologyDrag {
                content_id: record.content_id.clone(),
                category: record.category,
                element_index,
                axis_index,
                start_cursor: cursor,
                screen_axis,
                recipe_units_per_pixel,
                before: Box::new(recipe.clone()),
            });
        }
    }

    let Some(drag) = drag_state.0.as_ref() else {
        return;
    };
    if mouse.pressed(MouseButton::Left) {
        let amount = topology_drag_amount(
            cursor - drag.start_cursor,
            drag.screen_axis,
            drag.recipe_units_per_pixel,
            settings.translation_snap(),
        );
        let mut preview = (*drag.before).clone();
        if let Some(position) =
            topology_position_mut(drag.category, &mut preview, drag.element_index)
        {
            position[drag.axis_index] += amount;
            if let Some(recipe) = session
                .project
                .payloads
                .get_mut(&drag.content_id)
                .and_then(persistence::ContentPayload::procedural_recipe_mut)
            {
                *recipe = preview;
                status.message = format!("Topology drag preview: {amount:+.2}m");
            }
        }
    }

    if mouse.just_released(MouseButton::Left) {
        let drag = drag_state.0.take().expect("topology drag exists");
        let after = session
            .project
            .payloads
            .get(&drag.content_id)
            .and_then(persistence::ContentPayload::procedural_recipe)
            .cloned();
        if let Some(recipe) = session
            .project
            .payloads
            .get_mut(&drag.content_id)
            .and_then(persistence::ContentPayload::procedural_recipe_mut)
        {
            *recipe = (*drag.before).clone();
        }
        let Some(after) = after else {
            status.message = "Topology drag cancelled because its recipe disappeared".into();
            return;
        };
        if *drag.before == after {
            status.message = "Topology drag cancelled".into();
            return;
        }
        pending.0.push(EditorTransaction {
            description: "Drag World Kit topology element".into(),
            commands: vec![EditorCommand::SetProceduralRecipe {
                content_id: drag.content_id,
                before: drag.before,
                after: Box::new(after),
            }],
        });
    } else if !mouse.pressed(MouseButton::Left) {
        let drag = drag_state.0.take().expect("topology drag exists");
        if let Some(recipe) = session
            .project
            .payloads
            .get_mut(&drag.content_id)
            .and_then(persistence::ContentPayload::procedural_recipe_mut)
        {
            *recipe = *drag.before;
        }
        status.message = "Topology drag cancelled".into();
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
    topology_drag: Res<WorldKitTopologyDragState>,
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
    if filter.active
        || drag.0.is_some()
        || topology_drag.0.is_some()
        || !mouse.just_pressed(MouseButton::Left)
    {
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

#[derive(Debug, Clone, Copy)]
enum MaterialEdit {
    CycleFamily,
    CyclePreset,
    NextParameter,
    Adjust(f32),
    Reset,
}

const WORLD_RECIPE_CATEGORIES: [persistence::ContentCategory; 5] = [
    persistence::ContentCategory::Biome,
    persistence::ContentCategory::Road,
    persistence::ContentCategory::Building,
    persistence::ContentCategory::Cave,
    persistence::ContentCategory::City,
];

fn world_recipe_label(category: persistence::ContentCategory) -> &'static str {
    match category {
        persistence::ContentCategory::Biome => "Biome",
        persistence::ContentCategory::Road => "Road",
        persistence::ContentCategory::Building => "Building",
        persistence::ContentCategory::Cave => "Cave",
        persistence::ContentCategory::City => "City",
        _ => "Unsupported",
    }
}

fn world_recipe_default_name(category: persistence::ContentCategory) -> &'static str {
    match category {
        persistence::ContentCategory::Biome => "Main World",
        persistence::ContentCategory::Road => "Speed Network",
        persistence::ContentCategory::Building => "World",
        persistence::ContentCategory::Cave => "Secret Network",
        persistence::ContentCategory::City => "Main",
        _ => "World Recipe",
    }
}

fn selected_procedural_recipe(
    world: &World,
) -> Option<(String, persistence::ContentCategory, ProceduralRecipeDraft)> {
    let registry = world.resource::<EditorRegistryState>();
    let session = world.resource::<EditorProjectSession>();
    let record = session.project.records.get(registry.selected)?;
    let recipe = session
        .project
        .payloads
        .get(&record.content_id)?
        .procedural_recipe()?
        .clone();
    Some((record.content_id.clone(), record.category, recipe))
}

fn topology_element_count(
    category: persistence::ContentCategory,
    recipe: &ProceduralRecipeDraft,
) -> usize {
    if category == persistence::ContentCategory::Road {
        recipe.spline_points.len()
    } else {
        recipe.topology_nodes.len()
    }
}

fn selected_topology_element(
    category: persistence::ContentCategory,
    recipe: &ProceduralRecipeDraft,
    selected: usize,
) -> Option<(&str, [f32; 3], &'static str)> {
    if category == persistence::ContentCategory::Road {
        if recipe.spline_points.is_empty() {
            return None;
        }
        let point = recipe
            .spline_points
            .get(selected % recipe.spline_points.len())?;
        Some((&point.point_id, point.position, "spline point"))
    } else {
        if recipe.topology_nodes.is_empty() {
            return None;
        }
        let node = recipe
            .topology_nodes
            .get(selected % recipe.topology_nodes.len())?;
        Some((&node.node_id, node.position, "topology node"))
    }
}

fn cycle_selected_topology_element(world: &mut World, direction: isize) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a World Kit recipe before editing topology");
        return;
    };
    let count = topology_element_count(category, &recipe);
    if count == 0 {
        set_editor_status(world, "Seed topology before selecting a point or node");
        return;
    }
    let selected = {
        let mut registry = world.resource_mut::<EditorRegistryState>();
        registry.topology_element = if direction < 0 {
            registry
                .topology_element
                .checked_sub(1)
                .unwrap_or(count - 1)
        } else {
            (registry.topology_element + 1) % count
        };
        registry.topology_socket = 0;
        registry.topology_element
    };
    let (id, position, kind) = selected_topology_element(category, &recipe, selected)
        .expect("nonempty topology has a selected element");
    set_editor_status(
        world,
        format!(
            "Selected {kind} {id} at [{:.2}, {:.2}, {:.2}]",
            position[0], position[1], position[2]
        ),
    );
}

fn move_selected_topology_element(world: &mut World, axis: usize, direction: f32) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a World Kit recipe before editing topology");
        return;
    };
    let count = topology_element_count(category, &recipe);
    if count == 0 {
        set_editor_status(world, "Seed topology before moving a point or node");
        return;
    }
    let selected = world.resource::<EditorRegistryState>().topology_element % count;
    let (id, _, kind) = selected_topology_element(category, &recipe, selected)
        .expect("nonempty topology has a selected element");
    let id = id.to_owned();
    let snap = world.resource::<EditorGizmoSettings>().translation_snap();
    execute_recipe_edit(
        world,
        format!("Move {kind} {id} axis {axis} by {:.2}m", direction * snap),
        move |recipe| {
            let position = if category == persistence::ContentCategory::Road {
                &mut recipe.spline_points[selected].position
            } else {
                &mut recipe.topology_nodes[selected].position
            };
            position[axis] += direction * snap;
        },
    );
}

fn terrain_projection_y(
    category: persistence::ContentCategory,
    recipe: &ProceduralRecipeDraft,
    element_index: usize,
) -> Option<f32> {
    let (position, half_height) = if category == persistence::ContentCategory::Road {
        (recipe.spline_points.get(element_index)?.position, 0.0)
    } else {
        let node = recipe.topology_nodes.get(element_index)?;
        (node.position, node.size[1] * 0.5)
    };
    let projection = recipe.terrain_projection;
    let world_x = projection.world_origin[0] + position[0];
    let world_z = projection.world_origin[1] + position[2];
    Some(
        crate::plugins::world_plugin::terrain_surface_y(world_x, world_z, recipe.seed)
            + half_height
            + projection.clearance,
    )
}

fn adjust_terrain_origin(world: &mut World, axis: usize, direction: f32) {
    let Some((_, _, _)) = selected_procedural_recipe(world) else {
        set_editor_status(
            world,
            "Select a World Kit recipe before moving its terrain origin",
        );
        return;
    };
    let delta = direction * 250.0;
    execute_recipe_edit(
        world,
        format!("Move terrain origin {} by {delta:.0}m", ["X", "Z"][axis]),
        move |recipe| {
            recipe.terrain_projection.world_origin[axis] =
                (recipe.terrain_projection.world_origin[axis] + delta).clamp(-10_000.0, 10_000.0);
        },
    );
}

fn adjust_terrain_clearance(world: &mut World, direction: f32) {
    let Some((_, _, _)) = selected_procedural_recipe(world) else {
        set_editor_status(
            world,
            "Select a World Kit recipe before changing terrain clearance",
        );
        return;
    };
    let delta = direction * world.resource::<EditorGizmoSettings>().translation_snap();
    execute_recipe_edit(
        world,
        format!("Adjust terrain clearance by {delta:.2}m"),
        move |recipe| {
            recipe.terrain_projection.clearance =
                (recipe.terrain_projection.clearance + delta).clamp(-250.0, 250.0);
        },
    );
}

fn project_recipe_to_terrain(world: &mut World, all: bool) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a World Kit recipe before terrain projection");
        return;
    };
    let count = topology_element_count(category, &recipe);
    if count == 0 {
        set_editor_status(world, "Seed topology before terrain projection");
        return;
    }
    let selected = world.resource::<EditorRegistryState>().topology_element % count;
    let indices = if all {
        (0..count).collect::<Vec<_>>()
    } else {
        vec![selected]
    };
    let projected = indices
        .iter()
        .filter_map(|index| terrain_projection_y(category, &recipe, *index).map(|y| (*index, y)))
        .collect::<Vec<_>>();
    let description = if all {
        format!("Project {} topology elements to terrain", projected.len())
    } else {
        let (id, _, kind) = selected_topology_element(category, &recipe, selected)
            .expect("nonempty topology has a selected element");
        format!("Project {kind} {id} to terrain")
    };
    execute_recipe_edit(world, description, move |recipe| {
        for (index, y) in projected {
            if let Some(position) = topology_position_mut(category, recipe, index) {
                position[1] = y;
            }
        }
    });
}

const TOPOLOGY_EDGE_KINDS: [WorldTopologyEdgeKind; 4] = [
    WorldTopologyEdgeKind::Door,
    WorldTopologyEdgeKind::Stair,
    WorldTopologyEdgeKind::Tunnel,
    WorldTopologyEdgeKind::Portal,
];

const TOPOLOGY_SOCKET_KINDS: [WorldTopologySocketKind; 5] = [
    WorldTopologySocketKind::Doorway,
    WorldTopologySocketKind::StairLanding,
    WorldTopologySocketKind::Encounter,
    WorldTopologySocketKind::Item,
    WorldTopologySocketKind::Portal,
];

fn socket_indices_for_node(recipe: &ProceduralRecipeDraft, node_id: &str) -> Vec<usize> {
    recipe
        .topology_sockets
        .iter()
        .enumerate()
        .filter_map(|(index, socket)| (socket.node_id == node_id).then_some(index))
        .collect()
}

fn selected_socket_index(
    recipe: &ProceduralRecipeDraft,
    node_id: &str,
    selected: usize,
) -> Option<usize> {
    let indices = socket_indices_for_node(recipe, node_id);
    if indices.is_empty() {
        return None;
    }
    indices.get(selected % indices.len()).copied()
}

fn selected_topology_node_id(
    category: persistence::ContentCategory,
    recipe: &ProceduralRecipeDraft,
    selected: usize,
) -> Option<&str> {
    if category == persistence::ContentCategory::Road {
        return None;
    }
    selected_topology_element(category, recipe, selected).map(|(id, _, _)| id)
}

fn cycle_selected_socket(world: &mut World) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a topology node before choosing a socket");
        return;
    };
    let selected_node = world.resource::<EditorRegistryState>().topology_element;
    let Some(node_id) = selected_topology_node_id(category, &recipe, selected_node) else {
        set_editor_status(world, "Road points do not use room doorway sockets");
        return;
    };
    let indices = socket_indices_for_node(&recipe, node_id);
    if indices.is_empty() {
        set_editor_status(world, "This node has no sockets; create one first");
        return;
    }
    let selected = {
        let mut registry = world.resource_mut::<EditorRegistryState>();
        registry.topology_socket = (registry.topology_socket + 1) % indices.len();
        registry.topology_socket
    };
    let socket = &recipe.topology_sockets[indices[selected]];
    set_editor_status(
        world,
        format!("Selected {:?} socket {}", socket.kind, socket.socket_id),
    );
}

fn cycle_topology_socket_kind(world: &mut World) {
    let kind = {
        let mut registry = world.resource_mut::<EditorRegistryState>();
        registry.topology_socket_kind =
            (registry.topology_socket_kind + 1) % TOPOLOGY_SOCKET_KINDS.len();
        TOPOLOGY_SOCKET_KINDS[registry.topology_socket_kind]
    };
    set_editor_status(world, format!("New topology socket type: {kind:?}"));
}

fn create_topology_socket(world: &mut World) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a topology node before creating a socket");
        return;
    };
    let registry = world.resource::<EditorRegistryState>();
    let Some(node_id) = selected_topology_node_id(category, &recipe, registry.topology_element)
    else {
        set_editor_status(world, "Road points do not use room doorway sockets");
        return;
    };
    let node_id = node_id.to_owned();
    let node = recipe
        .topology_nodes
        .iter()
        .find(|node| node.node_id == node_id)
        .expect("selected topology node exists");
    let kind = TOPOLOGY_SOCKET_KINDS[registry.topology_socket_kind % TOPOLOGY_SOCKET_KINDS.len()];
    let kind_id = format!("{kind:?}").to_lowercase();
    let mut suffix = socket_indices_for_node(&recipe, &node_id).len() + 1;
    let socket_id = loop {
        let candidate = format!("{node_id}.socket.{kind_id}.{suffix}");
        if !recipe
            .topology_sockets
            .iter()
            .any(|socket| socket.socket_id == candidate)
        {
            break candidate;
        }
        suffix += 1;
    };
    let local_position = [0.0, 0.0, node.size[2] * 0.5];
    execute_recipe_edit(world, format!("Create socket {socket_id}"), move |recipe| {
        recipe.topology_sockets.push(WorldTopologySocketDraft {
            socket_id,
            node_id,
            kind,
            local_position,
            facing: [0.0, 0.0, 1.0],
            size: match kind {
                WorldTopologySocketKind::Doorway | WorldTopologySocketKind::StairLanding => {
                    [2.5, 3.2]
                }
                _ => [1.5, 1.5],
            },
        });
    });
}

fn move_selected_topology_socket(world: &mut World, axis: usize, direction: f32) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a topology socket before moving it");
        return;
    };
    let registry = world.resource::<EditorRegistryState>();
    let Some(node_id) = selected_topology_node_id(category, &recipe, registry.topology_element)
    else {
        set_editor_status(world, "Road points do not use room doorway sockets");
        return;
    };
    let Some(socket_index) = selected_socket_index(&recipe, node_id, registry.topology_socket)
    else {
        set_editor_status(world, "Create a socket on this node before moving it");
        return;
    };
    let node = recipe
        .topology_nodes
        .iter()
        .find(|node| node.node_id == node_id)
        .expect("selected socket node exists");
    let half_extent = node.size[axis] * 0.5;
    let socket_id = recipe.topology_sockets[socket_index].socket_id.clone();
    let step = world.resource::<EditorGizmoSettings>().translation_snap();
    execute_recipe_edit(world, format!("Move socket {socket_id}"), move |recipe| {
        let socket = &mut recipe.topology_sockets[socket_index];
        socket.local_position[axis] =
            (socket.local_position[axis] + direction * step).clamp(-half_extent, half_extent);
        let facing = Vec3::from_array(socket.facing);
        if facing.length_squared() <= 0.25 {
            socket.facing = [0.0, 0.0, 1.0];
        }
    });
}

fn delete_selected_topology_socket(world: &mut World) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a topology socket before deleting it");
        return;
    };
    let registry = world.resource::<EditorRegistryState>();
    let Some(node_id) = selected_topology_node_id(category, &recipe, registry.topology_element)
    else {
        set_editor_status(world, "Road points do not use room doorway sockets");
        return;
    };
    let Some(socket_index) = selected_socket_index(&recipe, node_id, registry.topology_socket)
    else {
        set_editor_status(world, "This node has no socket to delete");
        return;
    };
    let socket_id = recipe.topology_sockets[socket_index].socket_id.clone();
    execute_recipe_edit(world, format!("Delete socket {socket_id}"), move |recipe| {
        recipe.topology_sockets.remove(socket_index);
        for edge in &mut recipe.topology_edges {
            if edge.from_socket.as_deref() == Some(socket_id.as_str()) {
                edge.from_socket = None;
            }
            if edge.to_socket.as_deref() == Some(socket_id.as_str()) {
                edge.to_socket = None;
            }
        }
    });
}

fn mark_topology_edge_start(world: &mut World) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a World Kit recipe before authoring an edge");
        return;
    };
    if category == persistence::ContentCategory::Road {
        set_editor_status(world, "Road splines do not use room/zone topology edges");
        return;
    }
    let count = topology_element_count(category, &recipe);
    if count == 0 {
        set_editor_status(world, "Seed topology before marking an edge endpoint");
        return;
    }
    let selected = world.resource::<EditorRegistryState>().topology_element % count;
    let (node_id, _, _) = selected_topology_element(category, &recipe, selected)
        .expect("nonempty topology has a selected node");
    let node_id = node_id.to_owned();
    let start_socket = {
        let registry = world.resource::<EditorRegistryState>();
        selected_socket_index(&recipe, &node_id, registry.topology_socket)
            .map(|index| recipe.topology_sockets[index].socket_id.clone())
    };
    {
        let mut registry = world.resource_mut::<EditorRegistryState>();
        registry.topology_edge_start = Some(node_id.clone());
        registry.topology_edge_start_socket = start_socket.clone();
    }
    set_editor_status(
        world,
        format!(
            "Marked {node_id}{}; select a destination node",
            start_socket
                .as_deref()
                .map(|socket| format!(" via {socket}"))
                .unwrap_or_default()
        ),
    );
}

fn cycle_topology_edge_kind(world: &mut World) {
    let kind = {
        let mut registry = world.resource_mut::<EditorRegistryState>();
        registry.topology_edge_kind = (registry.topology_edge_kind + 1) % TOPOLOGY_EDGE_KINDS.len();
        TOPOLOGY_EDGE_KINDS[registry.topology_edge_kind]
    };
    set_editor_status(world, format!("Topology edge type: {kind:?}"));
}

fn edit_topology_edge(world: &mut World, connect: bool) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a World Kit recipe before authoring an edge");
        return;
    };
    if category == persistence::ContentCategory::Road {
        set_editor_status(world, "Road splines do not use room/zone topology edges");
        return;
    }
    let count = topology_element_count(category, &recipe);
    if count == 0 {
        set_editor_status(world, "Seed topology before authoring an edge");
        return;
    }
    let (start, stored_start_socket, kind, selected, socket_selection) = {
        let registry = world.resource::<EditorRegistryState>();
        (
            registry.topology_edge_start.clone(),
            registry.topology_edge_start_socket.clone(),
            TOPOLOGY_EDGE_KINDS[registry.topology_edge_kind % TOPOLOGY_EDGE_KINDS.len()],
            registry.topology_element % count,
            registry.topology_socket,
        )
    };
    let Some(start) = start else {
        set_editor_status(
            world,
            "Mark an edge start node before connecting or removing",
        );
        return;
    };
    let (end, _, _) = selected_topology_element(category, &recipe, selected)
        .expect("nonempty topology has a selected node");
    let end = end.to_owned();
    if start == end {
        set_editor_status(world, "An edge must connect two different topology nodes");
        return;
    }
    let start_exists = recipe
        .topology_nodes
        .iter()
        .any(|node| node.node_id == start);
    if !start_exists {
        let mut registry = world.resource_mut::<EditorRegistryState>();
        registry.topology_edge_start = None;
        registry.topology_edge_start_socket = None;
        set_editor_status(world, "Marked edge start no longer exists; mark it again");
        return;
    }
    let start_socket = stored_start_socket.filter(|socket_id| {
        recipe
            .topology_sockets
            .iter()
            .any(|socket| socket.socket_id == socket_id.as_str() && socket.node_id == start)
    });
    let end_socket = selected_socket_index(&recipe, &end, socket_selection)
        .map(|index| recipe.topology_sockets[index].socket_id.clone());
    let matches_edge = |edge: &WorldTopologyEdgeDraft| {
        edge.kind == kind
            && ((edge.from == start && edge.to == end) || (edge.from == end && edge.to == start))
    };
    let existing = recipe.topology_edges.iter().position(matches_edge);
    if connect && existing.is_some() {
        set_editor_status(
            world,
            format!("{kind:?} edge {start} ↔ {end} already exists"),
        );
        return;
    }
    let removal_index = if connect {
        None
    } else {
        let Some(index) = existing else {
            set_editor_status(world, format!("No {kind:?} edge {start} ↔ {end} exists"));
            return;
        };
        Some(index)
    };
    let description = if connect {
        format!("Connect {kind:?} edge {start} to {end}")
    } else {
        format!("Remove {kind:?} edge {start} to {end}")
    };
    execute_recipe_edit(world, description, move |recipe| {
        if connect {
            recipe.topology_edges.push(WorldTopologyEdgeDraft {
                from: start,
                to: end,
                kind,
                from_socket: start_socket,
                to_socket: end_socket,
            });
        } else {
            recipe
                .topology_edges
                .remove(removal_index.expect("remove operation has an edge index"));
        }
    });
}

const ROAD_JUNCTION_KINDS: [RoadJunctionKind; 4] = [
    RoadJunctionKind::Merge,
    RoadJunctionKind::Split,
    RoadJunctionKind::Cross,
    RoadJunctionKind::LoopLink,
];

fn cycle_road_tangent_axis(world: &mut World) {
    let axis = {
        let mut registry = world.resource_mut::<EditorRegistryState>();
        registry.road_tangent_axis = (registry.road_tangent_axis + 1) % 3;
        registry.road_tangent_axis
    };
    set_editor_status(
        world,
        format!("Road tangent axis: {}", ["X", "Y", "Z"][axis]),
    );
}

fn adjust_selected_road_tangent(world: &mut World, direction: f32) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a Road recipe before editing tangents");
        return;
    };
    if category != persistence::ContentCategory::Road || recipe.spline_points.is_empty() {
        set_editor_status(world, "Select a seeded Road recipe before editing tangents");
        return;
    }
    let registry = world.resource::<EditorRegistryState>();
    let point_index = registry.topology_element % recipe.spline_points.len();
    let axis = registry.road_tangent_axis % 3;
    let step = world.resource::<EditorGizmoSettings>().translation_snap();
    let point_id = recipe.spline_points[point_index].point_id.clone();
    let automatic = persistence::resolved_road_tangent(&recipe.spline_points, point_index);
    execute_recipe_edit(
        world,
        format!("Adjust road tangent {point_id} {}", ["X", "Y", "Z"][axis]),
        move |recipe| {
            let point = &mut recipe.spline_points[point_index];
            if point.tangent.iter().all(|value| value.abs() <= 1.0e-4) {
                point.tangent = automatic;
            }
            point.tangent[axis] += direction * step;
        },
    );
}

fn reset_selected_road_tangent(world: &mut World) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a Road recipe before resetting tangents");
        return;
    };
    if category != persistence::ContentCategory::Road || recipe.spline_points.is_empty() {
        set_editor_status(
            world,
            "Select a seeded Road recipe before resetting tangents",
        );
        return;
    }
    let point_index =
        world.resource::<EditorRegistryState>().topology_element % recipe.spline_points.len();
    let point_id = recipe.spline_points[point_index].point_id.clone();
    execute_recipe_edit(
        world,
        format!("Reset road tangent {point_id}"),
        move |recipe| {
            recipe.spline_points[point_index].tangent = [0.0; 3];
        },
    );
}

fn auto_round_all_road_tangents(world: &mut World) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a Road recipe before auto-rounding");
        return;
    };
    if category != persistence::ContentCategory::Road || recipe.spline_points.len() < 2 {
        set_editor_status(
            world,
            "Seed at least two Road spline points before auto-rounding",
        );
        return;
    }
    execute_recipe_edit(world, "Auto-round all Road spline tangents", |recipe| {
        for point in &mut recipe.spline_points {
            point.tangent = [0.0; 3];
        }
    });
}

fn mark_road_junction_start(world: &mut World) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a Road recipe before authoring a junction");
        return;
    };
    if category != persistence::ContentCategory::Road || recipe.spline_points.is_empty() {
        set_editor_status(
            world,
            "Select a seeded Road recipe before authoring a junction",
        );
        return;
    }
    let index =
        world.resource::<EditorRegistryState>().topology_element % recipe.spline_points.len();
    let point_id = recipe.spline_points[index].point_id.clone();
    world
        .resource_mut::<EditorRegistryState>()
        .road_junction_start = Some(point_id.clone());
    set_editor_status(
        world,
        format!("Marked road point {point_id}; select a junction destination"),
    );
}

fn cycle_road_junction_kind(world: &mut World) {
    let kind = {
        let mut registry = world.resource_mut::<EditorRegistryState>();
        registry.road_junction_kind = (registry.road_junction_kind + 1) % ROAD_JUNCTION_KINDS.len();
        ROAD_JUNCTION_KINDS[registry.road_junction_kind]
    };
    set_editor_status(world, format!("Road junction type: {kind:?}"));
}

fn edit_road_junction(world: &mut World, connect: bool) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a Road recipe before authoring a junction");
        return;
    };
    if category != persistence::ContentCategory::Road || recipe.spline_points.is_empty() {
        set_editor_status(
            world,
            "Select a seeded Road recipe before authoring a junction",
        );
        return;
    }
    let (start, kind, selected) = {
        let registry = world.resource::<EditorRegistryState>();
        (
            registry.road_junction_start.clone(),
            ROAD_JUNCTION_KINDS[registry.road_junction_kind % ROAD_JUNCTION_KINDS.len()],
            registry.topology_element % recipe.spline_points.len(),
        )
    };
    let Some(start) = start else {
        set_editor_status(world, "Mark a road junction start point first");
        return;
    };
    let end = recipe.spline_points[selected].point_id.clone();
    if start == end {
        set_editor_status(world, "A road junction must connect two different points");
        return;
    }
    if !recipe
        .spline_points
        .iter()
        .any(|point| point.point_id == start)
    {
        world
            .resource_mut::<EditorRegistryState>()
            .road_junction_start = None;
        set_editor_status(world, "Marked road junction start no longer exists");
        return;
    }
    let matches = |junction: &RoadJunctionDraft| {
        junction.kind == kind
            && ((junction.from_point == start && junction.to_point == end)
                || (junction.from_point == end && junction.to_point == start))
    };
    let existing = recipe.road_junctions.iter().position(matches);
    if connect && existing.is_some() {
        set_editor_status(
            world,
            format!("{kind:?} junction {start} ↔ {end} already exists"),
        );
        return;
    }
    let removal_index = if connect {
        None
    } else {
        let Some(index) = existing else {
            set_editor_status(
                world,
                format!("No {kind:?} junction {start} ↔ {end} exists"),
            );
            return;
        };
        Some(index)
    };
    let description = if connect {
        format!("Connect {kind:?} road junction {start} to {end}")
    } else {
        format!("Remove {kind:?} road junction {start} to {end}")
    };
    execute_recipe_edit(world, description, move |recipe| {
        if connect {
            let kind_id = format!("{kind:?}").to_lowercase();
            recipe.road_junctions.push(RoadJunctionDraft {
                junction_id: format!("junction.{start}.{end}.{kind_id}"),
                from_point: start,
                to_point: end,
                kind,
            });
        } else {
            recipe
                .road_junctions
                .remove(removal_index.expect("remove operation has a junction index"));
        }
    });
}

fn execute_recipe_edit(
    world: &mut World,
    description: impl Into<String>,
    edit: impl FnOnce(&mut ProceduralRecipeDraft),
) {
    let Some((content_id, _, before)) = selected_procedural_recipe(world) else {
        set_editor_status(
            world,
            "Select a Biome, Road, Building, Cave, or City recipe",
        );
        return;
    };
    let mut after = before.clone();
    edit(&mut after);
    if before == after {
        set_editor_status(world, "Recipe already has that value");
        return;
    }
    let description = description.into();
    let transaction = EditorTransaction {
        description: description.clone(),
        commands: vec![EditorCommand::SetProceduralRecipe {
            content_id,
            before: Box::new(before),
            after: Box::new(after),
        }],
    };
    let result = world
        .resource_scope(|world, mut stack: Mut<EditorUndoStack>| stack.execute(world, transaction));
    match result {
        Ok(()) => {
            world.resource_mut::<EditorDocumentState>().dirty = true;
            world.resource_mut::<WorldKitPreviewState>().signature = None;
            set_editor_status(world, format!("{description} complete"));
        }
        Err(error) => set_editor_status(world, format!("{description} failed: {error:?}")),
    }
}

fn active_recipe_slot(world: &World) -> Option<(String, &'static str)> {
    let (content_id, category, _) = selected_procedural_recipe(world)?;
    let slots = persistence::procedural_material_slots(category);
    let index = world.resource::<EditorRegistryState>().recipe_slot % slots.len();
    Some((content_id, slots[index]))
}

fn adjust_selected_recipe_parameter(world: &mut World, direction: f64) {
    let Some((_, category, recipe)) = selected_procedural_recipe(world) else {
        set_editor_status(world, "Select a World Kit recipe before editing parameters");
        return;
    };
    let specs = persistence::procedural_parameter_specs(category);
    let index = world.resource::<EditorRegistryState>().recipe_parameter % specs.len();
    let spec = specs[index];
    let current = persistence::procedural_parameter_value(&recipe, spec);
    let mut next = (current + spec.step * direction).clamp(spec.min, spec.max);
    if spec.whole {
        next = next.round();
    }
    execute_recipe_edit(
        world,
        format!("Set {} to {next:.2}", spec.label),
        move |recipe| {
            recipe
                .fields
                .insert(spec.key.into(), serde_json::json!(next));
        },
    );
}

fn reset_selected_recipe_parameters(world: &mut World) {
    let Some((_, category, _)) = selected_procedural_recipe(world) else {
        set_editor_status(
            world,
            "Select a World Kit recipe before resetting parameters",
        );
        return;
    };
    let keys = persistence::procedural_parameter_specs(category)
        .iter()
        .map(|spec| spec.key)
        .collect::<Vec<_>>();
    execute_recipe_edit(world, "Reset World Kit parameters", move |recipe| {
        for key in keys {
            recipe.fields.remove(key);
        }
    });
}

fn selected_recipe_validation_errors(world: &World) -> Option<(String, Vec<String>)> {
    let (content_id, _, _) = selected_procedural_recipe(world)?;
    let errors = validate_project(&world.resource::<EditorProjectSession>().project)
        .into_iter()
        .map(|error| format!("{error:?}"))
        .filter(|error| error.contains(&content_id))
        .collect();
    Some((content_id, errors))
}

fn seed_default_topology(
    category: persistence::ContentCategory,
    recipe: &mut ProceduralRecipeDraft,
) {
    recipe.spline_points.clear();
    recipe.road_junctions.clear();
    recipe.topology_nodes.clear();
    recipe.topology_sockets.clear();
    recipe.topology_edges.clear();
    if category == persistence::ContentCategory::Road {
        recipe.spline_points = [
            ("start", [0.0, 0.0, 0.0]),
            ("ramp", [0.0, 3.0, 30.0]),
            ("ridge", [20.0, 6.0, 60.0]),
            ("finish", [0.0, 3.0, 90.0]),
        ]
        .into_iter()
        .map(|(point_id, position)| RoadSplinePointDraft {
            point_id: point_id.into(),
            position,
            width_scale: 1.0,
            tangent: [0.0; 3],
        })
        .collect();
        return;
    }
    let node = |node_id: &str, kind: WorldTopologyNodeKind, position: [f32; 3], size: [f32; 3]| {
        WorldTopologyNodeDraft {
            node_id: node_id.into(),
            kind,
            position,
            size,
        }
    };
    let edge = |from: &str, to: &str, kind: WorldTopologyEdgeKind| WorldTopologyEdgeDraft {
        from: from.into(),
        to: to.into(),
        kind,
        from_socket: None,
        to_socket: None,
    };
    let socket = |socket_id: &str,
                  node_id: &str,
                  kind: WorldTopologySocketKind,
                  local_position: [f32; 3],
                  facing: [f32; 3]| WorldTopologySocketDraft {
        socket_id: socket_id.into(),
        node_id: node_id.into(),
        kind,
        local_position,
        facing,
        size: [2.5, 3.2],
    };
    match category {
        persistence::ContentCategory::Building => {
            recipe.topology_nodes = vec![
                node(
                    "entrance",
                    WorldTopologyNodeKind::Entrance,
                    [-12.0, 0.0, 0.0],
                    [8.0, 4.0, 8.0],
                ),
                node(
                    "lobby",
                    WorldTopologyNodeKind::Room,
                    [0.0, 0.0, 0.0],
                    [10.0, 4.0, 10.0],
                ),
                node(
                    "upper",
                    WorldTopologyNodeKind::Room,
                    [12.0, 5.0, 0.0],
                    [10.0, 4.0, 10.0],
                ),
                node(
                    "suite",
                    WorldTopologyNodeKind::Room,
                    [24.0, 5.0, 0.0],
                    [10.0, 4.0, 10.0],
                ),
            ];
            recipe.topology_edges = vec![
                edge("entrance", "lobby", WorldTopologyEdgeKind::Door),
                edge("lobby", "upper", WorldTopologyEdgeKind::Stair),
                edge("upper", "suite", WorldTopologyEdgeKind::Door),
            ];
            recipe.topology_sockets = vec![
                socket(
                    "entrance.doorway.out",
                    "entrance",
                    WorldTopologySocketKind::Doorway,
                    [4.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                ),
                socket(
                    "lobby.doorway.in",
                    "lobby",
                    WorldTopologySocketKind::Doorway,
                    [-5.0, 0.0, 0.0],
                    [-1.0, 0.0, 0.0],
                ),
            ];
            recipe.topology_edges[0].from_socket = Some("entrance.doorway.out".into());
            recipe.topology_edges[0].to_socket = Some("lobby.doorway.in".into());
        }
        persistence::ContentCategory::Cave => {
            recipe.topology_nodes = vec![
                node(
                    "entrance",
                    WorldTopologyNodeKind::Entrance,
                    [-24.0, 0.0, 0.0],
                    [10.0, 6.0, 10.0],
                ),
                node(
                    "chamber",
                    WorldTopologyNodeKind::Room,
                    [-8.0, 0.0, 0.0],
                    [12.0, 7.0, 12.0],
                ),
                node(
                    "travel_a",
                    WorldTopologyNodeKind::FastTravel,
                    [8.0, 5.0, 0.0],
                    [10.0, 6.0, 10.0],
                ),
                node(
                    "travel_b",
                    WorldTopologyNodeKind::FastTravel,
                    [24.0, 5.0, 0.0],
                    [10.0, 6.0, 10.0],
                ),
            ];
            recipe.topology_edges = vec![
                edge("entrance", "chamber", WorldTopologyEdgeKind::Tunnel),
                edge("chamber", "travel_a", WorldTopologyEdgeKind::Tunnel),
                edge("travel_a", "travel_b", WorldTopologyEdgeKind::Portal),
            ];
            recipe.topology_sockets = vec![
                socket(
                    "entrance.tunnel.out",
                    "entrance",
                    WorldTopologySocketKind::Doorway,
                    [5.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                ),
                socket(
                    "chamber.tunnel.in",
                    "chamber",
                    WorldTopologySocketKind::Doorway,
                    [-6.0, 0.0, 0.0],
                    [-1.0, 0.0, 0.0],
                ),
            ];
            recipe.topology_edges[0].from_socket = Some("entrance.tunnel.out".into());
            recipe.topology_edges[0].to_socket = Some("chamber.tunnel.in".into());
        }
        persistence::ContentCategory::Biome => {
            recipe.topology_nodes = vec![
                node(
                    "core",
                    WorldTopologyNodeKind::Zone,
                    [0.0, 0.0, 0.0],
                    [24.0, 2.0, 24.0],
                ),
                node(
                    "water",
                    WorldTopologyNodeKind::Zone,
                    [28.0, -1.0, 0.0],
                    [18.0, 1.0, 18.0],
                ),
                node(
                    "hazard",
                    WorldTopologyNodeKind::Zone,
                    [-28.0, 2.0, 0.0],
                    [18.0, 3.0, 18.0],
                ),
                node(
                    "landmark",
                    WorldTopologyNodeKind::Landmark,
                    [0.0, 5.0, 28.0],
                    [8.0, 10.0, 8.0],
                ),
            ];
        }
        persistence::ContentCategory::City => {
            recipe.topology_nodes = vec![
                node(
                    "central",
                    WorldTopologyNodeKind::Zone,
                    [0.0, 0.0, 0.0],
                    [22.0, 3.0, 22.0],
                ),
                node(
                    "east",
                    WorldTopologyNodeKind::Zone,
                    [28.0, 0.0, 0.0],
                    [20.0, 4.0, 20.0],
                ),
                node(
                    "west",
                    WorldTopologyNodeKind::Zone,
                    [-28.0, 0.0, 0.0],
                    [20.0, 5.0, 20.0],
                ),
                node(
                    "landmark",
                    WorldTopologyNodeKind::Landmark,
                    [0.0, 8.0, 28.0],
                    [10.0, 16.0, 10.0],
                ),
            ];
        }
        _ => {}
    }
}

#[derive(Debug, Clone)]
struct CompiledWorldKitPart {
    name: String,
    transform: Transform,
    size: Vec3,
    material_slot: &'static str,
}

fn segment_part(
    name: String,
    start: Vec3,
    end: Vec3,
    width: f32,
    height: f32,
    material_slot: &'static str,
) -> Option<CompiledWorldKitPart> {
    let delta = end - start;
    let horizontal = Vec2::new(delta.x, delta.z).length();
    if horizontal <= 0.25 || delta.length() <= 0.5 {
        return None;
    }
    let yaw = delta.x.atan2(delta.z);
    let pitch = (delta.y / horizontal).atan();
    let transform = Transform::from_translation(start + delta * 0.5)
        .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch));
    Some(CompiledWorldKitPart {
        name,
        transform,
        size: Vec3::new(width, height, delta.length()),
        material_slot,
    })
}

fn road_segment_parts(
    name: &str,
    start: Vec3,
    end: Vec3,
    width: f32,
) -> Option<Vec<CompiledWorldKitPart>> {
    let deck = segment_part(name.to_string(), start, end, width, 0.8, "deck")?;
    let direction = (end - start).with_y(0.0).normalize_or_zero();
    let right = Vec3::new(direction.z, 0.0, -direction.x);
    let barrier_height = if width >= 30.0 { 4.6 } else { 3.2 };
    let barrier_lift = Vec3::Y * (barrier_height * 0.5 + 0.4);
    let mut parts = vec![deck];
    for (side, label) in [(-1.0_f32, "left"), (1.0, "right")] {
        let offset = right * side * width * 0.505 + barrier_lift;
        parts.push(segment_part(
            format!("{name} {label} barrier"),
            start + offset,
            end + offset,
            if width >= 30.0 { 1.05 } else { 0.82 },
            barrier_height,
            "barrier",
        )?);
    }
    Some(parts)
}

fn hermite_road_connector(
    start: Vec3,
    end: Vec3,
    mut start_tangent: Vec3,
    mut end_tangent: Vec3,
    t: f32,
) -> Vec3 {
    let chord = end - start;
    let tangent_limit = chord.length() * 0.58;
    start_tangent = start_tangent.clamp_length_max(tangent_limit);
    end_tangent = end_tangent.clamp_length_max(tangent_limit);
    if start_tangent.dot(chord) < 0.0 {
        start_tangent = -start_tangent;
    }
    if end_tangent.dot(chord) < 0.0 {
        end_tangent = -end_tangent;
    }
    let t = t.clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;
    start * (2.0 * t3 - 3.0 * t2 + 1.0)
        + start_tangent * (t3 - 2.0 * t2 + t)
        + end * (-2.0 * t3 + 3.0 * t2)
        + end_tangent * (t3 - t2)
}

fn compile_world_kit(
    category: persistence::ContentCategory,
    recipe: &ProceduralRecipeDraft,
) -> Result<Vec<CompiledWorldKitPart>, String> {
    let parameter = |key: &str| {
        persistence::procedural_parameter_specs(category)
            .iter()
            .find(|spec| spec.key == key)
            .map(|spec| persistence::procedural_parameter_value(recipe, *spec) as f32)
            .unwrap_or_default()
    };
    let mut parts = Vec::new();
    if category == persistence::ContentCategory::Road {
        if recipe.spline_points.len() < 2 {
            return Err("seed or author at least two spline points".into());
        }
        let spans = recipe.spline_points.len() - 1;
        let junction_part_count = recipe.road_junctions.len() * 12 * 3;
        let curve_part_budget = 192usize.saturating_sub(junction_part_count);
        let samples_per_span = (curve_part_budget / 3 / spans).clamp(1, 16);
        for span in 0..spans {
            let mut start = Vec3::from_array(recipe.spline_points[span].position);
            for sample in 1..=samples_per_span {
                let t = sample as f32 / samples_per_span as f32;
                let end = Vec3::from_array(
                    persistence::sample_road_spline_span(&recipe.spline_points, span, t)
                        .ok_or_else(|| format!("road span {span} cannot be sampled"))?,
                );
                let width_scale = recipe.spline_points[span].width_scale
                    + (recipe.spline_points[span + 1].width_scale
                        - recipe.spline_points[span].width_scale)
                        * t;
                let segment_name = format!("Road curve {span}.{sample}");
                let segment_parts =
                    road_segment_parts(&segment_name, start, end, parameter("width") * width_scale)
                        .ok_or_else(|| format!("road curve {span}.{sample} is degenerate"))?;
                parts.extend(segment_parts);
                start = end;
            }
        }
        let points = recipe
            .spline_points
            .iter()
            .enumerate()
            .map(|(index, point)| (point.point_id.as_str(), index))
            .collect::<BTreeMap<_, _>>();
        for junction in &recipe.road_junctions {
            let Some(start_index) = points.get(junction.from_point.as_str()).copied() else {
                return Err(format!(
                    "junction {} is missing its start point",
                    junction.junction_id
                ));
            };
            let Some(end_index) = points.get(junction.to_point.as_str()).copied() else {
                return Err(format!(
                    "junction {} is missing its destination point",
                    junction.junction_id
                ));
            };
            let start = Vec3::from_array(recipe.spline_points[start_index].position);
            let end = Vec3::from_array(recipe.spline_points[end_index].position);
            let start_tangent = Vec3::from_array(persistence::resolved_road_tangent(
                &recipe.spline_points,
                start_index,
            ));
            let end_tangent = Vec3::from_array(persistence::resolved_road_tangent(
                &recipe.spline_points,
                end_index,
            ));
            let start_width = recipe.spline_points[start_index].width_scale;
            let end_width = recipe.spline_points[end_index].width_scale;
            let mut previous = start;
            for sample in 1..=12 {
                let t = sample as f32 / 12.0;
                let current = hermite_road_connector(start, end, start_tangent, end_tangent, t);
                let width_scale = start_width + (end_width - start_width) * t;
                let segment_name = format!(
                    "{:?} junction {} curve {sample}",
                    junction.kind, junction.junction_id
                );
                let segment_parts = road_segment_parts(
                    &segment_name,
                    previous,
                    current,
                    parameter("width") * width_scale * 0.72,
                )
                .ok_or_else(|| format!("junction {} is degenerate", junction.junction_id))?;
                parts.extend(segment_parts);
                previous = current;
            }
        }
    } else {
        if recipe.topology_nodes.is_empty() {
            return Err("seed or author topology nodes before compiling".into());
        }
        for node in &recipe.topology_nodes {
            let material_slot = match (category, node.kind) {
                (persistence::ContentCategory::Biome, WorldTopologyNodeKind::Landmark) => "detail",
                (persistence::ContentCategory::Biome, _) => "terrain",
                (persistence::ContentCategory::Building, WorldTopologyNodeKind::Entrance) => {
                    "exterior"
                }
                (persistence::ContentCategory::Building, _) => "interior",
                (persistence::ContentCategory::Cave, WorldTopologyNodeKind::FastTravel) => "accent",
                (persistence::ContentCategory::Cave, _) => "rock",
                (persistence::ContentCategory::City, WorldTopologyNodeKind::Landmark) => "landmark",
                (persistence::ContentCategory::City, _) => "exterior",
                _ => "detail",
            };
            parts.push(CompiledWorldKitPart {
                name: format!("{:?} {}", node.kind, node.node_id),
                transform: Transform::from_translation(Vec3::from_array(node.position)),
                size: Vec3::from_array(node.size),
                material_slot,
            });
        }
        let nodes = recipe
            .topology_nodes
            .iter()
            .map(|node| (node.node_id.as_str(), Vec3::from_array(node.position)))
            .collect::<BTreeMap<_, _>>();
        let mut sockets = BTreeMap::new();
        for socket in &recipe.topology_sockets {
            let Some(node_position) = nodes.get(socket.node_id.as_str()).copied() else {
                return Err(format!("socket {} has no owning node", socket.socket_id));
            };
            let position = node_position + Vec3::from_array(socket.local_position);
            sockets.insert(socket.socket_id.as_str(), position);
            let facing = Vec3::from_array(socket.facing).normalize_or(Vec3::Z);
            let yaw = facing.x.atan2(facing.z);
            let material_slot = match category {
                persistence::ContentCategory::Building => "trim",
                persistence::ContentCategory::Cave => "accent",
                persistence::ContentCategory::City => "landmark",
                _ => "detail",
            };
            parts.push(CompiledWorldKitPart {
                name: format!("{:?} socket {}", socket.kind, socket.socket_id),
                transform: Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_y(yaw)),
                size: Vec3::new(socket.size[0], socket.size[1], 0.35),
                material_slot,
            });
        }
        for (index, edge) in recipe.topology_edges.iter().enumerate() {
            let start = edge
                .from_socket
                .as_deref()
                .and_then(|socket| sockets.get(socket).copied())
                .unwrap_or(nodes[edge.from.as_str()]);
            let end = edge
                .to_socket
                .as_deref()
                .and_then(|socket| sockets.get(socket).copied())
                .unwrap_or(nodes[edge.to.as_str()]);
            let (width, slot) = match category {
                persistence::ContentCategory::Building => (parameter("stair_width"), "floor"),
                persistence::ContentCategory::Cave => (parameter("tunnel_width") * 0.55, "floor"),
                persistence::ContentCategory::City => (parameter("road_width"), "road"),
                _ => (3.0, "detail"),
            };
            if let Some(part) = segment_part(
                format!("{:?} connector {index}", edge.kind),
                start,
                end,
                width,
                0.45,
                slot,
            ) {
                parts.push(part);
            }
        }
    }
    if parts.is_empty() || parts.len() > 192 {
        return Err("compiled root must contain 1–192 bounded parts".into());
    }
    Ok(parts)
}

fn edit_selected_material(world: &mut World, edit: MaterialEdit) {
    let (selected, parameter_index) = {
        let registry = world.resource::<EditorRegistryState>();
        (registry.selected, registry.material_parameter)
    };
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
    let current = world
        .resource::<EditorProjectSession>()
        .project
        .payloads
        .get(&content_id)
        .and_then(|payload| match payload {
            persistence::ContentPayload::Material(recipe) => {
                Some(ForgeMaterialSpec::from_recipe(recipe))
            }
            _ => None,
        });
    let Some(mut spec) = current else {
        set_editor_status(world, "Selected registry record is not a material");
        return;
    };
    let controls = material_controls(spec.family);
    let active = parameter_index % controls.len();

    if matches!(edit, MaterialEdit::NextParameter) {
        let next = (active + 1) % controls.len();
        world
            .resource_mut::<EditorRegistryState>()
            .material_parameter = next;
        set_editor_status(
            world,
            format!("Material parameter: {}", controls[next].label),
        );
        return;
    }

    match edit {
        MaterialEdit::CycleFamily => {
            spec = ForgeMaterialSpec::preset(spec.family.offset(1), 0);
            world
                .resource_mut::<EditorRegistryState>()
                .material_parameter = 0;
        }
        MaterialEdit::CyclePreset => {
            spec = ForgeMaterialSpec::preset(spec.family, (spec.preset + 1) % 3);
        }
        MaterialEdit::Adjust(direction) => {
            let control = controls[active];
            spec.values[active] =
                (spec.values[active] + control.step * direction).clamp(control.min, control.max);
        }
        MaterialEdit::Reset => {
            spec = ForgeMaterialSpec::preset(spec.family, spec.preset);
        }
        MaterialEdit::NextParameter => unreachable!(),
    }
    spec.normalize();

    if let Some(persistence::ContentPayload::Material(recipe)) = world
        .resource_mut::<EditorProjectSession>()
        .project
        .payloads
        .get_mut(&content_id)
    {
        spec.write_to(recipe);
    }
    world.resource_mut::<EditorDocumentState>().dirty = true;
    let controls = material_controls(spec.family);
    let active = world.resource::<EditorRegistryState>().material_parameter % controls.len();
    set_editor_status(
        world,
        format!(
            "{} preset {} • {} = {:.3}",
            spec.family.label(),
            spec.preset + 1,
            controls[active].label,
            spec.values[active],
        ),
    );
}

fn apply_selected_material_to_object(world: &mut World) {
    let selected_record = world.resource::<EditorRegistryState>().selected;
    let material_id = world
        .resource::<EditorProjectSession>()
        .project
        .records
        .get(selected_record)
        .filter(|record| record.category == persistence::ContentCategory::Material)
        .map(|record| record.content_id.clone());
    let Some(material_id) = material_id else {
        set_editor_status(world, "Select a Material registry record first");
        return;
    };
    if !world
        .resource::<PublishedMaterialCatalog>()
        .contains(&material_id)
    {
        set_editor_status(
            world,
            "Material is not published; publish the project first",
        );
        return;
    }
    let Some(editor_id) = world.resource::<EditorSelection>().active() else {
        set_editor_status(world, "Select a scene object before applying a material");
        return;
    };
    let Some(entity) = resolve_editor_entity(world, editor_id) else {
        set_editor_status(world, "Selected scene object is missing");
        return;
    };
    if apply_catalog_material(world, entity, &material_id) {
        world.resource_mut::<EditorDocumentState>().dirty = true;
        set_editor_status(
            world,
            format!("Applied {material_id} to object #{}", editor_id.0),
        );
    } else {
        set_editor_status(world, "Selected object has no renderable mesh");
    }
}

fn clear_selected_object_material(world: &mut World) {
    let Some(editor_id) = world.resource::<EditorSelection>().active() else {
        set_editor_status(world, "Select a scene object before clearing its material");
        return;
    };
    let Some(entity) = resolve_editor_entity(world, editor_id) else {
        set_editor_status(world, "Selected scene object is missing");
        return;
    };
    if clear_catalog_material(world, entity) {
        world.resource_mut::<EditorDocumentState>().dirty = true;
        set_editor_status(
            world,
            format!("Cleared material on object #{}", editor_id.0),
        );
    } else {
        set_editor_status(world, "Selected object has no renderable mesh");
    }
}

fn apply_editor_action(world: &mut World, action: EditorAction) {
    if action != EditorAction::Exit {
        world.resource_mut::<EditorDocumentState>().exit_armed = false;
    }
    if action != EditorAction::LevelDelete {
        if let Some(mut arm) = world.get_resource_mut::<EditorLevelDeleteArm>() {
            arm.0 = None;
        }
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
        EditorAction::SaveProject => {
            save_editor_project(world, false);
        }
        EditorAction::PublishProject => {
            save_editor_project(world, true);
        }
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
                registry.topology_element = 0;
                registry.topology_edge_start = None;
                registry.topology_edge_start_socket = None;
                registry.topology_socket = 0;
                registry.road_junction_start = None;
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
                .create_content(
                    persistence::ContentCategory::Material,
                    "New Shader Material",
                );
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
        EditorAction::PresetCapture => {
            let selected = world.resource::<EditorRegistryState>().selected;
            let session = world.resource::<EditorProjectSession>();
            let Some(record) = session.project.records.get(selected).cloned() else {
                set_editor_status(world, "No registry record selected");
                return;
            };
            let Some(payload) = session.project.payloads.get(&record.content_id).cloned() else {
                set_editor_status(
                    world,
                    format!("{} has no payload to capture", record.content_id),
                );
                return;
            };
            let store = presets::PresetStore::for_project(session.store.path());
            let preset = presets::capture(
                &record,
                &payload,
                &world.resource::<EditorPresetLibrary>().presets,
            );
            match store.save(&preset) {
                Ok(()) => {
                    let message = format!("Captured preset {}", preset.summary());
                    let mut library = world.resource_mut::<EditorPresetLibrary>();
                    library.presets.push(preset);
                    library.selected = library.presets.len() - 1;
                    set_editor_status(world, message);
                }
                Err(error) => set_editor_status(world, format!("Preset capture failed: {error}")),
            }
        }
        EditorAction::PresetNext => {
            let mut library = world.resource_mut::<EditorPresetLibrary>();
            if library.presets.is_empty() {
                set_editor_status(world, "No presets captured yet");
                return;
            }
            library.selected = (library.selected + 1) % library.presets.len();
            let summary = library.presets[library.selected].summary();
            set_editor_status(world, format!("Selected preset {summary}"));
        }
        EditorAction::PresetApply => {
            let library = world.resource::<EditorPresetLibrary>();
            let Some(preset) = library.presets.get(library.selected).cloned() else {
                set_editor_status(world, "No presets captured yet");
                return;
            };
            let mut session = world.resource_mut::<EditorProjectSession>();
            let Some(payload) = session.project.payloads.get_mut(&preset.source_content_id) else {
                set_editor_status(
                    world,
                    format!(
                        "{} no longer exists in this project",
                        preset.source_content_id
                    ),
                );
                return;
            };
            match presets::apply(&preset, payload) {
                Ok(_) => {
                    world.resource_mut::<EditorDocumentState>().dirty = true;
                    set_editor_status(
                        world,
                        format!(
                            "Applied {} to {}",
                            preset.summary(),
                            preset.source_content_id
                        ),
                    );
                }
                Err(error) => set_editor_status(world, format!("Apply rejected: {error}")),
            }
        }
        EditorAction::PresetCompare => {
            let library = world.resource::<EditorPresetLibrary>();
            let Some(preset) = library.presets.get(library.selected).cloned() else {
                set_editor_status(world, "No presets captured yet");
                return;
            };
            let session = world.resource::<EditorProjectSession>();
            let Some(payload) = session.project.payloads.get(&preset.source_content_id) else {
                set_editor_status(
                    world,
                    format!(
                        "{} no longer exists in this project",
                        preset.source_content_id
                    ),
                );
                return;
            };
            let differences = presets::compare(&preset, payload);
            let message = if differences.is_empty() {
                format!("{} matches the live content", preset.summary())
            } else {
                format!("{} differs: {}", preset.summary(), differences.join("; "))
            };
            set_editor_status(world, message);
        }
        EditorAction::PresetFork => {
            let library = world.resource::<EditorPresetLibrary>();
            let Some(preset) = library.presets.get(library.selected) else {
                set_editor_status(world, "No presets captured yet");
                return;
            };
            let forked = presets::fork(preset, &library.presets);
            let store = presets::PresetStore::for_project(
                world.resource::<EditorProjectSession>().store.path(),
            );
            match store.save(&forked) {
                Ok(()) => {
                    let message = format!("Forked into {}", forked.summary());
                    let mut library = world.resource_mut::<EditorPresetLibrary>();
                    library.presets.push(forked);
                    library.selected = library.presets.len() - 1;
                    set_editor_status(world, message);
                }
                Err(error) => set_editor_status(world, format!("Fork failed: {error}")),
            }
        }
        EditorAction::PresetNewSeedVariant => {
            let library = world.resource::<EditorPresetLibrary>();
            let Some(preset) = library.presets.get(library.selected) else {
                set_editor_status(world, "No presets captured yet");
                return;
            };
            let variant = match presets::new_seed_variant(preset, &library.presets) {
                Ok(variant) => variant,
                Err(error) => {
                    set_editor_status(world, format!("Variant rejected: {error}"));
                    return;
                }
            };
            let store = presets::PresetStore::for_project(
                world.resource::<EditorProjectSession>().store.path(),
            );
            match store.save(&variant) {
                Ok(()) => {
                    let message = format!("New-seed variant {}", variant.summary());
                    let mut library = world.resource_mut::<EditorPresetLibrary>();
                    library.presets.push(variant);
                    library.selected = library.presets.len() - 1;
                    set_editor_status(world, message);
                }
                Err(error) => set_editor_status(world, format!("Variant save failed: {error}")),
            }
        }
        EditorAction::ModifierCycleKind => {
            let mut armed = world.resource_mut::<EditorArmedModifier>();
            armed.0 = (armed.0 + 1) % 6;
            let label = armed_modifier_template(armed.0).label();
            set_editor_status(world, format!("Armed modifier: {label}"));
        }
        EditorAction::ModifierAdd => {
            let Some(editor_id) = world.resource::<EditorSelection>().active() else {
                set_editor_status(world, "Select a scene object before adding a modifier");
                return;
            };
            let Some(entity) = resolve_editor_entity(world, editor_id) else {
                set_editor_status(world, "Selected scene object is missing");
                return;
            };
            let modifier = armed_modifier_template(world.resource::<EditorArmedModifier>().0);
            let label = modifier.label();
            match world.get_mut::<EditorModifierStack>(entity) {
                Some(mut stack) => stack.0.push(modifier),
                None => {
                    world
                        .entity_mut(entity)
                        .insert(EditorModifierStack(vec![modifier]));
                }
            }
            if remesh_modified_object(world, entity) {
                world.resource_mut::<EditorDocumentState>().dirty = true;
                let depth = world
                    .get::<EditorModifierStack>(entity)
                    .map(|stack| stack.0.len())
                    .unwrap_or(0);
                set_editor_status(
                    world,
                    format!(
                        "Added {label} to object #{} (stack depth {depth})",
                        editor_id.0
                    ),
                );
            } else {
                if let Some(mut stack) = world.get_mut::<EditorModifierStack>(entity) {
                    stack.0.pop();
                }
                set_editor_status(world, "Selected object has no modifiable mesh");
            }
        }
        EditorAction::LevelCycleTemplate => {
            let mut armed = world.resource_mut::<EditorArmedLevelTemplate>();
            armed.0 = (armed.0 + 1) % LevelTemplate::ALL.len();
            let label = LevelTemplate::ALL[armed.0].label();
            set_editor_status(world, format!("Armed level template: {label}"));
        }
        EditorAction::LevelNew => {
            let template = LevelTemplate::ALL
                [world.resource::<EditorArmedLevelTemplate>().0 % LevelTemplate::ALL.len()];
            let scene = collect_working_scene(world);
            let (level_id, objects) = {
                let mut session = world.resource_mut::<EditorProjectSession>();
                session.project.scene = scene;
                let level_id = session.project.create_level(template);
                (level_id, session.project.scene.objects.clone())
            };
            world.resource_mut::<EditorSelection>().clear();
            respawn_scene_objects(world, &objects);
            world.resource_mut::<EditorDocumentState>().dirty = true;
            set_editor_status(
                world,
                format!("Created {} ({level_id}) — now active", template.label()),
            );
        }
        EditorAction::LevelNext => {
            let scene = collect_working_scene(world);
            let switch = {
                let mut session = world.resource_mut::<EditorProjectSession>();
                session.project.scene = scene;
                if session.project.levels.len() < 2 {
                    None
                } else {
                    let current = session
                        .project
                        .levels
                        .iter()
                        .position(|level| level.level_id == session.project.active_level_id)
                        .unwrap_or(0);
                    let next = (current + 1) % session.project.levels.len();
                    let next_id = session.project.levels[next].level_id.clone();
                    match session.project.activate_level(&next_id) {
                        Ok(()) => Some((
                            session.project.levels[next].display_name.clone(),
                            session.project.scene.objects.clone(),
                        )),
                        Err(_) => None,
                    }
                }
            };
            let Some((name, objects)) = switch else {
                set_editor_status(world, "This project has only one level");
                return;
            };
            world.resource_mut::<EditorSelection>().clear();
            respawn_scene_objects(world, &objects);
            world.resource_mut::<EditorDocumentState>().dirty = true;
            set_editor_status(world, format!("Editing level: {name}"));
        }
        EditorAction::LevelMoveEarlier | EditorAction::LevelMoveLater => {
            let offset = if action == EditorAction::LevelMoveEarlier {
                -1
            } else {
                1
            };
            let scene = collect_working_scene(world);
            let result = {
                let mut session = world.resource_mut::<EditorProjectSession>();
                session.project.scene = scene;
                let active_id = session.project.active_level_id.clone();
                let name = session
                    .project
                    .active_level()
                    .map(|level| level.display_name.clone())
                    .unwrap_or_else(|| active_id.clone());
                let count = session.project.levels.len();
                session
                    .project
                    .move_level(&active_id, offset)
                    .map(|index| (name, index, count))
            };
            match result {
                Ok((name, index, count)) => {
                    world.resource_mut::<EditorDocumentState>().dirty = true;
                    set_editor_status(
                        world,
                        format!("Moved {name} to level position {}/{}", index + 1, count),
                    );
                }
                Err(error) => set_editor_status(world, format!("Level move rejected: {error}")),
            }
        }
        EditorAction::LevelDelete => {
            let (active_id, name, count) = {
                let session = world.resource::<EditorProjectSession>();
                let active_id = session.project.active_level_id.clone();
                let name = session
                    .project
                    .active_level()
                    .map(|level| level.display_name.clone())
                    .unwrap_or_else(|| active_id.clone());
                (active_id, name, session.project.levels.len())
            };
            if count <= 1 {
                world.resource_mut::<EditorLevelDeleteArm>().0 = None;
                set_editor_status(world, "Delete rejected: a project must keep one level");
                return;
            }
            let confirmed =
                world.resource::<EditorLevelDeleteArm>().0.as_deref() == Some(active_id.as_str());
            if !confirmed {
                world.resource_mut::<EditorLevelDeleteArm>().0 = Some(active_id);
                set_editor_status(
                    world,
                    format!("Press DELETE LEVEL again to remove {name} from this project"),
                );
                return;
            }

            let scene = collect_working_scene(world);
            let result = {
                let mut session = world.resource_mut::<EditorProjectSession>();
                session.project.scene = scene;
                session.project.delete_level(&active_id).map(|deleted| {
                    let next_name = session
                        .project
                        .active_level()
                        .map(|level| level.display_name.clone())
                        .unwrap_or_else(|| session.project.active_level_id.clone());
                    (
                        deleted.display_name,
                        next_name,
                        session.project.scene.objects.clone(),
                    )
                })
            };
            world.resource_mut::<EditorLevelDeleteArm>().0 = None;
            match result {
                Ok((deleted_name, next_name, objects)) => {
                    world.resource_mut::<EditorSelection>().clear();
                    respawn_scene_objects(world, &objects);
                    world.resource_mut::<EditorDocumentState>().dirty = true;
                    set_editor_status(
                        world,
                        format!("Deleted {deleted_name}; now editing {next_name}"),
                    );
                }
                Err(error) => set_editor_status(world, format!("Delete rejected: {error}")),
            }
        }
        EditorAction::LevelSetStartup => {
            let (active_id, name) = {
                let mut session = world.resource_mut::<EditorProjectSession>();
                let active_id = session.project.active_level_id.clone();
                session.project.startup_level_id = active_id.clone();
                let name = session
                    .project
                    .active_level()
                    .map(|level| level.display_name.clone())
                    .unwrap_or_else(|| active_id.clone());
                (active_id, name)
            };
            world.resource_mut::<EditorDocumentState>().dirty = true;
            set_editor_status(world, format!("Startup level set to {name} ({active_id})"));
        }
        EditorAction::PlaytestStartup => {
            // PM3 workflow: Save → boot the startup level → leave the
            // protected editing boundary and play it.
            let scene = collect_working_scene(world);
            let switch: Result<Option<Vec<SceneObjectDraft>>, String> = {
                let mut session = world.resource_mut::<EditorProjectSession>();
                session.project.scene = scene;
                session.project.sync_active_level();
                let startup_id = session.project.startup_level_id.clone();
                if session.project.active_level_id == startup_id {
                    Ok(None)
                } else {
                    session
                        .project
                        .activate_level(&startup_id)
                        .map(|()| Some(session.project.scene.objects.clone()))
                }
            };
            let switch = match switch {
                Ok(switch) => switch,
                Err(error) => {
                    set_editor_status(world, format!("Playtest failed: {error}"));
                    return;
                }
            };
            let name = {
                let session = world.resource::<EditorProjectSession>();
                session
                    .project
                    .active_level()
                    .map(|level| level.display_name.clone())
                    .unwrap_or_else(|| session.project.startup_level_id.clone())
            };
            if let Some(objects) = switch {
                world.resource_mut::<EditorSelection>().clear();
                respawn_scene_objects(world, &objects);
            }
            if !save_editor_project(world, false) {
                // Stay in the editor; the save error is already on the
                // status line.
                return;
            }
            world
                .resource_mut::<NextState<EngineToolMode>>()
                .set(EngineToolMode::Playing);
            set_editor_status(world, format!("Playtesting startup level: {name}"));
        }
        EditorAction::ModifierCycleParam => {
            let Some((entity, editor_id)) = selected_modifier_target(world) else {
                return;
            };
            let Some(stack) = world.get::<EditorModifierStack>(entity) else {
                set_editor_status(world, "Selected object has no modifiers");
                return;
            };
            let Some(last) = stack.0.last().cloned() else {
                set_editor_status(world, "Selected object has no modifiers");
                return;
            };
            let mut cursor = world.resource_mut::<EditorModifierParamCursor>();
            cursor.0 = (cursor.0 + 1) % last.param_count();
            let index = cursor.0;
            set_editor_status(
                world,
                format!(
                    "{} on #{}: {}",
                    last.label(),
                    editor_id.0,
                    last.param_label(index)
                ),
            );
        }
        EditorAction::ModifierValueDown | EditorAction::ModifierValueUp => {
            let direction = if matches!(action, EditorAction::ModifierValueUp) {
                1
            } else {
                -1
            };
            let Some((entity, editor_id)) = selected_modifier_target(world) else {
                return;
            };
            let index = world.resource::<EditorModifierParamCursor>().0;
            let adjusted = world
                .get_mut::<EditorModifierStack>(entity)
                .and_then(|mut stack| {
                    stack.0.last_mut().map(|last| {
                        last.adjust_param(index, direction);
                        (last.label(), last.param_label(index))
                    })
                });
            let Some((label, value)) = adjusted else {
                set_editor_status(world, "Selected object has no modifiers");
                return;
            };
            remesh_modified_object(world, entity);
            world.resource_mut::<EditorDocumentState>().dirty = true;
            set_editor_status(world, format!("{} on #{}: {}", label, editor_id.0, value));
        }
        EditorAction::ModifierRemoveLast => {
            let Some(editor_id) = world.resource::<EditorSelection>().active() else {
                set_editor_status(world, "Select a scene object before removing a modifier");
                return;
            };
            let Some(entity) = resolve_editor_entity(world, editor_id) else {
                set_editor_status(world, "Selected scene object is missing");
                return;
            };
            let removed = world
                .get_mut::<EditorModifierStack>(entity)
                .and_then(|mut stack| stack.0.pop());
            let Some(removed) = removed else {
                set_editor_status(world, "Selected object has no modifiers");
                return;
            };
            remesh_modified_object(world, entity);
            world.resource_mut::<EditorDocumentState>().dirty = true;
            set_editor_status(
                world,
                format!("Removed {} from object #{}", removed.label(), editor_id.0),
            );
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
        EditorAction::MaterialCycleFamily => {
            edit_selected_material(world, MaterialEdit::CycleFamily)
        }
        EditorAction::MaterialCyclePreset => {
            edit_selected_material(world, MaterialEdit::CyclePreset)
        }
        EditorAction::MaterialNextParameter => {
            edit_selected_material(world, MaterialEdit::NextParameter)
        }
        EditorAction::MaterialDecrease => edit_selected_material(world, MaterialEdit::Adjust(-1.0)),
        EditorAction::MaterialIncrease => edit_selected_material(world, MaterialEdit::Adjust(1.0)),
        EditorAction::MaterialReset => edit_selected_material(world, MaterialEdit::Reset),
        EditorAction::ApplySelectedMaterial => apply_selected_material_to_object(world),
        EditorAction::ClearObjectMaterial => clear_selected_object_material(world),
        EditorAction::CycleRecipeKind => {
            let label = {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.recipe_kind = (registry.recipe_kind + 1) % WORLD_RECIPE_CATEGORIES.len();
                let category = WORLD_RECIPE_CATEGORIES[registry.recipe_kind];
                world_recipe_label(category)
            };
            set_editor_status(world, format!("New World Kit recipe type: {label}"));
        }
        EditorAction::CreateRecipeRecord => {
            let category = {
                let registry = world.resource::<EditorRegistryState>();
                WORLD_RECIPE_CATEGORIES[registry.recipe_kind % WORLD_RECIPE_CATEGORIES.len()]
            };
            let result = world
                .resource_mut::<EditorProjectSession>()
                .project
                .create_content(category, world_recipe_default_name(category));
            match result {
                Ok(content_id) => {
                    let index = world
                        .resource::<EditorProjectSession>()
                        .project
                        .records
                        .iter()
                        .position(|record| record.content_id == content_id)
                        .unwrap_or(0);
                    let mut registry = world.resource_mut::<EditorRegistryState>();
                    registry.selected = index;
                    registry.recipe_slot = 0;
                    registry.recipe_parameter = 0;
                    registry.topology_element = 0;
                    registry.topology_edge_start = None;
                    registry.topology_edge_start_socket = None;
                    registry.topology_socket = 0;
                    registry.road_junction_start = None;
                    world.resource_mut::<EditorDocumentState>().dirty = true;
                    world.resource_mut::<WorldKitPreviewState>().signature = None;
                    set_editor_status(world, format!("Created {content_id}"));
                }
                Err(error) => set_editor_status(world, format!("Create rejected: {error}")),
            }
        }
        EditorAction::CopySelectedMaterial => {
            let selected = world.resource::<EditorRegistryState>().selected;
            let material_id = world
                .resource::<EditorProjectSession>()
                .project
                .records
                .get(selected)
                .filter(|record| record.category == persistence::ContentCategory::Material)
                .map(|record| record.content_id.clone());
            let Some(material_id) = material_id else {
                set_editor_status(world, "Select a Material record before copying");
                return;
            };
            if !world
                .resource::<PublishedMaterialCatalog>()
                .contains(&material_id)
            {
                set_editor_status(world, "Publish this Material before using it in World Kit");
                return;
            }
            world
                .resource_mut::<EditorRegistryState>()
                .material_clipboard = Some(material_id.clone());
            set_editor_status(world, format!("Copied {material_id}"));
        }
        EditorAction::RecipeNextSlot => {
            let Some((_, category, _)) = selected_procedural_recipe(world) else {
                set_editor_status(world, "Select a World Kit recipe before choosing a slot");
                return;
            };
            let slots = persistence::procedural_material_slots(category);
            let slot = {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.recipe_slot = (registry.recipe_slot + 1) % slots.len();
                slots[registry.recipe_slot]
            };
            set_editor_status(
                world,
                format!("Active {} slot: {slot}", world_recipe_label(category)),
            );
        }
        EditorAction::BindRecipeMaterial => {
            let Some((_, slot)) = active_recipe_slot(world) else {
                set_editor_status(world, "Select a World Kit recipe before binding a slot");
                return;
            };
            let material_id = world
                .resource::<EditorRegistryState>()
                .material_clipboard
                .clone();
            let Some(material_id) = material_id else {
                set_editor_status(world, "Copy a published Material before binding");
                return;
            };
            execute_recipe_edit(world, format!("Bind {slot} material"), move |recipe| {
                recipe.material_slots.insert(slot.into(), material_id);
            });
        }
        EditorAction::ClearRecipeMaterial => {
            let Some((_, slot)) = active_recipe_slot(world) else {
                set_editor_status(world, "Select a World Kit recipe before clearing a slot");
                return;
            };
            execute_recipe_edit(world, format!("Clear {slot} material"), move |recipe| {
                recipe.material_slots.remove(slot);
            });
        }
        EditorAction::RegenerateRecipePreview => {
            let Some((content_id, errors)) = selected_recipe_validation_errors(world) else {
                set_editor_status(world, "Select a World Kit recipe before regenerating");
                return;
            };
            if !errors.is_empty() {
                set_editor_status(
                    world,
                    format!("Regeneration blocked for {content_id}: {}", errors[0]),
                );
                return;
            }
            execute_recipe_edit(world, "Regenerate World Kit preview", |recipe| {
                recipe.revision = recipe.revision.wrapping_add(1);
            });
        }
        EditorAction::RecipeNextParameter => {
            let Some((_, category, _)) = selected_procedural_recipe(world) else {
                set_editor_status(
                    world,
                    "Select a World Kit recipe before choosing a parameter",
                );
                return;
            };
            let specs = persistence::procedural_parameter_specs(category);
            let spec = {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.recipe_parameter = (registry.recipe_parameter + 1) % specs.len();
                specs[registry.recipe_parameter]
            };
            set_editor_status(world, format!("Active recipe parameter: {}", spec.label));
        }
        EditorAction::RecipeParameterDecrease => adjust_selected_recipe_parameter(world, -1.0),
        EditorAction::RecipeParameterIncrease => adjust_selected_recipe_parameter(world, 1.0),
        EditorAction::RecipeResetParameters => reset_selected_recipe_parameters(world),
        EditorAction::RecipeSeedDecrease => {
            execute_recipe_edit(world, "Previous World Kit seed", |recipe| {
                recipe.seed = recipe.seed.wrapping_sub(1);
            });
        }
        EditorAction::RecipeSeedIncrease => {
            execute_recipe_edit(world, "Next World Kit seed", |recipe| {
                recipe.seed = recipe.seed.wrapping_add(1);
            });
        }
        EditorAction::ValidateSelectedRecipe => {
            let Some((content_id, errors)) = selected_recipe_validation_errors(world) else {
                set_editor_status(world, "Select a World Kit recipe before validation");
                return;
            };
            if errors.is_empty() {
                set_editor_status(world, format!("{content_id} passes World Kit validation"));
            } else {
                set_editor_status(
                    world,
                    format!("{content_id} has {} issue(s): {}", errors.len(), errors[0]),
                );
            }
        }
        EditorAction::SeedDefaultTopology => {
            let Some((_, category, _)) = selected_procedural_recipe(world) else {
                set_editor_status(world, "Select a World Kit recipe before seeding topology");
                return;
            };
            {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.topology_element = 0;
                registry.topology_edge_start = None;
                registry.topology_edge_start_socket = None;
                registry.topology_socket = 0;
                registry.road_junction_start = None;
            }
            execute_recipe_edit(world, "Seed default World Kit topology", move |recipe| {
                seed_default_topology(category, recipe);
            });
        }
        EditorAction::TopologyPrevious => cycle_selected_topology_element(world, -1),
        EditorAction::TopologyNext => cycle_selected_topology_element(world, 1),
        EditorAction::TopologyMoveXNegative => move_selected_topology_element(world, 0, -1.0),
        EditorAction::TopologyMoveXPositive => move_selected_topology_element(world, 0, 1.0),
        EditorAction::TopologyMoveYNegative => move_selected_topology_element(world, 1, -1.0),
        EditorAction::TopologyMoveYPositive => move_selected_topology_element(world, 1, 1.0),
        EditorAction::TopologyMoveZNegative => move_selected_topology_element(world, 2, -1.0),
        EditorAction::TopologyMoveZPositive => move_selected_topology_element(world, 2, 1.0),
        EditorAction::TopologyMarkEdgeStart => mark_topology_edge_start(world),
        EditorAction::TopologyCycleEdgeKind => cycle_topology_edge_kind(world),
        EditorAction::TopologyConnectEdge => edit_topology_edge(world, true),
        EditorAction::TopologyRemoveEdge => edit_topology_edge(world, false),
        EditorAction::RoadTangentNextAxis => cycle_road_tangent_axis(world),
        EditorAction::RoadTangentDecrease => adjust_selected_road_tangent(world, -1.0),
        EditorAction::RoadTangentIncrease => adjust_selected_road_tangent(world, 1.0),
        EditorAction::RoadTangentReset => reset_selected_road_tangent(world),
        EditorAction::RoadTangentAutoRoundAll => auto_round_all_road_tangents(world),
        EditorAction::RoadMarkJunctionStart => mark_road_junction_start(world),
        EditorAction::RoadCycleJunctionKind => cycle_road_junction_kind(world),
        EditorAction::RoadConnectJunction => edit_road_junction(world, true),
        EditorAction::RoadRemoveJunction => edit_road_junction(world, false),
        EditorAction::SocketNext => cycle_selected_socket(world),
        EditorAction::SocketCycleKind => cycle_topology_socket_kind(world),
        EditorAction::SocketCreate => create_topology_socket(world),
        EditorAction::SocketDelete => delete_selected_topology_socket(world),
        EditorAction::SocketMoveXNegative => move_selected_topology_socket(world, 0, -1.0),
        EditorAction::SocketMoveXPositive => move_selected_topology_socket(world, 0, 1.0),
        EditorAction::SocketMoveYNegative => move_selected_topology_socket(world, 1, -1.0),
        EditorAction::SocketMoveYPositive => move_selected_topology_socket(world, 1, 1.0),
        EditorAction::SocketMoveZNegative => move_selected_topology_socket(world, 2, -1.0),
        EditorAction::SocketMoveZPositive => move_selected_topology_socket(world, 2, 1.0),
        EditorAction::TerrainOriginXNegative => adjust_terrain_origin(world, 0, -1.0),
        EditorAction::TerrainOriginXPositive => adjust_terrain_origin(world, 0, 1.0),
        EditorAction::TerrainOriginZNegative => adjust_terrain_origin(world, 1, -1.0),
        EditorAction::TerrainOriginZPositive => adjust_terrain_origin(world, 1, 1.0),
        EditorAction::TerrainClearanceDecrease => adjust_terrain_clearance(world, -1.0),
        EditorAction::TerrainClearanceIncrease => adjust_terrain_clearance(world, 1.0),
        EditorAction::TerrainProjectSelected => project_recipe_to_terrain(world, false),
        EditorAction::TerrainProjectAll => project_recipe_to_terrain(world, true),
        EditorAction::CompileSandboxRoot => {
            let Some((content_id, _, _)) = selected_procedural_recipe(world) else {
                set_editor_status(
                    world,
                    "Select a World Kit recipe before sandbox compilation",
                );
                return;
            };
            {
                let mut sandbox = world.resource_mut::<WorldKitSandboxState>();
                sandbox.active_content_id = Some(content_id);
                sandbox.applied_signature = None;
            }
            sync_world_kit_sandbox(world);
        }
        EditorAction::ClearSandboxRoot => {
            clear_world_kit_sandbox(world);
            set_editor_status(world, "World Kit sandbox root cleared");
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
/// Despawn every editor primitive and rebuild the world from scene drafts
/// (used by project load and by PM3 level switching).
fn respawn_scene_objects(world: &mut World, objects: &[SceneObjectDraft]) {
    let primitive_entities = world
        .query_filtered::<Entity, With<EditorPrimitive>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in primitive_entities {
        world.despawn(entity);
    }
    for object in objects {
        let id = EditorEntityId(object.editor_id);
        world.resource_mut::<EditorIdAllocator>().reserve(id);
        let entity = spawn_editor_primitive_record(
            world,
            id,
            object.primitive.into(),
            object.name.clone(),
            object.transform.into(),
        );
        if !object.modifiers.is_empty() {
            world
                .entity_mut(entity)
                .insert(EditorModifierStack(object.modifiers.clone()));
            remesh_modified_object(world, entity);
        }
        if let Some(material_id) = &object.material_id {
            if !apply_catalog_material(world, entity, material_id) {
                world
                    .entity_mut(entity)
                    .insert(EditorMaterialBinding(material_id.clone()));
            }
        }
    }
}

/// Collect the live editor world into a scene draft (used by save and by PM3
/// level switching).
fn collect_working_scene(world: &mut World) -> EditorSceneDraft {
    let mut objects = world
        .query::<(
            &EditorEntityId,
            &EditorPrimitive,
            &Transform,
            Option<&Name>,
            Option<&EditorMaterialBinding>,
            Option<&EditorModifierStack>,
        )>()
        .iter(world)
        .map(
            |(id, primitive, transform, name, material, modifiers)| SceneObjectDraft {
                editor_id: id.0,
                name: name
                    .map(|name| name.as_str().to_owned())
                    .unwrap_or_else(|| format!("{} {}", primitive_label(*primitive), id.0)),
                primitive: (*primitive).into(),
                transform: transform.into(),
                material_id: material.map(|binding| binding.0.clone()),
                modifiers: modifiers.map(|stack| stack.0.clone()).unwrap_or_default(),
            },
        )
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

    EditorSceneDraft {
        objects,
        adapter_overrides,
    }
}

fn save_editor_project(world: &mut World, publish: bool) -> bool {
    let scene = collect_working_scene(world);
    let store = world.resource::<EditorProjectSession>().store.clone();
    let path = store.path().display().to_string();
    let result = {
        let mut session = world.resource_mut::<EditorProjectSession>();
        session.project.scene = scene;
        session.project.synchronize_authored_dependencies();
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
            if publish {
                let project = world.resource::<EditorProjectSession>().project.clone();
                rebuild_published_content_catalogs(world, &project);
            }
            set_editor_status(world, format!("Project {verb} atomically to {path}"));
            true
        }
        Err(error) => {
            set_editor_status(world, format!("Save failed: {error}"));
            false
        }
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

    world.resource_mut::<EditorSelection>().clear();
    world.resource_mut::<EditorUndoStack>().clear();
    rebuild_published_content_catalogs(world, &project);
    respawn_scene_objects(world, &project.scene.objects);

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

/// Base mesh for a primitive, matching the sizes used at spawn time.
fn editor_primitive_base_mesh(primitive: EditorPrimitive) -> Option<Mesh> {
    match primitive {
        EditorPrimitive::Empty => None,
        EditorPrimitive::Cube => Some(Cuboid::new(2.5, 2.5, 2.5).into()),
        EditorPrimitive::Pillar => Some(Cylinder::new(1.15, 5.0).into()),
        EditorPrimitive::Beacon => Some(Sphere::new(1.0).into()),
    }
}

/// Re-derive an object's render mesh from its base primitive plus its
/// modifier stack. The base is regenerated every time, so the stack stays
/// non-destructive.
fn remesh_modified_object(world: &mut World, entity: Entity) -> bool {
    let Some(primitive) = world.get::<EditorPrimitive>(entity).copied() else {
        return false;
    };
    let Some(base) = editor_primitive_base_mesh(primitive) else {
        return false;
    };
    let stack = world
        .get::<EditorModifierStack>(entity)
        .map(|stack| stack.0.clone())
        .unwrap_or_default();
    let derived = if stack.is_empty() {
        base
    } else {
        match crate::mesh_modifiers::apply_stack_to_mesh(&base, &stack) {
            Some(mesh) => mesh,
            None => return false,
        }
    };
    let handle = world.resource_mut::<Assets<Mesh>>().add(derived);
    let Some(render_entity) = find_render_mesh_entity(world, entity) else {
        return false;
    };
    world.entity_mut(render_entity).insert(Mesh3d(handle));
    true
}

/// Resolve the active selection for modifier actions, reporting the standard
/// status messages when nothing usable is selected.
fn selected_modifier_target(world: &mut World) -> Option<(Entity, EditorEntityId)> {
    let Some(editor_id) = world.resource::<EditorSelection>().active() else {
        set_editor_status(world, "Select a scene object first");
        return None;
    };
    let Some(entity) = resolve_editor_entity(world, editor_id) else {
        set_editor_status(world, "Selected scene object is missing");
        return None;
    };
    Some((entity, editor_id))
}

/// The entity carrying the object's `Mesh3d` — the object itself or its
/// render child.
fn find_render_mesh_entity(world: &mut World, entity: Entity) -> Option<Entity> {
    if world.get::<Mesh3d>(entity).is_some() {
        return Some(entity);
    }
    let children = world
        .get::<Children>(entity)?
        .iter()
        .collect::<Vec<Entity>>();
    children
        .into_iter()
        .find(|child| world.get::<Mesh3d>(*child).is_some())
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
) -> Entity {
    let (radius, mesh, color) = match primitive {
        EditorPrimitive::Empty => (0.55, None, Color::WHITE),
        EditorPrimitive::Cube => (
            1.75,
            Some(
                world
                    .resource_mut::<Assets<Mesh>>()
                    .add(Cuboid::new(2.5, 2.5, 2.5)),
            ),
            primitive_standard_color(primitive),
        ),
        EditorPrimitive::Pillar => (
            2.7,
            Some(
                world
                    .resource_mut::<Assets<Mesh>>()
                    .add(Cylinder::new(1.15, 5.0)),
            ),
            primitive_standard_color(primitive),
        ),
        EditorPrimitive::Beacon => (
            1.4,
            Some(world.resource_mut::<Assets<Mesh>>().add(Sphere::new(1.0))),
            primitive_standard_color(primitive),
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
    entity.id()
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
    let custom_material = world
        .get::<MeshMaterial3d<ToonMaterial>>(entity)
        .map(|handle| PublishedMaterialHandle::Toon(handle.0.clone()))
        .or_else(|| {
            world
                .get::<MeshMaterial3d<WaterMaterial>>(entity)
                .map(|handle| PublishedMaterialHandle::Water(handle.0.clone()))
        })
        .or_else(|| {
            world
                .get::<MeshMaterial3d<EnergyMaterial>>(entity)
                .map(|handle| PublishedMaterialHandle::Energy(handle.0.clone()))
        })
        .or_else(|| {
            world
                .get::<MeshMaterial3d<ShieldMaterial>>(entity)
                .map(|handle| PublishedMaterialHandle::Shield(handle.0.clone()))
        })
        .or_else(|| {
            world
                .get::<MeshMaterial3d<IceMaterial>>(entity)
                .map(|handle| PublishedMaterialHandle::Ice(handle.0.clone()))
        })
        .or_else(|| {
            world
                .get::<MeshMaterial3d<LavaMaterial>>(entity)
                .map(|handle| PublishedMaterialHandle::Lava(handle.0.clone()))
        });
    let binding = world.get::<EditorMaterialBinding>(entity).cloned();
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
    if let Some(material) = custom_material {
        match material {
            PublishedMaterialHandle::Toon(handle) => duplicate.insert(MeshMaterial3d(handle)),
            PublishedMaterialHandle::Water(handle) => duplicate.insert(MeshMaterial3d(handle)),
            PublishedMaterialHandle::Energy(handle) => duplicate.insert(MeshMaterial3d(handle)),
            PublishedMaterialHandle::Shield(handle) => duplicate.insert(MeshMaterial3d(handle)),
            PublishedMaterialHandle::Ice(handle) => duplicate.insert(MeshMaterial3d(handle)),
            PublishedMaterialHandle::Lava(handle) => duplicate.insert(MeshMaterial3d(handle)),
        };
    }
    if let Some(binding) = binding {
        duplicate.insert(binding);
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
    catalog: Res<PublishedMaterialCatalog>,
    procedural_catalog: Res<PublishedProceduralRecipeCatalog>,
    sandbox: Res<WorldKitSandboxState>,
    registry: Res<EditorRegistryState>,
    authorables: Query<
        (
            &EditorEntityId,
            Option<&Name>,
            Option<&EditorMaterialBinding>,
        ),
        With<Authorable>,
    >,
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
        .filter(|(_, name, _)| {
            filter.text.is_empty()
                || name.is_some_and(|value| {
                    value
                        .as_str()
                        .to_lowercase()
                        .contains(&filter.text.to_lowercase())
                })
        })
        .map(|(id, name, material)| {
            let name = name
                .map(|value| value.as_str().to_owned())
                .unwrap_or_else(|| format!("Object {}", id.0));
            match material {
                Some(binding) => format!("{name}  [{}]", binding.0),
                None => name,
            }
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
    let (level_name, level_position, level_count, startup) = session
        .project
        .active_level()
        .map(|level| {
            let position = session
                .project
                .levels
                .iter()
                .position(|candidate| candidate.level_id == level.level_id)
                .unwrap_or(0);
            (
                level.display_name.as_str(),
                position + 1,
                session.project.levels.len(),
                level.level_id == session.project.startup_level_id,
            )
        })
        .unwrap_or(("—", 0, session.project.levels.len(), false));
    let startup_marker = if startup { " • STARTUP" } else { "" };
    for mut text in &mut text_queries.p0() {
        *text = Text::new(format!(
            "Level {level_position}/{level_count}: {level_name}{startup_marker}\nAuthorable objects: {count}\nSelected: {}\nActive: {active}\n\n{listing}",
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
            let material = match session.project.payloads.get(&record.content_id) {
                Some(persistence::ContentPayload::Material(recipe)) => {
                    let spec = ForgeMaterialSpec::from_recipe(recipe);
                    let controls = material_controls(spec.family);
                    let parameter_index = registry.material_parameter % controls.len();
                    let parameter = controls[parameter_index];
                    format!(
                        "\nMATERIAL: {} • Preset {}\n{}: {:.3}  [{:.3}–{:.3}]",
                        spec.family.label(),
                        spec.preset + 1,
                        parameter.label,
                        spec.values[parameter_index],
                        parameter.min,
                        parameter.max,
                    )
                }
                Some(payload) if payload.procedural_recipe().is_some() => {
                    let recipe = payload.procedural_recipe().unwrap();
                    let slots = persistence::procedural_material_slots(record.category);
                    let slot = slots[registry.recipe_slot % slots.len()];
                    let material = recipe
                        .material_slots
                        .get(slot)
                        .map(String::as_str)
                        .unwrap_or("— fallback —");
                    let specs = persistence::procedural_parameter_specs(record.category);
                    let parameter = specs[registry.recipe_parameter % specs.len()];
                    let value = persistence::procedural_parameter_value(recipe, parameter);
                    let topology_selection = selected_topology_element(
                        record.category,
                        recipe,
                        registry.topology_element,
                    )
                    .map(|(id, position, kind)| {
                        format!(
                            "{kind} {id} @ [{:.2}, {:.2}, {:.2}]",
                            position[0], position[1], position[2]
                        )
                    })
                    .unwrap_or_else(|| "— seed topology to edit —".into());
                    let edge_start = registry
                        .topology_edge_start
                        .as_deref()
                        .unwrap_or("— mark a node —");
                    let edge_kind = TOPOLOGY_EDGE_KINDS
                        [registry.topology_edge_kind % TOPOLOGY_EDGE_KINDS.len()];
                    let authoring = if record.category == persistence::ContentCategory::Road
                        && !recipe.spline_points.is_empty()
                    {
                        let point_index =
                            registry.topology_element % recipe.spline_points.len();
                        let point = &recipe.spline_points[point_index];
                        let tangent = persistence::resolved_road_tangent(
                            &recipe.spline_points,
                            point_index,
                        );
                        let mode = if point.tangent.iter().all(|value| value.abs() <= 1.0e-4) {
                            "AUTO"
                        } else {
                            "EXPLICIT"
                        };
                        let tangent_axis = registry.road_tangent_axis % 3;
                        let junction_kind = ROAD_JUNCTION_KINDS
                            [registry.road_junction_kind % ROAD_JUNCTION_KINDS.len()];
                        let junction_start = registry
                            .road_junction_start
                            .as_deref()
                            .unwrap_or("— mark a road point —");
                        format!(
                            "Tangent {mode}: [{:.1}, {:.1}, {:.1}] • axis {}={:.1}\nJunction: {junction_kind:?} from {junction_start}",
                            tangent[0],
                            tangent[1],
                            tangent[2],
                            ["X", "Y", "Z"][tangent_axis],
                            tangent[tangent_axis]
                        )
                    } else {
                        let socket = selected_topology_node_id(
                            record.category,
                            recipe,
                            registry.topology_element,
                        )
                        .and_then(|node_id| {
                            selected_socket_index(recipe, node_id, registry.topology_socket)
                        })
                        .map(|index| &recipe.topology_sockets[index]);
                        let socket_label = socket.map_or_else(
                            || format!(
                                "— no socket • new {:?} —",
                                TOPOLOGY_SOCKET_KINDS[registry.topology_socket_kind
                                    % TOPOLOGY_SOCKET_KINDS.len()]
                            ),
                            |socket| {
                                format!(
                                    "{:?} {} @ [{:.1}, {:.1}, {:.1}]",
                                    socket.kind,
                                    socket.socket_id,
                                    socket.local_position[0],
                                    socket.local_position[1],
                                    socket.local_position[2]
                                )
                            },
                        );
                        format!(
                            "Socket: {socket_label}\nEdge: {edge_kind:?} from {edge_start}{}",
                            registry
                                .topology_edge_start_socket
                                .as_deref()
                                .map(|socket| format!(" via {socket}"))
                                .unwrap_or_default()
                        )
                    };
                    let terrain_target = terrain_projection_y(
                        record.category,
                        recipe,
                        registry.topology_element
                            % topology_element_count(record.category, recipe).max(1),
                    )
                    .map(|height| format!("{height:.2}m"))
                    .unwrap_or_else(|| "—".into());
                    format!(
                        "\nWORLD KIT: {} • Seed {} • Rev {}\nTopology: {} points • {} junctions • {} nodes • {} sockets • {} edges\nTerrain origin: [{:.0}, {:.0}] • clearance {:.2}m • selected target {terrain_target}\nSelected: {}\n{}\nSlot {}: {}\n{}: {:.2}  [{:.2}–{:.2}]",
                        world_recipe_label(record.category),
                        recipe.seed,
                        recipe.revision,
                        recipe.spline_points.len(),
                        recipe.road_junctions.len(),
                        recipe.topology_nodes.len(),
                        recipe.topology_sockets.len(),
                        recipe.topology_edges.len(),
                        recipe.terrain_projection.world_origin[0],
                        recipe.terrain_projection.world_origin[1],
                        recipe.terrain_projection.clearance,
                        topology_selection,
                        authoring,
                        slot,
                        material,
                        parameter.label,
                        value,
                        parameter.min,
                        parameter.max,
                    )
                }
                _ => String::new(),
            };
            format!(
                "\n{:?} • {state}\n{}\nHash: {}{material}",
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
    let recipe_type = WORLD_RECIPE_CATEGORIES[registry.recipe_kind % WORLD_RECIPE_CATEGORIES.len()];
    let clipboard = registry.material_clipboard.as_deref().unwrap_or("empty");
    let sandbox_label = sandbox.active_content_id.as_deref().unwrap_or("inactive");
    for mut text in &mut text_queries.p3() {
        *text = Text::new(format!(
            "Project: {}\nRecords: {} • Matches: {} • Runtime materials: {} • Recipes: {}{search}\nNew recipe: {} • Material clipboard: {}\nSandbox: {}\n\n{record_listing}{selected_details}{rename}\n\n{validation}",
            session.project.display_name,
            records.len(),
            registry_indices.len(),
            catalog.len(),
            procedural_catalog.len(),
            world_recipe_label(recipe_type),
            clipboard,
            sandbox_label,
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
    session: Res<EditorProjectSession>,
    registry: Res<EditorRegistryState>,
    sandbox_roots: Query<(&WorldKitGeneratedRoot, &GlobalTransform)>,
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

    if let Some(record) = session.project.records.get(registry.selected) {
        if let Some(recipe) = session
            .project
            .payloads
            .get(&record.content_id)
            .and_then(persistence::ContentPayload::procedural_recipe)
        {
            if let Some((_, position, _)) =
                selected_topology_element(record.category, recipe, registry.topology_element)
            {
                if let Some((_, root_transform)) = sandbox_roots
                    .iter()
                    .find(|(root, _)| root.content_id == record.content_id)
                {
                    let local = Vec3::from_array(position);
                    let center = root_transform.transform_point(local);
                    let element_count = topology_element_count(record.category, recipe);
                    let element_index = registry.topology_element % element_count.max(1);
                    if let Some(target_y) =
                        terrain_projection_y(record.category, recipe, element_index)
                    {
                        let target =
                            root_transform.transform_point(Vec3::new(local.x, target_y, local.z));
                        let terrain_color = Color::srgb(0.35, 1.0, 0.58);
                        gizmos.line(
                            target - Vec3::X * 0.5,
                            target + Vec3::X * 0.5,
                            terrain_color,
                        );
                        gizmos.line(
                            target - Vec3::Z * 0.5,
                            target + Vec3::Z * 0.5,
                            terrain_color,
                        );
                        if center.distance_squared(target) > 0.0001 {
                            gizmos.line(center, target, terrain_color);
                        }
                    }
                    for (axis, color) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().zip([
                        Color::srgb(1.0, 0.18, 0.18),
                        Color::srgb(0.25, 1.0, 0.35),
                        Color::srgb(0.22, 0.55, 1.0),
                    ]) {
                        let endpoint = root_transform.transform_point(local + axis * 10.0);
                        gizmos.arrow(center, endpoint, color);
                    }
                    gizmos.line(
                        center + Vec3::Y * 1.25,
                        center + Vec3::Y * 1.8,
                        Color::srgb(1.0, 0.82, 0.18),
                    );
                    if record.category == persistence::ContentCategory::Road {
                        let point_index = registry.topology_element % recipe.spline_points.len();
                        let tangent = Vec3::from_array(persistence::resolved_road_tangent(
                            &recipe.spline_points,
                            point_index,
                        ));
                        let tangent_end = root_transform.transform_point(local + tangent);
                        gizmos.arrow(center, tangent_end, Color::srgb(1.0, 0.24, 0.82));
                        if let Some(start_id) = registry.road_junction_start.as_deref() {
                            if let Some(start_point) = recipe
                                .spline_points
                                .iter()
                                .find(|point| point.point_id == start_id)
                            {
                                let start = root_transform
                                    .transform_point(Vec3::from_array(start_point.position));
                                let start_color = Color::srgb(0.18, 0.92, 1.0);
                                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                                    gizmos.line(
                                        start - axis * 0.55,
                                        start + axis * 0.55,
                                        start_color,
                                    );
                                }
                                if start != center {
                                    gizmos.line(start, center, start_color);
                                }
                            }
                        }
                    } else if let Some(node_id) = selected_topology_node_id(
                        record.category,
                        recipe,
                        registry.topology_element,
                    ) {
                        if let Some(socket_index) =
                            selected_socket_index(recipe, node_id, registry.topology_socket)
                        {
                            let socket = &recipe.topology_sockets[socket_index];
                            let socket_center = root_transform
                                .transform_point(local + Vec3::from_array(socket.local_position));
                            let socket_color = Color::srgb(1.0, 0.48, 0.12);
                            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                                gizmos.line(
                                    socket_center - axis * 0.35,
                                    socket_center + axis * 0.35,
                                    socket_color,
                                );
                            }
                            let facing = Vec3::from_array(socket.facing).normalize_or(Vec3::Z);
                            gizmos.arrow(socket_center, socket_center + facing * 0.9, socket_color);
                        }
                    }
                    if let Some(start_id) = registry.topology_edge_start.as_deref() {
                        if let Some(start_node) = recipe
                            .topology_nodes
                            .iter()
                            .find(|node| node.node_id == start_id)
                        {
                            let start_local = Vec3::from_array(start_node.position)
                                + registry
                                    .topology_edge_start_socket
                                    .as_deref()
                                    .and_then(|socket_id| {
                                        recipe
                                            .topology_sockets
                                            .iter()
                                            .find(|socket| socket.socket_id == socket_id)
                                    })
                                    .map(|socket| Vec3::from_array(socket.local_position))
                                    .unwrap_or(Vec3::ZERO);
                            let start = root_transform.transform_point(start_local);
                            let start_color = Color::srgb(0.18, 0.92, 1.0);
                            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                                gizmos.line(start - axis * 0.55, start + axis * 0.55, start_color);
                            }
                            if start != center {
                                gizmos.line(start, center, start_color);
                            }
                        }
                    }
                }
            }
        }
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
        world.init_resource::<Assets<ToonMaterial>>();
        world.init_resource::<Assets<WaterMaterial>>();
        world.init_resource::<Assets<EnergyMaterial>>();
        world.init_resource::<Assets<ShieldMaterial>>();
        world.init_resource::<Assets<IceMaterial>>();
        world.init_resource::<Assets<LavaMaterial>>();
        world.init_resource::<PublishedMaterialCatalog>();
        world.init_resource::<PublishedProceduralRecipeCatalog>();
        world.init_resource::<PublishedCreatureCatalog>();
        world.init_resource::<PublishedContentCatalogEpoch>();
        world.init_resource::<WorldKitPreviewState>();
        world.init_resource::<WorldKitSandboxState>();
        world.init_resource::<WorldKitSandboxAssets>();
        world.init_resource::<EditorSelection>();
        world.init_resource::<EditorIdAllocator>();
        world.init_resource::<EditorUndoStack>();
        world.init_resource::<EditorDocumentState>();
        world.init_resource::<EditorRegistryState>();
        world.init_resource::<EditorGizmoSettings>();
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
    fn topology_drag_projects_onto_axis_and_snaps_in_recipe_meters() {
        assert_eq!(
            topology_drag_amount(Vec2::new(17.0, 9.0), Vec2::X, 0.1, 0.5),
            1.5
        );
        assert_eq!(
            topology_drag_amount(Vec2::new(17.0, 9.0), Vec2::Y, 0.1, 0.5),
            1.0
        );
        assert_eq!(
            topology_drag_amount(Vec2::new(-13.0, 4.0), Vec2::X, 0.1, 0.25),
            -1.25
        );
    }

    #[test]
    fn topology_position_mut_targets_only_the_selected_recipe_element() {
        let mut road = ProceduralRecipeDraft::default();
        seed_default_topology(persistence::ContentCategory::Road, &mut road);
        let untouched = road.spline_points[0].position;
        topology_position_mut(persistence::ContentCategory::Road, &mut road, 1).unwrap()[0] += 3.0;
        assert_eq!(road.spline_points[0].position, untouched);
        assert_eq!(road.spline_points[1].position[0], 3.0);

        let mut cave = ProceduralRecipeDraft::default();
        seed_default_topology(persistence::ContentCategory::Cave, &mut cave);
        topology_position_mut(persistence::ContentCategory::Cave, &mut cave, 2).unwrap()[1] += 2.0;
        assert_eq!(cave.topology_nodes[2].position[1], 7.0);
        assert!(
            topology_position_mut(persistence::ContentCategory::Cave, &mut cave, usize::MAX)
                .is_none()
        );
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

    #[test]
    fn every_shader_family_round_trips_typed_forge_controls() {
        for family in ForgeMaterialFamily::ALL {
            let authored = ForgeMaterialSpec::preset(family, 1);
            let mut recipe = GenericRecipeDraft::default();
            authored.write_to(&mut recipe);

            assert_eq!(ForgeMaterialSpec::from_recipe(&recipe), authored);
            assert_eq!(
                recipe.fields["shader"].as_str(),
                Some(family.id()),
                "{} payload lost its shader identity",
                family.label(),
            );
        }
    }

    #[test]
    fn toon_authoring_keeps_ordered_bands_after_extreme_input() {
        let mut recipe = GenericRecipeDraft::default();
        recipe
            .fields
            .insert("shader".into(), serde_json::json!("toon_v1"));
        recipe
            .fields
            .insert("shadow_threshold".into(), serde_json::json!(0.8));
        recipe
            .fields
            .insert("light_threshold".into(), serde_json::json!(0.2));
        recipe
            .fields
            .insert("shadow_level".into(), serde_json::json!(0.8));
        recipe
            .fields
            .insert("mid_level".into(), serde_json::json!(0.2));

        let spec = ForgeMaterialSpec::from_recipe(&recipe);
        assert!(spec.values[0] < spec.values[1]);
        assert!(spec.values[2] < spec.values[3]);
    }

    #[test]
    fn authored_toon_values_override_legacy_nested_seed_fields() {
        let persistence::ContentPayload::Material(mut recipe) =
            persistence::ContentPayload::empty(persistence::ContentCategory::Material)
        else {
            unreachable!();
        };
        let authored = ForgeMaterialSpec::preset(ForgeMaterialFamily::Toon, 1);
        authored.write_to(&mut recipe);

        assert_eq!(ForgeMaterialSpec::from_recipe(&recipe), authored);
    }

    #[test]
    fn published_catalog_resolves_stable_id_to_typed_object_material() {
        let (mut world, root) = persistence_test_world("published_material_catalog");
        let material_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Material, "Hero Ink")
            .unwrap();
        world
            .resource_mut::<EditorProjectSession>()
            .project
            .publish_drafts()
            .unwrap();
        let project = world.resource::<EditorProjectSession>().project.clone();
        rebuild_published_content_catalogs(&mut world, &project);
        let entity = spawn_editor_primitive_record(
            &mut world,
            EditorEntityId(88),
            EditorPrimitive::Cube,
            "Catalog block".into(),
            Transform::default(),
        );

        assert!(apply_catalog_material(&mut world, entity, &material_id));
        assert_eq!(
            world.get::<EditorMaterialBinding>(entity),
            Some(&EditorMaterialBinding(material_id.clone()))
        );
        assert!(world.get::<MeshMaterial3d<ToonMaterial>>(entity).is_some());
        assert!(world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn published_object_material_binding_survives_project_reload() {
        let (mut world, root) = persistence_test_world("material_binding_reload");
        let object_id = EditorEntityId(91);
        spawn_editor_primitive_record(
            &mut world,
            object_id,
            EditorPrimitive::Cube,
            "Bound block".into(),
            Transform::default(),
        );
        world.resource_mut::<EditorSelection>().replace(object_id);
        let material_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Material, "Reload Ink")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;

        save_editor_project(&mut world, true);
        apply_selected_material_to_object(&mut world);
        save_editor_project(&mut world, true);
        load_editor_project(&mut world, false);

        let entity = resolve_editor_entity(&mut world, object_id).unwrap();
        assert_eq!(
            world.get::<EditorMaterialBinding>(entity),
            Some(&EditorMaterialBinding(material_id))
        );
        assert!(world.get::<MeshMaterial3d<ToonMaterial>>(entity).is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn published_creature_records_reach_the_runtime_catalog() {
        let (mut world, root) = persistence_test_world("creature_catalog");
        let content_id = {
            let mut session = world.resource_mut::<EditorProjectSession>();
            let spec = crate::robots::creature::CreatureSpec {
                content_id: "starfall.creature-test".into(),
                display_name: "Catalog Stalker".into(),
                seed: 99,
                ..Default::default()
            };
            creature_records::upsert_creature(&mut session.project, &spec).unwrap();
            session.project.refresh_hashes().unwrap();
            spec.content_id
        };

        // Draft-only records must not reach the runtime catalog.
        let project = world.resource::<EditorProjectSession>().project.clone();
        rebuild_published_content_catalogs(&mut world, &project);
        assert!(!world
            .resource::<PublishedCreatureCatalog>()
            .contains(&content_id));

        world
            .resource_mut::<EditorProjectSession>()
            .project
            .publish_drafts()
            .unwrap();
        let project = world.resource::<EditorProjectSession>().project.clone();
        rebuild_published_content_catalogs(&mut world, &project);
        let catalog = world.resource::<PublishedCreatureCatalog>();
        let published = catalog
            .get(&content_id)
            .expect("published creature is cataloged");
        assert_eq!(published.seed, 99);
        assert_eq!(catalog.len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn procedural_binding_resolves_once_and_restores_fallback_when_draft_changes() {
        let (mut world, root) = persistence_test_world("procedural_binding");
        let (material_id, road_id) = {
            let mut session = world.resource_mut::<EditorProjectSession>();
            let material_id = session
                .project
                .create_content(persistence::ContentCategory::Material, "Road Toon")
                .unwrap();
            let road_id = session
                .project
                .create_content(persistence::ContentCategory::Road, "Speed Network")
                .unwrap();
            let recipe = session
                .project
                .payloads
                .get_mut(&road_id)
                .unwrap()
                .procedural_recipe_mut()
                .unwrap();
            recipe.seed = 42_195;
            recipe.revision = 3;
            recipe
                .material_slots
                .insert("deck".into(), material_id.clone());
            session.project.publish_drafts().unwrap();
            (material_id, road_id)
        };
        let project = world.resource::<EditorProjectSession>().project.clone();
        rebuild_published_content_catalogs(&mut world, &project);
        assert_eq!(
            world
                .resource::<PublishedProceduralRecipeCatalog>()
                .generation(&road_id),
            Some((ProceduralRecipeKind::Road, 42_195, 3))
        );
        assert_eq!(
            world
                .resource::<PublishedProceduralRecipeCatalog>()
                .material_id(&road_id, "deck"),
            Some(material_id.as_str())
        );

        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 0.5, 8.0));
        let fallback = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(Color::srgb(0.2, 0.2, 0.2));
        let entity = world
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(fallback.clone()),
                ProceduralMaterialBinding::new(road_id.clone(), "deck"),
            ))
            .id();
        resolve_procedural_material_bindings(&mut world);
        assert!(world.get::<MeshMaterial3d<ToonMaterial>>(entity).is_some());
        assert!(world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .is_none());

        {
            let mut session = world.resource_mut::<EditorProjectSession>();
            let recipe = session
                .project
                .payloads
                .get_mut(&road_id)
                .unwrap()
                .procedural_recipe_mut()
                .unwrap();
            recipe.revision += 1;
            recipe.material_slots.clear();
            session.project.refresh_hashes().unwrap();
        }
        let changed = world.resource::<EditorProjectSession>().project.clone();
        rebuild_published_content_catalogs(&mut world, &changed);
        resolve_procedural_material_bindings(&mut world);
        assert_eq!(
            world.get::<MeshMaterial3d<StandardMaterial>>(entity),
            Some(&MeshMaterial3d(fallback))
        );
        assert!(world.get::<MeshMaterial3d<ToonMaterial>>(entity).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn world_kit_controller_actions_bind_and_regenerate_with_undo() {
        let (mut world, root) = persistence_test_world("world_kit_actions");
        let material_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Material, "Road Toon")
            .unwrap();
        world
            .resource_mut::<EditorProjectSession>()
            .project
            .publish_drafts()
            .unwrap();
        let project = world.resource::<EditorProjectSession>().project.clone();
        rebuild_published_content_catalogs(&mut world, &project);
        world.resource_mut::<EditorRegistryState>().selected = 1;
        apply_editor_action(&mut world, EditorAction::CopySelectedMaterial);
        assert_eq!(
            world
                .resource::<EditorRegistryState>()
                .material_clipboard
                .as_deref(),
            Some(material_id.as_str())
        );

        world.resource_mut::<EditorRegistryState>().recipe_kind = 1;
        apply_editor_action(&mut world, EditorAction::CreateRecipeRecord);
        let road_id = world
            .resource::<EditorProjectSession>()
            .project
            .records
            .last()
            .unwrap()
            .content_id
            .clone();
        assert_eq!(road_id, "road.speed_network");
        apply_editor_action(&mut world, EditorAction::BindRecipeMaterial);
        let recipe = world.resource::<EditorProjectSession>().project.payloads[&road_id]
            .procedural_recipe()
            .unwrap();
        assert_eq!(recipe.material_slots.get("deck"), Some(&material_id));

        apply_editor_action(&mut world, EditorAction::RegenerateRecipePreview);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .revision,
            1
        );
        apply_editor_action(&mut world, EditorAction::Undo);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .revision,
            0
        );
        apply_editor_action(&mut world, EditorAction::Redo);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .revision,
            1
        );
        apply_editor_action(&mut world, EditorAction::RecipeParameterIncrease);
        let recipe = world.resource::<EditorProjectSession>().project.payloads[&road_id]
            .procedural_recipe()
            .unwrap();
        assert_eq!(recipe.fields["width"], serde_json::json!(19.0));
        apply_editor_action(&mut world, EditorAction::Undo);
        assert!(
            !world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .fields
                .contains_key("width")
        );

        apply_editor_action(&mut world, EditorAction::RecipeSeedIncrease);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .seed,
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn world_kit_preview_regeneration_is_bounded_to_four_slot_objects() {
        let (mut world, root) = persistence_test_world("world_kit_preview");
        let road_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Road, "Speed Network")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        sync_world_kit_preview(&mut world);
        assert_eq!(
            world
                .query_filtered::<Entity, With<WorldKitPreview>>()
                .iter(&world)
                .count(),
            4
        );

        world
            .resource_mut::<EditorProjectSession>()
            .project
            .payloads
            .get_mut(&road_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap()
            .revision = 8;
        sync_world_kit_preview(&mut world);
        assert_eq!(
            world
                .query_filtered::<Entity, With<WorldKitPreview>>()
                .iter(&world)
                .count(),
            4
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_world_recipe_cannot_regenerate_preview_revision() {
        let (mut world, root) = persistence_test_world("invalid_world_kit_regeneration");
        let building_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Building, "Huge Tower")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        {
            let mut session = world.resource_mut::<EditorProjectSession>();
            let recipe = session
                .project
                .payloads
                .get_mut(&building_id)
                .unwrap()
                .procedural_recipe_mut()
                .unwrap();
            recipe.fields.insert("floors".into(), serde_json::json!(20));
            recipe
                .fields
                .insert("rooms_per_floor".into(), serde_json::json!(12));
        }
        apply_editor_action(&mut world, EditorAction::RegenerateRecipePreview);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&building_id]
                .procedural_recipe()
                .unwrap()
                .revision,
            0
        );
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("blocked"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn editor_focus_reveal_delta_moves_only_outside_items() {
        assert_eq!(editor_focus_reveal_delta(10.0, 90.0, 30.0, 50.0), 0.0);
        assert_eq!(editor_focus_reveal_delta(10.0, 90.0, -5.0, 15.0), 15.0);
        assert_eq!(editor_focus_reveal_delta(10.0, 90.0, 85.0, 105.0), -15.0);
    }

    #[test]
    fn default_topology_is_transactional_and_compiles_each_world_family() {
        let (mut world, root) = persistence_test_world("default_topology_compile");
        for category in WORLD_RECIPE_CATEGORIES {
            let content_id = world
                .resource_mut::<EditorProjectSession>()
                .project
                .create_content(category, world_recipe_default_name(category))
                .unwrap();
            let selected = world
                .resource::<EditorProjectSession>()
                .project
                .records
                .iter()
                .position(|record| record.content_id == content_id)
                .unwrap();
            world.resource_mut::<EditorRegistryState>().selected = selected;
            apply_editor_action(&mut world, EditorAction::SeedDefaultTopology);
            let (_, _, recipe) = selected_procedural_recipe(&world).unwrap();
            assert!(validate_project(&world.resource::<EditorProjectSession>().project).is_empty());
            let compiled = compile_world_kit(category, &recipe).unwrap();
            assert!(!compiled.is_empty());
            assert!(compiled.len() <= 192);
        }
        apply_editor_action(&mut world, EditorAction::Undo);
        let (_, _, recipe) = selected_procedural_recipe(&world).unwrap();
        assert!(recipe.spline_points.is_empty());
        assert!(recipe.topology_nodes.is_empty());
        assert!(recipe.topology_sockets.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn topology_selection_wraps_and_snapped_moves_are_undoable() {
        let (mut world, root) = persistence_test_world("topology_direct_edit");
        let road_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Road, "Speed Network")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        apply_editor_action(&mut world, EditorAction::SeedDefaultTopology);
        let point_count = world.resource::<EditorProjectSession>().project.payloads[&road_id]
            .procedural_recipe()
            .unwrap()
            .spline_points
            .len();

        apply_editor_action(&mut world, EditorAction::TopologyPrevious);
        assert_eq!(
            world.resource::<EditorRegistryState>().topology_element,
            point_count - 1
        );
        apply_editor_action(&mut world, EditorAction::TopologyNext);
        assert_eq!(world.resource::<EditorRegistryState>().topology_element, 0);
        apply_editor_action(&mut world, EditorAction::TopologyNext);
        assert_eq!(world.resource::<EditorRegistryState>().topology_element, 1);

        let before = world.resource::<EditorProjectSession>().project.payloads[&road_id]
            .procedural_recipe()
            .unwrap()
            .spline_points[1]
            .position;
        apply_editor_action(&mut world, EditorAction::TopologyMoveZPositive);
        let after = world.resource::<EditorProjectSession>().project.payloads[&road_id]
            .procedural_recipe()
            .unwrap()
            .spline_points[1]
            .position;
        assert_eq!(after[2], before[2] + 0.5);
        assert!(validate_project(&world.resource::<EditorProjectSession>().project).is_empty());

        apply_editor_action(&mut world, EditorAction::Undo);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .spline_points[1]
                .position,
            before
        );
        apply_editor_action(&mut world, EditorAction::Redo);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .spline_points[1]
                .position,
            after
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn topology_move_without_seeded_elements_is_safe() {
        let (mut world, root) = persistence_test_world("topology_empty_edit");
        world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Cave, "Empty Cave")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        apply_editor_action(&mut world, EditorAction::TopologyMoveXPositive);
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("Seed topology"));
        assert_eq!(world.resource::<EditorUndoStack>().undo.len(), 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn terrain_projection_is_controller_driven_transactional_and_collider_exact() {
        let (mut world, root) = persistence_test_world("terrain_projection_authoring");
        let building_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Building, "Mountain Rooms")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        apply_editor_action(&mut world, EditorAction::SeedDefaultTopology);
        apply_editor_action(&mut world, EditorAction::TerrainOriginXPositive);
        apply_editor_action(&mut world, EditorAction::TerrainOriginZNegative);
        apply_editor_action(&mut world, EditorAction::TerrainClearanceIncrease);

        let (before, expected_selected) = {
            let recipe = world.resource::<EditorProjectSession>().project.payloads[&building_id]
                .procedural_recipe()
                .unwrap();
            assert_eq!(recipe.terrain_projection.world_origin, [250.0, -250.0]);
            assert_eq!(recipe.terrain_projection.clearance, 1.5);
            (
                recipe.topology_nodes[0].position[1],
                terrain_projection_y(persistence::ContentCategory::Building, recipe, 0).unwrap(),
            )
        };
        apply_editor_action(&mut world, EditorAction::TerrainProjectSelected);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&building_id]
                .procedural_recipe()
                .unwrap()
                .topology_nodes[0]
                .position[1],
            expected_selected
        );
        apply_editor_action(&mut world, EditorAction::Undo);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&building_id]
                .procedural_recipe()
                .unwrap()
                .topology_nodes[0]
                .position[1],
            before
        );
        apply_editor_action(&mut world, EditorAction::Redo);

        let expected_all = {
            let recipe = world.resource::<EditorProjectSession>().project.payloads[&building_id]
                .procedural_recipe()
                .unwrap();
            (0..recipe.topology_nodes.len())
                .map(|index| {
                    terrain_projection_y(persistence::ContentCategory::Building, recipe, index)
                        .unwrap()
                })
                .collect::<Vec<_>>()
        };
        apply_editor_action(&mut world, EditorAction::TerrainProjectAll);
        let recipe = world.resource::<EditorProjectSession>().project.payloads[&building_id]
            .procedural_recipe()
            .unwrap();
        assert!(recipe
            .topology_nodes
            .iter()
            .zip(expected_all)
            .all(|(node, expected)| node.position[1] == expected));
        assert!(validate_project(&world.resource::<EditorProjectSession>().project).is_empty());
        assert!(compile_world_kit(persistence::ContentCategory::Building, recipe).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn typed_topology_edges_connect_remove_and_undo_as_recipe_transactions() {
        let (mut world, root) = persistence_test_world("topology_edge_authoring");
        let biome_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Biome, "Main World")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        apply_editor_action(&mut world, EditorAction::SeedDefaultTopology);
        apply_editor_action(&mut world, EditorAction::TopologyMarkEdgeStart);
        apply_editor_action(&mut world, EditorAction::TopologyNext);
        apply_editor_action(&mut world, EditorAction::TopologyConnectEdge);

        let recipe = world.resource::<EditorProjectSession>().project.payloads[&biome_id]
            .procedural_recipe()
            .unwrap();
        assert_eq!(recipe.topology_edges.len(), 1);
        assert_eq!(recipe.topology_edges[0].from, "core");
        assert_eq!(recipe.topology_edges[0].to, "water");
        assert_eq!(recipe.topology_edges[0].kind, WorldTopologyEdgeKind::Door);
        assert!(validate_project(&world.resource::<EditorProjectSession>().project).is_empty());

        apply_editor_action(&mut world, EditorAction::TopologyConnectEdge);
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("already exists"));
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&biome_id]
                .procedural_recipe()
                .unwrap()
                .topology_edges
                .len(),
            1
        );

        apply_editor_action(&mut world, EditorAction::TopologyRemoveEdge);
        assert!(
            world.resource::<EditorProjectSession>().project.payloads[&biome_id]
                .procedural_recipe()
                .unwrap()
                .topology_edges
                .is_empty()
        );
        apply_editor_action(&mut world, EditorAction::Undo);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&biome_id]
                .procedural_recipe()
                .unwrap()
                .topology_edges
                .len(),
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_cave_edge_removal_retains_last_valid_sandbox_root() {
        let (mut world, root) = persistence_test_world("topology_edge_sandbox_guard");
        let cave_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Cave, "Secret Network")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        apply_editor_action(&mut world, EditorAction::SeedDefaultTopology);
        apply_editor_action(&mut world, EditorAction::CompileSandboxRoot);
        let valid_root = world
            .query_filtered::<Entity, With<WorldKitGeneratedRoot>>()
            .iter(&world)
            .next()
            .unwrap();

        apply_editor_action(&mut world, EditorAction::TopologyMarkEdgeStart);
        apply_editor_action(&mut world, EditorAction::TopologyNext);
        apply_editor_action(&mut world, EditorAction::TopologyCycleEdgeKind);
        apply_editor_action(&mut world, EditorAction::TopologyCycleEdgeKind);
        assert_eq!(
            TOPOLOGY_EDGE_KINDS[world.resource::<EditorRegistryState>().topology_edge_kind],
            WorldTopologyEdgeKind::Tunnel
        );
        apply_editor_action(&mut world, EditorAction::TopologyRemoveEdge);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&cave_id]
                .procedural_recipe()
                .unwrap()
                .topology_edges
                .len(),
            2
        );
        assert!(!validate_project(&world.resource::<EditorProjectSession>().project).is_empty());

        sync_world_kit_sandbox(&mut world);
        let retained_root = world
            .query_filtered::<Entity, With<WorldKitGeneratedRoot>>()
            .iter(&world)
            .next()
            .unwrap();
        assert_eq!(retained_root, valid_root);
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("retained last valid"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn road_tangents_and_junctions_are_transactional_and_compile_curves() {
        let (mut world, root) = persistence_test_world("road_tangent_junction_authoring");
        let road_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Road, "Curve Network")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        apply_editor_action(&mut world, EditorAction::SeedDefaultTopology);
        apply_editor_action(&mut world, EditorAction::TopologyNext);

        let automatic = {
            let recipe = world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap();
            persistence::resolved_road_tangent(&recipe.spline_points, 1)
        };
        apply_editor_action(&mut world, EditorAction::RoadTangentIncrease);
        let explicit = world.resource::<EditorProjectSession>().project.payloads[&road_id]
            .procedural_recipe()
            .unwrap()
            .spline_points[1]
            .tangent;
        assert_eq!(explicit[0], automatic[0] + 0.5);
        apply_editor_action(&mut world, EditorAction::Undo);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .spline_points[1]
                .tangent,
            [0.0; 3]
        );

        apply_editor_action(&mut world, EditorAction::RoadMarkJunctionStart);
        apply_editor_action(&mut world, EditorAction::TopologyNext);
        apply_editor_action(&mut world, EditorAction::RoadConnectJunction);
        let (_, _, recipe) = selected_procedural_recipe(&world).unwrap();
        assert_eq!(recipe.road_junctions.len(), 1);
        assert_eq!(recipe.road_junctions[0].kind, RoadJunctionKind::Merge);
        assert!(validate_project(&world.resource::<EditorProjectSession>().project).is_empty());
        let compiled = compile_world_kit(persistence::ContentCategory::Road, &recipe).unwrap();
        let junction_parts = compiled
            .iter()
            .filter(|part| part.name.contains("junction"))
            .count();
        assert!(junction_parts >= 36);
        assert!(compiled
            .iter()
            .any(|part| part.name.contains("junction") && part.name.contains("barrier")));
        assert!(compiled.iter().any(|part| part.name.contains("Road curve")));

        apply_editor_action(&mut world, EditorAction::RoadRemoveJunction);
        assert!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .road_junctions
                .is_empty()
        );
        apply_editor_action(&mut world, EditorAction::Undo);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .road_junctions
                .len(),
            1
        );

        apply_editor_action(&mut world, EditorAction::RoadTangentIncrease);
        apply_editor_action(&mut world, EditorAction::TopologyNext);
        apply_editor_action(&mut world, EditorAction::RoadTangentIncrease);
        apply_editor_action(&mut world, EditorAction::RoadTangentAutoRoundAll);
        assert!(
            world.resource::<EditorProjectSession>().project.payloads[&road_id]
                .procedural_recipe()
                .unwrap()
                .spline_points
                .iter()
                .all(|point| point.tangent == [0.0; 3])
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn doorway_sockets_bind_edges_compile_offsets_and_delete_safely() {
        let (mut world, root) = persistence_test_world("doorway_socket_authoring");
        let building_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Building, "Socket Building")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        apply_editor_action(&mut world, EditorAction::SeedDefaultTopology);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&building_id]
                .procedural_recipe()
                .unwrap()
                .topology_sockets
                .len(),
            2
        );

        apply_editor_action(&mut world, EditorAction::SocketCreate);
        apply_editor_action(&mut world, EditorAction::SocketNext);
        let created_id = world.resource::<EditorProjectSession>().project.payloads[&building_id]
            .procedural_recipe()
            .unwrap()
            .topology_sockets[2]
            .socket_id
            .clone();
        apply_editor_action(&mut world, EditorAction::SocketMoveYPositive);
        assert_eq!(
            world.resource::<EditorProjectSession>().project.payloads[&building_id]
                .procedural_recipe()
                .unwrap()
                .topology_sockets[2]
                .local_position[1],
            0.5
        );
        apply_editor_action(&mut world, EditorAction::TopologyMarkEdgeStart);
        apply_editor_action(&mut world, EditorAction::TopologyNext);
        apply_editor_action(&mut world, EditorAction::TopologyCycleEdgeKind);
        apply_editor_action(&mut world, EditorAction::TopologyConnectEdge);

        let (_, _, recipe) = selected_procedural_recipe(&world).unwrap();
        let stair = recipe
            .topology_edges
            .iter()
            .find(|edge| {
                edge.kind == WorldTopologyEdgeKind::Stair
                    && edge.from == "entrance"
                    && edge.to == "lobby"
            })
            .unwrap();
        assert_eq!(stair.from_socket.as_deref(), Some(created_id.as_str()));
        assert_eq!(stair.to_socket.as_deref(), Some("lobby.doorway.in"));
        assert!(validate_project(&world.resource::<EditorProjectSession>().project).is_empty());
        let compiled = compile_world_kit(persistence::ContentCategory::Building, &recipe).unwrap();
        let connector = compiled
            .iter()
            .find(|part| part.name == "Stair connector 3")
            .unwrap();
        assert_ne!(connector.transform.translation, Vec3::new(-6.0, 0.0, 0.0));

        apply_editor_action(&mut world, EditorAction::TopologyPrevious);
        apply_editor_action(&mut world, EditorAction::SocketNext);
        apply_editor_action(&mut world, EditorAction::SocketDelete);
        let recipe = world.resource::<EditorProjectSession>().project.payloads[&building_id]
            .procedural_recipe()
            .unwrap();
        assert!(!recipe
            .topology_sockets
            .iter()
            .any(|socket| socket.socket_id == created_id));
        assert!(recipe
            .topology_edges
            .iter()
            .find(|edge| {
                edge.kind == WorldTopologyEdgeKind::Stair
                    && edge.from == "entrance"
                    && edge.to == "lobby"
            })
            .unwrap()
            .from_socket
            .is_none());
        apply_editor_action(&mut world, EditorAction::Undo);
        let recipe = world.resource::<EditorProjectSession>().project.payloads[&building_id]
            .procedural_recipe()
            .unwrap();
        assert!(recipe
            .topology_sockets
            .iter()
            .any(|socket| socket.socket_id == created_id));
        assert_eq!(
            recipe
                .topology_edges
                .iter()
                .find(|edge| {
                    edge.kind == WorldTopologyEdgeKind::Stair
                        && edge.from == "entrance"
                        && edge.to == "lobby"
                })
                .unwrap()
                .from_socket
                .as_deref(),
            Some(created_id.as_str())
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sandbox_root_swap_is_atomic_bounded_and_reuses_assets() {
        let (mut world, root) = persistence_test_world("sandbox_atomic_swap");
        let road_id = world
            .resource_mut::<EditorProjectSession>()
            .project
            .create_content(persistence::ContentCategory::Road, "Speed Network")
            .unwrap();
        world.resource_mut::<EditorRegistryState>().selected = 1;
        apply_editor_action(&mut world, EditorAction::SeedDefaultTopology);
        apply_editor_action(&mut world, EditorAction::CompileSandboxRoot);
        let first_root = world
            .query_filtered::<Entity, With<WorldKitGeneratedRoot>>()
            .iter(&world)
            .next()
            .unwrap();
        let first_part_count = world
            .query_filtered::<Entity, With<WorldKitGeneratedPart>>()
            .iter(&world)
            .count();
        assert!((25..=192).contains(&first_part_count));
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 1);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 1);

        apply_editor_action(&mut world, EditorAction::RecipeParameterIncrease);
        sync_world_kit_sandbox(&mut world);
        let second_root = world
            .query_filtered::<Entity, With<WorldKitGeneratedRoot>>()
            .iter(&world)
            .next()
            .unwrap();
        assert_ne!(first_root, second_root);
        assert!(world.get_entity(first_root).is_err());
        assert_eq!(
            world
                .query_filtered::<Entity, With<WorldKitGeneratedPart>>()
                .iter(&world)
                .count(),
            first_part_count
        );
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 1);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 1);

        world
            .resource_mut::<EditorProjectSession>()
            .project
            .payloads
            .get_mut(&road_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap()
            .spline_points[1]
            .position[1] = 200.0;
        sync_world_kit_sandbox(&mut world);
        let retained_root = world
            .query_filtered::<Entity, With<WorldKitGeneratedRoot>>()
            .iter(&world)
            .next()
            .unwrap();
        assert_eq!(retained_root, second_root);

        apply_editor_action(&mut world, EditorAction::ClearSandboxRoot);
        assert_eq!(
            world
                .query_filtered::<Entity, With<WorldKitGeneratedRoot>>()
                .iter(&world)
                .count(),
            0
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<WorldKitGeneratedPart>>()
                .iter(&world)
                .count(),
            0
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
