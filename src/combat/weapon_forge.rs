//! Modular weapon designer: build a Star Sabre from parts, and have the parts
//! decide what it plays like.
//!
//! The governing idea is that **stats are derived, not typed in**. A designer
//! picks a longer blade, a heavier pommel, a wider guard — and the weapon's
//! damage, reach, swing speed, and chain length fall out of that physical
//! description. Numbers that could be set independently of the model would let
//! a stubby practice blade hit like a greatsword, and the tool would stop being
//! a *design* tool and become a spreadsheet with a preview window.
//!
//! The output is a [`BladeProfile`], the same type the shop sells, so a
//! designed weapon is equippable exactly like a purchased one and inherits all
//! the wiring that already exists (`apply_sabre_blade_system`, the hilt mount,
//! the HUD readout).
//!
//! Balance is enforced rather than trusted: [`WeaponSpec::validate`] reports
//! errors that block a save, and every derived profile is normalized so a
//! design cannot be strictly better than the issued blade — the same
//! sidegrade rule the shop catalog lives under.
//!
//! Consumed by `plugins::weapon_forge_plugin`, the in-game authoring screen.

use serde::{Deserialize, Serialize};

use crate::combat::blades::{BladeColor, BladeProfile, BladeTrait, STARTER_BLADE};

/// How the grip is built. Length and mass distribution are what a real hilt
/// changes, so each style trades control against power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GripStyle {
    /// Balanced one-hand grip.
    #[default]
    Standard,
    /// Short and light — fastest recovery, least leverage.
    Compact,
    /// Long two-hand grip — more leverage, slower to bring around.
    Extended,
    /// Wrapped for control; adds a little of everything.
    Wrapped,
}

impl GripStyle {
    pub const ALL: [GripStyle; 4] = [
        GripStyle::Standard,
        GripStyle::Compact,
        GripStyle::Extended,
        GripStyle::Wrapped,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GripStyle::Standard => "Standard",
            GripStyle::Compact => "Compact",
            GripStyle::Extended => "Extended",
            GripStyle::Wrapped => "Wrapped",
        }
    }

    /// Physical length in metres — drives both the mesh and the leverage.
    pub fn length(self) -> f32 {
        match self {
            GripStyle::Standard => 0.26,
            GripStyle::Compact => 0.19,
            GripStyle::Extended => 0.38,
            GripStyle::Wrapped => 0.28,
        }
    }

    /// Leverage multiplier applied to blade damage.
    fn leverage(self) -> f32 {
        match self {
            GripStyle::Standard => 1.0,
            GripStyle::Compact => 0.94,
            GripStyle::Extended => 1.12,
            GripStyle::Wrapped => 1.03,
        }
    }

    /// Handling multiplier on cooldown — below 1.0 swings faster.
    fn handling(self) -> f32 {
        match self {
            GripStyle::Standard => 1.0,
            GripStyle::Compact => 0.88,
            GripStyle::Extended => 1.14,
            GripStyle::Wrapped => 0.97,
        }
    }
}

/// The guard, which protects the hand and adds mass at the worst place for
/// swing speed — right at the blade root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GuardStyle {
    #[default]
    None,
    /// Simple crossbar.
    Crossguard,
    /// Full basket — heavy, but its bulk turns aside energy waves.
    Basket,
    /// Twin side prongs.
    Tines,
}

impl GuardStyle {
    pub const ALL: [GuardStyle; 4] = [
        GuardStyle::None,
        GuardStyle::Crossguard,
        GuardStyle::Basket,
        GuardStyle::Tines,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GuardStyle::None => "None",
            GuardStyle::Crossguard => "Crossguard",
            GuardStyle::Basket => "Basket",
            GuardStyle::Tines => "Tines",
        }
    }

    /// Mass at the blade root, in arbitrary balance units.
    fn mass(self) -> f32 {
        match self {
            GuardStyle::None => 0.0,
            GuardStyle::Crossguard => 0.10,
            GuardStyle::Basket => 0.24,
            GuardStyle::Tines => 0.14,
        }
    }

    /// Knockback bonus from having something solid behind the hit.
    fn knockback_bonus(self) -> f32 {
        match self {
            GuardStyle::None => 0.0,
            GuardStyle::Crossguard => 0.08,
            GuardStyle::Basket => 0.18,
            GuardStyle::Tines => 0.12,
        }
    }
}

