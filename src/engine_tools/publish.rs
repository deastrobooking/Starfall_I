//! The Designer → Game publish step (docs/PROJECT_PLAN.md, P1).
//!
//! Authoring happens in versioned `ForgeProject` stores that consumers never
//! see. Publishing is the one-way bridge: validate every draft through the
//! store's own gate (`ForgeProject::publish_drafts`), then bake the published
//! weapons, creatures, vehicles, spacecraft, dialogue, and platformer routes
//! into plain JSON under an immutable generation in `assets/published/`. A
//! manifest committed last selects the complete
//! generation. The consumer Game edition loads those files and nothing else;
//! it has no writer for any of this.
//!
//! Baked output is deterministic (records sorted by content id) so publishing
//! twice without edits produces byte-identical files and clean VCS diffs.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use super::dialogue_records;
use super::persistence::{
    ContentCategory, ForgeProject, ProjectIoError, ProjectLoadSource, ProjectStore,
};
use super::platformer_route_records;
use super::project_registry::ForgeProjectRegistry;
use super::spaceship_records;
use super::vehicle_records;
use super::weapon_records;
use crate::combat::weapon_forge::WeaponSpec;
use crate::robots::creature::CreatureSpec;
use crate::spaceship_forge::SpacecraftSpec;
use crate::vehicle_forge::VehicleSpec;

const RECOVERY_LIMIT: usize = 3;
const PUBLISHED_MANIFEST_SCHEMA: u32 = 1;
const PUBLISHED_MANIFEST_FILE: &str = "current.json";
const PUBLISHED_GENERATIONS_DIR: &str = "generations";
const WEAPONS_FILE: &str = "weapons.json";
const CREATURES_FILE: &str = "creatures.json";
const VEHICLES_FILE: &str = "vehicles.json";
const SPACESHIPS_FILE: &str = "spaceships.json";
const DIALOGUES_FILE: &str = "dialogues.json";
const PLATFORMER_ROUTES_FILE: &str = "platformer_routes.json";
const PUBLISHED_OUTPUT_LOCK_FILE: &str = ".publish.lock";

/// Complete runtime payload of one published generation. Exporters use this
/// same list as the Game loaders so a bundle cannot silently omit a catalog.
pub(crate) const PUBLISHED_CONTENT_FILES: [&str; 6] = [
    WEAPONS_FILE,
    CREATURES_FILE,
    VEHICLES_FILE,
    SPACESHIPS_FILE,
    DIALOGUES_FILE,
    PLATFORMER_ROUTES_FILE,
];

static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);
static PUBLISHED_PROCESS_LOCK: Mutex<()> = Mutex::new(());

/// Publishing is shared by every Forge project, so a project-local writer
/// lock is not enough. This guard combines an in-process mutex (for consistent
/// same-process behavior on every OS) with an OS file lock (for other Forge
/// processes) rooted in the shared output directory.
struct PublishedOutputLock {
    _process: MutexGuard<'static, ()>,
    file: File,
}

impl Drop for PublishedOutputLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn acquire_published_output_lock(out_dir: &Path) -> Result<PublishedOutputLock, String> {
    std::fs::create_dir_all(out_dir).map_err(|error| {
        format!(
            "Could not create published output directory {}: {error}",
            out_dir.display()
        )
    })?;
    let process = PUBLISHED_PROCESS_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let lock_path = out_dir.join(PUBLISHED_OUTPUT_LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| {
            format!(
                "Could not open published output lock {}: {error}",
                lock_path.display()
            )
        })?;
    file.lock().map_err(|error| {
        format!(
            "Could not lock published output {}: {error}",
            out_dir.display()
        )
    })?;
    Ok(PublishedOutputLock {
        _process: process,
        file,
    })
}

/// A single pointer is the publication commit point. Generation directories
/// are immutable and complete before this document is atomically replaced, so
/// the consumer can never observe a weapon file from one run and a creature
/// file from another.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct PublishedGenerationManifest {
    schema_version: u32,
    generation: String,
}

struct BakedGeneration {
    generation: String,
    weapons_json: Vec<u8>,
    creatures_json: Vec<u8>,
    vehicles_json: Vec<u8>,
    spaceships_json: Vec<u8>,
    dialogues_json: Vec<u8>,
    platformer_routes_json: Vec<u8>,
    manifest_json: Vec<u8>,
    report: PublishReport,
}

struct StagedGeneration {
    generation: String,
    generation_dir: PathBuf,
    manifest_json: Vec<u8>,
    report: PublishReport,
    installed_new: bool,
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
    pub graph: dialogue_records::DialogueGraph,
}

/// What a publish run produced, for the hub status line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PublishReport {
    /// Stable content hash naming the immutable generation selected by this
    /// publish. Exporters must use this identity rather than rereading the
    /// mutable `current.json` pointer after a long native build.
    pub generation: String,
    pub weapons: usize,
    pub creatures: usize,
    pub vehicles: usize,
    pub spaceships: usize,
    pub dialogues: usize,
    pub platformer_routes: usize,
    /// Records in categories that have no game-side loader yet.
    pub skipped: usize,
    /// The generation is visible to readers, but the final directory sync
    /// failed, so crash durability could not be confirmed to the publisher.
    pub durability_warning: Option<String>,
}

impl PublishReport {
    pub fn summary(&self) -> String {
        let mut parts = vec![format!(
            "Published {} weapon(s), {} creature(s), {} vehicle(s), {} spacecraft, {} dialogue(s), {} platformer route(s)",
            self.weapons,
            self.creatures,
            self.vehicles,
            self.spaceships,
            self.dialogues,
            self.platformer_routes
        )];
        if self.skipped > 0 {
            parts.push(format!("{} record(s) have no loader yet", self.skipped));
        }
        if let Some(warning) = &self.durability_warning {
            parts.push(format!("DURABILITY NOT CONFIRMED: {warning}"));
        }
        parts.join(" • ")
    }
}

