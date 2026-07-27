//! Data-driven hoverboard trick data: `TrickDef` + `TrickLibrary`.
//!
//! The Tony-Hawk-style trick roster is authored as data, the same way melee
//! frame data lives in `src/combat_data.rs`. Every trick declares its
//! category, input (button + stick direction), score, hold duration, and the
//! combo multiplier it contributes. On first run the defaults are written to
//! `assets/combat/tricks.json`; edit that file and relaunch to retune the
//! whole roster **without recompiling**. Delete it to restore defaults.
//!
//! Input mapping while the Rocket Hoverboard is the active traversal mode
//! (the board claims these buttons the way a drawn Star Sabre claims heavy —
//! see `hoverboard_claims_trick_input`):
//!
//! | Button            | Category |
//! |-------------------|----------|
//! | melee_light (X/□) | Flip     |
//! | melee_heavy (Y/△) | Grab     |
//! | dodge (B/○)       | Grind    |
//! | parry             | Manual   |
//!
//! The stick direction held at press time selects the variant, so each
//! category fans out into five tricks (neutral plus four directions).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::components::player::TraversalMode;

/// Which family a trick belongs to. Determines when it may be started:
/// grabs/flips need air, grinds need a rail, manuals need ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrickCategory {
    Grab,
    Flip,
    Grind,
    Manual,
    Lip,
    Special,
}

impl TrickCategory {
    pub fn label(self) -> &'static str {
        match self {
            TrickCategory::Grab => "Grab",
            TrickCategory::Flip => "Flip",
            TrickCategory::Grind => "Grind",
            TrickCategory::Manual => "Manual",
            TrickCategory::Lip => "Lip",
            TrickCategory::Special => "Special",
        }
    }

    /// Grabs, flips, and specials are air tricks; grinds ride a rail; manuals
    /// and lips are surface tricks.
    pub fn needs_air(self) -> bool {
        matches!(
            self,
            TrickCategory::Grab | TrickCategory::Flip | TrickCategory::Special
        )
    }
}

/// Stick direction held when the trick button is pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrickDir {
    Neutral,
    Up,
    Down,
    Left,
    Right,
}

impl TrickDir {
    /// Classify a stick vector. The dead zone is generous so a neutral trick
    /// is easy to land deliberately.
    pub fn from_axis(axis: Vec2) -> Self {
        const DEAD_ZONE: f32 = 0.45;
        if axis.length_squared() < DEAD_ZONE * DEAD_ZONE {
            return TrickDir::Neutral;
        }
        if axis.y.abs() >= axis.x.abs() {
            if axis.y > 0.0 {
                TrickDir::Up
            } else {
                TrickDir::Down
            }
        } else if axis.x > 0.0 {
            TrickDir::Right
        } else {
            TrickDir::Left
        }
    }
}

/// One authored trick.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrickDef {
    pub name: String,
    pub category: TrickCategory,
    /// Stick direction that selects this trick within its category.
    pub direction: TrickDir,
    /// Score before combo multiplier and repeat decay.
    pub base_score: f32,
    /// Seconds the trick pose holds. Air tricks must be released (landed)
    /// before this expires or the rider bails.
    pub duration: f32,
    /// Combo multiplier added when the trick lands cleanly.
    pub multiplier_bonus: f32,
    /// Special tricks only become available once the combo multiplier has
    /// been built up (see [`TrickLibrary::special_multiplier_threshold`]).
    #[serde(default)]
    pub special: bool,
}

/// All authored tricks, loaded from `assets/combat/tricks.json` (or defaults).
#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
pub struct TrickLibrary {
    pub tricks: Vec<TrickDef>,
    /// Combo multiplier required before special tricks replace their
    /// same-input basic trick.
    #[serde(default = "default_special_threshold")]
    pub special_multiplier_threshold: f32,
    /// Score per full 360° of spin, banked on landing.
    #[serde(default = "default_spin_score")]
    pub spin_score_per_rotation: f32,
    /// Ceiling on the combo multiplier.
    #[serde(default = "default_max_multiplier")]
    pub max_multiplier: f32,
    /// Seconds after landing an air trick during which another trick (or a
    /// rail) links into the same combo — the THPS3 revert. Without it every
    /// landing banks the run and long combos are impossible.
    #[serde(default = "default_revert_window")]
    pub revert_window: f32,
    /// Multiplier added for chaining through a revert.
    #[serde(default = "default_revert_bonus")]
    pub revert_multiplier_bonus: f32,
}

