//! Player-configurable control bindings (docs/PROJECT_PLAN.md P3).
//!
//! Two things are configurable, and one deliberately is not.
//!
//! **Face-button layout** is remapped at the source: `update_player_inputs`
//! reads every gamepad button through [`ControlBindings::remap_face`], so a
//! swap follows *everything* — including chords. If a player moves the sabre
//! toggle's North to West, `LB + North` becomes `LB + West` automatically
//! instead of the chord quietly pointing at the old key. Nintendo-layout pads
//! report their physical A/B and X/Y positions swapped from Xbox, so this is
//! the single most-needed rebind in the whole system.
//!
//! **Keyboard action keys** are looked up per action for player one.
//!
//! **Chords and modifiers are not rebindable, by design.** LB is *the*
//! modifier, Select is *the* utility modifier, the bare D-pad swaps what RT
//! fires. Those are structural grammar rather than preferences; making them
//! configurable would let a player build a layout in which some actions are
//! unreachable, and every context claim (drawn sabre, hoverboard) would need
//! re-verification per layout. The manual says so out loud.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// The four face-button positions, named by physical position rather than by
/// any vendor's letters (Xbox and Nintendo disagree about which letter sits
/// where, which is the entire reason this type exists).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FaceButton {
    South,
    East,
    West,
    North,
}

impl FaceButton {
    pub const ALL: [FaceButton; 4] = [
        FaceButton::South,
        FaceButton::East,
        FaceButton::West,
        FaceButton::North,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FaceButton::South => "South (A/B)",
            FaceButton::East => "East (B/A)",
            FaceButton::West => "West (X/Y)",
            FaceButton::North => "North (Y/X)",
        }
    }
}

/// A named face-button arrangement. `Custom` carries an explicit
/// logical→physical permutation so a player can build any arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FaceLayout {
    /// As labelled on an Xbox-style pad: what the game calls South is South.
    #[default]
    Standard,
    /// Nintendo-style pads report the positions the player sees as A/B and
    /// X/Y transposed; this swaps both pairs back.
    Nintendo,
    /// Swap only confirm/cancel, leaving West/North alone.
    SwapConfirmCancel,
    /// Swap only the upper pair, leaving confirm/cancel alone.
    SwapUpperPair,
}

