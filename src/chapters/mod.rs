//! Starfall I chapter system. Replaces the old wave-based difficulty loop.
//!
//! A chapter is a hand-authored sequence of `EncounterStep`s. The
//! `chapter_plugin` advances through the steps when their completion
//! condition fires (group cleared, dialogue timer elapsed, boss dead).
//!
//! All 14 chapters are playable slices of the new cartoon star-beam
//! platformer/RPG setup. Each chapter sets a biome, then seeds star tools,
//! companion recruits, alien waves, dragon bosses, and mirror-human rivals.

use bevy::prelude::*;
use std::sync::OnceLock;

use crate::components::discoverable::{DiscoverableKind, PuzzleArchetype};
use crate::components::enemy::EnemyType;
use crate::components::faction::Faction;
use crate::robot_pets::{RobotPartKind, RobotPetRole};
use crate::upgrades::TechUpgradeId;

// ── Chapter ID ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChapterId(pub u8);

impl ChapterId {
    pub const FIRST: Self = ChapterId(1);
    pub const LAST: Self = ChapterId(14);
    pub fn next(self) -> Option<Self> {
        if self.0 < Self::LAST.0 {
            Some(ChapterId(self.0 + 1))
        } else {
            None
        }
    }
    pub fn index(self) -> usize {
        self.0 as usize - 1
    }
}

// ── World Map / Fast Travel Locations ────────────────────────────────────────
pub const EVEREST_RANGE_MILES: f32 = 200.0;
pub const EVEREST_RANGE_UNITS_PER_MILE: f32 = 100.0;
pub const EVEREST_RANGE_WORLD_SIZE: f32 = EVEREST_RANGE_MILES * EVEREST_RANGE_UNITS_PER_MILE;
pub const EVEREST_RANGE_HALF_EXTENT: f32 = EVEREST_RANGE_WORLD_SIZE * 0.5;

#[derive(Debug, Clone, Copy)]
pub struct ChapterMapLocation {
    pub id: ChapterId,
    pub anchor_id: &'static str,
    pub region: &'static str,
    pub landmark: &'static str,
    pub x: f32,
    pub z: f32,
    pub facing_yaw: f32,
}

/// Non-chapter world-map destination. These reuse the active chapter session
/// while moving the party to a stable authored world anchor.
#[derive(Debug, Clone, Copy)]
pub struct WorldMapTravelPoint {
    pub chapter: ChapterId,
    pub anchor_id: &'static str,
    pub label: &'static str,
    pub x: f32,
    pub z: f32,
}

pub const RACE_REGION_TRAVEL_POINTS: [WorldMapTravelPoint; 2] = [
    WorldMapTravelPoint {
        chapter: ChapterId(10),
        anchor_id: "race_region_north_gate",
        label: "Starfall Grand Raceway — North Gate",
        x: 8_150.0,
        z: -6_500.0,
    },
    WorldMapTravelPoint {
        chapter: ChapterId(11),
        anchor_id: "race_region_pit_lane",
        label: "Starfall Grand Raceway — Pit Lane",
        x: 6_350.0,
        z: -8_600.0,
    },
];

/// Authored mountain-cave destination shared by world generation, dungeon
/// activation, and the fast-travel map.
#[derive(Debug, Clone, Copy)]
pub struct SecretCaveLocation {
    pub chapter: ChapterId,
    pub anchor_id: &'static str,
    pub label: &'static str,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub length: f32,
}

pub const SECRET_CAVE_LOCATIONS: [SecretCaveLocation; 14] = [
    SecretCaveLocation {
        chapter: ChapterId(1),
        anchor_id: "secret_cave_ch01",
        label: "Star Engine Grotto",
        x: 42.0,
        z: 54.0,
        yaw: -0.55,
        length: 38.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(2),
        anchor_id: "secret_cave_ch02",
        label: "Rift-Glass Underpass",
        x: 2340.0,
        z: 1780.0,
        yaw: -1.10,
        length: 42.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(3),
        anchor_id: "secret_cave_ch03",
        label: "Sister Starwell Cave",
        x: -2380.0,
        z: 2720.0,
        yaw: 0.70,
        length: 36.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(4),
        anchor_id: "secret_cave_ch04",
        label: "Brother Trial Burrow",
        x: 3200.0,
        z: -1800.0,
        yaw: 0.25,
        length: 40.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(5),
        anchor_id: "secret_cave_ch05",
        label: "Mirror Sludge Cavern",
        x: -3320.0,
        z: 790.0,
        yaw: 1.20,
        length: 40.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(6),
        anchor_id: "secret_cave_ch06",
        label: "Crownroot Ice Cave",
        x: -8850.0,
        z: -8200.0,
        yaw: 0.78,
        length: 46.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(7),
        anchor_id: "secret_cave_ch07",
        label: "Ember Breathing Hollow",
        x: -7950.0,
        z: 4200.0,
        yaw: -0.40,
        length: 44.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(8),
        anchor_id: "secret_cave_ch08",
        label: "Fangroot Scrap Tunnel",
        x: -6000.0,
        z: 8200.0,
        yaw: -0.90,
        length: 38.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(9),
        anchor_id: "secret_cave_ch09",
        label: "Pink Flame Root Cave",
        x: 6300.0,
        z: 260.0,
        yaw: 1.00,
        length: 36.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(10),
        anchor_id: "secret_cave_ch10",
        label: "Granite Echo Cave",
        x: 8900.0,
        z: 5200.0,
        yaw: -1.35,
        length: 46.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(11),
        anchor_id: "secret_cave_ch11",
        label: "Icebreaker Under-Cave",
        x: 2820.0,
        z: -8650.0,
        yaw: 0.95,
        length: 42.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(12),
        anchor_id: "secret_cave_ch12",
        label: "Mana Gear Grotto",
        x: 5750.0,
        z: -3450.0,
        yaw: 0.35,
        length: 38.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(13),
        anchor_id: "secret_cave_ch13",
        label: "Crown Gate Underpath",
        x: -4700.0,
        z: -6550.0,
        yaw: -0.20,
        length: 42.0,
    },
    SecretCaveLocation {
        chapter: ChapterId(14),
        anchor_id: "secret_cave_ch14",
        label: "Starfall Core Hollow",
        x: 650.0,
        z: -8300.0,
        yaw: 0.0,
        length: 44.0,
    },
];

pub fn secret_cave_location(chapter: ChapterId) -> Option<&'static SecretCaveLocation> {
    SECRET_CAVE_LOCATIONS
        .iter()
        .find(|location| location.chapter == chapter)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapSettlementKind {
    City,
    Village,
    Harbor,
    Outpost,
}

