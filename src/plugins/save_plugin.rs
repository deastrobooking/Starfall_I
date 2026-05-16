use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::components::player::{Player, PlayerStats};
use crate::damage::Health;
use crate::events::UiMessageEvent;
use crate::perks::PerkTree;
use crate::resources::{ChapterProgress, WaveInfo};
use crate::state::AppState;

const SAVE_FILE: &str = "starfall_i_save.json";

// ── Save Data ─────────────────────────────────────────────────────────────────
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SaveData {
    pub level: u32,
    pub experience: u32,
    pub credits: u32,
    pub max_health: f32,
    pub max_stamina: f32,
    pub max_armor: f32,
    pub wave_number: u32,
    pub completed_chapters: Vec<u8>,
    pub discoverables: Vec<String>,
    pub companions_recruited: Vec<String>,
    pub scientist_relics: Vec<String>,
    #[serde(default)]
    pub perk_points_unspent: u32,
    #[serde(default)]
    pub perk_ranks: Vec<(String, u32)>,
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
            perk_points_unspent: 0,
            perk_ranks: Vec::new(),
        }
    }
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
    stats: &PlayerStats,
    health: &Health,
    wave: &WaveInfo,
    progress: &ChapterProgress,
    perks: &PerkTree,
) -> Result<(), String> {
    let data = SaveData {
        level: stats.level,
        experience: stats.experience,
        credits: stats.credits,
        max_health: health.max,
        max_stamina: stats.max_stamina,
        max_armor: stats.max_armor,
        wave_number: wave.wave_number,
        completed_chapters: progress.completed.clone(),
        discoverables: progress.discoverables.clone(),
        companions_recruited: progress.companions_recruited.clone(),
        scientist_relics: progress.scientist_relics.clone(),
        perk_points_unspent: perks.points_unspent,
        perk_ranks: perks.ranks.clone(),
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

// ── Systems ───────────────────────────────────────────────────────────────────
fn hydrate_progress_from_disk(mut progress: ResMut<ChapterProgress>, mut perks: ResMut<PerkTree>) {
    if let Some(data) = load_save() {
        progress.completed = data.completed_chapters;
        progress.discoverables = data.discoverables;
        progress.companions_recruited = data.companions_recruited;
        progress.scientist_relics = data.scientist_relics;
        perks.points_unspent = data.perk_points_unspent;
        perks.ranks = data.perk_ranks;
    }
}

fn load_save_on_enter(
    mut player_q: Query<(&mut PlayerStats, &mut Health), With<Player>>,
    mut wave: ResMut<WaveInfo>,
    mut progress: ResMut<ChapterProgress>,
    mut perks: ResMut<PerkTree>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    if let Some(data) = load_save() {
        for (mut stats, mut health) in player_q.iter_mut() {
            stats.level = data.level;
            stats.experience = data.experience;
            stats.credits = data.credits;
            stats.max_health = data.max_health;
            stats.max_stamina = data.max_stamina;
            stats.max_armor = data.max_armor;
            health.max = data.max_health;
            health.current = data.max_health;
        }
        wave.wave_number = data.wave_number;
        progress.completed = data.completed_chapters;
        progress.discoverables = data.discoverables;
        progress.companions_recruited = data.companions_recruited;
        progress.scientist_relics = data.scientist_relics;
        perks.points_unspent = data.perk_points_unspent;
        perks.ranks = data.perk_ranks;
        msg_ev.send(UiMessageEvent {
            text: format!("Save loaded — LVL {} Wave {}", data.level, data.wave_number),
            duration: 3.0,
        });
    }
}

fn autosave_system(
    time: Res<Time>,
    mut save_state: ResMut<SaveState>,
    player_q: Query<(&PlayerStats, &Health), With<Player>>,
    wave: Res<WaveInfo>,
    progress: Res<ChapterProgress>,
    perks: Res<PerkTree>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    save_state.last_save_timer += time.delta_secs();
    if save_state.last_save_timer < save_state.autosave_interval {
        return;
    }
    save_state.last_save_timer = 0.0;

    let Some((stats, health)) = player_q.iter().next() else {
        return;
    };
    match save_game(stats, health, &wave, &progress, &perks) {
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
    player_q: Query<(&PlayerStats, &Health), With<Player>>,
    wave: Res<WaveInfo>,
    progress: Res<ChapterProgress>,
    perks: Res<PerkTree>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }
    let Some((stats, health)) = player_q.iter().next() else {
        return;
    };
    match save_game(stats, health, &wave, &progress, &perks) {
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
