use bevy::input::gamepad::{GamepadButton, GamepadButtonStateChangedEvent};
use bevy::input::ButtonState;
use bevy::prelude::*;

use crate::character_blueprint::{
    BodyRecipe, CartoonAppearanceRecipe, CharacterBlueprint, CharacterPaletteRecipe,
};
use crate::character_parts::CharacterLoadout;
use crate::characters::{
    accent_preset, accent_presets, hair_preset, hair_presets, hero_config,
    hero_config_with_overrides, normalize_color_preset_index, outfit_preset, outfit_presets,
    spawn_cartoon_character,
};
use crate::plugins::input_plugin::{NativeButton, NativeControllerState};
use crate::resources::{CharacterDesignData, PlayerPartLoadout, PlayerSelectState};
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
                    part_preset_interaction,
                    physique_preset_interaction,
                    body_stepper_interaction,
                    button_interaction,
                    design_keyboard_input,
                    update_swatch_borders,
                    update_toggle_colors,
                    update_part_preset_text,
                    update_physique_preset_colors,
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

#[derive(Component, Clone, Copy)]
struct PartPresetButton {
    slot: DesignerPartSlot,
}

#[derive(Component, Clone, Copy)]
struct PartPresetText(DesignerPartSlot);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum DesignerPartSlot {
    Body,
    Arms,
    Legs,
    Shoulders,
    Head,
}

#[derive(Component, Clone, Copy)]
struct PhysiquePresetButton(PhysiquePreset);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum PhysiquePreset {
    Vanguard,
    Duelist,
    Arcanist,
    Lancer,
    Titan,
}

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
    Neck,
    Hips,
    Posture,
    TorsoCurve,
    Mass,
    Muscle,
    BodyFat,
    Asymmetry,
}

// ── Setup ─────────────────────────────────────────────────────────────────────

fn setup_character_design(
    mut commands: Commands,
    mut design_data: ResMut<CharacterDesignData>,
    select_state: Res<PlayerSelectState>,
    part_loadout: Res<PlayerPartLoadout>,
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
    design_data.body_preset = part_loadout.body;
    design_data.arm_preset = part_loadout.arms;
    design_data.leg_preset = part_loadout.legs;
    design_data.shoulder_preset = part_loadout.shoulders;
    design_data.head_preset = part_loadout.head;
    design_data.body = slot
        .blueprint
        .as_ref()
        .map(|blueprint| blueprint.body)
        .unwrap_or_default()
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
    commands.entity(entity).insert((
        PreviewRoot,
        CharacterLoadout {
            body: design_data.body_preset,
            arms: design_data.arm_preset,
            legs: design_data.leg_preset,
            shoulders: design_data.shoulder_preset,
            head: design_data.head_preset,
        },
    ));
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

fn part_preset_interaction(
    interaction_q: Query<(&Interaction, &PartPresetButton), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button.slot {
            DesignerPartSlot::Body => design_data.body_preset = design_data.body_preset.cycle(),
            DesignerPartSlot::Arms => design_data.arm_preset = design_data.arm_preset.cycle(),
            DesignerPartSlot::Legs => design_data.leg_preset = design_data.leg_preset.cycle(),
            DesignerPartSlot::Shoulders => {
                design_data.shoulder_preset = design_data.shoulder_preset.cycle()
            }
            DesignerPartSlot::Head => design_data.head_preset = design_data.head_preset.cycle(),
        }
        design_data.dirty = true;
    }
}

fn physique_preset_interaction(
    interaction_q: Query<(&Interaction, &PhysiquePresetButton), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for (interaction, preset) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        design_data.body = preset.0.body_recipe();
        design_data.body.validate();
        design_data.dirty = true;
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
    mut part_loadout: ResMut<PlayerPartLoadout>,
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
            save_design(&design_data, &mut select_state, &mut part_loadout);
            next_state.set(AppState::PlayerSelect);
            return;
        }
    }
}

// ── Keyboard / gamepad input ──────────────────────────────────────────────────

fn design_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    design_data: Res<CharacterDesignData>,
    mut select_state: ResMut<PlayerSelectState>,
    mut part_loadout: ResMut<PlayerPartLoadout>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let mut go_back = keys.just_pressed(KeyCode::Escape);
    let mut go_confirm =
        keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);

    let mut event_back = false;
    let mut event_confirm = false;
    for event in button_events.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        match event.button {
            GamepadButton::East => event_back = true,
            GamepadButton::South | GamepadButton::Start => event_confirm = true,
            _ => {}
        }
    }

    for gp in gamepads.iter() {
        if gp.just_pressed(GamepadButton::East) {
            go_back = true;
        }
        if gp.just_pressed(GamepadButton::South) || gp.just_pressed(GamepadButton::Start) {
            go_confirm = true;
        }
    }
    go_back = go_back || event_back || native.just_pressed(NativeButton::East);
    go_confirm = go_confirm
        || event_confirm
        || native.just_pressed(NativeButton::South)
        || native.just_pressed(NativeButton::Start);

    if go_confirm {
        save_design(&design_data, &mut select_state, &mut part_loadout);
        next_state.set(AppState::PlayerSelect);
    } else if go_back {
        next_state.set(AppState::PlayerSelect);
    }
}

