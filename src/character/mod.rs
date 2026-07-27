//! Character construction: how a body is described, assembled, and deformed.
//!
//! This folder owns the pipeline that turns an authored recipe into a spawned,
//! poseable figure. It is deliberately separate from `plugins/character_plugin`
//! (which *animates* characters at runtime) and from `character_studio` (the
//! authoring tool): this is the shared vocabulary both of those depend on.
//!
//! The pipeline reads top to bottom:
//!
//! 1. [`blueprint`] — authored description of a body (proportions, appearance).
//! 2. [`presets`] — the shipped hero/NPC blueprints built from that vocabulary.
//! 3. [`parts`] — the part catalog and slot system every figure is assembled from.
//! 4. [`modular`] — assembles a recipe into a spawned entity hierarchy.
//! 5. [`player_mesh`] — the player-specific assembly entry point.
//! 6. [`mesh_modifiers`] / [`procedural_meshes`] — mesh generation and deformation.
//! 7. [`hero_roster`] — per-hero gameplay stat/loadout profiles applied on spawn.
//! 8. [`animation_mvp`] — the baseline clip set characters ship with.
//!
//! [`face`] is the anime-RPG face system: eye styles, expressions, and the
//! layered eye geometry that [`parts`] turns into meshes.

pub mod animation_mvp;
pub mod blueprint;
pub mod face;
pub mod hero_roster;
pub mod mesh_modifiers;
pub mod modular;
pub mod parts;
pub mod player_mesh;
pub mod presets;
pub mod procedural_meshes;
