/// Unified input for up to 4 local players.
///
/// Each frame `update_player_inputs` writes a `PlayerInput` on every live
/// player entity:
///   P1 — keyboard + mouse + gamepad 0
///   P2 — gamepad 1
///   P3 — gamepad 2
///   P4 — gamepad 3
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
///  X / Square / Y   — reload                   (West)
///  Y / Triangle / X — parry                    (North)
///  L3 (left click)  — melee heavy              (LeftThumb)
///  R3 (right click) — melee light              (RightThumb)
///  Select/Back/Share/View — crafting (alone) OR special-slot modifier (+ DPad)
///  Start/Options/Menu     — pause              (Start)
///  Guide/Home             — sabre toggle       (Mode)  [fallback: L3+R3]
///  DPad Up           — enter vehicle
///  DPad Down         — interact
///  DPad Left         — weapon prev
///  DPad Right        — open map
///
///  Select + DPad (held Select, then tap DPad):
///    Up → special slot 0 | Down → special slot 1
///    Left → special slot 2 | Right → special slot 3
use bevy::input::gamepad::{
    Gamepad, GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent,
};
use bevy::input::mouse::MouseMotion;
use bevy::input::InputSystem;
use bevy::prelude::*;

use crate::components::player::{Player, PlayerIndex, PlayerInput};
use crate::resources::GameSettings;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, update_player_inputs.after(InputSystem))
            .add_systems(Update, log_gamepad_connections);
    }
}

