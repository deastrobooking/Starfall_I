//! The Designer → Game publish step (docs/PROJECT_PLAN.md, P1).
//!
//! Authoring happens in versioned `ForgeProject` stores that consumers never
//! see. Publishing is the one-way bridge: validate every draft through the
//! store's own gate (`ForgeProject::publish_drafts`), then bake the published
//! weapons and creatures into plain JSON under `assets/published/` — the same
//! read-only pattern `moves.json` and `tricks.json` already prove out. The
//! consumer Game edition loads those files and nothing else; it has no writer
//! for any of this.
//!
//! Baked output is deterministic (records sorted by content id) so publishing
//! twice without edits produces byte-identical files and clean VCS diffs.

use std::path::{Path, PathBuf};

use super::persistence::{ContentCategory, ForgeProject, ProjectStore};
use super::project_registry::ForgeProjectRegistry;
use super::weapon_records;
use crate::combat::weapon_forge::WeaponSpec;
use crate::robots::creature::CreatureSpec;

const RECOVERY_LIMIT: usize = 3;

/// One published weapon: the design plus the stable id the game resolves it
/// by (equip slots and save files reference this id).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PublishedWeapon {
    pub content_id: String,
    pub spec: WeaponSpec,
}

/// What a publish run produced, for the hub status line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PublishReport {
    pub weapons: usize,
    pub creatures: usize,
    /// Records in categories that have no game-side loader yet.
    pub skipped: usize,
}

impl PublishReport {
    pub fn summary(&self) -> String {
        let mut parts = vec![format!(
            "Published {} weapon(s), {} creature(s)",
            self.weapons, self.creatures
        )];
        if self.skipped > 0 {
            parts.push(format!("{} record(s) have no loader yet", self.skipped));
        }
        parts.join(" • ")
    }
}

/// The directory the Game edition reads published content from.
pub fn published_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("published")
}

/// Collect every publishable weapon, sorted by content id for deterministic
/// output.
fn bake_weapons(project: &ForgeProject) -> Result<Vec<PublishedWeapon>, String> {
    let mut weapons = Vec::new();
    for (content_id, _) in weapon_records::weapon_entries(project) {
        let spec = weapon_records::load_weapon(project, &content_id)?;
        weapons.push(PublishedWeapon { content_id, spec });
    }
    weapons.sort_by(|a, b| a.content_id.cmp(&b.content_id));
    Ok(weapons)
}

/// Collect every publishable creature, sorted by content id.
fn bake_creatures(project: &ForgeProject) -> Result<Vec<CreatureSpec>, String> {
    let mut creatures = Vec::new();
    for (content_id, _) in super::creature_records::creature_entries(project) {
        creatures.push(super::creature_records::load_creature(
            project,
            &content_id,
        )?);
    }
    creatures.sort_by(|a, b| a.content_id.cmp(&b.content_id));
    Ok(creatures)
}

/// Records that exist in the project but have no baked representation yet.
fn unbaked_record_count(project: &ForgeProject) -> usize {
    project
        .records
        .iter()
        .filter(|record| {
            !matches!(
                record.category,
                ContentCategory::Weapon | ContentCategory::Creature
            )
        })
        .count()
}

/// Bake a project's publishable content into `out_dir`.
///
/// Separated from the store handling so tests can publish into a temp
/// directory without touching the repository's real `assets/published/`.
pub fn bake_project_to(project: &ForgeProject, out_dir: &Path) -> Result<PublishReport, String> {
    let weapons = bake_weapons(project)?;
    let creatures = bake_creatures(project)?;
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

    let weapons_json = serde_json::to_string_pretty(&weapons).map_err(|e| e.to_string())?;
    std::fs::write(out_dir.join("weapons.json"), weapons_json).map_err(|e| e.to_string())?;

    let creatures_json = serde_json::to_string_pretty(&creatures).map_err(|e| e.to_string())?;
    std::fs::write(out_dir.join("creatures.json"), creatures_json).map_err(|e| e.to_string())?;

    Ok(PublishReport {
        weapons: weapons.len(),
        creatures: creatures.len(),
        skipped: unbaked_record_count(project),
    })
}

