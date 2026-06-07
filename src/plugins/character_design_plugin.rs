use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;

use crate::character_blueprint::{
    BodyRecipe, CartoonAppearanceRecipe, CharacterBlueprint, CharacterPaletteRecipe,
};
use crate::characters::{
    accent_preset, accent_presets, hair_preset, hair_presets, hero_config,
    hero_config_with_overrides, normalize_color_preset_index, outfit_preset, outfit_presets,
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
                    body_stepper_interaction,
                    button_interaction,
                    design_keyboard_input,
                    update_swatch_borders,
                    update_toggle_colors,
                    update_body_value_text,
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
    Hood,
    Cape,
    Gloves,
    Boots,
    ShoulderPads,
    Visor,
}

#[derive(Component, Clone, Copy)]
struct BodyStepper {
    field: BodyField,
    delta: f32,
}

#[derive(Component, Clone, Copy)]
struct BodyValueText(BodyField);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum BodyField {
    Height,
    Shoulders,
    Chest,
    Arms,
    Legs,
    Hands,
    Feet,
    Head,
    Mass,
}

// ── Setup ─────────────────────────────────────────────────────────────────────

fn setup_character_design(
    mut commands: Commands,
    mut design_data: ResMut<CharacterDesignData>,
    select_state: Res<PlayerSelectState>,
    mut cam_q: Query<&mut Transform, With<Camera3d>>,
) {
    design_data.player_index = design_data.player_index.min(select_state.slots.len() - 1);

    // Pre-populate design data from current slot overrides (or hero defaults)
    let slot = &select_state.slots[design_data.player_index];
    let base = hero_config(select_state.character_name(design_data.player_index));
    design_data.outfit_idx = slot
        .outfit_idx
        .map(normalize_color_preset_index)
        .unwrap_or(0);
    design_data.accent_idx = slot
        .accent_idx
        .map(normalize_color_preset_index)
        .unwrap_or(0);
    design_data.hair_idx = slot.hair_idx.map(normalize_color_preset_index).unwrap_or(0);
    design_data.body = slot
        .blueprint
        .as_ref()
        .map(|blueprint| blueprint.body)
        .unwrap_or_else(BodyRecipe::default)
        .validated();
    let blueprint_appearance = slot
        .blueprint
        .as_ref()
        .map(|blueprint| blueprint.cartoon_appearance);
    design_data.has_hood = slot
        .has_hood
        .or(blueprint_appearance.map(|appearance| appearance.has_hood))
        .unwrap_or(base.has_hood);
    design_data.has_cape = slot
        .has_cape
        .or(blueprint_appearance.map(|appearance| appearance.has_cape))
        .unwrap_or(base.has_cape);
    design_data.has_gloves = slot
        .has_gloves
        .or(blueprint_appearance.map(|appearance| appearance.has_gloves))
        .unwrap_or(base.has_gloves);
    design_data.has_boots = slot
        .has_boots
        .or(blueprint_appearance.map(|appearance| appearance.has_boots))
        .unwrap_or(base.has_boots);
    design_data.has_shoulder_pads = slot
        .has_shoulder_pads
        .or(blueprint_appearance.map(|appearance| appearance.has_shoulder_pads))
        .unwrap_or(base.has_shoulder_pads);
    design_data.has_visor = slot
        .has_visor
        .or(blueprint_appearance.map(|appearance| appearance.has_visor))
        .unwrap_or(base.has_visor);
    design_data.spin_angle = 0.0;
    design_data.dirty = true;
    design_data.preview_entity = None;

    // Reposition the menu camera for a character preview angle
    if let Ok(mut t) = cam_q.single_mut() {
        *t = Transform::from_xyz(0.0, 0.5, -3.2).looking_at(Vec3::new(0.0, -0.1, 0.0), Vec3::Y);
    }

    // Key fill light (warm)
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.92, 0.82),
            intensity: 3500.0,
            range: 14.0,
            ..default()
        },
        Transform::from_xyz(-2.0, 3.5, -2.5),
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
        Transform::from_xyz(2.5, 1.5, 1.5),
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
        commands.entity(e).despawn();
    }
    for e in lights_q.iter() {
        commands.entity(e).despawn();
    }
    if let Some(e) = design_data.preview_entity.take() {
        commands.entity(e).despawn();
    }
}

// ── Spin preview ──────────────────────────────────────────────────────────────

