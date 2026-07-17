//! PM2 presets: immutable versioned content records captured from editor
//! payloads.
//!
//! A preset stores the complete parameters of one content payload (including
//! its seed for procedural generators) under a stable `preset_id` plus a
//! monotonically growing `revision`. Stored preset files are never rewritten:
//! every operation that changes anything — forking, new-seed variants —
//! produces a new record, and [`PresetStore::save`] refuses to overwrite an
//! existing `(preset_id, revision)` file. Presets live in the project's
//! `presets/` folder next to `project.json` and are written atomically.

use std::path::{Path, PathBuf};

use bevy::prelude::warn;
use serde::{Deserialize, Serialize};

use super::persistence::{atomic_write, ContentCategory, ContentPayload, ContentRecord};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresetRecord {
    pub preset_id: String,
    pub revision: u32,
    pub name: String,
    pub category: ContentCategory,
    /// Generator fingerprint: lowercase payload category name.
    pub generator: String,
    /// The content id this preset was captured from.
    pub source_content_id: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    /// Complete parameters, including the seed for procedural payloads.
    pub payload: ContentPayload,
}

impl PresetRecord {
    pub fn seed(&self) -> Option<u64> {
        self.payload.procedural_recipe().map(|recipe| recipe.seed)
    }

    /// Short display line for editor status output.
    pub fn summary(&self) -> String {
        match self.seed() {
            Some(seed) => format!("{} r{} (seed {seed})", self.preset_id, self.revision),
            None => format!("{} r{}", self.preset_id, self.revision),
        }
    }
}

/// Capture the payload of `record` as a new revision-1 preset with a unique
/// stable id derived from the source content id.
pub fn capture(
    record: &ContentRecord,
    payload: &ContentPayload,
    existing: &[PresetRecord],
) -> PresetRecord {
    let base = record.content_id.replace('.', "-");
    let preset_id = (1..)
        .map(|n| format!("{base}-preset-{n}"))
        .find(|candidate| !existing.iter().any(|preset| &preset.preset_id == candidate))
        .expect("unbounded numbering always finds a free preset id");
    PresetRecord {
        preset_id,
        revision: 1,
        name: format!("{} Preset", record.display_name),
        category: record.category,
        generator: format!("{:?}", payload.category()).to_lowercase(),
        source_content_id: record.content_id.clone(),
        tags: record.tags.clone(),
        dependencies: record.dependencies.clone(),
        thumbnail: record.thumbnail.clone(),
        payload: payload.clone(),
    }
}

/// Apply the preset's complete parameters onto a live payload of the same
/// category. Returns the applied seed (if procedural) for status output.
pub fn apply(preset: &PresetRecord, payload: &mut ContentPayload) -> Result<Option<u64>, String> {
    if payload.category() != preset.payload.category() {
        return Err(format!(
            "Preset {} is {:?} content and cannot apply to {:?}",
            preset.preset_id,
            preset.payload.category(),
            payload.category()
        ));
    }
    *payload = preset.payload.clone();
    Ok(preset.seed())
}

/// Human-readable differences between the preset and a live payload. Empty
/// when the payload matches the preset exactly.
pub fn compare(preset: &PresetRecord, payload: &ContentPayload) -> Vec<String> {
    let mut differences = Vec::new();
    if payload.category() != preset.payload.category() {
        differences.push(format!(
            "category: preset {:?} vs live {:?}",
            preset.payload.category(),
            payload.category()
        ));
        return differences;
    }
    match (preset.payload.procedural_recipe(), payload.procedural_recipe()) {
        (Some(stored), Some(live)) => {
            if stored.seed != live.seed {
                differences.push(format!("seed: preset {} vs live {}", stored.seed, live.seed));
            }
            if stored.revision != live.revision {
                differences.push(format!(
                    "revision: preset {} vs live {}",
                    stored.revision, live.revision
                ));
            }
            if stored.material_slots != live.material_slots {
                differences.push("material slots differ".to_string());
            }
            if stored.fields != live.fields {
                differences.push("generator fields differ".to_string());
            }
            if stored.spline_points != live.spline_points
                || stored.road_junctions != live.road_junctions
                || stored.topology_nodes != live.topology_nodes
                || stored.topology_sockets != live.topology_sockets
                || stored.topology_edges != live.topology_edges
            {
                differences.push("authored geometry differs".to_string());
            }
            if stored.terrain_projection != live.terrain_projection {
                differences.push("terrain projection differs".to_string());
            }
        }
        _ => {
            if &preset.payload != payload {
                differences.push("parameters differ".to_string());
            }
        }
    }
    differences
}

