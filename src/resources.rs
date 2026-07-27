#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::chapters::{Biome, ChapterId};
use crate::character::blueprint::{BodyRecipe, CartoonAppearanceRecipe, CharacterBlueprint};
use crate::character::hero_roster::HERO_NAMES;
use crate::character::parts::{
    ArmPreset, BodyPreset, CharacterLoadout, HeadPreset, LegPreset, ShoulderPreset,
};
use crate::character_studio::spec::CharacterSpec;
use crate::components::player::PlayerProgression;
use crate::robots::designer::RobotStyle;

// ── Wave State (legacy population counter) ────────────────────────────────────
// Kept for compatibility with existing systems (loot, save). The chapter director
// drives all gameplay scheduling now.
#[derive(Resource, Debug, Default)]
pub struct WaveInfo {
    pub wave_number: u32,
    pub wave_timer: f32,
    pub wave_duration: f32,
    pub enemy_count: u32,
    pub max_enemies: u32,
    pub spawn_timer: f32,
    pub spawn_interval: f32,
}

impl WaveInfo {
    pub fn new() -> Self {
        Self {
            wave_number: 1,
            wave_timer: 0.0,
            wave_duration: 60.0,
            enemy_count: 0,
            max_enemies: 50,
            spawn_timer: 0.0,
            spawn_interval: 5.0,
        }
    }

    pub fn advance(&mut self) {
        self.wave_number += 1;
        self.wave_timer = 0.0;
    }

    pub fn difficulty_multiplier(&self) -> f32 {
        1.0 + (self.wave_number.saturating_sub(1) as f32) * 0.2
    }
}

// ── UI input ownership ────────────────────────────────────────────────────────

/// Captures one player's gameplay controls while an in-game modal panel uses
/// that controller. Menu-specific input fields remain available on
/// `PlayerInput`; movement, combat, and interaction actions are suppressed.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct UiGameplayCapture {
    pub owner: Option<u8>,
}

// ── Game Settings ─────────────────────────────────────────────────────────────
#[derive(Resource, Debug, Serialize, Deserialize)]
pub struct GameSettings {
    pub mouse_sensitivity: f32,
    pub master_volume: f32,
    pub show_damage_numbers: bool,
    pub world_seed: u64,
    /// Difficulty multiplier: Easy=0.7, Normal=1.0, Hard=1.3
    #[serde(default = "default_one_f32")]
    pub difficulty_scale: f32,
    /// Music volume (0.0–1.0)
    #[serde(default = "default_one_f32")]
    pub music_volume: f32,
    /// SFX volume (0.0–1.0)
    #[serde(default = "default_one_f32")]
    pub sfx_volume: f32,
    /// Whether to trigger controller rumble on hit events
    #[serde(default = "default_true")]
    pub rumble_on_hit: bool,
}

fn default_one_f32() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 0.0008,
            master_volume: 1.0,
            show_damage_numbers: true,
            world_seed: 42_195,
            difficulty_scale: 1.0,
            music_volume: 1.0,
            sfx_volume: 1.0,
            rumble_on_hit: true,
        }
    }
}

// ── UI Message Queue ──────────────────────────────────────────────────────────
#[derive(Resource, Debug, Default)]
pub struct UiMessage {
    pub text: String,
    pub timer: f32,
}

// ── Player Guidance Prompt ───────────────────────────────────────────────────
#[derive(Resource, Debug, Clone, Default)]
pub struct PlayerGuidance {
    pub visible: bool,
    pub title: String,
    pub body: String,
    pub action: String,
}

impl PlayerGuidance {
    pub fn set(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        action: impl Into<String>,
    ) {
        self.visible = true;
        self.title = title.into();
        self.body = body.into();
        self.action = action.into();
    }

    pub fn clear(&mut self) {
        self.visible = false;
        self.title.clear();
        self.body.clear();
        self.action.clear();
    }
}

// ── Player Score ──────────────────────────────────────────────────────────────
#[derive(Resource, Debug, Default)]
pub struct PlayerScore {
    pub kills: u32,
    pub total_damage_dealt: f32,
    pub chests_opened: u32,
    pub waves_survived: u32,
}

// ── Play Session Transitions ─────────────────────────────────────────────────
/// Tracks pause/resume transitions so `OnEnter(Playing)` setup systems do not
/// rebuild the active chapter when returning from the pause menu.
#[derive(Resource, Debug, Default)]
pub struct PlaySessionTransition {
    pub pausing: bool,
    pub resuming_from_pause: bool,
}

/// Optional non-chapter anchor selected from the world map. Chapter startup
/// consumes this once world anchors exist, then clears it.
#[derive(Resource, Debug, Default, Clone)]
pub struct FastTravelDestination {
    pub anchor_id: Option<&'static str>,
    pub label: Option<&'static str>,
    pub enter_dungeon: bool,
}

impl FastTravelDestination {
    pub fn cave(&mut self, anchor_id: &'static str, label: &'static str) {
        self.anchor_id = Some(anchor_id);
        self.label = Some(label);
        self.enter_dungeon = true;
    }

    pub fn world_anchor(&mut self, anchor_id: &'static str, label: &'static str) {
        self.anchor_id = Some(anchor_id);
        self.label = Some(label);
        self.enter_dungeon = false;
    }

    pub fn clear(&mut self) {
        self.anchor_id = None;
        self.label = None;
        self.enter_dungeon = false;
    }
}

// ── Local Multiplayer ─────────────────────────────────────────────────────────
/// How many local players are active (1–4).
/// Set this before entering `AppState::Playing` to change the player count.
#[derive(Resource, Debug, Clone)]
pub struct LocalPlayerConfig {
    pub active: u8,
}

impl Default for LocalPlayerConfig {
    fn default() -> Self {
        Self { active: 1 }
    }
}

// ── Current Chapter (Starfall I) ──────────────────────────────────────────────
/// The active chapter session. The chapter director system advances `step_index`
/// when each step's completion condition fires.
#[derive(Resource, Debug)]
pub struct CurrentChapter {
    pub id: ChapterId,
    pub biome: Biome,
    pub difficulty_scale: f32,
    pub step_index: usize,
    pub step_timer: f32,
    pub awaiting_kills: u32,
    pub awaiting_puzzle: bool,
    pub completed: bool,
    pub started: bool,
}

impl Default for CurrentChapter {
    fn default() -> Self {
        Self {
            id: ChapterId::FIRST,
            biome: Biome::StarfallLab,
            difficulty_scale: 1.0,
            step_index: 0,
            step_timer: 0.0,
            awaiting_kills: 0,
            awaiting_puzzle: false,
            completed: false,
            started: false,
        }
    }
}

