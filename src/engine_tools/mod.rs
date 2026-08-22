//! Shared foundations for Starfall's in-game creation tools.
//!
//! Gameplay generators remain recipe-first. This module supplies editor
//! services that can be reused by Character Studio, Creature Forge, World Kit
//! Forge, and the future Level Scene Composer without serializing transient ECS
//! entities or depending on Bevy's runtime `Entity` values.

#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
pub mod character_records;
pub mod creature_records;
/// Dialogue graphs, cutscene/gameplay animation cues, and voice capture.
pub mod dialogue_records;
pub mod editable_mesh;
/// Shared button/row/label widgets for Forge authoring screens.
pub mod forge_widgets;
pub mod mesh_selection;
pub mod mesh_uv;
pub(crate) mod persistence;
/// Reusable structural and kinetic platforming prefab recipes.
pub mod platformer_prefabs;
mod presets;
pub mod project_registry;
/// Designer→Game publish step: bake validated records to assets/published/.
pub mod publish;
/// Spaceship Forge saves, routed through the versioned project contract.
pub mod spaceship_records;
/// Draggable, minimizable in-game tool windows shared by every editor.
pub mod tool_windows;
/// Vehicle Forge saves, routed through the versioned project contract.
pub mod vehicle_records;
/// Weapon Forge saves, routed through the versioned project contract.
pub mod weapon_records;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use bevy::dev_tools::infinite_grid::{InfiniteGrid, InfiniteGridSettings};
use bevy::ecs::system::SystemParam;
use bevy::feathers::controls::{FeathersTextInput, FeathersTextInputContainer};
use bevy::feathers::theme::UiTheme;
use bevy::gizmos::prelude::{
    TransformGizmoCamera, TransformGizmoFocus, TransformGizmoMode as BevyTransformGizmoMode,
    TransformGizmoSettings as BevyTransformGizmoSettings,
    TransformGizmoSpace as BevyTransformGizmoSpace, TransformGizmoState, TransformGizmoSystems,
};
use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::input::keyboard::Key;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::settings::SaveSettingsDeferred;
use bevy::text::{EditableText, EditableTextSystems};
use bevy::time::Virtual;
use bevy::ui::RelativeCursorPosition;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow, Window};
use serde::{Deserialize, Serialize};

use crate::components::player::{Player, PlayerCamera, PlayerInput};
use crate::components::world::{
    CollapsePlatform, CollapsePlatformState, DungeonCrawlGate, EnterableBuilding, FlipPlatform,
    MovingPlatform, RotatingElevator, SettlementBuildTerminal, SpeedLoopGuide, SpikePlatformHazard,
    SpringJumpPad, WalkableSurface, WaterBody, WaterBodyKind, WorldAnchor, WorldRouteMarker,
};
use crate::engine::bindings::FaceButton;
use crate::engine::machine_settings::{EditorPreferences, RenderQualitySettings};
use crate::engine::physics::prelude::{Physics, PhysicsTime};
use crate::engine::rendering::{
    all_gameplay_render_layers, EnergyMaterial, EnergyMaterialUniform, IceMaterial,
    IceMaterialUniform, LavaMaterial, LavaMaterialUniform, ShieldMaterial, ShieldMaterialUniform,
    ToonMaterial, ToonMaterialUniform, WaterMaterial, WaterMaterialUniform,
};
use crate::engine::state::AppState;
use crate::plugins::input_plugin::{
    mapped_gamepad_face, mapped_native_face, NativeButton, NativeControllerState,
};
use crate::resources::GameSettings;
use crate::spaceship_forge::PublishedSpacecraftCatalog;
use crate::vehicle_forge::PublishedVehicleCatalog;
use persistence::{
    validate_project, AdapterOverrideDraft, DraftPrimitive, EditorSceneDraft, ForgeProject,
    GenericRecipeDraft, LevelTemplate, ProceduralRecipeDraft, ProjectIoError, ProjectLoadSource,
    ProjectStore, RoadJunctionDraft, RoadJunctionKind, RoadSplinePointDraft, SceneObjectDraft,
    TransformDraft, WorldTopologyEdgeDraft, WorldTopologyEdgeKind, WorldTopologyNodeDraft,
    WorldTopologyNodeKind, WorldTopologySocketDraft, WorldTopologySocketKind,
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
struct ForgeMachinePreferences<'w> {
    gizmo: ResMut<'w, EditorGizmoSettings>,
    editor: ResMut<'w, EditorPreferences>,
    render: Res<'w, RenderQualitySettings>,
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
    map_points: Vec<PublishedMapPoint>,
}

