//! Imported Character Forge — GLB ingestion plus non-destructive mesh editing.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use bevy::asset::AssetId;
use bevy::gltf::{Gltf, GltfMaterial, GltfMesh, GltfNode};
use bevy::prelude::*;
use bevy::window::{FileDragAndDrop, PrimaryWindow};

use super::ui_plugin::MenuScrollPanel;
use crate::character::imported_modular::request_imported_character;
use crate::character::mesh_modifiers::{apply_stack_to_mesh, MeshModifier};
use crate::engine::rendering::{Camera3dBundle, DirectionalLightBundle, PbrBundle};
use crate::engine::state::AppState;
use crate::engine_tools::character_records::{
    self, ImportedAnimationJoint, ImportedCharacterSpec, ImportedFacePaint, ImportedGameplayRole,
    ImportedMaterialEdit, ImportedMeshEdit, ImportedPartAssignment, ImportedPartSlot,
    ImportedUvEdit, ImportedUvProjection,
};
use crate::engine_tools::mesh_selection::{pick_face, selected_face_mesh, MeshTopology};
use crate::engine_tools::mesh_uv::{
    apply_uv_edit, rasterize_face_paint, selection_boundary_edges, static_mesh_copy,
};
use crate::engine_tools::project_registry::ForgeProjectRegistry;
use crate::engine_tools::tool_windows::{spawn_tool_window, ToolWindowStyle};
use crate::resources::ImportedForgeReturnTarget;

pub struct ImportedCharacterForgePlugin;

impl Plugin for ImportedCharacterForgePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FileDragAndDrop>()
            .init_resource::<ImportedForgeState>()
            .init_resource::<ImportedForgeReturnTarget>()
            .add_systems(OnEnter(AppState::ImportedCharacterForge), setup_forge)
            .add_systems(OnExit(AppState::ImportedCharacterForge), cleanup_forge)
            .add_systems(
                Update,
                (
                    import_dropped_glb,
                    imported_forge_actions,
                    poll_imported_gltf,
                    pick_imported_face,
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
    part_library: Vec<(String, String)>,
    part_library_cursor: usize,
    selected_primitive: usize,
    face_cursor: u32,
    selected_faces: BTreeSet<u32>,
    part_preset: usize,
    selected_assignment: usize,
    material_param: usize,
    texture_channel: usize,
    uv_param: usize,
    paint_color: usize,
    preview_dirty: bool,
    labels_dirty: bool,
    status: String,
    spin: f32,
    face_material: Option<Handle<StandardMaterial>>,
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
            part_library: Vec::new(),
            part_library_cursor: 0,
            selected_primitive: 0,
            face_cursor: 0,
            selected_faces: BTreeSet::new(),
            part_preset: 0,
            selected_assignment: 0,
            material_param: 0,
            texture_channel: 0,
            uv_param: 0,
            paint_color: 0,
            preview_dirty: false,
            labels_dirty: true,
            status: "Drop a .glb anywhere, or load the AMP sample.".into(),
            spin: 0.0,
            face_material: None,
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

    fn selected_material_edit(&self) -> Option<&ImportedMaterialEdit> {
        self.spec.material_edits.iter().find(|edit| {
            edit.source_asset == self.spec.source_asset
                && edit.mesh_index == self.spec.selected_mesh
                && edit.primitive_index == self.selected_primitive
        })
    }

    fn selected_material_edit_mut(&mut self) -> &mut ImportedMaterialEdit {
        let source_asset = self.spec.source_asset.clone();
        let mesh_index = self.spec.selected_mesh;
        let primitive_index = self.selected_primitive;
        let index = self
            .spec
            .material_edits
            .iter()
            .position(|edit| {
                edit.source_asset == source_asset
                    && edit.mesh_index == mesh_index
                    && edit.primitive_index == primitive_index
            })
            .unwrap_or_else(|| {
                self.spec.material_edits.push(ImportedMaterialEdit {
                    source_asset,
                    mesh_index,
                    primitive_index,
                    base_color: [1.0; 4],
                    metallic: 0.0,
                    perceptual_roughness: 0.5,
                    emissive: [1.0; 3],
                    emissive_strength: 0.0,
                    base_color_texture: None,
                    normal_texture: None,
                    metallic_roughness_texture: None,
                    emissive_texture: None,
                    cleared_texture_channels: [false; 4],
                });
                self.spec.material_edits.len() - 1
            });
        &mut self.spec.material_edits[index]
    }

    fn selected_uv_edit(&self) -> Option<&ImportedUvEdit> {
        self.spec.uv_edits.iter().find(|edit| {
            edit.source_asset == self.spec.source_asset
                && edit.mesh_index == self.spec.selected_mesh
                && edit.primitive_index == self.selected_primitive
        })
    }

    fn selected_uv_edit_mut(&mut self) -> &mut ImportedUvEdit {
        let source_asset = self.spec.source_asset.clone();
        let mesh_index = self.spec.selected_mesh;
        let primitive_index = self.selected_primitive;
        let index = self
            .spec
            .uv_edits
            .iter()
            .position(|edit| {
                edit.source_asset == source_asset
                    && edit.mesh_index == mesh_index
                    && edit.primitive_index == primitive_index
            })
            .unwrap_or_else(|| {
                self.spec.uv_edits.push(ImportedUvEdit {
                    source_asset,
                    mesh_index,
                    primitive_index,
                    projection: ImportedUvProjection::Authored,
                    scale: [1.0; 2],
                    offset: [0.0; 2],
                    rotation_radians: 0.0,
                    seam_edges: Vec::new(),
                    face_paints: Vec::new(),
                    baked_paint_texture: None,
                });
                self.spec.uv_edits.len() - 1
            });
        &mut self.spec.uv_edits[index]
    }
}

#[derive(Component)]
struct ImportedForgeRoot;
#[derive(Component)]
struct ImportedPreviewRoot;
#[derive(Component)]
struct ImportedPreviewPart;
#[derive(Component)]
struct ImportedRuntimeTestRoot;
#[derive(Component, Clone, Copy)]
struct ImportedPreviewSource {
    mesh_index: usize,
    primitive_index: usize,
}
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
#[derive(Component)]
struct ImportedSelectionText;
#[derive(Component)]
struct ImportedAssignmentText;
#[derive(Component)]
struct ImportedMaterialText;
#[derive(Component)]
struct ImportedUvText;

#[derive(Component, Clone, Copy)]
struct ImportedForgeButton(ImportedForgeAction);

#[derive(Debug, Clone, Copy)]
enum ImportedForgeAction {
    LoadSample,
    MeshNext,
    PrimitiveNext,
    FacePrevious,
    FaceNext,
    ToggleFace,
    SelectAll,
    SelectNone,
    SelectInvert,
    SelectLinked,
    SelectGrow,
    SelectShrink,
    SelectSimilar,
    SelectMirror,
    ModifierTypeNext,
    AddModifier,
    RemoveModifier,
    ParamNext,
    AdjustModifier(i32),
    AdjustScale(usize, f32),
    PartPresetNext,
    AssignPart,
    AssignmentNext,
    RemoveAssignment,
    SavePart,
    LoadPart,
    PreviewAssembly,
    MaterialColorNext,
    MaterialParamNext,
    AdjustMaterial(i32),
    TextureChannelNext,
    ClearTexture,
    ResetMaterial,
    UvProjectionNext,
    UvParamNext,
    AdjustUv(i32),
    PaintColorNext,
    PaintSelection,
    MarkSeams,
    ClearSeams,
    BakePaintTexture,
    ClearPaint,
    ResetUv,
    Save,
    LoadNextSaved,
    Reset,
    Back,
}

#[derive(Debug, Clone, Copy)]
enum ImportedTextureChannel {
    BaseColor,
    Normal,
    MetallicRoughness,
    Emissive,
}

impl ImportedTextureChannel {
    const ALL: [Self; 4] = [
        Self::BaseColor,
        Self::Normal,
        Self::MetallicRoughness,
        Self::Emissive,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::BaseColor => "BASE COLOR",
            Self::Normal => "NORMAL",
            Self::MetallicRoughness => "METAL/ROUGH",
            Self::Emissive => "EMISSIVE",
        }
    }
}

