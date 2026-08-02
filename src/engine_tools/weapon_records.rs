//! Bridge between the Weapon Forge and the versioned project contract.
//!
//! Mirrors `creature_records`: a designed weapon is a
//! [`ContentCategory::Weapon`] record whose payload carries the complete
//! [`WeaponSpec`] as a generator field, so weapon authoring inherits the same
//! atomic writes, recovery snapshots, draft/published hashes, and validation
//! gates as every other content type rather than inventing its own save path.

// In the consumer Game edition the authoring panel that consumes this
// module is compiled out (docs/PROJECT_PLAN.md P0); the model itself is
// removed from that build in P2.
#![cfg_attr(not(feature = "designer"), allow(dead_code))]

use super::persistence::{
    ContentCategory, ContentPayload, ContentRecord, ForgeProject, GenericRecipeDraft, ProjectStore,
    CURRENT_PROJECT_SCHEMA,
};
use super::project_registry::ForgeProjectRegistry;
use crate::combat::weapon_forge::WeaponSpec;

const SPEC_FIELD: &str = "weapon_spec";
const RECOVERY_LIMIT: usize = 3;

/// Open the active project's store. Keeps the persistence layer private to
/// `engine_tools` — tool plugins go through these helpers rather than reaching
/// into the store themselves.
fn open_active_store(
    registry: &ForgeProjectRegistry,
) -> Result<(ProjectStore, ForgeProject), String> {
    let Some(active) = registry.active.as_deref() else {
        return Err("No active project — open one in the Project Hub first".to_string());
    };
    let store = ProjectStore::new(active, RECOVERY_LIMIT);
    let (project, _) = store
        .load_with_recovery()
        .map_err(|e| format!("Could not load active project: {e}"))?;
    Ok((store, project))
}

/// Save `spec` into the active project through the full store contract,
/// returning the content id it landed under.
pub fn save_to_active_project(
    registry: &ForgeProjectRegistry,
    spec: &WeaponSpec,
) -> Result<String, String> {
    let issues = blocking_issues(spec);
    if !issues.is_empty() {
        return Err(format!("Fix before saving: {}", issues.join(" • ")));
    }
    let (store, mut project) = open_active_store(registry)?;
    let id = upsert_weapon(&mut project, spec)?;
    store
        .save(&mut project)
        .map_err(|e| format!("Project save failed: {e}"))?;
    Ok(id)
}

/// List the active project's weapon records.
pub fn list_active_project(registry: &ForgeProjectRegistry) -> Vec<(String, String)> {
    open_active_store(registry)
        .map(|(_, project)| weapon_entries(&project))
        .unwrap_or_default()
}

/// Delete one weapon design from the active project, through the full store
/// contract so the removal is atomic and snapshotted like any other edit.
pub fn delete_from_active_project(
    registry: &ForgeProjectRegistry,
    content_id: &str,
) -> Result<bool, String> {
    let (store, mut project) = open_active_store(registry)?;
    let removed = delete_weapon(&mut project, content_id);
    if removed {
        store
            .save(&mut project)
            .map_err(|e| format!("Project save failed: {e}"))?;
    }
    Ok(removed)
}

/// Load one weapon design from the active project.
pub fn load_from_active_project(
    registry: &ForgeProjectRegistry,
    content_id: &str,
) -> Result<WeaponSpec, String> {
    let (_, project) = open_active_store(registry)?;
    load_weapon(&project, content_id)
}

/// Stable content id for a weapon name. Ids are the join key for records and
/// payloads, so they must be derived deterministically rather than generated.
pub fn content_id_for(name: &str) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let slug = slug.trim_matches('_').to_string();
    if slug.is_empty() {
        "starfall.weapon.unnamed".to_string()
    } else {
        format!("starfall.weapon.{slug}")
    }
}

/// Messages that must block a save. Empty means the design is publishable.
pub fn blocking_issues(spec: &WeaponSpec) -> Vec<String> {
    spec.validate()
        .into_iter()
        .filter(|issue| issue.blocking)
        .map(|issue| issue.message)
        .collect()
}

