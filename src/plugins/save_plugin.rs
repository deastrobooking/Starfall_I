use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::character_blueprint::CharacterBlueprint;
use crate::character_parts::{ArmPreset, BodyPreset, HeadPreset, LegPreset, ShoulderPreset};
use crate::character_studio::spec::CharacterSpec;
use crate::commands::{initial_command_assets, CommandAssetSaveRecord, CommandRegistry};
use crate::components::armor::{ArmorSet, ElementType};
use crate::components::inventory::{Inventory, QuickItemSlot};
use crate::components::player::{
    Player, PlayerIndex, PlayerProgression, PlayerStats, TraversalMode, TraversalModeState,
};
use crate::components::weapon::{SpecialWeaponInventory, WeaponInventory, WeaponRanks};
use crate::damage::Health;
use crate::events::UiMessageEvent;
use crate::final_war::{FinalWarRegistry, FinalWarSaveRecord};
use crate::hacking::HackingRegistry;
use crate::perks::PerkTree;
use crate::raids::{RaidRecord, RaidRegistry};
use crate::resources::{
    initial_world_routes, initial_world_sites, is_stale_reference_blueprint, ChapterProgress,
    GameSettings, PlaySessionTransition, PlayerPartLoadout, PlayerSelectState, WaveInfo,
    WorldRouteRegistry, WorldRouteSaveRecord, WorldSiteRegistry, WorldSiteSaveRecord,
};
use crate::robot_pets::RobotPetCollection;
use crate::settlement_economy::SettlementEconomy;
use crate::shop_transactions::ShopOwnership;
use crate::state::AppState;
use crate::upgrades::UpgradeLedger;

const SAVE_FILE_LEGACY: &str = "starfall_i_save.json";
const SAVE_FILE_A: &str = "starfall_i_save_a.json";
const SAVE_FILE_B: &str = "starfall_i_save_b.json";
const SAVE_FILE_C: &str = "starfall_i_save_c.json";
const SETTINGS_FILE: &str = "starfall_i_settings.json";
/// Current on-disk schema version. v4 introduced `save_generation`, the
/// monotonic counter that decides which rotating slot is newest on load.
const SAVE_VERSION: u32 = 4;

// ── Save Rotation State ───────────────────────────────────────────────────────
/// Tracks which save slot (0=A, 1=B, 2=C) to write to next.
#[derive(Resource, Debug, Clone, Default)]
pub struct SaveRotationState {
    pub current_slot: u8,
}

fn slot_file_name(slot: u8) -> &'static str {
    match slot % 3 {
        0 => SAVE_FILE_A,
        1 => SAVE_FILE_B,
        _ => SAVE_FILE_C,
    }
}

fn save_slot_path_in(root: &Path, slot: u8) -> PathBuf {
    root.join(slot_file_name(slot))
}

fn next_save_slot(current: u8) -> u8 {
    (current + 1) % 3
}

// ── Save Directory ────────────────────────────────────────────────────────────
/// Platform-appropriate save directory (e.g. `~/Library/Application Support/
/// starfall_i` on macOS, `~/.local/share/starfall_i` on Linux). Falls back to
/// the current working directory when no platform data dir is available.
/// Resolved once per process; the first resolution migrates any legacy save
/// files left in the working directory by older builds so no progress is lost.
fn save_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = crate::platform_paths::data_root();
        migrate_legacy_files(&root);
        root
    })
}

/// Copies legacy working-directory save/settings files into `root` on first
/// run with the platform location. Non-destructive: the originals stay behind
/// as a backup, and existing files in `root` are never overwritten.
fn migrate_legacy_files(root: &Path) {
    let copy_legacy = |from: &Path, to: &Path| {
        if let Some(parent) = to.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                warn!(
                    "Failed to create save directory {}: {}",
                    parent.display(),
                    e
                );
                return;
            }
        }
        match fs::copy(from, to) {
            Ok(_) => info!(
                "Migrated legacy save file {} to {}",
                from.display(),
                to.display()
            ),
            Err(e) => warn!(
                "Failed to migrate legacy save file {}: {}",
                from.display(),
                e
            ),
        }
    };

    let has_new_slots = (0u8..3).any(|slot| save_slot_path_in(root, slot).exists());
    if !has_new_slots {
        for slot in 0u8..3 {
            let legacy = PathBuf::from(slot_file_name(slot));
            if legacy.exists() {
                copy_legacy(&legacy, &save_slot_path_in(root, slot));
            }
        }
        // Pre-rotation builds wrote a single save file; adopt it as slot A if
        // no rotating slot claimed that name.
        let legacy_single = PathBuf::from(SAVE_FILE_LEGACY);
        let slot_a = save_slot_path_in(root, 0);
        if legacy_single.exists() && !slot_a.exists() {
            copy_legacy(&legacy_single, &slot_a);
        }
    }

    let legacy_settings = PathBuf::from(SETTINGS_FILE);
    let new_settings = root.join(SETTINGS_FILE);
    if legacy_settings.exists() && !new_settings.exists() {
        copy_legacy(&legacy_settings, &new_settings);
    }
}

// ── Atomic Write ──────────────────────────────────────────────────────────────
/// Writes `contents` to `path` via a same-directory temporary file that is
/// flushed, synced, and then atomically renamed over the destination, so an
/// interrupted write can never corrupt an existing save.
fn write_atomic(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let mut file = File::create(&temp).map_err(|e| e.to_string())?;
    file.write_all(contents.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())?;
    // Best-effort directory sync so the rename itself is durable.
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = File::open(parent).and_then(|directory| directory.sync_all());
        }
    }
    Ok(())
}

