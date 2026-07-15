//! Versioned, atomic persistence for Starfall Forge projects.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

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
}

impl Default for ForgeProject {
    fn default() -> Self {
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
            scene: EditorSceneDraft::default(),
        }
    }
}

impl ForgeProject {
    pub fn refresh_hashes(&mut self) -> Result<(), ProjectIoError> {
        let scene_json = serde_json::to_vec(&self.scene)
            .map_err(|error| ProjectIoError::Serialization(error.to_string()))?;
        let scene_hash = fnv1a_hash(&scene_json);
        for record in &mut self.records {
            if record.category == ContentCategory::Scene {
                record.draft_hash.clone_from(&scene_hash);
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
    NoValidProject,
}

impl std::fmt::Display for ProjectIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::Serialization(message) => write!(formatter, "project data error: {message}"),
            Self::Validation(errors) => write!(formatter, "project validation failed: {errors:?}"),
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
}

fn load_project_file(path: &Path) -> Result<ForgeProject, ProjectIoError> {
    let bytes = fs::read(path).map_err(io_error)?;
    let project: ForgeProject = serde_json::from_slice(&bytes)
        .map_err(|error| ProjectIoError::Serialization(error.to_string()))?;
    let project = migrate_project(project)?;
    let errors = validate_project(&project);
    if errors.is_empty() {
        Ok(project)
    } else {
        Err(ProjectIoError::Validation(errors))
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ProjectIoError> {
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
    fn content_lookup_can_be_built_without_entity_ids() {
        let project = ForgeProject::default();
        let index = project
            .records
            .iter()
            .map(|record| (record.content_id.as_str(), record.category))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(index["scene.main_world"], ContentCategory::Scene);
    }
}
