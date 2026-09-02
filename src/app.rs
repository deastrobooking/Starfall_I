// Bevy systems naturally use broad query/resource signatures. Keep clippy strict
// for correctness-oriented lints while allowing those ECS-heavy shapes.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::engine::physics::prelude::*;
use crate::{character, combat, commands, engine, engine_tools, framework, resources, world};
#[cfg(test)]
use crate::{
    components,
    events::{self, EventsPlugin},
    graph,
    plugins::*,
};
use bevy::asset::AssetApp;
use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::window::WindowResolution;
use bevy::world_serialization::WorldSerializationPlugin;

use character::blueprint::{BodyRecipe, CartoonAppearanceRecipe};
use character::parts::{
    ArmPreset, BodyPreset, CharacterLoadout, HeadPreset, LegPreset, ShoulderPreset,
};
use combat::perks::PerkTree;
use combat::upgrades::UpgradeLedger;
use commands::{CommandOverlayState, CommandRegistry};
use engine::machine_settings::{EditorPreferences, RenderQualitySettings};
use engine::state::AppState;
use engine_tools::EngineToolMode;
use resources::{
    CharacterBaseModel, CharacterBaseModelCatalog, CharacterDesignData, CharacterDesignSnapshot,
    GameSettings, LocalPlayerConfig, PlayExperience, PlaySessionTransition, PlayerPartLoadout,
    PlayerScore, PlayerSelectState, ShopCatalog, WaveInfo, WorldRouteRegistry, WorldSiteRegistry,
};
use world::final_war::FinalWarRegistry;
use world::hacking::HackingRegistry;
use world::raids::RaidRegistry;
use world::robot_pets::RobotPetCollection;
use world::settlement_economy::SettlementEconomy;

