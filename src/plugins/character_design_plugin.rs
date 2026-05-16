use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;

use crate::characters::{
    accent_presets, hair_presets, hero_config, hero_config_with_overrides, outfit_presets,
    spawn_cartoon_character,
};
use crate::resources::{CharacterDesignData, PlayerSelectState};
use crate::state::AppState;

pub struct CharacterDesignPlugin;

impl Plugin for CharacterDesignPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::CharacterDesign), setup_character_design)
            .add_systems(OnExit(AppState::CharacterDesign), cleanup_character_design)
            .add_systems(
                Update,
                (
                    spin_preview,
                    rebuild_preview_if_dirty,
                    swatch_interaction,
                    accessory_interaction,
                    button_interaction,
                    design_keyboard_input,
                    update_swatch_borders,
                    update_toggle_colors,
                )
                    .run_if(in_state(AppState::CharacterDesign)),
            );
    }
}

// ── Marker components ─────────────────────────────────────────────────────────

#[derive(Component)]
struct DesignUiRoot;

#[derive(Component)]
struct DesignLight;

#[derive(Component)]
struct PreviewRoot;

#[derive(Component)]
struct BackButton;

#[derive(Component)]
struct ConfirmButton;

#[derive(Component, Clone, Copy)]
struct SwatchButton {
    category: SwatchCategory,
    index: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum SwatchCategory {
    Outfit,
    Accent,
    Hair,
}

#[derive(Component, Clone, Copy, PartialEq)]
enum AccessoryToggle {
    Cape,
    ShoulderPads,
    Visor,
}

// ── Setup ─────────────────────────────────────────────────────────────────────

fn setup_character_design(
    mut commands: Commands,
    mut design_data: ResMut<CharacterDesignData>,
    select_state: Res<PlayerSelectState>,
    mut cam_q: Query<&mut Transform, With<Camera3d>>,
) {
    // Pre-populate design data from current slot overrides (or hero defaults)
    let slot = &select_state.slots[design_data.player_index];
    let base = hero_config(select_state.character_name(design_data.player_index));
    design_data.outfit_idx = slot.outfit_idx.unwrap_or(0);
    design_data.accent_idx = slot.accent_idx.unwrap_or(0);
    design_data.hair_idx = slot.hair_idx.unwrap_or(0);
    design_data.has_cape = slot.has_cape.unwrap_or(base.has_cape);
    design_data.has_shoulder_pads = slot.has_shoulder_pads.unwrap_or(base.has_shoulder_pads);
    design_data.has_visor = slot.has_visor.unwrap_or(base.has_visor);
    design_data.spin_angle = 0.0;
    design_data.dirty = true;
    design_data.preview_entity = None;

    // Reposition the menu camera for a character preview angle
    if let Ok(mut t) = cam_q.get_single_mut() {
        *t = Transform::from_xyz(0.0, 0.5, 3.2)
            .looking_at(Vec3::new(0.0, -0.1, 0.0), Vec3::Y);
    }

    // Key fill light (warm)
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.92, 0.82),
            intensity: 3500.0,
            range: 14.0,
            ..default()
        },
        Transform::from_xyz(-2.0, 3.5, 2.5),
        DesignLight,
    ));
    // Rim / fill light (cool)
    commands.spawn((
        PointLight {
            color: Color::srgb(0.55, 0.70, 1.0),
            intensity: 1400.0,
            range: 10.0,
            ..default()
        },
        Transform::from_xyz(2.5, 1.5, -1.5),
        DesignLight,
    ));

    spawn_design_ui(&mut commands, &select_state, &design_data);
}

// ── Cleanup ───────────────────────────────────────────────────────────────────

fn cleanup_character_design(
    mut commands: Commands,
    ui_q: Query<Entity, With<DesignUiRoot>>,
    lights_q: Query<Entity, With<DesignLight>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for e in ui_q.iter() {
        commands.entity(e).despawn_recursive();
    }
    for e in lights_q.iter() {
        commands.entity(e).despawn_recursive();
    }
    if let Some(e) = design_data.preview_entity.take() {
        commands.entity(e).despawn_recursive();
    }
}

// ── Spin preview ──────────────────────────────────────────────────────────────

fn spin_preview(
    time: Res<Time>,
    mut design_data: ResMut<CharacterDesignData>,
    mut preview_q: Query<&mut Transform, With<PreviewRoot>>,
) {
    design_data.spin_angle += time.delta_secs() * 0.7;
    if let Ok(mut t) = preview_q.get_single_mut() {
        t.rotation = Quat::from_rotation_y(design_data.spin_angle);
    }
}

// ── Rebuild preview when dirty ────────────────────────────────────────────────