// ── Save SystemParam Bundle ───────────────────────────────────────────────────
/// Groups all save-relevant read params into one SystemParam so callers like
/// `pause_menu_action_system` stay within Bevy's 16-param system limit.
#[derive(SystemParam)]
pub struct SaveParams<'w, 's> {
    pub player_q: Query<
        'w,
        's,
        (
            &'static PlayerIndex,
            &'static PlayerStats,
            &'static Health,
            &'static WeaponInventory,
            &'static SpecialWeaponInventory,
            &'static ArmorSet,
            &'static Inventory,
            &'static QuickItemSlot,
            &'static TraversalModeState,
            &'static PlayerProgression,
        ),
        With<Player>,
    >,
    pub wave: Res<'w, WaveInfo>,
    pub progress: Res<'w, ChapterProgress>,
    pub perks: Res<'w, PerkTree>,
    pub select: Res<'w, PlayerSelectState>,
    pub robot_pets: Res<'w, RobotPetCollection>,
    pub settlement_economy: Res<'w, SettlementEconomy>,
    pub upgrades: Res<'w, UpgradeLedger>,
    pub part_loadout: Res<'w, PlayerPartLoadout>,
    pub weapon_ranks: Res<'w, WeaponRanks>,
    pub world_site_registry: Res<'w, WorldSiteRegistry>,
    pub world_route_registry: Res<'w, WorldRouteRegistry>,
    pub raid_registry: Res<'w, RaidRegistry>,
    pub command_registry: Res<'w, CommandRegistry>,
    pub hacking_registry: Res<'w, HackingRegistry>,
    pub final_war_registry: Res<'w, FinalWarRegistry>,
}

/// Bundles the mutable registry resources that were added in later milestones so
/// that `load_save_on_enter` stays within Bevy's 16-parameter system limit.
#[derive(SystemParam)]
struct LoadRegistriesParam<'w> {
    raid_registry: ResMut<'w, RaidRegistry>,
    command_registry: ResMut<'w, CommandRegistry>,
    hacking_registry: ResMut<'w, HackingRegistry>,
    final_war_registry: ResMut<'w, FinalWarRegistry>,
}

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
    pub studio_character_specs: Vec<Option<CharacterSpec>>,
    #[serde(default)]
    pub part_loadouts: Vec<Option<PlayerPartLoadout>>,
    #[serde(default)]
    pub players: Vec<PlayerSaveData>,
    #[serde(default)]
    pub robot_pets: RobotPetCollection,
    #[serde(default)]
    pub settlement_economy: SettlementEconomy,
    #[serde(default)]
    pub tech_upgrades: UpgradeLedger,
    #[serde(default)]
    pub part_loadout_body: BodyPreset,
    #[serde(default)]
    pub part_loadout_arms: ArmPreset,
    #[serde(default)]
    pub part_loadout_legs: LegPreset,
    #[serde(default)]
    pub part_loadout_shoulders: ShoulderPreset,
    #[serde(default)]
    pub part_loadout_head: HeadPreset,
    #[serde(default)]
    pub weapon_ranks: [u32; 6],
    #[serde(default)]
    pub world_sites: Vec<WorldSiteSaveRecord>,
    #[serde(default)]
    pub world_routes: Vec<WorldRouteSaveRecord>,
    #[serde(default)]
    pub raids: Vec<RaidRecord>,
    #[serde(default)]
    pub command_assets: Vec<CommandAssetSaveRecord>,
    #[serde(default)]
    pub hacking: HackingRegistry,
    #[serde(default)]
    pub final_war: FinalWarSaveRecord,
    #[serde(default)]
    pub save_version: u32,
    /// Monotonic counter bumped on every write; the loader picks the valid
    /// slot with the highest generation. v3 and earlier saves default to 0.
    #[serde(default)]
    pub save_generation: u64,
    #[serde(default)]
    pub campaign_complete: bool,
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
    #[serde(default)]
    pub primary_weapon_slot: usize,
    #[serde(default)]
    pub special_weapon_slot: Option<u8>,
    #[serde(default)]
    pub armor_element: ElementType,
    /// `None` identifies saves written before inventory persistence existed, so
    /// their freshly spawned starter items are not accidentally erased.
    #[serde(default)]
    pub inventory: Option<Inventory>,
    #[serde(default)]
    pub quick_item_id: Option<String>,
    #[serde(default)]
    pub traversal_mode: u8,
    #[serde(default)]
    pub perk_tree: Option<PerkTree>,
    #[serde(default)]
    pub tech_upgrades: Option<UpgradeLedger>,
    #[serde(default)]
    pub weapon_ranks: Option<[u32; 6]>,
    /// `None` identifies saves written before the PX3 shop existed.
    #[serde(default)]
    pub shop: Option<ShopOwnership>,
}

impl PlayerSaveData {
    #[allow(clippy::too_many_arguments)]
    fn from_runtime(
        player_index: u8,
        stats: &PlayerStats,
        health: &Health,
        weapons: &WeaponInventory,
        specials: &SpecialWeaponInventory,
        armor: &ArmorSet,
        inventory: &Inventory,
        quick: &QuickItemSlot,
        traversal: &TraversalModeState,
        progression: &PlayerProgression,
    ) -> Self {
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
            primary_weapon_slot: weapons.active_slot,
            special_weapon_slot: specials.active_slot,
            armor_element: armor.active_element,
            inventory: Some(inventory.clone()),
            quick_item_id: quick.item_id.clone(),
            traversal_mode: traversal_mode_index(traversal.active),
            perk_tree: Some(progression.perks.clone()),
            tech_upgrades: Some(progression.upgrades.clone()),
            weapon_ranks: Some(progression.weapon_ranks.ranks),
            shop: Some(progression.shop.clone()),
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
            primary_weapon_slot: 0,
            special_weapon_slot: None,
            armor_element: ElementType::None,
            inventory: None,
            quick_item_id: None,
            traversal_mode: 0,
            perk_tree: None,
            tech_upgrades: None,
            weapon_ranks: None,
            shop: None,
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

    fn apply_loadout(
        &self,
        weapons: &mut WeaponInventory,
        specials: &mut SpecialWeaponInventory,
        armor: &mut ArmorSet,
        inventory: &mut Inventory,
        quick: &mut QuickItemSlot,
        traversal: &mut TraversalModeState,
    ) {
        weapons.active_slot = self.primary_weapon_slot.min(weapons.slots.len() - 1);
        specials.active_slot = self.special_weapon_slot.filter(|slot| *slot <= 3);
        armor.active_element = self.armor_element;
        if let Some(saved_inventory) = &self.inventory {
            inventory.clone_from(saved_inventory);
        }
        quick.item_id.clone_from(&self.quick_item_id);
        if quick
            .item_id
            .as_deref()
            .is_some_and(|item_id| !inventory.has(item_id, 1))
        {
            quick.item_id = None;
        }
        traversal.active = traversal_mode_from_index(self.traversal_mode);
    }

    fn apply_progression(&self, progression: &mut PlayerProgression) {
        if let Some(perks) = &self.perk_tree {
            progression.perks.clone_from(perks);
        }
        if let Some(upgrades) = &self.tech_upgrades {
            progression.upgrades.clone_from(upgrades);
        }
        if let Some(ranks) = self.weapon_ranks {
            progression.weapon_ranks.ranks = ranks;
        }
        if let Some(shop) = &self.shop {
            progression.shop.clone_from(shop);
        }
    }
}

fn traversal_mode_index(mode: TraversalMode) -> u8 {
    match mode {
        TraversalMode::Grapple => 0,
        TraversalMode::HoverJet => 1,
        TraversalMode::Flight => 2,
        TraversalMode::Hoverboard => 3,
    }
}

fn traversal_mode_from_index(index: u8) -> TraversalMode {
    match index {
        1 => TraversalMode::HoverJet,
        2 => TraversalMode::Flight,
        3 => TraversalMode::Hoverboard,
        _ => TraversalMode::Grapple,
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
            studio_character_specs: vec![None, None, None, None],
            part_loadouts: vec![None, None, None, None],
            players: Vec::new(),
            robot_pets: RobotPetCollection::default(),
            settlement_economy: SettlementEconomy::default(),
            tech_upgrades: UpgradeLedger::default(),
            part_loadout_body: BodyPreset::default(),
            part_loadout_arms: ArmPreset::default(),
            part_loadout_legs: LegPreset::default(),
            part_loadout_shoulders: ShoulderPreset::default(),
            part_loadout_head: HeadPreset::default(),
            weapon_ranks: [0u32; 6],
            world_sites: Vec::new(),
            world_routes: Vec::new(),
            raids: Vec::new(),
            command_assets: Vec::new(),
            hacking: HackingRegistry::default(),
            final_war: FinalWarSaveRecord::default(),
            save_version: SAVE_VERSION,
            save_generation: 0,
            campaign_complete: false,
        }
    }
}

