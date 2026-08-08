//! Deterministic Heavy Water world-domain foundations.
//!
//! This module ports the executable game's authored region catalog, atmosphere
//! cues, map-marker behavior, destructible/mining-prop rules, and the geometry
//! facts available for its local versus arena. It deliberately contains no
//! Bevy systems or rendering handles: world, UI, save, and combat adapters can
//! consume the same serializable authority without making frame order part of
//! the rules.

#![allow(dead_code)] // Public port domain; runtime/UI adapters land incrementally.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::TAU;

pub const HEAVY_REGIONS_SCHEMA_VERSION: u16 = 1;
pub const HEAVY_WORLD_LEVEL_COUNT: usize = 11;
pub const HEAVY_SECONDS_PER_DAY: f32 = 300.0;
pub const HEAVY_MAP_WORLD_SIZE: f32 = 1_200.0;
pub const HEAVY_MAP_CANVAS_SIZE: f32 = 250.0;
pub const HEAVY_MAP_ICON_FALLOFF_NEAR: f32 = 80.0;
pub const HEAVY_MAP_ICON_FALLOFF_FAR: f32 = 320.0;
pub const MAX_TRACKED_PLAYERS: u8 = 24;
pub const MAX_ENVIRONMENT_PROPS: usize = 220;
pub const VERSUS_ARENA_HALF_EXTENT: f32 = 320.0;
pub const VERSUS_ARENA_WALL_HEIGHT: f32 = 96.0;
pub const VERSUS_ARENA_WALL_THICKNESS: f32 = 4.0;
pub const VERSUS_ARENA_SPAWN_COUNT: u8 = 24;
pub const VERSUS_ARENA_SPAWN_RADIUS: f32 = 42.0;

/// Save-friendly counterpart to a renderer or physics engine's 3D vector.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldPoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl WorldPoint {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn horizontal_distance_squared(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dz = self.z - other.z;
        dx * dx + dz * dz
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyRegionId {
    DetroitStarCityFront,
    DetroitHoldTheLine,
    DetroitPurgeTheVoid,
    AshurSanctuary,
    OrbitalFront,
    PontiacSecretLab,
    SwarmsLair,
    SaginawUnderwaterLab,
    ZugIslandLegion,
    AnnArborApocalypse,
    MichiganWilds,
}

impl LegacyRegionId {
    pub const ALL: [Self; HEAVY_WORLD_LEVEL_COUNT] = [
        Self::DetroitStarCityFront,
        Self::DetroitHoldTheLine,
        Self::DetroitPurgeTheVoid,
        Self::AshurSanctuary,
        Self::OrbitalFront,
        Self::PontiacSecretLab,
        Self::SwarmsLair,
        Self::SaginawUnderwaterLab,
        Self::ZugIslandLegion,
        Self::AnnArborApocalypse,
        Self::MichiganWilds,
    ];

