//! Robot Garage — assemble robot pets into vehicles and mechs.
//!
//! Accessible from the chapter-select screen via [G].
//! Keys A/D browse assembly forms; Enter assembles; X disassembles; Esc returns.

use bevy::input::gamepad::{GamepadButton, GamepadButtonStateChangedEvent};
use bevy::input::ButtonState;
use bevy::prelude::*;

use crate::plugins::input_plugin::{NativeButton, NativeControllerState};
use crate::robot_pets::{RobotAssemblyForm, RobotPartKind, RobotPetCollection, RobotPetError};
use crate::state::AppState;
use crate::upgrades::UpgradeLedger;

pub struct RobotGaragePlugin;

impl Plugin for RobotGaragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GarageData>()
            .add_systems(OnEnter(AppState::RobotGarage), setup_garage)
            .add_systems(OnExit(AppState::RobotGarage), teardown_garage)
            .add_systems(
                Update,
                (garage_keyboard_input, update_garage_display)
                    .run_if(in_state(AppState::RobotGarage)),
            );
    }
}

// ── State ──────────────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct GarageData {
    form_index: usize,
    last_result: Option<GarageActionResult>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GarageActionResult {
    Assembled,
    Disassembled,
    NotEnoughPets,
    NotEnoughParts,
    AlreadyAssembled,
}

// ── Marker components ──────────────────────────────────────────────────────────

#[derive(Component)]
struct GarageRoot;

#[derive(Component)]
struct PartsInventoryText;

#[derive(Component)]
struct PetRosterText;

#[derive(Component)]
struct FormBrowserText;

#[derive(Component)]
struct AssemblyStatusText;

#[derive(Component)]
struct ActionFeedbackText;

// ── Setup ──────────────────────────────────────────────────────────────────────

fn setup_garage(
    mut commands: Commands,
    mut data: ResMut<GarageData>,
    robot_pets: Res<RobotPetCollection>,
    upgrades: Res<UpgradeLedger>,
) {
    data.last_result = None;

    // Clamp form index in case collection changed.
    let form_count = RobotAssemblyForm::ALL.len();
    if data.form_index >= form_count {
        data.form_index = 0;
    }

    let dark_bg = Color::srgba(0.04, 0.06, 0.10, 0.97);
    let header_color = Color::srgb(0.4, 0.85, 1.0);
    let dim_color = Color::srgb(0.5, 0.60, 0.75);
    let body_color = Color::srgb(0.82, 0.88, 0.96);

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(40.0), Val::Px(32.0)),
                row_gap: Val::Px(18.0),
                ..default()
            },
            BackgroundColor(dark_bg),
            GarageRoot,
        ))
        .with_children(|root| {
            // Title
            root.spawn((
                Text::new("ROBOT GARAGE"),
                TextFont { font_size: 36.0, ..default() },
                TextColor(header_color),
            ));

            // Parts inventory panel
            root.spawn((
                Text::new(""),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::srgb(0.75, 0.90, 0.75)),
                PartsInventoryText,
            ));

            // Two-column row: pets (left) + form browser (right)
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(40.0),
                flex_grow: 1.0,
                ..default()
            })
            .with_children(|row| {
                // Left: pet roster
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    width: Val::Percent(40.0),
                    row_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|col| {
                    col.spawn((
                        Text::new("── YOUR ROBOT PETS ──"),
                        TextFont { font_size: 14.0, ..default() },
                        TextColor(dim_color),
                    ));
                    col.spawn((
                        Text::new(""),
                        TextFont { font_size: 15.0, ..default() },
                        TextColor(body_color),
                        PetRosterText,
                    ));
                });

                // Right: form browser
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    width: Val::Percent(60.0),
                    row_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|col| {
                    col.spawn((
                        Text::new("── ASSEMBLY FORM ──"),
                        TextFont { font_size: 14.0, ..default() },
                        TextColor(dim_color),
                    ));
                    col.spawn((
                        Text::new(""),
                        TextFont { font_size: 15.0, ..default() },
                        TextColor(body_color),
                        FormBrowserText,
                    ));
                });
            });

            // Active assembly status
            root.spawn((
                Text::new(""),
                TextFont { font_size: 16.0, ..default() },
                TextColor(Color::srgb(1.0, 0.92, 0.42)),
                AssemblyStatusText,
            ));

            // Action feedback
            root.spawn((
                Text::new(""),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::srgb(1.0, 0.55, 0.55)),
                ActionFeedbackText,
            ));

            // Controls hint
            root.spawn((
                Text::new(
                    "[A/←][D/→] / D-pad / LB/RB  Browse    [Enter/A]  Assemble    [X]  Disassemble    [Esc/B]  Back",
                ),
                TextFont { font_size: 14.0, ..default() },
                TextColor(dim_color),
            ));
        });

    // Immediately push initial content (avoids one frame blank).
    populate_garage_texts_direct(&robot_pets, &upgrades, &data);
}

