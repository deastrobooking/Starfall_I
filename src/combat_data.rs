//! Data-driven combat frame data (EC2): `MoveDef` + `MoveLibrary`.
//!
//! Every melee move is authored as data — startup/active/recovery windows,
//! cancel timing, damage, knockback, hitstop — instead of hardcoded tuples.
//! On first run the defaults are written to `assets/combat/moves.json`; edit
//! that file and relaunch to retune combat **without recompiling**. Delete it
//! to restore defaults.
//!
//! Times are in seconds (the fixed-tick motor makes them frame-stable at any
//! display rate).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One authored melee move.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MoveDef {
    pub name: String,
    pub damage: f32,
    pub knockback: f32,
    /// Wind-up before the hit lands.
    pub startup: f32,
    /// Hit window (the strike itself).
    pub active: f32,
    /// Follow-through before returning to idle.
    pub recovery: f32,
    /// Seconds into recovery after which a buffered next attack may cancel in.
    pub cancel_after: f32,
    /// Freeze-frame applied on hit (feeds `HitstopState`, still capped there).
    pub hitstop: f32,
}

impl MoveDef {
    pub fn total_duration(&self) -> f32 {
        self.startup + self.active + self.recovery
    }
}

/// Which chain a move belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MeleeChain {
    Light,
    Heavy,
}

/// Execution phase of the move currently being performed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MeleePhase {
    Startup,
    Active,
    Recovery,
}

/// The move a player is currently executing.
#[derive(Clone, Copy, Debug)]
pub struct ActiveMelee {
    pub chain: MeleeChain,
    pub index: usize,
    pub phase: MeleePhase,
    pub timer: f32,
}

/// All authored moves, loaded from `assets/combat/moves.json` (or defaults).
#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct MoveLibrary {
    pub light: Vec<MoveDef>,
    pub heavy: Vec<MoveDef>,
}

impl MoveLibrary {
    pub fn get(&self, chain: MeleeChain, index: usize) -> Option<&MoveDef> {
        match chain {
            MeleeChain::Light => self.light.get(index),
            MeleeChain::Heavy => self.heavy.get(index),
        }
    }

    pub fn chain_len(&self, chain: MeleeChain) -> usize {
        match chain {
            MeleeChain::Light => self.light.len(),
            MeleeChain::Heavy => self.heavy.len(),
        }
    }

    /// The shipping tuning — matches the pre-frame-data damage/knockback and
    /// total durations, now split into authored phases.
    pub fn defaults() -> Self {
        let m = |name: &str,
                 damage: f32,
                 knockback: f32,
                 startup: f32,
                 active: f32,
                 recovery: f32,
                 cancel_after: f32,
                 hitstop: f32| MoveDef {
            name: name.to_string(),
            damage,
            knockback,
            startup,
            active,
            recovery,
            cancel_after,
            hitstop,
        };
        Self {
            light: vec![
                m("Star Jab", 15.0, 3.0, 0.07, 0.10, 0.23, 0.08, 0.030),
                m("Comet Cross", 20.0, 4.0, 0.08, 0.10, 0.27, 0.10, 0.034),
                m("Moon Uppercut", 30.0, 6.0, 0.12, 0.12, 0.36, 0.16, 0.050),
            ],
            heavy: vec![
                m("Meteor Slam", 35.0, 8.0, 0.16, 0.14, 0.40, 0.18, 0.060),
                m("Orbit Sweep", 45.0, 10.0, 0.18, 0.16, 0.46, 0.20, 0.070),
            ],
        }
    }

    /// Reject nonsense that would soft-lock combat (zero-length chains,
    /// non-positive phases, cancel windows outside recovery).
    pub fn validate(&self) -> Result<(), String> {
        if self.light.is_empty() || self.heavy.is_empty() {
            return Err("both light and heavy chains need at least one move".into());
        }
        for def in self.light.iter().chain(self.heavy.iter()) {
            if def.startup <= 0.0 || def.active <= 0.0 || def.recovery <= 0.0 {
                return Err(format!("'{}': all phases must be > 0", def.name));
            }
            if def.cancel_after < 0.0 || def.cancel_after > def.recovery {
                return Err(format!(
                    "'{}': cancel_after must lie within recovery (0..={})",
                    def.name, def.recovery
                ));
            }
            if def.damage <= 0.0 {
                return Err(format!("'{}': damage must be > 0", def.name));
            }
        }
        Ok(())
    }
}

fn moves_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("combat")
        .join("moves.json")
}

/// Startup: load authored frame data, or write the defaults out so designers
/// have an editable file on disk from the very first run.
fn load_move_library(mut commands: Commands) {
    let path = moves_path();
    let library = match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<MoveLibrary>(&text) {
            Ok(lib) => match lib.validate() {
                Ok(()) => {
                    info!("MoveLibrary: loaded {}", path.display());
                    lib
                }
                Err(err) => {
                    warn!(
                        "MoveLibrary: {} invalid ({err}); using defaults",
                        path.display()
                    );
                    MoveLibrary::defaults()
                }
            },
            Err(err) => {
                warn!(
                    "MoveLibrary: {} unparsable ({err}); using defaults",
                    path.display()
                );
                MoveLibrary::defaults()
            }
        },
        Err(_) => {
            let defaults = MoveLibrary::defaults();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match serde_json::to_string_pretty(&defaults) {
                Ok(json) => match std::fs::write(&path, json) {
                    Ok(()) => info!("MoveLibrary: wrote editable defaults to {}", path.display()),
                    Err(err) => warn!("MoveLibrary: could not write defaults ({err})"),
                },
                Err(err) => warn!("MoveLibrary: serialize failed ({err})"),
            }
            defaults
        }
    };
    commands.insert_resource(library);
}

pub struct CombatDataPlugin;

impl Plugin for CombatDataPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_move_library);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_match_legacy_totals() {
        let lib = MoveLibrary::defaults();
        lib.validate().expect("defaults must validate");
        // Legacy total durations preserved so combat pacing is unchanged.
        let totals: Vec<f32> = lib.light.iter().map(|m| m.total_duration()).collect();
        for (total, legacy) in totals.iter().zip([0.4_f32, 0.45, 0.6]) {
            assert!((total - legacy).abs() < 1e-4, "light totals drifted");
        }
        assert!((lib.heavy[0].total_duration() - 0.7).abs() < 1e-4);
        assert!((lib.heavy[1].total_duration() - 0.8).abs() < 1e-4);
    }

    #[test]
    fn json_round_trip_preserves_frame_data() {
        let lib = MoveLibrary::defaults();
        let json = serde_json::to_string_pretty(&lib).unwrap();
        let back: MoveLibrary = serde_json::from_str(&json).unwrap();
        back.validate().unwrap();
        assert_eq!(back.light.len(), lib.light.len());
        assert_eq!(back.light[2].name, "Moon Uppercut");
        assert!((back.heavy[1].cancel_after - 0.20).abs() < 1e-6);
    }

    #[test]
    fn validation_rejects_softlock_data() {
        let mut lib = MoveLibrary::defaults();
        lib.light[0].startup = 0.0;
        assert!(lib.validate().is_err());

        let mut lib = MoveLibrary::defaults();
        lib.heavy[0].cancel_after = 99.0;
        assert!(lib.validate().is_err());

        let mut lib = MoveLibrary::defaults();
        lib.light.clear();
        assert!(lib.validate().is_err());
    }
}