    pub const fn legacy_level(self) -> u8 {
        match self {
            Self::DetroitStarCityFront => 1,
            Self::DetroitHoldTheLine => 2,
            Self::DetroitPurgeTheVoid => 3,
            Self::AshurSanctuary => 4,
            Self::OrbitalFront => 5,
            Self::PontiacSecretLab => 6,
            Self::SwarmsLair => 7,
            Self::SaginawUnderwaterLab => 8,
            Self::ZugIslandLegion => 9,
            Self::AnnArborApocalypse => 10,
            Self::MichiganWilds => 11,
        }
    }

    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::DetroitStarCityFront => "detroit_star_city_front",
            Self::DetroitHoldTheLine => "detroit_hold_the_line",
            Self::DetroitPurgeTheVoid => "detroit_purge_the_void",
            Self::AshurSanctuary => "ashur_sanctuary",
            Self::OrbitalFront => "orbital_front",
            Self::PontiacSecretLab => "pontiac_secret_lab",
            Self::SwarmsLair => "swarms_lair",
            Self::SaginawUnderwaterLab => "saginaw_underwater_lab",
            Self::ZugIslandLegion => "zug_island_legion",
            Self::AnnArborApocalypse => "ann_arbor_apocalypse",
            Self::MichiganWilds => "michigan_wilds",
        }
    }

    pub fn from_legacy_level(level: u8) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|region| region.legacy_level() == level)
    }

    /// Mirrors `LevelSystem.applyLoadedState`: values above the authored
    /// catalog clamp to level 11 and values below level 1 clamp to level 1.
    pub fn from_legacy_level_clamped(level: i32) -> Self {
        let level = level.clamp(1, HEAVY_WORLD_LEVEL_COUNT as i32) as u8;
        Self::from_legacy_level(level).expect("the clamped legacy level is always authored")
    }

    pub const fn is_detroit_campaign(self) -> bool {
        matches!(
            self,
            Self::DetroitStarCityFront | Self::DetroitHoldTheLine | Self::DetroitPurgeTheVoid
        )
    }

    pub const fn campaign_successor(self) -> Option<Self> {
        match self {
            Self::DetroitStarCityFront => Some(Self::DetroitHoldTheLine),
            Self::DetroitHoldTheLine => Some(Self::DetroitPurgeTheVoid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    DetroitCampaign,
    Sanctuary,
    OrbitalCombat,
    InteriorExploration,
    InteriorCombat,
    OpenCombat,
    HeightmapFrontier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionDirector {
    CampaignFortress,
    AshurSanctuary,
    OrbitalFleet,
    PontiacLab,
    SwarmsLair,
    SaginawLab,
    ZugLegion,
    AnnArborSwarm,
    MichiganWilds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weather {
    Clear,
    Overcast,
    Storm,
}

impl Weather {
    pub const fn overcast_factor(self) -> f32 {
        match self {
            Self::Clear => 0.0,
            Self::Overcast => 0.7,
            Self::Storm => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyMode {
    Ground,
    DeepSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyPhase {
    Night,
    Dawn,
    Morning,
    Day,
    Dusk,
    Evening,
    DeepSpace,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CityTheme {
    pub tint: Rgb,
    pub glow_tint: Rgb,
    pub ground: Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyPalette {
    pub zenith: Rgb,
    pub horizon: Rgb,
    pub sun_disc: Rgb,
    pub sun_light: Rgb,
    pub ambient: Rgb,
    pub ambient_ground: Rgb,
    pub fog: Rgb,
    pub sun_intensity: f32,
    pub ambient_intensity: f32,
}

pub const MIDNIGHT_SKY_PALETTE: SkyPalette = SkyPalette {
    zenith: Rgb::new(0.01, 0.01, 0.05),
    horizon: Rgb::new(0.05, 0.05, 0.18),
    sun_disc: Rgb::new(0.6, 0.7, 1.0),
    sun_light: Rgb::new(0.25, 0.3, 0.6),
    ambient: Rgb::new(0.25, 0.3, 0.55),
    ambient_ground: Rgb::new(0.1, 0.05, 0.2),
    fog: Rgb::new(0.04, 0.04, 0.12),
    sun_intensity: 0.3,
    ambient_intensity: 0.35,
};

pub const DAWN_SKY_PALETTE: SkyPalette = SkyPalette {
    zenith: Rgb::new(0.15, 0.2, 0.45),
    horizon: Rgb::new(1.0, 0.55, 0.35),
    sun_disc: Rgb::new(1.0, 0.7, 0.4),
    sun_light: Rgb::new(1.0, 0.65, 0.45),
    ambient: Rgb::new(0.85, 0.65, 0.7),
    ambient_ground: Rgb::new(0.4, 0.2, 0.25),
    fog: Rgb::new(0.55, 0.4, 0.4),
    sun_intensity: 0.85,
    ambient_intensity: 0.5,
};

pub const DAY_SKY_PALETTE: SkyPalette = SkyPalette {
    zenith: Rgb::new(0.25, 0.55, 0.95),
    horizon: Rgb::new(0.7, 0.85, 1.0),
    sun_disc: Rgb::new(1.0, 0.95, 0.85),
    sun_light: Rgb::new(1.0, 0.95, 0.85),
    ambient: Rgb::new(0.7, 0.85, 1.0),
    ambient_ground: Rgb::new(0.35, 0.3, 0.4),
    fog: Rgb::new(0.55, 0.7, 0.9),
    sun_intensity: 1.4,
    ambient_intensity: 0.55,
};

pub const DUSK_SKY_PALETTE: SkyPalette = SkyPalette {
    zenith: Rgb::new(0.25, 0.15, 0.4),
    horizon: Rgb::new(1.0, 0.4, 0.55),
    sun_disc: Rgb::new(1.0, 0.55, 0.5),
    sun_light: Rgb::new(1.0, 0.55, 0.55),
    ambient: Rgb::new(0.8, 0.55, 0.7),
    ambient_ground: Rgb::new(0.35, 0.15, 0.3),
    fog: Rgb::new(0.5, 0.3, 0.45),
    sun_intensity: 0.75,
    ambient_intensity: 0.5,
};

pub const DEEP_SPACE_SKY_PALETTE: SkyPalette = SkyPalette {
    zenith: Rgb::new(0.005, 0.008, 0.025),
    horizon: Rgb::new(0.015, 0.020, 0.045),
    sun_disc: Rgb::new(0.0, 0.0, 0.0),
    sun_light: Rgb::new(0.6, 0.7, 1.0),
    ambient: Rgb::new(0.35, 0.4, 0.6),
    ambient_ground: Rgb::new(0.05, 0.05, 0.15),
    fog: Rgb::new(0.005, 0.008, 0.025),
    sun_intensity: 0.5,
    ambient_intensity: 0.45,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtmosphereSample {
    pub phase: SkyPhase,
    pub palette: SkyPalette,
    pub fog_color: Rgb,
    pub fog_density: f32,
    pub star_factor: f32,
    pub overcast_factor: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtmosphereProfile {
    /// `None` means the authored deep-space override ignores the ground clock.
    pub start_hour: Option<f32>,
    pub seconds_per_day: f32,
    pub weather: Weather,
    pub sky_mode: SkyMode,
    pub sky_tint: Rgb,
    pub city_theme: Option<CityTheme>,
}

impl AtmosphereProfile {
    pub fn normalized_start_hour(self) -> Option<f32> {
        self.start_hour.map(normalize_hour)
    }

    pub fn phase_at(self, hour: f32) -> SkyPhase {
        if self.sky_mode == SkyMode::DeepSpace {
            return SkyPhase::DeepSpace;
        }
        sky_phase(hour)
    }

    pub fn fog_density(self) -> f32 {
        if self.sky_mode == SkyMode::DeepSpace {
            0.0001
        } else {
            0.0015 + self.weather.overcast_factor() * 0.0025
        }
    }

    pub fn star_factor(self, hour: f32) -> f32 {
        if self.sky_mode == SkyMode::DeepSpace {
            return 1.5;
        }
        night_factor(hour) * (1.0 - self.weather.overcast_factor())
    }

    pub fn advance_hour(self, current_hour: f32, delta_seconds: f32) -> f32 {
        let day_seconds = self.seconds_per_day.max(10.0);
        normalize_hour(current_hour + (24.0 / day_seconds) * delta_seconds)
    }

    pub fn sample(self, hour: f32) -> AtmosphereSample {
        if self.sky_mode == SkyMode::DeepSpace {
            return AtmosphereSample {
                phase: SkyPhase::DeepSpace,
                palette: DEEP_SPACE_SKY_PALETTE,
                fog_color: DEEP_SPACE_SKY_PALETTE.fog,
                fog_density: 0.0001,
                star_factor: 1.5,
                overcast_factor: 0.0,
            };
        }

        let hour = normalize_hour(hour);
        let (from, to, blend) = if !(5.0..21.0).contains(&hour) {
            (MIDNIGHT_SKY_PALETTE, MIDNIGHT_SKY_PALETTE, 0.0)
        } else if hour < 7.0 {
            (MIDNIGHT_SKY_PALETTE, DAWN_SKY_PALETTE, (hour - 5.0) / 2.0)
        } else if hour < 9.0 {
            (DAWN_SKY_PALETTE, DAY_SKY_PALETTE, (hour - 7.0) / 2.0)
        } else if hour < 17.0 {
            (DAY_SKY_PALETTE, DAY_SKY_PALETTE, 0.0)
        } else if hour < 19.0 {
            (DAY_SKY_PALETTE, DUSK_SKY_PALETTE, (hour - 17.0) / 2.0)
        } else {
            (DUSK_SKY_PALETTE, MIDNIGHT_SKY_PALETTE, (hour - 19.0) / 2.0)
        };
        let palette = tint_sky_palette(lerp_sky_palette(from, to, blend), self.sky_tint);
        let overcast = self.weather.overcast_factor();
        let fog_mix = overcast * 0.3;
        AtmosphereSample {
            phase: sky_phase(hour),
            palette,
            fog_color: lerp_rgb(palette.fog, Rgb::new(0.4, 0.4, 0.45), fog_mix),
            fog_density: self.fog_density(),
            star_factor: self.star_factor(hour),
            overcast_factor: overcast,
        }
    }
}

fn lerp_rgb(from: Rgb, to: Rgb, blend: f32) -> Rgb {
    Rgb::new(
        from.r + (to.r - from.r) * blend,
        from.g + (to.g - from.g) * blend,
        from.b + (to.b - from.b) * blend,
    )
}

fn lerp_sky_palette(from: SkyPalette, to: SkyPalette, blend: f32) -> SkyPalette {
    SkyPalette {
        zenith: lerp_rgb(from.zenith, to.zenith, blend),
        horizon: lerp_rgb(from.horizon, to.horizon, blend),
        sun_disc: lerp_rgb(from.sun_disc, to.sun_disc, blend),
        sun_light: lerp_rgb(from.sun_light, to.sun_light, blend),
        ambient: lerp_rgb(from.ambient, to.ambient, blend),
        ambient_ground: lerp_rgb(from.ambient_ground, to.ambient_ground, blend),
        fog: lerp_rgb(from.fog, to.fog, blend),
        sun_intensity: from.sun_intensity + (to.sun_intensity - from.sun_intensity) * blend,
        ambient_intensity: from.ambient_intensity
            + (to.ambient_intensity - from.ambient_intensity) * blend,
    }
}

fn tint_sky_palette(palette: SkyPalette, tint: Rgb) -> SkyPalette {
    let apply = |color: Rgb| {
        Rgb::new(
            (color.r * tint.r).min(1.0),
            (color.g * tint.g).min(1.0),
            (color.b * tint.b).min(1.0),
        )
    };
    SkyPalette {
        zenith: apply(palette.zenith),
        horizon: apply(palette.horizon),
        sun_disc: apply(palette.sun_disc),
        sun_light: apply(palette.sun_light),
        ambient: apply(palette.ambient),
        ambient_ground: apply(palette.ambient_ground),
        fog: apply(palette.fog),
        sun_intensity: palette.sun_intensity,
        ambient_intensity: palette.ambient_intensity,
    }
}

pub fn normalize_hour(hour: f32) -> f32 {
    hour.rem_euclid(24.0)
}

pub fn sky_phase(hour: f32) -> SkyPhase {
    let hour = normalize_hour(hour);
    if !(5.0..21.0).contains(&hour) {
        SkyPhase::Night
    } else if hour < 7.0 {
        SkyPhase::Dawn
    } else if hour < 9.0 {
        SkyPhase::Morning
    } else if hour < 17.0 {
        SkyPhase::Day
    } else if hour < 19.0 {
        SkyPhase::Dusk
    } else {
        SkyPhase::Evening
    }
}

fn night_factor(hour: f32) -> f32 {
    let hour = normalize_hour(hour);
    if !(5.0..21.0).contains(&hour) {
        1.0
    } else if hour < 6.0 {
        1.0 - (hour - 5.0)
    } else if hour > 20.0 {
        hour - 20.0
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionEntryAnchor {
    pub stable_id: &'static str,
    pub position: WorldPoint,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MichiganWarpDefinition {
    pub stable_id: &'static str,
    /// Normalized horizontal coordinate in the source heightmap.
    pub fraction_x: f32,
    /// Normalized vertical coordinate in the source heightmap.
    pub fraction_z: f32,
}

pub static MICHIGAN_WARP_DEFINITIONS: &[MichiganWarpDefinition] = &[
    MichiganWarpDefinition {
        stable_id: "west-giant-base",
        fraction_x: -0.39,
        fraction_z: -0.08,
    },
    MichiganWarpDefinition {
        stable_id: "north-ridge-base",
        fraction_x: 0.02,
        fraction_z: 0.39,
    },
    MichiganWarpDefinition {
        stable_id: "southeast-marsh-base",
        fraction_x: 0.37,
        fraction_z: -0.31,
    },
    MichiganWarpDefinition {
        stable_id: "keweenaw-watchtower",
        fraction_x: -0.42,
        fraction_z: 0.43,
    },
    MichiganWarpDefinition {
        stable_id: "central-firetower",
        fraction_x: -0.04,
        fraction_z: -0.02,
    },
    MichiganWarpDefinition {
        stable_id: "huron-watchtower",
        fraction_x: 0.43,
        fraction_z: 0.08,
    },
    MichiganWarpDefinition {
        stable_id: "southline-watchtower",
        fraction_x: -0.08,
        fraction_z: -0.45,
    },
    MichiganWarpDefinition {
        stable_id: "marquette-rescue-lab",
        fraction_x: -0.31,
        fraction_z: 0.25,
    },
    MichiganWarpDefinition {
        stable_id: "lansing-field-lab",
        fraction_x: 0.19,
        fraction_z: -0.35,
    },
    MichiganWarpDefinition {
        stable_id: "thumb-coast-lab",
        fraction_x: 0.39,
        fraction_z: 0.29,
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegionContentProfile {
    DetroitFortress {
        fortress_center: WorldPoint,
    },
    AshurSanctuary {
        center: WorldPoint,
    },
    OrbitalFront {
        asteroid_count: u16,
        asteroid_radius: f32,
        spawn_altitude: f32,
        earth_distance: f32,
        forced_cruise_speed: f32,
    },
    PontiacLab {
        center: WorldPoint,
        room_size: f32,
        interaction_radius: f32,
        rescue_ids: &'static [&'static str],
    },
    SwarmsLair {
        center: WorldPoint,
        arena_radius: f32,
        ceiling_height: f32,
        general_z_offset: f32,
        general_kill_radius: f32,
        swarm_minion_count: u8,
    },
    SaginawLab {
        center: WorldPoint,
        arena_radius: f32,
        ceiling_height: f32,
        captain_count: u8,
        spider_tank_count: u8,
    },
    ZugIsland {
        center: WorldPoint,
        arena_radius: f32,
        live_target: u16,
        lifetime_cap: u16,
        spawn_interval_seconds: f32,
        spawns_per_tick: u8,
        initial_titans: u8,
        initial_captains: u8,
        initial_spider_tanks: u8,
        titan_spawn_weight: u8,
        captain_spawn_weight: u8,
        spider_tank_spawn_weight: u8,
    },
    AnnArbor {
        center: WorldPoint,
        arena_radius: f32,
        saucer_altitude: f32,
        saucer_radius: f32,
        throne_captains: u8,
        live_ground_target: u16,
        lifetime_cap: u16,
        spawn_interval_seconds: f32,
        spawns_per_tick: u8,
        temporary_enemy_cap: u16,
        peripheral_buildings: u8,
        crushed_towers: u8,
    },
    MichiganWilds {
        center: WorldPoint,
        heightmap_pixel_width: u16,
        heightmap_pixel_height: u16,
        terrain_width: f32,
        terrain_depth: f32,
        subdivisions: u16,
        minimum_height: f32,
        maximum_height: f32,
        sea_level: f32,
        safe_spawn_height: f32,
        rock_line_start: f32,
        rock_line_end: f32,
        warp_definitions: &'static [MichiganWarpDefinition],
    },
}

pub static PONTIAC_RESCUE_IDS: &[&str] = &["lab_animal_kit", "glim", "mossback", "rivet"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionDefinition {
    pub id: LegacyRegionId,
    pub display_name: &'static str,
    pub banner: &'static str,
    pub objective: &'static str,
    pub completion_subtitle: &'static str,
    pub kind: RegionKind,
    pub director: RegionDirector,
    pub difficulty_multiplier: f32,
    pub entry_anchor: RegionEntryAnchor,
    pub atmosphere: AtmosphereProfile,
    pub content: RegionContentProfile,
    pub hides_shared_world: bool,
    pub suppresses_default_ground_director: bool,
}

const NEUTRAL_CITY: CityTheme = CityTheme {
    tint: Rgb::new(1.0, 1.0, 1.0),
    glow_tint: Rgb::new(1.0, 1.0, 1.0),
    ground: Rgb::new(1.0, 1.0, 1.0),
};

const DETROIT_RED_CITY: CityTheme = CityTheme {
    tint: Rgb::new(1.6, 0.55, 0.35),
    glow_tint: Rgb::new(1.8, 0.65, 0.30),
    ground: Rgb::new(2.6, 1.20, 0.70),
};

const DETROIT_VOID_CITY: CityTheme = CityTheme {
    tint: Rgb::new(0.45, 0.55, 1.30),
    glow_tint: Rgb::new(1.30, 0.40, 1.80),
    ground: Rgb::new(0.55, 0.50, 1.00),
};

const ASHUR_CITY: CityTheme = CityTheme {
    tint: Rgb::new(1.20, 1.00, 0.80),
    glow_tint: Rgb::new(1.30, 1.10, 0.70),
    ground: Rgb::new(1.40, 1.20, 0.90),
};

const PONTIAC_CITY: CityTheme = CityTheme {
    tint: Rgb::new(0.55, 0.75, 1.20),
    glow_tint: Rgb::new(0.50, 0.90, 1.40),
    ground: Rgb::new(0.40, 0.50, 0.70),
};

const fn ground_atmosphere(
    hour: f32,
    sky_tint: Rgb,
    city_theme: Option<CityTheme>,
) -> AtmosphereProfile {
    AtmosphereProfile {
        start_hour: Some(hour),
        seconds_per_day: HEAVY_SECONDS_PER_DAY,
        weather: Weather::Clear,
        sky_mode: SkyMode::Ground,
        sky_tint,
        city_theme,
    }
}

pub static REGION_DEFINITIONS: &[RegionDefinition] = &[
    RegionDefinition {
        id: LegacyRegionId::DetroitStarCityFront,
        display_name: "DETROIT — Star City Front",
        banner: "LEVEL 1 — RESCUE THE ALLY",
        objective: "Breach the enemy fortress and rescue the captured ally.",
        completion_subtitle: "Stand by — the war isn't over.",
        kind: RegionKind::DetroitCampaign,
        director: RegionDirector::CampaignFortress,
        difficulty_multiplier: 1.0,
        entry_anchor: RegionEntryAnchor {
            stable_id: "detroit_star_city_spawn",
            position: WorldPoint::ZERO,
        },
        atmosphere: ground_atmosphere(9.0, Rgb::new(1.0, 1.0, 1.0), Some(NEUTRAL_CITY)),
        content: RegionContentProfile::DetroitFortress {
            fortress_center: WorldPoint::new(380.0, 0.0, -120.0),
        },
        hides_shared_world: false,
        suppresses_default_ground_director: false,
    },
    RegionDefinition {
        id: LegacyRegionId::DetroitHoldTheLine,
        display_name: "DETROIT — Hold the Line",
        banner: "LEVEL 2 — HOLD THE LINE",
        objective: "The captains have invaded. Crush the next fortress and survive.",
        completion_subtitle: "The plague clears. One stronghold left.",
        kind: RegionKind::DetroitCampaign,
        director: RegionDirector::CampaignFortress,
        difficulty_multiplier: 1.5,
        entry_anchor: RegionEntryAnchor {
            stable_id: "detroit_hold_line_spawn",
            position: WorldPoint::new(-200.0, 0.0, -200.0),
        },
        atmosphere: ground_atmosphere(
            18.5,
            Rgb::new(1.25, 0.55, 0.45),
            Some(DETROIT_RED_CITY),
        ),
        content: RegionContentProfile::DetroitFortress {
            fortress_center: WorldPoint::new(-360.0, 0.0, -360.0),
        },
        hides_shared_world: false,
        suppresses_default_ground_director: false,
    },
    RegionDefinition {
        id: LegacyRegionId::DetroitPurgeTheVoid,
        display_name: "DETROIT — Purge the Void",
        banner: "LEVEL 3 — PURGE THE VOID",
        objective: "Storm the final command tower. End the invasion.",
        completion_subtitle: "DETROIT IS FREE. The hybrids are broken.",
        kind: RegionKind::DetroitCampaign,
        director: RegionDirector::CampaignFortress,
        difficulty_multiplier: 2.0,
        entry_anchor: RegionEntryAnchor {
            stable_id: "detroit_void_spawn",
            position: WorldPoint::new(-60.0, 0.0, 240.0),
        },
        atmosphere: ground_atmosphere(
            22.5,
            Rgb::new(0.55, 0.55, 1.05),
            Some(DETROIT_VOID_CITY),
        ),
        content: RegionContentProfile::DetroitFortress {
            fortress_center: WorldPoint::new(-120.0, 0.0, 420.0),
        },
        hides_shared_world: false,
        suppresses_default_ground_director: false,
    },
    RegionDefinition {
        id: LegacyRegionId::AshurSanctuary,
        display_name: "ASHUR SANCTUARY",
        banner: "ASHUR SANCTUARY — A QUIET PLACE TO HEAL",
        objective: "Tend the bio-crops, rehabilitate rescued Animatons, and help the Village of Earth.",
        completion_subtitle: "The sanctuary endures.",
        kind: RegionKind::Sanctuary,
        director: RegionDirector::AshurSanctuary,
        difficulty_multiplier: 0.0,
        entry_anchor: RegionEntryAnchor {
            stable_id: "ashur_sanctuary_spawn",
            position: WorldPoint::new(-480.0, 0.0, -480.0),
        },
        atmosphere: ground_atmosphere(6.5, Rgb::new(1.15, 1.0, 0.85), Some(ASHUR_CITY)),
        content: RegionContentProfile::AshurSanctuary {
            center: WorldPoint::new(-480.0, 0.0, -480.0),
        },
        hides_shared_world: true,
        suppresses_default_ground_director: true,
    },
    RegionDefinition {
        id: LegacyRegionId::OrbitalFront,
        display_name: "ORBITAL FRONT",
        banner: "LEVEL 5 — ORBITAL FRONT",
        objective: "Engage the orbital fleet — clear asteroids and bring down the motherships.",
        completion_subtitle: "Orbit secured.",
        kind: RegionKind::OrbitalCombat,
        director: RegionDirector::OrbitalFleet,
        difficulty_multiplier: 2.4,
        entry_anchor: RegionEntryAnchor {
            stable_id: "orbital_front_spawn",
            // SpaceLevelSystem authors altitude 300; adapters should use this
            // stable anchor rather than Game.tsx's older y=60 travel shim.
            position: WorldPoint::new(0.0, 300.0, 0.0),
        },
        atmosphere: AtmosphereProfile {
            start_hour: None,
            seconds_per_day: HEAVY_SECONDS_PER_DAY,
            weather: Weather::Clear,
            sky_mode: SkyMode::DeepSpace,
            sky_tint: Rgb::new(0.4, 0.5, 0.95),
            city_theme: None,
        },
        content: RegionContentProfile::OrbitalFront {
            asteroid_count: 64,
            asteroid_radius: 260.0,
            spawn_altitude: 300.0,
            earth_distance: 1_200.0,
            forced_cruise_speed: 28.0,
        },
        hides_shared_world: true,
        suppresses_default_ground_director: true,
    },
    RegionDefinition {
        id: LegacyRegionId::PontiacSecretLab,
        display_name: "PONTIAC SECRET LAB",
        banner: "PONTIAC SECRET LAB — RESTRICTED RESEARCH",
        objective: "Explore the covert lab. Read the terminals, study the cryo subjects, talk to Dr. You.",
        completion_subtitle: "The lab keeps its secrets — for now.",
        kind: RegionKind::InteriorExploration,
        director: RegionDirector::PontiacLab,
        difficulty_multiplier: 0.0,
        entry_anchor: RegionEntryAnchor {
            stable_id: "pontiac_lab_spawn",
            position: WorldPoint::new(480.0, 0.0, 480.0),
        },
        atmosphere: ground_atmosphere(0.5, Rgb::new(0.55, 0.65, 1.10), Some(PONTIAC_CITY)),
        content: RegionContentProfile::PontiacLab {
            center: WorldPoint::new(480.0, 0.0, 480.0),
            room_size: 30.0,
            interaction_radius: 4.5,
            rescue_ids: PONTIAC_RESCUE_IDS,
        },
        hides_shared_world: true,
        suppresses_default_ground_director: true,
    },
    RegionDefinition {
        id: LegacyRegionId::SwarmsLair,
        display_name: "SWARMS LAIR",
        banner: "LEVEL 7 — SWARMS LAIR",
        objective: "Descend into the lair. Cut through the swarm. End the General.",
        completion_subtitle: "The General falls. The Swarm scatters.",
        kind: RegionKind::InteriorCombat,
        director: RegionDirector::SwarmsLair,
        difficulty_multiplier: 2.8,
        entry_anchor: RegionEntryAnchor {
            stable_id: "swarms_lair_spawn",
            position: WorldPoint::ZERO,
        },
        atmosphere: ground_atmosphere(22.0, Rgb::new(0.85, 0.30, 0.55), None),
        content: RegionContentProfile::SwarmsLair {
            center: WorldPoint::ZERO,
            arena_radius: 40.0,
            ceiling_height: 22.0,
            general_z_offset: 28.0,
            general_kill_radius: 200.0,
            swarm_minion_count: 10,
        },
        hides_shared_world: true,
        suppresses_default_ground_director: true,
    },
    RegionDefinition {
        id: LegacyRegionId::SaginawUnderwaterLab,
        display_name: "SAGINAW UNDERWATER LAB",
        banner: "LEVEL 8 — SAGINAW UNDERWATER LAB",
        objective: "Breach the flooded Saginaw lab. Survive the captains. Bring down the spider tanks.",
        completion_subtitle: "Saginaw is silent. The water swallows the rest.",
        kind: RegionKind::InteriorCombat,
        director: RegionDirector::SaginawLab,
        difficulty_multiplier: 3.5,
        entry_anchor: RegionEntryAnchor {
            stable_id: "saginaw_lab_spawn",
            position: WorldPoint::new(1_500.0, 0.0, -1_500.0),
        },
        atmosphere: ground_atmosphere(23.0, Rgb::new(0.20, 0.45, 0.85), None),
        content: RegionContentProfile::SaginawLab {
            center: WorldPoint::new(1_500.0, 0.0, -1_500.0),
            arena_radius: 50.0,
            ceiling_height: 28.0,
            captain_count: 4,
            spider_tank_count: 2,
        },
        hides_shared_world: true,
        suppresses_default_ground_director: true,
    },
    RegionDefinition {
        id: LegacyRegionId::ZugIslandLegion,
        display_name: "ZUG ISLAND — LEGION",
        banner: "LEVEL 9 — ZUG ISLAND LEGION",
        objective: "Hold Zug Island. Cut down the Legion — titans, captains, spider tanks, no end.",
        completion_subtitle: "The Legion is broken. Zug Island holds.",
        kind: RegionKind::OpenCombat,
        director: RegionDirector::ZugLegion,
        difficulty_multiplier: 4.5,
        entry_anchor: RegionEntryAnchor {
            stable_id: "zug_island_spawn",
            position: WorldPoint::new(-1_500.0, 0.0, -1_500.0),
        },
        atmosphere: ground_atmosphere(21.0, Rgb::new(1.40, 0.55, 0.25), None),
        content: RegionContentProfile::ZugIsland {
            center: WorldPoint::new(-1_500.0, 0.0, -1_500.0),
            arena_radius: 120.0,
            live_target: 60,
            lifetime_cap: 600,
            spawn_interval_seconds: 1.5,
            spawns_per_tick: 4,
            initial_titans: 12,
            initial_captains: 8,
            initial_spider_tanks: 4,
            titan_spawn_weight: 55,
            captain_spawn_weight: 30,
            spider_tank_spawn_weight: 15,
        },
        hides_shared_world: true,
        suppresses_default_ground_director: true,
    },
    RegionDefinition {
        id: LegacyRegionId::AnnArborApocalypse,
        display_name: "ANN ARBOR APOCALYPSE",
        banner: "LEVEL 10 — ANN ARBOR APOCALYPSE",
        objective: "A mothership has landed on Ann Arbor. Bring down the maxed captains on its deck and hold the streets against the swarm.",
        completion_subtitle: "The mothership is dead. Ann Arbor breathes.",
        kind: RegionKind::OpenCombat,
        director: RegionDirector::AnnArborSwarm,
        difficulty_multiplier: 5.0,
        entry_anchor: RegionEntryAnchor {
            stable_id: "ann_arbor_spawn",
            position: WorldPoint::new(-3_000.0, 0.0, 0.0),
        },
        atmosphere: ground_atmosphere(21.5, Rgb::new(0.85, 0.40, 1.10), None),
        content: RegionContentProfile::AnnArbor {
            center: WorldPoint::new(-3_000.0, 0.0, 0.0),
            arena_radius: 220.0,
            saucer_altitude: 130.0,
            saucer_radius: 160.0,
            throne_captains: 10,
            live_ground_target: 70,
            lifetime_cap: 700,
            spawn_interval_seconds: 1.4,
            spawns_per_tick: 5,
            temporary_enemy_cap: 120,
            peripheral_buildings: 18,
            crushed_towers: 5,
        },
        hides_shared_world: true,
        suppresses_default_ground_director: true,
    },
    RegionDefinition {
        id: LegacyRegionId::MichiganWilds,
        display_name: "MICHIGAN WILDS",
        banner: "MICHIGAN WILDS — HEIGHTMAP FRONTIER",
        objective: "Explore flooded lowlands, grass foothills, and rocky peaks generated from MIHEIGHTMAP.",
        completion_subtitle: "The wilds settle back into the mist.",
        kind: RegionKind::HeightmapFrontier,
        director: RegionDirector::MichiganWilds,
        difficulty_multiplier: 0.0,
        entry_anchor: RegionEntryAnchor {
            stable_id: "michigan_wilds_spawn",
            position: WorldPoint::new(3_000.0, 18.0, 1_500.0),
        },
        atmosphere: ground_atmosphere(14.5, Rgb::new(0.80, 1.00, 1.08), None),
        content: RegionContentProfile::MichiganWilds {
            center: WorldPoint::new(3_000.0, 0.0, 1_500.0),
            heightmap_pixel_width: 1_448,
            heightmap_pixel_height: 1_086,
            terrain_width: 7_200.0,
            terrain_depth: 5_400.0,
            subdivisions: 320,
            minimum_height: -26.0,
            maximum_height: 148.0,
            sea_level: 0.0,
            safe_spawn_height: 18.0,
            rock_line_start: 34.0,
            rock_line_end: 58.0,
            warp_definitions: MICHIGAN_WARP_DEFINITIONS,
        },
        hides_shared_world: true,
        suppresses_default_ground_director: true,
    },
];

pub fn region_definition(id: LegacyRegionId) -> &'static RegionDefinition {
    // Definitions are asserted to be complete and uniquely keyed in tests.
    &REGION_DEFINITIONS[usize::from(id.legacy_level() - 1)]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PontiacRescueId {
    LabAnimalKit,
    Glim,
    Mossback,
    Rivet,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionPersistentFlag {
    FortressCleared,
    SwarmGeneralDefeated,
    PontiacAnimalRescued(PontiacRescueId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionReturnAnchor {
    pub stable_id: String,
    pub region: LegacyRegionId,
    pub position: WorldPoint,
}

impl RegionReturnAnchor {
    pub fn new(
        stable_id: impl Into<String>,
        region: LegacyRegionId,
        position: WorldPoint,
    ) -> Result<Self, RegionSessionError> {
        let stable_id = stable_id.into();
        if stable_id.trim().is_empty() {
            return Err(RegionSessionError::EmptyAnchorId);
        }
        if !position.is_finite() {
            return Err(RegionSessionError::NonFinitePosition);
        }
        Ok(Self {
            stable_id,
            region,
            position,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionMountPhase {
    Mounting,
    Active,
    Unmounting,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionLifecycleRecord {
    pub visits: u32,
    pub clears: u32,
    pub last_generation: u64,
    pub persistent_flags: BTreeSet<RegionPersistentFlag>,
}

impl RegionLifecycleRecord {
    pub const fn is_cleared(&self) -> bool {
        self.clears > 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveRegionSession {
    pub region: LegacyRegionId,
    pub generation: u64,
    pub phase: RegionMountPhase,
    /// Preserved while hopping between custom zones, so a return operation
    /// reaches the shared-world point from which the trip began.
    pub return_anchor: Option<RegionReturnAnchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionSession {
    pub schema_version: u16,
    pub active: ActiveRegionSession,
    pub records: BTreeMap<LegacyRegionId, RegionLifecycleRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionTravelAction {
    Enter,
    Reassert,
    Return,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionTravelPlan {
    pub action: RegionTravelAction,
    pub from: LegacyRegionId,
    pub to: LegacyRegionId,
    pub generation: u64,
    pub arrival: WorldPoint,
    pub return_anchor: Option<RegionReturnAnchor>,
    /// Mirrors Game.tsx: only a true level change clears transient ground and
    /// aerial enemies; same-level force-start merely reasserts state.
    pub clear_transient_combat: bool,
    /// Consumers must tear down the previous custom zone before mounting this
    /// one, preventing stale cleanup from restoring shared-world visibility.
    pub dispose_previous_before_mount: bool,
    pub hides_shared_world: bool,
    pub suppresses_default_ground_director: bool,
    pub atmosphere: AtmosphereProfile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionSessionError {
    UnsupportedSchemaVersion(u16),
    EmptyAnchorId,
    NonFinitePosition,
    NoReturnAnchor,
    MissingActiveRecord,
    WrongMountPhase {
        expected: RegionMountPhase,
        actual: RegionMountPhase,
    },
    FlagDoesNotBelong {
        region: LegacyRegionId,
        flag: RegionPersistentFlag,
    },
    AuthoredRouteBlocked {
        from: LegacyRegionId,
        to: LegacyRegionId,
    },
}

/// Mirrors Heavy Water's one explicit travel restriction: Ashur's sanctuary
/// does not open directly onto any of the three Detroit war fronts.
pub const fn authored_fast_travel_allowed(from: LegacyRegionId, to: LegacyRegionId) -> bool {
    !(matches!(from, LegacyRegionId::AshurSanctuary) && to.is_detroit_campaign())
}

impl RegionSession {
    pub fn new(initial: LegacyRegionId) -> Self {
        let mut records = BTreeMap::new();
        records.insert(
            initial,
            RegionLifecycleRecord {
                visits: 1,
                clears: 0,
                last_generation: 1,
                persistent_flags: BTreeSet::new(),
            },
        );
        Self {
            schema_version: HEAVY_REGIONS_SCHEMA_VERSION,
            active: ActiveRegionSession {
                region: initial,
                generation: 1,
                phase: RegionMountPhase::Active,
                return_anchor: None,
            },
            records,
        }
    }

    /// Creates a session from Heavy Water's numeric level snapshot. Only a
    /// loaded Detroit L2/L3 implies prior campaign clears; loading any side
    /// zone does not grant campaign progress.
    pub fn from_legacy_snapshot(raw_level: i32) -> Self {
        let initial = LegacyRegionId::from_legacy_level_clamped(raw_level);
        let mut session = Self::new(initial);
        match initial {
            LegacyRegionId::DetroitHoldTheLine => {
                session.mark_cleared(LegacyRegionId::DetroitStarCityFront);
            }
            LegacyRegionId::DetroitPurgeTheVoid => {
                session.mark_cleared(LegacyRegionId::DetroitStarCityFront);
                session.mark_cleared(LegacyRegionId::DetroitHoldTheLine);
            }
            _ => {}
        }
        session
    }

    pub fn validate(&self) -> Result<(), RegionSessionError> {
        if self.schema_version != HEAVY_REGIONS_SCHEMA_VERSION {
            return Err(RegionSessionError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        let active_record = self
            .records
            .get(&self.active.region)
            .ok_or(RegionSessionError::MissingActiveRecord)?;
        if active_record.last_generation > self.active.generation {
            return Err(RegionSessionError::MissingActiveRecord);
        }
        if let Some(anchor) = &self.active.return_anchor {
            if anchor.stable_id.trim().is_empty() {
                return Err(RegionSessionError::EmptyAnchorId);
            }
            if !anchor.position.is_finite() {
                return Err(RegionSessionError::NonFinitePosition);
            }
        }
        for (region, record) in &self.records {
            for flag in &record.persistent_flags {
                if !persistent_flag_belongs(*region, flag) {
                    return Err(RegionSessionError::FlagDoesNotBelong {
                        region: *region,
                        flag: flag.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn record(&self, region: LegacyRegionId) -> Option<&RegionLifecycleRecord> {
        self.records.get(&region)
    }

    pub fn travel_to(
        &mut self,
        target: LegacyRegionId,
        current_position: WorldPoint,
    ) -> Result<RegionTravelPlan, RegionSessionError> {
        if !current_position.is_finite() {
            return Err(RegionSessionError::NonFinitePosition);
        }

        let previous = self.active.region;
        if !authored_fast_travel_allowed(previous, target) {
            return Err(RegionSessionError::AuthoredRouteBlocked {
                from: previous,
                to: target,
            });
        }
        if previous == target {
            let definition = region_definition(target);
            self.active.phase = RegionMountPhase::Mounting;
            return Ok(RegionTravelPlan {
                action: RegionTravelAction::Reassert,
                from: previous,
                to: target,
                generation: self.active.generation,
                arrival: definition.entry_anchor.position,
                return_anchor: self.active.return_anchor.clone(),
                clear_transient_combat: false,
                dispose_previous_before_mount: false,
                hides_shared_world: definition.hides_shared_world,
                suppresses_default_ground_director: definition.suppresses_default_ground_director,
                atmosphere: definition.atmosphere,
            });
        }

        let previous_definition = region_definition(previous);
        let target_definition = region_definition(target);
        let return_anchor = if target_definition.hides_shared_world {
            if previous_definition.hides_shared_world {
                self.active.return_anchor.clone()
            } else {
                Some(RegionReturnAnchor::new(
                    format!("return_from_{}", previous.stable_id()),
                    previous,
                    current_position,
                )?)
            }
        } else {
            None
        };

        let generation = self.active.generation.saturating_add(1);
        let record = self.records.entry(target).or_default();
        record.visits = record.visits.saturating_add(1);
        record.last_generation = generation;
        self.active = ActiveRegionSession {
            region: target,
            generation,
            phase: RegionMountPhase::Mounting,
            return_anchor: return_anchor.clone(),
        };

        Ok(RegionTravelPlan {
            action: RegionTravelAction::Enter,
            from: previous,
            to: target,
            generation,
            arrival: target_definition.entry_anchor.position,
            return_anchor,
            clear_transient_combat: true,
            dispose_previous_before_mount: true,
            hides_shared_world: target_definition.hides_shared_world,
            suppresses_default_ground_director: target_definition
                .suppresses_default_ground_director,
            atmosphere: target_definition.atmosphere,
        })
    }

    pub fn finish_mount(&mut self) -> Result<(), RegionSessionError> {
        if self.active.phase != RegionMountPhase::Mounting {
            return Err(RegionSessionError::WrongMountPhase {
                expected: RegionMountPhase::Mounting,
                actual: self.active.phase,
            });
        }
        self.active.phase = RegionMountPhase::Active;
        Ok(())
    }

    pub fn begin_return(&mut self) -> Result<RegionTravelPlan, RegionSessionError> {
        let anchor = self
            .active
            .return_anchor
            .clone()
            .ok_or(RegionSessionError::NoReturnAnchor)?;
        let previous = self.active.region;
        if !authored_fast_travel_allowed(previous, anchor.region) {
            return Err(RegionSessionError::AuthoredRouteBlocked {
                from: previous,
                to: anchor.region,
            });
        }
        let generation = self.active.generation.saturating_add(1);
        let target_definition = region_definition(anchor.region);
        let record = self.records.entry(anchor.region).or_default();
        record.visits = record.visits.saturating_add(1);
        record.last_generation = generation;
        self.active = ActiveRegionSession {
            region: anchor.region,
            generation,
            phase: RegionMountPhase::Mounting,
            return_anchor: None,
        };
        Ok(RegionTravelPlan {
            action: RegionTravelAction::Return,
            from: previous,
            to: anchor.region,
            generation,
            arrival: anchor.position,
            return_anchor: None,
            clear_transient_combat: true,
            dispose_previous_before_mount: true,
            hides_shared_world: target_definition.hides_shared_world,
            suppresses_default_ground_director: target_definition
                .suppresses_default_ground_director,
            atmosphere: target_definition.atmosphere,
        })
    }

    pub fn mark_cleared(&mut self, region: LegacyRegionId) {
        let record = self.records.entry(region).or_default();
        // Heavy Water guards duplicate fortress-clear events; completion is
        // therefore idempotent even though the field remains a migration-
        // friendly counter.
        record.clears = 1;
        if region.is_detroit_campaign() {
            record
                .persistent_flags
                .insert(RegionPersistentFlag::FortressCleared);
        }
    }

    pub fn set_persistent_flag(
        &mut self,
        region: LegacyRegionId,
        flag: RegionPersistentFlag,
    ) -> Result<bool, RegionSessionError> {
        if !persistent_flag_belongs(region, &flag) {
            return Err(RegionSessionError::FlagDoesNotBelong { region, flag });
        }
        let fortress_clear = matches!(&flag, RegionPersistentFlag::FortressCleared);
        let record = self.records.entry(region).or_default();
        let inserted = record.persistent_flags.insert(flag);
        if fortress_clear {
            record.clears = 1;
        }
        Ok(inserted)
    }
}

fn persistent_flag_belongs(region: LegacyRegionId, flag: &RegionPersistentFlag) -> bool {
    match flag {
        RegionPersistentFlag::FortressCleared => region.is_detroit_campaign(),
        RegionPersistentFlag::SwarmGeneralDefeated => region == LegacyRegionId::SwarmsLair,
        RegionPersistentFlag::PontiacAnimalRescued(_) => region == LegacyRegionId::PontiacSecretLab,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerSlot(pub u8);

impl PlayerSlot {
    pub fn new(slot: u8) -> Result<Self, PlayerVisibilityError> {
        if slot >= MAX_TRACKED_PLAYERS {
            return Err(PlayerVisibilityError::PlayerOutOfRange(slot));
        }
        Ok(Self(slot))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerVisibility(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerVisibilityError {
    PlayerOutOfRange(u8),
    EmptyVisibility,
}

impl PlayerVisibility {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self((1_u32 << MAX_TRACKED_PLAYERS) - 1);

    pub fn only(player: PlayerSlot) -> Self {
        Self(1_u32 << player.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn is_valid(self) -> bool {
        self.0 & !Self::ALL.0 == 0
    }

    pub fn contains(self, player: PlayerSlot) -> bool {
        player.0 < MAX_TRACKED_PLAYERS && self.0 & (1_u32 << player.0) != 0
    }

    pub fn insert(&mut self, player: PlayerSlot) {
        if player.0 < MAX_TRACKED_PLAYERS {
            self.0 |= 1_u32 << player.0;
        }
    }

    pub fn remove(&mut self, player: PlayerSlot) {
        if player.0 < MAX_TRACKED_PLAYERS {
            self.0 &= !(1_u32 << player.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapMarkerKind {
    Player,
    Enemy,
    Shop,
    Garden,
    Chest,
    SupplyCache,
    EnemyBase,
    BossFortress,
    RegionPortal,
    Objective,
    MiningNode,
}

impl MapMarkerKind {
    /// Higher values are returned first by [`MapMarkerRegistry::visible_for`].
    pub const fn priority(self) -> u8 {
        match self {
            Self::Objective => 100,
            Self::Player => 90,
            Self::BossFortress => 80,
            Self::EnemyBase => 70,
            Self::Enemy => 60,
            Self::RegionPortal => 50,
            Self::Shop => 40,
            Self::Garden => 35,
            Self::Chest => 30,
            Self::SupplyCache => 25,
            Self::MiningNode => 20,
        }
    }

    pub const fn sourced_offscreen_policy(self) -> OffscreenArrowPolicy {
        match self {
            // Cleared bases retain a dim edge arrow in Heavy Water.
            Self::EnemyBase => OffscreenArrowPolicy::Always,
            // A cleared boss fortress loses its off-screen objective arrow.
            Self::BossFortress => OffscreenArrowPolicy::WhileActive,
            _ => OffscreenArrowPolicy::Never,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapMarkerStatus {
    Active,
    Cleared,
    Looted,
    Inactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffscreenArrowPolicy {
    Never,
    WhileActive,
    Always,
}

impl OffscreenArrowPolicy {
    pub const fn permits(self, status: MapMarkerStatus) -> bool {
        match self {
            Self::Never => false,
            Self::WhileActive => matches!(status, MapMarkerStatus::Active),
            Self::Always => !matches!(status, MapMarkerStatus::Inactive),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapMarker {
    pub stable_id: String,
    pub kind: MapMarkerKind,
    pub region: LegacyRegionId,
    pub position: WorldPoint,
    pub label: Option<String>,
    pub status: MapMarkerStatus,
    pub visible_to: PlayerVisibility,
    pub offscreen_arrow: OffscreenArrowPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapRegistryError {
    EmptyMarkerId,
    NonFinitePosition,
    EmptyVisibility,
    InvalidVisibility,
    UnknownMarker(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MapMarkerRegistry {
    markers: BTreeMap<String, MapMarker>,
}

impl MapMarkerRegistry {
    pub fn len(&self) -> usize {
        self.markers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.markers.is_empty()
    }

    pub fn get(&self, stable_id: &str) -> Option<&MapMarker> {
        self.markers.get(stable_id)
    }

    pub fn upsert(&mut self, marker: MapMarker) -> Result<Option<MapMarker>, MapRegistryError> {
        if marker.stable_id.trim().is_empty() {
            return Err(MapRegistryError::EmptyMarkerId);
        }
        if !marker.position.is_finite() {
            return Err(MapRegistryError::NonFinitePosition);
        }
        if marker.visible_to.is_empty() {
            return Err(MapRegistryError::EmptyVisibility);
        }
        if !marker.visible_to.is_valid() {
            return Err(MapRegistryError::InvalidVisibility);
        }
        Ok(self.markers.insert(marker.stable_id.clone(), marker))
    }

    pub fn remove(&mut self, stable_id: &str) -> Option<MapMarker> {
        self.markers.remove(stable_id)
    }

    pub fn set_status(
        &mut self,
        stable_id: &str,
        status: MapMarkerStatus,
    ) -> Result<(), MapRegistryError> {
        let marker = self
            .markers
            .get_mut(stable_id)
            .ok_or_else(|| MapRegistryError::UnknownMarker(stable_id.to_owned()))?;
        marker.status = status;
        Ok(())
    }

    pub fn set_visible_for(
        &mut self,
        stable_id: &str,
        player: PlayerSlot,
        visible: bool,
    ) -> Result<(), MapRegistryError> {
        let marker = self
            .markers
            .get_mut(stable_id)
            .ok_or_else(|| MapRegistryError::UnknownMarker(stable_id.to_owned()))?;
        if visible {
            marker.visible_to.insert(player);
        } else {
            marker.visible_to.remove(player);
        }
        Ok(())
    }

    pub fn visible_for(&self, player: PlayerSlot, region: LegacyRegionId) -> Vec<&MapMarker> {
        let mut visible: Vec<_> = self
            .markers
            .values()
            .filter(|marker| {
                marker.region == region
                    && marker.visible_to.contains(player)
                    && marker.status != MapMarkerStatus::Inactive
            })
            .collect();
        visible.sort_by(|left, right| {
            right
                .kind
                .priority()
                .cmp(&left.kind.priority())
                .then_with(|| left.stable_id.cmp(&right.stable_id))
        });
        visible
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MapPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapViewport {
    pub world_center_x: f32,
    pub world_center_z: f32,
    pub world_width: f32,
    pub world_depth: f32,
    pub pixel_width: f32,
    pub pixel_height: f32,
    pub edge_inset: f32,
}

impl Default for MapViewport {
    fn default() -> Self {
        Self {
            world_center_x: 600.0,
            world_center_z: 600.0,
            world_width: HEAVY_MAP_WORLD_SIZE,
            world_depth: HEAVY_MAP_WORLD_SIZE,
            pixel_width: HEAVY_MAP_CANVAS_SIZE,
            pixel_height: HEAVY_MAP_CANVAS_SIZE,
            edge_inset: 10.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapEdgeArrow {
    pub position: MapPoint,
    pub angle_radians: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapIconFalloff {
    pub scale: f32,
    pub alpha: f32,
}

impl MapViewport {
    pub fn is_valid(self) -> bool {
        self.world_center_x.is_finite()
            && self.world_center_z.is_finite()
            && self.world_width.is_finite()
            && self.world_width > 0.0
            && self.world_depth.is_finite()
            && self.world_depth > 0.0
            && self.pixel_width.is_finite()
            && self.pixel_width > 0.0
            && self.pixel_height.is_finite()
            && self.pixel_height > 0.0
            && self.edge_inset.is_finite()
            && self.edge_inset >= 0.0
            && self.edge_inset * 2.0 < self.pixel_width
            && self.edge_inset * 2.0 < self.pixel_height
    }

    pub fn project(self, world: WorldPoint) -> Option<MapPoint> {
        if !self.is_valid() || !world.is_finite() {
            return None;
        }
        let relative_x =
            (world.x - self.world_center_x + self.world_width * 0.5) / self.world_width;
        let relative_z =
            (world.z - self.world_center_z + self.world_depth * 0.5) / self.world_depth;
        Some(MapPoint {
            x: relative_x * self.pixel_width,
            y: relative_z * self.pixel_height,
        })
    }

    pub fn is_on_screen(self, point: MapPoint) -> bool {
        point.x >= 0.0
            && point.x <= self.pixel_width
            && point.y >= 0.0
            && point.y <= self.pixel_height
    }

    /// Heavy Water's ray-to-inset-rectangle off-screen indicator algorithm.
    pub fn edge_arrow(
        self,
        target_world: WorldPoint,
        player_world: WorldPoint,
    ) -> Option<MapEdgeArrow> {
        let target = self.project(target_world)?;
        if self.is_on_screen(target) {
            return None;
        }

        let player = self.project(player_world)?;
        let min_x = self.edge_inset;
        let max_x = self.pixel_width - self.edge_inset;
        let min_y = self.edge_inset;
        let max_y = self.pixel_height - self.edge_inset;
        let (mut origin_x, mut origin_y) = if self.is_on_screen(player) {
            (player.x, player.y)
        } else {
            (self.pixel_width * 0.5, self.pixel_height * 0.5)
        };
        origin_x = origin_x.clamp(min_x, max_x);
        origin_y = origin_y.clamp(min_y, max_y);

        let dx = target.x - origin_x;
        let dy = target.y - origin_y;
        if dx == 0.0 && dy == 0.0 {
            return None;
        }

        let mut intersection = f32::INFINITY;
        if dx > 0.0 {
            intersection = intersection.min((max_x - origin_x) / dx);
        } else if dx < 0.0 {
            intersection = intersection.min((min_x - origin_x) / dx);
        }
        if dy > 0.0 {
            intersection = intersection.min((max_y - origin_y) / dy);
        } else if dy < 0.0 {
            intersection = intersection.min((min_y - origin_y) / dy);
        }
        if !intersection.is_finite() || intersection <= 0.0 {
            return None;
        }

        Some(MapEdgeArrow {
            position: MapPoint {
                x: origin_x + dx * intersection,
                y: origin_y + dy * intersection,
            },
            angle_radians: dy.atan2(dx),
        })
    }
}

pub fn map_icon_falloff(distance: f32) -> MapIconFalloff {
    if distance <= HEAVY_MAP_ICON_FALLOFF_NEAR {
        return MapIconFalloff {
            scale: 1.0,
            alpha: 1.0,
        };
    }
    if distance >= HEAVY_MAP_ICON_FALLOFF_FAR {
        return MapIconFalloff {
            scale: 0.45,
            alpha: 0.35,
        };
    }
    let t = (distance - HEAVY_MAP_ICON_FALLOFF_NEAR)
        / (HEAVY_MAP_ICON_FALLOFF_FAR - HEAVY_MAP_ICON_FALLOFF_NEAR);
    MapIconFalloff {
        scale: 1.0 - t * 0.55,
        alpha: 1.0 - t * 0.65,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPropKind {
    Crate,
    Barrel,
    Canister,
    Container,
    HoloSign,
    OpenContainer,
    ScrapPile,
    CrystalCluster,
    BioPod,
    GearCache,
}

impl LegacyPropKind {
    pub const ALL: [Self; 10] = [
        Self::Crate,
        Self::Barrel,
        Self::Canister,
        Self::Container,
        Self::HoloSign,
        Self::OpenContainer,
        Self::ScrapPile,
        Self::CrystalCluster,
        Self::BioPod,
        Self::GearCache,
    ];

    pub const fn is_mining_node(self) -> bool {
        matches!(
            self,
            Self::ScrapPile | Self::CrystalCluster | Self::BioPod | Self::GearCache
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropLootAuthority {
    WorldDestruction,
    /// A Heavy Water open container is one shared-world claim. The Rust state
    /// records who triggered it, while ensuring another player or subsequent
    /// destruction cannot mint the same bundle twice.
    SharedProximityOrDestruction,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropRespawnPolicy {
    Never,
    AfterSeconds(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LootDropDefinition {
    pub pickup_id: &'static str,
    pub amount: u32,
    /// Heavy Water represents weapon parts as `weapon_part` plus a weapon id.
    /// Economy adapters may map this pair to a concrete inventory item.
    pub weapon_id: Option<&'static str>,
}

const fn drop(pickup_id: &'static str, amount: u32) -> LootDropDefinition {
    LootDropDefinition {
        pickup_id,
        amount,
        weapon_id: None,
    }
}

const CRATE_DROPS: &[LootDropDefinition] = &[drop("scrap_metal", 2), drop("gear", 1)];
const BARREL_DROPS: &[LootDropDefinition] = &[drop("energy_core", 1), drop("scrap_metal", 1)];
const CANISTER_DROPS: &[LootDropDefinition] = &[drop("nano_fiber", 1), drop("circuit_board", 1)];
const CONTAINER_DROPS: &[LootDropDefinition] = &[
    drop("scrap_metal", 4),
    drop("gear", 3),
    drop("circuit_board", 1),
    drop("energy_core", 1),
];
const HOLO_SIGN_DROPS: &[LootDropDefinition] = &[drop("circuit_board", 1)];
const OPEN_CONTAINER_DROPS: &[LootDropDefinition] = &[
    drop("scrap_metal", 3),
    drop("gear", 2),
    drop("health_kit", 35),
];
const SCRAP_PILE_DROPS: &[LootDropDefinition] = &[drop("scrap_metal", 4), drop("gear", 1)];
const CRYSTAL_CLUSTER_DROPS: &[LootDropDefinition] = &[
    drop("energy_core", 2),
    drop("scrap_metal", 2),
    drop("circuit_board", 1),
];
const BIO_POD_DROPS: &[LootDropDefinition] = &[drop("bio_essence", 3), drop("nano_fiber", 1)];
const GEAR_CACHE_DROPS: &[LootDropDefinition] = &[
    drop("gear", 6),
    drop("circuit_board", 2),
    LootDropDefinition {
        pickup_id: "weapon_part",
        amount: 1,
        weapon_id: Some("rifle"),
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropDefinition {
    pub kind: LegacyPropKind,
    pub max_health: f32,
    pub hit_radius: f32,
    pub visual_top_y: f32,
    pub minimum_damage: f32,
    pub loot_spread: f32,
    pub loot_height: f32,
    pub proximity_claim_radius: Option<f32>,
    pub loot_authority: PropLootAuthority,
    pub respawn: PropRespawnPolicy,
    pub drops: &'static [LootDropDefinition],
}

pub static PROP_DEFINITIONS: &[PropDefinition] = &[
    PropDefinition {
        kind: LegacyPropKind::Crate,
        max_health: 60.0,
        hit_radius: 1.1,
        visual_top_y: 1.0,
        minimum_damage: 1.0,
        loot_spread: 0.9,
        loot_height: 0.4,
        proximity_claim_radius: None,
        loot_authority: PropLootAuthority::WorldDestruction,
        respawn: PropRespawnPolicy::Never,
        drops: CRATE_DROPS,
    },
    PropDefinition {
        kind: LegacyPropKind::Barrel,
        max_health: 40.0,
        hit_radius: 0.9,
        visual_top_y: 1.2,
        minimum_damage: 1.0,
        loot_spread: 1.0,
        loot_height: 0.4,
        proximity_claim_radius: None,
        loot_authority: PropLootAuthority::WorldDestruction,
        respawn: PropRespawnPolicy::Never,
        drops: BARREL_DROPS,
    },
    PropDefinition {
        kind: LegacyPropKind::Canister,
        max_health: 35.0,
        hit_radius: 0.8,
        visual_top_y: 1.2,
        minimum_damage: 1.0,
        loot_spread: 0.8,
        loot_height: 0.4,
        proximity_claim_radius: None,
        loot_authority: PropLootAuthority::WorldDestruction,
        respawn: PropRespawnPolicy::Never,
        drops: CANISTER_DROPS,
    },
    PropDefinition {
        kind: LegacyPropKind::Container,
        max_health: 140.0,
        hit_radius: 1.8,
        visual_top_y: 1.6,
        minimum_damage: 1.0,
        loot_spread: 1.4,
        loot_height: 0.4,
        proximity_claim_radius: None,
        loot_authority: PropLootAuthority::WorldDestruction,
        respawn: PropRespawnPolicy::Never,
        drops: CONTAINER_DROPS,
    },
    PropDefinition {
        kind: LegacyPropKind::HoloSign,
        max_health: 30.0,
        hit_radius: 0.7,
        visual_top_y: 2.4,
        minimum_damage: 1.0,
        loot_spread: 0.7,
        loot_height: 0.4,
        proximity_claim_radius: None,
        loot_authority: PropLootAuthority::WorldDestruction,
        respawn: PropRespawnPolicy::Never,
        drops: HOLO_SIGN_DROPS,
    },
    PropDefinition {
        kind: LegacyPropKind::OpenContainer,
        max_health: 120.0,
        hit_radius: 1.6,
        visual_top_y: 1.0,
        minimum_damage: 1.0,
        loot_spread: 1.0,
        loot_height: 0.4,
        proximity_claim_radius: Some(2.6),
        loot_authority: PropLootAuthority::SharedProximityOrDestruction,
        respawn: PropRespawnPolicy::Never,
        drops: OPEN_CONTAINER_DROPS,
    },
    PropDefinition {
        kind: LegacyPropKind::ScrapPile,
        max_health: 30.0,
        hit_radius: 1.6,
        visual_top_y: 1.5,
        minimum_damage: 0.0,
        loot_spread: 1.4,
        loot_height: 0.6,
        proximity_claim_radius: None,
        loot_authority: PropLootAuthority::WorldDestruction,
        respawn: PropRespawnPolicy::AfterSeconds(35.0),
        drops: SCRAP_PILE_DROPS,
    },
    PropDefinition {
        kind: LegacyPropKind::CrystalCluster,
        max_health: 50.0,
        hit_radius: 1.8,
        visual_top_y: 1.5,
        minimum_damage: 0.0,
        loot_spread: 1.4,
        loot_height: 0.6,
        proximity_claim_radius: None,
        loot_authority: PropLootAuthority::WorldDestruction,
        respawn: PropRespawnPolicy::AfterSeconds(50.0),
        drops: CRYSTAL_CLUSTER_DROPS,
    },
    PropDefinition {
        kind: LegacyPropKind::BioPod,
        max_health: 40.0,
        hit_radius: 1.6,
        visual_top_y: 1.5,
        minimum_damage: 0.0,
        loot_spread: 1.4,
        loot_height: 0.6,
        proximity_claim_radius: None,
        loot_authority: PropLootAuthority::WorldDestruction,
        respawn: PropRespawnPolicy::AfterSeconds(45.0),
        drops: BIO_POD_DROPS,
    },
    PropDefinition {
        kind: LegacyPropKind::GearCache,
        max_health: 70.0,
        hit_radius: 2.0,
        visual_top_y: 1.5,
        minimum_damage: 0.0,
        loot_spread: 1.4,
        loot_height: 0.6,
        proximity_claim_radius: None,
        loot_authority: PropLootAuthority::WorldDestruction,
        respawn: PropRespawnPolicy::AfterSeconds(60.0),
        drops: GEAR_CACHE_DROPS,
    },
];

pub fn prop_definition(kind: LegacyPropKind) -> &'static PropDefinition {
    &PROP_DEFINITIONS[LegacyPropKind::ALL
        .iter()
        .position(|candidate| *candidate == kind)
        .expect("every LegacyPropKind must have a definition")]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LootGrant {
    pub pickup_id: String,
    pub amount: u32,
    pub weapon_id: Option<String>,
}

impl From<LootDropDefinition> for LootGrant {
    fn from(value: LootDropDefinition) -> Self {
        Self {
            pickup_id: value.pickup_id.to_owned(),
            amount: value.amount,
            weapon_id: value.weapon_id.map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LootSpawn {
    pub origin: WorldPoint,
    pub spread: f32,
    pub grants: Vec<LootGrant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropDamageStage {
    Pristine,
    Damaged,
    HeavilyDamaged,
    Destroyed,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropLifecycle {
    Active,
    Destroyed {
        /// Save-relative cooldown; `None` means permanent destruction.
        respawn_remaining_seconds: Option<f32>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropState {
    pub schema_version: u16,
    pub stable_id: String,
    pub kind: LegacyPropKind,
    pub region: LegacyRegionId,
    pub position: WorldPoint,
    pub health: f32,
    pub damage_stage: PropDamageStage,
    pub lifecycle: PropLifecycle,
    /// Empty until claimed. Shared caches mark only the triggering player for
    /// attribution; `cache_has_been_claimed` is the authoritative world check.
    pub claimed_by: PlayerVisibility,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropDamageOutcome {
    pub stable_id: String,
    pub applied_damage: f32,
    pub previous_stage: PropDamageStage,
    pub current_stage: PropDamageStage,
    pub health_remaining: f32,
    pub destroyed: bool,
    pub loot: Option<LootSpawn>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CacheClaimOutcome {
    Claimed(LootSpawn),
    AlreadyClaimed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PropLifecycleEvent {
    Respawned { stable_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropStateError {
    UnsupportedSchemaVersion(u16),
    EmptyPropId,
    NonFinitePosition,
    InvalidDamage,
    InvalidDelta,
    AlreadyDestroyed,
    NotAProximityCache,
    CacheUnavailable,
    EnvironmentPropBudgetExceeded,
    DuplicatePropId(String),
    UnknownProp(String),
    MalformedRecord(&'static str),
}

impl PropState {
    pub fn new(
        stable_id: impl Into<String>,
        kind: LegacyPropKind,
        region: LegacyRegionId,
        position: WorldPoint,
    ) -> Result<Self, PropStateError> {
        let stable_id = stable_id.into();
        if stable_id.trim().is_empty() {
            return Err(PropStateError::EmptyPropId);
        }
        if !position.is_finite() {
            return Err(PropStateError::NonFinitePosition);
        }
        Ok(Self {
            schema_version: HEAVY_REGIONS_SCHEMA_VERSION,
            stable_id,
            kind,
            region,
            position,
            health: prop_definition(kind).max_health,
            damage_stage: PropDamageStage::Pristine,
            lifecycle: PropLifecycle::Active,
            claimed_by: PlayerVisibility::NONE,
        })
    }

    pub fn validate(&self) -> Result<(), PropStateError> {
        if self.schema_version != HEAVY_REGIONS_SCHEMA_VERSION {
            return Err(PropStateError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if self.stable_id.trim().is_empty() {
            return Err(PropStateError::EmptyPropId);
        }
        if !self.position.is_finite() || !self.health.is_finite() {
            return Err(PropStateError::NonFinitePosition);
        }
        let definition = prop_definition(self.kind);
        if !self.claimed_by.is_valid() {
            return Err(PropStateError::MalformedRecord("prop claim visibility"));
        }
        if definition.loot_authority == PropLootAuthority::WorldDestruction
            && !self.claimed_by.is_empty()
        {
            return Err(PropStateError::MalformedRecord(
                "non-cache prop claim attribution",
            ));
        }
        match self.lifecycle {
            PropLifecycle::Active => {
                if self.health <= 0.0 || self.health > definition.max_health {
                    return Err(PropStateError::MalformedRecord("active prop health"));
                }
                if self.damage_stage != damage_stage(self.health, definition.max_health) {
                    return Err(PropStateError::MalformedRecord("active prop damage stage"));
                }
            }
            PropLifecycle::Destroyed {
                respawn_remaining_seconds,
            } => {
                if self.health != 0.0 || self.damage_stage != PropDamageStage::Destroyed {
                    return Err(PropStateError::MalformedRecord("destroyed prop state"));
                }
                match (definition.respawn, respawn_remaining_seconds) {
                    (PropRespawnPolicy::Never, None) => {}
                    (PropRespawnPolicy::AfterSeconds(_), Some(remaining))
                        if remaining.is_finite() && remaining >= 0.0 => {}
                    _ => return Err(PropStateError::MalformedRecord("prop respawn policy")),
                }
            }
        }
        Ok(())
    }

    pub fn cache_has_been_claimed(&self) -> bool {
        !self.claimed_by.is_empty()
    }

    pub fn damage(&mut self, amount: f32) -> Result<PropDamageOutcome, PropStateError> {
        if !amount.is_finite() || amount <= 0.0 {
            return Err(PropStateError::InvalidDamage);
        }
        if !matches!(self.lifecycle, PropLifecycle::Active) {
            return Err(PropStateError::AlreadyDestroyed);
        }

        let definition = prop_definition(self.kind);
        let applied_damage = amount.max(definition.minimum_damage).min(self.health);
        let previous_stage = self.damage_stage;
        self.health = (self.health - applied_damage).max(0.0);
        let destroyed = self.health <= 0.0;
        let loot = if destroyed {
            self.damage_stage = PropDamageStage::Destroyed;
            self.lifecycle = PropLifecycle::Destroyed {
                respawn_remaining_seconds: match definition.respawn {
                    PropRespawnPolicy::Never => None,
                    PropRespawnPolicy::AfterSeconds(seconds) => Some(seconds),
                },
            };
            let may_drop = definition.loot_authority == PropLootAuthority::WorldDestruction
                || !self.cache_has_been_claimed();
            if may_drop {
                // A destroyed shared cache has now paid out to the world.
                if definition.loot_authority == PropLootAuthority::SharedProximityOrDestruction {
                    self.claimed_by = PlayerVisibility::ALL;
                }
                Some(self.loot_spawn(definition.loot_spread))
            } else {
                None
            }
        } else {
            self.damage_stage = damage_stage(self.health, definition.max_health);
            None
        };

        Ok(PropDamageOutcome {
            stable_id: self.stable_id.clone(),
            applied_damage,
            previous_stage,
            current_stage: self.damage_stage,
            health_remaining: self.health,
            destroyed,
            loot,
        })
    }

    pub fn claim_cache(&mut self, player: PlayerSlot) -> Result<CacheClaimOutcome, PropStateError> {
        let definition = prop_definition(self.kind);
        if definition.loot_authority != PropLootAuthority::SharedProximityOrDestruction {
            return Err(PropStateError::NotAProximityCache);
        }
        if !matches!(self.lifecycle, PropLifecycle::Active) {
            return Err(PropStateError::CacheUnavailable);
        }
        if self.cache_has_been_claimed() {
            return Ok(CacheClaimOutcome::AlreadyClaimed);
        }
        self.claimed_by.insert(player);
        Ok(CacheClaimOutcome::Claimed(self.loot_spawn(0.0)))
    }

    /// Advances a save-relative mining cooldown. Returns true exactly once
    /// when the node becomes active again.
    pub fn tick(&mut self, delta_seconds: f32) -> Result<bool, PropStateError> {
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(PropStateError::InvalidDelta);
        }
        let PropLifecycle::Destroyed {
            respawn_remaining_seconds: Some(remaining),
        } = &mut self.lifecycle
        else {
            return Ok(false);
        };
        *remaining = (*remaining - delta_seconds).max(0.0);
        if *remaining > 0.0 {
            return Ok(false);
        }

        let definition = prop_definition(self.kind);
        self.health = definition.max_health;
        self.damage_stage = PropDamageStage::Pristine;
        self.lifecycle = PropLifecycle::Active;
        self.claimed_by = PlayerVisibility::NONE;
        Ok(true)
    }

    fn loot_spawn(&self, spread: f32) -> LootSpawn {
        let definition = prop_definition(self.kind);
        LootSpawn {
            origin: WorldPoint::new(
                self.position.x,
                self.position.y + definition.loot_height,
                self.position.z,
            ),
            spread,
            grants: definition.drops.iter().copied().map(Into::into).collect(),
        }
    }
}

fn damage_stage(health: f32, max_health: f32) -> PropDamageStage {
    let fraction = health / max_health;
    if fraction < 0.3 {
        PropDamageStage::HeavilyDamaged
    } else if fraction < 0.6 {
        PropDamageStage::Damaged
    } else {
        PropDamageStage::Pristine
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PropStateRegistry {
    props: BTreeMap<String, PropState>,
}

impl PropStateRegistry {
    pub fn len(&self) -> usize {
        self.props.len()
    }

    pub fn is_empty(&self) -> bool {
        self.props.is_empty()
    }

    pub fn validate(&self) -> Result<(), PropStateError> {
        let environment_count = self
            .props
            .values()
            .filter(|prop| !prop.kind.is_mining_node())
            .count();
        if environment_count > MAX_ENVIRONMENT_PROPS {
            return Err(PropStateError::EnvironmentPropBudgetExceeded);
        }
        for (stable_id, prop) in &self.props {
            if stable_id != &prop.stable_id {
                return Err(PropStateError::MalformedRecord("prop key mismatch"));
            }
            prop.validate()?;
        }
        Ok(())
    }

    pub fn get(&self, stable_id: &str) -> Option<&PropState> {
        self.props.get(stable_id)
    }

    pub fn insert(&mut self, prop: PropState) -> Result<(), PropStateError> {
        prop.validate()?;
        if self.props.contains_key(&prop.stable_id) {
            return Err(PropStateError::DuplicatePropId(prop.stable_id));
        }
        if !prop.kind.is_mining_node()
            && self
                .props
                .values()
                .filter(|existing| !existing.kind.is_mining_node())
                .count()
                >= MAX_ENVIRONMENT_PROPS
        {
            return Err(PropStateError::EnvironmentPropBudgetExceeded);
        }
        self.props.insert(prop.stable_id.clone(), prop);
        Ok(())
    }

    pub fn damage(
        &mut self,
        stable_id: &str,
        amount: f32,
    ) -> Result<PropDamageOutcome, PropStateError> {
        self.props
            .get_mut(stable_id)
            .ok_or_else(|| PropStateError::UnknownProp(stable_id.to_owned()))?
            .damage(amount)
    }

    pub fn tick(&mut self, delta_seconds: f32) -> Result<Vec<PropLifecycleEvent>, PropStateError> {
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(PropStateError::InvalidDelta);
        }
        let mut events = Vec::new();
        // BTreeMap traversal makes simultaneous respawns deterministic.
        for prop in self.props.values_mut() {
            if prop.tick(delta_seconds)? {
                events.push(PropLifecycleEvent::Respawned {
                    stable_id: prop.stable_id.clone(),
                });
            }
        }
        Ok(events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaMode {
    FreeForAll,
    Teams { team_count: u8 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArenaRules {
    pub mode: ArenaMode,
    pub score_limit: u16,
    pub time_limit_seconds: f32,
    pub max_participants: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaParticipant {
    pub slot: PlayerSlot,
    pub team: Option<u8>,
    pub score: u16,
    pub deaths: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaLeader {
    Player(PlayerSlot),
    Team(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaOutcome {
    PlayerWon(PlayerSlot),
    TeamWon(u8),
    Draw { leaders: Vec<ArenaLeader> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaPhase {
    Waiting,
    Running,
    Finished(ArenaOutcome),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineArenaState {
    pub schema_version: u16,
    pub rules: ArenaRules,
    pub phase: ArenaPhase,
    pub elapsed_seconds: f32,
    pub participants: BTreeMap<PlayerSlot, ArenaParticipant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArenaError {
    UnsupportedSchemaVersion(u16),
    ZeroScoreLimit,
    InvalidTimeLimit,
    InvalidParticipantLimit,
    InvalidTeamCount,
    SlotOutsideArena(u8),
    ArenaFull,
    DuplicateParticipant(PlayerSlot),
    UnknownParticipant(PlayerSlot),
    TeamRequired,
    TeamNotAllowed,
    TeamOutOfRange(u8),
    WrongPhase,
    NeedAtLeastTwoParticipants,
    NeedAtLeastTwoTeams,
    MalformedRecord(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArenaEliminationOutcome {
    pub attacker: Option<PlayerSlot>,
    pub victim: PlayerSlot,
    pub point_awarded: bool,
    pub phase: ArenaPhase,
}

impl ArenaRules {
    /// Heavy Water supplies arena geometry and 24 spawn slots, but no score or
    /// timer constants. Callers must therefore choose these rules explicitly.
    pub fn new(
        mode: ArenaMode,
        score_limit: u16,
        time_limit_seconds: f32,
        max_participants: u8,
    ) -> Result<Self, ArenaError> {
        let rules = Self {
            mode,
            score_limit,
            time_limit_seconds,
            max_participants,
        };
        rules.validate()?;
        Ok(rules)
    }

    pub fn validate(&self) -> Result<(), ArenaError> {
        if self.score_limit == 0 {
            return Err(ArenaError::ZeroScoreLimit);
        }
        if !self.time_limit_seconds.is_finite() || self.time_limit_seconds <= 0.0 {
            return Err(ArenaError::InvalidTimeLimit);
        }
        if self.max_participants < 2 || self.max_participants > VERSUS_ARENA_SPAWN_COUNT {
            return Err(ArenaError::InvalidParticipantLimit);
        }
        if let ArenaMode::Teams { team_count } = self.mode {
            if team_count < 2 || team_count > self.max_participants {
                return Err(ArenaError::InvalidTeamCount);
            }
        }
        Ok(())
    }
}

impl OfflineArenaState {
    pub fn new(rules: ArenaRules) -> Result<Self, ArenaError> {
        rules.validate()?;
        Ok(Self {
            schema_version: HEAVY_REGIONS_SCHEMA_VERSION,
            rules,
            phase: ArenaPhase::Waiting,
            elapsed_seconds: 0.0,
            participants: BTreeMap::new(),
        })
    }

    pub fn validate(&self) -> Result<(), ArenaError> {
        if self.schema_version != HEAVY_REGIONS_SCHEMA_VERSION {
            return Err(ArenaError::UnsupportedSchemaVersion(self.schema_version));
        }
        self.rules.validate()?;
        if !self.elapsed_seconds.is_finite()
            || self.elapsed_seconds < 0.0
            || self.elapsed_seconds > self.rules.time_limit_seconds
        {
            return Err(ArenaError::MalformedRecord("arena elapsed time"));
        }
        if self.participants.len() > usize::from(self.rules.max_participants) {
            return Err(ArenaError::MalformedRecord("arena participant count"));
        }
        for (key, participant) in &self.participants {
            if *key != participant.slot || key.0 >= VERSUS_ARENA_SPAWN_COUNT {
                return Err(ArenaError::MalformedRecord("arena participant slot"));
            }
            match self.rules.mode {
                ArenaMode::FreeForAll if participant.team.is_some() => {
                    return Err(ArenaError::TeamNotAllowed)
                }
                ArenaMode::FreeForAll => {}
                ArenaMode::Teams { team_count } => match participant.team {
                    Some(team) if team < team_count => {}
                    Some(team) => return Err(ArenaError::TeamOutOfRange(team)),
                    None => return Err(ArenaError::TeamRequired),
                },
            }
        }
        if self.phase != ArenaPhase::Waiting {
            if self.participants.len() < 2 {
                return Err(ArenaError::NeedAtLeastTwoParticipants);
            }
            if matches!(self.rules.mode, ArenaMode::Teams { .. }) {
                let represented: BTreeSet<_> = self
                    .participants
                    .values()
                    .filter_map(|participant| participant.team)
                    .collect();
                if represented.len() < 2 {
                    return Err(ArenaError::NeedAtLeastTwoTeams);
                }
            }
        }
        Ok(())
    }

    pub fn join(&mut self, slot: PlayerSlot, team: Option<u8>) -> Result<(), ArenaError> {
        self.validate()?;
        if self.phase != ArenaPhase::Waiting {
            return Err(ArenaError::WrongPhase);
        }
        if slot.0 >= VERSUS_ARENA_SPAWN_COUNT {
            return Err(ArenaError::SlotOutsideArena(slot.0));
        }
        if self.participants.len() >= usize::from(self.rules.max_participants) {
            return Err(ArenaError::ArenaFull);
        }
        if self.participants.contains_key(&slot) {
            return Err(ArenaError::DuplicateParticipant(slot));
        }
        match self.rules.mode {
            ArenaMode::FreeForAll if team.is_some() => return Err(ArenaError::TeamNotAllowed),
            ArenaMode::FreeForAll => {}
            ArenaMode::Teams { team_count } => {
                let team = team.ok_or(ArenaError::TeamRequired)?;
                if team >= team_count {
                    return Err(ArenaError::TeamOutOfRange(team));
                }
            }
        }
        self.participants.insert(
            slot,
            ArenaParticipant {
                slot,
                team,
                score: 0,
                deaths: 0,
            },
        );
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), ArenaError> {
        self.validate()?;
        if self.phase != ArenaPhase::Waiting {
            return Err(ArenaError::WrongPhase);
        }
        if self.participants.len() < 2 {
            return Err(ArenaError::NeedAtLeastTwoParticipants);
        }
        if matches!(self.rules.mode, ArenaMode::Teams { .. }) {
            let represented: BTreeSet<_> = self
                .participants
                .values()
                .filter_map(|participant| participant.team)
                .collect();
            if represented.len() < 2 {
                return Err(ArenaError::NeedAtLeastTwoTeams);
            }
        }
        self.elapsed_seconds = 0.0;
        self.phase = ArenaPhase::Running;
        Ok(())
    }

    pub fn remaining_seconds(&self) -> f32 {
        (self.rules.time_limit_seconds - self.elapsed_seconds).max(0.0)
    }

    pub fn team_score(&self, team: u8) -> u32 {
        self.participants
            .values()
            .filter(|participant| participant.team == Some(team))
            .map(|participant| u32::from(participant.score))
            .sum()
    }

    pub fn record_elimination(
        &mut self,
        attacker: Option<PlayerSlot>,
        victim: PlayerSlot,
    ) -> Result<ArenaEliminationOutcome, ArenaError> {
        self.validate()?;
        if self.phase != ArenaPhase::Running {
            return Err(ArenaError::WrongPhase);
        }
        if !self.participants.contains_key(&victim) {
            return Err(ArenaError::UnknownParticipant(victim));
        }
        if let Some(attacker) = attacker {
            if !self.participants.contains_key(&attacker) {
                return Err(ArenaError::UnknownParticipant(attacker));
            }
        }

        let victim_record = self
            .participants
            .get_mut(&victim)
            .ok_or(ArenaError::UnknownParticipant(victim))?;
        victim_record.deaths = victim_record.deaths.saturating_add(1);

        let point_awarded = if let Some(attacker) = attacker {
            let attacker_team = self
                .participants
                .get(&attacker)
                .ok_or(ArenaError::UnknownParticipant(attacker))?
                .team;
            let victim_team = self
                .participants
                .get(&victim)
                .ok_or(ArenaError::UnknownParticipant(victim))?
                .team;
            let friendly_fire =
                matches!(self.rules.mode, ArenaMode::Teams { .. }) && attacker_team == victim_team;
            if attacker != victim && !friendly_fire {
                let scorer = self
                    .participants
                    .get_mut(&attacker)
                    .ok_or(ArenaError::UnknownParticipant(attacker))?;
                scorer.score = scorer.score.saturating_add(1);
                true
            } else {
                false
            }
        } else {
            false
        };

        if point_awarded {
            let scorer = attacker.ok_or(ArenaError::MalformedRecord(
                "awarded arena point without attacker",
            ))?;
            self.finish_if_score_limit_reached(scorer)?;
        }
        Ok(ArenaEliminationOutcome {
            attacker,
            victim,
            point_awarded,
            phase: self.phase.clone(),
        })
    }

    pub fn tick(&mut self, delta_seconds: f32) -> Result<&ArenaPhase, ArenaError> {
        self.validate()?;
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(ArenaError::InvalidTimeLimit);
        }
        if self.phase != ArenaPhase::Running {
            return Err(ArenaError::WrongPhase);
        }
        self.elapsed_seconds =
            (self.elapsed_seconds + delta_seconds).min(self.rules.time_limit_seconds);
        if self.elapsed_seconds >= self.rules.time_limit_seconds {
            self.phase = ArenaPhase::Finished(self.outcome_at_time_limit()?);
        }
        Ok(&self.phase)
    }

    fn finish_if_score_limit_reached(&mut self, scorer: PlayerSlot) -> Result<(), ArenaError> {
        let outcome = match self.rules.mode {
            ArenaMode::FreeForAll => {
                let score = self
                    .participants
                    .get(&scorer)
                    .ok_or(ArenaError::UnknownParticipant(scorer))?
                    .score;
                (score >= self.rules.score_limit).then_some(ArenaOutcome::PlayerWon(scorer))
            }
            ArenaMode::Teams { .. } => {
                let team = self
                    .participants
                    .get(&scorer)
                    .ok_or(ArenaError::UnknownParticipant(scorer))?
                    .team
                    .ok_or(ArenaError::TeamRequired)?;
                (self.team_score(team) >= u32::from(self.rules.score_limit))
                    .then_some(ArenaOutcome::TeamWon(team))
            }
        };
        if let Some(outcome) = outcome {
            self.phase = ArenaPhase::Finished(outcome);
        }
        Ok(())
    }

    fn outcome_at_time_limit(&self) -> Result<ArenaOutcome, ArenaError> {
        match self.rules.mode {
            ArenaMode::FreeForAll => {
                let highest = self
                    .participants
                    .values()
                    .map(|participant| participant.score)
                    .max()
                    .ok_or(ArenaError::NeedAtLeastTwoParticipants)?;
                let leaders: Vec<_> = self
                    .participants
                    .values()
                    .filter(|participant| participant.score == highest)
                    .map(|participant| ArenaLeader::Player(participant.slot))
                    .collect();
                if let [ArenaLeader::Player(winner)] = leaders.as_slice() {
                    Ok(ArenaOutcome::PlayerWon(*winner))
                } else {
                    Ok(ArenaOutcome::Draw { leaders })
                }
            }
            ArenaMode::Teams { team_count } => {
                let represented: BTreeSet<_> = self
                    .participants
                    .values()
                    .filter_map(|participant| participant.team)
                    .collect();
                let scores: Vec<_> = (0..team_count)
                    .filter(|team| represented.contains(team))
                    .map(|team| (team, self.team_score(team)))
                    .collect();
                let highest = scores
                    .iter()
                    .map(|(_, score)| *score)
                    .max()
                    .ok_or(ArenaError::NeedAtLeastTwoTeams)?;
                let leaders: Vec<_> = scores
                    .into_iter()
                    .filter(|(_, score)| *score == highest)
                    .map(|(team, _)| ArenaLeader::Team(team))
                    .collect();
                if let [ArenaLeader::Team(winner)] = leaders.as_slice() {
                    Ok(ArenaOutcome::TeamWon(*winner))
                } else {
                    Ok(ArenaOutcome::Draw { leaders })
                }
            }
        }
    }
}

/// Returns the source arena's deterministic central-ring spawn for a slot.
/// Slots wrap modulo 24, matching `VersusArena.getSpawnPoint`.
pub fn versus_arena_spawn(slot: u32) -> WorldPoint {
    let index = slot % u32::from(VERSUS_ARENA_SPAWN_COUNT);
    let angle = index as f32 / f32::from(VERSUS_ARENA_SPAWN_COUNT) * TAU;
    WorldPoint::new(
        angle.cos() * VERSUS_ARENA_SPAWN_RADIUS,
        2.0,
        angle.sin() * VERSUS_ARENA_SPAWN_RADIUS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    fn slot(index: u8) -> PlayerSlot {
        PlayerSlot::new(index).expect("test slot should be valid")
    }

    fn marker(stable_id: &str, kind: MapMarkerKind, visible_to: PlayerVisibility) -> MapMarker {
        MapMarker {
            stable_id: stable_id.to_owned(),
            kind,
            region: LegacyRegionId::DetroitStarCityFront,
            position: WorldPoint::ZERO,
            label: None,
            status: MapMarkerStatus::Active,
            visible_to,
            offscreen_arrow: kind.sourced_offscreen_policy(),
        }
    }

    #[test]
    fn region_catalog_is_complete_stable_and_level_ordered() {
        assert_eq!(REGION_DEFINITIONS.len(), HEAVY_WORLD_LEVEL_COUNT);
        let mut stable_ids = BTreeSet::new();
        let mut anchor_ids = BTreeSet::new();
        for (index, expected_id) in LegacyRegionId::ALL.iter().enumerate() {
            let definition = &REGION_DEFINITIONS[index];
            assert_eq!(definition.id, *expected_id);
            assert_eq!(definition.id.legacy_level(), index as u8 + 1);
            assert_eq!(
                LegacyRegionId::from_legacy_level(index as u8 + 1),
                Some(*expected_id)
            );
            assert!(stable_ids.insert(definition.id.stable_id()));
            assert!(anchor_ids.insert(definition.entry_anchor.stable_id));
            assert!(definition.entry_anchor.position.is_finite());
        }
        assert_eq!(LegacyRegionId::from_legacy_level(0), None);
        assert_eq!(LegacyRegionId::from_legacy_level(12), None);
    }

    #[test]
    fn sourced_region_content_keeps_high_value_constants() {
        let orbital = region_definition(LegacyRegionId::OrbitalFront);
        assert_eq!(orbital.entry_anchor.position.y, 300.0);
        assert!(matches!(
            orbital.content,
            RegionContentProfile::OrbitalFront {
                asteroid_count: 64,
                asteroid_radius: 260.0,
                spawn_altitude: 300.0,
                earth_distance: 1_200.0,
                forced_cruise_speed: 28.0,
            }
        ));

        let lair = region_definition(LegacyRegionId::SwarmsLair);
        assert!(matches!(
            lair.content,
            RegionContentProfile::SwarmsLair {
                general_z_offset: 28.0,
                general_kill_radius: 200.0,
                swarm_minion_count: 10,
                ..
            }
        ));

        let zug = region_definition(LegacyRegionId::ZugIslandLegion);
        assert!(matches!(
            zug.content,
            RegionContentProfile::ZugIsland {
                live_target: 60,
                lifetime_cap: 600,
                titan_spawn_weight: 55,
                captain_spawn_weight: 30,
                spider_tank_spawn_weight: 15,
                ..
            }
        ));

        let wilds = region_definition(LegacyRegionId::MichiganWilds);
        assert!(matches!(
            wilds.content,
            RegionContentProfile::MichiganWilds {
                heightmap_pixel_width: 1_448,
                heightmap_pixel_height: 1_086,
                terrain_width: 7_200.0,
                terrain_depth: 5_400.0,
                subdivisions: 320,
                ..
            }
        ));
        assert_eq!(MICHIGAN_WARP_DEFINITIONS.len(), 10);
        assert_eq!(MICHIGAN_WARP_DEFINITIONS[0].stable_id, "west-giant-base");
        assert_eq!(MICHIGAN_WARP_DEFINITIONS[9].stable_id, "thumb-coast-lab");
    }

    #[test]
    fn campaign_successors_do_not_absorb_side_zones() {
        assert_eq!(
            LegacyRegionId::DetroitStarCityFront.campaign_successor(),
            Some(LegacyRegionId::DetroitHoldTheLine)
        );
        assert_eq!(
            LegacyRegionId::DetroitHoldTheLine.campaign_successor(),
            Some(LegacyRegionId::DetroitPurgeTheVoid)
        );
        assert_eq!(
            LegacyRegionId::DetroitPurgeTheVoid.campaign_successor(),
            None
        );
        assert_eq!(LegacyRegionId::AshurSanctuary.campaign_successor(), None);
    }

    #[test]
    fn loaded_campaign_infers_only_prior_detroit_clears() {
        let campaign = RegionSession::from_legacy_snapshot(3);
        assert_eq!(campaign.active.region, LegacyRegionId::DetroitPurgeTheVoid);
        assert!(campaign
            .record(LegacyRegionId::DetroitStarCityFront)
            .unwrap()
            .is_cleared());
        assert!(campaign
            .record(LegacyRegionId::DetroitHoldTheLine)
            .unwrap()
            .is_cleared());
        campaign.validate().unwrap();

        let side_zone = RegionSession::from_legacy_snapshot(9);
        assert_eq!(side_zone.active.region, LegacyRegionId::ZugIslandLegion);
        assert!(side_zone
            .record(LegacyRegionId::DetroitStarCityFront)
            .is_none());
        assert_eq!(
            RegionSession::from_legacy_snapshot(99).active.region,
            LegacyRegionId::MichiganWilds
        );
        assert_eq!(
            RegionSession::from_legacy_snapshot(-4).active.region,
            LegacyRegionId::DetroitStarCityFront
        );
    }

    #[test]
    fn atmosphere_matches_source_phase_weather_and_space_rules() {
        assert_eq!(sky_phase(4.99), SkyPhase::Night);
        assert_eq!(sky_phase(5.0), SkyPhase::Dawn);
        assert_eq!(sky_phase(7.0), SkyPhase::Morning);
        assert_eq!(sky_phase(9.0), SkyPhase::Day);
        assert_eq!(sky_phase(17.0), SkyPhase::Dusk);
        assert_eq!(sky_phase(19.0), SkyPhase::Evening);
        assert_eq!(sky_phase(21.0), SkyPhase::Night);
        assert_eq!(sky_phase(-1.0), SkyPhase::Night);
        assert_close(normalize_hour(-1.0), 23.0);

        let mut ground = region_definition(LegacyRegionId::DetroitStarCityFront).atmosphere;
        assert_close(ground.advance_hour(23.8, 5.0), 0.2);
        ground.weather = Weather::Storm;
        assert_close(ground.fog_density(), 0.004);
        assert_close(ground.star_factor(23.0), 0.0);
        let storm_sample = ground.sample(12.0);
        assert_eq!(storm_sample.phase, SkyPhase::Day);
        // Detroit L1 has a neutral tint; storm fog is 30% gray blend.
        assert_close(storm_sample.palette.zenith.r, DAY_SKY_PALETTE.zenith.r);
        assert_close(storm_sample.fog_color.r, 0.505);
        assert_close(storm_sample.fog_color.g, 0.61);
        assert_close(storm_sample.fog_color.b, 0.765);

        let space = region_definition(LegacyRegionId::OrbitalFront).atmosphere;
        assert_eq!(space.phase_at(12.0), SkyPhase::DeepSpace);
        assert_close(space.fog_density(), 0.0001);
        assert_close(space.star_factor(12.0), 1.5);
        assert_eq!(space.sample(12.0).palette, DEEP_SPACE_SKY_PALETTE);

        let red_detroit = region_definition(LegacyRegionId::DetroitHoldTheLine)
            .atmosphere
            .sample(9.0);
        // Multiplicative level tint clamps authored palette channels to 1.
        assert_close(red_detroit.palette.zenith.r, 0.3125);
        assert_close(red_detroit.palette.horizon.r, 0.875);
        assert_close(red_detroit.palette.sun_disc.r, 1.0);
    }

    #[test]
    fn region_session_preserves_return_anchor_across_custom_zone_hops() {
        let home = WorldPoint::new(23.0, 4.0, -17.0);
        let mut session = RegionSession::new(LegacyRegionId::DetroitStarCityFront);
        let enter_ashur = session
            .travel_to(LegacyRegionId::AshurSanctuary, home)
            .unwrap();
        assert_eq!(enter_ashur.action, RegionTravelAction::Enter);
        assert!(enter_ashur.dispose_previous_before_mount);
        assert!(enter_ashur.clear_transient_combat);
        assert!(enter_ashur.hides_shared_world);
        assert_eq!(enter_ashur.return_anchor.as_ref().unwrap().position, home);
        session.finish_mount().unwrap();

        let enter_pontiac = session
            .travel_to(
                LegacyRegionId::PontiacSecretLab,
                WorldPoint::new(-480.0, 0.0, -480.0),
            )
            .unwrap();
        assert_eq!(enter_pontiac.return_anchor.as_ref().unwrap().position, home);
        session.finish_mount().unwrap();

        let return_plan = session.begin_return().unwrap();
        assert_eq!(return_plan.action, RegionTravelAction::Return);
        assert_eq!(return_plan.to, LegacyRegionId::DetroitStarCityFront);
        assert_eq!(return_plan.arrival, home);
        assert_eq!(
            session
                .record(LegacyRegionId::AshurSanctuary)
                .unwrap()
                .visits,
            1
        );
        assert!(!session
            .record(LegacyRegionId::DetroitStarCityFront)
            .unwrap()
            .is_cleared());
    }

    #[test]
    fn same_region_reassert_does_not_count_as_a_new_visit() {
        let mut session = RegionSession::new(LegacyRegionId::SaginawUnderwaterLab);
        let generation = session.active.generation;
        let plan = session
            .travel_to(LegacyRegionId::SaginawUnderwaterLab, WorldPoint::ZERO)
            .unwrap();
        assert_eq!(plan.action, RegionTravelAction::Reassert);
        assert_eq!(plan.generation, generation);
        assert!(!plan.clear_transient_combat);
        assert!(!plan.dispose_previous_before_mount);
        assert_eq!(
            session
                .record(LegacyRegionId::SaginawUnderwaterLab)
                .unwrap()
                .visits,
            1
        );
    }

    #[test]
    fn ashur_to_detroit_route_gate_is_preserved() {
        assert!(!authored_fast_travel_allowed(
            LegacyRegionId::AshurSanctuary,
            LegacyRegionId::DetroitStarCityFront
        ));
        assert!(authored_fast_travel_allowed(
            LegacyRegionId::AshurSanctuary,
            LegacyRegionId::PontiacSecretLab
        ));
        let mut session = RegionSession::new(LegacyRegionId::AshurSanctuary);
        assert!(matches!(
            session.travel_to(LegacyRegionId::DetroitPurgeTheVoid, WorldPoint::ZERO),
            Err(RegionSessionError::AuthoredRouteBlocked { .. })
        ));
    }

    #[test]
    fn persistent_flags_are_typed_and_region_scoped() {
        let mut session = RegionSession::new(LegacyRegionId::PontiacSecretLab);
        assert!(session
            .set_persistent_flag(
                LegacyRegionId::PontiacSecretLab,
                RegionPersistentFlag::PontiacAnimalRescued(PontiacRescueId::Glim),
            )
            .unwrap());
        assert!(!session
            .set_persistent_flag(
                LegacyRegionId::PontiacSecretLab,
                RegionPersistentFlag::PontiacAnimalRescued(PontiacRescueId::Glim),
            )
            .unwrap());
        assert!(matches!(
            session.set_persistent_flag(
                LegacyRegionId::AshurSanctuary,
                RegionPersistentFlag::SwarmGeneralDefeated,
            ),
            Err(RegionSessionError::FlagDoesNotBelong { .. })
        ));
    }

    #[test]
    fn region_session_round_trips_through_save_json() {
        let mut session = RegionSession::new(LegacyRegionId::SwarmsLair);
        session
            .set_persistent_flag(
                LegacyRegionId::SwarmsLair,
                RegionPersistentFlag::SwarmGeneralDefeated,
            )
            .unwrap();
        let encoded = serde_json::to_string(&session).unwrap();
        let decoded: RegionSession = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, session);
        decoded.validate().unwrap();
    }

    #[test]
    fn marker_registry_filters_per_player_and_orders_deterministically() {
        let p0 = slot(0);
        let p1 = slot(1);
        let mut both = PlayerVisibility::only(p0);
        both.insert(p1);
        let mut registry = MapMarkerRegistry::default();
        registry
            .upsert(marker("enemy-z", MapMarkerKind::Enemy, both))
            .unwrap();
        registry
            .upsert(marker("enemy-a", MapMarkerKind::Enemy, both))
            .unwrap();
        registry
            .upsert(marker(
                "objective",
                MapMarkerKind::Objective,
                PlayerVisibility::only(p0),
            ))
            .unwrap();
        registry
            .upsert(marker("chest", MapMarkerKind::Chest, both))
            .unwrap();

        let p0_ids: Vec<_> = registry
            .visible_for(p0, LegacyRegionId::DetroitStarCityFront)
            .iter()
            .map(|marker| marker.stable_id.as_str())
            .collect();
        assert_eq!(p0_ids, ["objective", "enemy-a", "enemy-z", "chest"]);

        let p1_ids: Vec<_> = registry
            .visible_for(p1, LegacyRegionId::DetroitStarCityFront)
            .iter()
            .map(|marker| marker.stable_id.as_str())
            .collect();
        assert_eq!(p1_ids, ["enemy-a", "enemy-z", "chest"]);

        registry.set_visible_for("enemy-a", p1, false).unwrap();
        registry
            .set_status("enemy-z", MapMarkerStatus::Inactive)
            .unwrap();
        let p1_ids: Vec<_> = registry
            .visible_for(p1, LegacyRegionId::DetroitStarCityFront)
            .iter()
            .map(|marker| marker.stable_id.as_str())
            .collect();
        assert_eq!(p1_ids, ["chest"]);
    }

    #[test]
    fn sourced_marker_arrow_policies_distinguish_bases_and_bosses() {
        let base = MapMarkerKind::EnemyBase.sourced_offscreen_policy();
        assert!(base.permits(MapMarkerStatus::Active));
        assert!(base.permits(MapMarkerStatus::Cleared));
        let boss = MapMarkerKind::BossFortress.sourced_offscreen_policy();
        assert!(boss.permits(MapMarkerStatus::Active));
        assert!(!boss.permits(MapMarkerStatus::Cleared));
        assert_eq!(
            MapMarkerKind::SupplyCache.sourced_offscreen_policy(),
            OffscreenArrowPolicy::Never
        );
    }

    #[test]
    fn map_projection_and_edge_arrows_match_source_geometry() {
        let viewport = MapViewport::default();
        let center = viewport
            .project(WorldPoint::new(600.0, 99.0, 600.0))
            .unwrap();
        assert_close(center.x, 125.0);
        assert_close(center.y, 125.0);
        assert!(viewport
            .edge_arrow(
                WorldPoint::new(600.0, 0.0, 600.0),
                WorldPoint::new(600.0, 0.0, 600.0),
            )
            .is_none());

        let east = viewport
            .edge_arrow(
                WorldPoint::new(2_000.0, 0.0, 600.0),
                WorldPoint::new(600.0, 0.0, 600.0),
            )
            .unwrap();
        assert_close(east.position.x, 240.0);
        assert_close(east.position.y, 125.0);
        assert_close(east.angle_radians, 0.0);

        // An off-screen player falls back to the canvas center for both axes.
        let west = viewport
            .edge_arrow(
                WorldPoint::new(-2_000.0, 0.0, 600.0),
                WorldPoint::new(2_000.0, 0.0, 2_000.0),
            )
            .unwrap();
        assert_close(west.position.x, 10.0);
        assert_close(west.position.y, 125.0);
    }

    #[test]
    fn map_icon_falloff_keeps_heavy_water_thresholds() {
        assert_eq!(
            map_icon_falloff(80.0),
            MapIconFalloff {
                scale: 1.0,
                alpha: 1.0
            }
        );
        let middle = map_icon_falloff(200.0);
        assert_close(middle.scale, 0.725);
        assert_close(middle.alpha, 0.675);
        assert_eq!(
            map_icon_falloff(320.0),
            MapIconFalloff {
                scale: 0.45,
                alpha: 0.35
            }
        );
    }

    #[test]
    fn prop_catalog_contains_every_environment_and_mining_definition() {
        assert_eq!(PROP_DEFINITIONS.len(), LegacyPropKind::ALL.len());
        for kind in LegacyPropKind::ALL {
            let definition = prop_definition(kind);
            assert_eq!(definition.kind, kind);
            assert!(definition.max_health > 0.0);
            assert!(!definition.drops.is_empty());
        }
        let gear_cache = prop_definition(LegacyPropKind::GearCache);
        assert_eq!(gear_cache.max_health, 70.0);
        assert_eq!(gear_cache.hit_radius, 2.0);
        assert_eq!(gear_cache.drops[2].pickup_id, "weapon_part");
        assert_eq!(gear_cache.drops[2].weapon_id, Some("rifle"));
    }

    #[test]
    fn environmental_props_cross_strict_damage_thresholds_and_drop_once() {
        let mut prop = PropState::new(
            "crate-01",
            LegacyPropKind::Crate,
            LegacyRegionId::DetroitStarCityFront,
            WorldPoint::new(4.0, 0.0, 8.0),
        )
        .unwrap();
        let at_sixty = prop.damage(24.0).unwrap();
        assert_eq!(at_sixty.current_stage, PropDamageStage::Pristine);
        let under_sixty = prop.damage(0.1).unwrap();
        assert_close(under_sixty.applied_damage, 1.0);
        assert_eq!(under_sixty.current_stage, PropDamageStage::Damaged);
        let under_thirty = prop.damage(18.0).unwrap();
        assert_eq!(under_thirty.current_stage, PropDamageStage::HeavilyDamaged);
        let destroyed = prop.damage(100.0).unwrap();
        assert!(destroyed.destroyed);
        let loot = destroyed.loot.unwrap();
        assert_eq!(loot.grants.len(), 2);
        assert_eq!(loot.grants[0].pickup_id, "scrap_metal");
        assert_close(loot.origin.y, 0.4);
        assert_eq!(prop.damage(1.0), Err(PropStateError::AlreadyDestroyed));
        prop.validate().unwrap();
    }

    #[test]
    fn open_container_is_a_single_shared_claim_without_destruction_duplication() {
        let p0 = slot(0);
        let p1 = slot(1);
        let mut cache = PropState::new(
            "supply-cache",
            LegacyPropKind::OpenContainer,
            LegacyRegionId::DetroitStarCityFront,
            WorldPoint::ZERO,
        )
        .unwrap();
        let CacheClaimOutcome::Claimed(loot) = cache.claim_cache(p0).unwrap() else {
            panic!("first cache claim should pay out")
        };
        assert_eq!(loot.spread, 0.0);
        assert_eq!(loot.grants[2].pickup_id, "health_kit");
        assert_eq!(loot.grants[2].amount, 35);
        assert_eq!(
            cache.claim_cache(p1).unwrap(),
            CacheClaimOutcome::AlreadyClaimed
        );
        assert!(cache.damage(500.0).unwrap().loot.is_none());
    }

    #[test]
    fn mining_nodes_respawn_from_save_relative_cooldowns() {
        let mut crystal = PropState::new(
            "crystal-01",
            LegacyPropKind::CrystalCluster,
            LegacyRegionId::MichiganWilds,
            WorldPoint::ZERO,
        )
        .unwrap();
        let destroyed = crystal.damage(50.0).unwrap();
        assert!(destroyed.destroyed);
        assert_eq!(destroyed.loot.unwrap().grants.len(), 3);
        assert!(matches!(
            crystal.lifecycle,
            PropLifecycle::Destroyed {
                respawn_remaining_seconds: Some(50.0)
            }
        ));
        assert!(!crystal.tick(49.0).unwrap());
        assert!(crystal.tick(1.0).unwrap());
        assert_eq!(crystal.lifecycle, PropLifecycle::Active);
        assert_eq!(crystal.health, 50.0);
        assert_eq!(crystal.damage_stage, PropDamageStage::Pristine);
        crystal.validate().unwrap();
    }

    #[test]
    fn prop_registry_emits_simultaneous_respawns_in_stable_id_order() {
        let mut registry = PropStateRegistry::default();
        for id in ["node-z", "node-a"] {
            let mut node = PropState::new(
                id,
                LegacyPropKind::ScrapPile,
                LegacyRegionId::MichiganWilds,
                WorldPoint::ZERO,
            )
            .unwrap();
            node.damage(30.0).unwrap();
            registry.insert(node).unwrap();
        }
        let events = registry.tick(35.0).unwrap();
        assert_eq!(
            events,
            vec![
                PropLifecycleEvent::Respawned {
                    stable_id: "node-a".to_owned()
                },
                PropLifecycleEvent::Respawned {
                    stable_id: "node-z".to_owned()
                },
            ]
        );
    }

    #[test]
    fn prop_state_round_trips_with_remaining_respawn_time() {
        let mut node = PropState::new(
            "bio-save",
            LegacyPropKind::BioPod,
            LegacyRegionId::MichiganWilds,
            WorldPoint::new(3.0, 4.0, 5.0),
        )
        .unwrap();
        node.damage(40.0).unwrap();
        node.tick(12.5).unwrap();
        let encoded = serde_json::to_string(&node).unwrap();
        let decoded: PropState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, node);
        decoded.validate().unwrap();
    }

    #[test]
    fn arena_rules_are_explicit_because_source_has_no_match_constants() {
        assert_eq!(
            ArenaRules::new(ArenaMode::FreeForAll, 0, 60.0, 16),
            Err(ArenaError::ZeroScoreLimit)
        );
        assert_eq!(
            ArenaRules::new(ArenaMode::FreeForAll, 10, 0.0, 16),
            Err(ArenaError::InvalidTimeLimit)
        );
        assert_eq!(
            ArenaRules::new(ArenaMode::Teams { team_count: 1 }, 10, 60.0, 16),
            Err(ArenaError::InvalidTeamCount)
        );
        assert_eq!(
            PlayerSlot::new(24),
            Err(PlayerVisibilityError::PlayerOutOfRange(24))
        );
    }

    #[test]
    fn free_for_all_finishes_at_the_configured_score_limit() {
        let rules = ArenaRules::new(ArenaMode::FreeForAll, 2, 90.0, 16).unwrap();
        let mut arena = OfflineArenaState::new(rules).unwrap();
        arena.join(slot(0), None).unwrap();
        arena.join(slot(1), None).unwrap();
        arena.start().unwrap();
        assert!(
            arena
                .record_elimination(Some(slot(0)), slot(1))
                .unwrap()
                .point_awarded
        );
        let final_elimination = arena.record_elimination(Some(slot(0)), slot(1)).unwrap();
        assert_eq!(
            final_elimination.phase,
            ArenaPhase::Finished(ArenaOutcome::PlayerWon(slot(0)))
        );
        assert_eq!(arena.participants[&slot(0)].score, 2);
        assert_eq!(arena.participants[&slot(1)].deaths, 2);
    }

    #[test]
    fn team_arena_ignores_suicides_and_friendly_fire_then_aggregates_scores() {
        let rules = ArenaRules::new(ArenaMode::Teams { team_count: 2 }, 2, 90.0, 16).unwrap();
        let mut arena = OfflineArenaState::new(rules).unwrap();
        arena.join(slot(0), Some(0)).unwrap();
        arena.join(slot(1), Some(0)).unwrap();
        arena.join(slot(2), Some(1)).unwrap();
        arena.start().unwrap();
        assert!(
            !arena
                .record_elimination(Some(slot(0)), slot(1))
                .unwrap()
                .point_awarded
        );
        assert!(
            !arena
                .record_elimination(Some(slot(2)), slot(2))
                .unwrap()
                .point_awarded
        );
        arena.record_elimination(Some(slot(0)), slot(2)).unwrap();
        let result = arena.record_elimination(Some(slot(1)), slot(2)).unwrap();
        assert_eq!(result.phase, ArenaPhase::Finished(ArenaOutcome::TeamWon(0)));
        assert_eq!(arena.team_score(0), 2);
    }

    #[test]
    fn arena_timer_resolves_unique_winner_or_sorted_draw() {
        let rules = ArenaRules::new(ArenaMode::FreeForAll, 10, 10.0, 16).unwrap();
        let mut arena = OfflineArenaState::new(rules).unwrap();
        arena.join(slot(0), None).unwrap();
        arena.join(slot(1), None).unwrap();
        arena.join(slot(2), None).unwrap();
        arena.start().unwrap();
        arena.record_elimination(Some(slot(0)), slot(2)).unwrap();
        arena.record_elimination(Some(slot(1)), slot(2)).unwrap();
        assert_close(arena.remaining_seconds(), 10.0);
        assert_eq!(
            arena.tick(10.0).unwrap(),
            &ArenaPhase::Finished(ArenaOutcome::Draw {
                leaders: vec![ArenaLeader::Player(slot(0)), ArenaLeader::Player(slot(1))]
            })
        );
        assert_close(arena.remaining_seconds(), 0.0);
    }

    #[test]
    fn versus_spawn_ring_is_deterministic_and_wraps_at_twenty_four() {
        let first = versus_arena_spawn(0);
        let opposite = versus_arena_spawn(12);
        assert_close(first.x, 42.0);
        assert_close(first.y, 2.0);
        assert_close(first.z, 0.0);
        assert_close(opposite.x, -42.0);
        assert_close(opposite.z, 0.0);
        assert_eq!(versus_arena_spawn(24), first);
    }
}
