//! Project-backed records for Spaceship Forge recipes.
//!
//! Each design is stored as a typed `spaceship_spec` generator field while
//! inheriting the shared project's atomic writes, revision comparison,
//! recovery snapshots, validation, and draft/published hashes.

use super::persistence::{
    ContentCategory, ContentPayload, ContentRecord, ForgeProject, GenericRecipeDraft,
    ProjectLoadSource, ProjectStore, CURRENT_PROJECT_SCHEMA,
};
use super::project_registry::ForgeProjectRegistry;
use crate::spaceship_forge::{SpacecraftClass, SpacecraftSpec, SpacecraftValidationSeverity};

const SPEC_FIELD: &str = "spaceship_spec";
const RECOVERY_LIMIT: usize = 3;

fn is_template_content_id(content_id: &str) -> bool {
    matches!(
        content_id,
        "starfall.spaceship.new_fighter"
            | "starfall.spaceship.new_bomber"
            | "starfall.spaceship.new_destroyer"
            | "starfall.spaceship.new_command_ship"
            | "starfall.spaceship.new_base"
    )
}

fn open_active_store(
    registry: &ForgeProjectRegistry,
) -> Result<
    (
        ProjectStore,
        ForgeProject,
        ProjectLoadSource,
        Option<String>,
    ),
    String,
> {
    let Some(active) = registry.active.as_deref() else {
        return Err("No active project — open one in the Project Hub first".to_string());
    };
    let store = ProjectStore::new(active, RECOVERY_LIMIT);
    let (project, source, revision) = store
        .load_with_recovery_and_revision()
        .map_err(|error| format!("Could not load active project: {error}"))?;
    Ok((store, project, source, revision))
}

fn writable_revision(
    source: ProjectLoadSource,
    revision: Option<String>,
) -> Result<String, String> {
    if matches!(source, ProjectLoadSource::Recovery(_)) {
        return Err(
            "Active project opened from recovery; promote it explicitly in the level editor before saving"
                .to_string(),
        );
    }
    revision.ok_or_else(|| "Active primary project has no writable revision".to_string())
}

/// Stable lowercase id for callers creating a new named recipe.
pub fn content_id_for(name: &str) -> String {
    let slug = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let slug = slug
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    let max_slug_bytes = 96 - "starfall.spaceship.".len();
    let slug = slug.chars().take(max_slug_bytes).collect::<String>();
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "starfall.spaceship.unnamed".to_string()
    } else {
        format!("starfall.spaceship.{slug}")
    }
}

pub fn blocking_issues(spec: &SpacecraftSpec) -> Vec<String> {
    spec.validate()
        .issues
        .into_iter()
        .filter(|issue| issue.severity == SpacecraftValidationSeverity::Error)
        .map(|issue| issue.message)
        .collect()
}

fn class_tag(class: SpacecraftClass) -> &'static str {
    match class {
        SpacecraftClass::Fighter => "fighter",
        SpacecraftClass::Bomber => "bomber",
        SpacecraftClass::Destroyer => "destroyer",
        SpacecraftClass::CommandShip => "command_ship",
        SpacecraftClass::Base => "base",
    }
}

