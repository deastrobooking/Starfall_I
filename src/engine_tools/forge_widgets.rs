//! Shared UI widgets for Forge authoring screens.
//!
//! Every forge (Creature, Weapon, and whatever comes next) builds its panels
//! from the same small vocabulary: action buttons in wrapping rows, section
//! labels, and readout text. Before this module each forge carried its own
//! copy of those spawn helpers with slightly drifting styling; now a new tool
//! gets the house look by construction, and a styling change lands everywhere
//! at once.
//!
//! The widgets are generic over the tool's action component: each forge keeps
//! its own `Component` wrapping its own action enum, so interaction systems
//! stay per-tool and there is no shared action bus to collide on.

use bevy::prelude::*;

/// Visual defaults for forge widgets. One per tool, usually derived from the
/// tool-window accent so buttons match their window chrome.
#[derive(Clone, Copy)]
pub struct ForgeWidgetStyle {
    pub button_background: Color,
    pub button_border: Color,
    pub text: Color,
    pub font_size: f32,
    pub min_width: f32,
    /// 36 px matches the UI accessibility pass: touch/click targets never
    /// drop below comfortable size in any tool.
    pub min_height: f32,
}

impl Default for ForgeWidgetStyle {
    fn default() -> Self {
        Self {
            button_background: Color::srgb(0.12, 0.14, 0.20),
            button_border: Color::srgb(0.34, 0.40, 0.58),
            text: Color::WHITE,
            font_size: 13.0,
            min_width: 104.0,
            min_height: 36.0,
        }
    }
}

/// Spawn one action button. `action` is the tool's own marker component
/// (e.g. `WeaponForgeButton(ForgeAction::Save)`), so each tool's interaction
/// system only ever sees its own buttons.
pub fn action_button(
    parent: &mut ChildSpawnerCommands,
    label: impl Into<String>,
    action: impl Bundle,
    style: &ForgeWidgetStyle,
) {
    parent
        .spawn((
            Button,
            action,
            Node {
                min_width: Val::Px(style.min_width),
                min_height: Val::Px(style.min_height),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(style.button_background),
            BorderColor::all(style.button_border),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label.into()),
                TextFont {
                    font_size: FontSize::Px(style.font_size),
                    ..default()
                },
                TextColor(style.text),
            ));
        });
}

/// A wrapping row of widgets — the standard layout unit of a forge panel.
pub fn widget_row(
    parent: &mut ChildSpawnerCommands,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(6.0),
            margin: UiRect::bottom(Val::Px(5.0)),
            flex_wrap: FlexWrap::Wrap,
            row_gap: Val::Px(5.0),
            ..default()
        })
        .with_children(build);
}

/// A muted section heading inside a panel.
pub fn section_label(parent: &mut ChildSpawnerCommands, label: impl Into<String>) {
    parent.spawn((
        Text::new(label.into()),
        TextFont {
            font_size: FontSize::Px(11.0),
            ..default()
        },
        TextColor(Color::srgb(0.58, 0.60, 0.66)),
    ));
}