fn save_design(
    design_data: &CharacterDesignData,
    select_state: &mut PlayerSelectState,
    part_loadout: &mut PlayerPartLoadout,
) {
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

    part_loadout.body = design_data.body_preset;
    part_loadout.arms = design_data.arm_preset;
    part_loadout.legs = design_data.leg_preset;
    part_loadout.shoulders = design_data.shoulder_preset;
    part_loadout.head = design_data.head_preset;
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
            Color::srgb(0.94, 0.76, 0.36)
        } else {
            Color::srgba(0.08, 0.08, 0.09, 0.82)
        });
    }
}

fn update_toggle_colors(
    design_data: Res<CharacterDesignData>,
    mut toggle_q: Query<(&AccessoryToggle, &mut BackgroundColor, &mut BorderColor)>,
) {
    if !design_data.is_changed() {
        return;
    }
    for (toggle, mut bg, mut border) in toggle_q.iter_mut() {
        let active = match *toggle {
            AccessoryToggle::Hood => design_data.has_hood,
            AccessoryToggle::Cape => design_data.has_cape,
            AccessoryToggle::Gloves => design_data.has_gloves,
            AccessoryToggle::Boots => design_data.has_boots,
            AccessoryToggle::ShoulderPads => design_data.has_shoulder_pads,
            AccessoryToggle::Visor => design_data.has_visor,
        };
        *bg = BackgroundColor(if active {
            Color::srgb(0.30, 0.24, 0.13)
        } else {
            Color::srgb(0.090, 0.094, 0.104)
        });
        *border = BorderColor::all(if active {
            Color::srgb(0.76, 0.58, 0.30)
        } else {
            Color::srgba(0.24, 0.25, 0.29, 0.82)
        });
    }
}

fn update_part_preset_text(
    design_data: Res<CharacterDesignData>,
    mut label_q: Query<(&PartPresetText, &mut Text)>,
) {
    if !design_data.is_changed() {
        return;
    }
    for (label, mut text) in label_q.iter_mut() {
        *text = Text::new(part_slot_value_label(&design_data, label.0));
    }
}

fn update_physique_preset_colors(
    design_data: Res<CharacterDesignData>,
    mut button_q: Query<(
        &PhysiquePresetButton,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
) {
    if !design_data.is_changed() {
        return;
    }
    for (button, mut bg, mut border) in button_q.iter_mut() {
        let selected = physique_matches(&design_data.body, button.0);
        *bg = BackgroundColor(if selected {
            Color::srgb(0.42, 0.30, 0.16)
        } else {
            Color::srgb(0.095, 0.105, 0.125)
        });
        *border = BorderColor::all(if selected {
            Color::srgb(0.92, 0.70, 0.36)
        } else {
            Color::srgba(0.24, 0.25, 0.29, 0.82)
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
            root.spawn(Node {
                width: Val::Percent(54.0),
                height: Val::Percent(100.0),
                ..default()
            });

            root.spawn((
                Node {
                    width: Val::Percent(46.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(Val::Px(28.0), Val::Px(24.0)),
                    row_gap: Val::Px(10.0),
                    overflow: Overflow::clip_y(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.026, 0.030, 0.96)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(format!("HERO ATELIER\n{}", hero_name.to_uppercase())),
                    TextFont {
                        font_size: 25.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.92, 0.76, 0.42)),
                ));

                spawn_section_label(panel, "PALETTE");

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

                spawn_section_label(panel, "BATTLE SILHOUETTE");
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(8.0),
                        row_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|grid| {
                        for (label, slot) in [
                            ("BODY", DesignerPartSlot::Body),
                            ("ARMS", DesignerPartSlot::Arms),
                            ("LEGS", DesignerPartSlot::Legs),
                            ("SHOULDERS", DesignerPartSlot::Shoulders),
                            ("HEAD", DesignerPartSlot::Head),
                        ] {
                            spawn_part_preset_row(grid, label, slot, design_data);
                        }
                    });

                spawn_section_label(panel, "GEAR");
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

                spawn_section_label(panel, "PHYSIQUE ARCHETYPE");
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(7.0),
                        row_gap: Val::Px(7.0),
                        ..default()
                    })
                    .with_children(|row| {
                        for preset in [
                            PhysiquePreset::Vanguard,
                            PhysiquePreset::Duelist,
                            PhysiquePreset::Arcanist,
                            PhysiquePreset::Lancer,
                            PhysiquePreset::Titan,
                        ] {
                            spawn_physique_preset_button(row, preset, &design_data.body);
                        }
                    });

                spawn_section_label(panel, "PROPORTIONS");
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(7.0),
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
                            ("NECK", BodyField::Neck),
                            ("HIPS", BodyField::Hips),
                            ("POSTURE", BodyField::Posture),
                            ("TORSO", BodyField::TorsoCurve),
                            ("MASS", BodyField::Mass),
                            ("MUSCLE", BodyField::Muscle),
                            ("WEIGHT", BodyField::BodyFat),
                            ("ASYMM", BodyField::Asymmetry),
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
                                padding: UiRect::axes(Val::Px(18.0), Val::Px(13.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.16, 0.08, 0.07)),
                            BorderColor::all(Color::srgb(0.42, 0.25, 0.20)),
                            BackButton,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("BACK"),
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
                                padding: UiRect::axes(Val::Px(18.0), Val::Px(13.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.22, 0.17, 0.08)),
                            BorderColor::all(Color::srgb(0.82, 0.62, 0.32)),
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
            });
        });
}