#[derive(Debug, Clone, Copy)]
pub struct MapSettlement {
    pub anchor_id: &'static str,
    pub name: &'static str,
    pub region: &'static str,
    pub kind: MapSettlementKind,
    pub reward_id: &'static str,
    pub x: f32,
    pub z: f32,
    pub facing_yaw: f32,
    /// Corresponding `WorldSiteId` if this settlement is tracked in the
    /// `WorldSiteRegistry`. When `Some`, the chapter-select map renders the
    /// world-site badge instead of the generic settlement marker.
    pub site_id: Option<u16>,
}

impl ChapterMapLocation {
    pub fn spawn_position(&self, anchor: Vec3, player_index: u8) -> Vec3 {
        const OFFSETS: [Vec3; 4] = [
            Vec3::new(0.0, 2.4, 0.0),
            Vec3::new(3.2, 2.4, 0.0),
            Vec3::new(0.0, 2.4, 3.2),
            Vec3::new(3.2, 2.4, 3.2),
        ];
        let offset = OFFSETS[player_index.min(3) as usize];
        anchor + Quat::from_rotation_y(self.facing_yaw) * offset
    }
}

pub fn chapter_map_locations() -> &'static [ChapterMapLocation] {
    &[
        ChapterMapLocation {
            id: ChapterId(1),
            anchor_id: "chapter_map_ch01",
            region: "Starfall Lab",
            landmark: "Invasion Gate",
            x: 22.0,
            z: 28.0,
            facing_yaw: -0.25,
        },
        ChapterMapLocation {
            id: ChapterId(2),
            anchor_id: "chapter_map_ch02",
            region: "Rift City",
            landmark: "Tony's Shortcut",
            x: 2200.0,
            z: 1600.0,
            facing_yaw: -0.80,
        },
        ChapterMapLocation {
            id: ChapterId(3),
            anchor_id: "chapter_map_ch03",
            region: "Sister Starwell",
            landmark: "Sanctum Rise",
            x: -2200.0,
            z: 2500.0,
            facing_yaw: 0.70,
        },
        ChapterMapLocation {
            id: ChapterId(4),
            anchor_id: "chapter_map_ch04",
            region: "Brother Trials",
            landmark: "Training Court",
            x: 3000.0,
            z: -1600.0,
            facing_yaw: 0.35,
        },
        ChapterMapLocation {
            id: ChapterId(5),
            anchor_id: "chapter_map_ch05",
            region: "Bile Lab",
            landmark: "Mirror Sludge",
            x: -3100.0,
            z: 600.0,
            facing_yaw: 1.15,
        },
        ChapterMapLocation {
            id: ChapterId(6),
            anchor_id: "chapter_map_ch06",
            region: "Everest Crown Range",
            landmark: "Collosar's Lair",
            x: -8500.0,
            z: -7800.0,
            facing_yaw: 0.12,
        },
        ChapterMapLocation {
            id: ChapterId(7),
            anchor_id: "chapter_map_ch07",
            region: "Ember Nest",
            landmark: "Tarack's Furnace",
            x: -7600.0,
            z: 3800.0,
            facing_yaw: -0.52,
        },
        ChapterMapLocation {
            id: ChapterId(8),
            anchor_id: "chapter_map_ch08",
            region: "Fangroot Wildland",
            landmark: "Shread's Scrapwing",
            x: -5600.0,
            z: 7800.0,
            facing_yaw: -1.05,
        },
        ChapterMapLocation {
            id: ChapterId(9),
            anchor_id: "chapter_map_ch09",
            region: "Pink Flame Garden",
            landmark: "Rift Bloom Walk",
            x: 6600.0,
            z: 700.0,
            facing_yaw: 0.84,
        },
        ChapterMapLocation {
            id: ChapterId(10),
            anchor_id: "chapter_map_ch10",
            region: "Colorado Rockies",
            landmark: "Ragar's Granite Gate",
            x: 8500.0,
            z: 4800.0,
            facing_yaw: -1.28,
        },
        ChapterMapLocation {
            id: ChapterId(11),
            anchor_id: "chapter_map_ch11",
            region: "Antarctica Outpost",
            landmark: "Blackskull Icebreaker",
            x: 3200.0,
            z: -8200.0,
            facing_yaw: 0.42,
        },
        ChapterMapLocation {
            id: ChapterId(12),
            anchor_id: "chapter_map_ch12",
            region: "Mana Switchworks",
            landmark: "Gear Relay Lane",
            x: 5400.0,
            z: -3100.0,
            facing_yaw: 0.35,
        },
        ChapterMapLocation {
            id: ChapterId(13),
            anchor_id: "chapter_map_ch13",
            region: "Crown Gate",
            landmark: "Scallarian Front",
            x: -4300.0,
            z: -6100.0,
            facing_yaw: -0.20,
        },
        ChapterMapLocation {
            id: ChapterId(14),
            anchor_id: "chapter_map_ch14",
            region: "Starfall Core",
            landmark: "Sky Closure",
            x: 300.0,
            z: -7800.0,
            facing_yaw: 0.0,
        },
    ]
}

pub fn chapter_map_location(id: ChapterId) -> Option<ChapterMapLocation> {
    chapter_map_locations()
        .iter()
        .copied()
        .find(|location| location.id == id)
}