/// The directory the Game edition reads published content from.
pub fn published_dir() -> PathBuf {
    crate::engine::platform_paths::asset_root().join("published")
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

/// Collect every publishable vehicle, sorted by stable content id.
fn bake_vehicles(project: &ForgeProject) -> Result<Vec<PublishedVehicle>, String> {
    let mut vehicles = Vec::new();
    for (content_id, _) in vehicle_records::vehicle_entries(project) {
        let spec = vehicle_records::load_vehicle(project, &content_id)?;
        vehicles.push(PublishedVehicle { content_id, spec });
    }
    vehicles.sort_by(|a, b| a.content_id.cmp(&b.content_id));
    Ok(vehicles)
}

/// Collect every publishable spacecraft, sorted by stable content id.
fn bake_spaceships(project: &ForgeProject) -> Result<Vec<PublishedSpaceship>, String> {
    let mut spaceships = Vec::new();
    for (content_id, _) in spaceship_records::spaceship_entries(project) {
        let spec = spaceship_records::load_spaceship(project, &content_id)?;
        spaceships.push(PublishedSpaceship { content_id, spec });
    }
    spaceships.sort_by(|a, b| a.content_id.cmp(&b.content_id));
    Ok(spaceships)
}

/// Collect every publishable dialogue graph, sorted by stable content id.
/// Invalid graphs fail the whole bake before any output is staged; a broken
/// draft can never reach the consumer half-published.
fn bake_dialogues(project: &ForgeProject) -> Result<Vec<PublishedDialogue>, String> {
    let mut dialogues = Vec::new();
    for (content_id, _) in dialogue_records::dialogue_entries(project) {
        let graph = dialogue_records::load_dialogue(project, &content_id)
            .map_err(|error| format!("{content_id}: {error:?}"))?;
        dialogues.push(PublishedDialogue { content_id, graph });
    }
    dialogues.sort_by(|a, b| a.content_id.cmp(&b.content_id));
    Ok(dialogues)
}

/// Compile typed authoring graphs into the smaller route documents consumed by
/// Game builds. Structural graph errors fail publication before staging.
fn bake_platformer_routes(
    project: &ForgeProject,
) -> Result<Vec<starfall_platformer_graph::PlatformerRouteDocument>, String> {
    let mut routes = Vec::new();
    for (content_id, _) in platformer_route_records::platformer_route_entries(project) {
        routes.push(
            platformer_route_records::compile_record(project, &content_id)
                .map_err(|error| format!("{content_id}: {error:?}"))?,
        );
    }
    routes.sort_by(|left, right| left.id.cmp(&right.id));
    if let Some(duplicate) = routes
        .windows(2)
        .find(|pair| pair[0].id == pair[1].id)
        .map(|pair| pair[0].id.clone())
    {
        return Err(format!(
            "platformer route runtime id {duplicate} is authored more than once"
        ));
    }
    Ok(routes)
}

/// Records that exist in the project but have no baked representation yet.
fn unbaked_record_count(project: &ForgeProject) -> usize {
    let dialogue_ids = dialogue_records::dialogue_entries(project)
        .into_iter()
        .map(|(content_id, _)| content_id)
        .collect::<std::collections::BTreeSet<_>>();
    let platformer_route_ids = platformer_route_records::platformer_route_entries(project)
        .into_iter()
        .map(|(content_id, _)| content_id)
        .collect::<std::collections::BTreeSet<_>>();
    project
        .records
        .iter()
        .filter(|record| {
            !matches!(
                record.category,
                ContentCategory::Weapon
                    | ContentCategory::Creature
                    | ContentCategory::Vehicle
                    | ContentCategory::Spaceship
            ) && !dialogue_ids.contains(&record.content_id)
                && !platformer_route_ids.contains(&record.content_id)
        })
        .count()
}

fn generation_id(
    weapons_json: &[u8],
    creatures_json: &[u8],
    vehicles_json: &[u8],
    spaceships_json: &[u8],
    dialogues_json: &[u8],
    platformer_routes_json: &[u8],
) -> String {
    // Stable FNV-1a rather than DefaultHasher: the id must remain reproducible
    // across processes and Rust releases because it names an immutable output
    // generation checked into source control.
    let mut hash = 0xcbf29ce484222325_u64;
    for bytes in [
        PUBLISHED_MANIFEST_SCHEMA.to_le_bytes().as_slice(),
        WEAPONS_FILE.as_bytes(),
        weapons_json,
        CREATURES_FILE.as_bytes(),
        creatures_json,
        VEHICLES_FILE.as_bytes(),
        vehicles_json,
        SPACESHIPS_FILE.as_bytes(),
        spaceships_json,
        DIALOGUES_FILE.as_bytes(),
        dialogues_json,
        PLATFORMER_ROUTES_FILE.as_bytes(),
        platformer_routes_json,
    ] {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

/// Finish all fallible collection and serialization before touching the
/// published directory. Any malformed typed recipe therefore cannot disturb
/// the last committed generation.
fn prepare_generation(project: &ForgeProject) -> Result<BakedGeneration, String> {
    let weapons = bake_weapons(project)?;
    let creatures = bake_creatures(project)?;
    let vehicles = bake_vehicles(project)?;
    let spaceships = bake_spaceships(project)?;
    let dialogues = bake_dialogues(project)?;
    let platformer_routes = bake_platformer_routes(project)?;
    let weapons_json = serde_json::to_vec_pretty(&weapons).map_err(|e| e.to_string())?;
    let creatures_json = serde_json::to_vec_pretty(&creatures).map_err(|e| e.to_string())?;
    let vehicles_json = serde_json::to_vec_pretty(&vehicles).map_err(|e| e.to_string())?;
    let spaceships_json = serde_json::to_vec_pretty(&spaceships).map_err(|e| e.to_string())?;
    let dialogues_json = serde_json::to_vec_pretty(&dialogues).map_err(|e| e.to_string())?;
    let platformer_routes_json =
        serde_json::to_vec_pretty(&platformer_routes).map_err(|e| e.to_string())?;
    let generation = generation_id(
        &weapons_json,
        &creatures_json,
        &vehicles_json,
        &spaceships_json,
        &dialogues_json,
        &platformer_routes_json,
    );
    let manifest_json = serde_json::to_vec_pretty(&PublishedGenerationManifest {
        schema_version: PUBLISHED_MANIFEST_SCHEMA,
        generation: generation.clone(),
    })
    .map_err(|e| e.to_string())?;
    let report = PublishReport {
        generation: generation.clone(),
        weapons: weapons.len(),
        creatures: creatures.len(),
        vehicles: vehicles.len(),
        spaceships: spaceships.len(),
        dialogues: dialogues.len(),
        platformer_routes: platformer_routes.len(),
        skipped: unbaked_record_count(project),
        durability_warning: None,
    };
    Ok(BakedGeneration {
        generation,
        weapons_json,
        creatures_json,
        vehicles_json,
        spaceships_json,
        dialogues_json,
        platformer_routes_json,
        manifest_json,
        report,
    })
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::create(path)
        .map_err(|error| format!("Could not create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("Could not sync {}: {error}", path.display()))
}

/// Atomically replace the publication pointer without sharing the persistence
/// layer's fixed `<path>.tmp` name. The output lock serializes publishers, and
/// `create_new` plus a process/counter suffix also makes stale temp files and
/// unrelated writers harmless.
fn atomic_write_manifest(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Could not create published manifest directory {}: {error}",
            parent.display()
        )
    })?;
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let (temp, mut file) = loop {
        let id = NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed);
        let temp = parent.join(format!(
            ".{file_name}.publish-tmp-{}-{id}",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => break (temp, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Could not create published manifest temp {}: {error}",
                    temp.display()
                ));
            }
        }
    };

    if let Err(error) = file.write_all(bytes) {
        drop(file);
        let _ = std::fs::remove_file(&temp);
        return Err(format!(
            "Could not write published manifest temp {}: {error}",
            temp.display()
        ));
    }
    if let Err(error) = file.sync_all() {
        drop(file);
        let _ = std::fs::remove_file(&temp);
        return Err(format!(
            "Could not sync published manifest temp {}: {error}",
            temp.display()
        ));
    }
    drop(file);
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!(
            "Could not promote published manifest temp {}: {error}",
            temp.display()
        ));
    }
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            format!(
                "Published manifest is visible but directory {} could not be synced: {error}",
                parent.display()
            )
        })
}

