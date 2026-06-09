//! Chassis editor — live 3D preview with swappable humanoid/robot part slots.
//!
//! Keys Q/W/E/R toggle Body/Arms/Legs/Shoulders between Humanoid and Robot.
//! Keys 1-5 cycle the robot accent colour preset.
//! +/- adjust the preview scale (informational; applied to the saved PlayerChassis).
//! Enter confirms and saves to `PlayerPartLoadout`. Esc exits without saving.

use bevy::prelude::*;

use crate::character_parts::{CharacterLoadout, CharacterPartStyle};
use crate::characters::{hero_config, spawn_cartoon_character};
use crate::resources::{PlayerChassis, PlayerPartLoadout, PlayerSelectState};
use crate::robots::presets::{amp, atlas, theta, valor, volt};
use crate::state::AppState;

pub struct ChassisEditorPlugin;

impl Plugin for ChassisEditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerChassis>()
            .init_resource::<ChassisEditorData>()
            .add_systems(OnEnter(AppState::ChassisEditor), setup_editor)
            .add_systems(OnExit(AppState::ChassisEditor), teardown_editor)
            .add_systems(
                Update,
                (spin_preview, editor_keyboard_input, update_slot_labels)
                    .run_if(in_state(AppState::ChassisEditor)),
            );
    }
}

// ── Editor state ──────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct ChassisEditorData {
    preview_entity: Option<Entity>,
    spin_angle: f32,
    body: CharacterPartStyle,
    arms: CharacterPartStyle,
    legs: CharacterPartStyle,
    shoulders: CharacterPartStyle,
}

// ── Marker components ─────────────────────────────────────────────────────────

#[derive(Component)]
struct ChassisEditorRoot;

#[derive(Component)]
struct ChassisEditorLight;

#[derive(Component)]
struct ChassisPreviewRoot;

#[derive(Component)]
struct SlotLabel(SlotId);

#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotId {
    Body,
    Arms,
    Legs,
    Shoulders,
}

// ── Setup ─────────────────────────────────────────────────────────────────────

fn setup_editor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut data: ResMut<ChassisEditorData>,
    loadout: Res<PlayerPartLoadout>,
    select_state: Res<PlayerSelectState>,
    mut cam_q: Query<&mut Transform, With<Camera3d>>,
) {
    // Restore last saved loadout so editor opens with previous choices.
    data.body = loadout.body;
    data.arms = loadout.arms;
    data.legs = loadout.legs;
    data.shoulders = loadout.shoulders;
    data.spin_angle = 0.0;

    // Position camera the same as the character design preview.
    if let Ok(mut t) = cam_q.single_mut() {
        *t = Transform::from_xyz(0.0, 0.5, -3.2).looking_at(Vec3::new(0.0, -0.1, 0.0), Vec3::Y);
    }

    // Warm key light
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.92, 0.82),
            intensity: 3500.0,
            range: 14.0,
            ..default()
        },
        Transform::from_xyz(-2.0, 3.5, -2.5),
        ChassisEditorLight,
    ));
    // Cool rim light
    commands.spawn((
        PointLight {
            color: Color::srgb(0.55, 0.70, 1.0),
            intensity: 1400.0,
            range: 10.0,
            ..default()
        },
        Transform::from_xyz(2.5, 1.5, 1.5),
        ChassisEditorLight,
    ));

    // 3D character preview
    let hero_name = select_state.character_name(0);
    let config = hero_config(hero_name);
    let preview = spawn_cartoon_character(
        &mut commands,
        &mut meshes,
        &mut materials,
        config,
        Vec3::ZERO,
    );
    commands.entity(preview).insert(ChassisPreviewRoot);
    data.preview_entity = Some(preview);

    // UI panel (right 42% of screen, transparent left side shows 3D)
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                ..default()
            },
            ChassisEditorRoot,
        ))
        .with_children(|row| {
            // Left spacer — transparent, 3D shows through
            row.spawn(Node {
                width: Val::Percent(58.0),
                height: Val::Percent(100.0),
                ..default()
            });

            // Right panel
            row.spawn((
                Node {
                    width: Val::Percent(42.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(28.0), Val::Px(32.0)),
                    row_gap: Val::Px(14.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.03, 0.08, 0.94)),
            ))
            .with_children(|panel| {
                // Title
                panel.spawn((
                    Text::new("CHASSIS EDITOR"),
                    TextFont {
                        font_size: 34.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.4, 0.85, 1.0)),
                ));

                // Divider label
                panel.spawn((
                    Text::new("── PART STYLES ──"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.5, 0.6, 0.75)),
                ));

                // Part slot rows
                for (slot_id, key, label) in [
                    (SlotId::Body, "Q", "Body"),
                    (SlotId::Arms, "W", "Arms"),
                    (SlotId::Legs, "E", "Legs"),
                    (SlotId::Shoulders, "R", "Shoulders"),
                ] {
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(10.0),
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Text::new(format!("[{key}] {label:<12}")),
                                TextFont {
                                    font_size: 17.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.75, 0.85, 1.0)),
                            ));
                            row.spawn((
                                Text::new("Humanoid"),
                                TextFont {
                                    font_size: 17.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 0.9, 0.6)),
                                SlotLabel(slot_id),
                            ));
                        });
                }

                // Divider label
                panel.spawn((
                    Text::new("── ROBOT ACCENT PRESET ──"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.5, 0.6, 0.75)),
                ));
                panel.spawn((
                    Text::new("[1] Amp  [2] Atlas  [3] Volt  [4] Valor  [5] Theta"),
                    TextFont {
                        font_size: 15.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.8, 0.85, 0.95)),
                ));

                // Divider label
                panel.spawn((
                    Text::new("── CONTROLS ──"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.5, 0.6, 0.75)),
                ));
                panel.spawn((
                    Text::new("[+/-]  Scale preview\n[Enter]  Save & return\n[Esc]   Cancel"),
                    TextFont {
                        font_size: 15.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.7, 0.75, 0.85)),
                ));
            });
        });
}

