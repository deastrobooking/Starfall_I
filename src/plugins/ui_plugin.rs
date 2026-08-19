use bevy::audio::{AudioPlayer, PlaybackSettings, Volume};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow};

use bevy::input::gamepad::{GamepadButton, GamepadButtonStateChangedEvent};
use bevy::input::ButtonState;
use bevy::ui::{ComputedStackIndex, InteractionDisabled, UiGlobalTransform, UiSystems};

use crate::chapters::{
    all_chapters, chapter_map_locations, map_settlements, ChapterId, MapSettlementKind,
    EVEREST_RANGE_HALF_EXTENT, EVEREST_RANGE_WORLD_SIZE, RACE_REGION_TRAVEL_POINTS,
    SECRET_CAVE_LOCATIONS,
};
use crate::combat::damage::Health;
use crate::combat::perks::{all_perks, PerkTree};
use crate::combat::tricks::TrickState;
use crate::combat::upgrades::{
    all_tech_upgrades, format_part_costs, TechUpgradeId, UpgradeLedger, SABRE_RELIC_CATALOG,
};
use crate::commands::{CommandOverlayState, CommandRegistry};
use crate::components::armor::ArmorSet;
use crate::components::discoverable::{
    Discoverable, DiscoverableKind, PuzzleArchetype, PuzzleRelicEncounter,
};
use crate::components::enemy::CitySpyDrone;
use crate::components::inventory::{
    all_items, item_definition as canonical_item_definition, Inventory, ItemType, QuickItemSlot,
};
use crate::components::player::{
    AimReticleState, AimSolution, BoardBoostState, ClimbState, JetpackState, Player, PlayerCamera,
    PlayerCameraRef, PlayerIndex, PlayerInput, PlayerMovement, PlayerProgression, PlayerStats,
    RooftopTrialProgress, StuntRunState, TraversalMode, TraversalModeState, WaterTraversalState,
};
use crate::components::weapon::{
    BeamSabre, SpecialWeaponInventory, TrackingMissile, WeaponInventory, WeaponRanks, WeaponType,
    MAX_WEAPON_RANK,
};
use crate::components::world::{
    BoatPassenger, BoatVehicle, DiscussionNpc, DungeonCrawlGate, DungeonExitPortal, WorldLoot,
};
use crate::engine::bindings::{ControlBindings, FaceButton};
use crate::engine::physics::prelude::{Physics, PhysicsTime};
use crate::engine::rendering::Camera3dBundle;
use crate::engine::state::AppState;
use crate::engine_tools::project_registry::ForgeProjectRegistry;
use crate::engine_tools::tool_windows::{ToolWindow, ToolWindowChromeControl};
use crate::engine_tools::{
    EngineToolMode, PublishedMapPointKind, PublishedProceduralRecipeCatalog,
};
use crate::events::*;
use crate::plugins::crafting_plugin::{all_recipes, start_craft, CraftingQueue};
use crate::plugins::heavy_bio_plugin::{try_harvest_bio_plot, try_plant_bio_seed, HeavyBioClock};
use crate::plugins::heavy_economy_plugin::{
    buy_from_vendor_canonical_atomic, mount_jewel_from_canonical_atomic,
    sell_to_vendor_canonical_atomic, unmount_jewel_to_canonical_atomic,
};
use crate::plugins::input_plugin::{
    mapped_gamepad_face, mapped_native_face, ControllerInputSet, GamepadAssignments, NativeButton,
    NativeControllerState,
};
use crate::plugins::save_plugin::{save_current_session, save_settings, SaveParams};
use crate::plugins::vehicle_plugin::VehicleState;
use crate::resources::{
    AuthoringTextInputCapture, AuthoringUnsavedChanges, ChapterProgress, CharacterDesignData,
    CharacterDesignReturnTarget, CurrentChapter, DungeonCrawlState, FastTravelDestination,
    GameSettings, ImportedForgeReturnTarget, LocalPlayerConfig, PlaySessionTransition,
    PlayerGuidance, PlayerSelectState, ShopCatalog, ShopCategory, UiGameplayCapture, UiMessage,
    WaveInfo, WorldSiteRegistry, HERO_ROSTER,
};
use crate::world::discussion::DiscussionState;
use crate::world::heavy_bio::{GrowthStage, GARDEN_PLOT_COUNT};
use crate::world::heavy_economy::{JewelTier, OwnerId, WeaponSocket};
use crate::world::heavy_water::{local_player_key, HeavyWaterProgress};
use crate::world::missions::{
    active_custom_mission, chapter_mission, mission_for_travel_anchor, CustomMissionState,
    SPECIAL_MISSION_TRAVEL_POINTS,
};
use crate::world::published_content::ReloadPublishedCraftCatalogs;
use crate::world::published_craft::PublishedFleet;
use crate::world::robot_pets::{RobotPartKind, RobotPetCollection};
use crate::world::settlement_economy::SettlementEconomy;
use crate::world::shop_transactions;

use super::ui_foundation::UiFoundationPlugin;
pub(crate) use super::ui_foundation::{UiPromptKind, UiPromptText};
pub use super::ui_foundation::{UiTextCatalog, UiTextKey, UiTheme};

pub struct UiPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ChapterSelectUiSet {
    TravelActions,
    LoadingFeedback,
}

/// Shared menu focus publishes controller-driven `Interaction::Pressed` in
/// `PreUpdate`, after Bevy's pointer focus pass and before every screen's
/// `Update` action consumers. The schedule boundary also covers button
/// consumers registered by the individual forge and designer plugins.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MenuUiSet {
    ReleaseActivation,
    FocusDispatch,
    ActionConsumers,
}

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiFoundationPlugin)
            // Headless builds omit WindowPlugin, but shared focus still drains
            // this stream. Registration is idempotent in windowed builds.
            .add_message::<CursorMoved>()
            .init_resource::<UiMessage>()
            .init_resource::<ShopUiState>()
            .init_resource::<PlayerGuidance>()
            .init_resource::<CraftingPanelState>()
            .init_resource::<LoadoutPanelState>()
            .init_resource::<DiscussionState>()
            .init_resource::<PauseMenuState>()
            .init_resource::<MainMenuUiState>()
            .init_resource::<MenuFocus>()
            .init_resource::<GamepadAssignments>()
            .init_resource::<AuthoringTextInputCapture>()
            .init_resource::<ChapterProgressionOwner>()
            .init_resource::<CustomMissionState>()
            .configure_sets(
                Update,
                (
                    ChapterSelectUiSet::TravelActions,
                    ChapterSelectUiSet::LoadingFeedback,
                )
                    .chain(),
            )
            .configure_sets(
                PreUpdate,
                MenuUiSet::ReleaseActivation.before(UiSystems::Focus),
            )
            .configure_sets(
                PreUpdate,
                MenuUiSet::FocusDispatch
                    .after(UiSystems::Focus)
                    .after(ControllerInputSet::Ready),
            )
            .configure_sets(Update, MenuUiSet::ActionConsumers)
            .add_systems(Startup, spawn_menu_camera)
            .add_systems(
                PreUpdate,
                release_menu_controller_activation.in_set(MenuUiSet::ReleaseActivation),
            )
            .add_systems(
                PreUpdate,
                (
                    register_menu_buttons.run_if(in_menu_state),
                    menu_focus_navigation,
                    keep_menu_focus_in_view.run_if(in_menu_state),
                    menu_back_navigation,
                    menu_focus_style.run_if(in_menu_state),
                )
                    .chain()
                    .in_set(MenuUiSet::FocusDispatch),
            )
            .add_systems(
                OnEnter(AppState::MainMenu),
                (cleanup_play_ui_for_menu, setup_main_menu),
            )
            .add_systems(
                OnEnter(AppState::ProjectHub),
                (despawn_menu, setup_project_hub).chain(),
            )
            .add_systems(OnExit(AppState::ProjectHub), despawn_project_hub)
            .add_systems(
                OnEnter(AppState::PlayerSelect),
                (despawn_menu, setup_player_select),
            )
            .add_systems(OnExit(AppState::PlayerSelect), despawn_player_select)
            .add_systems(OnEnter(AppState::ChapterSelect), setup_chapter_select)
            .add_systems(OnExit(AppState::ChapterSelect), despawn_chapter_select)
            .init_resource::<ControllerDiagState>()
            .add_systems(
                OnEnter(AppState::Playing),
                (
                    setup_hud,
                    despawn_menu,
                    setup_controller_diag,
                    setup_command_overlay,
                ),
            )
            .add_systems(
                OnExit(AppState::Playing),
                (despawn_controller_diag, despawn_command_overlay),
            )
            .add_systems(
                OnEnter(AppState::Paused),
                (setup_pause_menu, freeze_physics_on_pause),
            )
            .add_systems(
                OnExit(AppState::Paused),
                (despawn_pause_menu, resume_physics_after_pause),
            )
            .add_systems(OnEnter(AppState::GameOver), setup_game_over)
            .add_systems(OnEnter(AppState::Victory), setup_victory_screen)
            .add_systems(
                Update,
                victory_input
                    .in_set(MenuUiSet::ActionConsumers)
                    .run_if(in_state(AppState::Victory)),
            )
            .init_resource::<PendingRebind>()
            .add_systems(
                Update,
                (
                    settings_panel_input_system,
                    // Capture runs after the buttons so arming and consuming a
                    // press can never happen in the same frame (the click that
                    // arms would otherwise be eaten as the new binding).
                    settings_rebind_capture_system.after(settings_panel_input_system),
                )
                    .in_set(MenuUiSet::ActionConsumers)
                    .run_if(in_state(AppState::Paused).or_else(in_state(AppState::MainMenu))),
            )
            .add_systems(Update, pause_input_system)
            .add_systems(
                Update,
                (
                    pause_menu_action_system,
                    chapter_select_fast_travel_buttons,
                    cave_fast_travel_buttons,
                    world_anchor_fast_travel_buttons,
                    editor_fast_travel_buttons,
                    shop_action_system,
                    shop_panel_refresh_system.after(shop_action_system),
                    update_pause_menu_page_visibility,
                )
                    .in_set(MenuUiSet::ActionConsumers)
                    .run_if(in_state(AppState::Paused)),
            )
            .add_systems(
                Update,
                clear_play_session_transition_flags.run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (toggle_controller_diag, update_controller_diag)
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::Paused))),
            )
            .add_systems(
                Update,
                (toggle_command_overlay, update_command_overlay)
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::Paused))),
            )
            .add_systems(
                Update,
                (
                    responsive_hud_layout_system,
                    hud_update_system,
                    aim_reticle_system,
                )
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                puzzle_objective_hud_system
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                message_timer_system.run_if(
                    in_state(AppState::Playing)
                        .or_else(in_state(AppState::Paused))
                        .or_else(in_state(AppState::GameOver)),
                ),
            )
            .add_systems(
                Update,
                ui_message_listener.run_if(
                    in_state(AppState::Playing)
                        .or_else(in_state(AppState::Paused))
                        .or_else(in_state(AppState::GameOver)),
                ),
            )
            .add_systems(
                Update,
                damage_vignette_system
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                (crafting_panel_system, loadout_panel_system)
                    .chain()
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                discussion_panel_system
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                player_guidance_system.run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                boss_alert_system
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                (
                    level_up_event_system,
                    chapter_completed_ui_system,
                    boss_defeated_ui_system,
                )
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                game_over_input
                    .in_set(MenuUiSet::ActionConsumers)
                    .run_if(in_state(AppState::GameOver)),
            )
            .add_systems(
                Update,
                (menu_start_button, update_main_menu_page_visibility)
                    .chain()
                    .in_set(MenuUiSet::ActionConsumers)
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                project_hub_action_system
                    .in_set(MenuUiSet::ActionConsumers)
                    .run_if(in_state(AppState::ProjectHub)),
            )
            .add_systems(
                Update,
                player_select_controller_join.in_set(MenuUiSet::ActionConsumers),
            )
            .add_systems(
                Update,
                (sync_player_select_button_availability, player_select_update)
                    .chain()
                    .after(player_select_controller_join)
                    .in_set(MenuUiSet::ActionConsumers)
                    .run_if(in_state(AppState::PlayerSelect)),
            )
            .add_systems(
                Update,
                (
                    chapter_select_action_buttons.in_set(ChapterSelectUiSet::TravelActions),
                    chapter_select_progression_owner_buttons,
                    chapter_select_fast_travel_buttons.in_set(ChapterSelectUiSet::TravelActions),
                    cave_fast_travel_buttons.in_set(ChapterSelectUiSet::TravelActions),
                    world_anchor_fast_travel_buttons.in_set(ChapterSelectUiSet::TravelActions),
                    chapter_select_perk_buttons,
                    chapter_select_upgrade_buttons,
                    chapter_select_weapon_rank_buttons,
                    chapter_select_mouse_scroll,
                    chapter_select_perk_panel_update,
                    chapter_select_upgrade_panel_update,
                    chapter_select_weapon_rank_panel_update,
                    chapter_select_economy_panel_update,
                    final_push_unlock_system,
                )
                    .in_set(MenuUiSet::ActionConsumers)
                    .run_if(in_state(AppState::ChapterSelect)),
            )
            .add_systems(
                Update,
                update_chapter_loading_overlay
                    .in_set(ChapterSelectUiSet::LoadingFeedback)
                    .run_if(in_state(AppState::ChapterSelect)),
            );
    }
}

// ── Marker Components ─────────────────────────────────────────────────────────
#[derive(Component)]
struct MenuCamera;
#[derive(Component)]
struct MainMenuRoot;
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct MainMenuPagePanel(MainMenuPage);
#[derive(Clone, Copy, PartialEq, Eq)]
enum MainMenuPage {
    Home,
    Settings,
}
#[derive(Resource)]
struct MainMenuUiState {
    page: MainMenuPage,
}
impl Default for MainMenuUiState {
    fn default() -> Self {
        Self {
            page: MainMenuPage::Home,
        }
    }
}
#[derive(Component)]
struct ProjectHubRoot;
#[derive(Component)]
struct HudRoot;
#[derive(Component)]
struct PauseRoot;
#[derive(Component, Clone, Copy)]
struct PauseMenuButton(PauseAction);
#[derive(Clone, Copy)]
enum PauseAction {
    Resume,
    Map,
    Save,
    Title,
    Controls,
    Shop,
    Settings,
    Back,
}
#[derive(Component, Clone, Copy)]
struct PausePagePanel(PausePage);
#[derive(Clone, Copy, PartialEq, Eq)]
enum PausePage {
    Main,
    Map,
    Controls,
    Shop,
    Settings,
}
#[derive(Resource)]
struct PauseMenuState {
    page: PausePage,
    requested_page: Option<PausePage>,
    resume_lockout: f32,
    resume_armed: bool,
    back_resume_requested: bool,
}

#[derive(Component, Debug, Clone)]
struct EditorFastTravelButton {
    point_id: String,
    label: String,
    position: Vec3,
}
impl Default for PauseMenuState {
    fn default() -> Self {
        Self {
            page: PausePage::Main,
            requested_page: None,
            resume_lockout: 0.0,
            resume_armed: true,
            back_resume_requested: false,
        }
    }
}
/// PX3 shop actions raised by focusable pause-menu buttons. Kept separate from
/// `PauseAction` so the shop flow does not widen `pause_menu_action_system`.
#[derive(Component, Clone, Copy)]
struct ShopButton(ShopAction);
#[derive(Clone, Copy)]
enum ShopAction {
    Category(ShopCategory),
    Select(&'static str),
    OwnerCycle,
    ConfirmBuy,
    Equip,
    Unequip,
    CloseDetail,
}
#[derive(Component)]
struct ShopHeaderText;
#[derive(Component)]
struct ShopCardList;
#[derive(Component)]
struct ShopDetailPanel;
/// Live pause-shop browsing state: active category tab, which joined player is
/// shopping (per-player ownership), and the card selected for the detail /
/// confirmation panel. `dirty` requests a card/detail rebuild.
#[derive(Resource)]
struct ShopUiState {
    category: ShopCategory,
    owner: u8,
    selected: Option<&'static str>,
    dirty: bool,
}
impl Default for ShopUiState {
    fn default() -> Self {
        Self {
            category: ShopCategory::Outfits,
            owner: 0,
            selected: None,
            dirty: true,
        }
    }
}
#[derive(Component)]
struct GameOverRoot;
#[derive(Component)]
struct VictoryRoot;
#[derive(Component)]
#[allow(dead_code)]
struct SettingsPanelRoot;
#[derive(Component)]
struct DifficultyText;
#[derive(Component)]
struct VolumeText;
#[derive(Component)]
struct InterfaceLayoutText;
/// Live readout of the control bindings on the settings page.
#[derive(Component)]
struct ControlBindingsText;
#[derive(Component)]
struct AccessibilitySettingsText;
#[derive(Component)]
struct FinalPushText;
#[derive(Component)]
struct StartButton;
#[derive(Component)]
struct EditorStartButton;
#[derive(Component)]
struct MainMenuSettingsButton;
#[derive(Component)]
struct MainMenuSettingsBackButton;
#[derive(Component, Clone, Copy)]
struct ProjectHubButton(ProjectHubAction);

/// The hub's status line; publish results land here so the designer sees the
/// outcome where they clicked, not in a log file.
#[derive(Component)]
struct ProjectHubStatusText;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectHubAction {
    /// Continue the registry's active project.
    OpenCurrent,
    /// Open the registry project at this index and make it active.
    OpenProject(usize),
    /// Create a fresh empty project, activate it, and enter the editor.
    NewProject,
    /// Open the Creature Forge authoring screen.
    CreatureForge,
    /// Open the modular Weapon Forge authoring screen.
    WeaponForge,
    /// Open the ground-vehicle recipe authoring screen.
    VehicleForge,
    /// Open the spacecraft recipe authoring screen.
    SpaceshipForge,
    /// Validate the active project and bake it to `assets/published/` — the
    /// Designer→Game content bridge (docs/PROJECT_PLAN.md P1).
    Publish,
    /// Open the GLB-based imported character editor.
    ImportedCharacterForge,
    Back,
}

/// Authoring destinations shown together in the responsive Project Hub. Keep
/// this one registry as the source for labels, ordering, and state actions so
/// adding a Forge cannot create a visible button that routes somewhere else.
fn project_hub_authoring_actions() -> [(&'static str, ProjectHubAction, Color); 5] {
    [
        (
            "CREATURE FORGE",
            ProjectHubAction::CreatureForge,
            Color::srgb(0.12, 0.42, 0.28),
        ),
        (
            "WEAPON FORGE",
            ProjectHubAction::WeaponForge,
            Color::srgb(0.16, 0.28, 0.52),
        ),
        (
            "VEHICLE FORGE",
            ProjectHubAction::VehicleForge,
            Color::srgb(0.42, 0.24, 0.12),
        ),
        (
            "SPACESHIP FORGE",
            ProjectHubAction::SpaceshipForge,
            Color::srgb(0.24, 0.18, 0.50),
        ),
        (
            "IMPORTED CHARACTER FORGE",
            ProjectHubAction::ImportedCharacterForge,
            Color::srgb(0.12, 0.32, 0.46),
        ),
    ]
}

/// Shared marker applied automatically to every Bevy UI button while a menu is
/// active. Individual screens keep owning their actions; this layer only owns
/// focus, directional navigation, activation, and common interaction feedback.
#[derive(Component)]
struct MenuFocusable;

/// A vertically scrolling menu surface whose focused button must remain on-screen.
/// Chapter Select currently owns one; other menus can opt in without duplicating
/// controller-scroll math.
#[derive(Component)]
pub(crate) struct MenuScrollPanel;

#[derive(Component)]
#[require(InteractionDisabled)]
pub(crate) struct MenuButtonDisabled;

#[derive(Component, Clone)]
struct MenuButtonPalette {
    background: BackgroundColor,
    border: BorderColor,
    applied_background: Option<BackgroundColor>,
    applied_border: Option<BorderColor>,
}

#[derive(Resource, Default)]
struct MenuFocus {
    entity: Option<Entity>,
    state: Option<AppState>,
    controller_activation: Option<Entity>,
    pointer_active: bool,
    reveal_requested: bool,
    layout_reveal_frames: u8,
    repeat_direction: Option<MenuDirection>,
    repeat_timer: f32,
}
#[derive(Component, Clone, Copy)]
struct SettingsButton(SettingsAction);
#[derive(Clone, Copy)]
enum SettingsAction {
    Difficulty(DifficultyChoice),
    MusicDown,
    MusicUp,
    SfxDown,
    SfxUp,
    ToggleRumble,
    UiScaleDown,
    UiScaleUp,
    SafeAreaDown,
    SafeAreaUp,
    ControllerGlyphStyleNext,
    ToggleDamageNumbers,
    ToggleHighContrast,
    ToggleReducedMotion,
    ToggleSubtitles,
    /// Cycle the face-button layout preset (Standard / Nintendo / Swap A/B).
    FaceLayoutNext,
    /// Invert the look Y axis for mouse and stick together.
    ToggleInvertLook,
    /// Begin capturing the next key press for this action.
    RebindKey(crate::engine::bindings::KeyAction),
    /// Restore every keyboard binding to its shipped default.
    ResetKeys,
}

/// The action currently waiting for a key press, if the player is rebinding.
#[derive(Resource, Debug, Default)]
struct PendingRebind(Option<crate::engine::bindings::KeyAction>);
#[derive(Clone, Copy)]
enum DifficultyChoice {
    Easy,
    Normal,
    Hard,
}
#[derive(Component)]
struct GameOverMenuButton;
#[derive(Component, Clone, Copy)]
struct VictoryMenuButton(VictoryAction);
#[derive(Clone, Copy)]
enum VictoryAction {
    MainMenu,
    NewGamePlus,
}
#[derive(Component)]
struct PlayerHudBar {
    player_index: u8,
    kind: PlayerHudBarKind,
}

#[derive(Component)]
struct PlayerHudPanel {
    player_index: u8,
}

#[derive(Component)]
struct PlayerHudBarRow {
    player_index: u8,
    kind: PlayerHudBarKind,
}

#[derive(Component)]
struct UnderwaterOverlay {
    player_index: u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlayerHudBarKind {
    Health,
    Armor,
    Stamina,
    Jetpack,
    Climb,
    Air,
}

#[derive(Component)]
struct PlayerHudText {
    player_index: u8,
    kind: PlayerHudTextKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlayerHudTextKind {
    Header,
    Credits,
    Level,
    Element,
    WeaponName,
    Ammo,
    SpecialAmmo,
    TraversalStatus,
    SabreStatus,
}

#[derive(Component)]
struct EnemyCountText;

// ── Controller Diagnostics Overlay ───────────────────────────────────────────
#[derive(Resource, Default)]
struct ControllerDiagState {
    visible: bool,
}
#[derive(Component)]
struct ControllerDiagRoot;
#[derive(Component)]
struct ControllerDiagText;
#[derive(Component)]
struct WaveText;
#[derive(Component)]
struct ObjectiveTitleText;
#[derive(Component)]
struct ObjectiveStatusText;
#[derive(Component)]
struct MessageText;
#[derive(Component)]
struct DamageVignette {
    player_index: u8,
    alpha: f32,
}
#[derive(Component)]
struct BossAlertText;
#[derive(Component, Clone, Copy)]
struct AimReticle {
    player_index: u8,
}
#[derive(Component, Clone, Copy)]
struct AimReticleBar {
    player_index: u8,
}
#[derive(Component)]
struct CraftingPanelRoot;
#[derive(Component)]
struct CraftingPanelText;
#[derive(Component)]
struct LoadoutPanelRoot;
#[derive(Component)]
struct LoadoutTitleText;
#[derive(Component)]
struct LoadoutStatusText;
#[derive(Component, Clone, Copy)]
struct LoadoutTabButton(LoadoutTab);
#[derive(Component, Clone, Copy)]
struct LoadoutEntryButton(usize);
#[derive(Component)]
struct LoadoutCloseButton;
#[derive(Component, Clone, Copy)]
struct LoadoutEntryLabel(usize);
#[derive(Component)]
struct DiscussionPanelRoot;
#[derive(Component)]
struct DiscussionTitleText;
#[derive(Component)]
struct DiscussionSpeakerText;
#[derive(Component)]
struct DiscussionBodyText;
#[derive(Component)]
struct DiscussionHintText;
#[derive(Component)]
struct GuidancePanelRoot;
#[derive(Component)]
struct GuidanceTitleText;
#[derive(Component)]
struct GuidanceBodyText;
#[derive(Component)]
struct GuidanceActionText;

// ── Crafting Panel State ──────────────────────────────────────────────────────
#[derive(Resource, Default)]
struct CraftingPanelState {
    visible: bool,
    owner: Option<u8>,
    tab: [WorkshopTab; 4],
    recipe_index: [usize; 4],
    jewel_index: [usize; 4],
    garden_index: [usize; 4],
    vendor_index: [usize; 4],
    vendor_item_index: [usize; 4],
    repeat_direction: i8,
    repeat_timer: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum WorkshopTab {
    #[default]
    Crafting,
    Jewels,
    BioGarden,
    Market,
}

impl WorkshopTab {
    const fn offset(self, direction: i8) -> Self {
        match (self, direction.signum()) {
            (Self::Crafting, -1) => Self::Market,
            (Self::Crafting, _) => Self::Jewels,
            (Self::Jewels, -1) => Self::Crafting,
            (Self::Jewels, _) => Self::BioGarden,
            (Self::BioGarden, -1) => Self::Jewels,
            (Self::BioGarden, _) => Self::Market,
            (Self::Market, -1) => Self::BioGarden,
            (Self::Market, _) => Self::Crafting,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LoadoutTab {
    #[default]
    Weapons,
    Armor,
    Items,
    Specials,
    Rides,
    Relics,
}

impl LoadoutTab {
    const ALL: [Self; 6] = [
        Self::Weapons,
        Self::Armor,
        Self::Items,
        Self::Specials,
        Self::Rides,
        Self::Relics,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Weapons => 0,
            Self::Armor => 1,
            Self::Items => 2,
            Self::Specials => 3,
            Self::Rides => 4,
            Self::Relics => 5,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Weapons => "WEAPONS",
            Self::Armor => "ARMOR",
            Self::Items => "ITEMS",
            Self::Specials => "SPECIALS",
            Self::Rides => "RIDES",
            Self::Relics => "RELICS",
        }
    }

    fn offset(self, delta: i8) -> Self {
        let len = Self::ALL.len();
        let index = if delta < 0 {
            (self.index() + len - 1) % len
        } else {
            (self.index() + 1) % len
        };
        Self::ALL[index]
    }
}

#[derive(Resource, Default)]
struct LoadoutPanelState {
    visible: bool,
    owner: Option<u8>,
    tab: LoadoutTab,
    selection: [[usize; 6]; 4],
    repeat_direction: i8,
    repeat_timer: f32,
}

fn in_menu_state(state: Res<State<AppState>>) -> bool {
    is_shared_menu_state(state.get())
}

fn is_shared_menu_state(state: &AppState) -> bool {
    // CharacterStudio owns a specialized row/field navigator for sliders and
    // viewport controls; do not double-dispatch its controller input here.
    !matches!(state, AppState::Playing | AppState::CharacterStudio)
}

fn register_menu_buttons(
    mut commands: Commands,
    buttons: Query<
        (Entity, &BackgroundColor, &BorderColor),
        (
            Added<Button>,
            Without<MenuFocusable>,
            Without<ToolWindowChromeControl>,
        ),
    >,
) {
    for (entity, background, border) in buttons.iter() {
        commands.entity(entity).insert((
            MenuFocusable,
            MenuButtonPalette {
                background: *background,
                border: *border,
                applied_background: None,
                applied_border: None,
            },
        ));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuDirection {
    Up,
    Down,
    Left,
    Right,
}

impl MenuDirection {
    fn vector(self) -> Vec2 {
        match self {
            // Bevy UI coordinates increase downward, unlike world-space Y.
            Self::Up => Vec2::NEG_Y,
            Self::Down => Vec2::Y,
            Self::Left => Vec2::NEG_X,
            Self::Right => Vec2::X,
        }
    }
}

fn menu_focus_navigation(
    time: Res<Time>,
    state: Res<State<AppState>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    gamepads: Query<(Entity, &Gamepad)>,
    native: Res<NativeControllerState>,
    assignments: Res<GamepadAssignments>,
    windows: Query<Ref<Window>, With<PrimaryWindow>>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    mut cursor_events: MessageReader<CursorMoved>,
    settings: Res<GameSettings>,
    text_input: Res<AuthoringTextInputCapture>,
    mut focus: ResMut<MenuFocus>,
    parents: Query<&ChildOf>,
    ui_nodes: Query<&Node>,
    tool_windows: Query<(&UiGlobalTransform, &ComputedNode, &ComputedStackIndex), With<ToolWindow>>,
    mut button_set: ParamSet<(
        Query<
            (
                Entity,
                &UiGlobalTransform,
                &ComputedNode,
                &ComputedStackIndex,
                &Interaction,
                &InheritedVisibility,
                Option<&MenuButtonDisabled>,
            ),
            (With<Button>, With<MenuFocusable>),
        >,
        Query<&mut Interaction, (With<Button>, With<MenuFocusable>)>,
    )>,
) {
    let current_state = state.get().clone();
    let pressed_button_events = button_events.read().copied().collect::<Vec<_>>();
    let ignored_overlay = assignments.native_overlay_gamepad();
    let pointer_moved = cursor_events.read().count() != 0;
    let pointer_pressed = mouse_buttons.just_pressed(MouseButton::Left);

    // Keep both message-reader cursors current while gameplay or Character
    // Studio owns input. Otherwise an old East/Start chord can replay as Back
    // on the first frame after Pause opens.
    if !is_shared_menu_state(&current_state) {
        focus.state = Some(current_state);
        focus.entity = None;
        focus.pointer_active = false;
        focus.reveal_requested = false;
        focus.layout_reveal_frames = 0;
        focus.repeat_direction = None;
        focus.repeat_timer = 0.0;
        return;
    }

    if focus.state.as_ref() != Some(&current_state) {
        focus.state = Some(current_state);
        focus.entity = None;
    }
    if focus.entity.is_some()
        && (settings.is_changed() || windows.iter().any(|window| window.is_changed()))
    {
        // The first reveal can still observe the previous frame's layout;
        // keep one request for the post-layout geometry on the next frame.
        focus.layout_reveal_frames = 2;
    }
    let previous_focus = focus.entity;

    let direction_input = if text_input.active {
        MenuDirectionInput::default()
    } else {
        menu_direction_input(
            &keyboard,
            &gamepads,
            &native,
            &pressed_button_events,
            ignored_overlay,
        )
    };
    let direction = repeated_menu_direction(&mut focus, direction_input, time.delta_secs());
    let (confirm_gamepad, confirm_native) = mapped_menu_face(&settings.bindings, FaceButton::South);
    let confirm = !text_input.active
        && (keyboard.just_pressed(KeyCode::Enter)
            || keyboard.just_pressed(KeyCode::NumpadEnter)
            || keyboard.just_pressed(KeyCode::Space)
            || gamepads
                .iter()
                .any(|(entity, gamepad)| {
                    Some(entity) != ignored_overlay && gamepad.just_pressed(confirm_gamepad)
                })
            || gamepad_button_pressed_in_events(
                &pressed_button_events,
                confirm_gamepad,
                ignored_overlay,
            )
            || native.just_pressed(confirm_native));
    if pointer_moved || pointer_pressed {
        focus.pointer_active = true;
    } else if direction.is_some() || confirm {
        focus.pointer_active = false;
    }

    let window_rects = tool_windows
        .iter()
        .filter(|(_, computed, _)| !computed.is_empty())
        .map(|(transform, computed, stack)| (ui_node_rect(transform, computed), stack.0))
        .collect::<Vec<_>>();

    let mut visible = Vec::new();
    let mut hovered = None;
    for (entity, transform, computed, stack, interaction, inherited_visibility, disabled) in
        button_set.p0().iter()
    {
        if !menu_focusable_geometry(
            entity,
            transform,
            computed,
            inherited_visibility,
            disabled.is_some(),
            &parents,
            &ui_nodes,
            stack.0,
            &window_rects,
        ) {
            continue;
        }
        visible.push((entity, transform.affine().translation));
        if pointer_interaction_claims_focus(*interaction, pointer_moved, pointer_pressed) {
            hovered = Some(entity);
        }
    }

    if let Some(entity) = hovered {
        focus.entity = Some(entity);
    }

    if !visible
        .iter()
        .any(|(entity, _)| Some(*entity) == focus.entity)
    {
        focus.entity = default_menu_focus(&visible);
    }

    if let (Some(direction), Some(current)) = (direction, focus.entity) {
        if let Some(next) = directional_menu_focus(current, direction, &visible) {
            focus.entity = Some(next);
        }
    }
    if focus.entity != previous_focus {
        focus.reveal_requested = true;
    }

    if confirm {
        if let Some(entity) = focus.entity {
            if let Ok(mut interaction) = button_set.p1().get_mut(entity) {
                *interaction = Interaction::Pressed;
                focus.controller_activation = Some(entity);
            }
        }
    }
}

/// Controller activation is a one-frame pulse, not Bevy's mouse-down latch.
/// Release it before `UiSystems::Focus` so that pass can still publish a real
/// mouse click occurring on the following frame.
fn release_menu_controller_activation(
    mut focus: ResMut<MenuFocus>,
    mut interactions: Query<&mut Interaction, (With<Button>, With<MenuFocusable>)>,
) {
    let Some(entity) = focus.controller_activation.take() else {
        return;
    };
    if let Ok(mut interaction) = interactions.get_mut(entity) {
        if *interaction == Interaction::Pressed {
            *interaction = Interaction::None;
        }
    }
}

fn pointer_interaction_claims_focus(
    interaction: Interaction,
    pointer_moved: bool,
    pointer_pressed: bool,
) -> bool {
    (pointer_moved && matches!(interaction, Interaction::Hovered | Interaction::Pressed))
        || (pointer_pressed && interaction == Interaction::Pressed)
}

fn gamepad_button_pressed_in_events(
    events: &[GamepadButtonStateChangedEvent],
    button: GamepadButton,
    ignored_gamepad: Option<Entity>,
) -> bool {
    events
        .iter()
        .any(|event| {
            Some(event.entity) != ignored_gamepad
                && event.state == ButtonState::Pressed
                && event.button == button
        })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct UiFocusRect {
    min: Vec2,
    max: Vec2,
}

impl UiFocusRect {
    fn contains(self, point: Vec2) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }
}

fn ui_node_rect(transform: &UiGlobalTransform, computed: &ComputedNode) -> UiFocusRect {
    let center = transform.affine().translation;
    let half = computed.size() * 0.5;
    UiFocusRect {
        min: center - half,
        max: center + half,
    }
}

fn has_display_none_ancestor(
    entity: Entity,
    parents: &Query<&ChildOf>,
    ui_nodes: &Query<&Node>,
) -> bool {
    std::iter::successors(Some(entity), |current| {
        parents.get(*current).ok().map(ChildOf::parent)
    })
    .take(64)
    .any(|ancestor| {
        ui_nodes
            .get(ancestor)
            .is_ok_and(|node| node.display == Display::None)
    })
}

fn center_is_occluded(
    center: Vec2,
    element_stack: u32,
    window_rects: &[(UiFocusRect, u32)],
) -> bool {
    window_rects
        .iter()
        .any(|(rect, stack)| *stack > element_stack && rect.contains(center))
}

#[allow(clippy::too_many_arguments)]
fn menu_focusable_geometry(
    entity: Entity,
    transform: &UiGlobalTransform,
    computed: &ComputedNode,
    inherited_visibility: &InheritedVisibility,
    disabled: bool,
    parents: &Query<&ChildOf>,
    ui_nodes: &Query<&Node>,
    element_stack: u32,
    window_rects: &[(UiFocusRect, u32)],
) -> bool {
    if disabled
        || !inherited_visibility.get()
        || computed.is_empty()
        || has_display_none_ancestor(entity, parents, ui_nodes)
    {
        return false;
    }
    !center_is_occluded(transform.affine().translation, element_stack, window_rects)
}

#[derive(Clone, Copy, Default)]
struct MenuDirectionInput {
    just: Option<MenuDirection>,
    held: Option<MenuDirection>,
}

fn menu_direction_input(
    keyboard: &ButtonInput<KeyCode>,
    gamepads: &Query<(Entity, &Gamepad)>,
    native: &NativeControllerState,
    button_events: &[GamepadButtonStateChangedEvent],
    ignored_gamepad: Option<Entity>,
) -> MenuDirectionInput {
    let gamepad_just = |button| {
        gamepads.iter().any(|(entity, gamepad)| {
            Some(entity) != ignored_gamepad && gamepad.just_pressed(button)
        }) || gamepad_button_pressed_in_events(button_events, button, ignored_gamepad)
    };
    let gamepad_pressed = |button| {
        gamepads.iter().any(|(entity, gamepad)| {
            Some(entity) != ignored_gamepad && gamepad.pressed(button)
        })
    };
    let stick = gamepads
        .iter()
        .filter(|(entity, _)| Some(*entity) != ignored_gamepad)
        .map(|(_, gamepad)| {
            Vec2::new(
                gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0),
                gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0),
            )
        })
        .find(|axis| axis.length_squared() > 0.36)
        .or_else(|| (native.move_axis.length_squared() > 0.36).then_some(native.move_axis));

    let just = if keyboard.just_pressed(KeyCode::ArrowUp)
        || keyboard.just_pressed(KeyCode::KeyW)
        || gamepad_just(GamepadButton::DPadUp)
        || native.just_pressed(NativeButton::DPadUp)
    {
        Some(MenuDirection::Up)
    } else if keyboard.just_pressed(KeyCode::ArrowDown)
        || keyboard.just_pressed(KeyCode::KeyS)
        || gamepad_just(GamepadButton::DPadDown)
        || native.just_pressed(NativeButton::DPadDown)
    {
        Some(MenuDirection::Down)
    } else if keyboard.just_pressed(KeyCode::ArrowLeft)
        || keyboard.just_pressed(KeyCode::KeyA)
        || gamepad_just(GamepadButton::DPadLeft)
        || native.just_pressed(NativeButton::DPadLeft)
    {
        Some(MenuDirection::Left)
    } else if keyboard.just_pressed(KeyCode::ArrowRight)
        || keyboard.just_pressed(KeyCode::KeyD)
        || gamepad_just(GamepadButton::DPadRight)
        || native.just_pressed(NativeButton::DPadRight)
    {
        Some(MenuDirection::Right)
    } else {
        None
    };

    let held = if keyboard.pressed(KeyCode::ArrowUp)
        || keyboard.pressed(KeyCode::KeyW)
        || gamepad_pressed(GamepadButton::DPadUp)
        || native.pressed(NativeButton::DPadUp)
        || stick.is_some_and(|axis| axis.y > 0.6 && axis.y.abs() >= axis.x.abs())
    {
        Some(MenuDirection::Up)
    } else if keyboard.pressed(KeyCode::ArrowDown)
        || keyboard.pressed(KeyCode::KeyS)
        || gamepad_pressed(GamepadButton::DPadDown)
        || native.pressed(NativeButton::DPadDown)
        || stick.is_some_and(|axis| axis.y < -0.6 && axis.y.abs() >= axis.x.abs())
    {
        Some(MenuDirection::Down)
    } else if keyboard.pressed(KeyCode::ArrowLeft)
        || keyboard.pressed(KeyCode::KeyA)
        || gamepad_pressed(GamepadButton::DPadLeft)
        || native.pressed(NativeButton::DPadLeft)
        || stick.is_some_and(|axis| axis.x < -0.6 && axis.x.abs() > axis.y.abs())
    {
        Some(MenuDirection::Left)
    } else if keyboard.pressed(KeyCode::ArrowRight)
        || keyboard.pressed(KeyCode::KeyD)
        || gamepad_pressed(GamepadButton::DPadRight)
        || native.pressed(NativeButton::DPadRight)
        || stick.is_some_and(|axis| axis.x > 0.6 && axis.x.abs() > axis.y.abs())
    {
        Some(MenuDirection::Right)
    } else {
        None
    };

    MenuDirectionInput { just, held }
}

fn repeated_menu_direction(
    focus: &mut MenuFocus,
    input: MenuDirectionInput,
    dt: f32,
) -> Option<MenuDirection> {
    const INITIAL_REPEAT_DELAY: f32 = 0.34;
    const REPEAT_INTERVAL: f32 = 0.11;

    if let Some(direction) = input.just {
        focus.repeat_direction = Some(direction);
        focus.repeat_timer = INITIAL_REPEAT_DELAY;
        return Some(direction);
    }
    if input.held != focus.repeat_direction {
        focus.repeat_direction = input.held;
        focus.repeat_timer = INITIAL_REPEAT_DELAY;
        return input.held;
    }
    let Some(direction) = input.held else {
        focus.repeat_direction = None;
        focus.repeat_timer = 0.0;
        return None;
    };
    focus.repeat_timer -= dt;
    if focus.repeat_timer <= 0.0 {
        focus.repeat_timer = REPEAT_INTERVAL;
        Some(direction)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_back_navigation(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    native: Res<NativeControllerState>,
    assignments: Res<GamepadAssignments>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    settings: Res<GameSettings>,
    text_input: Res<AuthoringTextInputCapture>,
    unsaved_changes: Res<AuthoringUnsavedChanges>,
    state: Res<State<AppState>>,
    design_data: Res<CharacterDesignData>,
    imported_forge_return: Res<ImportedForgeReturnTarget>,
    mut main_menu: ResMut<MainMenuUiState>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
    victory_q: Query<Entity, With<VictoryRoot>>,
) {
    let (back_gamepad, back_native) = mapped_menu_face(&settings.bindings, FaceButton::East);
    let pressed_button_events = button_events.read().copied().collect::<Vec<_>>();
    let ignored_overlay = assignments.native_overlay_gamepad();
    if !is_shared_menu_state(state.get()) {
        return;
    }
    let back = !text_input.active
        && (keyboard.just_pressed(KeyCode::Escape)
            || gamepads
                .iter()
                .any(|(entity, gamepad)| {
                    Some(entity) != ignored_overlay && gamepad.just_pressed(back_gamepad)
                })
            || gamepad_button_pressed_in_events(
                &pressed_button_events,
                back_gamepad,
                ignored_overlay,
            )
            || native.just_pressed(back_native));
    if !back {
        return;
    }
    if unsaved_changes.active
        && matches!(
            state.get(),
            AppState::VehicleForge | AppState::SpaceshipForge
        )
    {
        return;
    }
    if let Some(destination) = forge_back_destination(state.get()) {
        next_state.set(destination);
        return;
    }

    match state.get() {
        AppState::MainMenu if main_menu.page == MainMenuPage::Settings => {
            main_menu.page = MainMenuPage::Home;
        }
        AppState::MainMenu | AppState::Playing | AppState::CharacterStudio => {}
        AppState::ProjectHub => next_state.set(AppState::MainMenu),
        AppState::CreatureForge
        | AppState::WeaponForge
        | AppState::VehicleForge
        | AppState::SpaceshipForge => unreachable!("Forge back routes returned above"),
        AppState::ImportedCharacterForge => next_state.set(match *imported_forge_return {
            ImportedForgeReturnTarget::ProjectHub => AppState::ProjectHub,
            ImportedForgeReturnTarget::PlayerSelect => AppState::PlayerSelect,
        }),
        AppState::PlayerSelect => next_state.set(AppState::MainMenu),
        AppState::CharacterDesign => next_state.set(match design_data.return_target {
            CharacterDesignReturnTarget::PlayerSelect => AppState::PlayerSelect,
            CharacterDesignReturnTarget::ChapterSelect => AppState::ChapterSelect,
        }),
        AppState::ChapterSelect => next_state.set(AppState::PlayerSelect),
        AppState::RobotGarage => next_state.set(AppState::ChapterSelect),
        AppState::Paused if pause_menu.page != PausePage::Main => {
            pause_menu.page = PausePage::Main;
            pause_menu.resume_armed = false;
            pause_menu.back_resume_requested = false;
        }
        AppState::Paused => {
            // State transitions are consumed by `pause_input_system` in
            // Update. Crossing schedules here would let the same Escape edge
            // resume in PreUpdate and immediately pause again in Update.
            pause_menu.back_resume_requested = true;
        }
        AppState::GameOver => next_state.set(AppState::MainMenu),
        AppState::Victory => {
            for entity in victory_q.iter() {
                commands.entity(entity).despawn();
            }
            next_state.set(AppState::MainMenu);
        }
    }
}

/// Resolve one logical menu face action for Bevy and native-controller input.
/// Keeping the pair together prevents the two controller backends from
/// drifting when the configured face layout swaps confirm and back.
fn mapped_menu_face(
    bindings: &ControlBindings,
    logical: FaceButton,
) -> (GamepadButton, NativeButton) {
    (
        mapped_gamepad_face(bindings, logical),
        mapped_native_face(bindings, logical),
    )
}

fn project_hub_forge_destination(action: ProjectHubAction) -> Option<AppState> {
    match action {
        ProjectHubAction::CreatureForge => Some(AppState::CreatureForge),
        ProjectHubAction::WeaponForge => Some(AppState::WeaponForge),
        ProjectHubAction::VehicleForge => Some(AppState::VehicleForge),
        ProjectHubAction::SpaceshipForge => Some(AppState::SpaceshipForge),
        _ => None,
    }
}

fn forge_back_destination(state: &AppState) -> Option<AppState> {
    matches!(
        state,
        AppState::CreatureForge
            | AppState::WeaponForge
            | AppState::VehicleForge
            | AppState::SpaceshipForge
    )
    .then_some(AppState::ProjectHub)
}

fn default_menu_focus(buttons: &[(Entity, Vec2)]) -> Option<Entity> {
    buttons
        .iter()
        .min_by(|(_, a), (_, b)| a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x)))
        .map(|(entity, _)| *entity)
}

fn directional_menu_focus(
    current: Entity,
    direction: MenuDirection,
    buttons: &[(Entity, Vec2)],
) -> Option<Entity> {
    let origin = buttons
        .iter()
        .find_map(|(entity, position)| (*entity == current).then_some(*position))?;
    let axis = direction.vector();

    buttons
        .iter()
        .filter(|(entity, _)| *entity != current)
        .filter_map(|(entity, position)| {
            let delta = *position - origin;
            let forward = delta.dot(axis);
            if forward <= 1.0 {
                return None;
            }
            let perpendicular = (delta - axis * forward).length();
            let score = forward + perpendicular * 2.5;
            Some((*entity, score))
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(entity, _)| entity)
}

fn menu_focus_style(
    focus: Res<MenuFocus>,
    settings: Res<GameSettings>,
    mut buttons: Query<
        (
            Entity,
            &Interaction,
            &mut MenuButtonPalette,
            Option<&MenuButtonDisabled>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        (With<Button>, With<MenuFocusable>),
    >,
) {
    for (entity, interaction, mut palette, disabled, mut background, mut border) in
        buttons.iter_mut()
    {
        if palette
            .applied_background
            .is_none_or(|applied| applied != *background)
        {
            palette.background = *background;
        }
        if palette
            .applied_border
            .is_none_or(|applied| applied != *border)
        {
            palette.border = *border;
        }

        let (styled_background, styled_border) = if disabled.is_some() {
            (
                BackgroundColor(Color::srgb(0.07, 0.08, 0.11)),
                BorderColor::all(Color::srgb(0.28, 0.30, 0.36)),
            )
        } else if *interaction == Interaction::Pressed {
            (
                BackgroundColor(Color::srgb(0.98, 0.62, 0.14)),
                BorderColor::all(Color::WHITE),
            )
        } else if focus.entity == Some(entity)
            || (focus.pointer_active && *interaction == Interaction::Hovered)
        {
            (
                BackgroundColor(if settings.high_contrast_ui {
                    Color::BLACK
                } else {
                    Color::srgb(0.12, 0.48, 0.82)
                }),
                BorderColor::all(if settings.high_contrast_ui {
                    Color::WHITE
                } else {
                    Color::srgb(1.0, 0.9, 0.34)
                }),
            )
        } else {
            (palette.background, palette.border)
        };

        *background = styled_background;
        *border = styled_border;
        palette.applied_background =
            (styled_background != palette.background).then_some(styled_background);
        palette.applied_border = (styled_border != palette.border).then_some(styled_border);
    }
}

fn keep_menu_focus_in_view(
    mut focus: ResMut<MenuFocus>,
    focused_q: Query<(&UiGlobalTransform, &ComputedNode), With<MenuFocusable>>,
    parents: Query<&ChildOf>,
    mut panel_q: Query<
        (
            Entity,
            &UiGlobalTransform,
            &ComputedNode,
            &mut ScrollPosition,
        ),
        With<MenuScrollPanel>,
    >,
) {
    if focus.layout_reveal_frames > 0 {
        focus.layout_reveal_frames -= 1;
        focus.reveal_requested = true;
    }
    if !focus.reveal_requested {
        return;
    }
    focus.reveal_requested = false;
    let Some(entity) = focus.entity else {
        return;
    };
    let Ok((button_transform, button_node)) = focused_q.get(entity) else {
        return;
    };

    // Scroll only the nearest owning surface. Nested pages (for example Pause
    // Settings inside the full-screen pause scroller) must not move twice.
    let Some(panel_entity) = std::iter::successors(Some(entity), |current| {
        parents.get(*current).ok().map(|child_of| child_of.parent())
    })
    .take(64)
    .find(|ancestor| panel_q.contains(*ancestor)) else {
        return;
    };
    let Ok((_, panel_transform, panel_node, mut scroll)) = panel_q.get_mut(panel_entity) else {
        return;
    };
    let max_y = menu_scroll_max(panel_node);
    if max_y <= 0.0 {
        return;
    }
    let panel_center = panel_transform.affine().translation.y;
    let button_center = button_transform.affine().translation.y;
    let panel_half = panel_node.size().y * 0.5;
    let button_half = button_node.size().y * 0.5;
    let margin = 18.0;
    let panel_min = panel_center - panel_half + margin;
    let panel_max = panel_center + panel_half - margin;
    let button_min = button_center - button_half;
    let button_max = button_center + button_half;

    let delta = focus_reveal_scroll_delta(panel_min, panel_max, button_min, button_max)
        * panel_node.inverse_scale_factor();
    scroll.y = (scroll.y + delta).clamp(0.0, max_y);
}

fn menu_scroll_max(computed: &ComputedNode) -> f32 {
    ((computed.content_size().y - computed.size().y) * computed.inverse_scale_factor()).max(0.0)
}

fn focus_reveal_scroll_delta(
    viewport_min: f32,
    viewport_max: f32,
    item_min: f32,
    item_max: f32,
) -> f32 {
    if item_min < viewport_min {
        item_min - viewport_min
    } else if item_max > viewport_max {
        item_max - viewport_max
    } else {
        0.0
    }
}

// ── UI Fallback Camera ────────────────────────────────────────────────────────
fn spawn_menu_camera(mut commands: Commands, existing: Query<Entity, With<MenuCamera>>) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        Camera3dBundle {
            camera: Camera {
                order: -100,
                ..default()
            },
            ..default()
        },
        MenuCamera,
    ));
}

// ── Menu Setup ────────────────────────────────────────────────────────────────
fn despawn_menu(mut commands: Commands, q: Query<Entity, With<MainMenuRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
}

fn cleanup_play_ui_for_menu(
    mut commands: Commands,
    hud_q: Query<Entity, With<HudRoot>>,
    pause_q: Query<Entity, With<PauseRoot>>,
    game_over_q: Query<Entity, With<GameOverRoot>>,
    crafting_q: Query<Entity, With<CraftingPanelRoot>>,
) {
    for entity in hud_q
        .iter()
        .chain(pause_q.iter())
        .chain(game_over_q.iter())
        .chain(crafting_q.iter())
    {
        commands.entity(entity).despawn();
    }
}

fn setup_main_menu(
    mut commands: Commands,
    theme: Res<UiTheme>,
    catalog: Res<UiTextCatalog>,
    settings: Res<GameSettings>,
    mut menu: ResMut<MainMenuUiState>,
) {
    menu.page = MainMenuPage::Home;
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(18.0),
                padding: UiRect::all(Val::Px(32.0)),
                ..default()
            },
            BackgroundColor(theme.canvas),
            MainMenuRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(18.0),
                    ..default()
                },
                Visibility::Visible,
                MainMenuPagePanel(MainMenuPage::Home),
            ))
            .with_children(|page| {
                page.spawn((
                    Text::new(catalog.text(UiTextKey::GameTitle)),
                    TextFont {
                        font_size: FontSize::Px(72.0),
                        ..default()
                    },
                    TextColor(theme.objective),
                ));
                page.spawn((
                    Text::new(catalog.text(UiTextKey::GameSubtitle)),
                    TextFont {
                        font_size: FontSize::Px(24.0),
                        ..default()
                    },
                    TextColor(theme.text_muted),
                ));
                page.spawn(Node {
                    height: Val::Px(24.0),
                    ..default()
                });
                spawn_main_menu_button(
                    page,
                    catalog.text(UiTextKey::StartGame),
                    theme.play,
                    theme.player_accent(0),
                    StartButton,
                );
                // The Project Hub is the Designer edition's front door; a
                // consumer build has no authoring entry points at all.
                if cfg!(feature = "designer") {
                    spawn_main_menu_button(
                        page,
                        catalog.text(UiTextKey::StartEditor),
                        theme.create,
                        theme.player_accent(1),
                        EditorStartButton,
                    );
                }
                spawn_main_menu_button(
                    page,
                    catalog.text(UiTextKey::Settings),
                    Color::srgb(0.18, 0.28, 0.48),
                    theme.energy,
                    MainMenuSettingsButton,
                );
            });

            root.spawn((
                Node {
                    max_width: Val::Px(620.0),
                    max_height: Val::Percent(90.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(12.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(theme.panel),
                Visibility::Hidden,
                ScrollPosition::default(),
                MenuScrollPanel,
                MainMenuPagePanel(MainMenuPage::Settings),
            ))
            .with_children(|page| {
                page.spawn((
                    Text::new(catalog.text(UiTextKey::SettingsAccessibility)),
                    TextFont {
                        font_size: FontSize::Px(32.0),
                        ..default()
                    },
                    TextColor(theme.objective),
                ));
                spawn_settings_controls(page, &settings, &theme);
                spawn_main_menu_button(
                    page,
                    catalog.text(UiTextKey::Back),
                    Color::srgb(0.20, 0.24, 0.36),
                    theme.energy,
                    MainMenuSettingsBackButton,
                );
            });
        });
}

fn spawn_main_menu_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    background: Color,
    border: Color,
    marker: impl Component,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(260.0),
                height: Val::Px(58.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(background),
            BorderColor::all(border),
            marker,
        ))
        .with_child((
            Text::new(label.to_string()),
            TextFont {
                font_size: FontSize::Px(22.0),
                ..default()
            },
            TextColor(Color::WHITE),
        ));
}

fn menu_start_button(
    interaction_q: Query<&Interaction, (Changed<Interaction>, With<StartButton>)>,
    editor_q: Query<
        &Interaction,
        (
            Changed<Interaction>,
            With<EditorStartButton>,
            Without<StartButton>,
        ),
    >,
    settings_q: Query<
        &Interaction,
        (
            Changed<Interaction>,
            With<MainMenuSettingsButton>,
            Without<StartButton>,
            Without<EditorStartButton>,
        ),
    >,
    settings_back_q: Query<
        &Interaction,
        (
            Changed<Interaction>,
            With<MainMenuSettingsBackButton>,
            Without<StartButton>,
            Without<EditorStartButton>,
            Without<MainMenuSettingsButton>,
        ),
    >,
    mut menu: ResMut<MainMenuUiState>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::PlayerSelect);
        }
    }
    for interaction in editor_q.iter() {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::ProjectHub);
        }
    }
    for interaction in settings_q.iter() {
        if *interaction == Interaction::Pressed {
            menu.page = MainMenuPage::Settings;
        }
    }
    for interaction in settings_back_q.iter() {
        if *interaction == Interaction::Pressed {
            menu.page = MainMenuPage::Home;
        }
    }
}

