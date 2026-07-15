use bevy::audio::{AudioPlayer, PlaybackSettings, Volume};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use bevy::input::gamepad::{GamepadButton, GamepadButtonStateChangedEvent};
use bevy::input::ButtonState;

use crate::chapters::{
    all_chapters, chapter_map_locations, map_settlements, ChapterId, MapSettlementKind,
    EVEREST_RANGE_HALF_EXTENT, EVEREST_RANGE_WORLD_SIZE, SECRET_CAVE_LOCATIONS,
};
use crate::commands::{CommandOverlayState, CommandRegistry};
use crate::components::armor::ArmorSet;
use crate::components::discoverable::{
    Discoverable, DiscoverableKind, PuzzleArchetype, PuzzleRelicEncounter,
};
use crate::components::enemy::CitySpyDrone;
use crate::components::inventory::Inventory;
use crate::components::player::{
    ClimbState, JetpackState, Player, PlayerIndex, PlayerInput, PlayerStats,
};
use crate::components::weapon::{
    BeamSabre, SpecialWeaponInventory, TrackingMissile, WeaponInventory, WeaponRanks, WeaponType,
    MAX_WEAPON_RANK,
};
use crate::components::world::{BoatVehicle, DiscussionNpc, DungeonCrawlGate, WorldLoot};
use crate::damage::Health;
use crate::discussion::DiscussionState;
use crate::events::*;
use crate::perks::{all_perks, PerkTree};
use crate::physics::prelude::{Physics, PhysicsTime};
use crate::plugins::crafting_plugin::{all_recipes, start_craft, CraftingQueue};
use crate::plugins::input_plugin::{NativeButton, NativeControllerState};
use crate::plugins::save_plugin::{save_current_session, SaveParams};
use crate::rendering::Camera3dBundle;
use crate::resources::{
    ChapterProgress, CharacterDesignData, CharacterDesignReturnTarget, CurrentChapter,
    FastTravelDestination, GameSettings, LocalPlayerConfig, PlaySessionTransition, PlayerGuidance,
    PlayerSelectState, ShopCatalog, ShopCategory, UiGameplayCapture, UiMessage, WaveInfo,
    WorldSiteRegistry, HERO_ROSTER,
};
use crate::robot_pets::{RobotPartKind, RobotPetCollection};
use crate::settlement_economy::SettlementEconomy;
use crate::state::AppState;
use crate::upgrades::{all_tech_upgrades, format_part_costs, TechUpgradeId, UpgradeLedger};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiMessage>()
            .init_resource::<PlayerGuidance>()
            .init_resource::<CraftingPanelState>()
            .init_resource::<DiscussionState>()
            .init_resource::<PauseMenuState>()
            .init_resource::<MenuFocus>()
            .add_systems(Startup, spawn_menu_camera)
            .add_systems(
                Update,
                (
                    register_menu_buttons,
                    menu_focus_navigation,
                    keep_menu_focus_in_view,
                    menu_back_navigation,
                    menu_focus_style,
                )
                    .chain()
                    .run_if(in_menu_state),
            )
            .add_systems(
                OnEnter(AppState::MainMenu),
                (cleanup_play_ui_for_menu, setup_main_menu),
            )
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
            .add_systems(Update, victory_input.run_if(in_state(AppState::Victory)))
            .add_systems(
                Update,
                settings_panel_input_system.run_if(in_state(AppState::Paused)),
            )
            .add_systems(
                Update,
                pause_input_system
                    .run_if(in_state(AppState::Playing).or_else(in_state(AppState::Paused))),
            )
            .add_systems(
                Update,
                (pause_menu_action_system, update_pause_menu_page_visibility)
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
                hud_update_system
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
                crafting_panel_system
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
            .add_systems(Update, game_over_input.run_if(in_state(AppState::GameOver)))
            .add_systems(
                Update,
                menu_start_button.run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                player_select_update.run_if(in_state(AppState::PlayerSelect)),
            )
            .add_systems(
                Update,
                (
                    chapter_select_action_buttons,
                    chapter_select_fast_travel_buttons,
                    cave_fast_travel_buttons,
                    chapter_select_perk_buttons,
                    chapter_select_upgrade_buttons,
                    chapter_select_weapon_rank_buttons,
                    chapter_select_controller_scroll,
                    chapter_select_perk_panel_update,
                    chapter_select_upgrade_panel_update,
                    chapter_select_weapon_rank_panel_update,
                    chapter_select_economy_panel_update,
                    final_push_unlock_system,
                )
                    .run_if(in_state(AppState::ChapterSelect)),
            );
    }
}

