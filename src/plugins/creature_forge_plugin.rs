//! Creature Forge — the authoring screen for versioned creature recipes.
//!
//! Edits a [`CreatureSpec`] (kind, topology, role, faction, surface,
//! morphology, features, modifier stack, seed) against a live 3D preview
//! built by the proven robot geometry factory. The whole GUI is made of
//! movable, minimizable tool windows from `crate::engine_tools::tool_windows`, and specs
//! save as versioned JSON files under the platform data directory.

use bevy::prelude::*;

use super::ui_plugin::MenuScrollPanel;
use crate::character::mesh_modifiers::MeshModifier;
use crate::engine::platform_paths;
use crate::engine::state::AppState;
use crate::engine_tools::creature_records;
use crate::engine_tools::project_registry::ForgeProjectRegistry;
use crate::engine_tools::tool_windows::{spawn_tool_window, ToolWindowStyle};
use crate::robots::creature::{
    CreatureFaction, CreatureKind, CreatureRole, CreatureSpec, CreatureSurface, CreatureTopology,
};
use crate::robots::factory::spawn_creature;

pub struct CreatureForgePlugin;

impl Plugin for CreatureForgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ForgeState>()
            .init_resource::<ForgeLibrary>()
            .add_systems(OnEnter(AppState::CreatureForge), setup_creature_forge)
            .add_systems(OnExit(AppState::CreatureForge), despawn_creature_forge)
            .add_systems(
                Update,
                (
                    forge_action_system,
                    forge_preview_rebuild_system,
                    forge_label_refresh_system,
                    forge_preview_spin_system,
                )
                    .chain()
                    .run_if(in_state(AppState::CreatureForge)),
            );
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Resource)]
struct ForgeState {
    spec: CreatureSpec,
    undo: Vec<CreatureSpec>,
    dirty: bool,
    labels_dirty: bool,
    status: String,
    armed_modifier: usize,
    param_cursor: usize,
}

impl Default for ForgeState {
    fn default() -> Self {
        Self {
            spec: CreatureSpec::default(),
            undo: Vec::new(),
            dirty: true,
            labels_dirty: true,
            status: "Welcome to the Creature Forge".to_string(),
            armed_modifier: 0,
            param_cursor: 0,
        }
    }
}

impl ForgeState {
    fn push_undo(&mut self) {
        self.undo.push(self.spec.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
    }

    fn mark(&mut self, status: impl Into<String>) {
        self.status = status.into();
        self.dirty = true;
        self.labels_dirty = true;
    }
}

/// Saved spec files found in the forge library directory, plus the creature
/// records of the active project (the authoritative, published path).
#[derive(Resource, Default)]
struct ForgeLibrary {
    files: Vec<String>,
    project_creatures: Vec<(String, String)>,
    dirty: bool,
}

// ── Actions ───────────────────────────────────────────────────────────────────

#[derive(Component, Clone, Copy)]
struct ForgeButton(ForgeAction);

#[derive(Clone, Copy, PartialEq)]
enum ForgeAction {
    KindNext,
    TopologyNext,
    RoleNext,
    FactionNext,
    SurfaceNext,
    ArchetypePreset(usize),
    Adjust(ForgeField, f32),
    ToggleWings,
    ToggleCannons,
    ToggleBackpack,
    ModifierKind,
    ModifierAdd,
    ModifierRemove,
    ModParamCycle,
    ModValueDown,
    ModValueUp,
    NewSeed,
    Validate,
    Undo,
    Reset,
    SaveToProject,
    LoadProjectCreature(usize),
    SaveVersion,
    LoadFile(usize),
    Back,
}

/// Continuous morphology fields editable with −/+ buttons.
#[derive(Clone, Copy, PartialEq)]
enum ForgeField {
    Scale,
    TorsoWidth,
    TorsoHeight,
    TorsoDepth,
    HeadSize,
    ArmLength,
    ArmThickness,
    LegLength,
    LegThickness,
    WingSpan,
    CannonSize,
}

impl ForgeField {
    const ALL: [Self; 11] = [
        Self::Scale,
        Self::TorsoWidth,
        Self::TorsoHeight,
        Self::TorsoDepth,
        Self::HeadSize,
        Self::ArmLength,
        Self::ArmThickness,
        Self::LegLength,
        Self::LegThickness,
        Self::WingSpan,
        Self::CannonSize,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Scale => "Scale",
            Self::TorsoWidth => "Torso Width",
            Self::TorsoHeight => "Torso Height",
            Self::TorsoDepth => "Torso Depth",
            Self::HeadSize => "Head Size",
            Self::ArmLength => "Arm Length",
            Self::ArmThickness => "Arm Thick",
            Self::LegLength => "Leg Length",
            Self::LegThickness => "Leg Thick",
            Self::WingSpan => "Wing Span",
            Self::CannonSize => "Cannon Size",
        }
    }