fn update_main_menu_page_visibility(
    menu: Res<MainMenuUiState>,
    mut panels: Query<(&MainMenuPagePanel, &mut Visibility, &mut Node)>,
) {
    if !menu.is_changed() {
        return;
    }
    for (panel, mut visibility, mut node) in panels.iter_mut() {
        let active = panel.0 == menu.page;
        *visibility = if active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        node.display = if active { Display::Flex } else { Display::None };
    }
}

fn project_hub_root_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        min_height: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::FlexStart,
        row_gap: Val::Px(12.0),
        padding: UiRect::axes(Val::Px(24.0), Val::Px(20.0)),
        overflow: Overflow::scroll_y(),
        ..default()
    }
}

fn setup_project_hub(mut commands: Commands, registry: Res<ForgeProjectRegistry>) {
    commands
        .spawn((
            project_hub_root_node(),
            BackgroundColor(Color::srgba(0.012, 0.014, 0.034, 1.0)),
            ScrollPosition::default(),
            MenuScrollPanel,
            ProjectHubRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("STARFALL FORGE — PROJECT HUB"),
                TextFont {
                    font_size: FontSize::Px(42.0),
                    ..default()
                },
                TextColor(Color::srgb(0.78, 0.58, 1.0)),
            ));
            root.spawn((
                Text::new("Continue your active project, open another, or start a new one."),
                TextFont {
                    font_size: FontSize::Px(17.0),
                    ..default()
                },
                TextColor(Color::srgb(0.72, 0.82, 1.0)),
            ));
            let status = match registry.active_entry() {
                Some(entry) => format!(
                    "Active: {}\n{}\nAtomic save • recovery snapshots • draft/published validation",
                    entry.name,
                    entry.path.display()
                ),
                None => "No projects yet — NEW PROJECT creates your first one.".to_string(),
            };
            root.spawn((
                Text::new(status),
                ProjectHubStatusText,
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextLayout::justify(Justify::Center),
                TextColor(Color::srgb(0.88, 0.90, 1.0)),
            ));
            if let Some(entry) = registry.active_entry() {
                spawn_project_hub_button(
                    root,
                    format!("CONTINUE {}", entry.name.to_uppercase()),
                    ProjectHubAction::OpenCurrent,
                    Color::srgb(0.28, 0.16, 0.54),
                );
            }
            for (index, entry) in registry.projects.iter().enumerate() {
                if registry.active.as_deref() == Some(entry.path.as_path()) {
                    continue;
                }
                spawn_project_hub_button(
                    root,
                    format!("OPEN {}", entry.name.to_uppercase()),
                    ProjectHubAction::OpenProject(index),
                    Color::srgb(0.20, 0.20, 0.46),
                );
            }
            spawn_project_hub_button(
                root,
                "NEW PROJECT".to_string(),
                ProjectHubAction::NewProject,
                Color::srgb(0.14, 0.36, 0.30),
            );
            for (label, action, color) in project_hub_authoring_actions() {
                spawn_project_hub_button(root, label.to_string(), action, color);
            }
            spawn_project_hub_button(
                root,
                "PUBLISH TO GAME".to_string(),
                ProjectHubAction::Publish,
                Color::srgb(0.42, 0.30, 0.10),
            );
            spawn_project_hub_button(
                root,
                "BACK".to_string(),
                ProjectHubAction::Back,
                Color::srgb(0.22, 0.26, 0.38),
            );
        });
}

fn spawn_project_hub_button(
    parent: &mut ChildSpawnerCommands,
    label: String,
    action: ProjectHubAction,
    color: Color,
) {
    parent
        .spawn((
            Button,
            project_hub_button_node(),
            BackgroundColor(color),
            BorderColor::all(Color::srgb(0.62, 0.48, 0.86)),
            ProjectHubButton(action),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn project_hub_requires_active_project(action: ProjectHubAction) -> bool {
    matches!(
        action,
        ProjectHubAction::CreatureForge
            | ProjectHubAction::WeaponForge
            | ProjectHubAction::VehicleForge
            | ProjectHubAction::SpaceshipForge
            | ProjectHubAction::Publish
    )
}

fn project_hub_button_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        max_width: Val::Px(310.0),
        min_height: Val::Px(52.0),
        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        border: UiRect::all(Val::Px(2.0)),
        ..default()
    }
}

fn project_hub_action_system(
    interactions: Query<(&Interaction, &ProjectHubButton), (Changed<Interaction>, With<Button>)>,
    mut registry: ResMut<ForgeProjectRegistry>,
    mut imported_forge_return: ResMut<ImportedForgeReturnTarget>,
    mut next_state: ResMut<NextState<AppState>>,
    mut next_tool_mode: ResMut<NextState<EngineToolMode>>,
    mut hub_status: Query<&mut Text, With<ProjectHubStatusText>>,
    mut craft_reload: MessageWriter<ReloadPublishedCraftCatalogs>,
) {
    for (interaction, button) in interactions.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if registry.active.is_none() && project_hub_requires_active_project(button.0) {
            for mut text in hub_status.iter_mut() {
                *text = Text::new("Create or open a project before entering an authoring Forge.");
            }
            continue;
        }
        let enter_editor =
            |next_state: &mut NextState<AppState>,
             next_tool_mode: &mut NextState<EngineToolMode>| {
                next_state.set(AppState::Playing);
                next_tool_mode.set(EngineToolMode::Editing);
            };
        match button.0 {
            ProjectHubAction::OpenCurrent => {
                if registry.active.is_some() {
                    enter_editor(&mut next_state, &mut next_tool_mode);
                }
            }
            ProjectHubAction::OpenProject(index) => {
                let Some(path) = registry.projects.get(index).map(|entry| entry.path.clone())
                else {
                    continue;
                };
                registry.set_active(&path);
                if let Err(error) = registry.save() {
                    warn!("Could not persist project registry: {error}");
                }
                enter_editor(&mut next_state, &mut next_tool_mode);
            }
            ProjectHubAction::NewProject => match registry.create_new_project() {
                Ok(entry) => {
                    if let Err(error) = registry.save() {
                        warn!("Could not persist project registry: {error}");
                    }
                    info!("Created project {} at {}", entry.name, entry.path.display());
                    enter_editor(&mut next_state, &mut next_tool_mode);
                }
                Err(error) => warn!("Could not create a new project: {error}"),
            },
            ProjectHubAction::CreatureForge
            | ProjectHubAction::WeaponForge
            | ProjectHubAction::VehicleForge
            | ProjectHubAction::SpaceshipForge => {
                let destination = project_hub_forge_destination(button.0)
                    .expect("each Forge action has one AppState destination");
                next_state.set(destination);
            }
            ProjectHubAction::ImportedCharacterForge => {
                *imported_forge_return = ImportedForgeReturnTarget::ProjectHub;
                next_state.set(AppState::ImportedCharacterForge)
            }
            ProjectHubAction::Publish => {
                let outcome = crate::engine_tools::publish::publish_active_project(&registry);
                let message = match outcome {
                    Ok(report) => {
                        craft_reload.write(ReloadPublishedCraftCatalogs);
                        format!("PUBLISHED ✓  {}", report.summary())
                    }
                    Err(error) => format!("PUBLISH FAILED — {error}"),
                };
                info!("{message}");
                for mut text in hub_status.iter_mut() {
                    *text = Text::new(message.clone());
                }
            }
            ProjectHubAction::Back => next_state.set(AppState::MainMenu),
        }
    }
}

fn despawn_project_hub(mut commands: Commands, roots: Query<Entity, With<ProjectHubRoot>>) {
    for entity in roots.iter() {
        commands.entity(entity).despawn();
    }
}

fn setup_pause_menu(
    mut commands: Commands,
    mut menu: ResMut<PauseMenuState>,
    settings: Res<GameSettings>,
    progress: Res<ChapterProgress>,
    world_site_registry: Res<WorldSiteRegistry>,
    published_recipes: Res<PublishedProceduralRecipeCatalog>,
    theme: Res<UiTheme>,
    player_q: Query<(&PlayerIndex, &Transform), With<Player>>,
) {
    menu.page = menu.requested_page.take().unwrap_or(PausePage::Main);
    menu.resume_lockout = 0.20;
    menu.resume_armed = false;
    menu.back_resume_requested = false;
    let chapters = all_chapters();
    let mut player_positions = player_q
        .iter()
        .map(|(index, transform)| (index.0, transform.translation))
        .collect::<Vec<_>>();
    player_positions.sort_by_key(|(index, _)| *index);
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::FlexStart,
                row_gap: Val::Px(12.0),
                padding: UiRect::all(Val::Px(34.0)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
            ScrollPosition::default(),
            MenuScrollPanel,
            PauseRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("PAUSED"),
                TextFont {
                    font_size: FontSize::Px(58.0),
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.95, 1.0)),
            ));
            root.spawn((
                Text::new("Any active player can pause. Save and title actions affect the party."),
                TextFont {
                    font_size: FontSize::Px(19.0),
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.85, 1.0)),
            ));
            root.spawn((
                Text::new("D-PAD / LEFT STICK: navigate   A: select   B: back / resume"),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::srgb(0.68, 0.78, 0.92)),
                UiPromptText(UiPromptKind::PauseNavigation),
            ));
            root.spawn(Node {
                height: Val::Px(8.0),
                ..default()
            });

            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                Visibility::Visible,
                PausePagePanel(PausePage::Main),
            ))
            .with_children(|page| {
                for (label, action, color) in [
                    ("RESUME", PauseAction::Resume, Color::srgb(0.0, 0.42, 0.74)),
                    (
                        "WORLD MAP / FAST TRAVEL",
                        PauseAction::Map,
                        Color::srgb(0.10, 0.38, 0.58),
                    ),
                    ("SAVE GAME", PauseAction::Save, Color::srgb(0.10, 0.48, 0.28)),
                    ("MAIN MENU", PauseAction::Title, Color::srgb(0.48, 0.18, 0.18)),
                    (
                        "CONTROLS",
                        PauseAction::Controls,
                        Color::srgb(0.22, 0.24, 0.52),
                    ),
                    ("SHOP", PauseAction::Shop, Color::srgb(0.22, 0.38, 0.44)),
                    ("SETTINGS", PauseAction::Settings, Color::srgb(0.36, 0.28, 0.52)),
                ] {
                    spawn_pause_button(page, label, action, color);
                }
            });

            root.spawn((
                Node {
                    width: Val::Px(760.0),
                    height: Val::Px(430.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(8.0),
                    ..default()
                },
                Visibility::Hidden,
                PausePagePanel(PausePage::Map),
            ))
            .with_children(|page| {
                page.spawn((
                    Text::new("WORLD MAP / FAST TRAVEL"),
                    TextFont {
                        font_size: FontSize::Px(28.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.95, 1.0)),
                ));
                page.spawn((
                    Text::new(
                        "P1–P4 show the party. Select any travel marker; U markers come from published player levels.",
                    ),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.70, 0.86, 1.0)),
                ));
                page.spawn(Node {
                    width: Val::Px(700.0),
                    height: Val::Px(330.0),
                    ..default()
                })
                .with_children(|map_host| {
                    spawn_fast_travel_map(
                        map_host,
                        &chapters,
                        &progress,
                        &world_site_registry,
                        &published_recipes,
                        &player_positions,
                        &theme,
                    );
                });
                spawn_pause_button(page, "BACK", PauseAction::Back, Color::srgb(0.0, 0.42, 0.74));
            });

            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0),
                    max_width: Val::Px(760.0),
                    ..default()
                },
                Visibility::Hidden,
                PausePagePanel(PausePage::Controls),
            ))
            .with_children(|page| {
                page.spawn((
                    Text::new("CONTROLS / TIPS"),
                    TextFont {
                        font_size: FontSize::Px(30.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.95, 1.0)),
                ));
                page.spawn((
                    Text::new(
                        "Movement, aiming, star-beam combat, traversal tools, vehicle boarding, interaction, map use, weapon swaps, and special tools are available during play.\nCamera: P (keyboard) or Select + D-pad Left (controller) toggles your view between first and third person.\nTraversal supports wall slide, wall-jump, ledge hang, zip, swing, glide, hover, air dash, and hoverboard movement.",
                    ),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.72, 0.82, 1.0)),
                ));
                page.spawn((
                    Text::new(
                        "Boat: board near a dock, steer along the wake, and dock at the city or island before disembarking. Hidden rooms can contain puzzle props, rewards, armor, XP, and special ability upgrades.",
                    ),
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.86, 0.90, 1.0)),
                ));
                spawn_pause_button(page, "BACK", PauseAction::Back, Color::srgb(0.0, 0.42, 0.74));
            });

            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    row_gap: Val::Px(8.0),
                    max_width: Val::Px(920.0),
                    padding: UiRect::all(Val::Px(18.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.030, 0.034, 0.042, 0.92)),
                Visibility::Hidden,
                PausePagePanel(PausePage::Shop),
            ))
            .with_children(|page| {
                page.spawn((
                    Text::new("STAR SHOP"),
                    TextFont {
                        font_size: FontSize::Px(30.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.95, 1.0)),
                ));
                page.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.94, 0.76, 0.36)),
                    ShopHeaderText,
                ));
                page.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|tabs| {
                    for category in ShopCategory::ALL {
                        spawn_shop_button(
                            tabs,
                            category.label().to_uppercase(),
                            ShopAction::Category(category),
                            Color::srgb(0.16, 0.30, 0.42),
                        );
                    }
                    spawn_shop_button(
                        tabs,
                        "SWITCH PLAYER".to_string(),
                        ShopAction::OwnerCycle,
                        Color::srgb(0.30, 0.24, 0.44),
                    );
                });
                page.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexStart,
                    column_gap: Val::Px(14.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(6.0),
                            ..default()
                        },
                        ShopCardList,
                    ));
                    row.spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(6.0),
                            max_width: Val::Px(360.0),
                            padding: UiRect::all(Val::Px(10.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.06, 0.08, 0.11, 0.9)),
                        ShopDetailPanel,
                    ));
                });
                spawn_pause_button(page, "BACK", PauseAction::Back, Color::srgb(0.0, 0.42, 0.74));
            });

            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0),
                    max_height: Val::Percent(82.0),
                    padding: UiRect::horizontal(Val::Px(12.0)),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                Visibility::Hidden,
                ScrollPosition::default(),
                MenuScrollPanel,
                PausePagePanel(PausePage::Settings),
            ))
            .with_children(|page| {
                page.spawn((
                    Text::new("SETTINGS"),
                    TextFont { font_size: FontSize::Px(30.0), ..default() },
                    TextColor(Color::srgb(0.9, 0.95, 1.0)),
                ));
                page.spawn((
                    Text::new(difficulty_text(&settings)),
                    TextFont { font_size: FontSize::Px(16.0), ..default() },
                    TextColor(Color::srgb(0.72, 0.82, 1.0)),
                    DifficultyText,
                ));
                page.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_settings_button(
                        row,
                        "EASY",
                        SettingsAction::Difficulty(DifficultyChoice::Easy),
                    );
                    spawn_settings_button(
                        row,
                        "NORMAL",
                        SettingsAction::Difficulty(DifficultyChoice::Normal),
                    );
                    spawn_settings_button(
                        row,
                        "HARD",
                        SettingsAction::Difficulty(DifficultyChoice::Hard),
                    );
                });
                page.spawn((
                    Text::new(volume_text(&settings)),
                    TextFont { font_size: FontSize::Px(16.0), ..default() },
                    TextColor(Color::srgb(0.72, 0.82, 1.0)),
                    VolumeText,
                ));
                page.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    justify_content: JustifyContent::Center,
                    max_width: Val::Px(420.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_settings_button(row, "MUSIC -", SettingsAction::MusicDown);
                    spawn_settings_button(row, "MUSIC +", SettingsAction::MusicUp);
                    spawn_settings_button(row, "SFX -", SettingsAction::SfxDown);
                    spawn_settings_button(row, "SFX +", SettingsAction::SfxUp);
                    spawn_settings_button(row, "RUMBLE", SettingsAction::ToggleRumble);
                });
                page.spawn((
                    Text::new(interface_layout_text(&settings)),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.72, 0.82, 1.0)),
                    InterfaceLayoutText,
                ));

                // ── Controls ───────────────────────────────────────────────
                page.spawn((
                    Text::new(control_bindings_text(&settings, &PendingRebind::default())),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.86, 0.90, 0.72)),
                    ControlBindingsText,
                ));
                page.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    justify_content: JustifyContent::Center,
                    max_width: Val::Px(420.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_settings_button(row, "FACE LAYOUT", SettingsAction::FaceLayoutNext);
                    spawn_settings_button(row, "INVERT Y", SettingsAction::ToggleInvertLook);
                    spawn_settings_button(row, "RESET KEYS", SettingsAction::ResetKeys);
                });
                page.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    justify_content: JustifyContent::Center,
                    max_width: Val::Px(420.0),
                    ..default()
                })
                .with_children(|row| {
                    // One rebind button per action; pressing it arms capture.
                    for action in crate::engine::bindings::KeyAction::ALL {
                        spawn_settings_button(
                            row,
                            action.label(),
                            SettingsAction::RebindKey(action),
                        );
                    }
                });
                page.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    justify_content: JustifyContent::Center,
                    max_width: Val::Px(520.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_settings_button(row, "UI SCALE -", SettingsAction::UiScaleDown);
                    spawn_settings_button(row, "UI SCALE +", SettingsAction::UiScaleUp);
                    spawn_settings_button(row, "SAFE AREA -", SettingsAction::SafeAreaDown);
                    spawn_settings_button(row, "SAFE AREA +", SettingsAction::SafeAreaUp);
                    spawn_settings_button(
                        row,
                        "CONTROLLER GLYPHS",
                        SettingsAction::ControllerGlyphStyleNext,
                    );
                });
                page.spawn((
                    Text::new(accessibility_settings_text(&settings)),
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.72, 0.82, 1.0)),
                    AccessibilitySettingsText,
                ));
                spawn_settings_button_grid(page, |row| {
                    spawn_settings_button(
                        row,
                        "DAMAGE NUMBERS",
                        SettingsAction::ToggleDamageNumbers,
                    );
                    spawn_settings_button(
                        row,
                        "HIGH CONTRAST",
                        SettingsAction::ToggleHighContrast,
                    );
                    spawn_settings_button(
                        row,
                        "REDUCED MOTION",
                        SettingsAction::ToggleReducedMotion,
                    );
                    spawn_settings_button(row, "SUBTITLES", SettingsAction::ToggleSubtitles);
                });
                spawn_pause_button(page, "BACK", PauseAction::Back, Color::srgb(0.0, 0.42, 0.74));
            });
        });
}

