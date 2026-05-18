use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::character_blueprint::CharacterBlueprint;
use crate::components::player::{Player, PlayerIndex, PlayerStats};
use crate::damage::Health;
use crate::events::UiMessageEvent;
use crate::perks::PerkTree;
use crate::resources::{ChapterProgress, PlaySessionTransition, PlayerSelectState, WaveInfo};
use crate::state::AppState;

const SAVE_FILE: &str = "starfall_i_save.json";

// ── Save Data ─────────────────────────────────────────────────────────────────
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SaveData {
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub experience: u32,
    #[serde(default)]
    pub credits: u32,
    #[serde(default = "default_max_stat")]
    pub max_health: f32,
    #[serde(default = "default_max_stat")]
    pub max_stamina: f32,
    #[serde(default = "default_max_stat")]
    pub max_armor: f32,
    pub wave_number: u32,
    pub completed_chapters: Vec<u8>,
    pub discoverables: Vec<String>,
    pub companions_recruited: Vec<String>,
    pub scientist_relics: Vec<String>,
    #[serde(default)]
    pub relic_fragments: Vec<String>,
    #[serde(default)]
    pub perk_points_unspent: u32,
    #[serde(default)]
    pub perk_ranks: Vec<(String, u32)>,
    #[serde(default)]
    pub character_blueprints: Vec<Option<CharacterBlueprint>>,
    #[serde(default)]
    pub players: Vec<PlayerSaveData>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerSaveData {
    pub player_index: u8,
    pub level: u32,
    pub experience: u32,
    pub credits: u32,
    pub health_current: f32,
    pub health_max: f32,
    pub stamina: f32,
    pub max_stamina: f32,
    pub armor: f32,
    pub max_armor: f32,
}

impl PlayerSaveData {
    fn from_runtime(player_index: u8, stats: &PlayerStats, health: &Health) -> Self {
        Self {
            player_index,
            level: stats.level,
            experience: stats.experience,
            credits: stats.credits,
            health_current: health.current,
            health_max: health.max,
            stamina: stats.stamina,
            max_stamina: stats.max_stamina,
            armor: stats.armor,
            max_armor: stats.max_armor,
        }
    }

    fn legacy(data: &SaveData, player_index: u8) -> Self {
        Self {
            player_index,
            level: data.level.max(1),
            experience: data.experience,
            credits: data.credits,
            health_current: data.max_health,
            health_max: data.max_health,
            stamina: data.max_stamina,
            max_stamina: data.max_stamina,
            armor: data.max_armor,
            max_armor: data.max_armor,
        }
    }

    fn apply_to(&self, stats: &mut PlayerStats, health: &mut Health) {
        stats.level = self.level.max(1);
        stats.experience = self.experience;
        stats.credits = self.credits;
        stats.max_health = self.health_max.max(1.0);
        stats.max_stamina = self.max_stamina.max(1.0);
        stats.stamina = self.stamina.clamp(0.0, stats.max_stamina);
        stats.max_armor = self.max_armor.max(1.0);
        stats.armor = self.armor.clamp(0.0, stats.max_armor);
        health.max = stats.max_health;
        health.current = self.health_current.clamp(0.0, health.max);
    }
}

impl Default for SaveData {
    fn default() -> Self {
        Self {
            level: 1,
            experience: 0,
            credits: 0,
            max_health: 100.0,
            max_stamina: 100.0,
            max_armor: 100.0,
            wave_number: 1,
            completed_chapters: Vec::new(),
            discoverables: Vec::new(),
            companions_recruited: Vec::new(),
            scientist_relics: Vec::new(),
            relic_fragments: Vec::new(),
            perk_points_unspent: 0,
            perk_ranks: Vec::new(),
            character_blueprints: vec![None, None, None, None],
            players: Vec::new(),
        }
    }
}

fn default_max_stat() -> f32 {
    100.0
}

// ── Resource ──────────────────────────────────────────────────────────────────
#[derive(Resource)]
pub struct SaveState {
    pub last_save_timer: f32,
    pub autosave_interval: f32,
}

impl Default for SaveState {
    fn default() -> Self {
        Self {
            last_save_timer: 0.0,
            autosave_interval: 30.0,
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────────
pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveState>()
            .add_systems(Startup, hydrate_progress_from_disk)
            .add_systems(OnEnter(AppState::Playing), load_save_on_enter)
            .add_systems(
                Update,
                (autosave_system, manual_save_system).run_if(in_state(AppState::Playing)),
            );
    }
}

// ── Save Path ─────────────────────────────────────────────────────────────────
fn save_path() -> PathBuf {
    PathBuf::from(SAVE_FILE)
}

// ── Save ──────────────────────────────────────────────────────────────────────
pub fn save_game(
    players: Vec<PlayerSaveData>,
    wave: &WaveInfo,
    progress: &ChapterProgress,
    perks: &PerkTree,
    select: &PlayerSelectState,
) -> Result<(), String> {
    let data = SaveData {
        wave_number: wave.wave_number,
        completed_chapters: progress.completed.clone(),
        discoverables: progress.discoverables.clone(),
        companions_recruited: progress.companions_recruited.clone(),
        scientist_relics: progress.scientist_relics.clone(),
        relic_fragments: progress.relic_fragments.clone(),
        perk_points_unspent: perks.points_unspent,
        perk_ranks: perks.ranks.clone(),
        character_blueprints: select
            .slots
            .iter()
            .map(|slot| slot.blueprint.clone())
            .collect(),
        players,
        ..SaveData::default()
    };
    let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    fs::write(save_path(), json).map_err(|e| e.to_string())
}

pub fn load_save() -> Option<SaveData> {
    let path = save_path();
    if !path.exists() {
        return None;
    }
    let json = fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

fn hydrate_character_blueprints(
    select: &mut PlayerSelectState,
    blueprints: Vec<Option<CharacterBlueprint>>,
) {
    for (slot, blueprint) in select.slots.iter_mut().zip(blueprints.into_iter()) {
        slot.blueprint = blueprint;
    }
}

fn player_save_for(data: &SaveData, player_index: u8) -> Option<PlayerSaveData> {
    if data.players.is_empty() {
        Some(PlayerSaveData::legacy(data, player_index))
    } else {
        data.players
            .iter()
            .find(|player| player.player_index == player_index)
            .cloned()
    }
}

fn collect_player_saves(
    player_q: &Query<(&PlayerIndex, &PlayerStats, &Health), With<Player>>,
) -> Vec<PlayerSaveData> {
    let mut players: Vec<_> = player_q
        .iter()
        .map(|(index, stats, health)| PlayerSaveData::from_runtime(index.0, stats, health))
        .collect();
    players.sort_by_key(|player| player.player_index);
    players
}

// ── Systems ───────────────────────────────────────────────────────────────────
fn hydrate_progress_from_disk(
    mut progress: ResMut<ChapterProgress>,
    mut perks: ResMut<PerkTree>,
    mut select: ResMut<PlayerSelectState>,
) {
    if let Some(data) = load_save() {
        progress.completed = data.completed_chapters;
        progress.discoverables = data.discoverables;
        progress.companions_recruited = data.companions_recruited;
        progress.scientist_relics = data.scientist_relics;
        progress.relic_fragments = data.relic_fragments;
        perks.points_unspent = data.perk_points_unspent;
        perks.ranks = data.perk_ranks;
        hydrate_character_blueprints(&mut select, data.character_blueprints);
    }
}

fn load_save_on_enter(
    mut player_q: Query<(&PlayerIndex, &mut PlayerStats, &mut Health), With<Player>>,
    mut wave: ResMut<WaveInfo>,
    mut progress: ResMut<ChapterProgress>,
    mut perks: ResMut<PerkTree>,
    mut select: ResMut<PlayerSelectState>,
    transition: Res<PlaySessionTransition>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    if transition.resuming_from_pause {
        return;
    }

    if let Some(data) = load_save() {
        let mut active_players = 0usize;
        for (index, mut stats, mut health) in player_q.iter_mut() {
            active_players += 1;
            if let Some(saved) = player_save_for(&data, index.0) {
                saved.apply_to(&mut stats, &mut health);
            }
        }
        wave.wave_number = data.wave_number;
        progress.completed = data.completed_chapters;
        progress.discoverables = data.discoverables;
        progress.companions_recruited = data.companions_recruited;
        progress.scientist_relics = data.scientist_relics;
        progress.relic_fragments = data.relic_fragments;
        perks.points_unspent = data.perk_points_unspent;
        perks.ranks = data.perk_ranks;
        hydrate_character_blueprints(&mut select, data.character_blueprints);
        let loaded_players = data.players.len().max(active_players);
        msg_ev.send(UiMessageEvent {
            text: format!(
                "Save loaded — {} player{} Rift {}",
                loaded_players,
                if loaded_players == 1 { "" } else { "s" },
                data.wave_number
            ),
            duration: 3.0,
        });
    }
}

fn autosave_system(
    time: Res<Time>,
    mut save_state: ResMut<SaveState>,
    player_q: Query<(&PlayerIndex, &PlayerStats, &Health), With<Player>>,
    wave: Res<WaveInfo>,
    progress: Res<ChapterProgress>,
    perks: Res<PerkTree>,
    select: Res<PlayerSelectState>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    save_state.last_save_timer += time.delta_secs();
    if save_state.last_save_timer < save_state.autosave_interval {
        return;
    }
    save_state.last_save_timer = 0.0;

    let players = collect_player_saves(&player_q);
    if players.is_empty() {
        return;
    };
    match save_game(players, &wave, &progress, &perks, &select) {
        Ok(()) => {
            msg_ev.send(UiMessageEvent {
                text: "Game autosaved.".to_string(),
                duration: 1.5,
            });
        }
        Err(e) => {
            warn!("Autosave failed: {}", e);
        }
    }
}

fn manual_save_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    player_q: Query<(&PlayerIndex, &PlayerStats, &Health), With<Player>>,
    wave: Res<WaveInfo>,
    progress: Res<ChapterProgress>,
    perks: Res<PerkTree>,
    select: Res<PlayerSelectState>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }
    let players = collect_player_saves(&player_q);
    if players.is_empty() {
        return;
    };
    match save_game(players, &wave, &progress, &perks, &select) {
        Ok(()) => {
            msg_ev.send(UiMessageEvent {
                text: "Game saved! [F5]".to_string(),
                duration: 2.0,
            });
        }
        Err(e) => {
            msg_ev.send(UiMessageEvent {
                text: format!("Save failed: {}", e),
                duration: 2.0,
            });
        }
    }
}