fn default_max_stat() -> f32 {
    100.0
}

// ── Settings Persistence ──────────────────────────────────────────────────────
fn settings_path() -> PathBuf {
    save_root().join(SETTINGS_FILE)
}

pub fn save_settings(settings: &GameSettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    write_atomic(&settings_path(), &json)
}

pub fn load_settings() -> Option<GameSettings> {
    let path = settings_path();
    if !path.exists() {
        return None;
    }
    let json = match fs::read_to_string(&path) {
        Ok(json) => json,
        Err(e) => {
            warn!("Failed to read settings file {}: {}", path.display(), e);
            return None;
        }
    };
    match serde_json::from_str(&json) {
        Ok(settings) => Some(settings),
        Err(e) => {
            warn!("Ignoring corrupt settings file {}: {}", path.display(), e);
            None
        }
    }
}

fn load_settings_on_startup(mut settings: ResMut<GameSettings>) {
    if let Some(loaded) = load_settings() {
        *settings = loaded;
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
            .init_resource::<SaveRotationState>()
            .add_systems(
                Startup,
                (hydrate_progress_from_disk, load_settings_on_startup),
            )
            .add_systems(OnEnter(AppState::Playing), load_save_on_enter)
            .add_systems(
                Update,
                (autosave_system, manual_save_system).run_if(in_state(AppState::Playing)),
            );
    }
}

// ── Save ──────────────────────────────────────────────────────────────────────
/// Plain snapshot of everything a save write reads, replacing the former
/// sixteen-parameter save function signatures.
pub struct SaveSnapshot<'a> {
    pub players: Vec<PlayerSaveData>,
    pub wave: &'a WaveInfo,
    pub progress: &'a ChapterProgress,
    pub perks: &'a PerkTree,
    pub select: &'a PlayerSelectState,
    pub robot_pets: &'a RobotPetCollection,
    pub settlement_economy: &'a SettlementEconomy,
    pub upgrades: &'a UpgradeLedger,
    pub part_loadout: &'a PlayerPartLoadout,
    pub weapon_ranks: &'a WeaponRanks,
    pub world_site_registry: &'a WorldSiteRegistry,
    pub world_route_registry: &'a WorldRouteRegistry,
    pub raid_registry: &'a RaidRegistry,
    pub command_registry: &'a CommandRegistry,
    pub hacking_registry: &'a HackingRegistry,
    pub final_war_registry: &'a FinalWarRegistry,
}

impl<'a> SaveSnapshot<'a> {
    pub fn from_params(sp: &'a SaveParams, players: Vec<PlayerSaveData>) -> Self {
        Self {
            players,
            wave: &sp.wave,
            progress: &sp.progress,
            perks: &sp.perks,
            select: &sp.select,
            robot_pets: &sp.robot_pets,
            settlement_economy: &sp.settlement_economy,
            upgrades: &sp.upgrades,
            part_loadout: &sp.part_loadout,
            weapon_ranks: &sp.weapon_ranks,
            world_site_registry: &sp.world_site_registry,
            world_route_registry: &sp.world_route_registry,
            raid_registry: &sp.raid_registry,
            command_registry: &sp.command_registry,
            hacking_registry: &sp.hacking_registry,
            final_war_registry: &sp.final_war_registry,
        }
    }
}

pub fn save_game(snapshot: SaveSnapshot) -> Result<(), String> {
    save_game_to_slot(0, snapshot)
}

pub fn save_game_to_slot(slot: u8, snapshot: SaveSnapshot) -> Result<(), String> {
    write_save_to_slot(save_root(), slot, snapshot)
}

fn write_save_to_slot(root: &Path, slot: u8, snapshot: SaveSnapshot) -> Result<(), String> {
    let mut data = build_save_data(snapshot);
    data.save_generation = next_generation_in(root);
    write_save_data(root, slot, &data)
}

fn write_save_data(root: &Path, slot: u8, data: &SaveData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    write_atomic(&save_slot_path_in(root, slot), &json)
}

/// Minimal probe used to read a slot's generation without requiring the rest
/// of the record to deserialize.
#[derive(Deserialize)]
struct GenerationProbe {
    #[serde(default)]
    save_generation: u64,
}