pub fn map_settlements() -> &'static [MapSettlement] {
    &[
        MapSettlement {
            anchor_id: "settlement_riftglass_village",
            name: "Riftglass Village",
            region: "Rift Foothills",
            kind: MapSettlementKind::Village,
            reward_id: "settlement_riftglass_cache",
            x: 1500.0,
            z: 3300.0,
            facing_yaw: -0.45,
            site_id: Some(2),
        },
        MapSettlement {
            anchor_id: "settlement_cloudrail_city",
            name: "Cloudrail City",
            region: "High Sky Rail",
            kind: MapSettlementKind::City,
            reward_id: "settlement_cloudrail_cache",
            x: 4200.0,
            z: 4300.0,
            facing_yaw: -1.10,
            site_id: Some(4),
        },
        MapSettlement {
            anchor_id: "settlement_lantern_hamlet",
            name: "Lantern Hamlet",
            region: "Tibet Snow Road",
            kind: MapSettlementKind::Village,
            reward_id: "settlement_lantern_cache",
            x: -6900.0,
            z: -4200.0,
            facing_yaw: 0.28,
            site_id: Some(5),
        },
        MapSettlement {
            anchor_id: "settlement_star_orchard",
            name: "Star Orchard",
            region: "Fangroot Meadow",
            kind: MapSettlementKind::Village,
            reward_id: "settlement_star_orchard_cache",
            x: -3800.0,
            z: 5700.0,
            facing_yaw: 0.75,
            site_id: Some(6),
        },
        MapSettlement {
            anchor_id: "settlement_frost_harbor",
            name: "Frost Harbor",
            region: "Antarctic Range",
            kind: MapSettlementKind::Harbor,
            reward_id: "settlement_frost_harbor_cache",
            x: 2300.0,
            z: -6700.0,
            facing_yaw: 0.20,
            site_id: Some(7),
        },
        MapSettlement {
            anchor_id: "settlement_granite_market",
            name: "Granite Market",
            region: "Rockies Gate",
            kind: MapSettlementKind::Village,
            reward_id: "settlement_granite_market_cache",
            x: 7100.0,
            z: 2800.0,
            facing_yaw: -0.82,
            site_id: Some(8),
        },
        MapSettlement {
            anchor_id: "settlement_switchwork_borough",
            name: "Switchwork Borough",
            region: "Mana Switchworks",
            kind: MapSettlementKind::City,
            reward_id: "settlement_switchwork_cache",
            x: 4400.0,
            z: -4300.0,
            facing_yaw: 0.58,
            site_id: Some(9),
        },
        MapSettlement {
            anchor_id: "settlement_starfell_outpost",
            name: "Starfell Outpost",
            region: "Crown Road",
            kind: MapSettlementKind::Outpost,
            reward_id: "settlement_starfell_cache",
            x: -1800.0,
            z: -6900.0,
            facing_yaw: -0.15,
            site_id: Some(3),
        },
    ]
}

// ── Biome ─────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Biome {
    StarfallLab,
    RiftCity,
    SisterSanctum,
    BrotherTrialGrounds,
    BileLab,
    TibetPeak,
    EmberNest,
    DragonWildland,
    PinkFlameGarden,
    RockiesDomain,
    AntarcticaOutpost,
    PuzzleBattleground,
    CrownGate,
    StarfallArena,
}

impl Biome {
    /// Returns (bloom_intensity, fog_density) configuration for dynamic post-processing.
    pub fn atmosphere_settings(&self) -> (f32, f32) {
        use Biome::*;
        match self {
            StarfallLab => (0.35, 0.00018),
            RiftCity => (0.4, 0.00025),
            PinkFlameGarden => (0.45, 0.00015),
            AntarcticaOutpost => (0.2, 0.00030),
            TibetPeak => (0.15, 0.00010),
            _ => (0.25, 0.00018),
        }
    }

    /// Returns (sky, fog, ground, accent) palette for this biome.
    pub fn palette(&self) -> (Color, Color, Color, Color) {
        use Biome::*;
        match self {
            StarfallLab => (
                c(0.18, 0.10, 0.06),
                c(0.10, 0.06, 0.04),
                c(0.30, 0.25, 0.22),
                c(1.0, 0.4, 0.1),
            ),
            RiftCity => (
                c(0.05, 0.20, 0.05),
                c(0.04, 0.10, 0.04),
                c(0.18, 0.25, 0.10),
                c(0.4, 1.0, 0.2),
            ),
            SisterSanctum => (
                c(0.10, 0.10, 0.18),
                c(0.05, 0.05, 0.10),
                c(0.30, 0.30, 0.40),
                c(0.4, 0.85, 1.0),
            ),
            BrotherTrialGrounds => (
                c(0.15, 0.05, 0.10),
                c(0.08, 0.03, 0.05),
                c(0.25, 0.20, 0.20),
                c(1.0, 0.5, 0.0),
            ),
            BileLab => (
                c(0.04, 0.06, 0.18),
                c(0.02, 0.04, 0.10),
                c(0.20, 0.25, 0.45),
                c(0.0, 1.0, 1.0),
            ),
            TibetPeak => (
                c(0.30, 0.06, 0.02),
                c(0.18, 0.04, 0.01),
                c(0.40, 0.15, 0.05),
                c(1.0, 0.2, 0.0),
            ),
            EmberNest => (
                c(0.10, 0.02, 0.15),
                c(0.05, 0.01, 0.08),
                c(0.20, 0.05, 0.25),
                c(0.9, 0.0, 0.6),
            ),
            DragonWildland => (
                c(0.25, 0.20, 0.15),
                c(0.18, 0.14, 0.10),
                c(0.45, 0.40, 0.30),
                c(0.8, 0.6, 0.2),
            ),
            PinkFlameGarden => (
                c(0.40, 0.55, 0.65),
                c(0.30, 0.45, 0.55),
                c(0.30, 0.55, 0.30),
                c(1.0, 0.95, 0.7),
            ),
            RockiesDomain => (
                c(0.45, 0.30, 0.10),
                c(0.25, 0.18, 0.06),
                c(0.50, 0.40, 0.20),
                c(1.0, 0.7, 0.2),
            ),
            AntarcticaOutpost => (
                c(0.50, 0.65, 0.85),
                c(0.40, 0.55, 0.75),
                c(0.30, 0.55, 0.20),
                c(0.9, 0.85, 0.5),
            ),
            PuzzleBattleground => (
                c(0.20, 0.20, 0.20),
                c(0.10, 0.10, 0.10),
                c(0.35, 0.30, 0.25),
                c(1.0, 0.3, 0.1),
            ),
            CrownGate => (
                c(0.55, 0.70, 0.85),
                c(0.75, 0.85, 0.95),
                c(0.85, 0.90, 0.95),
                c(0.7, 0.9, 1.0),
            ),
            StarfallArena => (
                c(0.02, 0.02, 0.06),
                c(0.01, 0.01, 0.04),
                c(0.10, 0.10, 0.20),
                c(1.0, 0.9, 0.0),
            ),
        }
    }
}

#[inline]
fn c(r: f32, g: f32, b: f32) -> Color {
    Color::srgb(r, g, b)
}