// ── Teardown ───────────────────────────────────────────────────────────────────

fn teardown_garage(mut commands: Commands, q: Query<Entity, With<GarageRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
}

// ── Input ──────────────────────────────────────────────────────────────────────

fn garage_keyboard_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    mut data: ResMut<GarageData>,
    mut robot_pets: ResMut<RobotPetCollection>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let form_count = RobotAssemblyForm::ALL.len();
    let mut browse_left =
        keyboard.just_pressed(KeyCode::ArrowLeft) || keyboard.just_pressed(KeyCode::KeyA);
    let mut browse_right =
        keyboard.just_pressed(KeyCode::ArrowRight) || keyboard.just_pressed(KeyCode::KeyD);
    let mut assemble =
        keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);
    let mut disassemble = keyboard.just_pressed(KeyCode::KeyX);
    let mut back = keyboard.just_pressed(KeyCode::Escape) || keyboard.just_pressed(KeyCode::KeyG);

    for event in button_events.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        match event.button {
            GamepadButton::DPadLeft | GamepadButton::LeftTrigger => browse_left = true,
            GamepadButton::DPadRight | GamepadButton::RightTrigger => browse_right = true,
            GamepadButton::South | GamepadButton::Start => assemble = true,
            GamepadButton::West => disassemble = true,
            GamepadButton::East => back = true,
            _ => {}
        }
    }

    for gp in gamepads.iter() {
        browse_left = browse_left
            || gp.just_pressed(GamepadButton::DPadLeft)
            || gp.just_pressed(GamepadButton::LeftTrigger);
        browse_right = browse_right
            || gp.just_pressed(GamepadButton::DPadRight)
            || gp.just_pressed(GamepadButton::RightTrigger);
        assemble = assemble
            || gp.just_pressed(GamepadButton::South)
            || gp.just_pressed(GamepadButton::Start);
        disassemble = disassemble || gp.just_pressed(GamepadButton::West);
        back = back || gp.just_pressed(GamepadButton::East);
    }

    browse_left = browse_left
        || native.just_pressed(NativeButton::DPadLeft)
        || native.just_pressed(NativeButton::LeftShoulder);
    browse_right = browse_right
        || native.just_pressed(NativeButton::DPadRight)
        || native.just_pressed(NativeButton::RightShoulder);
    assemble = assemble
        || native.just_pressed(NativeButton::South)
        || native.just_pressed(NativeButton::Start);
    disassemble = disassemble || native.just_pressed(NativeButton::West);
    back = back || native.just_pressed(NativeButton::East);

    if browse_left {
        data.form_index = (data.form_index + form_count - 1) % form_count;
        data.last_result = None;
    }
    if browse_right {
        data.form_index = (data.form_index + 1) % form_count;
        data.last_result = None;
    }

    if assemble {
        if robot_pets.active_assembly.is_some() {
            data.last_result = Some(GarageActionResult::AlreadyAssembled);
        } else {
            let form = RobotAssemblyForm::ALL[data.form_index];
            let pet_ids: Vec<String> = robot_pets
                .pets
                .iter()
                .filter(|p| p.can_support(form))
                .take(form.required_pets())
                .map(|p| p.id.clone())
                .collect();

            let result = robot_pets.assemble(form, &pet_ids);
            data.last_result = Some(match result {
                Ok(_) => GarageActionResult::Assembled,
                Err(RobotPetError::NotEnoughPets) => GarageActionResult::NotEnoughPets,
                Err(RobotPetError::MissingParts) => GarageActionResult::NotEnoughParts,
                Err(_) => GarageActionResult::NotEnoughPets,
            });
        }
    }

    if disassemble {
        robot_pets.disassemble();
        data.last_result = Some(GarageActionResult::Disassembled);
    }

    if back {
        next_state.set(AppState::ChapterSelect);
    }
}