fn spawn_pause_button(
    parent: &mut ChildSpawnerCommands,
    label: &'static str,
    action: PauseAction,
    color: Color,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(220.0),
                height: Val::Px(48.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(color),
            PauseMenuButton(action),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn spawn_settings_button(
    parent: &mut ChildSpawnerCommands,
    label: &'static str,
    action: SettingsAction,
) {
    parent
        .spawn((
            Button,
            Node {
                min_width: Val::Px(92.0),
                height: Val::Px(34.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.16, 0.22)),
            BorderColor::all(Color::srgb(0.28, 0.42, 0.56)),
            SettingsButton(action),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn spawn_settings_controls(
    parent: &mut ChildSpawnerCommands,
    settings: &GameSettings,
    theme: &UiTheme,
) {
    parent.spawn((
        Text::new(difficulty_text(settings)),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(theme.text_primary),
        DifficultyText,
    ));
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            spawn_settings_button(
                row,
                "EASY",
                SettingsAction::Difficulty(DifficultyChoice::Easy),
            );
            spawn_settings_button(
                row,
                "NORMAL",
                SettingsAction::Difficulty(DifficultyChoice::Normal),
            );
            spawn_settings_button(
                row,
                "HARD",
                SettingsAction::Difficulty(DifficultyChoice::Hard),
            );
        });
    parent.spawn((
        Text::new(volume_text(settings)),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(theme.text_primary),
        VolumeText,
    ));
    spawn_settings_button_grid(parent, |row| {
        spawn_settings_button(row, "MUSIC -", SettingsAction::MusicDown);
        spawn_settings_button(row, "MUSIC +", SettingsAction::MusicUp);
        spawn_settings_button(row, "SFX -", SettingsAction::SfxDown);
        spawn_settings_button(row, "SFX +", SettingsAction::SfxUp);
        spawn_settings_button(row, "RUMBLE", SettingsAction::ToggleRumble);
    });
    parent.spawn((
        Text::new(interface_layout_text(settings)),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(theme.text_primary),
        InterfaceLayoutText,
    ));
    spawn_settings_button_grid(parent, |row| {
        spawn_settings_button(row, "UI SCALE -", SettingsAction::UiScaleDown);
        spawn_settings_button(row, "UI SCALE +", SettingsAction::UiScaleUp);
        spawn_settings_button(row, "SAFE AREA -", SettingsAction::SafeAreaDown);
        spawn_settings_button(row, "SAFE AREA +", SettingsAction::SafeAreaUp);
        spawn_settings_button(
            row,
            "CONTROLLER GLYPHS",
            SettingsAction::ControllerGlyphStyleNext,
        );
    });
    parent.spawn((
        Text::new(accessibility_settings_text(settings)),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(theme.text_primary),
        AccessibilitySettingsText,
    ));
    spawn_settings_button_grid(parent, |row| {
        spawn_settings_button(row, "DAMAGE NUMBERS", SettingsAction::ToggleDamageNumbers);
        spawn_settings_button(row, "HIGH CONTRAST", SettingsAction::ToggleHighContrast);
        spawn_settings_button(row, "REDUCED MOTION", SettingsAction::ToggleReducedMotion);
        spawn_settings_button(row, "SUBTITLES", SettingsAction::ToggleSubtitles);
    });
}

fn spawn_settings_button_grid(
    parent: &mut ChildSpawnerCommands,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(8.0),
            row_gap: Val::Px(8.0),
            justify_content: JustifyContent::Center,
            max_width: Val::Px(540.0),
            ..default()
        })
        .with_children(build);
}

fn spawn_shop_button(
    parent: &mut ChildSpawnerCommands,
    label: String,
    action: ShopAction,
    color: Color,
) {
    parent
        .spawn((
            Button,
            Node {
                min_width: Val::Px(120.0),
                height: Val::Px(38.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(color),
            ShopButton(action),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn shop_card_color(state: shop_transactions::CardState) -> Color {
    use shop_transactions::CardState;
    match state {
        CardState::Equipped => Color::srgb(0.10, 0.40, 0.22),
        CardState::Owned => Color::srgb(0.12, 0.28, 0.46),
        CardState::Affordable => Color::srgb(0.42, 0.32, 0.10),
        CardState::Unaffordable => Color::srgb(0.22, 0.22, 0.26),
        CardState::Locked => Color::srgb(0.30, 0.18, 0.14),
    }
}

fn shop_preview_line(item: &crate::resources::ShopItem) -> Option<String> {
    item.preview_loadout.map(|loadout| {
        format!(
            "Preview: {} / {} / {} / {}",
            loadout.arms.label(),
            loadout.legs.label(),
            loadout.shoulders.label(),
            loadout.head.label()
        )
    })
}

/// Executes PX3 shop button presses: category tabs, owner cycling, card
/// selection, and the confirmed buy/equip/unequip transactions. All rules run
/// through `shop_transactions`, so the UI cannot bypass funds/duplicate
/// protection.
fn shop_action_system(
    button_q: Query<(&Interaction, &ShopButton), (Changed<Interaction>, With<Button>)>,
    mut ui: ResMut<ShopUiState>,
    catalog: Res<ShopCatalog>,
    mut player_q: Query<(&PlayerIndex, &mut PlayerStats, &mut PlayerProgression), With<Player>>,
    mut select: ResMut<PlayerSelectState>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let mut action = None;
    for (interaction, button) in button_q.iter() {
        if *interaction == Interaction::Pressed {
            action = Some(button.0);
        }
    }
    let Some(action) = action else {
        return;
    };
    ui.dirty = true;

    let mut toast = |text: String| {
        msg_ev.write(UiMessageEvent {
            text,
            duration: 2.0,
        });
    };

    match action {
        ShopAction::Category(category) => {
            ui.category = category;
            ui.selected = None;
        }
        ShopAction::Select(item_id) => {
            ui.selected = Some(item_id);
        }
        ShopAction::CloseDetail => {
            ui.selected = None;
        }
        ShopAction::OwnerCycle => {
            let mut indices: Vec<u8> = player_q.iter().map(|(idx, _, _)| idx.0).collect();
            indices.sort_unstable();
            if let Some(next) = indices
                .iter()
                .find(|idx| **idx > ui.owner)
                .or_else(|| indices.first())
            {
                ui.owner = *next;
            }
        }
        ShopAction::ConfirmBuy | ShopAction::Equip | ShopAction::Unequip => {
            let Some(item) = ui
                .selected
                .and_then(|id| catalog.items.iter().find(|item| item.id == id))
            else {
                return;
            };
            let Some((_, mut stats, mut progression)) =
                player_q.iter_mut().find(|(idx, _, _)| idx.0 == ui.owner)
            else {
                toast("That player is not in the game right now.".to_string());
                return;
            };
            let result = match action {
                ShopAction::ConfirmBuy => {
                    shop_transactions::buy(&mut progression.shop, &mut stats.credits, item)
                        .map(|()| format!("Purchased {}!", item.name))
                }
                ShopAction::Equip => shop_transactions::equip(&mut progression.shop, item)
                    .map(|()| format!("Equipped {}.", item.name)),
                _ => shop_transactions::unequip(&mut progression.shop, item)
                    .map(|()| format!("Unequipped {}.", item.name)),
            };
            match result {
                Ok(message) => {
                    // Outfit chassis presets apply to the owner's slot config so
                    // the equipped look persists into the next spawn.
                    if item.category == ShopCategory::Outfits {
                        if let Some(slot) = select.slots.get_mut(ui.owner as usize) {
                            slot.part_loadout = match action {
                                ShopAction::Equip => item.preview_loadout,
                                ShopAction::Unequip => None,
                                _ => slot.part_loadout,
                            };
                        }
                    }
                    toast(message);
                }
                Err(err) => toast(err.message()),
            }
        }
    }
}

/// Rebuilds the shop header, card list, and detail panel whenever the state is
/// dirty or the pause menu was just (re)spawned.
#[allow(clippy::too_many_arguments)]
fn shop_panel_refresh_system(
    mut commands: Commands,
    mut ui: ResMut<ShopUiState>,
    catalog: Res<ShopCatalog>,
    player_q: Query<(&PlayerIndex, &PlayerStats, &PlayerProgression), With<Player>>,
    list_q: Query<Entity, With<ShopCardList>>,
    detail_q: Query<Entity, With<ShopDetailPanel>>,
    added_q: Query<Entity, Added<ShopCardList>>,
    children_q: Query<&Children>,
    mut header_q: Query<&mut Text, With<ShopHeaderText>>,
) {
    if !ui.dirty && added_q.is_empty() {
        return;
    }
    ui.dirty = false;
    let (Ok(list), Ok(detail)) = (list_q.single(), detail_q.single()) else {
        return;
    };

    let Some((_, stats, progression)) = player_q
        .iter()
        .find(|(idx, _, _)| idx.0 == ui.owner)
        .or_else(|| player_q.iter().next())
    else {
        return;
    };
    let credits = stats.credits;
    let ownership = progression.shop.clone();

    if let Ok(mut header) = header_q.single_mut() {
        *header = Text::new(format!(
            "SHOPPING AS P{} — {} CREDITS — {}",
            ui.owner + 1,
            credits,
            ui.category.label().to_uppercase()
        ));
    }

    for panel in [list, detail] {
        if let Ok(children) = children_q.get(panel) {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }
    }

    let selected = ui.selected;
    let category = ui.category;
    commands.entity(list).with_children(|cards| {
        for item in catalog.items_for(category) {
            let state = shop_transactions::card_state(&ownership, item, credits);
            spawn_shop_button(
                cards,
                format!(
                    "{} · {} CR · [{}]",
                    item.name,
                    item.price_credits,
                    state.badge()
                ),
                ShopAction::Select(item.id),
                shop_card_color(state),
            );
        }
    });

    let detail_item = selected.and_then(|id| catalog.items.iter().find(|item| item.id == id));
    commands.entity(detail).with_children(|panel| {
        let text = |panel: &mut ChildSpawnerCommands, value: String, size: f32, color: Color| {
            panel.spawn((
                Text::new(value),
                TextFont {
                    font_size: FontSize::Px(size),
                    ..default()
                },
                TextColor(color),
            ));
        };
        let Some(item) = detail_item else {
            text(
                panel,
                "Pick an item card to preview it.".to_string(),
                15.0,
                Color::srgb(0.72, 0.82, 1.0),
            );
            return;
        };
        let state = shop_transactions::card_state(&ownership, item, credits);
        text(
            panel,
            item.name.to_string(),
            20.0,
            Color::srgb(0.9, 0.95, 1.0),
        );
        text(
            panel,
            item.summary.to_string(),
            14.0,
            Color::srgb(0.82, 0.88, 1.0),
        );
        if let Some(preview) = shop_preview_line(item) {
            text(panel, preview, 13.0, Color::srgb(0.70, 0.86, 0.92));
        }
        let equipped_now = ownership
            .equipped_for(item.category)
            .and_then(|id| catalog.items.iter().find(|other| other.id == id))
            .map(|other| other.name)
            .unwrap_or("none");
        text(
            panel,
            format!("Currently equipped: {equipped_now}"),
            13.0,
            Color::srgb(0.70, 0.78, 0.94),
        );
        use shop_transactions::CardState;
        match state {
            CardState::Affordable => spawn_shop_button(
                panel,
                format!("BUY ({} CR)", item.price_credits),
                ShopAction::ConfirmBuy,
                Color::srgb(0.46, 0.34, 0.08),
            ),
            CardState::Unaffordable => text(
                panel,
                format!(
                    "Save up {} more credits to buy this.",
                    item.price_credits.saturating_sub(credits)
                ),
                13.0,
                Color::srgb(0.94, 0.72, 0.52),
            ),
            CardState::Owned => spawn_shop_button(
                panel,
                "EQUIP".to_string(),
                ShopAction::Equip,
                Color::srgb(0.10, 0.40, 0.22),
            ),
            CardState::Equipped => spawn_shop_button(
                panel,
                "UNEQUIP".to_string(),
                ShopAction::Unequip,
                Color::srgb(0.34, 0.26, 0.10),
            ),
            CardState::Locked => text(
                panel,
                "Vehicle frames are party-shared and get built in the Robot Garage.".to_string(),
                13.0,
                Color::srgb(0.94, 0.72, 0.52),
            ),
        }
        spawn_shop_button(
            panel,
            "CLOSE".to_string(),
            ShopAction::CloseDetail,
            Color::srgb(0.20, 0.26, 0.34),
        );
    });
}

fn update_pause_menu_page_visibility(
    menu: Res<PauseMenuState>,
    mut page_q: Query<(&PausePagePanel, &mut Visibility, &mut Node)>,
) {
    if !menu.is_changed() {
        return;
    }
    for (panel, mut visibility, mut node) in page_q.iter_mut() {
        let active = panel.0 == menu.page;
        *visibility = if active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        // Hidden pages must leave layout as well as rendering; otherwise their
        // controls still consume vertical space and can push the active page
        // outside the viewport on 720p/split-screen displays.
        node.display = if active { Display::Flex } else { Display::None };
    }
}

fn despawn_pause_menu(mut commands: Commands, q: Query<Entity, With<PauseRoot>>) {
    for entity in q.iter() {
        commands.entity(entity).despawn();
    }
}

fn freeze_physics_on_pause(mut physics_time: ResMut<Time<Physics>>) {
    physics_time.pause();
}

fn resume_physics_after_pause(mut physics_time: ResMut<Time<Physics>>) {
    physics_time.unpause();
}

#[allow(clippy::too_many_arguments)]
fn pause_input_system(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    native: Res<NativeControllerState>,
    assignments: Res<GamepadAssignments>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    input_q: Query<&PlayerInput, With<Player>>,
    capture: Res<UiGameplayCapture>,
    state: Res<State<AppState>>,
    mut menu: ResMut<PauseMenuState>,
    mut transition: ResMut<PlaySessionTransition>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let ignored_overlay = assignments.native_overlay_gamepad();
    let assigned_bevy = |entity: Entity| {
        Some(entity) != ignored_overlay && assignments.player_for_gamepad(entity).is_some()
    };
    let native_active = assignments.native_player().is_some();
    let raw_event_pause_pressed = button_events
        .read()
        .filter(|event| {
            assigned_bevy(event.entity)
                && event.state == ButtonState::Pressed
                && event.button == GamepadButton::Start
        })
        .count()
        != 0;
    // Select + Start belongs to the level-editor workspace. Do not also open
    // the pause menu on the frame that chord changes tool mode.
    let editor_chord = gamepads.iter().any(|(entity, gamepad)| {
        assigned_bevy(entity)
            && gamepad.pressed(GamepadButton::Select)
            && gamepad.just_pressed(GamepadButton::Start)
    }) || (native_active
        && native.pressed(NativeButton::Select)
        && native.just_pressed(NativeButton::Start));
    if editor_chord {
        return;
    }
    let raw_pause_pressed = keyboard.just_pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|(entity, gamepad)| {
                assigned_bevy(entity) && gamepad.just_pressed(GamepadButton::Start)
            })
        || (native_active && native.just_pressed(NativeButton::Start))
        || raw_event_pause_pressed;
    let pause_held = keyboard.pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|(entity, gamepad)| {
                assigned_bevy(entity) && gamepad.pressed(GamepadButton::Start)
            })
        || (native_active && native.pressed(NativeButton::Start));
    let player_pause_pressed = input_q.iter().any(|input| input.pause);
    let back_resume_requested = std::mem::take(&mut menu.back_resume_requested);

    if matches!(state.get(), AppState::Paused) {
        menu.resume_lockout = (menu.resume_lockout - time.delta_secs()).max(0.0);
        if !pause_held && menu.resume_lockout <= 0.0 {
            menu.resume_armed = true;
        }
    }

    match state.get() {
        AppState::Playing => {
            let map_pressed = input_q.iter().any(|input| input.open_map);
            if map_pressed {
                menu.requested_page = Some(PausePage::Map);
            }
            if capture.owner.is_some()
                || !(raw_pause_pressed || player_pause_pressed || map_pressed)
            {
                return;
            }
            transition.pausing = true;
            transition.resuming_from_pause = false;
            menu.resume_lockout = 0.20;
            menu.resume_armed = false;
            next_state.set(AppState::Paused);
        }
        AppState::Paused
            if menu.resume_armed
                && (raw_pause_pressed || player_pause_pressed || back_resume_requested) =>
        {
            transition.pausing = false;
            transition.resuming_from_pause = true;
            menu.resume_armed = false;
            next_state.set(AppState::Playing);
        }
        AppState::Paused => {}
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn pause_menu_action_system(
    button_q: Query<(&Interaction, &PauseMenuButton), (Changed<Interaction>, With<Button>)>,
    sp: SaveParams,
    mut menu: ResMut<PauseMenuState>,
    mut transition: ResMut<PlaySessionTransition>,
    mut next_state: ResMut<NextState<AppState>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let mut action = None;
    for (interaction, button) in button_q.iter() {
        if *interaction == Interaction::Pressed {
            action = Some(button.0);
        }
    }

    let Some(action) = action else {
        return;
    };

    match action {
        PauseAction::Resume => {
            transition.pausing = false;
            transition.resuming_from_pause = true;
            next_state.set(AppState::Playing);
        }
        PauseAction::Map => {
            menu.page = PausePage::Map;
        }
        PauseAction::Save => {
            send_pause_save_result(save_current_session(&sp), &mut msg_ev);
        }
        PauseAction::Title => {
            send_pause_save_result(save_current_session(&sp), &mut msg_ev);
            transition.pausing = false;
            transition.resuming_from_pause = false;
            next_state.set(AppState::MainMenu);
        }
        PauseAction::Controls => {
            menu.page = PausePage::Controls;
        }
        PauseAction::Shop => {
            menu.page = PausePage::Shop;
        }
        PauseAction::Settings => {
            menu.page = PausePage::Settings;
        }
        PauseAction::Back => {
            menu.page = PausePage::Main;
        }
    }
}

fn send_pause_save_result(result: Result<(), String>, msg_ev: &mut MessageWriter<UiMessageEvent>) {
    match result {
        Ok(()) => {
            msg_ev.write(UiMessageEvent {
                text: "Game saved.".to_string(),
                duration: 2.0,
            });
        }
        Err(e) => {
            msg_ev.write(UiMessageEvent {
                text: format!("Save failed: {}", e),
                duration: 2.4,
            });
        }
    }
}

fn clear_play_session_transition_flags(mut transition: ResMut<PlaySessionTransition>) {
    transition.pausing = false;
    transition.resuming_from_pause = false;
}

// ── Chapter Select ────────────────────────────────────────────────────────────
#[derive(Component)]
struct ChapterSelectRoot;
#[derive(Component)]
struct ChapterSelectScrollPanel;
#[derive(Component)]
struct ChapterLoadingOverlay;
#[derive(Component)]
struct ChapterFastTravelButton(ChapterId);
#[derive(Component, Clone, Copy)]
struct CaveFastTravelButton {
    chapter: ChapterId,
    anchor_id: &'static str,
    label: &'static str,
}
#[derive(Component, Clone, Copy)]
struct WorldAnchorFastTravelButton {
    chapter: ChapterId,
    anchor_id: &'static str,
    label: &'static str,
    enter_dungeon: bool,
}
#[derive(Component, Clone, Copy)]
struct ChapterSelectActionButton(ChapterSelectAction);
#[derive(Clone, Copy)]
enum ChapterSelectAction {
    Back,
    CharacterEditor,
    RobotGarage,
}
#[derive(Component)]
struct ChapterPerkButton(&'static str);
#[derive(Component, Clone, Copy)]
struct ChapterUpgradeButton(TechUpgradeId);
#[derive(Component, Clone, Copy)]
struct ChapterWeaponRankButton(usize);
#[derive(Component, Clone, Copy)]
struct ChapterProgressionOwnerButton(u8);
#[derive(Resource, Debug, Clone, Copy, Default)]
struct ChapterProgressionOwner(u8);
// Legacy single-block text — kept so old queries don't break; no longer spawned.
#[derive(Component)]
#[allow(dead_code)]
struct PerkPanelText;
#[derive(Component)]
#[allow(dead_code)]
struct TechUpgradePanelText;
// New per-row markers for the structured panels.
#[derive(Component)]
struct PerkPointsHeader;
#[derive(Component)]
struct PerkRowText(pub &'static str); // perk_id
#[derive(Component)]
struct UpgradeReserveHeader;
#[derive(Component)]
struct UpgradeRowText(pub TechUpgradeId);

// ── Weapon Rank Panel markers ──────────────────────────────────────────────────
#[derive(Component)]
struct WeaponRankHeader;
#[derive(Component)]
struct WeaponRankRowText(pub usize); // weapon slot index 0-5

// ── Settlement Economy Panel markers ──────────────────────────────────────────
#[derive(Component)]
struct EconomyPanelHeader;
#[derive(Component)]
struct EconomyPanelSiteRow(pub usize); // index into map_settlements()

fn setup_chapter_select(
    mut commands: Commands,
    progress: Res<ChapterProgress>,
    select: Res<PlayerSelectState>,
    mut owner: ResMut<ChapterProgressionOwner>,
    robot_pets: Res<RobotPetCollection>,
    world_site_registry: Res<WorldSiteRegistry>,
    published_recipes: Res<PublishedProceduralRecipeCatalog>,
    theme: Res<UiTheme>,
    economy: Res<SettlementEconomy>,
) {
    let chapters = all_chapters();
    if !select
        .slots
        .get(owner.0 as usize)
        .is_some_and(|slot| slot.joined)
    {
        owner.0 = select
            .slots
            .iter()
            .position(|slot| slot.joined)
            .unwrap_or(0) as u8;
    }
    let selected_progression = selected_progression(&select, *owner);
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::FlexStart,
                padding: UiRect::all(Val::Px(26.0)),
                row_gap: Val::Px(8.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.06, 1.0)),
            ScrollPosition::default(),
            ChapterSelectRoot,
            ChapterSelectScrollPanel,
            MenuScrollPanel,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("EVEREST RANGE - 200 MILE FAST TRAVEL"),
                TextFont {
                    font_size: FontSize::Px(40.0),
                    ..default()
                },
                TextColor(Color::srgb(0.4, 0.85, 1.0)),
            ));
            p.spawn((
                Text::new("D-PAD / LEFT STICK: navigate and scroll   A: select / start   B: back"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.68, 0.78, 0.92)),
                UiPromptText(UiPromptKind::MenuNavigation),
            ));
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(10.0),
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|row| {
                spawn_chapter_action_button(row, "BACK", ChapterSelectAction::Back);
                spawn_chapter_action_button(
                    row,
                    "CHARACTER EDITOR",
                    ChapterSelectAction::CharacterEditor,
                );
                spawn_chapter_action_button(row, "ROBOT GARAGE", ChapterSelectAction::RobotGarage);
                for (index, slot) in select.slots.iter().enumerate() {
                    if slot.joined {
                        spawn_progression_owner_button(row, index as u8, owner.0 == index as u8);
                    }
                }
            });
            p.spawn(Node {
                width: Val::Percent(100.0),
                max_width: Val::Px(1160.0),
                height: Val::Px(330.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(16.0),
                ..default()
            })
            .with_children(|row| {
                spawn_fast_travel_map(
                    row,
                    &chapters,
                    &progress,
                    &world_site_registry,
                    &published_recipes,
                    &[],
                    &theme,
                );
                row.spawn((
                    Node {
                        width: Val::Px(430.0),
                        height: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.035, 0.045, 0.08, 0.92)),
                    BorderColor::all(Color::srgba(0.25, 0.42, 0.62, 0.75)),
                ))
                .with_children(|list| {
                    for ch in &chapters {
                        let location = chapter_map_locations()
                            .iter()
                            .find(|location| location.id == ch.id);
                        let unlocked = progress.is_fast_travel_unlocked(ch.id);
                        let done = progress.completed.contains(&ch.id.0);
                        let prefix = if done {
                            "[✓]"
                        } else if unlocked {
                            "[ ]"
                        } else {
                            "[X]"
                        };
                        let color = if done {
                            Color::srgb(0.4, 1.0, 0.4)
                        } else if unlocked {
                            Color::WHITE
                        } else {
                            Color::srgb(0.4, 0.4, 0.4)
                        };
                        let region = location
                            .map(|location| location.region)
                            .unwrap_or(ch.subtitle);
                        let mut start_button = list.spawn((
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                min_height: Val::Px(25.0),
                                justify_content: JustifyContent::FlexStart,
                                align_items: AlignItems::Center,
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(if unlocked {
                                Color::srgba(0.06, 0.10, 0.18, 0.88)
                            } else {
                                Color::srgba(0.03, 0.035, 0.055, 0.82)
                            }),
                            BorderColor::all(if unlocked {
                                Color::srgba(0.20, 0.42, 0.66, 0.72)
                            } else {
                                Color::srgba(0.12, 0.14, 0.20, 0.62)
                            }),
                            ChapterFastTravelButton(ch.id),
                        ));
                        if !unlocked {
                            start_button.insert(MenuButtonDisabled);
                        }
                        start_button.with_children(|button| {
                            let mission_title = chapter_mission(ch.id)
                                .map(|mission| mission.title.as_str())
                                .unwrap_or(ch.title);
                            button.spawn((
                                Text::new(format!(
                                    "{} CH.{:02} {} / {} — MISSION: {}",
                                    prefix, ch.id.0, ch.title, region, mission_title
                                )),
                                TextFont {
                                    font_size: FontSize::Px(12.4),
                                    ..default()
                                },
                                TextColor(color),
                            ));
                        });
                    }
                });
            });
            p.spawn(Node {
                height: Val::Px(4.0),
                ..default()
            });

            // ── Perk Training panel ───────────────────────────────────────────
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.04, 0.06, 0.14, 0.90)),
                BorderColor::all(Color::srgba(0.3, 0.5, 0.8, 0.5)),
            ))
            .with_children(|pp| {
                // Header
                pp.spawn((
                    Text::new(format_owner_header(
                        *owner,
                        format_perk_header(&selected_progression.perks),
                    )),
                    TextFont {
                        font_size: FontSize::Px(13.5),
                        ..default()
                    },
                    TextColor(Color::srgb(0.6, 0.82, 1.0)),
                    PerkPointsHeader,
                ));
                // One row per perk
                let defs = all_perks();
                for perk_id in CHAPTER_PERK_IDS {
                    if let Some(def) = defs.iter().find(|d| d.id == perk_id) {
                        let branch_color = branch_color(def.branch);
                        pp.spawn((
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                justify_content: JustifyContent::FlexStart,
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.06, 0.08, 0.16, 0.72)),
                            BorderColor::all(Color::srgba(0.18, 0.32, 0.52, 0.5)),
                            ChapterPerkButton(perk_id),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                Text::new(format_perk_row(def, &selected_progression.perks)),
                                TextFont {
                                    font_size: FontSize::Px(12.5),
                                    ..default()
                                },
                                TextColor(branch_color),
                                PerkRowText(perk_id),
                            ));
                        });
                    }
                }
            });

            // ── Tech Upgrades panel ───────────────────────────────────────────
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.04, 0.10, 0.06, 0.90)),
                BorderColor::all(Color::srgba(0.3, 0.75, 0.45, 0.5)),
            ))
            .with_children(|up| {
                // Header
                up.spawn((
                    Text::new(format_owner_header(
                        *owner,
                        format_upgrade_header(&selected_progression.upgrades),
                    )),
                    TextFont {
                        font_size: FontSize::Px(13.5),
                        ..default()
                    },
                    TextColor(Color::srgb(0.5, 1.0, 0.65)),
                    UpgradeReserveHeader,
                ));
                // One row per upgrade
                for def in all_tech_upgrades() {
                    let track_color = track_color(def.track);
                    up.spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            justify_content: JustifyContent::FlexStart,
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.05, 0.12, 0.07, 0.72)),
                        BorderColor::all(Color::srgba(0.22, 0.45, 0.28, 0.5)),
                        ChapterUpgradeButton(def.id),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Text::new(format_upgrade_row(
                                &def,
                                &selected_progression.upgrades,
                                &robot_pets,
                            )),
                            TextFont {
                                font_size: FontSize::Px(12.0),
                                ..default()
                            },
                            TextColor(track_color),
                            UpgradeRowText(def.id),
                        ));
                    });
                }
            });

            // ── Weapon Rank panel ─────────────────────────────────────────────
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.08, 0.04, 0.12, 0.90)),
                BorderColor::all(Color::srgba(0.72, 0.45, 1.0, 0.55)),
            ))
            .with_children(|wp| {
                wp.spawn((
                    Text::new(format_weapon_rank_header(&robot_pets)),
                    TextFont {
                        font_size: FontSize::Px(13.5),
                        ..default()
                    },
                    TextColor(Color::srgb(0.82, 0.62, 1.0)),
                    WeaponRankHeader,
                ));
                for (slot, weapon_type) in WEAPON_RANK_WEAPONS.iter().enumerate() {
                    wp.spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            justify_content: JustifyContent::FlexStart,
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.10, 0.05, 0.14, 0.72)),
                        BorderColor::all(Color::srgba(0.44, 0.28, 0.62, 0.5)),
                        ChapterWeaponRankButton(slot),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Text::new(format_weapon_rank_row(
                                slot,
                                *weapon_type,
                                &selected_progression.weapon_ranks,
                                &robot_pets,
                            )),
                            TextFont {
                                font_size: FontSize::Px(12.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.72, 0.55, 0.96)),
                            WeaponRankRowText(slot),
                        ));
                    });
                }
            });

            // ── Settlement Economy panel ──────────────────────────────────────
            p.spawn(Node {
                height: Val::Px(4.0),
                ..default()
            });
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.04, 0.10, 0.10, 0.90)),
                BorderColor::all(Color::srgba(0.3, 0.8, 0.75, 0.5)),
            ))
            .with_children(|ep| {
                ep.spawn((
                    Text::new(format_economy_header(&economy)),
                    TextFont {
                        font_size: FontSize::Px(13.5),
                        ..default()
                    },
                    TextColor(Color::srgb(0.4, 0.95, 0.9)),
                    EconomyPanelHeader,
                ));
                for (idx, settlement) in map_settlements().iter().enumerate() {
                    ep.spawn((
                        Text::new(format_economy_site_row(
                            settlement,
                            &economy,
                            &world_site_registry,
                        )),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.55, 0.85, 0.8)),
                        EconomyPanelSiteRow(idx),
                    ));
                }
            });
        });

    // `NextState` changes requested by the travel buttons are not applied
    // until the following frame. Keep this overlay ready but out of layout so
    // that frame can present an acknowledgement before synchronous
    // `OnEnter(Playing)` world generation begins.
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                display: Display::None,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(theme.canvas),
            ZIndex(1_000),
            ChapterLoadingOverlay,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: Val::Px(560.0),
                        max_width: Val::Percent(88.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(12.0),
                        padding: UiRect::all(Val::Px(28.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(theme.panel),
                    BorderColor::all(theme.energy),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("LOADING WORLD…"),
                        TextFont {
                            font_size: FontSize::Px(42.0),
                            ..default()
                        },
                        TextColor(theme.objective),
                    ));
                    panel.spawn((
                        Text::new("Preparing the city, roads, ramps, and skyline."),
                        TextFont {
                            font_size: FontSize::Px(18.0),
                            ..default()
                        },
                        TextLayout::justify(Justify::Center),
                        TextColor(theme.text_primary),
                    ));
                    panel.spawn((
                        Text::new("The first visit can take a moment."),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(theme.text_muted),
                    ));
                });
        });
}

fn despawn_chapter_select(
    mut commands: Commands,
    root_q: Query<Entity, With<ChapterSelectRoot>>,
    overlay_q: Query<Entity, With<ChapterLoadingOverlay>>,
) {
    for e in root_q.iter().chain(overlay_q.iter()) {
        commands.entity(e).despawn();
    }
}

fn chapter_loading_overlay_display(next_state: &NextState<AppState>) -> Display {
    match next_state {
        NextState::Pending(AppState::Playing) | NextState::PendingIfNeq(AppState::Playing) => {
            Display::Flex
        }
        _ => Display::None,
    }
}

fn update_chapter_loading_overlay(
    next_state: Res<NextState<AppState>>,
    mut overlay_q: Query<&mut Node, With<ChapterLoadingOverlay>>,
) {
    let display = chapter_loading_overlay_display(&next_state);
    for mut node in overlay_q.iter_mut() {
        if node.display != display {
            node.display = display;
        }
    }
}