fn default_special_threshold() -> f32 {
    3.0
}

fn default_spin_score() -> f32 {
    325.0
}

fn default_max_multiplier() -> f32 {
    8.0
}

fn default_revert_window() -> f32 {
    0.85
}

fn default_revert_bonus() -> f32 {
    0.2
}

impl TrickLibrary {
    /// The trick bound to a button+direction, honoring the special-trick
    /// upgrade once the combo multiplier is high enough. Specials are
    /// checked first so they shadow their basic counterpart.
    pub fn resolve(
        &self,
        category: TrickCategory,
        direction: TrickDir,
        multiplier: f32,
    ) -> Option<&TrickDef> {
        if multiplier >= self.special_multiplier_threshold {
            if let Some(special) = self
                .tricks
                .iter()
                .find(|t| t.special && t.category == category && t.direction == direction)
            {
                return Some(special);
            }
        }
        self.tricks
            .iter()
            .find(|t| !t.special && t.category == category && t.direction == direction)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.tricks.iter().position(|t| t.name == name)
    }

    /// Repeat decay from the Tony Hawk scoring rules: reusing a trick inside
    /// one combo is worth progressively less (2nd ÷1.33, 3rd ÷1.5, 4th+ ÷2).
    pub fn repeat_divisor(times_already_used: u32) -> f32 {
        match times_already_used {
            0 => 1.0,
            1 => 1.33,
            2 => 1.5,
            _ => 2.0,
        }
    }

    /// Rotation bonus multiplier: a half rotation is worth 1.5x, a full
    /// rotation 2x, and it keeps climbing for multi-spins.
    pub fn spin_bonus(spin_degrees: f32) -> f32 {
        let half_rotations = (spin_degrees.abs() / 180.0).floor();
        1.0 + half_rotations * 0.5
    }

    /// The shipping roster. Names follow the real trick vocabulary so the
    /// HUD reads like a skate game.
    pub fn defaults() -> Self {
        let t = |name: &str,
                 category: TrickCategory,
                 direction: TrickDir,
                 base_score: f32,
                 duration: f32,
                 multiplier_bonus: f32| TrickDef {
            name: name.to_string(),
            category,
            direction,
            base_score,
            duration,
            multiplier_bonus,
            special: false,
        };
        let special = |name: &str,
                       category: TrickCategory,
                       direction: TrickDir,
                       base_score: f32,
                       duration: f32,
                       multiplier_bonus: f32| TrickDef {
            name: name.to_string(),
            category,
            direction,
            base_score,
            duration,
            multiplier_bonus,
            special: true,
        };
        use TrickCategory::*;
        use TrickDir::*;
        Self {
            tricks: vec![
                // ── Grabs (air, melee_heavy) ─────────────────────────────
                t("Indy", Grab, Neutral, 450.0, 0.85, 0.25),
                t("Nosegrab", Grab, Up, 520.0, 0.85, 0.25),
                t("Tailgrab", Grab, Down, 520.0, 0.85, 0.25),
                t("Melon", Grab, Left, 600.0, 0.90, 0.30),
                t("Stalefish", Grab, Right, 600.0, 0.90, 0.30),
                // ── Flips (air, melee_light) ─────────────────────────────
                t("Kickflip", Flip, Neutral, 480.0, 0.70, 0.25),
                t("Heelflip", Flip, Up, 520.0, 0.70, 0.25),
                t("Pop Shove-It", Flip, Down, 500.0, 0.70, 0.25),
                t("Varial Kickflip", Flip, Left, 640.0, 0.80, 0.30),
                t("Impossible", Flip, Right, 700.0, 0.85, 0.35),
                // ── Grinds (rail, dodge) ─────────────────────────────────
                t("50-50", Grind, Neutral, 250.0, 0.0, 0.25),
                t("Nosegrind", Grind, Up, 300.0, 0.0, 0.30),
                t("5-0", Grind, Down, 300.0, 0.0, 0.30),
                t("Smith Grind", Grind, Left, 350.0, 0.0, 0.35),
                t("Feeble Grind", Grind, Right, 350.0, 0.0, 0.35),
                // ── Manuals (ground, parry) ──────────────────────────────
                t("Manual", Manual, Neutral, 120.0, 0.0, 0.20),
                t("Nose Manual", Manual, Up, 160.0, 0.0, 0.25),
                t("One Wheel Manual", Manual, Down, 160.0, 0.0, 0.25),
                // ── Lip tricks (rail apex, parry in air over a rail) ─────
                t("Rocket Lip", Lip, Neutral, 400.0, 0.55, 0.30),
                // ── Specials (unlocked by combo multiplier) ──────────────
                special("The 900", Grab, Neutral, 3200.0, 1.10, 0.75),
                special("Christ Air", Grab, Up, 2600.0, 1.05, 0.60),
                special("Kickflip McTwist", Flip, Neutral, 2800.0, 1.00, 0.65),
                special("Darkslide", Grind, Neutral, 1800.0, 0.0, 0.60),
            ],
            special_multiplier_threshold: default_special_threshold(),
            spin_score_per_rotation: default_spin_score(),
            max_multiplier: default_max_multiplier(),
            revert_window: default_revert_window(),
            revert_multiplier_bonus: default_revert_bonus(),
        }
    }

