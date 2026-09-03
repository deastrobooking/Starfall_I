//! Designer → standalone Game bundle export.
//!
//! Publishing selects one immutable content generation. Exporting then builds
//! the Game-only executable and assembles it with shipped assets plus exactly
//! that generation. The final directory appears through one rename, so a
//! failed copy never looks like a finished game.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::persistence::ProjectStore;
use super::project_registry::ForgeProjectRegistry;
use super::publish::{
    publish_active_project, published_dir, published_generation_dir, PUBLISHED_CONTENT_FILES,
};

const PROJECT_RECOVERY_LIMIT: usize = 3;
pub const GAME_BUNDLE_MANIFEST_FILE: &str = "starfall.bundle.json";
pub const GAME_BUNDLE_SCHEMA_VERSION: u32 = 1;

/// Machine-readable contract beside every exported executable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameBundleManifest {
    pub schema_version: u32,
    pub project_id: String,
    pub display_name: String,
    pub engine_version: String,
    pub target_os: String,
    pub target_arch: String,
    pub executable: String,
    pub asset_root: String,
    pub published_generation: Option<String>,
    pub published_files: Vec<String>,
}

/// Inputs to the filesystem-only bundle assembler. Keeping Cargo out of this
/// layer makes the package contract fast and deterministic to test.
#[derive(Debug, Clone)]
pub struct GameBundleRequest {
    pub project_id: String,
    pub display_name: String,
    pub bundle_id: String,
    pub executable_source: PathBuf,
    pub asset_source: PathBuf,
    pub published_source: PathBuf,
    pub published_generation: Option<String>,
    pub output_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExportReport {
    pub bundle_dir: PathBuf,
    pub executable: PathBuf,
    pub copied_files: usize,
    pub published_generation: Option<String>,
}

impl GameExportReport {
    pub fn summary(&self) -> String {
        let generation = self.published_generation.as_deref().unwrap_or("legacy");
        format!(
            "Exported {} file(s), content {generation} → {}",
            self.copied_files,
            self.bundle_dir.display()
        )
    }
}

/// Build the Game edition and export the active Forge project. Intended for a
/// background worker because a release Cargo build and asset copy may take
/// several minutes.
pub fn build_and_export_active_project(
    registry: &ForgeProjectRegistry,
) -> Result<GameExportReport, String> {
    let active = registry
        .active_entry()
        .ok_or_else(|| "No active project — open one in the Project Hub first".to_string())?;
    let store = ProjectStore::new(&active.path, PROJECT_RECOVERY_LIMIT);
    let (project, _) = store
        .load_with_recovery()
        .map_err(|error| format!("Could not load active project: {error:?}"))?;
    let project_dir = active.path.parent().unwrap_or_else(|| Path::new("."));
    let export_build_root = project_dir
        .join("build")
        .join("cache")
        .join("game-export-target");

    let publication = publish_active_project(registry)?;
    let executable_source = build_game_executable(&export_build_root)?;

    let published_root = published_dir();
    let published_source = published_generation_dir(&published_root, &publication.generation)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let project_slug = path_segment(&project.project_id)?;
    let bundle_id = format!(
        "{project_slug}-{}-{}-{timestamp}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    assemble_game_bundle(&GameBundleRequest {
        project_id: project.project_id,
        display_name: project.display_name,
        bundle_id,
        executable_source,
        asset_source: crate::engine::platform_paths::asset_root(),
        published_source,
        published_generation: Some(publication.generation),
        output_root: project_dir.join("build").join("exports"),
    })
}

fn build_game_executable(target_dir: &Path) -> Result<PathBuf, String> {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--release",
            "--locked",
            "--no-default-features",
            "--features",
            "heavy-water-demo",
            "--bin",
            "starfall-i",
        ])
        .arg("--target-dir")
        .arg(target_dir)
        .output()
        .map_err(|error| format!("Could not start Cargo: {error}"))?;
    if output.status.success() {
        return Ok(release_executable_path(target_dir));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail = stderr
        .char_indices()
        .rev()
        .nth(3_999)
        .map_or(stderr.as_ref(), |(index, _)| &stderr[index..]);
    Err(format!("Game build failed:\n{tail}"))
}

fn release_executable_path(target_dir: &Path) -> PathBuf {
    target_dir
        .join("release")
        .join(format!("starfall-i{}", std::env::consts::EXE_SUFFIX))
}