fn generation_matches(path: &Path, baked: &BakedGeneration) -> Result<bool, String> {
    let weapons = std::fs::read(path.join(WEAPONS_FILE))
        .map_err(|error| format!("Could not verify {}: {error}", path.display()))?;
    let creatures = std::fs::read(path.join(CREATURES_FILE))
        .map_err(|error| format!("Could not verify {}: {error}", path.display()))?;
    let vehicles = std::fs::read(path.join(VEHICLES_FILE))
        .map_err(|error| format!("Could not verify {}: {error}", path.display()))?;
    let spaceships = std::fs::read(path.join(SPACESHIPS_FILE))
        .map_err(|error| format!("Could not verify {}: {error}", path.display()))?;
    let dialogues = std::fs::read(path.join(DIALOGUES_FILE))
        .map_err(|error| format!("Could not verify {}: {error}", path.display()))?;
    let platformer_routes = std::fs::read(path.join(PLATFORMER_ROUTES_FILE))
        .map_err(|error| format!("Could not verify {}: {error}", path.display()))?;
    Ok(weapons == baked.weapons_json
        && creatures == baked.creatures_json
        && vehicles == baked.vehicles_json
        && spaceships == baked.spaceships_json
        && dialogues == baked.dialogues_json
        && platformer_routes == baked.platformer_routes_json)
}

fn cleanup_staging(path: &Path) {
    if let Err(error) = std::fs::remove_dir_all(path) {
        // Cleanup is best effort and never replaces the original publication
        // error. A dot-prefixed, unreferenced staging directory is ignored by
        // every consumer and can be diagnosed manually.
        eprintln!(
            "Could not remove abandoned publish staging directory {}: {error}",
            path.display()
        );
    }
}

fn stage_generation(out_dir: &Path, baked: BakedGeneration) -> Result<StagedGeneration, String> {
    let generations_dir = out_dir.join(PUBLISHED_GENERATIONS_DIR);
    std::fs::create_dir_all(&generations_dir).map_err(|error| {
        format!(
            "Could not create published generations directory {}: {error}",
            generations_dir.display()
        )
    })?;
    let generation_dir = generations_dir.join(&baked.generation);

    if generation_dir.exists() {
        if !generation_matches(&generation_dir, &baked)? {
            return Err(format!(
                "Published generation {} already exists with different bytes",
                baked.generation
            ));
        }
        return Ok(StagedGeneration {
            generation: baked.generation,
            generation_dir,
            manifest_json: baked.manifest_json,
            report: baked.report,
            installed_new: false,
        });
    }

    let staging_id = NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed);
    let staging_dir = generations_dir.join(format!(
        ".staging-{}-{}-{staging_id}",
        baked.generation,
        std::process::id()
    ));
    std::fs::create_dir(&staging_dir).map_err(|error| {
        format!(
            "Could not create publish staging directory {}: {error}",
            staging_dir.display()
        )
    })?;

    let mut installed = false;
    let stage_result = (|| {
        write_synced(&staging_dir.join(WEAPONS_FILE), &baked.weapons_json)?;
        write_synced(&staging_dir.join(CREATURES_FILE), &baked.creatures_json)?;
        write_synced(&staging_dir.join(VEHICLES_FILE), &baked.vehicles_json)?;
        write_synced(&staging_dir.join(SPACESHIPS_FILE), &baked.spaceships_json)?;
        write_synced(&staging_dir.join(DIALOGUES_FILE), &baked.dialogues_json)?;
        write_synced(
            &staging_dir.join(PLATFORMER_ROUTES_FILE),
            &baked.platformer_routes_json,
        )?;
        File::open(&staging_dir)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| {
                format!(
                    "Could not sync publish staging directory {}: {error}",
                    staging_dir.display()
                )
            })?;
        std::fs::rename(&staging_dir, &generation_dir).map_err(|error| {
            format!(
                "Could not promote publish staging directory {}: {error}",
                staging_dir.display()
            )
        })?;
        installed = true;
        File::open(&generations_dir)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| {
                format!(
                    "Could not sync published generations directory {}: {error}",
                    generations_dir.display()
                )
            })
    })();
    if let Err(error) = stage_result {
        cleanup_staging(if installed {
            &generation_dir
        } else {
            &staging_dir
        });
        return Err(error);
    }

    Ok(StagedGeneration {
        generation: baked.generation,
        generation_dir,
        manifest_json: baked.manifest_json,
        report: baked.report,
        installed_new: true,
    })
}

