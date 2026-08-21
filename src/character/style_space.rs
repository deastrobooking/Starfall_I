//! Shared semantic style coordinates for player, NPC, and enemy generators.
//!
//! These values describe intent rather than vertices. Generators remain free
//! to interpret the same axis in topology-appropriate ways while saved recipes
//! stay compact, deterministic, and migration friendly.

use bevy::prelude::Reflect;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Reflect, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterStyleVector {
    pub fantasy: f32,
    pub cute: f32,
    pub heroic: f32,
    pub mechanical: f32,
    pub creature: f32,
}

impl Default for CharacterStyleVector {
    fn default() -> Self {
        Self {
            fantasy: 0.0,
            cute: 0.0,
            heroic: 0.0,
            mechanical: 0.0,
            creature: 0.0,
        }
    }
}

impl CharacterStyleVector {
    pub const STARFALL_HERO: Self = Self {
        fantasy: 0.72,
        cute: 0.20,
        heroic: 0.54,
        mechanical: 0.13,
        creature: 0.0,
    };

    pub fn validate(&mut self) {
        self.fantasy = normalized(self.fantasy);
        self.cute = normalized(self.cute);
        self.heroic = normalized(self.heroic);
        self.mechanical = normalized(self.mechanical);
        self.creature = normalized(self.creature);
    }

    pub fn validated(mut self) -> Self {
        self.validate();
        self
    }

    /// Smooth influence avoids a visible linear kink near either end of an
    /// editor slider while preserving exact zero and one endpoints.
    pub fn smooth(value: f32) -> f32 {
        let value = normalized(value);
        value * value * (3.0 - 2.0 * value)
    }

    pub fn smoothed(self) -> Self {
        let value = self.validated();
        Self {
            fantasy: Self::smooth(value.fantasy),
            cute: Self::smooth(value.cute),
            heroic: Self::smooth(value.heroic),
            mechanical: Self::smooth(value.mechanical),
            creature: Self::smooth(value.creature),
        }
    }
}

fn normalized(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_space_is_bounded_and_non_finite_safe() {
        let value = CharacterStyleVector {
            fantasy: -2.0,
            cute: 2.0,
            heroic: f32::NAN,
            mechanical: f32::INFINITY,
            creature: 0.5,
        }
        .validated();
        assert_eq!(value.fantasy, 0.0);
        assert_eq!(value.cute, 1.0);
        assert_eq!(value.heroic, 0.0);
        assert_eq!(value.mechanical, 0.0);
        assert_eq!(value.creature, 0.5);
    }

    #[test]
    fn smooth_style_weights_preserve_endpoints_and_midpoint() {
        assert_eq!(CharacterStyleVector::smooth(0.0), 0.0);
        assert_eq!(CharacterStyleVector::smooth(0.5), 0.5);
        assert_eq!(CharacterStyleVector::smooth(1.0), 1.0);
    }
}
