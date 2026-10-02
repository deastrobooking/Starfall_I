//! Starfall runtime services without Bevy, platform, or game dependencies.
//!
//! Capacity ledgers, preallocated tick timelines, and rolling frame statistics
//! are shared by native gameplay and the minimal framework consumer.

pub mod budget;
pub mod history;
pub mod stats;