const IMPORTED_COLOR_PALETTE: [[f32; 4]; 14] = [
    [1.0, 1.0, 1.0, 1.0],
    [0.03, 0.04, 0.06, 1.0],
    [0.72, 0.02, 0.04, 1.0],
    [1.0, 0.18, 0.02, 1.0],
    [1.0, 0.72, 0.02, 1.0],
    [1.0, 0.08, 0.42, 1.0],
    [0.64, 0.12, 0.95, 1.0],
    [0.06, 0.3, 1.0, 1.0],
    [0.02, 0.86, 1.0, 1.0],
    [0.02, 0.76, 0.36, 1.0],
    [0.36, 0.82, 0.1, 1.0],
    [0.45, 0.18, 0.06, 1.0],
    [0.48, 0.5, 0.56, 1.0],
    [0.82, 0.68, 0.28, 1.0],
];

const IMPORTED_UV_PROJECTIONS: [ImportedUvProjection; 7] = [
    ImportedUvProjection::Authored,
    ImportedUvProjection::PlanarXy,
    ImportedUvProjection::PlanarXz,
    ImportedUvProjection::PlanarYz,
    ImportedUvProjection::CylindricalY,
    ImportedUvProjection::BoxSeams,
    ImportedUvProjection::SeamUnwrap,
];

fn uv_projection_label(projection: ImportedUvProjection) -> &'static str {
    match projection {
        ImportedUvProjection::Authored => "AUTHORED",
        ImportedUvProjection::PlanarXy => "PLANAR XY",
        ImportedUvProjection::PlanarXz => "PLANAR XZ",
        ImportedUvProjection::PlanarYz => "PLANAR YZ",
        ImportedUvProjection::CylindricalY => "CYLINDER Y",
        ImportedUvProjection::BoxSeams => "BOX + AUTO SEAMS",
        ImportedUvProjection::SeamUnwrap => "MANUAL SEAMS + PACK",
    }
}

#[derive(Debug, Clone, Copy)]
struct ImportedPartPreset {
    label: &'static str,
    slot: ImportedPartSlot,
    joint: ImportedAnimationJoint,
    role: ImportedGameplayRole,
}

impl ImportedPartPreset {
    const ALL: [Self; 16] = [
        Self::new(
            "BODY",
            ImportedPartSlot::Body,
            ImportedAnimationJoint::Pelvis,
        ),
        Self::new("HEAD", ImportedPartSlot::Head, ImportedAnimationJoint::Head),
        Self::new("HAIR", ImportedPartSlot::Hair, ImportedAnimationJoint::Head),
        Self::new(
            "LEFT ARM",
            ImportedPartSlot::LeftArm,
            ImportedAnimationJoint::LeftShoulder,
        ),
        Self::new(
            "RIGHT ARM",
            ImportedPartSlot::RightArm,
            ImportedAnimationJoint::RightShoulder,
        ),
        Self::new(
            "LEFT HAND",
            ImportedPartSlot::LeftHand,
            ImportedAnimationJoint::LeftWrist,
        ),
        Self::new(
            "RIGHT HAND",
            ImportedPartSlot::RightHand,
            ImportedAnimationJoint::RightWrist,
        ),
        Self::new(
            "LEFT LEG",
            ImportedPartSlot::LeftLeg,
            ImportedAnimationJoint::LeftHip,
        ),
        Self::new(
            "RIGHT LEG",
            ImportedPartSlot::RightLeg,
            ImportedAnimationJoint::RightHip,
        ),
        Self::new(
            "LEFT FOOT",
            ImportedPartSlot::LeftFoot,
            ImportedAnimationJoint::LeftAnkle,
        ),
        Self::new(
            "RIGHT FOOT",
            ImportedPartSlot::RightFoot,
            ImportedAnimationJoint::RightAnkle,
        ),
        Self::new(
            "SHOULDERS",
            ImportedPartSlot::Shoulders,
            ImportedAnimationJoint::Chest,
        ),
        Self::new(
            "CAPE",
            ImportedPartSlot::Cape,
            ImportedAnimationJoint::Chest,
        ),
        Self::new(
            "ACCESSORY",
            ImportedPartSlot::Accessory,
            ImportedAnimationJoint::None,
        ),
        Self {
            label: "WEAPON",
            slot: ImportedPartSlot::Weapon,
            joint: ImportedAnimationJoint::RightWrist,
            role: ImportedGameplayRole::Weapon,
        },
        Self {
            label: "HURTBOX",
            slot: ImportedPartSlot::Body,
            joint: ImportedAnimationJoint::Pelvis,
            role: ImportedGameplayRole::Hurtbox,
        },
    ];

    const fn new(
        label: &'static str,
        slot: ImportedPartSlot,
        joint: ImportedAnimationJoint,
    ) -> Self {
        Self {
            label,
            slot,
            joint,
            role: ImportedGameplayRole::Visual,
        }
    }
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
    state.part_library = character_records::list_part_library(&registry);
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

