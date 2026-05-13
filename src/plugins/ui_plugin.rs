use bevy::prelude::*;

use bevy::input::gamepad::{GamepadAxis, GamepadButton};

use crate::chapters::{all_chapters, ChapterId};
use crate::components::armor::ArmorSet;
use crate::components::inventory::Inventory;
use crate::components::player::{JetpackState, Player, PlayerStats};
use crate::components::weapon::{BeamSabre, SpecialWeaponInventory, WeaponInventory};
use crate::damage::Health;
use crate::events::*;
use crate::plugins::crafting_plugin::{all_recipes, start_craft, CraftingQueue};
use crate::resources::{
    ChapterProgress, CurrentChapter, LocalPlayerConfig, PlayerSelectState, UiMessage, WaveInfo,
    HERO_ROSTER,
};
use crate::state::AppState;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiMessage>()
            .init_resource::<CraftingPanelState>()
            .add_systems(Startup, spawn_menu_camera)
            .add_systems(OnEnter(AppState::MainMenu), setup_main_menu)
            .add_systems(OnEnter(AppState::PlayerSelect), (despawn_menu, setup_player_select))
            .add_systems(OnExit(AppState::PlayerSelect), despawn_player_select)
            .add_systems(OnEnter(AppState::ChapterSelect), setup_chapter_select)
            .add_systems(OnExit(AppState::ChapterSelect), despawn_chapter_select)
            .add_systems(
                OnEnter(AppState::Playing),
                (setup_hud, despawn_menu, despawn_menu_camera),
            )
            .add_systems(OnEnter(AppState::GameOver), setup_game_over)
            .add_systems(
                Update,
                (
                    hud_update_system,
                    message_timer_system,
                    ui_message_listener,
                    damage_vignette_system,
                    crafting_panel_system,
                    boss_alert_system,
                    game_over_input,
                )
                    .run_if(in_state(AppState::Playing).or(in_state(AppState::GameOver))),
            )
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
                chapter_select_input.run_if(in_state(AppState::ChapterSelect)),
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
struct GameOverRoot;
#[derive(Component)]
struct StartButton;
#[derive(Component, Default)]
struct HealthBar;
#[derive(Component, Default)]
struct ArmorBar;
#[derive(Component, Default)]
struct StaminaBar;
#[derive(Component, Default)]
struct JetpackBar;
#[derive(Component)]
struct CreditsText;
#[derive(Component)]
struct LevelText;
#[derive(Component)]
struct ElementBadge;
#[derive(Component)]
struct EnemyCountText;
#[derive(Component)]
struct WaveText;
#[derive(Component)]
struct WeaponNameText;
#[derive(Component)]
struct AmmoText;
#[derive(Component)]
struct SpecialAmmoText;
#[derive(Component)]
struct MessageText;
#[derive(Component)]
struct DamageVignette {
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
}

// ── Menu Camera ───────────────────────────────────────────────────────────────
fn spawn_menu_camera(mut commands: Commands) {
    commands.spawn((Camera3dBundle::default(), MenuCamera));
}

fn despawn_menu_camera(mut commands: Commands, q: Query<Entity, With<MenuCamera>>) {
    for e in q.iter() {
        commands.entity(e).despawn_recursive();
    }
}

// ── Menu Setup ────────────────────────────────────────────────────────────────
fn despawn_menu(mut commands: Commands, q: Query<Entity, With<MainMenuRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn_recursive();
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
        p.spawn((Text::new("Four siblings, rift aliens, dragon royalty, and star-powered science."), TextFont { font_size: 24.0, ..default() }, TextColor(Color::srgb(0.65, 0.8, 1.0))));
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
            Text::new("WASD Move  |  Mouse Look  |  LMB Star Beam  |  V/B Mana Combos  |  T Star Sabre\nSpace Jump/Wall Jump  |  Hold toward ledges to hang  |  E Climb  |  7/8/9/0 Energy Tools  |  F Parry  |  Q Drop/Dodge"),
            TextFont { font_size: 14.0, ..default() }, TextColor(Color::srgb(0.5, 0.5, 0.7)),
        ));
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

// ── Chapter Select ────────────────────────────────────────────────────────────
#[derive(Component)]
struct ChapterSelectRoot;

