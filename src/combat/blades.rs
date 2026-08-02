//! Star Sabre blade catalog: the hilts a player can own, equip, and fight with.
//!
//! Before this existed, shop "weapons" were inert — `ShopOwnership::equipped_weapon`
//! was stored and saved but nothing in gameplay read it, so buying a blade
//! changed nothing. A blade profile is the bridge: the shop sells an id, this
//! catalog turns that id into stats and a colour, and
//! `apply_sabre_blade_system` stamps it onto the player's `BeamSabre` and its
//! rendered blade.
//!
//! Blades are **sidegrades, not a ladder**. Every one is bought with credits at
//! a comparable price, and each trades something for what it gives — a longer
//! chain swings slower, the heaviest hitter has the shortest chain. Relic gems
//! (elemental damage types, the Starheart multiplier) stack on top and remain
//! the exploration reward; the blade is the loadout choice.
//!
//! Adding a blade: append a [`BladeProfile`] here and a matching
//! `ShopCategory::Weapons` entry in `ShopCatalog::default()` using the same id.
//! The test `every_shop_blade_id_resolves` fails if the two ever drift.

use bevy::prelude::*;

/// Visual identity of a blade. Maps to a shared energy material at render time
/// rather than allocating a material per blade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BladeColor {
    /// The issued blade every player starts with: pale blue-white.
    Azure,
    /// Deeper solar blue.
    Solar,
    Crimson,
    Emerald,
    Violet,
    Gold,
    Frost,
    /// Unstable dark blade with a bright rim.
    Void,
}

/// Canonical colour order. Renderers build one material per entry and index by
/// [`BladeColor::index`], so the two must stay in step — `colour_index_round_trips`
/// fails if they drift.
pub const BLADE_COLOR_ORDER: [BladeColor; 8] = [
    BladeColor::Azure,
    BladeColor::Solar,
    BladeColor::Crimson,
    BladeColor::Emerald,
    BladeColor::Violet,
    BladeColor::Gold,
    BladeColor::Frost,
    BladeColor::Void,
];

impl BladeColor {
    /// Position in [`BLADE_COLOR_ORDER`], used to index prebuilt materials.
    pub fn index(self) -> usize {
        BLADE_COLOR_ORDER
            .iter()
            .position(|c| *c == self)
            .unwrap_or(0)
    }

    /// Aura tint. The core stays hot and near-white for readability, so only
    /// the surrounding glow carries the blade's identity.
    pub fn aura_rgb(self) -> (f32, f32, f32) {
        match self {
            BladeColor::Azure => (0.62, 0.86, 1.00),
            BladeColor::Solar => (0.25, 0.55, 1.00),
            BladeColor::Crimson => (1.00, 0.22, 0.26),
            BladeColor::Emerald => (0.24, 1.00, 0.45),
            BladeColor::Violet => (0.72, 0.32, 1.00),
            BladeColor::Gold => (1.00, 0.82, 0.25),
            BladeColor::Frost => (0.55, 0.92, 1.00),
            BladeColor::Void => (0.42, 0.10, 0.62),
        }
    }
}

/// The one distinguishing behaviour a blade adds beyond its numbers. Kept as a
/// small closed set so every trait is actually wired to something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BladeTrait {
    /// No special behaviour — pure stat profile.
    None,
    /// Waves pierce through enemies regardless of sabre level.
    PiercingWaves,
    /// Waves detonate on impact regardless of sabre level.
    ExplosiveWaves,
    /// Landing a slash returns a fraction of the damage as health.
    Lifesteal,
    /// Techniques cost noticeably less cooldown, rewarding constant pressure.
    RapidTechniques,
}

impl BladeTrait {
    pub fn label(self) -> &'static str {
        match self {
            BladeTrait::None => "—",
            BladeTrait::PiercingWaves => "Piercing waves",
            BladeTrait::ExplosiveWaves => "Explosive waves",
            BladeTrait::Lifesteal => "Lifesteal",
            BladeTrait::RapidTechniques => "Rapid techniques",
        }
    }

    /// Fraction of slash damage returned as health (0 unless Lifesteal).
    pub fn lifesteal_fraction(self) -> f32 {
        match self {
            BladeTrait::Lifesteal => 0.06,
            _ => 0.0,
        }
    }

    /// Multiplier applied to technique cooldowns.
    pub fn technique_cooldown_mult(self) -> f32 {
        match self {
            BladeTrait::RapidTechniques => 0.68,
            _ => 1.0,
        }
    }
}

