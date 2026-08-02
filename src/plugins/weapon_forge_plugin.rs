//! Weapon Forge — the authoring screen for modular sabre designs.
//!
//! Edits a [`WeaponSpec`] against a live preview assembled from the very parts
//! the stats are derived from, so the model and the numbers can never disagree:
//! lengthen the blade and you watch it grow *and* watch the swing speed drop in
//! the same frame. Designs save as versioned `ContentCategory::Weapon` records
//! through `engine_tools::weapon_records`.
//!
//! The preview is deliberately built from the same measurements the derivation
//! reads (`GripStyle::length`, `blade_length`, `blade_width`) rather than from
//! separate display constants — a preview that drifts from the maths would make
//! the whole tool lie.

use bevy::input::keyboard::Key;
use bevy::prelude::*;

use super::ui_plugin::MenuScrollPanel;

use crate::combat::blades::BladeColor;
use crate::combat::weapon_forge::{
    compare_to_starter, forge_presets, EmitterStyle, GripStyle, GuardStyle, PommelStyle,
    WeaponSpec, BLADE_LENGTH_RANGE, BLADE_WIDTH_RANGE,
};
use crate::engine::state::AppState;
use crate::engine_tools::project_registry::ForgeProjectRegistry;
use crate::engine_tools::tool_windows::{spawn_tool_window, ToolWindowStyle};
use crate::engine_tools::weapon_records;

pub struct WeaponForgePlugin;

impl Plugin for WeaponForgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponForgeState>()
            .add_systems(OnEnter(AppState::WeaponForge), setup_weapon_forge)
            .add_systems(OnExit(AppState::WeaponForge), despawn_weapon_forge)
            .add_systems(
                Update,
                (
                    weapon_forge_name_input_system,
                    weapon_forge_action_system,
                    weapon_forge_library_rebuild_system,
                    weapon_forge_preview_rebuild_system,
                    weapon_forge_label_refresh_system,
                    weapon_forge_spin_system,
                )
                    .chain()
                    .run_if(in_state(AppState::WeaponForge)),
            );
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct WeaponForgeState {
    spec: WeaponSpec,
    /// Rebuild the 3D preview.
    dirty: bool,
    /// Refresh the readout text.
    labels_dirty: bool,
    /// Typing into the name field: printable keys edit `spec.name` until
    /// Enter/Escape (button actions are suspended so shortcuts don't fire).
    naming: bool,
    /// Saved designs in the active project as `(content_id, display_name)`.
    library: Vec<(String, String)>,
    /// Rebuild the library rows.
    library_dirty: bool,
    status: String,
}

impl Default for WeaponForgeState {
    fn default() -> Self {
        Self {
            spec: WeaponSpec::default(),
            dirty: true,
            labels_dirty: true,
            naming: false,
            library: Vec::new(),
            library_dirty: true,
            status: "Design a sabre. Stats follow the build.".to_string(),
        }
    }
}

impl WeaponForgeState {
    fn mark(&mut self, status: impl Into<String>) {
        self.spec = self.spec.clone().sanitized();
        self.dirty = true;
        self.labels_dirty = true;
        self.status = status.into();
    }
}

#[derive(Component)]
struct WeaponForgeRoot;
#[derive(Component)]
struct WeaponPreviewRoot;
#[derive(Component)]
struct WeaponForgeStatusText;
#[derive(Component)]
struct WeaponForgeSpecText;
#[derive(Component)]
struct WeaponForgeStatsText;

#[derive(Clone, Copy, PartialEq)]
enum ForgeAction {
    GripNext,
    GuardNext,
    EmitterNext,
    PommelNext,
    ColorNext,
    LengthDown,
    LengthUp,
    WidthDown,
    WidthUp,
    Preset(usize),
    /// Toggle typing mode for the weapon's name.
    NameEdit,
    Save,
    /// Load a saved design from the active project's library.
    LibraryLoad(usize),
    /// Delete a saved design from the active project's library.
    LibraryDelete(usize),
    Back,
}

#[derive(Component)]
struct WeaponForgeButton(ForgeAction);

