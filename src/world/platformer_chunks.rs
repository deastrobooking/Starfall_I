//! Compatibility facade for the extracted reusable platformer contracts.

pub use starfall_platformer::*;

use crate::components::player::PlayerMovement;

/// Converts production controller tuning into the renderer-neutral envelope
/// used by chunk and route validation.
pub fn jump_envelope_for_movement(movement: &PlayerMovement) -> JumpEnvelope {
    JumpEnvelope::from_profile(MovementProfile {
        walk_speed: movement.walk_speed,
        sprint_speed: movement.sprint_speed,
        jump_force: movement.jump_force,
        gravity: movement.gravity,
        fall_gravity_mult: movement.fall_gravity_mult,
    })
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn reusable_default_tracks_the_production_controller() {
        assert_eq!(
            jump_envelope_for_movement(&PlayerMovement::default()),
            JumpEnvelope::standard()
        );
    }
}