    /// Reject data that would break scoring or leave a category unreachable.
    pub fn validate(&self) -> Result<(), String> {
        if self.tricks.is_empty() {
            return Err("the trick roster needs at least one trick".into());
        }
        for def in &self.tricks {
            if def.name.trim().is_empty() {
                return Err("every trick needs a name".into());
            }
            if def.base_score <= 0.0 {
                return Err(format!("'{}': base_score must be > 0", def.name));
            }
            if def.duration < 0.0 {
                return Err(format!("'{}': duration must be >= 0", def.name));
            }
            if def.category.needs_air() && def.duration <= 0.0 {
                return Err(format!(
                    "'{}': air tricks need a positive hold duration",
                    def.name
                ));
            }
            if def.multiplier_bonus < 0.0 {
                return Err(format!("'{}': multiplier_bonus must be >= 0", def.name));
            }
        }
        // Duplicate (special, category, direction) triples would make one
        // trick unreachable — the resolver takes the first match.
        let mut seen = Vec::new();
        for def in &self.tricks {
            let key = (def.special, def.category, def.direction);
            if seen.contains(&key) {
                return Err(format!(
                    "duplicate binding for {:?} {:?} (special: {})",
                    def.category, def.direction, def.special
                ));
            }
            seen.push(key);
        }
        if self.special_multiplier_threshold <= 1.0 {
            return Err("special_multiplier_threshold must exceed the base 1.0 multiplier".into());
        }
        if self.max_multiplier <= 1.0 {
            return Err("max_multiplier must exceed 1.0".into());
        }
        if self.spin_score_per_rotation < 0.0 {
            return Err("spin_score_per_rotation must be >= 0".into());
        }
        if self.revert_window < 0.0 || self.revert_multiplier_bonus < 0.0 {
            return Err("revert window and bonus must be >= 0".into());
        }
        Ok(())
    }
}

/// While the Rocket Hoverboard is the active traversal mode, the flip / grab /
/// grind / manual buttons (`melee_light`, `melee_heavy`, `dodge`, `parry`)
/// belong to the trick system — combat and evasion must not also consume
/// them. Deliberately independent of any cooldown so the claim cannot depend
/// on cross-plugin system ordering, matching `sabre_claims_heavy_input`.
pub fn hoverboard_claims_trick_input(traversal: TraversalMode) -> bool {
    traversal == TraversalMode::Hoverboard
}

/// The trick a rider is currently performing.
#[derive(Debug, Clone)]
pub struct ActiveTrick {
    /// Index into [`TrickLibrary::tricks`].
    pub index: usize,
    pub name: String,
    pub category: TrickCategory,
    /// Counts down while held; air tricks must land before it hits zero.
    pub remaining: f32,
    pub base_score: f32,
    pub multiplier_bonus: f32,
}

/// Per-player trick runtime state. Score/multiplier accumulation itself stays
/// in `StuntRunState`; this tracks what is being performed and what has
/// already been used this combo (for repeat decay).
#[derive(Component, Debug, Clone, Default)]
pub struct TrickState {
    pub active: Option<ActiveTrick>,
    /// How many times each trick index has landed in the current combo.
    pub combo_uses: Vec<(usize, u32)>,
    /// Name of the most recent landed trick, for HUD readout.
    pub last_trick: Option<String>,
    /// Set for one frame when a trick is bailed, so feedback can react.
    pub bailed: bool,
    /// Seconds left in the post-landing revert window. Landing an air trick
    /// opens it; holding a trick button (or riding into a rail) inside the
    /// window keeps the combo alive across the landing instead of banking it.
    pub revert_window: f32,
    /// True while the combo is being carried through a revert, so the HUD and
    /// pose can show the rider still working.
    pub reverting: bool,
}

