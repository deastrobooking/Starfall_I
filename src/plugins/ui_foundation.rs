//! Shared GUI theme, localization keys, scaling, and device-aware prompt text.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::resources::{ControllerGlyphStyle, GameSettings};

/// Semantic star-tech palette shared by game screens and creator tools.
#[derive(Resource, Debug, Clone)]
pub struct UiTheme {
    pub canvas: Color,
    pub panel: Color,
    pub text_primary: Color,
    pub text_muted: Color,
    pub play: Color,
    pub create: Color,
    pub objective: Color,
    pub health: Color,
    pub armor: Color,
    pub stamina: Color,
    pub energy: Color,
    pub climb: Color,
    pub players: [Color; 4],
}

impl UiTheme {
    pub fn player_accent(&self, player_index: u8) -> Color {
        self.players[player_index.min(3) as usize]
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            canvas: Color::srgba(0.008, 0.012, 0.030, 1.0),
            panel: Color::srgba(0.035, 0.050, 0.105, 0.82),
            text_primary: Color::srgb(0.90, 0.95, 1.0),
            text_muted: Color::srgb(0.58, 0.70, 0.84),
            play: Color::srgb(0.00, 0.42, 0.78),
            create: Color::srgb(0.46, 0.20, 0.68),
            objective: Color::srgb(1.00, 0.82, 0.22),
            health: Color::srgb(0.22, 0.86, 0.42),
            armor: Color::srgb(0.20, 0.55, 1.00),
            stamina: Color::srgb(0.98, 0.72, 0.10),
            energy: Color::srgb(0.05, 0.90, 0.95),
            climb: Color::srgb(0.98, 0.50, 0.16),
            players: [
                Color::srgb(0.25, 0.82, 1.00),
                Color::srgb(0.88, 0.34, 1.00),
                Color::srgb(0.30, 1.00, 0.55),
                Color::srgb(1.00, 0.66, 0.18),
            ],
        }
    }
}

/// Stable keys keep screen construction independent from English display copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiTextKey {
    GameTitle,
    GameSubtitle,
    StartGame,
    StartEditor,
    Settings,
    SettingsAccessibility,
    Back,
}

#[derive(Resource, Debug, Default)]
pub struct UiTextCatalog {
    overrides: HashMap<UiTextKey, String>,
}

impl UiTextCatalog {
    pub fn text(&self, key: UiTextKey) -> &str {
        self.overrides
            .get(&key)
            .map(String::as_str)
            .unwrap_or_else(|| english_text(key))
    }

    /// Translation loaders can replace individual keys while missing entries
    /// continue to use the complete built-in English catalog.
    #[allow(dead_code)] // Public extension point for the planned locale asset loader.
    pub fn set_override(&mut self, key: UiTextKey, value: impl Into<String>) {
        self.overrides.insert(key, value.into());
    }
}

fn english_text(key: UiTextKey) -> &'static str {
    match key {
        UiTextKey::GameTitle => "STARFALL I",
        UiTextKey::GameSubtitle => "Everest Range",
        UiTextKey::StartGame => "ORIGINAL 3D CAMPAIGN",
        UiTextKey::StartEditor => "START EDITOR",
        UiTextKey::Settings => "SETTINGS",
        UiTextKey::SettingsAccessibility => "SETTINGS & ACCESSIBILITY",
        UiTextKey::Back => "BACK",
    }
}

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
enum UiInputFamily {
    KeyboardMouse,
    #[default]
    Gamepad,
}

#[derive(Resource, Debug, Clone, Copy)]
struct UiPromptDevice {
    family: UiInputFamily,
    detected_glyphs: ControllerGlyphStyle,
}