// ── Encounter Steps ───────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub enum EncounterStep {
    /// Fire one or more radio-chatter lines, then advance after `hold` seconds.
    Dialogue {
        speaker: &'static str,
        faction: Faction,
        line: &'static str,
        hold: f32,
    },
    /// Spawn a group of enemies of the given type. Step completes when all are dead.
    SpawnGroup {
        faction: Faction,
        enemy_type: EnemyType,
        count: u32,
        scale: f32,
    },
    /// Spawn a named mid-boss (preset name from `robots/presets.rs`).
    MidBoss {
        preset: &'static str,
        name: &'static str,
        faction: Faction,
        scale: f32,
    },
    /// Spawn the chapter's main boss with intro line.
    BossFight {
        preset: &'static str,
        name: &'static str,
        faction: Faction,
        intro_line: &'static str,
        scale: f32,
    },
    /// Story beat after a castle boss drops: their airship appears and they flee.
    AirshipEscape {
        boss_name: &'static str,
        faction: Faction,
        airship_label: &'static str,
        line: &'static str,
        hold: f32,
    },
    /// Move the party onto the boss airship and clear its deck guards.
    AirshipDeckRaid {
        faction: Faction,
        enemy_type: EnemyType,
        count: u32,
        scale: f32,
    },
    /// Place a discoverable beacon at a position offset from the player.
    PlaceDiscoverable {
        kind: DiscoverableKind,
        label: &'static str,
        offset: Vec3,
    },
    /// Place an optional secret-cave discovery beacon at an authored world anchor.
    PlaceSecretCave {
        chapter: u8,
        cave_id: &'static str,
        label: &'static str,
        anchor: &'static str,
    },
    /// Spawn an activation puzzle; the reward beacon appears when all switches are hit in order.
    PlaceRelicPuzzle {
        scientist: &'static str,
        relic_id: &'static str,
        label: &'static str,
        hint: &'static str,
        archetype: PuzzleArchetype,
        reward_anchor: &'static str,
        node_anchors: Vec<&'static str>,
    },
    /// Scatter small relic fragments through a moving obstacle course.
    PlaceRelicFragmentPuzzle {
        scientist: &'static str,
        relic_id: &'static str,
        label: &'static str,
        hint: &'static str,
        total: u8,
        center_offset: Vec3,
    },
    /// Final outro line + completion fire.
    Outro { line: &'static str },
}

// ── Chapter Definition ────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct ChapterDef {
    pub id: ChapterId,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub biome: Biome,
    pub difficulty_scale: f32,
    pub script: Vec<EncounterStep>,
}