impl TrickState {
    pub fn times_used(&self, index: usize) -> u32 {
        self.combo_uses
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, count)| *count)
            .unwrap_or(0)
    }

    pub fn record_use(&mut self, index: usize) {
        if let Some(entry) = self.combo_uses.iter_mut().find(|(i, _)| *i == index) {
            entry.1 = entry.1.saturating_add(1);
        } else {
            self.combo_uses.push((index, 1));
        }
    }

    /// Clear per-combo history — called when a run banks or bails.
    pub fn reset_combo(&mut self) {
        self.combo_uses.clear();
        self.active = None;
        self.revert_window = 0.0;
        self.reverting = false;
    }

    /// True while a landing can still be linked into the running combo.
    pub fn can_revert(&self) -> bool {
        self.revert_window > 0.0
    }
}

fn tricks_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("combat")
        .join("tricks.json")
}

/// Startup: load the authored roster, or write the defaults out so designers
/// have an editable file from the very first run. Mirrors `load_move_library`.
fn load_trick_library(mut commands: Commands) {
    let path = tricks_path();
    let library = match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<TrickLibrary>(&text) {
            Ok(lib) => match lib.validate() {
                Ok(()) => {
                    info!("TrickLibrary: loaded {}", path.display());
                    lib
                }
                Err(err) => {
                    warn!(
                        "TrickLibrary: {} invalid ({err}); using defaults",
                        path.display()
                    );
                    TrickLibrary::defaults()
                }
            },
            Err(err) => {
                warn!(
                    "TrickLibrary: {} unparsable ({err}); using defaults",
                    path.display()
                );
                TrickLibrary::defaults()
            }
        },
        Err(_) => {
            let defaults = TrickLibrary::defaults();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match serde_json::to_string_pretty(&defaults) {
                Ok(json) => match std::fs::write(&path, json) {
                    Ok(()) => info!(
                        "TrickLibrary: wrote editable defaults to {}",
                        path.display()
                    ),
                    Err(err) => warn!("TrickLibrary: could not write defaults ({err})"),
                },
                Err(err) => warn!("TrickLibrary: serialize failed ({err})"),
            }
            defaults
        }
    };
    commands.insert_resource(library);
}

pub struct TrickDataPlugin;

