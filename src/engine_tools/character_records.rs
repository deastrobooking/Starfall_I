//! Project-backed records for imported Character Forge assets.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::persistence::{ContentCategory, ContentPayload, GenericRecipeDraft, ProjectStore};
use super::project_registry::ForgeProjectRegistry;
use crate::mesh_modifiers::MeshModifier;

const SPEC_FIELD: &str = "imported_character_spec";
const RECOVERY_LIMIT: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedMeshEdit {
    pub mesh_index: usize,
    #[serde(default)]
    pub modifiers: Vec<MeshModifier>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedCharacterSpec {
    pub schema_version: u32,
    pub content_id: String,
    pub display_name: String,
    /// Path relative to Bevy's `assets/` root.
    pub source_asset: String,
    pub selected_mesh: usize,
    pub root_scale: [f32; 3],
    #[serde(default)]
    pub mesh_edits: Vec<ImportedMeshEdit>,
}

impl Default for ImportedCharacterSpec {
    fn default() -> Self {
        Self {
            schema_version: 1,
            content_id: "character.imported".into(),
            display_name: "Imported Character".into(),
            source_asset: String::new(),
            selected_mesh: 0,
            root_scale: [1.0; 3],
            mesh_edits: Vec::new(),
        }
    }
}

impl ImportedCharacterSpec {
    pub fn validate(&self) -> Result<(), String> {
        if self.display_name.trim().is_empty() {
            return Err("Character name cannot be empty".into());
        }
        if self.source_asset.is_empty() {
            return Err("Drop or load a GLB before saving".into());
        }
        if !self.source_asset.to_ascii_lowercase().ends_with(".glb") {
            return Err("Imported character source must be a .glb file".into());
        }
        if self
            .root_scale
            .iter()
            .any(|value| !value.is_finite() || !(0.05..=20.0).contains(value))
        {
            return Err("Root scale must stay inside 0.05..=20.0".into());
        }
        if self.mesh_edits.iter().any(|edit| edit.modifiers.len() > 16) {
            return Err("Each mesh modifier stack is limited to 16 operations".into());
        }
        Ok(())
    }
}

fn open_active(
    registry: &ForgeProjectRegistry,
) -> Result<(ProjectStore, super::persistence::ForgeProject), String> {
    let active = registry
        .active
        .as_deref()
        .ok_or_else(|| "Open a Forge project before saving".to_string())?;
    let store = ProjectStore::new(active, RECOVERY_LIMIT);
    let (project, _) = store
        .load_with_recovery()
        .map_err(|error| format!("Could not load active project: {error}"))?;
    Ok((store, project))
}

pub fn save_to_active_project(
    registry: &ForgeProjectRegistry,
    spec: &mut ImportedCharacterSpec,
) -> Result<String, String> {
    spec.validate()?;
    let (store, mut project) = open_active(registry)?;
    let existing = project
        .records
        .iter()
        .any(|record| record.content_id == spec.content_id);
    if !existing || spec.content_id == "character.imported" {
        spec.content_id = project
            .create_content(ContentCategory::Character, &spec.display_name)
            .map_err(|error| error.to_string())?;
    }
    let record = project
        .records
        .iter_mut()
        .find(|record| record.content_id == spec.content_id)
        .ok_or_else(|| "Character record disappeared during save".to_string())?;
    if record.category != ContentCategory::Character {
        return Err(format!(
            "{} is already used by non-character content",
            spec.content_id
        ));
    }
    record.display_name = spec.display_name.clone();
    record.tags = vec!["character".into(), "imported_glb".into(), "sculpt".into()];

    let mut recipe = GenericRecipeDraft::default();
    recipe.fields.insert(
        SPEC_FIELD.into(),
        serde_json::to_value(&*spec).map_err(|error| error.to_string())?,
    );
    project
        .payloads
        .insert(spec.content_id.clone(), ContentPayload::Character(recipe));
    store
        .save(&mut project)
        .map_err(|error| format!("Project save failed: {error}"))?;
    Ok(spec.content_id.clone())
}

pub fn list_active_project(registry: &ForgeProjectRegistry) -> Vec<(String, String)> {
    open_active(registry)
        .map(|(_, project)| {
            project
                .records
                .iter()
                .filter(|record| record.category == ContentCategory::Character)
                .map(|record| (record.content_id.clone(), record.display_name.clone()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn load_from_active_project(
    registry: &ForgeProjectRegistry,
    content_id: &str,
) -> Result<ImportedCharacterSpec, String> {
    let (_, project) = open_active(registry)?;
    let Some(ContentPayload::Character(recipe)) = project.payloads.get(content_id) else {
        return Err(format!("{content_id} has no character payload"));
    };
    let value = recipe
        .fields
        .get(SPEC_FIELD)
        .ok_or_else(|| format!("{content_id} is not an imported-character record"))?;
    serde_json::from_value(value.clone()).map_err(|error| error.to_string())
}

pub fn active_project_label(registry: &ForgeProjectRegistry) -> String {
    registry
        .active
        .as_deref()
        .map(Path::display)
        .map(|path| path.to_string())
        .unwrap_or_else(|| "no active project".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_character_validation_requires_safe_glb_and_scale() {
        let mut spec = ImportedCharacterSpec::default();
        assert!(spec.validate().is_err());
        spec.source_asset = "imported_characters/hero.glb".into();
        assert!(spec.validate().is_ok());
        spec.root_scale[1] = 0.0;
        assert!(spec.validate().is_err());
    }

    #[test]
    fn imported_character_json_preserves_modifier_stack() {
        let spec = ImportedCharacterSpec {
            source_asset: "imported_characters/hero.glb".into(),
            mesh_edits: vec![ImportedMeshEdit {
                mesh_index: 0,
                modifiers: vec![MeshModifier::Smooth {
                    iterations: 2,
                    factor: 0.4,
                }],
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&spec).unwrap();
        let decoded: ImportedCharacterSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, spec);
    }
}
