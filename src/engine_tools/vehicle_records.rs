//! Project-backed records for Vehicle Forge recipes.
//!
//! Vehicle designs inherit the same compare-and-save revision contract,
//! recovery policy, atomic source writes, and draft/published hashes as other
//! Forge content. Runtime code consumes baked [`VehicleSpec`] values instead
//! of opening a writable project store.

use super::persistence::{
    ContentCategory, ContentPayload, ContentRecord, ForgeProject, GenericRecipeDraft,
    ProjectLoadSource, ProjectStore, CURRENT_PROJECT_SCHEMA,
};
use super::project_registry::ForgeProjectRegistry;
use crate::vehicle_forge::{VehicleIssueSeverity, VehicleSpec};

const SPEC_FIELD: &str = "vehicle_spec";
const RECOVERY_LIMIT: usize = 3;

fn is_template_content_id(content_id: &str) -> bool {
    matches!(
        content_id,
        "starfall.vehicle.new_boat"
            | "starfall.vehicle.new_car"
            | "starfall.vehicle.new_truck"
            | "starfall.vehicle.new_motorcycle"
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
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "starfall.vehicle.unnamed".to_string()
    } else {
        format!("starfall.vehicle.{slug}")
    }
}

pub fn blocking_issues(spec: &VehicleSpec) -> Vec<String> {
    spec.validate()
        .into_iter()
        .filter(|issue| issue.severity == VehicleIssueSeverity::Error)
        .map(|issue| issue.message)
        .collect()
}

pub fn upsert_vehicle(project: &mut ForgeProject, spec: &VehicleSpec) -> Result<String, String> {
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
            if record.category != ContentCategory::Vehicle {
                return Err(format!(
                    "{content_id} already exists as {:?} content",
                    record.category
                ));
            }
            record.display_name = spec.display_name.clone();
            record.tags = vec!["vehicle".into(), class_tag(spec).into()];
        }
        None => project.records.push(ContentRecord {
            content_id: content_id.clone(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Vehicle,
            source_path: format!("vehicles/{}.json", content_id.replace('.', "_")),
            dependencies: Vec::new(),
            display_name: spec.display_name.clone(),
            tags: vec!["vehicle".into(), class_tag(spec).into()],
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        }),
    }
    project
        .payloads
        .insert(content_id.clone(), ContentPayload::Vehicle(recipe));
    Ok(content_id)
}

fn class_tag(spec: &VehicleSpec) -> &'static str {
    match spec.class {
        crate::vehicle_forge::VehicleClass::Boat => "boat",
        crate::vehicle_forge::VehicleClass::Car => "car",
        crate::vehicle_forge::VehicleClass::Truck => "truck",
        crate::vehicle_forge::VehicleClass::Motorcycle => "motorcycle",
    }
}

pub fn vehicle_entries(project: &ForgeProject) -> Vec<(String, String)> {
    project
        .records
        .iter()
        .filter(|record| record.category == ContentCategory::Vehicle)
        .map(|record| (record.content_id.clone(), record.display_name.clone()))
        .collect()
}

pub fn load_vehicle(project: &ForgeProject, content_id: &str) -> Result<VehicleSpec, String> {
    let Some(ContentPayload::Vehicle(recipe)) = project.payloads.get(content_id) else {
        return Err(format!("{content_id} has no vehicle payload"));
    };
    let value = recipe
        .fields
        .get(SPEC_FIELD)
        .ok_or_else(|| format!("{content_id} payload is missing {SPEC_FIELD}"))?;
    let spec: VehicleSpec =
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
            "{content_id} carries an invalid vehicle recipe: {}",
            issues.join(" • ")
        ))
    }
}

pub fn delete_vehicle(project: &mut ForgeProject, content_id: &str) -> Result<bool, String> {
    let Some(record) = project
        .records
        .iter()
        .find(|record| record.content_id == content_id)
    else {
        return Ok(false);
    };
    if record.category != ContentCategory::Vehicle {
        return Ok(false);
    }
    project
        .delete_content(content_id)
        .map(|_| true)
        .map_err(|error| format!("Cannot delete {content_id}: {error}"))
}

pub fn save_to_active_project(
    registry: &ForgeProjectRegistry,
    spec: &VehicleSpec,
) -> Result<String, String> {
    let issues = blocking_issues(spec);
    if !issues.is_empty() {
        return Err(format!("Fix before saving: {}", issues.join(" • ")));
    }
    let (store, mut project, source, revision) = open_active_store(registry)?;
    let revision = writable_revision(source, revision)?;
    let content_id = upsert_vehicle(&mut project, spec)?;
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
            vehicle_entries(&project),
            source == ProjectLoadSource::Primary && revision.is_some(),
        )
    })
}

pub fn load_from_active_project(
    registry: &ForgeProjectRegistry,
    content_id: &str,
) -> Result<VehicleSpec, String> {
    let (_, project, _, _) = open_active_store(registry)?;
    load_vehicle(&project, content_id)
}