impl Plugin for TrickDataPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_trick_library);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate_and_cover_every_button_direction() {
        let lib = TrickLibrary::defaults();
        lib.validate().expect("defaults must validate");
        // Every basic category/direction the input layer can produce must
        // resolve to something, or a press would silently do nothing.
        for category in [
            TrickCategory::Grab,
            TrickCategory::Flip,
            TrickCategory::Grind,
        ] {
            for direction in [
                TrickDir::Neutral,
                TrickDir::Up,
                TrickDir::Down,
                TrickDir::Left,
                TrickDir::Right,
            ] {
                assert!(
                    lib.resolve(category, direction, 1.0).is_some(),
                    "{category:?} {direction:?} unbound"
                );
            }
        }
    }

    #[test]
    fn specials_shadow_basics_only_once_the_multiplier_is_earned() {
        let lib = TrickLibrary::defaults();
        let low = lib
            .resolve(TrickCategory::Grab, TrickDir::Neutral, 1.0)
            .unwrap();
        assert_eq!(low.name, "Indy");
        assert!(!low.special);

        let high = lib
            .resolve(TrickCategory::Grab, TrickDir::Neutral, 5.0)
            .unwrap();
        assert_eq!(high.name, "The 900");
        assert!(high.special);
        assert!(high.base_score > low.base_score);

        // A direction with no special keeps its basic trick at any multiplier.
        let no_special = lib
            .resolve(TrickCategory::Grab, TrickDir::Down, 8.0)
            .unwrap();
        assert_eq!(no_special.name, "Tailgrab");
    }

    #[test]
    fn stick_direction_classification_has_a_dead_zone() {
        assert_eq!(TrickDir::from_axis(Vec2::ZERO), TrickDir::Neutral);
        assert_eq!(TrickDir::from_axis(Vec2::new(0.2, 0.2)), TrickDir::Neutral);
        assert_eq!(TrickDir::from_axis(Vec2::new(0.0, 1.0)), TrickDir::Up);
        assert_eq!(TrickDir::from_axis(Vec2::new(0.0, -1.0)), TrickDir::Down);
        assert_eq!(TrickDir::from_axis(Vec2::new(1.0, 0.0)), TrickDir::Right);
        assert_eq!(TrickDir::from_axis(Vec2::new(-1.0, 0.0)), TrickDir::Left);
        // Diagonals resolve to the dominant axis.
        assert_eq!(TrickDir::from_axis(Vec2::new(0.9, 0.5)), TrickDir::Right);
        assert_eq!(TrickDir::from_axis(Vec2::new(0.5, 0.9)), TrickDir::Up);
    }

    #[test]
    fn repeat_decay_and_spin_bonus_follow_the_scoring_rules() {
        assert_eq!(TrickLibrary::repeat_divisor(0), 1.0);
        assert!((TrickLibrary::repeat_divisor(1) - 1.33).abs() < 1e-6);
        assert!((TrickLibrary::repeat_divisor(2) - 1.5).abs() < 1e-6);
        assert_eq!(TrickLibrary::repeat_divisor(3), 2.0);
        assert_eq!(TrickLibrary::repeat_divisor(9), 2.0);

        assert_eq!(TrickLibrary::spin_bonus(0.0), 1.0);
        assert_eq!(TrickLibrary::spin_bonus(180.0), 1.5);
        assert_eq!(TrickLibrary::spin_bonus(360.0), 2.0);
        assert_eq!(TrickLibrary::spin_bonus(-360.0), 2.0, "direction agnostic");
        assert_eq!(TrickLibrary::spin_bonus(900.0), 3.5);
    }

    #[test]
    fn revert_window_keeps_a_combo_alive_across_a_landing() {
        let lib = TrickLibrary::defaults();
        assert!(lib.revert_window > 0.0, "reverts must be enabled");

        let mut state = TrickState::default();
        assert!(!state.can_revert(), "no window before a landing");

        // Landing an air trick opens the window.
        state.revert_window = lib.revert_window;
        assert!(state.can_revert());

        // It closes once it runs out…
        state.revert_window = 0.0;
        assert!(!state.can_revert());

        // …and a bail clears everything, so a blown trick cannot be reverted
        // back into a combo.
        state.revert_window = lib.revert_window;
        state.reverting = true;
        state.record_use(2);
        state.reset_combo();
        assert!(!state.can_revert());
        assert!(!state.reverting);
        assert_eq!(state.times_used(2), 0);
    }

    #[test]
    fn trick_state_tracks_per_combo_repeats() {
        let mut state = TrickState::default();
        assert_eq!(state.times_used(3), 0);
        state.record_use(3);
        state.record_use(3);
        state.record_use(7);
        assert_eq!(state.times_used(3), 2);
        assert_eq!(state.times_used(7), 1);
        state.reset_combo();
        assert_eq!(state.times_used(3), 0);
    }

    #[test]
    fn validation_rejects_unreachable_and_broken_tricks() {
        let mut lib = TrickLibrary::defaults();
        lib.tricks[0].base_score = 0.0;
        assert!(lib.validate().is_err());

        // Duplicate binding hides a trick behind another.
        let mut lib = TrickLibrary::defaults();
        let clone = lib.tricks[0].clone();
        lib.tricks.push(clone);
        assert!(lib.validate().is_err());

        // Air tricks need a hold window.
        let mut lib = TrickLibrary::defaults();
        lib.tricks[0].duration = 0.0;
        assert!(lib.validate().is_err());

        let mut lib = TrickLibrary::defaults();
        lib.special_multiplier_threshold = 0.5;
        assert!(lib.validate().is_err());

        let mut lib = TrickLibrary::defaults();
        lib.tricks.clear();
        assert!(lib.validate().is_err());
    }

    #[test]
    fn json_round_trip_preserves_the_roster() {
        let lib = TrickLibrary::defaults();
        let json = serde_json::to_string_pretty(&lib).unwrap();
        let back: TrickLibrary = serde_json::from_str(&json).unwrap();
        back.validate().unwrap();
        assert_eq!(back.tricks.len(), lib.tricks.len());
        assert_eq!(back.tricks[0].name, "Indy");
        assert!(back
            .resolve(TrickCategory::Flip, TrickDir::Right, 1.0)
            .is_some());
    }
}
