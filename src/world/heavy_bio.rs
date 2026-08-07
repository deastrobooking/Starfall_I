//! Rust-native Heavy Water Bio Garden, creature, companion, farm, and rescue domain.
#![allow(dead_code)] // Public port domain; runtime/UI adapters land incrementally.
//!
//! The data in this module is ported from Heavy Water's executable TypeScript
//! systems. It intentionally contains no Bevy systems or UI assumptions: callers
//! can persist [`HeavyBioSave`], translate the returned domain effects into
//! inventory/UI events, and render the static catalogs however they choose.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const HEAVY_BIO_SCHEMA_VERSION: u16 = 1;
pub const BIO_SPECIES_COUNT: usize = 133;
pub const ACTIVE_BIO_PET_CAP: usize = 3;
pub const GARDEN_PLOT_COUNT: usize = 12;
pub const GROWTH_STAGE_MS: u64 = 22_000;
pub const SYNTHETIC_RESCUE_COUNT: usize = 15;
pub const LAB_ANIMAL_RESCUE_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureArchetype {
    Fox,
    Cat,
    Bunny,
    Mouse,
    Pup,
    Beetle,
    Frog,
    Lizard,
    Salamander,
    Serpent,
    Owl,
    Bird,
    Dragon,
    Fish,
    Crab,
    Turtle,
    Bear,
    Monkey,
    Golem,
    Flutter,
    Slime,
    Bot,
    Drone,
    Roller,
}

