//! Versioned, atomic persistence for Starfall Forge projects.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CURRENT_PROJECT_SCHEMA: u32 = 1;
pub const DEFAULT_PROJECT_FILE: &str = "starfall_forge/project.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForgeProject {
    pub project_id: String,
    pub display_name: String,
    pub schema_version: u32,
    #[serde(default)]
    pub records: Vec<ContentRecord>,
    #[serde(default)]
    pub scene: EditorSceneDraft,
    #[serde(default)]
    pub payloads: BTreeMap<String, ContentPayload>,
}

impl Default for ForgeProject {
    fn default() -> Self {
        let scene = EditorSceneDraft::default();
        let mut payloads = BTreeMap::new();
        payloads.insert(
            "scene.main_world".into(),
            ContentPayload::Scene(scene.clone()),
        );
        Self {
            project_id: "starfall-main-world".into(),
            display_name: "Starfall Main World".into(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            records: vec![ContentRecord {
                content_id: "scene.main_world".into(),
                schema_version: CURRENT_PROJECT_SCHEMA,
                category: ContentCategory::Scene,
                source_path: "scenes/main_world.json".into(),
                dependencies: Vec::new(),
                display_name: "Main World Draft".into(),
                tags: vec!["world".into(), "draft".into()],
                thumbnail: None,
                draft_hash: String::new(),
                published_hash: None,
            }],
            scene,
            payloads,
        }
    }
}

impl ForgeProject {
    pub fn refresh_hashes(&mut self) -> Result<(), ProjectIoError> {
        for record in &self.records {
            if record.category == ContentCategory::Scene {
                self.payloads.insert(
                    record.content_id.clone(),
                    ContentPayload::Scene(self.scene.clone()),
                );
            }
        }
        for record in &mut self.records {
            if let Some(payload) = self.payloads.get(&record.content_id) {
                record.draft_hash = payload_hash(payload)?;
            }
        }
        Ok(())
    }

    pub fn rename_content(
        &mut self,
        old_id: &str,
        new_id: &str,
    ) -> Result<(), ProjectValidationError> {
        if new_id.trim().is_empty() {
            return Err(ProjectValidationError::InvalidContentId(new_id.into()));
        }
        if self
            .records
            .iter()
            .any(|record| record.content_id == new_id)
        {
            return Err(ProjectValidationError::DuplicateContentId(new_id.into()));
        }
        let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.content_id == old_id)
        else {
            return Err(ProjectValidationError::MissingContent(old_id.into()));
        };
        record.content_id = new_id.into();
        if let Some(payload) = self.payloads.remove(old_id) {
            self.payloads.insert(new_id.into(), payload);
        }
        for record in &mut self.records {
            for dependency in &mut record.dependencies {
                if dependency == old_id {
                    *dependency = new_id.into();
                }
            }
        }
        for object in &mut self.scene.objects {
            if object.material_id.as_deref() == Some(old_id) {
                object.material_id = Some(new_id.into());
            }
        }
        for payload in self.payloads.values_mut() {
            let Some(recipe) = payload.procedural_recipe_mut() else {
                continue;
            };
            for material_id in recipe.material_slots.values_mut() {
                if material_id == old_id {
                    *material_id = new_id.into();
                }
            }
        }
        Ok(())
    }

    pub fn publish_drafts(&mut self) -> Result<(), ProjectIoError> {
        self.synchronize_authored_dependencies();
        self.refresh_hashes()?;
        let errors = validate_project(self);
        if !errors.is_empty() {
            return Err(ProjectIoError::Validation(errors));
        }
        for record in &mut self.records {
            record.published_hash = Some(record.draft_hash.clone());
        }
        Ok(())
    }

    /// Rebuild material dependencies from authored scene and procedural slots.
    /// Other dependency families are retained so this remains forward-compatible
    /// with character, creature, and gameplay-graph references.
    pub fn synchronize_authored_dependencies(&mut self) {
        let material_ids = self
            .records
            .iter()
            .filter(|record| record.category == ContentCategory::Material)
            .map(|record| record.content_id.clone())
            .collect::<BTreeSet<_>>();
        let scene_materials = self
            .scene
            .objects
            .iter()
            .filter_map(|object| object.material_id.clone())
            .collect::<BTreeSet<_>>();
        let authored = self
            .records
            .iter()
            .map(|record| {
                let bindings = if record.category == ContentCategory::Scene {
                    scene_materials.clone()
                } else {
                    self.payloads
                        .get(&record.content_id)
                        .and_then(ContentPayload::procedural_recipe)
                        .map(|recipe| recipe.material_slots.values().cloned().collect())
                        .unwrap_or_default()
                };
                (record.content_id.clone(), bindings)
            })
            .collect::<BTreeMap<_, BTreeSet<_>>>();
        for record in &mut self.records {
            record
                .dependencies
                .retain(|dependency| !material_ids.contains(dependency));
            if let Some(bindings) = authored.get(&record.content_id) {
                record.dependencies.extend(bindings.iter().cloned());
                record.dependencies.sort();
                record.dependencies.dedup();
            }
        }
    }