/// Assemble one immutable, self-contained native bundle. Existing exports are
/// never overwritten; the caller supplies a unique bundle id.
pub fn assemble_game_bundle(request: &GameBundleRequest) -> Result<GameExportReport, String> {
    let bundle_id = path_segment(&request.bundle_id)?;
    require_file(&request.executable_source, "Game executable")?;
    require_directory(&request.asset_source, "Asset root")?;
    require_directory(&request.published_source, "Published generation")?;

    let output_root = &request.output_root;
    let output_existed = output_root.exists();
    fs::create_dir_all(output_root).map_err(|error| {
        format!(
            "Could not create export directory {}: {error}",
            output_root.display()
        )
    })?;
    let asset_source = fs::canonicalize(&request.asset_source).map_err(|error| {
        format!(
            "Could not resolve asset root {}: {error}",
            request.asset_source.display()
        )
    })?;
    let resolved_output = fs::canonicalize(output_root).map_err(|error| {
        format!(
            "Could not resolve export root {}: {error}",
            output_root.display()
        )
    })?;
    if resolved_output.starts_with(&asset_source) {
        if !output_existed {
            let _ = fs::remove_dir(output_root);
        }
        return Err(format!(
            "Export root {} cannot be inside asset root {}",
            output_root.display(),
            request.asset_source.display()
        ));
    }
    let final_dir = output_root.join(bundle_id);
    if final_dir.exists() {
        return Err(format!(
            "Export already exists at {}; export again to create a new version",
            final_dir.display()
        ));
    }
    let staging = unique_staging_path(output_root, bundle_id);
    fs::create_dir(&staging).map_err(|error| {
        format!(
            "Could not create export staging directory {}: {error}",
            staging.display()
        )
    })?;

    let result = assemble_staged_bundle(request, &staging).and_then(|mut report| {
        fs::rename(&staging, &final_dir).map_err(|error| {
            format!(
                "Could not commit exported game {}: {error}",
                final_dir.display()
            )
        })?;
        report.bundle_dir = final_dir.clone();
        report.executable = final_dir.join(
            report
                .executable
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("starfall-i")),
        );
        Ok(report)
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn assemble_staged_bundle(
    request: &GameBundleRequest,
    staging: &Path,
) -> Result<GameExportReport, String> {
    let executable_name = format!(
        "{}{}",
        path_segment(&request.project_id)?,
        std::env::consts::EXE_SUFFIX
    );
    let executable = staging.join(&executable_name);
    fs::copy(&request.executable_source, &executable).map_err(|error| {
        format!(
            "Could not copy executable {}: {error}",
            request.executable_source.display()
        )
    })?;

    let asset_target = staging.join("assets");
    fs::create_dir(&asset_target)
        .map_err(|error| format!("Could not create bundle asset root: {error}"))?;
    let mut copied_files = copy_asset_tree(&request.asset_source, &asset_target)?;

    let published_target = asset_target.join("published");
    let generation_target = match request.published_generation.as_deref() {
        Some(generation) => {
            let generation = generation_segment(generation)?;
            published_target.join("generations").join(generation)
        }
        None => published_target.clone(),
    };
    fs::create_dir_all(&generation_target)
        .map_err(|error| format!("Could not create published bundle directory: {error}"))?;

    let mut published_files = Vec::with_capacity(PUBLISHED_CONTENT_FILES.len());
    for file in PUBLISHED_CONTENT_FILES {
        let source = request.published_source.join(file);
        require_file(&source, "Published catalog")?;
        fs::copy(&source, generation_target.join(file))
            .map_err(|error| format!("Could not copy {}: {error}", source.display()))?;
        published_files.push(file.to_string());
        copied_files += 1;
    }
    if let Some(generation) = request.published_generation.as_deref() {
        let pointer = serde_json::json!({
            "schema_version": 1,
            "generation": generation,
        });
        let bytes = serde_json::to_vec_pretty(&pointer)
            .map_err(|error| format!("Could not encode published pointer: {error}"))?;
        fs::write(published_target.join("current.json"), bytes)
            .map_err(|error| format!("Could not write published pointer: {error}"))?;
        copied_files += 1;
    }

    let manifest = GameBundleManifest {
        schema_version: GAME_BUNDLE_SCHEMA_VERSION,
        project_id: request.project_id.clone(),
        display_name: request.display_name.clone(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        executable: executable_name,
        asset_root: "assets".to_string(),
        published_generation: request.published_generation.clone(),
        published_files,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("Could not encode game bundle manifest: {error}"))?;
    fs::write(staging.join(GAME_BUNDLE_MANIFEST_FILE), manifest_bytes)
        .map_err(|error| format!("Could not write game bundle manifest: {error}"))?;
    copied_files += 2; // executable and bundle manifest

    Ok(GameExportReport {
        bundle_dir: staging.to_path_buf(),
        executable,
        copied_files,
        published_generation: request.published_generation.clone(),
    })
}

/// Copy shipped assets while deliberately excluding editor publication state;
/// the selected immutable generation is installed separately below.
fn copy_asset_tree(source: &Path, target: &Path) -> Result<usize, String> {
    let mut copied = 0;
    for entry in fs::read_dir(source).map_err(|error| {
        format!(
            "Could not read asset directory {}: {error}",
            source.display()
        )
    })? {
        let entry = entry.map_err(|error| format!("Could not read asset entry: {error}"))?;
        let name = entry.file_name();
        if name == ".DS_Store" || name == "published" {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not inspect {}: {error}", entry.path().display()))?;
        let destination = target.join(&name);
        if file_type.is_symlink() {
            return Err(format!(
                "Asset symlinks are not exportable: {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            fs::create_dir(&destination).map_err(|error| {
                format!(
                    "Could not create asset directory {}: {error}",
                    destination.display()
                )
            })?;
            copied += copy_asset_tree(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &destination).map_err(|error| {
                format!("Could not copy asset {}: {error}", entry.path().display())
            })?;
            copied += 1;
        }
    }
    Ok(copied)
}

fn require_file(path: &Path, label: &str) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("{label} is missing: {}", path.display()))
    }
}

fn require_directory(path: &Path, label: &str) -> Result<(), String> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(format!("{label} is missing: {}", path.display()))
    }
}

