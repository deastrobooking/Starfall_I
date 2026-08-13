//! Vehicle Forge — project-backed authoring for boats, cars, trucks, and
//! motorcycles.
//!
//! The screen edits a runtime-neutral [`VehicleSpec`], derives performance
//! from visible dimensions, and compiles the same dimensions into a bounded
//! procedural preview. All panels use the shared movable, resizable,
//! minimizable tool-window shell.

use bevy::input::keyboard::Key;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use std::path::PathBuf;

use super::ui_plugin::{MenuButtonDisabled, MenuScrollPanel};
use crate::engine::state::AppState;
use crate::engine_tools::forge_widgets::{
    action_button, section_label, widget_row, ForgeWidgetStyle,
};
use crate::engine_tools::project_registry::ForgeProjectRegistry;
use crate::engine_tools::tool_windows::{spawn_tool_window, ToolWindowStyle};
use crate::engine_tools::vehicle_records;
use crate::resources::{AuthoringTextInputCapture, AuthoringUnsavedChanges, GameSettings};
use crate::vehicle_forge::{
    despawn_generated_vehicle, spawn_vehicle, vehicle_preview_scale, BodyStyle, DriveLayout,
    GeneratedVehicleAssets, VehicleClass, VehicleIssueSeverity, VehicleSpec,
};

pub struct VehicleForgePlugin;

impl Plugin for VehicleForgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VehicleForgeState>()
            .init_resource::<AuthoringTextInputCapture>()
            .init_resource::<AuthoringUnsavedChanges>()
            .add_systems(OnEnter(AppState::VehicleForge), setup_vehicle_forge)
            .add_systems(OnExit(AppState::VehicleForge), teardown_vehicle_forge)
            .add_systems(
                Update,
                (
                    sync_vehicle_name_input_capture,
                    vehicle_forge_name_input_system,
                    vehicle_forge_action_system,
                    sync_vehicle_unsaved_changes,
                    vehicle_forge_save_availability_system,
                    vehicle_forge_library_rebuild_system,
                    vehicle_forge_preview_rebuild_system,
                    vehicle_forge_label_refresh_system,
                    vehicle_forge_preview_spin_system,
                )
                    .chain()
                    .run_if(in_state(AppState::VehicleForge)),
            );
    }
}

#[derive(Resource)]
struct VehicleForgeState {
    spec: VehicleSpec,
    undo: Vec<VehicleSpec>,
    library: Vec<(String, String)>,
    dirty: bool,
    labels_dirty: bool,
    library_dirty: bool,
    naming: bool,
    load_armed: Option<String>,
    delete_armed: Option<String>,
    status: String,
    project_path: Option<PathBuf>,
    project_writable: bool,
    exit_armed: bool,
    saved_spec: Option<VehicleSpec>,
}

impl Default for VehicleForgeState {
    fn default() -> Self {
        Self {
            spec: VehicleSpec::default(),
            undo: Vec::new(),
            library: Vec::new(),
            dirty: true,
            labels_dirty: true,
            library_dirty: true,
            naming: false,
            load_armed: None,
            delete_armed: None,
            status: "Choose a class, shape its frame, then save it to the active project.".into(),
            project_path: None,
            project_writable: false,
            exit_armed: false,
            saved_spec: Some(VehicleSpec::default()),
        }
    }
}

impl VehicleForgeState {
    fn begin_edit(&mut self) {
        self.undo.push(self.spec.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.exit_armed = false;
    }

    fn mark(&mut self, message: impl Into<String>) {
        self.spec.sanitize_in_place();
        self.status = message.into();
        self.dirty = true;
        self.labels_dirty = true;
    }

    fn confirm_dirty_load(&mut self, content_id: &str) -> bool {
        arm_or_confirm_dirty_load(
            &mut self.load_armed,
            self.saved_spec.as_ref(),
            &self.spec,
            content_id,
        )
    }

    fn reset_after_deleted_current(&mut self, content_id: &str) -> bool {
        if self.spec.content_id != content_id {
            return false;
        }
        let fresh = VehicleSpec::preset(self.spec.class);
        self.spec = fresh.clone();
        self.saved_spec = Some(fresh);
        self.undo.clear();
        self.exit_armed = false;
        true
    }
}

#[derive(Component)]
struct VehicleForgeRoot;
#[derive(Component)]
struct VehiclePreviewRoot;
#[derive(Component)]
struct VehicleForgeStatusText;
#[derive(Component)]
struct VehicleForgeIdentityText;
#[derive(Component)]
struct VehicleForgeStatsText;
#[derive(Component)]
struct VehicleForgeFieldText(VehicleField);
#[derive(Component)]
struct VehicleForgeLibraryPanel;
#[derive(Component)]
struct VehicleForgeSaveButton;

#[derive(Component, Clone, Copy)]
struct VehicleForgeButton(VehicleForgeAction);

#[derive(Debug, Clone, Copy, PartialEq)]
enum VehicleForgeAction {
    Preset(VehicleClass),
    ClassNext,
    DriveNext,
    BodyNext,
    NameEdit,
    Adjust(VehicleField, f32),
    Seats(i8),
    Validate,
    Undo,
    Reset,
    Save,
    LibraryLoad(usize),
    LibraryDelete(usize),
    Back,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum VehicleField {
    Length,
    Width,
    Height,
    WheelRadius,
    GroundClearance,
    Power,
    Mass,
    Cargo,
    CabinPosition,
    Aero,
    Suspension,
    Armor,
}

impl VehicleField {
    const fn label(self) -> &'static str {
        match self {
            Self::Length => "Length",
            Self::Width => "Width",
            Self::Height => "Height",
            Self::WheelRadius => "Wheel radius",
            Self::GroundClearance => "Clearance",
            Self::Power => "Power",
            Self::Mass => "Mass",
            Self::Cargo => "Cargo",
            Self::CabinPosition => "Cabin position",
            Self::Aero => "Aerodynamics",
            Self::Suspension => "Suspension",
            Self::Armor => "Armor",
        }
    }

