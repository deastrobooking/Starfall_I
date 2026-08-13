//! Spaceship Forge — project-backed authoring for fighters, bombers,
//! destroyers, command ships, and space bases.
//!
//! Every panel uses the shared movable/resizable/minimizable tool-window shell.
//! The live preview calls the same procedural compiler exposed to future
//! runtime placement adapters, and every saved design enters the versioned
//! Forge project rather than a tool-specific side file.

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
use crate::engine_tools::spaceship_records;
use crate::engine_tools::tool_windows::{spawn_tool_window, ToolWindowStyle};
use crate::resources::{AuthoringTextInputCapture, AuthoringUnsavedChanges, GameSettings};
use crate::spaceship_forge::{
    spacecraft_preview_scale, spawn_spacecraft, SpacecraftClass, SpacecraftSpec,
    SpacecraftValidationSeverity,
};

pub struct SpaceshipForgePlugin;

impl Plugin for SpaceshipForgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpaceshipForgeState>()
            .init_resource::<AuthoringTextInputCapture>()
            .init_resource::<AuthoringUnsavedChanges>()
            .add_systems(OnEnter(AppState::SpaceshipForge), setup_spaceship_forge)
            .add_systems(OnExit(AppState::SpaceshipForge), teardown_spaceship_forge)
            .add_systems(
                Update,
                (
                    sync_spaceship_name_input_capture,
                    spaceship_name_input_system,
                    spaceship_action_system,
                    sync_spaceship_unsaved_changes,
                    spaceship_save_availability_system,
                    spaceship_library_rebuild_system,
                    spaceship_preview_rebuild_system,
                    spaceship_label_refresh_system,
                    spaceship_preview_spin_system,
                )
                    .chain()
                    .run_if(in_state(AppState::SpaceshipForge)),
            );
    }
}

#[derive(Resource)]
struct SpaceshipForgeState {
    spec: SpacecraftSpec,
    undo: Vec<SpacecraftSpec>,
    library: Vec<(String, String)>,
    dirty: bool,
    labels_dirty: bool,
    library_dirty: bool,
    naming: bool,
    /// Presets are templates, not durable identities. Their first save derives
    /// an id from the edited name; loaded/saved designs retain their id when
    /// renamed so level references do not break.
    template_identity: bool,
    load_armed: Option<String>,
    delete_armed: Option<String>,
    status: String,
    project_path: Option<PathBuf>,
    project_writable: bool,
    exit_armed: bool,
    saved_spec: Option<SpacecraftSpec>,
}

impl Default for SpaceshipForgeState {
    fn default() -> Self {
        Self {
            spec: SpacecraftSpec::default(),
            undo: Vec::new(),
            library: Vec::new(),
            dirty: true,
            labels_dirty: true,
            library_dirty: true,
            naming: false,
            template_identity: true,
            load_armed: None,
            delete_armed: None,
            status:
                "Choose a role preset, shape the hull and systems, then save to the active project."
                    .into(),
            project_path: None,
            project_writable: false,
            exit_armed: false,
            saved_spec: Some(SpacecraftSpec::default()),
        }
    }
}

impl SpaceshipForgeState {
    fn has_unsaved_changes(&self) -> bool {
        self.saved_spec.as_ref() != Some(&self.spec)
    }

