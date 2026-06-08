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
use crate::robot_pets::RobotPetCollection;
use crate::state::AppState;
use crate::upgrades::UpgradeLedger;

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
    #[serde(default)]
    pub robot_pets: RobotPetCollection,
    #[serde(default)]
    pub tech_upgrades: UpgradeLedger,
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
            robot_pets: RobotPetCollection::default(),
            tech_upgrades: UpgradeLedger::default(),
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
    robot_pets: &RobotPetCollection,
    upgrades: &UpgradeLedger,
) -> Result<(), String> {
    let data = build_save_data(players, wave, progress, perks, select, robot_pets, upgrades);
    let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    fs::write(save_path(), json).map_err(|e| e.to_string())
}

fn build_save_data(
    mut players: Vec<PlayerSaveData>,
    wave: &WaveInfo,
    progress: &ChapterProgress,
    perks: &PerkTree,
    select: &PlayerSelectState,
    robot_pets: &RobotPetCollection,
    upgrades: &UpgradeLedger,
) -> SaveData {
    players.sort_by_key(|player| player.player_index);
    SaveData {
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
        robot_pets: robot_pets.clone(),
        tech_upgrades: upgrades.clone(),
        ..SaveData::default()
    }
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
    for (slot, blueprint) in select.slots.iter_mut().zip(blueprints) {
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

pub fn save_current_session(
    player_q: &Query<(&PlayerIndex, &PlayerStats, &Health), With<Player>>,
    wave: &WaveInfo,
    progress: &ChapterProgress,
    perks: &PerkTree,
    select: &PlayerSelectState,
    robot_pets: &RobotPetCollection,
    upgrades: &UpgradeLedger,
) -> Result<(), String> {
    let players = collect_player_saves(player_q);
    if players.is_empty() {
        return Err("No active players to save".to_string());
    }
    save_game(players, wave, progress, perks, select, robot_pets, upgrades)
}

// ── Systems ───────────────────────────────────────────────────────────────────
fn hydrate_progress_from_disk(
    mut progress: ResMut<ChapterProgress>,
    mut perks: ResMut<PerkTree>,
    mut select: ResMut<PlayerSelectState>,
    mut robot_pets: ResMut<RobotPetCollection>,
    mut upgrades: ResMut<UpgradeLedger>,
) {
    if let Some(data) = load_save() {
        *robot_pets = data.robot_pets.clone();
        *upgrades = data.tech_upgrades.clone();
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
    mut robot_pets: ResMut<RobotPetCollection>,
    mut upgrades: ResMut<UpgradeLedger>,
    transition: Res<PlaySessionTransition>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if transition.resuming_from_pause {
        return;
    }

    if let Some(data) = load_save() {
        *robot_pets = data.robot_pets.clone();
        *upgrades = data.tech_upgrades.clone();
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
        msg_ev.write(UiMessageEvent {
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
    robot_pets: Res<RobotPetCollection>,
    upgrades: Res<UpgradeLedger>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    save_state.last_save_timer += time.delta_secs();
    if save_state.last_save_timer < save_state.autosave_interval {
        return;
    }
    save_state.last_save_timer = 0.0;

    match save_current_session(
        &player_q,
        &wave,
        &progress,
        &perks,
        &select,
        &robot_pets,
        &upgrades,
    ) {
        Ok(()) => {
            msg_ev.write(UiMessageEvent {
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
    robot_pets: Res<RobotPetCollection>,
    upgrades: Res<UpgradeLedger>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }
    match save_current_session(
        &player_q,
        &wave,
        &progress,
        &perks,
        &select,
        &robot_pets,
        &upgrades,
    ) {
        Ok(()) => {
            msg_ev.write(UiMessageEvent {
                text: "Game saved! [F5]".to_string(),
                duration: 2.0,
            });
        }
        Err(e) => {
            msg_ev.write(UiMessageEvent {
                text: format!("Save failed: {}", e),
                duration: 2.0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character_blueprint::{BodyRecipe, CartoonAppearanceRecipe, CharacterPaletteRecipe};
    use crate::robot_pets::{RobotPartKind, RobotPetBlueprint, RobotPetRole};
    use crate::upgrades::{TechUpgradeId, UpgradeLedger};

    fn player_save(
        player_index: u8,
        level: u32,
        experience: u32,
        credits: u32,
        health_current: f32,
        health_max: f32,
    ) -> PlayerSaveData {
        PlayerSaveData {
            player_index,
            level,
            experience,
            credits,
            health_current,
            health_max,
            stamina: 40.0 + f32::from(player_index),
            max_stamina: 120.0 + f32::from(player_index),
            armor: 10.0 + f32::from(player_index),
            max_armor: 90.0 + f32::from(player_index),
        }
    }

    fn test_blueprint(name: &str, height: f32) -> CharacterBlueprint {
        let body = BodyRecipe {
            height,
            ..BodyRecipe::default()
        };
        CharacterBlueprint::hero(
            name,
            body,
            CharacterPaletteRecipe {
                skin: Color::srgb(0.95, 0.72, 0.55),
                outfit: Color::srgb(0.09, 0.28, 0.80),
                accent: Color::srgb(1.0, 0.86, 0.20),
                hair: Color::srgb(0.12, 0.08, 0.05),
                eye: Color::srgb(0.25, 0.95, 1.0),
            },
            CartoonAppearanceRecipe::default(),
        )
    }

    #[test]
    fn build_save_data_sorts_players_and_preserves_shared_state() {
        let wave = WaveInfo {
            wave_number: 7,
            ..WaveInfo::default()
        };
        let progress = ChapterProgress {
            completed: vec![1, 2, 3],
            discoverables: vec!["cave:starfall_lab".to_string()],
            companions_recruited: vec!["Nova".to_string()],
            scientist_relics: vec!["Giacoma:focus_lens".to_string()],
            relic_fragments: vec!["Giovanni:caliper:3".to_string()],
        };
        let perks = PerkTree {
            points_unspent: 2,
            ranks: vec![("star_focus".to_string(), 3)],
        };
        let mut select = PlayerSelectState::default();
        select.slots[0].blueprint = Some(test_blueprint("P1", 1.0));
        select.slots[2].blueprint = Some(test_blueprint("P3", 1.12));
        let mut robot_pets = RobotPetCollection::default();
        robot_pets.add_part(RobotPartKind::CircuitBoard, 6);
        robot_pets.rescue_pet(RobotPetBlueprint::rescued(
            "spark-pup",
            "Spark Pup",
            RobotPetRole::Scout,
        ));
        let mut upgrades = UpgradeLedger::default();
        upgrades.ranks.push((TechUpgradeId::BeamCapacitors, 2));
        upgrades.rejuvenation_charge = 75.0;

        let data = build_save_data(
            vec![
                player_save(2, 8, 700, 90, 44.0, 150.0),
                player_save(0, 4, 300, 20, 88.0, 110.0),
            ],
            &wave,
            &progress,
            &perks,
            &select,
            &robot_pets,
            &upgrades,
        );

        assert_eq!(data.wave_number, 7);
        assert_eq!(data.completed_chapters, vec![1, 2, 3]);
        assert_eq!(data.discoverables, vec!["cave:starfall_lab"]);
        assert_eq!(data.perk_points_unspent, 2);
        assert_eq!(data.perk_ranks, vec![("star_focus".to_string(), 3)]);
        assert_eq!(data.players[0].player_index, 0);
        assert_eq!(data.players[1].player_index, 2);
        assert_eq!(
            data.character_blueprints[0]
                .as_ref()
                .map(|blueprint| blueprint.name.as_str()),
            Some("P1")
        );
        assert_eq!(
            data.character_blueprints[2]
                .as_ref()
                .map(|blueprint| blueprint.name.as_str()),
            Some("P3")
        );
        assert_eq!(data.robot_pets.part_count(RobotPartKind::CircuitBoard), 6);
        assert_eq!(data.robot_pets.pets[0].id, "spark-pup");
        assert_eq!(data.tech_upgrades.rank(TechUpgradeId::BeamCapacitors), 2);
        assert_eq!(data.tech_upgrades.rejuvenation_charge, 75.0);
    }

    #[test]
    fn save_data_round_trip_preserves_per_player_records() {
        let mut robot_pets = RobotPetCollection::default();
        robot_pets.add_part(RobotPartKind::StarDrive, 2);
        robot_pets.rescue_pet(RobotPetBlueprint::rescued(
            "nova-kit",
            "Nova Kit",
            RobotPetRole::Pilot,
        ));
        let mut tech_upgrades = UpgradeLedger::default();
        tech_upgrades
            .ranks
            .push((TechUpgradeId::RejuvenationMatrix, 1));
        tech_upgrades.rejuvenation_charge = 120.0;

        let data = SaveData {
            players: vec![
                player_save(0, 2, 125, 10, 80.0, 100.0),
                player_save(1, 5, 500, 60, 45.0, 130.0),
            ],
            character_blueprints: vec![
                Some(test_blueprint("Vincenzo", 1.0)),
                Some(test_blueprint("Antonio", 1.08)),
                None,
                None,
            ],
            completed_chapters: vec![1],
            perk_ranks: vec![("heart_vitality".to_string(), 2)],
            robot_pets,
            tech_upgrades,
            ..SaveData::default()
        };

        let json = serde_json::to_string(&data).expect("save data should serialize");
        let loaded: SaveData = serde_json::from_str(&json).expect("save data should deserialize");

        assert_eq!(loaded.players.len(), 2);
        assert_eq!(loaded.players[0].player_index, 0);
        assert_eq!(loaded.players[1].player_index, 1);
        assert_eq!(loaded.players[1].level, 5);
        assert_eq!(loaded.players[1].health_current, 45.0);
        assert_eq!(
            loaded.character_blueprints[0].as_ref().unwrap().name,
            "Vincenzo"
        );
        assert_eq!(loaded.completed_chapters, vec![1]);
        assert_eq!(loaded.perk_ranks, vec![("heart_vitality".to_string(), 2)]);
        assert_eq!(loaded.robot_pets.part_count(RobotPartKind::StarDrive), 2);
        assert_eq!(loaded.robot_pets.pets[0].name, "Nova Kit");
        assert_eq!(
            loaded.tech_upgrades.rank(TechUpgradeId::RejuvenationMatrix),
            1
        );
        assert_eq!(loaded.tech_upgrades.rejuvenation_charge, 120.0);
    }

    #[test]
    fn current_saves_select_matching_player_index_not_record_order() {
        let data = SaveData {
            players: vec![
                player_save(3, 9, 900, 99, 30.0, 160.0),
                player_save(1, 4, 250, 44, 70.0, 120.0),
            ],
            ..SaveData::default()
        };

        let p1 = player_save_for(&data, 1).expect("P2 record should exist");
        let p3 = player_save_for(&data, 3).expect("P4 record should exist");

        assert_eq!(p1.level, 4);
        assert_eq!(p1.credits, 44);
        assert_eq!(p3.level, 9);
        assert_eq!(p3.credits, 99);
        assert!(player_save_for(&data, 0).is_none());
    }

    #[test]
    fn legacy_saves_hydrate_any_active_player_slot() {
        let data = SaveData {
            level: 6,
            experience: 555,
            credits: 42,
            max_health: 140.0,
            max_stamina: 115.0,
            max_armor: 75.0,
            players: Vec::new(),
            ..SaveData::default()
        };

        let p0 = player_save_for(&data, 0).expect("legacy P1 should hydrate");
        let p2 = player_save_for(&data, 2).expect("legacy P3 should hydrate");

        assert_eq!(p0.player_index, 0);
        assert_eq!(p2.player_index, 2);
        assert_eq!(p2.level, 6);
        assert_eq!(p2.experience, 555);
        assert_eq!(p2.health_current, 140.0);
        assert_eq!(p2.max_stamina, 115.0);
        assert_eq!(p2.max_armor, 75.0);
    }

    #[test]
    fn applying_player_save_clamps_loaded_runtime_values() {
        let saved = PlayerSaveData {
            player_index: 0,
            level: 0,
            experience: 50,
            credits: 25,
            health_current: 500.0,
            health_max: 125.0,
            stamina: 999.0,
            max_stamina: 80.0,
            armor: -10.0,
            max_armor: 60.0,
        };
        let mut stats = PlayerStats::default();
        let mut health = Health::new(100.0);

        saved.apply_to(&mut stats, &mut health);

        assert_eq!(stats.level, 1);
        assert_eq!(stats.experience, 50);
        assert_eq!(stats.credits, 25);
        assert_eq!(stats.max_health, 125.0);
        assert_eq!(health.max, 125.0);
        assert_eq!(health.current, 125.0);
        assert_eq!(stats.max_stamina, 80.0);
        assert_eq!(stats.stamina, 80.0);
        assert_eq!(stats.max_armor, 60.0);
        assert_eq!(stats.armor, 0.0);
    }
}