    const fn step(self) -> f32 {
        match self {
            Self::Length | Self::Width | Self::Height => 0.10,
            Self::WheelRadius | Self::GroundClearance => 0.05,
            Self::Power => 20.0,
            Self::Mass => 100.0,
            Self::Cargo => 100.0,
            Self::CabinPosition | Self::Aero | Self::Suspension | Self::Armor => 0.05,
        }
    }

    fn read(self, spec: &VehicleSpec) -> f32 {
        match self {
            Self::Length => spec.length,
            Self::Width => spec.width,
            Self::Height => spec.height,
            Self::WheelRadius => spec.wheel_radius,
            Self::GroundClearance => spec.ground_clearance,
            Self::Power => spec.power_kw,
            Self::Mass => spec.mass_kg,
            Self::Cargo => spec.cargo_capacity_kg,
            Self::CabinPosition => spec.cabin_position,
            Self::Aero => spec.aero,
            Self::Suspension => spec.suspension,
            Self::Armor => spec.armor,
        }
    }

    fn write(self, spec: &mut VehicleSpec, value: f32) {
        match self {
            Self::Length => spec.length = value,
            Self::Width => spec.width = value,
            Self::Height => spec.height = value,
            Self::WheelRadius => spec.wheel_radius = value,
            Self::GroundClearance => spec.ground_clearance = value,
            Self::Power => spec.power_kw = value,
            Self::Mass => spec.mass_kg = value,
            Self::Cargo => spec.cargo_capacity_kg = value,
            Self::CabinPosition => spec.cabin_position = value,
            Self::Aero => spec.aero = value,
            Self::Suspension => spec.suspension = value,
            Self::Armor => spec.armor = value,
        }
        spec.sanitize_in_place();
    }

