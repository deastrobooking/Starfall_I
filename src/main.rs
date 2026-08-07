// Bevy systems naturally use broad query/resource signatures. Keep clippy strict
// for correctness-oriented lints while allowing those ECS-heavy shapes.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::engine::physics::prelude::*;
use bevy::asset::AssetApp;
use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::window::WindowResolution;
use bevy::world_serialization::WorldSerializationPlugin;

// ── Cross-cutting shared vocabulary (used by every layer) ───────────────
mod commands;
mod components;
mod events;
mod resources;

// ── Domain folders (see each mod.rs for what belongs where) ─────────────
mod audio;
mod chapters;
mod character;
mod character_studio;
mod combat;
mod engine;
mod engine_tools;
mod lsystem;
mod plugins;
mod robots;
mod world;

use character::blueprint::{BodyRecipe, CartoonAppearanceRecipe};
use character::modular::ModularCharacterPlugin;
use character::parts::{
    ArmPreset, BodyPreset, CharacterLoadout, HeadPreset, LegPreset, ShoulderPreset,
};
use combat::perks::PerkTree;
use combat::upgrades::UpgradeLedger;
use commands::{CommandOverlayState, CommandRegistry};
use engine::state::AppState;
use engine_tools::{EngineToolMode, EngineToolsPlugin};
use events::EventsPlugin;
use plugins::{
    ArmorPlugin, ChapterPlugin, CharacterDesignPlugin, CharacterPlugin, ChestPlugin,
    CompanionPlugin, CraftingPlugin, DiscoverablePlugin, EnemyPlugin, HackingPlugin,
    HeavyBioPlugin, HeavyCombatPlugin, HeavyEconomyPlugin, HeavyRegionsPlugin,
    ImportedCharacterForgePlugin, InputPlugin, PlayerPlugin, RadioPlugin, RobotGaragePlugin,
    SavePlugin, UiPlugin, VehiclePlugin, WeaponPlugin, WorldPlugin,
};
use resources::{
    CharacterBaseModel, CharacterBaseModelCatalog, CharacterDesignData, CharacterDesignSnapshot,
    GameSettings, LocalPlayerConfig, PlaySessionTransition, PlayerPartLoadout, PlayerScore,
    PlayerSelectState, ShopCatalog, WaveInfo, WorldRouteRegistry, WorldSiteRegistry,
};
use world::final_war::FinalWarRegistry;
use world::hacking::HackingRegistry;
use world::raids::RaidRegistry;
use world::robot_pets::RobotPetCollection;
use world::settlement_economy::SettlementEconomy;

fn main() {
    install_crash_logger();

    let mut app = build_app();
    apply_boot_overrides(&mut app);
    app.run();
}

/// Constructs the complete production app without starting its runner.
///
/// Keeping construction separate from `main` gives smoke tests and future
/// platform launchers one authoritative plugin/resource registration boundary.
pub fn build_app() -> App {
    build_starfall_app(StarfallAppMode::Production)
}

/// Constructs the complete game without presentation backends.
///
/// The returned app uses the same state/resource/plugin registration as
/// production and can execute Startup plus ordinary schedule frames in tests,
/// CI, extraction harnesses, and future server-side tooling.
pub fn build_headless_app() -> App {
    build_starfall_app(StarfallAppMode::Headless)
}

/// Platform profile for the authoritative Starfall app factory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarfallAppMode {
    Production,
    Headless,
}

