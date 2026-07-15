//! Bounded hitstop (roadmap `EC2` opening move).
//!
//! On a landed hit the *simulation* freezes for a few tens of milliseconds —
//! motor, enemy AI, projectiles and melee timers all hold — while cameras, UI
//! and rendering keep running, so the pause reads as impact, not as a hang.
//!
//! Implementation: a [`HitstopState`] resource drained against `Time<Real>`;
//! gameplay chains opt out of running via the [`hitstop_inactive`] run
//! condition. We deliberately do NOT scale `Time<Virtual>` — that would drag
//! camera tweens and UI animations down with it.

use bevy::prelude::*;

use crate::events::{CombatImpactEvent, EnemyKilledEvent};

/// Remaining freeze time in seconds. Hits extend it (never stack unbounded).
#[derive(Resource, Default)]
pub struct HitstopState {
    pub remaining: f32,
}

/// Hard ceiling so chained hits can't lock the sim up.
const HITSTOP_MAX: f32 = 0.09;
/// Base pause for any landed hit.
const HITSTOP_BASE: f32 = 0.028;
/// Extra pause per point of resolved damage.
const HITSTOP_PER_DAMAGE: f32 = 0.0009;
/// Kills always punctuate with the full window.
const HITSTOP_KILL: f32 = 0.075;

/// Run condition: gameplay simulation may advance.
pub fn hitstop_inactive(state: Res<HitstopState>) -> bool {
    state.remaining <= 0.0
}

/// Extend the freeze from this frame's impacts. Uses `max`, not `+=`: burst
/// weapons produce one clean pause, not a slideshow.
fn accumulate_hitstop(
    mut state: ResMut<HitstopState>,
    mut impacts: MessageReader<CombatImpactEvent>,
    mut kills: MessageReader<EnemyKilledEvent>,
) {
    for impact in impacts.read() {
        let pause = (HITSTOP_BASE + impact.damage * HITSTOP_PER_DAMAGE).min(HITSTOP_MAX);
        let pause = if impact.is_critical {
            HITSTOP_MAX
        } else {
            pause
        };
        state.remaining = state.remaining.max(pause);
    }
    for _ in kills.read() {
        state.remaining = state.remaining.max(HITSTOP_KILL);
    }
}

/// Drain on REAL time so the freeze length is display-rate independent and
/// cannot be extended by the freeze itself.
fn drain_hitstop(real_time: Res<Time<Real>>, mut state: ResMut<HitstopState>) {
    if state.remaining > 0.0 {
        state.remaining = (state.remaining - real_time.delta_secs()).max(0.0);
    }
}

pub struct HitstopPlugin;

impl Plugin for HitstopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HitstopState>()
            .add_systems(Update, (accumulate_hitstop, drain_hitstop).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hitstop_is_bounded_and_uses_max_not_sum() {
        let mut state = HitstopState::default();
        // Simulate the accumulate rule for three rapid hits.
        for damage in [20.0_f32, 35.0, 900.0] {
            let pause = (HITSTOP_BASE + damage * HITSTOP_PER_DAMAGE).min(HITSTOP_MAX);
            state.remaining = state.remaining.max(pause);
        }
        assert!(state.remaining <= HITSTOP_MAX + f32::EPSILON);
        assert!(state.remaining > 0.0);
    }

    #[test]
    fn drain_reaches_zero_and_stops() {
        let mut remaining = HITSTOP_MAX;
        let dt = 1.0 / 120.0;
        let mut steps = 0;
        while remaining > 0.0 && steps < 60 {
            remaining = (remaining - dt).max(0.0);
            steps += 1;
        }
        assert_eq!(remaining, 0.0);
        assert!(steps <= 12, "90 ms must drain in ~11 frames at 120 Hz");
    }
}
