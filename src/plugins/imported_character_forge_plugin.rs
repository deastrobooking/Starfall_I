//! Imported Character Forge — GLB ingestion plus non-destructive mesh editing.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bevy::asset::AssetId;
use bevy::gltf::{Gltf, GltfMesh, GltfNode};
use bevy::prelude::*;
use bevy::window::FileDragAndDrop;

use super::ui_plugin::MenuScrollPanel;
use crate::engine_tools::character_records::{self, ImportedCharacterSpec, ImportedMeshEdit};
use crate::engine_tools::project_registry::ForgeProjectRegistry;
use crate::mesh_modifiers::{apply_stack_to_mesh, MeshModifier};
use crate::rendering::{Camera3dBundle, DirectionalLightBundle, PbrBundle};
use crate::state::AppState;
use crate::tool_windows::{spawn_tool_window, ToolWindowStyle};

pub struct ImportedCharacterForgePlugin;

impl Plugin for ImportedCharacterForgePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FileDragAndDrop>()
            .init_resource::<ImportedForgeState>()
            .add_systems(OnEnter(AppState::ImportedCharacterForge), setup_forge)
            .add_systems(OnExit(AppState::ImportedCharacterForge), cleanup_forge)
            .add_systems(
                Update,
                (
                    import_dropped_glb,
                    imported_forge_actions,
                    poll_imported_gltf,
                    rebuild_imported_preview,
                    refresh_imported_labels,
                    spin_imported_preview,
                )
                    .chain()
                    .run_if(in_state(AppState::ImportedCharacterForge)),
            );
    }
}

#[derive(Resource)]
struct ImportedForgeState {
    spec: ImportedCharacterSpec,
    gltf: Option<Handle<Gltf>>,
    mesh_names: Vec<String>,
    armed_modifier: usize,
    selected_modifier: usize,
    selected_param: usize,
    saved_entries: Vec<(String, String)>,
    saved_cursor: usize,
    preview_dirty: bool,
    labels_dirty: bool,
    status: String,
    spin: f32,
    selected_material: Option<Handle<StandardMaterial>>,
    normal_material: Option<Handle<StandardMaterial>>,
}

impl Default for ImportedForgeState {
    fn default() -> Self {
        Self {
            spec: ImportedCharacterSpec::default(),
            gltf: None,
            mesh_names: Vec::new(),
            armed_modifier: 4,
            selected_modifier: 0,
            selected_param: 0,
            saved_entries: Vec::new(),
            saved_cursor: 0,
            preview_dirty: false,
            labels_dirty: true,
            status: "Drop a .glb anywhere, or load the AMP sample.".into(),
            spin: 0.0,
            selected_material: None,
            normal_material: None,
        }
    }
}

impl ImportedForgeState {
    fn selected_edit(&self) -> Option<&ImportedMeshEdit> {
        self.spec
            .mesh_edits
            .iter()
            .find(|edit| edit.mesh_index == self.spec.selected_mesh)
    }

    fn selected_edit_mut(&mut self) -> &mut ImportedMeshEdit {
        let mesh_index = self.spec.selected_mesh;
        let index = self
            .spec
            .mesh_edits
            .iter()
            .position(|edit| edit.mesh_index == mesh_index)
            .unwrap_or_else(|| {
                self.spec.mesh_edits.push(ImportedMeshEdit {
                    mesh_index,
                    modifiers: Vec::new(),
                });
                self.spec.mesh_edits.len() - 1
            });
        &mut self.spec.mesh_edits[index]
    }

    fn selected_stack(&self) -> &[MeshModifier] {
        self.selected_edit()
            .map(|edit| edit.modifiers.as_slice())
            .unwrap_or_default()
    }
}