impl StagedGeneration {
    fn discard_if_uncommitted(&self, out_dir: &Path) {
        if !self.installed_new {
            return;
        }
        let current_generation = read_generation_manifest(out_dir)
            .ok()
            .flatten()
            .map(|manifest| manifest.generation);
        if current_generation.as_deref() != Some(self.generation.as_str()) {
            cleanup_staging(&self.generation_dir);
        }
    }
}

fn promote_generation_with(
    out_dir: &Path,
    staged: &StagedGeneration,
    promote: impl FnOnce(&Path, &[u8]) -> Result<(), String>,
) -> Result<ManifestPromotion, String> {
    let manifest_path = out_dir.join(PUBLISHED_MANIFEST_FILE);
    match promote(&manifest_path, &staged.manifest_json) {
        Ok(()) => Ok(ManifestPromotion::DurablyCommitted),
        Err(error) => {
            // The atomic writer renames the complete manifest before syncing its
            // parent directory. A sync error therefore does not necessarily
            // mean promotion failed: readers may already see the new commit.
            // Inspect the commit point itself so callers never roll project
            // hashes back underneath a manifest that is already visible.
            if std::fs::read(&manifest_path).is_ok_and(|visible| visible == staged.manifest_json) {
                Ok(ManifestPromotion::CommittedDurabilityUnknown(error))
            } else {
                Err(error)
            }
        }
    }
}

enum ManifestPromotion {
    DurablyCommitted,
    CommittedDurabilityUnknown(String),
}