/// Insert or update one spacecraft while keeping its explicit content id as
/// the durable join key. Display-name edits therefore rename in place rather
/// than silently duplicating references held by levels or future fleets.
pub fn upsert_spaceship(
    project: &mut ForgeProject,
    spec: &SpacecraftSpec,
) -> Result<String, String> {
    let issues = blocking_issues(spec);
    if !issues.is_empty() {
        return Err(issues.join("; "));
    }
    let creating = is_template_content_id(&spec.content_id);
    let content_id = if creating {
        content_id_for(&spec.display_name)
    } else {
        spec.content_id.clone()
    };
    if creating && is_template_content_id(&content_id) {
        return Err(
            "That name resolves to a reserved template identity; choose a more specific name"
                .to_string(),
        );
    }
    if creating
        && project
            .records
            .iter()
            .any(|record| record.content_id == content_id)
    {
        return Err(format!(
            "{content_id} already exists; load that design to update it or choose another name"
        ));
    }
    let mut stored = spec.clone();
    stored.content_id.clone_from(&content_id);
    let stored_issues = blocking_issues(&stored);
    if !stored_issues.is_empty() {
        return Err(stored_issues.join("; "));
    }
    let stored_json = serde_json::to_vec(&stored).map_err(|error| error.to_string())?;
    let stored_value = serde_json::from_slice(&stored_json).map_err(|error| error.to_string())?;
    let mut recipe = GenericRecipeDraft::default();
    recipe.fields.insert(SPEC_FIELD.to_string(), stored_value);

    match project
        .records
        .iter_mut()
        .find(|record| record.content_id == content_id)
    {
        Some(record) => {
            if record.category != ContentCategory::Spaceship {
                return Err(format!(
                    "{content_id} already exists as {:?} content",
                    record.category
                ));
            }
            record.display_name = spec.display_name.clone();
            record.tags = vec!["spaceship".into(), class_tag(spec.class).into()];
        }
        None => project.records.push(ContentRecord {
            content_id: content_id.clone(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Spaceship,
            source_path: format!("spaceships/{}.json", content_id.replace('.', "_")),
            dependencies: Vec::new(),
            display_name: spec.display_name.clone(),
            tags: vec!["spaceship".into(), class_tag(spec.class).into()],
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        }),
    }
    project
        .payloads
        .insert(content_id.clone(), ContentPayload::Spaceship(recipe));
    Ok(content_id)
}

/// Every spacecraft record as `(content_id, display_name)`, sorted for stable
/// library and publish ordering.
pub fn spaceship_entries(project: &ForgeProject) -> Vec<(String, String)> {
    let mut entries = project
        .records
        .iter()
        .filter(|record| record.category == ContentCategory::Spaceship)
        .map(|record| (record.content_id.clone(), record.display_name.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

/// Decode one recipe and enforce that its embedded stable identity agrees
/// with the record/payload key. A mismatched wrapper must never enter the
/// published catalog under an identity it does not actually describe.
pub fn load_spaceship(project: &ForgeProject, content_id: &str) -> Result<SpacecraftSpec, String> {
    let Some(ContentPayload::Spaceship(recipe)) = project.payloads.get(content_id) else {
        return Err(format!("{content_id} has no spaceship payload"));
    };
    let value = recipe
        .fields
        .get(SPEC_FIELD)
        .ok_or_else(|| format!("{content_id} payload is missing {SPEC_FIELD}"))?;
    let spec: SpacecraftSpec =
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    if spec.content_id != content_id {
        return Err(format!(
            "{content_id} payload identity mismatch: embedded id is {}",
            spec.content_id
        ));
    }
    let issues = blocking_issues(&spec);
    if issues.is_empty() {
        Ok(spec)
    } else {
        Err(format!(
            "{content_id} carries an invalid spaceship recipe: {}",
            issues.join(" • ")
        ))
    }
}

/// Remove a spacecraft record and payload together without touching another
/// category that happens to share a malformed/conflicting id.
pub fn delete_spaceship(project: &mut ForgeProject, content_id: &str) -> Result<bool, String> {
    let Some(record) = project
        .records
        .iter()
        .find(|record| record.content_id == content_id)
    else {
        return Ok(false);
    };
    if record.category != ContentCategory::Spaceship {
        return Ok(false);
    }
    project
        .delete_content(content_id)
        .map(|_| true)
        .map_err(|error| format!("Cannot delete {content_id}: {error}"))
}

pub fn save_to_active_project(
    registry: &ForgeProjectRegistry,
    spec: &SpacecraftSpec,
) -> Result<String, String> {
    let issues = blocking_issues(spec);
    if !issues.is_empty() {
        return Err(format!("Fix before saving: {}", issues.join(" • ")));
    }
    let (store, mut project, source, revision) = open_active_store(registry)?;
    let revision = writable_revision(source, revision)?;
    let content_id = upsert_spaceship(&mut project, spec)?;
    store
        .compare_and_save(&mut project, &revision)
        .map_err(|error| format!("Project save failed: {error}"))?;
    Ok(content_id)
}

pub fn list_active_project(
    registry: &ForgeProjectRegistry,
) -> Result<(Vec<(String, String)>, bool), String> {
    open_active_store(registry).map(|(_, project, source, revision)| {
        (
            spaceship_entries(&project),
            source == ProjectLoadSource::Primary && revision.is_some(),
        )
    })
}

pub fn load_from_active_project(
    registry: &ForgeProjectRegistry,
    content_id: &str,
) -> Result<SpacecraftSpec, String> {
    let (_, project, _, _) = open_active_store(registry)?;
    load_spaceship(&project, content_id)
}

pub fn delete_from_active_project(
    registry: &ForgeProjectRegistry,
    content_id: &str,
) -> Result<bool, String> {
    let (store, mut project, source, revision) = open_active_store(registry)?;
    let removed = delete_spaceship(&mut project, content_id)?;
    if removed {
        let revision = writable_revision(source, revision)?;
        store
            .compare_and_save(&mut project, &revision)
            .map_err(|error| format!("Project save failed: {error}"))?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_classes_round_trip_as_spaceship_records() {
        let mut project = ForgeProject::default();
        for class in SpacecraftClass::ALL {
            let spec = SpacecraftSpec::preset(class);
            let id = upsert_spaceship(&mut project, &spec).unwrap();
            let loaded = load_spaceship(&project, &id).unwrap();
            assert_eq!(loaded.class, spec.class);
            assert_eq!(loaded.content_id, id);
        }
        assert_eq!(spaceship_entries(&project).len(), 5);
        assert!(project
            .records
            .iter()
            .any(|record| record.category == ContentCategory::Scene));
    }

    #[test]
    fn upsert_is_stable_and_delete_removes_record_and_payload_together() {
        let mut project = ForgeProject::default();
        let mut spec = SpacecraftSpec::preset(SpacecraftClass::Destroyer);
        let first = upsert_spaceship(&mut project, &spec).unwrap();
        spec.content_id.clone_from(&first);
        spec.weapon_mounts += 2;
        spec.display_name = "Vanguard Destroyer II".into();
        let second = upsert_spaceship(&mut project, &spec).unwrap();
        assert_eq!(first, second);
        assert_eq!(spaceship_entries(&project).len(), 1);
        assert_eq!(load_spaceship(&project, &first).unwrap().weapon_mounts, 16);
        assert!(delete_spaceship(&mut project, &first).unwrap());
        assert!(!project.payloads.contains_key(&first));
    }

    #[test]
    fn spaceship_cannot_overwrite_an_existing_category() {
        let mut project = ForgeProject::default();
        let spec = SpacecraftSpec {
            content_id: "starfall.spaceship.collision".into(),
            ..Default::default()
        };
        project.records.push(ContentRecord {
            content_id: spec.content_id.clone(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Creature,
            source_path: "creatures/spaceship_collision.json".into(),
            dependencies: Vec::new(),
            display_name: "Collision".into(),
            tags: Vec::new(),
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        });
        let error = upsert_spaceship(&mut project, &spec).unwrap_err();
        assert!(error.contains("already exists"));
        assert!(!project.payloads.contains_key(&spec.content_id));
    }

    #[test]
    fn load_rejects_wrapper_and_embedded_identity_mismatch() {
        let mut project = ForgeProject::default();
        let id = upsert_spaceship(&mut project, &SpacecraftSpec::default()).unwrap();
        let ContentPayload::Spaceship(recipe) = project.payloads.get_mut(&id).unwrap() else {
            panic!("spaceship payload expected");
        };
        recipe.fields.get_mut(SPEC_FIELD).unwrap()["content_id"] =
            serde_json::json!("starfall.spaceship.someone_else");
        let error = load_spaceship(&project, &id).unwrap_err();
        assert!(error.contains("identity mismatch"));
    }

    #[test]
    fn invalid_recipe_is_refused_before_project_mutation() {
        let mut project = ForgeProject::default();
        let spec = SpacecraftSpec {
            engine_count: 0,
            ..Default::default()
        };
        assert!(upsert_spaceship(&mut project, &spec).is_err());
        assert!(spaceship_entries(&project).is_empty());
    }

    #[test]
    fn public_field_contract_uses_spaceship_spec() {
        let mut project = ForgeProject::default();
        let id = upsert_spaceship(&mut project, &SpacecraftSpec::default()).unwrap();
        let ContentPayload::Spaceship(recipe) = project.payloads.get(&id).unwrap() else {
            panic!("spaceship payload expected");
        };
        assert!(recipe.fields.contains_key("spaceship_spec"));
    }

    #[test]
    fn a_new_template_cannot_silently_replace_an_existing_design() {
        let mut project = ForgeProject::default();
        let mut existing = SpacecraftSpec::preset(SpacecraftClass::Fighter);
        existing.display_name = "Night Lance".into();
        let id = upsert_spaceship(&mut project, &existing).unwrap();
        assert_eq!(id, "starfall.spaceship.night_lance");

        let mut collision = SpacecraftSpec::preset(SpacecraftClass::Bomber);
        collision.display_name = "Night Lance".into();
        let error = upsert_spaceship(&mut project, &collision).unwrap_err();
        assert!(error.contains("already exists"));
        assert_eq!(
            load_spaceship(&project, &id).unwrap().class,
            SpacecraftClass::Fighter
        );
    }

    #[test]
    fn derived_ids_stay_inside_the_spaceship_schema_limit() {
        let name = "İ".repeat(64);
        let id = content_id_for(&name);
        assert!(id.len() <= 96);
        assert!(id.starts_with("starfall.spaceship."));
    }

    #[test]
    fn deleting_a_referenced_spaceship_is_refused() {
        let mut project = ForgeProject::default();
        let id = upsert_spaceship(
            &mut project,
            &SpacecraftSpec::preset(SpacecraftClass::CommandShip),
        )
        .unwrap();
        project.records[0].dependencies.push(id.clone());
        let error = delete_spaceship(&mut project, &id).unwrap_err();
        assert!(error.contains("ContentHasDependents"));
        assert!(project.payloads.contains_key(&id));
    }
}
