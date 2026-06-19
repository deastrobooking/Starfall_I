use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::chapters::{Biome, ChapterId};
use crate::character_blueprint::{BodyRecipe, CharacterBlueprint};
use crate::character_parts::{ArmPreset, BodyPreset, HeadPreset, LegPreset, ShoulderPreset};
use crate::hero_roster::HERO_NAMES;
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

// ── Camera Shake ──────────────────────────────────────────────────────────────
// Trauma model: trauma decays over time, shake magnitude = trauma^2.
// `trauma` is a global fallback, while `per_player` drives split-screen cameras.
#[derive(Resource, Debug)]
pub struct CameraShake {
    pub trauma: f32,
    pub per_player: [f32; 4],
}

impl Default for CameraShake {
    fn default() -> Self {
        Self {
            trauma: 0.0,
            per_player: [0.0; 4],
        }
    }
}

impl CameraShake {
    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }

    pub fn add_player_trauma(&mut self, player_index: u8, amount: f32) {
        let slot = usize::from(player_index.min(3));
        self.per_player[slot] = (self.per_player[slot] + amount).min(1.0);
    }

    pub fn trauma_for(&self, player_index: u8) -> f32 {
        self.per_player[usize::from(player_index.min(3))].max(self.trauma)
    }

    pub fn decay(&mut self, amount: f32) {
        self.trauma = (self.trauma - amount).max(0.0);
        for trauma in &mut self.per_player {
            *trauma = (*trauma - amount).max(0.0);
        }
    }
}

// ── Play Session Transitions ─────────────────────────────────────────────────
/// Tracks pause/resume transitions so `OnEnter(Playing)` setup systems do not
/// rebuild the active chapter when returning from the pause menu.
#[derive(Resource, Debug, Default)]
pub struct PlaySessionTransition {
    pub pausing: bool,
    pub resuming_from_pause: bool,
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
}

// ── Dungeon Crawl Mode ───────────────────────────────────────────────────────
#[derive(Resource, Debug, Clone)]
pub struct DungeonCrawlState {
    pub active: bool,
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
        chapter: ChapterId,
        label: impl Into<String>,
        focus: Vec3,
        anchor: Vec3,
        radius: f32,
    ) {
        self.active = true;
        self.chapter = Some(chapter);
        self.label = label.into();
        self.focus = focus;
        self.anchor = anchor;
        self.radius = radius.max(28.0);
    }

    pub fn clear(&mut self) {
        self.active = false;
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

/// Per-slot preset choices, persisted between the chassis editor and gameplay.
/// Applied to `CharacterLoadout` when the player character spawns.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerPartLoadout {
    pub body: BodyPreset,
    pub arms: ArmPreset,
    pub legs: LegPreset,
    pub shoulders: ShoulderPreset,
    pub head: HeadPreset,
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
        self.is_completed(ChapterId(id.0 - 1))
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
/// Transient resource holding the in-progress customization for one player slot.
/// Set `player_index` before transitioning to `AppState::CharacterDesign`.
#[derive(Resource, Debug)]
pub struct CharacterDesignData {
    pub player_index: usize,
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
        Self {
            player_index: 0,
            skin_idx: 0,
            outfit_idx: 0,
            accent_idx: 0,
            hair_idx: 0,
            eye_idx: 0,
            body_preset: BodyPreset::default(),
            arm_preset: ArmPreset::default(),
            leg_preset: LegPreset::default(),
            shoulder_preset: ShoulderPreset::default(),
            head_preset: HeadPreset::default(),
            body: BodyRecipe::default(),
            has_hood: true,
            has_cape: true,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: false,
            has_visor: false,
            spin_angle: 0.0,
            preview_distance: 3.2,
            dirty: false,
            preview_entity: None,
        }
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
            ChapterId(6),
            "Collosar's Crown Gate",
            Vec3::new(-500.0, 4.0, -330.0),
            Vec3::new(-500.0, 2.0, -386.0),
            66.0,
        );

        assert!(dungeon.active);
        assert_eq!(dungeon.chapter, Some(ChapterId(6)));
        assert_eq!(dungeon.label, "Collosar's Crown Gate");
        assert_eq!(dungeon.radius, 66.0);

        dungeon.clear();

        assert!(!dungeon.active);
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
}
