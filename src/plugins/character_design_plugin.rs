use bevy::prelude::*;

use crate::character::face::{BrowStyle, Expression, EyeStyle, FaceRecipe};

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};

use crate::character::blueprint::{
    BodyRecipe, CartoonAppearanceRecipe, CharacterBlueprint, CharacterPaletteRecipe,
    CHARACTER_BLUEPRINT_SCHEMA_VERSION,
};
use crate::character::parts::{
    ArmPreset, BodyPreset, CharacterLoadout, HeadPreset, LegPreset, ShoulderPreset,
};
use crate::character::player_mesh::spawn_modular_player_preview;
use crate::character::presets::{
    accent_preset, accent_presets, eye_preset, eye_presets, hair_preset, hair_presets, hero_config,
    hero_config_with_overrides, normalize_color_preset_index, outfit_preset, outfit_presets,
    skin_preset, skin_presets,
};
use crate::components::player::PlayerCamera;
use crate::engine::state::AppState;
use crate::plugins::ui_plugin::MenuScrollPanel;
use crate::resources::{
    is_stale_reference_blueprint, reference_appearance_recipe, reference_body_recipe,
    CharacterBaseModel, CharacterDesignData, CharacterDesignReturnTarget, CharacterDesignSnapshot,
    PlayerPartLoadout, PlayerSelectState,
};
use std::fs;
use std::path::PathBuf;

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
                    design_preset_interaction,
                    swatch_interaction,
                    accessory_interaction,
                    part_preset_interaction,
                    physique_preset_interaction,
                    body_stepper_interaction,
                    // Grouped: Bevy caps a system tuple at 20 entries.
                    (
                        face_stepper_interaction,
                        face_cycle_interaction,
                        update_face_ui_text,
                    ),
                    button_interaction,
                    human_studio_interaction,
                    prefab_action_interaction,
                    preview_zoom_interaction,
                    scroll_design_panel,
                    // Grouped: Bevy caps a system tuple at 20 entries.
                    (
                        update_swatch_borders,
                        update_base_model_status_text,
                        update_design_preset_colors,
                        update_toggle_colors,
                        update_part_preset_text,
                        update_physique_preset_colors,
                        update_body_value_text,
                    ),
                )
                    .run_if(in_state(AppState::CharacterDesign)),
            );
    }
}

// ── Marker components ─────────────────────────────────────────────────────────

#[derive(Component)]
struct DesignUiRoot;

#[derive(Component)]
struct DesignScrollPanel;

#[derive(Component)]
struct DesignLight;

#[derive(Component)]
struct PreviewRoot;

#[derive(Component)]
struct BackButton;

#[derive(Component)]
struct ConfirmButton;

#[derive(Component)]
struct HumanStudioButton;

#[derive(Component, Clone, Copy)]
struct PreviewZoomButton(f32);

#[derive(Component, Clone, Copy)]
struct SwatchButton {
    category: SwatchCategory,
    index: usize,
}

#[derive(Component, Clone, Copy)]
struct DesignPresetButton(ReferenceDesignPreset);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ReferenceDesignPreset {
    Amp,
    Antonio,
    Chroma,
    Daria,
    Vincenzo,
}

#[derive(Clone, Copy, PartialEq)]
enum SwatchCategory {
    Skin,
    Outfit,
    Accent,
    Hair,
    Eye,
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

#[derive(Component)]
struct FaceStepper {
    field: FaceField,
    delta: f32,
}

#[derive(Component)]
struct FaceValueText(FaceField);

#[derive(Component)]
struct FaceCycleButton(FaceCycle);

#[derive(Component)]
struct FaceCycleText(FaceCycle);

#[derive(Component, Clone, Copy)]
struct PartPresetButton {
    slot: DesignerPartSlot,
}

#[derive(Component, Clone, Copy)]
struct PartPresetText(DesignerPartSlot);

#[derive(Component)]
struct BaseModelStatusText;

#[derive(Component, Clone, Copy)]
struct PrefabActionButton(PrefabAction);

#[derive(Clone, Copy, PartialEq, Eq)]
enum PrefabAction {
    Export,
    Import,
}

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

/// Scalar face sliders. Enum-valued face choices (eye style, brow style,
/// expression) cycle through `FaceCycle` instead.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum FaceField {
    EyeSize,
    EyeSpacing,
    EyeHeight,
    EyeTilt,
    Pupil,
    Highlight,
    Lash,
    BrowHeight,
    Nose,
    MouthWidth,
}

/// Cycles a face choice forward through its list.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum FaceCycle {
    EyeStyle,
    BrowStyle,
    Expression,
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

fn design_preview_camera_transform(distance: f32) -> Transform {
    Transform::from_xyz(0.0, 0.5, -distance).looking_at(Vec3::new(0.0, -0.1, 0.0), Vec3::Y)
}

