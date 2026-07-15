//! Deterministic per-player input buffering (roadmap `EC1`).
//!
//! The motor and combat are moving to a fixed `FixedUpdate` tick. A render frame
//! may run **zero or many** fixed ticks, so reading device edges (`just_pressed`)
//! directly inside the fixed loop would *drop* presses (frame with no tick) or
//! *double-count* them (frame with several ticks).
//!
//! This module fixes that with a latch/consume buffer per player:
//! * [`latch_player_input`] runs once per render frame (before the fixed loop)
//!   and OR-accumulates edge events + sums look-delta into a pending buffer,
//!   while snapshotting held/analog state.
//! * [`consume_fixed_input`] runs once per fixed tick, snapshots the pending
//!   buffer into [`FixedInput`] for that tick, and clears the edge accumulator.
//!
//! EC1a introduced the additive buffer. EC1b now consumes [`FixedInput`] in the
//! default-on fixed traversal, grapple, and motor chain; the legacy `Update`
//! motor remains available as an A/B escape hatch. Combat migration and replay
//! capture remain later engine-core work.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::components::player::{PlayerIndex, PlayerInput, TraversalMode};

/// Continuous (held / analog) input — overwritten with the latest sample each
/// render frame; the fixed tick reads the most recent value.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct HeldInput {
    pub move_axis: Vec2,
    pub fire: bool,
    pub aim: bool,
    pub sprint: bool,
    pub jetpack: bool,
    pub grapple: bool,
}

impl HeldInput {
    fn sample(input: &PlayerInput) -> Self {
        HeldInput {
            move_axis: input.move_axis,
            fire: input.fire,
            aim: input.aim,
            sprint: input.sprint,
            jetpack: input.jetpack,
            grapple: input.grapple,
        }
    }
}

/// Discrete (edge / one-shot) input — OR-accumulated across every render frame
/// since the last fixed tick, then cleared on consume so each press fires once.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct EdgeInput {
    pub jump: bool,
    pub dodge: bool,
    pub parry: bool,
    pub grapple: bool,
    pub fire: bool,
    pub melee_light: bool,
    pub melee_heavy: bool,
    pub interact: bool,
    pub reload: bool,
    pub weapon_next: bool,
    pub weapon_prev: bool,
    pub sabre_toggle: bool,
    pub enter_vehicle: bool,
    /// Summed mouse/stick look delta across frames coalesced into this tick.
    pub look_delta: Vec2,
    pub weapon_slot: Option<usize>,
    pub special_slot: Option<u8>,
    pub traversal: Option<TraversalMode>,
}

impl EdgeInput {
    /// OR one render frame's edges into the accumulator (last-`Some` wins for
    /// the optional selectors; look-delta sums).
    fn accumulate(&mut self, input: &PlayerInput) {
        self.jump |= input.jump;
        self.dodge |= input.dodge;
        self.parry |= input.parry;
        self.grapple |= input.grapple_just;
        self.fire |= input.fire_just;
        self.melee_light |= input.melee_light;
        self.melee_heavy |= input.melee_heavy;
        self.interact |= input.interact;
        self.reload |= input.reload;
        self.weapon_next |= input.weapon_next;
        self.weapon_prev |= input.weapon_prev;
        self.sabre_toggle |= input.sabre_toggle;
        self.enter_vehicle |= input.enter_vehicle;
        self.look_delta += input.look_delta;
        if input.weapon_slot.is_some() {
            self.weapon_slot = input.weapon_slot;
        }
        if input.special_slot.is_some() {
            self.special_slot = input.special_slot;
        }
        if input.traversal_mode_switch.is_some() {
            self.traversal = input.traversal_mode_switch;
        }
    }
}

/// Per-player accumulator: latest held state + pending (un-consumed) edges.
#[derive(Default, Debug)]
pub struct PlayerBuffer {
    held: HeldInput,
    pending: EdgeInput,
}

impl PlayerBuffer {
    /// Record one render frame of input.
    pub fn latch(&mut self, input: &PlayerInput) {
        self.held = HeldInput::sample(input);
        self.pending.accumulate(input);
    }

    /// Produce the command for one fixed tick and clear consumed edges.
    pub fn consume(&mut self) -> FixedInput {
        let out = FixedInput {
            held: self.held,
            edges: self.pending,
        };
        self.pending = EdgeInput::default();
        out
    }
}

/// The resolved input for a single fixed simulation tick.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct FixedInput {
    pub held: HeldInput,
    pub edges: EdgeInput,
}

impl FixedInput {
    /// Project this tick's buffered command onto a full [`PlayerInput`], using
    /// `base` for any field the buffer doesn't own (pause/crafting/etc.).
    ///
    /// This is the EC1b seam: fixed-tick systems call it so edges fire exactly
    /// once per tick regardless of render frame rate, while the legacy
    /// `Update` path keeps reading the live `PlayerInput` untouched.
    pub fn overlay(&self, base: &PlayerInput) -> PlayerInput {
        let mut out = base.clone();
        // Held/analog state: latest render-frame sample wins.
        out.move_axis = self.held.move_axis;
        out.fire = self.held.fire;
        out.aim = self.held.aim;
        out.sprint = self.held.sprint;
        out.jetpack = self.held.jetpack;
        out.grapple = self.held.grapple;
        // Edges: accumulated since the previous tick, consumed exactly once.
        out.jump = self.edges.jump;
        out.dodge = self.edges.dodge;
        out.parry = self.edges.parry;
        out.grapple_just = self.edges.grapple;
        out.fire_just = self.edges.fire;
        out.melee_light = self.edges.melee_light;
        out.melee_heavy = self.edges.melee_heavy;
        out.interact = self.edges.interact;
        out.reload = self.edges.reload;
        out.weapon_next = self.edges.weapon_next;
        out.weapon_prev = self.edges.weapon_prev;
        out.sabre_toggle = self.edges.sabre_toggle;
        out.enter_vehicle = self.edges.enter_vehicle;
        out.look_delta = self.edges.look_delta;
        if self.edges.weapon_slot.is_some() {
            out.weapon_slot = self.edges.weapon_slot;
        }
        if self.edges.special_slot.is_some() {
            out.special_slot = self.edges.special_slot;
        }
        if self.edges.traversal.is_some() {
            out.traversal_mode_switch = self.edges.traversal;
        }
        out
    }
}