fn setup_chapter_select(mut commands: Commands, progress: Res<ChapterProgress>) {
    let chapters = all_chapters();
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::FlexStart,
                padding: UiRect::all(Val::Px(40.0)),
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.06, 1.0)),
            ChapterSelectRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("SELECT CHAPTER"),
                TextFont {
                    font_size: 48.0,
                    ..default()
                },
                TextColor(Color::srgb(0.4, 0.85, 1.0)),
            ));
            p.spawn((
                Text::new("Press 1-9 / 0 / Q W E R for chapters 1-14   |   [E]ditor   [Esc] Back"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.85)),
            ));
            p.spawn(Node {
                height: Val::Px(20.0),
                ..default()
            });
            for ch in &chapters {
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
                p.spawn((
                    Text::new(format!(
                        "{} Ch.{:02} — {}    ({})",
                        prefix, ch.id.0, ch.title, ch.subtitle
                    )),
                    TextFont {
                        font_size: 18.0,
                        ..default()
                    },
                    TextColor(color),
                ));
            }
        });
}

fn despawn_chapter_select(mut commands: Commands, q: Query<Entity, With<ChapterSelectRoot>>) {
    for e in q.iter() {
        commands.entity(e).despawn_recursive();
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

fn setup_player_select(
    mut commands: Commands,
    mut select: ResMut<PlayerSelectState>,
) {
    // Reset lobby: P1 always joined, others cleared.
    *select = PlayerSelectState::default();
    select.slots[0].joined = true;

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
                        let ctrl_text = if i == 0 { "KEYBOARD + MOUSE" } else { "waiting..." };
                        card.spawn((
                            Text::new(ctrl_text),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(Color::srgb(0.55, 0.55, 0.75)),
                            PlayerSlotInputText(i),
                        ));

                        // Status / instruction
                        let status = if i == 0 { "[ ENTER ] Ready" } else { "Press any btn to join" };
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
                Text::new("P1: ← → chars  |  Enter = ready  |  ESC = back\nP2-P4: any button joins  |  D-pad/stick = chars  |  A = ready  |  B = leave"),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::srgb(0.38, 0.38, 0.5)),
            ));
        });
}

fn despawn_player_select(
    mut commands: Commands,
    q: Query<Entity, With<PlayerSelectRoot>>,
) {
    for e in q.iter() {
        commands.entity(e).despawn_recursive();
    }
}