fn setup_character_design(
    mut commands: Commands,
    mut design_data: ResMut<CharacterDesignData>,
    select_state: Res<PlayerSelectState>,
    part_loadout: Res<PlayerPartLoadout>,
    mut cam_q: Query<&mut Transform, (With<Camera3d>, Without<PlayerCamera>)>,
) {
    design_data.player_index = design_data.player_index.min(select_state.slots.len() - 1);

    // Pre-populate design data from current slot overrides (or hero defaults)
    let slot = &select_state.slots[design_data.player_index];
    let hero_name = select_state.character_name(design_data.player_index);
    let base = hero_config(hero_name);
    let blueprint = slot
        .blueprint
        .as_ref()
        .filter(|blueprint| !is_stale_reference_blueprint(hero_name, blueprint));
    design_data.skin_idx =
        palette_index_from_saved(slot.skin_idx, blueprint, "skin", base.skin, &skin_presets());
    design_data.outfit_idx = palette_index_from_saved(
        slot.outfit_idx,
        blueprint,
        "outfit",
        base.outfit,
        &outfit_presets(),
    );
    design_data.accent_idx = palette_index_from_saved(
        slot.accent_idx,
        blueprint,
        "accent",
        base.accent,
        &accent_presets(),
    );
    design_data.hair_idx =
        palette_index_from_saved(slot.hair_idx, blueprint, "hair", base.hair, &hair_presets());
    design_data.eye_idx = palette_index_from_saved(
        slot.eye_idx,
        blueprint,
        "eye",
        base.eye_color,
        &eye_presets(),
    );
    // Seed the face from the saved blueprint if there is one, else from the
    // hero's authored face — entering the designer must never flatten a
    // character back to the default.
    design_data.face = blueprint
        .filter(|bp| bp.schema_version >= CHARACTER_BLUEPRINT_SCHEMA_VERSION)
        .map(|bp| bp.face)
        .unwrap_or(base.face)
        .sanitized();
    let slot_loadout =
        PlayerPartLoadout::resolve_for_hero(hero_name, slot.part_loadout, *part_loadout);
    design_data.body_preset = slot_loadout.body;
    design_data.arm_preset = slot_loadout.arms;
    design_data.leg_preset = slot_loadout.legs;
    design_data.shoulder_preset = slot_loadout.shoulders;
    design_data.head_preset = slot_loadout.head;
    design_data.base_model = if slot.part_loadout.is_some() || blueprint.is_some() {
        CharacterBaseModel::from_name_and_loadout(hero_name, slot_loadout)
    } else {
        CharacterBaseModel::from_name(hero_name)
    };
    design_data.body = blueprint
        .as_ref()
        .map(|blueprint| blueprint.body)
        .unwrap_or_else(|| reference_body_recipe(hero_name))
        .validated();
    let reference_appearance = reference_appearance_recipe(hero_name);
    let preserve_slot_appearance = slot
        .part_loadout
        .is_some_and(|loadout| !loadout.is_stale_native_default())
        || blueprint.is_some();
    let blueprint_appearance = blueprint.map(|blueprint| blueprint.cartoon_appearance);
    if preserve_slot_appearance {
        design_data.has_hood = slot
            .has_hood
            .or(blueprint_appearance.map(|appearance| appearance.has_hood))
            .unwrap_or(reference_appearance.has_hood);
        design_data.has_cape = slot
            .has_cape
            .or(blueprint_appearance.map(|appearance| appearance.has_cape))
            .unwrap_or(reference_appearance.has_cape);
        design_data.has_gloves = slot
            .has_gloves
            .or(blueprint_appearance.map(|appearance| appearance.has_gloves))
            .unwrap_or(reference_appearance.has_gloves);
        design_data.has_boots = slot
            .has_boots
            .or(blueprint_appearance.map(|appearance| appearance.has_boots))
            .unwrap_or(reference_appearance.has_boots);
        design_data.has_shoulder_pads = slot
            .has_shoulder_pads
            .or(blueprint_appearance.map(|appearance| appearance.has_shoulder_pads))
            .unwrap_or(reference_appearance.has_shoulder_pads);
        design_data.has_visor = slot
            .has_visor
            .or(blueprint_appearance.map(|appearance| appearance.has_visor))
            .unwrap_or(reference_appearance.has_visor);
    } else {
        design_data.has_hood = reference_appearance.has_hood;
        design_data.has_cape = reference_appearance.has_cape;
        design_data.has_gloves = reference_appearance.has_gloves;
        design_data.has_boots = reference_appearance.has_boots;
        design_data.has_shoulder_pads = reference_appearance.has_shoulder_pads;
        design_data.has_visor = reference_appearance.has_visor;
    }
    design_data.spin_angle = 0.0;
    design_data.preview_distance = 3.2;
    design_data.dirty = true;
    design_data.preview_entity = None;

    // Reposition the menu camera for a character preview angle
    for mut t in cam_q.iter_mut() {
        *t = design_preview_camera_transform(design_data.preview_distance);
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
    design_data.skin_idx = normalize_color_preset_index(design_data.skin_idx);
    design_data.outfit_idx = normalize_color_preset_index(design_data.outfit_idx);
    design_data.accent_idx = normalize_color_preset_index(design_data.accent_idx);
    design_data.hair_idx = normalize_color_preset_index(design_data.hair_idx);
    design_data.eye_idx = normalize_color_preset_index(design_data.eye_idx);

    let name = select_state.character_name(design_data.player_index);
    let mut config = hero_config_with_overrides(
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
    config.skin = skin_preset(design_data.skin_idx);
    config.eye_color = eye_preset(design_data.eye_idx);
    // The designer edits the anime face directly; sanitize so a slider can
    // never hand the mesh builder out-of-range geometry.
    config.face = design_data.face.sanitized();
    config.emissive_eyes = config.emissive_eyes || design_data.has_visor;

    let loadout = CharacterLoadout {
        body: design_data.body_preset,
        arms: design_data.arm_preset,
        legs: design_data.leg_preset,
        shoulders: design_data.shoulder_preset,
        head: design_data.head_preset,
    };
    let entity = spawn_modular_player_preview(
        &mut commands,
        &mut meshes,
        &mut materials,
        config,
        Vec3::ZERO,
        loadout,
    );
    commands.entity(entity).insert(PreviewRoot);
    design_data.preview_entity = Some(entity);
    design_data.dirty = false;
}

// ── Swatch clicks ─────────────────────────────────────────────────────────────

fn design_preset_interaction(
    interaction_q: Query<(&Interaction, &DesignPresetButton), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for (interaction, preset) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        preset.0.apply(&mut design_data);
        design_data.dirty = true;
    }
}

fn swatch_interaction(
    interaction_q: Query<(&Interaction, &SwatchButton), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for (interaction, swatch) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            match swatch.category {
                SwatchCategory::Skin => design_data.skin_idx = swatch.index,
                SwatchCategory::Outfit => design_data.outfit_idx = swatch.index,
                SwatchCategory::Accent => design_data.accent_idx = swatch.index,
                SwatchCategory::Hair => design_data.hair_idx = swatch.index,
                SwatchCategory::Eye => design_data.eye_idx = swatch.index,
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

fn face_stepper_interaction(
    interaction_q: Query<(&Interaction, &FaceStepper), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for (interaction, stepper) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let mut face = design_data.face;
        apply_face_field(&mut face, stepper.field, stepper.delta);
        design_data.face = face;
        design_data.dirty = true;
    }
}

fn face_cycle_interaction(
    interaction_q: Query<(&Interaction, &FaceCycleButton), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let mut face = design_data.face;
        cycle_face_choice(&mut face, button.0);
        design_data.face = face;
        design_data.dirty = true;
    }
}

fn update_face_ui_text(
    design_data: Res<CharacterDesignData>,
    mut value_q: Query<(&FaceValueText, &mut Text), Without<FaceCycleText>>,
    mut cycle_q: Query<(&FaceCycleText, &mut Text), Without<FaceValueText>>,
) {
    if !design_data.is_changed() {
        return;
    }
    for (marker, mut text) in value_q.iter_mut() {
        *text = Text::new(format!(
            "{:.2}",
            face_field_value(&design_data.face, marker.0)
        ));
    }
    for (marker, mut text) in cycle_q.iter_mut() {
        *text = Text::new(face_choice_label(&design_data.face, marker.0).to_uppercase());
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
            next_state.set(character_design_return_state(design_data.return_target));
            return;
        }
    }
    for interaction in confirm_q.iter() {
        if *interaction == Interaction::Pressed {
            save_design(&design_data, &mut select_state, &mut part_loadout);
            next_state.set(character_design_return_state(design_data.return_target));
            return;
        }
    }
}