    fn value_label(self, spec: &VehicleSpec) -> String {
        let value = self.read(spec);
        match self {
            Self::Length
            | Self::Width
            | Self::Height
            | Self::WheelRadius
            | Self::GroundClearance => {
                format!("{}: {value:.2} m", self.label())
            }
            Self::Power => format!("{}: {value:.0} kW", self.label()),
            Self::Mass | Self::Cargo => format!("{}: {value:.0} kg", self.label()),
            Self::CabinPosition | Self::Aero | Self::Suspension | Self::Armor => {
                format!("{}: {:.0}%", self.label(), value * 100.0)
            }
        }
    }
}

fn next_in<T: Copy + PartialEq>(values: &[T], current: T) -> T {
    let index = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    values[(index + 1) % values.len()]
}

fn setup_vehicle_forge(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<VehicleForgeState>,
    mut text_capture: ResMut<AuthoringTextInputCapture>,
    registry: Res<ForgeProjectRegistry>,
) {
    text_capture.active = false;
    state.dirty = true;
    state.labels_dirty = true;
    state.naming = false;
    state.load_armed = None;
    state.delete_armed = None;
    state.exit_armed = false;
    state.library_dirty = true;
    if state.project_path.as_deref() != registry.active.as_deref() {
        state.spec = VehicleSpec::default();
        state.undo.clear();
        state.saved_spec = Some(state.spec.clone());
        state.project_path = registry.active.clone();
    }
    match vehicle_records::list_active_project(&registry) {
        Ok((library, writable)) => {
            state.library = library;
            state.project_writable = writable;
            state.status = if writable {
                registry.active_entry().map_or_else(
                    || {
                        "No active project — return to the Project Hub and create or open one"
                            .into()
                    },
                    |entry| format!("Active project: {}", entry.name),
                )
            } else {
                "RECOVERY PROJECT — read-only until promoted in the level editor".into()
            };
        }
        Err(error) => {
            state.library.clear();
            state.project_writable = false;
            state.status = format!("PROJECT UNAVAILABLE — {error}");
        }
    }

    commands.spawn((
        VehicleForgeRoot,
        Camera3d::default(),
        Transform::from_xyz(7.4, 4.2, 9.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        VehicleForgeRoot,
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        VehicleForgeRoot,
        PointLight {
            intensity: 1_600_000.0,
            range: 24.0,
            color: Color::srgb(0.15, 0.64, 1.0),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-4.0, 3.0, -3.0),
    ));
    commands.spawn((
        VehicleForgeRoot,
        Mesh3d(meshes.add(Cylinder::new(4.1, 0.12))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.055, 0.075, 0.105),
            metallic: 0.65,
            perceptual_roughness: 0.48,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.08, 0.0),
    ));

    spawn_vehicle_forge_ui(&mut commands);
}

fn teardown_vehicle_forge(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<
        (Entity, Option<&GeneratedVehicleAssets>),
        Or<(With<VehicleForgeRoot>, With<VehiclePreviewRoot>)>,
    >,
    mut text_capture: ResMut<AuthoringTextInputCapture>,
    mut unsaved: ResMut<AuthoringUnsavedChanges>,
) {
    text_capture.active = false;
    unsaved.active = false;
    for (entity, generated_assets) in roots.iter() {
        if generated_assets.is_some() {
            despawn_generated_vehicle(
                &mut commands,
                &mut meshes,
                &mut materials,
                entity,
                generated_assets,
            );
        } else {
            commands.entity(entity).despawn();
        }
    }
}

fn widget_style() -> ForgeWidgetStyle {
    ForgeWidgetStyle {
        button_background: Color::srgb(0.07, 0.15, 0.21),
        button_border: Color::srgb(0.12, 0.66, 0.78),
        text: Color::srgb(0.92, 0.98, 1.0),
        min_width: 96.0,
        ..Default::default()
    }
}

fn forge_button(
    parent: &mut ChildSpawnerCommands,
    label: impl Into<String>,
    action: VehicleForgeAction,
) {
    action_button(parent, label, VehicleForgeButton(action), &widget_style());
}

fn forge_save_button(parent: &mut ChildSpawnerCommands, label: impl Into<String>) {
    action_button(
        parent,
        label,
        (
            VehicleForgeButton(VehicleForgeAction::Save),
            VehicleForgeSaveButton,
        ),
        &widget_style(),
    );
}

fn window_style(content_height: f32, initially_minimized: bool) -> ToolWindowStyle {
    ToolWindowStyle {
        accent: Color::srgb(0.08, 0.62, 0.74),
        background: Color::srgba(0.018, 0.055, 0.075, 0.95),
        // Two primary panels still fit side-by-side in an 800 px window at
        // 1.4× UI scale; rows wrap and each window remains independently
        // resizable for larger workspaces.
        width: 270.0,
        content_height: Val::Px(content_height),
        initially_minimized,
    }
}

fn spawn_vehicle_forge_ui(commands: &mut Commands) {
    commands
        .spawn((
            VehicleForgeRoot,
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
                Text::new("VEHICLE FORGE"),
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
                TextColor(Color::srgb(0.40, 0.94, 1.0)),
            ));
            root.spawn((
                Text::new(""),
                VehicleForgeStatusText,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(16.0),
                    top: Val::Px(46.0),
                    max_width: Val::Px(800.0),
                    ..default()
                },
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.86, 0.92, 0.98)),
            ));

            spawn_tool_window(
                root,
                "CLASS & PRESETS",
                Vec2::new(12.0, 84.0),
                window_style(220.0, false),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        VehicleForgeIdentityText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.88, 0.95, 1.0)),
                    ));
                    section_label(panel, "QUICK START");
                    widget_row(panel, |row| {
                        for class in VehicleClass::ALL {
                            forge_button(
                                row,
                                class.label().to_uppercase(),
                                VehicleForgeAction::Preset(class),
                            );
                        }
                    });
                    widget_row(panel, |row| {
                        forge_button(row, "NEXT CLASS", VehicleForgeAction::ClassNext);
                        forge_button(row, "DRIVE", VehicleForgeAction::DriveNext);
                        forge_button(row, "BODY", VehicleForgeAction::BodyNext);
                    });
                    widget_row(panel, |row| {
                        forge_button(row, "NAME", VehicleForgeAction::NameEdit);
                        forge_button(row, "SEAT -", VehicleForgeAction::Seats(-1));
                        forge_button(row, "SEAT +", VehicleForgeAction::Seats(1));
                    });
                },
            );

            spawn_tool_window(
                root,
                "DIMENSIONS & ENGINE",
                Vec2::new(958.0, 168.0),
                window_style(310.0, false),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    for field in [
                        VehicleField::Length,
                        VehicleField::Width,
                        VehicleField::Height,
                        VehicleField::WheelRadius,
                        VehicleField::GroundClearance,
                        VehicleField::Power,
                        VehicleField::Mass,
                        VehicleField::Cargo,
                    ] {
                        spawn_field_row(panel, field);
                    }
                },
            );

            spawn_tool_window(
                root,
                "TUNING & PERFORMANCE",
                Vec2::new(958.0, 84.0),
                window_style(330.0, true),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    for field in [
                        VehicleField::CabinPosition,
                        VehicleField::Aero,
                        VehicleField::Suspension,
                        VehicleField::Armor,
                    ] {
                        spawn_field_row(panel, field);
                    }
                    section_label(panel, "DERIVED — NOT TYPED");
                    panel.spawn((
                        Text::new(""),
                        VehicleForgeStatsText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.58, 0.92, 1.0)),
                    ));
                    widget_row(panel, |row| {
                        forge_button(row, "VALIDATE", VehicleForgeAction::Validate);
                        forge_button(row, "UNDO", VehicleForgeAction::Undo);
                        forge_button(row, "RESET", VehicleForgeAction::Reset);
                    });
                },
            );

            spawn_tool_window(
                root,
                "PROJECT LIBRARY",
                Vec2::new(958.0, 126.0),
                window_style(250.0, true),
                (
                    VehicleForgeLibraryPanel,
                    MenuScrollPanel,
                    ScrollPosition::default(),
                ),
                |_panel| {},
            );

            spawn_tool_window(
                root,
                "WORKFLOW",
                Vec2::new(640.0, 84.0),
                ToolWindowStyle {
                    width: 260.0,
                    ..window_style(130.0, true)
                },
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new("Save writes a versioned project record. Runtime motor binding is explicit at publish/use time."),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.68, 0.78, 0.84)),
                    ));
                    widget_row(panel, |row| {
                        forge_save_button(row, "SAVE");
                        forge_button(row, "BACK", VehicleForgeAction::Back);
                    });
                },
            );
        });
}