// ── Dungeon Room Progress ─────────────────────────────────────────────────────
#[derive(Resource, Default, Debug, Clone)]
pub struct DungeonRoomState {
    /// Chapters whose dungeon key has been collected this session.
    pub keys_collected: Vec<u8>,
    pub active_gate_id: Option<&'static str>,
    pub active_room: Option<u8>,
    pub visited_rooms: Vec<(&'static str, u8)>,
    pub cleared_rooms: Vec<(&'static str, u8)>,
}

impl DungeonRoomState {
    pub fn has_key(&self, chapter: u8) -> bool {
        self.keys_collected.contains(&chapter)
    }

    pub fn collect_key(&mut self, chapter: u8) {
        if !self.keys_collected.contains(&chapter) {
            self.keys_collected.push(chapter);
        }
    }

    pub fn enter_room(&mut self, gate_id: &'static str, room: u8) -> bool {
        let changed = self.active_gate_id != Some(gate_id) || self.active_room != Some(room);
        self.active_gate_id = Some(gate_id);
        self.active_room = Some(room);
        if !self.visited_rooms.contains(&(gate_id, room)) {
            self.visited_rooms.push((gate_id, room));
        }
        changed
    }

    pub fn mark_cleared(&mut self, gate_id: &'static str, room: u8) {
        if !self.cleared_rooms.contains(&(gate_id, room)) {
            self.cleared_rooms.push((gate_id, room));
        }
    }

    pub fn clear_active(&mut self) {
        self.active_gate_id = None;
        self.active_room = None;
    }
}

// ── Dungeon Crawl Mode ───────────────────────────────────────────────────────
#[derive(Resource, Debug, Clone)]
pub struct DungeonCrawlState {
    pub active: bool,
    pub gate_id: Option<&'static str>,
    /// False on the activation frame so the gate-opening press cannot also
    /// trigger a nearby return portal.
    pub exit_armed: bool,
    pub chapter: Option<ChapterId>,
    pub label: String,
    pub focus: Vec3,
    pub anchor: Vec3,
    pub radius: f32,
}

impl Default for DungeonCrawlState {
    fn default() -> Self {
        Self {
            active: false,
            gate_id: None,
            exit_armed: false,
            chapter: None,
            label: String::new(),
            focus: Vec3::ZERO,
            anchor: Vec3::ZERO,
            radius: 54.0,
        }
    }
}

impl DungeonCrawlState {
    pub fn activate(
        &mut self,
        gate_id: &'static str,
        chapter: ChapterId,
        label: impl Into<String>,
        focus: Vec3,
        anchor: Vec3,
        radius: f32,
    ) {
        self.active = true;
        self.gate_id = Some(gate_id);
        self.exit_armed = false;
        self.chapter = Some(chapter);
        self.label = label.into();
        self.focus = focus;
        self.anchor = anchor;
        self.radius = radius.max(28.0);
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.gate_id = None;
        self.exit_armed = false;
        self.chapter = None;
        self.label.clear();
        self.focus = Vec3::ZERO;
        self.anchor = Vec3::ZERO;
        self.radius = 54.0;
    }
}

// ── Biome Palette ─────────────────────────────────────────────────────────────
#[derive(Resource, Debug, Clone)]
pub struct BiomePalette {
    pub sky: Color,
    pub fog: Color,
    pub ground: Color,
    pub accent: Color,
}

impl Default for BiomePalette {
    fn default() -> Self {
        let (sky, fog, ground, accent) = Biome::StarfallLab.palette();
        Self {
            sky,
            fog,
            ground,
            accent,
        }
    }
}

// ── Player Chassis (visual customization) ─────────────────────────────────────
#[derive(Resource, Debug, Clone)]
pub struct PlayerChassis(pub RobotStyle);

impl Default for PlayerChassis {
    fn default() -> Self {
        Self(crate::robots::presets::amp())
    }
}

/// Per-slot preset choices, persisted between the character editor and gameplay.
/// Applied to `CharacterLoadout` when the player character spawns.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Resource)]
pub struct PlayerPartLoadout {
    pub body: BodyPreset,
    pub arms: ArmPreset,
    pub legs: LegPreset,
    pub shoulders: ShoulderPreset,
    pub head: HeadPreset,
}

impl Default for PlayerPartLoadout {
    fn default() -> Self {
        Self::vincenzo_reference()
    }
}

impl PlayerPartLoadout {
    pub const fn new(
        body: BodyPreset,
        arms: ArmPreset,
        legs: LegPreset,
        shoulders: ShoulderPreset,
        head: HeadPreset,
    ) -> Self {
        Self {
            body,
            arms,
            legs,
            shoulders,
            head,
        }
    }

    pub const fn legacy_stock() -> Self {
        Self::new(
            BodyPreset::StandardMech,
            ArmPreset::MechArms,
            LegPreset::MechLegs,
            ShoulderPreset::DomePauldrons,
            HeadPreset::OpenFace,
        )
    }

    pub const fn legacy_vincenzo_scout() -> Self {
        Self::new(
            BodyPreset::ScoutVest,
            ArmPreset::ScoutArms,
            LegPreset::ScoutLegs,
            ShoulderPreset::DomePauldrons,
            HeadPreset::OpenFace,
        )
    }

    pub const fn amp_reference() -> Self {
        Self::new(
            BodyPreset::HeavyPlate,
            ArmPreset::HeavyArms,
            LegPreset::HeavyLegs,
            ShoulderPreset::PlateEpaulettes,
            HeadPreset::FullHelm,
        )
    }

    pub const fn antonio_reference() -> Self {
        Self::new(
            BodyPreset::RiftMantle,
            ArmPreset::RiftTalons,
            LegPreset::RiftBoots,
            ShoulderPreset::RiftCloak,
            HeadPreset::RiftCowl,
        )
    }

    pub const fn chroma_reference() -> Self {
        Self::new(
            BodyPreset::ChromaFrame,
            ArmPreset::ChromaBlades,
            LegPreset::ChromaStriders,
            ShoulderPreset::ChromaMantle,
            HeadPreset::ChromaCrown,
        )
    }

    pub const fn daria_reference() -> Self {
        Self::new(
            BodyPreset::DariaCore,
            ArmPreset::DariaCannon,
            LegPreset::DariaGreaves,
            ShoulderPreset::DariaFlares,
            HeadPreset::DariaHelm,
        )
    }

    pub const fn vincenzo_reference() -> Self {
        Self::chroma_reference()
    }

    pub fn reference_for_name(name: &str) -> Self {
        match name {
            "AMP" | "Amp" | "Angelo" | "Joseph" => Self::amp_reference(),
            "Antonio" | "Fortuna" => Self::antonio_reference(),
            "Daria" | "Gabriella" | "Aurora" => Self::daria_reference(),
            "Chroma" | "Nova" | "Vincenzo" => Self::chroma_reference(),
            _ => Self::vincenzo_reference(),
        }
    }

    pub fn resolve_for_hero(name: &str, slot_loadout: Option<Self>, shared_loadout: Self) -> Self {
        if let Some(loadout) = slot_loadout.filter(|loadout| !loadout.is_stale_native_default()) {
            return loadout;
        }
        if !shared_loadout.is_stale_native_default() {
            return shared_loadout;
        }
        Self::reference_for_name(name)
    }

    pub fn is_legacy_stock(self) -> bool {
        self == Self::legacy_stock()
    }

    pub fn is_stale_native_default(self) -> bool {
        self.is_legacy_stock() || self == Self::legacy_vincenzo_scout()
    }
}

impl From<PlayerPartLoadout> for CharacterLoadout {
    fn from(loadout: PlayerPartLoadout) -> Self {
        Self {
            body: loadout.body,
            arms: loadout.arms,
            legs: loadout.legs,
            shoulders: loadout.shoulders,
            head: loadout.head,
        }
    }
}

impl From<CharacterLoadout> for PlayerPartLoadout {
    fn from(loadout: CharacterLoadout) -> Self {
        Self {
            body: loadout.body,
            arms: loadout.arms,
            legs: loadout.legs,
            shoulders: loadout.shoulders,
            head: loadout.head,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Reflect)]
pub enum CharacterBaseModel {
    AmpSiege,
    AntonioRift,
    #[default]
    ChromaTrace,
    DariaFlares,
    VincenzoDeep,
}

impl CharacterBaseModel {
    pub const ALL: [Self; 5] = [
        Self::AmpSiege,
        Self::AntonioRift,
        Self::ChromaTrace,
        Self::DariaFlares,
        Self::VincenzoDeep,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::AmpSiege => "AMP Siege",
            Self::AntonioRift => "Antonio Rift",
            Self::ChromaTrace => "Chroma Trace",
            Self::DariaFlares => "Daria Flares",
            Self::VincenzoDeep => "Vincenzo Deep",
        }
    }

    pub fn hero_hint(self) -> &'static str {
        match self {
            Self::AmpSiege => "Joseph",
            Self::AntonioRift => "Antonio",
            Self::ChromaTrace => "Nova",
            Self::DariaFlares => "Gabriella",
            Self::VincenzoDeep => "Vincenzo",
        }
    }

    pub const fn loadout(self) -> PlayerPartLoadout {
        match self {
            Self::AmpSiege => PlayerPartLoadout::amp_reference(),
            Self::AntonioRift => PlayerPartLoadout::antonio_reference(),
            Self::ChromaTrace => PlayerPartLoadout::chroma_reference(),
            Self::DariaFlares => PlayerPartLoadout::daria_reference(),
            Self::VincenzoDeep => PlayerPartLoadout::vincenzo_reference(),
        }
    }

    pub fn body_recipe(self) -> BodyRecipe {
        reference_body_recipe(self.hero_hint())
    }

    pub fn appearance(self) -> CartoonAppearanceRecipe {
        reference_appearance_recipe(self.hero_hint())
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "AMP" | "Amp" | "Angelo" | "Joseph" => Self::AmpSiege,
            "Antonio" | "Fortuna" => Self::AntonioRift,
            "Daria" | "Gabriella" | "Aurora" => Self::DariaFlares,
            "Chroma" | "Nova" => Self::ChromaTrace,
            "Vincenzo" => Self::VincenzoDeep,
            _ => Self::default(),
        }
    }

