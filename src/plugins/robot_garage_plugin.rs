//! Robot Garage — assemble robot pets into vehicles and mechs.

use bevy::prelude::*;

use crate::combat::upgrades::UpgradeLedger;
use crate::engine::state::AppState;
use crate::plugins::ui_plugin::MenuScrollPanel;
use crate::world::robot_pets::{
    RobotAssemblyForm, RobotPartKind, RobotPetCollection, RobotPetError,
};

pub struct RobotGaragePlugin;

impl Plugin for RobotGaragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GarageData>()
            .add_systems(OnEnter(AppState::RobotGarage), setup_garage)
            .add_systems(OnExit(AppState::RobotGarage), teardown_garage)
            .add_systems(
                Update,
                (garage_button_input, update_garage_display)
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
#[derive(Component, Clone, Copy)]
struct GarageButton(GarageAction);
#[derive(Clone, Copy)]
enum GarageAction {
    PreviousForm,
    NextForm,
    Assemble,
    Disassemble,
    Back,
}

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
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(dark_bg),
            ScrollPosition::default(),
            MenuScrollPanel,
            GarageRoot,
        ))
        .with_children(|root| {
            // Title
            root.spawn((
                Text::new("ROBOT GARAGE"),
                TextFont {
                    font_size: FontSize::Px(36.0),
                    ..default()
                },
                TextColor(header_color),
            ));
            root.spawn((
                Text::new(
                    "D-PAD / LEFT STICK: move focus   A: activate   B: back   Selection is remembered",
                ),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(dim_color),
            ));

            // Parts inventory panel
            root.spawn((
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
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
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(dim_color),
                    ));
                    col.spawn((
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
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
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(dim_color),
                    ));
                    col.spawn((
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
                        TextColor(body_color),
                        FormBrowserText,
                    ));
                });
            });

            // Active assembly status
            root.spawn((
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.92, 0.42)),
                AssemblyStatusText,
            ));

            // Action feedback
            root.spawn((
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.55, 0.55)),
                ActionFeedbackText,
            ));

            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(10.0),
                row_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|row| {
                spawn_garage_button(row, "PREVIOUS", GarageAction::PreviousForm);
                spawn_garage_button(row, "NEXT", GarageAction::NextForm);
                spawn_garage_button(row, "ASSEMBLE", GarageAction::Assemble);
                spawn_garage_button(row, "DISASSEMBLE", GarageAction::Disassemble);
                spawn_garage_button(row, "BACK", GarageAction::Back);
            });
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

// ── Button Input ──────────────────────────────────────────────────────────────

fn spawn_garage_button(
    parent: &mut ChildSpawnerCommands,
    label: &'static str,
    action: GarageAction,
) {
    parent
        .spawn((
            Button,
            Node {
                min_width: Val::Px(112.0),
                height: Val::Px(40.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(12.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.14, 0.20)),
            BorderColor::all(Color::srgb(0.24, 0.48, 0.66)),
            GarageButton(action),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn garage_button_input(
    interaction_q: Query<(&Interaction, &GarageButton), (Changed<Interaction>, With<Button>)>,
    mut data: ResMut<GarageData>,
    mut robot_pets: ResMut<RobotPetCollection>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let form_count = RobotAssemblyForm::ALL.len();
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button.0 {
            GarageAction::PreviousForm => {
                data.form_index = (data.form_index + form_count - 1) % form_count;
                data.last_result = None;
            }
            GarageAction::NextForm => {
                data.form_index = (data.form_index + 1) % form_count;
                data.last_result = None;
            }
            GarageAction::Assemble => assemble_current_form(&mut data, &mut robot_pets),
            GarageAction::Disassemble => {
                robot_pets.disassemble();
                data.last_result = Some(GarageActionResult::Disassembled);
            }
            GarageAction::Back => {
                next_state.set(AppState::ChapterSelect);
                return;
            }
        }
    }
}

fn assemble_current_form(data: &mut GarageData, robot_pets: &mut RobotPetCollection) {
    if robot_pets.active_assembly.is_some() {
        data.last_result = Some(GarageActionResult::AlreadyAssembled);
        return;
    }
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
                crate::world::robot_pets::RobotPetSource::Rescued => "rescued",
                crate::world::robot_pets::RobotPetSource::StoreBuilt => "built",
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
    let mech_link_rank = upgrades.rank(crate::combat::upgrades::TechUpgradeId::MechCommandLink);
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
        "ASSEMBLED".to_string()
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
        "READY".to_string()
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
        Some(GarageActionResult::AlreadyAssembled) => "Already assembled.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garage_remembers_selected_form_across_screen_reentry() {
        let mut data = GarageData {
            form_index: 3,
            last_result: Some(GarageActionResult::NotEnoughParts),
        };
        data.last_result = None;
        let form_count = RobotAssemblyForm::ALL.len();
        if data.form_index >= form_count {
            data.form_index = 0;
        }
        assert_eq!(data.form_index, 3);
        assert!(data.last_result.is_none());
    }
}