fn spawn_section_label(parent: &mut ChildSpawnerCommands, label: &str) {
    parent.spawn((
        Text::new(label),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.58, 0.61, 0.66)),
    ));
}

fn spawn_part_preset_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    slot: DesignerPartSlot,
    design_data: &CharacterDesignData,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(178.0),
                min_height: Val::Px(46.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.090, 0.094, 0.104)),
            BorderColor::all(Color::srgba(0.34, 0.30, 0.24, 0.92)),
            PartPresetButton { slot },
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(0.50, 0.54, 0.60)),
            ));
            button.spawn((
                Text::new(part_slot_value_label(design_data, slot)),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.90, 0.77, 0.48)),
                PartPresetText(slot),
            ));
        });
}

fn spawn_physique_preset_button(
    parent: &mut ChildSpawnerCommands,
    preset: PhysiquePreset,
    body: &BodyRecipe,
) {
    let selected = physique_matches(body, preset);
    parent
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(Val::Px(11.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if selected {
                Color::srgb(0.42, 0.30, 0.16)
            } else {
                Color::srgb(0.095, 0.105, 0.125)
            }),
            BorderColor::all(if selected {
                Color::srgb(0.92, 0.70, 0.36)
            } else {
                Color::srgba(0.24, 0.25, 0.29, 0.82)
            }),
            PhysiquePresetButton(preset),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(preset.label()),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.91, 0.86)),
            ));
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
            font_size: 11.0,
            ..default()
        },
        TextColor(Color::srgb(0.52, 0.55, 0.60)),
    ));
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(6.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|row| {
            for (i, &color) in colors.iter().enumerate() {
                let is_sel = i == selected;
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(34.0),
                        height: Val::Px(34.0),
                        border: UiRect::all(Val::Px(if is_sel { 3.0 } else { 1.0 })),
                        ..default()
                    },
                    BackgroundColor(color),
                    BorderColor::all(if is_sel {
                        Color::srgb(0.94, 0.76, 0.36)
                    } else {
                        Color::srgba(0.08, 0.08, 0.09, 0.82)
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
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if active {
                Color::srgb(0.30, 0.24, 0.13)
            } else {
                Color::srgb(0.090, 0.094, 0.104)
            }),
            BorderColor::all(if active {
                Color::srgb(0.76, 0.58, 0.30)
            } else {
                Color::srgba(0.24, 0.25, 0.29, 0.82)
            }),
            toggle,
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.91, 0.86)),
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
            width: Val::Px(142.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(0.56, 0.58, 0.64)),
                Node {
                    width: Val::Px(54.0),
                    ..default()
                },
            ));
            spawn_step_button(row, field, -0.04, "-");
            row.spawn((
                Text::new(format!("{:.2}", body_field_value(body, field))),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.90, 0.86, 0.76)),
                Node {
                    width: Val::Px(34.0),
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
                width: Val::Px(24.0),
                height: Val::Px(22.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.10, 0.105, 0.12)),
            BorderColor::all(Color::srgba(0.36, 0.32, 0.24, 0.84)),
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
        BodyField::Neck => body.neck_length += delta,
        BodyField::Hips => body.hip_width += delta,
        BodyField::Posture => body.spine_posture += delta,
        BodyField::TorsoCurve => body.torso_curve += delta,
        BodyField::Mass => body.mass += delta,
        BodyField::Muscle => body.muscle += delta,
        BodyField::BodyFat => body.body_fat += delta,
        BodyField::Asymmetry => body.asymmetry += delta,
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
        BodyField::Neck => body.neck_length,
        BodyField::Hips => body.hip_width,
        BodyField::Posture => body.spine_posture,
        BodyField::TorsoCurve => body.torso_curve,
        BodyField::Mass => body.mass,
        BodyField::Muscle => body.muscle,
        BodyField::BodyFat => body.body_fat,
        BodyField::Asymmetry => body.asymmetry,
    }
}

