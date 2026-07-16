/// Unified input for up to 4 local players.
///
/// Each frame `update_player_inputs` writes a `PlayerInput` on every live
/// player entity:
///   P1 — keyboard + mouse, plus the controller that claims P1
///   P2–P4 — controllers explicitly assigned by pressing Start in the lobby
///
/// Controller layout (works for Xbox, DualSense, DualShock, 8BitDo, Switch Pro,
/// Stadia, and any HID-compliant gamepad via Bevy's gilrs backend):
///
///  Left stick        — move
///  Right stick       — look (quadratic curve applied)
///  LT  (LTrigger2)  — aim
///  RT  (RTrigger2)  — fire
///  LB  (LTrigger)   — sprint
///  RB  (RTrigger)   — weapon next
///  A / Cross / B    — jump                     (South)
///  B / Circle / A   — dodge                    (East)
///  X / Square / Y   — reload; LB+X uses equipped quick item (West)
///  Y / Triangle / X — parry                    (North)
///  LB + Y / Triangle — toggle Star Sabre; RT swings it
///  (LT + Y and Guide/Home remain alternate mappings)
///  L3 (left click)  — melee heavy              (LeftThumb)
///  R3 (right click) — melee light              (RightThumb)
///  Select/Back/Share/View — crafting (alone), loadout (LB+Select), or special modifier (+ DPad)
///  Start/Options/Menu     — pause              (Start)
///  Guide/Home             — sabre toggle       (Mode)  [fallback: L3+R3]
///  G / Select+RB      — grapple hook foundation
///  DPad Up           — enter vehicle
///  DPad Down         — interact
///  DPad Left         — weapon prev
///  DPad Right        — open map
///
///  Select + DPad (held Select, then tap DPad):
///    Up → select tracking missile | Down → special slot 1
///    Left → special slot 2 | Right → special slot 3
use bevy::input::gamepad::{
    Gamepad, GamepadAxis, GamepadButton, GamepadButtonStateChangedEvent, GamepadConnection,
    GamepadConnectionEvent,
};
use bevy::input::mouse::MouseMotion;
use bevy::input::ButtonState;
use bevy::input::InputSystems;
use bevy::prelude::*;

use crate::components::player::{Player, PlayerIndex, PlayerInput};
use crate::engine_tools::EngineToolMode;
use crate::resources::{GameSettings, UiGameplayCapture};

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .add_systems(PreUpdate, poll_native_controllers.after(InputSystems))
            .add_systems(
                PreUpdate,
                update_player_inputs
                    .after(poll_native_controllers)
                    .run_if(in_state(EngineToolMode::Playing)),
            )
            .add_systems(
                PreUpdate,
                clear_player_inputs
                    .after(poll_native_controllers)
                    .run_if(in_state(EngineToolMode::Editing)),
            )
            .add_systems(
                Update,
                (log_gamepad_connections, release_disconnected_gamepads),
            );
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct GamepadAssignments {
    players: [Option<Entity>; 4],
    native_player: Option<u8>,
}

impl GamepadAssignments {
    pub fn gamepad_for_player(&self, player_index: u8) -> Option<Entity> {
        self.players.get(player_index as usize).copied().flatten()
    }

    pub fn player_for_gamepad(&self, gamepad: Entity) -> Option<u8> {
        self.players
            .iter()
            .position(|assigned| *assigned == Some(gamepad))
            .map(|index| index as u8)
    }

    pub fn assign_next(&mut self, gamepad: Entity, joined: &[bool; 4]) -> Option<u8> {
        if let Some(player) = self.player_for_gamepad(gamepad) {
            return Some(player);
        }
        let slot = (0..4)
            .find(|index| joined[*index] && self.players[*index].is_none())
            .or_else(|| (0..4).find(|index| self.players[*index].is_none()))?;
        self.players[slot] = Some(gamepad);
        Some(slot as u8)
    }

    pub fn assign_native_next(&mut self, joined: &[bool; 4]) -> Option<u8> {
        if self.native_player.is_some() {
            return self.native_player;
        }
        let slot = (0..4)
            .find(|index| joined[*index] && self.players[*index].is_none())
            .or_else(|| (0..4).find(|index| self.players[*index].is_none()))?;
        self.native_player = Some(slot as u8);
        Some(slot as u8)
    }

    pub fn native_player(&self) -> Option<u8> {
        self.native_player
    }

    fn release(&mut self, gamepad: Entity) {
        for assigned in &mut self.players {
            if *assigned == Some(gamepad) {
                *assigned = None;
            }
        }
    }
}

