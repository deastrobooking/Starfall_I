//! Engine infrastructure: the layer that knows *how* the game runs, never
//! *what* the game is about.
//!
//! Nothing in here encodes a game rule. These modules provide the simulation
//! frame every gameplay system is written against — the fixed-tick loop and
//! system ordering, the physics backend shim, deterministic randomness, render
//! material plumbing, and platform paths. A change to a weapon, enemy, or
//! chapter should never require touching this folder; a change to Bevy, Avian,
//! or the tick rate should mostly be contained by it.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`game_loop`] | `GameSet` ordering, fixed-tick config, perf overlay |
//! | [`physics`] | Avian compatibility shim (`Collider`, controller, layers) |
//! | [`input_buffer`] | Per-player edge latch so fixed ticks never drop presses |
//! | [`rendering`] | Shared bundles and custom materials |
//! | [`spatial_lod`] | Distance-based detail management |
//! | [`game_rng`] | Seeded RNG streams (replay/determinism seam) |
//! | [`state`] | `AppState` — the top-level screen/mode machine |
//! | [`platform_paths`] | OS-specific save/config/crash locations |

pub mod game_loop;
pub mod game_rng;
pub mod input_buffer;
pub mod physics;
pub mod platform_paths;
pub mod rendering;
pub mod spatial_lod;
pub mod state;