impl Default for UiPromptDevice {
    fn default() -> Self {
        Self {
            family: UiInputFamily::Gamepad,
            detected_glyphs: ControllerGlyphStyle::Xbox,
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct UiPromptText(pub UiPromptKind);

#[derive(Debug, Clone, Copy)]
pub enum UiPromptKind {
    MenuNavigation,
    PauseNavigation,
    Loadout,
    Crafting,
}

pub struct UiFoundationPlugin;

impl Plugin for UiFoundationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiTheme>()
            .init_resource::<UiTextCatalog>()
            .init_resource::<UiPromptDevice>()
            .add_systems(
                Update,
                (
                    sync_ui_scale_from_settings,
                    track_ui_prompt_device,
                    refresh_ui_prompt_text,
                )
                    .chain(),
            );
    }
}

fn sync_ui_scale_from_settings(settings: Res<GameSettings>, ui_scale: Option<ResMut<UiScale>>) {
    let Some(mut ui_scale) = ui_scale else {
        return;
    };
    let requested = settings.ui_scale.clamp(0.8, 1.4);
    if (ui_scale.0 - requested).abs() > f32::EPSILON {
        ui_scale.0 = requested;
    }
}

fn track_ui_prompt_device(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut device: ResMut<UiPromptDevice>,
) {
    let keyboard_used = keyboard.get_just_pressed().next().is_some();
    let active_gamepad = gamepads
        .iter()
        .find(|gamepad| gamepad.get_just_pressed().next().is_some());
    if let Some(gamepad) = active_gamepad {
        device.family = UiInputFamily::Gamepad;
        device.detected_glyphs = glyph_style_for_vendor(gamepad.vendor_id());
    } else if keyboard_used {
        device.family = UiInputFamily::KeyboardMouse;
    }
}

fn refresh_ui_prompt_text(
    device: Res<UiPromptDevice>,
    settings: Res<GameSettings>,
    mut prompts: Query<(Ref<UiPromptText>, &mut Text)>,
) {
    for (prompt, mut text) in prompts.iter_mut() {
        if device.is_changed() || settings.is_changed() || prompt.is_added() {
            let glyphs = match settings.controller_glyph_style {
                ControllerGlyphStyle::Auto => device.detected_glyphs,
                explicit => explicit,
            };
            *text = Text::new(prompt_text(prompt.0, device.family, glyphs));
        }
    }
}

fn glyph_style_for_vendor(vendor_id: Option<u16>) -> ControllerGlyphStyle {
    match vendor_id {
        Some(0x054c) => ControllerGlyphStyle::PlayStation,
        Some(0x057e) => ControllerGlyphStyle::Nintendo,
        _ => ControllerGlyphStyle::Xbox,
    }
}

fn prompt_text(kind: UiPromptKind, family: UiInputFamily, glyphs: ControllerGlyphStyle) -> String {
    if family == UiInputFamily::KeyboardMouse {
        return match kind {
            UiPromptKind::MenuNavigation => {
                "ARROWS / WASD: navigate and scroll   ENTER: select   ESC: back".into()
            }
            UiPromptKind::PauseNavigation => {
                "ARROWS / WASD: navigate   ENTER: select   ESC: back / resume".into()
            }
            UiPromptKind::Loadout => {
                "ARROWS/WASD: navigate   ENTER: equip   ESC or I: close".into()
            }
            UiPromptKind::Crafting => {
                "CRAFTING — ARROWS / WASD: choose   ENTER: craft   ESC or C: close".into()
            }
        };
    }

    let (confirm, cancel, shoulder, system) = match glyphs {
        ControllerGlyphStyle::PlayStation => ("CROSS", "CIRCLE", "L1", "CREATE"),
        ControllerGlyphStyle::Nintendo => ("B", "A", "L", "MINUS"),
        ControllerGlyphStyle::Auto | ControllerGlyphStyle::Xbox => ("A", "B", "LB", "VIEW"),
    };
    match kind {
        UiPromptKind::MenuNavigation => {
            format!("D-PAD / LEFT STICK: navigate and scroll   {confirm}: select   {cancel}: back")
        }
        UiPromptKind::PauseNavigation => {
            format!("D-PAD / LEFT STICK: navigate   {confirm}: select   {cancel}: back / resume")
        }
        UiPromptKind::Loadout => format!(
            "D-PAD/STICK: navigate   {confirm}: equip   {cancel} or {shoulder}+{system}: close"
        ),
        UiPromptKind::Crafting => format!(
            "CRAFTING — D-PAD / STICK: choose   {confirm}: craft   {cancel} or {system}: close"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_catalog_falls_back_to_complete_english_copy() {
        let mut catalog = UiTextCatalog::default();
        assert_eq!(catalog.text(UiTextKey::StartGame), "ORIGINAL 3D CAMPAIGN");
        catalog.set_override(UiTextKey::StartGame, "COMMENCER");
        assert_eq!(catalog.text(UiTextKey::StartGame), "COMMENCER");
        assert_eq!(catalog.text(UiTextKey::Back), "BACK");
    }

    #[test]
    fn usb_vendor_detection_selects_expected_controller_family() {
        assert_eq!(
            glyph_style_for_vendor(Some(0x054c)),
            ControllerGlyphStyle::PlayStation
        );
        assert_eq!(
            glyph_style_for_vendor(Some(0x057e)),
            ControllerGlyphStyle::Nintendo
        );
        assert_eq!(
            glyph_style_for_vendor(Some(0x045e)),
            ControllerGlyphStyle::Xbox
        );
        assert_eq!(glyph_style_for_vendor(None), ControllerGlyphStyle::Xbox);
    }

    #[test]
    fn prompts_use_controller_family_specific_labels() {
        let playstation = prompt_text(
            UiPromptKind::Loadout,
            UiInputFamily::Gamepad,
            ControllerGlyphStyle::PlayStation,
        );
        let nintendo = prompt_text(
            UiPromptKind::Loadout,
            UiInputFamily::Gamepad,
            ControllerGlyphStyle::Nintendo,
        );
        assert!(playstation.contains("CROSS") && playstation.contains("L1+CREATE"));
        assert!(nintendo.contains("B: equip") && nintendo.contains("L+MINUS"));
    }
}
