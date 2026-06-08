use bevy::prelude::*;

use bevy::input::gamepad::{GamepadAxis, GamepadButton, GamepadButtonStateChangedEvent};
use bevy::input::ButtonState;
use bevy_rapier3d::prelude::RapierConfiguration;

use crate::chapters::{all_chapters, chapter_map_locations, ChapterId};
use crate::components::armor::ArmorSet;
use crate::components::discoverable::{PuzzleArchetype, PuzzleRelicEncounter};
use crate::components::inventory::Inventory;
use crate::components::player::{JetpackState, Player, PlayerIndex, PlayerInput, PlayerStats};
use crate::components::weapon::{BeamSabre, SpecialWeaponInventory, WeaponInventory};
use crate::damage::Health;
use crate::events::*;
use crate::perks::{all_perks, PerkTree};
use crate::plugins::crafting_plugin::{all_recipes, start_craft, CraftingQueue};
use crate::plugins::input_plugin::{NativeButton, NativeControllerState};
use crate::plugins::save_plugin::save_current_session;
use crate::rendering::Camera3dBundle;
use crate::resources::{
    ChapterProgress, CharacterDesignData, CurrentChapter, LocalPlayerConfig, PlaySessionTransition,
    PlayerSelectState, UiMessage, WaveInfo, HERO_ROSTER,
};
use crate::robot_pets::RobotPetCollection;
use crate::state::AppState;
use crate::upgrades::{all_tech_upgrades, format_part_costs, TechUpgradeId, UpgradeLedger};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiMessage>()
            .init_resource::<CraftingPanelState>()
            .init_resource::<PauseMenuState>()
            .add_systems(Startup, spawn_menu_camera)
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
            .add_systems(OnEnter(AppState::Playing), (setup_hud, despawn_menu))
            .add_systems(
                OnEnter(AppState::Paused),
                (setup_pause_menu, freeze_physics_on_pause),
            )
            .add_systems(
                OnExit(AppState::Paused),
                (despawn_pause_menu, resume_physics_after_pause),
            )
            .add_systems(OnEnter(AppState::GameOver), setup_game_over)
            .add_systems(
                Update,
                pause_input_system
                    .run_if(in_state(AppState::Playing).or(in_state(AppState::Paused))),
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
                hud_update_system
                    .run_if(in_state(AppState::Playing).or(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                puzzle_objective_hud_system
                    .run_if(in_state(AppState::Playing).or(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                message_timer_system.run_if(
                    in_state(AppState::Playing)
                        .or(in_state(AppState::Paused))
                        .or(in_state(AppState::GameOver)),
                ),
            )
            .add_systems(
                Update,
                ui_message_listener.run_if(
                    in_state(AppState::Playing)
                        .or(in_state(AppState::Paused))
                        .or(in_state(AppState::GameOver)),
                ),
            )
            .add_systems(
                Update,
                damage_vignette_system
                    .run_if(in_state(AppState::Playing).or(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                crafting_panel_system
                    .run_if(in_state(AppState::Playing).or(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                boss_alert_system
                    .run_if(in_state(AppState::Playing).or(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                game_over_input
                    .run_if(in_state(AppState::Playing).or(in_state(AppState::GameOver))),
            )
            .add_systems(
                Update,
                (menu_start_button, main_menu_controller_status_system)
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                player_select_update.run_if(in_state(AppState::PlayerSelect)),
            )
            .add_systems(
                Update,
                (
                    chapter_select_input,
                    chapter_select_fast_travel_buttons,
                    chapter_select_perk_input,
                    chapter_select_upgrade_input,
                    chapter_select_perk_panel_update,
                    chapter_select_upgrade_panel_update,
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
    Back,
}
#[derive(Component, Clone, Copy)]
struct PausePagePanel(PausePage);
#[derive(Clone, Copy, PartialEq, Eq)]
enum PausePage {
    Main,
    Controls,
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
struct StartButton;
#[derive(Component)]
struct ControllerStatusText;
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

// ── Crafting Panel State ──────────────────────────────────────────────────────
#[derive(Resource, Default)]
struct CraftingPanelState {
    visible: bool,
    owner: Option<u8>,
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
    commands.spawn((
        Node {
            width: Val::Percent(100.0), height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center, justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.01, 0.01, 0.05, 1.0)),
        MainMenuRoot,
    )).with_children(|p| {
        p.spawn((Text::new("STARFALL I"), TextFont { font_size: 72.0, ..default() }, TextColor(Color::srgb(1.0, 0.9, 0.25))));
        p.spawn((Text::new("Eight siblings, Scallarians, dragon royalty, and star-powered science."), TextFont { font_size: 24.0, ..default() }, TextColor(Color::srgb(0.65, 0.8, 1.0))));
        p.spawn(Node { height: Val::Px(40.0), ..default() });
        p.spawn((
            Button,
            Node { padding: UiRect::all(Val::Px(16.0)), ..default() },
            BackgroundColor(Color::srgb(0.0, 0.4, 0.8)),
            StartButton,
        )).with_children(|btn| {
            btn.spawn((Text::new("BEGIN CHAPTER"), TextFont { font_size: 28.0, ..default() }, TextColor(Color::WHITE)));
        });
        p.spawn(Node { height: Val::Px(30.0), ..default() });
        p.spawn((
            Text::new("Controller: scanning..."),
            TextFont { font_size: 16.0, ..default() },
            TextColor(Color::srgb(0.60, 0.82, 1.0)),
            ControllerStatusText,
        ));
        p.spawn(Node { height: Val::Px(16.0), ..default() });
        p.spawn((
            Text::new("WASD / Left Stick Move  |  Mouse / Right Stick Look  |  A/Space Jump  |  Start/Esc Pause\nLMB/RT Star Beam  |  V/B Mana Combos  |  T Star Sabre  |  E/DPad Down Interact"),
            TextFont { font_size: 14.0, ..default() }, TextColor(Color::srgb(0.5, 0.5, 0.7)),
        ));
    });
}

fn menu_start_button(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    interaction_q: Query<&Interaction, (Changed<Interaction>, With<StartButton>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::Space)
        || gamepads.iter().any(gamepad_start_or_confirm_just_pressed)
        || native.start_or_confirm_just_pressed()
        || button_events
            .read()
            .any(gamepad_button_event_is_start_or_confirm)
    {
        next_state.set(AppState::PlayerSelect);
        return;
    }

    for interaction in interaction_q.iter() {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::PlayerSelect);
        }
    }
}

fn main_menu_controller_status_system(
    gamepads: Query<(&Name, &Gamepad)>,
    native: Res<NativeControllerState>,
    mut text_q: Query<&mut Text, With<ControllerStatusText>>,
) {
    let Ok(mut text) = text_q.single_mut() else {
        return;
    };
    let mut connected = gamepads.iter();
    if let Some((name, gamepad)) = connected.next() {
        let lx = gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        let ly = gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
        *text = Text::new(format!(
            "Controller: {} detected   Left Stick X:{:+.2} Y:{:+.2}   Press A or Start",
            name.as_str(),
            lx,
            ly
        ));
    } else if native.connected {
        *text = Text::new(format!(
            "Controller: {} detected via macOS   Left Stick X:{:+.2} Y:{:+.2}   Press A or Start",
            native.name, native.move_axis.x, native.move_axis.y
        ));
    } else {
        *text = Text::new("Controller: not detected yet. Reconnect/power on the Xbox controller, then press A or Start.");
    }
}

fn gamepad_start_or_confirm_just_pressed(gamepad: &Gamepad) -> bool {
    gamepad.any_just_pressed([
        GamepadButton::Start,
        GamepadButton::South,
        GamepadButton::East,
        GamepadButton::North,
        GamepadButton::West,
    ])
}

fn gamepad_button_event_is_start_or_confirm(event: &GamepadButtonStateChangedEvent) -> bool {
    event.state == ButtonState::Pressed
        && matches!(
            event.button,
            GamepadButton::Start
                | GamepadButton::South
                | GamepadButton::East
                | GamepadButton::North
                | GamepadButton::West
        )
}

fn setup_pause_menu(mut commands: Commands, mut menu: ResMut<PauseMenuState>) {
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
                    font_size: 58.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.95, 1.0)),
            ));
            root.spawn((
                Text::new("Any active player can pause. Save and title actions affect the party."),
                TextFont {
                    font_size: 19.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.85, 1.0)),
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
                    (
                        "SAVE & RETURN TO TITLE",
                        PauseAction::Title,
                        Color::srgb(0.48, 0.18, 0.18),
                    ),
                    (
                        "CONTROLS / TIPS",
                        PauseAction::Controls,
                        Color::srgb(0.22, 0.24, 0.52),
                    ),
                ] {
                    spawn_pause_button(page, label, action, color);
                }
                page.spawn((
                    Text::new(
                        "Quick keys: [S/F5] Save   [T] Save & Title   [F9] Collider Debug",
                    ),
                    TextFont {
                        font_size: 15.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.86, 0.90, 1.0)),
                ));
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
                        font_size: 30.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.95, 1.0)),
                ));
                page.spawn((
                    Text::new(
                        "Move: WASD / Left Stick     Look: Mouse / Right Stick     Jump: Space / South\nDodge: Q / East     Parry: F / North     Interact: E / DPad Down     Boat/Vehicle: J / DPad Up\nFire: LMB / RT     Aim: RMB / LT     Weapons: 1-6 / RB     Special Tools: 7-0 or Select + DPad\nTraversal: hold into a wall while falling to slide, then jump to wall-jump. Use slingshot pads with Jump or Interact.",
                    ),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.72, 0.82, 1.0)),
                ));
                page.spawn((
                    Text::new(
                        "Boat: board near a dock, steer along the wake, and dock at the city or island before disembarking. Hidden rooms can contain puzzle props, rewards, armor, XP, and special ability upgrades.",
                    ),
                    TextFont {
                        font_size: 15.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.86, 0.90, 1.0)),
                ));
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
                width: Val::Px(320.0),
                height: Val::Px(44.0),
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
                    font_size: 21.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn update_pause_menu_page_visibility(
    menu: Res<PauseMenuState>,
    mut page_q: Query<(&PausePagePanel, &mut Visibility)>,
) {
    if !menu.is_changed() {
        return;
    }
    for (panel, mut visibility) in page_q.iter_mut() {
        *visibility = if panel.0 == menu.page {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn despawn_pause_menu(mut commands: Commands, q: Query<Entity, With<PauseRoot>>) {
    for entity in q.iter() {
        commands.entity(entity).despawn();
    }
}

fn freeze_physics_on_pause(mut rapier_q: Query<&mut RapierConfiguration>) {
    for mut rapier in rapier_q.iter_mut() {
        rapier.physics_pipeline_active = false;
    }
}

fn resume_physics_after_pause(mut rapier_q: Query<&mut RapierConfiguration>) {
    for mut rapier in rapier_q.iter_mut() {
        rapier.physics_pipeline_active = true;
    }
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
        AppState::Paused => {
            if !raw_pause_pressed || !menu.resume_armed {
                return;
            }
            transition.pausing = false;
            transition.resuming_from_pause = true;
            next_state.set(AppState::Playing);
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn pause_menu_action_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    native: Res<NativeControllerState>,
    button_q: Query<(&Interaction, &PauseMenuButton), (Changed<Interaction>, With<Button>)>,
    player_q: Query<(&PlayerIndex, &PlayerStats, &Health), With<Player>>,
    wave: Res<WaveInfo>,
    progress: Res<ChapterProgress>,
    perks: Res<PerkTree>,
    select: Res<PlayerSelectState>,
    robot_pets: Res<RobotPetCollection>,
    upgrades: Res<UpgradeLedger>,
    mut menu: ResMut<PauseMenuState>,
    mut transition: ResMut<PlaySessionTransition>,
    mut next_state: ResMut<NextState<AppState>>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    let mut action = None;
    if keyboard.just_pressed(KeyCode::KeyS)
        || keyboard.just_pressed(KeyCode::F5)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::Select))
        || native.just_pressed(NativeButton::Select)
    {
        action = Some(PauseAction::Save);
    }
    if keyboard.just_pressed(KeyCode::KeyT) {
        action = Some(PauseAction::Title);
    }
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
            send_pause_save_result(
                save_current_session(
                    &player_q,
                    &wave,
                    &progress,
                    &perks,
                    &select,
                    &robot_pets,
                    &upgrades,
                ),
                &mut msg_ev,
            );
        }
        PauseAction::Title => {
            send_pause_save_result(
                save_current_session(
                    &player_q,
                    &wave,
                    &progress,
                    &perks,
                    &select,
                    &robot_pets,
                    &upgrades,
                ),
                &mut msg_ev,
            );
            transition.pausing = false;
            transition.resuming_from_pause = false;
            next_state.set(AppState::MainMenu);
        }
        PauseAction::Controls => {
            menu.page = PausePage::Controls;
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
struct ChapterFastTravelButton(ChapterId);
#[derive(Component)]
struct PerkPanelText;
#[derive(Component)]
struct TechUpgradePanelText;

fn setup_chapter_select(
    mut commands: Commands,
    progress: Res<ChapterProgress>,
    perks: Res<PerkTree>,
    upgrades: Res<UpgradeLedger>,
    robot_pets: Res<RobotPetCollection>,
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
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.06, 1.0)),
            ChapterSelectRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("EVEREST RANGE FAST TRAVEL"),
                TextFont {
                    font_size: 40.0,
                    ..default()
                },
                TextColor(Color::srgb(0.4, 0.85, 1.0)),
            ));
            p.spawn((
                Text::new("Press 1-9 / 0 / Q W R T or click a marker   |   [E]ditor   [Esc] Back"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.85)),
            ));
            p.spawn(Node {
                width: Val::Percent(100.0),
                max_width: Val::Px(1160.0),
                height: Val::Px(330.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(16.0),
                ..default()
            })
            .with_children(|row| {
                spawn_fast_travel_map(row, &chapters, &progress);
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
                        list.spawn((
                            Text::new(format!(
                                "{} {} Ch.{:02} — {} / {}",
                                prefix,
                                chapter_key_hint(ch.id.0),
                                ch.id.0,
                                ch.title,
                                region
                            )),
                            TextFont {
                                font_size: 13.5,
                                ..default()
                            },
                            TextColor(color),
                        ));
                    }
                });
            });
            p.spawn(Node {
                height: Val::Px(4.0),
                ..default()
            });
            p.spawn((
                Text::new(format_perk_panel(&perks)),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.9, 1.0)),
                PerkPanelText,
            ));
            p.spawn((
                Text::new(format_upgrade_panel(&upgrades, &robot_pets)),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.78, 1.0, 0.88)),
                TechUpgradePanelText,
            ));
        });
}