fn spawn_field_row(parent: &mut ChildSpawnerCommands, field: VehicleField) {
    widget_row(parent, |row| {
        row.spawn((
            Text::new(""),
            VehicleForgeFieldText(field),
            Node {
                min_width: Val::Px(160.0),
                ..default()
            },
            TextFont {
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::srgb(0.84, 0.91, 0.96)),
        ));
        forge_button(row, "−", VehicleForgeAction::Adjust(field, -field.step()));
        forge_button(row, "+", VehicleForgeAction::Adjust(field, field.step()));
    });
}

const VEHICLE_NAME_LIMIT: usize = 64;

fn sync_vehicle_name_input_capture(
    state: Res<VehicleForgeState>,
    mut capture: ResMut<AuthoringTextInputCapture>,
) {
    capture.active = state.naming;
}

fn sync_vehicle_unsaved_changes(
    state: Res<VehicleForgeState>,
    mut unsaved: ResMut<AuthoringUnsavedChanges>,
) {
    unsaved.active = state.saved_spec.as_ref() != Some(&state.spec);
}

fn vehicle_forge_name_input_system(
    keys: Res<ButtonInput<Key>>,
    mut state: ResMut<VehicleForgeState>,
) {
    if !state.naming {
        return;
    }
    let mut changed = false;
    for key in keys.get_just_pressed() {
        match key {
            Key::Character(value) => {
                for character in value.chars() {
                    if (character.is_alphanumeric() || matches!(character, ' ' | '.' | '_' | '-'))
                        && state.spec.display_name.chars().count() < VEHICLE_NAME_LIMIT
                    {
                        state.spec.display_name.push(character);
                        changed = true;
                    }
                }
            }
            Key::Space if state.spec.display_name.chars().count() < VEHICLE_NAME_LIMIT => {
                state.spec.display_name.push(' ');
                changed = true;
            }
            Key::Backspace => {
                changed |= state.spec.display_name.pop().is_some();
            }
            Key::Enter | Key::Escape => {
                state.naming = false;
                changed = true;
            }
            _ => {}
        }
    }
    if changed {
        let name = state.spec.display_name.clone();
        let message = if state.naming {
            format!("Naming: {name}_")
        } else {
            format!("Named: {name}")
        };
        state.mark(message);
    }
}