/// Single construction boundary shared by the executable and headless tests.
pub fn build_starfall_app(mode: StarfallAppMode) -> App {
    let mut app = App::new();

    match mode {
        StarfallAppMode::Production => {
            app.add_plugins(
                DefaultPlugins
                    .set(WindowPlugin {
                        primary_window: Some(Window {
                            title: "Starfall I".to_string(),
                            resolution: WindowResolution::new(1280, 720),
                            ..default()
                        }),
                        ..default()
                    })
                    .set(ImagePlugin::default_nearest())
                    .set(production_asset_plugin()),
            );
        }
        StarfallAppMode::Headless => {
            app.add_plugins((
                MinimalPlugins,
                bevy::diagnostic::DiagnosticsPlugin,
                bevy::transform::TransformPlugin,
                bevy::input::InputPlugin,
                StatesPlugin,
                production_asset_plugin(),
                bevy::mesh::MeshPlugin,
                WorldSerializationPlugin,
                bevy::scene::ScenePlugin,
                bevy::gizmos::GizmoPlugin,
            ));
            register_headless_asset_stores(&mut app);
        }
    }

    // The authored world routinely has enough visible lights to exceed Bevy's
    // default 1,024-entry GPU Z-slice list. Reserve the observed requirement up
    // front so lighting is not corrupted for a few frames while it resizes.
    if mode == StarfallAppMode::Production {
        if let Some(mut cluster_settings) =
            app.world_mut()
                .get_resource_mut::<bevy::light::cluster::GlobalClusterSettings>()
        {
            if let Some(gpu_settings) = cluster_settings.gpu_clustering.as_mut() {
                gpu_settings.initial_z_slice_list_capacity =
                    gpu_settings.initial_z_slice_list_capacity.max(2_048);
            }
        }
    }

    app.add_plugins(PhysicsPlugins::default())
        .add_plugins(PhysicsCompatPlugin);
    configure_starfall_app(&mut app, mode == StarfallAppMode::Production);
    app
}

fn production_asset_plugin() -> AssetPlugin {
    AssetPlugin {
        file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
        ..default()
    }
}

/// Startup systems allocate gameplay assets even when no renderer or audio
/// device exists. Register only their CPU-side stores so the exact production
/// plugins can execute headlessly without a window, GPU, or sound backend.
fn register_headless_asset_stores(app: &mut App) {
    app.init_asset::<StandardMaterial>()
        .init_asset::<AudioSource>()
        .init_asset::<AnimationClip>()
        .init_asset::<AnimationGraph>()
        .init_asset::<engine::rendering::ToonMaterial>()
        .init_asset::<engine::rendering::WaterMaterial>()
        .init_asset::<engine::rendering::EnergyMaterial>()
        .init_asset::<engine::rendering::ShieldMaterial>()
        .init_asset::<engine::rendering::IceMaterial>()
        .init_asset::<engine::rendering::LavaMaterial>();
}