fn despawn_chapter_select(mut commands: Commands, q: Query<Entity, With<ChapterSelectRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
}

fn spawn_fast_travel_map(
    parent: &mut ChildSpawnerCommands,
    chapters: &[crate::chapters::ChapterDef],
    progress: &ChapterProgress,
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
            spawn_map_region_band(
                map,
                2.0,
                62.0,
                41.0,
                36.0,
                Color::srgba(0.35, 0.48, 0.62, 0.26),
            );
            spawn_map_region_band(
                map,
                4.0,
                64.0,
                25.0,
                20.0,
                Color::srgba(0.72, 0.82, 0.94, 0.32),
            );
            spawn_map_region_band(
                map,
                72.0,
                20.0,
                22.0,
                58.0,
                Color::srgba(0.45, 0.62, 0.36, 0.22),
            );
            spawn_map_region_band(
                map,
                0.0,
                0.0,
                100.0,
                8.0,
                Color::srgba(0.08, 0.28, 0.44, 0.42),
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

                map.spawn((
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
                ))
                .with_children(|marker| {
                    marker.spawn((
                        Text::new(chapter_key_hint(location.id.0)),
                        TextFont {
                            font_size: 10.5,
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
                        font_size: 10.0,
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
        });
}

fn spawn_map_region_band(
    parent: &mut ChildSpawnerCommands,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    color: Color,
) {
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(left),
            top: Val::Percent(top),
            width: Val::Percent(width),
            height: Val::Percent(height),
            ..default()
        },
        BackgroundColor(color),
    ));
}

fn world_to_map_left(x: f32) -> f32 {
    ((x + 600.0) / 1200.0 * 100.0).clamp(2.0, 98.0)
}

fn world_to_map_top(z: f32) -> f32 {
    ((600.0 - z) / 1200.0 * 100.0).clamp(2.0, 98.0)
}

fn chapter_key_hint(chapter: u8) -> &'static str {
    match chapter {
        1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        9 => "9",
        10 => "0",
        11 => "Q",
        12 => "W",
        13 => "R",
        14 => "T",
        _ => "?",
    }
}