    pub fn create_content(
        &mut self,
        category: ContentCategory,
        display_name: &str,
    ) -> Result<String, ProjectMutationError> {
        if category == ContentCategory::Scene
            && self
                .records
                .iter()
                .any(|record| record.category == ContentCategory::Scene)
        {
            return Err(ProjectMutationError::SingleSceneWorkspace);
        }
        let base = format!("{}.{}", category.id_prefix(), slug(display_name));
        let content_id = unique_value(&base, |candidate| {
            self.records
                .iter()
                .any(|record| record.content_id == candidate)
        });
        let source_base = format!(
            "{}/{}.json",
            category.source_folder(),
            content_id.replace('.', "_")
        );
        let source_path = unique_value(&source_base, |candidate| {
            self.records
                .iter()
                .any(|record| record.source_path == candidate)
        });
        self.records.push(ContentRecord {
            content_id: content_id.clone(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category,
            source_path,
            dependencies: Vec::new(),
            display_name: display_name.trim().to_owned(),
            tags: vec!["draft".into()],
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        });
        self.payloads
            .insert(content_id.clone(), ContentPayload::empty(category));
        Ok(content_id)
    }

    pub fn duplicate_content(&mut self, content_id: &str) -> Result<String, ProjectMutationError> {
        let Some(source) = self
            .records
            .iter()
            .find(|record| record.content_id == content_id)
            .cloned()
        else {
            return Err(ProjectMutationError::MissingContent(content_id.into()));
        };
        if source.category == ContentCategory::Scene {
            return Err(ProjectMutationError::SingleSceneWorkspace);
        }
        let payload = self
            .payloads
            .get(content_id)
            .cloned()
            .ok_or_else(|| ProjectMutationError::MissingPayload(content_id.into()))?;
        let new_id = unique_value(&format!("{content_id}_copy"), |candidate| {
            self.records
                .iter()
                .any(|record| record.content_id == candidate)
        });
        let source_stem = source.source_path.trim_end_matches(".json");
        let new_path = unique_value(&format!("{source_stem}_copy.json"), |candidate| {
            self.records
                .iter()
                .any(|record| record.source_path == candidate)
        });
        self.records.push(ContentRecord {
            content_id: new_id.clone(),
            source_path: new_path,
            display_name: format!("{} Copy", source.display_name),
            draft_hash: String::new(),
            published_hash: None,
            ..source
        });
        self.payloads.insert(new_id.clone(), payload);
        Ok(new_id)
    }

    pub fn delete_content(
        &mut self,
        content_id: &str,
    ) -> Result<ContentRecord, ProjectMutationError> {
        let Some(index) = self
            .records
            .iter()
            .position(|record| record.content_id == content_id)
        else {
            return Err(ProjectMutationError::MissingContent(content_id.into()));
        };
        if self.records[index].category == ContentCategory::Scene {
            return Err(ProjectMutationError::SingleSceneWorkspace);
        }
        if let Some(owner) = self.records.iter().find(|record| {
            record
                .dependencies
                .iter()
                .any(|dependency| dependency == content_id)
        }) {
            return Err(ProjectMutationError::ContentHasDependents {
                content_id: content_id.into(),
                owner: owner.content_id.clone(),
            });
        }
        if self
            .scene
            .objects
            .iter()
            .any(|object| object.material_id.as_deref() == Some(content_id))
        {
            return Err(ProjectMutationError::ContentHasDependents {
                content_id: content_id.into(),
                owner: "scene object material binding".into(),
            });
        }
        if let Some((owner, slot)) = self.payloads.iter().find_map(|(owner, payload)| {
            payload.procedural_recipe().and_then(|recipe| {
                recipe
                    .material_slots
                    .iter()
                    .find_map(|(slot, material_id)| {
                        (material_id == content_id).then(|| (owner.clone(), slot.clone()))
                    })
            })
        }) {
            return Err(ProjectMutationError::ContentHasDependents {
                content_id: content_id.into(),
                owner: format!("{owner} material slot {slot}"),
            });
        }
        self.payloads.remove(content_id);
        Ok(self.records.remove(index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectMutationError {
    MissingContent(String),
    MissingPayload(String),
    ContentHasDependents { content_id: String, owner: String },
    SingleSceneWorkspace,
    InvalidSourcePath(String),
    SourceAlreadyExists(String),
    Io(String),
}

impl std::fmt::Display for ProjectMutationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

fn slug(value: &str) -> String {
    let slug = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if slug.is_empty() {
        "untitled".into()
    } else {
        slug
    }
}

fn unique_value(base: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(base) {
        return base.into();
    }
    (2_u32..)
        .map(|suffix| format!("{base}_{suffix}"))
        .find(|candidate| !exists(candidate))
        .unwrap_or_else(|| format!("{base}_overflow"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContentRecord {
    pub content_id: String,
    pub schema_version: u32,
    pub category: ContentCategory,
    pub source_path: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub display_name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub draft_hash: String,
    pub published_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct RecordSourceDocument {
    content_id: String,
    schema_version: u32,
    payload: ContentPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ContentPayload {
    Scene(EditorSceneDraft),
    Character(GenericRecipeDraft),
    Creature(GenericRecipeDraft),
    Biome(ProceduralRecipeDraft),
    Road(ProceduralRecipeDraft),
    Building(ProceduralRecipeDraft),
    Cave(ProceduralRecipeDraft),
    City(ProceduralRecipeDraft),
    Material(GenericRecipeDraft),
    Ui(GenericRecipeDraft),
}

impl ContentPayload {
    pub fn empty(category: ContentCategory) -> Self {
        let recipe = GenericRecipeDraft::default();
        match category {
            ContentCategory::Scene => Self::Scene(EditorSceneDraft::default()),
            ContentCategory::Character => Self::Character(recipe),
            ContentCategory::Creature => Self::Creature(recipe),
            ContentCategory::Biome => Self::Biome(ProceduralRecipeDraft::default()),
            ContentCategory::Road => Self::Road(ProceduralRecipeDraft::default()),
            ContentCategory::Building => Self::Building(ProceduralRecipeDraft::default()),
            ContentCategory::Cave => Self::Cave(ProceduralRecipeDraft::default()),
            ContentCategory::City => Self::City(ProceduralRecipeDraft::default()),
            ContentCategory::Material => {
                let mut recipe = recipe;
                recipe
                    .fields
                    .insert("shader".into(), serde_json::json!("toon_v1"));
                recipe.fields.insert(
                    "base_color".into(),
                    serde_json::json!([0.18, 0.72, 1.0, 1.0]),
                );
                recipe.fields.insert(
                    "light_direction".into(),
                    serde_json::json!([0.45, 1.0, 0.3]),
                );
                recipe.fields.insert(
                    "bands".into(),
                    serde_json::json!({
                        "shadow_threshold": 0.12,
                        "light_threshold": 0.52,
                        "shadow_level": 0.28,
                        "mid_level": 0.64
                    }),
                );
                recipe.fields.insert(
                    "rim".into(),
                    serde_json::json!({
                        "color": [0.55, 0.92, 1.0],
                        "strength": 0.48,
                        "threshold": 0.58,
                        "exponent": 2.4
                    }),
                );
                Self::Material(recipe)
            }
            ContentCategory::Ui => Self::Ui(recipe),
        }
    }

    pub fn category(&self) -> ContentCategory {
        match self {
            Self::Scene(_) => ContentCategory::Scene,
            Self::Character(_) => ContentCategory::Character,
            Self::Creature(_) => ContentCategory::Creature,
            Self::Biome(_) => ContentCategory::Biome,
            Self::Road(_) => ContentCategory::Road,
            Self::Building(_) => ContentCategory::Building,
            Self::Cave(_) => ContentCategory::Cave,
            Self::City(_) => ContentCategory::City,
            Self::Material(_) => ContentCategory::Material,
            Self::Ui(_) => ContentCategory::Ui,
        }
    }

    pub fn procedural_recipe(&self) -> Option<&ProceduralRecipeDraft> {
        match self {
            Self::Biome(recipe)
            | Self::Road(recipe)
            | Self::Building(recipe)
            | Self::Cave(recipe)
            | Self::City(recipe) => Some(recipe),
            _ => None,
        }
    }

    pub fn procedural_recipe_mut(&mut self) -> Option<&mut ProceduralRecipeDraft> {
        match self {
            Self::Biome(recipe)
            | Self::Road(recipe)
            | Self::Building(recipe)
            | Self::Cave(recipe)
            | Self::City(recipe) => Some(recipe),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenericRecipeDraft {
    #[serde(default = "current_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

impl Default for GenericRecipeDraft {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_PROJECT_SCHEMA,
            fields: BTreeMap::new(),
        }
    }
}

/// Deterministic source data for generated world content. Runtime entities and
/// Bevy asset handles are deliberately excluded; regeneration consumes this
/// recipe and resolves material slots through published stable content IDs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProceduralRecipeDraft {
    #[serde(default = "current_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub material_slots: BTreeMap<String, String>,
    #[serde(default)]
    pub spline_points: Vec<RoadSplinePointDraft>,
    #[serde(default)]
    pub road_junctions: Vec<RoadJunctionDraft>,
    #[serde(default)]
    pub topology_nodes: Vec<WorldTopologyNodeDraft>,
    #[serde(default)]
    pub topology_sockets: Vec<WorldTopologySocketDraft>,
    #[serde(default)]
    pub topology_edges: Vec<WorldTopologyEdgeDraft>,
    #[serde(default)]
    pub terrain_projection: TerrainProjectionDraft,
    #[serde(default)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

impl Default for ProceduralRecipeDraft {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_PROJECT_SCHEMA,
            seed: 0,
            revision: 0,
            material_slots: BTreeMap::new(),
            spline_points: Vec::new(),
            road_junctions: Vec::new(),
            topology_nodes: Vec::new(),
            topology_sockets: Vec::new(),
            topology_edges: Vec::new(),
            terrain_projection: TerrainProjectionDraft::default(),
            fields: BTreeMap::new(),
        }
    }
}

/// Maps recipe-local X/Z coordinates onto the shipped world terrain. Projecting
/// is an explicit authoring operation: legacy recipes keep their authored Y
/// values until the user invokes it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TerrainProjectionDraft {
    pub world_origin: [f32; 2],
    pub clearance: f32,
}

impl Default for TerrainProjectionDraft {
    fn default() -> Self {
        Self {
            world_origin: [0.0, 0.0],
            clearance: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoadSplinePointDraft {
    pub point_id: String,
    pub position: [f32; 3],
    #[serde(default = "one_f32")]
    pub width_scale: f32,
    /// Hermite derivative in recipe meters. Zero selects the deterministic
    /// automatic tangent derived from neighboring control points.
    #[serde(default)]
    pub tangent: [f32; 3],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoadJunctionKind {
    #[default]
    Merge,
    Split,
    Cross,
    LoopLink,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoadJunctionDraft {
    pub junction_id: String,
    pub from_point: String,
    pub to_point: String,
    #[serde(default)]
    pub kind: RoadJunctionKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorldTopologyNodeKind {
    #[default]
    Room,
    Entrance,
    FastTravel,
    Zone,
    Landmark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldTopologyNodeDraft {
    pub node_id: String,
    #[serde(default)]
    pub kind: WorldTopologyNodeKind,
    pub position: [f32; 3],
    pub size: [f32; 3],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorldTopologySocketKind {
    #[default]
    Doorway,
    StairLanding,
    Encounter,
    Item,
    Portal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldTopologySocketDraft {
    pub socket_id: String,
    pub node_id: String,
    #[serde(default)]
    pub kind: WorldTopologySocketKind,
    pub local_position: [f32; 3],
    pub facing: [f32; 3],
    pub size: [f32; 2],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorldTopologyEdgeKind {
    #[default]
    Door,
    Stair,
    Tunnel,
    Portal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldTopologyEdgeDraft {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub kind: WorldTopologyEdgeKind,
    #[serde(default)]
    pub from_socket: Option<String>,
    #[serde(default)]
    pub to_socket: Option<String>,
}

fn one_f32() -> f32 {
    1.0
}

pub fn resolved_road_tangent(points: &[RoadSplinePointDraft], index: usize) -> [f32; 3] {
    let Some(point) = points.get(index) else {
        return [0.0; 3];
    };
    if point.tangent.iter().any(|value| value.abs() > 1.0e-4) {
        return point.tangent;
    }
    if points.len() < 2 {
        return [0.0; 3];
    }
    let (from, to, scale) = if index == 0 {
        (points[0].position, points[1].position, 1.0)
    } else if index + 1 == points.len() {
        (points[index - 1].position, points[index].position, 1.0)
    } else {
        (points[index - 1].position, points[index + 1].position, 0.5)
    };
    [
        (to[0] - from[0]) * scale,
        (to[1] - from[1]) * scale,
        (to[2] - from[2]) * scale,
    ]
}

pub fn sample_road_spline_span(
    points: &[RoadSplinePointDraft],
    span: usize,
    t: f32,
) -> Option<[f32; 3]> {
    let start = points.get(span)?;
    let end = points.get(span + 1)?;
    let m0 = resolved_road_tangent(points, span);
    let m1 = resolved_road_tangent(points, span + 1);
    let t = t.clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    Some(std::array::from_fn(|axis| {
        h00 * start.position[axis] + h10 * m0[axis] + h01 * end.position[axis] + h11 * m1[axis]
    }))
}

pub fn procedural_material_slots(category: ContentCategory) -> &'static [&'static str] {
    match category {
        ContentCategory::Biome => &["terrain", "detail", "water", "hazard"],
        ContentCategory::Road => &["deck", "barrier", "support", "boost"],
        ContentCategory::Building => &["exterior", "interior", "floor", "trim"],
        ContentCategory::Cave => &["rock", "floor", "accent", "hazard"],
        ContentCategory::City => &["road", "exterior", "interior", "landmark"],
        _ => &[],
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProceduralParameterSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub default: f64,
    pub whole: bool,
}

macro_rules! parameter {
    ($key:literal, $label:literal, $min:expr, $max:expr, $step:expr, $default:expr) => {
        ProceduralParameterSpec {
            key: $key,
            label: $label,
            min: $min,
            max: $max,
            step: $step,
            default: $default,
            whole: false,
        }
    };
    (whole $key:literal, $label:literal, $min:expr, $max:expr, $step:expr, $default:expr) => {
        ProceduralParameterSpec {
            key: $key,
            label: $label,
            min: $min,
            max: $max,
            step: $step,
            default: $default,
            whole: true,
        }
    };
}

pub fn procedural_parameter_specs(category: ContentCategory) -> &'static [ProceduralParameterSpec] {
    const BIOME: [ProceduralParameterSpec; 5] = [
        parameter!("zone_radius", "Zone radius", 20.0, 500.0, 10.0, 120.0),
        parameter!("relief", "Terrain relief", 0.0, 1.0, 0.05, 0.55),
        parameter!("water_level", "Water level", -20.0, 20.0, 1.0, 0.0),
        parameter!("hazard_density", "Hazard density", 0.0, 1.0, 0.05, 0.10),
        parameter!(whole "landmark_count", "Landmarks", 0.0, 24.0, 1.0, 4.0),
    ];
    const ROAD: [ProceduralParameterSpec; 6] = [
        parameter!("width", "Road width", 8.0, 40.0, 1.0, 18.0),
        parameter!("max_grade_deg", "Maximum grade", 5.0, 45.0, 1.0, 22.0),
        parameter!("curve_radius", "Curve radius", 12.0, 180.0, 4.0, 60.0),
        parameter!(whole "loop_count", "Loops", 0.0, 8.0, 1.0, 1.0),
        parameter!(whole "jump_count", "Jumps", 0.0, 16.0, 1.0, 2.0),
        parameter!("support_spacing", "Support spacing", 20.0, 160.0, 5.0, 80.0),
    ];
    const BUILDING: [ProceduralParameterSpec; 6] = [
        parameter!("width", "Footprint width", 8.0, 80.0, 2.0, 24.0),
        parameter!("depth", "Footprint depth", 8.0, 80.0, 2.0, 20.0),
        parameter!(whole "floors", "Floors", 1.0, 20.0, 1.0, 4.0),
        parameter!(whole "rooms_per_floor", "Rooms per floor", 1.0, 12.0, 1.0, 4.0),
        parameter!("stair_width", "Stair width", 1.5, 6.0, 0.25, 3.0),
        parameter!(whole "entrance_count", "Entrances", 1.0, 6.0, 1.0, 2.0),
    ];
    const CAVE: [ProceduralParameterSpec; 6] = [
        parameter!("tunnel_width", "Tunnel width", 6.0, 30.0, 1.0, 12.0),
        parameter!(whole "room_count", "Rooms", 1.0, 24.0, 1.0, 6.0),
        parameter!(whole "vertical_layers", "Vertical layers", 1.0, 8.0, 1.0, 2.0),
        parameter!("secret_density", "Secret density", 0.0, 1.0, 0.05, 0.25),
        parameter!(whole "fast_travel_points", "Fast travel points", 0.0, 8.0, 1.0, 2.0),
        parameter!(
            "shared_camera_radius",
            "Shared camera radius",
            16.0,
            80.0,
            2.0,
            32.0
        ),
    ];
    const CITY: [ProceduralParameterSpec; 5] = [
        parameter!("block_size", "Block size", 30.0, 180.0, 5.0, 80.0),
        parameter!("road_width", "Road width", 6.0, 36.0, 1.0, 14.0),
        parameter!(
            "building_density",
            "Building density",
            0.10,
            1.0,
            0.05,
            0.65
        ),
        parameter!(whole "district_count", "Districts", 1.0, 12.0, 1.0, 4.0),
        parameter!(whole "landmark_count", "Landmarks", 1.0, 16.0, 1.0, 4.0),
    ];
    match category {
        ContentCategory::Biome => &BIOME,
        ContentCategory::Road => &ROAD,
        ContentCategory::Building => &BUILDING,
        ContentCategory::Cave => &CAVE,
        ContentCategory::City => &CITY,
        _ => &[],
    }
}

pub fn procedural_parameter_value(
    recipe: &ProceduralRecipeDraft,
    spec: ProceduralParameterSpec,
) -> f64 {
    recipe
        .fields
        .get(spec.key)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(spec.default)
}

fn current_schema() -> u32 {
    CURRENT_PROJECT_SCHEMA
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentCategory {
    Scene,
    Character,
    Creature,
    Biome,
    Road,
    Building,
    Cave,
    City,
    Material,
    Ui,
}

impl ContentCategory {
    fn id_prefix(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Character => "character",
            Self::Creature => "creature",
            Self::Biome => "biome",
            Self::Road => "road",
            Self::Building => "building",
            Self::Cave => "cave",
            Self::City => "city",
            Self::Material => "material",
            Self::Ui => "ui",
        }
    }

    fn source_folder(self) -> &'static str {
        match self {
            Self::Scene => "scenes",
            Self::Character => "characters",
            Self::Creature => "creatures",
            Self::Biome => "biomes",
            Self::Road => "roads",
            Self::Building => "buildings",
            Self::Cave => "caves",
            Self::City => "cities",
            Self::Material => "materials",
            Self::Ui => "ui",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EditorSceneDraft {
    #[serde(default)]
    pub objects: Vec<SceneObjectDraft>,
    #[serde(default)]
    pub adapter_overrides: Vec<AdapterOverrideDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneObjectDraft {
    pub editor_id: u64,
    pub name: String,
    pub primitive: DraftPrimitive,
    pub transform: TransformDraft,
    #[serde(default)]
    pub material_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DraftPrimitive {
    Empty,
    Cube,
    Pillar,
    Beacon,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterOverrideDraft {
    pub adapter_key: String,
    pub transform: TransformDraft,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TransformDraft {
    pub translation: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for TransformDraft {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectValidationError {
    UnsupportedSchema(u32),
    InvalidContentId(String),
    DuplicateContentId(String),
    InvalidSourcePath(String),
    DuplicateSourcePath(String),
    MissingPayload(String),
    PayloadCategoryMismatch(String),
    InvalidEditorId(u64),
    DuplicateEditorId(u64),
    InvalidAdapterKey(String),
    DuplicateAdapterKey(String),
    InvalidTransform(String),
    MissingDependency { owner: String, dependency: String },
    MissingContent(String),
    InvalidMaterial(String),
    InvalidMaterialBinding(String),
    InvalidProceduralRecipe(String),
}

#[derive(Debug)]
pub enum ProjectIoError {
    Io(String),
    Serialization(String),
    Validation(Vec<ProjectValidationError>),
    SourceConsistency(String),
    NoValidProject,
}

impl std::fmt::Display for ProjectIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::Serialization(message) => write!(formatter, "project data error: {message}"),
            Self::Validation(errors) => write!(formatter, "project validation failed: {errors:?}"),
            Self::SourceConsistency(message) => {
                write!(formatter, "content source consistency error: {message}")
            }
            Self::NoValidProject => write!(formatter, "no valid project or recovery snapshot"),
        }
    }
}

pub fn validate_project(project: &ForgeProject) -> Vec<ProjectValidationError> {
    let mut errors = Vec::new();
    if project.schema_version > CURRENT_PROJECT_SCHEMA {
        errors.push(ProjectValidationError::UnsupportedSchema(
            project.schema_version,
        ));
    }

    let mut content_ids = BTreeSet::new();
    let mut source_paths = BTreeSet::new();
    for record in &project.records {
        if record.content_id.trim().is_empty() {
            errors.push(ProjectValidationError::InvalidContentId(
                record.content_id.clone(),
            ));
        } else if !content_ids.insert(record.content_id.clone()) {
            errors.push(ProjectValidationError::DuplicateContentId(
                record.content_id.clone(),
            ));
        }
        if !safe_source_path(&record.source_path) {
            errors.push(ProjectValidationError::InvalidSourcePath(
                record.source_path.clone(),
            ));
        } else if !source_paths.insert(record.source_path.clone()) {
            errors.push(ProjectValidationError::DuplicateSourcePath(
                record.source_path.clone(),
            ));
        }
        match project.payloads.get(&record.content_id) {
            None => errors.push(ProjectValidationError::MissingPayload(
                record.content_id.clone(),
            )),
            Some(payload) if payload.category() != record.category => {
                errors.push(ProjectValidationError::PayloadCategoryMismatch(
                    record.content_id.clone(),
                ));
            }
            _ => {}
        }
        if let Some(ContentPayload::Material(recipe)) = project.payloads.get(&record.content_id) {
            validate_material_recipe(&record.content_id, recipe, &mut errors);
        }
        if let Some(recipe) = project
            .payloads
            .get(&record.content_id)
            .and_then(ContentPayload::procedural_recipe)
        {
            validate_procedural_recipe(&record.content_id, record.category, recipe, &mut errors);
            let allowed = procedural_material_slots(record.category);
            for (slot, material_id) in &recipe.material_slots {
                if !allowed.contains(&slot.as_str()) {
                    errors.push(ProjectValidationError::InvalidMaterialBinding(format!(
                        "{} uses unsupported {:?} material slot {}",
                        record.content_id, record.category, slot
                    )));
                    continue;
                }
                let valid = project
                    .records
                    .iter()
                    .find(|candidate| candidate.content_id == *material_id)
                    .is_some_and(|candidate| candidate.category == ContentCategory::Material)
                    && matches!(
                        project.payloads.get(material_id),
                        Some(ContentPayload::Material(_))
                    );
                if !valid {
                    errors.push(ProjectValidationError::InvalidMaterialBinding(format!(
                        "{} slot {} references missing/non-material {}",
                        record.content_id, slot, material_id
                    )));
                }
            }
        }
    }
    for record in &project.records {
        for dependency in &record.dependencies {
            if !content_ids.contains(dependency) {
                errors.push(ProjectValidationError::MissingDependency {
                    owner: record.content_id.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
    }

    let mut editor_ids = BTreeSet::new();
    for object in &project.scene.objects {
        if object.editor_id == 0 {
            errors.push(ProjectValidationError::InvalidEditorId(object.editor_id));
        } else if !editor_ids.insert(object.editor_id) {
            errors.push(ProjectValidationError::DuplicateEditorId(object.editor_id));
        }
        validate_transform(
            &object.transform,
            format!("editor object {}", object.editor_id),
            &mut errors,
        );
        if let Some(material_id) = &object.material_id {
            let valid = project
                .records
                .iter()
                .find(|record| record.content_id == *material_id)
                .is_some_and(|record| record.category == ContentCategory::Material)
                && matches!(
                    project.payloads.get(material_id),
                    Some(ContentPayload::Material(_))
                );
            if !valid {
                errors.push(ProjectValidationError::InvalidMaterialBinding(format!(
                    "editor object {} references missing/non-material {}",
                    object.editor_id, material_id
                )));
            }
        }
    }
    let mut adapter_keys = BTreeSet::new();
    for adapter in &project.scene.adapter_overrides {
        if adapter.adapter_key.trim().is_empty() {
            errors.push(ProjectValidationError::InvalidAdapterKey(
                adapter.adapter_key.clone(),
            ));
        } else if !adapter_keys.insert(adapter.adapter_key.clone()) {
            errors.push(ProjectValidationError::DuplicateAdapterKey(
                adapter.adapter_key.clone(),
            ));
        }
        validate_transform(
            &adapter.transform,
            format!("adapter {}", adapter.adapter_key),
            &mut errors,
        );
    }
    errors
}

fn validate_procedural_recipe(
    content_id: &str,
    category: ContentCategory,
    recipe: &ProceduralRecipeDraft,
    errors: &mut Vec<ProjectValidationError>,
) {
    if !recipe
        .terrain_projection
        .world_origin
        .iter()
        .all(|value| value.is_finite() && (-10_000.0..=10_000.0).contains(value))
        || !recipe.terrain_projection.clearance.is_finite()
        || !(-250.0..=250.0).contains(&recipe.terrain_projection.clearance)
    {
        errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
            "{content_id}: terrain projection origin must stay inside the 20km world and clearance inside -250–250m"
        )));
    }
    let specs = procedural_parameter_specs(category);
    for spec in specs {
        let Some(raw) = recipe.fields.get(spec.key) else {
            continue;
        };
        let Some(value) = raw.as_f64() else {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: {} must be numeric",
                spec.key
            )));
            continue;
        };
        if !value.is_finite() || value < spec.min || value > spec.max {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: {} must be within [{}, {}]",
                spec.key, spec.min, spec.max
            )));
        } else if spec.whole && value.fract().abs() > f64::EPSILON {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: {} must be a whole number",
                spec.key
            )));
        }
    }

    let value = |key: &str| {
        specs
            .iter()
            .find(|spec| spec.key == key)
            .map(|spec| procedural_parameter_value(recipe, *spec))
            .unwrap_or_default()
    };
    let budget_error = match category {
        ContentCategory::Road => {
            let estimated_segments = 12.0 + value("loop_count") * 16.0 + value("jump_count") * 3.0;
            (estimated_segments > 180.0).then(|| {
                format!("{content_id}: road preview exceeds the 180-segment generation budget")
            })
        }
        ContentCategory::Building => {
            let rooms = value("floors") * value("rooms_per_floor");
            (rooms > 160.0)
                .then(|| format!("{content_id}: building exceeds the 160-room navigation budget"))
        }
        ContentCategory::Cave => {
            let rooms = value("room_count") * value("vertical_layers");
            if rooms > 96.0 {
                Some(format!(
                    "{content_id}: cave exceeds the 96-room/layer generation budget"
                ))
            } else if value("shared_camera_radius") < value("tunnel_width") {
                Some(format!(
                    "{content_id}: shared camera radius must cover the tunnel width"
                ))
            } else {
                None
            }
        }
        ContentCategory::City => (value("road_width") >= value("block_size") * 0.5).then(|| {
            format!("{content_id}: city road width must be less than half the block size")
        }),
        _ => None,
    };
    if let Some(message) = budget_error {
        errors.push(ProjectValidationError::InvalidProceduralRecipe(message));
    }
    validate_procedural_topology(content_id, category, recipe, errors);
}

fn validate_procedural_topology(
    content_id: &str,
    category: ContentCategory,
    recipe: &ProceduralRecipeDraft,
    errors: &mut Vec<ProjectValidationError>,
) {
    if category == ContentCategory::Road {
        if recipe.spline_points.len() == 1 || recipe.spline_points.len() > 64 {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: road spline must contain 0 or 2–64 points"
            )));
        }
        let mut ids = BTreeSet::new();
        for point in &recipe.spline_points {
            if point.point_id.trim().is_empty() || !ids.insert(point.point_id.clone()) {
                errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                    "{content_id}: spline point IDs must be nonempty and unique"
                )));
            }
            if !point.position.iter().all(|value| value.is_finite())
                || !point.tangent.iter().all(|value| value.is_finite())
                || !point.width_scale.is_finite()
                || !(0.5..=2.0).contains(&point.width_scale)
                || point.tangent.iter().map(|value| value * value).sum::<f32>() > 250_000.0
            {
                errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                    "{content_id}: spline point {} has invalid position/tangent/width scale",
                    point.point_id
                )));
            }
        }
        let max_grade = procedural_parameter_specs(category)
            .iter()
            .find(|spec| spec.key == "max_grade_deg")
            .map(|spec| procedural_parameter_value(recipe, *spec))
            .unwrap_or(22.0)
            .to_radians();
        for points in recipe.spline_points.windows(2) {
            let a = points[0].position;
            let b = points[1].position;
            let dx = (b[0] - a[0]) as f64;
            let dy = (b[1] - a[1]) as f64;
            let dz = (b[2] - a[2]) as f64;
            let horizontal = dx.hypot(dz);
            let length = horizontal.hypot(dy);
            if !(2.0..=500.0).contains(&length) || horizontal <= 0.25 {
                errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                    "{content_id}: spline segments must be 2–500m with horizontal travel"
                )));
            } else if dy.abs().atan2(horizontal) > max_grade + 1.0e-6 {
                errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                    "{content_id}: spline segment exceeds maximum road grade"
                )));
            }
        }
        for span in 0..recipe.spline_points.len().saturating_sub(1) {
            let mut previous = recipe.spline_points[span].position;
            for sample in 1..=8 {
                let current =
                    sample_road_spline_span(&recipe.spline_points, span, sample as f32 / 8.0)
                        .unwrap_or(previous);
                let dx = (current[0] - previous[0]) as f64;
                let dy = (current[1] - previous[1]) as f64;
                let dz = (current[2] - previous[2]) as f64;
                let horizontal = dx.hypot(dz);
                if horizontal <= 0.01 || dy.abs().atan2(horizontal) > max_grade + 1.0e-6 {
                    errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                        "{content_id}: curved spline span {span} exceeds maximum road grade"
                    )));
                    break;
                }
                previous = current;
            }
        }

        if recipe.road_junctions.len() > 64 {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: road junctions exceed the 64-link compiler budget"
            )));
        }
        let point_ids = recipe
            .spline_points
            .iter()
            .map(|point| point.point_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut junction_ids = BTreeSet::new();
        let mut junction_links = BTreeSet::new();
        for junction in &recipe.road_junctions {
            let (a, b) = if junction.from_point <= junction.to_point {
                (&junction.from_point, &junction.to_point)
            } else {
                (&junction.to_point, &junction.from_point)
            };
            let link = format!("{:?}:{a}:{b}", junction.kind);
            if junction.junction_id.trim().is_empty()
                || !junction_ids.insert(junction.junction_id.as_str())
                || junction.from_point == junction.to_point
                || !point_ids.contains(junction.from_point.as_str())
                || !point_ids.contains(junction.to_point.as_str())
                || !junction_links.insert(link)
            {
                errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                    "{content_id}: road junction IDs/links must be unique and reference two points"
                )));
            }
        }
    } else if !recipe.spline_points.is_empty() {
        errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
            "{content_id}: spline points are only valid for Road recipes"
        )));
    } else if !recipe.road_junctions.is_empty() {
        errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
            "{content_id}: road junctions are only valid for Road recipes"
        )));
    }

    if recipe.topology_nodes.len() > 96
        || recipe.topology_sockets.len() > 192
        || recipe.topology_edges.len() > 192
        || recipe.topology_nodes.len() + recipe.topology_sockets.len() + recipe.topology_edges.len()
            > 192
    {
        errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
            "{content_id}: topology exceeds the 96-node/192-part compiler budget"
        )));
        return;
    }
    let mut nodes = BTreeMap::new();
    for node in &recipe.topology_nodes {
        if node.node_id.trim().is_empty() || nodes.insert(node.node_id.clone(), node).is_some() {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: topology node IDs must be nonempty and unique"
            )));
        }
        if !node.position.iter().all(|value| value.is_finite())
            || !node
                .size
                .iter()
                .all(|value| value.is_finite() && *value > 0.25 && *value <= 250.0)
        {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: topology node {} has invalid position/size",
                node.node_id
            )));
        }
    }
    let mut sockets = BTreeMap::new();
    for socket in &recipe.topology_sockets {
        let node = nodes.get(&socket.node_id).copied();
        let facing_length = socket.facing.iter().map(|value| value * value).sum::<f32>();
        let inside_node = node.is_some_and(|node| {
            (0..3).all(|axis| socket.local_position[axis].abs() <= node.size[axis] * 0.5 + 0.01)
        });
        if socket.socket_id.trim().is_empty()
            || sockets.insert(socket.socket_id.clone(), socket).is_some()
            || node.is_none()
            || !socket.local_position.iter().all(|value| value.is_finite())
            || !inside_node
            || !socket.facing.iter().all(|value| value.is_finite())
            || !(0.25..=4.0).contains(&facing_length)
            || !socket
                .size
                .iter()
                .all(|value| value.is_finite() && (0.25..=20.0).contains(value))
        {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: topology socket {} has invalid ID/node/transform/size",
                socket.socket_id
            )));
        }
    }
    let mut adjacency = BTreeMap::<String, Vec<String>>::new();
    let mut edge_ids = BTreeSet::new();
    for edge in &recipe.topology_edges {
        let from_socket_valid = edge.from_socket.as_ref().is_none_or(|socket_id| {
            sockets
                .get(socket_id)
                .is_some_and(|socket| socket.node_id == edge.from)
        });
        let to_socket_valid = edge.to_socket.as_ref().is_none_or(|socket_id| {
            sockets
                .get(socket_id)
                .is_some_and(|socket| socket.node_id == edge.to)
        });
        let valid = edge.from != edge.to
            && nodes.contains_key(&edge.from)
            && nodes.contains_key(&edge.to)
            && from_socket_valid
            && to_socket_valid;
        let (a, b) = if edge.from <= edge.to {
            (&edge.from, &edge.to)
        } else {
            (&edge.to, &edge.from)
        };
        let edge_id = format!("{:?}:{a}:{b}", edge.kind);
        if !valid || !edge_ids.insert(edge_id) {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: topology edges must be unique and reference two valid nodes"
            )));
            continue;
        }
        adjacency
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
        adjacency
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
    }

    if !recipe.topology_nodes.is_empty()
        && matches!(category, ContentCategory::Building | ContentCategory::Cave)
    {
        if !recipe
            .topology_nodes
            .iter()
            .any(|node| node.kind == WorldTopologyNodeKind::Entrance)
        {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: navigable topology requires an entrance"
            )));
        }
        let start = recipe.topology_nodes[0].node_id.clone();
        let mut visited = BTreeSet::from([start.clone()]);
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            for neighbor in adjacency.get(&node).into_iter().flatten() {
                if visited.insert(neighbor.clone()) {
                    queue.push_back(neighbor.clone());
                }
            }
        }
        if visited.len() != recipe.topology_nodes.len() {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: every room must be reachable from the topology entrance"
            )));
        }
        for (index, left) in recipe.topology_nodes.iter().enumerate() {
            for right in recipe.topology_nodes.iter().skip(index + 1) {
                let overlaps = (0..3).all(|axis| {
                    (left.position[axis] - right.position[axis]).abs()
                        < (left.size[axis] + right.size[axis]) * 0.5 - 0.25
                });
                if overlaps {
                    errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                        "{content_id}: rooms {} and {} overlap before collision build",
                        left.node_id, right.node_id
                    )));
                }
            }
        }
    }
    if category == ContentCategory::Cave && !recipe.topology_nodes.is_empty() {
        let requested = procedural_parameter_specs(category)
            .iter()
            .find(|spec| spec.key == "fast_travel_points")
            .map(|spec| procedural_parameter_value(recipe, *spec) as usize)
            .unwrap_or_default();
        let authored = recipe
            .topology_nodes
            .iter()
            .filter(|node| node.kind == WorldTopologyNodeKind::FastTravel)
            .count();
        if authored < requested {
            errors.push(ProjectValidationError::InvalidProceduralRecipe(format!(
                "{content_id}: cave requests {requested} fast-travel points but authors {authored}"
            )));
        }
    }
}