#[derive(Component)]
struct ImportedForgeRoot;
#[derive(Component)]
struct ImportedPreviewRoot;
#[derive(Component)]
struct ImportedPreviewPart;
#[derive(Component)]
struct ImportedForgeCamera;
#[derive(Component)]
struct ImportedStatusText;
#[derive(Component)]
struct ImportedAssetText;
#[derive(Component)]
struct ImportedMeshText;
#[derive(Component)]
struct ImportedModifierText;
#[derive(Component)]
struct ImportedScaleText;

#[derive(Component, Clone, Copy)]
struct ImportedForgeButton(ImportedForgeAction);

#[derive(Debug, Clone, Copy)]
enum ImportedForgeAction {
    LoadSample,
    MeshNext,
    ModifierTypeNext,
    AddModifier,
    RemoveModifier,
    ParamNext,
    AdjustModifier(i32),
    AdjustScale(usize, f32),
    Save,
    LoadNextSaved,
    Reset,
    Back,
}

fn forge_button(
    parent: &mut ChildSpawnerCommands,
    label: impl Into<String>,
    action: ImportedForgeAction,
) {
    parent
        .spawn((
            Button,
            ImportedForgeButton(action),
            Node {
                min_height: Val::Px(30.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                margin: UiRect::all(Val::Px(2.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.10, 0.18, 0.30)),
            BorderColor::all(Color::srgb(0.28, 0.72, 0.92)),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn forge_row(parent: &mut ChildSpawnerCommands, build: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(build);
}

fn setup_forge(
    mut commands: Commands,
    mut state: ResMut<ImportedForgeState>,
    registry: Res<ForgeProjectRegistry>,
    asset_server: Res<AssetServer>,
) {
    state.saved_entries = character_records::list_active_project(&registry);
    state.labels_dirty = true;
    state.status = format!(
        "Drop a .glb to import • project: {}",
        character_records::active_project_label(&registry)
    );
    if !state.spec.source_asset.is_empty() {
        let asset_path = state.spec.source_asset.clone();
        begin_import(asset_path, &asset_server, &mut state, false);
    }

    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 1.6, 6.5)
                .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
            ..default()
        },
        ImportedForgeCamera,
        ImportedForgeRoot,
    ));
    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 8_000.0,
                shadow_maps_enabled: true,
                ..default()
            },
            transform: Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.6, 0.0)),
            ..default()
        },
        ImportedForgeRoot,
    ));

    commands
        .spawn((
            ImportedForgeRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("IMPORTED CHARACTER FORGE"),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(18.0),
                    top: Val::Px(12.0),
                    ..default()
                },
                TextFont {
                    font_size: FontSize::Px(30.0),
                    ..default()
                },
                TextColor(Color::srgb(0.42, 0.88, 1.0)),
            ));
            root.spawn((
                Text::new(""),
                ImportedStatusText,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(18.0),
                    top: Val::Px(50.0),
                    max_width: Val::Px(760.0),
                    ..default()
                },
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.92, 1.0)),
            ));

            let style = ToolWindowStyle {
                accent: Color::srgb(0.12, 0.52, 0.72),
                width: 320.0,
                content_height: Val::Px(260.0),
                ..default()
            };
            spawn_tool_window(
                root,
                "IMPORT + PROJECT",
                Vec2::new(14.0, 86.0),
                style,
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        ImportedAssetText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.82, 0.94, 1.0)),
                    ));
                    forge_row(panel, |row| {
                        forge_button(row, "LOAD AMP SAMPLE", ImportedForgeAction::LoadSample);
                        forge_button(row, "LOAD SAVED", ImportedForgeAction::LoadNextSaved);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "SAVE PROJECT", ImportedForgeAction::Save);
                        forge_button(row, "RESET", ImportedForgeAction::Reset);
                        forge_button(row, "BACK", ImportedForgeAction::Back);
                    });
                },
            );

            spawn_tool_window(
                root,
                "MESH SCULPT STACK",
                Vec2::new(14.0, 410.0),
                ToolWindowStyle {
                    accent: Color::srgb(0.48, 0.22, 0.74),
                    width: 320.0,
                    content_height: Val::Px(300.0),
                    ..default()
                },
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        ImportedMeshText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    forge_button(panel, "SELECT NEXT MESH", ImportedForgeAction::MeshNext);
                    panel.spawn((
                        Text::new(""),
                        ImportedModifierText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.92, 0.82, 1.0)),
                    ));
                    forge_row(panel, |row| {
                        forge_button(row, "MOD TYPE", ImportedForgeAction::ModifierTypeNext);
                        forge_button(row, "ADD", ImportedForgeAction::AddModifier);
                        forge_button(row, "REMOVE", ImportedForgeAction::RemoveModifier);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "PARAM", ImportedForgeAction::ParamNext);
                        forge_button(row, "−", ImportedForgeAction::AdjustModifier(-1));
                        forge_button(row, "+", ImportedForgeAction::AdjustModifier(1));
                    });
                },
            );

            spawn_tool_window(
                root,
                "ROOT PROPORTIONS",
                Vec2::new(350.0, 86.0),
                ToolWindowStyle {
                    accent: Color::srgb(0.18, 0.62, 0.48),
                    width: 270.0,
                    content_height: Val::Px(210.0),
                    ..default()
                },
                (),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        ImportedScaleText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    for (axis, label) in ["WIDTH", "HEIGHT", "DEPTH"].into_iter().enumerate() {
                        forge_row(panel, |row| {
                            forge_button(
                                row,
                                format!("{label} −"),
                                ImportedForgeAction::AdjustScale(axis, -0.05),
                            );
                            forge_button(
                                row,
                                format!("{label} +"),
                                ImportedForgeAction::AdjustScale(axis, 0.05),
                            );
                        });
                    }
                },
            );
        });
}