// ── Button-only window actions ────────────────────────────────────────────────

fn character_design_return_state(target: CharacterDesignReturnTarget) -> AppState {
    match target {
        CharacterDesignReturnTarget::PlayerSelect => AppState::PlayerSelect,
        CharacterDesignReturnTarget::ChapterSelect => AppState::ChapterSelect,
    }
}

fn prefab_action_interaction(
    interaction_q: Query<(&Interaction, &PrefabActionButton), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
    select_state: Res<PlayerSelectState>,
) {
    for (interaction, action) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        run_prefab_action(action.0, &mut design_data, &select_state);
    }
}

fn human_studio_interaction(
    interaction_q: Query<&Interaction, (Changed<Interaction>, With<HumanStudioButton>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::CharacterStudio);
            return;
        }
    }
}

fn preview_zoom_interaction(
    interaction_q: Query<(&Interaction, &PreviewZoomButton), Changed<Interaction>>,
    mut design_data: ResMut<CharacterDesignData>,
    mut cam_q: Query<&mut Transform, (With<Camera3d>, Without<PlayerCamera>)>,
) {
    for (interaction, zoom) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        design_data.preview_distance = (design_data.preview_distance + zoom.0).clamp(1.8, 6.0);
        for mut transform in cam_q.iter_mut() {
            *transform = design_preview_camera_transform(design_data.preview_distance);
        }
    }
}

fn scroll_design_panel(
    mut wheel: MessageReader<MouseWheel>,
    mut panel_q: Query<(&mut ScrollPosition, &ComputedNode), With<DesignScrollPanel>>,
) {
    let mut delta_y = 0.0;
    for event in wheel.read() {
        let scale = match event.unit {
            MouseScrollUnit::Line => 26.0,
            MouseScrollUnit::Pixel => 1.0,
        };
        delta_y -= event.y * scale;
    }

    if delta_y == 0.0 {
        return;
    }

    for (mut scroll_position, computed) in panel_q.iter_mut() {
        let max_y = ((computed.content_size().y - computed.size().y)
            * computed.inverse_scale_factor())
        .max(0.0);
        scroll_position.y = (scroll_position.y + delta_y).clamp(0.0, max_y);
    }
}

fn run_prefab_action(
    action: PrefabAction,
    design_data: &mut CharacterDesignData,
    select_state: &PlayerSelectState,
) {
    design_data.player_index = design_data.player_index.min(select_state.slots.len() - 1);
    let hero_name = select_state.character_name(design_data.player_index);
    let path = character_design_prefab_path(hero_name);

    match action {
        PrefabAction::Export => export_design_prefab(design_data, &path),
        PrefabAction::Import => import_design_prefab(design_data, &path),
    }
}