fn spawn_chapter_action_button(
    parent: &mut ChildSpawnerCommands,
    label: &'static str,
    action: ChapterSelectAction,
) {
    parent
        .spawn((
            Button,
            Node {
                min_width: Val::Px(140.0),
                height: Val::Px(38.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(12.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.14, 0.22)),
            BorderColor::all(Color::srgb(0.25, 0.50, 0.70)),
            ChapterSelectActionButton(action),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn spawn_progression_owner_button(parent: &mut ChildSpawnerCommands, player: u8, selected: bool) {
    parent
        .spawn((
            Button,
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if selected {
                Color::srgba(0.16, 0.38, 0.62, 0.95)
            } else {
                Color::srgba(0.06, 0.10, 0.18, 0.9)
            }),
            BorderColor::all(Color::srgba(0.35, 0.7, 1.0, 0.7)),
            ChapterProgressionOwnerButton(player),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(format!("P{} UPGRADES", player + 1)),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn selected_progression(
    select: &PlayerSelectState,
    owner: ChapterProgressionOwner,
) -> &PlayerProgression {
    &select.slots[owner.0.min(3) as usize].progression
}

fn format_owner_header(owner: ChapterProgressionOwner, header: String) -> String {
    format!("P{}  {header}", owner.0 + 1)
}

fn spawn_fast_travel_map(
    parent: &mut ChildSpawnerCommands,
    chapters: &[crate::chapters::ChapterDef],
    progress: &ChapterProgress,
    world_site_registry: &WorldSiteRegistry,
    published_recipes: &PublishedProceduralRecipeCatalog,
    player_positions: &[(u8, Vec3)],
    theme: &UiTheme,
) {
    parent
        .spawn((
            Node {
                width: Val::Px(700.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Relative,
                overflow: Overflow::clip(),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.025, 0.035, 0.055, 0.98)),
            BorderColor::all(Color::srgb(0.16, 0.32, 0.48)),
        ))
        .with_children(|map| {
            // ── Region bands (ordered back-to-front so labels overlap nicely) ──
            // Fangroot Wildland — upper strip, heavy forest canopy
            spawn_map_region_band(
                map,
                0.0,
                2.0,
                45.0,
                18.0,
                Color::srgba(0.22, 0.48, 0.18, 0.28),
                "Fangroot Wildland",
            );
            // Ember Nest — upper-left volcanic highland
            spawn_map_region_band(
                map,
                2.0,
                18.0,
                22.0,
                22.0,
                Color::srgba(0.62, 0.30, 0.10, 0.24),
                "Ember Nest",
            );
            // Sister Sanctum — center-left valley
            spawn_map_region_band(
                map,
                26.0,
                28.0,
                24.0,
                30.0,
                Color::srgba(0.55, 0.36, 0.72, 0.20),
                "Sister Sanctum",
            );
            // Starfall Zone — center hub around origin
            spawn_map_region_band(
                map,
                42.0,
                38.0,
                18.0,
                20.0,
                Color::srgba(0.94, 0.88, 0.22, 0.18),
                "Starfall Zone",
            );
            // Rift Highlands — center-right plateau
            spawn_map_region_band(
                map,
                55.0,
                28.0,
                22.0,
                36.0,
                Color::srgba(0.36, 0.58, 0.82, 0.20),
                "Rift Highlands",
            );
            // Pink Flame & Rockies — eastern range
            spawn_map_region_band(
                map,
                72.0,
                20.0,
                26.0,
                58.0,
                Color::srgba(0.45, 0.62, 0.36, 0.22),
                "Eastern Range",
            );
            // Everest Crown — lower-left fortress domain
            spawn_map_region_band(
                map,
                2.0,
                62.0,
                41.0,
                36.0,
                Color::srgba(0.35, 0.48, 0.62, 0.26),
                "Everest Crown",
            );
            // Glacier Fields — inner sub-region of crown
            spawn_map_region_band(
                map,
                4.0,
                64.0,
                22.0,
                18.0,
                Color::srgba(0.72, 0.82, 0.94, 0.30),
                "Glacier Fields",
            );
            // Antarctic Reach — bottom strip, ice shelf
            spawn_map_region_band(
                map,
                0.0,
                80.0,
                100.0,
                18.0,
                Color::srgba(0.08, 0.28, 0.44, 0.38),
                "Antarctic Reach",
            );
            spawn_map_region_band(
                map,
                76.0,
                74.0,
                20.0,
                22.0,
                Color::srgba(0.82, 0.16, 0.58, 0.30),
                "Grand Raceway",
            );

            for location in chapter_map_locations() {
                let Some(chapter) = chapters.iter().find(|chapter| chapter.id == location.id)
                else {
                    continue;
                };
                let unlocked = progress.is_fast_travel_unlocked(location.id);
                let done = progress.completed.contains(&location.id.0);
                let marker_color = if done {
                    Color::srgb(0.22, 0.85, 0.34)
                } else if unlocked {
                    Color::srgb(0.32, 0.72, 1.0)
                } else {
                    Color::srgb(0.16, 0.18, 0.25)
                };
                let border = if location.id.0 == 6 {
                    Color::srgb(0.96, 0.98, 1.0)
                } else if unlocked {
                    Color::srgb(0.70, 0.92, 1.0)
                } else {
                    Color::srgb(0.30, 0.32, 0.40)
                };

                let mut marker_button = map.spawn((
                    Button,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(world_to_map_left(location.x)),
                        top: Val::Percent(world_to_map_top(location.z)),
                        width: Val::Px(26.0),
                        height: Val::Px(26.0),
                        margin: UiRect {
                            left: Val::Px(-13.0),
                            top: Val::Px(-13.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(2.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(marker_color),
                    BorderColor::all(border),
                    ChapterFastTravelButton(location.id),
                ));
                if !unlocked {
                    marker_button.insert(MenuButtonDisabled);
                }
                marker_button.with_children(|marker| {
                    marker.spawn((
                        Text::new(location.id.0.to_string()),
                        TextFont {
                            font_size: FontSize::Px(10.5),
                            ..default()
                        },
                        TextColor(if unlocked {
                            Color::WHITE
                        } else {
                            Color::srgb(0.55, 0.58, 0.64)
                        }),
                    ));
                });

                map.spawn((
                    Text::new(format!(
                        "{} {} — {}",
                        location.id.0, chapter.title, location.landmark
                    )),
                    TextFont {
                        font_size: FontSize::Px(10.0),
                        ..default()
                    },
                    TextColor(if unlocked {
                        Color::srgb(0.80, 0.90, 1.0)
                    } else {
                        Color::srgb(0.36, 0.38, 0.46)
                    }),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent((world_to_map_left(location.x) + 1.7).min(88.0)),
                        top: Val::Percent((world_to_map_top(location.z) - 1.0).clamp(2.0, 94.0)),
                        ..default()
                    },
                ));
            }

            // Every authored cave is visible and usable from a new save so
            // players can explore the complete world in any order.
            for cave in SECRET_CAVE_LOCATIONS {
                let completed = mission_for_travel_anchor(cave.anchor_id)
                    .is_some_and(|mission| progress.has_discoverable(&mission.completion_key()));
                let mut cave_button = map.spawn((
                    Button,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(world_to_map_left(cave.x)),
                        top: Val::Percent(world_to_map_top(cave.z)),
                        width: Val::Px(20.0),
                        height: Val::Px(20.0),
                        margin: UiRect {
                            left: Val::Px(-10.0),
                            top: Val::Px(8.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(2.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(if completed {
                        Color::srgb(0.24, 0.72, 0.42)
                    } else {
                        Color::srgb(0.42, 0.18, 0.62)
                    }),
                    BorderColor::all(if completed {
                        Color::srgb(0.66, 1.0, 0.76)
                    } else {
                        Color::srgb(0.82, 0.52, 1.0)
                    }),
                    CaveFastTravelButton {
                        chapter: cave.chapter,
                        anchor_id: cave.anchor_id,
                        label: cave.label,
                    },
                ));
                cave_button.with_children(|marker| {
                    marker.spawn((
                        Text::new(if completed { "✓" } else { "C" }),
                        TextFont {
                            font_size: FontSize::Px(10.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            }

            // The Grand Raceway is a full terrain district rather than a
            // chapter. Its two entrances reuse the selected chapter session
            // and move the whole party to authored world anchors.
            for point in RACE_REGION_TRAVEL_POINTS {
                let unlocked = progress.is_fast_travel_unlocked(point.chapter);
                let mut travel_button = map.spawn((
                    Button,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(world_to_map_left(point.x)),
                        top: Val::Percent(world_to_map_top(point.z)),
                        width: Val::Px(22.0),
                        height: Val::Px(22.0),
                        margin: UiRect {
                            left: Val::Px(-11.0),
                            top: Val::Px(-11.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(2.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(if unlocked {
                        Color::srgb(0.92, 0.28, 0.72)
                    } else {
                        Color::srgb(0.18, 0.10, 0.18)
                    }),
                    BorderColor::all(if unlocked {
                        Color::srgb(0.40, 0.95, 1.0)
                    } else {
                        Color::srgb(0.30, 0.24, 0.32)
                    }),
                    WorldAnchorFastTravelButton {
                        chapter: point.chapter,
                        anchor_id: point.anchor_id,
                        label: point.label,
                        enter_dungeon: false,
                    },
                ));
                if !unlocked {
                    travel_button.insert(MenuButtonDisabled);
                }
                travel_button.with_children(|marker| {
                    marker.spawn((
                        Text::new("R"),
                        TextFont {
                            font_size: FontSize::Px(10.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            }

            // Dragon strongholds and both castles are explicit mission travel
            // points rather than hidden scenery.
            for point in SPECIAL_MISSION_TRAVEL_POINTS {
                let completed = mission_for_travel_anchor(point.anchor_id)
                    .is_some_and(|mission| progress.has_discoverable(&mission.completion_key()));
                map.spawn((
                    Button,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(world_to_map_left(point.x)),
                        top: Val::Percent(world_to_map_top(point.z)),
                        width: Val::Px(24.0),
                        height: Val::Px(24.0),
                        margin: UiRect {
                            left: Val::Px(-12.0),
                            top: Val::Px(12.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(2.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(if completed {
                        Color::srgb(0.24, 0.78, 0.44)
                    } else if point.enter_dungeon {
                        Color::srgb(0.88, 0.20, 0.12)
                    } else {
                        Color::srgb(0.92, 0.72, 0.18)
                    }),
                    BorderColor::all(Color::srgb(1.0, 0.88, 0.58)),
                    WorldAnchorFastTravelButton {
                        chapter: point.chapter,
                        anchor_id: point.anchor_id,
                        label: point.label,
                        enter_dungeon: point.enter_dungeon,
                    },
                ))
                .with_children(|marker| {
                    marker.spawn((
                        Text::new(if completed {
                            "✓"
                        } else if point.enter_dungeon {
                            "D"
                        } else {
                            "K"
                        }),
                        TextFont {
                            font_size: FontSize::Px(10.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            }

            for temple in great_scientist_map_markers() {
                let collected = progress.has_discoverable(temple.reward_id);
                let marker_color = if collected {
                    Color::srgb(0.95, 0.82, 0.28)
                } else {
                    Color::srgb(0.62, 0.36, 0.94)
                };
                map.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(world_to_map_left(temple.x)),
                        top: Val::Percent(world_to_map_top(temple.z)),
                        width: Val::Px(18.0),
                        height: Val::Px(18.0),
                        margin: UiRect {
                            left: Val::Px(-9.0),
                            top: Val::Px(-9.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(2.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(marker_color),
                    BorderColor::all(if collected {
                        Color::srgb(1.0, 0.92, 0.45)
                    } else {
                        Color::srgb(0.86, 0.70, 1.0)
                    }),
                ))
                .with_children(|marker| {
                    marker.spawn((
                        Text::new("T"),
                        TextFont {
                            font_size: FontSize::Px(10.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });

                map.spawn((
                    Text::new(temple.label),
                    TextFont {
                        font_size: FontSize::Px(8.6),
                        ..default()
                    },
                    TextColor(if collected {
                        Color::srgb(1.0, 0.92, 0.56)
                    } else {
                        Color::srgb(0.90, 0.78, 1.0)
                    }),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent((world_to_map_left(temple.x) + 1.3).min(90.0)),
                        top: Val::Percent((world_to_map_top(temple.z) + 0.9).clamp(3.0, 94.0)),
                        ..default()
                    },
                ));
            }

            for settlement in map_settlements() {
                // Settlements linked to a WorldSite are rendered as site badges
                // in the loop below — skip the generic marker here to avoid overlap.
                if settlement.site_id.is_some() {
                    continue;
                }
                let visited = progress.has_discoverable(settlement.reward_id);
                let marker_color = if visited {
                    Color::srgb(0.34, 0.92, 0.52)
                } else {
                    match settlement.kind {
                        MapSettlementKind::City => Color::srgb(0.28, 0.70, 1.0),
                        MapSettlementKind::Village => Color::srgb(0.36, 0.74, 0.38),
                        MapSettlementKind::Harbor => Color::srgb(0.26, 0.88, 0.92),
                        MapSettlementKind::Outpost => Color::srgb(1.0, 0.52, 0.24),
                    }
                };
                map.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(world_to_map_left(settlement.x)),
                        top: Val::Percent(world_to_map_top(settlement.z)),
                        width: Val::Px(16.0),
                        height: Val::Px(16.0),
                        margin: UiRect {
                            left: Val::Px(-8.0),
                            top: Val::Px(-8.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(1.5)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(marker_color),
                    BorderColor::all(if visited {
                        Color::srgb(0.76, 1.0, 0.74)
                    } else {
                        Color::srgb(0.86, 0.94, 1.0)
                    }),
                ))
                .with_children(|marker| {
                    marker.spawn((
                        Text::new(match settlement.kind {
                            MapSettlementKind::City => "C",
                            MapSettlementKind::Village => "V",
                            MapSettlementKind::Harbor => "H",
                            MapSettlementKind::Outpost => "O",
                        }),
                        TextFont {
                            font_size: FontSize::Px(9.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });

                map.spawn((
                    Text::new(format!("{} — {}", settlement.name, settlement.region)),
                    TextFont {
                        font_size: FontSize::Px(8.2),
                        ..default()
                    },
                    TextColor(if visited {
                        Color::srgb(0.72, 1.0, 0.78)
                    } else {
                        Color::srgb(0.74, 0.90, 0.92)
                    }),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent((world_to_map_left(settlement.x) + 1.2).min(90.0)),
                        top: Val::Percent((world_to_map_top(settlement.z) - 1.8).clamp(3.0, 94.0)),
                        ..default()
                    },
                ));
            }

            // ── World Site badges ─────────────────────────────────────────
            for site in &world_site_registry.sites {
                let badge_color = site.state.map_badge_color();
                use crate::resources::WorldSiteState as WSS;
                let site_icon = match site.state {
                    WSS::Liberated | WSS::Building | WSS::Shielded => "●",
                    WSS::EnemyHeld | WSS::OccupiedAgain => "▲",
                    WSS::Contested | WSS::UnderAttack => "◆",
                    WSS::Damaged => "◈",
                    WSS::Hidden => "?",
                };
                let border_color = if site.is_liberated() {
                    Color::srgb(0.40, 1.0, 0.50)
                } else {
                    Color::srgb(1.0, 0.40, 0.30)
                };
                map.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(world_to_map_left(site.world_x)),
                        top: Val::Percent(world_to_map_top(site.world_z)),
                        width: Val::Px(14.0),
                        height: Val::Px(14.0),
                        margin: UiRect {
                            left: Val::Px(-7.0),
                            top: Val::Px(-7.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(1.5)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(badge_color),
                    BorderColor::all(border_color),
                ))
                .with_children(|badge| {
                    badge.spawn((
                        Text::new(site_icon),
                        TextFont {
                            font_size: FontSize::Px(9.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });

                map.spawn((
                    Text::new(site.name),
                    TextFont {
                        font_size: FontSize::Px(7.5),
                        ..default()
                    },
                    TextColor(if site.is_liberated() {
                        Color::srgb(0.55, 1.0, 0.65)
                    } else {
                        Color::srgb(1.0, 0.65, 0.55)
                    }),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent((world_to_map_left(site.world_x) + 1.1).min(90.0)),
                        top: Val::Percent((world_to_map_top(site.world_z) + 1.2).clamp(3.0, 94.0)),
                        ..default()
                    },
                ));
            }

            // Published World Kit recipes extend the map without requiring UI
            // code changes. Fast-travel topology nodes are interactive during
            // play; landmark nodes remain visible annotations.
            for point in published_recipes.map_points() {
                let travelable =
                    point.kind == PublishedMapPointKind::FastTravel && !player_positions.is_empty();
                let color = match point.kind {
                    PublishedMapPointKind::FastTravel => Color::srgb(0.18, 0.82, 0.92),
                    PublishedMapPointKind::Landmark => Color::srgb(0.92, 0.52, 0.94),
                };
                let mut marker = map.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(world_to_map_left(point.position.x)),
                        top: Val::Percent(world_to_map_top(point.position.z)),
                        width: Val::Px(20.0),
                        height: Val::Px(20.0),
                        margin: UiRect {
                            left: Val::Px(-10.0),
                            top: Val::Px(-10.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(2.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(color),
                    BorderColor::all(Color::srgb(0.78, 1.0, 1.0)),
                ));
                if travelable {
                    marker.insert((
                        Button,
                        EditorFastTravelButton {
                            point_id: point.id.clone(),
                            label: point.label.clone(),
                            position: point.position,
                        },
                    ));
                }
                marker.with_children(|badge| {
                    badge.spawn((
                        Text::new(match point.kind {
                            PublishedMapPointKind::FastTravel => "U",
                            PublishedMapPointKind::Landmark => "L",
                        }),
                        TextFont {
                            font_size: FontSize::Px(9.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
                map.spawn((
                    Text::new(point.label.clone()),
                    TextFont {
                        font_size: FontSize::Px(7.8),
                        ..default()
                    },
                    TextColor(color),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent((world_to_map_left(point.position.x) + 1.4).min(90.0)),
                        top: Val::Percent(
                            (world_to_map_top(point.position.z) + 1.2).clamp(3.0, 94.0),
                        ),
                        ..default()
                    },
                ));
            }

            // Party markers render last so player position remains readable
            // over regions, sites, missions, and player-authored content.
            for (index, position) in player_positions {
                let color = theme.player_accent(*index);
                map.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(world_to_map_left(position.x)),
                        top: Val::Percent(world_to_map_top(position.z)),
                        width: Val::Px(24.0),
                        height: Val::Px(24.0),
                        margin: UiRect {
                            left: Val::Px(-12.0),
                            top: Val::Px(-12.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(3.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.94)),
                    BorderColor::all(color),
                    ZIndex(20),
                ))
                .with_children(|marker| {
                    marker.spawn((
                        Text::new(format!("P{}", index + 1)),
                        TextFont {
                            font_size: FontSize::Px(9.0),
                            ..default()
                        },
                        TextColor(color),
                    ));
                });
            }
        });
}

#[derive(Clone, Copy)]
struct GreatScientistMapMarker {
    label: &'static str,
    reward_id: &'static str,
    x: f32,
    z: f32,
}

fn great_scientist_map_markers() -> &'static [GreatScientistMapMarker] {
    &[
        GreatScientistMapMarker {
            label: "Flight Temple",
            reward_id: "ancient_flight_core",
            x: -6400.0,
            z: -1800.0,
        },
        GreatScientistMapMarker {
            label: "Solar Temple",
            reward_id: "solar_sabre_glyph",
            x: -2100.0,
            z: 6400.0,
        },
        GreatScientistMapMarker {
            label: "Nova Foundry",
            reward_id: "nova_missile_matrix",
            x: 7200.0,
            z: -5200.0,
        },
        GreatScientistMapMarker {
            label: "Aegis Archive",
            reward_id: "aegis_armor_frame",
            x: 1200.0,
            z: -6100.0,
        },
    ]
}

fn spawn_map_region_band(
    parent: &mut ChildSpawnerCommands,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    color: Color,
    label: &'static str,
) {
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left),
                top: Val::Percent(top),
                width: Val::Percent(width),
                height: Val::Percent(height),
                align_items: AlignItems::FlexStart,
                justify_content: JustifyContent::FlexStart,
                padding: UiRect {
                    left: Val::Px(4.0),
                    top: Val::Px(3.0),
                    ..default()
                },
                ..default()
            },
            BackgroundColor(color),
        ))
        .with_children(|band| {
            band.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(8.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.55)),
            ));
        });
}

fn world_to_map_left(x: f32) -> f32 {
    ((x + EVEREST_RANGE_HALF_EXTENT) / EVEREST_RANGE_WORLD_SIZE * 100.0).clamp(2.0, 98.0)
}

fn world_to_map_top(z: f32) -> f32 {
    ((EVEREST_RANGE_HALF_EXTENT - z) / EVEREST_RANGE_WORLD_SIZE * 100.0).clamp(2.0, 98.0)
}

fn chapter_select_action_buttons(
    interaction_q: Query<
        (&Interaction, &ChapterSelectActionButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut design_data: ResMut<CharacterDesignData>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button.0 {
            ChapterSelectAction::Back => next_state.set(AppState::MainMenu),
            ChapterSelectAction::CharacterEditor => {
                design_data.player_index = 0;
                design_data.return_target = CharacterDesignReturnTarget::ChapterSelect;
                next_state.set(AppState::CharacterDesign);
            }
            ChapterSelectAction::RobotGarage => next_state.set(AppState::RobotGarage),
        }
        return;
    }
}

fn chapter_select_progression_owner_buttons(
    interaction_q: Query<
        (&Interaction, &ChapterProgressionOwnerButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut owner: ResMut<ChapterProgressionOwner>,
    mut button_q: Query<(&ChapterProgressionOwnerButton, &mut BackgroundColor), With<Button>>,
) {
    let Some(selected) = interaction_q.iter().find_map(|(interaction, button)| {
        (*interaction == Interaction::Pressed).then_some(button.0)
    }) else {
        return;
    };
    owner.0 = selected;
    for (button, mut background) in button_q.iter_mut() {
        background.0 = if button.0 == selected {
            Color::srgba(0.16, 0.38, 0.62, 0.95)
        } else {
            Color::srgba(0.06, 0.10, 0.18, 0.9)
        };
    }
}

fn chapter_select_fast_travel_buttons(
    mut current: ResMut<CurrentChapter>,
    mut mission_state: ResMut<CustomMissionState>,
    mut transition: ResMut<PlaySessionTransition>,
    mut next_state: ResMut<NextState<AppState>>,
    interaction_q: Query<
        (&Interaction, &ChapterFastTravelButton),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            if let Some(mission) = chapter_mission(button.0) {
                mission_state.activate(&mission.id);
            }
            current.id = button.0;
            current.started = false;
            transition.pausing = false;
            transition.resuming_from_pause = false;
            next_state.set(AppState::Playing);
        }
    }
}

fn cave_fast_travel_buttons(
    mut current: ResMut<CurrentChapter>,
    mut mission_state: ResMut<CustomMissionState>,
    mut destination: ResMut<FastTravelDestination>,
    mut transition: ResMut<PlaySessionTransition>,
    mut next_state: ResMut<NextState<AppState>>,
    interaction_q: Query<
        (&Interaction, &CaveFastTravelButton),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Some(mission) = mission_for_travel_anchor(button.anchor_id) {
            mission_state.activate(&mission.id);
        } else {
            mission_state.clear();
        }
        current.id = button.chapter;
        current.started = false;
        destination.cave(button.anchor_id, button.label);
        transition.pausing = false;
        transition.resuming_from_pause = false;
        next_state.set(AppState::Playing);
    }
}

fn world_anchor_fast_travel_buttons(
    mut current: ResMut<CurrentChapter>,
    mut mission_state: ResMut<CustomMissionState>,
    mut destination: ResMut<FastTravelDestination>,
    mut transition: ResMut<PlaySessionTransition>,
    mut next_state: ResMut<NextState<AppState>>,
    interaction_q: Query<
        (&Interaction, &WorldAnchorFastTravelButton),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Some(mission) = mission_for_travel_anchor(button.anchor_id) {
            mission_state.activate(&mission.id);
        } else {
            mission_state.clear();
        }
        current.id = button.chapter;
        current.started = false;
        if button.enter_dungeon {
            destination.cave(button.anchor_id, button.label);
        } else {
            destination.world_anchor(button.anchor_id, button.label);
        }
        transition.pausing = false;
        transition.resuming_from_pause = false;
        next_state.set(AppState::Playing);
    }
}

fn editor_fast_travel_buttons(
    mut commands: Commands,
    interaction_q: Query<
        (&Interaction, &EditorFastTravelButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut player_q: Query<(Entity, &PlayerIndex, &mut Transform, &mut PlayerMovement), With<Player>>,
    mut transition: ResMut<PlaySessionTransition>,
    mut next_state: ResMut<NextState<AppState>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    const OFFSETS: [Vec3; 4] = [
        Vec3::new(-3.0, 1.3, -3.0),
        Vec3::new(3.0, 1.3, -3.0),
        Vec3::new(-3.0, 1.3, 3.0),
        Vec3::new(3.0, 1.3, 3.0),
    ];
    let Some(button) = interaction_q
        .iter()
        .find_map(|(interaction, button)| (*interaction == Interaction::Pressed).then_some(button))
    else {
        return;
    };

    for (entity, index, mut transform, mut movement) in player_q.iter_mut() {
        transform.translation = button.position + OFFSETS[index.0.min(3) as usize];
        movement.velocity = Vec3::ZERO;
        movement.ground_velocity = Vec3::ZERO;
        movement.clear_motor_delivery();
        movement.is_grounded = false;
        commands.entity(entity).remove::<BoatPassenger>();
    }
    transition.pausing = false;
    transition.resuming_from_pause = true;
    next_state.set(AppState::Playing);
    msg_ev.write(UiMessageEvent {
        text: format!("FAST TRAVEL — {} [{}]", button.label, button.point_id),
        duration: 3.0,
    });
}

fn chapter_select_mouse_scroll(
    mut wheel: MessageReader<MouseWheel>,
    mut scroll_q: Query<(&mut ScrollPosition, &ComputedNode), With<ChapterSelectScrollPanel>>,
) {
    let mut delta_y = 0.0;
    for event in wheel.read() {
        let scale = match event.unit {
            MouseScrollUnit::Line => 34.0,
            MouseScrollUnit::Pixel => 1.0,
        };
        delta_y -= event.y * scale;
    }

    if delta_y == 0.0 {
        return;
    }

    for (mut scroll_position, computed) in scroll_q.iter_mut() {
        let max_y = ((computed.content_size().y - computed.size().y)
            * computed.inverse_scale_factor())
        .max(0.0);
        scroll_position.y = (scroll_position.y + delta_y).clamp(0.0, max_y);
    }
}

fn chapter_select_perk_buttons(
    interaction_q: Query<(&Interaction, &ChapterPerkButton), (Changed<Interaction>, With<Button>)>,
    owner: Res<ChapterProgressionOwner>,
    mut select: ResMut<PlayerSelectState>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            select.slots[owner.0.min(3) as usize]
                .progression
                .perks
                .try_spend(button.0);
        }
    }
}

fn chapter_select_perk_panel_update(
    owner: Res<ChapterProgressionOwner>,
    select: Res<PlayerSelectState>,
    mut header_q: Query<&mut Text, (With<PerkPointsHeader>, Without<PerkRowText>)>,
    mut row_q: Query<(&PerkRowText, &mut Text), Without<PerkPointsHeader>>,
) {
    if !(owner.is_changed() || select.is_changed()) {
        return;
    }
    let perks = &selected_progression(&select, *owner).perks;
    for mut text in header_q.iter_mut() {
        *text = Text::new(format_owner_header(*owner, format_perk_header(perks)));
    }
    let defs = all_perks();
    for (row, mut text) in row_q.iter_mut() {
        if let Some(def) = defs.iter().find(|d| d.id == row.0) {
            *text = Text::new(format_perk_row(def, perks));
        }
    }
}

fn chapter_select_upgrade_buttons(
    interaction_q: Query<
        (&Interaction, &ChapterUpgradeButton),
        (Changed<Interaction>, With<Button>),
    >,
    owner: Res<ChapterProgressionOwner>,
    mut select: ResMut<PlayerSelectState>,
    mut robot_pets: ResMut<RobotPetCollection>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            let _ = select.slots[owner.0.min(3) as usize]
                .progression
                .upgrades
                .try_purchase(button.0, &mut robot_pets);
        }
    }
}

fn chapter_select_upgrade_panel_update(
    owner: Res<ChapterProgressionOwner>,
    select: Res<PlayerSelectState>,
    robot_pets: Res<RobotPetCollection>,
    mut header_q: Query<&mut Text, (With<UpgradeReserveHeader>, Without<UpgradeRowText>)>,
    mut row_q: Query<(&UpgradeRowText, &mut Text), Without<UpgradeReserveHeader>>,
) {
    if !(owner.is_changed() || select.is_changed() || robot_pets.is_changed()) {
        return;
    }
    let upgrades = &selected_progression(&select, *owner).upgrades;
    for mut text in header_q.iter_mut() {
        *text = Text::new(format_owner_header(*owner, format_upgrade_header(upgrades)));
    }
    let defs = all_tech_upgrades();
    for (row, mut text) in row_q.iter_mut() {
        if let Some(def) = defs.iter().find(|d| d.id == row.0) {
            *text = Text::new(format_upgrade_row(def, upgrades, &robot_pets));
        }
    }
}

fn chapter_select_weapon_rank_buttons(
    interaction_q: Query<
        (&Interaction, &ChapterWeaponRankButton),
        (Changed<Interaction>, With<Button>),
    >,
    owner: Res<ChapterProgressionOwner>,
    mut select: ResMut<PlayerSelectState>,
    mut robot_pets: ResMut<RobotPetCollection>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            purchase_weapon_rank(
                button.0,
                &mut select.slots[owner.0.min(3) as usize]
                    .progression
                    .weapon_ranks,
                &mut robot_pets,
            );
        }
    }
}

fn purchase_weapon_rank(
    slot: usize,
    weapon_ranks: &mut WeaponRanks,
    robot_pets: &mut RobotPetCollection,
) {
    let Some(rank) = weapon_ranks.ranks.get_mut(slot) else {
        return;
    };
    if *rank >= MAX_WEAPON_RANK {
        return;
    }
    let cost = WeaponRanks::upgrade_cost(*rank);
    let recipe = [crate::world::robot_pets::PartCost::new(
        RobotPartKind::CircuitBoard,
        cost,
    )];
    if robot_pets.can_afford(&recipe) {
        let _ = robot_pets.spend_parts(&recipe);
        *rank += 1;
    }
}

fn chapter_select_weapon_rank_panel_update(
    owner: Res<ChapterProgressionOwner>,
    select: Res<PlayerSelectState>,
    robot_pets: Res<RobotPetCollection>,
    mut header_q: Query<&mut Text, (With<WeaponRankHeader>, Without<WeaponRankRowText>)>,
    mut row_q: Query<(&WeaponRankRowText, &mut Text), Without<WeaponRankHeader>>,
) {
    if !(owner.is_changed() || select.is_changed() || robot_pets.is_changed()) {
        return;
    }
    let weapon_ranks = &selected_progression(&select, *owner).weapon_ranks;
    for mut text in header_q.iter_mut() {
        *text = Text::new(format_weapon_rank_header(&robot_pets));
    }
    for (row, mut text) in row_q.iter_mut() {
        let slot = row.0;
        let weapon_type = WEAPON_RANK_WEAPONS[slot];
        *text = Text::new(format_weapon_rank_row(
            slot,
            weapon_type,
            weapon_ranks,
            &robot_pets,
        ));
    }
}

fn chapter_select_economy_panel_update(
    economy: Res<SettlementEconomy>,
    world_site_registry: Res<WorldSiteRegistry>,
    mut header_q: Query<&mut Text, (With<EconomyPanelHeader>, Without<EconomyPanelSiteRow>)>,
    mut row_q: Query<(&EconomyPanelSiteRow, &mut Text), Without<EconomyPanelHeader>>,
) {
    if !(economy.is_changed() || world_site_registry.is_changed()) {
        return;
    }
    for mut text in header_q.iter_mut() {
        *text = Text::new(format_economy_header(&economy));
    }
    let settlements = map_settlements();
    for (row, mut text) in row_q.iter_mut() {
        if let Some(settlement) = settlements.get(row.0) {
            *text = Text::new(format_economy_site_row(
                settlement,
                &economy,
                &world_site_registry,
            ));
        }
    }
}

fn format_economy_header(economy: &SettlementEconomy) -> String {
    format!(
        "SETTLEMENT ECONOMY   stockpile: {}",
        economy.stockpile.summary()
    )
}

fn format_economy_site_row(
    settlement: &crate::chapters::MapSettlement,
    economy: &SettlementEconomy,
    world_site_registry: &WorldSiteRegistry,
) -> String {
    let state_badge = settlement
        .site_id
        .and_then(|id| world_site_registry.get(crate::resources::WorldSiteId(id)))
        .map(|site| match site.state {
            crate::resources::WorldSiteState::Liberated
            | crate::resources::WorldSiteState::Building
            | crate::resources::WorldSiteState::Shielded => "[FREE]",
            crate::resources::WorldSiteState::EnemyHeld
            | crate::resources::WorldSiteState::OccupiedAgain => "[HELD]",
            crate::resources::WorldSiteState::Contested
            | crate::resources::WorldSiteState::UnderAttack => "[RAID]",
            crate::resources::WorldSiteState::Damaged => "[DMGD]",
            crate::resources::WorldSiteState::Hidden => "[????]",
        })
        .unwrap_or("[????]");
    let build_count = economy.total_builds_for(settlement.anchor_id);
    let rec = economy.next_recommended_build(settlement.anchor_id, settlement.kind);
    format!(
        "{} {} | {} build{} | next: {}",
        state_badge,
        settlement.name,
        build_count,
        if build_count == 1 { "" } else { "s" },
        rec.label()
    )
}

fn rank_bar(rank: u32, max: u32) -> String {
    let filled = "■".repeat(rank as usize);
    let empty = "□".repeat((max - rank) as usize);
    format!("{filled}{empty}")
}

fn format_perk_header(perks: &PerkTree) -> String {
    format!("PERK TRAINING   unspent points: {}", perks.points_unspent)
}

fn format_perk_row(def: &crate::combat::perks::PerkDef, perks: &PerkTree) -> String {
    let rank = perks.rank(def.id);
    let bar = rank_bar(rank, def.max_rank);
    let status = if rank >= def.max_rank {
        "MAX".to_string()
    } else if perks.points_unspent > 0 {
        format!("+{} to next", def.max_rank - rank)
    } else {
        "—".to_string()
    };
    format!(
        "{:<24} {bar}  {}/{max}  {status}",
        def.name,
        rank,
        max = def.max_rank,
    )
}

fn branch_color(branch: crate::combat::perks::PerkBranch) -> Color {
    match branch {
        crate::combat::perks::PerkBranch::Heart => Color::srgb(1.0, 0.55, 0.60),
        crate::combat::perks::PerkBranch::Star => Color::srgb(1.0, 0.92, 0.42),
        crate::combat::perks::PerkBranch::Acrobat => Color::srgb(0.42, 0.88, 1.0),
    }
}

fn format_upgrade_header(upgrades: &UpgradeLedger) -> String {
    format!(
        "TECH UPGRADES   rejuvenation reserve: {:.0}",
        upgrades.rejuvenation_charge
    )
}

fn format_upgrade_row(
    def: &crate::combat::upgrades::TechUpgradeDef,
    upgrades: &UpgradeLedger,
    robot_pets: &RobotPetCollection,
) -> String {
    let rank = upgrades.rank(def.id);
    let bar = rank_bar(rank, def.max_rank);
    let status = if rank >= def.max_rank {
        "MAX".to_string()
    } else {
        let cost = def.id.next_rank_cost(rank + 1);
        let afford = if robot_pets.can_afford(&cost) {
            "READY"
        } else {
            "NEED PARTS"
        };
        format!("{}  {}", format_part_costs(&cost), afford)
    };
    format!(
        "{:<26} {bar}  {rank}/{max}  {status}",
        def.name,
        max = def.max_rank,
    )
}

fn track_color(track: crate::combat::upgrades::TechUpgradeTrack) -> Color {
    match track {
        crate::combat::upgrades::TechUpgradeTrack::Weapons => Color::srgb(1.0, 0.65, 0.35),
        crate::combat::upgrades::TechUpgradeTrack::Missiles => Color::srgb(1.0, 0.45, 0.45),
        crate::combat::upgrades::TechUpgradeTrack::Turrets => Color::srgb(0.80, 0.55, 1.0),
        crate::combat::upgrades::TechUpgradeTrack::Health => Color::srgb(0.42, 1.0, 0.55),
        crate::combat::upgrades::TechUpgradeTrack::Mechs => Color::srgb(0.42, 0.88, 1.0),
        crate::combat::upgrades::TechUpgradeTrack::Mobility => Color::srgb(0.55, 1.0, 0.75),
        crate::combat::upgrades::TechUpgradeTrack::Armor => Color::srgb(0.70, 0.70, 0.85),
        crate::combat::upgrades::TechUpgradeTrack::Gauntlets => Color::srgb(1.0, 0.75, 0.45),
    }
}

// ── Weapon Rank panel helpers ──────────────────────────────────────────────────
const CHAPTER_PERK_IDS: [&str; 6] = [
    "heart_vitality",
    "heart_regen",
    "star_focus",
    "star_charges",
    "acro_evasion",
    "acro_parry",
];

const WEAPON_RANK_WEAPONS: [WeaponType; 6] = [
    WeaponType::Pistol,
    WeaponType::Rifle,
    WeaponType::Shotgun,
    WeaponType::Rocket,
    WeaponType::Laser,
    WeaponType::Grenade,
];

fn format_weapon_rank_header(robot_pets: &RobotPetCollection) -> String {
    let boards = robot_pets.part_count(RobotPartKind::CircuitBoard);
    format!("WEAPON UPGRADES   circuit boards: {boards}")
}

fn format_weapon_rank_row(
    slot: usize,
    weapon_type: WeaponType,
    ranks: &WeaponRanks,
    robot_pets: &RobotPetCollection,
) -> String {
    let rank = ranks.ranks[slot];
    let bar = rank_bar(rank, MAX_WEAPON_RANK);
    let label = WeaponRanks::rank_label(rank);
    let status = if rank >= MAX_WEAPON_RANK {
        "MAX".to_string()
    } else {
        let cost = WeaponRanks::upgrade_cost(rank);
        let recipe = [crate::world::robot_pets::PartCost::new(
            RobotPartKind::CircuitBoard,
            cost,
        )];
        if robot_pets.can_afford(&recipe) {
            format!("{cost} boards  READY")
        } else {
            let have = robot_pets.part_count(RobotPartKind::CircuitBoard);
            format!("{cost} boards  NEED {}", cost.saturating_sub(have))
        }
    };
    format!(
        "{:<24} {bar}  {rank}/{MAX_WEAPON_RANK}  {label}  {status}",
        weapon_type.display_name(),
    )
}

// ── Player Select ─────────────────────────────────────────────────────────────
#[derive(Component)]
struct PlayerSelectRoot;

#[derive(Component)]
struct PlayerSlotCard(u8);

#[derive(Component)]
struct PlayerSlotCharText(u8);

#[derive(Component)]
struct PlayerSlotStatusText(u8);

#[derive(Component)]
struct PlayerSlotInputText(u8);

#[derive(Component)]
struct PlayerSelectPrompt;
#[derive(Component, Clone, Copy)]
struct PlayerSelectButton {
    player_index: u8,
    action: PlayerSelectAction,
}
#[derive(Clone, Copy)]
enum PlayerSelectAction {
    PreviousCharacter,
    NextCharacter,
    ToggleReady,
    Customize,
    AdvancedDesign,
    CharacterAssets,
    JoinLeave,
    Back,
    Begin,
}

fn setup_player_select(
    mut commands: Commands,
    mut select: ResMut<PlayerSelectState>,
    theme: Res<UiTheme>,
) {
    // P1 is always present. Preserve saved/customized blueprints when this
    // screen is re-entered from the in-game character designer.
    if select.slots.iter().any(|slot| slot.joined) {
        for slot in &mut select.slots {
            slot.ready = false;
            slot.stick_cooldown = 0.0;
        }
        select.slots[0].joined = true;
    } else {
        let saved_slots = select.slots.clone();
        *select = PlayerSelectState::default();
        for (slot, saved) in select.slots.iter_mut().zip(saved_slots) {
            slot.character_index = saved.character_index;
            slot.skin_idx = saved.skin_idx;
            slot.outfit_idx = saved.outfit_idx;
            slot.accent_idx = saved.accent_idx;
            slot.hair_idx = saved.hair_idx;
            slot.eye_idx = saved.eye_idx;
            slot.has_hood = saved.has_hood;
            slot.has_cape = saved.has_cape;
            slot.has_gloves = saved.has_gloves;
            slot.has_boots = saved.has_boots;
            slot.has_shoulder_pads = saved.has_shoulder_pads;
            slot.has_visor = saved.has_visor;
            slot.part_loadout = saved.part_loadout;
            slot.blueprint = saved.blueprint;
            slot.studio_spec = saved.studio_spec;
        }
        select.slots[0].joined = true;
    }

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::FlexStart,
                row_gap: Val::Px(28.0),
                padding: UiRect::all(Val::Px(24.0)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.01, 0.05, 1.0)),
            ScrollPosition::default(),
            MenuScrollPanel,
            PlayerSelectRoot,
        ))
        .with_children(|root| {
            // Title
            root.spawn((
                Text::new("SELECT YOUR CREW"),
                TextFont {
                    font_size: FontSize::Px(54.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.25)),
            ));

            // Prompt line (updated dynamically)
            root.spawn((
                Text::new("All players ready to begin"),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.65, 0.85)),
                PlayerSelectPrompt,
            ));

            // Four player slot cards
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(16.0),
                row_gap: Val::Px(16.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|row| {
                for i in 0..4u8 {
                    row.spawn((
                        Node {
                            width: Val::Px(248.0),
                            min_height: Val::Px(324.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::FlexStart,
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(16.0)),
                            row_gap: Val::Px(12.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.03, 0.03, 0.08, 0.9)),
                        PlayerSlotCard(i),
                    ))
                    .with_children(|card| {
                        // "P1" header
                        card.spawn((
                            Text::new(format!("P{}", i + 1)),
                            TextFont {
                                font_size: FontSize::Px(30.0),
                                ..default()
                            },
                            TextColor(theme.player_accent(i)),
                        ));

                        // Character name / join prompt
                        let char_text = if i == 0 {
                            format!("< {} >", HERO_ROSTER[0])
                        } else {
                            "- - - - -".to_string()
                        };
                        card.spawn((
                            Text::new(char_text),
                            TextFont {
                                font_size: FontSize::Px(19.0),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            PlayerSlotCharText(i),
                        ));

                        // Slot label
                        let ctrl_text = if i == 0 { "PLAYER 1" } else { "OPEN SLOT" };
                        card.spawn((
                            Text::new(ctrl_text),
                            TextFont {
                                font_size: FontSize::Px(13.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.55, 0.55, 0.75)),
                            PlayerSlotInputText(i),
                        ));

                        // Status
                        let status = if i == 0 { "Not ready" } else { "Available" };
                        card.spawn((
                            Text::new(status),
                            TextFont {
                                font_size: FontSize::Px(15.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.65, 0.65, 0.4)),
                            PlayerSlotStatusText(i),
                        ));

                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(6.0),
                            justify_content: JustifyContent::Center,
                            ..default()
                        })
                        .with_children(|row| {
                            spawn_player_select_button(
                                row,
                                "PREVIOUS",
                                i,
                                PlayerSelectAction::PreviousCharacter,
                                108.0,
                            );
                            spawn_player_select_button(
                                row,
                                "NEXT",
                                i,
                                PlayerSelectAction::NextCharacter,
                                96.0,
                            );
                        });
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(6.0),
                            row_gap: Val::Px(6.0),
                            flex_wrap: FlexWrap::Wrap,
                            justify_content: JustifyContent::Center,
                            ..default()
                        })
                        .with_children(|row| {
                            if i > 0 {
                                spawn_player_select_button(
                                    row,
                                    "JOIN / LEAVE",
                                    i,
                                    PlayerSelectAction::JoinLeave,
                                    130.0,
                                );
                            }
                            spawn_player_select_button(
                                row,
                                "READY",
                                i,
                                PlayerSelectAction::ToggleReady,
                                82.0,
                            );
                            spawn_player_select_button(
                                row,
                                "EDIT HERO",
                                i,
                                PlayerSelectAction::Customize,
                                108.0,
                            );
                            spawn_player_select_button(
                                row,
                                "ADVANCED",
                                i,
                                PlayerSelectAction::AdvancedDesign,
                                108.0,
                            );
                            spawn_player_select_button(
                                row,
                                "ASSET EDITOR",
                                i,
                                PlayerSelectAction::CharacterAssets,
                                108.0,
                            );
                        });
                    });
                }
            });

            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|row| {
                spawn_player_select_button(row, "BACK", 0, PlayerSelectAction::Back, 120.0);
                spawn_player_select_button(row, "BEGIN GAME", 0, PlayerSelectAction::Begin, 170.0);
            });
        });
}

fn despawn_player_select(mut commands: Commands, q: Query<Entity, With<PlayerSelectRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
}

fn spawn_player_select_button(
    parent: &mut ChildSpawnerCommands,
    label: &'static str,
    player_index: u8,
    action: PlayerSelectAction,
    width: f32,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(width),
                min_height: Val::Px(38.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.12, 0.22)),
            BorderColor::all(Color::srgb(0.24, 0.40, 0.62)),
            PlayerSelectButton {
                player_index,
                action,
            },
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextLayout::justify(Justify::Center),
                TextColor(Color::WHITE),
            ));
        });
}

fn sync_player_select_button_availability(
    mut commands: Commands,
    select: Res<PlayerSelectState>,
    buttons: Query<(Entity, &PlayerSelectButton, Has<MenuButtonDisabled>)>,
) {
    for (entity, button, disabled) in buttons.iter() {
        let should_disable =
            matches!(button.action, PlayerSelectAction::Begin) && !select.all_ready();
        if should_disable && !disabled {
            commands.entity(entity).insert(MenuButtonDisabled);
        } else if !should_disable && disabled {
            commands
                .entity(entity)
                .remove::<(MenuButtonDisabled, InteractionDisabled)>();
        }
    }
}

fn player_select_update(
    interaction_q: Query<
        (
            &Interaction,
            &PlayerSelectButton,
            Option<&MenuButtonDisabled>,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut select: ResMut<PlayerSelectState>,
    assignments: Res<GamepadAssignments>,
    mut config: ResMut<LocalPlayerConfig>,
    mut design_data: ResMut<CharacterDesignData>,
    mut imported_forge_return: ResMut<ImportedForgeReturnTarget>,
    mut next_state: ResMut<NextState<AppState>>,
    // Text queries — each set is disjoint via Without<> guards
    mut char_q: Query<
        (&mut Text, &PlayerSlotCharText),
        (
            Without<PlayerSlotStatusText>,
            Without<PlayerSlotInputText>,
            Without<PlayerSelectPrompt>,
        ),
    >,
    mut status_q: Query<
        (&mut Text, &PlayerSlotStatusText),
        (
            Without<PlayerSlotCharText>,
            Without<PlayerSlotInputText>,
            Without<PlayerSelectPrompt>,
        ),
    >,
    mut input_q: Query<
        (&mut Text, &PlayerSlotInputText),
        (
            Without<PlayerSlotCharText>,
            Without<PlayerSlotStatusText>,
            Without<PlayerSelectPrompt>,
        ),
    >,
    mut prompt_q: Query<
        &mut Text,
        (
            With<PlayerSelectPrompt>,
            Without<PlayerSlotCharText>,
            Without<PlayerSlotStatusText>,
            Without<PlayerSlotInputText>,
        ),
    >,
    mut card_q: Query<(&mut BackgroundColor, &PlayerSlotCard)>,
) {
    let roster_len = HERO_ROSTER.len();
    select.slots[0].joined = true;

    for (interaction, button, disabled) in interaction_q.iter() {
        if *interaction != Interaction::Pressed || disabled.is_some() {
            continue;
        }

        match button.action {
            PlayerSelectAction::Back => {
                next_state.set(AppState::MainMenu);
                return;
            }
            PlayerSelectAction::Begin => {
                if select.all_ready() {
                    config.active = select.active_count().max(1);
                    next_state.set(AppState::ChapterSelect);
                    return;
                }
            }
            action => {
                let idx = button.player_index as usize;
                let Some(slot) = select.slots.get_mut(idx) else {
                    continue;
                };
                match action {
                    PlayerSelectAction::PreviousCharacter if slot.joined && !slot.ready => {
                        slot.character_index = (slot.character_index + roster_len - 1) % roster_len;
                        slot.studio_spec = None;
                    }
                    PlayerSelectAction::NextCharacter if slot.joined && !slot.ready => {
                        slot.character_index = (slot.character_index + 1) % roster_len;
                        slot.studio_spec = None;
                    }
                    PlayerSelectAction::ToggleReady if slot.joined => {
                        slot.ready = !slot.ready;
                    }
                    PlayerSelectAction::Customize if slot.joined && !slot.ready => {
                        design_data.player_index = idx;
                        design_data.return_target = CharacterDesignReturnTarget::PlayerSelect;
                        next_state.set(AppState::CharacterDesign);
                        return;
                    }
                    PlayerSelectAction::AdvancedDesign if slot.joined && !slot.ready => {
                        design_data.player_index = idx;
                        design_data.return_target = CharacterDesignReturnTarget::PlayerSelect;
                        next_state.set(AppState::CharacterStudio);
                        return;
                    }
                    PlayerSelectAction::CharacterAssets if slot.joined && !slot.ready => {
                        *imported_forge_return = ImportedForgeReturnTarget::PlayerSelect;
                        next_state.set(AppState::ImportedCharacterForge);
                        return;
                    }
                    PlayerSelectAction::JoinLeave if idx > 0 => {
                        if slot.joined {
                            if !slot.ready {
                                slot.joined = false;
                            }
                        } else {
                            slot.joined = true;
                            slot.character_index = idx % roster_len;
                            slot.ready = false;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ── Update text nodes ─────────────────────────────────────────────────────
    for (mut t, marker) in char_q.iter_mut() {
        let s = &select.slots[marker.0 as usize];
        *t = Text::new(if s.joined {
            format!(
                "< {}{} >",
                HERO_ROSTER[s.character_index % roster_len],
                if s.studio_spec.is_some() {
                    " — STUDIO CUSTOM"
                } else {
                    ""
                }
            )
        } else {
            "- - - - -".to_string()
        });
    }

    for (mut t, marker) in status_q.iter_mut() {
        let i = marker.0 as usize;
        let s = &select.slots[i];
        *t = Text::new(if !s.joined {
            "Available".to_string()
        } else if s.ready {
            "✓  READY".to_string()
        } else if s.studio_spec.is_some() {
            "Custom model loaded — ready up".to_string()
        } else {
            "Not ready".to_string()
        });
    }

    for (mut t, marker) in input_q.iter_mut() {
        let i = marker.0 as usize;
        let s = &select.slots[i];
        if let Some(gamepad) = assignments.gamepad_for_player(i as u8) {
            *t = Text::new(format!("CONTROLLER {}", gamepad.index()));
        } else if assignments.native_player() == Some(i as u8) {
            *t = Text::new("NATIVE CONTROLLER");
        } else if s.joined && i == 0 {
            *t = Text::new("KEYBOARD / MOUSE");
        } else if s.joined {
            *t = Text::new(format!("PLAYER {} — RECONNECT", i + 1));
        } else {
            *t = Text::new("PRESS START");
        }
    }

    // Update card background to reflect state
    for (mut bg, marker) in card_q.iter_mut() {
        let s = &select.slots[marker.0 as usize];
        *bg = BackgroundColor(if s.ready {
            Color::srgba(0.06, 0.22, 0.06, 0.95)
        } else if s.joined {
            Color::srgba(0.06, 0.08, 0.20, 0.95)
        } else {
            Color::srgba(0.03, 0.03, 0.08, 0.8)
        });
    }

    // Update top prompt
    let joined = select.active_count();
    let ready = select.slots.iter().filter(|s| s.joined && s.ready).count() as u8;
    if let Ok(mut t) = prompt_q.single_mut() {
        *t = Text::new(format!(
            "{}/{} players ready  —  {}",
            ready,
            joined,
            if ready == joined && joined > 0 {
                "Begin is available"
            } else {
                "ready the crew"
            }
        ));
    }
}

/// Wait briefly before assigning a Bevy-only lobby Start while native-only P1
/// is active. Either backend may publish the same physical press first.
const NATIVE_JOIN_CORRELATION_FRAMES: u8 = 3;

fn assign_player_select_gamepad(
    gamepad: Entity,
    assignments: &mut GamepadAssignments,
    select: &mut PlayerSelectState,
) {
    let joined = std::array::from_fn(|index| select.slots[index].joined);
    if let Some(player_index) = assignments.assign_next(gamepad, &joined) {
        let slot = &mut select.slots[player_index as usize];
        if !slot.joined {
            slot.joined = true;
            slot.character_index = player_index as usize % HERO_ROSTER.len();
            slot.ready = false;
        }
    }
}

fn player_select_controller_join(
    state: Res<State<AppState>>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    mut assignments: ResMut<GamepadAssignments>,
    mut select: ResMut<PlayerSelectState>,
    mut pending_native_joins: Local<Vec<(Entity, u8)>>,
) {
    let raw_start_presses = button_events
        .read()
        .filter(|event| {
            event.state == ButtonState::Pressed
                && event.button == GamepadButton::Start
                && gamepads.get(event.entity).is_ok()
        })
        .map(|event| event.entity)
        .collect::<Vec<_>>();
    if *state.get() != AppState::PlayerSelect {
        pending_native_joins.clear();
        return;
    }

    select.slots[0].joined = true;

    if assignments.native_start_guard_active() {
        pending_native_joins.clear();
    } else {
        let mut matured = Vec::new();
        let mut index = 0;
        while index < pending_native_joins.len() {
            pending_native_joins[index].1 = pending_native_joins[index].1.saturating_sub(1);
            if pending_native_joins[index].1 == 0 {
                matured.push(pending_native_joins.remove(index).0);
            } else {
                index += 1;
            }
        }
        for gamepad in matured {
            if gamepads.get(gamepad).is_ok() && assignments.player_for_gamepad(gamepad).is_none() {
                assign_player_select_gamepad(gamepad, &mut assignments, &mut select);
            }
        }
    }

    let mut pressed: Vec<Entity> = gamepads
        .iter()
        .filter_map(|(entity, gamepad)| {
            gamepad.just_pressed(GamepadButton::Start).then_some(entity)
        })
        .collect();
    pressed.extend(raw_start_presses);
    pressed.sort_by_key(|entity| entity.index());
    pressed.dedup();

    for gamepad in pressed.iter().copied() {
        if assignments.player_for_gamepad(gamepad).is_some() {
            continue;
        }

        // With no cross-backend hardware ID, native + Bevy Start edges close
        // together are ambiguous: they are usually one dual-exposed P1 pad.
        // Delay a Bevy-first edge long enough for its native half to arrive;
        // a genuinely separate controller joins automatically after the wait.
        let native_only_primary =
            assignments.native_player() == Some(0) && assignments.gamepad_for_player(0).is_none();
        if native_only_primary && !assignments.native_start_guard_active() {
            if !pending_native_joins
                .iter()
                .any(|(candidate, _)| *candidate == gamepad)
            {
                pending_native_joins.push((gamepad, NATIVE_JOIN_CORRELATION_FRAMES));
            }
            continue;
        }
        if assignments.native_start_guard_active() {
            continue;
        }
        assign_player_select_gamepad(gamepad, &mut assignments, &mut select);
    }

    // Native GameController is P1-only and is synchronized in PreUpdate after
    // the short dual-backend discovery window. It never consumes a lobby slot.
}

// ── HUD Setup ─────────────────────────────────────────────────────────────────
fn setup_hud(
    mut commands: Commands,
    config: Res<LocalPlayerConfig>,
    theme: Res<UiTheme>,
    existing_hud: Query<Entity, With<HudRoot>>,
) {
    if !existing_hud.is_empty() {
        return;
    }

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            HudRoot,
        ))
        .with_children(|root| {
            // ── Per-player stat panels ───────────────────────────────────────────
            let active = config.active.clamp(1, 4);
            for player_index in 0..active {
                root.spawn((
                    damage_vignette_node(player_index, active),
                    BackgroundColor(Color::srgba(0.8, 0.0, 0.0, 0.0)),
                    DamageVignette {
                        player_index,
                        alpha: 0.0,
                    },
                ));
                root.spawn((
                    damage_vignette_node(player_index, active),
                    BackgroundColor(Color::srgba(0.0, 0.42, 0.58, 0.0)),
                    UnderwaterOverlay { player_index },
                ));
                spawn_player_hud_panel(root, player_index, active, &theme);
            }

            // ── Top-right wave/enemy ──────────────────────────────────────────────
            root.spawn(Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(16.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Wave 1"),
                    TextFont {
                        font_size: FontSize::Px(20.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.4, 0.2)),
                    WaveText,
                ));
                panel.spawn((
                    Text::new("Enemies: 0"),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.7, 0.3)),
                    EnemyCountText,
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(22.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.2, 0.1)),
                    BossAlertText,
                ));
            });

            // ── Per-player aim reticles ──────────────────────────────────────────
            for player_index in 0..active {
                let (left, top) = reticle_center_percent(player_index, active);
                root.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(left),
                        top: Val::Percent(top),
                        margin: UiRect::all(Val::Px(-8.0)),
                        width: Val::Px(16.0),
                        height: Val::Px(16.0),
                        ..default()
                    },
                    AimReticle { player_index },
                ))
                .with_children(|ch| {
                    for node in [
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(7.0),
                            width: Val::Px(16.0),
                            height: Val::Px(2.0),
                            ..default()
                        },
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(7.0),
                            top: Val::Px(0.0),
                            width: Val::Px(2.0),
                            height: Val::Px(16.0),
                            ..default()
                        },
                    ] {
                        ch.spawn((
                            node,
                            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
                            AimReticleBar { player_index },
                        ));
                    }
                });
            }

            // ── Message popup (center top) ────────────────────────────────────────
            root.spawn(Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(30.0),
                top: Val::Px(80.0),
                width: Val::Percent(40.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|msg| {
                msg.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(22.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.9, 0.3)),
                    MessageText,
                ));
            });

            // ── Context guidance (nearby actions / city spy hints) ─────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(18.0),
                    bottom: Val::Px(76.0),
                    width: Val::Px(390.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(10.0)),
                    row_gap: Val::Px(4.0),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.035, 0.060, 0.88)),
                GuidancePanelRoot,
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.95, 0.82, 0.36)),
                    GuidanceTitleText,
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.86, 0.92, 1.0)),
                    GuidanceBodyText,
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(12.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.52, 0.94, 1.0)),
                    GuidanceActionText,
                ));
            });

            // ── Discussion panel (talking to settlement NPCs) ───────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(17.0),
                    bottom: Val::Px(74.0),
                    width: Val::Percent(66.0),
                    min_height: Val::Px(126.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(14.0)),
                    row_gap: Val::Px(7.0),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.025, 0.035, 0.060, 0.94)),
                DiscussionPanelRoot,
            ))
            .with_children(|panel| {
                panel
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(10.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Text::new(""),
                            TextFont {
                                font_size: FontSize::Px(16.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.52, 0.94, 1.0)),
                            DiscussionTitleText,
                        ));
                        row.spawn((
                            Text::new(""),
                            TextFont {
                                font_size: FontSize::Px(14.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.95, 0.78, 0.38)),
                            DiscussionSpeakerText,
                        ));
                    });
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(20.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.93, 0.96, 1.0)),
                    DiscussionBodyText,
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.58, 0.70, 0.84)),
                    DiscussionHintText,
                ));
            });

            // ── Active puzzle objective (upper center-left) ─────────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(34.0),
                    top: Val::Px(18.0),
                    width: Val::Px(420.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(10.0)),
                    row_gap: Val::Px(3.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.04, 0.05, 0.10, 0.84)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.92, 0.30)),
                    ObjectiveTitleText,
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.72, 0.82, 1.0)),
                    ObjectiveStatusText,
                ));
            });

            // ── Primary weapon selector bar (bottom center) ───────────────────────
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(16.0),
                left: Val::Percent(25.0),
                column_gap: Val::Px(4.0),
                flex_direction: FlexDirection::Row,
                ..default()
            })
            .with_children(|bar| {
                for name in &["1:Pop", "2:Comet", "3:Fan", "4:Nova", "5:Ray", "6:Bubble"] {
                    bar.spawn((
                        Node {
                            padding: UiRect::all(Val::Px(4.0)),
                            width: Val::Px(80.0),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.1, 0.1, 0.2, 0.7)),
                    ))
                    .with_children(|slot| {
                        slot.spawn((
                            Text::new(*name),
                            TextFont {
                                font_size: FontSize::Px(11.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.7, 0.8, 1.0)),
                        ));
                    });
                }
            });

            // ── Per-player loadout panel (I / LB+Select, hidden) ─────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(18.0),
                    top: Val::Percent(10.0),
                    width: Val::Percent(64.0),
                    max_height: Val::Percent(80.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(18.0)),
                    row_gap: Val::Px(10.0),
                    display: Display::None,
                    overflow: Overflow::clip_y(),
                    border_radius: BorderRadius::all(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(theme.canvas),
                BorderColor::all(theme.energy),
                LoadoutPanelRoot,
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("P1 STAR LOADOUT"),
                    TextFont {
                        font_size: FontSize::Px(25.0),
                        ..default()
                    },
                    TextColor(theme.objective),
                    LoadoutTitleText,
                ));
                panel.spawn((
                    Text::new("D-PAD/STICK: navigate   A: equip   B or LB+SELECT: close"),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(theme.text_muted),
                    UiPromptText(UiPromptKind::Loadout),
                ));
                panel
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|tabs| {
                        for tab in LoadoutTab::ALL {
                            tabs.spawn((
                                Button,
                                Node {
                                    flex_grow: 1.0,
                                    min_height: Val::Px(38.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.08, 0.13, 0.25)),
                                BorderColor::all(Color::srgb(0.16, 0.35, 0.55)),
                                LoadoutTabButton(tab),
                            ))
                            .with_child((
                                Text::new(tab.label()),
                                TextFont {
                                    font_size: FontSize::Px(13.0),
                                    ..default()
                                },
                                TextColor(theme.text_primary),
                            ));
                        }
                    });
                panel
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(5.0),
                        ..default()
                    })
                    .with_children(|entries| {
                        for index in 0..SABRE_RELIC_CATALOG.len() {
                            entries
                                .spawn((
                                    Button,
                                    Node {
                                        width: Val::Percent(100.0),
                                        min_height: Val::Px(38.0),
                                        padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)),
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.055, 0.075, 0.14)),
                                    BorderColor::all(Color::srgb(0.12, 0.24, 0.40)),
                                    LoadoutEntryButton(index),
                                ))
                                .with_child((
                                    Text::new(""),
                                    TextFont {
                                        font_size: FontSize::Px(15.0),
                                        ..default()
                                    },
                                    TextColor(theme.text_primary),
                                    LoadoutEntryLabel(index),
                                ));
                        }
                    });
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.50, 0.94, 0.78)),
                    LoadoutStatusText,
                ));
                panel
                    .spawn((
                        Button,
                        Node {
                            width: Val::Px(170.0),
                            min_height: Val::Px(38.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.32, 0.10, 0.18)),
                        BorderColor::all(Color::srgb(0.85, 0.28, 0.42)),
                        LoadoutCloseButton,
                    ))
                    .with_child((
                        Text::new("CLOSE"),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
            });

            // ── Crafting panel (toggle C, hidden by default) ──────────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(16.0),
                    top: Val::Px(140.0),
                    width: Val::Px(380.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(12.0)),
                    row_gap: Val::Px(6.0),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.05, 0.12, 0.92)),
                CraftingPanelRoot,
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("CRAFTING — D-PAD / STICK: choose   A: craft   B or SELECT: close"),
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.0, 0.9, 1.0)),
                    UiPromptText(UiPromptKind::Crafting),
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.85, 0.85)),
                    CraftingPanelText,
                ));
            });
        });
}