/// Per-player input buffers, keyed by `PlayerIndex` (0..=3).
#[derive(Resource, Default)]
pub struct PlayerInputBuffers {
    buffers: HashMap<u8, PlayerBuffer>,
    fixed: HashMap<u8, FixedInput>,
}

impl PlayerInputBuffers {
    /// The command resolved for the current fixed tick, if any. EC1b motor reads this.
    pub fn fixed(&self, player: u8) -> Option<&FixedInput> {
        self.fixed.get(&player)
    }
}

/// Runs once per render frame, before the fixed loop, after device input is written.
fn latch_player_input(
    players: Query<(&PlayerIndex, &PlayerInput)>,
    mut buffers: ResMut<PlayerInputBuffers>,
) {
    for (idx, input) in &players {
        buffers.buffers.entry(idx.0).or_default().latch(input);
    }
}

/// Runs once per fixed tick: snapshot each player's buffer into `FixedInput`.
fn consume_fixed_input(mut buffers: ResMut<PlayerInputBuffers>) {
    let keys: Vec<u8> = buffers.buffers.keys().copied().collect();
    for k in keys {
        let fixed = match buffers.buffers.get_mut(&k) {
            Some(buffer) => buffer.consume(),
            None => continue,
        };
        buffers.fixed.insert(k, fixed);
    }
}

/// EC1a: stand up the per-player input buffer. Additive — populates the buffer
/// without changing existing `Update` motor/combat behavior.
pub struct InputBufferPlugin;

impl Plugin for InputBufferPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInputBuffers>()
            // Latch after device input is written (PreUpdate), before the fixed loop.
            .add_systems(
                RunFixedMainLoop,
                latch_player_input.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
            )
            // Consume first thing each fixed tick.
            .add_systems(FixedUpdate, consume_fixed_input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with(edit: impl FnOnce(&mut PlayerInput)) -> PlayerInput {
        let mut i = PlayerInput::default();
        edit(&mut i);
        i
    }

    #[test]
    fn edges_accumulate_across_frames_then_clear_on_consume() {
        let mut buf = PlayerBuffer::default();
        // Frame 1: jump pressed. Frame 2: melee pressed. (No tick between them.)
        buf.latch(&input_with(|i| i.jump = true));
        buf.latch(&input_with(|i| i.melee_light = true));

        // One fixed tick consumes both edges.
        let tick = buf.consume();
        assert!(
            tick.edges.jump,
            "jump edge from frame 1 must survive to the tick"
        );
        assert!(
            tick.edges.melee_light,
            "melee edge from frame 2 must survive"
        );

        // A second tick with no new input sees no edges (no double-fire).
        let next = buf.consume();
        assert!(!next.edges.jump);
        assert!(!next.edges.melee_light);
    }

    #[test]
    fn held_state_is_latest_sample_and_look_delta_sums() {
        let mut buf = PlayerBuffer::default();
        buf.latch(&input_with(|i| {
            i.move_axis = Vec2::new(1.0, 0.0);
            i.look_delta = Vec2::new(2.0, 1.0);
            i.sprint = true;
        }));
        buf.latch(&input_with(|i| {
            i.move_axis = Vec2::new(0.0, 1.0); // newer sample wins
            i.look_delta = Vec2::new(3.0, -1.0); // deltas sum
            i.sprint = false;
        }));
        let tick = buf.consume();
        assert_eq!(tick.held.move_axis, Vec2::new(0.0, 1.0));
        assert!(!tick.held.sprint);
        assert_eq!(tick.edges.look_delta, Vec2::new(5.0, 0.0));
    }

    #[test]
    fn overlay_projects_buffer_onto_live_input() {
        let mut buf = PlayerBuffer::default();
        // Live input this frame: holding sprint, jump tapped two frames ago.
        buf.latch(&input_with(|i| i.jump = true));
        buf.latch(&input_with(|i| {
            i.sprint = true;
            i.move_axis = Vec2::new(0.0, 1.0);
        }));
        let tick = buf.consume();

        // Base carries a field the buffer doesn't own (pause) and a stale edge
        // (dodge) that must NOT leak through the overlay.
        let mut base = PlayerInput::default();
        base.pause = true;
        base.dodge = true;

        let eff = tick.overlay(&base);
        assert!(eff.jump, "buffered jump edge must fire on this tick");
        assert!(
            !eff.dodge,
            "buffer owns edges; stale live dodge must not leak"
        );
        assert!(
            eff.sprint,
            "held state comes from the latest latched sample"
        );
        assert_eq!(eff.move_axis, Vec2::new(0.0, 1.0));
        assert!(eff.pause, "fields the buffer doesn't own pass through");

        // Next tick with no new frames: the same press must not double-fire.
        let next = buf.consume().overlay(&base);
        assert!(!next.jump);
    }

    #[test]
    fn optional_selectors_keep_last_some() {
        let mut buf = PlayerBuffer::default();
        buf.latch(&input_with(|i| i.weapon_slot = Some(1)));
        buf.latch(&input_with(|i| i.weapon_slot = None)); // must not erase
        let tick = buf.consume();
        assert_eq!(tick.edges.weapon_slot, Some(1));
    }
}