fn vehicle_forge_action_system(
    interactions: Query<(&Interaction, &VehicleForgeButton), (Changed<Interaction>, With<Button>)>,
    mut state: ResMut<VehicleForgeState>,
    mut capture: ResMut<AuthoringTextInputCapture>,
    registry: Res<ForgeProjectRegistry>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let action = interactions.iter().find_map(|(interaction, button)| {
        (*interaction == Interaction::Pressed).then_some(button.0)
    });
    let Some(action) = action else {
        return;
    };
    if state.naming && action != VehicleForgeAction::NameEdit {
        return;
    }
    if !matches!(action, VehicleForgeAction::LibraryLoad(_)) && state.load_armed.take().is_some() {
        state.library_dirty = true;
    }
    if !matches!(action, VehicleForgeAction::LibraryDelete(_))
        && state.delete_armed.take().is_some()
    {
        state.library_dirty = true;
    }
    if action != VehicleForgeAction::Back {
        state.exit_armed = false;
    }

    match action {
        VehicleForgeAction::Preset(class) => {
            state.begin_edit();
            state.spec = VehicleSpec::preset(class);
            state.mark(format!("Loaded ergonomic {} preset", class.label()));
        }
        VehicleForgeAction::ClassNext => {
            state.begin_edit();
            let class = next_in(&VehicleClass::ALL, state.spec.class);
            state.spec = VehicleSpec::preset(class);
            state.mark(format!("Class: {}", class.label()));
        }
        VehicleForgeAction::DriveNext => {
            state.begin_edit();
            state.spec.drive = next_in(&DriveLayout::ALL, state.spec.drive);
            let label = state.spec.drive.label();
            state.mark(format!("Drive: {label}"));
        }
        VehicleForgeAction::BodyNext => {
            state.begin_edit();
            state.spec.body_style = next_in(&BodyStyle::ALL, state.spec.body_style);
            let label = state.spec.body_style.label();
            state.mark(format!("Body: {label}"));
        }
        VehicleForgeAction::NameEdit => {
            if !state.naming {
                state.begin_edit();
            }
            state.naming = !state.naming;
            capture.active = state.naming;
            let name = state.spec.display_name.clone();
            let naming = state.naming;
            state.mark(if naming {
                format!("Naming: {name}_  (Enter to finish)")
            } else {
                format!("Named: {name}")
            });
        }
        VehicleForgeAction::Adjust(field, delta) => {
            state.begin_edit();
            let value = field.read(&state.spec) + delta;
            field.write(&mut state.spec, value);
            let label = field.value_label(&state.spec);
            state.mark(label);
        }
        VehicleForgeAction::Seats(delta) => {
            state.begin_edit();
            state.spec.seats = state.spec.seats.saturating_add_signed(delta).clamp(1, 24);
            let seats = state.spec.seats;
            state.mark(format!("Seats: {seats}"));
        }
        VehicleForgeAction::Validate => {
            let issues = state.spec.validate();
            state.status = if issues.is_empty() {
                "Vehicle is valid and ready to save/publish.".into()
            } else {
                let blocking = issues
                    .iter()
                    .filter(|issue| issue.severity == VehicleIssueSeverity::Error)
                    .count();
                format!(
                    "{} issue(s), {blocking} blocking: {}",
                    issues.len(),
                    issues
                        .iter()
                        .take(3)
                        .map(|issue| issue.message.as_str())
                        .collect::<Vec<_>>()
                        .join(" • ")
                )
            };
            state.labels_dirty = true;
        }
        VehicleForgeAction::Undo => {
            if let Some(spec) = state.undo.pop() {
                state.spec = spec;
                state.mark("Undo");
            } else {
                state.status = "Nothing to undo".into();
                state.labels_dirty = true;
            }
        }
        VehicleForgeAction::Reset => {
            state.begin_edit();
            state.spec = VehicleSpec::preset(state.spec.class);
            state.mark("Reset current class to its safe preset");
        }
        VehicleForgeAction::Save => {
            let blocking = vehicle_records::blocking_issues(&state.spec);
            if !blocking.is_empty() {
                state.status = format!("SAVE DISABLED — {}", blocking.join(" • "));
                state.labels_dirty = true;
                return;
            }
            let spec = state.spec.clone();
            match vehicle_records::save_to_active_project(&registry, &spec) {
                Ok(content_id) => {
                    state.spec.content_id.clone_from(&content_id);
                    let refresh = vehicle_records::list_active_project(&registry);
                    let message = match refresh {
                        Ok((library, writable)) => {
                            state.library = library;
                            state.project_writable = writable;
                            format!("Saved {content_id} to the active project")
                        }
                        Err(error) => {
                            state.library.clear();
                            state.project_writable = false;
                            format!("Saved, but could not refresh library: {error}")
                        }
                    };
                    state.library_dirty = true;
                    state.undo.clear();
                    state.exit_armed = false;
                    let saved = state.spec.clone();
                    state.saved_spec = Some(saved);
                    state.mark(message);
                }
                Err(error) => {
                    state.status = format!("Cannot save: {error}");
                    state.labels_dirty = true;
                }
            }
        }
        VehicleForgeAction::LibraryLoad(index) => {
            let Some((content_id, name)) = state.library.get(index).cloned() else {
                return;
            };
            if !state.confirm_dirty_load(&content_id) {
                state.status = format!(
                    "Load {name} and discard unsaved changes? Press CONFIRM LOAD on this row; any other action cancels."
                );
                state.labels_dirty = true;
                state.library_dirty = true;
                return;
            }
            match vehicle_records::load_from_active_project(&registry, &content_id) {
                Ok(spec) => {
                    state.spec = spec;
                    state.undo.clear();
                    let saved = state.spec.clone();
                    state.saved_spec = Some(saved);
                    state.mark(format!("Loaded {name}"));
                }
                Err(error) => {
                    state.status = format!("Cannot load {name}: {error}");
                    state.labels_dirty = true;
                }
            }
        }
        VehicleForgeAction::LibraryDelete(index) => {
            if !state.project_writable {
                state.status = "DELETE DISABLED — the active project is read-only".into();
                state.labels_dirty = true;
                return;
            }
            let Some((content_id, name)) = state.library.get(index).cloned() else {
                return;
            };
            if !arm_or_confirm_delete(&mut state.delete_armed, &content_id) {
                state.status = format!(
                    "Delete {name}? Press CONFIRM DELETE on this row; any other action cancels."
                );
                state.labels_dirty = true;
                state.library_dirty = true;
                return;
            }
            match vehicle_records::delete_from_active_project(&registry, &content_id) {
                Ok(true) => {
                    let reset_current = state.reset_after_deleted_current(&content_id);
                    let message = match vehicle_records::list_active_project(&registry) {
                        Ok((library, writable)) => {
                            state.library = library;
                            state.project_writable = writable;
                            if reset_current {
                                format!(
                                    "Deleted {name}; opened a fresh {} template",
                                    state.spec.class.label()
                                )
                            } else {
                                format!("Deleted {name}")
                            }
                        }
                        Err(error) => {
                            state.library.clear();
                            state.project_writable = false;
                            format!("Deleted, but could not refresh library: {error}")
                        }
                    };
                    state.library_dirty = true;
                    state.status = message;
                    state.labels_dirty = true;
                }
                Ok(false) => {
                    state.status = format!("{name} was already gone");
                    state.labels_dirty = true;
                }
                Err(error) => {
                    state.status = format!("Cannot delete {name}: {error}");
                    state.labels_dirty = true;
                }
            }
            state.library_dirty = true;
        }
        VehicleForgeAction::Back => {
            if state.saved_spec.as_ref() != Some(&state.spec) && !state.exit_armed {
                state.exit_armed = true;
                state.status =
                    "Unsaved changes — press BACK again to discard them, or SAVE to keep them"
                        .into();
                state.labels_dirty = true;
                return;
            }
            if let Some(saved) = state.saved_spec.clone() {
                state.spec = saved;
            }
            state.undo.clear();
            state.naming = false;
            capture.active = false;
            next_state.set(AppState::ProjectHub);
        }
    }
}