fn damage_vignette_node(player_index: u8, active: u8) -> Node {
    let mut node = Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        ..default()
    };

    match active {
        2 => {
            node.height = Val::Percent(50.0);
            if player_index == 0 {
                node.top = Val::Percent(0.0);
            } else {
                node.bottom = Val::Percent(0.0);
            }
        }
        3 => match player_index {
            0 => {
                node.width = Val::Percent(50.0);
                node.height = Val::Percent(50.0);
                node.top = Val::Percent(0.0);
                node.left = Val::Percent(0.0);
            }
            1 => {
                node.width = Val::Percent(50.0);
                node.height = Val::Percent(50.0);
                node.top = Val::Percent(0.0);
                node.right = Val::Percent(0.0);
            }
            _ => {
                node.height = Val::Percent(50.0);
                node.bottom = Val::Percent(0.0);
            }
        },
        4 => {
            node.width = Val::Percent(50.0);
            node.height = Val::Percent(50.0);
            if player_index == 0 || player_index == 2 {
                node.left = Val::Percent(0.0);
            } else {
                node.right = Val::Percent(0.0);
            }
            if player_index < 2 {
                node.top = Val::Percent(0.0);
            } else {
                node.bottom = Val::Percent(0.0);
            }
        }
        _ => {}
    }

    node
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct HudPanelPlacement {
    left: Option<f32>,
    right: Option<f32>,
    top: Option<f32>,
    bottom: Option<f32>,
    width: f32,
}

fn hud_panel_placement(
    player_index: u8,
    active_players: u8,
    window_size: Vec2,
    safe_area_fraction: f32,
) -> HudPanelPlacement {
    let active = active_players.clamp(1, 4);
    let margin = (window_size.min_element() * safe_area_fraction.clamp(0.0, 0.08)).clamp(8.0, 48.0);
    let (left, right, top, bottom) = match (player_index, active) {
        (_, 1) | (0, 2) | (0, 3 | 4) => (Some(margin), None, Some(margin), None),
        (1, 2) => (Some(margin), None, None, Some(margin)),
        (1, 3 | 4) => (None, Some(margin), Some(margin), None),
        (2, 3 | 4) => (Some(margin), None, None, Some(margin)),
        (3, 4) => (None, Some(margin), None, Some(margin)),
        _ => (Some(margin), None, Some(margin), None),
    };
    HudPanelPlacement {
        left,
        right,
        top,
        bottom,
        width: if active >= 3 { 232.0 } else { 246.0 },
    }
}

fn apply_hud_panel_placement(node: &mut Node, placement: HudPanelPlacement) {
    node.left = placement.left.map_or(Val::Auto, Val::Px);
    node.right = placement.right.map_or(Val::Auto, Val::Px);
    node.top = placement.top.map_or(Val::Auto, Val::Px);
    node.bottom = placement.bottom.map_or(Val::Auto, Val::Px);
    node.width = Val::Px(placement.width);
}

fn responsive_hud_layout_system(
    settings: Res<GameSettings>,
    config: Res<LocalPlayerConfig>,
    theme: Res<UiTheme>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut panels: Query<(
        &PlayerHudPanel,
        &mut Node,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let size = Vec2::new(window.width(), window.height());
    for (panel, mut node, mut background, mut border) in panels.iter_mut() {
        apply_hud_panel_placement(
            &mut node,
            hud_panel_placement(
                panel.player_index,
                config.active,
                size,
                settings.safe_area_fraction,
            ),
        );
        background.0 = if settings.high_contrast_ui {
            Color::srgba(0.0, 0.0, 0.0, 0.97)
        } else {
            theme.panel
        };
        *border = BorderColor::all(theme.player_accent(panel.player_index));
    }
}

fn spawn_player_hud_panel(
    parent: &mut ChildSpawnerCommands,
    player_index: u8,
    active_players: u8,
    theme: &UiTheme,
) {
    let placement = hud_panel_placement(
        player_index,
        active_players,
        Vec2::new(1280.0, 720.0),
        0.025,
    );
    let mut panel_node = Node {
        position_type: PositionType::Absolute,
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(5.0),
        padding: UiRect::all(Val::Px(8.0)),
        border: UiRect::left(Val::Px(3.0)),
        ..default()
    };
    apply_hud_panel_placement(&mut panel_node, placement);
    parent
        .spawn((
            panel_node,
            BackgroundColor(theme.panel),
            BorderColor::all(theme.player_accent(player_index)),
            PlayerHudPanel { player_index },
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(format!("P{}", player_index + 1)),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(theme.player_accent(player_index)),
                PlayerHudText {
                    player_index,
                    kind: PlayerHudTextKind::Header,
                },
            ));
            spawn_bar(
                panel,
                player_index,
                "HP",
                PlayerHudBarKind::Health,
                theme.health,
            );
            spawn_bar(
                panel,
                player_index,
                "AR",
                PlayerHudBarKind::Armor,
                theme.armor,
            );
            spawn_bar(
                panel,
                player_index,
                "ST",
                PlayerHudBarKind::Stamina,
                theme.stamina,
            );
            spawn_bar(
                panel,
                player_index,
                "JP",
                PlayerHudBarKind::Jetpack,
                theme.energy,
            );
            spawn_bar(
                panel,
                player_index,
                "CL",
                PlayerHudBarKind::Climb,
                theme.climb,
            );
            spawn_bar(
                panel,
                player_index,
                "AIR",
                PlayerHudBarKind::Air,
                theme.energy,
            );
            spawn_player_hud_text(panel, player_index, PlayerHudTextKind::Credits, "¢ 0", 13.0);
            spawn_player_hud_text(panel, player_index, PlayerHudTextKind::Level, "LVL 1", 13.0);
            spawn_player_hud_text(
                panel,
                player_index,
                PlayerHudTextKind::Element,
                "Element: None",
                12.0,
            );
            spawn_player_hud_text(
                panel,
                player_index,
                PlayerHudTextKind::WeaponName,
                "Starlight Popper",
                13.0,
            );
            spawn_player_hud_text(
                panel,
                player_index,
                PlayerHudTextKind::Ammo,
                "60 / 60",
                12.0,
            );
            spawn_player_hud_text(
                panel,
                player_index,
                PlayerHudTextKind::SpecialAmmo,
                "7:Star 8:Tri 9:Moon 0:Sprite",
                10.0,
            );
            spawn_player_hud_text(
                panel,
                player_index,
                PlayerHudTextKind::TraversalStatus,
                "TRAVERSAL: GRAPPLE",
                10.0,
            );
            spawn_player_hud_text(
                panel,
                player_index,
                PlayerHudTextKind::SabreStatus,
                "SABER: WAVE 1",
                10.0,
            );
        });
}

fn spawn_player_hud_text(
    parent: &mut ChildSpawnerCommands,
    player_index: u8,
    kind: PlayerHudTextKind,
    text: &str,
    font_size: f32,
) {
    parent.spawn((
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(font_size),
            ..default()
        },
        TextColor(Color::srgb(0.78, 0.82, 0.94)),
        PlayerHudText { player_index, kind },
    ));
}

fn spawn_bar(
    parent: &mut ChildSpawnerCommands,
    player_index: u8,
    label: &str,
    kind: PlayerHudBarKind,
    color: Color,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            },
            PlayerHudBarRow { player_index, kind },
            if kind == PlayerHudBarKind::Air {
                Visibility::Hidden
            } else {
                Visibility::Visible
            },
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.8)),
            ));
            row.spawn((
                Node {
                    width: Val::Px(188.0),
                    height: Val::Px(9.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.8)),
            ))
            .with_children(|bg| {
                bg.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(color),
                    PlayerHudBar { player_index, kind },
                ));
            });
        });
}

fn reticle_center_percent(player_index: u8, active: u8) -> (f32, f32) {
    match (player_index, active) {
        (_, 0 | 1) => (50.0, 50.0),
        (0, 2) => (50.0, 25.0),
        (1, 2) => (50.0, 75.0),
        (0, 3 | 4) => (25.0, 25.0),
        (1, 3 | 4) => (75.0, 25.0),
        (2, 3) => (50.0, 75.0),
        (2, 4) => (25.0, 75.0),
        (3, 4) => (75.0, 75.0),
        _ => (50.0, 50.0),
    }
}

fn reticle_color(state: AimReticleState) -> Color {
    match state {
        AimReticleState::Idle => Color::srgba(1.0, 1.0, 1.0, 0.72),
        AimReticleState::Aiming => Color::srgba(0.25, 0.82, 1.0, 0.95),
        AimReticleState::Target => Color::srgba(1.0, 0.78, 0.18, 0.98),
        AimReticleState::Locked => Color::srgba(0.20, 1.0, 0.48, 1.0),
        AimReticleState::Obstructed => Color::srgba(1.0, 0.30, 0.22, 0.92),
        AimReticleState::Charging => Color::srgba(0.85, 0.34, 1.0, 1.0),
    }
}

fn aim_reticle_system(
    player_q: Query<
        (
            &PlayerIndex,
            &AimSolution,
            &WeaponInventory,
            &PlayerCameraRef,
        ),
        With<Player>,
    >,
    camera_q: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut reticle_q: Query<(&AimReticle, &mut Node)>,
    mut bar_q: Query<(&AimReticleBar, &mut BackgroundColor)>,
) {
    for (marker, mut node) in reticle_q.iter_mut() {
        let Some((_, aim, weapons, camera_ref)) = player_q
            .iter()
            .find(|(index, _, _, _)| index.0 == marker.player_index)
        else {
            continue;
        };
        if let Ok((camera, camera_transform)) = camera_q.get(camera_ref.0) {
            if let Ok(position) = camera.world_to_viewport(camera_transform, aim.aim_point) {
                node.left = Val::Px(position.x - 8.0);
                node.top = Val::Px(position.y - 8.0);
                node.margin = UiRect::ZERO;
            }
        }
        let color = reticle_color(aim.reticle_state(weapons.active().charge_progress > 0.05));
        for (bar, mut background) in bar_q.iter_mut() {
            if bar.player_index == marker.player_index {
                background.0 = color;
            }
        }
    }
}