fn cleanup_forge(
    mut commands: Commands,
    roots: Query<Entity, With<ImportedForgeRoot>>,
    previews: Query<Entity, With<ImportedPreviewRoot>>,
) {
    for entity in roots.iter().chain(previews.iter()) {
        commands.entity(entity).despawn();
    }
}

fn sanitize_file_stem(path: &Path) -> String {
    let raw = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("character");
    let clean = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if clean.is_empty() {
        "character".into()
    } else {
        clean
    }
}

fn import_glb_into_assets(source: &Path) -> Result<String, String> {
    if !source.is_file() {
        return Err(format!("{} is not a file", source.display()));
    }
    if source
        .extension()
        .and_then(|ext| ext.to_str())
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("glb"))
    {
        return Err("Only binary glTF (.glb) files are supported".into());
    }
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let source = source
        .canonicalize()
        .map_err(|error| format!("Could not resolve GLB: {error}"))?;
    if let Ok(relative) = source.strip_prefix(
        assets
            .canonicalize()
            .map_err(|error| format!("Could not resolve assets folder: {error}"))?,
    ) {
        return Ok(relative.to_string_lossy().replace('\\', "/"));
    }

    let import_dir = assets.join("imported_characters");
    std::fs::create_dir_all(&import_dir)
        .map_err(|error| format!("Could not create import folder: {error}"))?;
    let stem = sanitize_file_stem(&source);
    let target = (1_u32..)
        .map(|suffix| {
            if suffix == 1 {
                import_dir.join(format!("{stem}.glb"))
            } else {
                import_dir.join(format!("{stem}_{suffix}.glb"))
            }
        })
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| "Could not allocate an imported GLB filename".to_string())?;
    std::fs::copy(&source, &target)
        .map_err(|error| format!("Could not copy GLB into assets: {error}"))?;
    Ok(target
        .strip_prefix(&assets)
        .unwrap_or(&target)
        .to_string_lossy()
        .replace('\\', "/"))
}

fn begin_import(
    asset_path: String,
    asset_server: &AssetServer,
    state: &mut ImportedForgeState,
    clear_edits: bool,
) {
    state.spec.source_asset = asset_path.clone();
    if clear_edits {
        state.spec.selected_mesh = 0;
        state.spec.mesh_edits.clear();
    }
    state.mesh_names.clear();
    state.gltf = Some(asset_server.load(asset_path.clone()));
    state.preview_dirty = false;
    state.labels_dirty = true;
    state.status = format!("Loading and inspecting {asset_path}…");
}

