//! Versioned, validated manifests for Starfall projects and modules.
//!
//! Paths describe project inputs and outputs; they do not imply filesystem
//! existence. Discovery and scaffolding can therefore validate a document
//! before creating or modifying files.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use starfall_graph::StableId;

pub const PROJECT_SCHEMA_VERSION: u32 = 1;
pub const MODULE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub project: ProjectIdentity,
    #[serde(default)]
    pub paths: ProjectPaths,
    #[serde(default)]
    pub modules: Vec<ModuleRequirement>,
}

impl ProjectManifest {
    pub fn parse(source: &str) -> Result<Self, ManifestParseError> {
        parse_and_migrate(source, ManifestKind::Project)
    }

    pub fn to_toml_pretty(&self) -> Result<String, ManifestParseError> {
        toml::to_string_pretty(self).map_err(|error| ManifestParseError::Encode(error.to_string()))
    }

    pub fn validate(&self) -> Vec<ManifestDiagnostic> {
        let mut diagnostics = validate_schema(self.schema_version, PROJECT_SCHEMA_VERSION);
        validate_identity(&self.project, &mut diagnostics);
        validate_path("paths.content", &self.paths.content, &mut diagnostics);
        validate_path(
            "paths.source_assets",
            &self.paths.source_assets,
            &mut diagnostics,
        );
        validate_path(
            "paths.imported_assets",
            &self.paths.imported_assets,
            &mut diagnostics,
        );
        validate_path("paths.published", &self.paths.published, &mut diagnostics);
        validate_requirements(&self.modules, &mut diagnostics);
        diagnostics
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectIdentity {
    pub id: StableId,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPaths {
    pub content: String,
    pub source_assets: String,
    pub imported_assets: String,
    pub published: String,
}

impl Default for ProjectPaths {
    fn default() -> Self {
        Self {
            content: "content".into(),
            source_assets: "assets/source".into(),
            imported_assets: "assets/imported".into(),
            published: "build/published".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleRequirement {
    pub id: StableId,
    pub version: String,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub schema_version: u32,
    pub module: ModuleIdentity,
    #[serde(default)]
    pub dependencies: Vec<ModuleRequirement>,
    #[serde(default)]
    pub provides: ModuleProvides,
}

impl ModuleManifest {
    pub fn parse(source: &str) -> Result<Self, ManifestParseError> {
        parse_and_migrate(source, ManifestKind::Module)
    }

    pub fn to_toml_pretty(&self) -> Result<String, ManifestParseError> {
        toml::to_string_pretty(self).map_err(|error| ManifestParseError::Encode(error.to_string()))
    }

    pub fn validate(&self) -> Vec<ManifestDiagnostic> {
        let mut diagnostics = validate_schema(self.schema_version, MODULE_SCHEMA_VERSION);
        validate_identity(&self.module.identity, &mut diagnostics);
        validate_path(
            "module.source_root",
            &self.module.source_root,
            &mut diagnostics,
        );
        validate_requirements(&self.dependencies, &mut diagnostics);
        diagnostics
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleIdentity {
    #[serde(flatten)]
    pub identity: ProjectIdentity,
    pub kind: ModuleKind,
    #[serde(default = "default_source_root")]
    pub source_root: String,
}

fn default_source_root() -> String {
    "src".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleKind {
    Engine,
    GameplayKit,
    GameFeature,
    Editor,
    Presentation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleProvides {
    #[serde(default)]
    pub graph_nodes: Vec<StableId>,
    #[serde(default)]
    pub object_types: Vec<StableId>,
    #[serde(default)]
    pub editor_tools: Vec<StableId>,
    #[serde(default)]
    pub publish_steps: Vec<StableId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: &'static str,
    pub field: String,
    pub message: String,
}

impl ManifestDiagnostic {
    fn error(code: &'static str, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code,
            field: field.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestParseError {
    Decode(String),
    Encode(String),
    UnsupportedSchema { found: u32, supported: u32 },
    InvalidRoot,
}

impl fmt::Display for ManifestParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(message) => write!(formatter, "invalid manifest TOML: {message}"),
            Self::Encode(message) => write!(formatter, "could not encode manifest TOML: {message}"),
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "manifest schema {found} is newer than supported schema {supported}"
            ),
            Self::InvalidRoot => formatter.write_str("manifest root must be a TOML table"),
        }
    }
}

impl std::error::Error for ManifestParseError {}

#[derive(Clone, Copy)]
enum ManifestKind {
    Project,
    Module,
}

trait VersionedManifest: for<'de> Deserialize<'de> {
    const CURRENT_SCHEMA: u32;
}

impl VersionedManifest for ProjectManifest {
    const CURRENT_SCHEMA: u32 = PROJECT_SCHEMA_VERSION;
}

impl VersionedManifest for ModuleManifest {
    const CURRENT_SCHEMA: u32 = MODULE_SCHEMA_VERSION;
}

fn parse_and_migrate<T: VersionedManifest>(
    source: &str,
    kind: ManifestKind,
) -> Result<T, ManifestParseError> {
    let value = toml::from_str::<toml::Value>(source)
        .map_err(|error| ManifestParseError::Decode(error.to_string()))?;
    let value = migrate(value, kind, T::CURRENT_SCHEMA)?;
    value
        .try_into()
        .map_err(|error: toml::de::Error| ManifestParseError::Decode(error.to_string()))
}

fn migrate(
    mut value: toml::Value,
    kind: ManifestKind,
    supported: u32,
) -> Result<toml::Value, ManifestParseError> {
    let root = value
        .as_table_mut()
        .ok_or(ManifestParseError::InvalidRoot)?;
    let found = root
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        .unwrap_or(0);
    let found = u32::try_from(found).map_err(|_| {
        ManifestParseError::Decode("schema_version must be a non-negative integer".into())
    })?;
    if found > supported {
        return Err(ManifestParseError::UnsupportedSchema { found, supported });
    }

    if found == 0 {
        let identity_key = match kind {
            ManifestKind::Project => "project",
            ManifestKind::Module => "module",
        };
        if let Some(identity) = root
            .get_mut(identity_key)
            .and_then(toml::Value::as_table_mut)
        {
            if let Some(display_name) = identity.remove("display_name") {
                identity.entry("name").or_insert(display_name);
            }
        }
        root.insert("schema_version".into(), toml::Value::Integer(1));
    }
    Ok(value)
}

fn validate_schema(found: u32, supported: u32) -> Vec<ManifestDiagnostic> {
    if found == supported {
        Vec::new()
    } else {
        vec![ManifestDiagnostic::error(
            "manifest.schema_version",
            "schema_version",
            format!("expected schema {supported}, found {found}"),
        )]
    }
}

fn validate_identity(identity: &ProjectIdentity, diagnostics: &mut Vec<ManifestDiagnostic>) {
    if identity.name.trim().is_empty() {
        diagnostics.push(ManifestDiagnostic::error(
            "manifest.empty_name",
            "name",
            "display name must not be empty",
        ));
    }
    if semver::Version::parse(&identity.version).is_err() {
        diagnostics.push(ManifestDiagnostic::error(
            "manifest.invalid_version",
            "version",
            format!("{} is not a semantic version", identity.version),
        ));
    }
}

fn validate_requirements(
    requirements: &[ModuleRequirement],
    diagnostics: &mut Vec<ManifestDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for (index, requirement) in requirements.iter().enumerate() {
        if !seen.insert(&requirement.id) {
            diagnostics.push(ManifestDiagnostic::error(
                "manifest.duplicate_module",
                format!("modules[{index}].id"),
                format!("module {} is declared more than once", requirement.id),
            ));
        }
        if semver::VersionReq::parse(&requirement.version).is_err() {
            diagnostics.push(ManifestDiagnostic::error(
                "manifest.invalid_version_requirement",
                format!("modules[{index}].version"),
                format!(
                    "{} is not a semantic-version requirement",
                    requirement.version
                ),
            ));
        }
    }
}

fn validate_path(field: &str, value: &str, diagnostics: &mut Vec<ManifestDiagnostic>) {
    let path = Path::new(value);
    let invalid = value.trim().is_empty()
        || value.contains('\\')
        || value.contains(':')
        || value
            .split('/')
            .any(|segment| segment == "." || segment == "..")
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    if invalid {
        diagnostics.push(ManifestDiagnostic::error(
            "manifest.unsafe_path",
            field,
            "path must be non-empty, relative, and remain inside the project",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = r#"
schema_version = 1

[project]
id = "example.sky-town"
name = "Sky Town"
version = "0.1.0"

[[modules]]
id = "starfall.gameplay.platformer"
version = "^0.1"
"#;

    #[test]
    fn project_round_trips_without_losing_meaning() {
        let manifest = ProjectManifest::parse(PROJECT).unwrap();
        assert!(manifest.validate().is_empty());
        let encoded = manifest.to_toml_pretty().unwrap();
        assert_eq!(ProjectManifest::parse(&encoded).unwrap(), manifest);
    }

    #[test]
    fn legacy_display_name_is_migrated() {
        let legacy = PROJECT
            .replace("schema_version = 1\n\n", "")
            .replace("name = \"Sky Town\"", "display_name = \"Sky Town\"");
        let manifest = ProjectManifest::parse(&legacy).unwrap();
        assert_eq!(manifest.schema_version, PROJECT_SCHEMA_VERSION);
        assert_eq!(manifest.project.name, "Sky Town");
    }

    #[test]
    fn validation_rejects_unsafe_paths_and_duplicate_modules() {
        let mut manifest = ProjectManifest::parse(PROJECT).unwrap();
        manifest.paths.published = "../outside".into();
        manifest.modules.push(manifest.modules[0].clone());
        let codes = manifest
            .validate()
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<BTreeSet<_>>();
        assert!(codes.contains("manifest.unsafe_path"));
        assert!(codes.contains("manifest.duplicate_module"));
    }

    #[test]
    fn path_validation_is_portable_across_host_operating_systems() {
        for unsafe_path in ["/outside", "../outside", "..\\outside", "C:\\outside"] {
            let mut manifest = ProjectManifest::parse(PROJECT).unwrap();
            manifest.paths.published = unsafe_path.into();
            assert!(manifest
                .validate()
                .iter()
                .any(|diagnostic| diagnostic.code == "manifest.unsafe_path"));
        }
    }

    #[test]
    fn module_manifest_round_trips() {
        let source = r#"
schema_version = 1

[module]
id = "heavy_water.platformer"
name = "Heavy Water Platformer"
version = "0.1.0"
kind = "game-feature"
source_root = "src"

[[dependencies]]
id = "starfall.gameplay.platformer"
version = "^0.1"

[provides]
graph_nodes = ["heavy_water.platformer.checkpoint"]
"#;
        let manifest = ModuleManifest::parse(source).unwrap();
        assert!(manifest.validate().is_empty());
        assert_eq!(
            ModuleManifest::parse(&manifest.to_toml_pretty().unwrap()).unwrap(),
            manifest
        );
    }
}