fn export_design_prefab(design_data: &CharacterDesignData, path: &PathBuf) {
    let snapshot = CharacterDesignSnapshot::from_design_data(design_data);
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            warn!(
                "could not create character design prefab folder {:?}: {err}",
                parent
            );
            return;
        }
    }
    match serde_json::to_string_pretty(&snapshot)
        .map_err(|err| err.to_string())
        .and_then(|json| fs::write(path, json).map_err(|err| err.to_string()))
    {
        Ok(()) => info!("exported character design prefab to {:?}", path),
        Err(err) => warn!("could not export character design prefab {:?}: {err}", path),
    }
}

fn import_design_prefab(design_data: &mut CharacterDesignData, path: &PathBuf) {
    match fs::read_to_string(path)
        .map_err(|err| err.to_string())
        .and_then(|json| {
            serde_json::from_str::<CharacterDesignSnapshot>(&json).map_err(|err| err.to_string())
        }) {
        Ok(mut snapshot) => {
            snapshot.player_index = design_data.player_index;
            snapshot.apply_to_design_data(design_data);
            info!("imported character design prefab from {:?}", path);
        }
        Err(err) => warn!("could not import character design prefab {:?}: {err}", path),
    }
}

fn character_design_prefab_path(hero_name: &str) -> PathBuf {
    let mut file_stem = String::with_capacity(hero_name.len());
    for c in hero_name.chars() {
        if c.is_ascii_alphanumeric() {
            file_stem.push(c.to_ascii_lowercase());
        } else if !file_stem.ends_with('_') {
            file_stem.push('_');
        }
    }
    if file_stem.is_empty() {
        file_stem.push_str("character");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("Characters")
        .join("design_prefabs")
        .join(format!("{file_stem}.json"))
}

fn save_design(
    design_data: &CharacterDesignData,
    select_state: &mut PlayerSelectState,
    part_loadout: &mut PlayerPartLoadout,
) {
    let name = select_state.character_name(design_data.player_index);
    let mut body = design_data.body;
    body.validate();
    let blueprint = CharacterBlueprint::hero(
        name,
        body,
        CharacterPaletteRecipe {
            skin: skin_preset(design_data.skin_idx),
            outfit: outfit_preset(design_data.outfit_idx),
            accent: accent_preset(design_data.accent_idx),
            hair: hair_preset(design_data.hair_idx),
            eye: eye_preset(design_data.eye_idx),
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
    // `CharacterBlueprint::hero` has no face parameter, so the authored face
    // is stamped on after construction — without this every designer edit to
    // the face would be discarded on confirm.
    let blueprint = CharacterBlueprint {
        face: design_data.face.sanitized(),
        ..blueprint
    };

    let Some(slot) = select_state.slots.get_mut(design_data.player_index) else {
        return;
    };
    slot.skin_idx = Some(normalize_color_preset_index(design_data.skin_idx));
    slot.outfit_idx = Some(normalize_color_preset_index(design_data.outfit_idx));
    slot.accent_idx = Some(normalize_color_preset_index(design_data.accent_idx));
    slot.hair_idx = Some(normalize_color_preset_index(design_data.hair_idx));
    slot.eye_idx = Some(normalize_color_preset_index(design_data.eye_idx));
    slot.has_hood = Some(design_data.has_hood);
    slot.has_cape = Some(design_data.has_cape);
    slot.has_gloves = Some(design_data.has_gloves);
    slot.has_boots = Some(design_data.has_boots);
    slot.has_shoulder_pads = Some(design_data.has_shoulder_pads);
    slot.has_visor = Some(design_data.has_visor);
    slot.blueprint = Some(blueprint);
    // Applying the basic designer intentionally replaces any previously
    // selected Advanced Studio runtime recipe for this slot.
    slot.studio_spec = None;

    let loadout = current_design_loadout(design_data);
    slot.part_loadout = Some(loadout);
    *part_loadout = loadout;
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
            SwatchCategory::Skin => design_data.skin_idx == swatch.index,
            SwatchCategory::Outfit => design_data.outfit_idx == swatch.index,
            SwatchCategory::Accent => design_data.accent_idx == swatch.index,
            SwatchCategory::Hair => design_data.hair_idx == swatch.index,
            SwatchCategory::Eye => design_data.eye_idx == swatch.index,
        };
        node.border = UiRect::all(Val::Px(if selected { 3.0 } else { 1.0 }));
        *border = BorderColor::all(if selected {
            Color::srgb(0.94, 0.76, 0.36)
        } else {
            Color::srgba(0.08, 0.08, 0.09, 0.82)
        });
    }
}

fn update_base_model_status_text(
    design_data: Res<CharacterDesignData>,
    mut status_q: Query<&mut Text, With<BaseModelStatusText>>,
) {
    if !design_data.is_changed() {
        return;
    }
    for mut text in status_q.iter_mut() {
        *text = Text::new(base_model_status_text(&design_data));
    }
}

fn update_design_preset_colors(
    design_data: Res<CharacterDesignData>,
    mut button_q: Query<(&DesignPresetButton, &mut BackgroundColor, &mut BorderColor)>,
) {
    if !design_data.is_changed() {
        return;
    }
    for (button, mut bg, mut border) in button_q.iter_mut() {
        let selected = button.0.matches(&design_data);
        *bg = BackgroundColor(if selected {
            Color::srgb(0.38, 0.30, 0.18)
        } else {
            Color::srgb(0.090, 0.094, 0.104)
        });
        *border = BorderColor::all(if selected {
            Color::srgb(0.94, 0.72, 0.36)
        } else {
            Color::srgba(0.28, 0.27, 0.30, 0.84)
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

fn palette_index_from_saved(
    slot_index: Option<usize>,
    blueprint: Option<&CharacterBlueprint>,
    material_id: &str,
    default_color: Color,
    presets: &[Color; 8],
) -> usize {
    slot_index
        .map(normalize_color_preset_index)
        .unwrap_or_else(|| {
            let color = blueprint
                .and_then(|blueprint| blueprint.material_color(material_id))
                .unwrap_or(default_color);
            closest_palette_index(color, presets)
        })
}

fn closest_palette_index(color: Color, presets: &[Color; 8]) -> usize {
    let target = color.to_srgba();
    presets
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let a = a.to_srgba();
            let b = b.to_srgba();
            let da = (target.red - a.red).powi(2)
                + (target.green - a.green).powi(2)
                + (target.blue - a.blue).powi(2);
            let db = (target.red - b.red).powi(2)
                + (target.green - b.green).powi(2)
                + (target.blue - b.blue).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

// ── UI construction ───────────────────────────────────────────────────────────

fn spawn_design_ui(
    commands: &mut Commands,
    select_state: &PlayerSelectState,
    design_data: &CharacterDesignData,
) {
    let hero_name = select_state.character_name(design_data.player_index);
    let skins = skin_presets();
    let outfits = outfit_presets();
    let accents = accent_presets();
    let hairs = hair_presets();
    let eyes = eye_presets();

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
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.026, 0.030, 0.96)),
                ScrollPosition::default(),
                DesignScrollPanel,
                MenuScrollPanel,
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(format!("ROBOT BUILDER\n{}", hero_name.to_uppercase())),
                    TextFont {
                        font_size: FontSize::Px(25.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.92, 0.76, 0.42)),
                ));
                panel.spawn((
                    Text::new(base_model_status_text(design_data)),
                    TextFont {
                        font_size: FontSize::Px(12.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.72, 0.78, 0.84)),
                    BaseModelStatusText,
                ));

                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(8.0),
                        row_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_human_studio_button(row);
                        spawn_preview_zoom_button(row, "+", -0.25);
                        spawn_preview_zoom_button(row, "-", 0.25);
                    });

                spawn_section_label(panel, "BASE MODEL");
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
                            ReferenceDesignPreset::Amp,
                            ReferenceDesignPreset::Antonio,
                            ReferenceDesignPreset::Chroma,
                            ReferenceDesignPreset::Daria,
                            ReferenceDesignPreset::Vincenzo,
                        ] {
                            spawn_design_preset_button(row, preset, design_data);
                        }
                    });

                spawn_section_label(panel, "PREFABS");
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(8.0),
                        row_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|row| {
                        spawn_prefab_button(row, "EXPORT", PrefabAction::Export);
                        spawn_prefab_button(row, "IMPORT", PrefabAction::Import);
                    });

                spawn_section_label(panel, "PALETTE");

                spawn_swatch_row(
                    panel,
                    "SKIN",
                    SwatchCategory::Skin,
                    &skins,
                    design_data.skin_idx,
                );
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
                spawn_swatch_row(
                    panel,
                    "EYES",
                    SwatchCategory::Eye,
                    &eyes,
                    design_data.eye_idx,
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

                // ── Face ──────────────────────────────────────────────
                spawn_section_label(panel, "FACE");
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(7.0),
                        row_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|grid| {
                        for (label, cycle) in [
                            ("EYES", FaceCycle::EyeStyle),
                            ("BROWS", FaceCycle::BrowStyle),
                            ("MOOD", FaceCycle::Expression),
                        ] {
                            spawn_face_cycle_row(grid, label, cycle, &design_data.face);
                        }
                        for (label, field) in [
                            ("EYE SIZE", FaceField::EyeSize),
                            ("SPACING", FaceField::EyeSpacing),
                            ("EYE Y", FaceField::EyeHeight),
                            ("TILT", FaceField::EyeTilt),
                            ("PUPIL", FaceField::Pupil),
                            ("SPARKLE", FaceField::Highlight),
                            ("LASHES", FaceField::Lash),
                            ("BROW Y", FaceField::BrowHeight),
                            ("NOSE", FaceField::Nose),
                            ("MOUTH", FaceField::MouthWidth),
                        ] {
                            spawn_face_row(grid, label, field, &design_data.face);
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
                                width: Val::Px(112.0),
                                height: Val::Px(44.0),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
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
                                    font_size: FontSize::Px(16.0),
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });

                        row.spawn((
                            Button,
                            Node {
                                width: Val::Px(112.0),
                                height: Val::Px(44.0),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
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
                                    font_size: FontSize::Px(16.0),
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
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(Color::srgb(0.58, 0.61, 0.66)),
    ));
}

fn spawn_design_preset_button(
    parent: &mut ChildSpawnerCommands,
    preset: ReferenceDesignPreset,
    design_data: &CharacterDesignData,
) {
    let selected = preset.matches(design_data);
    parent
        .spawn((
            Button,
            Node {
                min_width: Val::Px(116.0),
                justify_content: JustifyContent::Center,
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if selected {
                Color::srgb(0.38, 0.30, 0.18)
            } else {
                Color::srgb(0.090, 0.094, 0.104)
            }),
            BorderColor::all(if selected {
                Color::srgb(0.94, 0.72, 0.36)
            } else {
                Color::srgba(0.28, 0.27, 0.30, 0.84)
            }),
            DesignPresetButton(preset),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(preset.label()),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.91, 0.86)),
            ));
        });
}