// ── Helpers for building scripts ─────────────────────────────────────────────
fn spawn(faction: Faction, t: EnemyType, count: u32, scale: f32) -> EncounterStep {
    EncounterStep::SpawnGroup {
        faction,
        enemy_type: t,
        count,
        scale,
    }
}
fn dialogue(
    speaker: &'static str,
    faction: Faction,
    line: &'static str,
    hold: f32,
) -> EncounterStep {
    EncounterStep::Dialogue {
        speaker,
        faction,
        line,
        hold,
    }
}
fn mid_boss(
    preset: &'static str,
    name: &'static str,
    faction: Faction,
    scale: f32,
) -> EncounterStep {
    EncounterStep::MidBoss {
        preset,
        name,
        faction,
        scale,
    }
}
fn boss(
    preset: &'static str,
    name: &'static str,
    faction: Faction,
    intro_line: &'static str,
    scale: f32,
) -> EncounterStep {
    EncounterStep::BossFight {
        preset,
        name,
        faction,
        intro_line,
        scale,
    }
}
fn airship_escape(
    boss_name: &'static str,
    faction: Faction,
    airship_label: &'static str,
    line: &'static str,
) -> EncounterStep {
    EncounterStep::AirshipEscape {
        boss_name,
        faction,
        airship_label,
        line,
        hold: 4.0,
    }
}
fn airship_raid(faction: Faction, enemy_type: EnemyType, count: u32, scale: f32) -> EncounterStep {
    EncounterStep::AirshipDeckRaid {
        faction,
        enemy_type,
        count,
        scale,
    }
}
fn place(kind: DiscoverableKind, label: &'static str, offset: Vec3) -> EncounterStep {
    EncounterStep::PlaceDiscoverable {
        kind,
        label,
        offset,
    }
}
fn robot_pet(
    pet_id: &'static str,
    name: &'static str,
    role: RobotPetRole,
    label: &'static str,
    offset: Vec3,
) -> EncounterStep {
    place(
        DiscoverableKind::RobotPetRescue { pet_id, name, role },
        label,
        offset,
    )
}
fn tech_cache(
    cache_id: &'static str,
    label: &'static str,
    parts: &[(RobotPartKind, u32)],
    upgrade_hint: Option<TechUpgradeId>,
    rejuvenation_charge: u32,
    offset: Vec3,
) -> EncounterStep {
    place(
        DiscoverableKind::TechCache {
            cache_id,
            parts: parts.to_vec(),
            upgrade_hint,
            rejuvenation_charge,
        },
        label,
        offset,
    )
}
fn secret_cave(chapter: u8, cave_id: &'static str, label: &'static str) -> EncounterStep {
    EncounterStep::PlaceSecretCave {
        chapter,
        cave_id,
        label,
        anchor: cave_id,
    }
}
fn relic_puzzle(
    scientist: &'static str,
    relic_id: &'static str,
    label: &'static str,
    hint: &'static str,
    archetype: PuzzleArchetype,
    reward_anchor: &'static str,
    node_anchors: &[&'static str],
) -> EncounterStep {
    EncounterStep::PlaceRelicPuzzle {
        scientist,
        relic_id,
        label,
        hint,
        archetype,
        reward_anchor,
        node_anchors: node_anchors.to_vec(),
    }
}
fn relic_fragment_puzzle(
    scientist: &'static str,
    relic_id: &'static str,
    label: &'static str,
    hint: &'static str,
    center_offset: Vec3,
) -> EncounterStep {
    EncounterStep::PlaceRelicFragmentPuzzle {
        scientist,
        relic_id,
        label,
        hint,
        total: 5,
        center_offset,
    }
}
fn outro(line: &'static str) -> EncounterStep {
    EncounterStep::Outro { line }
}

// ── All 14 Chapters ───────────────────────────────────────────────────────────
fn build_chapters() -> Vec<ChapterDef> {
    use Faction::*;
    // Note: deliberately NOT `use EnemyType::*;` so script calls stay explicit
    // where the specialized spike alien appears.
    use EnemyType::{Drone, Heavy, Hybrid, Soldier};

    vec![
        ChapterDef {
            id: ChapterId(1),
            title: "Invasion of the Scallarians",
            subtitle: "The Starfall Lab opens under attack",
            biome: Biome::StarfallLab,
            difficulty_scale: 1.0,
            script: vec![
                dialogue("Giacoma", WizardScientist, "The star engine is awake. Nobody sneeze near the dimension dial.", 3.5),
                secret_cave(1, "secret_cave_ch01", "Star Engine Grotto"),
                dialogue("Giovanni", WizardScientist, "Too late. A Scallarian scout just waved back.", 3.0),
                spawn(DimensionalAlien, Drone, 5, 1.0),
                dialogue("Gabrio", WizardScientist, "Give the brothers the beam gloves. Cartoon settings only.", 3.0),
                place(DiscoverableKind::BeamSabreUnlock, "Star Sabre Core", Vec3::new(8.0, 0.5, 0.0)),
                robot_pet(
                    "spark_pup",
                    "Spark Pup",
                    RobotPetRole::Scout,
                    "Spark Pup Rescue Pod",
                    Vec3::new(-7.0, 0.5, 5.0),
                ),
                tech_cache(
                    "tech_cache_ch01_beam",
                    "Starter Beam Cache",
                    &[
                        (RobotPartKind::CircuitBoard, 3),
                        (RobotPartKind::EnergyCore, 1),
                        (RobotPartKind::ServoMotor, 1),
                    ],
                    Some(TechUpgradeId::BeamCapacitors),
                    45,
                    Vec3::new(10.0, 0.5, 4.0),
                ),
                relic_puzzle(
                    "Giacoma",
                    "star_engine_focus",
                    "Giacoma's Star Engine Focus",
                    "Tune the star engine pylons from left to right to restore Giacoma's stolen focus lens.",
                    PuzzleArchetype::OrderedSwitches,
                    "ch01_giacoma_reward",
                    &[
                        "ch01_giacoma_node_1",
                        "ch01_giacoma_node_2",
                        "ch01_giacoma_node_3",
                    ],
                ),
                spawn(DimensionalAlien, EnemyType::SpikeAlien, 5, 1.0),
                mid_boss("JetWarden", "Scallarian Rift Skipper", DimensionalAlien, 1.25),
                place(DiscoverableKind::CompanionRecruit("Vincenzo"), "Vincenzo - Oldest Brother", Vec3::new(0.0, 0.5, -8.0)),
                outro("Vincenzo: Family first. Then we close the sky."),
            ],
        },

        ChapterDef {
            id: ChapterId(2),
            title: "Tony's Shortcut",
            subtitle: "Wall jumps across the rift city",
            biome: Biome::RiftCity,
            difficulty_scale: 1.2,
            script: vec![
                dialogue("Antonio", HeroBrother, "Call me Tony. Also, I found a wall we can bounce off.", 3.0),
                secret_cave(2, "secret_cave_ch02", "Rift-Glass Underpass"),
                spawn(DimensionalAlien, Drone, 6, 1.2),
                spawn(DimensionalAlien, Soldier, 5, 1.2),
                place(DiscoverableKind::WeaponMod("homing_star"), "Homing Star Focus", Vec3::new(6.0, 0.5, 6.0)),
                tech_cache(
                    "tech_cache_ch02_missile",
                    "Rift Missile Cache",
                    &[
                        (RobotPartKind::ScrapFrame, 6),
                        (RobotPartKind::EnergyCore, 2),
                        (RobotPartKind::HoverJet, 1),
                    ],
                    Some(TechUpgradeId::NovaMissileForge),
                    35,
                    Vec3::new(-8.0, 0.5, 7.0),
                ),
                mid_boss("Nero", "Scallarian Rift Cartographer", DimensionalAlien, 1.4),
                place(DiscoverableKind::CompanionRecruit("Antonio"), "Antonio aka Tony", Vec3::new(0.0, 0.5, -6.0)),
                place(DiscoverableKind::Blueprint("jump_gate_blueprint"), "Jump Gate Sketches", Vec3::new(-8.0, 0.5, 4.0)),
                relic_fragment_puzzle(
                    "Giovanni",
                    "rift_caliper",
                    "Giovanni's Rift Caliper",
                    "Find five caliper shards while the rift blocks slide and rotate around Tony's shortcut.",
                    Vec3::new(18.0, 1.0, -18.0),
                ),
                outro("Tony: See? Falling is just jumping with bad timing."),
            ],
        },

        ChapterDef {
            id: ChapterId(3),
            title: "Sisters Of The Star",
            subtitle: "Gabriella, Nova, Aurora, Fortuna",
            biome: Biome::SisterSanctum,
            difficulty_scale: 1.3,
            script: vec![
                dialogue("Gabriella", HeroSister, "Nova, paint the sky. Aurora, guard the ledge. Fortuna, keep score.", 3.5),
                secret_cave(3, "secret_cave_ch03", "Sister Starwell Cave"),
                spawn(DimensionalAlien, Soldier, 5, 1.3),
                dialogue("Nova", HeroSister, "Rainbow Ray is warmed up.", 2.5),
                spawn(DimensionalAlien, EnemyType::SpikeAlien, 6, 1.3),
                mid_boss("HybridOmega", "Scallarian Dimension Bruiser", DimensionalAlien, 1.45),
                place(DiscoverableKind::CompanionRecruit("Gabriella"), "Gabriella", Vec3::new(0.0, 0.5, -6.0)),
                place(DiscoverableKind::CompanionRecruit("Nova"), "Nova", Vec3::new(4.0, 0.5, -6.0)),
                place(DiscoverableKind::CompanionRecruit("Aurora"), "Aurora", Vec3::new(8.0, 0.5, -6.0)),
                place(DiscoverableKind::CompanionRecruit("Fortuna"), "Fortuna", Vec3::new(12.0, 0.5, -6.0)),
                outro("Fortuna: Four sisters, four brothers, one very rude portal."),
            ],
        },

        ChapterDef {
            id: ChapterId(4),
            title: "Four Brothers",
            subtitle: "Angelo and Little Joe join the fight",
            biome: Biome::BrotherTrialGrounds,
            difficulty_scale: 1.4,
            script: vec![
                dialogue("Angelo", HeroBrother, "If we hit the switches together, the bridge sings.", 3.0),
                secret_cave(4, "secret_cave_ch04", "Brother Trial Burrow"),
                spawn(DimensionalAlien, Soldier, 6, 1.4),
                spawn(DimensionalAlien, Heavy, 3, 1.4),
                mid_boss("Brutus", "Portal Bouncer", DimensionalAlien, 1.45),
                place(DiscoverableKind::WeaponMod("piercing_rounds"), "Star Pierce Mod", Vec3::new(8.0, 0.5, 0.0)),
                robot_pet(
                    "bolt_mason",
                    "Bolt Mason",
                    RobotPetRole::Builder,
                    "Bolt Mason Rescue Pod",
                    Vec3::new(-7.0, 0.5, 7.0),
                ),
                tech_cache(
                    "tech_cache_ch04_turret",
                    "Sprite Turret Cache",
                    &[
                        (RobotPartKind::CircuitBoard, 4),
                        (RobotPartKind::ServoMotor, 5),
                        (RobotPartKind::ScrapFrame, 4),
                    ],
                    Some(TechUpgradeId::SpriteTurretLattice),
                    55,
                    Vec3::new(12.0, 0.5, -4.0),
                ),
                boss("HybridOmega", "Scallarian Rift Gatekeeper", DimensionalAlien,
                     "GATEKEEPER: Your family makes too much light.", 1.8),
                relic_puzzle(
                    "Giacoma",
                    "harmonic_seal",
                    "Giacoma's Harmonic Seal",
                    "Hold the trial floor plates together long enough to unlock the harmonic seal.",
                    PuzzleArchetype::CoOpFloorPlates {
                        hold_secs: 2.4,
                        required_players: 2,
                    },
                    "ch04_giacoma_reward",
                    &[
                        "ch04_giacoma_plate_1",
                        "ch04_giacoma_plate_2",
                        "ch04_giacoma_plate_3",
                    ],
                ),
                place(DiscoverableKind::CompanionRecruit("Angelo"), "Angelo", Vec3::new(0.0, 0.5, -6.0)),
                place(DiscoverableKind::CompanionRecruit("Joseph"), "Joseph aka Little Joe", Vec3::new(4.0, 0.5, -6.0)),
                outro("Little Joe: I am small. My star beam is not."),
            ],
        },

        ChapterDef {
            id: ChapterId(5),
            title: "Dr. Bile",
            subtitle: "The mirror humans appear",
            biome: Biome::BileLab,
            difficulty_scale: 1.5,
            script: vec![
                dialogue("Dr. Bile", CorruptedHuman, "Children with stars. How quaint. I built four shadows.", 3.5),
                secret_cave(5, "secret_cave_ch05", "Mirror Sludge Cavern"),
                spawn(CorruptedHuman, Soldier, 5, 1.5),
                mid_boss("ScoutPrime", "Zark", CorruptedHuman, 1.45),
                mid_boss("TankTitan", "Crush", CorruptedHuman, 1.5),
                place(DiscoverableKind::ArmorMod("reactive_plating"), "Star Guard Weave", Vec3::new(0.0, 0.5, 8.0)),
                relic_fragment_puzzle(
                    "Gabrio",
                    "mirror_resonator",
                    "Gabrio's Mirror Resonator",
                    "Collect the five resonator teeth hidden between Bile's moving mirror presses.",
                    Vec3::new(-18.0, 1.0, 18.0),
                ),
                boss("HybridOmega", "Dr. Bile's Reactor Suit", CorruptedHuman,
                     "BILE: Same powers, sharper teeth.", 2.0),
                outro("Giacoma: Bile copied the gifts. He did not copy the heart."),
            ],
        },

        ChapterDef {
            id: ChapterId(6),
            title: "Tibet Peak",
            subtitle: "Collosar, king of the dragons",
            biome: Biome::TibetPeak,
            difficulty_scale: 1.6,
            script: vec![
                dialogue("Collosar", DragonRoyalty, "Tiny star children. Tibet is under my wing.", 3.0),
                secret_cave(6, "secret_cave_ch06", "Crownroot Ice Cave"),
                spawn(DragonRoyalty, Drone, 6, 1.6),
                spawn(DragonRoyalty, Heavy, 4, 1.6),
                place(DiscoverableKind::ArmorMod("coolant_weave"), "Dragon Chill Weave", Vec3::new(6.0, 0.5, 6.0)),
                tech_cache(
                    "tech_cache_ch06_chill_armor",
                    "Crownship Armor Cache",
                    &[
                        (RobotPartKind::HullPlate, 5),
                        (RobotPartKind::PressureSeal, 3),
                        (RobotPartKind::EnergyCore, 2),
                    ],
                    Some(TechUpgradeId::ArmorPlating),
                    65,
                    Vec3::new(-10.0, 0.5, 8.0),
                ),
                boss("BruteForge", "Collosar - King of the Dragons", DragonRoyalty,
                     "COLLOSAR: Prove your light protects Earth.", 2.4),
                airship_escape(
                    "Collosar",
                    DragonRoyalty,
                    "Collosar's Crown Airship",
                    "COLLOSAR: The castle falls. My crownship does not.",
                ),
                airship_raid(DragonRoyalty, Drone, 7, 1.7),
                boss("BruteForge", "Collosar - Crown Airship", DragonRoyalty,
                     "COLLOSAR: Now fight where the sky can judge us.", 2.1),
                outro("Vincenzo: The king is not our enemy. The invasion is."),
            ],
        },

        ChapterDef {
            id: ChapterId(7),
            title: "Tarack's Ember",
            subtitle: "The queen tests the family",
            biome: Biome::EmberNest,
            difficulty_scale: 1.8,
            script: vec![
                dialogue("Tarack", DragonRoyalty, "A family that fights together must also think together.", 3.0),
                secret_cave(7, "secret_cave_ch07", "Ember Breathing Hollow"),
                spawn(DragonRoyalty, Drone, 8, 1.8),
                spawn(DimensionalAlien, Soldier, 6, 1.8),
                mid_boss("WolfAnimaton", "Ember Warden", DragonRoyalty, 1.65),
                place(DiscoverableKind::WeaponMod("piercing_rounds"), "Royal Star Pierce", Vec3::new(0.0, 0.5, 10.0)),
                boss("HarvesterMech", "Tarack - Dragon Queen", DragonRoyalty,
                     "TARACK: Carry your light without burning each other.", 2.0),
                airship_escape(
                    "Tarack",
                    DragonRoyalty,
                    "Tarack's Ember Airship",
                    "TARACK: Good. Now climb aboard and prove you can keep balance.",
                ),
                airship_raid(DragonRoyalty, Drone, 8, 1.8),
                boss("HarvesterMech", "Tarack - Ember Airship", DragonRoyalty,
                     "TARACK: A true family fights even when the floor is moving.", 2.0),
                outro("Aurora: A test disguised as a battle. I like her."),
            ],
        },

        ChapterDef {
            id: ChapterId(8),
            title: "Spikes And Shreds",
            subtitle: "The dragon sons run wild",
            biome: Biome::DragonWildland,
            difficulty_scale: 1.9,
            script: vec![
                dialogue("Spikey", DragonRoyalty, "I am the youngest and the fastest and nobody can prove otherwise.", 3.0),
                secret_cave(8, "secret_cave_ch08", "Fangroot Scrap Tunnel"),
                spawn(DragonRoyalty, Drone, 8, 1.8),
                mid_boss("Nero", "Spikey - Youngest Son", DragonRoyalty, 1.7),
                dialogue("Shread", DragonRoyalty, "Little brother, stop chasing portals with your face.", 2.5),
                spawn(DimensionalAlien, EnemyType::SpikeAlien, 7, 1.9),
                robot_pet(
                    "rivet_guard",
                    "Rivet Guard",
                    RobotPetRole::Defender,
                    "Rivet Guard Rescue Pod",
                    Vec3::new(-8.0, 0.5, 8.0),
                ),
                tech_cache(
                    "tech_cache_ch08_mech_link",
                    "Scrapwing Mech-Link Cache",
                    &[
                        (RobotPartKind::ServoMotor, 6),
                        (RobotPartKind::StarDrive, 2),
                        (RobotPartKind::CommandDeck, 1),
                    ],
                    Some(TechUpgradeId::MechCommandLink),
                    80,
                    Vec3::new(9.0, 0.5, -9.0),
                ),
                boss("TankTitan", "Shread - Oldest Son", DragonRoyalty,
                     "SHREAD: Show me your best team combo.", 2.2),
                airship_escape(
                    "Shread",
                    DragonRoyalty,
                    "Shread's Scrapwing Airship",
                    "SHREAD: Castle rules are boring. Chase me onto the Scrapwing.",
                ),
                airship_raid(DragonRoyalty, EnemyType::SpikeAlien, 8, 1.9),
                boss("TankTitan", "Shread - Scrapwing Airship", DragonRoyalty,
                     "SHREAD: No walls, no excuses. Show me the combo again.", 2.15),
                outro("Tony: Dragon brothers are just us with wings and worse furniture."),
            ],
        },

        ChapterDef {
            id: ChapterId(9),
            title: "Pink Flame",
            subtitle: "A garden of puzzles and fire",
            biome: Biome::PinkFlameGarden,
            difficulty_scale: 1.5,
            script: vec![
                dialogue("Pink Flame", DragonRoyalty, "My brothers broke the garden. Please un-break it with star beams.", 3.5),
                secret_cave(9, "secret_cave_ch09", "Pink Flame Root Cave"),
                spawn(DimensionalAlien, Drone, 6, 1.4),
                spawn(DimensionalAlien, Soldier, 4, 1.4),
                relic_puzzle(
                    "Giovanni",
                    "garden_prism",
                    "Giovanni's Garden Prism",
                    "Light the crystal beds in sequence to reveal Giovanni's stolen prism.",
                    PuzzleArchetype::TimedCrystalChain { window_secs: 8.0 },
                    "ch09_giovanni_reward",
                    &[
                        "ch09_giovanni_node_1",
                        "ch09_giovanni_node_2",
                        "ch09_giovanni_node_3",
                        "ch09_giovanni_node_4",
                    ],
                ),
                place(DiscoverableKind::CompanionRecruit("Pink Flame"), "Pink Flame - Dragon Daughter", Vec3::new(0.0, 0.5, -4.0)),
                boss("WolfAnimaton", "Garden Rift Bloom", DimensionalAlien,
                     "BLOOM: Pollinating dimension three.", 1.6),
                outro("Gabriella: Pink Flame stays. The garden gets walls we can jump off."),
            ],
        },

        ChapterDef {
            id: ChapterId(10),
            title: "Rockies Domain",
            subtitle: "Ragar, uncle to the king",
            biome: Biome::RockiesDomain,
            difficulty_scale: 1.9,
            script: vec![
                dialogue("Ragar", DragonExile, "Colorado rock remembers every claw mark.", 3.0),
                secret_cave(10, "secret_cave_ch10", "Granite Echo Cave"),
                spawn(DragonExile, Drone, 8, 1.9),
                spawn(CorruptedHuman, Soldier, 5, 1.8),
                mid_boss("CharredCaptain", "Fang", CorruptedHuman, 1.7),
                relic_fragment_puzzle(
                    "Giovanni",
                    "granite_sextant",
                    "Giovanni's Granite Sextant",
                    "Gather five sextant stones from Ragar's shifting rock machinery.",
                    Vec3::new(22.0, 1.0, 12.0),
                ),
                boss("BruteForge", "Ragar - Rockies Domain", DragonExile,
                     "RAGAR: Collosar's crown made him soft.", 2.0),
                airship_escape(
                    "Ragar",
                    DragonExile,
                    "Ragar's Granite Airship",
                    "RAGAR: My castle is only the first stone. The sky fortress is next.",
                ),
                airship_raid(DragonExile, Drone, 8, 2.0),
                boss("BruteForge", "Ragar - Granite Airship", DragonExile,
                     "RAGAR: Up here, every fall teaches respect.", 2.1),
                outro("Angelo: Ragar is angry. The aliens are using it."),
            ],
        },

        ChapterDef {
            id: ChapterId(11),
            title: "Blackskull Ice",
            subtitle: "Antarctica opens below",
            biome: Biome::AntarcticaOutpost,
            difficulty_scale: 2.0,
            script: vec![
                dialogue("Blackskull", DragonExile, "Antarctica is quiet because I demand quiet.", 3.0),
                secret_cave(11, "secret_cave_ch11", "Icebreaker Under-Cave"),
                spawn(DragonExile, Soldier, 8, 2.0),
                mid_boss("ScoutPrime", "Sharp", CorruptedHuman, 1.9),
                spawn(DimensionalAlien, Heavy, 4, 2.0),
                robot_pet(
                    "tide_mender",
                    "Tide Mender",
                    RobotPetRole::Healer,
                    "Tide Mender Rescue Pod",
                    Vec3::new(-9.0, 0.5, 6.0),
                ),
                tech_cache(
                    "tech_cache_ch11_rejuvenation",
                    "Icebreaker Rejuvenation Cache",
                    &[
                        (RobotPartKind::EnergyCore, 5),
                        (RobotPartKind::CircuitBoard, 4),
                        (RobotPartKind::PressureSeal, 4),
                    ],
                    Some(TechUpgradeId::RejuvenationMatrix),
                    120,
                    Vec3::new(8.0, 0.5, -8.0),
                ),
                boss("HybridOmega", "Blackskull - Antarctica Domain", DragonExile,
                     "BLACKSKULL: Your bright little family cracks the ice.", 2.0),
                airship_escape(
                    "Blackskull",
                    DragonExile,
                    "Blackskull's Icebreaker Airship",
                    "BLACKSKULL: The ice is cracked. The Icebreaker still hunts.",
                ),
                airship_raid(DragonExile, Soldier, 8, 2.05),
                boss("HybridOmega", "Blackskull - Icebreaker Airship", DragonExile,
                     "BLACKSKULL: Let the high cold finish what the castle began.", 2.15),
                outro("Little Joe: Quiet is overrated."),
            ],
        },

        ChapterDef {
            id: ChapterId(12),
            title: "Mana Switchworks",
            subtitle: "Open-world puzzle battle",
            biome: Biome::PuzzleBattleground,
            difficulty_scale: 2.2,
            script: vec![
                dialogue("Giovanni", WizardScientist, "Classic platforming rule: the suspicious crystal is absolutely a switch.", 3.0),
                secret_cave(12, "secret_cave_ch12", "Mana Gear Grotto"),
                spawn(DimensionalAlien, Heavy, 6, 2.2),
                spawn(CorruptedHuman, Hybrid, 2, 2.2),
                relic_puzzle(
                    "Gabrio",
                    "mana_compass",
                    "Gabrio's Mana Compass",
                    "Charge every mana switch in order before the maze resets and Bile keeps the compass.",
                    PuzzleArchetype::BeamRouting,
                    "ch12_gabrio_reward",
                    &[
                        "ch12_gabrio_node_1",
                        "ch12_gabrio_node_2",
                        "ch12_gabrio_node_3",
                        "ch12_gabrio_node_4",
                    ],
                ),
                mid_boss("TankTitan", "Crush - Mirror Brother", CorruptedHuman, 2.0),
                boss("HybridOmega", "Dr. Bile's Puzzle Engine", CorruptedHuman,
                     "BILE: Four players, one maze, zero patience.", 2.5),
                outro("Gabrio: The engine is solved. Mostly. It is only smoking politely."),
            ],
        },

        ChapterDef {
            id: ChapterId(13),
            title: "Scallarian Front",
            subtitle: "The invaders reveal the crown gate",
            biome: Biome::CrownGate,
            difficulty_scale: 1.8,
            script: vec![
                dialogue("Giacoma", WizardScientist, "The crown gate is not a door. It is a mouth.", 3.5),
                secret_cave(13, "secret_cave_ch13", "Crown Gate Underpath"),
                dialogue("Fortuna", HeroSister, "Then we make it bite a star.", 2.5),
                spawn(DimensionalAlien, Drone, 8, 1.8),
                mid_boss("Selene", "Scallarian Rift Herald", DimensionalAlien, 1.8),
                boss("HybridOmega", "Scallarian Crown Gate Avatar", DimensionalAlien,
                     "AVATAR: Earth belongs to the between-place.", 1.9),
                outro("Nova: Tomorrow we paint over the between-place."),
            ],
        },

        ChapterDef {
            id: ChapterId(14),
            title: "Starfall",
            subtitle: "The family closes the sky",
            biome: Biome::StarfallArena,
            difficulty_scale: 2.5,
            script: vec![
                dialogue("Vincenzo", HeroBrother, "Brothers left. Sisters right. Scientists, please make the impossible thing possible.", 3.0),
                secret_cave(14, "secret_cave_ch14", "Starfall Core Hollow"),
                spawn(DimensionalAlien, Hybrid, 4, 2.5),
                mid_boss("Cygnus", "Zark - Final Mirror", CorruptedHuman, 2.2),
                mid_boss("Cygni", "Dr. Bile - Star Thief", CorruptedHuman, 2.2),
                dialogue("Collosar", DragonRoyalty, "Dragons hold the earth. Children hold the stars.", 3.0),
                boss("HybridOmega", "Starfall Rift Core", DimensionalAlien,
                     "CORE: Every dimension falls inward.", 3.0),
                outro("Giacoma: The sky is closed. Now someone please label the buttons."),
            ],
        },
    ]
}

pub fn chapter_catalog() -> &'static [ChapterDef] {
    static CHAPTERS: OnceLock<Vec<ChapterDef>> = OnceLock::new();
    CHAPTERS.get_or_init(build_chapters).as_slice()
}