fn spin_preview(
    time: Res<Time>,
    mut design_data: ResMut<CharacterDesignData>,
    mut preview_q: Query<&mut Transform, With<PreviewRoot>>,
) {
    design_data.spin_angle += time.delta_secs() * 0.7;
    if let Ok(mut t) = preview_q.single_mut() {
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
        commands.entity(e).despawn();
    }

    design_data.player_index = design_data.player_index.min(select_state.slots.len() - 1);
    design_data.outfit_idx = normalize_color_preset_index(design_data.outfit_idx);
    design_data.accent_idx = normalize_color_preset_index(design_data.accent_idx);
    design_data.hair_idx = normalize_color_preset_index(design_data.hair_idx);

    let name = select_state.character_name(design_data.player_index);
    let config = hero_config_with_overrides(
        name,
        Some(outfit_preset(design_data.outfit_idx)),
        Some(accent_preset(design_data.accent_idx)),
        Some(hair_preset(design_data.hair_idx)),
        Some(design_data.has_hood),
        Some(design_data.has_cape),
        Some(design_data.has_gloves),
        Some(design_data.has_boots),
        Some(design_data.has_shoulder_pads),
        Some(design_data.has_visor),
    )
    .with_body_recipe(design_data.body);

    let entity = spawn_cartoon_character(
        &mut commands,
        &mut meshes,
        &mut materials,
        config,
        Vec3::ZERO,
    );
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
                AccessoryToggle::Hood => design_data.has_hood = !design_data.has_hood,
                AccessoryToggle::Cape => design_data.has_cape = !design_data.has_cape,
                AccessoryToggle::Gloves => design_data.has_gloves = !design_data.has_gloves,
                AccessoryToggle::Boots => design_data.has_boots = !design_data.has_boots,
                AccessoryToggle::ShoulderPads => {
                    design_data.has_shoulder_pads = !design_data.has_shoulder_pads
                }
                AccessoryToggle::Visor => design_data.has_visor = !design_data.has_visor,
            }
            design_data.dirty = true;
        }
    }
}

fn body_stepper_interaction(
    interaction_q: Query<(&Interaction, &BodyStepper), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for (interaction, stepper) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        adjust_body_field(&mut design_data.body, stepper.field, stepper.delta);
        design_data.body.validate();
        design_data.dirty = true;
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
    let name = select_state.character_name(design_data.player_index);
    let base = hero_config(name);
    let mut body = design_data.body;
    body.validate();
    let blueprint = CharacterBlueprint::hero(
        name,
        body,
        CharacterPaletteRecipe {
            skin: base.skin,
            outfit: outfit_preset(design_data.outfit_idx),
            accent: accent_preset(design_data.accent_idx),
            hair: hair_preset(design_data.hair_idx),
            eye: base.eye_color,
        },
        CartoonAppearanceRecipe {
            has_hood: design_data.has_hood,
            has_cape: design_data.has_cape,
            has_gloves: design_data.has_gloves,
            has_boots: design_data.has_boots,
            has_shoulder_pads: design_data.has_shoulder_pads,
            has_visor: design_data.has_visor,
        },
    );

    let Some(slot) = select_state.slots.get_mut(design_data.player_index) else {
        return;
    };
    slot.outfit_idx = Some(normalize_color_preset_index(design_data.outfit_idx));
    slot.accent_idx = Some(normalize_color_preset_index(design_data.accent_idx));
    slot.hair_idx = Some(normalize_color_preset_index(design_data.hair_idx));
    slot.has_hood = Some(design_data.has_hood);
    slot.has_cape = Some(design_data.has_cape);
    slot.has_gloves = Some(design_data.has_gloves);
    slot.has_boots = Some(design_data.has_boots);
    slot.has_shoulder_pads = Some(design_data.has_shoulder_pads);
    slot.has_visor = Some(design_data.has_visor);
    slot.blueprint = Some(blueprint);
}

// ── Highlight active swatches ─────────────────────────────────────────────────