/// One ownable blade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BladeProfile {
    /// Shop item id — the join key between the catalog and `ShopCatalog`.
    pub id: &'static str,
    pub name: &'static str,
    pub color: BladeColor,
    pub trait_: BladeTrait,
    /// Multiplier on slash damage.
    pub slash_damage_mult: f32,
    /// Multiplier on energy-wave damage.
    pub wave_damage_mult: f32,
    /// Slashes added to (or removed from) the level-scaled chain length.
    pub slash_count_delta: i32,
    /// Multiplier on the slash cooldown — below 1.0 swings faster.
    pub cooldown_mult: f32,
    /// One-line shop pitch.
    pub summary: &'static str,
    pub price_credits: u32,
}

/// The blade every player starts with, and the fallback whenever an unknown or
/// unequipped id is resolved. Deliberately all-1.0 so "no blade equipped"
/// behaves exactly like the game did before blades existed.
/// Never sold — it is what every player already carries, and the fallback for
/// an unknown id. Its own identity so it can never collide with a shop item.
pub const STARTER_BLADE: BladeProfile = BladeProfile {
    id: "blade_standard_issue",
    name: "Standard Issue",
    color: BladeColor::Azure,
    trait_: BladeTrait::None,
    slash_damage_mult: 1.0,
    wave_damage_mult: 1.0,
    slash_count_delta: 0,
    cooldown_mult: 1.0,
    summary: "The issued beam blade. Balanced chain, balanced swing.",
    price_credits: 0,
};

/// Every ownable blade, starter first.
pub const BLADE_CATALOG: [BladeProfile; 8] = [
    STARTER_BLADE,
    BladeProfile {
        id: "weapon_solar_sabre",
        name: "Solar Sabre",
        color: BladeColor::Solar,
        trait_: BladeTrait::None,
        // The first blade most players buy: a touch more reach in the wave,
        // paid for with a slightly heavier swing.
        slash_damage_mult: 1.08,
        wave_damage_mult: 1.15,
        slash_count_delta: 0,
        cooldown_mult: 1.06,
        summary: "Close-range beam blade package with matching braced gauntlet poses.",
        price_credits: 900,
    },
    BladeProfile {
        id: "weapon_crimson_edge",
        name: "Crimson Edge",
        color: BladeColor::Crimson,
        trait_: BladeTrait::None,
        // Heaviest single hit in the catalog, paid for with a shorter chain
        // and a slower swing.
        slash_damage_mult: 1.45,
        wave_damage_mult: 0.90,
        slash_count_delta: -1,
        cooldown_mult: 1.18,
        summary: "Heavy war blade. Huge hits, short chain, deliberate swing.",
        price_credits: 1400,
    },
    BladeProfile {
        id: "weapon_emerald_lash",
        name: "Emerald Lash",
        color: BladeColor::Emerald,
        trait_: BladeTrait::Lifesteal,
        slash_damage_mult: 0.92,
        wave_damage_mult: 1.0,
        slash_count_delta: 1,
        cooldown_mult: 0.88,
        summary: "Living blade. Longer, faster chain that feeds health back on every hit.",
        price_credits: 1600,
    },
    BladeProfile {
        id: "weapon_violet_tempest",
        name: "Violet Tempest",
        color: BladeColor::Violet,
        trait_: BladeTrait::PiercingWaves,
        slash_damage_mult: 0.95,
        // The wave blade: weaker in the hand, strongest at range.
        wave_damage_mult: 1.55,
        slash_count_delta: 0,
        cooldown_mult: 1.0,
        summary: "Wave-tuned hilt. Energy waves pierce ranks and hit far harder.",
        price_credits: 1750,
    },
    BladeProfile {
        id: "weapon_gold_regent",
        name: "Gold Regent",
        color: BladeColor::Gold,
        trait_: BladeTrait::RapidTechniques,
        // The duelist: everything comes back fast, but each hit lands light.
        slash_damage_mult: 0.88,
        wave_damage_mult: 0.90,
        slash_count_delta: 1,
        cooldown_mult: 0.82,
        summary: "Duelist's hilt. Light, relentless swings and techniques that barely rest.",
        price_credits: 1900,
    },
    BladeProfile {
        id: "weapon_frost_vigil",
        name: "Frost Vigil",
        color: BladeColor::Frost,
        trait_: BladeTrait::ExplosiveWaves,
        slash_damage_mult: 0.98,
        wave_damage_mult: 1.20,
        slash_count_delta: 0,
        cooldown_mult: 1.0,
        summary: "Siege hilt. Every energy wave detonates on impact.",
        price_credits: 2000,
    },
    BladeProfile {
        id: "weapon_void_requiem",
        name: "Void Requiem",
        color: BladeColor::Void,
        trait_: BladeTrait::Lifesteal,
        // The high-risk blade: best raw numbers, longest recovery between
        // swings, so a missed chain is genuinely punishing.
        slash_damage_mult: 1.30,
        wave_damage_mult: 1.30,
        slash_count_delta: -1,
        cooldown_mult: 1.30,
        summary: "Unstable rift blade. Devastating and hungry, but slow to recover.",
        price_credits: 2400,
    },
];