// ── HUD Update ────────────────────────────────────────────────────────────────
fn hud_update_system(
    player_q: Query<
        (
            &PlayerIndex,
            &Health,
            &PlayerStats,
            &JetpackState,
            &ClimbState,
            &WeaponInventory,
            &SpecialWeaponInventory,
            &ArmorSet,
            &BeamSabre,
            (
                &TraversalModeState,
                &BoardBoostState,
                &PlayerProgression,
                Option<&TrickState>,
                Option<&StuntRunState>,
                Option<&RooftopTrialProgress>,
                &WaterTraversalState,
            ),
        ),
        With<Player>,
    >,
    wave: Res<WaveInfo>,
    current: Res<CurrentChapter>,
    missile_q: Query<&TrackingMissile>,
    mut row_q: Query<(&mut Visibility, &PlayerHudBarRow)>,
    mut hud_visuals: ParamSet<(
        Query<(&mut Node, &mut BackgroundColor, &PlayerHudBar)>,
        Query<(&mut BackgroundColor, &UnderwaterOverlay)>,
    )>,
    mut text_sets: ParamSet<(
        Query<(&mut Text, &PlayerHudText)>,
        Query<&mut Text, With<WaveText>>,
        Query<&mut Text, With<EnemyCountText>>,
    )>,
) {
    for (
        index,
        health,
        stats,
        jetpack,
        climb,
        weapons,
        special,
        armor,
        sabre,
        (traversal, board_boost, progression, tricks, stunt_run, rooftop_trial, water),
    ) in player_q.iter()
    {
        for (mut node, mut color, bar) in hud_visuals
            .p0()
            .iter_mut()
            .filter(|(_, _, bar)| bar.player_index == index.0)
        {
            let percent = match bar.kind {
                PlayerHudBarKind::Health => health.current / health.max,
                PlayerHudBarKind::Armor => stats.armor / stats.max_armor,
                PlayerHudBarKind::Stamina => stats.stamina / stats.max_stamina,
                PlayerHudBarKind::Jetpack => jetpack.fuel / jetpack.max_fuel,
                PlayerHudBarKind::Climb => climb.energy / climb.max_energy,
                PlayerHudBarKind::Air => water.breath / water.max_breath,
            };
            node.width = Val::Percent((percent * 100.0).clamp(0.0, 100.0));
            if bar.kind == PlayerHudBarKind::Air {
                color.0 = air_meter_color(percent);
            }
        }
        for (mut visibility, row) in row_q
            .iter_mut()
            .filter(|(_, row)| row.player_index == index.0)
        {
            if row.kind == PlayerHudBarKind::Air {
                *visibility = if water.swimming || water.breath < water.max_breath - 0.01 {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }
        for (mut color, _overlay) in hud_visuals
            .p1()
            .iter_mut()
            .filter(|(_, overlay)| overlay.player_index == index.0)
        {
            let alpha = if water.submerged { 0.085 } else { 0.0 };
            color.0 = Color::srgba(0.0, 0.42, 0.58, alpha);
        }

        let weapon = weapons.active();
        let sabre_str = if sabre.active { " [SABRE]" } else { "" };
        let missile_lock = missile_q
            .iter()
            .filter(|missile| missile.owner_player == index.0)
            .any(|missile| missile.target.is_some());
        let missile_active = missile_q
            .iter()
            .any(|missile| missile.owner_player == index.0);
        let lock_label = if missile_lock {
            " LOCK"
        } else if missile_active {
            " SEEK"
        } else {
            ""
        };
        for (mut text, marker) in text_sets
            .p0()
            .iter_mut()
            .filter(|(_, marker)| marker.player_index == index.0)
        {
            *text = Text::new(match marker.kind {
                PlayerHudTextKind::Header => format!("P{}", index.0 + 1),
                PlayerHudTextKind::Credits => format!("¢ {}", stats.credits),
                PlayerHudTextKind::Level => format!(
                    "LVL {}  XP {}/{}",
                    stats.level,
                    stats.experience,
                    stats.xp_for_next_level()
                ),
                PlayerHudTextKind::Element => {
                    format!("Element: {}", armor.active_element.display_name())
                }
                PlayerHudTextKind::WeaponName => {
                    let selected_name = special
                        .selected()
                        .map(|selected| selected.name)
                        .unwrap_or_else(|| weapon.weapon_type.display_name());
                    format!("{}{}", selected_name, sabre_str)
                }
                // Charge is the thing a player is actively waiting on, so it
                // takes the ammo line's space while winding up (ammo is
                // unlimited and therefore never urgent).
                PlayerHudTextKind::Ammo => charge_status_text(weapon),
                PlayerHudTextKind::SpecialAmmo => format!("SPECIALS ∞{}", lock_label),
                PlayerHudTextKind::TraversalStatus => {
                    traversal_status_text(traversal, board_boost, tricks, stunt_run, rooftop_trial)
                }
                PlayerHudTextKind::SabreStatus => sabre_status_text(
                    sabre,
                    &progression.upgrades,
                    &crate::combat::blades::blade_for_id(
                        progression.shop.equipped_weapon.as_deref(),
                    ),
                ),
            });
        }
    }

    if let Ok(mut t) = text_sets.p1().single_mut() {
        *t = Text::new(format!("Ch.{:02}  Rift {}", current.id.0, wave.wave_number));
    }
    if let Ok(mut t) = text_sets.p2().single_mut() {
        *t = Text::new(format!("Enemies: {}", wave.enemy_count));
    }
}

fn air_meter_color(percent: f32) -> Color {
    if percent <= 0.25 {
        Color::srgb(1.0, 0.28, 0.12)
    } else if percent <= 0.50 {
        Color::srgb(1.0, 0.72, 0.12)
    } else {
        Color::srgb(0.05, 0.90, 0.95)
    }
}

/// Live trick/combo readout for the board HUD line: the trick being held (or
/// the last one landed) plus the running combo total and multiplier, in the
/// arcade-skate idiom. `None` when no combo is in progress.
fn live_combo_text(tricks: Option<&TrickState>, run: Option<&StuntRunState>) -> Option<String> {
    let run = run?;
    let tricks = tricks?;
    if tricks.bailed {
        return Some("BOARD: BAIL!".into());
    }
    let held = tricks.active.as_ref().map(|a| a.name.as_str());
    let name = held.or(tricks.last_trick.as_deref());
    if !run.active && name.is_none() {
        return None;
    }
    let pending = run.pending_score.round() as u64;
    // A live revert is the thing the rider is racing against, so it leads.
    let prefix = if tricks.active.is_none() && tricks.can_revert() {
        "REVERT "
    } else {
        ""
    };
    match name {
        Some(name) if run.multiplier > 1.0 || pending > 0 => Some(format!(
            "BOARD: {prefix}{}  {}  x{:.2}",
            name.to_uppercase(),
            pending,
            run.multiplier
        )),
        Some(name) => Some(format!("BOARD: {}", name.to_uppercase())),
        None if pending > 0 => Some(format!("BOARD: COMBO {}  x{:.2}", pending, run.multiplier)),
        None => None,
    }
}

fn traversal_status_text(
    traversal: &TraversalModeState,
    boost: &BoardBoostState,
    tricks: Option<&TrickState>,
    run: Option<&StuntRunState>,
    rooftop_trial: Option<&RooftopTrialProgress>,
) -> String {
    if let Some(trial) = rooftop_trial.filter(|trial| trial.active_course.is_some()) {
        return format!(
            "ROOFTOP: {:.1}s  CP {}/{}",
            trial.elapsed,
            trial.next_checkpoint.saturating_add(1),
            trial.checkpoint_count
        );
    }
    if traversal.active != TraversalMode::Hoverboard {
        return format!("TRAVERSAL: {}", traversal.active.label().to_uppercase());
    }
    // A live combo readout outranks boost state — it is the thing the rider
    // is actively managing, and it disappears the moment the run banks.
    if let Some(text) = live_combo_text(tricks, run) {
        return text;
    }
    if boost.timer > 0.0 {
        return "BOARD: OVERDRIVE".into();
    }
    if boost.manual_cooldown <= 0.0 {
        return "BOARD: B/EAST BOOST READY".into();
    }
    let total = traversal.hoverboard_manual_boost_duration + 0.28;
    let ready = (1.0 - boost.manual_cooldown / total).clamp(0.0, 1.0);
    format!("BOARD: BOOST {:02}%", (ready * 100.0).round() as u32)
}

/// Charge readout: a filling bar while winding up, and a clear "READY" once
/// the blast can be released. Without this the only feedback for a mechanic
/// that takes over a second is the muzzle sparks.
fn charge_status_text(weapon: &crate::components::weapon::Weapon) -> String {
    let charge = weapon.charge_progress;
    if charge <= 0.02 {
        return "∞ AMMO".to_string();
    }
    if charge >= 1.0 {
        return "◆◆◆◆◆◆ CHARGED — RELEASE".to_string();
    }
    const SEGMENTS: usize = 6;
    let filled = ((charge * SEGMENTS as f32).round() as usize).min(SEGMENTS);
    format!(
        "{}{} CHARGING {:.0}%",
        "◆".repeat(filled),
        "◇".repeat(SEGMENTS - filled),
        charge * 100.0
    )
}

fn sabre_status_text(
    sabre: &BeamSabre,
    upgrades: &UpgradeLedger,
    blade: &crate::combat::blades::BladeProfile,
) -> String {
    if sabre.technique_timer > 0.0 {
        return format!("SABER: {}", sabre.technique.label());
    }
    let wave_tier = (upgrades.sabre_wave_upgrade_tier() + sabre.level.saturating_sub(1)).min(6) + 1;
    let mut techniques = vec![format!("WAVE {wave_tier}")];
    if upgrades.sabre_spin_unlocked() {
        techniques.push("SPIN".into());
    }
    if upgrades.sabre_dash_unlocked() {
        techniques.push("DASH".into());
    }
    if upgrades.sabre_pound_unlocked() {
        techniques.push("POUND".into());
    }
    let gem_count = ["solar_fire_gem", "storm_gem", "frost_gem", "void_gem"]
        .into_iter()
        .filter(|id| upgrades.has_relic(id))
        .count();
    let legendary = if upgrades.has_relic("legendary_starheart_gem") {
        " + STARHEART"
    } else {
        ""
    };
    // Surface the blade's special behaviour: without it a player has no way
    // to tell a lifesteal hilt from a wave hilt in the middle of a fight.
    let trait_note = match blade.trait_ {
        crate::combat::blades::BladeTrait::None => String::new(),
        other => format!(" • {}", other.label()),
    };
    format!(
        "{}: {}{trait_note} • GEMS {gem_count}/4{legendary}",
        blade.name.to_uppercase(),
        techniques.join("/")
    )
}

fn puzzle_objective_hud_system(
    encounter_q: Query<&PuzzleRelicEncounter>,
    mut objective_title_q: Query<
        &mut Text,
        (
            With<ObjectiveTitleText>,
            Without<ObjectiveStatusText>,
            Without<WaveText>,
            Without<EnemyCountText>,
        ),
    >,
    mut objective_status_q: Query<
        &mut Text,
        (
            With<ObjectiveStatusText>,
            Without<ObjectiveTitleText>,
            Without<WaveText>,
            Without<EnemyCountText>,
        ),
    >,
) {
    let objective = encounter_q.iter().next();
    if let Ok(mut t) = objective_title_q.single_mut() {
        *t = Text::new(match objective {
            Some(encounter) => format!("OBJECTIVE: Recover {}", encounter.label),
            None => String::new(),
        });
    }
    if let Ok(mut t) = objective_status_q.single_mut() {
        *t = Text::new(match objective {
            Some(encounter) => puzzle_status_text(encounter),
            None => String::new(),
        });
    }
}

fn puzzle_status_text(encounter: &PuzzleRelicEncounter) -> String {
    match encounter.archetype {
        PuzzleArchetype::OrderedSwitches => format!(
            "{} Progress: {}/{} pylons aligned. {}",
            encounter.scientist, encounter.active_nodes, encounter.total_nodes, encounter.hint
        ),
        PuzzleArchetype::TimedCrystalChain { .. } => format!(
            "{} Progress: {}/{} crystals lit. {:.1}s remaining. {}",
            encounter.scientist,
            encounter.active_nodes,
            encounter.total_nodes,
            encounter.timer_remaining.max(0.0),
            encounter.hint
        ),
        PuzzleArchetype::CoOpFloorPlates {
            hold_secs,
            required_players,
        } => format!(
            "{} Plates: {}/{} active. Hold {:.1}/{:.1}s with up to {} players. {}",
            encounter.scientist,
            encounter.active_nodes,
            encounter.total_nodes,
            encounter.hold_progress,
            hold_secs,
            required_players,
            encounter.hint
        ),
        PuzzleArchetype::BeamRouting => format!(
            "{} Route: {}/{} relays energized. {}",
            encounter.scientist, encounter.active_nodes, encounter.total_nodes, encounter.hint
        ),
    }
}

// ── Message Timer ─────────────────────────────────────────────────────────────
fn message_timer_system(
    time: Res<Time>,
    mut msg: ResMut<UiMessage>,
    mut text_q: Query<&mut Text, With<MessageText>>,
) {
    if msg.timer > 0.0 {
        msg.timer -= time.delta_secs();
        if msg.timer <= 0.0 {
            msg.text.clear();
            if let Ok(mut t) = text_q.single_mut() {
                *t = Text::new("");
            }
        }
    }
}

fn ui_message_listener(
    mut ev: MessageReader<UiMessageEvent>,
    mut msg: ResMut<UiMessage>,
    mut text_q: Query<&mut Text, With<MessageText>>,
) {
    for e in ev.read() {
        msg.text = e.text.clone();
        msg.timer = e.duration;
        if let Ok(mut t) = text_q.single_mut() {
            *t = Text::new(e.text.clone());
        }
    }
}

// ── Player Guidance ──────────────────────────────────────────────────────────
#[derive(Debug)]
struct GuidanceCandidate {
    score: f32,
    title: String,
    body: String,
    action: String,
}

fn player_guidance_system(
    mut guidance: ResMut<PlayerGuidance>,
    discussion: Res<DiscussionState>,
    dungeon: Res<DungeonCrawlState>,
    mission_state: Res<CustomMissionState>,
    player_q: Query<(&PlayerIndex, &Transform), With<Player>>,
    npc_q: Query<(&Transform, &DiscussionNpc)>,
    gate_q: Query<&DungeonCrawlGate>,
    exit_q: Query<&DungeonExitPortal>,
    boat_q: Query<(&Transform, &BoatVehicle)>,
    disc_q: Query<(&Transform, &Discoverable)>,
    loot_q: Query<(&Transform, &WorldLoot)>,
    spy_q: Query<(&Transform, &CitySpyDrone, &Health)>,
    mut panel_q: Query<&mut Node, With<GuidancePanelRoot>>,
    mut text_sets: ParamSet<(
        Query<&mut Text, With<GuidanceTitleText>>,
        Query<&mut Text, With<GuidanceBodyText>>,
        Query<&mut Text, With<GuidanceActionText>>,
    )>,
) {
    let mut best: Option<GuidanceCandidate> = None;

    if !discussion.active {
        if let Some(active_gate_id) = dungeon.gate_id.filter(|_| dungeon.active) {
            for portal in exit_q
                .iter()
                .filter(|portal| portal.gate_id == active_gate_id)
            {
                if let Some((player_index, distance)) =
                    nearest_player_to(&player_q, portal.position)
                {
                    if distance <= portal.interact_radius + 4.0 {
                        offer_guidance(
                            &mut best,
                            0.0,
                            distance,
                            "Return to the Mountain",
                            format!("P{} can return the complete party.", player_index + 1),
                            "E / D-pad Down: Exit dungeon",
                        );
                    }
                }
            }
        }

        for (npc_transform, npc) in npc_q.iter() {
            if let Some((player_index, distance)) =
                nearest_player_to(&player_q, npc_transform.translation)
            {
                if distance <= npc.interact_radius + 3.0 {
                    offer_guidance(
                        &mut best,
                        0.0,
                        distance,
                        format!("Talk to {}", npc.display_name),
                        format!("P{} is near a {}.", player_index + 1, npc.role),
                        "E / D-pad Down: Talk",
                    );
                }
            }
        }

        if !dungeon.active {
            for gate in gate_q.iter() {
                if let Some((player_index, distance)) = nearest_player_to(&player_q, gate.entry) {
                    if distance <= gate.interact_radius + 4.0 {
                        let verb = dungeon_gate_guidance_verb(gate.opened);
                        offer_guidance(
                            &mut best,
                            -1.0,
                            distance,
                            format!("{verb} {}", gate.label),
                            format!(
                                "P{} can gather the party for a top-down dungeon.",
                                player_index + 1
                            ),
                            format!("E / D-pad Down: {verb} dungeon"),
                        );
                    }
                }
            }
        }

        for (boat_transform, boat) in boat_q.iter() {
            if let Some((player_index, distance)) =
                nearest_player_to(&player_q, boat_transform.translation)
            {
                if distance <= boat.embark_radius + 5.0 {
                    offer_guidance(
                        &mut best,
                        2.0,
                        distance,
                        "Board Boat",
                        format!("P{} can pilot or ride with the party.", player_index + 1),
                        "J / D-pad Up: Board",
                    );
                }
            }
        }

        for (disc_transform, discoverable) in disc_q.iter() {
            if let Some((player_index, distance)) =
                nearest_player_to(&player_q, disc_transform.translation)
            {
                if distance <= 6.0 {
                    let (title, action) = discoverable_guidance(&discoverable.kind);
                    offer_guidance(
                        &mut best,
                        3.0,
                        distance,
                        title,
                        format!("P{} can collect {}.", player_index + 1, discoverable.label),
                        action,
                    );
                }
            }
        }

        for (loot_transform, loot) in loot_q.iter() {
            if let Some((player_index, distance)) =
                nearest_player_to(&player_q, loot_transform.translation)
            {
                if distance <= loot.pickup_radius + 3.0 {
                    offer_guidance(
                        &mut best,
                        4.0,
                        distance,
                        "Collect Loot",
                        format!(
                            "P{} can pick up {}x {}.",
                            player_index + 1,
                            loot.quantity,
                            loot.item_id.replace('_', " ")
                        ),
                        "Walk over pickup: Collect",
                    );
                }
            }
        }

        for (spy_transform, spy, health) in spy_q.iter() {
            if !health.is_alive() {
                continue;
            }
            if let Some((player_index, distance)) =
                nearest_player_to(&player_q, spy_transform.translation)
            {
                if distance <= 180.0 {
                    offer_guidance(
                        &mut best,
                        7.0,
                        distance,
                        "City Watch: Spy Drone",
                        format!(
                            "P{} spotted {} above the peaceful city.",
                            player_index + 1,
                            spy.label
                        ),
                        "Aim + Fire: Shoot it down, then recover data",
                    );
                }
            }
        }
    }

    if best.is_none() {
        if let Some(mission) = active_custom_mission(&mission_state) {
            best = Some(GuidanceCandidate {
                score: f32::MAX,
                title: format!("MISSION: {}", mission.title),
                body: mission.briefing.clone(),
                action: mission.objective_text(),
            });
        }
    }

    if let Some(candidate) = best {
        guidance.set(candidate.title, candidate.body, candidate.action);
    } else {
        guidance.clear();
    }

    if let Ok(mut panel) = panel_q.single_mut() {
        panel.display = if guidance.visible {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut text) = text_sets.p0().single_mut() {
        *text = Text::new(guidance.title.clone());
    }
    if let Ok(mut text) = text_sets.p1().single_mut() {
        *text = Text::new(guidance.body.clone());
    }
    if let Ok(mut text) = text_sets.p2().single_mut() {
        *text = Text::new(guidance.action.clone());
    }
}

fn nearest_player_to(
    player_q: &Query<(&PlayerIndex, &Transform), With<Player>>,
    position: Vec3,
) -> Option<(u8, f32)> {
    player_q
        .iter()
        .map(|(index, transform)| (index.0, transform.translation.distance(position)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

fn dungeon_gate_guidance_verb(opened: bool) -> &'static str {
    if opened {
        "Enter"
    } else {
        "Open"
    }
}

fn offer_guidance(
    best: &mut Option<GuidanceCandidate>,
    priority: f32,
    distance: f32,
    title: impl Into<String>,
    body: impl Into<String>,
    action: impl Into<String>,
) {
    let score = priority * 10_000.0 + distance;
    if best
        .as_ref()
        .map(|candidate| score < candidate.score)
        .unwrap_or(true)
    {
        *best = Some(GuidanceCandidate {
            score,
            title: title.into(),
            body: body.into(),
            action: action.into(),
        });
    }
}

fn discoverable_guidance(kind: &DiscoverableKind) -> (&'static str, &'static str) {
    match kind {
        DiscoverableKind::SpyData { .. } => ("Recover Spy Data", "Walk into data core: Recover"),
        DiscoverableKind::RobotPetRescue { .. } => ("Rescue Robot Pet", "Walk into beacon: Rescue"),
        DiscoverableKind::TechCache { .. } => ("Open Tech Cache", "Walk into cache: Collect"),
        DiscoverableKind::CompanionRecruit(_) => ("Rescue Friend", "Walk into beacon: Rescue"),
        DiscoverableKind::SecretCave { .. } => ("Chart Secret Cave", "Walk into marker: Chart"),
        DiscoverableKind::ScientistRelic { .. } | DiscoverableKind::RelicFragment { .. } => {
            ("Recover Relic", "Walk into relic: Recover")
        }
        DiscoverableKind::BeamSabreUnlock => ("Unlock Star Sabre", "Walk into core: Unlock"),
        DiscoverableKind::HiddenReward { .. } => ("Open Reward Cache", "Walk into cache: Collect"),
        DiscoverableKind::Blueprint(_) => ("Collect Blueprint", "Walk into blueprint: Collect"),
        DiscoverableKind::WeaponMod(_) => ("Collect Weapon Mod", "Walk into mod: Collect"),
        DiscoverableKind::ArmorMod(_) => ("Collect Armor Mod", "Walk into mod: Collect"),
        DiscoverableKind::LoreFragment(_) => ("Read Lore", "Walk into fragment: Read"),
    }
}

// ── Discussion Panel ─────────────────────────────────────────────────────────
fn discussion_panel_system(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    asset_server: Res<AssetServer>,
    settings: Res<GameSettings>,
    mut commands: Commands,
    mut discussion: ResMut<DiscussionState>,
    mut input_capture: ResMut<UiGameplayCapture>,
    mut last_voice_token: Local<u64>,
    mut panel_q: Query<&mut Node, With<DiscussionPanelRoot>>,
    mut text_sets: ParamSet<(
        Query<&mut Text, With<DiscussionTitleText>>,
        Query<&mut Text, With<DiscussionSpeakerText>>,
        Query<&mut Text, With<DiscussionBodyText>>,
        Query<&mut Text, With<DiscussionHintText>>,
    )>,
    player_input_q: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
) {
    if discussion.input_cooldown > 0.0 {
        discussion.input_cooldown = (discussion.input_cooldown - time.delta_secs()).max(0.0);
    }

    if let Ok(mut panel) = panel_q.single_mut() {
        panel.display = if discussion.active {
            Display::Flex
        } else {
            Display::None
        };
    }

    if !discussion.active {
        return;
    }

    input_capture.owner = Some(discussion.player_index);

    let Some(script) = discussion.script() else {
        discussion.close();
        return;
    };
    let Some(line) = discussion.line() else {
        discussion.close();
        return;
    };

    if *last_voice_token != discussion.voice_token {
        if let Some(path) = line.voice_mp3 {
            commands.spawn((
                AudioPlayer::new(asset_server.load(path)),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.86)),
            ));
        }
        *last_voice_token = discussion.voice_token;
    }

    if let Ok(mut text) = text_sets.p0().single_mut() {
        *text = Text::new(format!("{} - {}", script.title, script.role));
    }
    if let Ok(mut text) = text_sets.p1().single_mut() {
        *text = Text::new(format!(
            "{}  P{}",
            line.speaker,
            discussion.player_index + 1
        ));
    }
    if let Ok(mut text) = text_sets.p2().single_mut() {
        let show_text = settings.subtitles_enabled || line.voice_mp3.is_none();
        *text = Text::new(if show_text { line.text } else { "" });
    }
    if let Ok(mut text) = text_sets.p3().single_mut() {
        *text = Text::new(format!(
            "A / Interact: next   B: close   Line {}/{}   Voice: {}",
            discussion.line_index + 1,
            script.lines.len(),
            line.voice_mp3.unwrap_or("none")
        ));
    }

    let owner_input = player_input_q
        .iter()
        .find_map(|(idx, input)| (idx.0 == discussion.player_index).then_some(input));
    let owner_interact = owner_input.is_some_and(|input| input.interact || input.ui_confirm);
    let owner_cancel = owner_input.is_some_and(|input| input.ui_cancel);
    let keyboard_next = keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::NumpadEnter)
        || keyboard.just_pressed(KeyCode::Space)
        || keyboard.just_pressed(KeyCode::KeyE);

    if discussion.input_cooldown <= 0.0 && owner_cancel {
        discussion.close();
        input_capture.owner = None;
    } else if discussion.input_cooldown <= 0.0 && (owner_interact || keyboard_next) {
        discussion.advance();
        if !discussion.active {
            input_capture.owner = None;
        }
    }
}

// ── Damage Vignette ───────────────────────────────────────────────────────────
fn damage_vignette_system(
    time: Res<Time>,
    mut damage_ev: MessageReader<PlayerDamagedEvent>,
    mut vignette_q: Query<(&mut BackgroundColor, &mut DamageVignette)>,
) {
    for ev in damage_ev.read() {
        for (_, mut v) in vignette_q.iter_mut() {
            if ev
                .player_index
                .map(|player_index| player_index == v.player_index)
                .unwrap_or(true)
            {
                v.alpha = 0.45;
            }
        }
    }
    for (mut bg, mut v) in vignette_q.iter_mut() {
        v.alpha = (v.alpha - time.delta_secs() * 2.5).max(0.0);
        *bg = BackgroundColor(Color::srgba(0.7, 0.0, 0.0, v.alpha));
    }
}

// ── Boss Alert ────────────────────────────────────────────────────────────────
fn boss_alert_system(
    time: Res<Time>,
    mut boss_ev: MessageReader<BossSpawnedEvent>,
    mut alert_q: Query<(&mut Text, &mut TextColor), With<BossAlertText>>,
) {
    for ev in boss_ev.read() {
        if let Ok((mut t, _)) = alert_q.single_mut() {
            *t = Text::new(format!("!! BOSS WAVE {} !!", ev.wave));
        }
    }
    // Pulse the alert text color
    let pulse = (time.elapsed_secs() * 4.0).sin() * 0.5 + 0.5;
    for (_, mut color) in alert_q.iter_mut() {
        *color = TextColor(Color::srgb(1.0, pulse * 0.3, 0.0));
    }
}

// ── Crafting Panel ────────────────────────────────────────────────────────────
const fn weapon_socket_label(socket: WeaponSocket) -> &'static str {
    match socket {
        WeaponSocket::Pistol => "Starlight Popper",
        WeaponSocket::Rifle => "Comet Stream",
        WeaponSocket::Shotgun => "Sparkle Fan",
        WeaponSocket::Rocket => "Nova Orb",
        WeaponSocket::Laser => "Rainbow Ray",
        WeaponSocket::Grenade => "Star Bubble Bombs",
        WeaponSocket::TrackingMissile => "Homing Star",
    }
}

const fn jewel_tier_label(tier: JewelTier) -> &'static str {
    match tier {
        JewelTier::Rough => "Rough +15%",
        JewelTier::Cut => "Cut +30%",
        JewelTier::Flawless => "Flawless +50%",
    }
}

fn best_available_jewel(inventory: &Inventory) -> Option<JewelTier> {
    [JewelTier::Flawless, JewelTier::Cut, JewelTier::Rough]
        .into_iter()
        .find(|tier| inventory.has(tier.item_id(), 1))
}

const fn garden_stage_label(stage: GrowthStage) -> &'static str {
    match stage {
        GrowthStage::Empty => "Empty",
        GrowthStage::Seeded => "Seeded",
        GrowthStage::Sprout => "Sprout",
        GrowthStage::Grown => "Grown",
    }
}

fn crafting_panel_system(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut panel_state: ResMut<CraftingPanelState>,
    mut input_capture: ResMut<UiGameplayCapture>,
    mut panel_q: Query<&mut Node, With<CraftingPanelRoot>>,
    mut text_q: Query<&mut Text, With<CraftingPanelText>>,
    player_input_q: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
    mut player_q: Query<(&PlayerIndex, &mut Inventory, &mut PlayerStats), With<Player>>,
    mut queue: ResMut<CraftingQueue>,
    mut heavy_water: ResMut<HeavyWaterProgress>,
    bio_clock: Option<Res<HeavyBioClock>>,
    mut inventory_changed: MessageWriter<InventoryChangedEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let mut opened_this_frame = false;
    for (idx, input) in player_input_q.iter() {
        if input.crafting {
            let closing_same_owner = panel_state.visible && panel_state.owner == Some(idx.0);
            panel_state.visible = !closing_same_owner;
            panel_state.owner = panel_state.visible.then_some(idx.0);
            panel_state.repeat_direction = 0;
            panel_state.repeat_timer = 0.0;
            opened_this_frame = panel_state.visible;
        }
    }
    input_capture.owner = panel_state.visible.then_some(panel_state.owner).flatten();

    if let Ok(mut node) = panel_q.single_mut() {
        node.display = if panel_state.visible {
            Display::Flex
        } else {
            Display::None
        };
    }

    if !panel_state.visible {
        input_capture.owner = None;
        return;
    }

    let Some(owner) = panel_state.owner else {
        panel_state.visible = false;
        input_capture.owner = None;
        return;
    };

    let owner_input = player_input_q
        .iter()
        .find_map(|(idx, input)| (idx.0 == owner).then_some(input));

    if owner_input.is_some_and(|input| input.ui_cancel) {
        panel_state.visible = false;
        panel_state.owner = None;
        input_capture.owner = None;
        return;
    }

    let Some((_, mut inventory, mut stats)) =
        player_q.iter_mut().find(|(idx, _, _)| idx.0 == owner)
    else {
        panel_state.visible = false;
        panel_state.owner = None;
        input_capture.owner = None;
        return;
    };
    let selection_slot = usize::from(owner).min(panel_state.recipe_index.len() - 1);
    let tab_direction = if opened_this_frame {
        0
    } else {
        owner_input.map_or(0, |input| {
            if input.ui_left {
                -1
            } else if input.ui_right {
                1
            } else {
                0
            }
        })
    };
    if tab_direction != 0 {
        panel_state.tab[selection_slot] = panel_state.tab[selection_slot].offset(tab_direction);
        panel_state.repeat_direction = 0;
        panel_state.repeat_timer = 0.0;
    }

    if panel_state.tab[selection_slot] == WorkshopTab::Jewels {
        let direction = if opened_this_frame {
            0
        } else {
            owner_input
                .map(|input| {
                    crafting_navigation_direction(&mut panel_state, input, time.delta_secs())
                })
                .unwrap_or(0)
        };
        if direction != 0 {
            panel_state.jewel_index[selection_slot] = cycle_index(
                panel_state.jewel_index[selection_slot],
                WeaponSocket::ALL.len(),
                direction,
            );
        }
        let selected_socket = WeaponSocket::ALL[panel_state.jewel_index[selection_slot]];
        let owner_id = OwnerId(owner);
        let account = heavy_water.economy.account(owner_id).ok();
        let mut display = String::from(
            "JEWELS  [Left/Right: tabs]\nPower Jewels apply once to their matching weapon.\n\n",
        );
        for (index, socket) in WeaponSocket::ALL.into_iter().enumerate() {
            let mounted = account
                .and_then(|account| account.jewel_mounts.get(&socket).copied())
                .map_or("Empty", jewel_tier_label);
            display.push_str(&format!(
                "{} {:<18} {}\n",
                if index == panel_state.jewel_index[selection_slot] {
                    "▶"
                } else {
                    " "
                },
                weapon_socket_label(socket),
                mounted,
            ));
        }
        display.push_str(&format!(
            "\nBag: Rough {}  Cut {}  Flawless {}\nA/Enter: {}",
            inventory.count(JewelTier::Rough.item_id()),
            inventory.count(JewelTier::Cut.item_id()),
            inventory.count(JewelTier::Flawless.item_id()),
            if account.is_some_and(|account| account.jewel_mounts.contains_key(&selected_socket)) {
                "Unmount selected jewel"
            } else {
                "Mount best available jewel"
            }
        ));
        if let Ok(mut text) = text_q.single_mut() {
            *text = Text::new(format!("P{} Workshop\n\n{}", owner + 1, display));
        }

        let confirm = !opened_this_frame && owner_input.is_some_and(|input| input.ui_confirm);
        if confirm {
            let mounted = heavy_water
                .economy
                .account(owner_id)
                .ok()
                .and_then(|account| account.jewel_mounts.get(&selected_socket).copied());
            let Some(transaction_id) = heavy_water.allocate_economy_transaction_id() else {
                msg_ev.write(UiMessageEvent {
                    text: "Jewel transaction IDs exhausted".to_string(),
                    duration: 2.0,
                });
                return;
            };
            let result = if mounted.is_some() {
                unmount_jewel_to_canonical_atomic(
                    &mut heavy_water.economy,
                    &mut inventory,
                    &stats,
                    owner_id,
                    transaction_id,
                    selected_socket,
                )
                .map(|tier| format!("Unmounted {}", jewel_tier_label(tier)))
            } else if let Some(tier) = best_available_jewel(&inventory) {
                mount_jewel_from_canonical_atomic(
                    &mut heavy_water.economy,
                    &mut inventory,
                    &stats,
                    owner_id,
                    transaction_id,
                    selected_socket,
                    tier,
                )
                .map(|()| {
                    format!(
                        "Mounted {} on {}",
                        jewel_tier_label(tier),
                        weapon_socket_label(selected_socket)
                    )
                })
            } else {
                msg_ev.write(UiMessageEvent {
                    text: format!("P{} has no Power Jewel to mount", owner + 1),
                    duration: 2.0,
                });
                return;
            };
            match result {
                Ok(message) => {
                    inventory_changed.write(InventoryChangedEvent);
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} {message}", owner + 1),
                        duration: 2.2,
                    });
                }
                Err(error) => {
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} Jewel change failed: {error}", owner + 1),
                        duration: 2.5,
                    });
                }
            }
        }
        return;
    }

    if panel_state.tab[selection_slot] == WorkshopTab::BioGarden {
        let direction = if opened_this_frame {
            0
        } else {
            owner_input
                .map(|input| {
                    crafting_navigation_direction(&mut panel_state, input, time.delta_secs())
                })
                .unwrap_or(0)
        };
        if direction != 0 {
            panel_state.garden_index[selection_slot] = cycle_index(
                panel_state.garden_index[selection_slot],
                GARDEN_PLOT_COUNT,
                direction,
            );
        }

        let player_key = local_player_key(owner);
        let now_ms = bio_clock.as_deref().map_or(0, |clock| clock.game_time_ms);
        let profile = heavy_water.bio.player_mut(player_key.clone());
        profile.garden.advance_growth(now_ms);
        let selected_plot = panel_state.garden_index[selection_slot];
        let selected_stage = profile.garden.plots[selected_plot].stage;
        let (dex_caught, dex_total) = profile.dex_completion();
        let mut display = format!(
            "BIO GARDEN  [Left/Right: tabs]\nDex {dex_caught}/{dex_total}  Creatures {}/{}  Active {}\n\n",
            profile.creatures.len(),
            profile.garden_capture_cap(),
            profile.active_creature_ids.len(),
        );
        for (index, plot) in profile.garden.plots.iter().enumerate() {
            display.push_str(&format!(
                "{} Plot {:02}: {:<7}{}",
                if index == selected_plot { "▶" } else { " " },
                index + 1,
                garden_stage_label(plot.stage),
                if index % 2 == 1 { "\n" } else { "   " },
            ));
        }
        display.push_str(&format!(
            "\nBag: Seed {}  Crop {}  Feed {}\nA/Enter: {}",
            inventory.count("bio_seed"),
            inventory.count("bio_crop"),
            inventory.count("animaton_feed"),
            match selected_stage {
                GrowthStage::Empty => "Plant one Bio Seed",
                GrowthStage::Grown => "Harvest this plot",
                GrowthStage::Seeded | GrowthStage::Sprout => "Growing (check back soon)",
            }
        ));
        if let Ok(mut text) = text_q.single_mut() {
            *text = Text::new(format!("P{} Workshop\n\n{}", owner + 1, display));
        }

        let confirm = !opened_this_frame && owner_input.is_some_and(|input| input.ui_confirm);
        if confirm {
            let result = match selected_stage {
                GrowthStage::Empty => {
                    try_plant_bio_seed(profile, &mut inventory, selected_plot, now_ms)
                        .map(|_| format!("planted Bio Seed in plot {}", selected_plot + 1))
                }
                GrowthStage::Grown => try_harvest_bio_plot(
                    &player_key,
                    profile,
                    &mut inventory,
                    selected_plot,
                    now_ms,
                )
                .map(|applied| {
                    format!(
                        "harvested plot {}: +{} crop, +{} feed, +{} seed",
                        selected_plot + 1,
                        applied.outcome.bio_crops,
                        applied.outcome.animaton_feed,
                        applied.outcome.bio_seeds,
                    )
                }),
                GrowthStage::Seeded | GrowthStage::Sprout => {
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} plot {} is still growing", owner + 1, selected_plot + 1),
                        duration: 2.0,
                    });
                    return;
                }
            };
            match result {
                Ok(message) => {
                    inventory_changed.write(InventoryChangedEvent);
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} {message}", owner + 1),
                        duration: 2.2,
                    });
                }
                Err(error) => {
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} Bio Garden action failed: {error:?}", owner + 1),
                        duration: 2.5,
                    });
                }
            }
        }
        return;
    }

    if panel_state.tab[selection_slot] == WorkshopTab::Market {
        let vendor_count = heavy_water.economy.vendors.len();
        if vendor_count == 0 {
            if let Ok(mut text) = text_q.single_mut() {
                *text = Text::new(format!(
                    "P{} Market Uplink\n\nNo material vendors are registered.",
                    owner + 1
                ));
            }
            return;
        }

        let vendor_direction = if opened_this_frame {
            0
        } else {
            owner_input.map_or(0, |input| {
                if input.ui_page_prev {
                    -1
                } else if input.ui_page_next {
                    1
                } else {
                    0
                }
            })
        };
        if vendor_direction != 0 {
            panel_state.vendor_index[selection_slot] = cycle_index(
                panel_state.vendor_index[selection_slot],
                vendor_count,
                vendor_direction,
            );
            panel_state.vendor_item_index[selection_slot] = 0;
        }
        panel_state.vendor_index[selection_slot] =
            panel_state.vendor_index[selection_slot].min(vendor_count - 1);
        let vendor = heavy_water
            .economy
            .vendors
            .values()
            .nth(panel_state.vendor_index[selection_slot])
            .expect("bounded vendor index")
            .clone();
        let item_count = vendor.items.len();
        if item_count == 0 {
            if let Ok(mut text) = text_q.single_mut() {
                *text = Text::new(format!(
                    "P{} Market Uplink\n\n{} has no catalog lines.",
                    owner + 1,
                    vendor.name
                ));
            }
            return;
        }

        let direction = if opened_this_frame {
            0
        } else {
            owner_input
                .map(|input| {
                    crafting_navigation_direction(&mut panel_state, input, time.delta_secs())
                })
                .unwrap_or(0)
        };
        if direction != 0 {
            panel_state.vendor_item_index[selection_slot] = cycle_index(
                panel_state.vendor_item_index[selection_slot],
                item_count,
                direction,
            );
        }
        panel_state.vendor_item_index[selection_slot] =
            panel_state.vendor_item_index[selection_slot].min(item_count - 1);
        let selected_item_index = panel_state.vendor_item_index[selection_slot];
        let selected_line = vendor.items[selected_item_index].clone();

        let mut display = format!(
            "MARKET UPLINK  [Left/Right: tabs]\n{}  ({}/{})  Credits {}\n[Q/E or shoulders: vendor]\n\n",
            vendor.name,
            panel_state.vendor_index[selection_slot] + 1,
            vendor_count,
            stats.credits,
        );
        for (index, line) in vendor.items.iter().enumerate() {
            let item_name = canonical_item_definition(&line.item_id)
                .map_or(line.item_id.as_str(), |definition| definition.name);
            display.push_str(&format!(
                "{} {:<20} {:>4}c  stock {:>2}  bag {}\n",
                if index == selected_item_index {
                    "▶"
                } else {
                    " "
                },
                item_name,
                line.buy_price,
                line.stock,
                inventory.count(&line.item_id),
            ));
        }
        display.push_str("\nA/Enter: buy one   Reload/West: sell one");
        if let Ok(mut text) = text_q.single_mut() {
            *text = Text::new(format!("P{} Workshop\n\n{}", owner + 1, display));
        }

        let buying = !opened_this_frame && owner_input.is_some_and(|input| input.ui_confirm);
        let selling = !opened_this_frame && owner_input.is_some_and(|input| input.ui_secondary);
        if buying || selling {
            let Some(transaction_id) = heavy_water.allocate_economy_transaction_id() else {
                msg_ev.write(UiMessageEvent {
                    text: "Market transaction IDs exhausted".to_string(),
                    duration: 2.0,
                });
                return;
            };
            let owner_id = OwnerId(owner);
            let result = if buying {
                buy_from_vendor_canonical_atomic(
                    &mut heavy_water.economy,
                    &mut inventory,
                    &mut stats,
                    owner_id,
                    transaction_id,
                    &vendor.id,
                    &selected_line.item_id,
                    1,
                )
                .map(|balance| {
                    format!(
                        "bought {} for {} credits ({} left)",
                        selected_line.item_id, selected_line.buy_price, balance
                    )
                })
            } else {
                sell_to_vendor_canonical_atomic(
                    &mut heavy_water.economy,
                    &mut inventory,
                    &mut stats,
                    owner_id,
                    transaction_id,
                    &vendor.id,
                    &selected_line.item_id,
                    1,
                )
                .map(|balance| {
                    format!(
                        "sold {} for {} credits ({} total)",
                        selected_line.item_id, selected_line.sell_price, balance
                    )
                })
            };
            match result {
                Ok(message) => {
                    inventory_changed.write(InventoryChangedEvent);
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} {message}", owner + 1),
                        duration: 2.2,
                    });
                }
                Err(error) => {
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} Market transaction failed: {error}", owner + 1),
                        duration: 2.5,
                    });
                }
            }
        }
        return;
    }

    let recipes = all_recipes();
    panel_state.recipe_index[selection_slot] =
        panel_state.recipe_index[selection_slot].min(recipes.len().saturating_sub(1));

    if !opened_this_frame {
        let direction = owner_input
            .map(|input| crafting_navigation_direction(&mut panel_state, input, time.delta_secs()))
            .unwrap_or(0);
        if direction != 0 && !recipes.is_empty() {
            panel_state.recipe_index[selection_slot] = cycle_index(
                panel_state.recipe_index[selection_slot],
                recipes.len(),
                direction,
            );
        }
    }
    let selected_index = panel_state.recipe_index[selection_slot];

    // Keep the expanded 25-recipe Heavy Water catalog readable inside the
    // split-screen-safe modal instead of drawing rows beyond its bounds.
    const VISIBLE_RECIPE_ROWS: usize = 9;
    let max_start = recipes.len().saturating_sub(VISIBLE_RECIPE_ROWS);
    let visible_start = selected_index
        .saturating_sub(VISIBLE_RECIPE_ROWS / 2)
        .min(max_start);
    let visible_end = (visible_start + VISIBLE_RECIPE_ROWS).min(recipes.len());
    let mut display = format!(
        "Recipes {}–{} / {}\n",
        visible_start + usize::from(!recipes.is_empty()),
        visible_end,
        recipes.len()
    );
    for (i, recipe) in recipes
        .iter()
        .enumerate()
        .skip(visible_start)
        .take(VISIBLE_RECIPE_ROWS)
    {
        let can_craft = stats.level >= recipe.required_level
            && recipe
                .materials
                .iter()
                .all(|m| inventory.has(m.item_id, m.quantity));
        let status = if can_craft { "READY" } else { "LOCKED" };
        let mat_list: Vec<String> = recipe
            .materials
            .iter()
            .map(|m| format!("{}×{}", m.quantity, m.item_id.replace('_', " ")))
            .collect();
        display.push_str(&format!(
            "{} [{}] {} {}  →  {}×{}\n     {}\n",
            if i == selected_index { "▶" } else { " " },
            i + 1,
            status,
            recipe.name,
            recipe.result_quantity,
            recipe.result_item.replace('_', " "),
            mat_list.join(", "),
        ));
    }

    if let Ok(mut t) = text_q.single_mut() {
        *t = Text::new(format!(
            "P{} Crafting  [Left/Right: Workshop tabs]\n\n{}",
            owner + 1,
            display
        ));
    }

    // Number keys retain direct access. The panel owner can also navigate with
    // their own D-pad/left stick and activate with their own South/A button.
    let keyboard_index = if keyboard.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        Some(3)
    } else if keyboard.just_pressed(KeyCode::Digit5) {
        Some(4)
    } else {
        None
    };
    if let Some(index) = keyboard_index.filter(|index| *index < recipes.len()) {
        panel_state.recipe_index[selection_slot] = index;
    }
    let controller_confirm =
        !opened_this_frame && owner_input.is_some_and(|input| input.ui_confirm);
    let craft_index = keyboard_index.or(controller_confirm.then_some(selected_index));

    if let Some(idx) = craft_index {
        if let Some(recipe) = recipes.get(idx) {
            match start_craft(recipe.id, owner, &mut inventory, &stats, &mut queue) {
                Ok(()) => {
                    msg_ev.write(UiMessageEvent {
                        text: format!(
                            "P{} Crafting: {} ({:.0}s)",
                            owner + 1,
                            recipe.name,
                            recipe.craft_time
                        ),
                        duration: 2.5,
                    });
                }
                Err(msg) => {
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} Can't craft: {}", owner + 1, msg),
                        duration: 2.0,
                    });
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn loadout_panel_system(
    time: Res<Time>,
    mut vehicle_state: ResMut<VehicleState>,
    mut state: ResMut<LoadoutPanelState>,
    mut crafting: ResMut<CraftingPanelState>,
    mut capture: ResMut<UiGameplayCapture>,
    mut panel_q: Query<&mut Node, (With<LoadoutPanelRoot>, Without<Button>)>,
    mut texts: ParamSet<(
        Query<&mut Text, With<LoadoutTitleText>>,
        Query<&mut Text, With<LoadoutStatusText>>,
        Query<(&LoadoutEntryLabel, &mut Text)>,
    )>,
    input_q: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
    mut player_q: Query<
        (
            &PlayerIndex,
            &mut WeaponInventory,
            &mut SpecialWeaponInventory,
            &mut ArmorSet,
            &Inventory,
            &mut QuickItemSlot,
            &mut TraversalModeState,
            &PlayerProgression,
        ),
        With<Player>,
    >,
    mut buttons: ParamSet<(
        Query<
            (
                &Interaction,
                Option<&LoadoutTabButton>,
                Option<&LoadoutEntryButton>,
                Option<&LoadoutCloseButton>,
            ),
            (With<Button>, Changed<Interaction>),
        >,
        Query<
            (
                &Interaction,
                Option<&LoadoutTabButton>,
                Option<&LoadoutEntryButton>,
                Option<&LoadoutCloseButton>,
                &mut Node,
                &mut BackgroundColor,
                &mut BorderColor,
            ),
            With<Button>,
        >,
    )>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let mut opened_this_frame = false;
    for (index, input) in input_q.iter() {
        if input.loadout_menu {
            let closing = state.visible && state.owner == Some(index.0);
            state.visible = !closing;
            state.owner = state.visible.then_some(index.0);
            state.repeat_direction = 0;
            state.repeat_timer = 0.0;
            if state.visible {
                crafting.visible = false;
                crafting.owner = None;
                opened_this_frame = true;
            }
        } else if state.visible && state.owner == Some(index.0) && input.crafting {
            state.visible = false;
            state.owner = None;
        }
    }

    let mut clicked_tab = None;
    let mut clicked_entry = None;
    let mut clicked_close = false;
    for (interaction, tab, entry, close) in buttons.p0().iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        clicked_tab = clicked_tab.or(tab.map(|button| button.0));
        clicked_entry = clicked_entry.or(entry.map(|button| button.0));
        clicked_close |= close.is_some();
    }
    if clicked_close {
        state.visible = false;
        state.owner = None;
    }

    if let Ok(mut node) = panel_q.single_mut() {
        node.display = if state.visible {
            Display::Flex
        } else {
            Display::None
        };
    }
    if !state.visible {
        if !crafting.visible {
            capture.owner = None;
        }
        return;
    }

    let Some(owner) = state.owner else {
        state.visible = false;
        capture.owner = None;
        return;
    };
    capture.owner = Some(owner);
    let owner_slot = usize::from(owner).min(3);
    let owner_input = input_q
        .iter()
        .find_map(|(index, input)| (index.0 == owner).then_some(input));

    if owner_input.is_some_and(|input| input.ui_cancel) {
        state.visible = false;
        state.owner = None;
        capture.owner = None;
        return;
    }

    if let Some(tab) = clicked_tab {
        state.tab = tab;
    } else if !opened_this_frame {
        if owner_input.is_some_and(|input| input.ui_left) {
            state.tab = state.tab.offset(-1);
        } else if owner_input.is_some_and(|input| input.ui_right) {
            state.tab = state.tab.offset(1);
        }
    }

    let Some((
        _,
        mut weapons,
        mut specials,
        mut armor,
        inventory,
        mut quick,
        mut traversal,
        progression,
    )) = player_q.iter_mut().find(|(index, ..)| index.0 == owner)
    else {
        state.visible = false;
        state.owner = None;
        capture.owner = None;
        return;
    };

    let mut entries = loadout_entry_labels(
        state.tab,
        &weapons,
        &specials,
        &armor,
        inventory,
        &quick,
        vehicle_state.persistent_traversal(owner, traversal.active),
        &progression.upgrades,
    );
    let tab_slot = state.tab.index();
    state.selection[owner_slot][tab_slot] =
        state.selection[owner_slot][tab_slot].min(entries.len().saturating_sub(1));

    if !opened_this_frame {
        let direction = owner_input
            .map(|input| loadout_navigation_direction(&mut state, input, time.delta_secs()))
            .unwrap_or(0);
        if direction != 0 && !entries.is_empty() {
            state.selection[owner_slot][tab_slot] = cycle_index(
                state.selection[owner_slot][tab_slot],
                entries.len(),
                direction,
            );
        }
    }
    if let Some(index) = clicked_entry.filter(|index| *index < entries.len()) {
        state.selection[owner_slot][tab_slot] = index;
    }

    let selected = state.selection[owner_slot][tab_slot];
    let confirm = !opened_this_frame
        && (clicked_entry.is_some() || owner_input.is_some_and(|input| input.ui_confirm));
    if confirm && !entries.is_empty() {
        let selected_name = if state.tab == LoadoutTab::Rides {
            activate_ride_entry(selected, owner, &mut traversal, &mut vehicle_state)
        } else {
            activate_loadout_entry(
                state.tab,
                selected,
                &mut weapons,
                &mut specials,
                &mut armor,
                inventory,
                &mut quick,
                &mut traversal,
            )
        };
        msg_ev.write(UiMessageEvent {
            text: if state.tab == LoadoutTab::Relics {
                format!("P{} inspected {}", owner + 1, selected_name)
            } else {
                format!("P{} equipped {}", owner + 1, selected_name)
            },
            duration: 1.5,
        });
        entries = loadout_entry_labels(
            state.tab,
            &weapons,
            &specials,
            &armor,
            inventory,
            &quick,
            vehicle_state.persistent_traversal(owner, traversal.active),
            &progression.upgrades,
        );
    }

    if let Ok(mut text) = texts.p0().single_mut() {
        *text = Text::new(format!(
            "P{} STAR LOADOUT — {}",
            owner + 1,
            state.tab.label()
        ));
    }
    if let Ok(mut text) = texts.p1().single_mut() {
        *text = Text::new(match state.tab {
            LoadoutTab::Weapons => {
                "Primary beams have unlimited energy; selection changes RT fire.".to_string()
            }
            LoadoutTab::Armor => format!(
                "Active armor infusion: {}",
                armor.active_element.display_name()
            ),
            LoadoutTab::Items => format!(
                "Quick item: {}",
                quick.item_id.as_deref().unwrap_or("None equipped")
            ),
            LoadoutTab::Specials => {
                "A selected special replaces RT until PRIMARY FIRE is chosen.".to_string()
            }
            LoadoutTab::Rides => {
                if let Some(content_id) = vehicle_state.active_published_content_id.as_deref() {
                    format!(
                        "Authored craft {content_id} is active. Use the vehicle input to park it; hold sprint for its tuned turbo."
                    )
                } else {
                    "Walk to a published craft and use the vehicle input to mount it. Rocket Hoverboard remains available from this loadout."
                        .to_string()
                }
            }
            LoadoutTab::Relics => relic_detail_text(&progression.upgrades, selected),
        });
    }
    for (label, mut text) in texts.p2().iter_mut() {
        *text = Text::new(entries.get(label.0).cloned().unwrap_or_default());
    }

    for (interaction, tab, entry, close, mut node, mut background, mut border) in
        buttons.p1().iter_mut()
    {
        if let Some(tab) = tab {
            let active = tab.0 == state.tab;
            *background = BackgroundColor(if active {
                Color::srgb(0.10, 0.48, 0.72)
            } else {
                Color::srgb(0.08, 0.13, 0.25)
            });
            *border = BorderColor::all(if active {
                Color::srgb(0.92, 0.82, 0.28)
            } else {
                Color::srgb(0.16, 0.35, 0.55)
            });
        } else if let Some(entry) = entry {
            node.display = if entry.0 < entries.len() {
                Display::Flex
            } else {
                Display::None
            };
            let focused = entry.0 == selected;
            *background = BackgroundColor(if focused {
                Color::srgb(0.10, 0.34, 0.55)
            } else if *interaction == Interaction::Hovered {
                Color::srgb(0.09, 0.20, 0.34)
            } else {
                Color::srgb(0.055, 0.075, 0.14)
            });
            *border = BorderColor::all(if focused {
                Color::srgb(1.0, 0.82, 0.25)
            } else {
                Color::srgb(0.12, 0.24, 0.40)
            });
        } else if close.is_some() && *interaction == Interaction::Hovered {
            *background = BackgroundColor(Color::srgb(0.55, 0.14, 0.24));
        }
    }
}