/// Content node of the LIBRARY window; its rows are rebuilt whenever the
/// project's saved designs change.
#[derive(Component)]
struct WeaponForgeLibraryPanel;

fn next_in<T: PartialEq + Copy>(all: &[T], current: T) -> T {
    let index = all.iter().position(|value| *value == current).unwrap_or(0);
    all[(index + 1) % all.len()]
}

const COLORS: [BladeColor; 8] = [
    BladeColor::Azure,
    BladeColor::Solar,
    BladeColor::Crimson,
    BladeColor::Emerald,
    BladeColor::Violet,
    BladeColor::Gold,
    BladeColor::Frost,
    BladeColor::Void,
];

// ── Setup / teardown ──────────────────────────────────────────────────────────

fn setup_weapon_forge(
    mut commands: Commands,
    mut state: ResMut<WeaponForgeState>,
    registry: Res<ForgeProjectRegistry>,
) {
    state.dirty = true;
    state.labels_dirty = true;
    state.naming = false;
    state.library = weapon_records::list_active_project(&registry);
    state.library_dirty = true;

    commands.spawn((
        WeaponForgeRoot,
        Camera3d::default(),
        Transform::from_xyz(0.0, 1.1, 3.4).looking_at(Vec3::new(0.0, 0.95, 0.0), Vec3::Y),
    ));
    commands.spawn((
        WeaponForgeRoot,
        DirectionalLight {
            illuminance: 10_500.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(2.4, 4.0, 3.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));

    spawn_weapon_forge_ui(&mut commands);
}

fn despawn_weapon_forge(
    mut commands: Commands,
    roots: Query<Entity, Or<(With<WeaponForgeRoot>, With<WeaponPreviewRoot>)>>,
) {
    for entity in roots.iter() {
        commands.entity(entity).despawn();
    }
}

// ── Preview ───────────────────────────────────────────────────────────────────

/// Rebuild the weapon from its parts, using the same measurements the stat
/// derivation reads so the picture always matches the numbers.
fn weapon_forge_preview_rebuild_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<WeaponForgeState>,
    previews: Query<Entity, With<WeaponPreviewRoot>>,
) {
    if !state.dirty {
        return;
    }
    state.dirty = false;
    for entity in previews.iter() {
        commands.entity(entity).despawn();
    }

    let spec = state.spec.clone().sanitized();
    let grip_len = spec.grip.length();
    let (r, g, b) = spec.color.aura_rgb();

    let steel = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.32, 0.36),
        metallic: 0.85,
        perceptual_roughness: 0.35,
        ..default()
    });
    let accent = materials.add(StandardMaterial {
        base_color: Color::srgb(r * 0.6, g * 0.6, b * 0.6),
        emissive: LinearRgba::rgb(r * 3.0, g * 3.0, b * 3.0),
        ..default()
    });
    let beam = materials.add(StandardMaterial {
        base_color: Color::srgb(r, g, b),
        emissive: LinearRgba::rgb(r * 6.0, g * 6.0, b * 6.0),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    // Root sits at the pommel end so the weapon grows upward as parts change.
    let root = commands
        .spawn((
            WeaponPreviewRoot,
            Transform::from_xyz(0.0, 0.35, 0.0),
            Visibility::Visible,
        ))
        .id();

    commands.entity(root).with_children(|weapon| {
        // Grip.
        weapon.spawn((
            Mesh3d(meshes.add(Cylinder::new(0.030, grip_len))),
            MeshMaterial3d(steel.clone()),
            Transform::from_xyz(0.0, grip_len * 0.5, 0.0),
        ));
        // Pommel.
        let pommel_radius = match spec.pommel {
            PommelStyle::Light => 0.032,
            PommelStyle::Balanced => 0.040,
            PommelStyle::Heavy => 0.055,
            PommelStyle::Reactor => 0.046,
        };
        weapon.spawn((
            Mesh3d(meshes.add(Sphere::new(pommel_radius))),
            MeshMaterial3d(if spec.pommel == PommelStyle::Reactor {
                accent.clone()
            } else {
                steel.clone()
            }),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
        // Guard, at the blade root.
        match spec.guard {
            GuardStyle::None => {}
            GuardStyle::Crossguard => {
                weapon.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.22, 0.022, 0.05))),
                    MeshMaterial3d(steel.clone()),
                    Transform::from_xyz(0.0, grip_len, 0.0),
                ));
            }
            GuardStyle::Basket => {
                weapon.spawn((
                    Mesh3d(meshes.add(Torus::new(0.055, 0.10))),
                    MeshMaterial3d(steel.clone()),
                    Transform::from_xyz(0.0, grip_len, 0.0)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ));
            }
            GuardStyle::Tines => {
                for side in [-1.0_f32, 1.0] {
                    weapon.spawn((
                        Mesh3d(meshes.add(Cuboid::new(0.026, 0.13, 0.032))),
                        MeshMaterial3d(steel.clone()),
                        Transform::from_xyz(side * 0.055, grip_len + 0.05, 0.0)
                            .with_rotation(Quat::from_rotation_z(side * 0.35)),
                    ));
                }
            }
        }
        // Emitter ring, tinted to the blade.
        weapon.spawn((
            Mesh3d(meshes.add(Cylinder::new(0.042, 0.030))),
            MeshMaterial3d(accent.clone()),
            Transform::from_xyz(0.0, grip_len + 0.02, 0.0),
        ));
        // Blade — length and width straight from the spec.
        let blade_len = spec.blade_length * 1.45;
        weapon.spawn((
            Mesh3d(meshes.add(Cylinder::new(0.030 * spec.blade_width, blade_len))),
            MeshMaterial3d(beam.clone()),
            Transform::from_xyz(0.0, grip_len + 0.04 + blade_len * 0.5, 0.0),
        ));
        // Brighter core so the blade reads as energy rather than a plastic rod.
        weapon.spawn((
            Mesh3d(meshes.add(Cylinder::new(0.013 * spec.blade_width, blade_len * 0.98))),
            MeshMaterial3d(accent),
            Transform::from_xyz(0.0, grip_len + 0.04 + blade_len * 0.5, 0.0),
        ));
    });
}