fn spawn_prefab_button(parent: &mut ChildSpawnerCommands, label: &str, action: PrefabAction) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(100.0),
                height: Val::Px(36.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.12, 0.15)),
            BorderColor::all(Color::srgba(0.25, 0.46, 0.58, 0.86)),
            PrefabActionButton(action),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.78, 0.90, 0.96)),
            ));
        });
}

fn spawn_human_studio_button(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(160.0),
                height: Val::Px(42.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.16, 0.13, 0.24)),
            BorderColor::all(Color::srgba(0.62, 0.58, 1.0, 0.92)),
            HumanStudioButton,
        ))
        .with_children(|button| {
            button.spawn((
                Text::new("HUMAN STUDIO"),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::srgb(0.90, 0.86, 1.0)),
            ));
        });
}

fn spawn_preview_zoom_button(parent: &mut ChildSpawnerCommands, label: &str, delta: f32) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(44.0),
                height: Val::Px(36.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.09, 0.105, 0.12)),
            BorderColor::all(Color::srgba(0.26, 0.38, 0.42, 0.86)),
            PreviewZoomButton(delta),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(Color::srgb(0.78, 0.90, 0.96)),
            ));
        });
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
                    font_size: FontSize::Px(10.0),
                    ..default()
                },
                TextColor(Color::srgb(0.50, 0.54, 0.60)),
            ));
            button.spawn((
                Text::new(part_slot_value_label(design_data, slot)),
                TextFont {
                    font_size: FontSize::Px(14.0),
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
                    font_size: FontSize::Px(13.0),
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
            font_size: FontSize::Px(11.0),
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
                    font_size: FontSize::Px(13.0),
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
                    font_size: FontSize::Px(10.0),
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
                    font_size: FontSize::Px(12.0),
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

/// One face slider row: label, minus, live value, plus. Mirrors
/// `spawn_body_row` so the two sections read identically.
fn spawn_face_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    field: FaceField,
    face: &FaceRecipe,
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
                    font_size: FontSize::Px(10.0),
                    ..default()
                },
                TextColor(Color::srgb(0.56, 0.58, 0.64)),
                Node {
                    width: Val::Px(54.0),
                    ..default()
                },
            ));
            spawn_face_step_button(row, field, -0.04, "-");
            row.spawn((
                Text::new(format!("{:.2}", face_field_value(face, field))),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.90, 0.86, 0.76)),
                Node {
                    width: Val::Px(34.0),
                    ..default()
                },
                FaceValueText(field),
            ));
            spawn_face_step_button(row, field, 0.04, "+");
        });
}