// ── Display update ─────────────────────────────────────────────────────────────

fn update_garage_display(
    data: Res<GarageData>,
    robot_pets: Res<RobotPetCollection>,
    upgrades: Res<UpgradeLedger>,
    mut parts_q: Query<
        &mut Text,
        (
            With<PartsInventoryText>,
            Without<PetRosterText>,
            Without<FormBrowserText>,
            Without<AssemblyStatusText>,
            Without<ActionFeedbackText>,
        ),
    >,
    mut roster_q: Query<
        &mut Text,
        (
            With<PetRosterText>,
            Without<PartsInventoryText>,
            Without<FormBrowserText>,
            Without<AssemblyStatusText>,
            Without<ActionFeedbackText>,
        ),
    >,
    mut form_q: Query<
        &mut Text,
        (
            With<FormBrowserText>,
            Without<PartsInventoryText>,
            Without<PetRosterText>,
            Without<AssemblyStatusText>,
            Without<ActionFeedbackText>,
        ),
    >,
    mut assembly_q: Query<
        &mut Text,
        (
            With<AssemblyStatusText>,
            Without<PartsInventoryText>,
            Without<PetRosterText>,
            Without<FormBrowserText>,
            Without<ActionFeedbackText>,
        ),
    >,
    mut feedback_q: Query<
        &mut Text,
        (
            With<ActionFeedbackText>,
            Without<PartsInventoryText>,
            Without<PetRosterText>,
            Without<FormBrowserText>,
            Without<AssemblyStatusText>,
        ),
    >,
) {
    if !data.is_changed() && !robot_pets.is_changed() && !upgrades.is_changed() {
        return;
    }

    if let Ok(mut t) = parts_q.single_mut() {
        *t = Text::new(format_parts_inventory(&robot_pets));
    }
    if let Ok(mut t) = roster_q.single_mut() {
        *t = Text::new(format_pet_roster(&robot_pets));
    }
    if let Ok(mut t) = form_q.single_mut() {
        *t = Text::new(format_form_browser(&data, &robot_pets, &upgrades));
    }
    if let Ok(mut t) = assembly_q.single_mut() {
        *t = Text::new(format_assembly_status(&robot_pets));
    }
    if let Ok(mut t) = feedback_q.single_mut() {
        *t = Text::new(format_feedback(data.last_result));
    }
}

// A direct version used in setup before the query pipeline runs.
fn populate_garage_texts_direct(
    _robot_pets: &RobotPetCollection,
    _upgrades: &UpgradeLedger,
    _data: &GarageData,
) {
    // Texts are spawned with empty strings; the update system fills them on
    // the first frame. Nothing to do here.
}

// ── Formatting helpers ─────────────────────────────────────────────────────────

fn format_parts_inventory(robot_pets: &RobotPetCollection) -> String {
    let parts: Vec<String> = RobotPartKind::ALL
        .iter()
        .map(|kind| {
            let n = robot_pets.part_count(*kind);
            let label = kind.label();
            // Trim long labels to keep the line compact
            let short = label.split_whitespace().last().unwrap_or(label);
            format!("{short}: {n}")
        })
        .collect();
    format!("PARTS — {}", parts.join("  ·  "))
}