fn chapter_select_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    progress: Res<ChapterProgress>,
    mut current: ResMut<CurrentChapter>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let try_pick = |k: KeyCode, n: u8| -> Option<u8> {
        if keyboard.just_pressed(k) {
            Some(n)
        } else {
            None
        }
    };
    let pick = try_pick(KeyCode::Digit1, 1)
        .or_else(|| try_pick(KeyCode::Digit2, 2))
        .or_else(|| try_pick(KeyCode::Digit3, 3))
        .or_else(|| try_pick(KeyCode::Digit4, 4))
        .or_else(|| try_pick(KeyCode::Digit5, 5))
        .or_else(|| try_pick(KeyCode::Digit6, 6))
        .or_else(|| try_pick(KeyCode::Digit7, 7))
        .or_else(|| try_pick(KeyCode::Digit8, 8))
        .or_else(|| try_pick(KeyCode::Digit9, 9))
        .or_else(|| try_pick(KeyCode::Digit0, 10))
        .or_else(|| try_pick(KeyCode::KeyQ, 11))
        .or_else(|| try_pick(KeyCode::KeyW, 12))
        .or_else(|| try_pick(KeyCode::KeyR, 13))
        .or_else(|| try_pick(KeyCode::KeyT, 14));
    if let Some(n) = pick {
        if progress.is_unlocked(ChapterId(n)) {
            current.id = ChapterId(n);
            current.started = false;
            next_state.set(AppState::Playing);
        }
    }
    if keyboard.just_pressed(KeyCode::KeyE) {
        next_state.set(AppState::ChassisEditor);
    }
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::MainMenu);
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