fn validate_material_recipe(
    content_id: &str,
    recipe: &GenericRecipeDraft,
    errors: &mut Vec<ProjectValidationError>,
) {
    let shader = recipe
        .fields
        .get("shader")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("toon_v1");
    let bounds: &[(&str, f64, f64)] = match shader {
        "toon_v1" => &[
            ("shadow_threshold", 0.0, 0.8),
            ("light_threshold", 0.2, 1.0),
            ("shadow_level", 0.05, 0.8),
            ("mid_level", 0.2, 0.95),
            ("rim_strength", 0.0, 1.5),
            ("rim_exponent", 0.5, 8.0),
        ],
        "water_v1" => &[
            ("wave_speed", 0.0, 3.0),
            ("wave_frequency", 0.005, 0.3),
            ("wave_amplitude", 0.0, 0.8),
            ("secondary_wave", 0.0, 1.5),
            ("fresnel_exponent", 0.5, 8.0),
            ("opacity", 0.15, 1.0),
        ],
        "energy_v1" => &[
            ("flow_speed", 0.0, 8.0),
            ("spatial_frequency", 0.5, 14.0),
            ("pulse_speed", 0.0, 12.0),
            ("opacity", 0.1, 1.0),
        ],
        "shield_v1" => &[
            ("grid_scale", 2.0, 18.0),
            ("line_width", 0.02, 0.24),
            ("pulse_speed", 0.0, 10.0),
            ("opacity", 0.08, 0.9),
            ("fresnel_exponent", 0.5, 8.0),
            ("edge_strength", 0.0, 2.0),
        ],
        "ice_v1" => &[
            ("detail_scale", 0.02, 1.0),
            ("frost_coverage", 0.0, 1.0),
            ("sparkle_strength", 0.0, 1.5),
            ("fresnel_strength", 0.0, 1.5),
        ],
        "lava_v1" => &[
            ("flow_speed", 0.0, 2.0),
            ("cell_scale", 0.03, 0.8),
            ("seam_width", 0.04, 0.7),
            ("emissive_strength", 0.2, 2.0),
        ],
        other => {
            errors.push(ProjectValidationError::InvalidMaterial(format!(
                "{content_id}: unsupported shader {other}"
            )));
            return;
        }
    };
    if let Some(preset) = recipe.fields.get("preset") {
        let valid = preset.as_u64().is_some_and(|value| value < 3);
        if !valid {
            errors.push(ProjectValidationError::InvalidMaterial(format!(
                "{content_id}: preset must be 0, 1, or 2"
            )));
        }
    }
    for (key, min, max) in bounds {
        let Some(raw) = recipe.fields.get(*key) else {
            continue;
        };
        let valid = raw
            .as_f64()
            .is_some_and(|value| value.is_finite() && (*min..=*max).contains(&value));
        if !valid {
            errors.push(ProjectValidationError::InvalidMaterial(format!(
                "{content_id}: {key} must be numeric in [{min}, {max}]"
            )));
        }
    }
}

