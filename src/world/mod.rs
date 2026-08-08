//! World content systems: the simulated life of the setting.
//!
//! These modules own *content rules* rather than engine mechanics — what the
//! economy charges, which raids exist, how the war escalates, what a robot pet
//! does. They are the layer most likely to change for design reasons, which is
//! why they are grouped away from `engine/` and `combat/`.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`missions`] | Mission/objective definitions and completion rules |
//! | [`published_content`] | Loads Designer-published weapons/creatures into play |
//! | [`raids`] | Raid encounters and their progression gates |
//! | [`final_war`] | Late-campaign escalation state |
//! | [`settlement_economy`] | Settlement production, prices, and trade |
//! | [`shop_transactions`] | Per-player shop ownership and purchase rules |
//! | [`robot_pets`] | Companion robots: assembly, roles, behaviour |
//! | [`hacking`] | Hacked-unit conversion and its rules |
//! | [`discussion`] | Dialogue and conversation content |
//!
//! Terrain, buildings, and roads are generated in `plugins/world_plugin`; this
//! folder holds the data and rules those systems consult.

pub mod arcade_dungeon;
pub mod discussion;
pub mod final_war;
pub mod hacking;
pub mod heavy_bio;
pub mod heavy_combat;
pub mod heavy_economy;
pub mod heavy_regions;
pub mod heavy_vehicle;
pub mod heavy_water;
pub mod heavy_world_events;
pub mod missions;
pub mod published_content;
pub mod raids;
pub mod robot_pets;
pub mod settlement_economy;
pub mod shop_transactions;
