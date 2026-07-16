//! Data-driven combat frame data (EC2): `MoveDef` + `MoveLibrary`.
//!
//! Every melee move is authored as data — startup/active/recovery windows,
//! cancel timing, damage, knockback, hitstop — instead of hardcoded tuples.
//! The Star Sabre slash chain uses the same `MoveDef` shape, and the six
//! primary ranged weapons plus the sabre's energy wave use `RangedMoveDef`
//! (fire cycle + projectile flight parameters instead of melee reach).
//! On first run the defaults are written to `assets/combat/moves.json`; edit
//! that file and relaunch to retune combat **without recompiling**. Delete it
//! to restore defaults. Files written before the sabre/ranged sections
//! existed still load — missing sections fall back to the shipped defaults.
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

/// One authored ranged attack: the `MoveDef` idea adapted to projectile
/// weapons. Startup/active/recovery collapse into the shot cooldown
/// (`fire_rate`), and projectile flight parameters replace melee reach.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RangedMoveDef {
    pub name: String,
    /// Base damage per projectile (before rank/perk/armor multipliers).
    pub damage: f32,
    /// Seconds between shots — the ranged analog of `total_duration()`.
    pub fire_rate: f32,
    /// Muzzle speed in units/sec.
    pub projectile_speed: f32,
    /// Random cone half-width applied per pellet.
    pub spread: f32,
    /// Projectiles per trigger pull.
    pub pellets: u32,
    /// Seconds a projectile lives before despawning.
    pub projectile_lifetime: f32,
    /// Splash radius on impact (0 = no splash).
    pub explosion_radius: f32,
}

/// Number of primary weapon slots; `MoveLibrary::ranged` is indexed by
/// `WeaponInventory.active_slot` (0 = Pistol … 5 = Grenade), the same
/// convention as `WeaponRanks.ranks`.
pub const RANGED_SLOTS: usize = 6;

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
///
/// The sabre/ranged sections carry `serde(default)` so libraries authored
/// before those sections existed keep loading (they get the shipped
/// defaults for the missing parts).
#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct MoveLibrary {
    pub light: Vec<MoveDef>,
    pub heavy: Vec<MoveDef>,
    /// Star Sabre slash chain. The sabre repeats through this chain
    /// (clamped to the last entry) for each slash in its level-scaled
    /// slash count; `total_duration()` is the cadence between slashes.
    #[serde(default = "default_sabre_chain")]
    pub sabre: Vec<MoveDef>,
    /// The energy wave the Star Sabre launches (level 4+ dual wave, and
    /// every slash inside dungeons). `fire_rate` mirrors the level-1 sabre
    /// cooldown for reference; the live cooldown scales with sabre level.
    #[serde(default = "default_sabre_wave")]
    pub sabre_wave: RangedMoveDef,
    /// Primary ranged weapons, indexed by inventory slot (see [`RANGED_SLOTS`]).
    #[serde(default = "default_ranged_slots")]
    pub ranged: Vec<RangedMoveDef>,
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

    /// The sabre move for a given slash of the combo, clamped to the last
    /// authored entry so a short chain still covers high-level slash counts.
    pub fn sabre_slash(&self, slash_index: usize) -> Option<&MoveDef> {
        self.sabre
            .get(slash_index.min(self.sabre.len().saturating_sub(1)))
    }

    /// The ranged move for a primary weapon slot (0 = Pistol … 5 = Grenade).
    pub fn ranged_slot(&self, slot: usize) -> Option<&RangedMoveDef> {
        self.ranged.get(slot)
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
            sabre: default_sabre_chain(),
            sabre_wave: default_sabre_wave(),
            ranged: default_ranged_slots(),
        }
    }

    /// Reject nonsense that would soft-lock combat (zero-length chains,
    /// non-positive phases, cancel windows outside recovery, dead weapons).
    pub fn validate(&self) -> Result<(), String> {
        if self.light.is_empty() || self.heavy.is_empty() {
            return Err("both light and heavy chains need at least one move".into());
        }
        if self.sabre.is_empty() {
            return Err("the sabre chain needs at least one move".into());
        }
        for def in self
            .light
            .iter()
            .chain(self.heavy.iter())
            .chain(self.sabre.iter())
        {
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
        if self.ranged.len() != RANGED_SLOTS {
            return Err(format!(
                "ranged must define exactly {RANGED_SLOTS} weapon slots (found {})",
                self.ranged.len()
            ));
        }
        for def in self.ranged.iter().chain(std::iter::once(&self.sabre_wave)) {
            if def.damage <= 0.0 {
                return Err(format!("'{}': damage must be > 0", def.name));
            }
            if def.fire_rate <= 0.0 {
                return Err(format!("'{}': fire_rate must be > 0", def.name));
            }
            if def.projectile_speed <= 0.0 || def.projectile_lifetime <= 0.0 {
                return Err(format!(
                    "'{}': projectile speed and lifetime must be > 0",
                    def.name
                ));
            }
            if def.pellets == 0 {
                return Err(format!("'{}': pellets must be >= 1", def.name));
            }
            if def.spread < 0.0 || def.explosion_radius < 0.0 {
                return Err(format!(
                    "'{}': spread and explosion_radius must be >= 0",
                    def.name
                ));
            }
        }
        Ok(())
    }
}

