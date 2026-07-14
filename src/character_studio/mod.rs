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
pub mod human_mesh;
pub mod spec;

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use std::path::PathBuf;

use crate::state::AppState;
use generators::build_character_patch;
use spec::{CharacterSpec, MorphField, StyleField};

pub struct CharacterStudioPlugin;

impl Plugin for CharacterStudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StudioState>()
            .init_resource::<StudioFocus>()
            .init_resource::<StudioSliderDrag>()
            .init_resource::<SaveLibrary>()
            .add_systems(OnEnter(AppState::CharacterStudio), setup_studio)
            .add_systems(OnExit(AppState::CharacterStudio), cleanup_studio)
            .add_systems(
                Update,
                (
                    button_interaction,
                    morph_slider_interaction,
                    rebuild_library_rows,
                    rebuild_preview,
                    orbit_camera,
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
    Athletic,
    Heavy,
    Slim,
    SoftFace,
    SharpFace,
    Random,
    Undo,
    SaveVersion,
    Back,
    MorphDec(usize),
    MorphInc(usize),
    StylePrev(usize),
    StyleNext(usize),
    LoadFile(usize),
    DeleteFile(usize),
}

/// Focusable rows. Action groups hold several buttons, while morph/style rows
/// expose their own decrement/increment buttons.
const ACTION_GROUPS: [&[StudioAction]; 5] = [
    &[StudioAction::Male, StudioAction::Female],
    &[
        StudioAction::Athletic,
        StudioAction::Heavy,
        StudioAction::Slim,
    ],
    &[StudioAction::SoftFace, StudioAction::SharpFace],
    &[StudioAction::Random, StudioAction::Undo],
    &[StudioAction::SaveVersion, StudioAction::Back],
];
const MORPH_ROW_0: usize = ACTION_GROUPS.len(); // 5
const STYLE_ROW_0: usize = MORPH_ROW_0 + MorphField::ALL.len(); // 20
const FILE_ROW_0: usize = STYLE_ROW_0 + StyleField::ALL.len(); // 32

fn action_label(action: StudioAction) -> &'static str {
    match action {
        StudioAction::Male => "MALE",
        StudioAction::Female => "FEMALE",
        StudioAction::Athletic => "ATHLETIC",
        StudioAction::Heavy => "HEAVY",
        StudioAction::Slim => "SLIM",
        StudioAction::SoftFace => "SOFT FACE",
        StudioAction::SharpFace => "SHARP FACE",
        StudioAction::Random => "RANDOM",
        StudioAction::Undo => "UNDO",
        StudioAction::SaveVersion => "SAVE VERSION",
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
    labels_dirty: bool,
    yaw: f32,
    pitch: f32,
    distance: f32,
    status: String,
}

impl Default for StudioState {
    fn default() -> Self {
        Self {
            spec: generators::preset_male(),
            undo: Vec::new(),
            dirty: true,
            labels_dirty: true,
            yaw: 0.35,
            pitch: 0.12,
            distance: 3.4,
            status: "Welcome to the Character Studio".to_string(),
        }
    }
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
struct StatusText;

#[derive(Component)]
struct LibraryPanel;

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
) {
    state.dirty = true;
    state.labels_dirty = true;
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

    spawn_left_panel(&mut commands, &state);
    spawn_right_panel(&mut commands);
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
                width: Val::Px(24.0),
                height: Val::Px(20.0),
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
                width: Val::Px(118.0),
                height: Val::Px(20.0),
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

fn spawn_left_panel(commands: &mut Commands, state: &StudioState) {
    commands
        .spawn((
            StudioRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(8.0),
                top: Val::Px(8.0),
                bottom: Val::Px(8.0),
                width: Val::Px(340.0),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.90)),
        ))
        .with_children(|panel| {
            panel.spawn(text("CHARACTER STUDIO", 16.0, Color::srgb(0.65, 0.85, 1.0)));
            {
                let (t, f, c) = text("", 11.0, Color::srgb(0.95, 0.85, 0.4));
                panel.spawn((StatusText, t, f, c));
            }

            section_label(panel, "PRESETS");
            for (g, actions) in ACTION_GROUPS.iter().enumerate() {
                panel
                    .spawn((
                        FocusRow(g),
                        Node {
                            flex_direction: FlexDirection::Row,
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
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::SpaceBetween,
                            padding: UiRect::axes(Val::Px(4.0), Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(ROW_BG),
                    ))
                    .with_children(|row| {
                        row.spawn(Node {
                            width: Val::Px(110.0),
                            ..default()
                        })
                        .with_children(|n| {
                            n.spawn(text(field.label(), 12.0, Color::srgb(0.85, 0.90, 0.96)));
                        });
                        row.spawn(Node {
                            flex_direction: FlexDirection::Row,
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
                                    width: Val::Px(42.0),
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
                                "+",
                                StudioAction::MorphInc(i),
                                row_idx,
                                2,
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
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::SpaceBetween,
                            padding: UiRect::axes(Val::Px(4.0), Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(ROW_BG),
                    ))
                    .with_children(|row| {
                        row.spawn(Node {
                            width: Val::Px(110.0),
                            ..default()
                        })
                        .with_children(|n| {
                            n.spawn(text(field.label(), 12.0, Color::srgb(0.85, 0.90, 0.96)));
                        });
                        row.spawn(Node {
                            flex_direction: FlexDirection::Row,
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
                                    width: Val::Px(96.0),
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
                            spawn_small_button(
                                controls,
                                ">",
                                StudioAction::StyleNext(i),
                                row_idx,
                                1,
                            );
                        });
                    });
            }
        });
}

fn spawn_right_panel(commands: &mut Commands) {
    commands
        .spawn((
            StudioRoot,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(8.0),
                top: Val::Px(8.0),
                width: Val::Px(250.0),
                max_height: Val::Percent(85.0),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.90)),
        ))
        .with_children(|panel| {
            panel.spawn(text("SAVED VERSIONS", 14.0, Color::srgb(0.65, 0.85, 1.0)));
            panel.spawn((
                LibraryPanel,
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
            ));
        });
}

fn morph_percent_label(v: f32) -> String {
    format!("{:>3}%", (v.clamp(0.0, 1.0) * 100.0).round() as u8)
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

// ── Action dispatch ───────────────────────────────────────────────────────────

fn apply_action(
    action: StudioAction,
    state: &mut StudioState,
    library: &mut SaveLibrary,
    next_state: &mut NextState<AppState>,
) {
    let mark = |state: &mut StudioState, msg: String| {
        state.dirty = true;
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
            mark(state, "Preset: Soft Face".into());
        }
        StudioAction::SharpFace => {
            state.push_undo();
            generators::apply_sharp_face(&mut state.spec);
            mark(state, "Preset: Sharp Face".into());
        }
        StudioAction::Random => {
            state.push_undo();
            let mut spec = state.spec;
            generators::randomize(&mut spec);
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
}

// ── Mouse ─────────────────────────────────────────────────────────────────────

fn button_interaction(
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
                apply_action(button.action, &mut state, &mut library, &mut next_state);
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

        field.set(&mut state.spec, next_value);
        state.dirty = true;
        state.labels_dirty = true;
        state.status = format!("{} = {:.2}", field.label(), field.get(&state.spec));
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<StudioState>,
    previews: Query<Entity, With<StudioPreview>>,
) {
    if !state.dirty {
        return;
    }
    state.dirty = false;
    for e in previews.iter() {
        commands.entity(e).despawn();
    }
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

// ── Camera (mouse drag / wheel + gamepad right stick / triggers) ──────────────

fn orbit_camera(
    time: Res<Time>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    gamepads: Query<&Gamepad>,
    mut state: ResMut<StudioState>,
    mut cam_q: Query<&mut Transform, With<StudioCamera>>,
) {
    let dt = time.delta_secs();

    // Right-drag orbits; wheel zooms.
    let mut drag = Vec2::ZERO;
    for ev in motion.read() {
        if mouse_buttons.pressed(MouseButton::Right) {
            drag += ev.delta;
        }
    }
    state.yaw -= drag.x * 0.008;
    state.pitch = (state.pitch + drag.y * 0.006).clamp(-0.4, 1.2);
    for ev in wheel.read() {
        state.distance = (state.distance - ev.y * 0.25).clamp(1.2, 8.0);
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

    let focus = Vec3::new(0.0, 0.95, 0.0);
    let rot = Quat::from_rotation_y(state.yaw) * Quat::from_rotation_x(-state.pitch);
    let eye = focus + rot * Vec3::new(0.0, 0.0, state.distance);
    for mut tf in cam_q.iter_mut() {
        *tf = Transform::from_translation(eye).looking_at(focus, Vec3::Y);
    }
}

// ── Label + focus refresh ─────────────────────────────────────────────────────

fn refresh_labels(
    mut state: ResMut<StudioState>,
    mut morph_q: Query<(&mut Text, &MorphValueText), Without<StyleValueText>>,
    mut fill_q: Query<(&mut Node, &MorphSliderFill), Without<MorphSliderThumb>>,
    mut thumb_q: Query<(&mut Node, &MorphSliderThumb), Without<MorphSliderFill>>,
    mut style_q: Query<(&mut Text, &StyleValueText), Without<MorphValueText>>,
    mut status_q: Query<
        &mut Text,
        (
            With<StatusText>,
            Without<MorphValueText>,
            Without<StyleValueText>,
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
    for mut text in status_q.iter_mut() {
        *text = Text::new(state.status.clone());
    }
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

fn load_version(name: &str) -> Result<CharacterSpec, String> {
    let json = std::fs::read_to_string(preset_dir().join(name)).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}