impl FaceLayout {
    pub const PRESETS: [FaceLayout; 4] = [
        FaceLayout::Standard,
        FaceLayout::Nintendo,
        FaceLayout::SwapConfirmCancel,
        FaceLayout::SwapUpperPair,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FaceLayout::Standard => "Standard",
            FaceLayout::Nintendo => "Nintendo",
            FaceLayout::SwapConfirmCancel => "Swap A/B",
            FaceLayout::SwapUpperPair => "Swap X/Y",
        }
    }

    /// The physical button a logical face slot resolves to.
    pub fn physical(self, logical: FaceButton) -> FaceButton {
        match self {
            FaceLayout::Standard => logical,
            // Each preset swaps one or both pairs; anything it does not
            // touch passes through unchanged.
            FaceLayout::Nintendo | FaceLayout::SwapConfirmCancel | FaceLayout::SwapUpperPair => {
                let swap_lower =
                    matches!(self, FaceLayout::Nintendo | FaceLayout::SwapConfirmCancel);
                let swap_upper = matches!(self, FaceLayout::Nintendo | FaceLayout::SwapUpperPair);
                match logical {
                    FaceButton::South if swap_lower => FaceButton::East,
                    FaceButton::East if swap_lower => FaceButton::South,
                    FaceButton::West if swap_upper => FaceButton::North,
                    FaceButton::North if swap_upper => FaceButton::West,
                    other => other,
                }
            }
        }
    }

    /// One-line description of what this layout actually does to each slot,
    /// for the settings readout.
    pub fn mapping_summary(self) -> String {
        FaceButton::ALL
            .into_iter()
            .filter(|logical| self.physical(*logical) != *logical)
            .map(|logical| format!("{} → {}", logical.label(), self.physical(logical).label()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Discrete player-one keyboard actions that can be rebound. Movement (WASD)
/// and mouse look are excluded: they are axes, not discrete presses, and the
/// menus assume them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyAction {
    Jump,
    Dodge,
    Reload,
    Parry,
    Interact,
    QuickItem,
    SabreToggle,
    OpenMap,
    Crafting,
    EnterVehicle,
    Grapple,
    LoadoutMenu,
    TogglePerspective,
}

impl KeyAction {
    pub const ALL: [KeyAction; 13] = [
        KeyAction::Jump,
        KeyAction::Dodge,
        KeyAction::Reload,
        KeyAction::Parry,
        KeyAction::Interact,
        KeyAction::QuickItem,
        KeyAction::SabreToggle,
        KeyAction::OpenMap,
        KeyAction::Crafting,
        KeyAction::EnterVehicle,
        KeyAction::Grapple,
        KeyAction::LoadoutMenu,
        KeyAction::TogglePerspective,
    ];

    pub fn label(self) -> &'static str {
        match self {
            KeyAction::Jump => "Jump / Jetpack",
            KeyAction::Dodge => "Dodge",
            KeyAction::Reload => "Reload",
            KeyAction::Parry => "Parry",
            KeyAction::Interact => "Interact",
            KeyAction::QuickItem => "Quick item",
            KeyAction::SabreToggle => "Toggle Star Sabre",
            KeyAction::OpenMap => "Open map",
            KeyAction::Crafting => "Crafting",
            KeyAction::EnterVehicle => "Enter vehicle",
            KeyAction::Grapple => "Grapple",
            KeyAction::LoadoutMenu => "Loadout",
            KeyAction::TogglePerspective => "Toggle first / third person",
        }
    }

    /// The shipped default for this action.
    pub fn default_key(self) -> KeyCode {
        match self {
            KeyAction::Jump => KeyCode::Space,
            KeyAction::Dodge => KeyCode::KeyQ,
            KeyAction::Reload => KeyCode::KeyR,
            KeyAction::Parry => KeyCode::KeyF,
            KeyAction::Interact => KeyCode::KeyE,
            KeyAction::QuickItem => KeyCode::KeyH,
            KeyAction::SabreToggle => KeyCode::KeyT,
            KeyAction::OpenMap => KeyCode::KeyM,
            KeyAction::Crafting => KeyCode::KeyC,
            KeyAction::EnterVehicle => KeyCode::KeyJ,
            KeyAction::Grapple => KeyCode::KeyG,
            KeyAction::LoadoutMenu => KeyCode::KeyI,
            KeyAction::TogglePerspective => KeyCode::KeyP,
        }
    }
}

/// Every key a player may bind, with the name it is stored under.
///
/// A whitelist rather than all of `KeyCode` for three reasons: settings files
/// stay human-readable, `KeyCode` carries no serde impl here, and the UI needs
/// exactly this list to decide whether a captured press is bindable at all.
pub const BINDABLE_KEYS: [(KeyCode, &str); 40] = [
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyD, "D"),
    (KeyCode::KeyE, "E"),
    (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"),
    (KeyCode::KeyH, "H"),
    (KeyCode::KeyI, "I"),
    (KeyCode::KeyJ, "J"),
    (KeyCode::KeyK, "K"),
    (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"),
    (KeyCode::KeyN, "N"),
    (KeyCode::KeyO, "O"),
    (KeyCode::KeyP, "P"),
    (KeyCode::KeyQ, "Q"),
    (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyT, "T"),
    (KeyCode::KeyU, "U"),
    (KeyCode::KeyV, "V"),
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"),
    (KeyCode::KeyZ, "Z"),
    (KeyCode::Space, "Space"),
    (KeyCode::ShiftLeft, "LShift"),
    (KeyCode::ControlLeft, "LCtrl"),
    (KeyCode::AltLeft, "LAlt"),
    (KeyCode::Tab, "Tab"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::Escape, "Escape"),
    (KeyCode::Backquote, "`"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
    (KeyCode::BracketLeft, "["),
    (KeyCode::BracketRight, "]"),
    (KeyCode::Semicolon, ";"),
    (KeyCode::Quote, "'"),
];

/// Display/storage name for a key, or `"?"` for anything unbindable.
pub fn key_name(key: KeyCode) -> &'static str {
    BINDABLE_KEYS
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, name)| *name)
        .unwrap_or("?")
}

/// Parse a stored key name back into a `KeyCode`.
pub fn key_from_name(name: &str) -> Option<KeyCode> {
    BINDABLE_KEYS
        .iter()
        .find(|(_, candidate)| *candidate == name)
        .map(|(key, _)| *key)
}

/// Keys the game reserves; rebinding onto one would strand the player.
pub const RESERVED_KEYS: [KeyCode; 8] = [
    KeyCode::Escape,
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::Enter,
    KeyCode::Tab,
    KeyCode::F11,
];