/// Registers Starfall's state, resources, and game plugins independently from
/// the platform/window plugin group. Tests use the same registration boundary
/// without constructing macOS' process-global event loop.
fn configure_starfall_app(app: &mut App, add_render_materials: bool) {
    app.init_state::<AppState>()
        // Global resources
        .init_resource::<WaveInfo>()
        .init_resource::<GameSettings>()
        .init_resource::<resources::UiGameplayCapture>()
        .init_resource::<PlayerScore>()
        .init_resource::<PlaySessionTransition>()
        .init_resource::<LocalPlayerConfig>()
        .init_resource::<PlayerSelectState>()
        .init_resource::<CharacterDesignData>()
        .init_resource::<CharacterBaseModelCatalog>()
        .init_resource::<PerkTree>()
        .init_resource::<RobotPetCollection>()
        .init_resource::<SettlementEconomy>()
        .init_resource::<UpgradeLedger>()
        .init_resource::<PlayerPartLoadout>()
        .init_resource::<ShopCatalog>()
        .init_resource::<WorldSiteRegistry>()
        .init_resource::<WorldRouteRegistry>()
        .init_resource::<RaidRegistry>()
        .init_resource::<CommandRegistry>()
        .init_resource::<CommandOverlayState>()
        .init_resource::<HackingRegistry>()
        .init_resource::<FinalWarRegistry>()
        .init_resource::<engine::game_rng::GameRng>()
        .register_type::<BodyRecipe>()
        .register_type::<CartoonAppearanceRecipe>()
        .register_type::<BodyPreset>()
        .register_type::<ArmPreset>()
        .register_type::<LegPreset>()
        .register_type::<ShoulderPreset>()
        .register_type::<HeadPreset>()
        .register_type::<CharacterLoadout>()
        .register_type::<CharacterBaseModel>()
        .register_type::<PlayerPartLoadout>()
        .register_type::<CharacterDesignSnapshot>()
        // Event infrastructure
        .add_plugins(EventsPlugin)
        // Shared recipe-editor services: tool mode, stable IDs, selection,
        // and transactional undo/redo. Player-facing workspace UI lands ET2.
        .add_plugins(EngineToolsPlugin)
        // N11a: camera-aware hierarchical distance culling for world assets.
        .add_plugins(engine::spatial_lod::SpatialLodPlugin)
        // Movable/minimizable tool windows shared by creator screens.
        .add_plugins(engine_tools::tool_windows::ToolWindowsPlugin)
        // EC0: engine-core loop scaffolding (GameSet ordering + profiling overlay).
        .add_plugins(engine::game_loop::GameLoopPlugin)
        // EC2: bounded hitstop + hit-reaction feedback (flinch, flashes,
        // death dissolve, damage numbers, outgoing shake/rumble).
        .add_plugins(combat::hitstop::HitstopPlugin)
        .add_plugins(combat::data::CombatDataPlugin)
        // Hoverboard trick roster (data-driven, assets/combat/tricks.json).
        .add_plugins(combat::tricks::TrickDataPlugin)
        .add_plugins(combat::feedback::CombatFeedbackPlugin)
        // S3: procedural retro SFX bus (zero external assets).
        .add_plugins(audio::sfx::SfxPlugin)
        // Separate user music deck + MP3 import/reload tool. Arcade SFX remain
        // active through SfxPlugin and can be overridden action by action.
        .add_plugins(audio::player::MusicPlayerPlugin)
        // EC1: deterministic per-player input buffer (additive; consumed by motor in EC1b).
        .add_plugins(engine::input_buffer::InputBufferPlugin);

    if add_render_materials {
        app.add_plugins(MaterialPlugin::<engine::rendering::ToonMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::WaterMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::EnergyMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::ShieldMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::IceMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::LavaMaterial>::default());
    }

    app.add_plugins((
        InputPlugin,
        UiPlugin,
        WorldPlugin,
        PlayerPlugin,
        CharacterPlugin,
        CharacterDesignPlugin,
        WeaponPlugin,
        EnemyPlugin,
        HackingPlugin,
        ChestPlugin,
        CompanionPlugin,
        ArmorPlugin,
        CraftingPlugin,
        (
            world::missions::MissionPlugin,
            world::published_content::PublishedContentPlugin,
        ),
        SavePlugin,
    ))
    .add_plugins((
        HeavyEconomyPlugin,
        HeavyBioPlugin,
        HeavyCombatPlugin,
        HeavyRegionsPlugin,
    ))
    .add_plugins((
        ChapterPlugin,
        DiscoverablePlugin,
        RadioPlugin,
        VehiclePlugin,
        RobotGaragePlugin,
        ModularCharacterPlugin,
        character_studio::CharacterStudioPlugin,
        ImportedCharacterForgePlugin,
    ));

    // The authoring screens are the Designer edition (docs/PROJECT_PLAN.md).
    // A consumer build compiles them out entirely; their AppState variants
    // remain but nothing can enter them.
    #[cfg(feature = "designer")]
    app.add_plugins((plugins::CreatureForgePlugin, plugins::WeaponForgePlugin));
}

fn apply_boot_overrides(app: &mut App) {
    if std::env::var_os("STARFALL_AUTOSTART").is_some() {
        app.insert_state(AppState::Playing);
    }
    // Boot straight into the Character Studio for design-tool iteration.
    if std::env::var_os("STARFALL_STUDIO").is_some() {
        app.insert_state(AppState::CharacterStudio);
    }
    // Boot straight into the modular Weapon Forge for tool iteration.
    if cfg!(feature = "designer") && std::env::var_os("STARFALL_WEAPON_FORGE").is_some() {
        app.insert_state(AppState::WeaponForge);
    }
    // Boot directly into a live chapter with Starfall Forge open for editor
    // smoke tests and rapid tool iteration.
    if cfg!(feature = "designer") && std::env::var_os("STARFALL_EDITOR").is_some() {
        app.insert_state(AppState::Playing);
        app.insert_state(EngineToolMode::Editing);
    }
}

