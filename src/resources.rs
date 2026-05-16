use bevy::prelude::*;

use crate::chapters::{Biome, ChapterId};
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
#[derive(Resource, Debug)]
pub struct GameSettings {
    pub mouse_sensitivity: f32,
    pub master_volume: f32,
    pub show_damage_numbers: bool,
    pub world_seed: u64,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 0.0008,
            master_volume: 1.0,
            show_damage_numbers: true,
            world_seed: 42_195,
        }
    }
}

// ── UI Message Queue ──────────────────────────────────────────────────────────
#[derive(Resource, Debug, Default)]
pub struct UiMessage {
    pub text: String,
    pub timer: f32,
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
// Global resource; all player cameras share the same shake pool.
#[derive(Resource, Debug, Default)]
pub struct CameraShake {
    pub trauma: f32,
}

impl CameraShake {
    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
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

// ── Chapter Progress (saveable) ───────────────────────────────────────────────
#[derive(Resource, Debug, Default, Clone)]
pub struct ChapterProgress {
    pub completed: Vec<u8>,
    pub discoverables: Vec<String>,
    pub companions_recruited: Vec<String>,
    pub scientist_relics: Vec<String>,
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
    pub outfit_idx: usize,
    pub accent_idx: usize,
    pub hair_idx: usize,
    pub has_cape: bool,
    pub has_shoulder_pads: bool,
    pub has_visor: bool,
    pub spin_angle: f32,
    pub dirty: bool,
    pub preview_entity: Option<Entity>,
}

impl Default for CharacterDesignData {
    fn default() -> Self {
        Self {
            player_index: 0,
            outfit_idx: 0,
            accent_idx: 0,
            hair_idx: 0,
            has_cape: true,
            has_shoulder_pads: false,
            has_visor: false,
            spin_angle: 0.0,
            dirty: false,
            preview_entity: None,
        }
    }
}

// ── Hero Roster ───────────────────────────────────────────────────────────────
pub const HERO_ROSTER: [&str; 4] = ["Vincenzo", "Antonio", "Angelo", "Joseph"];

// ── Player Select Lobby ───────────────────────────────────────────────────────
#[derive(Clone)]
pub struct PlayerSlotConfig {
    pub joined: bool,
    pub character_index: usize,
    pub ready: bool,
    pub stick_cooldown: f32,
    // Customization overrides — None means use the hero's default
    pub outfit_idx: Option<usize>,
    pub accent_idx: Option<usize>,
    pub hair_idx: Option<usize>,
    pub has_cape: Option<bool>,
    pub has_shoulder_pads: Option<bool>,
    pub has_visor: Option<bool>,
}

impl Default for PlayerSlotConfig {
    fn default() -> Self {
        Self {
            joined: false,
            character_index: 0,
            ready: false,
            stick_cooldown: 0.0,
            outfit_idx: None,
            accent_idx: None,
            hair_idx: None,
            has_cape: None,
            has_shoulder_pads: None,
            has_visor: None,
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
        let idx = self.slots.get(slot).map(|s| s.character_index).unwrap_or(slot % 4);
        HERO_ROSTER[idx]
    }
}
