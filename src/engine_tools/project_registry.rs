//! PM1 project registry: the user-owned list of Starfall Forge projects and
//! the active project path.
//!
//! The Project Hub reads this registry to list projects and set the active
//! one; the editor session syncs its `ProjectStore` from `active` when the
//! protected editing mode is entered. The registry itself lives in the
//! platform data directory next to the save files and is written atomically,
//! so no user is hard-coded to `starfall_forge/project.json` — that legacy
//! workspace is only seeded as the first entry when it already exists on disk.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::persistence::{atomic_write, ForgeProject, ProjectStore, DEFAULT_PROJECT_FILE};

const REGISTRY_FILE: &str = "forge_projects.json";
const PROJECT_RECOVERY_LIMIT: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectEntry {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Resource, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeProjectRegistry {
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
    #[serde(default)]
    pub active: Option<PathBuf>,
}

impl Default for ForgeProjectRegistry {
    fn default() -> Self {
        Self::load_or_seed(&registry_root())
    }
}

/// Directory holding the registry file and newly created project folders.
fn registry_root() -> PathBuf {
    dirs::data_dir()
        .map(|dir| dir.join("starfall_i"))
        .unwrap_or_else(|| PathBuf::from("."))
}

impl ForgeProjectRegistry {
    /// Load the registry from `root`, seeding a fresh one with the legacy
    /// working-directory workspace when that project file already exists.
    pub fn load_or_seed(root: &Path) -> Self {
        let path = root.join(REGISTRY_FILE);
        match std::fs::read_to_string(&path) {
            Ok(json) => match serde_json::from_str::<Self>(&json) {
                Ok(registry) => return registry,
                Err(error) => {
                    warn!(
                        "Ignoring corrupt project registry {}: {error}",
                        path.display()
                    );
                }
            },
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                warn!(
                    "Failed to read project registry {}: {error}",
                    path.display()
                );
            }
            Err(_) => {}
        }
        let mut registry = Self {
            projects: Vec::new(),
            active: None,
        };
        let legacy = PathBuf::from(DEFAULT_PROJECT_FILE);
        if legacy.exists() {
            registry.projects.push(ProjectEntry {
                name: "Starfall Main World".to_string(),
                path: legacy.clone(),
            });
            registry.active = Some(legacy);
        }
        registry
    }

    /// Persist the registry atomically under `root`.
    pub fn save_to(&self, root: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        atomic_write(&root.join(REGISTRY_FILE), json.as_bytes()).map_err(|e| format!("{e:?}"))
    }

    /// Persist to the platform data directory.
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&registry_root())
    }

    pub fn active_entry(&self) -> Option<&ProjectEntry> {
        let active = self.active.as_ref()?;
        self.projects.iter().find(|entry| &entry.path == active)
    }

    pub fn set_active(&mut self, path: &Path) {
        self.active = Some(path.to_path_buf());
    }

    /// Create a new empty project under `root/projects/<slug>/project.json`,
    /// register it, and mark it active. The project file is written through
    /// `ProjectStore` so it carries the same versioned format the editor
    /// saves.
    pub fn create_new_project_in(&mut self, root: &Path) -> Result<ProjectEntry, String> {
        let number = self.projects.len() + 1;
        let projects_root = root.join("projects");
        std::fs::create_dir_all(&projects_root)
            .map_err(|error| format!("Could not create projects directory: {error}"))?;
        let (name, project_dir, path) = (0..)
            .find_map(|offset| {
                let n = number + offset;
                let project_dir = projects_root.join(format!("starfall-project-{n}"));
                let path = project_dir.join("project.json");
                if self.projects.iter().any(|project| project.path == path) {
                    return None;
                }
                match std::fs::create_dir(&project_dir) {
                    Ok(()) => Some(Ok((format!("Starfall Project {n}"), project_dir, path))),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(error) => Some(Err(format!(
                        "Could not reserve project directory {}: {error}",
                        project_dir.display()
                    ))),
                }
            })
            .expect("unbounded numbering always finds a free project slot")?;

        let store = ProjectStore::new(&path, PROJECT_RECOVERY_LIMIT);
        let mut project = ForgeProject::default();
        // A project's serialized identity must not inherit the factory
        // document's shared `starfall-main-world` id. The directory number is
        // already selected uniquely against the registry, so use the same
        // durable slug for both the project id and its initial display name.
        let project_slug = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .unwrap_or("starfall-project")
            .to_owned();
        project.project_id = project_slug;
        project.display_name = name.clone();
        if let Err(error) = store.save(&mut project) {
            // This call atomically created `project_dir`, so it is safe to
            // release only that reservation if the initial save fails.
            if let Err(cleanup_error) = std::fs::remove_dir_all(&project_dir) {
                return Err(format!(
                    "Could not create project file: {error:?}; could not release reserved directory {}: {cleanup_error}",
                    project_dir.display()
                ));
            }
            return Err(format!("Could not create project file: {error:?}"));
        }

        let entry = ProjectEntry { name, path };
        self.projects.push(entry.clone());
        self.set_active(&entry.path);
        Ok(entry)
    }

    /// Create a new project in the platform data directory.
    pub fn create_new_project(&mut self) -> Result<ProjectEntry, String> {
        self.create_new_project_in(&registry_root())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "starfall_registry_{label}_{}_{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("test root should be creatable");
        root
    }

    #[test]
    fn missing_registry_seeds_empty_when_no_legacy_workspace() {
        let root = test_root("seed_empty");
        let registry = ForgeProjectRegistry::load_or_seed(&root);
        // The test process cwd has no starfall_forge/project.json in CI, but
        // guard the assertion so a developer checkout with one still passes.
        if !PathBuf::from(DEFAULT_PROJECT_FILE).exists() {
            assert!(registry.projects.is_empty());
            assert!(registry.active.is_none());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn registry_save_and_load_round_trip() {
        let root = test_root("round_trip");
        let mut registry = ForgeProjectRegistry {
            projects: Vec::new(),
            active: None,
        };
        registry.projects.push(ProjectEntry {
            name: "Test World".to_string(),
            path: root.join("projects/test-world/project.json"),
        });
        registry.set_active(&root.join("projects/test-world/project.json"));
        registry.save_to(&root).expect("registry should save");

        let loaded = ForgeProjectRegistry::load_or_seed(&root);
        assert_eq!(loaded, registry);
        assert_eq!(
            loaded.active_entry().map(|e| e.name.as_str()),
            Some("Test World")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_registry_reseeds_instead_of_panicking() {
        let root = test_root("corrupt");
        std::fs::write(root.join(REGISTRY_FILE), b"{ not json").unwrap();
        let registry = ForgeProjectRegistry::load_or_seed(&root);
        if !PathBuf::from(DEFAULT_PROJECT_FILE).exists() {
            assert!(registry.projects.is_empty());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_new_project_writes_a_loadable_store_and_activates_it() {
        let root = test_root("create");
        let mut registry = ForgeProjectRegistry {
            projects: Vec::new(),
            active: None,
        };
        let entry = registry
            .create_new_project_in(&root)
            .expect("project creation should succeed");
        assert_eq!(entry.name, "Starfall Project 1");
        assert!(entry.path.exists());
        assert_eq!(registry.active.as_deref(), Some(entry.path.as_path()));

        let store = ProjectStore::new(&entry.path, PROJECT_RECOVERY_LIMIT);
        let first_project = store.load().expect("new project file should load");
        assert_eq!(first_project.project_id, "starfall-project-1");
        assert_eq!(first_project.display_name, entry.name);

        let second = registry
            .create_new_project_in(&root)
            .expect("second project creation should succeed");
        assert_eq!(second.name, "Starfall Project 2");
        assert_ne!(second.path, entry.path);
        let second_project = ProjectStore::new(&second.path, PROJECT_RECOVERY_LIMIT)
            .load()
            .expect("second project file should load");
        assert_eq!(second_project.project_id, "starfall-project-2");
        assert_ne!(second_project.project_id, first_project.project_id);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_new_project_never_overwrites_an_unregistered_project_file() {
        let root = test_root("create_preserves_unregistered");
        let occupied = root
            .join("projects")
            .join("starfall-project-1")
            .join("project.json");
        std::fs::create_dir_all(occupied.parent().expect("occupied project has a parent"))
            .expect("occupied project directory should be creatable");
        std::fs::write(&occupied, b"preserve this project").expect("fixture should be writable");

        let mut registry = ForgeProjectRegistry {
            projects: Vec::new(),
            active: None,
        };
        let entry = registry
            .create_new_project_in(&root)
            .expect("project creation should skip occupied paths");

        assert_eq!(entry.name, "Starfall Project 2");
        assert_eq!(
            std::fs::read(&occupied).expect("occupied project remains readable"),
            b"preserve this project"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_new_project_skips_an_occupied_source_only_directory() {
        let root = test_root("create_preserves_source_only");
        let occupied_dir = root.join("projects").join("starfall-project-1");
        let source = occupied_dir.join("characters").join("hero.json");
        std::fs::create_dir_all(source.parent().expect("source fixture has a parent"))
            .expect("source-only project directory should be creatable");
        std::fs::write(&source, b"preserve source-only project")
            .expect("source-only fixture should be writable");

        let mut registry = ForgeProjectRegistry {
            projects: Vec::new(),
            active: None,
        };
        let entry = registry
            .create_new_project_in(&root)
            .expect("project creation should skip an occupied directory");

        assert_eq!(entry.name, "Starfall Project 2");
        assert_eq!(
            std::fs::read(&source).expect("source-only project remains readable"),
            b"preserve source-only project"
        );
        assert!(!occupied_dir.join("project.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