fn import_dropped_glb(
    mut drops: MessageReader<FileDragAndDrop>,
    asset_server: Res<AssetServer>,
    mut state: ResMut<ImportedForgeState>,
) {
    for event in drops.read() {
        let FileDragAndDrop::DroppedFile { path_buf, .. } = event else {
            continue;
        };
        match import_glb_into_assets(path_buf) {
            Ok(asset_path) => begin_import(asset_path, &asset_server, &mut state, true),
            Err(error) => {
                state.status = error;
                state.labels_dirty = true;
            }
        }
    }
}

fn imported_forge_actions(
    buttons: Query<(&Interaction, &ImportedForgeButton), (Changed<Interaction>, With<Button>)>,
    asset_server: Res<AssetServer>,
    registry: Res<ForgeProjectRegistry>,
    mut state: ResMut<ImportedForgeState>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button.0 {
            ImportedForgeAction::LoadSample => {
                begin_import("Characters/AMP.glb".into(), &asset_server, &mut state, true);
            }
            ImportedForgeAction::MeshNext => {
                if !state.mesh_names.is_empty() {
                    state.spec.selected_mesh =
                        (state.spec.selected_mesh + 1) % state.mesh_names.len();
                    state.selected_modifier = 0;
                    state.selected_param = 0;
                    state.preview_dirty = true;
                    state.labels_dirty = true;
                }
            }
            ImportedForgeAction::ModifierTypeNext => {
                state.armed_modifier = (state.armed_modifier + 1) % MeshModifier::TEMPLATE_COUNT;
                state.labels_dirty = true;
            }
            ImportedForgeAction::AddModifier => {
                let armed = state.armed_modifier;
                let new_index = {
                    let edit = state.selected_edit_mut();
                    if edit.modifiers.len() >= 16 {
                        None
                    } else {
                        edit.modifiers.push(MeshModifier::template(armed));
                        Some(edit.modifiers.len() - 1)
                    }
                };
                if let Some(new_index) = new_index {
                    state.selected_modifier = new_index;
                    state.selected_param = 0;
                    state.preview_dirty = true;
                    state.labels_dirty = true;
                } else {
                    state.status = "Modifier stack is full (16 maximum)".into();
                }
            }
            ImportedForgeAction::RemoveModifier => {
                let selected = state.selected_modifier;
                let remaining = {
                    let edit = state.selected_edit_mut();
                    if selected < edit.modifiers.len() {
                        edit.modifiers.remove(selected);
                        Some(edit.modifiers.len())
                    } else {
                        None
                    }
                };
                if let Some(remaining) = remaining {
                    state.selected_modifier = selected.min(remaining.saturating_sub(1));
                    state.selected_param = 0;
                    state.preview_dirty = true;
                    state.labels_dirty = true;
                }
            }
            ImportedForgeAction::ParamNext => {
                let selected = state.selected_modifier;
                if let Some(modifier) = state.selected_stack().get(selected) {
                    state.selected_param =
                        (state.selected_param + 1) % modifier.param_count().max(1);
                    state.labels_dirty = true;
                }
            }
            ImportedForgeAction::AdjustModifier(direction) => {
                let selected = state.selected_modifier;
                let param = state.selected_param;
                let edit = state.selected_edit_mut();
                if let Some(modifier) = edit.modifiers.get_mut(selected) {
                    modifier.adjust_param(param, direction);
                    state.preview_dirty = true;
                    state.labels_dirty = true;
                }
            }
            ImportedForgeAction::AdjustScale(axis, delta) => {
                state.spec.root_scale[axis] =
                    (state.spec.root_scale[axis] + delta).clamp(0.05, 20.0);
                state.preview_dirty = true;
                state.labels_dirty = true;
            }
            ImportedForgeAction::Save => {
                match character_records::save_to_active_project(&registry, &mut state.spec) {
                    Ok(content_id) => {
                        state.status = format!("Saved {content_id} to active Forge project");
                        state.saved_entries = character_records::list_active_project(&registry);
                    }
                    Err(error) => state.status = error,
                }
                state.labels_dirty = true;
            }
            ImportedForgeAction::LoadNextSaved => {
                if state.saved_entries.is_empty() {
                    state.status = "No imported characters in this project yet".into();
                } else {
                    state.saved_cursor = (state.saved_cursor + 1) % state.saved_entries.len();
                    let content_id = state.saved_entries[state.saved_cursor].0.clone();
                    match character_records::load_from_active_project(&registry, &content_id) {
                        Ok(spec) => {
                            state.spec = spec;
                            let asset_path = state.spec.source_asset.clone();
                            begin_import(asset_path, &asset_server, &mut state, false);
                            state.status = format!("Loaded {content_id}");
                        }
                        Err(error) => state.status = error,
                    }
                }
                state.labels_dirty = true;
            }
            ImportedForgeAction::Reset => {
                state.spec.root_scale = [1.0; 3];
                state.spec.mesh_edits.clear();
                state.selected_modifier = 0;
                state.selected_param = 0;
                state.preview_dirty = true;
                state.labels_dirty = true;
                state.status = "Non-destructive edits reset; source GLB unchanged".into();
            }
            ImportedForgeAction::Back => next_state.set(AppState::ProjectHub),
        }
    }
}