    fn step(self) -> f32 {
        match self {
            Self::Scale => 0.1,
            _ => 2.0,
        }
    }

    fn read(self, spec: &CreatureSpec) -> f32 {
        let style = &spec.style;
        match self {
            Self::Scale => style.scale,
            Self::TorsoWidth => style.torso_width,
            Self::TorsoHeight => style.torso_height,
            Self::TorsoDepth => style.torso_depth,
            Self::HeadSize => style.head_size,
            Self::ArmLength => style.arm_length,
            Self::ArmThickness => style.arm_thickness,
            Self::LegLength => style.leg_length,
            Self::LegThickness => style.leg_thickness,
            Self::WingSpan => style.wing_span,
            Self::CannonSize => style.cannon_size,
        }
    }

    fn write(self, spec: &mut CreatureSpec, value: f32) {
        let style = &mut spec.style;
        match self {
            Self::Scale => style.scale = value.clamp(0.2, 4.0),
            Self::TorsoWidth => style.torso_width = value.clamp(6.0, 60.0),
            Self::TorsoHeight => style.torso_height = value.clamp(10.0, 90.0),
            Self::TorsoDepth => style.torso_depth = value.clamp(6.0, 50.0),
            Self::HeadSize => style.head_size = value.clamp(4.0, 40.0),
            Self::ArmLength => style.arm_length = value.clamp(6.0, 70.0),
            Self::ArmThickness => style.arm_thickness = value.clamp(2.0, 24.0),
            Self::LegLength => style.leg_length = value.clamp(6.0, 70.0),
            Self::LegThickness => style.leg_thickness = value.clamp(2.0, 26.0),
            Self::WingSpan => style.wing_span = value.clamp(6.0, 120.0),
            Self::CannonSize => style.cannon_size = value.clamp(2.0, 30.0),
        }
    }
}

// ── Cycling helpers ───────────────────────────────────────────────────────────

const KINDS: [CreatureKind; 2] = [CreatureKind::Robot, CreatureKind::Monster];
const TOPOLOGIES: [CreatureTopology; 6] = [
    CreatureTopology::Humanoid,
    CreatureTopology::Quadruped,
    CreatureTopology::Flyer,
    CreatureTopology::Crawler,
    CreatureTopology::Serpent,
    CreatureTopology::Drone,
];
const ROLES: [CreatureRole; 7] = [
    CreatureRole::Civilian,
    CreatureRole::Ally,
    CreatureRole::Scout,
    CreatureRole::Bruiser,
    CreatureRole::Artillery,
    CreatureRole::Boss,
    CreatureRole::Pet,
];
const FACTIONS: [CreatureFaction; 6] = [
    CreatureFaction::Starfall,
    CreatureFaction::Raider,
    CreatureFaction::Swarm,
    CreatureFaction::Caveborn,
    CreatureFaction::Ancient,
    CreatureFaction::Neutral,
];
const SURFACES: [CreatureSurface; 5] = [
    CreatureSurface::PaintedMetal,
    CreatureSurface::BareAlloy,
    CreatureSurface::Crystal,
    CreatureSurface::Organic,
    CreatureSurface::Energy,
];
const ARCHETYPE_PRESETS: [(&str, crate::robots::designer::RobotArchetype); 4] = [
    ("SCOUT", crate::robots::designer::RobotArchetype::Scout),
    ("BRUTE", crate::robots::designer::RobotArchetype::Brute),
    ("FLYER", crate::robots::designer::RobotArchetype::Flyer),
    ("TANK", crate::robots::designer::RobotArchetype::Tank),
];

fn next_in<T: Copy + PartialEq>(values: &[T], current: T) -> T {
    let index = values.iter().position(|v| *v == current).unwrap_or(0);
    values[(index + 1) % values.len()]
}

// ── Markers ───────────────────────────────────────────────────────────────────

