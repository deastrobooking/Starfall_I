//! Controller-focusable mode picker using the installed runtime catalog.
use super::*;
use crate::heavy_water::modes::{HeavyWaterModes, SwitchPlayMode};

#[derive(Component)]
struct ModeButton(String);

#[derive(Component)]
struct ModeLoadingRoot;

pub(super) fn install(app: &mut App) {
    app.add_systems(
        Update,
        mode_buttons.in_set(MenuUiSet::ActionConsumers).run_if(in_state(AppState::Paused)),
    )
    .add_systems(
        OnEnter(AppState::SwitchingMode),
        (cleanup_play_ui_for_menu, show_loading).chain(),
    )
    .add_systems(OnExit(AppState::SwitchingMode), hide_loading);
}

pub(super) fn spawn_picker(
    root: &mut ChildSpawnerCommands,
    modes: &HeavyWaterModes,
    experience: PlayExperience,
) {
    root.spawn((
        Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center,
            row_gap: Val::Px(10.0), ..default() },
        Visibility::Hidden,
        PausePagePanel(PausePage::Modes),
    )).with_children(|page| {
        page.spawn((Text::new("CHANGE PLAY MODE"), TextFont { font_size: FontSize::Px(28.0), ..default() }));
        page.spawn((Text::new("Keep your party and progression. Start a fresh scene."),
            TextFont { font_size: FontSize::Px(16.0), ..default() }));
        for mode in modes.0.iter().filter(|mode| mode.adapter != experience) {
            page.spawn((Button, ModeButton(mode.id.to_string()), Node {
                min_width: Val::Px(300.0), height: Val::Px(44.0),
                align_items: AlignItems::Center, justify_content: JustifyContent::Center,
                ..default()
            }, BackgroundColor(Color::srgb(0.18, 0.32, 0.55))))
                .with_children(|button| {
                    button.spawn((Text::new(&mode.label), TextFont { font_size: FontSize::Px(18.0), ..default() }));
                });
        }
        spawn_pause_button(page, "BACK", PauseAction::Back, Color::srgb(0.0, 0.42, 0.74));
    });
}

fn mode_buttons(
    buttons: Query<(&Interaction, &ModeButton), Changed<Interaction>>,
    mut requests: MessageWriter<SwitchPlayMode>,
) {
    for (interaction, button) in &buttons {
        if *interaction == Interaction::Pressed {
            requests.write(SwitchPlayMode(button.0.clone()));
        }
    }
}

fn show_loading(mut commands: Commands) {
    commands.spawn((ModeLoadingRoot, Node {
        width: Val::Percent(100.0), height: Val::Percent(100.0),
        align_items: AlignItems::Center, justify_content: JustifyContent::Center,
        ..default()
    }, BackgroundColor(Color::srgb(0.02, 0.03, 0.08))))
        .with_children(|root| {
            root.spawn((Text::new("LOADING PLAY MODE…"), TextFont { font_size: FontSize::Px(28.0), ..default() }));
        });
}

fn hide_loading(mut commands: Commands, roots: Query<Entity, With<ModeLoadingRoot>>) {
    for entity in &roots { commands.entity(entity).despawn(); }
}
