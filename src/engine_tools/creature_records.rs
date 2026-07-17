//! Bridge between the Creature Forge and the versioned project contract.
//!
//! Forge saves flow through the active `ForgeProject`: each creature is a
//! `ContentCategory::Creature` record whose payload carries the complete
//! [`CreatureSpec`] as a generator field. That routes creature authoring
//! through the same atomic writes, recovery snapshots, draft/published
//! hashes, and validation gates the level editor uses. Standalone
//! `creature_vNNN.json` files remain an import/export preset library only —
//! never the published path.

use std::path::Path;

use super::persistence::{
    ContentCategory, ContentPayload, ContentRecord, GenericRecipeDraft, ProjectStore,
    CURRENT_PROJECT_SCHEMA,
};
use super::project_registry::ForgeProjectRegistry;
use crate::robots::creature::{CreatureSpec, ValidationSeverity};

pub use super::persistence::ForgeProject;

const SPEC_FIELD: &str = "creature_spec";
const RECOVERY_LIMIT: usize = 3;
const DEFAULT_CONTENT_ID: &str = "starfall.scout.generated";

/// Validation gate for saving: every `Error`-severity issue blocks the save;
/// warnings are surfaced but allowed.
pub fn blocking_issues(spec: &CreatureSpec) -> Vec<String> {
    spec.validate()
        .issues
        .iter()
        .filter(|issue| issue.severity == ValidationSeverity::Error)
        .map(|issue| issue.message.clone())
        .collect()
}

