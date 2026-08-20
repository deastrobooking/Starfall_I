/// Unified input for up to 4 local players.
///
/// Each frame `update_player_inputs` writes a `PlayerInput` on every live
/// player entity:
///   P1 — keyboard + mouse, plus the first controller connected to the game
///   P2–P4 — additional controllers explicitly assigned by pressing Start in the lobby
///
/// Controller layout (works for Xbox, DualSense, DualShock, 8BitDo, Switch Pro,
/// Stadia, and any HID-compliant gamepad via Bevy's gilrs backend):
///
///  Left stick        — move
///  Right stick       — look (quadratic curve applied)
///  LT  (LTrigger2)  — aim
///  RT  (RTrigger2)  — fire
///  LB  (LTrigger)   — sprint, and the global modifier (LB + X = alt action)
///  RB  (RTrigger)   — primary attack: Star Sabre slash / physical weapon
///  A / Cross / B    — jump                     (South)
///  B / Circle / A   — dodge                    (East)
///  X / Square / Y   — reload; LB+X uses equipped quick item (West)
///  Y / Triangle / X — parry                    (North)
///  LB + Y / Triangle — toggle Star Sabre; RB swings it
///  (LT + Y and Guide/Home remain alternate mappings)
///  LB + DPad Up       — enable Rocket Hoverboard
///  LB + DPad Left     — return to Grapple traversal
///  LB + DPad Down     — enable Hover Jet
///  LB + DPad Right    — enable Flight
///  L3 (left click)  — melee heavy              (LeftThumb)
///  R3 (right click) — melee light              (RightThumb)
///  Select/Back/Share/View — crafting (alone), loadout (LB+Select), or the
///                           utility modifier (+ DPad)
///  Start/Options/Menu     — pause              (Start)
///  Guide/Home             — sabre toggle       (Mode)  [fallback: L3+R3]
///  G / Select+RB      — grapple hook foundation
///
///  DPad (bare) — swaps what RT fires:
///    Left/Right  — cycle primary weapon
///    Up/Down     — cycle special weapon (wraps through "none" = primary)
///
///  Select + DPad — utility actions:
///    Left → toggle first/third person | Up → enter vehicle
///    Down → interact | Right → open map
///  P                  — toggle first/third person for keyboard player one
///
///  LT + DPad Left/Right — cycle armor infusion element
use bevy::input::gamepad::{
    Gamepad, GamepadAxis, GamepadButton, GamepadButtonStateChangedEvent, GamepadConnection,
    GamepadConnectionEvent,
};
use bevy::input::mouse::MouseMotion;
use bevy::input::ButtonState;
use bevy::input::InputSystems;
use bevy::prelude::*;

use crate::components::player::TraversalMode;
use crate::components::player::{Player, PlayerIndex, PlayerInput};
use crate::engine::bindings::{ControlBindings, FaceButton, KeyAction};
use crate::engine_tools::EngineToolMode;
use crate::resources::{GameSettings, UiGameplayCapture};

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct InputPlugin;

/// Native polling and automatic controller ownership are complete for the
/// current frame. UI systems that sample `NativeControllerState` order after
/// this set so they never see a backend one frame late.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ControllerInputSet {
    Ready,
}

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .add_systems(PreUpdate, poll_native_controllers.after(InputSystems))
            .add_systems(PreUpdate, sync_gamepad_assignments.after(InputSystems))
            .add_systems(
                PreUpdate,
                sync_native_controller_assignment
                    .after(poll_native_controllers)
                    .after(sync_gamepad_assignments)
                    .in_set(ControllerInputSet::Ready),
            )
            .add_systems(
                PreUpdate,
                update_player_inputs
                    .after(ControllerInputSet::Ready)
                    .run_if(in_state(EngineToolMode::Playing)),
            )
            .add_systems(
                PreUpdate,
                clear_player_inputs
                    .after(ControllerInputSet::Ready)
                    .run_if(in_state(EngineToolMode::Editing)),
            )
            .add_systems(Update, log_gamepad_connections);
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct GamepadAssignments {
    players: [Option<Entity>; 4],
    native_player: Option<u8>,
    native_overlay_gamepad: Option<Entity>,
    native_overlay_suppressed: bool,
    connected_order: Vec<Entity>,
    bevy_primary_fresh: u8,
    native_discovery_grace: u8,
    native_start_guard_frames: u8,
}

/// Allow the two macOS controller backends a few frames to report the same
/// newly connected pad before deciding that a native reading is native-only.
const NATIVE_DISCOVERY_GRACE_FRAMES: u8 = 4;
/// Keep a native-only Start edge associated with its delayed Bevy counterpart.
/// The two macOS backends can report the same physical press on adjacent frames.
const NATIVE_START_GUARD_FRAMES: u8 = 3;

impl GamepadAssignments {
    fn player_is_unassigned(&self, player_index: usize) -> bool {
        self.players[player_index].is_none() && self.native_player != Some(player_index as u8)
    }

    pub fn gamepad_for_player(&self, player_index: u8) -> Option<Entity> {
        self.players.get(player_index as usize).copied().flatten()
    }

    pub fn player_for_gamepad(&self, gamepad: Entity) -> Option<u8> {
        self.players
            .iter()
            .position(|assigned| *assigned == Some(gamepad))
            .map(|index| index as u8)
    }