/// The emitter shapes the beam, so it owns the wave behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EmitterStyle {
    #[default]
    Focused,
    /// Splits the beam — waves pierce.
    Prism,
    /// Unstable, high-yield — waves detonate.
    Volatile,
    /// Recycles energy back down the blade — lifesteal.
    Siphon,
}

impl EmitterStyle {
    pub const ALL: [EmitterStyle; 4] = [
        EmitterStyle::Focused,
        EmitterStyle::Prism,
        EmitterStyle::Volatile,
        EmitterStyle::Siphon,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EmitterStyle::Focused => "Focused",
            EmitterStyle::Prism => "Prism",
            EmitterStyle::Volatile => "Volatile",
            EmitterStyle::Siphon => "Siphon",
        }
    }

    /// The behaviour this emitter grants the finished weapon.
    fn granted_trait(self) -> BladeTrait {
        match self {
            EmitterStyle::Focused => BladeTrait::None,
            EmitterStyle::Prism => BladeTrait::PiercingWaves,
            EmitterStyle::Volatile => BladeTrait::ExplosiveWaves,
            EmitterStyle::Siphon => BladeTrait::Lifesteal,
        }
    }

    /// Wave damage multiplier.
    fn wave_output(self) -> f32 {
        match self {
            // A focused emitter puts everything into the beam itself.
            EmitterStyle::Focused => 1.15,
            EmitterStyle::Prism => 1.0,
            EmitterStyle::Volatile => 1.08,
            // Siphoning costs raw output.
            EmitterStyle::Siphon => 0.88,
        }
    }
}

/// The pommel counterweight. Mass here pulls the balance point back toward the
/// hand, which is what actually makes a blade feel quick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PommelStyle {
    #[default]
    Light,
    Balanced,
    Heavy,
    /// A powered core: fast technique recovery at the cost of raw damage.
    Reactor,
}

impl PommelStyle {
    pub const ALL: [PommelStyle; 4] = [
        PommelStyle::Light,
        PommelStyle::Balanced,
        PommelStyle::Heavy,
        PommelStyle::Reactor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PommelStyle::Light => "Light",
            PommelStyle::Balanced => "Balanced",
            PommelStyle::Heavy => "Heavy",
            PommelStyle::Reactor => "Reactor",
        }
    }

    /// Counterweight mass, in the same balance units as the guard.
    fn mass(self) -> f32 {
        match self {
            PommelStyle::Light => 0.04,
            PommelStyle::Balanced => 0.12,
            PommelStyle::Heavy => 0.26,
            PommelStyle::Reactor => 0.16,
        }
    }

    fn grants_rapid_techniques(self) -> bool {
        matches!(self, PommelStyle::Reactor)
    }
}

/// Authoring ranges for the blade's continuous dimensions.
pub const BLADE_LENGTH_RANGE: (f32, f32) = (0.7, 1.9);
pub const BLADE_WIDTH_RANGE: (f32, f32) = (0.6, 1.6);

/// A complete designed weapon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponSpec {
    pub name: String,
    pub grip: GripStyle,
    pub guard: GuardStyle,
    pub emitter: EmitterStyle,
    pub pommel: PommelStyle,
    pub color: BladeColor,
    /// Blade length multiplier — reach and damage against swing speed.
    pub blade_length: f32,
    /// Blade width multiplier — damage against chain length.
    pub blade_width: f32,
}

impl Default for WeaponSpec {
    fn default() -> Self {
        Self {
            name: "New Sabre".to_string(),
            grip: GripStyle::Standard,
            guard: GuardStyle::None,
            emitter: EmitterStyle::Focused,
            pommel: PommelStyle::Balanced,
            color: BladeColor::Azure,
            blade_length: 1.0,
            blade_width: 1.0,
        }
    }
}

/// A validation finding. Errors block a save; warnings are advisory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeIssue {
    pub message: String,
    pub blocking: bool,
}

impl WeaponSpec {
    /// Clamp continuous dimensions into their authoring ranges.
    pub fn sanitized(mut self) -> Self {
        self.blade_length = self
            .blade_length
            .clamp(BLADE_LENGTH_RANGE.0, BLADE_LENGTH_RANGE.1);
        self.blade_width = self
            .blade_width
            .clamp(BLADE_WIDTH_RANGE.0, BLADE_WIDTH_RANGE.1);
        self
    }