fn format_pet_roster(robot_pets: &RobotPetCollection) -> String {
    if robot_pets.pets.is_empty() {
        return "No pets yet — rescue them during missions.".to_string();
    }
    robot_pets
        .pets
        .iter()
        .enumerate()
        .map(|(i, pet)| {
            let role = format!("{:?}", pet.role);
            let source = match pet.source {
                crate::robot_pets::RobotPetSource::Rescued => "rescued",
                crate::robot_pets::RobotPetSource::StoreBuilt => "built",
            };
            let forms: Vec<&str> = pet.unlocked_forms.iter().map(|f| f.label()).collect();
            let forms_str = if forms.is_empty() {
                "none".to_string()
            } else {
                forms.join(", ")
            };
            format!(
                "[{}] {:<18}  {:<10} ({})  → {}",
                i + 1,
                pet.name,
                role,
                source,
                forms_str,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_form_browser(
    data: &GarageData,
    robot_pets: &RobotPetCollection,
    upgrades: &UpgradeLedger,
) -> String {
    let form = RobotAssemblyForm::ALL[data.form_index];
    let idx = data.form_index + 1;
    let total = RobotAssemblyForm::ALL.len();
    let recipe = form.recipe();
    let req_pets = form.required_pets();

    // Count eligible pets for this form
    let eligible_pets = robot_pets
        .pets
        .iter()
        .filter(|p| p.can_support(form))
        .count();

    // MechCommandLink gates advanced forms
    let mech_link_rank = upgrades.rank(crate::upgrades::TechUpgradeId::MechCommandLink);
    let advanced = matches!(
        form,
        RobotAssemblyForm::GiantMech | RobotAssemblyForm::SpaceShip | RobotAssemblyForm::MegaShip
    );
    let locked_by_upgrade = advanced && mech_link_rank == 0;

    // Build recipe line
    let recipe_str = recipe
        .iter()
        .map(|c| format!("{}×{}", c.quantity, c.kind.label()))
        .collect::<Vec<_>>()
        .join("  ");

    // Readiness
    let can_afford = robot_pets.can_afford(recipe);
    let enough_pets = eligible_pets >= req_pets;

    let status = if locked_by_upgrade {
        "LOCKED — requires Mech Command Link rank 1".to_string()
    } else if robot_pets.active_assembly.as_ref().map(|a| a.form) == Some(form) {
        "ASSEMBLED — press [X] to disassemble".to_string()
    } else if !enough_pets && !can_afford {
        format!(
            "NEED {} more pets + parts",
            req_pets.saturating_sub(eligible_pets)
        )
    } else if !enough_pets {
        format!(
            "NEED {} more eligible pet(s)  (have {}/{})",
            req_pets.saturating_sub(eligible_pets),
            eligible_pets,
            req_pets,
        )
    } else if !can_afford {
        "NEED MORE PARTS".to_string()
    } else {
        "READY — press [Enter] to assemble".to_string()
    };

    // Browse all forms as a mini-list with current highlighted
    let form_list: Vec<String> = RobotAssemblyForm::ALL
        .iter()
        .enumerate()
        .map(|(i, f)| {
            if i == data.form_index {
                format!("► {}", f.label())
            } else {
                format!("  {}", f.label())
            }
        })
        .collect();

    format!(
        "{form_list_str}\n\n{label}  ({idx}/{total})\nPets needed: {req_pets}  (eligible: {eligible_pets})\nRecipe:  {recipe_str}\n\n{status}",
        form_list_str = form_list.join("  "),
        label = form.label(),
    )
}

fn format_assembly_status(robot_pets: &RobotPetCollection) -> String {
    match &robot_pets.active_assembly {
        None => String::new(),
        Some(a) => {
            let pet_names: Vec<&str> = a
                .pet_ids
                .iter()
                .filter_map(|id| {
                    robot_pets
                        .pets
                        .iter()
                        .find(|p| &p.id == id)
                        .map(|p| p.name.as_str())
                })
                .collect();
            format!(
                "ACTIVE ASSEMBLY: {}  ({})",
                a.form.label(),
                if pet_names.is_empty() {
                    "no pets assigned".to_string()
                } else {
                    pet_names.join(" + ")
                }
            )
        }
    }
}

fn format_feedback(result: Option<GarageActionResult>) -> &'static str {
    match result {
        None => "",
        Some(GarageActionResult::Assembled) => "Assembly complete!",
        Some(GarageActionResult::Disassembled) => "Assembly disbanded.",
        Some(GarageActionResult::NotEnoughPets) => "Not enough eligible pets for this form.",
        Some(GarageActionResult::NotEnoughParts) => "Missing parts — collect more in missions.",
        Some(GarageActionResult::AlreadyAssembled) => {
            "Already assembled — press [X] to disband first."
        }
    }
}