/// Star Sabre slash chain — the legacy hardcoded cadence (0.25 s between
/// slashes) split into authored phases, with the level-1 damage as the base
/// (`BeamSabre::set_level` scaling multiplies on top) and the legacy
/// knockback of 3.0. Hitstop ships at 0 to preserve the sabre's current feel;
/// raise it in `moves.json` to give slashes melee-style freeze frames.
fn default_sabre_chain() -> Vec<MoveDef> {
    vec![MoveDef {
        name: "Star Slash".to_string(),
        damage: 25.0,
        knockback: 3.0,
        startup: 0.05,
        active: 0.08,
        recovery: 0.12,
        cancel_after: 0.0,
        hitstop: 0.0,
    }]
}

/// Star Sabre energy wave — legacy hardcoded projectile (speed 20, 1.5 s
/// lifetime, level-1 wave damage 40, splash 4.0 once the level-5 AoE
/// unlocks). `fire_rate` mirrors the level-1 sabre cooldown.
fn default_sabre_wave() -> RangedMoveDef {
    RangedMoveDef {
        name: "Star Wave".to_string(),
        damage: 40.0,
        fire_rate: 0.8,
        projectile_speed: 20.0,
        spread: 0.0,
        pellets: 1,
        projectile_lifetime: 1.5,
        explosion_radius: 4.0,
    }
}