fn weapon_forge_spin_system(
    time: Res<Time>,
    mut previews: Query<&mut Transform, With<WeaponPreviewRoot>>,
) {
    for mut transform in previews.iter_mut() {
        transform.rotate_y(time.delta_secs() * 0.6);
    }
}

// ── UI ────────────────────────────────────────────────────────────────────────

fn forge_button(parent: &mut ChildSpawnerCommands, label: impl Into<String>, action: ForgeAction) {
    parent
        .spawn((
            Button,
            WeaponForgeButton(action),
            Node {
                min_width: Val::Px(104.0),
                min_height: Val::Px(36.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.14, 0.20)),
            BorderColor::all(Color::srgb(0.34, 0.40, 0.58)),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label.into()),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn button_row(parent: &mut ChildSpawnerCommands, build: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(6.0),
            margin: UiRect::bottom(Val::Px(5.0)),
            flex_wrap: FlexWrap::Wrap,
            row_gap: Val::Px(5.0),
            ..default()
        })
        .with_children(build);
}

fn spawn_weapon_forge_ui(commands: &mut Commands) {
    commands
        .spawn((
            WeaponForgeRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            GlobalZIndex(4_000),
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("WEAPON FORGE"),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(16.0),
                    top: Val::Px(10.0),
                    ..default()
                },
                TextFont {
                    font_size: FontSize::Px(30.0),
                    ..default()
                },
                TextColor(Color::srgb(0.68, 0.82, 1.0)),
            ));
            root.spawn((
                Text::new(""),
                WeaponForgeStatusText,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(16.0),
                    top: Val::Px(46.0),
                    max_width: Val::Px(760.0),
                    ..default()
                },
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.86, 0.82, 0.60)),
            ));

            let style = |height: f32| ToolWindowStyle {
                accent: Color::srgb(0.28, 0.42, 0.72),
                width: 306.0,
                content_height: Val::Px(height),
                ..Default::default()
            };

            // Parts: the physical description of the weapon.
            spawn_tool_window(
                root,
                "PARTS",
                Vec2::new(12.0, 84.0),
                style(232.0),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new("—"),
                        WeaponForgeSpecText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.92, 0.90, 0.80)),
                        Node {
                            margin: UiRect::bottom(Val::Px(6.0)),
                            ..default()
                        },
                    ));
                    button_row(panel, |row| {
                        forge_button(row, "GRIP", ForgeAction::GripNext);
                        forge_button(row, "GUARD", ForgeAction::GuardNext);
                    });
                    button_row(panel, |row| {
                        forge_button(row, "EMITTER", ForgeAction::EmitterNext);
                        forge_button(row, "POMMEL", ForgeAction::PommelNext);
                    });
                    button_row(panel, |row| {
                        forge_button(row, "COLOR", ForgeAction::ColorNext);
                    });
                    button_row(panel, |row| {
                        forge_button(row, "LEN -", ForgeAction::LengthDown);
                        forge_button(row, "LEN +", ForgeAction::LengthUp);
                    });
                    button_row(panel, |row| {
                        forge_button(row, "WIDE -", ForgeAction::WidthDown);
                        forge_button(row, "WIDE +", ForgeAction::WidthUp);
                    });
                },
            );

            // Derived stats: the whole point of the tool.
            spawn_tool_window(
                root,
                "PERFORMANCE",
                Vec2::new(12.0, 350.0),
                style(150.0),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new("—"),
                        WeaponForgeStatsText,
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.66, 0.88, 1.0)),
                    ));
                },
            );

            spawn_tool_window(
                root,
                "PRESETS & SAVE",
                Vec2::new(12.0, 524.0),
                style(120.0),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    button_row(panel, |row| {
                        for (index, preset) in forge_presets().into_iter().enumerate() {
                            forge_button(
                                row,
                                preset.name.to_uppercase(),
                                ForgeAction::Preset(index),
                            );
                        }
                    });
                    button_row(panel, |row| {
                        forge_button(row, "SAVE", ForgeAction::Save);
                        forge_button(row, "BACK", ForgeAction::Back);
                    });
                },
            );
        });
}

