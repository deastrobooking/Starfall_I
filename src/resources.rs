#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use std::collections::BTreeSet;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::chapters::{Biome, ChapterId};
use crate::character::blueprint::{BodyRecipe, CartoonAppearanceRecipe, CharacterBlueprint};
use crate::character::hero_roster::HERO_NAMES;
use crate::character::parts::{
    ArmPreset, BodyPreset, CharacterLoadout, HeadPreset, LegPreset, ShoulderPreset,
};
use crate::character::presets::normalize_color_preset_index;
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

/// True while an authoring screen owns printable keyboard input. Shared menu
/// navigation must yield Enter/Space/Escape until that edit is committed or
/// cancelled, preventing a name field from also activating buttons or leaving
/// the tool on the same key press.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct AuthoringTextInputCapture {
    pub active: bool,
}

/// Authoring screens set this while an unsaved document should intercept the
/// shared Escape/controller Back action instead of leaving immediately.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct AuthoringUnsavedChanges {
    pub active: bool,
}

// ── Game Settings ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ControllerGlyphStyle {
    #[default]
    Auto,
    Xbox,
    PlayStation,
    Nintendo,
}

impl ControllerGlyphStyle {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Xbox, Self::PlayStation, Self::Nintendo];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Xbox => "XBOX",
            Self::PlayStation => "PLAYSTATION",
            Self::Nintendo => "NINTENDO",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|style| *style == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
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
    /// Player-configurable face layout and keyboard actions. Rides the
    /// existing settings save/load rather than owning a second file.
    #[serde(default)]
    pub bindings: crate::engine::bindings::ControlBindings,
    /// Global multiplier for fixed-size UI values.
    #[serde(default = "default_one_f32")]
    pub ui_scale: f32,
    /// Fraction of the shortest window edge reserved around critical HUD UI.
    #[serde(default = "default_safe_area_fraction")]
    pub safe_area_fraction: f32,
    /// Use stronger text, panel, and focus contrast in the game UI.
    #[serde(default)]
    pub high_contrast_ui: bool,
    /// Suppress non-essential UI movement such as floating damage text.
    #[serde(default)]
    pub reduced_ui_motion: bool,
    /// Show written dialogue while voiced conversations play.
    #[serde(default = "default_true")]
    pub subtitles_enabled: bool,
    /// Controller button-label family. Auto follows the active USB vendor.
    #[serde(default)]
    pub controller_glyph_style: ControllerGlyphStyle,
}

fn default_one_f32() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

fn default_safe_area_fraction() -> f32 {
    0.025
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
            bindings: crate::engine::bindings::ControlBindings::default(),
            ui_scale: 1.0,
            safe_area_fraction: default_safe_area_fraction(),
            high_contrast_ui: false,
            reduced_ui_motion: false,
            subtitles_enabled: true,
            controller_glyph_style: ControllerGlyphStyle::Auto,
        }
    }
}

impl GameSettings {
    /// Repair settings loaded from hand-edited, legacy, or malformed JSON.
    /// Non-finite values use shipped defaults; finite values are clamped to
    /// the same ranges exposed by the settings UI.
    pub fn sanitize(&mut self) {
        let defaults = Self::default();
        self.mouse_sensitivity =
            finite_or(self.mouse_sensitivity, defaults.mouse_sensitivity).clamp(0.0001, 0.01);
        self.master_volume = finite_or(self.master_volume, defaults.master_volume).clamp(0.0, 1.0);
        self.difficulty_scale =
            finite_or(self.difficulty_scale, defaults.difficulty_scale).clamp(0.5, 2.0);
        self.music_volume = finite_or(self.music_volume, defaults.music_volume).clamp(0.0, 1.0);
        self.sfx_volume = finite_or(self.sfx_volume, defaults.sfx_volume).clamp(0.0, 1.0);
        self.ui_scale = finite_or(self.ui_scale, defaults.ui_scale).clamp(0.8, 1.4);
        self.safe_area_fraction =
            finite_or(self.safe_area_fraction, defaults.safe_area_fraction).clamp(0.0, 0.08);
    }

