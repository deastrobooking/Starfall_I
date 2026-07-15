//! Versioned, atomic persistence for Starfall Forge projects.

use std::collections::{BTreeMap, BTreeSet};
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
        Ok(())
    }

    pub fn publish_drafts(&mut self) -> Result<(), ProjectIoError> {
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
    Road(GenericRecipeDraft),
    Building(GenericRecipeDraft),
    Cave(GenericRecipeDraft),
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
            ContentCategory::Road => Self::Road(recipe),
            ContentCategory::Building => Self::Building(recipe),
            ContentCategory::Cave => Self::Cave(recipe),
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
            Self::Road(_) => ContentCategory::Road,
            Self::Building(_) => ContentCategory::Building,
            Self::Cave(_) => ContentCategory::Cave,
            Self::Material(_) => ContentCategory::Material,
            Self::Ui(_) => ContentCategory::Ui,
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

fn current_schema() -> u32 {
    CURRENT_PROJECT_SCHEMA
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentCategory {
    Scene,
    Character,
    Creature,
    Road,
    Building,
    Cave,
    Material,
    Ui,
}

impl ContentCategory {
    fn id_prefix(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Character => "character",
            Self::Creature => "creature",
            Self::Road => "road",
            Self::Building => "building",
            Self::Cave => "cave",
            Self::Material => "material",
            Self::Ui => "ui",
        }
    }

    fn source_folder(self) -> &'static str {
        match self {
            Self::Scene => "scenes",
            Self::Character => "characters",
            Self::Creature => "creatures",
            Self::Road => "roads",
            Self::Building => "buildings",
            Self::Cave => "caves",
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
        atomic_write(&self.path, &bytes)
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

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ProjectIoError> {
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
    fn project_round_trip_refreshes_scene_hash() {
        let (root, store) = test_store("round_trip");
        let mut project = ForgeProject::default();
        project.scene.objects.push(SceneObjectDraft {
            editor_id: 8,
            name: "Test Block".into(),
            primitive: DraftPrimitive::Cube,
            transform: TransformDraft::default(),
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
            },
            SceneObjectDraft {
                editor_id: 4,
                name: "B".into(),
                primitive: DraftPrimitive::Empty,
                transform: TransformDraft::default(),
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