fn chapter_select_perk_input(keyboard: Res<ButtonInput<KeyCode>>, mut perks: ResMut<PerkTree>) {
    let picks = [
        (KeyCode::KeyA, "heart_vitality"),
        (KeyCode::KeyS, "heart_regen"),
        (KeyCode::KeyD, "star_focus"),
        (KeyCode::KeyF, "star_charges"),
        (KeyCode::KeyG, "acro_evasion"),
        (KeyCode::KeyH, "acro_parry"),
    ];
    for (key, perk_id) in picks {
        if keyboard.just_pressed(key) {
            perks.try_spend(perk_id);
            break;
        }
    }
}

fn chapter_select_perk_panel_update(
    perks: Res<PerkTree>,
    mut text_q: Query<&mut Text, With<PerkPanelText>>,
) {
    if !perks.is_changed() {
        return;
    }
    for mut text in text_q.iter_mut() {
        *text = Text::new(format_perk_panel(&perks));
    }
}

fn chapter_select_upgrade_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut upgrades: ResMut<UpgradeLedger>,
    mut robot_pets: ResMut<RobotPetCollection>,
) {
    let picks = [
        (KeyCode::KeyZ, TechUpgradeId::BeamCapacitors),
        (KeyCode::KeyX, TechUpgradeId::NovaMissileForge),
        (KeyCode::KeyC, TechUpgradeId::SpriteTurretLattice),
        (KeyCode::KeyV, TechUpgradeId::ArmorPlating),
        (KeyCode::KeyB, TechUpgradeId::RejuvenationMatrix),
        (KeyCode::KeyN, TechUpgradeId::MechCommandLink),
    ];
    for (key, upgrade_id) in picks {
        if keyboard.just_pressed(key) {
            let _ = upgrades.try_purchase(upgrade_id, &mut robot_pets);
            break;
        }
    }
}