/// Editor input is intentionally destructive: held gameplay actions from the
/// previous frame must not survive when the authoring workspace takes control.
fn clear_player_inputs(mut players: Query<&mut PlayerInput, With<Player>>) {
    for mut input in &mut players {
        *input = PlayerInput::default();
    }
}

// ── Tuning ────────────────────────────────────────────────────────────────────
/// Minimum stick deflection before any output is produced.
const DEADZONE: f32 = 0.18;
/// Look rate in radians per second at full stick deflection.
const STICK_LOOK_RATE: f32 = std::f32::consts::PI * 1.5;
/// Analog trigger deflection required to count as a pressed LT/RT action.
const TRIGGER_AXIS_THRESHOLD: f32 = 0.35;

#[derive(Debug, Clone, Copy, Default)]
struct TriggerAxisState {
    left: bool,
    right: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum NativeButton {
    South,
    East,
    West,
    North,
    Start,
    Select,
    LeftShoulder,
    RightShoulder,
    LeftTrigger,
    RightTrigger,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl NativeButton {
    const fn mask(self) -> u32 {
        1 << (self as u32)
    }
}

#[derive(Resource, Debug, Clone)]
pub struct NativeControllerState {
    pub connected: bool,
    pub name: String,
    pub move_axis: Vec2,
    pub look_axis: Vec2,
    pressed_mask: u32,
    just_pressed_mask: u32,
}

impl Default for NativeControllerState {
    fn default() -> Self {
        Self {
            connected: false,
            name: String::new(),
            move_axis: Vec2::ZERO,
            look_axis: Vec2::ZERO,
            pressed_mask: 0,
            just_pressed_mask: 0,
        }
    }
}

impl NativeControllerState {
    pub fn pressed(&self, button: NativeButton) -> bool {
        self.pressed_mask & button.mask() != 0
    }

    pub fn just_pressed(&self, button: NativeButton) -> bool {
        self.just_pressed_mask & button.mask() != 0
    }

    pub fn start_or_confirm_just_pressed(&self) -> bool {
        self.just_pressed(NativeButton::Start)
            || self.just_pressed(NativeButton::South)
            || self.just_pressed(NativeButton::East)
            || self.just_pressed(NativeButton::West)
            || self.just_pressed(NativeButton::North)
    }

    fn finish_poll(
        &mut self,
        connected: bool,
        name: String,
        move_axis: Vec2,
        look_axis: Vec2,
        pressed_mask: u32,
    ) {
        let previous_connected = self.connected;
        let previous_name = self.name.clone();
        let previous = self.pressed_mask;
        self.connected = connected;
        self.name = name;
        self.move_axis = move_axis;
        self.look_axis = look_axis;
        self.pressed_mask = pressed_mask;
        self.just_pressed_mask = pressed_mask & !previous;

        if connected && (!previous_connected || previous_name != self.name) {
            info!("Native macOS controller connected: {}", self.name);
        } else if previous_connected && !connected {
            info!("Native macOS controller disconnected");
        }
    }

    fn clear_poll(&mut self) {
        self.finish_poll(false, String::new(), Vec2::ZERO, Vec2::ZERO, 0);
    }
}

fn poll_native_controllers(mut native: ResMut<NativeControllerState>) {
    native_controller_backend::poll(&mut native);
}

// ── Deadzone — normalized circular remapping ──────────────────────────────────
// Without normalization the stick has a dead zone, then a sudden jump to 0.18
// at the boundary.  Remapping maps the live zone [DEADZONE, 1.0] → [0.0, 1.0]
// so the output grows smoothly from zero the moment you pass the threshold.
#[inline]
fn apply_deadzone(v: Vec2) -> Vec2 {
    let len = v.length();
    if len < DEADZONE {
        Vec2::ZERO
    } else {
        v * ((len - DEADZONE) / ((1.0 - DEADZONE) * len))
    }
}

// ── Connection logging ────────────────────────────────────────────────────────
fn log_gamepad_connections(mut events: MessageReader<GamepadConnectionEvent>) {
    for ev in events.read() {
        match &ev.connection {
            GamepadConnection::Connected {
                name,
                vendor_id,
                product_id,
            } => {
                info!(
                    "Gamepad connected: {:?} ({name}, vendor: {:?}, product: {:?})",
                    ev.gamepad, vendor_id, product_id
                );
            }
            GamepadConnection::Disconnected => {
                info!("Gamepad disconnected: {:?}", ev.gamepad);
            }
        }
    }
}

fn release_disconnected_gamepads(
    mut events: MessageReader<GamepadConnectionEvent>,
    mut assignments: ResMut<GamepadAssignments>,
) {
    for event in events.read() {
        if matches!(event.connection, GamepadConnection::Disconnected) {
            assignments.release(event.gamepad);
        }
    }
}

// ── Main input system ─────────────────────────────────────────────────────────
#[allow(clippy::too_many_arguments)]
fn update_player_inputs(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_btn: Res<ButtonInput<MouseButton>>,
    mut mouse_ev: MessageReader<MouseMotion>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut button_events: MessageReader<GamepadButtonStateChangedEvent>,
    time: Res<Time>,
    settings: Res<GameSettings>,
    capture: Res<UiGameplayCapture>,
    native: Res<NativeControllerState>,
    assignments: Res<GamepadAssignments>,
    mut trigger_history: Local<[TriggerAxisState; 4]>,
    mut players: Query<(&PlayerIndex, &mut PlayerInput), With<Player>>,
) {
    let dt = time.delta_secs();
    let sens = settings.mouse_sensitivity;

    // Accumulate mouse delta for the whole frame.
    let mut raw_mouse = Vec2::ZERO;
    for ev in mouse_ev.read() {
        raw_mouse += ev.delta;
    }
    let mouse_look = raw_mouse * sens;

    let pressed_button_events: Vec<(Entity, GamepadButton)> = button_events
        .read()
        .filter(|event| event.state == ButtonState::Pressed)
        .map(|event| (event.entity, event.button))
        .collect();

    for (idx, mut pi) in players.iter_mut() {
        *pi = PlayerInput::default();

        let i = idx.0 as usize;
        let history_slot = i.min(trigger_history.len() - 1);
        let gp_entity = assignments.gamepad_for_player(idx.0);
        let gp: Option<&Gamepad> =
            gp_entity.and_then(|entity| gamepads.get(entity).ok().map(|v| v.1));
        let is_p1 = i == 0;
        // On macOS, Bevy can report a controller while its SDL/gilrs button
        // mapping is still wrong for a specific device. Merge the native
        // GameController reading for P1 whenever it exists so correctly mapped
        // native buttons can rescue those cases.
        let use_native = native.connected && assignments.native_player() == Some(idx.0);

        pi.gamepad_active = gp.is_some() || use_native;

        // Helper closures — all capture `gp`.
        let btn_held = |b: GamepadButton| gp.map(|g| g.pressed(b)).unwrap_or(false);
        let event_just = |b: GamepadButton| {
            gp_entity.is_some_and(|entity| {
                pressed_button_events
                    .iter()
                    .any(|(event_entity, event_button)| {
                        *event_entity == entity && *event_button == b
                    })
            })
        };
        let btn_just =
            |b: GamepadButton| gp.map(|g| g.just_pressed(b)).unwrap_or(false) || event_just(b);
        let axis_val = |a: GamepadAxis| -> f32 { gp.and_then(|g| g.get(a)).unwrap_or(0.0) };
        let native_held = |b: NativeButton| -> bool { use_native && native.pressed(b) };
        let native_just = |b: NativeButton| -> bool { use_native && native.just_pressed(b) };
        let left_trigger_axis = axis_pressed(axis_val(GamepadAxis::LeftZ));
        let right_trigger_axis = axis_pressed(axis_val(GamepadAxis::RightZ));
        let left_trigger_held = btn_held(GamepadButton::LeftTrigger2)
            || left_trigger_axis
            || native_held(NativeButton::LeftTrigger);
        let right_trigger_held = btn_held(GamepadButton::RightTrigger2)
            || right_trigger_axis
            || native_held(NativeButton::RightTrigger);
        let right_trigger_just = btn_just(GamepadButton::RightTrigger2)
            || native_just(NativeButton::RightTrigger)
            || (right_trigger_axis && !trigger_history[history_slot].right);
        let left_shoulder_held =
            btn_held(GamepadButton::LeftTrigger) || native_held(NativeButton::LeftShoulder);

        // ── Movement ──────────────────────────────────────────────────────────
        let raw_move = Vec2::new(
            axis_val(GamepadAxis::LeftStickX),
            axis_val(GamepadAxis::LeftStickY),
        );
        let gp_move = apply_deadzone(raw_move);
        let native_move = if use_native {
            apply_deadzone(native.move_axis)
        } else {
            Vec2::ZERO
        };
        let kb_move = if is_p1 {
            Vec2::new(
                (keyboard.pressed(KeyCode::KeyD) as i32 - keyboard.pressed(KeyCode::KeyA) as i32)
                    as f32,
                (keyboard.pressed(KeyCode::KeyW) as i32 - keyboard.pressed(KeyCode::KeyS) as i32)
                    as f32,
            )
            .normalize_or_zero()
        } else {
            Vec2::ZERO
        };
        pi.move_axis = if native_move.length_squared() > 0.001 {
            native_move
        } else if gp_move.length_squared() > 0.001 {
            gp_move
        } else {
            kb_move
        };

        // ── Look ──────────────────────────────────────────────────────────────
        let raw_look = Vec2::new(
            axis_val(GamepadAxis::RightStickX),
            -axis_val(GamepadAxis::RightStickY),
        );
        let clean = apply_deadzone(raw_look);
        let native_clean = if use_native {
            apply_deadzone(native.look_axis)
        } else {
            Vec2::ZERO
        };
        // Quadratic curve: more precision at low deflection, full speed at max.
        let controller_look_axis = if native_clean.length_squared() > 0.001 {
            native_clean
        } else {
            clean
        };
        let curved = controller_look_axis * controller_look_axis.length();
        let controller_look = curved * STICK_LOOK_RATE * dt;
        // P1 gets both mouse and gamepad look simultaneously.
        pi.look_delta = if is_p1 { mouse_look } else { Vec2::ZERO } + controller_look;

        // ── Fire ──────────────────────────────────────────────────────────────
        pi.fire = (is_p1 && mouse_btn.pressed(MouseButton::Left)) || right_trigger_held;
        pi.fire_just = (is_p1 && mouse_btn.just_pressed(MouseButton::Left)) || right_trigger_just;

        // ── Aim ───────────────────────────────────────────────────────────────
        pi.aim = (is_p1 && mouse_btn.pressed(MouseButton::Right)) || left_trigger_held;

        // ── Sprint ────────────────────────────────────────────────────────────
        pi.sprint = (is_p1
            && (keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)))
            || left_shoulder_held;

        // ── Jump / Jetpack ────────────────────────────────────────────────────
        pi.jump = (is_p1 && keyboard.just_pressed(KeyCode::Space))
            || btn_just(GamepadButton::South)
            || native_just(NativeButton::South);
        pi.jetpack = (is_p1 && keyboard.pressed(KeyCode::Space))
            || btn_held(GamepadButton::South)
            || native_held(NativeButton::South);

        // ── Dodge ─────────────────────────────────────────────────────────────
        pi.dodge = (is_p1 && keyboard.just_pressed(KeyCode::KeyQ))
            || btn_just(GamepadButton::East)
            || native_just(NativeButton::East);

        // ── Reload ────────────────────────────────────────────────────────────
        let west_just = btn_just(GamepadButton::West) || native_just(NativeButton::West);
        pi.use_quick_item =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyH)) || (left_shoulder_held && west_just);
        pi.reload =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyR)) || (!left_shoulder_held && west_just);

        // ── Parry ─────────────────────────────────────────────────────────────
        pi.parry = (is_p1 && keyboard.just_pressed(KeyCode::KeyF))
            || (!left_trigger_held
                && !left_shoulder_held
                && (btn_just(GamepadButton::North) || native_just(NativeButton::North)));

        // ── Melee ─────────────────────────────────────────────────────────────
        pi.melee_light = (is_p1 && keyboard.just_pressed(KeyCode::KeyV))
            || btn_just(GamepadButton::RightThumb)
            || native_just(NativeButton::RightThumb);
        pi.melee_heavy = (is_p1 && keyboard.just_pressed(KeyCode::KeyB))
            || btn_just(GamepadButton::LeftThumb)
            || native_just(NativeButton::LeftThumb);

        let select_held = btn_held(GamepadButton::Select) || native_held(NativeButton::Select);
        let grapple_button_held =
            btn_held(GamepadButton::RightTrigger) || native_held(NativeButton::RightShoulder);
        let grapple_button_just =
            btn_just(GamepadButton::RightTrigger) || native_just(NativeButton::RightShoulder);
        pi.grapple =
            (is_p1 && keyboard.pressed(KeyCode::KeyG)) || (select_held && grapple_button_held);
        pi.grapple_just =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyG)) || (select_held && grapple_button_just);

        // ── Weapon cycle ──────────────────────────────────────────────────────
        let armor_keyboard_modifier =
            keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
        pi.weapon_next =
            (is_p1 && !armor_keyboard_modifier && keyboard.just_pressed(KeyCode::BracketRight))
                || (!select_held
                    && (btn_just(GamepadButton::RightTrigger)
                        || native_just(NativeButton::RightShoulder))); // RB only — DPadRight freed for open_map
        pi.weapon_prev =
            is_p1 && !armor_keyboard_modifier && keyboard.just_pressed(KeyCode::BracketLeft);

        // ── Direct weapon slots (P1 keyboard) ────────────────────────────────
        pi.weapon_slot = if is_p1 {
            if keyboard.just_pressed(KeyCode::Digit1) {
                Some(0)
            } else if keyboard.just_pressed(KeyCode::Digit2) {
                Some(1)
            } else if keyboard.just_pressed(KeyCode::Digit3) {
                Some(2)
            } else if keyboard.just_pressed(KeyCode::Digit4) {
                Some(3)
            } else if keyboard.just_pressed(KeyCode::Digit5) {
                Some(4)
            } else if keyboard.just_pressed(KeyCode::Digit6) {
                Some(5)
            } else {
                None
            }
        } else {
            None
        };

        // ── Special weapon slots ──────────────────────────────────────────────
        // Keyboard (P1): digits 7–0.
        // Controller: hold Select then tap a DPad direction.
        //   Select alone (no DPad) still fires crafting below.
        let dpad_any_just = btn_just(GamepadButton::DPadUp)
            || btn_just(GamepadButton::DPadDown)
            || btn_just(GamepadButton::DPadLeft)
            || btn_just(GamepadButton::DPadRight)
            || native_just(NativeButton::DPadUp)
            || native_just(NativeButton::DPadDown)
            || native_just(NativeButton::DPadLeft)
            || native_just(NativeButton::DPadRight);

        pi.special_slot = if is_p1 {
            if keyboard.just_pressed(KeyCode::Digit7) {
                Some(0)
            } else if keyboard.just_pressed(KeyCode::Digit8) {
                Some(1)
            } else if keyboard.just_pressed(KeyCode::Digit9) {
                Some(2)
            } else if keyboard.just_pressed(KeyCode::Digit0) {
                Some(3)
            } else if select_held
                && (btn_just(GamepadButton::DPadUp) || native_just(NativeButton::DPadUp))
            {
                Some(0)
            } else if select_held
                && (btn_just(GamepadButton::DPadDown) || native_just(NativeButton::DPadDown))
            {
                Some(1)
            } else if select_held
                && (btn_just(GamepadButton::DPadLeft) || native_just(NativeButton::DPadLeft))
            {
                Some(2)
            } else if select_held
                && (btn_just(GamepadButton::DPadRight) || native_just(NativeButton::DPadRight))
            {
                Some(3)
            } else {
                None
            }
        } else if select_held && dpad_any_just {
            if btn_just(GamepadButton::DPadUp) || native_just(NativeButton::DPadUp) {
                Some(0)
            } else if btn_just(GamepadButton::DPadDown) || native_just(NativeButton::DPadDown) {
                Some(1)
            } else if btn_just(GamepadButton::DPadLeft) || native_just(NativeButton::DPadLeft) {
                Some(2)
            } else {
                Some(3)
            }
        } else {
            None
        };

        // ── DPad navigation (normal — skipped when Select modifier active) ────
        let armor_controller_modifier = left_trigger_held && !select_held;
        pi.armor_element_delta = if is_p1
            && armor_keyboard_modifier
            && keyboard.just_pressed(KeyCode::BracketLeft)
        {
            -1
        } else if is_p1 && armor_keyboard_modifier && keyboard.just_pressed(KeyCode::BracketRight) {
            1
        } else if armor_controller_modifier
            && (btn_just(GamepadButton::DPadLeft) || native_just(NativeButton::DPadLeft))
        {
            -1
        } else if armor_controller_modifier
            && (btn_just(GamepadButton::DPadRight) || native_just(NativeButton::DPadRight))
        {
            1
        } else {
            0
        };

        let dpad_free = !select_held && !armor_controller_modifier;

        pi.interact = (is_p1 && keyboard.just_pressed(KeyCode::KeyE))
            || (dpad_free
                && (btn_just(GamepadButton::DPadDown) || native_just(NativeButton::DPadDown)));

        pi.enter_vehicle = (is_p1 && keyboard.just_pressed(KeyCode::KeyJ))
            || (dpad_free
                && (btn_just(GamepadButton::DPadUp) || native_just(NativeButton::DPadUp)));

        pi.weapon_prev = pi.weapon_prev
            || (dpad_free
                && (btn_just(GamepadButton::DPadLeft) || native_just(NativeButton::DPadLeft)));

        // ── Open map ──────────────────────────────────────────────────────────
        // DPadRight freed from weapon_next (RB covers that); assigned to map.
        pi.open_map = (is_p1 && keyboard.just_pressed(KeyCode::KeyM))
            || (dpad_free
                && (btn_just(GamepadButton::DPadRight) || native_just(NativeButton::DPadRight)));

        // ── In-game equipment menus ───────────────────────────────────────────
        // Select alone opens crafting. LB+Select opens the unified loadout;
        // Select+DPad remains the direct special-slot chord.
        pi.loadout_menu = (is_p1 && keyboard.just_pressed(KeyCode::KeyI))
            || (left_shoulder_held
                && (btn_just(GamepadButton::Select) || native_just(NativeButton::Select)));
        pi.crafting = (is_p1 && keyboard.just_pressed(KeyCode::KeyC))
            || ((btn_just(GamepadButton::Select) || native_just(NativeButton::Select))
                && !dpad_any_just
                && !left_shoulder_held);

        // ── Pause ─────────────────────────────────────────────────────────────
        pi.pause = (is_p1 && keyboard.just_pressed(KeyCode::Escape))
            || btn_just(GamepadButton::Start)
            || native_just(NativeButton::Start);

        // ── Star Sabre toggle ─────────────────────────────────────────────────
        // Primary: LB + Y / Triangle is entirely digital and works on pads
        // whose analog-trigger or Guide mapping is unavailable.
        // Alternate: LT + Y, Guide/Home, or L3+R3.
        //   Note: on Windows the Guide button is handled by the OS in some
        //   configurations. The L3+R3 combo is a reliable cross-platform fallback.
        // Fallback: press Left Stick + Right Stick simultaneously.
        let l3r3 = btn_just(GamepadButton::LeftThumb) && btn_held(GamepadButton::RightThumb)
            || btn_just(GamepadButton::RightThumb) && btn_held(GamepadButton::LeftThumb);
        pi.sabre_toggle = (is_p1 && keyboard.just_pressed(KeyCode::KeyT))
            || (left_shoulder_held
                && (btn_just(GamepadButton::North) || native_just(NativeButton::North)))
            || (left_trigger_held
                && (btn_just(GamepadButton::North) || native_just(NativeButton::North)))
            || btn_just(GamepadButton::Mode)
            || l3r3
            || (native_just(NativeButton::LeftThumb) && native_held(NativeButton::RightThumb))
            || (native_just(NativeButton::RightThumb) && native_held(NativeButton::LeftThumb));

        pi.ui_vertical = pi.move_axis.y;
        pi.ui_up = (is_p1 && keyboard.just_pressed(KeyCode::ArrowUp))
            || btn_just(GamepadButton::DPadUp)
            || native_just(NativeButton::DPadUp);
        pi.ui_down = (is_p1 && keyboard.just_pressed(KeyCode::ArrowDown))
            || btn_just(GamepadButton::DPadDown)
            || native_just(NativeButton::DPadDown);
        pi.ui_left = (is_p1 && keyboard.just_pressed(KeyCode::ArrowLeft))
            || btn_just(GamepadButton::DPadLeft)
            || native_just(NativeButton::DPadLeft);
        pi.ui_right = (is_p1 && keyboard.just_pressed(KeyCode::ArrowRight))
            || btn_just(GamepadButton::DPadRight)
            || native_just(NativeButton::DPadRight);
        pi.ui_confirm = (is_p1 && keyboard.just_pressed(KeyCode::Enter))
            || btn_just(GamepadButton::South)
            || native_just(NativeButton::South);

        if capture.owner == Some(idx.0) {
            suppress_gameplay_for_ui(&mut pi);
        }

        trigger_history[history_slot].left = left_trigger_held;
        trigger_history[history_slot].right = right_trigger_held;
    }
}