fn safe_source_path(path: &str) -> bool {
    !path.trim().is_empty()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn validate_transform(
    transform: &TransformDraft,
    owner: String,
    errors: &mut Vec<ProjectValidationError>,
) {
    let finite = transform
        .translation
        .iter()
        .chain(transform.rotation_xyzw.iter())
        .chain(transform.scale.iter())
        .all(|value| value.is_finite());
    let rotation_length_squared = transform
        .rotation_xyzw
        .iter()
        .map(|value| value * value)
        .sum::<f32>();
    if !finite || rotation_length_squared <= f32::EPSILON {
        errors.push(ProjectValidationError::InvalidTransform(owner));
    }
}

pub fn migrate_project(mut project: ForgeProject) -> Result<ForgeProject, ProjectIoError> {
    if project.schema_version > CURRENT_PROJECT_SCHEMA {
        return Err(ProjectIoError::Validation(vec![
            ProjectValidationError::UnsupportedSchema(project.schema_version),
        ]));
    }
    if project.schema_version == 0 {
        project.schema_version = 1;
        for record in &mut project.records {
            if record.schema_version == 0 {
                record.schema_version = 1;
            }
        }
    }
    let legacy_scene = project.scene.clone();
    for record in &project.records {
        project
            .payloads
            .entry(record.content_id.clone())
            .or_insert_with(|| {
                if record.category == ContentCategory::Scene {
                    ContentPayload::Scene(legacy_scene.clone())
                } else {
                    ContentPayload::empty(record.category)
                }
            });
    }
    if project
        .records
        .iter()
        .any(|record| record.category == ContentCategory::Scene && record.draft_hash.is_empty())
    {
        project.refresh_hashes()?;
    }
    Ok(project)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectLoadSource {
    Primary,
    Recovery(usize),
}

/// PM2 lock document: a reproducibility snapshot written next to the project
/// file on every successful save. It captures the engine/build identity,
/// project schema, every record's source hashes and dependency ids, and every
/// generator payload's fingerprint (category, schema, seed, revision) so a
/// project state can be compared, diffed, and regenerated deterministically.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectLockDocument {
    pub lock_schema: u32,
    pub engine_version: String,
    pub project_id: String,
    pub project_schema: u32,
    pub records: Vec<LockRecord>,
    pub generators: Vec<LockGenerator>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockRecord {
    pub content_id: String,
    pub schema_version: u32,
    pub source_path: String,
    pub draft_hash: String,
    pub published_hash: Option<String>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockGenerator {
    pub content_id: String,
    pub generator: String,
    pub schema_version: u32,
    pub seed: u64,
    pub revision: u64,
}

pub const CURRENT_LOCK_SCHEMA: u32 = 1;

impl ProjectLockDocument {
    pub fn from_project(project: &ForgeProject) -> Self {
        let mut records: Vec<LockRecord> = project
            .records
            .iter()
            .map(|record| LockRecord {
                content_id: record.content_id.clone(),
                schema_version: record.schema_version,
                source_path: record.source_path.clone(),
                draft_hash: record.draft_hash.clone(),
                published_hash: record.published_hash.clone(),
                dependencies: record.dependencies.clone(),
            })
            .collect();
        records.sort_by(|a, b| a.content_id.cmp(&b.content_id));

        // `payloads` is a BTreeMap, so generator order is already
        // deterministic by content id.
        let generators = project
            .payloads
            .iter()
            .filter_map(|(content_id, payload)| {
                let generator = format!("{:?}", payload.category()).to_lowercase();
                let (schema_version, seed, revision) = match payload.procedural_recipe() {
                    Some(recipe) => (recipe.schema_version, recipe.seed, recipe.revision),
                    None => match payload {
                        ContentPayload::Scene(_) => (CURRENT_PROJECT_SCHEMA, 0, 0),
                        ContentPayload::Character(recipe)
                        | ContentPayload::Creature(recipe)
                        | ContentPayload::Material(recipe)
                        | ContentPayload::Ui(recipe) => (recipe.schema_version, 0, 0),
                        _ => return None,
                    },
                };
                Some(LockGenerator {
                    content_id: content_id.clone(),
                    generator,
                    schema_version,
                    seed,
                    revision,
                })
            })
            .collect();

        Self {
            lock_schema: CURRENT_LOCK_SCHEMA,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            project_id: project.project_id.clone(),
            project_schema: project.schema_version,
            records,
            generators,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectStore {
    path: PathBuf,
    recovery_limit: usize,
}

impl ProjectStore {
    pub fn new(path: impl Into<PathBuf>, recovery_limit: usize) -> Self {
        Self {
            path: path.into(),
            recovery_limit,
        }
    }

    pub fn default_workspace() -> Self {
        Self::new(DEFAULT_PROJECT_FILE, 3)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn source_diagnostics(&self, project: &ForgeProject) -> Vec<String> {
        let root = self.path.parent().unwrap_or_else(|| Path::new("."));
        let mut diagnostics = Vec::new();
        for record in &project.records {
            let path = root.join(&record.source_path);
            let document = match fs::read(&path) {
                Ok(bytes) => match serde_json::from_slice::<RecordSourceDocument>(&bytes) {
                    Ok(document) => document,
                    Err(error) => {
                        diagnostics.push(format!(
                            "{} is not a valid record source: {error}",
                            record.source_path
                        ));
                        continue;
                    }
                },
                Err(error) => {
                    diagnostics.push(format!("{} is unavailable: {error}", record.source_path));
                    continue;
                }
            };
            if document.content_id != record.content_id {
                diagnostics.push(format!(
                    "{} declares {} instead of {}",
                    record.source_path, document.content_id, record.content_id
                ));
            }
            if document.schema_version > CURRENT_PROJECT_SCHEMA {
                diagnostics.push(format!(
                    "{} uses unsupported schema {}",
                    record.source_path, document.schema_version
                ));
            }
            if document.payload.category() != record.category {
                diagnostics.push(format!(
                    "{} contains a {:?} payload instead of {:?}",
                    record.source_path,
                    document.payload.category(),
                    record.category
                ));
            }
            match payload_hash(&document.payload) {
                Ok(hash) if hash != record.draft_hash => diagnostics.push(format!(
                    "{} hash {} does not match manifest {}",
                    record.source_path, hash, record.draft_hash
                )),
                Err(error) => diagnostics.push(error.to_string()),
                _ => {}
            }
        }
        diagnostics
    }

    pub fn move_source_to_canonical(
        &self,
        project: &mut ForgeProject,
        content_id: &str,
    ) -> Result<String, ProjectMutationError> {
        let Some(record) = project
            .records
            .iter()
            .find(|record| record.content_id == content_id)
        else {
            return Err(ProjectMutationError::MissingContent(content_id.into()));
        };
        let base = format!(
            "{}/{}.json",
            record.category.source_folder(),
            record.content_id.replace('.', "_")
        );
        let canonical = unique_value(&base, |candidate| {
            project
                .records
                .iter()
                .any(|other| other.content_id != content_id && other.source_path == candidate)
        });
        self.move_source(project, content_id, &canonical)?;
        Ok(canonical)
    }

    pub fn move_source(
        &self,
        project: &mut ForgeProject,
        content_id: &str,
        new_source_path: &str,
    ) -> Result<(), ProjectMutationError> {
        if !safe_source_path(new_source_path) {
            return Err(ProjectMutationError::InvalidSourcePath(
                new_source_path.into(),
            ));
        }
        let Some(index) = project
            .records
            .iter()
            .position(|record| record.content_id == content_id)
        else {
            return Err(ProjectMutationError::MissingContent(content_id.into()));
        };
        if project
            .records
            .iter()
            .enumerate()
            .any(|(other_index, record)| {
                other_index != index && record.source_path == new_source_path
            })
        {
            return Err(ProjectMutationError::SourceAlreadyExists(
                new_source_path.into(),
            ));
        }
        let old_source_path = project.records[index].source_path.clone();
        if old_source_path == new_source_path {
            return Ok(());
        }
        let root = self.path.parent().unwrap_or_else(|| Path::new("."));
        let old_path = root.join(&old_source_path);
        let new_path = root.join(new_source_path);
        if new_path.exists() {
            return Err(ProjectMutationError::SourceAlreadyExists(
                new_source_path.into(),
            ));
        }
        if old_path.exists() {
            if let Some(parent) = new_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| ProjectMutationError::Io(error.to_string()))?;
            }
            fs::rename(&old_path, &new_path)
                .map_err(|error| ProjectMutationError::Io(error.to_string()))?;
            if let Some(parent) = new_path.parent() {
                if let Err(error) = File::open(parent).and_then(|directory| directory.sync_all()) {
                    let _ = fs::rename(&new_path, &old_path);
                    return Err(ProjectMutationError::Io(error.to_string()));
                }
            }
        }
        project.records[index].source_path = new_source_path.into();
        Ok(())
    }

    pub fn save(&self, project: &mut ForgeProject) -> Result<(), ProjectIoError> {
        project.refresh_hashes()?;
        let errors = validate_project(project);
        if !errors.is_empty() {
            return Err(ProjectIoError::Validation(errors));
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        self.rotate_recoveries()?;
        self.write_record_sources(project)?;
        let bytes = serde_json::to_vec_pretty(project)
            .map_err(|error| ProjectIoError::Serialization(error.to_string()))?;
        atomic_write(&self.path, &bytes)?;
        let lock = ProjectLockDocument::from_project(project);
        let lock_bytes = serde_json::to_vec_pretty(&lock)
            .map_err(|error| ProjectIoError::Serialization(error.to_string()))?;
        atomic_write(&self.lock_path(), &lock_bytes)
    }

    /// The PM2 lock document path: `project.lock.json` beside `project.json`.
    pub fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock.json")
    }

    pub fn load_lock(&self) -> Result<ProjectLockDocument, ProjectIoError> {
        let bytes = fs::read(self.lock_path()).map_err(io_error)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| ProjectIoError::Serialization(error.to_string()))
    }

    pub fn load(&self) -> Result<ForgeProject, ProjectIoError> {
        load_project_file(&self.path)
    }

    pub fn load_recovery(&self, index: usize) -> Result<ForgeProject, ProjectIoError> {
        if index == 0 || index > self.recovery_limit {
            return Err(ProjectIoError::NoValidProject);
        }
        load_project_file(&self.recovery_path(index))
    }

    pub fn load_with_recovery(&self) -> Result<(ForgeProject, ProjectLoadSource), ProjectIoError> {
        if let Ok(project) = self.load() {
            return Ok((project, ProjectLoadSource::Primary));
        }
        for index in 1..=self.recovery_limit {
            if let Ok(project) = load_project_file(&self.recovery_path(index)) {
                return Ok((project, ProjectLoadSource::Recovery(index)));
            }
        }
        Err(ProjectIoError::NoValidProject)
    }

    fn rotate_recoveries(&self) -> Result<(), ProjectIoError> {
        if self.recovery_limit == 0 || !self.path.exists() {
            return Ok(());
        }
        for index in (2..=self.recovery_limit).rev() {
            let previous = self.recovery_path(index - 1);
            let next = self.recovery_path(index);
            if previous.exists() {
                fs::copy(previous, next).map_err(io_error)?;
            }
        }
        fs::copy(&self.path, self.recovery_path(1)).map_err(io_error)?;
        Ok(())
    }

    fn recovery_path(&self, index: usize) -> PathBuf {
        let mut value = self.path.as_os_str().to_owned();
        value.push(format!(".recovery.{index}"));
        PathBuf::from(value)
    }

    fn write_record_sources(&self, project: &ForgeProject) -> Result<(), ProjectIoError> {
        let root = self.path.parent().unwrap_or_else(|| Path::new("."));
        for record in &project.records {
            let payload = project
                .payloads
                .get(&record.content_id)
                .cloned()
                .ok_or_else(|| {
                    ProjectIoError::SourceConsistency(format!(
                        "{} has no payload",
                        record.content_id
                    ))
                })?;
            let document = RecordSourceDocument {
                content_id: record.content_id.clone(),
                schema_version: record.schema_version,
                payload,
            };
            let bytes = serde_json::to_vec_pretty(&document)
                .map_err(|error| ProjectIoError::Serialization(error.to_string()))?;
            atomic_write(&root.join(&record.source_path), &bytes)?;
        }
        Ok(())
    }
}

fn load_project_file(path: &Path) -> Result<ForgeProject, ProjectIoError> {
    let bytes = fs::read(path).map_err(io_error)?;
    let project: ForgeProject = serde_json::from_slice(&bytes)
        .map_err(|error| ProjectIoError::Serialization(error.to_string()))?;
    let mut project = migrate_project(project)?;
    let errors = validate_project(&project);
    if !errors.is_empty() {
        return Err(ProjectIoError::Validation(errors));
    }
    hydrate_record_sources(path, &mut project)?;
    Ok(project)
}

fn hydrate_record_sources(
    manifest_path: &Path,
    project: &mut ForgeProject,
) -> Result<(), ProjectIoError> {
    let root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    for record in project.records.clone() {
        let embedded_payload = project
            .payloads
            .get(&record.content_id)
            .cloned()
            .or_else(|| {
                (record.category == ContentCategory::Scene)
                    .then(|| ContentPayload::Scene(project.scene.clone()))
            });
        let embedded_hash = embedded_payload.as_ref().map(payload_hash).transpose()?;
        let source_path = root.join(&record.source_path);
        let source_result = fs::read(&source_path)
            .map_err(io_error)
            .and_then(|bytes| {
                serde_json::from_slice::<RecordSourceDocument>(&bytes)
                    .map_err(|error| ProjectIoError::Serialization(error.to_string()))
            })
            .and_then(|document| {
                if document.content_id != record.content_id {
                    return Err(ProjectIoError::SourceConsistency(format!(
                        "{} declares content ID {} instead of {}",
                        record.source_path, document.content_id, record.content_id
                    )));
                }
                if document.schema_version > CURRENT_PROJECT_SCHEMA {
                    return Err(ProjectIoError::SourceConsistency(format!(
                        "{} uses unsupported schema {}",
                        record.source_path, document.schema_version
                    )));
                }
                if document.payload.category() != record.category {
                    return Err(ProjectIoError::SourceConsistency(format!(
                        "{} contains a {:?} payload instead of {:?}",
                        record.source_path,
                        document.payload.category(),
                        record.category
                    )));
                }
                Ok(document.payload)
            });

        if let Ok(payload) = source_result {
            if payload_hash(&payload)? == record.draft_hash {
                if let ContentPayload::Scene(scene) = &payload {
                    project.scene = scene.clone();
                }
                project.payloads.insert(record.content_id.clone(), payload);
                continue;
            }
        }
        if embedded_hash.as_deref() == Some(record.draft_hash.as_str()) {
            continue;
        }
        return Err(ProjectIoError::SourceConsistency(format!(
            "{} and its embedded recovery copy do not match draft hash {}",
            record.source_path, record.draft_hash
        )));
    }
    Ok(())
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ProjectIoError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let mut file = File::create(&temp).map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temp, path).map_err(io_error)?;
    if let Some(parent) = path.parent() {
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(io_error)?;
    }
    Ok(())
}

fn scene_hash(scene: &EditorSceneDraft) -> Result<String, ProjectIoError> {
    let scene_json = serde_json::to_vec(scene)
        .map_err(|error| ProjectIoError::Serialization(error.to_string()))?;
    Ok(fnv1a_hash(&scene_json))
}

fn payload_hash(payload: &ContentPayload) -> Result<String, ProjectIoError> {
    match payload {
        ContentPayload::Scene(scene) => scene_hash(scene),
        _ => {
            let bytes = serde_json::to_vec(payload)
                .map_err(|error| ProjectIoError::Serialization(error.to_string()))?;
            Ok(fnv1a_hash(&bytes))
        }
    }
}

fn fnv1a_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn io_error(error: std::io::Error) -> ProjectIoError {
    ProjectIoError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_store(label: &str) -> (PathBuf, ProjectStore) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "starfall_forge_{label}_{}_{nonce}",
            std::process::id()
        ));
        let store = ProjectStore::new(root.join("project.json"), 3);
        (root, store)
    }

    #[test]
    fn save_writes_lock_document_with_hashes_and_generator_fingerprints() {
        let (root, store) = test_store("lock_doc");
        let mut project = ForgeProject::default();
        let recipe = ProceduralRecipeDraft {
            seed: 42_195,
            revision: 7,
            ..Default::default()
        };
        project.records.push(ContentRecord {
            content_id: "biome.test_ridge".into(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Biome,
            source_path: "biomes/test_ridge.json".into(),
            dependencies: vec!["scene.main_world".into()],
            display_name: "Test Ridge".into(),
            tags: Vec::new(),
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        });
        project
            .payloads
            .insert("biome.test_ridge".into(), ContentPayload::Biome(recipe));

        store.save(&mut project).unwrap();
        assert!(store.lock_path().is_file());

        let lock = store.load_lock().unwrap();
        assert_eq!(lock.lock_schema, CURRENT_LOCK_SCHEMA);
        assert_eq!(lock.engine_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(lock.project_id, project.project_id);
        assert_eq!(lock.records.len(), project.records.len());

        // Records are sorted by content id and carry the refreshed hashes.
        assert!(lock
            .records
            .windows(2)
            .all(|pair| pair[0].content_id <= pair[1].content_id));
        let biome_record = lock
            .records
            .iter()
            .find(|record| record.content_id == "biome.test_ridge")
            .expect("lock should include the biome record");
        assert_eq!(biome_record.dependencies, vec!["scene.main_world"]);
        let scene_record = lock
            .records
            .iter()
            .find(|record| record.content_id == "scene.main_world")
            .expect("lock should include the scene record");
        assert!(!scene_record.draft_hash.is_empty());

        let generator = lock
            .generators
            .iter()
            .find(|generator| generator.content_id == "biome.test_ridge")
            .expect("lock should fingerprint the biome generator");
        assert_eq!(generator.generator, "biome");
        assert_eq!(generator.seed, 42_195);
        assert_eq!(generator.revision, 7);

        // Same project state saves to an identical lock document.
        store.save(&mut project).unwrap();
        assert_eq!(store.load_lock().unwrap(), lock);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_round_trip_refreshes_scene_hash() {
        let (root, store) = test_store("round_trip");
        let mut project = ForgeProject::default();
        project.scene.objects.push(SceneObjectDraft {
            editor_id: 8,
            name: "Test Block".into(),
            primitive: DraftPrimitive::Cube,
            transform: TransformDraft::default(),
            material_id: None,
        });
        store.save(&mut project).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.scene, project.scene);
        assert!(!loaded.records[0].draft_hash.is_empty());
        assert!(root.join("scenes/main_world.json").is_file());
        assert!(store.source_diagnostics(&loaded).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn matching_record_source_is_authoritative_over_embedded_copy() {
        let (root, store) = test_store("source_authority");
        let mut project = ForgeProject::default();
        project.scene.objects.push(SceneObjectDraft {
            editor_id: 9,
            name: "Source object".into(),
            primitive: DraftPrimitive::Beacon,
            transform: TransformDraft::default(),
            material_id: None,
        });
        store.save(&mut project).unwrap();

        let mut manifest: ForgeProject =
            serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        manifest.scene = EditorSceneDraft::default();
        fs::write(store.path(), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.scene.objects[0].editor_id, 9);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_cross_file_save_uses_matching_embedded_generation() {
        let (root, store) = test_store("cross_file_recovery");
        let mut first = ForgeProject::default();
        first.scene.objects.push(SceneObjectDraft {
            editor_id: 1,
            name: "First generation".into(),
            primitive: DraftPrimitive::Cube,
            transform: TransformDraft::default(),
            material_id: None,
        });
        store.save(&mut first).unwrap();
        let mut second = first.clone();
        second.scene.objects[0].name = "Second generation".into();
        store.save(&mut second).unwrap();

        fs::copy(store.recovery_path(1), store.path()).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.scene.objects[0].name, "First generation");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_primary_recovers_previous_valid_snapshot() {
        let (root, store) = test_store("recovery");
        let mut first = ForgeProject {
            display_name: "First".into(),
            ..ForgeProject::default()
        };
        store.save(&mut first).unwrap();
        let mut second = first.clone();
        second.display_name = "Second".into();
        store.save(&mut second).unwrap();
        fs::write(store.path(), b"not json").unwrap();

        let (loaded, source) = store.load_with_recovery().unwrap();
        assert_eq!(loaded.display_name, "First");
        assert_eq!(source, ProjectLoadSource::Recovery(1));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_temp_file_does_not_replace_primary() {
        let (root, store) = test_store("interrupted");
        let mut project = ForgeProject::default();
        store.save(&mut project).unwrap();
        let mut temp = store.path().as_os_str().to_owned();
        temp.push(".tmp");
        fs::write(PathBuf::from(temp), b"partial").unwrap();
        assert_eq!(store.load().unwrap().project_id, project.project_id);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_ids_and_missing_dependencies_are_rejected() {
        let mut project = ForgeProject::default();
        let mut duplicate = project.records[0].clone();
        duplicate.dependencies.push("missing.asset".into());
        project.records.push(duplicate);
        project.scene.objects.extend([
            SceneObjectDraft {
                editor_id: 4,
                name: "A".into(),
                primitive: DraftPrimitive::Empty,
                transform: TransformDraft::default(),
                material_id: None,
            },
            SceneObjectDraft {
                editor_id: 4,
                name: "B".into(),
                primitive: DraftPrimitive::Empty,
                transform: TransformDraft::default(),
                material_id: None,
            },
        ]);
        let errors = validate_project(&project);
        assert!(errors
            .iter()
            .any(|error| matches!(error, ProjectValidationError::DuplicateContentId(_))));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ProjectValidationError::DuplicateEditorId(4))));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ProjectValidationError::MissingDependency { .. })));
    }

    #[test]
    fn rename_migrates_all_references() {
        let mut project = ForgeProject::default();
        project.records.push(ContentRecord {
            content_id: "scene.consumer".into(),
            schema_version: 1,
            category: ContentCategory::Scene,
            source_path: "consumer.json".into(),
            dependencies: vec!["scene.main_world".into()],
            display_name: "Consumer".into(),
            tags: Vec::new(),
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        });
        project.payloads.insert(
            "scene.consumer".into(),
            ContentPayload::Scene(EditorSceneDraft::default()),
        );
        project
            .rename_content("scene.main_world", "scene.overworld")
            .unwrap();
        assert_eq!(project.records[0].content_id, "scene.overworld");
        assert_eq!(project.records[1].dependencies, ["scene.overworld"]);
        assert!(validate_project(&project).is_empty());
    }

    #[test]
    fn legacy_schema_zero_migrates_to_current() {
        let mut project = ForgeProject {
            schema_version: 0,
            ..ForgeProject::default()
        };
        project.records[0].schema_version = 0;
        let migrated = migrate_project(project).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_PROJECT_SCHEMA);
        assert_eq!(migrated.records[0].schema_version, CURRENT_PROJECT_SCHEMA);
    }

    #[test]
    fn stable_hash_changes_with_scene_content() {
        let empty = serde_json::to_vec(&EditorSceneDraft::default()).unwrap();
        let mut scene = EditorSceneDraft::default();
        scene.objects.push(SceneObjectDraft {
            editor_id: 1,
            name: "Changed".into(),
            primitive: DraftPrimitive::Beacon,
            transform: TransformDraft::default(),
            material_id: None,
        });
        let changed = serde_json::to_vec(&scene).unwrap();
        assert_ne!(fnv1a_hash(&empty), fnv1a_hash(&changed));
    }

    #[test]
    fn publishing_promotes_the_validated_draft_hash() {
        let mut project = ForgeProject::default();
        project.publish_drafts().unwrap();
        assert_eq!(
            project.records[0].published_hash.as_deref(),
            Some(project.records[0].draft_hash.as_str())
        );
    }

    #[test]
    fn adapter_keys_must_be_unique() {
        let mut project = ForgeProject::default();
        let adapter = AdapterOverrideDraft {
            adapter_key: "cave:everest".into(),
            transform: TransformDraft::default(),
        };
        project.scene.adapter_overrides = vec![adapter.clone(), adapter];
        assert!(validate_project(&project)
            .iter()
            .any(|error| matches!(error, ProjectValidationError::DuplicateAdapterKey(_))));
    }

    #[test]
    fn invalid_ids_keys_and_transforms_are_rejected() {
        let mut project = ForgeProject::default();
        project.scene.objects.push(SceneObjectDraft {
            editor_id: 0,
            name: "Broken".into(),
            primitive: DraftPrimitive::Cube,
            transform: TransformDraft {
                rotation_xyzw: [0.0; 4],
                ..TransformDraft::default()
            },
            material_id: None,
        });
        project.scene.adapter_overrides.push(AdapterOverrideDraft {
            adapter_key: " ".into(),
            transform: TransformDraft::default(),
        });
        let errors = validate_project(&project);
        assert!(errors
            .iter()
            .any(|error| matches!(error, ProjectValidationError::InvalidEditorId(0))));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ProjectValidationError::InvalidAdapterKey(_))));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ProjectValidationError::InvalidTransform(_))));
    }

    #[test]
    fn source_paths_must_be_unique_and_project_relative() {
        let mut project = ForgeProject::default();
        project.records[0].source_path = "../outside.json".into();
        let errors = validate_project(&project);
        assert!(errors
            .iter()
            .any(|error| matches!(error, ProjectValidationError::InvalidSourcePath(_))));

        project.records[0].source_path = "scenes/shared.json".into();
        let mut duplicate = project.records[0].clone();
        duplicate.content_id = "scene.second".into();
        project.records.push(duplicate);
        assert!(validate_project(&project)
            .iter()
            .any(|error| matches!(error, ProjectValidationError::DuplicateSourcePath(_))));
    }

    #[test]
    fn source_diagnostics_report_corrupt_record_files() {
        let (root, store) = test_store("source_diagnostics");
        let mut project = ForgeProject::default();
        store.save(&mut project).unwrap();
        fs::write(root.join("scenes/main_world.json"), b"broken").unwrap();
        let diagnostics = store.source_diagnostics(&project);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("not a valid record source"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn content_lookup_can_be_built_without_entity_ids() {
        let project = ForgeProject::default();
        let index = project
            .records
            .iter()
            .map(|record| (record.content_id.as_str(), record.category))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(index["scene.main_world"], ContentCategory::Scene);
    }

    #[test]
    fn record_lifecycle_is_unique_and_dependency_safe() {
        let mut project = ForgeProject::default();
        let first = project
            .create_content(ContentCategory::Material, "Hero Ink")
            .unwrap();
        let second = project
            .create_content(ContentCategory::Material, "Hero Ink")
            .unwrap();
        assert_ne!(first, second);
        assert_eq!(
            project.payloads[&first].category(),
            ContentCategory::Material
        );

        let duplicate = project.duplicate_content(&first).unwrap();
        assert_ne!(duplicate, first);
        project
            .records
            .iter_mut()
            .find(|record| record.content_id == second)
            .unwrap()
            .dependencies
            .push(first.clone());
        assert!(matches!(
            project.delete_content(&first),
            Err(ProjectMutationError::ContentHasDependents { .. })
        ));
        project
            .records
            .iter_mut()
            .find(|record| record.content_id == second)
            .unwrap()
            .dependencies
            .clear();
        project.delete_content(&first).unwrap();
        assert!(!project.payloads.contains_key(&first));
        assert!(project.payloads.contains_key(&duplicate));
    }

    #[test]
    fn generic_material_payload_round_trips_through_its_source_codec() {
        let (root, store) = test_store("material_codec");
        let mut project = ForgeProject::default();
        let material_id = project
            .create_content(ContentCategory::Material, "Anime Rim Light")
            .unwrap();
        let ContentPayload::Material(recipe) = project.payloads.get_mut(&material_id).unwrap()
        else {
            panic!("created material has the wrong payload codec");
        };
        recipe
            .fields
            .insert("rim_power".into(), serde_json::json!(3.5));
        recipe
            .fields
            .insert("shadow_bands".into(), serde_json::json!([0.28, 0.62, 1.0]));

        store.save(&mut project).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(
            loaded.payloads[&material_id],
            project.payloads[&material_id]
        );
        assert!(store.source_diagnostics(&loaded).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn material_validation_rejects_unknown_shaders_and_out_of_range_controls() {
        let mut project = ForgeProject::default();
        let material_id = project
            .create_content(ContentCategory::Material, "Broken Shader")
            .unwrap();
        let ContentPayload::Material(recipe) = project.payloads.get_mut(&material_id).unwrap()
        else {
            panic!("created material has the wrong payload codec");
        };
        recipe
            .fields
            .insert("shader".into(), serde_json::json!("phantom_v9"));
        assert!(validate_project(&project)
            .iter()
            .any(|error| matches!(error, ProjectValidationError::InvalidMaterial(_))));

        let ContentPayload::Material(recipe) = project.payloads.get_mut(&material_id).unwrap()
        else {
            unreachable!();
        };
        recipe
            .fields
            .insert("shader".into(), serde_json::json!("lava_v1"));
        recipe
            .fields
            .insert("emissive_strength".into(), serde_json::json!(9.0));
        assert!(validate_project(&project)
            .iter()
            .any(|error| matches!(error, ProjectValidationError::InvalidMaterial(_))));
    }

    #[test]
    fn scene_material_bindings_validate_rename_and_block_delete() {
        let mut project = ForgeProject::default();
        let material_id = project
            .create_content(ContentCategory::Material, "Castle Ice")
            .unwrap();
        project.scene.objects.push(SceneObjectDraft {
            editor_id: 17,
            name: "Ice floor".into(),
            primitive: DraftPrimitive::Cube,
            transform: TransformDraft::default(),
            material_id: Some(material_id.clone()),
        });
        assert!(validate_project(&project).is_empty());

        project
            .rename_content(&material_id, "material.castle_ice_runtime")
            .unwrap();
        assert_eq!(
            project.scene.objects[0].material_id.as_deref(),
            Some("material.castle_ice_runtime")
        );
        assert!(matches!(
            project.delete_content("material.castle_ice_runtime"),
            Err(ProjectMutationError::ContentHasDependents { .. })
        ));
    }

    #[test]
    fn procedural_recipe_material_slots_publish_rename_and_round_trip() {
        let (root, store) = test_store("procedural_material_slots");
        let mut project = ForgeProject::default();
        let material_id = project
            .create_content(ContentCategory::Material, "Road Toon")
            .unwrap();
        let road_id = project
            .create_content(ContentCategory::Road, "Speed Network")
            .unwrap();
        let recipe = project
            .payloads
            .get_mut(&road_id)
            .and_then(ContentPayload::procedural_recipe_mut)
            .unwrap();
        recipe.seed = 42_195;
        recipe.revision = 7;
        recipe
            .material_slots
            .insert("deck".into(), material_id.clone());

        project.publish_drafts().unwrap();
        let road_record = project
            .records
            .iter()
            .find(|record| record.content_id == road_id)
            .unwrap();
        assert_eq!(
            road_record.dependencies.as_slice(),
            std::slice::from_ref(&material_id)
        );
        assert_eq!(
            road_record.published_hash,
            Some(road_record.draft_hash.clone())
        );

        project
            .rename_content(&material_id, "material.road_runtime")
            .unwrap();
        let recipe = project.payloads[&road_id].procedural_recipe().unwrap();
        assert_eq!(
            recipe.material_slots.get("deck").map(String::as_str),
            Some("material.road_runtime")
        );
        assert!(matches!(
            project.delete_content("material.road_runtime"),
            Err(ProjectMutationError::ContentHasDependents { .. })
        ));

        store.save(&mut project).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.payloads[&road_id], project.payloads[&road_id]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn procedural_recipe_validation_rejects_unknown_slots_and_materials() {
        let mut project = ForgeProject::default();
        let cave_id = project
            .create_content(ContentCategory::Cave, "Secret Network")
            .unwrap();
        let recipe = project
            .payloads
            .get_mut(&cave_id)
            .and_then(ContentPayload::procedural_recipe_mut)
            .unwrap();
        recipe
            .material_slots
            .insert("ceiling_fog".into(), "material.missing".into());
        let errors = validate_project(&project);
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidMaterialBinding(message)
                if message.contains("unsupported")
        )));

        let recipe = project
            .payloads
            .get_mut(&cave_id)
            .and_then(ContentPayload::procedural_recipe_mut)
            .unwrap();
        recipe.material_slots.clear();
        recipe
            .material_slots
            .insert("rock".into(), "material.missing".into());
        assert!(validate_project(&project).iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidMaterialBinding(message)
                if message.contains("missing/non-material")
        )));
    }

    #[test]
    fn typed_world_recipe_parameters_enforce_ranges_whole_values_and_budgets() {
        let mut project = ForgeProject::default();
        let road_id = project
            .create_content(ContentCategory::Road, "Budget Road")
            .unwrap();
        let recipe = project
            .payloads
            .get_mut(&road_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap();
        recipe
            .fields
            .insert("width".into(), serde_json::json!(900.0));
        recipe
            .fields
            .insert("loop_count".into(), serde_json::json!(1.5));
        let errors = validate_project(&project);
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("width")
        )));
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("whole number")
        )));

        let building_id = project
            .create_content(ContentCategory::Building, "Huge Tower")
            .unwrap();
        let recipe = project
            .payloads
            .get_mut(&building_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap();
        recipe.fields.insert("floors".into(), serde_json::json!(20));
        recipe
            .fields
            .insert("rooms_per_floor".into(), serde_json::json!(12));
        assert!(validate_project(&project).iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("160-room")
        )));
    }

    #[test]
    fn every_world_recipe_family_has_safe_typed_defaults() {
        for category in [
            ContentCategory::Biome,
            ContentCategory::Road,
            ContentCategory::Building,
            ContentCategory::Cave,
            ContentCategory::City,
        ] {
            let specs = procedural_parameter_specs(category);
            assert!(specs.len() >= 5);
            let recipe = ProceduralRecipeDraft::default();
            for spec in specs {
                let value = procedural_parameter_value(&recipe, *spec);
                assert!((spec.min..=spec.max).contains(&value));
                if spec.whole {
                    assert_eq!(value.fract(), 0.0);
                }
            }
        }
    }

    #[test]
    fn road_spline_preflight_rejects_duplicate_ids_and_excessive_grade() {
        let mut project = ForgeProject::default();
        let road_id = project
            .create_content(ContentCategory::Road, "Unsafe Spline")
            .unwrap();
        let recipe = project
            .payloads
            .get_mut(&road_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap();
        recipe.spline_points = vec![
            RoadSplinePointDraft {
                point_id: "same".into(),
                position: [0.0, 0.0, 0.0],
                width_scale: 1.0,
                tangent: [0.0; 3],
            },
            RoadSplinePointDraft {
                point_id: "same".into(),
                position: [0.0, 50.0, 10.0],
                width_scale: 1.0,
                tangent: [0.0; 3],
            },
        ];
        let errors = validate_project(&project);
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("unique")
        )));
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("maximum road grade")
        )));
    }

    #[test]
    fn automatic_road_tangents_and_hermite_samples_are_deterministic() {
        let points = vec![
            RoadSplinePointDraft {
                point_id: "a".into(),
                position: [0.0, 0.0, 0.0],
                width_scale: 1.0,
                tangent: [0.0; 3],
            },
            RoadSplinePointDraft {
                point_id: "b".into(),
                position: [10.0, 0.0, 10.0],
                width_scale: 1.0,
                tangent: [0.0; 3],
            },
            RoadSplinePointDraft {
                point_id: "c".into(),
                position: [20.0, 0.0, 0.0],
                width_scale: 1.0,
                tangent: [0.0; 3],
            },
        ];
        assert_eq!(resolved_road_tangent(&points, 1), [10.0, 0.0, 0.0]);
        assert_eq!(
            sample_road_spline_span(&points, 0, 0.0),
            Some([0.0, 0.0, 0.0])
        );
        assert_eq!(
            sample_road_spline_span(&points, 0, 1.0),
            Some([10.0, 0.0, 10.0])
        );
        let midpoint = sample_road_spline_span(&points, 0, 0.5).unwrap();
        assert_ne!(midpoint, [5.0, 0.0, 5.0]);
        assert_eq!(midpoint, sample_road_spline_span(&points, 0, 0.5).unwrap());
    }

    #[test]
    fn road_junction_preflight_rejects_missing_endpoints_and_duplicate_links() {
        let mut project = ForgeProject::default();
        let road_id = project
            .create_content(ContentCategory::Road, "Junction Test")
            .unwrap();
        let recipe = project.payloads[&road_id].procedural_recipe().unwrap();
        assert!(recipe.road_junctions.is_empty());
        let recipe = project
            .payloads
            .get_mut(&road_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap();
        recipe.spline_points = vec![
            RoadSplinePointDraft {
                point_id: "a".into(),
                position: [0.0, 0.0, 0.0],
                width_scale: 1.0,
                tangent: [0.0; 3],
            },
            RoadSplinePointDraft {
                point_id: "b".into(),
                position: [0.0, 0.0, 20.0],
                width_scale: 1.0,
                tangent: [0.0; 3],
            },
        ];
        recipe.road_junctions = vec![
            RoadJunctionDraft {
                junction_id: "bad".into(),
                from_point: "a".into(),
                to_point: "missing".into(),
                kind: RoadJunctionKind::Merge,
            },
            RoadJunctionDraft {
                junction_id: "duplicate".into(),
                from_point: "b".into(),
                to_point: "a".into(),
                kind: RoadJunctionKind::Merge,
            },
            RoadJunctionDraft {
                junction_id: "duplicate".into(),
                from_point: "a".into(),
                to_point: "b".into(),
                kind: RoadJunctionKind::Merge,
            },
        ];
        let errors = validate_project(&project);
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("junction IDs/links")
        )));
    }

    #[test]
    fn topology_sockets_validate_node_bounds_and_edge_ownership() {
        let mut project = ForgeProject::default();
        let building_id = project
            .create_content(ContentCategory::Building, "Socket Rooms")
            .unwrap();
        let recipe = project
            .payloads
            .get_mut(&building_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap();
        recipe.topology_nodes = vec![
            WorldTopologyNodeDraft {
                node_id: "entrance".into(),
                kind: WorldTopologyNodeKind::Entrance,
                position: [0.0, 0.0, 0.0],
                size: [4.0, 4.0, 4.0],
            },
            WorldTopologyNodeDraft {
                node_id: "room".into(),
                kind: WorldTopologyNodeKind::Room,
                position: [8.0, 0.0, 0.0],
                size: [4.0, 4.0, 4.0],
            },
        ];
        recipe.topology_sockets = vec![
            WorldTopologySocketDraft {
                socket_id: "entrance.out".into(),
                node_id: "entrance".into(),
                kind: WorldTopologySocketKind::Doorway,
                local_position: [2.0, 0.0, 0.0],
                facing: [1.0, 0.0, 0.0],
                size: [2.0, 3.0],
            },
            WorldTopologySocketDraft {
                socket_id: "room.in".into(),
                node_id: "room".into(),
                kind: WorldTopologySocketKind::Doorway,
                local_position: [-2.0, 0.0, 0.0],
                facing: [-1.0, 0.0, 0.0],
                size: [2.0, 3.0],
            },
        ];
        recipe.topology_edges = vec![WorldTopologyEdgeDraft {
            from: "entrance".into(),
            to: "room".into(),
            kind: WorldTopologyEdgeKind::Door,
            from_socket: Some("entrance.out".into()),
            to_socket: Some("room.in".into()),
        }];
        assert!(validate_project(&project).is_empty());

        let recipe = project
            .payloads
            .get_mut(&building_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap();
        recipe.topology_sockets[1].local_position[0] = 12.0;
        recipe.topology_edges[0].to_socket = Some("entrance.out".into());
        let errors = validate_project(&project);
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("topology socket")
        )));
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("topology edges")
        )));
    }

    #[test]
    fn terrain_projection_settings_are_backward_compatible_and_bounded() {
        let legacy: ProceduralRecipeDraft = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "seed": 42,
                "revision": 0,
                "material_slots": {},
                "spline_points": [],
                "topology_nodes": [],
                "topology_edges": [],
                "fields": {}
            }"#,
        )
        .unwrap();
        assert_eq!(legacy.terrain_projection, TerrainProjectionDraft::default());

        let mut project = ForgeProject::default();
        let city_id = project
            .create_content(ContentCategory::City, "Terrain Projection")
            .unwrap();
        let recipe = project
            .payloads
            .get_mut(&city_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap();
        recipe.terrain_projection.world_origin = [10_001.0, 0.0];
        recipe.terrain_projection.clearance = 251.0;
        let errors = validate_project(&project);
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("terrain projection origin")
        )));
    }

    #[test]
    fn room_graph_preflight_requires_reachability_and_nonoverlap() {
        let mut project = ForgeProject::default();
        let building_id = project
            .create_content(ContentCategory::Building, "Broken Rooms")
            .unwrap();
        let recipe = project
            .payloads
            .get_mut(&building_id)
            .unwrap()
            .procedural_recipe_mut()
            .unwrap();
        recipe.topology_nodes = vec![
            WorldTopologyNodeDraft {
                node_id: "entry".into(),
                kind: WorldTopologyNodeKind::Entrance,
                position: [0.0, 0.0, 0.0],
                size: [8.0, 4.0, 8.0],
            },
            WorldTopologyNodeDraft {
                node_id: "room".into(),
                kind: WorldTopologyNodeKind::Room,
                position: [1.0, 0.0, 0.0],
                size: [8.0, 4.0, 8.0],
            },
        ];
        let errors = validate_project(&project);
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("reachable")
        )));
        assert!(errors.iter().any(|error| matches!(
            error,
            ProjectValidationError::InvalidProceduralRecipe(message)
                if message.contains("overlap")
        )));
    }

    #[test]
    fn legacy_generic_road_json_loads_as_a_procedural_recipe() {
        let payload: ContentPayload = serde_json::from_value(serde_json::json!({
            "kind": "road",
            "data": {
                "schema_version": 1,
                "fields": { "lane_count": 4 }
            }
        }))
        .unwrap();
        let ContentPayload::Road(recipe) = payload else {
            panic!("legacy road decoded as the wrong payload kind");
        };
        assert_eq!(recipe.fields["lane_count"], serde_json::json!(4));
        assert!(recipe.material_slots.is_empty());
        assert_eq!(recipe.seed, 0);
        assert_eq!(recipe.revision, 0);
    }

    #[test]
    fn canonical_source_move_preserves_file_and_updates_record() {
        let (root, store) = test_store("source_move");
        let mut project = ForgeProject::default();
        let material_id = project
            .create_content(ContentCategory::Material, "Old Name")
            .unwrap();
        store.save(&mut project).unwrap();
        let old_path = project
            .records
            .iter()
            .find(|record| record.content_id == material_id)
            .unwrap()
            .source_path
            .clone();
        project
            .rename_content(&material_id, "material.hero_ink")
            .unwrap();
        let new_path = store
            .move_source_to_canonical(&mut project, "material.hero_ink")
            .unwrap();
        assert_ne!(old_path, new_path);
        assert!(!root.join(old_path).exists());
        assert!(root.join(&new_path).is_file());
        assert_eq!(
            project
                .records
                .iter()
                .find(|record| record.content_id == "material.hero_ink")
                .unwrap()
                .source_path,
            new_path
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejected_source_move_leaves_manifest_and_files_unchanged() {
        let (root, store) = test_store("source_move_collision");
        let mut project = ForgeProject::default();
        let first = project
            .create_content(ContentCategory::Material, "First")
            .unwrap();
        let second = project
            .create_content(ContentCategory::Material, "Second")
            .unwrap();
        store.save(&mut project).unwrap();
        let first_path = project
            .records
            .iter()
            .find(|record| record.content_id == first)
            .unwrap()
            .source_path
            .clone();
        let second_path = project
            .records
            .iter()
            .find(|record| record.content_id == second)
            .unwrap()
            .source_path
            .clone();

        assert!(matches!(
            store.move_source(&mut project, &first, &second_path),
            Err(ProjectMutationError::SourceAlreadyExists(_))
        ));
        assert_eq!(
            project
                .records
                .iter()
                .find(|record| record.content_id == first)
                .unwrap()
                .source_path,
            first_path
        );
        assert!(root.join(first_path).is_file());
        assert!(root.join(second_path).is_file());
        let _ = fs::remove_dir_all(root);
    }
}