    /// Balance point, 0 (blade-heavy) to 1 (hand-heavy). Mass behind the grip
    /// pulls it back; a long wide blade pushes it forward.
    pub fn balance(&self) -> f32 {
        let spec = self.clone().sanitized();
        let forward = spec.blade_length * spec.blade_width * 0.42 + spec.guard.mass();
        let rear = spec.pommel.mass() + spec.grip.length() * 0.55;
        (rear / (rear + forward)).clamp(0.0, 1.0)
    }

    /// Total physical length of the weapon in metres, for the preview and for
    /// the reach the finished sabre gets.
    pub fn overall_length(&self) -> f32 {
        let spec = self.clone().sanitized();
        spec.grip.length() + spec.blade_length * 1.45
    }

    /// Turn the physical design into playable stats.
    ///
    /// Every term traces to something on the model: length and width give
    /// damage, length costs swing speed, balance buys it back, the emitter
    /// decides the wave behaviour, and a bulky weapon loses chain length.
    pub fn derived_profile(&self) -> DerivedWeapon {
        let spec = self.clone().sanitized();
        let bulk = spec.blade_length * spec.blade_width;

        let slash_damage_mult = (0.62 + bulk * 0.40) * spec.grip.leverage();
        let wave_damage_mult = (0.70 + spec.blade_width * 0.32) * spec.emitter.wave_output();

        // Longer, heavier blades swing slower; a rearward balance point
        // recovers much of that.
        let balance_relief = (spec.balance() - 0.5) * 0.34;
        let cooldown_mult = (0.72 + spec.blade_length * 0.34) * spec.grip.handling()
            - balance_relief
            + spec.guard.mass() * 0.30;

        // A big blade is unwieldy enough to cost a link in the chain; a small
        // nimble one gains it.
        let slash_count_delta = if bulk >= 1.9 {
            -1
        } else if bulk <= 0.95 {
            1
        } else {
            0
        };

        let trait_ = if spec.pommel.grants_rapid_techniques()
            && spec.emitter.granted_trait() == BladeTrait::None
        {
            BladeTrait::RapidTechniques
        } else {
            spec.emitter.granted_trait()
        };

        DerivedWeapon {
            slash_damage_mult,
            wave_damage_mult,
            slash_count_delta,
            cooldown_mult: cooldown_mult.max(0.55),
            knockback_bonus: spec.guard.knockback_bonus(),
            trait_,
        }
    }

    /// The equippable profile, normalized so no design is a strict upgrade
    /// over the issued blade. Anything that comes out better in every respect
    /// pays for it in swing speed — the same sidegrade rule the shop obeys.
    pub fn to_blade_profile(&self) -> BladeProfile {
        let derived = self.derived_profile();
        let mut cooldown_mult = derived.cooldown_mult;

        let no_downside = derived.slash_damage_mult >= 1.0
            && derived.wave_damage_mult >= 1.0
            && derived.slash_count_delta >= 0
            && cooldown_mult <= 1.0;
        if no_downside {
            // Push the cost into swing speed rather than silently nerfing the
            // numbers the designer chose.
            cooldown_mult = 1.06;
        }

        BladeProfile {
            id: "weapon_forged",
            name: "Forged Sabre",
            color: self.color,
            trait_: derived.trait_,
            slash_damage_mult: derived.slash_damage_mult,
            wave_damage_mult: derived.wave_damage_mult,
            slash_count_delta: derived.slash_count_delta,
            cooldown_mult,
            summary: "A weapon forged in the Weapon Forge.",
            price_credits: self.estimated_value(),
        }
    }

    /// Rough credit value, so forged weapons can be priced against the shop.
    pub fn estimated_value(&self) -> u32 {
        let derived = self.derived_profile();
        let power = derived.slash_damage_mult + derived.wave_damage_mult
            - (derived.cooldown_mult - 1.0)
            + derived.slash_count_delta as f32 * 0.20
            + if derived.trait_ == BladeTrait::None {
                0.0
            } else {
                0.45
            };
        (power * 900.0).clamp(300.0, 4000.0) as u32
    }

