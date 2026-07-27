//! Audio: a procedural effects bus plus a separate user music deck.
//!
//! The game ships with no audio asset files. [`synth`] generates every sound
//! effect deterministically as WAV bytes at startup, and [`sfx`] maps gameplay
//! events onto those presets with cooldowns and jitter so combat is never
//! silent and never machine-guns the same sample. File-based sounds can replace
//! any preset handle-for-handle without touching the event mapping.
//!
//! [`player`] is deliberately independent: it is the player's own music deck
//! (MP3 import, playlist, reload), so a user's library can never interfere with
//! gameplay-critical effect playback.

pub mod player;
pub mod sfx;
pub mod synth;
