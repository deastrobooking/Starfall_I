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

// ── Hero Roster ───────────────────────────────────────────────────────────────
pub const HERO_ROSTER: [&str; 4] = ["Vincenzo", "Antonio", "Angelo", "Joseph"];

// ── Player Select Lobby ───────────────────────────────────────────────────────
#[derive(Clone)]
pub struct PlayerSlotConfig {
    pub joined: bool,
    pub character_index: usize,
    pub ready: bool,
    pub stick_cooldown: f32,
}

impl Default for PlayerSlotConfig {
    fn default() -> Self {
        Self { joined: false, character_index: 0, ready: false, stick_cooldown: 0.0 }
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