/// Launches the native all-in-one Starfall application with the Heavy Water
/// demo project installed.
pub fn run() {
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
            app.add_plugins((
                bevy::dev_tools::infinite_grid::InfiniteGridPlugin,
                bevy::gizmos::prelude::TransformGizmoPlugin,
                bevy::feathers::FeathersPlugins,
            ));
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

    // SettingsPlugin discovers reflected groups when it is added, so register
    // them first. Headless apps keep the defaults without touching user files.
    app.register_type::<EditorPreferences>()
        .register_type::<RenderQualitySettings>()
        .init_resource::<EditorPreferences>()
        .init_resource::<RenderQualitySettings>();
    if mode == StarfallAppMode::Production {
        app.add_plugins(bevy::settings::SettingsPlugin::new("com.starfall-i.game"));
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
        file_path: engine::platform_paths::asset_root()
            .to_string_lossy()
            .into_owned(),
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
        .init_resource::<resources::AuthoringTextInputCapture>()
        .init_resource::<resources::AuthoringUnsavedChanges>()
        .init_resource::<PlayerScore>()
        .init_resource::<PlaySessionTransition>()
        .init_resource::<PlayExperience>()
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
        .add_systems(PostStartup, resources::validate_world_registry_catalogs)
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
        .add_plugins(framework::StarfallFoundationPlugins);

    if add_render_materials {
        app.add_plugins(MaterialPlugin::<engine::rendering::ToonMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::WaterMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::EnergyMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::ShieldMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::IceMaterial>::default())
            .add_plugins(MaterialPlugin::<engine::rendering::LavaMaterial>::default());
    }

    app.add_plugins(framework::HeavyWaterDemoPlugins);

    // The authoring screens are the Designer edition (docs/PROJECT_PLAN.md).
    // A consumer build compiles them out entirely; their AppState variants
    // remain but nothing can enter them.
    #[cfg(feature = "designer")]
    app.add_plugins(framework::StarfallForgePlugins);
}

fn apply_boot_overrides(app: &mut App) {
    if std::env::var_os("STARFALL_AUTOSTART").is_some() {
        app.insert_state(AppState::Playing);
    }
    // Boot straight into a shared-screen platformer route for level iteration.
    if std::env::var_os("STARFALL_PLATFORMER").is_some() {
        app.insert_state(AppState::Playing);
        *app.world_mut().resource_mut::<PlayExperience>() = PlayExperience::SharedPlatformer;
    }
    // Boot straight into the Character Studio for design-tool iteration.
    if std::env::var_os("STARFALL_STUDIO").is_some() {
        app.insert_state(AppState::CharacterStudio);
    }
    // Boot straight into the modular Weapon Forge for tool iteration.
    if cfg!(feature = "designer") && std::env::var_os("STARFALL_WEAPON_FORGE").is_some() {
        app.insert_state(AppState::WeaponForge);
    }
    // Boot straight into either vehicle authoring workspace for tool iteration.
    if cfg!(feature = "designer") && std::env::var_os("STARFALL_VEHICLE_FORGE").is_some() {
        app.insert_state(AppState::VehicleForge);
    }
    if cfg!(feature = "designer") && std::env::var_os("STARFALL_SPACESHIP_FORGE").is_some() {
        app.insert_state(AppState::SpaceshipForge);
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
        assert!(world.contains_resource::<graph::NodeRegistry>());

        assert!(app.is_plugin_added::<engine::game_loop::GameLoopPlugin>());
        assert!(app.is_plugin_added::<EventsPlugin>());
        assert!(app.is_plugin_added::<graph::GraphRegistryPlugin>());
        assert!(app.is_plugin_added::<InputPlugin>());
        assert!(app.is_plugin_added::<UiPlugin>());
        assert!(app.is_plugin_added::<PlayerPlugin>());
        assert!(app.is_plugin_added::<WeaponPlugin>());
        assert!(app.is_plugin_added::<WorldPlugin>());
        assert!(app.is_plugin_added::<PublishedVehicleRuntimePlugin>());
        assert!(app.is_plugin_added::<PublishedSpacecraftRuntimePlugin>());
        assert!(app.is_plugin_added::<SavePlugin>());
        assert!(app.is_plugin_added::<HeavyEconomyPlugin>());
        assert!(app.is_plugin_added::<HeavyBioPlugin>());
        assert!(app.is_plugin_added::<HeavyCombatPlugin>());
        assert!(app.is_plugin_added::<HeavyRegionsPlugin>());
        assert!(app.is_plugin_added::<HeavyVehiclePlugin>());
        assert!(app.is_plugin_added::<HeavyWorldEventsPlugin>());
        #[cfg(feature = "designer")]
        {
            assert!(app.is_plugin_added::<CreatureForgePlugin>());
            assert!(app.is_plugin_added::<WeaponForgePlugin>());
            assert!(app.is_plugin_added::<VehicleForgePlugin>());
            assert!(app.is_plugin_added::<SpaceshipForgePlugin>());
        }
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

    /// A campaign session must actually let the party walk.
    ///
    /// Added after a report of being "trapped in one location": world
    /// generation succeeding says nothing about whether the motor moves
    /// anyone, so this drives real input through the real systems and asserts
    /// the player's transform responds.
    #[test]
    fn a_campaign_player_moves_when_input_is_applied() {
        use components::player::{Player, PlayerInput};

        let mut app = build_headless_app();
        app.insert_resource(ClearColor::default());
        app.insert_resource(GlobalAmbientLight::default());
        // Campaign world generation allocates image handles that the renderer
        // would normally register.
        app.init_asset::<Image>();
        app.insert_state(AppState::Playing);
        app.finish();
        app.cleanup();
        // Let world generation and player spawn settle.
        for _ in 0..8 {
            app.update();
        }

        let world = app.world_mut();
        let player = world
            .query_filtered::<Entity, With<Player>>()
            .iter(world)
            .next()
            .expect("a campaign session must spawn a player");
        // Drive the real key, not the PlayerInput component: the input layer
        // rewrites PlayerInput every frame, so injecting there would measure
        // nothing. Pressing W exercises binding lookup, input assembly, and the
        // motor exactly as a player does.
        // Lift the party clear of any geometry they may have spawned inside,
        // so this measures the motor rather than a spawn-point collision.
        if let Some(mut transform) = app.world_mut().get_mut::<Transform>(player) {
            transform.translation.y += 40.0;
        }
        for _ in 0..10 {
            app.update();
        }
        let start = app.world().get::<Transform>(player).unwrap().translation;

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        let mut moved_axis = false;
        for _ in 0..90 {
            app.update();
            if let Some(input) = app.world().get::<PlayerInput>(player) {
                if input.move_axis.length_squared() > 0.01 {
                    moved_axis = true;
                }
            }
        }
        assert!(
            moved_axis,
            "holding W never reached PlayerInput — the input layer is not seeing the key"
        );

        // Diagnostics for the gates that can silently freeze the motor.
        let hitstop = app
            .world()
            .resource::<combat::hitstop::HitstopState>()
            .remaining;
        let tool_mode = *app.world().resource::<State<EngineToolMode>>().get();
        let movement = app
            .world()
            .get::<components::player::PlayerMovement>(player)
            .unwrap();
        let diag = format!(
            "hitstop={hitstop:.3} tool_mode={tool_mode:?} grounded={} vel={:?} accum={:?} carry={:?}",
            movement.is_grounded, movement.velocity, movement.motor_accum, movement.motor_carry
        );
        let end = app
            .world()
            .get::<Transform>(player)
            .expect("player still exists")
            .translation;
        let travelled = start.distance(end);
        assert!(
            travelled > 1.0,
            "player did not move with sustained input (from {start:?} to {end:?}, {travelled:.3}m) [{diag}]"
        );
    }

    #[test]
    fn headless_app_launches_bounded_platformer_as_a_direct_play_session() {
        let mut app = build_headless_app();
        *app.world_mut().resource_mut::<PlayExperience>() = PlayExperience::SharedPlatformer;
        // Presentation plugins normally provide these in production. The
        // headless harness supplies their CPU-side values so gameplay Update
        // systems can exercise a direct Playing session without a renderer.
        app.insert_resource(ClearColor::default());
        app.insert_resource(GlobalAmbientLight::default());
        app.insert_state(AppState::Playing);
        app.finish();
        app.cleanup();
        app.update();
        app.update();

        let world = app.world_mut();
        assert_eq!(
            world.resource::<State<AppState>>().get(),
            &AppState::Playing
        );
        assert!(
            world
                .resource::<resources::DungeonCrawlState>()
                .platformer_rules
        );
        assert_eq!(
            world
                .query::<&world::co_op_platformer::PlatformerStageRoot>()
                .iter(world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query::<&components::player::Player>()
                .iter(world)
                .count(),
            1
        );
    }

    #[test]
    fn returning_from_platformer_to_title_restores_campaign_front_door() {
        let mut app = build_headless_app();
        *app.world_mut().resource_mut::<PlayExperience>() = PlayExperience::SharedPlatformer;
        // Campaign world generation allocates texture handles even though this
        // transition smoke never renders them. Production gets this from the
        // render plugins; the headless harness registers the CPU asset type.
        app.init_asset::<Image>();
        app.insert_resource(ClearColor::default());
        app.insert_resource(GlobalAmbientLight::default());
        app.insert_state(AppState::Playing);
        app.finish();
        app.cleanup();
        app.update();
        app.update();

        assert_eq!(
            *app.world().resource::<PlayExperience>(),
            PlayExperience::SharedPlatformer
        );
        assert!(
            app.world()
                .resource::<resources::DungeonCrawlState>()
                .platformer_rules
        );

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::MainMenu);
        app.update();
        app.update();

        assert_eq!(
            app.world().resource::<State<AppState>>().get(),
            &AppState::MainMenu
        );
        assert_eq!(
            *app.world().resource::<PlayExperience>(),
            PlayExperience::Campaign
        );
        let dungeon = app.world().resource::<resources::DungeonCrawlState>();
        assert!(!dungeon.active);
        assert!(!dungeon.arcade_rules);
        assert!(!dungeon.platformer_rules);
        assert_eq!(
            app.world_mut()
                .query::<&world::co_op_platformer::PlatformerStageRoot>()
                .iter(app.world())
                .count(),
            0
        );

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();
        app.update();

        assert_eq!(
            app.world().resource::<State<AppState>>().get(),
            &AppState::Playing
        );
        assert_eq!(
            *app.world().resource::<PlayExperience>(),
            PlayExperience::Campaign
        );
        assert!(app.world().resource::<resources::CurrentChapter>().started);
        assert!(
            !app.world()
                .resource::<resources::DungeonCrawlState>()
                .active
        );
        assert_eq!(
            app.world_mut()
                .query::<&world::co_op_platformer::PlatformerStageRoot>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world_mut()
                .query::<&components::player::Player>()
                .iter(app.world())
                .count(),
            1
        );
    }
}