fn poll_imported_gltf(
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut state: ResMut<ImportedForgeState>,
) {
    if !state.mesh_names.is_empty() {
        return;
    }
    let Some(handle) = state.gltf.as_ref() else {
        return;
    };
    let Some(gltf) = gltfs.get(handle) else {
        return;
    };
    if gltf.meshes.is_empty() {
        state.status = "The imported GLB contains no editable meshes".into();
        state.labels_dirty = true;
        return;
    }
    let mut names = Vec::with_capacity(gltf.meshes.len());
    for (index, handle) in gltf.meshes.iter().enumerate() {
        let Some(mesh) = gltf_meshes.get(handle) else {
            return;
        };
        names.push(if mesh.name.is_empty() {
            format!("Mesh {index}")
        } else {
            mesh.name.clone()
        });
    }
    state.mesh_names = names;
    state.spec.selected_mesh = state
        .spec
        .selected_mesh
        .min(state.mesh_names.len().saturating_sub(1));
    state.preview_dirty = true;
    state.labels_dirty = true;
    state.status = format!(
        "GLB ready • {} meshes • select a mesh and build its sculpt stack",
        state.mesh_names.len()
    );
}

fn collect_node_instances(
    handle: &Handle<GltfNode>,
    parent_transform: Transform,
    nodes: &Assets<GltfNode>,
    mesh_handles: &[Handle<GltfMesh>],
    visited: &mut HashSet<AssetId<GltfNode>>,
    instances: &mut Vec<(usize, Transform)>,
) -> Option<()> {
    if !visited.insert(handle.id()) {
        return Some(());
    }
    let node = nodes.get(handle)?;
    let transform = parent_transform.mul_transform(node.transform);
    if let Some(mesh) = node.mesh.as_ref() {
        if let Some(mesh_index) = mesh_handles.iter().position(|candidate| candidate == mesh) {
            instances.push((mesh_index, transform));
        }
    }
    for child in &node.children {
        collect_node_instances(child, transform, nodes, mesh_handles, visited, instances)?;
    }
    Some(())
}