/// Generation for the next write: one past the highest generation currently on
/// disk across all rotating slots (corrupt/missing slots are simply skipped).
fn next_generation_in(root: &Path) -> u64 {
    (0u8..3)
        .filter_map(|slot| {
            fs::read_to_string(save_slot_path_in(root, slot))
                .ok()
                .and_then(|json| serde_json::from_str::<GenerationProbe>(&json).ok())
                .map(|probe| probe.save_generation)
        })
        .max()
        .map_or(1, |generation| generation.saturating_add(1))
}

fn build_save_data(snapshot: SaveSnapshot) -> SaveData {
    let SaveSnapshot {
        mut players,
        wave,
        progress,
        perks,
        select,
        robot_pets,
        settlement_economy,
        upgrades,
        part_loadout,
        weapon_ranks,
        world_site_registry,
        world_route_registry,
        raid_registry,
        command_registry,
        hacking_registry,
        final_war_registry,
    } = snapshot;
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
        studio_character_specs: select.slots.iter().map(|slot| slot.studio_spec).collect(),
        part_loadouts: select.slots.iter().map(|slot| slot.part_loadout).collect(),
        players,
        robot_pets: robot_pets.clone(),
        settlement_economy: settlement_economy.clone(),
        tech_upgrades: upgrades.clone(),
        part_loadout_body: part_loadout.body,
        part_loadout_arms: part_loadout.arms,
        part_loadout_legs: part_loadout.legs,
        part_loadout_shoulders: part_loadout.shoulders,
        part_loadout_head: part_loadout.head,
        weapon_ranks: weapon_ranks.ranks,
        world_sites: world_site_registry.to_save_records(),
        world_routes: world_route_registry.to_save_records(),
        raids: raid_registry.to_save_records(),
        command_assets: command_registry.to_save_records(),
        hacking: hacking_registry.clone(),
        final_war: final_war_registry.to_save_record(),
        campaign_complete: progress.campaign_complete,
        ..SaveData::default()
    }
}

pub fn load_save() -> Option<SaveData> {
    load_newest_save_in(save_root()).map(|(_, data)| data)
}

/// Inspects all rotating slots and returns the valid record with the highest
/// save generation, along with the slot it was read from. Corrupt, unreadable,
/// or unsupported slots are logged and skipped so any surviving slot recovers.
fn load_newest_save_in(root: &Path) -> Option<(u8, SaveData)> {
    let mut newest: Option<(u8, SaveData)> = None;
    for slot in 0u8..3 {
        let Some(data) = read_save_slot(root, slot) else {
            continue;
        };
        let is_newer = newest
            .as_ref()
            .is_none_or(|(_, best)| data.save_generation > best.save_generation);
        if is_newer {
            newest = Some((slot, data));
        }
    }
    newest
}

fn read_save_slot(root: &Path, slot: u8) -> Option<SaveData> {
    let path = save_slot_path_in(root, slot);
    if !path.exists() {
        return None;
    }
    let json = match fs::read_to_string(&path) {
        Ok(json) => json,
        Err(e) => {
            warn!("Failed to read save slot {}: {}", path.display(), e);
            return None;
        }
    };
    let data = match serde_json::from_str::<SaveData>(&json) {
        Ok(data) => data,
        Err(e) => {
            warn!("Ignoring corrupt save slot {}: {}", path.display(), e);
            return None;
        }
    };
    migrate_save_data(data, &path)
}

/// Validates `save_version`: older supported schemas are migrated in place,
/// while records from a newer build are rejected with a warning instead of
/// being misread or silently skipped.
fn migrate_save_data(mut data: SaveData, path: &Path) -> Option<SaveData> {
    if data.save_version > SAVE_VERSION {
        warn!(
            "Ignoring save slot {} with unsupported save_version {} (this build supports up to {})",
            path.display(),
            data.save_version,
            SAVE_VERSION
        );
        return None;
    }
    if data.save_version < SAVE_VERSION {
        // v3 and earlier predate the generation counter; serde's default has
        // already assigned generation 0, so they sort as the oldest records.
        data.save_version = SAVE_VERSION;
    }
    Some(data)
}

fn hydrate_character_blueprints(
    select: &mut PlayerSelectState,
    blueprints: Vec<Option<CharacterBlueprint>>,
) {
    for index in 0..select.slots.len() {
        let name = select.character_name(index);
        let blueprint = blueprints.get(index).cloned().flatten();
        select.slots[index].blueprint =
            blueprint.filter(|blueprint| !is_stale_reference_blueprint(name, blueprint));
    }
}

fn hydrate_studio_character_specs(
    select: &mut PlayerSelectState,
    specs: Vec<Option<CharacterSpec>>,
) {
    for (index, slot) in select.slots.iter_mut().enumerate() {
        slot.studio_spec = specs.get(index).copied().flatten();
    }
}