fn suppress_gameplay_for_ui(input: &mut PlayerInput) {
    let preserved = (
        input.gamepad_active,
        input.crafting,
        input.loadout_menu,
        input.ui_vertical,
        input.ui_up,
        input.ui_down,
        input.ui_left,
        input.ui_right,
        input.ui_confirm,
    );
    *input = PlayerInput::default();
    (
        input.gamepad_active,
        input.crafting,
        input.loadout_menu,
        input.ui_vertical,
        input.ui_up,
        input.ui_down,
        input.ui_left,
        input.ui_right,
        input.ui_confirm,
    ) = preserved;
}

#[inline]
fn axis_pressed(value: f32) -> bool {
    value > TRIGGER_AXIS_THRESHOLD
}

/// No-op rumble trigger seam — wires up when Bevy adds native rumble API.
pub fn trigger_player_rumble(_player_index: usize, _strength: f32) {
    // Reserved for future Bevy gamepad rumble support.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadzone_remaps_stick_smoothly_from_zero() {
        assert_eq!(apply_deadzone(Vec2::new(0.05, 0.0)), Vec2::ZERO);

        let live = apply_deadzone(Vec2::new(0.59, 0.0));

        assert!(live.x > 0.45 && live.x < 0.55);
        assert_eq!(live.y, 0.0);
    }

    #[test]
    fn trigger_axis_threshold_matches_controller_action_gate() {
        assert!(!axis_pressed(TRIGGER_AXIS_THRESHOLD));
        assert!(axis_pressed(TRIGGER_AXIS_THRESHOLD + 0.01));
    }

    #[test]
    fn modal_ui_capture_preserves_navigation_but_clears_gameplay() {
        let mut input = PlayerInput {
            move_axis: Vec2::ONE,
            fire: true,
            jump: true,
            enter_vehicle: true,
            crafting: true,
            loadout_menu: true,
            ui_vertical: -1.0,
            ui_down: true,
            ui_right: true,
            ui_confirm: true,
            ..default()
        };
        suppress_gameplay_for_ui(&mut input);
        assert_eq!(input.move_axis, Vec2::ZERO);
        assert!(!input.fire);
        assert!(!input.jump);
        assert!(!input.enter_vehicle);
        assert!(input.crafting);
        assert!(input.loadout_menu);
        assert_eq!(input.ui_vertical, -1.0);
        assert!(input.ui_down);
        assert!(input.ui_right);
        assert!(input.ui_confirm);
    }

    #[test]
    fn start_assigns_controllers_to_next_available_players() {
        let mut world = World::new();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        let mut assignments = GamepadAssignments::default();
        let joined = [true, false, false, false];

        assert_eq!(assignments.assign_next(first, &joined), Some(0));
        assert_eq!(assignments.assign_next(second, &joined), Some(1));
        assert_eq!(assignments.gamepad_for_player(0), Some(first));
        assert_eq!(assignments.gamepad_for_player(1), Some(second));
        assert_eq!(assignments.assign_next(first, &joined), Some(0));
    }

    #[test]
    fn released_controller_can_reclaim_joined_slot() {
        let mut world = World::new();
        let original = world.spawn_empty().id();
        let reconnected = world.spawn_empty().id();
        let mut assignments = GamepadAssignments::default();
        assert_eq!(
            assignments.assign_next(original, &[true, true, false, false]),
            Some(0)
        );
        assignments.release(original);
        assert_eq!(
            assignments.assign_next(reconnected, &[true, true, false, false]),
            Some(0)
        );
    }
}

