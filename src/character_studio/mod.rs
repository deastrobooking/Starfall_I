//! Character Studio — the in-game human character generator.
//!
//! Fully GUI-driven (no keyboard commands): every control is a clickable
//! button. Mouse hover pulls focus into the shared highlight model. The
//! viewport orbits with right-mouse drag / wheel zoom, or gamepad right stick +
//! triggers.
//!
//! ```text
//! buttons/presets/randomize → CharacterSpec → generators → CharacterPatch → meshes
//! ```
//!
//! Saved characters are **versioned**: every SAVE writes a new
//! `human_vNNN.json` under `assets/presets/humans/`, listed live in the
//! library panel with LOAD/DELETE buttons per version.

pub mod generators;
pub mod glb_export;
pub mod human_mesh;
pub mod rig_bridge;
pub mod spec;

use bevy::asset::LoadState;
use bevy::gltf::GltfAssetLabel;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use std::path::PathBuf;

use crate::character::blueprint::{
    BodyRecipe, CartoonAppearanceRecipe, CharacterBlueprint, CharacterPaletteRecipe,
};
use crate::engine::bindings::FaceButton;
use crate::engine::game_rng::GameRng;
use crate::engine::state::AppState;
use crate::engine_tools::tool_windows::{
    spawn_tool_window, ToolWindowPointerState, ToolWindowStyle, ToolWindowSystemSet,
};
use crate::plugins::input_plugin::{
    mapped_gamepad_face, mapped_native_face, NativeButton, NativeControllerState,
};
use crate::resources::{
    CharacterDesignData, CharacterDesignReturnTarget, GameSettings, PlayerSelectState,
};
use generators::build_character_patch;
use rig_bridge::{ImportedHumanoidRig, ImportedRigStatus};
use spec::{CharacterGeometryScope, CharacterSpec, MorphField, StyleField};

pub struct CharacterStudioPlugin;

impl Plugin for CharacterStudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StudioState>()
            .init_resource::<StudioFocus>()
            .init_resource::<StudioSliderDrag>()
            .init_resource::<StudioNavRepeat>()
            .init_resource::<SaveLibrary>()
            .init_resource::<ToolWindowPointerState>()
            .add_systems(OnEnter(AppState::CharacterStudio), setup_studio)
            .add_systems(OnExit(AppState::CharacterStudio), cleanup_studio)
            .add_systems(
                Update,
                (
                    button_interaction,
                    morph_slider_interaction,
                    studio_controller_navigation,
                    apply_studio_character_to_player,
                    export_glb_system,
                    rebuild_library_rows,
                    rebuild_preview,
                    preview_expression,
                    fallback_failed_rig_preview,
                    refresh_imported_rig_status,
                    orbit_camera.after(ToolWindowSystemSet::PointerState),
                    refresh_labels,
                    refresh_focus_highlight,
                )
                    .chain()
                    .run_if(in_state(AppState::CharacterStudio)),
            );
    }
}