impl CreatureArchetype {
    pub const fn is_flyer(self) -> bool {
        matches!(
            self,
            Self::Owl
                | Self::Bird
                | Self::Serpent
                | Self::Dragon
                | Self::Flutter
                | Self::Fish
                | Self::Drone
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BioElement {
    Normal,
    Flame,
    Water,
    Grass,
    Electric,
    Ice,
    Psychic,
    Evil,
    Steel,
    Crystal,
    Dragon,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BioPalette {
    pub primary: [f32; 3],
    pub secondary: [f32; 3],
    pub emissive: [f32; 3],
    pub ui_hex: &'static str,
}

impl BioElement {
    /// Heavy Water's exact per-element visual defaults.
    pub const fn palette(self) -> BioPalette {
        match self {
            Self::Normal => BioPalette {
                primary: [0.78, 0.78, 0.82],
                secondary: [0.92, 0.92, 0.95],
                emissive: [0.85, 0.85, 0.90],
                ui_hex: "#bdbdc4",
            },
            Self::Flame => BioPalette {
                primary: [1.0, 0.35, 0.10],
                secondary: [1.0, 0.70, 0.25],
                emissive: [1.0, 0.50, 0.15],
                ui_hex: "#ff7038",
            },
            Self::Water => BioPalette {
                primary: [0.18, 0.50, 1.0],
                secondary: [0.50, 0.80, 1.0],
                emissive: [0.30, 0.70, 1.0],
                ui_hex: "#4f9cff",
            },
            Self::Grass => BioPalette {
                primary: [0.25, 0.85, 0.35],
                secondary: [0.60, 1.0, 0.50],
                emissive: [0.40, 1.0, 0.45],
                ui_hex: "#65d24a",
            },
            Self::Electric => BioPalette {
                primary: [1.0, 0.95, 0.25],
                secondary: [1.0, 1.0, 0.55],
                emissive: [1.0, 1.0, 0.40],
                ui_hex: "#f7d633",
            },
            Self::Ice => BioPalette {
                primary: [0.55, 0.85, 1.0],
                secondary: [0.85, 0.95, 1.0],
                emissive: [0.60, 0.95, 1.0],
                ui_hex: "#9ad9f7",
            },
            Self::Psychic => BioPalette {
                primary: [1.0, 0.40, 0.85],
                secondary: [1.0, 0.70, 0.95],
                emissive: [1.0, 0.50, 0.90],
                ui_hex: "#f56fb8",
            },
            Self::Evil => BioPalette {
                primary: [0.25, 0.18, 0.40],
                secondary: [0.45, 0.35, 0.60],
                emissive: [0.60, 0.30, 0.85],
                ui_hex: "#6b5392",
            },
            Self::Steel => BioPalette {
                primary: [0.55, 0.60, 0.70],
                secondary: [0.85, 0.88, 0.92],
                emissive: [0.70, 0.85, 1.0],
                ui_hex: "#9aa6b3",
            },
            Self::Crystal => BioPalette {
                primary: [0.40, 0.90, 1.0],
                secondary: [0.85, 1.0, 1.0],
                emissive: [0.60, 1.0, 1.0],
                ui_hex: "#7be1ff",
            },
            Self::Dragon => BioPalette {
                primary: [0.55, 0.25, 0.85],
                secondary: [0.95, 0.60, 0.40],
                emissive: [0.80, 0.40, 1.0],
                ui_hex: "#a663ff",
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BioRarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl BioRarity {
    pub const fn base_capture_chance(self) -> f32 {
        match self {
            Self::Common => 0.55,
            Self::Uncommon => 0.42,
            Self::Rare => 0.28,
            Self::Legendary => 0.14,
        }
    }

    pub const fn wild_spawn_weight(self) -> u32 {
        match self {
            Self::Common => 8,
            Self::Uncommon => 4,
            Self::Rare => 2,
            Self::Legendary => 1,
        }
    }

    pub const fn base_stats(self) -> BioStats {
        match self {
            Self::Common => BioStats::new(60.0, 10.0, 1.0),
            Self::Uncommon => BioStats::new(75.0, 13.0, 1.0),
            Self::Rare => BioStats::new(95.0, 17.0, 1.0),
            Self::Legendary => BioStats::new(130.0, 24.0, 1.0),
        }
    }

    const fn bond_multiplier(self) -> f32 {
        match self {
            Self::Common => 1.0,
            Self::Uncommon => 1.25,
            Self::Rare => 1.6,
            Self::Legendary => 2.2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BioStats {
    pub max_health: f32,
    pub attack: f32,
    pub speed: f32,
}

impl BioStats {
    pub const fn new(max_health: f32, attack: f32, speed: f32) -> Self {
        Self {
            max_health,
            attack,
            speed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BioSpecies {
    pub id: &'static str,
    pub name: &'static str,
    pub archetype: CreatureArchetype,
    pub element: BioElement,
    pub rarity: BioRarity,
    pub scale: f32,
    pub role: &'static str,
}

impl BioSpecies {
    pub const fn base_capture_chance(self) -> f32 {
        self.rarity.base_capture_chance()
    }

    pub const fn base_stats(self) -> BioStats {
        self.rarity.base_stats()
    }
}

macro_rules! species {
    ($id:literal, $name:literal, $archetype:ident, $element:ident, $rarity:ident, $scale:literal, $role:literal) => {
        BioSpecies {
            id: $id,
            name: $name,
            archetype: CreatureArchetype::$archetype,
            element: BioElement::$element,
            rarity: BioRarity::$rarity,
            scale: $scale,
            role: $role,
        }
    };
}

/// The complete, stable-id Heavy Water creature catalog.
#[rustfmt::skip]
pub const BIO_SPECIES: [BioSpecies; BIO_SPECIES_COUNT] = [
    species!("robofox", "RoboFox", Fox, Flame, Common, 0.55, "Agile attacker"),
    species!("crystalbeetle", "Crystal Beetle", Beetle, Crystal, Common, 0.40, "Tough scout"),
    species!("hoverserpent", "Hover Serpent", Serpent, Grass, Uncommon, 0.60, "Aerial striker"),
    species!("neonowl", "Neon Owl", Owl, Psychic, Uncommon, 0.50, "Recon drone"),
    species!("voltfrog", "Volt Frog", Frog, Electric, Common, 0.45, "Shock support"),
    species!("emberkit", "Emberkit", Fox, Flame, Common, 0.50, "Spits ember bursts"),
    species!("emberpup", "Emberpup", Pup, Flame, Common, 0.55, "Loyal flame hound"),
    species!("magmacat", "Magmacat", Cat, Flame, Uncommon, 0.50, "Searing pounce"),
    species!("scorchbunny", "Scorchbunny", Bunny, Flame, Uncommon, 0.45, "Hops on jet flares"),
    species!("infernodrake", "Infernodrake", Dragon, Flame, Rare, 0.75, "Fire-spit dragonling"),
    species!("ashbear", "Ashbear", Bear, Flame, Rare, 0.85, "Cinder-cloaked tank"),
    species!("lavalizard", "Lavalizard", Lizard, Flame, Common, 0.48, "Heat-armored runner"),
    species!("blazemoth", "Blazemoth", Flutter, Flame, Uncommon, 0.55, "Wings of fire"),
    species!("pyroowl", "Pyrowl", Owl, Flame, Uncommon, 0.55, "Twilight burner"),
    species!("magmacrab", "Magmacrab", Crab, Flame, Uncommon, 0.60, "Shell of slag"),
    species!("infernoslime", "Infernoslime", Slime, Flame, Common, 0.45, "Living lava blob"),
    species!("phoenixhatch", "Phoenixhatch", Bird, Flame, Legendary, 0.70, "Reborn at zero HP"),
    species!("aquakit", "Aquakit", Fox, Water, Common, 0.50, "Dives like a sluice"),
    species!("aquapup", "Aquapup", Pup, Water, Common, 0.55, "Splash dive support"),
    species!("hydraserpent", "Hydraserpent", Serpent, Water, Rare, 0.70, "Tide-rider"),
    species!("tidalcat", "Tidalcat", Cat, Water, Uncommon, 0.50, "Surf-step striker"),
    species!("brinebear", "Brinebear", Bear, Water, Rare, 0.85, "Salt-sea tank"),
    species!("mistmoth", "Mistmoth", Flutter, Water, Uncommon, 0.55, "Cloaks allies in fog"),
    species!("marlin", "Marlin", Fish, Water, Uncommon, 0.65, "Hover-fin striker"),
    species!("pufferbot", "Pufferbot", Fish, Water, Common, 0.45, "Inflates as shield"),
    species!("clamcrab", "Clamcrab", Crab, Water, Common, 0.55, "Pearl-blast scout"),
    species!("riverturtle", "Riverturtle", Turtle, Water, Common, 0.55, "Plated swimmer"),
    species!("splashfrog", "Splashfrog", Frog, Water, Common, 0.45, "Geyser-leap support"),
    species!("oceanowl", "Oceanowl", Owl, Water, Uncommon, 0.55, "Glides on sea spray"),
    species!("krakling", "Krakling", Fish, Water, Legendary, 0.80, "Tentacled deep-runner"),
    species!("mossfox", "Mossfox", Fox, Grass, Common, 0.50, "Camo-cloaked striker"),
    species!("vinepup", "Vinepup", Pup, Grass, Common, 0.55, "Vine-whip support"),
    species!("leafbeetle", "Leafbeetle", Beetle, Grass, Common, 0.40, "Pollen-burst scout"),
    species!("verdantcat", "Verdantcat", Cat, Grass, Uncommon, 0.50, "Blossom-bound striker"),
    species!("sproutfrog", "Sproutfrog", Frog, Grass, Common, 0.45, "Spore-cloud emitter"),
    species!("blossomflutter", "Blossomflutter", Flutter, Grass, Uncommon, 0.55, "Petal-storm wings"),
    species!("herbcrab", "Herbcrab", Crab, Grass, Common, 0.55, "Healing pollen aura"),
    species!("vinebear", "Vinebear", Bear, Grass, Rare, 0.85, "Moss-armored tank"),
    species!("leafdrake", "Leafdrake", Dragon, Grass, Rare, 0.75, "Sap-wing dragonling"),
    species!("seedmouse", "Seedmouse", Mouse, Grass, Common, 0.40, "Tiny pollen scout"),
    species!("forestowl", "Forestowl", Owl, Grass, Uncommon, 0.55, "Canopy hunter"),
    species!("verdantgolem", "Verdantgolem", Golem, Grass, Rare, 0.95, "Walking topiary"),
    species!("zapfox", "Zapfox", Fox, Electric, Common, 0.50, "Static-charge dasher"),
    species!("sparkpup", "Sparkpup", Pup, Electric, Common, 0.55, "Tiny shock support"),
    species!("sparkbeetle", "Sparkbeetle", Beetle, Electric, Common, 0.40, "Builds charge cells"),
    species!("voltcat", "Voltcat", Cat, Electric, Uncommon, 0.50, "Coil-paw striker"),
    species!("plasmamouse", "Plasmamouse", Mouse, Electric, Common, 0.40, "Cheek-cell zapper"),
    species!("currentbunny", "Currentbunny", Bunny, Electric, Uncommon, 0.45, "Antenna-ear conductor"),
    species!("lightningserpent", "Lightningserpent", Serpent, Electric, Rare, 0.70, "Bolt-bodied flyer"),
    species!("voltflutter", "Voltflutter", Flutter, Electric, Uncommon, 0.55, "Static-storm wings"),
    species!("surgegolem", "Surgegolem", Golem, Electric, Rare, 0.95, "Walking transformer"),
    species!("jolturtle", "Jolturtle", Turtle, Electric, Common, 0.55, "Plate-coil shell"),
    species!("ampbear", "Ampbear", Bear, Electric, Rare, 0.85, "Storm-fur tank"),
    species!("dynacrab", "Dynacrab", Crab, Electric, Uncommon, 0.55, "Claw-arc shocker"),
    species!("frostfox", "Frostfox", Fox, Ice, Common, 0.50, "Frostbite dasher"),
    species!("frostpup", "Frostpup", Pup, Ice, Common, 0.55, "Snow-step support"),
    species!("frostbeetle", "Frostbeetle", Beetle, Ice, Common, 0.40, "Crystallized scout"),
    species!("snowcat", "Snowcat", Cat, Ice, Uncommon, 0.50, "Glacier prowler"),
    species!("icypup", "Icypup", Pup, Ice, Common, 0.55, "Sub-zero support"),
    species!("frosttoad", "Frosttoad", Frog, Ice, Common, 0.45, "Hailspit support"),
    species!("blizzardbear", "Blizzardbear", Bear, Ice, Rare, 0.90, "Storm-coat tank"),
    species!("hailmoth", "Hailmoth", Flutter, Ice, Uncommon, 0.55, "Sleet-cloud wings"),
    species!("glaciercrab", "Glaciercrab", Crab, Ice, Uncommon, 0.55, "Frost-shell crusher"),
    species!("frostowl", "Frostowl", Owl, Ice, Uncommon, 0.55, "Polar glide hunter"),
    species!("crystaldrake", "Crystaldrake", Dragon, Ice, Rare, 0.75, "Glacial dragonling"),
    species!("rimeserpent", "Rimeserpent", Serpent, Ice, Uncommon, 0.65, "Frost-arc flyer"),
    species!("mindcat", "Mindcat", Cat, Psychic, Uncommon, 0.50, "Telekinesis striker"),
    species!("telekfox", "Telekfox", Fox, Psychic, Uncommon, 0.50, "Mind-spike dasher"),
    species!("dreampup", "Dreampup", Pup, Psychic, Common, 0.55, "Sleep-aura support"),
    species!("psyowl", "Psyowl", Owl, Psychic, Uncommon, 0.55, "Mind's-eye hunter"),
    species!("esperflutter", "Esperflutter", Flutter, Psychic, Uncommon, 0.55, "Resonance wings"),
    species!("phasebear", "Phasebear", Bear, Psychic, Rare, 0.85, "Reality-bend tank"),
    species!("mindlinkslime", "Mindlinkslime", Slime, Psychic, Common, 0.45, "Hive-mind blob"),
    species!("oraclepup", "Oraclepup", Pup, Psychic, Rare, 0.55, "Foresight support"),
    species!("astralmonkey", "Astralmonkey", Monkey, Psychic, Uncommon, 0.60, "Star-leap acrobat"),
    species!("dreamserpent", "Dreamserpent", Serpent, Psychic, Rare, 0.70, "Lullaby-coil flyer"),
    species!("prismslime", "Prismslime", Slime, Psychic, Uncommon, 0.45, "Refracting blob"),
    species!("voidcat", "Voidcat", Cat, Evil, Uncommon, 0.50, "Shadow-step striker"),
    species!("shadefox", "Shadefox", Fox, Evil, Uncommon, 0.50, "Twilight ambusher"),
    species!("dreadbeetle", "Dreadbeetle", Beetle, Evil, Common, 0.40, "Shadow-shell scout"),
    species!("gloomserpent", "Gloomserpent", Serpent, Evil, Rare, 0.70, "Eclipse-coil flyer"),
    species!("nightowl", "Nightowl", Owl, Evil, Uncommon, 0.55, "Silent dive hunter"),
    species!("eclipsedrake", "Eclipsedrake", Dragon, Evil, Legendary, 0.80, "Black-sun dragonling"),
    species!("voidpup", "Voidpup", Pup, Evil, Common, 0.55, "Wisp-trail support"),
    species!("shadowbear", "Shadowbear", Bear, Evil, Rare, 0.90, "Umbral cloak tank"),
    species!("hexmoth", "Hexmoth", Flutter, Evil, Uncommon, 0.55, "Curse-dust wings"),
    species!("voidturtle", "Voidturtle", Turtle, Evil, Uncommon, 0.55, "Null-shell defender"),
    species!("shadeslime", "Shadeslime", Slime, Evil, Common, 0.45, "Inky absorber"),
    species!("alloyfox", "Alloyfox", Fox, Steel, Common, 0.50, "Plated dasher"),
    species!("ironcat", "Ironcat", Cat, Steel, Uncommon, 0.50, "Plated pounce striker"),
    species!("steelpup", "Steelpup", Pup, Steel, Common, 0.55, "Riveted support"),
    species!("mechabeetle", "Mechabeetle", Beetle, Steel, Common, 0.40, "Drill-horn scout"),
    species!("gearowl", "Gearowl", Owl, Steel, Uncommon, 0.55, "Cogwheel hunter"),
    species!("alloyserpent", "Alloyserpent", Serpent, Steel, Uncommon, 0.65, "Chain-link flyer"),
    species!("ironbear", "Ironbear", Bear, Steel, Rare, 0.95, "Riveted tank"),
    species!("ironcrab", "Ironcrab", Crab, Steel, Uncommon, 0.60, "Hydraulic-claw crusher"),
    species!("scrapgolem", "Scrapgolem", Golem, Steel, Uncommon, 0.95, "Junk-pile bruiser"),
    species!("voltgolem", "Voltgolem", Golem, Steel, Rare, 1.00, "Powerline juggernaut"),
    species!("rivetcrab", "Rivetcrab", Crab, Steel, Common, 0.55, "Bolt-shell scout"),
    species!("titanmonkey", "Titanmonkey", Monkey, Steel, Rare, 0.65, "Industrial swinger"),
    species!("prismcat", "Prismcat", Cat, Crystal, Uncommon, 0.50, "Refraction striker"),
    species!("crystalfox", "Crystalfox", Fox, Crystal, Uncommon, 0.50, "Shard-step dasher"),
    species!("gemslime", "Gemslime", Slime, Crystal, Common, 0.45, "Hardened blob"),
    species!("crystalpup", "Crystalpup", Pup, Crystal, Common, 0.55, "Faceted support"),
    species!("crystalflutter", "Crystalflutter", Flutter, Crystal, Uncommon, 0.55, "Geode wings"),
    species!("crystalbear", "Crystalbear", Bear, Crystal, Rare, 0.90, "Geode-armored tank"),
    species!("prismbeetle", "Prismbeetle", Beetle, Crystal, Common, 0.40, "Refractive scout"),
    species!("crystaldragon", "Crystaldragon", Dragon, Crystal, Legendary, 0.80, "Faceted dragon king"),
    species!("voidskydragon", "Voidskydragon", Dragon, Dragon, Legendary, 0.85, "Void-cloud sovereign"),
    species!("twincoildragon", "Twincoildragon", Dragon, Dragon, Rare, 0.78, "Twin-element drake"),
    species!("hatchdragon", "Hatchdragon", Dragon, Dragon, Uncommon, 0.55, "Tiny dragonling"),
    species!("starfeatherdrake", "Starfeatherdrake", Dragon, Dragon, Rare, 0.75, "Feathered cosmic drake"),
    species!("scoutcat", "Scoutcat", Cat, Normal, Common, 0.50, "Camp recon striker"),
    species!("courierpup", "Courierpup", Pup, Normal, Common, 0.55, "Cargo support"),
    species!("dashbunny", "Dashbunny", Bunny, Normal, Common, 0.45, "Hop-jet scout"),
    species!("runnermouse", "Runnermouse", Mouse, Normal, Common, 0.40, "Wire-tap scout"),
    species!("cargobear", "Cargobear", Bear, Normal, Uncommon, 0.85, "Pack-frame tank"),
    species!("signalflutter", "Signalflutter", Flutter, Normal, Common, 0.55, "Beacon wings"),
    species!("surveyowl", "Surveyowl", Owl, Normal, Common, 0.55, "Cartography hunter"),
    species!("lightcrab", "Lightcrab", Crab, Normal, Common, 0.55, "Flare-claw scout"),
    species!("swiftserpent", "Swiftserpent", Serpent, Normal, Common, 0.60, "Light-frame flyer"),
    species!("tinypup", "Tinypup", Pup, Normal, Common, 0.50, "Pocket-scale support"),
    species!("chipsalamander", "Chipsalamander", Salamander, Normal, Common, 0.50, "Heat-sink lizard"),
    species!("dustcat", "Dustcat", Cat, Normal, Common, 0.50, "Vent-clearing scout"),
    species!("loaderape", "Loaderape", Monkey, Normal, Uncommon, 0.65, "Lift-frame hauler"),
    species!("beepbot", "Beepbot", Bot, Normal, Common, 0.46, "Tiny helper chassis"),
    species!("hugbot", "Hugbot", Bot, Normal, Uncommon, 0.52, "Comfort-care robot"),
    species!("sproutbot", "Sproutbot", Bot, Grass, Common, 0.48, "Seedling field helper"),
    species!("seedroller", "Seedroller", Roller, Grass, Common, 0.50, "Soil-tilling roller"),
    species!("sunnyroller", "Sunnyroller", Roller, Electric, Common, 0.50, "Solar charge rover"),
    species!("orbdrone", "Orbdrone", Drone, Electric, Uncommon, 0.48, "Floating scout helper"),
    species!("medidrone", "Medidrone", Drone, Water, Uncommon, 0.50, "Clinic mist drone"),
    species!("pearlbot", "Pearlbot", Bot, Crystal, Rare, 0.52, "Polished sanctuary aide"),
];

pub fn species_by_id(id: &str) -> Option<&'static BioSpecies> {
    BIO_SPECIES.iter().find(|species| species.id == id)
}

pub fn species_by_element(element: BioElement) -> impl Iterator<Item = &'static BioSpecies> {
    BIO_SPECIES
        .iter()
        .filter(move |species| species.element == element)
}

/// Deterministic counterpart to Heavy Water's rarity-weighted wild picker.
pub fn weighted_species(seed: u64) -> &'static BioSpecies {
    let total: u32 = BIO_SPECIES
        .iter()
        .map(|species| species.rarity.wild_spawn_weight())
        .sum();
    let mut pick = (mix64(seed) % u64::from(total)) as u32;
    for species in &BIO_SPECIES {
        let weight = species.rarity.wild_spawn_weight();
        if pick < weight {
            return species;
        }
        pick -= weight;
    }
    &BIO_SPECIES[0]
}

// -------------------------------------------------------------------------
// Player-keyed save state, capture, Dex, care, and passive Bio-pet bonuses.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeavyBioSave {
    pub schema_version: u16,
    /// Stable account/local-slot key to that player's independent Bio state.
    pub players: BTreeMap<String, PlayerBioRecord>,
}

impl Default for HeavyBioSave {
    fn default() -> Self {
        Self {
            schema_version: HEAVY_BIO_SCHEMA_VERSION,
            players: BTreeMap::new(),
        }
    }
}

impl HeavyBioSave {
    pub fn player(&self, player_key: &str) -> Option<&PlayerBioRecord> {
        self.players.get(player_key)
    }

    pub fn player_mut(&mut self, player_key: impl Into<String>) -> &mut PlayerBioRecord {
        self.players.entry(player_key.into()).or_default()
    }

    /// Prune unknown catalog ids and clamp legacy/malformed scalar fields.
    pub fn normalize(&mut self) {
        self.schema_version = HEAVY_BIO_SCHEMA_VERSION;
        self.players.retain(|key, _| !key.trim().is_empty());
        for profile in self.players.values_mut() {
            profile.normalize();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerBioRecord {
    /// Ever-caught species ids. Removing/deploying a creature never shrinks it.
    pub dex_species_ids: BTreeSet<String>,
    /// Stable instance id to the creatures currently housed in this garden.
    pub creatures: BTreeMap<String, CapturedCreatureRecord>,
    /// Ordered follower selection, capped independently from garden capacity.
    pub active_creature_ids: Vec<String>,
    pub garden_level: u8,
    pub garden: GardenRecord,
    pub companions: CompanionRosterRecord,
    pub rescues: RescueProgressRecord,
    pub next_creature_serial: u64,
}

impl Default for PlayerBioRecord {
    fn default() -> Self {
        Self {
            dex_species_ids: BTreeSet::new(),
            creatures: BTreeMap::new(),
            active_creature_ids: Vec::new(),
            garden_level: 1,
            garden: GardenRecord::default(),
            companions: CompanionRosterRecord::default(),
            rescues: RescueProgressRecord::default(),
            next_creature_serial: 1,
        }
    }
}

impl PlayerBioRecord {
    pub fn garden_capture_cap(&self) -> usize {
        garden_capture_cap(self.garden_level)
    }

    pub fn garden_capture_bonus(&self) -> f32 {
        garden_capture_bonus(self.garden_level)
    }

    pub fn dex_completion(&self) -> (usize, usize) {
        (self.dex_species_ids.len(), BIO_SPECIES_COUNT)
    }

    pub fn attempt_capture(
        &mut self,
        player_key: &str,
        request: CaptureRequest<'_>,
    ) -> Result<CaptureResolution, CaptureError> {
        if self.creatures.len() >= self.garden_capture_cap() {
            return Err(CaptureError::GardenRosterFull {
                capacity: self.garden_capture_cap(),
            });
        }
        let species = species_by_id(request.species_id)
            .ok_or_else(|| CaptureError::UnknownSpecies(request.species_id.to_owned()))?;
        let odds = capture_odds(
            player_key,
            species,
            self.garden_level,
            request.current_health,
            request.max_health,
            request.encounter_seed,
            request.attempt_index,
        )?;
        if odds.roll >= odds.total_chance {
            return Ok(CaptureResolution::BrokeFree { odds });
        }

        let serial = self.next_creature_serial.max(1);
        let mut next = serial;
        let instance_id = loop {
            let candidate = format!("bio-{next:016x}");
            next = next.saturating_add(1);
            if !self.creatures.contains_key(&candidate) {
                break candidate;
            }
        };
        self.next_creature_serial = next;
        let creature = CapturedCreatureRecord::from_species(&instance_id, species);
        self.creatures.insert(instance_id.clone(), creature);
        self.dex_species_ids.insert(species.id.to_owned());

        Ok(CaptureResolution::Captured {
            odds,
            creature_id: instance_id,
        })
    }

    pub fn remove_captured(&mut self, creature_id: &str) -> Option<CapturedCreatureRecord> {
        let removed = self.creatures.remove(creature_id);
        if removed.is_some() {
            self.active_creature_ids.retain(|id| id != creature_id);
        }
        removed
    }

    /// Replace the active follower order atomically. Unknown ids do not leave
    /// a partially-mutated selection behind.
    pub fn select_active<I, S>(&mut self, creature_ids: I) -> Result<(), ActivePetError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut selected = Vec::new();
        for id in creature_ids {
            let id = id.as_ref();
            if !self.creatures.contains_key(id) {
                return Err(ActivePetError::UnknownCreature(id.to_owned()));
            }
            if selected.iter().any(|existing| existing == id) {
                continue;
            }
            if selected.len() >= ACTIVE_BIO_PET_CAP {
                return Err(ActivePetError::TooManyActive {
                    capacity: ACTIVE_BIO_PET_CAP,
                });
            }
            selected.push(id.to_owned());
        }
        self.active_creature_ids = selected;
        Ok(())
    }

    pub fn care_for(
        &mut self,
        creature_id: &str,
        item: CareItem,
    ) -> Result<CareOutcome, CareError> {
        let creature = self
            .creatures
            .get_mut(creature_id)
            .ok_or_else(|| CareError::UnknownCreature(creature_id.to_owned()))?;
        let previous_bond = creature.bond_level;
        creature.care = creature.care.saturating_add(item.care_power()).min(3);
        if creature.care >= 3 && creature.bond_level < 10 {
            creature.care = 0;
            creature.bond_level += 1;
            creature.level = creature.level.saturating_add(1).min(50);
            creature.max_health += 8.0;
            creature.current_health = (creature.current_health + 8.0).min(creature.max_health);
            creature.attack += 2.0;
            creature.speed += 0.03;
        } else {
            creature.current_health = creature.max_health;
        }
        Ok(CareOutcome {
            item_consumed: item,
            bonded: creature.bond_level > previous_bond,
            bond_level: creature.bond_level,
            care: creature.care,
            level: creature.level,
        })
    }

    /// Heavy Water's all-roster bond passives, including its category caps.
    pub fn bond_bonuses(&self) -> PetBondBonuses {
        let mut damage = 0.0;
        let mut fire_rate = 0.0;
        let mut reduction = 0.0;
        for creature in self.creatures.values() {
            let Some(species) = species_by_id(&creature.species_id) else {
                continue;
            };
            let power = species.rarity.bond_multiplier()
                * (0.0025
                    + f32::from(creature.bond_level) * 0.0015
                    + f32::from(creature.level.max(1)) * 0.0004);
            match species.element {
                BioElement::Flame | BioElement::Dragon | BioElement::Evil => damage += power,
                BioElement::Electric | BioElement::Psychic | BioElement::Crystal => {
                    fire_rate += power * 0.8;
                }
                _ => reduction += power * 0.55,
            }
        }
        PetBondBonuses {
            damage_multiplier: 1.0 + damage.min(0.25),
            fire_rate_multiplier: 1.0 + fire_rate.min(0.18),
            damage_reduction: reduction.min(0.15),
        }
    }

    /// Active-pet elemental augments. Only the selected three can contribute.
    pub fn active_pet_bonuses(&self) -> ActivePetBonuses {
        let mut damage = 0.0;
        let mut fire_rate = 0.0;
        let mut shield_regen = 0.0;
        for id in self.active_creature_ids.iter().take(ACTIVE_BIO_PET_CAP) {
            let Some(creature) = self.creatures.get(id) else {
                continue;
            };
            let Some(species) = species_by_id(&creature.species_id) else {
                continue;
            };
            let power = 0.003 * f32::from(creature.level.clamp(1, 50));
            match species.element {
                BioElement::Flame | BioElement::Dragon | BioElement::Evil => damage += power,
                BioElement::Electric | BioElement::Psychic | BioElement::Crystal => {
                    fire_rate += power * 0.8;
                }
                _ => shield_regen += power * 0.6,
            }
        }
        ActivePetBonuses {
            damage_multiplier: 1.0 + damage.min(0.20),
            fire_rate_multiplier: 1.0 + fire_rate.min(0.15),
            speed_multiplier: 1.0,
            shield_regen_per_second: shield_regen.min(4.0),
            health_regen_per_second: 0.0,
            critical_chance: 0.0,
        }
    }

    fn normalize(&mut self) {
        self.garden_level = self.garden_level.clamp(1, 3);
        self.next_creature_serial = self.next_creature_serial.max(1);
        self.dex_species_ids
            .retain(|species_id| species_by_id(species_id).is_some());
        self.creatures.retain(|_, creature| {
            let Some(species) = species_by_id(&creature.species_id) else {
                return false;
            };
            creature.normalize(species);
            self.dex_species_ids.insert(species.id.to_owned());
            true
        });
        let mut seen = BTreeSet::new();
        self.active_creature_ids.retain(|id| {
            self.creatures.contains_key(id)
                && seen.insert(id.clone())
                && seen.len() <= ACTIVE_BIO_PET_CAP
        });
        self.garden.normalize();
        self.companions.normalize();
        self.rescues.normalize();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapturedCreatureRecord {
    #[serde(default, alias = "speciesId")]
    pub species_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_one_u8")]
    pub level: u8,
    #[serde(default = "default_nan_f32", alias = "currentHp")]
    pub current_health: f32,
    #[serde(default, alias = "hp")]
    pub max_health: f32,
    #[serde(default, alias = "attackPower")]
    pub attack: f32,
    #[serde(default = "default_one_f32")]
    pub speed: f32,
    #[serde(default, alias = "bondLevel")]
    pub bond_level: u8,
    #[serde(default)]
    pub care: u8,
}

impl CapturedCreatureRecord {
    fn from_species(_instance_id: &str, species: &BioSpecies) -> Self {
        let stats = species.base_stats();
        Self {
            species_id: species.id.to_owned(),
            name: species.name.to_owned(),
            level: 1,
            current_health: stats.max_health,
            max_health: stats.max_health,
            attack: stats.attack,
            speed: stats.speed,
            bond_level: 0,
            care: 0,
        }
    }

    fn normalize(&mut self, species: &BioSpecies) {
        let fallback = species.base_stats();
        if self.name.trim().is_empty() {
            self.name = species.name.to_owned();
        }
        self.level = self.level.clamp(1, 50);
        if !self.max_health.is_finite() || self.max_health <= 0.0 {
            self.max_health = fallback.max_health;
        }
        if !self.current_health.is_finite() {
            self.current_health = self.max_health;
        }
        self.current_health = self.current_health.clamp(0.0, self.max_health);
        if !self.attack.is_finite() || self.attack <= 0.0 {
            self.attack = fallback.attack;
        }
        if !self.speed.is_finite() || self.speed <= 0.0 {
            self.speed = fallback.speed;
        }
        self.bond_level = self.bond_level.min(10);
        self.care = self.care.min(3);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CaptureRequest<'a> {
    pub species_id: &'a str,
    pub current_health: f32,
    pub max_health: f32,
    /// Stable world encounter seed supplied by the spawn system.
    pub encounter_seed: u64,
    /// Increment when another orb is spent on the same encounter.
    pub attempt_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureOdds {
    pub base_chance: f32,
    pub garden_bonus: f32,
    pub weakened_health_bonus: f32,
    pub total_chance: f32,
    pub roll: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureResolution {
    BrokeFree {
        odds: CaptureOdds,
    },
    Captured {
        odds: CaptureOdds,
        creature_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    UnknownSpecies(String),
    InvalidHealth,
    GardenRosterFull { capacity: usize },
}

/// Sequel capture policy: retain Heavy Water's rarity/garden odds while
/// awarding up to +35 percentage points for reducing a target to zero health.
pub fn capture_odds(
    player_key: &str,
    species: &BioSpecies,
    garden_level: u8,
    current_health: f32,
    max_health: f32,
    encounter_seed: u64,
    attempt_index: u32,
) -> Result<CaptureOdds, CaptureError> {
    if !current_health.is_finite()
        || !max_health.is_finite()
        || current_health < 0.0
        || max_health <= 0.0
    {
        return Err(CaptureError::InvalidHealth);
    }
    let health_ratio = (current_health / max_health).clamp(0.0, 1.0);
    let base_chance = species.base_capture_chance();
    let garden_bonus = garden_capture_bonus(garden_level);
    let weakened_health_bonus = (1.0 - health_ratio) * 0.35;
    let total_chance = (base_chance + garden_bonus + weakened_health_bonus).min(0.95);
    let roll = stable_roll(&[
        b"capture",
        player_key.as_bytes(),
        species.id.as_bytes(),
        &encounter_seed.to_le_bytes(),
        &attempt_index.to_le_bytes(),
    ]);
    Ok(CaptureOdds {
        base_chance,
        garden_bonus,
        weakened_health_bonus,
        total_chance,
        roll,
    })
}

pub const fn garden_capture_cap(level: u8) -> usize {
    match level {
        0 | 1 => 15,
        2 => 30,
        _ => 50,
    }
}

pub const fn garden_capture_bonus(level: u8) -> f32 {
    match level {
        0 | 1 => 0.0,
        2 => 0.15,
        _ => 0.30,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivePetError {
    UnknownCreature(String),
    TooManyActive { capacity: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CareItem {
    BioCrop,
    AnimatonFeed,
}

impl CareItem {
    const fn care_power(self) -> u8 {
        match self {
            Self::BioCrop => 1,
            Self::AnimatonFeed => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CareOutcome {
    pub item_consumed: CareItem,
    pub bonded: bool,
    pub bond_level: u8,
    pub care: u8,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CareError {
    UnknownCreature(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PetBondBonuses {
    pub damage_multiplier: f32,
    pub fire_rate_multiplier: f32,
    pub damage_reduction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivePetBonuses {
    pub damage_multiplier: f32,
    pub fire_rate_multiplier: f32,
    pub speed_multiplier: f32,
    pub shield_regen_per_second: f32,
    pub health_regen_per_second: f32,
    pub critical_chance: f32,
}

const fn default_one_u8() -> u8 {
    1
}

const fn default_one_f32() -> f32 {
    1.0
}

fn default_nan_f32() -> f32 {
    f32::NAN
}

// -------------------------------------------------------------------------
// Twelve-plot Bio Garden farming.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrowthStage {
    #[default]
    Empty,
    Seeded,
    Sprout,
    Grown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GardenPlotRecord {
    pub stage: GrowthStage,
    /// Absolute game/save clock time. Unlike performance.now(), this survives
    /// a save/load and lets growth catch up deterministically.
    pub stage_started_at_ms: u64,
    pub crop_cycle: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GardenRecord {
    #[serde(default = "default_garden_plots")]
    pub plots: [GardenPlotRecord; GARDEN_PLOT_COUNT],
}

impl Default for GardenRecord {
    fn default() -> Self {
        Self {
            plots: default_garden_plots(),
        }
    }
}

impl GardenRecord {
    pub fn plant(&mut self, plot_index: usize, now_ms: u64) -> Result<PlantOutcome, GardenError> {
        let plot = self
            .plots
            .get_mut(plot_index)
            .ok_or(GardenError::InvalidPlot(plot_index))?;
        if plot.stage != GrowthStage::Empty {
            return Err(GardenError::PlotOccupied(plot_index));
        }
        plot.crop_cycle = plot.crop_cycle.saturating_add(1);
        plot.stage = GrowthStage::Seeded;
        plot.stage_started_at_ms = now_ms;
        Ok(PlantOutcome { bio_seeds_spent: 1 })
    }

    /// Advance every plot, including both stages after a long/offline gap.
    pub fn advance_growth(&mut self, now_ms: u64) -> usize {
        self.plots
            .iter_mut()
            .map(|plot| usize::from(advance_plot(plot, now_ms)))
            .sum()
    }

    pub fn harvest(
        &mut self,
        player_key: &str,
        plot_index: usize,
        now_ms: u64,
    ) -> Result<HarvestYield, GardenError> {
        let plot = self
            .plots
            .get_mut(plot_index)
            .ok_or(GardenError::InvalidPlot(plot_index))?;
        advance_plot(plot, now_ms);
        if plot.stage != GrowthStage::Grown {
            return Err(match plot.stage {
                GrowthStage::Empty => GardenError::PlotEmpty(plot_index),
                _ => GardenError::StillGrowing(plot_index),
            });
        }

        let plot_bytes = (plot_index as u64).to_le_bytes();
        let cycle_bytes = plot.crop_cycle.to_le_bytes();
        let started_bytes = plot.stage_started_at_ms.to_le_bytes();
        let feed_roll = stable_roll(&[
            b"garden-feed",
            player_key.as_bytes(),
            &plot_bytes,
            &cycle_bytes,
            &started_bytes,
        ]);
        let seed_roll = stable_roll(&[
            b"garden-seed",
            player_key.as_bytes(),
            &plot_bytes,
            &cycle_bytes,
            &started_bytes,
        ]);
        let yield_record = HarvestYield {
            bio_crops: 2,
            animaton_feed: u8::from(feed_roll < 0.45),
            bio_seeds: u8::from(seed_roll < 0.35),
        };
        plot.stage = GrowthStage::Empty;
        plot.stage_started_at_ms = now_ms;
        Ok(yield_record)
    }

    fn normalize(&mut self) {
        for plot in &mut self.plots {
            if plot.stage == GrowthStage::Empty {
                plot.stage_started_at_ms = 0;
            }
        }
    }
}

fn advance_plot(plot: &mut GardenPlotRecord, now_ms: u64) -> bool {
    let original = plot.stage;
    loop {
        let next = match plot.stage {
            GrowthStage::Seeded => GrowthStage::Sprout,
            GrowthStage::Sprout => GrowthStage::Grown,
            GrowthStage::Empty | GrowthStage::Grown => break,
        };
        if now_ms.saturating_sub(plot.stage_started_at_ms) < GROWTH_STAGE_MS {
            break;
        }
        plot.stage = next;
        plot.stage_started_at_ms = plot.stage_started_at_ms.saturating_add(GROWTH_STAGE_MS);
    }
    plot.stage != original
}

const fn default_garden_plots() -> [GardenPlotRecord; GARDEN_PLOT_COUNT] {
    [GardenPlotRecord {
        stage: GrowthStage::Empty,
        stage_started_at_ms: 0,
        crop_cycle: 0,
    }; GARDEN_PLOT_COUNT]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlantOutcome {
    /// The inventory integration consumes this only after plant succeeds.
    pub bio_seeds_spent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarvestYield {
    pub bio_crops: u8,
    pub animaton_feed: u8,
    pub bio_seeds: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GardenError {
    InvalidPlot(usize),
    PlotOccupied(usize),
    PlotEmpty(usize),
    StillGrowing(usize),
}

// -------------------------------------------------------------------------
// Persistent helper-companion roster, upgrades, death, and revival.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelperCompanionKind {
    Ally,
    Pet,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompanionPresetDef {
    pub id: &'static str,
    pub kind: HelperCompanionKind,
    pub base_max_health: f32,
    pub base_damage: f32,
    pub base_heal: f32,
    pub base_move_speed: f32,
    pub base_attack_cooldown: f32,
    /// Heavy Water explicitly restores Spark Pup and RoboDragon after player
    /// death. The legendary mini-General is also durable here so its one-shot
    /// rescue reward cannot be permanently lost.
    pub auto_revive_on_player_respawn: bool,
}

const fn ally_preset(id: &'static str, durable: bool) -> CompanionPresetDef {
    CompanionPresetDef {
        id,
        kind: HelperCompanionKind::Ally,
        base_max_health: 150.0,
        base_damage: 22.0,
        base_heal: 5.0,
        base_move_speed: 0.12,
        base_attack_cooldown: 0.85,
        auto_revive_on_player_respawn: durable,
    }
}

const fn pet_preset(id: &'static str, durable: bool) -> CompanionPresetDef {
    CompanionPresetDef {
        id,
        kind: HelperCompanionKind::Pet,
        base_max_health: 50.0,
        base_damage: 0.0,
        base_heal: 2.0,
        base_move_speed: 0.15,
        base_attack_cooldown: 99.0,
        auto_revive_on_player_respawn: durable,
    }
}

pub const COMPANION_PRESETS: [CompanionPresetDef; 9] = [
    ally_preset("MegaUnitX", false),
    ally_preset("GuardianUnit", false),
    CompanionPresetDef {
        id: "MedicDrone",
        kind: HelperCompanionKind::Ally,
        base_max_health: 150.0,
        base_damage: 10.0,
        base_heal: 8.0,
        base_move_speed: 0.12,
        base_attack_cooldown: 1.4,
        auto_revive_on_player_respawn: false,
    },
    ally_preset("ScoutCompanion", false),
    ally_preset("MiniGeneralVoidcrown", true),
    ally_preset("RoboDragon", true),
    pet_preset("SparkPup", true),
    pet_preset("NeonCat", false),
    pet_preset("HoverOrb", false),
];

pub fn companion_preset(id: &str) -> Option<&'static CompanionPresetDef> {
    COMPANION_PRESETS.iter().find(|preset| preset.id == id)
}

pub const fn lab_companion_cap(level: u8) -> u8 {
    match level {
        0 | 1 => 3,
        2 => 5,
        _ => 8,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionLifeState {
    #[default]
    Active,
    Downed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionRecord {
    #[serde(default, alias = "presetName")]
    pub preset_id: String,
    #[serde(default, alias = "type")]
    pub kind: Option<HelperCompanionKind>,
    #[serde(default = "default_one_u8")]
    pub level: u8,
    #[serde(default, alias = "weaponLevel")]
    pub weapon_level: u8,
    #[serde(default)]
    pub current_health: f32,
    #[serde(default)]
    pub life: CompanionLifeState,
    #[serde(default)]
    pub revive_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompanionRosterRecord {
    pub capacity: u8,
    pub companions: BTreeMap<String, CompanionRecord>,
    pub next_serial: u64,
}

impl Default for CompanionRosterRecord {
    fn default() -> Self {
        Self {
            capacity: 3,
            companions: BTreeMap::new(),
            next_serial: 1,
        }
    }
}

impl CompanionRosterRecord {
    pub fn set_capacity(&mut self, capacity: u8) {
        self.capacity = capacity.clamp(1, 20);
    }

    pub fn add_companion(
        &mut self,
        preset_id: &str,
        allow_duplicate: bool,
    ) -> Result<String, CompanionError> {
        let preset = companion_preset(preset_id)
            .ok_or_else(|| CompanionError::UnknownPreset(preset_id.to_owned()))?;
        if self.companions.len() >= usize::from(self.capacity) {
            return Err(CompanionError::RosterFull {
                capacity: self.capacity,
            });
        }
        if !allow_duplicate
            && self
                .companions
                .values()
                .any(|companion| companion.preset_id == preset_id)
        {
            return Err(CompanionError::DuplicatePreset(preset_id.to_owned()));
        }

        let mut next = self.next_serial.max(1);
        let id = loop {
            let candidate = format!("companion-{next:016x}");
            next = next.saturating_add(1);
            if !self.companions.contains_key(&candidate) {
                break candidate;
            }
        };
        self.next_serial = next;
        self.companions.insert(
            id.clone(),
            CompanionRecord {
                preset_id: preset.id.to_owned(),
                kind: Some(preset.kind),
                level: 1,
                weapon_level: 0,
                current_health: preset.base_max_health,
                life: CompanionLifeState::Active,
                revive_count: 0,
            },
        );
        Ok(id)
    }

    pub fn dismiss(&mut self, companion_id: &str) -> Option<CompanionRecord> {
        self.companions.remove(companion_id)
    }

    pub fn active_count(&self) -> usize {
        self.companions
            .values()
            .filter(|companion| companion.life == CompanionLifeState::Active)
            .count()
    }

    pub fn stats(&self, companion_id: &str) -> Result<CompanionStats, CompanionError> {
        let record = self
            .companions
            .get(companion_id)
            .ok_or_else(|| CompanionError::UnknownCompanion(companion_id.to_owned()))?;
        let preset = companion_preset(&record.preset_id)
            .ok_or_else(|| CompanionError::UnknownPreset(record.preset_id.clone()))?;
        Ok(companion_stats(preset, record.level, record.weapon_level))
    }

    pub fn next_upgrade_cost(
        &self,
        companion_id: &str,
    ) -> Result<Option<CompanionUpgradeCost>, CompanionError> {
        let record = self
            .companions
            .get(companion_id)
            .ok_or_else(|| CompanionError::UnknownCompanion(companion_id.to_owned()))?;
        Ok((record.level < 5).then_some(CompanionUpgradeCost {
            gears: 8 * u32::from(record.level),
            energy_cores: u32::from(record.level),
        }))
    }

    /// Apply an already-paid body upgrade and return its authoritative cost.
    pub fn upgrade_companion(
        &mut self,
        companion_id: &str,
    ) -> Result<CompanionUpgradeCost, CompanionError> {
        let record = self
            .companions
            .get_mut(companion_id)
            .ok_or_else(|| CompanionError::UnknownCompanion(companion_id.to_owned()))?;
        if record.level >= 5 {
            return Err(CompanionError::MaxBodyLevel);
        }
        let preset = companion_preset(&record.preset_id)
            .ok_or_else(|| CompanionError::UnknownPreset(record.preset_id.clone()))?;
        let cost = CompanionUpgradeCost {
            gears: 8 * u32::from(record.level),
            energy_cores: u32::from(record.level),
        };
        let old_max = companion_stats(preset, record.level, record.weapon_level).max_health;
        record.level += 1;
        let new_max = companion_stats(preset, record.level, record.weapon_level).max_health;
        if record.life == CompanionLifeState::Active {
            record.current_health = (record.current_health + new_max - old_max).min(new_max);
        }
        Ok(cost)
    }

    pub fn next_weapon_upgrade_cost(
        &self,
        companion_id: &str,
    ) -> Result<Option<CompanionUpgradeCost>, CompanionError> {
        let record = self
            .companions
            .get(companion_id)
            .ok_or_else(|| CompanionError::UnknownCompanion(companion_id.to_owned()))?;
        let next = u32::from(record.weapon_level) + 1;
        Ok((record.weapon_level < 3).then_some(CompanionUpgradeCost {
            gears: 25 * next,
            energy_cores: 4 * next,
        }))
    }

    /// Apply an already-paid helper-weapon upgrade.
    pub fn upgrade_weapon(
        &mut self,
        companion_id: &str,
    ) -> Result<CompanionUpgradeCost, CompanionError> {
        let record = self
            .companions
            .get_mut(companion_id)
            .ok_or_else(|| CompanionError::UnknownCompanion(companion_id.to_owned()))?;
        if record.weapon_level >= 3 {
            return Err(CompanionError::MaxWeaponLevel);
        }
        let next = u32::from(record.weapon_level) + 1;
        let cost = CompanionUpgradeCost {
            gears: 25 * next,
            energy_cores: 4 * next,
        };
        record.weapon_level += 1;
        Ok(cost)
    }

    pub fn damage(
        &mut self,
        companion_id: &str,
        amount: f32,
    ) -> Result<CompanionLifeState, CompanionError> {
        let record = self
            .companions
            .get_mut(companion_id)
            .ok_or_else(|| CompanionError::UnknownCompanion(companion_id.to_owned()))?;
        if !amount.is_finite() || amount < 0.0 {
            return Err(CompanionError::InvalidDamage);
        }
        if record.life == CompanionLifeState::Downed {
            return Ok(record.life);
        }
        record.current_health = (record.current_health - amount).max(0.0);
        if record.current_health <= 0.0 {
            record.current_health = 0.0;
            record.life = CompanionLifeState::Downed;
        }
        Ok(record.life)
    }

    /// Clinic/manual revival for any persistent roster member.
    pub fn revive(&mut self, companion_id: &str) -> Result<(), CompanionError> {
        let record = self
            .companions
            .get_mut(companion_id)
            .ok_or_else(|| CompanionError::UnknownCompanion(companion_id.to_owned()))?;
        let preset = companion_preset(&record.preset_id)
            .ok_or_else(|| CompanionError::UnknownPreset(record.preset_id.clone()))?;
        if record.life == CompanionLifeState::Active {
            return Err(CompanionError::AlreadyActive);
        }
        record.life = CompanionLifeState::Active;
        record.current_health =
            companion_stats(preset, record.level, record.weapon_level).max_health;
        record.revive_count = record.revive_count.saturating_add(1);
        Ok(())
    }

    /// Source-compatible player-respawn restoration, while preserving paid
    /// levels instead of deleting and recreating a base-level companion.
    pub fn revive_durable_after_player_respawn(&mut self) -> Vec<String> {
        let ids: Vec<String> = self
            .companions
            .iter()
            .filter_map(|(id, record)| {
                let preset = companion_preset(&record.preset_id)?;
                (record.life == CompanionLifeState::Downed && preset.auto_revive_on_player_respawn)
                    .then(|| id.clone())
            })
            .collect();
        for id in &ids {
            let _ = self.revive(id);
        }
        ids
    }

    fn normalize(&mut self) {
        self.capacity = self.capacity.clamp(1, 20);
        self.next_serial = self.next_serial.max(1);
        self.companions.retain(|_, record| {
            let Some(preset) = companion_preset(&record.preset_id) else {
                return false;
            };
            record.kind = Some(preset.kind);
            record.level = record.level.clamp(1, 5);
            record.weapon_level = record.weapon_level.min(3);
            let max_health = companion_stats(preset, record.level, record.weapon_level).max_health;
            match record.life {
                CompanionLifeState::Active => {
                    if !record.current_health.is_finite() || record.current_health <= 0.0 {
                        record.current_health = max_health;
                    } else {
                        record.current_health = record.current_health.min(max_health);
                    }
                }
                CompanionLifeState::Downed => record.current_health = 0.0,
            }
            true
        });
    }
}

fn companion_stats(preset: &CompanionPresetDef, level: u8, weapon_level: u8) -> CompanionStats {
    let tier = f32::from(level.clamp(1, 5) - 1);
    let weapon_tier = f32::from(weapon_level.min(3));
    CompanionStats {
        max_health: (preset.base_max_health * (1.0 + 0.30 * tier)).floor(),
        attack_damage: preset.base_damage * (1.0 + 0.25 * tier) * (1.0 + 0.60 * weapon_tier),
        heal_amount: preset.base_heal * (1.0 + 0.25 * tier),
        move_speed: preset.base_move_speed * (1.0 + 0.08 * tier),
        attack_cooldown: preset.base_attack_cooldown / (1.0 + 0.40 * weapon_tier),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompanionStats {
    pub max_health: f32,
    pub attack_damage: f32,
    pub heal_amount: f32,
    pub move_speed: f32,
    pub attack_cooldown: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompanionUpgradeCost {
    pub gears: u32,
    pub energy_cores: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionError {
    UnknownPreset(String),
    UnknownCompanion(String),
    DuplicatePreset(String),
    RosterFull { capacity: u8 },
    MaxBodyLevel,
    MaxWeaponLevel,
    InvalidDamage,
    AlreadyActive,
}

// -------------------------------------------------------------------------
// Synthetic and lab-animal rescue records plus one-way reward milestones.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyntheticRescueDef {
    pub id: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub level: u8,
    pub position: [f32; 3],
    pub story_lines: &'static [&'static str],
}

pub const SYNTHETIC_RESCUES: [SyntheticRescueDef; SYNTHETIC_RESCUE_COUNT] = [
    SyntheticRescueDef {
        id: "L1_archivist_trace",
        name: "ARCHIVIST TRACE",
        title: "Keeper of the Star City Memories",
        level: 1,
        position: [230.0, 0.0, -60.0],
        story_lines: &[
            "You came back. I — I had stopped counting the days.",
            "Their captain caught me trying to wire the city memories off-site.",
            "Take the archive key. If Detroit falls, at least its songs survive.",
        ],
    },
    SyntheticRescueDef {
        id: "L1_dock_runner_vee",
        name: "DOCK RUNNER VEE",
        title: "Supply Hauler, North Pier",
        level: 1,
        position: [320.0, 0.0, -200.0],
        story_lines: &[
            "Ribcage cracked, comms gone — three of my haulers got reduced to scrap.",
            "I was running med-gel to the spire when the swarm folded over me.",
            "Tell the medics I'll walk back. Slowly. But I'll walk.",
        ],
    },
    SyntheticRescueDef {
        id: "L1_sister_rho",
        name: "SISTER RHO",
        title: "Spire Medic, Order of the Quiet Hand",
        level: 1,
        position: [440.0, 0.0, -50.0],
        story_lines: &[
            "Five wounded behind that door. I refused to leave them.",
            "The captain laughed and welded me into this cage as 'incentive.'",
            "Get them home. I'll handle the captain — with my bare hands if I must.",
        ],
    },
    SyntheticRescueDef {
        id: "L2_ranger_obsidian",
        name: "RANGER OBSIDIAN",
        title: "Long-Range Scout, Ash Border",
        level: 2,
        position: [-270.0, 0.0, -260.0],
        story_lines: &[
            "I got too close to a captain's command tent. Won't make that mistake twice.",
            "Their second wave is built different — armored, organized, *patient*.",
            "Push fast. The longer the line holds, the more they bleed in to break it.",
        ],
    },
    SyntheticRescueDef {
        id: "L2_smith_kira",
        name: "SMITH KIRA",
        title: "Field Weapons-Forger",
        level: 2,
        position: [-420.0, 0.0, -300.0],
        story_lines: &[
            "They wanted my schematics. They got my left forearm instead.",
            "I'll build you something for that — pick me up at the sanctuary.",
            "And tell whoever's running this assault: red sky, red rifles. I prefer matching gear.",
        ],
    },
    SyntheticRescueDef {
        id: "L2_yan_lost_twin",
        name: "YAN — LOST TWIN",
        title: "Survivor of the Second Wave",
        level: 2,
        position: [-360.0, 0.0, -460.0],
        story_lines: &[
            "My sister Lan — she's still out there. The void took her.",
            "We held the line at the second tower. She covered my retreat.",
            "If you reach the third front, find her. Tell her Yan is alive.",
        ],
    },
    SyntheticRescueDef {
        id: "L3_lan_lost_twin",
        name: "LAN — LOST TWIN",
        title: "Lost in the Void Front",
        level: 3,
        position: [-50.0, 0.0, 320.0],
        story_lines: &[
            "Yan… you found Yan? She's *alive*?",
            "I dreamed her voice. The void shows you what you can't bear to lose.",
            "I'm going home. Tell her I never stopped covering her retreat.",
        ],
    },
    SyntheticRescueDef {
        id: "L3_dr_inkwell",
        name: "DR. INKWELL",
        title: "Synthetic-Origin Researcher",
        level: 3,
        position: [-160.0, 0.0, 480.0],
        story_lines: &[
            "I know who built the hybrids. I have the files. They knew I'd talk.",
            "Char's lab — the Animatons came from there too. The same hands.",
            "Burn the command tower. Then come find me. We have work to do.",
        ],
    },
    SyntheticRescueDef {
        id: "L3_pilot_zeph",
        name: "PILOT ZEPH",
        title: "Cruiser Wing 3 — KIA Status: Recanted",
        level: 3,
        position: [-230.0, 0.0, 380.0],
        story_lines: &[
            "I rode my fighter all the way into the void mothership's belly.",
            "Punched out before the burn. They scooped me up before I hit dirt.",
            "I'm flying again. Get me a frame and I'll meet you in orbit.",
        ],
    },
    SyntheticRescueDef {
        id: "L5_ensign_helio",
        name: "ENSIGN HELIO",
        title: "Cruiser ASHUR — Survival Pod 7",
        level: 5,
        position: [90.0, 14.0, -40.0],
        story_lines: &[
            "Pod nav locked the moment my ship cracked open. I've been drifting for days.",
            "There's a third mothership behind the asteroid band — they're hiding it.",
            "Get me back to a hull. I can crew anything that flies.",
        ],
    },
    SyntheticRescueDef {
        id: "L5_captain_nova",
        name: "CAPTAIN NOVA",
        title: "Last Officer of the Orbital Fleet",
        level: 5,
        position: [-120.0, -8.0, 30.0],
        story_lines: &[
            "My fleet is gone. I logged every ship as it fell. Take the records.",
            "Earth doesn't know yet. Earth needs to know.",
            "I'll take command of whatever you can spare. We are not done.",
        ],
    },
    SyntheticRescueDef {
        id: "L5_navigator_ix",
        name: "NAVIGATOR IX",
        title: "Fold-Drive Navigator",
        level: 5,
        position: [40.0, 22.0, 80.0],
        story_lines: &[
            "I memorized the fold-coordinates home before my ship was breached.",
            "If you give me a console — any console — we can fold a relief wave in.",
            "Don't look at the void too long out here. It looks back. It remembers.",
        ],
    },
    SyntheticRescueDef {
        id: "L11_ranger_maple",
        name: "RANGER MAPLE",
        title: "MI Wilds Rescue Scout",
        level: 11,
        position: [2590.0, 8.0, 1765.0],
        story_lines: &[
            "The labs are moving through the treeline. Every night, a new tower wakes up.",
            "I marked the power blooms before they caged me. Follow the cyan flare.",
            "Those giant walkers are not patrols. They're hunting rare Animatons.",
        ],
    },
    SyntheticRescueDef {
        id: "L11_dr_heron",
        name: "DR. HERON",
        title: "Sanctuary Field Surgeon",
        level: 11,
        position: [3235.0, 8.0, 1140.0],
        story_lines: &[
            "They were cutting bond cores out of rescued pets. I tried to stop them.",
            "The clinic at Ashur can reverse the damage, but it needs Bio Crop and feed.",
            "Bring the rare ones home. They remember who helped them.",
        ],
    },
    SyntheticRescueDef {
        id: "L11_pilot_cedar",
        name: "PILOT CEDAR",
        title: "Downed Mothership Cartographer",
        level: 11,
        position: [3520.0, 10.0, 1760.0],
        story_lines: &[
            "I got inside the mothership before it tore itself open over the wilds.",
            "There are more carriers above the clouds. The wrecks here are only scouts.",
            "Free me and I'll mark their flight lanes for the sanctuary.",
        ],
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabAnimalRescueDef {
    pub id: &'static str,
    pub name: &'static str,
    pub flavor: &'static str,
}

pub const LAB_ANIMAL_RESCUES: [LabAnimalRescueDef; LAB_ANIMAL_RESCUE_COUNT] = [
    LabAnimalRescueDef {
        id: "lab_animal_kit",
        name: "KIT",
        flavor: "Bio-printed fox cub. Circuits glow under fur.",
    },
    LabAnimalRescueDef {
        id: "lab_animal_glim",
        name: "GLIM",
        flavor: "Bioluminescent glider. Wings folded for transport.",
    },
    LabAnimalRescueDef {
        id: "lab_animal_mossback",
        name: "MOSSBACK",
        flavor: "Tortoise-frame Animaton. Old, gentle, watchful.",
    },
    LabAnimalRescueDef {
        id: "lab_animal_rivet",
        name: "RIVET",
        flavor: "Silvermouse drone. Survived the purges by hiding.",
    },
];

pub fn synthetic_rescue_by_id(id: &str) -> Option<&'static SyntheticRescueDef> {
    SYNTHETIC_RESCUES.iter().find(|rescue| rescue.id == id)
}

pub fn lab_animal_rescue_by_id(id: &str) -> Option<&'static LabAnimalRescueDef> {
    LAB_ANIMAL_RESCUES.iter().find(|rescue| rescue.id == id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RescueMilestone {
    AllSyntheticsRescued,
    AllLabAnimalsFreed,
    VoidcrownDefeated,
    LegendaryCompanionGranted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescueReward {
    CompanionPreset(&'static str),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RescueUpdate {
    pub newly_recorded: bool,
    pub new_milestones: Vec<RescueMilestone>,
    pub rewards: Vec<RescueReward>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RescueProgressRecord {
    pub rescued_synthetic_ids: BTreeSet<String>,
    pub freed_lab_animal_ids: BTreeSet<String>,
    pub voidcrown_defeated: bool,
    /// One-way claim latches make rewards safe across duplicated events/save loads.
    pub claimed_milestones: BTreeSet<RescueMilestone>,
}

impl RescueProgressRecord {
    pub fn record_synthetic(&mut self, id: &str) -> Result<RescueUpdate, RescueError> {
        if synthetic_rescue_by_id(id).is_none() {
            return Err(RescueError::UnknownSynthetic(id.to_owned()));
        }
        let newly_recorded = self.rescued_synthetic_ids.insert(id.to_owned());
        Ok(self.reconcile_milestones(newly_recorded))
    }

    pub fn record_lab_animal(&mut self, id: &str) -> Result<RescueUpdate, RescueError> {
        if lab_animal_rescue_by_id(id).is_none() {
            return Err(RescueError::UnknownLabAnimal(id.to_owned()));
        }
        let newly_recorded = self.freed_lab_animal_ids.insert(id.to_owned());
        Ok(self.reconcile_milestones(newly_recorded))
    }

    pub fn record_voidcrown_defeat(&mut self) -> RescueUpdate {
        let newly_recorded = !self.voidcrown_defeated;
        self.voidcrown_defeated = true;
        self.reconcile_milestones(newly_recorded)
    }

    /// Call after loading a legacy record whose progress predates milestone
    /// latches. Returned rewards must be applied before the next save.
    pub fn reconcile_after_load(&mut self) -> RescueUpdate {
        self.reconcile_milestones(false)
    }

    pub fn legendary_gate_complete(&self) -> bool {
        self.rescued_synthetic_ids.len() == SYNTHETIC_RESCUE_COUNT
            && self.freed_lab_animal_ids.len() == LAB_ANIMAL_RESCUE_COUNT
            && self.voidcrown_defeated
    }

    fn reconcile_milestones(&mut self, newly_recorded: bool) -> RescueUpdate {
        let candidates = [
            RescueMilestone::AllSyntheticsRescued,
            RescueMilestone::AllLabAnimalsFreed,
            RescueMilestone::VoidcrownDefeated,
            RescueMilestone::LegendaryCompanionGranted,
        ];
        let mut update = RescueUpdate {
            newly_recorded,
            ..Default::default()
        };
        for milestone in candidates {
            if self.milestone_complete(milestone) && self.claimed_milestones.insert(milestone) {
                update.new_milestones.push(milestone);
                if milestone == RescueMilestone::LegendaryCompanionGranted {
                    update
                        .rewards
                        .push(RescueReward::CompanionPreset("MiniGeneralVoidcrown"));
                }
            }
        }
        update
    }

    fn milestone_complete(&self, milestone: RescueMilestone) -> bool {
        match milestone {
            RescueMilestone::AllSyntheticsRescued => {
                self.rescued_synthetic_ids.len() == SYNTHETIC_RESCUE_COUNT
            }
            RescueMilestone::AllLabAnimalsFreed => {
                self.freed_lab_animal_ids.len() == LAB_ANIMAL_RESCUE_COUNT
            }
            RescueMilestone::VoidcrownDefeated => self.voidcrown_defeated,
            RescueMilestone::LegendaryCompanionGranted => self.legendary_gate_complete(),
        }
    }

    fn normalize(&mut self) {
        self.rescued_synthetic_ids
            .retain(|id| synthetic_rescue_by_id(id).is_some());
        self.freed_lab_animal_ids
            .retain(|id| lab_animal_rescue_by_id(id).is_some());
        let claimed = std::mem::take(&mut self.claimed_milestones);
        self.claimed_milestones = claimed
            .into_iter()
            .filter(|milestone| self.milestone_complete(*milestone))
            .collect();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RescueError {
    UnknownSynthetic(String),
    UnknownLabAnimal(String),
}

// -------------------------------------------------------------------------
// Stable deterministic hashing. This is deliberately not DefaultHasher,
// whose output is not a cross-version save/gameplay contract.

fn stable_roll(parts: &[&[u8]]) -> f32 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for part in parts {
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let value = mix64(hash) >> 40;
    value as f32 / (1_u32 << 24) as f32
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn house(profile: &mut PlayerBioRecord, creature_id: &str, species_id: &str) {
        let species = species_by_id(species_id).expect("test species should exist");
        profile.creatures.insert(
            creature_id.to_owned(),
            CapturedCreatureRecord::from_species(creature_id, species),
        );
        profile.dex_species_ids.insert(species_id.to_owned());
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 1.0e-5, "{a} != {b}");
    }

    #[test]
    fn source_catalog_has_all_133_unique_species_and_taxonomy() {
        assert_eq!(BIO_SPECIES.len(), 133);
        assert_eq!(
            BIO_SPECIES
                .iter()
                .take(5)
                .map(|species| species.id)
                .collect::<Vec<_>>(),
            [
                "robofox",
                "crystalbeetle",
                "hoverserpent",
                "neonowl",
                "voltfrog",
            ]
        );
        let ids: BTreeSet<_> = BIO_SPECIES.iter().map(|species| species.id).collect();
        let archetypes: BTreeSet<_> = BIO_SPECIES
            .iter()
            .map(|species| species.archetype)
            .collect();
        let elements: BTreeSet<_> = BIO_SPECIES.iter().map(|species| species.element).collect();
        assert_eq!(ids.len(), BIO_SPECIES_COUNT);
        assert_eq!(archetypes.len(), 24);
        assert_eq!(elements.len(), 11);
        assert_eq!(species_by_element(BioElement::Dragon).count(), 4);
        assert_eq!(species_by_id("pearlbot").unwrap().rarity, BioRarity::Rare);
        assert!(CreatureArchetype::Drone.is_flyer());
        assert!(!CreatureArchetype::Bot.is_flyer());
        assert_eq!(BioElement::Flame.palette().ui_hex, "#ff7038");
        assert_eq!(BioElement::Crystal.palette().emissive, [0.60, 1.0, 1.0]);

        assert_eq!(weighted_species(913).id, weighted_species(913).id);
        assert!(species_by_id(weighted_species(42).id).is_some());
    }

    #[test]
    fn rarity_stats_chances_and_garden_tiers_match_heavy_water() {
        assert_eq!(
            BioRarity::Common.base_stats(),
            BioStats::new(60.0, 10.0, 1.0)
        );
        assert_eq!(
            BioRarity::Legendary.base_stats(),
            BioStats::new(130.0, 24.0, 1.0)
        );
        close(BioRarity::Common.base_capture_chance(), 0.55);
        close(BioRarity::Legendary.base_capture_chance(), 0.14);
        assert_eq!(BioRarity::Common.wild_spawn_weight(), 8);
        assert_eq!(BioRarity::Legendary.wild_spawn_weight(), 1);
        assert_eq!((garden_capture_cap(1), garden_capture_bonus(1)), (15, 0.0));
        assert_eq!((garden_capture_cap(2), garden_capture_bonus(2)), (30, 0.15));
        assert_eq!((garden_capture_cap(3), garden_capture_bonus(3)), (50, 0.30));
        assert_eq!(
            (
                lab_companion_cap(1),
                lab_companion_cap(2),
                lab_companion_cap(3)
            ),
            (3, 5, 8)
        );
    }

    #[test]
    fn capture_is_deterministic_and_weakened_health_can_change_the_result() {
        let species = species_by_id("robofox").unwrap();
        let mut flipping_attempt = None;
        for attempt in 0..10_000 {
            let full = capture_odds("player-a", species, 1, 60.0, 60.0, 77, attempt).unwrap();
            let weak = capture_odds("player-a", species, 1, 0.0, 60.0, 77, attempt).unwrap();
            close(full.total_chance, 0.55);
            close(weak.total_chance, 0.90);
            assert_eq!(full.roll, weak.roll);
            if full.roll >= full.total_chance && weak.roll < weak.total_chance {
                flipping_attempt = Some(attempt);
                break;
            }
        }
        let attempt = flipping_attempt.expect("deterministic stream should expose health bonus");
        let first = capture_odds("player-a", species, 1, 0.0, 60.0, 77, attempt).unwrap();
        let second = capture_odds("player-a", species, 1, 0.0, 60.0, 77, attempt).unwrap();
        assert_eq!(first, second);

        let mut profile = PlayerBioRecord::default();
        let result = profile
            .attempt_capture(
                "player-a",
                CaptureRequest {
                    species_id: species.id,
                    current_health: 0.0,
                    max_health: 60.0,
                    encounter_seed: 77,
                    attempt_index: attempt,
                },
            )
            .unwrap();
        let CaptureResolution::Captured { creature_id, .. } = result else {
            panic!("weakened target should capture for selected deterministic roll");
        };
        assert!(profile.creatures.contains_key(&creature_id));
        assert!(profile.dex_species_ids.contains(species.id));
        assert_eq!(
            capture_odds("player-a", species, 1, f32::NAN, 60.0, 1, 0),
            Err(CaptureError::InvalidHealth)
        );
    }

    #[test]
    fn capture_cap_is_enforced_before_an_orb_can_add_another_creature() {
        let mut profile = PlayerBioRecord::default();
        for index in 0..15 {
            house(&mut profile, &format!("resident-{index}"), "robofox");
        }
        assert_eq!(profile.garden_capture_cap(), 15);
        assert_eq!(
            profile.attempt_capture(
                "player-a",
                CaptureRequest {
                    species_id: "voltfrog",
                    current_health: 0.0,
                    max_health: 60.0,
                    encounter_seed: 1,
                    attempt_index: 0,
                },
            ),
            Err(CaptureError::GardenRosterFull { capacity: 15 })
        );
        profile.garden_level = 2;
        assert_eq!(profile.garden_capture_cap(), 30);
    }

    #[test]
    fn dex_active_selection_care_and_passives_follow_source_policy() {
        let mut profile = PlayerBioRecord::default();
        house(&mut profile, "fox", "robofox");
        house(&mut profile, "frog", "voltfrog");
        house(&mut profile, "turtle", "riverturtle");
        house(&mut profile, "owl", "neonowl");
        profile.select_active(["fox", "frog", "turtle"]).unwrap();
        let previous = profile.active_creature_ids.clone();
        assert_eq!(
            profile.select_active(["fox", "frog", "turtle", "owl"]),
            Err(ActivePetError::TooManyActive { capacity: 3 })
        );
        assert_eq!(profile.active_creature_ids, previous);

        let fox_before = profile.creatures["fox"].clone();
        let first = profile.care_for("fox", CareItem::BioCrop).unwrap();
        assert!(!first.bonded);
        assert_eq!(first.care, 1);
        let second = profile.care_for("fox", CareItem::AnimatonFeed).unwrap();
        assert!(second.bonded);
        assert_eq!((second.bond_level, second.care, second.level), (1, 0, 2));
        let fox = &profile.creatures["fox"];
        close(fox.max_health, fox_before.max_health + 8.0);
        close(fox.attack, fox_before.attack + 2.0);
        close(fox.speed, fox_before.speed + 0.03);

        let bonds = profile.bond_bonuses();
        assert!(bonds.damage_multiplier > 1.0);
        assert!(bonds.fire_rate_multiplier > 1.0);
        assert!(bonds.damage_reduction > 0.0);
        let active = profile.active_pet_bonuses();
        assert!(active.damage_multiplier > 1.0);
        assert!(active.fire_rate_multiplier > 1.0);
        assert!(active.shield_regen_per_second > 0.0);

        profile.remove_captured("fox").unwrap();
        assert!(profile.dex_species_ids.contains("robofox"));
        assert!(!profile.active_creature_ids.iter().any(|id| id == "fox"));
    }

    #[test]
    fn garden_has_twelve_plots_catches_up_growth_and_harvests_deterministically() {
        let mut garden = GardenRecord::default();
        assert_eq!(garden.plots.len(), 12);
        assert_eq!(garden.plant(0, 1_000).unwrap().bio_seeds_spent, 1);
        assert_eq!(garden.plots[0].stage, GrowthStage::Seeded);
        assert_eq!(garden.advance_growth(22_999), 0);
        assert_eq!(garden.advance_growth(23_000), 1);
        assert_eq!(garden.plots[0].stage, GrowthStage::Sprout);
        assert_eq!(garden.advance_growth(45_000), 1);
        assert_eq!(garden.plots[0].stage, GrowthStage::Grown);

        let mut restored_copy = garden.clone();
        let first = garden.harvest("slot-0", 0, 45_000).unwrap();
        let second = restored_copy.harvest("slot-0", 0, 45_000).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.bio_crops, 2);
        assert!(first.animaton_feed <= 1 && first.bio_seeds <= 1);
        assert_eq!(garden.plots[0].stage, GrowthStage::Empty);
        assert_eq!(
            garden.harvest("slot-0", 0, 45_000),
            Err(GardenError::PlotEmpty(0))
        );

        garden.plant(11, 50_000).unwrap();
        assert_eq!(garden.advance_growth(100_000), 1);
        assert_eq!(garden.plots[11].stage, GrowthStage::Grown);
    }

    #[test]
    fn helper_roster_preserves_upgrades_through_death_and_policy_revive() {
        let ids: BTreeSet<_> = COMPANION_PRESETS.iter().map(|preset| preset.id).collect();
        assert_eq!(ids.len(), 9);
        let mut roster = CompanionRosterRecord::default();
        let spark = roster.add_companion("SparkPup", false).unwrap();
        let guardian = roster.add_companion("GuardianUnit", false).unwrap();
        roster.add_companion("MedicDrone", false).unwrap();
        assert_eq!(
            roster.add_companion("RoboDragon", false),
            Err(CompanionError::RosterFull { capacity: 3 })
        );

        assert_eq!(
            roster.upgrade_companion(&spark).unwrap(),
            CompanionUpgradeCost {
                gears: 8,
                energy_cores: 1,
            }
        );
        assert_eq!(
            roster.upgrade_weapon(&spark).unwrap(),
            CompanionUpgradeCost {
                gears: 25,
                energy_cores: 4,
            }
        );
        assert_eq!(roster.companions[&spark].level, 2);
        assert_eq!(roster.companions[&spark].weapon_level, 1);

        assert_eq!(
            roster.damage(&spark, 10_000.0).unwrap(),
            CompanionLifeState::Downed
        );
        assert_eq!(
            roster.damage(&guardian, 10_000.0).unwrap(),
            CompanionLifeState::Downed
        );
        let revived = roster.revive_durable_after_player_respawn();
        assert_eq!(revived.as_slice(), std::slice::from_ref(&spark));
        assert_eq!(roster.companions[&spark].level, 2);
        assert_eq!(roster.companions[&spark].weapon_level, 1);
        assert_eq!(
            roster.companions[&guardian].life,
            CompanionLifeState::Downed
        );
        roster.revive(&guardian).unwrap();
        assert_eq!(
            roster.companions[&guardian].life,
            CompanionLifeState::Active
        );
    }

    #[test]
    fn helper_upgrade_formulas_reach_the_source_caps() {
        let mut roster = CompanionRosterRecord::default();
        let id = roster.add_companion("GuardianUnit", false).unwrap();
        let base = roster.stats(&id).unwrap();
        for level in 1..5 {
            let cost = roster.upgrade_companion(&id).unwrap();
            assert_eq!(cost.gears, 8 * level);
            assert_eq!(cost.energy_cores, level);
        }
        assert_eq!(roster.companions[&id].level, 5);
        assert_eq!(
            roster.upgrade_companion(&id),
            Err(CompanionError::MaxBodyLevel)
        );
        for tier in 1..=3 {
            let cost = roster.upgrade_weapon(&id).unwrap();
            assert_eq!(cost.gears, 25 * tier);
            assert_eq!(cost.energy_cores, 4 * tier);
        }
        let maxed = roster.stats(&id).unwrap();
        assert!(maxed.max_health > base.max_health);
        assert!(maxed.attack_damage > base.attack_damage);
        assert!(maxed.attack_cooldown < base.attack_cooldown);
        assert_eq!(
            roster.upgrade_weapon(&id),
            Err(CompanionError::MaxWeaponLevel)
        );
    }

    #[test]
    fn all_rescue_records_are_unique_and_final_reward_is_idempotent() {
        assert_eq!(SYNTHETIC_RESCUES.len(), 15);
        assert_eq!(LAB_ANIMAL_RESCUES.len(), 4);
        assert_eq!(
            SYNTHETIC_RESCUES
                .iter()
                .map(|rescue| rescue.id)
                .collect::<BTreeSet<_>>()
                .len(),
            15
        );
        assert_eq!(
            LAB_ANIMAL_RESCUES
                .iter()
                .map(|rescue| rescue.id)
                .collect::<BTreeSet<_>>()
                .len(),
            4
        );
        for level in [1, 2, 3, 5, 11] {
            assert_eq!(
                SYNTHETIC_RESCUES
                    .iter()
                    .filter(|rescue| rescue.level == level)
                    .count(),
                3
            );
        }

        let mut progress = RescueProgressRecord::default();
        for rescue in &SYNTHETIC_RESCUES {
            progress.record_synthetic(rescue.id).unwrap();
        }
        assert!(progress
            .claimed_milestones
            .contains(&RescueMilestone::AllSyntheticsRescued));
        for animal in &LAB_ANIMAL_RESCUES {
            progress.record_lab_animal(animal.id).unwrap();
        }
        assert!(!progress.legendary_gate_complete());
        let final_update = progress.record_voidcrown_defeat();
        assert_eq!(
            final_update.new_milestones,
            [
                RescueMilestone::VoidcrownDefeated,
                RescueMilestone::LegendaryCompanionGranted,
            ]
        );
        assert_eq!(
            final_update.rewards,
            [RescueReward::CompanionPreset("MiniGeneralVoidcrown")]
        );
        assert!(progress.legendary_gate_complete());

        let duplicate = progress.record_voidcrown_defeat();
        assert!(!duplicate.newly_recorded);
        assert!(duplicate.new_milestones.is_empty());
        assert!(duplicate.rewards.is_empty());
        assert_eq!(
            progress.record_synthetic("invented-rescue"),
            Err(RescueError::UnknownSynthetic("invented-rescue".to_owned()))
        );
    }

    #[test]
    fn legacy_rescue_completion_reconciles_reward_once_after_load() {
        let mut progress = RescueProgressRecord {
            rescued_synthetic_ids: SYNTHETIC_RESCUES
                .iter()
                .map(|rescue| rescue.id.to_owned())
                .collect(),
            freed_lab_animal_ids: LAB_ANIMAL_RESCUES
                .iter()
                .map(|rescue| rescue.id.to_owned())
                .collect(),
            voidcrown_defeated: true,
            claimed_milestones: BTreeSet::new(),
        };
        let first = progress.reconcile_after_load();
        assert_eq!(first.new_milestones.len(), 4);
        assert_eq!(first.rewards.len(), 1);
        let second = progress.reconcile_after_load();
        assert!(second.new_milestones.is_empty());
        assert!(second.rewards.is_empty());
    }

    #[test]
    fn player_keyed_save_round_trips_and_legacy_defaults_normalize() {
        let mut save = HeavyBioSave::default();
        let slot_zero = save.player_mut("local-slot-0");
        house(slot_zero, "fox", "robofox");
        slot_zero.select_active(["fox"]).unwrap();
        slot_zero.garden.plant(2, 1_000).unwrap();
        slot_zero
            .companions
            .add_companion("SparkPup", false)
            .unwrap();
        slot_zero
            .rescues
            .record_synthetic("L1_archivist_trace")
            .unwrap();
        save.player_mut("local-slot-1").garden_level = 3;

        let encoded = serde_json::to_string(&save).unwrap();
        let decoded: HeavyBioSave = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, save);
        assert_eq!(
            decoded.players.keys().cloned().collect::<Vec<_>>(),
            ["local-slot-0".to_owned(), "local-slot-1".to_owned(),]
        );

        let mut legacy: HeavyBioSave = serde_json::from_str(
            r#"{"players":{"slot":{"garden_level":99,"dex_species_ids":["fake"],"creatures":{"old":{"speciesId":"robofox","hp":84.0,"attackPower":17.0,"speed":1.2,"bondLevel":3}}}}}"#,
        )
        .unwrap();
        legacy.normalize();
        let profile = legacy.player("slot").unwrap();
        assert_eq!(legacy.schema_version, HEAVY_BIO_SCHEMA_VERSION);
        assert_eq!(profile.garden_level, 3);
        assert_eq!(
            profile.dex_species_ids,
            BTreeSet::from(["robofox".to_owned()])
        );
        close(profile.creatures["old"].max_health, 84.0);
        close(profile.creatures["old"].current_health, 84.0);
        close(profile.creatures["old"].attack, 17.0);
        assert_eq!(profile.creatures["old"].bond_level, 3);
        assert_eq!(profile.garden.plots.len(), 12);
        assert_eq!(profile.companions.capacity, 3);
    }
}