fn part_slot_value_label(design_data: &CharacterDesignData, slot: DesignerPartSlot) -> String {
    let label = match slot {
        DesignerPartSlot::Body => design_data.body_preset.label(),
        DesignerPartSlot::Arms => design_data.arm_preset.label(),
        DesignerPartSlot::Legs => design_data.leg_preset.label(),
        DesignerPartSlot::Shoulders => design_data.shoulder_preset.label(),
        DesignerPartSlot::Head => design_data.head_preset.label(),
    };
    label.to_string()
}

impl PhysiquePreset {
    fn label(self) -> &'static str {
        match self {
            PhysiquePreset::Vanguard => "Vanguard",
            PhysiquePreset::Duelist => "Duelist",
            PhysiquePreset::Arcanist => "Arcanist",
            PhysiquePreset::Lancer => "Lancer",
            PhysiquePreset::Titan => "Titan",
        }
    }

    fn body_recipe(self) -> BodyRecipe {
        match self {
            PhysiquePreset::Vanguard => BodyRecipe {
                height: 1.05,
                shoulder_width: 1.12,
                chest_size: 1.10,
                arm_length: 1.16,
                leg_length: 1.18,
                hand_size: 1.06,
                foot_size: 1.10,
                head_size: 0.95,
                neck_length: 1.02,
                torso_curve: 0.04,
                hip_width: 1.02,
                spine_posture: 0.04,
                mass: 1.14,
                muscle: 1.18,
                body_fat: 0.94,
                asymmetry: 0.03,
            },
            PhysiquePreset::Duelist => BodyRecipe {
                height: 1.08,
                shoulder_width: 0.96,
                chest_size: 0.96,
                arm_length: 1.22,
                leg_length: 1.30,
                hand_size: 0.96,
                foot_size: 1.04,
                head_size: 0.91,
                neck_length: 1.04,
                torso_curve: 0.10,
                hip_width: 0.94,
                spine_posture: -0.06,
                mass: 0.88,
                muscle: 1.04,
                body_fat: 0.84,
                asymmetry: 0.05,
            },
            PhysiquePreset::Arcanist => BodyRecipe {
                height: 0.98,
                shoulder_width: 0.88,
                chest_size: 0.90,
                arm_length: 1.10,
                leg_length: 1.12,
                hand_size: 1.08,
                foot_size: 0.92,
                head_size: 1.06,
                neck_length: 1.10,
                torso_curve: -0.05,
                hip_width: 0.96,
                spine_posture: -0.12,
                mass: 0.82,
                muscle: 0.88,
                body_fat: 0.92,
                asymmetry: 0.04,
            },
            PhysiquePreset::Lancer => BodyRecipe {
                height: 1.16,
                shoulder_width: 1.02,
                chest_size: 1.02,
                arm_length: 1.24,
                leg_length: 1.38,
                hand_size: 1.02,
                foot_size: 1.12,
                head_size: 0.88,
                neck_length: 1.08,
                torso_curve: 0.02,
                hip_width: 0.98,
                spine_posture: -0.02,
                mass: 1.00,
                muscle: 1.08,
                body_fat: 0.86,
                asymmetry: 0.02,
            },
            PhysiquePreset::Titan => BodyRecipe {
                height: 1.18,
                shoulder_width: 1.30,
                chest_size: 1.26,
                arm_length: 1.26,
                leg_length: 1.26,
                hand_size: 1.22,
                foot_size: 1.28,
                head_size: 0.93,
                neck_length: 0.94,
                torso_curve: 0.02,
                hip_width: 1.20,
                spine_posture: 0.10,
                mass: 1.38,
                muscle: 1.36,
                body_fat: 1.02,
                asymmetry: 0.02,
            },
        }
        .validated()
    }
}

fn physique_matches(body: &BodyRecipe, preset: PhysiquePreset) -> bool {
    let target = preset.body_recipe();
    (body.height - target.height).abs() < 0.01
        && (body.shoulder_width - target.shoulder_width).abs() < 0.01
        && (body.chest_size - target.chest_size).abs() < 0.01
        && (body.arm_length - target.arm_length).abs() < 0.01
        && (body.leg_length - target.leg_length).abs() < 0.01
        && (body.mass - target.mass).abs() < 0.01
        && (body.muscle - target.muscle).abs() < 0.01
}