    fn begin_edit(&mut self) {
        self.undo.push(self.spec.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.exit_armed = false;
    }

    fn mark(&mut self, message: impl Into<String>) {
        // Preserve a trailing space/cursor while typing; every numerical and
        // palette field still passes through the shared sanitizer.
        let editing_name = self.naming.then(|| self.spec.display_name.clone());
        self.spec = self.spec.sanitized();
        if let Some(name) = editing_name {
            self.spec.display_name = name;
        }
        self.status = message.into();
        self.dirty = true;
        self.labels_dirty = true;
    }

    /// Dirty drafts require two presses on the same stable library record.
    /// Clean documents can switch immediately, and choosing another row merely
    /// moves the confirmation without touching the in-memory draft.
    fn authorize_library_load(&mut self, content_id: &str) -> bool {
        if !self.has_unsaved_changes() {
            self.load_armed = None;
            return true;
        }
        if self.load_armed.as_deref() == Some(content_id) {
            self.load_armed = None;
            true
        } else {
            self.load_armed = Some(content_id.to_owned());
            false
        }
    }

    fn cancel_library_load(&mut self) -> bool {
        self.load_armed.take().is_some()
    }

    /// A successfully deleted open document must not remain a clean-looking
    /// handle to a record that no longer exists. Start a new template of the
    /// same role; deleting any other library row leaves the document untouched.
    fn reset_if_current_deleted(&mut self, content_id: &str) -> bool {
        if self.spec.content_id != content_id {
            return false;
        }
        self.spec = SpacecraftSpec::preset(self.spec.class);
        self.template_identity = true;
        self.undo.clear();
        self.exit_armed = false;
        self.saved_spec = Some(self.spec.clone());
        self.dirty = true;
        self.labels_dirty = true;
        true
    }
}

#[derive(Component)]
struct SpaceshipForgeRoot;
#[derive(Component)]
struct SpaceshipPreviewRoot;
#[derive(Component)]
struct SpaceshipStatusText;
#[derive(Component)]
struct SpaceshipIdentityText;
#[derive(Component)]
struct SpaceshipStatsText;
#[derive(Component)]
struct SpaceshipFieldText(SpacecraftField);
#[derive(Component)]
struct SpaceshipSystemText(SpacecraftSystem);
#[derive(Component)]
struct SpaceshipLibraryPanel;

#[derive(Component, Clone, Copy)]
struct SpaceshipButton(SpaceshipAction);

/// Marks every save affordance so invalid recipes are visibly and
/// semantically unavailable, including controller activation.
#[derive(Component)]
struct SpaceshipSaveButton;

#[derive(Debug, Clone, Copy, PartialEq)]
enum SpaceshipAction {
    Preset(SpacecraftClass),
    NameEdit,
    Adjust(SpacecraftField, f32),
    AdjustSystem(SpacecraftSystem, i8),
    PaletteNext,
    Validate,
    Undo,
    Reset,
    Save,
    LibraryLoad(usize),
    LibraryDelete(usize),
    Back,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum SpacecraftField {
    HullLength,
    HullWidth,
    HullHeight,
    WingSpan,
    NoseTaper,
    WingSweep,
    ArmorDensity,
    EngineScale,
    ReactorScale,
}

impl SpacecraftField {
    const HULL: [Self; 6] = [
        Self::HullLength,
        Self::HullWidth,
        Self::HullHeight,
        Self::WingSpan,
        Self::NoseTaper,
        Self::WingSweep,
    ];
    const SYSTEM_TUNING: [Self; 3] = [Self::ArmorDensity, Self::EngineScale, Self::ReactorScale];

    const fn label(self) -> &'static str {
        match self {
            Self::HullLength => "Hull length",
            Self::HullWidth => "Hull width",
            Self::HullHeight => "Hull height",
            Self::WingSpan => "Wing / ring span",
            Self::NoseTaper => "Nose taper",
            Self::WingSweep => "Wing sweep",
            Self::ArmorDensity => "Armor density",
            Self::EngineScale => "Engine scale",
            Self::ReactorScale => "Reactor scale",
        }
    }

    const fn step(self) -> f32 {
        match self {
            Self::HullLength | Self::HullWidth | Self::HullHeight | Self::WingSpan => 2.0,
            Self::NoseTaper | Self::WingSweep | Self::ArmorDensity => 0.05,
            Self::EngineScale | Self::ReactorScale => 0.10,
        }
    }

    fn read(self, spec: &SpacecraftSpec) -> f32 {
        match self {
            Self::HullLength => spec.hull_length,
            Self::HullWidth => spec.hull_width,
            Self::HullHeight => spec.hull_height,
            Self::WingSpan => spec.wing_span,
            Self::NoseTaper => spec.nose_taper,
            Self::WingSweep => spec.wing_sweep,
            Self::ArmorDensity => spec.armor_density,
            Self::EngineScale => spec.engine_scale,
            Self::ReactorScale => spec.reactor_scale,
        }
    }

    fn write(self, spec: &mut SpacecraftSpec, value: f32) {
        match self {
            Self::HullLength => spec.hull_length = value,
            Self::HullWidth => spec.hull_width = value,
            Self::HullHeight => spec.hull_height = value,
            Self::WingSpan => spec.wing_span = value,
            Self::NoseTaper => spec.nose_taper = value,
            Self::WingSweep => spec.wing_sweep = value,
            Self::ArmorDensity => spec.armor_density = value,
            Self::EngineScale => spec.engine_scale = value,
            Self::ReactorScale => spec.reactor_scale = value,
        }
        *spec = spec.sanitized();
    }