/// Why a requested rebind was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebindRejection {
    /// The key runs the game itself (movement, pause, menus).
    Reserved,
    /// Another action already uses it.
    Conflict(KeyAction),
}

/// One rebound action, stored by key name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyOverride {
    pub action: KeyAction,
    pub key: String,
}

/// The player's control configuration. Serialized inside `GameSettings`, so
/// it rides the existing settings save/load with no new file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ControlBindings {
    #[serde(default)]
    pub face_layout: FaceLayout,
    /// Overrides only; anything absent uses [`KeyAction::default_key`], so a
    /// settings file written before a new action existed still loads. Keys
    /// are stored by name (see [`BINDABLE_KEYS`]) to keep the file readable.
    #[serde(default)]
    pub key_overrides: Vec<KeyOverride>,
    #[serde(default)]
    pub invert_look_y: bool,
}

impl ControlBindings {
    /// The key currently bound to `action`.
    pub fn key(&self, action: KeyAction) -> KeyCode {
        self.key_overrides
            .iter()
            .find(|override_| override_.action == action)
            // An unparsable name (hand-edited file, older build) falls back to
            // the default rather than leaving the action unbound.
            .and_then(|override_| key_from_name(&override_.key))
            .unwrap_or_else(|| action.default_key())
    }

    /// Which action currently uses `key`, if any.
    pub fn action_using(&self, key: KeyCode) -> Option<KeyAction> {
        KeyAction::ALL
            .into_iter()
            .find(|action| self.key(*action) == key)
    }

    /// Bind `key` to `action`, refusing reserved keys and conflicts. Binding
    /// an action to the key it already has is a no-op success, so
    /// double-tapping a rebind prompt cannot report a conflict with itself.
    pub fn rebind(&mut self, action: KeyAction, key: KeyCode) -> Result<(), RebindRejection> {
        if RESERVED_KEYS.contains(&key) || key_name(key) == "?" {
            return Err(RebindRejection::Reserved);
        }
        match self.action_using(key) {
            Some(existing) if existing != action => {
                return Err(RebindRejection::Conflict(existing))
            }
            _ => {}
        }
        self.key_overrides
            .retain(|override_| override_.action != action);
        if key != action.default_key() {
            self.key_overrides.push(KeyOverride {
                action,
                key: key_name(key).to_string(),
            });
        }
        Ok(())
    }

    /// Restore every key to its shipped default.
    pub fn reset_keys(&mut self) {
        self.key_overrides.clear();
    }

