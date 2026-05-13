//! Chassis editor — visual customization for the player's robot.
//!
//! Currently a minimal headless config screen: keys 1/2 cycle primary color,
//! 3/4 cycle scale. The chosen `RobotStyle` is stored in `PlayerChassis` and
//! applied to the player on next chapter start. ESC returns to chapter select.

use bevy::prelude::*;

use crate::resources::PlayerChassis;
use crate::robots::presets::{amp, atlas, theta, valor, volt};
use crate::state::AppState;

pub struct ChassisEditorPlugin;

impl Plugin for ChassisEditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::ChassisEditor), setup_editor)
            .add_systems(OnExit(AppState::ChassisEditor), teardown_editor)
            .add_systems(
                Update,
                editor_input.run_if(in_state(AppState::ChassisEditor)),
            );
    }
}

#[derive(Component)]
struct ChassisEditorRoot;
#[derive(Component)]
struct ChassisStatusText;

fn setup_editor(mut commands: Commands, chassis: Res<PlayerChassis>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.06, 1.0)),
            ChassisEditorRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("CHASSIS EDITOR"),
                TextFont {
                    font_size: 48.0,
                    ..default()
                },
                TextColor(Color::srgb(0.4, 0.85, 1.0)),
            ));
            p.spawn((
                Text::new("[1] Amp  [2] Atlas  [3] Volt  [4] Valor  [5] Theta"),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            p.spawn((
                Text::new("[+/-] Scale     [Esc] Back"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.9)),
            ));
            p.spawn((
                Text::new(format!("Current: scale={:.2}", chassis.0.scale)),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.9, 0.5)),
                ChassisStatusText,
            ));
        });
}

fn teardown_editor(mut commands: Commands, q: Query<Entity, With<ChassisEditorRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn_recursive();
    }
}

fn editor_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut chassis: ResMut<PlayerChassis>,
    mut next_state: ResMut<NextState<AppState>>,
    mut text_q: Query<&mut Text, With<ChassisStatusText>>,
) {
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
    if keyboard.just_pressed(KeyCode::Equal) || keyboard.just_pressed(KeyCode::NumpadAdd) {
        chassis.0.scale = (chassis.0.scale + 0.1).min(2.0);
    }
    if keyboard.just_pressed(KeyCode::Minus) || keyboard.just_pressed(KeyCode::NumpadSubtract) {
        chassis.0.scale = (chassis.0.scale - 0.1).max(0.5);
    }
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::ChapterSelect);
    }
    if let Ok(mut text) = text_q.get_single_mut() {
        text.0 = format!("Current: scale={:.2}", chassis.0.scale);
    }
}