fn spawn_face_step_button(
    parent: &mut ChildSpawnerCommands,
    field: FaceField,
    delta: f32,
    label: &str,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(34.0),
                height: Val::Px(34.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.10, 0.105, 0.12)),
            BorderColor::all(Color::srgba(0.36, 0.32, 0.24, 0.84)),
            FaceStepper { field, delta },
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(0.88, 0.84, 0.74)),
            ));
        });
}

/// A wide button that cycles one enum-valued face choice and shows its
/// current name.
fn spawn_face_cycle_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    cycle: FaceCycle,
    face: &FaceRecipe,
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
                    font_size: FontSize::Px(10.0),
                    ..default()
                },
                TextColor(Color::srgb(0.56, 0.58, 0.64)),
                Node {
                    width: Val::Px(54.0),
                    ..default()
                },
            ));
            row.spawn((
                Button,
                Node {
                    width: Val::Px(84.0),
                    height: Val::Px(34.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.10, 0.105, 0.12)),
                BorderColor::all(Color::srgba(0.36, 0.32, 0.24, 0.84)),
                FaceCycleButton(cycle),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new(face_choice_label(face, cycle).to_uppercase()),
                    TextFont {
                        font_size: FontSize::Px(11.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.90, 0.86, 0.76)),
                    FaceCycleText(cycle),
                ));
            });
        });
}

fn spawn_step_button(parent: &mut ChildSpawnerCommands, field: BodyField, delta: f32, label: &str) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(34.0),
                height: Val::Px(34.0),
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
                    font_size: FontSize::Px(16.0),
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

/// Step a face slider. Ranges are enforced by `FaceRecipe::sanitized`, so a
/// held button can never drive geometry out of range.
fn apply_face_field(face: &mut FaceRecipe, field: FaceField, delta: f32) {
    match field {
        FaceField::EyeSize => face.eye_size += delta,
        FaceField::EyeSpacing => face.eye_spacing += delta,
        // Positional knobs are far smaller in scale than the multipliers.
        FaceField::EyeHeight => face.eye_height += delta * 0.05,
        FaceField::EyeTilt => face.eye_tilt += delta * 0.35,
        FaceField::Pupil => face.pupil_scale += delta,
        FaceField::Highlight => face.highlight_scale += delta,
        FaceField::Lash => face.lash_thickness += delta,
        FaceField::BrowHeight => face.brow_height += delta * 0.04,
        FaceField::Nose => face.nose_scale += delta,
        FaceField::MouthWidth => face.mouth_width += delta,
    }
    *face = face.sanitized();
}