#[derive(Component)]
struct ForgeRoot;
#[derive(Component)]
struct ForgePreviewRoot;
#[derive(Component)]
struct ForgeStatusText;
#[derive(Component)]
struct ForgeIdentityText;
#[derive(Component)]
struct ForgeFieldText(usize);
#[derive(Component)]
struct ForgeModifierText;
#[derive(Component)]
struct ForgeLibraryList;

// ── Setup / teardown ─────────────────────────────────────────────────────────

fn setup_creature_forge(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<ForgeState>,
    mut library: ResMut<ForgeLibrary>,
    registry: Res<ForgeProjectRegistry>,
) {
    state.dirty = true;
    state.labels_dirty = true;
    library.files = scan_forge_library();
    library.project_creatures = creature_records::list_active_project(&registry);
    library.dirty = true;

    commands.spawn((
        ForgeRoot,
        Camera3d::default(),
        Transform::from_xyz(0.0, 2.2, 6.5).looking_at(Vec3::new(0.0, 1.4, 0.0), Vec3::Y),
    ));
    commands.spawn((
        ForgeRoot,
        DirectionalLight {
            illuminance: 11_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(3.0, 5.0, 3.0).looking_at(Vec3::new(0.0, 1.2, 0.0), Vec3::Y),
    ));
    commands.spawn((
        ForgeRoot,
        DirectionalLight {
            illuminance: 3_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-3.5, 2.5, -2.5).looking_at(Vec3::new(0.0, 1.2, 0.0), Vec3::Y),
    ));
    commands.spawn((
        ForgeRoot,
        Mesh3d(meshes.add(Mesh::from(Cylinder::new(2.4, 0.08)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.20, 0.22, 0.27),
            perceptual_roughness: 0.65,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.04, 0.0),
    ));

    spawn_forge_ui(&mut commands);
}

fn despawn_creature_forge(
    mut commands: Commands,
    roots: Query<Entity, Or<(With<ForgeRoot>, With<ForgePreviewRoot>)>>,
) {
    for entity in roots.iter() {
        commands.entity(entity).despawn();
    }
}

// ── UI construction ───────────────────────────────────────────────────────────

fn forge_button(parent: &mut ChildSpawnerCommands, label: String, action: ForgeAction) {
    parent
        .spawn((
            Button,
            ForgeButton(action),
            Node {
                min_width: Val::Px(108.0),
                min_height: Val::Px(24.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.10, 0.16, 0.24)),
            BorderColor::all(Color::srgb(0.20, 0.44, 0.62)),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn forge_row(parent: &mut ChildSpawnerCommands, content: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(6.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(content);
}

fn spawn_forge_ui(commands: &mut Commands) {
    commands
        .spawn((
            ForgeRoot,
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
                Text::new("CREATURE FORGE"),
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
                TextColor(Color::srgb(0.55, 1.0, 0.75)),
            ));
            root.spawn((
                Text::new(""),
                ForgeStatusText,
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
                TextColor(Color::srgb(0.85, 0.92, 1.0)),
            ));

            let style = |height: f32| ToolWindowStyle {
                accent: Color::srgb(0.16, 0.55, 0.40),
                width: 300.0,
                content_height: Val::Px(height),
                ..Default::default()
            };

            spawn_tool_window(
                root,
                "CREATURE",
                Vec2::new(12.0, 84.0),
                style(240.0),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    panel.spawn((
                        Text::new(""),
                        ForgeIdentityText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.85, 0.95, 1.0)),
                    ));
                    forge_row(panel, |row| {
                        forge_button(row, "KIND".into(), ForgeAction::KindNext);
                        forge_button(row, "TOPOLOGY".into(), ForgeAction::TopologyNext);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "ROLE".into(), ForgeAction::RoleNext);
                        forge_button(row, "FACTION".into(), ForgeAction::FactionNext);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "SURFACE".into(), ForgeAction::SurfaceNext);
                        forge_button(row, "NEW SEED".into(), ForgeAction::NewSeed);
                    });
                    forge_row(panel, |row| {
                        for (index, (label, _)) in ARCHETYPE_PRESETS.iter().enumerate() {
                            forge_button(
                                row,
                                (*label).to_string(),
                                ForgeAction::ArchetypePreset(index),
                            );
                        }
                    });
                },
            );

            spawn_tool_window(
                root,
                "MORPHOLOGY",
                Vec2::new(12.0, 380.0),
                style(300.0),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    for (index, field) in ForgeField::ALL.iter().enumerate() {
                        forge_row(panel, |row| {
                            row.spawn((
                                Text::new(""),
                                ForgeFieldText(index),
                                Node {
                                    min_width: Val::Px(150.0),
                                    ..default()
                                },
                                TextFont {
                                    font_size: FontSize::Px(13.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.85, 0.92, 1.0)),
                            ));
                            forge_button(
                                row,
                                "−".into(),
                                ForgeAction::Adjust(*field, -field.step()),
                            );
                            forge_button(
                                row,
                                "+".into(),
                                ForgeAction::Adjust(*field, field.step()),
                            );
                        });
                    }
                },
            );

            spawn_tool_window(
                root,
                "FEATURES & MODIFIERS",
                Vec2::new(966.0, 84.0),
                style(240.0),
                (MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    forge_row(panel, |row| {
                        forge_button(row, "WINGS".into(), ForgeAction::ToggleWings);
                        forge_button(row, "CANNONS".into(), ForgeAction::ToggleCannons);
                        forge_button(row, "BACKPACK".into(), ForgeAction::ToggleBackpack);
                    });
                    panel.spawn((
                        Text::new(""),
                        ForgeModifierText,
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.75, 0.90, 0.85)),
                    ));
                    forge_row(panel, |row| {
                        forge_button(row, "MOD KIND".into(), ForgeAction::ModifierKind);
                        forge_button(row, "ADD MOD".into(), ForgeAction::ModifierAdd);
                        forge_button(row, "REMOVE MOD".into(), ForgeAction::ModifierRemove);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "MOD PARAM".into(), ForgeAction::ModParamCycle);
                        forge_button(row, "VALUE −".into(), ForgeAction::ModValueDown);
                        forge_button(row, "VALUE +".into(), ForgeAction::ModValueUp);
                    });
                },
            );

            spawn_tool_window(
                root,
                "LIBRARY",
                Vec2::new(966.0, 380.0),
                style(300.0),
                (ForgeLibraryList, MenuScrollPanel, ScrollPosition::default()),
                |panel| {
                    forge_row(panel, |row| {
                        forge_button(row, "VALIDATE".into(), ForgeAction::Validate);
                        forge_button(row, "SAVE TO PROJECT".into(), ForgeAction::SaveToProject);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "EXPORT PRESET".into(), ForgeAction::SaveVersion);
                    });
                    forge_row(panel, |row| {
                        forge_button(row, "UNDO".into(), ForgeAction::Undo);
                        forge_button(row, "RESET".into(), ForgeAction::Reset);
                        forge_button(row, "BACK".into(), ForgeAction::Back);
                    });
                },
            );
        });
}