    fn value_label(self, spec: &SpacecraftSpec) -> String {
        let value = self.read(spec);
        match self {
            Self::HullLength | Self::HullWidth | Self::HullHeight | Self::WingSpan => {
                format!("{}: {value:.1} m", self.label())
            }
            Self::NoseTaper | Self::WingSweep | Self::ArmorDensity => {
                format!("{}: {:.0}%", self.label(), value * 100.0)
            }
            Self::EngineScale | Self::ReactorScale => {
                format!("{}: {value:.2}x", self.label())
            }
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum SpacecraftSystem {
    Engines,
    Weapons,
    Hangars,
    Command,
    Shields,
}

impl SpacecraftSystem {
    const ALL: [Self; 5] = [
        Self::Engines,
        Self::Weapons,
        Self::Hangars,
        Self::Command,
        Self::Shields,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Engines => "Engines",
            Self::Weapons => "Weapon mounts",
            Self::Hangars => "Hangar bays",
            Self::Command => "Command modules",
            Self::Shields => "Shield emitters",
        }
    }

    fn read(self, spec: &SpacecraftSpec) -> u8 {
        match self {
            Self::Engines => spec.engine_count,
            Self::Weapons => spec.weapon_mounts,
            Self::Hangars => spec.hangar_bays,
            Self::Command => spec.command_modules,
            Self::Shields => spec.shield_emitters,
        }
    }

    fn write(self, spec: &mut SpacecraftSpec, delta: i8) {
        let value = self.read(spec).saturating_add_signed(delta);
        match self {
            Self::Engines => spec.engine_count = value,
            Self::Weapons => spec.weapon_mounts = value,
            Self::Hangars => spec.hangar_bays = value,
            Self::Command => spec.command_modules = value,
            Self::Shields => spec.shield_emitters = value,
        }
        *spec = spec.sanitized();
    }
}

fn is_template_identity(spec: &SpacecraftSpec) -> bool {
    spec.content_id == SpacecraftSpec::preset(spec.class).content_id
}

fn setup_spaceship_forge(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<SpaceshipForgeState>,
    mut text_capture: ResMut<AuthoringTextInputCapture>,
    registry: Res<ForgeProjectRegistry>,
) {
    text_capture.active = false;
    state.dirty = true;
    state.labels_dirty = true;
    state.library_dirty = true;
    state.naming = false;
    state.load_armed = None;
    state.delete_armed = None;
    state.exit_armed = false;
    if state.project_path.as_deref() != registry.active.as_deref() {
        state.spec = SpacecraftSpec::default();
        state.undo.clear();
        state.saved_spec = Some(state.spec.clone());
        state.template_identity = true;
        state.project_path = registry.active.clone();
    } else {
        state.template_identity = is_template_identity(&state.spec);
    }
    match spaceship_records::list_active_project(&registry) {
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
        SpaceshipForgeRoot,
        Camera3d::default(),
        Transform::from_xyz(10.5, 7.0, 13.5).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        SpaceshipForgeRoot,
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(6.0, 10.0, 7.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        SpaceshipForgeRoot,
        PointLight {
            intensity: 1_800_000.0,
            range: 30.0,
            color: Color::srgb(0.18, 0.46, 1.0),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-6.0, 2.0, -4.0),
    ));
    commands.spawn((
        SpaceshipForgeRoot,
        Mesh3d(meshes.add(Torus::new(3.5, 3.8))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.025, 0.045, 0.085),
            emissive: LinearRgba::new(0.02, 0.18, 0.42, 1.0),
            metallic: 0.70,
            perceptual_roughness: 0.35,
            ..default()
        })),
        Transform::from_xyz(0.0, -2.25, 0.0),
    ));

    spawn_spaceship_forge_ui(&mut commands);
}

fn teardown_spaceship_forge(
    mut commands: Commands,
    roots: Query<Entity, Or<(With<SpaceshipForgeRoot>, With<SpaceshipPreviewRoot>)>>,
    mut text_capture: ResMut<AuthoringTextInputCapture>,
    mut unsaved: ResMut<AuthoringUnsavedChanges>,
) {
    text_capture.active = false;
    unsaved.active = false;
    for entity in roots.iter() {
        commands.entity(entity).despawn();
    }
}

fn widget_style() -> ForgeWidgetStyle {
    ForgeWidgetStyle {
        button_background: Color::srgb(0.08, 0.10, 0.22),
        button_border: Color::srgb(0.46, 0.34, 0.84),
        text: Color::srgb(0.94, 0.94, 1.0),
        min_width: 96.0,
        ..Default::default()
    }
}

fn forge_button(
    parent: &mut ChildSpawnerCommands,
    label: impl Into<String>,
    action: SpaceshipAction,
) {
    action_button(parent, label, SpaceshipButton(action), &widget_style());
}

fn forge_save_button(parent: &mut ChildSpawnerCommands, label: impl Into<String>) {
    action_button(
        parent,
        label,
        (SpaceshipButton(SpaceshipAction::Save), SpaceshipSaveButton),
        &widget_style(),
    );
}

fn window_style(content_height: f32, initially_minimized: bool) -> ToolWindowStyle {
    ToolWindowStyle {
        accent: Color::srgb(0.44, 0.30, 0.82),
        background: Color::srgba(0.025, 0.025, 0.085, 0.96),
        // Keep the two initially-open panels non-overlapping at the supported
        // 800 px / 1.4× compact workspace; controls wrap inside the window.
        width: 270.0,
        content_height: Val::Px(content_height),
        initially_minimized,
    }
}