/// Insert or update `spec` as a Creature content record + payload. The spec
/// must already have passed [`blocking_issues`].
pub fn upsert_creature(project: &mut ForgeProject, spec: &CreatureSpec) -> Result<(), String> {
    let value = serde_json::to_value(spec).map_err(|e| e.to_string())?;
    let mut recipe = GenericRecipeDraft::default();
    recipe.fields.insert(SPEC_FIELD.to_string(), value);

    match project
        .records
        .iter_mut()
        .find(|record| record.content_id == spec.content_id)
    {
        Some(record) => {
            if record.category != ContentCategory::Creature {
                return Err(format!(
                    "{} already exists as {:?} content",
                    spec.content_id, record.category
                ));
            }
            record.display_name = spec.display_name.clone();
        }
        None => project.records.push(ContentRecord {
            content_id: spec.content_id.clone(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Creature,
            source_path: format!("creatures/{}.json", spec.content_id.replace('.', "_")),
            dependencies: Vec::new(),
            display_name: spec.display_name.clone(),
            tags: vec!["creature".to_string()],
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        }),
    }
    project
        .payloads
        .insert(spec.content_id.clone(), ContentPayload::Creature(recipe));
    Ok(())
}

/// Every creature record in the project as `(content_id, display_name)`.
pub fn creature_entries(project: &ForgeProject) -> Vec<(String, String)> {
    project
        .records
        .iter()
        .filter(|record| record.category == ContentCategory::Creature)
        .map(|record| (record.content_id.clone(), record.display_name.clone()))
        .collect()
}

/// Decode the stored spec for `content_id`.
pub fn load_creature(project: &ForgeProject, content_id: &str) -> Result<CreatureSpec, String> {
    let Some(ContentPayload::Creature(recipe)) = project.payloads.get(content_id) else {
        return Err(format!("{content_id} has no creature payload"));
    };
    let Some(value) = recipe.fields.get(SPEC_FIELD) else {
        return Err(format!("{content_id} carries no {SPEC_FIELD} field"));
    };
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
}

/// Next free `starfall.creature-N` id in the project, used when a spec still
/// carries the factory-default id so repeated saves author new creatures
/// instead of silently overwriting the default one.
pub fn next_free_content_id(project: &ForgeProject) -> String {
    (1..)
        .map(|n| format!("starfall.creature-{n}"))
        .find(|candidate| {
            !project
                .records
                .iter()
                .any(|record| &record.content_id == candidate)
        })
        .expect("unbounded numbering always finds a free creature id")
}

fn open_active_store(registry: &ForgeProjectRegistry) -> Result<(ProjectStore, ForgeProject), String> {
    let Some(active) = registry.active.as_deref() else {
        return Err("No active project — open one in the Project Hub first".to_string());
    };
    let store = ProjectStore::new(active, RECOVERY_LIMIT);
    let (project, _) = store
        .load_with_recovery()
        .map_err(|e| format!("Could not load active project: {e}"))?;
    Ok((store, project))
}

/// Save `spec` into the active project through the full store contract.
/// Assigns a fresh content id when the spec still uses the factory default.
/// Returns the content id the creature was saved under.
pub fn save_to_active_project(
    registry: &ForgeProjectRegistry,
    spec: &mut CreatureSpec,
) -> Result<String, String> {
    let issues = blocking_issues(spec);
    if !issues.is_empty() {
        return Err(format!("Fix before saving: {}", issues.join(" • ")));
    }
    let (store, mut project) = open_active_store(registry)?;
    if spec.content_id == DEFAULT_CONTENT_ID {
        spec.content_id = next_free_content_id(&project);
    }
    upsert_creature(&mut project, spec)?;
    store
        .save(&mut project)
        .map_err(|e| format!("Project save failed: {e}"))?;
    Ok(spec.content_id.clone())
}

/// List the active project's creature records.
pub fn list_active_project(registry: &ForgeProjectRegistry) -> Vec<(String, String)> {
    open_active_store(registry)
        .map(|(_, project)| creature_entries(&project))
        .unwrap_or_default()
}

/// Load one creature spec from the active project.
pub fn load_from_active_project(
    registry: &ForgeProjectRegistry,
    content_id: &str,
) -> Result<CreatureSpec, String> {
    let (_, project) = open_active_store(registry)?;
    load_creature(&project, content_id)
}

/// Shared active-project path label for status lines.
pub fn active_project_label(registry: &ForgeProjectRegistry) -> String {
    registry
        .active
        .as_deref()
        .map(Path::display)
        .map(|display| display.to_string())
        .unwrap_or_else(|| "no active project".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_registry(label: &str) -> (PathBuf, ForgeProjectRegistry) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "starfall_creature_records_{label}_{}_{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("test root should be creatable");
        let mut registry = ForgeProjectRegistry {
            projects: Vec::new(),
            active: None,
        };
        let entry = registry
            .create_new_project_in(&root)
            .expect("project creation succeeds");
        registry.set_active(&entry.path);
        (root, registry)
    }

    #[test]
    fn save_load_round_trip_through_the_active_project() {
        let (root, registry) = test_registry("round_trip");
        let mut spec = CreatureSpec {
            display_name: "Ridge Stalker".into(),
            seed: 77,
            ..Default::default()
        };
        spec.style.has_wings = true;

        let content_id =
            save_to_active_project(&registry, &mut spec).expect("first save succeeds");
        assert_eq!(content_id, "starfall.creature-1");
        assert_eq!(spec.content_id, content_id);

        let listed = list_active_project(&registry);
        assert!(listed
            .iter()
            .any(|(id, name)| id == &content_id && name == "Ridge Stalker"));

        let loaded =
            load_from_active_project(&registry, &content_id).expect("load succeeds");
        assert_eq!(loaded.seed, 77);
        assert!(loaded.style.has_wings);

        // Upsert keeps the same id and does not duplicate the record.
        spec.display_name = "Ridge Stalker II".into();
        let second = save_to_active_project(&registry, &mut spec).expect("upsert succeeds");
        assert_eq!(second, content_id);
        assert_eq!(list_active_project(&registry).len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_specs_are_refused_before_touching_the_project() {
        let (root, registry) = test_registry("invalid");
        let mut spec = CreatureSpec {
            content_id: "INVALID ID".into(),
            ..Default::default()
        };
        let error = save_to_active_project(&registry, &mut spec)
            .expect_err("invalid content id must be refused");
        assert!(error.contains("Fix before saving"));
        assert!(list_active_project(&registry).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn saving_without_an_active_project_reports_cleanly() {
        let registry = ForgeProjectRegistry {
            projects: Vec::new(),
            active: None,
        };
        let mut spec = CreatureSpec::default();
        let error = save_to_active_project(&registry, &mut spec).expect_err("no project");
        assert!(error.contains("Project Hub"));
    }
}