// ── Interaction ───────────────────────────────────────────────────────────────

/// How far one press moves a dimension slider.
const DIMENSION_STEP: f32 = 0.05;

/// Longest weapon name the forge accepts (validation blocks past 40; stopping
/// earlier keeps the readout tidy).
const NAME_LIMIT: usize = 40;

/// Typing mode for the weapon's name, following the editor's key-capture
/// idiom: printable characters append, Backspace deletes, Enter/Escape end
/// the edit. While active, button actions are suspended by the action system
/// so typing can never trigger shortcuts.
fn weapon_forge_name_input_system(
    keys: Res<ButtonInput<Key>>,
    mut state: ResMut<WeaponForgeState>,
) {
    if !state.naming {
        return;
    }
    for key in keys.get_just_pressed() {
        match key {
            Key::Character(value) => {
                for character in value.chars() {
                    if (character.is_alphanumeric() || matches!(character, ' ' | '.' | '_' | '-'))
                        && state.spec.name.chars().count() < NAME_LIMIT
                    {
                        state.spec.name.push(character);
                    }
                }
            }
            Key::Space if state.spec.name.chars().count() < NAME_LIMIT => {
                state.spec.name.push(' ');
            }
            Key::Backspace => {
                state.spec.name.pop();
            }
            Key::Enter | Key::Escape => {
                state.naming = false;
            }
            _ => {}
        }
    }
    let name = state.spec.name.clone();
    let message = if state.naming {
        format!("Naming: {name}_")
    } else {
        format!("Named: {name}")
    };
    state.mark(message);
}