/// The six primary weapons in inventory-slot order. Numbers are the shipping
/// `Weapon::new` tuning, now authored as data (3.0 s was the hardcoded
/// projectile lifetime shared by every primary shot).
fn default_ranged_slots() -> Vec<RangedMoveDef> {
    let r = |name: &str,
             damage: f32,
             fire_rate: f32,
             projectile_speed: f32,
             spread: f32,
             pellets: u32,
             explosion_radius: f32| RangedMoveDef {
        name: name.to_string(),
        damage,
        fire_rate,
        projectile_speed,
        spread,
        pellets,
        projectile_lifetime: 3.0,
        explosion_radius,
    };
    vec![
        r("Starlight Popper", 16.0, 0.26, 68.0, 0.02, 1, 0.0),
        r("Comet Stream", 18.0, 0.075, 105.0, 0.025, 1, 0.0),
        r("Sparkle Fan", 7.0, 0.7, 80.0, 0.18, 10, 0.0),
        r("Nova Orb", 90.0, 1.2, 34.0, 0.0, 1, 6.5),
        r("Rainbow Ray", 26.0, 0.045, 320.0, 0.0, 1, 0.0),
        r("Star Bubble Bombs", 75.0, 0.95, 16.0, 0.0, 1, 8.5),
    ]
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
    fn sabre_defaults_match_legacy_hardcoded_values() {
        let lib = MoveLibrary::defaults();
        // Legacy inter-slash cadence was a hardcoded 0.25 s.
        let slash = lib.sabre_slash(0).expect("sabre chain must exist");
        assert!((slash.total_duration() - 0.25).abs() < 1e-4);
        assert!((slash.damage - 25.0).abs() < 1e-6, "level-1 slash base");
        assert!((slash.knockback - 3.0).abs() < 1e-6, "legacy knockback");
        assert_eq!(slash.hitstop, 0.0, "sabre shipped without hitstop");
        // Clamping: high slash indices reuse the last authored entry.
        assert_eq!(
            lib.sabre_slash(7).unwrap().name,
            lib.sabre.last().unwrap().name
        );
        // Legacy wave projectile: speed 20, lifetime 1.5, base damage 40, AoE 4.
        let wave = &lib.sabre_wave;
        assert!((wave.projectile_speed - 20.0).abs() < 1e-6);
        assert!((wave.projectile_lifetime - 1.5).abs() < 1e-6);
        assert!((wave.damage - 40.0).abs() < 1e-6);
        assert!((wave.explosion_radius - 4.0).abs() < 1e-6);
    }

    #[test]
    fn ranged_defaults_match_legacy_weapon_tuning() {
        let lib = MoveLibrary::defaults();
        assert_eq!(lib.ranged.len(), RANGED_SLOTS);
        // Spot-check slots against the shipping Weapon::new numbers
        // (slot order: Pistol, Rifle, Shotgun, Rocket, Laser, Grenade).
        let pistol = lib.ranged_slot(0).unwrap();
        assert!((pistol.damage - 16.0).abs() < 1e-6);
        assert!((pistol.fire_rate - 0.26).abs() < 1e-6);
        assert!((pistol.projectile_speed - 68.0).abs() < 1e-6);
        let shotgun = lib.ranged_slot(2).unwrap();
        assert_eq!(shotgun.pellets, 10);
        assert!((shotgun.spread - 0.18).abs() < 1e-6);
        let rocket = lib.ranged_slot(3).unwrap();
        assert!((rocket.explosion_radius - 6.5).abs() < 1e-6);
        let grenade = lib.ranged_slot(5).unwrap();
        assert!((grenade.fire_rate - 0.95).abs() < 1e-6);
        assert!((grenade.damage - 75.0).abs() < 1e-6);
        // Every primary shot used a hardcoded 3.0 s lifetime.
        for def in &lib.ranged {
            assert!((def.projectile_lifetime - 3.0).abs() < 1e-6);
        }
        assert!(lib.ranged_slot(6).is_none());
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
        assert_eq!(back.sabre[0].name, "Star Slash");
        assert_eq!(back.sabre_wave.name, "Star Wave");
        assert_eq!(back.ranged.len(), RANGED_SLOTS);
        assert!((back.ranged[4].projectile_speed - 320.0).abs() < 1e-6);
    }

    #[test]
    fn legacy_json_without_sabre_or_ranged_sections_still_loads() {
        // Files written before the sabre/ranged sections existed (like the
        // moves.json already on players' disks) must keep loading, with the
        // missing sections filled from defaults.
        let lib = MoveLibrary::defaults();
        let legacy = serde_json::json!({
            "light": serde_json::to_value(&lib.light).unwrap(),
            "heavy": serde_json::to_value(&lib.heavy).unwrap(),
        });
        let back: MoveLibrary = serde_json::from_value(legacy).unwrap();
        back.validate()
            .expect("legacy file plus defaults must validate");
        assert_eq!(back.sabre.len(), lib.sabre.len());
        assert_eq!(back.ranged.len(), RANGED_SLOTS);
        assert!((back.sabre_wave.projectile_speed - 20.0).abs() < 1e-6);
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

        let mut lib = MoveLibrary::defaults();
        lib.sabre.clear();
        assert!(lib.validate().is_err());
    }

    #[test]
    fn validation_rejects_dead_ranged_weapons() {
        let mut lib = MoveLibrary::defaults();
        lib.ranged[0].fire_rate = 0.0;
        assert!(lib.validate().is_err());

        let mut lib = MoveLibrary::defaults();
        lib.ranged[2].pellets = 0;
        assert!(lib.validate().is_err());

        let mut lib = MoveLibrary::defaults();
        lib.ranged.pop();
        assert!(lib.validate().is_err(), "must define every weapon slot");

        let mut lib = MoveLibrary::defaults();
        lib.sabre_wave.projectile_lifetime = 0.0;
        assert!(lib.validate().is_err());

        let mut lib = MoveLibrary::defaults();
        lib.ranged[1].spread = -0.1;
        assert!(lib.validate().is_err());
    }
}