#[cfg(target_os = "macos")]
mod native_controller_backend {
    use super::{NativeButton, NativeControllerState};
    use bevy::prelude::{info, Vec2};
    use objc2::runtime::{AnyClass, AnyObject, Sel};
    use objc2::{msg_send, sel};
    use std::ffi::CStr;
    use std::os::raw::c_char;
    use std::ptr;
    use std::sync::Once;

    #[link(name = "GameController", kind = "framework")]
    extern "C" {}

    pub fn poll(state: &mut NativeControllerState) {
        objc2::rc::autoreleasepool(|_| unsafe {
            poll_impl(state);
        });
    }

    unsafe fn poll_impl(state: &mut NativeControllerState) {
        let Some(controller_class) = AnyClass::get(c"GCController") else {
            state.clear_poll();
            return;
        };
        unsafe {
            start_controller_discovery(controller_class);
        }

        let controllers: *mut AnyObject = unsafe { msg_send![controller_class, controllers] };
        if controllers.is_null() {
            state.clear_poll();
            return;
        }

        let count: usize = unsafe { msg_send![controllers, count] };
        for index in 0..count {
            let controller: *mut AnyObject =
                unsafe { msg_send![controllers, objectAtIndex: index] };
            if controller.is_null() {
                continue;
            }
            let extended = unsafe { object_property(controller, sel!(extendedGamepad)) };
            if extended.is_null() {
                continue;
            }

            let name = unsafe { nsstring_to_string(object_property(controller, sel!(vendorName))) }
                .unwrap_or_else(|| "macOS GameController".to_string());
            let left =
                unsafe { direction_pad_axes(object_property(extended, sel!(leftThumbstick))) };
            let right =
                unsafe { direction_pad_axes(object_property(extended, sel!(rightThumbstick))) };
            let dpad = unsafe { direction_pad_axes(object_property(extended, sel!(dpad))) };

            let mut mask = 0;
            unsafe {
                set_button(
                    &mut mask,
                    NativeButton::South,
                    button_pressed(object_property(extended, sel!(buttonA))),
                );
                set_button(
                    &mut mask,
                    NativeButton::East,
                    button_pressed(object_property(extended, sel!(buttonB))),
                );
                set_button(
                    &mut mask,
                    NativeButton::West,
                    button_pressed(object_property(extended, sel!(buttonX))),
                );
                set_button(
                    &mut mask,
                    NativeButton::North,
                    button_pressed(object_property(extended, sel!(buttonY))),
                );
                set_button(
                    &mut mask,
                    NativeButton::LeftShoulder,
                    button_pressed(object_property(extended, sel!(leftShoulder))),
                );
                set_button(
                    &mut mask,
                    NativeButton::RightShoulder,
                    button_pressed(object_property(extended, sel!(rightShoulder))),
                );
                set_button(
                    &mut mask,
                    NativeButton::LeftTrigger,
                    button_pressed(object_property(extended, sel!(leftTrigger))),
                );
                set_button(
                    &mut mask,
                    NativeButton::RightTrigger,
                    button_pressed(object_property(extended, sel!(rightTrigger))),
                );
                set_button(
                    &mut mask,
                    NativeButton::LeftThumb,
                    button_pressed(object_property(extended, sel!(leftThumbstickButton))),
                );
                set_button(
                    &mut mask,
                    NativeButton::RightThumb,
                    button_pressed(object_property(extended, sel!(rightThumbstickButton))),
                );
                set_button(
                    &mut mask,
                    NativeButton::Start,
                    button_pressed(object_property(extended, sel!(buttonMenu)))
                        || button_pressed(object_property(extended, sel!(buttonHome))),
                );
                set_button(
                    &mut mask,
                    NativeButton::Select,
                    button_pressed(object_property(extended, sel!(buttonOptions))),
                );
            }
            set_button(&mut mask, NativeButton::DPadLeft, dpad.x < -0.5);
            set_button(&mut mask, NativeButton::DPadRight, dpad.x > 0.5);
            set_button(&mut mask, NativeButton::DPadDown, dpad.y < -0.5);
            set_button(&mut mask, NativeButton::DPadUp, dpad.y > 0.5);

            state.finish_poll(true, name, left, Vec2::new(right.x, -right.y), mask);
            return;
        }

        state.clear_poll();
    }