    pub fn from_loadout(loadout: PlayerPartLoadout) -> Self {
        if loadout == PlayerPartLoadout::amp_reference() {
            Self::AmpSiege
        } else if loadout == PlayerPartLoadout::antonio_reference() {
            Self::AntonioRift
        } else if loadout == PlayerPartLoadout::daria_reference() {
            Self::DariaFlares
        } else if loadout == PlayerPartLoadout::vincenzo_reference() {
            Self::VincenzoDeep
        } else {
            Self::ChromaTrace
        }
    }

    pub fn from_name_and_loadout(name: &str, loadout: PlayerPartLoadout) -> Self {
        let named = Self::from_name(name);
        if loadout == named.loadout() {
            named
        } else {
            Self::from_loadout(loadout)
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct CharacterBaseModelCatalog {
    pub models: Vec<CharacterBaseModel>,
}

impl Default for CharacterBaseModelCatalog {
    fn default() -> Self {
        Self {
            models: CharacterBaseModel::ALL.to_vec(),
        }
    }
}

impl CharacterBaseModelCatalog {
    pub fn by_label(&self, label: &str) -> Option<CharacterBaseModel> {
        self.models
            .iter()
            .copied()
            .find(|model| model.label() == label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShopCategory {
    Outfits,
    Armor,
    Weapons,
    Vehicles,
}

impl ShopCategory {
    pub const ALL: [Self; 4] = [Self::Outfits, Self::Armor, Self::Weapons, Self::Vehicles];

    pub fn label(self) -> &'static str {
        match self {
            Self::Outfits => "Outfits",
            Self::Armor => "Armor",
            Self::Weapons => "Weapons",
            Self::Vehicles => "Vehicles",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShopItem {
    pub id: &'static str,
    pub name: &'static str,
    pub category: ShopCategory,
    pub summary: &'static str,
    pub price_credits: u32,
    pub preview_loadout: Option<PlayerPartLoadout>,
}

impl ShopItem {
    pub const fn new(
        id: &'static str,
        name: &'static str,
        category: ShopCategory,
        summary: &'static str,
        price_credits: u32,
        preview_loadout: Option<PlayerPartLoadout>,
    ) -> Self {
        Self {
            id,
            name,
            category,
            summary,
            price_credits,
            preview_loadout,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct ShopCatalog {
    pub items: Vec<ShopItem>,
}

impl Default for ShopCatalog {
    fn default() -> Self {
        Self {
            items: vec![
                ShopItem::new(
                    "outfit_amp_siege_base",
                    "AMP Siege Base",
                    ShopCategory::Outfits,
                    "Wide mecha-human frame with armored arms, heavy boots, and plate shoulders.",
                    1250,
                    Some(PlayerPartLoadout::amp_reference()),
                ),
                ShopItem::new(
                    "outfit_antonio_rift_base",
                    "Antonio Rift Base",
                    ShopCategory::Outfits,
                    "Long talon arms, swept rift boots, asymmetric cloak silhouette.",
                    1400,
                    Some(PlayerPartLoadout::antonio_reference()),
                ),
                ShopItem::new(
                    "outfit_daria_flare_base",
                    "Daria Flare Base",
                    ShopCategory::Outfits,
                    "Catlike helmet, shoulder fins, cannon arm, and athletic greaves.",
                    1400,
                    Some(PlayerPartLoadout::daria_reference()),
                ),
                ShopItem::new(
                    "armor_chroma_boot_kit",
                    "Chroma Boot Kit",
                    ShopCategory::Armor,
                    "Segmented foot shells, toe spurs, knee guards, and rear shin thrusters.",
                    620,
                    Some(PlayerPartLoadout::chroma_reference()),
                ),
                ShopItem::new(
                    "armor_aegis_helmet",
                    "Aegis Helmet Shell",
                    ShopCategory::Armor,
                    "Enclosed visor shell for high-speed patrol routes and mountain combat.",
                    780,
                    Some(PlayerPartLoadout::amp_reference()),
                ),
                ShopItem::new(
                    "weapon_solar_sabre",
                    "Solar Sabre",
                    ShopCategory::Weapons,
                    "Close-range beam blade package with matching braced gauntlet poses.",
                    900,
                    None,
                ),
                ShopItem::new(
                    "weapon_nova_missile_matrix",
                    "Nova Missile Matrix",
                    ShopCategory::Weapons,
                    "Shoulder and wrist targeting upgrade for heavy robot patrol fights.",
                    1150,
                    None,
                ),
                // Star Sabre blades. Ids and prices are the join key into
                // `crate::combat::blades::BLADE_CATALOG`, which owns what each
                // one actually does; `every_shop_blade_id_resolves` guards the
                // pairing. Equipping one restats and recolours the sabre.
                ShopItem::new(
                    "weapon_crimson_edge",
                    "Crimson Edge",
                    ShopCategory::Weapons,
                    "Heavy war blade. Huge hits, short chain, deliberate swing.",
                    1400,
                    None,
                ),
                ShopItem::new(
                    "weapon_emerald_lash",
                    "Emerald Lash",
                    ShopCategory::Weapons,
                    "Living blade. Longer, faster chain that feeds health back on every hit.",
                    1600,
                    None,
                ),
                ShopItem::new(
                    "weapon_violet_tempest",
                    "Violet Tempest",
                    ShopCategory::Weapons,
                    "Wave-tuned hilt. Energy waves pierce ranks and hit far harder.",
                    1750,
                    None,
                ),
                ShopItem::new(
                    "weapon_gold_regent",
                    "Gold Regent",
                    ShopCategory::Weapons,
                    "Duelist's hilt. Light, relentless swings and techniques that barely rest.",
                    1900,
                    None,
                ),
                ShopItem::new(
                    "weapon_frost_vigil",
                    "Frost Vigil",
                    ShopCategory::Weapons,
                    "Siege hilt. Every energy wave detonates on impact.",
                    2000,
                    None,
                ),
                ShopItem::new(
                    "weapon_void_requiem",
                    "Void Requiem",
                    ShopCategory::Weapons,
                    "Unstable rift blade. Devastating and hungry, but slow to recover.",
                    2400,
                    None,
                ),
                ShopItem::new(
                    "vehicle_hoverboard_race",
                    "Hoverboard Race Deck",
                    ShopCategory::Vehicles,
                    "Wide speed-road board tuned for ramps, loops, and mountain-scale tricks.",
                    1000,
                    None,
                ),
                ShopItem::new(
                    "vehicle_giant_mech_frame",
                    "Giant Mech Frame",
                    ShopCategory::Vehicles,
                    "Robot-pet assembly target for future driver-scale combat routes.",
                    2200,
                    None,
                ),
            ],
        }
    }
}

impl ShopCatalog {
    pub fn items_for(&self, category: ShopCategory) -> impl Iterator<Item = &ShopItem> {
        self.items
            .iter()
            .filter(move |item| item.category == category)
    }

    pub fn category_count(&self, category: ShopCategory) -> usize {
        self.items_for(category).count()
    }
}

pub fn reference_body_recipe(name: &str) -> BodyRecipe {
    let body = match name {
        "AMP" | "Amp" | "Angelo" | "Joseph" => BodyRecipe {
            height: 1.12,
            shoulder_width: 1.34,
            chest_size: 1.30,
            arm_length: 1.22,
            leg_length: 1.22,
            hand_size: 1.24,
            foot_size: 1.36,
            head_size: 0.90,
            neck_length: 0.90,
            torso_curve: 0.04,
            hip_width: 1.20,
            spine_posture: 0.10,
            mass: 1.45,
            muscle: 1.36,
            body_fat: 1.00,
            asymmetry: 0.04,
        },
        "Antonio" | "Fortuna" => BodyRecipe {
            height: 1.12,
            shoulder_width: 1.04,
            chest_size: 0.96,
            arm_length: 1.28,
            leg_length: 1.30,
            hand_size: 1.10,
            foot_size: 1.22,
            head_size: 1.08,
            neck_length: 1.04,
            torso_curve: 0.16,
            hip_width: 0.88,
            spine_posture: -0.14,
            mass: 0.92,
            muscle: 1.04,
            body_fat: 0.84,
            asymmetry: 0.22,
        },
        "Daria" | "Gabriella" | "Aurora" => BodyRecipe {
            height: 1.16,
            shoulder_width: 1.22,
            chest_size: 1.02,
            arm_length: 1.26,
            leg_length: 1.36,
            hand_size: 1.22,
            foot_size: 1.30,
            head_size: 1.00,
            neck_length: 1.08,
            torso_curve: -0.08,
            hip_width: 0.90,
            spine_posture: 0.02,
            mass: 0.94,
            muscle: 1.16,
            body_fat: 0.82,
            asymmetry: 0.24,
        },
        "Chroma" | "Nova" => BodyRecipe {
            height: 1.10,
            shoulder_width: 1.02,
            chest_size: 0.94,
            arm_length: 1.22,
            leg_length: 1.40,
            hand_size: 1.04,
            foot_size: 1.20,
            head_size: 1.16,
            neck_length: 1.06,
            torso_curve: -0.06,
            hip_width: 0.92,
            spine_posture: -0.06,
            mass: 0.88,
            muscle: 1.00,
            body_fat: 0.86,
            asymmetry: 0.12,
        },
        _ => BodyRecipe {
            height: 1.18,
            shoulder_width: 1.00,
            chest_size: 0.96,
            arm_length: 1.30,
            leg_length: 1.45,
            hand_size: 1.02,
            foot_size: 1.18,
            head_size: 0.92,
            neck_length: 1.08,
            torso_curve: 0.10,
            hip_width: 0.92,
            spine_posture: -0.12,
            mass: 0.86,
            muscle: 1.08,
            body_fat: 0.82,
            asymmetry: 0.08,
        },
    };
    body.validated()
}

pub fn reference_appearance_recipe(name: &str) -> CartoonAppearanceRecipe {
    match name {
        "Antonio" | "Fortuna" => CartoonAppearanceRecipe {
            has_hood: true,
            has_cape: true,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: true,
            has_visor: true,
        },
        _ => CartoonAppearanceRecipe {
            has_hood: false,
            has_cape: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: true,
            has_visor: true,
        },
    }
}

pub fn is_stale_reference_blueprint(name: &str, blueprint: &CharacterBlueprint) -> bool {
    let body = blueprint.body.validated();
    let appearance = blueprint.cartoon_appearance;
    let blueprint_name_matches = blueprint.name == name || blueprint.name == "Vincenzo";

    blueprint_name_matches
        && matches!(name, "Vincenzo" | "Chroma" | "Nova")
        && (body.leg_length < 1.42
            || body.arm_length < 1.20
            || body.mass > 0.95
            || !appearance.has_visor
            || appearance.has_hood
            || appearance.has_cape)
}

// ── Chapter Progress (saveable) ───────────────────────────────────────────────
#[derive(Resource, Debug, Default, Clone)]
pub struct ChapterProgress {
    pub completed: Vec<u8>,
    pub discoverables: Vec<String>,
    pub companions_recruited: Vec<String>,
    pub scientist_relics: Vec<String>,
    pub relic_fragments: Vec<String>,
    /// Set to true when all territories are liberated and the Final War is dormant.
    pub campaign_complete: bool,
}

impl ChapterProgress {
    pub fn is_completed(&self, id: ChapterId) -> bool {
        self.completed.contains(&id.0)
    }
    pub fn is_unlocked(&self, id: ChapterId) -> bool {
        if id == ChapterId::FIRST {
            return true;
        }
        self.completed
            .iter()
            .copied()
            .map(ChapterId)
            .any(|completed| completed.next() == Some(id))
    }
    /// Exploration-first travel policy: every authored chapter destination is
    /// visible and usable from a new save, while story completion remains
    /// independently tracked by [`Self::is_unlocked`].
    pub fn is_fast_travel_unlocked(&self, id: ChapterId) -> bool {
        (ChapterId::FIRST.0..=ChapterId::LAST.0).contains(&id.0)
    }
    pub fn mark_completed(&mut self, id: ChapterId) {
        if !self.completed.contains(&id.0) {
            self.completed.push(id.0);
        }
    }
    pub fn unlock(&mut self, id: &str) {
        if !self.discoverables.iter().any(|d| d == id) {
            self.discoverables.push(id.to_string());
        }
    }
    pub fn has_discoverable(&self, id: &str) -> bool {
        self.discoverables.iter().any(|d| d == id)
    }
    pub fn recruit(&mut self, name: &str) {
        if !self.companions_recruited.iter().any(|c| c == name) {
            self.companions_recruited.push(name.to_string());
        }
    }
    pub fn recover_relic(&mut self, scientist: &str, relic_id: &str) {
        let key = format!("{scientist}:{relic_id}");
        if !self.scientist_relics.iter().any(|entry| entry == &key) {
            self.scientist_relics.push(key);
        }
    }
    pub fn has_relic(&self, scientist: &str, relic_id: &str) -> bool {
        let key = format!("{scientist}:{relic_id}");
        self.scientist_relics.iter().any(|entry| entry == &key)
    }
    pub fn recover_relic_fragment(&mut self, scientist: &str, relic_id: &str, piece: u8) -> bool {
        let key = format!("{scientist}:{relic_id}:{piece}");
        if self.relic_fragments.iter().any(|entry| entry == &key) {
            false
        } else {
            self.relic_fragments.push(key);
            true
        }
    }
    pub fn relic_fragment_count(&self, scientist: &str, relic_id: &str) -> usize {
        let prefix = format!("{scientist}:{relic_id}:");
        self.relic_fragments
            .iter()
            .filter(|entry| entry.starts_with(&prefix))
            .count()
    }
    pub fn has_relic_fragment(&self, scientist: &str, relic_id: &str, piece: u8) -> bool {
        let key = format!("{scientist}:{relic_id}:{piece}");
        self.relic_fragments.iter().any(|entry| entry == &key)
    }
}

// ── Radio Chatter Queue ───────────────────────────────────────────────────────
#[derive(Resource, Debug, Default)]
pub struct RadioChatter {
    pub lines: Vec<RadioLine>,
}

#[derive(Debug, Clone)]
pub struct RadioLine {
    pub speaker: String,
    pub text: String,
    pub color: Color,
    pub remaining: f32,
}

// ── Character Design State ────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharacterDesignReturnTarget {
    #[default]
    PlayerSelect,
    ChapterSelect,
}

/// Screen that opened the imported-character asset editor. The forge is shared
/// by the creator Project Hub and the player-facing character selection flow.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportedForgeReturnTarget {
    #[default]
    ProjectHub,
    PlayerSelect,
}

/// Transient resource holding the in-progress customization for one player slot.
/// Set `player_index` before transitioning to `AppState::CharacterDesign`.
#[derive(Resource, Debug)]
pub struct CharacterDesignData {
    /// Anime face authoring edited by the designer's FACE section.
    pub face: crate::character::face::FaceRecipe,
    pub player_index: usize,
    pub return_target: CharacterDesignReturnTarget,
    pub base_model: CharacterBaseModel,
    pub skin_idx: usize,
    pub outfit_idx: usize,
    pub accent_idx: usize,
    pub hair_idx: usize,
    pub eye_idx: usize,
    pub body_preset: BodyPreset,
    pub arm_preset: ArmPreset,
    pub leg_preset: LegPreset,
    pub shoulder_preset: ShoulderPreset,
    pub head_preset: HeadPreset,
    pub body: BodyRecipe,
    pub has_hood: bool,
    pub has_cape: bool,
    pub has_gloves: bool,
    pub has_boots: bool,
    pub has_shoulder_pads: bool,
    pub has_visor: bool,
    pub spin_angle: f32,
    pub preview_distance: f32,
    pub dirty: bool,
    pub preview_entity: Option<Entity>,
}

impl Default for CharacterDesignData {
    fn default() -> Self {
        let loadout = PlayerPartLoadout::vincenzo_reference();
        Self {
            face: crate::character::face::FaceRecipe::DEFAULT,
            player_index: 0,
            return_target: CharacterDesignReturnTarget::PlayerSelect,
            base_model: CharacterBaseModel::VincenzoDeep,
            skin_idx: 0,
            outfit_idx: 6,
            accent_idx: 6,
            hair_idx: 7,
            eye_idx: 5,
            body_preset: loadout.body,
            arm_preset: loadout.arms,
            leg_preset: loadout.legs,
            shoulder_preset: loadout.shoulders,
            head_preset: loadout.head,
            body: reference_body_recipe("Vincenzo"),
            has_hood: false,
            has_cape: false,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: true,
            has_visor: true,
            spin_angle: 0.0,
            preview_distance: 3.2,
            dirty: false,
            preview_entity: None,
        }
    }
}

pub const CHARACTER_DESIGN_SNAPSHOT_VERSION: u32 = 2;

/// Serializable, reflected editor payload for one character design.
///
/// This intentionally excludes transient preview entities so the same struct can
/// back save slots, in-game prefab files, and a future reflected inspector.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect, PartialEq)]
pub struct CharacterDesignSnapshot {
    pub schema_version: u32,
    pub player_index: usize,
    /// Added in schema 2. Schema-1 imports leave the current authored face
    /// untouched because a serde default cannot represent "field absent."
    #[serde(default)]
    pub face: crate::character::face::FaceRecipe,
    #[serde(default)]
    pub base_model: CharacterBaseModel,
    pub skin_idx: usize,
    pub outfit_idx: usize,
    pub accent_idx: usize,
    pub hair_idx: usize,
    pub eye_idx: usize,
    pub part_loadout: PlayerPartLoadout,
    pub body: BodyRecipe,
    pub appearance: CartoonAppearanceRecipe,
}

impl CharacterDesignSnapshot {
    pub fn from_design_data(data: &CharacterDesignData) -> Self {
        Self {
            schema_version: CHARACTER_DESIGN_SNAPSHOT_VERSION,
            player_index: data.player_index,
            face: data.face.sanitized(),
            base_model: data.base_model,
            skin_idx: data.skin_idx,
            outfit_idx: data.outfit_idx,
            accent_idx: data.accent_idx,
            hair_idx: data.hair_idx,
            eye_idx: data.eye_idx,
            part_loadout: PlayerPartLoadout {
                body: data.body_preset,
                arms: data.arm_preset,
                legs: data.leg_preset,
                shoulders: data.shoulder_preset,
                head: data.head_preset,
            },
            body: data.body.validated(),
            appearance: CartoonAppearanceRecipe {
                has_hood: data.has_hood,
                has_cape: data.has_cape,
                has_gloves: data.has_gloves,
                has_boots: data.has_boots,
                has_shoulder_pads: data.has_shoulder_pads,
                has_visor: data.has_visor,
            },
        }
    }

    pub fn apply_to_design_data(&self, data: &mut CharacterDesignData) {
        data.player_index = self.player_index;
        if self.schema_version >= CHARACTER_DESIGN_SNAPSHOT_VERSION {
            data.face = self.face.sanitized();
        }
        data.base_model = self.base_model;
        data.skin_idx = self.skin_idx;
        data.outfit_idx = self.outfit_idx;
        data.accent_idx = self.accent_idx;
        data.hair_idx = self.hair_idx;
        data.eye_idx = self.eye_idx;
        data.body_preset = self.part_loadout.body;
        data.arm_preset = self.part_loadout.arms;
        data.leg_preset = self.part_loadout.legs;
        data.shoulder_preset = self.part_loadout.shoulders;
        data.head_preset = self.part_loadout.head;
        data.body = self.body.validated();
        data.has_hood = self.appearance.has_hood;
        data.has_cape = self.appearance.has_cape;
        data.has_gloves = self.appearance.has_gloves;
        data.has_boots = self.appearance.has_boots;
        data.has_shoulder_pads = self.appearance.has_shoulder_pads;
        data.has_visor = self.appearance.has_visor;
        data.dirty = true;
    }
}

// ── Hero Roster ───────────────────────────────────────────────────────────────
pub const HERO_ROSTER: [&str; 8] = HERO_NAMES;

// ── Player Select Lobby ───────────────────────────────────────────────────────
#[derive(Clone)]
pub struct PlayerSlotConfig {
    pub joined: bool,
    pub character_index: usize,
    pub ready: bool,
    pub stick_cooldown: f32,
    // Customization overrides — None means use the hero's default
    pub skin_idx: Option<usize>,
    pub outfit_idx: Option<usize>,
    pub accent_idx: Option<usize>,
    pub hair_idx: Option<usize>,
    pub eye_idx: Option<usize>,
    pub has_hood: Option<bool>,
    pub has_cape: Option<bool>,
    pub has_gloves: Option<bool>,
    pub has_boots: Option<bool>,
    pub has_shoulder_pads: Option<bool>,
    pub has_visor: Option<bool>,
    pub part_loadout: Option<PlayerPartLoadout>,
    pub blueprint: Option<CharacterBlueprint>,
    /// Exact Advanced Character Studio recipe used by the runtime mesh builder.
    pub studio_spec: Option<CharacterSpec>,
    /// Save-backed perks, tech upgrades, and weapon ranks owned by this player.
    pub progression: PlayerProgression,
}

impl Default for PlayerSlotConfig {
    fn default() -> Self {
        Self {
            joined: false,
            character_index: 0,
            ready: false,
            stick_cooldown: 0.0,
            skin_idx: None,
            outfit_idx: None,
            accent_idx: None,
            hair_idx: None,
            eye_idx: None,
            has_hood: None,
            has_cape: None,
            has_gloves: None,
            has_boots: None,
            has_shoulder_pads: None,
            has_visor: None,
            part_loadout: None,
            blueprint: None,
            studio_spec: None,
            progression: PlayerProgression::default(),
        }
    }
}

#[derive(Resource, Default)]
pub struct PlayerSelectState {
    pub slots: [PlayerSlotConfig; 4],
}

impl PlayerSelectState {
    pub fn active_count(&self) -> u8 {
        self.slots.iter().filter(|s| s.joined).count() as u8
    }

    pub fn all_ready(&self) -> bool {
        self.slots.iter().any(|s| s.joined)
            && self.slots.iter().filter(|s| s.joined).all(|s| s.ready)
    }

    pub fn character_name(&self, slot: usize) -> &'static str {
        let idx = self
            .slots
            .get(slot)
            .map(|s| s.character_index)
            .unwrap_or(slot % HERO_ROSTER.len());
        HERO_ROSTER[idx % HERO_ROSTER.len()]
    }
}

// ── World Site Registry (M5 Reclaimable World State) ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorldSiteKind {
    City,
    Village,
    Farm,
    Factory,
    Spaceport,
    PowerPlant,
    ResearchLab,
    DefenseOutpost,
    Harbor,
    Temple,
    Castle,
    Mine,
    BridgeHub,
    MountainGate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum WorldSiteState {
    Hidden,
    #[default]
    EnemyHeld,
    Contested,
    Liberated,
    Building,
    Damaged,
    UnderAttack,
    Shielded,
    OccupiedAgain,
}

impl WorldSiteState {
    pub fn map_badge_color(&self) -> Color {
        match self {
            WorldSiteState::EnemyHeld | WorldSiteState::OccupiedAgain => {
                Color::srgba(0.85, 0.15, 0.15, 0.9)
            }
            WorldSiteState::Contested | WorldSiteState::UnderAttack => {
                Color::srgba(0.95, 0.65, 0.05, 0.9)
            }
            WorldSiteState::Liberated | WorldSiteState::Building | WorldSiteState::Shielded => {
                Color::srgba(0.15, 0.85, 0.25, 0.9)
            }
            WorldSiteState::Damaged => Color::srgba(0.85, 0.55, 0.15, 0.9),
            WorldSiteState::Hidden => Color::srgba(0.4, 0.4, 0.4, 0.5),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum WorldSiteOwner {
    FreePeoples,
    #[default]
    Scallarian,
    DragonDomain,
    Neutral,
    PlayerAlliance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldSiteId(pub u16);

#[derive(Debug, Clone)]
pub struct WorldSite {
    pub id: WorldSiteId,
    pub name: &'static str,
    pub region: &'static str,
    pub kind: WorldSiteKind,
    pub state: WorldSiteState,
    pub owner: WorldSiteOwner,
    pub world_x: f32,
    pub world_z: f32,
    /// Number of enemies that must be defeated to liberate this site.
    pub enemy_count_to_liberate: u8,
    pub enemies_defeated: u8,
}

impl WorldSite {
    pub fn is_liberated(&self) -> bool {
        matches!(
            self.state,
            WorldSiteState::Liberated | WorldSiteState::Building | WorldSiteState::Shielded
        )
    }

    /// Record one enemy defeat. Returns true the first time liberation is reached.
    pub fn record_enemy_defeated(&mut self) -> bool {
        if self.is_liberated() {
            return false;
        }
        self.enemies_defeated = self
            .enemies_defeated
            .saturating_add(1)
            .min(self.enemy_count_to_liberate);
        if self.enemies_defeated >= self.enemy_count_to_liberate {
            self.state = WorldSiteState::Liberated;
            self.owner = WorldSiteOwner::PlayerAlliance;
            return true;
        }
        false
    }
}

/// Compact record serialized into the save file.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorldSiteSaveRecord {
    pub id: u16,
    pub state: WorldSiteState,
    pub owner: WorldSiteOwner,
    pub enemies_defeated: u8,
}

#[derive(Resource, Debug, Clone)]
pub struct WorldSiteRegistry {
    pub sites: Vec<WorldSite>,
}

impl Default for WorldSiteRegistry {
    fn default() -> Self {
        Self {
            sites: initial_world_sites(),
        }
    }
}

impl WorldSiteRegistry {
    pub fn get(&self, id: WorldSiteId) -> Option<&WorldSite> {
        self.sites.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: WorldSiteId) -> Option<&mut WorldSite> {
        self.sites.iter_mut().find(|s| s.id == id)
    }

    pub fn to_save_records(&self) -> Vec<WorldSiteSaveRecord> {
        self.sites
            .iter()
            .map(|s| WorldSiteSaveRecord {
                id: s.id.0,
                state: s.state,
                owner: s.owner,
                enemies_defeated: s.enemies_defeated,
            })
            .collect()
    }

    pub fn apply_save_records(&mut self, records: &[WorldSiteSaveRecord]) {
        for record in records {
            if let Some(site) = self.get_mut(WorldSiteId(record.id)) {
                site.state = record.state;
                site.owner = record.owner;
                site.enemies_defeated = record.enemies_defeated;
            }
        }
    }
}

// ── World Site Static Definitions ────────────────────────────────────────────

/// Returns the canonical initial set of world sites. Called once on registry setup.
pub fn initial_world_sites() -> Vec<WorldSite> {
    vec![
        // ── Standalone defense sites ──────────────────────────────────────
        WorldSite {
            id: WorldSiteId(1),
            name: "Iron Watchpost",
            region: "Starfall Zone",
            kind: WorldSiteKind::DefenseOutpost,
            state: WorldSiteState::EnemyHeld,
            owner: WorldSiteOwner::Scallarian,
            world_x: 380.0,
            world_z: 520.0,
            enemy_count_to_liberate: 5,
            enemies_defeated: 0,
        },
        // ── Settlements (match map_settlements() positions and site_id) ───
        WorldSite {
            id: WorldSiteId(2),
            name: "Riftglass Village",
            region: "Rift Foothills",
            kind: WorldSiteKind::Village,
            state: WorldSiteState::EnemyHeld,
            owner: WorldSiteOwner::Scallarian,
            world_x: 1500.0,
            world_z: 3300.0,
            enemy_count_to_liberate: 8,
            enemies_defeated: 0,
        },
        WorldSite {
            id: WorldSiteId(3),
            name: "Starfell Outpost",
            region: "Crown Road",
            kind: WorldSiteKind::DefenseOutpost,
            state: WorldSiteState::EnemyHeld,
            owner: WorldSiteOwner::Scallarian,
            world_x: -1800.0,
            world_z: -6900.0,
            enemy_count_to_liberate: 6,
            enemies_defeated: 0,
        },
        // Peaceful mega-city hubs — start Liberated under Free Peoples protection.
        WorldSite {
            id: WorldSiteId(4),
            name: "Cloudrail City",
            region: "High Sky Rail",
            kind: WorldSiteKind::City,
            state: WorldSiteState::Liberated,
            owner: WorldSiteOwner::FreePeoples,
            world_x: 4200.0,
            world_z: 4300.0,
            enemy_count_to_liberate: 0,
            enemies_defeated: 0,
        },
        WorldSite {
            id: WorldSiteId(5),
            name: "Lantern Hamlet",
            region: "Tibet Snow Road",
            kind: WorldSiteKind::Village,
            state: WorldSiteState::EnemyHeld,
            owner: WorldSiteOwner::Scallarian,
            world_x: -6900.0,
            world_z: -4200.0,
            enemy_count_to_liberate: 8,
            enemies_defeated: 0,
        },
        WorldSite {
            id: WorldSiteId(6),
            name: "Star Orchard",
            region: "Fangroot Meadow",
            kind: WorldSiteKind::Village,
            state: WorldSiteState::EnemyHeld,
            owner: WorldSiteOwner::Scallarian,
            world_x: -3800.0,
            world_z: 5700.0,
            enemy_count_to_liberate: 6,
            enemies_defeated: 0,
        },
        WorldSite {
            id: WorldSiteId(7),
            name: "Frost Harbor",
            region: "Antarctic Range",
            kind: WorldSiteKind::Harbor,
            state: WorldSiteState::EnemyHeld,
            owner: WorldSiteOwner::Scallarian,
            world_x: 2300.0,
            world_z: -6700.0,
            enemy_count_to_liberate: 10,
            enemies_defeated: 0,
        },
        WorldSite {
            id: WorldSiteId(8),
            name: "Granite Market",
            region: "Rockies Gate",
            kind: WorldSiteKind::Village,
            state: WorldSiteState::EnemyHeld,
            owner: WorldSiteOwner::Scallarian,
            world_x: 7100.0,
            world_z: 2800.0,
            enemy_count_to_liberate: 7,
            enemies_defeated: 0,
        },
        WorldSite {
            id: WorldSiteId(9),
            name: "Switchwork Borough",
            region: "Mana Switchworks",
            kind: WorldSiteKind::City,
            state: WorldSiteState::Liberated,
            owner: WorldSiteOwner::FreePeoples,
            world_x: 4400.0,
            world_z: -4300.0,
            enemy_count_to_liberate: 0,
            enemies_defeated: 0,
        },
    ]
}

// ── World Routes ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorldRouteKind {
    Road,
    MountainPath,
    SkyBridge,
    Rail,
    River,
    OceanLane,
    Tunnel,
    SpaceLane,
    DungeonGate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum WorldRouteState {
    #[default]
    Locked,
    Open,
    Contested,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldRouteId(pub u16);

#[derive(Debug, Clone)]
pub struct WorldRoute {
    pub id: WorldRouteId,
    pub from_site: WorldSiteId,
    pub to_site: WorldSiteId,
    pub kind: WorldRouteKind,
    pub state: WorldRouteState,
    /// The site that must be `Liberated` for this route to auto-open.
    pub required_site: WorldSiteId,
}

impl WorldRoute {
    pub fn is_open(&self) -> bool {
        self.state == WorldRouteState::Open
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorldRouteSaveRecord {
    pub id: u16,
    pub state: WorldRouteState,
}

#[derive(Resource, Default, Debug, Clone)]
pub struct WorldRouteRegistry {
    pub routes: Vec<WorldRoute>,
}

impl WorldRouteRegistry {
    pub fn get(&self, id: WorldRouteId) -> Option<&WorldRoute> {
        self.routes.iter().find(|r| r.id.0 == id.0)
    }
    pub fn get_mut(&mut self, id: WorldRouteId) -> Option<&mut WorldRoute> {
        self.routes.iter_mut().find(|r| r.id.0 == id.0)
    }
    pub fn to_save_records(&self) -> Vec<WorldRouteSaveRecord> {
        self.routes
            .iter()
            .map(|r| WorldRouteSaveRecord {
                id: r.id.0,
                state: r.state,
            })
            .collect()
    }
    pub fn apply_save_records(&mut self, records: &[WorldRouteSaveRecord]) {
        for record in records {
            if let Some(route) = self.routes.iter_mut().find(|r| r.id.0 == record.id) {
                route.state = record.state;
            }
        }
    }
}

pub fn initial_world_routes() -> Vec<WorldRoute> {
    vec![
        // Iron Watchpost → Riftglass Village (unlock by liberating Watchpost)
        WorldRoute {
            id: WorldRouteId(1),
            from_site: WorldSiteId(1),
            to_site: WorldSiteId(2),
            kind: WorldRouteKind::SkyBridge,
            state: WorldRouteState::Locked,
            required_site: WorldSiteId(1),
        },
        // Riftglass Village → Cloudrail City (unlock by liberating Village)
        WorldRoute {
            id: WorldRouteId(2),
            from_site: WorldSiteId(2),
            to_site: WorldSiteId(4),
            kind: WorldRouteKind::Road,
            state: WorldRouteState::Locked,
            required_site: WorldSiteId(2),
        },
        // Cloudrail City → Granite Market via Rail — starts Open (Cloudrail is pre-Liberated)
        WorldRoute {
            id: WorldRouteId(3),
            from_site: WorldSiteId(4),
            to_site: WorldSiteId(8),
            kind: WorldRouteKind::Rail,
            state: WorldRouteState::Open,
            required_site: WorldSiteId(4),
        },
        // Starfell Outpost → Frost Harbor via MountainPath
        WorldRoute {
            id: WorldRouteId(4),
            from_site: WorldSiteId(3),
            to_site: WorldSiteId(7),
            kind: WorldRouteKind::MountainPath,
            state: WorldRouteState::Locked,
            required_site: WorldSiteId(3),
        },
        // Star Orchard → Lantern Hamlet via Road
        WorldRoute {
            id: WorldRouteId(5),
            from_site: WorldSiteId(6),
            to_site: WorldSiteId(5),
            kind: WorldRouteKind::Road,
            state: WorldRouteState::Locked,
            required_site: WorldSiteId(6),
        },
        // Switchwork Borough → Frost Harbor via OceanLane — starts Open (Switchwork pre-Liberated)
        WorldRoute {
            id: WorldRouteId(6),
            from_site: WorldSiteId(9),
            to_site: WorldSiteId(7),
            kind: WorldRouteKind::OceanLane,
            state: WorldRouteState::Open,
            required_site: WorldSiteId(9),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dungeon_crawl_state_activates_and_clears() {
        let mut dungeon = DungeonCrawlState::default();
        dungeon.activate(
            "test_gate",
            ChapterId(6),
            "Collosar's Crown Gate",
            Vec3::new(-500.0, 4.0, -330.0),
            Vec3::new(-500.0, 2.0, -386.0),
            66.0,
        );

        assert!(dungeon.active);
        assert_eq!(dungeon.gate_id, Some("test_gate"));
        assert_eq!(dungeon.chapter, Some(ChapterId(6)));
        assert_eq!(dungeon.label, "Collosar's Crown Gate");
        assert_eq!(dungeon.radius, 66.0);

        dungeon.clear();

        assert!(!dungeon.active);
        assert!(dungeon.gate_id.is_none());
        assert!(dungeon.chapter.is_none());
        assert!(dungeon.label.is_empty());
    }

    #[test]
    fn player_guidance_sets_and_clears_prompt() {
        let mut guidance = PlayerGuidance::default();

        guidance.set("Talk", "P1 is near Captain Mira.", "E: Talk");
        assert!(guidance.visible);
        assert_eq!(guidance.title, "Talk");
        assert_eq!(guidance.body, "P1 is near Captain Mira.");
        assert_eq!(guidance.action, "E: Talk");

        guidance.clear();
        assert!(!guidance.visible);
        assert!(guidance.title.is_empty());
        assert!(guidance.body.is_empty());
        assert!(guidance.action.is_empty());
    }

    #[test]
    fn reference_loadout_replaces_legacy_stock_default() {
        let resolved = PlayerPartLoadout::resolve_for_hero(
            "Vincenzo",
            None,
            PlayerPartLoadout::legacy_stock(),
        );

        assert_eq!(resolved, PlayerPartLoadout::vincenzo_reference());
        assert_eq!(resolved.body, BodyPreset::ChromaFrame);
        assert_eq!(resolved.head, HeadPreset::ChromaCrown);
    }

    #[test]
    fn default_part_loadout_is_reference_not_legacy() {
        assert_eq!(
            PlayerPartLoadout::default(),
            PlayerPartLoadout::vincenzo_reference()
        );
        assert!(!PlayerPartLoadout::default().is_legacy_stock());
    }

    #[test]
    fn legacy_vincenzo_scout_loadout_is_migrated_to_reference() {
        let resolved = PlayerPartLoadout::resolve_for_hero(
            "Vincenzo",
            Some(PlayerPartLoadout::legacy_vincenzo_scout()),
            PlayerPartLoadout::legacy_stock(),
        );

        assert_eq!(resolved, PlayerPartLoadout::vincenzo_reference());
    }

    #[test]
    fn explicit_custom_loadout_still_wins_over_reference() {
        let custom = PlayerPartLoadout::antonio_reference();
        let resolved = PlayerPartLoadout::resolve_for_hero(
            "Vincenzo",
            Some(custom),
            PlayerPartLoadout::legacy_stock(),
        );

        assert_eq!(resolved, custom);
    }

    #[test]
    fn vincenzo_reference_body_uses_glb_traced_proportions() {
        let body = reference_body_recipe("Vincenzo");

        assert!(body.leg_length >= 1.45);
        assert!(body.arm_length >= 1.30);
        assert!(body.mass < 0.90);
        assert!(body.asymmetry > 0.0);
    }

    #[test]
    fn vincenzo_reference_appearance_drops_retired_hood_and_cape() {
        let appearance = reference_appearance_recipe("Vincenzo");

        assert!(!appearance.has_hood);
        assert!(!appearance.has_cape);
        assert!(appearance.has_visor);
        assert!(appearance.has_shoulder_pads);
    }

    #[test]
    fn character_design_snapshot_round_trips_editable_state() {
        let mut design = CharacterDesignData {
            face: crate::character::face::FaceRecipe {
                eye_style: crate::character::face::EyeStyle::Sharp,
                expression: crate::character::face::Expression::Determined,
                eye_size: 1.34,
                ..crate::character::face::FaceRecipe::DEFAULT
            },
            player_index: 2,
            return_target: CharacterDesignReturnTarget::PlayerSelect,
            base_model: CharacterBaseModel::AntonioRift,
            skin_idx: 3,
            outfit_idx: 4,
            accent_idx: 5,
            hair_idx: 6,
            eye_idx: 7,
            body_preset: BodyPreset::RiftMantle,
            arm_preset: ArmPreset::RiftTalons,
            leg_preset: LegPreset::RiftBoots,
            shoulder_preset: ShoulderPreset::RiftCloak,
            head_preset: HeadPreset::RiftCowl,
            body: BodyRecipe {
                arm_length: 1.22,
                leg_length: 1.41,
                asymmetry: 0.2,
                ..BodyRecipe::default()
            },
            has_hood: true,
            has_cape: true,
            has_gloves: false,
            has_boots: true,
            has_shoulder_pads: true,
            has_visor: false,
            spin_angle: 1.0,
            preview_distance: 4.0,
            dirty: false,
            preview_entity: None,
        };
        let snapshot = CharacterDesignSnapshot::from_design_data(&design);
        let json = serde_json::to_string(&snapshot).expect("snapshot should serialize");
        let decoded: CharacterDesignSnapshot =
            serde_json::from_str(&json).expect("snapshot should deserialize");

        design = CharacterDesignData::default();
        decoded.apply_to_design_data(&mut design);

        assert_eq!(design.player_index, 2);
        assert_eq!(design.base_model, CharacterBaseModel::AntonioRift);
        assert_eq!(design.body_preset, BodyPreset::RiftMantle);
        assert_eq!(design.arm_preset, ArmPreset::RiftTalons);
        assert_eq!(design.leg_preset, LegPreset::RiftBoots);
        assert_eq!(design.shoulder_preset, ShoulderPreset::RiftCloak);
        assert_eq!(design.head_preset, HeadPreset::RiftCowl);
        assert_eq!(
            design.face.eye_style,
            crate::character::face::EyeStyle::Sharp
        );
        assert_eq!(
            design.face.expression,
            crate::character::face::Expression::Determined
        );
        assert!((design.face.eye_size - 1.34).abs() < f32::EPSILON);
        assert!((design.body.leg_length - 1.41).abs() < f32::EPSILON);
        assert!(design.has_hood);
        assert!(!design.has_gloves);
        assert!(design.dirty);
    }

    #[test]
    fn legacy_character_snapshot_does_not_erase_the_current_authored_face() {
        let mut current = CharacterDesignData {
            face: crate::character::face::FaceRecipe {
                eye_style: crate::character::face::EyeStyle::Gentle,
                ..crate::character::face::FaceRecipe::DEFAULT
            },
            ..CharacterDesignData::default()
        };
        let mut legacy = CharacterDesignSnapshot::from_design_data(&current);
        legacy.schema_version = 1;
        legacy.face = crate::character::face::FaceRecipe::DEFAULT;

        legacy.apply_to_design_data(&mut current);

        assert_eq!(
            current.face.eye_style,
            crate::character::face::EyeStyle::Gentle
        );
    }

    #[test]
    fn character_base_models_map_to_editable_loadouts() {
        let catalog = CharacterBaseModelCatalog::default();
        let daria = catalog
            .by_label("Daria Flares")
            .expect("daria base model should exist");

        assert_eq!(catalog.models.len(), CharacterBaseModel::ALL.len());
        assert_eq!(daria.loadout(), PlayerPartLoadout::daria_reference());
        assert_eq!(
            CharacterBaseModel::from_loadout(PlayerPartLoadout::amp_reference()),
            CharacterBaseModel::AmpSiege
        );
        assert_eq!(
            CharacterBaseModel::from_name_and_loadout(
                "Nova",
                PlayerPartLoadout::chroma_reference()
            ),
            CharacterBaseModel::ChromaTrace
        );
        assert_eq!(
            CharacterBaseModel::from_name_and_loadout(
                "Vincenzo",
                PlayerPartLoadout::chroma_reference()
            ),
            CharacterBaseModel::VincenzoDeep
        );
    }

    #[test]
    fn shop_catalog_covers_core_customization_categories() {
        let catalog = ShopCatalog::default();

        for category in ShopCategory::ALL {
            assert!(
                catalog.category_count(category) > 0,
                "{} should have at least one shop item",
                category.label()
            );
        }
        assert!(catalog
            .items_for(ShopCategory::Outfits)
            .any(|item| item.preview_loadout.is_some()));
    }

    #[test]
    fn world_site_ids_are_unique() {
        let sites = initial_world_sites();
        let mut ids: Vec<u16> = sites.iter().map(|s| s.id.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), sites.len(), "duplicate WorldSiteId found");
    }

    #[test]
    fn world_site_count_matches_settlements_plus_standalone() {
        // 8 settlements + 1 standalone (Iron Watchpost) = 9
        let sites = initial_world_sites();
        assert_eq!(sites.len(), 9);
    }

    #[test]
    fn liberated_cities_have_no_defenders() {
        for site in initial_world_sites() {
            if site.state == WorldSiteState::Liberated {
                assert_eq!(
                    site.enemy_count_to_liberate, 0,
                    "Liberated site '{}' should have 0 defenders",
                    site.name
                );
            }
        }
    }

    #[test]
    fn enemy_held_sites_have_positive_defender_count() {
        for site in initial_world_sites() {
            if site.state == WorldSiteState::EnemyHeld {
                assert!(
                    site.enemy_count_to_liberate > 0,
                    "EnemyHeld site '{}' needs at least 1 defender",
                    site.name
                );
            }
        }
    }

    #[test]
    fn world_site_registry_apply_records_round_trip() {
        let mut registry = WorldSiteRegistry {
            sites: initial_world_sites(),
        };

        // Simulate: liberate site 2
        let records = vec![WorldSiteSaveRecord {
            id: 2,
            state: WorldSiteState::Liberated,
            owner: WorldSiteOwner::PlayerAlliance,
            enemies_defeated: 8,
        }];
        registry.apply_save_records(&records);

        let site = registry.get(WorldSiteId(2)).unwrap();
        assert!(site.is_liberated());
        assert_eq!(site.owner, WorldSiteOwner::PlayerAlliance);
        assert_eq!(site.enemies_defeated, 8);

        // Other sites unaffected
        assert_eq!(
            registry.get(WorldSiteId(1)).unwrap().state,
            WorldSiteState::EnemyHeld
        );
    }

    #[test]
    fn cave_fast_travel_destination_is_one_shot_and_clearable() {
        let mut destination = FastTravelDestination::default();
        destination.cave("secret_cave_ch03", "Sister Starwell Cave");
        assert_eq!(destination.anchor_id, Some("secret_cave_ch03"));
        assert_eq!(destination.label, Some("Sister Starwell Cave"));
        assert!(destination.enter_dungeon);
        destination.world_anchor("race_region_north_gate", "Grand Raceway");
        assert_eq!(destination.anchor_id, Some("race_region_north_gate"));
        assert_eq!(destination.label, Some("Grand Raceway"));
        assert!(!destination.enter_dungeon);
        destination.clear();
        assert!(destination.anchor_id.is_none());
        assert!(destination.label.is_none());
        assert!(!destination.enter_dungeon);
    }

    #[test]
    fn every_authored_chapter_fast_travel_point_is_open_on_a_new_save() {
        let progress = ChapterProgress::default();
        for chapter in ChapterId::FIRST.0..=ChapterId::LAST.0 {
            assert!(progress.is_fast_travel_unlocked(ChapterId(chapter)));
        }
        assert!(!progress.is_fast_travel_unlocked(ChapterId(0)));
        assert!(!progress.is_fast_travel_unlocked(ChapterId(15)));
        assert!(!progress.is_unlocked(ChapterId::LAST));
    }
}