            spawn_tool_window(
                root,
                "MESH-AWARE SELECTION",
                Vec2::new(350.0, 330.0),
                ToolWindowStyle {
                    accent: Color::srgb(0.92, 0.48, 0.12),
                    width: 390.0,
                    content_height: Val::Px(350.0),
                    ..default()
                },
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        ImportedSelectionText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.88, 0.62)),
                    ));
                    panel.spawn((
                        Text::new("ALT+CLICK preview • SHIFT+ALT+CLICK linked"),
                        TextFont {
                            font_size: FontSize::Px(11.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.82, 0.75, 0.65)),
                    ));
                    forge_row(panel, |row| {
                        forge_button(row, "NEXT PRIMITIVE", ImportedForgeAction::PrimitiveNext);
                        forge_button(row, "FACE −", ImportedForgeAction::FacePrevious);
                        forge_button(row, "FACE +", ImportedForgeAction::FaceNext);
                        forge_button(row, "TOGGLE", ImportedForgeAction::ToggleFace);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "ALL", ImportedForgeAction::SelectAll);
                        forge_button(row, "NONE", ImportedForgeAction::SelectNone);
                        forge_button(row, "INVERT", ImportedForgeAction::SelectInvert);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "LINKED", ImportedForgeAction::SelectLinked);
                        forge_button(row, "GROW", ImportedForgeAction::SelectGrow);
                        forge_button(row, "SHRINK", ImportedForgeAction::SelectShrink);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "SIMILAR NORMAL", ImportedForgeAction::SelectSimilar);
                        forge_button(row, "MIRROR X", ImportedForgeAction::SelectMirror);
                    });
                },
            );

            spawn_tool_window(
                root,
                "MODULAR PART ASSIGNMENT",
                Vec2::new(756.0, 86.0),
                ToolWindowStyle {
                    accent: Color::srgb(0.92, 0.18, 0.42),
                    width: 350.0,
                    content_height: Val::Px(300.0),
                    ..default()
                },
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        ImportedAssignmentText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.82, 0.9)),
                    ));
                    forge_row(panel, |row| {
                        forge_button(row, "PART PRESET", ImportedForgeAction::PartPresetNext);
                        forge_button(row, "ASSIGN SELECTION", ImportedForgeAction::AssignPart);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "NEXT ASSIGNMENT", ImportedForgeAction::AssignmentNext);
                        forge_button(row, "REMOVE", ImportedForgeAction::RemoveAssignment);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "SAVE TO LIBRARY", ImportedForgeAction::SavePart);
                        forge_button(row, "ADD LIBRARY PART", ImportedForgeAction::LoadPart);
                    });
                    forge_row(panel, |row| {
                        forge_button(
                            row,
                            "ASSEMBLE RUNTIME PREVIEW",
                            ImportedForgeAction::PreviewAssembly,
                        );
                    });
                },
            );

            spawn_tool_window(
                root,
                "COLOR + TEXTURE",
                Vec2::new(756.0, 430.0),
                ToolWindowStyle {
                    accent: Color::srgb(0.05, 0.78, 0.62),
                    width: 350.0,
                    content_height: Val::Px(290.0),
                    ..default()
                },
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        ImportedMaterialText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.75, 1.0, 0.92)),
                    ));
                    forge_row(panel, |row| {
                        forge_button(row, "NEXT COLOR", ImportedForgeAction::MaterialColorNext);
                        forge_button(row, "PARAM", ImportedForgeAction::MaterialParamNext);
                        forge_button(row, "−", ImportedForgeAction::AdjustMaterial(-1));
                        forge_button(row, "+", ImportedForgeAction::AdjustMaterial(1));
                    });
                    forge_row(panel, |row| {
                        forge_button(
                            row,
                            "TEXTURE CHANNEL",
                            ImportedForgeAction::TextureChannelNext,
                        );
                        forge_button(row, "CLEAR MAP", ImportedForgeAction::ClearTexture);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "RESTORE AUTHORED", ImportedForgeAction::ResetMaterial);
                    });
                    panel.spawn((
                        Text::new("Drop PNG/JPG to assign the armed texture channel."),
                        TextFont {
                            font_size: FontSize::Px(11.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.68, 0.78, 0.74)),
                    ));
                },
            );

            spawn_tool_window(
                root,
                "UV + FACE PAINT",
                Vec2::new(1120.0, 86.0),
                ToolWindowStyle {
                    accent: Color::srgb(0.72, 0.36, 0.95),
                    width: 330.0,
                    content_height: Val::Px(310.0),
                    ..default()
                },
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        ImportedUvText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.94, 0.82, 1.0)),
                    ));
                    forge_row(panel, |row| {
                        forge_button(
                            row,
                            "PROJECTION",
                            ImportedForgeAction::UvProjectionNext,
                        );
                        forge_button(row, "UV PARAM", ImportedForgeAction::UvParamNext);
                        forge_button(row, "−", ImportedForgeAction::AdjustUv(-1));
                        forge_button(row, "+", ImportedForgeAction::AdjustUv(1));
                    });
                    forge_row(panel, |row| {
                        forge_button(
                            row,
                            "PAINT COLOR",
                            ImportedForgeAction::PaintColorNext,
                        );
                        forge_button(
                            row,
                            "PAINT SELECTION",
                            ImportedForgeAction::PaintSelection,
                        );
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "MARK BOUNDARY SEAM", ImportedForgeAction::MarkSeams);
                        forge_button(row, "CLEAR SEAMS", ImportedForgeAction::ClearSeams);
                    });
                    forge_row(panel, |row| {
                        forge_button(
                            row,
                            "BAKE PAINT PNG",
                            ImportedForgeAction::BakePaintTexture,
                        );
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "CLEAR PAINT", ImportedForgeAction::ClearPaint);
                        forge_button(row, "RESTORE UV", ImportedForgeAction::ResetUv);
                    });
                    panel.spawn((
                        Text::new(
                            "BOX projection creates hard seams. Face paint uses the active mesh selection.",
                        ),
                        TextFont {
                            font_size: FontSize::Px(11.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.75, 0.68, 0.8)),
                    ));
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

fn import_texture_into_assets(source: &Path) -> Result<String, String> {
    if !source.is_file() {
        return Err(format!("{} is not a file", source.display()));
    }
    let extension = source
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|ext| matches!(ext.as_str(), "png" | "jpg" | "jpeg"))
        .ok_or_else(|| "Texture maps must be PNG or JPG files".to_string())?;
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let source = source
        .canonicalize()
        .map_err(|error| format!("Could not resolve texture: {error}"))?;
    if let Ok(relative) = source.strip_prefix(
        assets
            .canonicalize()
            .map_err(|error| format!("Could not resolve assets folder: {error}"))?,
    ) {
        return Ok(relative.to_string_lossy().replace('\\', "/"));
    }
    let import_dir = assets.join("imported_textures");
    std::fs::create_dir_all(&import_dir)
        .map_err(|error| format!("Could not create texture import folder: {error}"))?;
    let stem = sanitize_file_stem(&source);
    let target = (1_u32..)
        .map(|suffix| {
            if suffix == 1 {
                import_dir.join(format!("{stem}.{extension}"))
            } else {
                import_dir.join(format!("{stem}_{suffix}.{extension}"))
            }
        })
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| "Could not allocate an imported texture filename".to_string())?;
    std::fs::copy(&source, &target)
        .map_err(|error| format!("Could not copy texture into assets: {error}"))?;
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
        state.spec.material_edits.clear();
        state.spec.uv_edits.clear();
        state.spec.part_assignments.clear();
    }
    state.selected_primitive = 0;
    state.face_cursor = 0;
    state.selected_faces.clear();
    state.mesh_names.clear();
    state.gltf = Some(asset_server.load(asset_path.clone()));
    state.preview_dirty = false;
    state.labels_dirty = true;
    state.status = format!("Loading and inspecting {asset_path}…");
}

fn selected_topology(
    state: &ImportedForgeState,
    gltfs: &Assets<Gltf>,
    gltf_meshes: &Assets<GltfMesh>,
    meshes: &Assets<Mesh>,
) -> Option<MeshTopology> {
    let gltf = gltfs.get(state.gltf.as_ref()?)?;
    let gltf_mesh = gltf_meshes.get(gltf.meshes.get(state.spec.selected_mesh)?)?;
    let primitive = gltf_mesh.primitives.get(state.selected_primitive)?;
    MeshTopology::from_mesh(meshes.get(&primitive.mesh)?)
}

fn selected_source_mesh_handle(
    state: &ImportedForgeState,
    gltfs: &Assets<Gltf>,
    gltf_meshes: &Assets<GltfMesh>,
) -> Option<Handle<Mesh>> {
    let gltf = gltfs.get(state.gltf.as_ref()?)?;
    let gltf_mesh = gltf_meshes.get(gltf.meshes.get(state.spec.selected_mesh)?)?;
    Some(
        gltf_mesh
            .primitives
            .get(state.selected_primitive)?
            .mesh
            .clone(),
    )
}

fn write_paint_texture(
    source_asset: &str,
    mesh_index: usize,
    primitive_index: usize,
    pixels: &[u8],
    size: u32,
) -> Result<String, String> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let output_dir = assets.join("imported_textures");
    std::fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Could not create texture output folder: {error}"))?;
    let stem = sanitize_file_stem(Path::new(source_asset));
    let base = format!("{stem}_m{mesh_index}_p{primitive_index}_paint");
    let target = (1_u32..)
        .map(|version| output_dir.join(format!("{base}_v{version:03}.png")))
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| "Could not allocate a paint texture version".to_string())?;
    image::save_buffer_with_format(
        &target,
        pixels,
        size,
        size,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|error| format!("Could not encode paint texture: {error}"))?;
    Ok(target
        .strip_prefix(&assets)
        .unwrap_or(&target)
        .to_string_lossy()
        .replace('\\', "/"))
}

fn primitive_count(
    state: &ImportedForgeState,
    gltfs: &Assets<Gltf>,
    gltf_meshes: &Assets<GltfMesh>,
) -> usize {
    state
        .gltf
        .as_ref()
        .and_then(|handle| gltfs.get(handle))
        .and_then(|gltf| gltf.meshes.get(state.spec.selected_mesh))
        .and_then(|handle| gltf_meshes.get(handle))
        .map(|mesh| mesh.primitives.len())
        .unwrap_or(0)
}

