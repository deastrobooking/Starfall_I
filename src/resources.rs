use bevy::prelude::*;

use crate::chapters::{Biome, ChapterId};
use crate::character_blueprint::{BodyRecipe, CharacterBlueprint};
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

// ── Chapter Progress (saveable) ───────────────────────────────────────────────
#[derive(Resource, Debug, Default, Clone)]
pub struct ChapterProgress {
    pub completed: Vec<u8>,
    pub discoverables: Vec<String>,
    pub companions_recruited: Vec<String>,
    pub scientist_relics: Vec<String>,
    pub relic_fragments: Vec<String>,
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
    pub outfit_idx: usize,
    pub accent_idx: usize,
    pub hair_idx: usize,
    pub body: BodyRecipe,
    pub has_hood: bool,
    pub has_cape: bool,
    pub has_gloves: bool,
    pub has_boots: bool,
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
            body: BodyRecipe::default(),
            has_hood: true,
            has_cape: true,
            has_gloves: true,
            has_boots: true,
            has_shoulder_pads: false,
            has_visor: false,
            spin_angle: 0.0,
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
    pub outfit_idx: Option<usize>,
    pub accent_idx: Option<usize>,
    pub hair_idx: Option<usize>,
    pub has_hood: Option<bool>,
    pub has_cape: Option<bool>,
    pub has_gloves: Option<bool>,
    pub has_boots: Option<bool>,
    pub has_shoulder_pads: Option<bool>,
    pub has_visor: Option<bool>,
    pub blueprint: Option<CharacterBlueprint>,
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
            has_hood: None,
            has_cape: None,
            has_gloves: None,
            has_boots: None,
            has_shoulder_pads: None,
            has_visor: None,
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
}
