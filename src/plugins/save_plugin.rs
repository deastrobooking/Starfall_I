use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::components::player::{Player, PlayerStats};
use crate::damage::Health;
use crate::events::UiMessageEvent;
use crate::resources::WaveInfo;
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
        }
    }
}

// ── Resource ──────────────────────────────────────────────────────────────────
#[derive(Resource, Default)]
pub struct SaveState {
    pub last_save_timer: f32,
    pub autosave_interval: f32,
}

impl SaveState {
    pub fn new() -> Self {
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
pub fn save_game(stats: &PlayerStats, health: &Health, wave: &WaveInfo) -> Result<(), String> {
    let data = SaveData {
        level: stats.level,
        experience: stats.experience,
        credits: stats.credits,
        max_health: health.max,
        max_stamina: stats.max_stamina,
        max_armor: stats.max_armor,
        wave_number: wave.wave_number,
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
fn load_save_on_enter(
    mut player_q: Query<(&mut PlayerStats, &mut Health), With<Player>>,
    mut wave: ResMut<WaveInfo>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    if let Some(data) = load_save() {
        if let Ok((mut stats, mut health)) = player_q.get_single_mut() {
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
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    save_state.last_save_timer += time.delta_secs();
    if save_state.last_save_timer < save_state.autosave_interval {
        return;
    }
    save_state.last_save_timer = 0.0;

    let Ok((stats, health)) = player_q.get_single() else {
        return;
    };
    match save_game(stats, health, &wave) {
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
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }
    let Ok((stats, health)) = player_q.get_single() else {
        return;
    };
    match save_game(stats, health, &wave) {
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