fn player_select_update(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    time: Res<Time>,
    mut select: ResMut<PlayerSelectState>,
    mut config: ResMut<LocalPlayerConfig>,
    mut next_state: ResMut<NextState<AppState>>,
    // Text queries — each set is disjoint via Without<> guards
    mut char_q: Query<
        (&mut Text, &PlayerSlotCharText),
        (Without<PlayerSlotStatusText>, Without<PlayerSlotInputText>, Without<PlayerSelectPrompt>),
    >,
    mut status_q: Query<
        (&mut Text, &PlayerSlotStatusText),
        (Without<PlayerSlotCharText>, Without<PlayerSlotInputText>, Without<PlayerSelectPrompt>),
    >,
    mut input_q: Query<
        (&mut Text, &PlayerSlotInputText),
        (Without<PlayerSlotCharText>, Without<PlayerSlotStatusText>, Without<PlayerSelectPrompt>),
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

    // ESC → back to main menu
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::MainMenu);
        return;
    }

    // Sorted gamepads for stable P2/P3/P4 assignment
    let mut gps: Vec<(Entity, &Gamepad)> = gamepads.iter().collect();
    gps.sort_by_key(|(e, _)| e.index());

    // ── P1 (always joined, keyboard) ──────────────────────────────────────────
    {
        let slot = &mut select.slots[0];
        slot.joined = true;
        slot.stick_cooldown = (slot.stick_cooldown - dt).max(0.0);

        if !slot.ready && slot.stick_cooldown <= 0.0 {
            if keyboard.just_pressed(KeyCode::ArrowLeft) {
                slot.character_index = (slot.character_index + 3) % 4;
                slot.stick_cooldown = 0.18;
            } else if keyboard.just_pressed(KeyCode::ArrowRight) {
                slot.character_index = (slot.character_index + 1) % 4;
                slot.stick_cooldown = 0.18;
            }
        }
        if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::Space) {
            slot.ready = !slot.ready;
        }
    }

    // ── P2-P4 (gamepads) ──────────────────────────────────────────────────────
    for i in 1u8..4 {
        let gp_idx = (i - 1) as usize;
        let slot = &mut select.slots[i as usize];
        slot.stick_cooldown = (slot.stick_cooldown - dt).max(0.0);

        let Some((_, gp)) = gps.get(gp_idx) else {
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
        .any(|&b| gp.just_pressed(b));

        if !slot.joined {
            if any_just {
                slot.joined = true;
                slot.character_index = i as usize % 4;
                slot.ready = false;
            }
        } else {
            // Navigate character (not while ready)
            if !slot.ready && slot.stick_cooldown <= 0.0 {
                let lx = gp.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
                let left = gp.just_pressed(GamepadButton::DPadLeft) || lx < -0.5;
                let right = gp.just_pressed(GamepadButton::DPadRight) || lx > 0.5;
                if left {
                    slot.character_index = (slot.character_index + 3) % 4;
                    slot.stick_cooldown = 0.25;
                } else if right {
                    slot.character_index = (slot.character_index + 1) % 4;
                    slot.stick_cooldown = 0.25;
                }
            }
            // A = ready toggle
            if gp.just_pressed(GamepadButton::South) {
                slot.ready = !slot.ready;
            }
            // B = leave (only if not ready)
            if gp.just_pressed(GamepadButton::East) && !slot.ready {
                slot.joined = false;
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
            format!("< {} >", HERO_ROSTER[s.character_index])
        } else {
            "- - - - -".to_string()
        });
    }

    for (mut t, marker) in status_q.iter_mut() {
        let i = marker.0 as usize;
        let s = &select.slots[i];
        *t = Text::new(if !s.joined {
            if i == 0 { "[ ENTER ] Ready" } else { "Press any btn to join" }.to_string()
        } else if s.ready {
            "✓  READY".to_string()
        } else if i == 0 {
            "[ ENTER ] Ready".to_string()
        } else {
            "[A] Ready    [B] Leave".to_string()
        });
    }

    for (mut t, marker) in input_q.iter_mut() {
        let i = marker.0 as usize;
        if i == 0 {
            *t = Text::new("KEYBOARD + MOUSE");
        } else {
            let gp_idx = i - 1;
            if let Some((_, gp)) = gps.get(gp_idx) {
                let lx = gp.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
                let ly = gp.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
                *t = Text::new(format!("GAMEPAD {}   X:{:+.2} Y:{:+.2}", i, lx, ly));
            } else {
                *t = Text::new(format!("GAMEPAD {} — not found", i));
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
    if let Ok(mut t) = prompt_q.get_single_mut() {
        *t = Text::new(format!(
            "{}/{} players ready  —  {}",
            ready,
            joined,
            if ready == joined && joined > 0 { "Starting..." } else { "mark ready to begin" }
        ));
    }
}

// ── HUD Setup ─────────────────────────────────────────────────────────────────
fn setup_hud(mut commands: Commands) {
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
            // ── Damage vignette (full-screen red overlay) ─────────────────────────
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.8, 0.0, 0.0, 0.0)),
                DamageVignette { alpha: 0.0 },
            ));

            // ── Top-left stats panel ──────────────────────────────────────────────
            root.spawn(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(16.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                width: Val::Px(220.0),
                ..default()
            })
            .with_children(|panel| {
                spawn_bar(panel, "HP", HealthBar, Color::srgb(0.2, 0.8, 0.2));
                spawn_bar(panel, "AR", ArmorBar, Color::srgb(0.2, 0.5, 1.0));
                spawn_bar(panel, "ST", StaminaBar, Color::srgb(0.9, 0.7, 0.0));
                spawn_bar(panel, "JP", JetpackBar, Color::srgb(0.0, 0.9, 0.9));
                panel.spawn((
                    Text::new("¢ 0"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.75, 0.1)),
                    CreditsText,
                ));
                panel.spawn((
                    Text::new("LVL 1"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.7, 0.4, 1.0)),
                    LevelText,
                ));
                panel.spawn((
                    Text::new("Element: None"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.7, 0.7, 0.9)),
                    ElementBadge,
                ));
            });

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

            // ── Bottom-left weapon ────────────────────────────────────────────────
            root.spawn(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(80.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Starlight Popper"),
                    TextFont {
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.9, 0.25)),
                    WeaponNameText,
                ));
                panel.spawn((
                    Text::new("60 / 60"),
                    TextFont {
                        font_size: 18.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.8, 0.8, 0.8)),
                    AmmoText,
                ));
                panel.spawn((
                    Text::new("7:Star 8:Tri 9:Moon 0:Sprite"),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.6, 0.6, 0.8)),
                    SpecialAmmoText,
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

fn spawn_bar<L: Component + Default>(
    parent: &mut ChildBuilder,
    label: &str,
    _marker: L,
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
                    width: Val::Px(150.0),
                    height: Val::Px(12.0),
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
                    L::default(),
                ));
            });
        });
}

