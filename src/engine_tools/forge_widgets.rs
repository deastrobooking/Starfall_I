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

use super::gui_spacing;

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
    /// Muted tint for a live [`stepper_row`] readout — dimmer than `text` so
    /// a displayed number reads as data rather than as an available action.
    pub readout_text: Color,
    /// Minimum width reserved for a [`stepper_row`] readout label, so a
    /// stack of stepper rows keeps its +/− buttons aligned into a column.
    pub readout_min_width: f32,
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
            readout_text: Color::srgb(0.84, 0.90, 0.96),
            readout_min_width: 160.0,
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
                padding: UiRect::axes(gui_spacing::MD, gui_spacing::XS),
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
            column_gap: gui_spacing::SM,
            margin: UiRect::bottom(gui_spacing::SM),
            flex_wrap: FlexWrap::Wrap,
            row_gap: gui_spacing::SM,
            ..default()
        })
        .with_children(build);
}

/// A row pairing a live numeric readout with −/+ stepper buttons — the
/// standard "adjust one field" unit shared by every specs-driven forge
/// (Vehicle, Spaceship, and whatever comes next). The caller supplies the
/// readout's own marker component (e.g. `VehicleForgeFieldText(field)`) so
/// each tool's own display-refresh system still owns writing the text, and
/// its own decrement/increment action bundles so interaction stays per-tool.
pub fn stepper_row(
    parent: &mut ChildSpawnerCommands,
    readout_marker: impl Bundle,
    decrement_action: impl Bundle,
    increment_action: impl Bundle,
    style: &ForgeWidgetStyle,
) {
    widget_row(parent, |row| {
        row.spawn((
            Text::new(""),
            readout_marker,
            Node {
                min_width: Val::Px(style.readout_min_width),
                ..default()
            },
            TextFont {
                font_size: FontSize::Px(style.font_size - 1.0),
                ..default()
            },
            TextColor(style.readout_text),
        ));
        action_button(row, "−", decrement_action, style);
        action_button(row, "+", increment_action, style);
    });
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