fn face_field_value(face: &FaceRecipe, field: FaceField) -> f32 {
    match field {
        FaceField::EyeSize => face.eye_size,
        FaceField::EyeSpacing => face.eye_spacing,
        FaceField::EyeHeight => face.eye_height,
        FaceField::EyeTilt => face.eye_tilt,
        FaceField::Pupil => face.pupil_scale,
        FaceField::Highlight => face.highlight_scale,
        FaceField::Lash => face.lash_thickness,
        FaceField::BrowHeight => face.brow_height,
        FaceField::Nose => face.nose_scale,
        FaceField::MouthWidth => face.mouth_width,
    }
}

/// Advance one of the enum-valued face choices, wrapping at the end.
fn cycle_face_choice(face: &mut FaceRecipe, cycle: FaceCycle) {
    fn next<T: PartialEq + Copy>(all: &[T], current: T) -> T {
        let index = all.iter().position(|value| *value == current).unwrap_or(0);
        all[(index + 1) % all.len()]
    }
    match cycle {
        FaceCycle::EyeStyle => face.eye_style = next(&EyeStyle::ALL, face.eye_style),
        FaceCycle::BrowStyle => face.brow_style = next(&BrowStyle::ALL, face.brow_style),
        FaceCycle::Expression => face.expression = next(&Expression::ALL, face.expression),
    }
}

/// Current label for a face choice, for the cycle button's caption.
fn face_choice_label(face: &FaceRecipe, cycle: FaceCycle) -> &'static str {
    match cycle {
        FaceCycle::EyeStyle => face.eye_style.label(),
        FaceCycle::BrowStyle => face.brow_style.label(),
        FaceCycle::Expression => face.expression.label(),
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

fn current_design_loadout(design_data: &CharacterDesignData) -> PlayerPartLoadout {
    PlayerPartLoadout {
        body: design_data.body_preset,
        arms: design_data.arm_preset,
        legs: design_data.leg_preset,
        shoulders: design_data.shoulder_preset,
        head: design_data.head_preset,
    }
}

fn base_model_status_text(design_data: &CharacterDesignData) -> String {
    let edited_state = if base_model_has_custom_edits(design_data) {
        "custom edit"
    } else {
        "reference"
    };
    format!(
        "Model: {} ({})\nParts: {} / {} / {} / {} / {}",
        design_data.base_model.label(),
        edited_state,
        design_data.body_preset.label(),
        design_data.arm_preset.label(),
        design_data.leg_preset.label(),
        design_data.shoulder_preset.label(),
        design_data.head_preset.label()
    )
}

fn base_model_has_custom_edits(design_data: &CharacterDesignData) -> bool {
    current_design_loadout(design_data) != design_data.base_model.loadout()
        || !body_recipe_matches(&design_data.body, design_data.base_model.body_recipe())
        || reference_appearance_recipe(design_data.base_model.hero_hint())
            != CartoonAppearanceRecipe {
                has_hood: design_data.has_hood,
                has_cape: design_data.has_cape,
                has_gloves: design_data.has_gloves,
                has_boots: design_data.has_boots,
                has_shoulder_pads: design_data.has_shoulder_pads,
                has_visor: design_data.has_visor,
            }
}

impl ReferenceDesignPreset {
    fn label(self) -> &'static str {
        self.base_model().label()
    }

    fn base_model(self) -> CharacterBaseModel {
        match self {
            ReferenceDesignPreset::Amp => CharacterBaseModel::AmpSiege,
            ReferenceDesignPreset::Antonio => CharacterBaseModel::AntonioRift,
            ReferenceDesignPreset::Chroma => CharacterBaseModel::ChromaTrace,
            ReferenceDesignPreset::Daria => CharacterBaseModel::DariaFlares,
            ReferenceDesignPreset::Vincenzo => CharacterBaseModel::VincenzoDeep,
        }
    }

    fn apply(self, design_data: &mut CharacterDesignData) {
        match self {
            ReferenceDesignPreset::Amp => {
                design_data.base_model = CharacterBaseModel::AmpSiege;
                design_data.skin_idx = 3;
                design_data.outfit_idx = 0;
                design_data.accent_idx = 1;
                design_data.hair_idx = 6;
                design_data.eye_idx = 5;
                design_data.body_preset = BodyPreset::HeavyPlate;
                design_data.arm_preset = ArmPreset::HeavyArms;
                design_data.leg_preset = LegPreset::HeavyLegs;
                design_data.shoulder_preset = ShoulderPreset::PlateEpaulettes;
                design_data.head_preset = HeadPreset::FullHelm;
                design_data.body = Self::amp_body();
                design_data.has_hood = false;
                design_data.has_cape = false;
                design_data.has_gloves = true;
                design_data.has_boots = true;
                design_data.has_shoulder_pads = true;
                design_data.has_visor = true;
            }
            ReferenceDesignPreset::Antonio => {
                design_data.base_model = CharacterBaseModel::AntonioRift;
                design_data.skin_idx = 4;
                design_data.outfit_idx = 7;
                design_data.accent_idx = 3;
                design_data.hair_idx = 6;
                design_data.eye_idx = 5;
                design_data.body_preset = BodyPreset::RiftMantle;
                design_data.arm_preset = ArmPreset::RiftTalons;
                design_data.leg_preset = LegPreset::RiftBoots;
                design_data.shoulder_preset = ShoulderPreset::RiftCloak;
                design_data.head_preset = HeadPreset::RiftCowl;
                design_data.body = Self::antonio_body();
                design_data.has_hood = true;
                design_data.has_cape = true;
                design_data.has_gloves = true;
                design_data.has_boots = true;
                design_data.has_shoulder_pads = true;
                design_data.has_visor = true;
            }
            ReferenceDesignPreset::Chroma => {
                design_data.base_model = CharacterBaseModel::ChromaTrace;
                design_data.skin_idx = 2;
                design_data.outfit_idx = 6;
                design_data.accent_idx = 6;
                design_data.hair_idx = 7;
                design_data.eye_idx = 5;
                design_data.body_preset = BodyPreset::ChromaFrame;
                design_data.arm_preset = ArmPreset::ChromaBlades;
                design_data.leg_preset = LegPreset::ChromaStriders;
                design_data.shoulder_preset = ShoulderPreset::ChromaMantle;
                design_data.head_preset = HeadPreset::ChromaCrown;
                design_data.body = Self::chroma_body();
                design_data.has_hood = false;
                design_data.has_cape = false;
                design_data.has_gloves = true;
                design_data.has_boots = true;
                design_data.has_shoulder_pads = true;
                design_data.has_visor = true;
            }
            ReferenceDesignPreset::Daria => {
                design_data.base_model = CharacterBaseModel::DariaFlares;
                design_data.skin_idx = 1;
                design_data.outfit_idx = 5;
                design_data.accent_idx = 4;
                design_data.hair_idx = 5;
                design_data.eye_idx = 6;
                design_data.body_preset = BodyPreset::DariaCore;
                design_data.arm_preset = ArmPreset::DariaCannon;
                design_data.leg_preset = LegPreset::DariaGreaves;
                design_data.shoulder_preset = ShoulderPreset::DariaFlares;
                design_data.head_preset = HeadPreset::DariaHelm;
                design_data.body = Self::daria_body();
                design_data.has_hood = false;
                design_data.has_cape = false;
                design_data.has_gloves = true;
                design_data.has_boots = true;
                design_data.has_shoulder_pads = true;
                design_data.has_visor = true;
            }
            ReferenceDesignPreset::Vincenzo => {
                design_data.base_model = CharacterBaseModel::VincenzoDeep;
                design_data.skin_idx = 0;
                design_data.outfit_idx = 6;
                design_data.accent_idx = 6;
                design_data.hair_idx = 7;
                design_data.eye_idx = 5;
                design_data.body_preset = BodyPreset::ChromaFrame;
                design_data.arm_preset = ArmPreset::ChromaBlades;
                design_data.leg_preset = LegPreset::ChromaStriders;
                design_data.shoulder_preset = ShoulderPreset::ChromaMantle;
                design_data.head_preset = HeadPreset::ChromaCrown;
                design_data.body = Self::vincenzo_body();
                design_data.has_hood = false;
                design_data.has_cape = false;
                design_data.has_gloves = true;
                design_data.has_boots = true;
                design_data.has_shoulder_pads = true;
                design_data.has_visor = true;
            }
        }
        design_data.body.validate();
    }

    fn matches(self, design_data: &CharacterDesignData) -> bool {
        design_data.base_model == self.base_model()
    }

    fn amp_body() -> BodyRecipe {
        reference_body_recipe("AMP")
    }

    fn antonio_body() -> BodyRecipe {
        reference_body_recipe("Antonio")
    }

    fn chroma_body() -> BodyRecipe {
        reference_body_recipe("Chroma")
    }

    fn daria_body() -> BodyRecipe {
        reference_body_recipe("Daria")
    }

    fn vincenzo_body() -> BodyRecipe {
        reference_body_recipe("Vincenzo")
    }
}