/// A destructive library action needs two presses on the same stable record.
/// Choosing another record merely moves the arm; the second matching press
/// consumes it and authorizes the caller to perform the project transaction.
fn arm_or_confirm_delete(armed: &mut Option<String>, content_id: &str) -> bool {
    if armed.as_deref() == Some(content_id) {
        *armed = None;
        true
    } else {
        *armed = Some(content_id.to_owned());
        false
    }
}

/// Loading is immediate when the current document matches its saved baseline.
/// A dirty document requires two presses on the same stable library record;
/// choosing any other row moves the confirmation rather than losing the draft.
fn arm_or_confirm_dirty_load(
    armed: &mut Option<String>,
    saved_spec: Option<&VehicleSpec>,
    current_spec: &VehicleSpec,
    content_id: &str,
) -> bool {
    if saved_spec == Some(current_spec) {
        *armed = None;
        return true;
    }
    if armed.as_deref() == Some(content_id) {
        *armed = None;
        true
    } else {
        *armed = Some(content_id.to_owned());
        false
    }
}

/// A successfully deleted open document must stop carrying the deleted stable
/// id. Deleting an unrelated row leaves the current draft and undo history
/// untouched.
fn vehicle_forge_save_availability_system(
    mut commands: Commands,
    state: Res<VehicleForgeState>,
    buttons: Query<
        (Entity, Has<InteractionDisabled>, Has<MenuButtonDisabled>),
        With<VehicleForgeSaveButton>,
    >,
) {
    let disabled =
        !state.project_writable || !vehicle_records::blocking_issues(&state.spec).is_empty();
    for (entity, interaction_disabled, menu_disabled) in buttons.iter() {
        match (disabled, interaction_disabled || menu_disabled) {
            (true, false) => {
                commands
                    .entity(entity)
                    .insert((InteractionDisabled, MenuButtonDisabled));
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .remove::<(InteractionDisabled, MenuButtonDisabled)>();
            }
            _ => {}
        }
    }
}

fn vehicle_forge_library_rebuild_system(
    mut commands: Commands,
    mut state: ResMut<VehicleForgeState>,
    panels: Query<(Entity, Option<&Children>), With<VehicleForgeLibraryPanel>>,
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
        widget_row(panel, |row| {
            forge_save_button(row, "SAVE CURRENT");
            forge_button(row, "BACK", VehicleForgeAction::Back);
        });
        if state.library.is_empty() {
            panel.spawn((
                Text::new("No saved vehicles yet. Start with a class preset, tune it, then SAVE."),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.58, 0.66, 0.70)),
            ));
            return;
        }
        for (index, (content_id, name)) in state.library.iter().enumerate() {
            widget_row(panel, |row| {
                row.spawn((
                    Text::new(name.clone()),
                    Node {
                        min_width: Val::Px(140.0),
                        ..default()
                    },
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.86, 0.94, 0.98)),
                ));
                let load_armed = state.load_armed.as_deref() == Some(content_id.as_str());
                forge_button(
                    row,
                    if load_armed { "CONFIRM LOAD" } else { "LOAD" },
                    VehicleForgeAction::LibraryLoad(index),
                );
                let armed = state.delete_armed.as_deref() == Some(content_id.as_str());
                forge_button(
                    row,
                    if armed { "CONFIRM DELETE" } else { "DELETE" },
                    VehicleForgeAction::LibraryDelete(index),
                );
            });
        }
    });
}