fn spawn_spaceship_forge_ui(commands: &mut Commands) {
    commands
        .spawn((
            SpaceshipForgeRoot,
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
                Text::new("SPACESHIP FORGE"),
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
                TextColor(Color::srgb(0.74, 0.62, 1.0)),
            ));
            root.spawn((
                Text::new(""),
                SpaceshipStatusText,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(16.0),
                    top: Val::Px(46.0),
                    max_width: Val::Px(830.0),
                    ..default()
                },
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.86, 0.90, 1.0)),
            ));

            spawn_tool_window(
                root,
                "CLASS & HULL",
                Vec2::new(12.0, 84.0),
                window_style(300.0, false),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        SpaceshipIdentityText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.91, 0.90, 1.0)),
                    ));
                    section_label(panel, "ROLE PRESETS");
                    widget_row(panel, |row| {
                        for class in SpacecraftClass::ALL {
                            forge_button(
                                row,
                                class.label().to_uppercase(),
                                SpaceshipAction::Preset(class),
                            );
                        }
                    });
                    widget_row(panel, |row| {
                        forge_button(row, "NAME", SpaceshipAction::NameEdit);
                        forge_button(row, "PALETTE", SpaceshipAction::PaletteNext);
                    });
                    for field in SpacecraftField::HULL {
                        spawn_field_row(panel, field);
                    }
                },
            );

            spawn_tool_window(
                root,
                "SYSTEMS",
                Vec2::new(12.0, 430.0),
                window_style(340.0, true),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    for system in SpacecraftSystem::ALL {
                        spawn_system_row(panel, system);
                    }
                    for field in SpacecraftField::SYSTEM_TUNING {
                        spawn_field_row(panel, field);
                    }
                },
            );

            spawn_tool_window(
                root,
                "DERIVED PERFORMANCE",
                Vec2::new(958.0, 84.0),
                window_style(300.0, false),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    section_label(panel, "DERIVED FROM THE PHYSICAL BUILD");
                    panel.spawn((
                        Text::new(""),
                        SpaceshipStatsText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.62, 0.88, 1.0)),
                    ));
                    widget_row(panel, |row| {
                        forge_button(row, "VALIDATE", SpaceshipAction::Validate);
                        forge_button(row, "UNDO", SpaceshipAction::Undo);
                        forge_button(row, "RESET", SpaceshipAction::Reset);
                    });
                },
            );

            spawn_tool_window(
                root,
                "PROJECT LIBRARY",
                Vec2::new(958.0, 430.0),
                window_style(260.0, true),
                (
                    SpaceshipLibraryPanel,
                    MenuScrollPanel,
                    ScrollPosition::default(),
                ),
                |_panel| {},
            );

            spawn_tool_window(
                root,
                "WORKFLOW",
                Vec2::new(646.0, 84.0),
                ToolWindowStyle {
                    width: 260.0,
                    ..window_style(150.0, true)
                },
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new("Save writes a versioned spacecraft recipe. Publishing fills the read-only spacecraft catalog; gameplay motor/fleet binding stays explicit."),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.70, 0.74, 0.86)),
                    ));
                    widget_row(panel, |row| {
                        forge_save_button(row, "SAVE");
                        forge_button(row, "BACK", SpaceshipAction::Back);
                    });
                },
            );
        });
}

fn spawn_field_row(parent: &mut ChildSpawnerCommands, field: SpacecraftField) {
    widget_row(parent, |row| {
        row.spawn((
            Text::new(""),
            SpaceshipFieldText(field),
            Node {
                min_width: Val::Px(164.0),
                ..default()
            },
            TextFont {
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::srgb(0.84, 0.88, 0.96)),
        ));
        forge_button(row, "−", SpaceshipAction::Adjust(field, -field.step()));
        forge_button(row, "+", SpaceshipAction::Adjust(field, field.step()));
    });
}

fn spawn_system_row(parent: &mut ChildSpawnerCommands, system: SpacecraftSystem) {
    widget_row(parent, |row| {
        row.spawn((
            Text::new(""),
            SpaceshipSystemText(system),
            Node {
                min_width: Val::Px(164.0),
                ..default()
            },
            TextFont {
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::srgb(0.84, 0.88, 0.96)),
        ));
        forge_button(row, "−", SpaceshipAction::AdjustSystem(system, -1));
        forge_button(row, "+", SpaceshipAction::AdjustSystem(system, 1));
    });
}

const SPACESHIP_NAME_LIMIT: usize = 64;

fn sync_spaceship_name_input_capture(
    state: Res<SpaceshipForgeState>,
    mut capture: ResMut<AuthoringTextInputCapture>,
) {
    capture.active = state.naming;
}

fn sync_spaceship_unsaved_changes(
    state: Res<SpaceshipForgeState>,
    mut unsaved: ResMut<AuthoringUnsavedChanges>,
) {
    unsaved.active = state.has_unsaved_changes();
}