fn path_segment(value: &str) -> Result<&str, String> {
    let path = Path::new(value);
    let mut components = path.components();
    if value.is_empty()
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(format!("Invalid export path segment: {value:?}"));
    }
    Ok(value)
}

fn generation_segment(value: &str) -> Result<&str, String> {
    if value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("Invalid published generation id: {value:?}"));
    }
    path_segment(value)
}

fn unique_staging_path(root: &Path, bundle_id: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    root.join(format!(
        ".{bundle_id}.staging-{}-{nonce}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "starfall_game_export_{label}_{}_{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn request(root: &Path) -> GameBundleRequest {
        let executable = root.join("source-game");
        fs::write(&executable, b"game").unwrap();
        let assets = root.join("assets");
        fs::create_dir_all(assets.join("shaders")).unwrap();
        fs::write(assets.join("shaders/water.wgsl"), b"shader").unwrap();
        fs::write(assets.join(".DS_Store"), b"metadata").unwrap();
        fs::create_dir(assets.join("published")).unwrap();
        fs::write(assets.join("published/stale.json"), b"stale").unwrap();
        let published = root.join("published-generation");
        fs::create_dir(&published).unwrap();
        for file in PUBLISHED_CONTENT_FILES {
            fs::write(published.join(file), b"[]").unwrap();
        }
        GameBundleRequest {
            project_id: "test-game".into(),
            display_name: "Test Game".into(),
            bundle_id: "test-game-bundle".into(),
            executable_source: executable,
            asset_source: assets,
            published_source: published,
            published_generation: Some("0123456789abcdef".into()),
            output_root: root.join("exports"),
        }
    }

    #[test]
    fn assembles_selected_generation_and_manifest_atomically() {
        let root = test_root("complete");
        let report = assemble_game_bundle(&request(&root)).unwrap();

        assert!(report.executable.is_file());
        assert!(report
            .bundle_dir
            .join("assets/shaders/water.wgsl")
            .is_file());
        assert!(!report.bundle_dir.join("assets/.DS_Store").exists());
        assert!(!report
            .bundle_dir
            .join("assets/published/stale.json")
            .exists());
        for file in PUBLISHED_CONTENT_FILES {
            assert!(report
                .bundle_dir
                .join("assets/published/generations/0123456789abcdef")
                .join(file)
                .is_file());
        }
        let manifest: GameBundleManifest = serde_json::from_slice(
            &fs::read(report.bundle_dir.join(GAME_BUNDLE_MANIFEST_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.project_id, "test-game");
        assert_eq!(
            manifest.published_generation.as_deref(),
            Some("0123456789abcdef")
        );
        assert_eq!(
            manifest.published_files.len(),
            PUBLISHED_CONTENT_FILES.len()
        );
        let selected = crate::engine_tools::publish::published_generation_in(
            &report.bundle_dir.join("assets/published"),
        )
        .unwrap();
        assert_eq!(
            fs::read(selected.file(PUBLISHED_CONTENT_FILES[0])).unwrap(),
            b"[]"
        );
        assert_eq!(fs::read_dir(root.join("exports")).unwrap().count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_catalog_leaves_no_finished_bundle() {
        let root = test_root("missing_catalog");
        let request = request(&root);
        fs::remove_file(request.published_source.join(PUBLISHED_CONTENT_FILES[0])).unwrap();
        let result = assemble_game_bundle(&request);
        assert!(result.is_err());
        assert!(!request.output_root.join(&request.bundle_id).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_replace_an_existing_export() {
        let root = test_root("no_replace");
        let request = request(&root);
        assemble_game_bundle(&request).unwrap();
        let result = assemble_game_bundle(&request);
        assert!(result.unwrap_err().contains("already exists"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_path_traversal_in_bundle_identity() {
        let root = test_root("path_traversal");
        let mut request = request(&root);
        request.bundle_id = "../outside".into();
        assert!(assemble_game_bundle(&request).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_invalid_published_generation_identity() {
        let root = test_root("invalid_generation");
        let mut request = request(&root);
        request.published_generation = Some("not-a-content-hash".into());
        assert!(assemble_game_bundle(&request).is_err());
        assert!(!request.output_root.join(&request.bundle_id).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_an_export_root_inside_the_asset_tree() {
        let root = test_root("recursive_output");
        let mut request = request(&root);
        request.output_root = request.asset_source.join("build/exports");
        assert!(assemble_game_bundle(&request).is_err());
        assert!(!request.output_root.join(&request.bundle_id).exists());
        let _ = fs::remove_dir_all(root);
    }
}
