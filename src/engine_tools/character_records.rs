//! Project-backed records for imported Character Forge assets.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::persistence::{ContentCategory, ContentPayload, GenericRecipeDraft, ProjectStore};
use super::project_registry::ForgeProjectRegistry;
use crate::character::mesh_modifiers::MeshModifier;

const SPEC_FIELD: &str = "imported_character_spec";
const PART_FIELD: &str = "imported_modular_part";
const RECOVERY_LIMIT: usize = 3;
const CURRENT_IMPORTED_CHARACTER_SCHEMA: u32 = 5;
const CURRENT_IMPORTED_PART_SCHEMA: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedMeshEdit {
    pub mesh_index: usize,
    #[serde(default)]
    pub modifiers: Vec<MeshModifier>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedMaterialEdit {
    pub source_asset: String,
    pub mesh_index: usize,
    pub primitive_index: usize,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub perceptual_roughness: f32,
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub base_color_texture: Option<String>,
    pub normal_texture: Option<String>,
    pub metallic_roughness_texture: Option<String>,
    pub emissive_texture: Option<String>,
    #[serde(default)]
    pub cleared_texture_channels: [bool; 4],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportedUvProjection {
    Authored,
    PlanarXy,
    PlanarXz,
    PlanarYz,
    CylindricalY,
    BoxSeams,
    SeamUnwrap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedFacePaint {
    pub face_indices: Vec<u32>,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedUvEdit {
    pub source_asset: String,
    pub mesh_index: usize,
    pub primitive_index: usize,
    pub projection: ImportedUvProjection,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
    pub rotation_radians: f32,
    #[serde(default)]
    pub seam_edges: Vec<[u32; 2]>,
    #[serde(default)]
    pub face_paints: Vec<ImportedFacePaint>,
    #[serde(default)]
    pub baked_paint_texture: Option<String>,
}

impl ImportedUvEdit {
    pub fn validate(&self) -> Result<(), String> {
        if !self.source_asset.to_ascii_lowercase().ends_with(".glb") {
            return Err("UV edit source must be a GLB".into());
        }
        if self
            .scale
            .iter()
            .chain(self.offset.iter())
            .chain(std::iter::once(&self.rotation_radians))
            .any(|value| !value.is_finite())
            || self
                .scale
                .iter()
                .any(|value| !(0.01..=100.0).contains(value))
            || self
                .offset
                .iter()
                .any(|value| !(-100.0..=100.0).contains(value))
            || !(-std::f32::consts::TAU..=std::f32::consts::TAU).contains(&self.rotation_radians)
        {
            return Err("UV transform is outside its supported range".into());
        }
        if self.face_paints.len() > 64 {
            return Err("A primitive is limited to 64 non-destructive paint strokes".into());
        }
        if self.seam_edges.len() > 250_000 || self.seam_edges.iter().any(|edge| edge[0] >= edge[1])
        {
            return Err("UV seam edges must be sorted, distinct vertex pairs".into());
        }
        if self.baked_paint_texture.as_ref().is_some_and(|path| {
            path.starts_with('/')
                || path.contains("..")
                || !path.to_ascii_lowercase().ends_with(".png")
        }) {
            return Err("Baked paint texture must be a project-relative PNG".into());
        }
        for paint in &self.face_paints {
            if paint.face_indices.is_empty()
                || paint.face_indices.len() > 250_000
                || paint
                    .color
                    .iter()
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            {
                return Err(
                    "Face paint stroke is empty, too large, or has an invalid color".into(),
                );
            }
        }
        Ok(())
    }
}

impl ImportedMaterialEdit {
    pub fn validate(&self) -> Result<(), String> {
        if !self.source_asset.to_ascii_lowercase().ends_with(".glb") {
            return Err("Material override source must be a GLB".into());
        }
        if self
            .base_color
            .iter()
            .chain(self.emissive.iter())
            .chain([
                &self.metallic,
                &self.perceptual_roughness,
                &self.emissive_strength,
            ])
            .any(|value| !value.is_finite())
        {
            return Err("Material controls must be finite".into());
        }
        if self
            .base_color
            .iter()
            .any(|value| !(0.0..=1.0).contains(value))
            || self
                .emissive
                .iter()
                .any(|value| !(0.0..=1.0).contains(value))
            || !(0.0..=1.0).contains(&self.metallic)
            || !(0.089..=1.0).contains(&self.perceptual_roughness)
            || !(0.0..=100.0).contains(&self.emissive_strength)
        {
            return Err("Material controls are outside their supported ranges".into());
        }
        for path in [
            &self.base_color_texture,
            &self.normal_texture,
            &self.metallic_roughness_texture,
            &self.emissive_texture,
        ]
        .into_iter()
        .flatten()
        {
            let lower = path.to_ascii_lowercase();
            if path.starts_with('/')
                || path.contains("..")
                || !(lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg"))
            {
                return Err(format!("Invalid imported texture path: {path}"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportedPartSlot {
    Body,
    Head,
    Hair,
    LeftArm,
    RightArm,
    LeftHand,
    RightHand,
    LeftLeg,
    RightLeg,
    LeftFoot,
    RightFoot,
    Shoulders,
    Cape,
    Accessory,
    Weapon,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportedAnimationJoint {
    Pelvis,
    Spine,
    Chest,
    Neck,
    Head,
    LeftShoulder,
    LeftElbow,
    LeftWrist,
    RightShoulder,
    RightElbow,
    RightWrist,
    LeftHip,
    LeftKnee,
    LeftAnkle,
    RightHip,
    RightKnee,
    RightAnkle,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportedGameplayRole {
    Visual,
    Hurtbox,
    Weapon,
    Shield,
    Traversal,
    Interact,
    Cosmetic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedPartAssignment {
    pub part_id: String,
    pub display_name: String,
    pub source_asset: String,
    pub mesh_index: usize,
    pub primitive_index: usize,
    pub face_indices: Vec<u32>,
    pub slot: ImportedPartSlot,
    pub joint: ImportedAnimationJoint,
    pub gameplay_role: ImportedGameplayRole,
    pub pivot: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportedPartAsset {
    pub schema_version: u32,
    pub content_id: String,
    pub assignment: ImportedPartAssignment,
    #[serde(default)]
    pub material: Option<ImportedMaterialEdit>,
    #[serde(default)]
    pub uv_edit: Option<ImportedUvEdit>,
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
    #[serde(default)]
    pub material_edits: Vec<ImportedMaterialEdit>,
    #[serde(default)]
    pub uv_edits: Vec<ImportedUvEdit>,
    #[serde(default)]
    pub part_assignments: Vec<ImportedPartAssignment>,
}

impl Default for ImportedCharacterSpec {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_IMPORTED_CHARACTER_SCHEMA,
            content_id: "character.imported".into(),
            display_name: "Imported Character".into(),
            source_asset: String::new(),
            selected_mesh: 0,
            root_scale: [1.0; 3],
            mesh_edits: Vec::new(),
            material_edits: Vec::new(),
            uv_edits: Vec::new(),
            part_assignments: Vec::new(),
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
        let mut material_keys = std::collections::BTreeSet::new();
        for material in &self.material_edits {
            material.validate()?;
            if !material_keys.insert((
                material.source_asset.as_str(),
                material.mesh_index,
                material.primitive_index,
            )) {
                return Err("Only one material override is allowed per source primitive".into());
            }
        }
        let mut uv_keys = std::collections::BTreeSet::new();
        for edit in &self.uv_edits {
            edit.validate()?;
            if !uv_keys.insert((
                edit.source_asset.as_str(),
                edit.mesh_index,
                edit.primitive_index,
            )) {
                return Err("Only one UV edit is allowed per source primitive".into());
            }
        }
        let mut part_ids = std::collections::BTreeSet::new();
        let mut claimed_faces = std::collections::BTreeSet::new();
        for assignment in &self.part_assignments {
            if assignment.part_id.is_empty()
                || !assignment
                    .part_id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
            {
                return Err("Part IDs may only use letters, numbers, '.', '_' and '-'".into());
            }
            if !part_ids.insert(assignment.part_id.as_str()) {
                return Err(format!("Duplicate modular part ID: {}", assignment.part_id));
            }
            if assignment.face_indices.is_empty() {
                return Err(format!("{} has no selected faces", assignment.display_name));
            }
            if !assignment
                .source_asset
                .to_ascii_lowercase()
                .ends_with(".glb")
            {
                return Err(format!(
                    "{} has an invalid source GLB",
                    assignment.display_name
                ));
            }
            for face in &assignment.face_indices {
                if !claimed_faces.insert((
                    assignment.source_asset.as_str(),
                    assignment.mesh_index,
                    assignment.primitive_index,
                    *face,
                )) {
                    return Err(format!(
                        "Face {face} is assigned to more than one modular part"
                    ));
                }
            }
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
    spec.schema_version = CURRENT_IMPORTED_CHARACTER_SCHEMA;
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
                .filter(|record| {
                    record.category == ContentCategory::Character
                        && project
                            .payloads
                            .get(&record.content_id)
                            .and_then(|payload| match payload {
                                ContentPayload::Character(recipe) => recipe.fields.get(SPEC_FIELD),
                                _ => None,
                            })
                            .is_some()
                })
                .map(|record| (record.content_id.clone(), record.display_name.clone()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn save_part_to_active_project(
    registry: &ForgeProjectRegistry,
    assignment: &ImportedPartAssignment,
    material: Option<&ImportedMaterialEdit>,
    uv_edit: Option<&ImportedUvEdit>,
) -> Result<String, String> {
    if assignment.face_indices.is_empty() {
        return Err("Select at least one face before saving a part".into());
    }
    let (store, mut project) = open_active(registry)?;
    let content_id = project
        .create_content(ContentCategory::Character, &assignment.display_name)
        .map_err(|error| error.to_string())?;
    let record = project
        .records
        .iter_mut()
        .find(|record| record.content_id == content_id)
        .ok_or_else(|| "Part record disappeared during save".to_string())?;
    record.tags = vec![
        "character_part".into(),
        "modular".into(),
        "animation_ready".into(),
    ];
    let part = ImportedPartAsset {
        schema_version: CURRENT_IMPORTED_PART_SCHEMA,
        content_id: content_id.clone(),
        assignment: assignment.clone(),
        material: material.cloned(),
        uv_edit: uv_edit.cloned(),
    };
    let mut recipe = GenericRecipeDraft::default();
    recipe.fields.insert(
        PART_FIELD.into(),
        serde_json::to_value(part).map_err(|error| error.to_string())?,
    );
    project
        .payloads
        .insert(content_id.clone(), ContentPayload::Character(recipe));
    store
        .save(&mut project)
        .map_err(|error| format!("Project save failed: {error}"))?;
    Ok(content_id)
}

pub fn list_part_library(registry: &ForgeProjectRegistry) -> Vec<(String, String)> {
    open_active(registry)
        .map(|(_, project)| {
            project
                .records
                .iter()
                .filter(|record| {
                    project
                        .payloads
                        .get(&record.content_id)
                        .and_then(|payload| match payload {
                            ContentPayload::Character(recipe) => recipe.fields.get(PART_FIELD),
                            _ => None,
                        })
                        .is_some()
                })
                .map(|record| (record.content_id.clone(), record.display_name.clone()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn load_part_from_active_project(
    registry: &ForgeProjectRegistry,
    content_id: &str,
) -> Result<ImportedPartAsset, String> {
    let (_, project) = open_active(registry)?;
    let Some(ContentPayload::Character(recipe)) = project.payloads.get(content_id) else {
        return Err(format!("{content_id} has no character payload"));
    };
    let value = recipe
        .fields
        .get(PART_FIELD)
        .ok_or_else(|| format!("{content_id} is not a modular-part record"))?;
    serde_json::from_value(value.clone()).map_err(|error| error.to_string())
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

    #[test]
    fn material_overrides_validate_texture_paths_and_round_trip() {
        let material = ImportedMaterialEdit {
            source_asset: "imported_characters/hero.glb".into(),
            mesh_index: 0,
            primitive_index: 1,
            base_color: [0.1, 0.2, 0.3, 1.0],
            metallic: 0.8,
            perceptual_roughness: 0.3,
            emissive: [1.0, 0.2, 0.4],
            emissive_strength: 3.0,
            base_color_texture: Some("imported_textures/armor.png".into()),
            normal_texture: None,
            metallic_roughness_texture: None,
            emissive_texture: None,
            cleared_texture_channels: [false; 4],
        };
        assert!(material.validate().is_ok());
        let mut spec = ImportedCharacterSpec {
            source_asset: material.source_asset.clone(),
            material_edits: vec![material],
            ..Default::default()
        };
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(
            serde_json::from_str::<ImportedCharacterSpec>(&json).unwrap(),
            spec
        );
        spec.material_edits[0].base_color_texture = Some("../escape.png".into());
        assert!(spec.validate().is_err());
    }

    #[test]
    fn schema_two_character_defaults_to_no_material_overrides() {
        let json = r#"{
            "schema_version": 2,
            "content_id": "character.legacy",
            "display_name": "Legacy",
            "source_asset": "imported_characters/legacy.glb",
            "selected_mesh": 0,
            "root_scale": [1.0, 1.0, 1.0],
            "mesh_edits": [],
            "part_assignments": []
        }"#;
        let spec: ImportedCharacterSpec = serde_json::from_str(json).unwrap();
        assert!(spec.material_edits.is_empty());
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn uv_projection_and_face_paint_round_trip_in_schema_four() {
        let edit = ImportedUvEdit {
            source_asset: "imported_characters/hero.glb".into(),
            mesh_index: 2,
            primitive_index: 0,
            projection: ImportedUvProjection::BoxSeams,
            scale: [1.5, 0.75],
            offset: [0.1, -0.2],
            rotation_radians: 0.25,
            seam_edges: vec![[0, 1]],
            face_paints: vec![ImportedFacePaint {
                face_indices: vec![1, 2, 3],
                color: [0.8, 0.1, 0.3, 1.0],
            }],
            baked_paint_texture: None,
        };
        assert!(edit.validate().is_ok());
        let spec = ImportedCharacterSpec {
            source_asset: edit.source_asset.clone(),
            uv_edits: vec![edit],
            ..Default::default()
        };
        let json = serde_json::to_string(&spec).unwrap();
        let decoded: ImportedCharacterSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, spec);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn imported_character_rejects_overlapping_part_regions() {
        let part = ImportedPartAssignment {
            part_id: "arm.left".into(),
            display_name: "Left Arm".into(),
            source_asset: "imported_characters/hero.glb".into(),
            mesh_index: 0,
            primitive_index: 0,
            face_indices: vec![2],
            slot: ImportedPartSlot::LeftArm,
            joint: ImportedAnimationJoint::LeftShoulder,
            gameplay_role: ImportedGameplayRole::Visual,
            pivot: [0.0; 3],
        };
        let mut spec = ImportedCharacterSpec {
            source_asset: part.source_asset.clone(),
            part_assignments: vec![
                part.clone(),
                ImportedPartAssignment {
                    part_id: "arm.left.second".into(),
                    ..part
                },
            ],
            ..Default::default()
        };
        assert!(spec.validate().is_err());
        spec.part_assignments[1].face_indices = vec![3];
        assert!(spec.validate().is_ok());
    }
}
