//! The read side of the Designer → Game publish manifest
//! (docs/PROJECT_PLAN.md, P1) — resolving which immutable generation under
//! `assets/published/` is current, and the payload shapes baked into it.
//!
//! Split out of `engine_tools::publish` so the Game edition (via
//! `world::published_content`) can resolve and read a publication without
//! depending on `engine_tools`, which keeps every writer concern (baking,
//! staging, atomically promoting a new generation) that only the Designer
//! edition ever runs. `PublishedGenerationManifest`/`read_generation_manifest`
//! and the three manifest constants below are genuinely shared: the writer
//! (`engine_tools::publish::prepare_generation` et al.) constructs and reads
//! them too, and imports them back from here rather than this module
//! depending on the writer.

use std::path::{Path, PathBuf};

use crate::combat::weapon_forge::WeaponSpec;
use crate::spaceship_forge::SpacecraftSpec;
use crate::vehicle_forge::VehicleSpec;
use crate::world::dialogue_schema::DialogueGraph;

pub(crate) const PUBLISHED_MANIFEST_SCHEMA: u32 = 1;
pub(crate) const PUBLISHED_MANIFEST_FILE: &str = "current.json";
pub(crate) const PUBLISHED_GENERATIONS_DIR: &str = "generations";

/// The directory the Game edition reads published content from.
pub fn published_dir() -> PathBuf {
    crate::engine::platform_paths::asset_root().join("published")
}

/// A single pointer is the publication commit point. Generation directories
/// are immutable and complete before this document is atomically replaced, so
/// the consumer can never observe a weapon file from one run and a creature
/// file from another.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub(crate) struct PublishedGenerationManifest {
    pub(crate) schema_version: u32,
    pub(crate) generation: String,
}

/// Resolve the commit pointer exactly once. Projects published before
/// generation manifests were introduced retain the legacy flat directory
/// until their next publish. Shared by the writer (deciding whether a staged
/// generation ever got committed) and the reader (finding what to load).
pub(crate) fn read_generation_manifest(
    out_dir: &Path,
) -> Result<Option<PublishedGenerationManifest>, String> {
    let path = out_dir.join(PUBLISHED_MANIFEST_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Could not read published manifest {}: {error}",
                path.display()
            ));
        }
    };
    let manifest: PublishedGenerationManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Published manifest {} is invalid: {error}", path.display()))?;
    if manifest.schema_version != PUBLISHED_MANIFEST_SCHEMA {
        return Err(format!(
            "Published manifest {} uses unsupported schema {}",
            path.display(),
            manifest.schema_version
        ));
    }
    if manifest.generation.len() != 16
        || !manifest
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "Published manifest {} contains an invalid generation id",
            path.display()
        ));
    }
    Ok(Some(manifest))
}

/// One immutable view of the publication pointer. A Game catalog load keeps
/// this snapshot for every category so a concurrent publish cannot mix files
/// from different generations.
#[derive(Debug, Clone)]
pub(crate) struct PublishedGenerationSnapshot {
    content_dir: PathBuf,
}

impl PublishedGenerationSnapshot {
    pub(crate) fn file(&self, file: &str) -> PathBuf {
        self.content_dir.join(file)
    }
}

pub(crate) fn published_generation_in(
    out_dir: &Path,
) -> Result<PublishedGenerationSnapshot, String> {
    let content_dir = match read_generation_manifest(out_dir)? {
        Some(manifest) => out_dir
            .join(PUBLISHED_GENERATIONS_DIR)
            .join(manifest.generation),
        None => out_dir.to_path_buf(),
    };
    Ok(PublishedGenerationSnapshot { content_dir })
}

/// Convenience resolver for callers that need only one file. Multi-file
/// consumers must retain [`PublishedGenerationSnapshot`] instead. Only
/// `engine_tools::publish`'s own tests use the single-file form today; real
/// callers (`world::published_content`) read every category and keep the
/// snapshot.
#[cfg(test)]
pub(crate) fn published_file_in(out_dir: &Path, file: &str) -> Result<PathBuf, String> {
    Ok(published_generation_in(out_dir)?.file(file))
}

/// Resolve one already-published immutable generation by identity. This is
/// the export-safe path: another Forge process may advance `current.json`
/// while an optimized native build is still running.
pub(crate) fn published_generation_dir(
    out_dir: &Path,
    generation: &str,
) -> Result<PathBuf, String> {
    if generation.len() != 16 || !generation.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("Invalid published generation id: {generation:?}"));
    }
    Ok(out_dir.join(PUBLISHED_GENERATIONS_DIR).join(generation))
}

/// One published weapon: the design plus the stable id the game resolves it
/// by (equip slots and save files reference this id).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PublishedWeapon {
    pub content_id: String,
    pub spec: WeaponSpec,
}

/// A published vehicle recipe paired with its stable project content id.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PublishedVehicle {
    pub content_id: String,
    pub spec: VehicleSpec,
}

/// A published spacecraft recipe paired with its stable project content id.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PublishedSpaceship {
    pub content_id: String,
    pub spec: SpacecraftSpec,
}

/// One published dialogue graph: the validated conversation plus the stable
/// id NPCs, chapter scripts, and gameplay signals reference it by.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PublishedDialogue {
    pub content_id: String,
    pub graph: DialogueGraph,
}