fn unique_part_id(spec: &ImportedCharacterSpec, base: &str) -> String {
    let base = base.to_ascii_lowercase().replace(' ', "_");
    (1_u32..)
        .map(|suffix| {
            if suffix == 1 {
                format!("part.{base}")
            } else {
                format!("part.{base}.{suffix}")
            }
        })
        .find(|candidate| {
            !spec
                .part_assignments
                .iter()
                .any(|part| part.part_id == *candidate)
        })
        .unwrap_or_else(|| "part.overflow".into())
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
        let is_glb = path_buf
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("glb"));
        if is_glb {
            match import_glb_into_assets(path_buf) {
                Ok(asset_path) => begin_import(asset_path, &asset_server, &mut state, true),
                Err(error) => state.status = error,
            }
        } else {
            match import_texture_into_assets(path_buf) {
                Ok(texture_path) => {
                    let channel = ImportedTextureChannel::ALL[state.texture_channel];
                    let edit = state.selected_material_edit_mut();
                    match channel {
                        ImportedTextureChannel::BaseColor => {
                            edit.base_color_texture = Some(texture_path.clone());
                            edit.cleared_texture_channels[0] = false;
                        }
                        ImportedTextureChannel::Normal => {
                            edit.normal_texture = Some(texture_path.clone());
                            edit.cleared_texture_channels[1] = false;
                        }
                        ImportedTextureChannel::MetallicRoughness => {
                            edit.metallic_roughness_texture = Some(texture_path.clone());
                            edit.cleared_texture_channels[2] = false;
                            edit.metallic = 1.0;
                            edit.perceptual_roughness = 1.0;
                        }
                        ImportedTextureChannel::Emissive => {
                            edit.emissive_texture = Some(texture_path.clone());
                            edit.cleared_texture_channels[3] = false;
                            edit.emissive_strength = edit.emissive_strength.max(1.0);
                        }
                    }
                    state.status = format!("Assigned {texture_path} to {}", channel.label());
                    state.preview_dirty = true;
                }
                Err(error) => state.status = error,
            }
        }
        state.labels_dirty = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn imported_forge_actions(
    mut commands: Commands,
    buttons: Query<(&Interaction, &ImportedForgeButton), (Changed<Interaction>, With<Button>)>,
    asset_server: Res<AssetServer>,
    registry: Res<ForgeProjectRegistry>,
    return_target: Res<ImportedForgeReturnTarget>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
    runtime_previews: Query<Entity, With<ImportedRuntimeTestRoot>>,
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
                    state.selected_primitive = 0;
                    state.face_cursor = 0;
                    state.selected_faces.clear();
                    state.preview_dirty = true;
                    state.labels_dirty = true;
                }
            }
            ImportedForgeAction::PrimitiveNext => {
                let count = primitive_count(&state, &gltfs, &gltf_meshes);
                if count > 0 {
                    state.selected_primitive = (state.selected_primitive + 1) % count;
                    state.face_cursor = 0;
                    state.selected_faces.clear();
                    state.preview_dirty = true;
                    state.labels_dirty = true;
                }
            }
            ImportedForgeAction::FacePrevious | ImportedForgeAction::FaceNext => {
                if let Some(topology) = selected_topology(&state, &gltfs, &gltf_meshes, &meshes) {
                    let count = topology.face_count() as u32;
                    if count > 0 {
                        state.face_cursor = match button.0 {
                            ImportedForgeAction::FacePrevious => {
                                state.face_cursor.checked_sub(1).unwrap_or(count - 1)
                            }
                            _ => (state.face_cursor + 1) % count,
                        };
                        state.labels_dirty = true;
                    }
                }
            }
            ImportedForgeAction::ToggleFace => {
                if let Some(topology) = selected_topology(&state, &gltfs, &gltf_meshes, &meshes) {
                    let face_cursor = state.face_cursor;
                    if (face_cursor as usize) < topology.face_count()
                        && !state.selected_faces.remove(&face_cursor)
                    {
                        state.selected_faces.insert(face_cursor);
                    }
                    state.preview_dirty = true;
                    state.labels_dirty = true;
                }
            }
            ImportedForgeAction::SelectAll
            | ImportedForgeAction::SelectNone
            | ImportedForgeAction::SelectInvert
            | ImportedForgeAction::SelectLinked
            | ImportedForgeAction::SelectGrow
            | ImportedForgeAction::SelectShrink
            | ImportedForgeAction::SelectSimilar
            | ImportedForgeAction::SelectMirror => {
                if let Some(topology) = selected_topology(&state, &gltfs, &gltf_meshes, &meshes) {
                    state.selected_faces = match button.0 {
                        ImportedForgeAction::SelectAll => {
                            (0..topology.face_count() as u32).collect()
                        }
                        ImportedForgeAction::SelectNone => BTreeSet::new(),
                        ImportedForgeAction::SelectInvert => topology.invert(&state.selected_faces),
                        ImportedForgeAction::SelectLinked => topology.linked(state.face_cursor),
                        ImportedForgeAction::SelectGrow => topology.grow(&state.selected_faces),
                        ImportedForgeAction::SelectShrink => topology.shrink(&state.selected_faces),
                        ImportedForgeAction::SelectSimilar => {
                            topology.similar_normal(state.face_cursor, 0.94)
                        }
                        ImportedForgeAction::SelectMirror => {
                            topology.mirrored_x(&state.selected_faces)
                        }
                        _ => unreachable!(),
                    };
                    state.preview_dirty = true;
                    state.labels_dirty = true;
                } else {
                    state.status = "Selection requires a triangle-list mesh with positions".into();
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
            ImportedForgeAction::PartPresetNext => {
                state.part_preset = (state.part_preset + 1) % ImportedPartPreset::ALL.len();
                state.labels_dirty = true;
            }
            ImportedForgeAction::AssignPart => {
                let Some(topology) = selected_topology(&state, &gltfs, &gltf_meshes, &meshes)
                else {
                    state.status =
                        "The selected primitive has no editable triangle topology".into();
                    state.labels_dirty = true;
                    continue;
                };
                let selected = state
                    .selected_faces
                    .iter()
                    .copied()
                    .filter(|face| {
                        !state.spec.part_assignments.iter().any(|part| {
                            part.source_asset == state.spec.source_asset
                                && part.mesh_index == state.spec.selected_mesh
                                && part.primitive_index == state.selected_primitive
                                && part.face_indices.contains(face)
                        })
                    })
                    .collect::<Vec<_>>();
                if selected.is_empty() {
                    state.status = "Select unassigned faces before creating a modular part".into();
                    state.labels_dirty = true;
                    continue;
                }
                let preset = ImportedPartPreset::ALL[state.part_preset];
                let pivot = selected
                    .iter()
                    .filter_map(|face| topology.centers.get(*face as usize))
                    .copied()
                    .fold(Vec3::ZERO, |sum, center| sum + center)
                    / selected.len() as f32;
                let part_id = unique_part_id(&state.spec, preset.label);
                let source_asset = state.spec.source_asset.clone();
                let mesh_index = state.spec.selected_mesh;
                let primitive_index = state.selected_primitive;
                state.spec.part_assignments.push(ImportedPartAssignment {
                    part_id,
                    display_name: preset.label.into(),
                    source_asset,
                    mesh_index,
                    primitive_index,
                    face_indices: selected,
                    slot: preset.slot,
                    joint: preset.joint,
                    gameplay_role: preset.role,
                    pivot: pivot.to_array(),
                });
                state.selected_assignment = state.spec.part_assignments.len() - 1;
                state.status = format!("Assigned selection as {}", preset.label);
                state.labels_dirty = true;
            }
            ImportedForgeAction::AssignmentNext => {
                if !state.spec.part_assignments.is_empty() {
                    state.selected_assignment =
                        (state.selected_assignment + 1) % state.spec.part_assignments.len();
                    state.labels_dirty = true;
                }
            }
            ImportedForgeAction::RemoveAssignment => {
                if state.selected_assignment < state.spec.part_assignments.len() {
                    let selected_assignment = state.selected_assignment;
                    let removed = state.spec.part_assignments.remove(selected_assignment);
                    state.selected_assignment = state
                        .selected_assignment
                        .min(state.spec.part_assignments.len().saturating_sub(1));
                    state.status = format!("Removed modular assignment {}", removed.part_id);
                    state.labels_dirty = true;
                }
            }
            ImportedForgeAction::SavePart => {
                if let Some(part) = state
                    .spec
                    .part_assignments
                    .get(state.selected_assignment)
                    .cloned()
                {
                    let material = state.spec.material_edits.iter().find(|material| {
                        material.source_asset == part.source_asset
                            && material.mesh_index == part.mesh_index
                            && material.primitive_index == part.primitive_index
                    });
                    let uv_edit = state.spec.uv_edits.iter().find(|edit| {
                        edit.source_asset == part.source_asset
                            && edit.mesh_index == part.mesh_index
                            && edit.primitive_index == part.primitive_index
                    });
                    match character_records::save_part_to_active_project(
                        &registry, &part, material, uv_edit,
                    ) {
                        Ok(content_id) => {
                            state.status = format!("Saved reusable part {content_id}");
                            state.part_library = character_records::list_part_library(&registry);
                        }
                        Err(error) => state.status = error,
                    }
                } else {
                    state.status = "Create or select a part assignment first".into();
                }
                state.labels_dirty = true;
            }
            ImportedForgeAction::LoadPart => {
                if state.part_library.is_empty() {
                    state.status = "The active project has no reusable modular parts".into();
                } else {
                    let content_id = state.part_library[state.part_library_cursor].0.clone();
                    state.part_library_cursor =
                        (state.part_library_cursor + 1) % state.part_library.len();
                    match character_records::load_part_from_active_project(&registry, &content_id) {
                        Ok(part) => {
                            let mut assignment = part.assignment;
                            assignment.part_id =
                                unique_part_id(&state.spec, &assignment.display_name);
                            let overlaps = state.spec.part_assignments.iter().any(|existing| {
                                existing.source_asset == assignment.source_asset
                                    && existing.mesh_index == assignment.mesh_index
                                    && existing.primitive_index == assignment.primitive_index
                                    && existing
                                        .face_indices
                                        .iter()
                                        .any(|face| assignment.face_indices.contains(face))
                            });
                            if overlaps {
                                state.status =
                                    "Library part overlaps an existing assignment".into();
                            } else {
                                if let Some(material) = part.material {
                                    state.spec.material_edits.retain(|existing| {
                                        existing.source_asset != material.source_asset
                                            || existing.mesh_index != material.mesh_index
                                            || existing.primitive_index != material.primitive_index
                                    });
                                    state.spec.material_edits.push(material);
                                }
                                if let Some(uv_edit) = part.uv_edit {
                                    state.spec.uv_edits.retain(|existing| {
                                        existing.source_asset != uv_edit.source_asset
                                            || existing.mesh_index != uv_edit.mesh_index
                                            || existing.primitive_index != uv_edit.primitive_index
                                    });
                                    state.spec.uv_edits.push(uv_edit);
                                }
                                state.spec.part_assignments.push(assignment);
                                state.selected_assignment = state.spec.part_assignments.len() - 1;
                                state.status = format!("Added reusable part {content_id}");
                            }
                        }
                        Err(error) => state.status = error,
                    }
                }
                state.labels_dirty = true;
            }
            ImportedForgeAction::PreviewAssembly => {
                for entity in &runtime_previews {
                    commands.entity(entity).try_despawn();
                }
                if state.spec.part_assignments.is_empty() {
                    state.status = "Assign or add modular parts before runtime assembly".into();
                } else {
                    let root = commands
                        .spawn((
                            ImportedForgeRoot,
                            ImportedRuntimeTestRoot,
                            Transform::from_xyz(2.6, 0.0, 0.0),
                            GlobalTransform::default(),
                            Visibility::default(),
                            InheritedVisibility::default(),
                        ))
                        .id();
                    request_imported_character(&mut commands, root, state.spec.clone());
                    state.status =
                        "Runtime assembly requested at the right side of the preview".into();
                }
                state.labels_dirty = true;
            }
            ImportedForgeAction::MaterialColorNext => {
                let next = state
                    .selected_material_edit()
                    .and_then(|edit| {
                        IMPORTED_COLOR_PALETTE
                            .iter()
                            .position(|color| *color == edit.base_color)
                    })
                    .map_or(0, |index| (index + 1) % IMPORTED_COLOR_PALETTE.len());
                state.selected_material_edit_mut().base_color = IMPORTED_COLOR_PALETTE[next];
                state.preview_dirty = true;
                state.labels_dirty = true;
            }
            ImportedForgeAction::MaterialParamNext => {
                state.material_param = (state.material_param + 1) % 3;
                state.labels_dirty = true;
            }
            ImportedForgeAction::AdjustMaterial(direction) => {
                let param = state.material_param;
                let edit = state.selected_material_edit_mut();
                match param {
                    0 => edit.metallic = (edit.metallic + direction as f32 * 0.05).clamp(0.0, 1.0),
                    1 => {
                        edit.perceptual_roughness =
                            (edit.perceptual_roughness + direction as f32 * 0.05).clamp(0.089, 1.0)
                    }
                    _ => {
                        edit.emissive_strength =
                            (edit.emissive_strength + direction as f32 * 0.5).clamp(0.0, 100.0)
                    }
                }
                state.preview_dirty = true;
                state.labels_dirty = true;
            }
            ImportedForgeAction::TextureChannelNext => {
                state.texture_channel =
                    (state.texture_channel + 1) % ImportedTextureChannel::ALL.len();
                state.labels_dirty = true;
            }
            ImportedForgeAction::ClearTexture => {
                let channel = ImportedTextureChannel::ALL[state.texture_channel];
                let edit = state.selected_material_edit_mut();
                match channel {
                    ImportedTextureChannel::BaseColor => {
                        edit.base_color_texture = None;
                        edit.cleared_texture_channels[0] = true;
                    }
                    ImportedTextureChannel::Normal => {
                        edit.normal_texture = None;
                        edit.cleared_texture_channels[1] = true;
                    }
                    ImportedTextureChannel::MetallicRoughness => {
                        edit.metallic_roughness_texture = None;
                        edit.cleared_texture_channels[2] = true;
                    }
                    ImportedTextureChannel::Emissive => {
                        edit.emissive_texture = None;
                        edit.cleared_texture_channels[3] = true;
                    }
                }
                state.status = format!("Cleared {} texture", channel.label());
                state.preview_dirty = true;
                state.labels_dirty = true;
            }
            ImportedForgeAction::ResetMaterial => {
                let source_asset = state.spec.source_asset.clone();
                let mesh_index = state.spec.selected_mesh;
                let primitive_index = state.selected_primitive;
                state.spec.material_edits.retain(|edit| {
                    edit.source_asset != source_asset
                        || edit.mesh_index != mesh_index
                        || edit.primitive_index != primitive_index
                });
                state.status = "Restored the primitive's authored GLB material".into();
                state.preview_dirty = true;
                state.labels_dirty = true;
            }
            ImportedForgeAction::UvProjectionNext => {
                let current = state
                    .selected_uv_edit()
                    .map(|edit| edit.projection)
                    .unwrap_or(ImportedUvProjection::Authored);
                let next = IMPORTED_UV_PROJECTIONS
                    .iter()
                    .position(|projection| *projection == current)
                    .map_or(0, |index| (index + 1) % IMPORTED_UV_PROJECTIONS.len());
                state.selected_uv_edit_mut().projection = IMPORTED_UV_PROJECTIONS[next];
                state.preview_dirty = true;
                state.labels_dirty = true;
            }
            ImportedForgeAction::UvParamNext => {
                state.uv_param = (state.uv_param + 1) % 5;
                state.labels_dirty = true;
            }
            ImportedForgeAction::AdjustUv(direction) => {
                let param = state.uv_param;
                let edit = state.selected_uv_edit_mut();
                match param {
                    0 => {
                        edit.scale[0] = (edit.scale[0] + direction as f32 * 0.05).clamp(0.01, 100.0)
                    }
                    1 => {
                        edit.scale[1] = (edit.scale[1] + direction as f32 * 0.05).clamp(0.01, 100.0)
                    }
                    2 => {
                        edit.offset[0] =
                            (edit.offset[0] + direction as f32 * 0.05).clamp(-100.0, 100.0)
                    }
                    3 => {
                        edit.offset[1] =
                            (edit.offset[1] + direction as f32 * 0.05).clamp(-100.0, 100.0)
                    }
                    _ => {
                        edit.rotation_radians = (edit.rotation_radians
                            + direction as f32 * 5.0_f32.to_radians())
                        .clamp(-std::f32::consts::TAU, std::f32::consts::TAU)
                    }
                }
                state.preview_dirty = true;
                state.labels_dirty = true;
            }
            ImportedForgeAction::PaintColorNext => {
                state.paint_color = (state.paint_color + 1) % IMPORTED_COLOR_PALETTE.len();
                state.labels_dirty = true;
            }
            ImportedForgeAction::PaintSelection => {
                if state.selected_faces.is_empty() {
                    state.status = "Select faces before applying face paint".into();
                } else {
                    let color = IMPORTED_COLOR_PALETTE[state.paint_color];
                    let faces = state.selected_faces.iter().copied().collect();
                    let edit = state.selected_uv_edit_mut();
                    if edit.face_paints.len() >= 64 {
                        edit.face_paints.remove(0);
                    }
                    let previous_bake = edit.baked_paint_texture.take();
                    edit.face_paints.push(ImportedFacePaint {
                        face_indices: faces,
                        color,
                    });
                    if let Some(previous_bake) = previous_bake {
                        let material = state.selected_material_edit_mut();
                        if material.base_color_texture.as_ref() == Some(&previous_bake) {
                            material.base_color_texture = None;
                        }
                    }
                    state.status = "Painted the selected source faces".into();
                    state.preview_dirty = true;
                }
                state.labels_dirty = true;
            }
            ImportedForgeAction::MarkSeams => {
                if state.selected_faces.is_empty() {
                    state.status = "Select a face region before marking its boundary seam".into();
                } else if let Some(topology) =
                    selected_topology(&state, &gltfs, &gltf_meshes, &meshes)
                {
                    let boundary = selection_boundary_edges(&topology, &state.selected_faces);
                    let edit = state.selected_uv_edit_mut();
                    edit.seam_edges.extend(boundary);
                    edit.seam_edges.sort_unstable();
                    edit.seam_edges.dedup();
                    edit.projection = ImportedUvProjection::SeamUnwrap;
                    state.status =
                        "Marked selection boundary and packed seam-separated UV islands".into();
                    state.preview_dirty = true;
                }
                state.labels_dirty = true;
            }
            ImportedForgeAction::ClearSeams => {
                let edit = state.selected_uv_edit_mut();
                edit.seam_edges.clear();
                if edit.projection == ImportedUvProjection::SeamUnwrap {
                    edit.projection = ImportedUvProjection::Authored;
                }
                state.status = "Cleared manual UV seams".into();
                state.preview_dirty = true;
                state.labels_dirty = true;
            }
            ImportedForgeAction::BakePaintTexture => {
                let uv_edit = state.selected_uv_edit().cloned();
                let source_handle = selected_source_mesh_handle(&state, &gltfs, &gltf_meshes);
                match (uv_edit, source_handle) {
                    (Some(edit), Some(handle)) if !edit.face_paints.is_empty() => {
                        let result = meshes
                            .get(&handle)
                            .and_then(|source| rasterize_face_paint(source, &edit, 256))
                            .ok_or_else(|| "Could not rasterize face paint".to_string())
                            .and_then(|pixels| {
                                write_paint_texture(
                                    &state.spec.source_asset,
                                    state.spec.selected_mesh,
                                    state.selected_primitive,
                                    &pixels,
                                    256,
                                )
                            });
                        match result {
                            Ok(path) => {
                                state.selected_uv_edit_mut().baked_paint_texture =
                                    Some(path.clone());
                                let material = state.selected_material_edit_mut();
                                material.base_color = [1.0; 4];
                                material.base_color_texture = Some(path.clone());
                                material.cleared_texture_channels[0] = false;
                                state.status = format!("Baked face paint to {path}");
                                state.preview_dirty = true;
                            }
                            Err(error) => state.status = error,
                        }
                    }
                    _ => state.status = "Add face paint before baking a PNG texture".into(),
                }
                state.labels_dirty = true;
            }
            ImportedForgeAction::ClearPaint => {
                let previous_bake = {
                    let edit = state.selected_uv_edit_mut();
                    edit.face_paints.clear();
                    edit.baked_paint_texture.take()
                };
                if let Some(previous_bake) = previous_bake {
                    let material = state.selected_material_edit_mut();
                    if material.base_color_texture.as_ref() == Some(&previous_bake) {
                        material.base_color_texture = None;
                    }
                }
                state.status = "Cleared face paint on the active primitive".into();
                state.preview_dirty = true;
                state.labels_dirty = true;
            }
            ImportedForgeAction::ResetUv => {
                let source_asset = state.spec.source_asset.clone();
                let mesh_index = state.spec.selected_mesh;
                let primitive_index = state.selected_primitive;
                let baked = state
                    .selected_uv_edit()
                    .and_then(|edit| edit.baked_paint_texture.clone());
                state.spec.uv_edits.retain(|edit| {
                    edit.source_asset != source_asset
                        || edit.mesh_index != mesh_index
                        || edit.primitive_index != primitive_index
                });
                if let Some(baked) = baked {
                    if let Some(material) = state.spec.material_edits.iter_mut().find(|material| {
                        material.source_asset == source_asset
                            && material.mesh_index == mesh_index
                            && material.primitive_index == primitive_index
                    }) {
                        if material.base_color_texture.as_ref() == Some(&baked) {
                            material.base_color_texture = None;
                        }
                    }
                }
                state.status = "Restored authored UVs and cleared face paint".into();
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
                state.spec.part_assignments.clear();
                state.spec.material_edits.clear();
                state.spec.uv_edits.clear();
                state.selected_faces.clear();
                state.selected_modifier = 0;
                state.selected_param = 0;
                state.preview_dirty = true;
                state.labels_dirty = true;
                state.status = "Non-destructive edits reset; source GLB unchanged".into();
            }
            ImportedForgeAction::Back => next_state.set(match *return_target {
                ImportedForgeReturnTarget::ProjectHub => AppState::ProjectHub,
                ImportedForgeReturnTarget::PlayerSelect => AppState::PlayerSelect,
            }),
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
    state.selected_primitive = 0;
    state.face_cursor = 0;
    state.selected_faces.clear();
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

/// Build a display-only copy that can be rendered by a plain `PbrBundle`.
///
/// Imported skinned and morphed meshes require companion ECS components that
/// belong to the source GLTF scene. The Forge preview intentionally rebuilds
/// that scene as independently selectable mesh parts, so reusing a deformable
/// source mesh would make Bevy select a skinned/morphed pipeline while binding
/// only the model transform. Keep the authored asset intact and omit deformation
/// data from this static bind-pose preview.
fn static_import_preview_mesh(source: &Mesh) -> Mesh {
    static_mesh_copy(source)
}

#[allow(clippy::too_many_arguments)]
fn pick_imported_face(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<ImportedForgeCamera>>,
    preview_parts: Query<(&ImportedPreviewSource, &GlobalTransform)>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
    mut state: ResMut<ImportedForgeState>,
) {
    let alt = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
    if !alt || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
        return;
    };
    let Some(gltf) = state.gltf.as_ref().and_then(|handle| gltfs.get(handle)) else {
        return;
    };
    let Some(gltf_mesh) = gltf
        .meshes
        .get(state.spec.selected_mesh)
        .and_then(|handle| gltf_meshes.get(handle))
    else {
        return;
    };
    let Some(source) = gltf_mesh
        .primitives
        .get(state.selected_primitive)
        .and_then(|primitive| meshes.get(&primitive.mesh))
    else {
        return;
    };

    let mut nearest = None;
    for (preview_source, transform) in &preview_parts {
        if preview_source.mesh_index != state.spec.selected_mesh
            || preview_source.primitive_index != state.selected_primitive
        {
            continue;
        }
        let Some((face, distance)) = pick_face(
            source,
            transform.affine(),
            ray.origin,
            Vec3::from(ray.direction),
        ) else {
            continue;
        };
        if nearest.is_none_or(|(_, nearest_distance)| distance < nearest_distance) {
            nearest = Some((face, distance));
        }
    }
    let Some((face, _)) = nearest else {
        state.status = "No face hit on the active mesh primitive".into();
        state.labels_dirty = true;
        return;
    };
    state.face_cursor = face;
    if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
        if let Some(topology) = MeshTopology::from_mesh(source) {
            state.selected_faces.extend(topology.linked(face));
        }
    } else {
        state.selected_faces = BTreeSet::from([face]);
    }
    state.status = format!("Picked source face {}", face + 1);
    state.preview_dirty = true;
    state.labels_dirty = true;
}

