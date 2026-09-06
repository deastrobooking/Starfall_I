//! One shared spacing scale for every creator-tool GUI surface.
//!
//! Before this module, every screen invented its own `Val::Px(...)` literals
//! for gaps and padding — `forge_widgets::widget_row` used `6.0`/`5.0`,
//! `tool_windows`'s content column used `5.0`/`12.0`, `action_button` used
//! `8.0`/`3.0`, and so on: all close, none shared, so a deliberate spacing
//! change had no single place to land. These four steps are that place.
//! [`docs/GUI_SYSTEM.md`](../../docs/GUI_SYSTEM.md) tracks adoption.
//!
//! There is no `XL` step yet — nothing in the current GUI needs a gap wider
//! than [`LG`]; add one when a real screen does, not speculatively.

use bevy::ui::Val;

/// Tightest gap: within a compact control (e.g. a stepper row's internal
/// button padding).
pub const XS: Val = Val::Px(3.0);
/// The default gap between adjacent widgets in a row or column.
pub const SM: Val = Val::Px(5.0);
/// A button's horizontal padding, or the gap between unrelated widget groups.
pub const MD: Val = Val::Px(8.0);
/// A panel's outer padding.
pub const LG: Val = Val::Px(12.0);