    /// Claim Player 1 for a newly connected controller without disturbing any
    /// existing player ownership. Additional controllers still use
    /// [`Self::assign_next`] from the Player Select lobby.
    pub(crate) fn assign_primary(&mut self, gamepad: Entity) -> Option<u8> {
        if let Some(player) = self.player_for_gamepad(gamepad) {
            return (player == 0).then_some(player);
        }
        if !self.player_is_unassigned(0) {
            return None;
        }
        self.players[0] = Some(gamepad);
        Some(0)
    }

    pub fn assign_next(&mut self, gamepad: Entity, joined: &[bool; 4]) -> Option<u8> {
        if let Some(player) = self.player_for_gamepad(gamepad) {
            return Some(player);
        }
        // P1 is reserved for automatic connection ownership. Start-to-join is
        // deliberately limited to couch-player slots P2-P4.
        let slot = (1..4)
            .find(|index| joined[*index] && self.player_is_unassigned(*index))
            .or_else(|| (1..4).find(|index| self.player_is_unassigned(*index)))?;
        self.players[slot] = Some(gamepad);
        Some(slot as u8)
    }

    pub(crate) fn assign_native_primary(&mut self) -> Option<u8> {
        if let Some(player) = self.native_player {
            return (player == 0).then_some(player);
        }
        if self.players[0].is_some() {
            return None;
        }
        self.native_player = Some(0);
        self.native_overlay_gamepad = None;
        Some(0)
    }

    pub(crate) fn assign_native_overlay(&mut self, gamepad: Entity) -> Option<u8> {
        if self.players[0] != Some(gamepad) {
            return None;
        }
        self.native_player = Some(0);
        self.native_overlay_gamepad = Some(gamepad);
        self.native_overlay_suppressed = false;
        Some(0)
    }

    pub fn native_player(&self) -> Option<u8> {
        self.native_player
    }

    pub(crate) fn native_overlay_gamepad(&self) -> Option<Entity> {
        self.native_overlay_gamepad
    }

    pub(crate) fn native_start_guard_active(&self) -> bool {
        self.native_start_guard_frames > 0 && self.players[0].is_none()
    }

    fn release_native(&mut self) {
        self.native_player = None;
        self.native_overlay_gamepad = None;
    }

    fn release(&mut self, gamepad: Entity) {
        let released_native_overlay = self.native_overlay_gamepad == Some(gamepad);
        for assigned in &mut self.players {
            if *assigned == Some(gamepad) {
                *assigned = None;
            }
        }
        if released_native_overlay {
            self.release_native();
            self.native_overlay_suppressed = self.players[1..].iter().any(Option::is_some);
        }
    }

    fn note_connected(&mut self, gamepad: Entity) {
        if !self.connected_order.contains(&gamepad) {
            self.connected_order.push(gamepad);
        }
    }

    fn note_disconnected(&mut self, gamepad: Entity) {
        self.release(gamepad);
        self.connected_order
            .retain(|connected| *connected != gamepad);
    }

    fn has_secondary_bevy_owner(&self) -> bool {
        self.players[1..].iter().any(Option::is_some)
    }

    /// Claim the oldest still-connected, currently unassigned Bevy device
    /// whenever P1 becomes vacant. This preserves connection order without
    /// ever treating ECS query order as player identity.
    fn claim_waiting_primary(&mut self) -> Option<Entity> {
        if !self.player_is_unassigned(0) {
            return None;
        }
        let gamepad = self
            .connected_order
            .iter()
            .copied()
            .find(|gamepad| self.player_for_gamepad(*gamepad).is_none())?;
        self.assign_primary(gamepad)?;
        self.bevy_primary_fresh = NATIVE_DISCOVERY_GRACE_FRAMES;
        Some(gamepad)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Resolve a logical face-button action to the physical Bevy button selected
/// by the player's global layout. Menus and designer tools share this adapter
/// with gameplay so Nintendo/swap presets behave consistently everywhere.
pub(crate) fn mapped_gamepad_face(
    bindings: &ControlBindings,
    logical: FaceButton,
) -> GamepadButton {
    match bindings.remap_face(logical) {
        FaceButton::South => GamepadButton::South,
        FaceButton::East => GamepadButton::East,
        FaceButton::West => GamepadButton::West,
        FaceButton::North => GamepadButton::North,
    }
}

/// Native-macOS counterpart to [`mapped_gamepad_face`].
pub(crate) fn mapped_native_face(bindings: &ControlBindings, logical: FaceButton) -> NativeButton {
    match bindings.remap_face(logical) {
        FaceButton::South => NativeButton::South,
        FaceButton::East => NativeButton::East,
        FaceButton::West => NativeButton::West,
        FaceButton::North => NativeButton::North,
    }
}

#[derive(Resource, Debug, Clone)]
pub struct NativeControllerState {
    pub connected: bool,
    pub name: String,
    pub move_axis: Vec2,
    pub look_axis: Vec2,
    backend_identity: Option<usize>,
    just_connected: bool,
    backend_identity_changed: bool,
    reconnected_after_gap: bool,
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
            backend_identity: None,
            just_connected: false,
            backend_identity_changed: false,
            reconnected_after_gap: false,
            pressed_mask: 0,
            just_pressed_mask: 0,
        }
    }
}

impl NativeControllerState {
    #[cfg(test)]
    pub(crate) fn connected_with_just_pressed(button: NativeButton) -> Self {
        Self {
            connected: true,
            pressed_mask: button.mask(),
            just_pressed_mask: button.mask(),
            ..default()
        }
    }

