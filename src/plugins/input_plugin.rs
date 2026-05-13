/// Unified input for up to 4 local players.
///
/// Each frame, `update_player_inputs` writes a `PlayerInput` component on
/// every live player entity:
///   P1 (PlayerIndex 0) — keyboard + mouse + gamepad 0
///   P2 (PlayerIndex 1) — gamepad 1
///   P3 (PlayerIndex 2) — gamepad 2
///   P4 (PlayerIndex 3) — gamepad 3
///
/// Bevy 0.15 gamepad notes:
///   `Gamepad` is a component on entities; gamepads are sorted by Entity id
///   for a stable assignment order across reconnects.
use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;

use crate::components::player::{Player, PlayerIndex, PlayerInput};
use crate::resources::GameSettings;

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, update_player_inputs);
    }
}

// ── Tuning ────────────────────────────────────────────────────────────────────
const DEADZONE: f32 = 0.18;
const STICK_LOOK_RATE: f32 = std::f32::consts::PI * 1.5; // 270°/s at full deflection

// ── System ────────────────────────────────────────────────────────────────────
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

    // Accumulate mouse delta for this frame.
    let mut raw_mouse = Vec2::ZERO;
    for ev in mouse_ev.read() {
        raw_mouse += ev.delta;
    }
    let mouse_look = raw_mouse * sens;

    // Stable gamepad order — sorted by Entity so the assignment doesn't jump
    // around when controllers connect/disconnect.
    let mut gps: Vec<(Entity, &Gamepad)> = gamepads.iter().collect();
    gps.sort_by_key(|(e, _)| *e);

    let apply_dz = |v: Vec2| if v.length() < DEADZONE { Vec2::ZERO } else { v };

    for (idx, mut pi) in players.iter_mut() {
        *pi = PlayerInput::default();

        let i = idx.0 as usize;
        let gp: Option<&Gamepad> = gps.get(i).map(|(_, g)| *g);
        let is_p1 = i == 0;

        pi.gamepad_active = gp.is_some();

        let btn_held = |b: GamepadButton| gp.map(|g| g.pressed(b)).unwrap_or(false);
        let btn_just = |b: GamepadButton| gp.map(|g| g.just_pressed(b)).unwrap_or(false);
        let axis = |a: GamepadAxis| -> f32 { gp.and_then(|g| g.get(a)).unwrap_or(0.0) };

        // ── Movement ─────────────────────────────────────────────────────────
        let gp_move = apply_dz(Vec2::new(axis(GamepadAxis::LeftStickX), axis(GamepadAxis::LeftStickY)));
        let kb_move = if is_p1 {
            Vec2::new(
                (keyboard.pressed(KeyCode::KeyD) as i32 - keyboard.pressed(KeyCode::KeyA) as i32) as f32,
                (keyboard.pressed(KeyCode::KeyW) as i32 - keyboard.pressed(KeyCode::KeyS) as i32) as f32,
            )
            .normalize_or_zero()
        } else {
            Vec2::ZERO
        };
        pi.move_axis = if gp_move.length_squared() > 0.001 { gp_move } else { kb_move };

        // ── Look ──────────────────────────────────────────────────────────────
        let raw_stick = apply_dz(Vec2::new(axis(GamepadAxis::RightStickX), -axis(GamepadAxis::RightStickY)));
        let curved = raw_stick * raw_stick.length();
        let gp_look = curved * STICK_LOOK_RATE * dt;
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
        pi.jump = (is_p1 && keyboard.just_pressed(KeyCode::Space)) || btn_just(GamepadButton::South);
        pi.jetpack = (is_p1 && keyboard.pressed(KeyCode::Space)) || btn_held(GamepadButton::South);

        // ── Dodge ─────────────────────────────────────────────────────────────
        pi.dodge = (is_p1 && keyboard.just_pressed(KeyCode::KeyQ)) || btn_just(GamepadButton::East);

        // ── Reload ────────────────────────────────────────────────────────────
        pi.reload = (is_p1 && keyboard.just_pressed(KeyCode::KeyR)) || btn_just(GamepadButton::West);

        // ── Parry ─────────────────────────────────────────────────────────────
        pi.parry = (is_p1 && keyboard.just_pressed(KeyCode::KeyF)) || btn_just(GamepadButton::North);

        // ── Interact ──────────────────────────────────────────────────────────
        pi.interact = (is_p1 && keyboard.just_pressed(KeyCode::KeyE)) || btn_just(GamepadButton::DPadDown);

        // ── Melee ─────────────────────────────────────────────────────────────
        pi.melee_light =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyV)) || btn_just(GamepadButton::RightThumb);
        pi.melee_heavy =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyB)) || btn_just(GamepadButton::LeftThumb);

        // ── Crafting ──────────────────────────────────────────────────────────
        pi.crafting =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyC)) || btn_just(GamepadButton::Select);

        // ── Pause ─────────────────────────────────────────────────────────────
        pi.pause =
            (is_p1 && keyboard.just_pressed(KeyCode::Escape)) || btn_just(GamepadButton::Start);

        // ── Weapon cycle ──────────────────────────────────────────────────────
        pi.weapon_next = (is_p1 && keyboard.just_pressed(KeyCode::BracketRight))
            || btn_just(GamepadButton::RightTrigger)
            || btn_just(GamepadButton::DPadRight);
        pi.weapon_prev = (is_p1 && keyboard.just_pressed(KeyCode::BracketLeft))
            || btn_just(GamepadButton::DPadLeft);

        // ── Direct weapon slots (P1 keyboard) ────────────────────────────────
        pi.weapon_slot = if is_p1 {
            if keyboard.just_pressed(KeyCode::Digit1) { Some(0) }
            else if keyboard.just_pressed(KeyCode::Digit2) { Some(1) }
            else if keyboard.just_pressed(KeyCode::Digit3) { Some(2) }
            else if keyboard.just_pressed(KeyCode::Digit4) { Some(3) }
            else if keyboard.just_pressed(KeyCode::Digit5) { Some(4) }
            else if keyboard.just_pressed(KeyCode::Digit6) { Some(5) }
            else { None }
        } else {
            None
        };

        // ── Special weapons (P1 keyboard only; controller binding TBD) ───────
        pi.special_slot = if is_p1 {
            if keyboard.just_pressed(KeyCode::Digit7) { Some(0) }
            else if keyboard.just_pressed(KeyCode::Digit8) { Some(1) }
            else if keyboard.just_pressed(KeyCode::Digit9) { Some(2) }
            else if keyboard.just_pressed(KeyCode::Digit0) { Some(3) }
            else { None }
        } else {
            None
        };

        // ── Vehicle / map ─────────────────────────────────────────────────────
        pi.enter_vehicle =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyJ)) || btn_just(GamepadButton::DPadUp);
        pi.open_map = is_p1 && keyboard.just_pressed(KeyCode::KeyM);

        // ── Star Sabre toggle ─────────────────────────────────────────────────
        pi.sabre_toggle =
            (is_p1 && keyboard.just_pressed(KeyCode::KeyT)) || btn_just(GamepadButton::Mode);
    }
}