// ── Systems ───────────────────────────────────────────────────────────────────

fn forge_action_system(
    interactions: Query<(&Interaction, &ForgeButton), (Changed<Interaction>, With<Button>)>,
    mut state: ResMut<ForgeState>,
    mut library: ResMut<ForgeLibrary>,
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

    match action {
        ForgeAction::KindNext => {
            state.push_undo();
            state.spec.kind = next_in(&KINDS, state.spec.kind);
            let message = format!("Kind: {:?}", state.spec.kind);
            state.mark(message);
        }
        ForgeAction::TopologyNext => {
            state.push_undo();
            state.spec.topology = next_in(&TOPOLOGIES, state.spec.topology);
            let message = format!("Topology: {:?}", state.spec.topology);
            state.mark(message);
        }
        ForgeAction::RoleNext => {
            state.push_undo();
            state.spec.role = next_in(&ROLES, state.spec.role);
            let message = format!("Role: {:?}", state.spec.role);
            state.mark(message);
        }
        ForgeAction::FactionNext => {
            state.push_undo();
            state.spec.faction = next_in(&FACTIONS, state.spec.faction);
            let message = format!("Faction: {:?}", state.spec.faction);
            state.mark(message);
        }
        ForgeAction::SurfaceNext => {
            state.push_undo();
            state.spec.surface = next_in(&SURFACES, state.spec.surface);
            let message = format!("Surface: {:?}", state.spec.surface);
            state.mark(message);
        }
        ForgeAction::ArchetypePreset(index) => {
            let (label, archetype) = ARCHETYPE_PRESETS[index % ARCHETYPE_PRESETS.len()];
            state.push_undo();
            let modifiers = state.spec.style.modifiers.clone();
            state.spec.style = crate::robots::designer::RobotStyle::for_archetype(archetype);
            state.spec.style.modifiers = modifiers;
            state.mark(format!("Preset: {label}"));
        }
        ForgeAction::Adjust(field, delta) => {
            state.push_undo();
            let value = field.read(&state.spec) + delta;
            field.write(&mut state.spec, value);
            let message = format!("{}: {:.1}", field.label(), field.read(&state.spec));
            state.mark(message);
        }
        ForgeAction::ToggleWings => {
            state.push_undo();
            state.spec.style.has_wings = !state.spec.style.has_wings;
            let message = format!("Wings: {}", state.spec.style.has_wings);
            state.mark(message);
        }
        ForgeAction::ToggleCannons => {
            state.push_undo();
            state.spec.style.has_cannons = !state.spec.style.has_cannons;
            let message = format!("Cannons: {}", state.spec.style.has_cannons);
            state.mark(message);
        }
        ForgeAction::ToggleBackpack => {
            state.push_undo();
            state.spec.style.has_backpack = !state.spec.style.has_backpack;
            let message = format!("Backpack: {}", state.spec.style.has_backpack);
            state.mark(message);
        }
        ForgeAction::ModifierKind => {
            state.armed_modifier = (state.armed_modifier + 1) % MeshModifier::TEMPLATE_COUNT;
            let label = MeshModifier::template(state.armed_modifier).label();
            state.status = format!("Armed modifier: {label}");
            state.labels_dirty = true;
        }
        ForgeAction::ModifierAdd => {
            if state.spec.style.modifiers.len() >= 6 {
                state.status = "Modifier stack is full (6)".into();
                state.labels_dirty = true;
                return;
            }
            state.push_undo();
            let modifier = MeshModifier::template(state.armed_modifier);
            let label = modifier.label();
            state.spec.style.modifiers.push(modifier);
            state.mark(format!("Added {label}"));
        }
        ForgeAction::ModifierRemove => {
            if state.spec.style.modifiers.is_empty() {
                state.status = "No modifiers to remove".into();
                state.labels_dirty = true;
                return;
            }
            state.push_undo();
            let removed = state.spec.style.modifiers.pop();
            state.mark(format!(
                "Removed {}",
                removed.map(|m| m.label()).unwrap_or_default()
            ));
        }
        ForgeAction::NewSeed => {
            state.push_undo();
            state.spec.seed = state
                .spec
                .seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let message = format!("Seed: {}", state.spec.seed);
            state.mark(message);
        }
        ForgeAction::Validate => {
            let validation = state.spec.validate();
            state.status = if validation.issues.is_empty() {
                format!(
                    "Valid — estimated {} parts, publishable",
                    validation.estimated_parts
                )
            } else {
                let mut lines = validation
                    .issues
                    .iter()
                    .map(|issue| issue.message.clone())
                    .collect::<Vec<_>>();
                lines.truncate(3);
                format!(
                    "{} issue(s): {}",
                    validation.issues.len(),
                    lines.join(" • ")
                )
            };
            state.labels_dirty = true;
        }
        ForgeAction::Undo => {
            if let Some(previous) = state.undo.pop() {
                state.spec = previous;
                state.mark("Undo");
            } else {
                state.status = "Nothing to undo".into();
                state.labels_dirty = true;
            }
        }
        ForgeAction::Reset => {
            state.push_undo();
            state.spec = CreatureSpec::default();
            state.mark("Reset to default creature");
        }
        ForgeAction::SaveToProject => {
            let mut spec = state.spec.clone();
            match creature_records::save_to_active_project(&registry, &mut spec) {
                Ok(content_id) => {
                    state.spec = spec;
                    library.project_creatures = creature_records::list_active_project(&registry);
                    library.dirty = true;
                    state.status = format!(
                        "Saved {content_id} to {}",
                        creature_records::active_project_label(&registry)
                    );
                    state.labels_dirty = true;
                }
                Err(error) => {
                    state.status = error;
                    state.labels_dirty = true;
                }
            }
        }
        ForgeAction::LoadProjectCreature(index) => {
            let Some((content_id, _)) = library.project_creatures.get(index).cloned() else {
                return;
            };
            match creature_records::load_from_active_project(&registry, &content_id) {
                Ok(spec) => {
                    state.push_undo();
                    state.spec = spec;
                    state.mark(format!("Loaded {content_id} from project"));
                }
                Err(error) => {
                    state.status = format!("Load failed: {error}");
                    state.labels_dirty = true;
                }
            }
        }
        ForgeAction::ModParamCycle => {
            let Some(last) = state.spec.style.modifiers.last().copied() else {
                state.status = "Add a modifier first".into();
                state.labels_dirty = true;
                return;
            };
            state.param_cursor = (state.param_cursor + 1) % last.param_count();
            let cursor = state.param_cursor;
            state.status = format!("{}: {}", last.label(), last.param_label(cursor));
            state.labels_dirty = true;
        }
        ForgeAction::ModValueDown | ForgeAction::ModValueUp => {
            let direction = if action == ForgeAction::ModValueUp {
                1
            } else {
                -1
            };
            if state.spec.style.modifiers.is_empty() {
                state.status = "Add a modifier first".into();
                state.labels_dirty = true;
                return;
            }
            state.push_undo();
            let cursor = state.param_cursor;
            let Some(last) = state.spec.style.modifiers.last_mut() else {
                return;
            };
            last.adjust_param(cursor, direction);
            let message = format!("{}: {}", last.label(), last.param_label(cursor));
            state.mark(message);
        }
        ForgeAction::SaveVersion => match save_forge_version(&state.spec, &library.files) {
            Ok(name) => {
                library.files = scan_forge_library();
                library.dirty = true;
                state.status = format!("Saved {name}");
                state.labels_dirty = true;
            }
            Err(error) => {
                state.status = format!("Save failed: {error}");
                state.labels_dirty = true;
            }
        },
        ForgeAction::LoadFile(index) => {
            let Some(name) = library.files.get(index).cloned() else {
                return;
            };
            match load_forge_version(&name) {
                Ok(spec) => {
                    state.push_undo();
                    state.spec = spec;
                    state.mark(format!("Loaded {name}"));
                }
                Err(error) => {
                    state.status = format!("Load failed: {error}");
                    state.labels_dirty = true;
                }
            }
        }
        ForgeAction::Back => {
            next_state.set(AppState::ProjectHub);
        }
    }
}

