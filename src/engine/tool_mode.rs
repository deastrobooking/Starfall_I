//! The Playing/Editing app-mode state — a small, genuinely cross-cutting
//! sibling of [`crate::engine::state::AppState`], not an editor implementation
//! detail. It lives here, not in `engine_tools`, because non-designer
//! gameplay files (`plugins::input_plugin`, `app`) branch on it directly
//! even though the Game edition never actually transitions into `Editing`
//! (the Project Hub button and the Tab/controller chord that could request
//! it are both `designer`-feature-gated — see `engine_tools`).

use bevy::prelude::*;

/// Independent of `AppState`: later workspaces can edit a loaded chapter while
/// retaining its game state. No global hotkey is installed until ET2 owns
/// camera, cursor, and gameplay-input capture.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineToolMode {
    #[default]
    Playing,
    Editing,
}