    #[cfg(test)]
    pub(crate) fn connected_without_edges() -> Self {
        Self {
            connected: true,
            ..default()
        }
    }

    pub fn pressed(&self, button: NativeButton) -> bool {
        self.pressed_mask & button.mask() != 0
    }

    pub fn just_pressed(&self, button: NativeButton) -> bool {
        self.just_pressed_mask & button.mask() != 0
    }

    #[allow(dead_code)]
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
        backend_identity: Option<usize>,
        move_axis: Vec2,
        look_axis: Vec2,
        pressed_mask: u32,
    ) {
        let previous_connected = self.connected;
        let previous_identity = self.backend_identity;
        let previous = self.pressed_mask;
        let backend_identity_changed =
            connected && previous_identity.is_some() && previous_identity != backend_identity;
        let reconnected_after_gap = connected && !previous_connected && previous_identity.is_some();
        let just_connected = connected && (!previous_connected || backend_identity_changed);
        self.connected = connected;
        self.name = name;
        if connected {
            // Retain the last concrete object identity across a transient empty
            // poll so a different controller cannot masquerade as a reconnect.
            self.backend_identity = backend_identity;
        }
        self.move_axis = move_axis;
        self.look_axis = look_axis;
        self.just_connected = just_connected;
        self.backend_identity_changed = backend_identity_changed;
        self.reconnected_after_gap = reconnected_after_gap;
        self.pressed_mask = pressed_mask;
        self.just_pressed_mask = pressed_mask & !previous;

        if just_connected {
            info!("Native macOS controller connected: {}", self.name);
        } else if previous_connected && !connected {
            info!("Native macOS controller disconnected");
        }
    }

    fn clear_poll(&mut self) {
        self.finish_poll(false, String::new(), None, Vec2::ZERO, Vec2::ZERO, 0);
    }
}

fn poll_native_controllers(mut native: ResMut<NativeControllerState>) {
    native_controller_backend::poll(&mut native);
}