/// Publish the active project: run the store's own validate-and-promote gate,
/// persist the published hashes, then bake to `assets/published/`.
pub fn publish_active_project(registry: &ForgeProjectRegistry) -> Result<PublishReport, String> {
    let Some(active) = registry.active.as_deref() else {
        return Err("No active project — open one in the Project Hub first".to_string());
    };
    let store = ProjectStore::new(active, RECOVERY_LIMIT);
    let (mut project, _) = store
        .load_with_recovery()
        .map_err(|e| format!("Could not load active project: {e}"))?;

    // The store's gate: every draft must validate before anything is
    // promoted. A project with one broken record publishes nothing.
    project
        .publish_drafts()
        .map_err(|e| format!("Validation failed: {e}"))?;
    store
        .save(&mut project)
        .map_err(|e| format!("Could not persist published hashes: {e}"))?;

    bake_project_to(&project, &published_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::weapon_forge::{EmitterStyle, GripStyle};

    fn project_with_weapons(names: &[&str]) -> ForgeProject {
        let mut project = ForgeProject::default();
        for name in names {
            let spec = WeaponSpec {
                name: name.to_string(),
                grip: GripStyle::Extended,
                emitter: EmitterStyle::Prism,
                ..Default::default()
            };
            weapon_records::upsert_weapon(&mut project, &spec).expect("valid spec saves");
        }
        project
    }

    #[test]
    fn baking_is_deterministic_regardless_of_authoring_order() {
        // Same content, opposite insertion order → byte-identical output.
        let a = bake_weapons(&project_with_weapons(&["Zenith", "Aurora"])).unwrap();
        let b = bake_weapons(&project_with_weapons(&["Aurora", "Zenith"])).unwrap();
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap(),
            "publish output must not depend on authoring order"
        );
        assert_eq!(a[0].content_id, "starfall.weapon.aurora");
    }

    #[test]
    fn a_published_project_round_trips_through_the_baked_files() {
        let dir =
            std::env::temp_dir().join(format!("starfall_publish_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let project = project_with_weapons(&["Round Trip"]);
        let report = bake_project_to(&project, &dir).expect("bake succeeds");
        assert_eq!(report.weapons, 1);
        assert_eq!(report.creatures, 0);

        // The game-side reader must get back exactly what was authored.
        let text = std::fs::read_to_string(dir.join("weapons.json")).unwrap();
        let loaded: Vec<PublishedWeapon> = serde_json::from_str(&text).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content_id, "starfall.weapon.round_trip");
        assert_eq!(loaded[0].spec.grip, GripStyle::Extended);
        // And an empty creatures file is still a valid file, not an error.
        let creatures: Vec<CreatureSpec> =
            serde_json::from_str(&std::fs::read_to_string(dir.join("creatures.json")).unwrap())
                .unwrap();
        assert!(creatures.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_without_a_loader_are_counted_not_silently_dropped() {
        use super::super::persistence::{ContentRecord, CURRENT_PROJECT_SCHEMA};
        let mut project = project_with_weapons(&["Solo"]);
        // A default project may seed non-loader records of its own; measure
        // the delta this scene adds rather than assuming a clean slate.
        let baseline = unbaked_record_count(&project);
        project.records.push(ContentRecord {
            content_id: "starfall.scene.test".into(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Scene,
            source_path: "scenes/test.json".into(),
            dependencies: Vec::new(),
            display_name: "Test Scene".into(),
            tags: Vec::new(),
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        });

        assert_eq!(unbaked_record_count(&project), baseline + 1);
        let report = PublishReport {
            weapons: 1,
            creatures: 0,
            skipped: 1,
        };
        assert!(report.summary().contains("no loader yet"));
    }
}