fn loadout_entry_labels(
    tab: LoadoutTab,
    weapons: &WeaponInventory,
    specials: &SpecialWeaponInventory,
    armor: &ArmorSet,
    inventory: &Inventory,
    quick: &QuickItemSlot,
    selected_traversal: TraversalMode,
    upgrades: &UpgradeLedger,
) -> Vec<String> {
    match tab {
        LoadoutTab::Weapons => weapons
            .slots
            .iter()
            .enumerate()
            .map(|(index, weapon)| {
                format!(
                    "{} {}  •  Rank {}",
                    if weapons.active_slot == index {
                        "✓"
                    } else {
                        " "
                    },
                    weapon.weapon_type.display_name(),
                    weapon.rank
                )
            })
            .collect(),
        LoadoutTab::Armor => armor_elements()
            .iter()
            .map(|element| {
                format!(
                    "{} {} Infusion",
                    if armor.active_element == *element {
                        "✓"
                    } else {
                        " "
                    },
                    element.display_name()
                )
            })
            .collect(),
        LoadoutTab::Items => consumable_inventory_entries(inventory)
            .into_iter()
            .map(|(id, name, quantity)| {
                format!(
                    "{} {}  ×{}",
                    if quick.item_id.as_deref() == Some(id.as_str()) {
                        "✓"
                    } else {
                        " "
                    },
                    name,
                    quantity
                )
            })
            .collect(),
        LoadoutTab::Specials => std::iter::once(format!(
            "{} PRIMARY FIRE",
            if specials.active_slot.is_none() {
                "✓"
            } else {
                " "
            }
        ))
        .chain(
            [
                &specials.slot7,
                &specials.slot8,
                &specials.slot9,
                &specials.slot0,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, special)| {
                format!(
                    "{} {}  •  Level {}",
                    if specials.active_slot == Some(index as u8) {
                        "✓"
                    } else {
                        " "
                    },
                    special.name,
                    special.level
                )
            }),
        )
        .collect(),
        LoadoutTab::Rides => ride_modes()
            .iter()
            .map(|mode| {
                format!(
                    "{} {}",
                    if selected_traversal == *mode {
                        "✓"
                    } else {
                        " "
                    },
                    mode.label()
                )
            })
            .collect(),
        LoadoutTab::Relics => SABRE_RELIC_CATALOG
            .iter()
            .map(|relic| {
                format!(
                    "{} [{}] {}",
                    relic.kind.shape(),
                    if upgrades.has_relic(relic.id) {
                        "FOUND"
                    } else {
                        "LOCKED"
                    },
                    relic.name
                )
            })
            .collect(),
    }
}

fn activate_loadout_entry(
    tab: LoadoutTab,
    index: usize,
    weapons: &mut WeaponInventory,
    specials: &mut SpecialWeaponInventory,
    armor: &mut ArmorSet,
    inventory: &Inventory,
    quick: &mut QuickItemSlot,
    traversal: &mut TraversalModeState,
) -> String {
    match tab {
        LoadoutTab::Weapons => {
            weapons.active_slot = index.min(weapons.slots.len() - 1);
            weapons.active().weapon_type.display_name().to_string()
        }
        LoadoutTab::Armor => {
            armor.active_element = armor_elements()[index.min(armor_elements().len() - 1)];
            format!("{} Infusion", armor.active_element.display_name())
        }
        LoadoutTab::Items => {
            let entries = consumable_inventory_entries(inventory);
            let Some((id, name, _)) = entries.get(index) else {
                return "no quick item".to_string();
            };
            quick.item_id = Some(id.clone());
            name.clone()
        }
        LoadoutTab::Specials => {
            if index == 0 {
                specials.active_slot = None;
                "Primary Fire".to_string()
            } else {
                specials
                    .select((index - 1).min(3) as u8)
                    .map(|special| special.name.to_string())
                    .unwrap_or_else(|| "Primary Fire".to_string())
            }
        }
        LoadoutTab::Rides => {
            traversal.active = ride_modes()[index.min(ride_modes().len() - 1)];
            traversal.active.label().to_string()
        }
        LoadoutTab::Relics => SABRE_RELIC_CATALOG[index.min(SABRE_RELIC_CATALOG.len() - 1)]
            .name
            .to_string(),
    }
}

fn activate_ride_entry(
    index: usize,
    owner: u8,
    traversal: &mut TraversalModeState,
    vehicle_state: &mut VehicleState,
) -> String {
    let selected = ride_modes()[index.min(ride_modes().len() - 1)];
    if traversal.active == TraversalMode::Vehicle || vehicle_state.player_owns_active_vehicle(owner)
    {
        vehicle_state.select_traversal_while_active(owner, selected);
        traversal.active = TraversalMode::Vehicle;
    } else {
        traversal.active = selected;
    }
    selected.label().to_string()
}

fn relic_detail_text(upgrades: &UpgradeLedger, index: usize) -> String {
    let relic = SABRE_RELIC_CATALOG[index.min(SABRE_RELIC_CATALOG.len() - 1)];
    format!(
        "{} {} — {}  •  INPUT: {}  •  SOURCE: {}",
        if upgrades.has_relic(relic.id) {
            "DISCOVERED"
        } else {
            "LOCKED"
        },
        relic.kind.shape(),
        relic.effect,
        relic.input,
        relic.source_hint
    )
}

fn consumable_inventory_entries(inventory: &Inventory) -> Vec<(String, String, u32)> {
    let definitions = all_items();
    inventory
        .slots
        .iter()
        .flatten()
        .filter_map(|slot| {
            let definition = definitions
                .iter()
                .find(|definition| definition.id == slot.item_id)?;
            (definition.item_type == ItemType::Consumable).then(|| {
                (
                    slot.item_id.clone(),
                    definition.name.to_string(),
                    slot.quantity,
                )
            })
        })
        .collect()
}

fn armor_elements() -> [crate::components::armor::ElementType; 6] {
    use crate::components::armor::ElementType;
    [
        ElementType::None,
        ElementType::Fire,
        ElementType::Ice,
        ElementType::Electric,
        ElementType::DarkEnergy,
        ElementType::Rift,
    ]
}

fn ride_modes() -> [TraversalMode; 4] {
    [
        TraversalMode::Grapple,
        TraversalMode::HoverJet,
        TraversalMode::Flight,
        TraversalMode::Hoverboard,
    ]
}

fn loadout_navigation_direction(
    state: &mut LoadoutPanelState,
    input: &PlayerInput,
    delta_secs: f32,
) -> i8 {
    const INITIAL_REPEAT_DELAY: f32 = 0.34;
    const REPEAT_INTERVAL: f32 = 0.11;
    let just = if input.ui_up {
        -1
    } else if input.ui_down {
        1
    } else {
        0
    };
    let held = if input.ui_vertical > 0.60 {
        -1
    } else if input.ui_vertical < -0.60 {
        1
    } else {
        0
    };
    if just != 0 {
        state.repeat_direction = just;
        state.repeat_timer = INITIAL_REPEAT_DELAY;
        return just;
    }
    if held != state.repeat_direction {
        state.repeat_direction = held;
        state.repeat_timer = INITIAL_REPEAT_DELAY;
        return held;
    }
    if held == 0 {
        state.repeat_direction = 0;
        state.repeat_timer = 0.0;
        return 0;
    }
    state.repeat_timer -= delta_secs;
    if state.repeat_timer <= 0.0 {
        state.repeat_timer = REPEAT_INTERVAL;
        held
    } else {
        0
    }
}

fn cycle_index(current: usize, len: usize, direction: i8) -> usize {
    if len == 0 {
        return 0;
    }
    if direction < 0 {
        (current + len - 1) % len
    } else if direction > 0 {
        (current + 1) % len
    } else {
        current.min(len - 1)
    }
}

fn crafting_navigation_direction(
    state: &mut CraftingPanelState,
    input: &PlayerInput,
    delta_secs: f32,
) -> i8 {
    const INITIAL_REPEAT_DELAY: f32 = 0.34;
    const REPEAT_INTERVAL: f32 = 0.11;

    let just = if input.ui_up {
        -1
    } else if input.ui_down {
        1
    } else {
        0
    };
    let held = if input.ui_vertical > 0.6 {
        -1
    } else if input.ui_vertical < -0.6 {
        1
    } else {
        0
    };

    if just != 0 {
        state.repeat_direction = just;
        state.repeat_timer = INITIAL_REPEAT_DELAY;
        return just;
    }
    if held != state.repeat_direction {
        state.repeat_direction = held;
        state.repeat_timer = INITIAL_REPEAT_DELAY;
        return held;
    }
    if held == 0 {
        state.repeat_direction = 0;
        state.repeat_timer = 0.0;
        return 0;
    }
    state.repeat_timer -= delta_secs;
    if state.repeat_timer <= 0.0 {
        state.repeat_timer = REPEAT_INTERVAL;
        held
    } else {
        0
    }
}

// ── Game Over ─────────────────────────────────────────────────────────────────
fn setup_game_over(mut commands: Commands) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            GameOverRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("SYSTEM FAILURE"),
                TextFont {
                    font_size: FontSize::Px(64.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.2, 0.1)),
            ));
            p.spawn((
                Button,
                Node {
                    width: Val::Px(180.0),
                    height: Val::Px(52.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.34, 0.10, 0.08)),
                BorderColor::all(Color::srgb(0.80, 0.30, 0.22)),
                GameOverMenuButton,
            ))
            .with_children(|button| {
                button.spawn((
                    Text::new("MAIN MENU"),
                    TextFont {
                        font_size: FontSize::Px(18.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

// ── Level-up / chapter event feedback ────────────────────────────────────────

fn level_up_event_system(
    mut ev: MessageReader<PlayerLevelUpEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for e in ev.read() {
        msg_ev.write(UiMessageEvent {
            text: format!("LEVEL UP! Now level {}", e.level),
            duration: 3.0,
        });
    }
}

fn chapter_completed_ui_system(
    mut ev: MessageReader<ChapterCompletedEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for e in ev.read() {
        msg_ev.write(UiMessageEvent {
            text: format!("Chapter {:02} complete!", e.chapter),
            duration: 4.0,
        });
    }
}

fn boss_defeated_ui_system(
    mut ev: MessageReader<BossDefeatedEvent>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for e in ev.read() {
        msg_ev.write(UiMessageEvent {
            text: format!("{} defeated!", e.name),
            duration: 4.0,
        });
    }
}

fn game_over_input(
    interaction_q: Query<&Interaction, (Changed<Interaction>, With<GameOverMenuButton>)>,
    mut next_state: ResMut<NextState<AppState>>,
    go_root: Query<Entity, With<GameOverRoot>>,
    hud_root: Query<Entity, With<HudRoot>>,
    mut commands: Commands,
) {
    for interaction in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            for e in go_root.iter() {
                commands.entity(e).despawn();
            }
            for e in hud_root.iter() {
                commands.entity(e).despawn();
            }
            next_state.set(AppState::MainMenu);
            return;
        }
    }
}

// ── Victory Screen ────────────────────────────────────────────────────────────

fn setup_victory_screen(mut commands: Commands, progress: Res<ChapterProgress>) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(18.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.06, 0.02, 0.96)),
            VictoryRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("VICTORY"),
                TextFont {
                    font_size: FontSize::Px(82.0),
                    ..default()
                },
                TextColor(Color::srgb(0.72, 1.0, 0.52)),
            ));
            root.spawn((
                Text::new("THE EVEREST RANGE IS FREE!"),
                TextFont {
                    font_size: FontSize::Px(28.0),
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.95, 0.72)),
            ));
            let chapter_count = progress.completed.len();
            root.spawn((
                Text::new(format!(
                    "Chapters completed: {}   Sites liberated: 9 / 9",
                    chapter_count
                )),
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(Color::srgb(0.76, 0.92, 0.80)),
            ));
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|row| {
                spawn_victory_button(row, "MAIN MENU", VictoryAction::MainMenu);
                spawn_victory_button(row, "NEW GAME +", VictoryAction::NewGamePlus);
            });
        });
}

fn spawn_victory_button(
    parent: &mut ChildSpawnerCommands,
    label: &'static str,
    action: VictoryAction,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(170.0),
                height: Val::Px(46.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.28, 0.14)),
            BorderColor::all(Color::srgb(0.46, 0.84, 0.42)),
            VictoryMenuButton(action),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn victory_input(
    interaction_q: Query<(&Interaction, &VictoryMenuButton), (Changed<Interaction>, With<Button>)>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    victory_q: Query<Entity, With<VictoryRoot>>,
    mut site_registry: ResMut<WorldSiteRegistry>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if matches!(button.0, VictoryAction::NewGamePlus) {
            site_registry.sites = crate::resources::initial_world_sites();
        }
        for entity in victory_q.iter() {
            commands.entity(entity).despawn();
        }
        next_state.set(AppState::MainMenu);
        return;
    }
}

// ── Settings Panel Input ──────────────────────────────────────────────────────

fn settings_panel_input_system(
    mut settings: ResMut<GameSettings>,
    interaction_q: Query<(&Interaction, &SettingsButton), (Changed<Interaction>, With<Button>)>,
    mut pending_rebind: ResMut<PendingRebind>,
    mut text_q: ParamSet<(
        Query<&mut Text, With<DifficultyText>>,
        Query<&mut Text, With<VolumeText>>,
        Query<&mut Text, With<InterfaceLayoutText>>,
        Query<&mut Text, With<AccessibilitySettingsText>>,
        Query<&mut Text, With<ControlBindingsText>>,
    )>,
) {
    let mut changed = false;
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button.0 {
            SettingsAction::FaceLayoutNext => {
                use crate::engine::bindings::FaceLayout;
                let presets = FaceLayout::PRESETS;
                let current = presets
                    .iter()
                    .position(|preset| *preset == settings.bindings.face_layout)
                    .unwrap_or(0);
                settings.bindings.face_layout = presets[(current + 1) % presets.len()];
            }
            SettingsAction::ToggleInvertLook => {
                settings.bindings.invert_look_y = !settings.bindings.invert_look_y;
            }
            SettingsAction::RebindKey(action) => {
                // Arm capture; the next bindable key press lands on this
                // action (see `settings_rebind_capture_system`).
                pending_rebind.0 = Some(action);
            }
            SettingsAction::ResetKeys => {
                settings.bindings.reset_keys();
                pending_rebind.0 = None;
            }
            SettingsAction::Difficulty(DifficultyChoice::Easy) => settings.difficulty_scale = 0.7,
            SettingsAction::Difficulty(DifficultyChoice::Normal) => settings.difficulty_scale = 1.0,
            SettingsAction::Difficulty(DifficultyChoice::Hard) => settings.difficulty_scale = 1.3,
            SettingsAction::MusicDown => {
                settings.music_volume = (settings.music_volume - 0.1).clamp(0.0, 1.0);
            }
            SettingsAction::MusicUp => {
                settings.music_volume = (settings.music_volume + 0.1).clamp(0.0, 1.0);
            }
            SettingsAction::SfxDown => {
                settings.sfx_volume = (settings.sfx_volume - 0.1).clamp(0.0, 1.0);
            }
            SettingsAction::SfxUp => {
                settings.sfx_volume = (settings.sfx_volume + 0.1).clamp(0.0, 1.0);
            }
            SettingsAction::ToggleRumble => settings.rumble_on_hit = !settings.rumble_on_hit,
            SettingsAction::UiScaleDown => {
                settings.ui_scale = (settings.ui_scale - 0.1).clamp(0.8, 1.4);
            }
            SettingsAction::UiScaleUp => {
                settings.ui_scale = (settings.ui_scale + 0.1).clamp(0.8, 1.4);
            }
            SettingsAction::SafeAreaDown => {
                settings.safe_area_fraction =
                    (settings.safe_area_fraction - 0.005).clamp(0.0, 0.08);
            }
            SettingsAction::SafeAreaUp => {
                settings.safe_area_fraction =
                    (settings.safe_area_fraction + 0.005).clamp(0.0, 0.08);
            }
            SettingsAction::ControllerGlyphStyleNext => {
                settings.controller_glyph_style = settings.controller_glyph_style.next();
            }
            SettingsAction::ToggleDamageNumbers => {
                settings.show_damage_numbers = !settings.show_damage_numbers;
            }
            SettingsAction::ToggleHighContrast => {
                settings.high_contrast_ui = !settings.high_contrast_ui;
            }
            SettingsAction::ToggleReducedMotion => {
                settings.reduced_ui_motion = !settings.reduced_ui_motion;
            }
            SettingsAction::ToggleSubtitles => {
                settings.subtitles_enabled = !settings.subtitles_enabled;
            }
        }
        changed = true;
    }
    if !changed {
        return;
    }
    if let Err(e) = save_settings(&settings) {
        warn!("Failed to persist settings: {e}");
    }
    let diff_str = difficulty_text(&settings);
    for mut text in text_q.p0().iter_mut() {
        *text = Text::new(diff_str.clone());
    }
    let vol_str = volume_text(&settings);
    for mut text in text_q.p1().iter_mut() {
        *text = Text::new(vol_str.clone());
    }
    let interface_str = interface_layout_text(&settings);
    for mut text in text_q.p2().iter_mut() {
        *text = Text::new(interface_str.clone());
    }
    let accessibility_str = accessibility_settings_text(&settings);
    for mut text in text_q.p3().iter_mut() {
        *text = Text::new(accessibility_str.clone());
    }
    let bindings_str = control_bindings_text(&settings, &pending_rebind);
    for mut text in text_q.p4().iter_mut() {
        *text = Text::new(bindings_str.clone());
    }
}

fn difficulty_text(settings: &GameSettings) -> String {
    let difficulty_label = if settings.difficulty_scale <= 0.75 {
        "EASY"
    } else if settings.difficulty_scale <= 1.05 {
        "NORMAL"
    } else {
        "HARD"
    };
    format!("Difficulty: {}", difficulty_label)
}

fn volume_text(settings: &GameSettings) -> String {
    format!(
        "Music: {:.0}%  SFX: {:.0}%  Rumble: {}",
        settings.music_volume * 100.0,
        settings.sfx_volume * 100.0,
        if settings.rumble_on_hit { "ON" } else { "OFF" },
    )
}

/// Live control readout: layout, invert, and every action's current key, with
/// the armed action calling for a press.
fn control_bindings_text(settings: &GameSettings, pending: &PendingRebind) -> String {
    use crate::engine::bindings::{key_name, KeyAction};
    let mut lines = vec![format!(
        "Face layout: {}   Invert look Y: {}",
        settings.bindings.face_layout.label(),
        on_off(settings.bindings.invert_look_y),
    )];
    let face_mapping = settings.bindings.face_layout.mapping_summary();
    if !face_mapping.is_empty() {
        lines.push(format!("Button mapping: {face_mapping}"));
    }
    if let Some(action) = pending.0 {
        lines.push(format!(
            "Press a key for {}…  (Esc cancels)",
            action.label()
        ));
    }
    // Two actions per line keeps the block readable at 420px.
    for pair in KeyAction::ALL.chunks(2) {
        lines.push(
            pair.iter()
                .map(|action| {
                    format!(
                        "{}: {}",
                        action.label(),
                        key_name(settings.bindings.key(*action))
                    )
                })
                .collect::<Vec<_>>()
                .join("   "),
        );
    }
    lines.join("\n")
}

/// While a rebind is armed, the next key press binds — unless it is Escape
/// (cancel), reserved, or already taken, in which case the readout says why.
fn settings_rebind_capture_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<GameSettings>,
    mut pending: ResMut<PendingRebind>,
    mut text_q: Query<&mut Text, With<ControlBindingsText>>,
) {
    let Some(action) = pending.0 else {
        return;
    };
    let Some(pressed) = keyboard.get_just_pressed().next().copied() else {
        return;
    };
    let message = if pressed == KeyCode::Escape {
        pending.0 = None;
        None
    } else {
        use crate::engine::bindings::{key_name, RebindRejection};
        match settings.bindings.rebind(action, pressed) {
            Ok(()) => {
                pending.0 = None;
                None
            }
            Err(RebindRejection::Reserved) => {
                Some(format!("{} is reserved — pick another", key_name(pressed)))
            }
            Err(RebindRejection::Conflict(existing)) => Some(format!(
                "{} already runs {} — pick another",
                key_name(pressed),
                existing.label()
            )),
        }
    };
    // Rejections keep the prompt armed so the player can simply try again.
    for mut text in text_q.iter_mut() {
        *text = Text::new(match &message {
            Some(problem) => format!("{}\n{problem}", control_bindings_text(&settings, &pending)),
            None => control_bindings_text(&settings, &pending),
        });
    }
    if message.is_none() {
        if let Err(error) = crate::plugins::save_plugin::save_settings(&settings) {
            warn!("could not persist control bindings: {error}");
        }
    }
}

fn interface_layout_text(settings: &GameSettings) -> String {
    format!(
        "UI scale: {:.0}%  Safe area: {:.1}%  Controller glyphs: {}",
        settings.ui_scale * 100.0,
        settings.safe_area_fraction * 100.0,
        settings.controller_glyph_style.label(),
    )
}

fn accessibility_settings_text(settings: &GameSettings) -> String {
    format!(
        "Damage numbers: {}  High contrast: {}  Reduced motion: {}  Subtitles: {}",
        on_off(settings.show_damage_numbers),
        on_off(settings.high_contrast_ui),
        on_off(settings.reduced_ui_motion),
        on_off(settings.subtitles_enabled),
    )
}

fn on_off(value: bool) -> &'static str {
    if value {
        "ON"
    } else {
        "OFF"
    }
}

// ── Final Push Unlock ─────────────────────────────────────────────────────────

fn final_push_unlock_system(
    mut commands: Commands,
    final_war: Res<crate::world::final_war::FinalWarRegistry>,
    site_registry: Res<WorldSiteRegistry>,
    existing_q: Query<Entity, With<FinalPushText>>,
    chapter_select_root_q: Query<Entity, With<ChapterSelectRoot>>,
) {
    if !final_war.is_changed() && !site_registry.is_changed() {
        return;
    }
    let liberated = site_registry
        .sites
        .iter()
        .filter(|s| s.is_liberated())
        .count();
    let show = final_war.phase == crate::world::final_war::FinalWarPhase::Climax && liberated >= 6;
    let already_shown = !existing_q.is_empty();
    if show && !already_shown {
        if let Ok(root) = chapter_select_root_q.single() {
            commands.entity(root).with_children(|parent| {
                parent.spawn((
                    Text::new("★ FINAL PUSH UNLOCKED — All sites must be reclaimed!"),
                    TextFont {
                        font_size: FontSize::Px(20.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.88, 0.25)),
                    FinalPushText,
                ));
            });
        }
    } else if !show && already_shown {
        for e in existing_q.iter() {
            commands.entity(e).despawn();
        }
    }
}

// ── Controller Diagnostics Overlay ───────────────────────────────────────────

fn setup_controller_diag(mut commands: Commands) {
    commands
        .spawn((
            ControllerDiagRoot,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(8.0),
                bottom: Val::Px(8.0),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.80)),
        ))
        .with_children(|root| {
            root.spawn((
                ControllerDiagText,
                Text::new("Controller Diagnostics"),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgb(0.80, 0.95, 1.0)),
            ));
        });
}

fn despawn_controller_diag(
    mut commands: Commands,
    q: Query<Entity, With<ControllerDiagRoot>>,
    mut state: ResMut<ControllerDiagState>,
) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
    state.visible = false;
}

fn toggle_controller_diag(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<ControllerDiagState>,
    mut root_q: Query<&mut Visibility, With<ControllerDiagRoot>>,
) {
    // Controller diagnostics: Designer edition only.
    if !cfg!(feature = "designer") || !keyboard.just_pressed(KeyCode::F8) {
        return;
    }
    state.visible = !state.visible;
    let vis = if state.visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in root_q.iter_mut() {
        *v = vis;
    }
}

fn update_controller_diag(
    state: Res<ControllerDiagState>,
    players: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
    gamepads: Query<(Entity, &Gamepad)>,
    assignments: Res<GamepadAssignments>,
    native: Res<NativeControllerState>,
    mut text_q: Query<&mut Text, With<ControllerDiagText>>,
) {
    if !state.visible {
        return;
    }
    let Ok(mut text) = text_q.single_mut() else {
        return;
    };

    let mut lines = Vec::with_capacity(8);
    lines.push("── Controller Diagnostics [F8] ──".to_string());

    for (idx, pi) in players
        .iter()
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<Vec<_>>()
    {
        let i = idx.0 as usize;
        let assigned = assignments.gamepad_for_player(idx.0);
        let gp_info =
            if let Some((entity, gp)) = assigned.and_then(|entity| gamepads.get(entity).ok()) {
                let ls = Vec2::new(
                    gp.get(bevy::input::gamepad::GamepadAxis::LeftStickX)
                        .unwrap_or(0.0),
                    gp.get(bevy::input::gamepad::GamepadAxis::LeftStickY)
                        .unwrap_or(0.0),
                );
                let rs = Vec2::new(
                    gp.get(bevy::input::gamepad::GamepadAxis::RightStickX)
                        .unwrap_or(0.0),
                    gp.get(bevy::input::gamepad::GamepadAxis::RightStickY)
                        .unwrap_or(0.0),
                );
                format!(
                    "PAD {} → P{}  L({:+.2},{:+.2}) R({:+.2},{:+.2})",
                    entity.index(),
                    i + 1,
                    ls.x,
                    ls.y,
                    rs.x,
                    rs.y
                )
            } else if assignments.native_player() == Some(idx.0) && native.connected {
                format!(
                    "Native({}) L({:+.2},{:+.2})",
                    &native.name[..native.name.len().min(12)],
                    native.move_axis.x,
                    native.move_axis.y
                )
            } else {
                "KB/no pad".to_string()
            };

        let mut actions = String::new();
        if pi.fire {
            actions.push_str(" FIRE");
        }
        if pi.jump {
            actions.push_str(" JMP");
        }
        if pi.sprint {
            actions.push_str(" SPR");
        }
        if pi.dodge {
            actions.push_str(" DODGE");
        }
        if pi.melee_light {
            actions.push_str(" ML");
        }
        if pi.melee_heavy {
            actions.push_str(" MH");
        }

        lines.push(format!(
            "P{}  Move({:+.2},{:+.2})  {}{}",
            i + 1,
            pi.move_axis.x,
            pi.move_axis.y,
            gp_info,
            if actions.is_empty() {
                String::new()
            } else {
                format!(" |{}", actions)
            }
        ));
    }

    if native.connected {
        lines.push(format!("Native: {} ✓", native.name));
    } else {
        lines.push("Native: --".to_string());
    }

    *text = Text::new(lines.join("\n"));
}

// ── Command Strategy Overlay ──────────────────────────────────────────────────

#[derive(Component)]
struct CommandOverlayRoot;

#[derive(Component)]
struct CommandOverlayText;

fn setup_command_overlay(mut commands: Commands) {
    commands
        .spawn((
            CommandOverlayRoot,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(8.0),
                bottom: Val::Px(8.0),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                max_width: Val::Px(340.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.05, 0.12, 0.85)),
        ))
        .with_children(|root| {
            root.spawn((
                CommandOverlayText,
                Text::new("Command Assets"),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgb(0.70, 1.0, 0.80)),
            ));
        });
}

fn despawn_command_overlay(
    mut commands: Commands,
    q: Query<Entity, With<CommandOverlayRoot>>,
    mut state: ResMut<CommandOverlayState>,
) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
    state.enabled = false;
}

