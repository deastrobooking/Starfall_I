//! Deterministic RNG seam (EC6 on-ramp, Sprint 4).
//!
//! One seeded resource with per-domain streams replaces scattered
//! `rand::thread_rng()` calls, so a session's randomness is reproducible from
//! its seed — the prerequisite for replay, ghosts, and desync debugging.
//! Domains keep streams independent: burning combat rolls never shifts loot.
//!
//! Seed source: `STARFALL_SEED=<u64>` env for reproduction, otherwise wall
//! clock (logged at startup so any session can be re-run).
//!
//! **Exemption:** pure-presentation randomness (e.g. camera-shake offsets)
//! may stay off this seam — it cannot affect simulation state.

use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;

#[derive(Resource)]
pub struct GameRng {
    pub seed: u64,
    combat: StdRng,
    loot: StdRng,
    world: StdRng,
    cosmetic: StdRng,
}

impl GameRng {
    pub fn from_seed(seed: u64) -> Self {
        // Distinct stream seeds via splitmix-style mixing per domain tag.
        let stream = |tag: u64| StdRng::seed_from_u64(mix(seed ^ tag));
        Self {
            seed,
            combat: stream(0xA5A5_0001),
            loot: stream(0xA5A5_0002),
            world: stream(0xA5A5_0003),
            cosmetic: stream(0xA5A5_0004),
        }
    }

    /// Damage rolls, spread, crit-adjacent combat variation.
    pub fn combat(&mut self) -> &mut StdRng {
        &mut self.combat
    }

    /// Drop tables, chest contents, reward variation.
    pub fn loot(&mut self) -> &mut StdRng {
        &mut self.loot
    }

    /// Spawn placement, AI wander, director decisions.
    pub fn world(&mut self) -> &mut StdRng {
        &mut self.world
    }

    /// Particles, sparks, studio randomize — visible but sim-neutral.
    pub fn cosmetic(&mut self) -> &mut StdRng {
        &mut self.cosmetic
    }
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Default for GameRng {
    fn default() -> Self {
        let seed = std::env::var("STARFALL_SEED")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0x5EED_5EED)
            });
        info!("GameRng seed: {seed} (reproduce with STARFALL_SEED={seed})");
        Self::from_seed(seed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn same_seed_reproduces_every_stream() {
        let mut a = GameRng::from_seed(42);
        let mut b = GameRng::from_seed(42);
        for _ in 0..64 {
            assert_eq!(a.combat().gen::<u64>(), b.combat().gen::<u64>());
            assert_eq!(a.loot().gen::<u64>(), b.loot().gen::<u64>());
            assert_eq!(a.world().gen::<u64>(), b.world().gen::<u64>());
            assert_eq!(a.cosmetic().gen::<u64>(), b.cosmetic().gen::<u64>());
        }
    }

    #[test]
    fn domains_are_independent_streams() {
        // Burning combat rolls must not shift loot results.
        let mut plain = GameRng::from_seed(7);
        let expected: Vec<u64> = (0..8).map(|_| plain.loot().gen()).collect();

        let mut burned = GameRng::from_seed(7);
        for _ in 0..1000 {
            let _ = burned.combat().gen::<u64>();
        }
        let got: Vec<u64> = (0..8).map(|_| burned.loot().gen()).collect();
        assert_eq!(expected, got);
    }
}
