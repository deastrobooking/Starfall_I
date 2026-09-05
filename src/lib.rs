//! Starfall Engine and the native Heavy Water demo application.
//!
//! The minimal library exposes Starfall's graph, project, and platformer
//! contracts without rendering, physics, or native audio dependencies. The
//! `heavy-water-demo` feature adds the complete current runtime and application;
//! `designer` adds Forge and implies the demo while the finer engine/gameplay
//! capability split is developed.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

/// Typed graph contracts supplied by the independent `starfall-graph` crate.
///
/// The re-export preserves the convenient `starfall_i::graph` facade while
/// allowing graph tools and native extensions to depend on the smaller crate.
pub use starfall_graph as graph;
/// Renderer-neutral platformer prefab, envelope, chunk, and route contracts.
pub use starfall_platformer as platformer;
/// Platformer route graph nodes, documents, and compiler adapters.
pub use starfall_platformer_graph as platformer_graph;
/// Versioned project and module manifest contracts.
pub use starfall_project as project;

#[cfg(feature = "heavy-water-demo")]
pub mod app;
#[cfg(feature = "heavy-water-demo")]
pub mod audio;
#[cfg(feature = "heavy-water-demo")]
pub mod chapters;
#[cfg(feature = "heavy-water-demo")]
pub mod character;
#[cfg(feature = "heavy-water-demo")]
pub mod character_studio;
#[cfg(feature = "heavy-water-demo")]
pub mod combat;
#[cfg(feature = "heavy-water-demo")]
pub mod commands;
#[cfg(feature = "heavy-water-demo")]
pub mod components;
#[cfg(feature = "heavy-water-demo")]
pub mod engine;
#[cfg(feature = "heavy-water-demo")]
pub mod engine_tools;
#[cfg(feature = "heavy-water-demo")]
pub mod events;
#[cfg(feature = "heavy-water-demo")]
pub mod framework;
#[cfg(feature = "heavy-water-demo")]
pub mod heavy_water;
#[cfg(feature = "heavy-water-demo")]
pub mod lsystem;
#[cfg(feature = "heavy-water-demo")]
pub mod plugins;
#[cfg(feature = "heavy-water-demo")]
pub mod resources;
#[cfg(feature = "heavy-water-demo")]
pub mod robots;
#[cfg(feature = "heavy-water-demo")]
pub mod spaceship_forge;
#[cfg(feature = "heavy-water-demo")]
pub mod vehicle_forge;
#[cfg(feature = "heavy-water-demo")]
pub mod world;

#[cfg(feature = "heavy-water-demo")]
pub use app::{build_app, build_headless_app, build_starfall_app, StarfallAppMode};

/// Common imports for the selected framework capabilities.
///
/// The prelude stays intentionally small. Domain-specific types should be
/// imported from their owning modules so feature dependencies remain visible.
pub mod prelude {
    pub use crate::graph::{GraphRegistryPlugin, NodeRegistry};
    pub use crate::platformer::{ChunkDef, JumpEnvelope, MovementProfile, PlatformerPrefabKind};
    pub use crate::platformer_graph::{
        compile_platformer_graph, register_platformer_nodes, PlatformerRouteDocument,
        PlatformerRouteGraph, PlatformerRouteGraphBuilder,
    };
    pub use crate::project::{ModuleManifest, ProjectManifest};

    #[cfg(feature = "heavy-water-demo")]
    pub use crate::app::{build_app, build_headless_app, build_starfall_app, StarfallAppMode};
    #[cfg(feature = "heavy-water-demo")]
    pub use crate::engine::game_loop::{GameLoopPlugin, GameSet};
    #[cfg(feature = "heavy-water-demo")]
    pub use crate::engine::physics::PhysicsCompatPlugin;
    #[cfg(feature = "heavy-water-demo")]
    pub use crate::engine::state::AppState;
    #[cfg(feature = "heavy-water-demo")]
    pub use crate::events::EventsPlugin;
    #[cfg(feature = "designer")]
    pub use crate::framework::StarfallForgePlugins;
    #[cfg(feature = "heavy-water-demo")]
    pub use crate::framework::{HeavyWaterDemoPlugins, StarfallFoundationPlugins};
}
