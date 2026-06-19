//! Chassis editor — live 3D preview with named part presets per slot.
//!
//! Keys Q/W/E/R cycle Body/Arms/Legs/Shoulders through their preset variants.
//! Keys 1-5 cycle the accent colour preset.
//! +/- adjust the preview scale. Enter confirms. Esc exits without saving.

use bevy::input::gamepad::{GamepadButton, GamepadButtonStateChangedEvent};
use bevy::input::ButtonState;
use bevy::prelude::*;

use crate::character_parts::{
    ArmPreset, BodyPreset, CharacterLoadout, HeadPreset, LegPreset, ShoulderPreset,
};
use crate::characters::{hero_config, spawn_cartoon_character};
use crate::plugins::input_plugin::{NativeButton, NativeControllerState};
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
    body: BodyPreset,
    arms: ArmPreset,
    legs: LegPreset,
    shoulders: ShoulderPreset,
    head: HeadPreset,
    preview_scale: f32,
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
    Head,
}

// ── Setup ─────────────────────────────────────────────────────────────────────

fn setup_editor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut data: ResMut<ChassisEditorData>,
    loadout: Res<PlayerPartLoadout>,
    select_state: Res<PlayerSelectState>,
    chassis: Res<PlayerChassis>,
    mut cam_q: Query<&mut Transform, With<Camera3d>>,
) {
    // Restore last saved loadout so editor opens with previous choices.
    let slot_loadout = select_state.slots[0].part_loadout.unwrap_or(*loadout);
    data.body = slot_loadout.body;
    data.arms = slot_loadout.arms;
    data.legs = slot_loadout.legs;
    data.shoulders = slot_loadout.shoulders;
    data.head = slot_loadout.head;
    data.spin_angle = 0.0;
    data.preview_scale = chassis.0.scale.clamp(0.5, 2.0);

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
    commands.entity(preview).insert((
        ChassisPreviewRoot,
        Transform::from_scale(Vec3::splat(data.preview_scale)),
    ));
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
                    (SlotId::Head, "T", "Head"),
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
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    mut data: ResMut<ChassisEditorData>,
    mut chassis: ResMut<PlayerChassis>,
    mut part_loadout: ResMut<PlayerPartLoadout>,
    mut select_state: ResMut<PlayerSelectState>,
    mut next_state: ResMut<NextState<AppState>>,
    mut preview_q: Query<&mut Transform, With<ChassisPreviewRoot>>,
    mut loadout_q: Query<&mut CharacterLoadout>,
) {
    let mut toggle_body = keyboard.just_pressed(KeyCode::KeyQ);
    let mut toggle_arms = keyboard.just_pressed(KeyCode::KeyW);
    let mut toggle_legs = keyboard.just_pressed(KeyCode::KeyE);
    let mut toggle_shoulders = keyboard.just_pressed(KeyCode::KeyR);
    let mut toggle_head = keyboard.just_pressed(KeyCode::KeyT);
    let mut scale_up =
        keyboard.just_pressed(KeyCode::Equal) || keyboard.just_pressed(KeyCode::NumpadAdd);
    let mut scale_down =
        keyboard.just_pressed(KeyCode::Minus) || keyboard.just_pressed(KeyCode::NumpadSubtract);
    let mut confirm =
        keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);
    let mut cancel = keyboard.just_pressed(KeyCode::Escape);

    for event in button_events.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        match event.button {
            GamepadButton::DPadUp => toggle_body = true,
            GamepadButton::DPadRight => toggle_arms = true,
            GamepadButton::DPadDown => toggle_legs = true,
            GamepadButton::DPadLeft => toggle_shoulders = true,
            GamepadButton::North => toggle_head = true,
            GamepadButton::RightTrigger => scale_up = true,
            GamepadButton::LeftTrigger => scale_down = true,
            GamepadButton::South | GamepadButton::Start => confirm = true,
            GamepadButton::East => cancel = true,
            _ => {}
        }
    }

    for gp in gamepads.iter() {
        toggle_body = toggle_body || gp.just_pressed(GamepadButton::DPadUp);
        toggle_arms = toggle_arms || gp.just_pressed(GamepadButton::DPadRight);
        toggle_legs = toggle_legs || gp.just_pressed(GamepadButton::DPadDown);
        toggle_shoulders = toggle_shoulders || gp.just_pressed(GamepadButton::DPadLeft);
        toggle_head = toggle_head || gp.just_pressed(GamepadButton::North);
        scale_up = scale_up || gp.just_pressed(GamepadButton::RightTrigger);
        scale_down = scale_down || gp.just_pressed(GamepadButton::LeftTrigger);
        confirm = confirm
            || gp.just_pressed(GamepadButton::South)
            || gp.just_pressed(GamepadButton::Start);
        cancel = cancel || gp.just_pressed(GamepadButton::East);
    }

    toggle_body = toggle_body || native.just_pressed(NativeButton::DPadUp);
    toggle_arms = toggle_arms || native.just_pressed(NativeButton::DPadRight);
    toggle_legs = toggle_legs || native.just_pressed(NativeButton::DPadDown);
    toggle_shoulders = toggle_shoulders || native.just_pressed(NativeButton::DPadLeft);
    toggle_head = toggle_head || native.just_pressed(NativeButton::North);
    scale_up = scale_up || native.just_pressed(NativeButton::RightShoulder);
    scale_down = scale_down || native.just_pressed(NativeButton::LeftShoulder);
    confirm = confirm
        || native.just_pressed(NativeButton::South)
        || native.just_pressed(NativeButton::Start);
    cancel = cancel || native.just_pressed(NativeButton::East);

    // Slot toggles
    let toggled = if toggle_body {
        Some(SlotId::Body)
    } else if toggle_arms {
        Some(SlotId::Arms)
    } else if toggle_legs {
        Some(SlotId::Legs)
    } else if toggle_shoulders {
        Some(SlotId::Shoulders)
    } else if toggle_head {
        Some(SlotId::Head)
    } else {
        None
    };

    if let Some(slot) = toggled {
        match slot {
            SlotId::Body => data.body = data.body.cycle(),
            SlotId::Arms => data.arms = data.arms.cycle(),
            SlotId::Legs => data.legs = data.legs.cycle(),
            SlotId::Shoulders => data.shoulders = data.shoulders.cycle(),
            SlotId::Head => data.head = data.head.cycle(),
        };

        // Apply to the live preview character immediately
        if let Some(preview) = data.preview_entity {
            if let Ok(mut cl) = loadout_q.get_mut(preview) {
                cl.body = data.body;
                cl.arms = data.arms;
                cl.legs = data.legs;
                cl.shoulders = data.shoulders;
                cl.head = data.head;
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
    if scale_up {
        chassis.0.scale = (chassis.0.scale + 0.1).min(2.0);
    }
    if scale_down {
        chassis.0.scale = (chassis.0.scale - 0.1).max(0.5);
    }
    if scale_up || scale_down {
        data.preview_scale = chassis.0.scale;
        for mut transform in preview_q.iter_mut() {
            transform.scale = Vec3::splat(data.preview_scale);
        }
    }

    // Confirm — save to PlayerPartLoadout
    if confirm {
        let loadout = PlayerPartLoadout {
            body: data.body,
            arms: data.arms,
            legs: data.legs,
            shoulders: data.shoulders,
            head: data.head,
        };
        *part_loadout = loadout;
        select_state.slots[0].part_loadout = Some(loadout);
        next_state.set(AppState::ChapterSelect);
    }

    // Cancel
    if cancel {
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
        let name = match label.0 {
            SlotId::Body => data.body.label(),
            SlotId::Arms => data.arms.label(),
            SlotId::Legs => data.legs.label(),
            SlotId::Shoulders => data.shoulders.label(),
            SlotId::Head => data.head.label(),
        };
        text.0 = format!("◄ {name} ►");
        *color = TextColor(Color::srgb(0.4, 0.95, 1.0));
    }
}