// ── Tuning ────────────────────────────────────────────────────────────────────
/// Minimum stick deflection before any output is produced.
const DEADZONE: f32 = 0.18;
/// Look rate in radians per second at full stick deflection.
const STICK_LOOK_RATE: f32 = std::f32::consts::PI * 1.5;

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
fn log_gamepad_connections(mut events: EventReader<GamepadConnectionEvent>) {
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

// ── Main input system ─────────────────────────────────────────────────────────
fn update_player_inputs(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_btn: Res<ButtonInput<MouseButton>>,
    mut mouse_ev: EventReader<MouseMotion>,
    gamepads: Query<(Entity, &Gamepad)>,
    time: Res<Time>,
    settings: Res<GameSettings>,
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

    // Sort by Entity::index() for a stable cross-frame assignment order.
    // Using index() (the numeric part) matches what ui_plugin does and is
    // more stable than sorting by the full Entity value across reconnects.
    let mut gps: Vec<(Entity, &Gamepad)> = gamepads.iter().collect();
    gps.sort_by_key(|(e, _)| e.index());

    for (idx, mut pi) in players.iter_mut() {
        *pi = PlayerInput::default();

        let i = idx.0 as usize;
        let gp: Option<&Gamepad> = gps.get(i).map(|(_, g)| *g);
        let is_p1 = i == 0;

        pi.gamepad_active = gp.is_some();

        // Helper closures — all capture `gp`.
        let btn_held = |b: GamepadButton| gp.map(|g| g.pressed(b)).unwrap_or(false);
        let btn_just = |b: GamepadButton| gp.map(|g| g.just_pressed(b)).unwrap_or(false);
        let axis_val = |a: GamepadAxis| -> f32 { gp.and_then(|g| g.get(a)).unwrap_or(0.0) };

        // ── Movement ──────────────────────────────────────────────────────────
        let raw_move = Vec2::new(
            axis_val(GamepadAxis::LeftStickX),
            axis_val(GamepadAxis::LeftStickY),
        );
        let gp_move = apply_deadzone(raw_move);
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
        pi.move_axis = if gp_move.length_squared() > 0.001 {
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
        // Quadratic curve: more precision at low deflection, full speed at max.
        let curved = clean * clean.length();
        let gp_look = curved * STICK_LOOK_RATE * dt;
        // P1 gets both mouse and gamepad look simultaneously.
        pi.look_delta = if is_p1 { mouse_look } else { Vec2::ZERO } + gp_look;

        // ── Fire ──────────────────────────────────────────────────────────────
        pi.fire = (is_p1 && mouse_btn.pressed(MouseButton::Left))
            || btn_held(GamepadButton::RightTrigger2);
        pi.fire_just = (is_p1 && mouse_btn.just_pressed(MouseButton::Left))
            || btn_just(GamepadButton::RightTrigger2);

        // ── Aim ───────────────────────────────────────────────────────────────
        pi.aim = (is_p1 && mouse_btn.pressed(MouseButton::Right))
            || btn_held(GamepadButton::LeftTrigger2);

        // ── Sprint ────────────────────────────────────────────────────────────
        pi.sprint = (is_p1
            && (keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)))
            || btn_held(GamepadButton::LeftTrigger);

        // ── Jump / Jetpack ────────────────────────────────────────────────────
        pi.jump =
            (is_p1 && keyboard.just_pressed(KeyCode::Space)) || btn_just(GamepadButton::South);
        pi.jetpack = (is_p1 && keyboard.pressed(KeyCode::Space)) || btn_held(GamepadButton::South);

        // ── Dodge ─────────────────────────────────────────────────────────────
        pi.dodge = (is_p1 && keyboard.just_pressed(KeyCode::KeyQ)) || btn_just(GamepadButton::East);

        // ── Reload ────────────────────────────────────────────────────────────
        pi.reload =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyR)) || btn_just(GamepadButton::West);

        // ── Parry ─────────────────────────────────────────────────────────────
        pi.parry =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyF)) || btn_just(GamepadButton::North);

        // ── Melee ─────────────────────────────────────────────────────────────
        pi.melee_light =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyV)) || btn_just(GamepadButton::RightThumb);
        pi.melee_heavy =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyB)) || btn_just(GamepadButton::LeftThumb);

        // ── Weapon cycle ──────────────────────────────────────────────────────
        pi.weapon_next = (is_p1 && keyboard.just_pressed(KeyCode::BracketRight))
            || btn_just(GamepadButton::RightTrigger); // RB only — DPadRight freed for open_map
        pi.weapon_prev = is_p1 && keyboard.just_pressed(KeyCode::BracketLeft);

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
        let select_held = btn_held(GamepadButton::Select);
        let dpad_any_just = btn_just(GamepadButton::DPadUp)
            || btn_just(GamepadButton::DPadDown)
            || btn_just(GamepadButton::DPadLeft)
            || btn_just(GamepadButton::DPadRight);

        pi.special_slot = if is_p1 {
            if keyboard.just_pressed(KeyCode::Digit7) {
                Some(0)
            } else if keyboard.just_pressed(KeyCode::Digit8) {
                Some(1)
            } else if keyboard.just_pressed(KeyCode::Digit9) {
                Some(2)
            } else if keyboard.just_pressed(KeyCode::Digit0) {
                Some(3)
            } else if select_held && btn_just(GamepadButton::DPadUp) {
                Some(0)
            } else if select_held && btn_just(GamepadButton::DPadDown) {
                Some(1)
            } else if select_held && btn_just(GamepadButton::DPadLeft) {
                Some(2)
            } else if select_held && btn_just(GamepadButton::DPadRight) {
                Some(3)
            } else {
                None
            }
        } else if select_held && dpad_any_just {
            if btn_just(GamepadButton::DPadUp) {
                Some(0)
            } else if btn_just(GamepadButton::DPadDown) {
                Some(1)
            } else if btn_just(GamepadButton::DPadLeft) {
                Some(2)
            } else {
                Some(3)
            }
        } else {
            None
        };

        // ── DPad navigation (normal — skipped when Select modifier active) ────
        let dpad_free = !select_held;

        pi.interact = (is_p1 && keyboard.just_pressed(KeyCode::KeyE))
            || (dpad_free && btn_just(GamepadButton::DPadDown));

        pi.enter_vehicle = (is_p1 && keyboard.just_pressed(KeyCode::KeyJ))
            || (dpad_free && btn_just(GamepadButton::DPadUp));

        pi.weapon_prev = pi.weapon_prev || (dpad_free && btn_just(GamepadButton::DPadLeft));

        // ── Open map ──────────────────────────────────────────────────────────
        // DPadRight freed from weapon_next (RB covers that); assigned to map.
        pi.open_map = (is_p1 && keyboard.just_pressed(KeyCode::KeyM))
            || (dpad_free && btn_just(GamepadButton::DPadRight));

        // ── Crafting ──────────────────────────────────────────────────────────
        // Select alone fires crafting; Select+DPad is the special-slot modifier.
        pi.crafting = (is_p1 && keyboard.just_pressed(KeyCode::KeyC))
            || (btn_just(GamepadButton::Select) && !dpad_any_just);

        // ── Pause ─────────────────────────────────────────────────────────────
        pi.pause =
            (is_p1 && keyboard.just_pressed(KeyCode::Escape)) || btn_just(GamepadButton::Start);

        // ── Star Sabre toggle ─────────────────────────────────────────────────
        // Primary:  Guide/Home button (Xbox/PS/Switch).
        //   Note: on Windows the Guide button is handled by the OS in some
        //   configurations. The L3+R3 combo is a reliable cross-platform fallback.
        // Fallback: press Left Stick + Right Stick simultaneously.
        let l3r3 = btn_just(GamepadButton::LeftThumb) && btn_held(GamepadButton::RightThumb)
            || btn_just(GamepadButton::RightThumb) && btn_held(GamepadButton::LeftThumb);
        pi.sabre_toggle = (is_p1 && keyboard.just_pressed(KeyCode::KeyT))
            || btn_just(GamepadButton::Mode)
            || l3r3;
    }
}