// ── Marker Components ─────────────────────────────────────────────────────────
#[derive(Component)]
struct MenuCamera;
#[derive(Component)]
struct MainMenuRoot;
#[derive(Component)]
struct HudRoot;
#[derive(Component)]
struct PauseRoot;
#[derive(Component, Clone, Copy)]
struct PauseMenuButton(PauseAction);
#[derive(Clone, Copy)]
enum PauseAction {
    Resume,
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
    Controls,
    Shop,
    Settings,
}
#[derive(Resource)]
struct PauseMenuState {
    page: PausePage,
    resume_lockout: f32,
    resume_armed: bool,
}
impl Default for PauseMenuState {
    fn default() -> Self {
        Self {
            page: PausePage::Main,
            resume_lockout: 0.0,
            resume_armed: true,
        }
    }
}
#[derive(Component)]
struct GameOverRoot;
#[derive(Component)]
struct VictoryRoot;
#[derive(Component)]
struct SettingsPanelRoot;
#[derive(Component)]
struct DifficultyText;
#[derive(Component)]
struct VolumeText;
#[derive(Component)]
struct FinalPushText;
#[derive(Component)]
struct StartButton;

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
pub(crate) struct MenuButtonDisabled;

#[derive(Component, Clone)]
struct MenuButtonPalette {
    background: BackgroundColor,
    border: BorderColor,
}

#[derive(Resource, Default)]
struct MenuFocus {
    entity: Option<Entity>,
    state: Option<AppState>,
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
}
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlayerHudBarKind {
    Health,
    Armor,
    Stamina,
    Jetpack,
    Climb,
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
#[derive(Component)]
struct CraftingPanelRoot;
#[derive(Component)]
struct CraftingPanelText;
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
    recipe_index: [usize; 4],
    repeat_direction: i8,
    repeat_timer: f32,
}

fn in_menu_state(state: Res<State<AppState>>) -> bool {
    // CharacterStudio owns a specialized row/field navigator for sliders and
    // viewport controls; do not double-dispatch its controller input here.
    !matches!(state.get(), AppState::Playing | AppState::CharacterStudio)
}

