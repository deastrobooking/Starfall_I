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
//! EC1a is **additive**: it populates [`PlayerInputBuffers`] but nothing consumes
//! `FixedInput` yet — the existing motor still reads `PlayerInput` in `Update`.
//! EC1b migrates the motor into `FixedUpdate` to read `FixedInput`, at which point
//! input is frame-rate-independent and replay-ready.

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
    fn optional_selectors_keep_last_some() {
        let mut buf = PlayerBuffer::default();
        buf.latch(&input_with(|i| i.weapon_slot = Some(1)));
        buf.latch(&input_with(|i| i.weapon_slot = None)); // must not erase
        let tick = buf.consume();
        assert_eq!(tick.edges.weapon_slot, Some(1));
    }
}