fn rebuild_preview_if_dirty(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut design_data: ResMut<CharacterDesignData>,
    select_state: Res<PlayerSelectState>,
) {
    if !design_data.dirty {
        return;
    }
    if let Some(e) = design_data.preview_entity.take() {
        commands.entity(e).despawn_recursive();
    }

    let name = select_state.character_name(design_data.player_index);
    let config = hero_config_with_overrides(
        name,
        Some(outfit_presets()[design_data.outfit_idx]),
        Some(accent_presets()[design_data.accent_idx]),
        Some(hair_presets()[design_data.hair_idx]),
        Some(design_data.has_cape),
        Some(design_data.has_shoulder_pads),
        Some(design_data.has_visor),
    );

    let entity = spawn_cartoon_character(&mut commands, &mut meshes, &mut materials, config, Vec3::ZERO);
    commands.entity(entity).insert(PreviewRoot);
    design_data.preview_entity = Some(entity);
    design_data.dirty = false;
}

// ── Swatch clicks ─────────────────────────────────────────────────────────────

fn swatch_interaction(
    interaction_q: Query<(&Interaction, &SwatchButton), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for (interaction, swatch) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            match swatch.category {
                SwatchCategory::Outfit => design_data.outfit_idx = swatch.index,
                SwatchCategory::Accent => design_data.accent_idx = swatch.index,
                SwatchCategory::Hair => design_data.hair_idx = swatch.index,
            }
            design_data.dirty = true;
        }
    }
}

// ── Accessory toggles ─────────────────────────────────────────────────────────

fn accessory_interaction(
    interaction_q: Query<(&Interaction, &AccessoryToggle), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for (interaction, toggle) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            match *toggle {
                AccessoryToggle::Cape => design_data.has_cape = !design_data.has_cape,
                AccessoryToggle::ShoulderPads => {
                    design_data.has_shoulder_pads = !design_data.has_shoulder_pads
                }
                AccessoryToggle::Visor => design_data.has_visor = !design_data.has_visor,
            }
            design_data.dirty = true;
        }
    }
}

// ── Back / Confirm buttons ────────────────────────────────────────────────────

fn button_interaction(
    back_q: Query<&Interaction, (Changed<Interaction>, With<BackButton>)>,
    confirm_q: Query<&Interaction, (Changed<Interaction>, With<ConfirmButton>)>,
    design_data: Res<CharacterDesignData>,
    mut select_state: ResMut<PlayerSelectState>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in back_q.iter() {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::PlayerSelect);
            return;
        }
    }
    for interaction in confirm_q.iter() {
        if *interaction == Interaction::Pressed {
            save_design(&design_data, &mut select_state);
            next_state.set(AppState::PlayerSelect);
            return;
        }
    }
}

// ── Keyboard / gamepad input ──────────────────────────────────────────────────

fn design_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    design_data: Res<CharacterDesignData>,
    mut select_state: ResMut<PlayerSelectState>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let mut go_back = keys.just_pressed(KeyCode::Escape);
    let mut go_confirm =
        keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);

    for gp in gamepads.iter() {
        if gp.just_pressed(GamepadButton::East) {
            go_back = true;
        }
        if gp.just_pressed(GamepadButton::South) {
            go_confirm = true;
        }
    }

    if go_confirm {
        save_design(&design_data, &mut select_state);
        next_state.set(AppState::PlayerSelect);
    } else if go_back {
        next_state.set(AppState::PlayerSelect);
    }
}

fn save_design(design_data: &CharacterDesignData, select_state: &mut PlayerSelectState) {
    let slot = &mut select_state.slots[design_data.player_index];
    slot.outfit_idx = Some(design_data.outfit_idx);
    slot.accent_idx = Some(design_data.accent_idx);
    slot.hair_idx = Some(design_data.hair_idx);
    slot.has_cape = Some(design_data.has_cape);
    slot.has_shoulder_pads = Some(design_data.has_shoulder_pads);
    slot.has_visor = Some(design_data.has_visor);
}

// ── Highlight active swatches ─────────────────────────────────────────────────

fn update_swatch_borders(
    design_data: Res<CharacterDesignData>,
    mut swatch_q: Query<(&SwatchButton, &mut BorderColor)>,
) {
    if !design_data.is_changed() {
        return;
    }
    for (swatch, mut border) in swatch_q.iter_mut() {
        let selected = match swatch.category {
            SwatchCategory::Outfit => design_data.outfit_idx == swatch.index,
            SwatchCategory::Accent => design_data.accent_idx == swatch.index,
            SwatchCategory::Hair => design_data.hair_idx == swatch.index,
        };
        *border = BorderColor(if selected {
            Color::WHITE
        } else {
            Color::srgba(0.0, 0.0, 0.0, 0.0)
        });
    }
}

fn update_toggle_colors(
    design_data: Res<CharacterDesignData>,
    mut toggle_q: Query<(&AccessoryToggle, &mut BackgroundColor)>,
) {
    if !design_data.is_changed() {
        return;
    }
    for (toggle, mut bg) in toggle_q.iter_mut() {
        let active = match *toggle {
            AccessoryToggle::Cape => design_data.has_cape,
            AccessoryToggle::ShoulderPads => design_data.has_shoulder_pads,
            AccessoryToggle::Visor => design_data.has_visor,
        };
        *bg = BackgroundColor(if active {
            Color::srgb(0.10, 0.50, 0.15)
        } else {
            Color::srgb(0.15, 0.15, 0.22)
        });
    }
}