    /// Findings for the designer. Errors block saving.
    pub fn validate(&self) -> Vec<ForgeIssue> {
        let spec = self.clone().sanitized();
        let mut issues = Vec::new();
        let error = |message: &str| ForgeIssue {
            message: message.to_string(),
            blocking: true,
        };
        let warn = |message: &str| ForgeIssue {
            message: message.to_string(),
            blocking: false,
        };

        if spec.name.trim().is_empty() {
            issues.push(error("The weapon needs a name."));
        }
        if spec.name.chars().count() > 40 {
            issues.push(error("Name is too long (40 characters max)."));
        }
        // A two-hand grip on a tiny blade, or a huge blade on a stub grip, are
        // buildable but bad — warn rather than block, so experiments survive.
        if spec.grip == GripStyle::Extended && spec.blade_length < 0.9 {
            issues.push(warn(
                "An extended grip on a short blade wastes its leverage.",
            ));
        }
        if spec.grip == GripStyle::Compact && spec.blade_length > 1.5 {
            issues.push(warn(
                "A compact grip struggles to control a blade this long.",
            ));
        }
        let balance = spec.balance();
        if balance < 0.28 {
            issues.push(warn(
                "Very blade-heavy: it will hit hard but swing slowly.",
            ));
        }
        if balance > 0.78 {
            issues.push(warn("Very hand-heavy: quick, but the hits land light."));
        }
        issues
    }

    /// Whether this design can be saved.
    pub fn is_saveable(&self) -> bool {
        !self.validate().iter().any(|issue| issue.blocking)
    }
}

/// Stats derived from a physical weapon design.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DerivedWeapon {
    pub slash_damage_mult: f32,
    pub wave_damage_mult: f32,
    pub slash_count_delta: i32,
    pub cooldown_mult: f32,
    pub knockback_bonus: f32,
    pub trait_: BladeTrait,
}

/// Starting points for the designer, so a new project is never a blank sheet.
pub fn forge_presets() -> Vec<WeaponSpec> {
    vec![
        WeaponSpec {
            name: "Duelist".to_string(),
            grip: GripStyle::Compact,
            guard: GuardStyle::Tines,
            emitter: EmitterStyle::Focused,
            pommel: PommelStyle::Reactor,
            color: BladeColor::Gold,
            blade_length: 0.85,
            blade_width: 0.8,
        },
        WeaponSpec {
            name: "Warblade".to_string(),
            grip: GripStyle::Extended,
            guard: GuardStyle::Basket,
            emitter: EmitterStyle::Focused,
            pommel: PommelStyle::Heavy,
            color: BladeColor::Crimson,
            blade_length: 1.7,
            blade_width: 1.4,
        },
        WeaponSpec {
            name: "Wavecaster".to_string(),
            grip: GripStyle::Wrapped,
            guard: GuardStyle::Crossguard,
            emitter: EmitterStyle::Prism,
            pommel: PommelStyle::Balanced,
            color: BladeColor::Violet,
            blade_length: 1.1,
            blade_width: 0.9,
        },
        WeaponSpec {
            name: "Leech".to_string(),
            grip: GripStyle::Standard,
            guard: GuardStyle::None,
            emitter: EmitterStyle::Siphon,
            pommel: PommelStyle::Light,
            color: BladeColor::Emerald,
            blade_length: 1.0,
            blade_width: 1.0,
        },
    ]
}