// ── HUD Update ────────────────────────────────────────────────────────────────
fn hud_update_system(
    player_q: Query<
        (
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
    mut hp_q: Query<
        &mut Node,
        (
            With<HealthBar>,
            Without<ArmorBar>,
            Without<StaminaBar>,
            Without<JetpackBar>,
        ),
    >,
    mut ar_q: Query<
        &mut Node,
        (
            With<ArmorBar>,
            Without<HealthBar>,
            Without<StaminaBar>,
            Without<JetpackBar>,
        ),
    >,
    mut st_q: Query<
        &mut Node,
        (
            With<StaminaBar>,
            Without<HealthBar>,
            Without<ArmorBar>,
            Without<JetpackBar>,
        ),
    >,
    mut jp_q: Query<
        &mut Node,
        (
            With<JetpackBar>,
            Without<HealthBar>,
            Without<ArmorBar>,
            Without<StaminaBar>,
        ),
    >,
    mut credits_q: Query<
        &mut Text,
        (
            With<CreditsText>,
            Without<LevelText>,
            Without<ElementBadge>,
            Without<WaveText>,
            Without<EnemyCountText>,
            Without<WeaponNameText>,
            Without<AmmoText>,
            Without<SpecialAmmoText>,
            Without<MessageText>,
            Without<BossAlertText>,
            Without<CraftingPanelText>,
        ),
    >,
    mut level_q: Query<
        &mut Text,
        (
            With<LevelText>,
            Without<CreditsText>,
            Without<ElementBadge>,
            Without<WaveText>,
            Without<EnemyCountText>,
            Without<WeaponNameText>,
            Without<AmmoText>,
            Without<SpecialAmmoText>,
            Without<MessageText>,
            Without<BossAlertText>,
            Without<CraftingPanelText>,
        ),
    >,
    mut elem_q: Query<
        &mut Text,
        (
            With<ElementBadge>,
            Without<CreditsText>,
            Without<LevelText>,
            Without<WaveText>,
            Without<EnemyCountText>,
            Without<WeaponNameText>,
            Without<AmmoText>,
            Without<SpecialAmmoText>,
            Without<MessageText>,
            Without<BossAlertText>,
            Without<CraftingPanelText>,
        ),
    >,
    mut wave_q: Query<
        &mut Text,
        (
            With<WaveText>,
            Without<EnemyCountText>,
            Without<CreditsText>,
            Without<LevelText>,
            Without<ElementBadge>,
            Without<WeaponNameText>,
            Without<AmmoText>,
            Without<SpecialAmmoText>,
            Without<MessageText>,
            Without<BossAlertText>,
            Without<CraftingPanelText>,
        ),
    >,
    mut enm_q: Query<
        &mut Text,
        (
            With<EnemyCountText>,
            Without<WaveText>,
            Without<CreditsText>,
            Without<LevelText>,
            Without<ElementBadge>,
            Without<WeaponNameText>,
            Without<AmmoText>,
            Without<SpecialAmmoText>,
            Without<MessageText>,
            Without<BossAlertText>,
            Without<CraftingPanelText>,
        ),
    >,
    mut wname_q: Query<
        &mut Text,
        (
            With<WeaponNameText>,
            Without<AmmoText>,
            Without<SpecialAmmoText>,
            Without<WaveText>,
            Without<EnemyCountText>,
            Without<CreditsText>,
            Without<LevelText>,
            Without<ElementBadge>,
            Without<MessageText>,
            Without<BossAlertText>,
            Without<CraftingPanelText>,
        ),
    >,
    mut ammo_q: Query<
        &mut Text,
        (
            With<AmmoText>,
            Without<WeaponNameText>,
            Without<SpecialAmmoText>,
            Without<WaveText>,
            Without<EnemyCountText>,
            Without<CreditsText>,
            Without<LevelText>,
            Without<ElementBadge>,
            Without<MessageText>,
            Without<BossAlertText>,
            Without<CraftingPanelText>,
        ),
    >,
    mut spammo_q: Query<
        &mut Text,
        (
            With<SpecialAmmoText>,
            Without<WeaponNameText>,
            Without<AmmoText>,
            Without<WaveText>,
            Without<EnemyCountText>,
            Without<CreditsText>,
            Without<LevelText>,
            Without<ElementBadge>,
            Without<MessageText>,
            Without<BossAlertText>,
            Without<CraftingPanelText>,
        ),
    >,
) {
    let Ok((health, stats, jetpack, weapons, special, armor, sabre)) = player_q.get_single() else {
        return;
    };

    if let Ok(mut n) = hp_q.get_single_mut() {
        n.width = Val::Percent((health.current / health.max * 100.0).clamp(0.0, 100.0));
    }
    if let Ok(mut n) = ar_q.get_single_mut() {
        n.width = Val::Percent((stats.armor / stats.max_armor * 100.0).clamp(0.0, 100.0));
    }
    if let Ok(mut n) = st_q.get_single_mut() {
        n.width = Val::Percent((stats.stamina / stats.max_stamina * 100.0).clamp(0.0, 100.0));
    }
    if let Ok(mut n) = jp_q.get_single_mut() {
        n.width = Val::Percent((jetpack.fuel / jetpack.max_fuel * 100.0).clamp(0.0, 100.0));
    }

    if let Ok(mut t) = credits_q.get_single_mut() {
        *t = Text::new(format!("¢ {}", stats.credits));
    }
    if let Ok(mut t) = level_q.get_single_mut() {
        *t = Text::new(format!(
            "LVL {}  XP {}/{}",
            stats.level,
            stats.experience,
            stats.xp_for_next_level()
        ));
    }
    if let Ok(mut t) = elem_q.get_single_mut() {
        *t = Text::new(format!("Element: {}", armor.active_element.display_name()));
    }
    if let Ok(mut t) = wave_q.get_single_mut() {
        *t = Text::new(format!("Ch.{:02}  Rift {}", current.id.0, wave.wave_number));
    }
    if let Ok(mut t) = enm_q.get_single_mut() {
        *t = Text::new(format!("Enemies: {}", wave.enemy_count));
    }

    let weapon = weapons.active();
    let sabre_str = if sabre.active { " [STAR SABRE]" } else { "" };
    if let Ok(mut t) = wname_q.get_single_mut() {
        *t = Text::new(format!(
            "{}{}",
            weapon.weapon_type.display_name(),
            sabre_str
        ));
    }
    if let Ok(mut t) = ammo_q.get_single_mut() {
        *t = Text::new(format!("{} / {}", weapon.ammo, weapon.max_ammo));
    }

    if let Ok(mut t) = spammo_q.get_single_mut() {
        *t = Text::new(format!(
            "7:Star({})  8:Tri({})  9:Moon({})  0:Sprite({})",
            special.slot7.ammo, special.slot8.ammo, special.slot9.ammo, special.slot0.ammo,
        ));
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
            if let Ok(mut t) = text_q.get_single_mut() {
                *t = Text::new("");
            }
        }
    }
}