fn register_menu_buttons(
    mut commands: Commands,
    buttons: Query<
        (Entity, &BackgroundColor, &BorderColor),
        (Added<Button>, Without<MenuFocusable>),
    >,
) {
    for (entity, background, border) in buttons.iter() {
        commands.entity(entity).insert((
            MenuFocusable,
            MenuButtonPalette {
                background: *background,
                border: *border,
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
            Self::Up => Vec2::Y,
            Self::Down => Vec2::NEG_Y,
            Self::Left => Vec2::NEG_X,
            Self::Right => Vec2::X,
        }
    }
}

fn menu_focus_navigation(
    time: Res<Time>,
    state: Res<State<AppState>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    mut focus: ResMut<MenuFocus>,
    mut button_set: ParamSet<(
        Query<
            (
                Entity,
                &GlobalTransform,
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
    if focus.state.as_ref() != Some(&current_state) {
        focus.state = Some(current_state);
        focus.entity = None;
    }

    let direction_input = menu_direction_input(&keyboard, &gamepads, &native);
    let direction = repeated_menu_direction(&mut focus, direction_input, time.delta_secs());
    let confirm = keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::NumpadEnter)
        || keyboard.just_pressed(KeyCode::Space)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::South))
        || native.just_pressed(NativeButton::South);

    let mut visible = Vec::new();
    let mut hovered = None;
    for (entity, transform, interaction, inherited_visibility, disabled) in button_set.p0().iter() {
        if !inherited_visibility.get() || disabled.is_some() {
            continue;
        }
        let translation = transform.translation();
        visible.push((entity, Vec2::new(translation.x, translation.y)));
        if matches!(interaction, Interaction::Hovered | Interaction::Pressed) {
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

    if confirm {
        if let Some(entity) = focus.entity {
            if let Ok(mut interaction) = button_set.p1().get_mut(entity) {
                *interaction = Interaction::Pressed;
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct MenuDirectionInput {
    just: Option<MenuDirection>,
    held: Option<MenuDirection>,
}

fn menu_direction_input(
    keyboard: &ButtonInput<KeyCode>,
    gamepads: &Query<&Gamepad>,
    native: &NativeControllerState,
) -> MenuDirectionInput {
    let gamepad_just = |button| gamepads.iter().any(|gamepad| gamepad.just_pressed(button));
    let gamepad_pressed = |button| gamepads.iter().any(|gamepad| gamepad.pressed(button));
    let stick = gamepads
        .iter()
        .map(|gamepad| {
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
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    state: Res<State<AppState>>,
    design_data: Res<CharacterDesignData>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut transition: ResMut<PlaySessionTransition>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
    victory_q: Query<Entity, With<VictoryRoot>>,
) {
    let back = keyboard.just_pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::East))
        || native.just_pressed(NativeButton::East);
    if !back {
        return;
    }

    match state.get() {
        AppState::MainMenu | AppState::Playing | AppState::CharacterStudio => {}
        AppState::PlayerSelect => next_state.set(AppState::MainMenu),
        AppState::CharacterDesign => next_state.set(match design_data.return_target {
            CharacterDesignReturnTarget::PlayerSelect => AppState::PlayerSelect,
            CharacterDesignReturnTarget::ChapterSelect => AppState::ChapterSelect,
        }),
        AppState::ChapterSelect => next_state.set(AppState::PlayerSelect),
        AppState::RobotGarage => next_state.set(AppState::ChapterSelect),
        AppState::Paused if pause_menu.page != PausePage::Main => {
            pause_menu.page = PausePage::Main;
        }
        AppState::Paused => {
            transition.pausing = false;
            transition.resuming_from_pause = true;
            next_state.set(AppState::Playing);
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

fn default_menu_focus(buttons: &[(Entity, Vec2)]) -> Option<Entity> {
    buttons
        .iter()
        .min_by(|(_, a), (_, b)| b.y.total_cmp(&a.y).then_with(|| a.x.total_cmp(&b.x)))
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
    mut buttons: Query<
        (
            Entity,
            &Interaction,
            &MenuButtonPalette,
            Option<&MenuButtonDisabled>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        (With<Button>, With<MenuFocusable>),
    >,
) {
    for (entity, interaction, palette, disabled, mut background, mut border) in buttons.iter_mut() {
        if disabled.is_some() {
            *background = BackgroundColor(Color::srgb(0.07, 0.08, 0.11));
            *border = BorderColor::all(Color::srgb(0.28, 0.30, 0.36));
        } else if *interaction == Interaction::Pressed {
            *background = BackgroundColor(Color::srgb(0.98, 0.62, 0.14));
            *border = BorderColor::all(Color::WHITE);
        } else if focus.entity == Some(entity) || *interaction == Interaction::Hovered {
            *background = BackgroundColor(Color::srgb(0.12, 0.48, 0.82));
            *border = BorderColor::all(Color::srgb(1.0, 0.9, 0.34));
        } else {
            *background = palette.background;
            *border = palette.border;
        }
    }
}

fn keep_menu_focus_in_view(
    focus: Res<MenuFocus>,
    focused_q: Query<(&GlobalTransform, &ComputedNode), With<MenuFocusable>>,
    mut panel_q: Query<
        (&GlobalTransform, &ComputedNode, &mut ScrollPosition),
        With<MenuScrollPanel>,
    >,
) {
    let Some(entity) = focus.entity else {
        return;
    };
    let Ok((button_transform, button_node)) = focused_q.get(entity) else {
        return;
    };

    for (panel_transform, panel_node, mut scroll) in panel_q.iter_mut() {
        let max_y = menu_scroll_max(panel_node);
        if max_y <= 0.0 {
            continue;
        }
        let panel_center = panel_transform.translation().y;
        let button_center = button_transform.translation().y;
        let panel_half = panel_node.size().y * 0.5;
        let button_half = button_node.size().y * 0.5;
        let margin = 18.0;
        let panel_min = panel_center - panel_half + margin;
        let panel_max = panel_center + panel_half - margin;
        let button_min = button_center - button_half;
        let button_max = button_center + button_half;

        let delta = focus_reveal_scroll_delta(panel_min, panel_max, button_min, button_max);
        scroll.y = (scroll.y + delta).clamp(0.0, max_y);
    }
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
        viewport_min - item_min
    } else if item_max > viewport_max {
        viewport_max - item_max
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

fn setup_main_menu(mut commands: Commands) {
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
            BackgroundColor(Color::srgba(0.01, 0.012, 0.028, 1.0)),
            MainMenuRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("STARFALL I"),
                TextFont {
                    font_size: FontSize::Px(72.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.25)),
            ));
            p.spawn((
                Text::new("Everest Range"),
                TextFont {
                    font_size: FontSize::Px(24.0),
                    ..default()
                },
                TextColor(Color::srgb(0.65, 0.8, 1.0)),
            ));
            p.spawn(Node {
                height: Val::Px(24.0),
                ..default()
            });
            p.spawn((
                Button,
                Node {
                    width: Val::Px(260.0),
                    height: Val::Px(62.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.0, 0.36, 0.72)),
                BorderColor::all(Color::srgb(0.25, 0.72, 1.0)),
                StartButton,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("START GAME"),
                    TextFont {
                        font_size: FontSize::Px(24.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

fn menu_start_button(
    interaction_q: Query<&Interaction, (Changed<Interaction>, With<StartButton>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::PlayerSelect);
        }
    }
}

fn setup_pause_menu(
    mut commands: Commands,
    mut menu: ResMut<PauseMenuState>,
    shop: Res<ShopCatalog>,
    settings: Res<GameSettings>,
) {
    menu.page = PausePage::Main;
    menu.resume_lockout = 0.20;
    menu.resume_armed = false;
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(12.0),
                padding: UiRect::all(Val::Px(34.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
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
                        "Movement, aiming, star-beam combat, traversal tools, vehicle boarding, interaction, map use, weapon swaps, and special tools are available during play.\nTraversal supports wall slide, wall-jump, ledge hang, zip, swing, glide, hover, air dash, and hoverboard movement.",
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
                    Text::new("SHOP / GARAGE BROWSER"),
                    TextFont {
                        font_size: FontSize::Px(30.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.95, 1.0)),
                ));
                page.spawn((
                    Text::new("Browse base outfits, armor shells, weapons, and vehicle frames. Purchase/equip actions will wire into this catalog next."),
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.72, 0.82, 1.0)),
                ));
                for category in ShopCategory::ALL {
                    page.spawn((
                        Text::new(format!("{}:", category.label().to_uppercase())),
                        TextFont {
                            font_size: FontSize::Px(16.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.94, 0.76, 0.36)),
                    ));
                    for item in shop.items_for(category).take(4) {
                        page.spawn((
                            Text::new(format_shop_item_row(item)),
                            TextFont {
                                font_size: FontSize::Px(14.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.86, 0.90, 1.0)),
                        ));
                    }
                }
                spawn_pause_button(page, "BACK", PauseAction::Back, Color::srgb(0.0, 0.42, 0.74));
            });

            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                Visibility::Hidden,
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

fn format_shop_item_row(item: &crate::resources::ShopItem) -> String {
    let preview = item
        .preview_loadout
        .map(|loadout| {
            format!(
                " preview: {} / {} / {} / {}",
                loadout.arms.label(),
                loadout.legs.label(),
                loadout.shoulders.label(),
                loadout.head.label()
            )
        })
        .unwrap_or_default();
    format!(
        "{}  {} cr | {}{}",
        item.name, item.price_credits, item.summary, preview
    )
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
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    input_q: Query<&PlayerInput, With<Player>>,
    state: Res<State<AppState>>,
    mut menu: ResMut<PauseMenuState>,
    mut transition: ResMut<PlaySessionTransition>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    // Select + Start belongs to the level-editor workspace. Do not also open
    // the pause menu on the frame that chord changes tool mode.
    let editor_chord = gamepads.iter().any(|gamepad| {
        gamepad.pressed(GamepadButton::Select) && gamepad.just_pressed(GamepadButton::Start)
    }) || (native.pressed(NativeButton::Select)
        && native.just_pressed(NativeButton::Start));
    if editor_chord {
        return;
    }
    let raw_pause_pressed = keyboard.just_pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::Start))
        || native.just_pressed(NativeButton::Start)
        || button_events.read().any(|event| {
            event.state == ButtonState::Pressed && event.button == GamepadButton::Start
        });
    let pause_held = keyboard.pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.pressed(GamepadButton::Start))
        || native.pressed(NativeButton::Start);
    let player_pause_pressed = input_q.iter().any(|input| input.pause);

    if matches!(state.get(), AppState::Paused) {
        menu.resume_lockout = (menu.resume_lockout - time.delta_secs()).max(0.0);
        if !pause_held && menu.resume_lockout <= 0.0 {
            menu.resume_armed = true;
        }
    }

    match state.get() {
        AppState::Playing => {
            if !(raw_pause_pressed || player_pause_pressed) {
                return;
            }
            transition.pausing = true;
            transition.resuming_from_pause = false;
            menu.resume_lockout = 0.20;
            menu.resume_armed = false;
            next_state.set(AppState::Paused);
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
struct ChapterFastTravelButton(ChapterId);
#[derive(Component, Clone, Copy)]
struct CaveFastTravelButton {
    chapter: ChapterId,
    anchor_id: &'static str,
    label: &'static str,
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
// Legacy single-block text — kept so old queries don't break; no longer spawned.
#[derive(Component)]
struct PerkPanelText;
#[derive(Component)]
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
    perks: Res<PerkTree>,
    upgrades: Res<UpgradeLedger>,
    robot_pets: Res<RobotPetCollection>,
    weapon_ranks: Res<WeaponRanks>,
    world_site_registry: Res<WorldSiteRegistry>,
    economy: Res<SettlementEconomy>,
) {
    let chapters = all_chapters();
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
                spawn_fast_travel_map(row, &chapters, &progress, &world_site_registry);
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
                        let unlocked = progress.is_unlocked(ch.id);
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
                            button.spawn((
                                Text::new(format!(
                                    "{} START CH.{:02} - {} / {}",
                                    prefix, ch.id.0, ch.title, region
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
                    Text::new(format_perk_header(&perks)),
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
                                Text::new(format_perk_row(def, &perks)),
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
                    Text::new(format_upgrade_header(&upgrades)),
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
                            Text::new(format_upgrade_row(&def, &upgrades, &robot_pets)),
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
                                &weapon_ranks,
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
}

fn despawn_chapter_select(mut commands: Commands, q: Query<Entity, With<ChapterSelectRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn();
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

fn spawn_fast_travel_map(
    parent: &mut ChildSpawnerCommands,
    chapters: &[crate::chapters::ChapterDef],
    progress: &ChapterProgress,
    world_site_registry: &WorldSiteRegistry,
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

            for location in chapter_map_locations() {
                let Some(chapter) = chapters.iter().find(|chapter| chapter.id == location.id)
                else {
                    continue;
                };
                let unlocked = progress.is_unlocked(location.id);
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
                    Text::new(format!("{} {}", location.id.0, chapter.title)),
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

            // Discovered caves become direct checkpoints. Locked silhouettes
            // still advertise that each mountain region contains a dungeon.
            for cave in SECRET_CAVE_LOCATIONS {
                let discovered = progress.has_discoverable(cave.anchor_id);
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
                    BackgroundColor(if discovered {
                        Color::srgb(0.42, 0.18, 0.62)
                    } else {
                        Color::srgb(0.10, 0.10, 0.14)
                    }),
                    BorderColor::all(if discovered {
                        Color::srgb(0.82, 0.52, 1.0)
                    } else {
                        Color::srgb(0.28, 0.28, 0.34)
                    }),
                    CaveFastTravelButton {
                        chapter: cave.chapter,
                        anchor_id: cave.anchor_id,
                        label: cave.label,
                    },
                ));
                if !discovered {
                    cave_button.insert(MenuButtonDisabled);
                }
                cave_button.with_children(|marker| {
                    marker.spawn((
                        Text::new(if discovered { "C" } else { "?" }),
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
                    Text::new(settlement.name),
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

fn chapter_select_fast_travel_buttons(
    progress: Res<ChapterProgress>,
    mut current: ResMut<CurrentChapter>,
    mut next_state: ResMut<NextState<AppState>>,
    interaction_q: Query<
        (&Interaction, &ChapterFastTravelButton),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction == Interaction::Pressed && progress.is_unlocked(button.0) {
            current.id = button.0;
            current.started = false;
            next_state.set(AppState::Playing);
        }
    }
}

fn cave_fast_travel_buttons(
    progress: Res<ChapterProgress>,
    mut current: ResMut<CurrentChapter>,
    mut destination: ResMut<FastTravelDestination>,
    mut next_state: ResMut<NextState<AppState>>,
    interaction_q: Query<
        (&Interaction, &CaveFastTravelButton),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed || !progress.has_discoverable(button.anchor_id) {
            continue;
        }
        current.id = button.chapter;
        current.started = false;
        destination.cave(button.anchor_id, button.label);
        next_state.set(AppState::Playing);
    }
}

fn chapter_select_controller_scroll(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
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

    let down = keyboard.pressed(KeyCode::ArrowDown)
        || keyboard.pressed(KeyCode::KeyS)
        || gamepads.iter().any(|gamepad| {
            gamepad.pressed(GamepadButton::DPadDown)
                || gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0) < -0.6
        })
        || native.pressed(NativeButton::DPadDown)
        || native.move_axis.y < -0.6;
    let up = keyboard.pressed(KeyCode::ArrowUp)
        || keyboard.pressed(KeyCode::KeyW)
        || gamepads.iter().any(|gamepad| {
            gamepad.pressed(GamepadButton::DPadUp)
                || gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0) > 0.6
        })
        || native.pressed(NativeButton::DPadUp)
        || native.move_axis.y > 0.6;

    if down {
        delta_y += 22.0;
    } else if up {
        delta_y -= 22.0;
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
    mut perks: ResMut<PerkTree>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            perks.try_spend(button.0);
        }
    }
}

fn chapter_select_perk_panel_update(
    perks: Res<PerkTree>,
    mut header_q: Query<&mut Text, (With<PerkPointsHeader>, Without<PerkRowText>)>,
    mut row_q: Query<(&PerkRowText, &mut Text), Without<PerkPointsHeader>>,
) {
    if !perks.is_changed() {
        return;
    }
    for mut text in header_q.iter_mut() {
        *text = Text::new(format_perk_header(&perks));
    }
    let defs = all_perks();
    for (row, mut text) in row_q.iter_mut() {
        if let Some(def) = defs.iter().find(|d| d.id == row.0) {
            *text = Text::new(format_perk_row(def, &perks));
        }
    }
}

fn chapter_select_upgrade_buttons(
    interaction_q: Query<
        (&Interaction, &ChapterUpgradeButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut upgrades: ResMut<UpgradeLedger>,
    mut robot_pets: ResMut<RobotPetCollection>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            let _ = upgrades.try_purchase(button.0, &mut robot_pets);
        }
    }
}

fn chapter_select_upgrade_panel_update(
    upgrades: Res<UpgradeLedger>,
    robot_pets: Res<RobotPetCollection>,
    mut header_q: Query<&mut Text, (With<UpgradeReserveHeader>, Without<UpgradeRowText>)>,
    mut row_q: Query<(&UpgradeRowText, &mut Text), Without<UpgradeReserveHeader>>,
) {
    if !(upgrades.is_changed() || robot_pets.is_changed()) {
        return;
    }
    for mut text in header_q.iter_mut() {
        *text = Text::new(format_upgrade_header(&upgrades));
    }
    let defs = all_tech_upgrades();
    for (row, mut text) in row_q.iter_mut() {
        if let Some(def) = defs.iter().find(|d| d.id == row.0) {
            *text = Text::new(format_upgrade_row(def, &upgrades, &robot_pets));
        }
    }
}

fn chapter_select_weapon_rank_buttons(
    interaction_q: Query<
        (&Interaction, &ChapterWeaponRankButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut weapon_ranks: ResMut<WeaponRanks>,
    mut robot_pets: ResMut<RobotPetCollection>,
) {
    for (interaction, button) in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            purchase_weapon_rank(button.0, &mut weapon_ranks, &mut robot_pets);
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
    let recipe = [crate::robot_pets::PartCost::new(
        RobotPartKind::CircuitBoard,
        cost,
    )];
    if robot_pets.can_afford(&recipe) {
        let _ = robot_pets.spend_parts(&recipe);
        *rank += 1;
    }
}

fn chapter_select_weapon_rank_panel_update(
    weapon_ranks: Res<WeaponRanks>,
    robot_pets: Res<RobotPetCollection>,
    mut header_q: Query<&mut Text, (With<WeaponRankHeader>, Without<WeaponRankRowText>)>,
    mut row_q: Query<(&WeaponRankRowText, &mut Text), Without<WeaponRankHeader>>,
) {
    if !(weapon_ranks.is_changed() || robot_pets.is_changed()) {
        return;
    }
    for mut text in header_q.iter_mut() {
        *text = Text::new(format_weapon_rank_header(&robot_pets));
    }
    for (row, mut text) in row_q.iter_mut() {
        let slot = row.0;
        let weapon_type = WEAPON_RANK_WEAPONS[slot];
        *text = Text::new(format_weapon_rank_row(
            slot,
            weapon_type,
            &weapon_ranks,
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

fn format_perk_row(def: &crate::perks::PerkDef, perks: &PerkTree) -> String {
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

fn branch_color(branch: crate::perks::PerkBranch) -> Color {
    match branch {
        crate::perks::PerkBranch::Heart => Color::srgb(1.0, 0.55, 0.60),
        crate::perks::PerkBranch::Star => Color::srgb(1.0, 0.92, 0.42),
        crate::perks::PerkBranch::Acrobat => Color::srgb(0.42, 0.88, 1.0),
    }
}

fn format_upgrade_header(upgrades: &UpgradeLedger) -> String {
    format!(
        "TECH UPGRADES   rejuvenation reserve: {:.0}",
        upgrades.rejuvenation_charge
    )
}

fn format_upgrade_row(
    def: &crate::upgrades::TechUpgradeDef,
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

fn track_color(track: crate::upgrades::TechUpgradeTrack) -> Color {
    match track {
        crate::upgrades::TechUpgradeTrack::Weapons => Color::srgb(1.0, 0.65, 0.35),
        crate::upgrades::TechUpgradeTrack::Missiles => Color::srgb(1.0, 0.45, 0.45),
        crate::upgrades::TechUpgradeTrack::Turrets => Color::srgb(0.80, 0.55, 1.0),
        crate::upgrades::TechUpgradeTrack::Health => Color::srgb(0.42, 1.0, 0.55),
        crate::upgrades::TechUpgradeTrack::Mechs => Color::srgb(0.42, 0.88, 1.0),
        crate::upgrades::TechUpgradeTrack::Mobility => Color::srgb(0.55, 1.0, 0.75),
        crate::upgrades::TechUpgradeTrack::Armor => Color::srgb(0.70, 0.70, 0.85),
        crate::upgrades::TechUpgradeTrack::Gauntlets => Color::srgb(1.0, 0.75, 0.45),
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
        let recipe = [crate::robot_pets::PartCost::new(
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
    JoinLeave,
    Back,
    Begin,
}

fn slot_label_color(i: u8) -> Color {
    match i {
        0 => Color::srgb(0.3, 0.8, 1.0),
        1 => Color::srgb(0.8, 0.3, 1.0),
        2 => Color::srgb(0.3, 1.0, 0.5),
        _ => Color::srgb(1.0, 0.65, 0.2),
    }
}

fn setup_player_select(mut commands: Commands, mut select: ResMut<PlayerSelectState>) {
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
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(28.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.01, 0.05, 1.0)),
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
                            TextColor(slot_label_color(i)),
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

fn player_select_update(
    interaction_q: Query<(&Interaction, &PlayerSelectButton), (Changed<Interaction>, With<Button>)>,
    mut select: ResMut<PlayerSelectState>,
    mut config: ResMut<LocalPlayerConfig>,
    mut design_data: ResMut<CharacterDesignData>,
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

    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
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
        if s.joined {
            *t = Text::new(format!("PLAYER {}", i + 1));
        } else {
            *t = Text::new("OPEN SLOT");
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

// ── HUD Setup ─────────────────────────────────────────────────────────────────
fn setup_hud(
    mut commands: Commands,
    config: Res<LocalPlayerConfig>,
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
                spawn_player_hud_panel(root, player_index);
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

            // ── Crosshair ─────────────────────────────────────────────────────────
            root.spawn(Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                margin: UiRect::all(Val::Px(-8.0)),
                width: Val::Px(16.0),
                height: Val::Px(16.0),
                ..default()
            })
            .with_children(|ch| {
                ch.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(7.0),
                        width: Val::Px(16.0),
                        height: Val::Px(2.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
                ));
                ch.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(7.0),
                        top: Val::Px(0.0),
                        width: Val::Px(2.0),
                        height: Val::Px(16.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
                ));
            });

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
                    Text::new(
                        "CRAFTING — D-PAD / STICK: choose   A: craft   SELECT: close   Keyboard: 1-5",
                    ),
                    TextFont {
                        font_size: FontSize::Px(15.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.0, 0.9, 1.0)),
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

fn spawn_player_hud_panel(parent: &mut ChildSpawnerCommands, player_index: u8) {
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(16.0 + f32::from(player_index) * 144.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                width: Val::Px(246.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.05, 0.10, 0.78)),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(format!("P{}", player_index + 1)),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(slot_label_color(player_index)),
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
                Color::srgb(0.2, 0.8, 0.2),
            );
            spawn_bar(
                panel,
                player_index,
                "AR",
                PlayerHudBarKind::Armor,
                Color::srgb(0.2, 0.5, 1.0),
            );
            spawn_bar(
                panel,
                player_index,
                "ST",
                PlayerHudBarKind::Stamina,
                Color::srgb(0.9, 0.7, 0.0),
            );
            spawn_bar(
                panel,
                player_index,
                "JP",
                PlayerHudBarKind::Jetpack,
                Color::srgb(0.0, 0.9, 0.9),
            );
            spawn_bar(
                panel,
                player_index,
                "CL",
                PlayerHudBarKind::Climb,
                Color::srgb(0.95, 0.55, 0.15),
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
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        })
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
        ),
        With<Player>,
    >,
    wave: Res<WaveInfo>,
    current: Res<CurrentChapter>,
    missile_q: Query<&TrackingMissile>,
    mut bar_q: Query<(&mut Node, &PlayerHudBar)>,
    mut text_sets: ParamSet<(
        Query<(&mut Text, &PlayerHudText)>,
        Query<&mut Text, With<WaveText>>,
        Query<&mut Text, With<EnemyCountText>>,
    )>,
) {
    for (index, health, stats, jetpack, climb, weapons, special, armor, sabre) in player_q.iter() {
        for (mut node, bar) in bar_q
            .iter_mut()
            .filter(|(_, bar)| bar.player_index == index.0)
        {
            let percent = match bar.kind {
                PlayerHudBarKind::Health => health.current / health.max,
                PlayerHudBarKind::Armor => stats.armor / stats.max_armor,
                PlayerHudBarKind::Stamina => stats.stamina / stats.max_stamina,
                PlayerHudBarKind::Jetpack => jetpack.fuel / jetpack.max_fuel,
                PlayerHudBarKind::Climb => climb.energy / climb.max_energy,
            };
            node.width = Val::Percent((percent * 100.0).clamp(0.0, 100.0));
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
                PlayerHudTextKind::Ammo => "∞ AMMO".to_string(),
                PlayerHudTextKind::SpecialAmmo => format!("SPECIALS ∞{}", lock_label),
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
    player_q: Query<(&PlayerIndex, &Transform), With<Player>>,
    npc_q: Query<(&Transform, &DiscussionNpc)>,
    gate_q: Query<&DungeonCrawlGate>,
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

        for gate in gate_q.iter() {
            if let Some((player_index, distance)) = nearest_player_to(&player_q, gate.entry) {
                if distance <= gate.interact_radius + 4.0 {
                    offer_guidance(
                        &mut best,
                        1.0,
                        distance,
                        format!("Open {}", gate.label),
                        format!(
                            "P{} can gather the party for a top-down dungeon.",
                            player_index + 1
                        ),
                        "E / D-pad Down: Open gate",
                    );
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
    mut commands: Commands,
    mut discussion: ResMut<DiscussionState>,
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
        *text = Text::new(line.text);
    }
    if let Ok(mut text) = text_sets.p3().single_mut() {
        *text = Text::new(format!(
            "Interact / Enter / Space: next   Line {}/{}   Voice: {}",
            discussion.line_index + 1,
            script.lines.len(),
            line.voice_mp3.unwrap_or("none")
        ));
    }

    let owner_interact = player_input_q
        .iter()
        .any(|(idx, input)| idx.0 == discussion.player_index && input.interact);
    let keyboard_next = keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::NumpadEnter)
        || keyboard.just_pressed(KeyCode::Space)
        || keyboard.just_pressed(KeyCode::KeyE);

    if discussion.input_cooldown <= 0.0 && (owner_interact || keyboard_next) {
        discussion.advance();
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
fn crafting_panel_system(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut panel_state: ResMut<CraftingPanelState>,
    mut input_capture: ResMut<UiGameplayCapture>,
    mut panel_q: Query<&mut Node, With<CraftingPanelRoot>>,
    mut text_q: Query<&mut Text, With<CraftingPanelText>>,
    player_input_q: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
    mut player_q: Query<(&PlayerIndex, &mut Inventory, &PlayerStats), With<Player>>,
    mut queue: ResMut<CraftingQueue>,
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

    let Some((_, mut inventory, stats)) = player_q.iter_mut().find(|(idx, _, _)| idx.0 == owner)
    else {
        panel_state.visible = false;
        panel_state.owner = None;
        input_capture.owner = None;
        return;
    };
    let recipes = all_recipes();
    let selection_slot = usize::from(owner).min(panel_state.recipe_index.len() - 1);
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

    // Build recipe display text
    let mut display = String::new();
    for (i, recipe) in recipes.iter().enumerate() {
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
        *t = Text::new(format!("P{} Crafting\n\n{}", owner + 1, display));
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
            match start_craft(recipe.id, owner, &mut inventory, stats, &mut queue) {
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
    menu: Res<PauseMenuState>,
    mut settings: ResMut<GameSettings>,
    interaction_q: Query<(&Interaction, &SettingsButton), (Changed<Interaction>, With<Button>)>,
    mut text_q: ParamSet<(
        Query<&mut Text, With<DifficultyText>>,
        Query<&mut Text, With<VolumeText>>,
    )>,
) {
    if menu.page != PausePage::Settings {
        return;
    }
    let mut changed = false;
    for (interaction, button) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button.0 {
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
        }
        changed = true;
    }
    if !changed {
        return;
    }
    let diff_str = difficulty_text(&settings);
    for mut text in text_q.p0().iter_mut() {
        *text = Text::new(diff_str.clone());
    }
    let vol_str = volume_text(&settings);
    for mut text in text_q.p1().iter_mut() {
        *text = Text::new(vol_str.clone());
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

// ── Final Push Unlock ─────────────────────────────────────────────────────────

fn final_push_unlock_system(
    mut commands: Commands,
    final_war: Res<crate::final_war::FinalWarRegistry>,
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
    let show = final_war.phase == crate::final_war::FinalWarPhase::Climax && liberated >= 6;
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
    if !keyboard.just_pressed(KeyCode::F8) {
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
    native: Res<NativeControllerState>,
    mut text_q: Query<&mut Text, With<ControllerDiagText>>,
) {
    if !state.visible {
        return;
    }
    let Ok(mut text) = text_q.single_mut() else {
        return;
    };

    let mut gps: Vec<(Entity, &Gamepad)> = gamepads.iter().collect();
    gps.sort_by_key(|(e, _)| e.index());

    let mut lines = Vec::with_capacity(8);
    lines.push("── Controller Diagnostics [F8] ──".to_string());

    for (idx, pi) in players
        .iter()
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<Vec<_>>()
    {
        let i = idx.0 as usize;
        let gp_info = if let Some((_, gp)) = gps.get(i) {
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
                "GP{}  L({:+.2},{:+.2}) R({:+.2},{:+.2})",
                i, ls.x, ls.y, rs.x, rs.y
            )
        } else if i == 0 && native.connected {
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
        if at_site.is_empty() {
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

    if command_registry.assets.is_empty() {
        lines.push("No command assets yet.".to_string());
    }

    *text = Text::new(lines.join("\n"));
}

#[cfg(test)]
mod menu_navigation_tests {
    use super::*;

    fn four_button_grid() -> (World, Vec<(Entity, Vec2)>) {
        let mut world = World::new();
        let top_left = world.spawn_empty().id();
        let top_right = world.spawn_empty().id();
        let bottom_left = world.spawn_empty().id();
        let bottom_right = world.spawn_empty().id();
        let buttons = vec![
            (top_left, Vec2::new(-10.0, 10.0)),
            (top_right, Vec2::new(10.0, 10.0)),
            (bottom_left, Vec2::new(-10.0, -10.0)),
            (bottom_right, Vec2::new(10.0, -10.0)),
        ];
        (world, buttons)
    }

    #[test]
    fn default_focus_starts_at_top_left() {
        let (_world, buttons) = four_button_grid();
        assert_eq!(default_menu_focus(&buttons), Some(buttons[0].0));
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
        assert_eq!(focus_reveal_scroll_delta(10.0, 90.0, -5.0, 5.0), 15.0);
        assert_eq!(focus_reveal_scroll_delta(10.0, 90.0, 95.0, 105.0), -15.0);
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
}