    /// Resolve a logical face button to the physical one to read.
    pub fn remap_face(&self, logical: FaceButton) -> FaceButton {
        self.face_layout.physical(logical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_standard_layout_changes_nothing() {
        let bindings = ControlBindings::default();
        assert_eq!(bindings.face_layout, FaceLayout::Standard);
        for face in FaceButton::ALL {
            assert_eq!(bindings.remap_face(face), face);
        }
        for action in KeyAction::ALL {
            assert_eq!(bindings.key(action), action.default_key());
        }
    }

    #[test]
    fn nintendo_transposes_both_pairs_and_is_its_own_inverse() {
        let layout = FaceLayout::Nintendo;
        assert_eq!(layout.physical(FaceButton::South), FaceButton::East);
        assert_eq!(layout.physical(FaceButton::East), FaceButton::South);
        assert_eq!(layout.physical(FaceButton::West), FaceButton::North);
        assert_eq!(layout.physical(FaceButton::North), FaceButton::West);
        // Applying it twice returns the original — the mark of a pure swap.
        for face in FaceButton::ALL {
            assert_eq!(layout.physical(layout.physical(face)), face);
        }
    }

    #[test]
    fn swap_confirm_cancel_leaves_the_upper_pair_alone() {
        let layout = FaceLayout::SwapConfirmCancel;
        assert_eq!(layout.physical(FaceButton::South), FaceButton::East);
        assert_eq!(layout.physical(FaceButton::East), FaceButton::South);
        assert_eq!(layout.physical(FaceButton::West), FaceButton::West);
        assert_eq!(layout.physical(FaceButton::North), FaceButton::North);
    }

    #[test]
    fn every_layout_is_a_permutation_so_no_button_is_unreachable() {
        // A layout that mapped two logical slots onto one physical button
        // would make an action impossible to press.
        for layout in FaceLayout::PRESETS {
            let mut mapped: Vec<FaceButton> = FaceButton::ALL
                .into_iter()
                .map(|f| layout.physical(f))
                .collect();
            mapped.sort_by_key(|f| format!("{f:?}"));
            mapped.dedup();
            assert_eq!(mapped.len(), 4, "{layout:?} is not a permutation");
        }
    }

    #[test]
    fn the_mapping_summary_names_only_what_actually_moves() {
        assert!(
            FaceLayout::Standard.mapping_summary().is_empty(),
            "the identity layout has nothing to report"
        );
        let nintendo = FaceLayout::Nintendo.mapping_summary();
        assert_eq!(nintendo.matches('→').count(), 4, "all four slots move");

        let lower = FaceLayout::SwapConfirmCancel.mapping_summary();
        assert_eq!(lower.matches('→').count(), 2);
        assert!(lower.contains("South"), "{lower}");
        assert!(!lower.contains("West"), "upper pair untouched: {lower}");

        let upper = FaceLayout::SwapUpperPair.mapping_summary();
        assert_eq!(upper.matches('→').count(), 2);
        assert!(upper.contains("West"), "{upper}");
        assert!(!upper.contains("South"), "lower pair untouched: {upper}");
    }

    #[test]
    fn rebinding_refuses_reserved_keys_and_conflicts() {
        let mut bindings = ControlBindings::default();

        // Movement and pause keys would strand the player.
        assert_eq!(
            bindings.rebind(KeyAction::Dodge, KeyCode::KeyW),
            Err(RebindRejection::Reserved)
        );
        assert_eq!(
            bindings.rebind(KeyAction::Dodge, KeyCode::Escape),
            Err(RebindRejection::Reserved)
        );
        // The refused attempts changed nothing.
        assert_eq!(bindings.key(KeyAction::Dodge), KeyCode::KeyQ);

        // Taking another action's key is refused, and names the culprit so the
        // UI can say which one.
        assert_eq!(
            bindings.rebind(KeyAction::Dodge, KeyCode::KeyR),
            Err(RebindRejection::Conflict(KeyAction::Reload))
        );

        // A free key works.
        assert!(bindings.rebind(KeyAction::Dodge, KeyCode::KeyZ).is_ok());
        assert_eq!(bindings.key(KeyAction::Dodge), KeyCode::KeyZ);
        assert_eq!(bindings.action_using(KeyCode::KeyZ), Some(KeyAction::Dodge));
        // And the vacated default is now free for someone else.
        assert!(bindings.rebind(KeyAction::Reload, KeyCode::KeyQ).is_ok());
    }

    #[test]
    fn rebinding_an_action_to_the_key_it_already_has_is_a_no_op() {
        let mut bindings = ControlBindings::default();
        assert!(bindings.rebind(KeyAction::Dodge, KeyCode::KeyQ).is_ok());
        assert_eq!(bindings.key(KeyAction::Dodge), KeyCode::KeyQ);
        // Re-binding to the default must not leave a redundant override.
        assert!(bindings.key_overrides.is_empty());
    }

    #[test]
    fn resetting_restores_every_shipped_default() {
        let mut bindings = ControlBindings::default();
        bindings.rebind(KeyAction::Dodge, KeyCode::KeyZ).unwrap();
        bindings.rebind(KeyAction::Parry, KeyCode::KeyX).unwrap();
        bindings.reset_keys();
        for action in KeyAction::ALL {
            assert_eq!(bindings.key(action), action.default_key());
        }
    }

    #[test]
    fn defaults_are_unique_so_a_fresh_install_has_no_conflicts() {
        let mut keys: Vec<String> = KeyAction::ALL
            .into_iter()
            .map(|action| format!("{:?}", action.default_key()))
            .collect();
        let count = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), count, "two actions ship on the same key");

        // …and none of them sits on a reserved key.
        for action in KeyAction::ALL {
            assert!(
                !RESERVED_KEYS.contains(&action.default_key()),
                "{action:?} defaults onto a reserved key"
            );
        }
    }

    #[test]
    fn settings_written_before_an_action_existed_still_load() {
        // Overrides are a sparse list, so an old file simply lacks entries.
        let legacy = serde_json::json!({ "face_layout": "Nintendo" });
        let bindings: ControlBindings = serde_json::from_value(legacy).unwrap();
        assert_eq!(bindings.face_layout, FaceLayout::Nintendo);
        assert_eq!(bindings.key(KeyAction::Dodge), KeyCode::KeyQ);
        assert!(!bindings.invert_look_y);
    }
}