fn chapter_select_upgrade_panel_update(
    upgrades: Res<UpgradeLedger>,
    robot_pets: Res<RobotPetCollection>,
    mut text_q: Query<&mut Text, With<TechUpgradePanelText>>,
) {
    if !(upgrades.is_changed() || robot_pets.is_changed()) {
        return;
    }
    for mut text in text_q.iter_mut() {
        *text = Text::new(format_upgrade_panel(&upgrades, &robot_pets));
    }
}

fn format_perk_panel(perks: &PerkTree) -> String {
    let keys = [
        ("A", "heart_vitality"),
        ("S", "heart_regen"),
        ("D", "star_focus"),
        ("F", "star_charges"),
        ("G", "acro_evasion"),
        ("H", "acro_parry"),
    ];
    let defs = all_perks();
    let mut lines = vec![format!("PERK TRAINING  Points: {}", perks.points_unspent)];
    for (key, perk_id) in keys {
        if let Some(def) = defs.iter().find(|def| def.id == perk_id) {
            lines.push(format!(
                "[{}] {} {}/{} - {}",
                key,
                def.name,
                perks.rank(def.id),
                def.max_rank,
                def.description
            ));
        }
    }
    lines.join("\n")
}

fn format_upgrade_panel(upgrades: &UpgradeLedger, robot_pets: &RobotPetCollection) -> String {
    let mut lines = vec![format!(
        "TECH UPGRADES  Rejuvenation reserve: {:.0} charge",
        upgrades.rejuvenation_charge
    )];
    for def in all_tech_upgrades() {
        let rank = upgrades.rank(def.id);
        let cost = def.id.next_rank_cost(rank + 1);
        let cost_text = if rank >= def.max_rank {
            "MAX".to_string()
        } else {
            let afford = if robot_pets.can_afford(&cost) {
                "READY"
            } else {
                "NEED"
            };
            format!("{} [{}]", format_part_costs(&cost), afford)
        };
        lines.push(format!(
            "[{}] {} {}/{} - {} | {}",
            def.key_hint, def.name, rank, def.max_rank, def.description, cost_text
        ));
    }
    lines.push(
        "Rejuvenation healing spends reserve; buy Matrix ranks to refill and improve efficiency."
            .to_string(),
    );
    lines.join("\n")
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
        let blueprints: Vec<_> = select
            .slots
            .iter()
            .map(|slot| slot.blueprint.clone())
            .collect();
        *select = PlayerSelectState::default();
        for (slot, blueprint) in select.slots.iter_mut().zip(blueprints) {
            slot.blueprint = blueprint;
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
                TextFont { font_size: 54.0, ..default() },
                TextColor(Color::srgb(1.0, 0.9, 0.25)),
            ));

            // Prompt line (updated dynamically)
            root.spawn((
                Text::new("All players ready to begin"),
                TextFont { font_size: 18.0, ..default() },
                TextColor(Color::srgb(0.55, 0.65, 0.85)),
                PlayerSelectPrompt,
            ));

            // Four player slot cards
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(18.0),
                ..default()
            })
            .with_children(|row| {
                for i in 0..4u8 {
                    row.spawn((
                        Node {
                            width: Val::Px(210.0),
                            height: Val::Px(240.0),
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
                            TextFont { font_size: 30.0, ..default() },
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
                            TextFont { font_size: 19.0, ..default() },
                            TextColor(Color::WHITE),
                            PlayerSlotCharText(i),
                        ));

                        // Controller info (live axes)
                        let ctrl_text = if i == 0 { "KEYBOARD + GAMEPAD 1" } else { "waiting..." };
                        card.spawn((
                            Text::new(ctrl_text),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(Color::srgb(0.55, 0.55, 0.75)),
                            PlayerSlotInputText(i),
                        ));

                        // Status / instruction
                        let status = if i == 0 { "[ ENTER / A ] Ready" } else { "Press any btn to join" };
                        card.spawn((
                            Text::new(status),
                            TextFont { font_size: 15.0, ..default() },
                            TextColor(Color::srgb(0.65, 0.65, 0.4)),
                            PlayerSlotStatusText(i),
                        ));
                    });
                }
            });

            // Footer hints
            root.spawn((
                Text::new("P1: ← → / left stick chars  |  Enter/A = ready  |  C/Y = customize  |  ESC = back\nP2-P4: controller 2+ joins  |  D-pad/stick = chars  |  A = ready  |  Y = customize  |  B = leave"),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::srgb(0.38, 0.38, 0.5)),
            ));
        });
}