// ── Actions (one dispatcher for GUI buttons) ─────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
enum StudioAction {
    Male,
    Female,
    StarHero,
    ShadowRaider,
    ManaAdventurer,
    StreetRunner,
    MechaRobot,
    Athletic,
    Heavy,
    Slim,
    SoftFace,
    HeroicFace,
    ChibiFace,
    SharpFace,
    Random,
    Undo,
    ResetAll,
    ModifierKind,
    ModifierAdd,
    ModifierRemove,
    ViewFront,
    ViewProfile,
    ViewBack,
    ViewFull,
    ViewFace,
    ExpressionNeutral,
    ExpressionJoy,
    ExpressionDetermined,
    ExpressionSurprised,
    PreviewProcedural,
    PreviewRigged,
    SaveVersion,
    ExportGlb,
    UseInGame,
    Back,
    MorphDec(usize),
    MorphInc(usize),
    MorphReset(usize),
    StylePrev(usize),
    StyleNext(usize),
    StyleReset(usize),
    LoadFile(usize),
    DeleteFile(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StudioNavDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Focusable rows. Action groups hold several buttons, while morph/style rows
/// expose their own decrement/increment buttons.
const ACTION_GROUPS: [&[StudioAction]; 11] = [
    &[StudioAction::Male, StudioAction::Female],
    &[
        StudioAction::StarHero,
        StudioAction::ShadowRaider,
        StudioAction::ManaAdventurer,
        StudioAction::StreetRunner,
        StudioAction::MechaRobot,
    ],
    &[
        StudioAction::Athletic,
        StudioAction::Heavy,
        StudioAction::Slim,
    ],
    &[
        StudioAction::SoftFace,
        StudioAction::HeroicFace,
        StudioAction::ChibiFace,
        StudioAction::SharpFace,
    ],
    &[
        StudioAction::Random,
        StudioAction::Undo,
        StudioAction::ResetAll,
    ],
    &[
        StudioAction::ModifierKind,
        StudioAction::ModifierAdd,
        StudioAction::ModifierRemove,
    ],
    &[
        StudioAction::ViewFront,
        StudioAction::ViewProfile,
        StudioAction::ViewBack,
    ],
    &[StudioAction::ViewFull, StudioAction::ViewFace],
    &[
        StudioAction::ExpressionNeutral,
        StudioAction::ExpressionJoy,
        StudioAction::ExpressionDetermined,
        StudioAction::ExpressionSurprised,
    ],
    &[StudioAction::PreviewProcedural, StudioAction::PreviewRigged],
    &[
        StudioAction::SaveVersion,
        StudioAction::ExportGlb,
        StudioAction::UseInGame,
        StudioAction::Back,
    ],
];
const ACTION_SECTION_LABELS: [Option<&str>; ACTION_GROUPS.len()] = [
    None,
    None,
    Some("BODY PRESETS"),
    Some("FACE PRESETS"),
    Some("EDIT HISTORY"),
    Some("MESH MODIFIERS"),
    Some("VIEW ANGLE"),
    Some("FRAMING"),
    Some("EXPRESSION PREVIEW"),
    Some("PREVIEW BACKEND"),
    Some("PROJECT"),
];
const ACTION_ROW_COUNT: usize = ACTION_GROUPS.len();
const MORPH_ROW_0: usize = ACTION_ROW_COUNT;
const STYLE_ROW_0: usize = MORPH_ROW_0 + MorphField::ALL.len();
const FILE_ROW_0: usize = STYLE_ROW_0 + StyleField::ALL.len();

fn action_label(action: StudioAction) -> &'static str {
    match action {
        StudioAction::Male => "MALE",
        StudioAction::Female => "FEMALE",
        StudioAction::StarHero => "STAR HERO",
        StudioAction::ShadowRaider => "SHADOW RAIDER",
        StudioAction::ManaAdventurer => "MANA ADVENTURER",
        StudioAction::StreetRunner => "STREET RUNNER",
        StudioAction::MechaRobot => "MECHA ROBOT",
        StudioAction::Athletic => "ATHLETIC",
        StudioAction::Heavy => "HEAVY",
        StudioAction::Slim => "SLIM",
        StudioAction::SoftFace => "SOFT",
        StudioAction::HeroicFace => "HEROIC",
        StudioAction::ChibiFace => "CHIBI",
        StudioAction::SharpFace => "RIVAL",
        StudioAction::Random => "RANDOM",
        StudioAction::Undo => "UNDO",
        StudioAction::ResetAll => "RESET ALL",
        StudioAction::ModifierKind => "MOD KIND",
        StudioAction::ModifierAdd => "ADD MOD",
        StudioAction::ModifierRemove => "REMOVE MOD",
        StudioAction::ViewFront => "FRONT",
        StudioAction::ViewProfile => "PROFILE",
        StudioAction::ViewBack => "BACK VIEW",
        StudioAction::ViewFull => "FULL BODY",
        StudioAction::ViewFace => "FACE CLOSE-UP",
        StudioAction::ExpressionNeutral => "NEUTRAL",
        StudioAction::ExpressionJoy => "JOY",
        StudioAction::ExpressionDetermined => "DETERMINED",
        StudioAction::ExpressionSurprised => "SURPRISED",
        StudioAction::PreviewProcedural => "GENERATED",
        StudioAction::PreviewRigged => "RIG TEST",
        StudioAction::SaveVersion => "SAVE VERSION",
        StudioAction::ExportGlb => "EXPORT GLB",
        StudioAction::UseInGame => "USE IN GAME",
        StudioAction::Back => "BACK",
        _ => "",
    }
}

// ── Resources / components ────────────────────────────────────────────────────

#[derive(Resource)]
pub struct StudioState {
    pub spec: CharacterSpec,
    undo: Vec<CharacterSpec>,
    dirty: bool,
    dirty_scope: CharacterGeometryScope,
    last_rebuild_scope: CharacterGeometryScope,
    generation_revision: u64,
    labels_dirty: bool,
    yaw: f32,
    pitch: f32,
    distance: f32,
    focus_height: f32,
    preview_backend: StudioPreviewBackend,
    status: String,
    apply_requested: bool,
    export_requested: bool,
    /// Which modifier template the ADD MOD button is armed with.
    armed_modifier: usize,
    preview_expression: StudioExpression,
}

impl Default for StudioState {
    fn default() -> Self {
        Self {
            spec: generators::preset_male(),
            undo: Vec::new(),
            dirty: true,
            dirty_scope: CharacterGeometryScope::ALL,
            last_rebuild_scope: CharacterGeometryScope::NONE,
            generation_revision: 0,
            labels_dirty: true,
            // Generated humans face -Z, so PI is the true front view.
            yaw: std::f32::consts::PI,
            pitch: 0.05,
            distance: 3.4,
            focus_height: 0.95,
            preview_backend: StudioPreviewBackend::Procedural,
            status: "Welcome to the Character Studio".to_string(),
            apply_requested: false,
            export_requested: false,
            armed_modifier: 0,
            preview_expression: StudioExpression::Neutral,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StudioPreviewBackend {
    Procedural,
    RiggedAmp,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum StudioExpression {
    #[default]
    Neutral,
    Joy,
    Determined,
    Surprised,
}

impl StudioState {
    fn push_undo(&mut self) {
        self.undo.push(self.spec);
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
    }
}

/// Focus cursor shared by GUI button hover/click.
#[derive(Resource, Default)]
struct StudioFocus {
    row: usize,
    col: usize,
}

#[derive(Resource, Default)]
struct StudioSliderDrag {
    active: Option<usize>,
}

#[derive(Resource, Default)]
struct StudioNavRepeat {
    direction: Option<StudioNavDirection>,
    timer: f32,
}

/// Versioned character files on disk (`human_vNNN.json`).
#[derive(Resource, Default)]
struct SaveLibrary {
    files: Vec<String>,
    dirty: bool,
}

#[derive(Component)]
struct StudioRoot;

#[derive(Component)]
struct StudioPreview;

#[derive(Component)]
struct RiggedPreviewAsset(Handle<WorldAsset>);

#[derive(Component)]
struct StudioCamera;

#[derive(Component)]
struct ActionButton {
    action: StudioAction,
    row: usize,
    col: usize,
}

#[derive(Component)]
struct FocusRow(usize);

#[derive(Component)]
struct MorphValueText(usize);

#[derive(Component)]
struct MorphSlider {
    index: usize,
    row: usize,
    col: usize,
}

#[derive(Component)]
struct MorphSliderFill(usize);

#[derive(Component)]
struct MorphSliderThumb(usize);

#[derive(Component)]
struct StyleValueText(usize);

#[derive(Component)]
struct StyleSwatch(usize);

#[derive(Component)]
struct ModelInfoText;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct LibraryPanel;

#[derive(Component)]
struct StudioLeftPanel;

#[derive(Component)]
struct StudioRightPanel;

const BTN_BG: Color = Color::srgba(0.14, 0.18, 0.26, 0.95);
const BTN_BG_FOCUS: Color = Color::srgba(0.26, 0.45, 0.80, 0.95);
const ROW_BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
const ROW_BG_FOCUS: Color = Color::srgba(0.20, 0.32, 0.55, 0.35);

// ── Setup ─────────────────────────────────────────────────────────────────────

fn setup_studio(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<StudioState>,
    mut focus: ResMut<StudioFocus>,
    mut library: ResMut<SaveLibrary>,
    select: Res<PlayerSelectState>,
    design: Res<CharacterDesignData>,
) {
    state.spec = select
        .slots
        .get(design.player_index)
        .and_then(|slot| slot.studio_spec)
        .unwrap_or_else(generators::preset_male);
    state.undo.clear();
    state.preview_expression = StudioExpression::Neutral;
    state.apply_requested = false;
    state.dirty = true;
    state.dirty_scope = CharacterGeometryScope::ALL;
    state.labels_dirty = true;
    if std::env::var_os("STARFALL_STUDIO_RIG").is_some() {
        state.preview_backend = StudioPreviewBackend::RiggedAmp;
        state.status = "AMP rig diagnostic requested".into();
    }
    library.files = scan_library();
    library.dirty = true;
    focus.row = 0;
    focus.col = 0;

    commands.spawn((
        StudioRoot,
        StudioCamera,
        Camera3d::default(),
        Transform::from_xyz(0.0, 1.5, 3.4).looking_at(Vec3::new(0.0, 0.95, 0.0), Vec3::Y),
    ));
    commands.spawn((
        StudioRoot,
        DirectionalLight {
            illuminance: 11_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(2.5, 4.0, 2.5).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        StudioRoot,
        DirectionalLight {
            illuminance: 3_200.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-3.0, 2.0, -2.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        StudioRoot,
        Mesh3d(meshes.add(Mesh::from(Cylinder::new(1.15, 0.08)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.22, 0.24, 0.28),
            perceptual_roughness: 0.6,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.04, 0.0),
    ));
    commands.spawn((
        StudioRoot,
        Mesh3d(meshes.add(Mesh::from(Cylinder::new(4.5, 0.02)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.11, 0.13),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.09, 0.0),
    ));

    commands
        .spawn((
            StudioRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            spawn_left_panel(root, &state);
            spawn_right_panel(root);
        });
}

fn text(value: impl Into<String>, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn spawn_action_button(
    parent: &mut ChildSpawnerCommands,
    action: StudioAction,
    row: usize,
    col: usize,
) {
    parent
        .spawn((
            Button,
            ActionButton { action, row, col },
            Node {
                min_height: Val::Px(36.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                margin: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(BTN_BG),
        ))
        .with_children(|b| {
            b.spawn(text(
                action_label(action),
                12.0,
                Color::srgb(0.92, 0.95, 1.0),
            ));
        });
}

fn spawn_small_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: StudioAction,
    row: usize,
    col: usize,
) {
    parent
        .spawn((
            Button,
            ActionButton { action, row, col },
            Node {
                width: Val::Px(36.0),
                height: Val::Px(36.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                margin: UiRect::horizontal(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(BTN_BG),
        ))
        .with_children(|b| {
            b.spawn(text(label, 12.0, Color::srgb(0.92, 0.95, 1.0)));
        });
}

fn spawn_morph_slider(
    parent: &mut ChildSpawnerCommands,
    index: usize,
    value: f32,
    row: usize,
    col: usize,
) {
    let pct = value.clamp(0.0, 1.0);
    parent
        .spawn((
            Button,
            MorphSlider { index, row, col },
            RelativeCursorPosition::default(),
            Node {
                width: Val::Px(92.0),
                height: Val::Px(36.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::FlexStart,
                margin: UiRect::horizontal(Val::Px(4.0)),
                padding: UiRect::horizontal(Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.045, 0.065, 0.095, 0.95)),
            BorderColor::all(Color::srgb(0.22, 0.34, 0.52)),
        ))
        .with_children(|track| {
            track.spawn((
                MorphSliderFill(index),
                Node {
                    width: Val::Percent(pct * 100.0),
                    height: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.26, 0.78, 0.52)),
            ));
            track.spawn((
                MorphSliderThumb(index),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(pct * 100.0),
                    width: Val::Px(6.0),
                    height: Val::Px(16.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.88, 1.0, 0.78)),
            ));
        });
}

fn section_label(parent: &mut ChildSpawnerCommands, label: &str) {
    parent
        .spawn((
            Node {
                margin: UiRect::top(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|n| {
            n.spawn(text(label, 11.0, Color::srgb(0.55, 0.75, 1.0)));
        });
}

fn spawn_left_panel(parent: &mut ChildSpawnerCommands, state: &StudioState) {
    spawn_tool_window(
        parent,
        "CHARACTER STUDIO",
        Vec2::new(8.0, 8.0),
        ToolWindowStyle {
            accent: Color::srgb(0.22, 0.56, 0.82),
            background: Color::srgba(0.02, 0.03, 0.05, 0.94),
            width: 320.0,
            content_height: Val::Px(640.0),
            ..default()
        },
        (ScrollPosition::default(), StudioLeftPanel),
        |panel| {
            {
                let (t, f, c) = text("", 11.0, Color::srgb(0.95, 0.85, 0.4));
                panel.spawn((StatusText, t, f, c));
            }

            section_label(panel, "BASE PRESETS");
            for (g, actions) in ACTION_GROUPS[..ACTION_ROW_COUNT].iter().enumerate() {
                if let Some(label) = ACTION_SECTION_LABELS[g] {
                    section_label(panel, label);
                }
                panel
                    .spawn((
                        FocusRow(g),
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            row_gap: Val::Px(4.0),
                            padding: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(ROW_BG),
                    ))
                    .with_children(|row| {
                        for (c, action) in actions.iter().enumerate() {
                            spawn_action_button(row, *action, g, c);
                        }
                    });
            }

            section_label(panel, "BODY & FACE");
            for (i, field) in MorphField::ALL.iter().enumerate() {
                let row_idx = MORPH_ROW_0 + i;
                panel
                    .spawn((
                        FocusRow(row_idx),
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::SpaceBetween,
                            padding: UiRect::axes(Val::Px(4.0), Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(ROW_BG),
                    ))
                    .with_children(|row| {
                        row.spawn(Node {
                            width: Val::Px(92.0),
                            ..default()
                        })
                        .with_children(|n| {
                            n.spawn(text(field.label(), 12.0, Color::srgb(0.85, 0.90, 0.96)));
                        });
                        row.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|controls| {
                            spawn_small_button(
                                controls,
                                "-",
                                StudioAction::MorphDec(i),
                                row_idx,
                                0,
                            );
                            spawn_morph_slider(controls, i, field.get(&state.spec), row_idx, 1);
                            controls
                                .spawn(Node {
                                    width: Val::Px(38.0),
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                })
                                .with_children(|n| {
                                    let (t, f, c) = text(
                                        morph_percent_label(field.get(&state.spec)),
                                        11.0,
                                        Color::srgb(0.60, 0.95, 0.75),
                                    );
                                    n.spawn((MorphValueText(i), t, f, c));
                                });
                            spawn_small_button(
                                controls,
                                "R",
                                StudioAction::MorphReset(i),
                                row_idx,
                                2,
                            );
                            spawn_small_button(
                                controls,
                                "+",
                                StudioAction::MorphInc(i),
                                row_idx,
                                3,
                            );
                        });
                    });
            }

            section_label(panel, "STYLE & WARDROBE");
            for (i, field) in StyleField::ALL.iter().enumerate() {
                let row_idx = STYLE_ROW_0 + i;
                panel
                    .spawn((
                        FocusRow(row_idx),
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::SpaceBetween,
                            padding: UiRect::axes(Val::Px(4.0), Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(ROW_BG),
                    ))
                    .with_children(|row| {
                        row.spawn(Node {
                            width: Val::Px(92.0),
                            ..default()
                        })
                        .with_children(|n| {
                            n.spawn(text(field.label(), 12.0, Color::srgb(0.85, 0.90, 0.96)));
                        });
                        row.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|controls| {
                            spawn_small_button(
                                controls,
                                "<",
                                StudioAction::StylePrev(i),
                                row_idx,
                                0,
                            );
                            controls
                                .spawn(Node {
                                    width: Val::Px(76.0),
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                })
                                .with_children(|n| {
                                    let (t, f, c) = text(
                                        field.value_label(&state.spec),
                                        12.0,
                                        Color::srgb(0.95, 0.85, 0.55),
                                    );
                                    n.spawn((StyleValueText(i), t, f, c));
                                });
                            if style_field_has_swatch(*field) {
                                controls.spawn((
                                    StyleSwatch(i),
                                    Node {
                                        width: Val::Px(16.0),
                                        height: Val::Px(16.0),
                                        margin: UiRect::horizontal(Val::Px(2.0)),
                                        border: UiRect::all(Val::Px(1.0)),
                                        ..default()
                                    },
                                    BackgroundColor(style_swatch_color(*field, &state.spec)),
                                    BorderColor::all(Color::srgb(0.72, 0.78, 0.86)),
                                ));
                            }
                            spawn_small_button(
                                controls,
                                "R",
                                StudioAction::StyleReset(i),
                                row_idx,
                                1,
                            );
                            spawn_small_button(
                                controls,
                                ">",
                                StudioAction::StyleNext(i),
                                row_idx,
                                2,
                            );
                        });
                    });
            }
        },
    );
}

fn spawn_right_panel(parent: &mut ChildSpawnerCommands) {
    spawn_tool_window(
        parent,
        "INFO & LIBRARY",
        Vec2::new(1032.0, 8.0),
        ToolWindowStyle {
            accent: Color::srgb(0.38, 0.48, 0.72),
            background: Color::srgba(0.02, 0.03, 0.05, 0.94),
            width: 240.0,
            content_height: Val::Px(520.0),
            initially_minimized: false,
        },
        (ScrollPosition::default(), StudioRightPanel),
        |panel| {
            panel.spawn(text("MODEL INFO", 14.0, Color::srgb(0.65, 0.85, 1.0)));
            panel.spawn((ModelInfoText, text("", 11.0, Color::srgb(0.78, 0.84, 0.92))));
            panel.spawn(text(
                "CONTROLS\nD-pad / left stick: select + adjust\nA: activate   X: reset field\nLB/RB: jump sections\nRight stick: orbit   LT/RT: zoom\nMouse: drag sliders, right-drag orbit",
                10.0,
                Color::srgb(0.60, 0.70, 0.82),
            ));
            section_label(panel, "LIBRARY");
            panel.spawn(text("SAVED VERSIONS", 14.0, Color::srgb(0.65, 0.85, 1.0)));
            panel.spawn((
                LibraryPanel,
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
            ));
        },
    );
}

fn morph_percent_label(v: f32) -> String {
    format!("{:>3}%", (v.clamp(0.0, 1.0) * 100.0).round() as u8)
}

fn style_field_has_swatch(field: StyleField) -> bool {
    matches!(
        field,
        StyleField::SkinTone
            | StyleField::EyeColor
            | StyleField::HairColor
            | StyleField::PrimaryColor
            | StyleField::SecondaryColor
    )
}

fn style_swatch_color(field: StyleField, spec: &CharacterSpec) -> Color {
    match field {
        StyleField::SkinTone => generators::skin_tones()[spec.style.skin_tone % 8],
        StyleField::EyeColor => generators::eye_colors()[spec.style.eye_color % 8],
        StyleField::HairColor => generators::hair_colors()[spec.style.hair_color % 12],
        StyleField::PrimaryColor => generators::outfit_colors()[spec.style.primary_color % 12],
        StyleField::SecondaryColor => generators::outfit_colors()[spec.style.secondary_color % 12],
        _ => Color::NONE,
    }
}

fn cleanup_studio(
    mut commands: Commands,
    roots: Query<Entity, With<StudioRoot>>,
    previews: Query<Entity, With<StudioPreview>>,
) {
    for e in roots.iter().chain(previews.iter()) {
        commands.entity(e).despawn();
    }
}

pub fn studio_spec_to_blueprint(name: &str, spec: CharacterSpec) -> CharacterBlueprint {
    let style = spec.style_vector.smoothed();
    let body = BodyRecipe {
        height: (0.82 + spec.body.height * 0.42) * (1.0 - style.cute * 0.05),
        shoulder_width: (0.76 + spec.body.shoulder_width * 0.54)
            * (1.0 + style.heroic * 0.10 + style.mechanical * 0.07),
        chest_size: 0.74
            + spec.body.muscle * 0.26
            + spec.body.weight * 0.12
            + spec.body.chest_shape * 0.18
            + style.heroic * 0.08
            + style.mechanical * 0.06,
        arm_length: (0.78 + spec.body.limb_length * 0.48)
            * (1.0 + style.heroic * 0.06 + style.creature * 0.10),
        leg_length: (0.82 + spec.body.limb_length * 0.55)
            * (1.0 - style.cute * 0.05 + style.heroic * 0.05 + style.creature * 0.08),
        hand_size: (0.88 + spec.body.muscle * 0.22)
            * (1.0 + style.fantasy * 0.06 + style.cute * 0.16 + style.heroic * 0.08),
        foot_size: (0.90 + spec.body.weight * 0.20)
            * (1.0 + style.fantasy * 0.08 + style.cute * 0.18 + style.mechanical * 0.10),
        head_size: (0.88 + spec.face.eye_size * 0.22)
            * (1.0 + style.fantasy * 0.05 + style.cute * 0.18 + style.creature * 0.08),
        neck_length: 0.84 + spec.body.limb_length * 0.24,
        torso_curve: (spec.body.waist_width - 0.5) * 0.34 - style.fantasy * 0.04
            + style.creature * 0.05,
        hip_width: 0.78 + spec.body.hip_width * 0.48,
        spine_posture: 0.0,
        mass: 0.74 + spec.body.weight * 0.66,
        muscle: 0.72 + spec.body.muscle * 0.66,
        body_fat: 0.72 + spec.body.weight * 0.66,
        asymmetry: 0.0,
    }
    .validated();
    let wardrobe = spec.style.wardrobe;
    CharacterBlueprint::hero(
        name,
        body,
        CharacterPaletteRecipe {
            skin: generators::skin_tones()[spec.style.skin_tone % 8],
            outfit: generators::outfit_colors()[spec.style.primary_color % 12],
            accent: generators::outfit_colors()[spec.style.secondary_color % 12],
            hair: generators::hair_colors()[spec.style.hair_color % 12],
            eye: generators::eye_colors()[spec.style.eye_color % 8],
        },
        CartoonAppearanceRecipe {
            has_hood: false,
            has_cape: matches!(
                wardrobe.top,
                spec::TopStyle::Robe | spec::TopStyle::ArcaneCoat
            ) || matches!(spec.style.flair, spec::FlairStyle::RoyalMantle),
            has_gloves: !matches!(wardrobe.hands, spec::HandStyle::Bare),
            has_boots: !matches!(wardrobe.feet, spec::FootStyle::Barefoot),
            has_shoulder_pads: !matches!(wardrobe.armor, spec::ArmorStyle::None)
                || matches!(wardrobe.top, spec::TopStyle::StarKnightTunic),
            has_visor: matches!(
                wardrobe.armor,
                spec::ArmorStyle::MechaArmor
                    | spec::ArmorStyle::CrystalMecha
                    | spec::ArmorStyle::DragonMecha
            ),
        },
    )
}

fn apply_studio_character_to_player(
    mut state: ResMut<StudioState>,
    design: Res<CharacterDesignData>,
    mut select: ResMut<PlayerSelectState>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if !state.apply_requested {
        return;
    }
    state.apply_requested = false;
    let player_index = design.player_index.min(select.slots.len() - 1);
    let name = select.character_name(player_index);
    let blueprint = studio_spec_to_blueprint(name, state.spec);
    let Some(slot) = select.slots.get_mut(player_index) else {
        return;
    };
    slot.studio_spec = Some(state.spec);
    slot.blueprint = Some(blueprint);
    slot.skin_idx = Some(state.spec.style.skin_tone);
    slot.hair_idx = Some(state.spec.style.hair_color);
    slot.outfit_idx = Some(state.spec.style.primary_color);
    slot.accent_idx = Some(state.spec.style.secondary_color);
    slot.eye_idx = Some(state.spec.style.eye_color);
    slot.ready = false;

    next_state.set(match design.return_target {
        CharacterDesignReturnTarget::PlayerSelect => AppState::PlayerSelect,
        CharacterDesignReturnTarget::ChapterSelect => AppState::ChapterSelect,
    });
}

// ── Action dispatch ───────────────────────────────────────────────────────────

fn apply_action(
    action: StudioAction,
    state: &mut StudioState,
    library: &mut SaveLibrary,
    game_rng: &mut GameRng,
    next_state: &mut NextState<AppState>,
) {
    let before = state.spec;
    let mark = |state: &mut StudioState, msg: String| {
        state.labels_dirty = true;
        state.status = msg;
    };
    match action {
        StudioAction::Male => {
            state.push_undo();
            state.spec = generators::preset_male();
            mark(state, "Preset: Male".into());
        }
        StudioAction::Female => {
            state.push_undo();
            state.spec = generators::preset_female();
            mark(state, "Preset: Female".into());
        }
        StudioAction::StarHero => {
            state.push_undo();
            state.spec = generators::preset_star_hero();
            mark(state, "Good-guy preset: Star Hero".into());
        }
        StudioAction::ShadowRaider => {
            state.push_undo();
            state.spec = generators::preset_shadow_raider();
            mark(state, "Bad-guy preset: Shadow Raider".into());
        }
        StudioAction::ManaAdventurer => {
            state.push_undo();
            state.spec = generators::preset_mana_adventurer();
            mark(state, "Fantasy preset: Mana Adventurer".into());
        }
        StudioAction::StreetRunner => {
            state.push_undo();
            state.spec = generators::preset_street_runner();
            mark(state, "Street-clothes preset: Street Runner".into());
        }
        StudioAction::MechaRobot => {
            state.push_undo();
            state.spec = generators::preset_mecha_robot();
            mark(state, "Robot preset: Mecha Robot".into());
        }
        StudioAction::Athletic => {
            state.push_undo();
            generators::apply_athletic(&mut state.spec);
            mark(state, "Preset: Athletic".into());
        }
        StudioAction::Heavy => {
            state.push_undo();
            generators::apply_heavy(&mut state.spec);
            mark(state, "Preset: Heavy".into());
        }
        StudioAction::Slim => {
            state.push_undo();
            generators::apply_slim(&mut state.spec);
            mark(state, "Preset: Slim".into());
        }
        StudioAction::SoftFace => {
            state.push_undo();
            generators::apply_soft_face(&mut state.spec);
            mark(state, "Face seed: Soft Anime".into());
        }
        StudioAction::HeroicFace => {
            state.push_undo();
            generators::apply_heroic_face(&mut state.spec);
            mark(state, "Face seed: Heroic Anime".into());
        }
        StudioAction::ChibiFace => {
            state.push_undo();
            generators::apply_chibi_face(&mut state.spec);
            mark(state, "Face seed: Chibi Anime".into());
        }
        StudioAction::SharpFace => {
            state.push_undo();
            generators::apply_sharp_face(&mut state.spec);
            mark(state, "Face seed: Sharp Rival".into());
        }
        StudioAction::Random => {
            state.push_undo();
            let mut spec = state.spec;
            generators::randomize(&mut spec, game_rng.cosmetic());
            state.spec = spec;
            mark(state, "Randomized".into());
        }
        StudioAction::Undo => {
            if let Some(prev) = state.undo.pop() {
                state.spec = prev;
                mark(state, "Undo".into());
            } else {
                state.status = "Nothing to undo".into();
                state.labels_dirty = true;
            }
        }
        StudioAction::ResetAll => {
            state.push_undo();
            state.spec = CharacterSpec::default();
            mark(state, "Reset to neutral model".into());
        }
        StudioAction::ModifierKind => {
            state.armed_modifier = (state.armed_modifier + 1)
                % crate::character::mesh_modifiers::MeshModifier::TEMPLATE_COUNT;
            let label =
                crate::character::mesh_modifiers::MeshModifier::template(state.armed_modifier)
                    .label();
            state.status = format!("Armed modifier: {label}");
            state.labels_dirty = true;
        }
        StudioAction::ModifierAdd => {
            let armed =
                crate::character::mesh_modifiers::MeshModifier::template(state.armed_modifier);
            let Some(slot) = state.spec.modifiers.iter().position(|slot| slot.is_none()) else {
                state.status = "All 4 modifier slots are full".into();
                state.labels_dirty = true;
                return;
            };
            state.push_undo();
            let label = armed.label();
            state.spec.modifiers[slot] = Some(armed);
            mark(state, format!("Added {label} (slot {})", slot + 1));
        }
        StudioAction::ModifierRemove => {
            let Some(slot) = state.spec.modifiers.iter().rposition(|slot| slot.is_some()) else {
                state.status = "No modifiers to remove".into();
                state.labels_dirty = true;
                return;
            };
            state.push_undo();
            let label = state.spec.modifiers[slot]
                .map(|modifier| modifier.label())
                .unwrap_or_default();
            state.spec.modifiers[slot] = None;
            mark(state, format!("Removed {label}"));
        }
        StudioAction::ViewFront => {
            state.yaw = std::f32::consts::PI;
            state.pitch = 0.05;
            state.status = "Front orthographic-style view".into();
            state.labels_dirty = true;
        }
        StudioAction::ViewProfile => {
            state.yaw = std::f32::consts::FRAC_PI_2;
            state.pitch = 0.05;
            state.status = "Profile view".into();
            state.labels_dirty = true;
        }
        StudioAction::ViewBack => {
            state.yaw = 0.0;
            state.pitch = 0.05;
            state.status = "Back view".into();
            state.labels_dirty = true;
        }
        StudioAction::ViewFull => {
            state.focus_height = 0.95;
            state.distance = 3.4;
            state.status = "Full-body framing".into();
            state.labels_dirty = true;
        }
        StudioAction::ViewFace => {
            state.yaw = std::f32::consts::PI;
            state.pitch = 0.0;
            state.focus_height = estimated_height(&state.spec) * 0.928;
            state.distance = 1.25;
            state.status = "Face sculpt close-up".into();
            state.labels_dirty = true;
        }
        StudioAction::ExpressionNeutral
        | StudioAction::ExpressionJoy
        | StudioAction::ExpressionDetermined
        | StudioAction::ExpressionSurprised => {
            state.preview_expression = match action {
                StudioAction::ExpressionNeutral => StudioExpression::Neutral,
                StudioAction::ExpressionJoy => StudioExpression::Joy,
                StudioAction::ExpressionDetermined => StudioExpression::Determined,
                StudioAction::ExpressionSurprised => StudioExpression::Surprised,
                _ => unreachable!(),
            };
            if state.preview_backend != StudioPreviewBackend::Procedural {
                state.preview_backend = StudioPreviewBackend::Procedural;
                state.dirty = true;
                state.dirty_scope = state.dirty_scope.union(CharacterGeometryScope::ALL);
            }
            let label = match state.preview_expression {
                StudioExpression::Neutral => "Neutral",
                StudioExpression::Joy => "Joy",
                StudioExpression::Determined => "Determined",
                StudioExpression::Surprised => "Surprised",
            };
            state.status = format!("Expression preview: {label} (not saved)");
            state.labels_dirty = true;
        }
        StudioAction::PreviewProcedural => {
            state.preview_backend = StudioPreviewBackend::Procedural;
            state.dirty = true;
            state.dirty_scope = state.dirty_scope.union(CharacterGeometryScope::ALL);
            mark(state, "Generated editable preview active".into());
        }
        StudioAction::PreviewRigged => {
            state.preview_backend = StudioPreviewBackend::RiggedAmp;
            state.dirty = true;
            state.dirty_scope = state.dirty_scope.union(CharacterGeometryScope::ALL);
            mark(
                state,
                "AMP rig test: 17 joints mapped; shape keys not present".into(),
            );
        }
        StudioAction::SaveVersion => match save_new_version(&state.spec, &library.files) {
            Ok(name) => {
                library.files = scan_library();
                library.dirty = true;
                state.status = format!("Saved {name}");
                state.labels_dirty = true;
            }
            Err(err) => {
                state.status = format!("Save failed: {err}");
                state.labels_dirty = true;
            }
        },
        StudioAction::ExportGlb => {
            state.export_requested = true;
            state.status = "Exporting .glb...".into();
            state.labels_dirty = true;
        }
        StudioAction::UseInGame => {
            state.apply_requested = true;
            state.status = "Applying this model to the selected player...".into();
            state.labels_dirty = true;
        }
        StudioAction::Back => next_state.set(AppState::CharacterDesign),
        StudioAction::MorphDec(i) | StudioAction::MorphInc(i) => {
            let dir = if matches!(action, StudioAction::MorphInc(_)) {
                1.0
            } else {
                -1.0
            };
            let field = MorphField::ALL[i];
            state.push_undo();
            let v = field.get(&state.spec) + dir * 0.05;
            field.set(&mut state.spec, v);
            mark(
                state,
                format!("{} = {:.2}", field.label(), field.get(&state.spec)),
            );
        }
        StudioAction::MorphReset(i) => {
            let field = MorphField::ALL[i];
            state.push_undo();
            field.set(&mut state.spec, 0.5);
            mark(state, format!("{} reset to neutral", field.label()));
        }
        StudioAction::StylePrev(i) | StudioAction::StyleNext(i) => {
            let dir = if matches!(action, StudioAction::StyleNext(_)) {
                1
            } else {
                -1
            };
            let field = StyleField::ALL[i];
            state.push_undo();
            field.cycle(&mut state.spec, dir);
            mark(
                state,
                format!("{}: {}", field.label(), field.value_label(&state.spec)),
            );
        }
        StudioAction::StyleReset(i) => {
            let field = StyleField::ALL[i];
            state.push_undo();
            field.reset(&mut state.spec);
            mark(state, format!("{} reset", field.label()));
        }
        StudioAction::LoadFile(i) => {
            if let Some(name) = library.files.get(i).cloned() {
                match load_version(&name) {
                    Ok(spec) => {
                        state.push_undo();
                        state.spec = spec;
                        mark(state, format!("Loaded {name}"));
                    }
                    Err(err) => {
                        state.status = format!("Load failed: {err}");
                        state.labels_dirty = true;
                    }
                }
            }
        }
        StudioAction::DeleteFile(i) => {
            if let Some(name) = library.files.get(i).cloned() {
                let _ = std::fs::remove_file(preset_dir().join(&name));
                library.files = scan_library();
                library.dirty = true;
                state.status = format!("Deleted {name}");
                state.labels_dirty = true;
            }
        }
    }

    let changed_scope = CharacterGeometryScope::between(&before, &state.spec);
    if !changed_scope.is_empty() {
        state.dirty = true;
        state.dirty_scope = state.dirty_scope.union(changed_scope);
    }
}

// ── Mouse ─────────────────────────────────────────────────────────────────────

fn button_interaction(
    mut game_rng: ResMut<GameRng>,
    mut interactions: Query<(&Interaction, &ActionButton), Changed<Interaction>>,
    mut state: ResMut<StudioState>,
    mut library: ResMut<SaveLibrary>,
    mut focus: ResMut<StudioFocus>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for (interaction, button) in interactions.iter_mut() {
        match interaction {
            Interaction::Hovered => {
                focus.row = button.row;
                focus.col = button.col;
            }
            Interaction::Pressed => {
                focus.row = button.row;
                focus.col = button.col;
                apply_action(
                    button.action,
                    &mut state,
                    &mut library,
                    &mut game_rng,
                    &mut next_state,
                );
            }
            Interaction::None => {}
        }
    }
}

fn morph_slider_interaction(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut state: ResMut<StudioState>,
    mut focus: ResMut<StudioFocus>,
    mut drag: ResMut<StudioSliderDrag>,
    slider_q: Query<(&Interaction, &RelativeCursorPosition, &MorphSlider), With<Button>>,
) {
    if mouse_buttons.just_released(MouseButton::Left) {
        drag.active = None;
    }

    for (interaction, cursor, slider) in slider_q.iter() {
        if matches!(interaction, Interaction::Hovered | Interaction::Pressed) {
            focus.row = slider.row;
            focus.col = slider.col;
        }

        if cursor.cursor_over() && mouse_buttons.just_pressed(MouseButton::Left) {
            state.push_undo();
            drag.active = Some(slider.index);
        }

        if drag.active != Some(slider.index) || !mouse_buttons.pressed(MouseButton::Left) {
            continue;
        }

        let Some(normalized) = cursor.normalized else {
            continue;
        };

        let field = MorphField::ALL[slider.index];
        let next_value = normalized.x.clamp(0.0, 1.0);
        if (field.get(&state.spec) - next_value).abs() <= f32::EPSILON {
            continue;
        }

        let before = state.spec;
        field.set(&mut state.spec, next_value);
        state.dirty_scope = state
            .dirty_scope
            .union(CharacterGeometryScope::between(&before, &state.spec));
        state.dirty = true;
        state.labels_dirty = true;
        state.status = format!("{} = {:.2}", field.label(), field.get(&state.spec));
    }
}

#[allow(clippy::too_many_arguments)]
fn studio_controller_navigation(
    mut game_rng: ResMut<GameRng>,
    time: Res<Time>,
    settings: Res<GameSettings>,
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    mut repeat: ResMut<StudioNavRepeat>,
    mut focus: ResMut<StudioFocus>,
    mut state: ResMut<StudioState>,
    mut library: ResMut<SaveLibrary>,
    mut next_state: ResMut<NextState<AppState>>,
    buttons: Query<&ActionButton>,
    focus_rows: Query<(&FocusRow, &GlobalTransform, &ComputedNode)>,
    mut left_panel_q: Query<
        (&GlobalTransform, &ComputedNode, &mut ScrollPosition),
        With<StudioLeftPanel>,
    >,
    mut right_panel_q: Query<
        (&GlobalTransform, &ComputedNode, &mut ScrollPosition),
        (With<StudioRightPanel>, Without<StudioLeftPanel>),
    >,
) {
    let input = studio_nav_input(&keyboard, &gamepads, &native);
    let direction = repeated_studio_direction(&mut repeat, input, time.delta_secs());
    let confirm_gamepad = mapped_gamepad_face(&settings.bindings, FaceButton::South);
    let confirm_native = mapped_native_face(&settings.bindings, FaceButton::South);
    let back_gamepad = mapped_gamepad_face(&settings.bindings, FaceButton::East);
    let back_native = mapped_native_face(&settings.bindings, FaceButton::East);
    let reset_gamepad = mapped_gamepad_face(&settings.bindings, FaceButton::West);
    let reset_native = mapped_native_face(&settings.bindings, FaceButton::West);
    let confirm = keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::NumpadEnter)
        || keyboard.just_pressed(KeyCode::Space)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(confirm_gamepad))
        || native.just_pressed(confirm_native);
    let back = keyboard.just_pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(back_gamepad))
        || native.just_pressed(back_native);
    let reset_field = keyboard.just_pressed(KeyCode::KeyR)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(reset_gamepad))
        || native.just_pressed(reset_native);
    let previous_section = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::LeftTrigger))
        || native.just_pressed(NativeButton::LeftShoulder);
    let next_section = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::RightTrigger))
        || native.just_pressed(NativeButton::RightShoulder);

    if back {
        next_state.set(AppState::CharacterDesign);
        return;
    }

    if previous_section || next_section {
        let sections = [0, MORPH_ROW_0, MORPH_ROW_0 + 7, STYLE_ROW_0, FILE_ROW_0];
        let current = sections
            .iter()
            .rposition(|row| *row <= focus.row)
            .unwrap_or(0);
        let next = if next_section {
            (current + 1) % sections.len()
        } else {
            (current + sections.len() - 1) % sections.len()
        };
        focus.row = sections[next];
        focus.col = 0;
        keep_studio_focus_in_view(&focus, &focus_rows, &mut left_panel_q, &mut right_panel_q);
    }

    if reset_field {
        if focus.row >= MORPH_ROW_0 && focus.row < STYLE_ROW_0 {
            apply_action(
                StudioAction::MorphReset(focus.row - MORPH_ROW_0),
                &mut state,
                &mut library,
                &mut game_rng,
                &mut next_state,
            );
        } else if focus.row >= STYLE_ROW_0 && focus.row < FILE_ROW_0 {
            apply_action(
                StudioAction::StyleReset(focus.row - STYLE_ROW_0),
                &mut state,
                &mut library,
                &mut game_rng,
                &mut next_state,
            );
        }
    }

    if let Some(direction) = direction {
        match direction {
            StudioNavDirection::Up => {
                focus.row = focus.row.saturating_sub(1);
                focus.col = 0;
            }
            StudioNavDirection::Down => {
                let max_row = FILE_ROW_0 + library.files.len().saturating_sub(1);
                focus.row = (focus.row + 1).min(max_row);
                focus.col = 0;
            }
            StudioNavDirection::Left => {
                if focus.row >= MORPH_ROW_0 && focus.row < STYLE_ROW_0 {
                    apply_action(
                        StudioAction::MorphDec(focus.row - MORPH_ROW_0),
                        &mut state,
                        &mut library,
                        &mut game_rng,
                        &mut next_state,
                    );
                } else if focus.row >= STYLE_ROW_0 && focus.row < FILE_ROW_0 {
                    apply_action(
                        StudioAction::StylePrev(focus.row - STYLE_ROW_0),
                        &mut state,
                        &mut library,
                        &mut game_rng,
                        &mut next_state,
                    );
                } else {
                    focus.col = focus.col.saturating_sub(1);
                }
            }
            StudioNavDirection::Right => {
                if focus.row >= MORPH_ROW_0 && focus.row < STYLE_ROW_0 {
                    apply_action(
                        StudioAction::MorphInc(focus.row - MORPH_ROW_0),
                        &mut state,
                        &mut library,
                        &mut game_rng,
                        &mut next_state,
                    );
                } else if focus.row >= STYLE_ROW_0 && focus.row < FILE_ROW_0 {
                    apply_action(
                        StudioAction::StyleNext(focus.row - STYLE_ROW_0),
                        &mut state,
                        &mut library,
                        &mut game_rng,
                        &mut next_state,
                    );
                } else {
                    focus.col += 1;
                }
            }
        }
        keep_studio_focus_in_view(&focus, &focus_rows, &mut left_panel_q, &mut right_panel_q);
    }

    if confirm {
        if let Some(action) = focused_studio_action(&focus, &buttons) {
            apply_action(
                action,
                &mut state,
                &mut library,
                &mut game_rng,
                &mut next_state,
            );
        } else if focus.row >= MORPH_ROW_0 && focus.row < STYLE_ROW_0 {
            apply_action(
                StudioAction::MorphInc(focus.row - MORPH_ROW_0),
                &mut state,
                &mut library,
                &mut game_rng,
                &mut next_state,
            );
        }
    }
}

#[derive(Clone, Copy, Default)]
struct StudioNavInput {
    just: Option<StudioNavDirection>,
    held: Option<StudioNavDirection>,
}

fn studio_nav_input(
    keyboard: &ButtonInput<KeyCode>,
    gamepads: &Query<&Gamepad>,
    native: &NativeControllerState,
) -> StudioNavInput {
    let gamepad_just = |button| gamepads.iter().any(|gamepad| gamepad.just_pressed(button));
    let gamepad_pressed = |button| gamepads.iter().any(|gamepad| gamepad.pressed(button));
    let stick = gamepads
        .iter()
        .map(|gamepad| {
            Vec2::new(
                gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0),
                gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0),
            )
        })
        .find(|axis| axis.length_squared() > 0.36)
        .or_else(|| (native.move_axis.length_squared() > 0.36).then_some(native.move_axis));

    let just = if keyboard.just_pressed(KeyCode::ArrowUp)
        || keyboard.just_pressed(KeyCode::KeyW)
        || gamepad_just(GamepadButton::DPadUp)
        || native.just_pressed(NativeButton::DPadUp)
    {
        Some(StudioNavDirection::Up)
    } else if keyboard.just_pressed(KeyCode::ArrowDown)
        || keyboard.just_pressed(KeyCode::KeyS)
        || gamepad_just(GamepadButton::DPadDown)
        || native.just_pressed(NativeButton::DPadDown)
    {
        Some(StudioNavDirection::Down)
    } else if keyboard.just_pressed(KeyCode::ArrowLeft)
        || keyboard.just_pressed(KeyCode::KeyA)
        || gamepad_just(GamepadButton::DPadLeft)
        || native.just_pressed(NativeButton::DPadLeft)
    {
        Some(StudioNavDirection::Left)
    } else if keyboard.just_pressed(KeyCode::ArrowRight)
        || keyboard.just_pressed(KeyCode::KeyD)
        || gamepad_just(GamepadButton::DPadRight)
        || native.just_pressed(NativeButton::DPadRight)
    {
        Some(StudioNavDirection::Right)
    } else {
        None
    };

    let held = if keyboard.pressed(KeyCode::ArrowUp)
        || keyboard.pressed(KeyCode::KeyW)
        || gamepad_pressed(GamepadButton::DPadUp)
        || native.pressed(NativeButton::DPadUp)
        || stick.is_some_and(|axis| axis.y > 0.6 && axis.y.abs() >= axis.x.abs())
    {
        Some(StudioNavDirection::Up)
    } else if keyboard.pressed(KeyCode::ArrowDown)
        || keyboard.pressed(KeyCode::KeyS)
        || gamepad_pressed(GamepadButton::DPadDown)
        || native.pressed(NativeButton::DPadDown)
        || stick.is_some_and(|axis| axis.y < -0.6 && axis.y.abs() >= axis.x.abs())
    {
        Some(StudioNavDirection::Down)
    } else if keyboard.pressed(KeyCode::ArrowLeft)
        || keyboard.pressed(KeyCode::KeyA)
        || gamepad_pressed(GamepadButton::DPadLeft)
        || native.pressed(NativeButton::DPadLeft)
        || stick.is_some_and(|axis| axis.x < -0.6 && axis.x.abs() > axis.y.abs())
    {
        Some(StudioNavDirection::Left)
    } else if keyboard.pressed(KeyCode::ArrowRight)
        || keyboard.pressed(KeyCode::KeyD)
        || gamepad_pressed(GamepadButton::DPadRight)
        || native.pressed(NativeButton::DPadRight)
        || stick.is_some_and(|axis| axis.x > 0.6 && axis.x.abs() > axis.y.abs())
    {
        Some(StudioNavDirection::Right)
    } else {
        None
    };

    StudioNavInput { just, held }
}

fn repeated_studio_direction(
    repeat: &mut StudioNavRepeat,
    input: StudioNavInput,
    dt: f32,
) -> Option<StudioNavDirection> {
    const INITIAL_REPEAT_DELAY: f32 = 0.34;
    const REPEAT_INTERVAL: f32 = 0.11;

    if let Some(direction) = input.just {
        repeat.direction = Some(direction);
        repeat.timer = INITIAL_REPEAT_DELAY;
        return Some(direction);
    }
    if input.held != repeat.direction {
        repeat.direction = input.held;
        repeat.timer = INITIAL_REPEAT_DELAY;
        return input.held;
    }
    let Some(direction) = input.held else {
        repeat.direction = None;
        repeat.timer = 0.0;
        return None;
    };
    repeat.timer -= dt;
    if repeat.timer <= 0.0 {
        repeat.timer = REPEAT_INTERVAL;
        Some(direction)
    } else {
        None
    }
}

fn focused_studio_action(
    focus: &StudioFocus,
    buttons: &Query<&ActionButton>,
) -> Option<StudioAction> {
    buttons
        .iter()
        .filter(|button| button.row == focus.row)
        .min_by_key(|button| button.col.abs_diff(focus.col))
        .map(|button| button.action)
}

fn keep_studio_focus_in_view(
    focus: &StudioFocus,
    focus_rows: &Query<(&FocusRow, &GlobalTransform, &ComputedNode)>,
    left_panel_q: &mut Query<
        (&GlobalTransform, &ComputedNode, &mut ScrollPosition),
        With<StudioLeftPanel>,
    >,
    right_panel_q: &mut Query<
        (&GlobalTransform, &ComputedNode, &mut ScrollPosition),
        (With<StudioRightPanel>, Without<StudioLeftPanel>),
    >,
) {
    let Some((_, row_transform, row_node)) =
        focus_rows.iter().find(|(row, _, _)| row.0 == focus.row)
    else {
        return;
    };
    if focus.row >= FILE_ROW_0 {
        reveal_studio_row(right_panel_q, row_transform, row_node);
    } else {
        reveal_studio_row(left_panel_q, row_transform, row_node);
    }
}

fn reveal_studio_row<F: bevy::ecs::query::QueryFilter>(
    panel_q: &mut Query<(&GlobalTransform, &ComputedNode, &mut ScrollPosition), F>,
    row_transform: &GlobalTransform,
    row_node: &ComputedNode,
) {
    for (panel_transform, computed, mut scroll_position) in panel_q.iter_mut() {
        let max_y = ((computed.content_size().y - computed.size().y)
            * computed.inverse_scale_factor())
        .max(0.0);
        if max_y <= 0.0 {
            continue;
        }
        let margin = 18.0;
        let panel_center = panel_transform.translation().y;
        let panel_half = computed.size().y * 0.5;
        let visible_min = panel_center - panel_half + margin;
        let visible_max = panel_center + panel_half - margin;
        let row_center = row_transform.translation().y;
        let row_half = row_node.size().y * 0.5;
        let delta = if row_center - row_half < visible_min {
            visible_min - (row_center - row_half)
        } else if row_center + row_half > visible_max {
            visible_max - (row_center + row_half)
        } else {
            0.0
        };
        scroll_position.y = (scroll_position.y + delta).clamp(0.0, max_y);
    }
}

// ── Save library UI ───────────────────────────────────────────────────────────

fn rebuild_library_rows(
    mut commands: Commands,
    mut library: ResMut<SaveLibrary>,
    panel_q: Query<Entity, With<LibraryPanel>>,
    children_q: Query<&Children>,
) {
    if !library.dirty {
        return;
    }
    library.dirty = false;
    let Ok(panel) = panel_q.single() else {
        return;
    };
    if let Ok(children) = children_q.get(panel) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    let files = library.files.clone();
    commands.entity(panel).with_children(|list| {
        if files.is_empty() {
            list.spawn(text(
                "(no saved versions yet)",
                11.0,
                Color::srgb(0.6, 0.6, 0.65),
            ));
        }
        for (i, name) in files.iter().enumerate() {
            let row_idx = FILE_ROW_0 + i;
            list.spawn((
                FocusRow(row_idx),
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(ROW_BG),
            ))
            .with_children(|row| {
                row.spawn(text(
                    name.trim_end_matches(".json"),
                    11.0,
                    Color::srgb(0.85, 0.90, 0.96),
                ));
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    ..default()
                })
                .with_children(|buttons| {
                    buttons
                        .spawn((
                            Button,
                            ActionButton {
                                action: StudioAction::LoadFile(i),
                                row: row_idx,
                                col: 0,
                            },
                            Node {
                                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                                margin: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(BTN_BG),
                        ))
                        .with_children(|b| {
                            b.spawn(text("LOAD", 10.0, Color::srgb(0.7, 1.0, 0.8)));
                        });
                    buttons
                        .spawn((
                            Button,
                            ActionButton {
                                action: StudioAction::DeleteFile(i),
                                row: row_idx,
                                col: 1,
                            },
                            Node {
                                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                                margin: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(BTN_BG),
                        ))
                        .with_children(|b| {
                            b.spawn(text("DEL", 10.0, Color::srgb(1.0, 0.55, 0.5)));
                        });
                });
            });
        }
    });
}

// ── Rebuild preview ───────────────────────────────────────────────────────────

fn rebuild_preview(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<StudioState>,
    previews: Query<Entity, With<StudioPreview>>,
) {
    if !state.dirty {
        return;
    }
    let rebuild_scope = if state.dirty_scope.is_empty() {
        CharacterGeometryScope::ALL
    } else {
        state.dirty_scope
    };
    state.dirty = false;
    state.dirty_scope = CharacterGeometryScope::NONE;
    state.last_rebuild_scope = rebuild_scope;
    state.generation_revision = state.generation_revision.saturating_add(1);
    state.labels_dirty = true;
    for e in previews.iter() {
        commands.entity(e).despawn();
    }
    match state.preview_backend {
        StudioPreviewBackend::Procedural => {
            let patch = build_character_patch(&state.spec);
            let root = human_mesh::spawn_human(
                &mut commands,
                &mut meshes,
                &mut materials,
                &patch,
                Transform::IDENTITY,
            );
            commands.entity(root).insert(StudioPreview);
        }
        StudioPreviewBackend::RiggedAmp => {
            let height = 0.91 + state.spec.body.height * 0.18;
            let width =
                0.88 + state.spec.body.shoulder_width * 0.12 + state.spec.body.muscle * 0.08;
            let depth = 0.90 + state.spec.body.weight * 0.20;
            let scene =
                asset_server.load(GltfAssetLabel::Scene(0).from_asset("Characters/AMP.glb"));
            commands.spawn((
                StudioPreview,
                RiggedPreviewAsset(scene.clone()),
                ImportedHumanoidRig::new("Characters/AMP.glb"),
                ImportedRigStatus::Pending,
                WorldAssetRoot(scene),
                Transform::from_scale(Vec3::new(width, height, depth)),
            ));
        }
    }
}

fn preview_expression(
    state: Res<StudioState>,
    preview_roots: Query<(), With<StudioPreview>>,
    mut parts: Query<(&human_mesh::StudioHumanPart, &mut Transform)>,
) {
    for (part, mut transform) in &mut parts {
        let Some(feature) = part.face_feature else {
            continue;
        };
        if preview_roots.get(part.root).is_err() {
            continue;
        }
        *transform = expression_transform(state.preview_expression, feature, part.rest);
    }
}

fn expression_transform(
    expression: StudioExpression,
    feature: human_mesh::StudioFaceFeature,
    rest: Transform,
) -> Transform {
    use human_mesh::StudioFaceFeature;

    let mut transform = rest;
    match (expression, feature) {
        (StudioExpression::Neutral, _) => {}
        (StudioExpression::Joy, StudioFaceFeature::LeftEye | StudioFaceFeature::RightEye) => {
            transform.scale.y *= 0.52;
            transform.translation.y += 0.006;
        }
        (StudioExpression::Joy, StudioFaceFeature::LeftBrow | StudioFaceFeature::RightBrow) => {
            transform.translation.y += 0.014
        }
        (StudioExpression::Joy, StudioFaceFeature::Mouth) => {
            transform.scale.x *= 1.10;
            transform.scale.y *= 1.22;
        }
        (
            StudioExpression::Determined,
            StudioFaceFeature::LeftEye | StudioFaceFeature::RightEye,
        ) => transform.scale.y *= 0.78,
        (StudioExpression::Determined, StudioFaceFeature::LeftBrow) => {
            transform.rotation *= Quat::from_rotation_z(-0.18);
            transform.translation.y -= 0.005;
        }
        (StudioExpression::Determined, StudioFaceFeature::RightBrow) => {
            transform.rotation *= Quat::from_rotation_z(0.18);
            transform.translation.y -= 0.005;
        }
        (StudioExpression::Determined, StudioFaceFeature::Mouth) => {
            transform.scale.x *= 1.04;
            transform.scale.y *= 0.72;
        }
        (StudioExpression::Surprised, StudioFaceFeature::LeftEye | StudioFaceFeature::RightEye) => {
            transform.scale.x *= 1.06;
            transform.scale.y *= 1.18;
        }
        (
            StudioExpression::Surprised,
            StudioFaceFeature::LeftBrow | StudioFaceFeature::RightBrow,
        ) => transform.translation.y += 0.025,
        (StudioExpression::Surprised, StudioFaceFeature::Mouth) => {
            transform.scale.x *= 0.72;
            transform.scale.y *= 2.15;
        }
    }
    transform
}

fn fallback_failed_rig_preview(
    asset_server: Res<AssetServer>,
    rigged_preview: Query<&RiggedPreviewAsset>,
    mut state: ResMut<StudioState>,
) {
    if state.preview_backend != StudioPreviewBackend::RiggedAmp {
        return;
    }
    let failed = rigged_preview.iter().any(|preview| {
        matches!(
            asset_server.load_state(preview.0.id()),
            LoadState::Failed(_)
        )
    });
    if failed {
        state.preview_backend = StudioPreviewBackend::Procedural;
        state.dirty = true;
        state.dirty_scope = state.dirty_scope.union(CharacterGeometryScope::ALL);
        state.labels_dirty = true;
        state.status = "Rig asset failed to load; generated fallback restored".into();
    }
}

fn refresh_imported_rig_status(
    statuses: Query<(&ImportedHumanoidRig, &ImportedRigStatus), Changed<ImportedRigStatus>>,
    mut state: ResMut<StudioState>,
) {
    for (imported, status) in &statuses {
        state.status = match status {
            ImportedRigStatus::Pending => format!("Loading and validating {}…", imported.source),
            ImportedRigStatus::Ready { mapped_joint_count } => format!(
                "RIG READY • {mapped_joint_count}/{} canonical joints • shared animation enabled",
                crate::components::character::JointKind::HUMANOID.len()
            ),
            ImportedRigStatus::Invalid {
                missing,
                duplicate_targets,
                unresolved_hierarchy,
            } => format!(
                "RIG INVALID • missing {} • duplicates {} • hierarchy errors {}",
                missing.len(),
                duplicate_targets.len(),
                unresolved_hierarchy.len()
            ),
        };
        state.labels_dirty = true;
    }
}

// ── Camera (mouse drag / wheel + gamepad right stick / triggers) ──────────────

fn orbit_camera(
    time: Res<Time>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    tool_windows: Res<ToolWindowPointerState>,
    mut state: ResMut<StudioState>,
    mut cam_q: Query<&mut Transform, With<StudioCamera>>,
) {
    let dt = time.delta_secs();

    // Right-drag orbits; right-mouse wheel zooms. Plain wheel scrolls the GUI.
    let mut drag = Vec2::ZERO;
    for ev in motion.read() {
        if mouse_buttons.pressed(MouseButton::Right) && !tool_windows.captures_viewport() {
            drag += ev.delta;
        }
    }
    state.yaw -= drag.x * 0.008;
    state.pitch = (state.pitch + drag.y * 0.006).clamp(-0.4, 1.2);
    if mouse_buttons.pressed(MouseButton::Right) && !tool_windows.captures_viewport() {
        for ev in wheel.read() {
            state.distance = (state.distance - ev.y * 0.25).clamp(1.2, 8.0);
        }
    } else {
        wheel.clear();
    }

    // Gamepad right stick orbits; triggers zoom.
    for gp in gamepads.iter() {
        let rx = gp.get(GamepadAxis::RightStickX).unwrap_or(0.0);
        let ry = gp.get(GamepadAxis::RightStickY).unwrap_or(0.0);
        if rx.abs() > 0.15 {
            state.yaw -= rx * 1.8 * dt;
        }
        if ry.abs() > 0.15 {
            state.pitch = (state.pitch + ry * 1.2 * dt).clamp(-0.4, 1.2);
        }
        let zoom_in = gp.get(GamepadAxis::RightZ).unwrap_or(0.0);
        let zoom_out = gp.get(GamepadAxis::LeftZ).unwrap_or(0.0);
        state.distance = (state.distance - (zoom_in - zoom_out) * 1.8 * dt).clamp(1.2, 8.0);
    }

    if native.look_axis.x.abs() > 0.15 {
        state.yaw -= native.look_axis.x * 1.8 * dt;
    }
    if native.look_axis.y.abs() > 0.15 {
        state.pitch = (state.pitch + native.look_axis.y * 1.2 * dt).clamp(-0.4, 1.2);
    }
    let native_zoom_in = native.pressed(NativeButton::RightTrigger) as u8 as f32;
    let native_zoom_out = native.pressed(NativeButton::LeftTrigger) as u8 as f32;
    state.distance =
        (state.distance - (native_zoom_in - native_zoom_out) * 1.8 * dt).clamp(1.2, 8.0);

    let focus = Vec3::new(0.0, state.focus_height, 0.0);
    let rot = Quat::from_rotation_y(state.yaw) * Quat::from_rotation_x(-state.pitch);
    let eye = focus + rot * Vec3::new(0.0, 0.0, state.distance);
    for mut tf in cam_q.iter_mut() {
        *tf = Transform::from_translation(eye).looking_at(focus, Vec3::Y);
    }
}

// ── Label + focus refresh ─────────────────────────────────────────────────────

fn refresh_labels(
    mut state: ResMut<StudioState>,
    mut morph_q: Query<
        (&mut Text, &MorphValueText),
        (Without<StyleValueText>, Without<ModelInfoText>),
    >,
    mut fill_q: Query<(&mut Node, &MorphSliderFill), Without<MorphSliderThumb>>,
    mut thumb_q: Query<(&mut Node, &MorphSliderThumb), Without<MorphSliderFill>>,
    mut style_q: Query<
        (&mut Text, &StyleValueText),
        (Without<MorphValueText>, Without<ModelInfoText>),
    >,
    mut swatch_q: Query<(&mut BackgroundColor, &StyleSwatch)>,
    mut model_info_q: Query<
        &mut Text,
        (
            With<ModelInfoText>,
            Without<MorphValueText>,
            Without<StyleValueText>,
            Without<StatusText>,
        ),
    >,
    mut status_q: Query<
        &mut Text,
        (
            With<StatusText>,
            Without<MorphValueText>,
            Without<StyleValueText>,
            Without<ModelInfoText>,
        ),
    >,
) {
    if !state.labels_dirty {
        return;
    }
    state.labels_dirty = false;
    for (mut text, marker) in morph_q.iter_mut() {
        let field = MorphField::ALL[marker.0];
        *text = Text::new(morph_percent_label(field.get(&state.spec)));
    }
    for (mut node, marker) in fill_q.iter_mut() {
        let field = MorphField::ALL[marker.0];
        node.width = Val::Percent(field.get(&state.spec).clamp(0.0, 1.0) * 100.0);
    }
    for (mut node, marker) in thumb_q.iter_mut() {
        let field = MorphField::ALL[marker.0];
        node.left = Val::Percent(field.get(&state.spec).clamp(0.0, 1.0) * 100.0);
    }
    for (mut text, marker) in style_q.iter_mut() {
        let field = StyleField::ALL[marker.0];
        *text = Text::new(field.value_label(&state.spec));
    }
    for (mut background, marker) in swatch_q.iter_mut() {
        background.0 = style_swatch_color(StyleField::ALL[marker.0], &state.spec);
    }
    for mut text in model_info_q.iter_mut() {
        *text = Text::new(model_info_label(
            &state.spec,
            state.preview_backend,
            state.generation_revision,
            state.last_rebuild_scope,
        ));
    }
    for mut text in status_q.iter_mut() {
        *text = Text::new(state.status.clone());
    }
}

fn model_info_label(
    spec: &CharacterSpec,
    backend: StudioPreviewBackend,
    generation_revision: u64,
    rebuild_scope: CharacterGeometryScope,
) -> String {
    let meters = estimated_height(spec);
    let dependency_kind = if rebuild_scope.requires_full_hierarchy_rebuild() {
        "full-layout dependency"
    } else {
        "isolated dependency"
    };
    format!(
        "Backend: {}\nGeneration: {} • {} ({})\nHeight: {:.2} m / {:.0} in\nBuild: muscle {}  mass {}\nFace: jaw {}  eyes {}\nOutfit: {} + {}\nArmor: {}",
        match backend {
            StudioPreviewBackend::Procedural => "Generated / fully editable",
            StudioPreviewBackend::RiggedAmp => "AMP rig diagnostic",
        },
        generation_revision,
        rebuild_scope.summary(),
        dependency_kind,
        meters,
        meters * 39.3701,
        morph_percent_label(spec.body.muscle),
        morph_percent_label(spec.body.weight),
        morph_percent_label(spec.face.jaw_width),
        morph_percent_label(spec.face.eye_size),
        spec.style.wardrobe.top.label(),
        spec.style.wardrobe.bottom.label(),
        spec.style.wardrobe.armor.label(),
    )
}

fn estimated_height(spec: &CharacterSpec) -> f32 {
    let sex_scale = if spec.sex == spec::Sex::Female {
        0.945
    } else {
        1.0
    };
    1.75 * (0.91 + spec.body.height * 0.18) * sex_scale
}

fn refresh_focus_highlight(
    focus: Res<StudioFocus>,
    mut rows: Query<(&FocusRow, &mut BackgroundColor), Without<ActionButton>>,
    mut buttons: Query<(&ActionButton, &mut BackgroundColor, &Interaction), Without<FocusRow>>,
) {
    for (row, mut bg) in rows.iter_mut() {
        *bg = BackgroundColor(if row.0 == focus.row {
            ROW_BG_FOCUS
        } else {
            ROW_BG
        });
    }
    for (button, mut bg, interaction) in buttons.iter_mut() {
        let focused = button.row == focus.row && button.col == focus.col;
        let hovered = matches!(interaction, Interaction::Hovered | Interaction::Pressed);
        *bg = BackgroundColor(if focused || hovered {
            BTN_BG_FOCUS
        } else {
            BTN_BG
        });
    }
}

// ── Versioned save files ──────────────────────────────────────────────────────

fn preset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("presets")
        .join("humans")
}

fn scan_library() -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(preset_dir())
        .map(|dir| {
            dir.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

/// Never overwrites: each save allocates the next `human_vNNN.json`.
fn save_new_version(spec: &CharacterSpec, existing: &[String]) -> Result<String, String> {
    let dir = preset_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let next = existing
        .iter()
        .filter_map(|n| {
            n.strip_prefix("human_v")
                .and_then(|s| s.strip_suffix(".json"))
                .and_then(|s| s.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1;
    let name = format!("human_v{next:03}.json");
    let json = serde_json::to_string_pretty(spec).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&name), json).map_err(|e| e.to_string())?;
    Ok(name)
}

/// Walk the live preview hierarchy and serialize every mesh part to a
/// versioned `.glb` beside the JSON specs. Exports exactly what is on screen.
fn export_glb_system(
    mut state: ResMut<StudioState>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    roots: Query<(Entity, &GlobalTransform), With<StudioPreview>>,
    children_q: Query<&Children>,
    parts: Query<(
        &Mesh3d,
        &MeshMaterial3d<StandardMaterial>,
        &GlobalTransform,
        Option<&Name>,
    )>,
) {
    if !state.export_requested {
        return;
    }
    state.export_requested = false;

    let Ok((root, root_gt)) = roots.single() else {
        state.status = "Export failed: no preview to export".into();
        state.labels_dirty = true;
        return;
    };
    let root_inverse = root_gt.affine().inverse();

    let mut glb_parts = Vec::new();
    let mut part_index = 0usize;
    for entity in children_q.iter_descendants(root) {
        let Ok((mesh3d, material3d, part_gt, name)) = parts.get(entity) else {
            continue;
        };
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue;
        };
        let Some(positions) = float3_attribute(mesh, Mesh::ATTRIBUTE_POSITION) else {
            continue;
        };
        let indices = match mesh.indices() {
            Some(bevy::mesh::Indices::U32(v)) => v.clone(),
            Some(bevy::mesh::Indices::U16(v)) => v.iter().map(|i| *i as u32).collect(),
            None => (0..positions.len() as u32).collect(),
        };
        let (base_color, metallic, roughness) = materials
            .get(&material3d.0)
            .map(|m| {
                let c = m.base_color.to_linear();
                (
                    [c.red, c.green, c.blue, c.alpha],
                    m.metallic,
                    m.perceptual_roughness,
                )
            })
            .unwrap_or(([0.8, 0.8, 0.8, 1.0], 0.0, 0.9));

        part_index += 1;
        glb_parts.push(glb_export::GlbPart {
            name: name
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| format!("part_{part_index:02}")),
            positions,
            normals: float3_attribute(mesh, Mesh::ATTRIBUTE_NORMAL).unwrap_or_default(),
            uvs: float2_attribute(mesh, Mesh::ATTRIBUTE_UV_0).unwrap_or_default(),
            indices,
            base_color,
            metallic,
            roughness,
            matrix: Mat4::from(root_inverse * part_gt.affine()).to_cols_array(),
        });
    }

    match glb_export::build_glb(&glb_parts) {
        Some(bytes) => match save_glb_bytes(&bytes) {
            Ok(name) => {
                state.status = format!("Exported {name} ({} parts)", glb_parts.len());
                state.labels_dirty = true;
            }
            Err(err) => {
                state.status = format!("Export failed: {err}");
                state.labels_dirty = true;
            }
        },
        None => {
            state.status = "Export failed: preview has no mesh parts (rig view?)".into();
            state.labels_dirty = true;
        }
    }
}

fn float3_attribute(mesh: &Mesh, attr: bevy::mesh::MeshVertexAttribute) -> Option<Vec<[f32; 3]>> {
    match mesh.attribute(attr) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) => Some(v.clone()),
        _ => None,
    }
}

fn float2_attribute(mesh: &Mesh, attr: bevy::mesh::MeshVertexAttribute) -> Option<Vec<[f32; 2]>> {
    match mesh.attribute(attr) {
        Some(bevy::mesh::VertexAttributeValues::Float32x2(v)) => Some(v.clone()),
        _ => None,
    }
}

/// Never overwrites: allocates the next `human_vNNN.glb` (numbering is
/// independent of the JSON specs so either can be pruned freely).
fn save_glb_bytes(bytes: &[u8]) -> Result<String, String> {
    let dir = preset_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let next = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter_map(|n| {
                    n.strip_prefix("human_v")
                        .and_then(|s| s.strip_suffix(".glb"))
                        .and_then(|s| s.parse::<u32>().ok())
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
        + 1;
    let name = format!("human_v{next:03}.glb");
    std::fs::write(dir.join(&name), bytes).map_err(|e| e.to_string())?;
    Ok(name)
}

fn load_version(name: &str) -> Result<CharacterSpec, String> {
    let json = std::fs::read_to_string(preset_dir().join(name)).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod playable_tests {
    use super::*;

    #[test]
    fn studio_spec_converts_to_playable_blueprint_profiles() {
        let mut spec = CharacterSpec::default();
        spec.body.height = 0.9;
        spec.body.limb_length = 0.8;
        spec.style.primary_color = 7;
        let blueprint = studio_spec_to_blueprint("Custom Hero", spec);

        assert_eq!(blueprint.name, "Custom Hero");
        assert!(blueprint.body.height > 1.0);
        assert!(blueprint.body.leg_length > 1.0);
        assert_eq!(blueprint.animation_profile.states.len(), 9);
    }

    #[test]
    fn semantic_style_survives_into_the_playable_body_recipe() {
        let neutral = studio_spec_to_blueprint("Neutral", CharacterSpec::default());
        let styled = CharacterSpec {
            style_vector: crate::character::style_space::CharacterStyleVector::STARFALL_HERO,
            ..default()
        };
        let styled = studio_spec_to_blueprint("Styled", styled);

        assert!(styled.body.shoulder_width > neutral.body.shoulder_width);
        assert!(styled.body.hand_size > neutral.body.hand_size);
        assert!(styled.body.foot_size > neutral.body.foot_size);
        assert!(styled.body.head_size > neutral.body.head_size);
    }

    #[test]
    fn expression_preview_is_non_destructive_and_visibly_distinct() {
        let rest = Transform::from_xyz(0.1, 0.2, 0.3);
        let neutral = expression_transform(
            StudioExpression::Neutral,
            human_mesh::StudioFaceFeature::Mouth,
            rest,
        );
        let surprised = expression_transform(
            StudioExpression::Surprised,
            human_mesh::StudioFaceFeature::Mouth,
            rest,
        );
        assert_eq!(neutral, rest);
        assert_eq!(surprised.translation, rest.translation);
        assert!(surprised.scale.y > rest.scale.y * 2.0);
        assert!(surprised.scale.x < rest.scale.x);
    }

    #[test]
    fn action_rows_expose_clear_creator_sections() {
        assert_eq!(ACTION_SECTION_LABELS.len(), ACTION_GROUPS.len());
        assert_eq!(ACTION_SECTION_LABELS[3], Some("FACE PRESETS"));
        assert_eq!(ACTION_SECTION_LABELS[8], Some("EXPRESSION PREVIEW"));
        assert_eq!(
            ACTION_SECTION_LABELS[ACTION_SECTION_LABELS.len() - 1],
            Some("PROJECT")
        );
    }

    #[test]
    fn studio_panels_use_shared_floating_window_chrome() {
        let mut app = App::new();
        {
            let world = app.world_mut();
            let mut commands = world.commands();
            commands.spawn(Node::default()).with_children(|root| {
                spawn_left_panel(root, &StudioState::default());
                spawn_right_panel(root);
            });
        }
        app.world_mut().flush();

        let window_count = app
            .world_mut()
            .query::<&crate::engine_tools::tool_windows::ToolWindow>()
            .iter(app.world())
            .count();
        assert_eq!(window_count, 2);
        let minimized_count = app
            .world_mut()
            .query::<&crate::engine_tools::tool_windows::ToolWindow>()
            .iter(app.world())
            .filter(|window| window.minimized)
            .count();
        assert_eq!(minimized_count, 0);
        assert_eq!(
            app.world_mut()
                .query::<&StudioLeftPanel>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&StudioRightPanel>()
                .iter(app.world())
                .count(),
            1
        );
    }
}