    unsafe fn start_controller_discovery(controller_class: &AnyClass) {
        static START_DISCOVERY: Once = Once::new();
        START_DISCOVERY.call_once(|| {
            let can_start: bool = unsafe {
                msg_send![
                    controller_class,
                    respondsToSelector: sel!(startWirelessControllerDiscoveryWithCompletionHandler:)
                ]
            };
            if can_start {
                let _: () = unsafe {
                    msg_send![
                        controller_class,
                        startWirelessControllerDiscoveryWithCompletionHandler: ptr::null_mut::<AnyObject>()
                    ]
                };
                info!("Started macOS wireless controller discovery.");
            }
        });
    }

    fn set_button(mask: &mut u32, button: NativeButton, pressed: bool) {
        if pressed {
            *mask |= button.mask();
        }
    }

    unsafe fn responds_to(receiver: *mut AnyObject, selector: Sel) -> bool {
        if receiver.is_null() {
            return false;
        }
        unsafe { msg_send![receiver, respondsToSelector: selector] }
    }

    unsafe fn object_property(receiver: *mut AnyObject, selector: Sel) -> *mut AnyObject {
        if !unsafe { responds_to(receiver, selector) } {
            return ptr::null_mut();
        }
        unsafe { msg_send![receiver, performSelector: selector] }
    }