fn ui_message_listener(
    mut ev: EventReader<UiMessageEvent>,
    mut msg: ResMut<UiMessage>,
    mut text_q: Query<&mut Text, With<MessageText>>,
) {
    for e in ev.read() {
        msg.text = e.text.clone();
        msg.timer = e.duration;
        if let Ok(mut t) = text_q.get_single_mut() {
            *t = Text::new(e.text.clone());
        }
    }
}

// ── Damage Vignette ───────────────────────────────────────────────────────────
fn damage_vignette_system(
    time: Res<Time>,
    mut damage_ev: EventReader<PlayerDamagedEvent>,
    mut vignette_q: Query<(&mut BackgroundColor, &mut DamageVignette)>,
) {
    for _ in damage_ev.read() {
        for (_, mut v) in vignette_q.iter_mut() {
            v.alpha = 0.45;
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
    mut boss_ev: EventReader<BossSpawnedEvent>,
    mut alert_q: Query<(&mut Text, &mut TextColor), With<BossAlertText>>,
) {
    for ev in boss_ev.read() {
        if let Ok((mut t, _)) = alert_q.get_single_mut() {
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
    mut player_q: Query<(&mut Inventory, &PlayerStats), With<Player>>,
    mut queue: ResMut<CraftingQueue>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    // Toggle panel visibility
    if keyboard.just_pressed(KeyCode::KeyC) {
        panel_state.visible = !panel_state.visible;
    }

    if let Ok(mut node) = panel_q.get_single_mut() {
        node.display = if panel_state.visible {
            Display::Flex
        } else {
            Display::None
        };
    }

    if !panel_state.visible {
        return;
    }

    let Ok((mut inventory, stats)) = player_q.get_single_mut() else {
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

    if let Ok(mut t) = text_q.get_single_mut() {
        *t = Text::new(display);
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
            match start_craft(recipe.id, &mut inventory, stats, &mut queue) {
                Ok(()) => {
                    msg_ev.send(UiMessageEvent {
                        text: format!("Crafting: {} ({:.0}s)", recipe.name, recipe.craft_time),
                        duration: 2.5,
                    });
                }
                Err(msg) => {
                    msg_ev.send(UiMessageEvent {
                        text: format!("Can't craft: {}", msg),
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
            commands.entity(e).despawn_recursive();
        }
        for e in hud_root.iter() {
            commands.entity(e).despawn_recursive();
        }
        next_state.set(AppState::MainMenu);
    }
}