/// A map annotation exported by a published World Kit recipe. Fast-travel
/// nodes are interactive in the in-game map; landmarks remain visible context.
/// Owned strings keep this interface safe for player-authored content.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedMapPoint {
    pub id: String,
    pub label: String,
    pub position: Vec3,
    pub kind: PublishedMapPointKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishedMapPointKind {
    FastTravel,
    Landmark,
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

    /// Seed the catalog from baked `assets/published/` content at startup.
    ///
    /// Only fills an empty catalog: when a Designer session opens a project,
    /// the workspace rebuild (which loads the *live* store, drafts included)
    /// remains the authority and must not be clobbered by stale baked files.
    pub fn seed_from_published(
        &mut self,
        creatures: impl IntoIterator<Item = crate::robots::creature::CreatureSpec>,
    ) -> usize {
        if !self.entries.is_empty() {
            return 0;
        }
        for spec in creatures {
            self.entries.insert(spec.content_id.clone(), spec);
        }
        self.entries.len()
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

    /// All published map annotations in stable content/node order.
    pub fn map_points(&self) -> impl Iterator<Item = &PublishedMapPoint> {
        self.entries
            .values()
            .flat_map(|entry| entry.map_points.iter())
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
            .init_resource::<EditorValidationCache>()
            .init_resource::<EditorOutlinerFilter>()
            .init_resource::<EditorCameraRig>()
            .init_resource::<EditorGizmoSettings>()
            .init_resource::<EditorPreferences>()
            .init_resource::<RenderQualitySettings>()
            .init_resource::<EditorDragState>()
            .init_resource::<BevyTransformGizmoAdapterDrag>()
            .init_resource::<WorldKitTopologyDragState>()
            .init_resource::<EditorPendingTransactions>()
            .init_resource::<EditorAdapterBaselines>()
            .init_resource::<EditorProjectSession>()
            .init_resource::<project_registry::ForgeProjectRegistry>()
            .init_resource::<PublishedMaterialCatalog>()
            .init_resource::<PublishedProceduralRecipeCatalog>()
            .init_resource::<PublishedCreatureCatalog>()
            .init_resource::<PublishedVehicleCatalog>()
            .init_resource::<PublishedSpacecraftCatalog>()
            .init_resource::<PublishedContentCatalogEpoch>()
            .init_resource::<EditorRegistryState>()
            .init_resource::<WorldKitPreviewState>()
            .init_resource::<WorldKitSandboxState>()
            .init_resource::<WorldKitSandboxAssets>()
            .init_resource::<EditorTextInputCapture>()
            .init_resource::<InputFocus>()
            .init_resource::<EditorPresetLibrary>()
            .init_resource::<EditorArmedModifier>()
            .init_resource::<EditorModifierParamCursor>()
            .init_resource::<EditorArmedLevelTemplate>()
            .init_resource::<EditorLevelDeleteArm>()
            .insert_non_send(DialogueVoiceCapture::default())
            .register_type::<EditorEntityId>()
            .configure_sets(
                PostUpdate,
                TransformGizmoSystems
                    .run_if(in_state(EngineToolMode::Editing))
                    .run_if(bevy_transform_gizmo_pointer_available),
            )
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
                    spawn_forge_editable_fields,
                    adapt_world_authorables,
                    apply_active_project_scene_adapters,
                )
                    .chain(),
            )
            .add_systems(
                OnExit(EngineToolMode::Editing),
                (refresh_platformer_runtime_adapters, exit_editor_workspace).chain(),
            )
            .add_systems(Update, resolve_procedural_material_bindings)
            .add_systems(Update, sync_machine_render_settings)
            .add_systems(
                Update,
                (
                    adapt_world_authorables,
                    editor_button_interactions,
                    editor_search_input,
                    editor_controller_navigation,
                    keep_editor_focus_in_view,
                    sync_bevy_transform_gizmo,
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
                    draw_semantic_editor_labels,
                    editor_camera_controls,
                )
                    .chain()
                    .after(tool_windows::ToolWindowSystemSet::PointerState)
                    .run_if(in_state(EngineToolMode::Editing)),
            )
            .add_systems(
                PostUpdate,
                (
                    record_bevy_transform_gizmo_transaction.after(TransformGizmoSystems),
                    sync_native_forge_fields.after(EditableTextSystems),
                )
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
    MissingLevel(String),
    ProjectMutation(String),
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
    RenameContent {
        before: String,
        after: String,
    },
    SetProjectDisplayName {
        before: String,
        after: String,
    },
    SetLevelDisplayName {
        level_id: String,
        before: String,
        after: String,
    },
    SetDialogueGraph {
        content_id: String,
        before: Box<dialogue_records::DialogueGraph>,
        after: Box<dialogue_records::DialogueGraph>,
    },
}

impl EditorCommand {
    fn target(&self) -> EditorEntityId {
        match self {
            Self::SetTransform { id, .. } => *id,
            Self::SetProceduralRecipe { .. }
            | Self::RenameContent { .. }
            | Self::SetProjectDisplayName { .. }
            | Self::SetLevelDisplayName { .. }
            | Self::SetDialogueGraph { .. } => EditorEntityId(0),
        }
    }

    fn preflight(&self, world: &mut World, undo: bool) -> Result<(), EditorCommandError> {
        match self {
            Self::SetProceduralRecipe { content_id, .. } => {
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
            Self::RenameContent { before, after } => {
                let (source, destination) = if undo {
                    (after.as_str(), before.as_str())
                } else {
                    (before.as_str(), after.as_str())
                };
                let mut candidate = world.resource::<EditorProjectSession>().project.clone();
                return candidate
                    .rename_content(source, destination)
                    .map_err(|error| EditorCommandError::ProjectMutation(format!("{error:?}")));
            }
            Self::SetLevelDisplayName { level_id, .. } => {
                let exists = world
                    .resource::<EditorProjectSession>()
                    .project
                    .levels
                    .iter()
                    .any(|level| level.level_id == *level_id);
                return exists
                    .then_some(())
                    .ok_or_else(|| EditorCommandError::MissingLevel(level_id.clone()));
            }
            Self::SetProjectDisplayName { .. } => return Ok(()),
            Self::SetDialogueGraph {
                content_id,
                before,
                after,
            } => {
                let graph = if undo { before } else { after };
                let mut candidate = world.resource::<EditorProjectSession>().project.clone();
                return dialogue_records::save_dialogue(&mut candidate, content_id, graph)
                    .map_err(|error| EditorCommandError::ProjectMutation(format!("{error:?}")));
            }
            Self::SetTransform { .. } => {}
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
            Self::RenameContent { before, after } => rename_project_content(world, before, after),
            Self::SetProjectDisplayName { after, .. } => {
                set_project_display_name(world, after.clone())
            }
            Self::SetLevelDisplayName {
                level_id, after, ..
            } => set_level_display_name(world, level_id, after.clone()),
            Self::SetDialogueGraph {
                content_id, after, ..
            } => set_dialogue_graph(world, content_id, (**after).clone()),
        }
    }

    fn undo(&self, world: &mut World) -> Result<(), EditorCommandError> {
        match self {
            Self::SetTransform { id, before, .. } => set_transform(world, *id, *before),
            Self::SetProceduralRecipe {
                content_id, before, ..
            } => set_procedural_recipe(world, content_id, (**before).clone()),
            Self::RenameContent { before, after } => rename_project_content(world, after, before),
            Self::SetProjectDisplayName { before, .. } => {
                set_project_display_name(world, before.clone())
            }
            Self::SetLevelDisplayName {
                level_id, before, ..
            } => set_level_display_name(world, level_id, before.clone()),
            Self::SetDialogueGraph {
                content_id, before, ..
            } => set_dialogue_graph(world, content_id, (**before).clone()),
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
            command.preflight(world, false)?;
        }
        for command in &self.commands {
            command.execute(world)?;
        }
        Ok(())
    }

    fn undo(&self, world: &mut World) -> Result<(), EditorCommandError> {
        for command in &self.commands {
            command.preflight(world, true)?;
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
        // A command can become stale when its document is replaced (for
        // example after a level switch). Since it has already been popped, `?`
        // drops it instead of leaving it above every older valid transaction.
        transaction.undo(world)?;
        self.redo.push(transaction);
        Ok(true)
    }

    pub fn redo(&mut self, world: &mut World) -> Result<bool, EditorCommandError> {
        let Some(transaction) = self.redo.pop() else {
            return Ok(false);
        };
        // Failed redo entries are stale for the current document and must not
        // permanently block the remainder of the redo history.
        transaction.execute(world)?;
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

fn rename_project_content(
    world: &mut World,
    before: &str,
    after: &str,
) -> Result<(), EditorCommandError> {
    world
        .resource_mut::<EditorProjectSession>()
        .project
        .rename_content(before, after)
        .map_err(|error| EditorCommandError::ProjectMutation(format!("{error:?}")))
}

fn set_project_display_name(world: &mut World, value: String) -> Result<(), EditorCommandError> {
    world
        .resource_mut::<EditorProjectSession>()
        .project
        .display_name = value;
    Ok(())
}

fn set_level_display_name(
    world: &mut World,
    level_id: &str,
    value: String,
) -> Result<(), EditorCommandError> {
    let mut session = world.resource_mut::<EditorProjectSession>();
    let level = session
        .project
        .levels
        .iter_mut()
        .find(|level| level.level_id == level_id)
        .ok_or_else(|| EditorCommandError::MissingLevel(level_id.into()))?;
    level.display_name = value;
    Ok(())
}

fn set_dialogue_graph(
    world: &mut World,
    content_id: &str,
    value: dialogue_records::DialogueGraph,
) -> Result<(), EditorCommandError> {
    dialogue_records::save_dialogue(
        &mut world.resource_mut::<EditorProjectSession>().project,
        content_id,
        &value,
    )
    .map_err(|error| EditorCommandError::ProjectMutation(format!("{error:?}")))
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
    Ladder,
    Stairs,
    Tower,
    Castle,
    FloatingIsland,
    MovingPlatform,
    SpringPlatform,
    FlipPanel,
    RotatingBridge,
    CollapseBridge,
    SpikeBridge,
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
struct EditorInfiniteGrid;

#[derive(Component)]
struct EditorSummaryText;

#[derive(Component)]
struct EditorStatusText;

/// Fixed workspace chrome that owns pointer input just like a floating tool
/// window. Geometry comes from Bevy UI layout, never hard-coded screen bands.
#[derive(Component)]
struct EditorChromeSurface;

#[derive(Component)]
struct EditorSearchText;

#[derive(Component, Default, Clone, Copy, Debug, PartialEq, Eq)]
enum ForgeTextField {
    #[default]
    OutlinerFilter,
    RegistrySearch,
    ContentId,
    ProjectName,
    LevelName,
}

#[derive(Component, Default, Clone)]
struct ForgeTextFieldMarker {
    kind: ForgeTextField,
}

#[derive(Component)]
struct EditorRegistryText;

#[derive(Component)]
struct EditorSettingsText;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EditorPanelKind {
    Outliner,
    Inspector,
    Registry,
}

/// Declare each focusable control once. The variant is its stable identity,
/// declaration order is controller traversal order, and `panel` is used for
/// focus reveal. Keeping those three facts in one registry prevents the old
/// numeric-order collisions and gaps from silently routing activation to a
/// different button.
macro_rules! define_editor_controls {
    ($( $action:ident => $panel:expr ),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        enum EditorAction {
            $( $action ),+
        }

        const EDITOR_CONTROLS: &[EditorControlSpec] = &[
            $(EditorControlSpec {
                id: EditorAction::$action,
                panel: $panel,
            }),+
        ];
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EditorControlSpec {
    id: EditorAction,
    panel: Option<EditorPanelKind>,
}

define_editor_controls! {
    // Workspace toolbar, in visual order.
    Exit => None,
    SaveProject => None,
    LoadProject => None,
    RecoverProject => None,
    PublishProject => None,
    LevelNext => None,
    LevelCycleTemplate => None,
    LevelNew => None,
    LevelMoveEarlier => None,
    LevelMoveLater => None,
    LevelDelete => None,
    LevelSetStartup => None,
    PlaytestStartup => None,
    Undo => None,
    Redo => None,
    FrameSelection => None,
    ToggleCameraMode => None,
    GizmoTranslate => None,
    GizmoRotate => None,
    GizmoScale => None,
    ToggleTransformSpace => None,
    CycleSnap => None,
    ToggleSemanticLabels => None,

    // Outliner.
    FocusSearch => Some(EditorPanelKind::Outliner),
    SelectPrevious => Some(EditorPanelKind::Outliner),
    SelectNext => Some(EditorPanelKind::Outliner),
    CreateAnchor => Some(EditorPanelKind::Outliner),
    CreateCube => Some(EditorPanelKind::Outliner),
    CreatePillar => Some(EditorPanelKind::Outliner),
    CreateBeacon => Some(EditorPanelKind::Outliner),
    CreateLadder => Some(EditorPanelKind::Outliner),
    CreateStairs => Some(EditorPanelKind::Outliner),
    CreateTower => Some(EditorPanelKind::Outliner),
    CreateCastle => Some(EditorPanelKind::Outliner),
    CreateFloatingIsland => Some(EditorPanelKind::Outliner),
    CreateMovingPlatform => Some(EditorPanelKind::Outliner),
    CreateSpringPlatform => Some(EditorPanelKind::Outliner),
    CreateFlipPanel => Some(EditorPanelKind::Outliner),
    CreateRotatingBridge => Some(EditorPanelKind::Outliner),
    CreateCollapseBridge => Some(EditorPanelKind::Outliner),
    CreateSpikeBridge => Some(EditorPanelKind::Outliner),

    // Inspector.
    Duplicate => Some(EditorPanelKind::Inspector),
    Delete => Some(EditorPanelKind::Inspector),
    ModifierCycleKind => Some(EditorPanelKind::Inspector),
    ModifierAdd => Some(EditorPanelKind::Inspector),
    ModifierRemoveLast => Some(EditorPanelKind::Inspector),
    ModifierCycleParam => Some(EditorPanelKind::Inspector),
    ModifierValueDown => Some(EditorPanelKind::Inspector),
    ModifierValueUp => Some(EditorPanelKind::Inspector),
    MoveXNegative => Some(EditorPanelKind::Inspector),
    MoveXPositive => Some(EditorPanelKind::Inspector),
    MoveYNegative => Some(EditorPanelKind::Inspector),
    MoveYPositive => Some(EditorPanelKind::Inspector),
    MoveZNegative => Some(EditorPanelKind::Inspector),
    MoveZPositive => Some(EditorPanelKind::Inspector),
    RotateYNegative => Some(EditorPanelKind::Inspector),
    RotateYPositive => Some(EditorPanelKind::Inspector),
    ScaleDown => Some(EditorPanelKind::Inspector),
    ScaleUp => Some(EditorPanelKind::Inspector),
    ToggleGrid => Some(EditorPanelKind::Inspector),
    LabelsDistanceDecrease => Some(EditorPanelKind::Inspector),
    LabelsDistanceIncrease => Some(EditorPanelKind::Inspector),
    LabelsBudgetDecrease => Some(EditorPanelKind::Inspector),
    LabelsBudgetIncrease => Some(EditorPanelKind::Inspector),
    ToggleShadowMaps => Some(EditorPanelKind::Inspector),
    CycleShadowMapSize => Some(EditorPanelKind::Inspector),
    ToggleForgeContactShadows => Some(EditorPanelKind::Inspector),

    // Content Registry / World Kit.
    RegistryPrevious => Some(EditorPanelKind::Registry),
    RegistryNext => Some(EditorPanelKind::Registry),
    RegistryRename => Some(EditorPanelKind::Registry),
    EditProjectName => Some(EditorPanelKind::Registry),
    EditLevelName => Some(EditorPanelKind::Registry),
    CreateDialogueGraph => Some(EditorPanelKind::Registry),
    DialogueCycleMode => Some(EditorPanelKind::Registry),
    DialogueAddNode => Some(EditorPanelKind::Registry),
    DialogueAddAnimationCue => Some(EditorPanelKind::Registry),
    DialogueRecordVoice => Some(EditorPanelKind::Registry),
    ValidateProject => Some(EditorPanelKind::Registry),
    CreateMaterialRecord => Some(EditorPanelKind::Registry),
    DuplicateRecord => Some(EditorPanelKind::Registry),
    DeleteRecord => Some(EditorPanelKind::Registry),
    NormalizeSourcePath => Some(EditorPanelKind::Registry),
    SearchRegistry => Some(EditorPanelKind::Registry),
    PresetCapture => Some(EditorPanelKind::Registry),
    PresetNext => Some(EditorPanelKind::Registry),
    PresetApply => Some(EditorPanelKind::Registry),
    PresetCompare => Some(EditorPanelKind::Registry),
    PresetFork => Some(EditorPanelKind::Registry),
    PresetNewSeedVariant => Some(EditorPanelKind::Registry),
    MaterialCycleFamily => Some(EditorPanelKind::Registry),
    MaterialCyclePreset => Some(EditorPanelKind::Registry),
    MaterialNextParameter => Some(EditorPanelKind::Registry),
    MaterialDecrease => Some(EditorPanelKind::Registry),
    MaterialIncrease => Some(EditorPanelKind::Registry),
    MaterialReset => Some(EditorPanelKind::Registry),
    ApplySelectedMaterial => Some(EditorPanelKind::Registry),
    ClearObjectMaterial => Some(EditorPanelKind::Registry),
    CycleRecipeKind => Some(EditorPanelKind::Registry),
    CreateRecipeRecord => Some(EditorPanelKind::Registry),
    CopySelectedMaterial => Some(EditorPanelKind::Registry),
    RecipeNextSlot => Some(EditorPanelKind::Registry),
    BindRecipeMaterial => Some(EditorPanelKind::Registry),
    ClearRecipeMaterial => Some(EditorPanelKind::Registry),
    RegenerateRecipePreview => Some(EditorPanelKind::Registry),
    RecipeNextParameter => Some(EditorPanelKind::Registry),
    RecipeParameterDecrease => Some(EditorPanelKind::Registry),
    RecipeParameterIncrease => Some(EditorPanelKind::Registry),
    RecipeResetParameters => Some(EditorPanelKind::Registry),
    RecipeSeedDecrease => Some(EditorPanelKind::Registry),
    RecipeSeedIncrease => Some(EditorPanelKind::Registry),
    ValidateSelectedRecipe => Some(EditorPanelKind::Registry),
    SeedDefaultTopology => Some(EditorPanelKind::Registry),
    TopologyPrevious => Some(EditorPanelKind::Registry),
    TopologyNext => Some(EditorPanelKind::Registry),
    TopologyMoveXNegative => Some(EditorPanelKind::Registry),
    TopologyMoveXPositive => Some(EditorPanelKind::Registry),
    TopologyMoveYNegative => Some(EditorPanelKind::Registry),
    TopologyMoveYPositive => Some(EditorPanelKind::Registry),
    TopologyMoveZNegative => Some(EditorPanelKind::Registry),
    TopologyMoveZPositive => Some(EditorPanelKind::Registry),
    TopologyMarkEdgeStart => Some(EditorPanelKind::Registry),
    TopologyCycleEdgeKind => Some(EditorPanelKind::Registry),
    TopologyConnectEdge => Some(EditorPanelKind::Registry),
    TopologyRemoveEdge => Some(EditorPanelKind::Registry),
    RoadTangentNextAxis => Some(EditorPanelKind::Registry),
    RoadTangentDecrease => Some(EditorPanelKind::Registry),
    RoadTangentIncrease => Some(EditorPanelKind::Registry),
    RoadTangentReset => Some(EditorPanelKind::Registry),
    RoadTangentAutoRoundAll => Some(EditorPanelKind::Registry),
    RoadMarkJunctionStart => Some(EditorPanelKind::Registry),
    RoadCycleJunctionKind => Some(EditorPanelKind::Registry),
    RoadConnectJunction => Some(EditorPanelKind::Registry),
    RoadRemoveJunction => Some(EditorPanelKind::Registry),
    SocketNext => Some(EditorPanelKind::Registry),
    SocketCycleKind => Some(EditorPanelKind::Registry),
    SocketCreate => Some(EditorPanelKind::Registry),
    SocketDelete => Some(EditorPanelKind::Registry),
    SocketMoveXNegative => Some(EditorPanelKind::Registry),
    SocketMoveXPositive => Some(EditorPanelKind::Registry),
    SocketMoveYNegative => Some(EditorPanelKind::Registry),
    SocketMoveYPositive => Some(EditorPanelKind::Registry),
    SocketMoveZNegative => Some(EditorPanelKind::Registry),
    SocketMoveZPositive => Some(EditorPanelKind::Registry),
    TerrainOriginXNegative => Some(EditorPanelKind::Registry),
    TerrainOriginXPositive => Some(EditorPanelKind::Registry),
    TerrainOriginZNegative => Some(EditorPanelKind::Registry),
    TerrainOriginZPositive => Some(EditorPanelKind::Registry),
    TerrainClearanceDecrease => Some(EditorPanelKind::Registry),
    TerrainClearanceIncrease => Some(EditorPanelKind::Registry),
    TerrainProjectSelected => Some(EditorPanelKind::Registry),
    TerrainProjectAll => Some(EditorPanelKind::Registry),
    CompileSandboxRoot => Some(EditorPanelKind::Registry),
    ClearSandboxRoot => Some(EditorPanelKind::Registry),
}

#[derive(Component, Debug, Clone, Copy)]
struct EditorButton {
    control: EditorAction,
    panel: Option<EditorPanelKind>,
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

#[derive(Resource)]
struct EditorFocus(EditorAction);

impl Default for EditorFocus {
    fn default() -> Self {
        Self(EditorAction::SelectNext)
    }
}

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

/// Cached project/source diagnostics. Record source files can be numerous, so
/// they are read after document/session changes or an explicit validation—not
/// once per rendered editor frame.
#[derive(Resource, Default)]
struct EditorValidationCache {
    initialized: bool,
    issues: Vec<String>,
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
struct BevyTransformGizmoAdapterDrag(Option<(EditorEntityId, Transform)>);

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

/// Generated world adapters are not scene primitives. Preserve their initial
/// transforms once, then restore that baseline before applying each level's
/// sparse overrides so transforms never leak between levels.
#[derive(Resource, Default)]
struct EditorAdapterBaselines(BTreeMap<String, (Entity, Transform)>);

#[derive(Resource, Debug)]
struct EditorProjectSession {
    project: ForgeProject,
    store: ProjectStore,
    /// `project` contains a document that actually came from `store`, rather
    /// than the factory default installed while resources are initialized.
    /// Saves are forbidden until this is true.
    loaded: bool,
    /// The user explicitly selected Recover and may promote that snapshot over
    /// the exact primary state observed at recovery time (including a missing
    /// primary). Automatic recovery never grants this permission.
    recovery_write_authorized: bool,
    /// Exact primary-manifest fingerprint observed when this document was
    /// loaded or last saved. A mismatched disk revision means another Forge
    /// screen/process wrote the project and this session must not overwrite it.
    /// Automatic recovery clears this; explicit recovery records the exact
    /// primary state it was authorized to replace.
    primary_revision: Option<String>,
}

#[derive(Resource, Default)]
struct EditorRegistryState {
    selected: usize,
    rename_active: bool,
    rename_buffer: String,
    search_active: bool,
    search_text: String,
    project_name_active: bool,
    project_name_buffer: String,
    level_name_active: bool,
    level_name_buffer: String,
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

#[derive(Default)]
struct DialogueVoiceCapture {
    recorder: Option<dialogue_records::VoiceRecorder>,
    content_id: String,
    node_id: String,
    take: u16,
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
struct EditorModifierStack(Vec<crate::character::mesh_modifiers::MeshModifier>);

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
fn armed_modifier_template(index: usize) -> crate::character::mesh_modifiers::MeshModifier {
    crate::character::mesh_modifiers::MeshModifier::template(index)
}

impl Default for EditorProjectSession {
    fn default() -> Self {
        let store = ProjectStore::default_workspace();
        Self {
            project: ForgeProject::default(),
            store,
            loaded: false,
            recovery_write_authorized: false,
            primary_revision: None,
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

    // The live editor belongs to the Designer edition; consumers never enter
    // Editing, so the chord must be inert rather than opening dev tooling.
    if !cfg!(feature = "designer") {
        return;
    }
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
    mut machine_preferences: ForgeMachinePreferences,
    mut registry: ResMut<EditorRegistryState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut material_assets: ForgeMaterialAssets,
    bevy_gizmo: Option<Res<BevyTransformGizmoSettings>>,
) {
    virtual_time.pause();
    physics_time.pause();
    focus.0 = EditorAction::SelectNext;
    document.exit_armed = false;
    filter.active = false;
    registry.rename_active = false;
    registry.search_active = false;
    registry.project_name_active = false;
    registry.level_name_active = false;
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
    machine_preferences.editor.sanitize();
    machine_preferences.gizmo.snap_index =
        usize::from(machine_preferences.editor.translation_snap_index);

    let mut editor_camera = commands.spawn((
        EditorCamera,
        TransformGizmoCamera,
        Camera3d::default(),
        Camera {
            // Bevy's transform-gizmo overlay camera renders at order 1.
            // Player cameras are inactive in Forge, so order 0 keeps the
            // editor view below that overlay without sacrificing UI.
            order: 0,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            far: 30_000.0,
            ..default()
        }),
        all_gameplay_render_layers(),
        camera_transform,
    ));
    if machine_preferences.render.forge_contact_shadows {
        editor_camera.insert((
            bevy::camera::Hdr,
            Msaa::Off,
            bevy::pbr::ContactShadows::default(),
        ));
    }

    if bevy_gizmo.is_some() {
        commands.spawn((
            EditorWorkspaceRoot,
            EditorInfiniteGrid,
            Name::new("Forge Infinite Grid"),
            InfiniteGrid,
            InfiniteGridSettings {
                x_axis_color: Color::srgb(1.0, 0.18, 0.58),
                z_axis_color: Color::srgb(0.10, 0.68, 1.0),
                minor_line_color: Color::srgba(0.12, 0.30, 0.42, 0.44),
                major_line_color: Color::srgba(0.22, 0.68, 0.78, 0.62),
                fadeout_distance: machine_preferences.editor.infinite_grid_fade_distance,
                dot_fadeout_strength: 0.34,
                scale: 1.0,
            },
            if machine_preferences.editor.infinite_grid_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
    }

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
            let origin = recipe.terrain_projection.world_origin;
            let mut map_points = recipe
                .topology_nodes
                .iter()
                .filter_map(|node| {
                    let kind = match node.kind {
                        WorldTopologyNodeKind::FastTravel => PublishedMapPointKind::FastTravel,
                        WorldTopologyNodeKind::Landmark => PublishedMapPointKind::Landmark,
                        _ => return None,
                    };
                    let readable_node = node.node_id.replace(['_', '-'], " ");
                    Some(PublishedMapPoint {
                        id: format!("{}:{}", record.content_id, node.node_id),
                        label: format!("{} — {}", record.display_name, readable_node),
                        position: Vec3::new(
                            node.position[0] + origin[0],
                            node.position[1],
                            node.position[2] + origin[1],
                        ),
                        kind,
                    })
                })
                .collect::<Vec<_>>();
            map_points.sort_by(|left, right| left.id.cmp(&right.id));
            Some((
                record.content_id.clone(),
                PublishedProceduralRecipeEntry {
                    source_hash: record.draft_hash.clone(),
                    kind,
                    seed: recipe.seed,
                    revision: recipe.revision,
                    material_slots: recipe.material_slots.clone(),
                    map_points,
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

    let vehicles = project
        .records
        .iter()
        .filter(|record| {
            record.category == persistence::ContentCategory::Vehicle
                && record.published_hash.as_ref() == Some(&record.draft_hash)
                && record.published_hash.is_some()
        })
        .filter_map(|record| vehicle_records::load_vehicle(project, &record.content_id).ok())
        .collect::<Vec<_>>();
    world
        .resource_mut::<PublishedVehicleCatalog>()
        .replace(vehicles);

    let spacecraft = project
        .records
        .iter()
        .filter(|record| {
            record.category == persistence::ContentCategory::Spaceship
                && record.published_hash.as_ref() == Some(&record.draft_hash)
                && record.published_hash.is_some()
        })
        .filter_map(|record| spaceship_records::load_spaceship(project, &record.content_id).ok())
        .collect::<Vec<_>>();
    world
        .resource_mut::<PublishedSpacecraftCatalog>()
        .replace(spacecraft);

    let mut epoch = world.resource_mut::<PublishedContentCatalogEpoch>();
    epoch.0 = epoch.0.wrapping_add(1).max(1);
}

/// PM1: point the editor session at the registry's active project before the
/// workspace opens. Fresh projects are persisted by the Project Hub before
/// they become active; a missing or damaged store must never silently turn the
/// resource's factory document into a writable project. Switching projects
/// also rebuilds the published-content catalogs from the new file.
fn sync_active_project_session(world: &mut World) {
    let active = world
        .resource::<project_registry::ForgeProjectRegistry>()
        .active
        .clone();
    let Some(active) = active else {
        let mut session = world.resource_mut::<EditorProjectSession>();
        session.loaded = false;
        session.recovery_write_authorized = false;
        session.primary_revision = None;
        set_editor_status(
            world,
            "No active project — open one in the Project Hub first",
        );
        return;
    };
    let store = ProjectStore::new(&active, 3);
    let same_path = world.resource::<EditorProjectSession>().store.path() == active.as_path();
    let current_revision = store.primary_revision().ok();
    let session = world.resource::<EditorProjectSession>();
    let observed_revision = session.primary_revision.clone();
    let loaded = session.loaded;
    if same_path && loaded && current_revision == observed_revision {
        return;
    }
    if same_path && world.resource::<EditorDocumentState>().dirty {
        set_editor_status(
            world,
            "Project changed outside this editor while local edits are unsaved; reload or save to a new project before continuing",
        );
        return;
    }
    let (project, source, coherent_revision) = match store.load_with_recovery_and_revision() {
        Ok(loaded) => loaded,
        Err(error) => {
            // Never replace a damaged project with a fresh in-memory document
            // at the same path: a later Save would destroy recovery evidence.
            warn!(
                "Active project {} could not be loaded: {error:?}",
                active.display()
            );
            set_editor_status(
                world,
                format!(
                    "Project load failed for {} — primary and recovery snapshots were left untouched",
                    active.display()
                ),
            );
            let mut session = world.resource_mut::<EditorProjectSession>();
            session.store = store;
            session.loaded = false;
            session.recovery_write_authorized = false;
            session.primary_revision = None;
            world
                .resource_mut::<NextState<EngineToolMode>>()
                .set(EngineToolMode::Playing);
            return;
        }
    };
    let scene = project.scene.clone();
    rebuild_published_content_catalogs(world, &project);
    reload_preset_library(world, &active);
    world.resource_mut::<EditorSelection>().clear();
    world.resource_mut::<EditorUndoStack>().clear();
    world.resource_mut::<EditorDocumentState>().dirty = false;
    respawn_scene_objects(world, &scene.objects);
    let mut session = world.resource_mut::<EditorProjectSession>();
    session.project = project;
    session.store = store;
    session.loaded = true;
    session.recovery_write_authorized = false;
    session.primary_revision = match source {
        ProjectLoadSource::Primary => coherent_revision,
        ProjectLoadSource::Recovery(_) => None,
    };
    match source {
        ProjectLoadSource::Recovery(index) => {
            world.resource_mut::<EditorRuntimeStatus>().message = format!(
                "Loaded recovery snapshot {index} read-only; use RECOVER to authorize replacing the primary project"
            );
        }
        ProjectLoadSource::Primary if same_path => {
            world.resource_mut::<EditorRuntimeStatus>().message =
                "Reloaded primary project after an external project update".into();
        }
        ProjectLoadSource::Primary => {}
    }
}

/// Finish project activation only after `adapt_world_authorables` has flushed
/// its component inserts. This keeps scene ids reserved before adapter ids are
/// allocated, while ensuring the initial adapter baseline is captured before
/// the loaded override is applied.
fn apply_active_project_scene_adapters(world: &mut World) {
    let scene = {
        let session = world.resource::<EditorProjectSession>();
        if !session.loaded {
            return;
        }
        session.project.scene.clone()
    };
    apply_scene_adapter_overrides(world, &scene);
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
    let (project, source, revision) = match store.load_with_recovery_and_revision() {
        Ok(loaded) => loaded,
        Err(_) => {
            let mut session = world.resource_mut::<EditorProjectSession>();
            session.loaded = false;
            session.recovery_write_authorized = false;
            session.primary_revision = None;
            return;
        }
    };
    rebuild_published_content_catalogs(world, &project);
    reload_preset_library(world, store.path());
    let mut session = world.resource_mut::<EditorProjectSession>();
    session.project = project;
    session.loaded = true;
    session.recovery_write_authorized = false;
    session.primary_revision = match source {
        ProjectLoadSource::Primary => revision,
        ProjectLoadSource::Recovery(_) => None,
    };
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
    if let Some(kind) = editor_platformer_kind(primitive) {
        return kind.accent_color();
    }
    match primitive {
        EditorPrimitive::Empty => Color::WHITE,
        EditorPrimitive::Cube => Color::srgb(0.15, 0.68, 0.94),
        EditorPrimitive::Pillar => Color::srgb(0.92, 0.52, 0.14),
        EditorPrimitive::Beacon => Color::srgb(0.35, 1.0, 0.58),
        _ => unreachable!("platformer prefabs returned above"),
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
                crate::engine::physics::prelude::RigidBody::Fixed,
                crate::engine::physics::prelude::Collider::cuboid(0.5, 0.5, 0.5),
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
    focused: Query<Entity, With<TransformGizmoFocus>>,
    mut player_cameras: Query<&mut Camera, With<PlayerCamera>>,
    mut players: Query<&mut PlayerInput, With<Player>>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut physics_time: ResMut<Time<Physics>>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut topology_drag: ResMut<WorldKitTopologyDragState>,
    mut bevy_drag: ResMut<BevyTransformGizmoAdapterDrag>,
    mut session: ResMut<EditorProjectSession>,
    mut directional_lights: Query<&mut DirectionalLight>,
    mut input_focus: ResMut<InputFocus>,
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
    for entity in &focused {
        commands.entity(entity).remove::<TransformGizmoFocus>();
    }
    bevy_drag.0 = None;
    input_focus.clear();
    for mut light in &mut directional_lights {
        light.contact_shadows_enabled = false;
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
fn refresh_platformer_runtime_adapters(
    mut prefabs: Query<(
        &EditorPrimitive,
        &Transform,
        Option<&mut MovingPlatform>,
        Option<&mut RotatingElevator>,
        Option<&mut SpringJumpPad>,
        Option<&mut FlipPlatform>,
        Option<&mut CollapsePlatform>,
        Option<&mut SpikePlatformHazard>,
    )>,
) {
    for (primitive, transform, moving, rotating, spring, flip, collapse, spikes) in &mut prefabs {
        let Some(kind) = editor_platformer_kind(*primitive) else {
            continue;
        };
        let design = platformer_prefabs::PlatformerPrefabDesign::stock(kind);
        let size = design.size * transform.scale.abs();
        match kind.behavior() {
            platformer_prefabs::PlatformerBehavior::Oscillate { distance, seconds } => {
                if let Some(mut moving) = moving {
                    let half_travel = transform.rotation * Vec3::X * (distance * 0.5);
                    moving.start = transform.translation - half_travel;
                    moving.end = transform.translation + half_travel;
                    moving.speed = std::f32::consts::TAU * distance / seconds.max(0.1);
                    moving.size = size;
                }
            }
            platformer_prefabs::PlatformerBehavior::Rotate { degrees_per_second } => {
                if let Some(mut rotating) = rotating {
                    rotating.center = transform.translation;
                    rotating.angular_speed = degrees_per_second.to_radians();
                    rotating.size = size;
                }
            }
            platformer_prefabs::PlatformerBehavior::Bounce { launch_speed } => {
                if let Some(mut spring) = spring {
                    spring.launch_velocity = Vec3::Y * launch_speed;
                    spring.radius = size.x.min(size.z) * 0.48;
                    spring.cooldown_timers = [0.0; 4];
                }
            }
            platformer_prefabs::PlatformerBehavior::FlipOnJump => {
                if let Some(mut flip) = flip {
                    flip.base_rotation = transform.rotation;
                    flip.flipped = false;
                    flip.progress = 0.0;
                    flip.cooldown_timer = 0.0;
                }
            }
            platformer_prefabs::PlatformerBehavior::Collapse { warning_seconds } => {
                if let Some(mut collapse) = collapse {
                    collapse.origin = transform.translation;
                    collapse.size = size;
                    collapse.warning_seconds = warning_seconds;
                    collapse.timer = 0.0;
                    collapse.state = CollapsePlatformState::Armed;
                }
            }
            platformer_prefabs::PlatformerBehavior::Hazard => {
                if let Some(mut spikes) = spikes {
                    spikes.size = size;
                    spikes.cooldown_timers = [0.0; 4];
                }
            }
            platformer_prefabs::PlatformerBehavior::StaticTraversal
            | platformer_prefabs::PlatformerBehavior::Climbable => {}
        }
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
                EditorChromeSurface,
                RelativeCursorPosition::default(),
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
                spawn_editor_button(bar, EditorAction::Exit, "EXIT  [Tab]");
                spawn_editor_button(bar, EditorAction::SaveProject, "SAVE  [⌘S]");
                spawn_editor_button(bar, EditorAction::LoadProject, "LOAD");
                spawn_editor_button(bar, EditorAction::RecoverProject, "RECOVER");
                spawn_editor_button(bar, EditorAction::PublishProject, "PUBLISH");
                spawn_editor_button(bar, EditorAction::LevelNext, "NEXT LEVEL");
                spawn_editor_button(bar, EditorAction::LevelCycleTemplate, "LEVEL TEMPLATE");
                spawn_editor_button(bar, EditorAction::LevelNew, "NEW LEVEL");
                spawn_editor_button(bar, EditorAction::LevelMoveEarlier, "LEVEL ◀");
                spawn_editor_button(bar, EditorAction::LevelMoveLater, "LEVEL ▶");
                spawn_editor_button(bar, EditorAction::LevelDelete, "DELETE LEVEL");
                spawn_editor_button(bar, EditorAction::LevelSetStartup, "SET STARTUP");
                spawn_editor_button(bar, EditorAction::PlaytestStartup, "PLAYTEST");
                spawn_editor_button(bar, EditorAction::Undo, "UNDO");
                spawn_editor_button(bar, EditorAction::Redo, "REDO");
                spawn_editor_button(bar, EditorAction::FrameSelection, "FRAME  [F]");
                spawn_editor_button(bar, EditorAction::ToggleCameraMode, "FLY / ORBIT");
                spawn_editor_button(bar, EditorAction::GizmoTranslate, "MOVE [G]");
                spawn_editor_button(bar, EditorAction::GizmoRotate, "ROTATE [R]");
                spawn_editor_button(bar, EditorAction::GizmoScale, "SCALE [S]");
                spawn_editor_button(bar, EditorAction::ToggleTransformSpace, "WORLD/LOCAL");
                spawn_editor_button(bar, EditorAction::CycleSnap, "SNAP");
                spawn_editor_button(bar, EditorAction::ToggleSemanticLabels, "LABELS [V]");
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
                    spawn_editor_button(panel, EditorAction::FocusSearch, "SEARCH / FILTER");
                    spawn_editor_button(panel, EditorAction::SelectPrevious, "◀ PREVIOUS");
                    spawn_editor_button(panel, EditorAction::SelectNext, "NEXT ▶");
                    spawn_editor_button(panel, EditorAction::CreateAnchor, "+ EMPTY ANCHOR");
                    spawn_editor_button(panel, EditorAction::CreateCube, "+ BLOCK");
                    spawn_editor_button(panel, EditorAction::CreatePillar, "+ PILLAR");
                    spawn_editor_button(panel, EditorAction::CreateBeacon, "+ BEACON");
                    spawn_editor_button(panel, EditorAction::CreateLadder, "+ LADDER");
                    spawn_editor_button(panel, EditorAction::CreateStairs, "+ STAIRS");
                    spawn_editor_button(panel, EditorAction::CreateTower, "+ STAR TOWER");
                    spawn_editor_button(panel, EditorAction::CreateCastle, "+ STAR CASTLE");
                    spawn_editor_button(
                        panel,
                        EditorAction::CreateFloatingIsland,
                        "+ FLOATING ISLAND",
                    );
                    spawn_editor_button(
                        panel,
                        EditorAction::CreateMovingPlatform,
                        "+ MOVING PLATFORM",
                    );
                    spawn_editor_button(
                        panel,
                        EditorAction::CreateSpringPlatform,
                        "+ PULSE SPRING",
                    );
                    spawn_editor_button(panel, EditorAction::CreateFlipPanel, "+ FLIP PANEL");
                    spawn_editor_button(
                        panel,
                        EditorAction::CreateRotatingBridge,
                        "+ ROTATING BRIDGE",
                    );
                    spawn_editor_button(
                        panel,
                        EditorAction::CreateCollapseBridge,
                        "+ COLLAPSE BRIDGE",
                    );
                    spawn_editor_button(panel, EditorAction::CreateSpikeBridge, "+ SPIKE BRIDGE");
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
                    spawn_editor_button(panel, EditorAction::Duplicate, "DUPLICATE");
                    spawn_editor_button(panel, EditorAction::Delete, "DELETE");
                    spawn_editor_button(panel, EditorAction::ModifierCycleKind, "MODIFIER KIND");
                    spawn_editor_button(panel, EditorAction::ModifierAdd, "ADD MODIFIER");
                    spawn_editor_button(panel, EditorAction::ModifierRemoveLast, "REMOVE MODIFIER");
                    spawn_editor_button(panel, EditorAction::ModifierCycleParam, "MOD PARAM");
                    spawn_editor_button(panel, EditorAction::ModifierValueDown, "MOD VALUE −");
                    spawn_editor_button(panel, EditorAction::ModifierValueUp, "MOD VALUE +");
                    spawn_editor_button(panel, EditorAction::MoveXNegative, "X  −SNAP");
                    spawn_editor_button(panel, EditorAction::MoveXPositive, "X  +SNAP");
                    spawn_editor_button(panel, EditorAction::MoveYNegative, "Y  −SNAP");
                    spawn_editor_button(panel, EditorAction::MoveYPositive, "Y  +SNAP");
                    spawn_editor_button(panel, EditorAction::MoveZNegative, "Z  −SNAP");
                    spawn_editor_button(panel, EditorAction::MoveZPositive, "Z  +SNAP");
                    spawn_editor_button(panel, EditorAction::RotateYNegative, "ROTATE Y  −15°");
                    spawn_editor_button(panel, EditorAction::RotateYPositive, "ROTATE Y  +15°");
                    spawn_editor_button(panel, EditorAction::ScaleDown, "SCALE  −10%");
                    spawn_editor_button(panel, EditorAction::ScaleUp, "SCALE  +10%");
                    panel.spawn((
                        EditorSettingsText,
                        Text::new("Loading machine preferences…"),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.70, 0.90, 1.0)),
                    ));
                    spawn_editor_button(panel, EditorAction::ToggleGrid, "TOGGLE GRID");
                    spawn_editor_button(
                        panel,
                        EditorAction::LabelsDistanceDecrease,
                        "LABEL RANGE −",
                    );
                    spawn_editor_button(
                        panel,
                        EditorAction::LabelsDistanceIncrease,
                        "LABEL RANGE +",
                    );
                    spawn_editor_button(
                        panel,
                        EditorAction::LabelsBudgetDecrease,
                        "LABEL BUDGET −",
                    );
                    spawn_editor_button(
                        panel,
                        EditorAction::LabelsBudgetIncrease,
                        "LABEL BUDGET +",
                    );
                    spawn_editor_button(panel, EditorAction::ToggleShadowMaps, "TOGGLE SHADOWS");
                    spawn_editor_button(panel, EditorAction::CycleShadowMapSize, "SHADOW QUALITY");
                    spawn_editor_button(
                        panel,
                        EditorAction::ToggleForgeContactShadows,
                        "CONTACT SHADOWS",
                    );
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
                            EditorAction::RegistryPrevious,
                            "◀ PREVIOUS RECORD",
                        );
                        spawn_editor_button(panel, EditorAction::RegistryNext, "NEXT RECORD ▶");
                        spawn_editor_button(
                            panel,
                            EditorAction::RegistryRename,
                            "RENAME CONTENT ID",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::EditProjectName,
                            "EDIT PROJECT NAME",
                        );
                        spawn_editor_button(panel, EditorAction::EditLevelName, "EDIT LEVEL NAME");
                        spawn_editor_button(
                            panel,
                            EditorAction::CreateDialogueGraph,
                            "+ DIALOGUE / CUTSCENE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::DialogueCycleMode,
                            "GAMEPLAY / CUTSCENE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::DialogueAddNode,
                            "+ DIALOGUE NODE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::DialogueAddAnimationCue,
                            "+ ANIMATION CUE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::DialogueRecordVoice,
                            "START / STOP VOICE TAKE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::ValidateProject,
                            "VALIDATE PROJECT",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::CreateMaterialRecord,
                            "+ SHADER MATERIAL",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::DuplicateRecord,
                            "DUPLICATE RECORD",
                        );
                        spawn_editor_button(panel, EditorAction::DeleteRecord, "DELETE RECORD");
                        spawn_editor_button(
                            panel,
                            EditorAction::NormalizeSourcePath,
                            "SYNC SOURCE PATH",
                        );
                        spawn_editor_button(panel, EditorAction::SearchRegistry, "SEARCH RECORDS");
                        spawn_editor_button(panel, EditorAction::PresetCapture, "CAPTURE PRESET");
                        spawn_editor_button(panel, EditorAction::PresetNext, "NEXT PRESET");
                        spawn_editor_button(panel, EditorAction::PresetApply, "APPLY PRESET");
                        spawn_editor_button(panel, EditorAction::PresetCompare, "COMPARE PRESET");
                        spawn_editor_button(panel, EditorAction::PresetFork, "FORK PRESET");
                        spawn_editor_button(
                            panel,
                            EditorAction::PresetNewSeedVariant,
                            "NEW-SEED VARIANT",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::MaterialCycleFamily,
                            "MATERIAL FAMILY",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::MaterialCyclePreset,
                            "MATERIAL PRESET",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::MaterialNextParameter,
                            "NEXT PARAMETER",
                        );
                        spawn_editor_button(panel, EditorAction::MaterialDecrease, "VALUE  −");
                        spawn_editor_button(panel, EditorAction::MaterialIncrease, "VALUE  +");
                        spawn_editor_button(panel, EditorAction::MaterialReset, "RESET MATERIAL");
                        spawn_editor_button(
                            panel,
                            EditorAction::ApplySelectedMaterial,
                            "APPLY TO OBJECT",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::ClearObjectMaterial,
                            "CLEAR OBJECT MATERIAL",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::CycleRecipeKind,
                            "WORLD RECIPE TYPE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::CreateRecipeRecord,
                            "+ WORLD RECIPE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::CopySelectedMaterial,
                            "COPY MATERIAL",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RecipeNextSlot,
                            "NEXT MATERIAL SLOT",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::BindRecipeMaterial,
                            "BIND COPIED MATERIAL",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::ClearRecipeMaterial,
                            "CLEAR RECIPE SLOT",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RegenerateRecipePreview,
                            "REGENERATE PREVIEW",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RecipeNextParameter,
                            "NEXT RECIPE PARAMETER",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RecipeParameterDecrease,
                            "RECIPE VALUE  −",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RecipeParameterIncrease,
                            "RECIPE VALUE  +",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RecipeResetParameters,
                            "RESET RECIPE VALUES",
                        );
                        spawn_editor_button(panel, EditorAction::RecipeSeedDecrease, "SEED  −");
                        spawn_editor_button(panel, EditorAction::RecipeSeedIncrease, "SEED  +");
                        spawn_editor_button(
                            panel,
                            EditorAction::ValidateSelectedRecipe,
                            "VALIDATE WORLD RECIPE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::SeedDefaultTopology,
                            "SEED DEFAULT TOPOLOGY",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyPrevious,
                            "◀ PREVIOUS TOPOLOGY ITEM",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyNext,
                            "NEXT TOPOLOGY ITEM ▶",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyMoveXNegative,
                            "TOPOLOGY X  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyMoveXPositive,
                            "TOPOLOGY X  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyMoveYNegative,
                            "TOPOLOGY Y  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyMoveYPositive,
                            "TOPOLOGY Y  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyMoveZNegative,
                            "TOPOLOGY Z  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyMoveZPositive,
                            "TOPOLOGY Z  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyMarkEdgeStart,
                            "MARK EDGE START",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyCycleEdgeKind,
                            "EDGE TYPE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyConnectEdge,
                            "CONNECT TO SELECTED NODE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TopologyRemoveEdge,
                            "REMOVE MATCHING EDGE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RoadTangentNextAxis,
                            "ROAD TANGENT AXIS",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RoadTangentDecrease,
                            "TANGENT VALUE  −",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RoadTangentIncrease,
                            "TANGENT VALUE  +",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RoadTangentReset,
                            "RESET AUTO TANGENT",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RoadTangentAutoRoundAll,
                            "AUTO ROUND ALL SPLINE POINTS",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RoadMarkJunctionStart,
                            "MARK ROAD JUNCTION START",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RoadCycleJunctionKind,
                            "ROAD JUNCTION TYPE",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RoadConnectJunction,
                            "CONNECT ROAD JUNCTION",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::RoadRemoveJunction,
                            "REMOVE ROAD JUNCTION",
                        );
                        spawn_editor_button(panel, EditorAction::SocketNext, "NEXT NODE SOCKET");
                        spawn_editor_button(
                            panel,
                            EditorAction::SocketCycleKind,
                            "NEW SOCKET TYPE",
                        );
                        spawn_editor_button(panel, EditorAction::SocketCreate, "+ NODE SOCKET");
                        spawn_editor_button(
                            panel,
                            EditorAction::SocketDelete,
                            "DELETE NODE SOCKET",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::SocketMoveXNegative,
                            "SOCKET X  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::SocketMoveXPositive,
                            "SOCKET X  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::SocketMoveYNegative,
                            "SOCKET Y  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::SocketMoveYPositive,
                            "SOCKET Y  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::SocketMoveZNegative,
                            "SOCKET Z  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::SocketMoveZPositive,
                            "SOCKET Z  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TerrainOriginXNegative,
                            "TERRAIN ORIGIN X  −250M",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TerrainOriginXPositive,
                            "TERRAIN ORIGIN X  +250M",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TerrainOriginZNegative,
                            "TERRAIN ORIGIN Z  −250M",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TerrainOriginZPositive,
                            "TERRAIN ORIGIN Z  +250M",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TerrainClearanceDecrease,
                            "TERRAIN CLEARANCE  −SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TerrainClearanceIncrease,
                            "TERRAIN CLEARANCE  +SNAP",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TerrainProjectSelected,
                            "PROJECT SELECTED TO TERRAIN",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::TerrainProjectAll,
                            "PROJECT ALL TO TERRAIN",
                        );
                        spawn_editor_button(
                            panel,
                            EditorAction::CompileSandboxRoot,
                            "COMPILE SANDBOX ROOT",
                        );
                        spawn_editor_button(
                            panel,
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
                EditorChromeSurface,
                RelativeCursorPosition::default(),
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

/// Adds native Bevy 0.19 text fields through one reusable Feathers scene
/// adapter. Reduced/headless apps do not install the Feathers theme, so they
/// retain the deterministic manual keyboard-input fallback used by tests.
fn spawn_forge_editable_fields(
    mut commands: Commands,
    theme: Option<Res<UiTheme>>,
    panels: Query<(Entity, &EditorPanelKind)>,
) {
    if theme.is_none() {
        return;
    }
    let outliner = panels
        .iter()
        .find_map(|(entity, kind)| (*kind == EditorPanelKind::Outliner).then_some(entity));
    let registry = panels
        .iter()
        .find_map(|(entity, kind)| (*kind == EditorPanelKind::Registry).then_some(entity));
    let (Some(outliner), Some(registry)) = (outliner, registry) else {
        return;
    };

    let outliner_field = spawn_forge_text_field(
        &mut commands,
        ForgeTextField::OutlinerFilter,
        "Forge Outliner Search",
    );
    // Summary and the static filter caption remain first; the actual native
    // field is inserted before the controller activation button.
    commands
        .entity(outliner)
        .insert_children(2, &[outliner_field]);

    let registry_fields = [
        spawn_forge_text_field(
            &mut commands,
            ForgeTextField::ProjectName,
            "Forge Project Name",
        ),
        spawn_forge_text_field(
            &mut commands,
            ForgeTextField::LevelName,
            "Forge Active Level Name",
        ),
        spawn_forge_text_field(
            &mut commands,
            ForgeTextField::RegistrySearch,
            "Forge Registry Search",
        ),
        spawn_forge_text_field(&mut commands, ForgeTextField::ContentId, "Forge Content ID"),
    ];
    commands
        .entity(registry)
        .insert_children(1, &registry_fields);
}

fn spawn_forge_text_field(
    commands: &mut Commands,
    kind: ForgeTextField,
    name: &'static str,
) -> Entity {
    let root = commands
        .spawn_scene(bsn! {
            @FeathersTextInputContainer
            Name(name)
            Children [
                (
                    @FeathersTextInput
                    ForgeTextFieldMarker { kind }
                )
            ]
        })
        .id();
    let label = match kind {
        ForgeTextField::OutlinerFilter => "OBJECT FILTER",
        ForgeTextField::RegistrySearch => "REGISTRY SEARCH",
        ForgeTextField::ContentId => "CONTENT ID",
        ForgeTextField::ProjectName => "PROJECT NAME",
        ForgeTextField::LevelName => "ACTIVE LEVEL NAME",
    };
    let label_entity = commands
        .spawn((
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::srgb(0.48, 0.84, 1.0)),
        ))
        .id();
    commands.entity(root).insert_children(0, &[label_entity]);
    root
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
    crate::engine_tools::tool_windows::spawn_tool_window(
        parent,
        title,
        position,
        crate::engine_tools::tool_windows::ToolWindowStyle::default(),
        (kind, ScrollPosition::default()),
        content,
    );
}

fn spawn_editor_button(parent: &mut ChildSpawnerCommands, control: EditorAction, label: &str) {
    let panel = EDITOR_CONTROLS
        .iter()
        .find(|spec| spec.id == control)
        .expect("every editor button must be declared in EDITOR_CONTROLS")
        .panel;
    parent
        .spawn((
            Button,
            EditorButton { control, panel },
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
    mut input_focus: Option<ResMut<InputFocus>>,
) {
    for (interaction, button) in &mut interactions {
        if *interaction == Interaction::Pressed {
            if let Some(input_focus) = input_focus.as_deref_mut() {
                input_focus.clear();
            }
            focus.0 = button.control;
            pending.0.push(button.control);
        }
    }
}

fn editor_search_input(
    keys: Res<ButtonInput<Key>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    settings: Res<GameSettings>,
    mut filter: ResMut<EditorOutlinerFilter>,
    mut registry: ResMut<EditorRegistryState>,
    session: Res<EditorProjectSession>,
    mut status: ResMut<EditorRuntimeStatus>,
    mut capture: ResMut<EditorTextInputCapture>,
    mut input_focus: ResMut<InputFocus>,
    mut native_fields: Query<(Entity, &ForgeTextFieldMarker, &mut EditableText)>,
    mut pending: ResMut<EditorPendingTransactions>,
) {
    capture.0 = filter.active
        || registry.rename_active
        || registry.search_active
        || registry.project_name_active
        || registry.level_name_active;
    if !capture.0 {
        return;
    }

    let controller_accept = gamepads.iter().any(|gamepad| {
        gamepad.just_pressed(mapped_gamepad_face(&settings.bindings, FaceButton::South))
    }) || native
        .just_pressed(mapped_native_face(&settings.bindings, FaceButton::South));
    let controller_clear = gamepads.iter().any(|gamepad| {
        gamepad.just_pressed(mapped_gamepad_face(&settings.bindings, FaceButton::East))
    }) || native
        .just_pressed(mapped_native_face(&settings.bindings, FaceButton::East));

    let keyboard_accept = keys.just_pressed(Key::Enter);
    let keyboard_cancel = keys.just_pressed(Key::Escape);

    if registry.project_name_active {
        let native = native_fields
            .iter()
            .any(|(_, field, _)| field.kind == ForgeTextField::ProjectName);
        let completion = if native {
            field_completion(
                controller_accept || keyboard_accept,
                controller_clear || keyboard_cancel,
            )
        } else {
            manual_text_edit(
                &keys,
                &mut registry.project_name_buffer,
                ForgeTextField::ProjectName,
                controller_accept,
                controller_clear,
            )
        };
        if completion == TextFieldCompletion::Cancel {
            registry.project_name_active = false;
            input_focus.clear();
            status.message = "Project-name edit cancelled".into();
        } else if completion == TextFieldCompletion::Accept {
            match validated_display_name(&registry.project_name_buffer) {
                Ok(after) => {
                    let before = session.project.display_name.clone();
                    registry.project_name_active = false;
                    input_focus.clear();
                    if before == after {
                        status.message = "Project name unchanged".into();
                    } else {
                        pending.0.push(EditorTransaction {
                            description: format!("Rename project to {after}"),
                            commands: vec![EditorCommand::SetProjectDisplayName { before, after }],
                        });
                    }
                }
                Err(message) => status.message = message.into(),
            }
        }
        return;
    }

    if registry.level_name_active {
        let native = native_fields
            .iter()
            .any(|(_, field, _)| field.kind == ForgeTextField::LevelName);
        let completion = if native {
            field_completion(
                controller_accept || keyboard_accept,
                controller_clear || keyboard_cancel,
            )
        } else {
            manual_text_edit(
                &keys,
                &mut registry.level_name_buffer,
                ForgeTextField::LevelName,
                controller_accept,
                controller_clear,
            )
        };
        if completion == TextFieldCompletion::Cancel {
            registry.level_name_active = false;
            input_focus.clear();
            status.message = "Level-name edit cancelled".into();
        } else if completion == TextFieldCompletion::Accept {
            let active_level = session
                .project
                .active_level()
                .map(|level| (level.level_id.clone(), level.display_name.clone()));
            match (
                active_level,
                validated_display_name(&registry.level_name_buffer),
            ) {
                (Some((level_id, before)), Ok(after)) => {
                    registry.level_name_active = false;
                    input_focus.clear();
                    if before == after {
                        status.message = "Level name unchanged".into();
                    } else {
                        pending.0.push(EditorTransaction {
                            description: format!("Rename level to {after}"),
                            commands: vec![EditorCommand::SetLevelDisplayName {
                                level_id,
                                before,
                                after,
                            }],
                        });
                    }
                }
                (None, _) => {
                    registry.level_name_active = false;
                    status.message = "No active level to rename".into();
                }
                (_, Err(message)) => status.message = message.into(),
            }
        }
        return;
    }

    if registry.search_active {
        let native = native_fields
            .iter()
            .any(|(_, field, _)| field.kind == ForgeTextField::RegistrySearch);
        let completion = if native {
            field_completion(
                controller_accept || keyboard_accept,
                controller_clear || keyboard_cancel,
            )
        } else {
            manual_text_edit(
                &keys,
                &mut registry.search_text,
                ForgeTextField::RegistrySearch,
                controller_accept,
                controller_clear,
            )
        };
        if completion == TextFieldCompletion::Cancel {
            registry.search_text.clear();
            registry.search_active = false;
            clear_native_field(&mut native_fields, ForgeTextField::RegistrySearch);
            input_focus.clear();
            status.message = "Registry search cleared".into();
        } else if completion == TextFieldCompletion::Accept {
            registry.search_active = false;
            input_focus.clear();
            status.message = "Registry search applied".into();
        }
        return;
    }

    if registry.rename_active {
        let native = native_fields
            .iter()
            .any(|(_, field, _)| field.kind == ForgeTextField::ContentId);
        let completion = if native {
            field_completion(
                controller_accept || keyboard_accept,
                controller_clear || keyboard_cancel,
            )
        } else {
            manual_text_edit(
                &keys,
                &mut registry.rename_buffer,
                ForgeTextField::ContentId,
                controller_accept,
                controller_clear,
            )
        };
        if completion == TextFieldCompletion::Cancel {
            registry.rename_active = false;
            input_focus.clear();
            status.message = "Content rename cancelled".into();
        } else if completion == TextFieldCompletion::Accept {
            let old_id = session
                .project
                .records
                .get(registry.selected)
                .map(|record| record.content_id.clone());
            let new_id = registry.rename_buffer.trim().to_owned();
            match old_id {
                Some(old_id) if old_id == new_id => {
                    registry.rename_active = false;
                    input_focus.clear();
                    status.message = "Content ID unchanged".into();
                }
                Some(old_id) => {
                    let mut candidate = session.project.clone();
                    match candidate.rename_content(&old_id, &new_id) {
                        Ok(()) => {
                            registry.rename_active = false;
                            input_focus.clear();
                            pending.0.push(EditorTransaction {
                                description: format!("Rename {old_id} to {new_id}"),
                                commands: vec![EditorCommand::RenameContent {
                                    before: old_id,
                                    after: new_id,
                                }],
                            });
                        }
                        Err(error) => {
                            status.message = format!("Rename rejected: {error:?}");
                        }
                    }
                }
                None => {
                    registry.rename_active = false;
                    status.message = "No registry record selected".into();
                }
            }
        }
        return;
    }

    let native_outliner = native_fields.iter().find_map(|(entity, field, _)| {
        (field.kind == ForgeTextField::OutlinerFilter).then_some(entity)
    });
    if let Some(entity) = native_outliner {
        if input_focus.get() != Some(entity) {
            input_focus.set(entity, FocusCause::Navigated);
        }
        if controller_clear || keys.just_pressed(Key::Escape) {
            clear_native_field(&mut native_fields, ForgeTextField::OutlinerFilter);
            filter.text.clear();
            filter.active = false;
            input_focus.clear();
            status.message = "Outliner filter cleared".into();
        } else if controller_accept || keys.just_pressed(Key::Enter) {
            filter.active = false;
            input_focus.clear();
            status.message = "Outliner filter applied".into();
        }
        // Bevy's EditableTextInputPlugin owns character, selection, clipboard,
        // and IME events for the native field. Manual key editing below is the
        // deterministic fallback for reduced/headless editor harnesses.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextFieldCompletion {
    Continue,
    Accept,
    Cancel,
}

fn field_completion(accept: bool, cancel: bool) -> TextFieldCompletion {
    if cancel {
        TextFieldCompletion::Cancel
    } else if accept {
        TextFieldCompletion::Accept
    } else {
        TextFieldCompletion::Continue
    }
}

fn manual_text_edit(
    keys: &ButtonInput<Key>,
    buffer: &mut String,
    kind: ForgeTextField,
    controller_accept: bool,
    controller_cancel: bool,
) -> TextFieldCompletion {
    let mut completion = field_completion(controller_accept, controller_cancel);
    for key in keys.get_just_pressed() {
        match key {
            Key::Character(value) => {
                buffer.push_str(value);
                *buffer = sanitize_forge_text(kind, buffer);
            }
            Key::Backspace => {
                buffer.pop();
            }
            Key::Enter => completion = TextFieldCompletion::Accept,
            Key::Escape => completion = TextFieldCompletion::Cancel,
            _ => {}
        }
    }
    completion
}

fn sanitize_forge_text(kind: ForgeTextField, value: &str) -> String {
    let (max_characters, lowercase) = match kind {
        ForgeTextField::OutlinerFilter => (32, true),
        ForgeTextField::RegistrySearch => (48, true),
        ForgeTextField::ContentId => (64, true),
        ForgeTextField::ProjectName | ForgeTextField::LevelName => (64, false),
    };
    let characters = value.chars().filter(|character| match kind {
        ForgeTextField::OutlinerFilter => {
            character.is_alphanumeric() || matches!(character, ' ' | '_' | '-')
        }
        ForgeTextField::RegistrySearch => {
            character.is_alphanumeric() || matches!(character, ' ' | '.' | '_' | '-')
        }
        ForgeTextField::ContentId => {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        }
        ForgeTextField::ProjectName | ForgeTextField::LevelName => {
            !character.is_control() && *character != '\n' && *character != '\r'
        }
    });
    if lowercase {
        characters
            .flat_map(char::to_lowercase)
            .take(max_characters)
            .collect()
    } else {
        characters.take(max_characters).collect()
    }
}

fn validated_display_name(value: &str) -> Result<String, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        Err("Name cannot be empty")
    } else if value.chars().count() > 64 {
        Err("Name must be 64 characters or fewer")
    } else {
        Ok(value.to_owned())
    }
}

fn clear_native_field(
    fields: &mut Query<(Entity, &ForgeTextFieldMarker, &mut EditableText)>,
    target: ForgeTextField,
) {
    for (_, field, mut input) in fields.iter_mut() {
        if field.kind == target {
            input.editor_mut().set_text("");
            break;
        }
    }
}

fn sync_native_forge_fields(
    input_focus: Res<InputFocus>,
    mut inputs: Query<(Entity, &ForgeTextFieldMarker, &mut EditableText)>,
    mut filter: ResMut<EditorOutlinerFilter>,
    mut registry: ResMut<EditorRegistryState>,
    session: Res<EditorProjectSession>,
    mut capture: ResMut<EditorTextInputCapture>,
) {
    let focused_kind = input_focus.get().and_then(|focused| {
        inputs
            .iter()
            .find_map(|(entity, field, _)| (entity == focused).then_some(field.kind))
    });
    if let Some(kind) = focused_kind {
        filter.active = kind == ForgeTextField::OutlinerFilter;
        registry.search_active = kind == ForgeTextField::RegistrySearch;
        registry.rename_active = kind == ForgeTextField::ContentId;
        registry.project_name_active = kind == ForgeTextField::ProjectName;
        registry.level_name_active = kind == ForgeTextField::LevelName;
    }

    for (_, field, mut input) in &mut inputs {
        let kind = field.kind;
        let max_characters = match kind {
            ForgeTextField::OutlinerFilter => 32,
            ForgeTextField::RegistrySearch => 48,
            ForgeTextField::ContentId | ForgeTextField::ProjectName | ForgeTextField::LevelName => {
                64
            }
        };
        if input.max_characters != Some(max_characters) {
            input.max_characters = Some(max_characters);
        }
        if input.visible_width != Some(25.0) {
            input.visible_width = Some(25.0);
        }
        let active = match kind {
            ForgeTextField::OutlinerFilter => filter.active,
            ForgeTextField::RegistrySearch => registry.search_active,
            ForgeTextField::ContentId => registry.rename_active,
            ForgeTextField::ProjectName => registry.project_name_active,
            ForgeTextField::LevelName => registry.level_name_active,
        };
        let desired = match kind {
            ForgeTextField::OutlinerFilter => filter.text.clone(),
            ForgeTextField::RegistrySearch => registry.search_text.clone(),
            ForgeTextField::ContentId if registry.rename_active => registry.rename_buffer.clone(),
            ForgeTextField::ContentId => session
                .project
                .records
                .get(registry.selected)
                .map(|record| record.content_id.clone())
                .unwrap_or_default(),
            ForgeTextField::ProjectName if registry.project_name_active => {
                registry.project_name_buffer.clone()
            }
            ForgeTextField::ProjectName => session.project.display_name.clone(),
            ForgeTextField::LevelName if registry.level_name_active => {
                registry.level_name_buffer.clone()
            }
            ForgeTextField::LevelName => session
                .project
                .active_level()
                .map(|level| level.display_name.clone())
                .unwrap_or_default(),
        };

        if active {
            let raw = input.value().to_string();
            let sanitized = sanitize_forge_text(kind, &raw);
            if sanitized != raw {
                input.editor_mut().set_text(&sanitized);
            }
            match kind {
                ForgeTextField::OutlinerFilter => filter.text = sanitized,
                ForgeTextField::RegistrySearch => registry.search_text = sanitized,
                ForgeTextField::ContentId => registry.rename_buffer = sanitized,
                ForgeTextField::ProjectName => registry.project_name_buffer = sanitized,
                ForgeTextField::LevelName => registry.level_name_buffer = sanitized,
            }
        } else if input.value().to_string() != desired {
            input.editor_mut().set_text(&desired);
        }
    }
    capture.0 = filter.active
        || registry.rename_active
        || registry.search_active
        || registry.project_name_active
        || registry.level_name_active;
}

fn editor_controller_navigation(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    settings: Res<GameSettings>,
    buttons: Query<(&EditorButton, &ComputedNode)>,
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
    if keyboard.just_pressed(KeyCode::KeyV) {
        pending.0.push(EditorAction::ToggleSemanticLabels);
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
        || gamepads.iter().any(|gamepad| {
            gamepad.just_pressed(mapped_gamepad_face(&settings.bindings, FaceButton::South))
        })
        || native.just_pressed(mapped_native_face(&settings.bindings, FaceButton::South));

    let controls = ordered_present_editor_controls(buttons.iter().map(|(button, node)| {
        let size = node.size();
        (button.control, size.x > 0.0 && size.y > 0.0)
    }));
    let Some(mut current) = normalized_editor_focus(focus.0, &controls) else {
        return;
    };
    if previous {
        current = move_editor_focus(current, &controls, EditorFocusDirection::Previous)
            .expect("non-empty focus registry must have a previous control");
    } else if next {
        current = move_editor_focus(current, &controls, EditorFocusDirection::Next)
            .expect("non-empty focus registry must have a next control");
    }
    focus.0 = current;
    if activate {
        if let Some(action) = focused_editor_action(current, &controls) {
            pending.0.push(action);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorFocusDirection {
    Previous,
    Next,
}

/// Return the present, visible controls in the one authoritative traversal
/// order. Query iteration order is deliberately ignored because ECS storage
/// order is not a UI-navigation contract.
fn ordered_present_editor_controls(
    buttons: impl IntoIterator<Item = (EditorAction, bool)>,
) -> Vec<EditorAction> {
    let buttons = buttons.into_iter().collect::<Vec<_>>();
    EDITOR_CONTROLS
        .iter()
        .filter(|spec| {
            buttons
                .iter()
                .any(|(control, visible)| *control == spec.id && *visible)
        })
        .map(|spec| spec.id)
        .collect()
}

fn normalized_editor_focus(
    current: EditorAction,
    controls: &[EditorAction],
) -> Option<EditorAction> {
    controls
        .contains(&current)
        .then_some(current)
        .or_else(|| controls.first().copied())
}

fn focused_editor_action(current: EditorAction, controls: &[EditorAction]) -> Option<EditorAction> {
    controls.contains(&current).then_some(current)
}

fn move_editor_focus(
    current: EditorAction,
    controls: &[EditorAction],
    direction: EditorFocusDirection,
) -> Option<EditorAction> {
    let index = controls.iter().position(|control| *control == current)?;
    let next_index = match direction {
        EditorFocusDirection::Previous => index.checked_sub(1).unwrap_or(controls.len() - 1),
        EditorFocusDirection::Next => (index + 1) % controls.len(),
    };
    controls.get(next_index).copied()
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
    let Some((button, button_transform, button_node)) = buttons
        .iter()
        .find(|(button, _, _)| button.control == focus.0)
    else {
        return;
    };
    let Some(panel_kind) = button.panel else {
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

/// Whether viewport pointer handling may run this frame. Floating tool windows
/// own fresh pointer gestures under their current, movable geometry; an active
/// viewport drag keeps ownership until it receives its release/cancel frame.
fn editor_viewport_pointer_available(
    editor_ui_captures_pointer: bool,
    viewport_gesture_active: bool,
) -> bool {
    !editor_ui_captures_pointer || viewport_gesture_active
}

fn editor_ui_captures_pointer(
    tool_window_captures_pointer: bool,
    chrome_surfaces: impl IntoIterator<Item = bool>,
) -> bool {
    tool_window_captures_pointer || chrome_surfaces.into_iter().any(|cursor_over| cursor_over)
}

/// Prevents the upstream transform gizmo from beginning a drag through Forge's
/// floating chrome. Once Bevy owns a drag, it keeps receiving updates until
/// release even if the pointer crosses a panel.
fn bevy_transform_gizmo_pointer_available(
    tool_window_pointer: Option<Res<tool_windows::ToolWindowPointerState>>,
    chrome_surfaces: Query<&RelativeCursorPosition, With<EditorChromeSurface>>,
    bevy_state: Option<Res<TransformGizmoState>>,
) -> bool {
    editor_viewport_pointer_available(
        editor_ui_captures_pointer(
            tool_window_pointer.is_some_and(|pointer| pointer.captures_viewport()),
            chrome_surfaces
                .iter()
                .map(RelativeCursorPosition::cursor_over),
        ),
        bevy_state.is_some_and(|state| state.active),
    )
}

/// Maps Starfall's persistent selection and controller-friendly tool settings
/// onto Bevy 0.19's viewport gizmo. Persistent IDs and undo remain owned by
/// Forge; the upstream gizmo only performs the pointer-space manipulation.
fn sync_bevy_transform_gizmo(
    mut commands: Commands,
    selection: Res<EditorSelection>,
    settings: Res<EditorGizmoSettings>,
    mut bevy_settings: Option<ResMut<BevyTransformGizmoSettings>>,
    authorables: Query<
        (
            Entity,
            &EditorEntityId,
            &EditorAccess,
            Has<TransformGizmoFocus>,
        ),
        With<Authorable>,
    >,
) {
    if let Some(bevy_settings) = bevy_settings.as_deref_mut() {
        bevy_settings.mode = match settings.mode {
            EditorGizmoMode::Translate => BevyTransformGizmoMode::Translate,
            EditorGizmoMode::Rotate => BevyTransformGizmoMode::Rotate,
            EditorGizmoMode::Scale => BevyTransformGizmoMode::Scale,
        };
        bevy_settings.space = match settings.space {
            EditorTransformSpace::World => BevyTransformGizmoSpace::World,
            EditorTransformSpace::Local => BevyTransformGizmoSpace::Local,
        };
        bevy_settings.snap_translate = Some(settings.translation_snap());
        bevy_settings.snap_rotate = Some(15.0_f32.to_radians());
        bevy_settings.snap_scale = Some(0.1);
        bevy_settings.axis_length = 1.25;
        bevy_settings.rotate_ring_radius = 1.15;
        bevy_settings.screen_scale_factor = 0.105;
    }

    let active = selection.active();
    for (entity, id, access, focused) in &authorables {
        let should_focus = Some(*id) == active && *access == EditorAccess::Editable;
        match (focused, should_focus) {
            (false, true) => {
                commands.entity(entity).insert(TransformGizmoFocus);
            }
            (true, false) => {
                commands.entity(entity).remove::<TransformGizmoFocus>();
            }
            _ => {}
        }
    }
}

fn record_bevy_transform_gizmo_transaction(
    bevy_state: Option<Res<TransformGizmoState>>,
    mut adapter: ResMut<BevyTransformGizmoAdapterDrag>,
    authorables: Query<(&EditorEntityId, &Transform), With<Authorable>>,
    mut pending: ResMut<EditorPendingTransactions>,
    mut status: ResMut<EditorRuntimeStatus>,
) {
    let Some(bevy_state) = bevy_state else {
        adapter.0 = None;
        return;
    };
    if bevy_state.active {
        if adapter.0.is_none() {
            if let Some(entity) = bevy_state.entity {
                if let Ok((id, _)) = authorables.get(entity) {
                    adapter.0 = Some((*id, bevy_state.start_transform));
                    status.message = format!("Dragging {:?} with Bevy gizmo", bevy_state.axis);
                }
            }
        }
        return;
    }

    let Some((id, before)) = adapter.0.take() else {
        return;
    };
    let Some((_, after)) = authorables.iter().find(|(candidate, _)| **candidate == id) else {
        status.message = "Transform drag cancelled because its object disappeared".into();
        return;
    };
    if *after == before {
        status.message = "Transform drag cancelled".into();
        return;
    }
    pending.0.push(EditorTransaction {
        description: "Bevy transform gizmo drag".into(),
        commands: vec![EditorCommand::SetTransform {
            id,
            before,
            after: *after,
        }],
    });
}

#[allow(clippy::too_many_arguments)]
fn editor_gizmo_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<EditorCamera>>,
    tool_window_pointer: Res<tool_windows::ToolWindowPointerState>,
    chrome_surfaces: Query<&RelativeCursorPosition, With<EditorChromeSurface>>,
    text_capture: Res<EditorTextInputCapture>,
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
    bevy_gizmo: Option<Res<BevyTransformGizmoSettings>>,
) {
    // Production uses Bevy 0.19's precise ray/axis implementation. Headless
    // and reduced plugin harnesses retain the proven Starfall fallback.
    if bevy_gizmo.is_some() {
        return;
    }
    if text_capture.0 {
        return;
    }
    // Starting a viewport gesture through floating editor chrome is unsafe,
    // but an in-progress gizmo drag must still receive motion and release if
    // the pointer crosses a tool window on its way to the final position.
    if !editor_viewport_pointer_available(
        editor_ui_captures_pointer(
            tool_window_pointer.captures_viewport(),
            chrome_surfaces
                .iter()
                .map(RelativeCursorPosition::cursor_over),
        ),
        drag_state.0.is_some(),
    ) {
        return;
    }
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
    tool_window_pointer: Res<tool_windows::ToolWindowPointerState>,
    chrome_surfaces: Query<&RelativeCursorPosition, With<EditorChromeSurface>>,
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
    // Match ordinary transform drags: tool windows block a new topology drag,
    // while an existing drag remains authoritative until release/cancel even
    // if its pointer crosses editor UI.
    if !editor_viewport_pointer_available(
        editor_ui_captures_pointer(
            tool_window_pointer.captures_viewport(),
            chrome_surfaces
                .iter()
                .map(RelativeCursorPosition::cursor_over),
        ),
        drag_state.0.is_some(),
    ) {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) && drag_state.0.is_none() {
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
    tool_window_pointer: Res<tool_windows::ToolWindowPointerState>,
    chrome_surfaces: Query<&RelativeCursorPosition, With<EditorChromeSurface>>,
    filter: Res<EditorOutlinerFilter>,
    text_capture: Res<EditorTextInputCapture>,
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
        || text_capture.0
        || drag.0.is_some()
        || topology_drag.0.is_some()
        || !editor_viewport_pointer_available(
            editor_ui_captures_pointer(
                tool_window_pointer.captures_viewport(),
                chrome_surfaces
                    .iter()
                    .map(RelativeCursorPosition::cursor_over),
            ),
            false,
        )
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

fn selected_dialogue_graph(
    world: &World,
) -> Result<(String, dialogue_records::DialogueGraph), String> {
    let selected = world.resource::<EditorRegistryState>().selected;
    let session = world.resource::<EditorProjectSession>();
    let content_id = session
        .project
        .records
        .get(selected)
        .map(|record| record.content_id.clone())
        .ok_or_else(|| "No registry record selected".to_owned())?;
    let graph = dialogue_records::load_dialogue(&session.project, &content_id)
        .map_err(|_| "Select a Dialogue Forge record first".to_owned())?;
    Ok((content_id, graph))
}

fn queue_dialogue_graph_edit(
    world: &mut World,
    content_id: String,
    before: dialogue_records::DialogueGraph,
    after: dialogue_records::DialogueGraph,
    description: impl Into<String>,
) {
    world
        .resource_mut::<EditorPendingTransactions>()
        .0
        .push(EditorTransaction {
            description: description.into(),
            commands: vec![EditorCommand::SetDialogueGraph {
                content_id,
                before: Box::new(before),
                after: Box::new(after),
            }],
        });
}

fn toggle_dialogue_voice_recording(world: &mut World) {
    let active = world
        .get_non_send::<DialogueVoiceCapture>()
        .is_some_and(|capture| capture.recorder.is_some());
    if active {
        let (recorder, content_id, node_id, take) = {
            let mut capture = world.non_send_mut::<DialogueVoiceCapture>();
            (
                capture
                    .recorder
                    .take()
                    .expect("active capture has a recorder"),
                std::mem::take(&mut capture.content_id),
                std::mem::take(&mut capture.node_id),
                capture.take,
            )
        };
        let project_root = world
            .resource::<EditorProjectSession>()
            .store
            .path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let safe_graph = content_id.replace(['.', '/'], "_");
        let safe_node = node_id.replace(['.', '/'], "_");
        let asset_path = format!("voice/dialogue/{safe_graph}/{safe_node}_take_{take}.mp3");
        match recorder.finish(&project_root, &asset_path, take) {
            Ok(clip) => {
                let Ok((selected_id, before)) = selected_dialogue_graph(world) else {
                    set_editor_status(world, "Recording saved, but its dialogue selection changed");
                    return;
                };
                if selected_id != content_id {
                    set_editor_status(world, "Recording saved, but its dialogue selection changed");
                    return;
                }
                let mut after = before.clone();
                let Some(node) = after.nodes.iter_mut().find(|node| node.node_id == node_id) else {
                    set_editor_status(world, "Recording saved, but its dialogue node is missing");
                    return;
                };
                let duration = clip.duration_seconds;
                node.voice = Some(clip);
                queue_dialogue_graph_edit(world, content_id, before, after, "Attach voice take");
                set_editor_status(
                    world,
                    format!("Voice take {take} captured ({duration:.1}s) • Enter Undo to detach"),
                );
            }
            Err(error) => set_editor_status(world, format!("Voice take rejected: {error}")),
        }
        return;
    }

    let Ok((content_id, graph)) = selected_dialogue_graph(world) else {
        set_editor_status(world, "Select a Dialogue Forge record before recording");
        return;
    };
    let Some(node) = graph.nodes.last() else {
        set_editor_status(world, "Add a dialogue node before recording");
        return;
    };
    let take = node
        .voice
        .as_ref()
        .map_or(1, |voice| voice.take.saturating_add(1));
    match dialogue_records::VoiceRecorder::start() {
        Ok(recorder) => {
            let channels = recorder.input_channels();
            let node_id = node.node_id.clone();
            let mut capture = world.non_send_mut::<DialogueVoiceCapture>();
            capture.recorder = Some(recorder);
            capture.content_id = content_id;
            capture.node_id = node_id.clone();
            capture.take = take;
            set_editor_status(
                world,
                format!(
                    "RECORDING node {node_id} • take {take} • {channels}-channel input → 96-kbps mono MP3"
                ),
            );
        }
        Err(error) => set_editor_status(world, format!("Could not start voice recording: {error}")),
    }
}

fn apply_editor_action(world: &mut World, action: EditorAction) {
    if !matches!(
        action,
        EditorAction::FocusSearch
            | EditorAction::RegistryRename
            | EditorAction::SearchRegistry
            | EditorAction::EditProjectName
            | EditorAction::EditLevelName
    ) {
        deactivate_forge_text_fields(world);
    }
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
            deactivate_forge_text_fields(world);
            let active = {
                let mut filter = world.resource_mut::<EditorOutlinerFilter>();
                filter.active = true;
                filter.active
            };
            let value = world.resource::<EditorOutlinerFilter>().text.clone();
            focus_forge_text_field(world, ForgeTextField::OutlinerFilter, &value);
            let message = if active {
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
        EditorAction::CreateLadder => spawn_editor_primitive(world, EditorPrimitive::Ladder),
        EditorAction::CreateStairs => spawn_editor_primitive(world, EditorPrimitive::Stairs),
        EditorAction::CreateTower => spawn_editor_primitive(world, EditorPrimitive::Tower),
        EditorAction::CreateCastle => spawn_editor_primitive(world, EditorPrimitive::Castle),
        EditorAction::CreateFloatingIsland => {
            spawn_editor_primitive(world, EditorPrimitive::FloatingIsland)
        }
        EditorAction::CreateMovingPlatform => {
            spawn_editor_primitive(world, EditorPrimitive::MovingPlatform)
        }
        EditorAction::CreateSpringPlatform => {
            spawn_editor_primitive(world, EditorPrimitive::SpringPlatform)
        }
        EditorAction::CreateFlipPanel => spawn_editor_primitive(world, EditorPrimitive::FlipPanel),
        EditorAction::CreateRotatingBridge => {
            spawn_editor_primitive(world, EditorPrimitive::RotatingBridge)
        }
        EditorAction::CreateCollapseBridge => {
            spawn_editor_primitive(world, EditorPrimitive::CollapseBridge)
        }
        EditorAction::CreateSpikeBridge => {
            spawn_editor_primitive(world, EditorPrimitive::SpikeBridge)
        }
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
            let snap_index = world.resource::<EditorGizmoSettings>().snap_index as u8;
            world
                .resource_mut::<EditorPreferences>()
                .translation_snap_index = snap_index;
            queue_machine_settings_save(world);
            set_editor_status(world, format!("Translation snap: {snap:.2} m"));
        }
        EditorAction::ToggleSemanticLabels => {
            let visible = {
                let mut preferences = world.resource_mut::<EditorPreferences>();
                preferences.semantic_labels_visible = !preferences.semantic_labels_visible;
                preferences.semantic_labels_visible
            };
            queue_machine_settings_save(world);
            set_editor_status(
                world,
                if visible {
                    "Semantic viewport labels: visible"
                } else {
                    "Semantic viewport labels: hidden"
                },
            );
        }
        EditorAction::ToggleGrid => {
            let visible = {
                let mut preferences = world.resource_mut::<EditorPreferences>();
                preferences.infinite_grid_visible = !preferences.infinite_grid_visible;
                preferences.infinite_grid_visible
            };
            let mut grids = world.query_filtered::<&mut Visibility, With<EditorInfiniteGrid>>();
            for mut visibility in grids.iter_mut(world) {
                *visibility = if visible {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
            queue_machine_settings_save(world);
            set_editor_status(world, format!("Infinite grid: {}", on_off(visible)));
        }
        EditorAction::LabelsDistanceDecrease | EditorAction::LabelsDistanceIncrease => {
            let distance = {
                let mut preferences = world.resource_mut::<EditorPreferences>();
                let delta = if action == EditorAction::LabelsDistanceIncrease {
                    EditorPreferences::LABEL_DISTANCE_STEP
                } else {
                    -EditorPreferences::LABEL_DISTANCE_STEP
                };
                preferences.semantic_label_distance += delta;
                preferences.sanitize();
                preferences.semantic_label_distance
            };
            queue_machine_settings_save(world);
            set_editor_status(world, format!("Semantic label range: {distance:.0} m"));
        }
        EditorAction::LabelsBudgetDecrease | EditorAction::LabelsBudgetIncrease => {
            let budget = {
                let mut preferences = world.resource_mut::<EditorPreferences>();
                if action == EditorAction::LabelsBudgetIncrease {
                    preferences.semantic_label_budget = preferences
                        .semantic_label_budget
                        .saturating_add(EditorPreferences::LABEL_BUDGET_STEP);
                } else {
                    preferences.semantic_label_budget = preferences
                        .semantic_label_budget
                        .saturating_sub(EditorPreferences::LABEL_BUDGET_STEP);
                }
                preferences.sanitize();
                preferences.semantic_label_budget
            };
            queue_machine_settings_save(world);
            set_editor_status(world, format!("Semantic label budget: {budget}"));
        }
        EditorAction::ToggleShadowMaps => {
            let enabled = {
                let mut quality = world.resource_mut::<RenderQualitySettings>();
                quality.shadow_maps_enabled = !quality.shadow_maps_enabled;
                quality.shadow_maps_enabled
            };
            queue_machine_settings_save(world);
            set_editor_status(world, format!("Shadow maps: {}", on_off(enabled)));
        }
        EditorAction::CycleShadowMapSize => {
            let size = world
                .resource_mut::<RenderQualitySettings>()
                .cycle_shadow_map_size();
            queue_machine_settings_save(world);
            set_editor_status(world, format!("Directional shadow map: {size}px"));
        }
        EditorAction::ToggleForgeContactShadows => {
            let enabled = {
                let mut quality = world.resource_mut::<RenderQualitySettings>();
                quality.forge_contact_shadows = !quality.forge_contact_shadows;
                quality.forge_contact_shadows
            };
            let cameras = world
                .query_filtered::<Entity, With<EditorCamera>>()
                .iter(world)
                .collect::<Vec<_>>();
            for camera in cameras {
                if enabled {
                    world.entity_mut(camera).insert((
                        bevy::camera::Hdr,
                        Msaa::Off,
                        bevy::pbr::ContactShadows::default(),
                    ));
                } else {
                    world
                        .entity_mut(camera)
                        .remove::<bevy::pbr::ContactShadows>();
                }
            }
            queue_machine_settings_save(world);
            set_editor_status(world, format!("Forge contact shadows: {}", on_off(enabled)));
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
                deactivate_forge_text_fields(world);
                {
                    let mut registry = world.resource_mut::<EditorRegistryState>();
                    registry.rename_buffer = content_id.clone();
                    registry.rename_active = true;
                }
                focus_forge_text_field(world, ForgeTextField::ContentId, &content_id);
                set_editor_status(
                    world,
                    "Edit the content ID; Enter/A accepts and Escape/B cancels",
                );
            } else {
                set_editor_status(world, "No registry record selected");
            }
        }
        EditorAction::EditProjectName => {
            let value = world
                .resource::<EditorProjectSession>()
                .project
                .display_name
                .clone();
            deactivate_forge_text_fields(world);
            {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.project_name_buffer = value.clone();
                registry.project_name_active = true;
            }
            focus_forge_text_field(world, ForgeTextField::ProjectName, &value);
            set_editor_status(
                world,
                "Edit project name; Enter/A accepts and Escape/B cancels",
            );
        }
        EditorAction::EditLevelName => {
            let value = world
                .resource::<EditorProjectSession>()
                .project
                .active_level()
                .map(|level| level.display_name.clone());
            let Some(value) = value else {
                set_editor_status(world, "No active level to rename");
                return;
            };
            deactivate_forge_text_fields(world);
            {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.level_name_buffer = value.clone();
                registry.level_name_active = true;
            }
            focus_forge_text_field(world, ForgeTextField::LevelName, &value);
            set_editor_status(
                world,
                "Edit level name; Enter/A accepts and Escape/B cancels",
            );
        }
        EditorAction::CreateDialogueGraph => {
            let result = dialogue_records::create_dialogue(
                &mut world.resource_mut::<EditorProjectSession>().project,
                "New Dialogue Sequence",
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
                    set_editor_status(
                        world,
                        format!("Created modular dialogue graph {content_id}"),
                    );
                }
                Err(error) => set_editor_status(world, format!("Create rejected: {error:?}")),
            }
        }
        EditorAction::DialogueCycleMode => {
            let Ok((content_id, before)) = selected_dialogue_graph(world) else {
                set_editor_status(world, "Select a Dialogue Forge record first");
                return;
            };
            let mut after = before.clone();
            after.mode = match after.mode {
                dialogue_records::DialoguePlaybackMode::Gameplay => {
                    after.pause_gameplay = true;
                    dialogue_records::DialoguePlaybackMode::Cutscene
                }
                dialogue_records::DialoguePlaybackMode::Cutscene => {
                    after.pause_gameplay = false;
                    dialogue_records::DialoguePlaybackMode::Gameplay
                }
            };
            let mode = after.mode;
            queue_dialogue_graph_edit(world, content_id, before, after, "Change dialogue mode");
            set_editor_status(world, format!("Dialogue playback mode: {mode:?}"));
        }
        EditorAction::DialogueAddNode => {
            let Ok((content_id, before)) = selected_dialogue_graph(world) else {
                set_editor_status(world, "Select a Dialogue Forge record first");
                return;
            };
            if before.nodes.len() >= dialogue_records::MAX_DIALOGUE_NODES {
                set_editor_status(world, "Dialogue node limit reached");
                return;
            }
            let mut after = before.clone();
            let node_id = (after.nodes.len() + 1..)
                .map(|number| format!("line_{number}"))
                .find(|candidate| !after.nodes.iter().any(|node| node.node_id == *candidate))
                .expect("unbounded numbering finds a dialogue node id");
            if let Some(previous) = after.nodes.last_mut() {
                if previous.next_node.is_none() && previous.choices.is_empty() {
                    previous.next_node = Some(node_id.clone());
                }
            }
            after.nodes.push(dialogue_records::DialogueNode::speech(
                &node_id,
                "Speaker",
                "New dialogue line.",
            ));
            queue_dialogue_graph_edit(world, content_id, before, after, "Add dialogue node");
            set_editor_status(world, format!("Added modular dialogue node {node_id}"));
        }
        EditorAction::DialogueAddAnimationCue => {
            let Ok((content_id, before)) = selected_dialogue_graph(world) else {
                set_editor_status(world, "Select a Dialogue Forge record first");
                return;
            };
            let mut after = before.clone();
            let Some(node) = after.nodes.last_mut() else {
                set_editor_status(world, "Add a dialogue node first");
                return;
            };
            if node.timeline.len() >= dialogue_records::MAX_TIMELINE_CUES {
                set_editor_status(world, "Timeline cue limit reached");
                return;
            }
            node.timeline.push(dialogue_records::TimelineCue {
                at_seconds: node.minimum_hold_seconds.max(0.0),
                action: dialogue_records::TimelineAction::PlayAnimation {
                    actor: node.speaker.clone(),
                    clip: "talk_gesture".into(),
                    speed: 1.0,
                    looping: false,
                    blend_seconds: 0.15,
                },
            });
            let node_id = node.node_id.clone();
            queue_dialogue_graph_edit(world, content_id, before, after, "Add animation cue");
            set_editor_status(
                world,
                format!("Added gameplay/cutscene animation cue to {node_id}"),
            );
        }
        EditorAction::DialogueRecordVoice => toggle_dialogue_voice_recording(world),
        EditorAction::ValidateProject => {
            let issues = {
                let session = world.resource::<EditorProjectSession>();
                collect_project_diagnostics(session)
            };
            let issue_count = issues.len();
            let preview = issues
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" • ");
            let mut cache = world.resource_mut::<EditorValidationCache>();
            cache.initialized = true;
            cache.issues = issues;
            if issue_count == 0 {
                set_editor_status(world, "Project validation passed with no errors");
            } else {
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
            let (level_id, active_scene) = {
                let mut session = world.resource_mut::<EditorProjectSession>();
                session.project.scene = scene;
                let level_id = session.project.create_level(template);
                (level_id, session.project.scene.clone())
            };
            world.resource_mut::<EditorSelection>().clear();
            world.resource_mut::<EditorUndoStack>().clear();
            respawn_scene_objects(world, &active_scene.objects);
            apply_scene_adapter_overrides(world, &active_scene);
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
                            session.project.scene.clone(),
                        )),
                        Err(_) => None,
                    }
                }
            };
            let Some((name, active_scene)) = switch else {
                set_editor_status(world, "This project has only one level");
                return;
            };
            world.resource_mut::<EditorSelection>().clear();
            world.resource_mut::<EditorUndoStack>().clear();
            respawn_scene_objects(world, &active_scene.objects);
            apply_scene_adapter_overrides(world, &active_scene);
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
                        session.project.scene.clone(),
                    )
                })
            };
            world.resource_mut::<EditorLevelDeleteArm>().0 = None;
            match result {
                Ok((deleted_name, next_name, active_scene)) => {
                    world.resource_mut::<EditorSelection>().clear();
                    world.resource_mut::<EditorUndoStack>().clear();
                    respawn_scene_objects(world, &active_scene.objects);
                    apply_scene_adapter_overrides(world, &active_scene);
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
            let switch: Result<Option<EditorSceneDraft>, String> = {
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
                        .map(|()| Some(session.project.scene.clone()))
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
            if let Some(active_scene) = switch {
                world.resource_mut::<EditorSelection>().clear();
                world.resource_mut::<EditorUndoStack>().clear();
                respawn_scene_objects(world, &active_scene.objects);
                apply_scene_adapter_overrides(world, &active_scene);
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
            let value = world.resource::<EditorRegistryState>().search_text.clone();
            deactivate_forge_text_fields(world);
            {
                let mut registry = world.resource_mut::<EditorRegistryState>();
                registry.search_active = true;
            }
            focus_forge_text_field(world, ForgeTextField::RegistrySearch, &value);
            set_editor_status(
                world,
                "Type a record ID, name, or category; Enter/A applies, Escape/B clears",
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
            EditorPrimitive::Ladder => Self::Ladder,
            EditorPrimitive::Stairs => Self::Stairs,
            EditorPrimitive::Tower => Self::Tower,
            EditorPrimitive::Castle => Self::Castle,
            EditorPrimitive::FloatingIsland => Self::FloatingIsland,
            EditorPrimitive::MovingPlatform => Self::MovingPlatform,
            EditorPrimitive::SpringPlatform => Self::SpringPlatform,
            EditorPrimitive::FlipPanel => Self::FlipPanel,
            EditorPrimitive::RotatingBridge => Self::RotatingBridge,
            EditorPrimitive::CollapseBridge => Self::CollapseBridge,
            EditorPrimitive::SpikeBridge => Self::SpikeBridge,
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
            DraftPrimitive::Ladder => Self::Ladder,
            DraftPrimitive::Stairs => Self::Stairs,
            DraftPrimitive::Tower => Self::Tower,
            DraftPrimitive::Castle => Self::Castle,
            DraftPrimitive::FloatingIsland => Self::FloatingIsland,
            DraftPrimitive::MovingPlatform => Self::MovingPlatform,
            DraftPrimitive::SpringPlatform => Self::SpringPlatform,
            DraftPrimitive::FlipPanel => Self::FlipPanel,
            DraftPrimitive::RotatingBridge => Self::RotatingBridge,
            DraftPrimitive::CollapseBridge => Self::CollapseBridge,
            DraftPrimitive::SpikeBridge => Self::SpikeBridge,
        }
    }
}

fn editor_platformer_kind(
    primitive: EditorPrimitive,
) -> Option<platformer_prefabs::PlatformerPrefabKind> {
    use platformer_prefabs::PlatformerPrefabKind as Kind;
    Some(match primitive {
        EditorPrimitive::Ladder => Kind::Ladder,
        EditorPrimitive::Stairs => Kind::Stairs,
        EditorPrimitive::Tower => Kind::Tower,
        EditorPrimitive::Castle => Kind::Castle,
        EditorPrimitive::FloatingIsland => Kind::FloatingIsland,
        EditorPrimitive::MovingPlatform => Kind::MovingPlatform,
        EditorPrimitive::SpringPlatform => Kind::SpringPlatform,
        EditorPrimitive::FlipPanel => Kind::FlipPanel,
        EditorPrimitive::RotatingBridge => Kind::RotatingBridge,
        EditorPrimitive::CollapseBridge => Kind::CollapseBridge,
        EditorPrimitive::SpikeBridge => Kind::SpikeBridge,
        _ => return None,
    })
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

/// Apply the active scene's stable world-adapter transforms after that scene
/// becomes the editor workspace. Adapter entities are part of the generated
/// gameplay world rather than `EditorPrimitive`, so respawning scene objects
/// alone cannot restore their authored positions.
fn apply_scene_adapter_overrides(world: &mut World, scene: &EditorSceneDraft) -> usize {
    let overrides = scene
        .adapter_overrides
        .iter()
        .map(|draft| (draft.adapter_key.as_str(), draft.transform))
        .collect::<BTreeMap<_, _>>();
    let adapters = world
        .query::<(
            Entity,
            &Transform,
            &EditorAccess,
            Option<&WorldAnchor>,
            Option<&DungeonCrawlGate>,
            Option<&SettlementBuildTerminal>,
            Option<&WorldRouteMarker>,
        )>()
        .iter(world)
        .filter_map(
            |(entity, transform, access, anchor, gate, terminal, route)| {
                if *access != EditorAccess::Editable {
                    return None;
                }
                Some((
                    entity,
                    adapter_key(anchor, gate, terminal, route)?,
                    *transform,
                ))
            },
        )
        .collect::<Vec<_>>();
    {
        let mut baselines = world.resource_mut::<EditorAdapterBaselines>();
        for (entity, key, transform) in &adapters {
            let baseline = baselines
                .0
                .entry(key.clone())
                .or_insert((*entity, *transform));
            // App-state world teardown may later create a new runtime adapter
            // with the same stable key. Its new entity owns a new generated
            // baseline; retaining the prior world's transform would leak
            // authored state across world lifetimes.
            if baseline.0 != *entity {
                *baseline = (*entity, *transform);
            }
        }
    }
    let baselines = world.resource::<EditorAdapterBaselines>().0.clone();
    let mut applied = BTreeSet::new();
    for (entity, key, current) in adapters {
        let target = overrides
            .get(key.as_str())
            .copied()
            .map(Transform::from)
            .or_else(|| baselines.get(&key).map(|(_, transform)| *transform));
        if let Some(target) = target {
            // Mirror the same translation semantics as an interactive adapter
            // drag. These gameplay coordinates are stored separately from the
            // entity Transform and must move with a restored scene override.
            // Aligning `last_translation` below prevents `sync_world_adapters`
            // from applying this delta a second time on the next update.
            let coupled_translation = world
                .get::<EditorWorldAdapter>(entity)
                .map(|adapter| adapter.last_translation)
                .unwrap_or(current.translation);
            let delta = target.translation - coupled_translation;
            if delta != Vec3::ZERO {
                if let Some(mut gate) = world.get_mut::<DungeonCrawlGate>(entity) {
                    gate.entry += delta;
                    gate.focus += delta;
                }
                if let Some(mut terminal) = world.get_mut::<SettlementBuildTerminal>(entity) {
                    terminal.origin += delta;
                }
            }
            *world
                .get_mut::<Transform>(entity)
                .expect("queried adapter transform should still exist") = target;
            if let Some(mut adapter) = world.get_mut::<EditorWorldAdapter>(entity) {
                adapter.last_translation = target.translation;
            }
        }
        if overrides.contains_key(key.as_str()) {
            applied.insert(key);
        }
    }
    scene.adapter_overrides.len().saturating_sub(applied.len())
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
    if publish {
        return publish_editor_project(world);
    }
    let scene = collect_working_scene(world);
    let (store, loaded, recovery_write_authorized, observed_revision) = {
        let session = world.resource::<EditorProjectSession>();
        (
            session.store.clone(),
            session.loaded,
            session.recovery_write_authorized,
            session.primary_revision.clone(),
        )
    };
    let path = store.path().display().to_string();
    if !loaded || (!recovery_write_authorized && observed_revision.is_none()) {
        set_editor_status(
            world,
            "Save blocked: no writable primary project is loaded; load a valid primary project first",
        );
        return false;
    }
    let result = {
        let mut session = world.resource_mut::<EditorProjectSession>();
        session.project.scene = scene;
        session.project.synchronize_authored_dependencies();
        let before_save = session.project.clone();
        match store.compare_and_save_revision(&mut session.project, observed_revision.as_deref()) {
            Ok(revision) => Ok(revision),
            Err(error) => {
                session.project = before_save;
                Err(error)
            }
        }
    };
    match result {
        Ok(primary_revision) => {
            world
                .resource_mut::<EditorProjectSession>()
                .primary_revision = Some(primary_revision);
            world
                .resource_mut::<EditorProjectSession>()
                .recovery_write_authorized = false;
            let mut document = world.resource_mut::<EditorDocumentState>();
            document.dirty = false;
            document.exit_armed = false;
            set_editor_status(world, format!("Project saved atomically to {path}"));
            true
        }
        Err(ProjectIoError::RevisionConflict { .. }) => {
            set_editor_status(
                world,
                "Save blocked: the project changed on disk; reload and reconcile the newer revision first",
            );
            false
        }
        Err(error) => {
            set_editor_status(world, format!("Save failed: {error}"));
            false
        }
    }
}

fn publish_editor_project(world: &mut World) -> bool {
    publish_editor_project_to(world, &publish::published_dir())
}

fn publish_editor_project_to(world: &mut World, output_dir: &std::path::Path) -> bool {
    let scene = collect_working_scene(world);
    let (store, loaded, observed_revision) = {
        let session = world.resource::<EditorProjectSession>();
        (
            session.store.clone(),
            session.loaded,
            session.primary_revision.clone(),
        )
    };
    let Some(observed_revision) = loaded.then_some(observed_revision).flatten() else {
        set_editor_status(
            world,
            "Publish blocked: no writable primary project is loaded; load a valid primary project first",
        );
        return false;
    };
    {
        let mut session = world.resource_mut::<EditorProjectSession>();
        session.project.scene = scene;
        session.project.synchronize_authored_dependencies();
        let before_save = session.project.clone();
        if let Err(error) = store.compare_and_save(&mut session.project, &observed_revision) {
            session.project = before_save;
            let message = match error {
                ProjectIoError::RevisionConflict { .. } => {
                    "Publish blocked: the project changed on disk; reload and reconcile the newer revision first".to_owned()
                }
                error => format!("Publish save failed: {error}"),
            };
            set_editor_status(world, message);
            return false;
        }
    }

    publish_saved_editor_project(world, &store, output_dir)
}

fn publish_saved_editor_project(
    world: &mut World,
    store: &ProjectStore,
    output_dir: &std::path::Path,
) -> bool {
    match publish::publish_store_to(store, output_dir) {
        Ok(report) => {
            let (project, source, revision) = match store.load_with_recovery_and_revision() {
                Ok(loaded) => loaded,
                Err(error) => {
                    let mut session = world.resource_mut::<EditorProjectSession>();
                    session.loaded = false;
                    session.recovery_write_authorized = false;
                    session.primary_revision = None;
                    set_editor_status(
                        world,
                        format!("Publish committed, but editor reconciliation failed: {error}"),
                    );
                    return false;
                }
            };
            let writable_revision = match source {
                ProjectLoadSource::Primary => revision,
                ProjectLoadSource::Recovery(_) => None,
            };
            rebuild_published_content_catalogs(world, &project);
            {
                let mut session = world.resource_mut::<EditorProjectSession>();
                session.project = project;
                session.store = store.clone();
                session.loaded = true;
                session.recovery_write_authorized = false;
                session.primary_revision = writable_revision;
            }
            let mut document = world.resource_mut::<EditorDocumentState>();
            document.dirty = false;
            document.exit_armed = false;
            set_editor_status(world, report.summary());
            true
        }
        Err(error) => {
            // The publisher rolls project hashes and the generation pointer
            // back on failure. Reload that authoritative snapshot so this
            // session cannot retain optimistic published claims.
            match store.load_with_recovery_and_revision() {
                Ok((project, source, revision)) => {
                    let mut session = world.resource_mut::<EditorProjectSession>();
                    session.project = project;
                    session.loaded = true;
                    session.recovery_write_authorized = false;
                    session.primary_revision = match source {
                        ProjectLoadSource::Primary => revision,
                        ProjectLoadSource::Recovery(_) => None,
                    };
                    world.resource_mut::<EditorDocumentState>().dirty = false;
                }
                Err(_) => {
                    let mut session = world.resource_mut::<EditorProjectSession>();
                    session.loaded = false;
                    session.recovery_write_authorized = false;
                    session.primary_revision = None;
                }
            }
            set_editor_status(world, format!("Publish failed: {error}"));
            false
        }
    }
}

fn load_editor_project(world: &mut World, recovery_only: bool) {
    let store = world.resource::<EditorProjectSession>().store.clone();
    let result = if recovery_only {
        store
            .load_recovery_with_revision(1)
            .map(|(project, revision)| (project, ProjectLoadSource::Recovery(1), revision))
    } else {
        store.load_with_recovery_and_revision()
    };
    let (project, source, revision) = match result {
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
    let missing_adapters = apply_scene_adapter_overrides(world, &project.scene);
    {
        let mut session = world.resource_mut::<EditorProjectSession>();
        session.project = project;
        session.loaded = true;
        session.recovery_write_authorized = recovery_only;
        session.primary_revision = match (recovery_only, source) {
            (true, _) | (false, ProjectLoadSource::Primary) => revision,
            (false, ProjectLoadSource::Recovery(_)) => None,
        };
    }
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
    if let Some(kind) = editor_platformer_kind(primitive) {
        return Some(platformer_prefabs::PlatformerPrefabDesign::stock(kind).mesh());
    }
    match primitive {
        EditorPrimitive::Empty => None,
        EditorPrimitive::Cube => Some(Cuboid::new(2.5, 2.5, 2.5).into()),
        EditorPrimitive::Pillar => Some(Cylinder::new(1.15, 5.0).into()),
        EditorPrimitive::Beacon => Some(Sphere::new(1.0).into()),
        _ => unreachable!("platformer prefabs returned above"),
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
        match crate::character::mesh_modifiers::apply_stack_to_mesh(&base, &stack) {
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
    if let Some(kind) = editor_platformer_kind(primitive) {
        return kind.label();
    }
    match primitive {
        EditorPrimitive::Empty => "Anchor",
        EditorPrimitive::Cube => "Block",
        EditorPrimitive::Pillar => "Pillar",
        EditorPrimitive::Beacon => "Beacon",
        _ => unreachable!("platformer prefabs returned above"),
    }
}

fn spawn_editor_primitive_record(
    world: &mut World,
    id: EditorEntityId,
    primitive: EditorPrimitive,
    name: String,
    transform: Transform,
) -> Entity {
    let prefab =
        editor_platformer_kind(primitive).map(platformer_prefabs::PlatformerPrefabDesign::stock);
    let (radius, mesh, color, collider) = match primitive {
        EditorPrimitive::Empty => (0.55, None, Color::WHITE, None),
        EditorPrimitive::Cube => (
            1.75,
            Some(
                world
                    .resource_mut::<Assets<Mesh>>()
                    .add(Cuboid::new(2.5, 2.5, 2.5)),
            ),
            primitive_standard_color(primitive),
            None,
        ),
        EditorPrimitive::Pillar => (
            2.7,
            Some(
                world
                    .resource_mut::<Assets<Mesh>>()
                    .add(Cylinder::new(1.15, 5.0)),
            ),
            primitive_standard_color(primitive),
            None,
        ),
        EditorPrimitive::Beacon => (
            1.4,
            Some(world.resource_mut::<Assets<Mesh>>().add(Sphere::new(1.0))),
            primitive_standard_color(primitive),
            None,
        ),
        _ => {
            let design = prefab.expect("non-core primitive has a prefab recipe");
            let generated = design.mesh();
            let (vertices, triangles) = design.collider_geometry();
            let collider =
                crate::engine::physics::prelude::Collider::trimesh(vertices, triangles).ok();
            (
                design.selection_radius(),
                Some(world.resource_mut::<Assets<Mesh>>().add(generated)),
                primitive_standard_color(primitive),
                collider,
            )
        }
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
    let entity = entity.id();
    if let (Some(design), Some(collider)) = (prefab, collider) {
        insert_platformer_runtime_adapter(world, entity, design, transform, collider);
    }
    entity
}

/// Compile a Forge prefab recipe into the existing gameplay traversal
/// vocabulary. Reusing `MovingPlatform`, `RotatingElevator`, and
/// `SpringJumpPad` keeps player carrying, co-op cooldowns, and launch-state
/// transitions authoritative in WorldPlugin rather than cloning those rules
/// into the editor.
fn insert_platformer_runtime_adapter(
    world: &mut World,
    entity: Entity,
    design: platformer_prefabs::PlatformerPrefabDesign,
    transform: Transform,
    collider: crate::engine::physics::prelude::Collider,
) {
    use crate::engine::physics::prelude::RigidBody;
    use platformer_prefabs::PlatformerBehavior;

    let mut target = world.entity_mut(entity);
    target.insert((collider, WalkableSurface));
    match design.kind.behavior() {
        PlatformerBehavior::Oscillate { distance, seconds } => {
            let half_travel = transform.rotation * Vec3::X * (distance * 0.5);
            target.insert((
                RigidBody::KinematicPositionBased,
                MovingPlatform {
                    start: transform.translation - half_travel,
                    end: transform.translation + half_travel,
                    speed: std::f32::consts::TAU * distance / seconds.max(0.1),
                    phase: 0.0,
                    size: design.size * transform.scale.abs(),
                },
            ));
        }
        PlatformerBehavior::Bounce { launch_speed } => {
            target.insert((
                RigidBody::Fixed,
                SpringJumpPad {
                    launch_velocity: Vec3::Y * launch_speed,
                    radius: design.size.x.min(design.size.z) * 0.48,
                    cooldown: 0.22,
                    cooldown_timers: [0.0; 4],
                    force_hoverboard: false,
                },
            ));
        }
        PlatformerBehavior::Rotate { degrees_per_second } => {
            target.insert((
                RigidBody::KinematicPositionBased,
                RotatingElevator {
                    center: transform.translation,
                    radius: 0.0,
                    angular_speed: degrees_per_second.to_radians(),
                    vertical_amplitude: 0.0,
                    vertical_speed: 0.0,
                    phase: 0.0,
                    size: design.size * transform.scale.abs(),
                },
            ));
        }
        PlatformerBehavior::FlipOnJump => {
            target.insert((
                RigidBody::KinematicPositionBased,
                FlipPlatform {
                    base_rotation: transform.rotation,
                    flipped: false,
                    progress: 0.0,
                    turn_seconds: 0.28,
                    trigger_radius: design.size.x.max(design.size.z) * 0.8,
                    cooldown: 0.32,
                    cooldown_timer: 0.0,
                },
            ));
        }
        PlatformerBehavior::Collapse { warning_seconds } => {
            target.insert((
                RigidBody::KinematicPositionBased,
                CollapsePlatform {
                    origin: transform.translation,
                    size: design.size * transform.scale.abs(),
                    warning_seconds,
                    fallen_seconds: 1.8,
                    drop_distance: 7.0,
                    timer: 0.0,
                    state: CollapsePlatformState::Armed,
                },
            ));
        }
        PlatformerBehavior::Hazard => {
            target.insert((
                RigidBody::Fixed,
                SpikePlatformHazard {
                    size: design.size * transform.scale.abs(),
                    damage: 22.0,
                    cooldown: 0.8,
                    cooldown_timers: [0.0; 4],
                },
            ));
        }
        PlatformerBehavior::StaticTraversal | PlatformerBehavior::Climbable => {
            target.insert(RigidBody::Fixed);
        }
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
    let authored_name = world
        .get::<Name>(entity)
        .map(|name| name.as_str().to_owned());
    let modifier_stack = world.get::<EditorModifierStack>(entity).cloned();
    transform.translation.x += 1.0;
    let id = world.resource_mut::<EditorIdAllocator>().allocate();
    let mut duplicate = world.spawn((
        Authorable,
        access,
        bounds,
        primitive,
        id,
        Name::new(authored_name.unwrap_or_else(|| format!("{:?} {}", primitive, id.0))),
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
    if let Some(modifier_stack) = modifier_stack {
        duplicate.insert(modifier_stack);
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

fn deactivate_forge_text_fields(world: &mut World) {
    world.resource_mut::<InputFocus>().clear();
    world.resource_mut::<EditorOutlinerFilter>().active = false;
    let mut registry = world.resource_mut::<EditorRegistryState>();
    registry.rename_active = false;
    registry.search_active = false;
    registry.project_name_active = false;
    registry.level_name_active = false;
}

fn focus_forge_text_field(world: &mut World, kind: ForgeTextField, value: &str) -> bool {
    let entity = world
        .query::<(Entity, &ForgeTextFieldMarker)>()
        .iter(world)
        .find_map(|(entity, field)| (field.kind == kind).then_some(entity));
    let Some(entity) = entity else {
        return false;
    };
    if let Some(mut input) = world.get_mut::<EditableText>(entity) {
        input.editor_mut().set_text(value);
    }
    world
        .resource_mut::<InputFocus>()
        .set(entity, FocusCause::Navigated);
    true
}

fn queue_machine_settings_save(world: &mut World) {
    world
        .commands()
        .queue(SaveSettingsDeferred(Duration::from_millis(250)));
}

fn on_off(enabled: bool) -> &'static str {
    if enabled {
        "ON"
    } else {
        "OFF"
    }
}

fn collect_project_diagnostics(session: &EditorProjectSession) -> Vec<String> {
    validate_project(&session.project)
        .into_iter()
        .map(|error| format!("{error:?}"))
        .chain(dialogue_records::validate_dialogue_records(
            &session.project,
        ))
        .chain(session.store.source_diagnostics(&session.project))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn update_editor_workspace_text(
    selection: Res<EditorSelection>,
    status: Res<EditorRuntimeStatus>,
    document: Res<EditorDocumentState>,
    filter: Res<EditorOutlinerFilter>,
    camera_rig: Res<EditorCameraRig>,
    gizmo: Res<EditorGizmoSettings>,
    editor_preferences: Res<EditorPreferences>,
    render_quality: Res<RenderQualitySettings>,
    session: Res<EditorProjectSession>,
    catalog: Res<PublishedMaterialCatalog>,
    procedural_catalog: Res<PublishedProceduralRecipeCatalog>,
    sandbox: Res<WorldKitSandboxState>,
    registry: Res<EditorRegistryState>,
    mut validation_cache: ResMut<EditorValidationCache>,
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
        Query<&mut Text, With<EditorSettingsText>>,
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
    for mut text in &mut text_queries.p4() {
        *text = Text::new(format!(
            "\nMACHINE / VIEWPORT\nGrid: {} • Labels: {}\nLabel range: {:.0}m • budget {}\nShadows: {} • {}px\nForge contact shadows: {}",
            on_off(editor_preferences.infinite_grid_visible),
            on_off(editor_preferences.semantic_labels_visible),
            editor_preferences.semantic_label_distance,
            editor_preferences.semantic_label_budget,
            on_off(render_quality.shadow_maps_enabled),
            render_quality.directional_shadow_map_size,
            on_off(render_quality.forge_contact_shadows),
        ));
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
                Some(persistence::ContentPayload::Ui(_)) => {
                    dialogue_records::load_dialogue(&session.project, &record.content_id)
                        .map(|graph| {
                            let cues = graph
                                .nodes
                                .iter()
                                .map(|node| node.timeline.len())
                                .sum::<usize>();
                            let voiced = graph
                                .nodes
                                .iter()
                                .filter(|node| node.voice.is_some())
                                .count();
                            let selected_node = graph.nodes.last().map_or("—", |node| {
                                node.node_id.as_str()
                            });
                            format!(
                                "\nDIALOGUE FORGE: {:?} • {}\nNodes: {} • cues: {} • voiced: {}\nActive authoring node: {}\nSkippable: {} • pauses gameplay: {}",
                                graph.mode,
                                graph.entry_node,
                                graph.nodes.len(),
                                cues,
                                voiced,
                                selected_node,
                                on_off(graph.skippable),
                                on_off(graph.pause_gameplay),
                            )
                        })
                        .unwrap_or_default()
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
    if !validation_cache.initialized || session.is_changed() || document.is_changed() {
        validation_cache.issues = collect_project_diagnostics(&session);
        validation_cache.initialized = true;
    }
    let validation_count = validation_cache.issues.len();
    let validation = if validation_count == 0 {
        "VALIDATION: PASS".to_owned()
    } else {
        let details = validation_cache
            .issues
            .iter()
            .map(|diagnostic| format!("• {diagnostic}"))
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
        let focused = button.control == focus.0;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SemanticViewportLabelKind {
    Socket,
    Spawn,
    Route,
    WaterFlow,
    ModeBoundary,
}

impl SemanticViewportLabelKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Socket => "SOCKET",
            Self::Spawn => "SPAWN",
            Self::Route => "ROUTE",
            Self::WaterFlow => "FLOW",
            Self::ModeBoundary => "MODE",
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Socket => Color::srgb(1.0, 0.48, 0.12),
            Self::Spawn => Color::srgb(0.30, 1.0, 0.58),
            Self::Route => Color::srgb(1.0, 0.24, 0.82),
            Self::WaterFlow => Color::srgb(0.16, 0.86, 1.0),
            Self::ModeBoundary => Color::srgb(1.0, 0.78, 0.18),
        }
    }
}

#[derive(Debug)]
struct SemanticViewportLabel {
    kind: SemanticViewportLabelKind,
    text: String,
    position: Vec3,
    direction: Option<Vec3>,
}

fn semantic_label_text(kind: SemanticViewportLabelKind, value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    format!("{}: {}", kind.prefix(), value.trim())
}

fn authorable_semantic_kind(kind: EditorAdapterKind) -> Option<SemanticViewportLabelKind> {
    match kind {
        EditorAdapterKind::WorldAnchor => Some(SemanticViewportLabelKind::Spawn),
        EditorAdapterKind::RouteMarker | EditorAdapterKind::RoadLoop => {
            Some(SemanticViewportLabelKind::Route)
        }
        EditorAdapterKind::CaveGate | EditorAdapterKind::Building => {
            Some(SemanticViewportLabelKind::ModeBoundary)
        }
        EditorAdapterKind::SettlementTerminal => None,
    }
}

fn topology_node_semantic_kind(kind: WorldTopologyNodeKind) -> Option<SemanticViewportLabelKind> {
    match kind {
        WorldTopologyNodeKind::FastTravel => Some(SemanticViewportLabelKind::Spawn),
        WorldTopologyNodeKind::Entrance | WorldTopologyNodeKind::Zone => {
            Some(SemanticViewportLabelKind::ModeBoundary)
        }
        WorldTopologyNodeKind::Room | WorldTopologyNodeKind::Landmark => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_semantic_editor_labels(
    mut gizmos: Gizmos,
    settings: Res<EditorPreferences>,
    session: Res<EditorProjectSession>,
    registry: Res<EditorRegistryState>,
    cameras: Query<&GlobalTransform, With<EditorCamera>>,
    sandbox_roots: Query<(&WorldKitGeneratedRoot, &GlobalTransform)>,
    authorables: Query<(&GlobalTransform, Option<&Name>, &EditorWorldAdapter), With<Authorable>>,
    waters: Query<(&GlobalTransform, &WaterBody)>,
) {
    if !settings.semantic_labels_visible {
        return;
    }
    let Ok(camera) = cameras.single() else {
        return;
    };
    let camera_position = camera.translation();
    let camera_rotation = camera.rotation();
    let mut labels = Vec::new();

    for (transform, name, adapter) in &authorables {
        let Some(kind) = authorable_semantic_kind(adapter.kind) else {
            continue;
        };
        let value = name.map(Name::as_str).unwrap_or("unnamed");
        labels.push(SemanticViewportLabel {
            kind,
            text: semantic_label_text(kind, value),
            position: transform.translation(),
            direction: None,
        });
    }

    if let Some(record) = session.project.records.get(registry.selected) {
        if let (Some(recipe), Some((_, root_transform))) = (
            session
                .project
                .payloads
                .get(&record.content_id)
                .and_then(persistence::ContentPayload::procedural_recipe),
            sandbox_roots
                .iter()
                .find(|(root, _)| root.content_id == record.content_id),
        ) {
            if record.category == persistence::ContentCategory::Road {
                for (index, point) in recipe.spline_points.iter().enumerate() {
                    let kind = SemanticViewportLabelKind::Route;
                    let local = Vec3::from_array(point.position);
                    let tangent = Vec3::from_array(persistence::resolved_road_tangent(
                        &recipe.spline_points,
                        index,
                    ))
                    .normalize_or_zero();
                    labels.push(SemanticViewportLabel {
                        kind,
                        text: semantic_label_text(kind, &point.point_id),
                        position: root_transform.transform_point(local),
                        direction: (tangent != Vec3::ZERO)
                            .then_some(root_transform.rotation() * tangent),
                    });
                }
            }

            for node in &recipe.topology_nodes {
                let Some(kind) = topology_node_semantic_kind(node.kind) else {
                    continue;
                };
                labels.push(SemanticViewportLabel {
                    kind,
                    text: semantic_label_text(kind, &node.node_id),
                    position: root_transform.transform_point(Vec3::from_array(node.position)),
                    direction: None,
                });
            }
            for socket in &recipe.topology_sockets {
                let Some(node) = recipe
                    .topology_nodes
                    .iter()
                    .find(|node| node.node_id == socket.node_id)
                else {
                    continue;
                };
                let kind = SemanticViewportLabelKind::Socket;
                let local =
                    Vec3::from_array(node.position) + Vec3::from_array(socket.local_position);
                let facing = Vec3::from_array(socket.facing).normalize_or_zero();
                labels.push(SemanticViewportLabel {
                    kind,
                    text: semantic_label_text(
                        kind,
                        &format!("{} {:?}", socket.socket_id, socket.kind),
                    ),
                    position: root_transform.transform_point(local),
                    direction: (facing != Vec3::ZERO).then_some(root_transform.rotation() * facing),
                });
            }
        }
    }

    for (transform, water) in &waters {
        let direction = match water.kind {
            WaterBodyKind::River => transform.rotation() * Vec3::Z,
            WaterBodyKind::Waterfall => Vec3::NEG_Y,
            WaterBodyKind::Ocean | WaterBodyKind::Lake => continue,
        }
        .normalize_or_zero();
        let kind = SemanticViewportLabelKind::WaterFlow;
        labels.push(SemanticViewportLabel {
            kind,
            text: semantic_label_text(kind, &format!("{:?}", water.kind)),
            position: transform.translation(),
            direction: (direction != Vec3::ZERO).then_some(direction),
        });
    }

    labels.retain(|label| {
        label.position.distance_squared(camera_position) <= settings.semantic_label_distance.powi(2)
    });
    labels.sort_by(|left, right| {
        left.position
            .distance_squared(camera_position)
            .total_cmp(&right.position.distance_squared(camera_position))
    });
    labels.truncate(settings.semantic_label_budget as usize);

    for label in labels {
        let distance = label.position.distance(camera_position);
        let font_size = (distance * 0.025).clamp(0.32, 3.0);
        let lift = (font_size * 1.25).max(0.55);
        let text_position = label.position + Vec3::Y * lift;
        let color = label.kind.color();
        gizmos.line(label.position, text_position, color);
        if let Some(direction) = label.direction {
            let length = (font_size * 2.0).clamp(1.0, 8.0);
            gizmos.arrow(label.position, label.position + direction * length, color);
        }
        gizmos.text(
            Isometry3d::new(text_position, camera_rotation),
            &label.text,
            font_size,
            Vec2::new(-0.5, -0.5),
            color,
        );
    }
}

fn sync_machine_render_settings(
    quality: Res<RenderQualitySettings>,
    mode: Res<State<EngineToolMode>>,
    shadow_map: Option<ResMut<bevy::light::DirectionalLightShadowMap>>,
    mut lights: Query<(Entity, &mut DirectionalLight)>,
) {
    if let Some(mut shadow_map) = shadow_map {
        let requested_size = quality.directional_shadow_map_size as usize;
        if shadow_map.size != requested_size {
            shadow_map.size = requested_size;
        }
    }

    // Contact shadows are deliberately restricted to one key light. This is
    // especially important before the effect is considered for four cameras.
    let key_light =
        (quality.forge_contact_shadows && *mode.get() == EngineToolMode::Editing).then(|| {
            lights
                .iter()
                .max_by(|(_, left), (_, right)| left.illuminance.total_cmp(&right.illuminance))
                .map(|(entity, _)| entity)
        });
    let key_light = key_light.flatten();
    for (entity, mut light) in &mut lights {
        light.shadow_maps_enabled = quality.shadow_maps_enabled;
        light.contact_shadows_enabled = Some(entity) == key_light;
    }
}

fn draw_editor_gizmos(
    mut gizmos: Gizmos,
    selection: Res<EditorSelection>,
    settings: Res<EditorGizmoSettings>,
    editor_preferences: Res<EditorPreferences>,
    session: Res<EditorProjectSession>,
    registry: Res<EditorRegistryState>,
    infinite_grids: Query<(), With<EditorInfiniteGrid>>,
    bevy_gizmo: Option<Res<BevyTransformGizmoSettings>>,
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
    // Reduced/headless plugin harnesses keep the finite immediate-mode grid.
    // Production Forge uses Bevy's anti-aliased, distance-fading grid shader.
    if editor_preferences.infinite_grid_visible && infinite_grids.is_empty() {
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
        if *access == EditorAccess::Editable && bevy_gizmo.is_none() {
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
    tool_window_pointer: Res<tool_windows::ToolWindowPointerState>,
    chrome_surfaces: Query<&RelativeCursorPosition, With<EditorChromeSurface>>,
    filter: Res<EditorOutlinerFilter>,
    text_capture: Res<EditorTextInputCapture>,
    mut rig: ResMut<EditorCameraRig>,
    mut cameras: Query<&mut Transform, With<EditorCamera>>,
) {
    let Ok(mut transform) = cameras.single_mut() else {
        return;
    };
    if filter.active || text_capture.0 {
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
    let editor_ui_captures_pointer = editor_ui_captures_pointer(
        tool_window_pointer.captures_viewport(),
        chrome_surfaces
            .iter()
            .map(RelativeCursorPosition::cursor_over),
    );
    let mouse_delta = if !editor_ui_captures_pointer && mouse_buttons.pressed(MouseButton::Right) {
        mouse_motion
            .read()
            .fold(Vec2::ZERO, |sum, event| sum + event.delta)
            * 0.003
    } else {
        mouse_motion.clear();
        Vec2::ZERO
    };
    let mouse_wheel_delta = if editor_ui_captures_pointer {
        // Message readers are independent; advance this reader even when the
        // hovered tool window owns wheel input so stale zoom cannot be applied
        // after the pointer returns to the viewport.
        mouse_wheel.clear();
        0.0
    } else {
        mouse_wheel.read().map(|event| event.y).sum::<f32>()
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
            rig.orbit_distance =
                (offset.length() - mouse_wheel_delta * 1.5 - local_move.z * speed * dt)
                    .clamp(1.5, 500.0);
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
        world.init_resource::<PublishedVehicleCatalog>();
        world.init_resource::<PublishedSpacecraftCatalog>();
        world.init_resource::<PublishedContentCatalogEpoch>();
        world.init_resource::<WorldKitPreviewState>();
        world.init_resource::<WorldKitSandboxState>();
        world.init_resource::<WorldKitSandboxAssets>();
        world.init_resource::<EditorSelection>();
        world.init_resource::<EditorIdAllocator>();
        world.init_resource::<EditorUndoStack>();
        world.init_resource::<EditorAdapterBaselines>();
        world.init_resource::<EditorDocumentState>();
        world.init_resource::<EditorValidationCache>();
        world.init_resource::<EditorRegistryState>();
        world.init_resource::<EditorOutlinerFilter>();
        world.init_resource::<InputFocus>();
        world.init_resource::<EditorGizmoSettings>();
        world.init_resource::<EditorPreferences>();
        world.init_resource::<RenderQualitySettings>();
        world.insert_resource(EditorRuntimeStatus {
            message: String::new(),
        });
        let store = ProjectStore::new(root.join("project.json"), 3);
        let mut project = ForgeProject::default();
        store
            .save(&mut project)
            .expect("test project should be initialized on disk");
        let primary_revision = store.primary_revision().ok();
        world.insert_resource(EditorProjectSession {
            project,
            loaded: true,
            recovery_write_authorized: false,
            primary_revision,
            store,
        });
        (world, root)
    }

    fn ancestor_editor_panel(world: &World, entity: Entity) -> Option<EditorPanelKind> {
        let mut current = entity;
        while let Some(parent) = world.get::<ChildOf>(current) {
            current = parent.parent();
            if let Some(panel) = world.get::<EditorPanelKind>(current) {
                return Some(*panel);
            }
        }
        None
    }

    #[test]
    fn workspace_controls_have_unique_reachable_ids_and_correct_panels() {
        let mut world = World::new();
        {
            let mut commands = world.commands();
            spawn_editor_workspace_ui(&mut commands);
        }
        world.flush();

        let buttons = world
            .query::<(Entity, &EditorButton)>()
            .iter(&world)
            .map(|(entity, button)| (entity, *button))
            .collect::<Vec<_>>();
        let unique = buttons
            .iter()
            .map(|(_, button)| button.control)
            .collect::<std::collections::HashSet<_>>();

        assert_eq!(buttons.len(), EDITOR_CONTROLS.len());
        assert_eq!(unique.len(), buttons.len(), "duplicate editor control ID");
        for spec in EDITOR_CONTROLS {
            let matches = buttons
                .iter()
                .filter(|(_, button)| button.control == spec.id)
                .collect::<Vec<_>>();
            assert_eq!(
                matches.len(),
                1,
                "unreachable editor control: {:?}",
                spec.id
            );
            let (entity, button) = matches[0];
            assert_eq!(button.panel, spec.panel);
            assert_eq!(ancestor_editor_panel(&world, *entity), spec.panel);
        }
    }

    #[test]
    fn controller_focus_uses_registry_order_wraps_and_skips_hidden_controls() {
        // ECS query order and duplicate sightings do not influence traversal;
        // only present, visible IDs survive in registry order.
        let controls = ordered_present_editor_controls([
            (EditorAction::TerrainProjectAll, true),
            (EditorAction::CreateCube, true),
            (EditorAction::Exit, true),
            (EditorAction::PresetCapture, false),
            (EditorAction::CreateCube, true),
        ]);
        assert_eq!(
            controls,
            vec![
                EditorAction::Exit,
                EditorAction::CreateCube,
                EditorAction::TerrainProjectAll,
            ]
        );

        assert_eq!(
            normalized_editor_focus(EditorAction::PresetCapture, &controls),
            Some(EditorAction::Exit)
        );
        assert_eq!(
            move_editor_focus(
                EditorAction::Exit,
                &controls,
                EditorFocusDirection::Previous,
            ),
            Some(EditorAction::TerrainProjectAll)
        );
        assert_eq!(
            move_editor_focus(
                EditorAction::TerrainProjectAll,
                &controls,
                EditorFocusDirection::Next,
            ),
            Some(EditorAction::Exit)
        );
        assert_eq!(
            focused_editor_action(EditorAction::CreateCube, &controls),
            Some(EditorAction::CreateCube)
        );
        assert_eq!(
            focused_editor_action(EditorAction::PresetCapture, &controls),
            None,
            "a hidden control must not be activated"
        );
    }

    #[test]
    fn pointer_activation_focuses_and_dispatches_the_same_control_id() {
        let mut app = App::new();
        app.init_resource::<EditorFocus>()
            .init_resource::<EditorPendingActions>()
            .add_systems(Update, editor_button_interactions);
        app.world_mut().spawn((
            Interaction::Pressed,
            EditorButton {
                control: EditorAction::SocketCreate,
                panel: Some(EditorPanelKind::Registry),
            },
        ));

        app.update();

        assert_eq!(
            app.world().resource::<EditorFocus>().0,
            EditorAction::SocketCreate
        );
        assert_eq!(
            app.world().resource::<EditorPendingActions>().0,
            vec![EditorAction::SocketCreate]
        );
    }

    #[test]
    fn editor_initial_focus_is_a_harmless_visible_workspace_control() {
        assert_eq!(EditorFocus::default().0, EditorAction::SelectNext);
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
    fn failed_undo_entry_is_dropped_so_older_history_remains_reachable() {
        let mut world = World::new();
        let older_id = EditorEntityId(21);
        let stale_id = EditorEntityId(22);
        let older_entity = world.spawn((older_id, Transform::default())).id();
        let stale_entity = world.spawn((stale_id, Transform::default())).id();
        let mut stack = EditorUndoStack::default();

        stack
            .execute(
                &mut world,
                EditorTransaction::transform(
                    "Move older object",
                    older_id,
                    Transform::default(),
                    Transform::from_xyz(3.0, 0.0, 0.0),
                ),
            )
            .unwrap();
        stack
            .execute(
                &mut world,
                EditorTransaction::transform(
                    "Move object from replaced document",
                    stale_id,
                    Transform::default(),
                    Transform::from_xyz(7.0, 0.0, 0.0),
                ),
            )
            .unwrap();
        world.despawn(stale_entity);

        assert_eq!(
            stack.undo(&mut world),
            Err(EditorCommandError::MissingEntity(stale_id))
        );
        assert_eq!(stack.undo_description(), Some("Move older object"));
        assert!(stack.undo(&mut world).unwrap());
        assert_eq!(
            world.get::<Transform>(older_entity).unwrap().translation,
            Vec3::ZERO
        );
        assert!(!stack.undo(&mut world).unwrap());
    }

    #[test]
    fn failed_redo_entry_is_dropped_so_older_redo_remains_reachable() {
        let mut world = World::new();
        let stale_id = EditorEntityId(31);
        let valid_id = EditorEntityId(32);
        let stale_entity = world.spawn((stale_id, Transform::default())).id();
        let valid_entity = world.spawn((valid_id, Transform::default())).id();
        let valid_after = Transform::from_xyz(5.0, 0.0, 0.0);
        let mut stack = EditorUndoStack::default();

        stack
            .execute(
                &mut world,
                EditorTransaction::transform(
                    "Redo from replaced document",
                    stale_id,
                    Transform::default(),
                    Transform::from_xyz(9.0, 0.0, 0.0),
                ),
            )
            .unwrap();
        stack
            .execute(
                &mut world,
                EditorTransaction::transform(
                    "Redo valid object",
                    valid_id,
                    Transform::default(),
                    valid_after,
                ),
            )
            .unwrap();
        assert!(stack.undo(&mut world).unwrap());
        assert!(stack.undo(&mut world).unwrap());
        world.despawn(stale_entity);

        assert_eq!(
            stack.redo(&mut world),
            Err(EditorCommandError::MissingEntity(stale_id))
        );
        assert_eq!(stack.redo_description(), Some("Redo valid object"));
        assert!(stack.redo(&mut world).unwrap());
        assert_eq!(
            world.get::<Transform>(valid_entity).unwrap().translation,
            valid_after.translation
        );
        assert!(!stack.redo(&mut world).unwrap());
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
    fn platformer_prefabs_compile_into_existing_runtime_adapters() {
        let (mut world, root) = persistence_test_world("platformer_runtime_adapters");
        let moving = spawn_editor_primitive_record(
            &mut world,
            EditorEntityId(201),
            EditorPrimitive::MovingPlatform,
            "Moving route".into(),
            Transform::from_xyz(4.0, 6.0, -8.0),
        );
        let spring = spawn_editor_primitive_record(
            &mut world,
            EditorEntityId(202),
            EditorPrimitive::SpringPlatform,
            "Pulse spring".into(),
            Transform::from_xyz(8.0, 2.0, -8.0),
        );
        let rotating = spawn_editor_primitive_record(
            &mut world,
            EditorEntityId(203),
            EditorPrimitive::RotatingBridge,
            "Rotating route".into(),
            Transform::from_xyz(12.0, 6.0, -8.0),
        );
        let flip = spawn_editor_primitive_record(
            &mut world,
            EditorEntityId(204),
            EditorPrimitive::FlipPanel,
            "Flip route".into(),
            Transform::from_xyz(16.0, 4.0, -8.0),
        );
        let collapse = spawn_editor_primitive_record(
            &mut world,
            EditorEntityId(205),
            EditorPrimitive::CollapseBridge,
            "Collapse route".into(),
            Transform::from_xyz(22.0, 5.0, -8.0),
        );
        let spikes = spawn_editor_primitive_record(
            &mut world,
            EditorEntityId(206),
            EditorPrimitive::SpikeBridge,
            "Spike route".into(),
            Transform::from_xyz(30.0, 5.0, -8.0),
        );

        for entity in [moving, spring, rotating, flip, collapse, spikes] {
            assert!(world
                .get::<crate::engine::physics::prelude::Collider>(entity)
                .is_some());
            assert!(world.get::<WalkableSurface>(entity).is_some());
        }
        assert!(world.get::<MovingPlatform>(moving).is_some());
        assert!(world.get::<SpringJumpPad>(spring).is_some());
        assert!(world.get::<RotatingElevator>(rotating).is_some());
        assert!(world.get::<FlipPlatform>(flip).is_some());
        assert!(world.get::<CollapsePlatform>(collapse).is_some());
        assert!(world.get::<SpikePlatformHazard>(spikes).is_some());
        assert_eq!(
            world.get::<crate::engine::physics::prelude::RigidBody>(moving),
            Some(&crate::engine::physics::prelude::RigidBody::KinematicPositionBased)
        );

        let edited = Transform::from_xyz(40.0, 9.0, -12.0)
            .with_rotation(Quat::from_rotation_y(0.5))
            .with_scale(Vec3::new(1.5, 1.0, 0.8));
        *world.get_mut::<Transform>(moving).unwrap() = edited;
        *world.get_mut::<Transform>(collapse).unwrap() = edited;
        let mut refresh = Schedule::default();
        refresh.add_systems(refresh_platformer_runtime_adapters);
        refresh.run(&mut world);

        let moving_adapter = world.get::<MovingPlatform>(moving).unwrap();
        assert_eq!(
            (moving_adapter.start + moving_adapter.end) * 0.5,
            edited.translation
        );
        assert_eq!(moving_adapter.size, Vec3::new(7.5, 0.6, 3.2));
        let collapse_adapter = world.get::<CollapsePlatform>(collapse).unwrap();
        assert_eq!(collapse_adapter.origin, edited.translation);
        assert_eq!(collapse_adapter.state, CollapsePlatformState::Armed);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_same_path_never_turns_the_default_session_into_a_writable_project() {
        let factory_session = EditorProjectSession::default();
        assert!(!factory_session.loaded);
        assert!(factory_session.primary_revision.is_none());

        let (mut world, root) = persistence_test_world("corrupt_same_path");
        world.init_resource::<EditorPresetLibrary>();
        world.init_resource::<NextState<EngineToolMode>>();
        let store = world.resource::<EditorProjectSession>().store.clone();
        std::fs::write(store.path(), b"{ corrupt project")
            .expect("test should corrupt the primary manifest");
        let corrupt_bytes = std::fs::read(store.path()).unwrap();
        {
            let mut session = world.resource_mut::<EditorProjectSession>();
            session.project = ForgeProject::default();
            session.loaded = false;
            session.recovery_write_authorized = false;
            session.primary_revision = None;
        }
        world.insert_resource(project_registry::ForgeProjectRegistry {
            projects: vec![project_registry::ProjectEntry {
                name: "Damaged project".into(),
                path: store.path().to_path_buf(),
            }],
            active: Some(store.path().to_path_buf()),
        });

        sync_active_project_session(&mut world);

        assert!(!world.resource::<EditorProjectSession>().loaded);
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("load failed"));
        assert!(!save_editor_project(&mut world, false));
        assert_eq!(std::fs::read(store.path()).unwrap(), corrupt_bytes);
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("no writable primary project"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_manual_load_preserves_the_previous_valid_session() {
        let (mut world, root) = persistence_test_world("failed_load_preserves_session");
        let store = world.resource::<EditorProjectSession>().store.clone();
        let before_project = world.resource::<EditorProjectSession>().project.clone();
        let before_revision = world
            .resource::<EditorProjectSession>()
            .primary_revision
            .clone();
        std::fs::write(store.path(), b"{ corrupt project")
            .expect("test should corrupt the primary manifest");

        load_editor_project(&mut world, false);

        let session = world.resource::<EditorProjectSession>();
        assert!(session.loaded);
        assert_eq!(session.project, before_project);
        assert_eq!(session.primary_revision, before_revision);
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("Load failed"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_recovery_can_be_promoted_over_the_exact_observed_primary() {
        let (mut world, root) = persistence_test_world("explicit_recovery_promotion");
        let store = world.resource::<EditorProjectSession>().store.clone();
        let recovery_name = world
            .resource::<EditorProjectSession>()
            .project
            .display_name
            .clone();
        let mut newer = store.load().unwrap();
        newer.display_name = "Newer but damaged revision".into();
        store.save(&mut newer).unwrap();
        std::fs::write(store.path(), b"{ corrupt primary")
            .expect("test should corrupt the primary manifest");

        load_editor_project(&mut world, true);

        {
            let session = world.resource::<EditorProjectSession>();
            assert!(session.loaded);
            assert!(session.recovery_write_authorized);
            assert!(session.primary_revision.is_some());
            assert_eq!(session.project.display_name, recovery_name);
        }
        world
            .resource_mut::<EditorProjectSession>()
            .project
            .display_name = "Recovered and promoted".into();
        assert!(save_editor_project(&mut world, false));
        let session = world.resource::<EditorProjectSession>();
        assert!(!session.recovery_write_authorized);
        assert_eq!(store.load().unwrap().display_name, "Recovered and promoted");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn editor_on_enter_adapts_raw_anchor_before_restoring_scene_override() {
        let (mut world, root) = persistence_test_world("on_enter_adapter_order");
        world.init_resource::<EditorFocus>();
        world.init_resource::<EditorOutlinerFilter>();
        world.init_resource::<EditorCameraRig>();
        world.init_resource::<Time<Virtual>>();
        world.init_resource::<Time<Physics>>();
        world.init_resource::<EditorPresetLibrary>();

        let baseline = Transform::from_xyz(2.0, 3.0, 4.0);
        let restored = Transform::from_xyz(12.0, 13.0, 14.0);
        let anchor = world
            .spawn((WorldAnchor { id: "raw_anchor" }, baseline))
            .id();
        assert!(world.get::<EditorAccess>(anchor).is_none());

        let store = world.resource::<EditorProjectSession>().store.clone();
        let mut project = store.load().unwrap();
        project.normalize_levels();
        project.scene.objects = vec![SceneObjectDraft {
            editor_id: 41,
            name: "Reserved scene id".into(),
            primitive: DraftPrimitive::Empty,
            transform: TransformDraft::default(),
            material_id: None,
            modifiers: Vec::new(),
        }];
        project.scene.adapter_overrides = vec![AdapterOverrideDraft {
            adapter_key: "anchor:raw_anchor".into(),
            transform: (&restored).into(),
        }];
        project.sync_active_level();
        store.save(&mut project).unwrap();
        {
            let mut session = world.resource_mut::<EditorProjectSession>();
            session.loaded = false;
            session.recovery_write_authorized = false;
            session.primary_revision = None;
        }
        world.insert_resource(project_registry::ForgeProjectRegistry {
            projects: vec![project_registry::ProjectEntry {
                name: "Scheduled project".into(),
                path: store.path().to_path_buf(),
            }],
            active: Some(store.path().to_path_buf()),
        });

        let mut on_enter = Schedule::new(OnEnter(EngineToolMode::Editing));
        on_enter.add_systems(
            (
                sync_active_project_session,
                enter_editor_workspace,
                adapt_world_authorables,
                apply_active_project_scene_adapters,
            )
                .chain(),
        );
        on_enter.run(&mut world);

        assert_eq!(
            world.get::<EditorAccess>(anchor),
            Some(&EditorAccess::Editable)
        );
        assert_eq!(
            world.get::<Transform>(anchor).unwrap().translation,
            restored.translation
        );
        assert_eq!(
            world
                .resource::<EditorAdapterBaselines>()
                .0
                .get("anchor:raw_anchor")
                .unwrap()
                .1
                .translation,
            baseline.translation,
            "the loaded override must not be captured as the reset baseline"
        );
        assert_eq!(
            world
                .get::<EditorWorldAdapter>(anchor)
                .unwrap()
                .last_translation,
            restored.translation,
            "restoration must not become a synthetic adapter drag on Update"
        );
        assert_eq!(
            world.get::<EditorEntityId>(anchor),
            Some(&EditorEntityId(42)),
            "scene ids must be reserved before adapters allocate ids"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn restoring_cave_gate_override_moves_entry_and_focus_exactly_once() {
        let mut world = World::new();
        world.init_resource::<EditorAdapterBaselines>();
        let initial_transform = Transform::from_xyz(5.0, 6.0, 7.0);
        let target_transform =
            Transform::from_xyz(8.0, 4.0, 13.0).with_rotation(Quat::from_rotation_y(0.7));
        let delta = target_transform.translation - initial_transform.translation;
        let initial_entry = Vec3::new(10.0, 11.0, 12.0);
        let initial_focus = Vec3::new(20.0, 21.0, 22.0);
        let gate = world
            .spawn((
                EditorAccess::Editable,
                EditorWorldAdapter {
                    kind: EditorAdapterKind::CaveGate,
                    last_translation: initial_transform.translation,
                },
                DungeonCrawlGate {
                    gate_id: "coupled_gate",
                    chapter: 1,
                    label: "Coupled Gate",
                    entry: initial_entry,
                    focus: initial_focus,
                    radius: 30.0,
                    interact_radius: 6.0,
                    opened: false,
                },
                initial_transform,
            ))
            .id();
        let scene = EditorSceneDraft {
            objects: Vec::new(),
            adapter_overrides: vec![AdapterOverrideDraft {
                adapter_key: "cave:coupled_gate".into(),
                transform: (&target_transform).into(),
            }],
        };

        assert_eq!(apply_scene_adapter_overrides(&mut world, &scene), 0);

        let restored = world.get::<DungeonCrawlGate>(gate).unwrap();
        assert_eq!(restored.entry, initial_entry + delta);
        assert_eq!(restored.focus, initial_focus + delta);
        assert_eq!(world.get::<Transform>(gate).unwrap(), &target_transform);
        assert_eq!(
            world
                .get::<EditorWorldAdapter>(gate)
                .unwrap()
                .last_translation,
            target_transform.translation
        );

        let mut next_update = Schedule::default();
        next_update.add_systems(sync_world_adapters);
        next_update.run(&mut world);
        let after_update = world.get::<DungeonCrawlGate>(gate).unwrap();
        assert_eq!(after_update.entry, initial_entry + delta);
        assert_eq!(after_update.focus, initial_focus + delta);

        assert_eq!(
            apply_scene_adapter_overrides(&mut world, &EditorSceneDraft::default()),
            0
        );
        let reset = world.get::<DungeonCrawlGate>(gate).unwrap();
        assert_eq!(reset.entry, initial_entry);
        assert_eq!(reset.focus, initial_focus);
        assert_eq!(world.get::<Transform>(gate).unwrap(), &initial_transform);
        assert_eq!(
            world
                .get::<EditorWorldAdapter>(gate)
                .unwrap()
                .last_translation,
            initial_transform.translation
        );
        next_update.run(&mut world);
        let after_reset_update = world.get::<DungeonCrawlGate>(gate).unwrap();
        assert_eq!(after_reset_update.entry, initial_entry);
        assert_eq!(after_reset_update.focus, initial_focus);
    }

    #[test]
    fn restoring_settlement_override_moves_origin_exactly_once() {
        let mut world = World::new();
        world.init_resource::<EditorAdapterBaselines>();
        let initial_transform = Transform::from_xyz(-1.0, 2.0, 3.0);
        let target_transform =
            Transform::from_xyz(4.0, 6.0, -2.0).with_scale(Vec3::new(1.2, 0.8, 1.4));
        let delta = target_transform.translation - initial_transform.translation;
        let initial_origin = Vec3::new(-10.0, 20.0, 30.0);
        let terminal = world
            .spawn((
                EditorAccess::Editable,
                EditorWorldAdapter {
                    kind: EditorAdapterKind::SettlementTerminal,
                    last_translation: initial_transform.translation,
                },
                SettlementBuildTerminal {
                    settlement_id: "coupled_terminal",
                    settlement_name: "Coupled Terminal",
                    reward_id: "test_reward",
                    kind: crate::chapters::MapSettlementKind::Outpost,
                    origin: initial_origin,
                    facing_yaw: 0.0,
                    interact_radius: 18.0,
                },
                initial_transform,
            ))
            .id();
        let scene = EditorSceneDraft {
            objects: Vec::new(),
            adapter_overrides: vec![AdapterOverrideDraft {
                adapter_key: "settlement:coupled_terminal".into(),
                transform: (&target_transform).into(),
            }],
        };

        assert_eq!(apply_scene_adapter_overrides(&mut world, &scene), 0);

        assert_eq!(
            world
                .get::<SettlementBuildTerminal>(terminal)
                .unwrap()
                .origin,
            initial_origin + delta
        );
        assert_eq!(world.get::<Transform>(terminal).unwrap(), &target_transform);
        assert_eq!(
            world
                .get::<EditorWorldAdapter>(terminal)
                .unwrap()
                .last_translation,
            target_transform.translation
        );

        let mut next_update = Schedule::default();
        next_update.add_systems(sync_world_adapters);
        next_update.run(&mut world);
        assert_eq!(
            world
                .get::<SettlementBuildTerminal>(terminal)
                .unwrap()
                .origin,
            initial_origin + delta
        );

        assert_eq!(
            apply_scene_adapter_overrides(&mut world, &EditorSceneDraft::default()),
            0
        );
        assert_eq!(
            world
                .get::<SettlementBuildTerminal>(terminal)
                .unwrap()
                .origin,
            initial_origin
        );
        assert_eq!(
            world.get::<Transform>(terminal).unwrap(),
            &initial_transform
        );
        assert_eq!(
            world
                .get::<EditorWorldAdapter>(terminal)
                .unwrap()
                .last_translation,
            initial_transform.translation
        );
        next_update.run(&mut world);
        assert_eq!(
            world
                .get::<SettlementBuildTerminal>(terminal)
                .unwrap()
                .origin,
            initial_origin
        );
    }

    #[test]
    fn two_level_project_round_trip_applies_each_adapter_scene_from_baseline() {
        let (mut world, root) = persistence_test_world("two_level_adapter_round_trip");
        let baseline = Transform::from_xyz(1.0, 2.0, 3.0);
        let first = Transform::from_xyz(11.0, 12.0, 13.0);
        let second = Transform::from_xyz(21.0, 22.0, 23.0);
        let anchor = world
            .spawn((
                EditorAccess::Editable,
                WorldAnchor { id: "level_anchor" },
                baseline,
            ))
            .id();
        let (first_id, second_id, first_scene) = {
            let mut session = world.resource_mut::<EditorProjectSession>();
            session.project.normalize_levels();
            let first_id = session.project.active_level_id.clone();
            session.project.scene.adapter_overrides = vec![AdapterOverrideDraft {
                adapter_key: "anchor:level_anchor".into(),
                transform: (&first).into(),
            }];
            session.project.sync_active_level();
            let second_id = session.project.create_level(LevelTemplate::EmptyTestArena);
            session.project.scene.adapter_overrides = vec![AdapterOverrideDraft {
                adapter_key: "anchor:level_anchor".into(),
                transform: (&second).into(),
            }];
            session.project.sync_active_level();
            session.project.activate_level(&first_id).unwrap();
            (first_id, second_id, session.project.scene.clone())
        };
        respawn_scene_objects(&mut world, &first_scene.objects);
        apply_scene_adapter_overrides(&mut world, &first_scene);
        assert_eq!(
            world.get::<Transform>(anchor).unwrap().translation,
            first.translation
        );
        assert!(save_editor_project(&mut world, false));

        *world.get_mut::<Transform>(anchor).unwrap() = Transform::from_xyz(99.0, 99.0, 99.0);
        load_editor_project(&mut world, false);
        assert_eq!(
            world
                .resource::<EditorProjectSession>()
                .project
                .active_level_id,
            first_id
        );
        assert_eq!(
            world.get::<Transform>(anchor).unwrap().translation,
            first.translation
        );

        world
            .resource_mut::<EditorUndoStack>()
            .undo
            .push(EditorTransaction {
                description: "stale level command".into(),
                commands: Vec::new(),
            });
        apply_editor_action(&mut world, EditorAction::LevelNext);
        assert_eq!(
            world
                .resource::<EditorProjectSession>()
                .project
                .active_level_id,
            second_id
        );
        assert_eq!(
            world.get::<Transform>(anchor).unwrap().translation,
            second.translation
        );
        assert!(world.resource::<EditorUndoStack>().undo.is_empty());

        apply_editor_action(&mut world, EditorAction::LevelNext);
        assert_eq!(
            world.get::<Transform>(anchor).unwrap().translation,
            first.translation
        );
        world
            .resource_mut::<EditorProjectSession>()
            .project
            .levels
            .iter_mut()
            .find(|level| level.level_id == second_id)
            .unwrap()
            .scene
            .adapter_overrides
            .clear();
        apply_editor_action(&mut world, EditorAction::LevelNext);
        assert_eq!(
            world.get::<Transform>(anchor).unwrap().translation,
            baseline.translation,
            "a sparse target scene must reset adapters instead of inheriting the previous level"
        );

        world.despawn(anchor);
        let regenerated_baseline = Transform::from_xyz(31.0, 32.0, 33.0);
        let regenerated = world
            .spawn((
                EditorAccess::Editable,
                WorldAnchor { id: "level_anchor" },
                regenerated_baseline,
            ))
            .id();
        apply_scene_adapter_overrides(&mut world, &EditorSceneDraft::default());
        assert_eq!(
            world.get::<Transform>(regenerated).unwrap().translation,
            regenerated_baseline.translation,
            "a regenerated world owns a fresh adapter baseline even when its stable key is reused"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stale_editor_session_cannot_overwrite_a_newer_project_revision() {
        let (mut world, root) = persistence_test_world("stale_session_save");
        assert!(save_editor_project(&mut world, false));
        let store = world.resource::<EditorProjectSession>().store.clone();

        let mut external = store.load().expect("saved project should load");
        external.display_name = "Updated by Creature Forge".into();
        store
            .save(&mut external)
            .expect("external tool update should save");

        world
            .resource_mut::<EditorProjectSession>()
            .project
            .display_name = "Stale live editor".into();
        world.resource_mut::<EditorDocumentState>().dirty = true;
        assert!(!save_editor_project(&mut world, false));
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("changed on disk"));
        assert_eq!(
            store.load().unwrap().display_name,
            "Updated by Creature Forge",
            "the newer external revision must remain authoritative"
        );

        load_editor_project(&mut world, false);
        assert_eq!(
            world
                .resource::<EditorProjectSession>()
                .project
                .display_name,
            "Updated by Creature Forge"
        );
        assert!(!world.resource::<EditorDocumentState>().dirty);
        world
            .resource_mut::<EditorProjectSession>()
            .project
            .display_name = "Reconciled editor".into();
        assert!(save_editor_project(&mut world, false));
        assert_eq!(store.load().unwrap().display_name, "Reconciled editor");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reentering_clean_editor_reloads_same_path_after_external_update() {
        let (mut world, root) = persistence_test_world("same_path_reload");
        world.init_resource::<EditorPresetLibrary>();
        assert!(save_editor_project(&mut world, false));
        let store = world.resource::<EditorProjectSession>().store.clone();
        world.insert_resource(project_registry::ForgeProjectRegistry {
            projects: vec![project_registry::ProjectEntry {
                name: "Shared project".into(),
                path: store.path().to_path_buf(),
            }],
            active: Some(store.path().to_path_buf()),
        });

        let mut external = store.load().unwrap();
        external.display_name = "Fresh external revision".into();
        external.scene.objects.push(SceneObjectDraft {
            editor_id: 901,
            name: "External marker".into(),
            primitive: DraftPrimitive::Beacon,
            transform: TransformDraft::default(),
            material_id: None,
            modifiers: Vec::new(),
        });
        store.save(&mut external).unwrap();

        sync_active_project_session(&mut world);

        assert_eq!(
            world
                .resource::<EditorProjectSession>()
                .project
                .display_name,
            "Fresh external revision"
        );
        assert!(resolve_editor_entity(&mut world, EditorEntityId(901)).is_some());
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("Reloaded primary project"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn live_editor_publish_advances_runtime_generation_and_reconciles_session() {
        let (mut world, root) = persistence_test_world("live_publish_generation");
        let weapon_id = {
            let mut session = world.resource_mut::<EditorProjectSession>();
            let spec = crate::combat::weapon_forge::WeaponSpec {
                name: "Generation Sabre".into(),
                ..Default::default()
            };
            weapon_records::upsert_weapon(&mut session.project, &spec).unwrap()
        };
        world.resource_mut::<EditorDocumentState>().dirty = true;
        let published = root.join("published");

        assert!(publish_editor_project_to(&mut world, &published));

        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(published.join("current.json")).unwrap())
                .unwrap();
        let generation = manifest["generation"].as_str().unwrap();
        let weapons: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                published
                    .join("generations")
                    .join(generation)
                    .join("weapons.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(weapons.as_array().unwrap().len(), 1);
        let session = world.resource::<EditorProjectSession>();
        let weapon = session
            .project
            .records
            .iter()
            .find(|record| record.content_id == weapon_id)
            .unwrap();
        assert_eq!(weapon.published_hash.as_ref(), Some(&weapon.draft_hash));
        assert_eq!(
            session.primary_revision,
            session.store.primary_revision().ok()
        );
        assert!(!world.resource::<EditorDocumentState>().dirty);
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("Published 1 weapon"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn live_editor_publish_failure_reconciles_disk_revision_and_reports_failure() {
        let (mut world, root) = persistence_test_world("live_publish_failure");
        {
            let mut session = world.resource_mut::<EditorProjectSession>();
            let spec = crate::combat::weapon_forge::WeaponSpec {
                name: "Blocked Sabre".into(),
                ..Default::default()
            };
            weapon_records::upsert_weapon(&mut session.project, &spec).unwrap();
        }
        world.resource_mut::<EditorDocumentState>().dirty = true;
        let blocked_output = root.join("not-a-directory");
        std::fs::write(&blocked_output, b"block publish directory").unwrap();

        assert!(!publish_editor_project_to(&mut world, &blocked_output));

        let session = world.resource::<EditorProjectSession>();
        assert_eq!(session.project, session.store.load().unwrap());
        assert_eq!(
            session.primary_revision,
            session.store.primary_revision().ok()
        );
        assert!(!world.resource::<EditorDocumentState>().dirty);
        assert!(world
            .resource::<EditorRuntimeStatus>()
            .message
            .contains("Publish failed"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_preserves_authored_name_and_modifier_stack_through_reload() {
        use crate::character::mesh_modifiers::{MeshModifier, ModifierAxis};

        let (mut world, root) = persistence_test_world("duplicate_round_trip");
        let source_id = EditorEntityId(81);
        world.resource_mut::<EditorIdAllocator>().reserve(source_id);
        let source = spawn_editor_primitive_record(
            &mut world,
            source_id,
            EditorPrimitive::Cube,
            "Authored Observatory".into(),
            Transform::from_xyz(2.0, 3.0, -4.0),
        );
        let modifiers = vec![
            MeshModifier::Mirror {
                axis: ModifierAxis::Z,
            },
            MeshModifier::Array {
                count: 4,
                offset: [0.0, 2.5, 0.0],
            },
        ];
        world
            .entity_mut(source)
            .insert(EditorModifierStack(modifiers.clone()));
        assert!(remesh_modified_object(&mut world, source));
        world.resource_mut::<EditorSelection>().replace(source_id);

        duplicate_active(&mut world);

        let duplicate_id = world.resource::<EditorSelection>().active().unwrap();
        assert_ne!(duplicate_id, source_id);
        let duplicate = resolve_editor_entity(&mut world, duplicate_id).unwrap();
        assert_eq!(
            world.get::<Name>(duplicate).unwrap().as_str(),
            "Authored Observatory"
        );
        assert_eq!(
            &world.get::<EditorModifierStack>(duplicate).unwrap().0,
            &modifiers
        );

        // The duplicate owns an independent stack, not shared mutable state.
        world
            .get_mut::<EditorModifierStack>(source)
            .unwrap()
            .0
            .push(MeshModifier::Subdivide { levels: 1 });
        assert_eq!(
            &world.get::<EditorModifierStack>(duplicate).unwrap().0,
            &modifiers
        );

        let collected = collect_working_scene(&mut world);
        let duplicate_draft = collected
            .objects
            .iter()
            .find(|object| object.editor_id == duplicate_id.0)
            .unwrap();
        assert_eq!(duplicate_draft.name, "Authored Observatory");
        assert_eq!(duplicate_draft.modifiers, modifiers);

        assert!(save_editor_project(&mut world, false));
        let saved = world
            .resource::<EditorProjectSession>()
            .store
            .load()
            .unwrap();
        let saved_duplicate = saved
            .scene
            .objects
            .iter()
            .find(|object| object.editor_id == duplicate_id.0)
            .unwrap();
        assert_eq!(saved_duplicate.name, "Authored Observatory");
        assert_eq!(saved_duplicate.modifiers, modifiers);

        world
            .entity_mut(duplicate)
            .insert(Name::new("Unsaved mutation"))
            .insert(EditorModifierStack::default());
        load_editor_project(&mut world, false);
        let reloaded = resolve_editor_entity(&mut world, duplicate_id).unwrap();
        assert_eq!(
            world.get::<Name>(reloaded).unwrap().as_str(),
            "Authored Observatory"
        );
        assert_eq!(
            &world.get::<EditorModifierStack>(reloaded).unwrap().0,
            &modifiers
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

        let published = root.join("published");
        publish_editor_project_to(&mut world, &published);
        apply_selected_material_to_object(&mut world);
        publish_editor_project_to(&mut world, &published);
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
    fn published_world_topology_extends_the_runtime_map_catalog() {
        let (mut world, root) = persistence_test_world("published_map_points");
        {
            let mut session = world.resource_mut::<EditorProjectSession>();
            let cave_id = session
                .project
                .create_content(persistence::ContentCategory::Cave, "Rainbow Grotto")
                .unwrap();
            let recipe = session
                .project
                .payloads
                .get_mut(&cave_id)
                .unwrap()
                .procedural_recipe_mut()
                .unwrap();
            recipe.terrain_projection.world_origin = [120.0, -80.0];
            recipe
                .fields
                .insert("fast_travel_points".into(), serde_json::json!(1));
            recipe.topology_nodes = vec![
                WorldTopologyNodeDraft {
                    node_id: "entrance".into(),
                    kind: WorldTopologyNodeKind::Entrance,
                    position: [-10.0, 0.0, 0.0],
                    size: [4.0, 4.0, 4.0],
                },
                WorldTopologyNodeDraft {
                    node_id: "crystal_gate".into(),
                    kind: WorldTopologyNodeKind::FastTravel,
                    position: [10.0, 4.0, 20.0],
                    size: [4.0, 4.0, 4.0],
                },
                WorldTopologyNodeDraft {
                    node_id: "rainbow_falls".into(),
                    kind: WorldTopologyNodeKind::Landmark,
                    position: [30.0, 8.0, 40.0],
                    size: [6.0, 8.0, 6.0],
                },
            ];
            recipe.topology_edges = vec![
                WorldTopologyEdgeDraft {
                    from: "entrance".into(),
                    to: "crystal_gate".into(),
                    kind: WorldTopologyEdgeKind::Tunnel,
                    from_socket: None,
                    to_socket: None,
                },
                WorldTopologyEdgeDraft {
                    from: "crystal_gate".into(),
                    to: "rainbow_falls".into(),
                    kind: WorldTopologyEdgeKind::Tunnel,
                    from_socket: None,
                    to_socket: None,
                },
            ];
            session.project.refresh_hashes().unwrap();
            session.project.publish_drafts().unwrap();
        }

        let project = world.resource::<EditorProjectSession>().project.clone();
        rebuild_published_content_catalogs(&mut world, &project);
        let points = world
            .resource::<PublishedProceduralRecipeCatalog>()
            .map_points()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].kind, PublishedMapPointKind::FastTravel);
        assert_eq!(points[0].position, Vec3::new(130.0, 4.0, -60.0));
        assert!(points[0].label.contains("Rainbow Grotto"));
        assert_eq!(points[1].kind, PublishedMapPointKind::Landmark);
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
    fn tool_window_capture_blocks_new_viewport_gestures_but_not_active_drags() {
        assert!(editor_viewport_pointer_available(false, false));
        assert!(editor_viewport_pointer_available(false, true));
        assert!(editor_viewport_pointer_available(true, true));
        assert!(
            !editor_viewport_pointer_available(true, false),
            "a movable tool window must own a fresh pointer press under its real geometry"
        );
    }

    #[test]
    fn editor_pointer_capture_combines_floating_windows_and_fixed_chrome() {
        assert!(!editor_ui_captures_pointer(false, [false, false]));
        assert!(editor_ui_captures_pointer(true, [false, false]));
        assert!(editor_ui_captures_pointer(false, [true, false]));
        assert!(editor_ui_captures_pointer(false, [false, true]));
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

    #[test]
    fn semantic_label_kinds_cover_required_editor_domains() {
        assert_eq!(
            authorable_semantic_kind(EditorAdapterKind::WorldAnchor),
            Some(SemanticViewportLabelKind::Spawn)
        );
        assert_eq!(
            authorable_semantic_kind(EditorAdapterKind::RouteMarker),
            Some(SemanticViewportLabelKind::Route)
        );
        assert_eq!(
            authorable_semantic_kind(EditorAdapterKind::CaveGate),
            Some(SemanticViewportLabelKind::ModeBoundary)
        );
        assert_eq!(
            topology_node_semantic_kind(WorldTopologyNodeKind::FastTravel),
            Some(SemanticViewportLabelKind::Spawn)
        );
        assert_eq!(
            topology_node_semantic_kind(WorldTopologyNodeKind::Entrance),
            Some(SemanticViewportLabelKind::ModeBoundary)
        );
    }

    #[test]
    fn semantic_labels_are_ascii_for_bevy_stroke_text() {
        assert_eq!(
            semantic_label_text(SemanticViewportLabelKind::ModeBoundary, "Cave • Δ"),
            "MODE: Cave"
        );
        assert!(EditorPreferences::default().semantic_labels_visible);
    }

    #[test]
    fn native_outliner_filter_normalizes_and_bounds_pasted_text() {
        assert_eq!(
            sanitize_forge_text(ForgeTextField::OutlinerFilter, "  Sky.Castle/Δ-ONE_2!  "),
            "  skycastleδ-one_2  "
        );
        assert_eq!(
            sanitize_forge_text(ForgeTextField::OutlinerFilter, &"A".repeat(80)).len(),
            32
        );
        assert_eq!(
            sanitize_forge_text(ForgeTextField::ContentId, "Road.New Δ/LOOP"),
            "road.newloop"
        );
        assert_eq!(
            validated_display_name("  Sky Castle  "),
            Ok("Sky Castle".into())
        );
        assert!(validated_display_name("   ").is_err());
    }

    #[test]
    fn metadata_and_content_id_transactions_undo_and_redo_atomically() {
        let (mut world, root) = persistence_test_world("metadata_undo");
        let (level_id, old_level_name, old_project_name, old_content_id) = {
            let project = &world.resource::<EditorProjectSession>().project;
            let level = project.active_level().expect("normalized active level");
            (
                level.level_id.clone(),
                level.display_name.clone(),
                project.display_name.clone(),
                project.records[0].content_id.clone(),
            )
        };
        let new_content_id = "scene.metadata_undo".to_owned();
        let transaction = EditorTransaction {
            description: "Edit Forge metadata".into(),
            commands: vec![
                EditorCommand::SetProjectDisplayName {
                    before: old_project_name.clone(),
                    after: "Technicolor Project".into(),
                },
                EditorCommand::SetLevelDisplayName {
                    level_id: level_id.clone(),
                    before: old_level_name.clone(),
                    after: "Prism Harbor".into(),
                },
                EditorCommand::RenameContent {
                    before: old_content_id.clone(),
                    after: new_content_id.clone(),
                },
            ],
        };

        world
            .resource_scope(|world, mut stack: Mut<EditorUndoStack>| {
                stack.execute(world, transaction)
            })
            .unwrap();
        {
            let project = &world.resource::<EditorProjectSession>().project;
            assert_eq!(project.display_name, "Technicolor Project");
            assert_eq!(project.active_level().unwrap().display_name, "Prism Harbor");
            assert!(project
                .records
                .iter()
                .any(|record| record.content_id == new_content_id));
        }

        assert!(world
            .resource_scope(|world, mut stack: Mut<EditorUndoStack>| stack.undo(world))
            .unwrap());
        {
            let project = &world.resource::<EditorProjectSession>().project;
            assert_eq!(project.display_name, old_project_name);
            assert_eq!(project.active_level().unwrap().display_name, old_level_name);
            assert!(project
                .records
                .iter()
                .any(|record| record.content_id == old_content_id));
        }

        assert!(world
            .resource_scope(|world, mut stack: Mut<EditorUndoStack>| stack.redo(world))
            .unwrap());
        assert_eq!(
            world
                .resource::<EditorProjectSession>()
                .project
                .active_level()
                .unwrap()
                .display_name,
            "Prism Harbor"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn registry_rename_action_focuses_the_reusable_native_field() {
        let (mut world, root) = persistence_test_world("native_registry_focus");
        let field = world
            .spawn((
                ForgeTextFieldMarker {
                    kind: ForgeTextField::ContentId,
                },
                EditableText::default(),
            ))
            .id();
        let content_id = world.resource::<EditorProjectSession>().project.records[0]
            .content_id
            .clone();

        apply_editor_action(&mut world, EditorAction::RegistryRename);

        assert!(world.resource::<EditorRegistryState>().rename_active);
        assert_eq!(world.resource::<InputFocus>().get(), Some(field));
        assert_eq!(
            world
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string(),
            content_id
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dialogue_editor_actions_create_and_undo_modular_nodes() {
        let (mut world, root) = persistence_test_world("dialogue_actions");
        world.init_resource::<EditorPendingTransactions>();
        apply_editor_action(&mut world, EditorAction::CreateDialogueGraph);
        let selected = world.resource::<EditorRegistryState>().selected;
        let content_id = world.resource::<EditorProjectSession>().project.records[selected]
            .content_id
            .clone();
        assert_eq!(
            dialogue_records::load_dialogue(
                &world.resource::<EditorProjectSession>().project,
                &content_id,
            )
            .unwrap()
            .nodes
            .len(),
            1
        );

        apply_editor_action(&mut world, EditorAction::DialogueAddNode);
        let transaction = world
            .resource_mut::<EditorPendingTransactions>()
            .0
            .pop()
            .expect("add node should queue one transaction");
        world
            .resource_scope(|world, mut stack: Mut<EditorUndoStack>| {
                stack.execute(world, transaction)
            })
            .unwrap();
        assert_eq!(
            dialogue_records::load_dialogue(
                &world.resource::<EditorProjectSession>().project,
                &content_id,
            )
            .unwrap()
            .nodes
            .len(),
            2
        );
        assert!(world
            .resource_scope(|world, mut stack: Mut<EditorUndoStack>| stack.undo(world))
            .unwrap());
        assert_eq!(
            dialogue_records::load_dialogue(
                &world.resource::<EditorProjectSession>().project,
                &content_id,
            )
            .unwrap()
            .nodes
            .len(),
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