/// Insert or update `spec` as a Weapon record + payload.
///
/// Refuses designs that fail validation, so an unsaveable weapon can never
/// reach the project on disk.
pub fn upsert_weapon(project: &mut ForgeProject, spec: &WeaponSpec) -> Result<String, String> {
    let blocking = blocking_issues(spec);
    if !blocking.is_empty() {
        return Err(blocking.join("; "));
    }
    let content_id = content_id_for(&spec.name);
    let value = serde_json::to_value(spec).map_err(|e| e.to_string())?;
    let mut recipe = GenericRecipeDraft::default();
    recipe.fields.insert(SPEC_FIELD.to_string(), value);

    match project
        .records
        .iter_mut()
        .find(|record| record.content_id == content_id)
    {
        Some(record) => {
            // Never let a weapon quietly overwrite another content type that
            // happens to share an id.
            if record.category != ContentCategory::Weapon {
                return Err(format!(
                    "{content_id} already exists as {:?} content",
                    record.category
                ));
            }
            record.display_name = spec.name.clone();
        }
        None => project.records.push(ContentRecord {
            content_id: content_id.clone(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Weapon,
            source_path: format!("weapons/{}.json", content_id.replace('.', "_")),
            dependencies: Vec::new(),
            display_name: spec.name.clone(),
            tags: vec!["weapon".to_string(), "sabre".to_string()],
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        }),
    }
    project
        .payloads
        .insert(content_id.clone(), ContentPayload::Weapon(recipe));
    Ok(content_id)
}

/// Every weapon record in the project as `(content_id, display_name)`.
pub fn weapon_entries(project: &ForgeProject) -> Vec<(String, String)> {
    project
        .records
        .iter()
        .filter(|record| record.category == ContentCategory::Weapon)
        .map(|record| (record.content_id.clone(), record.display_name.clone()))
        .collect()
}

/// Decode the stored design for `content_id`.
pub fn load_weapon(project: &ForgeProject, content_id: &str) -> Result<WeaponSpec, String> {
    let Some(ContentPayload::Weapon(recipe)) = project.payloads.get(content_id) else {
        return Err(format!("{content_id} has no weapon payload"));
    };
    let value = recipe
        .fields
        .get(SPEC_FIELD)
        .ok_or_else(|| format!("{content_id} payload is missing {SPEC_FIELD}"))?;
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
}

/// Remove a weapon's record and payload together, so neither is orphaned.
pub fn delete_weapon(project: &mut ForgeProject, content_id: &str) -> bool {
    let before = project.records.len();
    project
        .records
        .retain(|record| record.content_id != content_id);
    let removed = project.records.len() != before;
    project.payloads.remove(content_id);
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::weapon_forge::{EmitterStyle, GripStyle};

    fn spec(name: &str) -> WeaponSpec {
        WeaponSpec {
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn content_ids_are_stable_slugs() {
        assert_eq!(content_id_for("Star Reaver"), "starfall.weapon.star_reaver");
        // Same name, same id — saving twice must update, not duplicate.
        assert_eq!(content_id_for("Star Reaver"), content_id_for("star reaver"));
        // Punctuation and padding cannot produce a ragged id.
        assert_eq!(content_id_for("  Edge!!  "), "starfall.weapon.edge");
        assert_eq!(content_id_for(""), "starfall.weapon.unnamed");
        assert_eq!(content_id_for("***"), "starfall.weapon.unnamed");
    }

    #[test]
    fn a_saved_weapon_round_trips_through_the_project() {
        let mut project = ForgeProject::default();
        let original = WeaponSpec {
            name: "Prism Edge".into(),
            grip: GripStyle::Extended,
            emitter: EmitterStyle::Prism,
            blade_length: 1.4,
            ..Default::default()
        };

        let id = upsert_weapon(&mut project, &original).expect("saves");
        assert_eq!(weapon_entries(&project).len(), 1);

        let loaded = load_weapon(&project, &id).expect("loads");
        assert_eq!(loaded, original);
        // And the design still produces the same weapon after the round trip.
        assert_eq!(
            loaded.to_blade_profile().trait_,
            original.to_blade_profile().trait_
        );
    }

    #[test]
    fn saving_the_same_name_updates_in_place_instead_of_duplicating() {
        let mut project = ForgeProject::default();
        upsert_weapon(&mut project, &spec("Twin")).unwrap();
        let revised = WeaponSpec {
            blade_length: 1.6,
            ..spec("Twin")
        };
        let id = upsert_weapon(&mut project, &revised).unwrap();

        assert_eq!(weapon_entries(&project).len(), 1, "must not duplicate");
        assert_eq!(load_weapon(&project, &id).unwrap().blade_length, 1.6);
    }

    #[test]
    fn an_invalid_design_never_reaches_the_project() {
        let mut project = ForgeProject::default();
        let unnamed = spec("   ");
        assert!(upsert_weapon(&mut project, &unnamed).is_err());
        assert!(
            weapon_entries(&project).is_empty(),
            "a rejected save must leave the project untouched"
        );
    }

    #[test]
    fn a_weapon_cannot_hijack_another_content_types_id() {
        let mut project = ForgeProject::default();
        let id = content_id_for("Shared");
        project.records.push(ContentRecord {
            content_id: id.clone(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Creature,
            source_path: "creatures/shared.json".into(),
            dependencies: Vec::new(),
            display_name: "Shared".into(),
            tags: Vec::new(),
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        });

        let err = upsert_weapon(&mut project, &spec("Shared")).unwrap_err();
        assert!(err.contains("Creature"), "error should name the conflict");
    }

    #[test]
    fn deleting_removes_the_record_and_its_payload_together() {
        let mut project = ForgeProject::default();
        let id = upsert_weapon(&mut project, &spec("Scrap")).unwrap();
        assert!(delete_weapon(&mut project, &id));
        assert!(weapon_entries(&project).is_empty());
        assert!(
            load_weapon(&project, &id).is_err(),
            "payload must not outlive its record"
        );
        // Deleting something absent is a no-op, not a panic.
        assert!(!delete_weapon(&mut project, &id));
    }
}