/// Fork a preset into a new independent lineage: fresh stable id, revision 1,
/// identical parameters.
pub fn fork(preset: &PresetRecord, existing: &[PresetRecord]) -> PresetRecord {
    let preset_id = (1..)
        .map(|n| format!("{}-fork-{n}", preset.preset_id))
        .find(|candidate| !existing.iter().any(|other| &other.preset_id == candidate))
        .expect("unbounded numbering always finds a free fork id");
    PresetRecord {
        preset_id,
        revision: 1,
        name: format!("Fork of {}", preset.name),
        ..preset.clone()
    }
}

/// Deterministic successor seed (LCG step) so variants never depend on
/// wall-clock or thread randomness.
pub fn successor_seed(seed: u64) -> u64 {
    seed.wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

/// Produce the next revision of this preset lineage with a new seed. Only
/// procedural payloads carry seeds; other categories are rejected.
pub fn new_seed_variant(
    preset: &PresetRecord,
    existing: &[PresetRecord],
) -> Result<PresetRecord, String> {
    let mut payload = preset.payload.clone();
    let Some(recipe) = payload.procedural_recipe_mut() else {
        return Err(format!(
            "{:?} presets have no seed to vary",
            preset.payload.category()
        ));
    };
    recipe.seed = successor_seed(recipe.seed);
    let next_revision = existing
        .iter()
        .filter(|other| other.preset_id == preset.preset_id)
        .map(|other| other.revision)
        .max()
        .unwrap_or(preset.revision)
        + 1;
    Ok(PresetRecord {
        revision: next_revision,
        payload,
        ..preset.clone()
    })
}

/// Filesystem home of a project's presets: `presets/` beside `project.json`.
#[derive(Debug, Clone)]
pub struct PresetStore {
    dir: PathBuf,
}

impl PresetStore {
    pub fn for_project(project_path: &Path) -> Self {
        let dir = project_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("presets");
        Self { dir }
    }

    fn preset_path(&self, preset: &PresetRecord) -> PathBuf {
        self.dir
            .join(format!("{}.r{}.json", preset.preset_id, preset.revision))
    }

    /// Write one immutable preset file. Refuses to overwrite an existing
    /// `(preset_id, revision)` — stored presets never change.
    pub fn save(&self, preset: &PresetRecord) -> Result<(), String> {
        let path = self.preset_path(preset);
        if path.exists() {
            return Err(format!(
                "{} r{} already exists; presets are immutable",
                preset.preset_id, preset.revision
            ));
        }
        let json = serde_json::to_string_pretty(preset).map_err(|e| e.to_string())?;
        atomic_write(&path, json.as_bytes()).map_err(|e| format!("{e:?}"))
    }

    /// Load every readable preset, sorted by (preset_id, revision). Corrupt
    /// files are warned about and skipped so one bad file never hides the
    /// rest of the library.
    pub fn load_all(&self) -> Vec<PresetRecord> {
        let mut presets = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(_) => return presets,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|json| serde_json::from_str::<PresetRecord>(&json).map_err(|e| e.to_string()))
            {
                Ok(preset) => presets.push(preset),
                Err(error) => warn!("Skipping unreadable preset {}: {error}", path.display()),
            }
        }
        presets.sort_by(|a, b| {
            a.preset_id
                .cmp(&b.preset_id)
                .then(a.revision.cmp(&b.revision))
        });
        presets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_tools::persistence::{ProceduralRecipeDraft, CURRENT_PROJECT_SCHEMA};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_project_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "starfall_presets_{label}_{}_{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("test root should be creatable");
        root.join("project.json")
    }

    fn biome_record() -> ContentRecord {
        ContentRecord {
            content_id: "biome.test_ridge".into(),
            schema_version: CURRENT_PROJECT_SCHEMA,
            category: ContentCategory::Biome,
            source_path: "biomes/test_ridge.json".into(),
            dependencies: vec!["scene.main_world".into()],
            display_name: "Test Ridge".into(),
            tags: vec!["biome".into()],
            thumbnail: None,
            draft_hash: String::new(),
            published_hash: None,
        }
    }

    fn biome_payload(seed: u64) -> ContentPayload {
        ContentPayload::Biome(ProceduralRecipeDraft {
            seed,
            ..Default::default()
        })
    }

    #[test]
    fn capture_apply_round_trip_restores_seed_and_parameters() {
        let record = biome_record();
        let preset = capture(&record, &biome_payload(42), &[]);
        assert_eq!(preset.preset_id, "biome-test_ridge-preset-1");
        assert_eq!(preset.revision, 1);
        assert_eq!(preset.generator, "biome");
        assert_eq!(preset.seed(), Some(42));

        let mut live = biome_payload(7);
        assert!(!compare(&preset, &live).is_empty());
        let applied_seed = apply(&preset, &mut live).expect("same-category apply succeeds");
        assert_eq!(applied_seed, Some(42));
        assert!(compare(&preset, &live).is_empty());
    }

    #[test]
    fn apply_rejects_category_mismatch() {
        let record = biome_record();
        let preset = capture(&record, &biome_payload(42), &[]);
        let mut road = ContentPayload::Road(ProceduralRecipeDraft::default());
        assert!(apply(&preset, &mut road).is_err());
    }

    #[test]
    fn fork_starts_a_new_lineage_and_variant_bumps_revision_with_new_seed() {
        let record = biome_record();
        let preset = capture(&record, &biome_payload(42), &[]);

        let forked = fork(&preset, std::slice::from_ref(&preset));
        assert_eq!(forked.preset_id, "biome-test_ridge-preset-1-fork-1");
        assert_eq!(forked.revision, 1);
        assert_eq!(forked.payload, preset.payload);

        let library = vec![preset.clone(), forked];
        let variant = new_seed_variant(&preset, &library).expect("procedural variant succeeds");
        assert_eq!(variant.preset_id, preset.preset_id);
        assert_eq!(variant.revision, 2);
        assert_ne!(variant.seed(), preset.seed());
        assert_eq!(variant.seed(), Some(successor_seed(42)));

        // Non-procedural payloads cannot vary a seed.
        let scene_preset = PresetRecord {
            payload: ContentPayload::Scene(Default::default()),
            ..preset
        };
        assert!(new_seed_variant(&scene_preset, &library).is_err());
    }

    #[test]
    fn store_round_trips_and_enforces_immutability() {
        let project_path = test_project_path("immutable");
        let store = PresetStore::for_project(&project_path);
        let record = biome_record();
        let preset = capture(&record, &biome_payload(42), &[]);

        store.save(&preset).expect("first save succeeds");
        assert!(store.save(&preset).is_err(), "same revision must not overwrite");

        let variant =
            new_seed_variant(&preset, std::slice::from_ref(&preset)).expect("variant succeeds");
        store.save(&variant).expect("new revision saves");

        let loaded = store.load_all();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].revision, 1);
        assert_eq!(loaded[1].revision, 2);
        assert_eq!(loaded[0].preset_id, loaded[1].preset_id);
        let _ = std::fs::remove_dir_all(project_path.parent().unwrap());
    }
}
