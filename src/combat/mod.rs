//! Combat rules: frame data, damage resolution, feedback, and progression.
//!
//! The EC2 principle behind this folder is that *what a move does* is data, and
//! *how a hit resolves* is one shared path. Systems in `plugins/` read these
//! definitions; they do not invent their own damage math.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`data`] | `MoveDef`/`MoveLibrary` — authored frame data (`assets/combat/moves.json`) |
//! | [`blades`] | Star Sabre blade catalog — the shop's weapons, and what they change |
//! | [`damage`] | The single `apply_damage` path: resistances, crits, knockback |
//! | [`hitstop`] | Bounded freeze-frames shared by every impact |
//! | [`feedback`] | Flinch, hit flashes, damage numbers, death dissolve |
//! | [`tricks`] | Hoverboard trick roster (`assets/combat/tricks.json`) |
//! | [`upgrades`] | `UpgradeLedger` — save-backed relics, gems, tech ranks |
//! | [`perks`] | The perk tree and its stat modifiers |
//!
//! Progression lives here rather than in `world/` because upgrades and perks
//! almost exclusively modify combat numbers and unlock combat verbs.

pub mod blades;
pub mod damage;
pub mod data;
pub mod feedback;
pub mod hitstop;
pub mod perks;
pub mod tricks;
pub mod upgrades;