fn gltf_mesh_instances(gltf: &Gltf, nodes: &Assets<GltfNode>) -> Option<Vec<(usize, Transform)>> {
    if gltf.nodes.is_empty() {
        return Some(
            (0..gltf.meshes.len())
                .map(|index| (index, Transform::IDENTITY))
                .collect(),
        );
    }
    let mut child_ids = HashSet::new();
    for handle in &gltf.nodes {
        let node = nodes.get(handle)?;
        child_ids.extend(node.children.iter().map(Handle::id));
    }
    let mut instances = Vec::new();
    let mut visited = HashSet::new();
    for root in gltf
        .nodes
        .iter()
        .filter(|handle| !child_ids.contains(&handle.id()))
    {
        collect_node_instances(
            root,
            Transform::IDENTITY,
            nodes,
            &gltf.meshes,
            &mut visited,
            &mut instances,
        )?;
    }
    // Some authoring tools export meshes without attaching them to the default
    // scene graph. Keep those editable instead of silently dropping them.
    for mesh_index in 0..gltf.meshes.len() {
        if !instances.iter().any(|(index, _)| *index == mesh_index) {
            instances.push((mesh_index, Transform::IDENTITY));
        }
    }
    Some(instances)
}

fn rebuild_imported_preview(
    mut commands: Commands,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_nodes: Res<Assets<GltfNode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<ImportedForgeState>,
    old_previews: Query<Entity, With<ImportedPreviewRoot>>,
) {
    if !state.preview_dirty {
        return;
    }
    let Some(gltf_handle) = state.gltf.as_ref() else {
        return;
    };
    let Some(gltf) = gltfs.get(gltf_handle) else {
        return;
    };
    let Some(instances) = gltf_mesh_instances(gltf, &gltf_nodes) else {
        return;
    };
    let mut parts = Vec::new();
    let mut selected_is_deformable = false;
    for (mesh_index, transform) in instances {
        let gltf_mesh_handle = &gltf.meshes[mesh_index];
        let Some(gltf_mesh) = gltf_meshes.get(gltf_mesh_handle) else {
            return;
        };
        for primitive in &gltf_mesh.primitives {
            let Some(source) = meshes.get(&primitive.mesh) else {
                return;
            };
            let selected = mesh_index == state.spec.selected_mesh;
            let skinned = source.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_some();
            let deformable = skinned || source.has_morph_targets();
            let handle = if selected && !state.selected_stack().is_empty() && !deformable {
                let Some(derived) = apply_stack_to_mesh(source, state.selected_stack()) else {
                    state.status = format!("{} is not an editable triangle mesh", gltf_mesh.name);
                    return;
                };
                meshes.add(derived)
            } else {
                if selected && deformable && !state.selected_stack().is_empty() {
                    selected_is_deformable = true;
                }
                primitive.mesh.clone()
            };
            parts.push((handle, selected, transform));
        }
    }

    for entity in &old_previews {
        commands.entity(entity).despawn();
    }
    let selected_material = state.selected_material.clone().unwrap_or_else(|| {
        let handle = materials.add(StandardMaterial {
            base_color: Color::srgb(0.20, 0.72, 1.0),
            metallic: 0.15,
            perceptual_roughness: 0.42,
            ..default()
        });
        state.selected_material = Some(handle.clone());
        handle
    });
    let normal_material = state.normal_material.clone().unwrap_or_else(|| {
        let handle = materials.add(StandardMaterial {
            base_color: Color::srgb(0.56, 0.62, 0.74),
            metallic: 0.08,
            perceptual_roughness: 0.55,
            ..default()
        });
        state.normal_material = Some(handle.clone());
        handle
    });
    let root = commands
        .spawn((
            ImportedPreviewRoot,
            Transform::from_scale(Vec3::from_array(state.spec.root_scale)),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
        ))
        .id();
    for (mesh, selected, transform) in parts {
        let child = commands
            .spawn((
                ImportedPreviewPart,
                PbrBundle {
                    mesh: Mesh3d(mesh),
                    material: MeshMaterial3d(if selected {
                        selected_material.clone()
                    } else {
                        normal_material.clone()
                    }),
                    transform,
                    ..default()
                },
            ))
            .id();
        commands.entity(root).add_child(child);
    }
    state.preview_dirty = false;
    if selected_is_deformable {
        state.status =
            "Selected mesh has skinning or morph targets: root transforms are live; topology modifiers are retained but preview-safe disabled to preserve deformation data".into();
    }
    state.labels_dirty = true;
}