fn vehicle_forge_label_refresh_system(
    mut state: ResMut<VehicleForgeState>,
    mut texts: ParamSet<(
        Query<&mut Text, With<VehicleForgeStatusText>>,
        Query<&mut Text, With<VehicleForgeIdentityText>>,
        Query<(&mut Text, &VehicleForgeFieldText)>,
        Query<&mut Text, With<VehicleForgeStatsText>>,
    )>,
) {
    if !state.labels_dirty {
        return;
    }
    state.labels_dirty = false;
    let visible_status = status_with_validation(&state.status, &state.spec);
    for mut text in texts.p0().iter_mut() {
        *text = Text::new(visible_status.clone());
    }
    for mut text in texts.p1().iter_mut() {
        *text = Text::new(format!(
            "{}{}\n{} • {} • {}\n{} seat(s)",
            state.spec.display_name,
            if state.naming { "_" } else { "" },
            state.spec.class.label(),
            state.spec.body_style.label(),
            state.spec.drive.label(),
            state.spec.seats,
        ));
    }
    for (mut text, marker) in texts.p2().iter_mut() {
        *text = Text::new(marker.0.value_label(&state.spec));
    }
    let stats = state.spec.derived_stats();
    for mut text in texts.p3().iter_mut() {
        let issues = state.spec.validate();
        let errors = issues
            .iter()
            .filter(|issue| issue.severity == VehicleIssueSeverity::Error)
            .count();
        let reasons = issues
            .iter()
            .filter(|issue| issue.severity == VehicleIssueSeverity::Error)
            .take(2)
            .map(|issue| format!("x {}", issue.message))
            .collect::<Vec<_>>()
            .join("\n");
        *text = Text::new(format!(
            "Top speed: {:.0} km/h\nAcceleration: {:.0}/100\nHandling: {:.0}/100\nDurability: {:.0}/100\nRange: {:.0}/100\n{}{}",
            stats.estimated_top_speed_kph,
            stats.acceleration_score,
            stats.handling_score,
            stats.durability_score,
            stats.range_score,
            if errors == 0 {
                "READY TO SAVE".to_string()
            } else {
                format!("{errors} BLOCKING ISSUE(S)")
            },
            if reasons.is_empty() {
                String::new()
            } else {
                format!("\n{reasons}")
            },
        ));
    }
}

fn status_with_validation(status: &str, spec: &VehicleSpec) -> String {
    let blocking = vehicle_records::blocking_issues(spec);
    if blocking.is_empty() {
        status.to_owned()
    } else {
        format!(
            "{status}\nSAVE BLOCKED — {}",
            blocking.into_iter().take(2).collect::<Vec<_>>().join(" • ")
        )
    }
}

fn vehicle_forge_preview_rebuild_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<VehicleForgeState>,
    previews: Query<(Entity, Option<&GeneratedVehicleAssets>), With<VehiclePreviewRoot>>,
) {
    if !state.dirty {
        return;
    }
    state.dirty = false;
    for (entity, generated_assets) in previews.iter() {
        despawn_generated_vehicle(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
            generated_assets,
        );
    }
    let spec = state.spec.clone().sanitized();
    if let Ok(root) = spawn_vehicle(
        &mut commands,
        &mut meshes,
        &mut materials,
        &spec,
        Vec3::ZERO,
    ) {
        let preview_scale = vehicle_preview_scale(&spec);
        commands.entity(root).insert((
            VehiclePreviewRoot,
            // Wheel centers are authored at one local wheel radius, so the
            // compiled tire bottoms already sit at local ground zero.
            Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::splat(preview_scale)),
        ));
    }
}