fn spaceship_name_input_system(
    keys: Res<ButtonInput<Key>>,
    mut state: ResMut<SpaceshipForgeState>,
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
                        && state.spec.display_name.chars().count() < SPACESHIP_NAME_LIMIT
                    {
                        state.spec.display_name.push(character);
                        changed = true;
                    }
                }
            }
            Key::Space if state.spec.display_name.chars().count() < SPACESHIP_NAME_LIMIT => {
                state.spec.display_name.push(' ');
                changed = true;
            }
            Key::Backspace => changed |= state.spec.display_name.pop().is_some(),
            Key::Enter | Key::Escape => {
                state.naming = false;
                changed = true;
            }
            _ => {}
        }
    }
    if changed {
        let name = state.spec.display_name.clone();
        let naming = state.naming;
        state.mark(if naming {
            format!("Naming: {name}_")
        } else {
            format!("Named: {name} (stable content ID retained)")
        });
    }
}

fn spaceship_action_system(
    interactions: Query<(&Interaction, &SpaceshipButton), (Changed<Interaction>, With<Button>)>,
    mut state: ResMut<SpaceshipForgeState>,
    mut text_capture: ResMut<AuthoringTextInputCapture>,
    registry: Res<ForgeProjectRegistry>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let action = interactions.iter().find_map(|(interaction, button)| {
        (*interaction == Interaction::Pressed).then_some(button.0)
    });
    let Some(action) = action else {
        return;
    };
    if state.naming && action != SpaceshipAction::NameEdit {
        return;
    }
    if !matches!(action, SpaceshipAction::LibraryLoad(_)) && state.cancel_library_load() {
        state.library_dirty = true;
    }
    if !matches!(action, SpaceshipAction::LibraryDelete(_)) && state.delete_armed.take().is_some() {
        state.library_dirty = true;
    }
    if action != SpaceshipAction::Back {
        state.exit_armed = false;
    }

    match action {
        SpaceshipAction::Preset(class) => {
            state.begin_edit();
            state.spec = SpacecraftSpec::preset(class);
            state.template_identity = true;
            state.mark(format!("Loaded {} role preset", class.label()));
        }
        SpaceshipAction::NameEdit => {
            if !state.naming {
                state.begin_edit();
            }
            state.naming = !state.naming;
            text_capture.active = state.naming;
            let name = state.spec.display_name.clone();
            let naming = state.naming;
            state.mark(if naming {
                format!("Naming: {name}_  (Enter to finish)")
            } else {
                format!("Named: {name} (stable content ID retained)")
            });
        }
        SpaceshipAction::Adjust(field, delta) => {
            state.begin_edit();
            let value = field.read(&state.spec) + delta;
            field.write(&mut state.spec, value);
            let label = field.value_label(&state.spec);
            state.mark(label);
        }
        SpaceshipAction::AdjustSystem(system, delta) => {
            state.begin_edit();
            system.write(&mut state.spec, delta);
            let value = system.read(&state.spec);
            state.mark(format!("{}: {value}", system.label()));
        }
        SpaceshipAction::PaletteNext => {
            state.begin_edit();
            state.spec.palette = state.spec.palette.next();
            state.mark("Palette changed");
        }
        SpaceshipAction::Validate => {
            let validation = state.spec.validate();
            state.status = if validation.issues.is_empty() {
                format!(
                    "READY — valid {}-part recipe with {:.0} spare reactor power",
                    validation.estimated_parts, validation.derived.power_margin
                )
            } else {
                format!(
                    "{} issue(s), {} blocking: {}",
                    validation.issues.len(),
                    validation.error_count(),
                    validation
                        .issues
                        .iter()
                        .take(3)
                        .map(|issue| issue.message.as_str())
                        .collect::<Vec<_>>()
                        .join(" • ")
                )
            };
            state.labels_dirty = true;
        }
        SpaceshipAction::Undo => {
            if let Some(spec) = state.undo.pop() {
                state.spec = spec;
                state.template_identity = is_template_identity(&state.spec);
                state.mark("Undo");
            } else {
                state.status = "Nothing to undo".into();
                state.labels_dirty = true;
            }
        }
        SpaceshipAction::Reset => {
            state.begin_edit();
            state.spec = SpacecraftSpec::preset(state.spec.class);
            state.template_identity = true;
            state.mark("Reset current role to its safe preset");
        }
        SpaceshipAction::Save => {
            let blocking = spaceship_records::blocking_issues(&state.spec);
            if !blocking.is_empty() {
                state.status = format!("SAVE BLOCKED — {}", blocking.join(" • "));
                state.labels_dirty = true;
                return;
            }
            match spaceship_records::save_to_active_project(&registry, &state.spec) {
                Ok(content_id) => {
                    state.spec.content_id = content_id.clone();
                    state.template_identity = false;
                    let message = match spaceship_records::list_active_project(&registry) {
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
        SpaceshipAction::LibraryLoad(index) => {
            let Some((content_id, name)) = state.library.get(index).cloned() else {
                return;
            };
            if !state.authorize_library_load(&content_id) {
                state.status = format!(
                    "Unsaved changes — press CONFIRM LOAD on {name} to replace this draft; any other Forge action cancels."
                );
                state.labels_dirty = true;
                state.library_dirty = true;
                return;
            }
            // Restore the ordinary LOAD label whether the confirmed read
            // succeeds or fails; a failed read still preserves the draft.
            state.library_dirty = true;
            match spaceship_records::load_from_active_project(&registry, &content_id) {
                Ok(spec) => {
                    state.spec = spec;
                    state.template_identity = false;
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
        SpaceshipAction::LibraryDelete(index) => {
            if !state.project_writable {
                state.status = "DELETE DISABLED — the active project is read-only".into();
                state.labels_dirty = true;
                return;
            }
            let Some((content_id, name)) = state.library.get(index).cloned() else {
                return;
            };
            if state.delete_armed.as_deref() != Some(content_id.as_str()) {
                state.delete_armed = Some(content_id);
                state.status = format!(
                    "Delete {name}? Press CONFIRM DELETE on this row; any other action cancels."
                );
                state.labels_dirty = true;
                state.library_dirty = true;
                return;
            }
            state.delete_armed = None;
            match spaceship_records::delete_from_active_project(&registry, &content_id) {
                Ok(true) => {
                    let mut message = match spaceship_records::list_active_project(&registry) {
                        Ok((library, writable)) => {
                            state.library = library;
                            state.project_writable = writable;
                            format!("Deleted {name}")
                        }
                        Err(error) => {
                            state.library.clear();
                            state.project_writable = false;
                            format!("Deleted, but could not refresh library: {error}")
                        }
                    };
                    if state.reset_if_current_deleted(&content_id) {
                        message.push_str(&format!(
                            "; started a fresh {} template",
                            state.spec.class.label()
                        ));
                    }
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
        SpaceshipAction::Back => {
            if state.has_unsaved_changes() && !state.exit_armed {
                state.exit_armed = true;
                state.status =
                    "Unsaved changes — press BACK again to discard them, or SAVE to keep them"
                        .into();
                state.labels_dirty = true;
                return;
            }
            if let Some(saved) = state.saved_spec.clone() {
                state.spec = saved;
                state.template_identity = is_template_identity(&state.spec);
            }
            state.undo.clear();
            state.naming = false;
            text_capture.active = false;
            next_state.set(AppState::ProjectHub);
        }
    }
}

fn spaceship_save_availability_system(
    mut commands: Commands,
    state: Res<SpaceshipForgeState>,
    buttons: Query<
        (Entity, Has<InteractionDisabled>, Has<MenuButtonDisabled>),
        With<SpaceshipSaveButton>,
    >,
) {
    let disabled =
        !state.project_writable || !spaceship_records::blocking_issues(&state.spec).is_empty();
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

fn spaceship_library_rebuild_system(
    mut commands: Commands,
    mut state: ResMut<SpaceshipForgeState>,
    panels: Query<(Entity, Option<&Children>), With<SpaceshipLibraryPanel>>,
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
            forge_button(row, "BACK", SpaceshipAction::Back);
        });
        if state.library.is_empty() {
            panel.spawn((
                Text::new("No saved spacecraft yet. Choose a role preset, tune it, then SAVE."),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.60, 0.64, 0.76)),
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
                    TextColor(Color::srgb(0.90, 0.90, 1.0)),
                ));
                let load_armed = state.load_armed.as_deref() == Some(content_id.as_str());
                forge_button(
                    row,
                    if load_armed { "CONFIRM LOAD" } else { "LOAD" },
                    SpaceshipAction::LibraryLoad(index),
                );
                let delete_armed = state.delete_armed.as_deref() == Some(content_id.as_str());
                forge_button(
                    row,
                    if delete_armed {
                        "CONFIRM DELETE"
                    } else {
                        "DELETE"
                    },
                    SpaceshipAction::LibraryDelete(index),
                );
            });
        }
    });
}

fn spaceship_preview_rebuild_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<SpaceshipForgeState>,
    previews: Query<Entity, With<SpaceshipPreviewRoot>>,
) {
    if !state.dirty {
        return;
    }
    state.dirty = false;
    for entity in previews.iter() {
        commands.entity(entity).despawn();
    }
    match spawn_spacecraft(
        &mut commands,
        &mut meshes,
        &mut materials,
        &state.spec,
        Vec3::ZERO,
    ) {
        Ok(entity) => {
            let scale = spacecraft_preview_scale(&state.spec);
            commands.entity(entity).insert((
                SpaceshipPreviewRoot,
                Transform::from_xyz(0.0, 0.15, 0.0).with_scale(Vec3::splat(scale)),
            ));
        }
        Err(validation) => {
            state.status = format!(
                "Preview blocked by {} invalid recipe field(s)",
                validation.error_count()
            );
            state.labels_dirty = true;
        }
    }
}

fn spaceship_label_refresh_system(
    mut state: ResMut<SpaceshipForgeState>,
    mut texts: ParamSet<(
        Query<&mut Text, With<SpaceshipStatusText>>,
        Query<&mut Text, With<SpaceshipIdentityText>>,
        Query<(&mut Text, &SpaceshipFieldText)>,
        Query<(&mut Text, &SpaceshipSystemText)>,
        Query<&mut Text, With<SpaceshipStatsText>>,
    )>,
) {
    if !state.labels_dirty {
        return;
    }
    state.labels_dirty = false;
    for mut text in texts.p0().iter_mut() {
        *text = Text::new(state.status.clone());
    }
    for mut text in texts.p1().iter_mut() {
        *text = Text::new(format!(
            "{}{}\n{}\n{}{}",
            state.spec.display_name,
            if state.naming { "_" } else { "" },
            state.spec.class.label(),
            state.spec.content_id,
            if state.template_identity {
                "  [new ID on first save]"
            } else {
                ""
            },
        ));
    }
    for (mut text, marker) in texts.p2().iter_mut() {
        *text = Text::new(marker.0.value_label(&state.spec));
    }
    for (mut text, marker) in texts.p3().iter_mut() {
        *text = Text::new(format!(
            "{}: {}",
            marker.0.label(),
            marker.0.read(&state.spec)
        ));
    }
    let stats = state.spec.derived_stats();
    let validation = state.spec.validate();
    let blocking = validation.error_count();
    let issue_lines = validation
        .issues
        .iter()
        .take(3)
        .map(|issue| {
            let prefix = if issue.severity == SpacecraftValidationSeverity::Error {
                "x"
            } else {
                "!"
            };
            format!("{prefix} {}", issue.message)
        })
        .collect::<Vec<_>>()
        .join("\n");
    for mut text in texts.p4().iter_mut() {
        *text = Text::new(format!(
            "Mass: {:.0} t\nHull: {}  Shields: {}\nCruise: {:.1}  Maneuver: {:.0}/100\nFirepower: {}\nHangar capacity: {}\nCommand capacity: {}\nCrew: {}\nPower: {:.0} / {:.0} ({:+.0})\n{}{}",
            stats.mass_tons,
            stats.hull_integrity,
            stats.shield_capacity,
            stats.cruise_speed,
            stats.maneuverability,
            stats.firepower,
            stats.hangar_capacity,
            stats.command_capacity,
            stats.crew_capacity,
            stats.reactor_output,
            stats.power_demand,
            stats.power_margin,
            if blocking == 0 {
                "READY TO SAVE"
            } else {
                "SAVE BLOCKED"
            },
            if issue_lines.is_empty() {
                String::new()
            } else {
                format!("\n{issue_lines}")
            },
        ));
    }
}

fn spaceship_preview_spin_system(
    time: Res<Time>,
    settings: Res<GameSettings>,
    mut previews: Query<&mut Transform, With<SpaceshipPreviewRoot>>,
) {
    if settings.reduced_ui_motion {
        return;
    }
    for mut transform in previews.iter_mut() {
        transform.rotate_y(time.delta_secs() * 0.24);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_tools::tool_windows::ToolWindow;

    #[test]
    fn all_major_panels_use_shared_windows_and_progressive_disclosure() {
        let mut app = App::new();
        {
            let world = app.world_mut();
            let mut commands = world.commands();
            spawn_spaceship_forge_ui(&mut commands);
        }
        app.world_mut().flush();
        let (all, minimized) = {
            let world = app.world_mut();
            let mut query = world.query::<&ToolWindow>();
            query.iter(world).fold((0, 0), |(all, minimized), window| {
                (all + 1, minimized + usize::from(window.minimized))
            })
        };
        assert_eq!(all, 5);
        assert_eq!(minimized, 3);
    }

    #[test]
    fn field_and_system_edits_stay_inside_compiler_limits() {
        let mut spec = SpacecraftSpec::preset(SpacecraftClass::Fighter);
        SpacecraftField::HullLength.write(&mut spec, 50_000.0);
        assert_eq!(spec.hull_length, 400.0);
        for _ in 0..80 {
            SpacecraftSystem::Weapons.write(&mut spec, 1);
        }
        assert_eq!(spec.weapon_mounts, 32);
        assert!(spec.validate().is_publishable());
    }

    #[test]
    fn preview_rebuild_uses_the_public_spacecraft_compiler() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>();
        let spec = SpacecraftSpec::preset(SpacecraftClass::CommandShip);
        let entity = app
            .world_mut()
            .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                    let mut commands = world.commands();
                    spawn_spacecraft(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &spec,
                        Vec3::ZERO,
                    )
                    .unwrap()
                })
            });
        app.world_mut().flush();
        assert_eq!(
            app.world()
                .get::<crate::spaceship_forge::GeneratedSpacecraft>(entity)
                .unwrap()
                .class,
            SpacecraftClass::CommandShip
        );
    }

    #[test]
    fn invalid_save_reason_is_visible_in_the_performance_readout_contract() {
        let mut state = SpaceshipForgeState::default();
        state.spec.display_name.clear();
        let blocking = spaceship_records::blocking_issues(&state.spec);
        assert!(!blocking.is_empty());
        assert!(blocking
            .iter()
            .any(|reason| reason.contains("Display name")));
    }

    #[test]
    fn named_templates_create_multiple_designs_but_loaded_identity_is_stable() {
        let mut first = SpacecraftSpec::preset(SpacecraftClass::Fighter);
        first.display_name = "Red Comet".into();
        let mut second = SpacecraftSpec::preset(SpacecraftClass::Fighter);
        second.display_name = "Blue Comet".into();
        let mut project = crate::engine_tools::persistence::ForgeProject::default();
        let first_id = spaceship_records::upsert_spaceship(&mut project, &first).unwrap();
        let second_id = spaceship_records::upsert_spaceship(&mut project, &second).unwrap();
        assert_ne!(first_id, second_id);

        let mut renamed = spaceship_records::load_spaceship(&project, &first_id).unwrap();
        renamed.display_name = "Red Comet Mark II".into();
        assert_eq!(
            spaceship_records::upsert_spaceship(&mut project, &renamed).unwrap(),
            first_id,
            "loaded designs retain their stable id when renamed"
        );
    }

    #[test]
    fn saved_bomber_and_destroyer_ids_do_not_reenter_as_templates() {
        for class in [SpacecraftClass::Bomber, SpacecraftClass::Destroyer] {
            let mut project = crate::engine_tools::persistence::ForgeProject::default();
            let preset = SpacecraftSpec::preset(class);
            let id = spaceship_records::upsert_spaceship(&mut project, &preset).unwrap();
            let loaded = spaceship_records::load_spaceship(&project, &id).unwrap();
            assert!(!is_template_identity(&loaded));
        }
    }

    #[test]
    fn dirty_library_load_requires_same_row_confirmation_and_preserves_the_draft() {
        let mut state = SpaceshipForgeState::default();
        state.spec.weapon_mounts += 1;
        let draft = state.spec.clone();

        assert!(!state.authorize_library_load("starfall.spaceship.alpha"));
        assert_eq!(state.spec, draft);
        assert_eq!(
            state.load_armed.as_deref(),
            Some("starfall.spaceship.alpha")
        );

        assert!(!state.authorize_library_load("starfall.spaceship.beta"));
        assert_eq!(state.spec, draft);
        assert_eq!(state.load_armed.as_deref(), Some("starfall.spaceship.beta"));
        assert!(state.authorize_library_load("starfall.spaceship.beta"));
        assert_eq!(state.spec, draft);
        assert!(state.load_armed.is_none());

        assert!(!state.authorize_library_load("starfall.spaceship.alpha"));
        assert!(state.cancel_library_load());
        assert!(state.load_armed.is_none());

        state.saved_spec = Some(draft.clone());
        assert!(state.authorize_library_load("starfall.spaceship.alpha"));
        assert!(state.load_armed.is_none());
    }

    #[test]
    fn deleting_current_design_resets_only_that_open_document() {
        let mut state = SpaceshipForgeState::default();
        let mut loaded = SpacecraftSpec::preset(SpacecraftClass::Destroyer);
        loaded.content_id = "starfall.spaceship.vanguard".into();
        state.spec = loaded.clone();
        state.saved_spec = Some(loaded);
        state.template_identity = false;
        state.undo.push(SpacecraftSpec::default());

        assert!(!state.reset_if_current_deleted("starfall.spaceship.someone_else"));
        assert_eq!(state.spec.content_id, "starfall.spaceship.vanguard");
        assert!(!state.template_identity);
        assert_eq!(state.undo.len(), 1);

        assert!(state.reset_if_current_deleted("starfall.spaceship.vanguard"));
        assert_eq!(state.spec.class, SpacecraftClass::Destroyer);
        assert_eq!(
            state.spec.content_id,
            SpacecraftSpec::preset(SpacecraftClass::Destroyer).content_id
        );
        assert!(state.template_identity);
        assert!(state.undo.is_empty());
        assert_eq!(state.saved_spec.as_ref(), Some(&state.spec));
        assert!(!state.has_unsaved_changes());
    }
}