fn refresh_imported_labels(
    mut state: ResMut<ImportedForgeState>,
    mut status: Query<&mut Text, With<ImportedStatusText>>,
    mut asset: Query<&mut Text, (With<ImportedAssetText>, Without<ImportedStatusText>)>,
    mut mesh: Query<
        &mut Text,
        (
            With<ImportedMeshText>,
            Without<ImportedStatusText>,
            Without<ImportedAssetText>,
        ),
    >,
    mut modifier: Query<
        &mut Text,
        (
            With<ImportedModifierText>,
            Without<ImportedStatusText>,
            Without<ImportedAssetText>,
            Without<ImportedMeshText>,
        ),
    >,
    mut scale: Query<
        &mut Text,
        (
            With<ImportedScaleText>,
            Without<ImportedStatusText>,
            Without<ImportedAssetText>,
            Without<ImportedMeshText>,
            Without<ImportedModifierText>,
        ),
    >,
) {
    if !state.labels_dirty {
        return;
    }
    state.labels_dirty = false;
    for mut text in &mut status {
        *text = Text::new(state.status.clone());
    }
    for mut text in &mut asset {
        *text = Text::new(format!(
            "{}\nSource: {}\nSaved records: {}",
            state.spec.display_name,
            if state.spec.source_asset.is_empty() {
                "none"
            } else {
                &state.spec.source_asset
            },
            state.saved_entries.len()
        ));
    }
    let mesh_name = state
        .mesh_names
        .get(state.spec.selected_mesh)
        .map(String::as_str)
        .unwrap_or("none");
    for mut text in &mut mesh {
        *text = Text::new(format!(
            "Mesh {}/{}: {mesh_name}",
            state.spec.selected_mesh.saturating_add(1),
            state.mesh_names.len()
        ));
    }
    let armed = MeshModifier::template(state.armed_modifier).label();
    let stack = state.selected_stack();
    let selected = stack.get(state.selected_modifier);
    let detail = selected
        .map(|modifier| {
            format!(
                "{}\n{}",
                modifier.label(),
                modifier.param_label(state.selected_param)
            )
        })
        .unwrap_or_else(|| "stack empty".into());
    for mut text in &mut modifier {
        *text = Text::new(format!(
            "Armed: {armed}\nStack: {} operations\nSelected: {detail}",
            stack.len()
        ));
    }
    for mut text in &mut scale {
        *text = Text::new(format!(
            "Width  {:.2}\nHeight {:.2}\nDepth  {:.2}",
            state.spec.root_scale[0], state.spec.root_scale[1], state.spec.root_scale[2]
        ));
    }
}

fn spin_imported_preview(
    time: Res<Time>,
    mut state: ResMut<ImportedForgeState>,
    mut previews: Query<&mut Transform, With<ImportedPreviewRoot>>,
) {
    state.spin = (state.spin + time.delta_secs() * 0.22) % std::f32::consts::TAU;
    for mut transform in &mut previews {
        transform.rotation = Quat::from_rotation_y(state.spin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_filename_sanitizer_is_path_safe() {
        assert_eq!(
            sanitize_file_stem(Path::new("My Hero (Final).glb")),
            "my_hero__final_"
        );
        assert_eq!(sanitize_file_stem(Path::new("###.glb")), "___");
    }

    #[test]
    fn per_mesh_edit_stacks_do_not_leak_between_selections() {
        let mut state = ImportedForgeState::default();
        state.spec.selected_mesh = 2;
        state
            .selected_edit_mut()
            .modifiers
            .push(MeshModifier::Subdivide { levels: 1 });
        state.spec.selected_mesh = 3;
        assert!(state.selected_stack().is_empty());
        state.spec.selected_mesh = 2;
        assert_eq!(state.selected_stack().len(), 1);
    }
}