fn update_swatch_borders(
    design_data: Res<CharacterDesignData>,
    mut swatch_q: Query<(&SwatchButton, &mut BorderColor, &mut Node)>,
) {
    if !design_data.is_changed() {
        return;
    }
    for (swatch, mut border, mut node) in swatch_q.iter_mut() {
        let selected = match swatch.category {
            SwatchCategory::Outfit => design_data.outfit_idx == swatch.index,
            SwatchCategory::Accent => design_data.accent_idx == swatch.index,
            SwatchCategory::Hair => design_data.hair_idx == swatch.index,
        };
        node.border = UiRect::all(Val::Px(if selected { 3.0 } else { 1.0 }));
        *border = BorderColor::all(if selected {
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
            AccessoryToggle::Hood => design_data.has_hood,
            AccessoryToggle::Cape => design_data.has_cape,
            AccessoryToggle::Gloves => design_data.has_gloves,
            AccessoryToggle::Boots => design_data.has_boots,
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

fn update_body_value_text(
    design_data: Res<CharacterDesignData>,
    mut value_q: Query<(&BodyValueText, &mut Text)>,
) {
    if !design_data.is_changed() {
        return;
    }
    for (marker, mut text) in value_q.iter_mut() {
        *text = Text::new(format!(
            "{:.2}",
            body_field_value(&design_data.body, marker.0)
        ));
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
                    TextFont {
                        font_size: 26.0,
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.9, 0.25)),
                ));

                panel.spawn(Node {
                    height: Val::Px(6.0),
                    ..default()
                });

                spawn_swatch_row(
                    panel,
                    "OUTFIT",
                    SwatchCategory::Outfit,
                    &outfits,
                    design_data.outfit_idx,
                );
                spawn_swatch_row(
                    panel,
                    "ACCENT",
                    SwatchCategory::Accent,
                    &accents,
                    design_data.accent_idx,
                );
                spawn_swatch_row(
                    panel,
                    "HAIR",
                    SwatchCategory::Hair,
                    &hairs,
                    design_data.hair_idx,
                );

                panel.spawn(Node {
                    height: Val::Px(6.0),
                    ..default()
                });

                panel.spawn((
                    Text::new("ACCESSORIES"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.70, 0.72, 0.92)),
                ));
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        row_gap: Val::Px(8.0),
                        flex_wrap: FlexWrap::Wrap,
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_toggle(row, "HOOD", AccessoryToggle::Hood, design_data.has_hood);
                        spawn_toggle(row, "CAPE", AccessoryToggle::Cape, design_data.has_cape);
                        spawn_toggle(
                            row,
                            "GLOVES",
                            AccessoryToggle::Gloves,
                            design_data.has_gloves,
                        );
                        spawn_toggle(row, "BOOTS", AccessoryToggle::Boots, design_data.has_boots);
                        spawn_toggle(
                            row,
                            "PADS",
                            AccessoryToggle::ShoulderPads,
                            design_data.has_shoulder_pads,
                        );
                        spawn_toggle(row, "VISOR", AccessoryToggle::Visor, design_data.has_visor);
                    });

                panel.spawn(Node {
                    height: Val::Px(6.0),
                    ..default()
                });

                panel.spawn((
                    Text::new("BODY SHAPE"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.70, 0.72, 0.92)),
                ));
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(8.0),
                        row_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|grid| {
                        for (label, field) in [
                            ("HEIGHT", BodyField::Height),
                            ("SHOULDERS", BodyField::Shoulders),
                            ("CHEST", BodyField::Chest),
                            ("ARMS", BodyField::Arms),
                            ("LEGS", BodyField::Legs),
                            ("HANDS", BodyField::Hands),
                            ("FEET", BodyField::Feet),
                            ("HEAD", BodyField::Head),
                            ("MASS", BodyField::Mass),
                        ] {
                            spawn_body_row(grid, label, field, &design_data.body);
                        }
                    });

                // Expand to push buttons to bottom
                panel.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });

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
                            Node {
                                padding: UiRect::all(Val::Px(14.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.35, 0.08, 0.08)),
                            BackButton,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("← BACK"),
                                TextFont {
                                    font_size: 18.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });

                        row.spawn((
                            Button,
                            Node {
                                padding: UiRect::all(Val::Px(14.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.08, 0.38, 0.12)),
                            ConfirmButton,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("CONFIRM"),
                                TextFont {
                                    font_size: 18.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                    });

                panel.spawn((
                    Text::new("Click swatches to customize  |  ESC: back  |  Enter: confirm"),
                    TextFont {
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.5, 0.5, 0.7, 0.8)),
                ));
            });
        });
}

fn spawn_swatch_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    category: SwatchCategory,
    colors: &[Color; 8],
    selected: usize,
) {
    parent.spawn((
        Text::new(label),
        TextFont {
            font_size: 13.0,
            ..default()
        },
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
                    BorderColor::all(if is_sel {
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
    parent: &mut ChildSpawnerCommands,
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
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn spawn_body_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    field: BodyField,
    body: &BodyRecipe,
) {
    parent
        .spawn(Node {
            width: Val::Px(178.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(5.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.62, 0.68, 0.86)),
                Node {
                    width: Val::Px(70.0),
                    ..default()
                },
            ));
            spawn_step_button(row, field, -0.04, "-");
            row.spawn((
                Text::new(format!("{:.2}", body_field_value(body, field))),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    width: Val::Px(38.0),
                    ..default()
                },
                BodyValueText(field),
            ));
            spawn_step_button(row, field, 0.04, "+");
        });
}

fn spawn_step_button(parent: &mut ChildSpawnerCommands, field: BodyField, delta: f32, label: &str) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(26.0),
                height: Val::Px(24.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.14, 0.24)),
            BodyStepper { field, delta },
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn adjust_body_field(body: &mut BodyRecipe, field: BodyField, delta: f32) {
    match field {
        BodyField::Height => body.height += delta,
        BodyField::Shoulders => body.shoulder_width += delta,
        BodyField::Chest => body.chest_size += delta,
        BodyField::Arms => body.arm_length += delta,
        BodyField::Legs => body.leg_length += delta,
        BodyField::Hands => body.hand_size += delta,
        BodyField::Feet => body.foot_size += delta,
        BodyField::Head => body.head_size += delta,
        BodyField::Mass => body.mass += delta,
    }
}

fn body_field_value(body: &BodyRecipe, field: BodyField) -> f32 {
    match field {
        BodyField::Height => body.height,
        BodyField::Shoulders => body.shoulder_width,
        BodyField::Chest => body.chest_size,
        BodyField::Arms => body.arm_length,
        BodyField::Legs => body.leg_length,
        BodyField::Hands => body.hand_size,
        BodyField::Feet => body.foot_size,
        BodyField::Head => body.head_size,
        BodyField::Mass => body.mass,
    }
}