fn forge_preview_rebuild_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<ForgeState>,
    previews: Query<Entity, With<ForgePreviewRoot>>,
) {
    if !state.dirty {
        return;
    }
    state.dirty = false;
    for entity in previews.iter() {
        commands.entity(entity).despawn();
    }
    match spawn_creature(
        &mut commands,
        &mut meshes,
        &mut materials,
        &state.spec,
        Vec3::ZERO,
    ) {
        Ok(root) => {
            commands.entity(root).insert(ForgePreviewRoot);
        }
        Err(validation) => {
            // Invalid recipes still preview through the raw factory so the
            // creator can see what they are fixing.
            let root = crate::robots::factory::spawn_robot(
                &mut commands,
                &mut meshes,
                &mut materials,
                &state.spec.compiled_style(),
                Vec3::ZERO,
            );
            commands.entity(root).insert(ForgePreviewRoot);
            state.status = format!(
                "Previewing draft with {} validation issue(s)",
                validation.issues.len()
            );
            state.labels_dirty = true;
        }
    }
}

fn forge_preview_spin_system(
    time: Res<Time>,
    mut previews: Query<&mut Transform, With<ForgePreviewRoot>>,
) {
    for mut transform in previews.iter_mut() {
        transform.rotate_y(time.delta_secs() * 0.5);
    }
}