/// Native GameController devices do not emit Bevy's connection messages, so a
/// native-only controller may automatically claim P1. When both backends first
/// report a pad together, the native reading overlays Bevy's P1 binding as a
/// mapping fallback; a native owner established earlier blocks later Bevy
/// devices from being merged into it.
pub(crate) fn sync_native_controller_assignment(
    native: Res<NativeControllerState>,
    mut assignments: ResMut<GamepadAssignments>,
) {
    if !native.connected {
        if assignments.native_player().is_some() {
            assignments.release_native();
        }
        assignments.native_discovery_grace = 0;
        assignments.native_start_guard_frames = 0;
        assignments.native_overlay_suppressed = assignments.has_secondary_bevy_owner();
        return;
    }

    assignments.native_start_guard_frames = assignments.native_start_guard_frames.saturating_sub(1);
    if native.just_pressed(NativeButton::Start) && assignments.gamepad_for_player(0).is_none() {
        assignments.native_start_guard_frames = NATIVE_START_GUARD_FRAMES;
    }

    if native.just_connected {
        // A changed native identity replaces only the native side. The Bevy
        // binding, if any, remains stable while the new reading is correlated.
        if assignments.native_player().is_some() {
            assignments.release_native();
        }
        if (native.backend_identity_changed || native.reconnected_after_gap)
            && assignments.has_secondary_bevy_owner()
        {
            // The native aggregate likely advanced from a disconnected P1 pad
            // to a Bevy pad already owned by P2-P4. Do not let that one device
            // drive two players.
            assignments.native_overlay_suppressed = true;
        } else if !native.backend_identity_changed && !native.reconnected_after_gap {
            assignments.native_overlay_suppressed = false;
        }
        assignments.native_discovery_grace = NATIVE_DISCOVERY_GRACE_FRAMES;
    }

    if assignments.native_player().is_some() {
        return;
    }

    if assignments.native_overlay_suppressed {
        if assignments.has_secondary_bevy_owner() {
            assignments.native_discovery_grace = 0;
            return;
        }
        assignments.native_overlay_suppressed = false;
    }

    if let Some(primary) = assignments.gamepad_for_player(0) {
        let paired_discovery =
            assignments.bevy_primary_fresh > 0 && assignments.native_discovery_grace > 0;
        assignments.native_discovery_grace = 0;
        if paired_discovery && assignments.assign_native_overlay(primary).is_some() {
            info!("Native controller mapping overlay assigned to Player 1");
        }
        return;
    }

    if assignments.native_discovery_grace > 0 {
        assignments.native_discovery_grace -= 1;
        return;
    }
    if assignments.assign_native_primary().is_some() {
        info!("Native controller automatically assigned to Player 1");
    }
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

/// Connection events, rather than gamepad query order, own automatic P1
/// assignment. This keeps the first controller stable, supports hot-plug and
/// reconnect, and lets the oldest waiting controller take over if P1's device
/// disconnects. Every additional controller remains available for Start-to-
/// join in Player Select.
fn sync_gamepad_assignments(
    mut events: MessageReader<GamepadConnectionEvent>,
    mut assignments: ResMut<GamepadAssignments>,
) {
    assignments.bevy_primary_fresh = assignments.bevy_primary_fresh.saturating_sub(1);
    for event in events.read() {
        match &event.connection {
            GamepadConnection::Connected { .. } => {
                assignments.note_connected(event.gamepad);
            }
            GamepadConnection::Disconnected => assignments.note_disconnected(event.gamepad),
        }
    }
    if let Some(gamepad) = assignments.claim_waiting_primary() {
        info!("Gamepad {gamepad:?} automatically assigned to Player 1");
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
        // The native macOS backend either owns P1 by itself or overlays the
        // Bevy binding established in the same discovery frame. Assignment
        // ordering prevents a later, distinct Bevy pad from being merged into
        // an already established native P1 owner.
        let use_native = native.connected && assignments.native_player() == Some(idx.0);
        let native_overlay =
            use_native && gp_entity.is_some() && assignments.native_overlay_gamepad() == gp_entity;

        pi.gamepad_active = gp.is_some() || use_native;

        // Face-button remap: every gamepad read goes through the player's
        // layout, so a swap follows chords too (LB+North becomes LB+<whatever
        // North was moved to>) instead of leaving chord partners pointing at
        // the old physical button. Non-face buttons pass straight through.
        let bindings = &settings.bindings;
        let remap = |b: GamepadButton| -> GamepadButton {
            let logical = match b {
                GamepadButton::South => FaceButton::South,
                GamepadButton::East => FaceButton::East,
                GamepadButton::West => FaceButton::West,
                GamepadButton::North => FaceButton::North,
                other => return other,
            };
            mapped_gamepad_face(bindings, logical)
        };
        let remap_native = |b: NativeButton| -> NativeButton {
            let logical = match b {
                NativeButton::South => FaceButton::South,
                NativeButton::East => FaceButton::East,
                NativeButton::West => FaceButton::West,
                NativeButton::North => FaceButton::North,
                other => return other,
            };
            mapped_native_face(bindings, logical)
        };
        // Keyboard key currently bound to an action (player one).
        let key_for = |action: KeyAction| bindings.key(action);

        // Helper closures — all capture `gp`.
        let btn_held =
            |b: GamepadButton| !native_overlay && gp.map(|g| g.pressed(remap(b))).unwrap_or(false);
        let event_just = |b: GamepadButton| {
            let physical = remap(b);
            !native_overlay
                && gp_entity.is_some_and(|entity| {
                    pressed_button_events
                        .iter()
                        .any(|(event_entity, event_button)| {
                            *event_entity == entity && *event_button == physical
                        })
                })
        };
        let btn_just = |b: GamepadButton| {
            (!native_overlay && gp.map(|g| g.just_pressed(remap(b))).unwrap_or(false))
                || event_just(b)
        };
        let axis_val = |a: GamepadAxis| -> f32 { gp.and_then(|g| g.get(a)).unwrap_or(0.0) };
        let native_held =
            |b: NativeButton| -> bool { use_native && native.pressed(remap_native(b)) };
        let native_just =
            |b: NativeButton| -> bool { use_native && native.just_pressed(remap_native(b)) };
        let left_trigger_axis = !native_overlay && axis_pressed(axis_val(GamepadAxis::LeftZ));
        let right_trigger_axis = !native_overlay && axis_pressed(axis_val(GamepadAxis::RightZ));
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
        // Invert applies to the assembled delta, so it covers mouse and stick
        // together rather than only one input device.
        if bindings.invert_look_y {
            pi.look_delta.y = -pi.look_delta.y;
        }

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
        pi.jump = (is_p1 && keyboard.just_pressed(key_for(KeyAction::Jump)))
            || btn_just(GamepadButton::South)
            || native_just(NativeButton::South);
        pi.jetpack = (is_p1 && keyboard.pressed(key_for(KeyAction::Jump)))
            || btn_held(GamepadButton::South)
            || native_held(NativeButton::South);

        // ── Dodge ─────────────────────────────────────────────────────────────
        pi.dodge = (is_p1 && keyboard.just_pressed(key_for(KeyAction::Dodge)))
            || btn_just(GamepadButton::East)
            || native_just(NativeButton::East);

        // ── Reload ────────────────────────────────────────────────────────────
        let west_just = btn_just(GamepadButton::West) || native_just(NativeButton::West);
        pi.use_quick_item = (is_p1 && keyboard.just_pressed(key_for(KeyAction::QuickItem)))
            || (left_shoulder_held && west_just);
        pi.reload = (is_p1 && keyboard.just_pressed(key_for(KeyAction::Reload)))
            || (!left_shoulder_held && west_just);

        // ── Parry ─────────────────────────────────────────────────────────────
        pi.parry = (is_p1 && keyboard.just_pressed(key_for(KeyAction::Parry)))
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
        pi.grapple = (is_p1 && keyboard.pressed(key_for(KeyAction::Grapple)))
            || (select_held && grapple_button_held);
        pi.grapple_just = (is_p1 && keyboard.just_pressed(key_for(KeyAction::Grapple)))
            || (select_held && grapple_button_just);

        // ── Weapon cycle ──────────────────────────────────────────────────────
        let armor_keyboard_modifier =
            keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
        pi.weapon_next =
            is_p1 && !armor_keyboard_modifier && keyboard.just_pressed(KeyCode::BracketRight);
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
        // Controller special selection uses the bare D-pad cycle below.
        // Select + D-pad is reserved for utility actions.
        let dpad_any_just = btn_just(GamepadButton::DPadUp)
            || btn_just(GamepadButton::DPadDown)
            || btn_just(GamepadButton::DPadLeft)
            || btn_just(GamepadButton::DPadRight)
            || native_just(NativeButton::DPadUp)
            || native_just(NativeButton::DPadDown)
            || native_just(NativeButton::DPadLeft)
            || native_just(NativeButton::DPadRight);

        // Controller special selection is now the D-pad up/down cycle below;
        // Select + D-pad carries the utility actions instead, so the direct
        // slot chord would double-fire and has been retired.
        pi.special_slot = if is_p1 {
            if keyboard.just_pressed(KeyCode::Digit7) {
                Some(0)
            } else if keyboard.just_pressed(KeyCode::Digit8) {
                Some(1)
            } else if keyboard.just_pressed(KeyCode::Digit9) {
                Some(2)
            } else if keyboard.just_pressed(KeyCode::Digit0) {
                Some(3)
            } else {
                None
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

        // ── Direct ride/traversal selection ─────────────────────────────────
        // LB is already the movement modifier, so pairing it with D-pad keeps
        // ride selection available to every assigned player without consuming
        // a face button. The chord suppresses the D-pad's normal vehicle,
        // interaction, weapon-cycle, and map actions for this frame.
        let ride_controller_modifier = left_shoulder_held && !select_held && !left_trigger_held;
        pi.traversal_mode_switch = controller_traversal_mode(
            ride_controller_modifier,
            btn_just(GamepadButton::DPadUp) || native_just(NativeButton::DPadUp),
            btn_just(GamepadButton::DPadDown) || native_just(NativeButton::DPadDown),
            btn_just(GamepadButton::DPadLeft) || native_just(NativeButton::DPadLeft),
            btn_just(GamepadButton::DPadRight) || native_just(NativeButton::DPadRight),
        );

        let dpad_free = !select_held && !armor_controller_modifier && !ride_controller_modifier;
        let dpad_up_just = btn_just(GamepadButton::DPadUp) || native_just(NativeButton::DPadUp);
        let dpad_down_just =
            btn_just(GamepadButton::DPadDown) || native_just(NativeButton::DPadDown);
        let dpad_left_just =
            btn_just(GamepadButton::DPadLeft) || native_just(NativeButton::DPadLeft);
        let dpad_right_just =
            btn_just(GamepadButton::DPadRight) || native_just(NativeButton::DPadRight);

        // ── D-pad: weapons on the horizontal, specials on the vertical ───────
        // The bare D-pad is reserved for swapping what RT fires. Utility
        // actions (vehicle, interact, map) move onto the Select chord, and
        // LB + D-pad stays the traversal/flight selector.
        pi.weapon_prev = pi.weapon_prev || (dpad_free && dpad_left_just);
        pi.weapon_next = pi.weapon_next || (dpad_free && dpad_right_just);
        pi.special_next = dpad_free && dpad_up_just;
        pi.special_prev = dpad_free && dpad_down_just;

        // ── Utility actions (Select + D-pad, or keyboard) ─────────────────────
        pi.interact = (is_p1 && keyboard.just_pressed(key_for(KeyAction::Interact)))
            || (select_held && dpad_down_just);

        pi.enter_vehicle = (is_p1 && keyboard.just_pressed(key_for(KeyAction::EnterVehicle)))
            || (select_held && dpad_up_just);

        pi.open_map = (is_p1 && keyboard.just_pressed(key_for(KeyAction::OpenMap)))
            || (select_held && dpad_right_just);

        pi.toggle_perspective = perspective_toggle_requested(
            is_p1 && keyboard.just_pressed(key_for(KeyAction::TogglePerspective)),
            select_held,
            dpad_left_just,
        );

        // ── In-game equipment menus ───────────────────────────────────────────
        // Select alone opens crafting. LB+Select opens the unified loadout;
        // Select+DPad remains the utility chord and suppresses crafting.
        pi.loadout_menu = (is_p1 && keyboard.just_pressed(key_for(KeyAction::LoadoutMenu)))
            || (left_shoulder_held
                && (btn_just(GamepadButton::Select) || native_just(NativeButton::Select)));
        pi.crafting = (is_p1 && keyboard.just_pressed(key_for(KeyAction::Crafting)))
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
        pi.sabre_toggle = (is_p1 && keyboard.just_pressed(key_for(KeyAction::SabreToggle)))
            || (left_shoulder_held
                && (btn_just(GamepadButton::North) || native_just(NativeButton::North)))
            || (left_trigger_held
                && (btn_just(GamepadButton::North) || native_just(NativeButton::North)))
            || btn_just(GamepadButton::Mode)
            || l3r3
            || (native_just(NativeButton::LeftThumb) && native_held(NativeButton::RightThumb))
            || (native_just(NativeButton::RightThumb) && native_held(NativeButton::LeftThumb));
        pi.sabre_attack = (is_p1 && mouse_btn.just_pressed(MouseButton::Left))
            || controller_sabre_attack(select_held, grapple_button_just);

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
        pi.ui_secondary = (is_p1 && keyboard.just_pressed(key_for(KeyAction::Reload)))
            || btn_just(GamepadButton::West)
            || native_just(NativeButton::West);
        pi.ui_page_prev = (is_p1 && keyboard.just_pressed(KeyCode::KeyQ))
            || btn_just(GamepadButton::LeftTrigger)
            || native_just(NativeButton::LeftShoulder);
        pi.ui_page_next = (is_p1 && keyboard.just_pressed(KeyCode::KeyE))
            || btn_just(GamepadButton::RightTrigger)
            || native_just(NativeButton::RightShoulder);
        pi.ui_cancel = (is_p1 && keyboard.just_pressed(KeyCode::Escape))
            || btn_just(GamepadButton::East)
            || native_just(NativeButton::East);

        if capture.owner == Some(idx.0) {
            suppress_gameplay_for_ui(&mut pi);
        }

        trigger_history[history_slot].left = left_trigger_held;
        trigger_history[history_slot].right = right_trigger_held;
    }
}

fn controller_traversal_mode(
    modifier: bool,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
) -> Option<TraversalMode> {
    if !modifier {
        return None;
    }
    if up {
        Some(TraversalMode::Hoverboard)
    } else if left {
        Some(TraversalMode::Grapple)
    } else if down {
        Some(TraversalMode::HoverJet)
    } else if right {
        Some(TraversalMode::Flight)
    } else {
        None
    }
}

fn controller_sabre_attack(select_held: bool, right_shoulder_just: bool) -> bool {
    right_shoulder_just && !select_held
}

fn perspective_toggle_requested(
    keyboard_just: bool,
    select_held: bool,
    dpad_left_just: bool,
) -> bool {
    keyboard_just || (select_held && dpad_left_just)
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
        input.ui_secondary,
        input.ui_page_prev,
        input.ui_page_next,
        input.ui_cancel,
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
        input.ui_secondary,
        input.ui_page_prev,
        input.ui_page_next,
        input.ui_cancel,
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
    use bevy::ecs::message::Messages;

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
    fn face_layout_adapters_match_across_both_controller_backends() {
        let bindings = ControlBindings {
            face_layout: crate::engine::bindings::FaceLayout::Nintendo,
            ..default()
        };

        assert_eq!(
            mapped_gamepad_face(&bindings, FaceButton::South),
            GamepadButton::East
        );
        assert_eq!(
            mapped_native_face(&bindings, FaceButton::South),
            NativeButton::East
        );
        assert_eq!(
            mapped_gamepad_face(&bindings, FaceButton::West),
            GamepadButton::North
        );
        assert_eq!(
            mapped_native_face(&bindings, FaceButton::West),
            NativeButton::North
        );
    }

    #[test]
    fn lb_dpad_assigns_each_traversal_mode_without_unmodified_leakage() {
        assert_eq!(
            controller_traversal_mode(true, true, false, false, false),
            Some(TraversalMode::Hoverboard)
        );
        assert_eq!(
            controller_traversal_mode(true, false, false, true, false),
            Some(TraversalMode::Grapple)
        );
        assert_eq!(
            controller_traversal_mode(true, false, true, false, false),
            Some(TraversalMode::HoverJet)
        );
        assert_eq!(
            controller_traversal_mode(true, false, false, false, true),
            Some(TraversalMode::Flight)
        );
        assert_eq!(
            controller_traversal_mode(false, true, false, false, false),
            None
        );
    }

    #[test]
    fn right_shoulder_attacks_with_sabre_unless_select_claims_grapple() {
        assert!(controller_sabre_attack(false, true));
        assert!(!controller_sabre_attack(true, true));
        assert!(!controller_sabre_attack(false, false));
    }

    #[test]
    fn perspective_toggle_requires_keyboard_or_the_select_left_chord() {
        assert!(perspective_toggle_requested(true, false, false));
        assert!(perspective_toggle_requested(false, true, true));
        assert!(!perspective_toggle_requested(false, false, true));
        assert!(!perspective_toggle_requested(false, true, false));
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
            toggle_perspective: true,
            ui_vertical: -1.0,
            ui_down: true,
            ui_right: true,
            ui_confirm: true,
            ui_secondary: true,
            ui_page_prev: true,
            ui_page_next: true,
            ui_cancel: true,
            ..default()
        };
        suppress_gameplay_for_ui(&mut input);
        assert_eq!(input.move_axis, Vec2::ZERO);
        assert!(!input.fire);
        assert!(!input.jump);
        assert!(!input.enter_vehicle);
        assert!(!input.toggle_perspective);
        assert!(input.crafting);
        assert!(input.loadout_menu);
        assert_eq!(input.ui_vertical, -1.0);
        assert!(input.ui_down);
        assert!(input.ui_right);
        assert!(input.ui_confirm);
        assert!(input.ui_secondary);
        assert!(input.ui_page_prev);
        assert!(input.ui_page_next);
        assert!(input.ui_cancel);
    }

    #[test]
    fn automatic_primary_assignment_leaves_extra_controllers_for_start_to_join() {
        let mut world = World::new();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        let mut assignments = GamepadAssignments::default();
        let joined = [true, false, false, false];

        assert_eq!(assignments.assign_primary(first), Some(0));
        assert_eq!(assignments.assign_next(second, &joined), Some(1));
        assert_eq!(assignments.gamepad_for_player(0), Some(first));
        assert_eq!(assignments.gamepad_for_player(1), Some(second));
        assert_eq!(assignments.assign_primary(first), Some(0));
        assert_eq!(assignments.assign_primary(second), None);
    }

    #[test]
    fn connection_and_reconnect_events_automatically_claim_player_one() {
        let mut app = App::new();
        app.add_message::<GamepadConnectionEvent>()
            .init_resource::<GamepadAssignments>()
            .add_systems(Update, sync_gamepad_assignments);
        let first = app.world_mut().spawn_empty().id();
        let waiting = app.world_mut().spawn_empty().id();
        let reconnected = app.world_mut().spawn_empty().id();

        let connected = |gamepad| {
            GamepadConnectionEvent::new(
                gamepad,
                GamepadConnection::Connected {
                    name: "Test controller".to_string(),
                    vendor_id: None,
                    product_id: None,
                },
            )
        };
        app.world_mut()
            .resource_mut::<Messages<GamepadConnectionEvent>>()
            .write(connected(first));
        app.world_mut()
            .resource_mut::<Messages<GamepadConnectionEvent>>()
            .write(connected(waiting));
        app.update();

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.gamepad_for_player(0), Some(first));
        assert_eq!(assignments.player_for_gamepad(waiting), None);

        app.world_mut()
            .resource_mut::<Messages<GamepadConnectionEvent>>()
            .write(GamepadConnectionEvent::new(
                first,
                GamepadConnection::Disconnected,
            ));
        app.update();
        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.gamepad_for_player(0), Some(waiting));

        app.world_mut()
            .resource_mut::<Messages<GamepadConnectionEvent>>()
            .write(GamepadConnectionEvent::new(
                waiting,
                GamepadConnection::Disconnected,
            ));
        app.update();
        assert_eq!(
            app.world()
                .resource::<GamepadAssignments>()
                .gamepad_for_player(0),
            None
        );

        app.world_mut()
            .resource_mut::<Messages<GamepadConnectionEvent>>()
            .write(connected(reconnected));
        app.update();
        assert_eq!(
            app.world()
                .resource::<GamepadAssignments>()
                .gamepad_for_player(0),
            Some(reconnected)
        );
    }

    #[test]
    fn released_controller_can_reclaim_joined_slot() {
        let mut world = World::new();
        let original = world.spawn_empty().id();
        let reconnected = world.spawn_empty().id();
        let mut assignments = GamepadAssignments::default();
        assert_eq!(
            assignments.assign_next(original, &[true, true, false, false]),
            Some(1)
        );
        assignments.release(original);
        assert_eq!(
            assignments.assign_next(reconnected, &[true, true, false, false]),
            Some(1)
        );
    }

    #[test]
    fn native_primary_does_not_merge_a_later_bevy_device() {
        let mut world = World::new();
        let primary = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        let mut assignments = GamepadAssignments::default();

        assert_eq!(assignments.assign_native_primary(), Some(0));
        assert_eq!(assignments.assign_primary(primary), None);
        assert_eq!(
            assignments.assign_next(primary, &[true, false, false, false]),
            Some(1)
        );
        assert_eq!(
            assignments.assign_next(second, &[true, false, false, false]),
            Some(2)
        );
        assert_eq!(assignments.native_player(), Some(0));
        assert_eq!(assignments.gamepad_for_player(0), None);
        assert_eq!(assignments.gamepad_for_player(1), Some(primary));
        assert_eq!(assignments.gamepad_for_player(2), Some(second));
    }

    #[test]
    fn paired_backend_discovery_enables_the_native_mapping_overlay() {
        let mut app = App::new();
        app.init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .add_systems(Update, sync_native_controller_assignment);
        let primary = app.world_mut().spawn_empty().id();
        {
            let mut assignments = app.world_mut().resource_mut::<GamepadAssignments>();
            assignments.note_connected(primary);
            assert_eq!(assignments.claim_waiting_primary(), Some(primary));
        }
        {
            let mut native = app.world_mut().resource_mut::<NativeControllerState>();
            native.connected = true;
            native.just_connected = true;
        }

        app.update();

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.gamepad_for_player(0), Some(primary));
        assert_eq!(assignments.native_player(), Some(0));
    }

    #[test]
    fn late_distinct_native_device_does_not_merge_into_bevy_primary() {
        let mut app = App::new();
        app.init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .add_systems(Update, sync_native_controller_assignment);
        let primary = app.world_mut().spawn_empty().id();
        assert_eq!(
            app.world_mut()
                .resource_mut::<GamepadAssignments>()
                .assign_primary(primary),
            Some(0)
        );
        {
            let mut native = app.world_mut().resource_mut::<NativeControllerState>();
            native.connected = true;
            native.just_connected = true;
        }

        app.update();

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.gamepad_for_player(0), Some(primary));
        assert_eq!(assignments.native_player(), None);
    }

    #[test]
    fn native_only_device_claims_primary_after_backend_discovery_grace() {
        let mut app = App::new();
        app.init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .add_systems(Update, sync_native_controller_assignment);
        {
            let mut native = app.world_mut().resource_mut::<NativeControllerState>();
            native.connected = true;
            native.just_connected = true;
        }

        app.update();
        app.world_mut()
            .resource_mut::<NativeControllerState>()
            .just_connected = false;
        for _ in 1..NATIVE_DISCOVERY_GRACE_FRAMES {
            app.update();
        }
        assert_eq!(
            app.world().resource::<GamepadAssignments>().native_player(),
            None
        );

        app.update();
        assert_eq!(
            app.world().resource::<GamepadAssignments>().native_player(),
            Some(0)
        );
    }

    #[test]
    fn native_overlay_cannot_switch_from_disconnected_p1_to_bevy_p2() {
        let mut app = App::new();
        app.init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .add_systems(Update, sync_native_controller_assignment);
        let primary = app.world_mut().spawn_empty().id();
        let secondary = app.world_mut().spawn_empty().id();
        {
            let mut assignments = app.world_mut().resource_mut::<GamepadAssignments>();
            assert_eq!(assignments.assign_primary(primary), Some(0));
            assert_eq!(assignments.assign_native_overlay(primary), Some(0));
            assert_eq!(
                assignments.assign_next(secondary, &[true, false, false, false]),
                Some(1)
            );
            assignments.note_disconnected(primary);
        }
        app.world_mut()
            .resource_mut::<NativeControllerState>()
            .connected = true;

        app.update();

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.gamepad_for_player(0), None);
        assert_eq!(assignments.gamepad_for_player(1), Some(secondary));
        assert_eq!(assignments.native_player(), None);
    }

    #[test]
    fn stable_native_overlay_survives_a_bevy_secondary_join() {
        let mut world = World::new();
        let primary = world.spawn_empty().id();
        let secondary = world.spawn_empty().id();
        let mut assignments = GamepadAssignments::default();

        assert_eq!(assignments.assign_primary(primary), Some(0));
        assert_eq!(assignments.assign_native_overlay(primary), Some(0));
        assert_eq!(
            assignments.assign_next(secondary, &[true, false, false, false]),
            Some(1)
        );
        assert_eq!(assignments.native_player(), Some(0));
        assert_eq!(assignments.native_overlay_gamepad(), Some(primary));
    }

    #[test]
    fn native_device_instance_switch_is_suppressed_while_player_two_is_owned() {
        let mut app = App::new();
        app.init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .add_systems(Update, sync_native_controller_assignment);
        let secondary = app.world_mut().spawn_empty().id();
        {
            let mut assignments = app.world_mut().resource_mut::<GamepadAssignments>();
            assert_eq!(assignments.assign_native_primary(), Some(0));
            assert_eq!(
                assignments.assign_next(secondary, &[true, false, false, false]),
                Some(1)
            );
        }
        {
            let mut native = app.world_mut().resource_mut::<NativeControllerState>();
            native.finish_poll(
                true,
                "Same controller model".to_string(),
                Some(1),
                Vec2::ZERO,
                Vec2::ZERO,
                0,
            );
            native.finish_poll(
                true,
                "Same controller model".to_string(),
                Some(2),
                Vec2::ZERO,
                Vec2::ZERO,
                0,
            );
        }

        app.update();

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.gamepad_for_player(1), Some(secondary));
        assert_eq!(assignments.native_player(), None);
    }

    #[test]
    fn native_backend_identity_detects_same_model_controller_switches() {
        let mut native = NativeControllerState::default();
        native.finish_poll(
            true,
            "Same controller model".to_string(),
            Some(1),
            Vec2::ZERO,
            Vec2::ZERO,
            0,
        );
        assert!(native.just_connected);
        assert!(!native.backend_identity_changed);

        native.finish_poll(
            true,
            "Same controller model".to_string(),
            Some(1),
            Vec2::ZERO,
            Vec2::ZERO,
            0,
        );
        assert!(!native.just_connected);

        native.finish_poll(
            true,
            "Same controller model".to_string(),
            Some(2),
            Vec2::ZERO,
            Vec2::ZERO,
            0,
        );
        assert!(native.just_connected);
        assert!(native.backend_identity_changed);

        native.clear_poll();
        native.finish_poll(
            true,
            "Same controller model".to_string(),
            Some(2),
            Vec2::ZERO,
            Vec2::ZERO,
            0,
        );
        assert!(native.just_connected);
        assert!(native.reconnected_after_gap);
    }

    #[test]
    fn native_reconnect_gap_cannot_claim_a_bevy_secondary_as_p1() {
        let mut app = App::new();
        app.init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .add_systems(Update, sync_native_controller_assignment);
        let secondary = app.world_mut().spawn_empty().id();
        {
            let mut assignments = app.world_mut().resource_mut::<GamepadAssignments>();
            assert_eq!(assignments.assign_native_primary(), Some(0));
            assert_eq!(
                assignments.assign_next(secondary, &[true, false, false, false]),
                Some(1)
            );
        }
        {
            let mut native = app.world_mut().resource_mut::<NativeControllerState>();
            native.finish_poll(
                true,
                "Controller".to_string(),
                Some(1),
                Vec2::ZERO,
                Vec2::ZERO,
                0,
            );
            native.just_connected = false;
            native.clear_poll();
        }
        app.update();
        {
            let mut native = app.world_mut().resource_mut::<NativeControllerState>();
            native.finish_poll(
                true,
                "Controller".to_string(),
                Some(1),
                Vec2::ZERO,
                Vec2::ZERO,
                0,
            );
        }
        app.update();

        let assignments = app.world().resource::<GamepadAssignments>();
        assert_eq!(assignments.gamepad_for_player(1), Some(secondary));
        assert_eq!(assignments.native_player(), None);
    }

    #[test]
    fn native_disconnect_releases_the_player_for_reclaim() {
        let mut app = App::new();
        app.init_resource::<NativeControllerState>()
            .init_resource::<GamepadAssignments>()
            .add_systems(Update, sync_native_controller_assignment);
        app.world_mut()
            .resource_mut::<NativeControllerState>()
            .connected = true;
        app.update();
        assert_eq!(
            app.world().resource::<GamepadAssignments>().native_player(),
            Some(0)
        );
        app.world_mut()
            .resource_mut::<NativeControllerState>()
            .connected = false;
        app.update();

        let reconnected = app.world_mut().spawn_empty().id();
        assert_eq!(
            app.world_mut()
                .resource_mut::<GamepadAssignments>()
                .assign_primary(reconnected),
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

            state.finish_poll(
                true,
                name,
                Some(controller as usize),
                left,
                Vec2::new(right.x, -right.y),
                mask,
            );
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