fn body_recipe_matches(body: &BodyRecipe, target: BodyRecipe) -> bool {
    let body = body.validated();
    (body.height - target.height).abs() < 0.01
        && (body.shoulder_width - target.shoulder_width).abs() < 0.01
        && (body.chest_size - target.chest_size).abs() < 0.01
        && (body.arm_length - target.arm_length).abs() < 0.01
        && (body.leg_length - target.leg_length).abs() < 0.01
        && (body.hand_size - target.hand_size).abs() < 0.01
        && (body.foot_size - target.foot_size).abs() < 0.01
        && (body.head_size - target.head_size).abs() < 0.01
        && (body.neck_length - target.neck_length).abs() < 0.01
        && (body.torso_curve - target.torso_curve).abs() < 0.01
        && (body.hip_width - target.hip_width).abs() < 0.01
        && (body.spine_posture - target.spine_posture).abs() < 0.01
        && (body.mass - target.mass).abs() < 0.01
        && (body.muscle - target.muscle).abs() < 0.01
        && (body.body_fat - target.body_fat).abs() < 0.01
        && (body.asymmetry - target.asymmetry).abs() < 0.01
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_model_selection_survives_custom_edits() {
        let mut design_data = CharacterDesignData::default();
        ReferenceDesignPreset::Daria.apply(&mut design_data);
        design_data.body.height += 0.12;
        design_data.leg_preset = LegPreset::RiftBoots;

        assert!(ReferenceDesignPreset::Daria.matches(&design_data));
        assert!(base_model_has_custom_edits(&design_data));
    }

    #[test]
    fn base_model_status_identifies_reference_and_custom_states() {
        let mut design_data = CharacterDesignData::default();
        ReferenceDesignPreset::Antonio.apply(&mut design_data);
        assert!(base_model_status_text(&design_data).contains("reference"));

        design_data.head_preset = HeadPreset::DariaHelm;
        let status = base_model_status_text(&design_data);
        assert!(status.contains("Antonio Rift"));
        assert!(status.contains("custom edit"));
        assert!(status.contains("Daria Helm"));
    }
}