fn forge_label_refresh_system(
    mut commands: Commands,
    mut state: ResMut<ForgeState>,
    mut library: ResMut<ForgeLibrary>,
    mut texts: ParamSet<(
        Query<&mut Text, With<ForgeStatusText>>,
        Query<&mut Text, With<ForgeIdentityText>>,
        Query<(&mut Text, &ForgeFieldText)>,
        Query<&mut Text, With<ForgeModifierText>>,
    )>,
    library_list: Query<Entity, With<ForgeLibraryList>>,
    library_buttons: Query<(Entity, &ForgeButton)>,
) {
    if !state.labels_dirty && !library.dirty {
        return;
    }
    state.labels_dirty = false;

    for mut text in texts.p0().iter_mut() {
        *text = Text::new(state.status.clone());
    }
    for mut text in texts.p1().iter_mut() {
        *text = Text::new(format!(
            "{:?} {:?} • {:?} • {:?} • {:?}\nseed {} • {} modifier(s)",
            state.spec.kind,
            state.spec.topology,
            state.spec.role,
            state.spec.faction,
            state.spec.surface,
            state.spec.seed,
            state.spec.style.modifiers.len(),
        ));
    }
    for (mut text, field_text) in texts.p2().iter_mut() {
        let field = ForgeField::ALL[field_text.0 % ForgeField::ALL.len()];
        *text = Text::new(format!("{}: {:.1}", field.label(), field.read(&state.spec)));
    }
    for mut text in texts.p3().iter_mut() {
        let stack = state
            .spec
            .style
            .modifiers
            .iter()
            .map(|modifier| modifier.label())
            .collect::<Vec<_>>();
        *text = Text::new(if stack.is_empty() {
            format!(
                "Stack empty — armed: {}",
                MeshModifier::template(state.armed_modifier).label()
            )
        } else {
            format!(
                "Stack: {} — armed: {}",
                stack.join(" → "),
                MeshModifier::template(state.armed_modifier).label()
            )
        });
    }

    if library.dirty {
        library.dirty = false;
        // Rebuild the LOAD button list: drop previous load buttons, then list
        // the active project's creatures (authoritative) ahead of the
        // exported preset files.
        for (entity, button) in library_buttons.iter() {
            if matches!(
                button.0,
                ForgeAction::LoadFile(_) | ForgeAction::LoadProjectCreature(_)
            ) {
                commands.entity(entity).despawn();
            }
        }
        if let Ok(list) = library_list.single() {
            let project_creatures = library.project_creatures.clone();
            let files = library.files.clone();
            commands.entity(list).with_children(|panel| {
                for (index, (_, display_name)) in project_creatures.iter().enumerate() {
                    forge_button(
                        panel,
                        format!("PROJ {display_name}"),
                        ForgeAction::LoadProjectCreature(index),
                    );
                }
                for (index, name) in files.iter().enumerate() {
                    forge_button(panel, format!("LOAD {name}"), ForgeAction::LoadFile(index));
                }
            });
        }
    }
}