fn hydrate_part_loadouts(
    select: &mut PlayerSelectState,
    loadouts: Vec<Option<PlayerPartLoadout>>,
    legacy_loadout: PlayerPartLoadout,
) {
    if loadouts.is_empty() {
        let fallback = if legacy_loadout.is_stale_native_default() {
            None
        } else {
            Some(legacy_loadout)
        };
        for slot in &mut select.slots {
            slot.part_loadout = fallback;
        }
        return;
    }

    for (slot, loadout) in select.slots.iter_mut().zip(loadouts) {
        slot.part_loadout = loadout.filter(|loadout| !loadout.is_stale_native_default());
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
    player_q: &Query<
        (
            &PlayerIndex,
            &PlayerStats,
            &Health,
            &WeaponInventory,
            &SpecialWeaponInventory,
            &ArmorSet,
            &Inventory,
            &QuickItemSlot,
            &TraversalModeState,
            &PlayerProgression,
        ),
        With<Player>,
    >,
) -> Vec<PlayerSaveData> {
    let mut players: Vec<_> = player_q
        .iter()
        .map(
            |(
                index,
                stats,
                health,
                weapons,
                specials,
                armor,
                inventory,
                quick,
                traversal,
                progression,
            )| {
                PlayerSaveData::from_runtime(
                    index.0,
                    stats,
                    health,
                    weapons,
                    specials,
                    armor,
                    inventory,
                    quick,
                    traversal,
                    progression,
                )
            },
        )
        .collect();
    players.sort_by_key(|player| player.player_index);
    players
}

pub fn save_current_session(sp: &SaveParams) -> Result<(), String> {
    let players = collect_player_saves(&sp.player_q);
    if players.is_empty() {
        return Err("No active players to save".to_string());
    }
    save_game(SaveSnapshot::from_params(sp, players))
}

// ── Systems ───────────────────────────────────────────────────────────────────
fn hydrate_progress_from_disk(
    mut progress: ResMut<ChapterProgress>,
    mut perks: ResMut<PerkTree>,
    mut select: ResMut<PlayerSelectState>,
    mut robot_pets: ResMut<RobotPetCollection>,
    mut settlement_economy: ResMut<SettlementEconomy>,
    mut upgrades: ResMut<UpgradeLedger>,
    mut part_loadout: ResMut<PlayerPartLoadout>,
    mut weapon_ranks: ResMut<WeaponRanks>,
    mut world_site_registry: ResMut<WorldSiteRegistry>,
    mut world_route_registry: ResMut<WorldRouteRegistry>,
    mut regs: LoadRegistriesParam,
    mut rotation: ResMut<SaveRotationState>,
) {
    if let Some((loaded_slot, data)) = load_newest_save_in(save_root()) {
        // Resume rotation after the newest record so the next autosave
        // overwrites the oldest slot instead of the freshest one.
        rotation.current_slot = next_save_slot(loaded_slot);
        for (index, slot) in select.slots.iter_mut().enumerate() {
            let mut progression = PlayerProgression {
                perks: PerkTree {
                    points_unspent: data.perk_points_unspent,
                    ranks: data.perk_ranks.clone(),
                },
                upgrades: data.tech_upgrades.clone(),
                weapon_ranks: WeaponRanks {
                    ranks: data.weapon_ranks,
                },
                shop: ShopOwnership::default(),
            };
            if let Some(saved) = player_save_for(&data, index as u8) {
                saved.apply_progression(&mut progression);
            }
            slot.progression = progression;
        }
        *robot_pets = data.robot_pets.clone();
        *settlement_economy = data.settlement_economy.clone();
        *upgrades = data.tech_upgrades.clone();
        progress.completed = data.completed_chapters;
        progress.discoverables = data.discoverables;
        progress.companions_recruited = data.companions_recruited;
        progress.scientist_relics = data.scientist_relics;
        progress.relic_fragments = data.relic_fragments;
        progress.campaign_complete = data.campaign_complete;
        perks.points_unspent = data.perk_points_unspent;
        perks.ranks = data.perk_ranks;
        hydrate_character_blueprints(&mut select, data.character_blueprints);
        hydrate_studio_character_specs(&mut select, data.studio_character_specs);
        part_loadout.body = data.part_loadout_body;
        part_loadout.arms = data.part_loadout_arms;
        part_loadout.legs = data.part_loadout_legs;
        part_loadout.shoulders = data.part_loadout_shoulders;
        part_loadout.head = data.part_loadout_head;
        hydrate_part_loadouts(&mut select, data.part_loadouts, *part_loadout);
        weapon_ranks.ranks = data.weapon_ranks;
        if world_site_registry.sites.is_empty() {
            world_site_registry.sites = initial_world_sites();
        }
        world_site_registry.apply_save_records(&data.world_sites);
        if world_route_registry.routes.is_empty() {
            world_route_registry.routes = initial_world_routes();
        }
        world_route_registry.apply_save_records(&data.world_routes);
        regs.raid_registry.apply_save_records(data.raids);
        if regs.command_registry.assets.is_empty() {
            regs.command_registry.assets = initial_command_assets();
        }
        regs.command_registry
            .apply_save_records(&data.command_assets);
        *regs.hacking_registry = data.hacking.clone();
        regs.final_war_registry
            .apply_save_record(data.final_war.clone());
    }
}

fn load_save_on_enter(
    mut player_q: Query<
        (
            &PlayerIndex,
            &mut PlayerStats,
            &mut Health,
            &mut WeaponInventory,
            &mut SpecialWeaponInventory,
            &mut ArmorSet,
            &mut Inventory,
            &mut QuickItemSlot,
            &mut TraversalModeState,
        ),
        With<Player>,
    >,
    mut wave: ResMut<WaveInfo>,
    mut progress: ResMut<ChapterProgress>,
    mut perks: ResMut<PerkTree>,
    mut robot_pets: ResMut<RobotPetCollection>,
    mut settlement_economy: ResMut<SettlementEconomy>,
    mut upgrades: ResMut<UpgradeLedger>,
    mut weapon_ranks: ResMut<WeaponRanks>,
    mut world_site_registry: ResMut<WorldSiteRegistry>,
    mut world_route_registry: ResMut<WorldRouteRegistry>,
    mut regs: LoadRegistriesParam,
    transition: Res<PlaySessionTransition>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if transition.resuming_from_pause {
        return;
    }

    if let Some(data) = load_save() {
        *robot_pets = data.robot_pets.clone();
        *settlement_economy = data.settlement_economy.clone();
        *upgrades = data.tech_upgrades.clone();
        let mut active_players = 0usize;
        for (
            index,
            mut stats,
            mut health,
            mut weapons,
            mut specials,
            mut armor,
            mut inventory,
            mut quick,
            mut traversal,
        ) in player_q.iter_mut()
        {
            active_players += 1;
            if let Some(saved) = player_save_for(&data, index.0) {
                saved.apply_to(&mut stats, &mut health);
                saved.apply_loadout(
                    &mut weapons,
                    &mut specials,
                    &mut armor,
                    &mut inventory,
                    &mut quick,
                    &mut traversal,
                );
            }
        }
        wave.wave_number = data.wave_number;
        progress.completed = data.completed_chapters;
        progress.discoverables = data.discoverables;
        progress.companions_recruited = data.companions_recruited;
        progress.scientist_relics = data.scientist_relics;
        progress.relic_fragments = data.relic_fragments;
        progress.campaign_complete = data.campaign_complete;
        perks.points_unspent = data.perk_points_unspent;
        perks.ranks = data.perk_ranks;
        // Character customization is hydrated at startup and may have been edited
        // in the lobby immediately before entering gameplay. Do not overwrite it
        // here with the previous disk save, or fresh editor changes will be lost
        // before the next autosave/manual save.
        weapon_ranks.ranks = data.weapon_ranks;
        if world_site_registry.sites.is_empty() {
            world_site_registry.sites = initial_world_sites();
        }
        world_site_registry.apply_save_records(&data.world_sites);
        if world_route_registry.routes.is_empty() {
            world_route_registry.routes = initial_world_routes();
        }
        world_route_registry.apply_save_records(&data.world_routes);
        regs.raid_registry.apply_save_records(data.raids);
        if regs.command_registry.assets.is_empty() {
            regs.command_registry.assets = initial_command_assets();
        }
        regs.command_registry
            .apply_save_records(&data.command_assets);
        *regs.hacking_registry = data.hacking.clone();
        regs.final_war_registry
            .apply_save_record(data.final_war.clone());
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
    mut rotation: ResMut<SaveRotationState>,
    sp: SaveParams,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    save_state.last_save_timer += time.delta_secs();
    if save_state.last_save_timer < save_state.autosave_interval {
        return;
    }
    save_state.last_save_timer = 0.0;

    let slot = rotation.current_slot;

    let players = collect_player_saves(&sp.player_q);
    if players.is_empty() {
        return;
    }
    match save_game_to_slot(slot, SaveSnapshot::from_params(&sp, players)) {
        Ok(()) => {
            // Advance only after the slot is durably replaced. A failed write
            // retries the same recovery slot instead of silently skipping it.
            rotation.current_slot = next_save_slot(slot);
            msg_ev.write(UiMessageEvent {
                text: format!("Game autosaved (slot {}).", (b'A' + slot) as char),
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
    sp: SaveParams,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }
    match save_current_session(&sp) {
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
    use crate::raids::{RaidId, RaidPhase};
    use crate::robot_pets::{RobotPartKind, RobotPetBlueprint, RobotPetRole};
    use crate::settlement_economy::{SettlementBuildKind, SettlementEconomy, SettlementResources};
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
            primary_weapon_slot: 0,
            special_weapon_slot: None,
            armor_element: ElementType::None,
            inventory: Some(Inventory::default()),
            quick_item_id: None,
            traversal_mode: 0,
            perk_tree: None,
            tech_upgrades: None,
            weapon_ranks: None,
            shop: None,
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
            campaign_complete: false,
        };
        let perks = PerkTree {
            points_unspent: 2,
            ranks: vec![("star_focus".to_string(), 3)],
        };
        let mut select = PlayerSelectState::default();
        select.slots[0].blueprint = Some(test_blueprint("P1", 1.0));
        select.slots[0].part_loadout = Some(PlayerPartLoadout {
            body: BodyPreset::DariaCore,
            arms: ArmPreset::DariaCannon,
            legs: LegPreset::DariaGreaves,
            shoulders: ShoulderPreset::DariaFlares,
            head: HeadPreset::DariaHelm,
        });
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
        let mut settlement_economy = SettlementEconomy {
            stockpile: SettlementResources::new(900, 70, 40, 22, 18, 4, 6, 8, 2),
            ..SettlementEconomy::default()
        };
        settlement_economy
            .try_build(
                "settlement_riftglass_village",
                SettlementBuildKind::Farm,
                &mut robot_pets,
            )
            .expect("test stockpile should build a farm");
        let part_loadout = PlayerPartLoadout {
            body: BodyPreset::HeavyPlate,
            arms: ArmPreset::ScoutArms,
            legs: LegPreset::JetLegs,
            shoulders: ShoulderPreset::SpikedPauldrons,
            head: HeadPreset::CombatHelmet,
        };

        let data = build_save_data(SaveSnapshot {
            players: vec![
                player_save(2, 8, 700, 90, 44.0, 150.0),
                player_save(0, 4, 300, 20, 88.0, 110.0),
            ],
            wave: &wave,
            progress: &progress,
            perks: &perks,
            select: &select,
            robot_pets: &robot_pets,
            settlement_economy: &settlement_economy,
            upgrades: &upgrades,
            part_loadout: &part_loadout,
            weapon_ranks: &WeaponRanks::default(),
            world_site_registry: &WorldSiteRegistry::default(),
            world_route_registry: &WorldRouteRegistry::default(),
            raid_registry: &RaidRegistry::default(),
            command_registry: &CommandRegistry::default(),
            hacking_registry: &HackingRegistry::default(),
            final_war_registry: &FinalWarRegistry::default(),
        });

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
        assert_eq!(
            data.settlement_economy
                .build_tier("settlement_riftglass_village", SettlementBuildKind::Farm),
            1
        );
        assert_eq!(data.tech_upgrades.rank(TechUpgradeId::BeamCapacitors), 2);
        assert_eq!(data.tech_upgrades.rejuvenation_charge, 75.0);
        assert!(data.raids.is_empty());
        assert_eq!(data.part_loadout_body, BodyPreset::HeavyPlate);
        assert_eq!(data.part_loadout_arms, ArmPreset::ScoutArms);
        assert_eq!(data.part_loadout_legs, LegPreset::JetLegs);
        assert_eq!(data.part_loadout_shoulders, ShoulderPreset::SpikedPauldrons);
        assert_eq!(data.part_loadout_head, HeadPreset::CombatHelmet);
        assert_eq!(
            data.part_loadouts[0],
            Some(PlayerPartLoadout {
                body: BodyPreset::DariaCore,
                arms: ArmPreset::DariaCannon,
                legs: LegPreset::DariaGreaves,
                shoulders: ShoulderPreset::DariaFlares,
                head: HeadPreset::DariaHelm,
            })
        );
    }

    #[test]
    fn save_data_round_trip_preserves_per_player_records_and_part_loadout() {
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
            studio_character_specs: vec![Some(CharacterSpec::default()), None, None, None],
            part_loadouts: vec![
                Some(PlayerPartLoadout {
                    body: BodyPreset::RiftMantle,
                    arms: ArmPreset::RiftTalons,
                    legs: LegPreset::RiftBoots,
                    shoulders: ShoulderPreset::RiftCloak,
                    head: HeadPreset::RiftCowl,
                }),
                Some(PlayerPartLoadout {
                    body: BodyPreset::ChromaFrame,
                    arms: ArmPreset::ChromaBlades,
                    legs: LegPreset::ChromaStriders,
                    shoulders: ShoulderPreset::ChromaMantle,
                    head: HeadPreset::ChromaCrown,
                }),
                None,
                None,
            ],
            completed_chapters: vec![1],
            perk_ranks: vec![("heart_vitality".to_string(), 2)],
            robot_pets,
            settlement_economy: {
                let mut economy = SettlementEconomy {
                    stockpile: SettlementResources::new(1_000, 80, 60, 40, 30, 0, 10, 12, 2),
                    ..SettlementEconomy::default()
                };
                let mut parts = RobotPetCollection::default();
                for kind in RobotPartKind::ALL {
                    parts.add_part(kind, 10);
                }
                economy
                    .try_build(
                        "settlement_star_orchard",
                        SettlementBuildKind::PowerPlant,
                        &mut parts,
                    )
                    .unwrap();
                economy
            },
            tech_upgrades,
            part_loadout_body: BodyPreset::VoidArmor,
            part_loadout_arms: ArmPreset::ClawArms,
            part_loadout_legs: LegPreset::HeavyLegs,
            part_loadout_shoulders: ShoulderPreset::PlateEpaulettes,
            part_loadout_head: HeadPreset::VoidMask,
            raids: vec![RaidRecord::cloudrail_tutorial(RaidId(7))],
            hacking: {
                let mut hacking = HackingRegistry::default();
                hacking.learn_blueprint(
                    "blueprint_scallarian_drone_core",
                    crate::hacking::HackTargetClass::SmallDrone,
                    PlayerIndex(0),
                );
                hacking
            },
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
        assert!(loaded.studio_character_specs[0].is_some());
        assert_eq!(loaded.completed_chapters, vec![1]);
        assert_eq!(loaded.perk_ranks, vec![("heart_vitality".to_string(), 2)]);
        assert_eq!(loaded.robot_pets.part_count(RobotPartKind::StarDrive), 2);
        assert_eq!(loaded.robot_pets.pets[0].name, "Nova Kit");
        assert_eq!(
            loaded
                .settlement_economy
                .build_tier("settlement_star_orchard", SettlementBuildKind::PowerPlant),
            1
        );
        assert_eq!(
            loaded.tech_upgrades.rank(TechUpgradeId::RejuvenationMatrix),
            1
        );
        assert_eq!(loaded.tech_upgrades.rejuvenation_charge, 120.0);
        assert_eq!(loaded.part_loadout_body, BodyPreset::VoidArmor);
        assert_eq!(loaded.part_loadout_arms, ArmPreset::ClawArms);
        assert_eq!(loaded.part_loadout_legs, LegPreset::HeavyLegs);
        assert_eq!(
            loaded.part_loadout_shoulders,
            ShoulderPreset::PlateEpaulettes
        );
        assert_eq!(loaded.part_loadout_head, HeadPreset::VoidMask);
        assert_eq!(
            loaded.part_loadouts[0],
            Some(PlayerPartLoadout {
                body: BodyPreset::RiftMantle,
                arms: ArmPreset::RiftTalons,
                legs: LegPreset::RiftBoots,
                shoulders: ShoulderPreset::RiftCloak,
                head: HeadPreset::RiftCowl,
            })
        );
        assert_eq!(
            loaded.part_loadouts[1],
            Some(PlayerPartLoadout {
                body: BodyPreset::ChromaFrame,
                arms: ArmPreset::ChromaBlades,
                legs: LegPreset::ChromaStriders,
                shoulders: ShoulderPreset::ChromaMantle,
                head: HeadPreset::ChromaCrown,
            })
        );
        assert_eq!(loaded.raids.len(), 1);
        assert_eq!(loaded.raids[0].id, RaidId(7));
        assert_eq!(loaded.raids[0].phase, RaidPhase::Warning);
        assert!(loaded
            .hacking
            .has_blueprint("blueprint_scallarian_drone_core"));
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
            primary_weapon_slot: 999,
            special_weapon_slot: Some(9),
            armor_element: ElementType::Electric,
            inventory: Some({
                let mut inventory = Inventory::default();
                inventory.add_item("health_pack", 2, 10);
                inventory
            }),
            quick_item_id: Some("health_pack".to_string()),
            traversal_mode: 3,
            perk_tree: None,
            tech_upgrades: None,
            weapon_ranks: None,
            shop: None,
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

        let mut weapons = WeaponInventory::default();
        let mut specials = SpecialWeaponInventory::default();
        let mut armor = ArmorSet::default();
        let mut inventory = Inventory::default();
        let mut quick = QuickItemSlot::default();
        let mut traversal = TraversalModeState::default();
        saved.apply_loadout(
            &mut weapons,
            &mut specials,
            &mut armor,
            &mut inventory,
            &mut quick,
            &mut traversal,
        );
        assert_eq!(weapons.active_slot, weapons.slots.len() - 1);
        assert_eq!(specials.active_slot, None);
        assert_eq!(armor.active_element, ElementType::Electric);
        assert_eq!(inventory.count("health_pack"), 2);
        assert_eq!(quick.item_id.as_deref(), Some("health_pack"));
        assert_eq!(traversal.active, TraversalMode::Hoverboard);
    }

    #[test]
    fn player_progression_round_trip_is_owned_by_player_index() {
        let mut saved = player_save(2, 5, 200, 40, 80.0, 120.0);
        saved.perk_tree = Some(PerkTree {
            points_unspent: 3,
            ranks: vec![("star_focus".to_string(), 2)],
        });
        saved.tech_upgrades = Some(UpgradeLedger {
            ranks: vec![(TechUpgradeId::NovaMissileForge, 3)],
            rejuvenation_charge: 24.0,
        });
        saved.weapon_ranks = Some([1, 2, 3, 4, 5, 6]);

        let json = serde_json::to_string(&saved).unwrap();
        let decoded: PlayerSaveData = serde_json::from_str(&json).unwrap();
        let mut progression = PlayerProgression::default();
        decoded.apply_progression(&mut progression);

        assert_eq!(decoded.player_index, 2);
        assert_eq!(progression.perks.rank("star_focus"), 2);
        assert_eq!(
            progression.upgrades.rank(TechUpgradeId::NovaMissileForge),
            3
        );
        assert_eq!(progression.weapon_ranks.ranks, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn shop_ownership_round_trips_per_player_and_defaults_for_legacy_saves() {
        let mut saved = player_save(1, 3, 150, 40, 80.0, 120.0);
        saved.shop = Some(ShopOwnership {
            owned: vec!["scout_visor".to_string(), "nova_plating".to_string()],
            equipped_armor: Some("nova_plating".to_string()),
            ..Default::default()
        });

        let json = serde_json::to_string(&saved).unwrap();
        let decoded: PlayerSaveData = serde_json::from_str(&json).unwrap();
        let mut progression = PlayerProgression::default();
        decoded.apply_progression(&mut progression);
        assert!(progression.shop.owns("scout_visor"));
        assert_eq!(
            progression.shop.equipped_armor.as_deref(),
            Some("nova_plating")
        );

        // A legacy record written before the shop existed has no field at all;
        // it must load as "nothing owned" without touching progression that
        // already has purchases.
        let mut legacy_value: serde_json::Value = serde_json::from_str(&json).unwrap();
        legacy_value.as_object_mut().unwrap().remove("shop");
        let legacy: PlayerSaveData = serde_json::from_value(legacy_value).unwrap();
        assert!(legacy.shop.is_none());
        let mut kept = progression.clone();
        legacy.apply_progression(&mut kept);
        assert!(kept.shop.owns("scout_visor"));
    }

    // ── Disk hardening tests ─────────────────────────────────────────────────
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Unique temp directory per test so save slots never collide across tests
    /// or with real player data.
    fn test_save_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "starfall_save_{label}_{}_{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("test save root should be creatable");
        root
    }

    fn save_with_generation(generation: u64) -> SaveData {
        SaveData {
            save_generation: generation,
            wave_number: 1 + generation as u32,
            ..SaveData::default()
        }
    }

    #[test]
    fn interrupted_partial_write_leaves_previous_slot_readable() {
        let root = test_save_root("partial_write");
        write_save_data(&root, 0, &save_with_generation(1)).unwrap();

        // Simulate a crash mid-write: a partial temp file exists but the
        // rename never happened.
        let mut temp = save_slot_path_in(&root, 0).into_os_string();
        temp.push(".tmp");
        fs::write(PathBuf::from(&temp), b"{\"save_version\": 4, \"wav").unwrap();

        let (slot, data) = load_newest_save_in(&root).expect("previous save should survive");
        assert_eq!(slot, 0);
        assert_eq!(data.save_generation, 1);
        assert_eq!(data.wave_number, 2);

        // A subsequent successful write still lands atomically over the slot.
        write_save_data(&root, 0, &save_with_generation(2)).unwrap();
        let (_, data) = load_newest_save_in(&root).expect("rewritten save should load");
        assert_eq!(data.save_generation, 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_newest_slot_falls_back_to_next_highest_generation() {
        let root = test_save_root("corrupt_fallback");
        write_save_data(&root, 0, &save_with_generation(1)).unwrap();
        write_save_data(&root, 1, &save_with_generation(2)).unwrap();
        write_save_data(&root, 2, &save_with_generation(3)).unwrap();

        // Corrupt the newest slot (C); recovery should pick B (generation 2).
        fs::write(save_slot_path_in(&root, 2), b"{ not valid json").unwrap();

        let (slot, data) = load_newest_save_in(&root).expect("valid older slot should recover");
        assert_eq!(slot, 1);
        assert_eq!(data.save_generation, 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn generation_ordering_wins_over_slot_order() {
        let root = test_save_root("generation_order");
        // Slot A holds an old record; slot C holds the newest. The legacy
        // loader would have returned stale slot A here.
        write_save_data(&root, 0, &save_with_generation(1)).unwrap();
        write_save_data(&root, 2, &save_with_generation(5)).unwrap();

        let (slot, data) = load_newest_save_in(&root).expect("save should load");
        assert_eq!(slot, 2);
        assert_eq!(data.save_generation, 5);
        // Rotation resumes after the newest slot, wrapping C -> A.
        assert_eq!(next_save_slot(slot), 0);
        // The next write is stamped one past the highest generation on disk.
        assert_eq!(next_generation_in(&root), 6);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn legacy_v3_save_still_loads_and_migrates_to_v4() {
        let root = test_save_root("legacy_v3");
        // Build a genuine v3 record: version 3 and no save_generation field.
        let mut value = serde_json::to_value(SaveData {
            wave_number: 9,
            completed_chapters: vec![1, 2],
            ..SaveData::default()
        })
        .unwrap();
        value["save_version"] = serde_json::json!(3);
        value
            .as_object_mut()
            .unwrap()
            .remove("save_generation")
            .expect("current schema should carry save_generation");
        fs::write(
            save_slot_path_in(&root, 0),
            serde_json::to_string_pretty(&value).unwrap(),
        )
        .unwrap();

        let (slot, data) = load_newest_save_in(&root).expect("v3 save should migrate");
        assert_eq!(slot, 0);
        assert_eq!(data.save_version, SAVE_VERSION);
        assert_eq!(data.save_generation, 0);
        assert_eq!(data.wave_number, 9);
        assert_eq!(data.completed_chapters, vec![1, 2]);
        // The first post-migration write supersedes the legacy record.
        assert_eq!(next_generation_in(&root), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unsupported_future_version_is_rejected_not_misread() {
        let root = test_save_root("future_version");
        let future = SaveData {
            save_version: 99,
            save_generation: 40,
            ..SaveData::default()
        };
        write_save_data(&root, 0, &future).unwrap();

        // Alone, the future record is rejected rather than misread or panicked on.
        assert!(load_newest_save_in(&root).is_none());

        // With a supported record present, that record still recovers even
        // though the future one carries a higher generation.
        write_save_data(&root, 1, &save_with_generation(2)).unwrap();
        let (slot, data) = load_newest_save_in(&root).expect("supported slot should recover");
        assert_eq!(slot, 1);
        assert_eq!(data.save_generation, 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn settings_write_is_atomic_over_existing_file() {
        let root = test_save_root("settings_atomic");
        let path = root.join(SETTINGS_FILE);
        let json = serde_json::to_string_pretty(&GameSettings::default()).unwrap();
        write_atomic(&path, &json).unwrap();
        // Leftover partial temp from an interrupted write must not shadow the
        // real file.
        let mut temp = path.clone().into_os_string();
        temp.push(".tmp");
        fs::write(PathBuf::from(temp), b"{ partial").unwrap();
        let loaded: GameSettings =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let _ = serde_json::to_string(&loaded).unwrap();
        let _ = fs::remove_dir_all(&root);
    }
}