pub fn all_chapters() -> Vec<ChapterDef> {
    chapter_catalog().to_vec()
}

pub fn get_chapter(id: ChapterId) -> Option<ChapterDef> {
    chapter_catalog()
        .iter()
        .find(|chapter| chapter.id == id)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_map_locations_cover_every_chapter() {
        let ids: Vec<u8> = chapter_map_locations()
            .iter()
            .map(|location| location.id.0)
            .collect();
        let expected: Vec<u8> = (ChapterId::FIRST.0..=ChapterId::LAST.0).collect();

        assert_eq!(ids, expected);
        for chapter in chapter_catalog() {
            assert!(chapter_map_location(chapter.id).is_some());
        }
    }

    #[test]
    fn chapter_map_locations_stay_inside_heightmap_world() {
        for location in chapter_map_locations() {
            assert!(location.x.abs() <= EVEREST_RANGE_HALF_EXTENT - 120.0);
            assert!(location.z.abs() <= EVEREST_RANGE_HALF_EXTENT - 120.0);
        }
    }

    #[test]
    fn everest_range_scale_matches_design_target() {
        assert_eq!(EVEREST_RANGE_MILES, 200.0);
        assert_eq!(EVEREST_RANGE_WORLD_SIZE, 20_000.0);
    }

    #[test]
    fn map_settlements_stay_inside_heightmap_world() {
        for settlement in map_settlements() {
            assert!(settlement.x.abs() <= EVEREST_RANGE_HALF_EXTENT - 120.0);
            assert!(settlement.z.abs() <= EVEREST_RANGE_HALF_EXTENT - 120.0);
        }
    }

    #[test]
    fn map_settlement_rewards_and_anchors_are_unique() {
        let mut anchors: Vec<&str> = map_settlements()
            .iter()
            .map(|settlement| settlement.anchor_id)
            .collect();
        anchors.sort_unstable();
        anchors.dedup();

        let mut rewards: Vec<&str> = map_settlements()
            .iter()
            .map(|settlement| settlement.reward_id)
            .collect();
        rewards.sort_unstable();
        rewards.dedup();

        assert_eq!(anchors.len(), map_settlements().len());
        assert_eq!(rewards.len(), map_settlements().len());
    }

    #[test]
    fn every_chapter_has_one_unique_mountain_cave_destination() {
        let chapters: Vec<u8> = SECRET_CAVE_LOCATIONS
            .iter()
            .map(|cave| cave.chapter.0)
            .collect();
        assert_eq!(chapters, (1..=14).collect::<Vec<_>>());

        let mut anchors: Vec<_> = SECRET_CAVE_LOCATIONS
            .iter()
            .map(|cave| cave.anchor_id)
            .collect();
        anchors.sort_unstable();
        anchors.dedup();
        assert_eq!(anchors.len(), SECRET_CAVE_LOCATIONS.len());
        for cave in SECRET_CAVE_LOCATIONS {
            assert!(cave.x.abs() <= EVEREST_RANGE_HALF_EXTENT);
            assert!(cave.z.abs() <= EVEREST_RANGE_HALF_EXTENT);
            assert!(cave.length >= 36.0);
        }
    }
}