fn despawn_player_select(mut commands: Commands, q: Query<Entity, With<PlayerSelectRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
}

fn player_select_update(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    native: Res<NativeControllerState>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    time: Res<Time>,
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
    let dt = time.delta_secs();
    let roster_len = HERO_ROSTER.len();

    // ESC → back to main menu
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::MainMenu);
        return;
    }

    // Sorted gamepads for stable assignment. Gameplay maps gamepad 0 to P1,
    // so the lobby must do the same or the first controller appears dead.
    let mut gps: Vec<(Entity, &Gamepad)> = gamepads.iter().collect();
    gps.sort_by_key(|(e, _)| e.index());
    let p1_gp = gps.first().map(|(_, gamepad)| *gamepad);
    let p1_entity = gps.first().map(|(entity, _)| *entity);
    let native_for_p1 = p1_gp.is_none() && native.connected;
    let pressed_buttons: Vec<(Entity, GamepadButton)> = button_events
        .read()
        .filter(|event| event.state == ButtonState::Pressed)
        .map(|event| (event.entity, event.button))
        .collect();
    let event_pressed = |entity: Entity, button: GamepadButton| {
        pressed_buttons
            .iter()
            .any(|(event_entity, event_button)| *event_entity == entity && *event_button == button)
    };

    // ── P1 (always joined, keyboard + gamepad 0) ─────────────────────────────
    {
        let slot = &mut select.slots[0];
        slot.joined = true;
        slot.stick_cooldown = (slot.stick_cooldown - dt).max(0.0);

        if !slot.ready && slot.stick_cooldown <= 0.0 {
            let lx = p1_gp
                .and_then(|gamepad| gamepad.get(GamepadAxis::LeftStickX))
                .unwrap_or(0.0);
            let native_lx = if native_for_p1 {
                native.move_axis.x
            } else {
                0.0
            };
            let left = keyboard.just_pressed(KeyCode::ArrowLeft)
                || p1_gp
                    .map(|gamepad| gamepad.just_pressed(GamepadButton::DPadLeft))
                    .unwrap_or(false)
                || p1_entity
                    .map(|entity| event_pressed(entity, GamepadButton::DPadLeft))
                    .unwrap_or(false)
                || (native_for_p1 && native.just_pressed(NativeButton::DPadLeft))
                || native_lx < -0.5
                || lx < -0.5;
            let right = keyboard.just_pressed(KeyCode::ArrowRight)
                || p1_gp
                    .map(|gamepad| gamepad.just_pressed(GamepadButton::DPadRight))
                    .unwrap_or(false)
                || p1_entity
                    .map(|entity| event_pressed(entity, GamepadButton::DPadRight))
                    .unwrap_or(false)
                || (native_for_p1 && native.just_pressed(NativeButton::DPadRight))
                || native_lx > 0.5
                || lx > 0.5;
            if left {
                slot.character_index = (slot.character_index + roster_len - 1) % roster_len;
                slot.stick_cooldown = 0.18;
            } else if right {
                slot.character_index = (slot.character_index + 1) % roster_len;
                slot.stick_cooldown = 0.18;
            }
        }
        if keyboard.just_pressed(KeyCode::Enter)
            || keyboard.just_pressed(KeyCode::Space)
            || p1_gp
                .map(|gamepad| {
                    gamepad.just_pressed(GamepadButton::South)
                        || gamepad.just_pressed(GamepadButton::Start)
                })
                .unwrap_or(false)
            || p1_entity
                .map(|entity| {
                    event_pressed(entity, GamepadButton::South)
                        || event_pressed(entity, GamepadButton::Start)
                })
                .unwrap_or(false)
            || (native_for_p1
                && (native.just_pressed(NativeButton::South)
                    || native.just_pressed(NativeButton::Start)))
        {
            slot.ready = !slot.ready;
        }
        // C / Y = open character designer for P1
        let customize_pressed = keyboard.just_pressed(KeyCode::KeyC)
            || p1_gp
                .map(|gamepad| gamepad.just_pressed(GamepadButton::North))
                .unwrap_or(false)
            || p1_entity
                .map(|entity| event_pressed(entity, GamepadButton::North))
                .unwrap_or(false)
            || (native_for_p1 && native.just_pressed(NativeButton::North));
        if customize_pressed && !slot.ready {
            design_data.player_index = 0;
            next_state.set(AppState::CharacterDesign);
            return;
        }
    }

    // ── P2-P4 (gamepads 1+) ──────────────────────────────────────────────────
    for i in 1u8..4 {
        let gp_idx = i as usize;
        let slot = &mut select.slots[i as usize];
        slot.stick_cooldown = (slot.stick_cooldown - dt).max(0.0);

        let Some((gp_entity, gp)) = gps.get(gp_idx) else {
            slot.joined = false;
            continue;
        };

        let any_just = [
            GamepadButton::South,
            GamepadButton::East,
            GamepadButton::North,
            GamepadButton::West,
            GamepadButton::Start,
            GamepadButton::Select,
        ]
        .iter()
        .any(|&b| gp.just_pressed(b) || event_pressed(*gp_entity, b));

        if !slot.joined {
            if any_just {
                slot.joined = true;
                slot.character_index = i as usize % roster_len;
                slot.ready = false;
            }
        } else {
            // Navigate character (not while ready)
            if !slot.ready && slot.stick_cooldown <= 0.0 {
                let lx = gp.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
                let left = gp.just_pressed(GamepadButton::DPadLeft)
                    || event_pressed(*gp_entity, GamepadButton::DPadLeft)
                    || lx < -0.5;
                let right = gp.just_pressed(GamepadButton::DPadRight)
                    || event_pressed(*gp_entity, GamepadButton::DPadRight)
                    || lx > 0.5;
                if left {
                    slot.character_index = (slot.character_index + roster_len - 1) % roster_len;
                    slot.stick_cooldown = 0.25;
                } else if right {
                    slot.character_index = (slot.character_index + 1) % roster_len;
                    slot.stick_cooldown = 0.25;
                }
            }
            // A = ready toggle
            if gp.just_pressed(GamepadButton::South)
                || event_pressed(*gp_entity, GamepadButton::South)
            {
                slot.ready = !slot.ready;
            }
            // B = leave (only if not ready)
            if (gp.just_pressed(GamepadButton::East)
                || event_pressed(*gp_entity, GamepadButton::East))
                && !slot.ready
            {
                slot.joined = false;
            }
            // Y = open character designer
            if (gp.just_pressed(GamepadButton::North)
                || event_pressed(*gp_entity, GamepadButton::North))
                && !slot.ready
            {
                design_data.player_index = i as usize;
                next_state.set(AppState::CharacterDesign);
                return;
            }
        }
    }

    // ── Auto-advance when all joined players are ready ────────────────────────
    if select.all_ready() {
        config.active = select.active_count().max(1);
        next_state.set(AppState::ChapterSelect);
        return;
    }

    // ── Update text nodes ─────────────────────────────────────────────────────
    for (mut t, marker) in char_q.iter_mut() {
        let s = &select.slots[marker.0 as usize];
        *t = Text::new(if s.joined {
            format!("< {} >", HERO_ROSTER[s.character_index % roster_len])
        } else {
            "- - - - -".to_string()
        });
    }

    for (mut t, marker) in status_q.iter_mut() {
        let i = marker.0 as usize;
        let s = &select.slots[i];
        *t = Text::new(if !s.joined {
            if i == 0 {
                "[ ENTER / A ] Ready"
            } else {
                "Press any btn to join"
            }
            .to_string()
        } else if s.ready {
            "✓  READY".to_string()
        } else if i == 0 {
            "[ ENTER / A ] Ready".to_string()
        } else {
            "[A] Ready    [B] Leave".to_string()
        });
    }

    for (mut t, marker) in input_q.iter_mut() {
        let i = marker.0 as usize;
        if i == 0 {
            if let Some((_, gp)) = gps.first() {
                let lx = gp.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
                let ly = gp.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
                *t = Text::new(format!("KEYBOARD + GAMEPAD 1   X:{:+.2} Y:{:+.2}", lx, ly));
            } else if native.connected {
                *t = Text::new(format!(
                    "KEYBOARD + MACOS CONTROLLER   X:{:+.2} Y:{:+.2}",
                    native.move_axis.x, native.move_axis.y
                ));
            } else {
                *t = Text::new("KEYBOARD + GAMEPAD 1");
            }
        } else {
            let gp_idx = i;
            if let Some((_, gp)) = gps.get(gp_idx) {
                let lx = gp.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
                let ly = gp.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
                *t = Text::new(format!("GAMEPAD {}   X:{:+.2} Y:{:+.2}", i + 1, lx, ly));
            } else {
                *t = Text::new(format!("GAMEPAD {} — not found", i + 1));
            }
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
                "Starting..."
            } else {
                "mark ready to begin"
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
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.4, 0.2)),
                    WaveText,
                ));
                panel.spawn((
                    Text::new("Enemies: 0"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.7, 0.3)),
                    EnemyCountText,
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 22.0,
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
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.9, 0.3)),
                    MessageText,
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
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.92, 0.30)),
                    ObjectiveTitleText,
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 13.0,
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
                                font_size: 11.0,
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
                    Text::new("[C] CRAFTING  –  press 1-5 to craft"),
                    TextFont {
                        font_size: 15.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.0, 0.9, 1.0)),
                ));
                panel.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 13.0,
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
                    font_size: 15.0,
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
            font_size,
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
                    font_size: 14.0,
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
            &WeaponInventory,
            &SpecialWeaponInventory,
            &ArmorSet,
            &BeamSabre,
        ),
        With<Player>,
    >,
    wave: Res<WaveInfo>,
    current: Res<CurrentChapter>,
    mut bar_q: Query<(&mut Node, &PlayerHudBar)>,
    mut text_sets: ParamSet<(
        Query<(&mut Text, &PlayerHudText)>,
        Query<&mut Text, With<WaveText>>,
        Query<&mut Text, With<EnemyCountText>>,
    )>,
) {
    for (index, health, stats, jetpack, weapons, special, armor, sabre) in player_q.iter() {
        for (mut node, bar) in bar_q
            .iter_mut()
            .filter(|(_, bar)| bar.player_index == index.0)
        {
            let percent = match bar.kind {
                PlayerHudBarKind::Health => health.current / health.max,
                PlayerHudBarKind::Armor => stats.armor / stats.max_armor,
                PlayerHudBarKind::Stamina => stats.stamina / stats.max_stamina,
                PlayerHudBarKind::Jetpack => jetpack.fuel / jetpack.max_fuel,
            };
            node.width = Val::Percent((percent * 100.0).clamp(0.0, 100.0));
        }

        let weapon = weapons.active();
        let sabre_str = if sabre.active { " [SABRE]" } else { "" };
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
                    format!("{}{}", weapon.weapon_type.display_name(), sabre_str)
                }
                PlayerHudTextKind::Ammo => format!("{} / {}", weapon.ammo, weapon.max_ammo),
                PlayerHudTextKind::SpecialAmmo => format!(
                    "7:{} 8:{} 9:{} 0:{}",
                    special.slot7.ammo, special.slot8.ammo, special.slot9.ammo, special.slot0.ammo,
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
    keyboard: Res<ButtonInput<KeyCode>>,
    mut panel_state: ResMut<CraftingPanelState>,
    mut panel_q: Query<&mut Node, With<CraftingPanelRoot>>,
    mut text_q: Query<&mut Text, With<CraftingPanelText>>,
    player_input_q: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
    mut player_q: Query<(&PlayerIndex, &mut Inventory, &PlayerStats), With<Player>>,
    mut queue: ResMut<CraftingQueue>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (idx, input) in player_input_q.iter() {
        if input.crafting {
            let closing_same_owner = panel_state.visible && panel_state.owner == Some(idx.0);
            panel_state.visible = !closing_same_owner;
            panel_state.owner = panel_state.visible.then_some(idx.0);
        }
    }

    if let Ok(mut node) = panel_q.single_mut() {
        node.display = if panel_state.visible {
            Display::Flex
        } else {
            Display::None
        };
    }

    if !panel_state.visible {
        return;
    }

    let Some(owner) = panel_state.owner else {
        panel_state.visible = false;
        return;
    };

    let Some((_, mut inventory, stats)) = player_q.iter_mut().find(|(idx, _, _)| idx.0 == owner)
    else {
        panel_state.visible = false;
        panel_state.owner = None;
        return;
    };
    let recipes = all_recipes();

    // Build recipe display text
    let mut display = String::new();
    for (i, recipe) in recipes.iter().enumerate() {
        let can_craft = stats.level >= recipe.required_level
            && recipe
                .materials
                .iter()
                .all(|m| inventory.has(m.item_id, m.quantity));
        let status = if can_craft { "✓" } else { "✗" };
        let mat_list: Vec<String> = recipe
            .materials
            .iter()
            .map(|m| format!("{}×{}", m.quantity, m.item_id.replace('_', " ")))
            .collect();
        display.push_str(&format!(
            "[{}] {} {}  →  {}×{}\n     {}\n",
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

    // Handle 1-5 key presses to craft
    let craft_index = if keyboard.just_pressed(KeyCode::Digit1) {
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
                    font_size: 64.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.2, 0.1)),
            ));
            p.spawn((
                Text::new("Press R to restart"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.8)),
            ));
        });
}

fn game_over_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
    go_root: Query<Entity, With<GameOverRoot>>,
    hud_root: Query<Entity, With<HudRoot>>,
    mut commands: Commands,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        for e in go_root.iter() {
            commands.entity(e).despawn();
        }
        for e in hud_root.iter() {
            commands.entity(e).despawn();
        }
        next_state.set(AppState::MainMenu);
    }
}