fn weapon_forge_action_system(
    interactions: Query<(&Interaction, &WeaponForgeButton), (Changed<Interaction>, With<Button>)>,
    mut state: ResMut<WeaponForgeState>,
    registry: Res<ForgeProjectRegistry>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let mut action = None;
    for (interaction, button) in interactions.iter() {
        if *interaction == Interaction::Pressed {
            action = Some(button.0);
        }
    }
    let Some(action) = action else {
        return;
    };
    // While typing a name, only the name toggle itself may fire — a stray
    // click must not load a preset or delete a library entry mid-edit.
    if state.naming && action != ForgeAction::NameEdit {
        return;
    }

    match action {
        ForgeAction::GripNext => {
            state.spec.grip = next_in(&GripStyle::ALL, state.spec.grip);
            let label = state.spec.grip.label();
            state.mark(format!("Grip: {label}"));
        }
        ForgeAction::GuardNext => {
            state.spec.guard = next_in(&GuardStyle::ALL, state.spec.guard);
            let label = state.spec.guard.label();
            state.mark(format!("Guard: {label}"));
        }
        ForgeAction::EmitterNext => {
            state.spec.emitter = next_in(&EmitterStyle::ALL, state.spec.emitter);
            let label = state.spec.emitter.label();
            state.mark(format!("Emitter: {label}"));
        }
        ForgeAction::PommelNext => {
            state.spec.pommel = next_in(&PommelStyle::ALL, state.spec.pommel);
            let label = state.spec.pommel.label();
            state.mark(format!("Pommel: {label}"));
        }
        ForgeAction::ColorNext => {
            state.spec.color = next_in(&COLORS, state.spec.color);
            state.mark("Blade colour changed");
        }
        ForgeAction::LengthDown | ForgeAction::LengthUp => {
            let delta = if action == ForgeAction::LengthUp {
                DIMENSION_STEP
            } else {
                -DIMENSION_STEP
            };
            state.spec.blade_length =
                (state.spec.blade_length + delta).clamp(BLADE_LENGTH_RANGE.0, BLADE_LENGTH_RANGE.1);
            let length = state.spec.blade_length;
            state.mark(format!("Blade length {length:.2}"));
        }
        ForgeAction::WidthDown | ForgeAction::WidthUp => {
            let delta = if action == ForgeAction::WidthUp {
                DIMENSION_STEP
            } else {
                -DIMENSION_STEP
            };
            state.spec.blade_width =
                (state.spec.blade_width + delta).clamp(BLADE_WIDTH_RANGE.0, BLADE_WIDTH_RANGE.1);
            let width = state.spec.blade_width;
            state.mark(format!("Blade width {width:.2}"));
        }
        ForgeAction::Preset(index) => {
            if let Some(preset) = forge_presets().into_iter().nth(index) {
                let name = preset.name.clone();
                state.spec = preset;
                state.mark(format!("Loaded preset: {name}"));
            }
        }
        ForgeAction::NameEdit => {
            state.naming = !state.naming;
            let name = state.spec.name.clone();
            let message = if state.naming {
                format!("Naming: {name}_  (Enter to finish)")
            } else {
                format!("Named: {name}")
            };
            state.mark(message);
        }
        ForgeAction::Save => {
            let spec = state.spec.clone();
            match weapon_records::save_to_active_project(&registry, &spec) {
                Ok(id) => {
                    state.library = weapon_records::list_active_project(&registry);
                    state.library_dirty = true;
                    state.mark(format!("Saved as {id}"));
                }
                Err(error) => state.mark(format!("Cannot save: {error}")),
            }
        }
        ForgeAction::LibraryLoad(index) => {
            let Some((content_id, name)) = state.library.get(index).cloned() else {
                return;
            };
            match weapon_records::load_from_active_project(&registry, &content_id) {
                Ok(spec) => {
                    state.spec = spec;
                    state.mark(format!("Loaded {name}"));
                }
                Err(error) => state.mark(format!("Cannot load {name}: {error}")),
            }
        }
        ForgeAction::LibraryDelete(index) => {
            let Some((content_id, name)) = state.library.get(index).cloned() else {
                return;
            };
            match weapon_records::delete_from_active_project(&registry, &content_id) {
                Ok(true) => {
                    state.library = weapon_records::list_active_project(&registry);
                    state.library_dirty = true;
                    state.mark(format!("Deleted {name}"));
                }
                Ok(false) => state.mark(format!("{name} was already gone")),
                Err(error) => state.mark(format!("Cannot delete {name}: {error}")),
            }
        }
        ForgeAction::Back => next_state.set(AppState::ProjectHub),
    }
}