fn standard_material_from_gltf(source: Option<&GltfMaterial>) -> StandardMaterial {
    let Some(source) = source else {
        return StandardMaterial::default();
    };
    StandardMaterial {
        base_color: source.base_color,
        base_color_channel: source.base_color_channel.clone(),
        base_color_texture: source.base_color_texture.clone(),
        emissive: source.emissive,
        emissive_channel: source.emissive_channel.clone(),
        emissive_texture: source.emissive_texture.clone(),
        perceptual_roughness: source.perceptual_roughness,
        metallic: source.metallic,
        metallic_roughness_channel: source.metallic_roughness_channel.clone(),
        metallic_roughness_texture: source.metallic_roughness_texture.clone(),
        reflectance: source.reflectance,
        specular_tint: source.specular_tint,
        normal_map_channel: source.normal_map_channel.clone(),
        normal_map_texture: source.normal_map_texture.clone(),
        double_sided: source.double_sided,
        cull_mode: source.cull_mode,
        alpha_mode: source.alpha_mode,
        ..default()
    }
}

fn apply_material_edit(
    material: &mut StandardMaterial,
    edit: &ImportedMaterialEdit,
    asset_server: &AssetServer,
) {
    material.base_color = Color::srgba(
        edit.base_color[0],
        edit.base_color[1],
        edit.base_color[2],
        edit.base_color[3],
    );
    material.metallic = edit.metallic;
    material.perceptual_roughness = edit.perceptual_roughness;
    material.emissive = LinearRgba::rgb(
        edit.emissive[0] * edit.emissive_strength,
        edit.emissive[1] * edit.emissive_strength,
        edit.emissive[2] * edit.emissive_strength,
    );
    if edit.cleared_texture_channels[0] {
        material.base_color_texture = None;
    } else if let Some(path) = &edit.base_color_texture {
        material.base_color_texture = Some(asset_server.load(path.clone()));
    }
    if edit.cleared_texture_channels[1] {
        material.normal_map_texture = None;
    } else if let Some(path) = &edit.normal_texture {
        material.normal_map_texture = Some(asset_server.load(path.clone()));
    }
    if edit.cleared_texture_channels[2] {
        material.metallic_roughness_texture = None;
    } else if let Some(path) = &edit.metallic_roughness_texture {
        material.metallic_roughness_texture = Some(asset_server.load(path.clone()));
    }
    if edit.cleared_texture_channels[3] {
        material.emissive_texture = None;
    } else if let Some(path) = &edit.emissive_texture {
        material.emissive_texture = Some(asset_server.load(path.clone()));
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild_imported_preview(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
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
    #[derive(Clone, Copy)]
    enum PreviewTint {
        Normal,
        ActiveMesh,
        SelectedFaces,
    }
    for (mesh_index, transform) in instances {
        let gltf_mesh_handle = &gltf.meshes[mesh_index];
        let Some(gltf_mesh) = gltf_meshes.get(gltf_mesh_handle) else {
            return;
        };
        for (primitive_index, primitive) in gltf_mesh.primitives.iter().enumerate() {
            let Some(authored_source) = meshes.get(&primitive.mesh) else {
                return;
            };
            let selected = mesh_index == state.spec.selected_mesh;
            let active_primitive = selected && primitive_index == state.selected_primitive;
            let mut preview_material = standard_material_from_gltf(
                primitive
                    .material
                    .as_ref()
                    .and_then(|handle| gltf_materials.get(handle)),
            );
            if let Some(edit) = state.spec.material_edits.iter().find(|edit| {
                edit.source_asset == state.spec.source_asset
                    && edit.mesh_index == mesh_index
                    && edit.primitive_index == primitive_index
            }) {
                apply_material_edit(&mut preview_material, edit, &asset_server);
            }
            let uv_mesh = state
                .spec
                .uv_edits
                .iter()
                .find(|edit| {
                    edit.source_asset == state.spec.source_asset
                        && edit.mesh_index == mesh_index
                        && edit.primitive_index == primitive_index
                })
                .and_then(|edit| apply_uv_edit(authored_source, edit));
            let source = uv_mesh.as_ref().unwrap_or(authored_source);
            let skinned = authored_source
                .attribute(Mesh::ATTRIBUTE_JOINT_INDEX)
                .is_some();
            let deformable = skinned || authored_source.has_morph_targets();
            if active_primitive && !state.selected_faces.is_empty() {
                let Some(topology) = MeshTopology::from_mesh(source) else {
                    state.status = format!("{} is not an editable triangle mesh", gltf_mesh.name);
                    return;
                };
                let remainder = topology.invert(&state.selected_faces);
                let base = selected_face_mesh(source, &remainder);
                let overlay = selected_face_mesh(source, &state.selected_faces);
                if let Some(base) = base {
                    parts.push((
                        meshes.add(base),
                        PreviewTint::ActiveMesh,
                        transform,
                        mesh_index,
                        primitive_index,
                        preview_material.clone(),
                    ));
                }
                if let Some(overlay) = overlay {
                    parts.push((
                        meshes.add(overlay),
                        PreviewTint::SelectedFaces,
                        transform,
                        mesh_index,
                        primitive_index,
                        preview_material,
                    ));
                }
                continue;
            }
            let handle = if deformable {
                if selected && !state.selected_stack().is_empty() {
                    selected_is_deformable = true;
                }
                let preview =
                    uv_mesh.unwrap_or_else(|| static_import_preview_mesh(authored_source));
                meshes.add(preview)
            } else if selected && !state.selected_stack().is_empty() {
                let Some(derived) = apply_stack_to_mesh(source, state.selected_stack()) else {
                    state.status = format!("{} is not an editable triangle mesh", gltf_mesh.name);
                    return;
                };
                meshes.add(derived)
            } else if let Some(uv_mesh) = uv_mesh {
                meshes.add(uv_mesh)
            } else {
                primitive.mesh.clone()
            };
            parts.push((
                handle,
                if selected {
                    PreviewTint::ActiveMesh
                } else {
                    PreviewTint::Normal
                },
                transform,
                mesh_index,
                primitive_index,
                preview_material,
            ));
        }
    }

    for entity in &old_previews {
        commands.entity(entity).despawn();
    }
    let face_material = state.face_material.clone().unwrap_or_else(|| {
        let handle = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.22, 0.04),
            emissive: LinearRgba::rgb(0.65, 0.06, 0.01),
            metallic: 0.05,
            perceptual_roughness: 0.32,
            ..default()
        });
        state.face_material = Some(handle.clone());
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
    for (mesh, tint, transform, mesh_index, primitive_index, material) in parts {
        let material = match tint {
            PreviewTint::SelectedFaces => face_material.clone(),
            PreviewTint::Normal | PreviewTint::ActiveMesh => materials.add(material),
        };
        let child = commands
            .spawn((
                ImportedPreviewPart,
                ImportedPreviewSource {
                    mesh_index,
                    primitive_index,
                },
                PbrBundle {
                    mesh: Mesh3d(mesh),
                    material: MeshMaterial3d(material),
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
    mut labels: Query<(
        &mut Text,
        Option<&ImportedStatusText>,
        Option<&ImportedAssetText>,
        Option<&ImportedMeshText>,
        Option<&ImportedModifierText>,
        Option<&ImportedScaleText>,
        Option<&ImportedSelectionText>,
        Option<&ImportedAssignmentText>,
        Option<&ImportedMaterialText>,
        Option<&ImportedUvText>,
    )>,
) {
    if !state.labels_dirty {
        return;
    }
    state.labels_dirty = false;
    let asset_label = format!(
        "{}\nSource: {}\nSaved records: {}",
        state.spec.display_name,
        if state.spec.source_asset.is_empty() {
            "none"
        } else {
            &state.spec.source_asset
        },
        state.saved_entries.len()
    );
    let mesh_name = state
        .mesh_names
        .get(state.spec.selected_mesh)
        .map(String::as_str)
        .unwrap_or("none");
    let mesh_label = format!(
        "Mesh {}/{}: {mesh_name}",
        state.spec.selected_mesh.saturating_add(1),
        state.mesh_names.len()
    );
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
    let modifier_label = format!(
        "Armed: {armed}\nStack: {} operations\nSelected: {detail}",
        stack.len()
    );
    let scale_label = format!(
        "Width  {:.2}\nHeight {:.2}\nDepth  {:.2}",
        state.spec.root_scale[0], state.spec.root_scale[1], state.spec.root_scale[2]
    );
    let selection_label = format!(
        "Primitive {} • Face {}\n{} faces selected\nTopology tools operate on the source mesh",
        state.selected_primitive + 1,
        state.face_cursor + 1,
        state.selected_faces.len()
    );
    let preset = ImportedPartPreset::ALL[state.part_preset];
    let selected_part = state
        .spec
        .part_assignments
        .get(state.selected_assignment)
        .map(|part| {
            format!(
                "{} • {:?} → {:?}\n{} faces • {:?}",
                part.part_id,
                part.slot,
                part.joint,
                part.face_indices.len(),
                part.gameplay_role
            )
        })
        .unwrap_or_else(|| "none".into());
    let assignment_label = format!(
        "Preset: {}\nAssignments: {} • Library: {}\nSelected: {}",
        preset.label,
        state.spec.part_assignments.len(),
        state.part_library.len(),
        selected_part
    );
    let texture_channel = ImportedTextureChannel::ALL[state.texture_channel];
    let material_label = state
        .selected_material_edit()
        .map(|edit| {
            let param = match state.material_param {
                0 => format!("Metallic {:.2}", edit.metallic),
                1 => format!("Roughness {:.2}", edit.perceptual_roughness),
                _ => format!("Emission {:.1}", edit.emissive_strength),
            };
            let texture = match texture_channel {
                ImportedTextureChannel::BaseColor => &edit.base_color_texture,
                ImportedTextureChannel::Normal => &edit.normal_texture,
                ImportedTextureChannel::MetallicRoughness => &edit.metallic_roughness_texture,
                ImportedTextureChannel::Emissive => &edit.emissive_texture,
            };
            format!(
                "Override active • {param}\nColor {:.2} {:.2} {:.2}\nChannel: {} • Map: {}",
                edit.base_color[0],
                edit.base_color[1],
                edit.base_color[2],
                texture_channel.label(),
                texture.as_deref().unwrap_or("authored / none")
            )
        })
        .unwrap_or_else(|| {
            format!(
                "Using authored GLB material\nChannel: {}\nNEXT COLOR or +/- creates an override",
                texture_channel.label()
            )
        });
    let uv_label = state
        .selected_uv_edit()
        .map(|edit| {
            let param = match state.uv_param {
                0 => format!("Scale U {:.2}", edit.scale[0]),
                1 => format!("Scale V {:.2}", edit.scale[1]),
                2 => format!("Offset U {:.2}", edit.offset[0]),
                3 => format!("Offset V {:.2}", edit.offset[1]),
                _ => format!("Rotation {:.0}°", edit.rotation_radians.to_degrees()),
            };
            format!(
                "{} • {param}\nSeams: {} • Paint strokes: {}\nSelected faces: {} • PNG: {}\nPaint RGB {:.2} {:.2} {:.2}",
                uv_projection_label(edit.projection),
                edit.seam_edges.len(),
                edit.face_paints.len(),
                state.selected_faces.len(),
                edit.baked_paint_texture.as_deref().unwrap_or("not baked"),
                IMPORTED_COLOR_PALETTE[state.paint_color][0],
                IMPORTED_COLOR_PALETTE[state.paint_color][1],
                IMPORTED_COLOR_PALETTE[state.paint_color][2],
            )
        })
        .unwrap_or_else(|| {
            format!(
                "Authored UVs • no paint\nPaint color RGB {:.2} {:.2} {:.2}",
                IMPORTED_COLOR_PALETTE[state.paint_color][0],
                IMPORTED_COLOR_PALETTE[state.paint_color][1],
                IMPORTED_COLOR_PALETTE[state.paint_color][2],
            )
        });
    for (mut text, status, asset, mesh, modifier, scale, selection, assignment, material, uv) in
        &mut labels
    {
        let value = if status.is_some() {
            &state.status
        } else if asset.is_some() {
            &asset_label
        } else if mesh.is_some() {
            &mesh_label
        } else if modifier.is_some() {
            &modifier_label
        } else if scale.is_some() {
            &scale_label
        } else if selection.is_some() {
            &selection_label
        } else if assignment.is_some() {
            &assignment_label
        } else if material.is_some() {
            &material_label
        } else if uv.is_some() {
            &uv_label
        } else {
            continue;
        };
        *text = Text::new(value.clone());
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

    #[test]
    fn static_import_preview_strips_deformation_data_without_touching_geometry() {
        let mut source = Mesh::from(Cuboid::new(1.0, 2.0, 3.0));
        let vertex_count = source.count_vertices();
        source.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_INDEX,
            bevy::mesh::VertexAttributeValues::Uint16x4(vec![[0_u16, 1, 0, 0]; vertex_count]),
        );
        source.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_WEIGHT,
            vec![[0.75_f32, 0.25, 0.0, 0.0]; vertex_count],
        );
        source.set_morph_targets(vec![default(); vertex_count]);

        let preview = static_import_preview_mesh(&source);

        assert!(source.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_some());
        assert!(source.has_morph_targets());
        assert!(preview.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        assert_eq!(preview.count_vertices(), vertex_count);
        assert_eq!(preview.indices(), source.indices());
        assert!(preview.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_none());
        assert!(preview.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_none());
        assert!(!preview.has_morph_targets());
    }
}