    unsafe fn direction_pad_axes(direction_pad: *mut AnyObject) -> Vec2 {
        if direction_pad.is_null() {
            return Vec2::ZERO;
        }
        let x_axis = unsafe { object_property(direction_pad, sel!(xAxis)) };
        let y_axis = unsafe { object_property(direction_pad, sel!(yAxis)) };
        Vec2::new(unsafe { axis_value(x_axis) }, unsafe { axis_value(y_axis) })
    }

    unsafe fn axis_value(axis: *mut AnyObject) -> f32 {
        if axis.is_null() {
            return 0.0;
        }
        unsafe { msg_send![axis, value] }
    }

    unsafe fn button_pressed(button: *mut AnyObject) -> bool {
        if button.is_null() {
            return false;
        }
        let is_pressed: bool = unsafe { msg_send![button, isPressed] };
        let value: f32 = unsafe { msg_send![button, value] };
        is_pressed || value > 0.18
    }

    unsafe fn nsstring_to_string(string: *mut AnyObject) -> Option<String> {
        if string.is_null() {
            return None;
        }
        let c_string: *const c_char = unsafe { msg_send![string, UTF8String] };
        if c_string.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(c_string) }
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod native_controller_backend {
    use super::NativeControllerState;

    pub fn poll(state: &mut NativeControllerState) {
        state.clear_poll();
    }
}