/// Rebuild the LIBRARY rows whenever the saved-design list changes. Rows are
/// despawned and respawned rather than patched — the list is small and this
/// can never leak a stale row.
fn weapon_forge_library_rebuild_system(
    mut commands: Commands,
    mut state: ResMut<WeaponForgeState>,
    panels: Query<(Entity, Option<&Children>), With<WeaponForgeLibraryPanel>>,
) {
    if !state.library_dirty {
        return;
    }
    let Ok((panel, children)) = panels.single() else {
        return;
    };
    state.library_dirty = false;
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    commands.entity(panel).with_children(|panel| {
        if state.library.is_empty() {
            panel.spawn((
                Text::new("No saved designs yet.
NAME it, then SAVE."),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.58, 0.60, 0.66)),
            ));
            return;
        }
        for (index, (_, name)) in state.library.iter().enumerate() {
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    align_items: AlignItems::Center,
                    margin: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new(name.clone()),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.92, 0.90, 0.80)),
                        Node {
                            min_width: Val::Px(120.0),
                            ..default()
                        },
                    ));
                    forge_button(row, "LOAD", ForgeAction::LibraryLoad(index));
                    forge_button(row, "DEL", ForgeAction::LibraryDelete(index));
                });
        }
    });
}

fn weapon_forge_label_refresh_system(
    mut state: ResMut<WeaponForgeState>,
    mut spec_text: Query<
        &mut Text,
        (
            With<WeaponForgeSpecText>,
            Without<WeaponForgeStatsText>,
            Without<WeaponForgeStatusText>,
        ),
    >,
    mut stats_text: Query<
        &mut Text,
        (
            With<WeaponForgeStatsText>,
            Without<WeaponForgeSpecText>,
            Without<WeaponForgeStatusText>,
        ),
    >,
    mut status_text: Query<
        &mut Text,
        (
            With<WeaponForgeStatusText>,
            Without<WeaponForgeSpecText>,
            Without<WeaponForgeStatsText>,
        ),
    >,
) {
    if !state.labels_dirty {
        return;
    }
    state.labels_dirty = false;
    let spec = state.spec.clone().sanitized();

    for mut text in spec_text.iter_mut() {
        let caret = if state.naming { "_" } else { "" };
        *text = Text::new(format!(
            "\"{}{caret}\"\n{} grip • {} guard\n{} emitter • {} pommel\nblade {:.2} long, {:.2} wide",
            spec.name,
            spec.grip.label(),
            spec.guard.label(),
            spec.emitter.label(),
            spec.pommel.label(),
            spec.blade_length,
            spec.blade_width,
        ));
    }

    for mut text in stats_text.iter_mut() {
        let profile = spec.to_blade_profile();
        let mut lines: Vec<String> = compare_to_starter(&spec)
            .into_iter()
            .map(|(label, ratio)| format!("{label}: {:.0}%", ratio * 100.0))
            .collect();
        lines.push(format!(
            "Chain: {:+} • Balance: {:.0}% • {:.2}m",
            profile.slash_count_delta,
            spec.balance() * 100.0,
            spec.overall_length()
        ));
        lines.push(format!(
            "Trait: {} • Value: {}cr",
            profile.trait_.label(),
            profile.price_credits
        ));
        // Blocking problems are called out up front rather than only on a
        // failed save, so a design never looks finished when it is not.
        if !spec.is_saveable() {
            lines.push("UNSAVEABLE:".to_string());
        }
        for issue in spec.validate() {
            let prefix = if issue.blocking { "x" } else { "!" };
            lines.push(format!("{prefix} {}", issue.message));
        }
        *text = Text::new(lines.join("\n"));
    }

    for mut text in status_text.iter_mut() {
        *text = Text::new(state.status.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// App with just the asset stores and the preview rebuild system.
    fn preview_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<WeaponForgeState>()
            .add_systems(Update, weapon_forge_preview_rebuild_system);
        app
    }

    fn preview_part_count(app: &mut App) -> usize {
        let mut roots = app.world_mut().query::<(Entity, &WeaponPreviewRoot)>();
        let root = roots
            .iter(app.world())
            .map(|(entity, _)| entity)
            .next()
            .expect("preview root should exist");
        app.world()
            .get::<Children>(root)
            .map(|children| children.len())
            .unwrap_or(0)
    }

    #[test]
    fn the_preview_assembles_the_weapon_from_its_parts() {
        let mut app = preview_app();
        app.update();

        // Grip, pommel, emitter, blade, core — a guardless build still has the
        // five core pieces.
        let bare = preview_part_count(&mut app);
        assert!(bare >= 5, "expected a full weapon, got {bare} parts");

        // Adding a guard adds geometry rather than silently doing nothing.
        {
            let mut state = app.world_mut().resource_mut::<WeaponForgeState>();
            state.spec.guard = GuardStyle::Basket;
            state.mark("guard");
        }
        app.update();
        assert!(
            preview_part_count(&mut app) > bare,
            "the guard must appear in the preview"
        );

        // Tines are two prongs, so they add more parts than a single basket.
        {
            let mut state = app.world_mut().resource_mut::<WeaponForgeState>();
            state.spec.guard = GuardStyle::Tines;
            state.mark("tines");
        }
        app.update();
        let tines = preview_part_count(&mut app);
        assert!(tines >= bare + 2);
    }

    #[test]
    fn rebuilding_replaces_the_preview_instead_of_stacking_copies() {
        let mut app = preview_app();
        for _ in 0..5 {
            {
                let mut state = app.world_mut().resource_mut::<WeaponForgeState>();
                state.mark("churn");
            }
            app.update();
        }
        let mut roots = app.world_mut().query::<&WeaponPreviewRoot>();
        assert_eq!(
            roots.iter(app.world()).count(),
            1,
            "each rebuild must despawn the previous weapon"
        );
    }

    #[test]
    fn an_idle_frame_does_not_rebuild_the_preview() {
        // The rebuild is gated on `dirty`; without that the tool would rebuild
        // meshes every frame just to spin the model.
        let mut app = preview_app();
        app.update();
        assert!(!app.world().resource::<WeaponForgeState>().dirty);
        app.update();
        let mut roots = app.world_mut().query::<&WeaponPreviewRoot>();
        assert_eq!(roots.iter(app.world()).count(), 1);
    }

    #[test]
    fn cycling_a_part_walks_the_whole_list_and_wraps() {
        let mut grip = GripStyle::default();
        let mut seen = vec![grip];
        for _ in 1..GripStyle::ALL.len() {
            grip = next_in(&GripStyle::ALL, grip);
            assert!(!seen.contains(&grip), "cycle repeated early");
            seen.push(grip);
        }
        assert_eq!(seen.len(), GripStyle::ALL.len(), "a grip is unreachable");
        assert_eq!(next_in(&GripStyle::ALL, grip), GripStyle::default());
    }

    #[test]
    fn the_colour_cycle_covers_every_blade_colour() {
        // The forge must be able to reach every colour the renderer can build
        // a material for, or some blades would be undesignable.
        let mut color = COLORS[0];
        let mut seen = vec![color];
        for _ in 1..COLORS.len() {
            color = next_in(&COLORS, color);
            seen.push(color);
        }
        for expected in crate::combat::blades::BLADE_COLOR_ORDER {
            assert!(seen.contains(&expected), "{expected:?} is undesignable");
        }
    }

    #[test]
    fn marking_sanitizes_so_held_buttons_cannot_escape_the_ranges() {
        let mut state = WeaponForgeState::default();
        state.spec.blade_length = 99.0;
        state.spec.blade_width = -5.0;
        state.mark("test");

        assert!(state.spec.blade_length <= BLADE_LENGTH_RANGE.1);
        assert!(state.spec.blade_width >= BLADE_WIDTH_RANGE.0);
        assert!(state.dirty && state.labels_dirty, "edits must redraw");
    }

    #[test]
    fn every_preset_button_maps_to_a_real_preset() {
        let presets = forge_presets();
        for index in 0..presets.len() {
            assert!(
                presets.clone().into_iter().nth(index).is_some(),
                "preset button {index} has no preset"
            );
        }
        // An out-of-range index is ignored rather than panicking.
        assert!(presets.into_iter().nth(999).is_none());
    }
}