// ── Teardown ──────────────────────────────────────────────────────────────────

fn teardown_editor(
    mut commands: Commands,
    ui_q: Query<Entity, With<ChassisEditorRoot>>,
    lights_q: Query<Entity, With<ChassisEditorLight>>,
    preview_q: Query<Entity, With<ChassisPreviewRoot>>,
    mut data: ResMut<ChassisEditorData>,
) {
    for e in ui_q.iter() {
        commands.entity(e).despawn();
    }
    for e in lights_q.iter() {
        commands.entity(e).despawn();
    }
    for e in preview_q.iter() {
        commands.entity(e).despawn();
    }
    data.preview_entity = None;
}

// ── Spin ──────────────────────────────────────────────────────────────────────

fn spin_preview(
    time: Res<Time>,
    mut data: ResMut<ChassisEditorData>,
    mut preview_q: Query<&mut Transform, With<ChassisPreviewRoot>>,
) {
    data.spin_angle += time.delta_secs() * 0.6;
    for mut t in preview_q.iter_mut() {
        t.rotation = Quat::from_rotation_y(data.spin_angle);
    }
}

// ── Keyboard input ────────────────────────────────────────────────────────────

fn editor_keyboard_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut data: ResMut<ChassisEditorData>,
    mut chassis: ResMut<PlayerChassis>,
    mut part_loadout: ResMut<PlayerPartLoadout>,
    mut next_state: ResMut<NextState<AppState>>,
    _preview_q: Query<Entity, With<ChassisPreviewRoot>>,
    mut loadout_q: Query<&mut CharacterLoadout>,
) {
    // Slot toggles
    let toggled = if keyboard.just_pressed(KeyCode::KeyQ) {
        Some(SlotId::Body)
    } else if keyboard.just_pressed(KeyCode::KeyW) {
        Some(SlotId::Arms)
    } else if keyboard.just_pressed(KeyCode::KeyE) {
        Some(SlotId::Legs)
    } else if keyboard.just_pressed(KeyCode::KeyR) {
        Some(SlotId::Shoulders)
    } else {
        None
    };

    if let Some(slot) = toggled {
        let style = match slot {
            SlotId::Body => &mut data.body,
            SlotId::Arms => &mut data.arms,
            SlotId::Legs => &mut data.legs,
            SlotId::Shoulders => &mut data.shoulders,
        };
        *style = match *style {
            CharacterPartStyle::HumanoidClothing => CharacterPartStyle::RobotMechanical,
            CharacterPartStyle::RobotMechanical => CharacterPartStyle::HumanoidClothing,
        };

        // Apply to the live preview character immediately
        if let Some(preview) = data.preview_entity {
            if let Ok(mut cl) = loadout_q.get_mut(preview) {
                cl.body = data.body;
                cl.arms = data.arms;
                cl.legs = data.legs;
                cl.shoulders = data.shoulders;
            }
        }
    }

    // Robot accent preset keys
    if keyboard.just_pressed(KeyCode::Digit1) {
        chassis.0 = amp();
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        chassis.0 = atlas();
    }
    if keyboard.just_pressed(KeyCode::Digit3) {
        chassis.0 = volt();
    }
    if keyboard.just_pressed(KeyCode::Digit4) {
        chassis.0 = valor();
    }
    if keyboard.just_pressed(KeyCode::Digit5) {
        chassis.0 = theta();
    }

    // Scale
    if keyboard.just_pressed(KeyCode::Equal) || keyboard.just_pressed(KeyCode::NumpadAdd) {
        chassis.0.scale = (chassis.0.scale + 0.1).min(2.0);
    }
    if keyboard.just_pressed(KeyCode::Minus) || keyboard.just_pressed(KeyCode::NumpadSubtract) {
        chassis.0.scale = (chassis.0.scale - 0.1).max(0.5);
    }

    // Confirm — save to PlayerPartLoadout
    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        part_loadout.body = data.body;
        part_loadout.arms = data.arms;
        part_loadout.legs = data.legs;
        part_loadout.shoulders = data.shoulders;
        next_state.set(AppState::ChapterSelect);
    }

    // Cancel
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::ChapterSelect);
    }
}

// ── Slot label update ─────────────────────────────────────────────────────────

fn update_slot_labels(
    data: Res<ChassisEditorData>,
    mut label_q: Query<(&SlotLabel, &mut Text, &mut TextColor)>,
) {
    if !data.is_changed() {
        return;
    }
    for (label, mut text, mut color) in label_q.iter_mut() {
        let style = match label.0 {
            SlotId::Body => data.body,
            SlotId::Arms => data.arms,
            SlotId::Legs => data.legs,
            SlotId::Shoulders => data.shoulders,
        };
        match style {
            CharacterPartStyle::HumanoidClothing => {
                text.0 = "◄ Humanoid ►".to_string();
                *color = TextColor(Color::srgb(1.0, 0.90, 0.55));
            }
            CharacterPartStyle::RobotMechanical => {
                text.0 = "◄ Robot    ►".to_string();
                *color = TextColor(Color::srgb(0.4, 0.85, 1.0));
            }
        }
    }
}