pub fn delete_from_active_project(
    registry: &ForgeProjectRegistry,
    content_id: &str,
) -> Result<bool, String> {
    let (store, mut project, source, revision) = open_active_store(registry)?;
    let removed = delete_vehicle(&mut project, content_id)?;
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
    use crate::vehicle_forge::VehicleClass;

    #[test]
    fn vehicle_records_round_trip_without_touching_other_categories() {
        let mut project = ForgeProject::default();
        let spec = VehicleSpec::preset(VehicleClass::Truck);
        let content_id = upsert_vehicle(&mut project, &spec).unwrap();
        let loaded = load_vehicle(&project, &content_id).unwrap();
        assert_eq!(loaded.class, VehicleClass::Truck);
        assert_eq!(loaded.content_id, content_id);
        assert_eq!(vehicle_entries(&project).len(), 1);
        assert!(project
            .records
            .iter()
            .any(|record| record.category == ContentCategory::Scene));
    }

    #[test]
    fn upsert_is_stable_and_delete_removes_record_and_payload_together() {
        let mut project = ForgeProject::default();
        let mut spec = VehicleSpec::preset(VehicleClass::Car);
        spec.display_name = "Moon Runner".into();
        let first = upsert_vehicle(&mut project, &spec).unwrap();
        spec.content_id.clone_from(&first);
        spec.power_kw += 50.0;
        let second = upsert_vehicle(&mut project, &spec).unwrap();
        assert_eq!(first, second);
        assert_eq!(vehicle_entries(&project).len(), 1);
        assert!(delete_vehicle(&mut project, &first).unwrap());
        assert!(!project.payloads.contains_key(&first));
    }

    #[test]
    fn a_vehicle_cannot_overwrite_an_existing_non_vehicle_record() {
        let mut project = ForgeProject::default();
        project.records.push(ContentRecord {
            content_id: "starfall.vehicle.collision".into(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Creature,
            source_path: "creatures/collision.json".into(),
            dependencies: Vec::new(),
            display_name: "Collision".into(),
            tags: Vec::new(),
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        });
        project.payloads.insert(
            "starfall.vehicle.collision".into(),
            ContentPayload::Creature(GenericRecipeDraft::default()),
        );
        let mut spec = VehicleSpec::preset(VehicleClass::Car);
        spec.content_id = "starfall.vehicle.collision".into();
        let error = upsert_vehicle(&mut project, &spec).unwrap_err();
        assert!(error.contains("already exists"));
    }

    #[test]
    fn content_id_slugging_never_emits_unicode_the_schema_rejects() {
        assert_eq!(
            content_id_for("Étoile Runner"),
            "starfall.vehicle.toile_runner"
        );
    }

    #[test]
    fn public_field_contract_uses_vehicle_spec() {
        let mut project = ForgeProject::default();
        let id = upsert_vehicle(&mut project, &VehicleSpec::default()).unwrap();
        let ContentPayload::Vehicle(recipe) = project.payloads.get(&id).unwrap() else {
            panic!("vehicle payload expected");
        };
        assert!(recipe.fields.contains_key("vehicle_spec"));
    }

    #[test]
    fn load_rejects_wrapper_and_embedded_identity_mismatch() {
        let mut project = ForgeProject::default();
        let id = upsert_vehicle(&mut project, &VehicleSpec::default()).unwrap();
        let ContentPayload::Vehicle(recipe) = project.payloads.get_mut(&id).unwrap() else {
            panic!("vehicle payload expected");
        };
        let value = recipe.fields.get_mut("vehicle_spec").unwrap();
        value["content_id"] = serde_json::json!("starfall.vehicle.someone_else");
        let error = load_vehicle(&project, &id).unwrap_err();
        assert!(error.contains("identity mismatch"));
    }

    #[test]
    fn a_renamed_vehicle_updates_in_place_on_its_next_save() {
        let mut project = ForgeProject::default();
        let original =
            upsert_vehicle(&mut project, &VehicleSpec::preset(VehicleClass::Motorcycle)).unwrap();
        let renamed = "starfall.vehicle.night_runner";
        project.rename_content(&original, renamed).unwrap();

        let mut loaded = load_vehicle(&project, renamed).unwrap();
        loaded.display_name = "Night Runner Mark II".into();
        assert_eq!(upsert_vehicle(&mut project, &loaded).unwrap(), renamed);
        assert_eq!(vehicle_entries(&project).len(), 1);
    }

    #[test]
    fn a_new_template_cannot_silently_replace_an_existing_design() {
        let mut project = ForgeProject::default();
        let mut existing = VehicleSpec::preset(VehicleClass::Car);
        existing.display_name = "Moon Runner".into();
        let id = upsert_vehicle(&mut project, &existing).unwrap();
        assert_eq!(id, "starfall.vehicle.moon_runner");

        let mut collision = VehicleSpec::preset(VehicleClass::Truck);
        collision.display_name = "Moon Runner".into();
        let error = upsert_vehicle(&mut project, &collision).unwrap_err();
        assert!(error.contains("already exists"));
        assert_eq!(
            load_vehicle(&project, &id).unwrap().class,
            VehicleClass::Car
        );
    }

    #[test]
    fn deleting_a_referenced_vehicle_is_refused() {
        let mut project = ForgeProject::default();
        let id = upsert_vehicle(&mut project, &VehicleSpec::preset(VehicleClass::Boat)).unwrap();
        project.records[0].dependencies.push(id.clone());
        let error = delete_vehicle(&mut project, &id).unwrap_err();
        assert!(error.contains("ContentHasDependents"));
        assert!(project.payloads.contains_key(&id));
    }
}