fn read_generation_manifest(out_dir: &Path) -> Result<Option<PublishedGenerationManifest>, String> {
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

/// Resolve the commit pointer exactly once. Projects published before
/// generation manifests were introduced retain the legacy flat directory
/// until their next publish.
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
/// consumers must retain [`PublishedGenerationSnapshot`] instead.
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

fn bake_project_to_with_promote(
    project: &ForgeProject,
    out_dir: &Path,
    promote: impl FnOnce(&Path, &[u8]) -> Result<(), String>,
) -> Result<PublishReport, String> {
    let baked = prepare_generation(project)?;
    // Keep every output mutation—from staging through promotion or cleanup—
    // inside the shared directory lock.
    let _output_lock = acquire_published_output_lock(out_dir)?;
    let staged = stage_generation(out_dir, baked)?;
    match promote_generation_with(out_dir, &staged, promote) {
        Ok(ManifestPromotion::DurablyCommitted) => {}
        Ok(ManifestPromotion::CommittedDurabilityUnknown(warning)) => {
            let mut report = staged.report;
            report.durability_warning = Some(warning);
            return Ok(report);
        }
        Err(error) => {
            staged.discard_if_uncommitted(out_dir);
            return Err(error);
        }
    }
    Ok(staged.report)
}

/// Bake a project's publishable content into an immutable generation under
/// `out_dir`, then atomically commit a manifest that exposes the complete
/// complete cross-category content set.
///
/// Separated from the store handling so tests can publish into a temp
/// directory without touching the repository's real `assets/published/`.
pub fn bake_project_to(project: &ForgeProject, out_dir: &Path) -> Result<PublishReport, String> {
    bake_project_to_with_promote(project, out_dir, atomic_write_manifest)
}

fn publish_store_to_with_promote(
    store: &ProjectStore,
    out_dir: &Path,
    promote: impl FnOnce(&Path, &[u8]) -> Result<(), String>,
) -> Result<PublishReport, String> {
    publish_store_to_with_hooks(store, out_dir, |_| Ok(()), promote)
}

fn publish_store_to_with_hooks(
    store: &ProjectStore,
    out_dir: &Path,
    before_hash_save: impl FnOnce(&ProjectStore) -> Result<(), String>,
    promote: impl FnOnce(&Path, &[u8]) -> Result<(), String>,
) -> Result<PublishReport, String> {
    let (original, source, observed_revision) = store
        .load_with_recovery_and_revision()
        .map_err(|e| format!("Could not load active project: {e}"))?;
    if source != ProjectLoadSource::Primary {
        return Err(format!(
            "Active project loaded from {source:?}; recover it explicitly before publishing"
        ));
    }
    let Some(observed_revision) = observed_revision else {
        return Err(
            "Active project has no readable primary manifest; recover it explicitly before publishing"
                .to_string(),
        );
    };
    let mut project = original.clone();

    // The store's gate: every draft must validate before anything is
    // promoted. A project with one broken record publishes nothing.
    project
        .publish_drafts()
        .map_err(|e| format!("Validation failed: {e}"))?;

    // Collection, serialization, and durable staging all complete before the
    // first published_hash claim reaches disk.
    let baked = prepare_generation(&project)?;
    // Lock ordering is always shared output first, then the short project
    // transaction below. No path acquires the output lock from a project-store
    // callback, avoiding lock inversion.
    let _output_lock = acquire_published_output_lock(out_dir)?;
    let staged = stage_generation(out_dir, baked)?;

    if let Err(error) = before_hash_save(store) {
        staged.discard_if_uncommitted(out_dir);
        return Err(error);
    }

    // Keep the store's writer lock across the published-hash save and final
    // manifest promotion. A hard promotion failure rolls the hashes back
    // before another author can enter; a revision conflict writes nothing.
    let promotion = match store.compare_and_save_with_rollback(
        &mut project,
        &original,
        &observed_revision,
        || promote_generation_with(out_dir, &staged, promote).map_err(ProjectIoError::Io),
    ) {
        Ok((promotion, _revision)) => promotion,
        Err(error) => {
            staged.discard_if_uncommitted(out_dir);
            return Err(format!("Could not commit publish transaction: {error}"));
        }
    };

    match promotion {
        ManifestPromotion::DurablyCommitted => {}
        ManifestPromotion::CommittedDurabilityUnknown(warning) => {
            // Visibility is the publication commit point. The saved hashes
            // now agree with what readers can open, so rolling them back
            // would create a split-brain state. Surface the weaker durability
            // guarantee while preserving that agreement.
            let mut report = staged.report;
            report.durability_warning = Some(warning);
            return Ok(report);
        }
    }

    Ok(staged.report)
}

pub(crate) fn publish_store_to(
    store: &ProjectStore,
    out_dir: &Path,
) -> Result<PublishReport, String> {
    publish_store_to_with_promote(store, out_dir, atomic_write_manifest)
}

/// Publish the active project: validate drafts, completely stage the baked
/// generation, persist matching published hashes, and commit the generation
/// pointer last.
pub fn publish_active_project(registry: &ForgeProjectRegistry) -> Result<PublishReport, String> {
    let Some(active) = registry.active.as_deref() else {
        return Err("No active project — open one in the Project Hub first".to_string());
    };
    let store = ProjectStore::new(active, RECOVERY_LIMIT);
    publish_store_to(&store, &published_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::weapon_forge::{EmitterStyle, GripStyle};
    use crate::engine_tools::persistence::ContentPayload;
    use crate::spaceship_forge::{SpacecraftClass, SpacecraftSpec};
    use crate::vehicle_forge::{VehicleClass, VehicleSpec};
    use std::sync::{mpsc, Arc, Barrier};
    use std::thread;
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "starfall_publish_{label}_{}_{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("publish test directory should be creatable");
        dir
    }

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

    fn complete_project(label: &str) -> ForgeProject {
        let id_label = label
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let mut project = project_with_weapons(&[&format!("{label} Blade")]);
        let creature = CreatureSpec {
            content_id: format!("starfall.creature.{id_label}"),
            display_name: format!("{label} Creature"),
            ..Default::default()
        };
        super::super::creature_records::upsert_creature(&mut project, &creature)
            .expect("test creature should save");
        let mut vehicle = VehicleSpec::preset(VehicleClass::Car);
        vehicle.display_name = format!("{label} Vehicle");
        vehicle.content_id = format!("starfall.vehicle.{id_label}");
        vehicle_records::upsert_vehicle(&mut project, &vehicle).expect("test vehicle should save");
        let mut spaceship = SpacecraftSpec::preset(SpacecraftClass::Fighter);
        spaceship.display_name = format!("{label} Spaceship");
        spaceship.content_id = format!("starfall.spaceship.{id_label}");
        spaceship_records::upsert_spaceship(&mut project, &spaceship)
            .expect("test spaceship should save");
        project
    }

    fn assert_selected_project(out_dir: &Path, label: &str) {
        let manifest_bytes = std::fs::read(out_dir.join(PUBLISHED_MANIFEST_FILE)).unwrap();
        let manifest: PublishedGenerationManifest =
            serde_json::from_slice(&manifest_bytes).expect("current.json must remain valid JSON");
        let generation_dir = out_dir
            .join(PUBLISHED_GENERATIONS_DIR)
            .join(&manifest.generation);
        let weapons: Vec<PublishedWeapon> =
            serde_json::from_slice(&std::fs::read(generation_dir.join(WEAPONS_FILE)).unwrap())
                .unwrap();
        let creatures: Vec<CreatureSpec> =
            serde_json::from_slice(&std::fs::read(generation_dir.join(CREATURES_FILE)).unwrap())
                .unwrap();
        let vehicles: Vec<PublishedVehicle> =
            serde_json::from_slice(&std::fs::read(generation_dir.join(VEHICLES_FILE)).unwrap())
                .unwrap();
        let spaceships: Vec<PublishedSpaceship> =
            serde_json::from_slice(&std::fs::read(generation_dir.join(SPACESHIPS_FILE)).unwrap())
                .unwrap();
        assert_eq!(weapons.len(), 1);
        assert_eq!(
            weapons[0].content_id,
            format!("starfall.weapon.{label}_blade")
        );
        assert_eq!(creatures.len(), 1);
        assert_eq!(
            creatures[0].content_id,
            format!("starfall.creature.{label}")
        );
        assert_eq!(vehicles.len(), 1);
        assert_eq!(vehicles[0].content_id, format!("starfall.vehicle.{label}"));
        assert_eq!(spaceships.len(), 1);
        assert_eq!(
            spaceships[0].content_id,
            format!("starfall.spaceship.{label}")
        );
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
        let dir = test_dir("round_trip");

        let project = project_with_weapons(&["Round Trip"]);
        let report = bake_project_to(&project, &dir).expect("bake succeeds");
        assert_eq!(report.weapons, 1);
        assert_eq!(report.creatures, 0);

        // The game-side reader must get back exactly what was authored.
        let weapons_path = published_file_in(&dir, WEAPONS_FILE).unwrap();
        assert!(weapons_path.starts_with(dir.join(PUBLISHED_GENERATIONS_DIR)));
        let text = std::fs::read_to_string(weapons_path).unwrap();
        let loaded: Vec<PublishedWeapon> = serde_json::from_str(&text).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content_id, "starfall.weapon.round_trip");
        assert_eq!(loaded[0].spec.grip, GripStyle::Extended);
        // And an empty creatures file is still a valid file, not an error.
        let creatures: Vec<CreatureSpec> = serde_json::from_str(
            &std::fs::read_to_string(published_file_in(&dir, CREATURES_FILE).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(creatures.is_empty());
        let vehicles: Vec<PublishedVehicle> = serde_json::from_str(
            &std::fs::read_to_string(published_file_in(&dir, VEHICLES_FILE).unwrap()).unwrap(),
        )
        .unwrap();
        let spaceships: Vec<PublishedSpaceship> = serde_json::from_str(
            &std::fs::read_to_string(published_file_in(&dir, SPACESHIPS_FILE).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(vehicles.is_empty());
        assert!(spaceships.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dialogue_records_bake_into_the_generation() {
        let dir = test_dir("dialogues");
        let mut project = ForgeProject::default();
        // A default project may seed non-loader records of its own; measure
        // the delta rather than assuming a clean slate.
        let baseline_skipped = unbaked_record_count(&project);
        let content_id = dialogue_records::create_dialogue(&mut project, "Opening Banter")
            .expect("dialogue record should save");
        // The dialogue now has a loader, so it must not count as skipped.
        assert_eq!(unbaked_record_count(&project), baseline_skipped);

        let report = bake_project_to(&project, &dir).expect("bake succeeds");
        assert_eq!(report.dialogues, 1);
        assert_eq!(report.skipped, baseline_skipped);
        assert!(report.summary().contains("1 dialogue(s)"));

        // The game-side reader gets back exactly the authored graph.
        let dialogues_path = published_file_in(&dir, DIALOGUES_FILE).unwrap();
        assert!(dialogues_path.starts_with(dir.join(PUBLISHED_GENERATIONS_DIR)));
        let dialogues: Vec<PublishedDialogue> =
            serde_json::from_str(&std::fs::read_to_string(dialogues_path).unwrap()).unwrap();
        assert_eq!(dialogues.len(), 1);
        assert_eq!(dialogues[0].content_id, content_id);
        assert_eq!(dialogues[0].graph.entry_node, "opening");
        assert_eq!(dialogues[0].graph.nodes.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn platformer_route_graphs_compile_into_the_atomic_generation() {
        let dir = test_dir("platformer_routes");
        let mut project = ForgeProject::default();
        let baseline_skipped = unbaked_record_count(&project);
        let _content_id = platformer_route_records::create_platformer_route(
            &mut project,
            "Published Rooftops",
            starfall_graph::StableId::new("route_city_rooftops").unwrap(),
            starfall_graph::StableId::new("heavy_water.theme.cityscape").unwrap(),
            [
                starfall_graph::StableId::new("city_rooftop_arrival").unwrap(),
                starfall_graph::StableId::new("city_plaza_arena").unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(unbaked_record_count(&project), baseline_skipped);

        let report = bake_project_to(&project, &dir).expect("route bake succeeds");
        assert_eq!(report.platformer_routes, 1);
        assert!(report.summary().contains("1 platformer route(s)"));
        let path = published_file_in(&dir, PLATFORMER_ROUTES_FILE).unwrap();
        let routes: Vec<starfall_platformer_graph::PlatformerRouteDocument> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].id.as_str(), "route_city_rooftops");
        assert_eq!(routes[0].chunks.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_platformer_runtime_ids_refuse_publication() {
        let mut project = ForgeProject::default();
        for display_name in ["First Route Record", "Second Route Record"] {
            platformer_route_records::create_platformer_route(
                &mut project,
                display_name,
                starfall_graph::StableId::new("route_city_rooftops").unwrap(),
                starfall_graph::StableId::new("heavy_water.theme.cityscape").unwrap(),
                [
                    starfall_graph::StableId::new("city_rooftop_arrival").unwrap(),
                    starfall_graph::StableId::new("city_plaza_arena").unwrap(),
                ],
            )
            .unwrap();
        }

        let error = bake_platformer_routes(&project).unwrap_err();
        assert!(error.contains("route_city_rooftops"));
        assert!(error.contains("authored more than once"));
    }

    #[test]
    fn an_invalid_dialogue_fails_the_whole_bake() {
        let dir = test_dir("invalid_dialogue");
        let mut project = ForgeProject::default();
        let content_id = dialogue_records::create_dialogue(&mut project, "Broken Banter")
            .expect("dialogue record should save");
        // Corrupt the graph in place: a choice targeting a missing node must
        // refuse publication rather than ship a broken conversation.
        let mut graph = dialogue_records::load_dialogue(&project, &content_id)
            .expect("stored dialogue should load");
        graph.nodes[0]
            .choices
            .push(dialogue_records::DialogueChoice {
                label: "Leave".into(),
                target_node: "missing_node".into(),
                required_flag: None,
                set_flag: None,
            });
        let save_result = dialogue_records::save_dialogue(&mut project, &content_id, &graph);
        assert!(
            save_result.is_err(),
            "validation must reject the corrupt graph at save time"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_projects_publish_one_complete_generation_and_valid_manifest() {
        let root = test_dir("concurrent_projects");
        let published = root.join("published");
        std::fs::create_dir_all(&published).unwrap();
        // A stale fixed temp from the old writer must neither block nor be
        // overwritten by the collision-free manifest writer.
        let old_fixed_temp = published.join("current.json.tmp");
        std::fs::write(&old_fixed_temp, b"pre-existing sentinel").unwrap();

        let store_alpha = ProjectStore::new(root.join("alpha/project.json"), 1);
        let store_beta = ProjectStore::new(root.join("beta/project.json"), 1);
        let mut alpha = complete_project("alpha");
        let mut beta = complete_project("beta");
        store_alpha.save(&mut alpha).unwrap();
        store_beta.save(&mut beta).unwrap();
        store_alpha.load().expect("alpha fixture must reload");
        store_beta.load().expect("beta fixture must reload");

        let start = Arc::new(Barrier::new(3));
        let alpha_start = Arc::clone(&start);
        let alpha_out = published.clone();
        let alpha_thread = thread::spawn(move || {
            alpha_start.wait();
            publish_store_to(&store_alpha, &alpha_out)
        });
        let beta_start = Arc::clone(&start);
        let beta_out = published.clone();
        let beta_thread = thread::spawn(move || {
            beta_start.wait();
            publish_store_to(&store_beta, &beta_out)
        });
        start.wait();

        let alpha_report = alpha_thread.join().unwrap().unwrap();
        let beta_report = beta_thread.join().unwrap().unwrap();
        for report in [&alpha_report, &beta_report] {
            assert!(published_generation_dir(&published, &report.generation)
                .unwrap()
                .is_dir());
        }

        let selected = read_generation_manifest(&published)
            .unwrap()
            .expect("one complete generation must be selected");
        let selected_dir = published
            .join(PUBLISHED_GENERATIONS_DIR)
            .join(&selected.generation);
        let selected_weapons: Vec<PublishedWeapon> =
            serde_json::from_slice(&std::fs::read(selected_dir.join(WEAPONS_FILE)).unwrap())
                .unwrap();
        let label = if selected_weapons[0].content_id.contains("alpha") {
            "alpha"
        } else {
            "beta"
        };
        assert_selected_project(&published, label);
        assert_eq!(
            std::fs::read(&old_fixed_temp).unwrap(),
            b"pre-existing sentinel"
        );
        assert!(std::fs::read_dir(&published).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".publish-tmp-")
        }));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_publish_cleans_its_generation_before_next_project_enters_output() {
        let root = test_dir("cleanup_serialization");
        let published = root.join("published");
        let store_failed = ProjectStore::new(root.join("failed/project.json"), 1);
        let store_winner = ProjectStore::new(root.join("winner/project.json"), 1);
        let mut failed = complete_project("failed");
        let mut winner = complete_project("winner");
        store_failed.save(&mut failed).unwrap();
        store_winner.save(&mut winner).unwrap();

        let (at_promotion_tx, at_promotion_rx) = mpsc::channel();
        let (release_failure_tx, release_failure_rx) = mpsc::channel();
        let failed_out = published.clone();
        let failed_thread = thread::spawn(move || {
            publish_store_to_with_promote(&store_failed, &failed_out, |_, _| {
                at_promotion_tx.send(()).unwrap();
                release_failure_rx.recv().unwrap();
                Err("simulated hard promotion failure".into())
            })
        });
        at_promotion_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("first publisher should reach manifest promotion");

        let (winner_tx, winner_rx) = mpsc::channel();
        let winner_out = published.clone();
        let winner_thread = thread::spawn(move || {
            let result = publish_store_to(&store_winner, &winner_out);
            winner_tx.send(result).unwrap();
        });
        assert!(
            winner_rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "second project must not enter the shared output while cleanup is pending"
        );

        release_failure_tx.send(()).unwrap();
        let failed_error = failed_thread.join().unwrap().unwrap_err();
        assert!(failed_error.contains("simulated hard promotion failure"));
        winner_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("winner should proceed after cleanup")
            .unwrap();
        winner_thread.join().unwrap();

        assert_selected_project(&published, "winner");
        let generations = std::fs::read_dir(published.join(PUBLISHED_GENERATIONS_DIR))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            generations.len(),
            1,
            "failed generation must be removed before the winner stages"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_bake_leaves_the_committed_generation_and_hash_claims_intact() {
        let root = test_dir("failed_bake");
        let store = ProjectStore::new(root.join("project/project.json"), 1);
        let published = root.join("published");

        let mut project = project_with_weapons(&["Last Good"]);
        store.save(&mut project).unwrap();
        publish_store_to(&store, &published).expect("initial publish succeeds");
        let manifest_before = std::fs::read(published.join(PUBLISHED_MANIFEST_FILE)).unwrap();
        let output_before = std::fs::read(published_file_in(&published, WEAPONS_FILE).unwrap())
            .expect("committed weapon generation exists");

        let mut malformed = store.load().unwrap();
        let malformed_id = weapon_records::upsert_weapon(
            &mut malformed,
            &WeaponSpec {
                name: "Malformed".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let Some(ContentPayload::Weapon(recipe)) = malformed.payloads.get_mut(&malformed_id) else {
            panic!("new weapon should own a weapon payload");
        };
        recipe.fields.remove("weapon_spec");
        store
            .save(&mut malformed)
            .expect("generic project validation permits the malformed typed payload");
        assert!(malformed
            .records
            .iter()
            .find(|record| record.content_id == malformed_id)
            .unwrap()
            .published_hash
            .is_none());

        let error = publish_store_to(&store, &published).unwrap_err();
        assert!(error.contains("missing weapon_spec"));
        assert_eq!(
            std::fs::read(published.join(PUBLISHED_MANIFEST_FILE)).unwrap(),
            manifest_before
        );
        assert_eq!(
            std::fs::read(published_file_in(&published, WEAPONS_FILE).unwrap()).unwrap(),
            output_before
        );
        let reloaded = store.load().unwrap();
        assert!(reloaded
            .records
            .iter()
            .find(|record| record.content_id == malformed_id)
            .unwrap()
            .published_hash
            .is_none());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recovery_snapshot_must_be_explicitly_restored_before_publish() {
        let root = test_dir("recovery_refusal");
        let store = ProjectStore::new(root.join("project/project.json"), 1);
        let published = root.join("published");

        let mut project = project_with_weapons(&["Recovery Candidate"]);
        store.save(&mut project).unwrap();
        project.display_name = "Creates Recovery Snapshot".into();
        store.save(&mut project).unwrap();
        std::fs::write(store.path(), b"not a project manifest").unwrap();

        let error = publish_store_to(&store, &published).unwrap_err();
        assert!(error.contains("loaded from Recovery(1)"), "{error}");
        assert!(error.contains("recover it explicitly"), "{error}");
        assert!(!published.join(PUBLISHED_MANIFEST_FILE).exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_manifest_promotion_rolls_back_claims_and_keeps_prior_output() {
        let root = test_dir("failed_promote");
        let store = ProjectStore::new(root.join("project/project.json"), 1);
        let published = root.join("published");

        let mut project = project_with_weapons(&["First Generation"]);
        store.save(&mut project).unwrap();
        publish_store_to(&store, &published).expect("initial publish succeeds");
        let manifest_before = std::fs::read(published.join(PUBLISHED_MANIFEST_FILE)).unwrap();
        let output_before = std::fs::read(published_file_in(&published, WEAPONS_FILE).unwrap())
            .expect("committed weapon generation exists");

        let mut edited = store.load().unwrap();
        let next_id = weapon_records::upsert_weapon(
            &mut edited,
            &WeaponSpec {
                name: "Second Generation".into(),
                ..Default::default()
            },
        )
        .unwrap();
        store.save(&mut edited).unwrap();
        let claims_before = edited
            .records
            .iter()
            .map(|record| (record.content_id.clone(), record.published_hash.clone()))
            .collect::<Vec<_>>();

        let error = publish_store_to_with_promote(&store, &published, |_, _| {
            Err("simulated manifest promotion failure".into())
        })
        .unwrap_err();
        assert!(error.contains("simulated manifest promotion failure"));
        assert_eq!(
            std::fs::read(published.join(PUBLISHED_MANIFEST_FILE)).unwrap(),
            manifest_before
        );
        assert_eq!(
            std::fs::read(published_file_in(&published, WEAPONS_FILE).unwrap()).unwrap(),
            output_before
        );
        let claims_after = store
            .load()
            .unwrap()
            .records
            .iter()
            .map(|record| (record.content_id.clone(), record.published_hash.clone()))
            .collect::<Vec<_>>();
        assert_eq!(claims_after, claims_before);
        assert!(claims_after
            .iter()
            .find(|(content_id, _)| content_id == &next_id)
            .unwrap()
            .1
            .is_none());

        // Only the previously committed immutable generation remains. The
        // failed candidate is not retained as an unreferenced directory.
        assert_eq!(
            std::fs::read_dir(published.join(PUBLISHED_GENERATIONS_DIR))
                .unwrap()
                .count(),
            1
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_project_writer_wins_over_publish_hash_cas() {
        let root = test_dir("concurrent_hash_writer");
        let store = ProjectStore::new(root.join("project/project.json"), 1);
        let published = root.join("published");

        let mut project = project_with_weapons(&["Publish Candidate"]);
        store.save(&mut project).unwrap();

        let error = publish_store_to_with_hooks(
            &store,
            &published,
            |store| {
                let mut concurrent = store.load().map_err(|error| error.to_string())?;
                weapon_records::upsert_weapon(
                    &mut concurrent,
                    &WeaponSpec {
                        name: "Concurrent Author".into(),
                        ..Default::default()
                    },
                )?;
                store
                    .save(&mut concurrent)
                    .map_err(|error| error.to_string())
            },
            atomic_write_manifest,
        )
        .unwrap_err();

        assert!(error.contains("revision conflict"), "{error}");
        let current = store.load().unwrap();
        assert!(current
            .records
            .iter()
            .any(|record| record.content_id == "starfall.weapon.concurrent_author"));
        assert!(current
            .records
            .iter()
            .all(|record| record.published_hash.is_none()));
        assert!(!published.join(PUBLISHED_MANIFEST_FILE).exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn visible_manifest_after_promotion_error_keeps_matching_hash_claims() {
        let root = test_dir("post_commit_sync_error");
        let store = ProjectStore::new(root.join("project/project.json"), 1);
        let published = root.join("published");

        let mut project = project_with_weapons(&["First Generation"]);
        store.save(&mut project).unwrap();
        publish_store_to(&store, &published).expect("initial publish succeeds");
        let first_manifest = std::fs::read(published.join(PUBLISHED_MANIFEST_FILE)).unwrap();

        let mut edited = store.load().unwrap();
        let next_id = weapon_records::upsert_weapon(
            &mut edited,
            &WeaponSpec {
                name: "Second Generation".into(),
                ..Default::default()
            },
        )
        .unwrap();
        store.save(&mut edited).unwrap();

        let manifest_path = published.join(PUBLISHED_MANIFEST_FILE);
        let report = publish_store_to_with_promote(&store, &published, |path, bytes| {
            // Model the atomic writer's only ambiguous failure: rename made the
            // new manifest visible, but syncing the parent directory failed.
            std::fs::write(path, bytes).unwrap();
            Err("simulated directory sync failure after rename".into())
        })
        .expect("visible commit is success with a durability warning");

        assert!(report
            .durability_warning
            .as_deref()
            .unwrap()
            .contains("directory sync failure"));
        assert_ne!(std::fs::read(&manifest_path).unwrap(), first_manifest);
        let visible: Vec<PublishedWeapon> = serde_json::from_slice(
            &std::fs::read(published_file_in(&published, WEAPONS_FILE).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(visible.iter().any(|weapon| weapon.content_id == next_id));

        let claims = store.load().unwrap();
        assert!(claims
            .records
            .iter()
            .find(|record| record.content_id == next_id)
            .unwrap()
            .published_hash
            .is_some());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn consumer_path_falls_back_to_legacy_flat_files_without_a_manifest() {
        let dir = test_dir("legacy_flat");
        assert_eq!(
            published_file_in(&dir, WEAPONS_FILE).unwrap(),
            dir.join(WEAPONS_FILE)
        );
        let _ = std::fs::remove_dir_all(dir);
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
            generation: "0123456789abcdef".into(),
            weapons: 1,
            creatures: 0,
            vehicles: 0,
            spaceships: 0,
            dialogues: 0,
            platformer_routes: 0,
            skipped: 1,
            durability_warning: None,
        };
        assert!(report.summary().contains("no loader yet"));
    }
}