fn install_crash_logger() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let log = format!("Starfall I crash\n\n{panic_info}\n\nBacktrace:\n{backtrace}\n");
        let _ = engine::platform_paths::write_crash_report(&log);
        default_hook(panic_info);
    }));
}

#[cfg(test)]
mod app_smoke_tests {
    use super::*;
    use bevy::ecs::message::Messages;

    #[test]
    fn app_constructs_with_core_state_resources_messages_and_plugins() {
        let app = build_headless_app();
        let world = app.world();

        assert_eq!(
            world.resource::<State<AppState>>().get(),
            &AppState::MainMenu
        );
        assert!(world.contains_resource::<LocalPlayerConfig>());
        assert!(world.contains_resource::<PlayerSelectState>());
        assert!(world.contains_resource::<GameSettings>());
        assert!(world.contains_resource::<world::heavy_water::HeavyWaterProgress>());
        assert!(world.contains_resource::<Messages<events::UiMessageEvent>>());
        assert!(world.contains_resource::<Messages<events::PlayerDamagedEvent>>());

        assert!(app.is_plugin_added::<engine::game_loop::GameLoopPlugin>());
        assert!(app.is_plugin_added::<EventsPlugin>());
        assert!(app.is_plugin_added::<InputPlugin>());
        assert!(app.is_plugin_added::<UiPlugin>());
        assert!(app.is_plugin_added::<PlayerPlugin>());
        assert!(app.is_plugin_added::<WeaponPlugin>());
        assert!(app.is_plugin_added::<WorldPlugin>());
        assert!(app.is_plugin_added::<SavePlugin>());
        assert!(app.is_plugin_added::<HeavyEconomyPlugin>());
        assert!(app.is_plugin_added::<HeavyBioPlugin>());
        assert!(app.is_plugin_added::<HeavyCombatPlugin>());
        assert!(app.is_plugin_added::<HeavyRegionsPlugin>());
    }

    #[test]
    fn headless_factory_omits_process_global_presentation_backends() {
        let app = build_headless_app();

        assert!(app.is_plugin_added::<bevy::input::InputPlugin>());
        assert!(app.is_plugin_added::<AssetPlugin>());
        assert!(app.is_plugin_added::<WorldSerializationPlugin>());
        assert!(app.is_plugin_added::<PhysicsCompatPlugin>());
        assert!(!app.is_plugin_added::<WindowPlugin>());
        assert!(!app.is_plugin_added::<bevy::render::RenderPlugin>());
    }

    #[test]
    fn headless_app_executes_startup_and_a_steady_frame() {
        let mut app = build_headless_app();

        // `App::run` finalizes plugins automatically; direct-update harnesses
        // must do the same before executing schedules.
        assert!(app.world().contains_resource::<AssetServer>());
        app.finish();
        assert!(app.world().contains_resource::<AssetServer>());
        app.cleanup();
        assert!(app.world().contains_resource::<AssetServer>());
        app.update();

        let world = app.world_mut();
        assert_eq!(
            world.resource::<State<AppState>>().get(),
            &AppState::MainMenu
        );
        assert!(!world.resource::<Assets<Mesh>>().is_empty());
        assert!(!world.resource::<Assets<StandardMaterial>>().is_empty());
        assert!(world.resource::<Assets<AudioSource>>().len() >= 10);
        assert!(world.resource::<Assets<AnimationClip>>().len() >= 8);
        assert!(
            world
                .resource::<Assets<engine::rendering::ShieldMaterial>>()
                .len()
                >= 4
        );

        let camera_count = world.query::<&Camera>().iter(world).count();
        let ui_node_count = world.query::<&Node>().iter(world).count();
        assert!(
            camera_count >= 1,
            "UI startup should create its menu camera"
        );
        assert!(
            ui_node_count >= 1,
            "startup should create headless UI roots"
        );

        app.update();
        assert_eq!(
            app.world().resource::<State<AppState>>().get(),
            &AppState::MainMenu
        );
    }
}