fn vehicle_forge_preview_spin_system(
    time: Res<Time>,
    settings: Res<GameSettings>,
    mut previews: Query<&mut Transform, With<VehiclePreviewRoot>>,
) {
    if settings.reduced_ui_motion {
        return;
    }
    for mut transform in previews.iter_mut() {
        transform.rotate_y(time.delta_secs() * 0.42);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_uses_shared_windows_and_progressive_disclosure() {
        let mut app = App::new();
        {
            let world = app.world_mut();
            let mut commands = world.commands();
            spawn_vehicle_forge_ui(&mut commands);
        }
        app.world_mut().flush();
        let (all, minimized) = {
            let world = app.world_mut();
            let mut query = world.query::<&crate::engine_tools::tool_windows::ToolWindow>();
            query.iter(world).fold((0, 0), |(all, minimized), window| {
                (all + 1, minimized + usize::from(window.minimized))
            })
        };
        assert_eq!(all, 5);
        assert_eq!(minimized, 3);
    }

    fn preview_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<VehicleForgeState>()
            .add_systems(Update, vehicle_forge_preview_rebuild_system);
        app
    }

    #[test]
    fn every_class_compiles_to_a_bounded_preview() {
        let mut app = preview_app();
        for class in VehicleClass::ALL {
            {
                let mut state = app.world_mut().resource_mut::<VehicleForgeState>();
                state.spec = VehicleSpec::preset(class);
                state.dirty = true;
            }
            app.update();
            let world = app.world_mut();
            let mut roots = world.query_filtered::<Entity, With<VehiclePreviewRoot>>();
            let root = roots.single(world).expect("exactly one preview root");
            let generated = world
                .get::<crate::vehicle_forge::GeneratedVehicle>(root)
                .expect("preview uses the shared runtime compiler");
            assert_eq!(generated.class, class);
            let parts = world.get::<Children>(root).map_or(0, Children::len);
            assert!(parts >= 3, "{} preview is incomplete", class.label());
        }
    }

    #[test]
    fn rebuild_replaces_instead_of_stacking_preview_roots() {
        let mut app = preview_app();
        for _ in 0..4 {
            app.world_mut().resource_mut::<VehicleForgeState>().dirty = true;
            app.update();
        }
        let world = app.world_mut();
        let mut roots = world.query::<&VehiclePreviewRoot>();
        assert_eq!(roots.iter(world).count(), 1);
    }

    #[test]
    fn field_edits_clamp_through_the_model_contract() {
        let mut spec = VehicleSpec::default();
        VehicleField::Length.write(&mut spec, 1_000.0);
        VehicleField::Armor.write(&mut spec, -4.0);
        assert_eq!(spec.length, 18.0);
        assert_eq!(spec.armor, 0.0);
    }

    #[test]
    fn delete_requires_two_presses_on_the_same_record() {
        let mut armed = None;
        assert!(!arm_or_confirm_delete(&mut armed, "vehicle.a"));
        assert_eq!(armed.as_deref(), Some("vehicle.a"));
        assert!(!arm_or_confirm_delete(&mut armed, "vehicle.b"));
        assert_eq!(armed.as_deref(), Some("vehicle.b"));
        assert!(arm_or_confirm_delete(&mut armed, "vehicle.b"));
        assert!(armed.is_none());
    }

    #[test]
    fn dirty_library_load_requires_same_row_confirmation_and_preserves_the_draft() {
        let saved = VehicleSpec::preset(VehicleClass::Car);
        let mut draft = saved.clone();
        draft.power_kw += 80.0;
        let draft_before = draft.clone();
        let mut armed = None;

        assert!(!arm_or_confirm_dirty_load(
            &mut armed,
            Some(&saved),
            &draft,
            "starfall.vehicle.boat_a"
        ));
        assert_eq!(draft, draft_before, "arming must not replace the draft");
        assert_eq!(armed.as_deref(), Some("starfall.vehicle.boat_a"));

        assert!(!arm_or_confirm_dirty_load(
            &mut armed,
            Some(&saved),
            &draft,
            "starfall.vehicle.truck_b"
        ));
        assert_eq!(draft, draft_before);
        assert_eq!(armed.as_deref(), Some("starfall.vehicle.truck_b"));
        assert!(arm_or_confirm_dirty_load(
            &mut armed,
            Some(&saved),
            &draft,
            "starfall.vehicle.truck_b"
        ));
        assert!(armed.is_none());

        assert!(arm_or_confirm_dirty_load(
            &mut armed,
            Some(&saved),
            &saved,
            "starfall.vehicle.clean_load"
        ));
    }

    #[test]
    fn deleting_the_open_design_resets_cleanly_but_deleting_another_preserves_it() {
        let mut current = VehicleSpec::preset(VehicleClass::Truck);
        current.content_id = "starfall.vehicle.open_truck".into();
        current.power_kw += 60.0;
        let saved = Some(current.clone());
        let undo = vec![VehicleSpec::preset(VehicleClass::Car)];

        let before = current.clone();
        let saved_before = saved.clone();
        let undo_before = undo.clone();
        let mut state = VehicleForgeState {
            spec: current,
            saved_spec: saved,
            undo,
            ..Default::default()
        };
        assert!(!state.reset_after_deleted_current("starfall.vehicle.some_other_vehicle"));
        assert_eq!(state.spec, before);
        assert_eq!(state.saved_spec, saved_before);
        assert_eq!(state.undo, undo_before);

        assert!(state.reset_after_deleted_current("starfall.vehicle.open_truck"));
        assert_eq!(state.spec, VehicleSpec::preset(VehicleClass::Truck));
        assert_eq!(state.saved_spec.as_ref(), Some(&state.spec));
        assert!(state.undo.is_empty());
    }

    #[test]
    fn naming_claims_and_releases_global_authoring_input() {
        let mut app = App::new();
        app.init_resource::<VehicleForgeState>()
            .init_resource::<AuthoringTextInputCapture>()
            .add_systems(Update, sync_vehicle_name_input_capture);
        app.world_mut().resource_mut::<VehicleForgeState>().naming = true;
        app.update();
        assert!(app.world().resource::<AuthoringTextInputCapture>().active);
        app.world_mut().resource_mut::<VehicleForgeState>().naming = false;
        app.update();
        assert!(!app.world().resource::<AuthoringTextInputCapture>().active);
    }

    #[test]
    fn invalid_recipe_disables_save_until_repaired() {
        let mut app = App::new();
        app.init_resource::<VehicleForgeState>()
            .add_systems(Update, vehicle_forge_save_availability_system);
        let save = app
            .world_mut()
            .spawn((
                VehicleForgeSaveButton,
                VehicleForgeButton(VehicleForgeAction::Save),
            ))
            .id();
        app.world_mut()
            .resource_mut::<VehicleForgeState>()
            .project_writable = true;
        app.world_mut()
            .resource_mut::<VehicleForgeState>()
            .spec
            .display_name
            .clear();
        app.update();
        assert!(app.world().get::<InteractionDisabled>(save).is_some());

        app.world_mut()
            .resource_mut::<VehicleForgeState>()
            .spec
            .display_name = "Repaired Runner".into();
        app.update();
        assert!(app.world().get::<InteractionDisabled>(save).is_none());
    }

    #[test]
    fn blocking_reasons_are_in_the_always_visible_status() {
        let mut spec = VehicleSpec::default();
        spec.display_name.clear();
        let status = status_with_validation("Naming", &spec);
        assert!(status.contains("SAVE BLOCKED"));
        assert!(status.contains("Display name"));
    }
}