// ── UI construction ───────────────────────────────────────────────────────────

fn spawn_design_ui(
    commands: &mut Commands,
    select_state: &PlayerSelectState,
    design_data: &CharacterDesignData,
) {
    let hero_name = select_state.character_name(design_data.player_index);
    let outfits = outfit_presets();
    let accents = accent_presets();
    let hairs = hair_presets();

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                ..default()
            },
            DesignUiRoot,
        ))
        .with_children(|root| {
            // Left half: transparent — the 3D character renders through here
            root.spawn(Node {
                width: Val::Percent(50.0),
                height: Val::Percent(100.0),
                ..default()
            });

            // Right half: opaque design panel
            root.spawn((
                Node {
                    width: Val::Percent(50.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(24.0)),
                    row_gap: Val::Px(8.0),
                    overflow: Overflow::clip_y(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.04, 0.04, 0.10, 0.97)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(format!("CUSTOMIZE\n{}", hero_name.to_uppercase())),
                    TextFont { font_size: 26.0, ..default() },
                    TextColor(Color::srgb(1.0, 0.9, 0.25)),
                ));

                panel.spawn(Node { height: Val::Px(6.0), ..default() });

                spawn_swatch_row(
                    panel, "OUTFIT", SwatchCategory::Outfit,
                    &outfits, design_data.outfit_idx,
                );
                spawn_swatch_row(
                    panel, "ACCENT", SwatchCategory::Accent,
                    &accents, design_data.accent_idx,
                );
                spawn_swatch_row(
                    panel, "HAIR", SwatchCategory::Hair,
                    &hairs, design_data.hair_idx,
                );

                panel.spawn(Node { height: Val::Px(6.0), ..default() });

                panel.spawn((
                    Text::new("ACCESSORIES"),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.70, 0.72, 0.92)),
                ));
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_toggle(row, "CAPE", AccessoryToggle::Cape, design_data.has_cape);
                        spawn_toggle(
                            row, "PADS",
                            AccessoryToggle::ShoulderPads, design_data.has_shoulder_pads,
                        );
                        spawn_toggle(
                            row, "VISOR",
                            AccessoryToggle::Visor, design_data.has_visor,
                        );
                    });

                // Expand to push buttons to bottom
                panel.spawn(Node { flex_grow: 1.0, ..default() });

                // Back / Confirm row
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(12.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Button,
                            Node { padding: UiRect::all(Val::Px(14.0)), ..default() },
                            BackgroundColor(Color::srgb(0.35, 0.08, 0.08)),
                            BackButton,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("← BACK"),
                                TextFont { font_size: 18.0, ..default() },
                                TextColor(Color::WHITE),
                            ));
                        });

                        row.spawn((
                            Button,
                            Node { padding: UiRect::all(Val::Px(14.0)), ..default() },
                            BackgroundColor(Color::srgb(0.08, 0.38, 0.12)),
                            ConfirmButton,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("CONFIRM"),
                                TextFont { font_size: 18.0, ..default() },
                                TextColor(Color::WHITE),
                            ));
                        });
                    });

                panel.spawn((
                    Text::new("Click swatches to customize  |  ESC: back  |  Enter: confirm"),
                    TextFont { font_size: 11.0, ..default() },
                    TextColor(Color::srgba(0.5, 0.5, 0.7, 0.8)),
                ));
            });
        });
}

fn spawn_swatch_row(
    parent: &mut ChildBuilder,
    label: &str,
    category: SwatchCategory,
    colors: &[Color; 8],
    selected: usize,
) {
    parent.spawn((
        Text::new(label),
        TextFont { font_size: 13.0, ..default() },
        TextColor(Color::srgb(0.70, 0.72, 0.92)),
    ));
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(5.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|row| {
            for (i, &color) in colors.iter().enumerate() {
                let is_sel = i == selected;
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(38.0),
                        height: Val::Px(38.0),
                        border: UiRect::all(Val::Px(if is_sel { 3.0 } else { 1.0 })),
                        ..default()
                    },
                    BackgroundColor(color),
                    BorderColor(if is_sel {
                        Color::WHITE
                    } else {
                        Color::srgba(0.0, 0.0, 0.0, 0.0)
                    }),
                    SwatchButton { category, index: i },
                ));
            }
        });
}

fn spawn_toggle(
    parent: &mut ChildBuilder,
    label: &str,
    toggle: AccessoryToggle,
    active: bool,
) {
    parent
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(if active {
                Color::srgb(0.10, 0.50, 0.15)
            } else {
                Color::srgb(0.15, 0.15, 0.22)
            }),
            toggle,
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::WHITE),
            ));
        });
}