fn toggle_command_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<CommandOverlayState>,
    mut root_q: Query<&mut Visibility, With<CommandOverlayRoot>>,
) {
    if !keyboard.just_pressed(KeyCode::F7) {
        return;
    }
    state.enabled = !state.enabled;
    let vis = if state.enabled {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in root_q.iter_mut() {
        *v = vis;
    }
}

fn update_command_overlay(
    state: Res<CommandOverlayState>,
    command_registry: Res<CommandRegistry>,
    published_fleet: Option<Res<PublishedFleet>>,
    site_registry: Res<WorldSiteRegistry>,
    mut text_q: Query<&mut Text, With<CommandOverlayText>>,
) {
    if !state.enabled {
        return;
    }
    let Ok(mut text) = text_q.single_mut() else {
        return;
    };

    let mut lines = Vec::new();
    lines.push("── Command Assets [F7] ──".to_string());

    for site in &site_registry.sites {
        let at_site = command_registry.assets_at_site(site.id);
        let published_at_site = published_fleet
            .as_ref()
            .map(|fleet| {
                fleet
                    .assets
                    .values()
                    .filter(|asset| asset.assigned_site == Some(site.id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if at_site.is_empty() && published_at_site.is_empty() {
            continue;
        }
        lines.push(format!("{} [{:?}]:", site.name, site.state));
        for a in at_site {
            lines.push(format!(
                "  {} hp:{} rd:{}",
                a.kind.label(),
                a.health,
                a.readiness
            ));
        }
        for asset in published_at_site {
            lines.push(format!(
                "  {} [{}] hp:{} rd:{} def:{}",
                asset.display_name,
                asset.role.label(),
                asset.health,
                asset.readiness,
                asset.defense_contribution(),
            ));
        }
    }

    let unassigned = command_registry.unassigned();
    if !unassigned.is_empty() {
        lines.push("Reserve:".to_string());
        for a in unassigned {
            lines.push(format!(
                "  {} hp:{} rd:{}",
                a.kind.label(),
                a.health,
                a.readiness
            ));
        }
    }

    let published_reserve = published_fleet
        .as_ref()
        .map(|fleet| {
            fleet
                .assets
                .values()
                .filter(|asset| asset.assigned_site.is_none())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !published_reserve.is_empty() {
        lines.push("Published reserve:".to_string());
        for asset in published_reserve {
            lines.push(format!(
                "  {} [{}] hp:{} rd:{}",
                asset.display_name,
                asset.role.label(),
                asset.health,
                asset.readiness,
            ));
        }
    }

    if command_registry.assets.is_empty()
        && published_fleet
            .as_ref()
            .is_none_or(|fleet| fleet.assets.is_empty())
    {
        lines.push("No command assets yet.".to_string());
    }

    *text = Text::new(lines.join("\n"));
}

#[cfg(test)]
mod menu_navigation_tests {
    use super::*;

    #[test]
    fn hidden_or_occluded_menu_geometry_is_not_focusable() {
        let covering_window = UiFocusRect {
            min: Vec2::new(-50.0, -50.0),
            max: Vec2::new(50.0, 50.0),
        };
        assert!(center_is_occluded(Vec2::ZERO, 3, &[(covering_window, 4)]));
        assert!(
            !center_is_occluded(Vec2::ZERO, 4, &[(covering_window, 4)]),
            "a window cannot occlude its own descendants at the same stack"
        );
        assert!(!center_is_occluded(
            Vec2::new(60.0, 0.0),
            3,
            &[(covering_window, 4)]
        ));

        let mut world = World::new();
        let hidden = world
            .spawn(Node {
                display: Display::None,
                ..default()
            })
            .id();
        let child = world.spawn(Node::default()).id();
        world.entity_mut(hidden).add_child(child);
        let mut parents = world.query::<&ChildOf>();
        let mut nodes = world.query::<&Node>();
        assert!(has_display_none_ancestor(
            child,
            &parents.query(&world),
            &nodes.query(&world)
        ));
    }

    #[test]
    fn production_buttons_expose_ui_geometry_to_menu_navigation() {
        let mut world = World::new();
        let button = world.spawn((Button, MenuFocusable)).id();

        assert!(world.entity(button).contains::<UiGlobalTransform>());
        assert!(
            !world.entity(button).contains::<GlobalTransform>(),
            "Bevy UI buttons do not carry the 3D transform used by world entities"
        );

        world.entity_mut(button).insert((
            UiGlobalTransform::from_xy(120.0, 80.0),
            ComputedNode {
                size: Vec2::new(60.0, 20.0),
                ..default()
            },
        ));
        let mut geometry = world.query::<(&UiGlobalTransform, &ComputedNode)>();
        let (transform, computed) = geometry.get(&world, button).unwrap();

        assert_eq!(
            ui_node_rect(transform, computed),
            UiFocusRect {
                min: Vec2::new(90.0, 70.0),
                max: Vec2::new(150.0, 90.0),
            }
        );
    }

    #[test]
    fn controller_confirm_drives_a_real_start_button_in_one_frame() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<AppState>()
            .add_message::<GamepadButtonStateChangedEvent>()
            .add_message::<CursorMoved>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .init_resource::<GameSettings>()
            .init_resource::<AuthoringTextInputCapture>()
            .init_resource::<MenuFocus>()
            .init_resource::<MainMenuUiState>()
            .configure_sets(
                PreUpdate,
                (MenuUiSet::ReleaseActivation, MenuUiSet::FocusDispatch).chain(),
            )
            .configure_sets(Update, MenuUiSet::ActionConsumers)
            .add_systems(
                PreUpdate,
                release_menu_controller_activation.in_set(MenuUiSet::ReleaseActivation),
            )
            .add_systems(
                PreUpdate,
                (register_menu_buttons, menu_focus_navigation)
                    .chain()
                    .in_set(MenuUiSet::FocusDispatch),
            )
            .add_systems(Update, menu_start_button.in_set(MenuUiSet::ActionConsumers));

        let button = app
            .world_mut()
            .spawn((
                Button,
                StartButton,
                ComputedNode {
                    size: Vec2::new(260.0, 58.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(0.0, 0.0),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        let gamepad = app.world_mut().spawn_empty().id();
        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                gamepad,
                GamepadButton::South,
                ButtonState::Pressed,
            ));

        app.update();

        assert_eq!(app.world().resource::<MenuFocus>().entity, Some(button));
        assert_eq!(
            app.world().get::<Interaction>(button),
            Some(&Interaction::Pressed)
        );
        assert!(matches!(
            app.world().resource::<NextState<AppState>>(),
            NextState::Pending(AppState::PlayerSelect)
        ));

        app.update();
        assert_eq!(
            app.world().get::<Interaction>(button),
            Some(&Interaction::None),
            "controller activation must release before the next pointer pass"
        );
    }

    #[test]
    fn gameplay_direction_event_is_not_replayed_when_pause_opens() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Playing)
            .add_message::<GamepadButtonStateChangedEvent>()
            .add_message::<CursorMoved>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .init_resource::<GameSettings>()
            .init_resource::<AuthoringTextInputCapture>()
            .init_resource::<MenuFocus>()
            .add_systems(PreUpdate, menu_focus_navigation);

        let top = app
            .world_mut()
            .spawn((
                Button,
                MenuFocusable,
                ComputedNode {
                    size: Vec2::new(120.0, 40.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(0.0, -20.0),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        app.world_mut().spawn((
            Button,
            MenuFocusable,
            ComputedNode {
                size: Vec2::new(120.0, 40.0),
                ..default()
            },
            UiGlobalTransform::from_xy(0.0, 20.0),
            InheritedVisibility::VISIBLE,
        ));
        let gamepad = app.world_mut().spawn_empty().id();
        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                gamepad,
                GamepadButton::DPadDown,
                ButtonState::Pressed,
            ));

        app.update();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Paused);
        app.update();
        // StateTransition runs after PreUpdate in Bevy 0.19, so navigation
        // sees the newly entered menu on the following frame.
        app.update();

        assert_eq!(
            app.world().resource::<MenuFocus>().entity,
            Some(top),
            "gameplay input must be drained instead of moving the first pause-menu focus"
        );
    }

    #[test]
    fn raw_start_event_opens_pause_without_a_gamepad_component() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Playing)
            .add_message::<GamepadButtonStateChangedEvent>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .init_resource::<UiGameplayCapture>()
            .init_resource::<PauseMenuState>()
            .init_resource::<PlaySessionTransition>()
            .add_systems(Update, pause_input_system);

        let gamepad = app.world_mut().spawn_empty().id();
        assert_eq!(
            app.world_mut()
                .resource_mut::<GamepadAssignments>()
                .assign_next(gamepad, &[true, false, false, false]),
            Some(1)
        );
        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                gamepad,
                GamepadButton::Start,
                ButtonState::Pressed,
            ));

        app.update();

        assert!(matches!(
            app.world().resource::<NextState<AppState>>(),
            NextState::Pending(AppState::Paused)
        ));
        let transition = app.world().resource::<PlaySessionTransition>();
        assert!(transition.pausing);
        assert!(!transition.resuming_from_pause);
        let menu = app.world().resource::<PauseMenuState>();
        assert!(!menu.resume_armed);
        assert_eq!(menu.resume_lockout, 0.20);
    }

    #[test]
    fn raw_start_event_resumes_an_armed_pause_menu() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Paused)
            .add_message::<GamepadButtonStateChangedEvent>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .init_resource::<UiGameplayCapture>()
            .init_resource::<PauseMenuState>()
            .init_resource::<PlaySessionTransition>()
            .add_systems(Update, pause_input_system);
        app.world_mut()
            .resource_mut::<PauseMenuState>()
            .resume_armed = true;
        let gamepad = app.world_mut().spawn_empty().id();
        assert_eq!(
            app.world_mut()
                .resource_mut::<GamepadAssignments>()
                .assign_next(gamepad, &[true, false, false, false]),
            Some(1)
        );
        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                gamepad,
                GamepadButton::Start,
                ButtonState::Pressed,
            ));

        app.update();

        assert!(matches!(
            app.world().resource::<NextState<AppState>>(),
            NextState::Pending(AppState::Playing)
        ));
        let transition = app.world().resource::<PlaySessionTransition>();
        assert!(!transition.pausing);
        assert!(transition.resuming_from_pause);
        assert!(!app.world().resource::<PauseMenuState>().resume_armed);
    }

    #[test]
    fn adjacent_overlay_start_edges_do_not_resume_then_repause() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Paused)
            .add_message::<GamepadButtonStateChangedEvent>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(NativeControllerState::connected_with_just_pressed(
                NativeButton::Start,
            ))
            .init_resource::<GamepadAssignments>()
            .init_resource::<UiGameplayCapture>()
            .init_resource::<PauseMenuState>()
            .init_resource::<PlaySessionTransition>()
            .add_systems(Update, pause_input_system);
        let overlay = app.world_mut().spawn_empty().id();
        {
            let mut assignments = app.world_mut().resource_mut::<GamepadAssignments>();
            assert_eq!(assignments.assign_primary(overlay), Some(0));
            assert_eq!(assignments.assign_native_overlay(overlay), Some(0));
        }
        app.world_mut()
            .resource_mut::<PauseMenuState>()
            .resume_armed = true;

        // Native reports Start first and resumes. The Bevy half of the same
        // physical edge arrives after the state transition on the next frame.
        app.update();
        app.insert_resource(NativeControllerState::connected_without_edges());
        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                overlay,
                GamepadButton::Start,
                ButtonState::Pressed,
            ));
        app.update();

        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Playing);
        assert!(!matches!(
            app.world().resource::<NextState<AppState>>(),
            NextState::Pending(AppState::Paused)
        ));
    }

    #[test]
    fn escape_back_and_pause_toggle_share_one_resume_transition() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Paused)
            .add_message::<GamepadButtonStateChangedEvent>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .init_resource::<GameSettings>()
            .init_resource::<AuthoringTextInputCapture>()
            .init_resource::<AuthoringUnsavedChanges>()
            .init_resource::<CharacterDesignData>()
            .init_resource::<ImportedForgeReturnTarget>()
            .init_resource::<MainMenuUiState>()
            .init_resource::<PauseMenuState>()
            .init_resource::<UiGameplayCapture>()
            .init_resource::<PlaySessionTransition>()
            .add_systems(PreUpdate, menu_back_navigation)
            .add_systems(Update, pause_input_system);
        app.world_mut()
            .resource_mut::<PauseMenuState>()
            .resume_armed = true;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);

        app.update();

        assert!(matches!(
            app.world().resource::<NextState<AppState>>(),
            NextState::Pending(AppState::Playing)
        ));
        let transition = app.world().resource::<PlaySessionTransition>();
        assert!(!transition.pausing);
        assert!(transition.resuming_from_pause);
    }

    #[test]
    fn lobby_join_reader_discards_start_pressed_before_player_select() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<AppState>()
            .add_message::<GamepadButtonStateChangedEvent>()
            .init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .init_resource::<PlayerSelectState>()
            .add_systems(Update, player_select_controller_join);
        let gamepad = app.world_mut().spawn(Gamepad::default()).id();
        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                gamepad,
                GamepadButton::Start,
                ButtonState::Pressed,
            ));

        app.update();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::PlayerSelect);
        app.update();

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.player_for_gamepad(gamepad), None);
        assert!(!app.world().resource::<PlayerSelectState>().slots[1].joined);
    }

    #[test]
    fn disconnected_start_event_cannot_rebind_a_missing_gamepad() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::PlayerSelect)
            .add_message::<GamepadButtonStateChangedEvent>()
            .init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .init_resource::<PlayerSelectState>()
            .add_systems(Update, player_select_controller_join);
        let disconnected = app.world_mut().spawn_empty().id();
        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                disconnected,
                GamepadButton::Start,
                ButtonState::Pressed,
            ));

        app.update();

        assert_eq!(
            app.world()
                .resource::<GamepadAssignments>()
                .player_for_gamepad(disconnected),
            None
        );
    }

    #[test]
    fn adjacent_native_and_bevy_start_does_not_join_the_same_pad_as_p2() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::PlayerSelect)
            .add_message::<GamepadButtonStateChangedEvent>()
            .insert_resource(NativeControllerState::connected_with_just_pressed(
                NativeButton::Start,
            ))
            .init_resource::<GamepadAssignments>()
            .init_resource::<PlayerSelectState>()
            .add_systems(
                PreUpdate,
                crate::plugins::input_plugin::sync_native_controller_assignment,
            )
            .add_systems(Update, player_select_controller_join);
        let bevy_half = app.world_mut().spawn(Gamepad::default()).id();
        assert_eq!(
            app.world_mut()
                .resource_mut::<GamepadAssignments>()
                .assign_native_primary(),
            Some(0)
        );

        // The native backend reports Start first. The Bevy event for that
        // physical press follows one frame later.
        app.update();
        app.insert_resource(NativeControllerState::connected_without_edges());
        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                bevy_half,
                GamepadButton::Start,
                ButtonState::Pressed,
            ));
        app.update();

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.gamepad_for_player(0), None);
        assert_eq!(assignments.player_for_gamepad(bevy_half), None);
        assert!(!app.world().resource::<PlayerSelectState>().slots[1].joined);
    }

    #[test]
    fn bevy_start_waits_for_an_adjacent_native_edge_before_joining_p2() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::PlayerSelect)
            .add_message::<GamepadButtonStateChangedEvent>()
            .insert_resource(NativeControllerState::connected_without_edges())
            .init_resource::<GamepadAssignments>()
            .init_resource::<PlayerSelectState>()
            .add_systems(
                PreUpdate,
                crate::plugins::input_plugin::sync_native_controller_assignment,
            )
            .add_systems(Update, player_select_controller_join);
        let bevy_half = app.world_mut().spawn(Gamepad::default()).id();
        assert_eq!(
            app.world_mut()
                .resource_mut::<GamepadAssignments>()
                .assign_native_primary(),
            Some(0)
        );

        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                bevy_half,
                GamepadButton::Start,
                ButtonState::Pressed,
            ));
        app.update();
        app.insert_resource(NativeControllerState::connected_with_just_pressed(
            NativeButton::Start,
        ));
        app.update();

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.player_for_gamepad(bevy_half), None);
        assert!(!app.world().resource::<PlayerSelectState>().slots[1].joined);
    }

    #[test]
    fn distinct_bevy_controller_joins_after_native_correlation_wait() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::PlayerSelect)
            .add_message::<GamepadButtonStateChangedEvent>()
            .insert_resource(NativeControllerState::connected_without_edges())
            .init_resource::<GamepadAssignments>()
            .init_resource::<PlayerSelectState>()
            .add_systems(
                PreUpdate,
                crate::plugins::input_plugin::sync_native_controller_assignment,
            )
            .add_systems(Update, player_select_controller_join);
        let second_controller = app.world_mut().spawn(Gamepad::default()).id();
        assert_eq!(
            app.world_mut()
                .resource_mut::<GamepadAssignments>()
                .assign_native_primary(),
            Some(0)
        );

        app.world_mut()
            .write_message(GamepadButtonStateChangedEvent::new(
                second_controller,
                GamepadButton::Start,
                ButtonState::Pressed,
            ));
        app.update();
        for _ in 0..NATIVE_JOIN_CORRELATION_FRAMES {
            app.update();
        }

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.gamepad_for_player(1), Some(second_controller));
        assert!(app.world().resource::<PlayerSelectState>().slots[1].joined);
    }

    #[test]
    fn project_hub_root_is_a_top_aligned_scroll_surface() {
        let node = project_hub_root_node();
        assert_eq!(node.width, Val::Percent(100.0));
        assert_eq!(node.height, Val::Percent(100.0));
        assert_eq!(node.justify_content, JustifyContent::FlexStart);
        assert_eq!(node.align_items, AlignItems::Center);
        assert_eq!(node.overflow, Overflow::scroll_y());
        assert!(matches!(node.row_gap, Val::Px(gap) if gap >= 12.0));

        let button = project_hub_button_node();
        assert_eq!(button.width, Val::Percent(100.0));
        assert_eq!(button.max_width, Val::Px(310.0));
        assert!(matches!(button.min_height, Val::Px(height) if height >= 52.0));
    }

    #[test]
    fn project_hub_registers_each_forge_once_with_its_exact_label() {
        let actions = project_hub_authoring_actions();
        let expected = [
            ("CREATURE FORGE", ProjectHubAction::CreatureForge),
            ("WEAPON FORGE", ProjectHubAction::WeaponForge),
            ("VEHICLE FORGE", ProjectHubAction::VehicleForge),
            ("SPACESHIP FORGE", ProjectHubAction::SpaceshipForge),
            (
                "IMPORTED CHARACTER FORGE",
                ProjectHubAction::ImportedCharacterForge,
            ),
        ];

        assert_eq!(actions.map(|(label, action, _)| (label, action)), expected);
        for (index, (_, action, _)) in actions.iter().enumerate() {
            assert!(
                actions[..index]
                    .iter()
                    .all(|(_, previous, _)| previous != action),
                "each Project Hub destination must have one stable action"
            );
        }

        assert_eq!(
            project_hub_forge_destination(ProjectHubAction::VehicleForge),
            Some(AppState::VehicleForge)
        );
        assert_eq!(
            project_hub_forge_destination(ProjectHubAction::SpaceshipForge),
            Some(AppState::SpaceshipForge)
        );
        assert_eq!(
            forge_back_destination(&AppState::VehicleForge),
            Some(AppState::ProjectHub)
        );
        assert_eq!(
            forge_back_destination(&AppState::SpaceshipForge),
            Some(AppState::ProjectHub)
        );
    }

    #[test]
    fn project_backed_hub_actions_require_an_active_project() {
        assert!(project_hub_requires_active_project(
            ProjectHubAction::VehicleForge
        ));
        assert!(project_hub_requires_active_project(
            ProjectHubAction::SpaceshipForge
        ));
        assert!(project_hub_requires_active_project(
            ProjectHubAction::Publish
        ));
        assert!(!project_hub_requires_active_project(
            ProjectHubAction::NewProject
        ));
        assert!(!project_hub_requires_active_project(ProjectHubAction::Back));
    }

    #[test]
    fn dungeon_guidance_switches_from_open_to_repeat_entry() {
        assert_eq!(dungeon_gate_guidance_verb(false), "Open");
        assert_eq!(dungeon_gate_guidance_verb(true), "Enter");
    }

    fn four_button_grid() -> (World, Vec<(Entity, Vec2)>) {
        let mut world = World::new();
        let top_left = world.spawn_empty().id();
        let top_right = world.spawn_empty().id();
        let bottom_left = world.spawn_empty().id();
        let bottom_right = world.spawn_empty().id();
        let buttons = vec![
            (top_left, Vec2::new(-10.0, -10.0)),
            (top_right, Vec2::new(10.0, -10.0)),
            (bottom_left, Vec2::new(-10.0, 10.0)),
            (bottom_right, Vec2::new(10.0, 10.0)),
        ];
        (world, buttons)
    }

    #[test]
    fn default_focus_starts_at_top_left() {
        let (_world, buttons) = four_button_grid();
        assert_eq!(default_menu_focus(&buttons), Some(buttons[0].0));
    }

    #[test]
    fn pointer_focus_changes_only_after_real_pointer_activity() {
        assert!(pointer_interaction_claims_focus(
            Interaction::Hovered,
            true,
            false
        ));
        assert!(pointer_interaction_claims_focus(
            Interaction::Pressed,
            false,
            true
        ));
        assert!(!pointer_interaction_claims_focus(
            Interaction::Hovered,
            false,
            false
        ));
        assert!(!pointer_interaction_claims_focus(
            Interaction::Pressed,
            false,
            false
        ));
        assert!(!pointer_interaction_claims_focus(
            Interaction::None,
            true,
            true
        ));
    }

    #[test]
    fn focus_styling_preserves_runtime_selection_colors() {
        let mut app = App::new();
        app.init_resource::<MenuFocus>()
            .init_resource::<GameSettings>()
            .add_systems(Update, menu_focus_style);
        let base_background = BackgroundColor(Color::srgb(0.08, 0.09, 0.10));
        let selected_background = BackgroundColor(Color::srgb(0.32, 0.18, 0.08));
        let base_border = BorderColor::all(Color::srgb(0.20, 0.22, 0.24));
        let button = app
            .world_mut()
            .spawn((
                Button,
                MenuFocusable,
                MenuButtonPalette {
                    background: base_background,
                    border: base_border,
                    applied_background: None,
                    applied_border: None,
                },
                base_background,
                base_border,
            ))
            .id();
        app.world_mut().resource_mut::<MenuFocus>().entity = Some(button);
        app.update();

        // A screen-specific action changes only its semantic background while
        // the shared focus highlight is active.
        *app.world_mut()
            .entity_mut(button)
            .get_mut::<BackgroundColor>()
            .unwrap() = selected_background;
        app.world_mut().resource_mut::<MenuFocus>().entity = None;
        app.update();

        assert_eq!(
            app.world().get::<BackgroundColor>(button),
            Some(&selected_background)
        );
        assert_eq!(app.world().get::<BorderColor>(button), Some(&base_border));
    }

    #[test]
    fn gamepad_button_event_fallback_requires_a_matching_press() {
        let mut world = World::new();
        let gamepad = world.spawn_empty().id();
        let events = [
            GamepadButtonStateChangedEvent::new(
                gamepad,
                GamepadButton::DPadDown,
                ButtonState::Released,
            ),
            GamepadButtonStateChangedEvent::new(
                gamepad,
                GamepadButton::South,
                ButtonState::Pressed,
            ),
        ];

        assert!(gamepad_button_pressed_in_events(
            &events,
            GamepadButton::South,
            None,
        ));
        assert!(!gamepad_button_pressed_in_events(
            &events,
            GamepadButton::DPadDown,
            None,
        ));
        assert!(!gamepad_button_pressed_in_events(
            &events,
            GamepadButton::Start,
            None,
        ));
        assert!(!gamepad_button_pressed_in_events(
            &events,
            GamepadButton::South,
            Some(gamepad),
        ));
    }

    #[test]
    fn shared_menu_confirm_and_back_follow_the_face_layout() {
        let mut bindings = ControlBindings::default();
        assert_eq!(
            mapped_menu_face(&bindings, FaceButton::South),
            (GamepadButton::South, NativeButton::South)
        );
        assert_eq!(
            mapped_menu_face(&bindings, FaceButton::East),
            (GamepadButton::East, NativeButton::East)
        );

        bindings.face_layout = crate::engine::bindings::FaceLayout::Nintendo;
        assert_eq!(
            mapped_menu_face(&bindings, FaceButton::South),
            (GamepadButton::East, NativeButton::East)
        );
        assert_eq!(
            mapped_menu_face(&bindings, FaceButton::East),
            (GamepadButton::South, NativeButton::South)
        );
    }

    #[test]
    fn accessibility_summary_reports_every_toggle_without_color_dependency() {
        let settings = GameSettings {
            show_damage_numbers: false,
            high_contrast_ui: true,
            reduced_ui_motion: true,
            subtitles_enabled: false,
            ..default()
        };
        let summary = accessibility_settings_text(&settings);
        assert!(summary.contains("Damage numbers: OFF"));
        assert!(summary.contains("High contrast: ON"));
        assert!(summary.contains("Reduced motion: ON"));
        assert!(summary.contains("Subtitles: OFF"));
    }

    #[test]
    fn world_map_projection_places_origin_at_center_and_clamps_edges() {
        assert!((world_to_map_left(0.0) - 50.0).abs() < 1e-4);
        assert!((world_to_map_top(0.0) - 50.0).abs() < 1e-4);
        assert_eq!(world_to_map_left(f32::MAX), 98.0);
        assert_eq!(world_to_map_top(f32::MAX), 2.0);
    }

    #[test]
    fn directional_focus_follows_spatial_grid() {
        let (_world, buttons) = four_button_grid();
        assert_eq!(
            directional_menu_focus(buttons[0].0, MenuDirection::Right, &buttons),
            Some(buttons[1].0)
        );
        assert_eq!(
            directional_menu_focus(buttons[1].0, MenuDirection::Down, &buttons),
            Some(buttons[3].0)
        );
        assert_eq!(
            directional_menu_focus(buttons[3].0, MenuDirection::Left, &buttons),
            Some(buttons[2].0)
        );
    }

    #[test]
    fn directional_focus_stays_put_at_an_edge() {
        let (_world, buttons) = four_button_grid();
        assert_eq!(
            directional_menu_focus(buttons[0].0, MenuDirection::Up, &buttons),
            None
        );
    }

    #[test]
    fn held_direction_waits_then_repeats() {
        let mut focus = MenuFocus::default();
        let held = MenuDirectionInput {
            just: Some(MenuDirection::Down),
            held: Some(MenuDirection::Down),
        };
        assert_eq!(
            repeated_menu_direction(&mut focus, held, 0.0),
            Some(MenuDirection::Down)
        );
        let held = MenuDirectionInput {
            just: None,
            held: Some(MenuDirection::Down),
        };
        assert_eq!(repeated_menu_direction(&mut focus, held, 0.2), None);
        assert_eq!(
            repeated_menu_direction(&mut focus, held, 0.2),
            Some(MenuDirection::Down)
        );
    }

    #[test]
    fn released_direction_clears_repeat_state() {
        let mut focus = MenuFocus {
            repeat_direction: Some(MenuDirection::Right),
            repeat_timer: 0.1,
            ..default()
        };
        assert_eq!(
            repeated_menu_direction(&mut focus, MenuDirectionInput::default(), 0.2),
            None
        );
        assert_eq!(focus.repeat_direction, None);
    }

    #[test]
    fn focused_item_scrolls_only_when_outside_viewport() {
        assert_eq!(focus_reveal_scroll_delta(10.0, 90.0, 20.0, 40.0), 0.0);
        assert_eq!(focus_reveal_scroll_delta(10.0, 90.0, -5.0, 5.0), -15.0);
        assert_eq!(focus_reveal_scroll_delta(10.0, 90.0, 95.0, 105.0), 15.0);
    }

    #[test]
    fn crafting_selection_wraps_in_both_directions() {
        assert_eq!(cycle_index(0, 5, -1), 4);
        assert_eq!(cycle_index(4, 5, 1), 0);
        assert_eq!(cycle_index(2, 5, 0), 2);
        assert_eq!(cycle_index(0, 0, 1), 0);
    }

    #[test]
    fn crafting_stick_navigation_waits_then_repeats() {
        let mut state = CraftingPanelState::default();
        let input = PlayerInput {
            ui_vertical: -1.0,
            ..default()
        };
        assert_eq!(crafting_navigation_direction(&mut state, &input, 0.0), 1);
        assert_eq!(crafting_navigation_direction(&mut state, &input, 0.2), 0);
        assert_eq!(crafting_navigation_direction(&mut state, &input, 0.2), 1);
    }

    #[test]
    fn crafting_selection_is_persistent_per_player() {
        let mut state = CraftingPanelState::default();
        state.recipe_index[0] = 3;
        state.recipe_index[1] = 1;
        assert_eq!(state.recipe_index[0], 3);
        assert_eq!(state.recipe_index[1], 1);
    }

    #[test]
    fn workshop_tabs_and_jewel_cursors_are_persistent_per_player() {
        let mut state = CraftingPanelState::default();
        state.tab[0] = state.tab[0].offset(1);
        state.jewel_index[0] = 6;
        state.jewel_index[1] = 2;

        assert_eq!(state.tab[0], WorkshopTab::Jewels);
        assert_eq!(state.tab[1], WorkshopTab::Crafting);
        assert_eq!(state.jewel_index[0], 6);
        assert_eq!(state.jewel_index[1], 2);
        assert_eq!(WorkshopTab::Jewels.offset(1), WorkshopTab::BioGarden);
        assert_eq!(WorkshopTab::BioGarden.offset(1), WorkshopTab::Market);
        assert_eq!(WorkshopTab::Market.offset(1), WorkshopTab::Crafting);
        assert_eq!(WorkshopTab::Crafting.offset(-1), WorkshopTab::Market);
    }

    #[test]
    fn jewel_workshop_prefers_the_strongest_available_tier() {
        let mut inventory = Inventory::default();
        assert_eq!(best_available_jewel(&inventory), None);

        assert_eq!(inventory.add_item(JewelTier::Rough.item_id(), 1, 99), 0);
        assert_eq!(best_available_jewel(&inventory), Some(JewelTier::Rough));

        assert_eq!(inventory.add_item(JewelTier::Cut.item_id(), 1, 99), 0);
        assert_eq!(best_available_jewel(&inventory), Some(JewelTier::Cut));

        assert_eq!(inventory.add_item(JewelTier::Flawless.item_id(), 1, 99), 0);
        assert_eq!(best_available_jewel(&inventory), Some(JewelTier::Flawless));
        assert_eq!(jewel_tier_label(JewelTier::Flawless), "Flawless +50%");
        assert_eq!(
            weapon_socket_label(WeaponSocket::TrackingMissile),
            "Homing Star"
        );
    }

    #[test]
    fn loadout_tabs_wrap_and_keep_per_player_selection() {
        assert_eq!(LoadoutTab::Weapons.offset(-1), LoadoutTab::Relics);
        assert_eq!(LoadoutTab::Relics.offset(1), LoadoutTab::Weapons);
        let mut state = LoadoutPanelState::default();
        state.selection[0][LoadoutTab::Weapons.index()] = 4;
        state.selection[1][LoadoutTab::Weapons.index()] = 1;
        assert_eq!(state.selection[0][LoadoutTab::Weapons.index()], 4);
        assert_eq!(state.selection[1][LoadoutTab::Weapons.index()], 1);
    }

    #[test]
    fn relic_catalog_uses_shape_and_ownership_without_color_dependency() {
        let mut upgrades = UpgradeLedger::default();
        upgrades.unlock_relic("solar_sabre_glyph");
        let labels = loadout_entry_labels(
            LoadoutTab::Relics,
            &WeaponInventory::default(),
            &SpecialWeaponInventory::default(),
            &ArmorSet::default(),
            &Inventory::default(),
            &QuickItemSlot::default(),
            TraversalMode::Grapple,
            &upgrades,
        );
        assert_eq!(labels.len(), SABRE_RELIC_CATALOG.len());
        assert!(labels[0].contains("(O) [FOUND]"));
        assert!(labels[1].contains(">> [LOCKED]"));
        let detail = relic_detail_text(&upgrades, 0);
        assert!(detail.contains("DISCOVERED"));
        assert!(detail.contains("INPUT:"));
        assert!(detail.contains("SOURCE:"));
    }

    #[test]
    fn loadout_activation_changes_only_the_requested_category() {
        let mut weapons = WeaponInventory::default();
        let mut specials = SpecialWeaponInventory::default();
        let mut armor = ArmorSet::default();
        let mut inventory = Inventory::default();
        inventory.add_item("health_pack", 2, 10);
        let mut quick = QuickItemSlot::default();
        let mut traversal = TraversalModeState::default();

        activate_loadout_entry(
            LoadoutTab::Weapons,
            4,
            &mut weapons,
            &mut specials,
            &mut armor,
            &inventory,
            &mut quick,
            &mut traversal,
        );
        activate_loadout_entry(
            LoadoutTab::Specials,
            1,
            &mut weapons,
            &mut specials,
            &mut armor,
            &inventory,
            &mut quick,
            &mut traversal,
        );
        activate_loadout_entry(
            LoadoutTab::Items,
            0,
            &mut weapons,
            &mut specials,
            &mut armor,
            &inventory,
            &mut quick,
            &mut traversal,
        );
        activate_loadout_entry(
            LoadoutTab::Rides,
            3,
            &mut weapons,
            &mut specials,
            &mut armor,
            &inventory,
            &mut quick,
            &mut traversal,
        );

        assert_eq!(weapons.active_slot, 4);
        assert_eq!(specials.active_slot, Some(0));
        assert_eq!(quick.item_id.as_deref(), Some("health_pack"));
        assert_eq!(traversal.active, TraversalMode::Hoverboard);
        assert_eq!(armor.active_element.display_name(), "None");
    }

    #[test]
    fn loadout_ride_selection_updates_suspended_vehicle_choice() {
        let mut vehicles = VehicleState::default();
        vehicles.active_owner = Some(0);
        vehicles.ground_mode = crate::plugins::vehicle_plugin::GroundMode::Tank;
        vehicles.select_traversal_while_active(0, TraversalMode::Grapple);
        let mut traversal = TraversalModeState {
            active: TraversalMode::Vehicle,
            ..default()
        };

        let label = activate_ride_entry(3, 0, &mut traversal, &mut vehicles);

        assert_eq!(label, "Rocket Hoverboard");
        assert_eq!(traversal.active, TraversalMode::Vehicle);
        assert_eq!(
            vehicles.persistent_traversal(0, traversal.active),
            TraversalMode::Hoverboard
        );
        let labels = loadout_entry_labels(
            LoadoutTab::Rides,
            &WeaponInventory::default(),
            &SpecialWeaponInventory::default(),
            &ArmorSet::default(),
            &Inventory::default(),
            &QuickItemSlot::default(),
            vehicles.persistent_traversal(0, traversal.active),
            &UpgradeLedger::default(),
        );
        assert!(labels[3].starts_with('✓'));
    }

    #[test]
    fn reticle_centers_match_split_screen_viewports() {
        assert_eq!(reticle_center_percent(0, 1), (50.0, 50.0));
        assert_eq!(reticle_center_percent(0, 2), (50.0, 25.0));
        assert_eq!(reticle_center_percent(1, 2), (50.0, 75.0));
        assert_eq!(reticle_center_percent(0, 4), (25.0, 25.0));
        assert_eq!(reticle_center_percent(3, 4), (75.0, 75.0));
    }

    #[test]
    fn chapter_loading_overlay_only_shows_for_a_pending_play_transition() {
        for next_state in [
            NextState::Unchanged,
            NextState::Pending(AppState::MainMenu),
            NextState::PendingIfNeq(AppState::CharacterDesign),
        ] {
            assert_eq!(
                chapter_loading_overlay_display(&next_state),
                Display::None,
                "non-gameplay navigation must leave the overlay hidden"
            );
        }

        assert_eq!(
            chapter_loading_overlay_display(&NextState::Pending(AppState::Playing)),
            Display::Flex
        );
        assert_eq!(
            chapter_loading_overlay_display(&NextState::PendingIfNeq(AppState::Playing)),
            Display::Flex
        );
    }

    #[test]
    fn four_player_hud_panels_anchor_inside_their_own_quadrants() {
        let size = Vec2::new(1280.0, 720.0);
        let p1 = hud_panel_placement(0, 4, size, 0.025);
        let p2 = hud_panel_placement(1, 4, size, 0.025);
        let p3 = hud_panel_placement(2, 4, size, 0.025);
        let p4 = hud_panel_placement(3, 4, size, 0.025);

        assert!(p1.left.is_some() && p1.top.is_some());
        assert!(p2.right.is_some() && p2.top.is_some());
        assert!(p3.left.is_some() && p3.bottom.is_some());
        assert!(p4.right.is_some() && p4.bottom.is_some());
        assert_eq!(p1.width, 232.0);
    }

    #[test]
    fn hud_safe_area_scales_and_stays_bounded() {
        let compact = hud_panel_placement(0, 1, Vec2::new(640.0, 360.0), 0.025);
        let television = hud_panel_placement(0, 1, Vec2::new(3840.0, 2160.0), 0.08);
        assert_eq!(compact.left, Some(9.0));
        assert_eq!(television.left, Some(48.0));
    }

    #[test]
    fn traversal_hud_reports_board_boost_readiness_and_active_state() {
        let traversal = TraversalModeState {
            active: TraversalMode::Hoverboard,
            ..default()
        };
        let mut boost = BoardBoostState::default();
        assert_eq!(
            traversal_status_text(&traversal, &boost, None, None, None),
            "BOARD: B/EAST BOOST READY"
        );

        boost.timer = 0.4;
        boost.manual_cooldown = 0.8;
        assert_eq!(
            traversal_status_text(&traversal, &boost, None, None, None),
            "BOARD: OVERDRIVE"
        );
    }

    #[test]
    fn the_control_readout_lists_every_action_and_prompts_while_arming() {
        use crate::engine::bindings::KeyAction;
        let mut settings = GameSettings::default();
        let idle = control_bindings_text(&settings, &PendingRebind::default());
        // Every rebindable action must be visible, or a player cannot tell
        // what is bound without pressing things at random.
        for action in KeyAction::ALL {
            assert!(idle.contains(action.label()), "{} missing", action.label());
        }
        assert!(idle.contains("Standard"), "layout is shown");
        assert!(!idle.contains("Press a key"), "no prompt when idle");

        // Arming shows the prompt for that action.
        let armed = PendingRebind(Some(KeyAction::Dodge));
        let prompt = control_bindings_text(&settings, &armed);
        assert!(prompt.contains("Press a key for Dodge"));

        // A rebind is reflected in the readout.
        settings
            .bindings
            .rebind(KeyAction::Dodge, KeyCode::KeyZ)
            .unwrap();
        assert!(control_bindings_text(&settings, &PendingRebind::default()).contains("Dodge: Z"));

        settings.bindings.face_layout = crate::engine::bindings::FaceLayout::Nintendo;
        settings.bindings.invert_look_y = true;
        let adjusted = control_bindings_text(&settings, &PendingRebind::default());
        assert!(adjusted.contains("Face layout: Nintendo"));
        assert!(adjusted.contains("Invert look Y: ON"));
        assert!(adjusted.contains("Button mapping:"));
    }

    #[test]
    fn charge_readout_fills_and_calls_out_the_release() {
        use crate::components::weapon::{Weapon, WeaponType};
        let mut weapon = Weapon::new(WeaponType::Pistol);

        // Idle: the line falls back to the ammo readout.
        weapon.charge_progress = 0.0;
        assert_eq!(charge_status_text(&weapon), "∞ AMMO");

        // Winding up: a filling bar with a percentage.
        weapon.charge_progress = 0.5;
        let mid = charge_status_text(&weapon);
        assert!(mid.contains("CHARGING"), "{mid}");
        assert!(mid.contains("50%"), "{mid}");
        assert_eq!(mid.matches('◆').count(), 3, "half of six segments");

        // Fuller charge shows more filled segments.
        weapon.charge_progress = 0.85;
        assert!(charge_status_text(&weapon).matches('◆').count() > 3);

        // Ready: an unmistakable prompt rather than "100%".
        weapon.charge_progress = 1.0;
        let ready = charge_status_text(&weapon);
        assert!(ready.contains("RELEASE"), "{ready}");
        assert!(!ready.contains('◇'), "a full bar has no empty segments");
    }

    #[test]
    fn board_hud_shows_the_live_trick_combo_over_boost_state() {
        let traversal = TraversalModeState {
            active: TraversalMode::Hoverboard,
            ..default()
        };
        // Overdrive would normally own the line…
        let boost = BoardBoostState {
            timer: 0.4,
            ..default()
        };
        let run = StuntRunState {
            active: true,
            pending_score: 1_250.0,
            multiplier: 2.5,
            ..default()
        };
        let tricks = TrickState {
            last_trick: Some("Kickflip".into()),
            ..default()
        };
        // …but an in-progress combo outranks it.
        assert_eq!(
            traversal_status_text(&traversal, &boost, Some(&tricks), Some(&run), None),
            "BOARD: KICKFLIP  1250  x2.50"
        );

        // A bail is called out immediately.
        let bailed = TrickState {
            bailed: true,
            ..default()
        };
        assert_eq!(
            traversal_status_text(&traversal, &boost, Some(&bailed), Some(&run), None),
            "BOARD: BAIL!"
        );

        // With no run in progress the line falls back to boost state.
        assert_eq!(
            traversal_status_text(
                &traversal,
                &boost,
                Some(&TrickState::default()),
                Some(&StuntRunState::default()),
                None,
            ),
            "BOARD: OVERDRIVE"
        );
    }

    #[test]
    fn rooftop_trial_timer_owns_the_traversal_hud_line() {
        let traversal = TraversalModeState::default();
        let boost = BoardBoostState::default();
        let trial = RooftopTrialProgress {
            active_course: Some("trial:test".into()),
            next_checkpoint: 2,
            checkpoint_count: 6,
            elapsed: 12.34,
            ..default()
        };

        assert_eq!(
            traversal_status_text(&traversal, &boost, None, None, Some(&trial)),
            "ROOFTOP: 12.3s  CP 3/6"
        );
    }

    #[test]
    fn sabre_hud_lists_owned_progression_and_live_technique() {
        let mut upgrades = UpgradeLedger::default();
        upgrades.unlock_relic("solar_sabre_glyph");
        upgrades.unlock_relic("cyclone_slash_blueprint");
        upgrades.unlock_relic("storm_gem");
        upgrades.unlock_relic("legendary_starheart_gem");
        let mut sabre = BeamSabre::default();

        let status = sabre_status_text(&sabre, &upgrades, &crate::combat::blades::STARTER_BLADE);
        assert!(status.contains("WAVE 3/SPIN"));
        assert!(status.contains("GEMS 1/4 + STARHEART"));

        sabre.technique = crate::components::weapon::SabreTechnique::CycloneSlash;
        sabre.technique_timer = 0.3;
        assert_eq!(
            sabre_status_text(&sabre, &upgrades, &crate::combat::blades::STARTER_BLADE),
            "SABER: CYCLONE"
        );
    }

    #[test]
    fn ui_theme_keeps_semantic_signals_and_player_accents_distinct() {
        let theme = UiTheme::default();
        assert_ne!(theme.play, theme.create);
        assert_ne!(theme.objective, theme.play);
        for left in 0..theme.players.len() {
            for right in left + 1..theme.players.len() {
                assert_ne!(theme.players[left], theme.players[right]);
            }
        }
        assert_eq!(theme.player_accent(99), theme.players[3]);
    }

    #[test]
    fn air_meter_escalates_from_cyan_to_amber_to_red() {
        assert_eq!(air_meter_color(1.0), Color::srgb(0.05, 0.90, 0.95));
        assert_eq!(air_meter_color(0.5), Color::srgb(1.0, 0.72, 0.12));
        assert_eq!(air_meter_color(0.25), Color::srgb(1.0, 0.28, 0.12));
    }
}