/// Compare a design against the issued blade, for the designer's readout.
pub fn compare_to_starter(spec: &WeaponSpec) -> Vec<(&'static str, f32)> {
    let profile = spec.to_blade_profile();
    vec![
        (
            "Slash damage",
            profile.slash_damage_mult / STARTER_BLADE.slash_damage_mult,
        ),
        (
            "Wave damage",
            profile.wave_damage_mult / STARTER_BLADE.wave_damage_mult,
        ),
        // Inverted so >1 always reads as "better" in the UI.
        (
            "Swing speed",
            STARTER_BLADE.cooldown_mult / profile.cooldown_mult,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_design_is_saveable_and_sane() {
        let spec = WeaponSpec::default();
        assert!(spec.is_saveable());
        let profile = spec.to_blade_profile();
        assert!(profile.slash_damage_mult > 0.0);
        assert!(profile.cooldown_mult > 0.0);
        assert!(profile.price_credits > 0);
    }

    #[test]
    fn a_bigger_blade_hits_harder_and_swings_slower() {
        // The central promise of the tool: the model decides the stats.
        let small = WeaponSpec {
            blade_length: 0.8,
            blade_width: 0.7,
            ..Default::default()
        };
        let large = WeaponSpec {
            blade_length: 1.8,
            blade_width: 1.5,
            ..Default::default()
        };

        let (small_d, large_d) = (small.derived_profile(), large.derived_profile());
        assert!(large_d.slash_damage_mult > small_d.slash_damage_mult);
        assert!(large_d.cooldown_mult > small_d.cooldown_mult);
        // And the unwieldy one gives up a link in the chain the nimble one gains.
        assert!(large_d.slash_count_delta < small_d.slash_count_delta);
        assert!(large.overall_length() > small.overall_length());
    }

    #[test]
    fn a_heavy_pommel_moves_the_balance_back_and_speeds_the_swing() {
        let base = WeaponSpec {
            blade_length: 1.5,
            blade_width: 1.2,
            pommel: PommelStyle::Light,
            ..Default::default()
        };
        let weighted = WeaponSpec {
            pommel: PommelStyle::Heavy,
            ..base.clone()
        };

        assert!(weighted.balance() > base.balance(), "mass moves rearward");
        assert!(
            weighted.derived_profile().cooldown_mult < base.derived_profile().cooldown_mult,
            "a rearward balance point makes the same blade quicker"
        );
    }

    #[test]
    fn grips_trade_leverage_against_handling() {
        let with = |grip| {
            WeaponSpec {
                grip,
                ..Default::default()
            }
            .derived_profile()
        };
        let compact = with(GripStyle::Compact);
        let standard = with(GripStyle::Standard);
        let extended = with(GripStyle::Extended);

        // Long grips hit harder but come around slower; short grips reverse it.
        assert!(extended.slash_damage_mult > standard.slash_damage_mult);
        assert!(extended.cooldown_mult > standard.cooldown_mult);
        assert!(compact.slash_damage_mult < standard.slash_damage_mult);
        assert!(compact.cooldown_mult < standard.cooldown_mult);
    }

    #[test]
    fn the_emitter_decides_the_weapons_behaviour() {
        let with = |emitter| {
            WeaponSpec {
                emitter,
                ..Default::default()
            }
            .derived_profile()
        };
        assert_eq!(with(EmitterStyle::Focused).trait_, BladeTrait::None);
        assert_eq!(
            with(EmitterStyle::Prism).trait_,
            BladeTrait::PiercingWaves
        );
        assert_eq!(
            with(EmitterStyle::Volatile).trait_,
            BladeTrait::ExplosiveWaves
        );
        assert_eq!(with(EmitterStyle::Siphon).trait_, BladeTrait::Lifesteal);

        // A focused emitter puts more into the wave than a siphon does.
        assert!(
            with(EmitterStyle::Focused).wave_damage_mult
                > with(EmitterStyle::Siphon).wave_damage_mult
        );

        // A reactor pommel only grants its trait when the emitter has not
        // claimed the slot, so a weapon never silently loses the trait the
        // designer picked the emitter for.
        let reactor_focused = WeaponSpec {
            pommel: PommelStyle::Reactor,
            emitter: EmitterStyle::Focused,
            ..Default::default()
        };
        assert_eq!(
            reactor_focused.derived_profile().trait_,
            BladeTrait::RapidTechniques
        );
        let reactor_prism = WeaponSpec {
            pommel: PommelStyle::Reactor,
            emitter: EmitterStyle::Prism,
            ..Default::default()
        };
        assert_eq!(
            reactor_prism.derived_profile().trait_,
            BladeTrait::PiercingWaves,
            "the emitter's behaviour must win"
        );
    }

    #[test]
    fn no_design_can_be_a_strict_upgrade_over_the_issued_blade() {
        // The tool must not be a route around the shop's sidegrade rule. Sweep
        // the whole design space and assert every result gives something up.
        for grip in GripStyle::ALL {
            for guard in GuardStyle::ALL {
                for emitter in EmitterStyle::ALL {
                    for pommel in PommelStyle::ALL {
                        for length in [0.7, 1.0, 1.4, 1.9] {
                            for width in [0.6, 1.0, 1.6] {
                                let profile = WeaponSpec {
                                    name: "sweep".into(),
                                    grip,
                                    guard,
                                    emitter,
                                    pommel,
                                    color: BladeColor::Azure,
                                    blade_length: length,
                                    blade_width: width,
                                }
                                .to_blade_profile();

                                let strictly_better = profile.slash_damage_mult
                                    >= STARTER_BLADE.slash_damage_mult
                                    && profile.wave_damage_mult >= STARTER_BLADE.wave_damage_mult
                                    && profile.slash_count_delta
                                        >= STARTER_BLADE.slash_count_delta
                                    && profile.cooldown_mult <= STARTER_BLADE.cooldown_mult;
                                assert!(
                                    !strictly_better,
                                    "{grip:?}/{guard:?}/{emitter:?}/{pommel:?} \
                                     len {length} width {width} is a strict upgrade"
                                );
                                // And nothing degenerate falls out of the maths.
                                assert!(profile.slash_damage_mult > 0.0);
                                assert!(profile.cooldown_mult >= 0.55);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn validation_blocks_only_what_it_should() {
        let unnamed = WeaponSpec {
            name: "   ".into(),
            ..Default::default()
        };
        assert!(!unnamed.is_saveable());
        assert!(unnamed.validate().iter().any(|issue| issue.blocking));

        let long_name = WeaponSpec {
            name: "x".repeat(50),
            ..Default::default()
        };
        assert!(!long_name.is_saveable());

        // Awkward-but-legal builds warn without blocking, so experiments and
        // deliberately odd weapons still save.
        let awkward = WeaponSpec {
            name: "Odd".into(),
            grip: GripStyle::Compact,
            blade_length: 1.8,
            ..Default::default()
        };
        assert!(awkward.is_saveable());
        assert!(awkward.validate().iter().any(|issue| !issue.blocking));
    }

    #[test]
    fn every_preset_is_saveable_and_actually_different() {
        let presets = forge_presets();
        assert!(presets.len() >= 4);
        for preset in &presets {
            assert!(preset.is_saveable(), "{} does not validate", preset.name);
        }
        // Presets should demonstrate range, not four flavours of the same
        // weapon: the fastest and the hardest hitting must differ clearly.
        let profiles: Vec<_> = presets.iter().map(|p| p.to_blade_profile()).collect();
        let fastest = profiles
            .iter()
            .map(|p| p.cooldown_mult)
            .fold(f32::INFINITY, f32::min);
        let slowest = profiles
            .iter()
            .map(|p| p.cooldown_mult)
            .fold(0.0_f32, f32::max);
        assert!(slowest > fastest * 1.25, "presets feel too similar");

        // And the traits on offer are varied.
        let traits: std::collections::BTreeSet<_> =
            profiles.iter().map(|p| format!("{:?}", p.trait_)).collect();
        assert!(traits.len() >= 3, "presets should show off distinct traits");
    }

    #[test]
    fn the_comparison_readout_is_relative_to_the_issued_blade() {
        let starter_like = WeaponSpec::default();
        for (label, ratio) in compare_to_starter(&starter_like) {
            assert!(ratio > 0.0, "{label} ratio must be positive");
        }
        // A monster blade reads as more damage and less speed.
        let big = WeaponSpec {
            blade_length: 1.9,
            blade_width: 1.6,
            ..Default::default()
        };
        let rows = compare_to_starter(&big);
        let damage = rows.iter().find(|(l, _)| *l == "Slash damage").unwrap().1;
        let speed = rows.iter().find(|(l, _)| *l == "Swing speed").unwrap().1;
        assert!(damage > 1.0);
        assert!(speed < 1.0);
    }

    #[test]
    fn a_design_round_trips_through_json() {
        let spec = WeaponSpec {
            name: "Round Trip".into(),
            grip: GripStyle::Extended,
            guard: GuardStyle::Basket,
            emitter: EmitterStyle::Volatile,
            pommel: PommelStyle::Reactor,
            color: BladeColor::Void,
            blade_length: 1.35,
            blade_width: 1.1,
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: WeaponSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
        assert_eq!(back.to_blade_profile().trait_, BladeTrait::ExplosiveWaves);
    }

    #[test]
    fn dimensions_are_clamped_so_the_preview_cannot_explode() {
        let wild = WeaponSpec {
            blade_length: 99.0,
            blade_width: -4.0,
            ..Default::default()
        }
        .sanitized();
        assert!(wild.blade_length <= BLADE_LENGTH_RANGE.1);
        assert!(wild.blade_width >= BLADE_WIDTH_RANGE.0);
        assert!(wild.overall_length() > 0.0);
        assert!(wild.balance() > 0.0 && wild.balance() < 1.0);
    }
}