/// Blades registered at runtime: the published set loaded at startup, plus
/// the Weapon Forge's equip-to-test design, which is *re-registered on every
/// test run*. A locked global rather than an ECS resource because
/// `blade_for_id` is called from pure helpers (HUD text, stat application)
/// that have no resource access — threading a resource through every call
/// site would couple the whole blade API to the ECS for one small list.
static PUBLISHED_BLADES: std::sync::LazyLock<std::sync::RwLock<Vec<BladeProfile>>> =
    std::sync::LazyLock::new(Default::default);

/// Register or update runtime blades, upserting by id: re-publishing content
/// or iterating on a test design replaces the previous version, latest wins.
pub fn register_published_blades(blades: Vec<BladeProfile>) {
    let mut published = PUBLISHED_BLADES
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for blade in blades {
        if let Some(existing) = published.iter_mut().find(|b| b.id == blade.id) {
            *existing = blade;
        } else {
            published.push(blade);
        }
    }
}

/// Resolve a shop item id to its blade — the built-in catalog first, then the
/// registered runtime set. Returns by value (`BladeProfile` is `Copy`) so
/// callers never hold a lock. Unknown or absent ids fall back to the starter
/// blade so a save referencing a removed item still plays correctly.
pub fn blade_for_id(id: Option<&str>) -> BladeProfile {
    let Some(id) = id else {
        return STARTER_BLADE;
    };
    if let Some(blade) = BLADE_CATALOG.iter().find(|blade| blade.id == id) {
        return *blade;
    }
    PUBLISHED_BLADES
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .find(|blade| blade.id == id)
        .copied()
        .unwrap_or(STARTER_BLADE)
}

/// Apply a blade to level-scaled sabre stats.
///
/// Takes the values `BeamSabre::set_level` produced and returns the equipped
/// blade's version of them, so blade and level scaling compose instead of one
/// overwriting the other. The chain is clamped to at least one slash — a
/// negative delta must never leave the sabre unable to swing.
pub fn apply_blade_to_stats(
    blade: &BladeProfile,
    slash_damage: f32,
    wave_damage: f32,
    slash_count: u32,
    cooldown: f32,
) -> (f32, f32, u32, f32) {
    let count = (slash_count as i32 + blade.slash_count_delta).max(1) as u32;
    (
        slash_damage * blade.slash_damage_mult,
        wave_damage * blade.wave_damage_mult,
        count,
        cooldown * blade.cooldown_mult,
    )
}