// ── Library persistence ───────────────────────────────────────────────────────

fn forge_library_dir() -> std::path::PathBuf {
    platform_paths::data_root().join("creature_forge")
}

fn scan_forge_library() -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(forge_library_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                    files.push(name.to_string());
                }
            }
        }
    }
    files.sort();
    files
}

fn save_forge_version(spec: &CreatureSpec, existing: &[String]) -> Result<String, String> {
    let dir = forge_library_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = (1..)
        .map(|n| format!("creature_v{n:03}"))
        .find(|candidate| !existing.contains(candidate))
        .expect("unbounded numbering always finds a free version name");
    let json = serde_json::to_string_pretty(spec).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{name}.json")), json).map_err(|e| e.to_string())?;
    Ok(name)
}

fn load_forge_version(name: &str) -> Result<CreatureSpec, String> {
    let path = forge_library_dir().join(format!("{name}.json"));
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_adjustments_clamp_to_safe_ranges() {
        let mut spec = CreatureSpec::default();
        ForgeField::Scale.write(&mut spec, 100.0);
        assert_eq!(ForgeField::Scale.read(&spec), 4.0);
        ForgeField::TorsoWidth.write(&mut spec, -50.0);
        assert_eq!(ForgeField::TorsoWidth.read(&spec), 6.0);
    }

    #[test]
    fn enum_cycles_cover_every_variant_and_wrap() {
        let mut topology = CreatureTopology::Humanoid;
        for _ in 0..TOPOLOGIES.len() {
            topology = next_in(&TOPOLOGIES, topology);
        }
        assert_eq!(topology, CreatureTopology::Humanoid);
        assert_eq!(next_in(&KINDS, CreatureKind::Monster), CreatureKind::Robot);
    }

    #[test]
    fn version_names_advance_past_existing_files() {
        let existing = ["creature_v001".to_string(), "creature_v002".to_string()];
        let next = (1..)
            .map(|n| format!("creature_v{n:03}"))
            .find(|candidate| !existing.contains(candidate))
            .unwrap();
        assert_eq!(next, "creature_v003");
    }
}