    pub fn validated(mut self) -> Self {
        self.sanitize();
        self
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
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
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayerScoreEntry {
    pub kills: u32,
    pub total_damage_dealt: f32,
    pub chests_opened: u32,
    pub waves_survived: u32,
}

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct PlayerScore {
    players: [PlayerScoreEntry; 4],
}

impl PlayerScore {
    pub fn player(&self, player_index: u8) -> Option<&PlayerScoreEntry> {
        self.players.get(usize::from(player_index))
    }

    pub fn players(&self) -> &[PlayerScoreEntry; 4] {
        &self.players
    }

    pub fn party_total(&self) -> PlayerScoreEntry {
        self.players
            .iter()
            .fold(PlayerScoreEntry::default(), |mut total, player| {
                total.kills = total.kills.saturating_add(player.kills);
                total.total_damage_dealt = finite_score(
                    total.total_damage_dealt + finite_score(player.total_damage_dealt),
                );
                total.chests_opened = total.chests_opened.saturating_add(player.chests_opened);
                total.waves_survived = total.waves_survived.saturating_add(player.waves_survived);
                total
            })
    }

    pub fn record_kill(&mut self, player_index: u8) -> bool {
        self.mutate(player_index, |score| {
            score.kills = score.kills.saturating_add(1);
        })
    }

    pub fn record_damage(&mut self, player_index: u8, damage: f32) -> bool {
        if !damage.is_finite() || damage <= 0.0 {
            return false;
        }
        self.mutate(player_index, |score| {
            score.total_damage_dealt = finite_score(score.total_damage_dealt + damage);
        })
    }

    pub fn record_chest(&mut self, player_index: u8) -> bool {
        self.mutate(player_index, |score| {
            score.chests_opened = score.chests_opened.saturating_add(1);
        })
    }

    pub fn record_wave(&mut self, player_index: u8) -> bool {
        self.mutate(player_index, |score| {
            score.waves_survived = score.waves_survived.saturating_add(1);
        })
    }

    fn mutate(&mut self, player_index: u8, update: impl FnOnce(&mut PlayerScoreEntry)) -> bool {
        let Some(score) = self.players.get_mut(usize::from(player_index)) else {
            return false;
        };
        update(score);
        true
    }
}

fn finite_score(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        f32::MAX
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

// ── Play Experience ─────────────────────────────────────────────────────────
/// Front-door gameplay format selected on the title screen. Campaign remains
/// the default; the bounded platformer is a deliberately separate ruleset so
/// its shared camera and catch-up rules cannot leak into the open world.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PlayExperience {
    #[default]
    Campaign,
    SharedPlatformer,
}

/// Optional non-chapter anchor selected from the world map. Chapter startup
/// consumes this once world anchors exist, then clears it.
#[derive(Resource, Debug, Default, Clone)]
pub struct FastTravelDestination {
    pub anchor_id: Option<String>,
    pub label: Option<String>,
    pub enter_dungeon: bool,
}

impl FastTravelDestination {
    pub fn cave(&mut self, anchor_id: impl Into<String>, label: impl Into<String>) {
        self.anchor_id = Some(anchor_id.into());
        self.label = Some(label.into());
        self.enter_dungeon = true;
    }

    pub fn world_anchor(&mut self, anchor_id: impl Into<String>, label: impl Into<String>) {
        self.anchor_id = Some(anchor_id.into());
        self.label = Some(label.into());
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

impl LocalPlayerConfig {
    pub const MIN_PLAYERS: u8 = 1;
    pub const MAX_PLAYERS: u8 = 4;

    pub fn new(active: u8) -> Self {
        Self {
            active: active.clamp(Self::MIN_PLAYERS, Self::MAX_PLAYERS),
        }
    }

    pub fn set_active(&mut self, active: usize) {
        self.active = u8::try_from(active)
            .unwrap_or(Self::MAX_PLAYERS)
            .clamp(Self::MIN_PLAYERS, Self::MAX_PLAYERS);
    }

    pub fn active_count(&self) -> usize {
        usize::from(self.active.clamp(Self::MIN_PLAYERS, Self::MAX_PLAYERS))
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
/// Shared-screen dungeon mode. Soft mode only flips camera/combat helpers on
/// the open world; `arcade_rules` marks a hard stage boundary (teleport + kit
/// lock) used by the Turtle Yard prototype and future authored arcade stages.
#[derive(Resource, Debug, Clone)]
pub struct DungeonCrawlState {
    pub active: bool,
    pub gate_id: Option<&'static str>,
    /// False on the activation frame so the gate-opening press cannot also
    /// trigger a nearby return portal.
    pub exit_armed: bool,
    /// Isolated arcade stage: party is teleported and open-world traversal
    /// toys (jetpack/grapple/hoverboard) are suppressed.
    pub arcade_rules: bool,
    /// Side-progressing shared-screen course. It keeps the arcade traversal
    /// lock while selecting a P1-led platformer camera and bubble recovery.
    pub platformer_rules: bool,
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
            arcade_rules: false,
            platformer_rules: false,
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
        self.activate_with_rules(gate_id, chapter, label, focus, anchor, radius, false);
    }

    pub fn activate_arcade(
        &mut self,
        gate_id: &'static str,
        chapter: ChapterId,
        label: impl Into<String>,
        focus: Vec3,
        anchor: Vec3,
        radius: f32,
    ) {
        self.activate_with_rules(gate_id, chapter, label, focus, anchor, radius, true);
    }

    pub fn activate_platformer(
        &mut self,
        gate_id: &'static str,
        chapter: ChapterId,
        label: impl Into<String>,
        focus: Vec3,
        anchor: Vec3,
        radius: f32,
    ) {
        self.activate_with_rules(gate_id, chapter, label, focus, anchor, radius, true);
        self.platformer_rules = true;
    }

    fn activate_with_rules(
        &mut self,
        gate_id: &'static str,
        chapter: ChapterId,
        label: impl Into<String>,
        focus: Vec3,
        anchor: Vec3,
        radius: f32,
        arcade_rules: bool,
    ) {
        self.active = true;
        self.gate_id = Some(gate_id);
        self.exit_armed = false;
        self.arcade_rules = arcade_rules;
        self.platformer_rules = false;
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
        self.arcade_rules = false;
        self.platformer_rules = false;
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
        let snapshot = self.clone().validated();
        data.player_index = snapshot.player_index;
        if snapshot.schema_version >= CHARACTER_DESIGN_SNAPSHOT_VERSION {
            data.face = snapshot.face;
        }
        data.base_model = snapshot.base_model;
        data.skin_idx = snapshot.skin_idx;
        data.outfit_idx = snapshot.outfit_idx;
        data.accent_idx = snapshot.accent_idx;
        data.hair_idx = snapshot.hair_idx;
        data.eye_idx = snapshot.eye_idx;
        data.body_preset = snapshot.part_loadout.body;
        data.arm_preset = snapshot.part_loadout.arms;
        data.leg_preset = snapshot.part_loadout.legs;
        data.shoulder_preset = snapshot.part_loadout.shoulders;
        data.head_preset = snapshot.part_loadout.head;
        data.body = snapshot.body;
        data.has_hood = snapshot.appearance.has_hood;
        data.has_cape = snapshot.appearance.has_cape;
        data.has_gloves = snapshot.appearance.has_gloves;
        data.has_boots = snapshot.appearance.has_boots;
        data.has_shoulder_pads = snapshot.appearance.has_shoulder_pads;
        data.has_visor = snapshot.appearance.has_visor;
        data.dirty = true;
    }

    /// Normalize imported/editor snapshots before they reach palette lookup or
    /// local-player selection. Unknown future schemas retain their durable
    /// fields but cannot create out-of-bounds runtime indices.
    pub fn validated(mut self) -> Self {
        self.player_index = self
            .player_index
            .min(LocalPlayerConfig::MAX_PLAYERS as usize - 1);
        self.face = self.face.sanitized();
        self.skin_idx = normalize_color_preset_index(self.skin_idx);
        self.outfit_idx = normalize_color_preset_index(self.outfit_idx);
        self.accent_idx = normalize_color_preset_index(self.accent_idx);
        self.hair_idx = normalize_color_preset_index(self.hair_idx);
        self.eye_idx = normalize_color_preset_index(self.eye_idx);
        self.body = self.body.validated();
        self
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveRecordApplyReport {
    pub applied: usize,
    pub unknown_ids: Vec<u16>,
    pub duplicate_ids: Vec<u16>,
    pub invalid_records: Vec<String>,
}

impl SaveRecordApplyReport {
    pub fn is_clean(&self) -> bool {
        self.unknown_ids.is_empty()
            && self.duplicate_ids.is_empty()
            && self.invalid_records.is_empty()
    }
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

    pub fn apply_save_records_checked(
        &mut self,
        records: &[WorldSiteSaveRecord],
    ) -> SaveRecordApplyReport {
        let mut report = SaveRecordApplyReport::default();
        let mut seen = BTreeSet::new();
        for record in records {
            if !seen.insert(record.id) {
                report.duplicate_ids.push(record.id);
                continue;
            }
            let Some(site) = self.get_mut(WorldSiteId(record.id)) else {
                report.unknown_ids.push(record.id);
                continue;
            };
            let completed_state = matches!(
                record.state,
                WorldSiteState::Liberated | WorldSiteState::Building | WorldSiteState::Shielded
            );
            let held_state = matches!(
                record.state,
                WorldSiteState::EnemyHeld | WorldSiteState::OccupiedAgain
            );
            let count_invalid = record.enemies_defeated > site.enemy_count_to_liberate
                || (site.enemy_count_to_liberate > 0
                    && ((completed_state
                        && record.enemies_defeated < site.enemy_count_to_liberate)
                        || (held_state
                            && record.enemies_defeated >= site.enemy_count_to_liberate)));
            if count_invalid {
                report.invalid_records.push(format!(
                    "site {} state {:?} has {}/{} defeated",
                    record.id, record.state, record.enemies_defeated, site.enemy_count_to_liberate
                ));
                continue;
            }
            site.state = record.state;
            site.owner = record.owner;
            site.enemies_defeated = record.enemies_defeated;
            report.applied += 1;
        }
        report
    }

    pub fn apply_save_records(&mut self, records: &[WorldSiteSaveRecord]) {
        let _ = self.apply_save_records_checked(records);
    }

    pub fn validate_catalog(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut ids = BTreeSet::new();
        for site in &self.sites {
            if !ids.insert(site.id.0) {
                errors.push(format!("duplicate world site id {}", site.id.0));
            }
            if site.state == WorldSiteState::EnemyHeld && site.enemy_count_to_liberate == 0 {
                errors.push(format!("enemy-held site {} has no defenders", site.id.0));
            }
            if site.enemies_defeated > site.enemy_count_to_liberate {
                errors.push(format!("site {} exceeds its defender count", site.id.0));
            }
        }
        errors
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
    pub fn apply_save_records_checked(
        &mut self,
        records: &[WorldRouteSaveRecord],
    ) -> SaveRecordApplyReport {
        let mut report = SaveRecordApplyReport::default();
        let mut seen = BTreeSet::new();
        for record in records {
            if !seen.insert(record.id) {
                report.duplicate_ids.push(record.id);
                continue;
            }
            let Some(route) = self.routes.iter_mut().find(|route| route.id.0 == record.id) else {
                report.unknown_ids.push(record.id);
                continue;
            };
            route.state = record.state;
            report.applied += 1;
        }
        report
    }

    pub fn apply_save_records(&mut self, records: &[WorldRouteSaveRecord]) {
        let _ = self.apply_save_records_checked(records);
    }

    pub fn validate_catalog(&self, sites: &WorldSiteRegistry) -> Vec<String> {
        let mut errors = Vec::new();
        let mut ids = BTreeSet::new();
        for route in &self.routes {
            if !ids.insert(route.id.0) {
                errors.push(format!("duplicate world route id {}", route.id.0));
            }
            if route.from_site == route.to_site {
                errors.push(format!("route {} connects a site to itself", route.id.0));
            }
            for (role, site_id) in [
                ("from", route.from_site),
                ("to", route.to_site),
                ("required", route.required_site),
            ] {
                if sites.get(site_id).is_none() {
                    errors.push(format!(
                        "route {} has unknown {role} site {}",
                        route.id.0, site_id.0
                    ));
                }
            }
        }
        errors
    }
}

pub fn validate_world_registry_catalogs(
    sites: Res<WorldSiteRegistry>,
    routes: Res<WorldRouteRegistry>,
) {
    for error in sites
        .validate_catalog()
        .into_iter()
        .chain(routes.validate_catalog(&sites))
    {
        error!("Invalid built-in world registry: {error}");
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
    fn legacy_settings_receive_responsive_ui_defaults() {
        let settings: GameSettings = serde_json::from_str(
            r#"{
                "mouse_sensitivity": 0.0008,
                "master_volume": 1.0,
                "show_damage_numbers": true,
                "world_seed": 42195,
                "difficulty_scale": 1.0,
                "music_volume": 1.0,
                "sfx_volume": 1.0,
                "rumble_on_hit": true
            }"#,
        )
        .unwrap();
        assert_eq!(settings.ui_scale, 1.0);
        assert_eq!(settings.safe_area_fraction, 0.025);
        assert!(!settings.high_contrast_ui);
        assert!(!settings.reduced_ui_motion);
        assert!(settings.subtitles_enabled);
        assert_eq!(settings.controller_glyph_style, ControllerGlyphStyle::Auto);
    }

    #[test]
    fn malformed_settings_are_repaired_at_the_resource_boundary() {
        let mut settings = GameSettings {
            mouse_sensitivity: f32::NAN,
            master_volume: -4.0,
            difficulty_scale: f32::INFINITY,
            music_volume: 12.0,
            sfx_volume: -1.0,
            ui_scale: 99.0,
            safe_area_fraction: f32::NEG_INFINITY,
            ..GameSettings::default()
        };
        settings.sanitize();
        assert_eq!(
            settings.mouse_sensitivity,
            GameSettings::default().mouse_sensitivity
        );
        assert_eq!(settings.master_volume, 0.0);
        assert_eq!(settings.difficulty_scale, 1.0);
        assert_eq!(settings.music_volume, 1.0);
        assert_eq!(settings.sfx_volume, 0.0);
        assert_eq!(settings.ui_scale, 1.4);
        assert_eq!(settings.safe_area_fraction, 0.025);
    }

    #[test]
    fn local_player_config_never_exposes_an_invalid_party_size() {
        assert_eq!(LocalPlayerConfig::new(0).active_count(), 1);
        assert_eq!(LocalPlayerConfig::new(9).active_count(), 4);
        let mut config = LocalPlayerConfig::default();
        config.set_active(usize::MAX);
        assert_eq!(config.active, 4);
    }

    #[test]
    fn player_score_is_owner_scoped_with_computed_party_totals() {
        let mut score = PlayerScore::default();
        assert!(score.record_chest(2));
        assert!(score.record_kill(2));
        assert!(score.record_damage(2, 14.5));
        assert!(score.record_wave(2));
        assert!(score.record_chest(0));

        assert_eq!(score.player(0).unwrap().chests_opened, 1);
        assert_eq!(score.player(1), Some(&PlayerScoreEntry::default()));
        assert_eq!(score.player(2).unwrap().kills, 1);
        assert_eq!(score.player(2).unwrap().total_damage_dealt, 14.5);
        let total = score.party_total();
        assert_eq!(total.kills, 1);
        assert_eq!(total.chests_opened, 2);
        assert_eq!(total.waves_survived, 1);
        assert_eq!(total.total_damage_dealt, 14.5);
    }

    #[test]
    fn player_score_rejects_unknown_owners_and_invalid_damage() {
        let mut score = PlayerScore::default();
        assert!(!score.record_chest(4));
        assert!(!score.record_kill(u8::MAX));
        assert!(!score.record_damage(0, f32::NAN));
        assert!(!score.record_damage(0, -5.0));
        assert_eq!(score.party_total(), PlayerScoreEntry::default());
    }

    #[test]
    fn controller_glyph_styles_cycle_through_every_supported_family() {
        let mut style = ControllerGlyphStyle::Auto;
        for expected in [
            ControllerGlyphStyle::Xbox,
            ControllerGlyphStyle::PlayStation,
            ControllerGlyphStyle::Nintendo,
            ControllerGlyphStyle::Auto,
        ] {
            style = style.next();
            assert_eq!(style, expected);
        }
    }

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
        assert!(!dungeon.arcade_rules);
        assert_eq!(dungeon.gate_id, Some("test_gate"));
        assert_eq!(dungeon.chapter, Some(ChapterId(6)));
        assert_eq!(dungeon.label, "Collosar's Crown Gate");
        assert_eq!(dungeon.radius, 66.0);

        dungeon.clear();

        assert!(!dungeon.active);
        assert!(dungeon.gate_id.is_none());
        assert!(dungeon.chapter.is_none());
        assert!(dungeon.label.is_empty());

        dungeon.activate_arcade(
            "arcade_gate",
            ChapterId(1),
            "Turtle Yard Arcade",
            Vec3::ZERO,
            Vec3::ZERO,
            30.0,
        );
        assert!(dungeon.active);
        assert!(dungeon.arcade_rules);
        assert_eq!(dungeon.gate_id, Some("arcade_gate"));

        dungeon.clear();
        assert!(!dungeon.active);
        assert!(!dungeon.arcade_rules);
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
    fn malformed_character_snapshot_indices_are_normalized_before_application() {
        let mut snapshot =
            CharacterDesignSnapshot::from_design_data(&CharacterDesignData::default());
        snapshot.player_index = usize::MAX;
        snapshot.skin_idx = usize::MAX;
        snapshot.outfit_idx = usize::MAX;
        snapshot.accent_idx = usize::MAX;
        snapshot.hair_idx = usize::MAX;
        snapshot.eye_idx = usize::MAX;
        snapshot.body.height = f32::NAN;
        let validated = snapshot.validated();
        assert_eq!(validated.player_index, 3);
        for index in [
            validated.skin_idx,
            validated.outfit_idx,
            validated.accent_idx,
            validated.hair_idx,
            validated.eye_idx,
        ] {
            assert!(index < crate::character::presets::COLOR_PRESET_COUNT);
        }
        assert!(validated.body.height.is_finite());
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
    fn checked_world_record_hydration_skips_only_bad_records() {
        let mut sites = WorldSiteRegistry::default();
        let site_report = sites.apply_save_records_checked(&[
            WorldSiteSaveRecord {
                id: 1,
                state: WorldSiteState::Contested,
                owner: WorldSiteOwner::Scallarian,
                enemies_defeated: 2,
            },
            WorldSiteSaveRecord {
                id: 1,
                state: WorldSiteState::Liberated,
                owner: WorldSiteOwner::PlayerAlliance,
                enemies_defeated: 5,
            },
            WorldSiteSaveRecord {
                id: 65_000,
                state: WorldSiteState::Hidden,
                owner: WorldSiteOwner::Neutral,
                enemies_defeated: 0,
            },
            WorldSiteSaveRecord {
                id: 2,
                state: WorldSiteState::EnemyHeld,
                owner: WorldSiteOwner::Scallarian,
                enemies_defeated: u8::MAX,
            },
        ]);
        assert_eq!(site_report.applied, 1);
        assert_eq!(site_report.duplicate_ids, vec![1]);
        assert_eq!(site_report.unknown_ids, vec![65_000]);
        assert_eq!(site_report.invalid_records.len(), 1);
        assert_eq!(sites.get(WorldSiteId(1)).unwrap().enemies_defeated, 2);
        assert_eq!(sites.get(WorldSiteId(2)).unwrap().enemies_defeated, 0);

        let mut routes = WorldRouteRegistry {
            routes: initial_world_routes(),
        };
        let route_report = routes.apply_save_records_checked(&[
            WorldRouteSaveRecord {
                id: 1,
                state: WorldRouteState::Open,
            },
            WorldRouteSaveRecord {
                id: 1,
                state: WorldRouteState::Blocked,
            },
            WorldRouteSaveRecord {
                id: 60_000,
                state: WorldRouteState::Open,
            },
        ]);
        assert_eq!(route_report.applied, 1);
        assert_eq!(route_report.duplicate_ids, vec![1]);
        assert_eq!(route_report.unknown_ids, vec![60_000]);
        assert_eq!(
            routes.get(WorldRouteId(1)).unwrap().state,
            WorldRouteState::Open
        );
    }

    #[test]
    fn built_in_world_registry_ids_and_endpoints_validate() {
        let sites = WorldSiteRegistry::default();
        let routes = WorldRouteRegistry {
            routes: initial_world_routes(),
        };
        assert!(sites.validate_catalog().is_empty());
        assert!(routes.validate_catalog(&sites).is_empty());
    }

    #[test]
    fn cave_fast_travel_destination_is_one_shot_and_clearable() {
        let mut destination = FastTravelDestination::default();
        destination.cave("secret_cave_ch03", "Sister Starwell Cave");
        assert_eq!(destination.anchor_id.as_deref(), Some("secret_cave_ch03"));
        assert_eq!(destination.label.as_deref(), Some("Sister Starwell Cave"));
        assert!(destination.enter_dungeon);
        destination.world_anchor("race_region_north_gate", "Grand Raceway");
        assert_eq!(
            destination.anchor_id.as_deref(),
            Some("race_region_north_gate")
        );
        assert_eq!(destination.label.as_deref(), Some("Grand Raceway"));
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