/// Marker recording which blade is currently stamped onto a player's sabre, so
/// the apply system is idempotent and only re-stamps when the choice changes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquippedBlade(pub &'static str);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_resolvable() {
        let mut ids: Vec<&str> = BLADE_CATALOG.iter().map(|b| b.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate blade id");

        for blade in &BLADE_CATALOG {
            assert_eq!(blade_for_id(Some(blade.id)).id, blade.id);
            assert!(!blade.name.is_empty() && !blade.summary.is_empty());
        }
    }

    #[test]
    fn runtime_registration_upserts_so_test_iterations_take_effect() {
        // Unique ids: the registry is process-global and tests run in parallel.
        let mut first = STARTER_BLADE;
        first.id = "blade_upsert_test";
        first.slash_damage_mult = 1.1;
        register_published_blades(vec![first]);
        assert!((blade_for_id(Some("blade_upsert_test")).slash_damage_mult - 1.1).abs() < 1e-6);

        // Re-registering the same id replaces it — this is what lets the
        // forge's TEST button iterate instead of freezing the first attempt.
        let mut second = STARTER_BLADE;
        second.id = "blade_upsert_test";
        second.slash_damage_mult = 1.4;
        register_published_blades(vec![second]);
        assert!((blade_for_id(Some("blade_upsert_test")).slash_damage_mult - 1.4).abs() < 1e-6);

        // The built-in catalog always wins over a runtime blade of the same id,
        // so published content can never shadow shipped blades.
        let mut impostor = STARTER_BLADE;
        impostor.id = "weapon_crimson_edge";
        impostor.slash_damage_mult = 99.0;
        register_published_blades(vec![impostor]);
        assert!(blade_for_id(Some("weapon_crimson_edge")).slash_damage_mult < 2.0);
    }

    #[test]
    fn unknown_or_missing_ids_fall_back_to_the_starter_blade() {
        // A save referencing a removed item must still be playable.
        assert_eq!(blade_for_id(None).id, STARTER_BLADE.id);
        assert_eq!(blade_for_id(Some("weapon_deleted")).id, STARTER_BLADE.id);
        assert_eq!(blade_for_id(Some("")).id, STARTER_BLADE.id);
    }

    #[test]
    fn the_starter_blade_is_exactly_neutral() {
        // "Nothing equipped" must behave identically to the pre-blade game.
        let (d, w, c, cd) = apply_blade_to_stats(&STARTER_BLADE, 50.0, 80.0, 5, 0.6);
        assert_eq!((d, w, c), (50.0, 80.0, 5));
        assert!((cd - 0.6).abs() < 1e-6);
        assert_eq!(STARTER_BLADE.trait_, BladeTrait::None);
        assert_eq!(STARTER_BLADE.price_credits, 0, "starter is never sold");
    }

    #[test]
    fn blade_stats_compose_with_level_scaling_and_never_break_the_chain() {
        let crimson = blade_for_id(Some("weapon_crimson_edge"));
        let (damage, _, count, cooldown) = apply_blade_to_stats(&crimson, 50.0, 80.0, 5, 0.6);
        assert!(damage > 50.0, "heavy blade hits harder");
        assert_eq!(count, 4, "and gives up a slash for it");
        assert!(cooldown > 0.6, "and swings slower");

        // A negative delta can never leave the sabre unable to swing.
        let (_, _, floored, _) = apply_blade_to_stats(&crimson, 25.0, 40.0, 1, 0.8);
        assert_eq!(floored, 1);

        // Blade and level scaling compose rather than overwrite: the same
        // blade on a stronger sabre still lands proportionally higher.
        let (low, _, _, _) = apply_blade_to_stats(&crimson, 25.0, 40.0, 4, 0.8);
        let (high, _, _, _) = apply_blade_to_stats(&crimson, 85.0, 150.0, 6, 0.4);
        assert!(high > low);
    }

    #[test]
    fn every_blade_is_a_sidegrade_not_a_ladder() {
        for blade in BLADE_CATALOG.iter().filter(|b| b.id != STARTER_BLADE.id) {
            let better_somewhere = blade.slash_damage_mult > 1.0
                || blade.wave_damage_mult > 1.0
                || blade.slash_count_delta > 0
                || blade.cooldown_mult < 1.0
                || blade.trait_ != BladeTrait::None;
            let worse_somewhere = blade.slash_damage_mult < 1.0
                || blade.wave_damage_mult < 1.0
                || blade.slash_count_delta < 0
                || blade.cooldown_mult > 1.0;
            assert!(better_somewhere, "{} has no upside", blade.name);
            assert!(
                worse_somewhere,
                "{} is a strict upgrade — every blade must cost something",
                blade.name
            );
            assert!(blade.price_credits > 0, "{} needs a price", blade.name);
        }
    }

    #[test]
    fn traits_expose_only_the_effects_they_claim() {
        assert!(BladeTrait::Lifesteal.lifesteal_fraction() > 0.0);
        assert_eq!(BladeTrait::None.lifesteal_fraction(), 0.0);
        assert_eq!(BladeTrait::PiercingWaves.lifesteal_fraction(), 0.0);

        assert!(BladeTrait::RapidTechniques.technique_cooldown_mult() < 1.0);
        assert_eq!(BladeTrait::None.technique_cooldown_mult(), 1.0);
        assert_eq!(BladeTrait::Lifesteal.technique_cooldown_mult(), 1.0);
    }

    #[test]
    fn colour_index_round_trips_and_covers_every_colour() {
        // Renderers build one material per BLADE_COLOR_ORDER entry and index
        // with `index()`; a mismatch would silently mis-colour blades.
        for (i, color) in BLADE_COLOR_ORDER.iter().enumerate() {
            assert_eq!(color.index(), i);
        }
        for blade in &BLADE_CATALOG {
            assert!(blade.color.index() < BLADE_COLOR_ORDER.len());
        }
    }

    #[test]
    fn every_colour_is_distinct_so_blades_read_apart_in_co_op() {
        let mut seen: Vec<(u32, u32, u32)> = Vec::new();
        for blade in &BLADE_CATALOG {
            let (r, g, b) = blade.color.aura_rgb();
            let key = ((r * 100.0) as u32, (g * 100.0) as u32, (b * 100.0) as u32);
            assert!(!seen.contains(&key), "{} reuses a colour", blade.name);
            seen.push(key);
        }
    }
}
