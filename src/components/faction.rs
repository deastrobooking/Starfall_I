use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Story groups. Drives enemy color/preset choice, dialogue, and recruitment rules.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Faction {
    /// Giacoma, Giovanni, and Gabrio: wizard scientists of the Starfall lab.
    WizardScientist,
    /// Vincenzo, Antonio/Tony, Angelo, and Joseph/Little Joe.
    HeroBrother,
    /// Gabriella, Nova, Aurora, and Fortuna.
    HeroSister,
    /// The Scallarians: space aliens invading Earth through another dimension.
    DimensionalAlien,
    /// Collosar's dragon royal family.
    DragonRoyalty,
    /// Ragar and Blackskull's rival dragon domains.
    DragonExile,
    /// Dr. Bile and the four corrupted human mirrors.
    CorruptedHuman,
    /// Civilians (Earth villagers, Star City inhabitants, etc.).
    Civilian,
    /// Default / unaffiliated.
    #[default]
    Neutral,
}

impl Faction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Faction::WizardScientist => "Wizard Scientist",
            Faction::HeroBrother => "Hero Brother",
            Faction::HeroSister => "Hero Sister",
            Faction::DimensionalAlien => "Scallarian",
            Faction::DragonRoyalty => "Dragon Royalty",
            Faction::DragonExile => "Dragon Exile",
            Faction::CorruptedHuman => "Corrupted Human",
            Faction::Civilian => "Civilian",
            Faction::Neutral => "Neutral",
        }
    }

    /// Voice/portrait tint used by the radio-chatter HUD.
    pub fn dialogue_color(&self) -> Color {
        match self {
            Faction::WizardScientist => Color::srgb(0.55, 0.9, 1.0),
            Faction::HeroBrother => Color::srgb(1.0, 0.85, 0.25),
            Faction::HeroSister => Color::srgb(1.0, 0.45, 0.9),
            Faction::DimensionalAlien => Color::srgb(0.35, 1.0, 0.55),
            Faction::DragonRoyalty => Color::srgb(1.0, 0.25, 0.15),
            Faction::DragonExile => Color::srgb(0.55, 0.7, 1.0),
            Faction::CorruptedHuman => Color::srgb(0.8, 0.15, 0.8),
            Faction::Civilian => Color::srgb(0.85, 0.85, 0.85),
            Faction::Neutral => Color::srgb(0.7, 0.7, 0.8),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensional_alien_faction_uses_scallarian_player_label() {
        assert_eq!(Faction::DimensionalAlien.as_str(), "Scallarian");
    }
}

/// Component tag for any entity with a story name (boss, recruitable, key NPC).
#[derive(Component, Debug, Clone)]
pub struct NamedCharacter {
    pub id: &'static str,
    pub display_name: &'static str,
    pub faction: Faction,
}
